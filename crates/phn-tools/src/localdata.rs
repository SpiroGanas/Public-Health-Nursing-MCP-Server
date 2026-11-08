//! Shared guardrails for tools that analyze the nurse's own CSV/XLSX files.
//!
//! * Columns are found by name (case-insensitive) with suggestions.
//! * Columns the PHI detector flags as identifiers are refused, except ages,
//!   dates, and geography (town, ZIP), which may be summarized in aggregate.
//! * Category values are checked: more than [`MAX_LEVELS`] distinct values,
//!   or values that look like identifiers, are refused (free text or IDs).
//! * Results report aggregates only; tools must not return individual values
//!   (no minimum or maximum, no lists of rows).

use phn_core::{PhnError, Result, Source};
use phn_local::phi::{self, ColumnFinding, Identifier};
use phn_local::table::Table;

use crate::context::Context;

/// Most distinct values a category variable may have.
pub const MAX_LEVELS: usize = 20;

/// A loaded local table with its identifier findings.
pub struct LocalTable {
    pub table: Table,
    /// Path relative to /data.
    pub label: String,
    pub findings: Vec<ColumnFinding>,
}

/// Identifier kinds that may be analyzed in aggregate.
fn aggregate_ok(kind: Identifier) -> bool {
    matches!(
        kind,
        Identifier::Age | Identifier::Date | Identifier::Geography | Identifier::ZipCode
    )
}

/// Load a CSV/XLSX table from /data and scan it for identifiers.
pub fn load(ctx: &Context, path: &str, sheet: Option<&str>) -> Result<LocalTable> {
    let (table, label) = crate::core::files::load_table(ctx, path, sheet)?;
    if table.rows.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' has no data rows"
        )));
    }
    let findings = phi::scan_table(&table);
    Ok(LocalTable {
        table,
        label,
        findings,
    })
}

impl LocalTable {
    /// Citation for the file.
    pub fn source(&self) -> Source {
        Source::local_file(&self.label)
    }

    /// Identifier categories found anywhere in the file (for the privacy block).
    pub fn identifier_categories(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .findings
            .iter()
            .map(|f| f.kind.label().to_string())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Find a column by name and refuse identifier columns.
    pub fn column(&self, name: &str) -> Result<usize> {
        let idx = self.table.column_index(name).ok_or_else(|| {
            let names: Vec<&str> = self
                .table
                .headers
                .iter()
                .filter(|h| {
                    !self
                        .findings
                        .iter()
                        .any(|f| &f.column == *h && !aggregate_ok(f.kind))
                })
                .map(String::as_str)
                .collect();
            PhnError::InvalidInput(format!(
                "'{}' has no column named '{name}'. Columns you can use: {}",
                self.label,
                names.join(", ")
            ))
        })?;
        if let Some(f) = self
            .findings
            .iter()
            .find(|f| f.index == idx && !aggregate_ok(f.kind))
        {
            return Err(PhnError::Privacy(format!(
                "column '{}' appears to contain {} and cannot be analyzed; use a non-identifying \
                 variable, or de-identify the file first (deidentify_dataset)",
                f.column,
                f.kind.label()
            )));
        }
        Ok(idx)
    }

    /// Numeric values of a column (`None` for missing or non-numeric cells),
    /// and the share of non-missing cells that parsed as numbers.
    pub fn numeric(&self, idx: usize) -> (Vec<Option<f64>>, f64) {
        let mut present = 0usize;
        let mut ok = 0usize;
        let v = self
            .table
            .column(idx)
            .map(|s| {
                if phi::is_missing(s) {
                    return None;
                }
                present += 1;
                let x = s
                    .trim()
                    .trim_end_matches('%')
                    .replace(',', "")
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite());
                if x.is_some() {
                    ok += 1;
                }
                x
            })
            .collect();
        (
            v,
            if present == 0 {
                0.0
            } else {
                ok as f64 / present as f64
            },
        )
    }

    /// Category values of a column (trimmed; `None` for missing), refusing
    /// columns with too many levels or identifier-like values.
    pub fn categorical(&self, idx: usize) -> Result<Vec<Option<String>>> {
        let vals: Vec<Option<String>> = self
            .table
            .column(idx)
            .map(|s| (!phi::is_missing(s)).then(|| s.trim().to_string()))
            .collect();
        let mut levels: Vec<&String> = vals.iter().flatten().collect();
        levels.sort();
        levels.dedup();
        let name = &self.table.headers[idx];
        if levels.len() > MAX_LEVELS {
            return Err(PhnError::InvalidInput(format!(
                "column '{name}' has {} different values; a category variable may have at most \
                 {MAX_LEVELS} (free text and ID columns cannot be used as categories)",
                levels.len()
            )));
        }
        if levels.iter().any(|l| !phi::scan_text(l).is_empty()) {
            return Err(PhnError::Privacy(format!(
                "values in column '{name}' look like identifiers, so they cannot be shown as \
                 categories"
            )));
        }
        Ok(vals)
    }
}

/// True/false reading of yes/no style cells.
pub fn truthy(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" | "true" | "1" | "x" | "positive" | "pos" | "+" => Some(true),
        "n" | "no" | "false" | "0" | "negative" | "neg" | "-" => Some(false),
        _ => None,
    }
}
