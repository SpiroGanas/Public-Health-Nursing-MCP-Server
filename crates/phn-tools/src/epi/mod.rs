//! Epidemiology tools (toolset `epi`).

pub mod age_adjust;
pub mod chart;
pub mod deidentify;
pub mod profile;
pub mod rates;
pub mod sample_size;
pub mod standardized_ratio;
pub mod stat_test;
pub mod suppression;
pub mod trends;
pub mod two_by_two;

pub use deidentify::{DeidentifyArgs, DeidentifyResult, deidentify_dataset};
pub use profile::{ProfileArgs, ProfileResult, profile_local_dataset};
pub use rates::{CalculateRatesArgs, RatesResult, calculate_rates};
pub use suppression::{SuppressionArgs, SuppressionResult, apply_small_cell_suppression};
// Phase 2
pub use age_adjust::{AgeAdjustArgs, AgeAdjustResult, age_adjust_rates};
pub use chart::{CreateChartArgs, CreateChartResult, create_chart};
pub use sample_size::{SampleSizeArgs, SampleSizeResult, calculate_sample_size};
pub use standardized_ratio::{
    StandardizedRatioArgs, StandardizedRatioResult, compute_standardized_ratio,
};
pub use stat_test::{StatTestArgs, StatTestResult, run_statistical_test};
pub use trends::{TrendArgs, TrendToolResult, analyze_trends};
pub use two_by_two::{TwoByTwoArgs, TwoByTwoResult, analyze_two_by_two};

// Phase 4 modules (EPI-12) go below this line.
pub mod choropleth;
pub use choropleth::{ChoroplethResult, CreateChoroplethArgs, create_choropleth_map};

// Phase 15 geo (EPI-13): add `pub mod` and `pub use` lines below.
pub mod geocode;
pub use geocode::{GeocodeArgs, GeocodeResult, geocode_and_aggregate_addresses};
// End of Phase 15 geo.
