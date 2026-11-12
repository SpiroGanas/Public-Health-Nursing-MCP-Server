//! Phase 18 tools (QI vs research, ethics, scope of practice): EBP-07, POL-05, POL-06.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 18 ethics: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase18_ethics_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
