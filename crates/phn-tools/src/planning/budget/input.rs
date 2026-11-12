//! Turning the request (typed line items, or a CSV/XLSX file in /data) into validated budget
//! lines. A file is read through the local-data guardrails: a file with an identifier column (a
//! staff roster with names, say) is refused, and no cell value is ever echoed in an error.

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::calc::{Category, Funding, Line, MtdcExclusion, treatment_for};
use crate::context::Context;

/// Most line items in one budget.
pub const MAX_LINES: usize = 150;
/// Longest item description (characters).
pub const MAX_TEXT: usize = 160;

/// One cost typed into the request.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LineItemArg {
    /// Object class: personnel, travel, equipment, supplies, contractual, or other. Fringe
    /// benefits are calculated from the personnel lines.
    pub category: Category,
    /// A role or item description ("Public health nurse", "Mileage", "Interpreter services").
    /// Use roles, never staff names.
    pub item: String,
    /// Who pays: `federal` (default, the request) or `cost_share` (your match).
    pub funding: Option<Funding>,
    /// Personnel only: annual base salary in dollars.
    pub annual_salary: Option<f64>,
    /// Personnel only: percent of time on the project (50 for half time).
    pub effort_percent: Option<f64>,
    /// Other categories: units per year (default 1).
    pub quantity: Option<f64>,
    /// Other categories: cost per unit in dollars.
    pub unit_cost: Option<f64>,
    /// Other categories: what a unit is ("trips", "months", "each").
    pub unit: Option<String>,
    /// Personnel only: fringe rate for this position in percent (default: the overall rate).
    pub fringe_rate_percent: Option<f64>,
    /// Apply the yearly escalation to this line (default: yes, except equipment).
    pub escalate: Option<bool>,
    /// First budget year the cost occurs (default 1).
    pub first_year: Option<u32>,
    /// Last budget year (default: the last year of the budget).
    pub last_year: Option<u32>,
    /// Contractual only: true for a subaward (only the first $50,000 counts in the MTDC base).
    pub subaward: Option<bool>,
    /// Leave this line out of the MTDC base: participant_support, rental, patient_care,
    /// tuition_or_scholarship, or capital_expenditure.
    pub exclude_from_mtdc: Option<MtdcExclusion>,
}

/// Parsed lines and the number of years they need.
pub struct Parsed {
    pub lines: Vec<Line>,
    /// Largest `last_year` any line named (0 when none did).
    pub max_named_year: usize,
}

fn check_text(s: &str, what: &str) -> Result<()> {
    if s.trim().is_empty() {
        return Err(PhnError::InvalidInput(format!("{what} is empty")));
    }
    if s.chars().count() > MAX_TEXT {
        return Err(PhnError::InvalidInput(format!(
            "{what} is longer than {MAX_TEXT} characters; shorten it"
        )));
    }
    Ok(())
}

fn pct(name: &str, v: f64, row: usize) -> Result<f64> {
    if !v.is_finite() || !(0.0..=100.0).contains(&v) {
        return Err(PhnError::InvalidInput(format!(
            "line {row}: {name} must be between 0 and 100"
        )));
    }
    Ok(v / 100.0)
}

fn year(v: Option<u32>, row: usize, what: &str) -> Result<Option<usize>> {
    match v {
        None => Ok(None),
        Some(y) if (1..=super::calc::MAX_YEARS as u32).contains(&y) => Ok(Some(y as usize)),
        Some(_) => Err(PhnError::InvalidInput(format!(
            "line {row}: {what} must be from 1 to {}",
            super::calc::MAX_YEARS
        ))),
    }
}

