use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::math::{
    apply_aow_attack_buffs, apply_aow_status_buffs, calculate_aow_damage, calculate_ar,
    calculate_status_buildup, class_by_name, compute_free_points, effective_str,
    meets_requirements, scadutree_attack_multiplier,
};
use crate::model::{
    Aow, AowAttackRow, COMBAT_STAT_COUNT, DamageBreakdown, DamageType, GameData, STAT_ARC,
    STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, StatusBuildup, StatusEffectSource, Weapon,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizeObjective {
    MaxAr,
    MaxPhysicalAr,
    MaxArPlusBleed,
    AowFirstHit,
    AowFullSequence,
}

impl OptimizeObjective {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "max_ar" | "ar" | "total_ar" => Ok(Self::MaxAr),
            "max_physical_ar" | "max_phys_ar" | "max_phy_ar" | "physical" => {
                Ok(Self::MaxPhysicalAr)
            }
            "max_ar_plus_bleed" | "max_ar+bleed" | "max_ar_plus_bleed_buildup" | "bleed" => {
                Ok(Self::MaxArPlusBleed)
            }
            "aow_first_hit" | "max_aow_first_hit" | "first_hit" => Ok(Self::AowFirstHit),
            "aow_full_sequence" | "max_aow_full_sequence" | "aow_full" | "full_sequence" => {
                Ok(Self::AowFullSequence)
            }
            _ => Err(format!(
                "invalid objective '{raw}', expected max_ar, max_physical_ar, max_ar_plus_bleed, aow_first_hit, or aow_full_sequence"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxAr => "max_ar",
            Self::MaxPhysicalAr => "max_physical_ar",
            Self::MaxArPlusBleed => "max_ar_plus_bleed",
            Self::AowFirstHit => "aow_first_hit",
            Self::AowFullSequence => "aow_full_sequence",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SomberFilter {
    All,
    StandardOnly,
    SomberOnly,
}

#[derive(Clone, Debug)]
pub struct OptimizeRequest {
    pub class_name: String,
    pub character_level: u16,
    pub current_stats: Stats,
    pub min_combat_stats: [u8; COMBAT_STAT_COUNT],
    pub locked_combat_stats: [Option<u8>; COMBAT_STAT_COUNT],
    pub standard_max_upgrade: u8,
    pub somber_max_upgrade: u8,
    pub exact_upgrade: bool,
    pub two_handing: bool,
    pub dlc_scaling: bool,
    pub scadutree_level: u8,
    pub weapon_name: Option<String>,
    pub affinity: Option<String>,
    pub aow_name: Option<String>,
    pub weapon_type_key: Option<String>,
    pub somber_filter: SomberFilter,
    pub objective: OptimizeObjective,
    pub top_k: usize,
}

impl OptimizeRequest {
    pub fn damage_multiplier(&self) -> f32 {
        scadutree_attack_multiplier(self.dlc_scaling, self.scadutree_level)
    }
}

#[derive(Clone, Debug)]
pub struct OptimizeResult {
    pub weapon_id: u32,
    pub weapon_name: String,
    pub affinity: String,
    pub is_somber: bool,
    pub upgrade: u8,
    pub stats: Stats,
    pub ar: DamageBreakdown,
    pub aow_id: Option<u16>,
    pub aow_name: Option<String>,
    pub bleed_buildup: f32,
    pub bleed_buildup_add: f32,
    pub frost_buildup: f32,
    pub poison_buildup: f32,
    pub scarlet_rot_buildup: f32,
    pub aow_first_hit_damage: f32,
    pub aow_full_sequence_damage: f32,
    pub score: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchEstimate {
    pub weapon_candidates: usize,
    pub stat_candidates: u64,
    pub combinations: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct ProgressSnapshot {
    pub checked: u64,
    pub total: u64,
    pub eligible: u64,
    pub best_score: f32,
    pub elapsed_ms: u64,
}

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
    weapons: Vec<PreparedWeapon<'a>>,
    groups: Vec<PreparedSearchGroup>,
    fine_work_units: Vec<SearchWorkUnit>,
    serial_work_units: Vec<SearchWorkUnit>,
    estimate: SearchEstimate,
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
    let constraints = build_combat_constraints(request)?;
    let weapons = prepare_weapons(request, data, constraints)?;
    let mut groups: Vec<PreparedSearchGroup> = Vec::new();
    let mut stat_candidates = 0_u64;
    let mut combinations = 0_u64;

    for (prepared_idx, prepared) in weapons.iter().enumerate() {
        for (aow_idx, aow_choice) in prepared.aow_choices.iter().enumerate() {
            let Some(search) =
                relevant_stat_search(request, data, constraints, prepared, aow_choice)
            else {
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

pub fn optimize_prepared_with_progress<F>(
    plan: &PreparedSearchPlan<'_>,
    progress_every: u64,
    progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    let request = &plan.request;
    if request.top_k == 0 {
        return Ok(Vec::new());
    }
    if plan.weapons.is_empty() {
        return Ok(Vec::new());
    }

    let group_mode = result_group_mode(request);
    let fine_work_units = &plan.fine_work_units;
    let total = fine_work_units
        .iter()
        .map(|unit| unit.candidate_count)
        .sum::<u64>();
    if total == 0 {
        return Ok(Vec::new());
    }

    if should_use_parallel_search(total, fine_work_units.len()) {
        optimize_parallel(
            plan,
            fine_work_units,
            group_mode,
            total,
            progress_every,
            progress_cb,
        )
    } else {
        let mut progress = SerialSearchProgress::new(total, progress_every, progress_cb);
        optimize_serial(plan, &plan.serial_work_units, group_mode, &mut progress)
    }
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

fn optimize_serial<F>(
    plan: &PreparedSearchPlan<'_>,
    work_units: &[SearchWorkUnit],
    group_mode: ResultGroupMode,
    progress: &mut SerialSearchProgress<F>,
) -> Result<Vec<OptimizeResult>, String>
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
            &plan.weapons,
            group_mode,
            request.top_k,
        );
    }
    progress.emit_final()?;
    materialize_scored_candidates(request, plan.data, &plan.weapons, candidates, group_mode)
}

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
            unit_results.into_iter(),
            &plan.weapons,
            group_mode,
            request.top_k,
        );
    }
    progress.emit_final()?;
    if progress.is_cancelled() {
        return Err("cancelled".to_string());
    }
    materialize_scored_candidates(request, plan.data, &plan.weapons, candidates, group_mode)
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
                if !could_enter_scored_top_k(&candidates, score, request.top_k, group_mode) {
                    continue;
                }
                push_scored_top_k(
                    &mut candidates,
                    ScoredCandidate {
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
                    },
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

    Ok(OptimizeResult {
        weapon_id: prepared.weapon.weapon_id,
        weapon_name: prepared.weapon.name.clone(),
        affinity: prepared.weapon.affinity.clone(),
        is_somber: prepared.weapon.is_somber,
        upgrade: candidate.upgrade,
        stats: candidate.stats,
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
        score: candidate.metric.score,
    })
}

fn could_enter_scored_top_k(
    results: &[ScoredCandidate],
    score: f32,
    top_k: usize,
    group_mode: ResultGroupMode,
) -> bool {
    let limit = scored_candidate_limit(top_k, group_mode);
    if results.len() < limit {
        return true;
    }
    results
        .last()
        .is_none_or(|worst| score >= worst.metric.score)
}

fn merge_scored_top_k(
    results: &mut Vec<ScoredCandidate>,
    candidates: impl IntoIterator<Item = ScoredCandidate>,
    weapons: &[PreparedWeapon<'_>],
    group_mode: ResultGroupMode,
    top_k: usize,
) {
    for candidate in candidates {
        push_scored_top_k(results, candidate, weapons, group_mode, top_k);
    }
}

fn push_scored_top_k(
    results: &mut Vec<ScoredCandidate>,
    candidate: ScoredCandidate,
    weapons: &[PreparedWeapon<'_>],
    group_mode: ResultGroupMode,
    top_k: usize,
) {
    if top_k == 0 {
        return;
    }

    if results.iter().any(|existing| {
        same_scored_result_group(&candidate, existing, weapons, group_mode)
            && compare_known_candidate_metrics(&candidate.metric, &existing.metric)
                == CmpOrdering::Less
    }) {
        return;
    }
    results.retain(|existing| {
        !same_scored_result_group(&candidate, existing, weapons, group_mode)
            || compare_known_candidate_metrics(&candidate.metric, &existing.metric)
                != CmpOrdering::Greater
    });

    let insert_at = results
        .iter()
        .position(|existing| candidate.metric.score > existing.metric.score)
        .unwrap_or(results.len());
    results.insert(insert_at, candidate);

    let limit = scored_candidate_limit(top_k, group_mode);
    if results.len() <= limit {
        return;
    }
    let cutoff = results[limit - 1].metric.score;
    results.retain(|candidate| candidate.metric.score >= cutoff);
}

fn compare_known_candidate_metrics(left: &CandidateMetric, right: &CandidateMetric) -> CmpOrdering {
    let score_order = compare_f32(left.score, right.score);
    if score_order != CmpOrdering::Equal {
        return score_order;
    }
    if let (Some(left_ar), Some(right_ar)) = (left.ar, right.ar) {
        let ar_order = compare_f32(left_ar.total(), right_ar.total());
        if ar_order != CmpOrdering::Equal {
            return ar_order;
        }
    }
    if let (Some(left_full), Some(right_full)) = (
        left.aow_full_sequence_damage,
        right.aow_full_sequence_damage,
    ) {
        let full_order = compare_f32(left_full, right_full);
        if full_order != CmpOrdering::Equal {
            return full_order;
        }
    }
    if let (Some(left_first), Some(right_first)) =
        (left.aow_first_hit_damage, right.aow_first_hit_damage)
    {
        let first_order = compare_f32(left_first, right_first);
        if first_order != CmpOrdering::Equal {
            return first_order;
        }
    }
    if let (Some(left_status), Some(right_status)) = (left.status_buildup, right.status_buildup) {
        let bleed_order = compare_f32(left_status.bleed, right_status.bleed);
        if bleed_order != CmpOrdering::Equal {
            return bleed_order;
        }
    }
    CmpOrdering::Equal
}

fn compare_f32(left: f32, right: f32) -> CmpOrdering {
    if left > right {
        CmpOrdering::Greater
    } else if left < right {
        CmpOrdering::Less
    } else {
        CmpOrdering::Equal
    }
}

fn scored_candidate_limit(top_k: usize, group_mode: ResultGroupMode) -> usize {
    match group_mode {
        ResultGroupMode::WeaponOnly => top_k,
        ResultGroupMode::Loadout => top_k
            .saturating_mul(SCORED_TOP_K_LOADOUT_OVERSAMPLE)
            .max(top_k),
    }
}

fn same_scored_result_group(
    left: &ScoredCandidate,
    right: &ScoredCandidate,
    weapons: &[PreparedWeapon<'_>],
    group_mode: ResultGroupMode,
) -> bool {
    let left_prepared = &weapons[left.prepared_idx];
    let right_prepared = &weapons[right.prepared_idx];
    match group_mode {
        ResultGroupMode::WeaponOnly => left_prepared
            .weapon
            .name
            .eq_ignore_ascii_case(&right_prepared.weapon.name),
        ResultGroupMode::Loadout => {
            left_prepared.weapon.weapon_id == right_prepared.weapon.weapon_id
                && left.upgrade == right.upgrade
                && left_prepared.aow_choices[left.aow_idx].skill_id
                    == right_prepared.aow_choices[right.aow_idx].skill_id
        }
    }
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

fn prepare_weapons<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
    constraints: CombatConstraints,
) -> Result<Vec<PreparedWeapon<'a>>, String> {
    let mut out = Vec::new();
    for weapon in data
        .weapons
        .iter()
        .filter(|entry| weapon_matches_request(entry, request))
    {
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

    if let Some(choice) = native_skill_choice
        && !choice.attack_rows.is_empty()
    {
        return Ok(Some(vec![choice]));
    }

    let choices: Vec<AowChoice<'a>> = data
        .aows
        .iter()
        .filter(|aow| !aow.name.eq_ignore_ascii_case("No Skill"))
        .filter(|aow| data.aow_compatible_with_weapon(aow, weapon))
        .map(|aow| build_aow_choice(aow, weapon, data, request.objective))
        .filter(|choice| !choice.attack_rows.is_empty())
        .collect();
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
    if !weapon.disable_gem_attr {
        return None;
    }
    let source_rows = data.native_skill_attack_rows(weapon.weapon_id);
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
        skill_id: weapon
            .native_skill_id
            .or_else(|| source_rows.first().map(|row| row.aow_id)),
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

fn push_top_k(
    results: &mut Vec<OptimizeResult>,
    candidate: OptimizeResult,
    top_k: usize,
    group_mode: ResultGroupMode,
) {
    if let Some(existing_idx) = results
        .iter()
        .position(|existing| same_result_group(&candidate, existing, group_mode))
    {
        if !better_result(&candidate, &results[existing_idx]) {
            return;
        }
        results.remove(existing_idx);
    }

    let insert_at = results
        .iter()
        .position(|existing| better_result(&candidate, existing))
        .unwrap_or(results.len());

    if insert_at >= top_k {
        if results.len() < top_k {
            results.push(candidate);
        }
        return;
    }

    results.insert(insert_at, candidate);
    if results.len() > top_k {
        results.pop();
    }
}

fn same_result_group(
    left: &OptimizeResult,
    right: &OptimizeResult,
    group_mode: ResultGroupMode,
) -> bool {
    match group_mode {
        ResultGroupMode::WeaponOnly => left.weapon_name.eq_ignore_ascii_case(&right.weapon_name),
        ResultGroupMode::Loadout => {
            left.weapon_id == right.weapon_id
                && left.upgrade == right.upgrade
                && left.aow_id == right.aow_id
        }
    }
}

fn better_result(left: &OptimizeResult, right: &OptimizeResult) -> bool {
    if left.score > right.score {
        return true;
    }
    if left.score < right.score {
        return false;
    }

    let left_ar = left.ar.total();
    let right_ar = right.ar.total();
    if left_ar > right_ar {
        return true;
    }
    if left_ar < right_ar {
        return false;
    }

    if left.aow_full_sequence_damage > right.aow_full_sequence_damage {
        return true;
    }
    if left.aow_full_sequence_damage < right.aow_full_sequence_damage {
        return false;
    }

    if left.aow_first_hit_damage > right.aow_first_hit_damage {
        return true;
    }
    if left.aow_first_hit_damage < right.aow_first_hit_damage {
        return false;
    }

    if left.bleed_buildup > right.bleed_buildup {
        return true;
    }
    if left.bleed_buildup < right.bleed_buildup {
        return false;
    }

    if left.weapon_id != right.weapon_id {
        return left.weapon_id < right.weapon_id;
    }
    if left.upgrade != right.upgrade {
        return left.upgrade > right.upgrade;
    }
    false
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::data::load_game_data;

    use super::*;

    fn load_data() -> GameData {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        load_game_data(data_path).expect("failed to load phase1 data")
    }

    #[test]
    fn objective_aliases_parse_to_canonical_variants() {
        assert_eq!(
            OptimizeObjective::parse("max_phys_ar").unwrap(),
            OptimizeObjective::MaxPhysicalAr
        );
        assert_eq!(
            OptimizeObjective::parse("max_ar+bleed").unwrap(),
            OptimizeObjective::MaxArPlusBleed
        );
        assert_eq!(
            OptimizeObjective::parse("aow_full").unwrap().as_str(),
            "aow_full_sequence"
        );
        assert!(OptimizeObjective::parse("not_real").is_err());
    }

    fn test_result(
        weapon_id: u32,
        upgrade: u8,
        physical_ar: f32,
        bleed_buildup: f32,
    ) -> OptimizeResult {
        OptimizeResult {
            weapon_id,
            weapon_name: format!("Test Weapon {weapon_id}"),
            affinity: "Standard".to_string(),
            is_somber: false,
            upgrade,
            stats: Stats {
                vig: 10,
                mnd: 10,
                end: 10,
                str: 10,
                dex: 10,
                int: 10,
                fai: 10,
                arc: 10,
            },
            ar: DamageBreakdown {
                physical: physical_ar,
                magic: 0.0,
                fire: 0.0,
                lightning: 0.0,
                holy: 0.0,
            },
            aow_id: None,
            aow_name: None,
            bleed_buildup,
            bleed_buildup_add: 0.0,
            frost_buildup: 0.0,
            poison_buildup: 0.0,
            scarlet_rot_buildup: 0.0,
            aow_first_hit_damage: 0.0,
            aow_full_sequence_damage: 0.0,
            score: bleed_buildup,
        }
    }

    fn scored_candidate(
        prepared_idx: usize,
        aow_idx: usize,
        upgrade: u8,
        stats: Stats,
        score: f32,
    ) -> ScoredCandidate {
        ScoredCandidate {
            prepared_idx,
            aow_idx,
            upgrade,
            stats,
            metric: CandidateMetric {
                score,
                ar: None,
                status_buildup: None,
                aow_first_hit_damage: None,
                aow_full_sequence_damage: None,
            },
        }
    }

    fn base_request() -> OptimizeRequest {
        OptimizeRequest {
            class_name: "Samurai".to_string(),
            character_level: 9,
            current_stats: Stats {
                vig: 12,
                mnd: 11,
                end: 13,
                str: 12,
                dex: 15,
                int: 9,
                fai: 8,
                arc: 8,
            },
            min_combat_stats: [0, 0, 0, 0, 0],
            locked_combat_stats: [None, None, None, None, None],
            standard_max_upgrade: 25,
            somber_max_upgrade: 10,
            exact_upgrade: false,
            two_handing: false,
            dlc_scaling: false,
            scadutree_level: 0,
            weapon_name: Some("Uchigatana".to_string()),
            affinity: Some("Keen".to_string()),
            aow_name: None,
            weapon_type_key: None,
            somber_filter: SomberFilter::All,
            objective: OptimizeObjective::MaxAr,
            top_k: 3,
        }
    }

    fn broad_request() -> OptimizeRequest {
        OptimizeRequest {
            class_name: "Samurai".to_string(),
            character_level: 150,
            current_stats: Stats {
                vig: 40,
                mnd: 20,
                end: 20,
                str: 20,
                dex: 20,
                int: 20,
                fai: 20,
                arc: 20,
            },
            min_combat_stats: [0, 0, 0, 0, 0],
            locked_combat_stats: [None, None, None, None, None],
            standard_max_upgrade: 25,
            somber_max_upgrade: 10,
            exact_upgrade: true,
            two_handing: false,
            dlc_scaling: false,
            scadutree_level: 0,
            weapon_name: None,
            affinity: None,
            aow_name: None,
            weapon_type_key: None,
            somber_filter: SomberFilter::All,
            objective: OptimizeObjective::MaxAr,
            top_k: 3,
        }
    }

    fn active_mask_for(
        game_data: &GameData,
        weapon_name: &str,
        affinity: &str,
        objective: OptimizeObjective,
        aow_name: Option<&str>,
    ) -> [bool; COMBAT_STAT_COUNT] {
        let mut request = broad_request();
        request.weapon_name = Some(weapon_name.to_string());
        request.affinity = Some(affinity.to_string());
        request.aow_name = aow_name.map(str::to_string);
        request.objective = objective;
        let constraints = build_combat_constraints(&request).expect("constraints failed");
        let prepared_weapons =
            prepare_weapons(&request, game_data, constraints).expect("prepare failed");
        let prepared = prepared_weapons.first().expect("expected prepared weapon");
        active_stats_for_choice(
            &request,
            prepared,
            prepared.aow_choices.first().expect("expected AoW choice"),
            game_data,
        )
    }

    #[test]
    fn optimize_returns_sorted_top_results_for_locked_weapon() {
        let game_data = load_data();
        let request = base_request();
        let results = optimize(&request, &game_data).expect("optimizer failed");

        assert!(!results.is_empty());
        assert!(
            results
                .windows(2)
                .all(|pair| pair[0].score >= pair[1].score)
        );
        assert!(
            results
                .iter()
                .all(|result| result.weapon_name == "Uchigatana")
        );
        assert!(results.iter().all(|result| result.affinity == "Keen"));
        assert!(results.iter().all(|result| result.upgrade <= 25));
    }

    #[test]
    fn max_physical_ar_scores_the_physical_ar_component() {
        let game_data = load_data();
        let mut request = base_request();
        request.objective = OptimizeObjective::MaxPhysicalAr;
        request.affinity = Some("Heavy".to_string());
        request.aow_name = Some("Seppuku".to_string());
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!((results[0].score - results[0].ar.physical).abs() < 0.001);
    }

    #[test]
    fn scadutree_scaling_multiplies_outgoing_damage_only() {
        let game_data = load_data();
        let mut base = base_request();
        base.affinity = Some("Blood".to_string());
        base.aow_name = Some("Seppuku".to_string());
        base.standard_max_upgrade = 25;
        base.somber_max_upgrade = 10;
        base.exact_upgrade = true;
        base.top_k = 1;

        let mut scaled = base.clone();
        scaled.dlc_scaling = true;
        scaled.scadutree_level = 20;

        let base_result = optimize(&base, &game_data)
            .expect("base optimizer failed")
            .pop()
            .expect("expected base result");
        let scaled_result = optimize(&scaled, &game_data)
            .expect("scaled optimizer failed")
            .pop()
            .expect("expected scaled result");

        assert!((scaled_result.ar.total() - base_result.ar.total() * 2.05).abs() < 0.1);
        assert!(
            (scaled_result.aow_first_hit_damage - base_result.aow_first_hit_damage * 2.05).abs()
                < 0.1
        );
        assert!((scaled_result.bleed_buildup - base_result.bleed_buildup).abs() < 0.001);
    }

    #[test]
    fn scadutree_curve_uses_patch_1122_values() {
        assert!((crate::math::scadutree_attack_multiplier(true, 1) - 1.10).abs() < f32::EPSILON);
        assert!((crate::math::scadutree_attack_multiplier(true, 12) - 1.85).abs() < f32::EPSILON);
        assert!((crate::math::scadutree_attack_multiplier(true, 20) - 2.05).abs() < f32::EPSILON);
        assert!((crate::math::scadutree_attack_multiplier(false, 20) - 1.0).abs() < f32::EPSILON);
        assert!(
            (crate::math::scadutree_damage_negation(true, 20) - (1.0 - 1.0 / 2.05)).abs() < 0.0001
        );
    }

    #[test]
    fn optimize_errors_when_stats_exceed_level_budget() {
        let game_data = load_data();
        let mut request = base_request();
        request.current_stats.str = 40;
        request.current_stats.dex = 40;

        let err = optimize(&request, &game_data).expect_err("expected budget error");
        assert!(err.contains("level budget"));
    }

    #[test]
    fn optimize_respects_weapon_type_filter() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = None;
        request.affinity = None;
        request.weapon_type_key = Some("Katana".to_string());
        request.top_k = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        for result in &results {
            let weapon = game_data
                .weapons
                .iter()
                .find(|weapon| {
                    weapon.weapon_id == result.weapon_id && weapon.affinity == result.affinity
                })
                .expect("missing weapon");
            assert!(weapon.weapon_type_name.eq_ignore_ascii_case("Katana"));
        }
    }

    #[test]
    fn open_weapon_search_returns_one_result_per_weapon() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = None;
        request.affinity = None;
        request.aow_name = None;
        request.weapon_type_key = Some("Katana".to_string());
        request.top_k = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");

        assert!(!results.is_empty());
        let unique_weapon_names = results
            .iter()
            .map(|result| result.weapon_name.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        assert_eq!(unique_weapon_names.len(), results.len());
        assert_eq!(
            results
                .iter()
                .filter(|result| result.weapon_name == "Uchigatana")
                .count(),
            1
        );
    }

    #[test]
    fn open_ar_search_excludes_compatible_aows_that_cannot_change_ar() {
        let game_data = load_data();
        let request = base_request();
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Blood")
            .expect("missing blood Uchigatana");

        let choices = resolve_aow_choices(weapon, &request, &game_data)
            .expect("AoW resolution failed")
            .expect("expected open AoW choices");
        let names = choices
            .iter()
            .filter_map(|choice| choice.skill_name)
            .collect::<HashSet<_>>();

        assert!(names.contains("Seppuku"));
        assert!(!names.contains("Double Slash"));
        assert!(choices.iter().all(|choice| {
            choice
                .aow
                .is_none_or(|aow| aow_affects_objective(aow, OptimizeObjective::MaxAr))
        }));
    }

    #[test]
    fn locked_weapon_search_keeps_multiple_loadouts_for_that_weapon() {
        let game_data = load_data();
        let mut request = base_request();
        request.affinity = None;
        request.aow_name = None;
        request.top_k = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");

        assert!(results.len() > 1);
        assert!(
            results
                .iter()
                .all(|result| result.weapon_name == "Uchigatana")
        );
        let unique_loadouts = results
            .iter()
            .map(|result| (result.upgrade, result.aow_id))
            .collect::<HashSet<_>>();
        assert!(unique_loadouts.len() > 1);
    }

    #[test]
    fn optimize_accepts_normalized_weapon_type_filter_names() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = None;
        request.affinity = None;
        request.weapon_type_key = Some("Hand-to-Hand Arts".to_string());
        request.top_k = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        for result in &results {
            let weapon = game_data
                .weapons
                .iter()
                .find(|weapon| {
                    weapon.weapon_id == result.weapon_id && weapon.affinity == result.affinity
                })
                .expect("missing weapon");
            assert!(weapon.weapon_type_name.eq_ignore_ascii_case("Hand-to-Hand"));
        }
    }

    #[test]
    fn parallel_search_matches_serial_results() {
        let game_data = load_data();
        let mut request = base_request();
        request.character_level = 46;
        request.current_stats = Stats {
            vig: 12,
            mnd: 11,
            end: 13,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 45,
        };
        request.weapon_name = None;
        request.affinity = None;
        request.weapon_type_key = Some("Katana".to_string());
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.top_k = 5;

        let plan = prepare_search(&request, &game_data).expect("prepare failed");
        let work_units = build_search_work_units(&plan, true);
        let total = work_units
            .iter()
            .map(|unit| unit.candidate_count)
            .sum::<u64>();

        let mut serial_progress = SerialSearchProgress::new(total, 0, |_snapshot| true);
        let serial = optimize_serial(
            &plan,
            &work_units,
            result_group_mode(&request),
            &mut serial_progress,
        )
        .expect("serial search failed");
        let parallel = optimize_parallel(
            &plan,
            &work_units,
            result_group_mode(&request),
            total,
            0,
            |_snapshot| true,
        )
        .expect("parallel search failed");

        assert_eq!(serial.len(), parallel.len());
        for (left, right) in serial.iter().zip(parallel.iter()) {
            assert_eq!(left.weapon_id, right.weapon_id);
            assert_eq!(left.aow_id, right.aow_id);
            assert_eq!(left.upgrade, right.upgrade);
            assert_eq!(left.stats, right.stats);
            assert!((left.score - right.score).abs() < 0.001);
        }
    }

    #[test]
    fn parallel_threshold_accepts_medium_searches_when_threads_are_available() {
        let thread_count = rayon::current_num_threads();
        assert!(!should_use_parallel_search(
            PARALLEL_SEARCH_MIN_COMBINATIONS - 1,
            usize::MAX
        ));
        assert!(!should_use_parallel_search(
            PARALLEL_SEARCH_MIN_COMBINATIONS,
            1
        ));
        if thread_count > 1 {
            assert!(should_use_parallel_search(
                PARALLEL_SEARCH_MIN_COMBINATIONS,
                thread_count.min(2)
            ));
        }
    }

    #[test]
    fn parallel_work_units_split_to_single_aow_choices() {
        let game_data = load_data();
        let mut request = base_request();
        request.aow_name = None;
        request.top_k = 5;

        let plan = prepare_search(&request, &game_data).expect("prepare failed");
        let fine = build_search_work_units(&plan, true);
        let grouped = build_search_work_units(&plan, false);

        assert!(!fine.is_empty());
        assert!(fine.iter().all(|unit| unit.aow_end - unit.aow_start == 1));
        assert_eq!(
            fine.iter().map(|unit| unit.candidate_count).sum::<u64>(),
            grouped.iter().map(|unit| unit.candidate_count).sum::<u64>()
        );
    }

    #[test]
    fn prepared_plan_groups_aows_that_share_one_stat_search() {
        let game_data = load_data();
        let mut request = base_request();
        request.character_level = 46;
        request.current_stats.arc = 8;
        request.weapon_name = None;
        request.affinity = None;
        request.weapon_type_key = Some("Katana".to_string());
        request.exact_upgrade = true;
        request.objective = OptimizeObjective::MaxArPlusBleed;
        request.top_k = 5;

        let plan = prepare_search(&request, &game_data).expect("prepare failed");
        assert!(
            plan.groups.iter().any(|group| group.aow_indices.len() > 1),
            "expected compatible AoWs with the same relevant-stat search to share a group"
        );
        let grouped = build_search_work_units(&plan, false);
        assert!(grouped.iter().any(|unit| unit.aow_end - unit.aow_start > 1));
        assert_eq!(
            grouped.iter().map(|unit| unit.candidate_count).sum::<u64>(),
            plan.estimate().combinations
        );
        let rows =
            optimize_prepared_with_progress(&plan, 0, |_| true).expect("prepared search failed");
        assert!(!rows.is_empty());
    }

    #[test]
    fn scored_top_k_retains_cutoff_score_ties() {
        let game_data = load_data();
        let request = base_request();
        let constraints = build_combat_constraints(&request).expect("constraints failed");
        let weapons = prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
        let stats = request.current_stats;
        let mut candidates = Vec::new();

        for (upgrade, score) in [
            (0, 20.0),
            (1, 19.0),
            (2, 18.0),
            (3, 17.0),
            (4, 16.0),
            (5, 15.0),
            (6, 14.0),
            (7, 13.0),
            (8, 13.0),
            (9, 12.0),
        ] {
            push_scored_top_k(
                &mut candidates,
                scored_candidate(0, 0, upgrade, stats, score),
                &weapons,
                ResultGroupMode::Loadout,
                1,
            );
        }

        assert_eq!(candidates.len(), 9);
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.metric.score)
                .collect::<Vec<_>>(),
            vec![20.0, 19.0, 18.0, 17.0, 16.0, 15.0, 14.0, 13.0, 13.0]
        );
    }

    #[test]
    fn scored_top_k_replaces_lower_score_for_same_loadout() {
        let game_data = load_data();
        let request = base_request();
        let constraints = build_combat_constraints(&request).expect("constraints failed");
        let weapons = prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
        let stats = request.current_stats;
        let mut candidates = Vec::new();

        push_scored_top_k(
            &mut candidates,
            scored_candidate(0, 0, 25, stats, 10.0),
            &weapons,
            ResultGroupMode::Loadout,
            3,
        );
        push_scored_top_k(
            &mut candidates,
            scored_candidate(0, 0, 25, stats, 20.0),
            &weapons,
            ResultGroupMode::Loadout,
            3,
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].metric.score, 20.0);
    }

    #[test]
    fn scored_top_k_discards_equal_score_candidates_with_lower_known_ar() {
        let game_data = load_data();
        let request = base_request();
        let constraints = build_combat_constraints(&request).expect("constraints failed");
        let weapons = prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
        let stats = request.current_stats;
        let mut candidates = Vec::new();

        for physical in [100.0, 200.0, 150.0] {
            let mut candidate = scored_candidate(0, 0, 25, stats, 80.0);
            candidate.metric.ar = Some(DamageBreakdown {
                physical,
                ..DamageBreakdown::default()
            });
            push_scored_top_k(
                &mut candidates,
                candidate,
                &weapons,
                ResultGroupMode::Loadout,
                3,
            );
        }

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].metric.ar.expect("AR missing").physical, 200.0);
    }

    #[test]
    fn top_k_zero_short_circuits_without_progress_callbacks() {
        let game_data = load_data();
        let mut request = base_request();
        request.top_k = 0;
        let mut callback_count = 0;

        let results = optimize_with_progress(&request, &game_data, 1, |_snapshot| {
            callback_count += 1;
            true
        })
        .expect("optimizer failed");

        assert!(results.is_empty());
        assert_eq!(callback_count, 0);
    }

    #[test]
    fn progress_emits_initial_and_final_snapshots() {
        let game_data = load_data();
        let request = base_request();
        let mut snapshots = Vec::new();

        optimize_with_progress(&request, &game_data, u64::MAX, |snapshot| {
            snapshots.push(snapshot);
            true
        })
        .expect("optimizer failed");

        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].checked, 0);
        assert_eq!(snapshots[1].checked, snapshots[1].total);
    }

    #[test]
    fn progress_callback_can_cancel_search() {
        let game_data = load_data();
        let request = base_request();
        let mut callback_count = 0;

        let err = optimize_with_progress(&request, &game_data, 1, |_snapshot| {
            callback_count += 1;
            false
        })
        .expect_err("expected cancellation");

        assert_eq!(err, "cancelled");
        assert_eq!(callback_count, 1);
    }

    #[test]
    fn optimize_respects_exact_stat_lock() {
        let game_data = load_data();
        let mut request = base_request();
        request.standard_max_upgrade = 0;
        request.somber_max_upgrade = 0;
        request.exact_upgrade = true;
        request.locked_combat_stats[STAT_ARC] = Some(8);
        request.locked_combat_stats[STAT_DEX] = Some(15);

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        for row in &results {
            assert_eq!(row.stats.dex, 15);
            assert_eq!(row.stats.arc, 8);
        }
    }

    #[test]
    fn optimize_respects_all_exact_combat_stat_locks() {
        let game_data = load_data();
        let mut request = base_request();
        request.standard_max_upgrade = 0;
        request.somber_max_upgrade = 0;
        request.exact_upgrade = true;
        request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(8)];

        let constraints = build_combat_constraints(&request).expect("constraints failed");
        assert_eq!(count_stat_candidates(constraints), 1);

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        for row in &results {
            assert_eq!(row.stats.str, 12);
            assert_eq!(row.stats.dex, 15);
            assert_eq!(row.stats.int, 9);
            assert_eq!(row.stats.fai, 8);
            assert_eq!(row.stats.arc, 8);
        }
    }

    #[test]
    fn relevant_stat_masks_track_scaling_sources() {
        let game_data = load_data();

        assert_eq!(
            active_mask_for(
                &game_data,
                "Giant-Crusher",
                "Heavy",
                OptimizeObjective::MaxAr,
                None
            ),
            [true, false, false, false, false]
        );
        assert_eq!(
            active_mask_for(
                &game_data,
                "Swift Spear",
                "Keen",
                OptimizeObjective::MaxAr,
                None
            ),
            [false, true, false, false, false]
        );
        assert_eq!(
            active_mask_for(
                &game_data,
                "Claymore",
                "Quality",
                OptimizeObjective::MaxAr,
                None
            ),
            [true, true, false, false, false]
        );
        assert_eq!(
            active_mask_for(
                &game_data,
                "Sword Lance",
                "Magic",
                OptimizeObjective::MaxAr,
                None
            ),
            [true, true, true, false, false]
        );
        assert!(
            active_mask_for(
                &game_data,
                "Uchigatana",
                "Blood",
                OptimizeObjective::MaxArPlusBleed,
                Some("Seppuku")
            )[STAT_ARC]
        );
    }

    #[test]
    fn aow_override_rows_contribute_relevant_stats() {
        let game_data = load_data();
        let mask = active_mask_for(
            &game_data,
            "Giant-Crusher",
            "Heavy",
            OptimizeObjective::AowFirstHit,
            Some("Prelate's Charge"),
        );

        assert!(
            mask[STAT_FAI],
            "expected fire override attack rows to activate FAI scaling"
        );
    }

    #[test]
    fn requirement_only_inactive_stats_are_preserved() {
        let game_data = load_data();
        let mut request = base_request();
        request.class_name = "Wretch".to_string();
        request.character_level = 20;
        request.current_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 10,
            dex: 10,
            int: 10,
            fai: 10,
            arc: 10,
        };
        request.weapon_name = Some("Uchigatana".to_string());
        request.affinity = Some("Heavy".to_string());
        request.aow_name = Some("Unsheathe".to_string());
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.top_k = 1;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(results[0].stats.dex >= 15);
    }

    #[test]
    fn exact_locks_override_relevant_stat_pruning() {
        let game_data = load_data();
        let mut request = base_request();
        request.character_level = 31;
        request.weapon_name = Some("Uchigatana".to_string());
        request.affinity = Some("Heavy".to_string());
        request.aow_name = Some("Unsheathe".to_string());
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.locked_combat_stats[STAT_FAI] = Some(30);
        request.top_k = 1;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert_eq!(results[0].stats.fai, 30);
    }

    #[test]
    fn estimate_search_space_uses_relevant_stat_counts() {
        let game_data = load_data();
        let mut request = broad_request();
        request.weapon_type_key = Some("Katana".to_string());
        request.top_k = 5;

        let constraints = build_combat_constraints(&request).expect("constraints failed");
        let broad_stat_count = count_stat_candidates(constraints);
        let prepared_weapons =
            prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
        let broad_slots: u64 = prepared_weapons
            .iter()
            .map(|prepared| (prepared.upgrades.len() * prepared.aow_choices.len()) as u64)
            .sum();
        let broad_combinations = broad_stat_count.saturating_mul(broad_slots);
        let estimate = estimate_search_space(&request, &game_data).expect("estimate failed");

        assert!(estimate.combinations < broad_combinations);
        assert!(estimate.stat_candidates < broad_stat_count.saturating_mul(broad_slots));
        assert!(
            !optimize(&request, &game_data)
                .expect("optimizer failed")
                .is_empty()
        );
    }

    #[test]
    fn optimize_keeps_one_result_per_weapon_setup() {
        let game_data = load_data();
        let mut request = base_request();
        request.character_level = 148;
        request.weapon_name = Some("Lizard Greatsword".to_string());
        request.affinity = Some("Keen".to_string());
        request.aow_name = Some("Seppuku".to_string());
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.two_handing = true;
        request.top_k = 50;

        let results = optimize(&request, &game_data).expect("optimizer failed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].weapon_name, "Lizard Greatsword");
        assert_eq!(results[0].affinity, "Keen");
        assert_eq!(results[0].aow_name.as_deref(), Some("Seppuku"));
        assert_eq!(results[0].upgrade, 25);
    }

    #[test]
    fn exact_stat_locks_reject_unallocatable_remaining_points() {
        let game_data = load_data();
        let mut request = base_request();
        request.character_level = 10;
        request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(8)];

        let err = optimize(&request, &game_data).expect_err("expected exact-lock budget error");
        assert!(err.contains("locked combat stats cannot absorb remaining free points"));
    }

    #[test]
    fn optimize_rejects_stat_caps_above_99() {
        let game_data = load_data();

        let mut current = base_request();
        current.current_stats.str = 100;
        let err = optimize(&current, &game_data).expect_err("expected current stat cap error");
        assert!(err.contains("str must be <= 99"));

        let mut minimum = base_request();
        minimum.min_combat_stats[STAT_STR] = 100;
        let err = optimize(&minimum, &game_data).expect_err("expected minimum stat cap error");
        assert!(err.contains("minimum combat stat 0 must be <= 99"));

        let mut locked = base_request();
        locked.locked_combat_stats[STAT_STR] = Some(100);
        let err = optimize(&locked, &game_data).expect_err("expected locked stat cap error");
        assert!(err.contains("locked combat stat 0 must be <= 99"));
    }

    #[test]
    fn available_upgrades_skips_sparse_reinforce_levels() {
        let mut game_data = load_data();
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
            .expect("missing weapon")
            .clone();
        let levels = &mut game_data.reinforce[usize::from(weapon.reinforce_type)];
        assert!(levels[5].take().is_some());

        let mut request = base_request();
        request.exact_upgrade = false;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        let upgrades =
            available_upgrades(&weapon, &request, &game_data).expect("expected upgrades");

        assert!(upgrades.contains(&4));
        assert!(!upgrades.contains(&5));
        assert!(upgrades.contains(&6));
    }

    #[test]
    fn fixed_upgrade_rejects_missing_sparse_reinforce_level() {
        let mut game_data = load_data();
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
            .expect("missing weapon")
            .clone();
        let levels = &mut game_data.reinforce[usize::from(weapon.reinforce_type)];
        assert!(levels[5].take().is_some());

        let mut request = base_request();
        request.standard_max_upgrade = 5;
        request.somber_max_upgrade = 5;
        request.exact_upgrade = true;

        assert!(available_upgrades(&weapon, &request, &game_data).is_none());
    }

    #[test]
    fn exact_upgrade_uses_weapon_class_caps() {
        let game_data = load_data();
        let standard_weapon = game_data
            .weapons
            .iter()
            .find(|weapon| {
                !weapon.is_somber && weapon.name == "Uchigatana" && weapon.affinity == "Keen"
            })
            .expect("missing standard weapon");
        let somber_weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.is_somber)
            .expect("missing somber weapon");
        let mut request = broad_request();
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;

        assert_eq!(
            available_upgrades(standard_weapon, &request, &game_data).expect("standard upgrades"),
            vec![25],
        );
        assert_eq!(
            available_upgrades(somber_weapon, &request, &game_data).expect("somber upgrades"),
            vec![10],
        );
    }

    #[test]
    fn somber_only_exact_uses_somber_cap_not_standard_cap() {
        let game_data = load_data();
        let mut request = broad_request();
        request.somber_filter = SomberFilter::SomberOnly;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.top_k = 25;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(results.iter().all(|result| result.is_somber));
        assert!(results.iter().all(|result| result.upgrade == 10));
    }

    #[test]
    fn optimize_rejects_seppuku_on_cold_affinity() {
        let game_data = load_data();
        let mut request = base_request();
        request.affinity = Some("Cold".to_string());
        request.aow_name = Some("Seppuku".to_string());
        request.objective = OptimizeObjective::MaxArPlusBleed;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(results.is_empty());
    }

    #[test]
    fn paired_weapon_two_handing_does_not_inflate_ar() {
        let game_data = load_data();
        let mut one_hand = base_request();
        one_hand.class_name = "Wretch".to_string();
        one_hand.character_level = 64;
        one_hand.current_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 68,
            dex: 15,
            int: 10,
            fai: 10,
            arc: 10,
        };
        one_hand.weapon_name = Some("Iron Ball".to_string());
        one_hand.affinity = Some("Heavy".to_string());
        one_hand.aow_name = None;
        one_hand.standard_max_upgrade = 25;
        one_hand.somber_max_upgrade = 10;
        one_hand.exact_upgrade = true;
        one_hand.locked_combat_stats = [Some(68), Some(15), Some(10), Some(10), Some(10)];
        one_hand.two_handing = false;

        let mut two_hand = one_hand.clone();
        two_hand.two_handing = true;

        let one_hand_results = optimize(&one_hand, &game_data).expect("optimizer failed");
        let two_hand_results = optimize(&two_hand, &game_data).expect("optimizer failed");
        assert!(!one_hand_results.is_empty());
        assert!(!two_hand_results.is_empty());
        assert!((one_hand_results[0].ar.total() - two_hand_results[0].ar.total()).abs() < 0.001);
    }

    #[test]
    fn paired_weapon_two_handing_does_not_reduce_requirements() {
        let game_data = load_data();
        let mut request = base_request();
        request.class_name = "Wretch".to_string();
        request.weapon_name = Some("Starscourge Greatsword".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = None;
        request.character_level = 24;
        request.current_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 26,
            dex: 12,
            int: 15,
            fai: 10,
            arc: 10,
        };
        request.standard_max_upgrade = 10;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.locked_combat_stats = [Some(26), Some(12), Some(15), Some(10), Some(10)];
        request.two_handing = true;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(results.is_empty());
    }

    #[test]
    fn wasted_points_on_zero_scaling_stats_are_filtered() {
        let game_data = load_data();
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Sword Lance" && weapon.affinity == "Magic")
            .expect("missing weapon");
        let mut request = base_request();
        request.weapon_name = Some("Sword Lance".to_string());
        request.affinity = Some("Magic".to_string());
        request.aow_name = Some("Glintstone Pebble".to_string());
        request.current_stats = Stats {
            vig: 40,
            mnd: 11,
            end: 20,
            str: 21,
            dex: 15,
            int: 40,
            fai: 8,
            arc: 8,
        };
        request.character_level = 86;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.top_k = 10;
        let constraints = build_combat_constraints(&request).expect("constraints failed");
        let prepared_weapons =
            prepare_weapons(&request, &game_data, constraints).expect("prepare failed");
        let prepared = prepared_weapons
            .iter()
            .find(|prepared| prepared.weapon.weapon_id == weapon.weapon_id)
            .expect("missing prepared weapon");
        let search = relevant_stat_search(
            &request,
            &game_data,
            constraints,
            prepared,
            &prepared.aow_choices[0],
        )
        .expect("expected relevant stat search");
        assert!(!search.active[STAT_FAI]);
        assert!(!search.active[STAT_ARC]);
        assert!(search.candidate_count < count_stat_candidates(constraints));

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .all(|row| row.stats.fai == 8 && row.stats.arc == 8)
        );
    }

    #[test]
    fn exact_aow_compatibility_is_loaded_from_csv() {
        let game_data = load_data();
        let cold_uchi = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Cold")
            .expect("missing cold uchigatana");
        let fire_uchi = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Fire")
            .expect("missing fire uchigatana");
        let blood_uchi = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Blood")
            .expect("missing blood uchigatana");
        let seppuku = game_data
            .aows
            .iter()
            .find(|aow| aow.name == "Seppuku")
            .expect("missing seppuku");

        assert!(!game_data.aow_compatible_with_weapon(seppuku, cold_uchi));
        assert!(!game_data.aow_compatible_with_weapon(seppuku, fire_uchi));
        assert!(game_data.aow_compatible_with_weapon(seppuku, blood_uchi));
    }

    #[test]
    fn max_ar_plus_bleed_uses_innate_weapon_buildup() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Rivers of Blood".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = None;
        request.objective = OptimizeObjective::MaxArPlusBleed;
        request.standard_max_upgrade = 10;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.current_stats = Stats {
            vig: 40,
            mnd: 11,
            end: 20,
            str: 12,
            dex: 20,
            int: 9,
            fai: 8,
            arc: 20,
        };
        request.character_level = 61;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(results[0].bleed_buildup >= 50.0);
        assert_eq!(results[0].score, results[0].bleed_buildup);
    }

    #[test]
    fn max_ar_plus_bleed_score_is_bleed_buildup() {
        let score = score_for(
            OptimizeObjective::MaxArPlusBleed,
            900.0,
            StatusBuildup {
                bleed: 78.0,
                frost: 0.0,
                poison: 0.0,
                scarlet_rot: 0.0,
                sleep: 0.0,
                madness: 0.0,
                death: 0.0,
            },
            0.0,
            0.0,
        );

        assert_eq!(score, 78.0);
    }

    #[test]
    fn max_ar_plus_bleed_prefers_higher_bleed_over_higher_ar_plus_bleed() {
        let high_ar = test_result(1, 25, 900.0, 40.0);
        let high_bleed = test_result(2, 25, 500.0, 60.0);

        assert!(better_result(&high_bleed, &high_ar));
        assert!(!better_result(&high_ar, &high_bleed));
    }

    #[test]
    fn max_ar_plus_bleed_equal_bleed_falls_through_to_higher_ar() {
        let low_ar = test_result(1, 25, 500.0, 60.0);
        let high_ar = test_result(2, 25, 900.0, 60.0);

        assert!(better_result(&high_ar, &low_ar));
        assert!(!better_result(&low_ar, &high_ar));
    }

    #[test]
    fn aow_first_hit_damage_is_loaded_and_scored() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Sword Lance".to_string());
        request.affinity = Some("Magic".to_string());
        request.aow_name = Some("Glintstone Pebble".to_string());
        request.objective = OptimizeObjective::AowFirstHit;
        request.current_stats = Stats {
            vig: 40,
            mnd: 11,
            end: 20,
            str: 21,
            dex: 15,
            int: 40,
            fai: 8,
            arc: 8,
        };
        request.character_level = 84;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(results[0].aow_first_hit_damage > 0.0);
        assert!(results[0].aow_full_sequence_damage >= results[0].aow_first_hit_damage);
        assert_eq!(results[0].score, results[0].aow_first_hit_damage);
    }

    #[test]
    fn seppuku_weapon_buff_affects_ar_and_bleed() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Uchigatana".to_string());
        request.affinity = Some("Blood".to_string());
        request.current_stats = Stats {
            vig: 12,
            mnd: 11,
            end: 13,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 45,
        };
        request.character_level = 46;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(45)];
        request.objective = OptimizeObjective::MaxAr;
        request.aow_name = Some("Double Slash".to_string());

        let base_results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!base_results.is_empty());

        request.aow_name = Some("Seppuku".to_string());
        let seppuku_results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!seppuku_results.is_empty());

        let base = &base_results[0];
        let buffed = &seppuku_results[0];
        assert!(buffed.ar.total() >= base.ar.total() + 29.9);
        assert!(buffed.bleed_buildup > base.bleed_buildup + 30.0);
    }

    #[test]
    fn open_max_ar_search_considers_compatible_buff_aows() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Uchigatana".to_string());
        request.affinity = Some("Blood".to_string());
        request.current_stats = Stats {
            vig: 12,
            mnd: 11,
            end: 13,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 45,
        };
        request.character_level = 46;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.locked_combat_stats = [Some(12), Some(15), Some(9), Some(8), Some(45)];
        request.objective = OptimizeObjective::MaxAr;

        let open_results = optimize(&request, &game_data).expect("open optimizer failed");
        assert!(!open_results.is_empty());
        assert_eq!(open_results[0].aow_name.as_deref(), Some("Seppuku"));

        request.aow_name = Some("Seppuku".to_string());
        let locked_results = optimize(&request, &game_data).expect("locked optimizer failed");
        assert!(!locked_results.is_empty());
        assert!(
            (open_results[0].score - locked_results[0].score).abs() < 0.001,
            "expected unlocked Max AR score {} to match Seppuku score {}",
            open_results[0].score,
            locked_results[0].score
        );
    }

    #[test]
    fn open_max_ar_plus_bleed_matches_best_explicit_aow() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Uchigatana".to_string());
        request.affinity = Some("Keen".to_string());
        request.current_stats = Stats {
            vig: 40,
            mnd: 11,
            end: 20,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 8,
        };
        request.character_level = 112;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.locked_combat_stats = [Some(18), Some(40), Some(9), Some(8), Some(45)];
        request.objective = OptimizeObjective::MaxArPlusBleed;

        let open_results = optimize(&request, &game_data).expect("open optimizer failed");
        assert!(!open_results.is_empty());

        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
            .expect("missing keen uchigatana");
        let mut expected_best: Option<OptimizeResult> = None;

        for aow in game_data
            .aows
            .iter()
            .filter(|aow| game_data.aow_compatible_with_weapon(aow, weapon))
        {
            request.aow_name = Some(aow.name.clone());
            let locked_results = optimize(&request, &game_data)
                .unwrap_or_else(|_| panic!("locked optimizer failed for {}", aow.name));
            if let Some(best_row) = locked_results.first()
                && expected_best
                    .as_ref()
                    .map(|expected| better_result(best_row, expected))
                    .unwrap_or(true)
            {
                expected_best = Some(best_row.clone());
            }
        }

        let expected_best =
            expected_best.expect("expected at least one compatible AoW for Keen Uchigatana");
        assert!(
            (open_results[0].score - expected_best.score).abs() < 0.001,
            "expected unlocked Max AR + Bleed score {} to match best explicit score {}",
            open_results[0].score,
            expected_best.score
        );
        assert!(
            (open_results[0].ar.total() - expected_best.ar.total()).abs() < 0.001,
            "expected equal-bleed unlocked Max AR + Bleed AR {} to match best explicit AR {}",
            open_results[0].ar.total(),
            expected_best.ar.total()
        );
        assert!(open_results[0].aow_name.is_some());
    }

    #[test]
    fn aow_variant_rows_match_weapon_type() {
        let game_data = load_data();
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
            .expect("missing keen uchigatana");
        let sword_dance = game_data
            .aows
            .iter()
            .find(|aow| aow.name == "Sword Dance")
            .expect("missing sword dance");
        let rows = select_aow_attack_rows(sword_dance.aow_id, weapon, &game_data);
        assert!(!rows.is_empty());
        assert!(
            rows.iter().all(
                |row| row.variant_weapon_type.is_empty() || row.variant_weapon_type == "Katana"
            )
        );
        assert!(
            rows.iter()
                .any(|row| row.raw_name.starts_with("[Katana] Sword Dance"))
        );
    }

    #[test]
    fn lion_claw_resolves_aow_choice_for_claymore() {
        let game_data = load_data();
        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Claymore" && weapon.affinity == "Standard")
            .expect("missing claymore");
        let mut request = base_request();
        request.weapon_name = Some("Claymore".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = Some("Lion's Claw".to_string());
        request.objective = OptimizeObjective::AowFirstHit;
        request.current_stats = Stats {
            vig: 20,
            mnd: 15,
            end: 20,
            str: 40,
            dex: 30,
            int: 10,
            fai: 10,
            arc: 10,
        };
        request.character_level = 76;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.locked_combat_stats = [Some(40), Some(30), Some(10), Some(10), Some(10)];

        let choices = resolve_aow_choices(weapon, &request, &game_data).expect("resolve failed");
        let choices = choices.expect("expected choices");
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].skill_name, Some("Lion's Claw"));
        assert!(
            !choices[0].attack_rows.is_empty(),
            "expected Lion's Claw attack rows for Claymore"
        );
    }

    #[test]
    fn beasts_roar_first_hit_uses_first_positive_damage_row() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Antspur Rapier".to_string());
        request.affinity = Some("Blood".to_string());
        request.aow_name = Some("Beast's Roar".to_string());
        request.objective = OptimizeObjective::AowFirstHit;
        request.current_stats = Stats {
            vig: 20,
            mnd: 20,
            end: 20,
            str: 60,
            dex: 60,
            int: 60,
            fai: 60,
            arc: 60,
        };
        request.character_level = 331;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(results[0].aow_first_hit_damage > 0.0);
        assert!(results[0].aow_full_sequence_damage >= results[0].aow_first_hit_damage);
    }

    #[test]
    fn zero_damage_roar_has_no_results_for_damage_objective() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Bandit's Curved Sword".to_string());
        request.affinity = Some("Blood".to_string());
        request.aow_name = Some("Braggart's Roar".to_string());
        request.objective = OptimizeObjective::AowFirstHit;
        request.current_stats = Stats {
            vig: 20,
            mnd: 20,
            end: 20,
            str: 60,
            dex: 60,
            int: 60,
            fai: 60,
            arc: 60,
        };
        request.character_level = 331;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(results.is_empty());
    }

    #[test]
    fn spinning_slash_placeholder_variants_match_greatsword() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Bastard Sword".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = Some("Spinning Slash".to_string());
        request.objective = OptimizeObjective::AowFirstHit;
        request.current_stats = Stats {
            vig: 20,
            mnd: 20,
            end: 20,
            str: 60,
            dex: 60,
            int: 60,
            fai: 60,
            arc: 60,
        };
        request.character_level = 331;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 25;
        request.somber_max_upgrade = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(results[0].aow_first_hit_damage > 0.0);
    }

    #[test]
    fn somber_weapons_do_not_accept_generic_ashes_of_war() {
        let game_data = load_data();
        let halo_scythe = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Halo Scythe" && weapon.affinity == "Standard")
            .expect("missing halo scythe");
        let sword_dance = game_data
            .aows
            .iter()
            .find(|aow| aow.name == "Sword Dance")
            .expect("missing sword dance");

        assert!(halo_scythe.disable_gem_attr);
        assert!(!game_data.aow_compatible_with_weapon(sword_dance, halo_scythe));
    }

    #[test]
    fn somber_weapon_native_skill_damage_is_loaded_and_scored() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Halo Scythe".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = None;
        request.objective = OptimizeObjective::AowFirstHit;
        request.current_stats = Stats {
            vig: 40,
            mnd: 11,
            end: 20,
            str: 16,
            dex: 16,
            int: 9,
            fai: 45,
            arc: 8,
        };
        request.character_level = 88;
        request.standard_max_upgrade = 10;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 10;
        request.somber_max_upgrade = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert_eq!(
            results[0].aow_name.as_deref(),
            Some("Miquella's Ring of Light")
        );
        assert!(results[0].aow_first_hit_damage > 0.0);
        assert!(results[0].aow_full_sequence_damage >= results[0].aow_first_hit_damage);
        assert_eq!(results[0].score, results[0].aow_first_hit_damage);
    }

    #[test]
    fn somber_weapon_max_ar_keeps_native_skill_metrics() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Halo Scythe".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = None;
        request.objective = OptimizeObjective::MaxAr;
        request.current_stats = Stats {
            vig: 40,
            mnd: 11,
            end: 20,
            str: 18,
            dex: 40,
            int: 9,
            fai: 26,
            arc: 45,
        };
        request.character_level = 150;
        request.standard_max_upgrade = 10;
        request.somber_max_upgrade = 10;
        request.exact_upgrade = true;
        request.standard_max_upgrade = 10;
        request.somber_max_upgrade = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert_eq!(
            results[0].aow_name.as_deref(),
            Some("Miquella's Ring of Light")
        );
        assert!(results[0].aow_first_hit_damage > 0.0);
        assert!(results[0].aow_full_sequence_damage > 0.0);
    }

    #[test]
    fn utility_aow_has_no_results_for_aow_damage_objective() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Buckler".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = Some("Parry".to_string());
        request.objective = OptimizeObjective::AowFirstHit;
        request.standard_max_upgrade = 0;
        request.somber_max_upgrade = 0;
        request.exact_upgrade = true;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(results.is_empty());
    }
}
