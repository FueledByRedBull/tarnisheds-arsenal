pub mod data;
pub mod math;
pub mod model;
pub mod optimizer;
#[cfg(feature = "python")]
pub mod python_api;

pub use data::{load_embedded_game_data, load_game_data};
pub use math::{
    ScalingContribution, StartingClass, StatIter, build_contributions, calculate_ar,
    calculate_ar_for_type, class_by_name, compute_free_points, effective_str, meets_requirements,
};
pub use model::{
    Aow, AttackElementCorrect, DamageBreakdown, DamageType, GameData, ReinforceLevel, STAT_ARC,
    STAT_DEX, STAT_FAI, STAT_INT, STAT_STR, Stats, Weapon,
};
pub use optimizer::{
    OptimizeObjective, OptimizeRequest, OptimizeResult, PreparedSearchPlan, ProgressSnapshot,
    SearchEstimate, SomberFilter, estimate_search_space, optimize, optimize_prepared_with_progress,
    optimize_with_progress, prepare_search,
};
