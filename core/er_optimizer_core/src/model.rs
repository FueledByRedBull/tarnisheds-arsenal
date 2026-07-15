use std::collections::{HashMap, HashSet};
use std::fmt;

pub const STAT_STR: usize = 0;
pub const STAT_DEX: usize = 1;
pub const STAT_INT: usize = 2;
pub const STAT_FAI: usize = 3;
pub const STAT_ARC: usize = 4;
pub const COMBAT_STAT_COUNT: usize = 5;
pub const DAMAGE_TYPE_COUNT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub vig: u8,
    pub mnd: u8,
    pub end: u8,
    pub str: u8,
    pub dex: u8,
    pub int: u8,
    pub fai: u8,
    pub arc: u8,
}

impl Stats {
    pub fn sum_all_8(self) -> u16 {
        u16::from(self.vig)
            + u16::from(self.mnd)
            + u16::from(self.end)
            + u16::from(self.str)
            + u16::from(self.dex)
            + u16::from(self.int)
            + u16::from(self.fai)
            + u16::from(self.arc)
    }

    pub fn combat_array(self) -> [u8; COMBAT_STAT_COUNT] {
        [self.str, self.dex, self.int, self.fai, self.arc]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamageType {
    Physical = 0,
    Magic = 1,
    Fire = 2,
    Lightning = 3,
    Holy = 4,
}

impl DamageType {
    pub const ALL: [DamageType; DAMAGE_TYPE_COUNT] = [
        DamageType::Physical,
        DamageType::Magic,
        DamageType::Fire,
        DamageType::Lightning,
        DamageType::Holy,
    ];

    pub fn as_index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for DamageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            DamageType::Physical => "physical",
            DamageType::Magic => "magic",
            DamageType::Fire => "fire",
            DamageType::Lightning => "lightning",
            DamageType::Holy => "holy",
        };
        write!(f, "{value}")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhysicalAttackAttribute {
    #[default]
    Standard,
    Strike,
    Slash,
    Pierce,
    AdaptivePrimary,
    AdaptiveSecondary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaminaCostMode {
    WeaponScaled,
    Precalculated,
}

impl fmt::Display for PhysicalAttackAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Standard => "standard",
            Self::Strike => "strike",
            Self::Slash => "slash",
            Self::Pierce => "pierce",
            Self::AdaptivePrimary => "adaptive_primary",
            Self::AdaptiveSecondary => "adaptive_secondary",
        };
        write!(f, "{value}")
    }
}

