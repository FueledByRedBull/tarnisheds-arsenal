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
    #[serde(default)]
    pub max_upgrade: Option<u8>,
    #[serde(default)]
    pub fixed_upgrade: Option<u8>,
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
    pub fn standard_upgrade_cap(&self) -> u8 {
        self.standard_max_upgrade
            .or(self.max_upgrade)
            .unwrap_or(25)
            .min(25)
    }

    pub fn somber_upgrade_cap(&self) -> u8 {
        self.somber_max_upgrade
            .or_else(|| self.max_upgrade.map(|value| value.min(10)))
            .unwrap_or(10)
            .min(25)
    }

    pub fn exact_upgrade_enabled(&self) -> bool {
        self.exact_upgrade.unwrap_or(self.fixed_upgrade.is_some())
    }

    pub fn set_exact_upgrade(&mut self, upgrade: u8, is_somber: bool) {
        if is_somber {
            self.somber_max_upgrade = Some(upgrade.min(25));
        } else {
            self.standard_max_upgrade = Some(upgrade.min(25));
        }
        self.exact_upgrade = Some(true);
        self.max_upgrade = None;
        self.fixed_upgrade = None;
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
pub struct UpgradeSeriesRequestDto {
    pub base: OptimizeRequestDto,
    pub solved: SolvedBuildDto,
    pub max_upgrade: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStepDto {
    pub level: u16,
    pub stats: CombatStateDto,
    pub metric: Option<f32>,
    pub score: Option<f32>,
    pub added_stat: Option<String>,
    pub requirement_gap: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathProgressDto {
    pub job_id: String,
    pub checked: u64,
    pub total: u64,
    pub title: String,
    pub level: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathFinishedDto {
    pub job_id: String,
    pub cancelled: bool,
    pub paths: Vec<PathPreviewDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchPointDto {
    pub level: u16,
    pub metric: Option<f32>,
    pub solved: Option<SolvedBuildDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchLineDto {
    pub affinity: String,
    pub points: Vec<AffinityWatchPointDto>,
    pub start_metric: Option<f32>,
    pub end_metric: Option<f32>,
    pub final_build: Option<SolvedBuildDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchPayloadDto {
    pub lines: Vec<AffinityWatchLineDto>,
    pub breakpoints: Vec<AffinityBreakpointDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchProgressDto {
    pub job_id: String,
    pub checked: u64,
    pub total: u64,
    pub affinity: String,
    pub level: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchFinishedDto {
    pub job_id: String,
    pub cancelled: bool,
    pub payload: Option<AffinityWatchPayloadDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityWatchJobStatusDto {
    pub progress: Option<AffinityWatchProgressDto>,
    pub finished: Option<AffinityWatchFinishedDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSearchResponseDto {
    pub job_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgressDto {
    pub job_id: String,
    pub checked: u64,
    pub total: u64,
    pub eligible: u64,
    pub best_score: f32,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFinishedDto {
    pub job_id: String,
    pub cancelled: bool,
    pub rows: Vec<SolvedBuildDto>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchJobStatusDto {
    pub progress: Option<SearchProgressDto>,
    pub finished: Option<SearchFinishedDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDto {
    pub weapon_count: usize,
    pub aow_count: usize,
    pub weapon_names: Vec<String>,
    pub weapon_type_keys: Vec<String>,
    pub classes: Vec<ClassMetadataDto>,
    pub weapon_type_options: Vec<WeaponTypeOptionDto>,
    pub aow_names: Vec<String>,
    pub affinity_names: Vec<String>,
    pub objective_ids: Vec<String>,
    pub somber_filters: Vec<String>,
    pub filter_dimensions: Vec<FilterDimensionDto>,
    pub data_manifest: DataManifestDto,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterDimensionDto {
    pub id: String,
    pub label: String,
    pub options: Vec<FilterOptionDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptionDto {
    pub id: String,
    pub label: String,
    pub count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataManifestDto {
    pub schema_version: u32,
    pub dataset_version: String,
    pub model_version: String,
    pub id: String,
    pub label: String,
    pub app_version: String,
    pub source: String,
    pub generated_at: String,
    pub extractor_version: String,
    pub provenance: String,
    pub profile: ProfileMetadataDto,
    pub capabilities: ProfileCapabilitiesDto,
    pub rules: ProfileRulesDto,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMetadataDto {
    pub id: String,
    pub display_name: String,
    pub game_version: String,
    pub mod_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCapabilitiesDto {
    pub class_budget: bool,
    pub weapon_ar: bool,
    pub weapon_ar_for_ammunition: bool,
    pub status_buildup: bool,
    pub weapon_passives: bool,
    pub aow_compatibility: bool,
    pub aow_damage: bool,
    pub aow_routes: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRulesDto {
    pub standard_max_upgrade: u8,
    pub somber_max_upgrade: u8,
    pub separate_upgrade_caps: bool,
    pub scadutree_scaling: bool,
    pub zero_attack_element_uses_weapon_scaling: bool,
    pub extended_scaling_grades: bool,
    pub status_buildup_scales: bool,
}

impl From<er_optimizer_core::SnapshotManifest> for DataManifestDto {
    fn from(value: er_optimizer_core::SnapshotManifest) -> Self {
        Self {
            schema_version: value.schema_version,
            dataset_version: value.dataset_version,
            model_version: value.model_version,
            id: value.id,
            label: value.label,
            app_version: value.app_version,
            source: value.source,
            generated_at: value.generated_at,
            extractor_version: value.extractor_version,
            provenance: value.provenance,
            profile: ProfileMetadataDto {
                id: value.profile.id,
                display_name: value.profile.display_name,
                game_version: value.profile.game_version,
                mod_version: value.profile.mod_version,
            },
            capabilities: ProfileCapabilitiesDto {
                class_budget: value.capabilities.class_budget,
                weapon_ar: value.capabilities.weapon_ar,
                weapon_ar_for_ammunition: value.capabilities.weapon_ar_for_ammunition,
                status_buildup: value.capabilities.status_buildup,
                weapon_passives: value.capabilities.weapon_passives,
                aow_compatibility: value.capabilities.aow_compatibility,
                aow_damage: value.capabilities.aow_damage,
                aow_routes: value.capabilities.aow_routes,
            },
            rules: ProfileRulesDto {
                standard_max_upgrade: value.rules.standard_max_upgrade,
                somber_max_upgrade: value.rules.somber_max_upgrade,
                separate_upgrade_caps: value.rules.separate_upgrade_caps,
                scadutree_scaling: value.rules.scadutree_scaling,
                zero_attack_element_uses_weapon_scaling: value
                    .rules
                    .zero_attack_element_uses_weapon_scaling,
                extended_scaling_grades: value.rules.extended_scaling_grades,
                status_buildup_scales: value.rules.status_buildup_scales,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMetadataDto {
    pub name: String,
    pub base_level: u16,
    pub base_total: u16,
    pub base_stats: EightStatsDto,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponTypeOptionDto {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleAowsRequestDto {
    pub profile_id: String,
    pub weapon_name: Option<String>,
    pub affinity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponProfileRequestDto {
    pub profile_id: String,
    pub weapon_name: String,
    pub affinity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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
pub struct WeaponNamesForTypeRequestDto {
    pub profile_id: String,
    pub weapon_type_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleAowsForAffinityRequestDto {
    pub profile_id: String,
    pub affinity: Option<String>,
}

impl TryFrom<&OptimizeRequestDto> for OptimizeRequest {
    type Error = AppError;

    fn try_from(value: &OptimizeRequestDto) -> Result<Self, Self::Error> {
        validate_optimize_request(value)?;
        Ok(Self {
            class_name: value.class_name.clone(),
            character_level: value.character_level,
            current_stats: Stats {
                vig: value.vig,
                mnd: value.mnd,
                end: value.end,
                str: value.str_stat,
                dex: value.dex,
                int: value.int_stat,
                fai: value.fai,
                arc: value.arc,
            },
            min_combat_stats: [
                value.min_str,
                value.min_dex,
                value.min_int,
                value.min_fai,
                value.min_arc,
            ],
            locked_combat_stats: [
                value.lock_str,
                value.lock_dex,
                value.lock_int,
                value.lock_fai,
                value.lock_arc,
            ],
            standard_max_upgrade: value.standard_upgrade_cap(),
            somber_max_upgrade: value.somber_upgrade_cap(),
            exact_upgrade: value.exact_upgrade_enabled(),
            two_handing: value.two_handing,
            dlc_scaling: value.dlc_scaling,
            scadutree_level: value.scadutree_level,
            weapon_name: value.weapon_name.clone(),
            affinity: value.affinity.clone(),
            aow_name: value.aow_name.clone(),
            weapon_type_key: value.weapon_type_key.clone(),
            somber_filter: parse_somber_filter(&value.somber_filter)?,
            filters: parse_stable_filters(&value.filters)?,
            result_grouping: ResultGrouping::parse(&value.result_grouping)
                .map_err(AppError::new)?,
            objective: parse_objective(&value.objective)?,
            top_k: value.top_k,
        })
    }
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

impl From<ProgressSnapshot> for SearchProgressDto {
    fn from(value: ProgressSnapshot) -> Self {
        Self {
            job_id: String::new(),
            checked: value.checked,
            total: value.total,
            eligible: value.eligible,
            best_score: value.best_score,
            elapsed_ms: value.elapsed_ms,
        }
    }
}

pub fn parse_objective(raw: &str) -> Result<OptimizeObjective, AppError> {
    OptimizeObjective::parse(raw).map_err(AppError::new)
}

pub fn validate_optimize_request(request: &OptimizeRequestDto) -> Result<(), AppError> {
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
    if request.filters.entries.len() > 512 {
        return Err(AppError::new(
            "filters.entries must contain at most 512 entries",
        ));
    }
    Ok(())
}

fn parse_stable_filters(filters: &StableFilterSetDto) -> Result<Vec<StableFilter>, AppError> {
    filters
        .entries
        .iter()
        .map(|filter| {
            if filter.id.is_empty() || filter.id.len() > 128 {
                return Err(AppError::new(
                    "filter ids must contain 1 through 128 characters",
                ));
            }
            Ok(StableFilter {
                dimension: FilterDimension::parse(&filter.dimension).map_err(AppError::new)?,
                id: filter.id.clone(),
                mode: FilterMode::parse(&filter.mode).map_err(AppError::new)?,
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

pub fn validate_path_batch(count: usize) -> Result<(), AppError> {
    if count > MAX_PATH_BATCH {
        return Err(AppError::new(format!(
            "path batch requests must contain at most {MAX_PATH_BATCH} entries; got {count}"
        )));
    }
    Ok(())
}

pub fn parse_somber_filter(raw: &str) -> Result<SomberFilter, AppError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(SomberFilter::All),
        "standard_only" | "standard" => Ok(SomberFilter::StandardOnly),
        "somber_only" | "somber" => Ok(SomberFilter::SomberOnly),
        _ => Err(AppError::new(format!(
            "invalid somber_filter '{raw}', expected all, standard_only, or somber_only"
        ))),
    }
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

pub fn lock_request_to_stats(request: &mut OptimizeRequestDto, stats: CombatStateDto) {
    request.lock_str = Some(stats.str_stat);
    request.lock_dex = Some(stats.dex);
    request.lock_int = Some(stats.int_stat);
    request.lock_fai = Some(stats.fai);
    request.lock_arc = Some(stats.arc);
}

pub fn set_min_combat_stats(request: &mut OptimizeRequestDto, mins: [u8; COMBAT_STAT_COUNT]) {
    request.min_str = mins[0];
    request.min_dex = mins[1];
    request.min_int = mins[2];
    request.min_fai = mins[3];
    request.min_arc = mins[4];
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
                        score: Some(10.0),
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
                            solved: Some(solved_build()),
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
        assert_eq!(request.standard_upgrade_cap(), 25);
        assert_eq!(request.somber_upgrade_cap(), 10);
        assert!(request.exact_upgrade_enabled());
        assert_eq!(request.scadutree_level, 20);
    }

    #[test]
    fn legacy_optimize_request_upgrade_keys_migrate() {
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
            "maxUpgrade": 25,
            "fixedUpgrade": 25,
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
        .expect("legacy request deserializes");

        assert_eq!(request.standard_upgrade_cap(), 25);
        assert_eq!(request.somber_upgrade_cap(), 10);
        assert!(request.exact_upgrade_enabled());
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
