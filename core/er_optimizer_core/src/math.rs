use std::collections::{BTreeMap, HashMap};

use crate::model::{
    AowActionResult, AowAttackRow, AowEffectRole, AowHitResult, AowRouteResult,
    AttackElementCorrectExt, COMBAT_STAT_COUNT, DamageBreakdown, DamageType, GameData, STAT_ARC,
    STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, StaminaCostMode, Stats, StatusBuildup,
    StatusCorrectionFlags, Weapon,
};

#[derive(Clone, Copy, Debug)]
pub struct ScalingContribution {
    pub scaling: f32,
    pub scaling_mult: f32,
    pub curve_mult: f32,
    pub contributes: bool,
}

pub fn effective_str(str_stat: u8, two_handing: bool, disable_two_hand_bonus: bool) -> u16 {
    if two_handing && !disable_two_hand_bonus {
        (u16::from(str_stat) * 3) / 2
    } else {
        u16::from(str_stat)
    }
}

pub const SCADUTREE_MAX_LEVEL: u8 = 20;
pub const SCADUTREE_ATTACK_MULTIPLIERS: [f32; 21] = [
    1.00, 1.10, 1.20, 1.25, 1.30, 1.35, 1.42, 1.50, 1.55, 1.60, 1.65, 1.75, 1.85, 1.87, 1.90, 1.92,
    1.95, 1.97, 2.00, 2.02, 2.05,
];

pub fn scadutree_attack_multiplier(dlc_scaling: bool, scadutree_level: u8) -> f32 {
    if !dlc_scaling {
        return 1.0;
    }
    SCADUTREE_ATTACK_MULTIPLIERS[usize::from(scadutree_level.min(SCADUTREE_MAX_LEVEL))]
}

pub fn scadutree_received_damage_multiplier(dlc_scaling: bool, scadutree_level: u8) -> f32 {
    1.0 / scadutree_attack_multiplier(dlc_scaling, scadutree_level)
}

pub fn scadutree_damage_negation(dlc_scaling: bool, scadutree_level: u8) -> f32 {
    1.0 - scadutree_received_damage_multiplier(dlc_scaling, scadutree_level)
}

pub fn meets_requirements(weapon: &Weapon, effective_str: u16, stats: &Stats) -> bool {
    effective_str >= u16::from(weapon.requirements[STAT_STR])
        && stats.dex >= weapon.requirements[STAT_DEX]
        && stats.int >= weapon.requirements[STAT_INT]
        && stats.fai >= weapon.requirements[STAT_FAI]
        && stats.arc >= weapon.requirements[STAT_ARC]
}

pub fn build_contributions(
    weapon: &Weapon,
    reinforce: &crate::model::ReinforceLevel,
    aec: &crate::model::AttackElementCorrect,
    curve_mults: &[f32; COMBAT_STAT_COUNT],
    damage_type: DamageType,
) -> [ScalingContribution; COMBAT_STAT_COUNT] {
    [
        ScalingContribution {
            scaling: weapon.scaling[STAT_STR],
            scaling_mult: reinforce.scaling_mult[STAT_STR],
            curve_mult: curve_mults[STAT_STR],
            contributes: aec.stat_scales(STAT_STR, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_DEX],
            scaling_mult: reinforce.scaling_mult[STAT_DEX],
            curve_mult: curve_mults[STAT_DEX],
            contributes: aec.stat_scales(STAT_DEX, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_INT],
            scaling_mult: reinforce.scaling_mult[STAT_INT],
            curve_mult: curve_mults[STAT_INT],
            contributes: aec.stat_scales(STAT_INT, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_FAI],
            scaling_mult: reinforce.scaling_mult[STAT_FAI],
            curve_mult: curve_mults[STAT_FAI],
            contributes: aec.stat_scales(STAT_FAI, damage_type),
        },
        ScalingContribution {
            scaling: weapon.scaling[STAT_ARC],
            scaling_mult: reinforce.scaling_mult[STAT_ARC],
            curve_mult: curve_mults[STAT_ARC],
            contributes: aec.stat_scales(STAT_ARC, damage_type),
        },
    ]
}

pub fn calculate_ar_for_type(
    actual_base: f32,
    contributions: &[ScalingContribution; COMBAT_STAT_COUNT],
) -> f32 {
    let bonus: f32 = contributions
        .iter()
        .filter(|contribution| contribution.contributes)
        .map(|contribution| {
            contribution.scaling * contribution.scaling_mult * contribution.curve_mult
        })
        .sum();
    actual_base * (1.0 + bonus)
}

fn stat_values_for_scaling(
    stats: &Stats,
    effective_str_value: u16,
    two_hand_disabled: bool,
) -> [u16; COMBAT_STAT_COUNT] {
    [
        if two_hand_disabled {
            u16::from(stats.str)
        } else {
            effective_str_value
        },
        u16::from(stats.dex),
        u16::from(stats.int),
        u16::from(stats.fai),
        u16::from(stats.arc),
    ]
}

