//! Text and table extraction for any supported file, plus folder listings.

use std::path::Path;

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::sandbox::Sandbox;
use crate::table::{self, Table};

/// Broad file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Csv,
    Spreadsheet,
    Docx,
    Pdf,
    Html,
    Text,
    Markdown,
    Image,
    Other,
}

impl FileKind {
    /// Classify by extension.
    pub fn of(path: &Path) -> Self {
        match table::extension(path).as_str() {
            "csv" | "tsv" => FileKind::Csv,
            "xlsx" | "xlsm" | "xls" | "ods" => FileKind::Spreadsheet,
            "docx" => FileKind::Docx,
            "pdf" => FileKind::Pdf,
            "html" | "htm" => FileKind::Html,
            "txt" | "text" => FileKind::Text,
            "md" | "markdown" => FileKind::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => FileKind::Image,
            _ => FileKind::Other,
        }
    }

    /// True for row-and-column data files.
    pub fn is_tabular(self) -> bool {
        matches!(self, FileKind::Csv | FileKind::Spreadsheet)
    }
}

/// A titled part of a document (heading or page).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Section {
    /// Heading text, or "Page N" for PDFs.
    pub heading: String,
    /// Heading level (1 = top); 0 for untitled leading text.
    pub level: u8,
    /// Page number for PDFs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    pub text: String,
}

/// Extracted content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extracted {
    pub kind: FileKind,
    pub text: String,
    pub sections: Vec<Section>,
    pub tables: Vec<Table>,
    pub pages: Option<usize>,
    pub notes: Vec<String>,
}

/// Extract text, sections, and tables from a document or data file.
pub fn extract(path: &Path) -> Result<Extracted> {
    let kind = FileKind::of(path);
    let mut notes = Vec::new();
    match kind {
        FileKind::Csv | FileKind::Spreadsheet => {
            let tables = table::read_tables(path)?;
            Ok(Extracted {
                kind,
                text: String::new(),
                sections: Vec::new(),
                tables,
                pages: None,
                notes,
            })
        }
        FileKind::Docx => {
            let doc = crate::docx::read_docx(path)?;
            let sections = docx_sections(&doc);
            let tables = doc
                .tables()
                .enumerate()
                .filter(|(_, t)| !t.rows.is_empty())
                .map(|(i, t)| Table {
                    name: format!("Table {}", i + 1),
                    headers: t.rows[0].clone(),
                    rows: t.rows[1..].to_vec(),
                    truncated: false,
                })
                .collect();
            Ok(Extracted {
                kind,
                text: doc.plain_text(),
                sections,
                tables,
                pages: None,
                notes,
            })
        }
        FileKind::Pdf => {
            let info = crate::pdf::read_pdf(path)?;
            if info.encrypted {
                notes.push("The PDF is encrypted; text may be incomplete.".into());
            }
            if !info.pages_without_text.is_empty() {
                notes.push(format!(
                    "No text was found on {} of {} page(s); they may be scanned images (OCR is \
                     not available).",
                    info.pages_without_text.len(),
                    info.pages
                ));
            }
            let sections = info
                .page_texts
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.trim().is_empty())
                .map(|(i, t)| Section {
                    heading: format!("Page {}", i + 1),
                    level: 1,
                    page: Some(i as u32 + 1),
                    text: t.trim().to_string(),
                })
                .collect();
            Ok(Extracted {
                kind,
                text: info.text(),
                sections,
                tables: Vec::new(),
                pages: Some(info.pages),
                notes,
            })
        }
        FileKind::Html => {
            let source = table::decode_text(&std::fs::read(path)?);
            let doc = crate::html::parse_html(&source);
            Ok(Extracted {
                kind,
                sections: vec![Section {
                    heading: doc.title.clone().unwrap_or_default(),
                    level: 0,
                    page: None,
                    text: doc.text.clone(),
                }],
                text: doc.text,
                tables: Vec::new(),
                pages: None,
                notes,
            })
        }
        FileKind::Text | FileKind::Markdown => {
            let text = table::decode_text(&std::fs::read(path)?);
            let sections = if kind == FileKind::Markdown {
                markdown_sections(&text)
            } else {
                vec![Section {
                    heading: String::new(),
                    level: 0,
                    page: None,
                    text: text.clone(),
                }]
            };
            Ok(Extracted {
                kind,
                text,
                sections,
                tables: Vec::new(),
                pages: None,
                notes,
            })
        }
        FileKind::Image | FileKind::Other => Err(PhnError::Unsupported(format!(
            "cannot extract text from {} (supported: PDF, DOCX, XLSX/XLS/ODS, CSV/TSV, HTML, TXT, MD)",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        ))),
    }
}

