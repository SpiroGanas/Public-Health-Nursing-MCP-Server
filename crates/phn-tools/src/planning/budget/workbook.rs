//! The budget workbook (XLSX) for `build_program_budget`.
//!
//! Sheets: Read me, Inputs (rates, with drop-downs), Line items (one row per cost, with live
//! formulas for each year's amount, fringe, and MTDC base; spare rows are ready to use), and
//! Budget summary (SUMIFS by object class for the federal request, the cost share, and the project
//! total, the indirect costs on the chosen base, the match check, and an arithmetic check). Every
//! formula carries the value this program computed as its cached result, so viewers that do not
//! recalculate still show numbers; Excel and LibreOffice recalculate when the file opens.

use phn_core::{PhnError, Result};
use rust_xlsxwriter::{
    DataValidation, DocProperties, Format, FormatAlign, FormatBorder, Formula, Workbook, Worksheet,
    XlsxError,
};

use super::calc::{Block, Category, Computed, Funding, IndirectBase, Line, MatchBasis, Rates};

fn xe(e: XlsxError) -> PhnError {
    PhnError::Internal(format!("XLSX: {e}"))
}

/// Spare formula rows after the last line, ready for the nurse to fill.
pub const SPARE_ROWS: usize = 12;

/// First data row (1-based) of the Line items sheet.
pub const FIRST_ROW: usize = 6;

/// Inputs sheet cell rows (1-based, column B).
pub mod inputs {
    pub const YEARS: u32 = 3;
    pub const SALARY_ESC: u32 = 4;
    pub const OTHER_ESC: u32 = 5;
    pub const FRINGE: u32 = 6;
    pub const METHOD: u32 = 7;
    pub const RATE: u32 = 8;
    pub const BASE: u32 = 9;
    pub const CS_INDIRECT: u32 = 10;
    pub const SUB_CAP: u32 = 11;
    pub const MATCH_PCT: u32 = 12;
    pub const MATCH_BASIS: u32 = 13;
    pub const BASE_INDEX: u32 = 14;
}

/// Column letters (A, B, ..., Z, AA, ...).
pub fn col_name(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

/// What goes into the workbook.
pub struct WorkbookInput<'a> {
    pub title: &'a str,
    pub lines: &'a [Line],
    pub rates: &'a Rates,
    /// "De minimis (2 CFR 200.414(f))", "Negotiated", or "None".
    pub method_label: &'a str,
    pub required_match: Option<(f64, MatchBasis)>,
    pub computed: &'a Computed,
    pub read_me: &'a [String],
}

fn money(v: f64) -> String {
    format!("{v}")
}

/// Column positions of the Line items sheet.
struct Layout {
    amount0: usize,
    fringe0: usize,
    mtdc0: usize,
    total: usize,
    first: usize,
    last: usize,
}

impl Layout {
    fn new(n: usize, rows: usize) -> Self {
        Layout {
            amount0: 12,
            fringe0: 12 + n,
            mtdc0: 12 + 2 * n,
            total: 12 + 3 * n,
            first: FIRST_ROW,
            last: FIRST_ROW + rows - 1,
        }
    }

    fn range(&self, col: usize) -> String {
        let c = col_name(col);
        format!("'Line items'!{c}${}:{c}${}", self.first, self.last)
    }
}

/// The amount formula of a line row (public for tests of the formula text).
pub fn amount_formula(r: usize, year_cell: &str) -> String {
    format!(
        "=IF(OR($B{r}=\"\",{y}<$J{r},{y}>$K{r}),0,ROUND($E{r}*$F{r}*IF($I{r}=\"Yes\",(1+IF($B{r}=\"Personnel\",Inputs!$B${se},Inputs!$B${oe}))^({y}-1),1),2))",
        y = year_cell,
        se = inputs::SALARY_ESC,
        oe = inputs::OTHER_ESC
    )
}

