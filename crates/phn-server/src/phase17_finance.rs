//! Phase 17 tools (Department budget and staffing): LEAD-01, LEAD-02.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 17 finance: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase17_finance_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
