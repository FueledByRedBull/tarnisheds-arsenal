use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use er_optimizer_core::GameData;
use tauri::Manager;

mod commands;
mod dto;
mod errors;

pub type CancelFlag = Arc<AtomicBool>;

pub struct AsyncJobHandle<T> {
    pub cancel: CancelFlag,
    pub status: Arc<Mutex<T>>,
}

pub struct JobRegistry<T> {
    kind: &'static str,
    handles: Mutex<std::collections::HashMap<String, AsyncJobHandle<T>>>,
}

impl<T: Clone> JobRegistry<T> {
    pub fn new(kind: &'static str) -> Self {
        Self {
            kind,
            handles: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn insert_if_idle(
        &self,
        job_id: String,
        handle: AsyncJobHandle<T>,
        is_finished: impl Fn(&T) -> bool,
    ) -> Result<(), errors::AppError> {
        let mut guard = self.handles.lock().map_err(|_| self.lock_error())?;
        guard.retain(|_, handle| {
            handle
                .status
                .lock()
                .map(|status| !is_finished(&status))
                .unwrap_or(false)
        });
        if !guard.is_empty() {
            return Err(errors::AppError::new(format!(
                "{} job is already running. Stop or wait for it before starting another.",
                self.kind
            )));
        }
        guard.insert(job_id, handle);
        Ok(())
    }

    pub fn cancel(&self, job_id: &str) -> Result<bool, errors::AppError> {
        let guard = self.handles.lock().map_err(|_| self.lock_error())?;
        let Some(handle) = guard.get(job_id) else {
            return Ok(false);
        };
        handle.cancel.store(true, Ordering::Relaxed);
        Ok(true)
    }

    pub fn status(
        &self,
        job_id: &str,
        is_finished: impl Fn(&T) -> bool,
    ) -> Result<Option<T>, errors::AppError> {
        let mut guard = self.handles.lock().map_err(|_| self.lock_error())?;
        let Some(handle) = guard.get(job_id) else {
            return Ok(None);
        };
        let status = handle
            .status
            .lock()
            .map_err(|_| {
                errors::AppError::new(format!(
                    "{} job status is unavailable. Retry once, then restart the app if it persists.",
                    self.kind
                ))
            })?
            .clone();
        if is_finished(&status) {
            guard.remove(job_id);
        }
        Ok(Some(status))
    }

    fn lock_error(&self) -> errors::AppError {
        errors::AppError::new(format!(
            "{} job registry is unavailable. Retry once, then restart the app if it persists.",
            self.kind
        ))
    }
}

pub struct AppState {
    pub data: Arc<GameData>,
    pub catalog_index: Arc<commands::data::CatalogIndex>,
    pub data_manifest: dto::DataManifestDto,
    pub search_jobs: Arc<JobRegistry<dto::SearchJobStatusDto>>,
    pub path_jobs: Arc<JobRegistry<dto::PathJobStatusDto>>,
    pub affinity_jobs: Arc<JobRegistry<dto::AffinityWatchJobStatusDto>>,
    pub next_job: AtomicU64,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let (data, data_manifest) = load_desktop_data(app)?;
            let catalog_index = commands::data::CatalogIndex::build(&data);
            app.manage(AppState {
                data: Arc::new(data),
                catalog_index: Arc::new(catalog_index),
                data_manifest,
                search_jobs: Arc::new(JobRegistry::new("search")),
                path_jobs: Arc::new(JobRegistry::new("path")),
                affinity_jobs: Arc::new(JobRegistry::new("affinity watch")),
                next_job: AtomicU64::new(1),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::data::get_catalog,
            commands::data::get_data_manifest,
            commands::data::get_weapon_profile,
            commands::data::affinities_for_weapon,
            commands::data::compatible_aow_names,
            commands::data::compatible_aow_names_for_affinity,
            commands::data::weapon_names_for_type,
            commands::data::weapon_scaling_for_upgrade,
            commands::optimize::estimate_search_space,
            commands::optimize::run_search,
            commands::optimize::solve_build,
            commands::optimize::build_upgrade_series,
            commands::optimize::start_search,
            commands::optimize::cancel_search,
            commands::optimize::get_search_status,
            commands::paths::build_path_preview,
            commands::paths::start_path_preview,
            commands::paths::cancel_path_preview,
            commands::paths::get_path_preview_status,
            commands::affinity_watch::build_affinity_watch,
            commands::affinity_watch::start_affinity_watch,
            commands::affinity_watch::cancel_affinity_watch,
            commands::affinity_watch::get_affinity_watch_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri app");
}

fn load_desktop_data(
    app: &tauri::App,
) -> Result<(GameData, dto::DataManifestDto), errors::AppError> {
    #[cfg(debug_assertions)]
    {
        let data_dir = resolve_data_dir(app)?;
        let (data, manifest) = er_optimizer_core::load_game_data_with_manifest(&data_dir)
            .map_err(errors::AppError::from)?;
        Ok((data, manifest.into()))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        let (data, manifest) = er_optimizer_core::load_embedded_game_data_with_manifest()
            .map_err(errors::AppError::from)?;
        Ok((data, manifest.into()))
    }
}

#[cfg(debug_assertions)]
fn resolve_data_dir(app: &tauri::App) -> Result<std::path::PathBuf, errors::AppError> {
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let portable_data_dir = exe_dir.join("data").join("phase1");
        if portable_data_dir.exists() {
            return Ok(portable_data_dir);
        }
    }

    let dev_data_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data/phase1")
        .canonicalize()
        .ok();
    if let Some(path) = dev_data_dir.filter(|path| path.exists()) {
        return Ok(path);
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| errors::AppError::new(format!("failed to resolve resource dir: {err}")))?;
    let bundled = resource_dir.join("data").join("phase1");
    Ok(bundled)
}

#[cfg(test)]
pub(crate) fn test_app_state() -> AppState {
    let (data, manifest) = er_optimizer_core::load_embedded_game_data_with_manifest()
        .expect("embedded test snapshot loads");
    let catalog_index = commands::data::CatalogIndex::build(&data);
    AppState {
        data: Arc::new(data),
        catalog_index: Arc::new(catalog_index),
        data_manifest: manifest.into(),
        search_jobs: Arc::new(JobRegistry::new("search")),
        path_jobs: Arc::new(JobRegistry::new("path")),
        affinity_jobs: Arc::new(JobRegistry::new("affinity watch")),
        next_job: AtomicU64::new(1),
    }
}

#[cfg(test)]
pub(crate) fn test_optimize_request() -> dto::OptimizeRequestDto {
    dto::OptimizeRequestDto {
        class_name: "Samurai".to_string(),
        character_level: 9,
        vig: 12,
        mnd: 11,
        end: 13,
        str_stat: 12,
        dex: 15,
        int_stat: 9,
        fai: 8,
        arc: 8,
        min_str: 0,
        min_dex: 0,
        min_int: 0,
        min_fai: 0,
        min_arc: 0,
        lock_str: None,
        lock_dex: None,
        lock_int: None,
        lock_fai: None,
        lock_arc: None,
        standard_max_upgrade: Some(0),
        somber_max_upgrade: Some(0),
        exact_upgrade: Some(true),
        max_upgrade: None,
        fixed_upgrade: None,
        two_handing: false,
        dlc_scaling: false,
        scadutree_level: 0,
        weapon_name: Some("Uchigatana".to_string()),
        affinity: Some("Keen".to_string()),
        aow_name: None,
        weapon_type_key: None,
        somber_filter: "all".to_string(),
        objective: "max_ar".to_string(),
        top_k: 1,
    }
}

#[cfg(test)]
mod release_data_tests {
    #[test]
    fn standalone_release_snapshot_and_manifest_are_complete() {
        let (data, manifest) = er_optimizer_core::load_embedded_game_data_with_manifest()
            .expect("embedded data and manifest load");
        assert!(data.weapons.len() > 3000);
        assert!(data.aows.len() > 100);
        assert!(!manifest.id.is_empty());
        assert!(!manifest.app_version.is_empty());
        assert_eq!(data.dataset_version, manifest.dataset_version);
    }
}
