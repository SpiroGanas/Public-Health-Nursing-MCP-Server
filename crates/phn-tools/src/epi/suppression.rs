//! EPI-06 `apply_small_cell_suppression`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::suppression::{CellStatus, SuppressionOptions, suppress};
use phn_local::phi;
use phn_local::table::Table;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// A table passed inline.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct InlineTable {
    /// Column headers, including label columns.
    pub columns: Vec<String>,
    /// Rows; label cells as text, counts as numbers or numeric text.
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// Arguments for `apply_small_cell_suppression`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SuppressionArgs {
    /// A table of counts passed inline. Give either `table` or `path`.
    pub table: Option<InlineTable>,
    /// A CSV/XLSX table of counts in /data.
    pub path: Option<String>,
    pub sheet: Option<String>,
    /// Number of leading label columns (default: detected).
    pub label_columns: Option<usize>,
    /// Threshold: counts from 1 to threshold−1 are suppressed (default: your configured threshold).
    pub threshold: Option<u32>,
    /// Also suppress zero counts (default false).
    pub suppress_zeros: bool,
    /// Apply complementary suppression (default true).
    pub complementary: Option<bool>,
    /// The last column holds row totals (default: detected from a "Total" header).
    pub last_column_is_total: Option<bool>,
    /// The last row holds column totals (default: detected from a "Total" label).
    pub last_row_is_total: Option<bool>,
    /// Marker shown for suppressed cells (default "*").
    pub marker: Option<String>,
    /// Write the suppressed table as CSV to the output folder (default true for files).
    pub write_output: Option<bool>,
}

/// Result of `apply_small_cell_suppression`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuppressionResult {
    pub columns: Vec<String>,
    /// The table safe to share: suppressed cells replaced by the marker.
    pub rows: Vec<Vec<String>>,
    /// Status of each count cell (label columns excluded).
    pub status: Vec<Vec<CellStatus>>,
    pub threshold: u32,
    pub primary_suppressed: usize,
    pub complementary_suppressed: usize,
    pub footnote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
}

fn cell_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn parse_count(s: &str) -> std::result::Result<Option<u64>, String> {
    let t = s.trim().replace(',', "");
    if t.is_empty()
        || phi::is_missing(&t)
        || matches!(t.as_str(), "*" | "**" | "†")
        || t.starts_with('<')
    {
        return Ok(None);
    }
    match t.parse::<f64>() {
        Ok(f) if f >= 0.0 && f.fract() == 0.0 => Ok(Some(f as u64)),
        _ => Err(s.to_string()),
    }
}

fn is_total(s: &str) -> bool {
    let n = s.trim().to_ascii_lowercase();
    n == "total" || n == "totals" || n.starts_with("total ") || n == "all" || n.ends_with(" total")
}

