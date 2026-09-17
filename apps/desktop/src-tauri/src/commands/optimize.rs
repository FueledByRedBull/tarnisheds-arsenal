use std::sync::Arc;
use std::sync::atomic::Ordering;

#[cfg(test)]
use er_optimizer_core::optimize;
use er_optimizer_core::{
    FilterDimension, FilterMode, GameData, LevelOptimizeResult, OptimizeRequest, StableFilter,
    optimize_level_range_with_progress, optimize_prepared_with_progress, optimize_with_cancel,
    prepare_loadout_evaluator_with_cancel, prepare_search_with_cancel,
    prepare_upgrade_series_evaluator_with_cancel,
};
use tauri::State;

use crate::dto::{
    AnalysisFinishedDto, AnalysisJobKindDto, AnalysisJobStatusDto, ArBleedFrontierPointDto,
    ArBleedFrontierRequestDto, CombatStateDto, SearchFinishedDto, SearchJobStatusDto,
    SearchProgressDto, SolveBuildRequestDto, SolvedBuildDto, StartSearchResponseDto,
    UpgradePointDto, UpgradeSeriesRequestDto, lock_request_to_stats, metric_for_objective,
    parse_objective,
};
use crate::errors::AppError;
use crate::{AppState, AsyncJobHandle, CancelFlag, ProfileData};

#[cfg(test)]
pub fn run_search_inner(
    mut request: crate::dto::OptimizeRequestDto,
    state: &AppState,
) -> Result<Vec<SolvedBuildDto>, AppError> {
    clamp_weapon_upgrade_request(&mut request, state)?;
    let profile = state.profile(&request.profile_id)?;
    let request = OptimizeRequest::try_from(&request)?;
    optimize(&request, &profile.data)
        .map(|rows| rows.into_iter().map(SolvedBuildDto::from).collect())
        .map_err(AppError::from)
}

#[cfg(test)]
pub fn run_search_inner_with_cancel<F>(
    mut request: crate::dto::OptimizeRequestDto,
    state: &AppState,
    should_continue: F,
) -> Result<Vec<SolvedBuildDto>, AppError>
where
    F: FnMut() -> bool + Send,
{
    clamp_weapon_upgrade_request(&mut request, state)?;
    let profile = state.profile(&request.profile_id)?;
    let request = OptimizeRequest::try_from(&request)?;
    optimize_with_cancel(&request, &profile.data, should_continue)
        .map(|rows| rows.into_iter().map(SolvedBuildDto::from).collect())
        .map_err(AppError::from)
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
    C: FnMut() -> bool + Send,
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

fn solve_build_base(
    request: SolveBuildRequestDto,
    state: &AppState,
) -> Result<crate::dto::OptimizeRequestDto, AppError> {
    let mut base = request.base;
    base.weapon_name = Some(request.weapon_name);
    base.affinity = request.affinity;
    base.aow_name = request.aow_name;
    base.weapon_type_key = None;
    base.somber_filter = "all".to_string();
    base.filters.entries.clear();
    if !state
        .profile(&base.profile_id)?
        .data
        .capabilities
        .class_budget
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
    base.top_k = 1;
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
    F: FnMut() -> bool + Send,
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
    let (job_id, cancel_flag, status) = start_analysis_job(&state)?;
    let job_id_for_task = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (result, error, cancelled) = match optimize_with_cancel(&core_request, &data, || {
            !cancel_flag.load(Ordering::Relaxed)
        }) {
            Ok(mut rows) if !cancel_flag.load(Ordering::Relaxed) => {
                (rows.pop().map(SolvedBuildDto::from), None, false)
            }
            Ok(_) => (None, None, true),
            Err(message) if message == "cancelled" => (None, None, true),
            Err(message) => (None, Some(message), false),
        };
        let finished = AnalysisFinishedDto {
            job_id: job_id_for_task,
            kind: AnalysisJobKindDto::SolveBuild,
            cancelled,
            result,
            points: Vec::new(),
            frontier: Vec::new(),
            error,
        };
        if let Ok(mut guard) = status.lock() {
            guard.finished = Some(finished);
        }
    });
    Ok(StartSearchResponseDto { job_id })
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
) -> Result<
    (
        OptimizeRequest,
        Arc<GameData>,
        u8,
        er_optimizer_core::OptimizeObjective,
    ),
    AppError,
