//! Shared helpers for the Phase 9 practice toolkit (CORE-01 to CORE-03, CD-11,
//! CD-12, EQ-05, EQ-07, EQ-11): typed content loading, citations, reading a
//! local note without returning it, agency translations in
//! `/data/templates/<language>/`, and writing documents to `/data/output`.

use std::path::PathBuf;

use phn_content::{ContentMeta, ReviewStatus};
use phn_core::{PhnError, Result, Source, SourceMode};
use phn_docs::model::Document;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// Load a bundled content item into its typed form.
pub(crate) fn load<T: DeserializeOwned>(id: &str) -> Result<T> {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PhnError::Internal(format!("content item {id} is not registered")))?;
    let v = phn_content::yaml_to_json(item.yaml).map_err(PhnError::Internal)?;
    serde_json::from_value(v).map_err(|e| PhnError::Internal(format!("{}: {e}", item.path)))
}

/// A source listed in a content file's `sources` map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContentSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub effective_date: Option<String>,
    pub retrieved: String,
}

impl ContentSource {
    /// A citation for responses (mode `snapshot`: bundled content).
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

/// Developer confidence in a drafted entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Text with its citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedText {
    pub text: String,
    /// Key in the content file's `sources` map.
    pub source: String,
    pub section: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// A cited note as returned to the model (citation resolved to a title).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedNote {
    pub text: String,
    /// Source title and section.
    pub citation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// Resolve `CitedText` items against a sources map.
pub(crate) fn notes(
    items: &[CitedText],
    sources: &std::collections::BTreeMap<String, ContentSource>,
) -> Vec<CitedNote> {
    items.iter().map(|n| note(n, sources)).collect()
}

/// Resolve one `CitedText`.
pub(crate) fn note(
    n: &CitedText,
    sources: &std::collections::BTreeMap<String, ContentSource>,
) -> CitedNote {
    CitedNote {
        text: n.text.clone(),
        citation: cite(sources, &n.source, &n.section),
        confidence: n.confidence,
    }
}

/// "Title, section" for a source key.
pub(crate) fn cite(
    sources: &std::collections::BTreeMap<String, ContentSource>,
    key: &str,
    section: &str,
) -> String {
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

/// Sources for a list of keys (deduplicated, in the order given).
pub(crate) fn sources_for<'a>(
    sources: &std::collections::BTreeMap<String, ContentSource>,
    keys: impl IntoIterator<Item = &'a str>,
) -> Vec<Source> {
    let mut seen: Vec<&str> = Vec::new();
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

/// The review caveat for a content item (none once reviewed).
pub(crate) fn review_caveat(meta: &ContentMeta) -> Option<String> {
    (meta.review.status == ReviewStatus::Pending).then(|| {
        format!(
            "REVIEW STATUS: '{}' (v{}, prepared {}) is a draft pending review by the subject \
             matter expert (review due {}). Verify against the cited sources before relying on it.",
            meta.title, meta.version, meta.prepared, meta.review.review_due
        )
    })
}

/// Text of a local note (TXT, MD, DOCX, PDF, HTML) and its path relative to
/// /data. The text is for local analysis only and must never be returned.
pub(crate) fn read_local_note(ctx: &Context, path: &str) -> Result<(String, String)> {
    let sandbox = ctx.sandbox()?;
    let full = sandbox.resolve_existing(path)?;
    if full.is_dir() {
        return Err(PhnError::InvalidInput(format!(
            "'{path}' is a folder; give the path of one note file"
        )));
    }
    let kind = phn_local::extract::FileKind::of(&full);
    if kind.is_tabular() || kind == phn_local::extract::FileKind::Image {
        return Err(PhnError::InvalidInput(
            "give a note as a text, Markdown, Word (.docx), PDF, or HTML file".into(),
        ));
    }
    let ex = phn_local::extract::extract(&full)?;
    if ex.text.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "the note file has no readable text (scanned PDFs need OCR, which this server does \
             not do)"
                .into(),
        ));
    }
    Ok((ex.text, sandbox.display_path(&full)))
}

/// An agency-approved translation in `/data/templates/<language>/<file_name>`,
/// if the nurse has provided one. Returns the text and its display path.
pub(crate) fn agency_template(
    ctx: &Context,
    language: &str,
    file_name: &str,
) -> Option<(String, String)> {
    let lang = language_folder(language)?;
    let sandbox = ctx.sandbox().ok()?;
    let rel = format!("templates/{lang}/{file_name}");
    let full: PathBuf = sandbox.resolve_existing(&rel).ok()?;
    let text = std::fs::read_to_string(&full).ok()?;
    (!text.trim().is_empty()).then(|| (text, sandbox.display_path(&full)))
}

