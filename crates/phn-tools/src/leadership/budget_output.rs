//! Summary text, report, and workbook (live formulas) for `analyze_health_department_budget`.

use std::path::Path;

use chrono::{Datelike as _, NaiveDate};
use phn_core::{PhnError, Result};
use rust_xlsxwriter::{ExcelDateTime, Format, Formula, Workbook, XlsxError};

use super::budget::{BudgetResult, CategoryLine, OutputInputs, Totals, round2};
use crate::context::Context;
use crate::md;
use crate::school::templates::{self, DocFormat};

fn money(x: Option<f64>) -> String {
    x.map_or("hidden".into(), |v| md::num(v, 0))
}

fn pct(x: Option<f64>) -> String {
    x.map_or("-".into(), |v| format!("{}%", md::num(v, 1)))
}

fn forecast(x: Option<f64>) -> String {
    x.map_or("-".into(), |v| md::num(v, 0))
}

fn category_table(lines: &[CategoryLine], expense: bool, monthly: bool) -> String {
    let mut heads = vec!["Category", "Budgeted", "Year to date"];
    if expense {
        heads.push("Encumbered");
    }
    heads.extend([
        "% of budget",
        "vs. pace (pts)",
        "Status",
        "Year-end, straight line",
    ]);
    if monthly {
        heads.push("Year-end, run rate");
    }
    let rows: Vec<Vec<String>> = lines
        .iter()
        .map(|l| {
            let mut r = vec![l.category.clone(), money(l.budgeted), money(l.actual)];
            if expense {
                r.push(money(l.encumbered));
            }
            r.push(pct(l.percent_of_budget));
            r.push(l.points_vs_pace.map_or("-".into(), |p| format!("{p:+.1}")));
            r.push(l.status.clone().unwrap_or_default());
            r.push(forecast(l.forecast_straight_line));
            if monthly {
                r.push(forecast(l.forecast_run_rate));
            }
            r
        })
        .collect();
    md::table(&heads, &rows)
}

fn totals_line(label: &str, t: &Totals) -> String {
    format!(
        "- **{label}{}:** budgeted {}, year to date {} ({} of budget), straight-line year-end {}{}.\n",
        if t.excludes_personnel {
            " (personnel left out: hidden)"
        } else {
            ""
        },
        md::num(t.budgeted, 0),
        md::num(t.actual, 0),
        pct(t.percent_of_budget),
        forecast(t.forecast_straight_line),
        t.forecast_run_rate.map_or(String::new(), |r| format!(
            ", run-rate year-end {}",
            md::num(r, 0)
        ))
    )
}

fn mix_table(r: &BudgetResult) -> String {
    let rows: Vec<Vec<String>> = r
        .revenue_mix
        .iter()
        .map(|m| {
            vec![
                m.group.clone(),
                md::num(m.budgeted, 0),
                pct(m.percent_of_budgeted_revenue),
                md::num(m.actual, 0),
                pct(m.percent_of_actual_revenue),
            ]
        })
        .collect();
    md::table(
        &[
            "Revenue group",
            "Budgeted",
            "% of budgeted",
            "Year to date",
            "% of year to date",
        ],
        &rows,
    )
}

fn services_table(r: &BudgetResult) -> String {
    let rows: Vec<Vec<String>> = r
        .services
        .iter()
        .map(|s| {
            vec![
                s.service.clone(),
                md::num(s.volume, 0),
                s.total_cost.map_or("withheld".into(), |v| md::num(v, 0)),
                s.cost_per_unit.map_or("withheld".into(), |v| md::num(v, 2)),
                s.budgeted_cost_per_unit
                    .map_or("-".into(), |v| md::num(v, 2)),
            ]
        })
        .collect();
    md::table(
        &[
            "Service",
            "Units so far",
            "Cost so far",
            "Cost per unit",
            "Budgeted cost per planned unit",
        ],
        &rows,
    )
}

