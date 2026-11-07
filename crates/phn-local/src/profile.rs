//! Dataset profiling that never shows identifier values.
//!
//! By default only structure is reported: column names, inferred types,
//! missingness, distinct counts, and possible identifiers. With value
//! summaries turned on, numeric statistics and frequency tables are added for
//! non-identifier columns, and every frequency table passes through
//! small-cell suppression.

use std::collections::{BTreeMap, HashMap, HashSet};

use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::phi::{self, ColumnFinding, Identifier, is_missing, parse_date};
use crate::table::Table;

/// Inferred column type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    Empty,
    Integer,
    Decimal,
    Boolean,
    Date,
    Categorical,
    Text,
}

/// Numeric summary (values above 89 in age columns are shown as "90+").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NumericSummary {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub p25: f64,
    pub median: f64,
    pub p75: f64,
    /// Smallest value (omitted for identifier columns).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// Largest value (omitted for identifier columns).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
    /// Values outside 1.5 × IQR (count only).
    pub outliers: usize,
}

/// One row of a frequency table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Frequency {
    pub value: String,
    /// `None` when suppressed.
    pub count: Option<u64>,
    /// Count as displayed ("12" or "*").
    pub display: String,
}

/// Profile of one column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColumnProfile {
    pub name: String,
    pub inferred_type: ColumnType,
    pub non_missing: usize,
    pub missing: usize,
    pub missing_pct: f64,
    pub distinct: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub possible_identifier: Option<ColumnFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric: Option<NumericSummary>,
    /// Earliest and latest month (YYYY-MM) for date columns other than dates of birth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_range: Option<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequencies: Option<Vec<Frequency>>,
    /// Why values are not shown, when they are not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values_withheld: Option<String>,
}

/// Profile of a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DatasetProfile {
    pub rows: usize,
    pub columns: usize,
    pub duplicate_rows: usize,
    pub truncated: bool,
    pub column_profiles: Vec<ColumnProfile>,
    pub identifier_columns: Vec<ColumnFinding>,
    /// Number of frequency cells hidden by suppression.
    pub suppressed_cells: usize,
}

/// Options for [`profile`].
#[derive(Debug, Clone)]
pub struct ProfileOptions {
    pub include_value_summaries: bool,
    pub suppression_threshold: u32,
    pub max_categories: usize,
}

impl Default for ProfileOptions {
    fn default() -> Self {
        Self {
            include_value_summaries: false,
            suppression_threshold: 5,
            max_categories: 15,
        }
    }
}

fn infer_type(values: &[&str]) -> ColumnType {
    if values.is_empty() {
        return ColumnType::Empty;
    }
    if values.iter().all(|v| v.parse::<i64>().is_ok()) {
        let distinct: HashSet<&str> = values.iter().copied().collect();
        if distinct.len() <= 2
            && distinct.iter().all(|v| *v == "0" || *v == "1")
            && values.len() > 2
        {
            return ColumnType::Boolean;
        }
        return ColumnType::Integer;
    }
    if values
        .iter()
        .all(|v| v.replace(',', "").parse::<f64>().is_ok())
    {
        return ColumnType::Decimal;
    }
    let lower: HashSet<String> = values.iter().map(|v| v.to_ascii_lowercase()).collect();
    if lower.iter().all(|v| {
        matches!(
            v.as_str(),
            "yes" | "no" | "y" | "n" | "true" | "false" | "t" | "f"
        )
    }) {
        return ColumnType::Boolean;
    }
    let dates = values.iter().filter(|v| parse_date(v).is_some()).count();
    if dates as f64 >= 0.9 * values.len() as f64 {
        return ColumnType::Date;
    }
    let distinct = lower.len();
    if distinct <= 20 || (distinct as f64) <= 0.05 * values.len() as f64 {
        ColumnType::Categorical
    } else {
        ColumnType::Text
    }
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 {
        format!("{}", x as i64)
    } else {
        format!("{x:.2}")
    }
}

