//! Crossref DOI metadata lookup (`api.crossref.org/works/{doi}`).
//!
//! * The request carries only the DOI, which is public bibliographic
//!   information. No text the nurse supplied is ever sent, and the client name
//!   is the server's generic user agent.
//! * Crossref asks automated clients to identify a contact address ("polite
//!   pool"). The server never sends an address of its own. A nurse who wants
//!   the polite pool sets `PHN_CROSSREF_MAILTO` in the server's environment;
//!   the address is added to the request only (never to the cache key, to a
//!   citation, or to a response).
//! * Responses are cached for one week. A body that is not a Crossref
//!   `status: ok` work record (for example a proxy block page) is never
//!   cached; the caller then falls back to the DOI as entered.
//!
//! The field names follow Crossref's REST API documentation
//! (`message.title`, `message.author`, `message.container-title`,
//! `message.issued.date-parts`, `message.volume`, `message.issue`,
//! `message.page`, `message.type`, `message.publisher`). The host was blocked
//! in the build environment, so the parser is tested against a local server
//! and a response written from that documentation, not against a live reply.

use phn_core::{PhnError, Result, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apifetch::{self, fetch_with_options, FetchOptions};
use crate::cache::Cache;
use crate::http::HttpClient;

/// Base URL of the works endpoint.
pub const BASE: &str = "https://api.crossref.org/works/";
/// Environment variable with the optional contact address for the polite pool.
pub const MAILTO_ENV: &str = "PHN_CROSSREF_MAILTO";
/// Cache time to live: one week.
pub const TTL_SECS: u64 = 7 * 24 * 3600;

/// Bibliographic metadata for one DOI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Work {
    /// The DOI, normalized (no URL prefix).
    pub doi: String,
    pub title: Option<String>,
    /// Journal or book title.
    pub container_title: Option<String>,
    /// Authors as "Family G" (at most 10; `author_count` has the total).
    pub authors: Vec<String>,
    pub author_count: usize,
    /// Year of publication (the earliest of issued, online, or print dates).
    pub year: Option<i32>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    /// Crossref's work type, for example `journal-article`.
    pub work_type: Option<String>,
    /// Link to the record (https://doi.org/...).
    pub url: String,
}

/// A lookup result with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    pub work: Work,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
}

/// Normalize a DOI as a nurse might paste it: trims, removes `doi:` and
/// `https://doi.org/` prefixes, and checks the `10.<registrant>/<suffix>`
/// shape. Returns `None` for anything else.
pub fn normalize_doi(input: &str) -> Option<String> {
    let mut s = input.trim().trim_end_matches(['.', ',', ';']).to_string();
    let lower = s.to_ascii_lowercase();
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi.org/",
        "doi:",
    ] {
        if lower.starts_with(prefix) {
            s = s[prefix.len()..].trim().to_string();
            break;
        }
    }
    let (registrant, suffix) = s.split_once('/')?;
    let reg = registrant.strip_prefix("10.")?;
    let reg_ok = (4..=9).contains(&reg.len()) && reg.bytes().all(|b| b.is_ascii_digit());
    let suffix_ok = !suffix.is_empty()
        && suffix.len() <= 200
        && suffix.chars().all(|c| !c.is_whitespace() && !c.is_control());
    (reg_ok && suffix_ok).then_some(s)
}

/// The works URL for a normalized DOI (characters other than `/` and the
/// unreserved set are percent-encoded; the DOI keeps its own slashes).
pub fn works_url(base: &str, doi: &str) -> String {
    let encoded: Vec<String> = doi.split('/').map(crate::socrata::encode).collect();
    format!("{base}{}", encoded.join("/"))
}

fn first_text(v: Option<&Value>) -> Option<String> {
    let t = match v? {
        Value::Array(a) => a.iter().find_map(|x| x.as_str())?.to_string(),
        Value::String(s) => s.clone(),
        _ => return None,
    };
    let t = strip_markup(&t);
    (!t.is_empty()).then_some(t)
}

