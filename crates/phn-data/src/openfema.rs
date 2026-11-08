//! OpenFEMA disaster declarations for Massachusetts
//! (`DisasterDeclarationsSummaries` v2, www.fema.gov/api/open).
//!
//! One row per declaration and designated area (county, tribal area, or
//! statewide). The live list is cached for a day; the bundled snapshot
//! (`fema-declarations-ma`) is the fallback.

use phn_core::{PhnError, Result, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::apifetch::{expect_json, fetch_validated};
use crate::cache::Cache;
use crate::fetch::SnapshotFallback;
use crate::http::HttpClient;
use crate::snapshots::SnapshotStore;

/// Snapshot id and file.
pub const SNAPSHOT_ID: &str = "fema-declarations-ma";
pub const SNAPSHOT_FILE: &str = "fema_declarations_ma.csv";
/// Dataset documentation.
pub const DATASET_PAGE: &str =
    "https://www.fema.gov/openfema-data-page/disaster-declarations-summaries-v2";
/// Cached for a day.
pub const TTL_SECS: u64 = 24 * 3600;

const FIELDS: &str = "disasterNumber,declarationType,declarationDate,incidentType,declarationTitle,\
designatedArea,fipsCountyCode,incidentBeginDate,incidentEndDate,ihProgramDeclared,\
iaProgramDeclared,paProgramDeclared,hmProgramDeclared,femaDeclarationString,lastRefresh";

/// The live query: every Massachusetts declaration, newest first.
pub fn massachusetts_url() -> String {
    format!(
        "https://www.fema.gov/api/open/v2/DisasterDeclarationsSummaries?$filter=state%20eq%20%27MA%27\
         &$orderby=declarationDate%20desc,disasterNumber%20desc&$top=5000&$select={FIELDS}"
    )
}

/// One declaration for one designated area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Declaration {
    /// FEMA's id, e.g. "DR-4780-MA".
    pub id: String,
    pub disaster_number: u32,
    /// DR (major disaster), EM (emergency), or FM (fire management).
    pub declaration_type: String,
    /// YYYY-MM-DD.
    pub declaration_date: String,
    /// FEMA incident type, e.g. "Flood", "Severe Storm", "Snowstorm", "Biological".
    pub incident_type: String,
    pub title: String,
    /// County ("Bristol (County)..."), tribal area, or "Statewide".
    pub designated_area: String,
    /// Five-digit county FIPS ("25005"); None for statewide and tribal areas.
    pub county_fips: Option<String>,
    pub incident_begin: Option<String>,
    pub incident_end: Option<String>,
    /// Programs declared: IH (individuals and households), IA, PA (public assistance), HM (hazard mitigation).
    pub programs: Vec<String>,
}

fn date(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(|s| s.chars().take(10).collect::<String>())
        .filter(|s| s.len() == 10)
}

/// Parse an OpenFEMA JSON body.
pub fn parse_json(body: &[u8]) -> Result<(Vec<Declaration>, Option<String>)> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("OpenFEMA response was not JSON ({e})")))?;
    let rows = v
        .get("DisasterDeclarationsSummaries")
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network("OpenFEMA response has no declarations".into()))?;
    let mut out = Vec::with_capacity(rows.len());
    let mut refreshed: Option<String> = None;
    for r in rows {
        let get = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let flag = |k: &str| r.get(k).and_then(Value::as_bool).unwrap_or(false);
        let Some(number) = r.get("disasterNumber").and_then(Value::as_u64) else {
            continue;
        };
        let county = get("fipsCountyCode");
        let mut programs = Vec::new();
        for (k, l) in [
            ("ihProgramDeclared", "IH"),
            ("iaProgramDeclared", "IA"),
            ("paProgramDeclared", "PA"),
            ("hmProgramDeclared", "HM"),
        ] {
            if flag(k) {
                programs.push(l.to_string());
            }
        }
        if let Some(d) = date(r.get("lastRefresh"))
            && refreshed.as_ref().is_none_or(|x| &d > x)
        {
            refreshed = Some(d);
        }
        let dtype = get("declarationType");
        out.push(Declaration {
            id: {
                let s = get("femaDeclarationString");
                if s.is_empty() {
                    format!("{dtype}-{number}-MA")
                } else {
                    s
                }
            },
            disaster_number: number as u32,
            declaration_type: dtype,
            declaration_date: date(r.get("declarationDate")).unwrap_or_default(),
            incident_type: get("incidentType"),
            title: get("declarationTitle"),
            designated_area: get("designatedArea"),
            county_fips: (county.len() == 3 && county != "000").then(|| format!("25{county}")),
            incident_begin: date(r.get("incidentBeginDate")),
            incident_end: date(r.get("incidentEndDate")),
            programs,
        });
    }
    Ok((out, refreshed))
}

