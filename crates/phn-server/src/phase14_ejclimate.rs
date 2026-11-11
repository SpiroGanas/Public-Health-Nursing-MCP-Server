//! Phase 14 tools (environmental health and climate). Environmental justice and climate: ENV-04 environmental_justice_screen, ENV-05 climate_health_vulnerability_profile.

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
#[tool_router(router = phase14_ejclimate_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
