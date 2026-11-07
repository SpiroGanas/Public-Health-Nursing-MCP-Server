//! Local data access with privacy guardrails.
//!
//! * [`sandbox`]: path resolution confined to the mounted `/data` folder.

pub mod sandbox;

pub use sandbox::Sandbox;