/// Normalize a language name or code to a folder name (lower case, letters,
/// digits, `-` and `_`). English needs no folder.
pub(crate) fn language_folder(language: &str) -> Option<String> {
    let l: String = language
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '_' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if l.is_empty() || matches!(l.as_str(), "en" | "eng" | "english" | "en-us") {
        None
    } else {
        Some(l)
    }
}

/// Fill `{{placeholder}}` fields in an agency template.
pub(crate) fn fill_placeholders(text: &str, values: &[(&str, String)]) -> String {
    let mut out = text.to_string();
    for (k, v) in values {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

/// Write a document to /data/output in each format ("docx", "xlsx", "pdf",
/// "md"); returns the display paths.
pub(crate) fn write_document(
    ctx: &Context,
    doc: &Document,
    base_name: &str,
    formats: &[&str],
) -> Result<Vec<String>> {
    let sandbox = ctx.sandbox()?;
    let mut written = Vec::new();
    for f in formats {
        let bytes = match *f {
            "docx" => phn_docs::docx::to_docx(doc)?,
            "xlsx" => phn_docs::xlsx::to_xlsx(doc)?,
            "pdf" => phn_docs::pdf::to_pdf(doc)?,
            "md" => document_markdown(doc).into_bytes(),
            other => {
                return Err(PhnError::InvalidInput(format!(
                    "unsupported format '{other}' (use docx, xlsx, pdf, or md)"
                )));
            }
        };
        let path = sandbox.new_output_file(&format!("{base_name}.{f}"))?;
        std::fs::write(&path, bytes)?;
        written.push(sandbox.display_path(&path));
    }
    Ok(written)
}

/// Markdown rendering of a document (for `.md` output).
pub(crate) fn document_markdown(doc: &Document) -> String {
    use phn_docs::model::Block;
    let mut s = format!("# {}\n\n", doc.title);
    if let Some(sub) = &doc.subtitle {
        s.push_str(&format!("{sub}\n\n"));
    }
    for b in &doc.blocks {
        match b {
            Block::Heading { level, text } => {
                s.push_str(&format!("{} {text}\n\n", "#".repeat((*level as usize).clamp(1, 6))));
            }
            Block::Paragraph(runs) => {
                let t: String = runs.iter().map(|r| r.text.as_str()).collect();
                s.push_str(&format!("{t}\n\n"));
            }
            Block::List { ordered, items } => {
                for (i, it) in items.iter().enumerate() {
                    let t: String = it.iter().map(|r| r.text.as_str()).collect();
                    if *ordered {
                        s.push_str(&format!("{}. {t}\n", i + 1));
                    } else {
                        s.push_str(&format!("- {t}\n"));
                    }
                }
                s.push('\n');
            }
            Block::Table(t) => {
                let h: Vec<&str> = t.headers.iter().map(String::as_str).collect();
                s.push_str(&crate::md::table(&h, &t.rows));
                s.push('\n');
            }
            Block::Image(_) | Block::PageBreak => {}
        }
    }
    s
}

/// Parse a YYYY-MM-DD argument (error names the argument, not other input).
pub(crate) fn date_arg(name: &str, v: &Option<String>) -> Result<Option<chrono::NaiveDate>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| PhnError::InvalidInput(format!("{name} must be a date (YYYY-MM-DD)"))),
    }
}

/// Add business days (Monday-Friday; holidays are not known).
pub(crate) fn add_business_days(d: chrono::NaiveDate, n: u32) -> chrono::NaiveDate {
    use chrono::Datelike;
    let mut out = d;
    let mut left = n;
    while left > 0 {
        out += chrono::Duration::days(1);
        if out.weekday().number_from_monday() <= 5 {
            left -= 1;
        }
    }
    out
}

/// Plain paragraph block.
pub(crate) fn para(text: impl Into<String>) -> phn_docs::model::Block {
    phn_docs::model::Block::Paragraph(vec![phn_docs::model::Inline::plain(text)])
}

/// Heading block.
pub(crate) fn heading(level: u8, text: impl Into<String>) -> phn_docs::model::Block {
    phn_docs::model::Block::Heading {
        level,
        text: text.into(),
    }
}

