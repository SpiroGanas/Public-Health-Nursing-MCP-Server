//! Phase 14 tools (environmental health and climate). Water: ENV-03 drinking_water_quality_lookup, CHR-07 community_water_fluoridation_status.

#[allow(unused_imports)]
use phn_core::Envelope;
#[allow(unused_imports)]
use rmcp::handler::server::common::schema_for_output;
#[allow(unused_imports)]
use rmcp::handler::server::wrapper::Parameters;
#[allow(unused_imports)]
use rmcp::model::CallToolResult;
#[allow(unused_imports)]
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

// Remove `allow_empty` when the first tool is added.
#[tool_router(router = phase14_water_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