pub fn calculate_ar(
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    data: &GameData,
) -> Result<DamageBreakdown, String> {
    let reinforce = data
        .reinforce_level(weapon.reinforce_type, upgrade)
        .ok_or_else(|| {
            format!(
                "missing reinforce level: type={} level={upgrade}",
                weapon.reinforce_type
            )
        })?;
    let aec = data
        .attack_element(weapon.attack_element_correct_id)
        .ok_or_else(|| {
            format!(
                "missing attack_element_correct_id={}",
                weapon.attack_element_correct_id
            )
        })?;

    let stat_values =
        stat_values_for_scaling(stats, effective_str_value, weapon.disable_two_hand_bonus);
    let mut breakdown = DamageBreakdown::default();
    for damage_type in DamageType::ALL {
        let damage_idx = damage_type.as_index();
        let actual_base = weapon.base[damage_idx] * reinforce.damage_mult[damage_idx];
        if actual_base <= 0.0 {
            continue;
        }

        let curve_id = weapon.damage_curve_ids[damage_idx];
        let curve_mults = [
            data.calc_curve_value(curve_id, stat_values[STAT_STR])
                .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
            data.calc_curve_value(curve_id, stat_values[STAT_DEX])
                .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
            data.calc_curve_value(curve_id, stat_values[STAT_INT])
                .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
            data.calc_curve_value(curve_id, stat_values[STAT_FAI])
                .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
            data.calc_curve_value(curve_id, stat_values[STAT_ARC])
                .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
        ];
        let contributions = build_contributions(weapon, reinforce, aec, &curve_mults, damage_type);
        let value = calculate_ar_for_type(actual_base, &contributions);
        match damage_type {
            DamageType::Physical => breakdown.physical = value,
            DamageType::Magic => breakdown.magic = value,
            DamageType::Fire => breakdown.fire = value,
            DamageType::Lightning => breakdown.lightning = value,
            DamageType::Holy => breakdown.holy = value,
        }
    }
    Ok(breakdown)
}

pub fn apply_aow_attack_buffs(
    mut breakdown: DamageBreakdown,
    aow: Option<&crate::model::Aow>,
) -> DamageBreakdown {
    let Some(aow) = aow else {
        return breakdown;
    };
    breakdown.physical += aow.buff_attack_power[DamageType::Physical.as_index()];
    breakdown.magic += aow.buff_attack_power[DamageType::Magic.as_index()];
    breakdown.fire += aow.buff_attack_power[DamageType::Fire.as_index()];
    breakdown.lightning += aow.buff_attack_power[DamageType::Lightning.as_index()];
    breakdown.holy += aow.buff_attack_power[DamageType::Holy.as_index()];
    breakdown
}

fn calculate_skill_damage_for_type(
    weapon: &Weapon,
    attack_row: &AowAttackRow,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_type: DamageType,
    data: &GameData,
) -> Result<f32, String> {
    let damage_idx = damage_type.as_index();
    let reinforce = data
        .reinforce_level(weapon.reinforce_type, upgrade)
        .ok_or_else(|| {
            format!(
                "missing reinforce level: type={} level={upgrade}",
                weapon.reinforce_type
            )
        })?;
    let weapon_motion_component = weapon.base[damage_idx]
        * reinforce.damage_mult[damage_idx]
        * (attack_row.motion_values[damage_idx] / 100.0);
    let fixed_attack_component = if attack_row.is_add_base_atk || attack_row.is_arrow_attack {
        attack_row.attack_base[damage_idx] * reinforce.base_attack_mult
    } else {
        0.0
    };
    let actual_base = weapon_motion_component + fixed_attack_component;
    if actual_base <= 0.0 {
        return Ok(0.0);
    }

    let stat_values = stat_values_for_scaling(
        stats,
        effective_str_value,
        attack_row.is_disable_both_hands_bonus,
    );
    let curve_id = weapon.damage_curve_ids[damage_idx];
    let curve_mults = [
        data.calc_curve_value(curve_id, stat_values[STAT_STR])
            .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
        data.calc_curve_value(curve_id, stat_values[STAT_DEX])
            .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
        data.calc_curve_value(curve_id, stat_values[STAT_INT])
            .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
        data.calc_curve_value(curve_id, stat_values[STAT_FAI])
            .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
        data.calc_curve_value(curve_id, stat_values[STAT_ARC])
            .ok_or_else(|| format!("missing curve_id={curve_id} for {damage_type}"))?,
    ];

    if let Some(override_id) = attack_row.overwrite_attack_element_correct_id {
        let aec_ext = data.attack_element_ext(override_id).ok_or_else(|| {
            format!(
                "missing attack_element_correct_ext_id={} for AoW row {} ({})",
                override_id, attack_row.sheet_row, attack_row.raw_name
            )
        })?;
        let contributions =
            build_override_contributions(weapon, reinforce, aec_ext, &curve_mults, damage_idx);
        return Ok(calculate_ar_for_type(actual_base, &contributions));
    }

    let aec = data
        .attack_element(weapon.attack_element_correct_id)
        .ok_or_else(|| {
            format!(
                "missing attack_element_correct_id={}",
                weapon.attack_element_correct_id
            )
        })?;
    let contributions = build_contributions(weapon, reinforce, aec, &curve_mults, damage_type);
    Ok(calculate_ar_for_type(actual_base, &contributions))
}

fn build_override_contributions(
    weapon: &Weapon,
    reinforce: &crate::model::ReinforceLevel,
    aec_ext: &AttackElementCorrectExt,
    curve_mults: &[f32; COMBAT_STAT_COUNT],
    damage_idx: usize,
) -> [ScalingContribution; COMBAT_STAT_COUNT] {
    std::array::from_fn(|stat_idx| ScalingContribution {
        scaling: aec_ext
            .overwrite_rate(stat_idx, damage_idx)
            .unwrap_or(weapon.scaling[stat_idx] * aec_ext.influence_rate(stat_idx, damage_idx)),
        scaling_mult: reinforce.scaling_mult[stat_idx],
        curve_mult: curve_mults[stat_idx],
        contributes: aec_ext.stat_scales(stat_idx, damage_idx),
    })
}

