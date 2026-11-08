//! The town-to-service directory framework shared by `find_ma_aging_services`
//! (AGE-02) and `find_crisis_resources` (MH-01).
//!
//! Directories are dated snapshots built from mass.gov by
//! `cargo xtask build-service-directories`: Aging Services Access Points and
//! SHINE offices (`ma-aging-service-directory`) and Community Behavioral
//! Health Centers (`ma-cbhc-directory`). Each provider lists the MassGIS town
//! names it serves; this module inverts those lists by town.

use phn_core::{PhnError, Result, Source};
use phn_data::SnapshotStore;
use phn_data::snapshots::SnapshotOrigin;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const AGING_SNAPSHOT: &str = "ma-aging-service-directory";
pub const CBHC_SNAPSHOT: &str = "ma-cbhc-directory";

/// An Aging Services Access Point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Asap {
    pub id: String,
    pub name: String,
    pub address: String,
    pub phone: String,
    pub phone_toll_free: Option<String>,
    pub web: Option<String>,
    /// Towns served (MassGIS names).
    pub towns: Vec<String>,
    /// Villages or Boston neighborhoods named in the published list.
    pub neighborhoods: Vec<String>,
    /// The "Areas served" text as published.
    pub served_as_published: String,
}

/// A town's SHINE regional office.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShineOffice {
    pub town: String,
    pub phone: String,
    pub office: String,
}

/// A Community Behavioral Health Center location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cbhc {
    pub id: String,
    pub name: String,
    pub address: String,
    /// Town where the center is.
    pub city: String,
    pub county: String,
    pub phone: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Towns the center lists as "Serving" (empty when none are published).
    pub towns: Vec<String>,
    pub villages: Vec<String>,
    pub served_as_published: String,
}

/// How the CBHCs for a town were found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CbhcMatch {
    /// The town is named in the center's "Serving" list.
    Listed,
    /// The town is in no "Serving" list; these centers are in the same county.
    SameCounty,
    /// No center was found.
    None,
}

/// Loaded directories.
#[derive(Debug, Clone)]
pub struct Directory {
    pub asaps: Vec<Asap>,
    pub shine: Vec<ShineOffice>,
    pub cbhcs: Vec<Cbhc>,
    pub aging_source: Source,
    pub cbhc_source: Source,
    pub aging_caveats: Vec<String>,
    pub cbhc_caveats: Vec<String>,
    /// True when a newer copy from /cache/snapshots was used.
    pub from_cache_override: bool,
}

fn split(s: &str) -> Vec<String> {
    s.split(';')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn records(text: &str) -> Result<(Vec<String>, Vec<csv::StringRecord>)> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| PhnError::Internal(e.to_string()))?
        .iter()
        .map(String::from)
        .collect();
    let rows = rdr
        .records()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| PhnError::Internal(e.to_string()))?;
    Ok((headers, rows))
}

fn get<'a>(headers: &[String], r: &'a csv::StringRecord, name: &str) -> &'a str {
    headers
        .iter()
        .position(|h| h == name)
        .and_then(|i| r.get(i))
        .unwrap_or_default()
}

