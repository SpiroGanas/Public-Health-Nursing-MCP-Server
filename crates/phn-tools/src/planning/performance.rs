//! PLAN-11 `track_program_performance`: compares a program's outputs and outcomes by period
//! (a CSV/XLSX in /data) with the targets the nurse gives, computes progress to target and pace,
//! flags trends with run-chart and p-chart rules, and writes a dashboard report, a workbook with
//! live formulas, and a chart per measure to /data/output.
//!
//! * The file is read through `localdata`: identifier columns are refused, and only the period,
//!   measure, value, and denominator columns the nurse names are read. Everything else in the
//!   file (including contact people) is never read or returned.
//! * The response holds aggregates by period, with small-cell suppression: a period whose count
//!   (or numerator, denominator, or the part of a percentage that is not the numerator) is from
//!   1 to threshold − 1 is hidden, and then the totals, progress, and center line that would
//!   reveal it are withheld too. The report, workbook, and charts in /data/output hold the full
//!   values from the nurse's own file.
//! * A file with one row per person can be counted by period (`count_rows`); dates are rolled up
//!   to months (or quarters or weeks) and no row is ever returned.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike as _, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::chart::{ChartKind, ChartSpec, Flag, RefLine, Series};
use phn_epi::spc::{self, ChartType, Input};
use rust_xlsxwriter::{Format, Formula, Workbook, XlsxError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::textmatch::words;
use crate::charts::{ChartFiles, ChartFormat, write_chart};
use crate::context::Context;
use crate::ebp::spc::Period;
use crate::localdata::{self, LocalTable};
use crate::md;
use crate::school::templates::{self, DocFormat};

const MAX_PERIODS: usize = 100;
const MAX_MEASURES: usize = 12;
/// Rows of the workbook's Data sheet that its formulas cover.
const DATA_LAST_ROW: u32 = 101;
/// Fewest periods for which trend rules are applied.
const MIN_TREND_POINTS: usize = 6;
/// Fewest periods for a p chart (fewer get a run chart).
const MIN_P_POINTS: usize = 8;
const DEFAULT_ON_TRACK: f64 = 0.9;

// ---------------------------------------------------------------- arguments

/// Which direction is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// A higher value is better (clients served, percent screened). Default.
    #[default]
    HigherIsBetter,
    /// A lower value is better (no-show rate, days to follow-up).
    LowerIsBetter,
}

/// What a measure counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeasureKind {
    /// A count per period (clients served, sessions held). Default.
    #[default]
    Count,
    /// A percentage: numerator divided by a denominator, times 100 (needs a denominator).
    Percent,
}

/// What the target applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// The target is for all periods together: a count's total, or a percentage pooled over all
    /// periods. Default.
    #[default]
    Cumulative,
    /// The target applies to each period; progress is judged on the latest period.
    EachPeriod,
}

/// One measure and its target.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TargetArg {
    /// The measure. Wide layout: a column name in the file. Long layout (`measure_column`
    /// given): a value in that column. With `count_rows`: "Records".
    pub measure: String,
    /// The target number (a count, or a percentage such as 80 for 80%).
    pub target: Option<f64>,
    /// higher_is_better (default) or lower_is_better.
    pub direction: Option<Direction>,
    /// count (default) or percent.
    pub kind: Option<MeasureKind>,
    /// For percent measures in the wide layout: the column holding the denominator. (In the
    /// long layout use `denominator_column`.)
    pub denominator: Option<String>,
    /// cumulative (default) or each_period.
    pub basis: Option<Basis>,
    /// Starting value before the program (optional). With it, progress for a percentage is the
    /// share of the distance from the baseline to the target.
    pub baseline: Option<f64>,
    /// Unit shown in the report, for example "clients" or "%".
    pub unit: Option<String>,
}

/// Arguments for `track_program_performance`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PerformanceArgs {
    /// CSV or XLSX in /data with the program's results by period (one row per period, or one
    /// row per period and measure), or one row per record when you use `count_rows`.
    pub path: String,
    /// Sheet name for an XLSX file (default: the first sheet).
    pub sheet: Option<String>,
    /// Column with the period: a date, "2025-Q1", "Jan 2025", "FY26 Q2", or any label. Dates are
    /// rolled up to `date_period`.
    pub period_column: String,
    /// Long layout: the column that names the measure in each row. Leave out for the wide layout
    /// (one column per measure).
    pub measure_column: Option<String>,
    /// Long layout: the column with each row's value (a count, or a numerator for a percentage).
    /// Without it in the long layout, rows are counted per period and measure.
    pub value_column: Option<String>,
    /// Long layout, percent measures: the column with each row's denominator.
    pub denominator_column: Option<String>,
    /// Count the rows in each period as a measure called "Records" (for a file with one row per
    /// service or person). Only counts are returned.
    pub count_rows: Option<bool>,
    /// The measures and their targets (at least one; up to 12).
    pub targets: Vec<TargetArg>,
    /// Program name for the report title.
    pub program_name: Option<String>,
    /// How many periods the program or grant year has in all (for example 4 quarters or 12
    /// months). With it, a cumulative count is judged against the pace needed to reach the
    /// target by the end.
    pub periods_in_program: Option<u32>,
    /// A cumulative count is "on track" when it is at least this share of the pace needed
    /// (default 0.9). A convention of this tool: use your funder's rule if it has one.
    pub on_track_fraction: Option<f64>,
    /// When the period column holds dates: month (default), quarter, or week.
    pub date_period: Option<Period>,
    /// Write the dashboard report (docx by default, or pdf) to /data/output (default true).
    pub write_report: Option<bool>,
    /// Write the workbook with live formulas to /data/output (default true).
    pub write_workbook: Option<bool>,
    /// Write one chart per measure (PNG) to /data/output/charts (default true).
    pub write_charts: Option<bool>,
    /// docx (default) or pdf, for the report.
    pub format: Option<DocFormat>,
}

// ---------------------------------------------------------------- results

/// One period of one measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PeriodRow {
    pub period: String,
    /// The period's value (count, or percentage); null when missing or suppressed.
    pub value: Option<f64>,
    /// Percent measures: the numerator (null when suppressed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numerator: Option<f64>,
    /// Percent measures: the denominator (null when suppressed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denominator: Option<f64>,
    /// True when a small count was hidden in this period.
    pub suppressed: bool,
    /// True when the period had no data in the file.
    pub missing: bool,
    /// Trend rules signaled at this period.
    pub signals: Vec<String>,
}

/// Trend rules for a measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrendInfo {
    /// "run chart", "p chart", or "not assessed".
    pub chart: String,
    /// Periods used (with data).
    pub points: usize,
    /// Plain-language signals with their periods.
    pub signals: Vec<String>,
    /// Center line (the median for a run chart); omitted when a small count is suppressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<f64>,
    /// "above", "below", or "on" the center line for the latest period; omitted when withheld.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_vs_center: Option<String>,
    pub notes: Vec<String>,
}

/// Results for one measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasureResult {
    pub measure: String,
    pub kind: MeasureKind,
    pub direction: Direction,
    pub basis: Basis,
    pub unit: Option<String>,
    pub target: Option<f64>,
    pub baseline: Option<f64>,
    pub periods: Vec<PeriodRow>,
    /// Periods with data.
    pub periods_reported: usize,
    /// Cumulative total (count) or pooled percentage; null when withheld.
    pub current: Option<f64>,
    /// Latest period's value; null when suppressed.
    pub latest: Option<f64>,
    /// Progress to target in percent (see the method); null when not computable or withheld.
    pub progress_percent: Option<f64>,
    /// Cumulative count: the part of the target expected by now at an even pace.
    pub expected_by_now: Option<f64>,
    /// Plain status: Met, On track, Behind, Within limit, Over limit, Below target, Above
    /// target, Not yet met, or a note that a small count is suppressed.
    pub status: String,
    /// Each-period basis: periods that met the target (null when withheld).
    pub periods_meeting_target: Option<usize>,
    /// True when a small count in this measure is suppressed, so totals and progress are
    /// withheld from this response (the report and workbook have them).
    pub withheld: bool,
    pub trend: TrendInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<ChartFiles>,
}

