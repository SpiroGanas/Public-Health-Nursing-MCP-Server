//! EPA AirNow: current and forecast Air Quality Index (AQI) by reporting
//! area.
//!
//! * Default (no key): the public **Reporting Area file**
//!   (`files.airnowtech.org/airnow/today/reportingarea.dat`), a pipe-delimited
//!   file updated several times an hour with yesterday's maximum AQI, the
//!   latest hourly observation, and agency forecasts for every reporting
//!   area. Format: AirNow "Fact Sheet: Reporting Area File"
//!   (files.airnowtech.org/airnow/docs/ReportingAreaFactSheet.pdf).
//! * Optional: with `PHN_AIRNOW_API_KEY`, the AirNow API (www.airnowapi.org)
//!   current-observation and forecast endpoints by latitude and longitude.
//!   The key is added to the request only, never cached or cited. **This
//!   path was not tested** (the API host was unreachable while it was
//!   built); when it fails the file product is used.
//!
//! AirNow data are preliminary and not for regulatory use.

use phn_core::{PhnError, Result, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apifetch::{api_key, expect_json, fetch_validated, reason};
use crate::cache::Cache;
use crate::http::HttpClient;

/// The public Reporting Area file.
pub const REPORTING_AREA_URL: &str = "https://files.airnowtech.org/airnow/today/reportingarea.dat";
/// The file's format description.
pub const FACT_SHEET_URL: &str =
    "https://files.airnowtech.org/airnow/docs/ReportingAreaFactSheet.pdf";
/// Optional API key variable.
pub const KEY_ENV: &str = "PHN_AIRNOW_API_KEY";
/// AirNow API base.
pub const API_BASE: &str = "https://www.airnowapi.org";
/// The file is updated at :10, :25, and :40; cache for 15 minutes.
pub const TTL_SECS: u64 = 15 * 60;

/// What a record holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// Agency forecast for a day.
    Forecast,
    /// Yesterday's observed maximum AQI.
    Yesterday,
    /// Latest hourly observation (NowCast).
    Observed,
}

/// One AQI record for a reporting area and pollutant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AqiRecord {
    pub kind: RecordKind,
    /// Date issued (MM/DD/YY in the file; as given).
    pub issue_date: String,
    /// Date the record applies to (YYYY-MM-DD).
    pub valid_date: String,
    /// Hour ending of an observation ("14:00"), with its time zone.
    pub valid_time: Option<String>,
    pub time_zone: Option<String>,
    /// True for the pollutant with the highest AQI.
    pub primary: bool,
    pub reporting_area: String,
    pub state: String,
    pub latitude: f64,
    pub longitude: f64,
    /// "OZONE", "PM2.5", "PM10", ...
    pub parameter: String,
    /// AQI value; forecasts sometimes give only a category.
    pub aqi: Option<u32>,
    /// AQI category name ("Good", "Moderate", ...).
    pub category: String,
    /// The agency declared an action day.
    pub action_day: bool,
    pub discussion: Option<String>,
    /// Forecasting or reporting agency.
    pub agency: Option<String>,
}

fn mmddyy(s: &str) -> Option<String> {
    let mut it = s.trim().split('/');
    let (m, d, y) = (it.next()?, it.next()?, it.next()?);
    let (m, d, y): (u32, u32, u32) = (m.parse().ok()?, d.parse().ok()?, y.parse().ok()?);
    let year = if y < 100 { 2000 + y } else { y };
    Some(format!("{year:04}-{m:02}-{d:02}"))
}

/// Parse the Reporting Area file, keeping rows for the given state codes
/// (all rows when `states` is empty).
pub fn parse_reporting_area(text: &str, states: &[&str]) -> Vec<AqiRecord> {
    let mut out = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 17 {
            continue;
        }
        let state = f[8].trim();
        if !states.is_empty() && !states.contains(&state) {
            continue;
        }
        let kind = match f[5].trim() {
            "F" => RecordKind::Forecast,
            "Y" => RecordKind::Yesterday,
            "O" => RecordKind::Observed,
            _ => continue,
        };
        let (Some(valid_date), Ok(lat), Ok(lon)) = (
            mmddyy(f[1]),
            f[9].trim().parse::<f64>(),
            f[10].trim().parse::<f64>(),
        ) else {
            continue;
        };
        let opt = |s: &str| Some(s.trim().to_string()).filter(|x| !x.is_empty());
        out.push(AqiRecord {
            kind,
            issue_date: mmddyy(f[0]).unwrap_or_else(|| f[0].trim().to_string()),
            valid_date,
            valid_time: opt(f[2]),
            time_zone: opt(f[3]),
            primary: f[6].trim() == "Y",
            reporting_area: f[7].trim().to_string(),
            state: state.to_string(),
            latitude: lat,
            longitude: lon,
            parameter: f[11].trim().to_string(),
            aqi: f[12].trim().parse().ok(),
            category: f[13].trim().to_string(),
            action_day: f[14].trim().eq_ignore_ascii_case("yes"),
            discussion: opt(f[15]),
            agency: opt(f[16..].join("|").as_str()),
        });
    }
    out
}

