//! Socrata (SODA 2.x) client for `data.cdc.gov` and other Socrata portals.
//!
//! CDC publishes NNDSS tables, PLACES, respiratory surveillance, and NWSS
//! wastewater data through Socrata. A [`SocrataQuery`] builds a SoQL URL
//! (`$select`, `$where`, `$group`, `$order`, `$limit`, `$offset`), and
//! [`query`] runs it through the live → cache → snapshot chain, so every
//! result says where its rows came from. A response that is not a list of
//! rows (an HTML block page, a Socrata error object) is never cached; the
//! cached copy or the snapshot is used instead.
//!
//! Socrata returns every value as a string (numbers included); use
//! [`num`] and [`text`] to read fields.

use phn_core::{PhnError, Result, Source, SourceMode};
use serde_json::{Map, Value};

use crate::apifetch::fetch_validated;
use crate::cache::Cache;
use crate::fetch::{SnapshotFallback, local_date};
use crate::http::HttpClient;

/// The CDC open-data portal.
pub const CDC_DOMAIN: &str = "data.cdc.gov";

/// Default rows per request (Socrata's default is 1,000).
pub const DEFAULT_LIMIT: u32 = 5_000;

/// Hard ceiling on rows per request, to keep responses and memory bounded.
pub const MAX_LIMIT: u32 = 50_000;

/// A SoQL query against one dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocrataQuery {
    /// Portal host, e.g. `data.cdc.gov`.
    pub domain: String,
    /// Four-by-four dataset identifier, e.g. `x9gk-5huc`.
    pub dataset_id: String,
    select: Option<String>,
    where_: Vec<String>,
    group: Option<String>,
    order: Option<String>,
    limit: u32,
    offset: u32,
}

impl SocrataQuery {
    /// Query a dataset on a portal.
    pub fn new(domain: impl Into<String>, dataset_id: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            dataset_id: dataset_id.into(),
            select: None,
            where_: Vec::new(),
            group: None,
            order: None,
            limit: DEFAULT_LIMIT,
            offset: 0,
        }
    }

    /// Query a dataset on data.cdc.gov.
    pub fn cdc(dataset_id: impl Into<String>) -> Self {
        Self::new(CDC_DOMAIN, dataset_id)
    }

    /// `$select` clause, e.g. `year, week, sum(m2) as cases`.
    pub fn select(mut self, s: impl Into<String>) -> Self {
        self.select = Some(s.into());
        self
    }

    /// Add a `$where` condition; several are joined with `AND`.
    pub fn and_where(mut self, w: impl Into<String>) -> Self {
        self.where_.push(w.into());
        self
    }

    /// `$group` clause.
    pub fn group(mut self, g: impl Into<String>) -> Self {
        self.group = Some(g.into());
        self
    }

    /// `$order` clause, e.g. `year DESC, week DESC`.
    pub fn order(mut self, o: impl Into<String>) -> Self {
        self.order = Some(o.into());
        self
    }

    /// `$limit` (capped at [`MAX_LIMIT`]).
    pub fn limit(mut self, n: u32) -> Self {
        self.limit = n.clamp(1, MAX_LIMIT);
        self
    }

    /// `$offset`, for paging.
    pub fn offset(mut self, n: u32) -> Self {
        self.offset = n;
        self
    }

    /// The dataset's landing page (for citations).
    pub fn landing_page(&self) -> String {
        format!("https://{}/d/{}", self.domain, self.dataset_id)
    }

    /// The full request URL.
    pub fn url(&self) -> String {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(s) = &self.select {
            params.push(("$select", s.clone()));
        }
        if !self.where_.is_empty() {
            let w = if self.where_.len() == 1 {
                self.where_[0].clone()
            } else {
                self.where_
                    .iter()
                    .map(|c| format!("({c})"))
                    .collect::<Vec<_>>()
                    .join(" AND ")
            };
            params.push(("$where", w));
        }
        if let Some(g) = &self.group {
            params.push(("$group", g.clone()));
        }
        if let Some(o) = &self.order {
            params.push(("$order", o.clone()));
        }
        params.push(("$limit", self.limit.to_string()));
        if self.offset > 0 {
            params.push(("$offset", self.offset.to_string()));
        }
        let qs = params
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        format!(
            "https://{}/resource/{}.json?{qs}",
            self.domain, self.dataset_id
        )
    }
}

/// Quote a string literal for SoQL (single quotes doubled).
pub fn soql_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Percent-encode a query-string component (RFC 3986 unreserved characters kept).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Rows returned by a query, with provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct SocrataRows {
    /// One JSON object per row.
    pub rows: Vec<Map<String, Value>>,
    /// Live, cache, or snapshot.
    pub mode: SourceMode,
    /// When the data was retrieved (RFC 3339) or the snapshot date.
    pub as_of: String,
    /// Why a fallback was used, when one was.
    pub fallback_reason: Option<String>,
    /// The request URL.
    pub url: String,
    /// True when the row count reached the query's limit (more rows may exist).
    pub truncated: bool,
}

impl SocrataRows {
    /// A citation for this result.
    pub fn source(&self, title: impl Into<String>, publisher: impl Into<String>) -> Source {
        Source {
            title: title.into(),
            publisher: publisher.into(),
            url: Some(self.url.clone()),
            effective_date: None,
            retrieved: Some(local_date(&self.as_of)),
            mode: self.mode,
        }
    }
}

