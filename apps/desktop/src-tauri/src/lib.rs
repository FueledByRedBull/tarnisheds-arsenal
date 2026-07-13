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
        let data = er_optimizer_core::load_game_data(&data_dir).map_err(errors::AppError::from)?;
        let manifest = load_data_manifest(&data_dir)?;
        Ok((data, manifest))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        let data = er_optimizer_core::load_embedded_game_data().map_err(errors::AppError::from)?;
        let manifest = load_embedded_data_manifest()?;
        Ok((data, manifest))
    }
}

#[cfg(any(not(debug_assertions), test))]
fn load_embedded_data_manifest() -> Result<dto::DataManifestDto, errors::AppError> {
    serde_json::from_str(include_str!("../../../../data/phase1/manifest.json")).map_err(|err| {
        errors::AppError::new(format!("failed to load embedded data manifest: {err}"))
    })
}

#[cfg(debug_assertions)]
fn load_data_manifest(
    data_dir: &std::path::Path,
) -> Result<dto::DataManifestDto, errors::AppError> {
    let content = std::fs::read_to_string(data_dir.join("manifest.json"))
        .unwrap_or_else(|_| include_str!("../../../../data/phase1/manifest.json").to_string());
    serde_json::from_str(&content)
        .map_err(|err| errors::AppError::new(format!("failed to load data manifest: {err}")))
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
mod release_data_tests {
    use super::load_embedded_data_manifest;

    #[test]
    fn standalone_release_snapshot_and_manifest_are_complete() {
        let data = er_optimizer_core::load_embedded_game_data().expect("embedded data loads");
        let manifest = load_embedded_data_manifest().expect("embedded manifest loads");
        assert!(data.weapons.len() > 3000);
        assert!(data.aows.len() > 100);
        assert!(!manifest.id.is_empty());
        assert!(!manifest.app_version.is_empty());
    }
}