/// Result of `track_program_performance`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PerformanceResult {
    pub program: Option<String>,
    /// Layout read: "wide", "long", or "records counted".
    pub layout: String,
    /// First and last period label.
    pub first_period: String,
    pub last_period: String,
    pub periods: usize,
    pub measures: Vec<MeasureResult>,
    pub periods_in_program: Option<u32>,
    pub on_track_fraction: f64,
    /// Suppression threshold applied.
    pub suppression_threshold: u32,
    /// Report (docx or pdf) in the output folder.
    pub report_file: Option<String>,
    /// Workbook with live formulas in the output folder.
    pub workbook_file: Option<String>,
}

// ---------------------------------------------------------------- reading the file

#[derive(Debug, Clone)]
struct Spec {
    measure: String,
    target: Option<f64>,
    direction: Direction,
    kind: MeasureKind,
    denominator: Option<String>,
    basis: Basis,
    baseline: Option<f64>,
    unit: Option<String>,
}

/// Values of one measure by period index.
#[derive(Debug, Clone, Default)]
struct Series2 {
    num: Vec<Option<f64>>,
    den: Vec<Option<f64>>,
}

struct Loaded {
    periods: Vec<String>,
    series: BTreeMap<String, Series2>,
    layout: &'static str,
    local: LocalTable,
}

fn date_key(d: NaiveDate, p: Period) -> (i32, u32) {
    match p {
        Period::Week => {
            let w = d.iso_week();
            (w.year(), w.week())
        }
        Period::Month => (d.year(), d.month()),
        Period::Quarter => (d.year(), (d.month() - 1) / 3 + 1),
    }
}

fn date_label(k: (i32, u32), p: Period) -> String {
    match p {
        Period::Week => format!("{}-W{:02}", k.0, k.1),
        Period::Month => format!("{}-{:02}", k.0, k.1),
        Period::Quarter => format!("{} Q{}", k.0, k.1),
    }
}

fn next_key(k: (i32, u32), p: Period) -> (i32, u32) {
    let max = match p {
        Period::Week => NaiveDate::from_ymd_opt(k.0, 12, 28).map_or(52, |d| d.iso_week().week()),
        Period::Month => 12,
        Period::Quarter => 4,
    };
    if k.1 >= max { (k.0 + 1, 1) } else { (k.0, k.1 + 1) }
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// A sort key for a text period label such as "2025-Q1", "Q2 FY26", "Jan 2025", or "2025-03".
pub(crate) fn text_period_key(label: &str) -> Option<(i32, u32)> {
    let toks = words(label);
    let mut year: Option<i32> = None;
    let mut ord: Option<u32> = None;
    for t in &toks {
        let t = t.as_str();
        if let Some(rest) = t.strip_prefix("fy") {
            if let Ok(n) = rest.parse::<i32>() {
                year = Some(if n < 100 { 2000 + n } else { n });
                continue;
            }
        }
        if let Ok(n) = t.parse::<i32>() {
            if (1900..=2100).contains(&n) {
                year = Some(n);
                continue;
            }
            if (1..=12).contains(&n) && ord.is_none() {
                ord = Some(n as u32);
                continue;
            }
        }
        if t.len() == 2 && t.starts_with('q') {
            if let Ok(q) = t[1..].parse::<u32>() {
                if (1..=4).contains(&q) {
                    ord = Some(q);
                    continue;
                }
            }
        }
        if t.len() >= 3
            && let Some(i) = MONTHS.iter().position(|m| t.starts_with(m))
        {
            ord = Some(i as u32 + 1);
        }
    }
    // A "2025-03" label has no separator-free year token issue: words() splits on '-'.
    Some((year?, ord.unwrap_or(0)))
}

fn numeric_column(lt: &LocalTable, name: &str) -> Result<Vec<Option<f64>>> {
    let idx = lt.column(name)?;
    let (v, share) = lt.numeric(idx);
    if share < 0.8 {
        return Err(PhnError::InvalidInput(format!(
            "column '{name}' is not numeric (only {:.0}% of its values are numbers)",
            share * 100.0
        )));
    }
    if v.iter().flatten().any(|x| *x < 0.0) {
        return Err(PhnError::InvalidInput(format!(
            "column '{name}' has negative values; counts and denominators cannot be negative"
        )));
    }
    Ok(v)
}

fn load(ctx: &Context, args: &PerformanceArgs, specs: &[Spec]) -> Result<Loaded> {
    let lt = localdata::load(ctx, &args.path, args.sheet.as_deref())?;
    if args.period_column.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "period_column is required: the column with the period of each row".into(),
        ));
    }
    let pi = lt.column(&args.period_column)?;
    let raw: Vec<Option<String>> = lt
        .table
        .column(pi)
        .map(|s| (!phn_local::phi::is_missing(s)).then(|| s.trim().to_string()))
        .collect();
    let n_rows = raw.len();
    let present = raw.iter().flatten().count();
    if present == 0 {
        return Err(PhnError::InvalidInput(format!(
            "column '{}' has no values",
            args.period_column
        )));
    }
    // Dates are rolled up; other labels are used as written.
    let dates: Vec<Option<NaiveDate>> = lt.table.column(pi).map(phn_local::phi::parse_date).collect();
    let date_count = dates.iter().flatten().count();
    let date_mode = date_count * 10 >= present * 8;
    let dp = args.date_period.unwrap_or_default();
    let mut labels: Vec<Option<String>> = Vec::with_capacity(n_rows);
    let mut keys: BTreeMap<String, (i32, u32)> = BTreeMap::new();
    let count_rows_mode = args.count_rows.unwrap_or(false);
    for i in 0..n_rows {
        let l = if date_mode {
            dates[i].map(|d| {
                let k = date_key(d, dp);
                let label = date_label(k, dp);
                keys.insert(label.clone(), k);
                label
            })
        } else {
            raw[i].clone()
        };
        labels.push(l);
    }
    // Labels must not look like identifiers.
    if !date_mode {
        let mut uniq: Vec<&String> = labels.iter().flatten().collect();
        uniq.sort();
        uniq.dedup();
        if uniq.len() > 600 {
            return Err(PhnError::InvalidInput(format!(
                "column '{}' has {} different values; a period column should have one value per \
                 reporting period",
                args.period_column,
                uniq.len()
            )));
        }
        let bad = uniq.iter().any(|l| {
            phn_local::phi::scan_text(l).iter().any(|f| {
                !matches!(
                    f.kind,
                    phn_local::phi::Identifier::Date
                        | phn_local::phi::Identifier::Age
                        | phn_local::phi::Identifier::Geography
                        | phn_local::phi::Identifier::ZipCode
                )
            })
        });
        if bad {
            return Err(PhnError::Privacy(format!(
                "values in column '{}' look like identifiers, so it cannot be used as the period \
                 column",
                args.period_column
            )));
        }
        for l in uniq {
            if let Some(k) = text_period_key(l) {
                keys.insert((*l).clone(), k);
            }
        }
    }
    // Order: by key when every label has one; otherwise first appearance.
    let mut order: Vec<String> = Vec::new();
    for l in labels.iter().flatten() {
        if !order.contains(l) {
            order.push(l.clone());
        }
    }
    if order.iter().all(|l| keys.contains_key(l)) {
        order.sort_by_key(|l| keys[l]);
    }
    // Count-rows mode fills gaps between date periods with zero.
    let long = args.measure_column.is_some();
    let counting = count_rows_mode || (long && args.value_column.is_none());
    if counting && date_mode {
        let (Some(first), Some(last)) = (
            order.first().and_then(|l| keys.get(l)).copied(),
            order.last().and_then(|l| keys.get(l)).copied(),
        ) else {
            return Err(PhnError::InvalidInput("no periods could be read".into()));
        };
        let mut all = vec![first];
        while *all.last().unwrap_or(&first) < last && all.len() <= MAX_PERIODS {
            let nx = next_key(*all.last().unwrap_or(&first), dp);
            all.push(nx);
        }
        order = all.iter().map(|k| date_label(*k, dp)).collect();
    }
    if order.len() > MAX_PERIODS {
        return Err(PhnError::InvalidInput(format!(
            "the file has {} periods; this tool handles up to {MAX_PERIODS}",
            order.len()
        )));
    }
    if order.is_empty() {
        return Err(PhnError::InvalidInput("no periods could be read".into()));
    }
    let pos: BTreeMap<&str, usize> = order.iter().enumerate().map(|(i, l)| (l.as_str(), i)).collect();
    let np = order.len();
    let mut series: BTreeMap<String, Series2> = BTreeMap::new();
    let new = || Series2 {
        num: vec![None; np],
        den: vec![None; np],
    };
    let add = |v: &mut Vec<Option<f64>>, i: usize, x: f64| {
        v[i] = Some(v[i].unwrap_or(0.0) + x);
    };
    let layout;
    if let Some(mc) = &args.measure_column {
        layout = "long";
        let mi = lt.column(mc)?;
        let cats = lt.categorical(mi)?;
        let vals = match &args.value_column {
            Some(v) => Some(numeric_column(&lt, v)?),
            None => None,
        };
        let dens = match &args.denominator_column {
            Some(d) => Some(numeric_column(&lt, d)?),
            None => None,
        };
        let mut found: BTreeSet<String> = BTreeSet::new();
        // Canonical measure names as the nurse typed them.
        for r in 0..n_rows {
            let (Some(l), Some(cat)) = (&labels[r], &cats[r]) else {
                continue;
            };
            let Some(spec) = specs.iter().find(|s| s.measure.eq_ignore_ascii_case(cat)) else {
                continue;
            };
            found.insert(spec.measure.clone());
            let Some(&pi2) = pos.get(l.as_str()) else {
                continue;
            };
            let s = series.entry(spec.measure.clone()).or_insert_with(new);
            match &vals {
                Some(v) => {
                    if let Some(x) = v[r] {
                        add(&mut s.num, pi2, x);
                    }
                    if let Some(d) = &dens
                        && let Some(x) = d[r]
                    {
                        add(&mut s.den, pi2, x);
                    }
                }
                None => add(&mut s.num, pi2, 1.0),
            }
        }
        for sp in specs {
            if !found.contains(&sp.measure) {
                let mut names: Vec<String> = cats.iter().flatten().cloned().collect();
                names.sort();
                names.dedup();
                return Err(PhnError::InvalidInput(format!(
                    "no rows have the measure '{}' in column '{mc}'. Measures in that column: {}",
                    sp.measure,
                    names.join(", ")
                )));
            }
        }
        if vals.is_none() && date_mode {
            // Rows were counted: a date period with no row has a count of zero.
            for s in series.values_mut() {
                for x in s.num.iter_mut() {
                    x.get_or_insert(0.0);
                }
            }
        }
    } else {
        layout = if count_rows_mode { "records counted" } else { "wide" };
        for sp in specs {
            let s = series.entry(sp.measure.clone()).or_insert_with(new);
            if count_rows_mode && sp.measure.eq_ignore_ascii_case("records") {
                for l in labels.iter().flatten() {
                    if let Some(&i) = pos.get(l.as_str()) {
                        add(&mut s.num, i, 1.0);
                    }
                }
                if date_mode {
                    for x in s.num.iter_mut() {
                        x.get_or_insert(0.0);
                    }
                }
                continue;
            }
            let v = numeric_column(&lt, &sp.measure)?;
            let d = match (&sp.kind, &sp.denominator) {
                (MeasureKind::Percent, Some(dn)) => Some(numeric_column(&lt, dn)?),
                _ => None,
            };
            for r in 0..n_rows {
                let Some(l) = &labels[r] else { continue };
                let Some(&i) = pos.get(l.as_str()) else { continue };
                if let Some(x) = v[r] {
                    add(&mut s.num, i, x);
                }
                if let Some(d) = &d
                    && let Some(x) = d[r]
                {
                    add(&mut s.den, i, x);
                }
            }
        }
    }
    Ok(Loaded {
        periods: order,
        series,
        layout,
        local: lt,
    })
}

