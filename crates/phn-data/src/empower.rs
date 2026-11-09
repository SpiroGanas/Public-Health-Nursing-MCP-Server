//! HHS emPOWER Map data: Medicare beneficiaries who rely on
//! electricity-dependent durable medical equipment (DME), by county and ZIP
//! code, from the emPOWER Map's JSON endpoints on empowerprogram.hhs.gov.
//!
//! The emPOWER REST Service (ArcGIS) carries the same totals but is hosted on
//! arcgis.com, which some agency networks block; the map's own endpoints are
//! used instead, with the bundled `hhs-empower-ma` snapshot as fallback.
//! Values from 1 to 11 are published as 11 (CMS cell-size suppression).

use phn_core::{PhnError, Result, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::apifetch::{expect_json, fetch_validated};
use crate::cache::Cache;
use crate::fetch::SnapshotFallback;
use crate::http::HttpClient;
use crate::snapshots::SnapshotStore;

pub const SNAPSHOT_ID: &str = "hhs-empower-ma";
pub const SNAPSHOT_FILE: &str = "empower_ma.csv";
/// The emPOWER Map (for people).
pub const MAP_URL: &str = "https://empowerprogram.hhs.gov/empowermap";
/// The emPOWER Program home page.
pub const PROGRAM_URL: &str = "https://empowerprogram.hhs.gov/";
/// Counties of Massachusetts.
pub const COUNTIES_URL: &str =
    "https://empowerprogram.hhs.gov/api/v1/dme/states/Massachusetts/counties.json";
/// ZIP codes of Massachusetts (with map geometry, which is dropped).
pub const ZIPS_URL: &str =
    "https://empowerprogram.hhs.gov/api/v1/dme/states/Massachusetts/zipcodes.json";
/// Every state.
pub const STATES_URL: &str = "https://empowerprogram.hhs.gov/api/v1/dme/states.json";
/// HHS updates the data monthly; cache for a week.
pub const TTL_SECS: u64 = 7 * 24 * 3600;
/// The value HHS shows for counts from 1 to 11.
pub const SUPPRESSED_VALUE: u64 = 11;

/// Geography level of an emPOWER row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmpowerLevel {
    Nation,
    State,
    County,
    Zip,
}

/// One emPOWER row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EmpowerRow {
    pub level: EmpowerLevel,
    /// "US", "25", a county FIPS ("25015"), or a ZIP code.
    pub geoid: String,
    pub name: String,
    /// County name (ZIP codes: the county HHS assigns the ZIP to).
    pub county: Option<String>,
    /// Medicare beneficiaries (fee-for-service and Medicare Advantage).
    pub medicare_beneficiaries: u64,
    /// Beneficiaries with claims for electricity-dependent DME or devices.
    pub electricity_dependent: u64,
}

impl EmpowerRow {
    /// True when the DME count is HHS's suppressed value (the true count is 1–11).
    pub fn dme_suppressed(&self) -> bool {
        self.electricity_dependent == SUPPRESSED_VALUE
    }
}

fn number(v: Option<&Value>) -> Option<u64> {
    match v? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.replace(',', "").trim().parse().ok(),
        _ => None,
    }
}

/// Parse the counties endpoint.
pub fn parse_counties(body: &[u8]) -> Result<Vec<EmpowerRow>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("emPOWER counties were not JSON ({e})")))?;
    let arr = v
        .as_array()
        .ok_or_else(|| PhnError::Network("emPOWER counties response is not a list".into()))?;
    let rows: Vec<EmpowerRow> = arr
        .iter()
        .filter_map(|r| {
            let name = r.get("area")?.as_str()?.to_string();
            Some(EmpowerRow {
                level: EmpowerLevel::County,
                geoid: r.get("countyFips")?.as_str()?.to_string(),
                county: Some(name.clone()),
                name,
                medicare_beneficiaries: number(r.get("benes"))?,
                electricity_dependent: number(r.get("dme"))?,
            })
        })
        .collect();
    if rows.is_empty() {
        return Err(PhnError::Network(
            "emPOWER returned no Massachusetts counties".into(),
        ));
    }
    Ok(rows)
}

/// Parse the ZIP code endpoint (geometry is ignored).
pub fn parse_zips(body: &[u8]) -> Result<Vec<EmpowerRow>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("emPOWER ZIP codes were not JSON ({e})")))?;
    let arr = v
        .as_array()
        .ok_or_else(|| PhnError::Network("emPOWER ZIP response is not a list".into()))?;
    let rows: Vec<EmpowerRow> = arr
        .iter()
        .filter_map(|r| {
            let zip = r.get("area")?.as_str()?.to_string();
            Some(EmpowerRow {
                level: EmpowerLevel::Zip,
                geoid: zip.clone(),
                name: zip,
                county: r.get("parent").and_then(Value::as_str).map(str::to_string),
                medicare_beneficiaries: number(r.get("benes"))?,
                electricity_dependent: number(r.get("dme"))?,
            })
        })
        .collect();
    if rows.is_empty() {
        return Err(PhnError::Network(
            "emPOWER returned no Massachusetts ZIP codes".into(),
        ));
    }
    Ok(rows)
}

