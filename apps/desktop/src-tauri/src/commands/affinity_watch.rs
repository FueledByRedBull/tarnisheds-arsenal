use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::atomic::Ordering as AtomicOrdering;

use tauri::{AppHandle, State};

use crate::commands::data::{affinities_for_weapon_inner, compatible_aow_names_inner};
use crate::commands::optimize::run_level_range_inner_with_progress;
use crate::dto::{
    AffinityBreakpointDto, AffinityWatchFinishedDto, AffinityWatchJobStatusDto,
    AffinityWatchLineDto, AffinityWatchPayloadDto, AffinityWatchPointDto, AffinityWatchProgressDto,
    AffinityWatchRequestDto, SolvedBuildDto, StartSearchResponseDto, metric_for_objective,
    parse_objective, validate_levels_ahead,
};
use crate::errors::AppError;
use crate::{AppState, AsyncJobHandle, CancelFlag, JobRegistry, LatestCancel};

#[tauri::command]
pub fn build_affinity_watch(
    request: AffinityWatchRequestDto,
    state: State<'_, AppState>,
) -> Result<AffinityWatchPayloadDto, AppError> {
    build_affinity_watch_inner(request, &state, |_| true, || true)
}

#[tauri::command]
pub fn start_affinity_watch(
    request: AffinityWatchRequestDto,
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StartSearchResponseDto, AppError> {
    validate_levels_ahead(request.levels_ahead)?;
    state.profile(&request.base.profile_id)?;
    let profiles = state.profiles.clone();
    let job_number = state.next_job.fetch_add(1, AtomicOrdering::Relaxed);
    let job_id = format!("affinity-{job_number}");
    let cancel_flag: CancelFlag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let status = Arc::new(std::sync::Mutex::new(AffinityWatchJobStatusDto {
        progress: None,
        finished: None,
    }));
    state.affinity_jobs.insert_if_idle(
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
            estimate_cancel: Arc::new(LatestCancel::new()),
            search_jobs: Arc::new(JobRegistry::new("search")),
            path_jobs: Arc::new(JobRegistry::new("path")),
            affinity_jobs: Arc::new(JobRegistry::new("affinity watch")),
            next_job: Default::default(),
        };
        let affinities = affinity_watch_affinities_for_profile(
            &request.solved,
            &task_state,
            &request.base.profile_id,
        );
        let total = (affinities.len() as u64).saturating_mul(u64::from(request.levels_ahead) + 1);
        let progress = AffinityWatchProgressDto {
            job_id: job_id_for_task.clone(),
            checked: 0,
            total: total.max(1),
            affinity: request.solved.affinity.clone(),
            level: request.base.character_level,
        };
        if let Ok(mut guard) = status.lock() {
            guard.progress = Some(progress.clone());
        }
        let (payload, error, cancelled) = if cancel_flag.load(AtomicOrdering::Relaxed) {
            (None, None, true)
        } else {
            match build_affinity_watch_inner(
                request,
                &task_state,
                |progress| {
                    if cancel_flag.load(AtomicOrdering::Relaxed) {
                        return false;
                    }
                    let mut payload = progress;
                    payload.job_id = job_id_for_task.clone();
                    if let Ok(mut guard) = status.lock() {
                        guard.progress = Some(payload.clone());
                    }
                    true
                },
                || !cancel_flag.load(AtomicOrdering::Relaxed),
            ) {
                Ok(payload) => (Some(payload), None, false),
                Err(err) if err.message == "cancelled" => (None, None, true),
                Err(err) => (None, Some(err.message), false),
            }
        };
        let finished = AffinityWatchFinishedDto {
            job_id: job_id_for_task.clone(),
            cancelled,
            payload,
            error,
        };
        if let Ok(mut guard) = status.lock() {
            guard.finished = Some(finished.clone());
        }
    });

    Ok(StartSearchResponseDto { job_id })
}

#[tauri::command]
pub fn cancel_affinity_watch(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    state.affinity_jobs.cancel(&job_id)
}

#[tauri::command]
pub fn get_affinity_watch_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AffinityWatchJobStatusDto>, AppError> {
    state
        .affinity_jobs
        .status(&job_id, |status| status.finished.is_some())
}

