//! LEAD-01 `analyze_health_department_budget`: analyzes the nurse's own budget or ledger file
//! (CSV/XLSX in /data) and returns aggregates only: year-to-date variance against the share of
//! the fiscal year that has passed, the revenue mix (fees, municipal appropriation, state Public
//! Health Excellence grants, other state, federal, other grants), cost per service from volumes
//! the nurse types, and a year-end forecast by two labelled methods (straight line and recent
//! run rate). A workbook with live formulas and a DOCX summary go to /data/output.
//!
//! Privacy:
//! * Only the columns the nurse names (or that match the content's alias lists) are read. Columns
//!   with vendor, employee, payee, or other person-like headers are refused when named and are
//!   never auto-detected; columns the PHI detector flags are refused by `localdata`.
//! * Results are category totals. A category label and amounts are never taken from a single
//!   line: all salary and wage categories are combined into one personnel row, and when that group
//!   has fewer than three positions its amounts are hidden everywhere (response, workbook, report)
//!   and the totals leave personnel out.
//! * Account codes, vendors, employees, and notes are never returned.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike as _, Months, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_local::phi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::budget_output;
use super::finance_content::{self as fc, FinanceContent, has_any, padded};
use crate::context::Context;
use crate::epr::content::{CitedOut, cite_all};
use crate::localdata::{self, LocalTable};
use crate::md;
use crate::school::templates::DocFormat;

/// Most categories reported.
const MAX_CATEGORIES: usize = 60;
/// Most services whose cost per unit is computed.
const MAX_SERVICES: usize = 20;

// ---------------------------------------------------------------- arguments

/// A fund or source label placed in a revenue group by hand.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct GroupOverride {
    /// The fund or source label exactly as it appears in the file (case does not matter).
    pub label: String,
    /// One of: phe, federal, state_other, fees, municipal, other_grants, other.
    pub group: String,
}

/// One budget category that pays for a service, and the share of it that does.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ServiceCostArg {
    /// A category name from the file (see the list in the error if it is not found). Salary and
    /// wage categories are one combined pool: name "Personnel" (or any one salary category) once.
    pub category: String,
    /// The percent of that category's year-to-date spending that belongs to this service
    /// (default 100). Your allocation, not the tool's.
    pub share_percent: Option<f64>,
}

/// A service whose cost per unit you want.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ServiceArg {
    /// The service, for example "Seasonal influenza clinics". Do not type a person's name.
    pub name: String,
    /// Units delivered so far this fiscal year (visits, doses, inspections, clients). You supply it.
    pub volume: f64,
    /// The categories that pay for it, with your allocation shares.
    pub costs: Vec<ServiceCostArg>,
    /// Planned units for the whole year (optional): adds the budgeted cost per unit.
    pub planned_annual_volume: Option<f64>,
}

/// Arguments for `analyze_health_department_budget`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct BudgetArgs {
    /// CSV or XLSX in /data with the department's budget or ledger: one row per account line
    /// (budget and year-to-date actual), or one row per account line per month (use
    /// `period_column` and `account_column`).
    pub path: String,
    /// Sheet name for an XLSX file (default: the first sheet).
    pub sheet: Option<String>,
    /// Column with the budget category (for example "Salaries", "Supplies", "Inspection Fees").
    /// Default: the first column named like Category, Account Category, Object Class.
    pub category_column: Option<String>,
    /// Column that says Revenue or Expense. Without it every row is treated as an expense and the
    /// revenue mix is skipped.
    pub type_column: Option<String>,
    /// Column with the fund or revenue source (for example "General Fund", "State PHE grant"). Used
    /// to sort revenue into groups.
    pub source_column: Option<String>,
    /// Column with the budgeted (appropriated) amount for the year.
    pub budget_column: Option<String>,
    /// Column with the actual amount: year to date for one row per line, or each month's amount
    /// when `period_column` is given.
    pub actual_column: Option<String>,
    /// Column with encumbrances (money reserved for open orders and contracts), optional.
    pub encumbrance_column: Option<String>,
    /// Column with the account number or code. Never returned. Needed with `period_column`, and
    /// used to count salary lines when no position count is available.
    pub account_column: Option<String>,
    /// Column with the month (a date, "2026-07", or "Jul 2026") for a monthly ledger. Enables the
    /// run-rate forecast.
    pub period_column: Option<String>,
    /// Column with the number of positions (or FTE) on each line, used to decide whether the
    /// personnel amounts can be shown.
    pub positions_column: Option<String>,
    /// How many people are paid from the salary and wage lines, if the file has no positions
    /// column. Fewer than 3 hides the personnel amounts.
    pub personnel_headcount: Option<u32>,
    /// Extra category names to treat as personnel (salary and wage) categories.
    pub personnel_categories: Option<Vec<String>>,
    /// The date the analysis is as of (YYYY-MM-DD). Default: today.
    pub as_of_date: Option<String>,
    /// Month the fiscal year starts, 1 to 12. Default 7 (July 1, the Massachusetts municipal fiscal
    /// year). Use 10 for a federal year or 1 for a calendar-year grant.
    pub fiscal_year_start_month: Option<u32>,
    /// Flag a line whose spending or revenue is this many percentage points ahead of or behind the
    /// share of the year that has passed (default 10, this tool's convention).
    pub pace_tolerance_points: Option<f64>,
    /// Months of recent actuals averaged for the run-rate forecast (default 3; needs a month column).
    pub run_rate_months: Option<u32>,
    /// Revenue fund or source labels to place in a group yourself.
    pub revenue_group_overrides: Option<Vec<GroupOverride>>,
    /// Services to cost, with the volumes you supply.
    pub services: Option<Vec<ServiceArg>>,
    /// An indirect cost rate to add to the cost per service, in percent (default 0). Your rate,
    /// for example your town's.
    pub indirect_rate_percent: Option<f64>,
    /// Population served, optional: adds budget per resident beside DPH's published figures.
    pub population: Option<f64>,
    /// Department or program name for the report title.
    pub department_name: Option<String>,
    /// Write the workbook with live formulas to /data/output (default true).
    pub write_workbook: Option<bool>,
    /// Write the summary report to /data/output (default true).
    pub write_report: Option<bool>,
    /// docx (default) or pdf, for the report.
    pub format: Option<DocFormat>,
}

// ---------------------------------------------------------------- results

/// The fiscal year the analysis uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FiscalYearInfo {
    /// For example "FY2027" (named for the calendar year in which it ends).
    pub label: String,
    pub start: String,
    pub end: String,
    pub as_of: String,
    pub days_in_year: i64,
    pub days_elapsed: i64,
    /// Share of the year that has passed, in percent.
    pub percent_elapsed: f64,
    /// True when the start month is the Massachusetts municipal default (July) and not chosen.
    pub start_month_is_default: bool,
}

/// One budget or revenue category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoryLine {
    pub category: String,
    /// True for the combined salary and wage row.
    pub is_personnel: bool,
    /// True when the amounts are hidden because the personnel group has fewer than 3 positions.
    pub hidden: bool,
    pub budgeted: Option<f64>,
    /// Year to date.
    pub actual: Option<f64>,
    /// Expenses only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encumbered: Option<f64>,
    /// Expenses: budget - actual - encumbrances. Revenue: budget - actual (still to collect).
    pub remaining: Option<f64>,
    /// Actual as a percent of the budget.
    pub percent_of_budget: Option<f64>,
    /// Budget times the share of the year elapsed.
    pub expected_by_pace: Option<f64>,
    /// Actual minus expected by pace (positive is ahead of pace).
    pub variance_vs_pace: Option<f64>,
    /// Percent of budget used minus percent of year elapsed.
    pub points_vs_pace: Option<f64>,
    /// Plain status: On pace, Ahead of pace, Behind pace, Over budget, No budget.
    pub status: Option<String>,
    /// Year-to-date actual divided by the share of the year elapsed.
    pub forecast_straight_line: Option<f64>,
    /// Year to date plus the recent monthly average for the rest of the year (monthly files only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_run_rate: Option<f64>,
    /// Expenses: budget minus the straight-line forecast (negative is a projected overspend).
    /// Revenue: the straight-line forecast minus budget (negative is a projected shortfall).
    pub projected_vs_budget_straight_line: Option<f64>,
    /// Same, from the run-rate forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_vs_budget_run_rate: Option<f64>,
}

