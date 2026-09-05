use std::sync::Arc;
use std::sync::atomic::Ordering;

#[cfg(test)]
use er_optimizer_core::optimize_with_cancel;
use er_optimizer_core::{
    GameData, OptimizeRequest, optimize, optimize_level_range_with_progress,
    optimize_prepared_with_progress, prepare_search_with_cancel,
    prepare_upgrade_series_evaluator_with_cancel,
};
use tauri::{AppHandle, State};

use crate::dto::{
    CombatStateDto, SearchFinishedDto, SearchJobStatusDto, SearchProgressDto, SolveBuildRequestDto,
    SolvedBuildDto, StartSearchResponseDto, UpgradePointDto, UpgradeSeriesRequestDto,
    lock_request_to_stats, metric_for_objective, parse_objective,
};
use crate::errors::AppError;
use crate::{AppState, AsyncJobHandle, CancelFlag};

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
    base.filters.entries.clear();
    base.lock_str = None;
    base.lock_dex = None;
    base.lock_int = None;
    base.lock_fai = None;
    base.lock_arc = None;
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
    let core_request = OptimizeRequest::try_from(&base)?;
    let evaluator = prepare_upgrade_series_evaluator_with_cancel(
        &core_request,
        &profile.data,
        &mut should_continue,
    )
    .map_err(AppError::from)?;
    Ok(evaluator
        .evaluate_with_cancel(&core_request, max_upgrade, &mut should_continue)
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
    let profile = state.profile(&request.profile_id)?;
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
