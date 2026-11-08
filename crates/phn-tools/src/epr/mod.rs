//! Emergency preparedness tools (toolset `epr`).
//!
//! Phase 12: EPR-02 to EPR-06, EPR-11.

// Phase 12 modules go below this line.
pub mod alerts;
pub mod area;
pub mod content;

pub use alerts::{HazardAlertsArgs, HazardAlertsResult, get_active_hazard_alerts};
// End of Phase 12 modules.
