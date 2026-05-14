use std::sync::Arc;
use std::sync::atomic::Ordering;

use er_optimizer_core::{
    OptimizeRequest, estimate_search_space as core_estimate_search_space, optimize,
    optimize_with_progress,
};
use tauri::{AppHandle, Emitter, State};

use crate::commands::data::weapon_upgrade_cap;
use crate::dto::{
    SearchEstimateDto, SearchFinishedDto, SearchProgressDto, SolveBuildRequestDto, SolvedBuildDto,
    StartSearchResponseDto, UpgradePointDto, UpgradeSeriesRequestDto, lock_request_to_stats,
    metric_for_objective,
};
use crate::errors::AppError;
use crate::{AppState, CancelFlag};

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
    base.max_upgrade = request.max_upgrade;
    base.fixed_upgrade = None;
    base.top_k = usize::from(request.max_upgrade) + 1;
    base.min_str = 0;
    base.min_dex = 0;
    base.min_int = 0;
    base.min_fai = 0;
    base.min_arc = 0;
    lock_request_to_stats(&mut base, request.solved.stats);

    let objective = base.objective.clone();
    let mut points: Vec<UpgradePointDto> = run_search_inner(base, &state)?
        .into_iter()
        .map(|row| UpgradePointDto {
            upgrade: row.upgrade,
            metric: metric_for_objective(&row, &objective),
        })
        .collect();
    points.sort_by_key(|point| point.upgrade);
    Ok(points)
}

#[tauri::command]
pub fn start_search(
    mut request: crate::dto::OptimizeRequestDto,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    clamp_weapon_upgrade_request(&mut request, &state)?;
    let core_request = OptimizeRequest::try_from(&request)?;
    let data = Arc::clone(&state.data);
    let jobs = Arc::clone(&state.jobs);
    let job_number = state.next_job.fetch_add(1, Ordering::Relaxed);
    let job_id = format!("search-{job_number}");
    let cancel_flag: CancelFlag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    state
        .jobs
        .lock()
        .map_err(|_| AppError::new("failed to lock job registry"))?
        .insert(job_id.clone(), Arc::clone(&cancel_flag));

    let job_id_for_task = job_id.clone();
    let app_for_task = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress_job_id = job_id_for_task.clone();
        let result = optimize_with_progress(&core_request, &data, 250, |snapshot| {
            if cancel_flag.load(Ordering::Relaxed) {
                return false;
            }
            let mut payload = SearchProgressDto::from(snapshot);
            payload.job_id = progress_job_id.clone();
            let _ = app_for_task.emit("search_progress", payload);
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
        let _ = app_for_task.emit(
            "search_finished",
            SearchFinishedDto {
                job_id: job_id_for_task.clone(),
                cancelled,
                rows,
                error,
            },
        );
        if let Ok(mut guard) = jobs.lock() {
            guard.remove(&job_id_for_task);
        }
    });

    Ok(StartSearchResponseDto { job_id })
}

#[tauri::command]
pub fn cancel_search(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let guard = state
        .jobs
        .lock()
        .map_err(|_| AppError::new("failed to lock job registry"))?;
    if let Some(flag) = guard.get(&job_id) {
        flag.store(true, Ordering::Relaxed);
        return Ok(true);
    }
    Ok(false)
}

pub fn clamp_weapon_upgrade_request(
    request: &mut crate::dto::OptimizeRequestDto,
    state: &AppState,
) -> Result<(), AppError> {
    let Some(weapon_name) = request.weapon_name.as_deref() else {
        return Ok(());
    };
    let cap = weapon_upgrade_cap(&state.data, weapon_name, request.affinity.as_deref())?;
    request.max_upgrade = request.max_upgrade.min(cap);
    if let Some(fixed_upgrade) = request.fixed_upgrade {
        request.fixed_upgrade = Some(fixed_upgrade.min(cap));
    }
    Ok(())
}
