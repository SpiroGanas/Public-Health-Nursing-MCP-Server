//! DOCX reader.
//!
//! Produces a structural model rich enough for text extraction, policy
//! indexing (headings become sections), and accessibility checks (headings,
//! table header rows, image alt text, text colors, title, and language).

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use phn_core::{PhnError, Result};
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};

/// A paragraph.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    pub style_id: Option<String>,
    pub style_name: Option<String>,
    /// 1-based heading level from the style or paragraph outline level.
    pub heading_level: Option<u8>,
    pub text: String,
    /// Every run with text is bold (a "visual heading" when short).
    pub all_bold: bool,
    /// Largest font size in points, when set directly.
    pub max_font_pt: Option<f32>,
    /// Paragraph is part of a numbered or bulleted list.
    pub is_list: bool,
    /// Paragraph is inside a table cell.
    pub in_table: bool,
}

/// A table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocxTable {
    pub rows: Vec<Vec<String>>,
    /// The first row is marked "repeat as header row" (`w:tblHeader`).
    pub header_row_marked: bool,
    /// Number of cells with merged ranges (`gridSpan`/`vMerge`).
    pub merged_cells: usize,
}

/// A block in document order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Paragraph(Paragraph),
    Table(DocxTable),
}

/// An image or other drawing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub name: Option<String>,
    pub alt_text: Option<String>,
    pub title: Option<String>,
    /// Marked decorative in Word (`adec:decorative`).
    pub decorative: bool,
}

/// A run of text with explicit color information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColoredRun {
    pub text: String,
    /// Foreground color as RRGGBB.
    pub color: String,
    /// Background color as RRGGBB (shading or highlight), when set.
    pub background: Option<String>,
    pub bold: bool,
    pub size_pt: Option<f32>,
}

/// A parsed DOCX file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocxDocument {
    pub title: Option<String>,
    pub language: Option<String>,
    pub blocks: Vec<Block>,
    pub drawings: Vec<Drawing>,
    pub hyperlink_texts: Vec<String>,
    pub colored_runs: Vec<ColoredRun>,
}

impl DocxDocument {
    /// Plain text with blank lines between blocks; tables as tab-separated rows.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            match b {
                Block::Paragraph(p) if !p.in_table => {
                    if !p.text.trim().is_empty() {
                        if let Some(l) = p.heading_level {
                            out.push_str(&"#".repeat(l as usize));
                            out.push(' ');
                        }
                        out.push_str(p.text.trim());
                        out.push_str("\n\n");
                    }
                }
                Block::Table(t) => {
                    for r in &t.rows {
                        out.push_str(&r.join("\t"));
                        out.push('\n');
                    }
                    out.push('\n');
                }
                _ => {}
            }
        }
        out
    }

    /// Paragraphs outside tables.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Paragraph(p) if !p.in_table => Some(p),
            _ => None,
        })
    }

    /// Tables.
    pub fn tables(&self) -> impl Iterator<Item = &DocxTable> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, Default)]
struct StyleInfo {
    name: Option<String>,
    outline_level: Option<u8>,
    based_on: Option<String>,
    bold: bool,
}

/// Read a DOCX file.
pub fn read_docx(path: &Path) -> Result<DocxDocument> {
    let bytes = std::fs::read(path)?;
    parse_docx(&bytes).map_err(|e| PhnError::parse(path, e))
}

/// Parse DOCX bytes.
pub fn parse_docx(bytes: &[u8]) -> std::result::Result<DocxDocument, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("not a DOCX (zip) file: {e}"))?;
    let document_xml = read_member(&mut zip, "word/document.xml")?
        .ok_or("missing word/document.xml (not a Word document)")?;
    let styles_xml = read_member(&mut zip, "word/styles.xml")?.unwrap_or_default();
    let core_xml = read_member(&mut zip, "docProps/core.xml")?.unwrap_or_default();
    let settings_xml = read_member(&mut zip, "word/settings.xml")?.unwrap_or_default();

    let (styles, default_lang) = parse_styles(&styles_xml)?;
    let mut doc = parse_document(&document_xml, &styles)?;
    doc.title = parse_core_title(&core_xml)?;
    doc.language = default_lang.or_else(|| theme_font_lang(&settings_xml));
    Ok(doc)
}

