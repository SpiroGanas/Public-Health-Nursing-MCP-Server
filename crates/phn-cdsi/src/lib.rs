//! CDC Clinical Decision Support for Immunization (CDSi) engine.
//!
//! Implements the CDSi *Logic Specification for ACIP Recommendations*
//! (version 4.6, December 2024) over CDC's *Supporting Data* (version 4.65,
//! August 2026), which is embedded in the crate (`data/`, with a manifest of
//! source URLs and SHA-256 checksums).
//!
//! * [`data`]: typed supporting data parsed with `quick-xml`, loaded lazily.
//! * [`duration`]: CDSi ages/intervals and date arithmetic (CALCDT rules).
//! * [`engine`]: evaluation, forecasting, best-series selection, and
//!   vaccine group forecasts.
//! * [`model`]: inputs and results.
//! * [`names`]: CVX codes, vaccine types, and trade names to CVX.
//!
//! The engine encodes CDC/ACIP recommendations only. Results are decision
//! support: verify against the immunization registry and current guidance.

pub mod data;
pub mod duration;
pub mod engine;
pub mod model;
pub mod names;

pub use data::SupportingData;
pub use engine::run;
pub use model::*;
