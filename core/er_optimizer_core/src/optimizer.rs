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
    class_by_name, compute_free_points, effective_str, has_unmodeled_attack_power,
    meets_requirements, prepare_scalar_aow_routes,
};
#[cfg(test)]
use crate::math::{calculate_aow_routes, evaluate_scalar_aow_route};
use crate::model::{
    Aow, AowAttackRow, AowRouteResult, COMBAT_STAT_COUNT, DamageBreakdown, DamageType, GameData,
    STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, StatusBuildup, StatusEffectSource,
    Weapon, normalize_weapon_type_display,
};

mod types;
pub use types::*;
mod ranking;
use ranking::*;
mod exact_dp;
mod preparation;
use preparation::*;
mod progress;
use progress::*;

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
    two_handing: bool,
) -> Result<Option<ScalarRouteSet<'routes, 'data>>, String> {
    match &choice.scalar_routes {
        Some(Ok(routes)) => Ok(routes
            .as_ref()
            .map(|routes| Cow::Borrowed(routes.as_slice()))),
        Some(Err(error)) => Err(error.clone()),
        None => prepare_scalar_aow_routes(&choice.attack_rows, data).map(|routes| {
            routes.map(|mut routes| {
                routes.retain(|route| route_matches_handling(&route.route_id, two_handing));
                Cow::Owned(routes)
            })
        }),
    }
}