/// Suppress small counts (with complementary suppression) so a table is safe to share.
pub fn apply_small_cell_suppression(
    ctx: &Context,
    args: SuppressionArgs,
) -> Result<Envelope<SuppressionResult>> {
    let (table, source_label) = match (&args.table, &args.path) {
        (Some(t), None) => (
            Table {
                name: "table".into(),
                headers: t.columns.clone(),
                rows: t
                    .rows
                    .iter()
                    .map(|r| r.iter().map(cell_text).collect())
                    .collect(),
                truncated: false,
            },
            None,
        ),
        (None, Some(p)) => {
            let (t, label) = super::super::core::files::load_table(ctx, p, args.sheet.as_deref())?;
            (t, Some(label))
        }
        _ => {
            return Err(PhnError::InvalidInput(
                "give either `table` or `path` (not both)".into(),
            ));
        }
    };
    if table.rows.is_empty() {
        return Err(PhnError::InvalidInput("the table has no rows".into()));
    }
    if table.rows.len() > 5_000 {
        return Err(PhnError::InvalidInput("this looks like record-level data (over 5,000 rows); summarize it first with profile_local_dataset".into()));
    }
    // Guard: refuse record-level data.
    let direct: Vec<String> = phi::scan_table(&table)
        .into_iter()
        .filter(|f| f.kind.is_direct() && f.kind != phi::Identifier::FreeText)
        .map(|f| format!("{} ({})", f.column, f.kind.label()))
        .collect();
    if !direct.is_empty() {
        return Err(PhnError::Privacy(format!(
            "this looks like record-level data with identifiers ({}). Suppression applies to tables \
             of counts; use profile_local_dataset to count first.",
            direct.join(", ")
        )));
    }

    let ncols = table.headers.len();
    let label_cols = args.label_columns.unwrap_or_else(|| {
        // Leading columns where any cell is non-numeric are labels.
        (0..ncols)
            .take_while(|&c| table.column(c).any(|v| parse_count(v).is_err()))
            .count()
    });
    if label_cols >= ncols {
        return Err(PhnError::InvalidInput(
            "no count columns found; counts must be whole numbers".into(),
        ));
    }
    let mut cells = Vec::with_capacity(table.rows.len());
    let mut bad = Vec::new();
    for (r, row) in table.rows.iter().enumerate() {
        let mut out = Vec::new();
        for c in label_cols..ncols {
            let v = row.get(c).map(String::as_str).unwrap_or("");
            match parse_count(v) {
                Ok(n) => out.push(n),
                Err(s) => {
                    bad.push(format!(
                        "row {} column '{}': '{s}'",
                        r + 1,
                        table.headers[c]
                    ));
                    out.push(None);
                }
            }
        }
        cells.push(out);
    }
    if !bad.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "count cells must be whole, non-negative numbers (suppress counts before computing rates): {}",
            bad.into_iter().take(5).collect::<Vec<_>>().join("; ")
        )));
    }
    let last_col_total = args
        .last_column_is_total
        .unwrap_or_else(|| table.headers.last().is_some_and(|h| is_total(h)));
    let last_row_total = args.last_row_is_total.unwrap_or_else(|| {
        table
            .rows
            .last()
            .is_some_and(|r| r.iter().take(label_cols.max(1)).any(|c| is_total(c)))
    });
    let threshold = match args.threshold {
        Some(t) => phn_core::config::validate_threshold(t)?,
        None => ctx.suppression_threshold(),
    };
    let opts = SuppressionOptions {
        threshold,
        suppress_zeros: args.suppress_zeros,
        complementary: args.complementary.unwrap_or(true),
        last_column_is_total: last_col_total,
        last_row_is_total: last_row_total,
    };
    let res = suppress(&cells, &opts);
    let marker = args.marker.clone().unwrap_or_else(|| "*".into());
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            let mut out: Vec<String> = row.iter().take(label_cols).cloned().collect();
            for c in 0..(ncols - label_cols) {
                out.push(if res.is_hidden(r, c) {
                    marker.clone()
                } else {
                    cells[r][c].map(|v| v.to_string()).unwrap_or_default()
                });
            }
            out
        })
        .collect();
    let footnote = format!(
        "{marker} Suppressed: counts from 1 to {} are not shown{}, and additional cells are \
         suppressed so hidden values cannot be calculated from totals.",
        threshold - 1,
        if args.suppress_zeros {
            " (zeros are also suppressed)"
        } else {
            ""
        }
    );

    let mut output_file = None;
    let mut privacy = Privacy {
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    };
    if args.write_output.unwrap_or(source_label.is_some()) {
        let sandbox = ctx.sandbox()?;
        let stem = source_label
            .as_deref()
            .and_then(|l| l.rsplit('/').next())
            .and_then(|n| n.rsplit_once('.').map(|(s, _)| s.to_string()))
            .unwrap_or_else(|| "table".into());
        let path = sandbox.new_output_file(&format!("{stem}_suppressed.csv"))?;
        let t = phn_docs::model::TableBlock {
            caption: None,
            headers: table.headers.clone(),
            rows: rows.clone(),
        };
        let mut bytes = phn_docs::csv_out::table_to_csv(&t)?;
        bytes.extend_from_slice(format!("\n\"{}\"\n", footnote.replace('"', "'")).as_bytes());
        std::fs::write(&path, bytes)?;
        let shown = sandbox.display_path(&path);
        privacy.files_written.push(shown.clone());
        output_file = Some(shown);
    }

    let mut summary = format!(
        "**Suppressed table** (threshold {threshold}: {} primary, {} complementary cell(s) hidden)\n\n",
        res.primary, res.complementary
    );
    summary.push_str(&md::table(
        &table.headers.iter().map(String::as_str).collect::<Vec<_>>(),
        &rows,
    ));
    summary.push_str(&format!("\n{footnote}\n"));
    if let Some(f) = &output_file {
        summary.push_str(&format!("\nWritten to /data/{f}.\n"));
    }
    let mut caveats = vec![
        "Complementary suppression is a heuristic that guarantees no row or column has a single \
         hidden cell; check that hidden values cannot be derived from other published tables."
            .to_string(),
    ];
    if !last_col_total && !last_row_total && opts.complementary {
        caveats.push(
            "No total row or column was detected; complementary suppression assumed totals may be \
             published elsewhere."
                .into(),
        );
    }
    let mut sources = vec![Source::computed(
        "Primary and complementary small-cell suppression",
        "PHN MCP server calculation",
    )];
    if let Some(l) = &source_label {
        sources.push(Source::local_file(l.clone()));
    }
    Ok(Envelope::new(
        summary,
        SuppressionResult {
            columns: table.headers.clone(),
            rows,
            status: res.status.clone(),
            threshold,
            primary_suppressed: res.primary,
            complementary_suppressed: res.complementary,
            footnote,
            output_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}
