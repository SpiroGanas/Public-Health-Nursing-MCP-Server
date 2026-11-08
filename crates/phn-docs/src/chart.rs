//! Report-quality charts as accessible SVG (and PNG through
//! [`crate::svg::svg_to_png`]).
//!
//! One renderer serves every chart the server draws: bar (grouped), stacked
//! bar, line, dot (estimates with confidence intervals, optionally on a log
//! scale), and histogram-style bars for epidemic curves. Control-chart
//! features (reference lines that can vary by point, flagged points, shaded
//! spans, and vertical markers) are part of the same spec.
//!
//! Accessibility, following WCAG 2.1 and the palette notes in
//! [`crate::palette`]: an SVG `<title>` and `<desc>` (generated alt text),
//! colors in the validated fixed order, a legend for two or more series plus
//! direct labels, marker shapes as a second channel, a hatch pattern for
//! suppressed values, text in ink colors (never series colors), and the data
//! returned as a table alongside the image.

use std::fmt::Write as _;

use phn_core::{PhnError, Result};
use serde::{Deserialize, Serialize};

use crate::docx::esc;
use crate::fonts::text_width;
use crate::palette::{self, Marker};

/// Chart forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    /// Vertical bars; several series are grouped side by side.
    Bar,
    /// Vertical bars with series stacked.
    StackedBar,
    /// Lines with point markers (time series).
    Line,
    /// Points with optional confidence intervals, one row per category
    /// (forest-plot style), values on the horizontal axis.
    Dot,
    /// Contiguous stacked bars for counts by time interval (epidemic curve).
    Histogram,
}

/// One data series.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    /// One value per category; `None` is missing.
    pub values: Vec<Option<f64>>,
    /// Lower confidence limits (dot and line charts).
    #[serde(default)]
    pub lower: Option<Vec<Option<f64>>>,
    /// Upper confidence limits.
    #[serde(default)]
    pub upper: Option<Vec<Option<f64>>>,
    /// Per-category flags for values hidden by small-cell suppression.
    #[serde(default)]
    pub suppressed: Vec<bool>,
}

/// A horizontal reference line (target, center line, control limit).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefLine {
    pub label: String,
    /// One value (constant) or one per category (stepped, e.g. p-chart limits).
    pub values: Vec<Option<f64>>,
    /// Dashed (limits) or solid (center line).
    pub dashed: bool,
}

/// A shaded span along the category axis (e.g., the likely exposure window).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Band {
    /// Start, in category positions (0 = first category; fractions allowed).
    pub from: f64,
    /// End (exclusive), in category positions.
    pub to: f64,
    pub label: String,
}

/// A vertical marker at a category position (e.g., a known exposure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VMarker {
    /// Category position (0 = first category; fractions allowed).
    pub at: f64,
    pub label: String,
}

/// A highlighted data point (e.g., a special-cause signal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flag {
    pub series: usize,
    pub index: usize,
    pub label: String,
}

/// Everything needed to draw a chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    #[serde(default)]
    pub x_label: Option<String>,
    #[serde(default)]
    pub y_label: Option<String>,
    /// Include zero on the value axis (always true for bar charts and histograms).
    #[serde(default)]
    pub include_zero: Option<bool>,
    /// Logarithmic value axis (ratios); all values must be positive.
    #[serde(default)]
    pub log_scale: bool,
    /// Decimal places for value labels and the table (default: automatic).
    #[serde(default)]
    pub decimals: Option<usize>,
    #[serde(default)]
    pub ref_lines: Vec<RefLine>,
    #[serde(default)]
    pub bands: Vec<Band>,
    #[serde(default)]
    pub markers: Vec<VMarker>,
    #[serde(default)]
    pub flags: Vec<Flag>,
    /// Label for flagged points in the legend (default "Flagged").
    #[serde(default)]
    pub flag_legend: Option<String>,
    /// Source or note line under the chart.
    #[serde(default)]
    pub note: Option<String>,
    /// Show values on bars/points (default: automatic when uncluttered).
    #[serde(default)]
    pub value_labels: Option<bool>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
}

impl ChartSpec {
    /// A spec with defaults.
    pub fn new(kind: ChartKind, title: impl Into<String>, categories: Vec<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            subtitle: None,
            categories,
            series: Vec::new(),
            x_label: None,
            y_label: None,
            include_zero: None,
            log_scale: false,
            decimals: None,
            ref_lines: Vec::new(),
            bands: Vec::new(),
            markers: Vec::new(),
            flags: Vec::new(),
            flag_legend: None,
            note: None,
            value_labels: None,
            width: None,
            height: None,
        }
    }
}

/// A rendered chart: SVG text, alt text, and the data as a table.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub svg: String,
    pub alt_text: String,
    pub table_headers: Vec<String>,
    pub table_rows: Vec<Vec<String>>,
}

/// Maximum categorical series (the palette's validated length).
pub const MAX_SERIES: usize = 8;
/// Maximum categories on one chart.
pub const MAX_CATEGORIES: usize = 400;

const TITLE: f32 = 18.0;
const SUB: f32 = 13.0;
const TICK: f32 = 11.0;
const LABEL: f32 = 12.0;
const PAD: f32 = 20.0;

/// Validate a spec and render it.
pub fn render(spec: &ChartSpec) -> Result<Rendered> {
    validate(spec)?;
    let decimals = spec.decimals.unwrap_or_else(|| auto_decimals(spec));
    let alt_text = alt_text(spec, decimals);
    let (table_headers, table_rows) = table(spec, decimals);
    let svg = Layout::new(spec, decimals).draw(spec, &alt_text);
    Ok(Rendered {
        svg,
        alt_text,
        table_headers,
        table_rows,
    })
}

fn validate(spec: &ChartSpec) -> Result<()> {
    let n = spec.categories.len();
    if n == 0 {
        return Err(PhnError::InvalidInput(
            "a chart needs at least one category".into(),
        ));
    }
    if n > MAX_CATEGORIES {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_CATEGORIES} categories per chart (got {n}); aggregate to longer intervals"
        )));
    }
    if spec.series.is_empty() {
        return Err(PhnError::InvalidInput(
            "a chart needs at least one series".into(),
        ));
    }
    if spec.series.len() > MAX_SERIES {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_SERIES} series per chart (got {}): combine the smallest into \"Other\" \
             or make several charts",
            spec.series.len()
        )));
    }
    for s in &spec.series {
        if s.values.len() != n {
            return Err(PhnError::InvalidInput(format!(
                "series '{}' has {} values but there are {n} categories",
                s.name,
                s.values.len()
            )));
        }
        for v in [&s.lower, &s.upper].into_iter().flatten() {
            if v.len() != n {
                return Err(PhnError::InvalidInput(format!(
                    "the confidence limits for '{}' need one value per category",
                    s.name
                )));
            }
        }
        if s.values.iter().flatten().any(|v| !v.is_finite()) {
            return Err(PhnError::InvalidInput(format!(
                "series '{}' contains a value that is not a finite number",
                s.name
            )));
        }
    }
    for r in &spec.ref_lines {
        if !(r.values.len() == 1 || r.values.len() == n) {
            return Err(PhnError::InvalidInput(format!(
                "reference line '{}' needs one value or one per category",
                r.label
            )));
        }
    }
    if spec.log_scale && all_values(spec).any(|v| v <= 0.0) {
        return Err(PhnError::InvalidInput(
            "a log scale needs every value and limit to be above zero".into(),
        ));
    }
    if matches!(spec.kind, ChartKind::StackedBar | ChartKind::Histogram)
        && spec
            .series
            .iter()
            .flat_map(|s| s.values.iter().flatten())
            .any(|v| *v < 0.0)
    {
        return Err(PhnError::InvalidInput(
            "stacked bars and histograms need non-negative values".into(),
        ));
    }
    Ok(())
}

