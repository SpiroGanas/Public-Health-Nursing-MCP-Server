//! ACS indicators for older adults (65+): living alone, households without a
//! vehicle, limited English, and poverty, by county, city or town, and census
//! tract in Massachusetts (AGE-05). Disability among adults 65+ and the 65+
//! share of the population come from the Phase 4 ACS indicators
//! ([`crate::census`]).
//!
//! The bundled snapshot `census-acs5-ma-older-adults` is built by
//! `cargo xtask build-older-adult-snapshots` from the keyless ACS 5-year
//! table-based summary files, with margins of error computed the same way as
//! the Phase 4 indicators.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source};

use crate::census::{self, AcsRow, Cells, Estimate, IndicatorDef, IndicatorKind};
use crate::snapshots::{SnapshotEntry, SnapshotOrigin, SnapshotStore};

pub const SNAPSHOT_ID: &str = "census-acs5-ma-older-adults";
pub const SNAPSHOT_FILE: &str = "ma_acs5_older_adults.csv";
/// Detailed tables used.
pub const TABLES: [&str; 4] = ["B09020", "B25045", "B16004", "B17001"];

const LEP65: &[&str] = &[
    "B16004_050", "B16004_051", "B16004_052", "B16004_055", "B16004_056", "B16004_057",
    "B16004_060", "B16004_061", "B16004_062", "B16004_065", "B16004_066", "B16004_067",
];
const POV65_BELOW: &[&str] = &["B17001_015", "B17001_016", "B17001_029", "B17001_030"];
const POV65_ALL: &[&str] = &[
    "B17001_015", "B17001_016", "B17001_029", "B17001_030", "B17001_044", "B17001_045",
    "B17001_058", "B17001_059",
];
const HH65: &[&str] = &["B25045_006", "B25045_010", "B25045_015", "B25045_019"];
const HH65_NOVEH: &[&str] = &["B25045_006", "B25045_015"];

const fn def(
    id: &'static str,
    label: &'static str,
    table: &'static str,
    kind: IndicatorKind,
    num: &'static [&'static str],
    den: &'static [&'static str],
    universe: &'static str,
) -> IndicatorDef {
    IndicatorDef {
        id,
        section: "Older adults",
        label,
        table,
        kind,
        num,
        den,
        universe,
    }
}

/// Older-adult indicators (ids are snapshot columns).
pub const INDICATORS: &[IndicatorDef] = &[
    def("pop65", "Adults 65 and older", "B09020", IndicatorKind::Count, &["B09020_001"], &[], "Population 65 years and over"),
    def("alone65", "Adults 65+ living alone", "B09020", IndicatorKind::Count, &["B09020_015", "B09020_018"], &[], "Population 65 years and over"),
    def("alone65_pct", "Adults 65+ living alone (%)", "B09020", IndicatorKind::Percent, &["B09020_015", "B09020_018"], &["B09020_001"], "Population 65 years and over"),
    def("hh65", "Households with a householder 65+", "B25045", IndicatorKind::Count, HH65, &[], "Occupied housing units with a householder 65 years and over"),
    def("noveh65", "Households with a householder 65+ and no vehicle", "B25045", IndicatorKind::Count, HH65_NOVEH, &[], "Occupied housing units with a householder 65 years and over"),
    def("noveh65_pct", "Households with a householder 65+ that have no vehicle (%)", "B25045", IndicatorKind::Percent, HH65_NOVEH, HH65, "Occupied housing units with a householder 65 years and over"),
    def("lep65", "Adults 65+ who speak English less than \"very well\"", "B16004", IndicatorKind::Count, LEP65, &[], "Population 65 years and over (5 years and over universe)"),
    def("lep65_pct", "Adults 65+ who speak English less than \"very well\" (%)", "B16004", IndicatorKind::Percent, LEP65, &["B16004_046"], "Population 65 years and over (5 years and over universe)"),
    def("pov65", "Adults 65+ below the poverty level", "B17001", IndicatorKind::Count, POV65_BELOW, &[], "Population 65+ for whom poverty status is determined"),
    def("pov65_pct", "Adults 65+ below the poverty level (%)", "B17001", IndicatorKind::Percent, POV65_BELOW, POV65_ALL, "Population 65+ for whom poverty status is determined"),
];

/// Every cell the indicators need.
pub fn required_cells() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = INDICATORS
        .iter()
        .flat_map(|d| d.num.iter().chain(d.den.iter()).copied())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Compute every older-adult indicator from raw cells.
pub fn compute_all(cells: &Cells) -> BTreeMap<String, Estimate> {
    INDICATORS
        .iter()
        .filter_map(|d| census::compute(d, cells).map(|e| (d.id.to_string(), e)))
        .collect()
}

/// An indicator by id.
pub fn indicator(id: &str) -> Option<&'static IndicatorDef> {
    INDICATORS.iter().find(|d| d.id == id)
}

/// The bundled older-adult snapshot.
#[derive(Debug, Clone)]
pub struct OlderAdultSnapshot {
    pub rows: HashMap<String, AcsRow>,
    pub entry: SnapshotEntry,
    pub origin: SnapshotOrigin,
}

impl OlderAdultSnapshot {
    /// Parse the snapshot file.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, origin) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE)?;
        Ok(Self {
            rows: census::parse_indicator_csv(&text)?,
            entry,
            origin,
        })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static OlderAdultSnapshot> {
        static CELL: OnceLock<std::result::Result<OlderAdultSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("older-adult ACS snapshot: {e}")))
    }

    /// Citation.
    pub fn source(&self) -> Source {
        let mut s = self.entry.to_source();
        s.title = format!(
            "{} — tables {}",
            census::acs_title(census::SNAPSHOT_VINTAGE),
            TABLES.join(", ")
        );
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_covers_every_town_with_plausible_values() {
        let s = OlderAdultSnapshot::load(&SnapshotStore::bundled()).unwrap();
        let towns = s.rows.values().filter(|r| r.level == census::GeoLevel::Town).count();
        assert_eq!(towns, 351);
        let ma = &s.rows["25"].values;
        for id in ["pop65", "alone65_pct", "noveh65_pct", "lep65_pct", "pov65_pct"] {
            assert!(ma.contains_key(id), "{id}");
        }
        // Massachusetts: roughly a quarter to a third of adults 65+ live alone.
        assert!((20.0..40.0).contains(&ma["alone65_pct"].value));
        assert!((3.0..20.0).contains(&ma["pov65_pct"].value));
        assert!(required_cells().len() > 20);
    }
}
