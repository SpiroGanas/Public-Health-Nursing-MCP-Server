//! The live → cache → snapshot fallback chain used by data adapters.
//!
//! 1. A fresh cached copy is used without a network call.
//! 2. Otherwise the live API is called; a success refreshes the cache.
//! 3. If the live call fails, a stale cached copy is used.
//! 4. If there is no cached copy, the bundled snapshot is used.
//!
//! Every result says which mode was used and how old the data is, so the
//! response envelope can cite it.

use phn_core::{PhnError, Result, SourceMode};

use crate::cache::Cache;
use crate::http::HttpClient;

/// A body plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub body: Vec<u8>,
    pub mode: SourceMode,
    /// Date (or date-time) of the data that was used.
    pub as_of: String,
    /// Why a fallback was used, when one was.
    pub fallback_reason: Option<String>,
}

/// A bundled fallback: body and its snapshot date.
pub type SnapshotFallback = Box<dyn FnOnce() -> Option<(Vec<u8>, String)> + Send>;

/// Fetch `url` with the fallback chain described in the module docs.
pub async fn fetch_with_fallback(
    http: &HttpClient,
    cache: Option<&Cache>,
    url: &str,
    ttl_secs: u64,
    snapshot: Option<SnapshotFallback>,
) -> Result<Fetched> {
    let cached = match cache {
        Some(c) => c.get(url)?,
        None => None,
    };
    if let Some(hit) = &cached {
        if hit.fresh {
            return Ok(Fetched {
                body: hit.body.clone(),
                mode: SourceMode::Cache,
                as_of: hit.fetched_at.to_rfc3339(),
                fallback_reason: None,
            });
        }
    }

    let live_error = match http.get_bytes(url).await {
        Ok(body) => {
            if let Some(c) = cache {
                if let Err(e) = c.put(url, &body, ttl_secs) {
                    tracing::warn!(error = %e, "could not write HTTP cache");
                }
            }
            return Ok(Fetched {
                body,
                mode: SourceMode::Live,
                as_of: chrono::Utc::now().to_rfc3339(),
                fallback_reason: None,
            });
        }
        Err(e) => e.to_string(),
    };

    if let Some(hit) = cached {
        return Ok(Fetched {
            body: hit.body,
            mode: SourceMode::Cache,
            as_of: hit.fetched_at.to_rfc3339(),
            fallback_reason: Some(format!(
                "live request failed ({live_error}); using cached copy"
            )),
        });
    }
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
        "{live_error}; no cached copy or snapshot is available"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_core::Config;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn serve_once(body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        format!("http://{addr}/data")
    }

    fn client(offline: bool) -> HttpClient {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::for_root(dir.path());
        config.offline = offline;
        HttpClient::new(&config).unwrap()
    }

    #[tokio::test]
    async fn live_then_fresh_cache() {
        let cache = Cache::in_memory().unwrap();
        let url = serve_once("live!").await;
        let http = client(false);
        let first = fetch_with_fallback(&http, Some(&cache), &url, 3600, None)
            .await
            .unwrap();
        assert_eq!(first.mode, SourceMode::Live);
        assert_eq!(first.body, b"live!");
        // The one-shot server is gone; a fresh cache entry avoids the network.
        let second = fetch_with_fallback(&http, Some(&cache), &url, 3600, None)
            .await
            .unwrap();
        assert_eq!(second.mode, SourceMode::Cache);
        assert!(second.fallback_reason.is_none());
    }

    #[tokio::test]
    async fn offline_uses_stale_cache_then_snapshot() {
        let http = client(true);
        let cache = Cache::in_memory().unwrap();
        cache
            .put_at(
                "https://example.org/a",
                b"stale",
                1,
                chrono::Utc::now() - chrono::Duration::days(1),
            )
            .unwrap();
        let r = fetch_with_fallback(&http, Some(&cache), "https://example.org/a", 1, None)
            .await
            .unwrap();
        assert_eq!(r.mode, SourceMode::Cache);
        assert!(r.fallback_reason.unwrap().contains("offline"));

        let snap: SnapshotFallback = Box::new(|| Some((b"snap".to_vec(), "2026-01-01".into())));
        let r = fetch_with_fallback(&http, Some(&cache), "https://example.org/b", 1, Some(snap))
            .await
            .unwrap();
        assert_eq!(r.mode, SourceMode::Snapshot);
        assert_eq!(r.as_of, "2026-01-01");

        let err = fetch_with_fallback(&http, None, "https://example.org/c", 1, None).await;
        assert!(err.is_err());
    }
}