> {
    let mut base = request.base;
    base.weapon_name = Some(request.solved.weapon_name.clone());
    base.affinity = Some(request.solved.affinity.clone());
    base.aow_name = request.solved.aow_name.clone();
    base.weapon_type_key = None;
    base.somber_filter = "all".to_string();
    base.filters.entries.clear();
    base.standard_max_upgrade = None;
    base.somber_max_upgrade = None;
    base.exact_upgrade = Some(false);
    base.max_upgrade = None;
    base.fixed_upgrade = None;
    base.top_k = 1;
    base.min_str = 0;
    base.min_dex = 0;
    base.min_int = 0;
    base.min_fai = 0;
    base.min_arc = 0;
    lock_request_to_stats(&mut base, request.solved.stats);

    let profile = state.profile(&base.profile_id)?;
    let (is_somber, profile_upgrade_cap) = weapon_reinforcement_info(
        &profile.data,
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
    let objective = parse_objective(&base.objective)?;
    Ok((
        OptimizeRequest::try_from(&base)?,
        Arc::clone(&profile.data),
        max_upgrade,
        objective,
    ))
}

fn evaluate_upgrade_series_with_cancel<F>(
    core_request: &OptimizeRequest,
    data: &GameData,
    max_upgrade: u8,
    objective: er_optimizer_core::OptimizeObjective,
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
    let (job_id, cancel_flag, status) = start_analysis_job(&state)?;
    let job_id_for_task = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
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
        let finished = AnalysisFinishedDto {
            job_id: job_id_for_task,
            kind: AnalysisJobKindDto::UpgradeSeries,
            cancelled,
            result: None,
            points,
            frontier: Vec::new(),
            error,
        };
        if let Ok(mut guard) = status.lock() {
            guard.finished = Some(finished);
        }
    });
    Ok(StartSearchResponseDto { job_id })
}

fn prepare_ar_bleed_frontier(
    request: ArBleedFrontierRequestDto,
    state: &AppState,
) -> Result<(OptimizeRequest, Arc<GameData>), AppError> {
    let profile = state.profile(&request.base.profile_id)?;
    if !profile.data.capabilities.class_budget || !profile.data.capabilities.status_buildup {
        return Err(AppError::new(
            "AR / bleed tradeoffs require class budgets and status modeling",
        ));
    }
    let solved = request.solved;
    let weapon = profile
        .data
        .weapons
        .iter()
        .find(|weapon| weapon.weapon_id == solved.weapon_id)
        .filter(|weapon| weapon.name == solved.weapon_name && weapon.affinity == solved.affinity)
        .ok_or_else(|| AppError::new("selected weapon identity does not match profile data"))?;
    let (is_somber, cap) =
        weapon_reinforcement_info(&profile.data, &solved.weapon_name, Some(&solved.affinity))?;
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
    let mut base = request.base;
    base.objective = "max_ar".into();
    base.exact_upgrade = Some(true);
    base.standard_max_upgrade = Some(solved.upgrade);
    base.somber_max_upgrade = Some(solved.upgrade);
    base.max_upgrade = None;
    base.fixed_upgrade = None;
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
    mut should_continue: F,
) -> Result<Vec<ArBleedFrontierPointDto>, AppError>
where
    F: FnMut() -> bool + Send,
{
    let evaluator = prepare_loadout_evaluator_with_cancel(request, data, &mut should_continue)
        .map_err(AppError::from)?;
    evaluator
        .evaluate_ar_bleed_frontier_with_cancel(request, &mut should_continue)
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
    let (job_id, cancel_flag, status) = start_analysis_job(&state)?;
    let task_id = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result =
            evaluate_ar_bleed_frontier(&request, &data, || !cancel_flag.load(Ordering::Relaxed));
        let (frontier, error, cancelled) = match result {
            Ok(points) if !cancel_flag.load(Ordering::Relaxed) => (points, None, false),
            Ok(_) => (Vec::new(), None, true),
            Err(error) if error.message == "cancelled" => (Vec::new(), None, true),
            Err(error) => (Vec::new(), Some(error.message), false),
        };
        if let Ok(mut guard) = status.lock() {
            guard.finished = Some(AnalysisFinishedDto {
                job_id: task_id,
                kind: AnalysisJobKindDto::ArBleedFrontier,
                cancelled,
                result: None,
                points: Vec::new(),
                frontier,
                error,
            });
        }
    });
    Ok(StartSearchResponseDto { job_id })
}