/// Build the workbook.
pub fn build(inp: &WorkbookInput) -> Result<Vec<u8>> {
    let rates = inp.rates;
    let n = rates.years;
    let rows = inp.lines.len() + SPARE_ROWS;
    let lay = Layout::new(n, rows);

    let mut wb = Workbook::new();
    wb.set_properties(
        &DocProperties::new()
            .set_title(inp.title)
            .set_subject("Draft program budget"),
    );
    let title_fmt = Format::new().set_bold().set_font_size(14);
    let head = Format::new()
        .set_bold()
        .set_text_wrap()
        .set_align(FormatAlign::Top)
        .set_border_bottom(FormatBorder::Thin)
        .set_background_color(0xDDE8F0);
    let bold = Format::new().set_bold();
    let wrap = Format::new().set_text_wrap().set_align(FormatAlign::Top);
    let input = Format::new().set_background_color(0xFFF7D6);
    let input_money = Format::new()
        .set_background_color(0xFFF7D6)
        .set_num_format("#,##0.00");
    let input_pct = Format::new()
        .set_background_color(0xFFF7D6)
        .set_num_format("0.0%");
    let money_fmt = Format::new().set_num_format("#,##0.00");
    let money_bold = Format::new()
        .set_num_format("#,##0.00")
        .set_bold()
        .set_border_top(FormatBorder::Thin);
    let bold_top = Format::new().set_bold().set_border_top(FormatBorder::Thin);
    let note_fmt = Format::new().set_italic().set_text_wrap();

    // ---- Read me
    {
        let ws = wb.add_worksheet();
        ws.set_name("Read me").map_err(xe)?;
        ws.write_string_with_format(0, 0, inp.title, &title_fmt)
            .map_err(xe)?;
        for (i, t) in inp.read_me.iter().enumerate() {
            ws.write_string_with_format((i + 2) as u32, 0, t, &wrap)
                .map_err(xe)?;
        }
        ws.set_column_width(0, 110).map_err(xe)?;
    }

    // ---- Inputs
    {
        let ws = wb.add_worksheet();
        ws.set_name("Inputs").map_err(xe)?;
        ws.write_string_with_format(
            0,
            0,
            "Inputs (yellow cells; the other sheets recalculate)",
            &title_fmt,
        )
        .map_err(xe)?;
        let put_label = |ws: &mut Worksheet, row: u32, label: &str, note: &str| -> Result<()> {
            ws.write_string(row - 1, 0, label).map_err(xe)?;
            ws.write_string_with_format(row - 1, 2, note, &note_fmt)
                .map_err(xe)?;
            Ok(())
        };
        put_label(
            ws,
            inputs::YEARS,
            "Budget years",
            "How many year columns the line items use (1 to 5). Set when the workbook was built; to add a year, rebuild with a higher number.",
        )?;
        ws.write_number(inputs::YEARS - 1, 1, n as f64)
            .map_err(xe)?;
        put_label(
            ws,
            inputs::SALARY_ESC,
            "Salary escalation per year",
            "Applied to personnel lines marked Escalate = Yes: year k = year 1 x (1 + rate)^(k-1).",
        )?;
        ws.write_number_with_format(
            inputs::SALARY_ESC - 1,
            1,
            rates.salary_escalation,
            &input_pct,
        )
        .map_err(xe)?;
        put_label(
            ws,
            inputs::OTHER_ESC,
            "Other escalation per year",
            "Applied to non-personnel lines marked Escalate = Yes.",
        )?;
        ws.write_number_with_format(inputs::OTHER_ESC - 1, 1, rates.other_escalation, &input_pct)
            .map_err(xe)?;
        put_label(
            ws,
            inputs::FRINGE,
            "Default fringe rate",
            "Applied to personnel lines with no rate of their own. Check it against your organization's written rate.",
        )?;
        ws.write_number_with_format(inputs::FRINGE - 1, 1, rates.fringe, &input_pct)
            .map_err(xe)?;
        put_label(
            ws,
            inputs::METHOD,
            "Indirect cost method",
            "For your records. The rate below is what the formulas use; the de minimis rate is at most 15 percent of MTDC (2 CFR 200.414(f)).",
        )?;
        ws.write_string_with_format(inputs::METHOD - 1, 1, inp.method_label, &input)
            .map_err(xe)?;
        put_label(
            ws,
            inputs::RATE,
            "Indirect rate",
            "Enter 0 for no indirect costs.",
        )?;
        ws.write_number_with_format(inputs::RATE - 1, 1, rates.indirect_rate, &input_pct)
            .map_err(xe)?;
        put_label(
            ws,
            inputs::BASE,
            "Indirect base",
            "Choose the base your rate agreement or the funder specifies. The de minimis rate applies to MTDC.",
        )?;
        ws.write_string_with_format(inputs::BASE - 1, 1, rates.indirect_base.label(), &input)
            .map_err(xe)?;
        let labels: Vec<&str> = IndirectBase::ALL.iter().map(|b| b.label()).collect();
        for (i, l) in labels.iter().enumerate() {
            ws.write_string(2 + i as u32, 4, *l).map_err(xe)?;
        }
        ws.write_string_with_format(1, 4, "Indirect base choices", &bold)
            .map_err(xe)?;
        ws.add_data_validation(
            inputs::BASE - 1,
            1,
            inputs::BASE - 1,
            1,
            &DataValidation::new()
                .allow_list_strings(&labels)
                .map_err(xe)?,
        )
        .map_err(xe)?;
        put_label(
            ws,
            inputs::CS_INDIRECT,
            "Charge indirect on cost share lines",
            "Yes only if the funder allows it (unrecovered indirect counts as cost share only with prior approval, 2 CFR 200.306(c)).",
        )?;
        ws.write_string_with_format(
            inputs::CS_INDIRECT - 1,
            1,
            if rates.indirect_on_cost_share {
                "Yes"
            } else {
                "No"
            },
            &input,
        )
        .map_err(xe)?;
        ws.add_data_validation(
            inputs::CS_INDIRECT - 1,
            1,
            inputs::CS_INDIRECT - 1,
            1,
            &DataValidation::new()
                .allow_list_strings(&["Yes", "No"])
                .map_err(xe)?,
        )
        .map_err(xe)?;
        put_label(
            ws,
            inputs::SUB_CAP,
            "Subaward amount counted in MTDC",
            "First dollars of each subaward over the whole award (2 CFR 200.1).",
        )?;
        ws.write_number_with_format(inputs::SUB_CAP - 1, 1, rates.subaward_cap, &input_money)
            .map_err(xe)?;
        put_label(
            ws,
            inputs::MATCH_PCT,
            "Required cost share (percent)",
            "Leave blank when there is none.",
        )?;
        if let Some((p, _)) = inp.required_match {
            ws.write_number_with_format(inputs::MATCH_PCT - 1, 1, p / 100.0, &input_pct)
                .map_err(xe)?;
        } else {
            ws.write_blank(inputs::MATCH_PCT - 1, 1, &input_pct)
                .map_err(xe)?;
        }
        put_label(
            ws,
            inputs::MATCH_BASIS,
            "Cost share is a percent of",
            "Total project cost = federal request plus cost share; Federal request = a match ratio.",
        )?;
        let basis = match inp.required_match.map(|m| m.1) {
            Some(MatchBasis::FederalRequest) => "Federal request",
            _ => "Total project cost",
        };
        ws.write_string_with_format(inputs::MATCH_BASIS - 1, 1, basis, &input)
            .map_err(xe)?;
        ws.add_data_validation(
            inputs::MATCH_BASIS - 1,
            1,
            inputs::MATCH_BASIS - 1,
            1,
            &DataValidation::new()
                .allow_list_strings(&["Total project cost", "Federal request"])
                .map_err(xe)?,
        )
        .map_err(xe)?;
        ws.write_string(
            inputs::BASE_INDEX - 1,
            0,
            "Indirect base number (calculated)",
        )
        .map_err(xe)?;
        ws.write_formula(
            inputs::BASE_INDEX - 1,
            1,
            Formula::new(format!("=MATCH(B{},E3:E6,0)", inputs::BASE))
                .set_result(rates.indirect_base.index().to_string()),
        )
        .map_err(xe)?;
        ws.set_column_width(0, 38).map_err(xe)?;
        ws.set_column_width(1, 36).map_err(xe)?;
        ws.set_column_width(2, 70).map_err(xe)?;
        ws.set_column_width(4, 36).map_err(xe)?;
    }

    // ---- Line items
    {
        let ws = wb.add_worksheet();
        ws.set_name("Line items").map_err(xe)?;
        ws.write_string_with_format(0, 0, "Line items", &title_fmt)
            .map_err(xe)?;
        ws.write_string_with_format(
            1,
            0,
            "One row per cost. Use roles and descriptions, never names. Personnel: Quantity = percent of time (50% = 0.5), Unit cost = annual base salary. Other lines: Quantity = units per year, Unit cost = cost per unit. Spare rows at the bottom are ready to use.",
            &note_fmt,
        )
        .map_err(xe)?;
        let heads = [
            "ID",
            "Category",
            "Item or role",
            "Funding",
            "Quantity or effort",
            "Unit cost or annual salary",
            "Unit",
            "Fringe rate (blank = default)",
            "Escalate",
            "First year",
            "Last year",
            "MTDC treatment",
        ];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(4, c as u16, *h, &head)
                .map_err(xe)?;
        }
        for k in 0..n {
            for (block0, what) in [
                (lay.amount0, "amount"),
                (lay.fringe0, "fringe"),
                (lay.mtdc0, "MTDC base"),
            ] {
                let c = (block0 + k) as u16;
                ws.write_number(3, c, (k + 1) as f64).map_err(xe)?;
                ws.write_string_with_format(4, c, format!("Year {} {what}", k + 1), &head)
                    .map_err(xe)?;
            }
        }
        ws.write_string(3, 11, "Year number").map_err(xe)?;
        ws.write_string_with_format(4, lay.total as u16, "Total (amount + fringe)", &head)
            .map_err(xe)?;

        for i in 0..rows {
            let r = FIRST_ROW + i; // Excel row
            let row0 = (r - 1) as u32;
            let line = inp.lines.get(i);
            ws.write_number(row0, 0, (i + 1) as f64).map_err(xe)?;
            if let Some(l) = line {
                ws.write_string_with_format(row0, 1, l.category.label(), &input)
                    .map_err(xe)?;
                ws.write_string_with_format(row0, 2, &l.item, &input)
                    .map_err(xe)?;
                ws.write_string_with_format(row0, 3, l.funding.label(), &input)
                    .map_err(xe)?;
                if l.category == Category::Personnel {
                    ws.write_number_with_format(row0, 4, l.quantity, &input_pct)
                        .map_err(xe)?;
                } else {
                    ws.write_number_with_format(row0, 4, l.quantity, &input)
                        .map_err(xe)?;
                }
                ws.write_number_with_format(row0, 5, l.unit_cost, &input_money)
                    .map_err(xe)?;
                ws.write_string_with_format(row0, 6, &l.unit, &input)
                    .map_err(xe)?;
                match l.fringe {
                    Some(f) => ws
                        .write_number_with_format(row0, 7, f, &input_pct)
                        .map_err(xe)?,
                    None => ws.write_blank(row0, 7, &input_pct).map_err(xe)?,
                };
                ws.write_string_with_format(row0, 8, if l.escalate { "Yes" } else { "No" }, &input)
                    .map_err(xe)?;
                ws.write_number_with_format(row0, 9, l.first_year as f64, &input)
                    .map_err(xe)?;
                ws.write_number_with_format(row0, 10, l.last_year as f64, &input)
                    .map_err(xe)?;
                ws.write_string_with_format(row0, 11, l.treatment.label(), &input)
                    .map_err(xe)?;
            } else {
                for c in 1..=11u16 {
                    let f = if c == 7 { &input_pct } else { &input };
                    ws.write_blank(row0, c, f).map_err(xe)?;
                }
            }
            let calc = line.map(|_| &inp.computed.rows[i]);
            for k in 0..n {
                let a = col_name(lay.amount0 + k);
                let f = col_name(lay.fringe0 + k);
                let (va, vf, vm) = calc
                    .map(|c| (c.amount[k], c.fringe[k], c.mtdc[k]))
                    .unwrap_or((0.0, 0.0, 0.0));
                ws.write_formula_with_format(
                    row0,
                    (lay.amount0 + k) as u16,
                    Formula::new(amount_formula(r, &format!("{a}$4"))).set_result(money(va)),
                    &money_fmt,
                )
                .map_err(xe)?;
                ws.write_formula_with_format(
                    row0,
                    (lay.fringe0 + k) as u16,
                    Formula::new(format!(
                        "=IF($B{r}=\"Personnel\",ROUND({a}{r}*IF($H{r}=\"\",Inputs!$B${},$H{r}),2),0)",
                        inputs::FRINGE
                    ))
                    .set_result(money(vf)),
                    &money_fmt,
                )
                .map_err(xe)?;
                let first_amt = col_name(lay.amount0);
                ws.write_formula_with_format(
                    row0,
                    (lay.mtdc0 + k) as u16,
                    Formula::new(format!(
                        "=IF($L{r}=\"Excluded\",0,IF($L{r}=\"Subaward\",MIN(SUM(${first_amt}{r}:{a}{r}),Inputs!$B${cap})-MIN(SUM(${first_amt}{r}:{a}{r})-{a}{r},Inputs!$B${cap}),{a}{r}+{f}{r}))",
                        cap = inputs::SUB_CAP
                    ))
                    .set_result(money(vm)),
                    &money_fmt,
                )
                .map_err(xe)?;
            }
            let first_amt = col_name(lay.amount0);
            let last_fr = col_name(lay.fringe0 + n - 1);
            let row_total = calc
                .map(|c| c.amount.iter().sum::<f64>() + c.fringe.iter().sum::<f64>())
                .unwrap_or(0.0);
            ws.write_formula_with_format(
                row0,
                lay.total as u16,
                Formula::new(format!("=SUM({first_amt}{r}:{last_fr}{r})"))
                    .set_result(money(row_total)),
                &money_fmt,
            )
            .map_err(xe)?;
        }
        let r0 = (lay.first - 1) as u32;
        let r1 = (lay.last - 1) as u32;
        let cats: Vec<&str> = Category::ALL.iter().map(|c| c.label()).collect();
        ws.add_data_validation(
            r0,
            1,
            r1,
            1,
            &DataValidation::new()
                .allow_list_strings(&cats)
                .map_err(xe)?,
        )
        .map_err(xe)?;
        ws.add_data_validation(
            r0,
            3,
            r1,
            3,
            &DataValidation::new()
                .allow_list_strings(&["Federal", "Cost share"])
                .map_err(xe)?,
        )
        .map_err(xe)?;
        ws.add_data_validation(
            r0,
            8,
            r1,
            8,
            &DataValidation::new()
                .allow_list_strings(&["Yes", "No"])
                .map_err(xe)?,
        )
        .map_err(xe)?;
        ws.add_data_validation(
            r0,
            11,
            r1,
            11,
            &DataValidation::new()
                .allow_list_strings(&["Included", "Excluded", "Subaward"])
                .map_err(xe)?,
        )
        .map_err(xe)?;
        ws.set_freeze_panes(5, 3).map_err(xe)?;
        for (c, w) in [
            (0u16, 5.0),
            (1, 13.0),
            (2, 36.0),
            (3, 12.0),
            (4, 12.0),
            (5, 16.0),
            (6, 12.0),
            (7, 14.0),
            (8, 9.0),
            (9, 8.0),
            (10, 8.0),
            (11, 14.0),
        ] {
            ws.set_column_width(c, w).map_err(xe)?;
        }
        for c in lay.amount0..=lay.total {
            ws.set_column_width(c as u16, 14.0).map_err(xe)?;
        }
        ws.set_row_height(4, 46).map_err(xe)?;
    }

    // ---- Budget summary
    {
        let ws = wb.add_worksheet();
        ws.set_name("Budget summary").map_err(xe)?;
        ws.write_string_with_format(0, 0, "Budget summary (all formulas)", &title_fmt)
            .map_err(xe)?;
        ws.write_string_with_format(
            1,
            0,
            "Rows follow the object class lines of SF-424A Section B (6a to 6k); the line letters are low confidence (the form was not read), so check them against the form your funder uses. Construction (6g) is not built.",
            &note_fmt,
        )
        .map_err(xe)?;
        let total_col = n + 1;
        let names = [
            ("Personnel (6a)", Some(Category::Personnel)),
            ("Fringe benefits (6b)", None),
            ("Travel (6c)", Some(Category::Travel)),
            ("Equipment (6d)", Some(Category::Equipment)),
            ("Supplies (6e)", Some(Category::Supplies)),
            ("Contractual (6f)", Some(Category::Contractual)),
            ("Other direct (6h)", Some(Category::Other)),
        ];
        let write_block = |ws: &mut Worksheet,
                           top: usize, // Excel row of the block header
                           label: &str,
                           funding: Option<Funding>,
                           blk: &Block,
                           others: Option<(usize, usize)>|
         -> Result<()> {
            ws.write_string_with_format((top - 1) as u32, 0, label, &head)
                .map_err(xe)?;
            for k in 0..n {
                ws.write_string_with_format(
                    (top - 1) as u32,
                    (k + 1) as u16,
                    format!("Year {}", k + 1),
                    &head,
                )
                .map_err(xe)?;
            }
            ws.write_string_with_format((top - 1) as u32, total_col as u16, "Total", &head)
                .map_err(xe)?;
            let row_of = |i: usize| top + 1 + i; // Excel rows: 0..=6 classes, 7 direct, 8 base, 9 indirect, 10 total
            for (i, (nm, cat)) in names.iter().enumerate() {
                ws.write_string((row_of(i) - 1) as u32, 0, *nm)
                    .map_err(xe)?;
                for k in 0..n {
                    let c = col_name(k + 1);
                    let vals: &[f64] = match cat {
                        Some(cc) => blk.category(*cc),
                        None => &blk.fringe,
                    };
                    let f = match (funding, others) {
                        (Some(fd), _) => {
                            let (amt_col, is_fringe) = match cat {
                                Some(_) => (lay.amount0 + k, false),
                                None => (lay.fringe0 + k, true),
                            };
                            let crit = if is_fringe {
                                "Personnel".to_string()
                            } else {
                                cat.unwrap().label().to_string()
                            };
                            format!(
                                "=SUMIFS({},'Line items'!$B${}:$B${},\"{crit}\",'Line items'!$D${}:$D${},\"{}\")",
                                lay.range(amt_col),
                                lay.first,
                                lay.last,
                                lay.first,
                                lay.last,
                                fd.label()
                            )
                        }
                        (None, Some((a, b))) => format!("={c}{}+{c}{}", a + 1 + i, b + 1 + i),
                        _ => unreachable!(),
                    };
                    ws.write_formula_with_format(
                        (row_of(i) - 1) as u32,
                        (k + 1) as u16,
                        Formula::new(f).set_result(money(vals[k])),
                        &money_fmt,
                    )
                    .map_err(xe)?;
                }
                let tot: f64 = match cat {
                    Some(cc) => blk.category(*cc).iter().sum(),
                    None => blk.fringe.iter().sum(),
                };
                let last = col_name(n);
                ws.write_formula_with_format(
                    (row_of(i) - 1) as u32,
                    total_col as u16,
                    Formula::new(format!("=SUM(B{r}:{last}{r})", r = row_of(i)))
                        .set_result(money(tot)),
                    &money_fmt,
                )
                .map_err(xe)?;
            }
            let (r_direct, r_base, r_ind, r_tot) = (row_of(7), row_of(8), row_of(9), row_of(10));
            ws.write_string_with_format(
                (r_direct - 1) as u32,
                0,
                "Total direct costs (6i)",
                &bold_top,
            )
            .map_err(xe)?;
            ws.write_string((r_base - 1) as u32, 0, "Indirect base (memo)")
                .map_err(xe)?;
            ws.write_string((r_ind - 1) as u32, 0, "Indirect charges (6j)")
                .map_err(xe)?;
            ws.write_string_with_format((r_tot - 1) as u32, 0, "Total (6k)", &bold_top)
                .map_err(xe)?;
            let last = col_name(n);
            for k in 0..n {
                let c = col_name(k + 1);
                ws.write_formula_with_format(
                    (r_direct - 1) as u32,
                    (k + 1) as u16,
                    Formula::new(format!("=SUM({c}{}:{c}{})", row_of(0), row_of(6)))
                        .set_result(money(blk.direct[k])),
                    &money_bold,
                )
                .map_err(xe)?;
                let (base_f, ind_f) = match (funding, others) {
                    (Some(fd), _) => {
                        let mtdc = format!(
                            "SUMIFS({},'Line items'!$D${}:$D${},\"{}\")",
                            lay.range(lay.mtdc0 + k),
                            lay.first,
                            lay.last,
                            fd.label()
                        );
                        let base = format!(
                            "=CHOOSE(Inputs!$B${},{mtdc},{c}{r_direct},{c}{},{c}{}+{c}{})",
                            inputs::BASE_INDEX,
                            row_of(0),
                            row_of(0),
                            row_of(1)
                        );
                        let ind = if fd == Funding::Federal {
                            format!("=ROUND(Inputs!$B${}*{c}{r_base},2)", inputs::RATE)
                        } else {
                            format!(
                                "=IF(Inputs!$B${}=\"Yes\",ROUND(Inputs!$B${}*{c}{r_base},2),0)",
                                inputs::CS_INDIRECT,
                                inputs::RATE
                            )
                        };
                        (base, ind)
                    }
                    (None, Some((a, b))) => (
                        format!("={c}{}+{c}{}", a + 8 + 1, b + 8 + 1),
                        format!("={c}{}+{c}{}", a + 9 + 1, b + 9 + 1),
                    ),
                    _ => unreachable!(),
                };
                ws.write_formula_with_format(
                    (r_base - 1) as u32,
                    (k + 1) as u16,
                    Formula::new(base_f).set_result(money(blk.indirect_base[k])),
                    &money_fmt,
                )
                .map_err(xe)?;
                ws.write_formula_with_format(
                    (r_ind - 1) as u32,
                    (k + 1) as u16,
                    Formula::new(ind_f).set_result(money(blk.indirect[k])),
                    &money_fmt,
                )
                .map_err(xe)?;
                ws.write_formula_with_format(
                    (r_tot - 1) as u32,
                    (k + 1) as u16,
                    Formula::new(format!("={c}{r_direct}+{c}{r_ind}"))
                        .set_result(money(blk.total[k])),
                    &money_bold,
                )
                .map_err(xe)?;
            }
            for (r, vals, f) in [
                (r_direct, &blk.direct, &money_bold),
                (r_base, &blk.indirect_base, &money_fmt),
                (r_ind, &blk.indirect, &money_fmt),
                (r_tot, &blk.total, &money_bold),
            ] {
                ws.write_formula_with_format(
                    (r - 1) as u32,
                    total_col as u16,
                    Formula::new(format!("=SUM(B{r}:{last}{r})"))
                        .set_result(money(vals.iter().sum())),
                    f,
                )
                .map_err(xe)?;
            }
            Ok(())
        };
        let c = inp.computed;
        let fed_top = 4;
        let cs_top = 17;
        let tot_top = 30;
        write_block(
            ws,
            fed_top,
            "Federal request",
            Some(Funding::Federal),
            &c.federal,
            None,
        )?;
        write_block(
            ws,
            cs_top,
            "Cost share",
            Some(Funding::CostShare),
            &c.cost_share,
            None,
        )?;
        let project = project_block(&c.federal, &c.cost_share);
        write_block(
            ws,
            tot_top,
            "Total project",
            None,
            &project,
            Some((fed_top, cs_top)),
        )?;

        // Match check (rows 43-47) and arithmetic check (rows 49-50).
        let last = col_name(n);
        let t = col_name(total_col);
        let fed_total_cell = format!("{t}{}", fed_top + 11);
        let cs_total_cell = format!("{t}{}", cs_top + 11);
        let proj_total_cell = format!("{t}{}", tot_top + 11);
        ws.write_string_with_format(42, 0, "Cost share check", &head)
            .map_err(xe)?;
        ws.write_string_with_format(42, 1, "Value", &head)
            .map_err(xe)?;
        let (req_val, short_val, status) = match &c.match_check {
            Some(m) => (
                m.required,
                m.shortfall,
                if m.shortfall <= 0.0 {
                    "Requirement met"
                } else {
                    "Below the requirement"
                },
            ),
            None => (0.0, 0.0, "No requirement entered"),
        };
        ws.write_string(43, 0, "Required cost share (amount)")
            .map_err(xe)?;
        ws.write_formula_with_format(
            43,
            1,
            Formula::new(format!(
                "=IF(Inputs!$B${p}=\"\",0,IF(Inputs!$B${b}=\"Federal request\",Inputs!$B${p}*{fed_total_cell},Inputs!$B${p}*{proj_total_cell}))",
                p = inputs::MATCH_PCT,
                b = inputs::MATCH_BASIS
            ))
            .set_result(money(req_val)),
            &money_fmt,
        )
        .map_err(xe)?;
        ws.write_string(44, 0, "Cost share in this budget")
            .map_err(xe)?;
        ws.write_formula_with_format(
            44,
            1,
            Formula::new(format!("={cs_total_cell}"))
                .set_result(money(c.cost_share.total.iter().sum())),
            &money_fmt,
        )
        .map_err(xe)?;
        ws.write_string(45, 0, "Shortfall").map_err(xe)?;
        ws.write_formula_with_format(
            45,
            1,
            Formula::new("=MAX(0,B44-B45)").set_result(money(short_val)),
            &money_fmt,
        )
        .map_err(xe)?;
        ws.write_string(46, 0, "Status").map_err(xe)?;
        ws.write_formula(
            46,
            1,
            Formula::new("=IF(Inputs!$B$12=\"\",\"No requirement entered\",IF(B46<=0,\"Requirement met\",\"Below the requirement\"))")
                .set_result(status),
        )
        .map_err(xe)?;
        ws.write_string_with_format(48, 0, "Arithmetic check", &head)
            .map_err(xe)?;
        ws.write_string_with_format(48, 1, "Result", &head)
            .map_err(xe)?;
        ws.write_string(49, 0, "Line items total equals the summary's direct costs")
            .map_err(xe)?;
        let line_total_range = lay.range(lay.total);
        ws.write_formula(
            49,
            1,
            Formula::new(format!(
                "=IF(ROUND(SUM({line_total_range})-({t}{}+{t}{}),2)=0,\"OK\",\"Check Category and Funding on every line\")",
                fed_top + 8,
                cs_top + 8
            ))
            .set_result("OK"),
        )
        .map_err(xe)?;
        let _ = last;
        ws.set_column_width(0, 40).map_err(xe)?;
        for k in 1..=total_col {
            ws.set_column_width(k as u16, 16.0).map_err(xe)?;
        }
        ws.set_freeze_panes(0, 1).map_err(xe)?;
    }

    // Open on the summary.
    wb.worksheet_from_name("Budget summary")
        .map_err(xe)?
        .set_active(true);
    wb.save_to_buffer().map_err(xe)
}

