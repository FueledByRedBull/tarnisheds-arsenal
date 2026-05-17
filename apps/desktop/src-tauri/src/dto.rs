use er_optimizer_core::model::COMBAT_STAT_COUNT;
use er_optimizer_core::{
    DamageBreakdown, OptimizeObjective, OptimizeRequest, OptimizeResult, ProgressSnapshot,
    SearchEstimate, SomberFilter, Stats,
};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeRequestDto {
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
    pub max_upgrade: u8,
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
    pub objective: String,
    pub top_k: usize,
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
pub struct SearchEstimateDto {
    pub weapon_candidates: usize,
    pub stat_candidates: u64,
    pub combinations: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolvedBuildDto {
    pub weapon_id: u32,
    pub weapon_name: String,
    pub affinity: String,
    pub is_somber: bool,
    pub upgrade: u8,
    pub stats: CombatStateDto,
    pub ar: DamageBreakdownDto,
    pub aow_id: Option<u16>,
    pub aow_name: Option<String>,
    pub bleed_buildup: f32,
    pub bleed_buildup_add: f32,
    pub frost_buildup: f32,
    pub poison_buildup: f32,
    pub scarlet_rot_buildup: f32,
    pub aow_first_hit_damage: f32,
    pub aow_full_sequence_damage: f32,
    pub score: f32,
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
    pub objective_ids: Vec<String>,
    pub somber_filters: Vec<String>,
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
    pub weapon_name: Option<String>,
    pub affinity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponProfileRequestDto {
    pub weapon_name: String,
    pub affinity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponProfileDto {
    pub requirements: CombatStateDto,
    pub max_upgrade: u8,
    pub is_somber: bool,
    pub disables_two_hand_bonus: bool,
    pub affinities: Vec<String>,
    pub compatible_aows: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponNamesForTypeRequestDto {
    pub weapon_type_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleAowsForAffinityRequestDto {
    pub affinity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponScalingRequestDto {
    pub weapon_name: String,
    pub affinity: String,
    pub upgrade: u8,
}

impl TryFrom<&OptimizeRequestDto> for OptimizeRequest {
    type Error = AppError;

    fn try_from(value: &OptimizeRequestDto) -> Result<Self, Self::Error> {
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
            max_upgrade: value.max_upgrade,
            fixed_upgrade: value.fixed_upgrade,
            two_handing: value.two_handing,
            dlc_scaling: value.dlc_scaling,
            scadutree_level: value.scadutree_level,
            weapon_name: value.weapon_name.clone(),
            affinity: value.affinity.clone(),
            aow_name: value.aow_name.clone(),
            weapon_type_key: value.weapon_type_key.clone(),
            somber_filter: parse_somber_filter(&value.somber_filter)?,
            objective: parse_objective(&value.objective)?,
            top_k: value.top_k,
        })
    }
}

impl From<SearchEstimate> for SearchEstimateDto {
    fn from(value: SearchEstimate) -> Self {
        Self {
            weapon_candidates: value.weapon_candidates,
            stat_candidates: value.stat_candidates,
            combinations: value.combinations,
        }
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

impl From<OptimizeResult> for SolvedBuildDto {
    fn from(value: OptimizeResult) -> Self {
        Self {
            weapon_id: value.weapon_id,
            weapon_name: value.weapon_name,
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
            ar: value.ar.into(),
            aow_id: value.aow_id,
            aow_name: value.aow_name,
            bleed_buildup: value.bleed_buildup,
            bleed_buildup_add: value.bleed_buildup_add,
            frost_buildup: value.frost_buildup,
            poison_buildup: value.poison_buildup,
            scarlet_rot_buildup: value.scarlet_rot_buildup,
            aow_first_hit_damage: value.aow_first_hit_damage,
            aow_full_sequence_damage: value.aow_full_sequence_damage,
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
    match raw.trim().to_ascii_lowercase().as_str() {
        "max_ar" => Ok(OptimizeObjective::MaxAr),
        "max_physical_ar" | "max_phys_ar" | "max_phy_ar" => Ok(OptimizeObjective::MaxPhysicalAr),
        "max_ar_plus_bleed" | "max_ar+bleed" | "max_ar_plus_bleed_buildup" => {
            Ok(OptimizeObjective::MaxArPlusBleed)
        }
        "aow_first_hit" | "max_aow_first_hit" => Ok(OptimizeObjective::AowFirstHit),
        "aow_full_sequence" | "max_aow_full_sequence" | "aow_full" => {
            Ok(OptimizeObjective::AowFullSequence)
        }
        _ => Err(AppError::new(format!(
            "invalid objective '{raw}', expected max_ar, max_physical_ar, max_ar_plus_bleed, aow_first_hit, or aow_full_sequence"
        ))),
    }
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

pub fn metric_for_objective(solved: &SolvedBuildDto, objective: &str) -> f32 {
    match objective {
        "max_physical_ar" => solved.ar.physical,
        "aow_first_hit" => solved.aow_first_hit_damage,
        "aow_full_sequence" => solved.aow_full_sequence_damage,
        "max_ar_plus_bleed" => solved.score,
        _ => solved.ar.total,
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