fn read_member(
    zip: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> std::result::Result<Option<String>, String> {
    match zip.by_name(name) {
        Ok(mut f) => {
            let mut s = String::new();
            f.read_to_string(&mut s)
                .map_err(|e| format!("{name}: {e}"))?;
            Ok(Some(s))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

fn local(e: &BytesStart<'_>) -> String {
    e.local_name().as_ref().to_string()
}

fn attr(e: &BytesStart<'_>, local_name: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        (a.key.local_name().as_ref() == local_name)
            .then(|| {
                a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .ok()
                    .map(|v| v.into_owned())
            })
            .flatten()
    })
}

/// Text of an entity reference event.
pub(crate) fn resolve_ref(r: &BytesRef<'_>) -> String {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        return c.to_string();
    }
    match r.xml10_content().as_ref() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
    .to_string()
}

/// `w:b` and similar toggles: present without `w:val`, or val true/1/on.
fn toggle_on(e: &BytesStart<'_>) -> bool {
    match attr(e, "val") {
        None => true,
        Some(v) => matches!(v.as_str(), "1" | "true" | "on"),
    }
}

fn parse_styles(
    xml: &str,
) -> std::result::Result<(HashMap<String, StyleInfo>, Option<String>), String> {
    let mut styles: HashMap<String, StyleInfo> = HashMap::new();
    let mut default_lang = None;
    if xml.is_empty() {
        return Ok((styles, None));
    }
    let mut reader = Reader::from_str(xml);
    let mut current: Option<(String, StyleInfo)> = None;
    let mut in_doc_defaults = false;
    let mut in_rpr = false;
    loop {
        match reader
            .read_event()
            .map_err(|e| format!("styles.xml: {e}"))?
        {
            Event::Start(e) | Event::Empty(e) => {
                let name = local(&e);
                match name.as_str() {
                    "docDefaults" => in_doc_defaults = true,
                    "style" => {
                        if let Some(id) = attr(&e, "styleId") {
                            current = Some((id, StyleInfo::default()));
                        }
                    }
                    "rPr" => in_rpr = true,
                    "name" => {
                        if let Some((_, s)) = current.as_mut() {
                            s.name = attr(&e, "val");
                        }
                    }
                    "basedOn" => {
                        if let Some((_, s)) = current.as_mut() {
                            s.based_on = attr(&e, "val");
                        }
                    }
                    "outlineLvl" => {
                        if let Some((_, s)) = current.as_mut() {
                            s.outline_level = attr(&e, "val").and_then(|v| v.parse().ok());
                        }
                    }
                    "b" if in_rpr => {
                        if let Some((_, s)) = current.as_mut() {
                            s.bold = toggle_on(&e);
                        }
                    }
                    "lang" if in_doc_defaults && default_lang.is_none() => {
                        default_lang = attr(&e, "val");
                    }
                    _ => {}
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "docDefaults" => in_doc_defaults = false,
                "rPr" => in_rpr = false,
                "style" => {
                    if let Some((id, s)) = current.take() {
                        styles.insert(id, s);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok((styles, default_lang))
}

fn theme_font_lang(settings_xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(settings_xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if local(&e) == "themeFontLang" => {
                return attr(&e, "val");
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

fn parse_core_title(xml: &str) -> std::result::Result<Option<String>, String> {
    if xml.is_empty() {
        return Ok(None);
    }
    let mut reader = Reader::from_str(xml);
    let mut in_title = false;
    let mut title = String::new();
    loop {
        match reader.read_event().map_err(|e| format!("core.xml: {e}"))? {
            Event::Start(e) if local(&e) == "title" => in_title = true,
            Event::End(e) if e.local_name().as_ref() == "title" => in_title = false,
            Event::Text(t) if in_title => title.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if in_title => title.push_str(&resolve_ref(&r)),
            Event::Eof => break,
            _ => {}
        }
    }
    let t = title.trim().to_string();
    Ok((!t.is_empty()).then_some(t))
}

/// Resolve a style's heading level, following `basedOn`.
fn style_heading_level(styles: &HashMap<String, StyleInfo>, id: &str) -> Option<u8> {
    let mut cur = Some(id.to_string());
    for _ in 0..10 {
        let id = cur?;
        if let Some(lvl) = heading_from_name(&id) {
            return Some(lvl);
        }
        let s = styles.get(&id)?;
        if let Some(lvl) = s.outline_level {
            return (lvl < 9).then_some(lvl + 1);
        }
        if let Some(lvl) = s.name.as_deref().and_then(heading_from_name) {
            return Some(lvl);
        }
        cur = s.based_on.clone();
    }
    None
}

fn heading_from_name(name: &str) -> Option<u8> {
    let n = name.to_ascii_lowercase().replace(' ', "");
    n.strip_prefix("heading")
        .and_then(|d| d.parse::<u8>().ok())
        .filter(|l| (1..=9).contains(l))
}

#[derive(Default)]
struct RunState {
    bold: bool,
    color: Option<String>,
    shading: Option<String>,
    highlight: Option<String>,
    size_pt: Option<f32>,
    text: String,
}

fn parse_document(
    xml: &str,
    styles: &HashMap<String, StyleInfo>,
) -> std::result::Result<DocxDocument, String> {
    let mut doc = DocxDocument::default();
    let mut reader = Reader::from_str(xml);

    let mut table_stack: Vec<(DocxTable, Vec<String>, String)> = Vec::new(); // (table, current row, current cell text)
    let mut row_index_in_table: Vec<usize> = Vec::new();
    let mut para: Option<Paragraph> = None;
    let mut para_runs_total = 0usize;
    let mut para_runs_bold = 0usize;
    let mut para_shading: Option<String> = None;
    let mut cell_shading: Option<String> = None;
    let mut run: Option<RunState> = None;
    let mut in_rpr = false;
    let mut in_ppr = false;
    let mut in_text = false;
    let mut in_trpr = false;
    let mut hyperlink_depth = 0usize;
    let mut hyperlink_text = String::new();
    let mut drawing: Option<Drawing> = None;

    loop {
        let ev = reader
            .read_event()
            .map_err(|e| format!("document.xml: {e}"))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(ev, Event::Empty(_));
                let name = local(e);
                match name.as_str() {
                    "tbl" => {
                        table_stack.push((DocxTable::default(), Vec::new(), String::new()));
                        row_index_in_table.push(0);
                    }
                    "tr" => {
                        if let Some((_, row, _)) = table_stack.last_mut() {
                            row.clear();
                        }
                    }
                    "trPr" => in_trpr = !is_empty,
                    "tblHeader" if in_trpr => {
                        if let (Some((t, _, _)), Some(&ri)) =
                            (table_stack.last_mut(), row_index_in_table.last())
                            && ri == 0
                            && toggle_on(e)
                        {
                            t.header_row_marked = true;
                        }
                    }
                    "gridSpan" | "vMerge" => {
                        if let Some((t, _, _)) = table_stack.last_mut() {
                            if name == "gridSpan" {
                                if attr(e, "val")
                                    .and_then(|v| v.parse::<u32>().ok())
                                    .unwrap_or(1)
                                    > 1
                                {
                                    t.merged_cells += 1;
                                }
                            } else {
                                t.merged_cells += 1;
                            }
                        }
                    }
                    "tc" => {
                        cell_shading = None;
                        if let Some((_, _, cell)) = table_stack.last_mut() {
                            cell.clear();
                        }
                    }
                    "p" => {
                        para = Some(Paragraph {
                            in_table: !table_stack.is_empty(),
                            ..Paragraph::default()
                        });
                        para_runs_total = 0;
                        para_runs_bold = 0;
                        para_shading = None;
                        if is_empty && let Some(p) = para.take() {
                            doc.blocks.push(Block::Paragraph(p));
                        }
                    }
                    "pPr" => in_ppr = !is_empty,
                    "pStyle" if in_ppr => {
                        if let Some(p) = para.as_mut() {
                            let id = attr(e, "val");
                            if let Some(id) = &id {
                                p.heading_level = style_heading_level(styles, id);
                                p.style_name = styles.get(id).and_then(|s| s.name.clone());
                            }
                            p.style_id = id;
                        }
                    }
                    "outlineLvl" if in_ppr => {
                        if let Some(p) = para.as_mut()
                            && let Some(l) = attr(e, "val").and_then(|v| v.parse::<u8>().ok())
                            && l < 9
                        {
                            p.heading_level = Some(l + 1);
                        }
                    }
                    "numPr" if in_ppr => {
                        if let Some(p) = para.as_mut() {
                            p.is_list = true;
                        }
                    }
                    "shd" => {
                        let fill = attr(e, "fill").filter(|f| f != "auto" && f.len() == 6);
                        if in_rpr {
                            if let Some(r) = run.as_mut() {
                                r.shading = fill;
                            }
                        } else if in_ppr {
                            para_shading = fill;
                        } else if !table_stack.is_empty() {
                            cell_shading = fill;
                        }
                    }
                    "r" => {
                        let style_bold = para
                            .as_ref()
                            .and_then(|p| p.style_id.as_ref())
                            .and_then(|id| styles.get(id))
                            .is_some_and(|s| s.bold);
                        run = Some(RunState {
                            bold: style_bold,
                            ..RunState::default()
                        });
                    }
                    "rPr" => in_rpr = !is_empty,
                    "b" if in_rpr => {
                        if let Some(r) = run.as_mut() {
                            r.bold = toggle_on(e);
                        }
                    }
                    "color" if in_rpr => {
                        if let Some(r) = run.as_mut() {
                            r.color = attr(e, "val").filter(|v| v != "auto" && v.len() == 6);
                        }
                    }
                    "highlight" if in_rpr => {
                        if let Some(r) = run.as_mut() {
                            r.highlight = attr(e, "val").and_then(|v| highlight_hex(&v));
                        }
                    }
                    "sz" if in_rpr => {
                        if let Some(r) = run.as_mut() {
                            r.size_pt = attr(e, "val")
                                .and_then(|v| v.parse::<f32>().ok())
                                .map(|half| half / 2.0);
                        }
                    }
                    "t" => in_text = !is_empty,
                    "tab" if run.is_some() => {
                        if let Some(r) = run.as_mut() {
                            r.text.push('\t');
                        }
                    }
                    "br" | "cr" if run.is_some() => {
                        if let Some(r) = run.as_mut() {
                            r.text.push('\n');
                        }
                    }
                    "hyperlink" => {
                        if !is_empty {
                            hyperlink_depth += 1;
                            hyperlink_text.clear();
                        }
                    }
                    "drawing" | "pict" => {
                        drawing = Some(Drawing::default());
                    }
                    "docPr" | "cNvPr" => {
                        if let Some(d) = drawing.as_mut()
                            && d.name.is_none()
                        {
                            d.name = attr(e, "name");
                            d.alt_text = attr(e, "descr").filter(|s| !s.trim().is_empty());
                            d.title = attr(e, "title").filter(|s| !s.trim().is_empty());
                        }
                    }
                    "decorative" => {
                        if let Some(d) = drawing.as_mut() {
                            d.decorative = attr(e, "val").is_none_or(|v| v == "1" || v == "true");
                        }
                    }
                    "imagedata" => {
                        // VML image: alt text lives on the parent shape (o:title).
                        if let Some(d) = drawing.as_mut()
                            && d.alt_text.is_none()
                        {
                            d.alt_text = attr(e, "title").filter(|s| !s.trim().is_empty());
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_text => {
                if let Some(r) = run.as_mut() {
                    r.text.push_str(&t.xml10_content());
                }
            }
            Event::GeneralRef(r) if in_text => {
                if let Some(run) = run.as_mut() {
                    run.text.push_str(&resolve_ref(&r));
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_string();
                match name.as_str() {
                    "t" => in_text = false,
                    "rPr" => in_rpr = false,
                    "pPr" => in_ppr = false,
                    "trPr" => in_trpr = false,
                    "r" => {
                        if let Some(r) = run.take()
                            && let Some(p) = para.as_mut()
                        {
                            if !r.text.trim().is_empty() {
                                para_runs_total += 1;
                                if r.bold {
                                    para_runs_bold += 1;
                                }
                                if let Some(sz) = r.size_pt {
                                    p.max_font_pt = Some(p.max_font_pt.map_or(sz, |m| m.max(sz)));
                                }
                                if let Some(color) = &r.color {
                                    doc.colored_runs.push(ColoredRun {
                                        text: r.text.trim().chars().take(80).collect(),
                                        color: color.to_ascii_uppercase(),
                                        background: r
                                            .shading
                                            .clone()
                                            .or(r.highlight.clone())
                                            .or(para_shading.clone())
                                            .or(cell_shading.clone())
                                            .map(|b| b.to_ascii_uppercase()),
                                        bold: r.bold,
                                        size_pt: r.size_pt,
                                    });
                                }
                            }
                            p.text.push_str(&r.text);
                            if hyperlink_depth > 0 {
                                hyperlink_text.push_str(&r.text);
                            }
                        }
                    }
                    "hyperlink" => {
                        if hyperlink_depth > 0 {
                            hyperlink_depth -= 1;
                            let t = hyperlink_text.trim().to_string();
                            if !t.is_empty() {
                                doc.hyperlink_texts.push(t);
                            }
                            hyperlink_text.clear();
                        }
                    }
                    "drawing" | "pict" => {
                        if let Some(d) = drawing.take() {
                            doc.drawings.push(d);
                        }
                    }
                    "p" => {
                        if let Some(mut p) = para.take() {
                            p.all_bold = para_runs_total > 0 && para_runs_bold == para_runs_total;
                            if let Some((_, _, cell)) = table_stack.last_mut() {
                                if !cell.is_empty() {
                                    cell.push('\n');
                                }
                                cell.push_str(p.text.trim());
                            }
                            doc.blocks.push(Block::Paragraph(p));
                        }
                    }
                    "tc" => {
                        if let Some((_, row, cell)) = table_stack.last_mut() {
                            row.push(std::mem::take(cell));
                        }
                    }
                    "tr" => {
                        if let Some((t, row, _)) = table_stack.last_mut() {
                            t.rows.push(std::mem::take(row));
                        }
                        if let Some(ri) = row_index_in_table.last_mut() {
                            *ri += 1;
                        }
                    }
                    "tbl" => {
                        row_index_in_table.pop();
                        if let Some((t, _, _)) = table_stack.pop() {
                            // A nested table's text joins the enclosing cell.
                            if let Some((_, _, cell)) = table_stack.last_mut() {
                                for r in &t.rows {
                                    cell.push_str(&r.join(" "));
                                    cell.push('\n');
                                }
                            } else {
                                doc.blocks.push(Block::Table(t));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(doc)
}

/// Word highlight color names to hex.
fn highlight_hex(name: &str) -> Option<String> {
    let hex = match name {
        "yellow" => "FFFF00",
        "green" => "00FF00",
        "cyan" => "00FFFF",
        "magenta" => "FF00FF",
        "blue" => "0000FF",
        "red" => "FF0000",
        "darkBlue" => "000080",
        "darkCyan" => "008080",
        "darkGreen" => "008000",
        "darkMagenta" => "800080",
        "darkRed" => "800000",
        "darkYellow" => "808000",
        "darkGray" => "808080",
        "lightGray" => "C0C0C0",
        "black" => "000000",
        "white" => "FFFFFF",
        _ => return None,
    };
    Some(hex.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal DOCX from document and styles XML bodies.
    pub fn make_docx(body: &str, title: Option<&str>) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("word/document.xml", opts).unwrap();
            write!(
                z,
                r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><w:body>{body}</w:body></w:document>"#
            )
            .unwrap();
            z.start_file("word/styles.xml", opts).unwrap();
            write!(
                z,
                r#"<?xml version="1.0"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:lang w:val="en-US"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style><w:style w:styleId="Custom2"><w:name w:val="My Section"/><w:basedOn w:val="Heading2"/></w:style></w:styles>"#
            )
            .unwrap();
            if let Some(t) = title {
                z.start_file("docProps/core.xml", opts).unwrap();
                write!(z, r#"<cp:coreProperties xmlns:cp="x" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{t}</dc:title></cp:coreProperties>"#).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn parses_headings_tables_images_and_colors() {
        let body = r#"
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Policy &amp; Procedure</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Custom2"/></w:pPr><w:r><w:t>Scope</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Fake heading</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:color w:val="AAAAAA"/></w:rPr><w:t xml:space="preserve">Light text </w:t></w:r><w:hyperlink><w:r><w:t>click here</w:t></w:r></w:hyperlink></w:p>
<w:tbl><w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:p><w:r><w:t>Town</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cases</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Amherst</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>3</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing><wp:inline><wp:docPr id="1" name="Picture 1" descr="Map of cases"/></wp:inline></w:drawing></w:r></w:p>
<w:p><w:r><w:drawing><wp:inline><wp:docPr id="2" name="Picture 2"/></wp:inline></w:drawing></w:r></w:p>"#;
        let doc = parse_docx(&make_docx(body, Some("Exposure Policy"))).unwrap();
        assert_eq!(doc.title.as_deref(), Some("Exposure Policy"));
        assert_eq!(doc.language.as_deref(), Some("en-US"));
        let paras: Vec<_> = doc.paragraphs().collect();
        assert_eq!(paras[0].text, "Policy & Procedure");
        assert_eq!(paras[0].heading_level, Some(1));
        assert_eq!(paras[1].heading_level, Some(2)); // via basedOn Heading2 name
        assert!(paras[2].all_bold && paras[2].heading_level.is_none());
        let t = doc.tables().next().unwrap();
        assert!(t.header_row_marked);
        assert_eq!(t.rows, vec![vec!["Town", "Cases"], vec!["Amherst", "3"]]);
        assert_eq!(doc.drawings.len(), 2);
        assert_eq!(doc.drawings[0].alt_text.as_deref(), Some("Map of cases"));
        assert!(doc.drawings[1].alt_text.is_none());
        assert_eq!(doc.colored_runs[0].color, "AAAAAA");
        assert_eq!(doc.hyperlink_texts, vec!["click here"]);
        assert!(doc.plain_text().contains("# Policy & Procedure"));
    }

    #[test]
    fn rejects_non_docx() {
        assert!(parse_docx(b"not a zip").is_err());
    }
}
