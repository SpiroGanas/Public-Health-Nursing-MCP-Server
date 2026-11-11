//! Massachusetts DPH mosquito- and tick-borne disease data (ENV-06).
//!
//! * **Arbovirus risk (live):** DPH's "Massachusetts arbovirus update" page on
//!   mass.gov links one "EEE and WNV risk level and data" workbook (XLSX) per
//!   season. [`workbook_links`] finds them in the page and
//!   [`fetch_workbook`] downloads one through the live → cache chain (the
//!   workbook is parsed by the tool, which has the spreadsheet reader). DPH
//!   updates the levels at least weekly from June to October.
//! * **Snapshots:** `dph-arbovirus-risk` (risk by town and positive findings,
//!   2022 to the latest season) and `dph-tickborne-surveillance` (tick-borne
//!   disease cases and DPH incidence rates by county from the annual summaries,
//!   and emergency department visits by county from the syndromic reports),
//!   built by `xtask/scripts/build_vectorhousing_snapshots.py`.

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::apifetch::fetch_validated;
use crate::cache::Cache;
use crate::fetch::Fetched;
use crate::http::HttpClient;
use crate::snapshots::{SnapshotEntry, SnapshotStore};

/// DPH's arbovirus update page (risk maps and the workbook links).
pub const ARBOVIRUS_PAGE_URL: &str =
    "https://www.mass.gov/info-details/massachusetts-arbovirus-update";
/// Snapshot of risk levels and positive findings.
pub const ARBO_SNAPSHOT_ID: &str = "dph-arbovirus-risk";
/// Snapshot of tick-borne disease cases and ED visits.
pub const TICK_SNAPSHOT_ID: &str = "dph-tickborne-surveillance";
/// Cache time for the page and workbook during the season (DPH updates at
/// least weekly, often daily).
pub const IN_SEASON_TTL_SECS: u64 = 6 * 3600;
/// Cache time outside the season.
pub const OFF_SEASON_TTL_SECS: u64 = 7 * 24 * 3600;

/// A DPH arbovirus risk level. EEE uses Remote to Critical; WNV uses Low to
/// Critical.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum RiskLevel {
    Remote,
    Low,
    Moderate,
    High,
    Critical,
}

impl RiskLevel {
    /// Every level, lowest first.
    pub const ALL: [RiskLevel; 5] = [
        RiskLevel::Remote,
        RiskLevel::Low,
        RiskLevel::Moderate,
        RiskLevel::High,
        RiskLevel::Critical,
    ];

    /// Parse DPH's spelling ("Moderate ", "high").
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "remote" => Some(Self::Remote),
            "low" => Some(Self::Low),
            "moderate" => Some(Self::Moderate),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }

    /// The level as DPH writes it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Remote => "Remote",
            Self::Low => "Low",
            Self::Moderate => "Moderate",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }
}

/// One town's risk levels in one season.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownRisk {
    pub season: i32,
    /// MassGIS town name.
    pub town: String,
    pub county: String,
    pub eee: RiskLevel,
    pub wnv: RiskLevel,
    /// Date DPH last updated the levels (YYYY-MM-DD).
    pub as_of: String,
}

/// Positive findings counted by season, virus, sample type, county, and town.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PositiveCount {
    pub season: i32,
    /// "EEE" or "WNV".
    pub virus: String,
    /// "Mosquito", "Animal", or "Human".
    pub sample_type: String,
    pub county: String,
    /// Town where the mosquito sample was collected or the animal found; none for
    /// human cases (DPH does not publish their town).
    pub town: Option<String>,
    pub count: u32,
    /// Latest collection or onset date (none for human cases).
    pub latest_date: Option<String>,
}

/// Tick-borne disease cases by county from DPH's annual summaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TickCases {
    pub year: i32,
    /// "Lyme disease", "Babesiosis", or "Anaplasmosis (HGA)".
    pub disease: String,
    /// County ("Dukes & Nantucket" combined), "Massachusetts" (the state total of a
    /// county table), or "Massachusetts (ten-year table)".
    pub area: String,
    pub cases: Option<u64>,
    /// DPH's incidence rate per 100,000 residents, as published.
    pub rate_per_100k: Option<f64>,
    /// Case classifications counted ("probable", "confirmed and probable", ...).
    pub case_classes: String,
    /// DPH report the row comes from.
    pub report: String,
}

