//! Live → cache → snapshot fetching for public APIs that take an optional
//! key and can answer with an error page instead of data.
//!
//! [`crate::fetch::fetch_with_fallback`] caches whatever the server returns
//! and uses the request URL as the cache key. Two public APIs need more care:
//!
//! * the **API key** (for example `PHN_CENSUS_API_KEY` or `PHN_NCBI_API_KEY`)
//!   is added only to the request URL. The cache key and every cited URL are
//!   built without it, so the key never reaches the cache, a response, or the
//!   AI vendor;
//! * the **body is validated** before it is cached, so an HTML error page (the
//!   Census API redirects keyless requests from some networks to
//!   `missing_key.html`) is reported as a failure and the chain falls back to
//!   the cache or the bundled snapshot.
//!
//! [`fetch_with_options`] adds limits on how old a cached copy may be (weather
//! alerts must never come from an old copy) and reports the HTTP status of a
//! failed live request.

use phn_core::{PhnError, Result, SourceMode};

use crate::cache::Cache;
use crate::fetch::{Fetched, SnapshotFallback};
use crate::http::HttpClient;

/// Read an optional API key from the environment. Blank values are ignored.
pub fn api_key(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Append `name=value` to a URL's query string.
pub fn with_param(url: &str, name: &str, value: &str) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!(
        "{url}{sep}{}={}",
        crate::socrata::encode(name),
        crate::socrata::encode(value)
    )
}

/// Extra controls for [`fetch_with_options`].
#[derive(Debug, Clone, Copy, Default)]
pub struct FetchOptions<'a> {
    /// API key as (query parameter, value). It is added to the request URL
    /// only, never to the cache key.
    pub key: Option<(&'a str, &'a str)>,
    /// When the live request fails, a cached copy older than this many
    /// seconds is not used (for time-critical feeds such as weather alerts).
    pub max_stale_secs: Option<u64>,
    /// Cached copies fetched before this time are not used, fresh or stale
    /// (the publisher has updated the data since then).
    pub not_before: Option<chrono::DateTime<chrono::Utc>>,
}

/// A fetch that produced no usable body.
#[derive(Debug)]
pub struct FetchFailure {
    /// What went wrong (never contains the API key).
    pub error: PhnError,
    /// HTTP status of the live request (for example 404), when the server
    /// answered with a non-success status.
    pub live_status: Option<u16>,
}

/// Fetch `public_url` (plus `key_param=key` when a key is given) through the
/// live → cache → snapshot chain. The cache key is `public_url`; `validate`
/// must accept the body before it is cached or returned as live data.
pub async fn fetch_validated(
    http: &HttpClient,
    cache: Option<&Cache>,
    public_url: &str,
    key: Option<(&str, &str)>,
    ttl_secs: u64,
    validate: &(dyn Fn(&[u8]) -> Result<()> + Sync),
    snapshot: Option<SnapshotFallback>,
) -> Result<Fetched> {
    let options = FetchOptions {
        key,
        ..FetchOptions::default()
    };
    fetch_with_options(
        http, cache, public_url, ttl_secs, validate, snapshot, options,
    )
    .await
    .map_err(|f| f.error)
}

