//! Typed access to the Phase 10 reference content (`content/aging/` and the
//! Phase 10 files in `content/bh/`).
//!
//! Each file is embedded through `phn_content::ITEMS` (so it is also an MCP
//! resource and is checked by `cargo xtask validate-content`). This module
//! holds the helpers the Phase 10 tools share: loading a file into typed
//! structs, citing it, citing the sources listed inside it, and the review
//! caveat.

use std::collections::BTreeMap;

use phn_content::{ContentMeta, ReviewStatus};
use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Content ids.
pub const MH_SCREENING: &str = "bh-mental-health-screening";
pub const CRISIS: &str = "bh-crisis-resources";
pub const FALLS: &str = "aging-steadi-falls";
pub const PROTECTIVE: &str = "aging-protective-services";
pub const SERVICES: &str = "aging-services-guide";
pub const COVERAGE: &str = "aging-coverage-options";
pub const NURSING_HOMES: &str = "aging-nursing-home-guide";
pub const ACP: &str = "aging-advance-care-planning";

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

/// A citation for a bundled content file (mode `snapshot`).
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

/// The review caveats for content items (empty once reviewed).
pub fn review_caveats(ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| meta(id).ok())
        .filter(|m| m.review.status == ReviewStatus::Pending)
        .filter_map(|m| m.review_caveat())
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

impl Confidence {
    /// Label for tables.
    pub fn label(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }
}

/// A source listed in a content file's `sources` map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub effective_date: Option<String>,
    pub retrieved: String,
}

impl SourceRef {
    /// As an envelope source (mode `snapshot`: bundled reference content).
    pub fn to_source(&self) -> Source {
        Source {
            title: self.title.clone(),
            publisher: self.publisher.clone(),
            url: Some(self.url.clone()),
            effective_date: self.effective_date.clone(),
            retrieved: Some(self.retrieved.clone()),
            mode: SourceMode::Snapshot,
        }
    }
}

/// Text with a citation (key into the file's `sources` map, and a section).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedText {
    pub text: String,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

/// A cited statement as returned to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedOut {
    pub text: String,
    /// Source title and section.
    pub citation: String,
    /// Developer confidence when below high (pending the nurse's review).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// Turn cited texts into output, resolving source keys to titles.
pub fn cite_all(sources: &BTreeMap<String, SourceRef>, items: &[CitedText]) -> Vec<CitedOut> {
    items.iter().map(|c| cite_one(sources, c)).collect()
}

/// Turn one cited text into output.
pub fn cite_one(sources: &BTreeMap<String, SourceRef>, c: &CitedText) -> CitedOut {
    CitedOut {
        text: c.text.clone(),
        citation: citation(sources, &c.source, &c.section),
        confidence: c.confidence.filter(|c| *c != Confidence::High),
    }
}

/// "Title, section" for a source key.
pub fn citation(sources: &BTreeMap<String, SourceRef>, key: &str, section: &str) -> String {
    let title = sources
        .get(key)
        .map(|s| s.title.as_str())
        .unwrap_or(key);
    if section.is_empty() {
        title.to_string()
    } else {
        format!("{title}, {section}")
    }
}

/// Envelope sources for the given keys (deduplicated, in order).
pub fn sources_for<'a>(
    sources: &BTreeMap<String, SourceRef>,
    keys: impl IntoIterator<Item = &'a str>,
) -> Vec<Source> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for k in keys {
        if seen.contains(&k) {
            continue;
        }
        seen.push(k);
        if let Some(s) = sources.get(k) {
            out.push(s.to_source());
        }
    }
    out
}

/// Check that every source key used in a file exists (for unit tests).
pub fn assert_keys_known(sources: &BTreeMap<String, SourceRef>, keys: &[&str], file: &str) {
    for k in keys {
        assert!(
            sources.contains_key(*k),
            "{file}: unknown source key '{k}'"
        );
    }
}

/// Resolve a Massachusetts city or town name, with suggestions on a miss.
pub fn resolve_town<'a>(
    ctx: &'a crate::Context,
    name: &str,
) -> Result<&'a phn_data::ma::Municipality> {
    let q = name.trim();
    if q.is_empty() {
        return Err(PhnError::InvalidInput(
            "give a Massachusetts city or town".into(),
        ));
    }
    ctx.ma.town(q).ok_or_else(|| {
        let s: Vec<String> = ctx
            .ma
            .suggest_towns(q, 3)
            .into_iter()
            .map(|m| m.name.clone())
            .collect();
        PhnError::NotFound(if s.is_empty() {
            format!("'{q}' is not one of the 351 Massachusetts cities and towns")
        } else {
            format!(
                "'{q}' is not one of the 351 Massachusetts cities and towns. Did you mean: {}?",
                s.join(", ")
            )
        })
    })
}

/// The town to use: the argument, or the configured jurisdiction.
pub fn town_or_configured(ctx: &crate::Context, town: Option<&str>) -> Option<String> {
    town.map(str::to_string)
        .filter(|t| !t.trim().is_empty())
        .or_else(|| ctx.settings().jurisdiction.town)
}
