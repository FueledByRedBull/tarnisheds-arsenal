use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use er_optimizer_core::model::COMBAT_STAT_COUNT;
use er_optimizer_core::{
    FilterDimension, FilterMode, GameData, OptimizeObjective, OptimizeRequest, StableFilter,
    Weapon, optimize_prepared_with_progress, optimize_with_cancel,
    prepare_loadout_evaluator_with_cancel, prepare_search_with_cancel,
    prepare_upgrade_series_evaluator_with_cancel,
};
use tauri::State;

use crate::commands::data::{uses_class_budget, weapon_row_upgrade_cap};
use crate::dto::{
    AnalysisFinishedDto, AnalysisJobKindDto, AnalysisJobStatusDto, ArBleedFrontierPointDto,
    ArBleedFrontierRequestDto, MAX_STAT, OptimizeRequestDto, SearchFinishedDto, SearchJobStatusDto,
    SearchProgressDto, SolveBuildRequestDto, SolvedBuildDto, StartSearchResponseDto,
    UpgradePointDto, UpgradeSeriesRequestDto, check_len, finish, metric_for_objective,
};
use crate::errors::AppError;
use crate::request::{self, cancel_job, job_status};
use crate::{AppState, ProfileData};

/// Candidates a search scores between progress snapshots.
const SEARCH_PROGRESS_EVERY: u64 = 10_000;
/// The shortest time between published search snapshots.
const SEARCH_PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(test)]
pub fn run_search_inner(
    request: OptimizeRequestDto,
    state: &AppState,
) -> Result<Vec<SolvedBuildDto>, AppError> {
    let (request, data) = prepare_search(&request, state)?;
    let status = Mutex::new(SearchJobStatusDto {
        progress: None,
        finished: None,
    });
    let finished = run_search_worker(
        &request,
        &data,
        String::new(),
        &AtomicBool::new(false),
        &status,
    );
    finished
        .error
        .map_or(Ok(finished.rows), |error| Err(AppError::new(error)))
}

/// The payload, error and cancellation of a finished worker. A result completed after
/// cancellation was requested is discarded.
fn split_outcome<T>(
    result: Result<T, String>,
    cancel_flag: &AtomicBool,
) -> (Option<T>, Option<String>, bool) {
    match result {
        Ok(value) if !cancel_flag.load(Ordering::Relaxed) => (Some(value), None, false),
        Ok(_) => (None, None, true),
        Err(message) if message == "cancelled" => (None, None, true),
        Err(message) => (None, Some(message), false),
    }
}

/// Pins the validated request to one loadout, locks the stats of a Custom stats request
/// to the ones it states, and fits the upgrade caps to the weapon.
fn pinned_build(
    mut request: OptimizeRequest,
    profile: &ProfileData,
    weapon_name: &str,
    affinity: Option<&str>,
    aow_name: Option<&str>,
) -> Result<OptimizeRequest, AppError> {
    request::pin_loadout(&mut request, weapon_name, affinity, aow_name);
    if !uses_class_budget(profile, &request.class_name) {
        request::lock_to_current_stats(&mut request);
    }
    request::settle_upgrade_caps(&mut request, profile)?;
    Ok(request)
}

fn prepare_solve_build(
    request: &SolveBuildRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>), AppError> {
    check_len("weaponName", &request.weapon_name)?;
    for (field, value) in [
        ("affinity", request.affinity.as_deref()),
        ("aowName", request.aow_name.as_deref()),
    ] {
        if let Some(value) = value {
            check_len(field, value)?;
        }
    }
    let profile = state.profile(&request.base.profile_id)?;
    if let Some(aow_name) = &request.aow_name {
        request::check_aow_known(profile, aow_name)?;
    }
    let base = request::validate(&request.base, profile)?;
    let core_request = pinned_build(
        base,
        profile,
        &request.weapon_name,
        request.affinity.as_deref(),
        request.aow_name.as_deref(),
    )?;
    Ok((core_request, Arc::clone(&profile.data)))
}

#[cfg(test)]
fn solve_build_inner(
    request: SolveBuildRequestDto,
    state: &AppState,
) -> Result<Option<SolvedBuildDto>, AppError> {
    solve_build_inner_with_cancel(request, state, || true)
}

#[cfg(test)]
fn solve_build_inner_with_cancel<F>(
    request: SolveBuildRequestDto,
    state: &AppState,
    should_continue: F,
) -> Result<Option<SolvedBuildDto>, AppError>
where
    F: Fn() -> bool + Sync,
{
    let (request, data) = prepare_solve_build(&request, state)?;
    optimize_with_cancel(&request, &data, should_continue)
        .map(|mut rows| rows.pop().map(SolvedBuildDto::from))
        .map_err(AppError::from)
}

#[tauri::command]
pub fn start_solve_build(
    request: SolveBuildRequestDto,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    let (core_request, data) = prepare_solve_build(&request, &state)?;
    state.analysis_jobs.start(
        &state.next_job,
        AnalysisJobStatusDto { finished: None },
        move |job_id, cancel_flag, _| {
            let result = optimize_with_cancel(&core_request, &data, || {
                !cancel_flag.load(Ordering::Relaxed)
            })
            .map(|mut rows| rows.pop().map(SolvedBuildDto::from));
            let (result, error, cancelled) = split_outcome(result, &cancel_flag);
            AnalysisFinishedDto {
                job_id,
                kind: AnalysisJobKindDto::SolveBuild,
                cancelled,
                result: result.flatten(),
                points: Vec::new(),
                frontier: Vec::new(),
                error,
            }
        },
        |status, outcome, job_id, cancelled| {
            publish_analysis_finished(
                status,
                outcome,
                job_id,
                AnalysisJobKindDto::SolveBuild,
                cancelled,
            );
        },
    )
}

#[cfg(test)]
pub fn build_upgrade_series_inner(
    request: UpgradeSeriesRequestDto,
    state: &AppState,
) -> Result<Vec<UpgradePointDto>, AppError> {
    build_upgrade_series_inner_with_cancel(request, state, || true)
}

fn prepare_upgrade_series(
    request: &UpgradeSeriesRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>, u8, OptimizeObjective), AppError> {
    let profile = state.profile(&request.base.profile_id)?;
    let solved = &request.solved;
    let weapon_cap = validate_solved_loadout(profile, solved)?;
    let mut core_request = request::validate(&request.base, profile)?;
    request::pin_loadout(
        &mut core_request,
        &solved.weapon_name,
        Some(&solved.affinity),
        solved.aow_name.as_deref(),
    );
    request::set_upgrade_range(
        &mut core_request,
        profile,
        solved.is_somber,
        request.max_upgrade,
    );
    request::set_min_stats(&mut core_request, [0; COMBAT_STAT_COUNT]);
    request::lock_to_stats(&mut core_request, solved.stats);
    request::settle_upgrade_caps(&mut core_request, profile)?;
    let objective = core_request.objective;
    Ok((
        core_request,
        Arc::clone(&profile.data),
        request.max_upgrade.min(weapon_cap),
        objective,
    ))
}

fn evaluate_upgrade_series_with_cancel<F>(
    core_request: &OptimizeRequest,
    data: &GameData,
    max_upgrade: u8,
    objective: OptimizeObjective,
    mut should_continue: F,
) -> Result<Vec<UpgradePointDto>, AppError>
where
    F: FnMut() -> bool + Send,
{
    let evaluator =
        prepare_upgrade_series_evaluator_with_cancel(core_request, data, &mut should_continue)
            .map_err(AppError::from)?;
    Ok(evaluator
        .evaluate_with_cancel(core_request, max_upgrade, &mut should_continue)
        .map_err(AppError::from)?
        .into_iter()
        .map(SolvedBuildDto::from)
        .map(|solved| UpgradePointDto {
            upgrade: solved.upgrade,
            metric: metric_for_objective(&solved, objective),
        })
        .collect())
}

#[cfg(test)]
pub fn build_upgrade_series_inner_with_cancel<F>(
    request: UpgradeSeriesRequestDto,
    state: &AppState,
    mut should_continue: F,
) -> Result<Vec<UpgradePointDto>, AppError>
where
    F: FnMut() -> bool + Send,
{
    let (core_request, data, max_upgrade, objective) = prepare_upgrade_series(&request, state)?;
    evaluate_upgrade_series_with_cancel(
        &core_request,
        &data,
        max_upgrade,
        objective,
        &mut should_continue,
    )
}

