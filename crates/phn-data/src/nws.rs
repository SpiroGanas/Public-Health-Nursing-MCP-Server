//! National Weather Service API (api.weather.gov): active alerts for
//! Massachusetts and point forecasts.
//!
//! The API is public and keyless. NWS asks every client to send a
//! descriptive User-Agent; [`crate::http::HttpClient`] sends the server's
//! name, version, and project URL (no personal email). Alerts are cached for
//! five minutes and are never served from a bundled snapshot or from a cached
//! copy more than an hour old: an old alert is worse than none, so when the
//! live API cannot be reached the tools say so.

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apifetch::{FetchOptions, expect_json, fetch_validated, fetch_with_options, reason};
use crate::cache::Cache;
use crate::fetch::Fetched;
use crate::http::HttpClient;

/// Active alerts for Massachusetts (land and adjacent marine zones).
pub const ALERTS_ACTIVE_MA: &str = "https://api.weather.gov/alerts/active?area=MA";
/// NWS API documentation.
pub const API_DOCS: &str = "https://www.weather.gov/documentation/services-web-api";
/// Alerts are re-fetched after five minutes.
pub const ALERTS_TTL_SECS: u64 = 300;
/// When the live feed cannot be reached, a cached copy of the alerts older
/// than this (60 minutes) is not shown.
pub const ALERTS_MAX_STALE_SECS: u64 = 3600;
/// The only host forecasts are fetched from.
pub const API_HOST_PREFIX: &str = "https://api.weather.gov/";
/// Forecasts are re-fetched after an hour.
pub const FORECAST_TTL_SECS: u64 = 3600;
/// A point's forecast office and grid rarely change.
pub const POINTS_TTL_SECS: u64 = 30 * 24 * 3600;

/// One NWS alert (CAP message) as returned by the API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NwsAlert {
    /// CAP identifier.
    pub id: String,
    /// Event type, e.g. "Heat Advisory", "Flood Warning".
    pub event: String,
    pub headline: Option<String>,
    /// Extreme, Severe, Moderate, Minor, or Unknown.
    pub severity: String,
    /// Immediate, Expected, Future, Past, or Unknown.
    pub urgency: String,
    /// Observed, Likely, Possible, Unlikely, or Unknown.
    pub certainty: String,
    /// Actual, Exercise, System, Test, or Draft.
    pub status: String,
    /// Alert, Update, or Cancel.
    pub message_type: String,
    pub sent: Option<String>,
    pub effective: Option<String>,
    pub onset: Option<String>,
    pub expires: Option<String>,
    pub ends: Option<String>,
    /// Zones or counties in plain words.
    pub area_desc: String,
    /// Five-digit county FIPS codes from the SAME geocodes ("25005").
    pub county_fips: Vec<String>,
    /// UGC zone codes ("MAZ005", "MAC023", "ANZ250").
    pub zones: Vec<String>,
    /// Issuing office, e.g. "NWS Boston/Norton MA".
    pub sender_name: Option<String>,
    pub description: Option<String>,
    pub instruction: Option<String>,
    /// CAP response type (Shelter, Evacuate, Prepare, Execute, Avoid, Monitor, ...).
    pub response: Option<String>,
}

impl NwsAlert {
    /// Link to the alert in the API.
    pub fn url(&self) -> String {
        format!("https://api.weather.gov/alerts/{}", self.id)
    }

    /// True when every zone is a marine zone (UGC codes starting "AN" for the
    /// western Atlantic coastal waters).
    pub fn is_marine_only(&self) -> bool {
        !self.zones.is_empty() && self.zones.iter().all(|z| z.starts_with("AN"))
    }
}

/// Parsed alerts plus the feed's update time.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertFeed {
    pub alerts: Vec<NwsAlert>,
    pub updated: Option<String>,
    pub title: Option<String>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Parse an `/alerts` GeoJSON body.
