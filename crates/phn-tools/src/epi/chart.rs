//! EPI-11 `create_chart`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::chart::{ChartKind, ChartSpec, RefLine, Series, VMarker};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::charts::{ChartFiles, ChartFormat, data_table_markdown, write_chart};
use crate::context::Context;

/// Chart types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChartTypeArg {
    /// Vertical bars (several series are grouped).
    Bar,
    /// Stacked bars (parts of a total).
    StackedBar,
    /// Lines for trends over time.
    Line,
    /// Point estimates with confidence intervals (rates, ratios by group).
    Dot,
    /// Contiguous bars for counts by time interval (epidemic-curve style).
    Histogram,
}

/// One series.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SeriesArg {
    /// Series name (shown in the legend).
    pub name: String,
    /// One value per category; null for missing.
    pub values: Vec<Option<f64>>,
    /// Lower confidence limits (dot and line charts).
    #[serde(default)]
    pub lower: Option<Vec<Option<f64>>>,
    /// Upper confidence limits.
    #[serde(default)]
    pub upper: Option<Vec<Option<f64>>>,
    /// True for values hidden by small-cell suppression (drawn hatched, never plotted as a value).
    #[serde(default)]
    pub suppressed: Option<Vec<bool>>,
}

/// A horizontal reference line (target, benchmark, state rate).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RefLineArg {
    pub label: String,
    pub value: f64,
    /// Dashed (default true).
    #[serde(default)]
    pub dashed: Option<bool>,
}

/// A vertical marker at a category (e.g., "policy started").
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MarkerArg {
    /// Category label the marker sits before.
    pub at_category: String,
    pub label: String,
}

/// Arguments for `create_chart`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateChartArgs {
    pub chart_type: ChartTypeArg,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Category labels (x axis; rows for dot charts).
    pub categories: Vec<String>,
    /// One to eight series (fold extras into "Other").
    pub series: Vec<SeriesArg>,
    #[serde(default)]
    pub x_label: Option<String>,
    #[serde(default)]
    pub y_label: Option<String>,
    /// Log scale for ratios (dot and line charts).
    #[serde(default)]
    pub log_scale: Option<bool>,
    /// Include zero on the value axis (always on for bars).
    #[serde(default)]
    pub include_zero: Option<bool>,
    /// Decimal places for labels and the table.
    #[serde(default)]
    pub decimals: Option<usize>,
    #[serde(default)]
    pub reference_lines: Option<Vec<RefLineArg>>,
    #[serde(default)]
    pub markers: Option<Vec<MarkerArg>>,
    /// Source line under the chart, e.g. "Source: MDPH MAVEN, 2025".
    #[serde(default)]
    pub source_note: Option<String>,
    /// Treat values as counts and suppress those from 1 to threshold − 1.
    #[serde(default)]
    pub suppress_small_counts: Option<bool>,
    /// File name without extension (default from the title).
    #[serde(default)]
    pub file_name: Option<String>,
    /// png, svg, or both (default both).
    #[serde(default)]
    pub format: Option<ChartFormat>,
    /// Size in points (default 800 × 460).
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
}

/// Result of `create_chart`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateChartResult {
    pub files: ChartFiles,
    /// The data drawn, as a table (headers then rows).
    pub table_headers: Vec<String>,
    pub table_rows: Vec<Vec<String>>,
    /// Values hidden by suppression in this call.
    pub suppressed_values: usize,
}

