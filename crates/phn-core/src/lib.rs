//! Shared foundation for the Public Health Nursing MCP server.
//!
//! * [`config`]: environment variables, the persisted settings file in `/cache`,
//!   and the nurse's jurisdiction.
//! * [`envelope`]: the response contract every tool returns (summary, data,
//!   sources, caveats, privacy).
//! * [`toolsets`]: toolset and role-profile registry.
//! * [`error`]: the shared error type.
//! * [`text`]: normalization and fuzzy matching used by lookups.

pub mod config;
pub mod envelope;
pub mod error;
pub mod text;
pub mod toolsets;

pub use config::{Config, Jurisdiction, Letterhead, Paths, Settings};
pub use envelope::{DECISION_SUPPORT_NOTICE, Envelope, Privacy, Source, SourceMode};
pub use error::{PhnError, Result};
pub use toolsets::{Profile, Toolset};

/// Name used in the MCP `serverInfo` block and the HTTP User-Agent.
pub const SERVER_NAME: &str = "public-health-nursing-mcp-server";

/// Crate version shared across the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
