//! Phase 16 tools (Logic models, objectives, evaluation plans): PLAN-01, PLAN-02, PLAN-05.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 16 design: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_design_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
