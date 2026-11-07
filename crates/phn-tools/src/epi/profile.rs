//! EPI-07 `profile_local_dataset`.

use phn_core::{Envelope, Privacy, Result, Source};
use phn_local::profile::{DatasetProfile, ProfileOptions, profile};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Arguments for `profile_local_dataset`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ProfileArgs {
    /// CSV or spreadsheet in /data.
    pub path: String,
    /// Sheet name (default: first sheet with data).
    pub sheet: Option<String>,
    /// Add numeric summaries and suppressed frequency tables for columns that
    /// are not identifiers. Default false: structure and identifier warnings only.
    pub include_value_summaries: bool,
    /// Categories to list per column before grouping the rest (default 15).
    pub max_categories: Option<usize>,
}

/// Result of `profile_local_dataset`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileResult {
    pub file: String,
    pub sheet: String,
    pub profile: DatasetProfile,
}

/// Describe a local dataset: variables, types, missingness, and possible identifiers.
pub fn profile_local_dataset(ctx: &Context, args: ProfileArgs) -> Result<Envelope<ProfileResult>> {
    let (table, label) = crate::core::files::load_table(ctx, &args.path, args.sheet.as_deref())?;
    let threshold = ctx.suppression_threshold();
    let p = profile(
        &table,
        &ProfileOptions {
            include_value_summaries: args.include_value_summaries,
            suppression_threshold: threshold,
            max_categories: args.max_categories.unwrap_or(15).clamp(3, 50),
        },
    );
    let mut summary = format!(
        "**{label}** (sheet \"{}\"): {} rows × {} columns, {} duplicate row(s)\n\n",
        table.name,
        md::num(p.rows as f64, 0),
        p.columns,
        p.duplicate_rows
    );
    if !p.identifier_columns.is_empty() {
        summary.push_str(&format!(
            "⚠ **Possible identifiers in {} column(s)** — values from these columns are never shown:\n",
            p.identifier_columns.len()
        ));
        for f in &p.identifier_columns {
            summary.push_str(&format!(
                "- {}: {} ({:?} confidence; {})\n",
                f.column,
                f.kind.label(),
                f.confidence,
                f.evidence
            ));
        }
        summary.push('\n');
    }
    let rows: Vec<Vec<String>> = p
        .column_profiles
        .iter()
        .map(|c| {
            let mut detail = Vec::new();
            if let Some(n) = &c.numeric {
                detail.push(format!(
                    "mean {:.2}, median {}{}",
                    n.mean,
                    n.median,
                    match (&n.min, &n.max) {
                        (Some(a), Some(b)) => format!(", range {a}–{b}"),
                        _ => String::new(),
                    }
                ));
                if n.outliers > 0 {
                    detail.push(format!("{} outlier(s)", n.outliers));
                }
            }
            if let Some((a, b)) = &c.date_range {
                detail.push(format!("{a} to {b}"));
            }
            if let Some(f) = &c.frequencies {
                detail.push(
                    f.iter()
                        .take(6)
                        .map(|x| format!("{}: {}", x.value, x.display))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            if let Some(w) = &c.values_withheld {
                detail.push(format!("values withheld: {w}"));
            }
            vec![
                c.name.clone(),
                format!("{:?}", c.inferred_type).to_lowercase(),
                format!("{}%", c.missing_pct),
                c.distinct.to_string(),
                c.possible_identifier
                    .as_ref()
                    .map(|f| f.kind.label().to_string())
                    .unwrap_or_default(),
                detail.join("; "),
            ]
        })
        .collect();
    summary.push_str(&md::table(
        &[
            "Column",
            "Type",
            "Missing",
            "Distinct",
            "Identifier?",
            "Summary",
        ],
        &rows,
    ));
    if !args.include_value_summaries {
        summary.push_str("\nValue summaries are off. Ask again with include_value_summaries for suppressed counts and statistics of non-identifier columns.\n");
    }
    let mut caveats = Vec::new();
    if p.truncated {
        caveats.push(
            "The file has more than 1,000,000 rows; only the first 1,000,000 were profiled.".into(),
        );
    }
    if p.suppressed_cells > 0 {
        caveats.push(format!(
            "{} frequency cell(s) are suppressed (counts below {threshold}).",
            p.suppressed_cells
        ));
    }
    caveats.push("Identifier detection is automated and can miss identifiers; review before sharing any output.".into());
    let privacy = Privacy {
        suppression_applied: args.include_value_summaries,
        suppression_threshold: Some(threshold),
        identifiers_detected: {
            let mut k: Vec<String> = p
                .identifier_columns
                .iter()
                .map(|f| f.kind.label().to_string())
                .collect();
            k.sort();
            k.dedup();
            k
        },
        ..Privacy::none()
    };
    Ok(Envelope::new(
        summary,
        ProfileResult {
            file: label.clone(),
            sheet: table.name.clone(),
            profile: p,
        },
    )
    .source(Source::local_file(label))
    .caveats(caveats)
    .privacy(privacy))
}