/// Convert typed items. `years` is the budget length (lines without a last year run to it).
pub fn from_args(items: &[LineItemArg], years: Option<usize>) -> Result<Parsed> {
    let mut max_named = 0usize;
    let mut pending = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let row = i + 1;
        check_text(&it.item, &format!("line {row}: the item"))?;
        let unit = it.unit.clone().unwrap_or_default();
        if unit.chars().count() > 40 {
            return Err(PhnError::InvalidInput(format!(
                "line {row}: the unit is too long"
            )));
        }
        let (quantity, unit_cost, fringe) = if it.category == Category::Personnel {
            if it.quantity.is_some() || it.unit_cost.is_some() {
                return Err(PhnError::InvalidInput(format!(
                    "line {row}: a personnel line takes annual_salary and effort_percent, not quantity and unit_cost"
                )));
            }
            let (Some(sal), Some(eff)) = (it.annual_salary, it.effort_percent) else {
                return Err(PhnError::InvalidInput(format!(
                    "line {row}: a personnel line needs annual_salary and effort_percent"
                )));
            };
            let f = it
                .fringe_rate_percent
                .map(|f| pct("fringe_rate_percent", f, row))
                .transpose()?;
            (pct("effort_percent", eff, row)?, sal, f)
        } else {
            if it.annual_salary.is_some()
                || it.effort_percent.is_some()
                || it.fringe_rate_percent.is_some()
            {
                return Err(PhnError::InvalidInput(format!(
                    "line {row}: annual_salary, effort_percent, and fringe_rate_percent are for personnel lines"
                )));
            }
            let Some(u) = it.unit_cost else {
                return Err(PhnError::InvalidInput(format!(
                    "line {row}: this line needs a unit_cost"
                )));
            };
            (it.quantity.unwrap_or(1.0), u, None)
        };
        if it.subaward == Some(true) && it.category != Category::Contractual {
            return Err(PhnError::InvalidInput(format!(
                "line {row}: only a contractual line can be a subaward"
            )));
        }
        let first = year(it.first_year, row, "first_year")?;
        let last = year(it.last_year, row, "last_year")?;
        if let Some(l) = last {
            max_named = max_named.max(l);
        }
        pending.push((
            it.category,
            it.item.trim().to_string(),
            it.funding.unwrap_or(Funding::Federal),
            quantity,
            unit_cost,
            unit,
            fringe,
            it.escalate.unwrap_or(it.category != Category::Equipment),
            first,
            last,
            it.subaward == Some(true),
            it.exclude_from_mtdc,
        ));
    }
    let n = years.unwrap_or(max_named.max(1));
    let lines = pending
        .into_iter()
        .map(
            |(
                category,
                item,
                funding,
                quantity,
                unit_cost,
                unit,
                fringe,
                escalate,
                first,
                last,
                sub,
                excl,
            )| Line {
                category,
                item,
                funding,
                quantity,
                unit_cost,
                unit,
                fringe,
                escalate,
                first_year: first.unwrap_or(1),
                last_year: last.unwrap_or(n),
                treatment: treatment_for(category, sub, excl),
                exclusion: excl,
            },
        )
        .collect();
    Ok(Parsed {
        lines,
        max_named_year: max_named,
    })
}

const CATEGORY: &[&str] = &["category", "object class", "class", "object_class"];
const ITEM: &[&str] = &[
    "item",
    "role",
    "description",
    "position",
    "position title",
    "line item",
];
const FUNDING: &[&str] = &["funding", "funding source", "source", "paid by"];
const SALARY: &[&str] = &["annual salary", "base salary", "salary", "annual_salary"];
const EFFORT: &[&str] = &[
    "effort percent",
    "percent effort",
    "effort",
    "effort %",
    "% effort",
];
const FTE: &[&str] = &["fte"];
const QUANTITY: &[&str] = &["quantity", "qty", "units", "number"];
const UNIT_COST: &[&str] = &["unit cost", "cost per unit", "unit_cost", "cost", "price"];
const UNIT: &[&str] = &["unit", "unit type"];
const FRINGE: &[&str] = &["fringe rate", "fringe rate percent", "fringe", "fringe %"];
const ESCALATE: &[&str] = &["escalate", "escalation"];
const FIRST: &[&str] = &["first year", "start year", "first_year"];
const LAST: &[&str] = &["last year", "end year", "last_year"];
const SUBAWARD: &[&str] = &["subaward", "is subaward"];
const EXCLUDE: &[&str] = &[
    "exclude from mtdc",
    "mtdc exclusion",
    "exclusion",
    "exclude",
];

