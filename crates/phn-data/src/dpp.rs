//! Massachusetts diabetes prevention program sites (CHR-03), from the bundled
//! `ma-dpp-programs` snapshot built by `xtask/scripts/build_screen_snapshots.py`.
//!
//! The snapshot is the Department of Public Health's own list of "Active MA
//! DPPs" (town, site, address, and an organization contact when DPH names no
//! person). It is not the CDC Diabetes Prevention Recognition Program
//! registry, which was not reachable when the snapshot was built, so it does
//! not say which sites are CDC-recognized or how the program is delivered.

use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::SnapshotStore;

pub const SNAPSHOT_ID: &str = "ma-dpp-programs";
pub const FILE: &str = "ma_dpp_programs.csv";
/// DPH's list of programs.
pub const DPH_LIST_URL: &str =
    "https://www.mass.gov/service-details/diabetes-prevention-programs-in-massachusetts";
/// CDC's registry of recognized organizations (not bundled; its host was blocked at build time).
pub const CDC_REGISTRY_URL: &str = "https://dprp.cdc.gov/Registry";
/// CDC's page for finding a program (the locator filters by delivery mode and language).
pub const CDC_FIND_URL: &str =
    "https://www.cdc.gov/diabetes-prevention/lifestyle-change-program/find-a-program.html";

/// One program site as DPH lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DppSite {
    /// Massachusetts city or town ("" for a site outside Massachusetts).
    pub municipality: String,
    pub county: String,
    /// The "City/Town" text as DPH printed it (can be a village or neighborhood).
    pub listed_place: String,
    pub site_name: String,
    pub street: String,
    pub city: String,
    pub state: String,
    pub zip: String,
    /// Organization phone number, present only when DPH's contact cell names no person.
    pub org_phone: Option<String>,
    /// Organization email address, same rule.
    pub org_email: Option<String>,
    /// True when DPH lists a named contact person (name, email, and phone are not kept).
    pub has_named_contact: bool,
}

/// The parsed snapshot.
#[derive(Debug, Clone)]
pub struct DppPrograms {
    pub sites: Vec<DppSite>,
    pub vintage: String,
    pub retrieved: String,
    pub source: Source,
}

impl DppPrograms {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        let headers = rdr
            .headers()
            .map_err(|e| PhnError::Internal(format!("DPP snapshot: {e}")))?
            .clone();
        let col = |name: &str| -> Result<usize> {
            headers
                .iter()
                .position(|h| h == name)
                .ok_or_else(|| PhnError::Internal(format!("DPP snapshot has no column {name}")))
        };
        let (i_muni, i_county, i_place, i_site) = (
            col("municipality")?,
            col("county")?,
            col("listed_place")?,
            col("site_name")?,
        );
        let (i_street, i_city, i_state, i_zip) =
            (col("street")?, col("city")?, col("state")?, col("zip")?);
        let (i_phone, i_email, i_named) =
            (col("org_phone")?, col("org_email")?, col("has_named_contact")?);
        let mut sites = Vec::new();
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("DPP snapshot: {e}")))?;
            let g = |i: usize| r.get(i).unwrap_or("").trim().to_string();
            let opt = |i: usize| Some(g(i)).filter(|v| !v.is_empty());
            sites.push(DppSite {
                municipality: g(i_muni),
                county: g(i_county),
                listed_place: g(i_place),
                site_name: g(i_site),
                street: g(i_street),
                city: g(i_city),
                state: g(i_state),
                zip: g(i_zip),
                org_phone: opt(i_phone),
                org_email: opt(i_email),
                has_named_contact: g(i_named) == "yes",
            });
        }
        Ok(Self {
            sites,
            vintage: entry.source_vintage.clone(),
            retrieved: entry.retrieved.clone(),
            source: entry.to_source(),
        })
    }
}
