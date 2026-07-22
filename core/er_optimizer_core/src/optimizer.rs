use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::math::{
    apply_aow_attack_buffs, apply_aow_status_buffs, calculate_aow_damage, calculate_aow_routes,
    calculate_ar, calculate_status_buildup, class_by_name, compute_free_points, effective_str,
    meets_requirements,
};
use crate::model::{
    Aow, AowAttackRow, AowRouteResult, COMBAT_STAT_COUNT, DamageBreakdown, DamageType, GameData,
    STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, StatusBuildup, StatusEffectSource,
    Weapon,
};

mod types;
pub use types::*;
mod ranking;
use ranking::*;

#[derive(Clone, Debug)]
struct AowChoice<'a> {
    aow: Option<&'a Aow>,
    skill_id: Option<u16>,
    skill_name: Option<&'a str>,
    attack_rows: Vec<&'a AowAttackRow>,
}

#[derive(Clone, Copy, Debug)]
struct CombatConstraints {
    mins: [u8; COMBAT_STAT_COUNT],
    maxs: [u8; COMBAT_STAT_COUNT],
    remaining_free: u16,
}

#[derive(Clone, Debug)]
struct PreparedWeapon<'a> {
    weapon: &'a Weapon,
    aow_choices: Vec<AowChoice<'a>>,
    upgrades: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PreparedSearchGroup {
    prepared_idx: usize,
    search: RelevantStatSearch,
    aow_indices: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct PreparedSearchPlan<'a> {
    request: OptimizeRequest,
    data: &'a GameData,
    weapons: Arc<[PreparedWeapon<'a>]>,
    groups: Vec<PreparedSearchGroup>,
    fine_work_units: Vec<SearchWorkUnit>,
    serial_work_units: Vec<SearchWorkUnit>,
    estimate: SearchEstimate,
}

pub struct PreparedLoadoutEvaluator<'a> {
    template: OptimizeRequest,
    data: &'a GameData,
    weapons: Arc<[PreparedWeapon<'a>]>,
}

pub struct PreparedUpgradeSeriesEvaluator<'a> {
    template: OptimizeRequest,
    data: &'a GameData,
    weapons: Arc<[PreparedWeapon<'a>]>,
}

impl PreparedLoadoutEvaluator<'_> {
    pub fn evaluate_with_cancel<F>(
        &self,
        request: &OptimizeRequest,
        mut should_continue: F,
    ) -> Result<Vec<OptimizeResult>, String>
    where
        F: FnMut() -> bool + Send,
    {
        validate_reusable_loadout(&self.template, request)?;
        let constraints = build_combat_constraints(request)?;
        let plan = build_prepared_plan(
            request,
            self.data,
            constraints,
            Arc::clone(&self.weapons),
            &mut should_continue,
        )?;
        optimize_prepared_with_progress(&plan, 1_024, |_snapshot| should_continue())
    }
}

impl PreparedUpgradeSeriesEvaluator<'_> {
    pub fn evaluate_with_cancel<F>(
        &self,
        request: &OptimizeRequest,
        max_upgrade: u8,
        mut should_continue: F,
    ) -> Result<Vec<OptimizeResult>, String>
    where
        F: FnMut() -> bool + Send,
    {
        validate_reusable_loadout(&self.template, request)?;
        if request.locked_combat_stats.iter().any(Option::is_none) {
            return Err("upgrade series evaluation requires exact combat stats".to_string());
        }
        let constraints = build_combat_constraints(request)?;
        let [str_stat, dex, int_stat, fai, arc] = constraints.mins;
        let stats = Stats {
            str: str_stat,
            dex,
            int: int_stat,
            fai,
            arc,
            ..request.current_stats
        };
        let mut rows = Vec::new();
        for upgrade in 0..=max_upgrade {
            if !should_continue() {
                return Err("cancelled".to_string());
            }
            if let Some(row) = evaluate_fixed_loadout_upgrade(
                request,
                self.data,
                &self.weapons,
                upgrade,
                stats,
                &mut should_continue,
            )? {
                rows.push(row);
            }
        }
        Ok(rows)
    }
}

impl PreparedSearchPlan<'_> {
    pub fn estimate(&self) -> SearchEstimate {
        self.estimate
    }
}

#[derive(Clone, Copy, Debug)]
struct CandidateMetric {
    score: f32,
    ar: Option<DamageBreakdown>,
    status_buildup: Option<StatusBuildup>,
    aow_first_hit_damage: Option<f32>,
    aow_full_sequence_damage: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default)]
struct BaseWeaponMetric {
    ar: Option<DamageBreakdown>,
    status_buildup: Option<StatusBuildup>,
}

#[derive(Clone, Copy, Debug)]
struct ScoredCandidate {
    prepared_idx: usize,
    aow_idx: usize,
    upgrade: u8,
    stats: Stats,
    metric: CandidateMetric,
}

#[derive(Clone, Copy, Debug)]
struct SearchWorkUnit {
    group_idx: usize,
    aow_start: usize,
    aow_end: usize,
    candidate_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResultGroupMode {
    WeaponOnly,
    Loadout,
}

#[derive(Clone, Copy, Debug)]
struct ProgressEmitState {
    last_checked: u64,
    last_at: Instant,
}

trait SearchProgress {
    fn advance(
        &mut self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
    ) -> Result<(), String>;
    fn is_cancelled(&self) -> bool;
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

const PARALLEL_SEARCH_MIN_COMBINATIONS: u64 = 1_000_000;
const PARALLEL_AOW_CHUNK_SIZE: usize = 8;
const PARALLEL_PROGRESS_BATCH: u64 = 8_192;
const SCORED_TOP_K_LOADOUT_OVERSAMPLE: usize = 8;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);
pub const CANCELLATION_LATENCY_TARGET_MS: u64 = 250;

pub fn estimate_search_space(
    request: &OptimizeRequest,
    data: &GameData,
) -> Result<SearchEstimate, String> {
    prepare_search(request, data).map(|plan| plan.estimate())
}

pub fn prepare_search<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
) -> Result<PreparedSearchPlan<'a>, String> {
    prepare_search_with_cancel(request, data, || true)
}

pub fn prepare_search_with_cancel<'a, F>(
    request: &OptimizeRequest,
    data: &'a GameData,
    mut should_continue: F,
) -> Result<PreparedSearchPlan<'a>, String>
where
    F: FnMut() -> bool,
{
    if !should_continue() {
        return Err("cancelled".to_string());
    }
    validate_profile_capabilities(request, data)?;
    let constraints = build_combat_constraints(request)?;
    let weapons = Arc::from(
        prepare_weapons_with_cancel(request, data, constraints, &mut should_continue)?
            .into_boxed_slice(),
    );
    build_prepared_plan(request, data, constraints, weapons, &mut should_continue)
}

fn validate_profile_capabilities(request: &OptimizeRequest, data: &GameData) -> Result<(), String> {
    let profile = if data.profile_display_name.trim().is_empty() {
        "selected profile"
    } else {
        data.profile_display_name.as_str()
    };
    if request.standard_max_upgrade > data.rules.standard_max_upgrade {
        return Err(format!(
            "{profile} supports weapon upgrades only through +{}",
            data.rules.standard_max_upgrade
        ));
    }
    if request.somber_max_upgrade > data.rules.somber_max_upgrade {
        return Err(format!(
            "{profile} supports alternate weapon upgrades only through +{}",
            data.rules.somber_max_upgrade
        ));
    }
    if !data.rules.scadutree_scaling && (request.dlc_scaling || request.scadutree_level != 0) {
        return Err(format!("{profile} does not use Scadutree Blessing scaling"));
    }
    if !data.capabilities.weapon_ar {
        return Err(format!("{profile} does not provide weapon AR data"));
    }
    match request.objective {
        OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr => Ok(()),
        OptimizeObjective::MaxArPlusBleed if data.capabilities.status_buildup => Ok(()),
        OptimizeObjective::MaxArPlusBleed => {
            Err(format!("{profile} does not provide status-buildup data"))
        }
        OptimizeObjective::AowFirstHit if data.capabilities.aow_damage => Ok(()),
        OptimizeObjective::AowFirstHit => Err(format!(
            "{profile} does not provide verified Ash of War damage data"
        )),
        OptimizeObjective::AowFullSequence
            if data.capabilities.aow_damage && data.capabilities.aow_routes =>
        {
            Ok(())
        }
        OptimizeObjective::AowFullSequence => Err(format!(
            "{profile} does not provide verified Ash of War route data"
        )),
    }
}

pub fn prepare_loadout_evaluator_with_cancel<'a, F>(
    request: &OptimizeRequest,
    data: &'a GameData,
    mut should_continue: F,
) -> Result<PreparedLoadoutEvaluator<'a>, String>
where
    F: FnMut() -> bool,
{
    validate_profile_capabilities(request, data)?;
    if request.weapon_name.is_none() || request.affinity.is_none() {
        return Err("reusable loadout evaluation requires a weapon and affinity".to_string());
    }
    if !request.exact_upgrade {
        return Err("reusable loadout evaluation requires exact upgrade levels".to_string());
    }
    let mut preparation_request = request.clone();
    preparation_request.min_combat_stats = [0; COMBAT_STAT_COUNT];
    preparation_request.locked_combat_stats = [None; COMBAT_STAT_COUNT];
    preparation_request.top_k = 1;
    let constraints = build_combat_constraints(&preparation_request)?;
    let weapons = Arc::from(
        prepare_weapons_with_cancel(
            &preparation_request,
            data,
            constraints,
            &mut should_continue,
        )?
        .into_boxed_slice(),
    );
    Ok(PreparedLoadoutEvaluator {
        template: preparation_request,
        data,
        weapons,
    })
}

pub fn prepare_upgrade_series_evaluator_with_cancel<'a, F>(
    request: &OptimizeRequest,
    data: &'a GameData,
    mut should_continue: F,
) -> Result<PreparedUpgradeSeriesEvaluator<'a>, String>
where
    F: FnMut() -> bool,
{
    validate_profile_capabilities(request, data)?;
    if request.weapon_name.is_none() || request.affinity.is_none() {
        return Err("upgrade series evaluation requires a weapon and affinity".to_string());
    }
    if request.exact_upgrade {
        return Err("upgrade series evaluation requires an upgrade range".to_string());
    }
    if !should_continue() {
        return Err("cancelled".to_string());
    }
    let mut preparation_request = request.clone();
    preparation_request.min_combat_stats = [0; COMBAT_STAT_COUNT];
    preparation_request.locked_combat_stats = [None; COMBAT_STAT_COUNT];
    preparation_request.top_k = 1;
    let constraints = build_combat_constraints(&preparation_request)?;
    let weapons = Arc::from(
        prepare_weapons_with_cancel(
            &preparation_request,
            data,
            constraints,
            &mut should_continue,
        )?
        .into_boxed_slice(),
    );
    Ok(PreparedUpgradeSeriesEvaluator {
        template: preparation_request,
        data,
        weapons,
    })
}

