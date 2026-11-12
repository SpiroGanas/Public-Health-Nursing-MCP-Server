//! PLAN-05 `build_evaluation_plan`: an evaluation plan that follows the six steps of CDC's
//! Program Evaluation Framework (2024): evaluation questions, indicators, data sources,
//! methods, timing, and responsible roles, with the process, outcome, impact, formative, and
//! economic distinctions and an optional RE-AIM lens.
//!
//! The plan is content-driven (`content/planning/evaluation_framework.yaml`): the question
//! bank is CDC's (MMWR Table 5), the evaluation types, designs, methods, and matrix columns
//! are CDC's, and the templates for items CDC does not word are this project's defaults and
//! say so. Rows come from the nurse's logic model (or are given directly); the tool fills
//! missing columns with suggestions marked as such, checks the plan for gaps, and writes a
//! Word (or PDF) plan to /data/output with blanks the nurse fills in locally. An optional
//! staff roster puts names beside the responsible roles in that file only.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::logic_model::Stage;
use super::smartie::read_date;
use crate::context::Context;
use crate::epr::content::{self, DocSource};
use crate::epr::roster::{self, Roster, RosterSummary, RosterTally};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/planning/evaluation_framework.yaml`.
pub const EVALUATION_FRAMEWORK: &str = "planning-evaluation-framework";

const MAX_ROWS: usize = 60;
const MAX_LIST: usize = 30;
const MAX_TEXT_CHARS: usize = 800;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct StepDef {
    number: u8,
    label: String,
    key_products: Vec<String>,
    key_questions: Vec<String>,
    in_plan: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ItemText {
    label: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ItemsWrap<T> {
    items: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
struct TypeDef {
    id: String,
    label: String,
    shows: String,
    when: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StageType {
    stage: String,
    types: Vec<String>,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StageTypes {
    items: Vec<StageType>,
}

#[derive(Debug, Clone, Deserialize)]
struct DesignDef {
    id: String,
    label: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MethodDef {
    id: String,
    label: String,
    kind: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MethodsWrap {
    items: Vec<MethodDef>,
    existing_data_note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DqDim {
    label: String,
    question: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DqWrap {
    dimensions: Vec<DqDim>,
}

#[derive(Debug, Clone, Deserialize)]
struct QBank {
    id: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct QBankWrap {
    items: Vec<QBank>,
}

#[derive(Debug, Clone, Deserialize)]
struct EquityWrap {
    items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Phase {
    step: u8,
    label: String,
    share: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct Timeline {
    note: String,
    default_weeks: i64,
    phases: Vec<Phase>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReAimDim {
    letter: String,
    label: String,
    level: String,
    definition: String,
    #[serde(rename = "type")]
    dtype: String,
    question: String,
    indicator: String,
    data_sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReAim {
    citation_note: String,
    pragmatic_note: String,
    mapping_provenance: String,
    dimensions: Vec<ReAimDim>,
}

#[derive(Debug, Clone, Deserialize)]
struct Holders {
    items: Vec<String>,
}

/// `content/planning/evaluation_framework.yaml` (the parts the tool uses).
#[derive(Debug, Clone, Deserialize)]
struct EvalContent {
    sources: BTreeMap<String, DocSource>,
    steps: Vec<StepDef>,
    cross_cutting_actions: ItemsWrap<ItemText>,
    standards: ItemsWrap<ItemText>,
    evaluation_types: ItemsWrap<TypeDef>,
    stage_types: StageTypes,
    designs: ItemsWrap<DesignDef>,
    design_by_type: BTreeMap<String, String>,
    methods: MethodsWrap,
    data_quality: DqWrap,
    question_bank: QBankWrap,
    equity_prompts: EquityWrap,
    templates: BTreeMap<String, String>,
    timing_by_type: BTreeMap<String, String>,
    data_source_suggestions: BTreeMap<String, serde_json::Value>,
    method_by_focus: BTreeMap<String, String>,
    default_roles: BTreeMap<String, String>,
    interest_holder_types: Holders,
    timeline_defaults: Timeline,
    reaim: ReAim,
}

// ---------------------------------------------------------------- arguments and results

/// Types of evaluation (CDC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvalType {
    /// Is the program feasible, appropriate, and acceptable before full implementation?
    Formative,
    /// Is the program carried out as planned (reach, delivery, quality)?
    Process,
    /// Were the intended outcomes achieved (cannot say what caused them)?
    Outcome,
    /// What would the outcomes have been without the program (causal)?
    Impact,
    /// What do the effects cost?
    Economic,
}

impl EvalType {
    fn id(self) -> &'static str {
        match self {
            EvalType::Formative => "formative",
            EvalType::Process => "process",
            EvalType::Outcome => "outcome",
            EvalType::Impact => "impact",
            EvalType::Economic => "economic",
        }
    }
    fn from_id(s: &str) -> Option<Self> {
        Some(match s {
            "formative" => EvalType::Formative,
            "process" => EvalType::Process,
            "outcome" => EvalType::Outcome,
            "impact" => EvalType::Impact,
            "economic" => EvalType::Economic,
            _ => return None,
        })
    }
}

/// Optional lenses on the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Lens {
    /// RE-AIM: reach, effectiveness, adoption, implementation, maintenance.
    ReAim,
}

/// The program's logic model components (the output of build_logic_model).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct LogicModelInput {
    pub inputs: Vec<String>,
    pub activities: Vec<String>,
    pub outputs: Vec<String>,
    pub short_term_outcomes: Vec<String>,
    pub intermediate_outcomes: Vec<String>,
    pub long_term_outcomes: Vec<String>,
}

/// An evaluation question you wrote, with whatever else you know about it. Anything left out is
/// filled with a suggestion that is marked as one.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PlanRowInput {
    /// The part of the program the question is about ("Home visits", "Referrals from the hospital").
    pub focus: String,
    /// formative, process, outcome, impact, or economic.
    pub evaluation_type: Option<EvalType>,
    /// The evaluation question.
    pub question: String,
    /// What will show the answer (a measurable statement: "percent of eligible parents visited
    /// within 14 days").
    pub indicator: Option<String>,
    /// The measure or metric: numerator, denominator, scale.
    pub measure: Option<String>,
    /// The baseline value and period, if known.
    pub baseline: Option<String>,
    /// Where the data come from.
    pub data_sources: Vec<String>,
    /// How the data are collected: survey, interviews, observation, focus groups, case study,
    /// record review (or your own words).
    pub method: Option<String>,
    /// When the data are collected ("quarterly", "baseline and 12 months").
    pub timing: Option<String>,
    /// The role responsible for collecting the data (a position, not a person's contact details).
    pub responsible: Option<String>,
    /// How the data will be analyzed.
    pub analysis: Option<String>,
}

/// Arguments for `build_evaluation_plan`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct EvaluationPlanArgs {
    /// The program's name.
    pub program_name: String,
    /// The population the program serves.
    pub population: Option<String>,
    /// Stage of development: planning, implementation, or maintenance. It sets the evaluation
    /// types when you give none (CDC: stage guides the type of evaluation).
    pub stage: Option<Stage>,
    /// Why the evaluation is being done and what decision it should inform (one or two
    /// sentences).
    pub purpose: Option<String>,
    /// Who will use the findings (funder, health director, board of health, partners, community
    /// members).
    pub intended_users: Vec<String>,
    /// How they will use them (decide whether to continue, improve delivery, report to a funder).
    pub intended_uses: Vec<String>,
    /// Types of evaluation to plan for: formative, process, outcome, impact, economic. Default:
    /// from the stage (planning: formative; implementation: process and formative; maintenance:
    /// process, outcome, and impact); with no stage, process and outcome.
    pub evaluation_types: Vec<EvalType>,
    /// The logic model (from build_logic_model). Questions and indicators are drafted from its
    /// activities, outputs, and outcomes when you give no rows.
    pub logic_model: Option<LogicModelInput>,
    /// Your own evaluation questions, with any indicator, source, method, timing, or role you
    /// already know.
    pub rows: Vec<PlanRowInput>,
    /// Optional lenses: "re_aim".
    pub lenses: Vec<Lens>,
    /// Groups to break results down by ("race and ethnicity", "language", "age group").
    pub disaggregate_by: Vec<String>,
    /// Roles available on the team (for example "Public health nurse", "Data manager").
    pub roles: Vec<String>,
    /// Evaluation start (YYYY-MM-DD, YYYY-MM, or a year). With an end date the timeline is dated;
    /// with only a start date it runs 52 weeks; with neither it is in weeks.
    pub start_date: Option<String>,
    /// Evaluation end (YYYY-MM-DD, YYYY-MM, or a year).
    pub end_date: Option<String>,
    /// A staff roster CSV/XLSX in /data (name, position). Names are put beside the responsible
    /// roles in the plan file only; nothing from the roster is returned.
    pub roster_path: Option<String>,
    /// Sheet name when the roster is a workbook.
    pub roster_sheet: Option<String>,
    /// Write the plan to /data/output (default true).
    pub write_document: Option<bool>,
    /// Document format: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
}

/// One row of the evaluation matrix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MatrixRow {
    pub number: usize,
    pub focus: String,
    pub evaluation_type: EvalType,
    pub question: String,
    pub indicator: String,
    pub measure: String,
    pub baseline: String,
    pub data_sources: Vec<String>,
    pub method: String,
    pub timing: String,
    pub responsible: String,
    /// Where the question came from: "CDC example question", "drafted from your logic model",
    /// or "your question".
    pub question_origin: String,
    /// Which columns are suggestions from the tool rather than your content.
    pub suggested: Vec<String>,
    /// How the results will be broken down.
    pub disaggregate_by: Vec<String>,
}

/// A gap or suggestion about the plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanFinding {
    pub id: String,
    pub message: String,
}

/// An evaluation type chosen, with CDC's description and the design suggested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TypeOut {
    pub evaluation_type: EvalType,
    pub label: String,
    pub shows: String,
    pub when_to_use: String,
    /// Suggested overarching design (experimental, quasi-experimental, or observational): this
    /// project's default; the design you can use depends on feasibility and ethics.
    pub suggested_design: String,
    pub design_note: String,
}

/// A step of the CDC framework and what the plan puts there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StepOut {
    pub number: u8,
    pub label: String,
    pub in_this_plan: String,
    pub key_products: Vec<String>,
    pub key_questions: Vec<String>,
}

/// A phase of the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhaseOut {
    pub step: u8,
    pub label: String,
    /// "YYYY-MM-DD to YYYY-MM-DD" or "Weeks 1-4".
    pub window: String,
}

/// A RE-AIM dimension with the program's wording filled in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReAimOut {
    pub letter: String,
    pub label: String,
    pub level: String,
    pub definition: String,
    /// Which CDC type of question it is (this project's mapping).
    pub evaluation_type: String,
    pub question: String,
    pub indicator: String,
    pub data_sources: Vec<String>,
}

/// Result of `build_evaluation_plan`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EvaluationPlanResult {
    pub program_name: String,
    pub population: Option<String>,
    pub stage: Option<String>,
    pub purpose: Option<String>,
    pub intended_users: Vec<String>,
    pub intended_uses: Vec<String>,
    pub evaluation_types: Vec<TypeOut>,
    /// Why these types were chosen.
    pub type_basis: String,
    pub matrix: Vec<MatrixRow>,
    /// The matrix's columns in CDC's order.
    pub matrix_columns: Vec<String>,
    pub findings: Vec<PlanFinding>,
    pub timeline: Vec<PhaseOut>,
    pub timeline_basis: String,
    pub framework_steps: Vec<StepOut>,
    pub cross_cutting_actions: Vec<String>,
    pub standards: Vec<String>,
    /// Questions CDC asks of the plan about equity and interest holder engagement.
    pub equity_questions: Vec<String>,
    pub data_collection_methods: Vec<String>,
    pub data_quality_checks: Vec<String>,
    pub reaim: Option<Vec<ReAimOut>>,
    pub roster: Option<RosterSummary>,
    pub file: Option<String>,
}

// ---------------------------------------------------------------- helpers

fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tidy_opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(tidy).filter(|x| !x.is_empty())
}

fn tidy_list(items: &[String], what: &str) -> Result<Vec<String>> {
    if items.len() > MAX_LIST {
        return Err(PhnError::InvalidInput(format!(
            "{what} has {} items; give at most {MAX_LIST}",
            items.len()
        )));
    }
    let mut out = Vec::new();
    for s in items {
        let t = tidy(s);
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > MAX_TEXT_CHARS {
            return Err(PhnError::InvalidInput(format!(
                "an item in {what} is longer than {MAX_TEXT_CHARS} characters"
            )));
        }
        out.push(t);
    }
    Ok(out)
}

fn lower_first(s: &str) -> String {
    let t = s.trim().trim_end_matches('.');
    let mut c = t.chars();
    match c.next() {
        // Keep acronyms and proper nouns ("CDC", "Springfield"): lower only a word that is
        // not followed by another capital letter and is a common word start.
        Some(f) if c.as_str().chars().next().is_some_and(|n| n.is_lowercase()) => {
            f.to_lowercase().collect::<String>() + c.as_str()
        }
        _ => t.to_string(),
    }
}

fn fill_item(template: &str, item: &str) -> String {
    template.replace("{item}", &lower_first(item))
}

fn type_label(t: EvalType) -> &'static str {
    match t {
        EvalType::Formative => "Formative",
        EvalType::Process => "Process",
        EvalType::Outcome => "Outcome",
        EvalType::Impact => "Impact",
        EvalType::Economic => "Economic",
    }
}

fn blank(what: &str) -> String {
    format!("[{what}]")
}

fn method_label(c: &EvalContent, id_or_text: &str) -> String {
    let key = phn_core::text::normalize(id_or_text).replace(' ', "_");
    c.methods
        .items
        .iter()
        .find(|m| {
            m.id == key
                || phn_core::text::normalize(&m.label) == phn_core::text::normalize(id_or_text)
        })
        .map(|m| format!("{} ({})", m.label, m.kind))
        .unwrap_or_else(|| id_or_text.to_string())
}

fn suggestions_for(c: &EvalContent, focus: &str) -> Vec<String> {
    c.data_source_suggestions
        .get(focus)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

struct RowSeed {
    focus: String,
    kind: EvalType,
    question: String,
    indicator: String,
    origin: String,
    focus_key: &'static str,
}

fn seeds_from_logic_model(
    c: &EvalContent,
    lm: &LogicModelInput,
    types: &[EvalType],
    population: Option<&str>,
) -> Vec<RowSeed> {
    let t = |k: &str| c.templates.get(k).cloned().unwrap_or_default();
    let want = |x: EvalType| types.contains(&x);
    let mut out: Vec<RowSeed> = Vec::new();
    let origin = "drafted from your logic model".to_string();
    if want(EvalType::Process) {
        if !lm.inputs.is_empty() {
            let q = c
                .question_bank
                .items
                .iter()
                .find(|q| q.id == "inputs_sufficient")
                .map(|q| q.text.clone())
                .unwrap_or_else(|| fill_item(&t("input_question"), &lm.inputs.join("; ")));
            out.push(RowSeed {
                focus: "Inputs".into(),
                kind: EvalType::Process,
                question: q,
                indicator: fill_item(&t("input_indicator"), &lm.inputs.join("; ")),
                origin: "CDC example question".into(),
                focus_key: "inputs",
            });
        }
        for a in &lm.activities {
            out.push(RowSeed {
                focus: format!("Activity: {a}"),
                kind: EvalType::Process,
                question: fill_item(&t("activity_question"), a),
                indicator: fill_item(&t("activity_indicator"), a),
                origin: origin.clone(),
                focus_key: "activities",
            });
        }
        for o in &lm.outputs {
            out.push(RowSeed {
                focus: format!("Output: {o}"),
                kind: EvalType::Process,
                question: fill_item(&t("output_question"), o),
                indicator: fill_item(&t("output_indicator"), o),
                origin: origin.clone(),
                focus_key: "outputs",
            });
        }
    }
    if want(EvalType::Formative) {
        for a in &lm.activities {
            out.push(RowSeed {
                focus: format!("Activity: {a}"),
                kind: EvalType::Formative,
                question: fill_item(&t("formative_question"), a),
                indicator: fill_item(&t("formative_indicator"), a),
                origin: origin.clone(),
                focus_key: "activities",
            });
        }
    }
    if want(EvalType::Outcome) || want(EvalType::Impact) {
        let groups: [(&Vec<String>, &str, &'static str, &str); 3] = [
            (&lm.short_term_outcomes, "Short-term outcome", "short_term", "short_term_question"),
            (&lm.intermediate_outcomes, "Intermediate outcome", "intermediate", "intermediate_question"),
            (&lm.long_term_outcomes, "Long-term outcome", "long_term", "long_term_question"),
        ];
        for (items, label, key, qk) in groups {
            for it in items {
                // Long-term outcomes: impact when asked for, else outcome.
                let kind = if key == "long_term" && want(EvalType::Impact) {
                    EvalType::Impact
                } else if want(EvalType::Outcome) {
                    EvalType::Outcome
                } else {
                    continue;
                };
                out.push(RowSeed {
                    focus: format!("{label}: {it}"),
                    kind,
                    question: fill_item(&t(qk), it),
                    indicator: fill_item(&t("outcome_indicator"), it),
                    origin: origin.clone(),
                    focus_key: key,
                });
            }
        }
    }
    // CDC example questions that carry equity and cost.
    let bank = |id: &str| c.question_bank.items.iter().find(|q| q.id == id).cloned();
    let pop = population.unwrap_or("the population of focus");
    if want(EvalType::Process)
        && let Some(q) = bank("equity_drivers")
    {
        out.push(RowSeed {
            focus: "Equity".into(),
            kind: EvalType::Process,
            question: q.text,
            indicator: format!("Documented ways activities are designed or tailored for {pop}"),
            origin: "CDC example question".into(),
            focus_key: "activities",
        });
    }
    if want(EvalType::Process)
        && let Some(q) = bank("reaching_audience")
    {
        out.push(RowSeed {
            focus: "Reach".into(),
            kind: EvalType::Process,
            question: q.text,
            indicator: format!("Number and percent of {pop} reached, by group"),
            origin: "CDC example question".into(),
            focus_key: "activities",
        });
    }
    if want(EvalType::Outcome)
        && let Some(q) = bank("for_whom_effective")
    {
        out.push(RowSeed {
            focus: "Equity".into(),
            kind: EvalType::Outcome,
            question: q.text,
            indicator: "Outcome indicators broken down by the groups listed in the plan".into(),
            origin: "CDC example question".into(),
            focus_key: "short_term",
        });
    }
    if want(EvalType::Outcome)
        && let Some(q) = bank("unintended")
    {
        out.push(RowSeed {
            focus: "Unintended effects".into(),
            kind: EvalType::Outcome,
            question: q.text,
            indicator: "Unintended effects reported by participants, staff, and partners".into(),
            origin: "CDC example question".into(),
            focus_key: "short_term",
        });
    }
    if want(EvalType::Economic)
        && let Some(q) = bank("cost_effectiveness")
    {
        out.push(RowSeed {
            focus: "Cost".into(),
            kind: EvalType::Economic,
            question: q.text,
            indicator: "Program cost per unit of service and cost per outcome achieved".into(),
            origin: "CDC example question".into(),
            focus_key: "inputs",
        });
    }
    out
}

fn default_types(c: &EvalContent, stage: Option<Stage>) -> (Vec<EvalType>, String) {
    match stage {
        Some(s) => {
            let d = c.stage_types.items.iter().find(|x| x.stage == s.id());
            let types: Vec<EvalType> = d
                .map(|d| d.types.iter().filter_map(|t| EvalType::from_id(t)).collect())
                .unwrap_or_default();
            (
                types,
                format!(
                    "Chosen from the program's stage ({}): {} (derived from the \"when to use it\" column of CDC's table of evaluation types; this project's mapping).",
                    s.id(),
                    d.map(|d| d.note.as_str()).unwrap_or("")
                ),
            )
        }
        None => (
            vec![EvalType::Process, EvalType::Outcome],
            "No stage was given, so process and outcome evaluation are planned (CDC's two most common question types). Give the stage to refine this.".into(),
        ),
    }
}

fn dated_phases(c: &EvalContent, start: NaiveDate, end: NaiveDate) -> Vec<PhaseOut> {
    let total = (end - start).num_days().max(1) as f64;
    let mut cursor = 0.0_f64;
    let n = c.timeline_defaults.phases.len();
    c.timeline_defaults
        .phases
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let a = start + Duration::days(cursor.round() as i64);
            cursor += total * p.share;
            let b = if i + 1 == n {
                end
            } else {
                start + Duration::days(cursor.round() as i64)
            };
            PhaseOut {
                step: p.step,
                label: p.label.clone(),
                window: format!("{a} to {b}"),
            }
        })
        .collect()
}

fn weekly_phases(c: &EvalContent, weeks: i64) -> Vec<PhaseOut> {
    let mut cursor = 0.0_f64;
    let n = c.timeline_defaults.phases.len();
    c.timeline_defaults
        .phases
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let a = cursor.round() as i64 + 1;
            cursor += weeks as f64 * p.share;
            let b = if i + 1 == n {
                weeks
            } else {
                (cursor.round() as i64).max(a)
            };
            PhaseOut {
                step: p.step,
                label: p.label.clone(),
                window: if a == b {
                    format!("Week {a}")
                } else {
                    format!("Weeks {a}-{b}")
                },
            }
        })
        .collect()
}

// ---------------------------------------------------------------- the tool

/// Build an evaluation plan.
#[allow(clippy::too_many_lines)]
pub fn build_evaluation_plan(
    ctx: &Context,
    args: EvaluationPlanArgs,
) -> Result<Envelope<EvaluationPlanResult>> {
    let c: EvalContent = content::load(EVALUATION_FRAMEWORK)?;
    let name = tidy(&args.program_name);
    if name.is_empty() {
        return Err(PhnError::InvalidInput(
            "give the program's name (program_name)".into(),
        ));
    }
    if name.chars().count() > 150 {
        return Err(PhnError::InvalidInput(
            "program_name is longer than 150 characters; use the short name".into(),
        ));
    }
    let population = tidy_opt(&args.population);
    let purpose = tidy_opt(&args.purpose);
    if purpose.as_deref().is_some_and(|p| p.chars().count() > MAX_TEXT_CHARS) {
        return Err(PhnError::InvalidInput(format!(
            "purpose is longer than {MAX_TEXT_CHARS} characters; use one or two sentences"
        )));
    }
    let users = tidy_list(&args.intended_users, "intended_users")?;
    let uses = tidy_list(&args.intended_uses, "intended_uses")?;
    let disagg = tidy_list(&args.disaggregate_by, "disaggregate_by")?;
    let roles = tidy_list(&args.roles, "roles")?;
    if args.rows.len() > MAX_ROWS {
        return Err(PhnError::InvalidInput(format!(
            "{} rows given; a plan with more than {MAX_ROWS} questions is too large to evaluate: prioritize",
            args.rows.len()
        )));
    }
    let lm = match &args.logic_model {
        Some(l) => Some(LogicModelInput {
            inputs: tidy_list(&l.inputs, "logic_model.inputs")?,
            activities: tidy_list(&l.activities, "logic_model.activities")?,
            outputs: tidy_list(&l.outputs, "logic_model.outputs")?,
            short_term_outcomes: tidy_list(&l.short_term_outcomes, "logic_model.short_term_outcomes")?,
            intermediate_outcomes: tidy_list(&l.intermediate_outcomes, "logic_model.intermediate_outcomes")?,
            long_term_outcomes: tidy_list(&l.long_term_outcomes, "logic_model.long_term_outcomes")?,
        }),
        None => None,
    };
    let lm_has_content = lm.as_ref().is_some_and(|l| {
        !(l.inputs.is_empty()
            && l.activities.is_empty()
            && l.outputs.is_empty()
            && l.short_term_outcomes.is_empty()
            && l.intermediate_outcomes.is_empty()
            && l.long_term_outcomes.is_empty())
    });
    if !lm_has_content && args.rows.iter().all(|r| tidy(&r.question).is_empty()) {
        return Err(PhnError::InvalidInput(
            "give the program's logic model (logic_model, from build_logic_model) or your own evaluation questions (rows); the tool does not invent program content".into(),
        ));
    }

    // ---- dates
    let start = match tidy_opt(&args.start_date) {
        Some(s) => Some(read_date(&s).ok_or_else(|| {
            PhnError::InvalidInput(
                "start_date could not be read; use YYYY-MM-DD, YYYY-MM, or a four-digit year (1900-2100)".into(),
            )
        })?),
        None => None,
    };
    let end = match tidy_opt(&args.end_date) {
        Some(s) => Some(read_date(&s).ok_or_else(|| {
            PhnError::InvalidInput(
                "end_date could not be read; use YYYY-MM-DD, YYYY-MM, or a four-digit year (1900-2100)".into(),
            )
        })?),
        None => None,
    };
    if end.is_some() && start.is_none() {
        return Err(PhnError::InvalidInput(
            "end_date needs a start_date".into(),
        ));
    }
    if let (Some(s), Some(e)) = (start, end) {
        if e <= s {
            return Err(PhnError::InvalidInput(
                "end_date must be after start_date".into(),
            ));
        }
        if (e - s).num_days() > 3660 {
            return Err(PhnError::InvalidInput(
                "the evaluation period is longer than 10 years; plan a shorter period and renew it".into(),
            ));
        }
    }
    let (timeline, timeline_basis) = match (start, end) {
        (Some(s), Some(e)) => (
            dated_phases(&c, s, e),
            format!(
                "Dated from {s} to {e}. {} Shares of the period per step are this project's defaults.",
                c.timeline_defaults.note
            ),
        ),
        (Some(s), None) => {
            let e = s + Duration::weeks(c.timeline_defaults.default_weeks);
            (
                dated_phases(&c, s, e),
                format!(
                    "Dated from {s} for {} weeks (no end date given). {} Shares per step are this project's defaults.",
                    c.timeline_defaults.default_weeks, c.timeline_defaults.note
                ),
            )
        }
        _ => (
            weekly_phases(&c, c.timeline_defaults.default_weeks),
            format!(
                "No dates given, so the schedule is in weeks over {} weeks. {} Shares per step are this project's defaults.",
                c.timeline_defaults.default_weeks, c.timeline_defaults.note
            ),
        ),
    };

    // ---- types
    let (types, type_basis) = if args.evaluation_types.is_empty() {
        default_types(&c, args.stage)
    } else {
        let mut t = args.evaluation_types.clone();
        t.sort();
        t.dedup();
        (t, "The evaluation types you asked for.".to_string())
    };
    let type_out: Vec<TypeOut> = types
        .iter()
        .filter_map(|t| {
            let d = c.evaluation_types.items.iter().find(|x| x.id == t.id())?;
            let design_id = c.design_by_type.get(t.id()).cloned().unwrap_or_default();
            let design = c.designs.items.iter().find(|x| x.id == design_id || x.id.replace('_', "-") == design_id);
            Some(TypeOut {
                evaluation_type: *t,
                label: d.label.clone(),
                shows: d.shows.clone(),
                when_to_use: d.when.clone(),
                suggested_design: design.map(|x| x.label.clone()).unwrap_or_else(|| design_id.clone()),
                design_note: design.map(|x| x.text.clone()).unwrap_or_default(),
            })
        })
        .collect();

    // ---- rows
    let mut seeds: Vec<RowSeed> = Vec::new();
    if lm_has_content {
        seeds.extend(seeds_from_logic_model(
            &c,
            lm.as_ref().expect("checked"),
            &types,
            population.as_deref(),
        ));
    }
    let n_generated = seeds.len();
    let mut explicit: Vec<&PlanRowInput> = Vec::new();
    for r in &args.rows {
        if tidy(&r.question).is_empty() {
            continue;
        }
        if r.question.chars().count() > MAX_TEXT_CHARS {
            return Err(PhnError::InvalidInput(format!(
                "a question is longer than {MAX_TEXT_CHARS} characters; keep each question to a sentence or two"
            )));
        }
        explicit.push(r);
    }
    if n_generated + explicit.len() > MAX_ROWS {
        return Err(PhnError::InvalidInput(format!(
            "the plan would have {} rows (limit {MAX_ROWS}). Keep the questions that matter most: CDC advises prioritizing questions by interest holder needs, stage, and resources",
            n_generated + explicit.len()
        )));
    }

    let default_resp = |kind: EvalType| -> String {
        let key = match kind {
            EvalType::Economic => "analysis",
            _ => "data_collection",
        };
        roles
            .first()
            .cloned()
            .unwrap_or_else(|| c.default_roles.get(key).cloned().unwrap_or_default())
    };
    let timing_for = |kind: EvalType| c.timing_by_type.get(kind.id()).cloned().unwrap_or_default();
    let mut matrix: Vec<MatrixRow> = Vec::new();
    for sd in seeds {
        let mut suggested = vec!["indicator".to_string(), "measure".to_string(), "baseline".to_string(), "timing".to_string()];
        let sources = suggestions_for(&c, sd.focus_key);
        if !sources.is_empty() {
            suggested.push("data_sources".into());
        }
        let method_id = c.method_by_focus.get(sd.focus_key).cloned().unwrap_or_else(|| "record_review".into());
        suggested.push("method".into());
        suggested.push("responsible".into());
        matrix.push(MatrixRow {
            number: matrix.len() + 1,
            focus: sd.focus,
            evaluation_type: sd.kind,
            question: sd.question,
            indicator: sd.indicator,
            measure: c.templates.get("measure_placeholder").cloned().unwrap_or_default(),
            baseline: c.templates.get("baseline_placeholder").cloned().unwrap_or_default(),
            data_sources: sources,
            method: method_label(&c, &method_id),
            timing: timing_for(sd.kind),
            responsible: default_resp(sd.kind),
            question_origin: sd.origin,
            suggested,
            disaggregate_by: if matches!(sd.kind, EvalType::Outcome | EvalType::Impact) || sd.focus_key == "activities" {
                disagg.clone()
            } else {
                Vec::new()
            },
        });
    }
    for r in explicit {
        let kind = r.evaluation_type.unwrap_or(EvalType::Process);
        let mut suggested: Vec<String> = Vec::new();
        let key = match kind {
            EvalType::Process | EvalType::Formative => "activities",
            EvalType::Outcome => "short_term",
            EvalType::Impact => "long_term",
            EvalType::Economic => "inputs",
        };
        let indicator = match tidy_opt(&r.indicator) {
            Some(i) => i,
            None => {
                suggested.push("indicator".into());
                blank("add an indicator: a measurable statement that answers the question")
            }
        };
        let measure = tidy_opt(&r.measure).unwrap_or_else(|| {
            suggested.push("measure".into());
            c.templates.get("measure_placeholder").cloned().unwrap_or_default()
        });
        let baseline = tidy_opt(&r.baseline).unwrap_or_else(|| {
            suggested.push("baseline".into());
            c.templates.get("baseline_placeholder").cloned().unwrap_or_default()
        });
        let given_sources = tidy_list(&r.data_sources, "data_sources")?;
        let data_sources = if given_sources.is_empty() {
            suggested.push("data_sources".into());
            suggestions_for(&c, key)
        } else {
            given_sources
        };
        let method = match tidy_opt(&r.method) {
            Some(m) => method_label(&c, &m),
            None => {
                suggested.push("method".into());
                method_label(&c, c.method_by_focus.get(key).map(String::as_str).unwrap_or("record_review"))
            }
        };
        let timing = tidy_opt(&r.timing).unwrap_or_else(|| {
            suggested.push("timing".into());
            timing_for(kind)
        });
        let responsible = tidy_opt(&r.responsible).unwrap_or_else(|| {
            suggested.push("responsible".into());
            default_resp(kind)
        });
        matrix.push(MatrixRow {
            number: matrix.len() + 1,
            focus: tidy_opt(&Some(r.focus.clone())).unwrap_or_else(|| blank("focus area")),
            evaluation_type: kind,
            question: tidy(&r.question),
            indicator,
            measure,
            baseline,
            data_sources,
            method,
            timing,
            responsible,
            question_origin: "your question".into(),
            suggested,
            disaggregate_by: if matches!(kind, EvalType::Outcome | EvalType::Impact) {
                disagg.clone()
            } else {
                Vec::new()
            },
        });
    }
    // ---- findings
    let mut findings: Vec<PlanFinding> = Vec::new();
    let mut find = |id: &str, m: String| findings.push(PlanFinding { id: id.into(), message: m });
    if purpose.is_none() {
        find("no_purpose", "No purpose statement was given. CDC's Step 3 asks for why the evaluation is being done, how the findings will be used, and who will use them.".into());
    }
    if users.is_empty() {
        find("no_intended_users", "No intended users were given. Name who will use the findings (funders, program staff, community members, the board of health).".into());
    }
    if uses.is_empty() {
        find("no_intended_uses", "No intended uses were given. Name what decision or action the findings should inform and when they are needed.".into());
    }
    if args.stage.is_none() {
        find("no_stage", "No stage of development was given; CDC uses it to choose the type of evaluation.".into());
    }
    if disagg.is_empty() {
        find("no_disaggregation", "No groups to break results down by were given. CDC's cross-cutting action on fair and just evaluation asks for questions that provide insight into health equity: name the groups (for example race and ethnicity, language, age) and check each has enough data.".into());
    }
    if start.is_none() {
        find("no_dates", "No start date was given, so the timeline is in weeks. Add start_date (and end_date) for dates.".into());
    }
    for r in &matrix {
        let words = r.question.split_whitespace().count();
        let lower = r.question.to_lowercase();
        if words < 5 {
            find("question_too_short", format!("Question {} is only {words} words; CDC advises specific questions that say what will be investigated.", r.number));
        } else if words > 45 {
            find("question_too_long", format!("Question {} is {words} words; split it so each question investigates one thing.", r.number));
        }
        if ["did ", "is ", "are ", "do ", "does ", "was ", "were ", "have ", "has "].iter().any(|p| lower.starts_with(p))
            && !lower.contains("to what extent")
            && !lower.contains(" how ")
            && !lower.contains("why")
        {
            find("yes_no_question", format!("Question {} can be answered yes or no. CDC describes evaluation questions as usually broad and open-ended: consider \"To what extent...\" or \"How...\".", r.number));
        }
        if r.indicator.starts_with('[') {
            find("missing_indicator", format!("Question {} has no indicator yet.", r.number));
        }
        if r.data_sources.is_empty() {
            find("missing_data_source", format!("Question {} has no data source.", r.number));
        }
    }
    let n_suggested = matrix.iter().filter(|r| !r.suggested.is_empty()).count();
    if n_suggested > 0 {
        find("suggestions_to_confirm", format!("{n_suggested} row(s) carry suggested indicators, measures, baselines, data sources, methods, timing, or roles written by this tool (listed in each row's suggested field). They are starting points: confirm each with the program team and interest holders."));
    }
    let types_in_matrix: Vec<EvalType> = {
        let mut v: Vec<EvalType> = matrix.iter().map(|r| r.evaluation_type).collect();
        v.sort();
        v.dedup();
        v
    };
    for t in &types {
        if !types_in_matrix.contains(t) {
            find("type_without_questions", format!("{} evaluation is planned but no question covers it yet.", type_label(*t)));
        }
    }
    if types.contains(&EvalType::Impact) {
        find("impact_needs_comparison", "Impact evaluation compares outcomes with what would have happened without the program, which needs a comparison group or period. If none is available, report outcomes only; CDC notes that outcome evaluation cannot say what caused the outcomes.".into());
    }

    // ---- RE-AIM
    let reaim: Option<Vec<ReAimOut>> = args.lenses.contains(&Lens::ReAim).then(|| {
        let pop = population.clone().unwrap_or_else(|| "the intended population".into());
        c.reaim
            .dimensions
            .iter()
            .map(|d| ReAimOut {
                letter: d.letter.clone(),
                label: d.label.clone(),
                level: d.level.clone(),
                definition: d.definition.clone(),
                evaluation_type: d.dtype.clone(),
                question: d.question.replace("{program}", &name).replace("{population}", &pop),
                indicator: d.indicator.clone(),
                data_sources: d.data_sources.clone(),
            })
            .collect()
    });

    // ---- roster (names only, into the file)
    let roster_loaded = match tidy_opt(&args.roster_path) {
        Some(p) => Some(Roster::load(ctx, &p, tidy_opt(&args.roster_sheet).as_deref())?),
        None => None,
    };
    let mut tally = RosterTally::default();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    if let Some(r) = &roster_loaded {
        let mut roles_used: Vec<String> = matrix.iter().map(|m| m.responsible.clone()).collect();
        roles_used.sort();
        roles_used.dedup();
        for role in roles_used {
            let holders = r.holders(&role);
            if holders.len() == 1 && !holders[0].name.trim().is_empty() {
                tally.matched += 1;
                names.insert(role, holders[0].name.trim().to_string());
            } else {
                tally.unmatched += 1;
            }
        }
    }
    let roster_summary = roster_loaded
        .as_ref()
        .map(|r| r.summary(tally.matched, tally.unmatched));

    // ---- steps and reference lists
    let framework_steps: Vec<StepOut> = c
        .steps
        .iter()
        .map(|s| StepOut {
            number: s.number,
            label: s.label.clone(),
            in_this_plan: s.in_plan.clone(),
            key_products: s.key_products.clone(),
            key_questions: s.key_questions.clone(),
        })
        .collect();
    let cross: Vec<String> = c
        .cross_cutting_actions
        .items
        .iter()
        .map(|i| format!("{}: {}", i.label, i.text))
        .collect();
    let standards: Vec<String> = c
        .standards
        .items
        .iter()
        .map(|i| format!("{}: {}", i.label, i.text))
        .collect();
    let methods: Vec<String> = c
        .methods
        .items
        .iter()
        .map(|m| format!("{} ({}): {}", m.label, m.kind, m.text))
        .collect();
    let dq: Vec<String> = c
        .data_quality
        .dimensions
        .iter()
        .map(|d| format!("{}: {}", d.label, d.question))
        .collect();

    // ---- document
    let write = args.write_document.unwrap_or(true);
    let mut file = None;
    let mut files_written = Vec::new();
    if write {
        let (dir, _) = templates::output_folder(ctx, "evaluation_plan")?;
        let format = args.format.unwrap_or_default();
        let m = document_markdown(
            &c,
            &name,
            population.as_deref(),
            args.stage,
            purpose.as_deref(),
            &users,
            &uses,
            &type_out,
            &type_basis,
            &matrix,
            &timeline,
            &timeline_basis,
            reaim.as_deref(),
            lm.as_ref().filter(|_| lm_has_content),
            &disagg,
            &names,
            ctx.suppression_threshold(),
        );
        let doc = templates::document(ctx, &m, "en-US", &format!("Evaluation plan: {name}"));
        let f = templates::write_in(
            ctx,
            &dir,
            &format!("evaluation_plan.{}", format.ext()),
            &format.render(&doc)?,
        )?;
        files_written.push(f.clone());
        file = Some(f);
    }

    // ---- summary
    let mut s = format!("**Evaluation plan: {name}**\n\n");
    if let Some(p) = &purpose {
        s.push_str(&format!("Purpose: {p}\n\n"));
    }
    s.push_str(&format!(
        "Types: {}. {type_basis}\n\n",
        type_out.iter().map(|t| t.label.as_str()).collect::<Vec<_>>().join("; ")
    ));
    let rows: Vec<Vec<String>> = matrix
        .iter()
        .map(|r| {
            vec![
                r.number.to_string(),
                type_label(r.evaluation_type).to_string(),
                r.question.clone(),
                r.indicator.clone(),
                r.data_sources.join("; "),
                r.method.clone(),
                r.responsible.clone(),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &["#", "Type", "Question", "Indicator", "Data source", "Method", "Responsible"],
        &rows,
    ));
    s.push_str("\n**Timeline**\n\n");
    for p in &timeline {
        s.push_str(&format!("- Step {}: {} ({})\n", p.step, p.label, p.window));
    }
    if let Some(r) = &reaim {
        s.push_str("\n**RE-AIM lens**\n\n");
        for d in r {
            s.push_str(&format!("- **{} ({}).** {}\n", d.label, d.letter, d.question));
        }
    }
    if !findings.is_empty() {
        s.push_str("\n**Gaps and suggestions**\n\n");
        for f in &findings {
            s.push_str(&format!("- {}\n", f.message));
        }
    }
    if let Some(f) = &file {
        s.push_str(&format!("\nPlan written to {f}.\n"));
    }

    // ---- caveats and privacy
    let texts = roster::argument_strings(
        &args,
        &["roster_path", "roster_sheet", "write_document", "format", "start_date", "end_date"],
    );
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut caveats: Vec<String> = vec![
        "This is a draft plan from CDC's framework. The question bank is CDC's (MMWR 2024, Table 5); the templates, suggested indicators, data sources, methods, timing, roles, and timeline shares for items CDC does not word are this project's defaults. Confirm each with interest holders: CDC's framework is iterative and non-prescriptive.".into(),
        c.methods.existing_data_note.clone(),
        format!(
            "Small numbers: when results are broken down by group, suppress cells below {} before reporting (the server's suppression threshold), and say how privacy and confidentiality will be protected when data are collected (CDC Step 4).",
            ctx.suppression_threshold()
        ),
    ];
    if reaim.is_some() {
        caveats.push(format!(
            "RE-AIM lens: {} {} The mapping of RE-AIM dimensions to CDC question types is {}.",
            c.reaim.citation_note, c.reaim.pragmatic_note, c.reaim.mapping_provenance
        ));
    }
    if roster_summary.is_some() {
        caveats.push("The plan file includes staff names from your roster beside matching roles; it was written to /data/output only. Nothing from the roster is in this response.".into());
    }
    if !found.is_empty() {
        caveats.push(format!(
            "Your inputs appear to contain {}. An evaluation plan describes a program, not people: remove personal details before sharing it.",
            found.join(", ").to_lowercase()
        ));
    }
    caveats.extend(content::review_caveats(&[EVALUATION_FRAMEWORK]));
    let mut keys: Vec<&str> = vec!["cdc-mmwr-2024", "cdc-framework", "cdc-action-guide", "cdc-step3", "cdc-step4"];
    if reaim.is_some() {
        keys.extend(["reaim-2019", "reaim-1999"]);
    }
    let sources = content::sources_for(keys, &c.sources);
    let mut sources = sources;
    if let Some(r) = &roster_loaded {
        sources.push(r.source());
    }
    Ok(Envelope::new(
        s,
        EvaluationPlanResult {
            program_name: name,
            population,
            stage: args.stage.map(|s| s.id().to_string()),
            purpose,
            intended_users: users,
            intended_uses: uses,
            evaluation_types: type_out,
            type_basis,
            matrix,
            matrix_columns: vec![
                "Focus area".into(),
                "Evaluation type".into(),
                "Evaluation question".into(),
                "Indicator".into(),
                "Measure or metric".into(),
                "Baseline".into(),
                "Data source".into(),
                "Data collection method".into(),
                "Timing".into(),
                "Responsible".into(),
            ],
            findings,
            timeline,
            timeline_basis,
            framework_steps,
            cross_cutting_actions: cross,
            standards,
            equity_questions: c.equity_prompts.items.clone(),
            data_collection_methods: methods,
            data_quality_checks: dq,
            reaim,
            roster: roster_summary,
            file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: found,
        files_written,
        notes: if roster_loaded.is_some() {
            vec!["Roster names are in the plan file only; the response has counts.".into()]
        } else {
            Vec::new()
        },
        ..Privacy::none()
    }))
}

#[allow(clippy::too_many_arguments)]
fn document_markdown(
    c: &EvalContent,
    name: &str,
    population: Option<&str>,
    stage: Option<Stage>,
    purpose: Option<&str>,
    users: &[String],
    uses: &[String],
    types: &[TypeOut],
    type_basis: &str,
    matrix: &[MatrixRow],
    timeline: &[PhaseOut],
    timeline_basis: &str,
    reaim: Option<&[ReAimOut]>,
    lm: Option<&LogicModelInput>,
    disagg: &[String],
    names: &BTreeMap<String, String>,
    threshold: u32,
) -> String {
    let who = |role: &str| -> String {
        match names.get(role) {
            Some(n) => format!("{role} ({n})"),
            None => role.to_string(),
        }
    };
    let list = |v: &[String], ph: &str| -> String {
        if v.is_empty() { format!("[{ph}]") } else { v.join("; ") }
    };
    let mut m = format!("# Evaluation plan: {name}\n\n");
    m.push_str(&format!(
        "Draft for review with interest holders. Follows the six steps of the CDC Program Evaluation Framework (2024). Items in [brackets] are for the program team to fill in.\n\n"
    ));
    // Step 1.
    m.push_str("## Step 1. Assess the context\n\n");
    m.push_str(&format!(
        "**Program:** {name}. **Population:** {}. **Stage:** {}.\n\n",
        population.unwrap_or("[population]"),
        stage.map(|s| s.id()).unwrap_or("[planning, implementation, or maintenance]")
    ));
    m.push_str("Table: Interest holders (CDC's four types)\n\n");
    let ih: Vec<Vec<String>> = c
        .interest_holder_types
        .items
        .iter()
        .map(|t| vec![t.clone(), "[names or groups]".into(), "[what they need from the evaluation]".into()])
        .collect();
    m.push_str(&md::table(&["Interest holder type", "Who", "What they need to know"], &ih));
    m.push_str("\n[Evaluation capacity: staff time, skills, budget, and data systems available. Evaluability: is the program logic clear, and are the needed data accessible?]\n\n");
    // Step 2.
    m.push_str("## Step 2. Describe the program\n\n");
    match lm {
        Some(l) => {
            let rows: Vec<Vec<String>> = [
                ("Inputs", &l.inputs),
                ("Activities", &l.activities),
                ("Outputs", &l.outputs),
                ("Short-term outcomes", &l.short_term_outcomes),
                ("Intermediate outcomes", &l.intermediate_outcomes),
                ("Long-term outcomes", &l.long_term_outcomes),
            ]
            .iter()
            .map(|(k, v)| vec![(*k).to_string(), list(v, "none given")])
            .collect();
            m.push_str("Table: Logic model components\n\n");
            m.push_str(&md::table(&["Component", "Content"], &rows));
        }
        None => m.push_str("[Add the program's logic model (see build_logic_model) and a short narrative: need, inputs, activities, outcomes, contextual factors, and stage.]\n"),
    }
    // Step 3.
    m.push_str("\n## Step 3. Focus the evaluation questions and design\n\n");
    m.push_str(&format!("**Purpose:** {}\n\n", purpose.unwrap_or("[why the evaluation is being done and what decision it informs]")));
    m.push_str(&format!("**Intended users:** {}\n\n**Intended uses:** {}\n\n", list(users, "who will use the findings"), list(uses, "how they will use them")));
    m.push_str(&format!("**Evaluation types.** {type_basis}\n\n"));
    for t in types {
        m.push_str(&format!(
            "- **{}.** {} _When to use:_ {} _Suggested design:_ {}.\n",
            t.label, t.shows, t.when_to_use, t.suggested_design
        ));
    }
    // Step 4.
    m.push_str("\n## Step 4. Gather credible evidence: evaluation matrix\n\n");
    for r in matrix {
        m.push_str(&format!("### Question {}: {}\n\n", r.number, r.question));
        let mut rows: Vec<Vec<String>> = vec![
            vec!["Focus area".into(), r.focus.clone()],
            vec!["Evaluation type".into(), type_label(r.evaluation_type).into()],
            vec!["Indicator".into(), r.indicator.clone()],
            vec!["Measure or metric".into(), r.measure.clone()],
            vec!["Baseline".into(), r.baseline.clone()],
            vec!["Data source".into(), if r.data_sources.is_empty() { "[data source]".into() } else { r.data_sources.join("; ") }],
            vec!["Data collection method".into(), r.method.clone()],
            vec!["Timing".into(), r.timing.clone()],
            vec!["Responsible".into(), who(&r.responsible)],
        ];
        if !r.disaggregate_by.is_empty() {
            rows.push(vec!["Results broken down by".into(), r.disaggregate_by.join("; ")]);
        }
        if !r.suggested.is_empty() {
            rows.push(vec!["Suggested by the tool (confirm)".into(), r.suggested.join(", ")]);
        }
        m.push_str(&md::table(&["Item", "Plan"], &rows));
        m.push('\n');
    }
    m.push_str("**Data quality checks (CDC).** ");
    m.push_str(&c.data_quality.dimensions.iter().map(|d| d.label.clone()).collect::<Vec<_>>().join(", "));
    m.push_str(".\n\n");
    m.push_str(&format!(
        "**Small numbers.** Suppress cells below {threshold} when reporting results by group. {}\n\n",
        if disagg.is_empty() { "[Name the groups results will be broken down by.]".to_string() } else { format!("Results are broken down by: {}.", disagg.join("; ")) }
    ));
    // Step 5.
    m.push_str("## Step 5. Generate and support conclusions\n\n");
    m.push_str("Table: Data analysis plan\n\n");
    let ap: Vec<Vec<String>> = matrix
        .iter()
        .map(|r| vec![format!("Q{}", r.number), "[analysis to be performed]".into(), who(&r.responsible), "[due date]".into()])
        .collect();
    m.push_str(&md::table(&["Question", "Analysis", "Responsible", "Due date"], &ap));
    m.push_str("\n[Agree the analysis plan with interest holders before data collection. Compare findings with the expectations for success set in Step 4. Plan how interest holders will help interpret the results.]\n\n");
    // Step 6.
    m.push_str("## Step 6. Act on findings\n\n");
    m.push_str("Table: Use and dissemination plan\n\n");
    m.push_str(&md::table(
        &["Audience", "What they need", "Format and channel", "When"],
        &[vec!["[audience]".into(), "[findings and recommendations]".into(), "[report, brief, presentation]".into(), "[date]".into()]],
    ));
    m.push_str("\n[Name who will champion the findings and recommendations, and how you will know afterwards whether the evaluation was used as intended.]\n\n");
    // Timeline.
    m.push_str("## Timeline\n\n");
    m.push_str(&format!("{timeline_basis}\n\n"));
    let tl: Vec<Vec<String>> = timeline.iter().map(|p| vec![format!("Step {}", p.step), p.label.clone(), p.window.clone()]).collect();
    m.push_str("Table: Evaluation timeline\n\n");
    m.push_str(&md::table(&["Step", "Work", "When"], &tl));
    // RE-AIM.
    if let Some(r) = reaim {
        m.push_str("\n## RE-AIM lens\n\n");
        let rows: Vec<Vec<String>> = r
            .iter()
            .map(|d| vec![format!("{} ({})", d.label, d.letter), d.question.clone(), d.indicator.clone(), d.data_sources.join("; ")])
            .collect();
        m.push_str(&md::table(&["Dimension", "Question", "Indicator", "Data sources"], &rows));
    }
    // Standards.
    m.push_str("\n## Cross-cutting actions and standards (CDC)\n\n");
    for i in &c.cross_cutting_actions.items {
        m.push_str(&format!("- **{}.** {}\n", i.label, i.text));
    }
    m.push('\n');
    for i in &c.standards.items {
        m.push_str(&format!("- **{}.** {}\n", i.label, i.text));
    }
    m.push_str("\n**Questions to ask of this plan (CDC).**\n\n");
    for q in &c.equity_prompts.items {
        m.push_str(&format!("- {q}\n"));
    }
    m.push_str("\nSources: CDC Program Evaluation Framework, 2024 (MMWR Recomm Rep 2024;73(RR-6)) and the CDC Program Evaluation Framework Action Guide.\n");
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn lm() -> LogicModelInput {
        LogicModelInput {
            inputs: vec!["Two public health nurses".into()],
            activities: vec![
                "Provide a nurse home visit within 14 days of discharge".into(),
                "Train hospital staff to refer eligible parents".into(),
            ],
            outputs: vec!["Home visits completed".into()],
            short_term_outcomes: vec!["Parents know the warning signs after birth".into()],
            intermediate_outcomes: vec!["Increase the share of parents who attend the postpartum visit".into()],
            long_term_outcomes: vec!["Reduce preventable postpartum emergency visits".into()],
        }
    }

    fn base() -> EvaluationPlanArgs {
        EvaluationPlanArgs {
            program_name: "Postpartum home visiting program".into(),
            population: Some("first-time parents in Springfield".into()),
            stage: Some(Stage::Implementation),
            purpose: Some("Decide whether to expand to a second hospital.".into()),
            intended_users: vec!["Health director".into()],
            intended_uses: vec!["Decide on expansion".into()],
            logic_model: Some(lm()),
            disaggregate_by: vec!["language".into()],
            start_date: Some("2026-07-01".into()),
            end_date: Some("2027-06-30".into()),
            write_document: Some(false),
            ..EvaluationPlanArgs::default()
        }
    }

    #[test]
    fn drafts_rows_from_the_logic_model_by_stage() {
        let (_d, ctx) = ctx();
        let r = build_evaluation_plan(&ctx, base()).unwrap();
        let types: Vec<EvalType> = r.data.evaluation_types.iter().map(|t| t.evaluation_type).collect();
        assert_eq!(types, vec![EvalType::Process, EvalType::Formative]);
        assert!(r.data.type_basis.contains("implementation"));
        // Process: 1 input row + 2 activity + 1 output + equity + reach; formative: 2 activities.
        let p = r.data.matrix.iter().filter(|m| m.evaluation_type == EvalType::Process).count();
        let f = r.data.matrix.iter().filter(|m| m.evaluation_type == EvalType::Formative).count();
        assert_eq!((p, f), (6, 2));
        assert!(r.data.matrix.iter().all(|m| !m.indicator.is_empty() && !m.data_sources.is_empty()));
        // CDC's own example question is used word for word and labeled.
        let q = r.data.matrix.iter().find(|m| m.question.starts_with("To what extent are the program resources sufficient")).unwrap();
        assert_eq!(q.question_origin, "CDC example question");
        assert_eq!(r.data.matrix[0].number, 1);
        assert!(r.data.matrix.iter().enumerate().all(|(i, m)| m.number == i + 1));
        assert_eq!(r.data.framework_steps.len(), 6);
        assert_eq!(r.data.matrix_columns[2], "Evaluation question");
    }

    #[test]
    fn maintenance_stage_adds_outcome_and_impact_questions() {
        let (_d, ctx) = ctx();
        let mut a = base();
        a.stage = Some(Stage::Maintenance);
        let r = build_evaluation_plan(&ctx, a).unwrap();
        let kinds: Vec<EvalType> = r.data.matrix.iter().map(|m| m.evaluation_type).collect();
        assert!(kinds.contains(&EvalType::Outcome) && kinds.contains(&EvalType::Impact));
        let imp = r.data.matrix.iter().find(|m| m.evaluation_type == EvalType::Impact).unwrap();
        assert!(imp.focus.starts_with("Long-term outcome"));
        assert!(r.data.findings.iter().any(|f| f.id == "impact_needs_comparison"));
        // Outcome rows carry the breakdown groups.
        assert!(r.data.matrix.iter().filter(|m| m.evaluation_type == EvalType::Outcome).all(|m| m.disaggregate_by == vec!["language"]));
    }

    #[test]
    fn explicit_rows_keep_the_nurses_words_and_mark_suggestions() {
        let (_d, ctx) = ctx();
        let r = build_evaluation_plan(
            &ctx,
            EvaluationPlanArgs {
                program_name: "Flu clinics".into(),
                evaluation_types: vec![EvalType::Outcome],
                rows: vec![
                    PlanRowInput {
                        focus: "Vaccination".into(),
                        evaluation_type: Some(EvalType::Outcome),
                        question: "To what extent did clinic attendance raise flu vaccination among adults 65 and older?".into(),
                        indicator: Some("Percent of adults 65+ vaccinated".into()),
                        data_sources: vec!["MIIS".into()],
                        method: Some("record_review".into()),
                        timing: Some("Baseline and May 2027".into()),
                        responsible: Some("Data manager".into()),
                        ..PlanRowInput::default()
                    },
                    PlanRowInput {
                        question: "Did clinics run?".into(),
                        ..PlanRowInput::default()
                    },
                ],
                write_document: Some(false),
                ..EvaluationPlanArgs::default()
            },
        )
        .unwrap();
        let a = &r.data.matrix[0];
        assert_eq!(a.question_origin, "your question");
        assert_eq!(a.indicator, "Percent of adults 65+ vaccinated");
        assert_eq!(a.data_sources, vec!["MIIS"]);
        assert_eq!(a.method, "Document or record review (qualitative)");
        assert_eq!(a.timing, "Baseline and May 2027");
        assert_eq!(a.suggested, vec!["measure", "baseline"]);
        let b = &r.data.matrix[1];
        assert!(b.indicator.starts_with('['));
        assert!(b.suggested.contains(&"indicator".to_string()));
        let ids: Vec<&str> = r.data.findings.iter().map(|f| f.id.as_str()).collect();
        for w in ["question_too_short", "missing_indicator", "yes_no_question", "no_purpose", "no_intended_users", "no_disaggregation", "no_dates", "no_stage", "suggestions_to_confirm"] {
            assert!(ids.contains(&w), "{w} not in {ids:?}");
        }
        assert!(!r.data.findings.iter().any(|f| f.id == "yes_no_question" && f.message.contains("Question 1 ")));
    }

    #[test]
    fn timeline_is_dated_or_in_weeks() {
        let (_d, ctx) = ctx();
        let r = build_evaluation_plan(&ctx, base()).unwrap();
        assert_eq!(r.data.timeline.len(), 6);
        assert!(r.data.timeline[0].window.starts_with("2026-07-01 to "));
        assert!(r.data.timeline[5].window.ends_with(" to 2027-06-30"));
        // Windows are contiguous.
        for w in r.data.timeline.windows(2) {
            let end = w[0].window.split(" to ").nth(1).unwrap();
            let start = w[1].window.split(" to ").next().unwrap();
            assert_eq!(end, start);
        }
        let mut a = base();
        a.start_date = None;
        a.end_date = None;
        let r = build_evaluation_plan(&ctx, a).unwrap();
        assert!(r.data.timeline[0].window.starts_with("Weeks 1-") || r.data.timeline[0].window.starts_with("Week 1"));
        assert!(r.data.timeline[5].window.ends_with("-52"));
        assert!(r.data.findings.iter().any(|f| f.id == "no_dates"));
        let mut a = base();
        a.end_date = None;
        let r = build_evaluation_plan(&ctx, a).unwrap();
        assert!(r.data.timeline[5].window.ends_with("2027-06-30"), "{:?}", r.data.timeline[5]);
    }

    #[test]
    fn bad_input_is_a_tool_error() {
        let (_d, ctx) = ctx();
        let e = |a: EvaluationPlanArgs| build_evaluation_plan(&ctx, a).unwrap_err().to_string();
        assert!(e(EvaluationPlanArgs::default()).contains("program's name"));
        assert!(e(EvaluationPlanArgs { program_name: "X".into(), ..Default::default() }).contains("logic model"));
        let mut a = base();
        a.end_date = Some("2026-01-01".into());
        assert!(e(a).contains("after start_date"));
        let mut a = base();
        a.start_date = Some("1850-01-01".into());
        assert!(e(a).contains("start_date could not be read"));
        let mut a = base();
        a.start_date = None;
        assert!(e(a).contains("needs a start_date"));
        let mut a = base();
        a.start_date = Some("2000-01-01".into());
        a.end_date = Some("2030-01-01".into());
        assert!(e(a).contains("10 years"));
        let mut a = base();
        a.rows = vec![PlanRowInput { question: "Q".into(), ..Default::default() }; 61];
        assert!(e(a).contains("rows given"));
        let mut a = base();
        a.logic_model.as_mut().unwrap().activities = vec!["Provide x".into(); 31];
        assert!(e(a).contains("at most 30"));
    }

    #[test]
    fn reaim_lens_adds_five_dimensions() {
        let (_d, ctx) = ctx();
        let mut a = base();
        a.lenses = vec![Lens::ReAim];
        let r = build_evaluation_plan(&ctx, a).unwrap();
        let ra = r.data.reaim.as_ref().unwrap();
        let letters: String = ra.iter().map(|d| d.letter.as_str()).collect();
        assert_eq!(letters, "REAIM");
        assert!(ra[0].question.contains("Postpartum home visiting program"));
        assert!(ra[0].question.contains("first-time parents in Springfield"));
        assert!(r.caveats.iter().any(|c| c.contains("RE-AIM")));
        assert!(r.sources.iter().any(|s| s.title.contains("RE-AIM")));
        let r = build_evaluation_plan(&ctx, base()).unwrap();
        assert!(r.data.reaim.is_none());
    }

    #[test]
    fn writes_the_plan_with_blanks_and_roster_names_only_in_the_file() {
        let (dir, ctx) = ctx();
        std::fs::write(
            dir.path().join("data/staff.csv"),
            "Name,Position,Phone,Email\nPat Quill,Data manager,617-555-0199,pq@example.org\nAvery Stone,Public health nurse,617-555-0100,as@example.org\nLee Moss,Public health nurse,617-555-0101,lm@example.org\n",
        )
        .unwrap();
        let mut a = base();
        a.write_document = Some(true);
        a.roles = vec!["Data manager".into()];
        a.roster_path = Some("staff.csv".into());
        a.lenses = vec![Lens::ReAim];
        let r = build_evaluation_plan(&ctx, a).unwrap();
        let f = r.data.file.clone().unwrap();
        assert!(f.ends_with("evaluation_plan.docx"), "{f}");
        let roster = r.data.roster.as_ref().unwrap();
        assert_eq!(roster.staff, 3);
        assert_eq!((roster.matched, roster.unmatched), (1, 0));
        // Nothing from the roster is in the response.
        let all = serde_json::to_string(&r).unwrap();
        for v in ["Pat Quill", "617-555", "example.org", "Avery"] {
            assert!(!all.contains(v), "{v} leaked");
        }
        // The file has the name (docx is a zip: check the XML inside).
        let bytes = std::fs::read(dir.path().join("data").join(&f)).unwrap();
        let zip = zip_text(&bytes);
        assert!(zip.contains("Data manager (Pat Quill)"), "name not in the plan file");
        assert!(!zip.contains("617-555"), "contact details must not be copied");
        assert!(zip.contains("[analysis to be performed]"));
        assert!(zip.contains("Step 4. Gather credible evidence"));
        assert!(zip.contains("RE-AIM lens"));
    }

    fn zip_text(bytes: &[u8]) -> String {
        phn_local::docx::parse_docx(bytes).unwrap().plain_text()
    }

    #[test]
    fn identifiers_in_inputs_are_flagged() {
        let (_d, ctx) = ctx();
        let mut a = base();
        a.purpose = Some("Call Jo Park at 617-555-0142 to decide".into());
        let r = build_evaluation_plan(&ctx, a).unwrap();
        assert!(!r.privacy.identifiers_detected.is_empty());
        assert!(!r.caveats.iter().any(|c| c.contains("555-0142")));
    }

    #[test]
    fn content_review_and_small_number_caveats() {
        let (_d, ctx) = ctx();
        let r = build_evaluation_plan(&ctx, base()).unwrap();
        assert!(r.caveats.iter().any(|c| c.contains("Small numbers")));
        assert!(r.caveats.iter().any(|c| c.to_lowercase().contains("review")));
        assert!(r.sources.iter().all(|s| s.retrieved.is_some()));
    }
}