#[tauri::command]
pub fn start_upgrade_series(
    request: UpgradeSeriesRequestDto,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    let (core_request, data, max_upgrade, objective) = prepare_upgrade_series(&request, &state)?;
    state.analysis_jobs.start(
        &state.next_job,
        AnalysisJobStatusDto { finished: None },
        move |job_id, cancel_flag, _| {
            let result = evaluate_upgrade_series_with_cancel(
                &core_request,
                &data,
                max_upgrade,
                objective,
                || !cancel_flag.load(Ordering::Relaxed),
            )
            .map_err(|error| error.message);
            let (points, error, cancelled) = split_outcome(result, &cancel_flag);
            AnalysisFinishedDto {
                job_id,
                kind: AnalysisJobKindDto::UpgradeSeries,
                cancelled,
                result: None,
                points: points.unwrap_or_default(),
                frontier: Vec::new(),
                error,
            }
        },
        |status, outcome, job_id, cancelled| {
            publish_analysis_finished(
                status,
                outcome,
                job_id,
                AnalysisJobKindDto::UpgradeSeries,
                cancelled,
            );
        },
    )
}

/// Checks that a client-supplied solved loadout still names this profile's weapon
/// row and an upgrade level that row supports. Returns the highest upgrade the row has.
pub fn validate_solved_loadout(
    profile: &ProfileData,
    solved: &SolvedBuildDto,
) -> Result<u8, AppError> {
    check_len("solved.weaponName", &solved.weapon_name)?;
    check_len("solved.affinity", &solved.affinity)?;
    if solved
        .stats
        .to_array()
        .into_iter()
        .any(|stat| stat > MAX_STAT)
    {
        return Err(AppError::new(format!(
            "solved.stats must be {MAX_STAT} or lower"
        )));
    }
    if let Some(aow_name) = &solved.aow_name {
        check_len("solved.aowName", aow_name)?;
        request::check_aow_known(profile, aow_name)?;
    }
    let weapon = solved_weapon(profile, solved)?;
    match weapon_row_upgrade_cap(&profile.data, weapon) {
        Some(cap)
            if weapon.is_somber == solved.is_somber
                && solved.upgrade <= cap
                && profile
                    .data
                    .reinforce_level(weapon.reinforce_type, solved.upgrade)
                    .is_some() =>
        {
            Ok(cap)
        }
        _ => Err(AppError::new(
            "selected loadout upgrade does not match profile data",
        )),
    }
}

/// The searchable profile row a client-supplied solved build names, found by its ID.
pub fn solved_weapon<'a>(
    profile: &'a ProfileData,
    solved: &SolvedBuildDto,
) -> Result<&'a Weapon, AppError> {
    profile
        .data
        .weapons
        .iter()
        .find(|weapon| weapon.weapon_id == solved.weapon_id)
        .filter(|weapon| {
            weapon.name == solved.weapon_name
                && weapon.affinity == solved.affinity
                && profile.data.weapon_ar_supported(weapon)
        })
        .ok_or_else(|| AppError::new("selected weapon identity does not match profile data"))
}

fn prepare_ar_bleed_frontier(
    request: &ArBleedFrontierRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>), AppError> {
    let profile = state.profile(&request.base.profile_id)?;
    let solved = &request.solved;
    validate_solved_loadout(profile, solved)?;
    let mut base = request::validate(&request.base, profile)?;
    if !uses_class_budget(profile, &base.class_name) || !profile.data.capabilities.status_buildup {
        return Err(AppError::new(
            "AR / bleed tradeoffs require class budgets and status modeling",
        ));
    }
    base.objective = OptimizeObjective::MaxAr;
    request::set_exact_upgrade(&mut base, solved.upgrade, solved.is_somber);
    let mut core_request = pinned_build(
        base,
        profile,
        &solved.weapon_name,
        Some(&solved.affinity),
        solved.aow_name.as_deref(),
    )?;
    core_request.filters.push(StableFilter {
        dimension: FilterDimension::Aow,
        mode: FilterMode::Include,
        id: solved
            .aow_id
            .map_or_else(|| "aow:none".into(), |id| format!("aow:{id}")),
    });
    Ok((core_request, Arc::clone(&profile.data)))
}

fn evaluate_ar_bleed_frontier<F>(
    request: &OptimizeRequest,
    data: &GameData,
    should_continue: F,
) -> Result<Vec<ArBleedFrontierPointDto>, AppError>
where
    F: Fn() -> bool + Sync,
{
    let evaluator = prepare_loadout_evaluator_with_cancel(request, data, &should_continue)
        .map_err(AppError::from)?;
    evaluator
        .evaluate_ar_bleed_frontier_with_cancel(request, &should_continue)
        .map(|points| {
            points
                .into_iter()
                .map(|point| ArBleedFrontierPointDto {
                    result: SolvedBuildDto::from(point.result),
                    ar_loss: point.ar_loss,
                    ar_loss_percent: point.ar_loss_percent,
                    minimum_ar_loss_bps: point.minimum_ar_loss_bps,
                    bleed_gain: point.bleed_gain,
                })
                .collect()
        })
        .map_err(AppError::from)
}

#[tauri::command]
pub fn start_ar_bleed_frontier(
    request: ArBleedFrontierRequestDto,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    let (request, data) = prepare_ar_bleed_frontier(&request, &state)?;
    state.analysis_jobs.start(
        &state.next_job,
        AnalysisJobStatusDto { finished: None },
        move |job_id, cancel_flag, _| {
            let result = evaluate_ar_bleed_frontier(&request, &data, || {
                !cancel_flag.load(Ordering::Relaxed)
            })
            .map_err(|error| error.message);
            let (frontier, error, cancelled) = split_outcome(result, &cancel_flag);
            AnalysisFinishedDto {
                job_id,
                kind: AnalysisJobKindDto::ArBleedFrontier,
                cancelled,
                result: None,
                points: Vec::new(),
                frontier: frontier.unwrap_or_default(),
                error,
            }
        },
        |status, outcome, job_id, cancelled| {
            publish_analysis_finished(
                status,
                outcome,
                job_id,
                AnalysisJobKindDto::ArBleedFrontier,
                cancelled,
            );
        },
    )
}

fn publish_analysis_finished(
    status: &mut AnalysisJobStatusDto,
    outcome: Result<AnalysisFinishedDto, String>,
    job_id: String,
    kind: AnalysisJobKindDto,
    cancelled: bool,
) {
    status.finished = Some(finish(outcome, cancelled, |error| AnalysisFinishedDto {
        job_id,
        kind,
        cancelled: false,
        result: None,
        points: Vec::new(),
        frontier: Vec::new(),
        error: Some(error),
    }));
}

#[tauri::command]
pub fn cancel_analysis(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    cancel_job(&state.analysis_jobs, &job_id)
}

// Terminal payloads can be large; async keeps their clone and serialization off the UI thread.
#[tauri::command(async)]
pub fn get_analysis_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AnalysisJobStatusDto>, AppError> {
    job_status(&state.analysis_jobs, &job_id)
}

fn prepare_search(
    request: &OptimizeRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>), AppError> {
    let profile = state.profile(&request.profile_id)?;
    let mut core_request = request::validate(request, profile)?;
    request::settle_upgrade_caps(&mut core_request, profile)?;
    Ok((core_request, Arc::clone(&profile.data)))
}

#[tauri::command]
pub fn start_search(
    request: OptimizeRequestDto,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    let (core_request, data) = prepare_search(&request, &state)?;
    state.search_jobs.start(
        &state.next_job,
        SearchJobStatusDto {
            progress: None,
            finished: None,
        },
        move |job_id, cancel_flag, status| {
            run_search_worker(&core_request, &data, job_id, &cancel_flag, &status)
        },
        publish_search_finished,
    )
}

