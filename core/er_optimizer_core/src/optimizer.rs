use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Instant;

use crate::math::{
    calculate_aow_damage, calculate_ar, class_by_name, compute_free_points, effective_str,
    meets_requirements,
};
use crate::model::{
    Aow, AowAttackRow, DamageBreakdown, DamageType, GameData, Stats, StatusBuildup, Weapon,
    COMBAT_STAT_COUNT, STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizeObjective {
    MaxAr,
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
    pub weapon_name: Option<String>,
    pub affinity: Option<String>,
    pub aow_name: Option<String>,
    pub weapon_type_key: Option<String>,
    pub somber_filter: SomberFilter,
    pub objective: OptimizeObjective,
    pub top_k: usize,
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
    upgrades_len: usize,
    upgrades: [u8; 26],
}

impl<'a> PreparedWeapon<'a> {
    fn upgrades(&self) -> &[u8] {
        &self.upgrades[..self.upgrades_len]
    }
}

pub fn estimate_search_space(
    request: &OptimizeRequest,
    data: &GameData,
) -> Result<SearchEstimate, String> {
    let constraints = build_combat_constraints(request)?;
    let stat_candidates = count_stat_candidates(constraints);
    let weapons = prepare_weapons(request, data)?;
    let upgrade_slots: u64 = weapons
        .iter()
        .map(|entry| (entry.upgrades_len * entry.aow_choices.len()) as u64)
        .sum();
    Ok(SearchEstimate {
        weapon_candidates: weapons.len(),
        stat_candidates,
        combinations: stat_candidates.saturating_mul(upgrade_slots),
    })
}

pub fn optimize(request: &OptimizeRequest, data: &GameData) -> Result<Vec<OptimizeResult>, String> {
    optimize_with_progress(request, data, 0, |_snapshot| {})
}

pub fn optimize_with_progress<F>(
    request: &OptimizeRequest,
    data: &GameData,
    progress_every: u64,
    mut progress_cb: F,
) -> Result<Vec<OptimizeResult>, String>
where
    F: FnMut(ProgressSnapshot),
{
    if request.top_k == 0 {
        return Ok(Vec::new());
    }

    let constraints = build_combat_constraints(request)?;
    let stat_candidates = generate_stat_candidates(constraints);
    if stat_candidates.is_empty() {
        return Ok(Vec::new());
    }

    let weapons = prepare_weapons(request, data)?;
    if weapons.is_empty() {
        return Ok(Vec::new());
    }

    let stat_count = stat_candidates.len() as u64;
    let upgrade_slots: u64 = weapons
        .iter()
        .map(|entry| (entry.upgrades_len * entry.aow_choices.len()) as u64)
        .sum();
    let total = stat_count.saturating_mul(upgrade_slots);
    let emit_every = if progress_every == 0 { 0 } else { progress_every };

    let started = Instant::now();
    let mut checked: u64 = 0;
    let mut eligible: u64 = 0;
    let mut best_score = 0.0_f32;
    let mut has_best = false;
    let mut results: Vec<OptimizeResult> = Vec::with_capacity(request.top_k);

    progress_cb(ProgressSnapshot {
        checked,
        total,
        eligible,
        best_score,
        elapsed_ms: 0,
    });

    for prepared in &weapons {
        for aow_choice in &prepared.aow_choices {
            for combat in &stat_candidates {
                let mut stats = request.current_stats;
                stats.str = combat[STAT_STR];
                stats.dex = combat[STAT_DEX];
                stats.int = combat[STAT_INT];
                stats.fai = combat[STAT_FAI];
                stats.arc = combat[STAT_ARC];

                let effective_str_value = effective_str(stats.str, request.two_handing);
                if combat_has_wasted_points(request, prepared.weapon, data, combat) {
                    let previous_checked = checked;
                    checked = checked.saturating_add(prepared.upgrades_len as u64);
                    if emit_every > 0 && checked / emit_every != previous_checked / emit_every {
                        progress_cb(ProgressSnapshot {
                            checked,
                            total,
                            eligible,
                            best_score,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        });
                    }
                    continue;
                }

                for upgrade in prepared.upgrades() {
                    checked += 1;
                    if !meets_requirements(prepared.weapon, effective_str_value, &stats) {
                        if emit_every > 0 && checked % emit_every == 0 {
                            progress_cb(ProgressSnapshot {
                                checked,
                                total,
                                eligible,
                                best_score,
                                elapsed_ms: started.elapsed().as_millis() as u64,
                            });
                        }
                        continue;
                    }

                    eligible += 1;
                    let ar =
                        calculate_ar(prepared.weapon, *upgrade, &stats, effective_str_value, data)?;
                    let status_buildup = data
                        .weapon_passive(prepared.weapon.weapon_id)
                        .with_aow_additions(aow_choice.aow);
                    let (aow_first_hit_damage, aow_full_sequence_damage) = if aow_choice.attack_rows.is_empty()
                    {
                        (0.0, 0.0)
                    } else {
                        calculate_aow_damage(
                            prepared.weapon,
                            &aow_choice.attack_rows,
                            *upgrade,
                            &stats,
                            effective_str_value,
                            data,
                        )?
                    };
                    let score = score_for(
                        request.objective,
                        ar.total(),
                        status_buildup,
                        aow_first_hit_damage,
                        aow_full_sequence_damage,
                    );
                    if !has_best || score > best_score {
                        best_score = score;
                        has_best = true;
                    }
                    push_top_k(
                        &mut results,
                        OptimizeResult {
                            weapon_id: prepared.weapon.weapon_id,
                            weapon_name: prepared.weapon.name.clone(),
                            affinity: prepared.weapon.affinity.clone(),
                            is_somber: prepared.weapon.is_somber,
                            upgrade: *upgrade,
                            stats,
                            ar,
                            aow_id: aow_choice.aow.map(|aow| aow.aow_id),
                            aow_name: aow_choice.aow.map(|aow| aow.name.clone()),
                            bleed_buildup: status_buildup.bleed,
                            bleed_buildup_add: aow_choice
                                .aow
                                .map(|aow| aow.bleed_buildup_add)
                                .unwrap_or(0.0),
                            frost_buildup: status_buildup.frost,
                            poison_buildup: status_buildup.poison,
                            aow_first_hit_damage,
                            aow_full_sequence_damage,
                            score,
                        },
                        request.top_k,
                    );

                    if emit_every > 0 && checked % emit_every == 0 {
                        progress_cb(ProgressSnapshot {
                            checked,
                            total,
                            eligible,
                            best_score,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        });
                    }
                }
            }
        }
    }

    progress_cb(ProgressSnapshot {
        checked,
        total,
        eligible,
        best_score,
        elapsed_ms: started.elapsed().as_millis() as u64,
    });

    Ok(results)
}

fn build_combat_constraints(request: &OptimizeRequest) -> Result<CombatConstraints, String> {
    let class_info = class_by_name(&request.class_name)
        .ok_or_else(|| format!("unknown starting class: {}", request.class_name))?;
    let free_points = compute_free_points(class_info, request.character_level, &request.current_stats)?;
    let current = request.current_stats.combat_array();

    let mut mins = [0_u8; COMBAT_STAT_COUNT];
    let mut mandatory_raise: u16 = 0;
    for idx in 0..COMBAT_STAT_COUNT {
        mins[idx] = current[idx].max(request.min_combat_stats[idx]);
        mandatory_raise += u16::from(mins[idx] - current[idx]);
    }
    if mandatory_raise > free_points {
        return Err("combat stat floors exceed free point budget".to_string());
    }

    let mut maxs = [99_u8; COMBAT_STAT_COUNT];
    for idx in 0..COMBAT_STAT_COUNT {
        if let Some(locked) = request.locked_combat_stats[idx] {
            if locked < mins[idx] {
                return Err(format!(
                    "locked combat stat {} is below minimum floor {}",
                    idx, mins[idx]
                ));
            }
            maxs[idx] = locked;
        }
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

fn prepare_weapons<'a>(
    request: &OptimizeRequest,
    data: &'a GameData,
) -> Result<Vec<PreparedWeapon<'a>>, String> {
    let mut out = Vec::new();
    for weapon in data.weapons.iter().filter(|entry| weapon_matches_request(entry, request)) {
        let Some((upgrades, upgrades_len)) = available_upgrades(weapon, request, data) else {
            continue;
        };
        let Some(aow_choices) = resolve_aow_choices(weapon, request, data)? else {
            continue;
        };
        out.push(PreparedWeapon {
            weapon,
            aow_choices,
            upgrades_len,
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
        OptimizeObjective::MaxArPlusBleed => total_ar + status_buildup.bleed,
        OptimizeObjective::AowFirstHit => aow_first_hit_damage,
        OptimizeObjective::AowFullSequence => aow_full_sequence_damage,
    }
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
    weapon
        .weapon_type_keys
        .split('|')
        .any(|key| key.eq_ignore_ascii_case(type_key))
}

fn available_upgrades(
    weapon: &Weapon,
    request: &OptimizeRequest,
    data: &GameData,
) -> Option<([u8; 26], usize)> {
    let levels = data.reinforce.get(usize::from(weapon.reinforce_type))?;
    if levels.is_empty() {
        return None;
    }

    let available_max = (levels.len() - 1) as u8;
    let upper = available_max.min(request.max_upgrade);
    if let Some(fixed) = request.fixed_upgrade {
        if fixed > upper {
            return None;
        }
        let mut out = [0_u8; 26];
        out[0] = fixed;
        return Some((out, 1));
    }

    let mut out = [0_u8; 26];
    let mut len = 0usize;
    for level in 0..=upper {
        out[len] = level;
        len += 1;
    }
    Some((out, len))
}

fn resolve_aow_choices<'a>(
    weapon: &Weapon,
    request: &OptimizeRequest,
    data: &'a GameData,
) -> Result<Option<Vec<AowChoice<'a>>>, String> {
    let no_aow = AowChoice {
        aow: None,
        attack_rows: Vec::new(),
    };

    if let Some(lock_aow_name) = request.aow_name.as_deref() {
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

    if request.objective == OptimizeObjective::MaxAr {
        return Ok(Some(vec![no_aow]));
    }

    if request.objective == OptimizeObjective::MaxArPlusBleed {
        let best = data
            .aows
            .iter()
            .filter(|aow| aow_compatible_with_weapon(aow, weapon, data))
            .max_by(|left, right| {
                left.bleed_buildup_add
                    .partial_cmp(&right.bleed_buildup_add)
                    .unwrap_or(Ordering::Equal)
            });
        if let Some(aow) = best {
            if aow.bleed_buildup_add > 0.0 {
                return Ok(Some(vec![build_aow_choice(aow, weapon, data)]));
            }
        }
        return Ok(Some(vec![no_aow]));
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
        attack_rows: select_aow_attack_rows(aow.aow_id, weapon, data),
    }
}

fn select_aow_attack_rows<'a>(
    aow_id: u16,
    weapon: &Weapon,
    data: &'a GameData,
) -> Vec<&'a AowAttackRow> {
    let rows = data.aow_attack_rows(aow_id);
    if rows.is_empty() {
        return Vec::new();
    }

    let has_variant_match = rows.iter().any(|row| {
        !row.variant_weapon_type.is_empty()
            && variant_weapon_type_matches(&row.variant_weapon_type, &weapon.weapon_type_name)
    });
    rows.iter()
        .filter(|row| {
            if has_variant_match {
                variant_weapon_type_matches(&row.variant_weapon_type, &weapon.weapon_type_name)
            } else {
                row.variant_weapon_type.is_empty()
            }
        })
        .collect()
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

pub(crate) fn aow_compatible_with_weapon(aow: &Aow, weapon: &Weapon, data: &GameData) -> bool {
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
    weapon: &Weapon,
    data: &GameData,
    combat: &[u8; COMBAT_STAT_COUNT],
) -> bool {
    let contributing_stats: [bool; COMBAT_STAT_COUNT] = std::array::from_fn(|idx| {
        weapon_stat_can_increase_ar(weapon, data, idx)
    });
    if !contributing_stats
        .iter()
        .enumerate()
        .any(|(idx, contributes)| *contributes && combat[idx] < 99)
    {
        return false;
    }

    for idx in 0..COMBAT_STAT_COUNT {
        if contributing_stats[idx] {
            continue;
        }
        if combat[idx] > minimum_useful_stat(request, weapon, idx) {
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
        minimum_str_for_requirement(weapon.requirements[STAT_STR], request.two_handing)
    } else {
        weapon.requirements[stat_idx]
    };
    floor.max(locked).max(required)
}

fn minimum_str_for_requirement(requirement: u8, two_handing: bool) -> u8 {
    if !two_handing {
        return requirement;
    }
    for candidate in 0..=requirement {
        if effective_str(candidate, true) >= requirement {
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

fn generate_stat_candidates(constraints: CombatConstraints) -> Vec<[u8; COMBAT_STAT_COUNT]> {
    let total = count_stat_candidates(constraints);
    let mut out = Vec::with_capacity(total.min(usize::MAX as u64) as usize);
    let mut current = constraints.mins;
    expand_candidates(
        0,
        constraints.remaining_free,
        &constraints.mins,
        &constraints.maxs,
        &mut current,
        &mut out,
    );
    out
}

fn expand_candidates(
    idx: usize,
    remaining: u16,
    mins: &[u8; COMBAT_STAT_COUNT],
    maxs: &[u8; COMBAT_STAT_COUNT],
    current: &mut [u8; COMBAT_STAT_COUNT],
    out: &mut Vec<[u8; COMBAT_STAT_COUNT]>,
) {
    if idx == COMBAT_STAT_COUNT {
        if remaining == 0 {
            out.push(*current);
        }
        return;
    }

    let cap = u16::from(maxs[idx] - mins[idx]);
    let max_add = cap.min(remaining);
    for add in 0..=max_add {
        current[idx] = mins[idx] + (add as u8);
        expand_candidates(idx + 1, remaining - add, mins, maxs, current, out);
    }
    current[idx] = mins[idx];
}

fn push_top_k(results: &mut Vec<OptimizeResult>, candidate: OptimizeResult, top_k: usize) {
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
        assert!(results.windows(2).all(|pair| pair[0].score >= pair[1].score));
        assert!(results.iter().all(|result| result.weapon_name == "Uchigatana"));
        assert!(results.iter().all(|result| result.affinity == "Keen"));
        assert!(results.iter().all(|result| result.upgrade <= 25));
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
        request.weapon_type_key = Some("katana".to_string());
        request.top_k = 10;

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        for result in &results {
            let weapon = game_data
                .weapons
                .iter()
                .find(|weapon| weapon.weapon_id == result.weapon_id && weapon.affinity == result.affinity)
                .expect("missing weapon");
            assert!(weapon
                .weapon_type_keys
                .split('|')
                .any(|key| key.eq_ignore_ascii_case("katana")));
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

        assert!(combat_has_wasted_points(
            &request,
            weapon,
            &game_data,
            &[21, 15, 40, 9, 8],
        ));
        assert!(!combat_has_wasted_points(
            &request,
            weapon,
            &game_data,
            &[21, 15, 41, 8, 8],
        ));

        let results = optimize(&request, &game_data).expect("optimizer failed");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|row| row.stats.fai == 8 && row.stats.arc == 8));
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
        assert!(rows
            .iter()
            .all(|row| row.variant_weapon_type.is_empty() || row.variant_weapon_type == "Katana"));
        assert!(rows
            .iter()
            .any(|row| row.raw_name.starts_with("[Katana] Sword Dance")));
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