pub fn parse_alerts(body: &[u8]) -> Result<AlertFeed> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("NWS alerts were not valid JSON ({e})")))?;
    let features = v
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network("NWS alerts response has no features".into()))?;
    let mut alerts = Vec::new();
    for f in features {
        let Some(p) = f.get("properties") else {
            continue;
        };
        let Some(id) = s(p, "id") else { continue };
        let geocode = p.get("geocode");
        let mut county_fips: Vec<String> = strings(geocode.and_then(|g| g.get("SAME")))
            .into_iter()
            .filter_map(|c| {
                // SAME codes are 6 digits: a subdivision digit (0) + state + county.
                let c = c.trim();
                (c.len() == 6 && c.chars().all(|d| d.is_ascii_digit())).then(|| c[1..].to_string())
            })
            .collect();
        county_fips.sort();
        county_fips.dedup();
        alerts.push(NwsAlert {
            id,
            event: s(p, "event").unwrap_or_else(|| "Unknown event".into()),
            headline: s(p, "headline"),
            severity: s(p, "severity").unwrap_or_else(|| "Unknown".into()),
            urgency: s(p, "urgency").unwrap_or_else(|| "Unknown".into()),
            certainty: s(p, "certainty").unwrap_or_else(|| "Unknown".into()),
            status: s(p, "status").unwrap_or_else(|| "Unknown".into()),
            message_type: s(p, "messageType").unwrap_or_else(|| "Alert".into()),
            sent: s(p, "sent"),
            effective: s(p, "effective"),
            onset: s(p, "onset"),
            expires: s(p, "expires"),
            ends: s(p, "ends"),
            area_desc: s(p, "areaDesc").unwrap_or_default(),
            county_fips,
            zones: strings(geocode.and_then(|g| g.get("UGC"))),
            sender_name: s(p, "senderName"),
            description: s(p, "description"),
            instruction: s(p, "instruction"),
            response: s(p, "response"),
        });
    }
    Ok(AlertFeed {
        alerts,
        updated: s(&v, "updated"),
        title: s(&v, "title"),
    })
}

/// Fetch the active Massachusetts alerts: live, then a cached copy no more
/// than [`ALERTS_MAX_STALE_SECS`] old; never an older copy or a snapshot.
pub async fn fetch_active_alerts(http: &HttpClient, cache: Option<&Cache>) -> Result<Fetched> {
    let options = FetchOptions {
        max_stale_secs: Some(ALERTS_MAX_STALE_SECS),
        ..FetchOptions::default()
    };
    fetch_with_options(
        http,
        cache,
        ALERTS_ACTIVE_MA,
        ALERTS_TTL_SECS,
        &validate_features,
        None,
        options,
    )
    .await
    .map_err(|f| {
        PhnError::Network(format!(
            "the NWS alert feed could not be reached ({}); alerts are not shown from a copy \
             more than 60 minutes old",
            reason(&f.error)
        ))
    })
}

fn validate_features(body: &[u8]) -> Result<()> {
    expect_json(body)?;
    let v: Value = serde_json::from_slice(body).map_err(|e| PhnError::Network(e.to_string()))?;
    if v.get("features").and_then(Value::as_array).is_some() {
        Ok(())
    } else {
        Err(PhnError::Network(
            "the NWS response has no list of alerts".into(),
        ))
    }
}

/// The `/points` URL for a location (4 decimals, as NWS requires).
pub fn points_url(lat: f64, lon: f64) -> String {
    format!("https://api.weather.gov/points/{lat:.4},{lon:.4}")
}

/// Forecast URL, grid id, and nearest named place from a `/points` body.
#[derive(Debug, Clone, PartialEq)]
pub struct PointInfo {
    pub forecast_url: String,
    pub office: Option<String>,
    pub nearest_place: Option<String>,
}