/// The bundled CSV converted to the OpenFEMA JSON shape (for the fallback chain).
pub fn snapshot_as_json(csv_text: &str) -> Option<Vec<u8>> {
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let r = rec.ok()?;
        let g = |i: usize| r.get(i).unwrap_or_default().to_string();
        let programs = g(10);
        let has = |p: &str| programs.split(';').any(|x| x == p);
        let ts = |d: String| {
            if d.is_empty() {
                Value::Null
            } else {
                Value::String(format!("{d}T00:00:00.000Z"))
            }
        };
        rows.push(json!({
            "femaDeclarationString": g(0),
            "disasterNumber": g(1).parse::<u64>().ok()?,
            "declarationType": g(2),
            "declarationDate": ts(g(3)),
            "incidentType": g(4),
            "declarationTitle": g(5),
            "designatedArea": g(6),
            "fipsCountyCode": g(7),
            "incidentBeginDate": ts(g(8)),
            "incidentEndDate": ts(g(9)),
            "ihProgramDeclared": has("IH"),
            "iaProgramDeclared": has("IA"),
            "paProgramDeclared": has("PA"),
            "hmProgramDeclared": has("HM"),
        }));
    }
    serde_json::to_vec(&json!({ "DisasterDeclarationsSummaries": rows })).ok()
}

/// Declarations plus where they came from.
#[derive(Debug, Clone)]
pub struct DeclarationsFetch {
    pub declarations: Vec<Declaration>,
    pub mode: SourceMode,
    /// Retrieval time (live or cache) or the snapshot date.
    pub as_of: String,
    /// OpenFEMA's latest refresh date in the data, when present.
    pub refreshed: Option<String>,
    pub fallback_reason: Option<String>,
}

/// Fetch every Massachusetts declaration (live → cache → bundled snapshot).
pub async fn fetch_massachusetts(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
) -> Result<DeclarationsFetch> {
    let store = store.clone();
    let fallback: SnapshotFallback = Box::new(move || {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE).ok()?;
        Some((snapshot_as_json(&text)?, entry.retrieved))
    });
    let fetched = fetch_validated(
        http,
        cache,
        &massachusetts_url(),
        None,
        TTL_SECS,
        &validate,
        Some(fallback),
    )
    .await?;
    let (declarations, refreshed) = parse_json(&fetched.body)?;
    Ok(DeclarationsFetch {
        declarations,
        mode: fetched.mode,
        as_of: fetched.as_of,
        refreshed,
        fallback_reason: fetched.fallback_reason,
    })
}

fn validate(body: &[u8]) -> Result<()> {
    expect_json(body)?;
    parse_json(body).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_bundled_snapshot_through_the_json_shape() {
        let store = SnapshotStore::bundled();
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE).unwrap();
        let body = snapshot_as_json(&text).unwrap();
        let (rows, _) = parse_json(&body).unwrap();
        assert_eq!(rows.len() as u64, entry.files[0].rows);
        // DR-4780-MA: severe storms and flooding, September 2023, Bristol and Worcester.
        let bristol = rows
            .iter()
            .find(|d| d.disaster_number == 4780 && d.county_fips.as_deref() == Some("25005"))
            .unwrap();
        assert_eq!(bristol.declaration_type, "DR");
        assert_eq!(bristol.incident_type, "Flood");
        assert_eq!(bristol.incident_begin.as_deref(), Some("2023-09-11"));
        assert!(bristol.programs.contains(&"IH".to_string()));
        assert!(rows.iter().any(|d| d.county_fips.is_none()));
    }

    #[test]
    fn parses_live_json() {
        let body = br#"{"metadata":{},"DisasterDeclarationsSummaries":[{"disasterNumber":3599,
            "declarationType":"EM","declarationDate":"2023-09-15T00:00:00.000Z","incidentType":"Hurricane",
            "declarationTitle":"HURRICANE LEE","designatedArea":"Barnstable (County)","fipsCountyCode":"001",
            "incidentBeginDate":"2023-09-15T00:00:00.000Z","incidentEndDate":"2023-09-17T00:00:00.000Z",
            "ihProgramDeclared":false,"iaProgramDeclared":false,"paProgramDeclared":true,"hmProgramDeclared":false,
            "femaDeclarationString":"EM-3599-MA","lastRefresh":"2026-11-06T12:27:53.589Z"}]}"#;
        let (rows, refreshed) = parse_json(body).unwrap();
        assert_eq!(rows[0].county_fips.as_deref(), Some("25001"));
        assert_eq!(rows[0].programs, vec!["PA"]);
        assert_eq!(refreshed.as_deref(), Some("2026-11-06"));
        assert!(massachusetts_url().contains("state%20eq%20%27MA%27"));
        assert!(validate(b"<html>").is_err());
    }
}