// ---------------------------------------------------------------- calculation

struct Computed {
    spec: Spec,
    /// Per-period (value, numerator, denominator) before suppression.
    values: Vec<Option<f64>>,
    nums: Vec<Option<f64>>,
    dens: Vec<Option<f64>>,
    hide: Vec<bool>,
    current: Option<f64>,
    latest: Option<f64>,
    expected: Option<f64>,
    progress: Option<f64>,
    status: String,
    meeting: Option<usize>,
    reported: usize,
    trend: Option<spc::Chart>,
    trend_labels: Vec<usize>,
    chart_type: Option<ChartType>,
    trend_notes: Vec<String>,
}

fn meets(v: f64, target: f64, d: Direction) -> bool {
    match d {
        Direction::HigherIsBetter => v >= target,
        Direction::LowerIsBetter => v <= target,
    }
}

fn compute(
    spec: &Spec,
    s: &Series2,
    threshold: u32,
    periods_in_program: Option<u32>,
    on_track: f64,
) -> Result<Computed> {
    let n = s.num.len();
    let t = f64::from(threshold);
    let small = |x: f64| x > 0.0 && x < t;
    let mut values = vec![None; n];
    let mut hide = vec![false; n];
    for i in 0..n {
        match spec.kind {
            MeasureKind::Count => {
                if let Some(x) = s.num[i] {
                    values[i] = Some(x);
                    hide[i] = small(x);
                }
            }
            MeasureKind::Percent => {
                if let (Some(num), Some(den)) = (s.num[i], s.den[i]) {
                    if den > 0.0 {
                        if num > den {
                            return Err(PhnError::InvalidInput(format!(
                                "measure '{}': a numerator is larger than its denominator",
                                spec.measure
                            )));
                        }
                        values[i] = Some(num / den * 100.0);
                        hide[i] = small(num) || small(den) || small(den - num);
                    }
                }
            }
        }
    }
    let idx: Vec<usize> = (0..n).filter(|i| values[*i].is_some()).collect();
    let reported = idx.len();
    let latest = idx.last().and_then(|i| values[*i]);
    // Cumulative figure.
    let current = match spec.kind {
        MeasureKind::Count => (reported > 0).then(|| idx.iter().filter_map(|i| s.num[*i]).sum()),
        MeasureKind::Percent => {
            let num: f64 = idx.iter().filter_map(|i| s.num[*i]).sum();
            let den: f64 = idx.iter().filter_map(|i| s.den[*i]).sum();
            (den > 0.0).then_some(num / den * 100.0)
        }
    };
    let basis_value = match spec.basis {
        Basis::Cumulative => current,
        Basis::EachPeriod => latest,
    };
    let mut expected = None;
    let mut progress = None;
    let mut status = "No target given".to_string();
    let mut meeting = None;
    if let Some(target) = spec.target
        && let Some(v) = basis_value
    {
        let higher = spec.direction == Direction::HigherIsBetter;
        progress = match (spec.kind, spec.baseline) {
            (MeasureKind::Percent, Some(b)) if (target - b).abs() > f64::EPSILON => {
                Some((v - b) / (target - b) * 100.0)
            }
            _ if higher && target != 0.0 => Some(v / target * 100.0),
            _ => None,
        };
        status = match (spec.kind, spec.basis) {
            (MeasureKind::Count, Basis::Cumulative) => {
                if higher {
                    if v >= target {
                        "Met".into()
                    } else if let Some(total) = periods_in_program.filter(|p| *p > 0) {
                        let e = target * reported as f64 / f64::from(total);
                        expected = Some(e);
                        if v >= on_track * e { "On track".into() } else { "Behind".into() }
                    } else {
                        "Not yet met".into()
                    }
                } else if v <= target {
                    "Within limit".into()
                } else {
                    "Over limit".into()
                }
            }
            _ => {
                if meets(v, target, spec.direction) {
                    "Met".into()
                } else if higher {
                    "Below target".into()
                } else {
                    "Above target".into()
                }
            }
        };
        if spec.basis == Basis::EachPeriod {
            meeting = Some(
                idx.iter()
                    .filter(|i| values[**i].is_some_and(|x| meets(x, target, spec.direction)))
                    .count(),
            );
        }
    }
    // Trend rules on the periods with data (full values; the response hides small ones).
    let mut trend = None;
    let mut chart_type = None;
    let mut notes = Vec::new();
    let series_vals: Vec<f64> = idx.iter().filter_map(|i| values[*i]).collect();
    if series_vals.len() >= MIN_TREND_POINTS {
        let use_p = spec.kind == MeasureKind::Percent
            && series_vals.len() >= MIN_P_POINTS
            && idx.iter().all(|i| s.den[*i].is_some_and(|d| d > 0.0));
        let input = if use_p {
            chart_type = Some(ChartType::P);
            Input {
                chart: ChartType::P,
                values: idx.iter().filter_map(|i| s.num[*i]).collect(),
                denominators: Some(idx.iter().filter_map(|i| s.den[*i]).collect()),
                phase_starts: vec![],
                baseline: None,
            }
        } else {
            chart_type = Some(ChartType::Run);
            Input {
                chart: ChartType::Run,
                values: series_vals.clone(),
                denominators: None,
                phase_starts: vec![],
                baseline: None,
            }
        };
        match spc::chart(&input) {
            Ok(c) => {
                notes.extend(c.warnings.clone());
                trend = Some(c);
            }
            Err(e) => notes.push(format!("Trend rules were not applied: {e}")),
        }
    } else if reported >= 2 {
        notes.push(format!(
            "Trend rules need at least {MIN_TREND_POINTS} periods with data; there are {reported}."
        ));
    }
    Ok(Computed {
        spec: spec.clone(),
        values,
        nums: s.num.clone(),
        dens: s.den.clone(),
        hide,
        current,
        latest,
        expected,
        progress,
        status,
        meeting,
        reported,
        trend,
        trend_labels: idx,
        chart_type,
        trend_notes: notes,
    })
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn to_result(c: &Computed, periods: &[String], chart: Option<ChartFiles>) -> MeasureResult {
    let withheld = c.hide.iter().any(|h| *h);
    // Signals by period index.
    let mut sig_by_period: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut signals_text = Vec::new();
    if let Some(t) = &c.trend {
        for (k, p) in t.points.iter().enumerate() {
            if !p.signals.is_empty() {
                let pi = c.trend_labels[k];
                let rules: Vec<String> = p.signals.iter().map(|r| r.describe().to_string()).collect();
                signals_text.push(format!("{}: {}", periods[pi], rules.join("; ")));
                sig_by_period.insert(pi, rules);
            }
        }
    }
    let center = c.trend.as_ref().and_then(|t| t.points.last()).map(|p| p.center);
    let latest_vs = match (center, c.trend.as_ref().and_then(|t| t.points.last())) {
        (Some(ce), Some(p)) => Some(
            if (p.value - ce).abs() < 1e-12 {
                "on"
            } else if p.value > ce {
                "above"
            } else {
                "below"
            }
            .to_string(),
        ),
        _ => None,
    };
    let rows: Vec<PeriodRow> = periods
        .iter()
        .enumerate()
        .map(|(i, p)| PeriodRow {
            period: p.clone(),
            value: if c.hide[i] { None } else { c.values[i].map(round2) },
            numerator: if c.spec.kind == MeasureKind::Percent && !c.hide[i] {
                c.nums.get(i).copied().flatten()
            } else {
                None
            },
            denominator: if c.spec.kind == MeasureKind::Percent && !c.hide[i] {
                c.dens.get(i).copied().flatten()
            } else {
                None
            },
            suppressed: c.hide[i],
            missing: c.values[i].is_none(),
            signals: sig_by_period.get(&i).cloned().unwrap_or_default(),
        })
        .collect();
    let chart_name = match c.chart_type {
        Some(ChartType::P) => "p chart",
        Some(_) => "run chart",
        None => "not assessed",
    };
    MeasureResult {
        measure: c.spec.measure.clone(),
        kind: c.spec.kind,
        direction: c.spec.direction,
        basis: c.spec.basis,
        unit: c.spec.unit.clone(),
        target: c.spec.target,
        baseline: c.spec.baseline,
        periods: rows,
        periods_reported: c.reported,
        current: if withheld { None } else { c.current.map(round2) },
        latest: if c.hide.iter().rev().zip(c.values.iter().rev()).find(|(_, v)| v.is_some()).is_some_and(|(h, _)| *h) {
            None
        } else {
            c.latest.map(round2)
        },
        progress_percent: if withheld { None } else { c.progress.map(round2) },
        expected_by_now: if withheld { None } else { c.expected.map(round2) },
        status: if withheld {
            "Not shown: a small count in this measure is suppressed (the report and workbook in the output folder have the values)".into()
        } else {
            c.status.clone()
        },
        periods_meeting_target: if withheld { None } else { c.meeting },
        withheld,
        trend: TrendInfo {
            chart: chart_name.into(),
            points: c.trend.as_ref().map_or(0, |t| t.points.len()),
            signals: signals_text,
            center: if withheld { None } else { center.map(round2) },
            latest_vs_center: if withheld { None } else { latest_vs },
            notes: c.trend_notes.clone(),
        },
        chart,
    }
}

// ---------------------------------------------------------------- the tool

fn clean_name(label: &str, s: &str) -> Result<String> {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() || t.chars().count() > 120 {
        return Err(PhnError::InvalidInput(format!(
            "{label} must be 1 to 120 characters"
        )));
    }
    Ok(t)
}

fn build_specs(args: &PerformanceArgs) -> Result<Vec<Spec>> {
    if args.targets.is_empty() {
        return Err(PhnError::InvalidInput(
            "give at least one target: {\"measure\": \"<column or measure name>\", \"target\": \
             <number>}"
                .into(),
        ));
    }
    if args.targets.len() > MAX_MEASURES {
        return Err(PhnError::InvalidInput(format!(
            "give at most {MAX_MEASURES} targets"
        )));
    }
    let mut specs: Vec<Spec> = Vec::new();
    for t in &args.targets {
        let measure = clean_name("measure", &t.measure)?;
        if specs.iter().any(|s| s.measure.eq_ignore_ascii_case(&measure)) {
            return Err(PhnError::InvalidInput(format!(
                "measure '{measure}' is listed twice"
            )));
        }
        let Some(target) = t.target else {
            return Err(PhnError::InvalidInput(format!(
                "measure '{measure}' needs a target number"
            )));
        };
        if !target.is_finite() || target < 0.0 {
            return Err(PhnError::InvalidInput(format!(
                "the target for '{measure}' must be a number of 0 or more"
            )));
        }
        let kind = t.kind.unwrap_or_default();
        if let Some(b) = t.baseline
            && !b.is_finite()
        {
            return Err(PhnError::InvalidInput(format!(
                "the baseline for '{measure}' must be a number"
            )));
        }
        let denominator = match (&t.denominator, kind) {
            (Some(d), MeasureKind::Percent) => Some(clean_name("denominator", d)?),
            (Some(_), MeasureKind::Count) => {
                return Err(PhnError::InvalidInput(format!(
                    "'{measure}' has a denominator but kind is count; set kind to percent"
                )));
            }
            (None, _) => None,
        };
        if kind == MeasureKind::Percent && denominator.is_none() && args.denominator_column.is_none()
        {
            return Err(PhnError::InvalidInput(format!(
                "'{measure}' is a percent measure and needs a denominator: give `denominator` \
                 (a column) or, in the long layout, `denominator_column`"
            )));
        }
        specs.push(Spec {
            measure,
            target: Some(target),
            direction: t.direction.unwrap_or_default(),
            kind,
            denominator,
            basis: t.basis.unwrap_or_default(),
            baseline: t.baseline,
            unit: t.unit.as_ref().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()),
        });
    }
    Ok(specs)
}