fn start_analysis_job(
    state: &AppState,
) -> Result<
    (
        String,
        CancelFlag,
        Arc<std::sync::Mutex<AnalysisJobStatusDto>>,
    ),
    AppError,
> {
    let job_number = state.next_job.fetch_add(1, Ordering::Relaxed);
    let job_id = format!("analysis-{job_number}");
    let cancel_flag: CancelFlag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let status = Arc::new(std::sync::Mutex::new(AnalysisJobStatusDto {
        finished: None,
    }));
    state.analysis_jobs.insert_if_idle(
        job_id.clone(),
        AsyncJobHandle {
            cancel: Arc::clone(&cancel_flag),
            status: Arc::clone(&status),
        },
        |status| status.finished.is_some(),
    )?;
    Ok((job_id, cancel_flag, status))
}

#[tauri::command]
pub fn cancel_analysis(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    state.analysis_jobs.cancel(&job_id)
}

#[tauri::command]
pub fn get_analysis_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AnalysisJobStatusDto>, AppError> {
    state
        .analysis_jobs
        .status(&job_id, |status| status.finished.is_some())
}

#[tauri::command]
pub fn start_search(
    mut request: crate::dto::OptimizeRequestDto,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    clamp_weapon_upgrade_request(&mut request, &state)?;
    let profile = state.profile(&request.profile_id)?;
    let core_request = OptimizeRequest::try_from(&request)?;
    let data = Arc::clone(&profile.data);
    let job_number = state.next_job.fetch_add(1, Ordering::Relaxed);
    let job_id = format!("search-{job_number}");
    let cancel_flag: CancelFlag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let status = Arc::new(std::sync::Mutex::new(SearchJobStatusDto {
        progress: None,
        finished: None,
    }));
    state.search_jobs.insert_if_idle(
        job_id.clone(),
        AsyncJobHandle {
            cancel: Arc::clone(&cancel_flag),
            status: Arc::clone(&status),
        },
        |status| status.finished.is_some(),
    )?;

    let job_id_for_task = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress_job_id = job_id_for_task.clone();
        let plan = match prepare_search_with_cancel(&core_request, &data, || {
            !cancel_flag.load(Ordering::Relaxed)
        }) {
            Ok(plan) => plan,
            Err(message) => {
                if let Ok(mut guard) = status.lock() {
                    guard.finished = Some(SearchFinishedDto {
                        job_id: job_id_for_task,
                        cancelled: message == "cancelled",
                        rows: Vec::new(),
                        error: (message != "cancelled").then_some(message),
                    });
                }
                return;
            }
        };
        let result = optimize_prepared_with_progress(&plan, 10_000, |snapshot| {
            if cancel_flag.load(Ordering::Relaxed) {
                return false;
            }
            let mut payload = SearchProgressDto::from(snapshot);
            payload.job_id = progress_job_id.clone();
            if let Ok(mut guard) = status.lock() {
                guard.progress = Some(payload);
            }
            true
        });

        let (rows, error, cancelled) = match result {
            Ok(rows) => (
                rows.into_iter().map(SolvedBuildDto::from).collect(),
                None,
                false,
            ),
            Err(message) if message == "cancelled" => (Vec::new(), None, true),
            Err(message) => (Vec::new(), Some(message), false),
        };
        let finished = SearchFinishedDto {
            job_id: job_id_for_task.clone(),
            cancelled,
            rows,
            error,
        };
        if let Ok(mut guard) = status.lock() {
            guard.finished = Some(finished);
        }
    });

    Ok(StartSearchResponseDto { job_id })
}

