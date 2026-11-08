//! SVG rendering of a choropleth map.
//!
//! Layout (800 px wide): title and subtitle on top, the map on the left, the
//! legend on the right, and the source note at the bottom. Fills come from
//! the one-hue sequential ramp (`phn_docs::palette::sequential_steps`, light
//! to dark); suppressed or missing areas get a gray hatch; areas outside the
//! data are drawn as white context. The SVG carries `<title>` and `<desc>`
//! (the alt text) so it has an accessible name and description.

use std::fmt::Write as _;

use phn_data::boundaries::Feature;
use phn_docs::palette;

/// How a feature is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// Class index into the color steps.
    Class(usize),
    /// Suppressed small count or missing value (hatched).
    Suppressed,
    /// Not part of the data (context only).
    Context,
}

/// One legend row.
#[derive(Debug, Clone)]
pub struct LegendRow {
    pub label: String,
    pub color: Option<String>,
    pub hatched: bool,
    pub outline_only: bool,
}

/// Everything the renderer needs.
pub struct MapSpec<'a> {
    pub title: String,
    pub subtitle: Option<String>,
    pub legend_title: String,
    pub legend: Vec<LegendRow>,
    pub note: String,
    pub desc: String,
    pub features: Vec<(&'a Feature, Fill)>,
    /// Outlines drawn on top for orientation (e.g. towns over tracts).
    pub overlay: Vec<&'a Feature>,
    pub colors: Vec<&'static str>,
    /// Labels to draw: (feature, text).
    pub labels: Vec<(&'a Feature, String)>,
    /// Map extent in metres (min x, min y, max x, max y).
    pub extent: (f64, f64, f64, f64),
}

const WIDTH: f64 = 800.0;
const PAD: f64 = 24.0;
const LEGEND_W: f64 = 230.0;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > max_chars {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn path_data(f: &Feature, tx: &dyn Fn(f64, f64) -> (f64, f64)) -> String {
    let mut d = String::new();
    for poly in &f.polygons {
        for ring in poly {
            let mut last = (f64::NAN, f64::NAN);
            for (i, &(x, y)) in ring.iter().enumerate() {
                let (px, py) = tx(x, y);
                let p = ((px * 10.0).round() / 10.0, (py * 10.0).round() / 10.0);
                if i > 0 && p == last {
                    continue;
                }
                let _ = write!(d, "{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, p.0, p.1);
                last = p;
            }
            d.push('Z');
        }
    }
    d
}

/// Render the map as SVG text.
pub fn render(spec: &MapSpec<'_>) -> String {
    let (minx, miny, maxx, maxy) = spec.extent;
    let (w_m, h_m) = ((maxx - minx).max(1.0), (maxy - miny).max(1.0));
    let map_w = WIDTH - 2.0 * PAD - LEGEND_W - 16.0;
    let mut scale = map_w / w_m;
    let mut map_h = h_m * scale;
    if map_h > 640.0 {
        scale = 640.0 / h_m;
        map_h = 640.0;
    }
    let drawn_w = w_m * scale;

    let title_lines = wrap(&spec.title, 62);
    let sub_lines = spec.subtitle.as_deref().map(|s| wrap(s, 90)).unwrap_or_default();
    let mut y = PAD;
    let mut head = String::new();
    for l in &title_lines {
        y += 22.0;
        let _ = write!(
            head,
            r#"<text x="{PAD}" y="{y:.1}" font-family="DejaVu Sans" font-size="18" font-weight="bold" fill="{}">{}</text>"#,
            palette::INK,
            esc(l)
        );
    }
    for l in &sub_lines {
        y += 17.0;
        let _ = write!(
            head,
            r#"<text x="{PAD}" y="{y:.1}" font-family="DejaVu Sans" font-size="13" fill="{}">{}</text>"#,
            palette::INK_SECONDARY,
            esc(l)
        );
    }
    let map_top = y + 14.0;
    let ox = PAD + (map_w - drawn_w) / 2.0;
    let tx = move |x: f64, yv: f64| (ox + (x - minx) * scale, map_top + (maxy - yv) * scale);

    let mut body = String::new();
    // Hatch pattern for suppressed or missing values.
    let _ = write!(
        body,
        r##"<defs><pattern id="hatch" patternUnits="userSpaceOnUse" width="6" height="6" patternTransform="rotate(45)"><rect width="6" height="6" fill="{}"/><line x1="0" y1="0" x2="0" y2="6" stroke="{}" stroke-width="1.5"/></pattern><clipPath id="mapclip"><rect x="{PAD}" y="{map_top:.1}" width="{map_w:.1}" height="{map_h:.1}"/></clipPath></defs>"##,
        palette::SUPPRESSED,
        palette::INK_MUTED
    );
    body.push_str(r#"<g clip-path="url(#mapclip)">"#);
    // Context first, then data, so data borders are on top.
    let mut ordered: Vec<&(&Feature, Fill)> = spec.features.iter().collect();
    ordered.sort_by_key(|(_, f)| match f {
        Fill::Context => 0,
        _ => 1,
    });
    for (f, fill) in ordered {
        let d = path_data(f, &tx);
        if d.is_empty() {
            continue;
        }
        let (fill_attr, stroke, sw) = match fill {
            Fill::Class(i) => (
                spec.colors[(*i).min(spec.colors.len() - 1)].to_string(),
                palette::SURFACE,
                0.6,
            ),
            Fill::Suppressed => ("url(#hatch)".to_string(), palette::SURFACE, 0.6),
            Fill::Context => (palette::SURFACE.to_string(), palette::AXIS, 0.5),
        };
        let _ = write!(
            body,
            r#"<path d="{d}" fill="{fill_attr}" fill-rule="evenodd" stroke="{stroke}" stroke-width="{sw}" stroke-linejoin="round"/>"#
        );
    }
    for f in &spec.overlay {
        let d = path_data(f, &tx);
        let _ = write!(
            body,
            r#"<path d="{d}" fill="none" stroke="{}" stroke-width="1.1" stroke-linejoin="round"/>"#,
            palette::INK_SECONDARY
        );
    }
    for (f, text) in &spec.labels {
        let (lx, ly) = f.label_point();
        let (px, py) = tx(lx, ly);
        let _ = write!(
            body,
            r#"<text x="{px:.1}" y="{py:.1}" font-family="DejaVu Sans" font-size="9" text-anchor="middle" fill="{}" stroke="{}" stroke-width="2.5" paint-order="stroke" stroke-linejoin="round">{}</text>"#,
            palette::INK,
            palette::SURFACE,
            esc(text)
        );
    }
    body.push_str("</g>");
    // Map frame.
    let _ = write!(
        body,
        r#"<rect x="{PAD}" y="{map_top:.1}" width="{map_w:.1}" height="{map_h:.1}" fill="none" stroke="{}" stroke-width="0.8"/>"#,
        palette::GRID
    );

    // Legend.
    let lx = WIDTH - PAD - LEGEND_W;
    let mut ly = map_top + 4.0;
    for l in wrap(&spec.legend_title, 30) {
        ly += 15.0;
        let _ = write!(
            body,
            r#"<text x="{lx:.1}" y="{ly:.1}" font-family="DejaVu Sans" font-size="12" font-weight="bold" fill="{}">{}</text>"#,
            palette::INK,
            esc(&l)
        );
    }
    ly += 8.0;
    for row in &spec.legend {
        let fill = if row.hatched {
            "url(#hatch)".to_string()
        } else {
            row.color.clone().unwrap_or_else(|| palette::SURFACE.to_string())
        };
        let stroke = if row.outline_only { palette::AXIS } else { palette::INK_MUTED };
        let _ = write!(
            body,
            r#"<rect x="{lx:.1}" y="{ly:.1}" width="22" height="14" fill="{fill}" stroke="{stroke}" stroke-width="0.6"/>"#
        );
        let lines = wrap(&row.label, 26);
        let mut ty = ly + 11.0;
        for l in &lines {
            let _ = write!(
                body,
                r#"<text x="{:.1}" y="{ty:.1}" font-family="DejaVu Sans" font-size="11" fill="{}">{}</text>"#,
                lx + 30.0,
                palette::INK,
                esc(l)
            );
            ty += 13.0;
        }
        ly += (lines.len().max(1) as f64 * 13.0).max(14.0) + 8.0;
    }

    let mut bottom = (map_top + map_h).max(ly) + 10.0;
    for l in wrap(&spec.note, 118) {
        bottom += 13.0;
        let _ = write!(
            body,
            r#"<text x="{PAD}" y="{bottom:.1}" font-family="DejaVu Sans" font-size="10" fill="{}">{}</text>"#,
            palette::INK_SECONDARY,
            esc(&l)
        );
    }
    let height = (bottom + PAD).ceil();
    let mut desc = spec.desc.clone();
    if desc.chars().count() > 2000 {
        desc = desc.chars().take(2000).collect::<String>() + "…";
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-labelledby="t d" xml:lang="en-US"><title id="t">{}</title><desc id="d">{}</desc><rect width="100%" height="100%" fill="{}"/>{head}{body}</svg>"##,
        esc(&spec.title),
        esc(&desc),
        palette::SURFACE
    )
}
