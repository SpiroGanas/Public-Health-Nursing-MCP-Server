//! Grant and bid listings for `search_grant_opportunities` (PLAN-06).
//!
//! * **Federal:** the Simpler.Grants.gov search API
//!   (`POST https://api.simpler.grants.gov/v1/opportunities/search`). The
//!   request and response shapes follow the project's published OpenAPI
//!   specification (HHS/simpler-grants-gov, `api/openapi.generated.yml`, read
//!   2026-11-12). The API needs a key sent in an `X-API-Key` header; this
//!   server reads an optional key from `PHN_GRANTS_GOV_API_KEY`. The key is
//!   never cached, returned, or put in a message. The host was **blocked** from
//!   the build environment, so the live path was tested only against a local
//!   server that answers in the specification's shape.
//! * **Massachusetts:** COMMBUYS, the state's procurement system, was also
//!   blocked. The Operational Services Division publishes a weekly list of new
//!   bids on mass.gov; the bundled `commbuys-weekly-bids` snapshot is that list
//!   as read on the date shown (see `xtask/scripts/build_grants_snapshots.py`).
//!
//! The requests carry only topic words and filters: nothing about a client or
//! the agency.

use chrono::NaiveDate;
use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::apifetch::{api_key, scrub};
use crate::http::HttpClient;
use crate::snapshots::{SnapshotOrigin, SnapshotStore};

/// Environment variable with the optional Simpler.Grants.gov API key.
pub const KEY_ENV: &str = "PHN_GRANTS_GOV_API_KEY";
/// The search endpoint.
pub const SEARCH_URL: &str = "https://api.simpler.grants.gov/v1/opportunities/search";
/// Snapshot id and file of the COMMBUYS weekly list.
pub const COMMBUYS_SNAPSHOT_ID: &str = "commbuys-weekly-bids";
pub const COMMBUYS_FILE: &str = "commbuys_weekly_bids.json";
/// The live location of the weekly list (a PDF).
pub const COMMBUYS_LIST_URL: &str =
    "https://www.mass.gov/doc/commbuys-home-page-bid-count/download";
/// Longest topic query the API accepts (characters).
pub const MAX_QUERY_CHARS: usize = 100;

// ---------------------------------------------------------------- federal

/// A federal search, in the API's own vocabulary.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FederalQuery {
    /// Topic words (at most 100 characters).
    pub query: Option<String>,
    /// Match any of the query's words rather than all of them.
    pub any_word: bool,
    /// `applicant_type` values, for example `county_governments`.
    pub applicant_types: Vec<String>,
    /// `funding_category` values, for example `health`.
    pub funding_categories: Vec<String>,
    /// `opportunity_status` values: `posted`, `forecasted`.
    pub statuses: Vec<String>,
    /// Closing-date window.
    pub close_from: Option<NaiveDate>,
    pub close_to: Option<NaiveDate>,
    /// Keep awards whose ceiling is at least this many dollars.
    pub award_ceiling_min: Option<u64>,
    /// Keep awards whose floor is at most this many dollars.
    pub award_floor_max: Option<u64>,
    /// Top-level agency code filter, for example `HHS`.
    pub agency: Option<String>,
    pub page_size: u32,
}

/// The JSON request body for a federal search.
pub fn request_body(q: &FederalQuery) -> Value {
    let mut filters = serde_json::Map::new();
    let one_of = |v: &[String]| json!({ "one_of": v });
    if !q.applicant_types.is_empty() {
        filters.insert("applicant_type".into(), one_of(&q.applicant_types));
    }
    if !q.funding_categories.is_empty() {
        filters.insert("funding_category".into(), one_of(&q.funding_categories));
    }
    if !q.statuses.is_empty() {
        filters.insert("opportunity_status".into(), one_of(&q.statuses));
    }
    if let Some(a) = &q.agency {
        filters.insert("top_level_agency".into(), json!({ "one_of": [a] }));
    }
    if q.close_from.is_some() || q.close_to.is_some() {
        let mut w = serde_json::Map::new();
        if let Some(d) = q.close_from {
            w.insert("start_date".into(), json!(d.to_string()));
        }
        if let Some(d) = q.close_to {
            w.insert("end_date".into(), json!(d.to_string()));
        }
        filters.insert("close_date".into(), Value::Object(w));
    }
    if let Some(v) = q.award_ceiling_min {
        filters.insert("award_ceiling".into(), json!({ "min": v }));
    }
    if let Some(v) = q.award_floor_max {
        filters.insert("award_floor".into(), json!({ "max": v }));
    }
    let (order_by, dir) = if q.query.is_some() {
        ("relevancy", "descending")
    } else {
        ("close_date", "ascending")
    };
    let mut body = json!({
        "filters": Value::Object(filters),
        "pagination": {
            "page_offset": 1,
            "page_size": q.page_size.clamp(1, 100),
            "sort_order": [{ "order_by": order_by, "sort_direction": dir }],
        },
    });
    if let Some(text) = &q.query {
        body["query"] = json!(text.chars().take(MAX_QUERY_CHARS).collect::<String>());
        if q.any_word {
            body["query_operator"] = json!("OR");
        }
    }
    body
}

