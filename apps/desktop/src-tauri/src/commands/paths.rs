use std::sync::Arc;
use std::sync::atomic::Ordering;

use er_optimizer_core::model::COMBAT_STAT_COUNT;
use er_optimizer_core::{
    OptimizeRequest, PreparedLoadoutEvaluator, effective_str, prepare_loadout_evaluator_with_cancel,
};
use tauri::{AppHandle, State};

use crate::commands::data::{weapon_disables_two_hand_bonus, weapon_requirements};
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
    build_path_preview_inner(request, &state, |_| true)
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
    let Some(first_lane) = request.requests.first() else {
        return Err(AppError::new("At least one path lane is required."));
    };
    let profile_id = first_lane.base.profile_id.clone();
    if request
        .requests
        .iter()
        .any(|lane| lane.base.profile_id != profile_id)
    {
        return Err(AppError::new(
            "All path lanes must use the same game profile.",
        ));
    }
    state.profile(&profile_id)?;
    let profiles = state.profiles.clone();
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
            profiles,
            search_jobs: Arc::new(JobRegistry::new("search")),
            path_jobs: Arc::new(JobRegistry::new("path")),
            affinity_jobs: Arc::new(JobRegistry::new("affinity watch")),
            next_job: Default::default(),
        };
        let total = request
            .requests
            .iter()
            .map(|lane| 3_u64.saturating_add(u64::from(lane.levels_ahead).saturating_mul(6)))
            .sum::<u64>()
            .max(1);
        let mut checked = 0_u64;
        let mut paths = Vec::new();
        let mut error = None;
        let mut cancelled = false;
        for lane in request.requests {
            if cancel_flag.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            let title = lane.title.clone();
            match build_path_preview_inner(lane, &task_state, |level| {
                if cancel_flag.load(Ordering::Relaxed) {
                    return false;
                }
                let progress = PathProgressDto {
                    job_id: job_id_for_task.clone(),
                    checked,
                    total,
                    title: title.clone(),
                    level,
                };
                checked = checked.saturating_add(1);
                if let Ok(mut guard) = status.lock() {
                    guard.progress = Some(progress);
                }
                true
            }) {
                Ok(path) => paths.push(path),
                Err(err) if err.message == "cancelled" => {
                    cancelled = true;
                    break;
                }
                Err(err) => {
                    error = Some(err.message);
                    break;
                }
            }
        }
        if cancelled {
            paths.clear();
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
    mut continue_cb: impl FnMut(u16) -> bool + Send,
) -> Result<PathPreviewDto, AppError> {
    validate_levels_ahead(request.levels_ahead)?;
    let start_state = request.solved.stats;
    let target_level = request
        .base
        .character_level
        .saturating_add(request.levels_ahead);
    if !continue_cb(request.base.character_level) {
        return Err(AppError::new("cancelled"));
    }
    let evaluator = prepare_path_evaluator(&request, target_level, state, &mut continue_cb)?;
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
        &evaluator,
        &mut continue_cb,
    )?];

    if !continue_cb(target_level) {
        return Err(AppError::new("cancelled"));
    }
    let target = path_target_build(&request, target_level, &evaluator, &mut continue_cb)?;
    if !continue_cb(target_level) {
        return Err(AppError::new("cancelled"));
    }
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
        let Some(next) = choose_next_step(
            &request,
            level,
            current_state,
            target.stats,
            state,
            &evaluator,
            &mut continue_cb,
        )?
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

fn prepare_path_evaluator<'a>(
    request: &PathPreviewRequestDto,
    target_level: u16,
    state: &'a AppState,
    continue_cb: &mut (impl FnMut(u16) -> bool + Send),
) -> Result<PreparedLoadoutEvaluator<'a>, AppError> {
    let mut template = request.base.clone();
    template.character_level = target_level;
    template.weapon_name = Some(request.solved.weapon_name.clone());
    template.affinity = Some(request.solved.affinity.clone());
    template.aow_name = request.solved.aow_name.clone();
    template.weapon_type_key = None;
    template.somber_filter = "all".to_string();
    template.set_exact_upgrade(request.solved.upgrade, request.solved.is_somber);
    template.top_k = 1;
    let core_request = OptimizeRequest::try_from(&template)?;
    let profile = state.profile(&request.base.profile_id)?;
    prepare_loadout_evaluator_with_cancel(&core_request, &profile.data, || {
        continue_cb(target_level)
    })
    .map_err(AppError::from)
}

fn path_target_build(
    request: &PathPreviewRequestDto,
    target_level: u16,
    evaluator: &PreparedLoadoutEvaluator<'_>,
    continue_cb: &mut (impl FnMut(u16) -> bool + Send),
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
    let core_request = OptimizeRequest::try_from(&target_request)?;
    evaluator
        .evaluate_with_cancel(&core_request, || continue_cb(target_level))
        .map(|mut rows| rows.pop().map(crate::dto::SolvedBuildDto::from))
        .map_err(AppError::from)
}