fn validate_file(body: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(body)
        .map_err(|_| PhnError::Network("the AirNow file is not text".into()))?;
    if text.trim_start().starts_with('<') {
        return Err(PhnError::Network(
            "AirNow returned a web page instead of the data file".into(),
        ));
    }
    if parse_reporting_area(text, &[]).is_empty() {
        return Err(PhnError::Network(
            "the AirNow reporting-area file has no records".into(),
        ));
    }
    Ok(())
}

/// Parse an AirNow API response (observations or forecasts).
pub fn parse_api(body: &[u8], kind: RecordKind) -> Result<Vec<AqiRecord>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("AirNow API response was not JSON ({e})")))?;
    let arr = v
        .as_array()
        .ok_or_else(|| PhnError::Network("AirNow API response is not a list".into()))?;
    let s = |r: &Value, k: &str| {
        r.get(k)
            .and_then(Value::as_str)
            .map(|x| x.trim().to_string())
    };
    let mut out: Vec<AqiRecord> = arr
        .iter()
        .filter_map(|r| {
            let date = match kind {
                RecordKind::Observed => s(r, "DateObserved"),
                _ => s(r, "DateForecast"),
            }?;
            let aqi = r
                .get("AQI")
                .and_then(Value::as_i64)
                .filter(|a| *a >= 0)
                .map(|a| a as u32);
            Some(AqiRecord {
                kind,
                issue_date: s(r, "DateIssue").unwrap_or_else(|| date.clone()),
                valid_date: date.trim().to_string(),
                valid_time: r
                    .get("HourObserved")
                    .and_then(Value::as_i64)
                    .map(|h| format!("{h:02}:00")),
                time_zone: s(r, "LocalTimeZone"),
                primary: false,
                reporting_area: s(r, "ReportingArea")?,
                state: s(r, "StateCode").unwrap_or_default(),
                latitude: r
                    .get("Latitude")
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                longitude: r
                    .get("Longitude")
                    .and_then(Value::as_f64)
                    .unwrap_or_default(),
                parameter: s(r, "ParameterName")?.to_ascii_uppercase(),
                aqi,
                category: r
                    .get("Category")
                    .and_then(|c| c.get("Name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                action_day: r.get("ActionDay").and_then(Value::as_bool).unwrap_or(false),
                discussion: s(r, "Discussion").filter(|d| !d.is_empty()),
                agency: None,
            })
        })
        .collect();
    // Mark the highest AQI per area and date as primary.
    let mut keys: Vec<(String, String)> = out
        .iter()
        .map(|r| (r.reporting_area.clone(), r.valid_date.clone()))
        .collect();
    keys.sort();
    keys.dedup();
    for (area, day) in keys {
        if let Some(i) = out
            .iter()
            .enumerate()
            .filter(|(_, r)| r.reporting_area == area && r.valid_date == day)
            .max_by_key(|(_, r)| r.aqi.unwrap_or(0))
            .map(|(i, _)| i)
        {
            out[i].primary = true;
        }
    }
    Ok(out)
}

/// Records plus where they came from.
#[derive(Debug, Clone)]
pub struct AirNowFetch {
    pub records: Vec<AqiRecord>,
    pub mode: SourceMode,
    pub as_of: String,
    /// "AirNow reporting-area file" or "AirNow API".
    pub product: String,
    pub notes: Vec<String>,
}

/// Massachusetts records from the Reporting Area file (live → cache).
pub async fn fetch_file(
    http: &HttpClient,
    cache: Option<&Cache>,
    states: &[&str],
) -> Result<AirNowFetch> {
    let f = fetch_validated(
        http,
        cache,
        REPORTING_AREA_URL,
        None,
        TTL_SECS,
        &validate_file,
        None,
    )
    .await?;
    let text = String::from_utf8_lossy(&f.body);
    let mut notes = Vec::new();
    if let Some(r) = f.fallback_reason {
        notes.push(r);
    }
    Ok(AirNowFetch {
        records: parse_reporting_area(&text, states),
        mode: f.mode,
        as_of: f.as_of,
        product: "AirNow reporting-area file".into(),
        notes,
    })
}

/// With `PHN_AIRNOW_API_KEY`: current observations and forecasts near a point
/// from the AirNow API; otherwise (or when the API fails) the file product.
pub async fn fetch_near(
    http: &HttpClient,
    cache: Option<&Cache>,
    lat: f64,
    lon: f64,
) -> Result<AirNowFetch> {
    if let Some(key) = api_key(KEY_ENV) {
        match fetch_api(http, cache, lat, lon, &key).await {
            Ok(f) => return Ok(f),
            Err(e) => {
                let mut f = fetch_file(http, cache, &["MA"]).await?;
                f.notes.push(format!(
                    "The AirNow API (with PHN_AIRNOW_API_KEY) failed ({}); the public \
                     reporting-area file was used instead.",
                    reason(&e)
                ));
                return Ok(f);
            }
        }
    }
    fetch_file(http, cache, &["MA"]).await
}

async fn fetch_api(
    http: &HttpClient,
    cache: Option<&Cache>,
    lat: f64,
    lon: f64,
    key: &str,
) -> Result<AirNowFetch> {
    let obs_url = format!(
        "{API_BASE}/aq/observation/latLong/current/?format=application/json&latitude={lat:.4}&longitude={lon:.4}&distance=25"
    );
    let fc_url = format!(
        "{API_BASE}/aq/forecast/latLong/?format=application/json&latitude={lat:.4}&longitude={lon:.4}&distance=25"
    );
    let obs = fetch_validated(
        http,
        cache,
        &obs_url,
        Some(("API_KEY", key)),
        TTL_SECS,
        &expect_json,
        None,
    )
    .await?;
    let fc = fetch_validated(
        http,
        cache,
        &fc_url,
        Some(("API_KEY", key)),
        TTL_SECS,
        &expect_json,
        None,
    )
    .await?;
    let mut records = parse_api(&obs.body, RecordKind::Observed)?;
    records.extend(parse_api(&fc.body, RecordKind::Forecast)?);
    let mode = if obs.mode == SourceMode::Live && fc.mode == SourceMode::Live {
        SourceMode::Live
    } else {
        SourceMode::Cache
    };
    Ok(AirNowFetch {
        records,
        mode,
        as_of: obs.as_of,
        product: "AirNow API".into(),
        notes: vec![
            "Data from the AirNow API using PHN_AIRNOW_API_KEY (this path was not tested when \
             the tool was built)."
                .into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = include_str!("../tests/fixtures/airnow_reportingarea_ma.dat");

    #[test]
    fn parses_massachusetts_reporting_areas() {
        let recs = parse_reporting_area(FILE, &["MA"]);
        let areas: std::collections::BTreeSet<&str> =
            recs.iter().map(|r| r.reporting_area.as_str()).collect();
        assert!(areas.contains("Boston Metro"));
        assert!(areas.contains("Cape Cod and Islands"));
        assert_eq!(areas.len(), 11);
        let obs = recs
            .iter()
            .find(|r| {
                r.reporting_area == "Boston Metro" && r.kind == RecordKind::Observed && r.primary
            })
            .unwrap();
        assert!(obs.aqi.is_some());
        assert_eq!(obs.time_zone.as_deref(), Some("EST"));
        let fc = recs
            .iter()
            .find(|r| r.reporting_area == "Boston Metro" && r.kind == RecordKind::Forecast)
            .unwrap();
        assert!(fc.aqi.is_none());
        assert_eq!(fc.category, "Good");
        assert!(fc.agency.as_deref().unwrap().contains("Massachusetts"));
        assert!(validate_file(FILE.as_bytes()).is_ok());
        assert!(validate_file(b"<html>").is_err());
        assert_eq!(mmddyy("11/08/26").as_deref(), Some("2026-11-08"));
    }

    #[test]
    fn parses_api_shapes_and_marks_primary() {
        let body = br#"[{"DateObserved":"2026-11-08 ","HourObserved":14,"LocalTimeZone":"EST",
            "ReportingArea":"Boston Metro","StateCode":"MA","Latitude":42.3892,"Longitude":-71.1317,
            "ParameterName":"O3","AQI":15,"Category":{"Number":1,"Name":"Good"}},
            {"DateObserved":"2026-11-08 ","HourObserved":14,"LocalTimeZone":"EST",
            "ReportingArea":"Boston Metro","StateCode":"MA","Latitude":42.3892,"Longitude":-71.1317,
            "ParameterName":"PM2.5","AQI":44,"Category":{"Number":1,"Name":"Good"}}]"#;
        let r = parse_api(body, RecordKind::Observed).unwrap();
        assert_eq!(r.len(), 2);
        assert!(r[1].primary && !r[0].primary);
        assert_eq!(r[0].valid_time.as_deref(), Some("14:00"));
    }
}
