use er_optimizer_core::model::COMBAT_STAT_COUNT;
use er_optimizer_core::model::{
    AowActionResult, AowEffect, AowHitResult, AowRouteResult, StatusBuildup,
};
use er_optimizer_core::{
    DamageBreakdown, FilterDimension, FilterMode, OptimizeObjective, OptimizeRequest,
    OptimizeResult, ProgressSnapshot, ResultGrouping, SomberFilter, StableFilter, Stats,
};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

pub const MAX_TOP_K: usize = 2_000;
pub const MAX_LEVELS_AHEAD: u16 = 200;
pub const MAX_PATH_BATCH: usize = 2;
/// Longest accepted text value, in bytes; the longest catalog name is well below it.
pub const MAX_TEXT_LEN: usize = 256;
pub const MAX_FILTER_ENTRIES: usize = 512;
/// Longest accepted filter ID, in bytes.
pub const MAX_FILTER_ID_LEN: usize = 128;
/// Highest value of any character stat.
pub const MAX_STAT: u8 = 99;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StableFilterEntryDto {
    pub dimension: String,
    pub id: String,
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StableFilterSetDto {
    pub version: u8,
    pub entries: Vec<StableFilterEntryDto>,
}

impl Default for StableFilterSetDto {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeRequestDto {
    pub profile_id: String,
    pub class_name: String,
    pub character_level: u16,
    pub vig: u8,
    pub mnd: u8,
    pub end: u8,
    pub str_stat: u8,
    pub dex: u8,
    pub int_stat: u8,
    pub fai: u8,
    pub arc: u8,
    pub min_str: u8,
    pub min_dex: u8,
    pub min_int: u8,
    pub min_fai: u8,
    pub min_arc: u8,
    pub lock_str: Option<u8>,
    pub lock_dex: Option<u8>,
    pub lock_int: Option<u8>,
    pub lock_fai: Option<u8>,
    pub lock_arc: Option<u8>,
    #[serde(default)]
    pub standard_max_upgrade: Option<u8>,
    #[serde(default)]
    pub somber_max_upgrade: Option<u8>,
    #[serde(default)]
    pub exact_upgrade: Option<bool>,
    pub two_handing: bool,
    #[serde(default)]
    pub dlc_scaling: bool,
    #[serde(default)]
    pub scadutree_level: u8,
    pub weapon_name: Option<String>,
    pub affinity: Option<String>,
    pub aow_name: Option<String>,
    pub weapon_type_key: Option<String>,
    pub somber_filter: String,
    #[serde(default)]
    pub filters: StableFilterSetDto,
    #[serde(default = "default_result_grouping")]
    pub result_grouping: String,
    pub objective: String,
    pub top_k: usize,
}

fn default_result_grouping() -> String {
    "automatic".to_string()
}

impl OptimizeRequestDto {
    pub fn exact_upgrade_enabled(&self) -> bool {
        self.exact_upgrade.unwrap_or(false)
    }

    /// Checks every field's shape and returns the core's request. A cap the client left
    /// out takes the given default, which the caller reads from the profile's rules.
    pub fn to_core(
        &self,
        default_standard_max_upgrade: u8,
        default_somber_max_upgrade: u8,
    ) -> Result<OptimizeRequest, AppError> {
        validate_optimize_request(self)?;
        Ok(OptimizeRequest {
            class_name: self.class_name.clone(),
            character_level: self.character_level,
            current_stats: Stats {
                vig: self.vig,
                mnd: self.mnd,
                end: self.end,
                str: self.str_stat,
                dex: self.dex,
                int: self.int_stat,
                fai: self.fai,
                arc: self.arc,
            },
            min_combat_stats: [
                self.min_str,
                self.min_dex,
                self.min_int,
                self.min_fai,
                self.min_arc,
            ],
            locked_combat_stats: [
                self.lock_str,
                self.lock_dex,
                self.lock_int,
                self.lock_fai,
                self.lock_arc,
            ],
            standard_max_upgrade: self
                .standard_max_upgrade
                .unwrap_or(default_standard_max_upgrade),
            somber_max_upgrade: self
                .somber_max_upgrade
                .unwrap_or(default_somber_max_upgrade),
            exact_upgrade: self.exact_upgrade_enabled(),
            two_handing: self.two_handing,
            dlc_scaling: self.dlc_scaling,
            scadutree_level: self.scadutree_level,
            weapon_name: self.weapon_name.clone(),
            affinity: self.affinity.clone(),
            aow_name: self.aow_name.clone(),
            weapon_type_key: self.weapon_type_key.clone(),
            somber_filter: parse_somber_filter(&self.somber_filter)?,
            filters: parse_stable_filters(&self.filters)?,
            result_grouping: ResultGrouping::parse(&self.result_grouping).map_err(|_| {
                invalid_choice("resultGrouping", &["automatic", "weapon", "loadout"])
            })?,
            objective: OptimizeObjective::parse(&self.objective).map_err(|_| {
                invalid_choice(
                    "objective",
                    &OptimizeObjective::ALL.map(OptimizeObjective::as_str),
                )
            })?,
            top_k: self.top_k,
        })
    }
}

/// For a request whose upgrade caps are already set, as in the test fixtures.
#[cfg(test)]
impl TryFrom<&OptimizeRequestDto> for OptimizeRequest {
    type Error = AppError;

    fn try_from(value: &OptimizeRequestDto) -> Result<Self, Self::Error> {
        let missing = |field| AppError::new(format!("{field} is required"));
        value.to_core(
            value
                .standard_max_upgrade
                .ok_or_else(|| missing("standardMaxUpgrade"))?,
            value
                .somber_max_upgrade
                .ok_or_else(|| missing("somberMaxUpgrade"))?,
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct CombatStateDto {
    pub str_stat: u8,
    pub dex: u8,
    pub int_stat: u8,
    pub fai: u8,
    pub arc: u8,
}

impl CombatStateDto {
    /// The stats in the core's combat order: Strength, Dexterity, Intelligence, Faith, Arcane.
    pub fn to_array(self) -> [u8; COMBAT_STAT_COUNT] {
        [self.str_stat, self.dex, self.int_stat, self.fai, self.arc]
    }

    pub fn from_array(stats: [u8; COMBAT_STAT_COUNT]) -> Self {
        let [str_stat, dex, int_stat, fai, arc] = stats;
        Self {
            str_stat,
            dex,
            int_stat,
            fai,
            arc,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageBreakdownDto {
    pub physical: f32,
    pub magic: f32,
    pub fire: f32,
    pub lightning: f32,
    pub holy: f32,
    pub total: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolvedBuildDto {
    pub weapon_id: u32,
    pub weapon_name: String,
    pub weapon_type_name: String,
    pub affinity: String,
    pub is_somber: bool,
    pub upgrade: u8,
    pub stats: CombatStateDto,
    pub requirements: CombatStateDto,
    pub effective_scaling: ScalingDto,
    pub ar: DamageBreakdownDto,
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
    pub aow_route: Option<AowRouteDto>,
    pub score: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusBuildupDto {
    pub bleed: f32,
    pub frost: f32,
    pub poison: f32,
    pub scarlet_rot: f32,
    pub sleep: f32,
    pub madness: f32,
    pub death: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AowEffectDto {
    pub effect_id: u32,
    pub effect_name: String,
    pub role: String,
    pub activation_timing: String,
    pub is_supported: bool,
    pub reason: String,
    pub attack_power: DamageBreakdownDto,
    pub status_buildup: StatusBuildupDto,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AowHitDto {
    pub sheet_row: u16,
    pub hit_order: u16,
    pub raw_name: String,
    pub damage: DamageBreakdownDto,
    pub poise_damage: f32,
    pub status_buildup: StatusBuildupDto,
    pub physical_attack_attribute: String,
    pub buff_active: bool,
    pub effects: Vec<AowEffectDto>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AowActionDto {
    pub action_id: String,
    pub action_order: u16,
    pub stamina_cost: f32,
    pub hits: Vec<AowHitDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AowRouteDto {
    pub route_id: String,
    pub route_label: String,
    pub route_priority: u16,
    pub buff_activation_action_id: Option<String>,
    pub actions: Vec<AowActionDto>,
    pub first_hit_damage: f32,
    pub total_damage: DamageBreakdownDto,
    pub total_poise_damage: f32,
    pub total_status_buildup: StatusBuildupDto,
    pub total_stamina_cost: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScalingDto {
    pub str: f32,
    pub dex: f32,
    pub int: f32,
    pub fai: f32,
    pub arc: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveBuildRequestDto {
    pub base: OptimizeRequestDto,
    pub weapon_name: String,
    pub affinity: Option<String>,
    pub aow_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArBleedFrontierRequestDto {
    pub base: OptimizeRequestDto,
    pub solved: SolvedBuildDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArBleedFrontierPointDto {
    pub result: SolvedBuildDto,
    pub ar_loss: f32,
    pub ar_loss_percent: f32,
    pub minimum_ar_loss_bps: u16,
    pub bleed_gain: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeSeriesRequestDto {
    pub base: OptimizeRequestDto,
    pub solved: SolvedBuildDto,
    pub max_upgrade: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradePointDto {
    pub upgrade: u8,
    pub metric: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPreviewRequestDto {
    pub base: OptimizeRequestDto,
    pub solved: SolvedBuildDto,
    pub levels_ahead: u16,
    pub title: String,
    #[serde(default)]
    pub mode: PathMode,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathMode {
    #[default]
    NoRespec,
    OptimumEnvelope,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStepDto {
    pub level: u16,
    pub stats: CombatStateDto,
    pub metric: Option<f32>,
    pub added_stat: Option<String>,
    pub requirement_gap: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPreviewDto {
    pub title: String,
    pub solved: SolvedBuildDto,
    pub steps: Vec<PathStepDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPathPreviewRequestDto {
    pub requests: Vec<PathPreviewRequestDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathProgressDto {
    pub job_id: String,
    pub checked: u64,
    pub total: u64,
    pub title: String,
    pub level: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathFinishedDto {
    pub job_id: String,
    pub cancelled: bool,
    pub paths: Vec<PathPreviewDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathJobStatusDto {
    pub progress: Option<PathProgressDto>,
    pub finished: Option<PathFinishedDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchRequestDto {
    pub base: OptimizeRequestDto,
    pub solved: SolvedBuildDto,
    pub levels_ahead: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchPointDto {
    pub level: u16,
    pub metric: Option<f32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchLineDto {
    pub affinity: String,
    pub points: Vec<AffinityWatchPointDto>,
    pub start_metric: Option<f32>,
    pub end_metric: Option<f32>,
    pub final_build: Option<SolvedBuildDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityBreakpointDto {
    pub level: u16,
    pub outgoing_affinity: String,
    pub incoming_affinity: String,
    pub outgoing_metric: Option<f32>,
    pub incoming_metric: Option<f32>,
    pub lead: Option<f32>,
    pub lead_percent: Option<f32>,
    pub quality: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchPayloadDto {
    pub lines: Vec<AffinityWatchLineDto>,
    pub breakpoints: Vec<AffinityBreakpointDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchProgressDto {
    pub job_id: String,
    pub checked: u64,
    pub total: u64,
    pub affinity: String,
    pub level: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchFinishedDto {
    pub job_id: String,
    pub cancelled: bool,
    pub payload: Option<AffinityWatchPayloadDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchJobStatusDto {
    pub progress: Option<AffinityWatchProgressDto>,
    pub finished: Option<AffinityWatchFinishedDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSearchResponseDto {
    pub job_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgressDto {
    pub job_id: String,
    pub checked: u64,
    pub total: u64,
    pub eligible: u64,
    pub best_score: f32,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFinishedDto {
    pub job_id: String,
    pub cancelled: bool,
    pub rows: Vec<SolvedBuildDto>,
    pub error: Option<String>,
    /// Setups the prepared search scores; zero when preparation failed.
    pub setups: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchJobStatusDto {
    pub progress: Option<SearchProgressDto>,
    pub finished: Option<SearchFinishedDto>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisJobKindDto {
    SolveBuild,
    UpgradeSeries,
    ArBleedFrontier,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFinishedDto {
    pub job_id: String,
    pub kind: AnalysisJobKindDto,
    pub cancelled: bool,
    pub result: Option<SolvedBuildDto>,
    pub points: Vec<UpgradePointDto>,
    pub frontier: Vec<ArBleedFrontierPointDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisJobStatusDto {
    pub finished: Option<AnalysisFinishedDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDto {
    pub weapon_names: Vec<String>,
    pub classes: Vec<ClassMetadataDto>,
    pub weapon_type_options: Vec<WeaponTypeOptionDto>,
    pub aow_names: Vec<String>,
    pub affinity_names: Vec<String>,
    pub objective_ids: Vec<String>,
    pub somber_filters: Vec<String>,
    pub filter_dimensions: Vec<FilterDimensionDto>,
    pub data_manifest: DataManifestDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterDimensionDto {
    pub id: String,
    pub label: String,
    pub options: Vec<FilterOptionDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptionDto {
    pub id: String,
    pub label: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataManifestDto {
    pub schema_version: u32,
    pub dataset_version: String,
    pub model_version: String,
    pub id: String,
    pub label: String,
    pub app_version: String,
    pub source: String,
    pub profile: ProfileMetadataDto,
    pub capabilities: ProfileCapabilitiesDto,
    pub rules: ProfileRulesDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMetadataDto {
    pub id: String,
    pub display_name: String,
    pub game_version: String,
    pub mod_version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCapabilitiesDto {
    pub class_budget: bool,
    pub status_buildup: bool,
    pub aow_damage: bool,
    pub aow_routes: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRulesDto {
    pub standard_max_upgrade: u8,
    pub somber_max_upgrade: u8,
    pub separate_upgrade_caps: bool,
    pub scadutree_scaling: bool,
    /// Outgoing damage multiplier per Scadutree Blessing level, from level 0.
    pub scadutree_attack_multipliers: Vec<f32>,
    pub zero_attack_element_uses_weapon_scaling: bool,
    pub extended_scaling_grades: bool,
}

impl From<er_optimizer_core::SnapshotManifest> for DataManifestDto {
    fn from(value: er_optimizer_core::SnapshotManifest) -> Self {
        Self {
            schema_version: value.schema_version,
            dataset_version: value.dataset_version,
            model_version: er_optimizer_core::runtime_model_version(&value.model_version),
            id: value.id,
            label: value.label,
            app_version: value.app_version,
            source: value.source,
            profile: ProfileMetadataDto {
                id: value.profile.id,
                display_name: value.profile.display_name,
                game_version: value.profile.game_version,
                mod_version: value.profile.mod_version,
            },
            capabilities: ProfileCapabilitiesDto {
                class_budget: value.capabilities.class_budget,
                status_buildup: value.capabilities.status_buildup,
                aow_damage: value.capabilities.aow_damage,
                aow_routes: value.capabilities.aow_routes,
            },
            rules: ProfileRulesDto {
                standard_max_upgrade: value.rules.standard_max_upgrade,
                somber_max_upgrade: value.rules.somber_max_upgrade,
                separate_upgrade_caps: value.rules.separate_upgrade_caps,
                scadutree_scaling: value.rules.scadutree_scaling,
                scadutree_attack_multipliers: er_optimizer_core::math::SCADUTREE_ATTACK_MULTIPLIERS
                    .to_vec(),
                zero_attack_element_uses_weapon_scaling: value
                    .rules
                    .zero_attack_element_uses_weapon_scaling,
                extended_scaling_grades: value.rules.extended_scaling_grades,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMetadataDto {
    pub name: String,
    pub base_level: u16,
    pub base_total: u16,
    pub base_stats: EightStatsDto,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EightStatsDto {
    pub vig: u8,
    pub mnd: u8,
    pub end: u8,
    pub str_stat: u8,
    pub dex: u8,
    pub int_stat: u8,
    pub fai: u8,
    pub arc: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponTypeOptionDto {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponProfileRequestDto {
    pub profile_id: String,
    pub weapon_name: String,
    pub affinity: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponProfileDto {
    pub can_change_aow: bool,
    pub native_skill_name: Option<String>,
    pub requirements: CombatStateDto,
    pub max_upgrade: u8,
    pub is_somber: bool,
    pub disables_two_hand_bonus: bool,
    pub forces_two_handing: bool,
    pub weight: f32,
    pub move_count: u16,
    pub one_handed_poise: DisplayPoiseDamageDto,
    pub two_handed_poise: DisplayPoiseDamageDto,
    pub affinities: Vec<String>,
    pub compatible_aows: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPoiseDamageDto {
    pub light: String,
    pub heavy: String,
    pub charged_heavy: String,
    pub jumping_light: String,
    pub jumping_heavy: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleAowsForAffinityRequestDto {
    pub profile_id: String,
    pub affinity: Option<String>,
}

impl From<DamageBreakdown> for DamageBreakdownDto {
    fn from(value: DamageBreakdown) -> Self {
        Self {
            physical: value.physical,
            magic: value.magic,
            fire: value.fire,
            lightning: value.lightning,
            holy: value.holy,
            total: value.total(),
        }
    }
}

impl From<StatusBuildup> for StatusBuildupDto {
    fn from(value: StatusBuildup) -> Self {
        Self {
            bleed: value.bleed,
            frost: value.frost,
            poison: value.poison,
            scarlet_rot: value.scarlet_rot,
            sleep: value.sleep,
            madness: value.madness,
            death: value.death,
        }
    }
}

impl From<AowEffect> for AowEffectDto {
    fn from(value: AowEffect) -> Self {
        Self {
            effect_id: value.effect_id,
            effect_name: value.effect_name,
            role: value.role.as_str().to_string(),
            activation_timing: value.activation_timing,
            is_supported: value.is_supported,
            reason: value.reason,
            attack_power: DamageBreakdown {
                physical: value.attack_power[0],
                magic: value.attack_power[1],
                fire: value.attack_power[2],
                lightning: value.attack_power[3],
                holy: value.attack_power[4],
            }
            .into(),
            status_buildup: value.status_buildup.into(),
        }
    }
}

impl From<AowHitResult> for AowHitDto {
    fn from(value: AowHitResult) -> Self {
        Self {
            sheet_row: value.sheet_row,
            hit_order: value.hit_order,
            raw_name: value.raw_name,
            damage: value.damage.into(),
            poise_damage: value.poise_damage,
            status_buildup: value.status_buildup.into(),
            physical_attack_attribute: value.physical_attack_attribute.to_string(),
            buff_active: value.buff_active,
            effects: value.effects.into_iter().map(AowEffectDto::from).collect(),
            warnings: value.warnings,
        }
    }
}

impl From<AowActionResult> for AowActionDto {
    fn from(value: AowActionResult) -> Self {
        Self {
            action_id: value.action_id,
            action_order: value.action_order,
            stamina_cost: value.stamina_cost,
            hits: value.hits.into_iter().map(AowHitDto::from).collect(),
        }
    }
}

impl From<AowRouteResult> for AowRouteDto {
    fn from(value: AowRouteResult) -> Self {
        Self {
            route_id: value.route_id,
            route_label: value.route_label,
            route_priority: value.route_priority,
            buff_activation_action_id: value.buff_activation_action_id,
            actions: value.actions.into_iter().map(AowActionDto::from).collect(),
            first_hit_damage: value.first_hit_damage,
            total_damage: value.total_damage.into(),
            total_poise_damage: value.total_poise_damage,
            total_status_buildup: value.total_status_buildup.into(),
            total_stamina_cost: value.total_stamina_cost,
        }
    }
}

impl From<OptimizeResult> for SolvedBuildDto {
    fn from(value: OptimizeResult) -> Self {
        Self {
            weapon_id: value.weapon_id,
            weapon_name: value.weapon_name,
            weapon_type_name: value.weapon_type_name,
            affinity: value.affinity,
            is_somber: value.is_somber,
            upgrade: value.upgrade,
            stats: CombatStateDto {
                str_stat: value.stats.str,
                dex: value.stats.dex,
                int_stat: value.stats.int,
                fai: value.stats.fai,
                arc: value.stats.arc,
            },
            requirements: CombatStateDto {
                str_stat: value.requirements[0],
                dex: value.requirements[1],
                int_stat: value.requirements[2],
                fai: value.requirements[3],
                arc: value.requirements[4],
            },
            effective_scaling: ScalingDto {
                str: value.effective_scaling[0],
                dex: value.effective_scaling[1],
                int: value.effective_scaling[2],
                fai: value.effective_scaling[3],
                arc: value.effective_scaling[4],
            },
            ar: value.ar.into(),
            aow_id: value.aow_id,
            aow_name: value.aow_name,
            bleed_buildup: value.bleed_buildup,
            bleed_buildup_add: value.bleed_buildup_add,
            frost_buildup: value.frost_buildup,
            poison_buildup: value.poison_buildup,
            scarlet_rot_buildup: value.scarlet_rot_buildup,
            sleep_buildup: value.sleep_buildup,
            madness_buildup: value.madness_buildup,
            death_buildup: value.death_buildup,
            aow_first_hit_damage: value.aow_first_hit_damage,
            aow_full_sequence_damage: value.aow_full_sequence_damage,
            aow_route: value.aow_route.map(AowRouteDto::from),
            score: value.score,
        }
    }
}

impl SearchProgressDto {
    pub fn new(job_id: String, value: ProgressSnapshot) -> Self {
        Self {
            job_id,
            checked: value.checked,
            total: value.total,
            eligible: value.eligible,
            best_score: value.best_score,
            elapsed_ms: value.elapsed_ms,
        }
    }
}

/// A job's terminal payload, as the registry publishes it.
pub trait JobFinished {
    fn has_error(&self) -> bool;
    /// Marks the job cancelled and drops its payload.
    fn cancel(&mut self);
}

impl JobFinished for SearchFinishedDto {
    fn has_error(&self) -> bool {
        self.error.is_some()
    }

    fn cancel(&mut self) {
        self.cancelled = true;
        self.rows.clear();
    }
}

impl JobFinished for AnalysisFinishedDto {
    fn has_error(&self) -> bool {
        self.error.is_some()
    }

    fn cancel(&mut self) {
        self.cancelled = true;
        self.result = None;
        self.points.clear();
        self.frontier.clear();
    }
}

impl JobFinished for AffinityWatchFinishedDto {
    fn has_error(&self) -> bool {
        self.error.is_some()
    }

    fn cancel(&mut self) {
        self.cancelled = true;
        self.payload = None;
    }
}

/// The terminal payload to publish for a worker outcome. A worker that stopped without
/// a result becomes `failed`; a cancellation requested before publication clears a
/// completed payload, but an error outranks it.
pub fn finish<T: JobFinished>(
    outcome: Result<T, String>,
    cancelled: bool,
    failed: impl FnOnce(String) -> T,
) -> T {
    let mut finished = outcome.unwrap_or_else(failed);
    if cancelled && !finished.has_error() {
        finished.cancel();
    }
    finished
}

/// Rejects an oversized text value without echoing it.
pub fn check_len(field: &str, value: &str) -> Result<(), AppError> {
    if value.len() > MAX_TEXT_LEN {
        return Err(AppError::new(format!(
            "{field} must be at most {MAX_TEXT_LEN} bytes"
        )));
    }
    Ok(())
}

/// The rejection of a value outside a fixed set, which names the field and the set but
/// never the value.
fn invalid_choice(field: &str, accepted: &[&str]) -> AppError {
    AppError::new(format!("{field} must be one of {}", accepted.join(", ")))
}

fn validate_optimize_request(request: &OptimizeRequestDto) -> Result<(), AppError> {
    for (field, value) in [
        ("profileId", Some(request.profile_id.as_str())),
        ("className", Some(request.class_name.as_str())),
        ("weaponName", request.weapon_name.as_deref()),
        ("affinity", request.affinity.as_deref()),
        ("aowName", request.aow_name.as_deref()),
        ("weaponTypeKey", request.weapon_type_key.as_deref()),
        ("objective", Some(request.objective.as_str())),
        ("somberFilter", Some(request.somber_filter.as_str())),
        ("resultGrouping", Some(request.result_grouping.as_str())),
    ] {
        if let Some(value) = value {
            check_len(field, value)?;
        }
    }
    for (field, value) in [
        ("vig", request.vig),
        ("mnd", request.mnd),
        ("end", request.end),
        ("strStat", request.str_stat),
        ("dex", request.dex),
        ("intStat", request.int_stat),
        ("fai", request.fai),
        ("arc", request.arc),
        ("minStr", request.min_str),
        ("minDex", request.min_dex),
        ("minInt", request.min_int),
        ("minFai", request.min_fai),
        ("minArc", request.min_arc),
        ("lockStr", request.lock_str.unwrap_or(0)),
        ("lockDex", request.lock_dex.unwrap_or(0)),
        ("lockInt", request.lock_int.unwrap_or(0)),
        ("lockFai", request.lock_fai.unwrap_or(0)),
        ("lockArc", request.lock_arc.unwrap_or(0)),
    ] {
        if value > MAX_STAT {
            return Err(AppError::new(format!(
                "{field} must be {MAX_STAT} or lower; got {value}"
            )));
        }
    }
    if request.top_k > MAX_TOP_K {
        return Err(AppError::new(format!(
            "topK must be {MAX_TOP_K} or lower; got {}",
            request.top_k
        )));
    }
    if request.scadutree_level > er_optimizer_core::math::SCADUTREE_MAX_LEVEL {
        return Err(AppError::new(format!(
            "scadutreeLevel must be {} or lower; got {}",
            er_optimizer_core::math::SCADUTREE_MAX_LEVEL,
            request.scadutree_level
        )));
    }
    if request.filters.version != 1 {
        return Err(AppError::new(format!(
            "filters.version must be 1; got {}",
            request.filters.version
        )));
    }
    if request.filters.entries.len() > MAX_FILTER_ENTRIES {
        return Err(AppError::new(format!(
            "filters.entries must contain at most {MAX_FILTER_ENTRIES} entries"
        )));
    }
    Ok(())
}

fn parse_stable_filters(filters: &StableFilterSetDto) -> Result<Vec<StableFilter>, AppError> {
    filters
        .entries
        .iter()
        .map(|filter| {
            if filter.id.is_empty() || filter.id.len() > MAX_FILTER_ID_LEN {
                return Err(AppError::new(format!(
                    "filters.entries.id must contain 1 through {MAX_FILTER_ID_LEN} bytes"
                )));
            }
            check_len("filters.entries.dimension", &filter.dimension)?;
            check_len("filters.entries.mode", &filter.mode)?;
            Ok(StableFilter {
                dimension: FilterDimension::parse(&filter.dimension).map_err(|_| {
                    invalid_choice(
                        "filters.entries.dimension",
                        &[
                            "weapon_family",
                            "weapon_type",
                            "affinity",
                            "aow",
                            "reinforcement",
                            "coverage",
                        ],
                    )
                })?,
                id: filter.id.clone(),
                mode: FilterMode::parse(&filter.mode)
                    .map_err(|_| invalid_choice("filters.entries.mode", &["include", "exclude"]))?,
            })
        })
        .collect()
}

pub fn validate_levels_ahead(levels_ahead: u16) -> Result<(), AppError> {
    if levels_ahead > MAX_LEVELS_AHEAD {
        return Err(AppError::new(format!(
            "levelsAhead must be {MAX_LEVELS_AHEAD} or lower; got {levels_ahead}"
        )));
    }
    Ok(())
}

/// Checks a path lane's own fields; its base request is validated against the profile.
pub fn validate_path_lane(lane: &PathPreviewRequestDto) -> Result<(), AppError> {
    validate_levels_ahead(lane.levels_ahead)?;
    check_len("title", &lane.title)
}

pub fn validate_path_batch(count: usize) -> Result<(), AppError> {
    if count > MAX_PATH_BATCH {
        return Err(AppError::new(format!(
            "path batch requests must contain at most {MAX_PATH_BATCH} entries; got {count}"
        )));
    }
    Ok(())
}

fn parse_somber_filter(raw: &str) -> Result<SomberFilter, AppError> {
    SomberFilter::ALL
        .into_iter()
        .find(|filter| filter.as_str() == raw)
        .ok_or_else(|| invalid_choice("somberFilter", &SomberFilter::ALL.map(SomberFilter::as_str)))
}

pub fn metric_for_objective(solved: &SolvedBuildDto, objective: OptimizeObjective) -> f32 {
    match objective {
        OptimizeObjective::MaxPhysicalAr => solved.ar.physical,
        OptimizeObjective::AowFirstHit => solved.aow_first_hit_damage,
        OptimizeObjective::AowFullSequence => solved.aow_full_sequence_damage,
        OptimizeObjective::BleedThenAr => solved.bleed_buildup,
        OptimizeObjective::MaxAr => solved.ar.total,
    }
}

#[cfg(test)]
pub fn lock_request_to_stats(request: &mut OptimizeRequestDto, stats: CombatStateDto) {
    request.lock_str = Some(stats.str_stat);
    request.lock_dex = Some(stats.dex);
    request.lock_int = Some(stats.int_stat);
    request.lock_fai = Some(stats.fai);
    request.lock_arc = Some(stats.arc);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn export_sized_top_k_limit_is_explicitly_bounded() {
        let mut request = crate::test_optimize_request();
        request.top_k = MAX_TOP_K;
        validate_optimize_request(&request).expect("maximum export row count");
        request.top_k += 1;
        assert!(validate_optimize_request(&request).is_err());
    }

    #[test]
    fn profile_rules_send_the_core_scadutree_table() {
        let state = crate::test_app_state();
        let manifest = &state
            .profile(er_optimizer_core::VANILLA_PROFILE_ID)
            .unwrap()
            .data_manifest;
        // Tauri sends serde_json text, which writes the shortest f32 form, so the UI
        // reads the regulation rates themselves.
        let rules: Value =
            serde_json::from_str(&serde_json::to_string(&manifest.rules).unwrap()).unwrap();
        assert_eq!(
            rules["scadutreeAttackMultipliers"],
            json!([
                1.0, 1.1, 1.2, 1.25, 1.3, 1.35, 1.425, 1.5, 1.55, 1.6, 1.65, 1.75, 1.85, 1.875,
                1.9, 1.925, 1.95, 1.975, 2.0, 2.025, 2.05
            ])
        );
        assert_eq!(
            manifest.rules.scadutree_attack_multipliers.len(),
            usize::from(er_optimizer_core::math::SCADUTREE_MAX_LEVEL) + 1
        );
    }

    #[test]
    fn text_fields_accept_the_maximum_length_and_reject_one_more_without_echoing_it() {
        type TextSetter = fn(&mut OptimizeRequestDto, String);
        let fields: [(&str, TextSetter); 9] = [
            ("profileId", |request, text| request.profile_id = text),
            ("className", |request, text| request.class_name = text),
            ("weaponName", |request, text| {
                request.weapon_name = Some(text)
            }),
            ("affinity", |request, text| request.affinity = Some(text)),
            ("aowName", |request, text| request.aow_name = Some(text)),
            ("weaponTypeKey", |request, text| {
                request.weapon_type_key = Some(text);
            }),
            ("objective", |request, text| request.objective = text),
            ("somberFilter", |request, text| request.somber_filter = text),
            ("resultGrouping", |request, text| {
                request.result_grouping = text;
            }),
        ];
        for (field, set) in fields {
            let mut request = crate::test_optimize_request();
            set(&mut request, "x".repeat(MAX_TEXT_LEN));
            validate_optimize_request(&request).unwrap_or_else(|error| panic!("{field}: {error}"));
            set(&mut request, "x".repeat(MAX_TEXT_LEN + 1));
            let error = validate_optimize_request(&request).expect_err(field);
            assert_eq!(
                error.message,
                format!("{field} must be at most {MAX_TEXT_LEN} bytes")
            );
        }
        check_len("title", &"é".repeat(MAX_TEXT_LEN / 2)).expect("256 bytes of multibyte text");
        assert!(check_len("title", &"é".repeat(MAX_TEXT_LEN / 2 + 1)).is_err());
    }

    #[test]
    fn path_titles_are_bounded() {
        let state = crate::test_app_state();
        let base = crate::test_optimize_request();
        let solved = crate::commands::optimize::run_search_inner(base.clone(), &state)
            .expect("seed search succeeds")
            .pop()
            .expect("seed build exists");
        let mut lane = PathPreviewRequestDto {
            base,
            solved,
            levels_ahead: MAX_LEVELS_AHEAD,
            title: "x".repeat(MAX_TEXT_LEN),
            mode: PathMode::NoRespec,
        };
        validate_path_lane(&lane).expect("maximum title");
        lane.title.push('x');
        assert_eq!(
            validate_path_lane(&lane).unwrap_err().message,
            format!("title must be at most {MAX_TEXT_LEN} bytes")
        );
    }

    #[test]
    fn every_catalog_name_fits_the_text_limit() {
        let state = crate::test_app_state();
        for profile in state.profiles.values() {
            let data = &profile.data;
            let classes = crate::commands::data::class_metadata(profile);
            let names = data
                .weapons
                .iter()
                .flat_map(|weapon| [&weapon.name, &weapon.affinity, &weapon.weapon_type_keys])
                .chain(data.aows.iter().map(|aow| &aow.name))
                .chain(classes.iter().map(|class| &class.name));
            let longest = names.map(|name| name.len()).max().unwrap();
            assert!(longest <= MAX_TEXT_LEN, "{longest}");
        }
    }

    #[test]
    fn invalid_scadutree_level_is_rejected() {
        let mut request = crate::test_optimize_request();
        request.scadutree_level = er_optimizer_core::math::SCADUTREE_MAX_LEVEL + 1;
        assert!(validate_optimize_request(&request).is_err());
    }

    #[test]
    fn representative_analysis_payloads_use_app_contract_keys() {
        let path_value = serde_json::to_value(PathJobStatusDto {
            progress: Some(PathProgressDto {
                job_id: "path-1".to_string(),
                checked: 1,
                total: 2,
                title: "Selected".to_string(),
                level: 151,
            }),
            finished: Some(PathFinishedDto {
                job_id: "path-1".to_string(),
                cancelled: false,
                paths: vec![PathPreviewDto {
                    title: "Selected".to_string(),
                    solved: solved_build(),
                    steps: vec![PathStepDto {
                        level: 151,
                        stats: combat_state(),
                        metric: Some(10.0),
                        added_stat: Some("dex".to_string()),
                        requirement_gap: 0,
                    }],
                }],
                error: None,
            }),
        })
        .expect("path status serializes");

        assert_has_path(
            &path_value,
            &["finished", "paths", "0", "steps", "0", "addedStat"],
        );
        assert_has_path(
            &path_value,
            &["finished", "paths", "0", "steps", "0", "requirementGap"],
        );
        assert!(
            lookup_path(
                &path_value,
                &["finished", "paths", "0", "steps", "0", "score"]
            )
            .is_none()
        );

        let affinity_value = serde_json::to_value(AffinityWatchJobStatusDto {
            progress: Some(AffinityWatchProgressDto {
                job_id: "affinity-1".to_string(),
                checked: 1,
                total: 2,
                affinity: "Blood".to_string(),
                level: 151,
            }),
            finished: Some(AffinityWatchFinishedDto {
                job_id: "affinity-1".to_string(),
                cancelled: false,
                payload: Some(AffinityWatchPayloadDto {
                    lines: vec![AffinityWatchLineDto {
                        affinity: "Blood".to_string(),
                        points: vec![AffinityWatchPointDto {
                            level: 151,
                            metric: Some(10.0),
                        }],
                        start_metric: Some(8.0),
                        end_metric: Some(10.0),
                        final_build: Some(solved_build()),
                    }],
                    breakpoints: vec![AffinityBreakpointDto {
                        level: 151,
                        outgoing_affinity: "Keen".to_string(),
                        incoming_affinity: "Blood".to_string(),
                        outgoing_metric: Some(9.0),
                        incoming_metric: Some(10.0),
                        lead: Some(1.0),
                        lead_percent: Some(11.111),
                        quality: "clear".to_string(),
                    }],
                }),
                error: None,
            }),
        })
        .expect("affinity status serializes");

        assert_eq!(
            affinity_value["finished"]["payload"]["lines"][0]["points"][0],
            json!({ "level": 151, "metric": 10.0 }),
        );

        assert_has_path(
            &affinity_value,
            &["finished", "payload", "lines", "0", "startMetric"],
        );
        assert_has_path(
            &affinity_value,
            &["finished", "payload", "lines", "0", "endMetric"],
        );
        assert_has_path(
            &affinity_value,
            &["finished", "payload", "lines", "0", "finalBuild"],
        );
        assert_has_path(
            &affinity_value,
            &[
                "finished",
                "payload",
                "breakpoints",
                "0",
                "incomingAffinity",
            ],
        );
    }

    #[test]
    fn optimize_request_deserializes_app_contract_keys() {
        let request: OptimizeRequestDto = serde_json::from_value(json!({
            "profileId": "vanilla",
            "className": "Samurai",
            "characterLevel": 150,
            "vig": 50,
            "mnd": 11,
            "end": 30,
            "strStat": 12,
            "dex": 60,
            "intStat": 9,
            "fai": 8,
            "arc": 45,
            "minStr": 12,
            "minDex": 15,
            "minInt": 9,
            "minFai": 8,
            "minArc": 8,
            "lockStr": null,
            "lockDex": null,
            "lockInt": null,
            "lockFai": null,
            "lockArc": null,
            "standardMaxUpgrade": 25,
            "somberMaxUpgrade": 10,
            "exactUpgrade": true,
            "twoHanding": false,
            "dlcScaling": true,
            "scadutreeLevel": 20,
            "weaponName": "Uchigatana",
            "affinity": "Blood",
            "aowName": "Seppuku",
            "weaponTypeKey": "katana",
            "somberFilter": "all",
            "objective": "max_ar",
            "topK": 10
        }))
        .expect("request deserializes");

        assert_eq!(request.class_name, "Samurai");
        assert_eq!(request.profile_id, "vanilla");
        assert_eq!(request.str_stat, 12);
        assert_eq!(request.int_stat, 9);
        assert_eq!(request.standard_max_upgrade, Some(25));
        assert_eq!(request.somber_max_upgrade, Some(10));
        assert!(request.exact_upgrade_enabled());
        assert_eq!(request.scadutree_level, 20);
    }

    fn solved_build() -> SolvedBuildDto {
        SolvedBuildDto {
            weapon_id: 100,
            weapon_name: "Uchigatana".to_string(),
            weapon_type_name: "Katana".to_string(),
            affinity: "Blood".to_string(),
            is_somber: false,
            upgrade: 25,
            stats: combat_state(),
            requirements: CombatStateDto {
                str_stat: 11,
                dex: 15,
                int_stat: 0,
                fai: 0,
                arc: 0,
            },
            effective_scaling: ScalingDto {
                str: 0.2,
                dex: 0.8,
                int: 0.0,
                fai: 0.0,
                arc: 0.5,
            },
            ar: DamageBreakdownDto {
                physical: 500.0,
                magic: 0.0,
                fire: 0.0,
                lightning: 0.0,
                holy: 0.0,
                total: 500.0,
            },
            aow_id: Some(1),
            aow_name: Some("Seppuku".to_string()),
            bleed_buildup: 84.0,
            bleed_buildup_add: 30.0,
            frost_buildup: 0.0,
            poison_buildup: 0.0,
            scarlet_rot_buildup: 0.0,
            sleep_buildup: 0.0,
            madness_buildup: 0.0,
            death_buildup: 0.0,
            aow_first_hit_damage: 100.0,
            aow_full_sequence_damage: 250.0,
            aow_route: None,
            score: 500.0,
        }
    }

    fn combat_state() -> CombatStateDto {
        CombatStateDto {
            str_stat: 12,
            dex: 60,
            int_stat: 9,
            fai: 8,
            arc: 45,
        }
    }

    fn assert_has_path(value: &Value, path: &[&str]) {
        assert!(
            lookup_path(value, path).is_some(),
            "expected JSON path {} in {value}",
            path.join(".")
        );
    }

    fn lookup_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
        path.iter()
            .try_fold(value, |current, segment| match current {
                Value::Array(items) => segment
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| items.get(index)),
                Value::Object(map) => map.get(*segment),
                _ => None,
            })
    }
}
