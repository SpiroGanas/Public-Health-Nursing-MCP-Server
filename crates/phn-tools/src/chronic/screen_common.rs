//! Helpers shared by the Phase 15 screening tools (CHR-02 blood pressure screening kit and
//! CHR-03 diabetes prevention finder): small-cell suppression of one-way count tables from a
//! local file, age groups, and the column finder.

use std::collections::BTreeMap;

use phn_core::{PhnError, Result};
use phn_epi::suppression::{SuppressionOptions, suppress};
use phn_local::phi::{Identifier, is_missing};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Largest age (years) accepted from a local file or an argument.
pub const MAX_AGE: f64 = 120.0;

/// One row of a suppressed count table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CountRow {
    pub label: String,
    /// The count, or "*" when it is suppressed.
    pub count: String,
}

/// A one-way table of counts from a local file with small cells suppressed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CountTable {
    pub title: String,
    pub rows: Vec<CountRow>,
    /// All rows in the table ("*" when suppressed).
    pub total: String,
    /// True when at least one count is hidden ("*").
    pub suppressed: bool,
}

/// Build a suppressed one-way table. Counts from 1 to `threshold - 1` are hidden, and
/// further cells are hidden so that no hidden count can be recovered from the total.
pub fn count_table(title: &str, counts: &[(String, u64)], threshold: u32) -> CountTable {
    let total: u64 = counts.iter().map(|(_, n)| *n).sum();
    let mut matrix: Vec<Vec<Option<u64>>> = counts.iter().map(|(_, n)| vec![Some(*n)]).collect();
    matrix.push(vec![Some(total)]);
    let sup = suppress(
        &matrix,
        &SuppressionOptions {
            threshold,
            last_row_is_total: true,
            ..SuppressionOptions::default()
        },
    );
    let show = |r: usize| -> String {
        if sup.is_hidden(r, 0) {
            "*".into()
        } else {
            matrix[r][0].unwrap_or(0).to_string()
        }
    };
    let rows: Vec<CountRow> = counts
        .iter()
        .enumerate()
        .map(|(i, (label, _))| CountRow {
            label: label.clone(),
            count: show(i),
        })
        .collect();
    let total_shown = show(counts.len());
    let suppressed = rows.iter().any(|r| r.count == "*") || total_shown == "*";
    CountTable {
        title: title.to_string(),
        rows,
        total: total_shown,
        suppressed,
    }
}

/// Counts of labels in a fixed order (labels never seen are listed with zero).
pub fn tally(order: &[&str], seen: &BTreeMap<String, u64>) -> Vec<(String, u64)> {
    order
        .iter()
        .map(|l| ((*l).to_string(), seen.get(*l).copied().unwrap_or(0)))
        .collect()
}

/// Age group label for an age in years (adults are grouped by decade).
pub fn age_group(age: f64) -> &'static str {
    match age {
        a if a < 18.0 => "Under 18",
        a if a < 30.0 => "18 to 29",
        a if a < 40.0 => "30 to 39",
        a if a < 50.0 => "40 to 49",
        a if a < 60.0 => "50 to 59",
        a if a < 70.0 => "60 to 69",
        a if a < 80.0 => "70 to 79",
        _ => "80 and older",
    }
}

/// Age group labels in display order, then the label for a missing or invalid age.
pub const AGE_GROUPS: [&str; 8] = [
    "Under 18",
    "18 to 29",
    "30 to 39",
    "40 to 49",
    "50 to 59",
    "60 to 69",
    "70 to 79",
    "80 and older",
];
pub const AGE_MISSING: &str = "Age not recorded";

/// The yes/no answers of a column, as `Some(true)` or `Some(false)` (`None` for a missing cell).
///
/// The identifier guardrail still applies, with one exception: a header such as "Family
/// history" is flagged as free text by name alone, so such a column is accepted only when every
/// answer in it is a yes or no (anything else is refused without echoing a value).
pub fn yes_no_column(
    lt: &crate::localdata::LocalTable,
    idx: usize,
) -> Result<Vec<Option<bool>>> {
    let name = &lt.table.headers[idx];
    if let Some(f) = lt.findings.iter().find(|f| f.index == idx) {
        let aggregate_ok = matches!(
            f.kind,
            Identifier::Age | Identifier::Date | Identifier::Geography | Identifier::ZipCode
        );
        if f.kind != Identifier::FreeText && !aggregate_ok {
            return Err(PhnError::Privacy(format!(
                "column '{name}' appears to contain {} and cannot be analyzed; use a \
                 non-identifying variable, or de-identify the file first (deidentify_dataset)",
                f.kind.label()
            )));
        }
    }
    let mut out = Vec::new();
    for cell in lt.table.column(idx) {
        if is_missing(cell) {
            out.push(None);
            continue;
        }
        match crate::localdata::truthy(cell) {
            Some(b) => out.push(Some(b)),
            None => {
                return Err(PhnError::Privacy(format!(
                    "column '{name}' has answers other than yes or no, so it cannot be used (free \
                     text is never analyzed)"
                )));
            }
        }
    }
    Ok(out)
}

/// Index of the first header that matches one of the aliases (case, accents, punctuation, and
/// spacing ignored).
pub fn find_column(headers: &[String], aliases: &[&str]) -> Option<usize> {
    let norm: Vec<String> = headers
        .iter()
        .map(|h| phn_core::text::normalize(h))
        .collect();
    aliases
        .iter()
        .find_map(|a| norm.iter().position(|h| *h == phn_core::text::normalize(a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_counts_are_hidden_and_cannot_be_recovered_from_the_total() {
        let t = count_table(
            "x",
            &[("a".into(), 20), ("b".into(), 2), ("c".into(), 9)],
            5,
        );
        assert_eq!(t.rows[1].count, "*");
        // A second cell is hidden too, so the hidden pair cannot be solved.
        assert!(t.rows.iter().filter(|r| r.count == "*").count() >= 2);
        assert!(t.suppressed);
    }

    #[test]
    fn large_counts_are_shown() {
        let t = count_table("x", &[("a".into(), 20), ("b".into(), 12)], 5);
        assert_eq!(t.rows[0].count, "20");
        assert_eq!(t.total, "32");
        assert!(!t.suppressed);
    }

    #[test]
    fn a_tiny_total_is_hidden() {
        let t = count_table("x", &[("a".into(), 3)], 5);
        assert_eq!(t.rows[0].count, "*");
        assert_eq!(t.total, "*");
    }

    #[test]
    fn age_groups_follow_decades() {
        assert_eq!(age_group(17.9), "Under 18");
        assert_eq!(age_group(18.0), "18 to 29");
        assert_eq!(age_group(59.0), "50 to 59");
        assert_eq!(age_group(95.0), "80 and older");
    }
}