fn run_search_worker(
    core_request: &OptimizeRequest,
    data: &GameData,
    job_id: String,
    cancel_flag: &AtomicBool,
    status: &Mutex<SearchJobStatusDto>,
) -> SearchFinishedDto {
    let plan = match prepare_search_with_cancel(core_request, data, || {
        !cancel_flag.load(Ordering::Relaxed)
    }) {
        Ok(plan) => plan,
        Err(message) => {
            return SearchFinishedDto {
                job_id,
                cancelled: message == "cancelled",
                rows: Vec::new(),
                error: (message != "cancelled").then_some(message),
                setups: 0,
            };
        }
    };
    let mut last_progress_at: Option<Instant> = None;
    let mut scoring_complete = false;
    let result = optimize_prepared_with_progress(
        &plan,
        SEARCH_PROGRESS_EVERY,
        || !cancel_flag.load(Ordering::Relaxed),
        |snapshot| {
            // Hydration reports at each row/route boundary without advancing
            // scoring counts. Publish status at a bounded rate.
            let first_scoring_completion = !scoring_complete && snapshot.checked == snapshot.total;
            scoring_complete |= snapshot.checked == snapshot.total;
            if !first_scoring_completion
                && last_progress_at.is_some_and(|at| at.elapsed() < SEARCH_PROGRESS_INTERVAL)
            {
                return;
            }
            last_progress_at = Some(Instant::now());
            let payload = SearchProgressDto::new(job_id.clone(), snapshot);
            if let Ok(mut guard) = status.lock() {
                guard.progress = Some(payload);
            }
        },
    );

    let (rows, error, cancelled) = match result {
        Ok(rows) => (
            rows.into_iter().map(SolvedBuildDto::from).collect(),
            None,
            false,
        ),
        Err(message) if message == "cancelled" => (Vec::new(), None, true),
        Err(message) => (Vec::new(), Some(message), false),
    };
    SearchFinishedDto {
        job_id,
        cancelled,
        rows,
        error,
        setups: plan.estimate().combinations,
    }
}

fn publish_search_finished(
    status: &mut SearchJobStatusDto,
    outcome: Result<SearchFinishedDto, String>,
    job_id: String,
    cancelled: bool,
) {
    status.finished = Some(finish(outcome, cancelled, |error| SearchFinishedDto {
        job_id,
        cancelled: false,
        rows: Vec::new(),
        error: Some(error),
        setups: 0,
    }));
}

#[tauri::command]
pub fn cancel_search(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    cancel_job(&state.search_jobs, &job_id)
}