fn validate_reusable_loadout(
    template: &OptimizeRequest,
    request: &OptimizeRequest,
) -> Result<(), String> {
    let compatible = template.class_name == request.class_name
        && template.standard_max_upgrade == request.standard_max_upgrade
        && template.somber_max_upgrade == request.somber_max_upgrade
        && template.exact_upgrade == request.exact_upgrade
        && template.two_handing == request.two_handing
        && template.dlc_scaling == request.dlc_scaling
        && template.scadutree_level == request.scadutree_level
        && template.weapon_name == request.weapon_name
        && template.affinity == request.affinity
        && template.aow_name == request.aow_name
        && template.weapon_type_key == request.weapon_type_key
        && template.somber_filter == request.somber_filter
        && template.objective == request.objective;
    if compatible {
        Ok(())
    } else {
        Err("request does not match the prepared loadout evaluator".to_string())
    }
}

fn build_prepared_plan<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
    constraints: CombatConstraints,
    weapons: Arc<[PreparedWeapon<'a>]>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<PreparedSearchPlan<'a>, String> {
    let mut groups: Vec<PreparedSearchGroup> = Vec::new();
    let mut stat_candidates = 0_u64;
    let mut combinations = 0_u64;

    for (prepared_idx, prepared) in weapons.iter().enumerate() {
        let shared_ar_search = if matches!(
            request.objective,
            OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr
        ) {
            prepared.aow_choices.first().and_then(|choice| {
                relevant_stat_search(request, data, constraints, prepared, choice)
            })
        } else {
            None
        };
        for (aow_idx, aow_choice) in prepared.aow_choices.iter().enumerate() {
            if !should_continue() {
                return Err("cancelled".to_string());
            }
            let search = if matches!(
                request.objective,
                OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr
            ) {
                shared_ar_search
            } else {
                relevant_stat_search(request, data, constraints, prepared, aow_choice)
            };
            let Some(search) = search else {
                continue;
            };
            stat_candidates = stat_candidates.saturating_add(search.candidate_count);
            combinations = combinations.saturating_add(
                search
                    .candidate_count
                    .saturating_mul(prepared.upgrades.len() as u64),
            );
            if let Some(group) = groups
                .iter_mut()
                .find(|group| group.prepared_idx == prepared_idx && group.search == search)
            {
                group.aow_indices.push(aow_idx);
            } else {
                groups.push(PreparedSearchGroup {
                    prepared_idx,
                    search,
                    aow_indices: vec![aow_idx],
                });
            }
        }
    }

    let estimate = SearchEstimate {
        weapon_candidates: weapons.len(),
        stat_candidates,
        combinations,
    };
    let mut plan = PreparedSearchPlan {
        request: request.clone(),
        data,
        weapons,
        groups,
        fine_work_units: Vec::new(),
        serial_work_units: Vec::new(),
        estimate,
    };
    plan.fine_work_units = build_search_work_units(&plan, true);
    plan.serial_work_units = build_search_work_units(&plan, false);
    Ok(plan)
}

pub fn optimize(request: &OptimizeRequest, data: &GameData) -> Result<Vec<OptimizeResult>, String> {
    if request.top_k == 0 {
        return Ok(Vec::new());
    }
    let plan = prepare_search(request, data)?;
    optimize_prepared_with_progress(&plan, 0, |_snapshot| true)
}

pub fn optimize_with_progress<F>(
    request: &OptimizeRequest,
    data: &GameData,
    progress_every: u64,
    progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    if request.top_k == 0 {
        return Ok(Vec::new());
    }
    let plan = prepare_search(request, data)?;
    optimize_prepared_with_progress(&plan, progress_every, progress_cb)
}

pub fn optimize_with_cancel<F>(
    request: &OptimizeRequest,
    data: &GameData,
    mut should_continue: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut() -> bool + Send,
{
    if request.top_k == 0 {
        return Ok(Vec::new());
    }
    let plan = prepare_search_with_cancel(request, data, &mut should_continue)?;
    optimize_prepared_with_progress(&plan, 1_024, move |_snapshot| should_continue())
}

pub fn optimize_level_range_with_progress<F, C>(
    request: &OptimizeRequest,
    levels: &[u16],
    data: &GameData,
    mut level_complete: F,
    mut should_continue: C,
) -> Result<Vec<LevelOptimizeResult>, String>
where
    F: FnMut(u16) -> bool,
    C: FnMut() -> bool + Send,
{
    if levels.is_empty() {
        return Ok(Vec::new());
    }
    let mut ordered_levels = levels.to_vec();
    ordered_levels.sort_unstable();
    ordered_levels.dedup();
    if ordered_levels[0] < request.character_level {
        return Err(format!(
            "level range starts at {} below request level {}",
            ordered_levels[0], request.character_level
        ));
    }
    if request.top_k == 0 {
        return Ok(ordered_levels
            .iter()
            .copied()
            .map(|level| LevelOptimizeResult {
                level,
                rows: Vec::new(),
            })
            .collect());
    }

    let max_level = *ordered_levels
        .last()
        .expect("non-empty levels must have a maximum");
    let mut max_request = request.clone();
    max_request.character_level = max_level;
    if !should_continue() {
        return Err("cancelled".to_string());
    }
    let max_constraints = build_combat_constraints(&max_request)?;
    let shared_weapons = Arc::from(
        prepare_weapons_with_cancel(&max_request, data, max_constraints, &mut should_continue)?
            .into_boxed_slice(),
    );

    let mut results = Vec::with_capacity(ordered_levels.len());
    for level in ordered_levels {
        if !should_continue() {
            return Err("cancelled".to_string());
        }
        let mut level_request = request.clone();
        level_request.character_level = level;
        let constraints = build_combat_constraints(&level_request)?;
        let plan = build_prepared_plan(
            &level_request,
            data,
            constraints,
            Arc::clone(&shared_weapons),
            &mut should_continue,
        )?;
        let rows = optimize_prepared_with_progress(&plan, 1_024, |_snapshot| should_continue())?;
        results.push(LevelOptimizeResult { level, rows });
        if !level_complete(level) {
            return Err("cancelled".to_string());
        }
    }
    Ok(results)
}

pub fn optimize_prepared_with_progress<F>(
    plan: &PreparedSearchPlan<'_>,
    progress_every: u64,
    progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let (candidates, group_mode) = score_prepared_with_progress(plan, progress_every, progress_cb)?;
    materialize_scored_candidates(
        &plan.request,
        plan.data,
        &plan.weapons,
        candidates,
        group_mode,
    )
}

pub fn optimize_profiled(
    request: &OptimizeRequest,
    data: &GameData,
) -> Result<ProfiledOptimizeResult, String> {
    let preparation_started = Instant::now();
    let plan = prepare_search(request, data)?;
    let preparation = preparation_started.elapsed();

    let scoring_started = Instant::now();
    let (candidates, group_mode) = score_prepared_with_progress(&plan, 0, |_| true)?;
    let scoring = scoring_started.elapsed();

    let materialization_started = Instant::now();
    let rows = materialize_scored_candidates(
        &plan.request,
        plan.data,
        &plan.weapons,
        candidates,
        group_mode,
    )?;
    let materialization = materialization_started.elapsed();
    Ok(ProfiledOptimizeResult {
        rows,
        timings: OptimizePhaseTimings {
            preparation,
            scoring,
            materialization,
        },
        estimate: plan.estimate(),
    })
}

fn score_prepared_with_progress<F>(
    plan: &PreparedSearchPlan<'_>,
    progress_every: u64,
    progress_cb: F,
) -> Result<(Vec<ScoredCandidate>, ResultGroupMode), String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let request = &plan.request;
    let group_mode = result_group_mode(request);
    if request.top_k == 0 {
        return Ok((Vec::new(), group_mode));
    }
    if plan.weapons.is_empty() {
        return Ok((Vec::new(), group_mode));
    }

    let fine_work_units = &plan.fine_work_units;
    let total = fine_work_units
        .iter()
        .map(|unit| unit.candidate_count)
        .sum::<u64>();
    if total == 0 {
        return Ok((Vec::new(), group_mode));
    }

    let candidates = if should_use_parallel_search(total, fine_work_units.len()) {
        score_parallel(
            plan,
            fine_work_units,
            group_mode,
            total,
            progress_every,
            progress_cb,
        )?
    } else {
        let mut progress = SerialSearchProgress::new(total, progress_every, progress_cb);
        score_serial(plan, &plan.serial_work_units, group_mode, &mut progress)?
    };
    Ok((candidates, group_mode))
}

fn result_group_mode(request: &OptimizeRequest) -> ResultGroupMode {
    if request.weapon_name.is_none() {
        ResultGroupMode::WeaponOnly
    } else {
        ResultGroupMode::Loadout
    }
}

fn should_use_parallel_search(total: u64, work_unit_count: usize) -> bool {
    let thread_count = rayon::current_num_threads();
    thread_count > 1
        && work_unit_count >= thread_count.min(2)
        && total >= PARALLEL_SEARCH_MIN_COMBINATIONS
}

fn build_search_work_units(plan: &PreparedSearchPlan<'_>, split_aows: bool) -> Vec<SearchWorkUnit> {
    let mut units = Vec::new();
    for (group_idx, group) in plan.groups.iter().enumerate() {
        let chunk_size = if split_aows {
            1
        } else {
            PARALLEL_AOW_CHUNK_SIZE
        };
        for aow_start in (0..group.aow_indices.len()).step_by(chunk_size) {
            let aow_end = (aow_start + chunk_size).min(group.aow_indices.len());
            let prepared = &plan.weapons[group.prepared_idx];
            let candidate_count = group
                .search
                .candidate_count
                .saturating_mul(prepared.upgrades.len() as u64)
                .saturating_mul((aow_end - aow_start) as u64);
            units.push(SearchWorkUnit {
                group_idx,
                aow_start,
                aow_end,
                candidate_count,
            });
        }
    }
    units
}

fn score_serial<F>(
    plan: &PreparedSearchPlan<'_>,
    work_units: &[SearchWorkUnit],
    group_mode: ResultGroupMode,
    progress: &mut SerialSearchProgress<F>,
) -> Result<Vec<ScoredCandidate>, String>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    let request = &plan.request;
    progress.emit_initial()?;
    let mut candidates = Vec::with_capacity(request.top_k);
    for unit in work_units {
        let mut unit_results = search_work_unit(plan, *unit, group_mode, progress)?;
        merge_scored_top_k(
            &mut candidates,
            unit_results.drain(..),
            request,
            plan.data,
            &plan.weapons,
            group_mode,
            request.top_k,
        )?;
    }
    progress.emit_final()?;
    Ok(candidates)
}