/// Run a query through the live → cache → snapshot chain.
///
/// `snapshot` supplies a bundled JSON array (same shape as the live
/// response) when the network and cache both fail.
pub async fn query(
    http: &HttpClient,
    cache: Option<&Cache>,
    q: &SocrataQuery,
    ttl_secs: u64,
    snapshot: Option<SnapshotFallback>,
) -> Result<SocrataRows> {
    let url = q.url();
    // Only a body that parses as rows is cached or returned as live data.
    let fetched = fetch_validated(
        http,
        cache,
        &url,
        None,
        ttl_secs,
        &|b: &[u8]| parse_rows(b).map(|_| ()),
        snapshot,
    )
    .await?;
    let rows = parse_rows(&fetched.body)?;
    let truncated = rows.len() as u32 >= q.limit;
    Ok(SocrataRows {
        rows,
        mode: fetched.mode,
        as_of: fetched.as_of,
        fallback_reason: fetched.fallback_reason,
        url,
        truncated,
    })
}

/// Parse a Socrata JSON response (an array of objects). Socrata reports
/// query errors as a JSON object with `message`/`errorCode`.
pub fn parse_rows(body: &[u8]) -> Result<Vec<Map<String, Value>>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("Socrata returned invalid JSON: {e}")))?;
    match v {
        Value::Array(items) => Ok(items
            .into_iter()
            .filter_map(|i| match i {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .collect()),
        Value::Object(m) => {
            let msg = m
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unexpected response");
            Err(PhnError::Network(format!("Socrata query failed: {msg}")))
        }
        _ => Err(PhnError::Network(
            "Socrata returned an unexpected response".into(),
        )),
    }
}

/// Read a numeric field (Socrata sends numbers as strings). Empty, missing,
/// and non-numeric values are `None`.
pub fn num(row: &Map<String, Value>, field: &str) -> Option<f64> {
    match row.get(field)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', "").parse::<f64>().ok(),
        _ => None,
    }
    .filter(|x| x.is_finite())
}

/// Read a text field.
pub fn text<'a>(row: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
    row.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_encoded_soql_urls() {
        let q = SocrataQuery::cdc("x9gk-5huc")
            .select("year, week, m2")
            .and_where(format!("states = {}", soql_string("MASSACHUSETTS")))
            .and_where("year >= '2024'")
            .order("year DESC")
            .limit(10);
        let url = q.url();
        assert!(url.starts_with("https://data.cdc.gov/resource/x9gk-5huc.json?"));
        assert!(url.contains("%24select=year%2C%20week%2C%20m2"));
        assert!(url.contains(
            "%24where=%28states%20%3D%20%27MASSACHUSETTS%27%29%20AND%20%28year%20%3E%3D%20%272024%27%29"
        ));
        assert!(url.ends_with("%24limit=10"));
        assert_eq!(q.landing_page(), "https://data.cdc.gov/d/x9gk-5huc");
        assert_eq!(soql_string("O'Brien"), "'O''Brien'");
        assert_eq!(SocrataQuery::cdc("a").limit(1_000_000).limit, MAX_LIMIT);
    }

    #[test]
    fn parses_rows_and_string_numbers() {
        let rows = parse_rows(br#"[{"year":"2024","m2":"12","flag":"-"},{"year":"2025","m2":""}]"#)
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(num(&rows[0], "m2"), Some(12.0));
        assert_eq!(num(&rows[1], "m2"), None);
        assert_eq!(text(&rows[0], "flag"), Some("-"));
        assert!(
            parse_rows(br#"{"message":"bad column","errorCode":"query.soql.no-such-column"}"#)
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_cached_block_page_is_ignored_and_the_snapshot_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&phn_core::Config::for_root(dir.path())).unwrap();
        let q = SocrataQuery::cdc("abcd-1234").limit(10);
        // A proxy's HTML page stored by an earlier version, still "fresh".
        let cache = Cache::in_memory().unwrap();
        cache
            .put(&q.url(), b"<html>Access blocked</html>", 3600)
            .unwrap();
        let snap: SnapshotFallback =
            Box::new(|| Some((br#"[{"a":"1"}]"#.to_vec(), "2026-01-01".to_string())));
        let r = query(&http, Some(&cache), &q, 3600, Some(snap))
            .await
            .unwrap();
        assert_eq!(r.mode, SourceMode::Snapshot);
        assert_eq!(r.rows.len(), 1);
        // Without a snapshot the query fails rather than returning the page.
        assert!(query(&http, Some(&cache), &q, 3600, None).await.is_err());
    }

    #[tokio::test]
    async fn offline_query_uses_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = phn_core::Config::for_root(dir.path());
        config.offline = true;
        let http = HttpClient::new(&config).unwrap();
        let q = SocrataQuery::cdc("abcd-1234").limit(2);
        let snap: SnapshotFallback = Box::new(|| {
            Some((
                br#"[{"a":"1"},{"a":"2"}]"#.to_vec(),
                "2026-01-01".to_string(),
            ))
        });
        let r = query(&http, None, &q, 60, Some(snap)).await.unwrap();
        assert_eq!(r.mode, SourceMode::Snapshot);
        assert_eq!(r.rows.len(), 2);
        assert!(r.truncated);
        assert_eq!(r.source("t", "p").retrieved.as_deref(), Some("2026-01-01"));
    }
}
