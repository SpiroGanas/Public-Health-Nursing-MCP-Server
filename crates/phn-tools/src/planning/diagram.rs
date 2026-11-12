//! Column-and-arrow diagrams for planning tools (logic models today; theory of
//! change and driver diagrams can use the same shape).
//!
//! A [`Diagram`] has columns read left to right (each a heading and boxes), optional
//! groups that label runs of columns ("Planned work", "Intended results"), and bands
//! under the columns for context (assumptions, external factors). It renders three ways:
//!
//! * [`Diagram::to_mermaid`]: Mermaid `flowchart LR` text that can be pasted into
//!   any Mermaid renderer (GitHub, Notion, Confluence, Mermaid Live);
//! * [`Diagram::to_svg`]: a self-contained, accessible SVG (`role="img"`, `<title>`,
//!   `<desc>` with the full text of every box, real text, dark ink on light fills, a
//!   marker for arrowheads, no scripts and no external files);
//! * [`Diagram::text_description`]: the same content as plain sentences, for alt text
//!   and the long description that goes beside the image.
//!
//! Colors come from the validated palette in `phn_docs::palette`; color never carries
//! meaning alone (every column has a heading, and groups have labels).

use phn_docs::palette::{AXIS, INK, INK_SECONDARY, SEQUENTIAL_BLUE, SURFACE, label_on};

/// Longest box text drawn, in characters (longer text is cut with an ellipsis).
const MAX_BOX_CHARS: usize = 400;

/// Which fill a column uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Planned work (inputs, activities, outputs): blue.
    Work,
    /// Intended results (outcomes): neutral gray.
    Result,
}

/// One column of boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramColumn {
    pub id: String,
    pub heading: String,
    pub items: Vec<String>,
    pub tone: Tone,
    /// Group label shown above this column; consecutive columns with the same label share one.
    pub group: Option<String>,
}

/// A band of boxes under the columns, across the full width.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramBand {
    pub id: String,
    pub heading: String,
    pub items: Vec<String>,
}

/// A diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: String,
    pub subtitle: Option<String>,
    pub columns: Vec<DiagramColumn>,
    pub bands: Vec<DiagramBand>,
    pub footer: Option<String>,
}

// ---------------------------------------------------------------- helpers

fn clip(s: &str) -> String {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= MAX_BOX_CHARS {
        t
    } else {
        let cut: String = t.chars().take(MAX_BOX_CHARS - 1).collect();
        format!("{}\u{2026}", cut.trim_end())
    }
}

/// Approximate width in pixels of text in DejaVu Sans (the bundled font) at `size` px.
/// Slightly generous so a wrapped line does not overflow its box in other sans-serif fonts.
fn text_width(s: &str, size: f32) -> f32 {
    let em: f32 = s
        .chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | '\'' | '|' | '!' | '.' | ',' | ':' | ';' => 0.30,
            ' ' => 0.34,
            'f' | 't' | 'r' | 'I' | '(' | ')' | '[' | ']' | '-' | '/' => 0.40,
            'm' | 'w' | 'M' | 'W' => 0.95,
            'A'..='Z' | '0'..='9' | '%' | '&' | '#' | '@' => 0.70,
            c if c.is_ascii() => 0.60,
            _ => 1.0,
        })
        .sum();
    em * size
}

