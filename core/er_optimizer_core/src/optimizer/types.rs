use crate::math::scadutree_attack_multiplier;
use crate::model::{AowRouteResult, COMBAT_STAT_COUNT, DamageBreakdown, Stats};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizeObjective {
    MaxAr,
    MaxPhysicalAr,
    BleedThenAr,
    AowFirstHit,
    AowFullSequence,
}

impl OptimizeObjective {
    pub const ALL: [Self; 5] = [
        Self::MaxAr,
        Self::MaxPhysicalAr,
        Self::BleedThenAr,
        Self::AowFirstHit,
        Self::AowFullSequence,
    ];

    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "max_ar" | "ar" | "total_ar" => Ok(Self::MaxAr),
            "max_physical_ar" | "max_phys_ar" | "max_phy_ar" | "physical" => {
                Ok(Self::MaxPhysicalAr)
            }
            "max_ar_plus_bleed" | "max_ar+bleed" | "max_ar_plus_bleed_buildup" | "bleed" => {
                Ok(Self::BleedThenAr)
            }
            "aow_first_hit" | "max_aow_first_hit" | "first_hit" => Ok(Self::AowFirstHit),
            "aow_full_sequence" | "max_aow_full_sequence" | "aow_full" | "full_sequence" => {
                Ok(Self::AowFullSequence)
            }
            _ => Err(format!(
                "invalid objective '{raw}', expected max_ar, max_physical_ar, max_ar_plus_bleed, aow_first_hit, or aow_full_sequence"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxAr => "max_ar",
            Self::MaxPhysicalAr => "max_physical_ar",
            Self::BleedThenAr => "max_ar_plus_bleed",
            Self::AowFirstHit => "aow_first_hit",
            Self::AowFullSequence => "aow_full_sequence",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SomberFilter {
    All,
    StandardOnly,
    SomberOnly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultGrouping {
    #[default]
    Automatic,
    Weapon,
    Loadout,
}

impl ResultGrouping {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "automatic" | "auto" => Ok(Self::Automatic),
            "weapon" | "weapon_only" => Ok(Self::Weapon),
            "loadout" | "setup" => Ok(Self::Loadout),
            _ => Err(format!(
                "invalid result grouping '{raw}', expected automatic, weapon, or loadout"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Weapon => "weapon",
            Self::Loadout => "loadout",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterDimension {
    WeaponFamily,
    WeaponType,
    Affinity,
    Aow,
    Reinforcement,
    Coverage,
}

impl FilterDimension {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "weapon_family" => Ok(Self::WeaponFamily),
            "weapon_type" => Ok(Self::WeaponType),
            "affinity" => Ok(Self::Affinity),
            "aow" => Ok(Self::Aow),
            "reinforcement" => Ok(Self::Reinforcement),
            "coverage" => Ok(Self::Coverage),
            _ => Err(format!("invalid filter dimension '{raw}'")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Include,
    Exclude,
}

impl FilterMode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "include" => Ok(Self::Include),
            "exclude" => Ok(Self::Exclude),
            _ => Err(format!("invalid filter mode '{raw}'")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StableFilter {
    pub dimension: FilterDimension,
    pub id: String,
    pub mode: FilterMode,
}

impl SomberFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::StandardOnly, Self::SomberOnly];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::StandardOnly => "standard_only",
            Self::SomberOnly => "somber_only",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OptimizeRequest {
    pub class_name: String,
    pub character_level: u16,
    pub current_stats: Stats,
    pub min_combat_stats: [u8; COMBAT_STAT_COUNT],
    pub locked_combat_stats: [Option<u8>; COMBAT_STAT_COUNT],
    pub standard_max_upgrade: u8,
    pub somber_max_upgrade: u8,
    pub exact_upgrade: bool,
    pub two_handing: bool,
    pub dlc_scaling: bool,
    pub scadutree_level: u8,
    pub weapon_name: Option<String>,
    pub affinity: Option<String>,
    pub aow_name: Option<String>,
    pub weapon_type_key: Option<String>,
    pub somber_filter: SomberFilter,
    pub filters: Vec<StableFilter>,
    pub result_grouping: ResultGrouping,
    pub objective: OptimizeObjective,
    pub top_k: usize,
}

impl OptimizeRequest {
    pub fn damage_multiplier(&self) -> f32 {
        scadutree_attack_multiplier(self.dlc_scaling, self.scadutree_level)
    }
}

#[derive(Clone, Debug)]
pub struct OptimizeResult {
    pub weapon_id: u32,
    pub weapon_name: String,
    pub weapon_type_name: String,
    pub affinity: String,
    pub is_somber: bool,
    pub upgrade: u8,
    pub stats: Stats,
    pub requirements: [u8; COMBAT_STAT_COUNT],
    pub effective_scaling: [f32; COMBAT_STAT_COUNT],
    pub ar: DamageBreakdown,
    pub aow_id: Option<u16>,
    pub aow_name: Option<String>,
    pub bleed_buildup: f32,
    pub bleed_buildup_add: f32,
    pub frost_buildup: f32,
    pub poison_buildup: f32,
    pub scarlet_rot_buildup: f32,
    pub sleep_buildup: f32,
    pub madness_buildup: f32,
    pub death_buildup: f32,
    pub aow_first_hit_damage: f32,
    pub aow_full_sequence_damage: f32,
    pub aow_route: Option<AowRouteResult>,
    pub score: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchEstimate {
    pub weapon_candidates: usize,
    pub stat_candidates: u64,
    pub combinations: u64,
}

#[derive(Clone, Debug)]
pub struct LevelOptimizeResult {
    pub level: u16,
    pub rows: Vec<OptimizeResult>,
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
pub struct OptimizePhaseTimings {
    pub preparation: Duration,
    pub scoring: Duration,
    pub materialization: Duration,
}

#[derive(Clone, Debug)]
pub struct ProfiledOptimizeResult {
    pub rows: Vec<OptimizeResult>,
    pub timings: OptimizePhaseTimings,
    pub estimate: SearchEstimate,
}
