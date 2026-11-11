//! CD-08 `get_notifiable_disease_trends`: NNDSS weekly counts for
//! Massachusetts, New England, and the United States, compared with the same
//! point in the five previous years.
//!
//! Data: CDC's NNDSS Weekly Data (data.cdc.gov `x9gk-5huc`, MMWR years 2022
//! onward), through the live → cache → bundled-snapshot chain.
//!
//! Method (also returned in `data.method`):
//! * YTD(y, w) is the cumulative count for MMWR year y through week w, as
//!   published that week (column m3; "-" = 0). For a year before the first in
//!   the dataset, the previous-year column (m4) of the next year's row is
//!   used. Week 53 is read as week 52 in 52-week years.
//! * Last 4 weeks: YTD(y, w) − YTD(y, w − 4), the cases added to the
//!   cumulative total in the 4 weeks ending week w (weeks 1–w when w ≤ 4).
//! * Baseline: the same quantity in each of the five previous MMWR years;
//!   mean, minimum, and maximum. It needs at least 3 of the 5 years.
//! * Year-end totals: YTD at the last week of each previous year (provisional,
//!   as published at year end).

use std::collections::{BTreeMap, BTreeSet};

use phn_core::text::{normalize, similarity};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::socrata::{self, SocrataQuery, SocrataRows, soql_string};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::surveillance::{fallback_caveat, mmwr_week_end, mmwr_weeks_in_year, snapshot_fallback};
use crate::context::Context;
use crate::md;

/// NNDSS Weekly Data on data.cdc.gov.
pub const NNDSS_DATASET: &str = "x9gk-5huc";
/// First MMWR year in the weekly dataset.
pub const FIRST_YEAR: i32 = 2022;
const SNAPSHOT_ID: &str = "cdc-nndss-weekly";
const SNAPSHOT_FILE: &str = "nndss_weekly.json";
/// The dataset is updated weekly; cache for six hours.
const TTL_SECS: u64 = 6 * 3600;
const MAX_AREAS: usize = 6;
const MAX_LABELS: usize = 12;
const ANNUAL_TABLES_URL: &str =
    "https://www.cdc.gov/nndss/infectious-disease/weekly-and-annual-disease-data-tables.html";

const METHOD: &str = "YTD(y, w) is the cumulative count for MMWR year y through week w as \
    published that week (NNDSS column m3; '-' counts as 0); for the year before the first in the \
    dataset, the previous-year column (m4) of the next year's row is used, and week 53 is read \
    as week 52 in 52-week years. Last 4 weeks = YTD(y, w) - YTD(y, w - 4), the cases added to \
    the cumulative total in the 4 weeks ending week w (weeks 1 to w when w <= 4). The baseline \
    is the same quantity in each of the five previous MMWR years (mean, minimum, maximum; at \
    least 3 of the 5 years needed). Ratio = current / baseline mean. 'Above' or 'below the \
    5-year range' means outside the baseline minimum-maximum. Year-end totals are YTD at the \
    last week of each previous year (provisional, as published at year end).";

/// Arguments for `get_notifiable_disease_trends`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct NotifiableTrendsArgs {
    /// Condition as named in NNDSS, or part of the name (e.g. "pertussis", "measles", "hepatitis A", "salmonellosis"). Every NNDSS row that matches is shown (up to 12). Omit for an overview of all conditions in the first area.
    pub condition: Option<String>,
    /// Reporting areas (default Massachusetts, New England, and United States). A state name, an NNDSS region such as "New England", or "United States".
    pub areas: Vec<String>,
    /// MMWR year (default: the latest published).
    pub year: Option<i32>,
    /// MMWR week, 1-53 (default: the latest published for the year). Needs `year`.
    pub week: Option<u32>,
    /// Overview only: how many conditions to list (default 25, at most 150).
    pub limit: Option<usize>,
}

/// A count for one year, or why it is missing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct YearCount {
    pub year: i32,
    /// The count; null when unavailable.
    pub count: Option<f64>,
    /// Why the count is missing (CDC flag meaning or "no data").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One week's count (current-week column).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct WeekCount {
    pub week: u32,
    /// Saturday ending the MMWR week.
    pub week_ending: String,
    pub count: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A current value compared with the five previous years.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Comparison {
    /// Current-year value; null when unavailable.
    pub current: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_note: Option<String>,
    /// The same quantity in each of the five previous years.
    pub baseline: Vec<YearCount>,
    /// Years with a value (of 5).
    pub baseline_years: usize,
    pub baseline_mean: Option<f64>,
    pub baseline_min: Option<f64>,
    pub baseline_max: Option<f64>,
    /// Current / baseline mean.
    pub ratio_to_mean: Option<f64>,
    /// "above 5-year range", "within 5-year range", "below 5-year range", or "no baseline".
    pub assessment: String,
}

/// One NNDSS condition row in one area.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ConditionTrend {
    /// NNDSS condition label.
    pub condition: String,
    pub area: String,
    /// Cases reported in the current week (column m1).
    pub current_week: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_week_note: Option<String>,
    /// Current-week counts for the latest 8 weeks.
    pub recent_weeks: Vec<WeekCount>,
    /// Year to date.
    pub year_to_date: Comparison,
    /// CDC's previous-year count for the same weeks (column m4 of the current week's row). It
    /// includes reports received since, so it is often higher than the as-published baseline value.
    pub cdc_revised_previous_year_ytd: Option<f64>,
    /// Last 4 weeks.
    pub last_4_weeks: Comparison,
    /// Provisional year-end totals for the five previous years.
    pub year_end_totals: Vec<YearCount>,
}

