//! Phase 15 tools (Preventive services and asthma): CHR-04, CHR-05.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 15 prevent: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_prevent_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
