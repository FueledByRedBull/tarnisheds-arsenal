//! The one reader of attack-element correction data.
//!
//! A correction record routes stats to damage types and, for the extended records, replaces
//! a weapon's scaling (overwrite) or multiplies it (influence). Two rules read a record:
//!
//! * The weapon-AR rule (optimizer-math Section 3): the coefficient of a routed stat is the
//!   overwrite rate when present, otherwise the weapon's scaling times the influence rate.
//!   Skills that use the weapon's own record follow it too.
//! * The skill rule (optimizer-math Section 3, non-additive corrections): a skill row with its
//!   own record gives each routed stat the contribution `(influence - 1) + influence * scaling *
//!   multiplier * curve`, with the overwrite rate or the weapon's scaling as `scaling`. The
//!   most negative contribution replaces the sum when one is negative.
//!
//! Both agree on records with influence 1.

use num_traits::{One, Zero};

use super::exact::{multiply_if_needed, rational, reinforce_level};
use super::exact_value::ExactRational;
use crate::model::{
    AowAttackRow, AttackElementCorrect, AttackElementCorrectExt, COMBAT_STAT_COUNT,
    DAMAGE_TYPE_COUNT, GameData, ReinforceLevel, Weapon,
};

/// A correction record: which stats scale which damage types, and for extended records how.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Record<'a> {
    /// Routing only: no overwrite, influence 1.
    Plain(&'a AttackElementCorrect),
    Ext(&'a AttackElementCorrectExt),
}

impl Record<'_> {
    pub(crate) fn scales(self, stat_idx: usize, damage_idx: usize) -> bool {
        match self {
            Self::Plain(record) => record.scales[stat_idx][damage_idx],
            Self::Ext(record) => record.stat_scales(stat_idx, damage_idx),
        }
    }

    fn overwrite(self, stat_idx: usize, damage_idx: usize) -> Option<f32> {
        match self {
            Self::Plain(_) => None,
            Self::Ext(record) => record.overwrite_rate(stat_idx, damage_idx),
        }
    }

    pub(crate) fn influence(self, stat_idx: usize, damage_idx: usize) -> f32 {
        match self {
            Self::Plain(_) => 1.0,
            Self::Ext(record) => record.influence_rate(stat_idx, damage_idx),
        }
    }

    /// The weapon-AR scaling coefficient before the reinforce multiplier, as `f32`. Callers
    /// that rank use the exact `Correction::ar_coefficient` instead.
    pub(crate) fn ar_scaling(self, weapon_scaling: f32, stat_idx: usize, damage_idx: usize) -> f32 {
        self.overwrite(stat_idx, damage_idx)
            .unwrap_or(weapon_scaling * self.influence(stat_idx, damage_idx))
    }

    /// Whether every stat routed to the damage type has influence 1, so the damage is a
    /// constant plus separable per-stat terms.
    pub(crate) fn is_additive(self, damage_idx: usize) -> bool {
        (0..COMBAT_STAT_COUNT)
            .all(|stat| !self.scales(stat, damage_idx) || self.influence(stat, damage_idx) == 1.0)
    }

    /// Whether every stat routed to the damage type has positive influence, which keeps each
    /// skill-rule contribution above -1.
    pub(crate) fn has_positive_influence(self, damage_idx: usize) -> bool {
        (0..COMBAT_STAT_COUNT)
            .all(|stat| !self.scales(stat, damage_idx) || self.influence(stat, damage_idx) > 0.0)
    }
}

/// A weapon's reinforce level and correction record, resolved once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Correction<'a> {
    pub(crate) reinforce: &'a ReinforceLevel,
    record: Record<'a>,
    weapon_scaling: [f32; COMBAT_STAT_COUNT],
}

impl<'a> Correction<'a> {
    pub(crate) fn new(
        reinforce: &'a ReinforceLevel,
        record: Record<'a>,
        weapon_scaling: [f32; COMBAT_STAT_COUNT],
    ) -> Self {
        Self {
            reinforce,
            record,
            weapon_scaling,
        }
    }

