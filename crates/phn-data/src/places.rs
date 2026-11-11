//! CDC PLACES: Local Data for Better Health (2025 release) for Massachusetts.
//!
//! PLACES publishes model-based estimates of 40 measures (chronic conditions,
//! prevention, risk behaviors, disability, health status, and health-related
//! social needs) for counties, places, census tracts, and ZCTAs. Live queries
//! go to the four Socrata datasets on data.cdc.gov through the live → cache →
//! snapshot chain; the bundled `cdc-places-2025-ma` snapshot holds every
//! Massachusetts location.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::cache::Cache;
use crate::fetch::SnapshotFallback;
use crate::http::HttpClient;
use crate::snapshots::{SnapshotEntry, SnapshotStore};
use crate::socrata::{self, SocrataQuery, soql_string};

/// Release year.
pub const RELEASE: &str = "2025";
/// Snapshot id.
pub const SNAPSHOT_ID: &str = "cdc-places-2025-ma";
/// Wide estimates file.
pub const FILE: &str = "ma_places_2025.csv";
/// Measure definitions and reference values.
pub const MEASURES_FILE: &str = "ma_places_2025_measures.csv";
/// Measure definitions page.
pub const DEFINITIONS_URL: &str = "https://www.cdc.gov/places/measure-definitions/index.html";
/// Cache TTL for live PLACES queries (7 days; releases are annual).
pub const TTL_SECS: u64 = 7 * 24 * 3600;

/// PLACES geographic level.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PlacesLevel {
    County,
    /// Census place (city or census-designated place).
    Place,
    Tract,
    Zcta,
}

impl PlacesLevel {
    /// Socrata dataset id of the 2025 release.
    pub fn dataset_id(self) -> &'static str {
        match self {
            Self::County => "swc5-untb",
            Self::Place => "eav7-hnsx",
            Self::Tract => "cwsq-ngmh",
            Self::Zcta => "qnzd-25i4",
        }
    }

    /// Dataset title.
    pub fn dataset_title(self) -> String {
        let l = match self {
            Self::County => "County Data",
            Self::Place => "Place Data",
            Self::Tract => "Census Tract Data",
            Self::Zcta => "ZCTA Data",
        };
        format!("PLACES: Local Data for Better Health, {l}, {RELEASE} release")
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "county" => Self::County,
            "place" => Self::Place,
            "tract" => Self::Tract,
            "zcta" => Self::Zcta,
            _ => return None,
        })
    }
}

/// One measure's definition and reference values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Measure {
    /// PLACES measure id, e.g. `DIABETES`.
    pub id: String,
    pub short_name: String,
    pub name: String,
    pub category: String,
    /// BRFSS survey year behind the estimate.
    pub data_year: String,
    /// U.S. reference: adult-population-weighted mean of county crude estimates (derived).
    pub us_crude: Option<f64>,
    pub us_age_adjusted: Option<f64>,
    /// Massachusetts reference: weighted mean of the 14 county estimates (derived).
    pub ma_crude: Option<f64>,
    pub ma_age_adjusted: Option<f64>,
}

/// An estimate with its 95% confidence interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlacesValue {
    /// Prevalence (%).
    pub value: f64,
    pub low: Option<f64>,
    pub high: Option<f64>,
}

/// Estimates for one location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlacesLocation {
    pub level: PlacesLevel,
    pub location_id: String,
    pub name: String,
    /// Adult population (18+) used by PLACES.
    pub population_18plus: Option<f64>,
    /// Crude prevalence by measure id.
    pub crude: BTreeMap<String, PlacesValue>,
    /// Age-adjusted prevalence (counties and places only).
    pub age_adjusted: BTreeMap<String, PlacesValue>,
}

/// The bundled snapshot.
#[derive(Debug, Clone)]
pub struct PlacesSnapshot {
    pub measures: Vec<Measure>,
    pub locations: BTreeMap<(PlacesLevel, String), PlacesLocation>,
    pub entry: SnapshotEntry,
}

