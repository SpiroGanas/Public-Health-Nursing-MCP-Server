//! USDA ERS Food Access Research Atlas for Massachusetts census tracts
//! (CHR-06), from the bundled `usda-food-access-ma` snapshot built by
//! `xtask/scripts/build_chemfood_snapshots.py`.
//!
//! * The 2025 SNAP-authorized Retailer Access Map (SRAM) uses 2020 census
//!   tracts, the same tracts as the project's boundaries, ACS, and SVI. Each
//!   tract has its general characteristics and two sets of access measures:
//!   straight-line (Euclidean) distance (`sd_`) and driving (road network)
//!   distance (`dd_`) to the nearest SNAP-authorized store.
//! * The 2019 Large Retailer Access Map (LRAM, formerly the Food Access
//!   Research Atlas) measures distance to supermarkets, supercenters, and
//!   large grocery stores on 2010 tracts. The builder carried its
//!   low-income/low-access flags to 2020 tracts with the Census Bureau's 2020
//!   to 2010 tract relationship file: each `lram_*` value is the share of the
//!   2020 tract's land area that lay in a 2010 tract with the flag.
//!
//! ERS's map services (gisportal.ers.usda.gov) are not reachable from every
//! network, so there is no live fetch; the data change every few years.

use std::collections::HashMap;
use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::SnapshotStore;

pub const SNAPSHOT_ID: &str = "usda-food-access-ma";
pub const FILE: &str = "usda_food_access_ma.csv";
/// The Atlas home page.
pub const ATLAS_URL: &str = "https://www.ers.usda.gov/data-products/food-access-research-atlas";
/// The download page.
pub const DOWNLOAD_URL: &str =
    "https://www.ers.usda.gov/data-products/food-access-research-atlas/download-the-data";

/// Distance measure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Distance {
    /// Straight-line ("as the crow flies") distance.
    #[default]
    StraightLine,
    /// Driving distance along roads.
    Driving,
}

impl Distance {
    pub fn label(self) -> &'static str {
        match self {
            Distance::StraightLine => "straight-line distance",
            Distance::Driving => "driving distance",
        }
    }
}

/// The four low-income/low-access definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Definition {
    /// More than 1 mile (urban) or 10 miles (rural).
    OneAndTen,
    /// More than one-half mile (urban) or 10 miles (rural).
    HalfAndTen,
    /// More than 1 mile (urban) or 20 miles (rural).
    OneAndTwenty,
    /// At least 100 households without a vehicle more than one-half mile away, or
    /// 500 people / 33% more than 20 miles away.
    Vehicle,
}

impl Definition {
    pub const ALL: [Definition; 4] = [
        Definition::OneAndTen,
        Definition::HalfAndTen,
        Definition::OneAndTwenty,
        Definition::Vehicle,
    ];

    /// Short label.
    pub fn label(self) -> &'static str {
        match self {
            Definition::OneAndTen => "1 mile urban / 10 miles rural",
            Definition::HalfAndTen => "1/2 mile urban / 10 miles rural",
            Definition::OneAndTwenty => "1 mile urban / 20 miles rural",
            Definition::Vehicle => "vehicle access (or 20 miles)",
        }
    }

    /// Stable id.
    pub fn id(self) -> &'static str {
        match self {
            Definition::OneAndTen => "one_and_ten",
            Definition::HalfAndTen => "half_and_ten",
            Definition::OneAndTwenty => "one_and_twenty",
            Definition::Vehicle => "vehicle",
        }
    }
}