#[derive(Clone, Debug)]
pub struct Weapon {
    pub weapon_id: u32,
    pub name: String,
    pub affinity: String,
    pub native_skill_id: Option<u16>,
    pub native_skill_name: Option<String>,
    pub weapon_type_id: u16,
    pub weapon_type_name: String,
    pub weapon_type_keys: String,
    pub stamina_consumption_rate: f32,
    pub physical_attributes: [PhysicalAttackAttribute; 2],
    pub base: [f32; DAMAGE_TYPE_COUNT],
    pub scaling: [f32; COMBAT_STAT_COUNT],
    pub requirements: [u8; COMBAT_STAT_COUNT],
    pub reinforce_type: u16,
    pub attack_element_correct_id: usize,
    pub damage_curve_ids: [usize; DAMAGE_TYPE_COUNT],
    pub bleed_curve_id: usize,
    pub disable_gem_attr: bool,
    pub is_somber: bool,
    pub disable_two_hand_bonus: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ReinforceLevel {
    pub damage_mult: [f32; DAMAGE_TYPE_COUNT],
    pub scaling_mult: [f32; COMBAT_STAT_COUNT],
    pub base_attack_mult: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AttackElementCorrect {
    pub scales: [[bool; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
}

impl AttackElementCorrect {
    pub fn stat_scales(self, stat_idx: usize, damage_type: DamageType) -> bool {
        self.scales[stat_idx][damage_type.as_index()]
    }
}

#[derive(Clone, Debug)]
pub struct AttackElementCorrectExt {
    pub scales: [[bool; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
    pub overwrite: [[Option<f32>; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
    pub influence: [[f32; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
}

impl AttackElementCorrectExt {
    pub fn stat_scales(&self, stat_idx: usize, damage_idx: usize) -> bool {
        self.scales[stat_idx][damage_idx]
    }

    pub fn overwrite_rate(&self, stat_idx: usize, damage_idx: usize) -> Option<f32> {
        self.overwrite[stat_idx][damage_idx]
    }

    pub fn influence_rate(&self, stat_idx: usize, damage_idx: usize) -> f32 {
        self.influence[stat_idx][damage_idx]
    }
}

#[derive(Clone, Debug)]
pub struct AowAttackRow {
    pub sheet_row: u16,
    pub aow_id: u16,
    pub aow_name: String,
    pub raw_name: String,
    pub variant_weapon_type: String,
    pub sequence_variant: String,
    pub hit_kind: String,
    pub hit_order: u16,
    pub is_lacking_fp: bool,
    pub atk_id: u32,
    pub overwrite_attack_element_correct_id: Option<usize>,
    pub is_disable_both_hands_bonus: bool,
    pub is_add_base_atk: bool,
    pub is_arrow_attack: bool,
    pub physical_attack_attribute: PhysicalAttackAttribute,
    pub motion_values: [f32; DAMAGE_TYPE_COUNT],
    pub attack_base: [f32; DAMAGE_TYPE_COUNT],
    pub status_mv: f32,
    pub weapon_buff_mv: f32,
    pub stamina_cost: f32,
    pub stamina_cost_mode: StaminaCostMode,
}

impl AowAttackRow {
    pub fn is_damaging(&self) -> bool {
        self.motion_values.iter().any(|value| *value > 0.0)
            || ((self.is_add_base_atk || self.is_arrow_attack)
                && self.attack_base.iter().any(|value| *value > 0.0))
    }

    pub fn resolved_physical_attribute(&self, weapon: &Weapon) -> PhysicalAttackAttribute {
        match self.physical_attack_attribute {
            PhysicalAttackAttribute::AdaptivePrimary => weapon.physical_attributes[0],
            PhysicalAttackAttribute::AdaptiveSecondary => weapon.physical_attributes[1],
            fixed => fixed,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatusBuildup {
    pub bleed: f32,
    pub frost: f32,
    pub poison: f32,
    pub scarlet_rot: f32,
    pub sleep: f32,
    pub madness: f32,
    pub death: f32,
}

impl StatusBuildup {
    pub fn scale(self, multiplier: f32) -> Self {
        Self {
            bleed: self.bleed * multiplier,
            frost: self.frost * multiplier,
            poison: self.poison * multiplier,
            scarlet_rot: self.scarlet_rot * multiplier,
            sleep: self.sleep * multiplier,
            madness: self.madness * multiplier,
            death: self.death * multiplier,
        }
    }

    pub fn combined_with(self, other: Self) -> Self {
        Self {
            bleed: self.bleed + other.bleed,
            frost: self.frost + other.frost,
            poison: self.poison + other.poison,
            scarlet_rot: self.scarlet_rot + other.scarlet_rot,
            sleep: self.sleep + other.sleep,
            madness: self.madness + other.madness,
            death: self.death + other.death,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatusCorrectionFlags {
    pub bleed: Option<bool>,
    pub frost: Option<bool>,
    pub poison: Option<bool>,
    pub scarlet_rot: Option<bool>,
    pub sleep: Option<bool>,
    pub madness: Option<bool>,
    pub death: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatusEffectSource {
    pub buildup: StatusBuildup,
    pub correction_flags: StatusCorrectionFlags,
}

impl StatusBuildup {
    pub fn with_aow_additions(self, aow: Option<&Aow>) -> Self {
        let Some(aow) = aow else {
            return self;
        };
        Self {
            bleed: self.bleed + aow.bleed_buildup_add,
            frost: self.frost + aow.frost_buildup_add,
            poison: self.poison + aow.poison_buildup_add,
            scarlet_rot: self.scarlet_rot + aow.scarlet_rot_buildup_add,
            sleep: self.sleep,
            madness: self.madness,
            death: self.death,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Aow {
    pub aow_id: u16,
    pub name: String,
    pub bleed_buildup_add: f32,
    pub frost_buildup_add: f32,
    pub poison_buildup_add: f32,
    pub scarlet_rot_buildup_add: f32,
    pub valid_weapon_types: String,
    pub buff_attack_power: [f32; DAMAGE_TYPE_COUNT],
    pub scaling_status_add: StatusBuildup,
    pub scaling_status_flags: StatusCorrectionFlags,
    pub persistent_weapon_status_add: StatusBuildup,
    pub persistent_on_hit_status_add: StatusBuildup,
    pub buff_activation_action_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AowEffectRole {
    PersistentSetup,
    PersistentWeaponBuff,
    PersistentOnHit,
    PerHitStatus,
    PerHitAttackPower,
    SelfBuff,
    SelfMechanic,
    ReplacementOrChained,
    VisualOrNonGameplay,
}

impl AowEffectRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PersistentSetup => "persistent_setup",
            Self::PersistentWeaponBuff => "persistent_weapon_buff",
            Self::PersistentOnHit => "persistent_on_hit",
            Self::PerHitStatus => "per_hit_status",
            Self::PerHitAttackPower => "per_hit_attack_power",
            Self::SelfBuff => "self_buff",
            Self::SelfMechanic => "self_mechanic",
            Self::ReplacementOrChained => "replacement_or_chained",
            Self::VisualOrNonGameplay => "visual_or_non_gameplay",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AowEffect {
    pub record_id: u32,
    pub aow_id: u16,
    pub sheet_row: u16,
    pub source_kind: String,
    pub source_param_ids: Vec<u32>,
    pub effect_id: u32,
    pub effect_name: String,
    pub parent_effect_id: Option<u32>,
    pub link_kind: String,
    pub role: AowEffectRole,
    pub activation_action_id: String,
    pub activation_timing: String,
    pub hand_variant: String,
    pub is_canonical: Option<bool>,
    pub is_supported: bool,
    pub reason: String,
    pub duration_seconds: f32,
    pub attack_power: [f32; DAMAGE_TYPE_COUNT],
    pub status_buildup: StatusBuildup,
    pub uses_status_correction: bool,
    pub uses_attack_correction: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DamageBreakdown {
    pub physical: f32,
    pub magic: f32,
    pub fire: f32,
    pub lightning: f32,
    pub holy: f32,
}

impl DamageBreakdown {
    pub fn total(self) -> f32 {
        self.physical + self.magic + self.fire + self.lightning + self.holy
    }

    pub fn scale(self, multiplier: f32) -> Self {
        Self {
            physical: self.physical * multiplier,
            magic: self.magic * multiplier,
            fire: self.fire * multiplier,
            lightning: self.lightning * multiplier,
            holy: self.holy * multiplier,
        }
    }

    pub fn combined_with(self, other: Self) -> Self {
        Self {
            physical: self.physical + other.physical,
            magic: self.magic + other.magic,
            fire: self.fire + other.fire,
            lightning: self.lightning + other.lightning,
            holy: self.holy + other.holy,
        }
    }

    pub fn by_type(self, damage_type: DamageType) -> f32 {
        match damage_type {
            DamageType::Physical => self.physical,
            DamageType::Magic => self.magic,
            DamageType::Fire => self.fire,
            DamageType::Lightning => self.lightning,
            DamageType::Holy => self.holy,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AowRouteAssignment {
    pub route_id: String,
    pub route_label: String,
    pub route_priority: u16,
    pub action_id: String,
    pub action_order: u16,
    pub hit_order: u16,
}

#[derive(Clone, Debug)]
pub struct AowHitResult {
    pub sheet_row: u16,
    pub hit_order: u16,
    pub raw_name: String,
    pub damage: DamageBreakdown,
    pub status_buildup: StatusBuildup,
    pub physical_attack_attribute: PhysicalAttackAttribute,
    pub buff_active: bool,
    pub effects: Vec<AowEffect>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AowActionResult {
    pub action_id: String,
    pub action_order: u16,
    pub stamina_cost: f32,
    pub hits: Vec<AowHitResult>,
}

#[derive(Clone, Debug)]
pub struct AowRouteResult {
    pub route_id: String,
    pub route_label: String,
    pub route_priority: u16,
    pub buff_activation_action_id: Option<String>,
    pub actions: Vec<AowActionResult>,
    pub first_hit_damage: f32,
    pub total_damage: DamageBreakdown,
    pub total_status_buildup: StatusBuildup,
    pub total_stamina_cost: f32,
}

#[derive(Clone, Debug, Default)]
pub struct GameData {
    pub snapshot_schema_version: u32,
    pub dataset_version: String,
    pub model_version: String,
    pub weapons: Vec<Weapon>,
    pub reinforce: Vec<Vec<Option<ReinforceLevel>>>,
    pub calc_correct: Vec<Vec<f32>>,
    pub attack_element_correct: Vec<Option<AttackElementCorrect>>,
    pub attack_element_correct_ext: HashMap<usize, AttackElementCorrectExt>,
    pub aows: Vec<Aow>,
    pub aow_attack_rows: HashMap<u16, Vec<AowAttackRow>>,
    pub native_skill_attack_rows: HashMap<u32, Vec<AowAttackRow>>,
    pub aow_route_assignments: HashMap<(u16, u16), Vec<AowRouteAssignment>>,
    pub aow_effects: HashMap<(u16, u16), Vec<AowEffect>>,
    pub weapon_passives: HashMap<u32, StatusEffectSource>,
    pub weapon_passive_overlays: HashMap<u32, Vec<Option<StatusEffectSource>>>,
    pub exact_aow_compat: HashSet<(u16, u32)>,
}

impl GameData {
    pub fn aow_effects(&self, aow_id: u16, sheet_row: u16) -> &[AowEffect] {
        self.aow_effects
            .get(&(aow_id, sheet_row))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn aow_compatible_with_weapon(&self, aow: &Aow, weapon: &Weapon) -> bool {
        if weapon.disable_gem_attr {
            return false;
        }
        if let Some(exact_match) = self.exact_aow_compatibility(aow.aow_id, weapon.weapon_id) {
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
        weapon
            .weapon_type_keys
            .split('|')
            .filter(|value| !value.is_empty())
            .any(|weapon_key| {
                aow.valid_weapon_types
                    .split('|')
                    .filter(|value| !value.is_empty())
                    .any(|valid_key| weapon_key == valid_key)
            })
    }

    pub fn aow_route_assignments(&self, aow_id: u16, sheet_row: u16) -> &[AowRouteAssignment] {
        self.aow_route_assignments
            .get(&(aow_id, sheet_row))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn reinforce_level(&self, reinforce_type: u16, level: u8) -> Option<&ReinforceLevel> {
        self.reinforce
            .get(usize::from(reinforce_type))
            .and_then(|levels| levels.get(usize::from(level)))
            .and_then(Option::as_ref)
    }

    pub fn calc_curve_value(&self, curve_id: usize, stat_value: u16) -> Option<f32> {
        self.calc_correct
            .get(curve_id)
            .and_then(|curve| curve.get(usize::from(stat_value)))
            .copied()
    }

    pub fn attack_element(
        &self,
        attack_element_correct_id: usize,
    ) -> Option<&AttackElementCorrect> {
        self.attack_element_correct
            .get(attack_element_correct_id)
            .and_then(Option::as_ref)
    }

    pub fn attack_element_ext(
        &self,
        attack_element_correct_id: usize,
    ) -> Option<&AttackElementCorrectExt> {
        self.attack_element_correct_ext
            .get(&attack_element_correct_id)
    }

    pub fn aow_attack_rows(&self, aow_id: u16) -> &[AowAttackRow] {
        self.aow_attack_rows
            .get(&aow_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn native_skill_attack_rows(&self, weapon_id: u32) -> &[AowAttackRow] {
        self.native_skill_attack_rows
            .get(&weapon_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn weapon_passive(&self, weapon_id: u32) -> StatusEffectSource {
        self.weapon_passives
            .get(&weapon_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn weapon_passive_overlay(&self, weapon_id: u32, level: u8) -> Option<StatusEffectSource> {
        self.weapon_passive_overlays
            .get(&weapon_id)
            .and_then(|levels| levels.get(usize::from(level)))
            .and_then(|entry| *entry)
    }

    pub fn exact_aow_compatibility(&self, aow_id: u16, weapon_id: u32) -> Option<bool> {
        if self.exact_aow_compat.is_empty() {
            return None;
        }
        Some(self.exact_aow_compat.contains(&(aow_id, weapon_id)))
    }
}
