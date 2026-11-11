//! Tagged PDF writer.
//!
//! Produces PDF 1.7 with: embedded, subset DejaVu fonts (Type0/CIDFontType2
//! with ToUnicode maps, so text is searchable and readable by assistive
//! technology); a structure tree (H1–H6, P, L/LI/Lbl/LBody, Table/TR/TH/TD
//! with column scope, Caption, Figure with alt text) and parent tree; the
//! document language; the title shown in the window title bar; and the
//! letterhead, table rules, and page numbers marked as artifacts.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat, dictionary};
use phn_core::{PhnError, Result};
use subsetter::GlyphRemapper;

use crate::fonts;
use crate::image::{self, ImageFormat};
use crate::model::{Block, Document, Inline, TableBlock};

const PAGE_W: f32 = 612.0;
const PAGE_H: f32 = 792.0;
const MARGIN: f32 = 72.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
const BODY: f32 = 11.0;
const LEADING: f32 = 1.35;

type Rgb = (f32, f32, f32);
const BLACK: Rgb = (0.0, 0.0, 0.0);
const NAVY: Rgb = (0.122, 0.220, 0.392); // #1F3864
const LINK: Rgb = (0.043, 0.325, 0.580); // #0B5394
const GREY: Rgb = (0.25, 0.25, 0.25);

struct FontRes {
    bold: bool,
    remap: GlyphRemapper,
    /// new gid -> (old gid, char)
    used: BTreeMap<u16, (u16, char)>,
}

impl FontRes {
    fn new(bold: bool) -> Self {
        Self {
            bold,
            remap: GlyphRemapper::new(),
            used: BTreeMap::new(),
        }
    }

    fn encode(&mut self, s: &str) -> String {
        let face = fonts::face(self.bold);
        let mut hex = String::with_capacity(s.len() * 4);
        for c in s.chars() {
            // A character the font lacks is drawn as "?" and is recorded as
            // "?" too, so a real question mark never reads as another character.
            let (gid, shown) = match face.glyph(c) {
                Some(g) => (g, c),
                None => (face.glyph('?').unwrap_or(0), '?'),
            };
            let new = self.remap.remap(gid);
            self.used.entry(new).or_insert((gid, shown));
            hex.push_str(&format!("{new:04X}"));
        }
        hex
    }
}

enum Kid {
    Elem(usize),
    Mcr { page: usize, mcid: u32 },
}

struct SElem {
    tag: &'static str,
    parent: Option<usize>,
    kids: Vec<Kid>,
    alt: Option<String>,
    column_scope: bool,
}

struct Page {
    ops: String,
    /// Struct element owning each MCID on this page, by MCID.
    mcid_owner: Vec<usize>,
}

struct ImageObj {
    name: String,
    id: ObjectId,
}

struct Writer<'a> {
    doc: &'a Document,
    pdf: lopdf::Document,
    pages: Vec<Page>,
    elems: Vec<SElem>,
    fonts: [FontRes; 2],
    y: f32,
    top: f32,
    images: Vec<(usize, ImageObj)>, // (page, image)
    logo: Option<(ObjectId, f32, f32)>,
}

fn rgb_op((r, g, b): Rgb) -> String {
    format!("{r:.3} {g:.3} {b:.3} rg ")
}

impl<'a> Writer<'a> {
    fn new(doc: &'a Document) -> Self {
        let mut w = Self {
            doc,
            pdf: lopdf::Document::with_version("1.7"),
            pages: Vec::new(),
            elems: Vec::new(),
            fonts: [FontRes::new(false), FontRes::new(true)],
            y: 0.0,
            top: PAGE_H - MARGIN,
            images: Vec::new(),
            logo: None,
        };
        w.elems.push(SElem {
            tag: "Document",
            parent: None,
            kids: Vec::new(),
            alt: None,
            column_scope: false,
        });
        if let Some(lh) = &doc.letterhead {
            let lines = crate::docx::letterhead_lines(lh).len() as f32;
            let text_h = 16.0 + (lines - 1.0) * 11.0;
            let mut h = text_h;
            if let Some(bytes) = &doc.logo
                && let Some((id, iw, ih)) = w.add_image(bytes)
            {
                let scale = (44.0 / ih).min(140.0 / iw);
                w.logo = Some((id, iw * scale, ih * scale));
                h = h.max(ih * scale);
            }
            w.top = PAGE_H - 48.0 - h - 18.0;
        }
        w
    }