fn docx_sections(doc: &crate::docx::DocxDocument) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut current = Section {
        heading: String::new(),
        level: 0,
        page: None,
        text: String::new(),
    };
    for b in &doc.blocks {
        match b {
            crate::docx::Block::Paragraph(p) if !p.in_table => {
                if let Some(level) = p.heading_level.filter(|_| !p.text.trim().is_empty()) {
                    if !current.text.trim().is_empty() || !current.heading.is_empty() {
                        out.push(std::mem::replace(
                            &mut current,
                            Section {
                                heading: String::new(),
                                level: 0,
                                page: None,
                                text: String::new(),
                            },
                        ));
                    }
                    current.heading = p.text.trim().to_string();
                    current.level = level;
                } else if !p.text.trim().is_empty() {
                    current.text.push_str(p.text.trim());
                    current.text.push('\n');
                }
            }
            crate::docx::Block::Table(t) => {
                for r in &t.rows {
                    current.text.push_str(&r.join(" | "));
                    current.text.push('\n');
                }
            }
            _ => {}
        }
    }
    if !current.text.trim().is_empty() || !current.heading.is_empty() {
        out.push(current);
    }
    out
}

fn markdown_sections(text: &str) -> Vec<Section> {
    let mut out = Vec::new();
    let mut current = Section {
        heading: String::new(),
        level: 0,
        page: None,
        text: String::new(),
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            if !current.text.trim().is_empty() || !current.heading.is_empty() {
                out.push(current);
            }
            current = Section {
                heading: trimmed[hashes..].trim().to_string(),
                level: hashes as u8,
                page: None,
                text: String::new(),
            };
        } else {
            current.text.push_str(line);
            current.text.push('\n');
        }
    }
    if !current.text.trim().is_empty() || !current.heading.is_empty() {
        out.push(current);
    }
    out
}

/// One entry in a folder listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FileEntry {
    /// Path relative to /data.
    pub path: String,
    pub kind: FileKind,
    pub bytes: u64,
    /// Last modified (UTC, RFC 3339).
    pub modified: Option<String>,
    pub is_dir: bool,
}

/// List files under a folder in the sandbox. Hidden files are skipped.
pub fn list_files(
    sandbox: &Sandbox,
    folder: &str,
    recursive: bool,
    limit: usize,
) -> Result<(Vec<FileEntry>, bool)> {
    let root = if folder.trim().is_empty() || folder.trim() == "." || folder.trim() == "/data" {
        sandbox.root().to_path_buf()
    } else {
        sandbox.resolve_existing(folder)?
    };
    if !root.is_dir() {
        return Err(PhnError::InvalidInput(format!(
            "'{folder}' is not a folder"
        )));
    }
    let mut out = Vec::new();
    let mut truncated = false;
    let walker = walkdir::WalkDir::new(&root)
        .min_depth(1)
        .max_depth(if recursive { 8 } else { 1 })
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));
    for entry in walker.flatten() {
        if out.len() >= limit {
            truncated = true;
            break;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified = meta.modified().ok().map(|t| {
            chrono::DateTime::<chrono::Utc>::from(t)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        });
        out.push(FileEntry {
            path: sandbox.display_path(entry.path()),
            kind: FileKind::of(entry.path()),
            bytes: if meta.is_dir() { 0 } else { meta.len() },
            modified,
            is_dir: meta.is_dir(),
        });
    }
    Ok((out, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_markdown_sections() {
        let s = markdown_sections("intro\n# One\na\n## Two\nb\n");
        assert_eq!(s.len(), 3);
        assert_eq!(s[1].heading, "One");
        assert_eq!(s[2].level, 2);
    }

    #[test]
    fn lists_and_extracts() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("policies")).unwrap();
        std::fs::write(data.join("policies/a.md"), "# Title\nBody").unwrap();
        std::fs::write(data.join(".hidden"), "x").unwrap();
        let sb = Sandbox::new(&data, &data.join("output")).unwrap();
        let (files, _) = list_files(&sb, "", true, 100).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["policies", "policies/a.md"]);
        let e = extract(&data.join("policies/a.md")).unwrap();
        assert_eq!(e.sections[0].heading, "Title");
    }
}
