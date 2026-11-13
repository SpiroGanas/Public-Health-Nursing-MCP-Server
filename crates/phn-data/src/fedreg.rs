//! Federal Register API v1 (`www.federalregister.gov/api/v1`) and the
//! Regulations.gov API v4 document lookup.
//!
//! * The Federal Register API needs no key. Requests carry only agency
//!   slugs, document types, dates, and search words the nurse typed about a
//!   topic; never personal data. The client name is the server's generic
//!   user agent.
//! * Regulations.gov (`api.regulations.gov/v4`) needs a free api.data.gov key.
//!   It is read from `PHN_REGULATIONS_GOV_API_KEY` and added to the request
//!   URL only; it is never part of the cache key, a citation, or a response.
//!   Without the key the Regulations.gov step is skipped.
//!
//! Both hosts were blocked in the build environment (HTTP 403 on the proxy
//! CONNECT), so the parsers were written from the Federal Register's own open
//! source (the `federalregister-api-core` repository: its serializers list the
//! API fields, its search class lists the `conditions[...]` filters) and from
//! Regulations.gov v4 responses recorded in that repository's test suite. They
//! are tested against local servers and responses written from those sources,
//! not against a live reply.

use phn_core::{PhnError, Result, SourceMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apifetch::{self, FetchOptions, fetch_with_options};
use crate::cache::Cache;
use crate::http::HttpClient;

/// Base URL of the Federal Register API.
pub const BASE: &str = "https://www.federalregister.gov/api/v1";
/// Base URL of the Regulations.gov API.
pub const REGULATIONS_BASE: &str = "https://api.regulations.gov/v4";
/// Optional key for Regulations.gov (api.data.gov).
pub const REGULATIONS_KEY_ENV: &str = "PHN_REGULATIONS_GOV_API_KEY";
/// Cache time to live for searches: three hours (the Federal Register
/// publishes once a business day).
pub const TTL_SECS: u64 = 3 * 3600;
/// A cached search older than this is not used when the live request fails.
pub const MAX_STALE_SECS: u64 = 2 * 24 * 3600;
/// Most documents requested per search.
pub const MAX_PER_PAGE: u32 = 50;

/// The fields requested from the API (all are in the API's field list).
const FIELDS: &[&str] = &[
    "title",
    "type",
    "abstract",
    "action",
    "dates",
    "document_number",
    "publication_date",
    "comments_close_on",
    "effective_on",
    "agencies",
    "html_url",
    "pdf_url",
    "comment_url",
    "regulations_dot_gov_url",
    "docket_ids",
];

/// A Federal Register search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Agency slugs (for example `environmental-protection-agency`).
    pub agencies: Vec<String>,
    /// Document types: `RULE`, `PRORULE`, `NOTICE`.
    pub types: Vec<String>,
    /// Search words.
    pub term: Option<String>,
    /// Published on or after (`YYYY-MM-DD`).
    pub published_gte: Option<String>,
    /// Published on or before.
    pub published_lte: Option<String>,
    /// Comment period closes on or after (`YYYY-MM-DD`): documents still open.
    pub comments_close_gte: Option<String>,
    /// Only documents accepting comments on Regulations.gov.
    pub accepting_comments: bool,
    /// Documents per page (at most [`MAX_PER_PAGE`]).
    pub per_page: u32,
}

/// One agency named on a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocAgency {
    pub name: String,
    pub slug: Option<String>,
}

/// One Federal Register document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub document_number: String,
    pub title: String,
    /// "Proposed Rule", "Rule", "Notice", and so on (the API's label).
    pub doc_type: String,
    pub abstract_text: Option<String>,
    pub action: Option<String>,
    /// The document's own description of its dates.
    pub dates_text: Option<String>,
    pub publication_date: Option<String>,
    pub comments_close_on: Option<String>,
    pub effective_on: Option<String>,
    pub agencies: Vec<DocAgency>,
    pub html_url: Option<String>,
    pub pdf_url: Option<String>,
    pub comment_url: Option<String>,
    pub regulations_dot_gov_url: Option<String>,
    pub docket_ids: Vec<String>,
}

/// A page of search results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    /// Total documents matching the search.
    pub count: u64,
    pub documents: Vec<Document>,
}

/// A search result with where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Searched {
    pub page: SearchPage,
    pub mode: SourceMode,
    pub as_of: String,
    /// Set when a cached copy was used because the live request failed.
    pub fallback_reason: Option<String>,
}

fn enc(s: &str) -> String {
    crate::socrata::encode(s)
}