pub fn calculate_aow_damage(
    weapon: &Weapon,
    attack_rows: &[&AowAttackRow],
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    data: &GameData,
) -> Result<(f32, f32), String> {
    let routes = calculate_aow_routes(
        weapon,
        attack_rows,
        upgrade,
        stats,
        effective_str_value,
        data,
    )?;
    let mut best_first_hit = 0.0_f32;
    let mut best_full_sequence = 0.0_f32;
    for route in routes {
        if route.first_hit_damage > best_first_hit {
            best_first_hit = route.first_hit_damage;
        }
        if route.total_damage.total() > best_full_sequence {
            best_full_sequence = route.total_damage.total();
        }
    }
    Ok((best_first_hit, best_full_sequence))
}

#[derive(Debug)]
struct PendingRoute {
    label: String,
    priority: u16,
    actions: BTreeMap<(u16, String), AowActionResult>,
}

pub fn calculate_aow_routes(
    weapon: &Weapon,
    attack_rows: &[&AowAttackRow],
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    data: &GameData,
) -> Result<Vec<AowRouteResult>, String> {
    let weapon_status = calculate_status_buildup(weapon, upgrade, stats, data)?;
    let aows_by_id = data
        .aows
        .iter()
        .map(|aow| (aow.aow_id, aow))
        .collect::<HashMap<_, _>>();
    let mut activation_orders = HashMap::<(String, u16), u16>::new();
    for row in attack_rows.iter().filter(|row| !row.is_lacking_fp) {
        let Some(activation_action_id) = aows_by_id
            .get(&row.aow_id)
            .and_then(|aow| aow.buff_activation_action_id.as_deref())
        else {
            continue;
        };
        for assignment in data.aow_route_assignments(row.aow_id, row.sheet_row) {
            if assignment.action_id == activation_action_id {
                activation_orders
                    .entry((assignment.route_id.clone(), row.aow_id))
                    .and_modify(|order| *order = (*order).min(assignment.action_order))
                    .or_insert(assignment.action_order);
            }
        }
    }
    let mut pending: HashMap<String, PendingRoute> = HashMap::new();

    for row in attack_rows.iter().filter(|row| !row.is_lacking_fp) {
        let effects = data.aow_effects(row.aow_id, row.sheet_row).to_vec();
        let has_route_effect = effects.iter().any(|effect| {
            matches!(
                effect.role,
                AowEffectRole::PerHitStatus
                    | AowEffectRole::PerHitAttackPower
                    | AowEffectRole::ReplacementOrChained
            )
        });
        let assignments = data.aow_route_assignments(row.aow_id, row.sheet_row);
        if assignments.is_empty() {
            if row.is_damaging() || has_route_effect {
                return Err(format!(
                    "missing AoW route assignment for skill={} sheet_row={}",
                    row.aow_id, row.sheet_row
                ));
            }
            continue;
        }

        let mut damage = DamageBreakdown::default();
        if row.is_damaging() {
            for damage_type in DamageType::ALL {
                let value = calculate_skill_damage_for_type(
                    weapon,
                    row,
                    upgrade,
                    stats,
                    effective_str_value,
                    damage_type,
                    data,
                )?;
                match damage_type {
                    DamageType::Physical => damage.physical = value,
                    DamageType::Magic => damage.magic = value,
                    DamageType::Fire => damage.fire = value,
                    DamageType::Lightning => damage.lightning = value,
                    DamageType::Holy => damage.holy = value,
                }
            }
        }
        let mut per_hit_status = StatusBuildup::default();
        let mut warnings = Vec::new();
        for effect in &effects {
            if !effect.is_supported {
                warnings.push(format!(
                    "effect {} ({}) is not modeled: {}",
                    effect.effect_id, effect.effect_name, effect.reason
                ));
                continue;
            }
            match effect.role {
                AowEffectRole::PerHitStatus => {
                    let use_correction = effect.uses_status_correction;
                    let flags = StatusCorrectionFlags {
                        bleed: (effect.status_buildup.bleed > 0.0).then_some(use_correction),
                        frost: (effect.status_buildup.frost > 0.0).then_some(use_correction),
                        poison: (effect.status_buildup.poison > 0.0).then_some(use_correction),
                        scarlet_rot: (effect.status_buildup.scarlet_rot > 0.0)
                            .then_some(use_correction),
                        sleep: (effect.status_buildup.sleep > 0.0).then_some(use_correction),
                        madness: (effect.status_buildup.madness > 0.0).then_some(use_correction),
                        death: (effect.status_buildup.death > 0.0).then_some(use_correction),
                    };
                    per_hit_status = per_hit_status.combined_with(truncate_status_buildup(
                        scale_status_additions(
                            effect.status_buildup,
                            flags,
                            weapon,
                            upgrade,
                            stats,
                            data,
                        )?,
                    ));
                }
                AowEffectRole::PerHitAttackPower => {
                    return Err(format!(
                        "supported per-hit attack-power effect is not implemented: skill={} sheet_row={} effect={}",
                        row.aow_id, row.sheet_row, effect.effect_id
                    ));
                }
                _ => {}
            }
        }
        let stamina_cost = match row.stamina_cost_mode {
            StaminaCostMode::WeaponScaled => row.stamina_cost * weapon.stamina_consumption_rate,
            StaminaCostMode::Precalculated => row.stamina_cost,
        };

        for assignment in assignments {
            let aow = aows_by_id.get(&row.aow_id).copied();
            let buff_active = activation_orders
                .get(&(assignment.route_id.clone(), row.aow_id))
                .is_some_and(|activation_order| assignment.action_order > *activation_order);
            let mut hit_damage = damage;
            let mut hit_status = weapon_status;
            if buff_active && let Some(aow) = aow {
                let buff_mv = row.weapon_buff_mv / 100.0;
                hit_damage.physical +=
                    aow.buff_attack_power[DamageType::Physical.as_index()] * buff_mv;
                hit_damage.magic += aow.buff_attack_power[DamageType::Magic.as_index()] * buff_mv;
                hit_damage.fire += aow.buff_attack_power[DamageType::Fire.as_index()] * buff_mv;
                hit_damage.lightning +=
                    aow.buff_attack_power[DamageType::Lightning.as_index()] * buff_mv;
                hit_damage.holy += aow.buff_attack_power[DamageType::Holy.as_index()] * buff_mv;
                hit_status =
                    apply_aow_status_buffs(hit_status, weapon, upgrade, stats, data, Some(aow))?;
            }
            hit_status = hit_status
                .scale(row.status_mv / 100.0)
                .combined_with(per_hit_status);
            let route = pending
                .entry(assignment.route_id.clone())
                .or_insert_with(|| PendingRoute {
                    label: assignment.route_label.clone(),
                    priority: assignment.route_priority,
                    actions: BTreeMap::new(),
                });
            let action = route
                .actions
                .entry((assignment.action_order, assignment.action_id.clone()))
                .or_insert_with(|| AowActionResult {
                    action_id: assignment.action_id.clone(),
                    action_order: assignment.action_order,
                    stamina_cost: 0.0,
                    hits: Vec::new(),
                });
            action.stamina_cost = action.stamina_cost.max(stamina_cost);
            action.hits.push(AowHitResult {
                sheet_row: row.sheet_row,
                hit_order: assignment.hit_order,
                raw_name: row.raw_name.clone(),
                damage: hit_damage,
                status_buildup: hit_status,
                physical_attack_attribute: row.resolved_physical_attribute(weapon),
                buff_active,
                effects: effects.clone(),
                warnings: warnings.clone(),
            });
        }
    }

    let mut routes = pending
        .into_iter()
        .map(|(route_id, pending)| {
            let mut actions = pending.actions.into_values().collect::<Vec<_>>();
            for action in &mut actions {
                action
                    .hits
                    .sort_by_key(|hit| (hit.hit_order, hit.sheet_row));
            }
            let mut total_damage = DamageBreakdown::default();
            let mut total_status_buildup = StatusBuildup::default();
            let mut first_hit_damage = 0.0_f32;
            let mut total_stamina_cost = 0.0_f32;
            for action in &actions {
                total_stamina_cost += action.stamina_cost;
                for hit in &action.hits {
                    let hit_damage = hit.damage.total();
                    if first_hit_damage <= 0.0 && hit_damage > 0.0 {
                        first_hit_damage = hit_damage;
                    }
                    total_damage = total_damage.combined_with(hit.damage);
                    total_status_buildup = total_status_buildup.combined_with(hit.status_buildup);
                }
            }
            AowRouteResult {
                route_id,
                route_label: pending.label,
                route_priority: pending.priority,
                buff_activation_action_id: actions.first().and_then(|action| {
                    action.hits.first().and_then(|hit| {
                        attack_rows
                            .iter()
                            .find(|row| row.sheet_row == hit.sheet_row)
                            .and_then(|row| aows_by_id.get(&row.aow_id))
                            .and_then(|aow| aow.buff_activation_action_id.clone())
                    })
                }),
                actions,
                first_hit_damage,
                total_damage,
                total_status_buildup,
                total_stamina_cost,
            }
        })
        .collect::<Vec<_>>();
    routes.sort_by(|left, right| {
        left.route_priority
            .cmp(&right.route_priority)
            .then_with(|| left.route_id.cmp(&right.route_id))
    });
    Ok(routes)
}

