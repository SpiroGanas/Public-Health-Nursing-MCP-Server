//! Phase 14 tools (environmental health and climate). Vectors and housing: ENV-06 vector_borne_disease_risk, ENV-07 housing_conditions_health_guide.

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
#[tool_router(router = phase14_vectorhousing_router, vis = "pub(crate)", allow_empty)]
impl PhnServer {}
