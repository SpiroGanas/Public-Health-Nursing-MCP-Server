//! EBP-04 `build_evidence_synthesis_table`: a synthesis table from the studies
//! the nurse has appraised (Word and Excel, with live formulas in the Excel
//! summary) and a PRISMA 2020 flow diagram (Mermaid, an accessible SVG, and a
//! text alternative) from counts the nurse enters. The counts are checked: a
//! flow that does not add up is a tool error.
//!
//! Everything in the table is what the nurse typed. The tool does not look up,
//! summarise, or rate studies.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, ImageBlock, Inline, TableBlock};
use rust_xlsxwriter::{Format, Formula, Workbook, XlsxError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::prisma::{self, PrismaCounts, PrismaFlow};
use crate::context::Context;
use crate::epr::content;
use crate::md;
use crate::school::templates;

/// Content id of `content/ebp/evidence_synthesis.yaml`.
pub const SYNTHESIS: &str = "ebp-evidence-synthesis";

/// Most studies in one table.
const MAX_STUDIES: usize = 300;
/// Longest text in a cell.
const MAX_CELL: usize = 600;
/// Largest sample size accepted.
const MAX_N: i64 = 1_000_000_000;
/// Shown in summaries for an empty design or rating.
const NOT_STATED: &str = "(not stated)";

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct SynthesisSource {
    title: String,
    publisher: String,
    url: String,
    licence: String,
    effective_date: Option<String>,
    retrieved: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Definition {
    term: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FlowDef {
    statement: String,
    arithmetic: Vec<String>,
    not_drawn: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TableDef {
    quality_scales_note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SynthesisContent {
    sources: BTreeMap<String, SynthesisSource>,
    definitions: Vec<Definition>,
    flow: FlowDef,
    table: TableDef,
}

// ---------------------------------------------------------------- arguments and results

/// One appraised study, as the nurse enters it.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SynthesisStudy {
    /// How you cite the study in the table (for example "Journal of Examples 2021" or a short
    /// reference). Required. The tool does not format references or look studies up.
    pub citation: String,
    /// Year of publication (1900-2100).
    pub year: Option<i32>,
    /// Study design in your words (for example "randomized trial", "cohort", "qualitative"). The
    /// summary counts studies by this text, ignoring capitals.
    pub design: Option<String>,
    /// Population and setting (for example "Adults with hypertension, community health centers").
    pub population: Option<String>,
    /// Number of participants analysed (a whole number; used in the Excel sums).
    pub sample_size: Option<i64>,
    /// The intervention or exposure.
    pub intervention: Option<String>,
    /// The comparator, when there is one.
    pub comparator: Option<String>,
    /// The outcome.
    pub outcome: Option<String>,
    /// The effect as the study reports it (for example "RR 0.82, 95% CI 0.70 to 0.96"). Copy it
    /// from your appraisal; the tool does not interpret it.
    pub effect: Option<String>,
    /// Your quality rating (for example "low risk of bias", "moderate", "high").
    pub quality_rating: Option<String>,
    /// The tool the rating comes from (for example "Cochrane risk of bias 2011", "ROBINS-I",
    /// "AMSTAR 2", "GRADE"). Ratings from different tools are counted separately.
    pub quality_tool: Option<String>,
    /// DOI or PubMed ID, for your reference.
    pub identifier: Option<String>,
    /// A note for the table.
    pub notes: Option<String>,
}

/// Which files to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisFormat {
    /// Word and Excel (default).
    #[default]
    Both,
    /// Word only.
    Docx,
    /// Excel only (with live formulas).
    Xlsx,
}

/// Arguments for `build_evidence_synthesis_table`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SynthesisArgs {
    /// The review question or topic, used as the title (for example "School-based asthma
    /// programs and emergency visits").
    pub question: Option<String>,
    /// The studies you appraised (at most 300). Give these, `prisma`, or both.
    pub studies: Vec<SynthesisStudy>,
    /// Counts for a PRISMA 2020 flow diagram. They are checked: if they do not add up, the tool
    /// reports every arithmetic error and writes nothing.
    pub prisma: Option<PrismaCounts>,
    /// Files to write to /data/output: "both" (default), "docx", or "xlsx". The diagram is also
    /// written as SVG and Mermaid text when counts are given.
    pub format: Option<SynthesisFormat>,
    /// Set false to get the table, summary, and diagram in the response only (no files). Default true.
    pub write_files: Option<bool>,
}

/// A row of the table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TableRow {
    pub citation: String,
    pub year: Option<i32>,
    pub design: Option<String>,
    pub population: Option<String>,
    pub sample_size: Option<i64>,
    pub intervention: Option<String>,
    pub comparator: Option<String>,
    pub outcome: Option<String>,
    pub effect: Option<String>,
    pub quality_rating: Option<String>,
    pub quality_tool: Option<String>,
    pub identifier: Option<String>,
    pub notes: Option<String>,
}

/// A count of studies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LabelCount {
    pub label: String,
    pub studies: usize,
}

