//! Environmental health and climate tools (toolset `env`).
//!
//! Phase 12: ENV-01, ENV-02. Phase 14: ENV-03 to ENV-08.

// Phase 12 modules go below this line.
pub mod air_quality;
pub mod heat;

pub use air_quality::{AirQualityArgs, AirQualityResult, air_quality_protective_guidance};
pub use heat::{HeatPlanArgs, HeatPlanResult, heat_health_action_planner};
// End of Phase 12 modules.
// Phase 14 modules (ENV-03 to ENV-08) go below this line.
// Phase 14 Water (ENV-03, CHR-07).
// End of Phase 14 water.
// Phase 14 EJ and climate (ENV-04, ENV-05).
pub mod climate_profile;
pub mod ej_screen;

pub use climate_profile::{
    ClimateProfileArgs, ClimateProfileResult, climate_health_vulnerability_profile,
};
pub use ej_screen::{EjScreenArgs, EjScreenResult, environmental_justice_screen};
// End of Phase 14 ejclimate.
// Phase 14 Vectors and housing (ENV-06, ENV-07).
// End of Phase 14 vectorhousing.
// Phase 14 Chemicals and food (ENV-08, CHR-06).
pub mod chemical;

pub use chemical::{ChemicalArgs, ChemicalResult, chemical_exposure_reference};
// End of Phase 14 chemfood.
// End of Phase 14 modules (ENV-03 to ENV-08).
