//! Epidemiology tools (toolset `epi`).

pub mod deidentify;
pub mod profile;
pub mod rates;
pub mod suppression;

pub use deidentify::{DeidentifyArgs, DeidentifyResult, deidentify_dataset};
pub use profile::{ProfileArgs, ProfileResult, profile_local_dataset};
pub use rates::{CalculateRatesArgs, RatesResult, calculate_rates};
pub use suppression::{SuppressionArgs, SuppressionResult, apply_small_cell_suppression};

// Phase 4 modules (EPI-12) go below this line.
pub mod choropleth;
pub use choropleth::{ChoroplethResult, CreateChoroplethArgs, create_choropleth_map};
