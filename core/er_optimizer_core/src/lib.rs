pub mod data;
pub mod math;
pub mod model;
pub mod optimizer;
mod snapshot;

/// Include the compiled scoring contract in persisted result/cache identities.
pub fn runtime_model_version(snapshot_model: &str) -> String {
    format!("{snapshot_model}/exact-v3")
}

pub use data::{
    CONVERGENCE_PROFILE_ID, VANILLA_PROFILE_ID, load_embedded_game_profile,
    load_embedded_game_profile_with_manifest, load_game_data, load_game_data_with_manifest,
};
pub use math::{
    CUSTOM_STATS_CLASS_NAME, StartingClass, calculate_ar, class_by_name, compute_free_points,
    custom_stats_class, effective_str, meets_requirements,
};
pub use model::{
    Aow, AttackElementCorrect, DamageBreakdown, DamageType, DataCapabilities, DisplayPoiseDamage,
    GameData, ReinforceLevel, STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, Weapon,
    normalize_weapon_type_display,
};
pub use optimizer::{
    ArBleedFrontierPoint, CANCELLATION_LATENCY_TARGET_MS, FilterDimension, FilterMode,
    LevelOptimizeResult, OptimizeObjective, OptimizePhaseTimings, OptimizeRequest, OptimizeResult,
    PreparedLoadoutEvaluator, PreparedSearchPlan, PreparedUpgradeSeriesEvaluator,
    ProfiledOptimizeResult, ProgressSnapshot, ResultGrouping, SearchEstimate, SomberFilter,
    StableFilter, optimize, optimize_level_range_with_progress, optimize_prepared_with_progress,
    optimize_profiled, optimize_with_cancel, optimize_with_progress,
    prepare_loadout_evaluator_with_cancel, prepare_search, prepare_search_with_cancel,
    prepare_upgrade_series_evaluator_with_cancel,
};
pub use snapshot::{
    SnapshotCapabilities, SnapshotFile, SnapshotManifest, SnapshotProfile, SnapshotSource,
};

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_identity_names_the_current_scoring_contract() {
        // Persisted results and caches are keyed by this string; change it only with the model.
        assert_eq!(
            super::runtime_model_version("aow-routes-effects-v8"),
            "aow-routes-effects-v8/exact-v3"
        );
    }
}