fn build_affinity_watch_inner(
    request: AffinityWatchRequestDto,
    state: &AppState,
    mut progress_cb: impl FnMut(AffinityWatchProgressDto) -> bool,
    mut should_continue: impl FnMut() -> bool + Send,
) -> Result<AffinityWatchPayloadDto, AppError> {
    validate_levels_ahead(request.levels_ahead)?;
    let objective = parse_objective(&request.base.objective)?;
    let affinities =
        affinity_watch_affinities_for_profile(&request.solved, state, &request.base.profile_id);
    let levels: Vec<u16> = (0..=request.levels_ahead)
        .map(|offset| request.base.character_level.saturating_add(offset))
        .collect();
    let total = (affinities.len() as u64)
        .saturating_mul(levels.len() as u64)
        .max(1);
    let mut checked = 0_u64;

    let mut lines = Vec::new();
    for affinity in affinities {
        let first_level = *levels.first().expect("affinity watch levels are non-empty");
        if !progress_cb(AffinityWatchProgressDto {
            job_id: String::new(),
            checked,
            total,
            affinity: affinity.clone(),
            level: first_level,
        }) {
            return Err(AppError::new("cancelled"));
        }
        let mut row_request = request.base.clone();
        row_request.weapon_name = Some(request.solved.weapon_name.clone());
        row_request.affinity = Some(affinity.clone());
        row_request.aow_name = request.solved.aow_name.clone();
        row_request.set_exact_upgrade(request.solved.upgrade, request.solved.is_somber);
        row_request.top_k = 1;
        row_request.weapon_type_key = None;
        row_request.somber_filter = "all".to_string();
        row_request.lock_str = None;
        row_request.lock_dex = None;
        row_request.lock_int = None;
        row_request.lock_fai = None;
        row_request.lock_arc = None;

        let level_rows = run_level_range_inner_with_progress(
            row_request,
            &levels,
            state,
            |level| {
                checked = checked.saturating_add(1);
                progress_cb(AffinityWatchProgressDto {
                    job_id: String::new(),
                    checked,
                    total,
                    affinity: affinity.clone(),
                    level,
                })
            },
            &mut should_continue,
        )?;
        let points: Vec<AffinityWatchPointDto> = level_rows
            .into_iter()
            .map(|(level, mut rows)| {
                let solved = rows.pop();
                let metric = solved
                    .as_ref()
                    .map(|solved| metric_for_objective(solved, objective));
                AffinityWatchPointDto {
                    level,
                    metric,
                    solved,
                }
            })
            .collect();
        let valid: Vec<&AffinityWatchPointDto> = points
            .iter()
            .filter(|point| point.metric.is_some() && point.solved.is_some())
            .collect();
        if valid.is_empty() {
            continue;
        }
        lines.push(AffinityWatchLineDto {
            affinity,
            start_metric: valid.first().and_then(|point| point.metric),
            end_metric: valid.last().and_then(|point| point.metric),
            final_build: valid.last().and_then(|point| point.solved.clone()),
            points,
        });
    }

    lines.sort_by(|left, right| compare_lines(right, left));
    let breakpoints = detect_breakpoints(&lines, &levels, objective);
    Ok(AffinityWatchPayloadDto { lines, breakpoints })
}

fn affinity_watch_affinities_for_profile(
    solved: &SolvedBuildDto,
    state: &AppState,
    profile_id: &str,
) -> Vec<String> {
    let Ok(profile) = state.profile(profile_id) else {
        return vec![solved.affinity.clone()];
    };
    let mut affinities = affinities_for_weapon_inner(&profile.catalog_index, &solved.weapon_name);
    if let Some(aow_name) = solved.aow_name.as_deref() {
        affinities.retain(|affinity| {
            compatible_aow_names_inner(
                &profile.catalog_index,
                Some(&solved.weapon_name),
                Some(affinity),
            )
            .iter()
            .any(|candidate| candidate == aow_name)
        });
    }
    if !affinities
        .iter()
        .any(|affinity| affinity == &solved.affinity)
    {
        affinities.push(solved.affinity.clone());
    }
    affinities
        .sort_by_key(|affinity| (affinity != &solved.affinity, affinity.to_ascii_lowercase()));
    affinities
}

fn detect_breakpoints(
    lines: &[AffinityWatchLineDto],
    levels: &[u16],
    objective: er_optimizer_core::OptimizeObjective,
) -> Vec<AffinityBreakpointDto> {
    let mut breakpoints = Vec::new();
    let mut leader_affinity: Option<String> = None;
    for level in levels {
        let mut contenders = Vec::new();
        for line in lines {
            if let Some(point) = line.points.iter().find(|point| point.level == *level)
                && let Some(solved) = point.solved.as_ref()
            {
                contenders.push(solved);
            }
        }
        let Some(leader) = contenders
            .into_iter()
            .max_by(|left, right| compare_solved(left, right, objective))
        else {
            continue;
        };
        if let Some(previous) = leader_affinity.as_ref()
            && previous != &leader.affinity
        {
            let outgoing = metric_at(lines, previous, *level);
            let incoming = metric_at(lines, &leader.affinity, *level);
            breakpoints.push(AffinityBreakpointDto {
                level: *level,
                outgoing_affinity: previous.clone(),
                incoming_affinity: leader.affinity.clone(),
                outgoing_metric: outgoing,
                incoming_metric: incoming,
            });
        }
        leader_affinity = Some(leader.affinity.clone());
    }
    breakpoints
}

