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
//! * [`socrata`]: SoQL client for data.cdc.gov (NNDSS, PLACES, respiratory,
//!   and wastewater data).
//! * Phase 4: [`census`] (ACS 5-year), [`geography`] (census geographies and
//!   crosswalks), [`boundaries`] (map polygons), [`svi`], [`places`],
//!   [`eutils`] (PubMed), [`cdc_guidance`] (MMWR, HAN, Stacks), and
//!   [`apifetch`] (keyed, validated fetches).

pub mod cache;
pub mod fetch;
pub mod http;
pub mod ma;
pub mod snapshots;
pub mod socrata;

// Phase 4 adapters (Census ACS, SVI, NCBI E-utilities, CDC feeds) go below this line.
pub mod apifetch;
pub mod boundaries;
pub mod cdc_guidance;
pub mod census;
pub mod eutils;
pub mod geography;
pub mod places;
pub mod svi;

// Phase 5 adapters (openFDA, DailyMed, RxNorm, CVX/MVX) go below this line.

// Phase 6 adapters (CDSi supporting data, NNDSS, respiratory, wastewater) go below this line.

pub use cache::Cache;
pub use http::HttpClient;
pub use snapshots::SnapshotStore;