/// Federal plus cost share, per year.
pub fn project_block(a: &Block, b: &Block) -> Block {
    let add = |x: &[f64], y: &[f64]| -> Vec<f64> { x.iter().zip(y).map(|(p, q)| p + q).collect() };
    Block {
        personnel: add(&a.personnel, &b.personnel),
        fringe: add(&a.fringe, &b.fringe),
        travel: add(&a.travel, &b.travel),
        equipment: add(&a.equipment, &b.equipment),
        supplies: add(&a.supplies, &b.supplies),
        contractual: add(&a.contractual, &b.contractual),
        other: add(&a.other, &b.other),
        direct: add(&a.direct, &b.direct),
        mtdc: add(&a.mtdc, &b.mtdc),
        indirect_base: add(&a.indirect_base, &b.indirect_base),
        indirect: add(&a.indirect, &b.indirect),
        total: add(&a.total, &b.total),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names() {
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(12), "M");
        assert_eq!(col_name(25), "Z");
        assert_eq!(col_name(26), "AA");
        assert_eq!(col_name(27), "AB");
    }

    #[test]
    fn amount_formula_references_the_right_cells() {
        let f = amount_formula(7, "M$4");
        assert!(f.starts_with("=IF(OR($B7=\"\",M$4<$J7,M$4>$K7),0,ROUND($E7*$F7*"));
        assert!(f.contains("Inputs!$B$4") && f.contains("Inputs!$B$5"));
    }
}