    /// The weapon's own record: its extended record when it has one, else the plain one.
    pub(crate) fn resolve(
        weapon: &Weapon,
        upgrade: u8,
        data: &'a GameData,
    ) -> Result<Self, String> {
        let reinforce = reinforce_level(weapon, upgrade, data)?;
        let plain = data
            .attack_element(weapon.attack_element_correct_id)
            .ok_or_else(|| {
                format!(
                    "missing attack_element_correct_id={}",
                    weapon.attack_element_correct_id
                )
            })?;
        let record = data
            .attack_element_ext(weapon.attack_element_correct_id)
            .map_or(Record::Plain(plain), Record::Ext);
        Ok(Self::new(reinforce, record, weapon.scaling))
    }

    /// The same weapon read through the extended record a skill row names for itself.
    pub(crate) fn overriding<'b>(
        &self,
        override_id: usize,
        row: &AowAttackRow,
        data: &'b GameData,
    ) -> Result<Correction<'b>, String>
    where
        'a: 'b,
    {
        let ext = data.attack_element_ext(override_id).ok_or_else(|| {
            format!(
                "missing attack_element_correct_ext_id={} for AoW row {} ({})",
                override_id, row.sheet_row, row.raw_name
            )
        })?;
        Ok(Correction {
            reinforce: self.reinforce,
            record: Record::Ext(ext),
            weapon_scaling: self.weapon_scaling,
        })
    }

    /// The weapon-AR rule as `f32` for scheduling estimates; `None` when the stat does not
    /// scale the damage type.
    pub(crate) fn ar_scaling_f32(&self, stat_idx: usize, damage_idx: usize) -> Option<f32> {
        self.record.scales(stat_idx, damage_idx).then(|| {
            self.record
                .ar_scaling(self.weapon_scaling[stat_idx], stat_idx, damage_idx)
        })
    }

    /// The weapon-AR rule, exact: scaling times the reinforce multiplier. `None` when the stat
    /// does not scale the damage type or the product is zero. An overwrite ignores influence.
    pub(crate) fn ar_coefficient(
        &self,
        stat_idx: usize,
        damage_idx: usize,
    ) -> Result<Option<ExactRational>, String> {
        if !self.record.scales(stat_idx, damage_idx) {
            return Ok(None);
        }
        let scaling = match self.record.overwrite(stat_idx, damage_idx) {
            Some(value) => rational(value, "attack correction overwrite")?,
            None => {
                let mut scaling = rational(self.weapon_scaling[stat_idx], "weapon scaling")?;
                let influence = self.record.influence(stat_idx, damage_idx);
                if influence != 1.0 {
                    scaling *= rational(influence, "attack correction influence")?;
                }
                scaling
            }
        };
        let scaling_mult = rational(
            self.reinforce.scaling_mult[stat_idx],
            "reinforce scaling multiplier",
        )?;
        Ok((!scaling.is_zero() && !scaling_mult.is_zero())
            .then(|| multiply_if_needed(scaling, scaling_mult)))
    }

    pub(crate) fn ar_coefficients(
        &self,
        damage_idx: usize,
    ) -> Result<[Option<ExactRational>; COMBAT_STAT_COUNT], String> {
        let mut coefficients = std::array::from_fn(|_| None);
        for (stat_idx, coefficient) in coefficients.iter_mut().enumerate() {
            *coefficient = self.ar_coefficient(stat_idx, damage_idx)?;
        }
        Ok(coefficients)
    }

    /// Whether the damage type is a constant plus separable per-stat terms.
    pub(crate) fn is_additive(&self, damage_idx: usize) -> bool {
        self.record.is_additive(damage_idx)
    }

    /// The skill rule's contribution of a stat, `offset + coefficient * curve`; `None` when
    /// the stat does not scale the damage type.
    pub(crate) fn skill_term(
        &self,
        stat_idx: usize,
        damage_idx: usize,
    ) -> Result<Option<SkillTerm>, String> {
        if !self.record.scales(stat_idx, damage_idx) {
            return Ok(None);
        }
        let influence = rational(
            self.record.influence(stat_idx, damage_idx),
            "attack correction influence",
        )?;
        let scaling = rational(
            self.record
                .overwrite(stat_idx, damage_idx)
                .unwrap_or(self.weapon_scaling[stat_idx]),
            "attack correction scaling",
        )?;
        let coefficient = scaling
            * rational(
                self.reinforce.scaling_mult[stat_idx],
                "reinforce scaling multiplier",
            )?
            * &influence;
        Ok(Some(SkillTerm {
            offset: influence - ExactRational::one(),
            coefficient,
        }))
    }

    /// What a skill row's base damage is multiplied by under the skill rule, given the curve
    /// value of each stat.
    pub(crate) fn skill_factor(
        &self,
        damage_idx: usize,
        curve_mults: &[ExactRational; COMBAT_STAT_COUNT],
    ) -> Result<ExactRational, String> {
        let mut bonus = CorrectionBonus::default();
        for (stat_idx, curve) in curve_mults.iter().enumerate() {
            if let Some(term) = self.skill_term(stat_idx, damage_idx)? {
                bonus.add(&term.value(curve));
            }
        }
        Ok(ExactRational::one() + bonus.finish())
    }
}