fn choose_next_step(
    request: &PathPreviewRequestDto,
    level: u16,
    current_state: CombatStateDto,
    target_state: CombatStateDto,
    state: &AppState,
    evaluator: &PreparedLoadoutEvaluator<'_>,
    continue_cb: &mut (impl FnMut(u16) -> bool + Send),
) -> Result<Option<PathStepDto>, AppError> {
    let mut candidates = Vec::new();
    for stat in ["str", "dex", "int", "fai", "arc"] {
        if combat_value(current_state, stat) >= combat_value(target_state, stat) {
            continue;
        }
        let Some(next_state) = add_point(current_state, stat) else {
            continue;
        };
        if !continue_cb(level) {
            return Err(AppError::new("cancelled"));
        }
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
            evaluator,
            continue_cb,
        )?);
    }
    if !continue_cb(level) {
        return Err(AppError::new("cancelled"));
    }
    candidates.sort_by(compare_steps);
    Ok(candidates.pop())
}

#[allow(clippy::too_many_arguments)]
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
    evaluator: &PreparedLoadoutEvaluator<'_>,
    continue_cb: &mut (impl FnMut(u16) -> bool + Send),
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

    let core_request = OptimizeRequest::try_from(&request)?;
    let solved = evaluator
        .evaluate_with_cancel(&core_request, || continue_cb(level))
        .map_err(AppError::from)?
        .pop()
        .map(crate::dto::SolvedBuildDto::from);
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
    let Ok(profile) = state.profile(&base.profile_id) else {
        return 999;
    };
    let Ok(reqs) = weapon_requirements(&profile.catalog_index, weapon_name, affinity) else {
        return 999;
    };
    let disables_bonus =
        weapon_disables_two_hand_bonus(&profile.catalog_index, weapon_name, affinity);
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

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::commands::optimize::run_search_inner_with_cancel;

    fn request(state: &AppState) -> PathPreviewRequestDto {
        let base = crate::test_optimize_request();
        let solved = run_search_inner_with_cancel(base.clone(), state, || true)
            .expect("seed search succeeds")
            .pop()
            .expect("seed build exists");
        PathPreviewRequestDto {
            base,
            solved,
            levels_ahead: 1,
            title: "Selected".to_string(),
        }
    }

    #[test]
    fn packaged_snapshot_executes_real_path_command_logic() {
        let state = crate::test_app_state();
        let path = build_path_preview_inner(request(&state), &state, |_| true)
            .expect("real path command succeeds");
        assert_eq!(path.title, "Selected");
        assert!(!path.steps.is_empty());
    }

    #[test]
    fn real_path_command_honors_cancellation() {
        let state = crate::test_app_state();
        let error = build_path_preview_inner(request(&state), &state, |_| false)
            .expect_err("cancelled path must fail closed");
        assert_eq!(error.message, "cancelled");
    }

    #[test]
    fn real_path_command_propagates_nested_cancellation_without_partial_success() {
        let state = crate::test_app_state();
        let mut nested_request = request(&state);
        nested_request.levels_ahead = 20;
        let cancel_after = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .expect("Vanilla profile exists")
            .data
            .weapons
            .len()
            + 8;
        let mut polls = 0_usize;
        let error = build_path_preview_inner(nested_request, &state, |_| {
            polls += 1;
            polls < cancel_after
        })
        .expect_err("nested path cancellation must not return a partial path");
        assert_eq!(error.message, "cancelled");
        assert_eq!(polls, cancel_after);
    }

    #[test]
    #[ignore = "release-mode workflow benchmark"]
    fn workflow_benchmark_paths() {
        let state = crate::test_app_state();
        let repeats = std::env::var("ER_BENCH_REPEATS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(3)
            .max(1);
        for horizon in [10_u16, 50, 200] {
            for lanes in [1_usize, 2] {
                let mut durations = Vec::with_capacity(repeats);
                for sample in 0..=repeats {
                    let started = std::time::Instant::now();
                    for lane in 0..lanes {
                        let mut lane_request = request(&state);
                        lane_request.levels_ahead = horizon;
                        lane_request.title = format!("Lane {}", lane + 1);
                        let path = build_path_preview_inner(lane_request, &state, |_| true)
                            .expect("benchmark path succeeds");
                        assert!(!path.steps.is_empty());
                    }
                    if sample > 0 {
                        durations.push(started.elapsed().as_secs_f64() * 1_000.0);
                    }
                }
                durations.sort_by(f64::total_cmp);
                println!(
                    "WORKFLOW_BENCH {}",
                    serde_json::json!({
                        "workflow": "paths",
                        "horizon": horizon,
                        "lanes": lanes,
                        "repeats": repeats,
                        "median_ms": durations[durations.len() / 2],
                        "best_ms": durations[0],
                        "worst_ms": durations[durations.len() - 1],
                        "samples_ms": durations,
                    })
                );
            }
        }
    }
}