/// Parse a `/points` body.
pub fn parse_points(body: &[u8]) -> Result<PointInfo> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("NWS points response was not JSON ({e})")))?;
    let p = v
        .get("properties")
        .ok_or_else(|| PhnError::Network("NWS points response has no properties".into()))?;
    let forecast_url = s(p, "forecast")
        .ok_or_else(|| PhnError::Network("NWS points response has no forecast link".into()))?;
    let nearest_place = p
        .get("relativeLocation")
        .and_then(|r| r.get("properties"))
        .and_then(|r| Some(format!("{}, {}", s(r, "city")?, s(r, "state")?)));
    Ok(PointInfo {
        forecast_url,
        office: s(p, "gridId"),
        nearest_place,
    })
}

/// True when a URL points at the NWS API host over HTTPS.
pub fn is_nws_api_url(url: &str) -> bool {
    url.starts_with(API_HOST_PREFIX)
}

/// One forecast period (12 hours).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ForecastPeriod {
    /// "Tonight", "Monday", ...
    pub name: String,
    pub start: String,
    pub is_daytime: bool,
    /// Temperature in °F (high for daytime periods, low for night).
    pub temperature_f: Option<f64>,
    pub short_forecast: String,
    /// Chance of precipitation, percent.
    pub precipitation_chance: Option<f64>,
}

/// A parsed forecast.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub periods: Vec<ForecastPeriod>,
    pub updated: Option<String>,
}

/// Parse a `/gridpoints/.../forecast` body.
pub fn parse_forecast(body: &[u8]) -> Result<Forecast> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("NWS forecast was not JSON ({e})")))?;
    let p = v
        .get("properties")
        .ok_or_else(|| PhnError::Network("NWS forecast has no properties".into()))?;
    let periods = p
        .get("periods")
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network("NWS forecast has no periods".into()))?
        .iter()
        .map(|x| {
            let mut temp = x.get("temperature").and_then(Value::as_f64);
            if s(x, "temperatureUnit").as_deref() == Some("C") {
                temp = temp.map(|c| c * 9.0 / 5.0 + 32.0);
            }
            ForecastPeriod {
                name: s(x, "name").unwrap_or_default(),
                start: s(x, "startTime").unwrap_or_default(),
                is_daytime: x.get("isDaytime").and_then(Value::as_bool).unwrap_or(true),
                temperature_f: temp,
                short_forecast: s(x, "shortForecast").unwrap_or_default(),
                precipitation_chance: x
                    .get("probabilityOfPrecipitation")
                    .and_then(|p| p.get("value"))
                    .and_then(Value::as_f64),
            }
        })
        .collect();
    Ok(Forecast {
        periods,
        updated: s(p, "updateTime").or_else(|| s(p, "generatedAt")),
    })
}

