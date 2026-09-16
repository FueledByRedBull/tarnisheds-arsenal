use super::exact_value::ExactRational;
use num_traits::{One, Zero, float::FloatCore};

use crate::model::{
    AowAttackRow, COMBAT_STAT_COUNT, DAMAGE_TYPE_COUNT, DamageType, GameData, ReinforceLevel,
    STAT_ARC, Stats, Weapon,
};

use super::ScalarAowRoute;

#[cfg(test)]
use super::ScalarAowHit;

pub(crate) fn exact_ar(
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    data: &GameData,
) -> Result<[ExactRational; DAMAGE_TYPE_COUNT], String> {
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
    let mut breakdown = std::array::from_fn(|_| ExactRational::zero());

    for damage_type in DamageType::ALL {
        let damage_idx = damage_type.as_index();
        let actual_base = rational(weapon.base[damage_idx], "weapon base")?
            * rational(
                reinforce.damage_mult[damage_idx],
                "reinforce damage multiplier",
            )?;
        if actual_base <= ExactRational::zero() {
            continue;
        }

        let curve_mults = curve_values(data, weapon.damage_curve_ids[damage_idx], stat_values)?;
        let mut coefficients = std::array::from_fn(|_| None);
        for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
            if aec.stat_scales(stat_idx, damage_type) {
                *coefficient = Some(
                    rational(weapon.scaling[stat_idx], "weapon scaling")?
                        * rational(
                            reinforce.scaling_mult[stat_idx],
                            "reinforce scaling multiplier",
                        )?,
                );
            }
        }
        breakdown[damage_idx] = apply_scaling(actual_base, &curve_mults, coefficients);
    }
    Ok(breakdown)
}

