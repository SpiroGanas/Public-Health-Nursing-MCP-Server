//! Emergency preparedness tools (toolset `epr`).
//!
//! Phase 12: EPR-02 to EPR-06, EPR-11. Phase 13: EPR-01, EPR-07 to EPR-10.

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

// Phase 13 shared modules.
pub mod roster;

// Phase 13 EPR-01 (ICS) modules go below this line.
// End of Phase 13 EPR-01 modules.

// Phase 13 EPR-07 (COOP) modules go below this line.
pub mod coop;
pub use coop::{
    CommInput, CoopArgs, CoopResult, DelegationInput, FunctionInput, RecordInput, SiteInput,
    SuccessionInput, continuity_of_operations_planner,
};
// End of Phase 13 EPR-07 modules.

// Phase 13 EPR-08/EPR-09 (HSEEP) modules go below this line.
// End of Phase 13 EPR-08/EPR-09 modules.

// Phase 13 EPR-10 (CERC) modules go below this line.
// End of Phase 13 EPR-10 modules.