/// The markdown summary returned to the model.
pub fn summary(r: &BudgetResult) -> String {
    let fy = &r.fiscal_year;
    let mut s = format!(
        "**Department budget{}: {}**, as of {} ({:.1}% of the year has passed; the year runs {} to {}).\n\n",
        r.department
            .as_ref()
            .map(|d| format!(" for {d}"))
            .unwrap_or_default(),
        fy.label,
        fy.as_of,
        fy.percent_elapsed,
        fy.start,
        fy.end
    );
    let monthly = r.layout == "monthly ledger";
    if let Some(t) = &r.expense_totals {
        s.push_str(&totals_line("Expenses", t));
    }
    if let Some(t) = &r.revenue_totals {
        s.push_str(&totals_line("Revenue", t));
    }
    if let Some(n) = &r.net {
        s.push_str(&format!(
            "- **Revenue minus expenses:** budgeted {}, year to date {}.\n",
            md::num(n.budgeted, 0),
            md::num(n.actual_ytd, 0)
        ));
    }
    s.push('\n');
    if !r.expenses.is_empty() {
        s.push_str("**Expenses by category**\n\n");
        s.push_str(&category_table(&r.expenses, true, monthly));
        s.push('\n');
    }
    if !r.revenue.is_empty() {
        s.push_str("**Revenue by category**\n\n");
        s.push_str(&category_table(&r.revenue, false, monthly));
        s.push('\n');
    }
    if !r.revenue_mix.is_empty() {
        s.push_str("**Revenue mix**\n\n");
        s.push_str(&mix_table(r));
        s.push('\n');
    }
    if !r.services.is_empty() {
        s.push_str("**Cost per service** (volumes and allocation shares are yours)\n\n");
        s.push_str(&services_table(r));
        s.push('\n');
    }
    if let Some(pc) = &r.per_capita {
        s.push_str(&format!(
            "**Per resident** (population {}): municipal appropriation budget {}, total revenue budget {}. DPH's 2022-2023 capacity assessment reported an average municipal budget of $17.56 per capita across shared services arrangements and cited $36.98 as an estimate of the amount needed for basic capacity (see the sources and caveats).\n\n",
            md::num(pc.population, 0),
            pc.municipal_appropriation_budget_per_resident
                .map_or("-".into(), |v| format!("${}", md::num(v, 2))),
            pc.total_revenue_budget_per_resident
                .map_or("-".into(), |v| format!("${}", md::num(v, 2)))
        ));
    }
    if !r.flags.is_empty() {
        s.push_str("**Flags**\n\n");
        for f in &r.flags {
            s.push_str(&format!("- ({}) {}\n", f.area, f.message));
        }
        s.push('\n');
    }
    s.push_str("Forecasts: straight line (year to date divided by the share of the year passed)");
    if monthly {
        s.push_str(" and run rate (recent monthly average carried forward)");
    }
    s.push_str(". See the assumptions in the data.\n");
    if let (Some(b), Some(d)) = (&r.workbook_file, &r.report_file) {
        s.push_str(&format!(
            "\nWorkbook with live formulas: {b}. Report: {d}.\n"
        ));
    } else if let Some(b) = &r.workbook_file {
        s.push_str(&format!("\nWorkbook with live formulas: {b}.\n"));
    } else if let Some(d) = &r.report_file {
        s.push_str(&format!("\nReport: {d}.\n"));
    }
    s
}