/// One federal opportunity, from the search response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FederalOpportunity {
    pub opportunity_id: String,
    pub opportunity_number: Option<String>,
    pub title: String,
    pub agency_name: Option<String>,
    pub top_level_agency_name: Option<String>,
    /// forecasted, posted, closed, or archived.
    pub status: String,
    pub post_date: Option<String>,
    pub close_date: Option<String>,
    pub close_date_description: Option<String>,
    pub award_floor: Option<u64>,
    pub award_ceiling: Option<u64>,
    pub estimated_total_program_funding: Option<u64>,
    pub expected_number_of_awards: Option<u64>,
    pub is_cost_sharing: Option<bool>,
    pub applicant_types: Vec<String>,
    pub funding_categories: Vec<String>,
    pub funding_instruments: Vec<String>,
    pub assistance_listings: Vec<String>,
    /// The opportunity's own summary, cut to a few hundred characters.
    pub summary: Option<String>,
    pub additional_info_url: Option<String>,
}

/// A parsed federal response.
#[derive(Debug, Clone, PartialEq)]
pub struct FederalResults {
    pub opportunities: Vec<FederalOpportunity>,
    pub total_records: Option<u64>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
}

fn n(v: &Value, k: &str) -> Option<u64> {
    v.get(k).and_then(|x| {
        x.as_u64()
            .or_else(|| x.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
    })
}

fn strs(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Remove markup and cut a summary to `max` characters.
pub fn plain_summary(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let flat = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let cut: String = flat.chars().take(max).collect();
        format!("{}...", cut.trim_end())
    }
}

/// Parse a search response (the `data` array of the specification's schema).
pub fn parse_response(body: &[u8]) -> Result<FederalResults> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("the grants search did not return JSON: {e}")))?;
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network("the grants search response has no 'data' list".into()))?;
    let mut out = Vec::new();
    for item in data {
        let sum = item.get("summary").cloned().unwrap_or(Value::Null);
        let id = match item.get("opportunity_id") {
            Some(Value::String(x)) => x.clone(),
            Some(Value::Number(x)) => x.to_string(),
            _ => continue,
        };
        let listings = item
            .get("opportunity_assistance_listings")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| s(l, "assistance_listing_number"))
                    .collect()
            })
            .unwrap_or_default();
        out.push(FederalOpportunity {
            opportunity_id: id,
            opportunity_number: s(item, "opportunity_number"),
            title: s(item, "opportunity_title").unwrap_or_else(|| "(no title)".into()),
            agency_name: s(item, "agency_name"),
            top_level_agency_name: s(item, "top_level_agency_name"),
            status: s(item, "opportunity_status").unwrap_or_else(|| "unknown".into()),
            post_date: s(&sum, "post_date"),
            close_date: s(&sum, "close_date"),
            close_date_description: s(&sum, "close_date_description"),
            award_floor: n(&sum, "award_floor"),
            award_ceiling: n(&sum, "award_ceiling"),
            estimated_total_program_funding: n(&sum, "estimated_total_program_funding"),
            expected_number_of_awards: n(&sum, "expected_number_of_awards"),
            is_cost_sharing: sum.get("is_cost_sharing").and_then(Value::as_bool),
            applicant_types: strs(&sum, "applicant_types"),
            funding_categories: strs(&sum, "funding_categories"),
            funding_instruments: strs(&sum, "funding_instruments"),
            assistance_listings: listings,
            summary: s(&sum, "summary_description").map(|t| plain_summary(&t, 400)),
            additional_info_url: s(&sum, "additional_info_url"),
        });
    }
    let total_records = v
        .get("pagination_info")
        .and_then(|p| p.get("total_records"))
        .and_then(Value::as_u64);
    Ok(FederalResults {
        opportunities: out,
        total_records,
    })
}

/// Why a live federal search was not used, in words for the nurse.
#[derive(Debug, Clone, PartialEq)]
pub enum FederalOutcome {
    /// The API answered.
    Live(FederalResults),
    /// No key is set; nothing was sent.
    NoKey,
    /// The request failed (the message has the key removed).
    Failed(String),
}

