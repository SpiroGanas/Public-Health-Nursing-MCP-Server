//! PLAN-09 `build_program_budget`: a line-item program budget in federal object classes
//! (personnel, fringe benefits, travel, equipment, supplies, contractual, other, indirect) with
//! multi-year escalation, cost share and a match check, and indirect costs on the right base
//! (the de minimis rate of up to 15 percent of MTDC under 2 CFR 200.414(f), or a negotiated
//! rate). It writes an XLSX workbook with live formulas and a DOCX budget justification drafted
//! from the line items to /data/output; the response returns totals and structure only.
//!
//! Content is `content/planning/budget_reference.yaml` (pending review). The arithmetic is in
//! [`calc`]; its golden tests compare with an independent Python computation, and the workbook's
//! formulas are recalculated in LibreOffice and compared with the same computation (see
//! `crates/phn-tools/tests/fixtures/generators/`).

pub mod calc;
pub mod input;
pub mod narrative;
pub mod workbook;

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use self::calc::{
    Block, Category, Computed, Funding, IndirectBase, MatchBasis, Rates, compute, round2,
};
pub use self::input::LineItemArg;
use crate::context::{Context, error_message};
use crate::epr::content::{self as econtent, Cited, CitedOut, DocSource};
use crate::epr::roster;
use crate::md;

/// Content id of `content/planning/budget_reference.yaml`.
pub const BUDGET_CONTENT: &str = "planning-budget-reference";

pub(crate) use self::IndirectMethod as IndirectMethodUsed;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct DeMinimis {
    rate_percent: f64,
    text: String,
    note: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Rules {
    de_minimis: DeMinimis,
    subaward_cap_in_mtdc: f64,
    equipment_threshold: f64,
    mtdc: Cited,
    equipment: Cited,
    participant_support: Cited,
    subaward_vs_contract: Cited,
    fringe: Cited,
    cost_sharing: Vec<Cited>,
}

#[derive(Debug, Clone, Deserialize)]
struct ObjectClass {
    label: String,
    sf424a_line: String,
    what: String,
    source: String,
    section: String,
    confidence: econtent::Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct BudgetContent {
    meta: ContentMeta,
    sources: BTreeMap<String, DocSource>,
    rules: Rules,
    object_classes: Vec<ObjectClass>,
    funder_checks: Vec<Cited>,
    narrative: BTreeMap<String, String>,
}

// ---------------------------------------------------------------- arguments

/// How indirect costs are charged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IndirectMethod {
    /// The de minimis rate (2 CFR 200.414(f)): up to 15 percent of MTDC, for an organization
    /// with no current federal negotiated rate.
    #[default]
    DeMinimis,
    /// Your federally negotiated rate (give `rate_percent` and the `base` in the agreement).
    Negotiated,
    /// No indirect costs (the funder does not allow them, or you waive them).
    None,
}

/// Indirect cost settings.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct IndirectArg {
    /// `de_minimis` (default), `negotiated`, or `none`.
    pub method: IndirectMethod,
    /// Rate in percent. De minimis: 15 unless you give a lower rate (a higher one is refused).
    /// Negotiated: required.
    pub rate_percent: Option<f64>,
    /// Base the rate applies to: `mtdc` (default), `total_direct`, `salaries_and_wages`, or
    /// `salaries_and_fringe`. The de minimis rate applies to MTDC only.
    pub base: Option<IndirectBase>,
    /// Also charge indirect costs on the cost share lines (default false; unrecovered indirect
    /// counts as cost share only with the funder's prior approval).
    pub charge_on_cost_share: Option<bool>,
}

/// A required cost share or match.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RequiredCostShare {
    /// The required percent (20 for 20 percent).
    pub percent: f64,
    /// `total_project` (the percent of the project total, federal plus cost share) or
    /// `federal_request` (a match ratio on the federal request). Read the funding notice.
    pub basis: MatchBasis,
}

