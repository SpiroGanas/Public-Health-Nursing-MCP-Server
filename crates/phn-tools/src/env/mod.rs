//! Environmental health and climate tools (toolset `env`).
//!
//! Phase 12: ENV-01, ENV-02.

// Phase 12 modules go below this line.
pub mod air_quality;
pub mod heat;

pub use air_quality::{AirQualityArgs, AirQualityResult, air_quality_protective_guidance};
pub use heat::{HeatPlanArgs, HeatPlanResult, heat_health_action_planner};
// End of Phase 12 modules.
