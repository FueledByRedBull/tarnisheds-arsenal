use std::sync::Arc;
use std::sync::atomic::Ordering;

use er_optimizer_core::effective_str;
use er_optimizer_core::model::COMBAT_STAT_COUNT;
use tauri::{AppHandle, State};

use crate::commands::data::{weapon_disables_two_hand_bonus, weapon_requirements};
use crate::commands::optimize::run_search_inner;
use crate::dto::{
    CombatStateDto, PathFinishedDto, PathJobStatusDto, PathPreviewDto, PathPreviewRequestDto,
    PathProgressDto, PathStepDto, StartPathPreviewRequestDto, StartSearchResponseDto,
    lock_request_to_stats, metric_for_objective, parse_objective, set_min_combat_stats,
    validate_levels_ahead, validate_path_batch,
};
use crate::errors::AppError;
use crate::{AppState, AsyncJobHandle, CancelFlag, JobRegistry};

#[tauri::command]
pub fn build_path_preview(
    request: PathPreviewRequestDto,
    state: State<'_, AppState>,
) -> Result<PathPreviewDto, AppError> {
    validate_levels_ahead(request.levels_ahead)?;
    let start_state = request.solved.stats;
    let mut steps = vec![evaluate_step(
        &request.base,
        &request.solved.weapon_name,
        &request.solved.affinity,
        request.solved.aow_name.as_deref(),
        request.solved.upgrade,
        request.solved.is_somber,
        request.base.character_level,
        start_state,
        None,
        &state,
    )?];

    let target_level = request
        .base
        .character_level
        .saturating_add(request.levels_ahead);
    let target = path_target_build(&request, target_level, &state)?;
    let Some(target) = target else {
        return Ok(PathPreviewDto {
            title: request.title,
            solved: request.solved,
            steps,
        });
    };

    let mut current_state = start_state;
    for delta in 1..=request.levels_ahead {
        let level = request.base.character_level.saturating_add(delta);
        let Some(next) = choose_next_step(&request, level, current_state, target.stats, &state)?
        else {
            break;
        };
        current_state = next.stats;
        steps.push(next);
    }

    Ok(PathPreviewDto {
        title: request.title,
        solved: request.solved,
        steps,
    })
}

#[tauri::command]
pub fn start_path_preview(
    request: StartPathPreviewRequestDto,
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    validate_path_batch(request.requests.len())?;
    for lane in &request.requests {
        validate_levels_ahead(lane.levels_ahead)?;
    }
    let data = Arc::clone(&state.data);
    let catalog_index = Arc::clone(&state.catalog_index);
    let data_manifest = state.data_manifest.clone();
    let job_number = state.next_job.fetch_add(1, Ordering::Relaxed);
    let job_id = format!("path-{job_number}");
    let cancel_flag: CancelFlag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let status = Arc::new(std::sync::Mutex::new(PathJobStatusDto {
        progress: None,
        finished: None,
    }));
    state.path_jobs.insert_if_idle(
        job_id.clone(),
        AsyncJobHandle {
            cancel: Arc::clone(&cancel_flag),
            status: Arc::clone(&status),
        },
        |status| status.finished.is_some(),
    )?;

    let job_id_for_task = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let task_state = AppState {
            data,
            catalog_index,
            data_manifest,
            search_jobs: Arc::new(JobRegistry::new("search")),
            path_jobs: Arc::new(JobRegistry::new("path")),
            affinity_jobs: Arc::new(JobRegistry::new("affinity watch")),
            next_job: Default::default(),
        };
        let total = request.requests.len().max(1) as u64;
        let mut paths = Vec::new();
        let mut error = None;
        let mut cancelled = false;
        for (idx, lane) in request.requests.into_iter().enumerate() {
            if cancel_flag.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            let progress = PathProgressDto {
                job_id: job_id_for_task.clone(),
                checked: idx as u64,
                total,
                title: lane.title.clone(),
                level: lane.base.character_level,
            };
            if let Ok(mut guard) = status.lock() {
                guard.progress = Some(progress.clone());
            }
            match build_path_preview_inner(lane, &task_state) {
                Ok(path) => paths.push(path),
                Err(err) => {
                    error = Some(err.message);
                    break;
                }
            }
        }
        let finished = PathFinishedDto {
            job_id: job_id_for_task.clone(),
            cancelled,
            paths,
            error,
        };
        if let Ok(mut guard) = status.lock() {
            guard.finished = Some(finished.clone());
        }
    });

    Ok(StartSearchResponseDto { job_id })
}

