//! Massachusetts reference lists: the 351 cities and towns, their counties
//! and HMCC regions, and public school districts.

use phn_core::text::{self, Ranked};
use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::{SnapshotOrigin, SnapshotStore};

/// Form of government recorded by MassGIS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MunicipalityType {
    City,
    Town,
    /// A town with a city form of government.
    TownWithCityGovernment,
}

/// A Massachusetts city or town.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Municipality {
    /// MassGIS town identifier (1–351, alphabetical).
    pub town_id: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: MunicipalityType,
    pub county: String,
    /// Five-digit state+county FIPS code.
    pub county_fips: String,
    /// DPH Emergency Preparedness Region (2005 layer): 1, 2, 3, 4A, 4B, 4C, or 5.
    pub dph_ep_region: String,
    /// Health and Medical Coordinating Coalition region: 1, 2, 3, 4AB, 4C, or 5.
    pub hmcc_region: String,
}

/// A public school district.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SchoolDistrict {
    /// DESE eight-digit organization code.
    pub org_code: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// Member towns (for regional districts) or the town itself.
    pub member_towns: Vec<String>,
    /// Grade span, e.g. "PK-12", when known.
    pub grades: Option<String>,
}

/// An HMCC region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HmccRegion {
    pub id: &'static str,
    pub description: &'static str,
}

/// HMCC regions and a plain-language description of each.
pub const HMCC_REGIONS: [HmccRegion; 6] = [
    HmccRegion {
        id: "1",
        description: "Region 1 (Western Massachusetts)",
    },
    HmccRegion {
        id: "2",
        description: "Region 2 (Central Massachusetts)",
    },
    HmccRegion {
        id: "3",
        description: "Region 3 (Northeast Massachusetts)",
    },
    HmccRegion {
        id: "4AB",
        description: "Region 4AB (Metro Boston area, excluding the City of Boston)",
    },
    HmccRegion {
        id: "4C",
        description: "Region 4C (City of Boston)",
    },
    HmccRegion {
        id: "5",
        description: "Region 5 (Southeastern Massachusetts, Cape Cod, and the Islands)",
    },
];

/// Look up an HMCC region by id (case-insensitive, optional "Region " prefix).
pub fn hmcc_region(id: &str) -> Option<HmccRegion> {
    let upper = id.trim().to_ascii_uppercase();
    let key = upper.strip_prefix("REGION").unwrap_or(&upper).trim();
    HMCC_REGIONS.iter().copied().find(|r| r.id == key)
}

/// Loaded reference lists with their citations.
#[derive(Debug, Clone)]
pub struct MaReference {
    pub municipalities: Vec<Municipality>,
    pub school_districts: Vec<SchoolDistrict>,
    pub municipality_source: Source,
    pub district_source: Source,
    pub caveats: Vec<String>,
    pub origins: Vec<(String, SnapshotOrigin)>,
}

#[derive(Deserialize)]
struct MuniRow {
    town_id: u32,
    name: String,
    #[serde(rename = "type")]
    kind: MunicipalityType,
    county: String,
    county_fips: String,
    dph_ep_region: String,
    hmcc_region: String,
}

#[derive(Deserialize)]
struct DistrictRow {
    org_code: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    member_towns: String,
    grades: String,
}

impl MaReference {
    /// Load the lists from the snapshot store.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (muni_text, muni_entry, muni_origin) =
            store.file_text("ma-municipalities", "ma_municipalities.csv")?;
        let mut municipalities = Vec::new();
        for row in csv::Reader::from_reader(muni_text.as_bytes()).deserialize::<MuniRow>() {
            let r = row.map_err(|e| PhnError::Internal(format!("municipality snapshot: {e}")))?;
            municipalities.push(Municipality {
                town_id: r.town_id,
                name: r.name,
                kind: r.kind,
                county: r.county,
                county_fips: r.county_fips,
                dph_ep_region: r.dph_ep_region,
                hmcc_region: r.hmcc_region,
            });
        }

