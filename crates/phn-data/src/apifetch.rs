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
    let cached = match cache {
        Some(c) => c.get(public_url)?,
        None => None,
    };
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

    let request_url = match key {
        Some((param, value)) => with_param(public_url, param, value),
        None => public_url.to_string(),
    };
    let live_error = match http.get_bytes(&request_url).await {
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
        // Never echo a URL that carries the key.
        Err(e) => scrub(&reason(&e), key.map(|k| k.1)),
    };

    if let Some(hit) = cached
        && validate(&hit.body).is_ok()
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
    Err(PhnError::Network(format!(
        "{live_error}; no cached copy{} is available",
        if had_snapshot { " or snapshot" } else { "" }
    )))
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
        _ => Err(PhnError::Network("the response was empty or not JSON".into())),
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
        assert_eq!(with_param("https://x/a", "key", "k 1"), "https://x/a?key=k%201");
        assert_eq!(with_param("https://x/a?b=1", "key", "k"), "https://x/a?b=1&key=k");
        assert_eq!(
            scrub("GET https://x/a?key=SECRET1: 403", Some("SECRET1")),
            "GET https://x/a?key=[key]: 403"
        );
        assert!(expect_json(b"[[\"NAME\"]]").is_ok());
        assert!(expect_json(b"<html>missing key</html>").is_err());
        assert!(expect_xml(b"<?xml version=\"1.0\"?><a/>").is_ok());
        assert!(expect_xml(b"{}").is_err());
    }

    #[tokio::test]
    async fn invalid_cached_bodies_are_not_used_and_snapshot_is() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::for_root(dir.path());
        let http = HttpClient::new(&config).unwrap();
        let cache = Cache::in_memory().unwrap();
        cache.put("https://x/a", b"<html>error</html>", 3600).unwrap();
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
    }
}
