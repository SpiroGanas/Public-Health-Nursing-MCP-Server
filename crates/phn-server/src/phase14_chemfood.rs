//! Phase 14 tools (environmental health and climate). Chemicals and food: ENV-08 chemical_exposure_reference, CHR-06 food_access_mapper.

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
#[tool_router(router = phase14_chemfood_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
