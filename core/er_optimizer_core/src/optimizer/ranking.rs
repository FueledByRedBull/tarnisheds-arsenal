use std::cmp::Ordering as CmpOrdering;

use super::*;

pub(super) fn could_enter_scored_top_k(
    results: &[ScoredCandidate],
    candidate: &ScoredCandidate,
    top_k: usize,
) -> bool {
    results.len() < top_k
        || results
            .last()
            .is_none_or(|worst| candidate.key >= worst.key)
}

pub(super) fn merge_scored_top_k(
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

pub(super) fn push_scored_top_k(
    results: &mut Vec<ScoredCandidate>,
    candidate: ScoredCandidate,
    weapons: &[PreparedWeapon<'_>],
    group_mode: ResultGroupMode,
    top_k: usize,
) {
    if top_k == 0 {
        return;
    }

    if let Some(existing_idx) = results
        .iter()
        .position(|existing| same_scored_result_group(&candidate, existing, weapons, group_mode))
    {
        if compare_scored_candidates(&candidate, &results[existing_idx], weapons)
            != CmpOrdering::Greater
        {
            return;
        }
        results.remove(existing_idx);
    }

    let insert_at = results
        .iter()
        .position(|existing| {
            compare_scored_candidates(&candidate, existing, weapons) == CmpOrdering::Greater
        })
        .unwrap_or(results.len());
    results.insert(insert_at, candidate);

    results.truncate(top_k);
}

fn compare_scored_candidates(
    left: &ScoredCandidate,
    right: &ScoredCandidate,
    weapons: &[PreparedWeapon<'_>],
) -> CmpOrdering {
    let metric_order = left.key.cmp(&right.key);
    if metric_order != CmpOrdering::Equal {
        return metric_order;
    }

    let left_weapon = &weapons[left.prepared_idx];
    let right_weapon = &weapons[right.prepared_idx];
    let weapon_order = right_weapon
        .weapon
        .weapon_id
        .cmp(&left_weapon.weapon.weapon_id);
    if weapon_order != CmpOrdering::Equal {
        return weapon_order;
    }
    let upgrade_order = left.upgrade.cmp(&right.upgrade);
    if upgrade_order != CmpOrdering::Equal {
        return upgrade_order;
    }
    let left_skill = left_weapon.aow_choices[left.aow_idx].skill_id;
    let right_skill = right_weapon.aow_choices[right.aow_idx].skill_id;
    let skill_order = right_skill.cmp(&left_skill);
    if skill_order != CmpOrdering::Equal {
        return skill_order;
    }

    // Match the final result ordering; internal indices only stabilize identical rows.
    right
        .stats
        .combat_array()
        .cmp(&left.stats.combat_array())
        .then_with(|| right.prepared_idx.cmp(&left.prepared_idx))
        .then_with(|| right.aow_idx.cmp(&left.aow_idx))
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

pub(super) fn push_top_k(
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

pub(super) fn better_result(left: &OptimizeResult, right: &OptimizeResult) -> bool {
    if left.exact_key != right.exact_key {
        return left.exact_key > right.exact_key;
    }

    if left.weapon_id != right.weapon_id {
        return left.weapon_id < right.weapon_id;
    }
    if left.upgrade != right.upgrade {
        return left.upgrade > right.upgrade;
    }
    left.aow_id < right.aow_id
        || left.aow_id == right.aow_id && left.stats.combat_array() < right.stats.combat_array()
}
