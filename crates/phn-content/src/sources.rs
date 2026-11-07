//! The data source registry (`content/data_sources.yaml`).

use std::sync::LazyLock;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContentMeta;

/// How a source is accessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Reviewed reference content bundled with the server.
    Content,
    /// Dated data snapshot bundled with the server.
    Snapshot,
    /// Public API called at request time (with snapshot fallback).
    Live,
    /// The nurse's own files.
    Local,
}

/// Lifecycle status of a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Active,
    Planned,
    Retired,
}

/// One registered data source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DataSource {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub kind: SourceKind,
    pub status: SourceStatus,
    #[serde(default)]
    pub planned_phase: Option<u32>,
    #[serde(default)]
    pub content_id: Option<String>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    pub homepage: Option<String>,
    pub probe_url: Option<String>,
    pub used_by: Vec<String>,
    pub caveats: Vec<String>,
}

/// The registry file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DataSourceRegistry {
    pub meta: ContentMeta,
    pub general_caveats: Vec<String>,
    pub sources: Vec<DataSource>,
}

static REGISTRY: LazyLock<DataSourceRegistry> = LazyLock::new(|| {
    let item = crate::ITEMS
        .iter()
        .find(|i| i.id == "data-source-registry")
        .expect("data source registry is registered");
    let v = crate::yaml_to_json(item.yaml).expect("data_sources.yaml parses");
    serde_json::from_value(v).expect("data_sources.yaml matches the Rust types")
});

/// The bundled data source registry.
pub fn registry() -> &'static DataSourceRegistry {
    &REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_and_references_known_content() {
        let r = registry();
        assert!(r.sources.iter().any(|s| s.status == SourceStatus::Active));
        for s in &r.sources {
            if let Some(cid) = &s.content_id {
                assert!(
                    crate::ITEMS.iter().any(|i| i.id == cid),
                    "unknown content {cid}"
                );
            }
        }
    }
}
