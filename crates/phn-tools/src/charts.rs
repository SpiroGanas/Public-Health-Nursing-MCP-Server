//! Writing charts to the output folder.
//!
//! Shared by `create_chart`, `create_spc_chart`, `analyze_trends`, and the
//! communicable-disease tools (epidemic curves). Charts are drawn by
//! [`phn_docs::chart`], saved as SVG and/or PNG under `/data/output/charts/`,
//! and described by generated alt text plus a data table, so the nurse can
//! embed them with `export_document` (`![alt text](output/charts/x.png)`).

use phn_core::{PhnError, Result};
use phn_docs::chart::{ChartSpec, Rendered, render};
use phn_docs::svg::svg_to_png;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Image formats to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChartFormat {
    /// PNG (for Word, PowerPoint, email) and SVG (sharp at any size, for the web).
    #[default]
    Both,
    Png,
    Svg,
}

/// Files written for one chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChartFiles {
    /// PNG path relative to /data, when written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png: Option<String>,
    /// SVG path relative to /data, when written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg: Option<String>,
    /// Alt text describing the chart (use it when embedding the image).
    pub alt_text: String,
    /// Markdown to embed the chart with `export_document`.
    pub embed_markdown: String,
}

/// Render `spec` and write it to `/data/output/charts/<stem>.{png,svg}`.
/// Returns the file paths and the rendered chart (alt text, data table).
pub fn write_chart(
    ctx: &Context,
    spec: &ChartSpec,
    stem: &str,
    format: ChartFormat,
) -> Result<(ChartFiles, Rendered)> {
    let rendered = render(spec)?;
    let sandbox = ctx.sandbox().map_err(|e| {
        PhnError::Config(format!(
            "charts are saved to the output folder, which needs the data folder mounted at /data \
             ({e})"
        ))
    })?;
    let dir = sandbox.output_dir()?.join("charts");
    std::fs::create_dir_all(&dir)?;
    let stem = phn_local::sandbox::sanitize_file_name(stem);
    let pick = |ext: &str| -> Result<std::path::PathBuf> {
        let candidate = dir.join(format!("{stem}.{ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
        for n in 2..10_000 {
            let p = dir.join(format!("{stem}-{n}.{ext}"));
            if !p.exists() {
                return Ok(p);
            }
        }
        Err(PhnError::Internal(
            "too many charts with the same name".into(),
        ))
    };
    let mut files = ChartFiles {
        png: None,
        svg: None,
        alt_text: rendered.alt_text.clone(),
        embed_markdown: String::new(),
    };
    if matches!(format, ChartFormat::Both | ChartFormat::Png) {
        let p = pick("png")?;
        std::fs::write(&p, svg_to_png(&rendered.svg, 2.0)?)?;
        files.png = Some(sandbox.display_path(&p));
    }
    if matches!(format, ChartFormat::Both | ChartFormat::Svg) {
        let p = pick("svg")?;
        std::fs::write(&p, &rendered.svg)?;
        files.svg = Some(sandbox.display_path(&p));
    }
    let target = files.png.clone().or(files.svg.clone()).unwrap_or_default();
    files.embed_markdown = format!("![{}]({target})", rendered.alt_text.replace(['[', ']'], ""));
    Ok((files, rendered))
}

/// The chart's data as a markdown table (limited to `max_rows`).
pub fn data_table_markdown(r: &Rendered, max_rows: usize) -> String {
    let headers: Vec<&str> = r.table_headers.iter().map(String::as_str).collect();
    let rows: Vec<Vec<String>> = r.table_rows.iter().take(max_rows).cloned().collect();
    let mut out = md::table(&headers, &rows);
    if r.table_rows.len() > max_rows {
        out.push_str(&format!(
            "\n_{} more rows are in the chart's data (structured content)._\n",
            r.table_rows.len() - max_rows
        ));
    }
    out
}
