//! Test harness: an in-process MCP client connected to the server over a
//! duplex pipe, with the synthetic corpus copied into a temporary /data.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use phn_core::Config;
use phn_server::PhnServer;
use phn_tools::Context;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;

pub struct Harness {
    pub dir: tempfile::TempDir,
    pub client: RunningService<RoleClient, ()>,
    pub ctx: Arc<Context>,
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn corpus() -> PathBuf {
    repo_root().join("fixtures/synthetic")
}

pub fn copy_dir(from: &Path, to: &Path) {
    for e in walkdir::WalkDir::new(from).into_iter().flatten() {
        let rel = e.path().strip_prefix(from).unwrap();
        let dest = to.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
        } else {
            std::fs::copy(e.path(), &dest).unwrap();
        }
    }
}

/// Start a server over the synthetic corpus. `customize` can adjust the configuration.
pub async fn start(customize: impl FnOnce(&mut Config)) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&corpus().join("data"), &dir.path().join("data"));
    std::fs::create_dir_all(dir.path().join("cache")).unwrap();
    let mut config = Config::for_root(dir.path());
    customize(&mut config);
    let ctx = Arc::new(Context::new(config).unwrap());
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    let server = PhnServer::new(Arc::clone(&ctx));
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    let client = ().serve(client_io).await.expect("client connects");
    Harness { dir, client, ctx }
}

impl Harness {
    pub async fn call(&self, name: &str, args: Value) -> CallToolResult {
        let obj = args.as_object().cloned().unwrap_or_default();
        self.client
            .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(obj))
            .await
            .unwrap_or_else(|e| panic!("protocol error calling {name}: {e:?}"))
    }

    pub fn data(&self) -> PathBuf {
        self.dir.path().join("data")
    }
}

/// Everything a client (and so the model) receives from a call.
pub fn everything(r: &CallToolResult) -> String {
    let mut s = String::new();
    for c in &r.content {
        if let Some(t) = c.as_text() {
            s.push_str(&t.text);
            s.push('\n');
        }
    }
    if let Some(v) = &r.structured_content {
        s.push_str(&v.to_string());
    }
    s
}

/// First text block (the markdown summary).
pub fn summary(r: &CallToolResult) -> String {
    r.content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Seeded {
    pub value: String,
    pub kind: String,
    pub file: String,
    pub column: Option<String>,
}

pub fn seeded() -> Vec<Seeded> {
    #[derive(serde::Deserialize)]
    struct F {
        identifiers: Vec<Seeded>,
    }
    let text = std::fs::read_to_string(corpus().join("seeded_identifiers.json")).unwrap();
    serde_json::from_str::<F>(&text).unwrap().identifiers
}

/// Seeded values from one file that appear (as whole words) in `haystack`.
pub fn leaks(haystack: &str, file: &str, kinds: Option<&[&str]>) -> Vec<String> {
    seeded()
        .into_iter()
        .filter(|s| s.file == file)
        .filter(|s| kinds.is_none_or(|k| k.contains(&s.kind.as_str())))
        .filter(|s| {
            let re = regex::Regex::new(&format!(r"(^|[^\w]){}([^\w]|$)", regex::escape(&s.value)))
                .unwrap();
            re.is_match(haystack)
        })
        .map(|s| format!("{} ({})", s.value, s.kind))
        .collect()
}
