use er_optimizer_core::model::COMBAT_STAT_COUNT;
use er_optimizer_core::{
    DamageBreakdown, OptimizeObjective, OptimizeRequest, OptimizeResult, ProgressSnapshot,
    SearchEstimate, SomberFilter, Stats,
};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

pub const MAX_TOP_K: usize = 500;
pub const MAX_LEVELS_AHEAD: u16 = 200;
pub const MAX_PATH_BATCH: usize = 2;

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
    pub data_manifest: DataManifestDto,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataManifestDto {
    pub id: String,
    pub label: String,
    pub app_version: String,
    pub source: String,
    pub generated_at: String,
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
    OptimizeObjective::parse(raw).map_err(AppError::new)
}

pub fn validate_optimize_request(request: &OptimizeRequestDto) -> Result<(), AppError> {
    if request.top_k > MAX_TOP_K {
        return Err(AppError::new(format!(
            "topK must be {MAX_TOP_K} or lower; got {}",
            request.top_k
        )));
    }
    Ok(())
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
        OptimizeObjective::MaxArPlusBleed => solved.bleed_buildup,
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
    fn representative_catalog_uses_app_contract_keys() {
        let value = serde_json::to_value(CatalogDto {
            weapon_count: 1,
            aow_count: 1,
            weapon_names: vec!["Uchigatana".to_string()],
            weapon_type_keys: vec!["katana".to_string()],
            classes: vec![ClassMetadataDto {
                name: "Samurai".to_string(),
                base_level: 9,
                base_total: 80,
                base_stats: EightStatsDto {
                    vig: 12,
                    mnd: 11,
                    end: 13,
                    str_stat: 12,
                    dex: 15,
                    int_stat: 9,
                    fai: 8,
                    arc: 8,
                },
            }],
            weapon_type_options: vec![WeaponTypeOptionDto {
                key: "katana".to_string(),
                label: "Katana".to_string(),
            }],
            aow_names: vec!["Seppuku".to_string()],
            objective_ids: vec!["max_ar".to_string()],
            somber_filters: vec!["all".to_string()],
            data_manifest: DataManifestDto {
                id: "phase1".to_string(),
                label: "Phase 1".to_string(),
                app_version: "0.4.9".to_string(),
                source: "test".to_string(),
                generated_at: "2026-06-10T00:00:00Z".to_string(),
            },
        })
        .expect("catalog serializes");

        assert_has_path(&value, &["weaponCount"]);
        assert_has_path(&value, &["weaponTypeOptions", "0", "key"]);
        assert_has_path(&value, &["classes", "0", "baseStats", "strStat"]);
        assert_has_path(&value, &["dataManifest", "appVersion"]);
        assert_missing_path(&value, &["weapon_count"]);
        assert_missing_path(&value, &["classes", "0", "base_stats"]);
        assert_missing_path(&value, &["data_manifest"]);
    }

    #[test]
    fn representative_job_status_uses_app_contract_keys() {
        let value = serde_json::to_value(SearchJobStatusDto {
            progress: Some(SearchProgressDto {
                job_id: "search-1".to_string(),
                checked: 10,
                total: 100,
                eligible: 3,
                best_score: 42.5,
                elapsed_ms: 250,
            }),
            finished: Some(SearchFinishedDto {
                job_id: "search-1".to_string(),
                cancelled: false,
                rows: vec![solved_build()],
                error: None,
            }),
        })
        .expect("status serializes");

        assert_has_path(&value, &["progress", "jobId"]);
        assert_has_path(&value, &["progress", "bestScore"]);
        assert_has_path(&value, &["progress", "elapsedMs"]);
        assert_has_path(&value, &["finished", "rows", "0", "weaponName"]);
        assert_has_path(&value, &["finished", "rows", "0", "stats", "strStat"]);
        assert_has_path(&value, &["finished", "rows", "0", "aowFirstHitDamage"]);
        assert_missing_path(&value, &["progress", "job_id"]);
        assert_missing_path(&value, &["finished", "rows", "0", "weapon_name"]);
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
        .expect("request deserializes");

        assert_eq!(request.class_name, "Samurai");
        assert_eq!(request.str_stat, 12);
        assert_eq!(request.int_stat, 9);
        assert_eq!(request.fixed_upgrade, Some(25));
        assert_eq!(request.scadutree_level, 20);
    }

    fn solved_build() -> SolvedBuildDto {
        SolvedBuildDto {
            weapon_id: 100,
            weapon_name: "Uchigatana".to_string(),
            affinity: "Blood".to_string(),
            is_somber: false,
            upgrade: 25,
            stats: combat_state(),
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
            aow_first_hit_damage: 100.0,
            aow_full_sequence_damage: 250.0,
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

    fn assert_missing_path(value: &Value, path: &[&str]) {
        assert!(
            lookup_path(value, path).is_none(),
            "unexpected JSON path {} in {value}",
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