fn all_values(spec: &ChartSpec) -> impl Iterator<Item = f64> + '_ {
    spec.series
        .iter()
        .flat_map(|s| {
            s.values
                .iter()
                .chain(s.lower.iter().flatten())
                .chain(s.upper.iter().flatten())
        })
        .flatten()
        .copied()
        .chain(
            spec.ref_lines
                .iter()
                .flat_map(|r| r.values.iter().flatten().copied()),
        )
}

fn auto_decimals(spec: &ChartSpec) -> usize {
    let vals: Vec<f64> = all_values(spec).collect();
    let max = vals.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let all_int = vals.iter().all(|v| (v - v.round()).abs() < 1e-9);
    if all_int {
        0
    } else if max >= 10.0 {
        1
    } else if max >= 1.0 {
        2
    } else {
        3
    }
}

/// Format a number with thousands separators.
pub fn fmt_num(x: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, x);
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s, None),
    };
    let (sign, digits) = match int.strip_prefix('-') {
        Some(d) => ("-", d.to_string()),
        None => ("", int),
    };
    let mut g = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            g.push(',');
        }
        g.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{g}.{f}"),
        None => format!("{sign}{g}"),
    }
}

// ------------------------------------------------------------------ alt text

fn kind_name(k: ChartKind) -> &'static str {
    match k {
        ChartKind::Bar => "Bar chart",
        ChartKind::StackedBar => "Stacked bar chart",
        ChartKind::Line => "Line chart",
        ChartKind::Dot => "Dot plot with confidence intervals",
        ChartKind::Histogram => "Histogram (epidemic curve)",
    }
}

fn alt_text(spec: &ChartSpec, dec: usize) -> String {
    let mut out = format!("{} titled \"{}\"", kind_name(spec.kind), spec.title.trim());
    let what = spec.y_label.clone().unwrap_or_else(|| "Values".into());
    let by = spec.x_label.clone().unwrap_or_else(|| "category".into());
    let n = spec.categories.len();
    let _ = write!(out, ": {what} by {by}, {n} categories");
    if n > 1 {
        let _ = write!(
            out,
            " from {} to {}",
            spec.categories[0].trim(),
            spec.categories[n - 1].trim()
        );
    }
    out.push('.');
    if spec.series.len() > 1 {
        let names: Vec<&str> = spec.series.iter().map(|s| s.name.as_str()).collect();
        let _ = write!(out, " {} series: {}.", names.len(), names.join(", "));
    }
    match spec.kind {
        ChartKind::StackedBar | ChartKind::Histogram => {
            let totals: Vec<f64> = (0..n)
                .map(|i| spec.series.iter().filter_map(|s| s.values[i]).sum())
                .collect();
            let sum: f64 = totals.iter().sum();
            if let Some((imax, max)) = argmax(&totals) {
                let _ = write!(
                    out,
                    " Total {} across all categories; the highest is {} ({}).",
                    fmt_num(sum, dec),
                    spec.categories[imax].trim(),
                    fmt_num(max, dec)
                );
            }
            if spec.kind == ChartKind::Histogram {
                let first = totals.iter().position(|v| *v > 0.0);
                let last = totals.iter().rposition(|v| *v > 0.0);
                if let (Some(f), Some(l)) = (first, last) {
                    let _ = write!(
                        out,
                        " The first non-zero interval is {} and the last is {}.",
                        spec.categories[f].trim(),
                        spec.categories[l].trim()
                    );
                }
            }
        }
        ChartKind::Line => {
            for s in &spec.series {
                let pts: Vec<(usize, f64)> = s
                    .values
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.map(|v| (i, v)))
                    .collect();
                if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
                    let vals: Vec<f64> = pts.iter().map(|p| p.1).collect();
                    let (imax, max) = argmax(&vals).unwrap_or((0, first.1));
                    let _ = write!(
                        out,
                        " {}: {} in {} to {} in {}",
                        s.name,
                        fmt_num(first.1, dec),
                        spec.categories[first.0].trim(),
                        fmt_num(last.1, dec),
                        spec.categories[last.0].trim()
                    );
                    if first.1 != 0.0 && pts.len() > 1 {
                        let _ = write!(
                            out,
                            " ({:+.0}%)",
                            100.0 * (last.1 - first.1) / first.1.abs()
                        );
                    }
                    let _ = write!(
                        out,
                        ", peak {} in {}.",
                        fmt_num(max, dec),
                        spec.categories[pts[imax].0].trim()
                    );
                }
            }
        }
        ChartKind::Bar | ChartKind::Dot => {
            for s in &spec.series {
                let vals: Vec<f64> = s.values.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
                let finite: Vec<(usize, f64)> = vals
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.is_finite())
                    .map(|(i, v)| (i, *v))
                    .collect();
                if let (Some(hi), Some(lo)) = (
                    finite.iter().max_by(|a, b| a.1.total_cmp(&b.1)),
                    finite.iter().min_by(|a, b| a.1.total_cmp(&b.1)),
                ) {
                    let prefix = if spec.series.len() > 1 {
                        format!(" {}: highest", s.name)
                    } else {
                        " Highest".into()
                    };
                    let _ = write!(
                        out,
                        "{prefix} {} ({}); lowest {} ({}).",
                        spec.categories[hi.0].trim(),
                        fmt_num(hi.1, dec),
                        spec.categories[lo.0].trim(),
                        fmt_num(lo.1, dec)
                    );
                }
            }
        }
    }
    for r in &spec.ref_lines {
        if r.values.len() == 1
            && let Some(v) = r.values[0]
        {
            let _ = write!(out, " Reference line: {} at {}.", r.label, fmt_num(v, dec));
        }
    }
    let suppressed: usize = spec
        .series
        .iter()
        .map(|s| s.suppressed.iter().filter(|x| **x).count())
        .sum();
    if suppressed > 0 {
        let _ = write!(
            out,
            " {suppressed} value(s) are suppressed (small numbers)."
        );
    }
    if !spec.flags.is_empty() {
        let labels: Vec<String> = spec
            .flags
            .iter()
            .take(6)
            .map(|f| {
                format!(
                    "{} ({})",
                    spec.categories
                        .get(f.index)
                        .map(|s| s.trim())
                        .unwrap_or("?"),
                    f.label
                )
            })
            .collect();
        let more = spec.flags.len().saturating_sub(6);
        let _ = write!(
            out,
            " {} point(s) flagged: {}{}.",
            spec.flags.len(),
            labels.join("; "),
            if more > 0 {
                format!("; and {more} more")
            } else {
                String::new()
            }
        );
    }
    out.push_str(" The full data are in the accompanying table.");
    out
}

