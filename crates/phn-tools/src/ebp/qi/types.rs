//! Arguments and results of `plan_quality_improvement`.

use phn_epi::spc::ChartType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::charts::{ChartFiles, ChartFormat};
use crate::ebp::spc::{FileMeasure, Period};
use crate::epr::content::{Confidence, CitedOut};

/// Parts of the plan. Default: all of them (the data analysis only when data are given).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    /// Model for Improvement questions and the aim statement.
    Aim,
    /// Outcome, process, and balancing measures with operational definitions.
    Measures,
    /// Driver diagram: primary drivers, secondary drivers, change ideas.
    Drivers,
    /// Fishbone (cause-and-effect) diagram.
    Fishbone,
    /// Process map (flowchart).
    ProcessMap,
    /// PDSA cycle plans.
    Pdsa,
    /// Run chart or control chart from your measure data.
    Analysis,
    /// SQUIRE 2.0 reporting checklist.
    Squire,
}

/// Which run chart rules to apply to a run chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleSet {
    /// IHI/Perla rules and Anhøj rules side by side (default).
    #[default]
    Both,
    /// IHI/Perla rules: shift of 6, trend of 5, too few or too many runs, astronomical point.
    Ihi,
    /// Anhøj rules: unusually long run and too few crossings.
    Anhoej,
}

/// Which way the aim should move the measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[serde(alias = "raise", alias = "improve", alias = "up")]
    Increase,
    #[serde(alias = "reduce", alias = "lower", alias = "down")]
    Decrease,
}

impl Direction {
    pub fn verb(self) -> &'static str {
        match self {
            Direction::Increase => "increase",
            Direction::Decrease => "decrease",
        }
    }
}

/// What the improvement project is trying to accomplish.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AimInput {
    /// increase or decrease.
    pub direction: Option<Direction>,
    /// What changes, in words: "the share of two-year-olds up to date on vaccines".
    pub what: Option<String>,
    /// For whom: the population (and setting): "children 24-35 months at the Elm Street clinic".
    pub population: Option<String>,
    /// Current level (baseline), as a number in `unit`.
    pub baseline: Option<f64>,
    /// Goal, as a number in `unit`.
    pub goal: Option<f64>,
    /// Unit: "%", "per 1,000", "minutes", and so on.
    pub unit: Option<String>,
    /// By when: a date (YYYY-MM-DD) or words such as "June 30, 2027" or "within 12 months".
    pub by: Option<String>,
}

/// Outcome, process, or balancing measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeasureKind {
    Outcome,
    Process,
    Balancing,
}

impl MeasureKind {
    pub fn label(self) -> &'static str {
        match self {
            MeasureKind::Outcome => "Outcome",
            MeasureKind::Process => "Process",
            MeasureKind::Balancing => "Balancing",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            MeasureKind::Outcome => "outcome",
            MeasureKind::Process => "process",
            MeasureKind::Balancing => "balancing",
        }
    }
}

/// One measure and its operational definition.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MeasureInput {
    pub name: String,
    pub kind: MeasureKind,
    /// Exactly what is counted.
    #[serde(default)]
    pub numerator: Option<String>,
    /// The group measured (leave out for a count or an average).
    #[serde(default)]
    pub denominator: Option<String>,
    #[serde(default)]
    pub data_source: Option<String>,
    /// How often it is measured and plotted ("monthly").
    #[serde(default)]
    pub frequency: Option<String>,
    /// All cases or a sample (and how chosen).
    #[serde(default)]
    pub sampling: Option<String>,
    /// Who collects and enters the data (a role, not a person's contact details).
    #[serde(default)]
    pub owner: Option<String>,
    /// The goal for this measure, in words.
    #[serde(default)]
    pub goal: Option<String>,
}

/// A secondary driver and the change ideas to test for it.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SecondaryInput {
    pub name: String,
    pub change_ideas: Vec<String>,
}

/// A primary driver with its secondary drivers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DriverInput {
    pub primary: String,
    pub secondary: Vec<SecondaryInput>,
}