pub fn calculate_status_buildup(
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    data: &GameData,
) -> Result<StatusBuildup, String> {
    let mut base = data.weapon_passive(weapon.weapon_id);
    if let Some(overlay) = data.weapon_passive_overlay(weapon.weapon_id, upgrade) {
        base = merge_status_effect_source(base, overlay);
    }
    if base.buildup.bleed <= 0.0
        && base.buildup.frost <= 0.0
        && base.buildup.poison <= 0.0
        && base.buildup.scarlet_rot <= 0.0
        && base.buildup.sleep <= 0.0
        && base.buildup.madness <= 0.0
        && base.buildup.death <= 0.0
    {
        return Ok(base.buildup);
    }

    Ok(truncate_status_buildup(scale_status_additions(
        base.buildup,
        base.correction_flags,
        weapon,
        upgrade,
        stats,
        data,
    )?))
}

pub fn apply_aow_status_buffs(
    mut buildup: StatusBuildup,
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    data: &GameData,
    aow: Option<&crate::model::Aow>,
) -> Result<StatusBuildup, String> {
    let Some(aow) = aow else {
        return Ok(buildup);
    };
    buildup = buildup.with_aow_additions(Some(aow));

    let scaling = aow.scaling_status_add;
    if scaling.bleed <= 0.0
        && scaling.frost <= 0.0
        && scaling.poison <= 0.0
        && scaling.scarlet_rot <= 0.0
        && scaling.sleep <= 0.0
        && scaling.madness <= 0.0
        && scaling.death <= 0.0
    {
        return Ok(buildup);
    }

    buildup = buildup.combined_with(scale_status_additions(
        scaling,
        aow.scaling_status_flags,
        weapon,
        upgrade,
        stats,
        data,
    )?);
    Ok(truncate_status_buildup(buildup))
}