/// Crossref titles can carry JATS or HTML tags; keep the text.
fn strip_markup(s: &str) -> String {
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

fn plain(v: Option<&Value>) -> Option<String> {
    let t = match v? {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!t.is_empty()).then_some(t)
}

fn year_of(m: &Value) -> Option<i32> {
    ["issued", "published-print", "published-online", "published"]
        .iter()
        .filter_map(|k| {
            m.get(*k)?
                .get("date-parts")?
                .get(0)?
                .get(0)?
                .as_i64()
                .and_then(|y| i32::try_from(y).ok())
        })
        .filter(|y| (1000..=2200).contains(y))
        .min()
}

/// Parse a Crossref works response body.
pub fn parse_work(body: &[u8]) -> Result<Work> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("Crossref sent a response that is not JSON ({e})")))?;
    if v.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(PhnError::Network(
            "Crossref did not answer with a work record".into(),
        ));
    }
    let m = v
        .get("message")
        .filter(|m| m.is_object())
        .ok_or_else(|| PhnError::Network("Crossref's answer has no work record".into()))?;
    let doi = m
        .get("DOI")
        .and_then(Value::as_str)
        .and_then(normalize_doi)
        .ok_or_else(|| PhnError::Network("Crossref's answer has no DOI".into()))?;
    let authors_all: Vec<String> = m
        .get("author")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let family = p.get("family").and_then(Value::as_str).map(str::trim);
                    let given = p.get("given").and_then(Value::as_str).map(str::trim);
                    match (family, given) {
                        (Some(f), Some(g)) if !f.is_empty() => {
                            let initials: String = g
                                .split(|c: char| c.is_whitespace() || c == '-')
                                .filter_map(|w| w.chars().next())
                                .collect();
                            Some(format!("{f} {initials}").trim().to_string())
                        }
                        (Some(f), _) if !f.is_empty() => Some(f.to_string()),
                        _ => p
                            .get("name")
                            .and_then(Value::as_str)
                            .map(|n| n.trim().to_string())
                            .filter(|n| !n.is_empty()),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Work {
        url: format!("https://doi.org/{doi}"),
        doi,
        title: first_text(m.get("title")),
        container_title: first_text(m.get("container-title")),
        author_count: authors_all.len(),
        authors: authors_all.into_iter().take(10).collect(),
        year: year_of(m),
        volume: plain(m.get("volume")),
        issue: plain(m.get("issue")),
        pages: plain(m.get("page")),
        publisher: plain(m.get("publisher")),
        work_type: plain(m.get("type")),
    })
}

fn expect_work(body: &[u8]) -> Result<()> {
    apifetch::expect_json(body)?;
    parse_work(body).map(|_| ())
}

/// Look up a DOI: fresh cache, then Crossref, then a stale cached copy. There is
/// no bundled snapshot, so a failure is an error the caller turns into the
/// "DOI as entered" fallback.
pub async fn lookup(http: &HttpClient, cache: Option<&Cache>, doi: &str) -> Result<Lookup> {
    lookup_at(http, cache, BASE, doi).await
}

