//! Phase 13 tools (emergency preparedness II): EPR-08 design_hseep_exercise and EPR-09 build_after_action_report (HSEEP).

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

#[tool_router(router = phase13_hseep_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
