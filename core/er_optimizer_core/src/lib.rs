pub mod data;
pub mod math;
pub mod model;
pub mod optimizer;
#[cfg(feature = "python")]
pub mod python_api;

pub use data::load_game_data;
pub use math::{
    calculate_ar, calculate_ar_for_type, class_by_name, compute_free_points, effective_str,
    meets_requirements, build_contributions, ScalingContribution, StatIter, StartingClass,
};
pub use model::{
    AttackElementCorrect, Aow, DamageBreakdown, DamageType, GameData, ReinforceLevel, Stats, Weapon,
    STAT_ARC, STAT_DEX, STAT_FAI, STAT_INT, STAT_STR,
};
pub use optimizer::{
    estimate_search_space, optimize, optimize_with_progress, OptimizeObjective, OptimizeRequest,
    OptimizeResult, ProgressSnapshot, SearchEstimate, SomberFilter,
};