/// Track program performance against targets.
pub fn track_program_performance(
    ctx: &Context,
    args: PerformanceArgs,
) -> Result<Envelope<PerformanceResult>> {
    if args.path.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "path is required: a CSV or XLSX in /data with the program's results by period".into(),
        ));
    }
    let specs = build_specs(&args)?;
    let on_track = args.on_track_fraction.unwrap_or(DEFAULT_ON_TRACK);
    if !(on_track > 0.0 && on_track <= 1.0) {
        return Err(PhnError::InvalidInput(
            "on_track_fraction must be more than 0 and at most 1".into(),
        ));
    }
    if let Some(p) = args.periods_in_program
        && !(1..=600).contains(&p)
    {
        return Err(PhnError::InvalidInput(
            "periods_in_program must be between 1 and 600".into(),
        ));
    }
    let program = match &args.program_name {
        Some(p) if !p.trim().is_empty() => {
            let t = clean_name("program_name", p)?;
            if !phn_local::phi::scan_text(&t).is_empty() {
                return Err(PhnError::Privacy(
                    "program_name appears to contain an identifier; use the program's name".into(),
                ));
            }
            Some(t)
        }
        _ => None,
    };
    if args.count_rows.unwrap_or(false)
        && args.measure_column.is_none()
        && !specs.iter().any(|s| s.measure.eq_ignore_ascii_case("records"))
    {
        return Err(PhnError::InvalidInput(
            "with count_rows, give a target for the measure \"Records\"".into(),
        ));
    }
    let threshold = ctx.suppression_threshold();
    let loaded = load(ctx, &args, &specs)?;
    let periods = loaded.periods.clone();

    let mut computed = Vec::new();
    for sp in &specs {
        let s = loaded.series.get(&sp.measure).cloned().unwrap_or_default();
        let s = if s.num.is_empty() {
            Series2 {
                num: vec![None; periods.len()],
                den: vec![None; periods.len()],
            }
        } else {
            s
        };
        computed.push(compute(sp, &s, threshold, args.periods_in_program, on_track)?);
    }

    // Charts (full values; written to the output folder).
    let mut files_written: Vec<String> = Vec::new();
    let mut chart_files: Vec<Option<ChartFiles>> = vec![None; computed.len()];
    if args.write_charts.unwrap_or(true) {
        for (i, c) in computed.iter().enumerate() {
            if c.reported < 2 {
                continue;
            }
            let title = format!(
                "{}{}",
                program.as_ref().map(|p| format!("{p}: ")).unwrap_or_default(),
                c.spec.measure
            );
            let mut spec = ChartSpec::new(ChartKind::Line, title, periods.clone());
            spec.series = vec![Series {
                name: c.spec.measure.clone(),
                values: c.values.clone(),
                ..Series::default()
            }];
            if let Some(t) = c.spec.target {
                spec.ref_lines.push(RefLine {
                    label: "Target".into(),
                    values: vec![Some(t)],
                    dashed: true,
                });
            }
            if let Some(t) = &c.trend {
                spec.ref_lines.push(RefLine {
                    label: "Center line".into(),
                    values: {
                        let mut v = vec![None; periods.len()];
                        for (k, p) in t.points.iter().enumerate() {
                            v[c.trend_labels[k]] = Some(p.center);
                        }
                        v
                    },
                    dashed: false,
                });
                spec.flags = t
                    .points
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| !p.signals.is_empty())
                    .map(|(k, p)| Flag {
                        series: 0,
                        index: c.trend_labels[k],
                        label: p.signals.iter().map(|r| r.describe()).collect::<Vec<_>>().join("; "),
                    })
                    .collect();
            }
            spec.y_label = Some(match (c.spec.kind, &c.spec.unit) {
                (MeasureKind::Percent, _) => "Percent".to_string(),
                (_, Some(u)) => u.clone(),
                _ => "Count".to_string(),
            });
            spec.note = Some("Full values from your file; held in the output folder only.".into());
            let stem = format!("program_{}", c.spec.measure);
            let (f, _) = write_chart(ctx, &spec, &stem, ChartFormat::Png)?;
            if let Some(p) = &f.png {
                files_written.push(p.clone());
            }
            chart_files[i] = Some(f);
        }
    }

    let measures: Vec<MeasureResult> = computed
        .iter()
        .zip(chart_files.iter().cloned())
        .map(|(c, f)| to_result(c, &periods, f))
        .collect();

    let mut result = PerformanceResult {
        program: program.clone(),
        layout: loaded.layout.to_string(),
        first_period: periods.first().cloned().unwrap_or_default(),
        last_period: periods.last().cloned().unwrap_or_default(),
        periods: periods.len(),
        measures,
        periods_in_program: args.periods_in_program,
        on_track_fraction: on_track,
        suppression_threshold: threshold,
        report_file: None,
        workbook_file: None,
    };

    // Report and workbook (full values; local).
    let want_report = args.write_report.unwrap_or(true);
    let want_book = args.write_workbook.unwrap_or(true);
    if want_report || want_book {
        let (dir, _) = templates::output_folder(ctx, "program_performance")?;
        if want_report {
            let md = report_markdown(&result, &computed, &periods);
            let format = args.format.unwrap_or_default();
            let doc = templates::document(ctx, &md, "en-US", "Program performance report");
            let f = templates::write_in(
                ctx,
                &dir,
                &format!("program_performance_report.{}", format.ext()),
                &format.render(&doc)?,
            )?;
            files_written.push(f.clone());
            result.report_file = Some(f);
        }
        if want_book {
            let bytes = build_workbook(&result, &computed, &periods, program.as_deref())
                .map_err(|e| PhnError::Internal(format!("XLSX: {e}")))?;
            let f = templates::write_in(ctx, &dir, "program_performance.xlsx", &bytes)?;
            files_written.push(f.clone());
            result.workbook_file = Some(f);
        }
    }

    let any_suppressed = result.measures.iter().any(|m| m.withheld);
    let summary = summary(&result);
    let mut caveats = vec![
        "Progress is computed from the values in your file as they are; the tool cannot check \
         them. For a count, 'cumulative' adds every period; for a percentage it pools the \
         numerators and denominators of all periods (not an average of period percentages)."
            .to_string(),
        format!(
            "Pace for a cumulative count assumes an even rate across {} periods and calls the \
             program on track when it has reached at least {:.0}% of the pace needed. That share \
             is this tool's convention (change on_track_fraction); use your funder's rule if it \
             has one.",
            args.periods_in_program
                .map_or("the program's".to_string(), |p| p.to_string()),
            on_track * 100.0
        ),
        "Trend flags use run-chart rules (an unusually long run on one side of the median, or too \
         few crossings of it) and, for percentages with at least eight periods, p-chart rules \
         (a point outside the limits; eight in a row on one side; six rising or falling). They \
         need about 10 or more periods to be reliable, and a flag says the pattern is unlikely to \
         be chance, not why it happened."
            .into(),
        "A hidden number is a count from 1 to the suppression threshold minus 1 (and, for a \
         percentage, a numerator, denominator, or the remainder that small). When one is hidden, \
         the measure's totals, progress, and center line are withheld from this response because \
         together with the visible numbers they would reveal it."
            .into(),
    ];
    if loaded.layout == "records counted" || args.measure_column.is_some() && args.value_column.is_none() {
        caveats.push(
            "Rows were counted per period; one row is assumed to be one record. Dates were rolled \
             up to periods, and only counts are returned."
                .into(),
        );
    }
    if args.periods_in_program.is_none() {
        caveats.push(
            "periods_in_program was not given, so cumulative counts below target are shown as 'Not \
             yet met' without a pace check."
                .into(),
        );
    }
    let mut notes = vec![
        "Only the columns you named (period, measure, value, and denominator) were read; other \
         columns in the file, including any contact people, were not read or returned."
            .to_string(),
    ];
    if !files_written.is_empty() {
        notes.push(
            "The report, workbook, and charts in the output folder hold the full values from your \
             file (aggregates by period, including any small counts). They stay on your computer."
                .into(),
        );
    }
    let sources = vec![
        loaded.local.source(),
        Source::computed(
            "Run chart rules (PLoS One 2014;9:e113825) and p-chart special-cause rules (The Health Care Data Guide, 2011)",
            "Calculated by this server",
        ),
    ];
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: any_suppressed,
        suppression_threshold: Some(threshold),
        identifiers_detected: loaded.local.identifier_categories(),
        files_written,
        notes,
    };
    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