/// Totals for expenses or revenue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Totals {
    /// True when personnel is left out of these totals because its amounts are hidden.
    pub excludes_personnel: bool,
    pub budgeted: f64,
    pub actual: f64,
    pub encumbered: Option<f64>,
    pub remaining: f64,
    pub percent_of_budget: Option<f64>,
    pub variance_vs_pace: f64,
    pub forecast_straight_line: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_run_rate: Option<f64>,
}

/// How the personnel group was handled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PersonnelInfo {
    /// Whether any category was recognized as salary or wages.
    pub present: bool,
    /// Where the count of positions came from: "positions column", "headcount you gave", or
    /// "salary lines in the file (a proxy)".
    pub position_count_basis: String,
    /// True when the combined amounts are shown (at least the minimum number of positions).
    pub amounts_shown: bool,
    /// The minimum number of positions.
    pub minimum_positions: u32,
    /// Number of salary and wage categories combined into the one row.
    pub categories_combined: usize,
}

/// A revenue group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MixLine {
    pub group_id: String,
    pub group: String,
    pub budgeted: f64,
    pub actual: f64,
    pub percent_of_budgeted_revenue: Option<f64>,
    pub percent_of_actual_revenue: Option<f64>,
}

/// Cost of one service.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceCost {
    pub service: String,
    pub volume: f64,
    /// Share-weighted year-to-date cost of the categories you listed, plus the indirect rate.
    pub total_cost: Option<f64>,
    pub cost_per_unit: Option<f64>,
    /// Budgeted cost per planned unit (when planned_annual_volume was given).
    pub budgeted_cost_per_unit: Option<f64>,
    /// Why the cost is withheld, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withheld_reason: Option<String>,
}

/// Budget per resident beside DPH's published figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PerCapita {
    pub population: f64,
    pub municipal_appropriation_budget_per_resident: Option<f64>,
    pub total_revenue_budget_per_resident: Option<f64>,
    /// DPH's published figures (2022-2023 capacity assessment), with their confidence.
    pub published: Vec<PublishedFigure>,
}

/// A published per-capita figure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublishedFigure {
    pub label: String,
    pub dollars_per_resident: f64,
    pub citation: String,
    pub confidence: String,
    pub note: String,
}

/// Net position (revenue minus expense).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NetInfo {
    pub budgeted: f64,
    pub actual_ytd: f64,
    /// Actual revenue as a percent of actual expenses.
    pub revenue_covers_percent_of_expenses: Option<f64>,
}

/// A point that needs the nurse's attention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BudgetFlag {
    /// "expenses", "revenue", "forecast", "services", or "data".
    pub area: String,
    pub message: String,
}

/// Result of `analyze_health_department_budget`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BudgetResult {
    pub department: Option<String>,
    pub fiscal_year: FiscalYearInfo,
    /// "one row per account line" or "monthly ledger".
    pub layout: String,
    pub rows_read: usize,
    /// Rows left out for being outside this fiscal year or after the as-of date (monthly files).
    pub rows_outside_year: usize,
    pub expenses: Vec<CategoryLine>,
    pub expense_totals: Option<Totals>,
    pub revenue: Vec<CategoryLine>,
    pub revenue_totals: Option<Totals>,
    pub revenue_mix: Vec<MixLine>,
    /// Fund or source labels that matched no revenue group (name them in revenue_group_overrides).
    pub unclassified_revenue_labels: Vec<String>,
    pub personnel: PersonnelInfo,
    pub net: Option<NetInfo>,
    pub services: Vec<ServiceCost>,
    pub per_capita: Option<PerCapita>,
    pub flags: Vec<BudgetFlag>,
    /// How the forecasts were made.
    pub forecast_assumptions: Vec<String>,
    /// What DPH says the Public Health Excellence grants are and fund (cited; pending review).
    pub public_health_excellence: Vec<CitedOut>,
    /// Workbook with live formulas in the output folder.
    pub workbook_file: Option<String>,
    /// Summary report in the output folder.
    pub report_file: Option<String>,
}

// ---------------------------------------------------------------- reading the file

/// Revenue or expense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Kind {
    Expense,
    Revenue,
}

/// One account line (a row, or all the months of one account in a monthly ledger).
#[derive(Debug, Clone)]
struct Line {
    kind: Kind,
    category: String,
    source_label: Option<String>,
    budget: Option<f64>,
    actual: f64,
    encumbrance: Option<f64>,
    positions: Option<f64>,
    /// Actual by month (monthly ledgers).
    months: BTreeMap<(i32, u32), f64>,
    /// Distinct key used to count salary lines.
    account: String,
}

struct Loaded {
    lines: Vec<Line>,
    layout: &'static str,
    rows_read: usize,
    rows_outside_year: usize,
    has_type: bool,
    positions_column: bool,
    local: LocalTable,
}

/// A number from a money cell: "$1,234.50", "(1,234)", "-12".
pub(crate) fn money(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() || phi::is_missing(t) {
        return None;
    }
    let negative = t.starts_with('(') && t.ends_with(')');
    let clean: String = t
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let x = clean.parse::<f64>().ok().filter(|x| x.is_finite())?;
    if x.abs() > 1.0e12 {
        return None;
    }
    Some(if negative { -x.abs() } else { x })
}

const MONTH_NAMES: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// The (year, month) of a month cell: a date, "2026-07", "2026/7", "Jul 2026", or "July 2026".
fn parse_month(s: &str) -> Option<(i32, u32)> {
    let t = s.trim();
    let ok = |y: i32, m: u32| ((1900..=2100).contains(&y) && (1..=12).contains(&m)).then_some((y, m));
    let parts: Vec<&str> = t.split(['-', '/']).collect();
    if parts.len() == 2
        && parts[0].len() == 4
        && let (Ok(y), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>())
    {
        return ok(y, m);
    }
    if let Some(d) = phi::parse_date(t) {
        return ok(d.year(), d.month());
    }
    let words: Vec<String> = padded(t).split_whitespace().map(str::to_string).collect();
    let year = words
        .iter()
        .find_map(|w| (w.len() == 4).then(|| w.parse::<i32>().ok()).flatten());
    let month = words.iter().find_map(|w| {
        (w.len() >= 3)
            .then(|| MONTH_NAMES.iter().position(|m| w.starts_with(m)))
            .flatten()
    });
    ok(year?, month? as u32 + 1)
}

fn clean_label(what: &str, s: &str) -> Result<String> {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return Err(PhnError::InvalidInput(format!("{what} is empty")));
    }
    if t.chars().count() > 120 {
        return Err(PhnError::InvalidInput(format!(
            "{what} is longer than 120 characters"
        )));
    }
    if !phi::scan_text(&t).is_empty() {
        return Err(PhnError::Privacy(format!(
            "{what} appears to contain an identifier; use a service, department, or category name"
        )));
    }
    Ok(t)
}

/// Resolve a column: refuse person-like headers, use the named column, or find one by alias.
fn resolve(
    lt: &LocalTable,
    explicit: &Option<String>,
    aliases: &[String],
    blocked: &[String],
    arg: &str,
) -> Result<Option<usize>> {
    if let Some(name) = explicit.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        if has_any(name, blocked) {
            return Err(PhnError::Privacy(format!(
                "{arg}: column '{name}' looks like it holds names of people or vendors, contact \
                 details, or other identifiers, and the budget analysis never reads such columns. \
                 Use the category, amount, and account columns instead."
            )));
        }
        return lt.column(name).map(Some);
    }
    for a in aliases {
        let want = padded(a);
        for (i, h) in lt.table.headers.iter().enumerate() {
            if padded(h) == want && !has_any(h, blocked) && lt.column(h).is_ok() {
                return Ok(Some(i));
            }
        }
    }
    Ok(None)
}