impl PlacesSnapshot {
    /// Parse the snapshot files.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (mtext, entry, _) = store.file_text(SNAPSHOT_ID, MEASURES_FILE)?;
        let mut measures = Vec::new();
        let mut rdr = csv::Reader::from_reader(mtext.as_bytes());
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("PLACES measures: {e}")))?;
            let s = |i: usize| r.get(i).unwrap_or_default().to_string();
            let f = |i: usize| r.get(i).and_then(|v| v.parse().ok());
            measures.push(Measure {
                id: s(0),
                short_name: s(1),
                name: s(2),
                category: s(3),
                data_year: s(4),
                us_crude: f(5),
                us_age_adjusted: f(6),
                ma_crude: f(7),
                ma_age_adjusted: f(8),
            });
        }
        let (text, _, _) = store.file_text(SNAPSHOT_ID, FILE)?;
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        let headers = rdr
            .headers()
            .map_err(|e| PhnError::Internal(format!("PLACES snapshot: {e}")))?
            .clone();
        let mut locations: BTreeMap<(PlacesLevel, String), PlacesLocation> = BTreeMap::new();
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("PLACES snapshot: {e}")))?;
            let level = PlacesLevel::parse(r.get(0).unwrap_or_default())
                .ok_or_else(|| PhnError::Internal("PLACES snapshot: bad level".into()))?;
            let id = r.get(1).unwrap_or_default().to_string();
            let age_adj = r.get(3) == Some("ageadj");
            let loc = locations
                .entry((level, id.clone()))
                .or_insert_with(|| PlacesLocation {
                    level,
                    location_id: id.clone(),
                    name: r.get(2).unwrap_or_default().to_string(),
                    population_18plus: r.get(4).and_then(|v| v.parse().ok()),
                    crude: BTreeMap::new(),
                    age_adjusted: BTreeMap::new(),
                });
            let mut i = 5;
            while i < headers.len() {
                let m = &headers[i];
                let num = |j: usize| r.get(j).and_then(|v| v.parse::<f64>().ok());
                if let Some(v) = num(i) {
                    let pv = PlacesValue {
                        value: v,
                        low: num(i + 1),
                        high: num(i + 2),
                    };
                    if age_adj {
                        loc.age_adjusted.insert(m.to_string(), pv);
                    } else {
                        loc.crude.insert(m.to_string(), pv);
                    }
                }
                i += 3;
            }
        }
        Ok(Self {
            measures,
            locations,
            entry,
        })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static PlacesSnapshot> {
        static CELL: OnceLock<std::result::Result<PlacesSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("PLACES snapshot: {e}")))
    }

    /// One location.
    pub fn location(&self, level: PlacesLevel, id: &str) -> Option<&PlacesLocation> {
        self.locations.get(&(level, id.to_string()))
    }

    /// A measure definition (id or short name, case-insensitive).
    pub fn measure(&self, q: &str) -> Option<&Measure> {
        let q = q.trim();
        self.measures
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(q) || m.short_name.eq_ignore_ascii_case(q))
    }

    /// The location as Socrata JSON rows (the live response shape), for the
    /// snapshot fallback.
    pub fn as_rows(&self, level: PlacesLevel, id: &str) -> Option<Vec<u8>> {
        let loc = self.location(level, id)?;
        let mut rows = Vec::new();
        for (kind, map) in [("CrdPrv", &loc.crude), ("AgeAdjPrv", &loc.age_adjusted)] {
            for (m, v) in map {
                let mut row = json!({
                    "locationid": loc.location_id,
                    "locationname": loc.name,
                    "measureid": m,
                    "datavaluetypeid": kind,
                    "data_value": v.value.to_string(),
                });
                let o = row.as_object_mut().expect("object");
                if let Some(l) = v.low {
                    o.insert("low_confidence_limit".into(), Value::String(l.to_string()));
                }
                if let Some(h) = v.high {
                    o.insert("high_confidence_limit".into(), Value::String(h.to_string()));
                }
                if let Some(p) = loc.population_18plus {
                    o.insert("totalpop18plus".into(), Value::String(p.to_string()));
                }
                rows.push(row);
            }
        }
        serde_json::to_vec(&rows).ok()
    }
}

/// Live (or cached, or snapshot) estimates for one location.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacesFetch {
    pub location: PlacesLocation,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
    pub url: String,
}

impl PlacesFetch {
    /// Citation.
    pub fn source(&self) -> Source {
        Source {
            title: self.location.level.dataset_title(),
            publisher: "Centers for Disease Control and Prevention, Division of Population Health"
                .into(),
            url: Some(format!(
                "https://data.cdc.gov/d/{}",
                self.location.level.dataset_id()
            )),
            effective_date: Some(RELEASE.into()),
            retrieved: Some(crate::fetch::local_date(&self.as_of)),
            mode: self.mode,
        }
    }
}

