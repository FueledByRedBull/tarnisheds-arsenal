use std::sync::Arc;
use std::sync::atomic::Ordering;

use er_optimizer_core::model::COMBAT_STAT_COUNT;
use er_optimizer_core::{
    OptimizeRequest, OptimizeResult, PreparedLoadoutEvaluator, effective_str,
    prepare_loadout_evaluator_with_cancel,
};
use tauri::State;

use crate::commands::data::{
    weapon_disables_two_hand_bonus, weapon_forces_two_handing, weapon_requirements,
};
use crate::dto::{
    CombatStateDto, PathFinishedDto, PathJobStatusDto, PathMode, PathPreviewDto,
    PathPreviewRequestDto, PathProgressDto, PathStepDto, StartPathPreviewRequestDto,
    StartSearchResponseDto, lock_request_to_stats, set_min_combat_stats, validate_levels_ahead,
    validate_path_batch,
};
use crate::errors::AppError;
use crate::{AppState, AsyncJobHandle, CancelFlag, ProfileData};

#[tauri::command]
pub fn start_path_preview(
    request: StartPathPreviewRequestDto,
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
    if !state.profile(&profile_id)?.data.capabilities.class_budget {
        return Err(AppError::new(
            "Paths requires verified profile class budgets.",
        ));
    }
    let profile = Arc::clone(state.profile(&profile_id)?);
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
    let worker_status = Arc::clone(&status);
    let failed_job_id = job_id.clone();
    let publish_cancel = Arc::clone(&cancel_flag);
    crate::spawn_supervised_job(
        status,
        move || {
            run_path_preview_job(
                request.requests,
                &profile,
                job_id_for_task,
                || !cancel_flag.load(Ordering::Relaxed),
                |progress| {
                    if let Ok(mut guard) = worker_status.lock() {
                        guard.progress = Some(progress);
                    }
                },
            )
        },
        move |status, result| {
            publish_path_result(
                status,
                result,
                failed_job_id,
                publish_cancel.load(Ordering::Relaxed),
            );
        },
    );

    Ok(StartSearchResponseDto { job_id })
}

