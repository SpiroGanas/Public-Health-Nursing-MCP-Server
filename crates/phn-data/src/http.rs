//! Outbound HTTP client.
//!
//! Agency networks often route traffic through a proxy and inspect TLS with a
//! private certificate authority. The client therefore:
//!
//! * honors `HTTPS_PROXY`, `HTTP_PROXY`, and `NO_PROXY` (reqwest's defaults);
//! * trusts the platform's root certificates plus any PEM bundle named by
//!   `PHN_CA_BUNDLE`;
//! * refuses every request when `PHN_OFFLINE` is set;
//! * never sends a `Referer` header, so a redirect cannot pass the previous
//!   URL (which may carry an API key in its query string) to another host.

use std::time::{Duration, Instant};

use phn_core::{Config, PhnError, Result};
use serde::{Deserialize, Serialize};

/// Outbound HTTP client shared by data adapters.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    offline: bool,
}

/// A failed GET.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpFailure {
    /// The HTTP status code, when the server answered with a non-success
    /// status (for example 404); `None` when no response was received.
    pub status: Option<u16>,
    /// What went wrong (includes the request URL).
    pub message: String,
}

/// Result of a reachability check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProbeResult {
    /// True when the endpoint answered with a success or redirect status.
    pub reachable: bool,
    /// HTTP status code, when a response was received.
    pub http_status: Option<u16>,
    /// Round-trip time in milliseconds.
    pub latency_ms: Option<u64>,
    /// Error message when no usable response was received.
    pub error: Option<String>,
    /// When the check ran (RFC 3339, UTC).
    pub checked_at: String,
}

impl HttpClient {
    /// Build the client from configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .user_agent(format!(
                "{}/{} (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)",
                phn_core::SERVER_NAME,
                phn_core::VERSION
            ))
            .timeout(config.http_timeout)
            .connect_timeout(Duration::from_secs(10))
            // reqwest's default sets `Referer` to the full previous URL on a
            // redirect, which would hand a `?key=...` query string to the
            // redirect target.
            .referer(false);
        if let Some(path) = &config.ca_bundle {
            let pem = std::fs::read(path).map_err(|e| {
                PhnError::Config(format!("cannot read PHN_CA_BUNDLE {}: {e}", path.display()))
            })?;
            let certs = reqwest::Certificate::from_pem_bundle(&pem).map_err(|e| {
                PhnError::Config(format!(
                    "PHN_CA_BUNDLE {} is not a PEM bundle: {e}",
                    path.display()
                ))
            })?;
            if certs.is_empty() {
                return Err(PhnError::Config(format!(
                    "PHN_CA_BUNDLE {} contains no PEM certificates",
                    path.display()
                )));
            }
            builder = builder.tls_certs_merge(certs);
        }
        let inner = builder
            .build()
            .map_err(|e| PhnError::Config(format!("cannot build HTTP client: {e}")))?;
        Ok(Self {
            inner,
            offline: config.offline,
        })
    }

    /// True when network calls are disabled.
    pub fn is_offline(&self) -> bool {
        self.offline
    }

    /// GET a URL and return the body. Non-success statuses are errors.
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.get_bytes_status(url)
            .await
            .map_err(|f| PhnError::Network(f.message))
    }

    /// GET a URL and return the body; a failure reports the HTTP status when
    /// the server answered with one, so callers can tell "not found" from
    /// "unreachable" without reading the message.
    pub async fn get_bytes_status(&self, url: &str) -> std::result::Result<Vec<u8>, HttpFailure> {
        if self.offline {
            return Err(HttpFailure {
                status: None,
                message: "offline mode is on (PHN_OFFLINE)".into(),
            });
        }
        let resp = self.inner.get(url).send().await.map_err(|e| HttpFailure {
            status: None,
            message: format!("GET {url}: {e}"),
        })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(HttpFailure {
                status: Some(status.as_u16()),
                message: format!("GET {url}: HTTP {status}"),
            });
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| HttpFailure {
                status: None,
                message: format!("GET {url}: {e}"),
            })
    }

    /// Check whether a URL is reachable. Tries `HEAD`, then a `GET` when the
    /// server rejects `HEAD`. Never returns an error; failures are reported in
    /// the result.
    pub async fn probe(&self, url: &str) -> ProbeResult {
        let checked_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        if self.offline {
            return ProbeResult {
                reachable: false,
                http_status: None,
                latency_ms: None,
                error: Some("offline mode is on (PHN_OFFLINE)".into()),
                checked_at,
            };
        }
        let start = Instant::now();
        let mut result = self.inner.head(url).send().await;
        if let Ok(r) = &result
            && matches!(r.status().as_u16(), 403 | 405 | 501)
        {
            result = self.inner.get(url).send().await;
        }
        let latency_ms = Some(start.elapsed().as_millis() as u64);
        match result {
            Ok(r) => {
                let status = r.status();
                ProbeResult {
                    reachable: status.is_success() || status.is_redirection(),
                    http_status: Some(status.as_u16()),
                    latency_ms,
                    error: (!status.is_success() && !status.is_redirection())
                        .then(|| format!("HTTP {status}")),
                    checked_at,
                }
            }
            Err(e) => ProbeResult {
                reachable: false,
                http_status: None,
                latency_ms,
                error: Some(describe_error(&e)),
                checked_at,
            },
        }
    }
}

fn describe_error(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "could not connect (blocked by a firewall or proxy, or no network)"
    } else {
        "request failed"
    };
    // Walk the source chain for the root cause (DNS, TLS, proxy refusal).
    let mut root = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        root = s.to_string();
        src = s.source();
    }
    format!("{kind}: {root}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn offline_mode_blocks_requests() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::for_root(dir.path());
        assert!(config.offline);
        let client = HttpClient::new(&config).unwrap();
        let probe = client.probe("https://example.invalid/").await;
        assert!(!probe.reachable);
        assert!(probe.error.unwrap().contains("offline"));
        assert!(client.get_bytes("https://example.invalid/").await.is_err());
    }

    #[tokio::test]
    async fn failed_gets_report_the_http_status() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::for_root(dir.path());
        config.offline = false;
        let client = HttpClient::new(&config).unwrap();
        // The URL contains "500"; only the real status counts.
        let url = format!("http://{addr}/item/500");
        let failure = client.get_bytes_status(&url).await.unwrap_err();
        assert_eq!(failure.status, Some(404));
        assert!(failure.message.contains("HTTP 404"), "{}", failure.message);
        let err = client.get_bytes(&url).await.unwrap_err();
        assert!(matches!(err, PhnError::Network(_)), "{err:?}");

        let offline = HttpClient::new(&Config::for_root(dir.path())).unwrap();
        let failure = offline.get_bytes_status(&url).await.unwrap_err();
        assert_eq!(failure.status, None);
        assert!(failure.message.contains("offline"));
    }

    #[test]
    fn bad_ca_bundle_is_a_config_error() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("ca.pem");
        std::fs::write(&bad, "not a certificate").unwrap();
        let mut config = Config::for_root(dir.path());
        config.ca_bundle = Some(bad);
        let err = HttpClient::new(&config).unwrap_err();
        assert!(matches!(err, PhnError::Config(_)), "{err:?}");
    }
}
