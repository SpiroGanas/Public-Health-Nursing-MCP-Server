//! HRSA health center sites (data.hrsa.gov) for Massachusetts: the live
//! download (filtered to active Massachusetts service sites), cached for a
//! week, with the bundled `hrsa-health-centers-ma` snapshot as the fallback.

use phn_core::{PhnError, Result, SourceMode};
use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::http::HttpClient;
use crate::snapshots::SnapshotStore;

/// HRSA's site download (about 14 MB, all states).
pub const HRSA_SITES_URL: &str = "https://data.hrsa.gov/DataDownload/DD_Files/Health_Center_Service_Delivery_and_LookAlike_Sites.csv";
/// Snapshot id and file.
pub const SNAPSHOT_ID: &str = "hrsa-health-centers-ma";
const SNAPSHOT_FILE: &str = "hrsa_health_centers_ma.csv";
const CACHE_KEY: &str = "hrsa-health-centers-ma-v1";
const TTL_SECS: u64 = 7 * 24 * 3600;

/// One health center site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthCenterSite {
    pub site_name: String,
    pub health_center: String,
    pub center_type: String,
    pub address: String,
    pub city: String,
    pub zip: String,
    pub phone: String,
    pub website: String,
    pub hours_per_week: String,
    pub setting: String,
    pub location_type: String,
    pub site_type: String,
    pub schedule: String,
    pub calendar: String,
    pub county: String,
    pub longitude: String,
    pub latitude: String,
}

/// Sites plus where they came from.
#[derive(Debug, Clone)]
pub struct Sites {
    pub sites: Vec<HealthCenterSite>,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
}

fn title_if_upper(s: &str) -> String {
    if s.chars().any(char::is_lowercase) {
        s.to_string()
    } else {
        phn_core::text::title_case(s)
    }
}

/// Filter HRSA's full download to active Massachusetts service sites, in the
/// snapshot's column layout (CSV text).
pub fn filter_download(bytes: &[u8]) -> Result<String> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes));
    let headers = rdr
        .headers()
        .map_err(|e| PhnError::Network(format!("HRSA download: {e}")))?
        .clone();
    let col = |name: &str| headers.iter().position(|h| h == name);
    let need = |name: &str| {
        col(name).ok_or_else(|| PhnError::Network(format!("HRSA download lacks column '{name}'")))
    };
    let cols = [
        need("Site Name")?,
        need("Health Center Name")?,
        need("Health Center Type")?,
        need("Site Address")?,
        need("Site City")?,
        need("Site Postal Code")?,
        need("Site Telephone Number")?,
        need("Site Web Address")?,
        need("Operating Hours per Week")?,
        need("Health Center Service Delivery Site Location Setting Description")?,
        need("Health Center Location Type Description")?,
        need("Health Center Type Description")?,
        need("Health Center Operational Schedule Description")?,
        need("Health Center Operating Calendar")?,
        need("Complete County Name")?,
        need("Geocoding Artifact Address Primary X Coordinate")?,
        need("Geocoding Artifact Address Primary Y Coordinate")?,
    ];
    let state = need("Site State Abbreviation")?;
    let status = need("Site Status Description")?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| PhnError::Network(format!("HRSA download: {e}")))?;
        if rec.get(state).map(str::trim) != Some("MA")
            || rec.get(status).map(str::trim) != Some("Active")
            || rec.get(cols[11]).map(str::trim) == Some("Administrative")
        {
            continue;
        }
        let mut row: Vec<String> = cols
            .iter()
            .map(|&i| rec.get(i).unwrap_or("").trim().to_string())
            .collect();
        row[1] = title_if_upper(&row[1]);
        row[4] = title_if_upper(&row[4]);
        row[14] = row[14].trim_end_matches(" County").to_string();
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(PhnError::Network(
            "the HRSA download had no active Massachusetts sites".into(),
        ));
    }
    rows.sort_by(|a, b| (&a[4], &a[0]).cmp(&(&b[4], &b[0])));
    let mut w = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    w.write_record([
        "site_name",
        "health_center",
        "center_type",
        "address",
        "city",
        "zip",
        "phone",
        "website",
        "hours_per_week",
        "setting",
        "location_type",
        "site_type",
        "schedule",
        "calendar",
        "county",
        "longitude",
        "latitude",
    ])
    .map_err(|e| PhnError::Internal(e.to_string()))?;
    for r in &rows {
        w.write_record(r)
            .map_err(|e| PhnError::Internal(e.to_string()))?;
    }
    let bytes = w
        .into_inner()
        .map_err(|e| PhnError::Internal(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| PhnError::Internal(e.to_string()))
}

