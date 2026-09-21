use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use er_optimizer_core::GameData;
use tauri::Manager;

mod commands;
mod dto;
mod errors;

pub type CancelFlag = Arc<AtomicBool>;

fn spawn_supervised_job<T, R>(
    status: Arc<Mutex<T>>,
    worker: impl FnOnce() -> R + Send + 'static,
    publish: impl FnOnce(&mut T, Result<R, String>) + Send + 'static,
) -> tauri::async_runtime::JoinHandle<()>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let worker = tauri::async_runtime::spawn_blocking(worker);
    tauri::async_runtime::spawn(async move {
        let outcome = worker.await.map_err(|_| {
            "Calculation worker stopped unexpectedly. Retry the operation.".to_string()
        });
        // Joining establishes termination, so a poisoned progress update cannot
        // discard worker ownership or prevent its terminal status from being read.
        let mut guard = status.lock().unwrap_or_else(|error| error.into_inner());
        publish(&mut guard, outcome);
        status.clear_poison();
    })
}

pub struct AsyncJobHandle<T> {
    pub cancel: CancelFlag,
    pub status: Arc<Mutex<T>>,
}

pub struct JobRegistry<T> {
    kind: &'static str,
    handle: Mutex<Option<(String, AsyncJobHandle<T>)>>,
}

impl<T: Clone> JobRegistry<T> {
    pub fn new(kind: &'static str) -> Self {
        Self {
            kind,
            handle: Mutex::new(None),
        }
    }

    pub fn insert_if_idle(
        &self,
        job_id: String,
        handle: AsyncJobHandle<T>,
        is_finished: impl Fn(&T) -> bool,
    ) -> Result<(), errors::AppError> {
        let mut guard = self.handle.lock().map_err(|_| self.lock_error())?;
        if let Some((_, previous)) = guard.as_ref() {
            let status = previous.status.lock().map_err(|_| self.status_error())?;
            if !is_finished(&status) {
                return Err(errors::AppError::new(format!(
                    "{} job is already running. Stop or wait for it before starting another.",
                    self.kind
                )));
            }
        }
        *guard = Some((job_id, handle));
        Ok(())
    }

    pub fn cancel(
        &self,
        job_id: &str,
        is_finished: impl Fn(&T) -> bool,
    ) -> Result<bool, errors::AppError> {
        let guard = self.handle.lock().map_err(|_| self.lock_error())?;
        let Some((active_id, handle)) = guard.as_ref() else {
            return Ok(false);
        };
        if active_id != job_id {
            return Ok(false);
        }
        // Serialize with terminal publication. Poisoned status still permits a
        // cancellation request, but never establishes that the worker finished.
        let status = handle.status.lock();
        if status.as_ref().is_ok_and(|status| is_finished(status)) {
            return Ok(false);
        }
        handle.cancel.store(true, Ordering::Relaxed);
        Ok(true)
    }

    pub fn status(
        &self,
        job_id: &str,
        is_finished: impl Fn(&T) -> bool,
    ) -> Result<Option<T>, errors::AppError> {
        let mut guard = self.handle.lock().map_err(|_| self.lock_error())?;
        let status = {
            let Some((active_id, handle)) = guard.as_ref() else {
                return Ok(None);
            };
            if active_id != job_id {
                return Ok(None);
            }
            handle
                .status
                .lock()
                .map_err(|_| self.status_error())?
                .clone()
        };
        if is_finished(&status) {
            guard.take();
        }
        Ok(Some(status))
    }

    fn status_error(&self) -> errors::AppError {
        errors::AppError::new(format!(
            "{} job status is unavailable. Retry once, then restart the app if it persists.",
            self.kind
        ))
    }

    fn lock_error(&self) -> errors::AppError {
        errors::AppError::new(format!(
            "{} job registry is unavailable. Retry once, then restart the app if it persists.",
            self.kind
        ))
    }
}

#[cfg(test)]
mod job_registry_tests {
    use super::*;

    fn handle(status: Arc<Mutex<bool>>) -> AsyncJobHandle<bool> {
        AsyncJobHandle {
            cancel: Arc::new(AtomicBool::new(false)),
            status,
        }
    }

