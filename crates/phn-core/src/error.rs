//! Shared error type.

use std::path::PathBuf;

/// Result alias using [`PhnError`].
pub type Result<T, E = PhnError> = std::result::Result<T, E>;

/// Errors raised by the server's crates.
///
/// Most variants are shown to the model as tool-level errors (the request was
/// valid but could not be completed), so messages are written for the nurse.
#[derive(Debug, thiserror::Error)]
pub enum PhnError {
    /// The arguments were understood but are not acceptable.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// A requested item (file, condition, town, …) does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// A path was rejected by the `/data` sandbox.
    #[error("path not allowed: {0}")]
    Sandbox(String),

    /// A file could not be parsed.
    #[error("could not read {path}: {message}")]
    Parse { path: PathBuf, message: String },

    /// A feature or file type is not supported.
    #[error("not supported: {0}")]
    Unsupported(String),

    /// A privacy guardrail blocked the request.
    #[error("blocked by privacy guardrail: {0}")]
    Privacy(String),

    /// A network request failed.
    #[error("network error: {0}")]
    Network(String),

    /// Configuration is missing or invalid.
    #[error("configuration error: {0}")]
    Config(String),

    /// Filesystem error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Unexpected internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl PhnError {
    /// Convenience constructor for parse errors.
    pub fn parse(path: impl Into<PathBuf>, message: impl std::fmt::Display) -> Self {
        Self::Parse {
            path: path.into(),
            message: message.to_string(),
        }
    }
}