/// Parse the snapshot layout.
pub fn parse_sites(text: &str) -> Result<Vec<HealthCenterSite>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    rdr.deserialize()
        .map(|r| r.map_err(|e| PhnError::Network(format!("HRSA sites: {e}"))))
        .collect()
}

/// Massachusetts sites: fresh cache, then live download, then stale cache,
/// then the bundled snapshot.
pub async fn ma_sites(
    http: &HttpClient,
    cache: Option<&Cache>,
    snapshots: &SnapshotStore,
) -> Result<Sites> {
    // A cache read error is a miss, not a failure.
    let cached = cache.and_then(|c| c.get_or_miss(CACHE_KEY));
    if let Some(hit) = &cached
        && hit.fresh
        && let Ok(text) = std::str::from_utf8(&hit.body)
        && let Ok(sites) = parse_sites(text)
    {
        return Ok(Sites {
            sites,
            mode: SourceMode::Cache,
            as_of: hit.fetched_at.to_rfc3339(),
            fallback_reason: None,
        });
    }
    let live_error = match http.get_bytes(HRSA_SITES_URL).await {
        Ok(body) => match filter_download(&body) {
            Ok(text) => {
                if let Some(c) = cache
                    && let Err(e) = c.put(CACHE_KEY, text.as_bytes(), TTL_SECS)
                {
                    tracing::warn!(error = %e, "could not cache HRSA sites");
                }
                return Ok(Sites {
                    sites: parse_sites(&text)?,
                    mode: SourceMode::Live,
                    as_of: chrono::Utc::now().to_rfc3339(),
                    fallback_reason: None,
                });
            }
            Err(e) => e.to_string(),
        },
        Err(e) => e.to_string(),
    };
    if let Some(hit) = cached
        && let Ok(text) = std::str::from_utf8(&hit.body)
        && let Ok(sites) = parse_sites(text)
    {
        return Ok(Sites {
            sites,
            mode: SourceMode::Cache,
            as_of: hit.fetched_at.to_rfc3339(),
            fallback_reason: Some(format!(
                "live download failed ({live_error}); using the cached copy"
            )),
        });
    }
    let (text, entry, _) = snapshots.file_text(SNAPSHOT_ID, SNAPSHOT_FILE)?;
    Ok(Sites {
        sites: parse_sites(&text)?,
        mode: SourceMode::Snapshot,
        as_of: entry.retrieved.clone(),
        fallback_reason: Some(format!(
            "live download failed ({live_error}); using the bundled snapshot"
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_damaged_cache_does_not_block_the_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&phn_core::Config::for_root(dir.path())).unwrap();
        let cache = Cache::in_memory().unwrap();
        cache.damage_for_tests();
        assert!(cache.get(CACHE_KEY).is_err());
        let sites = ma_sites(&http, Some(&cache), &SnapshotStore::bundled())
            .await
            .unwrap();
        assert_eq!(sites.mode, SourceMode::Snapshot);
        assert!(sites.sites.len() > 200);
    }

    #[test]
    fn snapshot_parses_and_filter_keeps_ma_service_sites() {
        let store = SnapshotStore::bundled();
        let (text, _, _) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE).unwrap();
        let sites = parse_sites(&text).unwrap();
        assert!(sites.len() > 200);
        assert!(sites.iter().all(|s| s.site_type != "Administrative"));
        let csv = "Site State Abbreviation,Site Status Description,Site Name,Health Center Name,\
Health Center Type,Site Address,Site City,Site Postal Code,Site Telephone Number,Site Web Address,\
Operating Hours per Week,Health Center Service Delivery Site Location Setting Description,\
Health Center Location Type Description,Health Center Type Description,\
Health Center Operational Schedule Description,Health Center Operating Calendar,\
Complete County Name,Geocoding Artifact Address Primary X Coordinate,\
Geocoding Artifact Address Primary Y Coordinate\n\
MA,Active,Site A,ACME HEALTH,FQHC,1 Main St,AMHERST,01002,413-555-0101,,40,School,Permanent,Service Delivery Site,Full-Time,Year-Round,Hampshire County,-72.5,42.4\n\
MA,Active,Admin,ACME HEALTH,FQHC,2 Main St,Amherst,01002,,,0,Unknown,Permanent,Administrative,Unknown,Unknown,Hampshire County,,\n\
RI,Active,Elsewhere,OTHER,FQHC,3 Main St,Providence,02903,,,40,Unknown,Permanent,Service Delivery Site,Unknown,Unknown,Providence County,,\n";
        let out = filter_download(csv.as_bytes()).unwrap();
        let sites = parse_sites(&out).unwrap();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].city, "Amherst");
        assert_eq!(sites[0].health_center, "Acme Health");
        assert_eq!(sites[0].county, "Hampshire");
    }
}