/// Emergency department visits with a tick-borne disease diagnosis by county.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TickEdVisits {
    pub year: i32,
    /// Last month covered (YYYY-MM): December for complete years.
    pub through_month: String,
    pub county: String,
    pub ed_visits: u64,
    pub tickborne_disease_visits: u64,
    /// Tick-borne disease visits per 10,000 ED visits, as published.
    pub per_10k_ed_visits: Option<f64>,
}

/// MassGIS name for a town as DPH writes it ("Manchester" →
/// "Manchester-by-the-Sea"; Boston neighborhoods → "Boston").
pub fn canonical_town(name: &str) -> String {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    match n.to_ascii_lowercase().as_str() {
        "manchester" => "Manchester-by-the-Sea".into(),
        "mattapoissett" => "Mattapoisett".into(),
        "allston" | "brighton" | "hyde park" | "mattapan" | "roxbury" | "dorchester"
        | "jamaica plain" | "west roxbury" | "roslindale" | "charlestown" | "east boston"
        | "south boston" => "Boston".into(),
        _ => n,
    }
}

/// Workbook links on the arbovirus update page as (season, absolute URL),
/// newest season first.
pub fn workbook_links(html: &str) -> Vec<(i32, String)> {
    const MARK: &str = "-eee-and-wnv-risk-level-and-data";
    let mut out: Vec<(i32, String)> = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("/doc/") {
        let after = &rest[i + 5..];
        rest = after;
        let (Some(year), Some(tail)) = (after.get(..4), after.get(4..)) else {
            continue;
        };
        let Ok(season) = year.parse::<i32>() else {
            continue;
        };
        if !(2000..=2100).contains(&season) || !tail.starts_with(MARK) {
            continue;
        }
        let Some(end) = after.find("/download") else {
            continue;
        };
        let slug = &after[..end];
        if !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            continue;
        }
        let url = format!("https://www.mass.gov/doc/{slug}/download");
        if !out.iter().any(|(s, _)| *s == season) {
            out.push((season, url));
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

fn expect_html(body: &[u8]) -> Result<()> {
    let text = String::from_utf8_lossy(&body[..body.len().min(200_000)]).to_lowercase();
    if text.contains("arbovirus") && text.contains("<html") {
        Ok(())
    } else {
        Err(PhnError::Network(
            "the arbovirus update page did not look like the DPH page".into(),
        ))
    }
}

fn expect_xlsx(body: &[u8]) -> Result<()> {
    if body.starts_with(b"PK\x03\x04") {
        Ok(())
    } else {
        Err(PhnError::Network(
            "the risk workbook was not a spreadsheet (XLSX)".into(),
        ))
    }
}

/// Fetch the arbovirus update page (live → cache).
pub async fn fetch_page(http: &HttpClient, cache: Option<&Cache>, ttl_secs: u64) -> Result<Fetched> {
    fetch_validated(
        http,
        cache,
        ARBOVIRUS_PAGE_URL,
        None,
        ttl_secs,
        &expect_html,
        None,
    )
    .await
}

/// Fetch one season's risk workbook (live → cache). Only mass.gov workbook
/// URLs found by [`workbook_links`] are accepted.
pub async fn fetch_workbook(
    http: &HttpClient,
    cache: Option<&Cache>,
    url: &str,
    ttl_secs: u64,
) -> Result<Fetched> {
    if !url.starts_with("https://www.mass.gov/doc/") {
        return Err(PhnError::InvalidInput(format!(
            "not a mass.gov workbook link: {url}"
        )));
    }
    fetch_validated(http, cache, url, None, ttl_secs, &expect_xlsx, None).await
}

fn csv_rows(text: &str, what: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| PhnError::Internal(format!("{what}: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();
    let mut rows = Vec::new();
    for r in rdr.records() {
        let r = r.map_err(|e| PhnError::Internal(format!("{what}: {e}")))?;
        rows.push(r.iter().map(str::to_string).collect());
    }
    Ok((headers, rows))
}

fn col(headers: &[String], name: &str, what: &str) -> Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| PhnError::Internal(format!("{what}: no column {name}")))
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Risk levels by town for every season in the snapshot.
pub fn snapshot_risk(store: &SnapshotStore) -> Result<(Vec<TownRisk>, SnapshotEntry)> {
    let what = "ma_arbovirus_risk.csv";
    let (text, entry, _) = store.file_text(ARBO_SNAPSHOT_ID, what)?;
    let (h, rows) = csv_rows(&text, what)?;
    let (cs, ct, cc) = (
        col(&h, "season", what)?,
        col(&h, "town", what)?,
        col(&h, "county", what)?,
    );
    let (ce, cw, ca) = (
        col(&h, "eee_level", what)?,
        col(&h, "wnv_level", what)?,
        col(&h, "as_of", what)?,
    );
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let level = |i: usize| {
            RiskLevel::parse(&r[i])
                .ok_or_else(|| PhnError::Internal(format!("{what}: unknown level {}", r[i])))
        };
        out.push(TownRisk {
            season: r[cs]
                .parse()
                .map_err(|_| PhnError::Internal(format!("{what}: bad season {}", r[cs])))?,
            town: r[ct].clone(),
            county: r[cc].clone(),
            eee: level(ce)?,
            wnv: level(cw)?,
            as_of: r[ca].clone(),
        });
    }
    Ok((out, entry))
}

/// Positive findings for every season in the snapshot.
pub fn snapshot_positives(store: &SnapshotStore) -> Result<Vec<PositiveCount>> {
    let what = "ma_arbovirus_positives.csv";
    let (text, _, _) = store.file_text(ARBO_SNAPSHOT_ID, what)?;
    let (h, rows) = csv_rows(&text, what)?;
    let idx: Vec<usize> = [
        "season",
        "virus",
        "sample_type",
        "county",
        "town",
        "count",
        "latest_date",
    ]
    .iter()
    .map(|n| col(&h, n, what))
    .collect::<Result<_>>()?;
    rows.into_iter()
        .map(|r| {
            Ok(PositiveCount {
                season: r[idx[0]]
                    .parse()
                    .map_err(|_| PhnError::Internal(format!("{what}: bad season")))?,
                virus: r[idx[1]].clone(),
                sample_type: r[idx[2]].clone(),
                county: r[idx[3]].clone(),
                town: opt(&r[idx[4]]),
                count: r[idx[5]]
                    .parse()
                    .map_err(|_| PhnError::Internal(format!("{what}: bad count")))?,
                latest_date: opt(&r[idx[6]]),
            })
        })
        .collect()
}

/// Tick-borne disease cases and ED visits from the snapshot.
pub fn snapshot_tickborne(
    store: &SnapshotStore,
) -> Result<(Vec<TickCases>, Vec<TickEdVisits>, SnapshotEntry)> {
    let what = "ma_tickborne_cases.csv";
    let (text, entry, _) = store.file_text(TICK_SNAPSHOT_ID, what)?;
    let (h, rows) = csv_rows(&text, what)?;
    let idx: Vec<usize> = [
        "year",
        "disease",
        "area",
        "cases",
        "rate_per_100k",
        "case_classes",
        "report",
    ]
    .iter()
    .map(|n| col(&h, n, what))
    .collect::<Result<_>>()?;
    let mut cases = Vec::with_capacity(rows.len());
    for r in rows {
        cases.push(TickCases {
            year: r[idx[0]]
                .parse()
                .map_err(|_| PhnError::Internal(format!("{what}: bad year")))?,
            disease: r[idx[1]].clone(),
            area: r[idx[2]].clone(),
            cases: r[idx[3]].trim().parse().ok(),
            rate_per_100k: r[idx[4]].trim().parse().ok(),
            case_classes: r[idx[5]].clone(),
            report: r[idx[6]].clone(),
        });
    }
    let what = "ma_tickborne_ed_visits.csv";
    let (text, _, _) = store.file_text(TICK_SNAPSHOT_ID, what)?;
    let (h, rows) = csv_rows(&text, what)?;
    let idx: Vec<usize> = [
        "year",
        "through_month",
        "county",
        "ed_visits",
        "tickborne_disease_visits",
        "per_10k_ed_visits",
    ]
    .iter()
    .map(|n| col(&h, n, what))
    .collect::<Result<_>>()?;
    let mut ed = Vec::with_capacity(rows.len());
    for r in rows {
        let n = |i: usize| -> Result<u64> {
            r[idx[i]]
                .trim()
                .parse()
                .map_err(|_| PhnError::Internal(format!("{what}: bad number {}", r[idx[i]])))
        };
        ed.push(TickEdVisits {
            year: r[idx[0]]
                .parse()
                .map_err(|_| PhnError::Internal(format!("{what}: bad year")))?,
            through_month: r[idx[1]].clone(),
            county: r[idx[2]].clone(),
            ed_visits: n(3)?,
            tickborne_disease_visits: n(4)?,
            per_10k_ed_visits: r[idx[5]].trim().parse().ok(),
        });
    }
    Ok((cases, ed, entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_workbook_links_newest_first() {
        let html = r#"<html><a href="https://www.mass.gov/doc/2025-eee-and-wnv-risk-level-and-data-final/download">2025</a>
            <a href="/doc/key-to-color-coding-on-eee-risk-map/download">key</a>
            <a href="https://www.mass.gov/doc/2026-eee-and-wnv-risk-level-and-data/download">2026</a>
            <a href="https://www.mass.gov/doc/2024-eee-and-wnv-risk-level-and-data-final-0/download">2024</a>
            <a href="/doc/2023-eee-and-wnv-risk-level-and-data-final/download">2023</a></html>"#;
        let links = workbook_links(html);
        assert_eq!(
            links.iter().map(|l| l.0).collect::<Vec<_>>(),
            vec![2026, 2025, 2024, 2023]
        );
        assert_eq!(
            links[0].1,
            "https://www.mass.gov/doc/2026-eee-and-wnv-risk-level-and-data/download"
        );
        assert_eq!(
            links[3].1,
            "https://www.mass.gov/doc/2023-eee-and-wnv-risk-level-and-data-final/download"
        );
        assert!(workbook_links("<html>nothing</html>").is_empty());
    }

    #[test]
    fn levels_parse_and_order() {
        assert_eq!(RiskLevel::parse("Moderate "), Some(RiskLevel::Moderate));
        assert_eq!(RiskLevel::parse("CRITICAL"), Some(RiskLevel::Critical));
        assert_eq!(RiskLevel::parse("none"), None);
        assert!(RiskLevel::High > RiskLevel::Moderate);
        assert_eq!(canonical_town("Manchester"), "Manchester-by-the-Sea");
        assert_eq!(canonical_town(" Hyde  Park"), "Boston");
        assert_eq!(canonical_town("Amherst"), "Amherst");
    }

    #[test]
    fn snapshots_load() {
        let store = SnapshotStore::bundled();
        let (risk, entry) = snapshot_risk(&store).unwrap();
        assert_eq!(entry.id, ARBO_SNAPSHOT_ID);
        let latest = risk.iter().map(|r| r.season).max().unwrap();
        assert_eq!(risk.iter().filter(|r| r.season == latest).count(), 351);
        assert!(risk.iter().any(|r| r.town == "Manchester-by-the-Sea"));
        let pos = snapshot_positives(&store).unwrap();
        assert!(
            pos.iter()
                .filter(|p| p.sample_type == "Human")
                .all(|p| p.town.is_none() && p.latest_date.is_none())
        );
        let (cases, ed, _) = snapshot_tickborne(&store).unwrap();
        assert!(cases.iter().any(|c| c.disease == "Babesiosis"
            && c.area == "Barnstable"
            && c.rate_per_100k.is_some()));
        assert_eq!(ed.iter().filter(|e| e.year == 2025).count(), 13);
        assert!(expect_xlsx(b"PK\x03\x04rest").is_ok());
        assert!(expect_xlsx(b"<html>").is_err());
    }
}