/// Arguments for `build_program_budget`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct BuildBudgetArgs {
    /// Name of the project (files and the justification are named from it).
    pub project_name: Option<String>,
    /// Your organization's name for the justification (default: the agency name saved with
    /// your letterhead, or a placeholder).
    pub organization: Option<String>,
    /// Budget years, 1 to 5. Default: the last year any line names, or 1.
    pub years: Option<u32>,
    /// Line items typed into the request (up to 150). Salary figures typed here pass through the
    /// AI assistant; to keep them local, put the lines in a file and use `line_items_path`.
    pub line_items: Vec<LineItemArg>,
    /// A CSV/XLSX in /data with one row per cost: category, item (or role), and for personnel
    /// annual salary and effort percent (or FTE), for other lines quantity and unit cost; optional
    /// funding, unit, fringe rate, escalate, first year, last year, subaward, MTDC exclusion.
    /// Read locally; a file with a name, phone, or other identifier column is refused, and no
    /// cell is returned.
    pub line_items_path: Option<String>,
    /// Sheet name when `line_items_path` is a workbook.
    pub sheet: Option<String>,
    /// Default fringe benefit rate in percent of salary (required when there are personnel lines
    /// without their own rate).
    pub fringe_rate_percent: Option<f64>,
    /// Yearly raise for personnel lines in percent (default 0: level salaries).
    pub salary_escalation_percent: Option<f64>,
    /// Yearly price increase for other lines that escalate, in percent (default 0).
    pub other_escalation_percent: Option<f64>,
    /// Indirect costs (default: de minimis rate on MTDC).
    pub indirect: Option<IndirectArg>,
    /// A required cost share or match, to check the budget against.
    pub required_cost_share: Option<RequiredCostShare>,
    /// Write the XLSX workbook with live formulas to /data/output (default true).
    pub write_workbook: Option<bool>,
    /// Write the DOCX budget justification to /data/output (default true when there are lines).
    pub write_justification: Option<bool>,
}

// ---------------------------------------------------------------- results

/// One year's (or the total's) figures, by object class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct YearOut {
    /// "Year 1", ..., or "Total".
    pub label: String,
    pub personnel: f64,
    pub fringe_benefits: f64,
    pub travel: f64,
    pub equipment: f64,
    pub supplies: f64,
    pub contractual: f64,
    pub other: f64,
    pub total_direct: f64,
    /// The amount indirect costs are charged on.
    pub indirect_base: f64,
    pub indirect: f64,
    pub total: f64,
}

/// Figures for one funding source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlockOut {
    pub years: Vec<YearOut>,
    pub total: YearOut,
}

/// The settings that were used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SettingsOut {
    pub years: usize,
    pub default_fringe_rate_percent: f64,
    pub salary_escalation_percent: f64,
    pub other_escalation_percent: f64,
    pub indirect_method: IndirectMethod,
    pub indirect_rate_percent: f64,
    pub indirect_base: String,
    pub indirect_on_cost_share: bool,
    /// Dollars of each subaward counted in the MTDC base (2 CFR 200.1).
    pub subaward_cap_in_mtdc: f64,
    /// Per-unit cost at which an item is equipment rather than supplies (2 CFR 200.1).
    pub equipment_threshold: f64,
}

/// Number of lines in an object class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LineCount {
    pub category: String,
    pub federal_lines: usize,
    pub cost_share_lines: usize,
}

/// The cost share check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MatchOut {
    pub required_percent: f64,
    pub basis: String,
    pub required_amount: f64,
    pub budgeted_cost_share: f64,
    pub shortfall: f64,
    pub met: bool,
}

/// An SF-424A style object class with its status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectClassOut {
    pub label: String,
    pub sf424a_line: String,
    pub what_goes_here: String,
    pub citation: String,
    pub confidence: econtent::Confidence,
}

