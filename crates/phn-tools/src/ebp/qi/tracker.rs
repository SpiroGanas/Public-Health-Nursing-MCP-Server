//! The measurement tracker workbook (XLSX) for `plan_quality_improvement`.
//!
//! The Data sheet holds one row per period and live formulas that apply the run chart rules
//! (shift, trend, runs, and the Anhøj long-run and crossings rules) and control limits (XmR, or p
//! when there are denominators), so the nurse can keep adding periods and see the signals update.
//! The thresholds are cells on the Settings sheet. Other sheets hold the measures, the PDSA log,
//! and the SQUIRE checklist. A native Excel chart plots the series with its median.

use phn_core::{PhnError, Result};
use rust_xlsxwriter::{
    Chart, ChartLine, ChartMarker, ChartMarkerType, ChartType, DataValidation,
    DataValidationRule, Format, FormatAlign, FormatBorder, Formula, Workbook, Worksheet,
    XlsxError,
};

use super::rules::{self, SHIFT_LEN, TREND_LEN};
use super::types::{CycleOut, MeasureRowOut, SquireItemOut, SquireState};

fn xe(e: XlsxError) -> PhnError {
    PhnError::Internal(format!("XLSX: {e}"))
}

/// What goes into the workbook.
pub(super) struct TrackerInput<'a> {
    pub title: &'a str,
    pub aim: Option<&'a str>,
    pub measures: &'a [MeasureRowOut],
    pub labels: &'a [String],
    pub numerators: &'a [f64],
    pub denominators: Option<&'a [f64]>,
    pub baseline_points: usize,
    pub measure_name: Option<&'a str>,
    pub pdsa: &'a [CycleOut],
    pub squire: &'a [SquireItemOut],
    pub review_note: &'a str,
}

/// Rows 2..=last of the Data sheet carry formulas.
fn last_row(n: usize) -> u32 {
    (n as u32 + 24).max(48)
}

/// The IHI runs-limit table written to the Settings sheet: for 8 to 100 points off the median,
/// split as evenly as possible above and below.
pub(super) fn runs_table() -> Vec<(usize, Option<usize>, Option<usize>)> {
    (8usize..=100)
        .map(|n| {
            let (few, many) = rules::runs_limits(n.div_ceil(2), n / 2);
            (n, few, many)
        })
        .collect()
}

/// Formulas for one Data row (`r` is the Excel row, 2 or more). Public to the module for tests.
pub(super) struct RowFormulas {
    pub value: String,
    pub median: String,
    pub side: String,
    pub last_side: String,
    pub run_len: String,
    pub shift: String,
    pub trend_len: String,
    pub trend_dir: String,
    pub trend: String,
    pub new_run: String,
    pub moving_range: Option<String>,
    pub center: String,
    pub lcl: String,
    pub ucl: String,
    pub outside: String,
}

