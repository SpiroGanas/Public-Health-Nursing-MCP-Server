//! MCP resources: bundled reference content and data snapshots.

use rmcp::model::Resource;

use crate::server::resource;
use phn_tools::Context;

/// Every resource the server exposes.
pub fn list(ctx: &Context) -> Vec<Resource> {
    let mut out = Vec::new();
    for item in phn_content::ITEMS {
        let (title, description) = match phn_content::item_meta(item) {
            Ok(m) => (
                m.title.clone(),
                format!(
                    "Reviewed reference content v{} ({}; review due {}). Source: {}.",
                    m.version,
                    match m.review.status {
                        phn_content::ReviewStatus::Pending => "pending review",
                        phn_content::ReviewStatus::Reviewed => "reviewed",
                    },
                    m.review.review_due,
                    m.source.title
                ),
            ),
            Err(_) => (item.id.to_string(), String::new()),
        };
        out.push(resource(
            item.resource_uri,
            item.id,
            &title,
            &description,
            "application/yaml",
        ));
    }
    out.push(resource(
        "phn://snapshots/manifest.json",
        "snapshot-manifest",
        "Snapshot manifest",
        "Sources, vintage, retrieval dates, and SHA-256 checksums of bundled data snapshots.",
        "application/json",
    ));
    for entry in ctx.snapshots.bundled_entries() {
        for f in &entry.files {
            out.push(resource(
                &format!("phn://snapshots/{}", f.path),
                &entry.id,
                &entry.title,
                &format!(
                    "{} rows. Source vintage {}; built {}. {}",
                    f.rows, entry.source_vintage, entry.retrieved, entry.derivation
                ),
                "text/csv",
            ));
        }
    }
    out
}

/// Read a resource by URI: (text, MIME type).
pub fn read(ctx: &Context, uri: &str) -> Option<(String, &'static str)> {
    if let Some(item) = phn_content::ITEMS.iter().find(|i| i.resource_uri == uri) {
        return Some((item.yaml.to_string(), "application/yaml"));
    }
    if uri == "phn://snapshots/manifest.json" {
        let entries = ctx.snapshots.bundled_entries();
        return serde_json::to_string_pretty(&serde_json::json!({
            "format_version": 1,
            "snapshots": entries,
        }))
        .ok()
        .map(|t| (t, "application/json"));
    }
    let path = uri.strip_prefix("phn://snapshots/")?;
    let entry = ctx
        .snapshots
        .bundled_entries()
        .iter()
        .find(|e| e.files.iter().any(|f| f.path == path))?;
    ctx.snapshots
        .file_text(&entry.id, path)
        .ok()
        .map(|(t, _, _)| (t.into_owned(), "text/csv"))
}
