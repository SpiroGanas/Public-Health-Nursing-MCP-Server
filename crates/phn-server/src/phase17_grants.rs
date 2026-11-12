//! Phase 17 tools (Grant search, NOFO analysis, narratives): PLAN-06, PLAN-07, PLAN-08.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 17 grants: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase17_grants_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
