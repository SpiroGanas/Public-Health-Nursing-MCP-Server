//! Phase 17 tools (Budgets and economic evaluation): PLAN-09, PLAN-10.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 17 budget: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase17_budget_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