fn report_markdown(r: &BudgetResult) -> String {
    let fy = &r.fiscal_year;
    let monthly = r.layout == "monthly ledger";
    let mut s = format!(
        "# Department budget analysis{} {}\n\n",
        r.department
            .as_ref()
            .map(|d| format!(": {d},"))
            .unwrap_or_default(),
        fy.label
    );
    s.push_str(&format!(
        "As of {}. The fiscal year runs {} to {}; {:.1}% of the year ({} of {} days) has passed. Prepared from the file the department supplied; the figures have not been checked against the general ledger.\n\n",
        fy.as_of, fy.start, fy.end, fy.percent_elapsed, fy.days_elapsed, fy.days_in_year
    ));
    s.push_str("## Where the year stands\n\n");
    if let Some(t) = &r.expense_totals {
        s.push_str(&totals_line("Expenses", t));
    }
    if let Some(t) = &r.revenue_totals {
        s.push_str(&totals_line("Revenue", t));
    }
    if let Some(n) = &r.net {
        s.push_str(&format!(
            "- **Revenue minus expenses:** budgeted {}, year to date {}.\n",
            md::num(n.budgeted, 0),
            md::num(n.actual_ytd, 0)
        ));
    }
    s.push('\n');
    if !r.expenses.is_empty() {
        s.push_str("## Expenses by category\n\n");
        s.push_str(&category_table(&r.expenses, true, monthly));
        s.push_str("\nPersonnel is shown only as one combined row, and only when it has enough positions to avoid identifying a person's pay.\n\n");
    }
    if !r.revenue.is_empty() {
        s.push_str("## Revenue by category\n\n");
        s.push_str(&category_table(&r.revenue, false, monthly));
        s.push('\n');
    }
    if !r.revenue_mix.is_empty() {
        s.push_str("## Revenue mix\n\n");
        s.push_str(&mix_table(r));
        s.push('\n');
        if !r.unclassified_revenue_labels.is_empty() {
            s.push_str(&format!(
                "Revenue labels counted as not classified: {}.\n\n",
                r.unclassified_revenue_labels.join("; ")
            ));
        }
    }
    if !r.services.is_empty() {
        s.push_str("## Cost per service\n\n");
        s.push_str(&services_table(r));
        s.push_str("\nVolumes, cost allocation shares, and any indirect rate are the department's own figures.\n\n");
    }
    if !r.flags.is_empty() {
        s.push_str("## Flags\n\n");
        for f in &r.flags {
            s.push_str(&format!("- {}\n", f.message));
        }
        s.push('\n');
    }
    s.push_str("## How the year-end forecast was made\n\n");
    for a in &r.forecast_assumptions {
        s.push_str(&format!("- {a}\n"));
    }
    s.push('\n');
    s.push_str("## State Public Health Excellence funding, as DPH describes it\n\n");
    for p in &r.public_health_excellence {
        s.push_str(&format!("- {} ({})\n", p.text, p.citation));
    }
    s.push_str("\nThese statements come from DPH's published pages and are pending review by the department's subject matter expert. The award terms were not read; check allowable costs and reporting in the award contract.\n\n");
    s.push_str("## About this report\n\n");
    s.push_str("This is decision support for the department's own review. It contains category totals only: no account codes, vendors, or employees. Check the figures against the accounting system before sharing them with the board of health or the town finance office.\n");
    s
}

// ---------------------------------------------------------------- workbook

type X<T> = std::result::Result<T, XlsxError>;

fn col(c: u16) -> String {
    rust_xlsxwriter::utility::column_number_to_name(c)
}

fn num_s(x: Option<f64>) -> String {
    x.map_or(String::new(), |v| format!("{}", round2(v)))
}

fn excel_date(d: NaiveDate) -> X<ExcelDateTime> {
    ExcelDateTime::from_ymd(d.year() as u16, d.month() as u8, d.day() as u8)
}

struct Styles {
    bold: Format,
    head: Format,
    money: Format,
    pct: Format,
    one: Format,
    date: Format,
    input: Format,
}

fn styles() -> Styles {
    Styles {
        bold: Format::new().set_bold(),
        head: Format::new().set_bold().set_text_wrap(),
        money: Format::new().set_num_format("#,##0;[Red]-#,##0"),
        pct: Format::new().set_num_format("0.0%"),
        one: Format::new().set_num_format("0.0"),
        date: Format::new().set_num_format("yyyy-mm-dd"),
        input: Format::new()
            .set_num_format("#,##0.00")
            .set_background_color(rust_xlsxwriter::Color::RGB(0xFFF2CC)),
    }
}

const A_ELAPSED: &str = "Assumptions!$B$6";
const A_TOL: &str = "Assumptions!$B$7";
const A_MONTHS: &str = "Assumptions!$B$8";
const A_INDIRECT: &str = "Assumptions!$B$9";