#[tauri::command(async)]
pub fn get_search_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<SearchJobStatusDto>, AppError> {
    job_status(&state.search_jobs, &job_id)
}
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::AsyncJobHandle;

    fn set_exact_upgrade(request: &mut OptimizeRequestDto, upgrade: u8, is_somber: bool) {
        if is_somber {
            request.somber_max_upgrade = Some(upgrade);
        } else {
            request.standard_max_upgrade = Some(upgrade);
        }
        request.exact_upgrade = Some(true);
    }

    /// What a search runs: the request validated against its profile and fitted to its weapon.
    fn settled(
        request: &OptimizeRequestDto,
        profile: &ProfileData,
    ) -> Result<OptimizeRequest, AppError> {
        let mut core_request = request::validate(request, profile)?;
        request::settle_upgrade_caps(&mut core_request, profile)?;
        Ok(core_request)
    }

    #[test]
    fn focused_exact_upgrade_does_not_substitute_a_lower_level() {
        let state = crate::test_app_state();
        let mut request = crate::test_optimize_request();
        request.character_level = 80;
        request.weapon_name = Some("Meteorite Staff".into());
        request.affinity = Some("Standard".into());
        request.somber_max_upgrade = Some(10);
        let error = settled(&request, state.profile("vanilla").unwrap())
            .expect_err("an exact +10 request must not become +0");
        assert!(error.message.contains("exact +10"));
    }

    #[test]
    fn broad_and_focused_upgrade_constraints_agree_for_both_profiles() {
        let state = crate::test_app_state();
        for (profile_id, weapon_name, exact_upgrade, is_somber) in [
            ("vanilla", "Meteorite Staff", 0, true),
            ("vanilla", "Meteorite Staff", 10, true),
            ("vanilla", "Dagger", 25, false),
            ("vanilla", "Black Knife", 10, true),
            ("convergence", "Dagger", 15, false),
            (
                "convergence",
                "Galvanic Culling Blade [Twinblade]",
                15,
                true,
            ),
        ] {
            let mut request = if profile_id == "vanilla" {
                let mut base = crate::test_optimize_request();
                base.character_level = 80;
                base
            } else {
                convergence_custom_stats_request()
            };
            request.weapon_name = None;
            request.affinity = Some("Standard".into());
            request.aow_name = None;
            set_exact_upgrade(&mut request, exact_upgrade, is_somber);
            let data = &state.profile(profile_id).unwrap().data;
            let weapon = data
                .weapons
                .iter()
                .find(|weapon| weapon.name == weapon_name && weapon.affinity == "Standard")
                .unwrap();
            request.filters.entries = vec![crate::dto::StableFilterEntryDto {
                dimension: "weapon_family".into(),
                id: weapon.family_filter_id(),
                mode: "include".into(),
            }];
            let broad = run_search_inner(request.clone(), &state).unwrap();
            request.weapon_name = Some(weapon_name.into());
            if weapon_name == "Meteorite Staff" && exact_upgrade == 10 {
                assert!(broad.is_empty());
                assert!(
                    run_search_inner(request.clone(), &state)
                        .unwrap_err()
                        .message
                        .contains("exact +10")
                );
                request.exact_upgrade = Some(false);
                assert_eq!(run_search_inner(request, &state).unwrap()[0].upgrade, 0);
            } else {
                let focused = run_search_inner(request, &state).unwrap();
                assert!(!focused.is_empty(), "{profile_id} {weapon_name}");
                assert_eq!(
                    serde_json::to_value(&focused).unwrap(),
                    serde_json::to_value(&broad).unwrap()
                );
                assert_eq!(focused[0].upgrade, exact_upgrade);
            }
        }
    }

    #[test]
    fn exact_profile_caps_and_sparse_levels_reject_without_clamping() {
        let state = crate::test_app_state();
        for profile_id in ["vanilla", "convergence"] {
            let mut request = if profile_id == "vanilla" {
                crate::test_optimize_request()
            } else {
                convergence_custom_stats_request()
            };
            request.weapon_name = None;
            request.standard_max_upgrade = Some(26);
            let profile = state.profile(profile_id).unwrap();
            assert!(
                settled(&request, profile)
                    .unwrap_err()
                    .message
                    .contains("exact standard +26")
            );
            request.exact_upgrade = Some(false);
            assert_eq!(
                settled(&request, profile).unwrap().standard_max_upgrade,
                profile.data.rules.standard_max_upgrade
            );
        }
        let profile = state.profile("vanilla").unwrap();
        let mut data = (*profile.data).clone();
        let weapon = data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
            .unwrap();
        let reinforce_type = weapon.reinforce_type;
        data.reinforce[usize::from(reinforce_type)][5] = None;
        let profile = ProfileData {
            data: Arc::new(data),
            catalog_index: Arc::clone(&profile.catalog_index),
            data_manifest: profile.data_manifest.clone(),
        };
        let mut request = crate::test_optimize_request();
        request.standard_max_upgrade = Some(5);
        assert!(
            settled(&request, &profile)
                .unwrap_err()
                .message
                .contains("exact +5")
        );
    }

    #[test]
    fn supervised_search_worker_recovers_panics_before_and_during_calculation() {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, AtomicUsize};

        let state = crate::test_app_state();
        for during_calculation in [false, true] {
            let registry =
                crate::JobRegistry::new("search", "search", |status: &SearchJobStatusDto| {
                    status.finished.is_some()
                });
            let status = Arc::new(Mutex::new(SearchJobStatusDto {
                progress: None,
                finished: None,
            }));
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
            let core_request = OptimizeRequest::try_from(&crate::test_optimize_request()).unwrap();
            let data = Arc::clone(
                &state
                    .profile(er_optimizer_core::VANILLA_PROFILE_ID)
                    .unwrap()
                    .data,
            );
            let injected = Arc::new(AtomicBool::new(false));
            let worker_injected = Arc::clone(&injected);
            let publications = Arc::new(AtomicUsize::new(0));
            let published = Arc::clone(&publications);
            let publish_cancel = Arc::clone(&cancel);
            let supervisor = crate::spawn_supervised_job(
                Arc::clone(&status),
                move || {
                    if !during_calculation {
                        worker_injected.store(true, Ordering::Relaxed);
                        panic!("injected before Search calculation");
                    }
                    let checkpoints = AtomicUsize::new(0);
                    let rows = optimize_with_cancel(&core_request, &data, || {
                        if checkpoints.fetch_add(1, Ordering::Relaxed) == 2 {
                            worker_injected.store(true, Ordering::Relaxed);
                            panic!("injected inside Search calculation");
                        }
                        true
                    })
                    .unwrap();
                    SearchFinishedDto {
                        job_id: "first".into(),
                        cancelled: false,
                        rows: rows.into_iter().map(SolvedBuildDto::from).collect(),
                        error: None,
                        setups: 0,
                    }
                },
                move |status, outcome| {
                    published.fetch_add(1, Ordering::Relaxed);
                    publish_search_finished(
                        status,
                        outcome,
                        "first".into(),
                        publish_cancel.load(Ordering::Relaxed),
                    );
                },
            );
            tauri::async_runtime::block_on(supervisor).unwrap();
            assert!(injected.load(Ordering::Relaxed));
            assert_eq!(publications.load(Ordering::Relaxed), 1);
            let terminal = status.lock().unwrap();
            let finished = terminal.finished.as_ref().unwrap();
            assert!(finished.error.is_some());
            assert!(!finished.cancelled);
            assert!(finished.rows.is_empty());
            drop(terminal);
            registry
                .insert_if_idle(
                    "restart".into(),
                    AsyncJobHandle {
                        cancel,
                        status: Arc::new(Mutex::new(SearchJobStatusDto {
                            progress: None,
                            finished: None,
                        })),
                    },
                )
                .expect("joined failed Search job permits restart");
        }
    }

    #[test]
    fn supervised_analysis_workers_recover_panics_before_and_during_calculation() {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, AtomicUsize};

        let state = crate::test_app_state();
        let base = crate::test_optimize_request();
        let solved = run_search_inner(base.clone(), &state)
            .unwrap()
            .pop()
            .unwrap();
        for kind in [
            AnalysisJobKindDto::SolveBuild,
            AnalysisJobKindDto::UpgradeSeries,
            AnalysisJobKindDto::ArBleedFrontier,
        ] {
            for during_calculation in [false, true] {
                let (core_request, data) = match kind {
                    AnalysisJobKindDto::SolveBuild => prepare_solve_build(
                        &SolveBuildRequestDto {
                            base: base.clone(),
                            weapon_name: solved.weapon_name.clone(),
                            affinity: Some(solved.affinity.clone()),
                            aow_name: solved.aow_name.clone(),
                        },
                        &state,
                    )
                    .unwrap(),
                    AnalysisJobKindDto::UpgradeSeries => {
                        let (request, data, _, _) = prepare_upgrade_series(
                            &UpgradeSeriesRequestDto {
                                base: base.clone(),
                                solved: solved.clone(),
                                max_upgrade: 0,
                            },
                            &state,
                        )
                        .unwrap();
                        (request, data)
                    }
                    AnalysisJobKindDto::ArBleedFrontier => prepare_ar_bleed_frontier(
                        &ArBleedFrontierRequestDto {
                            base: base.clone(),
                            solved: solved.clone(),
                        },
                        &state,
                    )
                    .unwrap(),
                };
                let registry = crate::JobRegistry::new(
                    "analysis",
                    "analysis",
                    |status: &AnalysisJobStatusDto| status.finished.is_some(),
                );
                let status = Arc::new(Mutex::new(AnalysisJobStatusDto { finished: None }));
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
                let injected = Arc::new(AtomicBool::new(false));
                let worker_injected = Arc::clone(&injected);
                let publications = Arc::new(AtomicUsize::new(0));
                let published = Arc::clone(&publications);
                let publish_cancel = Arc::clone(&cancel);
                let supervisor = crate::spawn_supervised_job(
                    Arc::clone(&status),
                    move || {
                        if !during_calculation {
                            worker_injected.store(true, Ordering::Relaxed);
                            panic!("injected before Analysis calculation");
                        }
                        let checkpoints = AtomicUsize::new(0);
                        let checkpoint = || {
                            if checkpoints.fetch_add(1, Ordering::Relaxed) == 2 {
                                worker_injected.store(true, Ordering::Relaxed);
                                panic!("injected inside Analysis calculation");
                            }
                            true
                        };
                        let mut finished = AnalysisFinishedDto {
                            job_id: "first".into(),
                            kind,
                            cancelled: false,
                            result: None,
                            points: Vec::new(),
                            frontier: Vec::new(),
                            error: None,
                        };
                        match kind {
                            AnalysisJobKindDto::SolveBuild => {
                                finished.result =
                                    optimize_with_cancel(&core_request, &data, checkpoint)
                                        .unwrap()
                                        .pop()
                                        .map(SolvedBuildDto::from)
                            }
                            AnalysisJobKindDto::UpgradeSeries => {
                                finished.points = evaluate_upgrade_series_with_cancel(
                                    &core_request,
                                    &data,
                                    0,
                                    core_request.objective,
                                    checkpoint,
                                )
                                .unwrap()
                            }
                            AnalysisJobKindDto::ArBleedFrontier => {
                                finished.frontier =
                                    evaluate_ar_bleed_frontier(&core_request, &data, checkpoint)
                                        .unwrap()
                            }
                        }
                        finished
                    },
                    move |status, outcome| {
                        published.fetch_add(1, Ordering::Relaxed);
                        publish_analysis_finished(
                            status,
                            outcome,
                            "first".into(),
                            kind,
                            publish_cancel.load(Ordering::Relaxed),
                        );
                    },
                );
                tauri::async_runtime::block_on(supervisor).unwrap();
                assert!(
                    injected.load(Ordering::Relaxed),
                    "{kind:?} must reach injected panic"
                );
                assert_eq!(publications.load(Ordering::Relaxed), 1);
                let terminal = status.lock().unwrap();
                let finished = terminal.finished.as_ref().unwrap();
                assert_eq!(finished.kind, kind);
                assert!(finished.error.is_some());
                assert!(!finished.cancelled);
                assert!(finished.result.is_none());
                assert!(finished.points.is_empty());
                assert!(finished.frontier.is_empty());
                drop(terminal);
                registry
                    .insert_if_idle(
                        "restart".into(),
                        AsyncJobHandle {
                            cancel,
                            status: Arc::new(Mutex::new(AnalysisJobStatusDto { finished: None })),
                        },
                    )
                    .expect("joined failed Analysis job permits restart");
            }
        }
    }

    #[test]
    fn search_publisher_cancels_completed_rows_and_preserves_errors() {
        let state = crate::test_app_state();
        let rows = run_search_inner(crate::test_optimize_request(), &state).unwrap();
        assert!(!rows.is_empty());
        let mut status = SearchJobStatusDto {
            progress: None,
            finished: None,
        };
        publish_search_finished(
            &mut status,
            Ok(SearchFinishedDto {
                job_id: "search-publish".into(),
                cancelled: false,
                rows,
                error: None,
                setups: 0,
            }),
            "search-publish".into(),
            true,
        );
        let finished = status.finished.take().unwrap();
        assert!(finished.cancelled);
        assert!(finished.rows.is_empty());
        assert!(finished.error.is_none());

        for outcome in [
            Ok(SearchFinishedDto {
                job_id: "search-publish".into(),
                cancelled: false,
                rows: Vec::new(),
                error: Some("calculation failed".into()),
                setups: 0,
            }),
            Err(String::from("worker panicked")),
        ] {
            let expected_error = match &outcome {
                Ok(finished) => finished.error.as_deref().unwrap(),
                Err(error) => error.as_str(),
            }
            .to_string();
            publish_search_finished(&mut status, outcome, "search-publish".into(), true);
            let finished = status.finished.take().unwrap();
            assert_eq!(finished.error.as_deref(), Some(expected_error.as_str()));
            assert!(!finished.cancelled);
            assert!(finished.rows.is_empty());
        }
    }

    #[test]
    fn analysis_publisher_cancels_every_payload_kind_and_preserves_errors() {
        let state = crate::test_app_state();
        let solved = run_search_inner(crate::test_optimize_request(), &state)
            .unwrap()
            .pop()
            .unwrap();
        let mut status = AnalysisJobStatusDto { finished: None };
        for kind in [
            AnalysisJobKindDto::SolveBuild,
            AnalysisJobKindDto::UpgradeSeries,
            AnalysisJobKindDto::ArBleedFrontier,
        ] {
            let mut completed = AnalysisFinishedDto {
                job_id: "analysis-publish".into(),
                kind,
                cancelled: false,
                result: None,
                points: Vec::new(),
                frontier: Vec::new(),
                error: None,
            };
            match kind {
                AnalysisJobKindDto::SolveBuild => completed.result = Some(solved.clone()),
                AnalysisJobKindDto::UpgradeSeries => completed.points.push(UpgradePointDto {
                    upgrade: solved.upgrade,
                    metric: solved.score,
                }),
                AnalysisJobKindDto::ArBleedFrontier => {
                    completed.frontier.push(ArBleedFrontierPointDto {
                        result: solved.clone(),
                        ar_loss: 0.0,
                        ar_loss_percent: 0.0,
                        minimum_ar_loss_bps: 0,
                        bleed_gain: 0.0,
                    })
                }
            }
            publish_analysis_finished(
                &mut status,
                Ok(completed),
                "analysis-publish".into(),
                kind,
                true,
            );
            let finished = status.finished.take().unwrap();
            assert_eq!(finished.kind, kind);
            assert!(finished.cancelled);
            assert!(finished.result.is_none());
            assert!(finished.points.is_empty());
            assert!(finished.frontier.is_empty());
            assert!(finished.error.is_none());

            for outcome in [
                Ok(AnalysisFinishedDto {
                    job_id: "analysis-publish".into(),
                    kind,
                    cancelled: false,
                    result: None,
                    points: Vec::new(),
                    frontier: Vec::new(),
                    error: Some("calculation failed".into()),
                }),
                Err(String::from("worker panicked")),
            ] {
                let expected_error = match &outcome {
                    Ok(finished) => finished.error.as_deref().unwrap(),
                    Err(error) => error.as_str(),
                }
                .to_string();
                publish_analysis_finished(
                    &mut status,
                    outcome,
                    "analysis-publish".into(),
                    kind,
                    true,
                );
                let finished = status.finished.take().unwrap();
                assert_eq!(finished.kind, kind);
                assert_eq!(finished.error.as_deref(), Some(expected_error.as_str()));
                assert!(!finished.cancelled);
                assert!(finished.result.is_none());
                assert!(finished.points.is_empty());
                assert!(finished.frontier.is_empty());
            }
        }
    }

    #[test]
    fn solving_a_saved_loadout_preserves_requested_stat_locks() {
        let state = crate::test_app_state();
        let mut base = crate::test_optimize_request();
        base.character_level = 47;
        base.standard_max_upgrade = Some(25);
        base.exact_upgrade = Some(true);
        base.lock_str = Some(50);
        base.lock_dex = Some(15);
        base.lock_int = Some(9);
        base.lock_fai = Some(8);
        base.lock_arc = Some(8);
        let expected = run_search_inner(base.clone(), &state)
            .unwrap()
            .pop()
            .unwrap();
        let solved = solve_build_inner(
            SolveBuildRequestDto {
                base,
                weapon_name: "Uchigatana".to_string(),
                affinity: Some("Keen".to_string()),
                aow_name: None,
            },
            &state,
        )
        .unwrap()
        .unwrap();
        assert_eq!(solved.stats.str_stat, 50);
        assert_eq!(solved.stats.dex, 15);
        assert_eq!(solved.ar.total, expected.ar.total);
    }

    fn upgrade_series_request(state: &AppState) -> UpgradeSeriesRequestDto {
        let mut base = crate::test_optimize_request();
        base.standard_max_upgrade = Some(25);
        base.exact_upgrade = Some(true);
        let solved = run_search_inner(base.clone(), state)
            .expect("seed search succeeds")
            .pop()
            .expect("seed build exists");
        UpgradeSeriesRequestDto {
            base,
            solved,
            max_upgrade: 25,
        }
    }

    fn convergence_custom_stats_request() -> crate::dto::OptimizeRequestDto {
        let mut request = crate::test_optimize_request();
        request.profile_id = er_optimizer_core::CONVERGENCE_PROFILE_ID.to_string();
        request.class_name = er_optimizer_core::CUSTOM_STATS_CLASS_NAME.to_string();
        request.vig = 20;
        request.mnd = 20;
        request.end = 20;
        request.str_stat = 40;
        request.dex = 40;
        request.int_stat = 40;
        request.fai = 20;
        request.arc = 20;
        request.character_level = 220;
        request.lock_str = Some(request.str_stat);
        request.lock_dex = Some(request.dex);
        request.lock_int = Some(request.int_stat);
        request.lock_fai = Some(request.fai);
        request.lock_arc = Some(request.arc);
        request.standard_max_upgrade = Some(15);
        request.somber_max_upgrade = Some(15);
        request.exact_upgrade = Some(true);
        request.weapon_name = Some("Galvanic Culling Blade [Twinblade]".to_string());
        request.affinity = Some("Standard".to_string());
        request
    }

    #[test]
    fn packaged_snapshot_executes_a_real_tauri_search() {
        let state = crate::test_app_state();
        let rows = run_search_inner(crate::test_optimize_request(), &state)
            .expect("real command search succeeds");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].weapon_name, "Uchigatana");
        assert_eq!(rows[0].affinity, "Keen");
        assert_eq!(rows[0].upgrade, 0);
    }

    #[test]
    fn level_93_type_search_returns_distinct_greatswords() {
        let state = crate::test_app_state();
        let mut request = crate::test_optimize_request();
        request.character_level = 93;
        request.weapon_name = None;
        request.affinity = None;
        request.weapon_type_key = Some("Greatsword".to_string());
        request.standard_max_upgrade = Some(25);
        request.somber_max_upgrade = Some(10);
        request.result_grouping = "weapon".to_string();
        request.top_k = 2;

        let rows = run_search_inner(request, &state).expect("type search succeeds");
        assert!(rows.len() >= 2);
        assert!(
            rows.iter()
                .skip(1)
                .any(|row| row.weapon_name != rows[0].weapon_name)
        );
    }

    #[test]
    fn search_worker_publishes_completed_progress_and_honors_cancellation() {
        let state = crate::test_app_state();
        let (request, data) = prepare_search(&crate::test_optimize_request(), &state).unwrap();
        let status = Mutex::new(SearchJobStatusDto {
            progress: None,
            finished: None,
        });
        let finished = run_search_worker(
            &request,
            &data,
            "search-1".into(),
            &AtomicBool::new(false),
            &status,
        );
        assert!(!finished.cancelled && finished.error.is_none());
        assert_eq!(finished.rows.len(), 1);
        assert!(finished.setups > 0);
        // The first snapshot with all setups scored is always published.
        let progress = status.lock().unwrap().progress.take().unwrap();
        assert_eq!(progress.job_id, "search-1");
        assert_eq!(progress.checked, progress.total);

        let cancelled = run_search_worker(
            &request,
            &data,
            "search-2".into(),
            &AtomicBool::new(true),
            &status,
        );
        assert!(cancelled.cancelled);
        assert!(cancelled.rows.is_empty() && cancelled.error.is_none());
    }

    #[test]
    fn real_solve_build_honors_cancellation() {
        let state = crate::test_app_state();
        let request = SolveBuildRequestDto {
            base: crate::test_optimize_request(),
            weapon_name: "Uchigatana".to_string(),
            affinity: Some("Keen".to_string()),
            aow_name: None,
        };
        let error = solve_build_inner_with_cancel(request, &state, || false)
            .expect_err("cancelled solve-build must fail closed");
        assert_eq!(error.message, "cancelled");
    }

    #[test]
    fn frontier_preserves_loadout_context_locks_and_cancellation() {
        let state = crate::test_app_state();
        let mut base = crate::test_optimize_request();
        base.character_level = 80;
        base.affinity = Some("Blood".into());
        base.aow_name = Some("Seppuku".into());
        base.standard_max_upgrade = Some(25);
        base.lock_str = Some(15);
        let solved = run_search_inner(base.clone(), &state)
            .unwrap()
            .pop()
            .unwrap();
        let (request, data) = prepare_ar_bleed_frontier(
            &ArBleedFrontierRequestDto {
                base: base.clone(),
                solved: solved.clone(),
            },
            &state,
        )
        .unwrap();
        let points = evaluate_ar_bleed_frontier(&request, &data, || true).unwrap();
        assert!(!points.is_empty());
        for point in &points {
            assert_eq!(point.result.weapon_id, solved.weapon_id);
            assert_eq!(point.result.affinity, solved.affinity);
            assert_eq!(point.result.aow_name, solved.aow_name);
            assert_eq!(point.result.upgrade, solved.upgrade);
            assert_eq!(point.result.stats.str_stat, 15);
        }
        assert_eq!(points[0].minimum_ar_loss_bps, 0);
        assert_eq!(
            evaluate_ar_bleed_frontier(&request, &data, || false)
                .unwrap_err()
                .message,
            "cancelled"
        );
        base.profile_id = "convergence".into();
        assert!(
            prepare_ar_bleed_frontier(&ArBleedFrontierRequestDto { base, solved }, &state).is_err()
        );
    }

    #[test]
    fn frontier_validates_selected_identity_and_native_skill() {
        let state = crate::test_app_state();
        let mut base = crate::test_optimize_request();
        base.weapon_name = Some("Dagger".into());
        base.affinity = Some("Standard".into());
        base.aow_name = Some("Quickstep".into());
        let solved = run_search_inner(base.clone(), &state)
            .unwrap()
            .pop()
            .unwrap();
        let prepare = |solved| {
            prepare_ar_bleed_frontier(
                &ArBleedFrontierRequestDto {
                    base: base.clone(),
                    solved,
                },
                &state,
            )
        };
        let (request, data) = prepare(solved.clone()).unwrap();
        let points = evaluate_ar_bleed_frontier(&request, &data, || true).unwrap();
        assert!(!points.is_empty());
        assert!(
            points
                .iter()
                .all(|point| point.result.aow_id == solved.aow_id)
        );
        let mut invalid = solved.clone();
        invalid.weapon_id = u32::MAX;
        assert!(prepare(invalid).is_err());
        let mut invalid = solved.clone();
        invalid.upgrade = 26;
        assert!(prepare(invalid).is_err());
        let mut invalid = solved;
        invalid.aow_id = Some(9999);
        let (request, data) = prepare(invalid).unwrap();
        assert!(evaluate_ar_bleed_frontier(&request, &data, || true).is_err());

        base.weapon_name = Some("Meteorite Staff".into());
        base.aow_name = None;
        base.exact_upgrade = Some(false);
        base.character_level = 80;
        let mut solved = run_search_inner(base.clone(), &state)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(solved.upgrade, 0);
        solved.upgrade = 1;
        assert!(
            prepare_ar_bleed_frontier(&ArBleedFrontierRequestDto { base, solved }, &state).is_err()
        );
    }

    #[test]
    fn direct_analysis_jobs_share_one_slot() {
        let state = crate::test_app_state();
        let (release, released) = std::sync::mpsc::sync_channel::<()>(1);
        let start = |worker: Box<dyn FnOnce() + Send>| {
            state.analysis_jobs.start(
                &state.next_job,
                AnalysisJobStatusDto { finished: None },
                move |_, _, _| worker(),
                |status, _, job_id, cancelled| {
                    status.finished = Some(AnalysisFinishedDto {
                        job_id,
                        kind: AnalysisJobKindDto::SolveBuild,
                        cancelled,
                        result: None,
                        points: Vec::new(),
                        frontier: Vec::new(),
                        error: None,
                    });
                },
            )
        };
        let first = start(Box::new(move || released.recv().unwrap())).expect("first job starts");
        let error = start(Box::new(|| {})).expect_err("second direct analysis job is rejected");
        assert!(error.message.contains("analysis job is already running"));
        assert!(state.analysis_jobs.cancel(&first.job_id).unwrap());
        release.send(()).unwrap();
    }

    #[test]
    fn packaged_snapshot_executes_direct_upgrade_series_and_cancellation() {
        let state = crate::test_app_state();
        let request = upgrade_series_request(&state);
        let points = build_upgrade_series_inner(request.clone(), &state)
            .expect("direct upgrade series succeeds");
        assert_eq!(
            points.iter().map(|point| point.upgrade).collect::<Vec<_>>(),
            (0_u8..=25).collect::<Vec<_>>()
        );
        let error = build_upgrade_series_inner_with_cancel(request, &state, || false)
            .expect_err("cancelled upgrade series must fail closed");
        assert_eq!(error.message, "cancelled");
    }

    #[test]
    fn convergence_unique_somber_upgrade_series_reaches_plus_fifteen() {
        let state = crate::test_app_state();
        let base = convergence_custom_stats_request();
        let solved = run_search_inner(base.clone(), &state)
            .expect("Convergence fixed-stat seed search succeeds")
            .pop()
            .expect("Convergence unique weapon seed exists");
        assert!(solved.is_somber);
        assert_eq!(solved.upgrade, 15);

        let points = build_upgrade_series_inner(
            UpgradeSeriesRequestDto {
                base,
                solved,
                max_upgrade: 25,
            },
            &state,
        )
        .expect("Convergence upgrade series succeeds");
        assert_eq!(
            points.iter().map(|point| point.upgrade).collect::<Vec<_>>(),
            (0_u8..=15).collect::<Vec<_>>()
        );
    }

    #[test]
    fn vanilla_somber_request_keeps_the_ten_level_cap() {
        let state = crate::test_app_state();
        let mut request = crate::test_optimize_request();
        request.weapon_name = Some("Black Knife".to_string());
        request.affinity = Some("Standard".to_string());
        request.standard_max_upgrade = Some(25);
        request.somber_max_upgrade = Some(25);
        request.exact_upgrade = Some(false);

        let request = settled(&request, state.profile("vanilla").unwrap())
            .expect("Vanilla unique weapon request should clamp");
        assert_eq!(request.standard_max_upgrade, 25);
        assert_eq!(request.somber_max_upgrade, 10);
    }

    #[test]
    fn selected_loadout_uses_explicit_zero_upgrade_and_series_range() {
        let state = crate::test_app_state();
        let mut base = crate::test_optimize_request();
        base.character_level = 80;
        base.standard_max_upgrade = Some(25);
        base.somber_max_upgrade = Some(0);
        let solved = solve_build_inner(
            SolveBuildRequestDto {
                base: base.clone(),
                weapon_name: "Meteorite Staff".into(),
                affinity: Some("Standard".into()),
                aow_name: None,
            },
            &state,
        )
        .unwrap()
        .expect("an unupgradeable pinned weapon remains available at +0");
        assert_eq!(solved.weapon_name, "Meteorite Staff");
        assert_eq!(solved.upgrade, 0);
        let points = build_upgrade_series_inner(
            UpgradeSeriesRequestDto {
                base,
                solved: solved.clone(),
                max_upgrade: 10,
            },
            &state,
        )
        .unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].upgrade, 0);
        assert_eq!(points[0].metric, solved.score);
    }

    #[test]
    fn convergence_custom_stats_boundary_requires_exact_level_and_locks() {
        let state = crate::test_app_state();

        let mut wrong_level = convergence_custom_stats_request();
        wrong_level.character_level += 1;
        let convergence = state.profile("convergence").unwrap();
        let error = settled(&wrong_level, convergence)
            .expect_err("custom stats with an inconsistent level must be rejected");
        assert!(error.message.contains("requires character level"));

        let mut missing_lock = convergence_custom_stats_request();
        missing_lock.lock_arc = None;
        let error = settled(&missing_lock, convergence)
            .expect_err("custom stats without all combat locks must be rejected");
        assert!(error.message.contains("arc to be locked"));
    }

    #[test]
    fn convergence_offers_its_classes_and_custom_stats_but_no_vanilla_class() {
        let state = crate::test_app_state();
        let convergence = state.profile("convergence").unwrap();
        let names = crate::commands::data::class_metadata(convergence)
            .into_iter()
            .map(|class_info| class_info.name)
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 30);
        assert_eq!(names.first().map(String::as_str), Some("Glint Sorcerer"));
        assert_eq!(
            names.last().map(String::as_str),
            Some(er_optimizer_core::CUSTOM_STATS_CLASS_NAME)
        );
        let vanilla = state.profile("vanilla").unwrap();
        assert!(
            !crate::commands::data::class_metadata(vanilla)
                .iter()
                .any(|class_info| class_info.name == er_optimizer_core::CUSTOM_STATS_CLASS_NAME)
        );

        // A Convergence class budgets stats like a Vanilla class and needs no locks.
        let mut prisoner = convergence_custom_stats_request();
        prisoner.class_name = "Prisoner".to_string();
        prisoner.character_level = 1 + 220 - 80;
        (prisoner.lock_str, prisoner.lock_dex, prisoner.lock_int) = (None, None, None);
        (prisoner.lock_fai, prisoner.lock_arc) = (None, None);
        settled(&prisoner, convergence).expect("Convergence class");
        let mut samurai = prisoner.clone();
        samurai.class_name = "Samurai".to_string();
        let error = settled(&samurai, convergence)
            .expect_err("Vanilla classes must not budget Convergence");
        assert!(
            error
                .message
                .contains("is not available for profile 'convergence'")
        );
    }

    #[test]
    fn stored_convergence_custom_stats_build_loads_and_solves_exactly() {
        let state = crate::test_app_state();
        let base = convergence_custom_stats_request();
        let ranked = run_search_inner(base.clone(), &state)
            .expect("Custom stats search succeeds")
            .pop()
            .expect("Custom stats result");
        // A saved build re-solves its exact loadout with the stats it was saved with.
        let solved = solve_build_inner(
            SolveBuildRequestDto {
                base: base.clone(),
                weapon_name: ranked.weapon_name.clone(),
                affinity: Some(ranked.affinity.clone()),
                aow_name: ranked.aow_name.clone(),
            },
            &state,
        )
        .expect("Custom stats solve succeeds")
        .expect("Custom stats build solves");
        assert_eq!(solved.stats, ranked.stats);
        assert_eq!(
            serde_json::to_value(solved.ar).unwrap(),
            serde_json::to_value(ranked.ar).unwrap()
        );
        assert_eq!(
            [
                solved.stats.str_stat,
                solved.stats.dex,
                solved.stats.int_stat,
                solved.stats.fai,
                solved.stats.arc
            ],
            [base.str_stat, base.dex, base.int_stat, base.fai, base.arc]
        );
    }

    #[test]
    #[ignore = "release-mode workflow benchmark"]
    fn workflow_benchmark_upgrade_series() {
        let state = crate::test_app_state();
        let request = upgrade_series_request(&state);
        let repeats = std::env::var("ER_BENCH_REPEATS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(25)
            .max(1);
        let mut durations = Vec::with_capacity(repeats);
        for sample in 0..=repeats {
            let started = std::time::Instant::now();
            let points = build_upgrade_series_inner(request.clone(), &state)
                .expect("benchmark upgrade series succeeds");
            assert_eq!(points.len(), 26);
            if sample > 0 {
                durations.push(started.elapsed().as_secs_f64() * 1_000.0);
            }
        }
        durations.sort_by(f64::total_cmp);
        println!(
            "WORKFLOW_BENCH {}",
            serde_json::json!({
                "workflow": "upgrade_series",
                "model_version": state.profile("vanilla").unwrap().data.model_version,
                "reinforcement": "standard",
                "points": 26,
                "repeats": repeats,
                "median_ms": durations[durations.len() / 2],
                "best_ms": durations[0],
                "worst_ms": durations[durations.len() - 1],
                "samples_ms": durations,
            })
        );
    }

    /// Command-path latency without IPC: DTO normalization, preparation, the job worker and
    /// JSON encoding of the terminal status. Not matched by the `workflow_benchmark` runner.
    #[test]
    #[ignore = "release-mode native command benchmark"]
    fn native_command_benchmark() {
        let state = crate::test_app_state();
        let repeats = std::env::var("ER_BENCH_REPEATS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(5)
            .max(1);
        let selected = std::env::var("ER_BENCH_CASE").ok();
        let samurai = |level: u16| {
            let mut request = crate::test_optimize_request();
            request.character_level = level;
            request.weapon_name = None;
            request.affinity = None;
            request.standard_max_upgrade = Some(25);
            request.somber_max_upgrade = Some(10);
            request
        };
        let mut open_46 = samurai(46);
        open_46.top_k = 25;
        let mut high_k = samurai(46);
        high_k.top_k = 500;
        high_k.result_grouping = "loadout".to_string();
        let mut all_upgrades_93 = samurai(93);
        all_upgrades_93.exact_upgrade = Some(false);
        all_upgrades_93.top_k = 25;
        let mut bleed_93 = samurai(93);
        bleed_93.objective = "max_ar_plus_bleed".to_string();
        bleed_93.top_k = 25;
        let mut aow_150 = samurai(150);
        aow_150.vig = 40;
        aow_150.mnd = 20;
        aow_150.end = 20;
        aow_150.objective = "aow_full_sequence".to_string();
        aow_150.top_k = 25;
        let mut custom = convergence_custom_stats_request();
        custom.weapon_name = None;
        custom.affinity = None;
        custom.top_k = 25;
        let searches = [
            ("search-open-46-k25", open_46),
            ("search-open-46-loadout-k500", high_k),
            ("search-all-upgrades-93-k25", all_upgrades_93),
            ("search-bleed-93-k25", bleed_93),
            ("search-aow-full-150-k25", aow_150),
            ("search-convergence-custom-k25", custom),
        ];
        let solve = |base: crate::dto::OptimizeRequestDto, weapon: &str, affinity: &str, ash| {
            SolveBuildRequestDto {
                base,
                weapon_name: weapon.to_string(),
                affinity: Some(affinity.to_string()),
                aow_name: Some(ash),
            }
        };
        let mut solve_base = samurai(9);
        solve_base.exact_upgrade = Some(false);
        let mut lifesteal_base = samurai(80);
        lifesteal_base.str_stat = 20;
        lifesteal_base.lock_str = Some(20);
        lifesteal_base.lock_int = Some(9);
        lifesteal_base.lock_fai = Some(8);
        lifesteal_base.objective = "aow_full_sequence".to_string();
        let solves = [
            (
                "solve-uchigatana-blood-all-upgrades",
                solve(solve_base, "Uchigatana", "Blood", "Seppuku".to_string()),
            ),
            (
                "solve-locked-lifesteal-fist",
                solve(
                    lifesteal_base,
                    "Caestus",
                    "Occult",
                    "Lifesteal Fist".to_string(),
                ),
            ),
        ];
        let mut frontier_base = samurai(93);
        frontier_base.objective = "max_ar".to_string();
        let frontier_solved = run_search_inner(
            {
                let mut request = frontier_base.clone();
                request.weapon_name = Some("Uchigatana".to_string());
                request.affinity = Some("Blood".to_string());
                request.aow_name = Some("Seppuku".to_string());
                request
            },
            &state,
        )
        .unwrap()
        .pop()
        .unwrap();

        let run = |name: &str, work: &mut dyn FnMut() -> Vec<u8>| {
            if selected.as_deref().is_some_and(|case| case != name) {
                return;
            }
            let mut samples = Vec::with_capacity(repeats);
            let mut expected: Option<Vec<u8>> = None;
            for sample in 0..=repeats {
                let started = std::time::Instant::now();
                let encoded = work();
                let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                if let Some(expected) = &expected {
                    assert_eq!(&encoded, expected, "{name} changed across samples");
                } else {
                    expected = Some(encoded);
                }
                if sample > 0 {
                    samples.push(elapsed);
                }
            }
            let mut sorted = samples.clone();
            sorted.sort_by(f64::total_cmp);
            let encoded = expected.expect("warmup result");
            println!(
                "NATIVE_COMMAND_BENCH {}",
                serde_json::json!({
                    "case": name,
                    "rayon_threads": std::env::var("RAYON_NUM_THREADS").ok(),
                    "warmups": 1,
                    "median_ms": sorted[sorted.len() / 2],
                    "best_ms": sorted[0],
                    "worst_ms": sorted[sorted.len() - 1],
                    "samples_ms": samples,
                    "serialized_bytes": encoded.len(),
                    "result": String::from_utf8(encoded).unwrap(),
                })
            );
        };
        for (name, request) in searches {
            run(name, &mut || {
                let (request, data) = prepare_search(&request, &state).unwrap();
                let status = Mutex::new(SearchJobStatusDto {
                    progress: None,
                    finished: None,
                });
                let finished = run_search_worker(
                    &request,
                    &data,
                    String::new(),
                    &AtomicBool::new(false),
                    &status,
                );
                assert!(finished.error.is_none(), "{:?}", finished.error);
                serde_json::to_vec(&SearchJobStatusDto {
                    progress: None,
                    finished: Some(finished),
                })
                .unwrap()
            });
        }
        for (name, request) in solves {
            run(name, &mut || {
                serde_json::to_vec(&solve_build_inner(request.clone(), &state).unwrap()).unwrap()
            });
        }
        run("frontier-uchigatana-blood-93", &mut || {
            let (request, data) = prepare_ar_bleed_frontier(
                &ArBleedFrontierRequestDto {
                    base: frontier_base.clone(),
                    solved: frontier_solved.clone(),
                },
                &state,
            )
            .unwrap();
            serde_json::to_vec(&evaluate_ar_bleed_frontier(&request, &data, || true).unwrap())
                .unwrap()
        });
    }
}

#[cfg(test)]
mod hostile_request_tests {
    use super::*;
    use crate::commands::optimize::run_search_inner;
    use crate::dto::CombatStateDto;
    use crate::{assert_hostile_values_rejected, assert_rejected_quickly, oversized_text};

    fn seed(state: &AppState) -> SolvedBuildDto {
        run_search_inner(crate::test_optimize_request(), state)
            .expect("seed search succeeds")
            .pop()
            .expect("seed build exists")
    }

    #[test]
    fn search_rejects_every_hostile_value() {
        let state = crate::test_app_state();
        assert_hostile_values_rejected("start_search", &[], |request| {
            run_search_inner(request, &state).map(|_| ())
        });
    }

    #[test]
    fn solve_build_rejects_every_hostile_value() {
        let state = crate::test_app_state();
        let solve = |base, weapon_name: &str, affinity: Option<String>, aow_name| {
            solve_build_inner(
                SolveBuildRequestDto {
                    base,
                    weapon_name: weapon_name.to_string(),
                    affinity,
                    aow_name,
                },
                &state,
            )
            .map(|_| ())
        };
        assert_hostile_values_rejected("start_solve_build", &[], |base| {
            solve(base, "Uchigatana", Some("Keen".into()), None)
        });
        let base = crate::test_optimize_request;
        assert_rejected_quickly("solve weaponName", || {
            solve(base(), &oversized_text(), Some("Keen".into()), None)
        });
        assert_rejected_quickly("solve affinity", || {
            solve(base(), "Uchigatana", Some(oversized_text()), None)
        });
        assert_rejected_quickly("solve aowName", || {
            solve(
                base(),
                "Uchigatana",
                Some("Keen".into()),
                Some(oversized_text()),
            )
        });
    }

    #[test]
    fn upgrade_series_rejects_every_hostile_value() {
        let state = crate::test_app_state();
        let solved = seed(&state);
        let series = |base, solved, max_upgrade| {
            build_upgrade_series_inner(
                UpgradeSeriesRequestDto {
                    base,
                    solved,
                    max_upgrade,
                },
                &state,
            )
            .map(|_| ())
        };
        assert_hostile_values_rejected("start_upgrade_series", &[], |base| {
            series(base, solved.clone(), 25)
        });
        let hostile_stats: [fn(&mut CombatStateDto); 5] = [
            |stats| stats.str_stat = u8::MAX,
            |stats| stats.dex = u8::MAX,
            |stats| stats.int_stat = u8::MAX,
            |stats| stats.fai = u8::MAX,
            |stats| stats.arc = u8::MAX,
        ];
        for (index, hostile_stat) in hostile_stats.into_iter().enumerate() {
            let mut hostile = solved.clone();
            hostile_stat(&mut hostile.stats);
            assert_rejected_quickly(&format!("series solved.stats[{index}]"), || {
                series(crate::test_optimize_request(), hostile, 25)
            });
        }
        let mut hostile = solved.clone();
        hostile.weapon_name = oversized_text();
        assert_rejected_quickly("series solved.weaponName", || {
            series(crate::test_optimize_request(), hostile, 25)
        });
    }

    #[test]
    fn upgrade_series_clamps_an_oversized_max_upgrade_to_the_weapon() {
        let state = crate::test_app_state();
        let solved = seed(&state);
        let points = build_upgrade_series_inner(
            UpgradeSeriesRequestDto {
                base: crate::test_optimize_request(),
                solved,
                max_upgrade: u8::MAX,
            },
            &state,
        )
        .expect("a maximum above the weapon's cap is clamped");
        assert_eq!(points.len(), 26);
        assert_eq!(points.last().map(|point| point.upgrade), Some(25));
    }

    #[test]
    fn ar_bleed_frontier_rejects_every_hostile_value() {
        let state = crate::test_app_state();
        let solved = seed(&state);
        let frontier = |base, solved| {
            let (request, data) =
                prepare_ar_bleed_frontier(&ArBleedFrontierRequestDto { base, solved }, &state)?;
            evaluate_ar_bleed_frontier(&request, &data, || true).map(|_| ())
        };
        assert_hostile_values_rejected("start_ar_bleed_frontier", &[], |base| {
            frontier(base, solved.clone())
        });
        let mut hostile = solved.clone();
        hostile.upgrade = u8::MAX;
        assert_rejected_quickly("frontier solved.upgrade", || {
            frontier(crate::test_optimize_request(), hostile)
        });
        let mut hostile = solved;
        hostile.affinity = oversized_text();
        assert_rejected_quickly("frontier solved.affinity", || {
            frontier(crate::test_optimize_request(), hostile)
        });
    }

    #[test]
    fn rejections_name_the_field_but_never_the_value() {
        let state = crate::test_app_state();
        let secret = "secret-client-text";
        type TextSetter = fn(&mut crate::dto::OptimizeRequestDto, &str);
        let cases: [(&str, TextSetter); 7] = [
            ("class_name", |request, text| {
                request.class_name = text.into();
            }),
            ("weapon_name", |request, text| {
                request.weapon_name = Some(text.into());
            }),
            ("aow_name", |request, text| {
                request.aow_name = Some(text.into());
            }),
            ("objective", |request, text| request.objective = text.into()),
            ("somber_filter", |request, text| {
                request.somber_filter = text.into();
            }),
            ("result_grouping", |request, text| {
                request.result_grouping = text.into();
            }),
            ("filter dimension", |request, text| {
                request.filters.entries = vec![crate::dto::StableFilterEntryDto {
                    dimension: text.into(),
                    id: "x".into(),
                    mode: "include".into(),
                }];
            }),
        ];
        for (label, apply) in cases {
            let mut request = crate::test_optimize_request();
            apply(&mut request, secret);
            let error = run_search_inner(request, &state).expect_err(label);
            assert!(
                !error.message.contains(secret),
                "{label}: {}",
                error.message
            );
        }
        let mut request = crate::test_optimize_request();
        request.filters.entries = vec![crate::dto::StableFilterEntryDto {
            dimension: "weapon_family".into(),
            id: secret.into(),
            mode: "include".into(),
        }];
        let error = run_search_inner(request, &state).expect_err("unknown family");
        assert!(!error.message.contains(secret), "{}", error.message);
        let mut request = crate::test_optimize_request();
        request.weapon_name = None;
        request.affinity = None;
        request.exact_upgrade = Some(false);
        request.filters.entries = vec![crate::dto::StableFilterEntryDto {
            dimension: "weapon_family".into(),
            id: "x".into(),
            mode: secret.into(),
        }];
        let error = run_search_inner(request, &state).expect_err("unknown mode");
        assert!(!error.message.contains(secret), "{}", error.message);
    }

    #[test]
    fn analysis_and_search_job_ids_are_bounded() {
        let state = crate::test_app_state();
        assert_rejected_quickly("cancel_analysis", || {
            cancel_job(&state.analysis_jobs, &oversized_text())
        });
        assert_rejected_quickly("get_analysis_status", || {
            job_status(&state.analysis_jobs, &oversized_text())
        });
        assert_rejected_quickly("cancel_search", || {
            cancel_job(&state.search_jobs, &oversized_text())
        });
        assert_rejected_quickly("get_search_status", || {
            job_status(&state.search_jobs, &oversized_text())
        });
    }
}