#[tauri::command]
pub fn cancel_search(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    state.search_jobs.cancel(&job_id)
}

#[tauri::command]
pub fn get_search_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<SearchJobStatusDto>, AppError> {
    state
        .search_jobs
        .status(&job_id, |status| status.finished.is_some())
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
    if !crate::commands::data::class_metadata(
        profile.data_manifest.profile.game_version == "1.17",
        profile.data.capabilities.class_budget,
    )
    .iter()
    .any(|class_info| class_info.name.eq_ignore_ascii_case(&request.class_name))
    {
        return Err(AppError::new(format!(
            "starting class '{}' is not available for profile '{}'",
            request.class_name, request.profile_id
        )));
    }
    if !profile.data.capabilities.class_budget {
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
    let standard_cap = request
        .standard_upgrade_cap()
        .min(profile.data.rules.standard_max_upgrade);
    let somber_cap = request
        .somber_upgrade_cap()
        .min(profile.data.rules.somber_max_upgrade);
    request.standard_max_upgrade = Some(standard_cap);
    request.somber_max_upgrade = Some(somber_cap);
    let Some(weapon_name) = request.weapon_name.as_deref() else {
        return Ok(());
    };
    let (is_somber, profile_upgrade_cap) =
        weapon_reinforcement_info(&profile.data, weapon_name, request.affinity.as_deref())?;
    if is_somber {
        request.somber_max_upgrade = Some(somber_cap.min(profile_upgrade_cap));
    } else {
        request.standard_max_upgrade = Some(standard_cap.min(profile_upgrade_cap));
    }
    request.max_upgrade = None;
    request.fixed_upgrade = None;
    Ok(())
}

fn weapon_reinforcement_info(
    data: &GameData,
    weapon_name: &str,
    affinity: Option<&str>,
) -> Result<(bool, u8), AppError> {
    let mut matches = data.weapons.iter().filter(|weapon| {
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
    let profile_upgrade_cap = if is_somber {
        data.rules.somber_max_upgrade
    } else {
        data.rules.standard_max_upgrade
    };
    Ok((is_somber, profile_upgrade_cap))
}

#[cfg(test)]
mod integration_tests {
    use super::*;

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
        request.standard_max_upgrade = Some(25);
        request.somber_max_upgrade = Some(25);
        request.exact_upgrade = Some(true);
        request.max_upgrade = None;
        request.fixed_upgrade = None;
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
    fn real_tauri_search_honors_cancellation() {
        let state = crate::test_app_state();
        let error = run_search_inner_with_cancel(crate::test_optimize_request(), &state, || false)
            .expect_err("cancelled command search must fail closed");
        assert_eq!(error.message, "cancelled");
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
        let (job_id, cancel_flag, _) = start_analysis_job(&state).expect("first job starts");
        let error = match start_analysis_job(&state) {
            Ok(_) => panic!("second direct analysis job must be rejected"),
            Err(error) => error,
        };
        assert!(error.message.contains("analysis job is already running"));
        assert!(
            state
                .analysis_jobs
                .cancel(&job_id)
                .expect("job is cancellable")
        );
        assert!(cancel_flag.load(Ordering::Relaxed));
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
}
