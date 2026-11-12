//! Phase 16 tools (Interventions, theory, performance tracking): PLAN-03, PLAN-04, PLAN-11.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 16 interventions: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_interventions_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