/// Break text into lines no wider than `max_w` px. A single word wider than the line is split.
fn wrap(s: &str, max_w: f32, size: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let mut word = word.to_string();
        // Split words that are wider than a line on their own.
        while text_width(&word, size) > max_w && word.chars().count() > 1 {
            let mut head = String::new();
            let mut rest = String::new();
            for (i, c) in word.chars().enumerate() {
                if rest.is_empty() && text_width(&format!("{head}{c}-"), size) <= max_w {
                    head.push(c);
                } else {
                    rest.extend(word.chars().skip(i));
                    break;
                }
            }
            if head.is_empty() {
                break;
            }
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            lines.push(format!("{head}-"));
            word = rest;
        }
        let candidate = if cur.is_empty() {
            word.clone()
        } else {
            format!("{cur} {word}")
        };
        if text_width(&candidate, size) <= max_w || cur.is_empty() {
            cur = candidate;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn esc(s: &str) -> String {
    phn_docs::docx::esc(s)
}

/// Text safe inside a Mermaid quoted label.
fn mermaid_text(s: &str) -> String {
    clip(s)
        .replace('&', "#amp;")
        .replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
        .replace(['[', ']', '{', '}', '(', ')', '|', '`'], " ")
        .replace(['\n', '\r'], " ")
}

fn id_safe(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if t.is_empty() { "x".into() } else { t }
}

// ---------------------------------------------------------------- layout constants

const COL_W: f32 = 212.0;
const GAP: f32 = 40.0;
const MARGIN: f32 = 18.0;
const PAD: f32 = 8.0;
const FONT: f32 = 12.0;
const LINE_H: f32 = 15.5;
const HEAD_FONT: f32 = 13.0;
const HEAD_H: f32 = 34.0;
const BOX_GAP: f32 = 8.0;
const TITLE_H: f32 = 56.0;
const GROUP_H: f32 = 26.0;

impl Diagram {
    /// Number of boxes in the columns and bands.
    pub fn box_count(&self) -> usize {
        self.columns.iter().map(|c| c.items.len()).sum::<usize>()
            + self.bands.iter().map(|b| b.items.len()).sum::<usize>()
    }

    /// Short alt text: what the image is and how many boxes it has.
    pub fn alt_text(&self) -> String {
        let cols: Vec<String> = self
            .columns
            .iter()
            .map(|c| format!("{} ({})", c.heading, c.items.len()))
            .collect();
        let bands: Vec<String> = self
            .bands
            .iter()
            .map(|b| format!("{} ({})", b.heading, b.items.len()))
            .collect();
        let mut s = format!(
            "{}. A left-to-right diagram with {} columns joined by arrows: {}.",
            self.title.trim_end_matches('.'),
            self.columns.len(),
            cols.join(", ")
        );
        if !bands.is_empty() {
            s.push_str(&format!(" Below the columns: {}.", bands.join(", ")));
        }
        s.push_str(" The full text of every box follows the image.");
        s
    }

    /// The whole diagram as sentences, in reading order.
    pub fn text_description(&self) -> String {
        let mut out = String::new();
        let n = self.columns.len();
        for (i, c) in self.columns.iter().enumerate() {
            let arrow = if i + 1 < n {
                format!(" An arrow leads to {}.", self.columns[i + 1].heading)
            } else {
                String::new()
            };
            if c.items.is_empty() {
                out.push_str(&format!("{}: none listed.{arrow}\n", c.heading));
            } else {
                out.push_str(&format!(
                    "{}: {}.{arrow}\n",
                    c.heading,
                    c.items
                        .iter()
                        .map(|x| clip(x).trim_end_matches('.').to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }
        for b in &self.bands {
            if b.items.is_empty() {
                out.push_str(&format!("{}: none listed.\n", b.heading));
            } else {
                out.push_str(&format!(
                    "{}: {}.\n",
                    b.heading,
                    b.items
                        .iter()
                        .map(|x| clip(x).trim_end_matches('.').to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }
        out.trim_end().to_string()
    }

    /// Mermaid `flowchart LR` source. Columns are subgraphs joined by arrows; bands are
    /// subgraphs placed after the columns.
    pub fn to_mermaid(&self) -> String {
        let mut s = String::from("flowchart LR\n");
        for (ci, c) in self.columns.iter().enumerate() {
            let cid = format!("c{ci}_{}", id_safe(&c.id));
            s.push_str(&format!(
                "  subgraph {cid}[\"{}\"]\n    direction TB\n",
                mermaid_text(&c.heading)
            ));
            if c.items.is_empty() {
                s.push_str(&format!("    {cid}_none[\"None listed\"]\n"));
            }
            for (ii, it) in c.items.iter().enumerate() {
                s.push_str(&format!("    {cid}_{ii}[\"{}\"]\n", mermaid_text(it)));
            }
            s.push_str("  end\n");
        }
        for (bi, b) in self.bands.iter().enumerate() {
            let bid = format!("b{bi}_{}", id_safe(&b.id));
            s.push_str(&format!(
                "  subgraph {bid}[\"{}\"]\n    direction TB\n",
                mermaid_text(&b.heading)
            ));
            if b.items.is_empty() {
                s.push_str(&format!("    {bid}_none[\"None listed\"]\n"));
            }
            for (ii, it) in b.items.iter().enumerate() {
                s.push_str(&format!("    {bid}_{ii}[\"{}\"]\n", mermaid_text(it)));
            }
            s.push_str("  end\n");
        }
        for ci in 0..self.columns.len().saturating_sub(1) {
            s.push_str(&format!(
                "  c{ci}_{} --> c{}_{}\n",
                id_safe(&self.columns[ci].id),
                ci + 1,
                id_safe(&self.columns[ci + 1].id)
            ));
        }
        // Colors: work columns blue, result columns gray, context bands white with a dashed edge.
        s.push_str(&format!(
            "  classDef work fill:{},stroke:{},color:{}\n",
            SEQUENTIAL_BLUE[0], SEQUENTIAL_BLUE[10], INK
        ));
        s.push_str(&format!(
            "  classDef result fill:#f0efec,stroke:{},color:{}\n",
            INK_SECONDARY, INK
        ));
        s.push_str(&format!(
            "  classDef context fill:{SURFACE},stroke:{AXIS},stroke-dasharray:4 3,color:{INK}\n"
        ));
        for (ci, c) in self.columns.iter().enumerate() {
            let cid = format!("c{ci}_{}", id_safe(&c.id));
            let class = if c.tone == Tone::Work { "work" } else { "result" };
            let mut nodes: Vec<String> = (0..c.items.len()).map(|i| format!("{cid}_{i}")).collect();
            if c.items.is_empty() {
                nodes.push(format!("{cid}_none"));
            }
            s.push_str(&format!("  class {} {class}\n", nodes.join(",")));
        }
        for (bi, b) in self.bands.iter().enumerate() {
            let bid = format!("b{bi}_{}", id_safe(&b.id));
            let mut nodes: Vec<String> = (0..b.items.len()).map(|i| format!("{bid}_{i}")).collect();
            if b.items.is_empty() {
                nodes.push(format!("{bid}_none"));
            }
            s.push_str(&format!("  class {} context\n", nodes.join(",")));
        }
        s
    }

    /// A self-contained, accessible SVG of the diagram.
    pub fn to_svg(&self) -> String {
        let n = self.columns.len().max(1) as f32;
        let width = MARGIN * 2.0 + n * COL_W + (n - 1.0) * GAP;
        let inner_w = COL_W - 2.0 * PAD;

        // Column layout: heights of each box.
        struct Laid {
            lines: Vec<Vec<String>>,
            heights: Vec<f32>,
            total: f32,
        }
        let laid: Vec<Laid> = self
            .columns
            .iter()
            .map(|c| {
                let items: Vec<String> = if c.items.is_empty() {
                    vec!["None listed".into()]
                } else {
                    c.items.iter().map(|i| clip(i)).collect()
                };
                let lines: Vec<Vec<String>> =
                    items.iter().map(|i| wrap(i, inner_w, FONT)).collect();
                let heights: Vec<f32> = lines
                    .iter()
                    .map(|l| l.len() as f32 * LINE_H + 2.0 * PAD - 2.0)
                    .collect();
                let total = heights.iter().sum::<f32>()
                    + BOX_GAP * (heights.len().saturating_sub(1)) as f32;
                Laid {
                    lines,
                    heights,
                    total,
                }
            })
            .collect();
        let col_body_h = laid.iter().map(|l| l.total).fold(40.0_f32, f32::max);

        // Title block.
        let subtitle_lines = self
            .subtitle
            .as_deref()
            .map(|s| wrap(&clip(s), width - 2.0 * MARGIN, FONT))
            .unwrap_or_default();
        let title_lines = wrap(&clip(&self.title), width - 2.0 * MARGIN, 18.0);
        let head_h = title_lines.len() as f32 * 24.0 + subtitle_lines.len() as f32 * LINE_H + 14.0;
        let head_h = head_h.max(TITLE_H);

        let has_groups = self.columns.iter().any(|c| c.group.is_some());
        let group_h = if has_groups { GROUP_H + 6.0 } else { 0.0 };
        let cols_top = MARGIN + head_h + group_h;
        let cols_bottom = cols_top + HEAD_H + BOX_GAP + col_body_h;

        // Bands: boxes flow in rows across the full width, up to 3 per row.
        let band_w = width - 2.0 * MARGIN;
        struct BandLaid {
            rows: Vec<Vec<(f32, Vec<String>)>>, // per row: (box height, lines)
            per_row: usize,
            box_w: f32,
        }
        let mut band_laid: Vec<BandLaid> = Vec::new();
        for b in &self.bands {
            let items: Vec<String> = if b.items.is_empty() {
                vec!["None listed".into()]
            } else {
                b.items.iter().map(|i| clip(i)).collect()
            };
            let per_row = (((width - 2.0 * MARGIN) / 300.0).floor() as usize).clamp(1, 3);
            let box_w = (band_w - 2.0 * PAD - BOX_GAP * (per_row as f32 - 1.0)) / per_row as f32;
            let mut rows: Vec<Vec<(f32, Vec<String>)>> = Vec::new();
            for chunk in items.chunks(per_row) {
                rows.push(
                    chunk
                        .iter()
                        .map(|i| {
                            let l = wrap(i, box_w - 2.0 * PAD, FONT);
                            (l.len() as f32 * LINE_H + 2.0 * PAD - 2.0, l)
                        })
                        .collect(),
                );
            }
            band_laid.push(BandLaid {
                rows,
                per_row,
                box_w,
            });
        }
        let band_heights: Vec<f32> = band_laid
            .iter()
            .map(|b| {
                let rows_h: f32 = b
                    .rows
                    .iter()
                    .map(|r| r.iter().map(|x| x.0).fold(0.0_f32, f32::max))
                    .sum::<f32>()
                    + BOX_GAP * (b.rows.len().saturating_sub(1)) as f32;
                24.0 + rows_h + PAD * 2.0
            })
            .collect();
        let bands_total: f32 = band_heights.iter().sum::<f32>() + 12.0 * band_heights.len() as f32;
        let footer_lines = self
            .footer
            .as_deref()
            .map(|s| wrap(&clip(s), width - 2.0 * MARGIN, 11.0))
            .unwrap_or_default();
        let footer_h = if footer_lines.is_empty() {
            0.0
        } else {
            footer_lines.len() as f32 * 14.0 + 10.0
        };
        let height = cols_bottom + 14.0 + bands_total + footer_h + MARGIN;

        let mut svg = String::new();
        svg.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" width=\"{w:.0}\" height=\"{h:.0}\" role=\"img\" aria-labelledby=\"diagram-title diagram-desc\" font-family=\"DejaVu Sans, Verdana, Arial, sans-serif\">\n",
            w = width,
            h = height
        ));
        svg.push_str(&format!(
            "<title id=\"diagram-title\">{}</title>\n<desc id=\"diagram-desc\">{}</desc>\n",
            esc(&self.title),
            esc(&format!("{} {}", self.alt_text(), self.text_description().replace('\n', " ")))
        ));
        svg.push_str(&format!(
            "<defs><marker id=\"arrowhead\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto\"><path d=\"M0,0 L10,5 L0,10 z\" fill=\"{INK_SECONDARY}\"/></marker></defs>\n"
        ));
        svg.push_str(&format!(
            "<rect x=\"0\" y=\"0\" width=\"{width:.0}\" height=\"{height:.0}\" fill=\"{SURFACE}\"/>\n"
        ));
        // Title and subtitle.
        let mut y = MARGIN + 18.0;
        for l in &title_lines {
            svg.push_str(&format!(
                "<text x=\"{MARGIN}\" y=\"{y:.1}\" font-size=\"18\" font-weight=\"bold\" fill=\"{INK}\">{}</text>\n",
                esc(l)
            ));
            y += 24.0;
        }
        for l in &subtitle_lines {
            svg.push_str(&format!(
                "<text x=\"{MARGIN}\" y=\"{y:.1}\" font-size=\"{FONT}\" fill=\"{INK_SECONDARY}\">{}</text>\n",
                esc(l)
            ));
            y += LINE_H;
        }
        // Group labels.
        if has_groups {
            let gy = MARGIN + head_h;
            let mut i = 0;
            while i < self.columns.len() {
                let label = self.columns[i].group.clone();
                let mut j = i;
                while j + 1 < self.columns.len() && self.columns[j + 1].group == label {
                    j += 1;
                }
                if let Some(l) = label {
                    let x0 = MARGIN + i as f32 * (COL_W + GAP);
                    let x1 = MARGIN + j as f32 * (COL_W + GAP) + COL_W;
                    svg.push_str(&format!(
                        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{FONT}\" font-weight=\"bold\" text-anchor=\"middle\" fill=\"{INK_SECONDARY}\">{}</text>\n<line x1=\"{x0:.1}\" y1=\"{:.1}\" x2=\"{x1:.1}\" y2=\"{:.1}\" stroke=\"{INK_SECONDARY}\" stroke-width=\"1.5\"/>\n",
                        (x0 + x1) / 2.0,
                        gy + 14.0,
                        esc(&l.to_uppercase()),
                        gy + 20.0,
                        gy + 20.0
                    ));
                }
                i = j + 1;
            }
        }
        // Columns.
        for (ci, (c, l)) in self.columns.iter().zip(&laid).enumerate() {
            let x = MARGIN + ci as f32 * (COL_W + GAP);
            let (head_fill, item_fill, edge) = match c.tone {
                Tone::Work => (SEQUENTIAL_BLUE[10], SEQUENTIAL_BLUE[0], SEQUENTIAL_BLUE[10]),
                Tone::Result => (INK_SECONDARY, "#f0efec", INK_SECONDARY),
            };
            svg.push_str(&format!(
                "<g id=\"col-{}\">\n<rect x=\"{x:.1}\" y=\"{cols_top:.1}\" width=\"{COL_W}\" height=\"{HEAD_H}\" rx=\"4\" fill=\"{head_fill}\"/>\n<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{HEAD_FONT}\" font-weight=\"bold\" text-anchor=\"middle\" fill=\"{}\">{}</text>\n",
                id_safe(&c.id),
                x + COL_W / 2.0,
                cols_top + HEAD_H / 2.0 + 4.5,
                label_on(head_fill),
                esc(&c.heading)
            ));
            let mut by = cols_top + HEAD_H + BOX_GAP;
            for (lines, h) in l.lines.iter().zip(&l.heights) {
                svg.push_str(&format!(
                    "<rect x=\"{x:.1}\" y=\"{by:.1}\" width=\"{COL_W}\" height=\"{h:.1}\" rx=\"4\" fill=\"{item_fill}\" stroke=\"{edge}\" stroke-width=\"1\"/>\n<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{FONT}\" fill=\"{INK}\">",
                    x + PAD,
                    by + PAD + 10.0
                ));
                for (k, line) in lines.iter().enumerate() {
                    svg.push_str(&format!(
                        "<tspan x=\"{:.1}\" dy=\"{}\">{}</tspan>",
                        x + PAD,
                        if k == 0 { 0.0 } else { LINE_H },
                        esc(line)
                    ));
                }
                svg.push_str("</text>\n");
                by += h + BOX_GAP;
            }
            svg.push_str("</g>\n");
        }
        // Arrows between columns, at the middle of the column area.
        let ay = cols_top + HEAD_H + BOX_GAP + col_body_h / 2.0;
        for ci in 0..self.columns.len().saturating_sub(1) {
            let x0 = MARGIN + ci as f32 * (COL_W + GAP) + COL_W + 3.0;
            let x1 = MARGIN + (ci + 1) as f32 * (COL_W + GAP) - 3.0;
            svg.push_str(&format!(
                "<line x1=\"{x0:.1}\" y1=\"{ay:.1}\" x2=\"{x1:.1}\" y2=\"{ay:.1}\" stroke=\"{INK_SECONDARY}\" stroke-width=\"2\" marker-end=\"url(#arrowhead)\"/>\n"
            ));
        }
        // Bands.
        let mut by = cols_bottom + 14.0;
        for (b, (bl, bh)) in self.bands.iter().zip(band_laid.iter().zip(&band_heights)) {
            svg.push_str(&format!(
                "<g id=\"band-{}\">\n<rect x=\"{MARGIN}\" y=\"{by:.1}\" width=\"{band_w:.1}\" height=\"{bh:.1}\" rx=\"4\" fill=\"{SURFACE}\" stroke=\"{AXIS}\" stroke-width=\"1.5\" stroke-dasharray=\"5 3\"/>\n<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{HEAD_FONT}\" font-weight=\"bold\" fill=\"{INK}\">{}</text>\n",
                id_safe(&b.id),
                MARGIN + PAD,
                by + 18.0,
                esc(&b.heading)
            ));
            let mut ry = by + 24.0 + PAD;
            for row in &bl.rows {
                let row_h = row.iter().map(|x| x.0).fold(0.0_f32, f32::max);
                for (k, (h, lines)) in row.iter().enumerate() {
                    let bx = MARGIN + PAD + k as f32 * (bl.box_w + BOX_GAP);
                    svg.push_str(&format!(
                        "<rect x=\"{bx:.1}\" y=\"{ry:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"4\" fill=\"#f7f6f3\" stroke=\"{AXIS}\" stroke-width=\"1\"/>\n<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{FONT}\" fill=\"{INK}\">",
                        bl.box_w,
                        row_h.max(*h),
                        bx + PAD,
                        ry + PAD + 10.0
                    ));
                    for (m, line) in lines.iter().enumerate() {
                        svg.push_str(&format!(
                            "<tspan x=\"{:.1}\" dy=\"{}\">{}</tspan>",
                            bx + PAD,
                            if m == 0 { 0.0 } else { LINE_H },
                            esc(line)
                        ));
                    }
                    svg.push_str("</text>\n");
                }
                ry += row_h + BOX_GAP;
            }
            let _ = bl.per_row;
            svg.push_str("</g>\n");
            by += bh + 12.0;
        }
        // Footer.
        let mut fy = by + 4.0;
        for l in &footer_lines {
            fy += 12.0;
            svg.push_str(&format!(
                "<text x=\"{MARGIN}\" y=\"{fy:.1}\" font-size=\"11\" fill=\"{INK_SECONDARY}\">{}</text>\n",
                esc(l)
            ));
            fy += 2.0;
        }
        svg.push_str("</svg>\n");
        svg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_docs::palette::contrast;

    fn sample() -> Diagram {
        Diagram {
            title: "Home visiting program".into(),
            subtitle: Some("Logic model, planning stage".into()),
            columns: vec![
                DiagramColumn {
                    id: "inputs".into(),
                    heading: "Inputs".into(),
                    items: vec![
                        "Two public health nurses".into(),
                        "Grant funding \"2026\" <draft> & partners".into(),
                    ],
                    tone: Tone::Work,
                    group: Some("Planned work".into()),
                },
                DiagramColumn {
                    id: "activities".into(),
                    heading: "Activities".into(),
                    items: vec![
                        "Provide home visits to first-time parents".into(),
                        "Extraordinarilylongwordwithoutanyspacesthatcannotfitononelineofabox and more".into(),
                    ],
                    tone: Tone::Work,
                    group: Some("Planned work".into()),
                },
                DiagramColumn {
                    id: "short_term".into(),
                    heading: "Short-term outcomes".into(),
                    items: vec![],
                    tone: Tone::Result,
                    group: Some("Intended results".into()),
                },
            ],
            bands: vec![DiagramBand {
                id: "external".into(),
                heading: "External factors".into(),
                items: vec!["State funding cycle".into(), "Housing costs".into()],
            }],
            footer: Some("Source: test".into()),
        }
    }

    #[test]
    fn svg_is_self_contained_and_accessible() {
        let svg = sample().to_svg();
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("role=\"img\""));
        assert!(svg.contains("<title id=\"diagram-title\">Home visiting program</title>"));
        assert!(svg.contains("<desc id=\"diagram-desc\">"));
        assert!(svg.contains("aria-labelledby=\"diagram-title diagram-desc\""));
        for bad in ["<script", "http://", "href=", "<image", "<foreignObject"] {
            // The only http:// is the xmlns declaration.
            let n = svg.matches(bad).count();
            if bad == "http://" {
                assert_eq!(n, 1, "{bad}");
            } else {
                assert_eq!(n, 0, "{bad}");
            }
        }
        // Text is escaped, not injected.
        assert!(svg.contains("&lt;draft&gt;"));
        assert!(svg.contains("&amp;"));
        assert!(!svg.contains("<draft>"));
        // Every box text is in the description.
        assert!(svg.contains("Two public health nurses"));
        // The empty column shows a placeholder, so no column is blank.
        assert!(svg.contains("None listed"));
        // Balanced groups.
        assert_eq!(svg.matches("<g ").count(), svg.matches("</g>").count());
        // Valid XML.
        let r = well_formed(&svg);
        assert!(r.is_ok(), "{r:?}");
    }

    // A tiny well-formedness check: tags nest and close.
    fn well_formed(svg: &str) -> Result<(), String> {
        let mut stack: Vec<String> = Vec::new();
        let mut rest = svg;
        while let Some(i) = rest.find('<') {
            rest = &rest[i + 1..];
            let end = rest.find('>').ok_or("unterminated tag")?;
            let tag = &rest[..end];
            rest = &rest[end + 1..];
            if tag.starts_with('?') || tag.starts_with('!') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                let open = stack.pop().ok_or("closing tag with no opener")?;
                if open != name.trim() {
                    return Err(format!("mismatch {open} / {name}"));
                }
            } else if !tag.ends_with('/') {
                stack.push(tag.split_whitespace().next().unwrap_or("").to_string());
            }
        }
        if stack.is_empty() {
            Ok(())
        } else {
            Err(format!("unclosed {stack:?}"))
        }
    }

    #[test]
    fn svg_renders_to_png() {
        let png = phn_docs::svg::svg_to_png(&sample().to_svg(), 1.0).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn text_has_dark_ink_on_light_fills() {
        for fill in [SEQUENTIAL_BLUE[0], "#f0efec", "#f7f6f3", SURFACE] {
            assert!(contrast(INK, fill) >= 7.0, "{fill}");
        }
        for head in [SEQUENTIAL_BLUE[10], INK_SECONDARY] {
            assert!(contrast(label_on(head), head) >= 4.5, "{head}");
        }
    }

    #[test]
    fn mermaid_is_well_formed() {
        let m = sample().to_mermaid();
        assert!(m.starts_with("flowchart LR\n"));
        assert_eq!(
            m.matches("  subgraph ").count(),
            m.matches("\n  end\n").count()
        );
        // Quotes, angle brackets, and ampersands in text are escaped.
        assert!(m.contains("#quot;2026#quot;"));
        assert!(m.contains("#lt;draft#gt; #amp; partners"));
        // No raw double quote inside a label: every quote is a delimiter, so the count is even.
        for line in m.lines() {
            assert_eq!(line.matches('"').count() % 2, 0, "{line}");
        }
        assert!(m.contains("c0_inputs --> c1_activities"));
        assert!(m.contains("c1_activities --> c2_short_term"));
        assert!(m.contains("classDef work"));
        // Node ids are unique.
        let mut ids: Vec<&str> = m
            .lines()
            .filter(|l| l.starts_with("    ") && l.contains("[\""))
            .map(|l| l.trim().split('[').next().unwrap())
            .collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn long_words_are_split_and_boxes_hold_their_text() {
        let lines = wrap("Extraordinarilylongwordwithoutanyspacesthatcannotfit", 120.0, 12.0);
        assert!(lines.len() > 1);
        for l in &lines {
            assert!(text_width(l, 12.0) <= 125.0, "{l}");
        }
        // Wrapping keeps every character.
        let joined: String = lines.concat().replace('-', "");
        assert_eq!(joined, "Extraordinarilylongwordwithoutanyspacesthatcannotfit");
    }

    #[test]
    fn text_description_reads_left_to_right() {
        let t = sample().text_description();
        assert!(t.starts_with("Inputs: Two public health nurses"));
        assert!(t.contains("An arrow leads to Activities."));
        assert!(t.contains("Short-term outcomes: none listed."));
        assert!(t.contains("External factors: State funding cycle; Housing costs."));
        assert!(sample().alt_text().contains("3 columns"));
    }

    #[test]
    fn long_text_is_clipped_on_character_boundaries() {
        let long = "é".repeat(1000);
        let c = clip(&long);
        assert_eq!(c.chars().count(), MAX_BOX_CHARS);
        assert!(c.ends_with('\u{2026}'));
    }
}