/// Result of `build_program_budget`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BudgetResult {
    pub project: String,
    pub settings: SettingsOut,
    pub line_counts: Vec<LineCount>,
    /// The federal request by year (category totals only; no individual salaries or lines).
    pub federal_request: BlockOut,
    pub cost_share: BlockOut,
    pub project_total: BlockOut,
    pub cost_share_check: Option<MatchOut>,
    /// True when an independent re-add of every line agrees with the category totals.
    pub arithmetic_check_passed: bool,
    pub workbook: Option<String>,
    pub justification: Option<String>,
    /// Number of `[NURSE: ...]` placeholders left in the justification to fill in.
    pub justification_placeholders: usize,
    pub rules_used: Vec<CitedOut>,
    pub object_classes: Vec<ObjectClassOut>,
    pub funder_checks: Vec<CitedOut>,
    pub next_steps: Vec<String>,
}

// ---------------------------------------------------------------- helpers

pub(crate) fn fmt_money(v: f64) -> String {
    let neg = v < 0.0;
    let cents = (v.abs() * 100.0).round() as u64;
    let (dollars, c) = (cents / 100, cents % 100);
    let s = dollars.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{}${out}.{c:02}", if neg { "-" } else { "" })
}

/// A fraction as a percent without trailing zeros: 0.125 -> "12.5%".
pub(crate) fn fmt_pct(fraction: f64) -> String {
    let s = format!("{:.2}", fraction * 100.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}%")
}

fn year_out(label: &str, b: &Block, k: Option<usize>) -> YearOut {
    let pick = |v: &[f64]| -> f64 {
        match k {
            Some(i) => round2(v[i]),
            None => round2(v.iter().sum()),
        }
    };
    YearOut {
        label: label.to_string(),
        personnel: pick(&b.personnel),
        fringe_benefits: pick(&b.fringe),
        travel: pick(&b.travel),
        equipment: pick(&b.equipment),
        supplies: pick(&b.supplies),
        contractual: pick(&b.contractual),
        other: pick(&b.other),
        total_direct: pick(&b.direct),
        indirect_base: pick(&b.indirect_base),
        indirect: pick(&b.indirect),
        total: pick(&b.total),
    }
}

fn block_out(b: &Block, n: usize) -> BlockOut {
    BlockOut {
        years: (0..n)
            .map(|k| year_out(&format!("Year {}", k + 1), b, Some(k)))
            .collect(),
        total: year_out("Total", b, None),
    }
}

/// Fields that hold file paths or column names, which are not scanned for contact details.
const SKIP_KEYS: &[&str] = &["line_items_path", "sheet"];

const MAX_NAME: usize = 120;

fn clean(v: &Option<String>, field: &str) -> Result<Option<String>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) if s.chars().count() > MAX_NAME => Err(PhnError::InvalidInput(format!(
            "{field} is longer than {MAX_NAME} characters"
        ))),
        Some(s) => Ok(Some(s.to_string())),
    }
}

fn stem(project: &str) -> String {
    let base: String = project.chars().take(40).collect();
    phn_local::sandbox::sanitize_file_name(&format!("budget_{base}"))
}

// ---------------------------------------------------------------- the tool

