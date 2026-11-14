//! Workbook (live formulas) and DOCX plan for `assess_workforce_competencies`.
//!
//! The workbook has the ratings (one row per staff code and domain, from the nurse's file or a
//! blank form), team results that hide any result for fewer than the minimum group size, a
//! training plan sheet, the credential tracker with due-date and status formulas, and DPH's
//! standards. Individual rows are in the workbook only; staff are identified by code.

use chrono::{Datelike, NaiveDate};
use phn_core::{PhnError, Result};
use rust_xlsxwriter::{
    Color, DataValidation, DataValidationRule, ExcelDateTime, Format, Formula, Workbook, XlsxError,
};

use super::strategy_docs::DocBuilder;
use super::workforce::{CredRow, StaffDomain, WorkforceResult, priority};
use super::workforce_content::{CompetencyContent, Framework};
use crate::context::Context;

pub(crate) struct OutputInputs<'a> {
    pub content: &'a CompetencyContent,
    pub framework: &'a Framework,
    pub result: &'a WorkforceResult,
    pub staff_domain: Option<&'a [StaffDomain]>,
    pub cred_rows: Option<&'a [CredRow]>,
    pub as_of: NaiveDate,
}

type X<T> = std::result::Result<T, XlsxError>;

const TEMPLATE_STAFF: usize = 12;
const SPARE_ROWS: u32 = 40;
const FIRST: u32 = 5; // first data row (1-based) on the Ratings sheet

fn xl_date(d: NaiveDate) -> X<ExcelDateTime> {
    ExcelDateTime::from_ymd(d.year() as u16, d.month() as u8, d.day() as u8)
}

fn num_s(x: f64) -> String {
    format!("{}", (x * 100.0).round() / 100.0)
}

