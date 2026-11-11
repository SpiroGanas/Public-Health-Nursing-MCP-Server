//! Public Health Nursing MCP server library: the `rmcp` server definition,
//! used by the `phn-mcp-server` binary and the integration tests.

mod phase1;
mod phase10;
mod phase11;
mod phase12;
mod phase13_cerc;
mod phase13_coop;
mod phase13_hseep;
mod phase13_ics;
mod phase14_chemfood;
mod phase14_ejclimate;
mod phase14_vectorhousing;
mod phase14_water;
mod phase2;
mod phase3;
mod phase4;
mod phase5;
mod phase6;
mod phase7;
mod phase8;
mod phase9;
mod prompts;
mod resources;
mod server;

pub use server::PhnServer;
