//! Phase 16 tools (Quality improvement planning): EBP-05.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 16 qi: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_qi_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