/// A fishbone category.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CategoryInput {
    pub name: String,
    pub causes: Vec<String>,
}

/// The fishbone diagram.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct FishboneInput {
    /// The problem at the head of the fish (default: the aim's measure, or the `problem`).
    pub problem: Option<String>,
    /// Categories with causes. Default: people, methods, materials, equipment, environment
    /// (empty, to fill in with your team).
    pub categories: Vec<CategoryInput>,
}

/// What kind of process map step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepKindInput {
    Start,
    Step,
    Decision,
    End,
}

/// One step in the process as it happens today.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ProcessStepInput {
    pub name: String,
    /// start, step (default), decision, or end.
    pub kind: Option<StepKindInput>,
    /// Who does it (a role).
    pub owner: Option<String>,
    /// A delay, rework, or drop-off point.
    pub pain_point: Option<bool>,
    /// For a decision: the step a "no" goes to, by step number (1 is the first) or name.
    pub no_goes_to: Option<String>,
}

/// One PDSA cycle plan.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PdsaInput {
    /// The change to test (one change per cycle where possible).
    pub change: String,
    pub objective: Option<String>,
    /// What you predict will happen, written before the test.
    pub prediction: Option<String>,
    pub who: Option<String>,
    pub when: Option<String>,
    #[serde(rename = "where")]
    pub where_: Option<String>,
    /// How small the test is: "one nurse, 5 families, one week".
    pub scale: Option<String>,
    /// What you will collect to compare with the prediction.
    pub data_to_collect: Option<String>,
    /// How this cycle builds on the earlier one (for cycles after the first).
    pub builds_on: Option<String>,
}

/// Progress on a SQUIRE 2.0 item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SquireState {
    #[default]
    Todo,
    InProgress,
    Done,
    NotApplicable,
}

/// Status of one SQUIRE item.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SquireStatusInput {
    /// Item number, 1 to 18.
    pub item: u8,
    pub status: SquireState,
    /// Where in your draft it is addressed.
    pub where_addressed: Option<String>,
}

/// Arguments for `plan_quality_improvement`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct QiPlanArgs {
    /// Short project name, for the headings and file names.
    pub project_title: Option<String>,
    /// The problem in a sentence (heads the fishbone if no other problem is given).
    pub problem: Option<String>,
    /// Which parts to produce. Default: everything (the analysis only when data are given).
    pub sections: Vec<Section>,
    /// The aim (Model for Improvement, question 1).
    pub aim: Option<AimInput>,
    /// The family of measures (question 2): outcome, process, and balancing.
    pub measures: Vec<MeasureInput>,
    /// Driver diagram content (question 3). Leave out for a blank scaffold.
    pub drivers: Vec<DriverInput>,
    /// Fishbone categories and causes.
    pub fishbone: Option<FishboneInput>,
    /// The process as it happens today, step by step.
    pub process_steps: Vec<ProcessStepInput>,
    /// PDSA cycles to plan.
    pub pdsa_cycles: Vec<PdsaInput>,
    /// Progress on the SQUIRE 2.0 checklist.
    pub squire_status: Vec<SquireStatusInput>,

    /// Data: inline values in time order (numerators for a proportion or rate).
    pub values: Option<Vec<f64>>,
    /// Data: period labels for inline values.
    pub labels: Option<Vec<String>>,
    /// Data: inline denominators (proportions) or exposure (rates).
    pub denominators: Option<Vec<f64>>,
    /// Data: CSV/XLSX in /data. One row per event (use `date_column` and `measure_kind`) or one
    /// row per period (use `period_column` and `numerator_column`). Only aggregates are
    /// returned; small counts are hidden.
    pub path: Option<String>,
    pub sheet: Option<String>,
    /// Event file: date column used to group rows into periods.
    pub date_column: Option<String>,
    /// Event file: week, month (default), or quarter.
    pub period: Option<Period>,
    /// Event file: count, proportion (with `flag_column`), or mean (with `value_column`). A
    /// period file with no denominator column is treated as counts unless this is `mean`.
    pub measure_kind: Option<FileMeasure>,
    pub flag_column: Option<String>,
    pub value_column: Option<String>,
    /// Period file: the column with each period's label (for example "2026-01").
    pub period_column: Option<String>,
    /// Period file: numerator (or count, or value) column.
    pub numerator_column: Option<String>,
    /// Period file: denominator column, for a proportion.
    pub denominator_column: Option<String>,
    /// Points at the start of each phase that give the median or control limits (12 to 20 is
    /// typical). Default: all points in the phase.
    pub baseline_points: Option<usize>,
    /// Labels of the first period after each process change (a new median or limits start).
    pub phase_starts: Option<Vec<String>>,
    /// run (default), xmr, p, u, or c.
    pub chart_type: Option<ChartType>,
    /// For run charts: both (default), ihi, or anhoej.
    pub rule_set: Option<RuleSet>,
    /// Name of the measure, e.g. "% of children up to date".
    pub measure_name: Option<String>,
    /// Multiply proportions by this for display (default 100 = percent).
    pub display_scale: Option<f64>,
    /// png, svg, or both (default both).
    pub format: Option<ChartFormat>,

    /// Write the DOCX plan to /data/output (default true).
    pub write_plan: Option<bool>,
    /// Write the XLSX measurement tracker (live formulas) to /data/output (default true).
    pub write_tracker: Option<bool>,
    /// Write the diagrams as SVG and PNG to /data/output (default true).
    pub write_diagrams: Option<bool>,
}

