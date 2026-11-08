//! Massachusetts census geographies and crosswalks: counties, the 351 cities
//! and towns (Census county subdivisions), 2020 census tracts, and ZIP Code
//! Tabulation Areas (ZCTAs), from the bundled `ma-census-geographies`
//! snapshot.
//!
//! Towns use the MassGIS names of [`crate::ma`] ("Manchester-by-the-Sea");
//! the Census name ("Manchester-by-the-Sea town", "Barnstable Town city") is
//! kept in `census_name`. Each tract and ZCTA lists the towns it overlaps
//! with a share (tracts: share of census blocks; ZCTAs: share of land area).

use std::collections::HashMap;
use std::sync::OnceLock;

use phn_core::text;
use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::census::GeoLevel;
use crate::snapshots::SnapshotStore;

/// Snapshot id.
pub const SNAPSHOT_ID: &str = "ma-census-geographies";
/// Snapshot file.
pub const FILE: &str = "ma_census_geographies.csv";

/// One geography.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Geography {
    /// Census GEOID: 5 digits (county), 10 (town), 11 (tract), or the 5-digit ZCTA.
    pub geoid: String,
    pub level: GeoLevel,
    /// Display name (MassGIS name for towns).
    pub name: String,
    /// Name used by the Census Bureau.
    pub census_name: String,
    /// Five-digit county FIPS code.
    pub county_geoid: String,
    /// Main town of a tract or ZCTA.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub town: Option<String>,
    /// Towns a tract or ZCTA overlaps, with shares (0–1).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub town_shares: Vec<(String, f64)>,
    /// Land area in square miles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aland_sqmi: Option<f64>,
    /// 2020 PUMA (state + 5 digits) of a tract.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub puma: Option<String>,
}

/// The loaded crosswalk.
#[derive(Debug, Clone)]
pub struct MaGeographies {
    pub records: Vec<Geography>,
    by_geoid: HashMap<String, usize>,
    pub source: Source,
}

