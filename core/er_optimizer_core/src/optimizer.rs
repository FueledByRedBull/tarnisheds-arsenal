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
    STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, StatusBuildup, Weapon,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizeObjective {
    MaxAr,
    MaxPhysicalAr,
    MaxArPlusBleed,
    AowFirstHit,
    AowFullSequence,
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
    pub max_upgrade: u8,
    pub fixed_upgrade: Option<u8>,
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
    useful_stats: [bool; COMBAT_STAT_COUNT],
    minimum_useful_stats: [u8; COMBAT_STAT_COUNT],
}

#[derive(Clone, Copy, Debug)]
struct CandidateMetric {
    score: f32,
    ar: Option<DamageBreakdown>,
    status_buildup: Option<StatusBuildup>,
    aow_first_hit_damage: Option<f32>,
    aow_full_sequence_damage: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct SearchWorkUnit {
    prepared_idx: usize,
    aow_start: usize,
    aow_end: usize,
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

const PARALLEL_SEARCH_MIN_COMBINATIONS: u64 = 50_000_000;
const PARALLEL_AOW_CHUNK_SIZE: usize = 8;
const PARALLEL_PROGRESS_BATCH: u64 = 8_192;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);

pub fn estimate_search_space(
    request: &OptimizeRequest,
    data: &GameData,
) -> Result<SearchEstimate, String> {
    let constraints = build_combat_constraints(request)?;
    let stat_candidates = count_stat_candidates(constraints);
    let weapons = prepare_weapons(request, data)?;
    let upgrade_slots: u64 = weapons
        .iter()
        .map(|entry| (entry.upgrades.len() * entry.aow_choices.len()) as u64)
        .sum();
    Ok(SearchEstimate {
        weapon_candidates: weapons.len(),
        stat_candidates,
        combinations: stat_candidates.saturating_mul(upgrade_slots),
    })
}

pub fn optimize(request: &OptimizeRequest, data: &GameData) -> Result<Vec<OptimizeResult>, String> {
    optimize_with_progress(request, data, 0, |_snapshot| true)
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

    let constraints = build_combat_constraints(request)?;
    let stat_count = count_stat_candidates(constraints);
    if stat_count == 0 {
        return Ok(Vec::new());
    }

    let weapons = prepare_weapons(request, data)?;
    if weapons.is_empty() {
        return Ok(Vec::new());
    }

    let upgrade_slots: u64 = weapons
        .iter()
        .map(|entry| (entry.upgrades.len() * entry.aow_choices.len()) as u64)
        .sum();
    let total = stat_count.saturating_mul(upgrade_slots);
    let work_units = build_search_work_units(&weapons);

    if should_use_parallel_search(total, work_units.len()) {
        optimize_parallel(
            request,
            data,
            constraints,
            &weapons,
            &work_units,
            total,
            progress_every,
            progress_cb,
        )
    } else {
        let mut progress = SerialSearchProgress::new(total, progress_every, progress_cb);
        optimize_serial(
            request,
            data,
            constraints,
            &weapons,
            &work_units,
            &mut progress,
        )
    }
}

fn should_use_parallel_search(total: u64, work_unit_count: usize) -> bool {
    work_unit_count >= 2
        && total >= PARALLEL_SEARCH_MIN_COMBINATIONS
        && rayon::current_num_threads() > 1
}

fn build_search_work_units(weapons: &[PreparedWeapon<'_>]) -> Vec<SearchWorkUnit> {
    let mut units = Vec::new();
    for (prepared_idx, prepared) in weapons.iter().enumerate() {
        if prepared.aow_choices.len() <= PARALLEL_AOW_CHUNK_SIZE {
            units.push(SearchWorkUnit {
                prepared_idx,
                aow_start: 0,
                aow_end: prepared.aow_choices.len(),
            });
            continue;
        }
        for aow_start in (0..prepared.aow_choices.len()).step_by(PARALLEL_AOW_CHUNK_SIZE) {
            units.push(SearchWorkUnit {
                prepared_idx,
                aow_start,
                aow_end: (aow_start + PARALLEL_AOW_CHUNK_SIZE).min(prepared.aow_choices.len()),
            });
        }
    }
    units
}

fn optimize_serial<F>(
    request: &OptimizeRequest,
    data: &GameData,
    constraints: CombatConstraints,
    weapons: &[PreparedWeapon<'_>],
    work_units: &[SearchWorkUnit],
    progress: &mut SerialSearchProgress<F>,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    progress.emit_initial()?;
    let mut results = Vec::with_capacity(request.top_k);
    for unit in work_units {
        let mut unit_results =
            search_work_unit(request, data, constraints, weapons, *unit, progress)?;
        merge_top_k(&mut results, unit_results.drain(..), request.top_k);
    }
    progress.emit_final()?;
    Ok(results)
}

fn optimize_parallel<F>(
    request: &OptimizeRequest,
    data: &GameData,
    constraints: CombatConstraints,
    weapons: &[PreparedWeapon<'_>],
    work_units: &[SearchWorkUnit],
    total: u64,
    progress_every: u64,
    progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
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
            let result = search_work_unit(
                request,
                data,
                constraints,
                weapons,
                *unit,
                &mut local_progress,
            );
            let finish_result = local_progress.finish();
            match (result, finish_result) {
                (Ok(results), Ok(())) => Ok(results),
                (Err(err), _) | (_, Err(err)) => Err(err),
            }
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut results = Vec::with_capacity(request.top_k);
    for unit_results in partial_results {
        merge_top_k(&mut results, unit_results.into_iter(), request.top_k);
    }
    progress.emit_final()?;
    if progress.is_cancelled() {
        return Err("cancelled".to_string());
    }
    Ok(results)
}

fn search_work_unit<P>(
    request: &OptimizeRequest,
    data: &GameData,
    constraints: CombatConstraints,
    weapons: &[PreparedWeapon<'_>],
    unit: SearchWorkUnit,
    progress: &mut P,
) -> Result<Vec<OptimizeResult>, String>
where
    P: SearchProgress,
{
    let prepared = &weapons[unit.prepared_idx];
    let mut results = Vec::with_capacity(request.top_k);
    let damage_multiplier = request.damage_multiplier();
    let mut current_combat = constraints.mins;
    let mut visit_result: Result<(), String> = Ok(());

    visit_stat_candidates(constraints, &mut current_combat, |combat| {
        if progress.is_cancelled() {
            visit_result = Err("cancelled".to_string());
            return false;
        }
        for aow_choice in &prepared.aow_choices[unit.aow_start..unit.aow_end] {
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
            if combat_has_wasted_points(request, prepared, combat) {
                if let Err(err) = progress.advance(prepared.upgrades.len() as u64, 0, None) {
                    visit_result = Err(err);
                    return false;
                }
                continue;
            }

            for upgrade in &prepared.upgrades {
                if !meets_requirements(prepared.weapon, effective_str_value, &stats) {
                    if let Err(err) = progress.advance(1, 0, None) {
                        visit_result = Err(err);
                        return false;
                    }
                    continue;
                }

                let metric = score_candidate(
                    request.objective,
                    prepared,
                    aow_choice,
                    *upgrade,
                    &stats,
                    effective_str_value,
                    damage_multiplier,
                    data,
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
                if !could_enter_top_k(&results, score, request.top_k) {
                    continue;
                }
                let full = complete_candidate_metric(
                    ar,
                    status_buildup,
                    aow_first_hit_damage,
                    aow_full_sequence_damage,
                    prepared,
                    aow_choice,
                    *upgrade,
                    &stats,
                    effective_str_value,
                    damage_multiplier,
                    data,
                );
                let CandidateMetric {
                    score: _,
                    ar,
                    status_buildup,
                    aow_first_hit_damage,
                    aow_full_sequence_damage,
                } = match full {
                    Ok(metric) => metric,
                    Err(err) => {
                        visit_result = Err(err);
                        return false;
                    }
                };
                push_top_k(
                    &mut results,
                    OptimizeResult {
                        weapon_id: prepared.weapon.weapon_id,
                        weapon_name: prepared.weapon.name.clone(),
                        affinity: prepared.weapon.affinity.clone(),
                        is_somber: prepared.weapon.is_somber,
                        upgrade: *upgrade,
                        stats,
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
                        score,
                    },
                    request.top_k,
                );
            }
        }
        true
    });
    visit_result?;
    progress.finish()?;
    Ok(results)
}

fn merge_top_k(
    results: &mut Vec<OptimizeResult>,
    candidates: impl IntoIterator<Item = OptimizeResult>,
    top_k: usize,
) {
    for candidate in candidates {
        push_top_k(results, candidate, top_k);
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
        if let Some(score) = best_score {
            if self.best_score.is_none_or(|current| score > current) {
                self.best_score = Some(score);
            }
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
        if let Some(score) = best_score {
            if self.best_score.is_none_or(|current| score > current) {
                self.best_score = Some(score);
            }
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
) -> Result<Vec<PreparedWeapon<'a>>, String> {
    let mut out = Vec::new();
    for weapon in data
        .weapons
        .iter()
        .filter(|entry| weapon_matches_request(entry, request))
    {
        let Some(upgrades) = available_upgrades(weapon, request, data) else {
            continue;
        };
        let Some(aow_choices) = resolve_aow_choices(weapon, request, data)? else {
            continue;
        };
        let useful_stats =
            std::array::from_fn(|idx| weapon_stat_can_increase_ar(weapon, data, idx));
        let minimum_useful_stats =
            std::array::from_fn(|idx| minimum_useful_stat(request, weapon, idx));
        out.push(PreparedWeapon {
            weapon,
            aow_choices,
            upgrades,
            useful_stats,
            minimum_useful_stats,
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
        OptimizeObjective::MaxArPlusBleed => total_ar + status_buildup.bleed,
        OptimizeObjective::AowFirstHit => aow_first_hit_damage,
        OptimizeObjective::AowFullSequence => aow_full_sequence_damage,
    }
}

fn score_candidate(
    objective: OptimizeObjective,
    prepared: &PreparedWeapon<'_>,
    aow_choice: &AowChoice<'_>,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    data: &GameData,
) -> Result<CandidateMetric, String> {
    match objective {
        OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr => {
            let ar = calculate_ar_with_buffs(
                prepared,
                aow_choice,
                upgrade,
                stats,
                effective_str_value,
                damage_multiplier,
                data,
            )?;
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
            let ar = calculate_ar_with_buffs(
                prepared,
                aow_choice,
                upgrade,
                stats,
                effective_str_value,
                damage_multiplier,
                data,
            )?;
            let status_buildup =
                calculate_status_with_buffs(prepared, aow_choice, upgrade, stats, data)?;
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
    if aow_choice.attack_rows.is_empty() {
        return Ok((0.0, 0.0));
    }
    let (first, full) = calculate_aow_damage(
        prepared.weapon,
        &aow_choice.attack_rows,
        upgrade,
        stats,
        effective_str_value,
        data,
    )?;
    Ok((first * damage_multiplier, full * damage_multiplier))
}

fn weapon_matches_request(weapon: &Weapon, request: &OptimizeRequest) -> bool {
    if let Some(lock_weapon) = request.weapon_name.as_deref() {
        if !weapon.name.eq_ignore_ascii_case(lock_weapon) {
            return false;
        }
    }
    if let Some(lock_affinity) = request.affinity.as_deref() {
        if !weapon.affinity.eq_ignore_ascii_case(lock_affinity) {
            return false;
        }
    }
    if let Some(type_key) = request.weapon_type_key.as_deref() {
        if !weapon_type_matches(weapon, type_key) {
            return false;
        }
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

    if let Some(fixed) = request.fixed_upgrade {
        return data
            .reinforce_level(weapon.reinforce_type, fixed)
            .is_some()
            .then(|| vec![fixed]);
    }

    let out: Vec<u8> = levels
        .iter()
        .enumerate()
        .filter_map(|(level, value)| {
            (value.is_some() && level <= usize::from(request.max_upgrade)).then_some(level as u8)
        })
        .collect();
    (!out.is_empty()).then_some(out)
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
    let native_skill_choice = native_skill_choice_for_weapon(weapon, data);

    if let Some(lock_aow_name) = request.aow_name.as_deref() {
        if let Some(choice) = native_skill_choice.as_ref() {
            if choice
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
        }
        let compatible_matches: Vec<&Aow> = data
            .aows
            .iter()
            .filter(|value| value.name.eq_ignore_ascii_case(lock_aow_name))
            .filter(|aow| aow_compatible_with_weapon(aow, weapon, data))
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
        let choice = build_aow_choice(aow, weapon, data);
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
        )));
    }

    if let Some(choice) = native_skill_choice {
        if !choice.attack_rows.is_empty() {
            return Ok(Some(vec![choice]));
        }
    }

    let choices: Vec<AowChoice<'a>> = data
        .aows
        .iter()
        .filter(|aow| !aow.name.eq_ignore_ascii_case("No Skill"))
        .filter(|aow| aow_compatible_with_weapon(aow, weapon, data))
        .map(|aow| build_aow_choice(aow, weapon, data))
        .filter(|choice| !choice.attack_rows.is_empty())
        .collect();
    if choices.is_empty() {
        return Ok(None);
    }
    Ok(Some(choices))
}

fn build_aow_choice<'a>(aow: &'a Aow, weapon: &Weapon, data: &'a GameData) -> AowChoice<'a> {
    AowChoice {
        aow: Some(aow),
        skill_id: Some(aow.aow_id),
        skill_name: Some(aow.name.as_str()),
        attack_rows: select_aow_attack_rows(aow.aow_id, weapon, data),
    }
}

fn open_aow_choices_for_objective<'a>(
    weapon: &'a Weapon,
    data: &'a GameData,
    no_aow: AowChoice<'a>,
    native_skill_choice: Option<AowChoice<'a>>,
) -> Vec<AowChoice<'a>> {
    if weapon.disable_gem_attr {
        return native_skill_choice.map_or_else(|| vec![no_aow], |choice| vec![choice]);
    }

    let mut choices = vec![no_aow];
    choices.extend(
        data.aows
            .iter()
            .filter(|aow| !aow.name.eq_ignore_ascii_case("No Skill"))
            .filter(|aow| aow_compatible_with_weapon(aow, weapon, data))
            .map(|aow| build_aow_choice(aow, weapon, data)),
    );
    choices
}

fn native_skill_choice_for_weapon<'a>(
    weapon: &'a Weapon,
    data: &'a GameData,
) -> Option<AowChoice<'a>> {
    if !weapon.disable_gem_attr {
        return None;
    }
    let attack_rows = select_attack_rows(data.native_skill_attack_rows(weapon.weapon_id), weapon);
    let skill_name = weapon
        .native_skill_name
        .as_deref()
        .or_else(|| attack_rows.first().map(|row| row.aow_name.as_str()))?;
    Some(AowChoice {
        aow: None,
        skill_id: weapon
            .native_skill_id
            .or_else(|| attack_rows.first().map(|row| row.aow_id)),
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

pub(crate) fn aow_compatible_with_weapon(aow: &Aow, weapon: &Weapon, data: &GameData) -> bool {
    if weapon.disable_gem_attr {
        return false;
    }
    if let Some(exact_match) = data.exact_aow_compatibility(aow.aow_id, weapon.weapon_id) {
        return exact_match;
    }
    if aow.name.eq_ignore_ascii_case("Seppuku")
        && (weapon.affinity.eq_ignore_ascii_case("Magic")
            || weapon.affinity.eq_ignore_ascii_case("Cold"))
    {
        return false;
    }
    if aow.valid_weapon_types.is_empty() {
        return true;
    }
    if weapon.weapon_type_keys.is_empty() {
        return false;
    }

    for weapon_key in weapon.weapon_type_keys.split('|') {
        if weapon_key.is_empty() {
            continue;
        }
        for valid_key in aow.valid_weapon_types.split('|') {
            if weapon_key == valid_key {
                return true;
            }
        }
    }
    false
}

fn combat_has_wasted_points(
    request: &OptimizeRequest,
    prepared: &PreparedWeapon<'_>,
    combat: &[u8; COMBAT_STAT_COUNT],
) -> bool {
    if !matches!(
        request.objective,
        OptimizeObjective::MaxAr | OptimizeObjective::MaxPhysicalAr
    ) {
        return false;
    }
    if !prepared
        .useful_stats
        .iter()
        .enumerate()
        .any(|(idx, contributes)| *contributes && combat[idx] < 99)
    {
        return false;
    }

    for idx in 0..COMBAT_STAT_COUNT {
        if prepared.useful_stats[idx] {
            continue;
        }
        if combat[idx] > prepared.minimum_useful_stats[idx] {
            return true;
        }
    }
    false
}

fn minimum_useful_stat(request: &OptimizeRequest, weapon: &Weapon, stat_idx: usize) -> u8 {
    let current = request.current_stats.combat_array()[stat_idx];
    let floor = current.max(request.min_combat_stats[stat_idx]);
    let locked = request.locked_combat_stats[stat_idx].unwrap_or(0);
    let required = if stat_idx == STAT_STR {
        minimum_str_for_requirement(
            weapon.requirements[STAT_STR],
            request.two_handing,
            weapon.disable_two_hand_bonus,
        )
    } else {
        weapon.requirements[stat_idx]
    };
    floor.max(locked).max(required)
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

fn count_stat_candidates(constraints: CombatConstraints) -> u64 {
    let mut caps = [0_u8; COMBAT_STAT_COUNT];
    for idx in 0..COMBAT_STAT_COUNT {
        caps[idx] = constraints.maxs[idx] - constraints.mins[idx];
    }
    let mut memo: HashMap<(usize, u16), u64> = HashMap::new();
    count_distributions(&caps, 0, constraints.remaining_free, &mut memo)
}

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

fn visit_stat_candidates<F>(
    constraints: CombatConstraints,
    current: &mut [u8; COMBAT_STAT_COUNT],
    mut visitor: F,
) where
    F: FnMut(&[u8; COMBAT_STAT_COUNT]) -> bool,
{
    visit_stat_candidates_inner(
        0,
        constraints.remaining_free,
        &constraints.mins,
        &constraints.maxs,
        current,
        &mut visitor,
    );
}

fn visit_stat_candidates_inner<F>(
    idx: usize,
    remaining: u16,
    mins: &[u8; COMBAT_STAT_COUNT],
    maxs: &[u8; COMBAT_STAT_COUNT],
    current: &mut [u8; COMBAT_STAT_COUNT],
    visitor: &mut F,
) -> bool
where
    F: FnMut(&[u8; COMBAT_STAT_COUNT]) -> bool,
{
    if idx == COMBAT_STAT_COUNT {
        if remaining == 0 {
            return visitor(current);
        }
        return true;
    }

    let cap = u16::from(maxs[idx] - mins[idx]);
    let max_add = cap.min(remaining);
    for add in 0..=max_add {
        current[idx] = mins[idx] + (add as u8);
        if !visit_stat_candidates_inner(idx + 1, remaining - add, mins, maxs, current, visitor) {
            current[idx] = mins[idx];
            return false;
        }
    }
    current[idx] = mins[idx];
    true
}

fn could_enter_top_k(results: &[OptimizeResult], score: f32, top_k: usize) -> bool {
    if results.len() < top_k {
        return true;
    }
    results.last().is_none_or(|worst| score >= worst.score)
}

fn push_top_k(results: &mut Vec<OptimizeResult>, candidate: OptimizeResult, top_k: usize) {
    if let Some(existing_idx) = results
        .iter()
        .position(|existing| same_result_loadout(&candidate, existing))
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

fn same_result_loadout(left: &OptimizeResult, right: &OptimizeResult) -> bool {
    left.weapon_id == right.weapon_id
        && left.upgrade == right.upgrade
        && left.aow_id == right.aow_id
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
            max_upgrade: 25,
            fixed_upgrade: None,
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
        request.max_upgrade = 25;
        request.fixed_upgrade = Some(25);

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
        base.max_upgrade = 25;
        base.fixed_upgrade = Some(25);
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
        request.fixed_upgrade = Some(25);
        request.top_k = 5;

        let constraints = build_combat_constraints(&request).expect("constraints failed");
        let stat_count = count_stat_candidates(constraints);
        let weapons = prepare_weapons(&request, &game_data).expect("prepare failed");
        let work_units = build_search_work_units(&weapons);
        let upgrade_slots: u64 = weapons
            .iter()
            .map(|entry| (entry.upgrades.len() * entry.aow_choices.len()) as u64)
            .sum();
        let total = stat_count.saturating_mul(upgrade_slots);

        let mut serial_progress = SerialSearchProgress::new(total, 0, |_snapshot| true);
        let serial = optimize_serial(
            &request,
            &game_data,
            constraints,
            &weapons,
            &work_units,
            &mut serial_progress,
        )
        .expect("serial search failed");
        let parallel = optimize_parallel(
            &request,
            &game_data,
            constraints,
            &weapons,
            &work_units,
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
    fn optimize_respects_exact_stat_lock() {
        let game_data = load_data();
        let mut request = base_request();
        request.max_upgrade = 0;
        request.fixed_upgrade = Some(0);
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
        request.max_upgrade = 0;
        request.fixed_upgrade = Some(0);
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
    fn optimize_keeps_one_result_per_weapon_setup() {
        let game_data = load_data();
        let mut request = base_request();
        request.character_level = 148;
        request.weapon_name = Some("Lizard Greatsword".to_string());
        request.affinity = Some("Keen".to_string());
        request.aow_name = Some("Seppuku".to_string());
        request.fixed_upgrade = Some(25);
        request.max_upgrade = 25;
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
        request.fixed_upgrade = None;
        request.max_upgrade = 25;
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
        request.fixed_upgrade = Some(5);
        request.max_upgrade = 25;

        assert!(available_upgrades(&weapon, &request, &game_data).is_none());
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
        one_hand.max_upgrade = 25;
        one_hand.fixed_upgrade = Some(25);
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
        request.max_upgrade = 10;
        request.fixed_upgrade = Some(10);
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
        request.character_level = 84;
        request.fixed_upgrade = Some(25);
        request.max_upgrade = 25;
        request.top_k = 10;
        let prepared_weapons = prepare_weapons(&request, &game_data).expect("prepare failed");
        let prepared = prepared_weapons
            .iter()
            .find(|prepared| prepared.weapon.weapon_id == weapon.weapon_id)
            .expect("missing prepared weapon");

        assert!(combat_has_wasted_points(
            &request,
            prepared,
            &[21, 15, 40, 9, 8],
        ));
        assert!(!combat_has_wasted_points(
            &request,
            prepared,
            &[21, 15, 41, 8, 8],
        ));

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

        assert!(!aow_compatible_with_weapon(seppuku, cold_uchi, &game_data));
        assert!(!aow_compatible_with_weapon(seppuku, fire_uchi, &game_data));
        assert!(aow_compatible_with_weapon(seppuku, blood_uchi, &game_data));
    }

    #[test]
    fn max_ar_plus_bleed_uses_innate_weapon_buildup() {
        let game_data = load_data();
        let mut request = base_request();
        request.weapon_name = Some("Rivers of Blood".to_string());
        request.affinity = Some("Standard".to_string());
        request.aow_name = None;
        request.objective = OptimizeObjective::MaxArPlusBleed;
        request.max_upgrade = 10;
        request.fixed_upgrade = Some(10);
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
        assert_eq!(
            results[0].score,
            results[0].ar.total() + results[0].bleed_buildup
        );
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
        request.fixed_upgrade = Some(25);
        request.max_upgrade = 25;

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
        request.max_upgrade = 25;
        request.fixed_upgrade = Some(25);
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
        request.max_upgrade = 25;
        request.fixed_upgrade = Some(25);
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
        request.max_upgrade = 25;
        request.fixed_upgrade = Some(25);
        request.locked_combat_stats = [Some(18), Some(40), Some(9), Some(8), Some(45)];
        request.objective = OptimizeObjective::MaxArPlusBleed;

        let open_results = optimize(&request, &game_data).expect("open optimizer failed");
        assert!(!open_results.is_empty());

        let weapon = game_data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
            .expect("missing keen uchigatana");
        let mut expected_best = f32::NEG_INFINITY;

        for aow in game_data
            .aows
            .iter()
            .filter(|aow| aow_compatible_with_weapon(aow, weapon, &game_data))
        {
            request.aow_name = Some(aow.name.clone());
            let locked_results = optimize(&request, &game_data)
                .unwrap_or_else(|_| panic!("locked optimizer failed for {}", aow.name));
            if let Some(best_row) = locked_results.first() {
                if best_row.score > expected_best {
                    expected_best = best_row.score;
                }
            }
        }

        assert!(
            expected_best.is_finite(),
            "expected at least one compatible AoW for Keen Uchigatana"
        );
        assert!(
            (open_results[0].score - expected_best).abs() < 0.001,
            "expected unlocked Max AR + Bleed score {} to match best explicit score {}",
            open_results[0].score,
            expected_best
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
        request.fixed_upgrade = Some(25);
        request.max_upgrade = 25;
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
        request.fixed_upgrade = Some(25);
        request.max_upgrade = 25;

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
        request.fixed_upgrade = Some(25);
        request.max_upgrade = 25;

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
        request.fixed_upgrade = Some(25);
        request.max_upgrade = 25;

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
        assert!(!aow_compatible_with_weapon(
            sword_dance,
            halo_scythe,
            &game_data
        ));
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
        request.fixed_upgrade = Some(10);
        request.max_upgrade = 10;

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
        request.fixed_upgrade = Some(10);
        request.max_upgrade = 10;

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
        request.max_upgrade = 0;
        request.fixed_upgrade = Some(0);

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(results.is_empty());
    }
}
