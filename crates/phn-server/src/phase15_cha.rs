//! Phase 15 tools (Community health assessment): EQ-08, EQ-09, EQ-10.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 15 cha: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_cha_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