#[tauri::command]
pub fn cancel_path_preview(job_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    state
        .path_jobs
        .cancel(&job_id, |status| status.finished.is_some())
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

fn run_path_preview_job(
    requests: Vec<PathPreviewRequestDto>,
    profile: &ProfileData,
    job_id: String,
    mut should_continue: impl FnMut() -> bool + Send,
    mut publish: impl FnMut(PathProgressDto),
) -> PathJobStatusDto {
    // Completed levels are work units; reserve one unit for successful batch completion.
    let mut progress = PathProgressDto {
        job_id: job_id.clone(),
        checked: 0,
        total: requests.iter().map(path_level_count).sum::<u64>() + 1,
        title: requests
            .first()
            .map_or_else(String::new, |lane| lane.title.clone()),
        level: requests.first().map_or(0, |lane| lane.base.character_level),
    };
    publish(progress.clone());
    let mut last_published = std::time::Instant::now();
    let mut paths = Vec::new();
    let mut error = None;
    let mut cancelled = false;
    for lane in requests {
        if !should_continue() {
            cancelled = true;
            break;
        }
        let planned_levels = path_level_count(&lane);
        let title = lane.title.clone();
        let mut completed_levels = 0;
        match build_path_preview_inner(lane, profile, &mut should_continue, |level| {
            completed_levels += 1;
            progress.checked += 1;
            progress.title.clone_from(&title);
            progress.level = level;
            if last_published.elapsed() >= std::time::Duration::from_millis(50) {
                publish(progress.clone());
                last_published = std::time::Instant::now();
            }
        }) {
            Ok(path) => {
                // A lane with no reachable next allocation completes without inventing levels.
                progress.total -= planned_levels - completed_levels;
                paths.push(path);
            }
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
    cancelled |= !should_continue();
    if cancelled || error.is_some() {
        paths.clear();
    } else {
        progress.checked += 1;
    }
    PathJobStatusDto {
        progress: Some(progress),
        finished: Some(PathFinishedDto {
            job_id,
            cancelled,
            paths,
            error,
        }),
    }
}

fn path_level_count(request: &PathPreviewRequestDto) -> u64 {
    u64::from(
        request
            .base
            .character_level
            .saturating_add(request.levels_ahead)
            - request.base.character_level,
    ) + 1
}

fn publish_path_result(
    status: &mut PathJobStatusDto,
    result: Result<PathJobStatusDto, String>,
    job_id: String,
    cancelled: bool,
) {
    match result {
        Ok(mut finished) => {
            if cancelled {
                if let Some(result) = &mut finished.finished {
                    result.cancelled = true;
                    result.paths.clear();
                }
                if let Some(progress) = &mut finished.progress
                    && progress.checked == progress.total
                {
                    progress.checked = progress.checked.saturating_sub(1);
                }
            }
            *status = finished;
        }
        Err(message) => {
            status.finished = Some(PathFinishedDto {
                job_id,
                cancelled: false,
                paths: Vec::new(),
                error: Some(message),
            });
        }
    }
}

fn build_path_preview_inner(
    request: PathPreviewRequestDto,
    profile: &ProfileData,
    mut should_continue: impl FnMut() -> bool + Send,
    mut level_complete: impl FnMut(u16),
) -> Result<PathPreviewDto, AppError> {
    if profile.data_manifest.profile.id != request.base.profile_id {
        return Err(AppError::new(format!(
            "Unknown game profile {:?}. Reload the catalog and choose an available profile.",
            request.base.profile_id
        )));
    }
    validate_levels_ahead(request.levels_ahead)?;
    if request.mode == PathMode::OptimumEnvelope {
        return build_optimum_envelope(request, profile, should_continue, level_complete);
    }
    let start_state = request.solved.stats;
    let target_level = request
        .base
        .character_level
        .saturating_add(request.levels_ahead);
    if !should_continue() {
        return Err(AppError::new("cancelled"));
    }
    let evaluator = prepare_path_evaluator(&request, target_level, profile, &mut should_continue)?;
    let first = evaluate_step(
        &request.base,
        &request.solved.weapon_name,
        &request.solved.affinity,
        request.solved.aow_name.as_deref(),
        request.solved.upgrade,
        request.solved.is_somber,
        request.base.character_level,
        start_state,
        None,
        profile,
        &evaluator,
        &mut should_continue,
    )?;
    let mut steps = vec![first.dto];
    level_complete(request.base.character_level);

    if !should_continue() {
        return Err(AppError::new("cancelled"));
    }
    let target = path_target_build(&request, target_level, &evaluator, &mut should_continue)?;
    if !should_continue() {
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
    for delta in 1..=target_level - request.base.character_level {
        let level = request.base.character_level.saturating_add(delta);
        let Some(next) = choose_next_step(
            &request,
            level,
            current_state,
            target.stats,
            profile,
            &evaluator,
            &mut should_continue,
        )?
        else {
            break;
        };
        current_state = next.dto.stats;
        steps.push(next.dto);
        level_complete(level);
    }

    Ok(PathPreviewDto {
        title: request.title,
        solved: request.solved,
        steps,
    })
}

fn build_optimum_envelope(
    request: PathPreviewRequestDto,
    profile: &ProfileData,
    should_continue: impl FnMut() -> bool + Send,
    mut level_complete: impl FnMut(u16),
) -> Result<PathPreviewDto, AppError> {
    let first_level = request.base.character_level;
    let last_level = first_level.saturating_add(request.levels_ahead);
    let levels = (first_level..=last_level).collect::<Vec<_>>();
    let mut template = request.base.clone();
    set_path_loadout(
        &mut template,
        &request.solved.weapon_name,
        &request.solved.affinity,
        request.solved.aow_name.as_deref(),
        request.solved.upgrade,
        request.solved.is_somber,
    );
    template.lock_str = None;
    template.lock_dex = None;
    template.lock_int = None;
    template.lock_fai = None;
    template.lock_arc = None;
    let rows = crate::commands::optimize::run_level_range_inner_with_progress(
        template,
        &levels,
        profile,
        |level| {
            level_complete(level);
            true
        },
        should_continue,
    )?;
    let mut previous = request.solved.stats;
    let steps = rows
        .into_iter()
        .map(|entry| {
            let level = entry.level;
            let rows = entry.rows;
            let solved = rows.into_iter().next();
            let stats = solved.as_ref().map_or(previous, |row| CombatStateDto {
                str_stat: row.stats.str,
                dex: row.stats.dex,
                int_stat: row.stats.int,
                fai: row.stats.fai,
                arc: row.stats.arc,
            });
            let added_stat = describe_allocation_change(previous, stats);
            previous = stats;
            PathStepDto {
                level,
                stats,
                metric: solved.as_ref().map(|row| row.score),
                added_stat,
                requirement_gap: u16::from(solved.is_none()),
            }
        })
        .collect();
    Ok(PathPreviewDto {
        title: request.title,
        solved: request.solved,
        steps,
    })
}

fn describe_allocation_change(previous: CombatStateDto, next: CombatStateDto) -> Option<String> {
    let deltas = [
        (
            "str",
            i16::from(next.str_stat) - i16::from(previous.str_stat),
        ),
        ("dex", i16::from(next.dex) - i16::from(previous.dex)),
        (
            "int",
            i16::from(next.int_stat) - i16::from(previous.int_stat),
        ),
        ("fai", i16::from(next.fai) - i16::from(previous.fai)),
        ("arc", i16::from(next.arc) - i16::from(previous.arc)),
    ];
    let changed = deltas
        .iter()
        .filter(|(_, delta)| *delta != 0)
        .collect::<Vec<_>>();
    if changed.is_empty() {
        None
    } else if changed.len() == 1 && changed[0].1 == 1 {
        Some(changed[0].0.to_string())
    } else {
        Some("respec".to_string())
    }
}

fn prepare_path_evaluator<'a>(
    request: &PathPreviewRequestDto,
    target_level: u16,
    profile: &'a ProfileData,
    should_continue: &mut (impl FnMut() -> bool + Send),
) -> Result<PreparedLoadoutEvaluator<'a>, AppError> {
    let mut template = request.base.clone();
    template.character_level = target_level;
    set_path_loadout(
        &mut template,
        &request.solved.weapon_name,
        &request.solved.affinity,
        request.solved.aow_name.as_deref(),
        request.solved.upgrade,
        request.solved.is_somber,
    );
    let core_request = OptimizeRequest::try_from(&template)?;
    prepare_loadout_evaluator_with_cancel(&core_request, &profile.data, should_continue)
        .map_err(AppError::from)
}

fn path_target_build(
    request: &PathPreviewRequestDto,
    target_level: u16,
    evaluator: &PreparedLoadoutEvaluator<'_>,
    should_continue: &mut (impl FnMut() -> bool + Send),
) -> Result<Option<crate::dto::SolvedBuildDto>, AppError> {
    let mut target_request = request.base.clone();
    target_request.character_level = target_level;
    set_path_loadout(
        &mut target_request,
        &request.solved.weapon_name,
        &request.solved.affinity,
        request.solved.aow_name.as_deref(),
        request.solved.upgrade,
        request.solved.is_somber,
    );
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
        .evaluate_with_cancel(&core_request, should_continue)
        .map(|mut rows| rows.pop().map(crate::dto::SolvedBuildDto::from))
        .map_err(AppError::from)
}

fn choose_next_step(
    request: &PathPreviewRequestDto,
    level: u16,
    current_state: CombatStateDto,
    target_state: CombatStateDto,
    profile: &ProfileData,
    evaluator: &PreparedLoadoutEvaluator<'_>,
    should_continue: &mut (impl FnMut() -> bool + Send),
) -> Result<Option<EvaluatedPathStep>, AppError> {
    let mut candidates = Vec::new();
    for stat in ["str", "dex", "int", "fai", "arc"] {
        if combat_value(current_state, stat) >= combat_value(target_state, stat) {
            continue;
        }
        let Some(next_state) = add_point(current_state, stat) else {
            continue;
        };
        if !should_continue() {
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
            profile,
            evaluator,
            should_continue,
        )?);
    }
    if !should_continue() {
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
    profile: &ProfileData,
    evaluator: &PreparedLoadoutEvaluator<'_>,
    should_continue: &mut (impl FnMut() -> bool + Send),
) -> Result<EvaluatedPathStep, AppError> {
    let mut request = base.clone();
    request.character_level = level;
    set_path_loadout(
        &mut request,
        weapon_name,
        affinity,
        aow_name,
        upgrade,
        is_somber,
    );
    request.min_str = 0;
    request.min_dex = 0;
    request.min_int = 0;
    request.min_fai = 0;
    request.min_arc = 0;
    lock_request_to_stats(&mut request, stats);

    let core_request = OptimizeRequest::try_from(&request)?;
    let solved = evaluator
        .evaluate_with_cancel(&core_request, should_continue)
        .map_err(AppError::from)?
        .pop();
    let requirement_gap = if solved.is_some() {
        0
    } else {
        requirement_gap(base, weapon_name, Some(affinity), stats, profile)?
    };
    Ok(EvaluatedPathStep {
        dto: PathStepDto {
            level,
            stats,
            metric: solved.as_ref().map(|solved| solved.score),
            added_stat,
            requirement_gap,
        },
        solved,
    })
}

fn set_path_loadout(
    request: &mut crate::dto::OptimizeRequestDto,
    weapon_name: &str,
    affinity: &str,
    aow_name: Option<&str>,
    upgrade: u8,
    is_somber: bool,
) {
    request.weapon_name = Some(weapon_name.to_string());
    request.affinity = Some(affinity.to_string());
    request.aow_name = aow_name.map(str::to_string);
    request.weapon_type_key = None;
    request.somber_filter = "all".to_string();
    request.filters.entries.clear();
    request.set_exact_upgrade(upgrade, is_somber);
    request.top_k = 1;
}

fn requirement_gap(
    base: &crate::dto::OptimizeRequestDto,
    weapon_name: &str,
    affinity: Option<&str>,
    stats: CombatStateDto,
    profile: &ProfileData,
) -> Result<u16, AppError> {
    let reqs = weapon_requirements(&profile.catalog_index, weapon_name, affinity)?;
    let disables_bonus =
        weapon_disables_two_hand_bonus(&profile.catalog_index, weapon_name, affinity);
    let effective_str = effective_str(
        stats.str_stat,
        base.two_handing || weapon_forces_two_handing(&profile.catalog_index, weapon_name),
        disables_bonus,
    );
    Ok(u16::from(reqs[0]).saturating_sub(effective_str)
        + u16::from(reqs[1].saturating_sub(stats.dex))
        + u16::from(reqs[2].saturating_sub(stats.int_stat))
        + u16::from(reqs[3].saturating_sub(stats.fai))
        + u16::from(reqs[4].saturating_sub(stats.arc)))
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

struct EvaluatedPathStep {
    dto: PathStepDto,
    solved: Option<OptimizeResult>,
}

fn compare_steps(left: &EvaluatedPathStep, right: &EvaluatedPathStep) -> std::cmp::Ordering {
    match (&left.solved, &right.solved) {
        (Some(left), Some(right)) => left.compare_numeric(right),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
    .then_with(|| right.dto.requirement_gap.cmp(&left.dto.requirement_gap))
    .then_with(|| {
        stat_priority(right.dto.added_stat.as_deref())
            .cmp(&stat_priority(left.dto.added_stat.as_deref()))
    })
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

    #[test]
    fn supervised_path_worker_recovers_panics_before_and_during_calculation() {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, AtomicUsize};

        let state = crate::test_app_state();
        let seed = request(&state);
        for mode in [PathMode::NoRespec, PathMode::OptimumEnvelope] {
            for during_calculation in [false, true] {
                let registry = crate::JobRegistry::new("path");
                let status = Arc::new(Mutex::new(PathJobStatusDto {
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
                        |status| status.finished.is_some(),
                    )
                    .unwrap();
                let profile = Arc::clone(
                    state
                        .profile(er_optimizer_core::VANILLA_PROFILE_ID)
                        .unwrap(),
                );
                let mut lane = seed.clone();
                lane.mode = mode;
                let injected = Arc::new(AtomicBool::new(false));
                let worker_injected = Arc::clone(&injected);
                let publications = Arc::new(AtomicUsize::new(0));
                let published = Arc::clone(&publications);
                let worker_status = Arc::clone(&status);
                let supervisor = crate::spawn_supervised_job(
                    Arc::clone(&status),
                    move || {
                        if !during_calculation {
                            worker_injected.store(true, Ordering::Relaxed);
                            panic!("injected before Paths calculation");
                        }
                        let mut checkpoints = 0;
                        run_path_preview_job(
                            vec![lane],
                            &profile,
                            "first".into(),
                            || {
                                checkpoints += 1;
                                if checkpoints == 4 {
                                    worker_injected.store(true, Ordering::Relaxed);
                                    panic!("injected inside Paths calculation");
                                }
                                true
                            },
                            |progress| worker_status.lock().unwrap().progress = Some(progress),
                        )
                    },
                    move |status, outcome| {
                        published.fetch_add(1, Ordering::Relaxed);
                        publish_path_result(status, outcome, "first".into(), false);
                    },
                );
                tauri::async_runtime::block_on(supervisor).unwrap();
                assert!(injected.load(Ordering::Relaxed));
                assert_eq!(publications.load(Ordering::Relaxed), 1);
                let terminal = status.lock().unwrap();
                let finished = terminal.finished.as_ref().unwrap();
                assert!(finished.error.is_some());
                assert!(!finished.cancelled);
                assert!(finished.paths.is_empty());
                if during_calculation {
                    assert_eq!(terminal.progress.as_ref().unwrap().checked, 0);
                }
                drop(terminal);
                registry
                    .insert_if_idle(
                        "restart".into(),
                        AsyncJobHandle {
                            cancel,
                            status: Arc::new(Mutex::new(PathJobStatusDto {
                                progress: None,
                                finished: None,
                            })),
                        },
                        |status| status.finished.is_some(),
                    )
                    .expect("joined failed Paths job permits restart");
            }
        }
    }

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
            mode: PathMode::NoRespec,
        }
    }

    #[test]
    fn invalid_path_modes_and_unknown_requirements_fail_explicitly() {
        assert!(serde_json::from_str::<PathMode>("\"unknown\"").is_err());
        assert_eq!(
            serde_json::to_string(&PathMode::NoRespec).unwrap(),
            "\"no_respec\""
        );
        let state = crate::test_app_state();
        let request = request(&state);
        assert!(
            requirement_gap(
                &request.base,
                "Unknown weapon",
                None,
                request.solved.stats,
                state
                    .profile(er_optimizer_core::VANILLA_PROFILE_ID)
                    .expect("Vanilla profile exists")
            )
            .is_err()
        );
    }

    #[test]
    fn packaged_snapshot_executes_real_path_command_logic() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .expect("Vanilla profile exists");
        let path = build_path_preview_inner(request(&state), profile, || true, |_| {})
            .expect("real path command succeeds");
        assert_eq!(path.title, "Selected");
        assert!(!path.steps.is_empty());
    }

    #[test]
    fn optimum_envelope_solves_each_level_independently() {
        let state = crate::test_app_state();
        let mut envelope_request = request(&state);
        envelope_request.mode = PathMode::OptimumEnvelope;
        envelope_request.levels_ahead = 2;
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .expect("Vanilla profile exists");
        let path = build_path_preview_inner(envelope_request, profile, || true, |_| {})
            .expect("optimum envelope succeeds");
        assert_eq!(path.steps.len(), 3);
        assert!(
            path.steps
                .windows(2)
                .all(|steps| steps[0].level + 1 == steps[1].level)
        );
    }

    #[test]
    fn path_modes_ignore_discovery_filters_for_selected_loadout() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .expect("Vanilla profile exists");
        let mut path_request = request(&state);
        let selected_weapon = profile
            .data
            .weapons
            .iter()
            .find(|weapon| {
                weapon
                    .name
                    .eq_ignore_ascii_case(&path_request.solved.weapon_name)
                    && weapon
                        .affinity
                        .eq_ignore_ascii_case(&path_request.solved.affinity)
            })
            .expect("selected loadout exists in profile");
        let conflicting_weapon = profile
            .data
            .weapons
            .iter()
            .find(|weapon| {
                weapon.type_filter_id() != selected_weapon.type_filter_id()
                    && weapon.affinity_filter_id() != selected_weapon.affinity_filter_id()
            })
            .expect("profile has a different type and affinity");
        path_request.base.filters.entries = vec![
            crate::dto::StableFilterEntryDto {
                dimension: "weapon_type".to_string(),
                id: conflicting_weapon.type_filter_id(),
                mode: "include".to_string(),
            },
            crate::dto::StableFilterEntryDto {
                dimension: "affinity".to_string(),
                id: conflicting_weapon.affinity_filter_id(),
                mode: "include".to_string(),
            },
        ];

        for mode in [PathMode::NoRespec, PathMode::OptimumEnvelope] {
            let mut lane = path_request.clone();
            lane.mode = mode;
            lane.levels_ahead = 2;
            let path = build_path_preview_inner(lane, profile, || true, |_| {})
                .expect("fixed selected loadout remains evaluable");
            assert!(
                path.steps.iter().all(|step| step.metric.is_some()),
                "{mode:?} should ignore discovery filters for the selected loadout"
            );
        }
    }

    #[test]
    fn path_progress_counts_completed_levels_only() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let mut completed = Vec::new();
        let path = build_path_preview_inner(
            request(&state),
            profile,
            || true,
            |level| {
                completed.push(level);
            },
        )
        .unwrap();
        assert_eq!(completed.len(), 2);
        assert_eq!(completed, [9, 10]);
        assert_eq!(path.steps.len(), 2);
    }

    #[test]
    fn envelope_cancellation_reaches_inner_optimizer() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let mut lane = request(&state);
        lane.mode = PathMode::OptimumEnvelope;
        let mut polls = 0;
        let result = build_path_preview_inner(
            lane,
            profile,
            || {
                polls += 1;
                polls < 3
            },
            |_| panic!("cancel must arrive before the first completed level"),
        );
        assert!(
            result.is_err(),
            "must cancel inside optimizer before completing two levels"
        );
        assert_eq!(polls, 3);
    }

    #[test]
    fn envelope_cancellation_before_first_level_completion_returns_no_partial_path() {
        use std::sync::atomic::AtomicUsize;

        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let mut lane = request(&state);
        lane.mode = PathMode::OptimumEnvelope;
        lane.base.character_level = 150;
        lane.levels_ahead = 0;
        let polls = AtomicUsize::new(0);
        let mut polls_at_completion = 0;
        build_path_preview_inner(
            lane.clone(),
            profile,
            || {
                polls.fetch_add(1, Ordering::Relaxed);
                true
            },
            |_| polls_at_completion = polls.load(Ordering::Relaxed),
        )
        .unwrap();
        assert!(polls_at_completion > 10);

        // Cancel at the final core checkpoint observed before this level completed.
        // This deliberately does not assume which optimizer phase owns that checkpoint.
        polls.store(0, Ordering::Relaxed);
        let error = build_path_preview_inner(
            lane,
            profile,
            || polls.fetch_add(1, Ordering::Relaxed) + 1 < polls_at_completion,
            |_| panic!("cancelled level must not publish completion"),
        )
        .unwrap_err();
        assert_eq!(error.message, "cancelled");
        assert_eq!(polls.load(Ordering::Relaxed), polls_at_completion);
    }

    #[test]
    fn path_cancellation_after_worker_completion_clears_success_at_publication() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let completed = run_path_preview_job(
            vec![request(&state)],
            profile,
            "late-cancel".to_string(),
            || true,
            |_| {},
        );
        assert!(!completed.finished.as_ref().unwrap().paths.is_empty());
        let mut status = PathJobStatusDto {
            progress: None,
            finished: None,
        };
        publish_path_result(&mut status, Ok(completed), "late-cancel".to_string(), true);
        let finished = status.finished.unwrap();
        assert!(finished.cancelled);
        assert!(finished.paths.is_empty());
        let progress = status.progress.unwrap();
        assert_eq!(progress.checked, 2);
        assert_eq!(progress.total, 3);
    }

    #[test]
    fn path_batch_progress_is_bounded_and_finishes_for_both_modes_and_horizons() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let seed = request(&state);
        for mode in [PathMode::NoRespec, PathMode::OptimumEnvelope] {
            for horizon in [0, 1, 200] {
                for lanes in [1, 2] {
                    let mut lane = seed.clone();
                    lane.mode = mode;
                    lane.levels_ahead = horizon;
                    let mut snapshots = Vec::new();
                    let mut polls = 0;
                    let status = run_path_preview_job(
                        vec![lane; lanes],
                        profile,
                        "path-test".to_string(),
                        || {
                            polls += 1;
                            true
                        },
                        |snapshot| snapshots.push(snapshot),
                    );
                    let finished = status.finished.unwrap();
                    assert!(!finished.cancelled);
                    assert!(finished.error.is_none(), "{:?}", finished.error);
                    assert_eq!(finished.paths.len(), lanes);
                    let terminal = status.progress.unwrap();
                    assert_eq!(terminal.checked, terminal.total);
                    assert_eq!(terminal.total, u64::from(horizon + 1) * lanes as u64 + 1);
                    assert!(
                        snapshots
                            .iter()
                            .all(|snapshot| snapshot.checked < snapshot.total)
                    );
                    assert_eq!(snapshots[0].checked, 0);
                    assert!(snapshots.len() < polls);
                    assert!(snapshots.len() <= terminal.checked as usize);
                    snapshots.push(terminal);
                    assert!(
                        snapshots
                            .windows(2)
                            .all(|pair| pair[0].checked <= pair[1].checked)
                    );
                }
            }
        }
    }

    #[test]
    fn path_batch_errors_and_cancellation_never_publish_partial_success() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let seed = request(&state);
        for mode in [PathMode::NoRespec, PathMode::OptimumEnvelope] {
            let mut lane = seed.clone();
            lane.mode = mode;
            let mut invalid = lane.clone();
            invalid.base.profile_id = "unknown".to_string();
            let status = run_path_preview_job(
                vec![lane.clone(), invalid],
                profile,
                "error".to_string(),
                || true,
                |_| {},
            );
            let finished = status.finished.unwrap();
            assert!(finished.error.is_some());
            assert!(finished.paths.is_empty());
            let progress = status.progress.unwrap();
            assert_eq!(progress.checked, 2);
            assert!(progress.checked < progress.total);

            let mut polls = 0;
            let mut snapshots = Vec::new();
            let status = run_path_preview_job(
                vec![lane; 2],
                profile,
                "cancelled".to_string(),
                || {
                    polls += 1;
                    polls < 5
                },
                |snapshot| snapshots.push(snapshot),
            );
            let finished = status.finished.unwrap();
            assert!(finished.cancelled);
            assert!(finished.paths.is_empty());
            assert!(finished.error.is_none());
            let progress = status.progress.unwrap();
            assert_eq!(progress.checked, 0);
            assert!(progress.checked < progress.total);
            assert_eq!(snapshots.len(), 1);
        }
    }

    #[test]
    fn path_batch_progress_counts_only_reachable_steps() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let mut lane = request(&state);
        lane.solved.weapon_name = "Giant-Crusher".to_string();
        lane.solved.affinity = "Standard".to_string();
        lane.solved.aow_name = None;
        lane.levels_ahead = 2;
        let status = run_path_preview_job(
            vec![lane],
            profile,
            "unreachable".to_string(),
            || true,
            |_| {},
        );
        let finished = status.finished.unwrap();
        assert!(finished.error.is_none(), "{:?}", finished.error);
        assert_eq!(finished.paths[0].steps.len(), 1);
        assert!(finished.paths[0].steps[0].metric.is_none());
        let progress = status.progress.unwrap();
        assert_eq!(progress.checked, 2);
        assert_eq!(progress.total, 2);
    }

    #[test]
    fn path_batch_progress_handles_capped_stats_and_unspendable_horizons() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap();
        let mut seed = request(&state);
        seed.base.character_level = 452;
        seed.solved.stats = CombatStateDto {
            str_stat: 99,
            dex: 99,
            int_stat: 99,
            fai: 99,
            arc: 99,
        };
        for mode in [PathMode::NoRespec, PathMode::OptimumEnvelope] {
            for horizon in [0, 1] {
                let mut lane = seed.clone();
                lane.mode = mode;
                lane.levels_ahead = horizon;
                let status = run_path_preview_job(
                    vec![lane],
                    profile,
                    "capped".to_string(),
                    || true,
                    |_| {},
                );
                let finished = status.finished.unwrap();
                let progress = status.progress.unwrap();
                if horizon == 0 {
                    assert!(finished.error.is_none(), "{:?}", finished.error);
                    assert_eq!(progress.checked, progress.total);
                } else {
                    assert!(finished.error.is_some());
                    assert!(finished.paths.is_empty());
                    assert!(progress.checked < progress.total);
                }
            }
        }
    }

    #[test]
    fn real_path_command_honors_cancellation() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .expect("Vanilla profile exists");
        let error = build_path_preview_inner(request(&state), profile, || false, |_| {})
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
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .expect("Vanilla profile exists");
        let mut polls = 0_usize;
        let error = build_path_preview_inner(
            nested_request,
            profile,
            || {
                polls += 1;
                polls < cancel_after
            },
            |_| {},
        )
        .expect_err("nested path cancellation must not return a partial path");
        assert_eq!(error.message, "cancelled");
        assert_eq!(polls, cancel_after);
    }

    #[test]
    #[ignore = "release-mode workflow benchmark"]
    fn workflow_benchmark_paths() {
        let state = crate::test_app_state();
        let profile = state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .expect("Vanilla profile exists");
        let repeats = std::env::var("ER_BENCH_REPEATS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(3)
            .max(1);
        let fixtures = benchmark_path_fixtures(&state);
        for mode in [PathMode::NoRespec, PathMode::OptimumEnvelope] {
            for horizon in [10_u16, 50, 200] {
                for lanes in [1_usize, 2] {
                    let requests = fixtures[..lanes]
                        .iter()
                        .cloned()
                        .map(|mut request| {
                            request.levels_ahead = horizon;
                            request.mode = mode;
                            request
                        })
                        .collect::<Vec<_>>();
                    let mut durations = Vec::with_capacity(repeats);
                    let mut expected_results = None;
                    for sample in 0..=repeats {
                        let (elapsed, paths) = measure_benchmark_paths(requests.clone(), profile);
                        assert!(
                            paths
                                .iter()
                                .all(|path| path.steps.len() == usize::from(horizon) + 1)
                        );
                        let results =
                            serde_json::to_value(&paths).expect("serialize complete paths");
                        if let Some(expected) = &expected_results {
                            assert_eq!(
                                &results, expected,
                                "benchmark Paths output changed across samples"
                            );
                        } else {
                            expected_results = Some(results);
                        }
                        if sample > 0 {
                            durations.push(elapsed.as_secs_f64() * 1_000.0);
                        }
                    }
                    let mut sorted = durations.clone();
                    sorted.sort_by(f64::total_cmp);
                    println!(
                        "WORKFLOW_BENCH {}",
                        serde_json::json!({
                            "workflow": "paths",
                            "model_version": profile.data.model_version,
                            "mode": mode,
                            "timing_scope": "paths_only",
                            "horizon": horizon,
                            "lanes": lanes,
                            "warmups": 1,
                            "repeats": repeats,
                            "requests": requests,
                            "results": expected_results.expect("warmup produces paths"),
                            "median_ms": sorted[sorted.len() / 2],
                            "best_ms": sorted[0],
                            "worst_ms": sorted[sorted.len() - 1],
                            "samples_ms": durations,
                        })
                    );
                }
            }
        }
    }

    fn benchmark_path_fixtures(state: &AppState) -> Vec<PathPreviewRequestDto> {
        [
            ("Uchigatana", "Keen", "Unsheathe"),
            ("Bloodhound's Fang", "Standard", "Bloodhound's Finesse"),
        ]
        .into_iter()
        .map(|(weapon, affinity, ash)| {
            let mut base = crate::test_optimize_request();
            base.character_level = 80;
            base.standard_max_upgrade = Some(25);
            base.somber_max_upgrade = Some(10);
            base.weapon_name = Some(weapon.to_string());
            base.affinity = Some(affinity.to_string());
            base.aow_name = Some(ash.to_string());
            let solved = run_search_inner_with_cancel(base.clone(), state, || true)
                .expect("benchmark fixture search succeeds")
                .pop()
                .expect("benchmark fixture exists");
            PathPreviewRequestDto {
                base,
                solved,
                levels_ahead: 1,
                title: weapon.to_string(),
                mode: PathMode::NoRespec,
            }
        })
        .collect()
    }

    // The timer accepts already-solved, owned requests. Seed searches and request
    // cloning cannot enter this interval; serialization and repeat checks follow it.
    fn measure_benchmark_paths(
        requests: Vec<PathPreviewRequestDto>,
        profile: &ProfileData,
    ) -> (std::time::Duration, Vec<PathPreviewDto>) {
        let started = std::time::Instant::now();
        let paths = requests
            .into_iter()
            .map(|request| {
                build_path_preview_inner(request, profile, || true, |_| {})
                    .expect("benchmark path succeeds")
            })
            .collect();
        (started.elapsed(), paths)
    }

    #[test]
    fn benchmark_path_fixtures_cover_distinct_loadouts_in_both_modes() {
        let state = crate::test_app_state();
        let profile = state.profile("vanilla").unwrap();
        let fixtures = benchmark_path_fixtures(&state);
        assert_eq!(fixtures.len(), 2);
        assert_ne!(fixtures[0].solved.weapon_id, fixtures[1].solved.weapon_id);
        assert_ne!(fixtures[0].solved.upgrade, fixtures[1].solved.upgrade);
        for mode in [PathMode::NoRespec, PathMode::OptimumEnvelope] {
            let requests = fixtures
                .iter()
                .cloned()
                .map(|mut request| {
                    request.mode = mode;
                    request
                })
                .collect();
            let (_, paths) = measure_benchmark_paths(requests, profile);
            for (path, fixture) in paths.iter().zip(&fixtures) {
                assert_eq!(path.solved.weapon_id, fixture.solved.weapon_id);
                assert_eq!(path.steps.len(), 2);
                assert_eq!(path.steps[0].level, 80);
                assert_eq!(path.steps[1].level, 81);
            }
        }
    }
}