/// Overview row (one condition, first area).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct OverviewRow {
    pub condition: String,
    pub year_to_date: Comparison,
    pub last_4_weeks: Comparison,
}

/// Result of `get_notifiable_disease_trends`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct NotifiableTrendsResult {
    /// "condition" or "overview".
    pub mode: String,
    pub mmwr_year: i32,
    pub mmwr_week: u32,
    /// Saturday ending the MMWR week.
    pub week_ending: String,
    pub areas: Vec<String>,
    /// Condition mode: one entry per matching NNDSS row and area.
    pub trends: Vec<ConditionTrend>,
    /// Overview mode: conditions in the first area, those above the 5-year range first.
    pub overview: Vec<OverviewRow>,
    /// Overview mode: conditions with cases this year or in the baseline (before the limit).
    pub overview_conditions: usize,
    /// How the comparisons are computed.
    pub method: String,
}

// ---------------------------------------------------------------- areas

#[derive(Debug, Clone, PartialEq)]
struct Area {
    display: String,
    /// Upper-case `states` values for this area (NNDSS uses both cases).
    keys: Vec<String>,
}

fn resolve_area(input: &str) -> Result<Area> {
    let t = input.trim();
    let n = normalize(t);
    let area = |d: &str, keys: &[&str]| Area {
        display: d.into(),
        keys: keys.iter().map(|k| k.to_string()).collect(),
    };
    Ok(match n.as_str() {
        "massachusetts" | "ma" | "mass" => area("Massachusetts", &["MASSACHUSETTS"]),
        "new england" | "ne" => area("New England", &["NEW ENGLAND"]),
        "united states" | "us" | "u s" | "usa" | "national" | "nation" | "u s residents"
        | "us residents" => area("United States", &["U.S. RESIDENTS", "US RESIDENTS"]),
        _ => {
            if t.len() < 2
                || t.len() > 40
                || !t
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || " .'-".contains(c))
            {
                return Err(PhnError::InvalidInput(format!(
                    "'{input}' is not a reporting area. Use a state name, an NNDSS region \
                     (e.g. New England, Middle Atlantic), or United States."
                )));
            }
            let display = t
                .split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ");
            Area {
                display,
                keys: vec![t.to_ascii_uppercase()],
            }
        }
    })
}

fn area_where(areas: &[Area]) -> String {
    let keys: Vec<String> = areas
        .iter()
        .flat_map(|a| a.keys.iter().map(|k| soql_string(k)))
        .collect();
    format!("upper(states) in ({})", keys.join(","))
}

fn area_of(areas: &[Area], states: &str) -> Option<usize> {
    let up = states.trim().to_ascii_uppercase();
    areas.iter().position(|a| a.keys.contains(&up))
}

// ---------------------------------------------------------------- matching

const ALIASES: &[(&str, &str)] = &[
    ("whooping cough", "pertussis"),
    ("chickenpox", "varicella"),
    ("chicken pox", "varicella"),
    ("tb", "tuberculosis"),
    ("e coli", "escherichia coli"),
    ("ecoli", "escherichia coli"),
    ("stec", "shiga toxin"),
    ("hep a", "hepatitis a"),
    ("hep b", "hepatitis b"),
    ("hep c", "hepatitis c"),
    ("hav", "hepatitis a"),
    ("hbv", "hepatitis b"),
    ("hcv", "hepatitis c"),
    ("ipd", "invasive pneumococcal"),
    ("hib", "haemophilus influenzae"),
    ("h flu", "haemophilus influenzae"),
    ("gc", "gonorrhea"),
    ("flu", "influenza"),
    ("wnv", "west nile"),
    ("eee", "eastern equine"),
    ("monkeypox", "mpox"),
    ("c auris", "candida auris"),
    ("cre", "carbapenem"),
    ("cpo", "carbapenemase producing organisms"),
    ("covid", "covid 19"),
    ("hansen", "hansens"),
    ("leprosy", "hansens"),
];

/// True when every token of `query` matches a token of `label` (short tokens
/// exactly, longer tokens as a prefix).
fn label_matches(query: &str, label: &str) -> bool {
    let l: Vec<&str> = label.split(' ').collect();
    query.split(' ').filter(|t| !t.is_empty()).all(|q| {
        l.iter().any(|t| {
            if q.len() <= 2 {
                *t == q
            } else {
                t.starts_with(q)
            }
        })
    })
}

fn match_labels(
    condition: &str,
    labels: &[String],
) -> std::result::Result<Vec<String>, Vec<String>> {
    let mut q = normalize(condition);
    for (a, b) in ALIASES {
        if q == *a || q.starts_with(&format!("{a} ")) {
            q = format!("{b}{}", &q[a.len()..]);
            break;
        }
    }
    let mut hits: Vec<String> = labels
        .iter()
        .filter(|l| label_matches(&q, &normalize(l)))
        .cloned()
        .collect();
    hits.sort();
    hits.dedup();
    if !hits.is_empty() {
        return Ok(hits);
    }
    let mut scored: Vec<(f64, &String)> = labels
        .iter()
        .map(|l| (similarity(&q, &normalize(l)), l))
        .filter(|(s, _)| *s >= 0.75)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    Err(scored.into_iter().take(5).map(|(_, l)| l.clone()).collect())
}

// ---------------------------------------------------------------- cells

#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Num(f64),
    Flag(String),
    Missing,
}

