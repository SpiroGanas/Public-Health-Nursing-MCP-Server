//! Summary text, report, and workbook (live formulas) for `model_staffing_needs`.

use std::path::Path;

use phn_core::{PhnError, Result};
use rust_xlsxwriter::{Color, Format, Formula, Workbook, XlsxError};

use super::staffing::{StaffingResult, round2};
use crate::context::Context;
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Typed inputs the workbook needs beyond the result.
pub(crate) struct OutputInputs<'a> {
    pub result: &'a StaffingResult,
    /// Added services by scenario (index 0 is the baseline, with none): name, volume, minutes.
    pub added: &'a [Vec<(String, f64, f64)>],
    pub current_fte: Option<f64>,
    pub population: Option<f64>,
    pub clients: Option<f64>,
    pub non_service: f64,
    pub paid: f64,
    pub holiday: f64,
    pub vacation: f64,
    pub sick: f64,
    pub training: f64,
}

fn opt(x: Option<f64>, d: usize) -> String {
    x.map_or("-".into(), |v| md::num(v, d))
}

fn scenario_table(r: &StaffingResult) -> String {
    let rows: Vec<Vec<String>> = r
        .scenarios
        .iter()
        .map(|s| {
            vec![
                s.name.clone(),
                md::num(s.total_hours, 0),
                md::num(s.fte_needed, 2),
                md::num(s.fte_rounded_up, 2),
                s.gap_vs_current_fte
                    .map_or("-".into(), |g| format!("{g:+.2}")),
                opt(s.residents_per_fte_needed, 0),
                opt(s.clients_per_fte_needed, 0),
            ]
        })
        .collect();
    md::table(
        &[
            "Scenario",
            "Hours of work",
            "FTE needed",
            "FTE rounded up",
            "More (+) or fewer (-) than now",
            "Residents per FTE",
            "Clients per FTE",
        ],
        &rows,
    )
}

fn hours_table(r: &StaffingResult) -> String {
    let mut rows: Vec<Vec<String>> = r
        .productive_hours
        .inputs
        .iter()
        .map(|i| vec![i.label.clone(), md::num(i.hours, 0), i.basis.clone()])
        .collect();
    rows.push(vec![
        "Share of remaining hours that is not direct service (%)".into(),
        md::num(r.productive_hours.non_service_percent, 0),
        r.productive_hours.non_service_basis.clone(),
    ]);
    rows.push(vec![
        "**Productive hours per FTE**".into(),
        format!(
            "**{}**",
            md::num(r.productive_hours.productive_hours_per_fte, 1)
        ),
        "calculated".into(),
    ]);
    md::table(&["Input", "Value", "Where it came from"], &rows)
}

fn services_table(r: &StaffingResult) -> String {
    let rows: Vec<Vec<String>> = r
        .services
        .iter()
        .map(|s| {
            vec![
                s.service.clone(),
                md::num(s.annual_volume, 0),
                md::num(s.minutes_per_unit, 1),
                md::num(s.hours, 0),
                s.percent_of_hours
                    .map_or("-".into(), |p| format!("{}%", md::num(p, 1))),
            ]
        })
        .collect();
    md::table(
        &[
            "Service",
            "Units per year",
            "Minutes per unit",
            "Hours of work",
            "Share of hours",
        ],
        &rows,
    )
}

fn current_text(r: &StaffingResult) -> String {
    let mut s = String::new();
    if let Some(c) = &r.current {
        s.push_str(&format!(
            "At the current {} FTE, the baseline work is {}% of the hours your staff have for direct service",
            md::num(c.current_fte, 2),
            md::num(c.baseline_workload_percent_of_capacity, 0)
        ));
        if let Some(p) = c.residents_per_fte {
            s.push_str(&format!("; {} residents per FTE", md::num(p, 0)));
        }
        if let Some(p) = c.clients_per_fte {
            s.push_str(&format!("; {} clients per FTE", md::num(p, 1)));
        }
        s.push_str(".\n");
    }
    if let Some(b) = &r.benchmark_comparison {
        s.push_str(&format!(
            "\nBenchmark you supplied ({}; source: {}): ",
            b.label, b.source_given_by_you
        ));
        let mut parts = Vec::new();
        if let (Some(per), Some(f)) = (b.population_per_fte, b.fte_implied_by_population) {
            parts.push(format!(
                "{} residents per FTE implies {} FTE for your population",
                md::num(per, 0),
                md::num(f, 2)
            ));
        }
        if let (Some(per), Some(f)) = (b.clients_per_fte, b.fte_implied_by_clients) {
            parts.push(format!(
                "{} clients per FTE implies {} FTE for your caseload",
                md::num(per, 0),
                md::num(f, 2)
            ));
        }
        s.push_str(&parts.join("; "));
        s.push_str(". This server did not verify it.\n");
    }
    s
}