pub(crate) fn exact_ar_upper_bound(
    weapon: &Weapon,
    upgrade: u8,
    data: &GameData,
    min_stat_values: [u16; COMBAT_STAT_COUNT],
    max_stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<[ExactRational; DAMAGE_TYPE_COUNT], String> {
    for (stat_idx, (&min_value, &max_value)) in min_stat_values
        .iter()
        .zip(max_stat_values.iter())
        .enumerate()
    {
        if min_value > max_value {
            return Err(format!(
                "exact AR upper-bound range is empty for stat index {stat_idx}: {min_value}>{max_value}"
            ));
        }
    }

    let reinforce = reinforce_level(weapon, upgrade, data)?;
    let aec = data
        .attack_element(weapon.attack_element_correct_id)
        .ok_or_else(|| {
            format!(
                "missing attack_element_correct_id={}",
                weapon.attack_element_correct_id
            )
        })?;
    let mut bounds = std::array::from_fn(|_| ExactRational::zero());

    for damage_type in DamageType::ALL {
        let damage_idx = damage_type.as_index();
        let actual_base = rational(weapon.base[damage_idx], "weapon base")?
            * rational(
                reinforce.damage_mult[damage_idx],
                "reinforce damage multiplier",
            )?;
        if actual_base <= ExactRational::zero() {
            continue;
        }

        let curve_id = weapon.damage_curve_ids[damage_idx];
        let mut value = actual_base.clone();
        for stat_idx in 0..COMBAT_STAT_COUNT {
            let maximum_curve = max_curve_value(
                data,
                curve_id,
                min_stat_values[stat_idx],
                max_stat_values[stat_idx],
                stat_idx,
            )?;
            if aec.stat_scales(stat_idx, damage_type) {
                let coefficient = rational(weapon.scaling[stat_idx], "weapon scaling")?
                    * rational(
                        reinforce.scaling_mult[stat_idx],
                        "reinforce scaling multiplier",
                    )?;
                value += actual_base.clone() * coefficient * maximum_curve;
            }
        }
        bounds[damage_idx] = value;
    }

    Ok(bounds)
}

#[derive(Clone, Debug)]
pub(crate) struct ExactScaledCurve {
    curve_id: usize,
    coefficient: ExactRational,
    values: Vec<f32>,
}

/// An exact additive formula for one damage component.
///
/// `base` is the fixed, positive base damage. Each stat entry stores the
/// unscaled curve and its exact coefficient so route construction can merge
/// contributions before doing any per-value arithmetic.
#[derive(Clone, Debug)]
pub(crate) struct ExactFormula {
    pub(crate) base: ExactRational,
    pub(crate) terms: [Vec<ExactScaledCurve>; COMBAT_STAT_COUNT],
    max_stat_values: [u16; COMBAT_STAT_COUNT],
}

impl ExactFormula {
    fn zero(max_stat_values: [u16; COMBAT_STAT_COUNT]) -> Self {
        Self {
            base: ExactRational::zero(),
            terms: std::array::from_fn(|_| Vec::new()),
            max_stat_values,
        }
    }

    pub(crate) fn evaluate(
        &self,
        stat_values: [u16; COMBAT_STAT_COUNT],
    ) -> Result<ExactRational, String> {
        let mut value = self.base.clone();
        for (stat_idx, stat_value) in stat_values.into_iter().enumerate() {
            if stat_value > self.max_stat_values[stat_idx] {
                return Err(format!(
                    "exact formula stat index {stat_idx} is missing value {stat_value}"
                ));
            }
            for term in &self.terms[stat_idx] {
                let curve_value = term.values.get(usize::from(stat_value)).ok_or_else(|| {
                    format!("exact formula stat index {stat_idx} is missing value {stat_value}")
                })?;
                if *curve_value == 0.0 {
                    continue;
                }
                let curve_value = rational(*curve_value, "calc-correct curve")?;
                let contribution = &term.coefficient * &curve_value;
                if value.is_zero() {
                    value = contribution;
                } else if !contribution.is_zero() {
                    value += contribution;
                }
            }
        }
        Ok(value)
    }

    fn delta(
        &self,
        stat_idx: usize,
        old_stat_value: u16,
        new_stat_value: u16,
    ) -> Result<ExactRational, String> {
        if stat_idx >= COMBAT_STAT_COUNT
            || old_stat_value > self.max_stat_values[stat_idx]
            || new_stat_value > self.max_stat_values[stat_idx]
        {
            return Err(format!(
                "exact formula stat index {stat_idx} is missing delta values {old_stat_value} and {new_stat_value}"
            ));
        }
        let mut value = ExactRational::zero();
        for term in &self.terms[stat_idx] {
            let old_curve = term.values.get(usize::from(old_stat_value)).ok_or_else(|| {
                format!(
                    "exact formula stat index {stat_idx} is missing delta values {old_stat_value} and {new_stat_value}"
                )
            })?;
            let new_curve = term.values.get(usize::from(new_stat_value)).ok_or_else(|| {
                format!(
                    "exact formula stat index {stat_idx} is missing delta values {old_stat_value} and {new_stat_value}"
                )
            })?;
            if new_curve == old_curve {
                continue;
            }
            let curve_delta = exact_float_difference(*new_curve, *old_curve)?;
            let contribution = &term.coefficient * &curve_delta;
            if value.is_zero() {
                value = contribution;
            } else if !contribution.is_zero() {
                value += contribution;
            }
        }
        Ok(value)
    }

    fn add_assign(&mut self, other: Self) -> Result<(), String> {
        if self.max_stat_values != other.max_stat_values {
            return Err("exact formulas use different stat ranges".to_string());
        }
        let ExactFormula {
            base: other_base,
            terms: other_terms,
            max_stat_values: _,
        } = other;
        self.base += other_base;
        for (left_terms, right_terms) in self.terms.iter_mut().zip(other_terms) {
            for right in right_terms {
                if let Some(left) = left_terms
                    .iter_mut()
                    .find(|left| left.curve_id == right.curve_id)
                {
                    if left.values.len() != right.values.len() {
                        return Err("exact formulas use different stat ranges".to_string());
                    }
                    left.coefficient += right.coefficient;
                } else if !right.coefficient.is_zero() {
                    left_terms.push(right);
                }
            }
            left_terms.retain(|term| !term.coefficient.is_zero());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExactArFormula {
    components: [ExactFormula; DAMAGE_TYPE_COUNT],
    total: ExactFormula,
}

impl ExactArFormula {
    pub(crate) fn evaluate(
        &self,
        stats: &Stats,
        effective_str_value: u16,
        two_hand_disabled: bool,
    ) -> Result<[ExactRational; DAMAGE_TYPE_COUNT], String> {
        let stat_values = stat_values_for_scaling(stats, effective_str_value, two_hand_disabled);
        let mut values = std::array::from_fn(|_| ExactRational::zero());
        for (index, component) in self.components.iter().enumerate() {
            values[index] = component.evaluate(stat_values)?;
        }
        Ok(values)
    }

    #[cfg(test)]
    pub(crate) fn delta(
        &self,
        stat_idx: usize,
        old_stats: &Stats,
        new_stats: &Stats,
        old_effective_str_value: u16,
        new_effective_str_value: u16,
        two_hand_disabled: bool,
    ) -> Result<[ExactRational; DAMAGE_TYPE_COUNT], String> {
        if stat_idx >= COMBAT_STAT_COUNT {
            return Err(format!(
                "exact formula stat index {stat_idx} is out of range"
            ));
        }
        let old_values =
            stat_values_for_scaling(old_stats, old_effective_str_value, two_hand_disabled);
        let new_values =
            stat_values_for_scaling(new_stats, new_effective_str_value, two_hand_disabled);
        let mut values = std::array::from_fn(|_| ExactRational::zero());
        for (index, component) in self.components.iter().enumerate() {
            values[index] =
                component.delta(stat_idx, old_values[stat_idx], new_values[stat_idx])?;
        }
        Ok(values)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn primary_delta(
        &self,
        stat_idx: usize,
        old_stats: &Stats,
        new_stats: &Stats,
        old_effective_str_value: u16,
        new_effective_str_value: u16,
        two_hand_disabled: bool,
        physical_needed: bool,
    ) -> Result<(ExactRational, ExactRational), String> {
        if stat_idx >= COMBAT_STAT_COUNT {
            return Err(format!(
                "exact formula stat index {stat_idx} is out of range"
            ));
        }
        let old_values =
            stat_values_for_scaling(old_stats, old_effective_str_value, two_hand_disabled);
        let new_values =
            stat_values_for_scaling(new_stats, new_effective_str_value, two_hand_disabled);
        let total = self
            .total
            .delta(stat_idx, old_values[stat_idx], new_values[stat_idx])?;
        let physical = if physical_needed {
            self.components[0].delta(stat_idx, old_values[stat_idx], new_values[stat_idx])?
        } else {
            ExactRational::zero()
        };
        Ok((total, physical))
    }
}

pub(crate) fn compile_ar_formula(
    weapon: &Weapon,
    upgrade: u8,
    data: &GameData,
    max_stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<ExactArFormula, String> {
    let reinforce = reinforce_level(weapon, upgrade, data)?;
    let aec = data
        .attack_element(weapon.attack_element_correct_id)
        .ok_or_else(|| {
            format!(
                "missing attack_element_correct_id={}",
                weapon.attack_element_correct_id
            )
        })?;
    let mut components = std::array::from_fn(|_| ExactFormula::zero(max_stat_values));
    let mut total = ExactFormula::zero(max_stat_values);
    for damage_type in DamageType::ALL {
        let damage_idx = damage_type.as_index();
        let actual_base = rational(weapon.base[damage_idx], "weapon base")?
            * rational(
                reinforce.damage_mult[damage_idx],
                "reinforce damage multiplier",
            )?;
        if actual_base <= ExactRational::zero() {
            continue;
        }
        let mut coefficients = std::array::from_fn(|_| None);
        for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
            if aec.stat_scales(stat_idx, damage_type) {
                *coefficient = Some(
                    rational(weapon.scaling[stat_idx], "weapon scaling")?
                        * rational(
                            reinforce.scaling_mult[stat_idx],
                            "reinforce scaling multiplier",
                        )?,
                );
            }
        }
        let formula = compile_formula(
            actual_base,
            coefficients,
            data,
            weapon.damage_curve_ids[damage_idx],
            max_stat_values,
        )?;
        total.add_assign(formula.clone())?;
        components[damage_idx] = formula;
    }
    Ok(ExactArFormula { components, total })
}

#[derive(Clone, Debug)]
struct ExactScalarHitFormula {
    formula: ExactFormula,
    two_hand_disabled: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ExactScalarRouteFormula {
    multiplier: ExactRational,
    hits: Vec<ExactScalarHitFormula>,
    first_positive_hit: Option<usize>,
}

impl ExactScalarRouteFormula {
    pub(crate) fn has_scaling(&self, first_hit: bool) -> bool {
        self.hits.iter().enumerate().any(|(index, hit)| {
            (!first_hit || self.first_positive_hit == Some(index))
                && hit.formula.terms.iter().any(|terms| !terms.is_empty())
        })
    }

    pub(crate) fn sole_scaling_stat(&self, first_hit: bool) -> Option<usize> {
        let mut stat = None;
        for (hit_index, hit) in self.hits.iter().enumerate() {
            if first_hit && self.first_positive_hit != Some(hit_index) {
                continue;
            }
            for (index, terms) in hit.formula.terms.iter().enumerate() {
                if !terms.is_empty() {
                    if stat.is_some_and(|previous| previous != index) {
                        return None;
                    }
                    stat = Some(index);
                }
            }
        }
        stat
    }

    pub(crate) fn evaluate(
        &self,
        stats: &Stats,
        effective_str_value: u16,
    ) -> Result<(ExactRational, ExactRational), String> {
        let mut first_hit = ExactRational::zero();
        let mut full_sequence = ExactRational::zero();
        for hit in &self.hits {
            let stat_values =
                stat_values_for_scaling(stats, effective_str_value, hit.two_hand_disabled);
            let mut damage = hit.formula.evaluate(stat_values)?;
            if !self.multiplier.is_one() {
                damage *= &self.multiplier;
            }
            if first_hit <= ExactRational::zero() && damage > ExactRational::zero() {
                first_hit = damage.clone();
            }
            if full_sequence.is_zero() {
                full_sequence = damage;
            } else if !damage.is_zero() {
                full_sequence += damage;
            }
        }
        Ok((first_hit, full_sequence))
    }

    pub(crate) fn unscaled_delta(
        &self,
        stat_idx: usize,
        old_stats: &Stats,
        new_stats: &Stats,
        old_effective_str_value: u16,
        new_effective_str_value: u16,
    ) -> Result<(ExactRational, ExactRational), String> {
        if stat_idx >= COMBAT_STAT_COUNT {
            return Err(format!(
                "exact formula stat index {stat_idx} is out of range"
            ));
        }
        // A positive multiplier is common to every allocation and omitted by the DP.
        if self.multiplier.is_zero() {
            return Ok((ExactRational::zero(), ExactRational::zero()));
        }
        let mut first_hit = ExactRational::zero();
        let mut full_sequence = ExactRational::zero();
        for (hit_idx, hit) in self.hits.iter().enumerate() {
            let old_values =
                stat_values_for_scaling(old_stats, old_effective_str_value, hit.two_hand_disabled);
            let new_values =
                stat_values_for_scaling(new_stats, new_effective_str_value, hit.two_hand_disabled);
            let damage = hit
                .formula
                .delta(stat_idx, old_values[stat_idx], new_values[stat_idx])?;
            if self.first_positive_hit == Some(hit_idx) {
                first_hit = damage.clone();
            }
            if full_sequence.is_zero() {
                full_sequence = damage;
            } else if !damage.is_zero() {
                full_sequence += damage;
            }
        }
        Ok((first_hit, full_sequence))
    }
}

pub(crate) fn compile_scalar_route_formula(
    route: &ScalarAowRoute<'_>,
    weapon: &Weapon,
    upgrade: u8,
    damage_multiplier: f32,
    data: &GameData,
    max_stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<ExactScalarRouteFormula, String> {
    let reinforce = reinforce_level(weapon, upgrade, data)?;
    let multiplier = rational(damage_multiplier, "damage multiplier")?;
    let mut hits = Vec::with_capacity(route.hits.len());
    let mut first_positive_hit = None;
    for hit in &route.hits {
        let mut formula = ExactFormula::zero(max_stat_values);
        for damage_type in DamageType::ALL {
            formula.add_assign(compile_skill_formula(
                weapon,
                hit.row,
                reinforce,
                damage_type,
                data,
                max_stat_values,
            )?)?;
            if hit.buff_active {
                formula.base += rational(
                    hit.buff_attack_power[damage_type.as_index()],
                    "Ash attack power buff",
                )? * rational(hit.row.weapon_buff_mv, "weapon buff motion value")?
                    / rational(100.0, "percent denominator")?;
            }
        }
        if first_positive_hit.is_none()
            && multiplier > ExactRational::zero()
            && formula.base > ExactRational::zero()
        {
            first_positive_hit = Some(hits.len());
        }
        hits.push(ExactScalarHitFormula {
            formula,
            two_hand_disabled: hit.row.is_disable_both_hands_bonus,
        });
    }
    Ok(ExactScalarRouteFormula {
        multiplier,
        hits,
        first_positive_hit,
    })
}

pub(crate) fn exact_bleed(
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    data: &GameData,
    aow: Option<&crate::model::Aow>,
) -> Result<ExactRational, String> {
    let base_source = data.weapon_passive(weapon.weapon_id);
    let mut base_bleed = rational(base_source.buildup.bleed, "weapon bleed buildup")?;
    let mut base_flag = base_source.correction_flags.bleed;
    if let Some(overlay) = data.weapon_passive_overlay(weapon.weapon_id, upgrade) {
        let overlay_bleed = rational(overlay.buildup.bleed, "weapon overlay bleed buildup")?;
        if overlay.buildup.bleed > 0.0 {
            base_bleed = overlay_bleed;
            if overlay.correction_flags.bleed.is_some() {
                base_flag = overlay.correction_flags.bleed;
            }
        }
    }

    let mut bleed = if base_bleed <= ExactRational::zero() {
        base_bleed
    } else if !data.rules.status_buildup_scales {
        floor_ratio(&base_bleed)
    } else {
        let reinforce = reinforce_level(weapon, upgrade, data)?;
        floor_ratio(&scale_status_value(
            base_bleed,
            STAT_ARC,
            stats.arc,
            weapon.status_curve_ids.blood,
            base_flag,
            weapon,
            reinforce,
            data,
        )?)
    };

    let Some(aow) = aow else {
        return Ok(bleed);
    };
    bleed += rational(aow.bleed_buildup_add, "Ash bleed buildup addition")?;

    let scaling_values = [
        aow.scaling_status_add.bleed,
        aow.scaling_status_add.frost,
        aow.scaling_status_add.poison,
        aow.scaling_status_add.scarlet_rot,
        aow.scaling_status_add.sleep,
        aow.scaling_status_add.madness,
        aow.scaling_status_add.death,
    ];
    let mut bleed_scaling = ExactRational::zero();
    let mut has_scaling = false;
    for (idx, value) in scaling_values.iter().enumerate() {
        validate_nonnegative_float(*value, "Ash scaling status addition")?;
        has_scaling |= *value > 0.0;
        if idx == 0 && *value > 0.0 {
            bleed_scaling = rational(*value, "Ash scaling status addition")?;
        }
    }

    if !bleed_scaling.is_zero() {
        if !data.rules.status_buildup_scales {
            add_nonzero(&mut bleed, bleed_scaling);
        } else {
            let reinforce = reinforce_level(weapon, upgrade, data)?;
            bleed += scale_status_value(
                bleed_scaling,
                STAT_ARC,
                stats.arc,
                weapon.status_curve_ids.blood,
                aow.scaling_status_flags.bleed,
                weapon,
                reinforce,
                data,
            )?;
        }
    }
    Ok(if has_scaling {
        floor_ratio(&bleed)
    } else {
        bleed
    })
}

pub(crate) fn exact_scalar_route(
    route: &ScalarAowRoute<'_>,
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_multiplier: f32,
    data: &GameData,
) -> Result<(ExactRational, ExactRational), String> {
    let damage_multiplier = rational(damage_multiplier, "damage multiplier")?;
    let mut first_hit = ExactRational::zero();
    let mut full_sequence = ExactRational::zero();

    for hit in &route.hits {
        let mut damage = ExactRational::zero();
        for damage_type in DamageType::ALL {
            add_nonzero(
                &mut damage,
                exact_skill_damage_for_type(
                    weapon,
                    hit.row,
                    upgrade,
                    stats,
                    effective_str_value,
                    damage_type,
                    data,
                )?,
            );
            if hit.buff_active {
                add_nonzero(
                    &mut damage,
                    exact_buff_damage(
                        hit.buff_attack_power[damage_type.as_index()],
                        hit.row.weapon_buff_mv,
                    )?,
                );
            }
        }
        scale_if_needed(&mut damage, &damage_multiplier);
        if first_hit.is_zero() && !damage.is_zero() {
            first_hit = damage.clone();
        }
        add_nonzero(&mut full_sequence, damage);
    }
    Ok((first_hit, full_sequence))
}

pub(crate) fn exact_skill_damage_for_type(
    weapon: &Weapon,
    attack_row: &AowAttackRow,
    upgrade: u8,
    stats: &Stats,
    effective_str_value: u16,
    damage_type: DamageType,
    data: &GameData,
) -> Result<ExactRational, String> {
    let damage_idx = damage_type.as_index();
    let reinforce = reinforce_level(weapon, upgrade, data)?;
    let weapon_base = rational(weapon.base[damage_idx], "weapon base")?;
    let damage_mult = rational(
        reinforce.damage_mult[damage_idx],
        "reinforce damage multiplier",
    )?;
    let motion_value = rational(attack_row.motion_values[damage_idx], "motion value")?;
    let weapon_motion_component =
        if weapon_base.is_zero() || damage_mult.is_zero() || motion_value.is_zero() {
            ExactRational::zero()
        } else {
            multiply_if_needed(multiply_if_needed(weapon_base, damage_mult), motion_value)
                / rational(100.0, "percent denominator")?
        };
    let fixed_attack_component = if attack_row.is_add_base_atk || attack_row.is_arrow_attack {
        let attack_base = rational(attack_row.attack_base[damage_idx], "fixed attack base")?;
        let base_attack_mult = rational(
            reinforce.base_attack_mult,
            "reinforce base attack multiplier",
        )?;
        if attack_base.is_zero() || base_attack_mult.is_zero() {
            ExactRational::zero()
        } else {
            multiply_if_needed(attack_base, base_attack_mult)
        }
    } else {
        ExactRational::zero()
    };
    let actual_base = if weapon_motion_component.is_zero() {
        fixed_attack_component
    } else if fixed_attack_component.is_zero() {
        weapon_motion_component
    } else {
        weapon_motion_component + fixed_attack_component
    };
    if actual_base.is_zero() {
        return Ok(ExactRational::zero());
    }

    let stat_values = stat_values_for_scaling(
        stats,
        effective_str_value,
        attack_row.is_disable_both_hands_bonus,
    );
    let curve_mults = curve_values(data, weapon.damage_curve_ids[damage_idx], stat_values)?;
    let mut coefficients = std::array::from_fn(|_| None);
    if let Some(override_id) = attack_row.overwrite_attack_element_correct_id {
        let aec_ext = data.attack_element_ext(override_id).ok_or_else(|| {
            format!(
                "missing attack_element_correct_ext_id={} for AoW row {} ({})",
                override_id, attack_row.sheet_row, attack_row.raw_name
            )
        })?;
        for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
            if !aec_ext.stat_scales(stat_idx, damage_idx) {
                continue;
            }
            let scaling = aec_ext
                .overwrite_rate(stat_idx, damage_idx)
                .map(|value| rational(value, "attack correction overwrite"))
                .unwrap_or_else(|| {
                    Ok(multiply_if_needed(
                        rational(weapon.scaling[stat_idx], "weapon scaling")?,
                        rational(
                            aec_ext.influence_rate(stat_idx, damage_idx),
                            "attack correction influence",
                        )?,
                    ))
                })?;
            let scaling_mult = rational(
                reinforce.scaling_mult[stat_idx],
                "reinforce scaling multiplier",
            )?;
            *coefficient = (!scaling.is_zero() && !scaling_mult.is_zero())
                .then(|| multiply_if_needed(scaling, scaling_mult));
        }
    } else {
        let aec = data
            .attack_element(weapon.attack_element_correct_id)
            .ok_or_else(|| {
                format!(
                    "missing attack_element_correct_id={}",
                    weapon.attack_element_correct_id
                )
            })?;
        for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
            if aec.stat_scales(stat_idx, damage_type) {
                let scaling = rational(weapon.scaling[stat_idx], "weapon scaling")?;
                let scaling_mult = rational(
                    reinforce.scaling_mult[stat_idx],
                    "reinforce scaling multiplier",
                )?;
                *coefficient = (!scaling.is_zero() && !scaling_mult.is_zero())
                    .then(|| multiply_if_needed(scaling, scaling_mult));
            }
        }
    }
    Ok(apply_scaling(actual_base, &curve_mults, coefficients))
}

fn compile_formula(
    actual_base: ExactRational,
    coefficients: [Option<ExactRational>; COMBAT_STAT_COUNT],
    data: &GameData,
    curve_id: usize,
    max_stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<ExactFormula, String> {
    let mut terms = std::array::from_fn(|_| Vec::new());
    let maximum_stat_value = max_stat_values.iter().copied().max().unwrap_or(0);
    let mut curve_values = Vec::with_capacity(usize::from(maximum_stat_value) + 1);
    for stat_value in 0..=maximum_stat_value {
        let curve_mult = data.calc_curve_value(curve_id, stat_value).ok_or_else(|| {
            let stat_idx = max_stat_values
                .iter()
                .position(|max_value| *max_value >= stat_value)
                .unwrap_or(0);
            format!("missing curve_id={curve_id} for stat index {stat_idx} value {stat_value}")
        })?;
        validate_nonnegative_float(curve_mult, "calc-correct curve")?;
        curve_values.push(curve_mult);
    }

    for stat_idx in 0..COMBAT_STAT_COUNT {
        let max_stat_value = max_stat_values[stat_idx];
        let base_coefficient = coefficients[stat_idx]
            .as_ref()
            .map(|coefficient| actual_base.clone() * coefficient);
        if let Some(coefficient) = base_coefficient.filter(|coefficient| !coefficient.is_zero()) {
            terms[stat_idx].push(ExactScaledCurve {
                curve_id,
                coefficient,
                values: curve_values[..=usize::from(max_stat_value)].to_vec(),
            });
        }
    }
    let formula = ExactFormula {
        base: actual_base,
        terms,
        max_stat_values,
    };
    Ok(formula)
}

fn compile_skill_formula(
    weapon: &Weapon,
    attack_row: &AowAttackRow,
    reinforce: &ReinforceLevel,
    damage_type: DamageType,
    data: &GameData,
    max_stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<ExactFormula, String> {
    let damage_idx = damage_type.as_index();
    let weapon_motion_component = rational(weapon.base[damage_idx], "weapon base")?
        * rational(
            reinforce.damage_mult[damage_idx],
            "reinforce damage multiplier",
        )?
        * rational(attack_row.motion_values[damage_idx], "motion value")?
        / rational(100.0, "percent denominator")?;
    let fixed_attack_component = if attack_row.is_add_base_atk || attack_row.is_arrow_attack {
        rational(attack_row.attack_base[damage_idx], "fixed attack base")?
            * rational(
                reinforce.base_attack_mult,
                "reinforce base attack multiplier",
            )?
    } else {
        ExactRational::zero()
    };
    let actual_base = weapon_motion_component + fixed_attack_component;
    if actual_base <= ExactRational::zero() {
        return Ok(ExactFormula::zero(max_stat_values));
    }

    let mut coefficients = std::array::from_fn(|_| None);
    if let Some(override_id) = attack_row.overwrite_attack_element_correct_id {
        let aec_ext = data.attack_element_ext(override_id).ok_or_else(|| {
            format!(
                "missing attack_element_correct_ext_id={} for AoW row {} ({})",
                override_id, attack_row.sheet_row, attack_row.raw_name
            )
        })?;
        for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
            if !aec_ext.stat_scales(stat_idx, damage_idx) {
                continue;
            }
            let scaling = aec_ext
                .overwrite_rate(stat_idx, damage_idx)
                .map(|value| rational(value, "attack correction overwrite"))
                .unwrap_or_else(|| {
                    Ok(rational(weapon.scaling[stat_idx], "weapon scaling")?
                        * rational(
                            aec_ext.influence_rate(stat_idx, damage_idx),
                            "attack correction influence",
                        )?)
                })?;
            *coefficient = Some(
                scaling
                    * rational(
                        reinforce.scaling_mult[stat_idx],
                        "reinforce scaling multiplier",
                    )?,
            );
        }
    } else {
        let aec = data
            .attack_element(weapon.attack_element_correct_id)
            .ok_or_else(|| {
                format!(
                    "missing attack_element_correct_id={}",
                    weapon.attack_element_correct_id
                )
            })?;
        for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
            if aec.stat_scales(stat_idx, damage_type) {
                *coefficient = Some(
                    rational(weapon.scaling[stat_idx], "weapon scaling")?
                        * rational(
                            reinforce.scaling_mult[stat_idx],
                            "reinforce scaling multiplier",
                        )?,
                );
            }
        }
    }
    compile_formula(
        actual_base,
        coefficients,
        data,
        weapon.damage_curve_ids[damage_idx],
        max_stat_values,
    )
}

fn reinforce_level<'a>(
    weapon: &Weapon,
    upgrade: u8,
    data: &'a GameData,
) -> Result<&'a ReinforceLevel, String> {
    data.reinforce_level(weapon.reinforce_type, upgrade)
        .ok_or_else(|| {
            format!(
                "missing reinforce level: type={} level={upgrade}",
                weapon.reinforce_type
            )
        })
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

fn curve_values(
    data: &GameData,
    curve_id: usize,
    stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<[ExactRational; COMBAT_STAT_COUNT], String> {
    let mut values = std::array::from_fn(|_| ExactRational::zero());
    for stat_idx in 0..COMBAT_STAT_COUNT {
        let value = data
            .calc_curve_value(curve_id, stat_values[stat_idx])
            .ok_or_else(|| format!("missing curve_id={curve_id} for stat index {stat_idx}"))?;
        values[stat_idx] = rational(value, "calc-correct curve")?;
    }
    Ok(values)
}

fn exact_float_difference(new_value: f32, old_value: f32) -> Result<ExactRational, String> {
    validate_nonnegative_float(new_value, "calc-correct curve")?;
    validate_nonnegative_float(old_value, "calc-correct curve")?;
    let (new_mantissa, new_exponent, new_sign) = new_value.integer_decode();
    let (old_mantissa, old_exponent, old_sign) = old_value.integer_decode();
    if (new_mantissa != 0 && new_sign < 0) || (old_mantissa != 0 && old_sign < 0) {
        return Err("negative calc-correct curve".to_string());
    }

    let common_exponent = new_exponent.min(old_exponent);
    let new_shift = u32::try_from(new_exponent - common_exponent).expect("nonnegative shift");
    let old_shift = u32::try_from(old_exponent - common_exponent).expect("nonnegative shift");
    let new_bits = if new_mantissa == 0 {
        0
    } else {
        64 - new_mantissa.leading_zeros()
    };
    let old_bits = if old_mantissa == 0 {
        0
    } else {
        64 - old_mantissa.leading_zeros()
    };
    if new_bits.saturating_add(new_shift) <= 126 && old_bits.saturating_add(old_shift) <= 126 {
        let aligned_new = i128::from(new_mantissa) << new_shift;
        let aligned_old = i128::from(old_mantissa) << old_shift;
        return Ok(ExactRational::from_scaled_i128(
            aligned_new - aligned_old,
            common_exponent,
        ));
    }

    let new_value = rational(new_value, "calc-correct curve")?;
    let old_value = rational(old_value, "calc-correct curve")?;
    Ok(new_value - old_value)
}

fn max_curve_value(
    data: &GameData,
    curve_id: usize,
    min_stat_value: u16,
    max_stat_value: u16,
    stat_idx: usize,
) -> Result<ExactRational, String> {
    let mut maximum = None;
    for stat_value in min_stat_value..=max_stat_value {
        let value = data.calc_curve_value(curve_id, stat_value).ok_or_else(|| {
            format!("missing curve_id={curve_id} for stat index {stat_idx} value {stat_value}")
        })?;
        validate_nonnegative_float(value, "calc-correct curve")?;
        if maximum
            .as_ref()
            .is_none_or(|current: &f32| value > *current)
        {
            maximum = Some(value);
        }
    }
    let maximum = maximum.ok_or_else(|| {
        format!(
            "exact AR upper-bound range is empty for stat index {stat_idx}: {min_stat_value}>{max_stat_value}"
        )
    })?;
    rational(maximum, "calc-correct curve")
}

fn apply_scaling(
    actual_base: ExactRational,
    curve_mults: &[ExactRational; COMBAT_STAT_COUNT],
    coefficients: [Option<ExactRational>; COMBAT_STAT_COUNT],
) -> ExactRational {
    let mut bonus = ExactRational::zero();
    for (coefficient, curve_mult) in coefficients.into_iter().zip(curve_mults) {
        if let Some(coefficient) = coefficient {
            add_nonzero(
                &mut bonus,
                multiply_if_needed(coefficient, curve_mult.clone()),
            );
        }
    }
    if bonus.is_zero() {
        actual_base
    } else {
        multiply_if_needed(actual_base, ExactRational::one() + bonus)
    }
}

fn add_nonzero(target: &mut ExactRational, contribution: ExactRational) {
    if contribution.is_zero() {
        return;
    }
    if target.is_zero() {
        *target = contribution;
    } else {
        *target += contribution;
    }
}

fn multiply_if_needed(left: ExactRational, right: ExactRational) -> ExactRational {
    if left.is_zero() || right.is_zero() {
        ExactRational::zero()
    } else if left.is_one() {
        right
    } else if right.is_one() {
        left
    } else {
        left * right
    }
}

fn scale_if_needed(value: &mut ExactRational, multiplier: &ExactRational) {
    if value.is_zero() || multiplier.is_one() {
        return;
    }
    if multiplier.is_zero() {
        *value = ExactRational::zero();
    } else {
        *value *= multiplier;
    }
}

fn exact_buff_damage(attack_power: f32, weapon_buff_mv: f32) -> Result<ExactRational, String> {
    let attack_power = rational(attack_power, "Ash attack power buff")?;
    let weapon_buff_mv = rational(weapon_buff_mv, "weapon buff motion value")?;
    if attack_power.is_zero() || weapon_buff_mv.is_zero() {
        return Ok(ExactRational::zero());
    }
    Ok(multiply_if_needed(attack_power, weapon_buff_mv) / rational(100.0, "percent denominator")?)
}

#[allow(clippy::too_many_arguments)]
fn scale_status_value(
    value: ExactRational,
    stat_idx: usize,
    stat_value: u8,
    curve_id: usize,
    flag: Option<bool>,
    weapon: &Weapon,
    reinforce: &ReinforceLevel,
    data: &GameData,
) -> Result<ExactRational, String> {
    if value <= ExactRational::zero()
        || !flag.unwrap_or(weapon.scaling[stat_idx] > 0.0)
        || weapon.scaling[stat_idx] <= 0.0
    {
        return Ok(value);
    }
    let curve_mult = data
        .calc_curve_value(curve_id, u16::from(stat_value))
        .ok_or_else(|| format!("missing curve_id={curve_id} for status scaling"))?;
    Ok(value
        * (ExactRational::one()
            + rational(weapon.scaling[stat_idx], "weapon status scaling")?
                * rational(
                    reinforce.scaling_mult[stat_idx],
                    "reinforce status scaling multiplier",
                )?
                * rational(curve_mult, "status calc-correct curve")?))
}

fn floor_ratio(value: &ExactRational) -> ExactRational {
    value.floor()
}

fn validate_nonnegative_float(value: f32, field: &str) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("non-finite {field}"));
    }
    if value < 0.0 {
        return Err(format!("negative {field}: {value}"));
    }
    Ok(())
}

pub(crate) fn rational(value: f32, field: &str) -> Result<ExactRational, String> {
    validate_nonnegative_float(value, field)?;
    // Preserve the source f32 bits; Ratio<i128>::from_f32 uses an approximation.
    let (mut mantissa, mut exponent, sign) = value.integer_decode();
    if mantissa == 0 {
        return Ok(ExactRational::zero());
    }
    if sign < 0 {
        return Err(format!("negative {field}: {value}"));
    }

    let trailing_zeros = mantissa.trailing_zeros();
    mantissa >>= trailing_zeros;
    exponent += trailing_zeros as i16;
    Ok(ExactRational::from_scaled_i128(
        i128::from(mantissa),
        exponent,
    ))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use num_bigint::BigInt;
    use num_rational::BigRational;
    use num_traits::ToPrimitive;

    use crate::data::load_game_data;
    use crate::math::{calculate_ar, calculate_bleed_buildup, effective_str};

    use super::*;

    fn data() -> GameData {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        load_game_data(path).expect("load vanilla data")
    }

    fn weapon<'a>(data: &'a GameData, name: &str, affinity: &str) -> &'a Weapon {
        data.weapons
            .iter()
            .find(|weapon| weapon.name == name && weapon.affinity == affinity)
            .expect("weapon fixture")
    }

    fn binary_f32_rational(value: f32) -> BigRational {
        assert!(value.is_finite());
        let bits = value.to_bits();
        let sign = if bits & (1 << 31) == 0 { 1 } else { -1 };
        let exponent_bits = (bits >> 23) & 0xff;
        let fraction = bits & 0x7f_ffff;
        let (mantissa, exponent) = if exponent_bits == 0 {
            (fraction, -149)
        } else {
            (fraction | (1 << 23), exponent_bits as i32 - 127 - 23)
        };
        let numerator = BigInt::from(sign) * BigInt::from(mantissa);
        if exponent < 0 {
            BigRational::new(numerator, BigInt::from(1_u8) << ((-exponent) as usize))
        } else {
            BigRational::from_integer(numerator << exponent as usize)
        }
    }

    #[test]
    fn f32_conversion_is_exact_and_round_trips() {
        let value = 0.1_f32;
        let converted = rational(value, "test value").expect("finite test value");
        assert_eq!(converted.to_f32(), Some(value));
        assert_eq!(converted.to_big(), binary_f32_rational(value));
        assert_ne!(
            converted,
            ExactRational::new(BigInt::from(1), BigInt::from(10))
        );

        let subnormal = f32::from_bits(1);
        assert_eq!(
            rational(subnormal, "subnormal test value")
                .expect("finite subnormal value")
                .to_big(),
            binary_f32_rational(subnormal),
        );
    }

    #[test]
    fn float_conversion_matches_independent_ieee_decoder() {
        let mut bits = 0x1234_5678_u32;
        let mut checked = 0;
        for _ in 0..10_000 {
            bits = bits.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let value = f32::from_bits(bits & 0x7fff_ffff);
            if !value.is_finite() {
                continue;
            }
            let converted = rational(value, "test value").expect("finite nonnegative value");
            let reference = binary_f32_rational(value);
            assert_eq!(converted.to_big(), reference, "bits={bits:08x}");
            checked += 1;
        }
        assert!(checked > 9_000);
    }

    #[test]
    fn float_difference_matches_exact_rational_subtraction() {
        let pairs = [
            (0.0_f32, 0.0_f32),
            (f32::from_bits(1), 0.0),
            (0.0, f32::from_bits(1)),
            (f32::MIN_POSITIVE, f32::from_bits(0x007f_ffff)),
            (f32::from_bits(0x007f_ffff), f32::MIN_POSITIVE),
            (f32::MAX, 0.0),
            (0.0, f32::MAX),
            (f32::MAX, f32::from_bits(1)),
            (f32::from_bits(1), f32::MAX),
            (1.0, f32::from_bits(1)),
            (f32::from_bits(0x7f7f_ffff), f32::from_bits(0x0080_0000)),
        ];
        for (new_value, old_value) in pairs {
            let actual = exact_float_difference(new_value, old_value).expect("curve values");
            let expected = binary_f32_rational(new_value) - binary_f32_rational(old_value);
            assert_eq!(
                actual.to_big(),
                expected,
                "new={new_value:?} old={old_value:?}"
            );
        }
    }

    #[test]
    fn nonreduced_compiled_table_entry_matches_direct_formula() {
        let mut data = data();
        let curve = data.calc_correct[0].as_mut().expect("curve fixture");
        curve[0] = Some(0.5);
        let actual_base = ExactRational::from_integer(BigInt::from(2));
        let coefficient = ExactRational::new_raw(BigInt::from(2), BigInt::from(6));
        let formula = compile_formula(
            actual_base.clone(),
            [Some(coefficient.clone()), None, None, None, None],
            &data,
            0,
            [0; COMBAT_STAT_COUNT],
        )
        .expect("compiled formula");
        let table_entry = &formula.terms[0][0];
        assert_eq!(table_entry.curve_id, 0);
        assert_eq!(
            table_entry.coefficient,
            ExactRational::new(BigInt::from(2), BigInt::from(3))
        );
        assert_eq!(table_entry.values[0], 0.5);
        let curve_value = rational(table_entry.values[0], "curve").expect("curve");
        assert_eq!(
            &table_entry.coefficient * &curve_value,
            ExactRational::new(BigInt::from(1), BigInt::from(3)),
        );

        let direct = actual_base
            * (ExactRational::one() + coefficient * rational(0.5, "curve").expect("curve"));
        assert_eq!(
            formula
                .evaluate([0; COMBAT_STAT_COUNT])
                .expect("evaluation"),
            direct
        );
    }

    #[test]
    fn exact_ar_projects_to_the_existing_calculator() {
        let data = data();
        let weapon = weapon(&data, "Uchigatana", "Keen");
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
        let exact = exact_ar(
            weapon,
            25,
            &stats,
            effective_str(stats.str, false, weapon.disable_two_hand_bonus),
            &data,
        )
        .expect("exact AR");
        let rounded = calculate_ar(
            weapon,
            25,
            &stats,
            effective_str(stats.str, false, weapon.disable_two_hand_bonus),
            &data,
        )
        .expect("f32 AR");
        let exact_total = exact
            .iter()
            .map(|value| value.to_f32().unwrap())
            .sum::<f32>();
        assert!((exact_total - rounded.total()).abs() < 0.01);
    }

    #[test]
    fn compiled_ar_formula_matches_direct_exact_ar() {
        let data = data();
        let weapon = weapon(&data, "Uchigatana", "Keen");
        let formula = compile_ar_formula(weapon, 25, &data, [148, 99, 99, 99, 99])
            .expect("compile exact AR formula");
        for stats in [
            Stats {
                vig: 10,
                mnd: 10,
                end: 10,
                str: 11,
                dex: 40,
                int: 9,
                fai: 8,
                arc: 8,
            },
            Stats {
                vig: 60,
                mnd: 20,
                end: 40,
                str: 99,
                dex: 99,
                int: 99,
                fai: 99,
                arc: 99,
            },
        ] {
            let effective_str_value = effective_str(stats.str, true, weapon.disable_two_hand_bonus);
            let direct =
                exact_ar(weapon, 25, &stats, effective_str_value, &data).expect("direct exact AR");
            let compiled = formula
                .evaluate(&stats, effective_str_value, weapon.disable_two_hand_bonus)
                .expect("compiled exact AR");
            assert_eq!(compiled, direct);
        }

        let old_stats = Stats {
            vig: 60,
            mnd: 20,
            end: 40,
            str: 98,
            dex: 99,
            int: 99,
            fai: 99,
            arc: 99,
        };
        let new_stats = Stats {
            str: 99,
            ..old_stats
        };
        let old_effective_str_value =
            effective_str(old_stats.str, true, weapon.disable_two_hand_bonus);
        let new_effective_str_value =
            effective_str(new_stats.str, true, weapon.disable_two_hand_bonus);
        let old_exact = exact_ar(weapon, 25, &old_stats, old_effective_str_value, &data)
            .expect("old direct exact AR");
        let new_exact = exact_ar(weapon, 25, &new_stats, new_effective_str_value, &data)
            .expect("new direct exact AR");
        let expected = std::array::from_fn(|index| &new_exact[index] - &old_exact[index]);
        assert_eq!(
            formula
                .delta(
                    crate::model::STAT_STR,
                    &old_stats,
                    &new_stats,
                    old_effective_str_value,
                    new_effective_str_value,
                    weapon.disable_two_hand_bonus,
                )
                .expect("compiled exact AR delta"),
            expected
        );
    }

    #[test]
    fn exact_ar_upper_bound_covers_nonmonotonic_curve_domain() {
        let mut data = data();
        let weapon_idx = data
            .weapons
            .iter()
            .position(|weapon| weapon.name == "Uchigatana" && weapon.affinity == "Keen")
            .expect("weapon fixture");
        let weapon = data.weapons[weapon_idx].clone();
        let curve = data.calc_correct[weapon.damage_curve_ids[0]]
            .as_mut()
            .expect("damage curve fixture");
        curve[10] = Some(1.0);
        curve[11] = Some(9.0);
        curve[12] = Some(2.0);

        let minimum = [10; COMBAT_STAT_COUNT];
        let maximum = [12; COMBAT_STAT_COUNT];
        let upper = exact_ar_upper_bound(&weapon, 25, &data, minimum, maximum)
            .expect("exact AR upper bound");
        let mut exhaustive = std::array::from_fn(|_| ExactRational::zero());
        for str_value in minimum[0]..=maximum[0] {
            for dex_value in minimum[1]..=maximum[1] {
                for int_value in minimum[2]..=maximum[2] {
                    for fai_value in minimum[3]..=maximum[3] {
                        for arc_value in minimum[4]..=maximum[4] {
                            let stats = Stats {
                                vig: 10,
                                mnd: 10,
                                end: 10,
                                str: str_value as u8,
                                dex: dex_value as u8,
                                int: int_value as u8,
                                fai: fai_value as u8,
                                arc: arc_value as u8,
                            };
                            let actual = exact_ar(
                                &weapon,
                                25,
                                &stats,
                                effective_str(stats.str, false, weapon.disable_two_hand_bonus),
                                &data,
                            )
                            .expect("exact AR");
                            for (index, value) in actual.into_iter().enumerate() {
                                if value > exhaustive[index] {
                                    exhaustive[index] = value;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(upper, exhaustive);
        assert!(upper[0] > ExactRational::zero());
    }

    #[test]
    fn exact_inputs_reject_negative_coefficients_but_accept_zero() {
        let data = data();
        let source_weapon = weapon(&data, "Uchigatana", "Keen").clone();
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
        let effective_str_value =
            effective_str(stats.str, false, source_weapon.disable_two_hand_bonus);

        let mut negative_weapon = source_weapon.clone();
        negative_weapon.base[0] = -1.0;
        assert!(exact_ar(&negative_weapon, 25, &stats, effective_str_value, &data,).is_err());
        assert!(compile_ar_formula(&negative_weapon, 25, &data, [148, 99, 99, 99, 99],).is_err());

        let mut zero_weapon = source_weapon.clone();
        zero_weapon.base[0] = 0.0;
        assert!(exact_ar(&zero_weapon, 25, &stats, effective_str_value, &data,).is_ok());
        assert!(compile_ar_formula(&zero_weapon, 25, &data, [148, 99, 99, 99, 99]).is_ok());

        let route = ScalarAowRoute {
            route_id: "empty".to_string(),
            route_priority: 0,
            hits: Vec::new(),
        };
        assert!(
            exact_scalar_route(
                &route,
                &source_weapon,
                25,
                &stats,
                effective_str_value,
                -1.0,
                &data,
            )
            .is_err()
        );
        assert!(
            compile_scalar_route_formula(
                &route,
                &source_weapon,
                25,
                -1.0,
                &data,
                [148, 99, 99, 99, 99],
            )
            .is_err()
        );
        assert!(
            exact_scalar_route(
                &route,
                &source_weapon,
                25,
                &stats,
                effective_str_value,
                0.0,
                &data,
            )
            .is_ok()
        );
        assert!(
            compile_scalar_route_formula(
                &route,
                &source_weapon,
                25,
                0.0,
                &data,
                [148, 99, 99, 99, 99],
            )
            .is_ok()
        );
    }

    #[test]
    fn compiled_scalar_route_formula_matches_direct_exact_route() {
        let data = data();
        let weapon = weapon(&data, "Uchigatana", "Keen");
        let mut row = data
            .aow_attack_rows
            .values()
            .flat_map(|rows| rows.iter())
            .find(|row| row.is_damaging())
            .expect("damaging attack row")
            .clone();
        row.weapon_buff_mv = 100.0;
        let mut zero_row = row.clone();
        zero_row.motion_values = [0.0; DAMAGE_TYPE_COUNT];
        zero_row.attack_base = [0.0; DAMAGE_TYPE_COUNT];
        zero_row.weapon_buff_mv = 100.0;
        let route = ScalarAowRoute {
            route_id: "test".to_string(),
            route_priority: 0,
            hits: vec![
                ScalarAowHit {
                    row: &zero_row,
                    action_order: 0,
                    hit_order: 0,
                    buff_active: true,
                    buff_attack_power: [3.0; DAMAGE_TYPE_COUNT],
                },
                ScalarAowHit {
                    row: &row,
                    action_order: 1,
                    hit_order: 0,
                    buff_active: true,
                    buff_attack_power: [3.0; DAMAGE_TYPE_COUNT],
                },
            ],
        };
        let formula =
            compile_scalar_route_formula(&route, weapon, 25, 2.05, &data, [148, 99, 99, 99, 99])
                .expect("compile exact scalar formula");
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 99,
            dex: 40,
            int: 9,
            fai: 8,
            arc: 8,
        };
        let effective_str_value = effective_str(stats.str, true, weapon.disable_two_hand_bonus);
        let direct =
            exact_scalar_route(&route, weapon, 25, &stats, effective_str_value, 2.05, &data)
                .expect("direct exact scalar route");
        let expected_buff_only = rational(3.0, "buff").expect("buff")
            * ExactRational::from_integer(BigInt::from(DAMAGE_TYPE_COUNT))
            * rational(2.05, "multiplier").expect("multiplier");
        assert_eq!(direct.0, expected_buff_only);
        let compiled = formula
            .evaluate(&stats, effective_str_value)
            .expect("compiled exact scalar route");
        assert_eq!(compiled, direct);

        let old_stats = Stats { str: 98, ..stats };
        let old_effective_str_value =
            effective_str(old_stats.str, true, weapon.disable_two_hand_bonus);
        let old_direct = exact_scalar_route(
            &route,
            weapon,
            25,
            &old_stats,
            old_effective_str_value,
            2.05,
            &data,
        )
        .expect("old direct exact scalar route");
        let expected_delta = (&direct.0 - &old_direct.0, &direct.1 - &old_direct.1);
        let delta = formula
            .unscaled_delta(
                crate::model::STAT_STR,
                &old_stats,
                &stats,
                old_effective_str_value,
                effective_str_value,
            )
            .expect("compiled exact scalar route delta");
        let multiplier = rational(2.05, "damage multiplier").unwrap();
        assert_eq!(
            (delta.0 * &multiplier, delta.1 * &multiplier),
            expected_delta
        );
    }

    #[test]
    fn exact_bleed_preserves_flooring() {
        let data = data();
        let weapon = weapon(&data, "Uchigatana", "Blood");
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 11,
            dex: 40,
            int: 9,
            fai: 8,
            arc: 45,
        };
        let exact = exact_bleed(weapon, 25, &stats, &data, None).expect("exact bleed");
        let rounded = calculate_bleed_buildup(weapon, 25, &stats, &data).expect("f32 bleed");
        assert_eq!(exact.to_f32(), Some(rounded));
        assert_eq!(exact, floor_ratio(&exact));
    }

    #[test]
    fn exact_status_floor_keeps_binary_boundary() {
        let scaled = rational(0.7, "status base").expect("status base")
            * rational(10.0, "status scale").expect("status scale");
        assert_eq!(scaled.floor(), ExactRational::from_integer(BigInt::from(6)));
        assert_eq!(0.7_f32 * 10.0_f32, 7.0);
    }
}
