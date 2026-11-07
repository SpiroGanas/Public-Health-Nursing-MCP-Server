//! `cargo xtask validate-content`: CI gate for reference content and snapshots.
//!
//! * Every content file matches its JSON Schema.
//! * Every content item has a citation, and no review is overdue (fails CI).
//! * Pending reviews are listed as warnings.
//! * Every snapshot file matches the checksum in `snapshots/manifest.json`.
//! * Every data source in the registry points at known content or snapshots.

use std::path::Path;

use anyhow::{Result, bail};
use chrono::NaiveDate;
use phn_content::{ITEMS, ReviewStatus};

use crate::download::sha256;
use crate::snapshots::Manifest;

pub fn run(root: &Path, today: NaiveDate) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for item in ITEMS {
        // The embedded copy must equal the file on disk (no stale build).
        let on_disk = std::fs::read_to_string(root.join(item.path))?;
        if on_disk != item.yaml {
            errors.push(format!(
                "{}: embedded copy differs from disk; rebuild",
                item.path
            ));
        }
        let schema: serde_json::Value = serde_json::from_str(item.schema)?;
        let validator = jsonschema::validator_for(&schema)
            .map_err(|e| anyhow::anyhow!("{}: invalid schema: {e}", item.path))?;
        let doc = phn_content::yaml_to_json(item.yaml).map_err(anyhow::Error::msg)?;
        for e in validator.iter_errors(&doc) {
            errors.push(format!("{}: {} at {}", item.path, e, e.instance_path()));
        }
        let meta = match phn_content::item_meta(item) {
            Ok(m) => m,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        if meta.source.url.is_none() {
            errors.push(format!("{}: source has no URL", item.path));
        }
        match meta.review.due_date() {
            None => errors.push(format!("{}: review_due is not a date", item.path)),
            Some(due) if due < today => errors.push(format!(
                "{}: review was due {due}; review the content and update meta.review",
                item.path
            )),
            Some(due) => {
                let days = (due - today).num_days();
                if meta.review.status == ReviewStatus::Pending {
                    warnings.push(format!(
                        "{}: PENDING subject-matter review (due {due}, {days} days)",
                        item.path
                    ));
                } else if days <= 30 {
                    warnings.push(format!("{}: review due in {days} days", item.path));
                }
            }
        }
        println!(
            "ok   {} ({}, v{}, {:?})",
            item.path, meta.id, meta.version, meta.review.status
        );
    }

    let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(
        root.join("snapshots/manifest.json"),
    )?)?;
    for entry in &manifest.snapshots {
        for f in &entry.files {
            let bytes = std::fs::read(root.join("snapshots").join(&f.path))?;
            if sha256(&bytes) != f.sha256 {
                errors.push(format!(
                    "snapshots/{}: checksum does not match manifest",
                    f.path
                ));
            }
        }
        println!("ok   snapshot {} ({} file(s))", entry.id, entry.files.len());
    }

    let registry = phn_content::sources::registry();
    for s in &registry.sources {
        if let Some(sid) = &s.snapshot_id
            && !manifest.snapshots.iter().any(|e| &e.id == sid)
        {
            errors.push(format!("data source {}: unknown snapshot {sid}", s.id));
        }
        if let Some(cid) = &s.content_id
            && !ITEMS.iter().any(|i| i.id == cid)
        {
            errors.push(format!("data source {}: unknown content {cid}", s.id));
        }
    }

    for w in &warnings {
        println!("warn {w}");
    }
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("FAIL {e}");
        }
        bail!("{} content problem(s)", errors.len());
    }
    println!("Content and snapshots are valid.");
    Ok(())
}
