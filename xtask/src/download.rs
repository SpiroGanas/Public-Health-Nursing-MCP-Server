//! Small download helper shared by the builders.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Download `url`, or read it from `source_dir` when given (offline rebuilds).
pub fn fetch(url: &str, source_dir: Option<&Path>) -> Result<Vec<u8>> {
    if let Some(dir) = source_dir {
        let name = url.rsplit('/').next().unwrap_or(url);
        let path = dir.join(name);
        return std::fs::read(&path).with_context(|| format!("reading {}", path.display()));
    }
    eprintln!("downloading {url}");
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "phn-mcp-server-xtask/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
        ))
        .timeout(Duration::from_secs(300))
        .build()?;
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    Ok(resp.bytes()?.to_vec())
}

/// Hex SHA-256 of a byte slice.
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