#[tauri::command]
pub fn cancel_path_preview(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    state.path_jobs.cancel(&job_id)
}

#[tauri::command]
pub fn get_path_preview_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<PathJobStatusDto>, AppError> {
    state
        .path_jobs
        .status(&job_id, |status| status.finished.is_some())
}

fn build_path_preview_inner(
    request: PathPreviewRequestDto,
    state: &AppState,
) -> Result<PathPreviewDto, AppError> {
    validate_levels_ahead(request.levels_ahead)?;
    let start_state = request.solved.stats;
    let mut steps = vec![evaluate_step(
        &request.base,
        &request.solved.weapon_name,
        &request.solved.affinity,
        request.solved.aow_name.as_deref(),
        request.solved.upgrade,
        request.solved.is_somber,
        request.base.character_level,
        start_state,
        None,
        state,
    )?];

    let target_level = request
        .base
        .character_level
        .saturating_add(request.levels_ahead);
    let target = path_target_build(&request, target_level, state)?;
    let Some(target) = target else {
        return Ok(PathPreviewDto {
            title: request.title,
            solved: request.solved,
            steps,
        });
    };

    let mut current_state = start_state;
    for delta in 1..=request.levels_ahead {
        let level = request.base.character_level.saturating_add(delta);
        let Some(next) = choose_next_step(&request, level, current_state, target.stats, state)?
        else {
            break;
        };
        current_state = next.stats;
        steps.push(next);
    }

    Ok(PathPreviewDto {
        title: request.title,
        solved: request.solved,
        steps,
    })
}

fn path_target_build(
    request: &PathPreviewRequestDto,
    target_level: u16,
    state: &AppState,
) -> Result<Option<crate::dto::SolvedBuildDto>, AppError> {
    let mut target_request = request.base.clone();
    target_request.character_level = target_level;
    target_request.weapon_name = Some(request.solved.weapon_name.clone());
    target_request.affinity = Some(request.solved.affinity.clone());
    target_request.aow_name = request.solved.aow_name.clone();
    target_request.set_exact_upgrade(request.solved.upgrade, request.solved.is_somber);
    target_request.top_k = 1;
    target_request.weapon_type_key = None;
    target_request.somber_filter = "all".to_string();
    target_request.lock_str = None;
    target_request.lock_dex = None;
    target_request.lock_int = None;
    target_request.lock_fai = None;
    target_request.lock_arc = None;
    set_min_combat_stats(
        &mut target_request,
        floor_mins(&request.base, request.solved.stats),
    );
    run_search_inner(target_request, state).map(|mut rows| rows.pop())
}

fn choose_next_step(
    request: &PathPreviewRequestDto,
    level: u16,
    current_state: CombatStateDto,
    target_state: CombatStateDto,
    state: &AppState,
) -> Result<Option<PathStepDto>, AppError> {
    let mut candidates = Vec::new();
    for stat in ["str", "dex", "int", "fai", "arc"] {
        if combat_value(current_state, stat) >= combat_value(target_state, stat) {
            continue;
        }
        let Some(next_state) = add_point(current_state, stat) else {
            continue;
        };
        candidates.push(evaluate_step(
            &request.base,
            &request.solved.weapon_name,
            &request.solved.affinity,
            request.solved.aow_name.as_deref(),
            request.solved.upgrade,
            request.solved.is_somber,
            level,
            next_state,
            Some(stat.to_string()),
            state,
        )?);
    }
    candidates.sort_by(compare_steps);
    Ok(candidates.pop())
}