fn flag_meaning(flag: &str) -> String {
    match flag {
        "U" => {
            "U: unavailable (the jurisdiction could not send the data, or CDC could not process it)"
                .into()
        }
        "N" => "N: not reportable in the jurisdiction".into(),
        "NN" => "NN: not nationally notifiable".into(),
        "NP" => "NP: nationally notifiable but not published".into(),
        "NC" => "NC: not calculated (insufficient data)".into(),
        other => format!("{other}: CDC flag"),
    }
}

fn cell(row: &Map<String, Value>, field: &str) -> Cell {
    if let Some(x) = socrata::num(row, field) {
        return Cell::Num(x);
    }
    match socrata::text(row, &format!("{field}_flag")) {
        Some("-") => Cell::Num(0.0),
        Some(f) => Cell::Flag(f.to_string()),
        None => Cell::Missing,
    }
}

fn value(c: &Cell) -> std::result::Result<f64, String> {
    match c {
        Cell::Num(x) => Ok(*x),
        Cell::Flag(f) => Err(flag_meaning(f)),
        Cell::Missing => Err("no data".into()),
    }
}

/// (m1, m3, m4) by (year, week) for one condition in one area.
type Series = BTreeMap<(i32, u32), (Cell, Cell, Cell)>;

fn ytd(s: &Series, y: i32, w: u32) -> std::result::Result<f64, String> {
    let w = w.min(mmwr_weeks_in_year(y));
    if let Some((_, m3, _)) = s.get(&(y, w)) {
        return value(m3);
    }
    if let Some((_, _, m4)) = s.get(&(y + 1, w)) {
        return value(m4);
    }
    Err("no data".into())
}

fn last4(s: &Series, y: i32, w: u32) -> std::result::Result<f64, String> {
    // Week 53 is week 52 in a 52-week year; clamp before subtracting so the
    // window is 4 weeks long in every year (not 3 in a baseline year).
    let w = w.min(mmwr_weeks_in_year(y));
    if w <= 4 {
        return ytd(s, y, w);
    }
    Ok(ytd(s, y, w)? - ytd(s, y, w - 4)?)
}

fn compare(
    current: std::result::Result<f64, String>,
    baseline: Vec<(i32, std::result::Result<f64, String>)>,
) -> Comparison {
    let base: Vec<YearCount> = baseline
        .into_iter()
        .map(|(year, v)| match v {
            Ok(c) => YearCount {
                year,
                count: Some(c),
                note: None,
            },
            Err(e) => YearCount {
                year,
                count: None,
                note: Some(e),
            },
        })
        .collect();
    let vals: Vec<f64> = base.iter().filter_map(|b| b.count).collect();
    let n = vals.len();
    let (mean, min, max) = if n >= 3 {
        (
            Some(vals.iter().sum::<f64>() / n as f64),
            vals.iter().copied().reduce(f64::min),
            vals.iter().copied().reduce(f64::max),
        )
    } else {
        (None, None, None)
    };
    let (cur, cur_note) = match current {
        Ok(c) => (Some(c), None),
        Err(e) => (None, Some(e)),
    };
    let ratio = match (cur, mean) {
        (Some(c), Some(m)) if m > 0.0 => Some(c / m),
        _ => None,
    };
    let assessment = match (cur, min, max) {
        (Some(c), Some(lo), Some(hi)) => {
            if c > hi {
                "above 5-year range"
            } else if c < lo {
                "below 5-year range"
            } else {
                "within 5-year range"
            }
        }
        (None, _, _) => "no current value",
        _ => "no baseline",
    }
    .to_string();
    Comparison {
        current: cur,
        current_note: cur_note,
        baseline: base,
        baseline_years: n,
        baseline_mean: mean,
        baseline_min: min,
        baseline_max: max,
        ratio_to_mean: ratio,
        assessment,
    }
}

