pub mod data;
pub mod math;
pub mod model;
pub mod optimizer;
#[cfg(feature = "python")]
pub mod python_api;
mod snapshot;

pub use data::{
    CONVERGENCE_PROFILE_ID, VANILLA_PROFILE_ID, load_embedded_game_data,
    load_embedded_game_data_with_manifest, load_embedded_game_profile,
    load_embedded_game_profile_with_manifest, load_game_data, load_game_data_with_manifest,
};
pub use math::{
    ScalingContribution, StartingClass, StatIter, build_contributions, calculate_ar,
    calculate_ar_for_type, class_by_name, compute_free_points, effective_str, meets_requirements,
};
pub use model::{
    Aow, AttackElementCorrect, DamageBreakdown, DamageType, DataCapabilities, GameData,
    ReinforceLevel, STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, Weapon,
};
pub use optimizer::{
    CANCELLATION_LATENCY_TARGET_MS, LevelOptimizeResult, OptimizeObjective, OptimizePhaseTimings,
    OptimizeRequest, OptimizeResult, PreparedLoadoutEvaluator, PreparedSearchPlan,
    PreparedUpgradeSeriesEvaluator, ProfiledOptimizeResult, ProgressSnapshot, SearchEstimate,
    SomberFilter, estimate_search_space, optimize, optimize_level_range_with_progress,
    optimize_prepared_with_progress, optimize_profiled, optimize_with_cancel,
    optimize_with_progress, prepare_loadout_evaluator_with_cancel, prepare_search,
    prepare_search_with_cancel, prepare_upgrade_series_evaluator_with_cancel,
};
pub use snapshot::{
    SnapshotCapabilities, SnapshotFile, SnapshotManifest, SnapshotProfile, SnapshotSource,
};
