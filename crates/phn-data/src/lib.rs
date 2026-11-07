//! Data access layer.
//!
//! * [`http`]: outbound HTTP client that honors `HTTPS_PROXY`/`NO_PROXY`,
//!   adds an agency CA bundle, and respects offline mode.
//! * [`cache`]: SQLite cache for live responses and source health checks.
//! * [`snapshots`]: dated data snapshots bundled in the image, with checksums
//!   and an optional newer copy in `/cache/snapshots`.
//! * [`fetch`]: the live → cache → snapshot fallback chain.
//! * [`ma`]: Massachusetts reference lists (351 municipalities, counties,
//!   HMCC regions, school districts).

pub mod cache;
pub mod fetch;
pub mod http;
pub mod ma;
pub mod snapshots;

pub use cache::Cache;
pub use http::HttpClient;
pub use snapshots::SnapshotStore;