    fn elem(&mut self, tag: &'static str, parent: usize) -> usize {
        self.elems.push(SElem {
            tag,
            parent: Some(parent),
            kids: Vec::new(),
            alt: None,
            column_scope: false,
        });
        let id = self.elems.len() - 1;
        self.elems[parent].kids.push(Kid::Elem(id));
        id
    }

    fn page(&mut self) -> &mut Page {
        self.pages.last_mut().expect("a page exists")
    }

    fn new_page(&mut self) {
        self.pages.push(Page {
            ops: String::new(),
            mcid_owner: Vec::new(),
        });
        self.y = self.top;
        self.draw_letterhead();
    }

    fn ensure(&mut self, h: f32) {
        if self.pages.is_empty() || self.y - h < MARGIN {
            self.new_page();
        }
    }

    /// Begin marked content owned by struct element `owner`.
    fn begin_mc(&mut self, owner: usize) {
        let page_idx = self.pages.len() - 1;
        let tag = self.elems[owner].tag;
        let page = self.page();
        let mcid = page.mcid_owner.len() as u32;
        page.mcid_owner.push(owner);
        page.ops.push_str(&format!("/{tag} <</MCID {mcid}>> BDC\n"));
        self.elems[owner].kids.push(Kid::Mcr {
            page: page_idx,
            mcid,
        });
    }