/// Fetch the 7-day forecast for a point (points lookup, then the forecast).
pub async fn fetch_forecast(
    http: &HttpClient,
    cache: Option<&Cache>,
    lat: f64,
    lon: f64,
) -> Result<(PointInfo, Forecast, Fetched)> {
    let points = fetch_validated(
        http,
        cache,
        &points_url(lat, lon),
        None,
        POINTS_TTL_SECS,
        &expect_json,
        None,
    )
    .await?;
    let info = parse_points(&points.body)?;
    // The forecast link comes from the response body; follow it only to NWS.
    if !is_nws_api_url(&info.forecast_url) {
        return Err(PhnError::Network(
            "the NWS points response links to a forecast outside api.weather.gov; it was not \
             fetched"
                .into(),
        ));
    }
    let fc = fetch_validated(
        http,
        cache,
        &info.forecast_url,
        None,
        FORECAST_TTL_SECS,
        &expect_json,
        None,
    )
    .await?;
    let forecast = parse_forecast(&fc.body)?;
    Ok((info, forecast, fc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALERTS: &str = include_str!("../tests/fixtures/nws_alerts_ma.json");
    const POINTS: &str = include_str!("../tests/fixtures/nws_points_boston.json");
    const FORECAST: &str = include_str!("../tests/fixtures/nws_forecast_boston.json");

    #[test]
    fn parses_alerts_with_counties_and_zones() {
        let feed = parse_alerts(ALERTS.as_bytes()).unwrap();
        assert!(feed.alerts.len() >= 5);
        let ffw = feed
            .alerts
            .iter()
            .find(|a| a.event == "Flash Flood Warning")
            .unwrap();
        assert_eq!(ffw.county_fips, vec!["25005", "25023"]);
        assert!(ffw.zones.contains(&"MAC005".to_string()));
        assert_eq!(ffw.severity, "Severe");
        assert!(
            ffw.url()
                .starts_with("https://api.weather.gov/alerts/urn:oid:")
        );
        let watch = feed
            .alerts
            .iter()
            .find(|a| a.event == "Flood Watch")
            .unwrap();
        assert!(watch.zones.iter().any(|z| z.starts_with("MAZ")));
        assert!(!watch.is_marine_only());
    }

    #[test]
    fn rejects_bodies_without_features() {
        assert!(parse_alerts(b"{\"type\":\"FeatureCollection\"}").is_err());
        assert!(validate_features(b"<html></html>").is_err());
        assert!(validate_features(b"{\"features\":[]}").is_ok());
    }

    #[test]
    fn forecast_links_must_stay_on_the_nws_host() {
        assert!(is_nws_api_url(
            "https://api.weather.gov/gridpoints/BOX/71,90/forecast"
        ));
        assert!(!is_nws_api_url(
            "https://api.weather.gov.example.org/gridpoints/BOX/71,90/forecast"
        ));
        assert!(!is_nws_api_url(
            "http://api.weather.gov/gridpoints/BOX/71,90/forecast"
        ));
        assert!(!is_nws_api_url("https://example.org/forecast"));
    }

    #[tokio::test]
    async fn a_points_link_to_another_host_is_not_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&phn_core::Config::for_root(dir.path())).unwrap();
        let cache = Cache::in_memory().unwrap();
        let body = br#"{"properties":{"forecast":"https://example.org/forecast","gridId":"BOX"}}"#;
        cache
            .put(&points_url(42.36, -71.06), body, POINTS_TTL_SECS)
            .unwrap();
        let err = fetch_forecast(&http, Some(&cache), 42.36, -71.06)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("outside api.weather.gov"), "{err}");
    }

    #[tokio::test]
    async fn alerts_are_never_served_from_an_old_cached_copy() {
        // Offline (or the feed is down): only a recent cached copy may be used.
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&phn_core::Config::for_root(dir.path())).unwrap();
        let cache = Cache::in_memory().unwrap();
        let now = chrono::Utc::now();
        cache
            .put_at(
                ALERTS_ACTIVE_MA,
                ALERTS.as_bytes(),
                ALERTS_TTL_SECS,
                now - chrono::Duration::days(7),
            )
            .unwrap();
        let err = fetch_active_alerts(&http, Some(&cache))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("alert feed could not be reached"), "{err}");
        assert!(err.contains("too old"), "{err}");

        cache
            .put_at(
                ALERTS_ACTIVE_MA,
                ALERTS.as_bytes(),
                ALERTS_TTL_SECS,
                now - chrono::Duration::minutes(20),
            )
            .unwrap();
        let f = fetch_active_alerts(&http, Some(&cache)).await.unwrap();
        assert_eq!(f.mode, phn_core::SourceMode::Cache);
        assert!(f.fallback_reason.is_some());
    }

    #[test]
    fn parses_points_and_forecast() {
        let p = parse_points(POINTS.as_bytes()).unwrap();
        assert!(p.forecast_url.ends_with("/forecast"));
        assert_eq!(p.office.as_deref(), Some("BOX"));
        assert_eq!(p.nearest_place.as_deref(), Some("Boston, MA"));
        let f = parse_forecast(FORECAST.as_bytes()).unwrap();
        assert!(f.periods.len() >= 10);
        assert!(f.periods[0].temperature_f.is_some());
        assert!(f.updated.is_some());
        assert_eq!(
            points_url(42.36012, -71.05891),
            "https://api.weather.gov/points/42.3601,-71.0589"
        );
    }
}