/// Make an accessible chart image in the output folder.
pub fn create_chart(ctx: &Context, args: CreateChartArgs) -> Result<Envelope<CreateChartResult>> {
    if args.title.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "give the chart a title (it becomes its accessible name)".into(),
        ));
    }
    let threshold = ctx.suppression_threshold();
    let suppress = args.suppress_small_counts.unwrap_or(false);
    let kind = match args.chart_type {
        ChartTypeArg::Bar => ChartKind::Bar,
        ChartTypeArg::StackedBar => ChartKind::StackedBar,
        ChartTypeArg::Line => ChartKind::Line,
        ChartTypeArg::Dot => ChartKind::Dot,
        ChartTypeArg::Histogram => ChartKind::Histogram,
    };
    let mut spec = ChartSpec::new(kind, args.title.clone(), args.categories.clone());
    spec.subtitle = args.subtitle.clone();
    spec.x_label = args.x_label.clone();
    spec.y_label = args.y_label.clone();
    spec.log_scale = args.log_scale.unwrap_or(false);
    spec.include_zero = args.include_zero;
    spec.decimals = args.decimals.map(|d| d.min(6));
    spec.note = args.source_note.clone();
    spec.width = args.width;
    spec.height = args.height;
    let mut suppressed_values = 0;
    let mut small_unsuppressed = 0;
    for s in &args.series {
        let mut flags = s.suppressed.clone().unwrap_or_default();
        flags.resize(s.values.len(), false);
        let mut values = s.values.clone();
        for (i, v) in values.iter_mut().enumerate() {
            if let Some(x) = *v {
                let small = x > 0.0 && x < f64::from(threshold) && (x - x.round()).abs() < 1e-9;
                if suppress && small {
                    flags[i] = true;
                } else if small {
                    small_unsuppressed += 1;
                }
            }
            if flags[i] {
                *v = None;
            }
        }
        suppressed_values += flags.iter().filter(|f| **f).count();
        spec.series.push(Series {
            name: s.name.clone(),
            values,
            lower: s.lower.clone(),
            upper: s.upper.clone(),
            suppressed: flags,
        });
    }
    spec.ref_lines = args
        .reference_lines
        .unwrap_or_default()
        .into_iter()
        .map(|r| RefLine {
            label: r.label,
            values: vec![Some(r.value)],
            dashed: r.dashed.unwrap_or(true),
        })
        .collect();
    for m in args.markers.unwrap_or_default() {
        let at = args
            .categories
            .iter()
            .position(|c| c.trim().eq_ignore_ascii_case(m.at_category.trim()))
            .ok_or_else(|| {
                PhnError::InvalidInput(format!(
                    "marker category '{}' is not one of the categories",
                    m.at_category
                ))
            })?;
        spec.markers.push(VMarker {
            at: at as f64,
            label: m.label,
        });
    }
    let stem = args
        .file_name
        .clone()
        .unwrap_or_else(|| args.title.chars().take(60).collect());
    let (files, rendered) = write_chart(ctx, &spec, &stem, args.format.unwrap_or_default())?;

    let mut summary = format!(
        "**Chart saved:** {}\n\n",
        files
            .png
            .iter()
            .chain(files.svg.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );
    summary.push_str(&format!("**Alt text:** {}\n\n", files.alt_text));
    summary.push_str(&format!(
        "Embed in a document with `export_document`: `{}`\n\n",
        files.embed_markdown
    ));
    summary.push_str("**Data shown**\n\n");
    summary.push_str(&data_table_markdown(&rendered, 40));

    let mut caveats = vec![
        "Colors follow a color-vision-safe order and every series also has a marker shape or a \
         direct label; keep the data table with the chart in reports (WCAG 1.1.1, 1.4.1)."
            .to_string(),
    ];
    if small_unsuppressed > 0 {
        caveats.push(format!(
            "{small_unsuppressed} whole-number value(s) are below your suppression threshold \
             ({threshold}). If they are counts from local data, redraw with \
             suppress_small_counts = true before sharing."
        ));
    }
    Ok(Envelope::new(
        summary,
        CreateChartResult {
            files: files.clone(),
            table_headers: rendered.table_headers,
            table_rows: rendered.table_rows,
            suppressed_values,
        },
    )
    .source(Source::computed(
        "Chart rendered by the server (accessible SVG/PNG with generated alt text)",
        "PHN MCP server",
    ))
    .caveats(caveats)
    .privacy(Privacy {
        suppression_applied: suppressed_values > 0,
        suppression_threshold: Some(threshold),
        notes: vec![
            "The chart contains only the values supplied in the request; it was saved to the \
             output folder."
                .into(),
        ],
        ..Privacy::none()
    }))
}
