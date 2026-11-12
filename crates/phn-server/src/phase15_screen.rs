//! Phase 15 tools (BP screening and diabetes prevention): CHR-02, CHR-03.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 15 screen: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_screen_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