fn scale_status_additions(
    buildup: StatusBuildup,
    flags: StatusCorrectionFlags,
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    data: &GameData,
) -> Result<StatusBuildup, String> {
    let reinforce = data
        .reinforce_level(weapon.reinforce_type, upgrade)
        .ok_or_else(|| {
            format!(
                "missing reinforce level: type={} level={upgrade}",
                weapon.reinforce_type
            )
        })?;
    let scale =
        |value: f32,
         stat_idx: usize,
         stat_value: u8,
         curve_id: usize,
         flag: Option<bool>|
         -> Result<f32, String> {
            if value <= 0.0
                || !uses_status_correction(flag, weapon.scaling[stat_idx] > 0.0)
                || weapon.scaling[stat_idx] <= 0.0
            {
                return Ok(value);
            }
            let curve_mult = data
                .calc_curve_value(curve_id, u16::from(stat_value))
                .ok_or_else(|| format!("missing curve_id={curve_id} for status scaling"))?;
            Ok(value
                * (1.0 + weapon.scaling[stat_idx] * reinforce.scaling_mult[stat_idx] * curve_mult))
        };
    Ok(StatusBuildup {
        bleed: scale(
            buildup.bleed,
            STAT_ARC,
            stats.arc,
            weapon.bleed_curve_id,
            flags.bleed,
        )?,
        frost: scale(
            buildup.frost,
            STAT_INT,
            stats.int,
            weapon.damage_curve_ids[DamageType::Magic.as_index()],
            flags.frost,
        )?,
        poison: scale(
            buildup.poison,
            STAT_ARC,
            stats.arc,
            weapon.bleed_curve_id,
            flags.poison,
        )?,
        scarlet_rot: scale(
            buildup.scarlet_rot,
            STAT_ARC,
            stats.arc,
            weapon.bleed_curve_id,
            flags.scarlet_rot,
        )?,
        sleep: scale(
            buildup.sleep,
            STAT_ARC,
            stats.arc,
            weapon.bleed_curve_id,
            flags.sleep,
        )?,
        madness: scale(
            buildup.madness,
            STAT_ARC,
            stats.arc,
            weapon.bleed_curve_id,
            flags.madness,
        )?,
        death: scale(
            buildup.death,
            STAT_ARC,
            stats.arc,
            weapon.bleed_curve_id,
            flags.death,
        )?,
    })
}

fn truncate_status_buildup(buildup: StatusBuildup) -> StatusBuildup {
    StatusBuildup {
        bleed: buildup.bleed.floor(),
        frost: buildup.frost.floor(),
        poison: buildup.poison.floor(),
        scarlet_rot: buildup.scarlet_rot.floor(),
        sleep: buildup.sleep.floor(),
        madness: buildup.madness.floor(),
        death: buildup.death.floor(),
    }
}

fn uses_status_correction(flag: Option<bool>, fallback: bool) -> bool {
    flag.unwrap_or(fallback)
}

fn merge_status_effect_source(
    mut base: crate::model::StatusEffectSource,
    overlay: crate::model::StatusEffectSource,
) -> crate::model::StatusEffectSource {
    merge_status_value(
        &mut base.buildup.bleed,
        &mut base.correction_flags.bleed,
        overlay.buildup.bleed,
        overlay.correction_flags.bleed,
    );
    merge_status_value(
        &mut base.buildup.frost,
        &mut base.correction_flags.frost,
        overlay.buildup.frost,
        overlay.correction_flags.frost,
    );
    merge_status_value(
        &mut base.buildup.poison,
        &mut base.correction_flags.poison,
        overlay.buildup.poison,
        overlay.correction_flags.poison,
    );
    merge_status_value(
        &mut base.buildup.scarlet_rot,
        &mut base.correction_flags.scarlet_rot,
        overlay.buildup.scarlet_rot,
        overlay.correction_flags.scarlet_rot,
    );
    merge_status_value(
        &mut base.buildup.sleep,
        &mut base.correction_flags.sleep,
        overlay.buildup.sleep,
        overlay.correction_flags.sleep,
    );
    merge_status_value(
        &mut base.buildup.madness,
        &mut base.correction_flags.madness,
        overlay.buildup.madness,
        overlay.correction_flags.madness,
    );
    merge_status_value(
        &mut base.buildup.death,
        &mut base.correction_flags.death,
        overlay.buildup.death,
        overlay.correction_flags.death,
    );
    base
}

fn merge_status_value(
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

#[derive(Clone, Copy, Debug)]
pub struct StartingClass {
    pub name: &'static str,
    pub base_level: u16,
    pub base_total: u16,
    pub base_stats: Stats,
}

pub const STARTING_CLASSES: [StartingClass; 10] = [
    StartingClass {
        name: "Vagabond",
        base_level: 9,
        base_total: 88,
        base_stats: Stats {
            vig: 15,
            mnd: 10,
            end: 11,
            str: 14,
            dex: 13,
            int: 9,
            fai: 9,
            arc: 7,
        },
    },
    StartingClass {
        name: "Warrior",
        base_level: 8,
        base_total: 87,
        base_stats: Stats {
            vig: 11,
            mnd: 12,
            end: 11,
            str: 10,
            dex: 16,
            int: 10,
            fai: 8,
            arc: 9,
        },
    },
    StartingClass {
        name: "Hero",
        base_level: 7,
        base_total: 86,
        base_stats: Stats {
            vig: 14,
            mnd: 9,
            end: 12,
            str: 16,
            dex: 9,
            int: 7,
            fai: 8,
            arc: 11,
        },
    },
    StartingClass {
        name: "Bandit",
        base_level: 5,
        base_total: 84,
        base_stats: Stats {
            vig: 10,
            mnd: 11,
            end: 10,
            str: 9,
            dex: 13,
            int: 9,
            fai: 8,
            arc: 14,
        },
    },
    StartingClass {
        name: "Astrologer",
        base_level: 6,
        base_total: 85,
        base_stats: Stats {
            vig: 9,
            mnd: 15,
            end: 9,
            str: 8,
            dex: 12,
            int: 16,
            fai: 7,
            arc: 9,
        },
    },
    StartingClass {
        name: "Prophet",
        base_level: 7,
        base_total: 86,
        base_stats: Stats {
            vig: 10,
            mnd: 14,
            end: 8,
            str: 11,
            dex: 10,
            int: 7,
            fai: 16,
            arc: 10,
        },
    },
    StartingClass {
        name: "Samurai",
        base_level: 9,
        base_total: 88,
        base_stats: Stats {
            vig: 12,
            mnd: 11,
            end: 13,
            str: 12,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 8,
        },
    },
    StartingClass {
        name: "Prisoner",
        base_level: 9,
        base_total: 88,
        base_stats: Stats {
            vig: 11,
            mnd: 12,
            end: 11,
            str: 11,
            dex: 14,
            int: 14,
            fai: 6,
            arc: 9,
        },
    },
    StartingClass {
        name: "Confessor",
        base_level: 10,
        base_total: 89,
        base_stats: Stats {
            vig: 10,
            mnd: 13,
            end: 10,
            str: 12,
            dex: 12,
            int: 9,
            fai: 14,
            arc: 9,
        },
    },
    StartingClass {
        name: "Wretch",
        base_level: 1,
        base_total: 80,
        base_stats: Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 10,
            dex: 10,
            int: 10,
            fai: 10,
            arc: 10,
        },
    },
];

