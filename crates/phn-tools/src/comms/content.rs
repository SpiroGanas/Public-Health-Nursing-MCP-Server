//! Typed access to bundled content for the Phase 11 tools (comms and the
//! substance-use and postvention tools in `bh`).
//!
//! Each file is embedded through `phn_content::ITEMS` (so it is also an MCP
//! resource and is checked by `cargo xtask validate-content`) and parsed with
//! [`load`].

use std::collections::BTreeMap;

use phn_content::{ContentMeta, ReviewStatus};
use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Content id: readability, plain language, and the Clear Communication Index.
pub const READABILITY: &str = "comms-readability";
/// Content id: safe-messaging rules (suicide and overdose).
pub const SAFE_MESSAGING: &str = "comms-safe-messaging";
/// Content id: social media, press materials, observances, and CERC.
pub const CAMPAIGNS: &str = "comms-campaigns";
/// Content id: translation workflow guidance.
pub const TRANSLATION: &str = "comms-translation";
/// Content id: naloxone, harm reduction, and treatment resources.
pub const SUBSTANCE_USE: &str = "bh-substance-use-resources";
/// Content id: school postvention (After a Suicide toolkit).
pub const POSTVENTION: &str = "bh-school-postvention";

/// Parse a registered content item into `T`.
pub fn load<T: DeserializeOwned>(id: &str) -> Result<T> {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PhnError::Internal(format!("content item {id} is not registered")))?;
    let v = phn_content::yaml_to_json(item.yaml).map_err(PhnError::Internal)?;
    serde_json::from_value(v).map_err(|e| PhnError::Internal(format!("{}: {e}", item.path)))
}

/// The meta block of a content item.
pub fn meta(id: &str) -> Result<ContentMeta> {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PhnError::Internal(format!("content item {id} is not registered")))?;
    phn_content::item_meta(item).map_err(PhnError::Internal)
}

/// A citation for a whole content file (mode `snapshot`).
pub fn content_source(id: &str) -> Result<Source> {
    let m = meta(id)?;
    Ok(Source {
        title: m.source.title.clone(),
        publisher: m.source.publisher.clone(),
        url: m.source.url.clone(),
        effective_date: m.effective_date.clone(),
        retrieved: Some(m.prepared.clone()),
        mode: SourceMode::Snapshot,
    })
}

/// Review caveats for content items that are still pending review.
pub fn review_caveats(ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| meta(id).ok())
        .filter(|m| m.review.status == ReviewStatus::Pending)
        .filter_map(|m| m.review_caveat())
        .collect()
}

/// A named source inside a content file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NamedSource {
    pub title: String,
    pub publisher: String,
    pub url: Option<String>,
    /// Publication or page date (ISO 8601), when known.
    pub date: Option<String>,
}

impl NamedSource {
    /// A citation (mode `snapshot`, retrieved on the content's prepared date).
    pub fn to_source(&self, retrieved: &str) -> Source {
        Source {
            title: self.title.clone(),
            publisher: self.publisher.clone(),
            url: self.url.clone(),
            effective_date: self.date.clone(),
            retrieved: Some(retrieved.to_string()),
            mode: SourceMode::Snapshot,
        }
    }
}

/// Citations for the named sources `keys` (unknown keys are skipped; duplicates removed).
pub fn cite(
    sources: &BTreeMap<String, NamedSource>,
    keys: &[&str],
    retrieved: &str,
) -> Vec<Source> {
    let mut seen = std::collections::BTreeSet::new();
    keys.iter()
        .filter(|k| seen.insert(**k))
        .filter_map(|k| sources.get(*k))
        .map(|s| s.to_source(retrieved))
        .collect()
}

/// Developer confidence in a drafted entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readability_content_loads_with_pending_review() {
        let m = meta(READABILITY).unwrap();
        assert_eq!(m.review.status, ReviewStatus::Pending);
        assert_eq!(m.review.review_due, "2027-02-01");
        assert!(!review_caveats(&[READABILITY]).is_empty());
        assert!(content_source(READABILITY).unwrap().url.is_some());
    }
}
