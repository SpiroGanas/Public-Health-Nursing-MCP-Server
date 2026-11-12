//! ZIP code to town and place name to town, from the bundled `ma-geocoder-crosswalk`
//! snapshot (`xtask/scripts/build_geocoder_index.py --crosswalk`).
//!
//! The shares are the fraction of the ZIP code's (or place name's) MassGIS address
//! points that lie in each town; ZIP codes cross town lines, so a ZIP-only result is
//! an approximation unless one town holds nearly all of them.

use std::collections::HashMap;

use phn_core::{PhnError, Result, Source};

use super::normalize::normalize_place;
use crate::snapshots::SnapshotStore;

/// Snapshot id and file.
pub const SNAPSHOT_ID: &str = "ma-geocoder-crosswalk";
pub const FILE: &str = "ma_geocoder_crosswalk.csv";

/// One town a ZIP code or place name reaches.
#[derive(Debug, Clone, PartialEq)]
pub struct ZipTown {
    pub town_id: u16,
    pub town: String,
    pub addresses: u32,
    /// Share of the ZIP's (or place's) addresses in this town (0–1).
    pub share: f64,
}

/// The loaded crosswalk.
#[derive(Debug, Clone)]
pub struct Crosswalk {
    zips: HashMap<String, Vec<ZipTown>>,
    places: HashMap<String, Vec<ZipTown>>,
    towns: HashMap<String, (u16, String)>,
    pub source: Source,
    pub caveats: Vec<String>,
}

impl Crosswalk {
    /// Parse the snapshot; `towns` is the list of (MassGIS id, name) of the 351 cities and towns.
    pub fn load(store: &SnapshotStore, towns: &[(u16, String)]) -> Result<Self> {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
        let mut zips: HashMap<String, Vec<ZipTown>> = HashMap::new();
        let mut places: HashMap<String, Vec<ZipTown>> = HashMap::new();
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("geocoder crosswalk: {e}")))?;
            let g = |i: usize| r.get(i).unwrap_or_default();
            let entry = ZipTown {
                town_id: g(2).parse().map_err(|_| PhnError::Internal("geocoder crosswalk town id".into()))?,
                town: g(3).to_string(),
                addresses: g(4).parse().unwrap_or(0),
                share: g(5).parse().unwrap_or(0.0),
            };
            match g(0) {
                "zip" => zips.entry(g(1).to_string()).or_default().push(entry),
                "place" => places.entry(normalize_place(g(1))).or_default().push(entry),
                _ => {}
            }
        }
        for v in zips.values_mut().chain(places.values_mut()) {
            v.sort_by(|a, b| b.share.total_cmp(&a.share).then(a.town_id.cmp(&b.town_id)));
        }
        let towns = towns
            .iter()
            .map(|(id, name)| (normalize_place(name), (*id, name.clone())))
            .collect();
        Ok(Self {
            zips,
            places,
            towns,
            source: entry.to_source(),
            caveats: entry.caveats.clone(),
        })
    }

    /// A city or town (or a village or post-office name MassGIS lists for a town) to its town.
    /// `normalized` comes from [`normalize_place`].
    pub fn town_by_name(&self, normalized: &str) -> Option<(u16, String)> {
        if let Some(t) = self.towns.get(normalized) {
            return Some(t.clone());
        }
        let p = self.places.get(normalized)?;
        let best = p.first()?;
        (best.share >= 0.9).then(|| (best.town_id, best.town.clone()))
    }

    /// True for a town name or a known place name (used to find a city in an address line).
    pub fn is_place(&self, normalized: &str) -> bool {
        self.towns.contains_key(normalized) || self.places.contains_key(normalized)
    }

    /// The towns a ZIP code reaches, largest share first.
    pub fn zip_towns(&self, zip: &str) -> &[ZipTown] {
        self.zips.get(zip).map_or(&[], Vec::as_slice)
    }

    /// The one town a ZIP code belongs to when its largest share is at least `min_share`.
    pub fn zip_town(&self, zip: &str, min_share: f64) -> Option<&ZipTown> {
        self.zip_towns(zip).first().filter(|t| t.share >= min_share)
    }

    /// Number of ZIP codes in the table.
    pub fn zip_count(&self) -> usize {
        self.zips.len()
    }
}