/// Profile a table.
pub fn profile(table: &Table, opts: &ProfileOptions) -> DatasetProfile {
    let findings = phi::scan_table(table);
    let by_index: HashMap<usize, &ColumnFinding> = findings.iter().map(|f| (f.index, f)).collect();
    let mut suppressed_cells = 0;
    let mut column_profiles = Vec::new();

    for (i, name) in table.headers.iter().enumerate() {
        let all: Vec<&str> = table.column(i).map(str::trim).collect();
        let values: Vec<&str> = all.iter().copied().filter(|v| !is_missing(v)).collect();
        let missing = all.len() - values.len();
        let distinct = values.iter().collect::<HashSet<_>>().len();
        let inferred_type = infer_type(&values);
        let finding = by_index.get(&i).map(|f| (*f).clone());
        let kind = finding.as_ref().map(|f| f.kind);
        let mut cp = ColumnProfile {
            name: name.clone(),
            inferred_type,
            non_missing: values.len(),
            missing,
            missing_pct: if all.is_empty() {
                0.0
            } else {
                (missing as f64 * 1000.0 / all.len() as f64).round() / 10.0
            },
            distinct,
            possible_identifier: finding,
            numeric: None,
            date_range: None,
            frequencies: None,
            values_withheld: None,
        };

        if !opts.include_value_summaries {
            column_profiles.push(cp);
            continue;
        }
        // Which summaries are safe for this column?
        let show_frequencies = matches!(
            kind,
            None | Some(Identifier::Geography) | Some(Identifier::ZipCode)
        );
        let show_numeric = matches!(kind, None | Some(Identifier::Age));
        match kind {
            Some(k) if !show_frequencies && !show_numeric && k != Identifier::Date => {
                cp.values_withheld = Some(format!("possible identifier ({})", k.label()));
            }
            _ => {}
        }

        if show_numeric && matches!(inferred_type, ColumnType::Integer | ColumnType::Decimal) {
            let mut nums: Vec<f64> = values
                .iter()
                .filter_map(|v| v.replace(',', "").parse::<f64>().ok())
                .collect();
            nums.sort_by(f64::total_cmp);
            if !nums.is_empty() {
                let n = nums.len();
                let mean = nums.iter().sum::<f64>() / n as f64;
                let sd = if n > 1 {
                    (nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
                } else {
                    0.0
                };
                let (p25, median, p75) = (
                    quantile(&nums, 0.25),
                    quantile(&nums, 0.5),
                    quantile(&nums, 0.75),
                );
                let iqr = p75 - p25;
                let outliers = nums
                    .iter()
                    .filter(|&&x| x < p25 - 1.5 * iqr || x > p75 + 1.5 * iqr)
                    .count();
                let is_age = kind == Some(Identifier::Age);
                let cap = |x: f64| {
                    if is_age && x > 89.0 {
                        "90+".to_string()
                    } else {
                        fmt_num(x)
                    }
                };
                // Extremes describe single records; hide them when few records.
                let show_extremes = n >= opts.suppression_threshold as usize;
                cp.numeric = Some(NumericSummary {
                    n,
                    mean: (mean * 100.0).round() / 100.0,
                    sd: (sd * 100.0).round() / 100.0,
                    p25,
                    median,
                    p75,
                    min: show_extremes.then(|| cap(nums[0])),
                    max: show_extremes.then(|| cap(nums[n - 1])),
                    outliers,
                });
            }
        }

        if kind == Some(Identifier::Date) || (kind.is_none() && inferred_type == ColumnType::Date) {
            let mut months: Vec<String> = values
                .iter()
                .filter_map(|v| parse_date(v))
                .map(|d| d.format("%Y-%m").to_string())
                .collect();
            months.sort();
            if let (Some(a), Some(b)) = (months.first(), months.last()) {
                cp.date_range = Some((a.clone(), b.clone()));
            }
        }

        if show_frequencies
            && matches!(inferred_type, ColumnType::Categorical | ColumnType::Boolean)
            || (show_frequencies
                && matches!(
                    kind,
                    Some(Identifier::Geography) | Some(Identifier::ZipCode)
                ))
        {
            let mut counts: BTreeMap<String, u64> = BTreeMap::new();
            for v in &values {
                *counts.entry(v.to_string()).or_default() += 1;
            }
            let mut sorted: Vec<(String, u64)> = counts.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            // Fold the tail into "Other" so the table stays readable.
            let mut shown: Vec<(String, u64)> =
                sorted.iter().take(opts.max_categories).cloned().collect();
            let other: u64 = sorted.iter().skip(opts.max_categories).map(|x| x.1).sum();
            if other > 0 {
                shown.push(("(all other values)".into(), other));
            }
            if missing > 0 {
                shown.push(("(missing)".into(), missing as u64));
            }
            let total: u64 = shown.iter().map(|x| x.1).sum();
            let mut cells: Vec<Vec<Option<u64>>> = shown.iter().map(|x| vec![Some(x.1)]).collect();
            cells.push(vec![Some(total)]);
            let res = suppress(
                &cells,
                &SuppressionOptions {
                    threshold: opts.suppression_threshold,
                    last_row_is_total: true,
                    ..SuppressionOptions::default()
                },
            );
            suppressed_cells += res.primary + res.complementary;
            cp.frequencies = Some(
                shown
                    .into_iter()
                    .enumerate()
                    .map(|(r, (value, count))| {
                        let hidden = res.is_hidden(r, 0);
                        Frequency {
                            value,
                            count: (!hidden).then_some(count),
                            display: if hidden {
                                "*".into()
                            } else {
                                count.to_string()
                            },
                        }
                    })
                    .collect(),
            );
        }
        column_profiles.push(cp);
    }

    let mut seen = HashSet::new();
    let duplicate_rows = table.rows.iter().filter(|r| !seen.insert(*r)).count();
    DatasetProfile {
        rows: table.rows.len(),
        columns: table.headers.len(),
        duplicate_rows,
        truncated: table.truncated,
        column_profiles,
        identifier_columns: findings,
        suppressed_cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_list() -> Table {
        let mut rows = Vec::new();
        let towns = [
            "Amherst", "Amherst", "Amherst", "Amherst", "Amherst", "Amherst", "Hadley", "Pelham",
            "Amherst", "Amherst",
        ];
        for (i, town) in towns.iter().enumerate() {
            rows.push(vec![
                format!("Person{i} Smith"),
                town.to_string(),
                if i == 9 {
                    "95".into()
                } else {
                    format!("{}", 10 + i)
                },
                "Pertussis".to_string(),
                format!("2024-01-{:02}", i + 1),
            ]);
        }
        Table {
            name: "ll".into(),
            headers: vec![
                "Name".into(),
                "Town".into(),
                "Age".into(),
                "Dx".into(),
                "Onset date".into(),
            ],
            rows,
            truncated: false,
        }
    }

    #[test]
    fn structure_only_by_default() {
        let p = profile(&line_list(), &ProfileOptions::default());
        assert_eq!(p.rows, 10);
        assert!(
            p.column_profiles
                .iter()
                .all(|c| c.frequencies.is_none() && c.numeric.is_none())
        );
        assert!(
            p.identifier_columns
                .iter()
                .any(|f| f.kind == Identifier::Name)
        );
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("Person1"));
    }

    #[test]
    fn value_summaries_suppress_and_cap() {
        let p = profile(
            &line_list(),
            &ProfileOptions {
                include_value_summaries: true,
                ..ProfileOptions::default()
            },
        );
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("Person1"), "names leaked");
        let town = p.column_profiles.iter().find(|c| c.name == "Town").unwrap();
        let freqs = town.frequencies.as_ref().unwrap();
        let hadley = freqs.iter().find(|f| f.value == "Hadley").unwrap();
        assert_eq!(hadley.display, "*");
        let total_hidden = freqs.iter().filter(|f| f.count.is_none()).count();
        assert!(total_hidden >= 2, "complementary suppression expected");
        let age = p.column_profiles.iter().find(|c| c.name == "Age").unwrap();
        assert_eq!(age.numeric.as_ref().unwrap().max.as_deref(), Some("90+"));
        let onset = p
            .column_profiles
            .iter()
            .find(|c| c.name == "Onset date")
            .unwrap();
        assert_eq!(onset.date_range, Some(("2024-01".into(), "2024-01".into())));
        let name = p.column_profiles.iter().find(|c| c.name == "Name").unwrap();
        assert!(name.values_withheld.is_some());
    }
}