fn route_matches_handling(route_id: &str, two_handing: bool) -> bool {
    // Extraction puts the optional handedness dimension first in canonical IDs.
    // Skills with their own fixed animation have no request-handling dimension.
    match route_id.split('_').next() {
        Some("1h") => !two_handing,
        Some("2h") => two_handing,
        _ => true,
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
        if !should_continue() {
            return Err("cancelled".into());
        }
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
            let mut best: Option<OptimizeResult> = None;
            for upgrade in upgrades {
                if let Some(row) = evaluate_fixed_loadout_upgrade(
                    request,
                    self.data,
                    &self.weapons,
                    upgrade,
                    stats,
                    &mut should_continue,
                )? && best.as_ref().is_none_or(|best| better_result(&row, best))
                {
                    best = Some(row);
                }
            }
            return Ok(best.into_iter().collect());
        }
        let plan = build_prepared_plan(
            request,
            self.data,
            constraints,
            Arc::clone(&self.weapons),
            &mut should_continue,
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
    let Some(routes) = scalar_route_set(
        choice,
        data,
        weapon_uses_two_handing(request, prepared.weapon),
    )?
    else {
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

const PARALLEL_SEARCH_MIN_COMBINATIONS: u64 = 1_000_000;
const PARALLEL_AOW_CHUNK_SIZE: usize = 8;

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
        prepare_weapons_with_cancel(request, data, Some(constraints), &mut should_continue)?
            .into_boxed_slice(),
    );
    build_prepared_plan(request, data, constraints, weapons, &mut should_continue)
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
    let (template, weapons) = prepare_reusable_weapons(request, data, true, &mut should_continue)?;
    Ok(PreparedLoadoutEvaluator {
        template,
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
    let (template, weapons) = prepare_reusable_weapons(request, data, false, &mut should_continue)?;
    Ok(PreparedUpgradeSeriesEvaluator {
        template,
        data,
        weapons,
    })
}

/// Prepares one weapon and affinity for every stat budget: stat floors and locks are
/// cleared so later requests can change them without repeating preparation. Loadout
/// evaluators use one exact upgrade; upgrade series need an upgrade range.
fn prepare_reusable_weapons<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
    exact_upgrade: bool,
    should_continue: &mut impl FnMut() -> bool,
) -> Result<(OptimizeRequest, Arc<[PreparedWeapon<'a>]>), String> {
    validate_profile_capabilities(request, data)?;
    let purpose = if exact_upgrade {
        "reusable loadout evaluation"
    } else {
        "upgrade series evaluation"
    };
    if request.weapon_name.is_none() || request.affinity.is_none() {
        return Err(format!("{purpose} requires a weapon and affinity"));
    }
    if request.exact_upgrade != exact_upgrade {
        let upgrades = if exact_upgrade {
            "exact upgrade levels"
        } else {
            "an upgrade range"
        };
        return Err(format!("{purpose} requires {upgrades}"));
    }
    if !should_continue() {
        return Err("cancelled".to_string());
    }
    let mut preparation_request = request.clone();
    preparation_request.min_combat_stats = [0; COMBAT_STAT_COUNT];
    preparation_request.locked_combat_stats = [None; COMBAT_STAT_COUNT];
    preparation_request.top_k = 1;
    build_combat_constraints(&preparation_request)?;
    let weapons = Arc::from(
        prepare_weapons_with_cancel(&preparation_request, data, None, should_continue)?
            .into_boxed_slice(),
    );
    Ok((preparation_request, weapons))
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
) -> Result<PreparedSearchPlan<'a>, String> {
    let mut groups: Vec<PreparedSearchGroup> = Vec::new();
    let mut stat_candidates = 0_u64;
    let mut combinations = 0_u64;

    // Each Ash choice's stat search depends only on its weapon, so weapons resolve in
    // parallel (with per-thread count memos) and groups are then built in weapon order.
    let mut searches: Vec<Vec<Option<RelevantStatSearch>>> = Vec::with_capacity(weapons.len());
    for chunk in weapons.chunks(prepare_weapon_chunk()) {
        if !should_continue() {
            return Err("cancelled".to_string());
        }
        searches.par_extend(chunk.par_iter().map_init(
            HashMap::new,
            |distribution_counts, prepared| {
                prepared
                    .aow_choices
                    .iter()
                    .map(|aow_choice| {
                        relevant_stat_search(
                            request,
                            data,
                            constraints,
                            prepared,
                            aow_choice,
                            distribution_counts,
                        )
                    })
                    .collect()
            },
        ));
    }

    for (prepared_idx, (prepared, choice_searches)) in weapons.iter().zip(searches).enumerate() {
        let mut groups_by_search = HashMap::<RelevantStatSearch, usize>::new();
        for (aow_idx, search) in choice_searches.into_iter().enumerate() {
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
    plan.fine_work_units = build_search_work_units(&plan, true)?;
    plan.serial_work_units = if matches!(
        request.objective,
        OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
    ) {
        build_search_work_units(&plan, false)?
    } else {
        plan.fine_work_units.clone()
    };
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

/// Every level's ranked rows are retained, so callers keep `top_k` small (the desktop uses 1).
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
        prepare_weapons_with_cancel(
            &max_request,
            data,
            Some(max_constraints),
            &mut should_continue,
        )?
        .into_boxed_slice(),
    );
    let mut max_plan = Some(build_prepared_plan(
        &max_request,
        data,
        max_constraints,
        Arc::clone(&shared_weapons),
        &mut should_continue,
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
            )?
        };
        let reuse = (range_reuse_enabled && level_plan_can_reuse(&plan, &dp_reuse))
            .then_some(&mut dp_reuse);
        let rows = optimize_prepared(&plan, 1_024, |_snapshot| should_continue(), reuse)?;
        results.push(LevelOptimizeResult { level, rows });
        if !level_complete(level) {
            return Err("cancelled".to_string());
        }
    }
    Ok(results)
}

fn level_range_reuse_enabled(request: &OptimizeRequest) -> bool {
    // The fixed-loadout DP cache stays bounded; broaden it only after measuring other range
    // shapes.
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

/// `progress_cb` both receives throttled snapshots and decides whether to continue.
/// `progress_every == 0` disables both, so cancellable callers pass a positive interval.
pub fn optimize_prepared_with_progress<F>(
    plan: &PreparedSearchPlan<'_>,
    progress_every: u64,
    progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    optimize_prepared(plan, progress_every, progress_cb, None)
}

/// With `reuse`, scoring runs serially and shares exact DP tables across a level range.
fn optimize_prepared<F>(
    plan: &PreparedSearchPlan<'_>,
    progress_every: u64,
    mut progress_cb: F,
    reuse: Option<&mut DpReuse>,
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
    let candidates = score_prepared_with_progress(
        plan,
        progress_every,
        |snapshot| {
            last_snapshot = snapshot;
            progress_cb(snapshot)
        },
        reuse,
    )?;
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

pub fn optimize_profiled(
    request: &OptimizeRequest,
    data: &GameData,
) -> Result<ProfiledOptimizeResult, String> {
    let preparation_started = Instant::now();
    let plan = prepare_search(request, data)?;
    let preparation = preparation_started.elapsed();

    let scoring_started = Instant::now();
    let candidates = score_prepared_with_progress(&plan, 0, |_| true, None)?;
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
    reuse: Option<&mut DpReuse>,
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

    let candidates = if reuse.is_none() && should_use_parallel_search(total, fine_work_units.len())
    {
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
            reuse,
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
    let mut candidates = Vec::<ScoredCandidate>::with_capacity(request.top_k.min(work_units.len()));
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
    // Keep serial's exact top-K cutoff visible across Rayon folds.
    // Only these objectives consume work-unit cutoffs during scoring.
    let shared_candidates = matches!(
        request.objective,
        OptimizeObjective::MaxAr
            | OptimizeObjective::MaxPhysicalAr
            | OptimizeObjective::BleedThenAr
    )
    .then(|| Mutex::new(Vec::<ScoredCandidate>::new()));
    let partial_results = work_units
        .par_iter()
        .try_fold(Vec::<ScoredCandidate>::new, |mut candidates, unit| {
            let mut local_progress = ParallelLocalProgress::new(Arc::clone(&progress));
            // Completed units are already shared; copy only the exact cutoff.
            let shared_cutoff = if let Some(shared_candidates) = &shared_candidates {
                let shared = shared_candidates
                    .lock()
                    .map_err(|_| "failed to lock parallel top-K candidates".to_string())?;
                score_cutoff(plan, *unit, &shared, group_mode).cloned()
            } else {
                None
            };
            let cutoff = if shared_candidates.is_some() {
                shared_cutoff.as_ref()
            } else {
                score_cutoff(plan, *unit, &candidates, group_mode)
            };
            let result =
                search_dp_work_unit(plan, *unit, group_mode, &mut local_progress, cutoff, None);
            let finish_result = local_progress.finish();
            let results = match (result, finish_result) {
                (Ok(results), Ok(())) => results,
                (Err(error), _) | (_, Err(error)) => return Err(error),
            };
            // Top-K merging is order-independent, so the shared list is the whole result.
            let target = match &shared_candidates {
                Some(shared_candidates) => &mut *shared_candidates
                    .lock()
                    .map_err(|_| "failed to lock parallel top-K candidates".to_string())?,
                None => &mut candidates,
            };
            merge_scored_top_k(target, results, &plan.weapons, group_mode, request.top_k);
            Ok(candidates)
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut candidates = match shared_candidates {
        Some(shared) => shared
            .into_inner()
            .map_err(|_| "failed to lock parallel top-K candidates".to_string())?,
        None => Vec::new(),
    };
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

/// Test oracle for the DP search.
#[cfg(test)]
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
    let mut candidates = Vec::new();
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
    let mut candidates: Vec<ScoredCandidate> =
        Vec::with_capacity(request.top_k.min(aow_indices.len()));
    let mut route_sets = Vec::with_capacity(aow_indices.len());
    for &aow_idx in aow_indices {
        progress.poll()?;
        // Choices with unmodeled per-hit attack power are dropped during preparation.
        let routes = scalar_route_set(
            &prepared.aow_choices[aow_idx],
            plan.data,
            weapon_uses_two_handing(request, prepared.weapon),
        )?
        .ok_or_else(|| "supported per-hit attack-power effect is not implemented".to_string())?;
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
    // Higher upgrades usually score best, so visiting them first lets the weapon-best bound
    // below skip lower levels early. Ranking is a total order, so visit order never changes
    // which candidate wins.
    for &upgrade in prepared.upgrades.iter().rev() {
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
            // Per weapon, only the weapon's best setup is listed, so a setup whose primary
            // score is strictly below one this unit already found cannot appear. Ties are still
            // solved, so tie order is unchanged.
            let weapon_best = matches!(group_mode, ResultGroupMode::WeaponOnly)
                .then(|| {
                    candidates
                        .iter()
                        .map(|candidate| &candidate.key.score)
                        .max()
                })
                .flatten();
            if let (Some(bound), Some(primary)) = (
                cutoff.max(weapon_best),
                primary.and_then(|p| p.best_primary.as_ref()),
            ) && primary.key.score < *bound
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
            if routes.is_empty()
                && matches!(
                    request.objective,
                    OptimizeObjective::AowFirstHit | OptimizeObjective::AowFullSequence
                )
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
        let mut poll_error = None;
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
            &mut || poll_continues(progress, &mut poll_error),
        )
        .map_err(|error| poll_error.take().unwrap_or(error))?;
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
        let mut poll_error = None;
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
            &mut || poll_continues(progress, &mut poll_error),
        )
        .map_err(|error| poll_error.take().unwrap_or(error))?;
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
    let mut poll_error = None;
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
        &mut || poll_continues(progress, &mut poll_error),
    )
    .map_err(|error| poll_error.take().unwrap_or(error))?;
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

/// DP `should_continue` adapter that keeps a failed poll's own error (for example a
/// poisoned progress lock) instead of the solver's generic cancellation.
fn poll_continues(progress: &mut impl SearchProgress, error: &mut Option<String>) -> bool {
    match progress.poll() {
        Ok(()) => true,
        Err(poll_error) => {
            *error = Some(poll_error);
            false
        }
    }
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
    let routes = scalar_route_set(
        choice,
        data,
        weapon_uses_two_handing(request, prepared.weapon),
    )?
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
    if let Some(route_id) = candidate.route_id.as_deref()
        && !route_matches_handling(route_id, weapon_uses_two_handing(request, prepared.weapon))
    {
        return Err(format!(
            "selected AoW route {route_id} is incompatible with the requested handling"
        ));
    }
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

#[cfg(test)]
mod tests;