    fn end_mc(&mut self) {
        self.page().ops.push_str("EMC\n");
    }

    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Rgb, s: &str) {
        if s.is_empty() {
            return;
        }
        let hex = self.fonts[usize::from(bold)].encode(s);
        let font = if bold { "F2" } else { "F1" };
        let op = format!(
            "BT /{font} {size} Tf {}1 0 0 1 {x:.2} {y:.2} Tm <{hex}> Tj ET\n",
            rgb_op(color)
        );
        self.page().ops.push_str(&op);
    }

    fn draw_letterhead(&mut self) {
        let Some(lh) = self.doc.letterhead.clone() else {
            return;
        };
        self.page()
            .ops
            .push_str("/Artifact <</Type /Pagination /Subtype /Header>> BDC\n");
        let mut x = MARGIN;
        let top = PAGE_H - 48.0;
        if let Some((_, w, h)) = self.logo {
            let op = format!(
                "q {w:.2} 0 0 {h:.2} {MARGIN:.2} {:.2} cm /Logo Do Q\n",
                top - h
            );
            self.page().ops.push_str(&op);
            x += w + 12.0;
        }
        let mut y = top - 13.0;
        for (i, (line, bold)) in crate::docx::letterhead_lines(&lh).into_iter().enumerate() {
            let size = if i == 0 { 13.0 } else { 9.0 };
            self.text(x, y, size, bold, if i == 0 { NAVY } else { GREY }, &line);
            y -= if i == 0 { 15.0 } else { 11.0 };
        }
        let rule_y = self.top + 10.0;
        let op = format!(
            "{} 0.75 w {MARGIN} {rule_y:.2} m {:.2} {rule_y:.2} l S\n",
            "0.122 0.220 0.392 RG",
            PAGE_W - MARGIN
        );
        self.page().ops.push_str(&op);
        self.page().ops.push_str("EMC\n");
    }

    /// Lay out inline runs with wrapping. Returns lines of (text, bold, color).
    fn layout_runs(
        runs: &[Inline],
        size: f32,
        width: f32,
        force_bold: bool,
    ) -> Vec<Vec<(String, bool, Rgb)>> {
        // Split runs into words that keep their style; append URLs after link text.
        let mut words: Vec<(String, bool, Rgb, bool)> = Vec::new(); // (word, bold, color, space_before)
        // Markdown leaves the space between "are " and "**rising**" at the end
        // of the first run, so the end of each run is remembered.
        let mut prev_ended_with_space = false;
        for r in runs {
            let bold = r.bold || force_bold;
            let color = if r.link.is_some() { LINK } else { BLACK };
            let mut text = r.text.clone();
            if let Some(url) = &r.link
                && url.trim() != r.text.trim()
                && !url.starts_with('#')
            {
                text.push_str(&format!(" ({url})"));
            }
            let starts_with_space = text.starts_with(char::is_whitespace);
            for (i, w) in text.split_whitespace().enumerate() {
                let space = i > 0 || starts_with_space || prev_ended_with_space || words.is_empty();
                words.push((w.to_string(), bold, color, space && !words.is_empty()));
            }
            if !text.is_empty() {
                prev_ended_with_space = text.ends_with(char::is_whitespace);
            }
        }
        let mut lines: Vec<Vec<(String, bool, Rgb)>> = vec![Vec::new()];
        let mut line_w = 0.0;
        for (word, bold, color, space) in words {
            let sep = if space && line_w > 0.0 { " " } else { "" };
            let piece = format!("{sep}{word}");
            let w = fonts::text_width(&piece, bold, size);
            // Also taken for a first word wider than the line (a long web
            // address), which is broken across lines instead of running off the page.
            if line_w + w > width {
                if lines.last().is_some_and(|l| !l.is_empty()) {
                    lines.push(Vec::new());
                }
                line_w = 0.0;
                let wrapped = fonts::wrap(&word, bold, size, width);
                let n = wrapped.len();
                for (i, part) in wrapped.into_iter().enumerate() {
                    line_w = fonts::text_width(&part, bold, size);
                    lines.last_mut().expect("line").push((part, bold, color));
                    if i + 1 < n {
                        lines.push(Vec::new());
                    }
                }
                continue;
            }
            line_w += w;
            let line = lines.last_mut().expect("line");
            match line.last_mut() {
                Some(last) if last.1 == bold && last.2 == color => last.0.push_str(&piece),
                _ => line.push((piece, bold, color)),
            }
        }
        lines.retain(|l| !l.is_empty());
        lines
    }

    fn draw_lines(&mut self, owner: usize, lines: &[Vec<(String, bool, Rgb)>], x: f32, size: f32) {
        let lh = size * LEADING;
        for line in lines {
            self.ensure(lh);
            self.y -= lh;
            self.begin_mc(owner);
            let mut cx = x;
            for (s, bold, color) in line {
                self.text(cx, self.y + size * 0.25, size, *bold, *color, s);
                cx += fonts::text_width(s, *bold, size);
            }
            self.end_mc();
        }
    }

    fn heading(&mut self, level: u8, text: &str) {
        let (tag, size) = match level {
            1 => ("H1", 16.0),
            2 => ("H2", 13.5),
            3 => ("H3", 12.0),
            4 => ("H4", 11.5),
            5 => ("H5", 11.0),
            _ => ("H6", 11.0),
        };
        let lines = Self::layout_runs(&[Inline::plain(text)], size, CONTENT_W, true);
        // Keep with next: require room for the heading plus two body lines.
        self.ensure(size * LEADING * lines.len() as f32 + 10.0 + BODY * LEADING * 2.0);
        self.y -= 10.0;
        let e = self.elem(tag, 0);
        let colored: Vec<Vec<(String, bool, Rgb)>> = lines
            .into_iter()
            .map(|l| l.into_iter().map(|(s, b, _)| (s, b, NAVY)).collect())
            .collect();
        self.draw_lines(e, &colored, MARGIN, size);
        self.y -= 4.0;
    }

    fn paragraph(&mut self, runs: &[Inline]) {
        let lines = Self::layout_runs(runs, BODY, CONTENT_W, false);
        let e = self.elem("P", 0);
        self.draw_lines(e, &lines, MARGIN, BODY);
        self.y -= 6.0;
    }

    fn list(&mut self, ordered: bool, items: &[Vec<Inline>]) {
        let l = self.elem("L", 0);
        let indent = 18.0;
        for (i, item) in items.iter().enumerate() {
            let li = self.elem("LI", l);
            let label = if ordered {
                format!("{}.", i + 1)
            } else {
                "•".to_string()
            };
            let lines = Self::layout_runs(item, BODY, CONTENT_W - indent, false);
            let lh = BODY * LEADING;
            self.ensure(lh);
            let lbl = self.elem("Lbl", li);
            let body = self.elem("LBody", li);
            // Label on the first line.
            let y_label = self.y - lh + BODY * 0.25;
            self.begin_mc(lbl);
            self.text(MARGIN + 4.0, y_label, BODY, false, BLACK, &label);
            self.end_mc();
            self.draw_lines(body, &lines, MARGIN + indent, BODY);
            self.y -= 2.0;
        }
        self.y -= 4.0;
    }

    fn table(&mut self, t: &TableBlock, number: usize) {
        let cols = t
            .headers
            .len()
            .max(t.rows.iter().map(Vec::len).max().unwrap_or(0))
            .max(1);
        let size = 9.5;
        let pad = 4.0;
        // Column widths proportional to content (capped), filling the line.
        let mut desired = vec![30.0f32; cols];
        for c in 0..cols {
            let header_w = fonts::text_width(
                t.headers.get(c).map(String::as_str).unwrap_or(""),
                true,
                size,
            );
            let body_w = t
                .rows
                .iter()
                .map(|r| fonts::text_width(r.get(c).map(String::as_str).unwrap_or(""), false, size))
                .fold(0.0, f32::max);
            desired[c] = header_w.max(body_w).clamp(30.0, 220.0) + 2.0 * pad;
        }
        let total: f32 = desired.iter().sum();
        let widths: Vec<f32> = desired.iter().map(|d| d * CONTENT_W / total).collect();

        let table = self.elem("Table", 0);
        let caption_text = format!(
            "Table {number}. {}",
            t.caption
                .clone()
                .unwrap_or_else(|| format!("Table {number}"))
        );
        let lines = Self::layout_runs(&[Inline::plain(&caption_text)], 10.0, CONTENT_W, true);
        self.ensure(10.0 * LEADING * 3.0 + 30.0);
        self.y -= 6.0;
        let cap = self.elem("Caption", table);
        let grey_lines: Vec<_> = lines
            .into_iter()
            .map(|l| l.into_iter().map(|(s, b, _)| (s, b, GREY)).collect())
            .collect();
        self.draw_lines(cap, &grey_lines, MARGIN, 10.0);
        self.y -= 2.0;

        let cell_lines =
            |text: &str, w: f32, bold: bool| fonts::wrap(text, bold, size, w - 2.0 * pad);
        let header_cells: Vec<Vec<String>> = (0..cols)
            .map(|c| {
                cell_lines(
                    t.headers.get(c).map(String::as_str).unwrap_or(""),
                    widths[c],
                    true,
                )
            })
            .collect();
        let has_header = !t.headers.is_empty();
        let row_h = |cells: &Vec<Vec<String>>| {
            cells.iter().map(Vec::len).max().unwrap_or(1) as f32 * size * 1.3 + 2.0 * pad
        };
        let header_h = row_h(&header_cells);

        let draw_row =
            |w: &mut Writer, cells: &Vec<Vec<String>>, header: bool, tag_row: Option<usize>| {
                let h = row_h(cells);
                let y_top = w.y;
                // Background and borders (artifact).
                let mut art = String::from("/Artifact BMC\n");
                let mut x = MARGIN;
                for &cw in &widths {
                    if header {
                        art.push_str(&format!(
                            "0.851 0.886 0.953 rg {x:.2} {:.2} {cw:.2} {h:.2} re f\n",
                            y_top - h
                        ));
                    }
                    art.push_str(&format!(
                        "0.5 0.5 0.5 RG 0.5 w {x:.2} {:.2} {cw:.2} {h:.2} re S\n",
                        y_top - h
                    ));
                    x += cw;
                }
                art.push_str("EMC\n");
                w.page().ops.push_str(&art);
                let mut x = MARGIN;
                for (c, lines) in cells.iter().enumerate() {
                    match tag_row {
                        Some(tr) => {
                            let cell = w.elem(if header { "TH" } else { "TD" }, tr);
                            if header {
                                w.elems[cell].column_scope = true;
                            }
                            w.begin_mc(cell);
                        }
                        None => w
                            .page()
                            .ops
                            .push_str("/Artifact <</Type /Pagination>> BDC\n"),
                    }
                    let mut ly = y_top - pad - size;
                    for line in lines {
                        w.text(x + pad, ly + 1.5, size, header, BLACK, line);
                        ly -= size * 1.3;
                    }
                    if tag_row.is_some() {
                        w.end_mc();
                    } else {
                        w.page().ops.push_str("EMC\n");
                    }
                    x += widths[c];
                }
                w.y -= h;
            };

        if has_header {
            self.ensure(header_h + 20.0);
            let tr = self.elem("TR", table);
            draw_row(self, &header_cells, true, Some(tr));
        }
        for r in &t.rows {
            let cells: Vec<Vec<String>> = (0..cols)
                .map(|c| cell_lines(r.get(c).map(String::as_str).unwrap_or(""), widths[c], false))
                .collect();
            let h = row_h(&cells);
            if self.y - h < MARGIN {
                self.new_page();
                if has_header {
                    // Repeated header rows are pagination artifacts.
                    draw_row(self, &header_cells, true, None);
                }
            }
            let tr = self.elem("TR", table);
            draw_row(self, &cells, false, Some(tr));
        }
        self.y -= 12.0;
    }

    /// Add an image XObject. Returns (object id, width pt, height pt).
    fn add_image(&mut self, bytes: &[u8]) -> Option<(ObjectId, f32, f32)> {
        let info = image::inspect(bytes)?;
        let (w, h) = (info.width as i64, info.height as i64);
        let id = match info.format {
            ImageFormat::Jpeg => {
                let cs = match info.components {
                    1 => "DeviceGray",
                    4 => "DeviceCMYK",
                    _ => "DeviceRGB",
                };
                let stream = Stream::new(
                    dictionary! {
                        "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h,
                        "ColorSpace" => cs, "BitsPerComponent" => 8, "Filter" => "DCTDecode",
                    },
                    bytes.to_vec(),
                );
                self.pdf.add_object(stream)
            }
            ImageFormat::Png => {
                let pm = resvg::tiny_skia::Pixmap::decode_png(bytes).ok()?;
                let mut rgb = Vec::with_capacity((w * h * 3) as usize);
                let mut alpha = Vec::with_capacity((w * h) as usize);
                let mut has_alpha = false;
                for p in pm.pixels() {
                    let c = p.demultiply();
                    rgb.extend_from_slice(&[c.red(), c.green(), c.blue()]);
                    alpha.push(c.alpha());
                    has_alpha |= c.alpha() != 255;
                }
                let mut dict = dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h,
                    "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
                };
                if has_alpha {
                    let mut smask = Stream::new(
                        dictionary! {
                            "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h,
                            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
                        },
                        alpha,
                    );
                    let _ = smask.compress();
                    let smask_id = self.pdf.add_object(smask);
                    dict.set("SMask", smask_id);
                }
                let mut stream = Stream::new(dict, rgb);
                let _ = stream.compress();
                self.pdf.add_object(stream)
            }
        };
        // 96 px per inch -> points.
        Some((id, info.width as f32 * 0.75, info.height as f32 * 0.75))
    }

    fn figure(&mut self, data: &[u8], alt: &str) {
        let Some((id, w, h)) = self.add_image(data) else {
            return;
        };
        let scale = (CONTENT_W / w).min(360.0 / h).min(1.0);
        let (w, h) = (w * scale, h * scale);
        self.ensure(h + 24.0);
        self.y -= h + 6.0;
        let fig = self.elem("Figure", 0);
        self.elems[fig].alt = Some(alt.to_string());
        let name = format!("Im{}", self.images.len() + 1);
        self.begin_mc(fig);
        let op = format!(
            "q {w:.2} 0 0 {h:.2} {MARGIN:.2} {:.2} cm /{name} Do Q\n",
            self.y
        );
        self.page().ops.push_str(&op);
        self.end_mc();
        let page = self.pages.len() - 1;
        self.images.push((page, ImageObj { name, id }));
        if !alt.is_empty() {
            let cap = self.elem("Caption", fig);
            let lines = Self::layout_runs(&[Inline::plain(alt)], 9.5, CONTENT_W, false);
            let grey: Vec<_> = lines
                .into_iter()
                .map(|l| l.into_iter().map(|(s, b, _)| (s, b, GREY)).collect())
                .collect();
            self.draw_lines(cap, &grey, MARGIN, 9.5);
        }
        self.y -= 8.0;
    }

    fn build(mut self) -> Result<Vec<u8>> {
        self.new_page();
        // Title and subtitle.
        let title = self.doc.title.clone();
        let lines = Self::layout_runs(&[Inline::plain(&title)], 20.0, CONTENT_W, true);
        let t = self.elem("H1", 0);
        let navy: Vec<_> = lines
            .into_iter()
            .map(|l| l.into_iter().map(|(s, b, _)| (s, b, NAVY)).collect())
            .collect();
        self.draw_lines(t, &navy, MARGIN, 20.0);
        if let Some(sub) = self.doc.subtitle.clone() {
            let lines = Self::layout_runs(&[Inline::plain(&sub)], 13.0, CONTENT_W, false);
            let p = self.elem("P", 0);
            let grey: Vec<_> = lines
                .into_iter()
                .map(|l| l.into_iter().map(|(s, b, _)| (s, b, GREY)).collect())
                .collect();
            self.draw_lines(p, &grey, MARGIN, 13.0);
        }
        self.y -= 10.0;

        let blocks = self.doc.blocks.clone();
        let mut table_no = 0;
        for b in &blocks {
            match b {
                Block::Heading { level, text } => self.heading(*level, text),
                Block::Paragraph(r) => self.paragraph(r),
                Block::List { ordered, items } => self.list(*ordered, items),
                Block::Table(t) => {
                    table_no += 1;
                    self.table(t, table_no);
                }
                Block::Image(img) => {
                    if let Some(d) = &img.data {
                        self.figure(d, &img.alt);
                    }
                }
                Block::PageBreak => self.new_page(),
            }
        }

        // Footers (artifacts) now that the page count is known.
        let n = self.pages.len();
        let footer = self
            .doc
            .letterhead
            .as_ref()
            .and_then(|l| l.footer_text.clone());
        for i in 0..n {
            let label = format!("Page {} of {n}", i + 1);
            let lw = fonts::text_width(&label, false, 8.5);
            let mut ops = String::from("/Artifact <</Type /Pagination /Subtype /Footer>> BDC\n");
            let hex = self.fonts[0].encode(&label);
            ops.push_str(&format!(
                "BT /F1 8.5 Tf {}1 0 0 1 {:.2} 40 Tm <{hex}> Tj ET\n",
                rgb_op(GREY),
                PAGE_W - MARGIN - lw
            ));
            if let Some(f) = &footer {
                let hex = self.fonts[0].encode(f);
                ops.push_str(&format!(
                    "BT /F1 8.5 Tf {}1 0 0 1 {MARGIN:.2} 40 Tm <{hex}> Tj ET\n",
                    rgb_op(GREY)
                ));
            }
            ops.push_str("EMC\n");
            self.pages[i].ops.push_str(&ops);
        }
        self.finish()
    }

    fn font_objects(&mut self, idx: usize) -> Result<ObjectId> {
        let bold = self.fonts[idx].bold;
        let data = if bold { fonts::BOLD } else { fonts::REGULAR };
        let face = fonts::face(bold);
        let upem = face.units_per_em();
        let scale = |v: f32| (v * 1000.0 / upem).round() as i64;
        let m = &face.metrics;
        let subset = subsetter::subset(data, 0, &self.fonts[idx].remap)
            .map_err(|e| PhnError::Internal(format!("font subsetting failed: {e}")))?;
        let len1 = subset.len() as i64;
        let mut file = Stream::new(dictionary! { "Length1" => len1 }, subset);
        let _ = file.compress();
        let file_id = self.pdf.add_object(file);
        let base = if bold {
            "PHNAAB+DejaVuSans-Bold"
        } else {
            "PHNAAA+DejaVuSans"
        };
        let bbox = m.bounds.unwrap_or(skrifa::metrics::BoundingBox {
            x_min: 0.0,
            y_min: m.descent,
            x_max: upem,
            y_max: m.ascent,
        });
        let descriptor = self.pdf.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(base.as_bytes().to_vec()),
            "Flags" => 32,
            "FontBBox" => vec![scale(bbox.x_min).into(), scale(bbox.y_min).into(), scale(bbox.x_max).into(), scale(bbox.y_max).into()],
            "ItalicAngle" => 0,
            "Ascent" => scale(m.ascent),
            "Descent" => scale(m.descent),
            "CapHeight" => scale(m.cap_height.unwrap_or(m.ascent)),
            "StemV" => if bold { 120 } else { 80 },
            "FontFile2" => file_id,
        });
        // Widths by new glyph id.
        let mut w_array: Vec<Object> = Vec::new();
        for (&new, &(old, _)) in &self.fonts[idx].used {
            let adv = face.advance(old);
            w_array.push(i64::from(new).into());
            w_array.push(vec![Object::Integer((adv * 1000.0 / upem).round() as i64)].into());
        }
        let cid = self.pdf.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => Object::Name(base.as_bytes().to_vec()),
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"),
                "Supplement" => 0,
            },
            "FontDescriptor" => descriptor,
            "DW" => 600,
            "W" => w_array,
            "CIDToGIDMap" => "Identity",
        });
        // ToUnicode CMap.
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        let entries: Vec<(u16, char)> = self.fonts[idx]
            .used
            .iter()
            .map(|(&n, &(_, c))| (n, c))
            .collect();
        for chunk in entries.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for (gid, c) in chunk {
                let mut buf = [0u16; 2];
                let utf16: String = c
                    .encode_utf16(&mut buf)
                    .iter()
                    .map(|u| format!("{u:04X}"))
                    .collect();
                cmap.push_str(&format!("<{gid:04X}> <{utf16}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        let mut tu = Stream::new(Dictionary::new(), cmap.into_bytes());
        let _ = tu.compress();
        let tu_id = self.pdf.add_object(tu);
        Ok(self.pdf.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => Object::Name(base.as_bytes().to_vec()),
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![cid.into()],
            "ToUnicode" => tu_id,
        }))
    }

    fn finish(mut self) -> Result<Vec<u8>> {
        let f1 = self.font_objects(0)?;
        let f2 = self.font_objects(1)?;
        let pages_id = self.pdf.new_object_id();
        let struct_root_id = self.pdf.new_object_id();
        let elem_ids: Vec<ObjectId> = (0..self.elems.len())
            .map(|_| self.pdf.new_object_id())
            .collect();

        let mut page_ids = Vec::new();
        for (i, p) in self.pages.iter().enumerate() {
            let mut content = Stream::new(Dictionary::new(), p.ops.clone().into_bytes());
            let _ = content.compress();
            let content_id = self.pdf.add_object(content);
            let mut xobjects = Dictionary::new();
            for (page, img) in &self.images {
                if *page == i {
                    xobjects.set(img.name.as_bytes().to_vec(), img.id);
                }
            }
            if let Some((logo, _, _)) = self.logo {
                xobjects.set("Logo", logo);
            }
            let page_id = self.pdf.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => f1, "F2" => f2 },
                    "XObject" => xobjects,
                },
                "StructParents" => i as i64,
                "Tabs" => "S",
            });
            page_ids.push(page_id);
        }
        self.pdf.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().map(|&id| id.into()).collect::<Vec<Object>>(),
                "Count" => page_ids.len() as i64,
            }),
        );

        // Structure elements.
        for (i, e) in self.elems.iter().enumerate() {
            let kids: Vec<Object> = e
                .kids
                .iter()
                .map(|k| match k {
                    Kid::Elem(c) => Object::Reference(elem_ids[*c]),
                    Kid::Mcr { page, mcid } => Object::Dictionary(dictionary! {
                        "Type" => "MCR", "Pg" => page_ids[*page], "MCID" => i64::from(*mcid),
                    }),
                })
                .collect();
            let mut d = dictionary! {
                "Type" => "StructElem",
                "S" => Object::Name(e.tag.as_bytes().to_vec()),
                "P" => e.parent.map(|p| elem_ids[p]).unwrap_or(struct_root_id),
                "K" => kids,
            };
            if let Some(alt) = &e.alt {
                d.set("Alt", text_string(alt));
            }
            if e.column_scope {
                d.set("A", dictionary! { "O" => "Table", "Scope" => "Column" });
            }
            if i == 0 {
                d.set("Lang", text_string(&self.doc.language));
            }
            self.pdf.objects.insert(elem_ids[i], Object::Dictionary(d));
        }
        let mut nums: Vec<Object> = Vec::new();
        for (i, p) in self.pages.iter().enumerate() {
            nums.push((i as i64).into());
            nums.push(
                p.mcid_owner
                    .iter()
                    .map(|&e| Object::Reference(elem_ids[e]))
                    .collect::<Vec<Object>>()
                    .into(),
            );
        }
        let parent_tree = self.pdf.add_object(dictionary! { "Nums" => nums });
        self.pdf.objects.insert(
            struct_root_id,
            Object::Dictionary(dictionary! {
                "Type" => "StructTreeRoot",
                "K" => elem_ids[0],
                "ParentTree" => parent_tree,
                "ParentTreeNextKey" => self.pages.len() as i64,
            }),
        );

        let catalog = self.pdf.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "StructTreeRoot" => struct_root_id,
            "MarkInfo" => dictionary! { "Marked" => true },
            "Lang" => text_string(&self.doc.language),
            "ViewerPreferences" => dictionary! { "DisplayDocTitle" => true },
        });
        let now = chrono::Utc::now().format("D:%Y%m%d%H%M%SZ").to_string();
        let mut info = dictionary! {
            "Title" => text_string(&self.doc.title),
            "Producer" => text_string("Public Health Nursing MCP Server"),
            "CreationDate" => Object::string_literal(now),
        };
        if let Some(a) = self.doc.author.as_ref().or(self
            .doc
            .letterhead
            .as_ref()
            .map(|l| &l.agency_name))
        {
            info.set("Author", text_string(a));
        }
        let info_id = self.pdf.add_object(info);
        self.pdf.trailer.set("Root", catalog);
        self.pdf.trailer.set("Info", info_id);
        let mut out = Vec::new();
        self.pdf
            .save_to(&mut out)
            .map_err(|e| PhnError::Internal(format!("PDF write: {e}")))?;
        Ok(out)
    }
}