pub(super) fn row_formulas(r: u32, proportion: bool) -> RowFormulas {
    let p = r - 1;
    let first = r == 2;
    RowFormulas {
        value: format!(r#"=IF(B{r}="","",IF(C{r}="",B{r},IF(C{r}=0,"",B{r}/C{r})))"#),
        median: format!(r#"=IF(D{r}="","",Settings!$B$5)"#),
        side: format!(r#"=IF(D{r}="","",IF(D{r}>E{r},1,IF(D{r}<E{r},-1,0)))"#),
        last_side: format!(r#"=IF(F{r}="","",IF(F{r}<>0,F{r},N(G{p})))"#),
        run_len: format!(
            r#"=IF(F{r}="","",IF(F{r}=0,N(H{p}),IF(F{r}=N(G{p}),N(H{p})+1,1)))"#
        ),
        shift: format!(r#"=IF(F{r}="","",IF(AND(F{r}<>0,H{r}>=Settings!$B$3),"Shift",""))"#),
        trend_len: if first {
            format!(r#"=IF(D{r}="","",1)"#)
        } else {
            format!(
                r#"=IF(D{r}="","",IF(D{r}=D{p},N(J{p}),IF(SIGN(D{r}-D{p})=N(K{p}),N(J{p})+1,2)))"#
            )
        },
        trend_dir: if first {
            format!(r#"=IF(D{r}="","",0)"#)
        } else {
            format!(r#"=IF(D{r}="","",IF(D{r}=D{p},N(K{p}),SIGN(D{r}-D{p})))"#)
        },
        trend: format!(r#"=IF(D{r}="","",IF(J{r}>=Settings!$B$4,"Trend",""))"#),
        new_run: format!(r#"=IF(F{r}="","",IF(AND(F{r}<>0,F{r}<>N(G{p})),1,0))"#),
        moving_range: (!first)
            .then(|| format!(r#"=IF(OR(D{r}="",D{p}=""),"",ABS(D{r}-D{p}))"#)),
        center: format!(r#"=IF(D{r}="","",Settings!$B$7)"#),
        lcl: if proportion {
            format!(
                r#"=IF(OR(D{r}="",C{r}=""),"",MAX(0,Settings!$B$7-3*SQRT(Settings!$B$7*(1-Settings!$B$7)/C{r})))"#
            )
        } else {
            format!(r#"=IF(D{r}="","",Settings!$B$7-3*Settings!$B$8/Settings!$B$9)"#)
        },
        ucl: if proportion {
            format!(
                r#"=IF(OR(D{r}="",C{r}=""),"",MIN(1,Settings!$B$7+3*SQRT(Settings!$B$7*(1-Settings!$B$7)/C{r})))"#
            )
        } else {
            format!(r#"=IF(D{r}="","",Settings!$B$7+3*Settings!$B$8/Settings!$B$9)"#)
        },
        outside: format!(
            r#"=IF(OR(D{r}="",P{r}="",Q{r}=""),"",IF(OR(D{r}>Q{r},D{r}<P{r}),"Outside limits",""))"#
        ),
    }
}

fn state_label(s: SquireState) -> &'static str {
    match s {
        SquireState::Todo => "to do",
        SquireState::InProgress => "in progress",
        SquireState::Done => "done",
        SquireState::NotApplicable => "not applicable",
    }
}

fn opt(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("")
}

/// Build the workbook.
pub(super) fn build(t: &TrackerInput) -> Result<Vec<u8>> {
    let n = t.labels.len();
    let last = last_row(n);
    let proportion = t.denominators.is_some();
    let mut wb = Workbook::new();
    let head = Format::new()
        .set_bold()
        .set_background_color(0xE1E0D9)
        .set_border(FormatBorder::Thin)
        .set_text_wrap();
    let bold = Format::new().set_bold();
    let wrap = Format::new().set_text_wrap().set_align(FormatAlign::Top);
    let input_fmt = Format::new().set_background_color(0xFFF7D6);
    let pct = Format::new().set_num_format("0.0%");
    let num2 = Format::new().set_num_format("0.00");
    let title_fmt = Format::new().set_bold().set_font_size(14);

    // ---- Read me
    {
        let ws = wb.add_worksheet();
        ws.set_name("Read me").map_err(xe)?;
        ws.set_column_width(0, 110).map_err(xe)?;
        ws.write_string_with_format(0, 0, format!("{}: measurement tracker", t.title), &title_fmt)
            .map_err(xe)?;
        let lines = [
            "How to use this workbook",
            "1. Data sheet: type or paste one row per period (a month, a week) in time order, with no blank rows in the middle. Column B is the number for the period (a count, an average, or the numerator). Column C is the denominator only if the measure is a proportion. Yellow cells are for you; everything else is a formula.",
            "2. Settings sheet: set how many points at the start make the baseline (the median or center line is calculated from them), and the lengths for a shift and a trend. The defaults are 6 and 5.",
            "3. Summary sheet: the rules check themselves as you add periods. Read a signal as 'look at what happened', not as proof of why.",
            "4. When you make a documented change to the process, note it in the PDSA log. To start a new median after a change, copy the Data sheet's rows for the new phase to a new workbook, or ask the tool to re-run the analysis with phase_starts.",
            "5. The chart covers the rows that had data when the workbook was made. If you add periods, drag the chart's data ranges to include them.",
            "",
            "What the rules mean",
            "Shift: 6 or more points in a row on one side of the median. Trend: 5 or more points in a row all going up or all going down (a repeated value counts once). Too few or too many runs: compared with the limits in the table on the Settings sheet. Anhøj rules: the longest run is longer than round(log2(n)+3), or the line crosses the median fewer times than the 5th percentile of a binomial(n-1, 0.5). In a simulation study the Anhøj rules performed better than the IHI/Perla rules, which gave many false signals.",
            "The astronomical-point rule is a judgment (a point that is obviously different from the rest), so it is not calculated here: look at the chart.",
            "",
            "Privacy",
            "This workbook is meant for aggregate numbers only. Do not type names, record numbers, or dates of birth into it. It was written to your output folder; keep it in your agency's secure storage.",
            "",
            "Status",
        ];
        for (i, l) in lines.iter().enumerate() {
            let f = if matches!(*l, "How to use this workbook" | "What the rules mean" | "Privacy" | "Status") {
                &bold
            } else {
                &wrap
            };
            ws.write_string_with_format(2 + i as u32, 0, *l, f).map_err(xe)?;
        }
        ws.write_string_with_format(2 + lines.len() as u32, 0, t.review_note, &wrap)
            .map_err(xe)?;
    }

    // ---- Aim and measures
    {
        let ws = wb.add_worksheet();
        ws.set_name("Measures").map_err(xe)?;
        ws.write_string_with_format(0, 0, "Aim", &bold).map_err(xe)?;
        ws.write_string_with_format(
            0,
            1,
            t.aim.unwrap_or("[Write the aim: how much improvement, for whom, by when]"),
            &wrap,
        )
        .map_err(xe)?;
        let heads = [
            "Measure",
            "Type",
            "Numerator",
            "Denominator",
            "Data source",
            "Frequency",
            "Sampling",
            "Owner (role)",
            "Goal",
        ];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(2, c as u16, *h, &head).map_err(xe)?;
            ws.set_column_width(c as u16, if c == 0 { 36.0 } else { 22.0 })
                .map_err(xe)?;
        }
        let rows = t.measures.len().max(6) as u32;
        for (i, m) in t.measures.iter().enumerate() {
            let r = 3 + i as u32;
            for (c, v) in [
                m.name.as_str(),
                m.kind.as_str(),
                opt(&m.numerator),
                opt(&m.denominator),
                opt(&m.data_source),
                opt(&m.frequency),
                opt(&m.sampling),
                opt(&m.owner),
                opt(&m.goal),
            ]
            .iter()
            .enumerate()
            {
                ws.write_string_with_format(r, c as u16, *v, &wrap).map_err(xe)?;
            }
        }
        let dv = DataValidation::new()
            .allow_list_strings(&["outcome", "process", "balancing"])
            .map_err(xe)?;
        ws.add_data_validation(3, 1, 2 + rows, 1, &dv).map_err(xe)?;
        ws.write_string_with_format(
            4 + rows,
            0,
            "A family of measures has at least one outcome measure, one process measure, and one balancing measure.",
            &wrap,
        )
        .map_err(xe)?;
        ws.set_freeze_panes(3, 1).map_err(xe)?;
    }

    // ---- Data
    {
        let ws = wb.add_worksheet();
        ws.set_name("Data").map_err(xe)?;
        let heads = [
            "Period",
            if proportion { "Numerator" } else { "Count or value" },
            "Denominator (proportions only)",
            "Value",
            "Median (baseline)",
            "Side of median (1 above, -1 below, 0 on)",
            "Last side seen",
            "Run length so far",
            "Shift signal",
            "Trend length so far",
            "Trend direction",
            "Trend signal",
            "New run (1 = yes)",
            "Moving range",
            if proportion { "Center line (p-bar)" } else { "Center line (mean)" },
            "Lower control limit",
            "Upper control limit",
            "Outside limits",
        ];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, *h, &head).map_err(xe)?;
            ws.set_column_width(c as u16, if c == 0 { 14.0 } else { 15.0 })
                .map_err(xe)?;
        }
        ws.set_row_height(0, 62).map_err(xe)?;
        let vfmt = if proportion { &pct } else { &num2 };
        for r in 2..=last {
            let i = (r - 2) as usize;
            let f = row_formulas(r, proportion);
            let row = r - 1;
            if i < n {
                ws.write_string_with_format(row, 0, &t.labels[i], &input_fmt).map_err(xe)?;
                ws.write_number_with_format(row, 1, t.numerators[i], &input_fmt)
                    .map_err(xe)?;
                if let Some(d) = t.denominators {
                    ws.write_number_with_format(row, 2, d[i], &input_fmt).map_err(xe)?;
                }
            } else {
                ws.write_blank(row, 0, &input_fmt).map_err(xe)?;
                ws.write_blank(row, 1, &input_fmt).map_err(xe)?;
                ws.write_blank(row, 2, &input_fmt).map_err(xe)?;
            }
            let put = |ws: &mut Worksheet, col: u16, s: &str, fmt: Option<&Format>| -> Result<()> {
                match fmt {
                    Some(fm) => ws.write_formula_with_format(row, col, Formula::new(s), fm),
                    None => ws.write_formula(row, col, Formula::new(s)),
                }
                .map(|_| ())
                .map_err(xe)
            };
            put(ws, 3, &f.value, Some(vfmt))?;
            put(ws, 4, &f.median, Some(vfmt))?;
            put(ws, 5, &f.side, None)?;
            put(ws, 6, &f.last_side, None)?;
            put(ws, 7, &f.run_len, None)?;
            put(ws, 8, &f.shift, None)?;
            put(ws, 9, &f.trend_len, None)?;
            put(ws, 10, &f.trend_dir, None)?;
            put(ws, 11, &f.trend, None)?;
            put(ws, 12, &f.new_run, None)?;
            if let Some(mr) = &f.moving_range {
                put(ws, 13, mr, Some(num2.clone()).as_ref())?;
            }
            put(ws, 14, &f.center, Some(vfmt))?;
            put(ws, 15, &f.lcl, Some(vfmt))?;
            put(ws, 16, &f.ucl, Some(vfmt))?;
            put(ws, 17, &f.outside, None)?;
        }
        ws.set_freeze_panes(1, 1).map_err(xe)?;
    }

    // ---- Settings and rules
    {
        let ws = wb.add_worksheet();
        ws.set_name("Settings").map_err(xe)?;
        ws.set_column_width(0, 44).map_err(xe)?;
        ws.set_column_width(1, 16).map_err(xe)?;
        ws.set_column_width(2, 56).map_err(xe)?;
        ws.set_column_width(4, 18).map_err(xe)?;
        ws.set_column_width(5, 26).map_err(xe)?;
        ws.set_column_width(6, 26).map_err(xe)?;
        ws.write_string_with_format(0, 0, "Settings (yellow cells can be changed)", &bold)
            .map_err(xe)?;
        let dl = last;
        let rng = |c: &str| format!("Data!${c}$2:${c}${dl}");
        let base = format!("INDEX({},1):INDEX({},Settings!$B$2)", rng("D"), rng("D"));
        let rows: [(u32, &str, Option<f64>, Option<String>, &str); 8] = [
            (1, "Baseline points (the first points used for the median or center line)", Some(t.baseline_points.max(2) as f64), None, "Use 12 to 20 points when you have them. Change it after a documented process change only to start a new baseline."),
            (2, "Shift: consecutive points on one side of the median", Some(SHIFT_LEN as f64), None, "IHI/Perla rule: 6 or more."),
            (3, "Trend: consecutive points going one way", Some(TREND_LEN as f64), None, "IHI/Perla rule: 5 or more; a repeated value counts once."),
            (4, "Median of the baseline points", None, Some(format!(r#"=IFERROR(MEDIAN({base}),"")"#)), "Points on the median are skipped by the run rules."),
            (5, "Control chart type", None, Some(if proportion { r#"="p chart""# } else { r#"="XmR (individuals)""# }.to_string()), "p chart when there is a denominator each period; otherwise XmR."),
            (6, if proportion { "Center line (p-bar: baseline numerators / baseline denominators)" } else { "Center line (mean of the baseline points)" }, None,
                Some(if proportion {
                    format!(r#"=IFERROR(SUM(INDEX({n},1):INDEX({n},Settings!$B$2))/SUM(INDEX({d},1):INDEX({d},Settings!$B$2)),"")"#, n = rng("B"), d = rng("C"))
                } else {
                    format!(r#"=IFERROR(AVERAGE({base}),"")"#)
                }), "Limits are 3 sigma from the center line."),
            (7, "Average moving range of the baseline (XmR only)", None,
                Some(if proportion { r#"="n/a""#.to_string() } else {
                    format!(r#"=IFERROR(AVERAGE(INDEX({m},2):INDEX({m},Settings!$B$2)),"")"#, m = rng("N"))
                }), "XmR sigma = average moving range / 1.128."),
            (8, "d2 constant for moving ranges of 2", Some(1.128), None, "3 / 1.128 = 2.66 times the average moving range."),
        ];
        for (r, label, val, formula, note) in rows {
            ws.write_string_with_format(r, 0, label, &wrap).map_err(xe)?;
            match (val, formula) {
                (Some(v), _) => {
                    ws.write_number_with_format(r, 1, v, &input_fmt).map_err(xe)?;
                }
                (None, Some(f)) => {
                    ws.write_formula(r, 1, Formula::new(f)).map_err(xe)?;
                }
                _ => {}
            }
            ws.write_string_with_format(r, 2, note, &wrap).map_err(xe)?;
        }
        let dv = DataValidation::new()
            .allow_whole_number(DataValidationRule::Between(2, 600))
            .set_input_title("Whole number")
            .map_err(xe)?;
        ws.add_data_validation(1, 1, 3, 1, &dv).map_err(xe)?;
        // Runs-limit table (columns E to G, from row 2 down).
        ws.write_string_with_format(0, 4, "IHI runs test limits", &bold).map_err(xe)?;
        for (c, h) in ["Points off the median", "Too few runs if at most", "Too many runs if at least"]
            .iter()
            .enumerate()
        {
            ws.write_string_with_format(1, 4 + c as u16, *h, &head).map_err(xe)?;
        }
        for (i, (n, few, many)) in runs_table().into_iter().enumerate() {
            let r = 2 + i as u32;
            ws.write_number(r, 4, n as f64).map_err(xe)?;
            match few {
                Some(v) => ws.write_number(r, 5, v as f64),
                None => ws.write_string(r, 5, "no signal possible"),
            }
            .map_err(xe)?;
            match many {
                Some(v) => ws.write_number(r, 6, v as f64),
                None => ws.write_string(r, 6, "no signal possible"),
            }
            .map_err(xe)?;
        }
        ws.write_string_with_format(
            2 + 93 + 1,
            4,
            "Computed from the exact distribution of the number of runs, 2.5% in each tail, with the points split as evenly as possible above and below the median. Perla's own table was not compared. The tool's response uses the actual numbers above and below.",
            &wrap,
        )
        .map_err(xe)?;
    }

    // ---- Summary
    {
        let ws = wb.add_worksheet();
        ws.set_name("Summary").map_err(xe)?;
        ws.set_column_width(0, 56).map_err(xe)?;
        ws.set_column_width(1, 30).map_err(xe)?;
        ws.write_string_with_format(0, 0, "Signals (update as you add periods)", &title_fmt)
            .map_err(xe)?;
        let rng = |c: &str| format!("Data!${c}$2:${c}${last}");
        let items: Vec<(&str, String)> = vec![
            ("Periods with data", format!("=COUNT({})", rng("D"))),
            ("Points above the median", format!("=COUNTIF({},1)", rng("F"))),
            ("Points below the median", format!("=COUNTIF({},-1)", rng("F"))),
            ("Points on the median (skipped)", format!("=COUNTIF({},0)", rng("F"))),
            ("Points off the median (n)", "=B4+B5".to_string()),
            ("Longest run on one side of the median", format!("=MAX({})", rng("H"))),
            ("Number of runs", format!("=SUM({})", rng("M"))),
            ("Crossings of the median", "=MAX(0,B9-1)".to_string()),
            ("Longest trend (points, ties counted once)", format!("=MAX({})", rng("J"))),
            ("", String::new()),
            ("IHI/Perla: shift", r#"=IF(B8>=Settings!$B$3,"Signal: shift","No shift")"#.to_string()),
            ("IHI/Perla: trend", r#"=IF(B11>=Settings!$B$4,"Signal: trend","No trend")"#.to_string()),
            ("IHI/Perla: too few runs if at most", r#"=IFERROR(VLOOKUP(B7,Settings!$E$3:$G$95,2,FALSE),"n/a")"#.to_string()),
            ("IHI/Perla: too many runs if at least", r#"=IFERROR(VLOOKUP(B7,Settings!$E$3:$G$95,3,FALSE),"n/a")"#.to_string()),
            ("IHI/Perla: runs test", r#"=IF(AND(ISNUMBER(B15),B9<=B15),"Signal: too few runs",IF(AND(ISNUMBER(B16),B9>=B16),"Signal: too many runs","No runs signal"))"#.to_string()),
            ("", String::new()),
            ("Anhøj: longest run is a signal when longer than", r#"=IF(B7<2,"n/a",ROUND(LOG(B7,2)+3,0))"#.to_string()),
            ("Anhøj: crossings are a signal when fewer than", r#"=IF(B7<2,"n/a",CRITBINOM(B7-1,0.5,0.05))"#.to_string()),
            ("Anhøj: unusually long run", r#"=IF(ISNUMBER(B19),IF(B8>B19,"Signal: unusually long run","No signal"),"n/a")"#.to_string()),
            ("Anhøj: too few crossings", r#"=IF(ISNUMBER(B20),IF(B10<B20,"Signal: too few crossings","No signal"),"n/a")"#.to_string()),
            ("", String::new()),
            ("Points outside the control limits", format!(r#"=COUNTIF({},"Outside limits")"#, rng("R"))),
        ];
        for (i, (label, f)) in items.iter().enumerate() {
            let r = 2 + i as u32;
            if label.is_empty() {
                continue;
            }
            ws.write_string(r, 0, *label).map_err(xe)?;
            ws.write_formula(r, 1, Formula::new(f.as_str())).map_err(xe)?;
        }
        ws.write_string_with_format(
            2 + items.len() as u32 + 1,
            0,
            "Astronomical point (IHI/Perla): a judgment, not calculated here. Look at the chart for a point that is obviously different from the rest and ask what happened that period.",
            &wrap,
        )
        .map_err(xe)?;
        // Chart of the series with its median (and limits).
        if n >= 2 {
            let mut chart = Chart::new(ChartType::Line);
            let name = t.measure_name.unwrap_or("Value");
            chart
                .add_series()
                .set_name(name)
                .set_categories(("Data", 1, 0, n as u32, 0))
                .set_values(("Data", 1, 3, n as u32, 3))
                .set_marker(&ChartMarker::new().set_type(ChartMarkerType::Circle).set_size(6));
            chart
                .add_series()
                .set_name("Median (baseline)")
                .set_categories(("Data", 1, 0, n as u32, 0))
                .set_values(("Data", 1, 4, n as u32, 4))
                .set_format(ChartLine::new().set_dash_type(rust_xlsxwriter::ChartLineDashType::Dash));
            chart.title().set_name(&format!("{}: run chart", t.title));
            chart.x_axis().set_name("Period");
            chart.y_axis().set_name(name);
            chart.set_alt_text(format!(
                "Line chart of {name} over {n} periods from {} to {} with the baseline median as a dashed line.",
                t.labels[0],
                t.labels[n - 1]
            ));
            ws.insert_chart(2, 3, &chart).map_err(xe)?;
        }
    }

    // ---- PDSA log
    {
        let ws = wb.add_worksheet();
        ws.set_name("PDSA").map_err(xe)?;
        let heads = [
            "Cycle",
            "Change tested",
            "Objective",
            "Prediction (before the test)",
            "Who",
            "When",
            "Where",
            "Scale (how small)",
            "Data to collect",
            "Builds on",
            "What happened (Do)",
            "Compared with prediction (Study)",
            "Decision and next cycle (Act)",
        ];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, *h, &head).map_err(xe)?;
            ws.set_column_width(c as u16, if c == 0 { 8.0 } else { 28.0 }).map_err(xe)?;
        }
        let total = t.pdsa.len().max(8);
        for i in 0..total {
            let r = 1 + i as u32;
            ws.write_number(r, 0, (i + 1) as f64).map_err(xe)?;
            if let Some(c) = t.pdsa.get(i) {
                for (col, v) in [
                    c.change.as_str(),
                    opt(&c.objective),
                    opt(&c.prediction),
                    opt(&c.who),
                    opt(&c.when),
                    opt(&c.where_),
                    opt(&c.scale),
                    opt(&c.data_to_collect),
                    opt(&c.builds_on),
                ]
                .iter()
                .enumerate()
                {
                    ws.write_string_with_format(r, 1 + col as u16, *v, &wrap).map_err(xe)?;
                }
            }
        }
        ws.set_freeze_panes(1, 2).map_err(xe)?;
    }

    // ---- SQUIRE checklist
    {
        let ws = wb.add_worksheet();
        ws.set_name("SQUIRE").map_err(xe)?;
        let heads = ["#", "Item", "Section", "What it asks (this tool's paraphrase)", "Status", "Where addressed in your draft"];
        for (c, h) in heads.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, *h, &head).map_err(xe)?;
        }
        for (c, w) in [5.0, 28.0, 18.0, 70.0, 16.0, 40.0].iter().enumerate() {
            ws.set_column_width(c as u16, *w).map_err(xe)?;
        }
        for (i, it) in t.squire.iter().enumerate() {
            let r = 1 + i as u32;
            ws.write_number(r, 0, f64::from(it.number)).map_err(xe)?;
            ws.write_string(r, 1, &it.name).map_err(xe)?;
            ws.write_string(r, 2, &it.section).map_err(xe)?;
            ws.write_string_with_format(r, 3, &it.purpose, &wrap).map_err(xe)?;
            ws.write_string_with_format(r, 4, state_label(it.status), &input_fmt).map_err(xe)?;
            ws.write_string_with_format(r, 5, opt(&it.where_addressed), &input_fmt).map_err(xe)?;
        }
        let k = t.squire.len() as u32;
        let dv = DataValidation::new()
            .allow_list_strings(&["to do", "in progress", "done", "not applicable"])
            .map_err(xe)?;
        ws.add_data_validation(1, 4, k.max(1), 4, &dv).map_err(xe)?;
        ws.write_string(k + 2, 3, "Done").map_err(xe)?;
        ws.write_formula(k + 2, 4, Formula::new(format!(r#"=COUNTIF(E2:E{},"done")"#, k + 1)))
            .map_err(xe)?;
        ws.write_string(k + 3, 3, "Remaining (to do or in progress)").map_err(xe)?;
        ws.write_formula(
            k + 3,
            4,
            Formula::new(format!(
                r#"=COUNTIF(E2:E{e},"to do")+COUNTIF(E2:E{e},"in progress")"#,
                e = k + 1
            )),
        )
        .map_err(xe)?;
        ws.write_string_with_format(
            k + 5,
            0,
            "SQUIRE 2.0 is a copyrighted guideline: this sheet lists item numbers and names with short paraphrases. Read the full guideline at squire-statement.org and cite it when you use it.",
            &wrap,
        )
        .map_err(xe)?;
        ws.set_freeze_panes(1, 2).map_err(xe)?;
    }

    wb.save_to_buffer().map_err(xe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_reference_the_right_cells() {
        let f = row_formulas(5, false);
        assert_eq!(f.value, r#"=IF(B5="","",IF(C5="",B5,IF(C5=0,"",B5/C5)))"#);
        assert!(f.run_len.contains("N(H4)") && f.run_len.contains("N(G4)"));
        assert!(f.trend_len.contains("D4") && f.trend_len.contains("N(K4)"));
        assert!(f.lcl.contains("Settings!$B$8") && f.lcl.contains("Settings!$B$9"));
        assert!(row_formulas(2, false).moving_range.is_none());
        assert!(row_formulas(3, false).moving_range.is_some());
        let p = row_formulas(6, true);
        assert!(p.lcl.contains("SQRT") && p.lcl.contains("C6") && p.ucl.contains("MIN(1"));
    }

    #[test]
    fn runs_table_matches_the_rules_module() {
        let t = runs_table();
        assert_eq!(t.len(), 93);
        let row20 = t.iter().find(|r| r.0 == 20).unwrap();
        assert_eq!((row20.1, row20.2), (Some(6), Some(16)));
    }
}

#[cfg(test)]
mod golden_workbooks {
    use super::*;

    /// Writes one workbook per golden series to the folder in `QI_TRACKER_DIR`, for
    /// `tests/fixtures/generators/check_qi_tracker.py`, which recalculates them in LibreOffice and
    /// compares the formulas' results with the independent Python computation.
    /// Run: `QI_TRACKER_DIR=/tmp/qi cargo test -p phn-tools --lib golden_workbooks -- --ignored`
    #[test]
    #[ignore = "writes files for an external LibreOffice check"]
    fn write_golden_trackers() {
        let Ok(dir) = std::env::var("QI_TRACKER_DIR") else {
            return;
        };
        let fx: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/qi_run_chart_golden.json"
        ))
        .unwrap();
        for c in fx["rule_cases"].as_array().unwrap() {
            let values: Vec<f64> = c["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            let labels: Vec<String> = (1..=values.len()).map(|i| format!("P{i}")).collect();
            let base = c["baseline"].as_u64().map(|b| b as usize).unwrap_or(values.len());
            let bytes = build(&TrackerInput {
                title: "Golden",
                aim: None,
                measures: &[],
                labels: &labels,
                numerators: &values,
                denominators: None,
                baseline_points: base,
                measure_name: None,
                pdsa: &[],
                squire: &[],
                review_note: "test",
            })
            .unwrap();
            std::fs::write(
                std::path::Path::new(&dir).join(format!("{}.xlsx", c["name"].as_str().unwrap())),
                bytes,
            )
            .unwrap();
        }
        // A p-chart workbook from the first control case.
        let p = &fx["control"]["p"][0];
        let nums: Vec<f64> = p["numerators"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let dens: Vec<f64> = p["denominators"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let labels: Vec<String> = (1..=nums.len()).map(|i| format!("P{i}")).collect();
        let bytes = build(&TrackerInput {
            title: "Golden p",
            aim: None,
            measures: &[],
            labels: &labels,
            numerators: &nums,
            denominators: Some(&dens),
            baseline_points: nums.len(),
            measure_name: None,
            pdsa: &[],
            squire: &[],
            review_note: "test",
        })
        .unwrap();
        std::fs::write(std::path::Path::new(&dir).join("control_p0.xlsx"), bytes).unwrap();
        // An XmR workbook from the first control case.
        let x = &fx["control"]["xmr"][0];
        let vals: Vec<f64> = x["values"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let labels: Vec<String> = (1..=vals.len()).map(|i| format!("P{i}")).collect();
        let bytes = build(&TrackerInput {
            title: "Golden xmr",
            aim: None,
            measures: &[],
            labels: &labels,
            numerators: &vals,
            denominators: None,
            baseline_points: vals.len(),
            measure_name: None,
            pdsa: &[],
            squire: &[],
            review_note: "test",
        })
        .unwrap();
        std::fs::write(std::path::Path::new(&dir).join("control_xmr0.xlsx"), bytes).unwrap();
    }
}