/// A numeric column that must be mostly numbers.
fn amounts(lt: &LocalTable, idx: usize) -> Result<Vec<Option<f64>>> {
    let mut present = 0usize;
    let mut bad = 0usize;
    let v: Vec<Option<f64>> = lt
        .table
        .column(idx)
        .map(|s| {
            if phi::is_missing(s) {
                return None;
            }
            present += 1;
            let x = money(s);
            if x.is_none() {
                bad += 1;
            }
            x
        })
        .collect();
    if present > 0 && bad * 5 > present {
        return Err(PhnError::InvalidInput(format!(
            "column '{}' is not mostly numbers ({bad} of {present} cells could not be read as amounts)",
            lt.table.headers[idx]
        )));
    }
    Ok(v)
}

fn kind_words(c: &FinanceContent) -> (BTreeSet<String>, BTreeSet<String>) {
    let norm = |v: &Vec<String>| v.iter().map(|w| padded(w).trim().to_string()).collect();
    (norm(&c.revenue_type_words), norm(&c.expense_type_words))
}

#[allow(clippy::too_many_lines)]
fn load(
    ctx: &Context,
    args: &BudgetArgs,
    c: &FinanceContent,
    fy_start: NaiveDate,
    as_of: NaiveDate,
) -> Result<Loaded> {
    let lt = localdata::load(ctx, &args.path, args.sheet.as_deref())?;
    if lt.table.truncated {
        return Err(PhnError::InvalidInput(
            "the file has more rows than this tool reads; summarize it in the accounting system or \
             split it by year before analyzing"
                .into(),
        ));
    }
    let blocked = &c.blocked_header_words;
    let find = |explicit: &Option<String>, field: &str, arg: &str| {
        resolve(&lt, explicit, &c.aliases(field), blocked, arg)
    };
    let cat_i = find(&args.category_column, "category", "category_column")?.ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "no category column found; name one with category_column. Columns you can use: {}",
            usable_headers(&lt, blocked)
        ))
    })?;
    let budget_i = find(&args.budget_column, "budget", "budget_column")?.ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "no budget column found; name one with budget_column. Columns you can use: {}",
            usable_headers(&lt, blocked)
        ))
    })?;
    let actual_i = find(&args.actual_column, "actual", "actual_column")?.ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "no actual (year-to-date) column found; name one with actual_column. Columns you can use: {}",
            usable_headers(&lt, blocked)
        ))
    })?;
    if budget_i == actual_i {
        return Err(PhnError::InvalidInput(
            "budget_column and actual_column are the same column".into(),
        ));
    }
    let type_i = find(&args.type_column, "type", "type_column")?;
    let src_i = find(&args.source_column, "source", "source_column")?;
    let enc_i = find(&args.encumbrance_column, "encumbrance", "encumbrance_column")?;
    let acct_i = find(&args.account_column, "account", "account_column")?;
    let per_i = find(&args.period_column, "period", "period_column")?;
    let pos_i = find(&args.positions_column, "positions", "positions_column")?;
    // A period or account column found only by its alias must not be mistaken for the
    // category or amounts.
    let distinct: BTreeSet<usize> = [Some(cat_i), Some(budget_i), Some(actual_i), type_i, src_i, enc_i]
        .into_iter()
        .flatten()
        .collect();
    let used = 3 + [type_i, src_i, enc_i].iter().flatten().count();
    if distinct.len() != used {
        return Err(PhnError::InvalidInput(
            "two of the columns you named (or that were found by name) are the same column; name \
             each one explicitly"
                .into(),
        ));
    }

    let budgets = amounts(&lt, budget_i)?;
    let actuals = amounts(&lt, actual_i)?;
    let encs = match enc_i {
        Some(i) => Some(amounts(&lt, i)?),
        None => None,
    };
    let poss = match pos_i {
        Some(i) => Some(amounts(&lt, i)?),
        None => None,
    };
    let cats = lt.categorical(cat_i)?;
    let srcs = match src_i {
        Some(i) => Some(lt.categorical(i)?),
        None => None,
    };
    let (rev_words, exp_words) = kind_words(c);
    let kinds: Option<Vec<Option<Kind>>> = match type_i {
        Some(i) => {
            let vals = lt.categorical(i)?;
            let mut unknown: BTreeSet<String> = BTreeSet::new();
            let ks: Vec<Option<Kind>> = vals
                .iter()
                .map(|v| {
                    let v = v.as_deref()?;
                    let n = padded(v);
                    let n = n.trim();
                    if rev_words.contains(n) {
                        Some(Kind::Revenue)
                    } else if exp_words.contains(n) {
                        Some(Kind::Expense)
                    } else {
                        unknown.insert(v.to_string());
                        None
                    }
                })
                .collect();
            if !unknown.is_empty() {
                return Err(PhnError::InvalidInput(format!(
                    "the type column '{}' has values that are not revenue or expense: {}. Use \
                     Revenue and Expense (or Receipt and Expenditure)",
                    lt.table.headers[i],
                    unknown.into_iter().take(6).collect::<Vec<_>>().join(", ")
                )));
            }
            Some(ks)
        }
        None => None,
    };
    let accts: Option<Vec<String>> =
        acct_i.map(|i| lt.table.column(i).map(|s| s.trim().to_string()).collect());

    // Monthly ledgers.
    let months: Option<Vec<Option<(i32, u32)>>> = match per_i {
        Some(i) => {
            if acct_i.is_none() {
                return Err(PhnError::InvalidInput(
                    "a monthly ledger (period_column) needs account_column too, so the annual \
                     budget is counted once per account and not once per month"
                        .into(),
                ));
            }
            let m: Vec<Option<(i32, u32)>> = lt.table.column(i).map(parse_month).collect();
            let blank = lt
                .table
                .column(i)
                .zip(&m)
                .filter(|(s, m)| m.is_none() && !phi::is_missing(s))
                .count();
            if blank > 0 {
                return Err(PhnError::InvalidInput(format!(
                    "{blank} cells in the period column '{}' are not months (use 2026-07, Jul 2026, or a date)",
                    lt.table.headers[i]
                )));
            }
            Some(m)
        }
        None => None,
    };

    let fy_first = (fy_start.year(), fy_start.month());
    let as_of_key = (as_of.year(), as_of.month());
    let mut outside = 0usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut by_key: BTreeMap<(Kind, String, String, String), usize> = BTreeMap::new();
    for r in 0..lt.table.rows.len() {
        let category = cats[r].clone().unwrap_or_else(|| "(no category)".into());
        let kind = match &kinds {
            Some(k) => match k[r] {
                Some(k) => k,
                None => continue,
            },
            None => Kind::Expense,
        };
        let (b, a) = (budgets[r], actuals[r]);
        let e = encs.as_ref().and_then(|v| v[r]);
        if b.is_none() && a.is_none() && e.is_none() {
            continue;
        }
        let src = srcs.as_ref().and_then(|v| v[r].clone());
        let account = accts.as_ref().map(|v| v[r].clone()).unwrap_or_default();
        let positions = poss.as_ref().and_then(|v| v[r]);
        match &months {
            None => lines.push(Line {
                kind,
                category,
                source_label: src,
                budget: b,
                actual: a.unwrap_or(0.0),
                encumbrance: e,
                positions,
                months: BTreeMap::new(),
                account: if account.is_empty() {
                    format!("row {r}")
                } else {
                    account
                },
            }),
            Some(m) => {
                let Some(mk) = m[r] else { continue };
                if mk < fy_first || mk > as_of_key {
                    outside += 1;
                    continue;
                }
                let key = (
                    kind,
                    category.clone(),
                    src.clone().unwrap_or_default(),
                    account.clone(),
                );
                let idx = *by_key.entry(key).or_insert_with(|| {
                    lines.push(Line {
                        kind,
                        category,
                        source_label: src,
                        budget: None,
                        actual: 0.0,
                        encumbrance: None,
                        positions: None,
                        months: BTreeMap::new(),
                        account,
                    });
                    lines.len() - 1
                });
                let l = &mut lines[idx];
                // Rows can come in any order; the latest month's budget and encumbrance stand.
                let latest = l.months.keys().next_back().is_none_or(|last| *last <= mk);
                if let Some(b) = b
                    && (latest || l.budget.is_none())
                {
                    l.budget = Some(b);
                }
                if let Some(e) = e
                    && latest
                {
                    l.encumbrance = Some(e);
                }
                if let Some(p) = positions {
                    l.positions = Some(l.positions.map_or(p, |q| q.max(p)));
                }
                if let Some(a) = a {
                    l.actual += a;
                    *l.months.entry(mk).or_insert(0.0) += a;
                } else {
                    l.months.entry(mk).or_insert(0.0);
                }
            }
        }
    }
    if lines.is_empty() {
        return Err(PhnError::InvalidInput(
            "no usable rows: every row was missing its amounts or was outside the fiscal year"
                .into(),
        ));
    }
    Ok(Loaded {
        lines,
        layout: if months.is_some() {
            "monthly ledger"
        } else {
            "one row per account line"
        },
        rows_read: lt.table.rows.len(),
        rows_outside_year: outside,
        has_type: type_i.is_some(),
        positions_column: pos_i.is_some(),
        local: lt,
    })
}

