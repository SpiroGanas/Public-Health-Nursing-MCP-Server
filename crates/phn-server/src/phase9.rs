//! Phase 9 tools: PHN practice toolkit and case management.

#[allow(unused_imports)]
use phn_core::Envelope;
#[allow(unused_imports)]
use rmcp::handler::server::common::schema_for_output;
#[allow(unused_imports)]
use rmcp::handler::server::wrapper::Parameters;
#[allow(unused_imports)]
use rmcp::model::CallToolResult;
#[allow(unused_imports)]
use rmcp::tool;
use rmcp::tool_router;

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

// Remove `allow_empty` once this phase has tools.
#[tool_router(router = phase9_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