#[allow(clippy::too_many_arguments)]
fn score_parallel<F>(
    plan: &PreparedSearchPlan<'_>,
    work_units: &[SearchWorkUnit],
    group_mode: ResultGroupMode,
    total: u64,
    progress_every: u64,
    progress_cb: F,
) -> Result<Vec<ScoredCandidate>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let request = &plan.request;
    let progress = Arc::new(ParallelSearchProgress::new(
        total,
        progress_every,
        progress_cb,
    ));
    progress.emit_initial()?;
    let partial_results = work_units
        .par_iter()
        .map(|unit| {
            let mut local_progress = ParallelLocalProgress::new(Arc::clone(&progress));
            let result = search_work_unit(plan, *unit, group_mode, &mut local_progress);
            let finish_result = local_progress.finish();
            match (result, finish_result) {
                (Ok(results), Ok(())) => Ok(results),
                (Err(err), _) | (_, Err(err)) => Err(err),
            }
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut candidates = Vec::with_capacity(request.top_k);
    for unit_results in partial_results {
        merge_scored_top_k(
            &mut candidates,
            unit_results,
            request,
            plan.data,
            &plan.weapons,
            group_mode,
            request.top_k,
        )?;
    }
    progress.emit_final()?;
    if progress.is_cancelled() {
        return Err("cancelled".to_string());
    }
    Ok(candidates)
}

#[cfg(test)]
fn optimize_serial<F>(
    plan: &PreparedSearchPlan<'_>,
    work_units: &[SearchWorkUnit],
    group_mode: ResultGroupMode,
    progress: &mut SerialSearchProgress<F>,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    let candidates = score_serial(plan, work_units, group_mode, progress)?;
    materialize_scored_candidates(
        &plan.request,
        plan.data,
        &plan.weapons,
        candidates,
        group_mode,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn optimize_parallel<F>(
    plan: &PreparedSearchPlan<'_>,
    work_units: &[SearchWorkUnit],
    group_mode: ResultGroupMode,
    total: u64,
    progress_every: u64,
    progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let candidates = score_parallel(
        plan,
        work_units,
        group_mode,
        total,
        progress_every,
        progress_cb,
    )?;
    materialize_scored_candidates(
        &plan.request,
        plan.data,
        &plan.weapons,
        candidates,
        group_mode,
    )
}

fn search_work_unit<P>(
    plan: &PreparedSearchPlan<'_>,
    unit: SearchWorkUnit,
    group_mode: ResultGroupMode,
    progress: &mut P,
) -> Result<Vec<ScoredCandidate>, String>
where
    P: SearchProgress,
{
    if matches!(
        plan.request.objective,
        OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr
    ) {
        return search_ar_work_unit(plan, unit, group_mode, progress);
    }
    search_work_unit_exhaustive(plan, unit, group_mode, progress)
}

fn search_work_unit_exhaustive<P>(
    plan: &PreparedSearchPlan<'_>,
    unit: SearchWorkUnit,
    group_mode: ResultGroupMode,
    progress: &mut P,
) -> Result<Vec<ScoredCandidate>, String>
where
    P: SearchProgress,
{
    let request = &plan.request;
    let group = &plan.groups[unit.group_idx];
    let prepared = &plan.weapons[group.prepared_idx];
    let aow_indices = &group.aow_indices[unit.aow_start..unit.aow_end];
    let mut candidates = Vec::with_capacity(request.top_k);
    let damage_multiplier = request.damage_multiplier();
    let mut visit_result: Result<(), String> = Ok(());

    let mut current_combat = group.search.mins;
    group.search.visit(&mut current_combat, |combat| {
        if progress.is_cancelled() {
            visit_result = Err("cancelled".to_string());
            return false;
        }
        let mut stats = request.current_stats;
        stats.str = combat[STAT_STR];
        stats.dex = combat[STAT_DEX];
        stats.int = combat[STAT_INT];
        stats.fai = combat[STAT_FAI];
        stats.arc = combat[STAT_ARC];

        let effective_str_value = effective_str(
            stats.str,
            request.two_handing,
            prepared.weapon.disable_two_hand_bonus,
        );

        if !meets_requirements(prepared.weapon, effective_str_value, &stats) {
            let skipped = (prepared.upgrades.len() * aow_indices.len()) as u64;
            if let Err(err) = progress.advance(skipped, 0, None) {
                visit_result = Err(err);
                return false;
            }
            return true;
        }

        for upgrade in &prepared.upgrades {
            let base_metric = match calculate_base_weapon_metric(
                request.objective,
                prepared,
                *upgrade,
                &stats,
                effective_str_value,
                plan.data,
            ) {
                Ok(metric) => metric,
                Err(err) => {
                    visit_result = Err(err);
                    return false;
                }
            };
            for aow_idx in aow_indices {
                let aow_choice = &prepared.aow_choices[*aow_idx];
                let metric = score_candidate(
                    request.objective,
                    prepared,
                    aow_choice,
                    *upgrade,
                    &stats,
                    effective_str_value,
                    damage_multiplier,
                    base_metric,
                    plan.data,
                );
                let CandidateMetric {
                    score,
                    ar,
                    status_buildup,
                    aow_first_hit_damage,
                    aow_full_sequence_damage,
                } = match metric {
                    Ok(metric) => metric,
                    Err(err) => {
                        visit_result = Err(err);
                        return false;
                    }
                };
                if matches!(request.objective, OptimizeObjective::AowFirstHit)
                    && aow_first_hit_damage.unwrap_or(0.0) <= 0.0
                {
                    if let Err(err) = progress.advance(1, 1, None) {
                        visit_result = Err(err);
                        return false;
                    }
                    continue;
                }
                if matches!(request.objective, OptimizeObjective::AowFullSequence)
                    && aow_full_sequence_damage.unwrap_or(0.0) <= 0.0
                {
                    if let Err(err) = progress.advance(1, 1, None) {
                        visit_result = Err(err);
                        return false;
                    }
                    continue;
                }
                if let Err(err) = progress.advance(1, 1, Some(score)) {
                    visit_result = Err(err);
                    return false;
                }
                let candidate = ScoredCandidate {
                    prepared_idx: group.prepared_idx,
                    aow_idx: *aow_idx,
                    upgrade: *upgrade,
                    stats,
                    metric: CandidateMetric {
                        score,
                        ar,
                        status_buildup,
                        aow_first_hit_damage,
                        aow_full_sequence_damage,
                    },
                };
                if !could_enter_scored_top_k(
                    &candidates,
                    &candidate,
                    &plan.weapons,
                    request.top_k,
                    group_mode,
                ) {
                    continue;
                }
                if let Err(err) = push_scored_top_k(
                    &mut candidates,
                    candidate,
                    request,
                    plan.data,
                    &plan.weapons,
                    group_mode,
                    request.top_k,
                ) {
                    visit_result = Err(err);
                    return false;
                }
            }
        }
        true
    });
    visit_result.as_ref().map_err(Clone::clone)?;
    progress.finish()?;

    Ok(candidates)
}

fn search_ar_work_unit<P>(
    plan: &PreparedSearchPlan<'_>,
    unit: SearchWorkUnit,
    group_mode: ResultGroupMode,
    progress: &mut P,
) -> Result<Vec<ScoredCandidate>, String>
where
    P: SearchProgress,
{
    let request = &plan.request;
    let group = &plan.groups[unit.group_idx];
    let prepared = &plan.weapons[group.prepared_idx];
    let aow_indices = &group.aow_indices[unit.aow_start..unit.aow_end];
    let mut candidates = Vec::with_capacity(request.top_k.min(aow_indices.len()));
    let damage_multiplier = request.damage_multiplier();

    for upgrade in &prepared.upgrades {
        if progress.is_cancelled() {
            return Err("cancelled".to_string());
        }
        let combat = best_ar_combat_stats(
            &group.search,
            request,
            prepared,
            *upgrade,
            plan.data,
            progress,
        )?;
        let mut stats = request.current_stats;
        stats.str = combat[STAT_STR];
        stats.dex = combat[STAT_DEX];
        stats.int = combat[STAT_INT];
        stats.fai = combat[STAT_FAI];
        stats.arc = combat[STAT_ARC];
        let effective_str_value = effective_str(
            stats.str,
            request.two_handing,
            prepared.weapon.disable_two_hand_bonus,
        );
        let base_metric = calculate_base_weapon_metric(
            request.objective,
            prepared,
            *upgrade,
            &stats,
            effective_str_value,
            plan.data,
        )?;
        for aow_idx in aow_indices {
            let aow_choice = &prepared.aow_choices[*aow_idx];
            let metric = score_candidate(
                request.objective,
                prepared,
                aow_choice,
                *upgrade,
                &stats,
                effective_str_value,
                damage_multiplier,
                base_metric,
                plan.data,
            )?;
            progress.advance(
                group.search.candidate_count,
                group.search.candidate_count,
                Some(metric.score),
            )?;
            let candidate = ScoredCandidate {
                prepared_idx: group.prepared_idx,
                aow_idx: *aow_idx,
                upgrade: *upgrade,
                stats,
                metric,
            };
            if could_enter_scored_top_k(
                &candidates,
                &candidate,
                &plan.weapons,
                request.top_k,
                group_mode,
            ) {
                push_scored_top_k(
                    &mut candidates,
                    candidate,
                    request,
                    plan.data,
                    &plan.weapons,
                    group_mode,
                    request.top_k,
                )?;
            }
        }
    }
    progress.finish()?;
    Ok(candidates)
}

#[derive(Clone, Copy)]
struct ArAllocation {
    primary: f32,
    total: f32,
    combat: [u8; COMBAT_STAT_COUNT],
}

fn best_ar_combat_stats<P>(
    search: &RelevantStatSearch,
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    upgrade: u8,
    data: &GameData,
    progress: &P,
) -> Result<[u8; COMBAT_STAT_COUNT], String>
where
    P: SearchProgress,
{
    let mut base_combat = search.mins;
    fill_inactive_stats(search, &mut base_combat, search.required_inactive_fill());
    let base_ar = ar_for_combat(base_combat, request, prepared, upgrade, data)?;
    let target_spend = search
        .remaining_free
        .saturating_sub(search.required_inactive_fill());
    let mut allocations = vec![None; usize::from(target_spend) + 1];
    allocations[0] = Some(ArAllocation {
        primary: ar_primary(request.objective, base_ar),
        total: base_ar.total(),
        combat: base_combat,
    });

    for stat_idx in 0..COMBAT_STAT_COUNT {
        if !search.active[stat_idx] {
            continue;
        }
        if progress.is_cancelled() {
            return Err("cancelled".to_string());
        }
        let cap = u16::from(search.maxs[stat_idx] - search.mins[stat_idx]).min(target_spend);
        let mut stat_values = Vec::with_capacity(usize::from(cap) + 1);
        for add in 0..=cap {
            let mut combat = base_combat;
            combat[stat_idx] = search.mins[stat_idx] + add as u8;
            let ar = ar_for_combat(combat, request, prepared, upgrade, data)?;
            stat_values.push((
                ar_primary(request.objective, ar) - ar_primary(request.objective, base_ar),
                ar.total() - base_ar.total(),
            ));
        }

        let mut next = vec![None; allocations.len()];
        for (spent, entry) in allocations.iter().enumerate() {
            let Some(entry) = entry else { continue };
            let remaining = usize::from(target_spend).saturating_sub(spent);
            for add in 0..=usize::from(cap).min(remaining) {
                let mut candidate = *entry;
                candidate.primary = entry.primary + stat_values[add].0;
                candidate.total = entry.total + stat_values[add].1;
                candidate.combat[stat_idx] = search.mins[stat_idx] + add as u8;
                let destination = &mut next[spent + add];
                if destination.is_none_or(|current| better_ar_allocation(candidate, current)) {
                    *destination = Some(candidate);
                }
            }
        }
        allocations = next;
    }

    allocations[usize::from(target_spend)]
        .map(|allocation| allocation.combat)
        .ok_or_else(|| "AR stat optimizer could not satisfy the stat budget".to_string())
}

fn ar_for_combat(
    combat: [u8; COMBAT_STAT_COUNT],
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    upgrade: u8,
    data: &GameData,
) -> Result<DamageBreakdown, String> {
    let mut stats = request.current_stats;
    stats.str = combat[STAT_STR];
    stats.dex = combat[STAT_DEX];
    stats.int = combat[STAT_INT];
    stats.fai = combat[STAT_FAI];
    stats.arc = combat[STAT_ARC];
    calculate_ar(
        prepared.weapon,
        upgrade,
        &stats,
        effective_str(
            stats.str,
            request.two_handing,
            prepared.weapon.disable_two_hand_bonus,
        ),
        data,
    )
}

fn ar_primary(objective: OptimizeObjective, ar: DamageBreakdown) -> f32 {
    match objective {
        OptimizeObjective::MaxPhysicalAr => ar.physical,
        _ => ar.total(),
    }
}

fn better_ar_allocation(candidate: ArAllocation, current: ArAllocation) -> bool {
    candidate.primary > current.primary
        || candidate.primary == current.primary && candidate.total > current.total
        || candidate.primary == current.primary
            && candidate.total == current.total
            && candidate.combat < current.combat
}

fn materialize_scored_candidates(
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    candidates: Vec<ScoredCandidate>,
    group_mode: ResultGroupMode,
) -> Result<Vec<OptimizeResult>, String> {
    let damage_multiplier = request.damage_multiplier();
    let mut results = Vec::with_capacity(request.top_k);
    for candidate in candidates {
        let result =
            materialize_scored_candidate(candidate, request, data, weapons, damage_multiplier)?;
        push_top_k(&mut results, result, request.top_k, group_mode);
    }
    Ok(results)
}

fn complete_scored_candidate_tie_breaks(
    left: &mut ScoredCandidate,
    right: &mut ScoredCandidate,
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
) -> Result<(), String> {
    complete_scored_candidate_ar(left, request, data, weapons)?;
    complete_scored_candidate_ar(right, request, data, weapons)?;
    if compare_known_candidate_metrics(&left.metric, &right.metric) != std::cmp::Ordering::Equal {
        return Ok(());
    }

    complete_scored_candidate_aow(left, request, data, weapons)?;
    complete_scored_candidate_aow(right, request, data, weapons)?;
    if compare_known_candidate_metrics(&left.metric, &right.metric) != std::cmp::Ordering::Equal {
        return Ok(());
    }

    complete_scored_candidate_status(left, data, weapons)?;
    complete_scored_candidate_status(right, data, weapons)
}

fn complete_scored_candidate_ar(
    candidate: &mut ScoredCandidate,
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
) -> Result<(), String> {
    if candidate.metric.ar.is_some() {
        return Ok(());
    }
    let prepared = &weapons[candidate.prepared_idx];
    let aow_choice = &prepared.aow_choices[candidate.aow_idx];
    let effective_str_value = effective_str(
        candidate.stats.str,
        request.two_handing,
        prepared.weapon.disable_two_hand_bonus,
    );
    candidate.metric.ar = Some(calculate_ar_with_buffs(
        prepared,
        aow_choice,
        candidate.upgrade,
        &candidate.stats,
        effective_str_value,
        request.damage_multiplier(),
        data,
    )?);
    Ok(())
}

fn complete_scored_candidate_aow(
    candidate: &mut ScoredCandidate,
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
) -> Result<(), String> {
    if candidate.metric.aow_first_hit_damage.is_some()
        && candidate.metric.aow_full_sequence_damage.is_some()
    {
        return Ok(());
    }
    let prepared = &weapons[candidate.prepared_idx];
    let aow_choice = &prepared.aow_choices[candidate.aow_idx];
    let effective_str_value = effective_str(
        candidate.stats.str,
        request.two_handing,
        prepared.weapon.disable_two_hand_bonus,
    );
    let (first, full) = calculate_aow_metric(
        prepared,
        aow_choice,
        candidate.upgrade,
        &candidate.stats,
        effective_str_value,
        request.damage_multiplier(),
        data,
    )?;
    candidate.metric.aow_first_hit_damage = Some(first);
    candidate.metric.aow_full_sequence_damage = Some(full);
    Ok(())
}

fn complete_scored_candidate_status(
    candidate: &mut ScoredCandidate,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
) -> Result<(), String> {
    if candidate.metric.status_buildup.is_none() {
        let prepared = &weapons[candidate.prepared_idx];
        let aow_choice = &prepared.aow_choices[candidate.aow_idx];
        candidate.metric.status_buildup = Some(calculate_status_with_buffs(
            prepared,
            aow_choice,
            candidate.upgrade,
            &candidate.stats,
            data,
        )?);
    }
    Ok(())
}

fn evaluate_fixed_loadout_upgrade(
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    upgrade: u8,
    stats: Stats,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<OptimizeResult>, String> {
    let mut results = Vec::with_capacity(1);
    let damage_multiplier = request.damage_multiplier();
    for (prepared_idx, prepared) in weapons.iter().enumerate() {
        if !prepared.upgrades.contains(&upgrade) {
            continue;
        }
        if !should_continue() {
            return Err("cancelled".to_string());
        }
        let effective_str_value = effective_str(
            stats.str,
            request.two_handing,
            prepared.weapon.disable_two_hand_bonus,
        );
        if !meets_requirements(prepared.weapon, effective_str_value, &stats) {
            continue;
        }
        let base_metric = calculate_base_weapon_metric(
            request.objective,
            prepared,
            upgrade,
            &stats,
            effective_str_value,
            data,
        )?;
        for (aow_idx, aow_choice) in prepared.aow_choices.iter().enumerate() {
            if !should_continue() {
                return Err("cancelled".to_string());
            }
            let metric = score_candidate(
                request.objective,
                prepared,
                aow_choice,
                upgrade,
                &stats,
                effective_str_value,
                damage_multiplier,
                base_metric,
                data,
            )?;
            if matches!(request.objective, OptimizeObjective::AowFirstHit)
                && metric.aow_first_hit_damage.unwrap_or(0.0) <= 0.0
            {
                continue;
            }
            if matches!(request.objective, OptimizeObjective::AowFullSequence)
                && metric.aow_full_sequence_damage.unwrap_or(0.0) <= 0.0
            {
                continue;
            }
            let row = materialize_scored_candidate(
                ScoredCandidate {
                    prepared_idx,
                    aow_idx,
                    upgrade,
                    stats,
                    metric,
                },
                request,
                data,
                weapons,
                damage_multiplier,
            )?;
            push_top_k(&mut results, row, 1, ResultGroupMode::Loadout);
        }
    }
    Ok(results.pop())
}

fn materialize_scored_candidate(
    candidate: ScoredCandidate,
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    damage_multiplier: f32,
) -> Result<OptimizeResult, String> {
    let prepared = &weapons[candidate.prepared_idx];
    let aow_choice = &prepared.aow_choices[candidate.aow_idx];
    let effective_str_value = effective_str(
        candidate.stats.str,
        request.two_handing,
        prepared.weapon.disable_two_hand_bonus,
    );
    let full = complete_candidate_metric(
        candidate.metric.ar,
        candidate.metric.status_buildup,
        candidate.metric.aow_first_hit_damage,
        candidate.metric.aow_full_sequence_damage,
        prepared,
        aow_choice,
        candidate.upgrade,
        &candidate.stats,
        effective_str_value,
        damage_multiplier,
        data,
    )?;
    let CandidateMetric {
        score: _,
        ar,
        status_buildup,
        aow_first_hit_damage,
        aow_full_sequence_damage,
    } = full;
    let aow_route = materialize_aow_route(
        request.objective,
        prepared,
        aow_choice,
        candidate.upgrade,
        &candidate.stats,
        effective_str_value,
        damage_multiplier,
        data,
    )?;
    let reinforce = data
        .reinforce_level(prepared.weapon.reinforce_type, candidate.upgrade)
        .ok_or_else(|| {
            format!(
                "missing reinforce row type={} level={}",
                prepared.weapon.reinforce_type, candidate.upgrade
            )
        })?;

    Ok(OptimizeResult {
        weapon_id: prepared.weapon.weapon_id,
        weapon_name: prepared.weapon.name.clone(),
        weapon_type_name: prepared.weapon.weapon_type_name.clone(),
        affinity: prepared.weapon.affinity.clone(),
        is_somber: prepared.weapon.is_somber,
        upgrade: candidate.upgrade,
        stats: candidate.stats,
        requirements: prepared.weapon.requirements,
        effective_scaling: std::array::from_fn(|idx| {
            prepared.weapon.scaling[idx] * reinforce.scaling_mult[idx]
        }),
        ar: ar.expect("complete candidate metric must include AR"),
        aow_id: aow_choice.skill_id,
        aow_name: aow_choice.skill_name.map(str::to_string),
        bleed_buildup: status_buildup
            .expect("complete candidate metric must include status")
            .bleed,
        bleed_buildup_add: aow_choice
            .aow
            .map(|aow| aow.bleed_buildup_add)
            .unwrap_or(0.0),
        frost_buildup: status_buildup
            .expect("complete candidate metric must include status")
            .frost,
        poison_buildup: status_buildup
            .expect("complete candidate metric must include status")
            .poison,
        scarlet_rot_buildup: status_buildup
            .expect("complete candidate metric must include status")
            .scarlet_rot,
        aow_first_hit_damage: aow_first_hit_damage
            .expect("complete candidate metric must include first-hit damage"),
        aow_full_sequence_damage: aow_full_sequence_damage
            .expect("complete candidate metric must include full-sequence damage"),
        aow_route,
        score: candidate.metric.score,
    })
}

#[allow(clippy::too_many_arguments)]
fn materialize_aow_route(
    objective: OptimizeObjective,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    data: &GameData,
) -> Result<Option<AowRouteResult>, String> {
    if aow_choice.attack_rows.is_empty() {
        return Ok(None);
    }
    let mut routes = calculate_aow_routes(
        prepared.weapon,
        &aow_choice.attack_rows,
        upgrade,
        stats,
        effective_str_value,
        data,
    )?;
    for route in &mut routes {
        route.first_hit_damage *= damage_multiplier;
        route.total_damage = route.total_damage.scale(damage_multiplier);
        for action in &mut route.actions {
            for hit in &mut action.hits {
                hit.damage = hit.damage.scale(damage_multiplier);
            }
        }
    }
    Ok(routes.into_iter().reduce(|best, candidate| {
        let best_metric = match objective {
            OptimizeObjective::AowFirstHit => best.first_hit_damage,
            _ => best.total_damage.total(),
        };
        let candidate_metric = match objective {
            OptimizeObjective::AowFirstHit => candidate.first_hit_damage,
            _ => candidate.total_damage.total(),
        };
        if candidate_metric > best_metric {
            candidate
        } else {
            best
        }
    }))
}

struct SerialSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    total: u64,
    checked: u64,
    eligible: u64,
    best_score: Option<f32>,
    started: Instant,
    progress_every: u64,
    last_emit: ProgressEmitState,
    callback: F,
    cancelled: bool,
}

impl<F> SerialSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    fn new(total: u64, progress_every: u64, callback: F) -> Self {
        let started = Instant::now();
        Self {
            total,
            checked: 0,
            eligible: 0,
            best_score: None,
            started,
            progress_every,
            last_emit: ProgressEmitState {
                last_checked: 0,
                last_at: started,
            },
            callback,
            cancelled: false,
        }
    }

    fn emit_initial(&mut self) -> Result<(), String> {
        self.emit(true)
    }

    fn emit_final(&mut self) -> Result<(), String> {
        self.emit(true)
    }

    fn emit_if_due(&mut self) -> Result<(), String> {
        if self.progress_every == 0 {
            return Ok(());
        }
        if self.checked.saturating_sub(self.last_emit.last_checked) < self.progress_every {
            return Ok(());
        }
        if self.last_emit.last_at.elapsed() < PROGRESS_MIN_INTERVAL {
            return Ok(());
        }
        self.emit(false)
    }

    fn emit(&mut self, force: bool) -> Result<(), String> {
        if self.cancelled && !force {
            return Err("cancelled".to_string());
        }
        let snapshot = ProgressSnapshot {
            checked: self.checked,
            total: self.total,
            eligible: self.eligible,
            best_score: self.best_score.unwrap_or(0.0),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        };
        if !(self.callback)(snapshot) {
            self.cancelled = true;
            return Err("cancelled".to_string());
        }
        self.last_emit = ProgressEmitState {
            last_checked: self.checked,
            last_at: Instant::now(),
        };
        Ok(())
    }
}

impl<F> SearchProgress for SerialSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    fn advance(
        &mut self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
    ) -> Result<(), String> {
        self.checked = self.checked.saturating_add(checked_delta);
        self.eligible = self.eligible.saturating_add(eligible_delta);
        if let Some(score) = best_score
            && self.best_score.is_none_or(|current| score > current)
        {
            self.best_score = Some(score);
        }
        self.emit_if_due()
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

struct ParallelSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    total: u64,
    checked: AtomicU64,
    eligible: AtomicU64,
    cancelled: AtomicBool,
    best_score: Mutex<Option<f32>>,
    started: Instant,
    progress_every: u64,
    last_emit: Mutex<ProgressEmitState>,
    callback: Mutex<F>,
}

impl<F> ParallelSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    fn new(total: u64, progress_every: u64, callback: F) -> Self {
        let started = Instant::now();
        Self {
            total,
            checked: AtomicU64::new(0),
            eligible: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
            best_score: Mutex::new(None),
            started,
            progress_every,
            last_emit: Mutex::new(ProgressEmitState {
                last_checked: 0,
                last_at: started,
            }),
            callback: Mutex::new(callback),
        }
    }

    fn record(
        &self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
        force: bool,
    ) -> Result<(), String> {
        if checked_delta > 0 {
            self.checked.fetch_add(checked_delta, Ordering::Relaxed);
        }
        if eligible_delta > 0 {
            self.eligible.fetch_add(eligible_delta, Ordering::Relaxed);
        }
        if let Some(score) = best_score {
            let mut guard = self
                .best_score
                .lock()
                .map_err(|_| "failed to lock progress best score".to_string())?;
            if guard.is_none_or(|current| score > current) {
                *guard = Some(score);
            }
        }
        self.emit_if_due(force)
    }

    fn emit_initial(&self) -> Result<(), String> {
        self.emit_if_due(true)
    }

    fn emit_final(&self) -> Result<(), String> {
        self.emit_if_due(true)
    }

    fn emit_if_due(&self, force: bool) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) && !force {
            return Err("cancelled".to_string());
        }
        let checked = self.checked.load(Ordering::Relaxed);
        if !force {
            if self.progress_every == 0 {
                return Ok(());
            }
            let mut emit_guard = self
                .last_emit
                .lock()
                .map_err(|_| "failed to lock progress emit state".to_string())?;
            if checked.saturating_sub(emit_guard.last_checked) < self.progress_every
                || emit_guard.last_at.elapsed() < PROGRESS_MIN_INTERVAL
            {
                return Ok(());
            }
            *emit_guard = ProgressEmitState {
                last_checked: checked,
                last_at: Instant::now(),
            };
        } else if let Ok(mut emit_guard) = self.last_emit.lock() {
            *emit_guard = ProgressEmitState {
                last_checked: checked,
                last_at: Instant::now(),
            };
        }

        let best_score = self
            .best_score
            .lock()
            .map_err(|_| "failed to lock progress best score".to_string())?
            .unwrap_or(0.0);
        let snapshot = ProgressSnapshot {
            checked,
            total: self.total,
            eligible: self.eligible.load(Ordering::Relaxed),
            best_score,
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        };
        let should_continue = {
            let mut callback = self
                .callback
                .lock()
                .map_err(|_| "failed to lock progress callback".to_string())?;
            (callback)(snapshot)
        };
        if !should_continue {
            self.cancelled.store(true, Ordering::Relaxed);
            return Err("cancelled".to_string());
        }
        Ok(())
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

struct ParallelLocalProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    shared: Arc<ParallelSearchProgress<F>>,
    checked: u64,
    eligible: u64,
    best_score: Option<f32>,
}