#[allow(clippy::too_many_lines)]
fn build_workbook(i: &OutputInputs) -> X<Vec<u8>> {
    let r = i.result;
    let st = styles();
    let mut wb = Workbook::new();

    // ---- Assumptions (first tab so formulas can refer to it).
    {
        let ws = wb.add_worksheet();
        ws.set_name("Assumptions")?;
        ws.set_column_width(0, 52)?;
        ws.set_column_width(1, 16)?;
        ws.write_string_with_format(
            0,
            0,
            format!(
                "Department budget analysis{}: {}",
                r.department
                    .as_ref()
                    .map(|d| format!(" for {d}"))
                    .unwrap_or_default(),
                r.fiscal_year.label
            ),
            &st.bold,
        )?;
        ws.write_string(
            1,
            0,
            "Yellow cells are inputs: change them and the sheets recalculate.",
        )?;
        ws.write_string(2, 0, "Fiscal year starts")?;
        ws.write_datetime_with_format(2, 1, excel_date(i.fy_start)?, &st.date)?;
        ws.write_string(3, 0, "As-of date")?;
        ws.write_datetime_with_format(3, 1, excel_date(i.as_of)?, &st.date)?;
        ws.write_string(4, 0, "Days in the fiscal year")?;
        ws.write_formula(
            4,
            1,
            Formula::new("=EDATE(B3,12)-B3").set_result(format!("{}", r.fiscal_year.days_in_year)),
        )?;
        ws.write_string(
            5,
            0,
            "Share of the year that has passed (as-of date counted)",
        )?;
        ws.write_formula_with_format(
            5,
            1,
            Formula::new("=MIN(1,(B4-B3+1)/B5)").set_result(format!("{}", i.elapsed)),
            &st.pct,
        )?;
        ws.write_string(6, 0, "Pace tolerance (percentage points)")?;
        ws.write_number_with_format(6, 1, i.tolerance, &st.input)?;
        ws.write_string(7, 0, "Months in the year (for the run-rate forecast)")?;
        ws.write_number(7, 1, 12.0)?;
        ws.write_string(8, 0, "Indirect cost rate for cost per service (%)")?;
        ws.write_number_with_format(8, 1, i.indirect_rate, &st.input)?;
        if let Some(pc) = &r.per_capita {
            ws.write_string(9, 0, "Population served")?;
            ws.write_number_with_format(9, 1, pc.population, &st.input)?;
        }
        ws.write_string(
            11,
            0,
            "Forecasts: straight line = year to date / share of year passed. Run rate = year to date + recent monthly average x months in year x share of year remaining.",
        )?;
        ws.write_string(
            12,
            0,
            "Personnel is one combined row and is left out of every sheet when it has too few positions.",
        )?;
    }

    // ---- Expenses.
    {
        let ws = wb.add_worksheet();
        ws.set_name("Expenses")?;
        ws.write_string_with_format(0, 0, "Expenses by category", &st.bold)?;
        let heads = [
            "Category",
            "Budgeted",
            "Year to date",
            "Encumbered",
            "Remaining",
            "% of budget used",
            "Expected by pace",
            "Actual minus pace",
            "Points vs. pace",
            "Status",
            "Straight-line year-end",
            "Recent monthly average (input)",
            "Run-rate year-end",
            "Budget minus straight-line year-end",
        ];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(3, c as u16, *h, &st.head)?;
            ws.set_column_width(c as u16, if c == 0 { 44 } else { 15 })?;
        }
        let first = 5u32;
        let mut n = 0u32;
        for (k, l) in r.expenses.iter().enumerate() {
            let row = 4 + n;
            n += 1;
            let xr = row + 1;
            ws.write_string(row, 0, &l.category)?;
            let Some(a) = i.expense_aggs[k] else {
                ws.write_string(row, 1, "hidden: too few positions")?;
                continue;
            };
            ws.write_number_with_format(row, 1, round2(a.budget), &st.money)?;
            ws.write_number_with_format(row, 2, round2(a.actual), &st.money)?;
            ws.write_number_with_format(row, 3, round2(a.enc), &st.money)?;
            if let Some(avg) = i.expense_avgs[k] {
                ws.write_number_with_format(row, 11, avg, &st.input)?;
            }
            expense_formulas(ws, row, xr, l, i.expense_avgs[k].is_some(), &st)?;
        }
        let last = 4 + n; // 1-based row of the last data line
        let tr = 4 + n + 1;
        let txr = tr + 1;
        let label = match &r.expense_totals {
            Some(t) if t.excludes_personnel => "Total (personnel left out)",
            _ => "Total",
        };
        ws.write_string_with_format(tr, 0, label, &st.bold)?;
        if let Some(t) = &r.expense_totals {
            for (c, cached) in [
                (1u16, t.budgeted),
                (2, t.actual),
                (3, t.encumbered.unwrap_or(0.0)),
            ] {
                ws.write_formula_with_format(
                    tr,
                    c,
                    Formula::new(format!("=SUM({0}{first}:{0}{last})", col(c)))
                        .set_result(num_s(Some(cached))),
                    &st.money,
                )?;
            }
            let fake = total_line(t, false);
            expense_formulas(ws, tr, txr, &fake, false, &st)?;
            // Totals: the run rate and straight line are sums of the lines'.
            ws.write_formula_with_format(
                tr,
                10,
                Formula::new(format!("=SUM(K{first}:K{last})"))
                    .set_result(num_s(t.forecast_straight_line)),
                &st.money,
            )?;
            if t.forecast_run_rate.is_some() {
                ws.write_formula_with_format(
                    tr,
                    12,
                    Formula::new(format!("=SUM(M{first}:M{last})"))
                        .set_result(num_s(t.forecast_run_rate)),
                    &st.money,
                )?;
            }
        }
        ws.write_string(
            tr + 2,
            0,
            "Status: Over budget when year to date plus encumbrances exceeds the budget; Ahead/Behind pace when points vs. pace are beyond the tolerance on the Assumptions sheet.",
        )?;
        ws.set_freeze_panes(4, 1)?;
    }

    // ---- Revenue.
    if !r.revenue.is_empty() {
        let ws = wb.add_worksheet();
        ws.set_name("Revenue")?;
        ws.write_string_with_format(0, 0, "Revenue by category", &st.bold)?;
        let heads = [
            "Category",
            "Budgeted",
            "Year to date",
            "Still to collect",
            "% of budget received",
            "Expected by pace",
            "Actual minus pace",
            "Points vs. pace",
            "Status",
            "Straight-line year-end",
            "Recent monthly average (input)",
            "Run-rate year-end",
            "Straight-line year-end minus budget",
        ];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(3, c as u16, *h, &st.head)?;
            ws.set_column_width(c as u16, if c == 0 { 44 } else { 15 })?;
        }
        let first = 5u32;
        let mut n = 0u32;
        for (k, l) in r.revenue.iter().enumerate() {
            let row = 4 + n;
            n += 1;
            let Some(a) = i.revenue_aggs[k] else { continue };
            ws.write_string(row, 0, &l.category)?;
            ws.write_number_with_format(row, 1, round2(a.budget), &st.money)?;
            ws.write_number_with_format(row, 2, round2(a.actual), &st.money)?;
            if let Some(avg) = i.revenue_avgs[k] {
                ws.write_number_with_format(row, 10, avg, &st.input)?;
            }
            revenue_formulas(ws, row, row + 1, l, i.revenue_avgs[k].is_some(), &st)?;
        }
        let last = 4 + n;
        let tr = 4 + n + 1;
        ws.write_string_with_format(tr, 0, "Total", &st.bold)?;
        if let Some(t) = &r.revenue_totals {
            for (c, cached) in [(1u16, t.budgeted), (2, t.actual)] {
                ws.write_formula_with_format(
                    tr,
                    c,
                    Formula::new(format!("=SUM({0}{first}:{0}{last})", col(c)))
                        .set_result(num_s(Some(cached))),
                    &st.money,
                )?;
            }
            let fake = total_line(t, true);
            revenue_formulas(ws, tr, tr + 1, &fake, false, &st)?;
            ws.write_formula_with_format(
                tr,
                9,
                Formula::new(format!("=SUM(J{first}:J{last})"))
                    .set_result(num_s(t.forecast_straight_line)),
                &st.money,
            )?;
            if t.forecast_run_rate.is_some() {
                ws.write_formula_with_format(
                    tr,
                    11,
                    Formula::new(format!("=SUM(L{first}:L{last})"))
                        .set_result(num_s(t.forecast_run_rate)),
                    &st.money,
                )?;
            }
        }
        ws.set_freeze_panes(4, 1)?;
    }

    // ---- Revenue mix.
    if !i.mix.is_empty() {
        let ws = wb.add_worksheet();
        ws.set_name("Revenue mix")?;
        ws.write_string_with_format(0, 0, "Revenue mix", &st.bold)?;
        for (c, h) in [
            "Revenue group",
            "Budgeted",
            "% of budgeted",
            "Year to date",
            "% of year to date",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(3, c as u16, *h, &st.head)?;
            ws.set_column_width(c as u16, if c == 0 { 50 } else { 16 })?;
        }
        let n = i.mix.len() as u32;
        let (first, last) = (5u32, 4 + n);
        let tb: f64 = i.mix.iter().map(|m| m.1).sum();
        let ta: f64 = i.mix.iter().map(|m| m.2).sum();
        let mut municipal_row = None;
        for (k, (label, b, a)) in i.mix.iter().enumerate() {
            let row = 4 + k as u32;
            let xr = row + 1;
            ws.write_string(row, 0, label)?;
            if r.revenue_mix[k].group_id == "municipal" {
                municipal_row = Some(xr);
            }
            ws.write_number_with_format(row, 1, round2(*b), &st.money)?;
            ws.write_formula_with_format(
                row,
                2,
                Formula::new(format!("=IF(B${tr}=0,\"\",B{xr}/B${tr})", tr = last + 1)).set_result(
                    if tb == 0.0 {
                        String::new()
                    } else {
                        format!("{}", b / tb)
                    },
                ),
                &st.pct,
            )?;
            ws.write_number_with_format(row, 3, round2(*a), &st.money)?;
            ws.write_formula_with_format(
                row,
                4,
                Formula::new(format!("=IF(D${tr}=0,\"\",D{xr}/D${tr})", tr = last + 1)).set_result(
                    if ta == 0.0 {
                        String::new()
                    } else {
                        format!("{}", a / ta)
                    },
                ),
                &st.pct,
            )?;
        }
        let tr = 4 + n;
        ws.write_string_with_format(tr, 0, "Total", &st.bold)?;
        for (c, cached) in [(1u16, tb), (3, ta)] {
            ws.write_formula_with_format(
                tr,
                c,
                Formula::new(format!("=SUM({0}{first}:{0}{last})", col(c)))
                    .set_result(num_s(Some(cached))),
                &st.money,
            )?;
        }
        if let (Some(pc), Some(mr)) = (&r.per_capita, municipal_row) {
            ws.write_string(tr + 2, 0, "Municipal appropriation budget per resident")?;
            ws.write_formula_with_format(
                tr + 2,
                1,
                Formula::new(format!(
                    "=IF(Assumptions!B10=\"\",\"\",B{mr}/Assumptions!B10)"
                ))
                .set_result(num_s(pc.municipal_appropriation_budget_per_resident)),
                &st.one,
            )?;
        }
    }

    // ---- Services.
    if !i.service_inputs.is_empty() {
        let ws = wb.add_worksheet();
        ws.set_name("Cost per service")?;
        ws.write_string_with_format(0, 0, "Cost per service", &st.bold)?;
        ws.write_string(
            1,
            0,
            "Volumes and the shares of each category are your figures. Cost so far and budgeted cost are the share-weighted category totals.",
        )?;
        for (c, h) in [
            "Service",
            "Units so far",
            "Cost so far (direct)",
            "Cost so far with indirect rate",
            "Cost per unit",
            "Planned units for the year",
            "Budgeted cost (direct)",
            "Budgeted cost per planned unit",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(3, c as u16, *h, &st.head)?;
            ws.set_column_width(c as u16, if c == 0 { 40 } else { 16 })?;
        }
        for (k, s) in i.service_inputs.iter().enumerate() {
            let row = 4 + k as u32;
            let xr = row + 1;
            let sv = &r.services[k];
            ws.write_string(row, 0, &s.name)?;
            ws.write_number(row, 1, s.volume)?;
            let Some(direct) = s.direct_cost else {
                ws.write_string(row, 2, "withheld: depends on hidden personnel amounts")?;
                continue;
            };
            ws.write_number_with_format(row, 2, round2(direct), &st.input)?;
            ws.write_formula_with_format(
                row,
                3,
                Formula::new(format!("=C{xr}*(1+{A_INDIRECT}/100)"))
                    .set_result(num_s(sv.total_cost)),
                &st.money,
            )?;
            ws.write_formula_with_format(
                row,
                4,
                Formula::new(format!("=IF(B{xr}=0,\"\",D{xr}/B{xr})"))
                    .set_result(num_s(sv.cost_per_unit)),
                &st.money,
            )?;
            if let (Some(p), Some(b)) = (s.planned_volume, s.budget_cost) {
                ws.write_number(row, 5, p)?;
                ws.write_number_with_format(row, 6, round2(b), &st.input)?;
                ws.write_formula_with_format(
                    row,
                    7,
                    Formula::new(format!(
                        "=IF(F{xr}=0,\"\",G{xr}*(1+{A_INDIRECT}/100)/F{xr})"
                    ))
                    .set_result(num_s(sv.budgeted_cost_per_unit)),
                    &st.money,
                )?;
            }
        }
    }
    wb.save_to_buffer()
}

