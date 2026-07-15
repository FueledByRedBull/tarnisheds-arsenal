use std::cmp::Ordering as CmpOrdering;

use super::*;

pub(super) fn could_enter_scored_top_k(
    results: &[ScoredCandidate],
    candidate: &ScoredCandidate,
    weapons: &[PreparedWeapon<'_>],
    top_k: usize,
    group_mode: ResultGroupMode,
) -> bool {
    let limit = scored_candidate_limit(top_k, group_mode);
    if results.len() < limit {
        return true;
    }
    results.iter().any(|existing| {
        same_scored_result_group(candidate, existing, weapons, group_mode)
            && compare_known_candidate_metrics(&candidate.metric, &existing.metric)
                != CmpOrdering::Less
    }) || results.last().is_none_or(|worst| {
        compare_known_candidate_metrics(&candidate.metric, &worst.metric) != CmpOrdering::Less
    })
}

pub(super) fn merge_scored_top_k(
    results: &mut Vec<ScoredCandidate>,
    candidates: impl IntoIterator<Item = ScoredCandidate>,
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    group_mode: ResultGroupMode,
    top_k: usize,
) -> Result<(), String> {
    for candidate in candidates {
        push_scored_top_k(
            results, candidate, request, data, weapons, group_mode, top_k,
        )?;
    }
    Ok(())
}

pub(super) fn push_scored_top_k(
    results: &mut Vec<ScoredCandidate>,
    mut candidate: ScoredCandidate,
    request: &OptimizeRequest,
    data: &GameData,
    weapons: &[PreparedWeapon<'_>],
    group_mode: ResultGroupMode,
    top_k: usize,
) -> Result<(), String> {
    if top_k == 0 {
        return Ok(());
    }

    let tied_indices = results
        .iter()
        .enumerate()
        .filter_map(|(index, existing)| {
            (compare_known_candidate_metrics(&candidate.metric, &existing.metric)
                == CmpOrdering::Equal)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if !tied_indices.is_empty() {
        for index in tied_indices {
            complete_scored_candidate_tie_breaks(
                &mut candidate,
                &mut results[index],
                request,
                data,
                weapons,
            )?;
        }
    }

    if let Some(existing_idx) = results
        .iter()
        .position(|existing| same_scored_result_group(&candidate, existing, weapons, group_mode))
    {
        if compare_scored_candidates(&candidate, &results[existing_idx], weapons)
            != CmpOrdering::Greater
        {
            return Ok(());
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

    let limit = scored_candidate_limit(top_k, group_mode);
    results.truncate(limit);
    Ok(())
}

fn compare_scored_candidates(
    left: &ScoredCandidate,
    right: &ScoredCandidate,
    weapons: &[PreparedWeapon<'_>],
) -> CmpOrdering {
    let metric_order = compare_known_candidate_metrics(&left.metric, &right.metric);
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

    // Final result ordering intentionally has no stat tie-break. Make the retained
    // representative stable across serial/parallel merge order by preferring the
    // lexicographically smaller combat-stat allocation.
    right
        .stats
        .combat_array()
        .cmp(&left.stats.combat_array())
        .then_with(|| right.prepared_idx.cmp(&left.prepared_idx))
        .then_with(|| right.aow_idx.cmp(&left.aow_idx))
}

pub(super) fn compare_known_candidate_metrics(
    left: &CandidateMetric,
    right: &CandidateMetric,
) -> CmpOrdering {
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