impl<F> ParallelLocalProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    fn new(shared: Arc<ParallelSearchProgress<F>>) -> Self {
        Self {
            shared,
            checked: 0,
            eligible: 0,
            best_score: None,
        }
    }

    fn flush(&mut self, force: bool) -> Result<(), String> {
        if self.checked == 0 && self.eligible == 0 && self.best_score.is_none() && !force {
            return Ok(());
        }
        let checked = self.checked;
        let eligible = self.eligible;
        let best_score = self.best_score;
        self.checked = 0;
        self.eligible = 0;
        self.best_score = None;
        self.shared.record(checked, eligible, best_score, force)
    }
}

impl<F> SearchProgress for ParallelLocalProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    fn advance(
        &mut self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
    ) -> Result<(), String> {
        self.checked = self.checked.saturating_add(checked_delta);
        self.eligible = self.eligible.saturating_add(eligible_delta);
        if let Some(score) = best_score
            && self.best_score.is_none_or(|current| score > current)
        {
            self.best_score = Some(score);
        }
        if self.checked >= PARALLEL_PROGRESS_BATCH {
            self.flush(false)?;
        }
        Ok(())
    }

    fn is_cancelled(&self) -> bool {
        self.shared.is_cancelled()
    }

    fn finish(&mut self) -> Result<(), String> {
        self.flush(false)
    }
}