fn usable_headers(lt: &LocalTable, blocked: &[String]) -> String {
    lt.table
        .headers
        .iter()
        .filter(|h| !has_any(h, blocked) && lt.column(h).is_ok())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------- calculation

/// Fiscal year start, end (exclusive), days, and elapsed share for a date.
pub(crate) fn fiscal_year(as_of: NaiveDate, start_month: u32) -> (NaiveDate, NaiveDate) {
    let mut start = NaiveDate::from_ymd_opt(as_of.year(), start_month, 1).expect("valid month");
    if start > as_of {
        start = start
            .checked_sub_months(Months::new(12))
            .expect("date in range");
    }
    let next = start
        .checked_add_months(Months::new(12))
        .expect("date in range");
    (start, next)
}

#[derive(Debug, Clone, Default)]
struct Agg {
    budget: f64,
    actual: f64,
    enc: f64,
    has_enc: bool,
    months: BTreeMap<(i32, u32), f64>,
    lines: usize,
}

impl Agg {
    fn add(&mut self, l: &Line) {
        self.budget += l.budget.unwrap_or(0.0);
        self.actual += l.actual;
        if let Some(e) = l.encumbrance {
            self.enc += e;
            self.has_enc = true;
        }
        for (m, x) in &l.months {
            *self.months.entry(*m).or_insert(0.0) += x;
        }
        self.lines += 1;
    }
}

pub(crate) fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Everything needed to compute one category line.
struct LineCalc {
    elapsed: f64,
    tolerance: f64,
    /// Recent months (complete, in the file) averaged for the run rate.
    recent: Vec<(i32, u32)>,
    monthly: bool,
}

fn status(kind: Kind, budget: f64, actual: f64, enc: f64, points: Option<f64>, tol: f64) -> String {
    if budget == 0.0 {
        return if actual > 0.0 && kind == Kind::Expense {
            "No budget".into()
        } else {
            "On pace".into()
        };
    }
    if kind == Kind::Expense && actual + enc > budget {
        return "Over budget".into();
    }
    match points {
        Some(p) if p > tol => "Ahead of pace".into(),
        Some(p) if p < -tol => "Behind pace".into(),
        _ => "On pace".into(),
    }
}

fn calc_line(kind: Kind, category: &str, a: &Agg, k: &LineCalc, personnel: bool) -> CategoryLine {
    let expense = kind == Kind::Expense;
    let enc = if expense { a.enc } else { 0.0 };
    let pct = (a.budget != 0.0).then(|| a.actual / a.budget * 100.0);
    let expected = a.budget * k.elapsed;
    let points = pct.map(|p| p - k.elapsed * 100.0);
    let straight = (k.elapsed > 0.0).then(|| a.actual / k.elapsed);
    let run = if k.monthly && !k.recent.is_empty() {
        let avg = k
            .recent
            .iter()
            .map(|m| a.months.get(m).copied().unwrap_or(0.0))
            .sum::<f64>()
            / k.recent.len() as f64;
        Some(a.actual + avg * 12.0 * (1.0 - k.elapsed))
    } else {
        None
    };
    let proj = |f: f64| if expense { a.budget - f } else { f - a.budget };
    CategoryLine {
        category: category.to_string(),
        is_personnel: personnel,
        hidden: false,
        budgeted: Some(round2(a.budget)),
        actual: Some(round2(a.actual)),
        encumbered: expense.then(|| round2(a.enc)),
        remaining: Some(round2(a.budget - a.actual - enc)),
        percent_of_budget: pct.map(round2),
        expected_by_pace: Some(round2(expected)),
        variance_vs_pace: Some(round2(a.actual - expected)),
        points_vs_pace: points.map(round2),
        status: Some(status(kind, a.budget, a.actual, enc, points, k.tolerance)),
        forecast_straight_line: straight.map(round2),
        forecast_run_rate: run.map(round2),
        projected_vs_budget_straight_line: straight.map(|f| round2(proj(f))),
        projected_vs_budget_run_rate: run.map(|f| round2(proj(f))),
    }
}

fn hidden_line(category: &str) -> CategoryLine {
    CategoryLine {
        category: category.to_string(),
        is_personnel: true,
        hidden: true,
        budgeted: None,
        actual: None,
        encumbered: None,
        remaining: None,
        percent_of_budget: None,
        expected_by_pace: None,
        variance_vs_pace: None,
        points_vs_pace: None,
        status: Some("Hidden: fewer than the minimum number of positions".into()),
        forecast_straight_line: None,
        forecast_run_rate: None,
        projected_vs_budget_straight_line: None,
        projected_vs_budget_run_rate: None,
    }
}

fn totals(kind: Kind, aggs: &[&Agg], k: &LineCalc, excludes_personnel: bool) -> Totals {
    let mut t = Agg::default();
    for a in aggs {
        t.budget += a.budget;
        t.actual += a.actual;
        t.enc += a.enc;
        t.has_enc |= a.has_enc;
        for (m, x) in &a.months {
            *t.months.entry(*m).or_insert(0.0) += x;
        }
    }
    let l = calc_line(kind, "Total", &t, k, false);
    Totals {
        excludes_personnel,
        budgeted: round2(t.budget),
        actual: round2(t.actual),
        encumbered: (kind == Kind::Expense).then(|| round2(t.enc)),
        remaining: l.remaining.unwrap_or(0.0),
        percent_of_budget: l.percent_of_budget,
        variance_vs_pace: l.variance_vs_pace.unwrap_or(0.0),
        forecast_straight_line: l.forecast_straight_line,
        forecast_run_rate: l.forecast_run_rate,
    }
}

/// The group a revenue label belongs to.
fn revenue_group(
    c: &FinanceContent,
    overrides: &[(String, String)],
    source: Option<&str>,
    category: &str,
) -> (String, bool) {
    let ov = |label: &str| {
        let p = padded(label);
        overrides
            .iter()
            .find(|(l, _)| padded(l) == p)
            .map(|(_, g)| g.clone())
    };
    if let Some(s) = source
        && let Some(g) = ov(s)
    {
        return (g, true);
    }
    if let Some(g) = ov(category) {
        return (g, true);
    }
    let classify = |label: &str| {
        c.revenue_groups
            .iter()
            .find(|g| g.id != "other" && has_any(label, &g.words))
            .map(|g| g.id.clone())
    };
    if let Some(s) = source
        && let Some(g) = classify(s)
    {
        return (g, true);
    }
    if let Some(g) = classify(category) {
        return (g, true);
    }
    ("other".into(), false)
}

fn norm_name(s: &str) -> String {
    padded(s).trim().to_string()
}

struct Computed {
    fiscal: FiscalYearInfo,
    elapsed: f64,
    tolerance: f64,
    expenses: Vec<CategoryLine>,
    expense_totals: Option<Totals>,
    revenue: Vec<CategoryLine>,
    revenue_totals: Option<Totals>,
    mix: Vec<MixLine>,
    unclassified: Vec<String>,
    personnel: PersonnelInfo,
    net: Option<NetInfo>,
    services: Vec<ServiceCost>,
    per_capita: Option<PerCapita>,
    flags: Vec<BudgetFlag>,
    recent_months: Vec<(i32, u32)>,
    /// Aggregates for the workbook (category label, aggregate), in the order of `expenses`.
    expense_aggs: Vec<Option<Agg>>,
    revenue_aggs: Vec<Option<Agg>>,
    mix_aggs: Vec<(String, Agg)>,
    /// Recent monthly average by line, for the workbook's run-rate column.
    expense_avgs: Vec<Option<f64>>,
    revenue_avgs: Vec<Option<f64>>,
    service_inputs: Vec<ServiceInput>,
}

/// What the workbook needs to compute a service's cost with formulas.
#[derive(Debug, Clone)]
pub(crate) struct ServiceInput {
    pub name: String,
    pub volume: f64,
    pub direct_cost: Option<f64>,
    pub budget_cost: Option<f64>,
    pub planned_volume: Option<f64>,
}

#[allow(clippy::too_many_lines)]
fn compute(
    c: &FinanceContent,
    args: &BudgetArgs,
    loaded: &Loaded,
    fy_start: NaiveDate,
    fy_next: NaiveDate,
    as_of: NaiveDate,
    start_month_is_default: bool,
) -> Result<Computed> {
    let days = (fy_next - fy_start).num_days();
    let elapsed_days = ((as_of - fy_start).num_days() + 1).clamp(1, days);
    let elapsed = elapsed_days as f64 / days as f64;
    let tolerance = args
        .pace_tolerance_points
        .unwrap_or(c.default_value("pace_tolerance_points")?);
    if !(0.0..=100.0).contains(&tolerance) {
        return Err(PhnError::InvalidInput(
            "pace_tolerance_points must be between 0 and 100".into(),
        ));
    }
    let run_months = args
        .run_rate_months
        .map_or(c.default_value("run_rate_months")? as usize, |n| n as usize);
    if !(1..=12).contains(&run_months) {
        return Err(PhnError::InvalidInput(
            "run_rate_months must be between 1 and 12".into(),
        ));
    }
    let min_group = c.default_value("min_personnel_group")?;

    // Months for the run rate: the last N complete months present in the file.
    let monthly = loaded.layout == "monthly ledger";
    let mut present: BTreeSet<(i32, u32)> = BTreeSet::new();
    for l in &loaded.lines {
        present.extend(l.months.keys().copied());
    }
    let complete = |m: &(i32, u32)| {
        NaiveDate::from_ymd_opt(m.0, m.1, 1)
            .and_then(|d| d.checked_add_months(Months::new(1)))
            .is_some_and(|next| next - chrono::Days::new(1) <= as_of)
    };
    let mut recent: Vec<(i32, u32)> = present.iter().copied().filter(complete).collect();
    if recent.len() > run_months {
        recent = recent.split_off(recent.len() - run_months);
    }
    let k = LineCalc {
        elapsed,
        tolerance,
        recent: recent.clone(),
        monthly,
    };

    let fiscal = FiscalYearInfo {
        label: format!(
            "FY{}",
            if fy_start.month() == 1 {
                fy_start.year()
            } else {
                fy_next.year()
            }
        ),
        start: fy_start.to_string(),
        end: (fy_next - chrono::Days::new(1)).to_string(),
        as_of: as_of.to_string(),
        days_in_year: days,
        days_elapsed: elapsed_days,
        percent_elapsed: round2(elapsed * 100.0),
        start_month_is_default,
    };

    // Personnel recognition.
    let extra: Vec<String> = args
        .personnel_categories
        .iter()
        .flatten()
        .map(|s| norm_name(s))
        .collect();
    let is_personnel = |cat: &str| {
        has_any(cat, &c.personnel_words) || extra.iter().any(|e| *e == norm_name(cat))
    };

    // Expense aggregates by category; personnel combined.
    let mut exp: BTreeMap<String, Agg> = BTreeMap::new();
    let mut pers = Agg::default();
    let mut pers_cats: BTreeSet<String> = BTreeSet::new();
    let mut pers_accounts: BTreeSet<String> = BTreeSet::new();
    let mut pers_positions = 0.0f64;
    let mut pers_positions_any = false;
    let mut rev: BTreeMap<String, Agg> = BTreeMap::new();
    let overrides: Vec<(String, String)> = args
        .revenue_group_overrides
        .iter()
        .flatten()
        .map(|o| (o.label.clone(), o.group.trim().to_ascii_lowercase()))
        .collect();
    for (label, g) in &overrides {
        if !c.revenue_groups.iter().any(|x| x.id == *g) {
            return Err(PhnError::InvalidInput(format!(
                "revenue_group_overrides: group '{g}' for '{label}' is not one of: {}",
                c.revenue_groups
                    .iter()
                    .map(|x| x.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    let mut mix: BTreeMap<String, Agg> = BTreeMap::new();
    let mut unclassified: BTreeSet<String> = BTreeSet::new();
    for l in &loaded.lines {
        match l.kind {
            Kind::Expense if is_personnel(&l.category) => {
                pers.add(l);
                pers_cats.insert(l.category.clone());
                pers_accounts.insert(l.account.clone());
                if let Some(p) = l.positions {
                    pers_positions += p;
                    pers_positions_any = true;
                }
            }
            Kind::Expense => exp.entry(l.category.clone()).or_default().add(l),
            Kind::Revenue => {
                rev.entry(l.category.clone()).or_default().add(l);
                let (g, matched) =
                    revenue_group(c, &overrides, l.source_label.as_deref(), &l.category);
                if !matched {
                    unclassified.insert(
                        l.source_label
                            .clone()
                            .unwrap_or_else(|| l.category.clone()),
                    );
                }
                mix.entry(g).or_default().add(l);
            }
        }
    }
    if exp.len() + usize::from(pers.lines > 0) > MAX_CATEGORIES || rev.len() > MAX_CATEGORIES {
        return Err(PhnError::InvalidInput(format!(
            "more than {MAX_CATEGORIES} categories; use a coarser category column"
        )));
    }

    // Is the personnel group big enough to show?
    let (positions, basis) = if loaded.positions_column && pers_positions_any {
        (pers_positions, "positions column")
    } else if let Some(h) = args.personnel_headcount {
        (f64::from(h), "headcount you gave")
    } else {
        (
            pers_accounts.len() as f64,
            "salary lines in the file (a proxy)",
        )
    };
    let shown = pers.lines == 0 || positions >= min_group;
    let personnel = PersonnelInfo {
        present: pers.lines > 0,
        position_count_basis: basis.to_string(),
        amounts_shown: pers.lines > 0 && shown,
        minimum_positions: min_group as u32,
        categories_combined: pers_cats.len(),
    };

    let mut flags: Vec<BudgetFlag> = Vec::new();
    let avg_of = |a: &Agg| -> Option<f64> {
        (monthly && !recent.is_empty()).then(|| {
            recent
                .iter()
                .map(|m| a.months.get(m).copied().unwrap_or(0.0))
                .sum::<f64>()
                / recent.len() as f64
        })
    };

    // Expense lines.
    let mut expenses: Vec<CategoryLine> = Vec::new();
    let mut expense_aggs: Vec<Option<Agg>> = Vec::new();
    let mut expense_avgs: Vec<Option<f64>> = Vec::new();
    for (cat, a) in &exp {
        expenses.push(calc_line(Kind::Expense, cat, a, &k, false));
        expense_aggs.push(Some(a.clone()));
        expense_avgs.push(avg_of(a));
    }
    if pers.lines > 0 {
        if shown {
            expenses.push(calc_line(
                Kind::Expense,
                PERSONNEL_LABEL,
                &pers,
                &k,
                true,
            ));
            expense_aggs.push(Some(pers.clone()));
            expense_avgs.push(avg_of(&pers));
        } else {
            expenses.push(hidden_line(PERSONNEL_LABEL));
            expense_aggs.push(None);
            expense_avgs.push(None);
            flags.push(BudgetFlag {
                area: "expenses".into(),
                message: format!(
                    "Salary and wage amounts are hidden because the combined personnel group has \
                     fewer than {} positions ({}); expense totals leave personnel out.",
                    min_group as u32, basis
                ),
            });
        }
    } else {
        flags.push(BudgetFlag {
            area: "data".into(),
            message: "No salary or wage category was recognized, so the personnel privacy rule \
                      was not applied. If a category holds pay for people, name it in \
                      personnel_categories and run again."
                .into(),
        });
    }
    let exp_refs: Vec<&Agg> = {
        let mut v: Vec<&Agg> = exp.values().collect();
        if pers.lines > 0 && shown {
            v.push(&pers);
        }
        v
    };
    let expense_totals = (!exp_refs.is_empty())
        .then(|| totals(Kind::Expense, &exp_refs, &k, pers.lines > 0 && !shown));

    // Revenue lines.
    let mut revenue: Vec<CategoryLine> = Vec::new();
    let mut revenue_aggs: Vec<Option<Agg>> = Vec::new();
    let mut revenue_avgs: Vec<Option<f64>> = Vec::new();
    for (cat, a) in &rev {
        revenue.push(calc_line(Kind::Revenue, cat, a, &k, false));
        revenue_aggs.push(Some(a.clone()));
        revenue_avgs.push(avg_of(a));
    }
    let rev_refs: Vec<&Agg> = rev.values().collect();
    let revenue_totals = (!rev_refs.is_empty()).then(|| totals(Kind::Revenue, &rev_refs, &k, false));

    // Revenue mix.
    let total_b: f64 = mix.values().map(|a| a.budget).sum();
    let total_a: f64 = mix.values().map(|a| a.actual).sum();
    let mut mix_lines: Vec<MixLine> = Vec::new();
    let mut mix_aggs: Vec<(String, Agg)> = Vec::new();
    for g in &c.revenue_groups {
        if let Some(a) = mix.get(&g.id) {
            mix_lines.push(MixLine {
                group_id: g.id.clone(),
                group: g.label.clone(),
                budgeted: round2(a.budget),
                actual: round2(a.actual),
                percent_of_budgeted_revenue: (total_b != 0.0).then(|| round2(a.budget / total_b * 100.0)),
                percent_of_actual_revenue: (total_a != 0.0).then(|| round2(a.actual / total_a * 100.0)),
            });
            mix_aggs.push((g.label.clone(), a.clone()));
        }
    }

    // Net position.
    let net = match (&expense_totals, &revenue_totals) {
        (Some(e), Some(r)) if !e.excludes_personnel => Some(NetInfo {
            budgeted: round2(r.budgeted - e.budgeted),
            actual_ytd: round2(r.actual - e.actual),
            revenue_covers_percent_of_expenses: (e.actual != 0.0)
                .then(|| round2(r.actual / e.actual * 100.0)),
        }),
        _ => None,
    };
    if net.is_none() && expense_totals.is_some() && revenue_totals.is_some() {
        flags.push(BudgetFlag {
            area: "data".into(),
            message: "Revenue minus expenses is not shown because personnel is hidden.".into(),
        });
    }

    // Services.
    let mut services: Vec<ServiceCost> = Vec::new();
    let mut service_inputs: Vec<ServiceInput> = Vec::new();
    let indirect = args.indirect_rate_percent.unwrap_or(0.0);
    if !(0.0..=200.0).contains(&indirect) {
        return Err(PhnError::InvalidInput(
            "indirect_rate_percent must be between 0 and 200".into(),
        ));
    }
    let specs = args.services.clone().unwrap_or_default();
    if specs.len() > MAX_SERVICES {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_SERVICES} services at a time"
        )));
    }
    let known: Vec<&String> = exp.keys().collect();
    for s in &specs {
        let name = clean_label("a service name", &s.name)?;
        if !(s.volume.is_finite() && s.volume > 0.0) {
            return Err(PhnError::InvalidInput(format!(
                "service '{name}': volume must be more than 0"
            )));
        }
        if let Some(p) = s.planned_annual_volume
            && !(p.is_finite() && p > 0.0)
        {
            return Err(PhnError::InvalidInput(format!(
                "service '{name}': planned_annual_volume must be more than 0"
            )));
        }
        if s.costs.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "service '{name}': list at least one cost category"
            )));
        }
        let (mut cost, mut budget) = (0.0f64, 0.0f64);
        let mut withheld: Option<String> = None;
        let mut used_personnel = false;
        for cc in &s.costs {
            let share = cc.share_percent.unwrap_or(100.0);
            if !(share.is_finite() && share > 0.0 && share <= 100.0) {
                return Err(PhnError::InvalidInput(format!(
                    "service '{name}': share_percent must be more than 0 and at most 100"
                )));
            }
            let want = norm_name(&cc.category);
            let is_pool = want == norm_name(PERSONNEL_LABEL)
                || want == "personnel"
                || (pers.lines > 0 && pers_cats.iter().any(|p| norm_name(p) == want));
            if is_pool {
                if used_personnel {
                    return Err(PhnError::InvalidInput(format!(
                        "service '{name}': salary and wage categories are one combined pool; list \
                         it once (as \"Personnel\") with one share"
                    )));
                }
                used_personnel = true;
                if pers.lines == 0 {
                    return Err(PhnError::InvalidInput(format!(
                        "service '{name}': the file has no salary or wage category"
                    )));
                }
                if shown {
                    cost += pers.actual * share / 100.0;
                    budget += pers.budget * share / 100.0;
                } else {
                    withheld = Some(
                        "its cost includes the personnel pool, which is hidden because it has \
                         fewer than the minimum number of positions"
                            .into(),
                    );
                }
                continue;
            }
            match exp.iter().find(|(n, _)| norm_name(n) == want) {
                Some((_, a)) => {
                    cost += a.actual * share / 100.0;
                    budget += a.budget * share / 100.0;
                }
                None => {
                    return Err(PhnError::InvalidInput(format!(
                        "service '{name}': no expense category named '{}'. Categories: {}{}",
                        cc.category,
                        known.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                        if pers.lines > 0 { ", Personnel" } else { "" }
                    )));
                }
            }
        }
        let total = cost * (1.0 + indirect / 100.0);
        let budget_total = budget * (1.0 + indirect / 100.0);
        let ok = withheld.is_none();
        services.push(ServiceCost {
            service: name.clone(),
            volume: s.volume,
            total_cost: ok.then(|| round2(total)),
            cost_per_unit: ok.then(|| round2(total / s.volume)),
            budgeted_cost_per_unit: s
                .planned_annual_volume
                .filter(|_| ok)
                .map(|p| round2(budget_total / p)),
            withheld_reason: withheld.clone(),
        });
        service_inputs.push(ServiceInput {
            name,
            volume: s.volume,
            direct_cost: ok.then_some(cost),
            budget_cost: ok.then_some(budget),
            planned_volume: s.planned_annual_volume,
        });
    }

    // Per resident.
    let per_capita = match args.population {
        Some(p) => {
            if !(p.is_finite() && (1.0..=20_000_000.0).contains(&p)) {
                return Err(PhnError::InvalidInput(
                    "population must be between 1 and 20,000,000".into(),
                ));
            }
            let muni = mix.get("municipal").map(|a| a.budget / p);
            let tot = revenue_totals.as_ref().map(|t| t.budgeted / p);
            let published: Vec<PublishedFigure> = c
                .per_capita_benchmarks
                .iter()
                .map(|b| PublishedFigure {
                    label: b.label.clone(),
                    dollars_per_resident: b.value,
                    citation: format!(
                        "{}, {}",
                        c.sources.get(&b.source).map_or(b.source.as_str(), |s| s.title.as_str()),
                        b.section
                    ),
                    confidence: format!("{:?}", b.confidence).to_ascii_lowercase(),
                    note: b.note.clone(),
                })
                .collect();
            Some(PerCapita {
                population: p,
                municipal_appropriation_budget_per_resident: muni.map(round2),
                total_revenue_budget_per_resident: tot.map(round2),
                published,
            })
        }
        None => None,
    };

    // Flags.
    for l in &expenses {
        if l.hidden {
            continue;
        }
        match l.status.as_deref() {
            Some("Over budget") | Some("No budget") => flags.push(BudgetFlag {
                area: "expenses".into(),
                message: format!(
                    "{}: {} (spent {} plus {} encumbered against a budget of {}).",
                    l.category,
                    l.status.as_deref().unwrap_or(""),
                    md::num(l.actual.unwrap_or(0.0), 0),
                    md::num(l.encumbered.unwrap_or(0.0), 0),
                    md::num(l.budgeted.unwrap_or(0.0), 0)
                ),
            }),
            Some("Ahead of pace") => flags.push(BudgetFlag {
                area: "expenses".into(),
                message: format!(
                    "{}: spending is {:.1} points ahead of the share of the year that has passed.",
                    l.category,
                    l.points_vs_pace.unwrap_or(0.0)
                ),
            }),
            Some("Behind pace") => flags.push(BudgetFlag {
                area: "expenses".into(),
                message: format!(
                    "{}: spending is {:.1} points behind pace (under-spending, or invoices not yet posted).",
                    l.category,
                    -l.points_vs_pace.unwrap_or(0.0)
                ),
            }),
            _ => {}
        }
    }
    for l in &revenue {
        if l.status.as_deref() == Some("Behind pace") {
            flags.push(BudgetFlag {
                area: "revenue".into(),
                message: format!(
                    "{}: revenue is {:.1} points behind the share of the year that has passed \
                     (check timing: grants paid in arrears and seasonal fees arrive unevenly).",
                    l.category,
                    -l.points_vs_pace.unwrap_or(0.0)
                ),
            });
        }
    }
    if elapsed < 0.25 {
        flags.push(BudgetFlag {
            area: "forecast".into(),
            message: format!(
                "Only {:.0}% of the year has passed, so straight-line forecasts multiply a short \
                 period by a large factor and are unreliable.",
                elapsed * 100.0
            ),
        });
    }
    if !unclassified.is_empty() {
        flags.push(BudgetFlag {
            area: "revenue".into(),
            message: format!(
                "{} revenue label(s) matched no revenue group and are counted as 'Other or not \
                 classified'; place them with revenue_group_overrides.",
                unclassified.len()
            ),
        });
    }
    if !loaded.has_type {
        flags.push(BudgetFlag {
            area: "data".into(),
            message: "The file has no revenue/expense column, so every row was treated as an \
                      expense and the revenue mix was skipped. Name one with type_column."
                .into(),
        });
    }

    Ok(Computed {
        fiscal,
        elapsed,
        tolerance,
        expenses,
        expense_totals,
        revenue,
        revenue_totals,
        mix: mix_lines,
        unclassified: unclassified.into_iter().collect(),
        personnel,
        net,
        services,
        per_capita,
        flags,
        recent_months: recent,
        expense_aggs,
        revenue_aggs,
        mix_aggs,
        expense_avgs,
        revenue_avgs,
        service_inputs,
    })
}

