use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::math::ScalarAowRoute;
use crate::math::{
    apply_aow_attack_buffs, apply_aow_bleed_buffs, apply_aow_status_buffs, calculate_aow_routes,
    calculate_ar, calculate_bleed_buildup, calculate_status_buildup, class_by_name,
    compute_free_points, effective_str, evaluate_scalar_aow_route, meets_requirements,
    prepare_scalar_aow_routes,
};
use crate::model::{
    Aow, AowAttackRow, AowEffectRole, AowRouteResult, COMBAT_STAT_COUNT, DamageBreakdown,
    DamageType, GameData, STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, StatusBuildup,
    StatusEffectSource, Weapon, normalize_weapon_type_display,
};

mod types;
pub use types::*;
mod ranking;
use ranking::*;

#[derive(Clone, Debug)]
struct AowChoice<'a> {
    no_applied_ash: bool,
    aow: Option<&'a Aow>,
    skill_id: Option<u16>,
    skill_name: Option<&'a str>,
    attack_rows: Vec<&'a AowAttackRow>,
    // `Some` contains an eager cache for a one-thread Rayon pool. `None` retains
    // the old per-work-unit preparation when multiple threads are available,
    // where eager route compilation creates a serial preparation barrier
    // without a measurable scoring benefit.
    scalar_routes: Option<Result<Option<Vec<ScalarAowRoute<'a>>>, String>>,
}

enum ScalarRouteSet<'routes, 'data> {
    Cached(&'routes [ScalarAowRoute<'data>]),
    Owned(Vec<ScalarAowRoute<'data>>),
}

impl<'routes, 'data> ScalarRouteSet<'routes, 'data> {
    fn as_slice(&self) -> &[ScalarAowRoute<'data>] {
        match self {
            Self::Cached(routes) => routes,
            Self::Owned(routes) => routes,
        }
    }
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
            true,
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
    bleed_buildup: Option<f32>,
    aow_first_hit_damage: Option<f32>,
    aow_full_sequence_damage: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default)]
struct BaseWeaponMetric {
    ar: Option<DamageBreakdown>,
    bleed_buildup: Option<f32>,
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
    estimate_search_space_with_cancel(request, data, || true)
}

pub fn estimate_search_space_with_cancel<F>(
    request: &OptimizeRequest,
    data: &GameData,
    mut should_continue: F,
) -> Result<SearchEstimate, String>
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
    build_prepared_plan(
        request,
        data,
        constraints,
        weapons,
        &mut should_continue,
        false,
    )
    .map(|plan| plan.estimate())
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
    build_prepared_plan(
        request,
        data,
        constraints,
        weapons,
        &mut should_continue,
        true,
    )
}