fn argmax(v: &[f64]) -> Option<(usize, f64)> {
    v.iter()
        .copied()
        .enumerate()
        .filter(|(_, x)| x.is_finite())
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn table(spec: &ChartSpec, dec: usize) -> (Vec<String>, Vec<Vec<String>>) {
    let mut headers = vec![spec.x_label.clone().unwrap_or_else(|| "Category".into())];
    for s in &spec.series {
        headers.push(s.name.clone());
        if s.lower.is_some() || s.upper.is_some() {
            headers.push(format!("{} lower", s.name));
            headers.push(format!("{} upper", s.name));
        }
    }
    for r in &spec.ref_lines {
        if r.values.len() > 1 {
            headers.push(r.label.clone());
        }
    }
    let cell = |v: Option<f64>| v.map(|v| fmt_num(v, dec)).unwrap_or_default();
    let rows = spec
        .categories
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut row = vec![c.clone()];
            for s in &spec.series {
                if s.suppressed.get(i).copied().unwrap_or(false) {
                    row.push("suppressed".into());
                } else {
                    row.push(cell(s.values[i]));
                }
                if s.lower.is_some() || s.upper.is_some() {
                    row.push(cell(s.lower.as_ref().and_then(|l| l[i])));
                    row.push(cell(s.upper.as_ref().and_then(|u| u[i])));
                }
            }
            for r in &spec.ref_lines {
                if r.values.len() > 1 {
                    row.push(cell(r.values[i]));
                }
            }
            row
        })
        .collect();
    (headers, rows)
}

// -------------------------------------------------------------------- layout

/// A value axis (linear or log).
struct Axis {
    min: f64,
    max: f64,
    log: bool,
    ticks: Vec<f64>,
}

impl Axis {
    fn frac(&self, v: f64) -> f64 {
        if self.log {
            (v.ln() - self.min.ln()) / (self.max.ln() - self.min.ln())
        } else {
            (v - self.min) / (self.max - self.min)
        }
    }
}