pub(crate) const PERSONNEL_LABEL: &str = "Personnel (all salary and wage categories combined)";

/// What the workbook and report writers use besides the result.
pub(crate) struct OutputInputs<'a> {
    pub result: &'a BudgetResult,
    pub fy_start: NaiveDate,
    pub as_of: NaiveDate,
    pub tolerance: f64,
    pub indirect_rate: f64,
    pub expense_aggs: &'a [Option<AggView>],
    pub revenue_aggs: &'a [Option<AggView>],
    pub expense_avgs: &'a [Option<f64>],
    pub revenue_avgs: &'a [Option<f64>],
    pub mix: Vec<(String, f64, f64)>,
    pub service_inputs: &'a [ServiceInput],
    pub elapsed: f64,
}

/// Budget, actual, and encumbrance of a category for the workbook.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AggView {
    pub budget: f64,
    pub actual: f64,
    pub enc: f64,
}

// ---------------------------------------------------------------- the tool

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .ok()
        .filter(|d| (1900..=2100).contains(&d.year()))
        .ok_or_else(|| {
            PhnError::InvalidInput("as_of_date must be a date like 2026-10-31 (year 1900 to 2100)".into())
        })
}

#[allow(clippy::too_many_lines)]
pub fn analyze_health_department_budget(
    ctx: &Context,
    args: BudgetArgs,
) -> Result<Envelope<BudgetResult>> {
    if args.path.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "path is required: a budget or ledger CSV or XLSX in /data".into(),
        ));
    }
    let c = FinanceContent::load()?;
    let department = match &args.department_name {
        Some(d) if !d.trim().is_empty() => Some(clean_label("department_name", d)?),
        _ => None,
    };
    let as_of = match &args.as_of_date {
        Some(d) if !d.trim().is_empty() => parse_date(d)?,
        _ => chrono::Local::now().date_naive(),
    };
    let start_month = args
        .fiscal_year_start_month
        .unwrap_or(c.fiscal_year.default_start_month);
    if !(1..=12).contains(&start_month) {
        return Err(PhnError::InvalidInput(
            "fiscal_year_start_month must be between 1 and 12".into(),
        ));
    }
    let (fy_start, fy_next) = fiscal_year(as_of, start_month);
    let loaded = load(ctx, &args, &c, fy_start, as_of)?;
    let comp = compute(
        &c,
        &args,
        &loaded,
        fy_start,
        fy_next,
        as_of,
        args.fiscal_year_start_month.is_none(),
    )?;

    let mut forecast_assumptions = vec![format!(
        "Straight line: year-to-date actual divided by the share of the year that has passed \
         ({:.2}%), which assumes spending or revenue arrives evenly through the year. It ignores \
         seasonality, one-time costs, grants paid in arrears, and fee seasons.",
        comp.fiscal.percent_elapsed
    )];
    if comp.recent_months.is_empty() {
        forecast_assumptions.push(
            "Run rate: not available, because the file has no month column with complete months \
             (add period_column and account_column)."
                .into(),
        );
    } else {
        forecast_assumptions.push(format!(
            "Run rate: year-to-date actual plus the average monthly amount of the last {} complete \
             month(s) in the file ({}), times the months left in the year (12 x share of the year \
             remaining). It follows recent spending but treats a one-time spike as ongoing.",
            comp.recent_months.len(),
            comp.recent_months
                .iter()
                .map(|(y, m)| format!("{y}-{m:02}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    forecast_assumptions.push(
        "Neither forecast is a prediction of what will happen: they show where the year ends if \
         the stated pattern continues. Known one-time costs and the timing of grant payments \
         should be added by hand."
            .into(),
    );

    let phe = cite_all(&c.phe_program.statements, &c.sources);
    let mut result = BudgetResult {
        department: department.clone(),
        fiscal_year: comp.fiscal.clone(),
        layout: loaded.layout.to_string(),
        rows_read: loaded.rows_read,
        rows_outside_year: loaded.rows_outside_year,
        expenses: comp.expenses.clone(),
        expense_totals: comp.expense_totals.clone(),
        revenue: comp.revenue.clone(),
        revenue_totals: comp.revenue_totals.clone(),
        revenue_mix: comp.mix.clone(),
        unclassified_revenue_labels: comp.unclassified.clone(),
        personnel: comp.personnel.clone(),
        net: comp.net.clone(),
        services: comp.services.clone(),
        per_capita: comp.per_capita.clone(),
        flags: comp.flags.clone(),
        forecast_assumptions,
        public_health_excellence: phe,
        workbook_file: None,
        report_file: None,
    };

    // Files (aggregates only; the personnel pool is hidden here too when it is small).
    let mut files_written: Vec<String> = Vec::new();
    let want_book = args.write_workbook.unwrap_or(true);
    let want_report = args.write_report.unwrap_or(true);
    if want_book || want_report {
        let view = |v: &[Option<Agg>]| -> Vec<Option<AggView>> {
            v.iter()
                .map(|a| {
                    a.as_ref().map(|a| AggView {
                        budget: a.budget,
                        actual: a.actual,
                        enc: a.enc,
                    })
                })
                .collect()
        };
        let ev = view(&comp.expense_aggs);
        let rv = view(&comp.revenue_aggs);
        let inputs = OutputInputs {
            result: &result,
            fy_start,
            as_of,
            tolerance: comp.tolerance,
            indirect_rate: args.indirect_rate_percent.unwrap_or(0.0),
            expense_aggs: &ev,
            revenue_aggs: &rv,
            expense_avgs: &comp.expense_avgs,
            revenue_avgs: &comp.revenue_avgs,
            mix: comp
                .mix_aggs
                .iter()
                .map(|(l, a)| (l.clone(), a.budget, a.actual))
                .collect(),
            service_inputs: &comp.service_inputs,
            elapsed: comp.elapsed,
        };
        let (dir, _) = crate::school::templates::output_folder(ctx, "department_budget")?;
        let (book, report) = budget_output::write_files(
            ctx,
            &dir,
            &inputs,
            want_book,
            want_report,
            args.format.unwrap_or_default(),
        )?;
        if let Some(f) = book {
            files_written.push(f.clone());
            result.workbook_file = Some(f);
        }
        if let Some(f) = report {
            files_written.push(f.clone());
            result.report_file = Some(f);
        }
    }

    let summary = budget_output::summary(&result);
    let mut caveats = vec![
        "Figures come from the file as it is; the tool cannot check them against the general \
         ledger. Pace compares the share of each budget used with the share of the fiscal year \
         that has passed, counting days (as-of date included). It is a straight-line yardstick: \
         payroll and rent are even, vaccine orders and grant payments are not."
            .to_string(),
        format!(
            "A line is flagged when spending or revenue is more than {:.0} percentage points ahead \
             of or behind pace (this tool's convention, change it with pace_tolerance_points), and \
             an expense is 'Over budget' when spending plus encumbrances exceeds the budget.",
            comp.tolerance
        ),
        "Personnel: every category recognized as salary or wages (salary, wage, payroll, \
         personnel, personal services, stipend, compensation, overtime, longevity, per diem, \
         fringe, employee benefits, or named in personnel_categories) is combined into one row. \
         When the group has fewer than 3 positions its amounts are hidden in the response, the \
         workbook, and the report, expense totals leave it out, and a service cost that depends on \
         it is withheld. The position count comes from positions_column, then personnel_headcount, \
         and only then from the number of salary accounts in the file, which can overstate it."
            .into(),
        "Revenue groups come from words in the fund or source label (content: tool defaults, not \
         DPH or DLS classifications). The Public Health Excellence group is a label match, not a \
         check against DPH's award records."
            .into(),
    ];
    if loaded.layout == "monthly ledger" {
        caveats.push(
            "Monthly ledger: each account's annual budget is taken from its latest month with a \
             budget, encumbrances from its latest month, and actuals are summed for the months from \
             the start of the fiscal year through the as-of month."
                .into(),
        );
        if loaded.rows_outside_year > 0 {
            caveats.push(format!(
                "{} row(s) were left out because their month is before this fiscal year or after \
                 the as-of date.",
                loaded.rows_outside_year
            ));
        }
    }
    if comp.fiscal.start_month_is_default {
        caveats.push(
            "The fiscal year starts July 1 (the Massachusetts municipal year, G.L. c. 44, s. 56). \
             If this file follows a grant or federal year, set fiscal_year_start_month."
                .into(),
        );
    }
    if let Some(pc) = &result.per_capita {
        let _ = pc;
        caveats.push(
            "Budget per resident is compared with DPH's published figures for shared services \
             arrangements (2022-2023), which are budgets of arrangements, not of one town, and the \
             $36.98 estimate comes from research DPH does not name (medium confidence). They are \
             context, not a standard your department must meet."
                .into(),
        );
    }
    caveats.extend(fc::caveats(&[fc::FINANCE]));

    let mut notes = vec![
        "Only the category, type, source, amount, account, month, and position columns were read. \
         Vendor, employee, payee, and contact columns were never read, and no account code, vendor, \
         employee, or row is returned or written."
            .to_string(),
        "Amounts are category totals. Personnel amounts are combined and hidden when the group is \
         small."
            .to_string(),
    ];
    if !files_written.is_empty() {
        notes.push(
            "The workbook and report in the output folder hold the same category totals as this \
             response (personnel hidden when small); they stay on your computer."
                .into(),
        );
    }
    let mut sources = vec![
        loaded.local.source(),
        Source::computed(
            "Pace, variance, and forecast arithmetic (straight line and recent run rate)",
            "Calculated by this server",
        ),
    ];
    let used = [
        "dls_glossary",
        "mgl_44_56",
        "phe_page",
        "saphe_archived",
        "capacity_assessment",
    ];
    sources.extend(fc_sources(&c, &used));
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: !result.personnel.amounts_shown && result.personnel.present,
        suppression_threshold: Some(c.default_value("min_personnel_group")? as u32),
        identifiers_detected: loaded.local.identifier_categories(),
        files_written,
        notes,
    };
    Ok(Envelope::new(summary, result)
        .decision_support()
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn fc_sources(c: &FinanceContent, keys: &[&str]) -> Vec<Source> {
    crate::epr::content::sources_for(keys.iter().copied(), &c.sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_reads_common_formats() {
        assert_eq!(money("$1,234.50"), Some(1234.5));
        assert_eq!(money("(1,200)"), Some(-1200.0));
        assert_eq!(money("-45"), Some(-45.0));
        assert_eq!(money(""), None);
        assert_eq!(money("n/a"), None);
        assert_eq!(money("abc"), None);
    }

    #[test]
    fn months_are_read_from_dates_and_labels() {
        assert_eq!(parse_month("2026-07"), Some((2026, 7)));
        assert_eq!(parse_month("2026/7"), Some((2026, 7)));
        assert_eq!(parse_month("Jul 2026"), Some((2026, 7)));
        assert_eq!(parse_month("September 2026"), Some((2026, 9)));
        assert_eq!(parse_month("2026-08-15"), Some((2026, 8)));
        assert_eq!(parse_month("2126-08"), None);
        assert_eq!(parse_month("2026-13"), None);
        assert_eq!(parse_month("hello"), None);
    }

    #[test]
    fn the_fiscal_year_starts_in_july_and_is_named_for_the_year_it_ends() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        let (s, n) = fiscal_year(d(2026, 10, 31), 7);
        assert_eq!((s, n), (d(2026, 7, 1), d(2027, 7, 1)));
        let (s, n) = fiscal_year(d(2027, 3, 1), 7);
        assert_eq!((s, n), (d(2026, 7, 1), d(2027, 7, 1)));
        let (s, n) = fiscal_year(d(2026, 6, 30), 7);
        assert_eq!((s, n), (d(2025, 7, 1), d(2026, 7, 1)));
        let (s, _) = fiscal_year(d(2026, 7, 1), 7);
        assert_eq!(s, d(2026, 7, 1));
        let (s, n) = fiscal_year(d(2026, 2, 10), 1);
        assert_eq!((s, n), (d(2026, 1, 1), d(2027, 1, 1)));
    }

    #[test]
    fn status_rules() {
        use Kind::*;
        assert_eq!(status(Expense, 100.0, 60.0, 50.0, Some(0.0), 10.0), "Over budget");
        assert_eq!(status(Expense, 100.0, 50.0, 0.0, Some(15.0), 10.0), "Ahead of pace");
        assert_eq!(status(Expense, 100.0, 20.0, 0.0, Some(-15.0), 10.0), "Behind pace");
        assert_eq!(status(Expense, 100.0, 35.0, 0.0, Some(5.0), 10.0), "On pace");
        assert_eq!(status(Expense, 0.0, 5.0, 0.0, None, 10.0), "No budget");
        assert_eq!(status(Revenue, 100.0, 120.0, 0.0, Some(-30.0), 10.0), "Behind pace");
    }

    #[test]
    fn revenue_labels_go_to_groups_and_phe_comes_first() {
        let c = FinanceContent::load().unwrap();
        let g = |s: &str, cat: &str| revenue_group(&c, &[], Some(s), cat).0;
        assert_eq!(g("State Public Health Excellence (PHE) Grant", "State Grants"), "phe");
        assert_eq!(g("SAPHE shared services", "Grants"), "phe");
        assert_eq!(g("CDC PHEP (federal)", "Federal Grants"), "federal");
        assert_eq!(g("DPH Immunization Grant", "State Grants"), "state_other");
        assert_eq!(g("Revolving Fund - Permits", "Inspection Fees"), "fees");
        assert_eq!(g("General Fund", "Municipal Appropriation"), "municipal");
        assert_eq!(g("Community Foundation grant", "Gifts"), "other_grants");
        assert_eq!(g("Sale of publications", "Miscellaneous"), "other");
        let ov = vec![("sale of publications".to_string(), "fees".to_string())];
        assert_eq!(revenue_group(&c, &ov, Some("Sale of Publications"), "x").0, "fees");
    }
}