/// Bulleted list block.
pub(crate) fn bullets(items: impl IntoIterator<Item = String>) -> phn_docs::model::Block {
    phn_docs::model::Block::List {
        ordered: false,
        items: items
            .into_iter()
            .map(|t| vec![phn_docs::model::Inline::plain(t)])
            .collect(),
    }
}

/// Table block.
pub(crate) fn table(
    caption: &str,
    headers: &[&str],
    rows: Vec<Vec<String>>,
) -> phn_docs::model::Block {
    phn_docs::model::Block::Table(phn_docs::model::TableBlock {
        caption: Some(caption.to_string()),
        headers: headers.iter().map(|h| h.to_string()).collect(),
        rows,
    })
}

/// A blank document shell with the agency letterhead.
pub(crate) fn document(ctx: &Context, title: &str, blocks: Vec<phn_docs::model::Block>) -> Document {
    Document {
        title: title.to_string(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

/// Lower-case text with runs of non-alphanumerics collapsed to one space,
/// padded with spaces so whole-word phrases can be found with `contains`.
pub(crate) fn padded_words(s: &str) -> String {
    let mut out = String::from(" ");
    let mut space = true;
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    if !space {
        out.push(' ');
    }
    out
}

/// Find a column by any of several normalized names (`first_name`, `dob`, ...).
pub(crate) fn find_col(t: &phn_local::table::Table, names: &[&str]) -> Option<usize> {
    t.headers.iter().position(|h| {
        let n = phn_core::text::normalize(h).replace(' ', "_");
        names.contains(&n.as_str())
    })
}

/// Columns that identify people (names, IDs, contact details), to be copied
/// to output files only. `inputs` are the columns the tool reads.
pub(crate) fn identifier_columns(
    t: &phn_local::table::Table,
    findings: &[phn_local::phi::ColumnFinding],
    inputs: &[usize],
) -> Vec<usize> {
    t.headers
        .iter()
        .enumerate()
        .filter(|(i, h)| {
            if inputs.contains(i) {
                return false;
            }
            let n = phn_core::text::normalize(h);
            findings.iter().any(|f| &f.column == *h && f.kind.is_direct())
                || n.contains("name")
                || n == "id"
                || n.ends_with(" id")
                || n.contains("phone")
                || n.contains("mrn")
        })
        .map(|(i, _)| i)
        .collect()
}

/// Counts in one row with primary and complementary suppression; returns the
/// shown value for each label ("*" when hidden) and whether anything was hidden.
pub(crate) fn suppressed_counts(
    counts: &[(String, u64)],
    threshold: u32,
) -> (Vec<(String, String)>, bool) {
    let row: Vec<Option<u64>> = counts.iter().map(|(_, c)| Some(*c)).collect();
    let sup = phn_epi::suppression::suppress(
        &[row],
        &phn_epi::suppression::SuppressionOptions {
            threshold,
            ..phn_epi::suppression::SuppressionOptions::default()
        },
    );
    let mut any = false;
    let shown = counts
        .iter()
        .enumerate()
        .map(|(i, (label, c))| {
            if sup.is_hidden(0, i) {
                any = true;
                (label.clone(), "*".to_string())
            } else {
                (label.clone(), c.to_string())
            }
        })
        .collect();
    (shown, any)
}

/// True for yes-like cells ("Y", "yes", "true", "1", "x").
pub(crate) fn yes(cell: &str) -> bool {
    crate::localdata::truthy(cell) == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn suppression_hides_small_counts_and_a_complement() {
        let (shown, any) = suppressed_counts(
            &[("a".into(), 2), ("b".into(), 30), ("c".into(), 40)],
            5,
        );
        assert!(any);
        assert_eq!(shown[0].1, "*");
        assert_eq!(shown.iter().filter(|(_, v)| v == "*").count(), 2);
    }

    #[test]
    fn business_days_skip_weekends() {
        // Friday 2026-11-06 + 1 business day = Monday 2026-11-09.
        let fri = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        assert_eq!(
            add_business_days(fri, 1),
            NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
        );
        assert_eq!(
            add_business_days(fri, 7),
            NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
        );
    }

    #[test]
    fn padded_words_and_languages() {
        assert_eq!(padded_words("Taught re: meds!"), " taught re meds ");
        assert_eq!(language_folder("English"), None);
        assert_eq!(language_folder("Haitian Creole").as_deref(), Some("haitian_creole"));
        assert_eq!(language_folder("../es").as_deref(), Some("es"));
        assert_eq!(
            fill_placeholders("Start {{start}}", &[("start", "2026-01-01".into())]),
            "Start 2026-01-01"
        );
    }
}
