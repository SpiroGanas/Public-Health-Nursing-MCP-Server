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
mod phase15_cha;
mod phase15_geo;
mod phase15_prevent;
mod phase15_screen;
mod phase16_appraise;
mod phase16_design;
mod phase16_interventions;
mod phase16_qi;
mod phase17_budget;
mod phase17_finance;
mod phase17_grants;
mod phase18_ethics;
mod phase18_strategy;
mod phase18_workforce;
mod phase19_law;
mod phase19_tracking;
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