    #[test]
    fn poisoned_status_does_not_allow_replacing_an_unconfirmed_job() {
        let registry = JobRegistry::new("test");
        let status = Arc::new(Mutex::new(false));
        registry
            .insert_if_idle("first".into(), handle(Arc::clone(&status)), |done| *done)
            .unwrap();
        let poisoned = Arc::clone(&status);
        assert!(
            std::thread::spawn(move || {
                let _guard = poisoned.lock().unwrap();
                panic!("injected status update failure");
            })
            .join()
            .is_err()
        );

        let replacement = registry.insert_if_idle(
            "second".into(),
            handle(Arc::new(Mutex::new(false))),
            |done| *done,
        );
        assert!(
            replacement.is_err(),
            "unknown worker state must retain ownership"
        );
        assert!(registry.cancel("first", |done| *done).unwrap());
        assert!(!registry.cancel("second", |done| *done).unwrap());
        assert!(registry.status("first", |done| *done).is_err());
    }

    #[test]
    fn finished_job_rejects_late_cancellation() {
        let registry = JobRegistry::new("test");
        let cancel = Arc::new(AtomicBool::new(false));
        registry
            .insert_if_idle(
                "finished".into(),
                AsyncJobHandle {
                    cancel: Arc::clone(&cancel),
                    status: Arc::new(Mutex::new(true)),
                },
                |done| *done,
            )
            .unwrap();
        assert!(!registry.cancel("finished", |done| *done).unwrap());
        assert!(!cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn cancellation_waits_for_in_progress_terminal_publication() {
        use std::sync::mpsc;
        use std::time::Duration;

        let registry = Arc::new(JobRegistry::new("test"));
        let status = Arc::new(Mutex::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        registry
            .insert_if_idle(
                "first".into(),
                AsyncJobHandle {
                    cancel: Arc::clone(&cancel),
                    status: Arc::clone(&status),
                },
                |done| *done,
            )
            .unwrap();
        let mut publishing = status.lock().unwrap();
        let cancelling = Arc::clone(&registry);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let caller = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            result_tx
                .send(cancelling.cancel("first", |done| *done))
                .unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            result_rx.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        *publishing = true;
        drop(publishing);
        assert!(
            !result_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap()
        );
        caller.join().unwrap();
        assert!(!cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn registry_replaces_finished_jobs_without_leaking_old_ids() {
        let registry = JobRegistry::new("test");
        let first_status = Arc::new(Mutex::new(false));
        registry
            .insert_if_idle(
                "first".to_string(),
                handle(Arc::clone(&first_status)),
                |status| *status,
            )
            .unwrap();
        assert!(
            registry
                .insert_if_idle(
                    "second".to_string(),
                    handle(Arc::new(Mutex::new(false))),
                    |status| *status,
                )
                .is_err()
        );
        assert!(registry.cancel("first", |done| *done).unwrap());
        assert!(
            registry
                .insert_if_idle(
                    "second".to_string(),
                    handle(Arc::new(Mutex::new(false))),
                    |status| *status,
                )
                .is_err()
        );

        *first_status.lock().unwrap() = true;
        let second_cancel = Arc::new(AtomicBool::new(false));
        let second_status = Arc::new(Mutex::new(false));
        registry
            .insert_if_idle(
                "second".to_string(),
                AsyncJobHandle {
                    cancel: Arc::clone(&second_cancel),
                    status: Arc::clone(&second_status),
                },
                |status| *status,
            )
            .unwrap();
        assert!(!registry.cancel("first", |done| *done).unwrap());
        assert!(!second_cancel.load(Ordering::Relaxed));
        assert!(registry.cancel("second", |done| *done).unwrap());
        assert!(second_cancel.load(Ordering::Relaxed));
        *second_status.lock().unwrap() = true;
        assert_eq!(registry.status("first", |status| *status).unwrap(), None);
        assert_eq!(
            registry.status("second", |status| *status).unwrap(),
            Some(true)
        );
        assert_eq!(registry.status("second", |status| *status).unwrap(), None);
    }

    #[test]
    fn supervisor_reports_panic_only_after_worker_termination_and_recovers_status() {
        use std::sync::mpsc;
        use std::time::Duration;

        for poison_status in [false, true] {
            let registry = JobRegistry::new("test");
            let status = Arc::new(Mutex::new(None::<Result<(), String>>));
            registry
                .insert_if_idle(
                    "first".into(),
                    AsyncJobHandle {
                        cancel: Arc::new(AtomicBool::new(false)),
                        status: Arc::clone(&status),
                    },
                    Option::is_some,
                )
                .unwrap();
            let worker_status = Arc::clone(&status);
            let (started_tx, started_rx) = mpsc::sync_channel(1);
            let (release_tx, release_rx) = mpsc::sync_channel(1);
            let publications = Arc::new(AtomicU64::new(0));
            let published = Arc::clone(&publications);
            let supervisor = spawn_supervised_job(
                Arc::clone(&status),
                move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    let _guard = poison_status.then(|| worker_status.lock().unwrap());
                    panic!("injected calculation panic");
                },
                move |status, outcome: Result<(), String>| {
                    published.fetch_add(1, Ordering::Relaxed);
                    *status = Some(outcome);
                },
            );
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(registry.cancel("first", Option::is_some).unwrap());
            assert!(
                registry
                    .status("first", Option::is_some)
                    .unwrap()
                    .unwrap()
                    .is_none()
            );
            assert!(
                registry
                    .insert_if_idle(
                        "second".into(),
                        AsyncJobHandle {
                            cancel: Arc::new(AtomicBool::new(false)),
                            status: Arc::new(Mutex::new(None)),
                        },
                        Option::is_some,
                    )
                    .is_err()
            );
            release_tx.send(()).unwrap();
            tauri::async_runtime::block_on(supervisor).unwrap();
            assert!(!status.is_poisoned());
            let finished = registry
                .status("first", Option::is_some)
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(
                finished.unwrap_err(),
                "Calculation worker stopped unexpectedly. Retry the operation."
            );
            assert_eq!(publications.load(Ordering::Relaxed), 1);
            assert!(registry.status("first", Option::is_some).unwrap().is_none());
            registry
                .insert_if_idle(
                    "second".into(),
                    AsyncJobHandle {
                        cancel: Arc::new(AtomicBool::new(false)),
                        status: Arc::new(Mutex::new(None)),
                    },
                    Option::is_some,
                )
                .unwrap();
        }
    }

    #[test]
    fn supervisor_preserves_worker_results() {
        for result in [Ok(Some(42)), Err("invalid request".to_string()), Ok(None)] {
            let status = Arc::new(Mutex::new(None));
            let expected = result.clone();
            let supervisor = spawn_supervised_job(
                Arc::clone(&status),
                move || result,
                |status, outcome| *status = Some(outcome.unwrap()),
            );
            tauri::async_runtime::block_on(supervisor).unwrap();
            assert_eq!(*status.lock().unwrap(), Some(expected));
        }
    }
}

pub struct AppState {
    pub profiles: HashMap<String, Arc<ProfileData>>,
    pub analysis_jobs: JobRegistry<dto::AnalysisJobStatusDto>,
    pub search_jobs: JobRegistry<dto::SearchJobStatusDto>,
    pub path_jobs: JobRegistry<dto::PathJobStatusDto>,
    pub affinity_jobs: JobRegistry<dto::AffinityWatchJobStatusDto>,
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
                analysis_jobs: JobRegistry::new("analysis"),
                search_jobs: JobRegistry::new("search"),
                path_jobs: JobRegistry::new("path"),
                affinity_jobs: JobRegistry::new("affinity watch"),
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
            commands::optimize::start_solve_build,
            commands::optimize::start_upgrade_series,
            commands::optimize::start_ar_bleed_frontier,
            commands::optimize::cancel_analysis,
            commands::optimize::get_analysis_status,
            commands::optimize::start_search,
            commands::optimize::cancel_search,
            commands::optimize::get_search_status,
            commands::paths::start_path_preview,
            commands::paths::cancel_path_preview,
            commands::paths::get_path_preview_status,
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
        analysis_jobs: JobRegistry::new("analysis"),
        search_jobs: JobRegistry::new("search"),
        path_jobs: JobRegistry::new("path"),
        affinity_jobs: JobRegistry::new("affinity watch"),
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
        filters: dto::StableFilterSetDto::default(),
        result_grouping: "automatic".to_string(),
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