fn build_combat_constraints(request: &OptimizeRequest) -> Result<CombatConstraints, String> {
    validate_stat_caps(request)?;
    let class_info = class_by_name(&request.class_name)
        .ok_or_else(|| format!("unknown starting class: {}", request.class_name))?;
    let free_points =
        compute_free_points(class_info, request.character_level, &request.current_stats)?;
    let current = request.current_stats.combat_array();

    let mut mins = [0_u8; COMBAT_STAT_COUNT];
    let mut maxs = [99_u8; COMBAT_STAT_COUNT];
    let mut mandatory_raise: u16 = 0;
    for idx in 0..COMBAT_STAT_COUNT {
        mins[idx] = current[idx].max(request.min_combat_stats[idx]);
        if let Some(locked) = request.locked_combat_stats[idx] {
            if locked < mins[idx] {
                return Err(format!(
                    "locked combat stat {} is below minimum floor {}",
                    idx, mins[idx]
                ));
            }
            mins[idx] = locked;
            maxs[idx] = locked;
        }
        mandatory_raise += u16::from(mins[idx].saturating_sub(current[idx]));
    }
    if mandatory_raise > free_points {
        return Err("combat stat floors exceed free point budget".to_string());
    }

    let remaining_free = free_points - mandatory_raise;
    let capacity: u16 = maxs
        .iter()
        .zip(mins.iter())
        .map(|(max_v, min_v)| u16::from(*max_v - *min_v))
        .sum();
    if remaining_free > capacity {
        return Err("locked combat stats cannot absorb remaining free points".to_string());
    }

    Ok(CombatConstraints {
        mins,
        maxs,
        remaining_free,
    })
}

fn validate_stat_caps(request: &OptimizeRequest) -> Result<(), String> {
    let stats = [
        ("vig", request.current_stats.vig),
        ("mnd", request.current_stats.mnd),
        ("end", request.current_stats.end),
        ("str", request.current_stats.str),
        ("dex", request.current_stats.dex),
        ("int", request.current_stats.int),
        ("fai", request.current_stats.fai),
        ("arc", request.current_stats.arc),
    ];
    for (name, value) in stats {
        if value > 99 {
            return Err(format!("{name} must be <= 99"));
        }
    }
    for idx in 0..COMBAT_STAT_COUNT {
        if request.min_combat_stats[idx] > 99 {
            return Err(format!("minimum combat stat {idx} must be <= 99"));
        }
        if request.locked_combat_stats[idx].is_some_and(|value| value > 99) {
            return Err(format!("locked combat stat {idx} must be <= 99"));
        }
    }
    Ok(())
}

#[cfg(test)]
fn prepare_weapons<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
    constraints: CombatConstraints,
) -> Result<Vec<PreparedWeapon<'a>>, String> {
    prepare_weapons_with_cancel(request, data, constraints, &mut || true)
}

fn prepare_weapons_with_cancel<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
    constraints: CombatConstraints,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Vec<PreparedWeapon<'a>>, String> {
    let mut out = Vec::new();
    for weapon in &data.weapons {
        if !should_continue() {
            return Err("cancelled".to_string());
        }
        if !data.weapon_ar_supported(weapon) || !weapon_matches_request(weapon, request) {
            continue;
        }
        if !weapon_requirements_can_fit(request, constraints, weapon) {
            continue;
        }
        let Some(upgrades) = available_upgrades(weapon, request, data) else {
            continue;
        };
        let Some(aow_choices) = resolve_aow_choices(weapon, request, data)? else {
            continue;
        };
        out.push(PreparedWeapon {
            weapon,
            aow_choices,
            upgrades,
        });
    }
    Ok(out)
}

