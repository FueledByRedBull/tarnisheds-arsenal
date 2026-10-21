use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use er_optimizer_core::{
    FilterDimension, FilterMode, GameData, LevelOptimizeResult, OptimizeObjective, OptimizeRequest,
    StableFilter, optimize_level_range_with_progress, optimize_level_ranges_with_progress,
    optimize_prepared_with_progress, optimize_with_cancel, prepare_loadout_evaluator_with_cancel,
    prepare_search_with_cancel, prepare_upgrade_series_evaluator_with_cancel,
};
use tauri::State;

use crate::dto::{
    AnalysisFinishedDto, AnalysisJobKindDto, AnalysisJobStatusDto, ArBleedFrontierPointDto,
    ArBleedFrontierRequestDto, CombatStateDto, SearchFinishedDto, SearchJobStatusDto,
    SearchProgressDto, SolveBuildRequestDto, SolvedBuildDto, StartSearchResponseDto,
    UpgradePointDto, UpgradeSeriesRequestDto, lock_request_to_stats, metric_for_objective,
    set_min_combat_stats,
};
use crate::errors::AppError;
use crate::{AppState, ProfileData};

#[cfg(test)]
pub fn run_search_inner(
    request: crate::dto::OptimizeRequestDto,
    state: &AppState,
) -> Result<Vec<SolvedBuildDto>, AppError> {
    let (request, data) = prepare_search(request, state)?;
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

pub fn run_level_range_inner_with_progress<F, C>(
    mut request: crate::dto::OptimizeRequestDto,
    levels: &[u16],
    profile: &ProfileData,
    level_complete: F,
    should_continue: C,
) -> Result<Vec<LevelOptimizeResult>, AppError>
where
    F: FnMut(u16) -> bool,
    C: Fn() -> bool + Sync,
{
    clamp_weapon_upgrade_request_for_profile(&mut request, profile)?;
    let request = OptimizeRequest::try_from(&request)?;
    optimize_level_range_with_progress(
        &request,
        levels,
        &profile.data,
        level_complete,
        should_continue,
    )
    .map_err(AppError::from)
}

/// Level ranges for independent requests, calculated concurrently. Results keep request
/// order, and a request that fails normalization keeps its error in place.
pub fn run_level_ranges_inner_with_progress<F, C>(
    requests: Vec<crate::dto::OptimizeRequestDto>,
    levels: &[u16],
    profile: &ProfileData,
    level_complete: F,
    should_continue: C,
) -> Vec<Result<Vec<LevelOptimizeResult>, AppError>>
where
    F: Fn(usize, u16) -> bool + Sync,
    C: Fn() -> bool + Sync,
{
    let normalized: Vec<Result<OptimizeRequest, AppError>> = requests
        .into_iter()
        .map(|mut request| {
            clamp_weapon_upgrade_request_for_profile(&mut request, profile)?;
            OptimizeRequest::try_from(&request)
        })
        .collect();
    let (positions, valid): (Vec<usize>, Vec<OptimizeRequest>) = normalized
        .iter()
        .enumerate()
        .filter_map(|(index, request)| Some((index, request.as_ref().ok()?.clone())))
        .unzip();
    let mut ranges = optimize_level_ranges_with_progress(
        &valid,
        levels,
        &profile.data,
        |index, level| level_complete(positions[index], level),
        should_continue,
    )
    .into_iter();
    normalized
        .into_iter()
        .map(|request| {
            request.and_then(|_| {
                ranges
                    .next()
                    .expect("one range per valid request")
                    .map_err(AppError::from)
            })
        })
        .collect()
}

fn solve_build_base(
    request: SolveBuildRequestDto,
    state: &AppState,
) -> Result<crate::dto::OptimizeRequestDto, AppError> {
    let mut base = request.base;
    base.pin_loadout(request.weapon_name, request.affinity, request.aow_name);
    if !crate::commands::data::uses_class_budget(state.profile(&base.profile_id)?, &base.class_name)
    {
        let stats = CombatStateDto {
            str_stat: base.str_stat,
            dex: base.dex,
            int_stat: base.int_stat,
            fai: base.fai,
            arc: base.arc,
        };
        lock_request_to_stats(&mut base, stats);
    }
    Ok(base)
}

fn prepare_solve_build(
    request: SolveBuildRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>), AppError> {
    let mut base = solve_build_base(request, state)?;
    clamp_weapon_upgrade_request(&mut base, state)?;
    let profile = state.profile(&base.profile_id)?;
    Ok((OptimizeRequest::try_from(&base)?, Arc::clone(&profile.data)))
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
    let (request, data) = prepare_solve_build(request, state)?;
    optimize_with_cancel(&request, &data, should_continue)
        .map(|mut rows| rows.pop().map(SolvedBuildDto::from))
        .map_err(AppError::from)
}

#[tauri::command]
pub fn start_solve_build(
    request: SolveBuildRequestDto,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    let (core_request, data) = prepare_solve_build(request, &state)?;
    state.analysis_jobs.start(
        &state.next_job,
        AnalysisJobStatusDto { finished: None },
        move |job_id, cancel_flag, _| {
            let (result, error, cancelled) =
                match optimize_with_cancel(&core_request, &data, || {
                    !cancel_flag.load(Ordering::Relaxed)
                }) {
                    Ok(mut rows) if !cancel_flag.load(Ordering::Relaxed) => {
                        (rows.pop().map(SolvedBuildDto::from), None, false)
                    }
                    Ok(_) => (None, None, true),
                    Err(message) if message == "cancelled" => (None, None, true),
                    Err(message) => (None, Some(message), false),
                };
            AnalysisFinishedDto {
                job_id,
                kind: AnalysisJobKindDto::SolveBuild,
                cancelled,
                result,
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
    request: UpgradeSeriesRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>, u8, OptimizeObjective), AppError> {
    let mut base = request.base;
    base.pin_loadout(
        request.solved.weapon_name.clone(),
        Some(request.solved.affinity.clone()),
        request.solved.aow_name.clone(),
    );
    base.standard_max_upgrade = None;
    base.somber_max_upgrade = None;
    base.exact_upgrade = Some(false);
    set_min_combat_stats(&mut base, [0; 5]);
    lock_request_to_stats(&mut base, request.solved.stats);

    let profile = state.profile(&base.profile_id)?;
    let (is_somber, profile_upgrade_cap) = weapon_reinforcement_info(
        profile,
        &request.solved.weapon_name,
        Some(&request.solved.affinity),
    )?;
    if is_somber != request.solved.is_somber {
        return Err(AppError::new(format!(
            "solved weapon reinforcement type does not match profile data for '{}'",
            request.solved.weapon_name
        )));
    }
    if is_somber {
        base.somber_max_upgrade = Some(request.max_upgrade);
    } else {
        base.standard_max_upgrade = Some(request.max_upgrade);
    }
    clamp_weapon_upgrade_request(&mut base, state)?;
    let max_upgrade = request.max_upgrade.min(profile_upgrade_cap);
    let request = OptimizeRequest::try_from(&base)?;
    let objective = request.objective;
    Ok((request, Arc::clone(&profile.data), max_upgrade, objective))
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
    let (core_request, data, max_upgrade, objective) = prepare_upgrade_series(request, state)?;
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
    let (core_request, data, max_upgrade, objective) = prepare_upgrade_series(request, &state)?;
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
            );
            let (points, error, cancelled) = match result {
                Ok(points) if !cancel_flag.load(Ordering::Relaxed) => (points, None, false),
                Ok(_) => (Vec::new(), None, true),
                Err(error) if error.message == "cancelled" => (Vec::new(), None, true),
                Err(error) => (Vec::new(), Some(error.message), false),
            };
            AnalysisFinishedDto {
                job_id,
                kind: AnalysisJobKindDto::UpgradeSeries,
                cancelled,
                result: None,
                points,
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
/// row and an upgrade level that row supports.
pub fn validate_solved_loadout(
    profile: &ProfileData,
    solved: &SolvedBuildDto,
) -> Result<(), AppError> {
    let weapon = profile
        .data
        .weapons
        .iter()
        .find(|weapon| weapon.weapon_id == solved.weapon_id)
        .filter(|weapon| weapon.name == solved.weapon_name && weapon.affinity == solved.affinity)
        .ok_or_else(|| AppError::new("selected weapon identity does not match profile data"))?;
    let (is_somber, cap) =
        weapon_reinforcement_info(profile, &solved.weapon_name, Some(&solved.affinity))?;
    if is_somber != solved.is_somber
        || solved.upgrade > cap
        || profile
            .data
            .reinforce_level(weapon.reinforce_type, solved.upgrade)
            .is_none()
    {
        return Err(AppError::new(
            "selected loadout upgrade does not match profile data",
        ));
    }
    Ok(())
}

fn prepare_ar_bleed_frontier(
    request: ArBleedFrontierRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>), AppError> {
    let profile = state.profile(&request.base.profile_id)?;
    if !crate::commands::data::uses_class_budget(profile, &request.base.class_name)
        || !profile.data.capabilities.status_buildup
    {
        return Err(AppError::new(
            "AR / bleed tradeoffs require class budgets and status modeling",
        ));
    }
    let solved = request.solved;
    validate_solved_loadout(profile, &solved)?;
    let mut base = request.base;
    base.objective = "max_ar".into();
    base.set_exact_upgrade(solved.upgrade, solved.is_somber);
    let (mut request, data) = prepare_solve_build(
        SolveBuildRequestDto {
            base,
            weapon_name: solved.weapon_name,
            affinity: Some(solved.affinity),
            aow_name: solved.aow_name,
        },
        state,
    )?;
    request.filters.push(StableFilter {
        dimension: FilterDimension::Aow,
        mode: FilterMode::Include,
        id: solved
            .aow_id
            .map_or_else(|| "aow:none".into(), |id| format!("aow:{id}")),
    });
    Ok((request, data))
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
    let (request, data) = prepare_ar_bleed_frontier(request, &state)?;
    state.analysis_jobs.start(
        &state.next_job,
        AnalysisJobStatusDto { finished: None },
        move |job_id, cancel_flag, _| {
            let result = evaluate_ar_bleed_frontier(&request, &data, || {
                !cancel_flag.load(Ordering::Relaxed)
            });
            let (frontier, error, cancelled) = match result {
                Ok(points) if !cancel_flag.load(Ordering::Relaxed) => (points, None, false),
                Ok(_) => (Vec::new(), None, true),
                Err(error) if error.message == "cancelled" => (Vec::new(), None, true),
                Err(error) => (Vec::new(), Some(error.message), false),
            };
            AnalysisFinishedDto {
                job_id,
                kind: AnalysisJobKindDto::ArBleedFrontier,
                cancelled,
                result: None,
                points: Vec::new(),
                frontier,
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
    let mut finished = outcome.unwrap_or_else(|error| AnalysisFinishedDto {
        job_id,
        kind,
        cancelled: false,
        result: None,
        points: Vec::new(),
        frontier: Vec::new(),
        error: Some(error),
    });
    if finished.error.is_none() && cancelled {
        finished.cancelled = true;
        finished.result = None;
        finished.points.clear();
        finished.frontier.clear();
    }
    status.finished = Some(finished);
}

#[tauri::command]
pub fn cancel_analysis(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    state.analysis_jobs.cancel(&job_id)
}

// Terminal payloads can be large; async keeps their clone and serialization off the UI thread.
#[tauri::command(async)]
pub fn get_analysis_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AnalysisJobStatusDto>, AppError> {
    state.analysis_jobs.status(&job_id)
}

fn prepare_search(
    mut request: crate::dto::OptimizeRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>), AppError> {
    clamp_weapon_upgrade_request(&mut request, state)?;
    let profile = state.profile(&request.profile_id)?;
    Ok((
        OptimizeRequest::try_from(&request)?,
        Arc::clone(&profile.data),
    ))
}

#[tauri::command]
pub fn start_search(
    request: crate::dto::OptimizeRequestDto,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    let (core_request, data) = prepare_search(request, &state)?;
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
        10_000,
        || !cancel_flag.load(Ordering::Relaxed),
        |snapshot| {
            // Hydration reports at each row/route boundary without advancing
            // scoring counts. Publish status at a bounded rate.
            let first_scoring_completion = !scoring_complete && snapshot.checked == snapshot.total;
            scoring_complete |= snapshot.checked == snapshot.total;
            if !first_scoring_completion
                && last_progress_at.is_some_and(|at| at.elapsed() < Duration::from_millis(100))
            {
                return;
            }
            last_progress_at = Some(Instant::now());
            let mut payload = SearchProgressDto::from(snapshot);
            payload.job_id.clone_from(&job_id);
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
    let mut finished = outcome.unwrap_or_else(|error| SearchFinishedDto {
        job_id,
        cancelled: false,
        rows: Vec::new(),
        error: Some(error),
        setups: 0,
    });
    if finished.error.is_none() && cancelled {
        finished.cancelled = true;
        finished.rows.clear();
    }
    status.finished = Some(finished);
}

#[tauri::command]
pub fn cancel_search(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    state.search_jobs.cancel(&job_id)
}

#[tauri::command(async)]
pub fn get_search_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<SearchJobStatusDto>, AppError> {
    state.search_jobs.status(&job_id)
}

pub fn clamp_weapon_upgrade_request(
    request: &mut crate::dto::OptimizeRequestDto,
    state: &AppState,
) -> Result<(), AppError> {
    let profile = state.profile(&request.profile_id)?;
    clamp_weapon_upgrade_request_for_profile(request, profile)
}

fn clamp_weapon_upgrade_request_for_profile(
    request: &mut crate::dto::OptimizeRequestDto,
    profile: &ProfileData,
) -> Result<(), AppError> {
    if profile.data_manifest.profile.id != request.profile_id {
        return Err(AppError::new(format!(
            "Unknown game profile {:?}. Reload the catalog and choose an available profile.",
            request.profile_id
        )));
    }
    if !crate::commands::data::class_metadata(profile)
        .iter()
        .any(|class_info| class_info.name.eq_ignore_ascii_case(&request.class_name))
    {
        return Err(AppError::new(format!(
            "starting class '{}' is not available for profile '{}'",
            request.class_name, request.profile_id
        )));
    }
    if !crate::commands::data::uses_class_budget(profile, &request.class_name) {
        if !request
            .class_name
            .eq_ignore_ascii_case(er_optimizer_core::CUSTOM_STATS_CLASS_NAME)
        {
            return Err(AppError::new(format!(
                "profile '{}' requires {} for custom stat budgets",
                request.profile_id,
                er_optimizer_core::CUSTOM_STATS_CLASS_NAME
            )));
        }
        let level_from_stats = [
            request.vig,
            request.mnd,
            request.end,
            request.str_stat,
            request.dex,
            request.int_stat,
            request.fai,
            request.arc,
        ]
        .into_iter()
        .map(u16::from)
        .sum::<u16>();
        if request.character_level != level_from_stats {
            return Err(AppError::new(format!(
                "custom stat profile '{}' requires character level {} for the selected stats; got {}",
                request.profile_id, level_from_stats, request.character_level
            )));
        }
        for (label, lock, current) in [
            ("str", request.lock_str, request.str_stat),
            ("dex", request.lock_dex, request.dex),
            ("int", request.lock_int, request.int_stat),
            ("fai", request.lock_fai, request.fai),
            ("arc", request.lock_arc, request.arc),
        ] {
            if lock != Some(current) {
                return Err(AppError::new(format!(
                    "custom stat profile '{}' requires {label} to be locked to its current value",
                    request.profile_id
                )));
            }
        }
    }
    let standard_cap = request.standard_upgrade_cap();
    let somber_cap = request.somber_upgrade_cap();
    if request.exact_upgrade_enabled() {
        for (kind, requested, supported) in [
            (
                "standard",
                standard_cap,
                profile.data.rules.standard_max_upgrade,
            ),
            ("somber", somber_cap, profile.data.rules.somber_max_upgrade),
        ] {
            if requested > supported {
                return Err(AppError::new(format!(
                    "profile '{}' does not support exact {kind} +{requested}; its maximum is +{supported}",
                    request.profile_id,
                )));
            }
        }
        if let Some(weapon_name) = request.weapon_name.as_deref() {
            let (is_somber, _) =
                weapon_reinforcement_info(profile, weapon_name, request.affinity.as_deref())?;
            let requested = if is_somber { somber_cap } else { standard_cap };
            let available = profile.data.weapons.iter().any(|weapon| {
                weapon.name.eq_ignore_ascii_case(weapon_name)
                    && request
                        .affinity
                        .as_deref()
                        .is_none_or(|affinity| weapon.affinity.eq_ignore_ascii_case(affinity))
                    && profile
                        .data
                        .reinforce_level(weapon.reinforce_type, requested)
                        .is_some()
            });
            if !available {
                return Err(AppError::new(format!(
                    "{weapon_name} does not support exact +{requested}; choose an available reinforcement level or use range mode",
                )));
            }
        }
        return Ok(());
    }
    let standard_cap = standard_cap.min(profile.data.rules.standard_max_upgrade);
    let somber_cap = somber_cap.min(profile.data.rules.somber_max_upgrade);
    request.standard_max_upgrade = Some(standard_cap);
    request.somber_max_upgrade = Some(somber_cap);
    let Some(weapon_name) = request.weapon_name.as_deref() else {
        return Ok(());
    };
    let (is_somber, profile_upgrade_cap) =
        weapon_reinforcement_info(profile, weapon_name, request.affinity.as_deref())?;
    if is_somber {
        request.somber_max_upgrade = Some(somber_cap.min(profile_upgrade_cap));
    } else {
        request.standard_max_upgrade = Some(standard_cap.min(profile_upgrade_cap));
    }
    Ok(())
}

fn weapon_reinforcement_info(
    profile: &ProfileData,
    weapon_name: &str,
    affinity: Option<&str>,
) -> Result<(bool, u8), AppError> {
    let mut matches = profile.data.weapons.iter().filter(|weapon| {
        weapon.name.eq_ignore_ascii_case(weapon_name)
            && affinity.is_none_or(|value| weapon.affinity.eq_ignore_ascii_case(value))
    });
    let Some(first) = matches.next() else {
        return Err(AppError::new(format!(
            "weapon not found in profile data: {}",
            weapon_name
        )));
    };
    let is_somber = first.is_somber;
    if matches.any(|weapon| weapon.is_somber != is_somber) {
        return Err(AppError::new(format!(
            "weapon '{}' has mixed reinforcement types; specify an affinity",
            weapon_name
        )));
    }
    let profile_upgrade_cap =
        crate::commands::data::weapon_upgrade_cap(&profile.catalog_index, weapon_name, affinity)?;
    Ok((is_somber, profile_upgrade_cap))
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::AsyncJobHandle;

    #[test]
    fn focused_exact_upgrade_does_not_substitute_a_lower_level() {
        let state = crate::test_app_state();
        let mut request = crate::test_optimize_request();
        request.character_level = 80;
        request.weapon_name = Some("Meteorite Staff".into());
        request.affinity = Some("Standard".into());
        request.somber_max_upgrade = Some(10);
        let error = clamp_weapon_upgrade_request(&mut request, &state)
            .expect_err("an exact +10 request must not become +0");
        assert!(error.message.contains("exact +10"));
        assert_eq!(request.somber_max_upgrade, Some(10));
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
            request.set_exact_upgrade(exact_upgrade, is_somber);
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
            assert!(
                clamp_weapon_upgrade_request(&mut request, &state)
                    .unwrap_err()
                    .message
                    .contains("exact standard +26")
            );
            assert_eq!(request.standard_max_upgrade, Some(26));
            request.exact_upgrade = Some(false);
            clamp_weapon_upgrade_request(&mut request, &state).unwrap();
            assert_eq!(
                request.standard_max_upgrade,
                Some(
                    state
                        .profile(profile_id)
                        .unwrap()
                        .data
                        .rules
                        .standard_max_upgrade
                )
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
            clamp_weapon_upgrade_request_for_profile(&mut request, &profile)
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
                        SolveBuildRequestDto {
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
                            UpgradeSeriesRequestDto {
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
                        ArBleedFrontierRequestDto {
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
        let (request, data) = prepare_search(crate::test_optimize_request(), &state).unwrap();
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
            ArBleedFrontierRequestDto {
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
            prepare_ar_bleed_frontier(ArBleedFrontierRequestDto { base, solved }, &state).is_err()
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
                ArBleedFrontierRequestDto {
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
            prepare_ar_bleed_frontier(ArBleedFrontierRequestDto { base, solved }, &state).is_err()
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

        clamp_weapon_upgrade_request(&mut request, &state)
            .expect("Vanilla unique weapon request should clamp");
        assert_eq!(request.standard_max_upgrade, Some(25));
        assert_eq!(request.somber_max_upgrade, Some(10));
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
        let error = clamp_weapon_upgrade_request(&mut wrong_level, &state)
            .expect_err("custom stats with an inconsistent level must be rejected");
        assert!(error.message.contains("requires character level"));

        let mut missing_lock = convergence_custom_stats_request();
        missing_lock.lock_arc = None;
        let error = clamp_weapon_upgrade_request(&mut missing_lock, &state)
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
        clamp_weapon_upgrade_request(&mut prisoner, &state).expect("Convergence class");
        let mut samurai = prisoner.clone();
        samurai.class_name = "Samurai".to_string();
        let error = clamp_weapon_upgrade_request(&mut samurai, &state)
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
                request.pin_loadout(
                    "Uchigatana".to_string(),
                    Some("Blood".to_string()),
                    Some("Seppuku".to_string()),
                );
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
                let (request, data) = prepare_search(request.clone(), &state).unwrap();
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
                ArBleedFrontierRequestDto {
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
