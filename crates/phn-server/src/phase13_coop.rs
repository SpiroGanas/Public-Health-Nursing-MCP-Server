//! Phase 13 tools (emergency preparedness II): EPR-07 continuity_of_operations_planner (COOP).

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

#[tool_router(router = phase13_coop_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
