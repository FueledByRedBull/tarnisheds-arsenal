use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Instant;

use crate::math::{
    calculate_ar, class_by_name, compute_free_points, effective_str, meets_requirements,
};
use crate::model::{
    Aow, DamageBreakdown, GameData, Stats, Weapon, COMBAT_STAT_COUNT, STAT_ARC, STAT_DEX,
    STAT_FAI, STAT_INT, STAT_STR,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizeObjective {
    MaxAr,
    MaxArPlusBleed,
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
    pub bleed_buildup_add: f32,
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

#[derive(Clone, Copy, Debug)]
struct AowChoice<'a> {
    aow: Option<&'a Aow>,
    bleed_buildup_add: f32,
}

#[derive(Clone, Copy, Debug)]
struct CombatConstraints {
    mins: [u8; COMBAT_STAT_COUNT],
    maxs: [u8; COMBAT_STAT_COUNT],
    remaining_free: u16,
}

#[derive(Clone, Copy, Debug)]
struct PreparedWeapon<'a> {
    weapon: &'a Weapon,
    aow_choice: AowChoice<'a>,
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
        .map(|entry| entry.upgrades_len as u64)
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
        .map(|entry| entry.upgrades_len as u64)
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
        for combat in &stat_candidates {
            let mut stats = request.current_stats;
            stats.str = combat[STAT_STR];
            stats.dex = combat[STAT_DEX];
            stats.int = combat[STAT_INT];
            stats.fai = combat[STAT_FAI];
            stats.arc = combat[STAT_ARC];

            let effective_str_value = effective_str(stats.str, request.two_handing);

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
                let ar = calculate_ar(prepared.weapon, *upgrade, &stats, effective_str_value, data)?;
                let score = score_for(request.objective, ar.total(), prepared.aow_choice.bleed_buildup_add);
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
                        aow_id: prepared.aow_choice.aow.map(|aow| aow.aow_id),
                        aow_name: prepared.aow_choice.aow.map(|aow| aow.name.clone()),
                        bleed_buildup_add: prepared.aow_choice.bleed_buildup_add,
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
        let Some(aow_choice) = resolve_aow_choice(weapon, request, data)? else {
            continue;
        };
        out.push(PreparedWeapon {
            weapon,
            aow_choice,
            upgrades_len,
            upgrades,
        });
    }
    Ok(out)
}

fn score_for(objective: OptimizeObjective, total_ar: f32, bleed_buildup_add: f32) -> f32 {
    match objective {
        OptimizeObjective::MaxAr => total_ar,
        OptimizeObjective::MaxArPlusBleed => total_ar + bleed_buildup_add,
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

fn resolve_aow_choice<'a>(
    weapon: &Weapon,
    request: &OptimizeRequest,
    data: &'a GameData,
) -> Result<Option<AowChoice<'a>>, String> {
    let no_aow = AowChoice {
        aow: None,
        bleed_buildup_add: 0.0,
    };

    if let Some(lock_aow_name) = request.aow_name.as_deref() {
        let Some(aow) = data
            .aows
            .iter()
            .find(|value| value.name.eq_ignore_ascii_case(lock_aow_name))
        else {
            return Err(format!("unknown AoW: {lock_aow_name}"));
        };
        if !aow_compatible_with_weapon(aow, weapon) {
            return Ok(None);
        }
        return Ok(Some(AowChoice {
            aow: Some(aow),
            bleed_buildup_add: aow.bleed_buildup_add,
        }));
    }

    if request.objective == OptimizeObjective::MaxAr {
        return Ok(Some(no_aow));
    }

    let best = data
        .aows
        .iter()
        .filter(|aow| aow_compatible_with_weapon(aow, weapon))
        .max_by(|left, right| {
            left.bleed_buildup_add
                .partial_cmp(&right.bleed_buildup_add)
                .unwrap_or(Ordering::Equal)
        });

    if let Some(aow) = best {
        if aow.bleed_buildup_add > 0.0 {
            return Ok(Some(AowChoice {
                aow: Some(aow),
                bleed_buildup_add: aow.bleed_buildup_add,
            }));
        }
    }

    Ok(Some(no_aow))
}

fn aow_compatible_with_weapon(aow: &Aow, weapon: &Weapon) -> bool {
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

    if left.bleed_buildup_add > right.bleed_buildup_add {
        return true;
    }
    if left.bleed_buildup_add < right.bleed_buildup_add {
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
}
