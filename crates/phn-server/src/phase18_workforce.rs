//! Phase 18 tools (Workforce, board of health, standing orders): LEAD-06, LEAD-07, LEAD-08.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 18 workforce: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase18_workforce_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