fn evaluate_step(
    base: &crate::dto::OptimizeRequestDto,
    weapon_name: &str,
    affinity: &str,
    aow_name: Option<&str>,
    upgrade: u8,
    is_somber: bool,
    level: u16,
    stats: CombatStateDto,
    added_stat: Option<String>,
    state: &AppState,
) -> Result<PathStepDto, AppError> {
    let mut request = base.clone();
    request.character_level = level;
    request.weapon_name = Some(weapon_name.to_string());
    request.affinity = Some(affinity.to_string());
    request.aow_name = aow_name.map(str::to_string);
    request.set_exact_upgrade(upgrade, is_somber);
    request.top_k = 1;
    request.weapon_type_key = None;
    request.somber_filter = "all".to_string();
    request.min_str = 0;
    request.min_dex = 0;
    request.min_int = 0;
    request.min_fai = 0;
    request.min_arc = 0;
    lock_request_to_stats(&mut request, stats);

    let solved = run_search_inner(request, state)?.pop();
    let objective = parse_objective(&base.objective)?;
    let requirement_gap = if solved.is_some() {
        0
    } else {
        requirement_gap(base, weapon_name, Some(affinity), stats, state)
    };
    Ok(PathStepDto {
        level,
        stats,
        metric: solved
            .as_ref()
            .map(|solved| metric_for_objective(solved, objective)),
        score: solved.as_ref().map(|solved| solved.score),
        added_stat,
        requirement_gap,
    })
}

fn requirement_gap(
    base: &crate::dto::OptimizeRequestDto,
    weapon_name: &str,
    affinity: Option<&str>,
    stats: CombatStateDto,
    state: &AppState,
) -> u16 {
    let Ok(reqs) = weapon_requirements(&state.catalog_index, weapon_name, affinity) else {
        return 999;
    };
    let disables_bonus =
        weapon_disables_two_hand_bonus(&state.catalog_index, weapon_name, affinity);
    let effective_str = effective_str(stats.str_stat, base.two_handing, disables_bonus);
    u16::from(reqs[0]).saturating_sub(effective_str)
        + u16::from(reqs[1].saturating_sub(stats.dex))
        + u16::from(reqs[2].saturating_sub(stats.int_stat))
        + u16::from(reqs[3].saturating_sub(stats.fai))
        + u16::from(reqs[4].saturating_sub(stats.arc))
}

fn floor_mins(
    base: &crate::dto::OptimizeRequestDto,
    state: CombatStateDto,
) -> [u8; COMBAT_STAT_COUNT] {
    [
        state.str_stat.max(base.min_str),
        state.dex.max(base.min_dex),
        state.int_stat.max(base.min_int),
        state.fai.max(base.min_fai),
        state.arc.max(base.min_arc),
    ]
}

fn add_point(state: CombatStateDto, stat: &str) -> Option<CombatStateDto> {
    if combat_value(state, stat) >= 99 {
        return None;
    }
    Some(CombatStateDto {
        str_stat: state.str_stat + u8::from(stat == "str"),
        dex: state.dex + u8::from(stat == "dex"),
        int_stat: state.int_stat + u8::from(stat == "int"),
        fai: state.fai + u8::from(stat == "fai"),
        arc: state.arc + u8::from(stat == "arc"),
    })
}

fn combat_value(state: CombatStateDto, stat: &str) -> u8 {
    match stat {
        "str" => state.str_stat,
        "dex" => state.dex,
        "int" => state.int_stat,
        "fai" => state.fai,
        "arc" => state.arc,
        _ => 0,
    }
}

fn compare_steps(left: &PathStepDto, right: &PathStepDto) -> std::cmp::Ordering {
    let left_key = step_key(left);
    let right_key = step_key(right);
    left_key
        .0
        .cmp(&right_key.0)
        .then_with(|| left_key.1.total_cmp(&right_key.1))
        .then_with(|| left_key.2.total_cmp(&right_key.2))
        .then_with(|| left_key.3.cmp(&right_key.3))
        .then_with(|| left_key.4.cmp(&right_key.4))
}

fn step_key(step: &PathStepDto) -> (u8, f32, f32, i16, i16) {
    (
        u8::from(step.metric.is_some() && step.score.is_some()),
        step.score.unwrap_or(0.0),
        step.metric.unwrap_or(0.0),
        -(step.requirement_gap as i16),
        -stat_priority(step.added_stat.as_deref()),
    )
}

fn stat_priority(stat: Option<&str>) -> i16 {
    match stat {
        Some("str") => 0,
        Some("dex") => 1,
        Some("int") => 2,
        Some("fai") => 3,
        Some("arc") => 4,
        _ => 5,
    }
}
