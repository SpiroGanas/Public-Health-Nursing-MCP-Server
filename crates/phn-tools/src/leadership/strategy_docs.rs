//! Shared helpers for the Phase 18 strategy tools (LEAD-03, LEAD-04, LEAD-05): a small document
//! builder that renders to DOCX and XLSX, and output-folder helpers.

use phn_core::Result;
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::school::templates;

/// File formats a strategy tool can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutFormat {
    /// Word document.
    Docx,
    /// Excel workbook (one worksheet per table).
    Xlsx,
}

impl OutFormat {
    pub fn ext(self) -> &'static str {
        match self {
            OutFormat::Docx => "docx",
            OutFormat::Xlsx => "xlsx",
        }
    }
}

/// Trim and collapse whitespace.
pub fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `tidy`, with empty meaning absent.
pub fn tidy_opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(tidy).filter(|t| !t.is_empty())
}

/// At most `max` characters, with an ellipsis when cut (never splits a character).
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('\u{2026}');
        t
    }
}

/// Builds a [`Document`] block by block.
#[derive(Debug, Default)]
pub struct DocBuilder {
    pub blocks: Vec<Block>,
}

impl DocBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn heading(&mut self, level: u8, text: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Heading {
            level,
            text: text.into(),
        });
        self
    }

    pub fn para(&mut self, text: impl Into<String>) -> &mut Self {
        self.blocks
            .push(Block::Paragraph(vec![Inline::plain(text.into())]));
        self
    }

    /// A paragraph that starts with a bold label.
    pub fn labeled(&mut self, label: &str, text: impl Into<String>) -> &mut Self {
        self.blocks.push(Block::Paragraph(vec![
            Inline {
                text: format!("{label} "),
                bold: true,
                ..Inline::default()
            },
            Inline::plain(text.into()),
        ]));
        self
    }

    pub fn list<S: AsRef<str>>(&mut self, items: &[S]) -> &mut Self {
        if items.is_empty() {
            return self;
        }
        self.blocks.push(Block::List {
            ordered: false,
            items: items
                .iter()
                .map(|i| vec![Inline::plain(i.as_ref())])
                .collect(),
        });
        self
    }

    /// A captioned table. The caption is also the worksheet name in XLSX (keep it short).
    pub fn table(&mut self, caption: &str, headers: &[&str], rows: Vec<Vec<String>>) -> &mut Self {
        self.blocks.push(Block::Table(TableBlock {
            caption: Some(caption.to_string()),
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows,
        }));
        self
    }

    pub fn finish(self, ctx: &Context, title: &str, subtitle: Option<String>) -> Document {
        Document {
            title: title.to_string(),
            subtitle,
            language: "en-US".to_string(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: self.blocks,
        }
    }
}

/// Write `doc` in each format to a new folder under /data/output. Returns the folder's display
/// path and the display path of each file written. `base` is the file name without extension.
pub fn write_documents(
    ctx: &Context,
    folder: &str,
    base: &str,
    doc: &Document,
    formats: &[OutFormat],
    xlsx_first_table: Option<&str>,
) -> Result<(String, Vec<String>)> {
    let (dir, shown) = templates::output_folder(ctx, folder)?;
    let mut files = Vec::new();
    for f in formats {
        let bytes = match f {
            OutFormat::Docx => phn_docs::docx::to_docx(doc)?,
            OutFormat::Xlsx => match xlsx_first_table {
                Some(name) => phn_docs::xlsx::to_xlsx(&table_first(doc, name))?,
                None => phn_docs::xlsx::to_xlsx(doc)?,
            },
        };
        files.push(templates::write_in(
            ctx,
            &dir,
            &format!("{base}.{}", f.ext()),
            &bytes,
        )?);
    }
    Ok((shown, files))
}

/// A copy of `doc` with the table captioned `name` moved to the front, so it is the first
/// worksheet of an XLSX.
pub fn table_first(doc: &Document, name: &str) -> Document {
    let mut d = doc.clone();
    if let Some(i) = d
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Table(t) if t.caption.as_deref() == Some(name)))
    {
        let t = d.blocks.remove(i);
        d.blocks.insert(0, t);
    }
    d
}

/// Formats to write: the arguments' list, or both when empty.
pub fn formats_or_default(given: &[OutFormat]) -> Vec<OutFormat> {
    let mut v: Vec<OutFormat> = Vec::new();
    for f in given {
        if !v.contains(f) {
            v.push(*f);
        }
    }
    if v.is_empty() {
        v = vec![OutFormat::Docx, OutFormat::Xlsx];
    }
    v
}

/// Markdown-free text for a percent with one decimal.
pub fn pct(num: usize, den: usize) -> Option<f64> {
    if den == 0 {
        None
    } else {
        Some((num as f64 / den as f64 * 1000.0).round() / 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_is_utf8_safe() {
        assert_eq!(clip("abcdef", 4), "abc\u{2026}");
        assert_eq!(clip("éééé", 10), "éééé");
        assert_eq!(clip("ééééé", 3), "éé\u{2026}");
    }

    #[test]
    fn percent_rounds_to_one_decimal() {
        assert_eq!(pct(1, 3), Some(33.3));
        assert_eq!(pct(0, 0), None);
        assert_eq!(pct(2, 2), Some(100.0));
    }

    #[test]
    fn a_named_table_moves_to_the_front() {
        let mut b = DocBuilder::new();
        b.heading(1, "H");
        b.table("First", &["a"], vec![vec!["1".into()]]);
        b.table("Second", &["b"], vec![vec!["2".into()]]);
        let doc = Document {
            title: "t".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: None,
            logo: None,
            blocks: b.blocks,
        };
        let d = table_first(&doc, "Second");
        assert!(matches!(&d.blocks[0], Block::Table(t) if t.caption.as_deref() == Some("Second")));
        assert_eq!(d.blocks.len(), 3);
    }

    #[test]
    fn formats_default_to_both() {
        assert_eq!(
            formats_or_default(&[]),
            vec![OutFormat::Docx, OutFormat::Xlsx]
        );
        assert_eq!(
            formats_or_default(&[OutFormat::Xlsx, OutFormat::Xlsx]),
            vec![OutFormat::Xlsx]
        );
    }
}