/// A count of studies by rating and tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QualityCount {
    pub quality_tool: String,
    pub quality_rating: String,
    pub studies: usize,
}

/// Summary figures (the same ones the Excel summary sheet computes with formulas).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SynthesisSummary {
    pub studies: usize,
    pub studies_with_sample_size: usize,
    /// Sum of the sample sizes entered. Participants in more than one study are counted more than once.
    pub total_participants: i64,
    pub median_sample_size: Option<f64>,
    pub by_design: Vec<LabelCount>,
    pub by_quality: Vec<QualityCount>,
}

/// The PRISMA flow diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrismaOut {
    /// The counts after the checks, with every derived number filled in.
    pub flow: PrismaFlow,
    /// Mermaid flowchart text.
    pub mermaid: String,
    /// A self-contained SVG (title, description, no scripts or external files).
    pub svg: String,
    /// The diagram in sentences, for readers who cannot see it.
    pub text_alternative: Vec<String>,
    /// Short alt text for the image.
    pub alt_text: String,
    /// Formulas the arithmetic follows.
    pub arithmetic: Vec<String>,
    /// What the diagram leaves out compared with the official template.
    pub not_drawn: Vec<String>,
}

/// Result of `build_evidence_synthesis_table`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SynthesisResult {
    pub question: Option<String>,
    pub rows: Vec<TableRow>,
    pub summary: Option<SynthesisSummary>,
    pub prisma: Option<PrismaOut>,
    /// Files written to /data/output.
    pub files: Vec<String>,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------- cleaning and summary

fn cell(s: &Option<String>, field: &str, problems: &mut Vec<String>, row: usize) -> Option<String> {
    let t = s
        .as_deref()?
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if t.is_empty() {
        return None;
    }
    if t.chars().count() > MAX_CELL {
        problems.push(format!(
            "study {row}: {field} is longer than {MAX_CELL} characters; shorten it"
        ));
        return None;
    }
    Some(t)
}

fn clean_rows(studies: &[SynthesisStudy]) -> Result<Vec<TableRow>> {
    if studies.len() > MAX_STUDIES {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_STUDIES} studies per table (you gave {})",
            studies.len()
        )));
    }
    let mut problems = Vec::new();
    let mut rows = Vec::new();
    for (i, s) in studies.iter().enumerate() {
        let n = i + 1;
        let citation = cell(&Some(s.citation.clone()), "citation", &mut problems, n);
        if citation.is_none() && s.citation.trim().is_empty() {
            problems.push(format!("study {n} needs a citation (how you refer to it)"));
        }
        if let Some(y) = s.year
            && !(1900..=2100).contains(&y)
        {
            problems.push(format!("study {n}: year {y} is outside 1900-2100"));
        }
        if let Some(k) = s.sample_size
            && !(0..=MAX_N).contains(&k)
        {
            problems.push(format!(
                "study {n}: sample_size {k} must be a whole number from 0 to {MAX_N}"
            ));
        }
        rows.push(TableRow {
            citation: citation.unwrap_or_default(),
            year: s.year,
            design: cell(&s.design, "design", &mut problems, n),
            population: cell(&s.population, "population", &mut problems, n),
            sample_size: s.sample_size,
            intervention: cell(&s.intervention, "intervention", &mut problems, n),
            comparator: cell(&s.comparator, "comparator", &mut problems, n),
            outcome: cell(&s.outcome, "outcome", &mut problems, n),
            effect: cell(&s.effect, "effect", &mut problems, n),
            quality_rating: cell(&s.quality_rating, "quality_rating", &mut problems, n),
            quality_tool: cell(&s.quality_tool, "quality_tool", &mut problems, n),
            identifier: cell(&s.identifier, "identifier", &mut problems, n),
            notes: cell(&s.notes, "notes", &mut problems, n),
        });
    }
    if !problems.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "The studies have errors:\n- {}",
            problems.join("\n- ")
        )));
    }
    Ok(rows)
}

fn counts_by<F: Fn(&TableRow) -> String>(rows: &[TableRow], f: F) -> Vec<(String, usize)> {
    let mut order: Vec<(String, String, usize)> = Vec::new(); // (lowercase key, first spelling, count)
    for r in rows {
        let label = f(r);
        let key = label.to_lowercase();
        match order.iter_mut().find(|(k, _, _)| *k == key) {
            Some(e) => e.2 += 1,
            None => order.push((key, label, 1)),
        }
    }
    order.into_iter().map(|(_, l, n)| (l, n)).collect()
}