/// The request URL for a search.
pub fn search_url(base: &str, q: &Query) -> String {
    let mut parts: Vec<String> = vec![
        format!("per_page={}", q.per_page.clamp(1, MAX_PER_PAGE)),
        "order=newest".into(),
    ];
    for f in FIELDS {
        parts.push(format!("fields%5B%5D={f}"));
    }
    for a in &q.agencies {
        parts.push(format!("conditions%5Bagencies%5D%5B%5D={}", enc(a)));
    }
    for t in &q.types {
        parts.push(format!("conditions%5Btype%5D%5B%5D={}", enc(t)));
    }
    if let Some(t) = &q.term {
        parts.push(format!("conditions%5Bterm%5D={}", enc(t)));
    }
    if let Some(d) = &q.published_gte {
        parts.push(format!(
            "conditions%5Bpublication_date%5D%5Bgte%5D={}",
            enc(d)
        ));
    }
    if let Some(d) = &q.published_lte {
        parts.push(format!(
            "conditions%5Bpublication_date%5D%5Blte%5D={}",
            enc(d)
        ));
    }
    if let Some(d) = &q.comments_close_gte {
        parts.push(format!("conditions%5Bcomment_date%5D%5Bgte%5D={}", enc(d)));
    }
    if q.accepting_comments {
        parts.push("conditions%5Baccepting_comments_on_regulations_dot_gov%5D=1".into());
    }
    format!("{base}/documents.json?{}", parts.join("&"))
}

/// The URL of one document's record.
pub fn document_url(base: &str, document_number: &str) -> String {
    format!(
        "{base}/documents/{}.json?{}",
        enc(document_number),
        FIELDS
            .iter()
            .map(|f| format!("fields%5B%5D={f}"))
            .collect::<Vec<_>>()
            .join("&")
    )
}

fn text(v: Option<&Value>) -> Option<String> {
    let t = v?
        .as_str()?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!t.is_empty()).then_some(t)
}

/// A `YYYY-MM-DD` date between 1900 and 2100, or `None`.
pub fn valid_date(s: &str) -> Option<String> {
    let head: String = s.trim().chars().take(10).collect();
    let d = chrono::NaiveDate::parse_from_str(&head, "%Y-%m-%d").ok()?;
    let year = chrono::Datelike::year(&d);
    ((1900..=2100).contains(&year) && head.len() == 10).then_some(head)
}

/// Remove markup tags from a short text field (abstracts can carry HTML).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_document(v: &Value) -> Option<Document> {
    let document_number = text(v.get("document_number"))?;
    let title = text(v.get("title")).map(|t| strip_tags(&t))?;
    let agencies = v
        .get("agencies")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let name = text(x.get("name")).or_else(|| text(x.get("raw_name")))?;
                    Some(DocAgency {
                        name,
                        slug: text(x.get("slug")),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let docket_ids = v
        .get("docket_ids")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| text(Some(x))).collect())
        .unwrap_or_default();
    let date = |k: &str| v.get(k).and_then(Value::as_str).and_then(valid_date);
    Some(Document {
        document_number,
        title,
        doc_type: text(v.get("type")).unwrap_or_else(|| "Document".into()),
        abstract_text: text(v.get("abstract")).map(|t| strip_tags(&t)),
        action: text(v.get("action")).map(|t| strip_tags(&t)),
        dates_text: text(v.get("dates")).map(|t| strip_tags(&t)),
        publication_date: date("publication_date"),
        comments_close_on: date("comments_close_on"),
        effective_on: date("effective_on"),
        agencies,
        html_url: text(v.get("html_url")),
        pdf_url: text(v.get("pdf_url")),
        comment_url: text(v.get("comment_url")),
        regulations_dot_gov_url: text(v.get("regulations_dot_gov_url")),
        docket_ids,
    })
}

/// Parse a `documents.json` response (`count`, `results`).
pub fn parse_search(body: &[u8]) -> Result<SearchPage> {
    let v: Value = serde_json::from_slice(body).map_err(|e| {
        PhnError::Network(format!(
            "the Federal Register sent a response that is not JSON ({e})"
        ))
    })?;
    let results = v.get("results").and_then(Value::as_array).ok_or_else(|| {
        PhnError::Network("the Federal Register's answer has no results list".into())
    })?;
    let documents: Vec<Document> = results.iter().filter_map(parse_document).collect();
    let count = v
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(documents.len() as u64);
    Ok(SearchPage { count, documents })
}

