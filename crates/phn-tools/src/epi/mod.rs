//! Epidemiology tools (toolset `epi`).

pub mod rates;

pub use rates::{CalculateRatesArgs, RatesResult, calculate_rates};
