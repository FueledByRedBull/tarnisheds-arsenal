use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use er_optimizer_core::{GameData, SnapshotManifest};
use tauri::Manager;

mod commands;
mod dto;
mod errors;

// Worker panics become a retryable job error only while panics unwind.
#[cfg(panic = "abort")]
compile_error!("spawn_supervised_job needs panic = \"unwind\"; do not set panic = \"abort\"");

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
    id_prefix: &'static str,
    is_finished: fn(&T) -> bool,
    handle: Mutex<Option<(String, AsyncJobHandle<T>)>>,
    /// The supervisor of the registered job, tagged with that job's id. A supervisor
    /// joins its worker before publishing, so one that has ended owns nothing.
    supervisor: Mutex<Option<(String, tauri::async_runtime::JoinHandle<()>)>>,
}

impl<T: Clone + Send + 'static> JobRegistry<T> {
    pub fn new(kind: &'static str, id_prefix: &'static str, is_finished: fn(&T) -> bool) -> Self {
        Self {
            kind,
            id_prefix,
            is_finished,
            handle: Mutex::new(None),
            supervisor: Mutex::new(None),
        }
    }

    /// Registers a job and runs `worker` on a blocking thread. `publish` receives the
    /// worker outcome, the job id and whether cancellation was requested by then.
    pub fn start<R: Send + 'static>(
        &self,
        next_job: &AtomicU64,
        initial: T,
        worker: impl FnOnce(String, CancelFlag, Arc<Mutex<T>>) -> R + Send + 'static,
        publish: impl FnOnce(&mut T, Result<R, String>, String, bool) + Send + 'static,
    ) -> Result<dto::StartSearchResponseDto, errors::AppError> {
        let job_id = format!(
            "{}-{}",
            self.id_prefix,
            next_job.fetch_add(1, Ordering::Relaxed)
        );
        let cancel: CancelFlag = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(initial));
        self.insert_if_idle(
            job_id.clone(),
            AsyncJobHandle {
                cancel: Arc::clone(&cancel),
                status: Arc::clone(&status),
            },
        )?;
        let (worker_id, worker_cancel, worker_status) =
            (job_id.clone(), Arc::clone(&cancel), Arc::clone(&status));
        let publish_id = job_id.clone();
        let supervisor = spawn_supervised_job(
            status,
            move || worker(worker_id, worker_cancel, worker_status),
            move |status, outcome| {
                publish(status, outcome, publish_id, cancel.load(Ordering::Relaxed));
            },
        );
        *self
            .supervisor
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some((job_id.clone(), supervisor));
        Ok(dto::StartSearchResponseDto { job_id })
    }

    fn supervisor_ended(&self, job_id: &str) -> bool {
        self.supervisor
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .is_some_and(|(id, supervisor)| id == job_id && supervisor.inner().is_finished())
    }

    pub fn insert_if_idle(
        &self,
        job_id: String,
        handle: AsyncJobHandle<T>,
    ) -> Result<(), errors::AppError> {
        let mut guard = self.handle.lock().map_err(|_| self.lock_error())?;
        // A supervisor that ended without publishing (its publication panicked) leaves the
        // status unfinished, and possibly poisoned, but its worker is already joined.
        if let Some((previous_id, previous)) = guard.as_ref()
            && !self.supervisor_ended(previous_id)
        {
            let status = previous.status.lock().map_err(|_| self.status_error())?;
            if !(self.is_finished)(&status) {
                return Err(errors::AppError::new(format!(
                    "{} job is already running. Stop or wait for it before starting another.",
                    self.kind
                )));
            }
        }
        *guard = Some((job_id, handle));
        Ok(())
    }

    pub fn cancel(&self, job_id: &str) -> Result<bool, errors::AppError> {
        let guard = self.handle.lock().map_err(|_| self.lock_error())?;
        Ok(guard
            .as_ref()
            .is_some_and(|(active_id, handle)| active_id == job_id && self.request_cancel(handle)))
    }

    /// Cancels the running job whatever its id, for a page that lost its job ids.
    pub fn cancel_active(&self) {
        if let Ok(guard) = self.handle.lock()
            && let Some((_, handle)) = guard.as_ref()
        {
            self.request_cancel(handle);
        }
    }

    fn request_cancel(&self, handle: &AsyncJobHandle<T>) -> bool {
        // Serialize with terminal publication. Poisoned status still permits a
        // cancellation request, but never establishes that the worker finished.
        let status = handle.status.lock();
        if status
            .as_ref()
            .is_ok_and(|status| (self.is_finished)(status))
        {
            return false;
        }
        handle.cancel.store(true, Ordering::Relaxed);
        true
    }

    /// Reading a terminal status keeps it until the next start, so a status reply
    /// lost in transit can be read again.
    pub fn status(&self, job_id: &str) -> Result<Option<T>, errors::AppError> {
        let status = {
            let guard = self.handle.lock().map_err(|_| self.lock_error())?;
            match guard.as_ref() {
                Some((active_id, handle)) if active_id == job_id => Arc::clone(&handle.status),
                _ => return Ok(None),
            }
        };
        // Clone a possibly large terminal payload without blocking start and cancel.
        let status = status.lock().map_err(|_| self.status_error())?.clone();
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

    fn registry() -> JobRegistry<bool> {
        JobRegistry::new("test", "test", |done| *done)
    }

    fn handle(status: Arc<Mutex<bool>>) -> AsyncJobHandle<bool> {
        AsyncJobHandle {
            cancel: Arc::new(AtomicBool::new(false)),
            status,
        }
    }

    #[test]
    fn poisoned_status_does_not_allow_replacing_an_unconfirmed_job() {
        let registry = registry();
        let status = Arc::new(Mutex::new(false));
        registry
            .insert_if_idle("first".into(), handle(Arc::clone(&status)))
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

        let replacement =
            registry.insert_if_idle("second".into(), handle(Arc::new(Mutex::new(false))));
        assert!(
            replacement.is_err(),
            "unknown worker state must retain ownership"
        );
        assert!(registry.cancel("first").unwrap());
        assert!(!registry.cancel("second").unwrap());
        assert!(registry.status("first").is_err());
    }

    #[test]
    fn finished_job_rejects_late_cancellation() {
        let registry = registry();
        let cancel = Arc::new(AtomicBool::new(false));
        registry
            .insert_if_idle(
                "finished".into(),
                AsyncJobHandle {
                    cancel: Arc::clone(&cancel),
                    status: Arc::new(Mutex::new(true)),
                },
            )
            .unwrap();
        assert!(!registry.cancel("finished").unwrap());
        registry.cancel_active();
        assert!(!cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn active_job_is_cancelled_without_its_id() {
        let registry = registry();
        let cancel = Arc::new(AtomicBool::new(false));
        registry.cancel_active();
        registry
            .insert_if_idle(
                "orphan".into(),
                AsyncJobHandle {
                    cancel: Arc::clone(&cancel),
                    status: Arc::new(Mutex::new(false)),
                },
            )
            .unwrap();
        registry.cancel_active();
        assert!(cancel.load(Ordering::Relaxed));
    }

    fn register_running<T: Clone + Send + 'static>(
        registry: &JobRegistry<T>,
        status: T,
    ) -> CancelFlag {
        let cancel: CancelFlag = Arc::new(AtomicBool::new(false));
        registry
            .insert_if_idle(
                "running".into(),
                AsyncJobHandle {
                    cancel: Arc::clone(&cancel),
                    status: Arc::new(Mutex::new(status)),
                },
            )
            .unwrap();
        cancel
    }

    #[test]
    fn page_load_cancellation_reaches_every_job_registry() {
        let state = crate::test_app_state();
        let flags = [
            register_running(
                &state.analysis_jobs,
                dto::AnalysisJobStatusDto { finished: None },
            ),
            register_running(
                &state.search_jobs,
                dto::SearchJobStatusDto {
                    progress: None,
                    finished: None,
                },
            ),
            register_running(
                &state.path_jobs,
                dto::PathJobStatusDto {
                    progress: None,
                    finished: None,
                },
            ),
            register_running(
                &state.affinity_jobs,
                dto::AffinityWatchJobStatusDto {
                    progress: None,
                    finished: None,
                },
            ),
        ];
        state.cancel_all_jobs();
        assert!(flags.iter().all(|cancel| cancel.load(Ordering::Relaxed)));
    }

    #[test]
    fn cancellation_waits_for_in_progress_terminal_publication() {
        use std::sync::mpsc;
        use std::time::Duration;

        let registry = Arc::new(registry());
        let status = Arc::new(Mutex::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        registry
            .insert_if_idle(
                "first".into(),
                AsyncJobHandle {
                    cancel: Arc::clone(&cancel),
                    status: Arc::clone(&status),
                },
            )
            .unwrap();
        let mut publishing = status.lock().unwrap();
        let cancelling = Arc::clone(&registry);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let caller = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            result_tx.send(cancelling.cancel("first")).unwrap();
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
        let registry = registry();
        let first_status = Arc::new(Mutex::new(false));
        registry
            .insert_if_idle("first".to_string(), handle(Arc::clone(&first_status)))
            .unwrap();
        assert!(
            registry
                .insert_if_idle("second".to_string(), handle(Arc::new(Mutex::new(false))))
                .is_err()
        );
        assert!(registry.cancel("first").unwrap());
        assert!(
            registry
                .insert_if_idle("second".to_string(), handle(Arc::new(Mutex::new(false))))
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
            )
            .unwrap();
        assert!(!registry.cancel("first").unwrap());
        assert!(!second_cancel.load(Ordering::Relaxed));
        assert!(registry.cancel("second").unwrap());
        assert!(second_cancel.load(Ordering::Relaxed));
        *second_status.lock().unwrap() = true;
        assert_eq!(registry.status("first").unwrap(), None);
        assert_eq!(registry.status("second").unwrap(), Some(true));
        // A terminal status survives repeated reads until the next job replaces it.
        assert_eq!(registry.status("second").unwrap(), Some(true));
    }

    #[test]
    fn supervisor_reports_panic_only_after_worker_termination_and_recovers_status() {
        use std::sync::mpsc;
        use std::time::Duration;

        for poison_status in [false, true] {
            let registry = JobRegistry::new("test", "test", Option::is_some);
            let status = Arc::new(Mutex::new(None::<Result<(), String>>));
            registry
                .insert_if_idle(
                    "first".into(),
                    AsyncJobHandle {
                        cancel: Arc::new(AtomicBool::new(false)),
                        status: Arc::clone(&status),
                    },
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
            assert!(registry.cancel("first").unwrap());
            assert!(registry.status("first").unwrap().unwrap().is_none());
            assert!(
                registry
                    .insert_if_idle(
                        "second".into(),
                        AsyncJobHandle {
                            cancel: Arc::new(AtomicBool::new(false)),
                            status: Arc::new(Mutex::new(None)),
                        },
                    )
                    .is_err()
            );
            release_tx.send(()).unwrap();
            tauri::async_runtime::block_on(supervisor).unwrap();
            assert!(!status.is_poisoned());
            let finished = registry.status("first").unwrap().unwrap().unwrap();
            assert_eq!(
                finished.unwrap_err(),
                "Calculation worker stopped unexpectedly. Retry the operation."
            );
            assert_eq!(publications.load(Ordering::Relaxed), 1);
            registry
                .insert_if_idle(
                    "second".into(),
                    AsyncJobHandle {
                        cancel: Arc::new(AtomicBool::new(false)),
                        status: Arc::new(Mutex::new(None)),
                    },
                )
                .unwrap();
            assert!(registry.status("first").unwrap().is_none());
        }
    }

    #[test]
    fn started_jobs_publish_cancellation_requested_before_completion() {
        use std::sync::mpsc;
        use std::time::Duration;

        let registry = JobRegistry::new("test", "test", Option::is_some);
        let next_job = AtomicU64::new(7);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let started = registry
            .start(
                &next_job,
                None::<(String, bool)>,
                move |_, cancel, _| {
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    cancel.load(Ordering::Relaxed)
                },
                |status, outcome, job_id, cancelled| {
                    assert!(outcome.unwrap());
                    *status = Some((job_id, cancelled));
                },
            )
            .unwrap();
        assert_eq!(started.job_id, "test-7");
        assert!(
            registry
                .start(&next_job, None, |_, _, _| false, |_, _, _, _| {})
                .is_err()
        );
        assert!(registry.cancel("test-7").unwrap());
        release_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let finished = loop {
            if let Some(Some(finished)) = registry.status("test-7").unwrap() {
                break finished;
            }
            assert!(std::time::Instant::now() < deadline, "job must publish");
            std::thread::yield_now();
        };
        assert_eq!(finished, ("test-7".to_string(), true));
    }

    #[test]
    fn job_whose_publication_panicked_does_not_wedge_the_registry() {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let registry = JobRegistry::new("test", "test", Option::is_some);
        let next_job = AtomicU64::new(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        registry
            .start(
                &next_job,
                None::<()>,
                move |_, _, _| release_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                |_, _, _, _| panic!("injected publication failure"),
            )
            .unwrap();
        let running = registry.start(&next_job, None, |_, _, _| (), |_, _, _, _| {});
        assert!(running.is_err(), "a running worker retains ownership");

        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let replacement = loop {
            match registry.start(
                &next_job,
                None,
                |_, _, _| (),
                |status, _, _, _| *status = Some(()),
            ) {
                Ok(started) => break started,
                Err(_) => {
                    assert!(
                        Instant::now() < deadline,
                        "ended supervisor must free the slot"
                    );
                    std::thread::yield_now();
                }
            }
        };
        assert!(registry.status("test-1").unwrap().is_none());
        while registry
            .status(&replacement.job_id)
            .unwrap()
            .is_none_or(|status| status.is_none())
        {
            assert!(Instant::now() < deadline, "replacement job must publish");
            std::thread::yield_now();
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
    fn load(
        load_profile: impl Fn(&str) -> Result<(GameData, SnapshotManifest), String> + Sync,
    ) -> Result<Self, errors::AppError> {
        let profile_ids = [
            er_optimizer_core::VANILLA_PROFILE_ID,
            er_optimizer_core::CONVERGENCE_PROFILE_ID,
        ];
        // Profiles are independent snapshots, so they load side by side; the first failing
        // profile in this order is the one reported.
        let load_profile = &load_profile;
        let loaded = std::thread::scope(|scope| {
            let workers = profile_ids.map(|profile_id| {
                scope.spawn(move || {
                    let (data, manifest) = load_profile(profile_id)?;
                    let catalog_index = commands::data::CatalogIndex::build(&data);
                    Ok::<_, String>(Arc::new(ProfileData {
                        data: Arc::new(data),
                        catalog_index: Arc::new(catalog_index),
                        data_manifest: manifest.into(),
                    }))
                })
            });
            workers.map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
        });
        let mut profiles = HashMap::new();
        for (profile_id, profile) in profile_ids.into_iter().zip(loaded) {
            profiles.insert(profile_id.to_string(), profile?);
        }
        Ok(Self {
            profiles,
            analysis_jobs: JobRegistry::new("analysis", "analysis", |status| {
                status.finished.is_some()
            }),
            search_jobs: JobRegistry::new("search", "search", |status| status.finished.is_some()),
            path_jobs: JobRegistry::new("path", "path", |status| status.finished.is_some()),
            affinity_jobs: JobRegistry::new("affinity watch", "affinity", |status| {
                status.finished.is_some()
            }),
            next_job: AtomicU64::new(1),
        })
    }

    pub fn profile(&self, profile_id: &str) -> Result<&Arc<ProfileData>, errors::AppError> {
        dto::check_len("profileId", profile_id)?;
        self.profiles.get(profile_id).ok_or_else(|| {
            errors::AppError::new(format!(
                "Unknown game profile {profile_id:?}. Reload the catalog and choose an available profile."
            ))
        })
    }

    fn cancel_all_jobs(&self) {
        self.analysis_jobs.cancel_active();
        self.search_jobs.cancel_active();
        self.path_jobs.cancel_active();
        self.affinity_jobs.cancel_active();
    }
}

pub fn run() {
    if let Err(message) = start() {
        report_startup_failure(&message);
        std::process::exit(1);
    }
}

fn start() -> Result<(), String> {
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut context = tauri::generate_context!();
    #[cfg(target_os = "windows")]
    if let Some(port) = smoke_debug_port(
        std::env::var("TARNISHEDS_ARSENAL_SMOKE_DEBUG_PORT").ok(),
        std::env::var_os("WEBVIEW2_USER_DATA_FOLDER"),
    )? {
        // GitHub's Windows runners do not honour WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS (documented
        // as appended, and honoured locally), so the smoke asks the app; the list repeats Tauri's default.
        let browser_args = format!(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
             --remote-debugging-port={port} --remote-debugging-address=127.0.0.1 \
             --disable-gpu --no-first-run"
        );
        for window in &mut context.config_mut().app.windows {
            window.additional_browser_args = Some(browser_args.clone());
        }
    }
    // Load before any window exists, so a slow or failed load never shows a blank window.
    let state = AppState::load(load_desktop_profile).map_err(|error| error.message)?;

    // Tauri panics instead of returning an error when it cannot create the window,
    // for example when the WebView2 runtime is missing.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tauri::Builder::default()
            .plugin(navigation_guard())
            .manage(state)
            .on_page_load(|webview, payload| {
                // A reloaded page lost its job ids; stop the work it can no longer own.
                if payload.event() == tauri::webview::PageLoadEvent::Started {
                    webview.state::<AppState>().cancel_all_jobs();
                }
            })
            .invoke_handler(tauri::generate_handler![
                commands::data::get_catalog,
                commands::data::get_profiles,
                commands::data::get_data_manifest,
                commands::data::get_weapon_profile,
                commands::data::compatible_aow_names_for_affinity,
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
    }))
    .map_err(|panic| {
        panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_string()))
            .unwrap_or_else(|| "The window could not be created.".to_string())
    })?
    .map_err(|error| error.to_string())
}

/// Keeps the main window on the app's own pages, so a link or script cannot load a remote
/// page that would then sit next to the IPC bridge.
fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("nav-guard")
        .on_navigation(|_, url| is_app_origin(url, tauri::is_dev()))
        .build()
}

/// The origins Tauri serves the bundled frontend from, plus the Vite server of a dev build.
/// A `blob:` URL belongs to the origin that created it, which keeps the CSV download working.
fn is_app_origin(url: &tauri::Url, dev: bool) -> bool {
    match (url.scheme(), url.host_str(), url.port()) {
        ("tauri", Some("localhost"), None) => true,
        ("http" | "https", Some("tauri.localhost"), None) => true,
        ("http", Some("localhost" | "127.0.0.1"), Some(1420)) => dev,
        ("blob", None, None) => tauri::Url::parse(url.path())
            .is_ok_and(|inner| inner.scheme() != "blob" && is_app_origin(&inner, dev)),
        _ => false,
    }
}

/// The packaged smoke's remote-debugging port. Only an environment variable (which a shortcut,
/// file association or another user cannot set) enables it, and only with an isolated
/// smoke-only WebView2 profile, so the debugging session never touches the user's data.
#[cfg(target_os = "windows")]
fn smoke_debug_port(
    raw_port: Option<String>,
    user_data_folder: Option<std::ffi::OsString>,
) -> Result<Option<u16>, String> {
    let Some(raw_port) = raw_port else {
        return Ok(None);
    };
    let port = raw_port
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or("TARNISHEDS_ARSENAL_SMOKE_DEBUG_PORT must be an integer from 1 to 65535")?;
    let isolated = user_data_folder.as_deref().is_some_and(|folder| {
        std::path::Path::new(folder)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("tarnisheds-arsenal-smoke-"))
    });
    if !isolated {
        return Err(
            "TARNISHEDS_ARSENAL_SMOKE_DEBUG_PORT requires WEBVIEW2_USER_DATA_FOLDER to name a tarnisheds-arsenal-smoke-* directory"
                .to_string(),
        );
    }
    Ok(Some(port))
}

// Release builds have no console, so a startup failure is also shown in a message box.
#[allow(unsafe_code)]
fn report_startup_failure(message: &str) {
    use std::io::Write;
    // `eprintln!` would panic on a closed stderr pipe.
    let _ = writeln!(
        std::io::stderr(),
        "Tarnished's Arsenal could not start: {message}"
    );
    #[cfg(windows)]
    {
        // The packaged smoke reads the message from stderr and waits for the exit code, which a
        // modal message box would hold back; its one variable also covers a rejected smoke setup.
        if std::env::var_os("TARNISHEDS_ARSENAL_SMOKE_DEBUG_PORT").is_some() {
            return;
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                window: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                kind: u32,
            ) -> i32;
        }
        const MB_ICONERROR: u32 = 0x10;
        let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let text = wide(&format!(
            "Tarnished's Arsenal could not start.\n\n{message}"
        ));
        let caption = wide("Tarnished's Arsenal");
        // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_ICONERROR,
            );
        }
    }
}