fn validate_profile_capabilities(request: &OptimizeRequest, data: &GameData) -> Result<(), String> {
    let profile = if data.profile_display_name.trim().is_empty() {
        "selected profile"
    } else {
        data.profile_display_name.as_str()
    };
    if request.scadutree_level > crate::math::SCADUTREE_MAX_LEVEL {
        return Err(format!(
            "Scadutree Blessing level must be {} or lower",
            crate::math::SCADUTREE_MAX_LEVEL
        ));
    }
    let max_character_level: u16 = if data.capabilities.class_budget {
        713
    } else {
        8 * 99
    };
    if request.character_level > max_character_level {
        return Err(format!(
            "character level must be {max_character_level} or lower; got {}",
            request.character_level,
        ));
    }
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
        OptimizeObjective::BleedThenAr if data.capabilities.status_buildup => Ok(()),
        OptimizeObjective::BleedThenAr => {
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

fn weapon_uses_two_handing(request: &OptimizeRequest, weapon: &Weapon) -> bool {
    request.two_handing || weapon.forces_two_handing()
}

fn effective_str_for_weapon(request: &OptimizeRequest, weapon: &Weapon, strength: u8) -> u16 {
    effective_str(
        strength,
        weapon_uses_two_handing(request, weapon),
        weapon.disable_two_hand_bonus,
    )
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
        && template.filters == request.filters
        && template.result_grouping == request.result_grouping
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
    build_work_units: bool,
) -> Result<PreparedSearchPlan<'a>, String> {
    let mut groups: Vec<PreparedSearchGroup> = Vec::new();
    let mut stat_candidates = 0_u64;
    let mut combinations = 0_u64;
    let mut distribution_counts = HashMap::new();

    for (prepared_idx, prepared) in weapons.iter().enumerate() {
        let mut groups_by_search = HashMap::<RelevantStatSearch, usize>::new();
        for (aow_idx, aow_choice) in prepared.aow_choices.iter().enumerate() {
            if !should_continue() {
                return Err("cancelled".to_string());
            }
            let search = relevant_stat_search(
                request,
                data,
                constraints,
                prepared,
                aow_choice,
                &mut distribution_counts,
            );
            let Some(search) = search else {
                continue;
            };
            stat_candidates = stat_candidates.saturating_add(search.candidate_count);
            combinations = combinations.saturating_add(
                search
                    .candidate_count
                    .saturating_mul(prepared.upgrades.len() as u64),
            );
            if let Some(&group_idx) = groups_by_search.get(&search) {
                groups[group_idx].aow_indices.push(aow_idx);
            } else {
                let group_idx = groups.len();
                groups.push(PreparedSearchGroup {
                    prepared_idx,
                    search,
                    aow_indices: vec![aow_idx],
                });
                groups_by_search.insert(search, group_idx);
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
    if build_work_units {
        plan.fine_work_units = build_search_work_units(&plan, true);
        plan.serial_work_units = build_search_work_units(&plan, false);
    }
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
            true,
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
    match request.result_grouping {
        ResultGrouping::Weapon => ResultGroupMode::WeaponOnly,
        ResultGrouping::Loadout => ResultGroupMode::Loadout,
        ResultGrouping::Automatic if request.weapon_name.is_none() => ResultGroupMode::WeaponOnly,
        ResultGrouping::Automatic => ResultGroupMode::Loadout,
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
        let chunk_size = if split_aows
            && matches!(
                plan.request.objective,
                OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
            ) {
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
    search_dp_work_unit(plan, unit, group_mode, progress)
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

        let effective_str_value = effective_str_for_weapon(request, prepared.weapon, stats.str);

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
                    bleed_buildup,
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
                        bleed_buildup,
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

fn search_dp_work_unit<P>(
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
    let mut route_sets = Vec::with_capacity(aow_indices.len());
    for &aow_idx in aow_indices {
        let routes = match &prepared.aow_choices[aow_idx].scalar_routes {
            Some(Ok(Some(routes))) => ScalarRouteSet::Cached(routes.as_slice()),
            Some(Ok(None)) => return search_work_unit_exhaustive(plan, unit, group_mode, progress),
            Some(Err(error)) => return Err(error.clone()),
            None => match prepare_scalar_aow_routes(
                &prepared.aow_choices[aow_idx].attack_rows,
                plan.data,
            )? {
                Some(routes) => ScalarRouteSet::Owned(routes),
                None => return search_work_unit_exhaustive(plan, unit, group_mode, progress),
            },
        };
        route_sets.push((aow_idx, routes));
    }

    let share_primary = aow_indices.len() > 1
        && matches!(
            request.objective,
            OptimizeObjective::MaxAr
                | OptimizeObjective::MaxPhysicalAr
                | OptimizeObjective::BleedThenAr
        );
    for &upgrade in &prepared.upgrades {
        let mut primary_plans = HashMap::new();
        for &(aow_idx, ref route_set) in &route_sets {
            if progress.is_cancelled() {
                return Err("cancelled".to_string());
            }
            let aow_choice = &prepared.aow_choices[aow_idx];
            let routes = route_set.as_slice();
            let primary = if share_primary {
                let key = primary_effect_key(aow_choice);
                if let std::collections::hash_map::Entry::Vacant(entry) = primary_plans.entry(key) {
                    entry.insert(prepare_primary_allocations(
                        &group.search,
                        request,
                        prepared,
                        aow_choice,
                        upgrade,
                        plan.data,
                        progress,
                    )?);
                }
                primary_plans.get(&key)
            } else {
                None
            };
            let best = if routes.is_empty() {
                best_objective_allocation(
                    &group.search,
                    request,
                    prepared,
                    aow_choice,
                    upgrade,
                    None,
                    plan.data,
                    progress,
                    primary,
                )?
            } else {
                let mut best = None;
                for route in routes {
                    let candidate = best_objective_allocation(
                        &group.search,
                        request,
                        prepared,
                        aow_choice,
                        upgrade,
                        Some(route),
                        plan.data,
                        progress,
                        primary,
                    )?;
                    if best.is_none_or(|current| better_objective_allocation(candidate, current)) {
                        best = Some(candidate);
                    }
                }
                best.ok_or_else(|| "AoW route optimizer found no feasible allocation".to_string())?
            };
            let metric = CandidateMetric {
                score: best.key.score,
                ar: Some(best.ar),
                status_buildup: None,
                bleed_buildup: Some(best.key.bleed),
                aow_first_hit_damage: Some(best.key.aow_first),
                aow_full_sequence_damage: Some(best.key.aow_full),
            };
            if matches!(request.objective, OptimizeObjective::AowFirstHit)
                && best.key.aow_first <= 0.0
                || matches!(request.objective, OptimizeObjective::AowFullSequence)
                    && best.key.aow_full <= 0.0
            {
                progress.advance(
                    group.search.candidate_count,
                    group.search.candidate_count,
                    None,
                )?;
                continue;
            }
            progress.advance(
                group.search.candidate_count,
                group.search.candidate_count,
                Some(metric.score),
            )?;
            let stats = stats_with_combat(request.current_stats, best.combat);
            let candidate = ScoredCandidate {
                prepared_idx: group.prepared_idx,
                aow_idx,
                upgrade,
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

#[derive(Clone, Copy, Debug, Default)]
struct ObjectiveKey {
    score: f32,
    ar_total: f32,
    aow_full: f32,
    aow_first: f32,
    bleed: f32,
}

#[derive(Clone, Copy, Debug)]
struct ObjectiveAllocation {
    key: ObjectiveKey,
    ar: DamageBreakdown,
    combat: [u8; COMBAT_STAT_COUNT],
}

#[derive(Debug)]
struct PrimaryAllocationPlan {
    base: ObjectiveAllocation,
    values: [Vec<ObjectiveAllocation>; COMBAT_STAT_COUNT],
    additions: [Vec<Vec<u8>>; COMBAT_STAT_COUNT],
    exact_unique_combat: Option<[u8; COMBAT_STAT_COUNT]>,
}

fn primary_effect_key(choice: &AowChoice<'_>) -> [u32; 9] {
    let Some(ash) = choice.aow else { return [0; 9] };
    let scaling = ash.scaling_status_add;
    let mut key = [0; 9];
    for (destination, value) in key.iter_mut().zip(ash.buff_attack_power) {
        *destination = value.to_bits();
    }
    key[5] = ash.bleed_buildup_add.to_bits();
    key[6] = scaling.bleed.to_bits();
    key[7] = u32::from(
        [
            scaling.bleed,
            scaling.frost,
            scaling.poison,
            scaling.scarlet_rot,
            scaling.sleep,
            scaling.madness,
            scaling.death,
        ]
        .iter()
        .any(|value| *value > 0.0),
    );
    key[8] = match ash.scaling_status_flags.bleed {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    key
}

#[allow(clippy::too_many_arguments)]
fn prepare_primary_allocations<P: SearchProgress>(
    search: &RelevantStatSearch,
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    choice: &AowChoice<'_>,
    upgrade: u8,
    data: &GameData,
    progress: &P,
) -> Result<PrimaryAllocationPlan, String> {
    let base =
        evaluate_objective_allocation(search.mins, request, prepared, choice, upgrade, None, data)?;
    let mut plan = PrimaryAllocationPlan {
        base,
        values: std::array::from_fn(|_| Vec::new()),
        additions: std::array::from_fn(|_| Vec::new()),
        exact_unique_combat: None,
    };
    let budget = usize::from(search.max_active_spend());
    let mut keys = vec![None; budget + 1];
    keys[0] = Some(base.key);
    for stat_idx in 0..COMBAT_STAT_COUNT {
        if !search.active[stat_idx] {
            continue;
        }
        if progress.is_cancelled() {
            return Err("cancelled".to_string());
        }
        let cap = usize::from(search.maxs[stat_idx] - search.mins[stat_idx]).min(budget);
        for add in 0..=cap {
            let mut combat = search.mins;
            combat[stat_idx] += add as u8;
            plan.values[stat_idx].push(evaluate_objective_allocation(
                combat, request, prepared, choice, upgrade, None, data,
            )?);
        }
        let mut next: Vec<Option<ObjectiveKey>> = vec![None; budget + 1];
        let mut additions = vec![Vec::new(); budget + 1];
        for (spent, key) in keys.iter().enumerate() {
            let Some(key) = key else { continue };
            for add in 0..=cap.min(budget - spent) {
                let candidate = add_objective_key(
                    *key,
                    subtract_objective_key(plan.values[stat_idx][add].key, base.key),
                );
                let destination = spent + add;
                let ordering = next[destination].map_or(std::cmp::Ordering::Greater, |current| {
                    if candidate.score == current.score && candidate.ar_total == current.ar_total {
                        std::cmp::Ordering::Equal
                    } else if candidate.score > current.score
                        || candidate.score == current.score && candidate.ar_total > current.ar_total
                    {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Less
                    }
                });
                if ordering.is_gt() {
                    next[destination] = Some(candidate);
                    additions[destination].clear();
                }
                if !ordering.is_lt() {
                    additions[destination].push(add as u8);
                }
            }
        }
        plan.additions[stat_idx] = additions;
        keys = next;
    }
    plan.exact_unique_combat = exact_unique_primary_allocation(
        search, request, prepared, choice, upgrade, data, progress, &plan,
    )?;
    Ok(plan)
}

fn primary_key_better(left: ObjectiveKey, right: ObjectiveKey) -> bool {
    left.score > right.score || left.score == right.score && left.ar_total > right.ar_total
}

fn primary_keys_equal(left: ObjectiveKey, right: ObjectiveKey) -> bool {
    left.score == right.score && left.ar_total == right.ar_total
}

#[allow(clippy::too_many_arguments)]
fn exact_unique_primary_allocation<P: SearchProgress>(
    search: &RelevantStatSearch,
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    choice: &AowChoice<'_>,
    upgrade: u8,
    data: &GameData,
    progress: &P,
    primary: &PrimaryAllocationPlan,
) -> Result<Option<[u8; COMBAT_STAT_COUNT]>, String> {
    if search.remaining_free == 0 {
        return Ok(None);
    }
    let active_stats = (0..COMBAT_STAT_COUNT)
        .filter(|&stat_idx| search.active[stat_idx])
        .collect::<Vec<_>>();

    let mut best = None;
    for spent in search.min_active_spend()..=search.max_active_spend() {
        if progress.is_cancelled() {
            return Err("cancelled".to_string());
        }
        let Some(mut combat) = unique_primary_combat(search, primary, &active_stats, spent) else {
            // A tied predecessor or missing terminal path needs the normal
            // route-aware DP to preserve every candidate and tie-break.
            return Ok(None);
        };
        fill_inactive_stats(search, &mut combat, search.remaining_free - spent);
        let allocation =
            evaluate_objective_allocation(combat, request, prepared, choice, upgrade, None, data)?;
        match best {
            None => best = Some((allocation.key, combat)),
            Some((best_key, _)) if primary_key_better(allocation.key, best_key) => {
                best = Some((allocation.key, combat));
            }
            Some((best_key, _)) if primary_keys_equal(allocation.key, best_key) => {
                // Route metrics are the next tie-break, so the route cannot
                // be skipped when the exact primary metrics tie.
                return Ok(None);
            }
            Some(_) => {}
        }
    }
    Ok(best.map(|(_, combat)| combat))
}

fn unique_primary_combat(
    search: &RelevantStatSearch,
    primary: &PrimaryAllocationPlan,
    active_stats: &[usize],
    spent: u16,
) -> Option<[u8; COMBAT_STAT_COUNT]> {
    let mut combat = search.mins;
    let mut destination = usize::from(spent);
    for &stat_idx in active_stats.iter().rev() {
        let additions = &primary.additions[stat_idx][destination];
        let [add] = additions.as_slice() else {
            return None;
        };
        let add = usize::from(*add);
        combat[stat_idx] = search.mins[stat_idx] + add as u8;
        destination -= add;
    }
    (destination == 0).then_some(combat)
}

#[allow(clippy::too_many_arguments)]
fn best_objective_allocation<P>(
    search: &RelevantStatSearch,
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    route: Option<&ScalarAowRoute<'_>>,
    data: &GameData,
    progress: &P,
    primary: Option<&PrimaryAllocationPlan>,
) -> Result<ObjectiveAllocation, String>
where
    P: SearchProgress,
{
    if search.remaining_free == 0 {
        if progress.is_cancelled() {
            return Err("cancelled".to_string());
        }
        return evaluate_objective_allocation(
            search.mins,
            request,
            prepared,
            aow_choice,
            upgrade,
            route,
            data,
        );
    }
    if let Some(primary) = primary
        && let Some(combat) = primary.exact_unique_combat
    {
        if progress.is_cancelled() {
            return Err("cancelled".to_string());
        }
        return evaluate_objective_allocation(
            combat, request, prepared, aow_choice, upgrade, route, data,
        );
    }
    let base_combat = search.mins;
    let with_route = |mut value: ObjectiveAllocation| -> Result<ObjectiveAllocation, String> {
        if let Some(route) = route {
            let stats = stats_with_combat(request.current_stats, value.combat);
            let metric = evaluate_scalar_aow_route(
                route,
                prepared.weapon,
                upgrade,
                &stats,
                effective_str_for_weapon(request, prepared.weapon, stats.str),
                request.damage_multiplier(),
                data,
            )?;
            value.key.aow_full = metric.full_sequence_damage;
            value.key.aow_first = metric.first_hit_damage;
        }
        Ok(value)
    };
    let base = if let Some(primary) = primary {
        with_route(primary.base)?
    } else {
        evaluate_objective_allocation(
            base_combat,
            request,
            prepared,
            aow_choice,
            upgrade,
            route,
            data,
        )?
    };
    let max_active_spend = search.max_active_spend();
    let mut allocations = vec![None; usize::from(max_active_spend) + 1];
    allocations[0] = Some(base);

    for stat_idx in 0..COMBAT_STAT_COUNT {
        if !search.active[stat_idx] {
            continue;
        }
        if progress.is_cancelled() {
            return Err("cancelled".to_string());
        }
        let cap = u16::from(search.maxs[stat_idx] - search.mins[stat_idx]).min(max_active_spend);
        let mut stat_values = Vec::with_capacity(usize::from(cap) + 1);
        for add in 0..=cap {
            let mut combat = base_combat;
            combat[stat_idx] = search.mins[stat_idx] + add as u8;
            let value = if let Some(primary) = primary {
                with_route(primary.values[stat_idx][usize::from(add)])?
            } else {
                evaluate_objective_allocation(
                    combat, request, prepared, aow_choice, upgrade, route, data,
                )?
            };
            stat_values.push(subtract_objective_key(value.key, base.key));
        }

        let mut next = vec![None; allocations.len()];
        let all_additions = (0..=cap as u8).collect::<Vec<_>>();
        for (destination, slot) in next.iter_mut().enumerate() {
            let additions = primary.map_or(
                &all_additions[..=usize::from(cap).min(destination)],
                |primary| &primary.additions[stat_idx][destination],
            );
            for &add in additions {
                let add = usize::from(add);
                let Some(entry) = allocations[destination - add] else {
                    continue;
                };
                let mut candidate = entry;
                candidate.key = add_objective_key(entry.key, stat_values[add]);
                candidate.combat[stat_idx] = search.mins[stat_idx] + add as u8;
                if slot.is_none_or(|current| better_objective_allocation(candidate, current)) {
                    *slot = Some(candidate);
                }
            }
        }
        allocations = next;
    }

    let uses_separable_primary = matches!(
        request.objective,
        OptimizeObjective::MaxAr
            | OptimizeObjective::MaxPhysicalAr
            | OptimizeObjective::BleedThenAr
    );
    let final_allocations = (search.min_active_spend()..=max_active_spend)
        .filter_map(|spent| allocations[usize::from(spent)].map(|allocation| (spent, allocation)))
        .map(|(spent, mut allocation)| {
            fill_inactive_stats(
                search,
                &mut allocation.combat,
                search.remaining_free - spent,
            );
            let primary = uses_separable_primary
                .then(|| {
                    evaluate_objective_allocation(
                        allocation.combat,
                        request,
                        prepared,
                        aow_choice,
                        upgrade,
                        None,
                        data,
                    )
                })
                .transpose()?;
            Ok((allocation, primary))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let best_primary_key = final_allocations
        .iter()
        .filter_map(|(_, primary)| primary.map(|value| value.key))
        .reduce(|best, key| {
            if primary_key_better(key, best) {
                key
            } else {
                best
            }
        });
    final_allocations
        .into_iter()
        .filter_map(|(allocation, primary)| {
            let Some(best_key) = best_primary_key else {
                return Some(evaluate_objective_allocation(
                    allocation.combat,
                    request,
                    prepared,
                    aow_choice,
                    upgrade,
                    route,
                    data,
                ));
            };
            let primary = primary.expect("separable objectives must have a primary metric");
            primary_keys_equal(primary.key, best_key).then(|| with_route(primary))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .reduce(|best, candidate| {
            if better_objective_allocation(candidate, best) {
                candidate
            } else {
                best
            }
        })
        .ok_or_else(|| "stat optimizer could not satisfy the stat budget".to_string())
}

#[allow(clippy::too_many_arguments)]
fn evaluate_objective_allocation(
    combat: [u8; COMBAT_STAT_COUNT],
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    route: Option<&ScalarAowRoute<'_>>,
    data: &GameData,
) -> Result<ObjectiveAllocation, String> {
    let stats = stats_with_combat(request.current_stats, combat);
    let effective_str_value = effective_str_for_weapon(request, prepared.weapon, stats.str);
    let ar = calculate_ar_with_buffs(
        prepared,
        aow_choice,
        upgrade,
        &stats,
        effective_str_value,
        request.damage_multiplier(),
        data,
    )?;
    let bleed = apply_aow_bleed_buffs(
        calculate_bleed_buildup(prepared.weapon, upgrade, &stats, data)?,
        prepared.weapon,
        upgrade,
        &stats,
        data,
        aow_choice.aow,
    )?;
    let aow = route.map_or(Ok(Default::default()), |route| {
        evaluate_scalar_aow_route(
            route,
            prepared.weapon,
            upgrade,
            &stats,
            effective_str_value,
            request.damage_multiplier(),
            data,
        )
    })?;
    let score = match request.objective {
        OptimizeObjective::MaxAr => ar.total(),
        OptimizeObjective::MaxPhysicalAr => ar.physical,
        OptimizeObjective::BleedThenAr => bleed,
        OptimizeObjective::AowFirstHit => aow.first_hit_damage,
        OptimizeObjective::AowFullSequence => aow.full_sequence_damage,
    };
    Ok(ObjectiveAllocation {
        key: ObjectiveKey {
            score,
            ar_total: ar.total(),
            aow_full: aow.full_sequence_damage,
            aow_first: aow.first_hit_damage,
            bleed,
        },
        ar,
        combat,
    })
}

fn stats_with_combat(mut stats: Stats, combat: [u8; COMBAT_STAT_COUNT]) -> Stats {
    stats.str = combat[STAT_STR];
    stats.dex = combat[STAT_DEX];
    stats.int = combat[STAT_INT];
    stats.fai = combat[STAT_FAI];
    stats.arc = combat[STAT_ARC];
    stats
}

fn add_objective_key(left: ObjectiveKey, right: ObjectiveKey) -> ObjectiveKey {
    ObjectiveKey {
        score: left.score + right.score,
        ar_total: left.ar_total + right.ar_total,
        aow_full: left.aow_full + right.aow_full,
        aow_first: left.aow_first + right.aow_first,
        bleed: left.bleed + right.bleed,
    }
}

fn subtract_objective_key(left: ObjectiveKey, right: ObjectiveKey) -> ObjectiveKey {
    ObjectiveKey {
        score: left.score - right.score,
        ar_total: left.ar_total - right.ar_total,
        aow_full: left.aow_full - right.aow_full,
        aow_first: left.aow_first - right.aow_first,
        bleed: left.bleed - right.bleed,
    }
}

fn better_objective_allocation(
    candidate: ObjectiveAllocation,
    current: ObjectiveAllocation,
) -> bool {
    candidate.key.score > current.key.score
        || candidate.key.score == current.key.score && candidate.key.ar_total > current.key.ar_total
        || candidate.key.score == current.key.score
            && candidate.key.ar_total == current.key.ar_total
            && candidate.key.aow_full > current.key.aow_full
        || candidate.key.score == current.key.score
            && candidate.key.ar_total == current.key.ar_total
            && candidate.key.aow_full == current.key.aow_full
            && candidate.key.aow_first > current.key.aow_first
        || candidate.key.score == current.key.score
            && candidate.key.ar_total == current.key.ar_total
            && candidate.key.aow_full == current.key.aow_full
            && candidate.key.aow_first == current.key.aow_first
            && candidate.key.bleed > current.key.bleed
        || candidate.key.score == current.key.score
            && candidate.key.ar_total == current.key.ar_total
            && candidate.key.aow_full == current.key.aow_full
            && candidate.key.aow_first == current.key.aow_first
            && candidate.key.bleed == current.key.bleed
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
    let effective_str_value =
        effective_str_for_weapon(request, prepared.weapon, candidate.stats.str);
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
    let effective_str_value =
        effective_str_for_weapon(request, prepared.weapon, candidate.stats.str);
    let (first, full) = calculate_aow_metric(
        request.objective,
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
        candidate.metric.bleed_buildup = None;
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
        let effective_str_value = effective_str_for_weapon(request, prepared.weapon, stats.str);
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
    let effective_str_value =
        effective_str_for_weapon(request, prepared.weapon, candidate.stats.str);
    let full = complete_candidate_metric(
        request.objective,
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
        bleed_buildup: _,
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
        sleep_buildup: status_buildup
            .expect("complete candidate metric must include status")
            .sleep,
        madness_buildup: status_buildup
            .expect("complete candidate metric must include status")
            .madness,
        death_buildup: status_buildup
            .expect("complete candidate metric must include status")
            .death,
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
    let resolved_attack_rows;
    let attack_rows = if aow_choice.attack_rows.is_empty() {
        resolved_attack_rows = if !prepared.weapon.can_change_aow {
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
        return Ok(None);
    }
    let mut routes = calculate_aow_routes(
        prepared.weapon,
        attack_rows,
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
    Ok(select_best_aow_route(routes, objective))
}

fn select_best_aow_route(
    routes: Vec<AowRouteResult>,
    objective: OptimizeObjective,
) -> Option<AowRouteResult> {
    routes.into_iter().reduce(|best, candidate| {
        let candidate_primary = match objective {
            OptimizeObjective::AowFirstHit => candidate.first_hit_damage,
            _ => candidate.total_damage.total(),
        };
        let best_primary = match objective {
            OptimizeObjective::AowFirstHit => best.first_hit_damage,
            _ => best.total_damage.total(),
        };
        let candidate_secondary = match objective {
            OptimizeObjective::AowFirstHit => candidate.total_damage.total(),
            _ => candidate.first_hit_damage,
        };
        let best_secondary = match objective {
            OptimizeObjective::AowFirstHit => best.total_damage.total(),
            _ => best.first_hit_damage,
        };
        if candidate_primary > best_primary
            || candidate_primary == best_primary && candidate_secondary > best_secondary
            || candidate_primary == best_primary
                && candidate_secondary == best_secondary
                && (candidate.route_priority, &candidate.route_id)
                    < (best.route_priority, &best.route_id)
        {
            candidate
        } else {
            best
        }
    })
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
        return Err(format!(
            "combat stat floors require {mandatory_raise} points, but the level budget has {free_points}"
        ));
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
        if !data.weapon_ar_supported(weapon) || !weapon_matches_request(weapon, request, data) {
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
                bleed_buildup: None,
                aow_first_hit_damage: None,
                aow_full_sequence_damage: None,
            })
        }
        OptimizeObjective::BleedThenAr => {
            let bleed_buildup = apply_aow_bleed_buffs(
                base_metric
                    .bleed_buildup
                    .expect("AR + Bleed must prepare base bleed buildup"),
                prepared.weapon,
                upgrade,
                stats,
                data,
                aow_choice.aow,
            )?;
            Ok(CandidateMetric {
                score: bleed_buildup,
                ar: None,
                status_buildup: None,
                bleed_buildup: Some(bleed_buildup),
                aow_first_hit_damage: None,
                aow_full_sequence_damage: None,
            })
        }
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence => {
            let (aow_first_hit_damage, aow_full_sequence_damage) = calculate_aow_metric(
                objective,
                prepared,
                aow_choice,
                upgrade,
                stats,
                effective_str_value,
                damage_multiplier,
                data,
            )?;
            Ok(CandidateMetric {
                score: match objective {
                    OptimizeObjective::AowFirstHit => aow_first_hit_damage,
                    OptimizeObjective::AowFullSequence => aow_full_sequence_damage,
                    _ => unreachable!("AoW branch requires an AoW objective"),
                },
                ar: None,
                status_buildup: None,
                bleed_buildup: None,
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
            bleed_buildup: None,
        }),
        OptimizeObjective::BleedThenAr => Ok(BaseWeaponMetric {
            ar: None,
            bleed_buildup: Some(calculate_bleed_buildup(
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
    objective: OptimizeObjective,
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
            objective,
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
        score: ar.total(),
        ar: Some(ar),
        status_buildup: Some(status_buildup),
        bleed_buildup: Some(status_buildup.bleed),
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

#[cfg(test)]
fn calculate_bleed_with_buffs(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    data: &GameData,
) -> Result<f32, String> {
    apply_aow_bleed_buffs(
        calculate_bleed_buildup(prepared.weapon, upgrade, stats, data)?,
        prepared.weapon,
        upgrade,
        stats,
        data,
        aow_choice.aow,
    )
}

#[allow(clippy::too_many_arguments)]
fn calculate_aow_metric(
    objective: OptimizeObjective,
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
        resolved_attack_rows = if !prepared.weapon.can_change_aow {
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
    let routes = calculate_aow_routes(
        prepared.weapon,
        attack_rows,
        upgrade,
        stats,
        effective_str_value,
        data,
    )?;
    let Some(route) = select_best_aow_route(routes, objective) else {
        return Ok((0.0, 0.0));
    };
    Ok((
        route.first_hit_damage * damage_multiplier,
        route.total_damage.total() * damage_multiplier,
    ))
}

fn weapon_matches_request(weapon: &Weapon, request: &OptimizeRequest, data: &GameData) -> bool {
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
    let reinforcement_matches = match request.somber_filter {
        SomberFilter::All => true,
        SomberFilter::StandardOnly => !weapon.is_somber,
        SomberFilter::SomberOnly => weapon.is_somber,
    };
    if !reinforcement_matches {
        return false;
    }
    let family_id = weapon.family_filter_id();
    let type_id = weapon.type_filter_id();
    let affinity_id = weapon.affinity_filter_id();
    let reinforcement_id = if weapon.is_somber {
        "reinforcement:somber"
    } else {
        "reinforcement:standard"
    };
    let mut coverage_ids = vec!["coverage:weapon-ar"];
    if data.capabilities.status_buildup {
        coverage_ids.push("coverage:status");
    }
    if data.capabilities.aow_compatibility {
        coverage_ids.push("coverage:aow-compatibility");
    }
    if data.capabilities.aow_damage {
        coverage_ids.push("coverage:aow-damage");
    }
    if data.capabilities.aow_routes {
        coverage_ids.push("coverage:aow-routes");
    }
    filter_dimension_matches(request, FilterDimension::WeaponFamily, |id| {
        id.eq_ignore_ascii_case(&family_id)
    }) && filter_dimension_matches(request, FilterDimension::WeaponType, |id| {
        id.eq_ignore_ascii_case(&type_id)
    }) && filter_dimension_matches(request, FilterDimension::Affinity, |id| {
        id.eq_ignore_ascii_case(&affinity_id)
    }) && filter_dimension_matches(request, FilterDimension::Reinforcement, |id| {
        id.eq_ignore_ascii_case(reinforcement_id)
    }) && filter_dimension_matches(request, FilterDimension::Coverage, |id| {
        coverage_ids
            .iter()
            .any(|value| id.eq_ignore_ascii_case(value))
    })
}

fn filter_dimension_matches(
    request: &OptimizeRequest,
    dimension: FilterDimension,
    matches_id: impl Fn(&str) -> bool,
) -> bool {
    let mut has_include = false;
    let mut include_matches = false;
    for filter in request
        .filters
        .iter()
        .filter(|filter| filter.dimension == dimension)
    {
        match filter.mode {
            FilterMode::Exclude if matches_id(&filter.id) => return false,
            FilterMode::Include => {
                has_include = true;
                include_matches |= matches_id(&filter.id);
            }
            FilterMode::Exclude => {}
        }
    }
    !has_include || include_matches
}

fn weapon_type_matches(weapon: &Weapon, type_key: &str) -> bool {
    normalize_weapon_type_display(&weapon.weapon_type_name).eq_ignore_ascii_case(type_key)
        || weapon.weapon_type_name.eq_ignore_ascii_case(type_key)
        || weapon
            .weapon_type_keys
            .split('|')
            .any(|key| key.eq_ignore_ascii_case(type_key))
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
    let native = native_skill_choice_for_weapon(weapon, data, request.objective);
    let mut choices: Vec<_> = native.clone().into_iter().collect();
    if native.is_none() && weapon.affinity.eq_ignore_ascii_case("Standard") {
        choices.push(AowChoice {
            no_applied_ash: true,
            aow: None,
            skill_id: None,
            skill_name: None,
            attack_rows: Vec::new(),
            scalar_routes: None,
        });
    }
    for aow in data
        .aows
        .iter()
        .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
    {
        if let Some(choice) = native
            .as_ref()
            .filter(|choice| choice.skill_id == Some(aow.aow_id))
        {
            if choice.no_applied_ash {
                let mut applied = choice.clone();
                applied.no_applied_ash = false;
                choices.push(applied);
            }
        } else {
            choices.push(AowChoice {
                no_applied_ash: false,
                aow: Some(aow),
                skill_id: Some(aow.aow_id),
                skill_name: Some(aow.name.as_str()),
                attack_rows: select_aow_attack_rows(aow.aow_id, weapon, data),
                scalar_routes: None,
            });
        }
    }
    if let Some(name) = request.aow_name.as_deref() {
        choices.retain(|choice| {
            choice
                .skill_name
                .is_some_and(|skill| skill.eq_ignore_ascii_case(name))
        });
        if choices.is_empty() {
            let known = data
                .aows
                .iter()
                .any(|aow| aow.name.eq_ignore_ascii_case(name))
                || data.weapons.iter().any(|candidate| {
                    candidate
                        .native_skill_name
                        .as_deref()
                        .is_some_and(|skill| skill.eq_ignore_ascii_case(name))
                        && data.native_skill_compatible_with_weapon(candidate)
                });
            if !known {
                return Err(format!("unknown AoW: {name}"));
            }
        }
    }
    let mut seen_skills = HashSet::new();
    choices.retain(|choice| {
        if matches!(
            request.objective,
            OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
        ) && choice.attack_rows.is_empty()
        {
            return false;
        }
        if matches!(
            request.objective,
            OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
        ) && choice.attack_rows.iter().any(|row| {
            !row.is_lacking_fp
                && data
                    .aow_effects(row.aow_id, row.sheet_row)
                    .iter()
                    .any(|effect| !effect.is_supported)
        }) {
            return false;
        }
        let skill_id = choice.skill_id.map(|id| format!("aow:{id}"));
        filter_dimension_matches(request, FilterDimension::Aow, |id| {
            if id.eq_ignore_ascii_case("aow:none") {
                choice.no_applied_ash
            } else {
                skill_id
                    .as_ref()
                    .is_some_and(|skill| id.eq_ignore_ascii_case(skill))
            }
        }) && seen_skills.insert(choice.skill_id)
    });
    if rayon::current_num_threads() == 1 {
        for choice in &mut choices {
            choice.scalar_routes = Some(
                if choice.attack_rows.iter().any(|row| {
                    !row.is_lacking_fp
                        && data
                            .aow_effects(row.aow_id, row.sheet_row)
                            .iter()
                            .any(|effect| {
                                effect.is_supported
                                    && effect.role == AowEffectRole::PerHitAttackPower
                            })
                }) {
                    Ok(None)
                } else {
                    prepare_scalar_aow_routes(&choice.attack_rows, data)
                },
            );
        }
    }
    Ok((!choices.is_empty()).then_some(choices))
}

fn native_skill_choice_for_weapon<'a>(
    weapon: &'a Weapon,
    data: &'a GameData,
    _objective: OptimizeObjective,
) -> Option<AowChoice<'a>> {
    if !data.native_skill_compatible_with_weapon(weapon) {
        return None;
    }
    let native_skill_id = weapon.native_skill_id?;
    let exact_rows = data.native_skill_attack_rows(weapon.weapon_id);
    let source_rows = if exact_rows.is_empty() {
        data.aow_attack_rows(native_skill_id)
    } else {
        exact_rows
    };
    let attack_rows = select_attack_rows(source_rows, weapon);
    let skill_name = weapon
        .native_skill_name
        .as_deref()
        .or_else(|| source_rows.first().map(|row| row.aow_name.as_str()))?;
    Some(AowChoice {
        no_applied_ash: weapon.affinity.eq_ignore_ascii_case("Standard"),
        aow: data.aows.iter().find(|aow| aow.aow_id == native_skill_id),
        skill_id: Some(native_skill_id),
        skill_name: Some(skill_name),
        attack_rows,
        scalar_routes: None,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RelevantStatSearch {
    mins: [u8; COMBAT_STAT_COUNT],
    maxs: [u8; COMBAT_STAT_COUNT],
    active: [bool; COMBAT_STAT_COUNT],
    remaining_free: u16,
    candidate_count: u64,
}

fn relevant_stat_search(
    request: &OptimizeRequest,
    data: &GameData,
    constraints: CombatConstraints,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    distribution_counts: &mut HashMap<DistributionCountKey, u64>,
) -> Option<RelevantStatSearch> {
    let active = active_stats_for_choice(request, prepared, aow_choice, data);
    RelevantStatSearch::new(
        request,
        constraints,
        prepared.weapon,
        active,
        distribution_counts,
    )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DistributionCountKey {
    mins: [u8; COMBAT_STAT_COUNT],
    maxs: [u8; COMBAT_STAT_COUNT],
    active: [bool; COMBAT_STAT_COUNT],
    remaining_free: u16,
}

impl RelevantStatSearch {
    fn new(
        request: &OptimizeRequest,
        constraints: CombatConstraints,
        weapon: &Weapon,
        active: [bool; COMBAT_STAT_COUNT],
        distribution_counts: &mut HashMap<DistributionCountKey, u64>,
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

        let count_key = DistributionCountKey {
            mins,
            maxs,
            active,
            remaining_free,
        };
        let candidate_count = *distribution_counts
            .entry(count_key)
            .or_insert_with(|| count_relevant_distributions(&mins, &maxs, &active, remaining_free));
        (candidate_count > 0).then_some(Self {
            mins,
            maxs,
            active,
            remaining_free,
            candidate_count,
        })
    }

    fn visit<F>(&self, current: &mut [u8; COMBAT_STAT_COUNT], mut visitor: F)
    where
        F: FnMut(&[u8; COMBAT_STAT_COUNT]) -> bool,
    {
        visit_relevant_stat_candidates_inner(0, self.remaining_free, self, current, &mut visitor);
    }

    fn inactive_capacity(&self) -> u16 {
        (0..COMBAT_STAT_COUNT)
            .filter(|idx| !self.active[*idx])
            .map(|idx| u16::from(self.maxs[idx] - self.mins[idx]))
            .sum()
    }

    fn min_active_spend(&self) -> u16 {
        self.remaining_free.saturating_sub(self.inactive_capacity())
    }

    fn max_active_spend(&self) -> u16 {
        let active_capacity: u16 = (0..COMBAT_STAT_COUNT)
            .filter(|idx| self.active[*idx])
            .map(|idx| u16::from(self.maxs[idx] - self.mins[idx]))
            .sum();
        self.remaining_free.min(active_capacity)
    }
}

fn count_relevant_distributions(
    mins: &[u8; COMBAT_STAT_COUNT],
    maxs: &[u8; COMBAT_STAT_COUNT],
    active: &[bool; COMBAT_STAT_COUNT],
    remaining_free: u16,
) -> u64 {
    let inactive_capacity = (0..COMBAT_STAT_COUNT)
        .filter(|idx| !active[*idx])
        .map(|idx| u16::from(maxs[idx] - mins[idx]))
        .sum();
    let mut memo: HashMap<(usize, u16), u64> = HashMap::new();
    count_active_distributions(
        mins,
        maxs,
        active,
        inactive_capacity,
        0,
        remaining_free,
        &mut memo,
    )
}

fn count_active_distributions(
    mins: &[u8; COMBAT_STAT_COUNT],
    maxs: &[u8; COMBAT_STAT_COUNT],
    active: &[bool; COMBAT_STAT_COUNT],
    inactive_capacity: u16,
    idx: usize,
    remaining_free: u16,
    memo: &mut HashMap<(usize, u16), u64>,
) -> u64 {
    if idx == COMBAT_STAT_COUNT {
        return if remaining_free <= inactive_capacity {
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
                    inactive_capacity,
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
            inactive_capacity,
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
        if remaining_free > search.inactive_capacity() {
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
    let requirement_mins = weapon_requirement_mins(request, weapon);
    let mut remaining_free = constraints.remaining_free;
    let mut capacity = 0_u16;
    for (idx, &requirement_min) in requirement_mins.iter().enumerate() {
        if requirement_min > constraints.maxs[idx] {
            return false;
        }
        let minimum = constraints.mins[idx].max(requirement_min);
        let raise = u16::from(minimum - constraints.mins[idx]);
        if raise > remaining_free {
            return false;
        }
        remaining_free -= raise;
        capacity = capacity.saturating_add(u16::from(constraints.maxs[idx] - minimum));
    }
    remaining_free <= capacity
}

fn weapon_requirement_mins(request: &OptimizeRequest, weapon: &Weapon) -> [u8; COMBAT_STAT_COUNT] {
    std::array::from_fn(|idx| {
        if idx == STAT_STR {
            minimum_str_for_requirement(
                weapon.requirements[STAT_STR],
                weapon_uses_two_handing(request, weapon),
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
    let _ = request;
    std::array::from_fn(|idx| {
        weapon_stat_can_increase_ar(prepared.weapon, data, idx)
            || stat_can_increase_bleed_for_choice(prepared, aow_choice, data, idx)
            || aow_choice
                .attack_rows
                .iter()
                .any(|row| attack_row_stat_can_increase_damage(prepared.weapon, row, data, idx))
    })
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
            || (row.is_add_base_atk || row.is_arrow_attack) && row.attack_base[damage_idx] > 0.0;
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
        weapon.scaling[stat_idx] > 0.0
            && data
                .attack_element(weapon.attack_element_correct_id)
                .is_none_or(|aec| aec.stat_scales(stat_idx, *damage_type))
    })
}

fn stat_can_increase_bleed_for_choice(
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    data: &GameData,
    stat_idx: usize,
) -> bool {
    if !data.rules.status_buildup_scales {
        return false;
    }

    prepared.upgrades.iter().any(|upgrade| {
        let mut source = data.weapon_passive(prepared.weapon.weapon_id);
        if let Some(overlay) = data.weapon_passive_overlay(prepared.weapon.weapon_id, *upgrade) {
            merge_status_relevance_value(
                &mut source.buildup.bleed,
                &mut source.correction_flags.bleed,
                overlay.buildup.bleed,
                overlay.correction_flags.bleed,
            );
        }
        bleed_source_stat_can_scale(source, prepared.weapon, stat_idx)
    }) || aow_choice.aow.is_some_and(|aow| {
        bleed_source_stat_can_scale(aow_status_source(aow), prepared.weapon, stat_idx)
    })
}

fn aow_status_source(aow: &Aow) -> StatusEffectSource {
    StatusEffectSource {
        buildup: aow.scaling_status_add,
        correction_flags: aow.scaling_status_flags,
    }
}

fn merge_status_relevance_value(
    base_value: &mut f32,
    base_flag: &mut Option<bool>,
    overlay_value: f32,
    overlay_flag: Option<bool>,
) {
    if overlay_value > 0.0 {
        *base_value = overlay_value;
        if overlay_flag.is_some() {
            *base_flag = overlay_flag;
        }
    }
}

fn bleed_source_stat_can_scale(
    source: StatusEffectSource,
    weapon: &Weapon,
    stat_idx: usize,
) -> bool {
    if stat_idx != STAT_ARC || weapon.scaling[stat_idx] <= 0.0 {
        return false;
    }
    status_value_can_scale(source.buildup.bleed, source.correction_flags.bleed)
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
