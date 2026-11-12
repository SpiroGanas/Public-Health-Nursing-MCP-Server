//! Phase 19 tools (Law search, policy briefs, information sharing): POL-02, POL-04, POL-07.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 19 law: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase19_law_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
