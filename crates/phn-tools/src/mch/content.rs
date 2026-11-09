//! Shared helpers for the Phase 8 content files (`content/mch/*.yaml` and
//! `content/equity/benefit_eligibility.yaml`): loading, citations, and review caveats.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_content::reportable::Confidence;
use phn_core::{Source, SourceMode};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// A dated source referenced by key from content items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub effective_date: Option<String>,
    pub retrieved: String,
}

impl SourceRef {
    /// Citation for an envelope (bundled content, so mode `snapshot`).
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

/// Text with a citation (source key and section).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedText {
    pub text: String,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

/// A cited note as returned to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NoteOut {
    pub text: String,
    /// Source title and section.
    pub citation: String,
    /// Developer confidence for draft content (high, medium, low).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// Load and deserialize a content item by id. Panics if the bundled file does not match
/// the Rust types (a unit test loads every file, so this cannot reach users).
pub(crate) fn load<T: DeserializeOwned>(id: &str) -> T {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("content item {id} is registered"));
    let v = phn_content::yaml_to_json(item.yaml)
        .unwrap_or_else(|e| panic!("{}: YAML parses: {e}", item.path));
    serde_json::from_value(v)
        .unwrap_or_else(|e| panic!("{}: content matches the Rust types: {e}", item.path))
}

/// "Title, section" for a source key.
pub(crate) fn cite(sources: &BTreeMap<String, SourceRef>, key: &str, section: &str) -> String {
    let title = sources.get(key).map(|s| s.title.as_str()).unwrap_or(key);
    if section.is_empty() {
        title.to_string()
    } else {
        format!("{title}, {section}")
    }
}

/// Convert cited texts to notes.
pub(crate) fn notes(sources: &BTreeMap<String, SourceRef>, v: &[CitedText]) -> Vec<NoteOut> {
    v.iter().map(|n| note(sources, n)).collect()
}

/// Convert one cited text.
pub(crate) fn note(sources: &BTreeMap<String, SourceRef>, n: &CitedText) -> NoteOut {
    NoteOut {
        text: n.text.clone(),
        citation: cite(sources, &n.source, &n.section),
        confidence: n.confidence,
    }
}

/// Envelope sources for the given keys (deduplicated, in key order).
pub(crate) fn sources_for<'a>(
    sources: &BTreeMap<String, SourceRef>,
    keys: impl IntoIterator<Item = &'a str>,
) -> Vec<Source> {
    let mut k: Vec<&str> = keys.into_iter().collect();
    k.sort_unstable();
    k.dedup();
    k.into_iter()
        .filter_map(|key| sources.get(key))
        .map(SourceRef::to_source)
        .collect()
}

/// The standard review caveat for a Phase 8 content file.
pub(crate) fn review_caveat(meta: &ContentMeta, what: &str) -> String {
    format!(
        "REVIEW STATUS: the {what} content is a PENDING-REVIEW draft (version {}, prepared {}) \
         that the subject matter expert has not yet checked (review due {}). {}",
        meta.version, meta.prepared, meta.review.review_due, meta.verification_note
    )
}

/// Markdown suffix for low- or medium-confidence items.
pub(crate) fn confidence_tag(c: Option<Confidence>) -> &'static str {
    match c {
        Some(Confidence::Low) => " _(low confidence: verify)_",
        Some(Confidence::Medium) => " _(medium confidence)_",
        _ => "",
    }
}

/// Markdown bullet for a note.
pub(crate) fn bullet(n: &NoteOut) -> String {
    format!(
        "- {}{} ({})\n",
        n.text,
        confidence_tag(n.confidence),
        n.citation
    )
}

/// An agency-approved translation of a family handout or template.
pub(crate) enum Translation {
    /// A Word file used as is.
    Docx(std::path::PathBuf),
    /// Markdown rendered to an accessible DOCX.
    Markdown(String),
}

/// Folder names tried for a language under `/data/templates/`.
pub(crate) fn language_folders(language: &str) -> Vec<String> {
    let l = phn_core::text::normalize(language).replace(' ', "_");
    let pairs: &[(&str, &str)] = &[
        ("es", "spanish"),
        ("pt", "portuguese"),
        ("ht", "haitian_creole"),
        ("vi", "vietnamese"),
        ("zh", "chinese"),
        ("ar", "arabic"),
        ("km", "khmer"),
        ("ru", "russian"),
        ("so", "somali"),
        ("fr", "french"),
        ("ne", "nepali"),
        ("sw", "swahili"),
    ];
    let mut out = vec![l.clone()];
    for (code, name) in pairs {
        if l == *code || l.starts_with(&format!("{code}_")) {
            out.push((*name).to_string());
        } else if l == *name {
            out.push((*code).to_string());
        }
    }
    out.dedup();
    out
}

/// True for English (or no language given).
pub(crate) fn is_english(language: Option<&str>) -> bool {
    language.is_none_or(|l| {
        let n = phn_core::text::normalize(l);
        n.is_empty() || n == "en" || n.starts_with("en ") || n == "english"
    })
}

/// Look for `/data/templates/<language>/<stem>.docx` or `.md`.
pub(crate) fn agency_translation(
    ctx: &crate::context::Context,
    language: &str,
    stem: &str,
) -> Option<(Translation, String)> {
    let sandbox = ctx.sandbox().ok()?;
    for folder in language_folders(language) {
        for ext in ["docx", "md"] {
            let rel = format!("templates/{folder}/{stem}.{ext}");
            if let Ok(p) = sandbox.resolve_existing(&rel) {
                let t = if ext == "docx" {
                    Translation::Docx(p)
                } else {
                    Translation::Markdown(std::fs::read_to_string(&p).ok()?)
                };
                return Some((t, rel));
            }
        }
    }
    None
}
