//! SAMHSA FindTreatment.gov locator API (substance use treatment facilities).
//!
//! The public API (`/locator/exportsAsJson/v2`, documented in SAMHSA's
//! *FindTreatment.gov API Documentation* v1.12, August 2026) returns
//! facilities near a point (`sAddr` latitude,longitude; `limitType=2`;
//! `limitValue` in metres). Each facility lists its services as
//! `{f1: category name, f2: category code, f3: "value; value"}`.
//!
//! [`search`] runs the live → cache → snapshot chain. The bundled snapshot is a
//! statewide copy (`findtreatment_ma.csv`, built by
//! `xtask/scripts/build_findtreatment.py`), so distances are computed here
//! when it is used.

use std::collections::BTreeMap;

use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::Cache;
use crate::fetch::{SnapshotFallback, fetch_with_fallback};
use crate::http::HttpClient;

/// Snapshot id and file.
pub const SNAPSHOT_ID: &str = "samhsa-findtreatment-ma";
pub const SNAPSHOT_FILE: &str = "findtreatment_ma.csv";
/// Listings change slowly; cache for a day.
pub const TTL_SECS: u64 = 24 * 3600;
const METRES_PER_MILE: f64 = 1609.344;

/// One treatment facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Facility {
    pub name: String,
    /// Second name line (program or site), when given.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name2: String,
    pub address: String,
    pub city: String,
    pub zip: String,
    pub phone: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub intake_phone: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub website: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Distance from the search point.
    pub miles: f64,
    /// Service values by category code (TC, SET, OT, OM, PAY, SL, ...).
    pub services: BTreeMap<String, Vec<String>>,
}

impl Facility {
    /// True when any value in category `code` contains `needle` (case-insensitive).
    pub fn has(&self, code: &str, needle: &str) -> bool {
        let n = needle.to_lowercase();
        self.services
            .get(code)
            .is_some_and(|v| v.iter().any(|s| s.to_lowercase().contains(&n)))
    }
}

/// The search URL for facilities within `miles` of a point.
pub fn search_url(lat: f64, lng: f64, miles: f64) -> String {
    format!(
        "https://findtreatment.gov/locator/exportsAsJson/v2?sAddr=%22{lat:.5},{lng:.5}%22&limitType=2&limitValue={:.0}&sType=sa&pageSize=2000&page=1&sort=0",
        miles * METRES_PER_MILE
    )
}

/// Great-circle distance in miles.
pub fn haversine_miles(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 3958.7613;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lng2 - lng1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Convert Massachusetts State Plane (NAD83 Mainland, EPSG:26986, metres) to
/// latitude and longitude (inverse Lambert conformal conic, Snyder 1987).
pub fn state_plane_to_lat_lng(x: f64, y: f64) -> (f64, f64) {
    let a = 6_378_137.0_f64;
    let f: f64 = 1.0 / 298.257_222_101;
    let e = (2.0 * f - f * f).sqrt();
    let (phi1, phi2, phi0, lam0) = (
        (42.0 + 41.0 / 60.0_f64).to_radians(),
        (41.0 + 43.0 / 60.0_f64).to_radians(),
        41.0_f64.to_radians(),
        (-71.5_f64).to_radians(),
    );
    let (x0, y0) = (200_000.0, 750_000.0);
    let m = |p: f64| p.cos() / (1.0 - e * e * p.sin().powi(2)).sqrt();
    let t = |p: f64| {
        (std::f64::consts::FRAC_PI_4 - p / 2.0).tan()
            / ((1.0 - e * p.sin()) / (1.0 + e * p.sin())).powf(e / 2.0)
    };
    let n = (m(phi1).ln() - m(phi2).ln()) / (t(phi1).ln() - t(phi2).ln());
    let big_f = m(phi1) / (n * t(phi1).powf(n));
    let rho0 = a * big_f * t(phi0).powf(n);
    let dx = x - x0;
    let dy = rho0 - (y - y0);
    let rho = (dx * dx + dy * dy).sqrt().copysign(n);
    let tt = (rho / (a * big_f)).powf(1.0 / n);
    let theta = dx.atan2(dy);
    let lam = theta / n + lam0;
    let mut phi = std::f64::consts::FRAC_PI_2 - 2.0 * tt.atan();
    for _ in 0..10 {
        let s = e * phi.sin();
        phi =
            std::f64::consts::FRAC_PI_2 - 2.0 * (tt * ((1.0 - s) / (1.0 + s)).powf(e / 2.0)).atan();
    }
    (phi.to_degrees(), lam.to_degrees())
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn split_values(v: &str) -> Vec<String> {
    v.split("; ")
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse the live API response.
pub fn parse_json(body: &[u8]) -> Result<Vec<Facility>> {
    let v: Value = serde_json::from_slice(body).map_err(|_| {
        PhnError::Network(format!(
            "FindTreatment.gov returned an unexpected response: {}",
            String::from_utf8_lossy(&body[..body.len().min(120)])
        ))
    })?;
    let rows = v
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network("FindTreatment.gov response has no rows".into()))?;
    Ok(rows
        .iter()
        .map(|r| {
            let mut services: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for sv in r
                .get("services")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                services
                    .entry(s(sv, "f2"))
                    .or_default()
                    .extend(split_values(&s(sv, "f3")));
            }
            let street = [s(r, "street1"), s(r, "street2")]
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            Facility {
                name: s(r, "name1"),
                name2: s(r, "name2"),
                address: street,
                city: s(r, "city"),
                zip: s(r, "zip"),
                phone: s(r, "phone"),
                intake_phone: s(r, "intake1"),
                website: s(r, "website"),
                latitude: s(r, "latitude").parse().unwrap_or(0.0),
                longitude: s(r, "longitude").parse().unwrap_or(0.0),
                miles: r.get("miles").and_then(Value::as_f64).unwrap_or(0.0),
                services,
            }
        })
        .collect())
}

/// Parse the bundled statewide snapshot and keep facilities within `miles` of a point.
pub fn parse_snapshot(text: &str, lat: f64, lng: f64, miles: f64) -> Result<Vec<Facility>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let mut out = Vec::new();
    for rec in rdr.records() {
        let r = rec.map_err(|e| PhnError::Internal(format!("{SNAPSHOT_FILE}: {e}")))?;
        let (flat, flng) = (r[11].parse().unwrap_or(0.0), r[12].parse().unwrap_or(0.0));
        let d = haversine_miles(lat, lng, flat, flng);
        if d > miles {
            continue;
        }
        let mut services: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for part in r[13].split('|') {
            if let Some((code, vals)) = part.split_once('=') {
                services
                    .entry(code.to_string())
                    .or_default()
                    .extend(split_values(vals));
            }
        }
        let street = [r[2].trim(), r[3].trim()]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        out.push(Facility {
            name: r[0].to_string(),
            name2: r[1].to_string(),
            address: street,
            city: r[4].to_string(),
            zip: r[6].to_string(),
            phone: r[7].to_string(),
            intake_phone: r[8].to_string(),
            website: r[10].to_string(),
            latitude: flat,
            longitude: flng,
            miles: (d * 10.0).round() / 10.0,
            services,
        });
    }
    out.sort_by(|a, b| a.miles.total_cmp(&b.miles));
    Ok(out)
}