/// The markdown summary returned to the model.
pub fn summary(r: &StaffingResult) -> String {
    let base = &r.scenarios[0];
    let mut s = format!(
        "**Nurse staffing model{}**: the baseline work is {} hours a year, which is {} FTE at {} productive hours per FTE (rounded up to {}).\n\n",
        r.program
            .as_ref()
            .map(|p| format!(" for {p}"))
            .unwrap_or_default(),
        md::num(base.total_hours, 0),
        md::num(base.fte_needed, 2),
        md::num(r.productive_hours.productive_hours_per_fte, 1),
        md::num(base.fte_rounded_up, 2)
    );
    s.push_str("**Productive hours per FTE**\n\n");
    s.push_str(&hours_table(r));
    s.push_str("\n**Hours of work by service**\n\n");
    s.push_str(&services_table(r));
    s.push_str("\n**Scenarios**\n\n");
    s.push_str(&scenario_table(r));
    s.push('\n');
    s.push_str(&current_text(r));
    s.push_str(
        "\nNo public health nurse staffing benchmark is bundled: none could be read from a source this server could fetch. Minutes per unit and the placeholder leave and training hours are yours to set.\n",
    );
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

fn report_markdown(r: &StaffingResult) -> String {
    let mut s = format!(
        "# Public health nurse staffing model{}\n\n",
        r.program
            .as_ref()
            .map(|p| format!(": {p}"))
            .unwrap_or_default()
    );
    s.push_str("This model turns the service volumes and time standards the department supplied into the number of nurse full-time equivalents (FTE) needed. It is decision support for workforce planning, not a staffing determination.\n\n");
    s.push_str("## Productive hours per FTE\n\n");
    s.push_str(&hours_table(r));
    s.push_str("\nValues marked tool default are placeholders with no published source; replace them with the agency's own.\n\n");
    s.push_str("## Hours of work by service (baseline)\n\n");
    s.push_str(&services_table(r));
    s.push_str(&format!(
        "\nBaseline total: {} hours a year.\n\n",
        md::num(r.baseline_hours, 0)
    ));
    s.push_str("## Scenarios\n\n");
    s.push_str(&scenario_table(r));
    s.push('\n');
    for sc in r.scenarios.iter().skip(1) {
        s.push_str(&format!("- **{}:** {}.\n", sc.name, sc.description));
    }
    s.push('\n');
    let cur = current_text(r);
    if !cur.is_empty() {
        s.push_str("## Current staffing and benchmarks\n\n");
        s.push_str(&cur);
        s.push('\n');
    }
    s.push_str("## Benchmarks that were looked for and not used\n\n");
    for n in &r.benchmarks_not_bundled {
        s.push_str(&format!("- **{}:** {}\n", n.item, n.detail));
    }
    s.push_str("\n## Method\n\nHours of work = volume x minutes per unit / 60, summed over services. Productive hours per FTE = (paid hours - holiday - vacation - sick and personal - training) x (1 - non-service share). FTE needed = hours of work / productive hours per FTE, rounded up to the step shown. Scenarios change volumes or add services; they do not change minutes per unit.\n");
    s
}

// ---------------------------------------------------------------- workbook

type X<T> = std::result::Result<T, XlsxError>;

fn num_s(x: f64) -> String {
    format!("{}", round2(x))
}

#[allow(clippy::too_many_lines)]
fn build_workbook(i: &OutputInputs) -> X<Vec<u8>> {
    let r = i.result;
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let head = Format::new().set_bold().set_text_wrap();
    let one = Format::new().set_num_format("#,##0.0");
    let two = Format::new().set_num_format("0.00");
    let whole = Format::new().set_num_format("#,##0");
    let pct = Format::new().set_num_format("0.0%");
    let input = Format::new()
        .set_num_format("#,##0.00")
        .set_background_color(Color::RGB(0xFFF2CC));

    // ---- Assumptions
    {
        let ws = wb.add_worksheet();
        ws.set_name("Assumptions")?;
        ws.set_column_width(0, 58)?;
        ws.set_column_width(1, 16)?;
        ws.set_column_width(2, 52)?;
        ws.write_string_with_format(
            0,
            0,
            format!(
                "Nurse staffing model{}",
                r.program
                    .as_ref()
                    .map(|p| format!(": {p}"))
                    .unwrap_or_default()
            ),
            &bold,
        )?;
        ws.write_string(
            1,
            0,
            "Yellow cells are inputs; change them and every sheet recalculates.",
        )?;
        let rows: [(&str, f64, &str); 6] = [
            (
                "Paid hours per year",
                i.paid,
                &r.productive_hours.inputs[0].basis,
            ),
            (
                "Holiday hours",
                i.holiday,
                &r.productive_hours.inputs[1].basis,
            ),
            (
                "Vacation hours",
                i.vacation,
                &r.productive_hours.inputs[2].basis,
            ),
            (
                "Sick and personal hours",
                i.sick,
                &r.productive_hours.inputs[3].basis,
            ),
            (
                "Training hours",
                i.training,
                &r.productive_hours.inputs[4].basis,
            ),
            (
                "Share of remaining hours that is not direct service (%)",
                i.non_service,
                &r.productive_hours.non_service_basis,
            ),
        ];
        for (k, (label, v, basis)) in rows.iter().enumerate() {
            ws.write_string(2 + k as u32, 0, *label)?;
            ws.write_number_with_format(2 + k as u32, 1, *v, &input)?;
            ws.write_string(2 + k as u32, 2, *basis)?;
        }
        ws.write_string_with_format(8, 0, "Productive hours per FTE", &bold)?;
        ws.write_formula_with_format(
            8,
            1,
            Formula::new("=(B3-B4-B5-B6-B7)*(1-B8/100)")
                .set_result(format!("{}", r.productive_hours.productive_hours_per_fte)),
            &one,
        )?;
        ws.write_string(9, 0, "Round FTE up to this step")?;
        ws.write_number_with_format(9, 1, r.rounding_step_fte, &input)?;
        ws.write_string(10, 0, "Current nurse FTE (optional)")?;
        if let Some(f) = i.current_fte {
            ws.write_number_with_format(10, 1, f, &input)?;
        }
        ws.write_string(11, 0, "Population served (optional)")?;
        if let Some(p) = i.population {
            ws.write_number_with_format(11, 1, p, &input)?;
        }
        ws.write_string(12, 0, "Active clients or cases (optional)")?;
        if let Some(p) = i.clients {
            ws.write_number_with_format(12, 1, p, &input)?;
        }
        ws.write_string(
            14,
            0,
            "Values marked 'tool default' are placeholders with no published source. No staffing benchmark is bundled.",
        )?;
    }

    // ---- Services
    let n = r.services.len() as u32;
    let total_xr = 5 + n; // 1-based row of the total
    {
        let ws = wb.add_worksheet();
        ws.set_name("Services")?;
        ws.write_string_with_format(0, 0, "Hours of work by service (baseline)", &bold)?;
        for (c, h) in [
            "Service",
            "Units per year",
            "Minutes per unit",
            "Hours of work",
            "Share of hours",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(3, c as u16, *h, &head)?;
            ws.set_column_width(c as u16, if c == 0 { 44 } else { 16 })?;
        }
        for (k, s) in r.services.iter().enumerate() {
            let row = 4 + k as u32;
            let xr = row + 1;
            ws.write_string(row, 0, &s.service)?;
            ws.write_number_with_format(row, 1, s.annual_volume, &input)?;
            ws.write_number_with_format(row, 2, s.minutes_per_unit, &input)?;
            ws.write_formula_with_format(
                row,
                3,
                Formula::new(format!("=B{xr}*C{xr}/60")).set_result(num_s(s.hours)),
                &whole,
            )?;
            ws.write_formula_with_format(
                row,
                4,
                Formula::new(format!("=IF(D${total_xr}=0,\"\",D{xr}/D${total_xr})")).set_result(
                    s.percent_of_hours
                        .map_or(String::new(), |p| format!("{}", p / 100.0)),
                ),
                &pct,
            )?;
        }
        ws.write_string_with_format(4 + n, 0, "Total", &bold)?;
        ws.write_formula_with_format(
            4 + n,
            3,
            Formula::new(format!("=SUM(D5:D{})", 4 + n)).set_result(num_s(r.baseline_hours)),
            &whole,
        )?;
        ws.set_freeze_panes(4, 1)?;
    }

    // ---- Scenarios
    {
        let ws = wb.add_worksheet();
        ws.set_name("Scenarios")?;
        ws.write_string_with_format(0, 0, "Scenarios", &bold)?;
        ws.write_string(
            1,
            0,
            "Volume change applies to every baseline service. Added hours come from the services listed below the table.",
        )?;
        let heads = [
            "Scenario",
            "Volume change (%)",
            "Added hours",
            "Total hours of work",
            "Change vs. baseline",
            "FTE needed",
            "FTE rounded up",
            "More (+) or fewer (-) than current FTE",
            "Residents per FTE needed",
            "Clients per FTE needed",
        ];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(3, c as u16, *h, &head)?;
            ws.set_column_width(c as u16, if c == 0 { 40 } else { 16 })?;
        }
        let ns = r.scenarios.len() as u32;
        let added_head_row = 4 + ns + 2; // 0-based row of the added-services header
        let added_first_xr = added_head_row + 2; // 1-based first data row
        let total_added_rows: usize = i.added.iter().map(Vec::len).sum();
        let added_last_xr = added_first_xr + total_added_rows as u32 + 20 - 1;
        for (k, s) in r.scenarios.iter().enumerate() {
            let row = 4 + k as u32;
            let xr = row + 1;
            ws.write_string(row, 0, &s.name)?;
            ws.write_number_with_format(row, 1, s.volume_growth_percent, &input)?;
            ws.write_formula_with_format(
                row,
                2,
                Formula::new(format!(
                    "=SUMIF($A${added_first_xr}:$A${added_last_xr},A{xr},$E${added_first_xr}:$E${added_last_xr})"
                ))
                .set_result(num_s(s.added_hours)),
                &whole,
            )?;
            ws.write_formula_with_format(
                row,
                3,
                Formula::new(format!("=Services!$D${total_xr}*(1+B{xr}/100)+C{xr}"))
                    .set_result(num_s(s.total_hours)),
                &whole,
            )?;
            ws.write_formula_with_format(
                row,
                4,
                Formula::new(format!("=D{xr}-D$5"))
                    .set_result(num_s(s.change_in_hours_vs_baseline)),
                &whole,
            )?;
            ws.write_formula_with_format(
                row,
                5,
                Formula::new(format!("=D{xr}/Assumptions!$B$9")).set_result(num_s(s.fte_needed)),
                &two,
            )?;
            ws.write_formula_with_format(
                row,
                6,
                Formula::new(format!(
                    "=ROUNDUP(ROUND(F{xr}/Assumptions!$B$10,6),0)*Assumptions!$B$10"
                ))
                .set_result(num_s(s.fte_rounded_up)),
                &two,
            )?;
            ws.write_formula_with_format(
                row,
                7,
                Formula::new(format!(
                    "=IF(Assumptions!$B$11=\"\",\"\",G{xr}-Assumptions!$B$11)"
                ))
                .set_result(s.gap_vs_current_fte.map_or(String::new(), num_s)),
                &two,
            )?;
            ws.write_formula_with_format(
                row,
                8,
                Formula::new(format!(
                    "=IF(OR(Assumptions!$B$12=\"\",F{xr}=0),\"\",Assumptions!$B$12/F{xr})"
                ))
                .set_result(s.residents_per_fte_needed.map_or(String::new(), num_s)),
                &whole,
            )?;
            ws.write_formula_with_format(
                row,
                9,
                Formula::new(format!(
                    "=IF(OR(Assumptions!$B$13=\"\",F{xr}=0),\"\",Assumptions!$B$13/F{xr})"
                ))
                .set_result(s.clients_per_fte_needed.map_or(String::new(), num_s)),
                &whole,
            )?;
        }
        ws.write_string_with_format(
            added_head_row,
            0,
            "Added services (scenario name must match a scenario above)",
            &bold,
        )?;
        for (c, h) in [
            "Scenario",
            "Service",
            "Units per year",
            "Minutes per unit",
            "Hours of work",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(added_head_row + 1, c as u16, *h, &head)?;
        }
        let mut row = added_head_row + 2;
        for (k, adds) in i.added.iter().enumerate() {
            for (name, vol, min) in adds {
                let xr = row + 1;
                ws.write_string(row, 0, &r.scenarios[k].name)?;
                ws.write_string(row, 1, name)?;
                ws.write_number_with_format(row, 2, *vol, &input)?;
                ws.write_number_with_format(row, 3, *min, &input)?;
                ws.write_formula_with_format(
                    row,
                    4,
                    Formula::new(format!("=C{xr}*D{xr}/60")).set_result(num_s(vol * min / 60.0)),
                    &whole,
                )?;
                row += 1;
            }
        }
        ws.set_freeze_panes(4, 1)?;
    }
    wb.save_to_buffer()
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
            "staffing_model.xlsx",
            &bytes,
        )?);
    }
    if want_report {
        let md = report_markdown(i.result);
        let doc = templates::document(ctx, &md, "en-US", "Public health nurse staffing model");
        report = Some(templates::write_in(
            ctx,
            dir,
            &format!("staffing_model_summary.{}", format.ext()),
            &format.render(&doc)?,
        )?);
    }
    Ok((book, report))
}