pub fn class_by_name(name: &str) -> Option<StartingClass> {
    STARTING_CLASSES
        .iter()
        .copied()
        .find(|class_info| class_info.name.eq_ignore_ascii_case(name))
}

pub fn compute_free_points(
    class_info: StartingClass,
    character_level: u16,
    current_stats: &Stats,
) -> Result<u16, String> {
    let base = class_info.base_stats;
    let floor_ok = current_stats.vig >= base.vig
        && current_stats.mnd >= base.mnd
        && current_stats.end >= base.end
        && current_stats.str >= base.str
        && current_stats.dex >= base.dex
        && current_stats.int >= base.int
        && current_stats.fai >= base.fai
        && current_stats.arc >= base.arc;
    if !floor_ok {
        return Err("current stats are below class minimums".to_string());
    }

    let total_stat_points = i32::from(class_info.base_total)
        + (i32::from(character_level) - i32::from(class_info.base_level));
    let current_sum = i32::from(current_stats.sum_all_8());
    let free = total_stat_points - current_sum;

    if free < 0 {
        return Err("current stats exceed level budget".to_string());
    }
    Ok(free as u16)
}

#[derive(Clone, Debug)]
pub struct StatIter {
    mins: [u8; COMBAT_STAT_COUNT],
    free: u16,
    current: [u8; COMBAT_STAT_COUNT],
    done: bool,
}

impl StatIter {
    pub fn new(mins: [u8; COMBAT_STAT_COUNT], free: u16) -> Result<Self, String> {
        let capacity: u16 = mins.iter().map(|value| 99_u16 - u16::from(*value)).sum();
        if free > capacity {
            return Err(format!(
                "free points {free} exceed combat stat capacity {capacity} for mins={mins:?}"
            ));
        }

        let mut current = mins;
        let mut remaining = free;
        for (idx, min_value) in mins.iter().enumerate() {
            let can_add = (99_u16 - u16::from(*min_value)).min(remaining) as u8;
            current[idx] = *min_value + can_add;
            remaining -= u16::from(can_add);
            if remaining == 0 {
                break;
            }
        }

        Ok(Self {
            mins,
            free,
            current,
            done: false,
        })
    }

    pub fn free(&self) -> u16 {
        self.free
    }
}

impl Iterator for StatIter {
    type Item = [u8; COMBAT_STAT_COUNT];

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let result = self.current;
        let mut pivot: Option<usize> = None;
        for idx in (0..=3).rev() {
            if self.current[idx] > self.mins[idx] {
                pivot = Some(idx);
                break;
            }
        }

        let Some(pivot_idx) = pivot else {
            self.done = true;
            return Some(result);
        };

