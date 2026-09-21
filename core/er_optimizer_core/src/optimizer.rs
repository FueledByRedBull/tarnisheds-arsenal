use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::math::exact::{project, project_damage, sum_components};
use crate::math::exact_value::ExactRational;
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, ToPrimitive, Zero};
use rayon::prelude::*;

use crate::math::ScalarAowRoute;
use crate::math::{
    apply_aow_status_buffs, calculate_aow_routes_scaled_with_cancel, calculate_status_buildup,
    class_by_name, compute_free_points, effective_str, meets_requirements,
    prepare_scalar_aow_routes,
};
#[cfg(test)]
use crate::math::{calculate_aow_routes, evaluate_scalar_aow_route};
use crate::model::{
    Aow, AowAttackRow, AowEffectRole, AowRouteResult, COMBAT_STAT_COUNT, DamageBreakdown,
    DamageType, GameData, STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, StatusBuildup,
    StatusEffectSource, Weapon, normalize_weapon_type_display,
};

mod types;
pub use types::*;
mod ranking;
use ranking::*;
mod exact_dp;

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

type ScalarRouteSet<'routes, 'data> = Cow<'routes, [ScalarAowRoute<'data>]>;

fn scalar_route_set<'routes, 'data>(
    choice: &'routes AowChoice<'data>,
    data: &'data GameData,
) -> Result<Option<ScalarRouteSet<'routes, 'data>>, String> {
    match &choice.scalar_routes {
        Some(Ok(routes)) => Ok(routes
            .as_ref()
            .map(|routes| Cow::Borrowed(routes.as_slice()))),
        Some(Err(error)) => Err(error.clone()),
        None => prepare_scalar_aow_routes(&choice.attack_rows, data)
            .map(|routes| routes.map(Cow::Owned)),
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
        validate_reusable_loadout(&self.template, request, self.data)?;
        let constraints = build_combat_constraints(request)?;
        if request.top_k == 1 && request.locked_combat_stats.iter().all(Option::is_some) {
            let stats = stats_with_combat(request.current_stats, constraints.mins);
            let mut upgrades = self
                .weapons
                .iter()
                .flat_map(|weapon| weapon.upgrades.iter().copied())
                .collect::<Vec<_>>();
            upgrades.sort_unstable();
            upgrades.dedup();
            let mut rows = Vec::new();
            for upgrade in upgrades {
                if let Some(row) = evaluate_fixed_loadout_upgrade(
                    request,
                    self.data,
                    &self.weapons,
                    upgrade,
                    stats,
                    &mut should_continue,
                )? {
                    push_top_k(&mut rows, row, 1, ResultGroupMode::Loadout);
                }
            }
            return Ok(rows);
        }
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

    /// All nondominated AR/bleed pairs for this fixed loadout and stat budget.
    pub fn evaluate_ar_bleed_frontier_with_cancel<F>(
        &self,
        request: &OptimizeRequest,
        mut should_continue: F,
    ) -> Result<Vec<ArBleedFrontierPoint>, String>
    where
        F: FnMut() -> bool + Send,
    {
        validate_reusable_loadout(&self.template, request, self.data)?;
        if !self.data.capabilities.status_buildup {
            return Err("selected profile does not provide status-buildup data".into());
        }
        if !should_continue() {
            return Err("cancelled".into());
        }
        let constraints = build_combat_constraints(request)?;
        let mut weapons = self.weapons.to_vec();
        for weapon in &mut weapons {
            weapon
                .aow_choices
                .retain(|choice| match request.aow_name.as_deref() {
                    Some(name) => choice
                        .skill_name
                        .is_some_and(|skill| skill.eq_ignore_ascii_case(name)),
                    None => choice.no_applied_ash || choice.skill_name.is_none(),
                });
        }
        if weapons.len() != 1 || weapons[0].aow_choices.len() != 1 || weapons[0].upgrades.len() != 1
        {
            return Err(
                "AR / bleed tradeoffs require one weapon, affinity, skill, and upgrade".into(),
            );
        }
        let mut fixed = request.clone();
        fixed.objective = OptimizeObjective::MaxAr;
        fixed.result_grouping = ResultGrouping::Loadout;
        fixed.top_k = 1;
        let evaluator = PreparedLoadoutEvaluator {
            template: fixed.clone(),
            data: self.data,
            weapons: Arc::from(weapons),
        };
        let other_capacity: u16 = (0..COMBAT_STAT_COUNT)
            .filter(|&stat| stat != STAT_ARC)
            .map(|stat| u16::from(constraints.maxs[stat] - constraints.mins[stat]))
            .sum();
        let min_spend = constraints.remaining_free.saturating_sub(other_capacity);
        let max_spend = constraints.remaining_free.min(u16::from(
            constraints.maxs[STAT_ARC] - constraints.mins[STAT_ARC],
        ));
        let candidates =
            match ar_bleed_candidates_with_reuse(&evaluator, &fixed, &mut should_continue)? {
                Some(candidates) => candidates,
                None => {
                    let mut candidates = Vec::new();
                    // Coupled routes retain the ordinary solver's exhaustive path.
                    for spend in min_spend..=max_spend {
                        if !should_continue() {
                            return Err("cancelled".into());
                        }
                        fixed.locked_combat_stats[STAT_ARC] =
                            Some(constraints.mins[STAT_ARC] + spend as u8);
                        candidates
                            .extend(evaluator.evaluate_with_cancel(&fixed, &mut should_continue)?);
                    }
                    candidates
                }
            };
        build_ar_bleed_frontier(candidates, &mut should_continue)
    }
}

fn build_ar_bleed_frontier(
    mut candidates: Vec<OptimizeResult>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Vec<ArBleedFrontierPoint>, String> {
    candidates.sort_by(|a, b| {
        b.exact_key
            .ar_total
            .cmp(&a.exact_key.ar_total)
            .then_with(|| b.exact_key.bleed.cmp(&a.exact_key.bleed))
            .then_with(|| {
                if better_result(a, b) {
                    std::cmp::Ordering::Less
                } else if better_result(b, a) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
    });
    let Some(first) = candidates.first() else {
        return Ok(Vec::new());
    };
    let max_ar = first.exact_key.ar_total.clone();
    let base_bleed = first.exact_key.bleed.clone();
    let mut best_bleed = None;
    let mut frontier = Vec::new();
    for result in candidates {
        if !should_continue() {
            return Err("cancelled".into());
        }
        if best_bleed
            .as_ref()
            .is_some_and(|best| result.exact_key.bleed <= *best)
        {
            continue;
        }
        best_bleed = Some(result.exact_key.bleed.clone());
        let loss = &max_ar - &result.exact_key.ar_total;
        frontier.push(ArBleedFrontierPoint {
            ar_loss: project(&loss)?,
            ar_loss_percent: if max_ar.is_zero() {
                0.0
            } else {
                project(&(&loss * rational(100.0)? / &max_ar))?
            },
            minimum_ar_loss_bps: minimum_ar_loss_bps(&loss, &max_ar)?,
            bleed_gain: project(&(&result.exact_key.bleed - &base_bleed))?,
            result,
        });
    }
    Ok(frontier)
}

fn ar_bleed_candidates_with_reuse(
    evaluator: &PreparedLoadoutEvaluator<'_>,
    request: &OptimizeRequest,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<Vec<OptimizeResult>>, String> {
    if !should_continue() {
        return Err("cancelled".into());
    }
    let prepared = &evaluator.weapons[0];
    let choice = &prepared.aow_choices[0];
    let upgrade = prepared.upgrades[0];
    let data = evaluator.data;
    let Some(routes) = scalar_route_set(choice, data)? else {
        return Ok(None);
    };
    if routes.iter().any(|route| !route.is_additive(data)) {
        return Ok(None);
    }
    let constraints = build_combat_constraints(request)?;
    let Some(search) = relevant_stat_search(
        request,
        data,
        constraints,
        prepared,
        choice,
        &mut HashMap::new(),
    ) else {
        return Ok(Some(Vec::new()));
    };
    let maxima = formula_max_stats(&search, request, prepared.weapon);
    let ar_formula =
        crate::math::exact::compile_ar_formula(prepared.weapon, upgrade, data, maxima)?;
    let arc_capacity = search
        .remaining_free
        .min(u16::from(search.maxs[STAT_ARC] - search.mins[STAT_ARC]));
    let mut best: Vec<Option<ObjectiveAllocation>> = vec![None; usize::from(arc_capacity) + 1];
    let mut other = search;
    other.active[STAT_ARC] = false;
    other.maxs[STAT_ARC] = other.mins[STAT_ARC];
    let budget = usize::from(other.max_active_spend());
    let routes = if routes.is_empty() {
        vec![None]
    } else {
        routes.iter().map(Some).collect()
    };
    for route in routes {
        if !should_continue() {
            return Err("cancelled".into());
        }
        let route_formula = route
            .map(|route| {
                crate::math::exact::compile_scalar_route_formula(
                    route,
                    prepared.weapon,
                    upgrade,
                    request.damage_multiplier(),
                    data,
                    maxima,
                )
            })
            .transpose()?;
        let evaluate = |combat| {
            evaluate_allocation_with_formulas(
                combat,
                request,
                prepared,
                choice,
                upgrade,
                route,
                data,
                Some(&ar_formula),
                route_formula.as_ref(),
            )
        };
        let base = evaluate(search.mins)?.key.components();
        let mut deltas = std::array::from_fn(|_| Vec::new());
        for stat in 0..COMBAT_STAT_COUNT {
            if !other.active[stat] {
                continue;
            }
            let cap = usize::from(other.maxs[stat] - other.mins[stat]).min(budget);
            for add in 0..=cap {
                if !should_continue() {
                    return Err("cancelled".into());
                }
                let mut combat = search.mins;
                combat[stat] += add as u8;
                let value = evaluate(combat)?.key.components();
                deltas[stat].push(std::array::from_fn(|component| {
                    &value[component] - &base[component]
                }));
            }
        }
        // ARC contributes a constant key at each slice. All remaining deltas,
        // including route ties, are shared; ranks remain comparable across spends.
        let solved = exact_dp::solve_exact::<5>(
            &std::array::from_fn(|_| ExactRational::zero()),
            &deltas,
            other.mins,
            other.active,
            budget,
            false,
            None,
            should_continue,
        )?;
        for arc_spend in 0..=arc_capacity {
            if !should_continue() {
                return Err("cancelled".into());
            }
            let mut slice = other;
            slice.remaining_free -= arc_spend;
            slice.mins[STAT_ARC] += arc_spend as u8;
            slice.maxs[STAT_ARC] = slice.mins[STAT_ARC];
            let mut winner = None;
            for spent in slice.min_active_spend()..=slice.max_active_spend() {
                if !should_continue() {
                    return Err("cancelled".into());
                }
                let Some(mut combat) = solved.combat[usize::from(spent)] else {
                    continue;
                };
                combat[STAT_ARC] = slice.mins[STAT_ARC];
                fill_inactive_stats(&slice, &mut combat, slice.remaining_free - spent);
                let rank = solved.ranks[usize::from(spent)].expect("reachable DP rank");
                if winner.is_none_or(|(current_rank, current_combat)| {
                    rank > current_rank || rank == current_rank && combat < current_combat
                }) {
                    winner = Some((rank, combat));
                }
            }
            if let Some((_, combat)) = winner {
                let candidate = evaluate(combat)?;
                let current = &mut best[usize::from(arc_spend)];
                if current
                    .as_ref()
                    .is_none_or(|current| better_objective_allocation(&candidate, current))
                {
                    *current = Some(candidate);
                }
            }
        }
    }
    best.into_iter()
        .flatten()
        .map(|value| {
            let candidate = ScoredCandidate {
                prepared_idx: 0,
                aow_idx: 0,
                upgrade,
                stats: stats_with_combat(request.current_stats, value.combat),
                metric: metric_from_allocation(&value)?,
                key: value.key,
                route_id: value.route_id,
            };
            materialize_scored_candidate(
                candidate,
                request,
                data,
                &evaluator.weapons,
                request.damage_multiplier(),
                should_continue,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
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
        validate_reusable_loadout(&self.template, request, self.data)?;
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
    ar: DamageBreakdown,
    aow_first_hit_damage: f32,
    aow_full_sequence_damage: f32,
}

#[derive(Clone, Debug)]
struct ScoredCandidate {
    prepared_idx: usize,
    aow_idx: usize,
    upgrade: u8,
    stats: Stats,
    metric: CandidateMetric,
    key: ObjectiveKey,
    route_id: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct SearchWorkUnit {
    group_idx: usize,
    aow_start: usize,
    aow_end: usize,
    candidate_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DpSearchShape {
    mins: [u8; COMBAT_STAT_COUNT],
    maxs: [u8; COMBAT_STAT_COUNT],
    active: [bool; COMBAT_STAT_COUNT],
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DpCacheStage {
    Primary,
    Final {
        route_id: Option<String>,
        allowed: bool,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DpCacheKey {
    shape: DpSearchShape,
    prepared_idx: usize,
    aow_idx: usize,
    upgrade: u8,
    stage: DpCacheStage,
}

type DpAdditions = [Vec<Vec<u8>>; COMBAT_STAT_COUNT];

struct CachedDpResult {
    combat: Vec<Option<[u8; COMBAT_STAT_COUNT]>>,
    additions: Arc<DpAdditions>,
    ranks: Arc<Vec<Option<usize>>>,
}

enum DpSolve {
    Owned(exact_dp::DpResult),
    Shared(Arc<CachedDpResult>),
}

impl DpSolve {
    fn combat(&self) -> &[Option<[u8; COMBAT_STAT_COUNT]>] {
        match self {
            Self::Owned(result) => &result.combat,
            Self::Shared(result) => &result.combat,
        }
    }

    fn additions(&self) -> &DpAdditions {
        match self {
            Self::Owned(result) => &result.additions,
            Self::Shared(result) => result.additions.as_ref(),
        }
    }

    fn ranks(&self) -> &[Option<usize>] {
        match self {
            Self::Owned(result) => &result.ranks,
            Self::Shared(result) => result.ranks.as_ref(),
        }
    }

    fn into_shared_parts(self) -> (Arc<DpAdditions>, Arc<Vec<Option<usize>>>) {
        match self {
            Self::Owned(result) => (Arc::new(result.additions), Arc::new(result.ranks)),
            Self::Shared(result) => (result.additions.clone(), result.ranks.clone()),
        }
    }
}

struct DpReuse {
    max_searches: HashMap<DpSearchShape, RelevantStatSearch>,
    solved: HashMap<DpCacheKey, Arc<CachedDpResult>>,
    primary: HashMap<DpCacheKey, Arc<PrimaryPreparation>>,
}

impl DpReuse {
    fn from_plan(plan: &PreparedSearchPlan<'_>) -> Self {
        let mut max_searches = HashMap::new();
        for group in &plan.groups {
            max_searches
                .entry(group.search.shape())
                .and_modify(|current: &mut RelevantStatSearch| {
                    if group.search.remaining_free > current.remaining_free {
                        *current = group.search;
                    }
                })
                .or_insert(group.search);
        }
        Self {
            max_searches,
            solved: HashMap::new(),
            primary: HashMap::new(),
        }
    }

    fn max_search(&self, search: &RelevantStatSearch) -> Option<RelevantStatSearch> {
        let max_search = self.max_searches.get(&search.shape())?;
        (max_search.remaining_free >= search.remaining_free).then_some(*max_search)
    }

    #[allow(clippy::too_many_arguments)]
    fn solve<const N: usize>(
        &mut self,
        key: DpCacheKey,
        base: &[ExactRational; N],
        deltas: &[Vec<[ExactRational; N]>; COMBAT_STAT_COUNT],
        mins: [u8; COMBAT_STAT_COUNT],
        active: [bool; COMBAT_STAT_COUNT],
        budget: usize,
        primary_only: bool,
        allowed: Option<&[Vec<Vec<u8>>; COMBAT_STAT_COUNT]>,
        should_continue: &mut impl FnMut() -> bool,
    ) -> Result<Arc<CachedDpResult>, String> {
        if let Some(result) = self.solved.get(&key) {
            if !should_continue() {
                return Err("cancelled".to_string());
            }
            return Ok(Arc::clone(result));
        }
        let result = exact_dp::solve_exact(
            base,
            deltas,
            mins,
            active,
            budget,
            primary_only,
            allowed,
            should_continue,
        )?;
        let result = Arc::new(CachedDpResult {
            combat: result.combat,
            additions: Arc::new(result.additions),
            ranks: Arc::new(result.ranks),
        });
        self.solved.insert(key, Arc::clone(&result));
        Ok(result)
    }
}

fn dp_cache_key(
    reuse: Option<&DpReuse>,
    search: &RelevantStatSearch,
    prepared_idx: usize,
    aow_idx: usize,
    upgrade: u8,
    stage: DpCacheStage,
) -> Option<DpCacheKey> {
    reuse
        .and_then(|reuse| reuse.max_search(search))
        .map(|_| DpCacheKey {
            shape: search.shape(),
            prepared_idx,
            aow_idx,
            upgrade,
            stage,
        })
}

#[allow(clippy::too_many_arguments)]
fn solve_exact_with_reuse<const N: usize>(
    reuse: Option<&mut DpReuse>,
    key: Option<DpCacheKey>,
    base: &[ExactRational; N],
    deltas: &[Vec<[ExactRational; N]>; COMBAT_STAT_COUNT],
    mins: [u8; COMBAT_STAT_COUNT],
    active: [bool; COMBAT_STAT_COUNT],
    budget: usize,
    primary_only: bool,
    allowed: Option<&[Vec<Vec<u8>>; COMBAT_STAT_COUNT]>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<DpSolve, String> {
    match (reuse, key) {
        (Some(reuse), Some(key)) => Ok(DpSolve::Shared(reuse.solve(
            key,
            base,
            deltas,
            mins,
            active,
            budget,
            primary_only,
            allowed,
            should_continue,
        )?)),
        _ => {
            let result = exact_dp::solve_exact(
                base,
                deltas,
                mins,
                active,
                budget,
                primary_only,
                allowed,
                should_continue,
            )?;
            Ok(DpSolve::Owned(result))
        }
    }
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
    fn poll(&mut self) -> Result<(), String>;
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

const PARALLEL_SEARCH_MIN_COMBINATIONS: u64 = 1_000_000;
const PARALLEL_AOW_CHUNK_SIZE: usize = 8;
const PARALLEL_PROGRESS_BATCH: u64 = 8_192;
const PROGRESS_POLL_BATCH: u32 = 1_024;
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
    for filter in &request.filters {
        if filter.dimension == FilterDimension::WeaponFamily
            && !data
                .weapons
                .iter()
                .any(|weapon| weapon.family_filter_id().eq_ignore_ascii_case(&filter.id))
        {
            return Err(format!(
                "Weapon family filter {} is no longer valid for this profile; remove it and reselect the weapon family",
                filter.id
            ));
        }
    }
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
    data: &GameData,
) -> Result<(), String> {
    validate_profile_capabilities(request, data)?;
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
        plan.fine_work_units = build_search_work_units(&plan, true)?;
        plan.serial_work_units = build_search_work_units(&plan, false)?;
    }
    Ok(plan)
}

pub fn optimize(request: &OptimizeRequest, data: &GameData) -> Result<Vec<OptimizeResult>, String> {
    if request.top_k == 0 {
        validate_profile_capabilities(request, data)?;
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
        validate_profile_capabilities(request, data)?;
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
        validate_profile_capabilities(request, data)?;
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
    validate_profile_capabilities(request, data)?;
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
    let max_level = *ordered_levels
        .last()
        .expect("non-empty levels must have a maximum");
    let mut max_request = request.clone();
    max_request.character_level = max_level;
    validate_profile_capabilities(&max_request, data)?;
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

    if !should_continue() {
        return Err("cancelled".to_string());
    }
    let max_constraints = build_combat_constraints(&max_request)?;
    let shared_weapons = Arc::from(
        prepare_weapons_with_cancel(&max_request, data, max_constraints, &mut should_continue)?
            .into_boxed_slice(),
    );
    let mut max_plan = Some(build_prepared_plan(
        &max_request,
        data,
        max_constraints,
        Arc::clone(&shared_weapons),
        &mut should_continue,
        true,
    )?);
    let mut dp_reuse = DpReuse::from_plan(max_plan.as_ref().expect("max plan exists"));
    let range_reuse_enabled = level_range_reuse_enabled(request);

    let mut results = Vec::with_capacity(ordered_levels.len());
    for level in ordered_levels {
        if !should_continue() {
            return Err("cancelled".to_string());
        }
        let mut level_request = request.clone();
        level_request.character_level = level;
        let plan = if level == max_level {
            max_plan.take().expect("max plan is consumed once")
        } else {
            let constraints = build_combat_constraints(&level_request)?;
            build_prepared_plan(
                &level_request,
                data,
                constraints,
                Arc::clone(&shared_weapons),
                &mut should_continue,
                true,
            )?
        };
        let rows = if range_reuse_enabled && level_plan_can_reuse(&plan, &dp_reuse) {
            optimize_prepared_with_reuse(
                &plan,
                1_024,
                |_snapshot| should_continue(),
                &mut dp_reuse,
            )?
        } else {
            optimize_prepared_with_progress(&plan, 1_024, |_snapshot| should_continue())?
        };
        results.push(LevelOptimizeResult { level, rows });
        if !level_complete(level) {
            return Err("cancelled".to_string());
        }
    }
    Ok(results)
}

fn level_range_reuse_enabled(request: &OptimizeRequest) -> bool {
    // ponytail: fixed-loadout cache stays bounded; broaden after measuring other range shapes.
    request.top_k == 1
        && request.exact_upgrade
        && request.weapon_name.is_some()
        && request.affinity.is_some()
        && request.aow_name.is_some()
        && matches!(
            request.objective,
            OptimizeObjective::MaxAr
                | OptimizeObjective::MaxPhysicalAr
                | OptimizeObjective::BleedThenAr
        )
}

fn level_plan_can_reuse(plan: &PreparedSearchPlan<'_>, reuse: &DpReuse) -> bool {
    !plan.groups.is_empty()
        && plan
            .groups
            .iter()
            .all(|group| reuse.max_search(&group.search).is_some())
}

pub fn optimize_prepared_with_progress<F>(
    plan: &PreparedSearchPlan<'_>,
    progress_every: u64,
    mut progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let started = Instant::now();
    let mut last_snapshot = ProgressSnapshot {
        checked: 0,
        total: 0,
        eligible: 0,
        best_score: 0.0,
        elapsed_ms: 0,
    };
    let candidates = score_prepared_with_progress(plan, progress_every, |snapshot| {
        last_snapshot = snapshot;
        progress_cb(snapshot)
    })?;
    // Hydration does not score more candidates. Keep the final counts while
    // allowing the same callback to cancel after the scoring workers join.
    materialize_scored_candidates_with_cancel(
        &plan.request,
        plan.data,
        &plan.weapons,
        candidates,
        &mut || {
            last_snapshot.elapsed_ms = started.elapsed().as_millis() as u64;
            progress_cb(last_snapshot)
        },
    )
}

fn optimize_prepared_with_reuse<F>(
    plan: &PreparedSearchPlan<'_>,
    progress_every: u64,
    progress_cb: F,
    reuse: &mut DpReuse,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let group_mode = result_group_mode(&plan.request);
    if plan.request.top_k == 0 || plan.weapons.is_empty() {
        return Ok(Vec::new());
    }
    let total = plan
        .serial_work_units
        .iter()
        .map(|unit| unit.candidate_count)
        .sum::<u64>();
    if total == 0 {
        return Ok(Vec::new());
    }
    let mut progress = SerialSearchProgress::new(total, progress_every, progress_cb);
    let candidates = score_serial(
        plan,
        &plan.serial_work_units,
        group_mode,
        &mut progress,
        Some(reuse),
    )?;
    materialize_scored_candidates_with_cancel(
        &plan.request,
        plan.data,
        &plan.weapons,
        candidates,
        &mut || progress.emit(true).is_ok(),
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
    let candidates = score_prepared_with_progress(&plan, 0, |_| true)?;
    let scoring = scoring_started.elapsed();

    let materialization_started = Instant::now();
    let rows = materialize_scored_candidates(&plan.request, plan.data, &plan.weapons, candidates)?;
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
) -> Result<Vec<ScoredCandidate>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let request = &plan.request;
    let group_mode = result_group_mode(request);
    if request.top_k == 0 {
        return Ok(Vec::new());
    }
    if plan.weapons.is_empty() {
        return Ok(Vec::new());
    }

    let fine_work_units = &plan.fine_work_units;
    let total = fine_work_units
        .iter()
        .map(|unit| unit.candidate_count)
        .sum::<u64>();
    if total == 0 {
        return Ok(Vec::new());
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
        score_serial(
            plan,
            &plan.serial_work_units,
            group_mode,
            &mut progress,
            None,
        )?
    };
    Ok(candidates)
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

fn build_search_work_units(
    plan: &PreparedSearchPlan<'_>,
    split_aows: bool,
) -> Result<Vec<SearchWorkUnit>, String> {
    let mut units = Vec::new();
    for (group_idx, group) in plan.groups.iter().enumerate() {
        let chunk_size = if matches!(
            plan.request.objective,
            OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
        ) {
            if split_aows {
                1
            } else {
                PARALLEL_AOW_CHUNK_SIZE
            }
        } else {
            group.aow_indices.len().max(1)
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
    if matches!(
        plan.request.objective,
        OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr
    ) {
        // Float estimates only schedule work; every pruning decision uses exact bounds.
        let estimates = plan
            .groups
            .iter()
            .map(|group| {
                let prepared = &plan.weapons[group.prepared_idx];
                let weapon = prepared.weapon;
                let upgrade = *prepared
                    .upgrades
                    .last()
                    .ok_or("weapon has no eligible upgrades")?;
                let mut best = 0.0_f32;
                for stat in 0..COMBAT_STAT_COUNT {
                    let mut combat = group.search.mins;
                    combat[stat] += u16::from(group.search.maxs[stat] - combat[stat])
                        .min(group.search.remaining_free) as u8;
                    let stats = stats_with_combat(plan.request.current_stats, combat);
                    let ar = crate::math::estimate_ar(
                        weapon,
                        upgrade,
                        &stats,
                        effective_str_for_weapon(&plan.request, weapon, stats.str),
                        plan.data,
                    )?;
                    let score =
                        if matches!(plan.request.objective, OptimizeObjective::MaxPhysicalAr) {
                            ar.physical
                        } else {
                            ar.total()
                        };
                    best = best.max(score);
                }
                Ok(best)
            })
            .collect::<Result<Vec<_>, String>>()?;
        units.sort_by(|left, right| {
            estimates[right.group_idx].total_cmp(&estimates[left.group_idx])
        });
    }
    Ok(units)
}

fn score_cutoff<'a>(
    plan: &PreparedSearchPlan<'_>,
    unit: SearchWorkUnit,
    candidates: &'a [ScoredCandidate],
    group_mode: ResultGroupMode,
) -> Option<&'a ExactRational> {
    let global = (candidates.len() == plan.request.top_k)
        .then(|| candidates.last().map(|candidate| &candidate.key.score))
        .flatten();
    let group = if matches!(group_mode, ResultGroupMode::WeaponOnly) {
        let weapon = plan.weapons[plan.groups[unit.group_idx].prepared_idx].weapon;
        candidates
            .iter()
            .find(|candidate| {
                plan.weapons[candidate.prepared_idx]
                    .weapon
                    .name
                    .eq_ignore_ascii_case(&weapon.name)
            })
            .map(|candidate| &candidate.key.score)
    } else {
        None
    };
    global.max(group)
}

fn score_serial<F>(
    plan: &PreparedSearchPlan<'_>,
    work_units: &[SearchWorkUnit],
    group_mode: ResultGroupMode,
    progress: &mut SerialSearchProgress<F>,
    mut reuse: Option<&mut DpReuse>,
) -> Result<Vec<ScoredCandidate>, String>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    let request = &plan.request;
    progress.emit_initial()?;
    let mut candidates = Vec::<ScoredCandidate>::with_capacity(request.top_k);
    for unit in work_units {
        let cutoff = score_cutoff(plan, *unit, &candidates, group_mode);
        let mut unit_results = search_dp_work_unit(
            plan,
            *unit,
            group_mode,
            progress,
            cutoff,
            reuse.as_deref_mut(),
        )?;
        merge_scored_top_k(
            &mut candidates,
            unit_results.drain(..),
            &plan.weapons,
            group_mode,
            request.top_k,
        );
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
        .try_fold(
            || Vec::<ScoredCandidate>::with_capacity(request.top_k),
            |mut candidates, unit| {
                let mut local_progress = ParallelLocalProgress::new(Arc::clone(&progress));
                let cutoff = score_cutoff(plan, *unit, &candidates, group_mode);
                let result =
                    search_dp_work_unit(plan, *unit, group_mode, &mut local_progress, cutoff, None);
                let finish_result = local_progress.finish();
                let results = match (result, finish_result) {
                    (Ok(results), Ok(())) => results,
                    (Err(error), _) | (_, Err(error)) => return Err(error),
                };
                merge_scored_top_k(
                    &mut candidates,
                    results,
                    &plan.weapons,
                    group_mode,
                    request.top_k,
                );
                Ok(candidates)
            },
        )
        .collect::<Result<Vec<_>, String>>()?;

    let mut candidates = Vec::with_capacity(request.top_k);
    for unit_results in partial_results {
        merge_scored_top_k(
            &mut candidates,
            unit_results,
            &plan.weapons,
            group_mode,
            request.top_k,
        );
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
    let candidates = score_serial(plan, work_units, group_mode, progress, None)?;
    materialize_scored_candidates(&plan.request, plan.data, &plan.weapons, candidates)
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
    materialize_scored_candidates(&plan.request, plan.data, &plan.weapons, candidates)
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
            for aow_idx in aow_indices {
                let candidate = match exact_candidate(
                    request,
                    plan.data,
                    &plan.weapons,
                    group.prepared_idx,
                    *aow_idx,
                    *upgrade,
                    stats,
                ) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        visit_result = Err(error);
                        return false;
                    }
                };
                if let Err(error) = progress.advance(1, 1, Some(candidate.metric.score)) {
                    visit_result = Err(error);
                    return false;
                }
                if matches!(
                    request.objective,
                    OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
                ) && candidate.key.score <= ExactRational::zero()
                {
                    continue;
                }
                if !could_enter_scored_top_k(&candidates, &candidate, request.top_k) {
                    continue;
                }
                push_scored_top_k(
                    &mut candidates,
                    candidate,
                    &plan.weapons,
                    group_mode,
                    request.top_k,
                );
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
    cutoff: Option<&ExactRational>,
    mut reuse: Option<&mut DpReuse>,
) -> Result<Vec<ScoredCandidate>, String>
where
    P: SearchProgress,
{
    let request = &plan.request;
    let group = &plan.groups[unit.group_idx];
    let prepared = &plan.weapons[group.prepared_idx];
    let aow_indices = &group.aow_indices[unit.aow_start..unit.aow_end];
    let reused_search = reuse
        .as_deref()
        .and_then(|reuse| reuse.max_search(&group.search));
    let dp_search = reused_search.as_ref().unwrap_or(&group.search);
    let mut candidates = Vec::with_capacity(request.top_k.min(aow_indices.len()));
    let mut route_sets = Vec::with_capacity(aow_indices.len());
    for &aow_idx in aow_indices {
        progress.poll()?;
        let Some(routes) = scalar_route_set(&prepared.aow_choices[aow_idx], plan.data)? else {
            return search_work_unit_exhaustive(plan, unit, group_mode, progress);
        };
        route_sets.push((aow_idx, routes));
    }

    if matches!(group_mode, ResultGroupMode::WeaponOnly)
        && request.damage_multiplier() > 0.0
        && matches!(
            request.objective,
            OptimizeObjective::MaxAr
                | OptimizeObjective::MaxPhysicalAr
                | OptimizeObjective::BleedThenAr
        )
    {
        // With the same bleed effect, flat buffs order the shared primary pair independently of stats.
        let mut best_buffs = HashMap::new();
        let mut keys = Vec::with_capacity(route_sets.len());
        for (index, _) in &route_sets {
            let choice = &prepared.aow_choices[*index];
            let bleed_effect: [u32; 4] =
                if matches!(request.objective, OptimizeObjective::BleedThenAr) {
                    primary_effect_key(choice)[5..]
                        .try_into()
                        .expect("four bleed effect fields")
                } else {
                    [0; 4]
                };
            let buff = choice.aow.map_or([0.0; 5], |ash| ash.buff_attack_power);
            let components = buff
                .map(rational)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            let total = components
                .iter()
                .filter(|value| !value.is_zero())
                .cloned()
                .sum::<ExactRational>();
            let score = match request.objective {
                OptimizeObjective::MaxPhysicalAr => components[0].clone(),
                OptimizeObjective::BleedThenAr => ExactRational::zero(),
                _ => total.clone(),
            };
            let key = (score, total);
            best_buffs
                .entry(bleed_effect)
                .and_modify(|best: &mut (ExactRational, ExactRational)| {
                    if key > *best {
                        *best = key.clone();
                    }
                })
                .or_insert_with(|| key.clone());
            keys.push((bleed_effect, key));
        }
        let mut keys = keys.iter();
        let original_count = route_sets.len();
        route_sets.retain(|_| {
            let (effect, key) = keys.next().expect("one key per route set");
            best_buffs.get(effect) == Some(key)
        });
        let skipped = group
            .search
            .candidate_count
            .saturating_mul((original_count - route_sets.len()) as u64)
            .saturating_mul(prepared.upgrades.len() as u64);
        progress.advance(skipped, skipped, None)?;
    }

    let share_primary = group.search.remaining_free > 0
        && matches!(
            request.objective,
            OptimizeObjective::MaxAr
                | OptimizeObjective::MaxPhysicalAr
                | OptimizeObjective::BleedThenAr
        );
    for &upgrade in &prepared.upgrades {
        if let Some(cutoff) = cutoff
            && matches!(
                request.objective,
                OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr
            )
        {
            let mut mins = group.search.mins.map(u16::from);
            let maxima = formula_max_stats(&group.search, request, prepared.weapon);
            mins[0] = effective_str_for_weapon(request, prepared.weapon, group.search.mins[0]);
            let upper = crate::math::exact::exact_ar_upper_bound(
                prepared.weapon,
                upgrade,
                plan.data,
                mins,
                maxima,
            )?;
            let physical_only = matches!(request.objective, OptimizeObjective::MaxPhysicalAr);
            let base_upper = if physical_only {
                upper[0].clone()
            } else {
                sum_components(&upper)
            };
            let mut buff_upper = ExactRational::zero();
            for &aow_idx in aow_indices {
                if let Some(ash) = prepared.aow_choices[aow_idx].aow {
                    let buff = if physical_only {
                        rational(ash.buff_attack_power[0])?
                    } else {
                        ash.buff_attack_power
                            .into_iter()
                            .map(rational)
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .sum()
                    };
                    buff_upper = buff_upper.max(buff);
                }
            }
            if (base_upper + buff_upper) * rational(request.damage_multiplier())? < *cutoff {
                let count = group
                    .search
                    .candidate_count
                    .saturating_mul(route_sets.len() as u64);
                progress.advance(count, count, None)?;
                continue;
            }
        }
        let mut primary_plans = HashMap::new();
        if share_primary {
            for &(aow_idx, _) in &route_sets {
                let choice = &prepared.aow_choices[aow_idx];
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    primary_plans.entry(primary_effect_key(choice))
                {
                    entry.insert(prepare_primary_allocations(
                        &group.search,
                        dp_search,
                        request,
                        prepared,
                        choice,
                        upgrade,
                        plan.data,
                        progress,
                        group.prepared_idx,
                        aow_idx,
                        reuse.as_deref_mut(),
                    )?);
                }
            }
        }
        let best_primary_key = if matches!(group_mode, ResultGroupMode::WeaponOnly) {
            primary_plans
                .values()
                .filter_map(|primary| primary.best_primary.as_ref())
                .map(|value| (&value.key.score, &value.key.ar_total))
                .max()
        } else {
            None
        };
        for &(aow_idx, ref route_set) in &route_sets {
            progress.poll()?;
            let aow_choice = &prepared.aow_choices[aow_idx];
            let routes = route_set.as_ref();
            let primary = primary_plans.get(&primary_effect_key(aow_choice));
            if let (Some(cutoff), Some(primary)) =
                (cutoff, primary.and_then(|p| p.best_primary.as_ref()))
                && primary.key.score < *cutoff
            {
                progress.advance(
                    group.search.candidate_count,
                    group.search.candidate_count,
                    None,
                )?;
                continue;
            }
            if let (Some(bound), Some(primary)) = (
                best_primary_key,
                primary.and_then(|p| p.best_primary.as_ref()),
            ) && (&primary.key.score, &primary.key.ar_total) < bound
            {
                progress.advance(
                    group.search.candidate_count,
                    group.search.candidate_count,
                    None,
                )?;
                continue;
            }
            let best = if routes.is_empty() {
                best_objective_allocation(
                    &group.search,
                    dp_search,
                    request,
                    prepared,
                    aow_choice,
                    upgrade,
                    None,
                    plan.data,
                    progress,
                    primary,
                    reuse.as_deref_mut(),
                    group.prepared_idx,
                    aow_idx,
                )?
            } else {
                let mut best = None;
                for route in routes {
                    let candidate = best_objective_allocation(
                        &group.search,
                        dp_search,
                        request,
                        prepared,
                        aow_choice,
                        upgrade,
                        Some(route),
                        plan.data,
                        progress,
                        primary,
                        reuse.as_deref_mut(),
                        group.prepared_idx,
                        aow_idx,
                    )?;
                    if best
                        .as_ref()
                        .is_none_or(|current| better_objective_allocation(&candidate, current))
                    {
                        best = Some(candidate);
                    }
                }
                best.ok_or_else(|| "AoW route optimizer found no feasible allocation".to_string())?
            };
            let metric = metric_from_allocation(&best)?;
            if matches!(request.objective, OptimizeObjective::AowFirstHit)
                && best.key.aow_first <= ExactRational::zero()
                || matches!(request.objective, OptimizeObjective::AowFullSequence)
                    && best.key.aow_full <= ExactRational::zero()
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
                key: best.key,
                route_id: best.route_id,
            };
            if could_enter_scored_top_k(&candidates, &candidate, request.top_k) {
                push_scored_top_k(
                    &mut candidates,
                    candidate,
                    &plan.weapons,
                    group_mode,
                    request.top_k,
                );
            }
        }
    }
    progress.finish()?;
    Ok(candidates)
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct ObjectiveKey {
    score: ExactRational,
    ar_total: ExactRational,
    aow_full: ExactRational,
    aow_first: ExactRational,
    bleed: ExactRational,
}

impl ObjectiveKey {
    fn components(&self) -> [ExactRational; 5] {
        [
            self.score.clone(),
            self.ar_total.clone(),
            self.aow_full.clone(),
            self.aow_first.clone(),
            self.bleed.clone(),
        ]
    }
}

#[derive(Clone, Debug)]
struct ObjectiveAllocation {
    key: ObjectiveKey,
    ar: DamageBreakdown,
    combat: [u8; COMBAT_STAT_COUNT],
    route_id: Option<String>,
}

#[derive(Debug)]
struct PrimaryPreparation {
    ar_formula: crate::math::exact::ExactArFormula,
    base: ObjectiveAllocation,
    values: Arc<[Vec<ObjectiveKey>; COMBAT_STAT_COUNT]>,
}

#[derive(Debug)]
struct PrimaryAllocationPlan {
    base: ObjectiveAllocation,
    values: Arc<[Vec<ObjectiveKey>; COMBAT_STAT_COUNT]>,
    additions: Arc<DpAdditions>,
    best_primary: Option<ObjectiveAllocation>,
    exact_unique_combat: Option<[u8; COMBAT_STAT_COUNT]>,
}

fn formula_max_stats(
    search: &RelevantStatSearch,
    request: &OptimizeRequest,
    weapon: &Weapon,
) -> [u16; 5] {
    let mut values = std::array::from_fn(|stat| {
        u16::from(search.mins[stat])
            + u16::from(search.maxs[stat] - search.mins[stat]).min(search.remaining_free)
    });
    values[0] = values[0].max(effective_str_for_weapon(request, weapon, values[0] as u8));
    values
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
    dp_search: &RelevantStatSearch,
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    choice: &AowChoice<'_>,
    upgrade: u8,
    data: &GameData,
    progress: &mut P,
    prepared_idx: usize,
    aow_idx: usize,
    mut reuse: Option<&mut DpReuse>,
) -> Result<PrimaryAllocationPlan, String> {
    progress.poll()?;
    let key = dp_cache_key(
        reuse.as_deref(),
        search,
        prepared_idx,
        aow_idx,
        upgrade,
        DpCacheStage::Primary,
    );
    let cached = key.as_ref().and_then(|key| {
        let reuse = reuse.as_deref()?;
        Some((
            reuse.primary.get(key)?.clone(),
            DpSolve::Shared(reuse.solved.get(key)?.clone()),
        ))
    });
    let (preparation, solved) = if let Some(cached) = cached {
        cached
    } else {
        let ar_formula = crate::math::exact::compile_ar_formula(
            prepared.weapon,
            upgrade,
            data,
            formula_max_stats(dp_search, request, prepared.weapon),
        )?;
        let evaluate = |combat| {
            evaluate_allocation_with_formulas(
                combat,
                request,
                prepared,
                choice,
                upgrade,
                None,
                data,
                Some(&ar_formula),
                None,
            )
        };
        let base = evaluate(search.mins)?;
        let mut values: [Vec<ObjectiveKey>; COMBAT_STAT_COUNT] =
            std::array::from_fn(|_| Vec::new());
        let budget = usize::from(dp_search.max_active_spend());
        let mut deltas = std::array::from_fn(|_| Vec::new());
        for stat in 0..COMBAT_STAT_COUNT {
            if !dp_search.active[stat] {
                continue;
            }
            let cap = usize::from(dp_search.maxs[stat] - dp_search.mins[stat]).min(budget);
            deltas[stat].reserve(cap + 1);
            values[stat].reserve(cap + 1);
            for add in 0..=cap {
                progress.poll()?;
                let mut combat = search.mins;
                combat[stat] += add as u8;
                let delta = primary_delta(
                    stat,
                    combat,
                    &base,
                    request,
                    prepared,
                    choice,
                    upgrade,
                    data,
                    &ar_formula,
                )?;
                deltas[stat].push([delta.score.clone(), delta.ar_total.clone()]);
                values[stat].push(delta);
            }
        }
        let solved = solve_exact_with_reuse(
            reuse.as_deref_mut(),
            key.clone(),
            &std::array::from_fn(|_| ExactRational::zero()),
            &deltas,
            search.mins,
            search.active,
            budget,
            true,
            None,
            &mut || progress.poll().is_ok(),
        )?;
        let preparation = Arc::new(PrimaryPreparation {
            ar_formula,
            base,
            values: Arc::new(values),
        });
        if let (Some(reuse), Some(key)) = (reuse, key) {
            reuse.primary.insert(key, preparation.clone());
        }
        (preparation, solved)
    };
    let (_, std::cmp::Reverse(best_combat)) = (search.min_active_spend()
        ..=search.max_active_spend())
        .filter_map(|spent| {
            let rank = solved.ranks()[usize::from(spent)]?;
            let mut combat = solved.combat()[usize::from(spent)]?;
            fill_inactive_stats(search, &mut combat, search.remaining_free - spent);
            Some((rank, std::cmp::Reverse(combat)))
        })
        .max()
        .ok_or_else(|| "stat optimizer could not satisfy the stat budget".to_string())?;
    let best_primary = Some(evaluate_allocation_with_formulas(
        best_combat,
        request,
        prepared,
        choice,
        upgrade,
        None,
        data,
        Some(&preparation.ar_formula),
        None,
    )?);
    let (additions, ranks) = solved.into_shared_parts();
    let exact_unique_combat =
        exact_unique_primary_allocation(search, progress, &ranks, &additions)?;
    let (base, values) = match Arc::try_unwrap(preparation) {
        Ok(preparation) => (preparation.base, preparation.values),
        Err(preparation) => (preparation.base.clone(), preparation.values.clone()),
    };
    Ok(PrimaryAllocationPlan {
        base,
        values,
        additions,
        best_primary,
        exact_unique_combat,
    })
}

#[allow(clippy::too_many_arguments)]
fn exact_unique_primary_allocation<P: SearchProgress>(
    search: &RelevantStatSearch,
    progress: &mut P,
    ranks: &[Option<usize>],
    additions: &[Vec<Vec<u8>>; COMBAT_STAT_COUNT],
) -> Result<Option<[u8; COMBAT_STAT_COUNT]>, String> {
    if search.remaining_free == 0 {
        return Ok(None);
    }
    let active_stats = (0..COMBAT_STAT_COUNT)
        .filter(|&stat_idx| search.active[stat_idx])
        .collect::<Vec<_>>();

    let mut best = None;
    for spent in search.min_active_spend()..=search.max_active_spend() {
        progress.poll()?;
        let Some(rank) = ranks[usize::from(spent)] else {
            continue;
        };
        match best {
            None => best = Some((rank, spent, false)),
            Some((best_rank, _, _)) if rank > best_rank => best = Some((rank, spent, false)),
            Some((best_rank, best_spent, _)) if rank == best_rank => {
                best = Some((best_rank, best_spent, true))
            }
            _ => {}
        }
    }
    let Some((_, spent, false)) = best else {
        return Ok(None);
    };
    let Some(mut combat) = unique_primary_combat(search, additions, &active_stats, spent) else {
        return Ok(None);
    };
    fill_inactive_stats(search, &mut combat, search.remaining_free - spent);
    Ok(Some(combat))
}

fn unique_primary_combat(
    search: &RelevantStatSearch,
    additions: &[Vec<Vec<u8>>; COMBAT_STAT_COUNT],
    active_stats: &[usize],
    spent: u16,
) -> Option<[u8; COMBAT_STAT_COUNT]> {
    let mut combat = search.mins;
    let mut destination = usize::from(spent);
    for &stat_idx in active_stats.iter().rev() {
        let additions = &additions[stat_idx][destination];
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
    dp_search: &RelevantStatSearch,
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    route: Option<&ScalarAowRoute<'_>>,
    data: &GameData,
    progress: &mut P,
    primary: Option<&PrimaryAllocationPlan>,
    reuse: Option<&mut DpReuse>,
    prepared_idx: usize,
    aow_idx: usize,
) -> Result<ObjectiveAllocation, String>
where
    P: SearchProgress,
{
    progress.poll()?;
    if search.remaining_free == 0 {
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
    if route.is_none()
        && match request.objective {
            OptimizeObjective::BleedThenAr => true,
            OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr => {
                !stat_can_increase_bleed_for_choice(prepared, aow_choice, data, STAT_ARC)
            }
            _ => false,
        }
        && let Some(best) = primary.and_then(|plan| plan.best_primary.as_ref())
    {
        // Without a route, the remaining metrics are zero, constant, or repeat bleed.
        return Ok(best.clone());
    }
    if let Some(combat) = primary.and_then(|plan| plan.exact_unique_combat) {
        let mut value = primary
            .and_then(|plan| plan.best_primary.clone())
            .expect("unique primary winner");
        value.combat = combat;
        if let Some(route) = route {
            let stats = stats_with_combat(request.current_stats, combat);
            let (first, full) = crate::math::exact::exact_scalar_route(
                route,
                prepared.weapon,
                upgrade,
                &stats,
                effective_str_for_weapon(request, prepared.weapon, stats.str),
                request.damage_multiplier(),
                data,
            )?;
            value.key.aow_first = first;
            value.key.aow_full = full;
            value.route_id = Some(route.route_id.clone());
        }
        return Ok(value);
    }
    if route.is_some_and(|route| !route.is_additive(data)) {
        // Penalty corrections couple stats; additive DP cannot prune them safely.
        let mut best = None;
        let mut error = None;
        let mut combat = search.mins;
        search.visit(&mut combat, |combat| {
            let candidate = progress.poll().and_then(|()| {
                evaluate_objective_allocation(
                    *combat, request, prepared, aow_choice, upgrade, route, data,
                )
            });
            match candidate {
                Ok(candidate) => {
                    if best
                        .as_ref()
                        .is_none_or(|current| better_objective_allocation(&candidate, current))
                    {
                        best = Some(candidate);
                    }
                    true
                }
                Err(failure) => {
                    error = Some(failure);
                    false
                }
            }
        });
        if let Some(error) = error {
            return Err(error);
        }
        return best.ok_or_else(|| "no feasible skill allocation".to_string());
    }
    let maxima = formula_max_stats(dp_search, request, prepared.weapon);
    let route_formula = route
        .map(|route| {
            crate::math::exact::compile_scalar_route_formula(
                route,
                prepared.weapon,
                upgrade,
                request.damage_multiplier(),
                data,
                maxima,
            )
        })
        .transpose()?;
    if matches!(
        request.objective,
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
    ) && let Some(formula) = &route_formula
        && let Some(stat) =
            formula.sole_scaling_stat(request.objective == OptimizeObjective::AowFirstHit)
        && search.active[stat]
        && search.remaining_free <= u16::from(search.maxs[stat] - search.mins[stat])
    {
        let mut combat = search.mins;
        combat[stat] += search.remaining_free as u8;
        let old_stats = stats_with_combat(request.current_stats, search.mins);
        let old_str = effective_str_for_weapon(request, prepared.weapon, old_stats.str);
        let primary_value = |combat| -> Result<ExactRational, String> {
            let stats = stats_with_combat(request.current_stats, combat);
            let (first, full) = formula.unscaled_delta(
                stat,
                &old_stats,
                &stats,
                old_str,
                effective_str_for_weapon(request, prepared.weapon, stats.str),
            )?;
            Ok(if request.objective == OptimizeObjective::AowFirstHit {
                first
            } else {
                full
            })
        };
        let maximum = primary_value(combat)?;
        let mut unique = true;
        for add in 0..search.remaining_free {
            progress.poll()?;
            let mut other = search.mins;
            other[stat] += add as u8;
            if primary_value(other)? >= maximum {
                unique = false;
                break;
            }
        }
        if unique {
            return evaluate_objective_allocation(
                combat, request, prepared, aow_choice, upgrade, route, data,
            );
        }
    }
    let mut route_primary = None;
    if matches!(
        request.objective,
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
    ) && let Some(formula) = &route_formula
        && formula.has_scaling(request.objective == OptimizeObjective::AowFirstHit)
    {
        let old_stats = stats_with_combat(request.current_stats, search.mins);
        let old_str = effective_str_for_weapon(request, prepared.weapon, old_stats.str);
        let budget = usize::from(dp_search.max_active_spend());
        let mut primary_deltas = std::array::from_fn(|_| Vec::new());
        for stat in 0..COMBAT_STAT_COUNT {
            if !dp_search.active[stat] {
                continue;
            }
            let cap = usize::from(dp_search.maxs[stat] - dp_search.mins[stat]).min(budget);
            primary_deltas[stat].reserve(cap + 1);
            for add in 0..=cap {
                progress.poll()?;
                let mut combat = search.mins;
                combat[stat] += add as u8;
                let stats = stats_with_combat(request.current_stats, combat);
                let (first, full) = formula.unscaled_delta(
                    stat,
                    &old_stats,
                    &stats,
                    old_str,
                    effective_str_for_weapon(request, prepared.weapon, stats.str),
                )?;
                let score = if request.objective == OptimizeObjective::AowFirstHit {
                    first
                } else {
                    full
                };
                primary_deltas[stat].push([score]);
            }
        }
        let solved = solve_exact_with_reuse(
            None,
            None,
            &std::array::from_fn(|_| ExactRational::zero()),
            &primary_deltas,
            search.mins,
            search.active,
            budget,
            true,
            None,
            &mut || progress.poll().is_ok(),
        )?;
        if let Some(combat) =
            exact_unique_primary_allocation(search, progress, solved.ranks(), solved.additions())?
        {
            return evaluate_objective_allocation(
                combat, request, prepared, aow_choice, upgrade, route, data,
            );
        }
        route_primary = Some(solved);
    }
    let ar_formula = if primary.is_none() {
        Some(crate::math::exact::compile_ar_formula(
            prepared.weapon,
            upgrade,
            data,
            maxima,
        )?)
    } else {
        None
    };
    let evaluate = |combat| {
        evaluate_allocation_with_formulas(
            combat,
            request,
            prepared,
            aow_choice,
            upgrade,
            route,
            data,
            ar_formula.as_ref(),
            route_formula.as_ref(),
        )
    };
    let with_route = |mut value: ObjectiveAllocation| -> Result<ObjectiveAllocation, String> {
        if let Some(route) = route {
            let stats = stats_with_combat(request.current_stats, value.combat);
            let (first, full) = route_formula.as_ref().expect("compiled route").evaluate(
                &stats,
                effective_str_for_weapon(request, prepared.weapon, stats.str),
            )?;
            value.key.aow_full = full;
            value.key.aow_first = first;
            value.route_id = Some(route.route_id.clone());
        }
        Ok(value)
    };
    let base = match primary {
        Some(plan) => with_route(plan.base.clone())?,
        None => evaluate(search.mins)?,
    };
    let budget = usize::from(dp_search.max_active_spend());
    let mut deltas = std::array::from_fn(|_| Vec::new());
    for stat in 0..COMBAT_STAT_COUNT {
        if !dp_search.active[stat] {
            continue;
        }
        let cap = usize::from(dp_search.maxs[stat] - dp_search.mins[stat]).min(budget);
        deltas[stat].reserve(cap + 1);
        for add in 0..=cap {
            progress.poll()?;
            let mut combat = search.mins;
            combat[stat] += add as u8;
            let mut delta = match primary {
                Some(plan) => plan.values[stat][add].clone(),
                None => primary_delta(
                    stat,
                    combat,
                    &base,
                    request,
                    prepared,
                    aow_choice,
                    upgrade,
                    data,
                    ar_formula.as_ref().expect("compiled AR"),
                )?,
            };
            if let Some(formula) = &route_formula {
                let old_stats = stats_with_combat(request.current_stats, search.mins);
                let new_stats = stats_with_combat(request.current_stats, combat);
                let (first, full) = formula.unscaled_delta(
                    stat,
                    &old_stats,
                    &new_stats,
                    effective_str_for_weapon(request, prepared.weapon, old_stats.str),
                    effective_str_for_weapon(request, prepared.weapon, new_stats.str),
                )?;
                delta.aow_first = first;
                delta.aow_full = full;
                match request.objective {
                    OptimizeObjective::AowFirstHit => delta.score = delta.aow_first.clone(),
                    OptimizeObjective::AowFullSequence => delta.score = delta.aow_full.clone(),
                    _ => {}
                }
            }
            deltas[stat].push(delta.components());
        }
    }
    let key = dp_cache_key(
        reuse.as_deref(),
        search,
        prepared_idx,
        aow_idx,
        upgrade,
        DpCacheStage::Final {
            route_id: route.map(|route| route.route_id.clone()),
            allowed: primary.is_some() || route_primary.is_some(),
        },
    );
    let solved = solve_exact_with_reuse(
        reuse,
        key,
        &std::array::from_fn(|_| ExactRational::zero()),
        &deltas,
        search.mins,
        search.active,
        budget,
        false,
        primary
            .map(|plan| plan.additions.as_ref())
            .or_else(|| route_primary.as_ref().map(|plan| plan.additions())),
        &mut || progress.poll().is_ok(),
    )?;
    let mut best = None;
    for spent in search.min_active_spend()..=search.max_active_spend() {
        progress.poll()?;
        let Some(mut combat) = solved.combat()[usize::from(spent)] else {
            continue;
        };
        fill_inactive_stats(search, &mut combat, search.remaining_free - spent);
        let rank = solved.ranks()[usize::from(spent)].expect("reachable DP state has a rank");
        if best.as_ref().is_none_or(|&(current_rank, current_combat)| {
            rank > current_rank || rank == current_rank && combat < current_combat
        }) {
            best = Some((rank, combat));
        }
    }
    let (_, combat) =
        best.ok_or_else(|| "stat optimizer could not satisfy the stat budget".to_string())?;
    evaluate(combat)
}

#[allow(clippy::too_many_arguments)]
fn primary_delta(
    stat: usize,
    combat: [u8; 5],
    base: &ObjectiveAllocation,
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    choice: &AowChoice<'_>,
    upgrade: u8,
    data: &GameData,
    formula: &crate::math::exact::ExactArFormula,
) -> Result<ObjectiveKey, String> {
    let old_stats = stats_with_combat(request.current_stats, base.combat);
    let new_stats = stats_with_combat(request.current_stats, combat);
    let (ar_total, physical) = formula.primary_delta(
        stat,
        &old_stats,
        &new_stats,
        effective_str_for_weapon(request, prepared.weapon, old_stats.str),
        effective_str_for_weapon(request, prepared.weapon, new_stats.str),
        prepared.weapon.disable_two_hand_bonus,
        request.objective == OptimizeObjective::MaxPhysicalAr,
    )?;
    let bleed = if stat == STAT_ARC {
        crate::math::exact::exact_bleed(prepared.weapon, upgrade, &new_stats, data, choice.aow)?
            - &base.key.bleed
    } else {
        ExactRational::zero()
    };
    let score = match request.objective {
        OptimizeObjective::MaxAr => ar_total.clone(),
        OptimizeObjective::MaxPhysicalAr => physical,
        OptimizeObjective::BleedThenAr => bleed.clone(),
        _ => ExactRational::zero(),
    };
    Ok(ObjectiveKey {
        score,
        ar_total,
        bleed,
        ..ObjectiveKey::default()
    })
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
    evaluate_allocation_with_formulas(
        combat, request, prepared, aow_choice, upgrade, route, data, None, None,
    )
}

#[allow(clippy::too_many_arguments)]
fn evaluate_allocation_with_formulas(
    combat: [u8; COMBAT_STAT_COUNT],
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    route: Option<&ScalarAowRoute<'_>>,
    data: &GameData,
    ar_formula: Option<&crate::math::exact::ExactArFormula>,
    route_formula: Option<&crate::math::exact::ExactScalarRouteFormula>,
) -> Result<ObjectiveAllocation, String> {
    let stats = stats_with_combat(request.current_stats, combat);
    let effective_str_value = effective_str_for_weapon(request, prepared.weapon, stats.str);
    let mut components = match ar_formula {
        Some(formula) => formula.evaluate(
            &stats,
            effective_str_value,
            prepared.weapon.disable_two_hand_bonus,
        )?,
        None => crate::math::exact::exact_ar(
            prepared.weapon,
            upgrade,
            &stats,
            effective_str_value,
            data,
        )?,
    };
    let multiplier = rational(request.damage_multiplier())?;
    for (index, value) in components.iter_mut().enumerate() {
        if let Some(ash) = aow_choice.aow
            && ash.buff_attack_power[index] != 0.0
        {
            *value += rational(ash.buff_attack_power[index])?;
        }
        if !multiplier.is_one() {
            *value *= &multiplier;
        }
    }
    let ar_total = sum_components(&components);
    let bleed =
        crate::math::exact::exact_bleed(prepared.weapon, upgrade, &stats, data, aow_choice.aow)?;
    let (first, full) = match (route_formula, route) {
        (Some(formula), _) => formula.evaluate(&stats, effective_str_value)?,
        (None, Some(route)) => crate::math::exact::exact_scalar_route(
            route,
            prepared.weapon,
            upgrade,
            &stats,
            effective_str_value,
            request.damage_multiplier(),
            data,
        )?,
        (None, None) => (ExactRational::zero(), ExactRational::zero()),
    };
    let score = match request.objective {
        OptimizeObjective::MaxAr => ar_total.clone(),
        OptimizeObjective::MaxPhysicalAr => components[0].clone(),
        OptimizeObjective::BleedThenAr => bleed.clone(),
        OptimizeObjective::AowFirstHit => first.clone(),
        OptimizeObjective::AowFullSequence => full.clone(),
    };
    Ok(ObjectiveAllocation {
        key: ObjectiveKey {
            score,
            ar_total,
            aow_full: full,
            aow_first: first,
            bleed,
        },
        ar: project_damage(&components)?,
        combat,
        route_id: route.map(|route| route.route_id.clone()),
    })
}

fn rational(value: f32) -> Result<ExactRational, String> {
    crate::math::exact::rational(value, "model coefficient")
}

fn minimum_ar_loss_bps(ar_loss: &ExactRational, max_ar: &ExactRational) -> Result<u16, String> {
    if max_ar.is_zero() {
        return Ok(0);
    }
    let scaled = (ar_loss * ExactRational::from_integer(BigInt::from(10_000))) / max_ar;
    let scaled = scaled.to_big();
    let (quotient, remainder) = scaled.numer().div_rem(scaled.denom());
    let bps = quotient
        + if remainder.is_zero() {
            BigInt::from(0)
        } else {
            BigInt::from(1)
        };
    bps.to_u16()
        .ok_or_else(|| "AR loss basis points exceed the supported range".to_string())
}

fn metric_from_allocation(value: &ObjectiveAllocation) -> Result<CandidateMetric, String> {
    Ok(CandidateMetric {
        score: project(&value.key.score)?,
        ar: value.ar,
        aow_first_hit_damage: project(&value.key.aow_first)?,
        aow_full_sequence_damage: project(&value.key.aow_full)?,
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

#[cfg(test)]
fn add_objective_key(left: &ObjectiveKey, right: &ObjectiveKey) -> ObjectiveKey {
    ObjectiveKey {
        score: &left.score + &right.score,
        ar_total: &left.ar_total + &right.ar_total,
        aow_full: &left.aow_full + &right.aow_full,
        aow_first: &left.aow_first + &right.aow_first,
        bleed: &left.bleed + &right.bleed,
    }
}

fn better_objective_allocation(
    candidate: &ObjectiveAllocation,
    current: &ObjectiveAllocation,
) -> bool {
    candidate.key > current.key || candidate.key == current.key && candidate.combat < current.combat
}

fn exact_candidate(
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    prepared_idx: usize,
    aow_idx: usize,
    upgrade: u8,
    stats: Stats,
) -> Result<ScoredCandidate, String> {
    let prepared = &weapons[prepared_idx];
    let choice = &prepared.aow_choices[aow_idx];
    let routes = scalar_route_set(choice, data)?
        .ok_or_else(|| "supported per-hit attack-power effect is not implemented".to_string())?;
    let base = evaluate_objective_allocation(
        stats.combat_array(),
        request,
        prepared,
        choice,
        upgrade,
        None,
        data,
    )?;
    let effective_str_value = effective_str_for_weapon(request, prepared.weapon, stats.str);
    let mut best = None;
    for route in routes.as_ref() {
        let mut candidate = base.clone();
        let (first, full) = crate::math::exact::exact_scalar_route(
            route,
            prepared.weapon,
            upgrade,
            &stats,
            effective_str_value,
            request.damage_multiplier(),
            data,
        )?;
        candidate.key.aow_first = first;
        candidate.key.aow_full = full;
        match request.objective {
            OptimizeObjective::AowFirstHit => candidate.key.score = candidate.key.aow_first.clone(),
            OptimizeObjective::AowFullSequence => {
                candidate.key.score = candidate.key.aow_full.clone()
            }
            _ => {}
        }
        candidate.route_id = Some(route.route_id.clone());
        if best
            .as_ref()
            .is_none_or(|current| better_objective_allocation(&candidate, current))
        {
            best = Some(candidate);
        }
    }
    let best = best.unwrap_or(base);
    Ok(ScoredCandidate {
        prepared_idx,
        aow_idx,
        upgrade,
        stats,
        metric: metric_from_allocation(&best)?,
        key: best.key,
        route_id: best.route_id,
    })
}

fn materialize_scored_candidates(
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    candidates: Vec<ScoredCandidate>,
) -> Result<Vec<OptimizeResult>, String> {
    materialize_scored_candidates_with_cancel(request, data, weapons, candidates, &mut || true)
}

fn materialize_scored_candidates_with_cancel(
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    candidates: Vec<ScoredCandidate>,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Vec<OptimizeResult>, String> {
    // Scoring has already grouped, bounded and ordered this batch. Hydration
    // preserves its exact keys and every final tie-break field.
    debug_assert!(candidates.len() <= request.top_k);
    let damage_multiplier = request.damage_multiplier();
    let mut results = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let result = materialize_scored_candidate(
            candidate,
            request,
            data,
            weapons,
            damage_multiplier,
            should_continue,
        )?;
        results.push(result);
    }
    if !should_continue() {
        return Err("cancelled".into());
    }
    Ok(results)
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
        for (aow_idx, _) in prepared.aow_choices.iter().enumerate() {
            if !should_continue() {
                return Err("cancelled".to_string());
            }
            let candidate = exact_candidate(
                request,
                data,
                weapons,
                prepared_idx,
                aow_idx,
                upgrade,
                stats,
            )?;
            if matches!(
                request.objective,
                OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
            ) && candidate.key.score <= ExactRational::zero()
            {
                continue;
            }
            push_scored_top_k(
                &mut results,
                candidate,
                weapons,
                ResultGroupMode::Loadout,
                1,
            );
        }
    }
    let result = results
        .pop()
        .map(|candidate| {
            materialize_scored_candidate(
                candidate,
                request,
                data,
                weapons,
                damage_multiplier,
                should_continue,
            )
        })
        .transpose()?;
    if !should_continue() {
        return Err("cancelled".into());
    }
    Ok(result)
}

fn materialize_scored_candidate(
    candidate: ScoredCandidate,
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    damage_multiplier: f32,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<OptimizeResult, String> {
    if !should_continue() {
        return Err("cancelled".into());
    }
    let prepared = &weapons[candidate.prepared_idx];
    let aow_choice = &prepared.aow_choices[candidate.aow_idx];
    let effective_str_value =
        effective_str_for_weapon(request, prepared.weapon, candidate.stats.str);
    let status_buildup = calculate_status_with_buffs(
        prepared,
        aow_choice,
        candidate.upgrade,
        &candidate.stats,
        data,
    )?;
    let CandidateMetric {
        score,
        ar,
        aow_first_hit_damage,
        aow_full_sequence_damage,
    } = candidate.metric;
    let aow_route = materialize_aow_route(
        candidate.route_id.as_deref(),
        prepared,
        aow_choice,
        candidate.upgrade,
        &candidate.stats,
        effective_str_value,
        damage_multiplier,
        data,
        should_continue,
    )?;
    if !should_continue() {
        return Err("cancelled".into());
    }
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
        ar,
        aow_id: aow_choice.skill_id,
        aow_name: aow_choice.skill_name.map(str::to_string),
        bleed_buildup: project(&candidate.key.bleed)?,
        bleed_buildup_add: aow_choice
            .aow
            .map(|aow| aow.bleed_buildup_add)
            .unwrap_or(0.0),
        frost_buildup: status_buildup.frost,
        poison_buildup: status_buildup.poison,
        scarlet_rot_buildup: status_buildup.scarlet_rot,
        sleep_buildup: status_buildup.sleep,
        madness_buildup: status_buildup.madness,
        death_buildup: status_buildup.death,
        aow_first_hit_damage,
        aow_full_sequence_damage,
        aow_route,
        score,
        exact_key: candidate.key,
    })
}

#[allow(clippy::too_many_arguments)]
fn materialize_aow_route(
    selected_route: Option<&str>,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    data: &GameData,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<Option<AowRouteResult>, String> {
    if !should_continue() {
        return Err("cancelled".into());
    }
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
    let Some(selected_route) = selected_route else {
        return Ok(None);
    };
    let routes = calculate_aow_routes_scaled_with_cancel(
        prepared.weapon,
        attack_rows,
        upgrade,
        stats,
        effective_str_value,
        damage_multiplier,
        data,
        Some(selected_route),
        should_continue,
    )?;
    let route = routes
        .into_iter()
        .find(|route| route.route_id == selected_route)
        .ok_or_else(|| {
            format!("selected exact route {selected_route} is missing during materialization")
        })?;
    Ok(Some(route))
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
    poll_count: u32,
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
            poll_count: 0,
        }
    }

    fn emit_initial(&mut self) -> Result<(), String> {
        self.emit(true)
    }

    fn emit_final(&mut self) -> Result<(), String> {
        self.emit(true)
    }

    fn emit_if_due(&mut self, ignore_count_threshold: bool) -> Result<(), String> {
        if self.progress_every == 0 {
            return Ok(());
        }
        if !ignore_count_threshold
            && self.checked.saturating_sub(self.last_emit.last_checked) < self.progress_every
        {
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
        self.emit_if_due(false)
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn poll(&mut self) -> Result<(), String> {
        if self.cancelled {
            return Err("cancelled".to_string());
        }
        if self.progress_every == 0 {
            return Ok(());
        }
        self.poll_count = self.poll_count.saturating_add(1);
        if self.poll_count < PROGRESS_POLL_BATCH {
            return Ok(());
        }
        self.poll_count = 0;
        self.emit_if_due(true)
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
        self.emit_if_due(force, false)
    }

    fn emit_initial(&self) -> Result<(), String> {
        self.emit_if_due(true, false)
    }

    fn emit_final(&self) -> Result<(), String> {
        self.emit_if_due(true, false)
    }

    fn emit_if_due(&self, force: bool, ignore_count_threshold: bool) -> Result<(), String> {
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
            if !ignore_count_threshold
                && checked.saturating_sub(emit_guard.last_checked) < self.progress_every
            {
                return Ok(());
            }
            if emit_guard.last_at.elapsed() < PROGRESS_MIN_INTERVAL {
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
    poll_count: u32,
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
            poll_count: 0,
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

    fn poll(&mut self) -> Result<(), String> {
        if self.shared.is_cancelled() {
            return Err("cancelled".to_string());
        }
        if self.shared.progress_every == 0 {
            return Ok(());
        }
        self.poll_count = self.poll_count.saturating_add(1);
        if self.poll_count < PROGRESS_POLL_BATCH {
            return Ok(());
        }
        self.poll_count = 0;
        self.flush(false)?;
        self.shared.emit_if_due(false, true)
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
        if !weapon_matches_request(weapon, request, data) || !data.weapon_ar_supported(weapon) {
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
    crate::math::apply_aow_bleed_buffs(
        crate::math::calculate_bleed_buildup(prepared.weapon, upgrade, stats, data)?,
        prepared.weapon,
        upgrade,
        stats,
        data,
        aow_choice.aow,
    )
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
    if request.filters.is_empty() {
        return true;
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
                    .chain(data.aow_effects(row.aow_id, 0))
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
    let aow = data.aows.iter().find(|aow| aow.aow_id == native_skill_id);
    let skill_name = weapon
        .native_skill_name
        .as_deref()
        .or_else(|| aow.map(|aow| aow.name.as_str()))
        .or_else(|| source_rows.first().map(|row| row.aow_name.as_str()));
    Some(AowChoice {
        no_applied_ash: weapon.affinity.eq_ignore_ascii_case("Standard"),
        aow,
        skill_id: Some(native_skill_id),
        skill_name,
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
    fn shape(&self) -> DpSearchShape {
        DpSearchShape {
            mins: self.mins,
            maxs: self.maxs,
            active: self.active,
        }
    }

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
    let budget = usize::from(remaining_free);
    let inactive_capacity: usize = (0..COMBAT_STAT_COUNT)
        .filter(|&stat| !active[stat])
        .map(|stat| usize::from(maxs[stat] - mins[stat]))
        .sum();
    let mut counts = vec![0u64; budget + 1];
    counts[0] = 1;
    for stat in 0..COMBAT_STAT_COUNT {
        if !active[stat] {
            continue;
        }
        let cap = usize::from(maxs[stat] - mins[stat]);
        let mut next = vec![0u64; budget + 1];
        let mut window = 0u64;
        for spent in 0..=budget {
            // Five u8 stat domains contain at most 256^5 distributions.
            window += counts[spent];
            if spent > cap {
                window -= counts[spent - cap - 1];
            }
            next[spent] = window;
        }
        counts = next;
    }
    counts[budget.saturating_sub(inactive_capacity)..]
        .iter()
        .sum()
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
    if row.is_lacking_fp || !row.is_damaging() || row.has_fixed_damage(&data.profile_id) {
        return false;
    }
    DamageType::ALL.iter().any(|damage_type| {
        let damage_idx = damage_type.as_index();
        let has_damage_base = weapon.base[damage_idx] > 0.0 && row.motion_values[damage_idx] > 0.0
            || row.uses_fixed_attack_base() && row.attack_base[damage_idx] > 0.0;
        if !has_damage_base {
            return false;
        }
        damage_type_stat_can_scale(
            weapon,
            data,
            stat_idx,
            *damage_type,
            row.overwrite_attack_element_correct_id,
        )
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
    DamageType::ALL.iter().any(|damage_type| {
        weapon.base[damage_type.as_index()] > 0.0
            && damage_type_stat_can_scale(weapon, data, stat_idx, *damage_type, None)
    })
}

fn damage_type_stat_can_scale(
    weapon: &Weapon,
    data: &GameData,
    stat_idx: usize,
    damage_type: DamageType,
    override_id: Option<usize>,
) -> bool {
    let id = override_id.unwrap_or(weapon.attack_element_correct_id);
    let damage_idx = damage_type.as_index();
    if let Some(ext) = data.attack_element_ext(id) {
        return ext.stat_scales(stat_idx, damage_idx)
            && ext
                .overwrite_rate(stat_idx, damage_idx)
                .unwrap_or(weapon.scaling[stat_idx] * ext.influence_rate(stat_idx, damage_idx))
                > 0.0;
    }
    override_id.is_none()
        && weapon.scaling[stat_idx] > 0.0
        && data
            .attack_element(id)
            .is_none_or(|aec| aec.stat_scales(stat_idx, damage_type))
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
