//! Tool implementations.
//!
//! Each tool is a plain function taking a [`Context`] and typed arguments and
//! returning a [`phn_core::Envelope`]. The MCP server crate wraps them with
//! `rmcp` tool definitions, so the logic can be tested without a transport.

pub mod aging;
pub mod bh;
pub mod cd;
pub mod charts;
pub mod chronic;
pub mod comms;
pub mod context;
pub mod core;
pub mod ebp;
pub mod env;
pub mod epi;
pub mod epr;
pub mod equity;
pub mod leadership;
pub mod localdata;
pub mod mch;
pub mod md;
pub mod planning;
pub mod policy;
pub mod registry;
pub mod school;
pub mod vax;

pub use context::Context;