/// Parse the bundled snapshot CSV.
pub fn parse_snapshot(text: &str) -> Result<Vec<EmpowerRow>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let mut out = Vec::new();
    for rec in rdr.records() {
        let r = rec.map_err(|e| PhnError::Internal(format!("emPOWER snapshot: {e}")))?;
        let g = |i: usize| r.get(i).unwrap_or_default();
        let level = match g(0) {
            "nation" => EmpowerLevel::Nation,
            "state" => EmpowerLevel::State,
            "county" => EmpowerLevel::County,
            "zip" => EmpowerLevel::Zip,
            other => {
                return Err(PhnError::Internal(format!(
                    "emPOWER snapshot: unknown level {other}"
                )));
            }
        };
        out.push(EmpowerRow {
            level,
            geoid: g(1).to_string(),
            name: g(2).to_string(),
            county: Some(g(3).to_string()).filter(|c| !c.is_empty()),
            medicare_beneficiaries: g(4).parse().unwrap_or(0),
            electricity_dependent: g(5).parse().unwrap_or(0),
        });
    }
    Ok(out)
}

fn snapshot_rows(text: &str, level: EmpowerLevel) -> Option<Vec<u8>> {
    let rows = parse_snapshot(text).ok()?;
    let out: Vec<Value> = rows
        .iter()
        .filter(|r| r.level == level)
        .map(|r| match level {
            EmpowerLevel::Zip => json!({"area": r.geoid, "parent": r.county,
                "benes": r.medicare_beneficiaries.to_string(), "dme": r.electricity_dependent.to_string()}),
            _ => json!({"area": r.name, "countyFips": r.geoid,
                "benes": r.medicare_beneficiaries.to_string(), "dme": r.electricity_dependent.to_string()}),
        })
        .collect();
    serde_json::to_vec(&out).ok()
}

/// Rows plus where they came from.
#[derive(Debug, Clone)]
pub struct EmpowerFetch {
    pub rows: Vec<EmpowerRow>,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
}

type Parser = fn(&[u8]) -> Result<Vec<EmpowerRow>>;

async fn fetch_level(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
    level: EmpowerLevel,
) -> Result<EmpowerFetch> {
    let (url, parse): (&str, Parser) = match level {
        EmpowerLevel::Zip => (ZIPS_URL, parse_zips),
        _ => (COUNTIES_URL, parse_counties),
    };
    let store = store.clone();
    let fallback: SnapshotFallback = Box::new(move || {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE).ok()?;
        Some((snapshot_rows(&text, level)?, entry.retrieved))
    });
    let validate = move |b: &[u8]| -> Result<()> {
        expect_json(b)?;
        parse(b).map(|_| ())
    };
    let f = fetch_validated(http, cache, url, None, TTL_SECS, &validate, Some(fallback)).await?;
    Ok(EmpowerFetch {
        rows: parse(&f.body)?,
        mode: f.mode,
        as_of: f.as_of,
        fallback_reason: f.fallback_reason,
    })
}

/// Massachusetts counties (live → cache → snapshot).
pub async fn fetch_counties(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
) -> Result<EmpowerFetch> {
    fetch_level(http, cache, store, EmpowerLevel::County).await
}

/// Massachusetts ZIP codes (live → cache → snapshot).
pub async fn fetch_zips(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
) -> Result<EmpowerFetch> {
    fetch_level(http, cache, store, EmpowerLevel::Zip).await
}

/// The Massachusetts and U.S. totals from the snapshot (for comparison rates).
pub fn snapshot_totals(store: &SnapshotStore) -> Result<(Option<EmpowerRow>, Option<EmpowerRow>)> {
    let (text, _, _) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE)?;
    let rows = parse_snapshot(&text)?;
    let find = |l: EmpowerLevel| rows.iter().find(|r| r.level == l).cloned();
    Ok((find(EmpowerLevel::State), find(EmpowerLevel::Nation)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_parses_and_round_trips_through_the_api_shape() {
        let store = SnapshotStore::bundled();
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE).unwrap();
        let rows = parse_snapshot(&text).unwrap();
        assert_eq!(rows.len() as u64, entry.files[0].rows);
        let counties: Vec<_> = rows
            .iter()
            .filter(|r| r.level == EmpowerLevel::County)
            .collect();
        assert_eq!(counties.len(), 14);
        let back = parse_counties(&snapshot_rows(&text, EmpowerLevel::County).unwrap()).unwrap();
        assert_eq!(back.len(), 14);
        assert_eq!(back[0].geoid, "25001");
        let zips = parse_zips(&snapshot_rows(&text, EmpowerLevel::Zip).unwrap()).unwrap();
        assert!(zips.len() > 400);
        assert!(zips.iter().any(EmpowerRow::dme_suppressed));
        let (ma, us) = snapshot_totals(&store).unwrap();
        assert!(ma.unwrap().electricity_dependent > 10_000);
        assert!(us.unwrap().medicare_beneficiaries > 1_000_000);
    }

    #[test]
    fn parses_live_shapes_with_comma_numbers() {
        let c = parse_counties(
            br#"[{"area":"Hampshire","benes":"40,063","countyFips":"25015","dme":"1,179"}]"#,
        )
        .unwrap();
        assert_eq!(c[0].medicare_beneficiaries, 40_063);
        assert_eq!(c[0].electricity_dependent, 1_179);
        let z = parse_zips(
            br#"[{"area":"01002","benes":"4,994","feature":"{}","parent":"Hampshire","dme":"93"}]"#,
        )
        .unwrap();
        assert_eq!(z[0].county.as_deref(), Some("Hampshire"));
        assert!(parse_counties(b"[]").is_err());
    }
}
