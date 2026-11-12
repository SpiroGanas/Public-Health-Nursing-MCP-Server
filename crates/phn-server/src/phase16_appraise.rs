//! Phase 16 tools (Evidence appraisal and synthesis): EBP-03, EBP-04.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 16 appraise: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_appraise_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