/// Parse one document's record (`documents/{number}.json`).
pub fn parse_one(body: &[u8]) -> Result<Document> {
    let v: Value = serde_json::from_slice(body).map_err(|e| {
        PhnError::Network(format!(
            "the Federal Register sent a response that is not JSON ({e})"
        ))
    })?;
    parse_document(&v)
        .ok_or_else(|| PhnError::Network("the Federal Register's answer is not a document".into()))
}

fn expect_search(body: &[u8]) -> Result<()> {
    apifetch::expect_json(body)?;
    parse_search(body).map(|_| ())
}

fn expect_one(body: &[u8]) -> Result<()> {
    apifetch::expect_json(body)?;
    parse_one(body).map(|_| ())
}

fn options() -> FetchOptions<'static> {
    FetchOptions {
        max_stale_secs: Some(MAX_STALE_SECS),
        ..FetchOptions::default()
    }
}

/// Search the Federal Register: fresh cache, live, then a cached copy no
/// older than [`MAX_STALE_SECS`]. There is no bundled snapshot; a failure is
/// an error the caller reports as "live data unavailable".
pub async fn search(
    http: &HttpClient,
    cache: Option<&Cache>,
    base: &str,
    q: &Query,
) -> Result<Searched> {
    let url = search_url(base, q);
    let f = fetch_with_options(http, cache, &url, TTL_SECS, &expect_search, None, options())
        .await
        .map_err(|f| f.error)?;
    Ok(Searched {
        page: parse_search(&f.body)?,
        mode: f.mode,
        as_of: f.as_of,
        fallback_reason: f.fallback_reason,
    })
}

/// One document by its Federal Register document number (for example
/// `2024-12345`).
pub async fn fetch_document(
    http: &HttpClient,
    cache: Option<&Cache>,
    base: &str,
    document_number: &str,
) -> Result<(Document, SourceMode, String, Option<String>)> {
    let n = document_number.trim();
    let ok = !n.is_empty()
        && n.len() <= 20
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(PhnError::InvalidInput(format!(
            "'{document_number}' is not a Federal Register document number (for example 2024-12345)"
        )));
    }
    let url = document_url(base, n);
    let f = fetch_with_options(http, cache, &url, TTL_SECS, &expect_one, None, options())
        .await
        .map_err(|f| match f.live_status {
            Some(404) => {
                PhnError::NotFound(format!("the Federal Register has no document numbered {n}"))
            }
            _ => f.error,
        })?;
    Ok((parse_one(&f.body)?, f.mode, f.as_of, f.fallback_reason))
}

// ---------------------------------------------------------------- Regulations.gov

/// A Regulations.gov document record (looked up by Federal Register number).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegsDocument {
    /// For example `FRTIB_FRDOC_0001-0319`.
    pub document_id: String,
    pub docket_id: Option<String>,
    pub title: Option<String>,
    pub open_for_comment: Option<bool>,
    pub withdrawn: Option<bool>,
    /// Start and end of the comment period as Regulations.gov gives them (UTC).
    pub comment_start: Option<String>,
    pub comment_end: Option<String>,
    /// The page where a comment is entered.
    pub comment_page: String,
}

/// Parse a `v4/documents?filter[frDocNum]=...` response. `None` when
/// Regulations.gov has no matching document.
pub fn parse_regs(body: &[u8]) -> Result<Option<RegsDocument>> {
    let v: Value = serde_json::from_slice(body).map_err(|e| {
        PhnError::Network(format!(
            "Regulations.gov sent a response that is not JSON ({e})"
        ))
    })?;
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network("Regulations.gov's answer has no data list".into()))?;
    let Some(first) = data.first() else {
        return Ok(None);
    };
    let id = text(first.get("id"))
        .ok_or_else(|| PhnError::Network("Regulations.gov's document has no id".into()))?;
    let a = first.get("attributes").unwrap_or(&Value::Null);
    let ts = |k: &str| {
        let t = a.get(k)?.as_str()?;
        // Keep the full UTC time stamp; the date part must be a real date.
        valid_date(t)?;
        Some(t.trim().to_string())
    };
    Ok(Some(RegsDocument {
        comment_page: format!("https://www.regulations.gov/commenton/{id}"),
        document_id: id,
        docket_id: text(a.get("docketId")),
        title: text(a.get("title")),
        open_for_comment: a.get("openForComment").and_then(Value::as_bool),
        withdrawn: a.get("withdrawn").and_then(Value::as_bool),
        comment_start: ts("commentStartDate"),
        comment_end: ts("commentEndDate"),
    }))
}

fn expect_regs(body: &[u8]) -> Result<()> {
    apifetch::expect_json(body)?;
    parse_regs(body).map(|_| ())
}

