use std::sync::Arc;
use std::sync::atomic::Ordering;

use er_optimizer_core::{
    OptimizeRequest, estimate_search_space as core_estimate_search_space, optimize,
    optimize_prepared_with_progress, prepare_search,
};
use tauri::{AppHandle, State};

use crate::commands::data::weapon_upgrade_cap;
use crate::dto::{
    SearchEstimateDto, SearchFinishedDto, SearchJobStatusDto, SearchProgressDto,
    SolveBuildRequestDto, SolvedBuildDto, StartOptimizationResponseDto, UpgradePointDto,
    UpgradeSeriesRequestDto, lock_request_to_stats, metric_for_objective, parse_objective,
};
use crate::errors::AppError;
use crate::{AppState, AsyncJobHandle, CancelFlag};

#[tauri::command]
pub fn estimate_search_space(
    request: crate::dto::OptimizeRequestDto,
    state: State<'_, AppState>,
) -> Result<SearchEstimateDto, AppError> {
    let request = OptimizeRequest::try_from(&request)?;
    core_estimate_search_space(&request, &state.data)
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
    let request = OptimizeRequest::try_from(&request)?;
    optimize(&request, &state.data)
        .map(|rows| rows.into_iter().map(SolvedBuildDto::from).collect())
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
    base.top_k = usize::from(request.max_upgrade) + 1;
    base.min_str = 0;
    base.min_dex = 0;
    base.min_int = 0;
    base.min_fai = 0;
    base.min_arc = 0;
    lock_request_to_stats(&mut base, request.solved.stats);

    let objective = parse_objective(&base.objective)?;
    let mut points: Vec<UpgradePointDto> = run_search_inner(base, &state)?
        .into_iter()
        .map(|row| UpgradePointDto {
            upgrade: row.upgrade,
            metric: metric_for_objective(&row, objective),
        })
        .collect();
    points.sort_by_key(|point| point.upgrade);
    Ok(points)
}

#[tauri::command]
pub fn start_search(
    mut request: crate::dto::OptimizeRequestDto,
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StartOptimizationResponseDto, AppError> {
    clamp_weapon_upgrade_request(&mut request, &state)?;
    let core_request = OptimizeRequest::try_from(&request)?;
    let data = Arc::clone(&state.data);
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
    let (estimate_tx, estimate_rx) = std::sync::mpsc::sync_channel(1);
    tauri::async_runtime::spawn_blocking(move || {
        let progress_job_id = job_id_for_task.clone();
        let plan = match prepare_search(&core_request, &data) {
            Ok(plan) => plan,
            Err(message) => {
                let _ = estimate_tx.send(Err(message.clone()));
                if let Ok(mut guard) = status.lock() {
                    guard.finished = Some(SearchFinishedDto {
                        job_id: job_id_for_task,
                        cancelled: false,
                        rows: Vec::new(),
                        error: Some(message),
                    });
                }
                return;
            }
        };
        let estimate = plan.estimate();
        if estimate_tx.send(Ok(estimate)).is_err() {
            if let Ok(mut guard) = status.lock() {
                guard.finished = Some(SearchFinishedDto {
                    job_id: job_id_for_task,
                    cancelled: true,
                    rows: Vec::new(),
                    error: None,
                });
            }
            return;
        }
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

    let estimate = estimate_rx
        .recv()
        .map_err(|_| AppError::new("search preparation stopped before returning an estimate"))?
        .map_err(AppError::from)?;
    Ok(StartOptimizationResponseDto {
        job_id,
        estimate: SearchEstimateDto::from(estimate),
    })
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
        &state.catalog_index,
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
