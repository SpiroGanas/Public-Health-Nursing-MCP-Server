//! Mental health and substance use tools (toolset `bh`).
//!
//! Phase 10: MH-01, MH-02. Phase 11: MH-03 to MH-07.

// Phase 10 modules go below this line.
// End of Phase 10 modules.

// Phase 11 modules go below this line.
pub mod overdose;
pub use overdose::{OverdoseArgs, OverdoseResult, get_overdose_data};
pub mod harm_reduction;
pub use harm_reduction::{
    HarmReductionArgs, HarmReductionResult, find_naloxone_and_harm_reduction,
};
pub mod postvention;
pub use postvention::{PostventionArgs, PostventionResult, plan_school_postvention};
pub mod treatment;
pub use treatment::{TreatmentArgs, TreatmentResult, find_substance_use_treatment};
pub mod youth_survey;
pub use youth_survey::{YouthSurveyArgs, YouthSurveyResult, get_youth_risk_behavior_data};
// End of Phase 11 modules.