/// Access measures for one distance type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AccessMeasures {
    /// Low-income and low-access flags: [1/10, 1/2/10, 1/20, vehicle].
    pub lila: [Option<bool>; 4],
    /// Low-access flags (no income criterion): [1/10, 1/2/10, 1/20].
    pub low_access: [Option<bool>; 3],
    /// At least 100 households without a vehicle more than 1/2 mile from a store.
    pub low_vehicle_access: Option<bool>,
    /// People beyond the distance: [1/10, 1/2/10, 1/20].
    pub low_access_pop: [Option<f64>; 3],
    /// Low-income people beyond the distance: [1/10, 1/2/10, 1/20].
    pub low_access_low_income_pop: [Option<f64>; 3],
    /// Households without a vehicle more than 1/2 mile and more than 1 mile away.
    pub no_vehicle_households_beyond_half: Option<f64>,
    pub no_vehicle_households_beyond_1: Option<f64>,
    /// Children and adults 65+ more than 1 mile away.
    pub kids_beyond_1: Option<f64>,
    pub seniors_beyond_1: Option<f64>,
}

impl AccessMeasures {
    /// Low-income/low-access flag for a definition.
    pub fn lila(&self, d: Definition) -> Option<bool> {
        self.lila[d as usize]
    }

    /// People beyond the definition's distance (none for the vehicle definition).
    pub fn low_access_people(&self, d: Definition) -> Option<f64> {
        match d {
            Definition::Vehicle => None,
            _ => self.low_access_pop[d as usize],
        }
    }

    /// Low-income people beyond the definition's distance.
    pub fn low_access_low_income_people(&self, d: Definition) -> Option<f64> {
        match d {
            Definition::Vehicle => None,
            _ => self.low_access_low_income_pop[d as usize],
        }
    }
}

/// One 2020 census tract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FoodAccessTract {
    pub geoid: String,
    pub urban: Option<bool>,
    pub population: Option<f64>,
    pub households: Option<f64>,
    pub group_quarters: Option<bool>,
    /// ERS low-income tract (poverty rate 20%+ or median family income at or below 80%
    /// of the state or metro median).
    pub low_income: Option<bool>,
    pub poverty_rate: Option<f64>,
    pub median_family_income: Option<f64>,
    pub low_income_people: Option<f64>,
    pub kids: Option<f64>,
    pub seniors: Option<f64>,
    pub households_no_vehicle: Option<f64>,
    pub households_snap: Option<f64>,
    pub straight_line: AccessMeasures,
    pub driving: AccessMeasures,
    /// LRAM 2019 (supermarkets; 2010 tracts): share of this tract's land in a 2010
    /// low-income/low-access tract, by definition [1/10, 1/2/10, 1/20, vehicle].
    pub lram_share: [Option<f64>; 4],
}

impl FoodAccessTract {
    /// Measures for a distance type.
    pub fn measures(&self, d: Distance) -> &AccessMeasures {
        match d {
            Distance::StraightLine => &self.straight_line,
            Distance::Driving => &self.driving,
        }
    }
}

/// The loaded snapshot.
#[derive(Debug, Clone)]
pub struct FoodAccess {
    pub tracts: Vec<FoodAccessTract>,
    by_geoid: HashMap<String, usize>,
    pub source: Source,
    pub vintage: String,
}