/// Build rows into a location.
pub fn location_from_rows(
    level: PlacesLevel,
    id: &str,
    rows: &[Map<String, Value>],
) -> Option<PlacesLocation> {
    if rows.is_empty() {
        return None;
    }
    let mut loc = PlacesLocation {
        level,
        location_id: id.to_string(),
        name: String::new(),
        population_18plus: None,
        crude: BTreeMap::new(),
        age_adjusted: BTreeMap::new(),
    };
    for r in rows {
        if loc.name.is_empty() {
            loc.name = socrata::text(r, "locationname").unwrap_or(id).to_string();
            if level == PlacesLevel::County && !loc.name.ends_with("County") {
                loc.name.push_str(" County");
            }
        }
        if loc.population_18plus.is_none() {
            loc.population_18plus = socrata::num(r, "totalpop18plus");
        }
        let (Some(m), Some(v)) = (socrata::text(r, "measureid"), socrata::num(r, "data_value"))
        else {
            continue;
        };
        let pv = PlacesValue {
            value: v,
            low: socrata::num(r, "low_confidence_limit"),
            high: socrata::num(r, "high_confidence_limit"),
        };
        if socrata::text(r, "datavaluetypeid") == Some("AgeAdjPrv") {
            loc.age_adjusted.insert(m.to_string(), pv);
        } else {
            loc.crude.insert(m.to_string(), pv);
        }
    }
    Some(loc)
}

/// Fetch one location's estimates (live → cache → bundled snapshot).
pub async fn fetch_location(
    http: &HttpClient,
    cache: Option<&Cache>,
    snapshot: Option<&'static PlacesSnapshot>,
    level: PlacesLevel,
    id: &str,
) -> Result<PlacesFetch> {
    let q = SocrataQuery::cdc(level.dataset_id())
        .select("locationid,locationname,measureid,datavaluetypeid,data_value,low_confidence_limit,high_confidence_limit,totalpop18plus")
        .and_where(format!("locationid = {}", soql_string(id)))
        .limit(500);
    let id_owned = id.to_string();
    let fallback: Option<SnapshotFallback> = snapshot.map(|s| {
        let f: SnapshotFallback = Box::new(move || {
            s.as_rows(level, &id_owned)
                .map(|b| (b, s.entry.retrieved.clone()))
        });
        f
    });
    let rows = socrata::query(http, cache, &q, TTL_SECS, fallback).await?;
    let location = location_from_rows(level, id, &rows.rows).ok_or_else(|| {
        PhnError::NotFound(format!(
            "PLACES has no estimates for {} {id} in the {RELEASE} release",
            match level {
                PlacesLevel::County => "county",
                PlacesLevel::Place => "place",
                PlacesLevel::Tract => "tract",
                PlacesLevel::Zcta => "ZCTA",
            }
        ))
    })?;
    Ok(PlacesFetch {
        location,
        mode: rows.mode,
        as_of: rows.as_of,
        fallback_reason: rows.fallback_reason,
        url: rows.url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_loads_every_level_and_round_trips_rows() {
        let s = PlacesSnapshot::load(&SnapshotStore::bundled()).unwrap();
        assert_eq!(s.measures.len(), 40);
        let counties = s
            .locations
            .keys()
            .filter(|k| k.0 == PlacesLevel::County)
            .count();
        assert_eq!(counties, 14);
        let hampden = s.location(PlacesLevel::County, "25013").unwrap();
        assert!(hampden.crude.contains_key("DIABETES"));
        assert!(hampden.age_adjusted.contains_key("DIABETES"));
        let d = s.measure("diabetes").unwrap();
        assert!(d.us_crude.is_some() && d.ma_crude.is_some());
        let rows: Vec<Map<String, Value>> =
            serde_json::from_slice(&s.as_rows(PlacesLevel::County, "25013").unwrap()).unwrap();
        let back = location_from_rows(PlacesLevel::County, "25013", &rows).unwrap();
        assert_eq!(back.crude, hampden.crude);
        assert_eq!(back.name, "Hampden County");
        assert!(
            s.locations
                .keys()
                .filter(|k| k.0 == PlacesLevel::Tract)
                .count()
                > 1500
        );
        assert!(
            s.locations
                .keys()
                .filter(|k| k.0 == PlacesLevel::Zcta)
                .count()
                > 500
        );
    }

    #[tokio::test]
    async fn offline_fetch_uses_the_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let config = phn_core::Config::for_root(dir.path());
        let http = HttpClient::new(&config).unwrap();
        let snap = PlacesSnapshot::shared(&SnapshotStore::bundled()).unwrap();
        let f = fetch_location(&http, None, Some(snap), PlacesLevel::Zcta, "01002")
            .await
            .unwrap();
        assert_eq!(f.mode, SourceMode::Snapshot);
        assert!(f.location.crude.len() >= 30);
        assert!(f.source().url.unwrap().ends_with("qnzd-25i4"));
    }
}
