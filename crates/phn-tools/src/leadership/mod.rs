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
// End of Phase 18 strategy.

// Phase 18 workforce (LEAD-06, LEAD-07, LEAD-08): add `pub mod` and `pub use` lines below.
// End of Phase 18 workforce.