/// The optional Regulations.gov key from the environment (blank is none).
pub fn regulations_key() -> Option<String> {
    apifetch::api_key(REGULATIONS_KEY_ENV)
}

/// Look up a document on Regulations.gov by its Federal Register number. The
/// key goes on the request only. `Ok(None)` when there is no such document.
pub async fn regulations_lookup(
    http: &HttpClient,
    cache: Option<&Cache>,
    base: &str,
    key: &str,
    fr_document_number: &str,
) -> Result<Option<RegsDocument>> {
    let url = format!(
        "{base}/documents?filter%5BfrDocNum%5D={}",
        enc(fr_document_number)
    );
    let opts = FetchOptions {
        key: Some(("api_key", key)),
        max_stale_secs: Some(MAX_STALE_SECS),
        ..FetchOptions::default()
    };
    let f = fetch_with_options(http, cache, &url, TTL_SECS, &expect_regs, None, opts)
        .await
        .map_err(|f| f.error)?;
    parse_regs(&f.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_core::Config;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const SEARCH: &str = r#"{"count":2,"description":"x","total_pages":1,"next_page_url":null,
      "results":[
        {"title":"Proposed <i>Rule</i> on lead in drinking water","type":"Proposed Rule",
         "abstract":"EPA proposes   changes.","action":"Proposed rule.","dates":"Comments are due 2026-12-14.",
         "document_number":"2026-20001","publication_date":"2026-10-15","comments_close_on":"2026-12-14",
         "effective_on":null,"agencies":[{"raw_name":"ENVIRONMENTAL PROTECTION AGENCY","name":"Environmental Protection Agency","slug":"environmental-protection-agency"}],
         "html_url":"https://www.federalregister.gov/documents/2026/10/15/2026-20001/x",
         "pdf_url":"https://www.govinfo.gov/x.pdf","comment_url":"https://www.regulations.gov/commenton/EPA-HQ-OW-2026-0001-0001",
         "regulations_dot_gov_url":"https://www.regulations.gov/document/EPA-HQ-OW-2026-0001-0001","docket_ids":["EPA-HQ-OW-2026-0001"]},
        {"title":"A notice","type":"Notice","document_number":"2026-20002","publication_date":"0026-10-15",
         "comments_close_on":null,"agencies":[],"docket_ids":[]},
        {"title":"no number"}
      ]}"#;
    const REGS: &str = r#"{"data":[{"id":"EPA-HQ-OW-2026-0001-0001","type":"documents",
      "attributes":{"docketId":"EPA-HQ-OW-2026-0001","documentType":"Proposed Rule","openForComment":true,
      "withdrawn":false,"commentStartDate":"2026-10-15T04:00:00Z","commentEndDate":"2026-12-15T04:59:59Z",
      "title":"Lead in drinking water"},"links":{"self":"x"}}],"meta":{"totalElements":1}}"#;

    #[test]
    fn builds_search_urls_without_keys_and_with_encoded_brackets() {
        let q = Query {
            agencies: vec!["environmental-protection-agency".into()],
            types: vec!["PRORULE".into(), "NOTICE".into()],
            term: Some("lead & copper".into()),
            comments_close_gte: Some("2026-11-12".into()),
            accepting_comments: true,
            per_page: 500,
            ..Query::default()
        };
        let u = search_url(BASE, &q);
        assert!(u.starts_with(
            "https://www.federalregister.gov/api/v1/documents.json?per_page=50&order=newest"
        ));
        assert!(u.contains("conditions%5Bagencies%5D%5B%5D=environmental-protection-agency"));
        assert!(u.contains("conditions%5Btype%5D%5B%5D=PRORULE&conditions%5Btype%5D%5B%5D=NOTICE"));
        assert!(u.contains("conditions%5Bterm%5D=lead%20%26%20copper"));
        assert!(u.contains("conditions%5Bcomment_date%5D%5Bgte%5D=2026-11-12"));
        assert!(u.contains("conditions%5Baccepting_comments_on_regulations_dot_gov%5D=1"));
        assert!(!u.contains('[') && !u.contains("key"));
        assert!(document_url(BASE, "2026-20001").contains("/documents/2026-20001.json?"));
    }

    #[test]
    fn parses_search_results_and_drops_bad_dates() {
        let p = parse_search(SEARCH.as_bytes()).unwrap();
        assert_eq!(p.count, 2);
        assert_eq!(
            p.documents.len(),
            2,
            "the entry without a number is dropped"
        );
        let d = &p.documents[0];
        assert_eq!(d.title, "Proposed Rule on lead in drinking water");
        assert_eq!(d.abstract_text.as_deref(), Some("EPA proposes changes."));
        assert_eq!(d.comments_close_on.as_deref(), Some("2026-12-14"));
        assert_eq!(
            d.agencies[0].slug.as_deref(),
            Some("environmental-protection-agency")
        );
        assert_eq!(d.docket_ids, vec!["EPA-HQ-OW-2026-0001"]);
        assert_eq!(
            p.documents[1].publication_date, None,
            "year 0026 is rejected"
        );
        assert!(parse_search(b"<html>blocked</html>").is_err());
        assert!(parse_search(br#"{"count":0}"#).is_err());
        assert!(parse_one(br#"{"title":"x"}"#).is_err());
    }

    #[test]
    fn parses_regulations_gov_records() {
        let r = parse_regs(REGS.as_bytes()).unwrap().unwrap();
        assert_eq!(r.document_id, "EPA-HQ-OW-2026-0001-0001");
        assert_eq!(r.open_for_comment, Some(true));
        assert_eq!(r.comment_end.as_deref(), Some("2026-12-15T04:59:59Z"));
        assert_eq!(
            r.comment_page,
            "https://www.regulations.gov/commenton/EPA-HQ-OW-2026-0001-0001"
        );
        assert_eq!(parse_regs(br#"{"data":[],"meta":{}}"#).unwrap(), None);
        assert!(parse_regs(b"<html/>").is_err());
    }

    async fn serve(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        tokio::spawn(async move {
            let mut i = 0;
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                seen2
                    .lock()
                    .unwrap()
                    .push(req.lines().next().unwrap_or("").to_string());
                let (status, body) = responses[i.min(responses.len() - 1)];
                i += 1;
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    fn client() -> (tempfile::TempDir, HttpClient) {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::for_root(dir.path());
        cfg.offline = false;
        (dir, HttpClient::new(&cfg).unwrap())
    }

    #[tokio::test]
    async fn searches_live_then_cache_and_reports_failures() {
        let (_d, http) = client();
        let cache = Cache::in_memory().unwrap();
        let (base, _) = serve(vec![("200 OK", SEARCH)]).await;
        let q = Query {
            per_page: 10,
            ..Query::default()
        };
        let a = search(&http, Some(&cache), &base, &q).await.unwrap();
        assert_eq!(a.mode, SourceMode::Live);
        let b = search(&http, Some(&cache), &base, &q).await.unwrap();
        assert_eq!(b.mode, SourceMode::Cache);
        assert!(b.fallback_reason.is_none());
        let (base2, _) = serve(vec![("200 OK", "<html>blocked</html>")]).await;
        assert!(search(&http, None, &base2, &q).await.is_err());
        let (base3, _) = serve(vec![("404 Not Found", "x")]).await;
        let e = fetch_document(&http, None, &base3, "2026-99999")
            .await
            .unwrap_err();
        assert!(matches!(e, PhnError::NotFound(_)), "{e}");
        assert!(matches!(
            fetch_document(&http, None, &base3, "../x")
                .await
                .unwrap_err(),
            PhnError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn regulations_key_goes_on_the_request_only() {
        let (_d, http) = client();
        let cache = Cache::in_memory().unwrap();
        let (base, seen) = serve(vec![("200 OK", REGS)]).await;
        let r = regulations_lookup(&http, Some(&cache), &base, "SECRETKEY123", "2026-20001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(r.docket_id.as_deref(), Some("EPA-HQ-OW-2026-0001"));
        assert!(seen.lock().unwrap()[0].contains("api_key=SECRETKEY123"));
        // The cache entry is keyed without the key, so the second call needs no request.
        let again = regulations_lookup(&http, Some(&cache), &base, "OTHERKEY", "2026-20001")
            .await
            .unwrap();
        assert!(again.is_some());
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn offline_fails_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::for_root(dir.path());
        cfg.offline = true;
        let http = HttpClient::new(&cfg).unwrap();
        assert!(
            search(
                &http,
                None,
                BASE,
                &Query {
                    per_page: 5,
                    ..Query::default()
                }
            )
            .await
            .is_err()
        );
    }

    /// Live check (run with `cargo test -- --ignored`).
    #[tokio::test]
    #[ignore = "needs network access to www.federalregister.gov"]
    async fn live_search() {
        let (_d, http) = client();
        let q = Query {
            agencies: vec!["environmental-protection-agency".into()],
            per_page: 3,
            ..Query::default()
        };
        let r = search(&http, None, BASE, &q).await.unwrap();
        assert!(!r.page.documents.is_empty());
    }
}