fn find(headers: &[String], aliases: &[&str]) -> Option<usize> {
    let norm: Vec<String> = headers
        .iter()
        .map(|h| phn_core::text::normalize(h))
        .collect();
    aliases
        .iter()
        .find_map(|a| norm.iter().position(|h| *h == phn_core::text::normalize(a)))
}

/// A number from a cell: "$1,200.50", "50%", "12".
fn number(cell: &str) -> Option<f64> {
    let s = cell
        .trim()
        .trim_start_matches('$')
        .trim_end_matches('%')
        .replace(',', "");
    if s.is_empty() {
        return None;
    }
    s.trim().parse::<f64>().ok().filter(|x| x.is_finite())
}

/// What reading a budget file found: the lines, and identifier categories (always empty,
/// because a file with identifier columns is refused).
pub struct FileLines {
    pub parsed: Parsed,
    pub label: String,
    pub source: phn_core::Source,
}

/// Read line items from a CSV/XLSX in /data. Identifier columns refuse the whole file.
pub fn from_file(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    years: Option<usize>,
) -> Result<FileLines> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    // A budget file has no business holding identifiers; refuse before reading any cell.
    let refused: Vec<&phn_local::phi::ColumnFinding> = lt
        .findings
        .iter()
        .filter(|f| {
            !matches!(
                f.kind,
                phn_local::phi::Identifier::Age
                    | phn_local::phi::Identifier::Date
                    | phn_local::phi::Identifier::Geography
                    | phn_local::phi::Identifier::ZipCode
            )
        })
        .collect();
    if let Some(f) = refused.first() {
        return Err(PhnError::Privacy(format!(
            "column '{}' in '{}' appears to contain {} and a budget file should not: use roles \
             (not names) and costs only, or remove the column. Nothing from the file was read.",
            f.column,
            lt.label,
            f.kind.label()
        )));
    }
    let h = &lt.table.headers;
    let need = |aliases: &[&str], what: &str| -> Result<usize> {
        find(h, aliases).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{}' needs a '{what}' column. Columns it has: {}",
                lt.label,
                h.join(", ")
            ))
        })
    };
    let c_cat = need(CATEGORY, "category")?;
    let c_item = need(ITEM, "item (or role)")?;
    let c_fund = find(h, FUNDING);
    let c_sal = find(h, SALARY);
    let c_eff = find(h, EFFORT);
    let c_fte = find(h, FTE);
    let c_qty = find(h, QUANTITY);
    let c_uc = find(h, UNIT_COST);
    let c_unit = find(h, UNIT);
    let c_fr = find(h, FRINGE);
    let c_esc = find(h, ESCALATE);
    let c_first = find(h, FIRST);
    let c_last = find(h, LAST);
    let c_sub = find(h, SUBAWARD);
    let c_ex = find(h, EXCLUDE);
    let cell = |row: &Vec<String>, c: Option<usize>| -> String {
        c.and_then(|i| row.get(i))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let mut items = Vec::new();
    for (ri, row) in lt.table.rows.iter().enumerate() {
        if row.iter().all(|v| v.trim().is_empty()) {
            continue;
        }
        let n = ri + 2; // spreadsheet row number
        let cat = Category::parse(&cell(row, Some(c_cat))).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "row {n}: the category is not personnel, travel, equipment, supplies, contractual, or other"
            ))
        })?;
        let num = |c: Option<usize>, what: &str| -> Result<Option<f64>> {
            let t = cell(row, c);
            if t.is_empty() {
                return Ok(None);
            }
            number(&t)
                .map(Some)
                .ok_or_else(|| PhnError::InvalidInput(format!("row {n}: '{what}' is not a number")))
        };
        let funding = {
            let t = cell(row, c_fund);
            if t.is_empty() {
                None
            } else {
                Some(Funding::parse(&t).ok_or_else(|| {
                    PhnError::InvalidInput(format!(
                        "row {n}: funding must be federal or cost share"
                    ))
                })?)
            }
        };
        let flag = |c: Option<usize>, what: &str| -> Result<Option<bool>> {
            let t = cell(row, c);
            if t.is_empty() {
                return Ok(None);
            }
            crate::localdata::truthy(&t).map(Some).ok_or_else(|| {
                PhnError::InvalidInput(format!("row {n}: '{what}' must be yes or no"))
            })
        };
        let mut effort = num(c_eff, "effort percent")?;
        if effort.is_none()
            && let Some(f) = num(c_fte, "FTE")?
        {
            effort = Some(f * 100.0);
        }
        let exclusion = {
            let t = cell(row, c_ex);
            if t.is_empty() {
                None
            } else {
                Some(MtdcExclusion::parse(&t).ok_or_else(|| {
                    PhnError::InvalidInput(format!("row {n}: the MTDC exclusion is not recognized"))
                })?)
            }
        };
        let yr = |c: Option<usize>, what: &str| -> Result<Option<u32>> {
            Ok(num(c, what)?.map(|v| v.max(0.0) as u32))
        };
        items.push(LineItemArg {
            category: cat,
            item: cell(row, Some(c_item)),
            funding,
            annual_salary: num(c_sal, "annual salary")?,
            effort_percent: effort,
            quantity: num(c_qty, "quantity")?,
            unit_cost: num(c_uc, "unit cost")?,
            unit: Some(cell(row, c_unit)).filter(|s| !s.is_empty()),
            fringe_rate_percent: num(c_fr, "fringe rate")?,
            escalate: flag(c_esc, "escalate")?,
            first_year: yr(c_first, "first year")?,
            last_year: yr(c_last, "last year")?,
            subaward: flag(c_sub, "subaward")?,
            exclude_from_mtdc: exclusion,
        });
        if items.len() > MAX_LINES {
            return Err(PhnError::InvalidInput(format!(
                "a budget file may have at most {MAX_LINES} line items"
            )));
        }
    }
    if items.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "'{}' has no line items",
            lt.label
        )));
    }
    let parsed = from_args(&items, years).map_err(|e| match e {
        PhnError::InvalidInput(m) => PhnError::InvalidInput(format!("'{}': {m}", lt.label)),
        o => o,
    })?;
    Ok(FileLines {
        parsed,
        label: lt.label.clone(),
        source: lt.source(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nurse(salary: f64, effort: f64) -> LineItemArg {
        LineItemArg {
            category: Category::Personnel,
            item: "Public health nurse".into(),
            funding: None,
            annual_salary: Some(salary),
            effort_percent: Some(effort),
            quantity: None,
            unit_cost: None,
            unit: None,
            fringe_rate_percent: None,
            escalate: None,
            first_year: None,
            last_year: None,
            subaward: None,
            exclude_from_mtdc: None,
        }
    }

    #[test]
    fn personnel_lines_need_salary_and_effort() {
        let mut l = nurse(80_000.0, 50.0);
        l.effort_percent = None;
        assert!(from_args(&[l], Some(1)).is_err());
        let mut l = nurse(80_000.0, 50.0);
        l.unit_cost = Some(5.0);
        assert!(from_args(&[l], Some(1)).is_err());
        let p = from_args(&[nurse(80_000.0, 50.0)], None).unwrap();
        assert_eq!(p.lines[0].quantity, 0.5);
        assert_eq!(p.lines[0].last_year, 1);
    }

    #[test]
    fn years_default_to_the_largest_named_year() {
        let mut l = nurse(80_000.0, 100.0);
        l.last_year = Some(3);
        let mut m = nurse(60_000.0, 50.0);
        m.last_year = None;
        let p = from_args(&[l, m], None).unwrap();
        assert_eq!(p.max_named_year, 3);
        assert_eq!(p.lines[1].last_year, 3);
    }

    #[test]
    fn numbers_read_dollars_and_percents() {
        assert_eq!(number("$1,200.50"), Some(1200.5));
        assert_eq!(number("50%"), Some(50.0));
        assert_eq!(number(""), None);
        assert_eq!(number("abc"), None);
    }

    #[test]
    fn subaward_only_on_contractual_lines() {
        let mut l = nurse(1.0, 1.0);
        l.subaward = Some(true);
        assert!(from_args(&[l], Some(1)).is_err());
    }
}
