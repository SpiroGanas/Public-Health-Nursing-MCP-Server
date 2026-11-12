//! Phase 18 tools (FPHS, PHAB, strategic planning): LEAD-03, LEAD-04, LEAD-05.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 18 strategy: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase18_strategy_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