// ---------------------------------------------------------------- report

fn fmt(x: Option<f64>) -> String {
    x.map_or("-".into(), |v| md::num(v, if (v - v.round()).abs() < 1e-9 { 0 } else { 1 }))
}

fn summary(r: &PerformanceResult) -> String {
    let mut s = format!(
        "**Program performance{}**: {} to {} ({} periods).\n\n",
        r.program.as_ref().map(|p| format!(" for {p}")).unwrap_or_default(),
        r.first_period,
        r.last_period,
        r.periods
    );
    let rows: Vec<Vec<String>> = r
        .measures
        .iter()
        .map(|m| {
            vec![
                m.measure.clone(),
                fmt(m.target),
                fmt(m.current),
                fmt(m.progress_percent).replace('-', "-"),
                m.status.clone(),
                if m.trend.signals.is_empty() {
                    if m.trend.points == 0 { "not assessed".into() } else { "no signal".into() }
                } else {
                    format!("{} signal(s)", m.trend.signals.len())
                },
            ]
        })
        .collect();
    s.push_str(&md::table(
        &["Measure", "Target", "Current", "Progress (%)", "Status", "Trend"],
        &rows,
    ));
    s.push('\n');
    for m in &r.measures {
        if !m.trend.signals.is_empty() {
            s.push_str(&format!("**{}** trend signals ({}):\n", m.measure, m.trend.chart));
            for x in &m.trend.signals {
                s.push_str(&format!("- {x}\n"));
            }
            s.push('\n');
        }
    }
    if r.measures.iter().any(|m| m.withheld) {
        s.push_str(
            "Small counts were suppressed for the measures marked 'Not shown'; the report and \
             workbook have the values.\n\n",
        );
    }
    if let Some(f) = &r.report_file {
        s.push_str(&format!("Report: `{f}`.\n"));
    }
    if let Some(f) = &r.workbook_file {
        s.push_str(&format!("Workbook with live formulas: `{f}`.\n"));
    }
    s
}