fn metric_at(lines: &[AffinityWatchLineDto], affinity: &str, level: u16) -> Option<f32> {
    lines
        .iter()
        .find(|line| line.affinity == affinity)
        .and_then(|line| {
            line.points
                .iter()
                .find(|point| point.level == level)
                .and_then(|point| point.metric)
        })
}

fn compare_lines(left: &AffinityWatchLineDto, right: &AffinityWatchLineDto) -> Ordering {
    left.end_metric
        .unwrap_or(f32::NEG_INFINITY)
        .total_cmp(&right.end_metric.unwrap_or(f32::NEG_INFINITY))
        .then_with(
            || match (left.final_build.as_ref(), right.final_build.as_ref()) {
                (Some(left), Some(right)) => {
                    compare_solved(left, right, er_optimizer_core::OptimizeObjective::MaxAr)
                }
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
        )
}

fn compare_solved(
    left: &SolvedBuildDto,
    right: &SolvedBuildDto,
    objective: er_optimizer_core::OptimizeObjective,
) -> Ordering {
    metric_for_objective(left, objective)
        .total_cmp(&metric_for_objective(right, objective))
        .then_with(|| left.score.total_cmp(&right.score))
        .then_with(|| left.ar.total.total_cmp(&right.ar.total))
        .then_with(|| {
            left.aow_full_sequence_damage
                .total_cmp(&right.aow_full_sequence_damage)
        })
        .then_with(|| {
            left.aow_first_hit_damage
                .total_cmp(&right.aow_first_hit_damage)
        })
        .then_with(|| left.bleed_buildup.total_cmp(&right.bleed_buildup))
        .then_with(|| right.weapon_id.cmp(&left.weapon_id))
        .then_with(|| left.upgrade.cmp(&right.upgrade))
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::commands::optimize::run_search_inner;

    fn request(state: &AppState) -> AffinityWatchRequestDto {
        let base = crate::test_optimize_request();
        let solved = run_search_inner(base.clone(), state)
            .expect("seed search succeeds")
            .pop()
            .expect("seed build exists");
        AffinityWatchRequestDto {
            base,
            solved,
            levels_ahead: 0,
        }
    }

    #[test]
    fn packaged_snapshot_executes_real_affinity_command_logic() {
        let state = crate::test_app_state();
        let mut request = request(&state);
        request.levels_ahead = 2;
        let payload = build_affinity_watch_inner(request, &state, |_| true, || true)
            .expect("real affinity watch succeeds");
        assert!(!payload.lines.is_empty());
        assert!(payload.lines.iter().all(|line| line.points.len() == 3));
        assert!(payload.lines.iter().all(|line| {
            line.points
                .windows(2)
                .all(|pair| pair[0].level + 1 == pair[1].level)
        }));
    }

    #[test]
    fn real_affinity_command_honors_cancellation() {
        let state = crate::test_app_state();
        let error = build_affinity_watch_inner(request(&state), &state, |_| false, || true)
            .expect_err("cancelled affinity watch must fail closed");
        assert_eq!(error.message, "cancelled");
    }

    #[test]
    fn real_affinity_command_propagates_nested_cancellation_without_partial_payload() {
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
        let error = build_affinity_watch_inner(
            nested_request,
            &state,
            |_| true,
            || {
                polls += 1;
                polls < cancel_after
            },
        )
        .expect_err("nested affinity cancellation must not return a partial payload");
        assert_eq!(error.message, "cancelled");
        assert_eq!(polls, cancel_after);
    }

    #[test]
    #[ignore = "release-mode workflow benchmark"]
    fn workflow_benchmark_affinity_watch() {
        let state = crate::test_app_state();
        let repeats = std::env::var("ER_BENCH_REPEATS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(3)
            .max(1);
        for horizon in [10_u16, 50, 200] {
            let mut durations = Vec::with_capacity(repeats);
            let mut affinity_count = 0;
            for sample in 0..=repeats {
                let mut benchmark_request = request(&state);
                benchmark_request.levels_ahead = horizon;
                benchmark_request.solved.aow_name = None;
                let started = std::time::Instant::now();
                let payload =
                    build_affinity_watch_inner(benchmark_request, &state, |_| true, || true)
                        .expect("benchmark affinity watch succeeds");
                affinity_count = payload.lines.len();
                if sample > 0 {
                    durations.push(started.elapsed().as_secs_f64() * 1_000.0);
                }
            }
            durations.sort_by(f64::total_cmp);
            println!(
                "WORKFLOW_BENCH {}",
                serde_json::json!({
                    "workflow": "affinity_watch",
                    "horizon": horizon,
                    "affinities": affinity_count,
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
