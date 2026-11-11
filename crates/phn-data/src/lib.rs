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
pub mod medication;
pub mod vaccine_codes;

// Phase 6 adapters (CDSi supporting data, NNDSS, respiratory, wastewater) go below this line.

// Phase 10 adapters (CMS Provider Data Catalog) go below this line.
pub mod cms_provider;
pub mod older_adults;
// Phase 9 adapters (HRSA health centers) go below this line.
pub mod hrsa;
// Phase 12 adapters (NWS, OpenFEMA, emPOWER, AirNow, HeatRisk, projection) go below this line.
pub mod airnow;
pub mod census_blocks;
pub mod empower;
pub mod heatrisk;
pub mod nws;
pub mod openfema;
pub mod projection;
// End of Phase 12 adapters.
// Phase 11 adapters (FindTreatment.gov) go below this line.
pub mod findtreatment;

pub use cache::Cache;
pub use http::HttpClient;
pub use snapshots::SnapshotStore;
// Phase 14 adapters go below this line.
// Phase 14 Water (ENV-03, CHR-07).
// End of Phase 14 water.
// Phase 14 EJ and climate (ENV-04, ENV-05).
// End of Phase 14 ejclimate.
// Phase 14 Vectors and housing (ENV-06, ENV-07).
// End of Phase 14 vectorhousing.
// Phase 14 Chemicals and food (ENV-08, CHR-06).
// End of Phase 14 chemfood.
// End of Phase 14 adapters.