impl Directory {
    /// Load both directories from the snapshot store.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (asap_text, aging_entry, o1) = store.file_text(AGING_SNAPSHOT, "ma_asap_directory.csv")?;
        let (shine_text, _, _) = store.file_text(AGING_SNAPSHOT, "ma_shine_directory.csv")?;
        let (cbhc_text, cbhc_entry, o2) = store.file_text(CBHC_SNAPSHOT, "ma_cbhc_directory.csv")?;
        let (h, rows) = records(&asap_text)?;
        let asaps = rows
            .iter()
            .map(|r| Asap {
                id: get(&h, r, "id").into(),
                name: get(&h, r, "name").into(),
                address: get(&h, r, "address").into(),
                phone: get(&h, r, "phone").into(),
                phone_toll_free: opt(get(&h, r, "phone_toll_free")),
                web: opt(get(&h, r, "web")),
                towns: split(get(&h, r, "towns")),
                neighborhoods: split(get(&h, r, "neighborhoods")),
                served_as_published: get(&h, r, "served_as_published").into(),
            })
            .collect();
        let (h, rows) = records(&shine_text)?;
        let shine = rows
            .iter()
            .map(|r| ShineOffice {
                town: get(&h, r, "town").into(),
                phone: get(&h, r, "phone").into(),
                office: get(&h, r, "office").into(),
            })
            .collect();
        let (h, rows) = records(&cbhc_text)?;
        let cbhcs = rows
            .iter()
            .map(|r| Cbhc {
                id: get(&h, r, "id").into(),
                name: get(&h, r, "name").into(),
                address: get(&h, r, "address").into(),
                city: get(&h, r, "city").into(),
                county: get(&h, r, "county").into(),
                phone: get(&h, r, "phone").into(),
                lat: get(&h, r, "lat").parse().ok(),
                lon: get(&h, r, "lon").parse().ok(),
                towns: split(get(&h, r, "towns")),
                villages: split(get(&h, r, "villages")),
                served_as_published: get(&h, r, "served_as_published").into(),
            })
            .collect();
        Ok(Self {
            asaps,
            shine,
            cbhcs,
            aging_source: aging_entry.to_source(),
            cbhc_source: cbhc_entry.to_source(),
            aging_caveats: aging_entry.caveats.clone(),
            cbhc_caveats: cbhc_entry.caveats.clone(),
            from_cache_override: o1 == SnapshotOrigin::CacheOverride
                || o2 == SnapshotOrigin::CacheOverride,
        })
    }

    /// ASAPs serving a town (Boston has several, by neighborhood).
    pub fn asaps_for(&self, town: &str) -> Vec<&Asap> {
        self.asaps
            .iter()
            .filter(|a| a.towns.iter().any(|t| t == town))
            .collect()
    }

    /// The town's SHINE regional office.
    pub fn shine_for(&self, town: &str) -> Option<&ShineOffice> {
        self.shine.iter().find(|s| s.town == town)
    }

    /// CBHCs for a town: those that list it, else those in its county.
    pub fn cbhcs_for(&self, town: &str, county: &str) -> (Vec<&Cbhc>, CbhcMatch) {
        let listed: Vec<&Cbhc> = self
            .cbhcs
            .iter()
            .filter(|c| c.towns.iter().any(|t| t == town))
            .collect();
        if !listed.is_empty() {
            return (listed, CbhcMatch::Listed);
        }
        let same: Vec<&Cbhc> = self
            .cbhcs
            .iter()
            .filter(|c| c.county.eq_ignore_ascii_case(county))
            .collect();
        if same.is_empty() {
            (Vec::new(), CbhcMatch::None)
        } else {
            (same, CbhcMatch::SameCounty)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_town_has_an_asap_and_most_have_a_listed_cbhc() {
        let d = Directory::load(&SnapshotStore::bundled()).unwrap();
        let ma = phn_data::ma::MaReference::load(&SnapshotStore::bundled()).unwrap();
        assert!(d.asaps.len() >= 20);
        assert!(d.cbhcs.len() >= 25);
        let mut listed = 0;
        for m in &ma.municipalities {
            assert!(!d.asaps_for(&m.name).is_empty(), "{} has no ASAP", m.name);
            let (c, how) = d.cbhcs_for(&m.name, &m.county);
            assert!(!c.is_empty(), "{} has no CBHC", m.name);
            if how == CbhcMatch::Listed {
                listed += 1;
            }
        }
        assert!(listed > 250, "only {listed} towns are in a CBHC list");
        assert!(d.asaps_for("Boston").len() >= 3);
        assert_eq!(d.cbhcs_for("Worcester", "Worcester").1, CbhcMatch::Listed);
        assert!(d.shine_for("Amherst").is_some());
    }
}
