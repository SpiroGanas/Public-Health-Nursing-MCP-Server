//! Immunization tools (toolset `vax`).

// Phase 5 modules (VAX-01, VAX-02, VAX-04 to VAX-09) go below this line.

// Phase 6 modules (VAX-03, the CDSi forecaster) go below this line.
pub mod forecast;
pub use forecast::{ForecastImmunizationsArgs, ForecastImmunizationsResult, forecast_immunizations_due};