// ---------------------------------------------------------------- results

/// Whether a check is met.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Met,
    /// Information missing from what you gave.
    Missing,
    /// Present, but worth a second look.
    Attention,
    /// A judgment for your team that the tool cannot check.
    Judgment,
}

/// One check on the plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Check {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    pub note: String,
}

/// One of the Model for Improvement's questions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOut {
    pub number: u8,
    pub question: String,
    pub how_the_tool_helps: String,
    pub citation: String,
}

/// The aim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AimOut {
    /// The aim statement built from what you gave (null when there is not enough to write one).
    pub statement: Option<String>,
    pub template: String,
    pub checks: Vec<Check>,
    pub examples: Vec<CitedOut>,
}

/// A measure type with prompts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasureTypeOut {
    pub id: String,
    pub label: String,
    pub plain: String,
    pub prompts: Vec<String>,
    pub example: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// A measure as entered, with what is missing from its definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasureRowOut {
    pub name: String,
    pub kind: String,
    pub numerator: Option<String>,
    pub denominator: Option<String>,
    pub data_source: Option<String>,
    pub frequency: Option<String>,
    pub sampling: Option<String>,
    pub owner: Option<String>,
    pub goal: Option<String>,
    /// Operational definition fields still blank.
    pub missing: Vec<String>,
}

/// The measures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasuresOut {
    pub types: Vec<MeasureTypeOut>,
    pub measures: Vec<MeasureRowOut>,
    pub checks: Vec<Check>,
    pub definition_fields: Vec<String>,
    pub note: CitedOut,
}

/// A diagram in every form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DiagramOut {
    /// True when no content was given and the diagram is a blank scaffold.
    pub is_scaffold: bool,
    /// Mermaid source (paste into any Mermaid renderer).
    pub mermaid: String,
    /// Text alternative: an outline of the diagram.
    pub outline: Vec<String>,
    /// Short description (also the image alt text).
    pub alt_text: String,
    pub svg_file: Option<String>,
    pub png_file: Option<String>,
    /// Guidance on building it with your team.
    pub guidance: Vec<CitedOut>,
}

/// One PDSA cycle with what is missing from its plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CycleOut {
    pub number: usize,
    pub change: String,
    pub objective: Option<String>,
    pub prediction: Option<String>,
    pub who: Option<String>,
    pub when: Option<String>,
    #[serde(rename = "where")]
    pub where_: Option<String>,
    pub scale: Option<String>,
    pub data_to_collect: Option<String>,
    pub builds_on: Option<String>,
    pub missing: Vec<String>,
}