/// A category line made from totals (for the workbook's total row formulas' cached values).
fn total_line(t: &Totals, revenue: bool) -> CategoryLine {
    CategoryLine {
        category: "Total".into(),
        is_personnel: false,
        hidden: false,
        budgeted: Some(t.budgeted),
        actual: Some(t.actual),
        encumbered: (!revenue).then_some(t.encumbered.unwrap_or(0.0)),
        remaining: Some(t.remaining),
        percent_of_budget: t.percent_of_budget,
        expected_by_pace: Some(t.expected_by_pace),
        variance_vs_pace: Some(t.variance_vs_pace),
        points_vs_pace: t.points_vs_pace,
        status: None,
        forecast_straight_line: t.forecast_straight_line,
        forecast_run_rate: t.forecast_run_rate,
        projected_vs_budget_straight_line: t.projected_vs_budget_straight_line,
        projected_vs_budget_run_rate: t.projected_vs_budget_run_rate,
    }
}

fn expense_formulas(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    xr: u32,
    l: &CategoryLine,
    has_avg: bool,
    st: &Styles,
) -> X<()> {
    let f = |s: String, cached: String| Formula::new(s).set_result(cached);
    ws.write_formula_with_format(
        row,
        4,
        f(format!("=B{xr}-C{xr}-D{xr}"), num_s(l.remaining)),
        &st.money,
    )?;
    ws.write_formula_with_format(
        row,
        5,
        f(
            format!("=IF(B{xr}=0,\"\",C{xr}/B{xr})"),
            l.percent_of_budget
                .map_or(String::new(), |p| format!("{}", p / 100.0)),
        ),
        &st.pct,
    )?;
    ws.write_formula_with_format(
        row,
        6,
        f(format!("=B{xr}*{A_ELAPSED}"), num_s(l.expected_by_pace)),
        &st.money,
    )?;
    ws.write_formula_with_format(
        row,
        7,
        f(format!("=C{xr}-G{xr}"), num_s(l.variance_vs_pace)),
        &st.money,
    )?;
    ws.write_formula_with_format(
        row,
        8,
        f(
            format!("=IF(B{xr}=0,\"\",(F{xr}-{A_ELAPSED})*100)"),
            num_s(l.points_vs_pace),
        ),
        &st.one,
    )?;
    if let Some(s) = &l.status {
        ws.write_formula(
            row,
            9,
            f(
                format!(
                    "=IF(B{xr}=0,IF(C{xr}>0,\"No budget\",\"On pace\"),IF(C{xr}+D{xr}>B{xr},\"Over budget\",IF(I{xr}>{A_TOL},\"Ahead of pace\",IF(I{xr}<-{A_TOL},\"Behind pace\",\"On pace\"))))"
                ),
                s.clone(),
            ),
        )?;
    }
    ws.write_formula_with_format(
        row,
        10,
        f(
            format!("=IF({A_ELAPSED}=0,\"\",C{xr}/{A_ELAPSED})"),
            num_s(l.forecast_straight_line),
        ),
        &st.money,
    )?;
    if has_avg {
        ws.write_formula_with_format(
            row,
            12,
            f(
                format!("=IF(L{xr}=\"\",\"\",C{xr}+L{xr}*{A_MONTHS}*(1-{A_ELAPSED}))"),
                num_s(l.forecast_run_rate),
            ),
            &st.money,
        )?;
    }
    ws.write_formula_with_format(
        row,
        13,
        f(
            format!("=IF(K{xr}=\"\",\"\",B{xr}-K{xr})"),
            num_s(l.projected_vs_budget_straight_line),
        ),
        &st.money,
    )?;
    Ok(())
}