/// [`lookup`] against another base URL (tests).
pub async fn lookup_at(
    http: &HttpClient,
    cache: Option<&Cache>,
    base: &str,
    doi: &str,
) -> Result<Lookup> {
    let normalized = normalize_doi(doi)
        .ok_or_else(|| PhnError::InvalidInput(format!("'{doi}' is not a DOI (10.xxxx/...)")))?;
    let url = works_url(base, &normalized);
    let mailto = apifetch::api_key(MAILTO_ENV);
    let options = FetchOptions {
        key: mailto.as_deref().map(|m| ("mailto", m)),
        ..FetchOptions::default()
    };
    let f = fetch_with_options(http, cache, &url, TTL_SECS, &expect_work, None, options)
        .await
        .map_err(|f| match f.live_status {
            Some(404) => PhnError::NotFound(format!("Crossref has no record of DOI {normalized}")),
            _ => f.error,
        })?;
    Ok(Lookup {
        work: parse_work(&f.body)?,
        mode: f.mode,
        as_of: f.as_of,
        fallback_reason: f.fallback_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_core::Config;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A response shaped as Crossref's documentation describes (written for
    /// this test; it is not a live reply).
    const SAMPLE: &str = r#"{"status":"ok","message-type":"work","message":{
        "DOI":"10.1234/example.5","type":"journal-article","publisher":"Example Press",
        "title":["A <i>fictional</i> trial of a fictional program"],
        "container-title":["Journal of Examples"],
        "author":[{"given":"Ana Maria","family":"Quarry"},{"name":"The Example Group"}],
        "issued":{"date-parts":[[2021,3,29]]},"volume":"12","issue":"3","page":"101-110"}}"#;

    #[test]
    fn dois_are_normalized() {
        for (input, want) in [
            ("10.1136/bmj.n71", Some("10.1136/bmj.n71")),
            (" https://doi.org/10.1136/bmj.n71. ", Some("10.1136/bmj.n71")),
            ("DOI:10.1016/j.jclinepi.2010.07.015", Some("10.1016/j.jclinepi.2010.07.015")),
            ("doi.org/10.1000/xyz123", Some("10.1000/xyz123")),
            ("11.1136/bmj.n71", None),
            ("10.12/short", None),
            ("10.1136/", None),
            ("not a doi", None),
            ("10.1136/has space", None),
        ] {
            assert_eq!(normalize_doi(input).as_deref(), want, "{input}");
        }
    }

    #[test]
    fn url_keeps_slashes_and_encodes_the_rest() {
        assert_eq!(
            works_url(BASE, "10.1002/(SICI)1097-0258(19980430)17:8<873::AID-SIM1>3.0.CO;2-I"),
            "https://api.crossref.org/works/10.1002/%28SICI%291097-0258%2819980430%2917%3A8%3C873%3A%3AAID-SIM1%3E3.0.CO%3B2-I"
        );
    }

    #[test]
    fn parses_a_work_record() {
        let w = parse_work(SAMPLE.as_bytes()).unwrap();
        assert_eq!(w.doi, "10.1234/example.5");
        assert_eq!(
            w.title.as_deref(),
            Some("A fictional trial of a fictional program")
        );
        assert_eq!(w.container_title.as_deref(), Some("Journal of Examples"));
        assert_eq!(w.authors, vec!["Quarry AM", "The Example Group"]);
        assert_eq!(w.author_count, 2);
        assert_eq!(w.year, Some(2021));
        assert_eq!(w.pages.as_deref(), Some("101-110"));
        assert_eq!(w.url, "https://doi.org/10.1234/example.5");
    }

    #[test]
    fn refuses_bodies_that_are_not_work_records() {
        assert!(parse_work(b"<html>blocked</html>").is_err());
        assert!(parse_work(br#"{"status":"failed","message":{}}"#).is_err());
        assert!(parse_work(br#"{"status":"ok","message":{"title":["x"]}}"#).is_err());
    }

    async fn serve(status: &'static str, body: &'static str, hits: usize) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for _ in 0..hits {
                if let Ok((mut sock, _)) = listener.accept().await {
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                }
            }
        });
        format!("http://{addr}/works/")
    }

    fn client() -> (tempfile::TempDir, HttpClient) {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::for_root(dir.path());
        cfg.offline = false;
        let http = HttpClient::new(&cfg).unwrap();
        (dir, http)
    }

    #[tokio::test]
    async fn looks_up_a_doi_live_and_then_from_the_cache() {
        let (_d, http) = client();
        let cache = Cache::in_memory().unwrap();
        let base = serve("200 OK", SAMPLE, 1).await;
        let a = lookup_at(&http, Some(&cache), &base, "https://doi.org/10.1234/example.5")
            .await
            .unwrap();
        assert_eq!(a.mode, SourceMode::Live);
        assert_eq!(a.work.year, Some(2021));
        // The server answered once; the second lookup must come from the cache.
        let b = lookup_at(&http, Some(&cache), &base, "10.1234/example.5")
            .await
            .unwrap();
        assert_eq!(b.mode, SourceMode::Cache);
    }

    #[tokio::test]
    async fn a_missing_doi_and_a_block_page_are_errors() {
        let (_d, http) = client();
        let base = serve("404 Not Found", "Resource not found.", 1).await;
        let e = lookup_at(&http, None, &base, "10.1234/nothere")
            .await
            .unwrap_err();
        assert!(matches!(e, PhnError::NotFound(_)), "{e}");
        let base = serve("200 OK", "<html>blocked</html>", 1).await;
        assert!(lookup_at(&http, None, &base, "10.1234/example.5").await.is_err());
        assert!(matches!(
            lookup_at(&http, None, BASE, "nonsense").await.unwrap_err(),
            PhnError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn offline_mode_fails_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::for_root(dir.path());
        cfg.offline = true;
        let http = HttpClient::new(&cfg).unwrap();
        assert!(lookup(&http, None, "10.1234/example.5").await.is_err());
    }
}
