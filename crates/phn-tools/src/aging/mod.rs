//! Healthy aging and older adult tools (toolset `aging`).
//!
//! Phase 10: AGE-01 to AGE-07.

// Phase 10 modules go below this line.
pub mod acp;
pub mod content;
pub mod coverage;
pub mod directory;
pub mod falls;
pub mod localfill;
pub mod nursing_homes;
pub mod protective;
pub mod services;
pub mod vulnerability;
pub use acp::{AcpArgs, AcpResult, advance_care_planning_toolkit};
pub use coverage::{CoverageArgs, CoverageResult, explain_older_adult_coverage_options};
pub use falls::{FallRiskArgs, FallRiskResult, steadi_fall_risk_assessment};
pub use nursing_homes::{NursingHomeArgs, NursingHomeResult, nursing_home_quality_lookup};
pub use protective::{ProtectiveArgs, ProtectiveResult, elder_protective_services_guide};
pub use services::{AgingServicesArgs, AgingServicesResult, find_ma_aging_services};
pub use vulnerability::{VulnerabilityArgs, VulnerabilityResult, map_older_adult_vulnerability};
// End of Phase 10 modules.