fn parse_tract(get: &dyn Fn(&str) -> Option<String>) -> FoodAccessTract {
    let num = |k: &str| get(k).and_then(|v| v.parse::<f64>().ok());
    let flag = |k: &str| get(k).and_then(|v| match v.as_str() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    });
    let measures = |p: &str| AccessMeasures {
        lila: [
            flag(&format!("{p}_lila_1_10")),
            flag(&format!("{p}_lila_half_10")),
            flag(&format!("{p}_lila_1_20")),
            flag(&format!("{p}_lila_vehicle")),
        ],
        low_access: [
            flag(&format!("{p}_la_1_10")),
            flag(&format!("{p}_la_half_10")),
            flag(&format!("{p}_la_1_20")),
        ],
        low_vehicle_access: flag(&format!("{p}_hunv_flag")),
        low_access_pop: [
            num(&format!("{p}_lapop_1_10")),
            num(&format!("{p}_lapop_half_10")),
            num(&format!("{p}_lapop_1_20")),
        ],
        low_access_low_income_pop: [
            num(&format!("{p}_lalowi_1_10")),
            num(&format!("{p}_lalowi_half_10")),
            num(&format!("{p}_lalowi_1_20")),
        ],
        no_vehicle_households_beyond_half: num(&format!("{p}_lahunv_half")),
        no_vehicle_households_beyond_1: num(&format!("{p}_lahunv_1")),
        kids_beyond_1: num(&format!("{p}_lakids_1")),
        seniors_beyond_1: num(&format!("{p}_laseniors_1")),
    };
    FoodAccessTract {
        geoid: get("geoid").unwrap_or_default(),
        urban: flag("urban"),
        population: num("pop2020"),
        households: num("ohu2020"),
        group_quarters: flag("group_quarters_flag"),
        low_income: flag("low_income"),
        poverty_rate: num("poverty_rate"),
        median_family_income: num("median_family_income"),
        low_income_people: num("low_income_pop"),
        kids: num("kids"),
        seniors: num("seniors"),
        households_no_vehicle: num("hh_no_vehicle"),
        households_snap: num("hh_snap"),
        straight_line: measures("sd"),
        driving: measures("dd"),
        lram_share: [
            num("lram_lila_1_10_share"),
            num("lram_lila_half_10_share"),
            num("lram_lila_1_20_share"),
            num("lram_lila_vehicle_share"),
        ],
    }
}

impl FoodAccess {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        let headers = rdr
            .headers()
            .map_err(|e| PhnError::Internal(format!("food access snapshot: {e}")))?
            .clone();
        let index: HashMap<&str, usize> =
            headers.iter().enumerate().map(|(i, h)| (h, i)).collect();
        let mut tracts = Vec::new();
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("food access snapshot: {e}")))?;
            let get = |k: &str| -> Option<String> {
                index
                    .get(k)
                    .and_then(|&i| r.get(i))
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(String::from)
            };
            tracts.push(parse_tract(&get));
        }
        let by_geoid = tracts
            .iter()
            .enumerate()
            .map(|(i, t)| (t.geoid.clone(), i))
            .collect();
        Ok(Self {
            tracts,
            by_geoid,
            vintage: entry.source_vintage.clone(),
            source: entry.to_source(),
        })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static FoodAccess> {
        static CELL: OnceLock<std::result::Result<FoodAccess, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("food access snapshot: {e}")))
    }

    /// A tract by GEOID.
    pub fn tract(&self, geoid: &str) -> Option<&FoodAccessTract> {
        self.by_geoid.get(geoid).map(|&i| &self.tracts[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshot_loads_with_2020_tracts() {
        let f = FoodAccess::load(&SnapshotStore::bundled()).unwrap();
        assert!(f.tracts.len() > 1600, "{}", f.tracts.len());
        assert!(f.tracts.iter().all(|t| t.geoid.len() == 11 && t.geoid.starts_with("25")));
        // Some tracts are LILA under the half-mile definition, fewer under 1 mile.
        let count = |d: Definition, dist: Distance| {
            f.tracts
                .iter()
                .filter(|t| t.measures(dist).lila(d) == Some(true))
                .count()
        };
        let half = count(Definition::HalfAndTen, Distance::StraightLine);
        let one = count(Definition::OneAndTen, Distance::StraightLine);
        assert!(half > one && one > 0, "{half} {one}");
        assert!(count(Definition::HalfAndTen, Distance::Driving) >= half);
        // The 2020 geography matches the project's tract list.
        let geos = crate::geography::MaGeographies::load(&SnapshotStore::bundled()).unwrap();
        let matched = f.tracts.iter().filter(|t| geos.get(&t.geoid).is_some()).count();
        assert_eq!(matched, f.tracts.len());
        assert!(
            f.tracts
                .iter()
                .any(|t| t.lram_share.iter().any(|s| s.is_some_and(|v| v > 0.5)))
        );
    }
}