#[allow(clippy::too_many_lines)]
fn build_workbook(i: &OutputInputs) -> X<Vec<u8>> {
    let f = i.framework;
    let r = i.result;
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let head = Format::new().set_bold().set_text_wrap();
    let wrap = Format::new().set_text_wrap();
    let two = Format::new().set_num_format("0.00");
    let input = Format::new().set_background_color(Color::RGB(0xFFF2CC));
    let input2 = Format::new()
        .set_num_format("0.00")
        .set_background_color(Color::RGB(0xFFF2CC));
    let date_in = Format::new()
        .set_num_format("yyyy-mm-dd")
        .set_background_color(Color::RGB(0xFFF2CC));
    let date = Format::new().set_num_format("yyyy-mm-dd");

    // Rows of the ratings sheet.
    let mut rating_rows: Vec<(String, Option<u32>, String, Option<f64>)> = Vec::new();
    match i.staff_domain {
        Some(rows) => {
            let mut v: Vec<&StaffDomain> = rows.iter().collect();
            v.sort_by(|a, b| a.staff.cmp(&b.staff).then(a.domain.cmp(&b.domain)));
            for s in v {
                rating_rows.push((
                    s.staff.clone(),
                    s.tier,
                    f.domains[s.domain].name.clone(),
                    Some(s.rating),
                ));
            }
        }
        None => {
            for k in 1..=TEMPLATE_STAFF {
                for d in &f.domains {
                    rating_rows.push((format!("S{k:02}"), None, d.name.clone(), None));
                }
            }
        }
    }
    let n_rows = rating_rows.len() as u32 + SPARE_ROWS;
    let last = FIRST + n_rows - 1;
    let rng = |col: &str| format!("Ratings!${col}${FIRST}:${col}${last}");

    // ---- Instructions
    {
        let ws = wb.add_worksheet();
        ws.set_name("Instructions")?;
        ws.set_column_width(0, 110)?;
        ws.write_string_with_format(
            0,
            0,
            format!("Workforce self-assessment: {}", f.label),
            &bold,
        )?;
        let lines = [
            "Yellow cells are inputs. Staff are identified by a code (S01, S02, ...). Do not type names in this workbook's Ratings or Credentials sheets: keep a roster of codes and names in a separate, private file.".to_string(),
            format!("Rating scale: {}", r.rating_scale.iter().map(|l| format!("{} = {} ({})", l.value, l.label, l.meaning)).collect::<Vec<_>>().join("; ")),
            "Tier: 1 = front-line and program support (the generalist public health nurse in the nursing framework), 2 = program management and supervisory, 3 = senior management and executive. Enter one tier per staff member.".to_string(),
            format!("Domains: {}", r.domains.join("; ")),
            format!("Team results hide any result for fewer than {} people (a team of fewer than {} gets one combined total at most), so no one's rating can be read from them. The Ratings and Credentials sheets list individual codes: share only the Team results and Training plan sheets.", r.minimum_group_size, r.minimum_group_size),
            "The numbered competency statements of the framework were not available to this tool: rate each domain, or type your own competency labels in your copy of the framework and enter the domain's overall rating here.".to_string(),
            format!("The target rating is a tool default with no published source. Change it on the Assumptions sheet. Frameworks: {}.", i.content.frameworks.iter().map(|x| x.label.clone()).collect::<Vec<_>>().join("; ")),
            "Credentials: the Standards sheet lists DPH's local public health workforce standards by position (from the Performance Standards workbook, modified 2024-07-15). Enter each staff member's position, hire date, and the credentials completed; due dates and status calculate. Waivers and equivalents are not modeled.".to_string(),
        ];
        for (k, l) in lines.iter().enumerate() {
            ws.write_string_with_format(2 + k as u32, 0, l, &wrap)?;
        }
    }

    // ---- Assumptions
    {
        let ws = wb.add_worksheet();
        ws.set_name("Assumptions")?;
        ws.set_column_width(0, 58)?;
        ws.set_column_width(1, 16)?;
        ws.set_column_width(2, 60)?;
        ws.write_string_with_format(
            0,
            0,
            "Assumptions (change the yellow cells; everything recalculates)",
            &bold,
        )?;
        ws.write_string(1, 0, "Target rating (1 to 4)")?;
        ws.write_number_with_format(1, 1, r.target_rating, &input2)?;
        ws.write_string(
            1,
            2,
            "Tool default of 3 (Knowledgeable); no published source",
        )?;
        ws.write_string(2, 0, "Minimum number of people for any result")?;
        ws.write_number_with_format(2, 1, f64::from(r.minimum_group_size), &input)?;
        ws.write_string(
            2,
            2,
            "Project rule: no result for fewer than this many staff",
        )?;
        ws.write_string(3, 0, "Measure credential deadlines as of")?;
        ws.write_datetime_with_format(3, 1, xl_date(i.as_of)?, &date_in)?;
        ws.write_string(4, 0, "Days before a deadline to call it due soon")?;
        let soon = r.credentials.as_ref().map_or(90, |c| c.due_soon_days);
        ws.write_number_with_format(4, 1, f64::from(soon), &input)?;
    }

    // ---- Ratings
    {
        let ws = wb.add_worksheet();
        ws.set_name("Ratings")?;
        ws.write_string_with_format(0, 0, "Ratings: one row per staff code and domain", &bold)?;
        ws.write_string(1, 0, "Rating 1 to 4 (blank = not rated). Column E marks the first rated row of each staff code, so the team size counts people once.")?;
        for (c, h) in [
            "Staff code",
            "Tier (1-3)",
            "Domain",
            "Rating (1-4)",
            "Counts as staff",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(FIRST - 2, c as u16, *h, &head)?;
            ws.set_column_width(c as u16, if c == 2 { 46 } else { 16 })?;
        }
        for (k, (staff, tier, domain, rating)) in rating_rows.iter().enumerate() {
            let row = FIRST - 1 + k as u32;
            ws.write_string_with_format(row, 0, staff, &input)?;
            match tier {
                Some(t) => ws.write_number_with_format(row, 1, f64::from(*t), &input)?,
                None => ws.write_blank(row, 1, &input)?,
            };
            ws.write_string_with_format(row, 2, domain, &input)?;
            match rating {
                Some(x) => ws.write_number_with_format(row, 3, *x, &input2)?,
                None => ws.write_blank(row, 3, &input2)?,
            };
        }
        for k in rating_rows.len() as u32..n_rows {
            let row = FIRST - 1 + k;
            for c in 0..4u16 {
                ws.write_blank(row, c, &input)?;
            }
        }
        for k in 0..n_rows {
            let row = FIRST - 1 + k;
            let x = row + 1;
            let counted = rating_rows
                .get(k as usize)
                .and_then(|(s, _, _, r)| r.map(|_| (s.clone(), k)))
                .map(|(s, idx)| {
                    let first = rating_rows
                        .iter()
                        .position(|(s2, _, _, r2)| *s2 == s && r2.is_some());
                    first == Some(idx as usize)
                })
                .unwrap_or(false);
            ws.write_formula(
                row,
                4,
                Formula::new(format!(
                    "=IF(AND(D{x}>0,A{x}<>\"\",COUNTIFS($A${FIRST}:A{x},A{x},$D${FIRST}:D{x},\">0\")=1),1,0)"
                ))
                .set_result(if counted { "1" } else { "0" }),
            )?;
        }
        let dv_rating =
            DataValidation::new().allow_decimal_number(DataValidationRule::Between(1.0, 4.0));
        ws.add_data_validation(FIRST - 1, 3, last - 1, 3, &dv_rating)?;
        let dv_tier = DataValidation::new().allow_list_strings(&["1", "2", "3"])?;
        ws.add_data_validation(FIRST - 1, 1, last - 1, 1, &dv_tier)?;
        ws.set_freeze_panes(FIRST - 1, 1)?;
    }

    // ---- Team results
    {
        let ws = wb.add_worksheet();
        ws.set_name("Team results")?;
        ws.set_column_width(0, 46)?;
        for c in 1..8u16 {
            ws.set_column_width(c, 16)?;
        }
        ws.write_string_with_format(0, 0, format!("Team results: {}", f.label), &bold)?;
        ws.write_string(
            1,
            0,
            "Results are hidden when fewer than the minimum number of people (Assumptions) rated.",
        )?;
        for (c, h) in [
            "Domain",
            "Staff who rated",
            "Mean rating",
            "Gap to target",
            "Below target",
            "Priority",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(3, c as u16, *h, &head)?;
        }
        let team = r.team.as_ref();
        let size_row = 14; // 1-based row of the team size
        for (k, d) in f.domains.iter().enumerate() {
            let row = 4 + k as u32;
            let x = row + 1;
            let dr = team.and_then(|t| t.domains.get(k));
            ws.write_string(row, 0, &d.name)?;
            let raw_n = format!("COUNTIFS({}, A{x}, {}, \">0\")", rng("C"), rng("D"));
            ws.write_formula(
                row,
                1,
                Formula::new(format!(
                    "=IF(OR($B${size_row}<Assumptions!$B$3,{raw_n}<Assumptions!$B$3),\"suppressed\",{raw_n})"
                ))
                .set_result(dr.and_then(|d| d.raters).map_or("suppressed".to_string(), |n| n.to_string())),
            )?;
            ws.write_formula_with_format(
                row,
                2,
                Formula::new(format!(
                    "=IF(ISNUMBER(B{x}),AVERAGEIFS({}, {}, A{x}, {}, \">0\"),\"suppressed\")",
                    rng("D"),
                    rng("C"),
                    rng("D")
                ))
                .set_result(
                    dr.and_then(|d| d.mean_rating)
                        .map_or("suppressed".to_string(), num_s),
                ),
                &two,
            )?;
            ws.write_formula_with_format(
                row,
                3,
                Formula::new(format!("=IF(ISNUMBER(C{x}),Assumptions!$B$2-C{x},\"\")")).set_result(
                    dr.and_then(|d| d.gap_to_target)
                        .map_or(String::new(), num_s),
                ),
                &two,
            )?;
            let below = format!(
                "COUNTIFS({}, A{x}, {}, \"<\"&Assumptions!$B$2, {}, \">0\")",
                rng("C"),
                rng("D"),
                rng("D")
            );
            ws.write_formula(
                row,
                4,
                Formula::new(format!(
                    "=IF(ISNUMBER(C{x}),IF(OR({below}=0,{below}>=Assumptions!$B$3),{below},\"fewer than \"&Assumptions!$B$3),\"\")"
                ))
                .set_result(dr.and_then(|d| d.staff_below_target).map_or(String::new(), |n| n.to_string())),
            )?;
            ws.write_formula(
                row,
                5,
                Formula::new(format!(
                    "=IF(ISNUMBER(D{x}),IF(D{x}>=1,\"High\",IF(D{x}>=0.5,\"Medium\",IF(D{x}>0,\"Low\",\"Maintain\"))),\"\")"
                ))
                .set_result(dr.and_then(|d| d.gap_to_target).map_or(String::new(), |g| priority(g).to_string())),
            )?;
        }
        ws.write_string_with_format(
            size_row - 1,
            0,
            "Team size (staff with at least one rating)",
            &bold,
        )?;
        let team_n = i
            .staff_domain
            .map(|rows| {
                rows.iter()
                    .map(|r| r.staff.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
            })
            .unwrap_or(0);
        ws.write_formula(
            size_row - 1,
            1,
            Formula::new(format!("=SUM({})", rng("E"))).set_result(team_n.to_string()),
        )?;
        ws.write_string_with_format(size_row, 0, "Overall mean rating", &bold)?;
        ws.write_formula_with_format(
            size_row,
            1,
            Formula::new(format!(
                "=IF($B${size_row}<2,\"suppressed\",IF($B${size_row}<Assumptions!$B$3,AVERAGEIFS({}, {}, \">0\"),IFERROR(AVERAGE(C5:C12),\"suppressed\")))",
                rng("D"),
                rng("D")
            ))
            .set_result(
                team.and_then(|t| t.overall_mean_rating)
                    .map_or("suppressed".to_string(), num_s),
            ),
            &two,
        )?;
        ws.write_string(
            size_row + 1,
            0,
            "Overall: a team of two gets one combined total; with enough staff it is the mean of the domain means shown.",
        )?;
        // Tiers.
        ws.write_string_with_format(size_row + 3, 0, "By tier", &bold)?;
        for (c, h) in ["Tier", "Staff", "Mean rating"].iter().enumerate() {
            ws.write_string_with_format(size_row + 4, c as u16, *h, &head)?;
        }
        for t in 1..=3u32 {
            let row = size_row + 4 + t;
            let x = row + 1;
            let tr = team.and_then(|tm| tm.tiers.iter().find(|x| x.tier == t));
            ws.write_number(row, 0, f64::from(t))?;
            ws.write_formula(
                row,
                1,
                Formula::new(format!(
                    "=IF(OR($B${size_row}<Assumptions!$B$3,SUMIFS({e}, {b}, A{x})<Assumptions!$B$3),\"not shown\",SUMIFS({e}, {b}, A{x}))",
                    e = rng("E"),
                    b = rng("B")
                ))
                .set_result(tr.and_then(|x| x.staff).map_or("not shown".to_string(), |n| n.to_string())),
            )?;
            ws.write_formula_with_format(
                row,
                2,
                Formula::new(format!(
                    "=IF(ISNUMBER(B{x}),AVERAGEIFS({}, {}, A{x}, {}, \">0\"),\"not shown\")",
                    rng("D"),
                    rng("B"),
                    rng("D")
                ))
                .set_result(
                    tr.and_then(|x| x.mean_rating)
                        .map_or("not shown".to_string(), num_s),
                ),
                &two,
            )?;
        }
    }

    // ---- Training plan
    {
        let ws = wb.add_worksheet();
        ws.set_name("Training plan")?;
        ws.set_column_width(0, 46)?;
        ws.set_column_width(1, 12)?;
        ws.set_column_width(2, 12)?;
        ws.set_column_width(3, 70)?;
        ws.set_column_width(4, 40)?;
        ws.set_column_width(5, 20)?;
        ws.set_column_width(6, 14)?;
        ws.write_string_with_format(0, 0, "Team training plan (no individual results)", &bold)?;
        ws.write_string(1, 0, "Domains where the team is below the target are listed with generic steps. Replace them with the training you choose; no course or vendor is recommended by this tool.")?;
        for (c, h) in [
            "Domain",
            "Priority",
            "Gap to target",
            "Generic steps to consider",
            "Your action [NURSE]",
            "Owner",
            "Due date",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(3, c as u16, *h, &head)?;
        }
        let steps = i
            .content
            .training_actions
            .iter()
            .map(|a| a.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        for (k, d) in f.domains.iter().enumerate() {
            let row = 4 + k as u32;
            let x = 5 + k as u32;
            ws.write_formula(
                row,
                0,
                Formula::new(format!("='Team results'!A{x}")).set_result(d.name.clone()),
            )?;
            ws.write_formula(row, 1, Formula::new(format!("='Team results'!F{x}")))?;
            ws.write_formula_with_format(
                row,
                2,
                Formula::new(format!("='Team results'!D{x}")),
                &two,
            )?;
            ws.write_formula_with_format(
                row,
                3,
                Formula::new(format!("=IF(OR('Team results'!F{x}=\"High\",'Team results'!F{x}=\"Medium\",'Team results'!F{x}=\"Low\"),\"{}\",\"\")", steps.replace('"', "\"\"")))
                    .set_result(String::new()),
                &wrap,
            )?;
            ws.write_blank(row, 4, &input)?;
            ws.write_blank(row, 5, &input)?;
            ws.write_blank(row, 6, &input)?;
        }
    }

    // ---- Credentials
    {
        let ws = wb.add_worksheet();
        ws.set_name("Credentials")?;
        ws.write_string_with_format(
            0,
            0,
            "Credential tracker: DPH local public health workforce standards",
            &bold,
        )?;
        ws.write_string(1, 0, "One row per staff code and requirement. Enter the completed date; due dates and status calculate from the hire date and DPH's \"within N months of hire\" wording (Assumptions has the as-of date).")?;
        for (c, h) in [
            "Staff code",
            "Position",
            "Hire date",
            "Requirement",
            "Kind",
            "Months from hire",
            "Completed",
            "Due date",
            "Status",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(3, c as u16, *h, &head)?;
            ws.set_column_width(
                c as u16,
                if c == 3 {
                    52
                } else if c == 1 {
                    30
                } else {
                    16
                },
            )?;
        }
        let rows: &[CredRow] = i.cred_rows.unwrap_or(&[]);
        let total = rows.len() as u32 + if rows.is_empty() { 60 } else { 20 };
        for k in 0..total {
            let row = 4 + k;
            let x = row + 1;
            let cr = rows.get(k as usize);
            match cr {
                Some(c) => {
                    ws.write_string_with_format(row, 0, &c.staff, &input)?;
                    ws.write_string_with_format(row, 1, &c.position, &input)?;
                    ws.write_datetime_with_format(row, 2, xl_date(c.hire)?, &date_in)?;
                    ws.write_string_with_format(row, 3, &c.requirement, &input)?;
                    ws.write_string_with_format(row, 4, &c.kind, &input)?;
                    match c.months {
                        Some(m) => ws.write_number_with_format(row, 5, f64::from(m), &input)?,
                        None => ws.write_blank(row, 5, &input)?,
                    };
                    match c.completed {
                        Some(d) => ws.write_datetime_with_format(row, 6, xl_date(d)?, &date_in)?,
                        None => ws.write_blank(row, 6, &date_in)?,
                    };
                }
                None => {
                    for c in 0..7u16 {
                        ws.write_blank(row, c, if c == 2 || c == 6 { &date_in } else { &input })?;
                    }
                }
            }
            ws.write_formula_with_format(
                row,
                7,
                Formula::new(format!(
                    "=IF(OR(C{x}=\"\",F{x}=\"\"),\"\",EDATE(C{x},F{x}))"
                ))
                .set_result(
                    cr.and_then(|c| {
                        c.months
                            .and_then(|m| c.hire.checked_add_months(chrono::Months::new(m)))
                    })
                    .map_or(String::new(), |d| {
                        // Excel serial days since 1899-12-30.
                        let base = NaiveDate::from_ymd_opt(1899, 12, 30).expect("base");
                        (d - base).num_days().to_string()
                    }),
                ),
                &date,
            )?;
            ws.write_formula(
                row,
                8,
                Formula::new(format!(
                    "=IF(A{x}=\"\",\"\",IF(G{x}<>\"\",IF(AND(H{x}<>\"\",G{x}>H{x}),\"complete (late)\",\"complete\"),IF(H{x}=\"\",IF(E{x}=\"required at hire\",\"confirm at hire\",\"no deadline\"),IF(Assumptions!$B$4>H{x},\"overdue\",IF(H{x}-Assumptions!$B$4<=Assumptions!$B$5,\"due soon\",\"upcoming\")))))"
                ))
                .set_result(cr.map_or(String::new(), |c| c.status.clone())),
            )?;
        }
        ws.set_freeze_panes(4, 1)?;
    }

    // ---- Standards (public content)
    {
        let ws = wb.add_worksheet();
        ws.set_name("Standards")?;
        ws.set_column_width(0, 50)?;
        ws.set_column_width(1, 52)?;
        ws.set_column_width(2, 20)?;
        ws.set_column_width(3, 14)?;
        ws.set_column_width(4, 70)?;
        ws.write_string_with_format(0, 0, "DPH local public health workforce standards by position (Performance Standards workbook, modified 2024-07-15)", &bold)?;
        for (c, h) in [
            "Position",
            "Requirement",
            "Kind",
            "Months from hire",
            "DPH wording",
        ]
        .iter()
        .enumerate()
        {
            ws.write_string_with_format(2, c as u16, *h, &head)?;
        }
        let mut row = 3;
        for p in &r.position_standards {
            for rq in &p.requirements {
                ws.write_string(row, 0, &p.position)?;
                ws.write_string(row, 1, &rq.label)?;
                ws.write_string(row, 2, &rq.kind)?;
                if let Some(m) = rq.deadline_months_from_hire {
                    ws.write_number(row, 3, f64::from(m))?;
                }
                ws.write_string_with_format(row, 4, &rq.dph_wording, &wrap)?;
                row += 1;
            }
        }
    }
    wb.save_to_buffer()
}

fn build_plan(ctx: &Context, i: &OutputInputs) -> phn_docs::model::Document {
    let r = i.result;
    let c = i.content;
    let mut b = DocBuilder::new();
    b.para("Self-assessment of staff competencies and the team training plan. Staff are identified by code in the workbook; no names appear here, and no result is shown for fewer than the minimum number of people.");
    b.labeled("Framework:", r.framework.clone());
    b.labeled(
        "Target rating:",
        format!(
            "{} (a tool default with no published source; set your own)",
            num_s(r.target_rating)
        ),
    );
    if let Some(t) = &r.team {
        b.heading(2, "Team results");
        b.labeled("Team size:", t.team_size.clone());
        b.para(match t.overall_mean_rating {
            Some(m) => format!(
                "Overall: {} on the 1 to 4 scale. {}",
                num_s(m),
                t.overall_basis
            ),
            None => t.overall_basis.clone(),
        });
        let rows: Vec<Vec<String>> = t
            .domains
            .iter()
            .map(|d| {
                vec![
                    d.domain.clone(),
                    d.raters.map_or("-".into(), |n| n.to_string()),
                    d.mean_rating.map_or("-".into(), num_s),
                    d.gap_to_target.map_or("-".into(), num_s),
                    d.staff_below_target.map_or("-".into(), |n| n.to_string()),
                    d.note.clone(),
                ]
            })
            .collect();
        b.table(
            "Domain results",
            &[
                "Domain",
                "Staff who rated",
                "Mean",
                "Gap to target",
                "Below target",
                "Note",
            ],
            rows,
        );
        b.list(&t.suppression_notes);
        b.heading(2, "Team training plan");
        let rows: Vec<Vec<String>> = r
            .training_plan
            .iter()
            .map(|p| {
                vec![
                    p.rank.to_string(),
                    p.domain.clone(),
                    p.priority.clone(),
                    num_s(p.gap_to_target),
                    "[NURSE: action]".into(),
                    "[NURSE: owner]".into(),
                    "[NURSE: due date]".into(),
                ]
            })
            .collect();
        b.table(
            "Training plan",
            &["#", "Domain", "Priority", "Gap", "Action", "Owner", "Due"],
            rows,
        );
        b.para("Generic steps to consider for each domain below target (no course, vendor, or curriculum is recommended or verified):");
        let steps: Vec<String> = c.training_actions.iter().map(|a| a.text.clone()).collect();
        b.list(&steps);
    } else {
        b.heading(2, "How to use the workbook");
        b.para("Each staff member rates themselves in each domain on the scale below; staff are identified by a code, not a name. Give the filled workbook back to the assistant as ratings_path to get the team results and training plan.");
        let rows: Vec<Vec<String>> = r
            .rating_scale
            .iter()
            .map(|l| vec![l.value.to_string(), l.label.clone(), l.meaning.clone()])
            .collect();
        b.table("Rating scale", &["Value", "Level", "Meaning"], rows);
        b.heading(3, "Domains");
        b.list(&r.domains);
        b.heading(3, "Tiers");
        b.list(&r.tiers);
    }
    if let Some(k) = &r.credentials {
        b.heading(2, "Massachusetts workforce standards (DPH)");
        b.para(format!("Status as of {} ({} days counts as due soon). Counts below the minimum group size are hidden.", k.as_of, k.due_soon_days));
        let f = |n: Option<u32>| n.map_or("hidden".to_string(), |v| v.to_string());
        let rows: Vec<Vec<String>> = k
            .summaries
            .iter()
            .map(|x| {
                vec![
                    x.position.clone(),
                    x.requirement.clone(),
                    x.kind.clone(),
                    f(x.complete),
                    f(x.needs_attention),
                    f(x.upcoming),
                ]
            })
            .collect();
        b.table(
            "Credential status",
            &[
                "Position",
                "Credential",
                "Kind",
                "Complete",
                "Overdue or due soon",
                "Upcoming",
            ],
            rows,
        );
        b.para(format!(
            "Staff whose position did not match: {}. Staff with no hire date: {}. Credential entries not matched to a requirement: {}.",
            k.staff_position_not_matched, k.staff_hire_date_missing, k.entries_not_matched
        ));
    }
    b.heading(2, "Method and limits");
    b.list(&[
        "Ratings use the four-level scale described in a published study that used a Core Competencies assessment; the meanings are short paraphrases. The competency statements of both frameworks were not available to this tool, so ratings are by domain and tier.".to_string(),
        "Each staff member's rating per domain is the mean of their ratings in it; the team mean is the mean of staff. The overall figure is the mean of the domain means shown.".to_string(),
        "Credential deadlines are read from DPH's wording and added to the hire date; waivers, prior service, and equivalents are not modeled.".to_string(),
        "Decision support for workforce planning, not a finding of compliance and not a personnel evaluation. Content is a draft pending review by the subject matter expert.".to_string(),
    ]);
    b.para("Prepared by: [NURSE: name and title]    Date: [NURSE: date]");
    b.finish(
        ctx,
        "Workforce competency assessment and training plan",
        None,
    )
}

pub(crate) fn write_files(ctx: &Context, i: &OutputInputs) -> Result<(String, String)> {
    let bytes = build_workbook(i).map_err(|e| PhnError::Internal(format!("XLSX: {e}")))?;
    let (dir, _) = crate::school::templates::output_folder(ctx, "workforce")?;
    let book = crate::school::templates::write_in(ctx, &dir, "workforce_assessment.xlsx", &bytes)?;
    let doc = build_plan(ctx, i);
    let plan = crate::school::templates::write_in(
        ctx,
        &dir,
        "workforce_training_plan.docx",
        &phn_docs::docx::to_docx(&doc)?,
    )?;
    Ok((book, plan))
}