fn nice_step(range: f64, target: usize) -> f64 {
    let raw = range / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn value_axis(spec: &ChartSpec) -> Axis {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let stacked = matches!(spec.kind, ChartKind::StackedBar | ChartKind::Histogram);
    if stacked {
        for i in 0..spec.categories.len() {
            let t: f64 = spec.series.iter().filter_map(|s| s.values[i]).sum();
            hi = hi.max(t);
        }
        lo = 0.0;
    }
    for v in all_values(spec) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if !lo.is_finite() || !hi.is_finite() {
        lo = 0.0;
        hi = 1.0;
    }
    if spec.log_scale {
        let (lo, hi) = (lo.max(1e-12), hi.max(lo * 1.0001));
        let lmin = 10f64.powf(lo.log10().floor());
        let lmax = 10f64.powf(hi.log10().ceil());
        let mut ticks = Vec::new();
        let mut p = lmin;
        while p <= lmax * 1.0001 {
            for m in [1.0, 2.0, 5.0] {
                let t = p * m;
                if t >= lo * 0.999 && t <= hi * 1.001 {
                    ticks.push(t);
                }
            }
            p *= 10.0;
        }
        if ticks.len() < 2 {
            ticks = vec![lo, hi];
        }
        let min = lo.min(ticks[0]) / 1.15;
        let max = hi.max(*ticks.last().unwrap_or(&hi)) * 1.15;
        return Axis {
            min,
            max,
            log: true,
            ticks,
        };
    }
    // Bars encode length, so their axis always includes zero; use a dot or
    // line chart when a zero baseline is not meaningful.
    let bars = matches!(
        spec.kind,
        ChartKind::Bar | ChartKind::StackedBar | ChartKind::Histogram
    );
    let include_zero = bars || spec.include_zero.unwrap_or(false);
    if include_zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if (hi - lo).abs() < 1e-12 {
        hi = lo + if lo == 0.0 { 1.0 } else { lo.abs() * 0.1 };
    }
    let step = nice_step(hi - lo, 5);
    let min = (lo / step).floor() * step;
    let mut max = (hi / step).ceil() * step;
    if max <= hi && !include_zero {
        max += step;
    }
    let mut ticks = Vec::new();
    let mut t = min;
    while t <= max + step * 1e-6 {
        ticks.push((t / step).round() * step);
        t += step;
    }
    Axis {
        min,
        max,
        log: false,
        ticks,
    }
}

struct Layout {
    w: f32,
    h: f32,
    title_lines: Vec<String>,
    sub_lines: Vec<String>,
    note_lines: Vec<String>,
    legend: Vec<LegendItem>,
    legend_rows: Vec<Vec<usize>>,
    plot: (f32, f32, f32, f32), // x, y, w, h
    axis: Axis,
    tick_dec: usize,
    dec: usize,
    rotate_x: bool,
    x_every: usize,
    show_values: bool,
}

enum LegendKind {
    Swatch(usize),
    Line(usize),
    Ref(bool),
    Flag,
    Suppressed,
    Band,
}

struct LegendItem {
    label: String,
    kind: LegendKind,
}

fn tick_decimals(ticks: &[f64]) -> usize {
    for d in 0..6 {
        let f = 10f64.powi(d as i32);
        if ticks
            .iter()
            .all(|t| ((t * f) - (t * f).round()).abs() < 1e-6)
        {
            return d;
        }
    }
    6
}

impl Layout {
    fn new(spec: &ChartSpec, dec: usize) -> Self {
        let w = spec.width.unwrap_or(800.0).clamp(320.0, 2000.0);
        let h = spec.height.unwrap_or(460.0).clamp(240.0, 2000.0);
        let inner = w - 2.0 * PAD;
        let title_lines = crate::fonts::wrap(spec.title.trim(), true, TITLE, inner);
        let sub_lines = spec
            .subtitle
            .as_deref()
            .map(|s| crate::fonts::wrap(s, false, SUB, inner))
            .unwrap_or_default();
        let note_lines = spec
            .note
            .as_deref()
            .map(|s| crate::fonts::wrap(s, false, TICK, inner))
            .unwrap_or_default();
        let axis = value_axis(spec);
        let tick_dec = tick_decimals(&axis.ticks);

        // Legend items.
        let mut legend = Vec::new();
        let line_like = matches!(spec.kind, ChartKind::Line | ChartKind::Dot);
        if spec.series.len() > 1 {
            for (i, s) in spec.series.iter().enumerate() {
                legend.push(LegendItem {
                    label: s.name.clone(),
                    kind: if line_like {
                        LegendKind::Line(i)
                    } else {
                        LegendKind::Swatch(i)
                    },
                });
            }
        }
        let mut seen_ref = Vec::new();
        for r in &spec.ref_lines {
            if !seen_ref.contains(&r.label) {
                legend.push(LegendItem {
                    label: r.label.clone(),
                    kind: LegendKind::Ref(r.dashed),
                });
                seen_ref.push(r.label.clone());
            }
        }
        if !spec.bands.is_empty() {
            for b in &spec.bands {
                legend.push(LegendItem {
                    label: b.label.clone(),
                    kind: LegendKind::Band,
                });
            }
        }
        if !spec.flags.is_empty() {
            legend.push(LegendItem {
                label: spec.flag_legend.clone().unwrap_or_else(|| "Flagged".into()),
                kind: LegendKind::Flag,
            });
        }
        if spec.series.iter().any(|s| s.suppressed.iter().any(|x| *x)) {
            legend.push(LegendItem {
                label: "Suppressed (small numbers)".into(),
                kind: LegendKind::Suppressed,
            });
        }
        let mut legend_rows: Vec<Vec<usize>> = Vec::new();
        let mut row_w = 0.0;
        for (i, it) in legend.iter().enumerate() {
            let iw = 26.0 + text_width(&it.label, false, LABEL) + 18.0;
            if legend_rows.is_empty() || row_w + iw > inner {
                legend_rows.push(vec![i]);
                row_w = iw;
            } else {
                legend_rows.last_mut().expect("row").push(i);
                row_w += iw;
            }
        }

        // Margins.
        let mut top = PAD
            + title_lines.len() as f32 * TITLE * 1.3
            + sub_lines.len() as f32 * SUB * 1.35
            + 8.0
            + legend_rows.len() as f32 * 20.0
            + if legend_rows.is_empty() { 0.0 } else { 6.0 };
        if !spec.markers.is_empty() {
            top += 16.0;
        }
        let note_h = note_lines.len() as f32 * TICK * 1.35;
        let n = spec.categories.len();

        let (plot_x, plot_w, bottom, rotate_x, x_every);
        if spec.kind == ChartKind::Dot {
            let cat_w = spec
                .categories
                .iter()
                .map(|c| text_width(c, false, TICK))
                .fold(0.0f32, f32::max)
                .min(w * 0.35);
            let left = PAD + cat_w + 10.0;
            let right = PAD + 90.0;
            plot_x = left;
            plot_w = w - left - right;
            bottom = PAD
                + TICK * 1.6
                + if spec.y_label.is_some() {
                    LABEL * 1.6
                } else {
                    0.0
                }
                + note_h;
            rotate_x = false;
            x_every = 1;
        } else {
            let ytw = axis
                .ticks
                .iter()
                .map(|t| text_width(&fmt_num(*t, tick_dec), false, TICK))
                .fold(0.0f32, f32::max);
            let left =
                PAD + if spec.y_label.is_some() {
                    LABEL * 1.6
                } else {
                    0.0
                } + ytw
                    + 8.0;
            let right = if spec.kind == ChartKind::Line {
                let lw = spec
                    .series
                    .iter()
                    .map(|s| text_width(&s.name, false, TICK))
                    .fold(0.0f32, f32::max)
                    .min(150.0);
                PAD + lw + 10.0
            } else {
                PAD + 10.0
            };
            plot_x = left;
            plot_w = w - left - right;
            let band = plot_w / n as f32;
            let max_lab = spec
                .categories
                .iter()
                .map(|c| text_width(c, false, TICK))
                .fold(0.0f32, f32::max);
            rotate_x = max_lab > band * 0.92;
            let slot = if rotate_x { TICK * 1.4 } else { max_lab + 6.0 };
            x_every = ((slot / band).ceil() as usize).max(1);
            let lab_h = if rotate_x {
                (max_lab.min(140.0)) * std::f32::consts::FRAC_1_SQRT_2 + TICK
            } else {
                TICK * 1.4
            };
            bottom = PAD
                + lab_h
                + 6.0
                + if spec.x_label.is_some() {
                    LABEL * 1.6
                } else {
                    0.0
                }
                + note_h;
        }
        let plot_h = (h - top - bottom).max(80.0);
        let n_marks = n * spec.series.len();
        let show_values = spec.value_labels.unwrap_or(match spec.kind {
            ChartKind::Bar => n_marks <= 24 && plot_w / n_marks as f32 >= 22.0,
            ChartKind::StackedBar => n <= 24 && plot_w / n as f32 >= 26.0,
            ChartKind::Dot => true,
            ChartKind::Line | ChartKind::Histogram => false,
        });
        Self {
            w,
            h: top + plot_h + bottom,
            title_lines,
            sub_lines,
            note_lines,
            legend,
            legend_rows,
            plot: (plot_x, top, plot_w, plot_h),
            axis,
            tick_dec,
            dec,
            rotate_x,
            x_every,
            show_values,
        }
    }

    fn vy(&self, v: f64) -> f32 {
        let (_, y, _, h) = self.plot;
        y + h - (self.axis.frac(v) as f32) * h
    }

    fn vx(&self, v: f64) -> f32 {
        let (x, _, w, _) = self.plot;
        x + (self.axis.frac(v) as f32) * w
    }

    fn band(&self, n: usize) -> f32 {
        self.plot.2 / n as f32
    }

    fn cx(&self, pos: f64, n: usize) -> f32 {
        self.plot.0 + (pos as f32 + 0.5) * self.band(n)
    }

    fn draw(&self, spec: &ChartSpec, alt: &str) -> String {
        let mut s = String::with_capacity(16_384);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" role="img" aria-labelledby="chart-title chart-desc" font-family="DejaVu Sans"><title id="chart-title">{t}</title><desc id="chart-desc">{d}</desc>"#,
            w = self.w,
            h = self.h,
            t = esc(spec.title.trim()),
            d = esc(alt)
        );
        let _ = write!(
            s,
            r#"<defs><pattern id="hatch" patternUnits="userSpaceOnUse" width="6" height="6" patternTransform="rotate(45)"><rect width="6" height="6" fill="{bg}"/><line x1="0" y1="0" x2="0" y2="6" stroke="{ink}" stroke-width="1.5"/></pattern></defs><rect width="100%" height="100%" fill="{surface}"/>"#,
            bg = palette::SUPPRESSED,
            ink = palette::INK_MUTED,
            surface = palette::SURFACE
        );
        // Title, subtitle.
        let mut y = PAD;
        for l in &self.title_lines {
            y += TITLE * 1.3;
            text(&mut s, PAD, y - 4.0, l, TITLE, true, palette::INK, "start");
        }
        for l in &self.sub_lines {
            y += SUB * 1.35;
            text(
                &mut s,
                PAD,
                y - 3.0,
                l,
                SUB,
                false,
                palette::INK_SECONDARY,
                "start",
            );
        }
        y += 8.0;
        // Legend.
        for row in &self.legend_rows {
            let mut x = PAD;
            y += 20.0;
            for &i in row {
                let it = &self.legend[i];
                let cy = y - 5.0;
                match it.kind {
                    LegendKind::Swatch(k) => {
                        let _ = write!(
                            s,
                            r#"<rect x="{x:.1}" y="{:.1}" width="14" height="14" rx="2" fill="{}"/>"#,
                            cy - 7.0,
                            palette::categorical(k)
                        );
                    }
                    LegendKind::Line(k) => {
                        let _ = write!(
                            s,
                            r#"<line x1="{x:.1}" y1="{cy:.1}" x2="{:.1}" y2="{cy:.1}" stroke="{}" stroke-width="2"/>"#,
                            x + 20.0,
                            palette::categorical(k)
                        );
                        marker(
                            &mut s,
                            palette::MARKERS[k],
                            x + 10.0,
                            cy,
                            4.0,
                            palette::categorical(k),
                        );
                    }
                    LegendKind::Ref(dashed) => {
                        let _ = write!(
                            s,
                            r#"<line x1="{x:.1}" y1="{cy:.1}" x2="{:.1}" y2="{cy:.1}" stroke="{}" stroke-width="1.5"{}/>"#,
                            x + 20.0,
                            palette::INK_SECONDARY,
                            if dashed {
                                r#" stroke-dasharray="5 3""#
                            } else {
                                ""
                            }
                        );
                    }
                    LegendKind::Flag => {
                        let _ = write!(
                            s,
                            r#"<circle cx="{:.1}" cy="{cy:.1}" r="6" fill="none" stroke="{}" stroke-width="2.5"/>"#,
                            x + 10.0,
                            palette::ALERT
                        );
                    }
                    LegendKind::Suppressed => {
                        let _ = write!(
                            s,
                            r#"<rect x="{x:.1}" y="{:.1}" width="14" height="14" fill="url(#hatch)"/>"#,
                            cy - 7.0
                        );
                    }
                    LegendKind::Band => {
                        let _ = write!(
                            s,
                            r##"<rect x="{x:.1}" y="{:.1}" width="18" height="14" fill="#efeee9" stroke="{}" stroke-width="0.5"/>"##,
                            cy - 7.0,
                            palette::AXIS
                        );
                    }
                }
                text(
                    &mut s,
                    x + 26.0,
                    y - 1.0,
                    &it.label,
                    LABEL,
                    false,
                    palette::INK,
                    "start",
                );
                x += 26.0 + text_width(&it.label, false, LABEL) + 18.0;
            }
        }

        if spec.kind == ChartKind::Dot {
            self.draw_dot(&mut s, spec);
        } else {
            self.draw_categorical(&mut s, spec);
        }

        // Note.
        let mut ny = self.h - PAD - (self.note_lines.len() as f32 - 1.0) * TICK * 1.35;
        for l in &self.note_lines {
            text(
                &mut s,
                PAD,
                ny,
                l,
                TICK,
                false,
                palette::INK_SECONDARY,
                "start",
            );
            ny += TICK * 1.35;
        }
        s.push_str("</svg>");
        s
    }

    fn draw_categorical(&self, s: &mut String, spec: &ChartSpec) {
        // Value labels are drawn last (with a halo) so lines never cover them.
        let mut labels = String::new();
        let (px, py, pw, ph) = self.plot;
        let n = spec.categories.len();
        let bw = self.band(n);
        // Bands and markers behind the data.
        for b in &spec.bands {
            let x0 = px + (b.from.max(0.0) as f32) * bw;
            let x1 = px + (b.to.min(n as f64) as f32) * bw;
            if x1 > x0 {
                let _ = write!(
                    s,
                    r##"<rect x="{x0:.1}" y="{py:.1}" width="{:.1}" height="{ph:.1}" fill="#efeee9"/>"##,
                    x1 - x0
                );
            }
        }
        // Gridlines and value ticks.
        for t in &self.axis.ticks {
            let y = self.vy(*t);
            let _ = write!(
                s,
                r#"<line x1="{px:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="{}" stroke-width="1"/>"#,
                px + pw,
                palette::GRID
            );
            text(
                s,
                px - 6.0,
                y + TICK * 0.35,
                &fmt_num(*t, self.tick_dec),
                TICK,
                false,
                palette::INK_MUTED,
                "end",
            );
        }
        if let Some(l) = &spec.y_label {
            let cx = PAD + LABEL * 0.8;
            let cy = py + ph / 2.0;
            let _ = write!(
                s,
                r#"<text x="{cx:.1}" y="{cy:.1}" font-size="{LABEL}" fill="{}" text-anchor="middle" transform="rotate(-90 {cx:.1} {cy:.1})">{}</text>"#,
                palette::INK_SECONDARY,
                esc(l)
            );
        }
        // Baseline.
        let base_v = if self.axis.min <= 0.0 && self.axis.max >= 0.0 && !self.axis.log {
            0.0
        } else {
            self.axis.min
        };
        let by = self.vy(base_v);

        match spec.kind {
            ChartKind::Bar => {
                let k = spec.series.len();
                let group = bw * if k > 1 { 0.8 } else { 0.7 };
                let sub = group / k as f32;
                for (si, ser) in spec.series.iter().enumerate() {
                    for i in 0..n {
                        let x = px + i as f32 * bw + (bw - group) / 2.0 + si as f32 * sub + 1.0;
                        let wbar = (sub - 2.0).max(1.0);
                        if ser.suppressed.get(i).copied().unwrap_or(false) {
                            // A fixed-height hatched stub: its height carries no value.
                            let h = 10.0;
                            let _ = write!(
                                s,
                                r#"<rect x="{x:.1}" y="{:.1}" width="{wbar:.1}" height="{h:.1}" fill="url(#hatch)"/>"#,
                                by - h
                            );
                            if wbar >= 40.0 {
                                text(
                                    s,
                                    x + wbar / 2.0,
                                    by - h - 4.0,
                                    "suppressed",
                                    TICK,
                                    false,
                                    palette::INK_SECONDARY,
                                    "middle",
                                );
                            }
                            continue;
                        }
                        let Some(v) = ser.values[i] else { continue };
                        let y = self.vy(v);
                        bar(
                            s,
                            x,
                            y.min(by),
                            wbar,
                            (y - by).abs(),
                            palette::categorical(si),
                            v >= base_v,
                        );
                        if self.show_values {
                            let ty = if v >= base_v { y - 4.0 } else { y + TICK + 2.0 };
                            text_halo(
                                &mut labels,
                                x + wbar / 2.0,
                                ty,
                                &fmt_num(v, self.dec),
                                TICK,
                                false,
                                palette::INK,
                                "middle",
                            );
                        }
                    }
                }
            }
            ChartKind::StackedBar | ChartKind::Histogram => {
                let hist = spec.kind == ChartKind::Histogram;
                let wbar = if hist { (bw - 2.0).max(1.0) } else { bw * 0.7 };
                for i in 0..n {
                    let x = px + i as f32 * bw + (bw - wbar) / 2.0;
                    let mut acc = 0.0;
                    let mut any_supp = false;
                    for (si, ser) in spec.series.iter().enumerate() {
                        if ser.suppressed.get(i).copied().unwrap_or(false) {
                            any_supp = true;
                            continue;
                        }
                        let Some(v) = ser.values[i] else { continue };
                        if v <= 0.0 {
                            continue;
                        }
                        let y0 = self.vy(acc);
                        let y1 = self.vy(acc + v);
                        // 2px surface gap between stacked segments.
                        let gap = if acc > 0.0 { 2.0 } else { 0.0 };
                        let top_seg = spec.series[si + 1..]
                            .iter()
                            .all(|o| o.values[i].unwrap_or(0.0) <= 0.0);
                        let hgt = (y0 - y1 - gap).max(0.5);
                        if top_seg {
                            bar(s, x, y1, wbar, hgt, palette::categorical(si), true);
                        } else {
                            let _ = write!(
                                s,
                                r#"<rect x="{x:.1}" y="{y1:.1}" width="{wbar:.1}" height="{hgt:.1}" fill="{}"/>"#,
                                palette::categorical(si)
                            );
                        }
                        acc += v;
                    }
                    if any_supp {
                        let h = 10.0;
                        let _ = write!(
                            s,
                            r#"<rect x="{x:.1}" y="{:.1}" width="{wbar:.1}" height="{h:.1}" fill="url(#hatch)"/>"#,
                            self.vy(acc) - h - if acc > 0.0 { 2.0 } else { 0.0 }
                        );
                    }
                    if self.show_values && acc > 0.0 {
                        text_halo(
                            &mut labels,
                            x + wbar / 2.0,
                            self.vy(acc) - 4.0,
                            &fmt_num(acc, self.dec),
                            TICK,
                            false,
                            palette::INK,
                            "middle",
                        );
                    }
                }
            }
            ChartKind::Line => {
                for (si, ser) in spec.series.iter().enumerate() {
                    let color = palette::categorical(si);
                    // Confidence band.
                    if let (Some(lo), Some(hi)) = (&ser.lower, &ser.upper) {
                        let mut seg: Vec<(f32, f32, f32)> = Vec::new();
                        let flush = |s: &mut String, seg: &mut Vec<(f32, f32, f32)>| {
                            if seg.len() > 1 {
                                let mut d = String::new();
                                for (j, (x, _, yh)) in seg.iter().enumerate() {
                                    let _ = write!(
                                        d,
                                        "{}{x:.1},{yh:.1} ",
                                        if j == 0 { "M" } else { "L" }
                                    );
                                }
                                for (x, yl, _) in seg.iter().rev() {
                                    let _ = write!(d, "L{x:.1},{yl:.1} ");
                                }
                                let _ = write!(
                                    s,
                                    r#"<path d="{d}Z" fill="{color}" fill-opacity="0.15"/>"#
                                );
                            }
                            seg.clear();
                        };
                        for i in 0..n {
                            match (lo[i], hi[i]) {
                                (Some(l), Some(h)) => {
                                    seg.push((self.cx(i as f64, n), self.vy(l), self.vy(h)))
                                }
                                _ => flush(s, &mut seg),
                            }
                        }
                        flush(s, &mut seg);
                    }
                    let mut d = String::new();
                    let mut pen = false;
                    for i in 0..n {
                        match ser.values[i] {
                            Some(v) => {
                                let _ = write!(
                                    d,
                                    "{}{:.1},{:.1} ",
                                    if pen { "L" } else { "M" },
                                    self.cx(i as f64, n),
                                    self.vy(v)
                                );
                                pen = true;
                            }
                            None => pen = false,
                        }
                    }
                    let _ = write!(
                        s,
                        r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="2" stroke-linejoin="round"/>"#
                    );
                    let r = if n > 60 { 2.5 } else { 4.0 };
                    for i in 0..n {
                        if let Some(v) = ser.values[i] {
                            marker(
                                s,
                                palette::MARKERS[si],
                                self.cx(i as f64, n),
                                self.vy(v),
                                r,
                                color,
                            );
                        }
                    }
                    // Direct label at the last point.
                    if let Some(last) = (0..n).rev().find(|i| ser.values[*i].is_some()) {
                        let v = ser.values[last].unwrap_or(0.0);
                        text(
                            s,
                            self.cx(last as f64, n) + 8.0,
                            self.vy(v) + TICK * 0.35,
                            &ser.name,
                            TICK,
                            false,
                            palette::INK,
                            "start",
                        );
                    }
                }
            }
            ChartKind::Dot => {}
        }

        // Reference lines (constant or stepped).
        for r in &spec.ref_lines {
            let dash = if r.dashed {
                r#" stroke-dasharray="5 3""#
            } else {
                ""
            };
            if r.values.len() == 1 {
                if let Some(v) = r.values[0] {
                    let y = self.vy(v);
                    let _ = write!(
                        s,
                        r#"<line x1="{px:.1}" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="{}" stroke-width="1.5"{dash}/>"#,
                        px + pw,
                        palette::INK_SECONDARY
                    );
                }
            } else {
                let mut d = String::new();
                let mut pen = false;
                for i in 0..n {
                    match r.values[i] {
                        Some(v) => {
                            let y = self.vy(v);
                            let x0 = px + i as f32 * bw;
                            let _ = write!(
                                d,
                                "{}{x0:.1},{y:.1} L{:.1},{y:.1} ",
                                if pen { "L" } else { "M" },
                                x0 + bw
                            );
                            pen = true;
                        }
                        None => pen = false,
                    }
                }
                let _ = write!(
                    s,
                    r#"<path d="{d}" fill="none" stroke="{}" stroke-width="1.5"{dash}/>"#,
                    palette::INK_SECONDARY
                );
            }
        }
        // Flags.
        for f in &spec.flags {
            if let Some(Some(v)) = spec
                .series
                .get(f.series)
                .and_then(|s| s.values.get(f.index))
            {
                let x = self.cx(f.index as f64, n);
                let y = if matches!(
                    spec.kind,
                    ChartKind::Bar | ChartKind::StackedBar | ChartKind::Histogram
                ) {
                    self.vy(*v) - 8.0
                } else {
                    self.vy(*v)
                };
                let _ = write!(
                    s,
                    r#"<circle cx="{x:.1}" cy="{y:.1}" r="7" fill="none" stroke="{}" stroke-width="2.5"/>"#,
                    palette::ALERT
                );
            }
        }
        // Vertical markers.
        for m in &spec.markers {
            let x = px + (m.at as f32) * bw;
            let _ = write!(
                s,
                r#"<line x1="{x:.1}" y1="{py:.1}" x2="{x:.1}" y2="{:.1}" stroke="{}" stroke-width="1.5" stroke-dasharray="2 3"/>"#,
                py + ph,
                palette::INK
            );
            text(
                s,
                x + 3.0,
                py - 4.0,
                &m.label,
                TICK,
                false,
                palette::INK,
                "start",
            );
        }
        s.push_str(&labels);
        // Axis line and category labels.
        let _ = write!(
            s,
            r#"<line x1="{px:.1}" y1="{by:.1}" x2="{:.1}" y2="{by:.1}" stroke="{}" stroke-width="1"/>"#,
            px + pw,
            palette::AXIS
        );
        let ly = py + ph + TICK * 1.3;
        for (i, c) in spec.categories.iter().enumerate() {
            if i % self.x_every != 0 {
                continue;
            }
            let x = self.cx(i as f64, n);
            if self.rotate_x {
                let label: String = truncate(c, 140.0);
                let _ = write!(
                    s,
                    r#"<text x="{x:.1}" y="{ly:.1}" font-size="{TICK}" fill="{}" text-anchor="end" transform="rotate(-45 {x:.1} {ly:.1})">{}</text>"#,
                    palette::INK_MUTED,
                    esc(&label)
                );
            } else {
                text(s, x, ly, c, TICK, false, palette::INK_MUTED, "middle");
            }
        }
        if let Some(l) = &spec.x_label {
            let y = self.h - PAD - self.note_lines.len() as f32 * TICK * 1.35 - 4.0;
            text(
                s,
                px + pw / 2.0,
                y,
                l,
                LABEL,
                false,
                palette::INK_SECONDARY,
                "middle",
            );
        }
    }

    fn draw_dot(&self, s: &mut String, spec: &ChartSpec) {
        let (px, py, pw, ph) = self.plot;
        let n = spec.categories.len();
        let row = ph / n as f32;
        let k = spec.series.len();
        for t in &self.axis.ticks {
            let x = self.vx(*t);
            let _ = write!(
                s,
                r#"<line x1="{x:.1}" y1="{py:.1}" x2="{x:.1}" y2="{:.1}" stroke="{}" stroke-width="1"/>"#,
                py + ph,
                palette::GRID
            );
            text(
                s,
                x,
                py + ph + TICK * 1.3,
                &fmt_num(*t, self.tick_dec),
                TICK,
                false,
                palette::INK_MUTED,
                "middle",
            );
        }
        for r in &spec.ref_lines {
            if let Some(Some(v)) = r.values.first() {
                let x = self.vx(*v);
                let dash = if r.dashed {
                    r#" stroke-dasharray="5 3""#
                } else {
                    ""
                };
                let _ = write!(
                    s,
                    r#"<line x1="{x:.1}" y1="{py:.1}" x2="{x:.1}" y2="{:.1}" stroke="{}" stroke-width="1.5"{dash}/>"#,
                    py + ph,
                    palette::INK_SECONDARY
                );
            }
        }
        for (i, c) in spec.categories.iter().enumerate() {
            let cy = py + (i as f32 + 0.5) * row;
            text(
                s,
                px - 10.0,
                cy + TICK * 0.35,
                &truncate(c, self.plot.0 - PAD - 10.0),
                TICK,
                false,
                palette::INK,
                "end",
            );
            for (si, ser) in spec.series.iter().enumerate() {
                let off = if k > 1 {
                    (si as f32 - (k as f32 - 1.0) / 2.0) * (row * 0.6 / k as f32)
                } else {
                    0.0
                };
                let y = cy + off;
                let color = palette::categorical(si);
                if let (Some(Some(l)), Some(Some(h))) = (
                    ser.lower.as_ref().map(|v| v[i]),
                    ser.upper.as_ref().map(|v| v[i]),
                ) {
                    let (x0, x1) = (self.vx(l), self.vx(h));
                    let _ = write!(
                        s,
                        r#"<line x1="{x0:.1}" y1="{y:.1}" x2="{x1:.1}" y2="{y:.1}" stroke="{color}" stroke-width="2"/><line x1="{x0:.1}" y1="{:.1}" x2="{x0:.1}" y2="{:.1}" stroke="{color}" stroke-width="2"/><line x1="{x1:.1}" y1="{:.1}" x2="{x1:.1}" y2="{:.1}" stroke="{color}" stroke-width="2"/>"#,
                        y - 4.0,
                        y + 4.0,
                        y - 4.0,
                        y + 4.0
                    );
                }
                if ser.suppressed.get(i).copied().unwrap_or(false) {
                    text(
                        s,
                        px + 4.0,
                        y + TICK * 0.35,
                        "suppressed",
                        TICK,
                        false,
                        palette::INK_SECONDARY,
                        "start",
                    );
                    continue;
                }
                if let Some(v) = ser.values[i] {
                    let x = self.vx(v);
                    marker(s, palette::MARKERS[si], x, y, 5.0, color);
                    if self.show_values {
                        let mut label = fmt_num(v, self.dec);
                        if let (Some(Some(l)), Some(Some(h))) = (
                            ser.lower.as_ref().map(|v| v[i]),
                            ser.upper.as_ref().map(|v| v[i]),
                        ) {
                            let _ = write!(
                                label,
                                " ({}–{})",
                                fmt_num(l, self.dec),
                                fmt_num(h, self.dec)
                            );
                        }
                        text(
                            s,
                            px + pw + 6.0,
                            y + TICK * 0.35,
                            &label,
                            TICK,
                            false,
                            palette::INK,
                            "start",
                        );
                    }
                }
            }
        }
        if let Some(l) = &spec.y_label {
            let y = self.h - PAD - self.note_lines.len() as f32 * TICK * 1.35 - 4.0;
            text(
                s,
                px + pw / 2.0,
                y,
                l,
                LABEL,
                false,
                palette::INK_SECONDARY,
                "middle",
            );
        }
    }
}