fn load_desktop_profile(profile_id: &str) -> Result<(GameData, SnapshotManifest), String> {
    // Debug builds read the repository snapshot, so data edits apply without a rebuild.
    #[cfg(debug_assertions)]
    {
        let relative = if profile_id == er_optimizer_core::VANILLA_PROFILE_ID {
            std::path::PathBuf::from("phase1")
        } else {
            std::path::PathBuf::from("profiles").join(profile_id)
        };
        let data_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../data")
            .join(relative);
        let (data, manifest) = er_optimizer_core::load_game_data_with_manifest(&data_dir)?;
        if manifest.profile.id != profile_id {
            return Err(format!(
                "{} holds profile {:?}, expected {profile_id:?}",
                data_dir.display(),
                manifest.profile.id
            ));
        }
        Ok((data, manifest))
    }
    #[cfg(not(debug_assertions))]
    {
        er_optimizer_core::load_embedded_game_profile_with_manifest(profile_id)
    }
}

#[cfg(test)]
pub(crate) fn test_app_state() -> AppState {
    AppState::load(er_optimizer_core::load_embedded_game_profile_with_manifest)
        .expect("embedded test snapshots load")
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
pub(crate) type HostileValue = (&'static str, fn(&mut dto::OptimizeRequestDto));

/// Out-of-range and oversized values for every client-controlled request field.
#[cfg(test)]
pub(crate) const HOSTILE_OPTIMIZE_VALUES: &[HostileValue] = &[
    ("vig", |request| request.vig = u8::MAX),
    ("mnd", |request| request.mnd = u8::MAX),
    ("end", |request| request.end = u8::MAX),
    ("str", |request| request.str_stat = u8::MAX),
    ("dex", |request| request.dex = u8::MAX),
    ("int", |request| request.int_stat = u8::MAX),
    ("fai", |request| request.fai = u8::MAX),
    ("arc", |request| request.arc = u8::MAX),
    ("min_str", |request| request.min_str = u8::MAX),
    ("min_dex", |request| request.min_dex = u8::MAX),
    ("min_int", |request| request.min_int = u8::MAX),
    ("min_fai", |request| request.min_fai = u8::MAX),
    ("min_arc", |request| request.min_arc = u8::MAX),
    ("lock_str", |request| request.lock_str = Some(u8::MAX)),
    ("lock_dex", |request| request.lock_dex = Some(u8::MAX)),
    ("lock_int", |request| request.lock_int = Some(u8::MAX)),
    ("lock_fai", |request| request.lock_fai = Some(u8::MAX)),
    ("lock_arc", |request| request.lock_arc = Some(u8::MAX)),
    ("character_level", |request| {
        request.character_level = u16::MAX;
    }),
    ("standard_max_upgrade", |request| {
        request.standard_max_upgrade = Some(u8::MAX);
    }),
    ("somber_max_upgrade", |request| {
        request.somber_max_upgrade = Some(u8::MAX);
    }),
    ("scadutree_level", |request| {
        request.scadutree_level = u8::MAX
    }),
    ("top_k", |request| request.top_k = usize::MAX),
    ("filters.version", |request| {
        request.filters.version = u8::MAX
    }),
    ("filters.entries", |request| {
        request.filters.entries = vec![
            dto::StableFilterEntryDto {
                dimension: "weapon_family".into(),
                id: "x".into(),
                mode: "include".into(),
            };
            513
        ];
    }),
    ("filters.entries.dimension", |request| {
        request.filters.entries = vec![dto::StableFilterEntryDto {
            dimension: oversized_text(),
            id: "x".into(),
            mode: "include".into(),
        }];
    }),
    ("filters.entries.mode", |request| {
        request.filters.entries = vec![dto::StableFilterEntryDto {
            dimension: "weapon_family".into(),
            id: "x".into(),
            mode: oversized_text(),
        }];
    }),
    ("profile_id", |request| {
        request.profile_id = oversized_text()
    }),
    ("class_name", |request| {
        request.class_name = oversized_text()
    }),
    ("weapon_name", |request| {
        request.weapon_name = Some(oversized_text());
    }),
    ("affinity", |request| {
        request.affinity = Some(oversized_text())
    }),
    ("aow_name", |request| {
        request.aow_name = Some(oversized_text())
    }),
    ("weapon_type_key", |request| {
        request.weapon_type_key = Some(oversized_text());
    }),
    ("somber_filter", |request| {
        request.somber_filter = oversized_text()
    }),
    ("result_grouping", |request| {
        request.result_grouping = oversized_text();
    }),
    ("objective", |request| request.objective = oversized_text()),
];

#[cfg(test)]
pub(crate) fn oversized_text() -> String {
    "x".repeat(dto::MAX_TEXT_LEN + 1)
}

/// Runs a hostile request, which must fail fast with an error that does not echo oversized text.
#[cfg(test)]
pub(crate) fn assert_rejected_quickly<T: std::fmt::Debug>(
    label: &str,
    run: impl FnOnce() -> Result<T, errors::AppError>,
) {
    let started = std::time::Instant::now();
    let outcome = run();
    let elapsed = started.elapsed();
    let error = outcome.expect_err(label);
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "{label} took {elapsed:?} to fail"
    );
    assert!(
        !error.message.contains(&oversized_text()[..32]),
        "{label} echoed oversized text: {}",
        error.message
    );
}

/// Applies each hostile value (except the skipped ones, which the entry point resets
/// before they matter) to the test request and requires the entry point to reject it.
#[cfg(test)]
pub(crate) fn assert_hostile_values_rejected(
    entry_point: &str,
    skipped: &[&str],
    run: impl Fn(dto::OptimizeRequestDto) -> Result<(), errors::AppError>,
) {
    for (label, apply) in HOSTILE_OPTIMIZE_VALUES
        .iter()
        .filter(|(label, _)| !skipped.contains(label))
    {
        let mut request = test_optimize_request();
        apply(&mut request);
        assert_rejected_quickly(&format!("{entry_point}: {label}"), || run(request));
    }
}

#[cfg(test)]
mod release_data_tests {
    #[cfg(target_os = "windows")]
    #[test]
    fn smoke_debug_port_needs_a_valid_port_and_an_isolated_profile() {
        let parse = |port: Option<&str>, folder: Option<&str>| {
            super::smoke_debug_port(port.map(str::to_string), folder.map(Into::into))
        };
        let isolated = Some(r"C:\Users\me\AppData\Local\main\tarnisheds-arsenal-smoke-123");
        assert_eq!(parse(None, None), Ok(None));
        assert_eq!(parse(Some("43117"), isolated), Ok(Some(43_117)));
        // Without the smoke-only WebView2 folder the port would open on the user's profile.
        assert!(parse(Some("43117"), None).is_err());
        assert!(
            parse(
                Some("43117"),
                Some(r"C:\Users\me\AppData\Local\main\EBWebView")
            )
            .is_err()
        );
        assert!(parse(Some("0"), isolated).is_err());
        assert!(parse(Some("port"), isolated).is_err());
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

#[cfg(test)]
mod navigation_guard_tests {
    use super::is_app_origin;

    fn allowed(url: &str, dev: bool) -> bool {
        is_app_origin(&tauri::Url::parse(url).expect("test URL parses"), dev)
    }

    #[test]
    fn app_pages_and_their_blobs_are_allowed() {
        for url in [
            "http://tauri.localhost/",
            "http://tauri.localhost/index.html#/rankings",
            "https://tauri.localhost/",
            "tauri://localhost/",
            "tauri://localhost/index.html",
            "blob:http://tauri.localhost/6a2f1f4e-0a5a-4c53-8f4a-2f3c1a7b9d10",
            "blob:tauri://localhost/6a2f1f4e-0a5a-4c53-8f4a-2f3c1a7b9d10",
        ] {
            assert!(allowed(url, false), "{url}");
            assert!(allowed(url, true), "{url}");
        }
    }

    #[test]
    fn the_dev_server_is_allowed_only_in_dev_builds() {
        for url in [
            "http://localhost:1420/",
            "http://127.0.0.1:1420/index.html",
            "blob:http://localhost:1420/6a2f1f4e-0a5a-4c53-8f4a-2f3c1a7b9d10",
        ] {
            assert!(allowed(url, true), "{url}");
            assert!(!allowed(url, false), "{url}");
        }
        for url in [
            "http://localhost:1421/",
            "http://localhost/",
            "https://localhost:1420/",
            "http://example.com:1420/",
        ] {
            assert!(!allowed(url, true), "{url}");
        }
    }

    #[test]
    fn remote_and_script_like_pages_are_refused() {
        for url in [
            "https://example.com/",
            "http://tauri.localhost.example.com/",
            "http://tauri.localhost@example.com/",
            "http://tauri.localhost:8080/",
            "http://ipc.localhost/",
            "tauri://example.com/",
            "tauri://localhost:1420/",
            "file:///C:/Windows/win.ini",
            "data:text/html,<script>alert(1)</script>",
            "javascript:alert(1)",
            "about:blank",
            "ftp://tauri.localhost/",
            "blob:https://example.com/6a2f1f4e-0a5a-4c53-8f4a-2f3c1a7b9d10",
            "blob:blob:http://tauri.localhost/x",
            "blob:null/6a2f1f4e-0a5a-4c53-8f4a-2f3c1a7b9d10",
            "blob:",
        ] {
            assert!(!allowed(url, true), "{url}");
            assert!(!allowed(url, false), "{url}");
        }
    }
}
