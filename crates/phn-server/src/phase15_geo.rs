//! Phase 15 tools (Offline geocoder): EPI-13.

use crate::server::PhnServer;
use rmcp::tool_router;

// Phase 15 geo: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_geo_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