fn trend(label: &str, area: &str, s: &Series, year: i32, week: u32) -> ConditionTrend {
    let prev: Vec<i32> = (year - 5..year).collect();
    let (cw, cw_note) = match s.get(&(year, week)).map(|r| value(&r.0)) {
        Some(Ok(v)) => (Some(v), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, Some("no data".to_string())),
    };
    let recent_weeks = (week.saturating_sub(7).max(1)..=week)
        .map(|w| {
            let v = s
                .get(&(year, w))
                .map(|r| value(&r.0))
                .unwrap_or_else(|| Err("no data".into()));
            WeekCount {
                week: w,
                week_ending: mmwr_week_end(year, w).to_string(),
                count: v.as_ref().ok().copied(),
                note: v.err(),
            }
        })
        .collect();
    ConditionTrend {
        condition: label.to_string(),
        area: area.to_string(),
        current_week: cw,
        current_week_note: cw_note,
        recent_weeks,
        cdc_revised_previous_year_ytd: s.get(&(year, week)).and_then(|r| value(&r.2).ok()),
        year_to_date: compare(
            ytd(s, year, week),
            prev.iter().map(|&y| (y, ytd(s, y, week))).collect(),
        ),
        last_4_weeks: compare(
            last4(s, year, week),
            prev.iter().map(|&y| (y, last4(s, y, week))).collect(),
        ),
        year_end_totals: prev
            .iter()
            .map(|&y| {
                let v = ytd(s, y, mmwr_weeks_in_year(y));
                YearCount {
                    year: y,
                    count: v.as_ref().ok().copied(),
                    note: v.err(),
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- snapshot

#[derive(Deserialize)]
struct SnapLatest {
    year: i32,
    week: u32,
}

#[derive(Deserialize)]
struct Snap {
    latest: SnapLatest,
    areas: Vec<String>,
    labels: Vec<String>,
    rows: Vec<(usize, i32, u32, usize, String, String, String)>,
}

fn snap(text: &str) -> Option<Snap> {
    serde_json::from_str(text).ok()
}

fn json_body(rows: Vec<Value>) -> Option<Vec<u8>> {
    serde_json::to_vec(&Value::Array(rows)).ok()
}

/// Expand a compact snapshot value into Socrata fields.
fn put(obj: &mut Map<String, Value>, field: &str, v: &str) {
    match v {
        "?" => {}
        "" => {
            obj.insert(format!("{field}_flag"), json!("-"));
        }
        _ if v.parse::<f64>().is_ok() => {
            obj.insert(field.into(), json!(v));
        }
        _ => {
            obj.insert(format!("{field}_flag"), json!(v));
        }
    }
}

// ---------------------------------------------------------------- queries

struct Fetch {
    rows: SocrataRows,
}

async fn run(
    ctx: &Context,
    q: &SocrataQuery,
    fallback: phn_data::fetch::SnapshotFallback,
) -> Result<Fetch> {
    let rows = socrata::query(&ctx.http, ctx.cache.as_ref(), q, TTL_SECS, Some(fallback)).await?;
    Ok(Fetch { rows })
}

fn fmt_count(x: f64) -> String {
    if x.fract() == 0.0 {
        let s = format!("{}", x.abs() as i64);
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        if x < 0.0 { format!("-{out}") } else { out }
    } else {
        md::num(x, 1)
    }
}

fn opt(x: Option<f64>) -> String {
    x.map_or_else(|| "–".into(), fmt_count)
}

fn mean_range(c: &Comparison) -> String {
    match (c.baseline_mean, c.baseline_min, c.baseline_max) {
        (Some(m), Some(lo), Some(hi)) => {
            format!("{} ({}–{})", md::num(m, 1), fmt_count(lo), fmt_count(hi))
        }
        _ => format!("not computed ({} of 5 years)", c.baseline_years),
    }
}

fn ratio(c: &Comparison) -> String {
    c.ratio_to_mean
        .map_or_else(|| "–".into(), |r| md::num(r, 2))
}

/// Pull NNDSS counts and compare them with the five previous years.
pub async fn get_notifiable_disease_trends(
    ctx: &Context,
    args: NotifiableTrendsArgs,
) -> Result<Envelope<NotifiableTrendsResult>> {
    let areas: Vec<Area> = if args.areas.is_empty() {
        vec![
            resolve_area("Massachusetts")?,
            resolve_area("New England")?,
            resolve_area("United States")?,
        ]
    } else {
        let mut v: Vec<Area> = Vec::new();
        for a in &args.areas {
            let r = resolve_area(a)?;
            if !v.contains(&r) {
                v.push(r);
            }
        }
        v
    };
    if areas.len() > MAX_AREAS {
        return Err(PhnError::InvalidInput(format!(
            "give at most {MAX_AREAS} reporting areas"
        )));
    }
    if args.week.is_some() && args.year.is_none() {
        return Err(PhnError::InvalidInput("give year with week".into()));
    }
    if let Some(w) = args.week
        && !(1..=53).contains(&w)
    {
        return Err(PhnError::InvalidInput(format!(
            "week must be 1-53 (got {w})"
        )));
    }
    let this_year = chrono::Datelike::year(&chrono::Local::now().date_naive());
    if let Some(y) = args.year
        && !(FIRST_YEAR..=this_year).contains(&y)
    {
        return Err(PhnError::InvalidInput(format!(
            "year must be {FIRST_YEAR}-{this_year}: the NNDSS weekly dataset starts in {FIRST_YEAR}"
        )));
    }
    let condition = args
        .condition
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    if condition.as_ref().is_some_and(|c| c.len() > 120) {
        return Err(PhnError::InvalidInput("condition is too long".into()));
    }
    let limit = args.limit.unwrap_or(25).clamp(1, 150);
    let mut caveats: Vec<String> = Vec::new();
    let mut fetches: Vec<SocrataRows> = Vec::new();

    // 1. MMWR year and week.
    let (year, week) = match (args.year, args.week) {
        (Some(y), Some(w)) => {
            if w > mmwr_weeks_in_year(y) {
                return Err(PhnError::InvalidInput(format!(
                    "MMWR {y} has {} weeks",
                    mmwr_weeks_in_year(y)
                )));
            }
            (y, w)
        }
        _ => {
            let mut q = SocrataQuery::cdc(NNDSS_DATASET)
                .select("year, week")
                .and_where(area_where(&areas[..1]))
                .order("year DESC, week DESC")
                .limit(1);
            if let Some(y) = args.year {
                q = q.and_where(format!("year = {}", soql_string(&y.to_string())));
            }
            let want_year = args.year;
            let fb = snapshot_fallback(ctx, SNAPSHOT_ID, SNAPSHOT_FILE, move |t| {
                let s = snap(t)?;
                let rows = if want_year.is_none_or(|y| y == s.latest.year) {
                    vec![
                        json!({"year": s.latest.year.to_string(), "week": s.latest.week.to_string()}),
                    ]
                } else {
                    vec![]
                };
                json_body(rows)
            });
            let f = run(ctx, &q, fb).await?;
            let first = f.rows.rows.first().ok_or_else(|| {
                PhnError::NotFound(match args.year {
                    Some(y) => format!(
                        "no NNDSS weekly data for {} in {y}{}",
                        areas[0].display,
                        if f.rows.mode == phn_core::SourceMode::Snapshot {
                            " in the bundled snapshot (it holds only the latest week; connect to the internet for other weeks)"
                        } else {
                            ""
                        }
                    ),
                    None => format!("no NNDSS weekly data for {}", areas[0].display),
                })
            })?;
            let y = socrata::text(first, "year")
                .and_then(|v| v.parse::<i32>().ok())
                .ok_or_else(|| PhnError::Network("NNDSS returned no MMWR year".into()))?;
            let w = socrata::num(first, "week")
                .map(|v| v as u32)
                .ok_or_else(|| PhnError::Network("NNDSS returned no MMWR week".into()))?;
            fetches.push(f.rows);
            (y, w)
        }
    };
    let years: Vec<i32> = (year - 4..=year).filter(|y| *y >= FIRST_YEAR).collect();
    let year_list = years
        .iter()
        .map(|y| soql_string(&y.to_string()))
        .collect::<Vec<_>>()
        .join(",");

    // 2. Condition labels.
    let selected: Option<Vec<String>> = match &condition {
        None => None,
        Some(c) => {
            let q = SocrataQuery::cdc(NNDSS_DATASET)
                .select("label")
                .and_where(area_where(&areas))
                .and_where(format!("year in ({year_list})"))
                .group("label")
                .order("label")
                .limit(2_000);
            let fb = snapshot_fallback(ctx, SNAPSHOT_ID, SNAPSHOT_FILE, |t| {
                let s = snap(t)?;
                json_body(s.labels.iter().map(|l| json!({"label": l})).collect())
            });
            let f = run(ctx, &q, fb).await?;
            let labels: Vec<String> = f
                .rows
                .rows
                .iter()
                .filter_map(|r| socrata::text(r, "label").map(str::to_string))
                .collect();
            fetches.push(f.rows);
            match match_labels(c, &labels) {
                Ok(mut hits) => {
                    if hits.len() > MAX_LABELS {
                        caveats.push(format!(
                            "'{c}' matched {} NNDSS rows; the first {MAX_LABELS} are shown. Use a more specific name.",
                            hits.len()
                        ));
                        hits.truncate(MAX_LABELS);
                    }
                    Some(hits)
                }
                Err(suggestions) => {
                    let hint = if suggestions.is_empty() {
                        String::new()
                    } else {
                        format!(" Closest NNDSS rows: {}.", suggestions.join("; "))
                    };
                    return Err(PhnError::NotFound(format!(
                        "no NNDSS weekly row matches '{c}'.{hint} Some conditions (for example \
                         Lyme disease) are not in the weekly tables; see CDC's annual tables at \
                         {ANNUAL_TABLES_URL}. Massachusetts reporting requirements: use \
                         lookup_reportable_condition."
                    )));
                }
            }
        }
    };

    // 3. Counts.
    let mut weeks: BTreeSet<u32> = BTreeSet::new();
    weeks.insert(week);
    if week > 4 {
        weeks.insert(week - 4);
    }
    if week == 53 {
        weeks.insert(52);
        if week > 4 {
            weeks.insert(48);
        }
    }
    let row_areas: Vec<Area> = if selected.is_some() {
        weeks.extend(week.saturating_sub(7).max(1)..=week);
        weeks.insert(52);
        weeks.insert(53);
        areas.clone()
    } else {
        areas[..1].to_vec()
    };
    let week_list = weeks
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut q = SocrataQuery::cdc(NNDSS_DATASET)
        .select("states, year, week, label, m1, m1_flag, m3, m3_flag, m4, m4_flag")
        .and_where(area_where(&row_areas))
        .and_where(format!("year in ({year_list})"))
        .and_where(format!("week in ({week_list})"))
        .order("year, week, label")
        .limit(20_000);
    if let Some(labels) = &selected {
        q = q.and_where(format!(
            "label in ({})",
            labels
                .iter()
                .map(|l| soql_string(l))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    let snap_areas = row_areas.clone();
    let snap_labels = selected.clone();
    let snap_years: BTreeSet<i32> = years.iter().copied().collect();
    let snap_weeks = weeks.clone();
    let fb = snapshot_fallback(ctx, SNAPSHOT_ID, SNAPSHOT_FILE, move |t| {
        let s = snap(t)?;
        let mut out = Vec::new();
        for (a, y, w, l, m1, m3, m4) in &s.rows {
            let (Some(area), Some(label)) = (s.areas.get(*a), s.labels.get(*l)) else {
                continue;
            };
            if area_of(&snap_areas, area).is_none()
                || !snap_years.contains(y)
                || !snap_weeks.contains(w)
                || snap_labels.as_ref().is_some_and(|ls| !ls.contains(label))
            {
                continue;
            }
            let mut o = Map::new();
            o.insert("states".into(), json!(area));
            o.insert("year".into(), json!(y.to_string()));
            o.insert("week".into(), json!(w.to_string()));
            o.insert("label".into(), json!(label));
            put(&mut o, "m1", m1);
            put(&mut o, "m3", m3);
            put(&mut o, "m4", m4);
            out.push(Value::Object(o));
        }
        json_body(out)
    });
    let f = run(ctx, &q, fb).await?;
    let counts = f.rows;
    if counts.truncated {
        caveats
            .push("NNDSS returned the maximum number of rows; some counts may be missing.".into());
    }
    // (area index, label) -> series
    let mut series: BTreeMap<(usize, String), Series> = BTreeMap::new();
    for r in &counts.rows {
        let (Some(states), Some(label)) = (socrata::text(r, "states"), socrata::text(r, "label"))
        else {
            continue;
        };
        let Some(ai) = area_of(&row_areas, states) else {
            continue;
        };
        let (Some(y), Some(w)) = (
            socrata::text(r, "year").and_then(|v| v.parse::<i32>().ok()),
            socrata::num(r, "week").map(|v| v as u32),
        ) else {
            continue;
        };
        series
            .entry((ai, label.to_string()))
            .or_default()
            .insert((y, w), (cell(r, "m1"), cell(r, "m3"), cell(r, "m4")));
    }
    let mode_snapshot = counts.mode == phn_core::SourceMode::Snapshot;
    if let Some(c) = fallback_caveat(ctx, &counts, "NNDSS", SNAPSHOT_ID) {
        caveats.push(c);
    }
    fetches.push(counts);

    let week_ending = mmwr_week_end(year, week).to_string();
    let mut trends = Vec::new();
    let mut overview = Vec::new();
    let mut overview_conditions = 0;
    let mut s = String::new();
    match &selected {
        Some(labels) => {
            s.push_str(&format!(
                "**NNDSS weekly data, MMWR {year} week {week} (week ending {week_ending})**: \
                 {} in {}. Provisional counts.\n",
                labels.join("; "),
                areas
                    .iter()
                    .map(|a| a.display.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            for label in labels {
                let mut rows_ytd = Vec::new();
                let mut rows_4 = Vec::new();
                let mut rows_year = Vec::new();
                let mut label_trends = Vec::new();
                for (ai, area) in areas.iter().enumerate() {
                    let Some(ser) = series.get(&(ai, label.clone())) else {
                        let why = if mode_snapshot {
                            "no rows in the bundled snapshot (it omits series with no reported cases)"
                        } else {
                            "no rows for this area"
                        };
                        rows_ytd.push(vec![
                            area.display.clone(),
                            why.into(),
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]);
                        continue;
                    };
                    let t = trend(label, &area.display, ser, year, week);
                    rows_ytd.push(vec![
                        area.display.clone(),
                        t.current_week
                            .map_or_else(|| short_note(t.current_week_note.as_deref()), fmt_count),
                        t.year_to_date.current.map_or_else(
                            || short_note(t.year_to_date.current_note.as_deref()),
                            fmt_count,
                        ),
                        mean_range(&t.year_to_date),
                        ratio(&t.year_to_date),
                        t.year_to_date.assessment.clone(),
                    ]);
                    rows_4.push(vec![
                        area.display.clone(),
                        t.last_4_weeks.current.map_or_else(
                            || short_note(t.last_4_weeks.current_note.as_deref()),
                            fmt_count,
                        ),
                        mean_range(&t.last_4_weeks),
                        ratio(&t.last_4_weeks),
                        t.last_4_weeks.assessment.clone(),
                    ]);
                    let mut yr = vec![area.display.clone()];
                    yr.extend(t.year_end_totals.iter().map(|y| {
                        y.count
                            .map_or_else(|| short_note(y.note.as_deref()), fmt_count)
                    }));
                    rows_year.push(yr);
                    label_trends.push(t);
                }
                s.push_str(&format!(
                    "\n### {label}\n\nYear to date (weeks 1–{week}):\n\n"
                ));
                s.push_str(&md::table(
                    &[
                        "Area",
                        "This week",
                        &format!("YTD {year}"),
                        "5-yr YTD mean (range)",
                        "Ratio",
                        "Compared with 5 years",
                    ],
                    &rows_ytd,
                ));
                let revised: Vec<String> = label_trends
                    .iter()
                    .filter_map(|t| {
                        t.cdc_revised_previous_year_ytd
                            .map(|v| format!("{} {}", t.area, fmt_count(v)))
                    })
                    .collect();
                if !revised.is_empty() {
                    s.push_str(&format!(
                        "\nCDC's revised {} count for weeks 1–{week} (includes later reports): {}.\n",
                        year - 1,
                        revised.join(", ")
                    ));
                }
                if !rows_4.is_empty() {
                    let first_w = week.saturating_sub(3).max(1);
                    s.push_str(&format!("\nLast 4 weeks (weeks {first_w}–{week}):\n\n"));
                    s.push_str(&md::table(
                        &[
                            "Area",
                            "Cases",
                            "5-yr mean (range)",
                            "Ratio",
                            "Compared with 5 years",
                        ],
                        &rows_4,
                    ));
                    let mut headers: Vec<String> = vec!["Area".into()];
                    headers.extend((year - 5..year).map(|y| y.to_string()));
                    let h: Vec<&str> = headers.iter().map(String::as_str).collect();
                    s.push_str("\nProvisional year-end totals:\n\n");
                    s.push_str(&md::table(&h, &rows_year));
                }
                if let Some(t) = label_trends.first() {
                    s.push_str(&format!(
                        "\nWeekly counts, {} ({year}): {}\n",
                        t.area,
                        t.recent_weeks
                            .iter()
                            .map(|w| format!(
                                "wk {} {}",
                                w.week,
                                w.count.map_or_else(|| "–".into(), fmt_count)
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                trends.extend(label_trends);
            }
        }
        None => {
            let area = &areas[0];
            let mut rows: Vec<OverviewRow> = series
                .iter()
                .filter(|((ai, _), _)| *ai == 0)
                .map(|((_, label), ser)| {
                    let t = trend(label, &area.display, ser, year, week);
                    OverviewRow {
                        condition: label.clone(),
                        year_to_date: t.year_to_date,
                        last_4_weeks: t.last_4_weeks,
                    }
                })
                .filter(|r| {
                    r.year_to_date.current.is_some_and(|c| c > 0.0)
                        || r.year_to_date.baseline_mean.is_some_and(|m| m > 0.0)
                })
                .collect();
            overview_conditions = rows.len();
            let rank = |c: &Comparison| match c.assessment.as_str() {
                "above 5-year range" => 0,
                "within 5-year range" => 1,
                "below 5-year range" => 2,
                _ => 3,
            };
            rows.sort_by(|a, b| {
                rank(&a.year_to_date)
                    .cmp(&rank(&b.year_to_date))
                    .then_with(|| {
                        b.year_to_date
                            .ratio_to_mean
                            .unwrap_or(0.0)
                            .total_cmp(&a.year_to_date.ratio_to_mean.unwrap_or(0.0))
                    })
                    .then_with(|| a.condition.cmp(&b.condition))
            });
            let above = rows
                .iter()
                .filter(|r| r.year_to_date.assessment == "above 5-year range")
                .count();
            rows.truncate(limit);
            s.push_str(&format!(
                "**NNDSS weekly data for {}, MMWR {year} week {week} (week ending \
                 {week_ending})**: {overview_conditions} conditions with cases this year or in \
                 the 5-year baseline; {above} above their 5-year range year to date. \
                 Provisional counts. Showing {} (above the range first, then by ratio).\n\n",
                area.display,
                rows.len()
            ));
            let table: Vec<Vec<String>> = rows
                .iter()
                .map(|r| {
                    vec![
                        r.condition.clone(),
                        opt(r.year_to_date.current),
                        r.year_to_date
                            .baseline_mean
                            .map_or_else(|| "–".into(), |m| md::num(m, 1)),
                        ratio(&r.year_to_date),
                        opt(r.last_4_weeks.current),
                        r.last_4_weeks
                            .baseline_mean
                            .map_or_else(|| "–".into(), |m| md::num(m, 1)),
                        r.year_to_date.assessment.clone(),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &[
                    "Condition",
                    &format!("YTD {year}"),
                    "5-yr YTD mean",
                    "Ratio",
                    "Last 4 wk",
                    "5-yr 4-wk mean",
                    "YTD compared with 5 years",
                ],
                &table,
            ));
            s.push_str("\nAsk about one condition (condition argument) for New England and U.S. comparisons and year-end totals.\n");
            overview = rows;
        }
    }

    // Caveats.
    caveats.insert(
        0,
        format!(
            "NNDSS counts are provisional and shown as published each week; CDC revises them, \
             and finalized annual counts can differ (annual tables: {ANNUAL_TABLES_URL})."
        ),
    );
    caveats.insert(
        1,
        "Baseline years use counts as published at the same week of each year, so they carry \
         reporting delays similar to the current year's; later revisions are not included."
            .into(),
    );
    caveats.insert(
        2,
        "Cases are counted by reporting jurisdiction and the MMWR week they were reported to \
         CDC, not by onset date. Massachusetts DPH counts (MAVEN) and DPH reports can differ; \
         use DPH data for local decisions."
            .into(),
    );
    let comps: Vec<&Comparison> = trends
        .iter()
        .flat_map(|t| [&t.year_to_date, &t.last_4_weeks])
        .chain(
            overview
                .iter()
                .flat_map(|o| [&o.year_to_date, &o.last_4_weeks]),
        )
        .collect();
    if comps
        .iter()
        .any(|c| c.current.is_some_and(|x| x < 5.0) || c.baseline_mean.is_some_and(|m| m < 5.0))
    {
        caveats.push(
            "Some counts are under 5, so ratios and ranges are unstable; treat them as a prompt \
             to look closer, not as a finding."
                .into(),
        );
    }
    if comps.iter().any(|c| c.baseline_years < 3) {
        caveats.push(
            "Some NNDSS rows exist only in recent years (CDC renamed conditions or changed case \
             definitions), so no baseline is computed for them. Rows with similar names were not \
             combined."
                .into(),
        );
    }
    if comps.iter().any(|c| {
        c.current.is_some_and(|x| x < 0.0)
            || c.baseline.iter().any(|b| b.count.is_some_and(|x| x < 0.0))
    }) {
        caveats.push(
            "A negative 4-week count means CDC revised the cumulative total down during those \
             weeks."
                .into(),
        );
    }
    let flags: BTreeSet<String> = comps
        .iter()
        .flat_map(|c| {
            c.baseline
                .iter()
                .filter_map(|b| b.note.clone())
                .chain(c.current_note.clone())
        })
        .filter(|n| n.contains(':'))
        .collect();
    if !flags.is_empty() {
        caveats.push(format!(
            "CDC flags in these rows: {}.",
            flags.into_iter().collect::<Vec<_>>().join("; ")
        ));
    }

    let mode =
        super::surveillance::combined_mode(&fetches.iter().map(|f| f.mode).collect::<Vec<_>>());
    let retrieved = fetches
        .last()
        .map(|f| f.as_of.chars().take(10).collect::<String>());
    let source = Source {
        title: format!("NNDSS Weekly Data (data.cdc.gov {NNDSS_DATASET}), MMWR {year} week {week}"),
        publisher: "CDC, Office of Public Health Data, Surveillance, and Technology".into(),
        url: Some(format!("https://data.cdc.gov/d/{NNDSS_DATASET}")),
        effective_date: Some(week_ending.clone()),
        retrieved,
        mode,
    };
    let env = Envelope::new(
        s,
        NotifiableTrendsResult {
            mode: if selected.is_some() {
                "condition"
            } else {
                "overview"
            }
            .into(),
            mmwr_year: year,
            mmwr_week: week,
            week_ending,
            areas: areas.iter().map(|a| a.display.clone()).collect(),
            trends,
            overview,
            overview_conditions,
            method: METHOD.into(),
        },
    )
    .source(source)
    .source(Source::computed(
        "Comparison with the five previous MMWR years (method in data.method)",
        "Public Health Nursing MCP Server",
    ))
    .caveats(caveats)
    .privacy(Privacy {
        notes: vec!["Public aggregate counts from CDC; no local data were used.".into()],
        ..Privacy::none()
    });
    Ok(env)
}

fn short_note(note: Option<&str>) -> String {
    match note {
        Some(n) if n.contains(':') => n.split(':').next().unwrap_or("–").to_string(),
        _ => "–".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(rows: &[(i32, u32, Cell, Cell, Cell)]) -> Series {
        rows.iter()
            .map(|(y, w, a, b, c)| ((*y, *w), (a.clone(), b.clone(), c.clone())))
            .collect()
    }

    #[test]
    fn baseline_uses_m3_and_previous_year_column() {
        use Cell::*;
        // Weeks 43 and 39 in 2022-2026; 2021 from the m4 column of 2022 rows.
        let s = series(&[
            (2022, 39, Num(1.0), Num(30.0), Num(20.0)),
            (2022, 43, Num(2.0), Num(40.0), Num(26.0)),
            (2023, 39, Num(1.0), Num(50.0), Missing),
            (2023, 43, Num(3.0), Num(58.0), Missing),
            (2024, 39, Num(0.0), Num(10.0), Missing),
            (2024, 43, Flag("U".into()), Flag("U".into()), Missing),
            (2025, 39, Num(4.0), Num(70.0), Missing),
            (2025, 43, Num(5.0), Num(90.0), Missing),
            (2026, 39, Num(6.0), Num(100.0), Missing),
            (2026, 43, Num(9.0), Num(130.0), Missing),
        ]);
        let t = trend("Pertussis", "Massachusetts", &s, 2026, 43);
        assert_eq!(t.current_week, Some(9.0));
        assert_eq!(t.cdc_revised_previous_year_ytd, None);
        let y = &t.year_to_date;
        assert_eq!(y.current, Some(130.0));
        // 2021: 26 (m4), 2022: 40, 2023: 58, 2024: U, 2025: 90.
        assert_eq!(y.baseline_years, 4);
        assert_eq!(y.baseline_mean, Some((26.0 + 40.0 + 58.0 + 90.0) / 4.0));
        assert_eq!(y.baseline_max, Some(90.0));
        assert_eq!(y.assessment, "above 5-year range");
        assert!(y.baseline[3].note.as_deref().unwrap().starts_with("U:"));
        // Last 4 weeks: 2026 30; 2021 6; 2022 10; 2023 8; 2025 20.
        let l = &t.last_4_weeks;
        assert_eq!(l.current, Some(30.0));
        assert_eq!(l.baseline_mean, Some((6.0 + 10.0 + 8.0 + 20.0) / 4.0));
        assert_eq!(l.assessment, "above 5-year range");
        // Too few baseline years.
        let short = series(&[
            (2025, 43, Num(1.0), Num(5.0), Num(4.0)),
            (2026, 43, Num(1.0), Num(7.0), Missing),
        ]);
        let t = trend("X", "MA", &short, 2026, 43);
        assert_eq!(t.year_to_date.assessment, "no baseline");
        assert_eq!(t.year_to_date.baseline_years, 2);
    }

    #[test]
    fn labels_match_by_tokens_and_aliases() {
        let labels: Vec<String> = [
            "Hepatitis A, Confirmed",
            "Hepatitis, A, acute",
            "Hepatitis B, acute",
            "Pertussis",
            "Measles, Indigenous",
            "Measles, Imported",
            "Varicella morbidity",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            match_labels("hepatitis A", &labels).unwrap(),
            vec!["Hepatitis A, Confirmed", "Hepatitis, A, acute"]
        );
        assert_eq!(
            match_labels("whooping cough", &labels).unwrap(),
            vec!["Pertussis"]
        );
        assert_eq!(match_labels("measles", &labels).unwrap().len(), 2);
        assert_eq!(
            match_labels("chickenpox", &labels).unwrap(),
            vec!["Varicella morbidity"]
        );
        assert_eq!(
            match_labels("pertusis", &labels).unwrap_err(),
            vec!["Pertussis"]
        );
        assert!(match_labels("zzzz", &labels).unwrap_err().is_empty());
    }

    #[test]
    fn areas_resolve() {
        assert_eq!(resolve_area("MA").unwrap().keys, vec!["MASSACHUSETTS"]);
        assert_eq!(resolve_area("U.S.").unwrap().display, "United States");
        assert_eq!(
            resolve_area("rhode island").unwrap().display,
            "Rhode Island"
        );
        assert!(resolve_area("x'); drop").is_err());
    }

    #[test]
    fn last_four_weeks_is_four_weeks_in_a_52_week_baseline_year() {
        // 2025 has 53 MMWR weeks; 2024 has 52. YTD rises by 10 a week.
        let mut s: Series = BTreeMap::new();
        for w in 40..=52 {
            s.insert(
                (2024, w),
                (
                    Cell::Num(10.0),
                    Cell::Num(10.0 * f64::from(w)),
                    Cell::Missing,
                ),
            );
        }
        // Current week 53 of 2025, read for 2024 as week 52: weeks 49-52.
        assert_eq!(last4(&s, 2024, 53), Ok(40.0));
        assert_eq!(last4(&s, 2024, 52), Ok(40.0));
    }
}
