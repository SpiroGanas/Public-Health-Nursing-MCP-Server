//! Phase 19 tools (Legislation and federal rules): POL-01, POL-03.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 19 tracking: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase19_tracking_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