impl MaGeographies {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
        let mut records = Vec::new();
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("geography snapshot: {e}")))?;
            let get = |i: usize| r.get(i).unwrap_or_default().to_string();
            let level = GeoLevel::parse(&get(1)).ok_or_else(|| {
                PhnError::Internal(format!("geography snapshot: level {}", get(1)))
            })?;
            let town_shares = get(6)
                .split(';')
                .filter_map(|p| {
                    let (t, s) = p.rsplit_once(':')?;
                    Some((t.to_string(), s.parse().ok()?))
                })
                .collect();
            records.push(Geography {
                geoid: get(0),
                level,
                name: get(2),
                census_name: get(3),
                county_geoid: get(4),
                town: Some(get(5)).filter(|t| !t.is_empty()),
                town_shares,
                aland_sqmi: get(7).parse().ok(),
                puma: Some(get(8)).filter(|p| !p.is_empty()),
            });
        }
        let by_geoid = records
            .iter()
            .enumerate()
            .map(|(i, g)| (g.geoid.clone(), i))
            .collect();
        Ok(Self {
            records,
            by_geoid,
            source: entry.to_source(),
        })
    }

    /// The shared, lazily parsed crosswalk.
    pub fn shared(store: &SnapshotStore) -> Result<&'static MaGeographies> {
        static CELL: OnceLock<std::result::Result<MaGeographies, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("geography snapshot: {e}")))
    }

    /// By GEOID.
    pub fn get(&self, geoid: &str) -> Option<&Geography> {
        self.by_geoid.get(geoid).map(|&i| &self.records[i])
    }

    /// Every geography at a level.
    pub fn at_level(&self, level: GeoLevel) -> impl Iterator<Item = &Geography> {
        self.records.iter().filter(move |g| g.level == level)
    }

    /// A town by name (normalized; "Manchester by the Sea" works).
    pub fn town(&self, name: &str) -> Option<&Geography> {
        let n = text::normalize(name);
        let stripped = n
            .strip_suffix(" city")
            .or_else(|| n.strip_suffix(" town"))
            .unwrap_or(&n)
            .to_string();
        self.at_level(GeoLevel::Town).find(|g| {
            let gn = text::normalize(&g.name);
            gn == n || gn == stripped || text::normalize(&g.census_name) == n
        })
    }

    /// A county by name ("Hampden", "Hampden County") or FIPS ("25013" or "013").
    pub fn county(&self, query: &str) -> Option<&Geography> {
        let q = query.trim();
        let digits = q.chars().all(|c| c.is_ascii_digit());
        if digits {
            let fips = if q.len() == 3 {
                format!("25{q}")
            } else {
                q.to_string()
            };
            return self.get(&fips).filter(|g| g.level == GeoLevel::County);
        }
        let n = text::normalize(q);
        let n = n.strip_suffix(" county").unwrap_or(&n).to_string();
        self.at_level(GeoLevel::County).find(|g| {
            text::normalize(&g.name)
                .strip_suffix(" county")
                .is_some_and(|c| c == n)
        })
    }

    /// Tracts whose main town is `town`.
    pub fn tracts_in_town(&self, town: &str) -> Vec<&Geography> {
        self.at_level(GeoLevel::Tract)
            .filter(|g| g.town.as_deref() == Some(town))
            .collect()
    }

    /// Tracts that overlap `town` at all, with the share of the tract in it.
    pub fn tracts_overlapping_town(&self, town: &str) -> Vec<(&Geography, f64)> {
        self.at_level(GeoLevel::Tract)
            .filter_map(|g| {
                g.town_shares
                    .iter()
                    .find(|(t, _)| t == town)
                    .map(|(_, s)| (g, *s))
            })
            .collect()
    }

    /// ZCTAs that overlap `town`, with the share of each ZCTA's land in it.
    pub fn zctas_for_town(&self, town: &str) -> Vec<(&Geography, f64)> {
        self.at_level(GeoLevel::Zcta)
            .filter_map(|g| {
                g.town_shares
                    .iter()
                    .find(|(t, _)| t == town)
                    .map(|(_, s)| (g, *s))
            })
            .collect()
    }

    /// Ranked town-name suggestions for a misspelling.
    pub fn suggest_towns(&self, query: &str, limit: usize) -> Vec<String> {
        let towns: Vec<&Geography> = self.at_level(GeoLevel::Town).collect();
        text::rank(query, &towns, |g| vec![g.name.clone()], 0.7)
            .into_iter()
            .take(limit)
            .map(|r| towns[r.index].name.clone())
            .collect()
    }

    /// Resolve free text to a geography: a town or city name, "X County", a
    /// county FIPS, an 11-digit tract GEOID, or a 5-digit ZIP/ZCTA. `level`
    /// narrows the search when given.
    pub fn resolve(&self, query: &str, level: Option<GeoLevel>) -> Result<&Geography> {
        let q = query.trim();
        if q.is_empty() {
            return Err(PhnError::InvalidInput("give a place name or code".into()));
        }
        let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
        let all_digits = !digits.is_empty()
            && q.chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == ' ');
        let found = match level {
            Some(GeoLevel::Tract) => self.tract(q),
            Some(GeoLevel::Zcta) => self.get(&digits).filter(|g| g.level == GeoLevel::Zcta),
            Some(GeoLevel::County) => self.county(q),
            Some(GeoLevel::Town) => self.town(q),
            Some(_) => None,
            None if all_digits && digits.len() == 11 => self.get(&digits),
            None if all_digits && digits.len() == 5 && digits.starts_with("25") => {
                self.get(&digits).filter(|g| g.level == GeoLevel::County)
            }
            None if all_digits && digits.len() == 5 => {
                self.get(&digits).filter(|g| g.level == GeoLevel::Zcta)
            }
            None if all_digits && digits.len() == 10 => self.get(&digits),
            None if text::normalize(q).ends_with(" county") => self.county(q),
            None => self.town(q).or_else(|| self.county(q)),
        };
        found.ok_or_else(|| {
            let mut msg = format!("'{q}' is not a Massachusetts ");
            msg.push_str(match level {
                Some(GeoLevel::Tract) => "census tract (use the 11-digit GEOID, e.g. 25015820400)",
                Some(GeoLevel::Zcta) => "ZIP Code Tabulation Area",
                Some(GeoLevel::County) => "county",
                _ => "city, town, county, census tract, or ZIP code area",
            });
            let s = self.suggest_towns(q, 3);
            if !s.is_empty() && !matches!(level, Some(GeoLevel::Tract | GeoLevel::Zcta)) {
                msg.push_str(&format!(". Did you mean: {}?", s.join(", ")));
            }
            PhnError::NotFound(msg)
        })
    }

    /// A tract by GEOID, or by "8204" / "8204.01" when unique.
    pub fn tract(&self, q: &str) -> Option<&Geography> {
        let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() == 11 {
            return self.get(&digits).filter(|g| g.level == GeoLevel::Tract);
        }
        let short = q.trim().trim_start_matches("Census Tract").trim();
        let matches: Vec<&Geography> = self
            .at_level(GeoLevel::Tract)
            .filter(|g| g.census_name == format!("Census Tract {short}"))
            .collect();
        (matches.len() == 1).then(|| matches[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crosswalk_covers_every_town_and_resolves_names() {
        let g = MaGeographies::load(&SnapshotStore::bundled()).unwrap();
        let ma = crate::ma::MaReference::load(&SnapshotStore::bundled()).unwrap();
        assert_eq!(g.at_level(GeoLevel::Town).count(), 351);
        for m in &ma.municipalities {
            let t = g
                .town(&m.name)
                .unwrap_or_else(|| panic!("{} missing", m.name));
            assert_eq!(t.county_geoid, m.county_fips, "{}", m.name);
        }
        assert_eq!(g.town("Boston").unwrap().census_name, "Boston city");
        assert_eq!(g.town("Barnstable Town city").unwrap().name, "Barnstable");
        assert_eq!(g.resolve("Hampden County", None).unwrap().geoid, "25013");
        assert_eq!(g.resolve("01002", None).unwrap().level, GeoLevel::Zcta);
        assert_eq!(g.at_level(GeoLevel::County).count(), 14);
        assert!(g.at_level(GeoLevel::Tract).count() > 1600);
        let amherst_tracts = g.tracts_in_town("Amherst");
        assert!(!amherst_tracts.is_empty());
        assert!(!g.zctas_for_town("Amherst").is_empty());
        let err = g.resolve("Worchester", None).unwrap_err().to_string();
        assert!(err.contains("Worcester"), "{err}");
    }
}
