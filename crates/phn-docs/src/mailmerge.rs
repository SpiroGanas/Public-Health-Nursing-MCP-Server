//! Local mail-merge.
//!
//! A template document contains placeholders such as `{{First Name}}` that
//! match column names in a local CSV/XLSX file. Merged documents are written
//! to the output folder by the caller; only counts and generic file names are
//! ever returned to the model, never the merged values.

use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use phn_core::{PhnError, Result};
use phn_local::table::Table;
use regex::Regex;

use crate::model::{Block, Document};

static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").expect("valid regex"));

/// Placeholder names used in a document.
pub fn placeholders(doc: &Document) -> BTreeSet<String> {
    PLACEHOLDER
        .captures_iter(&doc.all_text())
        .map(|c| c[1].to_string())
        .collect()
}

/// Map placeholders to column indexes; error on unknown placeholders.
pub fn bind(doc: &Document, table: &Table) -> Result<HashMap<String, usize>> {
    let names = placeholders(doc);
    if names.is_empty() {
        return Err(PhnError::InvalidInput(
            "the template has no {{placeholders}}; add fields such as {{First Name}} that match \
             column names in the data file"
                .into(),
        ));
    }
    let norm = |s: &str| phn_core::text::normalize(s);
    let mut map = HashMap::new();
    let mut unknown = Vec::new();
    for n in &names {
        match table.headers.iter().position(|h| norm(h) == norm(n)) {
            Some(i) => {
                map.insert(n.clone(), i);
            }
            None => unknown.push(n.clone()),
        }
    }
    if !unknown.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "placeholder(s) {} do not match any column. Columns in the data file: {}",
            unknown
                .iter()
                .map(|u| format!("{{{{{u}}}}}"))
                .collect::<Vec<_>>()
                .join(", "),
            table.headers.join(", ")
        )));
    }
    Ok(map)
}

/// Fill one record. Returns the document and the number of empty values used.
pub fn fill(
    template: &Document,
    binding: &HashMap<String, usize>,
    row: &[String],
) -> (Document, usize) {
    let empty = std::cell::Cell::new(0usize);
    let doc = template.map_text(&|s: &str| {
        PLACEHOLDER
            .replace_all(s, |c: &regex::Captures<'_>| {
                let v = binding
                    .get(&c[1])
                    .and_then(|&i| row.get(i))
                    .map(|v| v.trim().to_string())
                    .unwrap_or_default();
                if v.is_empty() {
                    empty.set(empty.get() + 1);
                }
                v
            })
            .into_owned()
    });
    (doc, empty.get())
}

/// One combined document with a page break between records.
pub fn combined(
    template: &Document,
    table: &Table,
    binding: &HashMap<String, usize>,
) -> (Document, usize) {
    let mut out = template.clone();
    out.title = PLACEHOLDER
        .replace_all(&template.title, "")
        .trim()
        .to_string();
    out.subtitle = None;
    out.blocks.clear();
    let mut empty = 0;
    for (i, row) in table.rows.iter().enumerate() {
        let (d, e) = fill(template, binding, row);
        empty += e;
        if i > 0 {
            out.blocks.push(Block::PageBreak);
        }
        if d.title != out.title || i == 0 {
            // Keep a per-record heading so each letter has structure.
            out.blocks.push(Block::Heading {
                level: 1,
                text: d.title.clone(),
            });
        }
        if let Some(s) = d.subtitle {
            out.blocks
                .push(Block::Paragraph(vec![crate::model::Inline::plain(s)]));
        }
        out.blocks.extend(d.blocks);
    }
    (out, empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::from_markdown;

    fn template() -> Document {
        Document {
            title: "Exclusion notice for {{Student}}".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: None,
            logo: None,
            blocks: from_markdown(
                "Dear family of **{{ student }}**,\n\nPlease keep your child home until {{Return Date}}.",
            ),
        }
    }

    fn data() -> Table {
        Table {
            name: "d".into(),
            headers: vec!["Student".into(), "Return date".into()],
            rows: vec![
                vec!["Ana Lima".into(), "2025-01-10".into()],
                vec!["Ben Cho".into(), "".into()],
            ],
            truncated: false,
        }
    }

    #[test]
    fn binds_fills_and_combines() {
        let t = template();
        let b = bind(&t, &data()).unwrap();
        let (d, empty) = fill(&t, &b, &data().rows[0]);
        assert_eq!(empty, 0);
        assert_eq!(d.title, "Exclusion notice for Ana Lima");
        assert!(d.all_text().contains("until 2025-01-10"));
        let (c, empty) = combined(&t, &data(), &b);
        assert_eq!(empty, 1);
        assert_eq!(
            c.blocks
                .iter()
                .filter(|b| matches!(b, Block::PageBreak))
                .count(),
            1
        );
    }

    #[test]
    fn unknown_placeholders_are_reported() {
        let mut t = template();
        t.blocks
            .push(Block::Paragraph(vec![crate::model::Inline::plain(
                "{{Nurse}}",
            )]));
        let err = bind(&t, &data()).unwrap_err().to_string();
        assert!(err.contains("{{Nurse}}") && err.contains("Columns in the data file"));
    }
}