fn revenue_formulas(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    xr: u32,
    l: &CategoryLine,
    has_avg: bool,
    st: &Styles,
) -> X<()> {
    let f = |s: String, cached: String| Formula::new(s).set_result(cached);
    ws.write_formula_with_format(
        row,
        3,
        f(format!("=B{xr}-C{xr}"), num_s(l.remaining)),
        &st.money,
    )?;
    ws.write_formula_with_format(
        row,
        4,
        f(
            format!("=IF(B{xr}=0,\"\",C{xr}/B{xr})"),
            l.percent_of_budget
                .map_or(String::new(), |p| format!("{}", p / 100.0)),
        ),
        &st.pct,
    )?;
    ws.write_formula_with_format(
        row,
        5,
        f(format!("=B{xr}*{A_ELAPSED}"), num_s(l.expected_by_pace)),
        &st.money,
    )?;
    ws.write_formula_with_format(
        row,
        6,
        f(format!("=C{xr}-F{xr}"), num_s(l.variance_vs_pace)),
        &st.money,
    )?;
    ws.write_formula_with_format(
        row,
        7,
        f(
            format!("=IF(B{xr}=0,\"\",(E{xr}-{A_ELAPSED})*100)"),
            num_s(l.points_vs_pace),
        ),
        &st.one,
    )?;
    if let Some(s) = &l.status {
        ws.write_formula(
            row,
            8,
            f(
                format!(
                    "=IF(B{xr}=0,\"On pace\",IF(H{xr}>{A_TOL},\"Ahead of pace\",IF(H{xr}<-{A_TOL},\"Behind pace\",\"On pace\")))"
                ),
                s.clone(),
            ),
        )?;
    }
    ws.write_formula_with_format(
        row,
        9,
        f(
            format!("=IF({A_ELAPSED}=0,\"\",C{xr}/{A_ELAPSED})"),
            num_s(l.forecast_straight_line),
        ),
        &st.money,
    )?;
    if has_avg {
        ws.write_formula_with_format(
            row,
            11,
            f(
                format!("=IF(K{xr}=\"\",\"\",C{xr}+K{xr}*{A_MONTHS}*(1-{A_ELAPSED}))"),
                num_s(l.forecast_run_rate),
            ),
            &st.money,
        )?;
    }
    ws.write_formula_with_format(
        row,
        12,
        f(
            format!("=IF(J{xr}=\"\",\"\",J{xr}-B{xr})"),
            num_s(l.projected_vs_budget_straight_line),
        ),
        &st.money,
    )?;
    Ok(())
}

/// Write the workbook and report; returns their display paths.
pub(crate) fn write_files(
    ctx: &Context,
    dir: &Path,
    i: &OutputInputs,
    want_book: bool,
    want_report: bool,
    format: DocFormat,
) -> Result<(Option<String>, Option<String>)> {
    let mut book = None;
    let mut report = None;
    if want_book {
        let bytes = build_workbook(i).map_err(|e| PhnError::Internal(format!("XLSX: {e}")))?;
        book = Some(templates::write_in(
            ctx,
            dir,
            "department_budget.xlsx",
            &bytes,
        )?);
    }
    if want_report {
        let md = report_markdown(i.result);
        let doc = templates::document(ctx, &md, "en-US", "Department budget analysis");
        report = Some(templates::write_in(
            ctx,
            dir,
            &format!("department_budget_summary.{}", format.ext()),
            &format.render(&doc)?,
        )?);
    }
    Ok((book, report))
}