fn report_markdown(r: &PerformanceResult, computed: &[Computed], periods: &[String]) -> String {
    let mut m = format!(
        "# Program performance report{}\n\n",
        r.program.as_ref().map(|p| format!(": {p}")).unwrap_or_default()
    );
    m.push_str(&format!(
        "Reporting periods: {} to {} ({} periods).\n\n",
        r.first_period, r.last_period, r.periods
    ));
    m.push_str("## Progress to target\n\n");
    let rows: Vec<Vec<String>> = computed
        .iter()
        .map(|c| {
            vec![
                c.spec.measure.clone(),
                fmt(c.spec.target),
                fmt(c.current.map(round2)),
                fmt(c.progress.map(round2)),
                if c.status == "No target given" { c.status.clone() } else { c.status.clone() },
            ]
        })
        .collect();
    m.push_str(&md::table(
        &["Measure", "Target", "Current", "Progress (%)", "Status"],
        &rows,
    ));
    m.push('\n');
    for (c, res) in computed.iter().zip(&r.measures) {
        m.push_str(&format!("## {}\n\n", c.spec.measure));
        let rows: Vec<Vec<String>> = periods
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut row = vec![p.clone(), fmt(c.values[i].map(round2))];
                if c.spec.kind == MeasureKind::Percent {
                    row.push(fmt(c.nums[i]));
                    row.push(fmt(c.dens[i]));
                }
                row
            })
            .collect();
        let headers: Vec<&str> = if c.spec.kind == MeasureKind::Percent {
            vec!["Period", "Percent", "Numerator", "Denominator"]
        } else {
            vec!["Period", "Count"]
        };
        m.push_str(&md::table(&headers, &rows));
        m.push('\n');
        if res.trend.signals.is_empty() {
            m.push_str(&format!(
                "Trend ({}): no signal. {}\n\n",
                res.trend.chart,
                res.trend.notes.join(" ")
            ));
        } else {
            m.push_str(&format!("Trend signals ({}):\n\n", res.trend.chart));
            for s in &res.trend.signals {
                m.push_str(&format!("- {s}\n"));
            }
            m.push('\n');
        }
    }
    m.push_str(
        "## Method\n\nCumulative counts add every period; cumulative percentages pool the \
         numerators and denominators. Progress for a count is the total divided by the target; \
         for a percentage with a baseline it is the share of the distance from the baseline to \
         the target. A count is on track when it has reached the stated share of the pace needed \
         to meet the target by the end of the program. Trend flags use run-chart and p-chart \
         rules and need about 10 or more periods. The numbers are as they appear in your file.\n",
    );
    m
}

// ---------------------------------------------------------------- workbook

fn col_name(c: u16) -> String {
    rust_xlsxwriter::utility::column_number_to_name(c)
}