/// Run a live federal search using `base_url` (the real API, or a test server).
pub async fn search_federal_at(
    http: &HttpClient,
    base_url: &str,
    key: Option<&str>,
    q: &FederalQuery,
) -> FederalOutcome {
    let Some(key) = key else {
        return FederalOutcome::NoKey;
    };
    let url = format!("{}/v1/opportunities/search", base_url.trim_end_matches('/'));
    let body = request_body(q).to_string();
    match http
        .post_json_bytes(&url, body.as_bytes(), &[("X-API-Key", key)])
        .await
    {
        Ok(bytes) => match parse_response(&bytes) {
            Ok(r) => FederalOutcome::Live(r),
            Err(e) => FederalOutcome::Failed(scrub(&e.to_string(), Some(key))),
        },
        Err(f) => {
            let hint = match f.status {
                Some(401) | Some(403) => " (the key was refused; check PHN_GRANTS_GOV_API_KEY)",
                _ => "",
            };
            FederalOutcome::Failed(format!("{}{hint}", scrub(&f.message, Some(key))))
        }
    }
}

/// Run a live federal search with the key from the environment.
pub async fn search_federal(http: &HttpClient, q: &FederalQuery) -> FederalOutcome {
    let key = api_key(KEY_ENV);
    search_federal_at(http, "https://api.simpler.grants.gov", key.as_deref(), q).await
}

/// Citation for a live federal search.
pub fn federal_source() -> Source {
    Source {
        title: "Simpler.Grants.gov opportunity search".into(),
        publisher: "U.S. Department of Health and Human Services (Grants.gov)".into(),
        url: Some("https://simpler.grants.gov/".into()),
        effective_date: None,
        retrieved: Some(chrono::Local::now().date_naive().to_string()),
        mode: SourceMode::Live,
    }
}

// ---------------------------------------------------------------- COMMBUYS

/// One bid on the COMMBUYS weekly list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommbuysBid {
    pub org_id: String,
    pub organization: String,
    /// Closing date (`YYYY-MM-DD`).
    pub bid_date: Option<String>,
    /// Closing time as printed, for example "2:00 PM".
    pub bid_time: String,
    pub description: String,
    /// COMMBUYS bid number, for example `BD-27-1039-EHS01-ASHWA-135088`.
    pub bid_number: String,
}

/// The weekly list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommbuysList {
    pub list_title: String,
    /// Date printed on the list.
    pub list_date: String,
    pub source_url: String,
    pub bids: Vec<CommbuysBid>,
}

