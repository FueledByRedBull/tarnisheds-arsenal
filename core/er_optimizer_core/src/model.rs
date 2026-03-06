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

#[derive(Clone, Debug)]
pub struct Weapon {
    pub weapon_id: u32,
    pub name: String,
    pub affinity: String,
    pub weapon_type_id: u16,
    pub weapon_type_name: String,
    pub weapon_type_keys: String,
    pub base: [f32; DAMAGE_TYPE_COUNT],
    pub scaling: [f32; COMBAT_STAT_COUNT],
    pub requirements: [u8; COMBAT_STAT_COUNT],
    pub reinforce_type: u16,
    pub attack_element_correct_id: usize,
    pub damage_curve_ids: [usize; DAMAGE_TYPE_COUNT],
    pub is_somber: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ReinforceLevel {
    pub damage_mult: [f32; DAMAGE_TYPE_COUNT],
    pub scaling_mult: [f32; COMBAT_STAT_COUNT],
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
pub struct Aow {
    pub aow_id: u16,
    pub name: String,
    pub bleed_buildup_add: f32,
    pub frost_buildup_add: f32,
    pub poison_buildup_add: f32,
    pub valid_weapon_types: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AttackElementCorrectExt {
    pub scales: [[bool; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
    pub overwrite: [[Option<f32>; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
    pub influence: [[f32; DAMAGE_TYPE_COUNT]; COMBAT_STAT_COUNT],
}

impl AttackElementCorrectExt {
    pub fn stat_scales(self, stat_idx: usize, damage_idx: usize) -> bool {
        self.scales[stat_idx][damage_idx]
    }

    pub fn overwrite_rate(self, stat_idx: usize, damage_idx: usize) -> Option<f32> {
        self.overwrite[stat_idx][damage_idx]
    }

    pub fn influence_rate(self, stat_idx: usize, damage_idx: usize) -> f32 {
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
    pub is_lacking_fp: bool,
    pub atk_id: u32,
    pub overwrite_attack_element_correct_id: Option<usize>,
    pub is_disable_both_hands_bonus: bool,
    pub is_add_base_atk: bool,
    pub motion_values: [f32; DAMAGE_TYPE_COUNT],
    pub attack_base: [f32; DAMAGE_TYPE_COUNT],
    pub status_mv: f32,
    pub weapon_buff_mv: f32,
    pub stamina_cost: f32,
}

impl AowAttackRow {
    pub fn is_damaging(&self) -> bool {
        self.motion_values.iter().any(|value| *value > 0.0)
            || self.attack_base.iter().any(|value| *value > 0.0)
            || self.is_add_base_atk
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatusBuildup {
    pub bleed: f32,
    pub frost: f32,
    pub poison: f32,
    pub sleep: f32,
    pub madness: f32,
    pub death: f32,
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
            sleep: self.sleep,
            madness: self.madness,
            death: self.death,
        }
    }
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

#[derive(Clone, Debug, Default)]
pub struct GameData {
    pub weapons: Vec<Weapon>,
    pub reinforce: Vec<Vec<Option<ReinforceLevel>>>,
    pub calc_correct: Vec<Vec<f32>>,
    pub attack_element_correct: Vec<Option<AttackElementCorrect>>,
    pub attack_element_correct_ext: HashMap<usize, AttackElementCorrectExt>,
    pub aows: Vec<Aow>,
    pub aow_attack_rows: HashMap<u16, Vec<AowAttackRow>>,
    pub weapon_passives: HashMap<u32, StatusBuildup>,
    pub exact_aow_compat: HashSet<(u16, u32)>,
}

impl GameData {
    pub fn reinforce_level(&self, reinforce_type: u16, level: u8) -> Option<&ReinforceLevel> {
        self.reinforce
            .get(usize::from(reinforce_type))
            .and_then(|levels| levels.get(usize::from(level)))
            .and_then(Option::as_ref)
    }

    pub fn calc_curve_value(&self, curve_id: usize, stat_value: u8) -> Option<f32> {
        self.calc_correct
            .get(curve_id)
            .and_then(|curve| curve.get(usize::from(stat_value)))
            .copied()
    }

    pub fn attack_element(&self, attack_element_correct_id: usize) -> Option<&AttackElementCorrect> {
        self.attack_element_correct
            .get(attack_element_correct_id)
            .and_then(Option::as_ref)
    }

    pub fn attack_element_ext(
        &self,
        attack_element_correct_id: usize,
    ) -> Option<&AttackElementCorrectExt> {
        self.attack_element_correct_ext.get(&attack_element_correct_id)
    }

    pub fn aow_attack_rows(&self, aow_id: u16) -> &[AowAttackRow] {
        self.aow_attack_rows
            .get(&aow_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn weapon_passive(&self, weapon_id: u32) -> StatusBuildup {
        self.weapon_passives
            .get(&weapon_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn exact_aow_compatibility(&self, aow_id: u16, weapon_id: u32) -> Option<bool> {
        if self.exact_aow_compat.is_empty() {
            return None;
        }
        Some(self.exact_aow_compat.contains(&(aow_id, weapon_id)))
    }
}
