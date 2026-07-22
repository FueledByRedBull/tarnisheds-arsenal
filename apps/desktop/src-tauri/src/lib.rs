use std::collections::HashMap;
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
    pub profiles: HashMap<String, Arc<ProfileData>>,
    pub search_jobs: Arc<JobRegistry<dto::SearchJobStatusDto>>,
    pub path_jobs: Arc<JobRegistry<dto::PathJobStatusDto>>,
    pub affinity_jobs: Arc<JobRegistry<dto::AffinityWatchJobStatusDto>>,
    pub next_job: AtomicU64,
}

pub struct ProfileData {
    pub data: Arc<GameData>,
    pub catalog_index: Arc<commands::data::CatalogIndex>,
    pub data_manifest: dto::DataManifestDto,
}

impl AppState {
    pub fn profile(&self, profile_id: &str) -> Result<&Arc<ProfileData>, errors::AppError> {
        self.profiles.get(profile_id).ok_or_else(|| {
            errors::AppError::new(format!(
                "Unknown game profile {profile_id:?}. Reload the catalog and choose an available profile."
            ))
        })
    }
}

pub fn run() {
    let mut context = tauri::generate_context!();
    #[cfg(target_os = "windows")]
    if let Some(config) = packaged_smoke_config(std::env::args())
        .unwrap_or_else(|message| panic!("invalid packaged smoke configuration: {message}"))
    {
        let browser_args = packaged_smoke_browser_args(config.port);
        for window in &mut context.config_mut().app.windows {
            window.additional_browser_args = Some(browser_args.clone());
            window.data_directory = Some(config.profile_directory.clone().into());
        }
    }

    tauri::Builder::default()
        .setup(|app| {
            let profiles = load_desktop_profiles(app)?;
            app.manage(AppState {
                profiles,
                search_jobs: Arc::new(JobRegistry::new("search")),
                path_jobs: Arc::new(JobRegistry::new("path")),
                affinity_jobs: Arc::new(JobRegistry::new("affinity watch")),
                next_job: AtomicU64::new(1),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::data::get_catalog,
            commands::data::get_profiles,
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
        .run(context)
        .expect("error while running Tauri app");
}

#[cfg(target_os = "windows")]
#[derive(Debug, PartialEq, Eq)]
struct PackagedSmokeConfig {
    port: u16,
    profile_directory: String,
}

#[cfg(target_os = "windows")]
fn packaged_smoke_config(
    args: impl IntoIterator<Item = String>,
) -> Result<Option<PackagedSmokeConfig>, String> {
    let mut port = None;
    let mut profile_directory = None;
    for argument in args {
        if let Some(raw_port) = argument.strip_prefix("--packaged-smoke-port=") {
            if port.is_some() {
                return Err("--packaged-smoke-port may only be provided once".to_string());
            }
            let parsed = raw_port.parse::<u16>().map_err(|_| {
                "--packaged-smoke-port must be an integer from 1 to 65535".to_string()
            })?;
            if parsed == 0 {
                return Err("--packaged-smoke-port must be an integer from 1 to 65535".to_string());
            }
            port = Some(parsed);
        } else if let Some(raw_profile) = argument.strip_prefix("--packaged-smoke-profile=") {
            if profile_directory.is_some() {
                return Err("--packaged-smoke-profile may only be provided once".to_string());
            }
            if !raw_profile.starts_with("tarnisheds-arsenal-smoke-")
                || !raw_profile
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            {
                return Err(
                    "--packaged-smoke-profile must be a safe smoke-only directory name".to_string(),
                );
            }
            profile_directory = Some(raw_profile.to_string());
        }
    }
    match (port, profile_directory) {
        (None, None) => Ok(None),
        (Some(port), Some(profile_directory)) => Ok(Some(PackagedSmokeConfig {
            port,
            profile_directory,
        })),
        (Some(_), None) => {
            Err("--packaged-smoke-profile is required with --packaged-smoke-port".to_string())
        }
        (None, Some(_)) => {
            Err("--packaged-smoke-port is required with --packaged-smoke-profile".to_string())
        }
    }
}

#[cfg(target_os = "windows")]
fn packaged_smoke_browser_args(port: u16) -> String {
    format!(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
         --remote-debugging-port={port} --remote-debugging-address=127.0.0.1 \
         --disable-gpu --no-first-run"
    )
}

fn load_desktop_profiles(
    app: &tauri::App,
) -> Result<HashMap<String, Arc<ProfileData>>, errors::AppError> {
    let mut profiles = HashMap::new();
    for profile_id in [
        er_optimizer_core::VANILLA_PROFILE_ID,
        er_optimizer_core::CONVERGENCE_PROFILE_ID,
    ] {
        let (data, manifest) = load_desktop_profile(app, profile_id)?;
        let catalog_index = commands::data::CatalogIndex::build(&data);
        profiles.insert(
            profile_id.to_string(),
            Arc::new(ProfileData {
                data: Arc::new(data),
                catalog_index: Arc::new(catalog_index),
                data_manifest: manifest,
            }),
        );
    }
    Ok(profiles)
}

fn load_desktop_profile(
    app: &tauri::App,
    profile_id: &str,
) -> Result<(GameData, dto::DataManifestDto), errors::AppError> {
    #[cfg(debug_assertions)]
    {
        let data_dir = resolve_data_dir(app, profile_id)?;
        let (data, manifest) = er_optimizer_core::load_game_data_with_manifest(&data_dir)
            .map_err(errors::AppError::from)?;
        Ok((data, manifest.into()))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        let (data, manifest) =
            er_optimizer_core::load_embedded_game_profile_with_manifest(profile_id)
                .map_err(errors::AppError::from)?;
        Ok((data, manifest.into()))
    }
}

#[cfg(debug_assertions)]
fn resolve_data_dir(
    app: &tauri::App,
    profile_id: &str,
) -> Result<std::path::PathBuf, errors::AppError> {
    let relative = if profile_id == er_optimizer_core::VANILLA_PROFILE_ID {
        std::path::PathBuf::from("phase1")
    } else {
        std::path::PathBuf::from("profiles").join(profile_id)
    };
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let portable_data_dir = exe_dir.join("data").join(&relative);
        if portable_data_dir.exists() {
            return Ok(portable_data_dir);
        }
    }

    let dev_data_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data")
        .join(&relative)
        .canonicalize()
        .ok();
    if let Some(path) = dev_data_dir.filter(|path| path.exists()) {
        return Ok(path);
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| errors::AppError::new(format!("failed to resolve resource dir: {err}")))?;
    let bundled = resource_dir.join("data").join(relative);
    Ok(bundled)
}

#[cfg(test)]
pub(crate) fn test_app_state() -> AppState {
    let mut profiles = HashMap::new();
    for profile_id in [
        er_optimizer_core::VANILLA_PROFILE_ID,
        er_optimizer_core::CONVERGENCE_PROFILE_ID,
    ] {
        let (data, manifest) =
            er_optimizer_core::load_embedded_game_profile_with_manifest(profile_id)
                .expect("embedded test snapshot loads");
        let catalog_index = commands::data::CatalogIndex::build(&data);
        profiles.insert(
            profile_id.to_string(),
            Arc::new(ProfileData {
                data: Arc::new(data),
                catalog_index: Arc::new(catalog_index),
                data_manifest: manifest.into(),
            }),
        );
    }
    AppState {
        profiles,
        search_jobs: Arc::new(JobRegistry::new("search")),
        path_jobs: Arc::new(JobRegistry::new("path")),
        affinity_jobs: Arc::new(JobRegistry::new("affinity watch")),
        next_job: AtomicU64::new(1),
    }
}

#[cfg(test)]
pub(crate) fn test_optimize_request() -> dto::OptimizeRequestDto {
    dto::OptimizeRequestDto {
        profile_id: er_optimizer_core::VANILLA_PROFILE_ID.to_string(),
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
    #[cfg(target_os = "windows")]
    #[test]
    fn packaged_smoke_config_is_explicit_isolated_and_validated() {
        assert_eq!(
            super::packaged_smoke_config(["app.exe".to_string()]).unwrap(),
            None
        );
        assert_eq!(
            super::packaged_smoke_config([
                "app.exe".to_string(),
                "--packaged-smoke-port=43117".to_string(),
                "--packaged-smoke-profile=tarnisheds-arsenal-smoke-test-123".to_string(),
            ])
            .unwrap(),
            Some(super::PackagedSmokeConfig {
                port: 43_117,
                profile_directory: "tarnisheds-arsenal-smoke-test-123".to_string(),
            })
        );
        assert!(
            super::packaged_smoke_config([
                "app.exe".to_string(),
                "--packaged-smoke-port=0".to_string(),
                "--packaged-smoke-profile=tarnisheds-arsenal-smoke-test-123".to_string(),
            ])
            .is_err()
        );
        assert!(
            super::packaged_smoke_config([
                "app.exe".to_string(),
                "--packaged-smoke-port=43117".to_string(),
                "--packaged-smoke-port=43118".to_string(),
                "--packaged-smoke-profile=tarnisheds-arsenal-smoke-test-123".to_string(),
            ])
            .is_err()
        );
        assert!(
            super::packaged_smoke_config([
                "app.exe".to_string(),
                "--packaged-smoke-port=43117".to_string(),
            ])
            .is_err()
        );
        assert!(
            super::packaged_smoke_config([
                "app.exe".to_string(),
                "--packaged-smoke-port=43117".to_string(),
                "--packaged-smoke-profile=../normal-profile".to_string(),
            ])
            .is_err()
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn packaged_smoke_browser_args_preserve_runtime_defaults() {
        let args = super::packaged_smoke_browser_args(43_117);
        assert!(args.contains("--remote-debugging-port=43117"));
        assert!(args.contains("--remote-debugging-address=127.0.0.1"));
        assert!(args.contains("msSmartScreenProtection"));
    }

    #[test]
    fn standalone_release_profiles_and_manifests_are_complete() {
        for profile_id in [
            er_optimizer_core::VANILLA_PROFILE_ID,
            er_optimizer_core::CONVERGENCE_PROFILE_ID,
        ] {
            let (data, manifest) =
                er_optimizer_core::load_embedded_game_profile_with_manifest(profile_id)
                    .expect("embedded profile data and manifest load");
            assert!(data.weapons.len() > 3000);
            assert!(data.aows.len() > 100);
            assert_eq!(manifest.profile.id, profile_id);
            assert!(!manifest.id.is_empty());
            assert!(!manifest.app_version.is_empty());
            assert_eq!(data.dataset_version, manifest.dataset_version);
        }
    }
}