fn build_workbook(
    r: &PerformanceResult,
    computed: &[Computed],
    periods: &[String],
    program: Option<&str>,
) -> std::result::Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let head = Format::new().set_bold().set_text_wrap();
    let pct = Format::new().set_num_format("0%");
    let one = Format::new().set_num_format("0.0");

    // Data sheet first (so formulas can refer to it), added second in tab order below.
    let mut data_cols: Vec<(u16, Option<u16>, Option<u16>)> = Vec::new();
    {
        let mut col = 1u16;
        for c in computed {
            match c.spec.kind {
                MeasureKind::Count => {
                    data_cols.push((col, None, None));
                    col += 1;
                }
                MeasureKind::Percent => {
                    data_cols.push((col, Some(col + 1), Some(col + 2)));
                    col += 3;
                }
            }
        }
    }

    let summary = wb.add_worksheet();
    summary.set_name("Summary")?;
    summary.write_string_with_format(
        0,
        0,
        format!(
            "Program performance{}",
            program.map(|p| format!(": {p}")).unwrap_or_default()
        ),
        &bold,
    )?;
    summary.write_string(1, 0, format!("Periods in the file: {} to {}", r.first_period, r.last_period))?;
    summary.write_string_with_format(2, 0, "Periods in the program year (for pace; leave blank for none)", &bold)?;
    if let Some(p) = r.periods_in_program {
        summary.write_number(2, 1, f64::from(p))?;
    }
    summary.write_string_with_format(3, 0, "On-track share of the pace needed", &bold)?;
    summary.write_number(3, 1, r.on_track_fraction)?;
    let heads = [
        "Measure",
        "Type",
        "Better is",
        "Target applies to",
        "Target",
        "Baseline",
        "Current",
        "Progress to target",
        "Expected by now",
        "Status",
        "Latest period",
    ];
    for (i, h) in heads.iter().enumerate() {
        summary.write_string_with_format(5, i as u16, *h, &head)?;
        summary.set_column_width(i as u16, if i == 0 { 34 } else { 16 })?;
    }
    let last = DATA_LAST_ROW;
    for (k, c) in computed.iter().enumerate() {
        let row = 6 + k as u32;
        let xr = row + 1; // 1-based Excel row
        let (c1, c2, c3) = data_cols[k];
        let sp = &c.spec;
        let rng = |col: u16| {
            let n = col_name(col);
            format!("Data!{n}$2:{n}${last}")
        };
        summary.write_string(row, 0, &sp.measure)?;
        summary.write_string(
            row,
            1,
            match sp.kind {
                MeasureKind::Count => "count",
                MeasureKind::Percent => "percent",
            },
        )?;
        let higher = sp.direction == Direction::HigherIsBetter;
        summary.write_string(row, 2, if higher { "higher" } else { "lower" })?;
        summary.write_string(
            row,
            3,
            match sp.basis {
                Basis::Cumulative => "all periods",
                Basis::EachPeriod => "each period",
            },
        )?;
        if let Some(t) = sp.target {
            summary.write_number(row, 4, t)?;
        }
        if let Some(b) = sp.baseline {
            summary.write_number(row, 5, b)?;
        }
        // The value the target is judged on, and the latest period's value.
        let value_col = c3.unwrap_or(c1);
        let latest = format!("LOOKUP(2,1/({r}<>\"\"),{r})", r = rng(value_col));
        let current = match (sp.kind, sp.basis) {
            (MeasureKind::Count, Basis::Cumulative) => format!("SUM({})", rng(c1)),
            (MeasureKind::Percent, Basis::Cumulative) => format!(
                "IF(SUM({d})=0,\"\",SUM({n})/SUM({d})*100)",
                n = rng(c1),
                d = rng(c2.unwrap_or(c1))
            ),
            (_, Basis::EachPeriod) => latest.clone(),
        };
        let cur_cached = match sp.basis {
            Basis::Cumulative => c.current,
            Basis::EachPeriod => c.latest,
        };
        let cached = |v: Option<f64>| v.map_or(String::new(), |x| format!("{}", round2(x)));
        summary.write_formula(
            row,
            6,
            Formula::new(format!("={current}")).set_result(cached(cur_cached)),
        )?;
        // Progress.
        let progress = match (sp.kind, sp.baseline.is_some()) {
            (MeasureKind::Percent, true) => format!(
                "IF(ISNUMBER(G{xr}),IF(E{xr}<>F{xr},(G{xr}-F{xr})/(E{xr}-F{xr}),\"\"),\"\")"
            ),
            _ if higher => format!("IF(AND(ISNUMBER(G{xr}),E{xr}<>0),G{xr}/E{xr},\"\")"),
            _ => "\"\"".to_string(),
        };
        summary.write_formula_with_format(
            row,
            7,
            Formula::new(format!("={progress}")).set_result(cached(c.progress.map(|p| p / 100.0))),
            &pct,
        )?;
        // Expected by now (cumulative higher-is-better counts only).
        let pace = sp.kind == MeasureKind::Count && sp.basis == Basis::Cumulative && higher;
        if pace {
            summary.write_formula(
                row,
                8,
                Formula::new(format!(
                    "=IF(ISNUMBER($B$3),E{xr}*COUNT({})/$B$3,\"\")",
                    rng(c1)
                ))
                .set_result(cached(c.expected)),
            )?;
        }
        // Status.
        let status = match (sp.kind, sp.basis, higher) {
            (MeasureKind::Count, Basis::Cumulative, true) => format!(
                "IF(G{xr}>=E{xr},\"Met\",IF(ISNUMBER(I{xr}),IF(G{xr}>=$B$4*I{xr},\"On track\",\"Behind\"),\"Not yet met\"))"
            ),
            (MeasureKind::Count, Basis::Cumulative, false) => {
                format!("IF(G{xr}<=E{xr},\"Within limit\",\"Over limit\")")
            }
            (_, _, true) => format!("IF(G{xr}>=E{xr},\"Met\",\"Below target\")"),
            (_, _, false) => format!("IF(G{xr}<=E{xr},\"Met\",\"Above target\")"),
        };
        summary.write_formula(
            row,
            9,
            Formula::new(format!("=IF(ISNUMBER(G{xr}),{status},\"\")")).set_result(c.status.clone()),
        )?;
        summary.write_formula_with_format(
            row,
            10,
            Formula::new(format!("={latest}")).set_result(cached(c.latest)),
            &one,
        )?;
    }
    let notes_row = 6 + computed.len() as u32 + 1;
    summary.write_string(
        notes_row,
        0,
        "Formulas read the Data sheet (rows 2 to 101). Add a period by typing a new row under the last one; Current, Progress, and Status update.",
    )?;
    summary.write_string(
        notes_row + 1,
        0,
        "This workbook holds the full values from your file. Keep it with your program records.",
    )?;
    summary.set_freeze_panes(6, 1)?;

    let data = wb.add_worksheet();
    data.set_name("Data")?;
    data.write_string_with_format(0, 0, "Period", &head)?;
    data.set_column_width(0, 16)?;
    for (k, c) in computed.iter().enumerate() {
        let (c1, c2, c3) = data_cols[k];
        match c.spec.kind {
            MeasureKind::Count => {
                data.write_string_with_format(0, c1, &c.spec.measure, &head)?;
                data.set_column_width(c1, 18)?;
            }
            MeasureKind::Percent => {
                data.write_string_with_format(0, c1, format!("{} numerator", c.spec.measure), &head)?;
                data.write_string_with_format(
                    0,
                    c2.unwrap_or(c1 + 1),
                    format!("{} denominator", c.spec.measure),
                    &head,
                )?;
                data.write_string_with_format(
                    0,
                    c3.unwrap_or(c1 + 2),
                    format!("{} percent", c.spec.measure),
                    &head,
                )?;
                for cc in c1..=c3.unwrap_or(c1 + 2) {
                    data.set_column_width(cc, 18)?;
                }
            }
        }
    }
    for (i, p) in periods.iter().enumerate() {
        data.write_string(1 + i as u32, 0, p)?;
    }
    for (k, c) in computed.iter().enumerate() {
        let (c1, c2, c3) = data_cols[k];
        for i in 0..periods.len() {
            let row = 1 + i as u32;
            if let Some(x) = c.nums[i] {
                data.write_number(row, c1, x)?;
            }
            if let (Some(c2), Some(x)) = (c2, c.dens[i]) {
                data.write_number(row, c2, x)?;
            }
        }
        // The percent column is a formula on every row up to the last the Summary reads.
        if let (Some(c2), Some(c3)) = (c2, c3) {
            for row in 1..(DATA_LAST_ROW) {
                let xr = row + 1;
                let n = col_name(c1);
                let d = col_name(c2);
                let cached = c.values.get(i_of(row)).copied().flatten().map_or(String::new(), |x| format!("{}", round2(x)));
                data.write_formula_with_format(
                    row,
                    c3,
                    Formula::new(format!("=IF(OR({d}{xr}=\"\",{d}{xr}=0),\"\",{n}{xr}/{d}{xr}*100)"))
                        .set_result(cached),
                    &one,
                )?;
            }
        }
    }
    data.set_freeze_panes(1, 1)?;
    Ok(wb.save_to_buffer()?)
}

