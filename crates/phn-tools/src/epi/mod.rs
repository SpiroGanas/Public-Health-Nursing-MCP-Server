//! Epidemiology tools (toolset `epi`).

pub mod deidentify;
pub mod profile;
pub mod rates;
pub mod suppression;

pub use deidentify::{DeidentifyArgs, DeidentifyResult, deidentify_dataset};
pub use profile::{ProfileArgs, ProfileResult, profile_local_dataset};
pub use rates::{CalculateRatesArgs, RatesResult, calculate_rates};
pub use suppression::{SuppressionArgs, SuppressionResult, apply_small_cell_suppression};
