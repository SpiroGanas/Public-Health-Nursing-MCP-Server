//! Public Health Nursing MCP server library: the `rmcp` server definition,
//! used by the `phn-mcp-server` binary and the integration tests.

mod phase1;
mod phase2;
mod phase3;
mod phase4;
mod phase5;
mod phase6;
mod prompts;
mod resources;
mod server;

pub use server::PhnServer;