fn score_for(
    objective: OptimizeObjective,
    total_ar: f32,
    status_buildup: StatusBuildup,
    aow_first_hit_damage: f32,
    aow_full_sequence_damage: f32,
) -> f32 {
    match objective {
        OptimizeObjective::MaxAr => total_ar,
        OptimizeObjective::MaxPhysicalAr => total_ar,
        OptimizeObjective::MaxArPlusBleed => status_buildup.bleed,
        OptimizeObjective::AowFirstHit => aow_first_hit_damage,
        OptimizeObjective::AowFullSequence => aow_full_sequence_damage,
    }
}

#[allow(clippy::too_many_arguments)]
fn score_candidate(
    objective: OptimizeObjective,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    base_metric: BaseWeaponMetric,
    data: &GameData,
) -> Result<CandidateMetric, String> {
    match objective {
        OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr => {
            let ar = apply_aow_attack_buffs(
                base_metric
                    .ar
                    .expect("AR objectives must prepare a base AR metric"),
                aow_choice.aow,
            )
            .scale(damage_multiplier);
            let score = match objective {
                OptimizeObjective::MaxPhysicalAr => ar.physical,
                _ => ar.total(),
            };
            Ok(CandidateMetric {
                score,
                ar: Some(ar),
                status_buildup: None,
                aow_first_hit_damage: None,
                aow_full_sequence_damage: None,
            })
        }
        OptimizeObjective::MaxArPlusBleed => {
            let ar = apply_aow_attack_buffs(
                base_metric
                    .ar
                    .expect("AR + Bleed must prepare a base AR metric"),
                aow_choice.aow,
            )
            .scale(damage_multiplier);
            let status_buildup = apply_aow_status_buffs(
                base_metric
                    .status_buildup
                    .expect("AR + Bleed must prepare base status buildup"),
                prepared.weapon,
                upgrade,
                stats,
                data,
                aow_choice.aow,
            )?;
            Ok(CandidateMetric {
                score: score_for(objective, ar.total(), status_buildup, 0.0, 0.0),
                ar: Some(ar),
                status_buildup: Some(status_buildup),
                aow_first_hit_damage: None,
                aow_full_sequence_damage: None,
            })
        }
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence => {
            let (aow_first_hit_damage, aow_full_sequence_damage) = calculate_aow_metric(
                prepared,
                aow_choice,
                upgrade,
                stats,
                effective_str_value,
                damage_multiplier,
                data,
            )?;
            Ok(CandidateMetric {
                score: score_for(
                    objective,
                    0.0,
                    StatusBuildup::default(),
                    aow_first_hit_damage,
                    aow_full_sequence_damage,
                ),
                ar: None,
                status_buildup: None,
                aow_first_hit_damage: Some(aow_first_hit_damage),
                aow_full_sequence_damage: Some(aow_full_sequence_damage),
            })
        }
    }
}

fn calculate_base_weapon_metric(
    objective: OptimizeObjective,
    prepared: &PreparedWeapon<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    data: &GameData,
) -> Result<BaseWeaponMetric, String> {
    match objective {
        OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr => Ok(BaseWeaponMetric {
            ar: Some(calculate_ar(
                prepared.weapon,
                upgrade,
                stats,
                effective_str_value,
                data,
            )?),
            status_buildup: None,
        }),
        OptimizeObjective::MaxArPlusBleed => Ok(BaseWeaponMetric {
            ar: Some(calculate_ar(
                prepared.weapon,
                upgrade,
                stats,
                effective_str_value,
                data,
            )?),
            status_buildup: Some(calculate_status_buildup(
                prepared.weapon,
                upgrade,
                stats,
                data,
            )?),
        }),
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence => {
            Ok(BaseWeaponMetric::default())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn complete_candidate_metric(
    ar: Option<DamageBreakdown>,
    status_buildup: Option<StatusBuildup>,
    aow_first_hit_damage: Option<f32>,
    aow_full_sequence_damage: Option<f32>,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    data: &GameData,
) -> Result<CandidateMetric, String> {
    let ar = match ar {
        Some(ar) => ar,
        None => calculate_ar_with_buffs(
            prepared,
            aow_choice,
            upgrade,
            stats,
            effective_str_value,
            damage_multiplier,
            data,
        )?,
    };
    let status_buildup = match status_buildup {
        Some(status_buildup) => status_buildup,
        None => calculate_status_with_buffs(prepared, aow_choice, upgrade, stats, data)?,
    };
    let (first, full) = match (aow_first_hit_damage, aow_full_sequence_damage) {
        (Some(first), Some(full)) => (first, full),
        _ => calculate_aow_metric(
            prepared,
            aow_choice,
            upgrade,
            stats,
            effective_str_value,
            damage_multiplier,
            data,
        )?,
    };
    Ok(CandidateMetric {
        score: score_for(
            OptimizeObjective::MaxAr,
            ar.total(),
            status_buildup,
            first,
            full,
        ),
        ar: Some(ar),
        status_buildup: Some(status_buildup),
        aow_first_hit_damage: Some(first),
        aow_full_sequence_damage: Some(full),
    })
}

fn calculate_ar_with_buffs(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    data: &GameData,
) -> Result<DamageBreakdown, String> {
    Ok(apply_aow_attack_buffs(
        calculate_ar(prepared.weapon, upgrade, stats, effective_str_value, data)?,
        aow_choice.aow,
    )
    .scale(damage_multiplier))
}

fn calculate_status_with_buffs(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    data: &GameData,
) -> Result<StatusBuildup, String> {
    apply_aow_status_buffs(
        calculate_status_buildup(prepared.weapon, upgrade, stats, data)?,
        prepared.weapon,
        upgrade,
        stats,
        data,
        aow_choice.aow,
    )
}

fn calculate_aow_metric(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    data: &GameData,
) -> Result<(f32, f32), String> {
    let resolved_attack_rows;
    let attack_rows = if aow_choice.attack_rows.is_empty() {
        resolved_attack_rows = if prepared.weapon.disable_gem_attr {
            select_attack_rows(
                data.native_skill_attack_rows(prepared.weapon.weapon_id),
                prepared.weapon,
            )
        } else if let Some(skill_id) = aow_choice.skill_id {
            select_aow_attack_rows(skill_id, prepared.weapon, data)
        } else {
            Vec::new()
        };
        resolved_attack_rows.as_slice()
    } else {
        aow_choice.attack_rows.as_slice()
    };
    if attack_rows.is_empty() {
        return Ok((0.0, 0.0));
    }
    let (first, full) = calculate_aow_damage(
        prepared.weapon,
        attack_rows,
        upgrade,
        stats,
        effective_str_value,
        data,
    )?;
    Ok((first * damage_multiplier, full * damage_multiplier))
}

fn weapon_matches_request(weapon: &Weapon, request: &OptimizeRequest) -> bool {
    if let Some(lock_weapon) = request.weapon_name.as_deref()
        && !weapon.name.eq_ignore_ascii_case(lock_weapon)
    {
        return false;
    }
    if let Some(lock_affinity) = request.affinity.as_deref()
        && !weapon.affinity.eq_ignore_ascii_case(lock_affinity)
    {
        return false;
    }
    if let Some(type_key) = request.weapon_type_key.as_deref()
        && !weapon_type_matches(weapon, type_key)
    {
        return false;
    }
    match request.somber_filter {
        SomberFilter::All => true,
        SomberFilter::StandardOnly => !weapon.is_somber,
        SomberFilter::SomberOnly => weapon.is_somber,
    }
}

fn weapon_type_matches(weapon: &Weapon, type_key: &str) -> bool {
    normalize_weapon_type_display(&weapon.weapon_type_name).eq_ignore_ascii_case(type_key)
        || weapon.weapon_type_name.eq_ignore_ascii_case(type_key)
        || weapon
            .weapon_type_keys
            .split('|')
            .any(|key| key.eq_ignore_ascii_case(type_key))
}

fn normalize_weapon_type_display(raw: &str) -> &str {
    match raw.trim() {
        "Hand-to-Hand" => "Hand-to-Hand Arts",
        "Heavy Spear" => "Great Spear",
        "Reverse-hand Blade" => "Backhand Blade",
        "Scythe" => "Reaper",
        "Seal" => "Sacred Seal",
        "Staff" => "Glintstone Staff",
        other => other,
    }
}

fn available_upgrades(
    weapon: &Weapon,
    request: &OptimizeRequest,
    data: &GameData,
) -> Option<Vec<u8>> {
    let levels = data.reinforce.get(usize::from(weapon.reinforce_type))?;
    if levels.is_empty() {
        return None;
    }

    let cap = upgrade_cap_for_weapon(weapon, request);

    if request.exact_upgrade {
        let fixed = cap;
        return data
            .reinforce_level(weapon.reinforce_type, fixed)
            .is_some()
            .then(|| vec![fixed]);
    }

    let out: Vec<u8> = levels
        .iter()
        .enumerate()
        .filter_map(|(level, value)| {
            (value.is_some() && level <= usize::from(cap)).then_some(level as u8)
        })
        .collect();
    (!out.is_empty()).then_some(out)
}

fn upgrade_cap_for_weapon(weapon: &Weapon, request: &OptimizeRequest) -> u8 {
    if weapon.is_somber {
        request.somber_max_upgrade
    } else {
        request.standard_max_upgrade
    }
}

fn resolve_aow_choices<'a>(
    weapon: &'a Weapon,
    request: &OptimizeRequest,
    data: &'a GameData,
) -> Result<Option<Vec<AowChoice<'a>>>, String> {
    let no_aow = AowChoice {
        aow: None,
        skill_id: None,
        skill_name: None,
        attack_rows: Vec::new(),
    };
    let native_skill_choice = native_skill_choice_for_weapon(weapon, data, request.objective);

    if let Some(lock_aow_name) = request.aow_name.as_deref() {
        if let Some(choice) = native_skill_choice.as_ref()
            && choice
                .skill_name
                .is_some_and(|skill_name| skill_name.eq_ignore_ascii_case(lock_aow_name))
        {
            if matches!(
                request.objective,
                OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
            ) && choice.attack_rows.is_empty()
            {
                return Ok(None);
            }
            return Ok(Some(vec![choice.clone()]));
        }
        let compatible_matches: Vec<&Aow> = data
            .aows
            .iter()
            .filter(|value| value.name.eq_ignore_ascii_case(lock_aow_name))
            .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
            .collect();
        if compatible_matches.is_empty() {
            let known = data
                .aows
                .iter()
                .any(|value| value.name.eq_ignore_ascii_case(lock_aow_name));
            if !known {
                return Err(format!("unknown AoW: {lock_aow_name}"));
            }
            return Ok(None);
        }
        let Some(aow) = compatible_matches.into_iter().next() else {
            return Err(format!("unknown AoW: {lock_aow_name}"));
        };
        let choice = build_aow_choice(aow, weapon, data, request.objective);
        if matches!(
            request.objective,
            OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
        ) && choice.attack_rows.is_empty()
        {
            return Ok(None);
        }
        return Ok(Some(vec![choice]));
    }

    if matches!(
        request.objective,
        OptimizeObjective::MaxAr
            | OptimizeObjective::MaxPhysicalAr
            | OptimizeObjective::MaxArPlusBleed
    ) {
        return Ok(Some(open_aow_choices_for_objective(
            weapon,
            data,
            no_aow,
            native_skill_choice,
            request.objective,
        )));
    }

    let native_skill_id = native_skill_choice
        .as_ref()
        .and_then(|choice| choice.skill_id);
    let mut choices: Vec<AowChoice<'a>> = native_skill_choice
        .into_iter()
        .filter(|choice| !choice.attack_rows.is_empty())
        .collect();
    choices.extend(
        data.aows
            .iter()
            .filter(|aow| !aow.name.eq_ignore_ascii_case("No Skill"))
            .filter(|aow| Some(aow.aow_id) != native_skill_id)
            .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
            .map(|aow| build_aow_choice(aow, weapon, data, request.objective))
            .filter(|choice| !choice.attack_rows.is_empty()),
    );
    if choices.is_empty() {
        return Ok(None);
    }
    Ok(Some(choices))
}

fn build_aow_choice<'a>(
    aow: &'a Aow,
    weapon: &Weapon,
    data: &'a GameData,
    objective: OptimizeObjective,
) -> AowChoice<'a> {
    AowChoice {
        aow: Some(aow),
        skill_id: Some(aow.aow_id),
        skill_name: Some(aow.name.as_str()),
        attack_rows: if objective_uses_aow_damage(objective) {
            select_aow_attack_rows(aow.aow_id, weapon, data)
        } else {
            Vec::new()
        },
    }
}