/// The bundled weekly list, and where it came from.
pub fn commbuys_snapshot(store: &SnapshotStore) -> Result<(CommbuysList, Source)> {
    let (text, entry, origin) = store.file_text(COMMBUYS_SNAPSHOT_ID, COMMBUYS_FILE)?;
    let list: CommbuysList = serde_json::from_str(&text)
        .map_err(|e| PhnError::Internal(format!("{COMMBUYS_FILE}: {e}")))?;
    let mut src = entry.to_source();
    src.effective_date = Some(list.list_date.clone());
    if matches!(origin, SnapshotOrigin::CacheOverride) {
        src.title.push_str(" (refreshed copy)");
    }
    Ok((list, src))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_core::Config;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const SAMPLE: &str = r#"{"data":[{"opportunity_id":"abc-1","opportunity_number":"TEST-24-001",
        "opportunity_title":"Community Health Prevention","agency_name":"Test Agency",
        "top_level_agency_name":"Test Department","opportunity_status":"posted",
        "opportunity_assistance_listings":[{"program_title":"X","assistance_listing_number":"93.000"}],
        "summary":{"summary_description":"<p>Prevention <b>grants</b> for health departments.</p>",
          "close_date":"2027-01-15","post_date":"2026-11-01","award_floor":50000,"award_ceiling":250000,
          "estimated_total_program_funding":1000000,"expected_number_of_awards":4,"is_cost_sharing":false,
          "applicant_types":["county_governments","city_or_township_governments"],
          "funding_categories":["health"],"funding_instruments":["cooperative_agreement"]}},
        {"opportunity_id":7,"opportunity_title":null,"summary":null}],
        "pagination_info":{"total_records":2,"page_offset":1,"page_size":25},"status_code":200}"#;

    #[test]
    fn response_parses_per_the_specification() {
        let r = parse_response(SAMPLE.as_bytes()).unwrap();
        assert_eq!(r.total_records, Some(2));
        assert_eq!(r.opportunities.len(), 2);
        let o = &r.opportunities[0];
        assert_eq!(o.opportunity_number.as_deref(), Some("TEST-24-001"));
        assert_eq!(o.close_date.as_deref(), Some("2027-01-15"));
        assert_eq!(o.award_ceiling, Some(250000));
        assert_eq!(o.assistance_listings, vec!["93.000"]);
        assert_eq!(
            o.summary.as_deref(),
            Some("Prevention grants for health departments.")
        );
        // Missing fields are tolerated.
        assert_eq!(r.opportunities[1].opportunity_id, "7");
        assert_eq!(r.opportunities[1].title, "(no title)");
        assert!(parse_response(b"<html>blocked</html>").is_err());
        assert!(parse_response(b"{\"message\":\"x\"}").is_err());
    }

    #[test]
    fn request_body_omits_empty_filters_and_limits_the_query() {
        let q = FederalQuery {
            query: Some("x".repeat(150)),
            applicant_types: vec!["county_governments".into()],
            statuses: vec!["posted".into()],
            close_from: NaiveDate::from_ymd_opt(2026, 11, 12),
            award_ceiling_min: Some(10000),
            page_size: 500,
            ..Default::default()
        };
        let b = request_body(&q);
        assert_eq!(b["query"].as_str().unwrap().chars().count(), 100);
        assert_eq!(b["pagination"]["page_size"], 100);
        assert_eq!(b["pagination"]["page_offset"], 1);
        assert_eq!(b["pagination"]["sort_order"][0]["order_by"], "relevancy");
        let f = b["filters"].as_object().unwrap();
        assert!(!f.contains_key("funding_category"));
        assert_eq!(f["applicant_type"]["one_of"][0], "county_governments");
        assert_eq!(f["close_date"]["start_date"], "2026-11-12");
        assert!(f["close_date"].get("end_date").is_none());
        assert_eq!(f["award_ceiling"]["min"], 10000);
        // No query: soonest deadline first.
        let b2 = request_body(&FederalQuery {
            page_size: 5,
            ..Default::default()
        });
        assert!(b2.get("query").is_none());
        assert_eq!(b2["pagination"]["sort_order"][0]["order_by"], "close_date");
    }

    #[test]
    fn the_committed_commbuys_snapshot_loads() {
        let store = SnapshotStore::bundled();
        let (list, src) = commbuys_snapshot(&store).unwrap();
        assert!(list.bids.len() > 100);
        assert!(list.list_date.starts_with("20"));
        assert_eq!(src.effective_date.as_deref(), Some(list.list_date.as_str()));
        assert!(list.bids.iter().all(|b| b.bid_number.starts_with("BD-")));
        assert!(list.bids.iter().all(|b| b.bid_date.is_some()));
    }

    async fn serve_once(
        status: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let h = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let mut got = Vec::new();
            loop {
                let k = sock.read(&mut buf).await.unwrap();
                got.extend_from_slice(&buf[..k]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some(i) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if got.len() >= i + 4 + len {
                        break;
                    }
                }
                if k == 0 {
                    break;
                }
            }
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            String::from_utf8_lossy(&got).to_string()
        });
        (format!("http://{addr}"), h)
    }

    fn client() -> HttpClient {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::for_root(dir.path());
        config.offline = false;
        HttpClient::new(&config).unwrap()
    }

    #[tokio::test]
    async fn live_search_sends_the_key_in_a_header_and_never_echoes_it() {
        let (base, h) = serve_once("200 OK", SAMPLE).await;
        let q = FederalQuery {
            query: Some("immunization".into()),
            page_size: 10,
            ..Default::default()
        };
        let out = search_federal_at(&client(), &base, Some("SECRETKEY123"), &q).await;
        let FederalOutcome::Live(r) = out else {
            panic!("{out:?}")
        };
        assert_eq!(r.opportunities.len(), 2);
        let req = h.await.unwrap();
        assert!(req.starts_with("POST /v1/opportunities/search"));
        assert!(req.to_ascii_lowercase().contains("x-api-key: secretkey123"));
        assert!(req.contains("\"query\":\"immunization\""));
    }

    #[tokio::test]
    async fn no_key_sends_nothing_and_errors_are_scrubbed() {
        let out = search_federal_at(
            &client(),
            "http://127.0.0.1:9",
            None,
            &FederalQuery::default(),
        )
        .await;
        assert_eq!(out, FederalOutcome::NoKey);
        let (base, _h) = serve_once("401 Unauthorized", "{\"message\":\"no\"}").await;
        let out = search_federal_at(
            &client(),
            &base,
            Some("SECRETKEY123"),
            &FederalQuery::default(),
        )
        .await;
        let FederalOutcome::Failed(m) = out else {
            panic!("{out:?}")
        };
        assert!(m.contains("401"), "{m}");
        assert!(m.contains("PHN_GRANTS_GOV_API_KEY"));
        assert!(!m.contains("SECRETKEY123"));
    }
}