fn truncate(s: &str, max_w: f32) -> String {
    if text_width(s, false, TICK) <= max_w {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if text_width(&format!("{out}{c}…"), false, TICK) > max_w {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Text with a surface-colored halo, for labels drawn over marks and lines.
#[allow(clippy::too_many_arguments)]
fn text_halo(
    s: &mut String,
    x: f32,
    y: f32,
    t: &str,
    size: f32,
    bold: bool,
    fill: &str,
    anchor: &str,
) {
    let _ = write!(
        s,
        r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}"{} fill="{fill}" stroke="{}" stroke-width="3" stroke-linejoin="round" paint-order="stroke" text-anchor="{anchor}">{}</text>"#,
        if bold { r#" font-weight="bold""# } else { "" },
        palette::SURFACE,
        esc(t)
    );
}

#[allow(clippy::too_many_arguments)]
fn text(s: &mut String, x: f32, y: f32, t: &str, size: f32, bold: bool, fill: &str, anchor: &str) {
    let _ = write!(
        s,
        r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}"{} fill="{fill}" text-anchor="{anchor}">{}</text>"#,
        if bold { r#" font-weight="bold""# } else { "" },
        esc(t)
    );
}

/// A bar with 4px rounded data end (the end away from the baseline).
fn bar(s: &mut String, x: f32, y: f32, w: f32, h: f32, fill: &str, up: bool) {
    let r = 4.0f32.min(w / 2.0).min(h);
    if r < 0.5 {
        let _ = write!(
            s,
            r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="{fill}"/>"#
        );
        return;
    }
    let (x1, y1) = (x + w, y + h);
    let d = if up {
        format!(
            "M{x:.1},{y1:.1} L{x:.1},{:.1} Q{x:.1},{y:.1} {:.1},{y:.1} L{:.1},{y:.1} Q{x1:.1},{y:.1} {x1:.1},{:.1} L{x1:.1},{y1:.1} Z",
            y + r,
            x + r,
            x1 - r,
            y + r
        )
    } else {
        format!(
            "M{x:.1},{y:.1} L{x1:.1},{y:.1} L{x1:.1},{:.1} Q{x1:.1},{y1:.1} {:.1},{y1:.1} L{:.1},{y1:.1} Q{x:.1},{y1:.1} {x:.1},{:.1} Z",
            y1 - r,
            x1 - r,
            x + r,
            y1 - r
        )
    };
    let _ = write!(s, r#"<path d="{d}" fill="{fill}"/>"#);
}

fn marker(s: &mut String, m: Marker, x: f32, y: f32, r: f32, fill: &str) {
    // A 2px surface ring keeps overlapping markers distinct.
    let ring = format!(r#" stroke="{}" stroke-width="1.5""#, palette::SURFACE);
    let _ = match m {
        Marker::Circle => write!(
            s,
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" fill="{fill}"{ring}/>"#
        ),
        Marker::Square => write!(
            s,
            r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{fill}"{ring}/>"#,
            x - r * 0.9,
            y - r * 0.9,
            r * 1.8,
            r * 1.8
        ),
        Marker::Triangle => write!(
            s,
            r#"<path d="M{x:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Z" fill="{fill}"{ring}/>"#,
            y - r * 1.1,
            x + r,
            y + r * 0.8,
            x - r,
            y + r * 0.8
        ),
        Marker::Diamond => write!(
            s,
            r#"<path d="M{x:.1},{:.1} L{:.1},{y:.1} L{x:.1},{:.1} L{:.1},{y:.1} Z" fill="{fill}"{ring}/>"#,
            y - r * 1.2,
            x + r * 1.2,
            y + r * 1.2,
            x - r * 1.2
        ),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(name: &str, v: &[f64]) -> Series {
        Series {
            name: name.into(),
            values: v.iter().map(|x| Some(*x)).collect(),
            ..Series::default()
        }
    }

    fn cats(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("Week {i}")).collect()
    }

    #[test]
    fn every_kind_renders_valid_svg_and_png() {
        for kind in [
            ChartKind::Bar,
            ChartKind::StackedBar,
            ChartKind::Line,
            ChartKind::Dot,
            ChartKind::Histogram,
        ] {
            let mut spec = ChartSpec::new(kind, "Pertussis cases <by week>", cats(6));
            spec.series = vec![
                series("Amherst", &[1.0, 3.0, 4.0, 2.0, 5.0, 1.0]),
                series("Hadley", &[2.0, 1.0, 0.0, 3.0, 2.0, 4.0]),
            ];
            if kind == ChartKind::Dot {
                spec.series[0].lower = Some(vec![Some(0.5); 6]);
                spec.series[0].upper = Some(vec![Some(6.0); 6]);
            }
            spec.ref_lines = vec![RefLine {
                label: "Target".into(),
                values: vec![Some(2.5)],
                dashed: true,
            }];
            spec.note = Some("Source: synthetic data".into());
            let r = render(&spec).unwrap();
            assert!(r.svg.starts_with("<svg"));
            assert!(
                r.svg
                    .contains("<title id=\"chart-title\">Pertussis cases &lt;by week&gt;</title>")
            );
            assert!(r.svg.contains("<desc id=\"chart-desc\">"));
            assert!(r.alt_text.contains("full data"));
            assert_eq!(r.table_rows.len(), 6);
            let png = crate::svg::svg_to_png(&r.svg, 1.0).unwrap();
            assert!(crate::image::inspect(&png).is_some(), "{kind:?}");
        }
    }

    #[test]
    fn rejects_bad_specs() {
        let mut spec = ChartSpec::new(ChartKind::Bar, "x", cats(3));
        assert!(render(&spec).is_err()); // no series
        spec.series = vec![series("a", &[1.0, 2.0])];
        assert!(render(&spec).is_err()); // length mismatch
        spec.series = (0..9)
            .map(|i| series(&format!("s{i}"), &[1.0, 2.0, 3.0]))
            .collect();
        assert!(render(&spec).is_err()); // too many series
        let mut log = ChartSpec::new(ChartKind::Dot, "x", cats(2));
        log.log_scale = true;
        log.series = vec![series("a", &[0.0, 2.0])];
        assert!(render(&log).is_err());
    }

    #[test]
    fn suppressed_values_are_hatched_and_tabled() {
        let mut spec = ChartSpec::new(
            ChartKind::Bar,
            "Cases by town",
            vec!["A".into(), "B".into()],
        );
        let mut s = series("Cases", &[12.0, 0.0]);
        s.values[1] = None;
        s.suppressed = vec![false, true];
        spec.series = vec![s];
        let r = render(&spec).unwrap();
        assert!(r.svg.contains("url(#hatch)"));
        assert_eq!(r.table_rows[1][1], "suppressed");
        assert!(r.alt_text.contains("suppressed"));
    }

    #[test]
    fn log_axis_and_flags_and_stepped_limits() {
        let mut spec = ChartSpec::new(
            ChartKind::Dot,
            "Risk ratios",
            vec!["Salad".into(), "Ham".into()],
        );
        spec.log_scale = true;
        let mut s = series("RR", &[4.5, 0.8]);
        s.lower = Some(vec![Some(1.9), Some(0.3)]);
        s.upper = Some(vec![Some(10.2), Some(2.1)]);
        spec.series = vec![s];
        spec.ref_lines = vec![RefLine {
            label: "No association".into(),
            values: vec![Some(1.0)],
            dashed: false,
        }];
        let r = render(&spec).unwrap();
        assert!(r.svg.contains("4.5 (1.9–10.2)"));

        let mut p = ChartSpec::new(ChartKind::Line, "p chart", cats(4));
        p.series = vec![series("Proportion", &[0.1, 0.12, 0.3, 0.11])];
        p.ref_lines = vec![RefLine {
            label: "Upper limit".into(),
            values: vec![Some(0.2), Some(0.21), Some(0.19), Some(0.2)],
            dashed: true,
        }];
        p.flags = vec![Flag {
            series: 0,
            index: 2,
            label: "above upper limit".into(),
        }];
        p.flag_legend = Some("Special cause".into());
        let r = render(&p).unwrap();
        assert!(r.svg.contains(palette::ALERT));
        assert!(r.alt_text.contains("Week 3 (above upper limit)"));
        assert_eq!(r.table_headers.last().unwrap(), "Upper limit");
    }

    #[test]
    fn nice_ticks_and_number_format() {
        assert_eq!(nice_step(97.0, 5), 20.0);
        assert_eq!(nice_step(1.0, 5), 0.2);
        assert_eq!(fmt_num(1234567.891, 1), "1,234,567.9");
        assert_eq!(fmt_num(-0.5, 2), "-0.50");
    }
}

/// Writes sample charts as PNG for visual review:
/// `PHN_CHART_PREVIEW_DIR=/tmp/x cargo test -p phn-docs chart_preview -- --ignored`
#[cfg(test)]
mod preview {
    use super::*;

    #[test]
    #[ignore = "writes preview images for manual review"]
    fn chart_preview() {
        let Ok(dir) = std::env::var("PHN_CHART_PREVIEW_DIR") else {
            return;
        };
        let days: Vec<String> = (1..=21).map(|d| format!("Sep {d}")).collect();
        let mut epi = ChartSpec::new(
            ChartKind::Histogram,
            "Salmonellosis cases by onset date, Town A, September 2026",
            days,
        );
        epi.subtitle = Some("Confirmed and probable cases (n = 38)".into());
        let conf = [
            0., 0., 1., 3., 6., 8., 5., 3., 2., 1., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0.,
        ];
        let prob = [
            0., 0., 0., 1., 2., 2., 1., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.,
        ];
        epi.series = vec![
            Series {
                name: "Confirmed".into(),
                values: conf.iter().map(|v| Some(*v)).collect(),
                ..Series::default()
            },
            Series {
                name: "Probable".into(),
                values: prob.iter().map(|v| Some(*v)).collect(),
                ..Series::default()
            },
        ];
        epi.markers = vec![VMarker {
            at: 2.0,
            label: "Church supper".into(),
        }];
        epi.bands = vec![Band {
            from: 2.25,
            to: 5.0,
            label: "Expected onset (6–72 h)".into(),
        }];
        epi.x_label = Some("Date of symptom onset".into());
        epi.y_label = Some("Number of cases".into());
        epi.note = Some("Source: synthetic line list. Interval: 1 day.".into());
        let mut line = ChartSpec::new(
            ChartKind::Line,
            "Pertussis rate per 100,000, 2016–2025",
            (2016..=2025).map(|y| y.to_string()).collect(),
        );
        line.series = vec![
            Series {
                name: "Town A".into(),
                values: [3.1, 4.0, 2.2, 5.5, 1.0, 0.5, 2.0, 6.1, 9.8, 7.2]
                    .iter()
                    .map(|v| Some(*v))
                    .collect(),
                ..Series::default()
            },
            Series {
                name: "Massachusetts".into(),
                values: [2.5, 3.0, 2.8, 3.3, 0.8, 0.4, 1.1, 4.2, 6.9, 5.0]
                    .iter()
                    .map(|v| Some(*v))
                    .collect(),
                ..Series::default()
            },
        ];
        line.y_label = Some("Rate per 100,000".into());
        let mut dot = ChartSpec::new(
            ChartKind::Dot,
            "Food exposures and illness: risk ratios",
            vec![
                "Potato salad".into(),
                "Ham".into(),
                "Green beans".into(),
                "Rolls".into(),
            ],
        );
        dot.log_scale = true;
        dot.series = vec![Series {
            name: "Risk ratio".into(),
            values: vec![Some(4.5), Some(1.1), Some(0.8), Some(1.3)],
            lower: Some(vec![Some(1.98), Some(0.6), Some(0.35), Some(0.7)]),
            upper: Some(vec![Some(10.2), Some(2.0), Some(1.8), Some(2.4)]),
            suppressed: vec![],
        }];
        dot.ref_lines = vec![RefLine {
            label: "No association (1.0)".into(),
            values: vec![Some(1.0)],
            dashed: false,
        }];
        dot.y_label = Some("Risk ratio (log scale) with 95% CI".into());
        let mut bar = ChartSpec::new(
            ChartKind::Bar,
            "Kindergarten MMR coverage by school",
            vec![
                "Fort River".into(),
                "Crocker Farm".into(),
                "Wildwood".into(),
                "Pelham".into(),
                "Hadley Elementary".into(),
            ],
        );
        bar.series = vec![Series {
            name: "Coverage (%)".into(),
            values: vec![Some(96.2), Some(91.5), Some(97.8), None, Some(94.0)],
            suppressed: vec![false, false, false, true, false],
            ..Series::default()
        }];
        bar.ref_lines = vec![RefLine {
            label: "95% threshold".into(),
            values: vec![Some(95.0)],
            dashed: true,
        }];
        for (name, spec) in [("epi", epi), ("line", line), ("dot", dot), ("bar", bar)] {
            let r = render(&spec).unwrap();
            std::fs::write(
                format!("{dir}/{name}.png"),
                crate::svg::svg_to_png(&r.svg, 1.0).unwrap(),
            )
            .unwrap();
        }
    }
}
