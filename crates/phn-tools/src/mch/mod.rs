//! Maternal, child, and family health tools (toolset `mch`).
//!
//! Phase 8: MCH-01 to MCH-07.

// Phase 8 modules go below this line.
pub(crate) mod content;
pub mod growth;
pub mod lead;
pub(crate) mod lead_data;
pub mod lead_risk;
pub mod mandated_reporter;
pub mod maternal_infant;
pub mod milestones;
pub mod newborn;

pub use growth::{GrowthArgs, GrowthResult, calculate_growth_percentiles};
pub use lead::{LeadGuideArgs, LeadGuideResult, childhood_lead_screening_guide};
pub use lead_risk::{LeadRiskArgs, LeadRiskResult, map_lead_risk};
pub use mandated_reporter::{
    MandatedReporterArgs, MandatedReporterResult, mandated_reporter_51a_guide,
};
pub use maternal_infant::{
    MaternalInfantArgs, MaternalInfantResult, maternal_infant_health_profile,
};
pub use milestones::{MilestoneArgs, MilestoneResult, check_developmental_milestones};
pub use newborn::{NewbornVisitArgs, NewbornVisitResult, plan_newborn_postpartum_visit};
// End of Phase 8 modules.