        let (dist_text, dist_entry, dist_origin) =
            store.file_text("ma-school-districts", "ma_school_districts.csv")?;
        let mut school_districts = Vec::new();
        for row in csv::Reader::from_reader(dist_text.as_bytes()).deserialize::<DistrictRow>() {
            let r = row.map_err(|e| PhnError::Internal(format!("district snapshot: {e}")))?;
            school_districts.push(SchoolDistrict {
                org_code: r.org_code,
                name: r.name,
                kind: r.kind,
                member_towns: r
                    .member_towns
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect(),
                grades: (!r.grades.is_empty()).then_some(r.grades),
            });
        }

        let mut caveats = muni_entry.caveats.clone();
        caveats.extend(dist_entry.caveats.iter().cloned());
        Ok(Self {
            municipalities,
            school_districts,
            municipality_source: muni_entry.to_source(),
            district_source: dist_entry.to_source(),
            caveats,
            origins: vec![
                (muni_entry.id.clone(), muni_origin),
                (dist_entry.id.clone(), dist_origin),
            ],
        })
    }

    /// Exact (normalized) town match.
    pub fn town(&self, name: &str) -> Option<&Municipality> {
        let n = text::normalize(name);
        self.municipalities
            .iter()
            .find(|m| text::normalize(&m.name) == n)
    }

    /// Ranked town candidates for a possibly misspelled name.
    pub fn suggest_towns(&self, query: &str, limit: usize) -> Vec<&Municipality> {
        text::rank(query, &self.municipalities, |m| vec![m.name.clone()], 0.75)
            .into_iter()
            .take(limit)
            .map(|r: Ranked| &self.municipalities[r.index])
            .collect()
    }

    /// Exact (normalized) district match by name or org code.
    pub fn district(&self, query: &str) -> Option<&SchoolDistrict> {
        let n = text::normalize(query);
        self.school_districts
            .iter()
            .find(|d| d.org_code == query.trim() || text::normalize(&d.name) == n)
    }

    /// Ranked district candidates.
    pub fn suggest_districts(&self, query: &str, limit: usize) -> Vec<&SchoolDistrict> {
        text::rank(
            query,
            &self.school_districts,
            |d| vec![d.name.clone()],
            0.75,
        )
        .into_iter()
        .take(limit)
        .map(|r| &self.school_districts[r.index])
        .collect()
    }

    /// Districts that serve a town.
    pub fn districts_for_town(&self, town: &str) -> Vec<&SchoolDistrict> {
        let n = text::normalize(town);
        self.school_districts
            .iter()
            .filter(|d| d.member_towns.iter().any(|t| text::normalize(t) == n))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> MaReference {
        MaReference::load(&SnapshotStore::bundled()).unwrap()
    }

    #[test]
    fn loads_351_municipalities_in_14_counties() {
        let r = reference();
        assert_eq!(r.municipalities.len(), 351);
        let counties: std::collections::BTreeSet<_> =
            r.municipalities.iter().map(|m| m.county.as_str()).collect();
        assert_eq!(counties.len(), 14);
        let boston = r.town("boston").unwrap();
        assert_eq!(boston.county, "Suffolk");
        assert_eq!(boston.hmcc_region, "4C");
        assert_eq!(boston.kind, MunicipalityType::City);
        assert!(
            r.municipalities
                .iter()
                .all(|m| hmcc_region(&m.hmcc_region).is_some())
        );
    }

    #[test]
    fn suggests_misspelled_towns_and_districts() {
        let r = reference();
        assert_eq!(
            r.town("Manchester by the Sea").unwrap().name,
            "Manchester-by-the-Sea"
        );
        assert_eq!(r.suggest_towns("Worchester", 3)[0].name, "Worcester");
        assert!(r.school_districts.len() > 350);
        assert!(!r.districts_for_town("Amherst").is_empty());
        assert!(r.district("Amherst").is_some());
    }

    #[test]
    fn hmcc_lookup_is_forgiving() {
        assert_eq!(hmcc_region("4ab").unwrap().id, "4AB");
        assert_eq!(hmcc_region("Region 5").unwrap().id, "5");
        assert!(hmcc_region("7").is_none());
    }
}
