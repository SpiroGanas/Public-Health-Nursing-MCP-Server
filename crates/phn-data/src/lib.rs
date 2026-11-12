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
pub mod water;
// End of Phase 14 water.
// Phase 14 EJ and climate (ENV-04, ENV-05).
pub mod ejclimate;
// End of Phase 14 ejclimate.
// Phase 14 Vectors and housing (ENV-06, ENV-07).
pub mod vectorborne;
// End of Phase 14 vectorhousing.
// Phase 14 Chemicals and food (ENV-08, CHR-06).
pub mod chemicals;
pub mod food_access;
// End of Phase 14 chemfood.
// End of Phase 14 adapters.
// Phase 15 adapters go below this line.
// Phase 15 screen (CHR-02, CHR-03).
pub mod dpp;
// End of Phase 15 screen.
// Phase 15 prevent (CHR-04, CHR-05).
pub mod preventive;
// End of Phase 15 prevent.
// Phase 15 cha (EQ-08, EQ-09, EQ-10).
// End of Phase 15 cha.
// Phase 15 geo (EPI-13).
pub mod geocoder;
// End of Phase 15 geo.
// End of Phase 15 adapters.
// Phase 16 adapters go below this line.
// Phase 16 design (PLAN-01, PLAN-02, PLAN-05).
// End of Phase 16 design.
// Phase 16 interventions (PLAN-03, PLAN-04, PLAN-11).
pub mod cpstf;
// End of Phase 16 interventions.
// Phase 16 appraise (EBP-03, EBP-04).
pub mod crossref;
// End of Phase 16 appraise.
// Phase 16 qi (EBP-05).
// End of Phase 16 qi.
// End of Phase 16 adapters.
// Phase 17 adapters go below this line.
// Phase 17 grants (PLAN-06, PLAN-07, PLAN-08).
// End of Phase 17 grants.
// Phase 17 budget (PLAN-09, PLAN-10).
// End of Phase 17 budget.
// Phase 17 finance (LEAD-01, LEAD-02).
// End of Phase 17 finance.
// End of Phase 17 adapters.
// Phase 18 adapters go below this line.
// Phase 18 strategy (LEAD-03, LEAD-04, LEAD-05).
// End of Phase 18 strategy.
// Phase 18 workforce (LEAD-06, LEAD-07, LEAD-08).
// End of Phase 18 workforce.
// Phase 18 ethics (EBP-07, POL-05, POL-06).
// End of Phase 18 ethics.
// End of Phase 18 adapters.
// Phase 19 adapters go below this line.
// Phase 19 tracking (POL-01, POL-03).
// End of Phase 19 tracking.
// Phase 19 law (POL-02, POL-04, POL-07).
// End of Phase 19 law.
// End of Phase 19 adapters.