/// [`fetch_validated`] with [`FetchOptions`]. A failure also carries the HTTP
/// status of the live request.
pub async fn fetch_with_options(
    http: &HttpClient,
    cache: Option<&Cache>,
    public_url: &str,
    ttl_secs: u64,
    validate: &(dyn Fn(&[u8]) -> Result<()> + Sync),
    snapshot: Option<SnapshotFallback>,
    options: FetchOptions<'_>,
) -> std::result::Result<Fetched, FetchFailure> {
    // A cache read error is a miss, not a failure. A copy fetched before
    // `not_before` is out of date and is ignored.
    let cached = cache
        .and_then(|c| c.get_or_miss(public_url))
        .filter(|hit| options.not_before.is_none_or(|t| hit.fetched_at >= t));
    if let Some(hit) = &cached
        && hit.fresh
        && validate(&hit.body).is_ok()
    {
        return Ok(Fetched {
            body: hit.body.clone(),
            mode: SourceMode::Cache,
            as_of: hit.fetched_at.to_rfc3339(),
            fallback_reason: None,
        });
    }

    let request_url = match options.key {
        Some((param, value)) => with_param(public_url, param, value),
        None => public_url.to_string(),
    };
    let mut live_status = None;
    let live_error = match http.get_bytes_status(&request_url).await {
        Ok(body) => match validate(&body) {
            Ok(()) => {
                if let Some(c) = cache
                    && let Err(e) = c.put(public_url, &body, ttl_secs)
                {
                    tracing::warn!(error = %e, "could not write HTTP cache");
                }
                return Ok(Fetched {
                    body,
                    mode: SourceMode::Live,
                    as_of: chrono::Utc::now().to_rfc3339(),
                    fallback_reason: None,
                });
            }
            Err(e) => reason(&e),
        },
        Err(failure) => {
            live_status = failure.status;
            // Never echo a URL that carries the key.
            scrub(&failure.message, options.key.map(|k| k.1))
        }
    };

    let mut too_old = false;
    if let Some(hit) = cached
        && validate(&hit.body).is_ok()
    {
        let age = chrono::Utc::now()
            .signed_duration_since(hit.fetched_at)
            .num_seconds();
        if options
            .max_stale_secs
            .is_none_or(|max| age <= i64::try_from(max).unwrap_or(i64::MAX))
        {
            return Ok(Fetched {
                body: hit.body,
                mode: SourceMode::Cache,
                as_of: hit.fetched_at.to_rfc3339(),
                fallback_reason: Some(format!(
                    "live request failed ({live_error}); using cached copy"
                )),
            });
        }
        too_old = true;
    }
    let had_snapshot = snapshot.is_some();
    if let Some((body, date)) = snapshot.and_then(|f| f()) {
        return Ok(Fetched {
            body,
            mode: SourceMode::Snapshot,
            as_of: date,
            fallback_reason: Some(format!(
                "live request failed ({live_error}); using bundled snapshot"
            )),
        });
    }
    let message = if too_old {
        format!("{live_error}; the cached copy is too old to use")
    } else {
        format!(
            "{live_error}; no cached copy{} is available",
            if had_snapshot { " or snapshot" } else { "" }
        )
    };
    Err(FetchFailure {
        error: PhnError::Network(message),
        live_status,
    })
}

/// The message of an error without its "network error:" prefix, for nesting
/// in another message.
pub fn reason(e: &PhnError) -> String {
    match e {
        PhnError::Network(m) => m.clone(),
        other => other.to_string(),
    }
}

/// Remove a secret from a message.
pub fn scrub(message: &str, secret: Option<&str>) -> String {
    match secret {
        Some(s) if !s.is_empty() => message
            .replace(s, "[key]")
            .replace(&crate::socrata::encode(s), "[key]"),
        _ => message.to_string(),
    }
}

/// Validator: the body must be JSON (not an HTML error page).
pub fn expect_json(body: &[u8]) -> Result<()> {
    let start = body.iter().find(|b| !b.is_ascii_whitespace()).copied();
    match start {
        Some(b'[') | Some(b'{') => serde_json::from_slice::<serde_json::Value>(body)
            .map(|_| ())
            .map_err(|e| PhnError::Network(format!("the response was not valid JSON ({e})"))),
        Some(b'<') => Err(PhnError::Network(
            "the server returned a web page instead of data".into(),
        )),
        _ => Err(PhnError::Network(
            "the response was empty or not JSON".into(),
        )),
    }
}