fn i_of(row: u32) -> usize {
    (row - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: MeasureKind, target: f64) -> Spec {
        Spec {
            measure: "m".into(),
            target: Some(target),
            direction: Direction::HigherIsBetter,
            kind,
            denominator: None,
            basis: Basis::Cumulative,
            baseline: None,
            unit: None,
        }
    }

    fn series(nums: &[f64], dens: &[f64]) -> Series2 {
        Series2 {
            num: nums.iter().map(|x| Some(*x)).collect(),
            den: dens.iter().map(|x| Some(*x)).collect(),
        }
    }

    #[test]
    fn cumulative_count_progress_and_pace() {
        // 3 of 4 quarters reported, 270 of 400 clients: pace needed 300, 270 is 90% of it.
        let c = compute(&spec(MeasureKind::Count, 400.0), &series(&[90.0, 100.0, 80.0], &[]), 5, Some(4), 0.9).unwrap();
        assert_eq!(c.current, Some(270.0));
        assert!((c.progress.unwrap() - 67.5).abs() < 1e-9);
        assert!((c.expected.unwrap() - 300.0).abs() < 1e-9);
        assert_eq!(c.status, "On track");
        let c = compute(&spec(MeasureKind::Count, 400.0), &series(&[60.0, 100.0, 80.0], &[]), 5, Some(4), 0.9).unwrap();
        assert_eq!(c.status, "Behind");
        let c = compute(&spec(MeasureKind::Count, 200.0), &series(&[60.0, 100.0, 80.0], &[]), 5, Some(4), 0.9).unwrap();
        assert_eq!(c.status, "Met");
        let c = compute(&spec(MeasureKind::Count, 400.0), &series(&[60.0, 100.0], &[]), 5, None, 0.9).unwrap();
        assert_eq!(c.status, "Not yet met");
    }

    #[test]
    fn percent_is_pooled_and_uses_the_baseline() {
        let mut s = spec(MeasureKind::Percent, 80.0);
        s.baseline = Some(50.0);
        // 30/50 and 60/80: pooled 90/130 = 69.2308%, not the mean of 60% and 75%.
        let c = compute(&s, &series(&[30.0, 60.0], &[50.0, 80.0]), 5, None, 0.9).unwrap();
        assert!((c.current.unwrap() - 69.230_769).abs() < 1e-4);
        assert!((c.progress.unwrap() - (69.230_769 - 50.0) / 30.0 * 100.0).abs() < 1e-4);
        assert_eq!(c.status, "Below target");
    }

    #[test]
    fn small_cells_are_hidden_with_their_complements() {
        let c = compute(&spec(MeasureKind::Count, 100.0), &series(&[20.0, 3.0, 30.0], &[]), 5, None, 0.9).unwrap();
        assert_eq!(c.hide, vec![false, true, false]);
        // A percent whose numerator is large but whose remainder is small is hidden too.
        let c = compute(&spec(MeasureKind::Percent, 90.0), &series(&[47.0, 20.0], &[50.0, 40.0]), 5, None, 0.9).unwrap();
        assert_eq!(c.hide, vec![true, false]);
        // Zero is not small.
        let c = compute(&spec(MeasureKind::Count, 10.0), &series(&[0.0, 12.0], &[]), 5, None, 0.9).unwrap();
        assert_eq!(c.hide, vec![false, false]);
    }

    #[test]
    fn a_hidden_period_withholds_totals_progress_and_center() {
        let periods: Vec<String> = (1..=8).map(|i| format!("P{i}")).collect();
        let c = compute(
            &spec(MeasureKind::Count, 100.0),
            &series(&[20.0, 3.0, 30.0, 25.0, 22.0, 28.0, 31.0, 26.0], &[]),
            5,
            Some(8),
            0.9,
        )
        .unwrap();
        let r = to_result(&c, &periods, None);
        assert!(r.withheld);
        assert_eq!(r.current, None);
        assert_eq!(r.progress_percent, None);
        assert_eq!(r.trend.center, None);
        assert!(r.status.starts_with("Not shown"));
        assert_eq!(r.periods[1].value, None);
        assert!(r.periods[1].suppressed);
        assert_eq!(r.periods[0].value, Some(20.0));
    }

    #[test]
    fn run_chart_flags_a_sustained_shift() {
        // Median 20; the two points on it are ignored. Five low points then five high ones
        // cross it once, fewer than the 5th percentile of Binomial(9, 0.5), which is 2.
        let nums: Vec<f64> = vec![10.0, 12.0, 9.0, 11.0, 10.0, 20.0, 21.0, 22.0, 20.0, 21.0, 23.0, 24.0];
        let c = compute(&spec(MeasureKind::Count, 100.0), &series(&nums, &[]), 5, None, 0.9).unwrap();
        let t = c.trend.as_ref().unwrap();
        assert!(t.points.iter().any(|p| !p.signals.is_empty()));
        assert_eq!(c.chart_type, Some(ChartType::Run));
    }

    #[test]
    fn each_period_basis_counts_periods_meeting_the_target() {
        let mut s = spec(MeasureKind::Count, 25.0);
        s.basis = Basis::EachPeriod;
        let c = compute(&s, &series(&[30.0, 20.0, 26.0], &[]), 5, None, 0.9).unwrap();
        assert_eq!(c.meeting, Some(2));
        assert_eq!(c.status, "Met");
        assert_eq!(c.latest, Some(26.0));
    }

    #[test]
    fn text_periods_sort_by_year_and_quarter_or_month() {
        let mut v = vec!["2026 Q1", "FY25 Q3", "2025-Q4", "Q2 2026"];
        v.sort_by_key(|l| text_period_key(l).unwrap());
        assert_eq!(v, vec!["FY25 Q3", "2025-Q4", "2026 Q1", "Q2 2026"]);
        let mut m = vec!["Mar 2026", "January 2026", "2025-12"];
        m.sort_by_key(|l| text_period_key(l).unwrap());
        assert_eq!(m, vec!["2025-12", "January 2026", "Mar 2026"]);
        assert_eq!(text_period_key("Spring cohort"), None);
    }

    #[test]
    fn invalid_targets_are_errors() {
        let mut a = PerformanceArgs {
            path: "x.csv".into(),
            period_column: "Month".into(),
            ..Default::default()
        };
        assert!(build_specs(&a).unwrap_err().to_string().contains("at least one target"));
        a.targets = vec![TargetArg {
            measure: "Clients".into(),
            target: None,
            ..Default::default()
        }];
        assert!(build_specs(&a).unwrap_err().to_string().contains("needs a target"));
        a.targets[0].target = Some(10.0);
        a.targets[0].kind = Some(MeasureKind::Percent);
        assert!(build_specs(&a).unwrap_err().to_string().contains("denominator"));
        a.targets[0].kind = Some(MeasureKind::Count);
        a.targets[0].denominator = Some("Eligible".into());
        assert!(build_specs(&a).unwrap_err().to_string().contains("set kind to percent"));
    }
}