/// Build a program budget.
pub fn build_program_budget(
    ctx: &Context,
    args: BuildBudgetArgs,
) -> Result<Envelope<BudgetResult>> {
    let c: BudgetContent = econtent::load(BUDGET_CONTENT)?;
    let project =
        clean(&args.project_name, "project_name")?.unwrap_or_else(|| "Program budget".into());
    let organization = clean(&args.organization, "organization")?.or_else(|| {
        ctx.settings()
            .letterhead
            .map(|l| l.agency_name)
            .filter(|s| !s.trim().is_empty())
    });
    if args.line_items.len() > input::MAX_LINES {
        return Err(PhnError::InvalidInput(format!(
            "at most {} line items may be typed into a request; use line_items_path for more",
            input::MAX_LINES
        )));
    }
    if !args.line_items.is_empty() && args.line_items_path.is_some() {
        return Err(PhnError::InvalidInput(
            "give line_items or line_items_path, not both".into(),
        ));
    }
    let years_arg = match args.years {
        None => None,
        Some(y) if (1..=calc::MAX_YEARS as u32).contains(&y) => Some(y as usize),
        Some(_) => {
            return Err(PhnError::InvalidInput(format!(
                "years must be from 1 to {}",
                calc::MAX_YEARS
            )));
        }
    };

    // ---- lines
    let mut local_source: Option<Source> = None;
    let parsed = if let Some(p) = &args.line_items_path {
        let f = input::from_file(ctx, p, args.sheet.as_deref(), years_arg)?;
        local_source = Some(f.source);
        Some(f.parsed)
    } else if !args.line_items.is_empty() {
        Some(input::from_args(&args.line_items, years_arg)?)
    } else {
        None
    };
    let n = years_arg.unwrap_or_else(|| {
        parsed
            .as_ref()
            .map(|p| p.max_named_year.max(1))
            .unwrap_or(1)
    });
    let lines = parsed.map(|p| p.lines).unwrap_or_default();
    let blank = lines.is_empty();

    // ---- rates
    let pct = |v: Option<f64>, name: &str, lo: f64, hi: f64| -> Result<f64> {
        match v {
            None => Ok(0.0),
            Some(x) if x.is_finite() && (lo..=hi).contains(&x) => Ok(x / 100.0),
            Some(_) => Err(PhnError::InvalidInput(format!(
                "{name} must be between {lo} and {hi} percent"
            ))),
        }
    };
    let needs_fringe = lines
        .iter()
        .any(|l| l.category == Category::Personnel && l.fringe.is_none());
    if needs_fringe && args.fringe_rate_percent.is_none() {
        return Err(PhnError::InvalidInput(
            "personnel lines need a fringe rate: give fringe_rate_percent (your organization's \
             written rate, for example from your finance office), or a fringe_rate_percent on each \
             personnel line"
                .into(),
        ));
    }
    let fringe = pct(args.fringe_rate_percent, "fringe_rate_percent", 0.0, 100.0)?;
    let salary_escalation = pct(
        args.salary_escalation_percent,
        "salary_escalation_percent",
        -50.0,
        100.0,
    )?;
    let other_escalation = pct(
        args.other_escalation_percent,
        "other_escalation_percent",
        -50.0,
        100.0,
    )?;
    let ind = args.indirect.clone().unwrap_or_default();
    let de_min = &c.rules.de_minimis;
    let (indirect_rate, indirect_base) = match ind.method {
        IndirectMethod::DeMinimis => {
            let r = ind.rate_percent.unwrap_or(de_min.rate_percent);
            if !r.is_finite() || r < 0.0 || r > de_min.rate_percent {
                return Err(PhnError::InvalidInput(format!(
                    "the de minimis rate can be at most {}% of MTDC ({}); use method negotiated if your organization has a negotiated rate",
                    de_min.rate_percent, de_min.section
                )));
            }
            if ind.base.is_some_and(|b| b != IndirectBase::Mtdc) {
                return Err(PhnError::InvalidInput(
                    "the de minimis rate applies to modified total direct costs (base mtdc)".into(),
                ));
            }
            (r / 100.0, IndirectBase::Mtdc)
        }
        IndirectMethod::Negotiated => {
            let Some(r) = ind.rate_percent else {
                return Err(PhnError::InvalidInput(
                    "a negotiated indirect rate needs rate_percent and, if it is not MTDC, the base from your rate agreement".into(),
                ));
            };
            (
                pct(Some(r), "rate_percent", 0.0, 100.0)?,
                ind.base.unwrap_or(IndirectBase::Mtdc),
            )
        }
        IndirectMethod::None => (0.0, ind.base.unwrap_or(IndirectBase::Mtdc)),
    };
    let rates = Rates {
        years: n,
        salary_escalation,
        other_escalation,
        fringe,
        indirect_rate,
        indirect_base,
        indirect_on_cost_share: ind.charge_on_cost_share.unwrap_or(false),
        subaward_cap: c.rules.subaward_cap_in_mtdc,
    };
    let req = args
        .required_cost_share
        .as_ref()
        .map(|r| (r.percent, r.basis));
    let computed: Computed = compute(&lines, &rates, req)?;
    let arithmetic_ok = computed.self_check(&lines);
    if !arithmetic_ok {
        return Err(PhnError::Internal(
            "the budget arithmetic self-check failed; no files were written".into(),
        ));
    }

    // ---- structure for the response
    let count = |cat: Category, f: Funding| {
        lines
            .iter()
            .filter(|l| l.category == cat && l.funding == f)
            .count()
    };
    let line_counts: Vec<LineCount> = Category::ALL
        .iter()
        .map(|cat| LineCount {
            category: cat.label().to_string(),
            federal_lines: count(*cat, Funding::Federal),
            cost_share_lines: count(*cat, Funding::CostShare),
        })
        .collect();
    let federal_out = block_out(&computed.federal, n);
    let share_out = block_out(&computed.cost_share, n);
    let project_blk = workbook::project_block(&computed.federal, &computed.cost_share);
    let project_out = block_out(&project_blk, n);
    let match_out = computed.match_check.as_ref().map(|m| MatchOut {
        required_percent: m.percent,
        basis: match m.basis {
            MatchBasis::TotalProject => "percent of total project cost".into(),
            MatchBasis::FederalRequest => "percent of the federal request".into(),
        },
        required_amount: round2(m.required),
        budgeted_cost_share: round2(m.budgeted),
        shortfall: round2(m.shortfall),
        met: m.shortfall <= 0.005,
    });
    let method_label = match ind.method {
        IndirectMethod::DeMinimis => "De minimis (2 CFR 200.414(f))",
        IndirectMethod::Negotiated => "Negotiated rate",
        IndirectMethod::None => "None",
    };

    // ---- files
    let mut caveats: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut workbook_path = None;
    let mut justification_path = None;
    let mut placeholders = 0usize;
    let want_workbook = args.write_workbook.unwrap_or(true);
    let want_doc = args.write_justification.unwrap_or(!blank);
    if want_doc && blank {
        return Err(PhnError::InvalidInput(
            "the justification is drafted from line items: give line_items or line_items_path"
                .into(),
        ));
    }
    if want_workbook || want_doc {
        match ctx.sandbox() {
            Err(e) => caveats.push(format!(
                "No files were written: the output folder needs the data folder mounted at /data ({}).",
                error_message(&e)
            )),
            Ok(sb) => {
                let stem = stem(&project);
                if want_workbook {
                    let read_me = read_me_lines(&c, &rates, blank);
                    let bytes = workbook::build(&workbook::WorkbookInput {
                        title: &project,
                        lines: &lines,
                        rates: &rates,
                        method_label,
                        required_match: req,
                        computed: &computed,
                        read_me: &read_me,
                    })?;
                    let p = sb.new_output_file(&format!("{stem}.xlsx"))?;
                    std::fs::write(&p, bytes)?;
                    let shown = sb.display_path(&p);
                    files.push(shown.clone());
                    workbook_path = Some(shown);
                }
                if want_doc {
                    let req_text = computed.match_check.as_ref().map(|m| {
                        format!(
                            "The funder requires cost share of {}% of {}: {} is required and {} is budgeted{}",
                            m.percent,
                            match m.basis {
                                MatchBasis::TotalProject => "the total project cost",
                                MatchBasis::FederalRequest => "the federal request",
                            },
                            fmt_money(m.required),
                            fmt_money(m.budgeted),
                            if m.shortfall > 0.005 {
                                format!("; the budget is {} short. [NURSE: add cost share or confirm the requirement.]", fmt_money(m.shortfall))
                            } else {
                                ".".to_string()
                            }
                        )
                    });
                    let md_text = narrative::markdown(&narrative::NarrativeInput {
                        project: &project,
                        organization: organization.as_deref(),
                        lines: &lines,
                        rates: &rates,
                        method: ind.method,
                        computed: &computed,
                        template: &c.narrative,
                        required_match_text: req_text,
                    });
                    placeholders = narrative::placeholder_count(&md_text);
                    let doc = crate::school::templates::document(ctx, &md_text, "en-US", &project);
                    let bytes = phn_docs::docx::to_docx(&doc)?;
                    let p = sb.new_output_file(&format!("{stem}_justification.docx"))?;
                    std::fs::write(&p, bytes)?;
                    let shown = sb.display_path(&p);
                    files.push(shown.clone());
                    justification_path = Some(shown);
                }
            }
        }
    }

    // ---- summary, caveats, sources, privacy
    let fed = &federal_out.total;
    let mut s = format!("**{project}**: ");
    if blank {
        s.push_str("no line items were given, so a blank workbook with live formulas and spare rows was written.\n\n");
    } else {
        s.push_str(&format!(
            "{n}-year budget, federal request **{}** (direct {}, indirect {}); cost share {}; project total {}.\n\n",
            fmt_money(fed.total),
            fmt_money(fed.total_direct),
            fmt_money(fed.indirect),
            fmt_money(share_out.total.total),
            fmt_money(project_out.total.total),
        ));
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut add = |label: &str, f: &dyn Fn(&YearOut) -> f64| {
            let mut r = vec![label.to_string()];
            r.extend(federal_out.years.iter().map(|y| fmt_money(f(y))));
            r.push(fmt_money(f(&federal_out.total)));
            rows.push(r);
        };
        add("Personnel", &|y| y.personnel);
        add("Fringe benefits", &|y| y.fringe_benefits);
        add("Travel", &|y| y.travel);
        add("Equipment", &|y| y.equipment);
        add("Supplies", &|y| y.supplies);
        add("Contractual", &|y| y.contractual);
        add("Other direct", &|y| y.other);
        add("Total direct", &|y| y.total_direct);
        add("Indirect", &|y| y.indirect);
        add("Total federal request", &|y| y.total);
        let mut headers: Vec<String> = vec!["Federal request".into()];
        headers.extend((1..=n).map(|y| format!("Year {y}")));
        headers.push("Total".into());
        let h: Vec<&str> = headers.iter().map(String::as_str).collect();
        s.push_str(&md::table(&h, &rows));
        if let Some(m) = &match_out {
            s.push_str(&format!(
                "\nCost share check: {} required, {} budgeted{}.\n",
                fmt_money(m.required_amount),
                fmt_money(m.budgeted_cost_share),
                if m.met {
                    " (requirement met)".to_string()
                } else {
                    format!(" (short by {})", fmt_money(m.shortfall))
                }
            ));
        }
    }
    if let Some(w) = &workbook_path {
        s.push_str(&format!("\nWorkbook (live formulas): `{w}`"));
    }
    if let Some(j) = &justification_path {
        s.push_str(&format!(
            "\nJustification draft: `{j}` ({placeholders} `[NURSE: ...]` placeholders to fill in)"
        ));
    }

    caveats.push(format!(
        "Indirect costs: {}",
        match ind.method {
            IndirectMethod::DeMinimis => format!(
                "{} {} (as published in the eCFR, current through 2026-11-09; the rate was 10 percent before the October 1, 2024 revision).",
                de_min.text.split_whitespace().collect::<Vec<_>>().join(" "),
                de_min.note.split_whitespace().collect::<Vec<_>>().join(" ")
            ),
            IndirectMethod::Negotiated => "a negotiated rate must be applied to the base in your rate agreement; confirm the base and the agreement dates with your finance office.".into(),
            IndirectMethod::None => "none requested.".into(),
        }
    ));
    caveats.push(
        "Your funder's notice of funding opportunity controls: it may cap indirect costs, limit salaries, or define cost share differently. This is decision support, not a certified budget.".into(),
    );
    caveats.push(
        "The SF-424A line letters (6a to 6k) are low confidence: the form and its instructions could not be read. Check them against the form your funder uses.".into(),
    );
    if lines
        .iter()
        .filter(|l| l.category == Category::Personnel && l.funding == Funding::Federal)
        .count()
        == 1
    {
        caveats.push(
            "The personnel total here is one position's cost. Salary details are written only to the output files, but this total reflects them; do not share this response beyond people who may see that figure.".into(),
        );
    }
    if n > 1 && salary_escalation == 0.0 && other_escalation == 0.0 {
        caveats.push(
            "No yearly escalation was applied (amounts are level across years). Set salary_escalation_percent and other_escalation_percent if your funder expects them, and say how you chose them in the justification.".into(),
        );
    }
    if !blank && !lines.iter().any(|l| l.funding == Funding::CostShare) && match_out.is_some() {
        caveats.push("A cost share is required but no line is marked cost_share, so the check shows the full requirement as a shortfall.".into());
    }
    caveats.push(
        "Rates are percentages typed as 15 for 15 percent. Amounts are rounded to the cent line by line (ROUND in the workbook), so the workbook and this response agree to the cent.".into(),
    );
    caveats.extend(econtent::review_caveats(&[BUDGET_CONTENT]));

    let mut keys: Vec<&str> = vec!["ecfr-200-414", "ecfr-200-1"];
    if lines.iter().any(|l| l.category == Category::Personnel) {
        keys.push("ecfr-200-431");
    }
    if lines.iter().any(|l| l.funding == Funding::CostShare) || req.is_some() {
        keys.push("ecfr-200-306");
    }
    if lines
        .iter()
        .any(|l| l.exclusion == Some(calc::MtdcExclusion::ParticipantSupport))
    {
        keys.push("ecfr-200-456");
    }
    keys.push("sf424a");
    let mut sources = econtent::sources_for(keys, &c.sources);
    if let Some(ls) = local_source {
        sources.push(ls);
    }
    sources.push(Source::computed(
        "Budget arithmetic (line items, escalation, fringe, MTDC, indirect, cost share)",
        "This server",
    ));

    let texts = roster::argument_strings(&args, SKIP_KEYS);
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut privacy = Privacy {
        files_written: files.clone(),
        identifiers_detected: found.clone(),
        ..Privacy::none()
    };
    if !found.is_empty() {
        privacy.notes.push(
            "Contact details were found in the text of your request. A budget needs roles and costs, not people's names or contact details: remove them before you share the files.".into(),
        );
    }
    privacy.notes.push(
        "Individual salaries and line items are written only to the workbook and the justification in the output folder. This response has category totals, rates, and structure. Salary figures typed into the request were seen by the AI assistant; use line_items_path to keep them local.".into(),
    );

    let object_classes: Vec<ObjectClassOut> = c
        .object_classes
        .iter()
        .map(|o| ObjectClassOut {
            label: o.label.clone(),
            sf424a_line: o.sf424a_line.clone(),
            what_goes_here: o.what.clone(),
            citation: format!(
                "{}, {}",
                c.sources
                    .get(&o.source)
                    .map(|x| x.title.as_str())
                    .unwrap_or(&o.source),
                o.section
            ),
            confidence: o.confidence,
        })
        .collect();
    let mut rules_cited = vec![
        c.rules.mtdc.clone(),
        c.rules.equipment.clone(),
        c.rules.participant_support.clone(),
        c.rules.subaward_vs_contract.clone(),
        c.rules.fringe.clone(),
    ];
    rules_cited.extend(c.rules.cost_sharing.iter().cloned());
    let mut next_steps = vec![
        "Fill every [NURSE: ...] placeholder in the justification and check each figure against your records.".to_string(),
        "Compare the budget with the funder's notice: indirect cap, cost share rules, salary limits, and page limits.".to_string(),
    ];
    if blank {
        next_steps.insert(0, "Add your costs to the Line items sheet (spare rows are ready), or run this tool again with line_items or a line_items_path file.".into());
    }
    let result = BudgetResult {
        project: project.clone(),
        settings: SettingsOut {
            years: n,
            default_fringe_rate_percent: fringe * 100.0,
            salary_escalation_percent: salary_escalation * 100.0,
            other_escalation_percent: other_escalation * 100.0,
            indirect_method: ind.method,
            indirect_rate_percent: indirect_rate * 100.0,
            indirect_base: indirect_base.label().to_string(),
            indirect_on_cost_share: rates.indirect_on_cost_share,
            subaward_cap_in_mtdc: c.rules.subaward_cap_in_mtdc,
            equipment_threshold: c.rules.equipment_threshold,
        },
        line_counts,
        federal_request: federal_out,
        cost_share: share_out,
        project_total: project_out,
        cost_share_check: match_out,
        arithmetic_check_passed: arithmetic_ok,
        workbook: workbook_path,
        justification: justification_path,
        justification_placeholders: placeholders,
        rules_used: econtent::cite_all(&rules_cited, &c.sources),
        object_classes,
        funder_checks: econtent::cite_all(&c.funder_checks, &c.sources),
        next_steps,
    };
    Ok(Envelope::new(s, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

fn read_me_lines(c: &BudgetContent, rates: &Rates, blank: bool) -> Vec<String> {
    let mut v = vec![
        "DRAFT program budget. Decision support only: check every number against your records and the funder's notice before you submit.".to_string(),
        "Yellow cells are inputs. Everything else is a formula, so changing a yellow cell recalculates the budget. Add costs in the spare rows at the bottom of the Line items sheet (the summary includes them).".to_string(),
        "Personnel: Quantity = percent of time (50% = 0.5) and Unit cost = annual base salary. Other lines: Quantity = units per year and Unit cost = cost per unit. Fringe benefits are calculated on personnel lines.".to_string(),
        "Use roles and descriptions, never staff names. This file holds salaries: keep it in your secure folder.".to_string(),
        format!(
            "Escalation: a line marked Escalate = Yes grows by the Inputs rate each year after year 1 (year k = year 1 x (1 + rate)^(k-1)); amounts are rounded to the cent. This budget was built with {} year(s).",
            rates.years
        ),
        format!("MTDC: {}", c.rules.mtdc.text.split_whitespace().collect::<Vec<_>>().join(" ")),
        format!("Equipment: {}", c.rules.equipment.text.split_whitespace().collect::<Vec<_>>().join(" ")),
        format!("De minimis rate: {}", c.rules.de_minimis.text.split_whitespace().collect::<Vec<_>>().join(" ")),
        format!(
            "Review status: pending. {}",
            c.meta.verification_note.split_whitespace().collect::<Vec<_>>().join(" ")
        ),
    ];
    if blank {
        v.insert(1, "This workbook was written without line items: enter your costs on the Line items sheet.".to_string());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_percent_format() {
        assert_eq!(fmt_money(1234567.891), "$1,234,567.89");
        assert_eq!(fmt_money(0.5), "$0.50");
        assert_eq!(fmt_money(-12.0), "-$12.00");
        assert_eq!(fmt_pct(0.15), "15%");
        assert_eq!(fmt_pct(0.125), "12.5%");
        assert_eq!(fmt_pct(0.5), "50%");
    }

    #[test]
    fn content_loads_and_the_rates_are_the_published_ones() {
        let c: BudgetContent = econtent::load(BUDGET_CONTENT).unwrap();
        assert_eq!(c.rules.de_minimis.rate_percent, 15.0);
        assert_eq!(c.rules.subaward_cap_in_mtdc, 50_000.0);
        assert_eq!(c.rules.equipment_threshold, 10_000.0);
        assert!(
            c.object_classes
                .iter()
                .any(|o| o.label == "Indirect charges")
        );
    }
}