/// Validator: the body must be an XML document.
pub fn expect_xml(body: &[u8]) -> Result<()> {
    let text = String::from_utf8_lossy(&body[..body.len().min(512)]).to_ascii_lowercase();
    if text.trim_start().starts_with("<?xml") || text.contains("<esearchresult") {
        Ok(())
    } else {
        Err(PhnError::Network("the response was not XML".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_core::Config;

    #[test]
    fn params_and_scrubbing() {
        assert_eq!(
            with_param("https://x/a", "key", "k 1"),
            "https://x/a?key=k%201"
        );
        assert_eq!(
            with_param("https://x/a?b=1", "key", "k"),
            "https://x/a?b=1&key=k"
        );
        assert_eq!(
            scrub("GET https://x/a?key=SECRET1: 403", Some("SECRET1")),
            "GET https://x/a?key=[key]: 403"
        );
        assert!(expect_json(b"[[\"NAME\"]]").is_ok());
        assert!(expect_json(b"<html>missing key</html>").is_err());
        assert!(expect_xml(b"<?xml version=\"1.0\"?><a/>").is_ok());
        assert!(expect_xml(b"{}").is_err());
    }

    /// Serve every request with the same response.
    async fn serve(status_line: &'static str, body: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        format!("http://{addr}/data")
    }

    fn online() -> HttpClient {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::for_root(dir.path());
        config.offline = false;
        HttpClient::new(&config).unwrap()
    }

    #[tokio::test]
    async fn a_block_page_is_not_cached_and_the_snapshot_is_used() {
        // A proxy answers 200 with an HTML page instead of data.
        let url = serve("200 OK", "<html>blocked by policy</html>").await;
        let http = online();
        let cache = Cache::in_memory().unwrap();
        let snap: SnapshotFallback = Box::new(|| Some((b"[1]".to_vec(), "2026-01-01".into())));
        let r = fetch_validated(
            &http,
            Some(&cache),
            &url,
            None,
            3600,
            &expect_json,
            Some(snap),
        )
        .await
        .unwrap();
        assert_eq!(r.mode, SourceMode::Snapshot);
        assert_eq!(r.body, b"[1]");
        assert!(r.fallback_reason.unwrap().contains("web page"));
        // Nothing was stored, so the next call tries the network again.
        assert!(cache.get(&url).unwrap().is_none());
    }

    #[tokio::test]
    async fn failures_carry_the_live_http_status_without_the_key() {
        let url = serve("404 Not Found", "").await;
        let http = online();
        let options = FetchOptions {
            key: Some(("api_key", "SECRET404")),
            ..FetchOptions::default()
        };
        let failure = fetch_with_options(&http, None, &url, 60, &expect_json, None, options)
            .await
            .unwrap_err();
        assert_eq!(failure.live_status, Some(404));
        assert!(!failure.error.to_string().contains("SECRET404"));

        // Offline: no status, although the key contains "404".
        let dir = tempfile::tempdir().unwrap();
        let offline = HttpClient::new(&Config::for_root(dir.path())).unwrap();
        let failure = fetch_with_options(&offline, None, &url, 60, &expect_json, None, options)
            .await
            .unwrap_err();
        assert_eq!(failure.live_status, None);
    }

    #[tokio::test]
    async fn stale_copies_past_the_limit_and_outdated_copies_are_not_used() {
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&Config::for_root(dir.path())).unwrap();
        let cache = Cache::in_memory().unwrap();
        let fetched = chrono::Utc::now() - chrono::Duration::hours(2);
        cache
            .put_at("https://x/alerts", b"[]", 300, fetched)
            .unwrap();

        // Two hours old with a one-hour limit: refused.
        let strict = FetchOptions {
            max_stale_secs: Some(3600),
            ..FetchOptions::default()
        };
        let failure = fetch_with_options(
            &http,
            Some(&cache),
            "https://x/alerts",
            300,
            &expect_json,
            None,
            strict,
        )
        .await
        .unwrap_err();
        assert!(failure.error.to_string().contains("too old"));

        // Within a three-hour limit: used, and labeled as a fallback.
        let lenient = FetchOptions {
            max_stale_secs: Some(3 * 3600),
            ..FetchOptions::default()
        };
        let r = fetch_with_options(
            &http,
            Some(&cache),
            "https://x/alerts",
            300,
            &expect_json,
            None,
            lenient,
        )
        .await
        .unwrap();
        assert_eq!(r.mode, SourceMode::Cache);
        assert!(r.fallback_reason.is_some());

        // The publisher updated the data after the copy was fetched: ignored.
        let outdated = FetchOptions {
            not_before: Some(fetched + chrono::Duration::minutes(30)),
            ..FetchOptions::default()
        };
        let failure = fetch_with_options(
            &http,
            Some(&cache),
            "https://x/alerts",
            300,
            &expect_json,
            None,
            outdated,
        )
        .await
        .unwrap_err();
        assert!(failure.error.to_string().contains("no cached copy"));
    }

    #[tokio::test]
    async fn invalid_cached_bodies_are_not_used_and_snapshot_is() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::for_root(dir.path());
        let http = HttpClient::new(&config).unwrap();
        let cache = Cache::in_memory().unwrap();
        cache
            .put("https://x/a", b"<html>error</html>", 3600)
            .unwrap();
        let snap: SnapshotFallback = Box::new(|| Some((b"[]".to_vec(), "2026-01-01".into())));
        let r = fetch_validated(
            &http,
            Some(&cache),
            "https://x/a",
            Some(("key", "SECRET")),
            60,
            &expect_json,
            Some(snap),
        )
        .await
        .unwrap();
        assert_eq!(r.mode, SourceMode::Snapshot);
        assert!(!r.fallback_reason.unwrap().contains("SECRET"));

        // A cache that cannot be read is a miss, not an error.
        cache.damage_for_tests();
        let snap: SnapshotFallback = Box::new(|| Some((b"[]".to_vec(), "2026-01-01".into())));
        let r = fetch_validated(
            &http,
            Some(&cache),
            "https://x/a",
            None,
            60,
            &expect_json,
            Some(snap),
        )
        .await
        .unwrap();
        assert_eq!(r.mode, SourceMode::Snapshot);
    }
}