        self.current[pivot_idx] -= 1;
        let mut freed: u16 = 1;
        for idx in (pivot_idx + 1)..COMBAT_STAT_COUNT {
            freed += u16::from(self.current[idx] - self.mins[idx]);
            self.current[idx] = self.mins[idx];
        }
        for idx in (pivot_idx + 1)..COMBAT_STAT_COUNT {
            if freed == 0 {
                break;
            }
            let room = u16::from(99_u8 - self.current[idx]);
            let add = room.min(freed) as u8;
            self.current[idx] += add;
            freed -= u16::from(add);
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{calculate_ar, data::load_game_data};

    use super::*;

    fn find_weapon<'a>(data: &'a GameData, name: &str, affinity: &str) -> &'a Weapon {
        data.weapons
            .iter()
            .find(|weapon| weapon.name == name && weapon.affinity == affinity)
            .unwrap_or_else(|| panic!("weapon not found: {name} | {affinity}"))
    }

    #[test]
    fn stat_iter_preserves_sum() {
        let mins = [10, 10, 10, 10, 10];
        let free = 7;
        let iter = StatIter::new(mins, free).unwrap();
        let values: Vec<[u8; COMBAT_STAT_COUNT]> = iter.collect();
        assert!(!values.is_empty());

        let target_sum = mins.iter().map(|value| u16::from(*value)).sum::<u16>() + free;
        for value in values {
            let sum = value.iter().map(|point| u16::from(*point)).sum::<u16>();
            assert_eq!(sum, target_sum);
            assert!(value.iter().all(|point| (1..=99).contains(point)));
        }
    }

    #[test]
    fn data_matches_known_ar_cases() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();

        let uchi = find_weapon(&game_data, "Uchigatana", "Keen");
        let uchi_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 11,
            dex: 40,
            int: 9,
            fai: 8,
            arc: 8,
        };
        let uchi_breakdown = calculate_ar(
            uchi,
            25,
            &uchi_stats,
            effective_str(uchi_stats.str, false, uchi.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        assert!((uchi_breakdown.total() - 475.983).abs() < 0.01);

        let lordsworn = find_weapon(&game_data, "Lordsworn's Greatsword", "Quality");
        let lordsworn_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 40,
            dex: 40,
            int: 9,
            fai: 9,
            arc: 7,
        };
        let lordsworn_breakdown = calculate_ar(
            lordsworn,
            25,
            &lordsworn_stats,
            effective_str(lordsworn_stats.str, false, lordsworn.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        assert!((lordsworn_breakdown.total() - 582.1702).abs() < 0.01);

        let reduvia = find_weapon(&game_data, "Reduvia", "Standard");
        let reduvia_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 5,
            dex: 13,
            int: 9,
            fai: 8,
            arc: 45,
        };
        let reduvia_breakdown = calculate_ar(
            reduvia,
            10,
            &reduvia_stats,
            effective_str(reduvia_stats.str, false, reduvia.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        assert!((reduvia_breakdown.total() - 343.6182).abs() < 0.01);

        let fire_uchi = find_weapon(&game_data, "Uchigatana", "Fire");
        let fire_uchi_stats = Stats {
            vig: 12,
            mnd: 11,
            end: 13,
            str: 48,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 8,
        };
        let fire_uchi_breakdown = calculate_ar(
            fire_uchi,
            25,
            &fire_uchi_stats,
            effective_str(fire_uchi_stats.str, true, fire_uchi.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        assert!((fire_uchi_breakdown.total() - 652.8947).abs() < 0.02);
    }

    #[test]
    fn two_handed_strength_scales_beyond_stat_cap() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();

        let giant_crusher = find_weapon(&game_data, "Giant-Crusher", "Heavy");
        let stats = Stats {
            vig: 60,
            mnd: 11,
            end: 20,
            str: 84,
            dex: 15,
            int: 9,
            fai: 8,
            arc: 8,
        };
        assert_eq!(
            effective_str(stats.str, true, giant_crusher.disable_two_hand_bonus),
            126
        );

        let breakdown = calculate_ar(
            giant_crusher,
            25,
            &stats,
            effective_str(stats.str, true, giant_crusher.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        assert!((breakdown.physical - 976.025).abs() < 0.02);

        let capped_stats = Stats { str: 99, ..stats };
        let capped_breakdown = calculate_ar(
            giant_crusher,
            25,
            &capped_stats,
            effective_str(capped_stats.str, true, giant_crusher.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        assert!(capped_breakdown.physical > breakdown.physical);
    }

    #[test]
    fn paired_weapons_do_not_gain_two_hand_bonus() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();

        let iron_ball = find_weapon(&game_data, "Iron Ball", "Heavy");
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 68,
            dex: 15,
            int: 10,
            fai: 10,
            arc: 10,
        };
        let one_handed = calculate_ar(
            iron_ball,
            25,
            &stats,
            effective_str(stats.str, false, iron_ball.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        let two_handed = calculate_ar(
            iron_ball,
            25,
            &stats,
            effective_str(stats.str, true, iron_ball.disable_two_hand_bonus),
            &game_data,
        )
        .unwrap();
        assert!((one_handed.total() - 469.17657).abs() < 0.02);
        assert!((two_handed.total() - one_handed.total()).abs() < 0.001);
    }

    #[test]
    fn aow_damage_errors_when_override_attack_element_is_missing() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let mut game_data = load_game_data(data_path).unwrap();
        let (weapon, attack_row, damage_type) = game_data
            .aow_attack_rows
            .values()
            .flat_map(|rows| rows.iter())
            .filter(|row| row.overwrite_attack_element_correct_id.is_some() && row.is_damaging())
            .find_map(|row| {
                game_data.weapons.iter().find_map(|weapon| {
                    let reinforce = game_data.reinforce_level(weapon.reinforce_type, 25)?;
                    DamageType::ALL.iter().copied().find_map(|damage_type| {
                        let idx = damage_type.as_index();
                        let actual_base = weapon.base[idx]
                            * reinforce.damage_mult[idx]
                            * (row.motion_values[idx] / 100.0)
                            + row.attack_base[idx];
                        (actual_base > 0.0).then(|| (weapon.clone(), row.clone(), damage_type))
                    })
                })
            })
            .expect("missing override attack row with positive damage");
        let override_id = attack_row
            .overwrite_attack_element_correct_id
            .expect("missing override id");
        assert!(
            game_data
                .attack_element_correct_ext
                .remove(&override_id)
                .is_some()
        );
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 11,
            dex: 40,
            int: 9,
            fai: 8,
            arc: 8,
        };
        let err = calculate_skill_damage_for_type(
            &weapon,
            &attack_row,
            25,
            &stats,
            effective_str(stats.str, false, weapon.disable_two_hand_bonus),
            damage_type,
            &game_data,
        )
        .expect_err("expected missing override error");
        assert!(err.contains("missing attack_element_correct_ext_id"));
    }

    #[test]
    fn fixed_skill_attack_base_uses_reinforce_base_attack_multiplier() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();
        let weapon = find_weapon(&game_data, "Dagger", "Standard");
        let kick = game_data
            .aow_attack_rows
            .values()
            .flat_map(|rows| rows.iter())
            .find(|row| row.raw_name == "Kick")
            .expect("Kick attack row");
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 10,
            dex: 10,
            int: 10,
            fai: 10,
            arc: 10,
        };

        let plus_zero = calculate_skill_damage_for_type(
            weapon,
            kick,
            0,
            &stats,
            10,
            DamageType::Physical,
            &game_data,
        )
        .unwrap();
        let plus_twenty_five = calculate_skill_damage_for_type(
            weapon,
            kick,
            25,
            &stats,
            10,
            DamageType::Physical,
            &game_data,
        )
        .unwrap();
        assert!((plus_zero - 30.0).abs() < 0.001);
        assert!((plus_twenty_five - 120.0).abs() < 0.001);

        let mut disabled = kick.clone();
        disabled.is_add_base_atk = false;
        disabled.is_arrow_attack = false;
        assert_eq!(
            calculate_skill_damage_for_type(
                weapon,
                &disabled,
                0,
                &stats,
                10,
                DamageType::Physical,
                &game_data,
            )
            .unwrap(),
            0.0
        );

        disabled.is_arrow_attack = true;
        assert!(
            (calculate_skill_damage_for_type(
                weapon,
                &disabled,
                0,
                &stats,
                10,
                DamageType::Physical,
                &game_data,
            )
            .unwrap()
                - 30.0)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn adaptive_physical_attributes_resolve_from_weapon_data() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();
        let dagger = find_weapon(&game_data, "Dagger", "Standard");
        let row = game_data
            .aow_attack_rows
            .values()
            .flat_map(|rows| rows.iter())
            .find(|row| {
                row.physical_attack_attribute
                    == crate::model::PhysicalAttackAttribute::AdaptiveSecondary
            })
            .expect("adaptive-secondary AoW row");
        assert_eq!(
            row.resolved_physical_attribute(dagger),
            crate::model::PhysicalAttackAttribute::Pierce
        );
    }

    #[test]
    fn wild_strikes_routes_keep_finishers_exclusive_and_charge_stamina_per_action() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();
        let weapon = find_weapon(&game_data, "Battle Axe", "Blood");
        let rows = game_data
            .aow_attack_rows(110)
            .iter()
            .filter(|row| row.variant_weapon_type == "Axe")
            .collect::<Vec<_>>();
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 30,
            dex: 30,
            int: 10,
            fai: 10,
            arc: 30,
        };
        let routes = calculate_aow_routes(weapon, &rows, 25, &stats, 30, &game_data).unwrap();
        assert_eq!(
            routes
                .iter()
                .map(|route| route.route_id.as_str())
                .collect::<Vec<_>>(),
            vec!["r1", "r2"]
        );
        assert!(routes.iter().all(|route| route.actions.len() == 3));

        let r1 = routes.iter().find(|route| route.route_id == "r1").unwrap();
        let r2 = routes.iter().find(|route| route.route_id == "r2").unwrap();
        assert!((r1.total_stamina_cost - 32.0 * weapon.stamina_consumption_rate).abs() < 0.001);
        assert!((r2.total_stamina_cost - 42.0 * weapon.stamina_consumption_rate).abs() < 0.001);

        let weapon_status = calculate_status_buildup(weapon, 25, &stats, &game_data).unwrap();
        assert!((r1.total_status_buildup.bleed - weapon_status.bleed * 4.0).abs() < 0.01);
        assert!((r2.total_status_buildup.bleed - weapon_status.bleed * 4.0).abs() < 0.01);

        let (_, best_full) =
            calculate_aow_damage(weapon, &rows, 25, &stats, 30, &game_data).unwrap();
        assert!((best_full - r1.total_damage.total().max(r2.total_damage.total())).abs() < 0.001);
        assert!(best_full < r1.total_damage.total() + r2.total_damage.total());
    }

    #[test]
    fn passive_status_buildup_scales_with_relevant_stat() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();

        let star_fist_blood = find_weapon(&game_data, "Star Fist", "Blood");
        let blood_stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 12,
            dex: 10,
            int: 10,
            fai: 10,
            arc: 68,
        };
        let blood_status =
            calculate_status_buildup(star_fist_blood, 25, &blood_stats, &game_data).unwrap();
        assert!(blood_status.bleed > 75.0);

        let star_fist_occult = find_weapon(&game_data, "Star Fist", "Occult");
        let occult_status =
            calculate_status_buildup(star_fist_occult, 25, &blood_stats, &game_data).unwrap();
        assert!(occult_status.bleed > 70.0);

        let star_fist_cold = find_weapon(&game_data, "Star Fist", "Cold");
        let cold_stats = Stats {
            int: 68,
            ..blood_stats
        };
        let cold_status =
            calculate_status_buildup(star_fist_cold, 25, &cold_stats, &game_data).unwrap();
        assert!(cold_status.frost > 95.0);

        let antspur_occult = find_weapon(&game_data, "Antspur Rapier", "Occult");
        let antspur_status =
            calculate_status_buildup(antspur_occult, 25, &blood_stats, &game_data).unwrap();
        assert!(antspur_status.scarlet_rot > 60.0);
        assert!(antspur_status.poison <= 0.0);
    }

    #[test]
    fn passive_status_overlays_apply_upgrade_growth() {
        let data_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let game_data = load_game_data(data_path).unwrap();

        let great_katana_blood = find_weapon(&game_data, "Great Katana", "Blood");
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 30,
            dex: 53,
            int: 9,
            fai: 8,
            arc: 8,
        };
        let plus_zero =
            calculate_status_buildup(great_katana_blood, 0, &stats, &game_data).unwrap();
        let plus_twenty_five =
            calculate_status_buildup(great_katana_blood, 25, &stats, &game_data).unwrap();
        assert!(plus_zero.bleed >= 69.0);
        assert_eq!(plus_twenty_five.bleed, 101.0);
        assert!(plus_twenty_five.bleed > plus_zero.bleed + 25.0);
    }
}
