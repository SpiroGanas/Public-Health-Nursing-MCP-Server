//! Reviewed reference content.
//!
//! Content lives in the repository's `content/` folder as YAML, each file
//! with a JSON Schema in `content/schemas/`. Every item carries review
//! metadata ([`meta::ContentMeta`]): its source, effective date, who reviewed
//! it, and when the next review is due. CI fails when a review is overdue
//! (`cargo xtask validate-content`).
//!
//! Files are embedded in the binary at build time, so the server works
//! offline and the content shipped is exactly the content that was tested.

pub mod meta;
pub mod reportable;
pub mod sources;

pub use meta::{Citation, ContentMeta, Review, ReviewStatus};

/// A bundled content file.
#[derive(Debug, Clone, Copy)]
pub struct ContentItem {
    /// Stable identifier (matches `meta.id` in the file).
    pub id: &'static str,
    /// Path relative to the repository root.
    pub path: &'static str,
    /// MCP resource URI used to expose the file.
    pub resource_uri: &'static str,
    /// The YAML text.
    pub yaml: &'static str,
    /// The JSON Schema text.
    pub schema: &'static str,
}

/// Every bundled content item.
pub const ITEMS: &[ContentItem] = &[
    ContentItem {
        id: "ma-reportable-conditions",
        path: "content/cd/reportable_conditions.yaml",
        resource_uri: "phn://content/ma-reportable-conditions",
        yaml: include_str!("../../../content/cd/reportable_conditions.yaml"),
        schema: include_str!("../../../content/schemas/reportable_conditions.schema.json"),
    },
    ContentItem {
        id: "data-source-registry",
        path: "content/data_sources.yaml",
        resource_uri: "phn://content/data-source-registry",
        yaml: include_str!("../../../content/data_sources.yaml"),
        schema: include_str!("../../../content/schemas/data_sources.schema.json"),
    },
    // Phase 2 content items go below this line.

    // Phase 3 content items go below this line.

    // Phase 4 content items go below this line.
    ContentItem {
        id: "language-access-rules",
        path: "content/equity/language_access.yaml",
        resource_uri: "phn://content/language-access-rules",
        yaml: include_str!("../../../content/equity/language_access.yaml"),
        schema: include_str!("../../../content/schemas/language_access.schema.json"),
    },
    // Phase 5 content items go below this line.

    // Phase 6 content items go below this line.
];

/// Parse a YAML document into a JSON value (for schema validation).
pub fn yaml_to_json(yaml: &str) -> Result<serde_json::Value, String> {
    serde_saphyr::from_str::<serde_json::Value>(yaml).map_err(|e| e.to_string())
}

/// Read the `meta` block of a content item.
pub fn item_meta(item: &ContentItem) -> Result<ContentMeta, String> {
    #[derive(serde::Deserialize)]
    struct OnlyMeta {
        meta: ContentMeta,
    }
    let v = yaml_to_json(item.yaml)?;
    serde_json::from_value::<OnlyMeta>(v)
        .map(|m| m.meta)
        .map_err(|e| format!("{}: {e}", item.path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_matches_its_schema() {
        for item in ITEMS {
            let schema: serde_json::Value = serde_json::from_str(item.schema).unwrap();
            let validator = jsonschema::validator_for(&schema).unwrap();
            let doc = yaml_to_json(item.yaml).unwrap();
            let errors: Vec<String> = validator
                .iter_errors(&doc)
                .map(|e| format!("{} at {}", e, e.instance_path()))
                .collect();
            assert!(errors.is_empty(), "{}: {errors:#?}", item.path);
            let meta = item_meta(item).unwrap();
            assert_eq!(meta.id, item.id);
        }
    }
}
