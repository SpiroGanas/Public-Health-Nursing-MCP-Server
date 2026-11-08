//! Healthy aging and older adult tools (toolset `aging`).
//!
//! Phase 10: AGE-01 to AGE-07.

// Phase 10 modules go below this line.
pub mod content;
pub mod directory;
pub mod falls;
pub mod localfill;
pub mod protective;
pub use falls::{FallRiskArgs, FallRiskResult, steadi_fall_risk_assessment};
pub use protective::{ProtectiveArgs, ProtectiveResult, elder_protective_services_guide};
// End of Phase 10 modules.
