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
                    "Reference content v{} ({}; review due {}). Source: {}.",
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
                snapshot_mime(&f.path),
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
        .map(|(t, _, _)| (t.into_owned(), snapshot_mime(path)))
}

/// MIME type of a bundled snapshot file, from its extension (most are CSV; the
/// surveillance snapshots are JSON and the guide corpus is plain text).
fn snapshot_mime(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "csv" => "text/csv",
        "json" => "application/json",
        _ => "text/plain",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_mime_follows_the_extension() {
        assert_eq!(snapshot_mime("ma_municipalities.csv"), "text/csv");
        assert_eq!(snapshot_mime("nndss_weekly.json"), "application/json");
        assert_eq!(snapshot_mime("ma_dph_guide_corpus.txt"), "text/plain");
        assert_eq!(snapshot_mime("no_extension"), "text/plain");
    }

    #[test]
    fn listing_never_calls_pending_content_reviewed_and_types_follow_extensions() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let all = list(&ctx);
        let mut pending = 0;
        for r in &all {
            let d = r.description.as_deref().unwrap_or("");
            if d.contains("pending review") {
                pending += 1;
                assert!(
                    !d.contains("Reviewed"),
                    "{} is pending review but described as reviewed: {d}",
                    r.uri
                );
            }
            if r.uri.starts_with("phn://snapshots/") && r.uri.ends_with(".json") {
                assert_eq!(r.mime_type.as_deref(), Some("application/json"), "{}", r.uri);
            }
            if r.uri.starts_with("phn://snapshots/") && r.uri.ends_with(".csv") {
                assert_eq!(r.mime_type.as_deref(), Some("text/csv"), "{}", r.uri);
            }
        }
        assert!(pending > 0, "the bundled content includes pending items");
        // Reading a JSON snapshot reports the same type as the listing.
        let json = all
            .iter()
            .find(|r| r.uri.starts_with("phn://snapshots/") && r.uri.ends_with(".json"))
            .expect("a JSON snapshot is bundled");
        let (_, mime) = read(&ctx, &json.uri).expect("readable");
        assert_eq!(mime, "application/json");
    }
}
