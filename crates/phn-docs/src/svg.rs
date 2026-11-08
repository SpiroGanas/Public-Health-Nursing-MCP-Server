//! SVG and PNG writer: renders a document as a single image (useful for
//! slides, social media, or a quick visual of a table). Includes `<title>`
//! and `<desc>` so the image has an accessible name and description.

use phn_core::{PhnError, Result};

use crate::docx::esc;
use crate::fonts;
use crate::model::{Block, Document};

const WIDTH: f32 = 800.0;
const PAD: f32 = 32.0;

/// Render the document as SVG text.
pub fn to_svg(doc: &Document) -> String {
    let inner = WIDTH - 2.0 * PAD;
    let mut y = PAD;
    let mut body = String::new();
    let mut desc_parts: Vec<String> = Vec::new();
    let text = |body: &mut String,
                y: &mut f32,
                s: &str,
                size: f32,
                bold: bool,
                color: &str,
                x: f32,
                width: f32| {
        for line in fonts::wrap(s, bold, size, width) {
            *y += size * 1.3;
            body.push_str(&format!(
                r#"<text x="{x:.1}" y="{:.1}" font-family="DejaVu Sans" font-size="{size}" font-weight="{}" fill="{color}">{}</text>"#,
                *y,
                if bold { "bold" } else { "normal" },
                esc(&line)
            ));
        }
    };
    if let Some(l) = &doc.letterhead {
        text(
            &mut body,
            &mut y,
            &l.agency_name,
            13.0,
            true,
            "#1F3864",
            PAD,
            inner,
        );
        y += 6.0;
    }
    text(
        &mut body, &mut y, &doc.title, 22.0, true, "#1F3864", PAD, inner,
    );
    if let Some(s) = &doc.subtitle {
        text(&mut body, &mut y, s, 14.0, false, "#404040", PAD, inner);
    }
    y += 10.0;
    for b in &doc.blocks {
        match b {
            Block::Heading { text: t, level } => {
                y += 8.0;
                let size = if *level <= 1 { 17.0 } else { 15.0 };
                text(&mut body, &mut y, t, size, true, "#1F3864", PAD, inner);
                desc_parts.push(t.clone());
            }
            Block::Paragraph(r) => {
                let s: String = r.iter().map(|i| i.text.as_str()).collect();
                text(&mut body, &mut y, &s, 13.0, false, "#000000", PAD, inner);
                y += 6.0;
                desc_parts.push(s);
            }
            Block::List { ordered, items } => {
                for (i, item) in items.iter().enumerate() {
                    let s: String = item.iter().map(|x| x.text.as_str()).collect();
                    let label = if *ordered {
                        format!("{}. ", i + 1)
                    } else {
                        "• ".into()
                    };
                    text(
                        &mut body,
                        &mut y,
                        &format!("{label}{s}"),
                        13.0,
                        false,
                        "#000000",
                        PAD + 10.0,
                        inner - 10.0,
                    );
                    desc_parts.push(s);
                }
                y += 6.0;
            }
            Block::Table(t) => {
                let cols = t
                    .headers
                    .len()
                    .max(t.rows.iter().map(Vec::len).max().unwrap_or(0))
                    .max(1);
                let cw = inner / cols as f32;
                let size = 12.0;
                if let Some(c) = &t.caption {
                    text(&mut body, &mut y, c, 13.0, true, "#404040", PAD, inner);
                    desc_parts.push(format!("Table: {c}"));
                }
                y += 4.0;
                let mut rows: Vec<(bool, &Vec<String>)> = Vec::new();
                if !t.headers.is_empty() {
                    rows.push((true, &t.headers));
                }
                rows.extend(t.rows.iter().map(|r| (false, r)));
                for (header, r) in rows {
                    let wrapped: Vec<Vec<String>> = (0..cols)
                        .map(|c| {
                            fonts::wrap(
                                r.get(c).map(String::as_str).unwrap_or(""),
                                header,
                                size,
                                cw - 12.0,
                            )
                        })
                        .collect();
                    let h =
                        wrapped.iter().map(Vec::len).max().unwrap_or(1) as f32 * size * 1.3 + 10.0;
                    for c in 0..cols {
                        let x = PAD + c as f32 * cw;
                        body.push_str(&format!(
                            r##"<rect x="{x:.1}" y="{y:.1}" width="{cw:.1}" height="{h:.1}" fill="{}" stroke="#808080" stroke-width="1"/>"##,
                            if header { "#D9E2F3" } else { "#FFFFFF" }
                        ));
                        let mut ly = y + 5.0;
                        for line in &wrapped[c] {
                            ly += size * 1.3;
                            body.push_str(&format!(
                                r##"<text x="{:.1}" y="{:.1}" font-family="DejaVu Sans" font-size="{size}" font-weight="{}" fill="#000000">{}</text>"##,
                                x + 6.0,
                                ly - 3.0,
                                if header { "bold" } else { "normal" },
                                esc(line)
                            ));
                        }
                    }
                    desc_parts.push(r.join(", "));
                    y += h;
                }
                y += 12.0;
            }
            Block::Image(_) | Block::PageBreak => {}
        }
    }
    let height = y + PAD;
    let mut desc: String = desc_parts.join(". ");
    if desc.chars().count() > 1500 {
        desc = desc.chars().take(1500).collect::<String>() + "…";
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height:.0}" viewBox="0 0 {WIDTH} {height:.0}" role="img" aria-labelledby="t d" xml:lang="{}"><title id="t">{}</title><desc id="d">{}</desc><rect width="100%" height="100%" fill="#FFFFFF"/>{body}</svg>"##,
        esc(&doc.language),
        esc(&doc.title),
        esc(&desc)
    )
}

/// Render the document as PNG at 2x scale using the bundled fonts.
pub fn to_png(doc: &Document) -> Result<Vec<u8>> {
    svg_to_png(&to_svg(doc), 2.0).map_err(|e| match e {
        PhnError::InvalidInput(_) => PhnError::InvalidInput(
            "the document is too long to render as one image; export to PDF instead".into(),
        ),
        other => other,
    })
}

/// Rasterize any SVG (charts, maps, documents) to PNG at `scale`, using the
/// bundled DejaVu Sans fonts so text renders the same on every machine.
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let mut opt = usvg::Options {
        font_family: "DejaVu Sans".into(),
        ..usvg::Options::default()
    };
    opt.fontdb_mut().load_font_data(fonts::REGULAR.to_vec());
    opt.fontdb_mut().load_font_data(fonts::BOLD.to_vec());
    let tree = usvg::Tree::from_str(svg, &opt)
        .map_err(|e| PhnError::Internal(format!("SVG render: {e}")))?;
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    if w.saturating_mul(h) > 60_000_000 {
        return Err(PhnError::InvalidInput(
            "the image is too large to render".into(),
        ));
    }
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| PhnError::Internal("cannot allocate image".into()))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| PhnError::Internal(format!("PNG encode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::from_markdown;

    #[test]
    fn svg_has_accessible_name_and_png_renders() {
        let doc = Document {
            title: "Flu clinic <Saturday>".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: None,
            logo: None,
            blocks: from_markdown(
                "Free flu shots.\n\n| Site | Time |\n|---|---|\n| Town Hall | 9–12 |\n",
            ),
        };
        let svg = to_svg(&doc);
        assert!(svg.contains("<title id=\"t\">Flu clinic &lt;Saturday&gt;</title>"));
        assert!(svg.contains("Town Hall"));
        let png = to_png(&doc).unwrap();
        let info = crate::image::inspect(&png).unwrap();
        assert_eq!(info.width, 1600);
    }
}
