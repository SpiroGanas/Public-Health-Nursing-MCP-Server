//! Phase 1 tools: local data, privacy guardrails, and documents.

use rmcp::tool_router;

use crate::server::PhnServer;

#[tool_router(router = phase1_router, vis = "pub(crate)", allow_empty = true)]
impl PhnServer {}