fn open_aow_choices_for_objective<'a>(
    weapon: &'a Weapon,
    data: &'a GameData,
    no_aow: AowChoice<'a>,
    native_skill_choice: Option<AowChoice<'a>>,
    objective: OptimizeObjective,
) -> Vec<AowChoice<'a>> {
    if weapon.disable_gem_attr {
        return native_skill_choice.map_or_else(|| vec![no_aow], |choice| vec![choice]);
    }

    let mut choices = vec![no_aow];
    choices.extend(
        data.aows
            .iter()
            .filter(|aow| !aow.name.eq_ignore_ascii_case("No Skill"))
            .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
            .filter(|aow| aow_affects_objective(aow, objective))
            .map(|aow| build_aow_choice(aow, weapon, data, objective)),
    );
    choices
}

fn objective_uses_aow_damage(objective: OptimizeObjective) -> bool {
    matches!(
        objective,
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
    )
}

fn aow_affects_objective(aow: &Aow, objective: OptimizeObjective) -> bool {
    let changes_any_ar = aow.buff_attack_power.iter().any(|value| *value != 0.0);
    match objective {
        OptimizeObjective::MaxAr => changes_any_ar,
        OptimizeObjective::MaxPhysicalAr => {
            aow.buff_attack_power[DamageType::Physical.as_index()] != 0.0
        }
        OptimizeObjective::MaxArPlusBleed => {
            changes_any_ar || aow.bleed_buildup_add != 0.0 || aow.scaling_status_add.bleed != 0.0
        }
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence => true,
    }
}

fn native_skill_choice_for_weapon<'a>(
    weapon: &'a Weapon,
    data: &'a GameData,
    objective: OptimizeObjective,
) -> Option<AowChoice<'a>> {
    let native_skill_id = weapon.native_skill_id?;
    let exact_rows = data.native_skill_attack_rows(weapon.weapon_id);
    let source_rows = if exact_rows.is_empty() {
        data.aow_attack_rows(native_skill_id)
    } else {
        exact_rows
    };
    let attack_rows = if objective_uses_aow_damage(objective) {
        select_attack_rows(source_rows, weapon)
    } else {
        Vec::new()
    };
    let skill_name = weapon
        .native_skill_name
        .as_deref()
        .or_else(|| source_rows.first().map(|row| row.aow_name.as_str()))?;
    Some(AowChoice {
        aow: None,
        skill_id: Some(native_skill_id),
        skill_name: Some(skill_name),
        attack_rows,
    })
}

fn select_aow_attack_rows<'a>(
    aow_id: u16,
    weapon: &Weapon,
    data: &'a GameData,
) -> Vec<&'a AowAttackRow> {
    select_attack_rows(data.aow_attack_rows(aow_id), weapon)
}

fn select_attack_rows<'a>(rows: &'a [AowAttackRow], weapon: &Weapon) -> Vec<&'a AowAttackRow> {
    if rows.is_empty() {
        return Vec::new();
    }

    let matched_rows: Vec<&AowAttackRow> = rows
        .iter()
        .filter(|row| {
            !row.variant_weapon_type.is_empty()
                && variant_weapon_type_matches(&row.variant_weapon_type, &weapon.weapon_type_name)
        })
        .collect();
    if !matched_rows.is_empty() {
        return matched_rows;
    }

    let generic_rows: Vec<&AowAttackRow> = rows
        .iter()
        .filter(|row| row.variant_weapon_type.is_empty())
        .collect();
    if !generic_rows.is_empty() {
        return generic_rows;
    }

    let placeholder_rows: Vec<&AowAttackRow> = rows
        .iter()
        .filter(|row| is_placeholder_variant(&row.variant_weapon_type))
        .collect();
    if placeholder_rows.is_empty() {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for row in placeholder_rows {
        let key = raw_name_without_variant_prefix(&row.raw_name).to_ascii_lowercase();
        if seen.insert(key) {
            deduped.push(row);
        }
    }
    deduped
}

fn variant_weapon_type_matches(variant: &str, weapon_type_name: &str) -> bool {
    if variant.is_empty() {
        return false;
    }
    normalize_type_token(variant) == normalize_type_token(weapon_type_name)
}

fn normalize_type_token(value: &str) -> String {
    let mut normalized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized = match normalized.as_str() {
        "backhandblade" => "reversehandblade".to_string(),
        "greatspear" => "heavyspear".to_string(),
        "reaper" => "scythe".to_string(),
        _ => normalized,
    };
    normalized
}

