//! Public Health Nursing MCP server library: the `rmcp` server definition,
//! used by the `phn-mcp-server` binary and the integration tests.

mod phase1;
mod resources;
mod server;

pub use server::PhnServer;
