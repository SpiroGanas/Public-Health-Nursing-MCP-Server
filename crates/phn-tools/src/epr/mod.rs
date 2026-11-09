//! Emergency preparedness tools (toolset `epr`).
//!
//! Phase 12: EPR-02 to EPR-06, EPR-11.

// Phase 12 modules go below this line.
pub mod alerts;
pub mod area;
pub mod casper;
pub mod content;
pub mod dispensing;
pub mod hva;
pub mod population;
pub mod power;
pub mod shelter;

pub use alerts::{HazardAlertsArgs, HazardAlertsResult, get_active_hazard_alerts};
pub use casper::{CasperArgs, CasperResult, TrackingTotals, plan_casper_assessment};
pub use dispensing::{
    DispensingArgs, DispensingResult, StationInput, model_dispensing_site_throughput,
};
pub use hva::{HazardScoreInput, HvaArgs, HvaResult, hazard_vulnerability_assessment};
pub use power::{ElectricityArgs, ElectricityResult, get_electricity_dependent_population};
pub use shelter::{ShelterArgs, ShelterResult, functional_needs_shelter_intake};
// End of Phase 12 modules.
