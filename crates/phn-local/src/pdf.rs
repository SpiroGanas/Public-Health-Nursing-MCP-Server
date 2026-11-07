//! PDF reader: text by page, plus the document properties that matter for
//! accessibility (tagging, language, title, embedded fonts).

use std::path::Path;

use lopdf::{Dictionary, Document, Object};
use phn_core::{PhnError, Result};
use serde::{Deserialize, Serialize};

/// What the server knows about a PDF.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PdfInfo {
    pub pages: usize,
    pub page_texts: Vec<String>,
    pub title: Option<String>,
    pub language: Option<String>,
    /// `/MarkInfo << /Marked true >>`: the PDF claims to be tagged.
    pub marked: bool,
    /// A structure tree (`/StructTreeRoot`) is present.
    pub has_struct_tree: bool,
    /// `/ViewerPreferences << /DisplayDocTitle true >>`.
    pub display_doc_title: bool,
    pub encrypted: bool,
    /// Fonts used without an embedded font program.
    pub unembedded_fonts: Vec<String>,
    /// Pages where no text could be extracted (possibly scanned images).
    pub pages_without_text: Vec<usize>,
    /// `Figure` elements in the structure tree.
    pub figures: usize,
    /// `Figure` elements without `/Alt` or `/ActualText`.
    pub figures_without_alt: usize,
}

impl PdfInfo {
    /// All text, pages separated by form feeds.
    pub fn text(&self) -> String {
        self.page_texts.join("\n\u{c}\n")
    }
}

/// Read a PDF file.
pub fn read_pdf(path: &Path) -> Result<PdfInfo> {
    let bytes = std::fs::read(path)?;
    parse_pdf(&bytes).map_err(|e| PhnError::parse(path, e))
}

/// Parse PDF bytes.
pub fn parse_pdf(bytes: &[u8]) -> std::result::Result<PdfInfo, String> {
    let doc = std::panic::catch_unwind(|| Document::load_mem(bytes))
        .map_err(|_| "the PDF parser failed on this file".to_string())?
        .map_err(|e| format!("not a readable PDF: {e}"))?;
    let mut info = PdfInfo {
        encrypted: doc.is_encrypted(),
        ..PdfInfo::default()
    };
    let pages = doc.get_pages();
    info.pages = pages.len();
    for (&num, &page_id) in &pages {
        let text =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.extract_text(&[num])))
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
        if text.trim().is_empty() {
            info.pages_without_text.push(num as usize);
        }
        info.page_texts.push(text);
        if let Ok(fonts) = doc.get_page_fonts(page_id) {
            for (name, font) in fonts {
                if !font_is_embedded(&doc, font) {
                    let n = font
                        .get(b"BaseFont")
                        .ok()
                        .and_then(|o| o.as_name().ok())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .unwrap_or_else(|| String::from_utf8_lossy(&name).into_owned());
                    if !info.unembedded_fonts.contains(&n) {
                        info.unembedded_fonts.push(n);
                    }
                }
            }
        }
    }
    if let Ok(catalog) = doc.catalog() {
        info.language = catalog
            .get(b"Lang")
            .ok()
            .and_then(|o| pdf_string(&doc, o))
            .filter(|s| !s.trim().is_empty());
        info.has_struct_tree = catalog.has(b"StructTreeRoot");
        if let Ok(root) = catalog.get(b"StructTreeRoot") {
            let mut visited = std::collections::HashSet::new();
            walk_struct(&doc, root, 0, &mut visited, &mut info);
        }
        info.marked = deref_dict(&doc, catalog.get(b"MarkInfo").ok())
            .and_then(|d| d.get(b"Marked").ok().and_then(|o| o.as_bool().ok()))
            .unwrap_or(false);
        info.display_doc_title = deref_dict(&doc, catalog.get(b"ViewerPreferences").ok())
            .and_then(|d| {
                d.get(b"DisplayDocTitle")
                    .ok()
                    .and_then(|o| o.as_bool().ok())
            })
            .unwrap_or(false);
    }
    info.title = deref_dict(&doc, doc.trailer.get(b"Info").ok())
        .and_then(|d| d.get(b"Title").ok().and_then(|o| pdf_string(&doc, o)))
        .filter(|s| !s.trim().is_empty());
    Ok(info)
}

/// Count `Figure` elements (and those without alt text) in the structure tree.
fn walk_struct(
    doc: &Document,
    obj: &Object,
    depth: usize,
    visited: &mut std::collections::HashSet<lopdf::ObjectId>,
    info: &mut PdfInfo,
) {
    if depth > 64 || visited.len() > 200_000 {
        return;
    }
    let obj = match obj {
        Object::Reference(id) => {
            if !visited.insert(*id) {
                return;
            }
            match doc.get_object(*id) {
                Ok(o) => o,
                Err(_) => return,
            }
        }
        o => o,
    };
    match obj {
        Object::Array(items) => {
            for i in items {
                walk_struct(doc, i, depth + 1, visited, info);
            }
        }
        Object::Dictionary(d) => {
            if d.get(b"S").ok().and_then(|s| s.as_name().ok()) == Some(b"Figure".as_slice()) {
                info.figures += 1;
                let has_text = |k: &[u8]| {
                    d.get(k)
                        .ok()
                        .and_then(|o| pdf_string(doc, o))
                        .is_some_and(|s| !s.trim().is_empty())
                };
                if !has_text(b"Alt") && !has_text(b"ActualText") {
                    info.figures_without_alt += 1;
                }
            }
            if let Ok(k) = d.get(b"K") {
                walk_struct(doc, k, depth + 1, visited, info);
            }
        }
        _ => {}
    }
}

fn deref_dict<'a>(doc: &'a Document, obj: Option<&'a Object>) -> Option<&'a Dictionary> {
    match obj? {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn pdf_string(doc: &Document, obj: &Object) -> Option<String> {
    let obj = match obj {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        o => o,
    };
    match obj {
        Object::String(bytes, _) => Some(decode_pdf_text(bytes)),
        _ => None,
    }
}

/// Decode a PDF text string: UTF-16BE with BOM, UTF-8 with BOM, else Latin-1.
pub fn decode_pdf_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().map(|&b| b as char).collect()
}

fn font_is_embedded(doc: &Document, font: &Dictionary) -> bool {
    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .unwrap_or(b"");
    if subtype == b"Type3" {
        return true; // glyphs are defined in the PDF itself
    }
    let descriptor_of = |d: &Dictionary| -> Option<bool> {
        let fd = deref_dict(doc, d.get(b"FontDescriptor").ok())?;
        Some(fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3"))
    };
    if subtype == b"Type0" {
        let desc = font.get(b"DescendantFonts").ok().and_then(|o| match o {
            Object::Array(a) => a.first(),
            Object::Reference(id) => doc
                .get_object(*id)
                .ok()
                .and_then(|o| o.as_array().ok())
                .and_then(|a| a.first()),
            _ => None,
        });
        return deref_dict(doc, desc)
            .and_then(descriptor_of)
            .unwrap_or(false);
    }
    descriptor_of(font).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_text_strings() {
        assert_eq!(decode_pdf_text(&[0xFE, 0xFF, 0x00, 0x41, 0x00, 0xE9]), "Aé");
        assert_eq!(decode_pdf_text(b"Plain"), "Plain");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_pdf(b"hello").is_err());
    }
}