/// Facilities found, with provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub facilities: Vec<Facility>,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
    pub url: String,
}

impl SearchResult {
    /// A citation for this result.
    pub fn source(&self) -> Source {
        Source {
            title: "FindTreatment.gov substance use treatment facility locator".into(),
            publisher: "Substance Abuse and Mental Health Services Administration (SAMHSA)".into(),
            url: Some(self.url.clone()),
            effective_date: None,
            retrieved: Some(self.as_of.chars().take(10).collect()),
            mode: self.mode,
        }
    }
}

/// Search near a point through the live → cache → snapshot chain.
pub async fn search(
    http: &HttpClient,
    cache: Option<&Cache>,
    lat: f64,
    lng: f64,
    miles: f64,
    snapshot: Option<SnapshotFallback>,
) -> Result<SearchResult> {
    let url = search_url(lat, lng, miles);
    let fetched = fetch_with_fallback(http, cache, &url, TTL_SECS, snapshot).await?;
    let facilities = if fetched.mode == SourceMode::Snapshot {
        parse_snapshot(&String::from_utf8_lossy(&fetched.body), lat, lng, miles)?
    } else {
        parse_json(&fetched.body)?
    };
    Ok(SearchResult {
        facilities,
        mode: fetched.mode,
        as_of: fetched.as_of,
        fallback_reason: fetched.fallback_reason,
        url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_plane_converts_to_known_points() {
        // MassGIS state plane origin parameters: (200000, 750000) is 41N, 71.5W.
        let (lat, lng) = state_plane_to_lat_lng(200_000.0, 750_000.0);
        assert!(
            (lat - 41.0).abs() < 1e-6 && (lng + 71.5).abs() < 1e-6,
            "{lat} {lng}"
        );
        // Boston's label point is about (236000, 900000): near 42.35N, 71.06W.
        let (lat, lng) = state_plane_to_lat_lng(236_000.0, 900_000.0);
        assert!(
            (lat - 42.35).abs() < 0.05 && (lng + 71.06).abs() < 0.08,
            "{lat} {lng}"
        );
    }

    #[test]
    fn parses_live_rows_and_snapshot_rows() {
        let body = br#"{"page":1,"totalPages":1,"recordCount":1,"rows":[{"name1":"A","name2":"","street1":"1 Main St","street2":null,"city":"Pittsfield","state":"MA","zip":"01201","phone":"413-555-0100","intake1":null,"website":"","latitude":"42.45","longitude":"-73.25","miles":0.4,"services":[{"f1":"Payment","f2":"PAY","f3":"Medicaid; Cash or self-payment"},{"f1":"Type of Opioid Treatment","f2":"OT","f3":"Methadone maintenance"}]}]}"#;
        let f = parse_json(body).unwrap();
        assert_eq!(f.len(), 1);
        assert!(f[0].has("PAY", "medicaid") && f[0].has("OT", "methadone"));
        assert_eq!(f[0].address, "1 Main St");
        let csv = "name1,name2,street1,street2,city,state,zip,phone,intake1,hotline1,website,latitude,longitude,services\nB,,2 Elm St,,Lee,MA,01238,413-555-0101,,,,42.30,-73.25,PAY=Medicaid|SL=Spanish\nFar,,3 Oak St,,Boston,MA,02108,617-555-0102,,,,42.36,-71.06,PAY=Medicaid\n";
        let f = parse_snapshot(csv, 42.45, -73.25, 25.0).unwrap();
        assert_eq!(f.len(), 1);
        assert!(f[0].has("SL", "spanish"));
        assert!((f[0].miles - 10.4).abs() < 0.2, "{}", f[0].miles);
        assert!(search_url(42.45, -73.25, 10.0).contains("limitValue=16093"));
    }
}
