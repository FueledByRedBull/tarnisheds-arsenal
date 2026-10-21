use std::sync::Arc;

use super::exact_value::ExactRational;
use num_traits::{One, ToPrimitive, Zero, float::FloatCore};

use crate::model::{
    AowAttackRow, AttackElementCorrect, AttackElementCorrectExt, COMBAT_STAT_COUNT,
    DAMAGE_TYPE_COUNT, DamageBreakdown, DamageType, GameData, ReinforceLevel, STAT_ARC, Stats,
    Weapon,
};

use super::{ScalarAowRoute, stat_values_for_scaling};

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

    let aec_ext = data.attack_element_ext(weapon.attack_element_correct_id);
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
        let coefficients = weapon_coefficients(weapon, reinforce, aec, damage_type, aec_ext)?;
        breakdown[damage_idx] = apply_scaling(actual_base, &curve_mults, coefficients);
    }
    Ok(breakdown)
}

/// Curve maxima per damage type and stat, filled on first use; they do not depend on the
/// upgrade, so one cache serves every upgrade of a weapon over the same stat bounds.
pub(crate) type CurveMaxima = [[Option<ExactRational>; COMBAT_STAT_COUNT]; DAMAGE_TYPE_COUNT];

pub(crate) fn exact_ar_upper_bound(
    weapon: &Weapon,
    upgrade: u8,
    data: &GameData,
    min_stat_values: [u16; COMBAT_STAT_COUNT],
    max_stat_values: [u16; COMBAT_STAT_COUNT],
    curve_maxima: &mut CurveMaxima,
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

    let aec_ext = data.attack_element_ext(weapon.attack_element_correct_id);
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
        let coefficients = weapon_coefficients(weapon, reinforce, aec, damage_type, aec_ext)?;
        let mut value = actual_base.clone();
        for (stat_idx, coefficient) in coefficients.into_iter().enumerate() {
            let maximum_curve = match &mut curve_maxima[damage_idx][stat_idx] {
                Some(maximum) => maximum,
                slot => slot.insert(max_curve_value(
                    data,
                    curve_id,
                    min_stat_values[stat_idx],
                    max_stat_values[stat_idx],
                    stat_idx,
                )?),
            };
            if let Some(coefficient) = coefficient {
                value += actual_base.clone() * coefficient * &*maximum_curve;
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
    /// Curve values through the formula's largest stat maximum, shared by every term
    /// and clone of one compilation.
    values: Arc<[f32]>,
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

fn weapon_coefficients(
    weapon: &Weapon,
    reinforce: &ReinforceLevel,
    aec: &AttackElementCorrect,
    damage_type: DamageType,
    aec_ext: Option<&AttackElementCorrectExt>,
) -> Result<[Option<ExactRational>; COMBAT_STAT_COUNT], String> {
    let damage_idx = damage_type.as_index();
    let mut coefficients = std::array::from_fn(|_| None);
    for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
        let contributes = aec_ext.map_or_else(
            || aec.stat_scales(stat_idx, damage_type),
            |ext| ext.stat_scales(stat_idx, damage_idx),
        );
        if !contributes {
            continue;
        }
        let scaling = match aec_ext.and_then(|ext| ext.overwrite_rate(stat_idx, damage_idx)) {
            Some(value) => rational(value, "attack correction overwrite")?,
            None => {
                let mut scaling = rational(weapon.scaling[stat_idx], "weapon scaling")?;
                if let Some(ext) = aec_ext {
                    let influence = ext.influence_rate(stat_idx, damage_idx);
                    if influence != 1.0 {
                        scaling *= rational(influence, "attack correction influence")?;
                    }
                }
                scaling
            }
        };
        let scaling_mult = rational(
            reinforce.scaling_mult[stat_idx],
            "reinforce scaling multiplier",
        )?;
        if !scaling.is_zero() && !scaling_mult.is_zero() {
            *coefficient = Some(multiply_if_needed(scaling, scaling_mult));
        }
    }
    Ok(coefficients)
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
    let aec_ext = data.attack_element_ext(weapon.attack_element_correct_id);
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
        let coefficients = weapon_coefficients(weapon, reinforce, aec, damage_type, aec_ext)?;
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
    /// Non-additive components; only `compile_coupled_route_formula` fills these.
    corrected: Vec<ExactCorrectedTerm>,
    two_hand_disabled: bool,
}

impl ExactScalarHitFormula {
    fn evaluate(&self, stat_values: [u16; COMBAT_STAT_COUNT]) -> Result<ExactRational, String> {
        let mut damage = self.formula.evaluate(stat_values)?;
        for term in &self.corrected {
            add_nonzero(&mut damage, term.evaluate(stat_values)?);
        }
        Ok(damage)
    }
}

/// Route damage as `constant + Σ tables + Σ corrected`, where each table reads one stat value
/// (raw STR for hits that disable the two-handed bonus, otherwise effective STR) and each
/// corrected group adds its most negative entry if one is negative, else all of them.
#[derive(Clone, Debug, Default)]
pub(crate) struct ExactRouteParts {
    pub(crate) constant: ExactRational,
    pub(crate) tables: Vec<ExactStatTable>,
    pub(crate) corrected: Vec<Vec<ExactStatTable>>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExactStatTable {
    pub(crate) stat: usize,
    pub(crate) raw_str: bool,
    pub(crate) values: Vec<ExactRational>,
}

/// A corrected group's value: its most negative entry if one is negative, else their sum.
fn corrected_value(
    group: &[ExactStatTable],
    stats: &Stats,
    effective_str_value: u16,
) -> Result<ExactRational, String> {
    let values = group
        .iter()
        .map(|table| table.value(stats, effective_str_value))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match values.iter().min() {
        Some(minimum) if **minimum < ExactRational::zero() => (*minimum).clone(),
        _ => values.into_iter().cloned().sum(),
    })
}

impl ExactStatTable {
    fn value(&self, stats: &Stats, effective_str_value: u16) -> Result<&ExactRational, String> {
        let stat_value = match self.stat {
            0 if !self.raw_str => effective_str_value,
            stat => stat_values_for_scaling(stats, effective_str_value, true)[stat],
        };
        self.values.get(usize::from(stat_value)).ok_or_else(|| {
            format!(
                "route table for stat {} has no value {stat_value}",
                self.stat
            )
        })
    }
}

impl ExactRouteParts {
    /// The damage at these stats; equals the formula's own evaluation.
    pub(crate) fn evaluate(
        &self,
        stats: &Stats,
        effective_str_value: u16,
    ) -> Result<ExactRational, String> {
        let mut damage = self.constant.clone();
        for table in &self.tables {
            add_nonzero(
                &mut damage,
                table.value(stats, effective_str_value)?.clone(),
            );
        }
        for group in &self.corrected {
            add_nonzero(
                &mut damage,
                corrected_value(group, stats, effective_str_value)?,
            );
        }
        Ok(damage)
    }

    /// The damage change when only `stat` moves: tables and corrected groups that do not
    /// read it keep their values.
    pub(crate) fn delta(
        &self,
        stat: usize,
        old: (&Stats, u16),
        new: (&Stats, u16),
    ) -> Result<ExactRational, String> {
        let mut delta = ExactRational::zero();
        for table in self.tables.iter().filter(|table| table.stat == stat) {
            add_nonzero(
                &mut delta,
                table.value(new.0, new.1)? - table.value(old.0, old.1)?,
            );
        }
        for group in &self.corrected {
            if group.iter().any(|table| table.stat == stat) {
                add_nonzero(
                    &mut delta,
                    corrected_value(group, new.0, new.1)? - corrected_value(group, old.0, old.1)?,
                );
            }
        }
        Ok(delta)
    }

    /// Adds one hit scaled by the route multiplier. A corrected component's
    /// `base * m * (1 + bonus)` becomes the constant `base * m` plus tables of
    /// `base * m * contribution`; with `base * m` positive, the most negative scaled entry is
    /// the scaled most negative contribution, so the selection is unchanged.
    fn add_hit(
        &mut self,
        hit: &ExactScalarHitFormula,
        multiplier: &ExactRational,
    ) -> Result<(), String> {
        self.constant += &hit.formula.base * multiplier;
        let raw_str = |stat: usize| stat == 0 && hit.two_hand_disabled;
        for (stat, terms) in hit.formula.terms.iter().enumerate() {
            for term in terms {
                let index = match self
                    .tables
                    .iter()
                    .position(|table| table.stat == stat && table.raw_str == raw_str(stat))
                {
                    Some(index) => index,
                    None => {
                        self.tables.push(ExactStatTable {
                            stat,
                            raw_str: raw_str(stat),
                            values: vec![ExactRational::zero(); term.values.len()],
                        });
                        self.tables.len() - 1
                    }
                };
                let table = &mut self.tables[index].values;
                if table.len() != term.values.len() {
                    return Err("route terms use different stat ranges".to_string());
                }
                let scaled = &term.coefficient * multiplier;
                for (entry, curve) in table.iter_mut().zip(term.values.iter()) {
                    if *curve != 0.0 {
                        *entry += &scaled * rational(*curve, "calc-correct curve")?;
                    }
                }
            }
        }
        for term in &hit.corrected {
            let scale = &term.base * multiplier;
            self.constant += &scale;
            self.corrected.push(
                term.contributions
                    .iter()
                    .enumerate()
                    .filter_map(|(stat, table)| {
                        table.as_ref().map(|table| ExactStatTable {
                            stat,
                            raw_str: raw_str(stat),
                            values: table
                                .iter()
                                .map(|contribution| &scale * contribution)
                                .collect(),
                        })
                    })
                    .collect(),
            );
        }
        Ok(())
    }
}

/// One damage component of a skill row with a non-additive correction (optimizer-math
/// Section 3): `base * (1 + bonus)`, where the bonus is the most negative per-stat
/// contribution when one is negative and their sum otherwise. Each contribution reads one
/// stat, so it is tabulated per stat value once and only the selection runs per allocation.
#[derive(Clone, Debug)]
struct ExactCorrectedTerm {
    base: ExactRational,
    contributions: [Option<Arc<[ExactRational]>>; COMBAT_STAT_COUNT],
}

impl ExactCorrectedTerm {
    fn evaluate(&self, stat_values: [u16; COMBAT_STAT_COUNT]) -> Result<ExactRational, String> {
        let mut total = ExactRational::zero();
        let mut minimum = ExactRational::zero();
        for (stat_idx, table) in self.contributions.iter().enumerate() {
            let Some(table) = table else {
                continue;
            };
            let contribution = table
                .get(usize::from(stat_values[stat_idx]))
                .ok_or_else(|| {
                    format!(
                        "corrected skill term stat index {stat_idx} is missing value {}",
                        stat_values[stat_idx]
                    )
                })?;
            if *contribution < minimum {
                minimum = contribution.clone();
            }
            total += contribution;
        }
        let bonus = if minimum < ExactRational::zero() {
            minimum
        } else {
            total
        };
        Ok(&self.base * (ExactRational::one() + bonus))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExactScalarRouteFormula {
    multiplier: ExactRational,
    hits: Vec<ExactScalarHitFormula>,
    first_positive_hit: Option<usize>,
    full: Vec<ExactScalarHitFormula>,
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
            let mut damage = hit.evaluate(stat_values)?;
            if !self.multiplier.is_one() {
                damage *= &self.multiplier;
            }
            if first_hit <= ExactRational::zero() && damage > ExactRational::zero() {
                first_hit = damage.clone();
            }
            if !self.full.is_empty() {
                if first_hit > ExactRational::zero() {
                    break;
                }
                continue;
            }
            if full_sequence.is_zero() {
                full_sequence = damage;
            } else if !damage.is_zero() {
                full_sequence += damage;
            }
        }
        for hit in &self.full {
            let values = stat_values_for_scaling(stats, effective_str_value, hit.two_hand_disabled);
            let mut damage = hit.evaluate(values)?;
            scale_if_needed(&mut damage, &self.multiplier);
            add_nonzero(&mut full_sequence, damage);
        }
        Ok((first_hit, full_sequence))
    }

    /// The first-hit and full-sequence damage as tabulated parts. `None` when the first
    /// damaging hit could depend on the stats: a hit with scaling terms but no base, a
    /// correction that could cancel a hit, or a multiplier that is not positive.
    pub(crate) fn parts(&self) -> Result<Option<(ExactRouteParts, ExactRouteParts)>, String> {
        if self.multiplier <= ExactRational::zero() {
            return Ok(None);
        }
        let minus_one = ExactRational::zero() - ExactRational::one();
        let mut first = None;
        let mut full = ExactRouteParts::default();
        for hit in &self.hits {
            // A corrected component is base * (1 + bonus) with the bonus at least its most
            // negative contribution, so it stays positive while every contribution is above -1.
            let corrected_positive = hit.corrected.iter().all(|term| {
                term.contributions
                    .iter()
                    .flatten()
                    .flat_map(|table| table.iter())
                    .all(|contribution| *contribution > minus_one)
            });
            let has_terms = hit.formula.terms.iter().any(|terms| !terms.is_empty());
            let positive = hit.formula.base > ExactRational::zero() || !hit.corrected.is_empty();
            if !corrected_positive || !positive && has_terms {
                return Ok(None);
            }
            full.add_hit(hit, &self.multiplier)?;
            if first.is_none() && positive {
                let mut parts = ExactRouteParts::default();
                parts.add_hit(hit, &self.multiplier)?;
                first = Some(parts);
            }
        }
        Ok(Some((first.unwrap_or_default(), full)))
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
        if self.hits.iter().any(|hit| !hit.corrected.is_empty()) {
            return Err("a non-additive route has no separable stat delta".to_string());
        }
        // A positive multiplier is common to every allocation and omitted by the DP.
        if self.multiplier.is_zero() {
            return Ok((ExactRational::zero(), ExactRational::zero()));
        }
        let delta = |hit: &ExactScalarHitFormula| {
            let old_values =
                stat_values_for_scaling(old_stats, old_effective_str_value, hit.two_hand_disabled);
            let new_values =
                stat_values_for_scaling(new_stats, new_effective_str_value, hit.two_hand_disabled);
            hit.formula
                .delta(stat_idx, old_values[stat_idx], new_values[stat_idx])
        };
        let mut first_hit = ExactRational::zero();
        if !self.full.is_empty()
            && let Some(index) = self.first_positive_hit
        {
            first_hit = delta(&self.hits[index])?;
        }
        let mut full_sequence = ExactRational::zero();
        let terms = if self.full.is_empty() {
            &self.hits
        } else {
            &self.full
        };
        for (index, hit) in terms.iter().enumerate() {
            let damage = delta(hit)?;
            if self.full.is_empty() && self.first_positive_hit == Some(index) {
                first_hit = damage.clone();
            }
            add_nonzero(&mut full_sequence, damage);
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
            corrected: Vec::new(),
            two_hand_disabled: hit.row.is_disable_both_hands_bonus,
        });
    }
    // Compilation has already resolved branches, throw scaling and buff timing.
    // ExactFormula merges stat/curve identities; raw and effective STR stay apart.
    let mut full: Vec<ExactScalarHitFormula> = Vec::new();
    if hits.len() > 1 {
        for hit in &hits {
            if let Some(current) = full
                .iter_mut()
                .find(|entry| entry.two_hand_disabled == hit.two_hand_disabled)
            {
                current.formula.add_assign(hit.formula.clone())?;
            } else {
                full.push(hit.clone());
            }
        }
    }
    Ok(ExactScalarRouteFormula {
        multiplier,
        hits,
        first_positive_hit,
        full,
    })
}

/// Compiles a route that may carry non-additive corrections. Additive components and buffs
/// merge into each hit's formula as in `compile_scalar_route_formula`; a corrected component
/// keeps its base and per-stat contribution tables, so evaluation equals `exact_scalar_route`
/// for every allocation within `max_stat_values`. Only evaluation is supported:
/// `unscaled_delta` refuses corrected terms.
pub(crate) fn compile_coupled_route_formula(
    route: &ScalarAowRoute<'_>,
    weapon: &Weapon,
    upgrade: u8,
    damage_multiplier: f32,
    data: &GameData,
    max_stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<ExactScalarRouteFormula, String> {
    let reinforce = reinforce_level(weapon, upgrade, data)?;
    let mut hits = Vec::with_capacity(route.hits.len());
    for hit in &route.hits {
        let mut formula = ExactFormula::zero(max_stat_values);
        let mut corrected = Vec::new();
        for damage_type in DamageType::ALL {
            match compile_corrected_skill_term(
                weapon,
                hit.row,
                reinforce,
                damage_type,
                data,
                max_stat_values,
            )? {
                Some(term) => corrected.extend(term),
                None => formula.add_assign(compile_skill_formula(
                    weapon,
                    hit.row,
                    reinforce,
                    damage_type,
                    data,
                    max_stat_values,
                )?)?,
            }
            if hit.buff_active {
                formula.base += rational(
                    hit.buff_attack_power[damage_type.as_index()],
                    "Ash attack power buff",
                )? * rational(hit.row.weapon_buff_mv, "weapon buff motion value")?
                    / rational(100.0, "percent denominator")?;
            }
        }
        hits.push(ExactScalarHitFormula {
            formula,
            corrected,
            two_hand_disabled: hit.row.is_disable_both_hands_bonus,
        });
    }
    Ok(ExactScalarRouteFormula {
        multiplier: rational(damage_multiplier, "damage multiplier")?,
        hits,
        first_positive_hit: None,
        full: Vec::new(),
    })
}

/// The corrected branch of `skill_damage_with_curves` for one damage type: `None` when the
/// component is additive (fixed damage, or no scaling stat with an influence other than 1),
/// `Some(None)` when its base is zero.
fn compile_corrected_skill_term(
    weapon: &Weapon,
    attack_row: &AowAttackRow,
    reinforce: &ReinforceLevel,
    damage_type: DamageType,
    data: &GameData,
    max_stat_values: [u16; COMBAT_STAT_COUNT],
) -> Result<Option<Option<ExactCorrectedTerm>>, String> {
    let damage_idx = damage_type.as_index();
    let Some(override_id) = attack_row.overwrite_attack_element_correct_id else {
        return Ok(None);
    };
    if attack_row.has_fixed_damage(&data.profile_id) {
        return Ok(None);
    }
    let aec_ext = data.attack_element_ext(override_id).ok_or_else(|| {
        format!(
            "missing attack_element_correct_ext_id={} for AoW row {} ({})",
            override_id, attack_row.sheet_row, attack_row.raw_name
        )
    })?;
    if (0..COMBAT_STAT_COUNT).all(|stat_idx| {
        !aec_ext.stat_scales(stat_idx, damage_idx)
            || aec_ext.influence_rate(stat_idx, damage_idx) == 1.0
    }) {
        return Ok(None);
    }
    let mut base = rational(weapon.base[damage_idx], "weapon base")?
        * rational(
            reinforce.damage_mult[damage_idx],
            "reinforce damage multiplier",
        )?
        * rational(attack_row.motion_values[damage_idx], "motion value")?
        / rational(100.0, "percent denominator")?;
    if attack_row.uses_fixed_attack_base() {
        base += rational(attack_row.attack_base[damage_idx], "fixed attack base")?
            * rational(
                reinforce.base_attack_mult,
                "reinforce base attack multiplier",
            )?;
    }
    apply_throw_multiplier(&mut base, weapon, attack_row);
    if base.is_zero() {
        return Ok(Some(None));
    }
    let curve_id = weapon.damage_curve_ids[damage_idx];
    let mut contributions = std::array::from_fn(|_| None);
    for (stat_idx, table) in contributions.iter_mut().enumerate() {
        if !aec_ext.stat_scales(stat_idx, damage_idx) {
            continue;
        }
        let influence = rational(
            aec_ext.influence_rate(stat_idx, damage_idx),
            "attack correction influence",
        )?;
        let coefficient = rational(
            aec_ext
                .overwrite_rate(stat_idx, damage_idx)
                .unwrap_or(weapon.scaling[stat_idx]),
            "attack correction scaling",
        )? * rational(
            reinforce.scaling_mult[stat_idx],
            "reinforce scaling multiplier",
        )? * &influence;
        let offset = influence - ExactRational::one();
        let values = (0..=max_stat_values[stat_idx])
            .map(|stat_value| {
                let curve = data.calc_curve_value(curve_id, stat_value).ok_or_else(|| {
                    format!("missing curve_id={curve_id} for stat index {stat_idx}")
                })?;
                Ok(&offset + &coefficient * rational(curve, "calc-correct curve")?)
            })
            .collect::<Result<Vec<_>, String>>()?;
        *table = Some(values.into());
    }
    Ok(Some(Some(ExactCorrectedTerm {
        base,
        contributions,
    })))
}

pub(crate) fn exact_bleed(
    weapon: &Weapon,
    upgrade: u8,
    stats: &Stats,
    data: &GameData,
    aow: Option<&crate::model::Aow>,
) -> Result<ExactRational, String> {
    let base_source = data.weapon_passive(weapon.weapon_id);
    let mut base_bleed = base_source.buildup.bleed;
    validate_nonnegative_float(base_bleed, "weapon bleed buildup")?;
    let mut base_flag = base_source.correction_flags.bleed;
    if let Some(overlay) = data.weapon_passive_overlay(weapon.weapon_id, upgrade) {
        validate_nonnegative_float(overlay.buildup.bleed, "weapon overlay bleed buildup")?;
        super::merge_status_value(
            &mut base_bleed,
            &mut base_flag,
            overlay.buildup.bleed,
            overlay.correction_flags.bleed,
        );
    }

    // Status amounts round in f32 before their gameplay floors. The resulting
    // ARC-only value is then exact for DP and cross-loadout comparisons.
    let scaled_bleed = if base_bleed > 0.0 && data.rules.status_buildup_scales {
        let reinforce = reinforce_level(weapon, upgrade, data)?;
        super::scale_status_value(
            base_bleed,
            STAT_ARC,
            stats.arc,
            weapon.status_curve_ids.blood,
            base_flag,
            weapon,
            reinforce,
            data,
        )?
    } else {
        base_bleed
    };
    let bleed = rational(scaled_bleed, "weapon bleed buildup")?.floor();

    let Some(aow) = aow else {
        return Ok(bleed);
    };
    validate_nonnegative_float(aow.bleed_buildup_add, "Ash bleed buildup addition")?;
    for value in [
        aow.scaling_status_add.bleed,
        aow.scaling_status_add.frost,
        aow.scaling_status_add.poison,
        aow.scaling_status_add.scarlet_rot,
        aow.scaling_status_add.sleep,
        aow.scaling_status_add.madness,
        aow.scaling_status_add.death,
    ] {
        validate_nonnegative_float(value, "Ash scaling status addition")?;
    }
    rational(
        super::apply_aow_bleed_buffs(project(&bleed)?, weapon, upgrade, stats, data, Some(aow))?,
        "buffed bleed buildup",
    )
}

/// With `arc_status_curve_nonnegative` for ARC up to some `max_arc`: whether no ranking-key
/// component at `lower` can exceed its value at `upper`, at any allocation (ARC up to
/// `max_arc`), Ash, or route with positive corrections. Only the reinforce row, the passive
/// overlay and its correction flag depend on the upgrade, and no stat budget changes them.
///
/// AR and route damage: every base is a sum of products of non-negative loaded values with
/// one reinforce damage or base-attack multiplier. An additive scaling term multiplies that
/// base by a non-negative scaling, a reinforce scaling multiplier and a non-negative curve
/// value. A corrected component is `base * (1 + bonus)`, where each contribution
/// `(influence - 1) + scaling * multiplier * curve * influence` grows with the multiplier and
/// the bonus (the most negative contribution, else their sum) grows with each contribution;
/// positive influence keeps `1 + bonus` positive. Multipliers no larger therefore keep every
/// value no larger, and the same zero pattern keeps the same hits positive, so the first
/// damaging hit is the same hit at both upgrades. Bleed: with the flag unchanged, f32 products
/// and sums of non-negative operands (rounding preserves order), the projection and the floors
/// are monotone in the buildup and the ARC multiplier.
pub(crate) fn upgrade_inputs_dominated(
    weapon: &Weapon,
    lower: u8,
    upper: u8,
    data: &GameData,
) -> bool {
    let (Some(lower_row), Some(upper_row)) = (
        data.reinforce_level(weapon.reinforce_type, lower),
        data.reinforce_level(weapon.reinforce_type, upper),
    ) else {
        return false;
    };
    let multiplier_dominated =
        |low: f32, high: f32| (0.0..=high).contains(&low) && (low == 0.0) == (high == 0.0);
    let reinforce_dominated = lower_row
        .damage_mult
        .iter()
        .chain(&lower_row.scaling_mult)
        .chain([&lower_row.base_attack_mult])
        .zip(
            upper_row
                .damage_mult
                .iter()
                .chain(&upper_row.scaling_mult)
                .chain([&upper_row.base_attack_mult]),
        )
        .all(|(&low, &high)| multiplier_dominated(low, high));
    let passive = |upgrade| {
        let source = data.weapon_passive(weapon.weapon_id);
        let mut value = source.buildup.bleed;
        let mut flag = source.correction_flags.bleed;
        if let Some(overlay) = data.weapon_passive_overlay(weapon.weapon_id, upgrade) {
            super::merge_status_value(
                &mut value,
                &mut flag,
                overlay.buildup.bleed,
                overlay.correction_flags.bleed,
            );
        }
        (value, flag)
    };
    let ((lower_bleed, lower_flag), (upper_bleed, upper_flag)) = (passive(lower), passive(upper));
    reinforce_dominated && lower_flag == upper_flag && (0.0..=upper_bleed).contains(&lower_bleed)
}

/// The ARC condition of upgrade dominance, which no upgrade changes.
pub(crate) fn arc_status_curve_nonnegative(weapon: &Weapon, max_arc: u16, data: &GameData) -> bool {
    let curve = weapon.status_curve_ids.blood;
    weapon.scaling[STAT_ARC] >= 0.0
        && (0..=max_arc).all(|arc| {
            data.calc_curve_value(curve, arc)
                .is_some_and(|value| value >= 0.0)
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

/// Bounds a route over independent stat intervals; the STR interval must cover raw and
/// effective STR. Each damage component is nondecreasing in every curve value (signs from
/// optimizer-math Section 3; an influence penalty only switches from the strongest negative
/// contribution to the nonnegative sum as values rise), so evaluating every stat at its
/// interval's largest curve value bounds coupled and additive routes alike.
#[allow(clippy::too_many_arguments)]
pub(crate) fn exact_scalar_route_upper_bound(
    route: &ScalarAowRoute<'_>,
    weapon: &Weapon,
    upgrade: u8,
    damage_multiplier: f32,
    data: &GameData,
    min_stat_values: [u16; COMBAT_STAT_COUNT],
    max_stat_values: [u16; COMBAT_STAT_COUNT],
    curve_maxima: &mut CurveMaxima,
) -> Result<(ExactRational, ExactRational), String> {
    let damage_multiplier = rational(damage_multiplier, "damage multiplier")?;
    let mut first_hit = ExactRational::zero();
    let mut full_sequence = ExactRational::zero();
    for hit in &route.hits {
        let mut damage = ExactRational::zero();
        for damage_type in DamageType::ALL {
            add_nonzero(
                &mut damage,
                skill_damage_with_curves(
                    weapon,
                    hit.row,
                    upgrade,
                    damage_type,
                    data,
                    |curve_id| {
                        let mut values = std::array::from_fn(|_| ExactRational::zero());
                        let maxima = &mut curve_maxima[damage_type.as_index()];
                        for (stat_idx, value) in values.iter_mut().enumerate() {
                            *value = match &maxima[stat_idx] {
                                Some(maximum) => maximum.clone(),
                                None => maxima[stat_idx]
                                    .insert(max_curve_value(
                                        data,
                                        curve_id,
                                        min_stat_values[stat_idx],
                                        max_stat_values[stat_idx],
                                        stat_idx,
                                    )?)
                                    .clone(),
                            };
                        }
                        Ok(values)
                    },
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
        // The first nonzero hit may vary with stats; zero bounds a route without one.
        first_hit = first_hit.max(damage.clone());
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
    let stat_values = stat_values_for_scaling(
        stats,
        effective_str_value,
        attack_row.is_disable_both_hands_bonus,
    );
    skill_damage_with_curves(weapon, attack_row, upgrade, damage_type, data, |curve_id| {
        curve_values(data, curve_id, stat_values)
    })
}

fn skill_damage_with_curves(
    weapon: &Weapon,
    attack_row: &AowAttackRow,
    upgrade: u8,
    damage_type: DamageType,
    data: &GameData,
    curves: impl FnOnce(usize) -> Result<[ExactRational; COMBAT_STAT_COUNT], String>,
) -> Result<ExactRational, String> {
    let damage_idx = damage_type.as_index();
    let reinforce = reinforce_level(weapon, upgrade, data)?;
    if attack_row.has_fixed_damage(&data.profile_id) {
        return rational(
            attack_row.attack_base[damage_idx],
            "fixed unscaled skill damage",
        );
    }
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
    let fixed_attack_component = if attack_row.uses_fixed_attack_base() {
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
    let mut actual_base = if weapon_motion_component.is_zero() {
        fixed_attack_component
    } else if fixed_attack_component.is_zero() {
        weapon_motion_component
    } else {
        weapon_motion_component + fixed_attack_component
    };
    apply_throw_multiplier(&mut actual_base, weapon, attack_row);
    if actual_base.is_zero() {
        return Ok(ExactRational::zero());
    }

    let curve_mults = curves(weapon.damage_curve_ids[damage_idx])?;
    let coefficients;
    if let Some(override_id) = attack_row.overwrite_attack_element_correct_id {
        let aec_ext = data.attack_element_ext(override_id).ok_or_else(|| {
            format!(
                "missing attack_element_correct_ext_id={} for AoW row {} ({})",
                override_id, attack_row.sheet_row, attack_row.raw_name
            )
        })?;
        // Influence changes the whole contribution, including its constant term.
        // A negative contribution selects the strongest penalty instead of adding bonuses.
        let mut total = ExactRational::zero();
        let mut minimum = ExactRational::zero();
        for (stat_idx, curve) in curve_mults.iter().enumerate() {
            if !aec_ext.stat_scales(stat_idx, damage_idx) {
                continue;
            }
            let influence = rational(
                aec_ext.influence_rate(stat_idx, damage_idx),
                "attack correction influence",
            )?;
            let scaling = rational(
                aec_ext
                    .overwrite_rate(stat_idx, damage_idx)
                    .unwrap_or(weapon.scaling[stat_idx]),
                "attack correction scaling",
            )?;
            let contribution = influence.clone() - ExactRational::one()
                + scaling
                    * rational(
                        reinforce.scaling_mult[stat_idx],
                        "reinforce scaling multiplier",
                    )?
                    * curve
                    * influence;
            minimum = minimum.min(contribution.clone());
            total += contribution;
        }
        let bonus = if minimum < ExactRational::zero() {
            minimum
        } else {
            total
        };
        return Ok(actual_base * (ExactRational::one() + bonus));
    } else {
        let aec = data
            .attack_element(weapon.attack_element_correct_id)
            .ok_or_else(|| {
                format!(
                    "missing attack_element_correct_id={}",
                    weapon.attack_element_correct_id
                )
            })?;
        coefficients = weapon_coefficients(
            weapon,
            reinforce,
            aec,
            damage_type,
            data.attack_element_ext(weapon.attack_element_correct_id),
        )?;
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
    let curve_values: Arc<[f32]> = curve_values.into();

    for stat_idx in 0..COMBAT_STAT_COUNT {
        let base_coefficient = coefficients[stat_idx]
            .as_ref()
            .map(|coefficient| actual_base.clone() * coefficient);
        if let Some(coefficient) = base_coefficient.filter(|coefficient| !coefficient.is_zero()) {
            terms[stat_idx].push(ExactScaledCurve {
                curve_id,
                coefficient,
                values: Arc::clone(&curve_values),
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
    if attack_row.has_fixed_damage(&data.profile_id) {
        let mut formula = ExactFormula::zero(max_stat_values);
        formula.base = rational(
            attack_row.attack_base[damage_idx],
            "fixed unscaled skill damage",
        )?;
        return Ok(formula);
    }
    let weapon_motion_component = rational(weapon.base[damage_idx], "weapon base")?
        * rational(
            reinforce.damage_mult[damage_idx],
            "reinforce damage multiplier",
        )?
        * rational(attack_row.motion_values[damage_idx], "motion value")?
        / rational(100.0, "percent denominator")?;
    let fixed_attack_component = if attack_row.uses_fixed_attack_base() {
        rational(attack_row.attack_base[damage_idx], "fixed attack base")?
            * rational(
                reinforce.base_attack_mult,
                "reinforce base attack multiplier",
            )?
    } else {
        ExactRational::zero()
    };
    let mut actual_base = weapon_motion_component + fixed_attack_component;
    apply_throw_multiplier(&mut actual_base, weapon, attack_row);
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
            if aec_ext.influence_rate(stat_idx, damage_idx) != 1.0 {
                return Err("non-additive skill correction requires direct evaluation".to_string());
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
        coefficients = weapon_coefficients(
            weapon,
            reinforce,
            aec,
            damage_type,
            data.attack_element_ext(weapon.attack_element_correct_id),
        )?;
    }
    compile_formula(
        actual_base,
        coefficients,
        data,
        weapon.damage_curve_ids[damage_idx],
        max_stat_values,
    )
}

fn apply_throw_multiplier(damage: &mut ExactRational, weapon: &Weapon, row: &AowAttackRow) {
    if row.is_throw_attack && weapon.critical_damage_percent != 100 {
        *damage *= ExactRational::from_integer(weapon.critical_damage_percent.into())
            / ExactRational::from_integer(100.into());
    }
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

pub(crate) fn exact_buff_damage(
    attack_power: f32,
    weapon_buff_mv: f32,
) -> Result<ExactRational, String> {
    let attack_power = rational(attack_power, "Ash attack power buff")?;
    let weapon_buff_mv = rational(weapon_buff_mv, "weapon buff motion value")?;
    if attack_power.is_zero() || weapon_buff_mv.is_zero() {
        return Ok(ExactRational::zero());
    }
    Ok(multiply_if_needed(attack_power, weapon_buff_mv) / rational(100.0, "percent denominator")?)
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

pub(crate) fn project(value: &ExactRational) -> Result<f32, String> {
    value
        .to_f32()
        .filter(|value| value.is_finite())
        .ok_or_else(|| "exact score is outside the display range".to_string())
}

pub(crate) fn sum_components(values: &[ExactRational; DAMAGE_TYPE_COUNT]) -> ExactRational {
    values
        .iter()
        .filter(|value| !value.is_zero())
        .cloned()
        .reduce(|left, right| left + right)
        .unwrap_or_else(ExactRational::zero)
}

pub(crate) fn project_damage(
    values: &[ExactRational; DAMAGE_TYPE_COUNT],
) -> Result<DamageBreakdown, String> {
    Ok(DamageBreakdown {
        physical: project(&values[DamageType::Physical.as_index()])?,
        magic: project(&values[DamageType::Magic.as_index()])?,
        fire: project(&values[DamageType::Fire.as_index()])?,
        lightning: project(&values[DamageType::Lightning.as_index()])?,
        holy: project(&values[DamageType::Holy.as_index()])?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use num_bigint::BigInt;
    use num_rational::BigRational;
    use num_traits::ToPrimitive;

    use crate::data::load_game_data;
    use crate::math::{
        calculate_ar, calculate_bleed_buildup, calculate_skill_damage_for_type, effective_str,
    };

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
    fn compiled_ar_formula_matches_direct_exact_ar_for_every_weapon_and_profile() {
        for profile in [
            crate::data::VANILLA_PROFILE_ID,
            crate::data::CONVERGENCE_PROFILE_ID,
        ] {
            let data = crate::data::load_embedded_game_profile(profile).expect("profile");
            let mut checked = 0;
            for weapon in data
                .weapons
                .iter()
                .filter(|weapon| data.weapon_ar_supported(weapon))
            {
                let top = if weapon.is_somber {
                    data.rules.somber_max_upgrade
                } else {
                    data.rules.standard_max_upgrade
                };
                for upgrade in [0, top].into_iter().filter(|upgrade| {
                    data.reinforce_level(weapon.reinforce_type, *upgrade)
                        .is_some()
                }) {
                    let formula = compile_ar_formula(weapon, upgrade, &data, [148, 99, 99, 99, 99])
                        .expect("compile exact AR formula");
                    for value in [0, 1, 18, 60, 99] {
                        let stats = Stats {
                            vig: 10,
                            mnd: 10,
                            end: 10,
                            str: value,
                            dex: value,
                            int: value,
                            fai: value,
                            arc: value,
                        };
                        for two_handing in [false, true] {
                            let effective =
                                effective_str(value, two_handing, weapon.disable_two_hand_bonus);
                            let direct = exact_ar(weapon, upgrade, &stats, effective, &data)
                                .expect("direct exact AR");
                            let compiled = formula
                                .evaluate(&stats, effective, weapon.disable_two_hand_bonus)
                                .expect("compiled exact AR");
                            assert_eq!(
                                compiled, direct,
                                "{profile} {} {} +{upgrade} stats={value} two_handing={two_handing}",
                                weapon.name, weapon.affinity
                            );
                            checked += 1;
                        }
                    }
                }
            }
            assert!(checked > 50_000, "{profile}: {checked}");
        }
    }

    #[test]
    fn compiled_routes_match_direct_routes_for_every_vanilla_ash() {
        let data = data();
        let mut checked = 0;
        for aow in &data.aows {
            let Some(weapon) = data
                .weapons
                .iter()
                .find(|weapon| !weapon.is_somber && data.aow_compatible_with_weapon(aow, weapon))
            else {
                continue;
            };
            let rows: Vec<_> = data.aow_attack_rows(aow.aow_id).iter().collect();
            let Some(routes) = crate::math::prepare_scalar_aow_routes(&rows, &data).unwrap() else {
                continue;
            };
            for route in routes.iter().filter(|route| route.is_additive(&data)) {
                let formula = compile_scalar_route_formula(
                    route,
                    weapon,
                    25,
                    1.5,
                    &data,
                    [148, 99, 99, 99, 99],
                )
                .expect("compile route");
                for value in [1, 40, 99] {
                    let stats = Stats {
                        vig: 10,
                        mnd: 10,
                        end: 10,
                        str: value,
                        dex: value,
                        int: value,
                        fai: value,
                        arc: value,
                    };
                    let effective = effective_str(value, true, weapon.disable_two_hand_bonus);
                    assert_eq!(
                        formula.evaluate(&stats, effective).unwrap(),
                        exact_scalar_route(route, weapon, 25, &stats, effective, 1.5, &data)
                            .unwrap(),
                        "ash={} route={} value={value}",
                        aow.aow_id,
                        route.route_id
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 300, "{checked}");
    }

    #[test]
    fn dominated_upgrades_never_exceed_the_next_upgrade() {
        let stat_vectors = [1, 18, 40, 60, 99].map(|value| Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: value,
            dex: value,
            int: value,
            fai: value,
            arc: value,
        });
        for profile in [
            crate::data::VANILLA_PROFILE_ID,
            crate::data::CONVERGENCE_PROFILE_ID,
        ] {
            let data = crate::data::load_embedded_game_profile(profile).expect("profile");
            let pairs = |weapon: &Weapon| {
                let top = if weapon.is_somber {
                    data.rules.somber_max_upgrade
                } else {
                    data.rules.standard_max_upgrade
                };
                (0..top)
                    .filter(|&lower| {
                        upgrade_inputs_dominated(weapon, lower, lower + 1, &data)
                            && arc_status_curve_nonnegative(weapon, 99, &data)
                    })
                    .collect::<Vec<_>>()
            };
            let (mut dominated, mut checked) = (0, 0);
            for weapon in data
                .weapons
                .iter()
                .filter(|weapon| data.weapon_ar_supported(weapon))
            {
                for lower in pairs(weapon) {
                    dominated += 1;
                    for stats in &stat_vectors {
                        let effective =
                            effective_str(stats.str, true, weapon.disable_two_hand_bonus);
                        let low = exact_ar(weapon, lower, stats, effective, &data).unwrap();
                        let high = exact_ar(weapon, lower + 1, stats, effective, &data).unwrap();
                        assert!(
                            low.iter().zip(&high).all(|(low, high)| low <= high),
                            "{profile} {} +{lower} AR {stats:?}",
                            weapon.name
                        );
                        assert!(
                            exact_bleed(weapon, lower, stats, &data, None).unwrap()
                                <= exact_bleed(weapon, lower + 1, stats, &data, None).unwrap(),
                            "{profile} {} +{lower} bleed {stats:?}",
                            weapon.name
                        );
                        checked += 1;
                    }
                }
            }
            for aow in &data.aows {
                let rows: Vec<_> = data.aow_attack_rows(aow.aow_id).iter().collect();
                let routes = crate::math::prepare_scalar_aow_routes(&rows, &data)
                    .unwrap()
                    .unwrap_or_default();
                for somber in [false, true] {
                    let Some(weapon) = data.weapons.iter().find(|weapon| {
                        weapon.is_somber == somber && data.aow_compatible_with_weapon(aow, weapon)
                    }) else {
                        continue;
                    };
                    for lower in pairs(weapon) {
                        for stats in &stat_vectors {
                            let effective =
                                effective_str(stats.str, true, weapon.disable_two_hand_bonus);
                            assert!(
                                exact_bleed(weapon, lower, stats, &data, Some(aow)).unwrap()
                                    <= exact_bleed(weapon, lower + 1, stats, &data, Some(aow))
                                        .unwrap(),
                                "{profile} ash={} +{lower} bleed {stats:?}",
                                aow.aow_id
                            );
                            for route in routes
                                .iter()
                                .filter(|route| route.has_positive_corrections(&data))
                            {
                                let at = |upgrade| {
                                    exact_scalar_route(
                                        route, weapon, upgrade, stats, effective, 1.5, &data,
                                    )
                                    .unwrap()
                                };
                                let (low, high) = (at(lower), at(lower + 1));
                                assert!(
                                    low.0 <= high.0 && low.1 <= high.1,
                                    "{profile} ash={} route={} +{lower} {stats:?}",
                                    aow.aow_id,
                                    route.route_id
                                );
                                checked += 1;
                            }
                        }
                    }
                }
            }
            assert!(
                dominated > 1_000 && checked > 10_000,
                "{profile}: {dominated} {checked}"
            );
        }
    }

    #[test]
    fn smithscript_aec_overrides_match_reference_and_compiled_paths() {
        let data = data();
        let weapon = weapon(&data, "Smithscript Axe", "Flame Art");
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 70,
            dex: 20,
            int: 60,
            fai: 30,
            arc: 60,
        };
        let effective_str_value = effective_str(stats.str, false, weapon.disable_two_hand_bonus);
        let exact = exact_ar(weapon, 0, &stats, effective_str_value, &data).expect("exact AR");
        // The independent references use decimal CSV values; exact-v1 preserves loaded f32 bits.
        assert!(
            (exact[DamageType::Physical.as_index()].to_f64().unwrap() - 115.9599121865).abs()
                < 1e-5,
            "physical={:?}",
            exact[DamageType::Physical.as_index()]
        );
        assert!((exact[DamageType::Fire.as_index()].to_f64().unwrap() - 127.216667).abs() < 1e-5);

        let formula = compile_ar_formula(weapon, 0, &data, [148, 99, 99, 99, 99])
            .expect("compile exact AR formula");
        assert_eq!(
            formula
                .evaluate(&stats, effective_str_value, weapon.disable_two_hand_bonus)
                .expect("evaluate exact AR formula"),
            exact
        );

        let low_stats = Stats {
            int: 0,
            fai: 0,
            ..stats
        };
        let low_exact = exact_ar(
            weapon,
            0,
            &low_stats,
            effective_str(low_stats.str, false, weapon.disable_two_hand_bonus),
            &data,
        )
        .expect("low exact AR");
        assert!(
            exact[DamageType::Physical.as_index()] > low_exact[DamageType::Physical.as_index()]
        );

        let row = data
            .aow_attack_rows
            .values()
            .flat_map(|rows| rows.iter())
            .find(|row| {
                row.raw_name == "Wild Strikes - Loop [1]"
                    && row.overwrite_attack_element_correct_id.is_none()
            })
            .expect("Wild Strikes row");
        let route = ScalarAowRoute {
            route_id: "smithscript-aec".to_string(),
            route_priority: 0,
            hits: vec![ScalarAowHit {
                row,
                action_order: 0,
                hit_order: 0,
                buff_active: false,
                buff_attack_power: [0.0; DAMAGE_TYPE_COUNT],
            }],
        };
        let direct_route =
            exact_scalar_route(&route, weapon, 0, &stats, effective_str_value, 1.0, &data)
                .expect("direct exact skill route");
        let compiled_route =
            compile_scalar_route_formula(&route, weapon, 0, 1.0, &data, [148, 99, 99, 99, 99])
                .expect("compile exact skill route")
                .evaluate(&stats, effective_str_value)
                .expect("evaluate exact skill route");
        assert_eq!(compiled_route, direct_route);

        let exact_physical = exact_skill_damage_for_type(
            weapon,
            row,
            0,
            &stats,
            effective_str_value,
            DamageType::Physical,
            &data,
        )
        .expect("exact physical skill damage");
        let rounded_physical = calculate_skill_damage_for_type(
            weapon,
            row,
            0,
            &stats,
            effective_str_value,
            DamageType::Physical,
            &data,
        )
        .expect("rounded physical skill damage");
        assert!((exact_physical.to_f32().unwrap() - rounded_physical).abs() < 0.001);
        let low_physical = exact_skill_damage_for_type(
            weapon,
            row,
            0,
            &low_stats,
            effective_str(low_stats.str, false, weapon.disable_two_hand_bonus),
            DamageType::Physical,
            &data,
        )
        .expect("low exact physical skill damage");
        assert!(exact_physical > low_physical);
    }

    #[test]
    fn carian_retaliation_is_independent_of_weapon_upgrade_and_stats() {
        let data = data();
        let weapon = data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Perfumer's Shield" && weapon.affinity == "Cold")
            .unwrap();
        for row in data
            .aow_attack_rows
            .values()
            .flatten()
            .filter(|row| matches!(row.atk_id, 300_000_682 | 300_000_683))
        {
            for upgrade in [0, 7, 25] {
                for stat in [10, 99] {
                    let stats = Stats {
                        vig: 10,
                        mnd: 10,
                        end: 10,
                        str: stat,
                        dex: stat,
                        int: stat,
                        fai: stat,
                        arc: stat,
                    };
                    for two_handing in [false, true] {
                        let strength = effective_str(stat, two_handing, false);
                        let direct = exact_skill_damage_for_type(
                            weapon,
                            row,
                            upgrade,
                            &stats,
                            strength,
                            DamageType::Magic,
                            &data,
                        )
                        .unwrap();
                        assert_eq!(direct.to_f32(), Some(270.0));
                        let compiled = compile_skill_formula(
                            weapon,
                            row,
                            data.reinforce_level(weapon.reinforce_type, upgrade)
                                .unwrap(),
                            DamageType::Magic,
                            &data,
                            [148, 99, 99, 99, 99],
                        )
                        .unwrap();
                        assert_eq!(
                            compiled
                                .evaluate([
                                    strength,
                                    stat.into(),
                                    stat.into(),
                                    stat.into(),
                                    stat.into()
                                ])
                                .unwrap(),
                            direct
                        );
                        assert!(!row.has_fixed_damage(crate::data::CONVERGENCE_PROFILE_ID));
                    }
                }
            }
        }
    }

    #[test]
    fn numeric_bullets_include_fixed_damage_without_add_base_flag() {
        let data = data();
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 20,
            dex: 22,
            int: 10,
            fai: 10,
            arc: 10,
        };
        for name in ["Dragonscale Blade", "Dragon Halberd"] {
            let weapon = data
                .weapons
                .iter()
                .find(|weapon| weapon.name == name && weapon.affinity == "Standard")
                .unwrap();
            for (attack_id, fixed) in [(30_090_011, 149.0), (30_090_012, 69.0)] {
                let row = data.native_skill_attack_rows[&weapon.weapon_id]
                    .iter()
                    .find(|row| row.atk_id == attack_id)
                    .unwrap();
                assert!(!row.is_add_base_atk);
                assert!(row.is_bullet_attack && row.uses_fixed_attack_base());
                for upgrade in [0, 10] {
                    let reinforce = data
                        .reinforce_level(weapon.reinforce_type, upgrade)
                        .unwrap();
                    let expected = rational(fixed, "test fixed base").unwrap()
                        * rational(reinforce.base_attack_mult, "test upgrade rate").unwrap();
                    let direct = exact_skill_damage_for_type(
                        weapon,
                        row,
                        upgrade,
                        &stats,
                        20,
                        DamageType::Lightning,
                        &data,
                    )
                    .unwrap();
                    assert_eq!(direct, expected);
                    let compiled = compile_skill_formula(
                        weapon,
                        row,
                        reinforce,
                        DamageType::Lightning,
                        &data,
                        [148, 99, 99, 99, 99],
                    )
                    .unwrap();
                    assert_eq!(compiled.evaluate([20, 22, 10, 10, 10]).unwrap(), direct);
                }
            }
        }
    }

    #[test]
    fn lifesteal_influence_penalty_matches_independent_calculator() {
        let data = data();
        let weapon = data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Caestus" && weapon.affinity == "Occult")
            .unwrap();
        let row = data
            .aow_attack_rows
            .values()
            .flatten()
            .find(|row| row.raw_name == "Lifesteal Fist - Grab")
            .unwrap();
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 15,
            dex: 70,
            int: 70,
            fai: 20,
            arc: 70,
        };
        // tarnished.tools 1.17: same raw parameters, independently evaluated formula.
        for (damage_type, expected) in [
            (DamageType::Physical, 105.71547663176304),
            (DamageType::Magic, 292.74),
        ] {
            let actual =
                exact_skill_damage_for_type(weapon, row, 25, &stats, 15, damage_type, &data)
                    .unwrap();
            assert!(
                (actual.to_f64().unwrap() - expected).abs() < 0.0001,
                "{damage_type}: {actual:?}"
            );
        }
        // Even a positive DEX contribution must not offset a negative STR contribution.
        let ext = data.attack_element_ext(52000).unwrap();
        assert!(ext.influence_rate(0, 0) < 1.0);
        assert!(
            compile_skill_formula(
                weapon,
                row,
                data.reinforce_level(weapon.reinforce_type, 25).unwrap(),
                DamageType::Physical,
                &data,
                [148, 99, 99, 99, 99]
            )
            .is_err()
        );
    }

    #[test]
    fn lifesteal_grabs_apply_weapon_critical_damage() {
        let data = data();
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 40,
            dex: 40,
            int: 40,
            fai: 40,
            arc: 40,
        };
        for name in ["Katar", "Pata", "Raptor Talons"] {
            let weapon = weapon(&data, name, "Standard");
            assert_eq!(weapon.critical_damage_percent, 110);
            let mut normal_critical = weapon.clone();
            normal_critical.critical_damage_percent = 100;
            let rows = data
                .aow_attack_rows
                .values()
                .flatten()
                .filter(|row| row.aow_name == "Lifesteal Fist");
            let mut grabbed = false;
            let mut contact = false;
            for row in rows {
                grabbed |= row.is_throw_attack;
                contact |= !row.is_throw_attack;
                for upgrade in [0, 25] {
                    for damage_type in [DamageType::Physical, DamageType::Magic] {
                        let normal = exact_skill_damage_for_type(
                            &normal_critical,
                            row,
                            upgrade,
                            &stats,
                            60,
                            damage_type,
                            &data,
                        )
                        .unwrap();
                        let actual = exact_skill_damage_for_type(
                            weapon,
                            row,
                            upgrade,
                            &stats,
                            60,
                            damage_type,
                            &data,
                        )
                        .unwrap();
                        let expected = if row.is_throw_attack {
                            normal * ExactRational::from_integer(11.into())
                                / ExactRational::from_integer(10.into())
                        } else {
                            normal
                        };
                        assert_eq!(
                            actual, expected,
                            "{name}: {} +{upgrade} {damage_type}",
                            row.raw_name
                        );
                    }
                }
            }
            assert!(grabbed && contact);
        }
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
        let mut curve_maxima = std::array::from_fn(|_| std::array::from_fn(|_| None));
        let upper = exact_ar_upper_bound(&weapon, 25, &data, minimum, maximum, &mut curve_maxima)
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
        // HashMap order changes per process, and rows that override the weapon's correction
        // with partial influence need direct evaluation; take the first plain row by AoW id.
        let mut aows = data.aow_attack_rows.iter().collect::<Vec<_>>();
        aows.sort_by_key(|(aow_id, _)| **aow_id);
        let mut row = aows
            .into_iter()
            .flat_map(|(_, rows)| rows.iter())
            .find(|row| row.is_damaging() && row.overwrite_attack_element_correct_id.is_none())
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
    fn full_route_coefficients_preserve_raw_and_effective_strength() {
        let data = data();
        let weapon = weapon(&data, "Uchigatana", "Keen");
        let mut row = data
            .aow_attack_rows
            .values()
            .flatten()
            .find(|r| {
                r.raw_name == "Wild Strikes - Loop [1]"
                    && r.overwrite_attack_element_correct_id.is_none()
            })
            .unwrap()
            .clone();
        row.is_disable_both_hands_bonus = false;
        let mut raw_row = row.clone();
        raw_row.is_disable_both_hands_bonus = true;
        let route = ScalarAowRoute {
            route_id: "mixed-strength".into(),
            route_priority: 0,
            hits: [&row, &raw_row, &raw_row]
                .into_iter()
                .enumerate()
                .map(|(index, row)| ScalarAowHit {
                    row,
                    action_order: 0,
                    hit_order: index as u16,
                    buff_active: index > 0,
                    buff_attack_power: [7.0, 0.0, 0.0, 0.0, 0.0],
                })
                .collect(),
        };
        assert!(route.is_additive(&data));
        for upgrade in [0, 25] {
            let formula = compile_scalar_route_formula(
                &route,
                weapon,
                upgrade,
                2.05,
                &data,
                [148, 99, 99, 99, 99],
            )
            .unwrap();
            assert_eq!(
                formula.full.len(),
                2,
                "raw/effective transforms must remain distinct"
            );
            for strength in 1..=99 {
                let stats = Stats {
                    vig: 12,
                    mnd: 11,
                    end: 13,
                    str: strength,
                    dex: 40,
                    int: 9,
                    fai: 8,
                    arc: 20,
                };
                let effective = effective_str(strength, true, false);
                assert_eq!(
                    formula.evaluate(&stats, effective).unwrap(),
                    exact_scalar_route(&route, weapon, upgrade, &stats, effective, 2.05, &data)
                        .unwrap(),
                    "upgrade={upgrade} str={strength} effective={effective}"
                );
            }
        }
    }

    #[test]
    fn full_route_coefficients_match_real_routes_and_deltas() {
        let data = data();
        let mut route_count = 0;
        let mut buff_count = 0;
        let mut projectile_count = 0;
        for (weapon_name, affinity, ash, variant) in [
            ("Claymore", "Standard", 110, ""),
            ("Claymore", "Heavy", 651, "Greatsword"),
            ("Claymore", "Fire", 214, ""),
            ("Claymore", "Flame Art", 4140, ""),
            ("Uchigatana", "Keen", 112, ""),
            ("Uchigatana", "Keen", 210, ""),
            ("Uchigatana", "Keen", 114, ""),
        ] {
            let weapon = weapon(&data, weapon_name, affinity);
            let rows: Vec<_> = data
                .aow_attack_rows(ash)
                .iter()
                .filter(|row| !row.is_lacking_fp && row.variant_weapon_type == variant)
                .collect();
            let routes = crate::math::prepare_scalar_aow_routes(&rows, &data)
                .unwrap()
                .unwrap();
            assert!(!routes.is_empty(), "ash={ash}");
            for route in routes {
                assert!(
                    route.is_additive(&data),
                    "ash={ash} route={}",
                    route.route_id
                );
                route_count += 1;
                buff_count += route.hits.iter().filter(|hit| hit.buff_active).count();
                projectile_count += route
                    .hits
                    .iter()
                    .filter(|hit| hit.row.overwrite_attack_element_correct_id.is_some())
                    .count();
                for (upgrade, multiplier) in [(0, 1.0), (25, 2.05), (25, 0.0)] {
                    let formula = compile_scalar_route_formula(
                        &route,
                        weapon,
                        upgrade,
                        multiplier,
                        &data,
                        [148, 99, 99, 99, 99],
                    )
                    .unwrap();
                    let base = Stats {
                        vig: 12,
                        mnd: 11,
                        end: 13,
                        str: 12,
                        dex: 15,
                        int: 9,
                        fai: 8,
                        arc: 8,
                    };
                    let baseline = formula
                        .evaluate(&base, effective_str(base.str, true, false))
                        .unwrap();
                    for stat in 0..COMBAT_STAT_COUNT {
                        for value in [1, 20, 60, 80, 99] {
                            let mut combat = base.combat_array();
                            combat[stat] = value;
                            let stats = Stats {
                                str: combat[0],
                                dex: combat[1],
                                int: combat[2],
                                fai: combat[3],
                                arc: combat[4],
                                ..base
                            };
                            let effective = effective_str(stats.str, true, false);
                            let direct = exact_scalar_route(
                                &route, weapon, upgrade, &stats, effective, multiplier, &data,
                            )
                            .unwrap();
                            assert_eq!(
                                formula.evaluate(&stats, effective).unwrap(),
                                direct,
                                "ash={ash} route={} upgrade={upgrade} stat={stat} value={value}",
                                route.route_id
                            );
                            let delta = formula
                                .unscaled_delta(
                                    stat,
                                    &base,
                                    &stats,
                                    effective_str(base.str, true, false),
                                    effective,
                                )
                                .unwrap();
                            let scale = rational(multiplier, "test multiplier").unwrap();
                            assert_eq!(
                                (delta.0 * &scale, delta.1 * &scale),
                                (&direct.0 - &baseline.0, &direct.1 - &baseline.1)
                            );
                        }
                    }
                }
            }
        }
        assert!(route_count >= 7 && buff_count > 0 && projectile_count > 0);
        let weapon = weapon(&data, "Caestus", "Occult");
        let rows: Vec<_> = data
            .aow_attack_rows(205)
            .iter()
            .filter(|row| !row.is_lacking_fp)
            .collect();
        let routes = crate::math::prepare_scalar_aow_routes(&rows, &data)
            .unwrap()
            .unwrap();
        assert!(routes.iter().any(|route| !route.is_additive(&data)));
        for route in routes.iter().filter(|route| !route.is_additive(&data)) {
            assert!(
                compile_scalar_route_formula(route, weapon, 25, 1.0, &data, [148, 99, 99, 99, 99])
                    .is_err()
            );
        }
    }

    #[test]
    fn route_upper_bound_covers_every_allocation_in_its_interval() {
        let data = data();
        let grid = [10_u8, 17, 26, 33, 40];
        for (weapon_name, affinity, ash) in
            [("Caestus", "Occult", 205), ("Claymore", "Standard", 110)]
        {
            let weapon = weapon(&data, weapon_name, affinity);
            let rows: Vec<_> = data
                .aow_attack_rows(ash)
                .iter()
                .filter(|row| !row.is_lacking_fp)
                .collect();
            let routes = crate::math::prepare_scalar_aow_routes(&rows, &data)
                .unwrap()
                .unwrap();
            for route in &routes {
                // Raw STR up to 40 and its two-handed value up to 60.
                let (first_bound, full_bound) = exact_scalar_route_upper_bound(
                    route,
                    weapon,
                    25,
                    2.05,
                    &data,
                    [10; COMBAT_STAT_COUNT],
                    [60, 40, 40, 40, 40],
                    &mut std::array::from_fn(|_| std::array::from_fn(|_| None)),
                )
                .unwrap();
                for index in 0..grid.len().pow(5) {
                    let pick = |stat: u32| grid[index / grid.len().pow(stat) % grid.len()];
                    let stats = Stats {
                        vig: 10,
                        mnd: 10,
                        end: 10,
                        str: pick(0),
                        dex: pick(1),
                        int: pick(2),
                        fai: pick(3),
                        arc: pick(4),
                    };
                    for effective in [u16::from(stats.str), u16::from(stats.str) * 3 / 2] {
                        let (first, full) =
                            exact_scalar_route(route, weapon, 25, &stats, effective, 2.05, &data)
                                .unwrap();
                        assert!(first <= first_bound, "{weapon_name} {}", route.route_id);
                        assert!(full <= full_bound, "{weapon_name} {}", route.route_id);
                    }
                }
            }
            assert!(ash != 205 || routes.iter().any(|route| !route.is_additive(&data)));
        }
    }

    #[test]
    fn coupled_route_formula_matches_direct_evaluation() {
        let data = data();
        let rows: Vec<_> = data
            .aow_attack_rows(205)
            .iter()
            .filter(|row| !row.is_lacking_fp)
            .collect();
        let routes = crate::math::prepare_scalar_aow_routes(&rows, &data)
            .unwrap()
            .unwrap();
        assert!(routes.iter().any(|route| !route.is_additive(&data)));
        let mut compared = 0;
        let mut check = |weapon: &Weapon, upgrade: u8, values: &[u8], arcane: u8| {
            for route in &routes {
                for multiplier in [1.0, 2.05] {
                    let formula = compile_coupled_route_formula(
                        route,
                        weapon,
                        upgrade,
                        multiplier,
                        &data,
                        [148, 99, 99, 99, 99],
                    )
                    .unwrap();
                    for &strength in values {
                        for &dexterity in values {
                            let stats = Stats {
                                vig: 10,
                                mnd: 10,
                                end: 10,
                                str: strength,
                                dex: dexterity,
                                int: 10,
                                fai: 10,
                                arc: arcane,
                            };
                            for effective in [u16::from(strength), u16::from(strength) * 3 / 2] {
                                assert_eq!(
                                    formula.evaluate(&stats, effective).unwrap(),
                                    exact_scalar_route(
                                        route, weapon, upgrade, &stats, effective, multiplier,
                                        &data,
                                    )
                                    .unwrap(),
                                    "{} {} +{upgrade} {stats:?} {effective} {}",
                                    weapon.name,
                                    weapon.affinity,
                                    route.route_id
                                );
                                compared += 1;
                            }
                        }
                    }
                }
            }
        };
        // Every STR and DEX value, so each correction term is checked on both sides of zero.
        let every: Vec<u8> = (1..=99).collect();
        check(weapon(&data, "Caestus", "Occult"), 25, &every, 10);
        let sampled: Vec<u8> = (1..=99).step_by(7).collect();
        for weapon in data
            .weapons
            .iter()
            .filter(|weapon| weapon.name == "Caestus")
        {
            for upgrade in [0, 10, 25] {
                check(weapon, upgrade, &sampled, 60);
            }
        }
        assert!(compared > 0);
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
        assert_eq!(exact, exact.floor());
    }

    #[test]
    fn public_status_evaluator_rounds_scaled_bleed_before_flooring() {
        let mut data = data();
        let mut weapon = weapon(&data, "Uchigatana", "Blood").clone();
        weapon.scaling[STAT_ARC] = 9.0;
        data.weapon_passive_overlays.remove(&weapon.weapon_id);
        let source = data.weapon_passives.get_mut(&weapon.weapon_id).unwrap();
        source.buildup.bleed = 0.7;
        source.correction_flags.bleed = Some(true);
        data.reinforce[usize::from(weapon.reinforce_type)][0]
            .as_mut()
            .unwrap()
            .scaling_mult[STAT_ARC] = 1.0;
        data.calc_correct[weapon.status_curve_ids.blood]
            .as_mut()
            .unwrap()[99] = Some(1.0);
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 11,
            dex: 15,
            int: 10,
            fai: 10,
            arc: 99,
        };
        assert_eq!(
            crate::math::calculate_status_buildup(&weapon, 0, &stats, &data)
                .unwrap()
                .bleed,
            7.0
        );
        assert_eq!(
            calculate_bleed_buildup(&weapon, 0, &stats, &data).unwrap(),
            7.0
        );
        assert_eq!((0.7_f32 * 10.0).floor(), 7.0);
    }

    #[test]
    fn ash_status_additions_round_before_the_final_bleed_floor() {
        let mut data = data();
        let mut weapon = weapon(&data, "Uchigatana", "Blood").clone();
        weapon.scaling[STAT_ARC] = 9.0;
        data.weapon_passive_overlays.remove(&weapon.weapon_id);
        data.weapon_passives
            .get_mut(&weapon.weapon_id)
            .unwrap()
            .buildup
            .bleed = 0.0;
        data.reinforce[usize::from(weapon.reinforce_type)][0]
            .as_mut()
            .unwrap()
            .scaling_mult[STAT_ARC] = 1.0;
        data.calc_correct[weapon.status_curve_ids.blood]
            .as_mut()
            .unwrap()[99] = Some(1.0);
        let mut ash = data
            .aows
            .iter()
            .find(|ash| ash.name == "Seppuku")
            .unwrap()
            .clone();
        let stats = Stats {
            vig: 10,
            mnd: 10,
            end: 10,
            str: 11,
            dex: 15,
            int: 10,
            fai: 10,
            arc: 99,
        };
        for (flat, scaling, flag) in [(0.3, 6.7, false), (0.0, 0.7, true)] {
            ash.bleed_buildup_add = flat;
            ash.scaling_status_add.bleed = scaling;
            ash.scaling_status_flags.bleed = Some(flag);
            let actual = exact_bleed(&weapon, 0, &stats, &data, Some(&ash)).unwrap();
            assert_eq!(actual, ExactRational::from_integer(BigInt::from(7)));
        }
    }
}