/// A PDSA stage with its questions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageOut {
    pub id: String,
    pub label: String,
    pub questions: Vec<String>,
}

/// PDSA planning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PdsaOut {
    pub description: CitedOut,
    pub stages: Vec<StageOut>,
    pub cycles: Vec<CycleOut>,
    /// Checks against the application criteria used in a systematic review of PDSA reports.
    pub checks: Vec<Check>,
}

/// One period in the analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisRow {
    pub label: String,
    /// Null when hidden by small-cell suppression.
    pub value: Option<f64>,
    /// Median (run chart) or center line. Omitted when any period is suppressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lcl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ucl: Option<f64>,
    /// Short names of the rules that flag this period.
    pub signals: Vec<String>,
    pub suppressed: bool,
}

/// Summary of one phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhaseOut {
    pub first_period: String,
    pub last_period: String,
    pub baseline_points: usize,
    /// Median or center line (omitted when any period is suppressed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<f64>,
    /// Run chart: points not on the median.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub useful_points: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_run: Option<usize>,
    /// Anhøj: the longest run is a signal when it is longer than this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_run_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crossings: Option<usize>,
    /// Anhøj: too few crossings when fewer than this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crossings_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_trend: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<usize>,
    /// IHI: too few runs when the number of runs is at most this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_too_few_at_most: Option<usize>,
    /// IHI: too many runs when the number of runs is at least this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_too_many_at_least: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_moving_range: Option<f64>,
}

/// A signal found by a rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    /// ihi, anhoej, or control.
    pub rule_set: String,
    pub rule: String,
    /// The periods involved ("2026-03 to 2026-09"), or "whole series".
    pub periods: String,
    pub meaning: String,
}

/// The analysis of the nurse's measure data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisOut {
    pub data_from: String,
    pub chart_type: ChartType,
    pub rule_set: Option<RuleSet>,
    pub periods: usize,
    pub rows: Vec<AnalysisRow>,
    pub phases: Vec<PhaseOut>,
    pub findings: Vec<Finding>,
    /// Reading the result (data sufficiency, rule disagreement, direction).
    pub notes: Vec<String>,
    pub chart: Option<ChartFiles>,
    /// Mermaid line chart of the series (omitted when values are hidden).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mermaid_chart: Option<String>,
    /// Text alternative for the chart.
    pub text_alternative: String,
    /// True when center lines and limits are left out because a period is suppressed.
    pub values_withheld: bool,
}

/// One SQUIRE 2.0 item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SquireItemOut {
    pub number: u8,
    pub name: String,
    pub section: String,
    /// This tool's paraphrase of what the item asks for.
    pub purpose: String,
    /// A question to help you write it.
    pub ask: String,
    pub status: SquireState,
    pub where_addressed: Option<String>,
    /// Where in this plan the material comes from.
    pub plan_section: String,
}

/// SQUIRE 2.0 checklist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SquireOut {
    pub url: String,
    pub license_note: String,
    pub cite_as: String,
    pub note_to_authors: CitedOut,
    pub items: Vec<SquireItemOut>,
    pub done: usize,
    pub in_progress: usize,
    pub todo: usize,
    pub not_applicable: usize,
}

/// Result of `plan_quality_improvement`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QiPlanResult {
    pub project_title: Option<String>,
    pub questions: Vec<QuestionOut>,
    pub aim: Option<AimOut>,
    pub measures: Option<MeasuresOut>,
    pub driver_diagram: Option<DiagramOut>,
    pub fishbone: Option<DiagramOut>,
    pub process_map: Option<DiagramOut>,
    pub pdsa: Option<PdsaOut>,
    pub analysis: Option<AnalysisOut>,
    pub squire: Option<SquireOut>,
    /// Files written to the output folder, relative to /data.
    pub files: Vec<String>,
    pub next_steps: Vec<String>,
}