/// One stat's skill-rule contribution as a function of its curve value.
#[derive(Clone, Debug)]
pub(crate) struct SkillTerm {
    offset: ExactRational,
    coefficient: ExactRational,
}

impl SkillTerm {
    pub(crate) fn value(&self, curve: &ExactRational) -> ExactRational {
        &self.offset + &self.coefficient * curve
    }
}

/// The skill rule's bonus over the contributions added to it: the most negative contribution
/// when one is negative, otherwise their sum.
#[derive(Default)]
pub(crate) struct CorrectionBonus {
    total: ExactRational,
    /// Zero until a negative contribution replaces it.
    minimum: ExactRational,
}

impl CorrectionBonus {
    pub(crate) fn add(&mut self, contribution: &ExactRational) {
        if *contribution < self.minimum {
            self.minimum = contribution.clone();
        }
        self.total += contribution;
    }

    pub(crate) fn finish(self) -> ExactRational {
        if self.minimum.is_zero() {
            self.total
        } else {
            self.minimum
        }
    }
}

/// Whether every damage type of an extended record is additive.
pub(crate) fn is_additive(record: &AttackElementCorrectExt) -> bool {
    (0..DAMAGE_TYPE_COUNT).all(|damage| Record::Ext(record).is_additive(damage))
}

