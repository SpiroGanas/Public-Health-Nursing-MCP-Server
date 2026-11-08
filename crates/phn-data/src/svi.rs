//! CDC/ATSDR Social Vulnerability Index (SVI) 2022 for Massachusetts census
//! tracts, from the bundled `cdc-svi-2022-ma` snapshot.
//!
//! Percentile ranks (0–1) are given two ways: within Massachusetts (the
//! state-ranked file) and nationally (the U.S.-ranked file). Higher ranks
//! mean more vulnerable.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::SnapshotStore;

/// Snapshot id.
pub const SNAPSHOT_ID: &str = "cdc-svi-2022-ma";
/// Snapshot file.
pub const FILE: &str = "ma_svi_2022.csv";
/// Release documentation.
pub const DOCUMENTATION_URL: &str =
    "https://www.atsdr.cdc.gov/place-health/php/svi/svi-data-documentation-download.html";

/// The four SVI themes (2020 and 2022 names), with the variables in each.
pub const THEMES: [(&str, &str, &[&str]); 4] = [
    (
        "theme1",
        "Socioeconomic status",
        &[
            "ep_pov150",
            "ep_unemp",
            "ep_hburd",
            "ep_nohsdp",
            "ep_uninsur",
        ],
    ),
    (
        "theme2",
        "Household characteristics",
        &[
            "ep_age65",
            "ep_age17",
            "ep_disabl",
            "ep_sngpnt",
            "ep_limeng",
        ],
    ),
    (
        "theme3",
        "Racial and ethnic minority status",
        &["ep_minrty"],
    ),
    (
        "theme4",
        "Housing type and transportation",
        &["ep_munit", "ep_mobile", "ep_crowd", "ep_noveh", "ep_groupq"],
    ),
];

/// Plain-language labels of the 16 SVI variables (2022 definitions).
pub const VARIABLES: [(&str, &str); 16] = [
    ("ep_pov150", "Below 150% of the poverty level"),
    ("ep_unemp", "Unemployed (civilian, age 16+)"),
    (
        "ep_hburd",
        "Housing cost burden (30%+ of income, households under $75,000)",
    ),
    ("ep_nohsdp", "No high school diploma (age 25+)"),
    ("ep_uninsur", "No health insurance"),
    ("ep_age65", "Aged 65 and older"),
    ("ep_age17", "Aged 17 and younger"),
    ("ep_disabl", "Civilian with a disability"),
    (
        "ep_sngpnt",
        "Single-parent households with children under 18",
    ),
    ("ep_limeng", "Speak English \"less than well\" (age 5+)"),
    (
        "ep_minrty",
        "Racial and ethnic minority (all except non-Hispanic White)",
    ),
    ("ep_munit", "Housing in structures with 10+ units"),
    ("ep_mobile", "Mobile homes"),
    ("ep_crowd", "Crowding (more people than rooms)"),
    ("ep_noveh", "No vehicle available"),
    ("ep_groupq", "Living in group quarters"),
];

/// SVI percentile ranks (0–1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ranks {
    pub overall: Option<f64>,
    pub theme1: Option<f64>,
    pub theme2: Option<f64>,
    pub theme3: Option<f64>,
    pub theme4: Option<f64>,
}

impl Ranks {
    /// Rank for a theme id ("overall", "theme1", ...).
    pub fn get(&self, id: &str) -> Option<f64> {
        match id {
            "overall" => self.overall,
            "theme1" => self.theme1,
            "theme2" => self.theme2,
            "theme3" => self.theme3,
            "theme4" => self.theme4,
            _ => None,
        }
    }
}

/// One tract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SviTract {
    /// 11-digit tract GEOID.
    pub fips: String,
    pub location: String,
    pub population: Option<f64>,
    pub population_moe: Option<f64>,
    /// Percentile ranks among Massachusetts tracts.
    pub ma: Ranks,
    /// Percentile ranks among all U.S. tracts.
    pub us: Ranks,
    /// Number of the 16 variables at or above the 90th percentile (state file).
    pub flags: Option<f64>,
    /// Percent estimates of the 16 variables (`ep_*`).
    pub percents: BTreeMap<String, f64>,
}

/// The loaded snapshot.
#[derive(Debug, Clone)]
pub struct SviSnapshot {
    pub tracts: Vec<SviTract>,
    pub source: Source,
}

impl SviSnapshot {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        let headers = rdr
            .headers()
            .map_err(|e| PhnError::Internal(format!("SVI snapshot: {e}")))?
            .clone();
        let mut tracts = Vec::new();
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("SVI snapshot: {e}")))?;
            let field = |name: &str| -> Option<f64> {
                headers
                    .iter()
                    .position(|h| h == name)
                    .and_then(|i| r.get(i))
                    .and_then(|v| v.parse().ok())
            };
            let ranks = |prefix: &str| Ranks {
                overall: field(&format!("{prefix}_rpl_themes")),
                theme1: field(&format!("{prefix}_rpl_theme1")),
                theme2: field(&format!("{prefix}_rpl_theme2")),
                theme3: field(&format!("{prefix}_rpl_theme3")),
                theme4: field(&format!("{prefix}_rpl_theme4")),
            };
            let percents = VARIABLES
                .iter()
                .filter_map(|(id, _)| field(id).map(|v| (id.to_string(), v)))
                .collect();
            tracts.push(SviTract {
                fips: r.get(0).unwrap_or_default().to_string(),
                location: r.get(1).unwrap_or_default().to_string(),
                population: field("pop"),
                population_moe: field("pop_moe"),
                ma: ranks("ma"),
                us: ranks("us"),
                flags: field("f_total"),
                percents,
            });
        }
        let mut source = entry.to_source();
        source.url = Some(DOCUMENTATION_URL.into());
        Ok(Self { tracts, source })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static SviSnapshot> {
        static CELL: OnceLock<std::result::Result<SviSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("SVI snapshot: {e}")))
    }

    /// A tract by GEOID.
    pub fn tract(&self, fips: &str) -> Option<&SviTract> {
        self.tracts.iter().find(|t| t.fips == fips)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_has_massachusetts_tracts_with_both_rankings() {
        let s = SviSnapshot::load(&SnapshotStore::bundled()).unwrap();
        assert!(s.tracts.len() > 1600, "{}", s.tracts.len());
        let ranked: Vec<&SviTract> = s.tracts.iter().filter(|t| t.ma.overall.is_some()).collect();
        assert!(ranked.len() > 1550);
        assert!(
            ranked
                .iter()
                .all(|t| (0.0..=1.0).contains(&t.ma.overall.unwrap()))
        );
        assert!(ranked.iter().filter(|t| t.us.overall.is_some()).count() > 1550);
        // State percentile ranks span the full range.
        let max = ranked
            .iter()
            .map(|t| t.ma.overall.unwrap())
            .fold(0.0, f64::max);
        assert!(max > 0.99);
    }
}