fn summarize(rows: &[TableRow]) -> SynthesisSummary {
    let sizes: Vec<i64> = rows.iter().filter_map(|r| r.sample_size).collect();
    let mut sorted = sizes.clone();
    sorted.sort_unstable();
    let median = match sorted.len() {
        0 => None,
        n if n % 2 == 1 => Some(sorted[n / 2] as f64),
        n => Some((sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0),
    };
    let by_design = counts_by(rows, |r| {
        r.design.clone().unwrap_or_else(|| NOT_STATED.into())
    })
    .into_iter()
    .map(|(label, studies)| LabelCount { label, studies })
    .collect();
    // Rating and tool together, matched ignoring capitals.
    let mut quality: Vec<(String, String, usize)> = Vec::new();
    for r in rows {
        let tool = r.quality_tool.clone().unwrap_or_else(|| NOT_STATED.into());
        let rating = r.quality_rating.clone().unwrap_or_else(|| NOT_STATED.into());
        match quality
            .iter_mut()
            .find(|(t, g, _)| t.to_lowercase() == tool.to_lowercase() && g.to_lowercase() == rating.to_lowercase())
        {
            Some(e) => e.2 += 1,
            None => quality.push((tool, rating, 1)),
        }
    }
    SynthesisSummary {
        studies: rows.len(),
        studies_with_sample_size: sizes.len(),
        total_participants: sizes.iter().sum(),
        median_sample_size: median,
        by_design,
        by_quality: quality
            .into_iter()
            .map(|(quality_tool, quality_rating, studies)| QualityCount {
                quality_tool,
                quality_rating,
                studies,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- Excel

fn xerr(e: XlsxError) -> PhnError {
    PhnError::Internal(format!("XLSX: {e}"))
}

/// The workbook: the table, a summary sheet with live formulas, the PRISMA counts with live
/// checks, and notes.
fn workbook(
    question: &Option<String>,
    rows: &[TableRow],
    summary: &SynthesisSummary,
    flow: &Option<PrismaFlow>,
    notes: &[String],
) -> Result<Vec<u8>> {
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let wrap = Format::new().set_text_wrap().set_align(rust_xlsxwriter::FormatAlign::Top);
    let pct = Format::new().set_num_format("0.0%");
    let last = rows.len().max(1) + 1; // last data row (1-based) on the table sheet

    // ---- Synthesis table (only when studies were entered)
    let headers = [
        "Study",
        "Design",
        "Population and setting",
        "Sample size (n)",
        "Intervention or exposure",
        "Comparator",
        "Outcome",
        "Effect as reported",
        "Quality rating",
        "Rating tool",
        "Share of total n",
        "Year",
        "Identifier",
        "Notes",
    ];
    // Column letters: A study, B design, C population, D n, E intervention, F comparator,
    // G outcome, H effect, I rating, J tool, K share, L year, M identifier, N notes.
    if !rows.is_empty() {
        let ws = wb.add_worksheet();
        ws.set_name("Synthesis table").map_err(xerr)?;
        for (c, h) in headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, *h, &bold).map_err(xerr)?;
        }
        let total: i64 = rows.iter().filter_map(|r| r.sample_size).sum();
        for (i, r) in rows.iter().enumerate() {
            let rr = (i + 1) as u32;
            let excel_row = i + 2;
            ws.write_string_with_format(rr, 0, &r.citation, &wrap).map_err(xerr)?;
            let opt = |ws: &mut rust_xlsxwriter::Worksheet, c: u16, v: &Option<String>| -> Result<()> {
                if let Some(v) = v {
                    ws.write_string_with_format(rr, c, v, &wrap).map_err(xerr)?;
                }
                Ok(())
            };
            opt(ws, 1, &r.design)?;
            opt(ws, 2, &r.population)?;
            if let Some(n) = r.sample_size {
                ws.write_number(rr, 3, n as f64).map_err(xerr)?;
            }
            opt(ws, 4, &r.intervention)?;
            opt(ws, 5, &r.comparator)?;
            opt(ws, 6, &r.outcome)?;
            opt(ws, 7, &r.effect)?;
            opt(ws, 8, &r.quality_rating)?;
            opt(ws, 9, &r.quality_tool)?;
            let share = match r.sample_size {
                Some(n) if total > 0 => n as f64 / total as f64,
                _ => 0.0,
            };
            let f = Formula::new(format!(
                "=IF(OR(D{excel_row}=\"\",SUM($D$2:$D${last})=0),\"\",D{excel_row}/SUM($D$2:$D${last}))"
            ));
            let f = if r.sample_size.is_some() && total > 0 {
                f.set_result(format!("{share}"))
            } else {
                f.set_result("")
            };
            ws.write_formula_with_format(rr, 10, f, &pct).map_err(xerr)?;
            if let Some(y) = r.year {
                ws.write_number(rr, 11, y as f64).map_err(xerr)?;
            }
            opt(ws, 12, &r.identifier)?;
            opt(ws, 13, &r.notes)?;
        }
        ws.set_freeze_panes(1, 1).map_err(xerr)?;
        ws.autofilter(0, 0, rows.len().max(1) as u32, (headers.len() - 1) as u16)
            .map_err(xerr)?;
        for (c, w) in [28, 20, 32, 12, 32, 22, 28, 32, 18, 22, 12, 8, 22, 36].iter().enumerate() {
            ws.set_column_width(c as u16, *w as f64).map_err(xerr)?;
        }
    }

    // ---- Summary (live formulas over the table)
    if !rows.is_empty() {
        let ws = wb.add_worksheet();
        ws.set_name("Summary").map_err(xerr)?;
        ws.write_string_with_format(0, 0, "Summary (formulas read the Synthesis table sheet)", &bold).map_err(xerr)?;
        let t = "'Synthesis table'";
        let figures: [(&str, String, String); 4] = [
            (
                "Number of studies",
                format!("=COUNTA({t}!A2:A{last})"),
                summary.studies.to_string(),
            ),
            (
                "Studies reporting a sample size",
                format!("=COUNT({t}!D2:D{last})"),
                summary.studies_with_sample_size.to_string(),
            ),
            (
                "Total participants (sum of n)",
                format!("=SUM({t}!D2:D{last})"),
                summary.total_participants.to_string(),
            ),
            (
                "Median sample size",
                format!("=IF(COUNT({t}!D2:D{last})=0,\"\",MEDIAN({t}!D2:D{last}))"),
                summary.median_sample_size.map(|m| format!("{m}")).unwrap_or_default(),
            ),
        ];
        for (i, (label, f, result)) in figures.iter().enumerate() {
            let r = (i + 2) as u32;
            ws.write_string(r, 0, *label).map_err(xerr)?;
            ws.write_formula(r, 1, Formula::new(f.clone()).set_result(result.clone())).map_err(xerr)?;
        }
        let mut r = 7u32;
        ws.write_string_with_format(r, 0, "Studies by design", &bold).map_err(xerr)?;
        ws.write_string_with_format(r, 1, "Studies", &bold).map_err(xerr)?;
        for d in &summary.by_design {
            r += 1;
            ws.write_string(r, 0, &d.label).map_err(xerr)?;
            let a = r + 1;
            ws.write_formula(
                r,
                1,
                Formula::new(format!(
                    "=SUMPRODUCT(--(TRIM({t}!$B$2:$B${last})=IF($A{a}=\"{NOT_STATED}\",\"\",$A{a})))"
                ))
                .set_result(d.studies.to_string()),
            )
            .map_err(xerr)?;
        }
        r += 2;
        ws.write_string_with_format(r, 0, "Rating tool", &bold).map_err(xerr)?;
        ws.write_string_with_format(r, 1, "Quality rating", &bold).map_err(xerr)?;
        ws.write_string_with_format(r, 2, "Studies", &bold).map_err(xerr)?;
        for qc in &summary.by_quality {
            r += 1;
            ws.write_string(r, 0, &qc.quality_tool).map_err(xerr)?;
            ws.write_string(r, 1, &qc.quality_rating).map_err(xerr)?;
            let a = r + 1;
            ws.write_formula(
                r,
                2,
                Formula::new(format!(
                    "=SUMPRODUCT((TRIM({t}!$J$2:$J${last})=IF($A{a}=\"{NOT_STATED}\",\"\",$A{a}))*(TRIM({t}!$I$2:$I${last})=IF($B{a}=\"{NOT_STATED}\",\"\",$B{a})))"
                ))
                .set_result(qc.studies.to_string()),
            )
            .map_err(xerr)?;
        }
        ws.set_column_width(0, 36).map_err(xerr)?;
        ws.set_column_width(1, 24).map_err(xerr)?;
        ws.set_column_width(2, 12).map_err(xerr)?;
    }

    // ---- PRISMA counts with live checks
    if let Some(f) = flow {
        let ws = wb.add_worksheet();
        ws.set_name("PRISMA counts").map_err(xerr)?;
        for (c, h) in ["Count", "Entered (edit these)", "Calculated from the counts above", "Check"].iter().enumerate() {
            ws.write_string_with_format(0, c as u16, *h, &bold).map_err(xerr)?;
        }
        // Row numbers are 1-based Excel rows.
        let put = |ws: &mut rust_xlsxwriter::Worksheet,
                       row: u32,
                       label: &str,
                       entered: i64,
                       calc: Option<(String, i64)>|
         -> Result<()> {
            ws.write_string(row, 0, label).map_err(xerr)?;
            ws.write_number(row, 1, entered as f64).map_err(xerr)?;
            if let Some((formula, result)) = calc {
                let e = row + 1;
                ws.write_formula(row, 2, Formula::new(formula).set_result(result.to_string())).map_err(xerr)?;
                ws.write_formula(
                    row,
                    3,
                    Formula::new(format!("=IF(B{e}=C{e},\"OK\",\"CHECK\")")).set_result("OK"),
                )
                .map_err(xerr)?;
            }
            Ok(())
        };
        put(ws, 1, "Records identified", f.records_identified, None)?;
        put(ws, 2, "Duplicates removed", f.duplicates_removed, None)?;
        put(ws, 3, "Removed for other reasons", f.removed_other_reasons, None)?;
        put(ws, 4, "Records screened", f.records_screened, Some(("=B2-B3-B4".into(), f.records_screened)))?;
        put(ws, 5, "Records excluded after screening", f.records_excluded, None)?;
        put(ws, 6, "Reports sought for retrieval", f.reports_sought, Some(("=B5-B6".into(), f.reports_sought)))?;
        put(ws, 7, "Reports not retrieved", f.reports_not_retrieved, None)?;
        put(ws, 8, "Reports assessed for eligibility", f.reports_assessed, Some(("=B7-B8".into(), f.reports_assessed)))?;
        let mut row = 9u32;
        let first_reason = row + 1;
        for r in &f.reports_excluded {
            ws.write_string(row, 0, format!("Excluded: {}", r.reason)).map_err(xerr)?;
            ws.write_number(row, 1, r.reports as f64).map_err(xerr)?;
            row += 1;
        }
        let last_reason = row;
        let sum_f = if f.reports_excluded.is_empty() {
            "=0".to_string()
        } else {
            format!("=SUM(B{first_reason}:B{last_reason})")
        };
        ws.write_string(row, 0, "Reports excluded (total)").map_err(xerr)?;
        ws.write_formula(row, 1, Formula::new(sum_f).set_result(f.reports_excluded_total.to_string())).map_err(xerr)?;
        let total_row = row + 1;
        row += 1;
        put(
            ws,
            row,
            "Reports of included studies",
            f.reports_of_included_studies,
            Some((format!("=B9-B{total_row}"), f.reports_of_included_studies)),
        )?;
        let reports_row = row + 1;
        row += 1;
        ws.write_string(row, 0, "Studies included").map_err(xerr)?;
        ws.write_number(row, 1, f.studies_included as f64).map_err(xerr)?;
        let e = row + 1;
        ws.write_formula(
            row,
            3,
            Formula::new(format!("=IF(AND(B{e}>=0,B{e}<=B{reports_row}),\"OK\",\"CHECK\")")).set_result("OK"),
        )
        .map_err(xerr)?;
        ws.set_column_width(0, 40).map_err(xerr)?;
        ws.set_column_width(1, 22).map_err(xerr)?;
        ws.set_column_width(2, 32).map_err(xerr)?;
        ws.set_column_width(3, 10).map_err(xerr)?;
    }

    // ---- Notes
    {
        let ws = wb.add_worksheet();
        ws.set_name("Notes").map_err(xerr)?;
        ws.write_string_with_format(0, 0, question.as_deref().unwrap_or("Evidence synthesis table"), &bold).map_err(xerr)?;
        for (i, n) in notes.iter().enumerate() {
            ws.write_string_with_format((i + 2) as u32, 0, n, &wrap).map_err(xerr)?;
        }
        ws.set_column_width(0, 110).map_err(xerr)?;
    }
    wb.save_to_buffer().map_err(xerr)
}

// ---------------------------------------------------------------- Word

fn join_opt(parts: &[Option<&String>], sep: &str) -> String {
    parts
        .iter()
        .filter_map(|p| p.map(String::as_str))
        .collect::<Vec<_>>()
        .join(sep)
}

fn word_document(
    ctx: &Context,
    question: &Option<String>,
    rows: &[TableRow],
    summary: &SynthesisSummary,
    prisma: &Option<(&PrismaFlow, Vec<u8>, String, Vec<String>)>,
    notes: &[String],
) -> Document {
    let title = match question {
        Some(q) => format!("Evidence synthesis: {q}"),
        None => "Evidence synthesis".to_string(),
    };
    let p = |t: String| Block::Paragraph(vec![Inline::plain(t)]);
    let mut blocks = vec![p(format!(
        "Prepared {}. Every entry in this table was supplied by the person who appraised the studies; the tool that produced it did not look up, rate, or interpret any study.",
        chrono::Local::now().date_naive()
    ))];
    if let Some((_, png, alt, text_alt)) = prisma {
        blocks.push(Block::Heading {
            level: 2,
            text: "PRISMA 2020 flow diagram".into(),
        });
        blocks.push(Block::Image(ImageBlock {
            path: "prisma_flow.png".into(),
            alt: alt.clone(),
            data: Some(png.clone()),
        }));
        blocks.push(Block::Heading {
            level: 3,
            text: "Text alternative to the diagram".into(),
        });
        blocks.push(Block::List {
            ordered: false,
            items: text_alt.iter().map(|t| vec![Inline::plain(t.clone())]).collect(),
        });
    }
    if !rows.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            text: "Synthesis table".into(),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some("Appraised studies".into()),
            headers: [
                "Study",
                "Design and population (n)",
                "Intervention or exposure (comparator)",
                "Outcome",
                "Effect as reported",
                "Quality rating (tool)",
            ]
            .map(String::from)
            .to_vec(),
            rows: rows
                .iter()
                .map(|r| {
                    let study = match r.year {
                        Some(y) if !r.citation.contains(&y.to_string()) => format!("{} ({y})", r.citation),
                        _ => r.citation.clone(),
                    };
                    let n = r.sample_size.map(|n| format!("n = {n}"));
                    let design_pop = join_opt(
                        &[r.design.as_ref(), r.population.as_ref(), n.as_ref()],
                        "; ",
                    );
                    let comparator = r.comparator.as_ref().map(|c| format!("vs {c}"));
                    let interv = join_opt(&[r.intervention.as_ref(), comparator.as_ref()], " ");
                    let tool = r.quality_tool.as_ref().map(|t| format!("({t})"));
                    let quality = join_opt(&[r.quality_rating.as_ref(), tool.as_ref()], " ");
                    vec![
                        study,
                        design_pop,
                        interv,
                        r.outcome.clone().unwrap_or_default(),
                        r.effect.clone().unwrap_or_default(),
                        quality,
                    ]
                })
                .collect(),
        }));
        blocks.push(Block::Heading {
            level: 2,
            text: "Summary".into(),
        });
        let mut srows = vec![
            vec!["Number of studies".to_string(), summary.studies.to_string()],
            vec![
                "Studies reporting a sample size".into(),
                summary.studies_with_sample_size.to_string(),
            ],
            vec![
                "Total participants (sum of n)".into(),
                summary.total_participants.to_string(),
            ],
        ];
        if let Some(m) = summary.median_sample_size {
            srows.push(vec!["Median sample size".into(), md::num(m, 1)]);
        }
        blocks.push(Block::Table(TableBlock {
            caption: Some("Summary figures".into()),
            headers: vec!["Measure".into(), "Value".into()],
            rows: srows,
        }));
        blocks.push(Block::Table(TableBlock {
            caption: Some("Studies by design".into()),
            headers: vec!["Design".into(), "Studies".into()],
            rows: summary
                .by_design
                .iter()
                .map(|d| vec![d.label.clone(), d.studies.to_string()])
                .collect(),
        }));
        blocks.push(Block::Table(TableBlock {
            caption: Some("Studies by quality rating and rating tool".into()),
            headers: vec!["Rating tool".into(), "Quality rating".into(), "Studies".into()],
            rows: summary
                .by_quality
                .iter()
                .map(|q| {
                    vec![
                        q.quality_tool.clone(),
                        q.quality_rating.clone(),
                        q.studies.to_string(),
                    ]
                })
                .collect(),
        }));
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Notes".into(),
    });
    blocks.push(Block::List {
        ordered: false,
        items: notes.iter().map(|n| vec![Inline::plain(n.clone())]).collect(),
    });
    Document {
        title,
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

// ---------------------------------------------------------------- the tool

/// Build the synthesis table and PRISMA flow diagram.
pub fn build_evidence_synthesis_table(
    ctx: &Context,
    args: SynthesisArgs,
) -> Result<Envelope<SynthesisResult>> {
    let c: SynthesisContent = content::load(SYNTHESIS)?;
    if args.studies.is_empty() && args.prisma.is_none() {
        return Err(PhnError::InvalidInput(
            "give the studies you appraised, PRISMA counts, or both".into(),
        ));
    }
    let question = args
        .question
        .as_deref()
        .map(|q| q.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|q| !q.is_empty())
        .map(|q| q.chars().take(200).collect::<String>());
    let rows = clean_rows(&args.studies)?;
    // The counts are checked before anything is written.
    let flow = match &args.prisma {
        Some(counts) => Some(prisma::validate(counts)?),
        None => None,
    };
    let summary = (!rows.is_empty()).then(|| summarize(&rows));
    let diagram_title = match &question {
        Some(q) => format!("PRISMA 2020 flow diagram: {q}"),
        None => "PRISMA 2020 flow diagram".to_string(),
    };
    let prisma_out = flow.as_ref().map(|f| {
        let text_alt = prisma::text_alternative(f);
        PrismaOut {
            flow: f.clone(),
            mermaid: prisma::mermaid(f, &diagram_title),
            svg: prisma::svg(f, &diagram_title),
            alt_text: format!(
                "PRISMA 2020 flow diagram: {} records identified, {} studies included. The text alternative lists every count.",
                f.records_identified, f.studies_included
            ),
            text_alternative: text_alt,
            arithmetic: c.flow.arithmetic.clone(),
            not_drawn: c.flow.not_drawn.clone(),
        }
    });

    // Caveats and notes (also written into the files).
    let mut notes: Vec<String> = vec![
        "The table holds only what you entered. The tool does not look up, summarise, rate, or interpret studies, and it does not format references.".into(),
        c.table.quality_scales_note.split_whitespace().collect::<Vec<_>>().join(" "),
    ];
    if let Some(s) = &summary {
        notes.push(format!(
            "Total participants adds the sample sizes you entered ({} of {} studies gave one); people in more than one study are counted more than once.",
            s.studies_with_sample_size, s.studies
        ));
    }
    if let Some(f) = &flow {
        let d = |t: &str| c.definitions.iter().find(|d| d.term == t).map(|d| d.text.clone()).unwrap_or_default();
        notes.push(format!(
            "PRISMA 2020 flow elements: {} (PRISMA 2020 explanation and elaboration, item 16a). Record: {} Report: {} Study: {}",
            c.flow.statement.split_whitespace().collect::<Vec<_>>().join(" "),
            d("Record"),
            d("Report"),
            d("Study")
        ));
        notes.push(format!(
            "The diagram follows the text of item 16a, not the official figure (which could not be read when this tool was built), so its layout is this project's. Not drawn: {}. Use the official template at the PRISMA statement website for a publication.",
            c.flow.not_drawn.join("; ")
        ));
        if !f.defaults_used.is_empty() {
            notes.push(format!(
                "Numbers you did not enter were derived or defaulted: {}.",
                f.defaults_used.join("; ")
            ));
        }
        if !rows.is_empty() && rows.len() as i64 != f.studies_included {
            notes.push(format!(
                "The table has {} studies but the PRISMA counts say {} were included; check that they match.",
                rows.len(),
                f.studies_included
            ));
        }
    }
    let mut caveats = notes.clone();
    caveats.extend(content::review_caveats(&[SYNTHESIS]));
    let mut sources: Vec<Source> = Vec::new();
    if flow.is_some() {
        for k in ["prisma_2020", "prisma_ee"] {
            if let Some(s) = c.sources.get(k) {
                sources.push(Source {
                    title: format!("{} (licence: {})", s.title, s.licence),
                    publisher: s.publisher.clone(),
                    url: Some(s.url.clone()),
                    effective_date: s.effective_date.clone(),
                    retrieved: Some(s.retrieved.clone()),
                    mode: phn_core::SourceMode::Snapshot,
                });
            }
        }
    }
    sources.push(Source::computed(
        "Synthesis table and summary computed from the studies you entered",
        "Public Health Nursing MCP Server",
    ));

    // Files.
    let write = args.write_files.unwrap_or(true);
    let format = args.format.unwrap_or_default();
    let mut files = Vec::new();
    if write {
        let (dir, _) = templates::output_folder(ctx, "evidence_synthesis")?;
        if matches!(format, SynthesisFormat::Both | SynthesisFormat::Xlsx) {
            let empty_summary = summarize(&[]);
            let summary_ref = summary.as_ref().unwrap_or(&empty_summary);
            let bytes = workbook(&question, &rows, summary_ref, &flow, &notes)?;
            files.push(templates::write_in(ctx, &dir, "synthesis_table.xlsx", &bytes)?);
        }
        if matches!(format, SynthesisFormat::Both | SynthesisFormat::Docx) {
            let empty_summary = summarize(&[]);
            let summary_ref = summary.as_ref().unwrap_or(&empty_summary);
            let png = match &flow {
                Some(f) => Some(phn_docs::svg::svg_to_png(&prisma::svg(f, &diagram_title), 2.0)?),
                None => None,
            };
            let prisma_doc = match (&flow, png, &prisma_out) {
                (Some(f), Some(png), Some(po)) => {
                    Some((f, png, po.alt_text.clone(), po.text_alternative.clone()))
                }
                _ => None,
            };
            let doc = word_document(ctx, &question, &rows, summary_ref, &prisma_doc, &notes);
            let bytes = phn_docs::docx::to_docx(&doc)?;
            files.push(templates::write_in(ctx, &dir, "synthesis_table.docx", &bytes)?);
        }
        if let Some(po) = &prisma_out {
            files.push(templates::write_in(ctx, &dir, "prisma_flow.svg", po.svg.as_bytes())?);
            files.push(templates::write_in(ctx, &dir, "prisma_flow.mmd", po.mermaid.as_bytes())?);
        }
    }

    // Summary text.
    let mut s = format!(
        "**{}**\n\n",
        question.clone().unwrap_or_else(|| "Evidence synthesis".into())
    );
    if let Some(sm) = &summary {
        s.push_str(&format!(
            "{} studies; {} with a sample size (total {} participants{}).\n\n",
            sm.studies,
            sm.studies_with_sample_size,
            md::num(sm.total_participants as f64, 0),
            sm.median_sample_size
                .map(|m| format!(", median {}", md::num(m, 1)))
                .unwrap_or_default()
        ));
        let table_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|r| {
                vec![
                    r.citation.clone(),
                    r.design.clone().unwrap_or_default(),
                    r.sample_size.map(|n| n.to_string()).unwrap_or_default(),
                    r.outcome.clone().unwrap_or_default(),
                    r.effect.clone().unwrap_or_default(),
                    join_opt(&[r.quality_rating.as_ref(), r.quality_tool.as_ref()], ", "),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Study", "Design", "n", "Outcome", "Effect", "Quality"],
            &table_rows,
        ));
        s.push_str("\nBy design: ");
        s.push_str(
            &sm.by_design
                .iter()
                .map(|d| format!("{} ({})", d.label, d.studies))
                .collect::<Vec<_>>()
                .join("; "),
        );
        s.push_str("\n\nBy quality rating: ");
        s.push_str(
            &sm.by_quality
                .iter()
                .map(|q| format!("{} ({}) {}", q.quality_rating, q.quality_tool, q.studies))
                .collect::<Vec<_>>()
                .join("; "),
        );
        s.push_str("\n\n");
    }
    if let Some(po) = &prisma_out {
        s.push_str("**PRISMA 2020 flow (counts check out)**\n\n");
        for l in &po.text_alternative {
            s.push_str(&format!("- {l}\n"));
        }
        s.push_str("\nMermaid text and an accessible SVG are in the result.\n");
    }
    if !files.is_empty() {
        s.push_str(&format!("\nFiles written: {}.\n", files.join(", ")));
    }

    Ok(Envelope::new(
        s,
        SynthesisResult {
            question,
            rows,
            summary,
            prisma: prisma_out,
            files: files.clone(),
            notes,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        files_written: files,
        notes: vec![
            "Nothing was read from local files, and nothing was sent over the network; the tables hold only what you typed."
                .into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn study(cite: &str, design: &str, n: Option<i64>, rating: &str, tool: &str) -> SynthesisStudy {
        SynthesisStudy {
            citation: cite.into(),
            design: Some(design.into()),
            sample_size: n,
            quality_rating: Some(rating.into()),
            quality_tool: Some(tool.into()),
            outcome: Some("Visits".into()),
            ..SynthesisStudy::default()
        }
    }

    #[test]
    fn summary_matches_independent_arithmetic() {
        let rows = clean_rows(&[
            study("A 2019", "Randomized trial", Some(120), "Low", "Cochrane 2011"),
            study("B 2020", "randomized trial", Some(80), "Unclear", "Cochrane 2011"),
            study("C 2021", "Cohort", Some(1000), "Moderate", "ROBINS-I"),
            study("D 2022", "Cohort", None, "moderate", "robins-i"),
            SynthesisStudy {
                citation: "E 2023".into(),
                ..SynthesisStudy::default()
            },
        ])
        .unwrap();
        let s = summarize(&rows);
        assert_eq!(s.studies, 5);
        assert_eq!(s.studies_with_sample_size, 3);
        assert_eq!(s.total_participants, 1200);
        assert_eq!(s.median_sample_size, Some(120.0));
        // Capitals are ignored when grouping; the first spelling is shown.
        assert_eq!(
            s.by_design,
            vec![
                LabelCount { label: "Randomized trial".into(), studies: 2 },
                LabelCount { label: "Cohort".into(), studies: 2 },
                LabelCount { label: NOT_STATED.into(), studies: 1 },
            ]
        );
        assert_eq!(s.by_quality.len(), 4);
        assert_eq!(s.by_quality[2], QualityCount {
            quality_tool: "ROBINS-I".into(),
            quality_rating: "Moderate".into(),
            studies: 2
        });
        // The median of an even count is the mean of the middle two.
        let even = summarize(&clean_rows(&[
            study("A", "x", Some(10), "a", "t"),
            study("B", "x", Some(20), "a", "t"),
        ]).unwrap());
        assert_eq!(even.median_sample_size, Some(15.0));
        assert_eq!(summarize(&[]).median_sample_size, None);
    }

    #[test]
    fn bad_studies_are_tool_errors() {
        let mut bad = study("", "x", Some(-5), "a", "t");
        bad.year = Some(1850);
        let e = clean_rows(&[bad]).unwrap_err().to_string();
        assert!(e.contains("needs a citation") && e.contains("1850") && e.contains("sample_size -5"), "{e}");
        let long = SynthesisStudy {
            citation: "A".into(),
            effect: Some("x".repeat(601)),
            ..SynthesisStudy::default()
        };
        assert!(clean_rows(&[long]).unwrap_err().to_string().contains("effect is longer"));
        let many: Vec<SynthesisStudy> = (0..301).map(|i| study(&format!("S{i}"), "x", None, "a", "t")).collect();
        assert!(clean_rows(&many).is_err());
    }
}
