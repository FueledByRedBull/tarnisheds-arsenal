use std::sync::Arc;
use std::sync::atomic::Ordering;

#[cfg(test)]
use er_optimizer_core::optimize_with_cancel;
use er_optimizer_core::{
    OptimizeRequest, estimate_search_space as core_estimate_search_space, optimize,
    optimize_level_range_with_progress, optimize_prepared_with_progress,
    prepare_search_with_cancel, prepare_upgrade_series_evaluator_with_cancel,
};
use tauri::{AppHandle, State};

use crate::commands::data::weapon_upgrade_cap;
use crate::dto::{
    SearchEstimateDto, SearchFinishedDto, SearchJobStatusDto, SearchProgressDto,
    SolveBuildRequestDto, SolvedBuildDto, StartSearchResponseDto, UpgradePointDto,
    UpgradeSeriesRequestDto, lock_request_to_stats, metric_for_objective, parse_objective,
};
use crate::errors::AppError;
use crate::{AppState, AsyncJobHandle, CancelFlag};

#[tauri::command]
pub fn estimate_search_space(
    request: crate::dto::OptimizeRequestDto,
    state: State<'_, AppState>,
) -> Result<SearchEstimateDto, AppError> {
    let profile = state.profile(&request.profile_id)?;
    let request = OptimizeRequest::try_from(&request)?;
    core_estimate_search_space(&request, &profile.data)
        .map(SearchEstimateDto::from)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn run_search(
    request: crate::dto::OptimizeRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<SolvedBuildDto>, AppError> {
    run_search_inner(request, &state)
}

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
    state: &AppState,
    level_complete: F,
    should_continue: C,
) -> Result<Vec<(u16, Vec<SolvedBuildDto>)>, AppError>
where
    F: FnMut(u16) -> bool,
    C: FnMut() -> bool + Send,
{
    clamp_weapon_upgrade_request(&mut request, state)?;
    let profile = state.profile(&request.profile_id)?;
    let request = OptimizeRequest::try_from(&request)?;
    optimize_level_range_with_progress(
        &request,
        levels,
        &profile.data,
        level_complete,
        should_continue,
    )
    .map(|levels| {
        levels
            .into_iter()
            .map(|entry| {
                (
                    entry.level,
                    entry.rows.into_iter().map(SolvedBuildDto::from).collect(),
                )
            })
            .collect()
    })
    .map_err(AppError::from)
}

#[tauri::command]
pub fn solve_build(
    request: SolveBuildRequestDto,
    state: State<'_, AppState>,
) -> Result<Option<SolvedBuildDto>, AppError> {
    let mut base = request.base;
    base.weapon_name = Some(request.weapon_name);
    base.affinity = request.affinity;
    base.aow_name = request.aow_name;
    base.weapon_type_key = None;
    base.somber_filter = "all".to_string();
    base.lock_str = None;
    base.lock_dex = None;
    base.lock_int = None;
    base.lock_fai = None;
    base.lock_arc = None;
    base.top_k = 1;
    run_search_inner(base, &state).map(|mut rows| rows.pop())
}

#[tauri::command]
pub fn build_upgrade_series(
    request: UpgradeSeriesRequestDto,
    state: State<'_, AppState>,
) -> Result<Vec<UpgradePointDto>, AppError> {
    build_upgrade_series_inner(request, &state)
}

pub fn build_upgrade_series_inner(
    request: UpgradeSeriesRequestDto,
    state: &AppState,
) -> Result<Vec<UpgradePointDto>, AppError> {
    build_upgrade_series_inner_with_cancel(request, state, || true)
}

pub fn build_upgrade_series_inner_with_cancel<F>(
    request: UpgradeSeriesRequestDto,
    state: &AppState,
    mut should_continue: F,
) -> Result<Vec<UpgradePointDto>, AppError>
where
    F: FnMut() -> bool + Send,
{
    let mut base = request.base;
    base.weapon_name = Some(request.solved.weapon_name.clone());
    base.affinity = Some(request.solved.affinity.clone());
    base.aow_name = request.solved.aow_name.clone();
    base.weapon_type_key = None;
    base.somber_filter = "all".to_string();
    if request.solved.is_somber {
        base.somber_max_upgrade = Some(request.max_upgrade.min(10));
    } else {
        base.standard_max_upgrade = Some(request.max_upgrade.min(25));
    }
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

    let objective = parse_objective(&base.objective)?;
    let profile = state.profile(&base.profile_id)?;
    let core_request = OptimizeRequest::try_from(&base)?;
    let evaluator = prepare_upgrade_series_evaluator_with_cancel(
        &core_request,
        &profile.data,
        &mut should_continue,
    )
    .map_err(AppError::from)?;
    Ok(evaluator
        .evaluate_with_cancel(&core_request, request.max_upgrade, &mut should_continue)
        .map_err(AppError::from)?
        .into_iter()
        .map(SolvedBuildDto::from)
        .map(|solved| UpgradePointDto {
            upgrade: solved.upgrade,
            metric: metric_for_objective(&solved, objective),
        })
        .collect())
}

#[tauri::command]
pub fn start_search(
    mut request: crate::dto::OptimizeRequestDto,
    _app: AppHandle,
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
                guard.progress = Some(payload.clone());
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
            guard.finished = Some(finished.clone());
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
    let Some(weapon_name) = request.weapon_name.as_deref() else {
        return Ok(());
    };
    let cap = weapon_upgrade_cap(
        &state.profile(&request.profile_id)?.catalog_index,
        weapon_name,
        request.affinity.as_deref(),
    )?;
    if cap <= 10 {
        request.somber_max_upgrade = Some(request.somber_upgrade_cap().min(cap));
    } else {
        request.standard_max_upgrade = Some(request.standard_upgrade_cap().min(cap));
    }
    request.max_upgrade = None;
    request.fixed_upgrade = None;
    Ok(())
}

#[cfg(test)]
mod integration_tests {
    use super::*;

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
    fn packaged_snapshot_executes_a_real_search_estimate() {
        let state = crate::test_app_state();
        let mut request = crate::test_optimize_request();
        clamp_weapon_upgrade_request(&mut request, &state).expect("upgrade cap resolves");
        let request = OptimizeRequest::try_from(&request).expect("request converts");
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let estimate = core_estimate_search_space(&request, &profile.data)
            .expect("real command estimate succeeds");
        assert_eq!(estimate.weapon_candidates, 1);
        assert!(estimate.combinations > 0);
    }

    #[test]
    fn real_tauri_search_honors_cancellation() {
        let state = crate::test_app_state();
        let error = run_search_inner_with_cancel(crate::test_optimize_request(), &state, || false)
            .expect_err("cancelled command search must fail closed");
        assert_eq!(error.message, "cancelled");
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