/// Whether every damage type of an extended record has positive influence on its routed stats.
pub(crate) fn has_positive_influence(record: &AttackElementCorrectExt) -> bool {
    (0..DAMAGE_TYPE_COUNT).all(|damage| Record::Ext(record).has_positive_influence(damage))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STR: usize = 0;
    const DEX: usize = 1;
    const PHYSICAL: usize = 0;
    const MAGIC: usize = 1;

    fn ratio(numerator: i64, denominator: i64) -> ExactRational {
        ExactRational::from_integer(numerator.into())
            / ExactRational::from_integer(denominator.into())
    }

    fn reinforce(scaling_mult: f32) -> ReinforceLevel {
        ReinforceLevel {
            damage_mult: [1.0; DAMAGE_TYPE_COUNT],
            scaling_mult: [scaling_mult; COMBAT_STAT_COUNT],
            base_attack_mult: 1.0,
        }
    }

    fn plain(routes: &[(usize, usize)]) -> AttackElementCorrect {
        let mut record = AttackElementCorrect {
            scales: [[false; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
        };
        for &(stat, damage) in routes {
            record.scales[stat][damage] = true;
        }
        record
    }

    fn ext(routes: &[(usize, usize)]) -> AttackElementCorrectExt {
        let mut record = AttackElementCorrectExt {
            scales: [[false; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
            overwrite: [[None; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
            influence: [[1.0; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
        };
        for &(stat, damage) in routes {
            record.scales[stat][damage] = true;
        }
        record
    }

    fn weapon_scaling(str_scaling: f32, dex_scaling: f32) -> [f32; COMBAT_STAT_COUNT] {
        [str_scaling, dex_scaling, 0.0, 0.0, 0.0]
    }

    #[test]
    fn plain_record_scales_only_routed_stats_by_weapon_scaling() {
        let record = plain(&[(STR, PHYSICAL)]);
        let level = reinforce(0.5);
        let correction = Correction::new(&level, Record::Plain(&record), weapon_scaling(0.5, 0.25));

        // Scaling 0.5 times the reinforce multiplier 0.5.
        assert_eq!(
            correction.ar_coefficient(STR, PHYSICAL).unwrap(),
            Some(ratio(1, 4))
        );
        // DEX has scaling but no routing to physical; STR has routing but not to magic.
        assert_eq!(correction.ar_coefficient(DEX, PHYSICAL).unwrap(), None);
        assert_eq!(correction.ar_coefficient(STR, MAGIC).unwrap(), None);
        assert_eq!(correction.ar_scaling_f32(STR, PHYSICAL), Some(0.5));
        assert_eq!(correction.ar_scaling_f32(DEX, PHYSICAL), None);
    }

    #[test]
    fn routed_stat_without_weapon_scaling_has_no_coefficient() {
        let record = plain(&[(DEX, PHYSICAL)]);
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Plain(&record), weapon_scaling(0.5, 0.0));

        assert_eq!(correction.ar_coefficient(DEX, PHYSICAL).unwrap(), None);
    }

    #[test]
    fn influence_multiplies_weapon_scaling_in_the_ar_rule() {
        let mut record = ext(&[(STR, PHYSICAL)]);
        record.influence[STR][PHYSICAL] = 0.5;
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.0));

        // 0.5 * 0.5
        assert_eq!(
            correction.ar_coefficient(STR, PHYSICAL).unwrap(),
            Some(ratio(1, 4))
        );
        assert_eq!(correction.ar_scaling_f32(STR, PHYSICAL), Some(0.25));
    }

    #[test]
    fn overwrite_replaces_weapon_scaling_and_ignores_influence_in_the_ar_rule() {
        let mut record = ext(&[(STR, PHYSICAL), (DEX, PHYSICAL)]);
        record.overwrite[STR][PHYSICAL] = Some(0.25);
        record.influence[STR][PHYSICAL] = 2.0;
        // An explicit zero overwrite suppresses scaling; a positive one supplies it without any.
        record.overwrite[DEX][PHYSICAL] = Some(0.0);
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.5));

        assert_eq!(
            correction.ar_coefficient(STR, PHYSICAL).unwrap(),
            Some(ratio(1, 4))
        );
        assert_eq!(correction.ar_coefficient(DEX, PHYSICAL).unwrap(), None);

        let no_weapon_scaling =
            Correction::new(&level, Record::Ext(&record), weapon_scaling(0.0, 0.0));
        assert_eq!(
            no_weapon_scaling.ar_coefficient(STR, PHYSICAL).unwrap(),
            Some(ratio(1, 4))
        );
    }

    #[test]
    fn zero_reinforce_multiplier_removes_the_coefficient() {
        let record = plain(&[(STR, PHYSICAL)]);
        let level = reinforce(0.0);
        let correction = Correction::new(&level, Record::Plain(&record), weapon_scaling(0.5, 0.0));

        assert_eq!(correction.ar_coefficient(STR, PHYSICAL).unwrap(), None);
    }

    #[test]
    fn negative_influence_is_rejected_where_it_is_read() {
        let mut record = ext(&[(STR, PHYSICAL)]);
        record.influence[STR][PHYSICAL] = -0.5;
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.0));

        let error = correction.ar_coefficient(STR, PHYSICAL).unwrap_err();
        assert!(
            error.contains("negative attack correction influence"),
            "{error}"
        );
        let error = correction.skill_term(STR, PHYSICAL).unwrap_err();
        assert!(
            error.contains("negative attack correction influence"),
            "{error}"
        );

        // An overwrite makes the AR rule independent of influence, so it never reads it.
        record.overwrite[STR][PHYSICAL] = Some(0.25);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.0));
        assert_eq!(
            correction.ar_coefficient(STR, PHYSICAL).unwrap(),
            Some(ratio(1, 4))
        );
        assert!(correction.skill_term(STR, PHYSICAL).is_err());
    }

    #[test]
    fn skill_contribution_applies_influence_to_the_whole_term() {
        let mut record = ext(&[(STR, PHYSICAL)]);
        record.influence[STR][PHYSICAL] = 0.5;
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.0));
        let term = correction.skill_term(STR, PHYSICAL).unwrap().unwrap();

        // (0.5 - 1) + 0.5 * 0.5 * 1 * curve
        assert_eq!(term.value(&ExactRational::zero()), ratio(-1, 2));
        assert_eq!(term.value(&ExactRational::one()), ratio(-1, 4));
        assert_eq!(term.value(&ratio(1, 2)), ratio(-3, 8));
        assert!(correction.skill_term(DEX, PHYSICAL).unwrap().is_none());
    }

    #[test]
    fn skill_contribution_applies_influence_to_an_overwrite() {
        let mut record = ext(&[(STR, PHYSICAL)]);
        record.overwrite[STR][PHYSICAL] = Some(0.25);
        record.influence[STR][PHYSICAL] = 0.5;
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.0));
        let term = correction.skill_term(STR, PHYSICAL).unwrap().unwrap();

        // (0.5 - 1) + 0.5 * 0.25 * 1 * 1
        assert_eq!(term.value(&ExactRational::one()), ratio(-3, 8));
    }

    #[test]
    fn zero_influence_makes_the_skill_contribution_constant() {
        let mut record = ext(&[(STR, PHYSICAL)]);
        record.overwrite[STR][PHYSICAL] = Some(0.25);
        record.influence[STR][PHYSICAL] = 0.0;
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.0));
        let term = correction.skill_term(STR, PHYSICAL).unwrap().unwrap();

        // The stat cannot move the skill damage, which is multiplied by 1 + (-1) = 0.
        assert_eq!(term.value(&ExactRational::zero()), ratio(-1, 1));
        assert_eq!(term.value(&ratio(3, 2)), ratio(-1, 1));
        let curves = [
            ratio(3, 2),
            ExactRational::zero(),
            ExactRational::zero(),
            ExactRational::zero(),
            ExactRational::zero(),
        ];
        assert_eq!(
            correction.skill_factor(PHYSICAL, &curves).unwrap(),
            ExactRational::zero()
        );
        // The AR rule still gives that overwrite a coefficient: stat selection that follows it
        // may keep a stat the skill rule ignores, never the reverse.
        assert_eq!(
            correction.ar_coefficient(STR, PHYSICAL).unwrap(),
            Some(ratio(1, 4))
        );
    }

    #[test]
    fn skill_factor_adds_nonnegative_contributions() {
        let mut record = ext(&[(STR, PHYSICAL), (DEX, PHYSICAL)]);
        record.influence[DEX][PHYSICAL] = 2.0;
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.25));
        let curves = [
            ExactRational::one(),
            ExactRational::one(),
            ExactRational::zero(),
            ExactRational::zero(),
            ExactRational::zero(),
        ];

        // STR: 0.5. DEX: (2 - 1) + 2 * 0.25 = 1.5. Neither is negative, so 1 + 0.5 + 1.5.
        assert_eq!(
            correction.skill_factor(PHYSICAL, &curves).unwrap(),
            ratio(3, 1)
        );
    }

    #[test]
    fn skill_factor_takes_the_most_negative_contribution_instead_of_the_sum() {
        let mut record = ext(&[(STR, PHYSICAL), (DEX, PHYSICAL)]);
        record.influence[STR][PHYSICAL] = 0.5;
        let level = reinforce(1.0);
        let correction = Correction::new(&level, Record::Ext(&record), weapon_scaling(0.5, 0.25));
        let curves = [
            ExactRational::one(),
            ExactRational::one(),
            ExactRational::zero(),
            ExactRational::zero(),
            ExactRational::zero(),
        ];

        // STR: (0.5 - 1) + 0.5 * 0.5 = -0.25. DEX: 0.25 does not offset it. 1 - 0.25.
        assert_eq!(
            correction.skill_factor(PHYSICAL, &curves).unwrap(),
            ratio(3, 4)
        );
        // A damage type no stat is routed to has no bonus.
        assert_eq!(
            correction.skill_factor(MAGIC, &curves).unwrap(),
            ExactRational::one()
        );
    }

    #[test]
    fn bonus_selection_is_independent_of_contribution_order() {
        let contributions = [ratio(1, 2), ratio(-1, 4), ratio(-3, 8), ratio(1, 1)];
        for order in [[0, 1, 2, 3], [3, 2, 1, 0], [2, 0, 3, 1]] {
            let mut bonus = CorrectionBonus::default();
            for index in order {
                bonus.add(&contributions[index]);
            }
            assert_eq!(bonus.finish(), ratio(-3, 8));
        }
        let mut sum = CorrectionBonus::default();
        sum.add(&ratio(1, 2));
        sum.add(&ratio(1, 4));
        assert_eq!(sum.finish(), ratio(3, 4));
        assert_eq!(CorrectionBonus::default().finish(), ExactRational::zero());
    }

    #[test]
    fn additivity_and_influence_signs_ignore_unrouted_stats() {
        let mut record = ext(&[(STR, PHYSICAL)]);
        record.influence[DEX][PHYSICAL] = 0.0;
        record.influence[STR][MAGIC] = 0.0;
        assert!(is_additive(&record));
        assert!(has_positive_influence(&record));

        record.influence[STR][PHYSICAL] = 0.5;
        assert!(!Record::Ext(&record).is_additive(PHYSICAL));
        assert!(Record::Ext(&record).is_additive(MAGIC));
        assert!(!is_additive(&record));
        assert!(has_positive_influence(&record));

        record.influence[STR][PHYSICAL] = 0.0;
        assert!(!has_positive_influence(&record));
        assert!(Record::Ext(&record).has_positive_influence(MAGIC));

        let plain_record = plain(&[(STR, PHYSICAL)]);
        assert!(Record::Plain(&plain_record).is_additive(PHYSICAL));
        assert!(Record::Plain(&plain_record).has_positive_influence(PHYSICAL));
    }

    #[test]
    fn resolve_reads_the_reinforce_level_and_the_extended_record_when_present() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("data")
            .join("phase1");
        let mut data = crate::data::load_game_data(path).expect("load vanilla data");
        let weapon = data
            .weapons
            .iter()
            .find(|weapon| weapon.name == "Smithscript Axe" && weapon.affinity == "Flame Art")
            .expect("weapon fixture")
            .clone();

        let resolved = Correction::resolve(&weapon, 0, &data).unwrap();
        assert!(matches!(resolved.record, Record::Ext(_)));
        assert_eq!(resolved.weapon_scaling, weapon.scaling);
        data.attack_element_correct_ext
            .remove(&weapon.attack_element_correct_id)
            .expect("extended record");
        let resolved = Correction::resolve(&weapon, 0, &data).unwrap();
        assert!(matches!(resolved.record, Record::Plain(_)));
        let error = Correction::resolve(&weapon, u8::MAX, &data).unwrap_err();
        assert!(error.contains("missing reinforce level"), "{error}");
    }

    /// Active-stat selection reads the AR rule for skill rows as well (`damage_type_stat_can_scale`
    /// in the optimizer's preparation). That is sound only while the two rules agree on which
    /// stats can move a result; this fails if a snapshot adds a record where they would not.
    #[test]
    fn shipped_records_keep_both_rules_in_agreement() {
        for profile in [
            crate::data::VANILLA_PROFILE_ID,
            crate::data::CONVERGENCE_PROFILE_ID,
        ] {
            let data = crate::data::load_embedded_game_profile(profile).expect("profile");
            let mut checked = 0;
            for (id, record) in &data.attack_element_correct_ext {
                for stat in 0..COMBAT_STAT_COUNT {
                    for damage in 0..DAMAGE_TYPE_COUNT {
                        if !record.stat_scales(stat, damage) {
                            continue;
                        }
                        let influence = record.influence_rate(stat, damage);
                        assert!(
                            influence > 0.0,
                            "{profile} record {id} stat {stat} damage {damage}: influence {influence}"
                        );
                        assert!(
                            record.overwrite_rate(stat, damage).is_none() || influence == 1.0,
                            "{profile} record {id} stat {stat} damage {damage}: overwrite with influence {influence}"
                        );
                        checked += 1;
                    }
                }
            }
            assert!(checked > 100, "{profile}: {checked}");
        }
    }
}