/// A PDF text string: ASCII literal, or UTF-16BE with BOM.
fn text_string(s: &str) -> Object {
    if s.is_ascii() {
        Object::string_literal(s)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            bytes.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

/// Render a document as tagged PDF bytes.
pub fn to_pdf(doc: &Document) -> Result<Vec<u8>> {
    Writer::new(doc).build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::from_markdown;
    use phn_core::Letterhead;

    fn doc() -> Document {
        let mut rows = String::new();
        for i in 0..80 {
            rows.push_str(&format!("| Week {i} | {} | Amherst & Pelham |\n", i * 3));
        }
        Document {
            title: "Informe de tos ferina — señales".into(),
            subtitle: Some("Para enfermeras escolares".into()),
            language: "es-US".into(),
            author: None,
            letterhead: Some(Letterhead {
                agency_name: "Town of Example Health Department".into(),
                phone: Some("413-555-0100".into()),
                footer_text: Some("Internal use".into()),
                ..Letterhead::default()
            }),
            logo: None,
            blocks: from_markdown(&format!(
                "# Resumen\n\nLos casos **aumentan**. Vea [la guía](https://www.mass.gov).\n\n- Excluir casos\n- Notificar\n\nTable: Casos por semana\n\n| Semana | Casos | Pueblos |\n|---|---|---|\n{rows}"
            )),
        }
    }

    #[test]
    fn tagged_pdf_round_trips_through_the_reader() {
        let bytes = to_pdf(&doc()).unwrap();
        let info = phn_local::pdf::parse_pdf(&bytes).unwrap();
        assert!(info.pages >= 2, "table should span pages");
        assert!(info.marked && info.has_struct_tree && info.display_doc_title);
        assert_eq!(info.language.as_deref(), Some("es-US"));
        assert_eq!(
            info.title.as_deref(),
            Some("Informe de tos ferina — señales")
        );
        assert!(
            info.unembedded_fonts.is_empty(),
            "{:?}",
            info.unembedded_fonts
        );
        assert!(info.pages_without_text.is_empty());
        let text = info.text();
        assert!(text.contains("señales"), "{}", &text[..200.min(text.len())]);
        assert!(text.contains("Casos por semana"));
        assert!(text.contains("Week 79"));
    }

    #[test]
    fn styled_runs_keep_the_space_before_them() {
        let runs = vec![
            Inline::plain("Cases are "),
            Inline {
                text: "rising".into(),
                bold: true,
                ..Inline::default()
            },
            Inline::plain(" & we need help. See "),
            Inline {
                text: "the guide".into(),
                link: Some("https://www.mass.gov".into()),
                ..Inline::default()
            },
            Inline::plain(" today."),
        ];
        // Wide enough that nothing wraps.
        let lines = Writer::layout_runs(&runs, BODY, 10_000.0, false);
        let text: String = lines.iter().flatten().map(|(s, _, _)| s.as_str()).collect();
        assert_eq!(
            text,
            "Cases are rising & we need help. See the guide (https://www.mass.gov) today."
        );
    }

    #[test]
    fn a_long_first_word_is_broken_across_lines() {
        let url = format!("https://www.mass.gov/{}", "info-details/".repeat(12));
        let lines = Writer::layout_runs(&[Inline::plain(url.clone())], BODY, CONTENT_W, false);
        assert!(lines.len() > 1);
        for l in &lines {
            let w: f32 = l
                .iter()
                .map(|(s, b, _)| fonts::text_width(s, *b, BODY))
                .sum();
            assert!(w <= CONTENT_W, "{w}");
        }
        let joined: String = lines.iter().flatten().map(|(s, _, _)| s.as_str()).collect();
        assert_eq!(joined, url);
    }

    #[test]
    fn missing_glyphs_are_recorded_as_question_marks() {
        let mut f = FontRes::new(false);
        let _ = f.encode("中?");
        let q = fonts::face(false).glyph('?').unwrap();
        let chars: Vec<char> = f
            .used
            .values()
            .filter(|(old, _)| *old == q)
            .map(|(_, c)| *c)
            .collect();
        assert_eq!(chars, vec!['?']);
    }

    #[test]
    fn figures_carry_alt_text() {
        let mut pm = resvg::tiny_skia::Pixmap::new(30, 30).unwrap();
        pm.fill(resvg::tiny_skia::Color::from_rgba8(10, 20, 30, 128));
        let mut d = doc();
        d.blocks.insert(
            0,
            Block::Image(crate::model::ImageBlock {
                path: "c.png".into(),
                alt: "Chart of weekly cases".into(),
                data: Some(pm.encode_png().unwrap()),
            }),
        );
        d.logo = Some(pm.encode_png().unwrap());
        let bytes = to_pdf(&d).unwrap();
        let info = phn_local::pdf::parse_pdf(&bytes).unwrap();
        assert_eq!(info.figures, 1);
        assert_eq!(info.figures_without_alt, 0);
    }
}
