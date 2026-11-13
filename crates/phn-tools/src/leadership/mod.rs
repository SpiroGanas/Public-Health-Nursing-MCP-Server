//! Leadership, finance, and administration tools (toolset `leadership`).

// Phase 17 finance (LEAD-01, LEAD-02): add `pub mod` and `pub use` lines below.
pub mod budget;
mod budget_output;
pub mod finance_content;
pub mod staffing;
mod staffing_output;

pub use budget::{BudgetArgs, BudgetResult, analyze_health_department_budget};
pub use staffing::{StaffingArgs, StaffingResult, model_staffing_needs};
// End of Phase 17 finance.

// Phase 18 strategy (LEAD-03, LEAD-04, LEAD-05): add `pub mod` and `pub use` lines below.
pub mod fphs;
pub mod phab;
pub mod strategic_plan;
pub mod strategy_docs;
pub use fphs::{
    FPHS_FRAMEWORK, FphsArgs, FphsResult, StatusEntry, StdSet,
    assess_foundational_public_health_services,
};
pub use phab::{PHAB_STANDARDS, PhabArgs, PhabResult, phab_accreditation_crosswalk};
pub use strategic_plan::{
    STRATEGIC_PLANNING, StrategicPlanArgs, StrategicPlanResult, build_strategic_plan,
};
pub use strategy_docs::OutFormat;
// End of Phase 18 strategy.

// Phase 18 workforce (LEAD-06, LEAD-07, LEAD-08): add `pub mod` and `pub use` lines below.
pub mod board_meeting;
pub mod oml;
pub mod standing_orders;
pub mod workforce;
pub mod workforce_content;
mod workforce_output;
pub use board_meeting::{BoardMeetingArgs, BoardMeetingResult, board_of_health_meeting_assistant};
pub use workforce::{WorkforceArgs, WorkforceResult, assess_workforce_competencies};
pub use standing_orders::{StandingOrderArgs, StandingOrderResult, draft_standing_orders};
// End of Phase 18 workforce.