fn is_placeholder_variant(variant: &str) -> bool {
    let normalized = normalize_type_token(variant);
    normalized.starts_with("var")
        && normalized.get(3..).is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn raw_name_without_variant_prefix(raw_name: &str) -> &str {
    if let Some(remainder) = raw_name
        .strip_prefix('[')
        .and_then(|tail| tail.split_once(']').map(|(_, remainder)| remainder.trim()))
    {
        return remainder;
    }
    raw_name
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RelevantStatSearch {
    mins: [u8; COMBAT_STAT_COUNT],
    maxs: [u8; COMBAT_STAT_COUNT],
    active: [bool; COMBAT_STAT_COUNT],
    remaining_free: u16,
    required_inactive_fill: u16,
    candidate_count: u64,
}

fn relevant_stat_search(
    request: &OptimizeRequest,
    data: &GameData,
    constraints: CombatConstraints,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
) -> Option<RelevantStatSearch> {
    let active = active_stats_for_choice(request, prepared, aow_choice, data);
    RelevantStatSearch::new(request, constraints, prepared.weapon, active)
}

impl RelevantStatSearch {
    fn new(
        request: &OptimizeRequest,
        constraints: CombatConstraints,
        weapon: &Weapon,
        active: [bool; COMBAT_STAT_COUNT],
    ) -> Option<Self> {
        let mut mins = constraints.mins;
        let maxs = constraints.maxs;
        let mut remaining_free = constraints.remaining_free;
        let requirement_mins = weapon_requirement_mins(request, weapon);
        for idx in 0..COMBAT_STAT_COUNT {
            if requirement_mins[idx] > maxs[idx] {
                return None;
            }
            if requirement_mins[idx] > mins[idx] {
                let raise = u16::from(requirement_mins[idx] - mins[idx]);
                if raise > remaining_free {
                    return None;
                }
                mins[idx] = requirement_mins[idx];
                remaining_free -= raise;
            }
        }

        let active_capacity: u16 = (0..COMBAT_STAT_COUNT)
            .filter(|idx| active[*idx])
            .map(|idx| u16::from(maxs[idx] - mins[idx]))
            .sum();
        let required_inactive_fill = remaining_free.saturating_sub(active_capacity);
        let candidate_count = count_relevant_distributions(
            &mins,
            &maxs,
            &active,
            remaining_free,
            required_inactive_fill,
        );
        (candidate_count > 0).then_some(Self {
            mins,
            maxs,
            active,
            remaining_free,
            required_inactive_fill,
            candidate_count,
        })
    }

    fn visit<F>(&self, current: &mut [u8; COMBAT_STAT_COUNT], mut visitor: F)
    where
        F: FnMut(&[u8; COMBAT_STAT_COUNT]) -> bool,
    {
        visit_relevant_stat_candidates_inner(0, self.remaining_free, self, current, &mut visitor);
    }

    fn required_inactive_fill(&self) -> u16 {
        self.required_inactive_fill
    }
}

fn count_relevant_distributions(
    mins: &[u8; COMBAT_STAT_COUNT],
    maxs: &[u8; COMBAT_STAT_COUNT],
    active: &[bool; COMBAT_STAT_COUNT],
    remaining_free: u16,
    required_inactive_fill: u16,
) -> u64 {
    let mut memo: HashMap<(usize, u16), u64> = HashMap::new();
    count_active_distributions(
        mins,
        maxs,
        active,
        required_inactive_fill,
        0,
        remaining_free,
        &mut memo,
    )
}

fn count_active_distributions(
    mins: &[u8; COMBAT_STAT_COUNT],
    maxs: &[u8; COMBAT_STAT_COUNT],
    active: &[bool; COMBAT_STAT_COUNT],
    required_inactive_fill: u16,
    idx: usize,
    remaining_free: u16,
    memo: &mut HashMap<(usize, u16), u64>,
) -> u64 {
    if idx == COMBAT_STAT_COUNT {
        return if remaining_free == required_inactive_fill {
            1
        } else {
            0
        };
    }
    if let Some(value) = memo.get(&(idx, remaining_free)) {
        return *value;
    }
    let total = if active[idx] {
        let cap = u16::from(maxs[idx] - mins[idx]).min(remaining_free);
        (0..=cap)
            .map(|add| {
                count_active_distributions(
                    mins,
                    maxs,
                    active,
                    required_inactive_fill,
                    idx + 1,
                    remaining_free - add,
                    memo,
                )
            })
            .fold(0_u64, u64::saturating_add)
    } else {
        count_active_distributions(
            mins,
            maxs,
            active,
            required_inactive_fill,
            idx + 1,
            remaining_free,
            memo,
        )
    };
    memo.insert((idx, remaining_free), total);
    total
}

fn visit_relevant_stat_candidates_inner<F>(
    idx: usize,
    remaining_free: u16,
    search: &RelevantStatSearch,
    current: &mut [u8; COMBAT_STAT_COUNT],
    visitor: &mut F,
) -> bool
where
    F: FnMut(&[u8; COMBAT_STAT_COUNT]) -> bool,
{
    if idx == COMBAT_STAT_COUNT {
        if remaining_free != search.required_inactive_fill() {
            return true;
        }
        let mut filled = *current;
        fill_inactive_stats(search, &mut filled, remaining_free);
        return visitor(&filled);
    }

    if !search.active[idx] {
        current[idx] = search.mins[idx];
        return visit_relevant_stat_candidates_inner(
            idx + 1,
            remaining_free,
            search,
            current,
            visitor,
        );
    }

    let cap = u16::from(search.maxs[idx] - search.mins[idx]).min(remaining_free);
    for add in 0..=cap {
        current[idx] = search.mins[idx] + (add as u8);
        if !visit_relevant_stat_candidates_inner(
            idx + 1,
            remaining_free - add,
            search,
            current,
            visitor,
        ) {
            current[idx] = search.mins[idx];
            return false;
        }
    }
    current[idx] = search.mins[idx];
    true
}

#[allow(clippy::needless_range_loop)]
fn fill_inactive_stats(
    search: &RelevantStatSearch,
    current: &mut [u8; COMBAT_STAT_COUNT],
    mut remaining_free: u16,
) {
    for idx in 0..COMBAT_STAT_COUNT {
        if search.active[idx] {
            continue;
        }
        let later_capacity: u16 = ((idx + 1)..COMBAT_STAT_COUNT)
            .filter(|later| !search.active[*later])
            .map(|later| u16::from(search.maxs[later] - search.mins[later]))
            .sum();
        let cap = u16::from(search.maxs[idx] - search.mins[idx]).min(remaining_free);
        let add = remaining_free.saturating_sub(later_capacity).min(cap);
        current[idx] = search.mins[idx] + (add as u8);
        remaining_free -= add;
    }
}

fn weapon_requirements_can_fit(
    request: &OptimizeRequest,
    constraints: CombatConstraints,
    weapon: &Weapon,
) -> bool {
    RelevantStatSearch::new(request, constraints, weapon, [true; COMBAT_STAT_COUNT]).is_some()
}

fn weapon_requirement_mins(request: &OptimizeRequest, weapon: &Weapon) -> [u8; COMBAT_STAT_COUNT] {
    std::array::from_fn(|idx| {
        if idx == STAT_STR {
            minimum_str_for_requirement(
                weapon.requirements[STAT_STR],
                request.two_handing,
                weapon.disable_two_hand_bonus,
            )
        } else {
            weapon.requirements[idx]
        }
    })
}

fn active_stats_for_choice(
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    data: &GameData,
) -> [bool; COMBAT_STAT_COUNT] {
    match request.objective {
        OptimizeObjective::MaxAr => {
            std::array::from_fn(|idx| weapon_stat_can_increase_ar(prepared.weapon, data, idx))
        }
        OptimizeObjective::MaxPhysicalAr => std::array::from_fn(|idx| {
            weapon_stat_can_increase_damage_type(prepared.weapon, data, idx, DamageType::Physical)
        }),
        OptimizeObjective::MaxArPlusBleed => std::array::from_fn(|idx| {
            weapon_stat_can_increase_ar(prepared.weapon, data, idx)
                || stat_can_increase_status_for_choice(prepared, aow_choice, data, idx)
        }),
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence => {
            std::array::from_fn(|idx| {
                aow_choice
                    .attack_rows
                    .iter()
                    .any(|row| attack_row_stat_can_increase_damage(prepared.weapon, row, data, idx))
            })
        }
    }
}

fn weapon_stat_can_increase_damage_type(
    weapon: &Weapon,
    data: &GameData,
    stat_idx: usize,
    damage_type: DamageType,
) -> bool {
    if weapon.scaling[stat_idx] <= 0.0 || weapon.base[damage_type.as_index()] <= 0.0 {
        return false;
    }
    let Some(aec) = data.attack_element(weapon.attack_element_correct_id) else {
        return true;
    };
    aec.stat_scales(stat_idx, damage_type)
}

fn attack_row_stat_can_increase_damage(
    weapon: &Weapon,
    row: &AowAttackRow,
    data: &GameData,
    stat_idx: usize,
) -> bool {
    if row.is_lacking_fp || !row.is_damaging() {
        return false;
    }
    DamageType::ALL.iter().any(|damage_type| {
        let damage_idx = damage_type.as_index();
        let has_damage_base = weapon.base[damage_idx] > 0.0 && row.motion_values[damage_idx] > 0.0
            || row.attack_base[damage_idx] > 0.0;
        if !has_damage_base {
            return false;
        }
        if let Some(override_id) = row.overwrite_attack_element_correct_id {
            let Some(aec_ext) = data.attack_element_ext(override_id) else {
                return false;
            };
            if !aec_ext.stat_scales(stat_idx, damage_idx) {
                return false;
            }
            return aec_ext
                .overwrite_rate(stat_idx, damage_idx)
                .is_some_and(|rate| rate > 0.0)
                || weapon.scaling[stat_idx] * aec_ext.influence_rate(stat_idx, damage_idx) > 0.0;
        }
        weapon_stat_can_increase_damage_type(weapon, data, stat_idx, *damage_type)
    })
}

fn stat_can_increase_status_for_choice(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    data: &GameData,
    stat_idx: usize,
) -> bool {
    prepared.upgrades.iter().any(|upgrade| {
        let mut source = data.weapon_passive(prepared.weapon.weapon_id);
        if let Some(overlay) = data.weapon_passive_overlay(prepared.weapon.weapon_id, *upgrade) {
            source = merge_status_source_for_relevance(source, overlay);
        }
        status_source_stat_can_scale(source, prepared.weapon, stat_idx)
    }) || aow_choice.aow.is_some_and(|aow| {
        status_source_stat_can_scale(aow_status_source(aow), prepared.weapon, stat_idx)
    })
}

fn aow_status_source(aow: &Aow) -> StatusEffectSource {
    StatusEffectSource {
        buildup: aow.scaling_status_add,
        correction_flags: aow.scaling_status_flags,
    }
}

fn merge_status_source_for_relevance(
    mut base: StatusEffectSource,
    overlay: StatusEffectSource,
) -> StatusEffectSource {
    merge_status_relevance_value(
        &mut base.buildup.bleed,
        &mut base.correction_flags.bleed,
        overlay.buildup.bleed,
        overlay.correction_flags.bleed,
    );
    merge_status_relevance_value(
        &mut base.buildup.frost,
        &mut base.correction_flags.frost,
        overlay.buildup.frost,
        overlay.correction_flags.frost,
    );
    merge_status_relevance_value(
        &mut base.buildup.poison,
        &mut base.correction_flags.poison,
        overlay.buildup.poison,
        overlay.correction_flags.poison,
    );
    merge_status_relevance_value(
        &mut base.buildup.scarlet_rot,
        &mut base.correction_flags.scarlet_rot,
        overlay.buildup.scarlet_rot,
        overlay.correction_flags.scarlet_rot,
    );
    merge_status_relevance_value(
        &mut base.buildup.sleep,
        &mut base.correction_flags.sleep,
        overlay.buildup.sleep,
        overlay.correction_flags.sleep,
    );
    merge_status_relevance_value(
        &mut base.buildup.madness,
        &mut base.correction_flags.madness,
        overlay.buildup.madness,
        overlay.correction_flags.madness,
    );
    merge_status_relevance_value(
        &mut base.buildup.death,
        &mut base.correction_flags.death,
        overlay.buildup.death,
        overlay.correction_flags.death,
    );
    base
}

fn merge_status_relevance_value(
    base_value: &mut f32,
    base_flag: &mut Option<bool>,
    overlay_value: f32,
    overlay_flag: Option<bool>,
) {
    if overlay_value > 0.0 {
        *base_value = overlay_value;
        *base_flag = overlay_flag;
    }
}

fn status_source_stat_can_scale(
    source: StatusEffectSource,
    weapon: &Weapon,
    stat_idx: usize,
) -> bool {
    if weapon.scaling[stat_idx] <= 0.0 {
        return false;
    }
    match stat_idx {
        STAT_INT => {
            source.buildup.frost > 0.0
                && status_uses_correction(source.correction_flags.frost, true)
        }
        STAT_ARC => {
            status_value_can_scale(source.buildup.bleed, source.correction_flags.bleed)
                || status_value_can_scale(source.buildup.poison, source.correction_flags.poison)
                || status_value_can_scale(
                    source.buildup.scarlet_rot,
                    source.correction_flags.scarlet_rot,
                )
                || status_value_can_scale(source.buildup.sleep, source.correction_flags.sleep)
                || status_value_can_scale(source.buildup.madness, source.correction_flags.madness)
                || status_value_can_scale(source.buildup.death, source.correction_flags.death)
        }
        _ => false,
    }
}

fn status_value_can_scale(value: f32, flag: Option<bool>) -> bool {
    value > 0.0 && status_uses_correction(flag, true)
}

fn status_uses_correction(flag: Option<bool>, fallback: bool) -> bool {
    flag.unwrap_or(fallback)
}

fn minimum_str_for_requirement(
    requirement: u8,
    two_handing: bool,
    disable_two_hand_bonus: bool,
) -> u8 {
    if !two_handing || disable_two_hand_bonus {
        return requirement;
    }
    for candidate in 0..=requirement {
        if effective_str(candidate, true, false) >= u16::from(requirement) {
            return candidate;
        }
    }
    requirement
}

fn weapon_stat_can_increase_ar(weapon: &Weapon, data: &GameData, stat_idx: usize) -> bool {
    if weapon.scaling[stat_idx] <= 0.0 {
        return false;
    }
    let Some(aec) = data
        .attack_element_correct
        .get(weapon.attack_element_correct_id)
        .and_then(|entry| *entry)
    else {
        return true;
    };

    DamageType::ALL.iter().any(|damage_type| {
        weapon.base[damage_type.as_index()] > 0.0 && aec.stat_scales(stat_idx, *damage_type)
    })
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
fn count_stat_candidates(constraints: CombatConstraints) -> u64 {
    let mut caps = [0_u8; COMBAT_STAT_COUNT];
    for idx in 0..COMBAT_STAT_COUNT {
        caps[idx] = constraints.maxs[idx] - constraints.mins[idx];
    }
    let mut memo: HashMap<(usize, u16), u64> = HashMap::new();
    count_distributions(&caps, 0, constraints.remaining_free, &mut memo)
}

#[cfg(test)]
fn count_distributions(
    caps: &[u8; COMBAT_STAT_COUNT],
    idx: usize,
    remaining: u16,
    memo: &mut HashMap<(usize, u16), u64>,
) -> u64 {
    if idx == COMBAT_STAT_COUNT {
        return if remaining == 0 { 1 } else { 0 };
    }
    if let Some(value) = memo.get(&(idx, remaining)) {
        return *value;
    }

    let mut total = 0_u64;
    let max_add = u16::from(caps[idx]).min(remaining);
    for add in 0..=max_add {
        total = total.saturating_add(count_distributions(caps, idx + 1, remaining - add, memo));
    }
    memo.insert((idx, remaining), total);
    total
}

#[cfg(test)]
mod tests;
