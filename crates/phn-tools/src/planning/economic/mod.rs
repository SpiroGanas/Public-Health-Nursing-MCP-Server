//! PLAN-10 `economic_evaluation_calculator`: cost per participant and per outcome, average
//! cost-effectiveness ratio and ICER (with dominance), net benefit, benefit-cost ratio, return on
//! investment, break-even, discounting over a multi-year horizon, and one-way sensitivity analysis
//! with a tornado table and an accessible chart. Plain-language interpretation is guarded against
//! over-claiming (decision support only).
//!
//! Content is `content/planning/economic_evaluation.yaml` (pending review). The arithmetic is in
//! [`calc`]; golden tests compare it with the published benefit-cost results in CDC's MMWR 2024
//! and with an independent Python computation.

pub mod calc;

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::chart::{ChartKind, ChartSpec, Series};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use self::calc::{Comparison, Metric, Model, Parameter, SensRow, metrics, one_way};
use crate::charts::{ChartFiles, ChartFormat, write_chart};
use crate::context::{Context, error_message};
use crate::epr::content::{self as econtent, Cited, DocSource};
use crate::epr::roster;
use crate::md;

/// Content id of `content/planning/economic_evaluation.yaml`.
pub const ECONOMIC_CONTENT: &str = "planning-economic-evaluation";

/// Longest horizon (years).
pub const MAX_HORIZON: usize = 30;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Defaults {
    discount_rate_percent: f64,
    discount_rate_source: String,
    discount_rate_section: String,
    discount_rate_confidence: econtent::Confidence,
    discount_rate_note: String,
    discount_rate_sensitivity_low_percent: f64,
    discount_rate_sensitivity_high_percent: f64,
    sensitivity_swing_percent: f64,
    sensitivity_swing_note: String,
    first_year_discounted: bool,
    timing_note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Definition {
    id: String,
    label: String,
    formula: String,
    text: String,
    source: String,
    section: String,
    confidence: econtent::Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct PerspectiveDef {
    id: String,
    label: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Interpretation {
    always: Vec<Cited>,
    no_comparator: Cited,
    dominant: Cited,
    dominated: Cited,
    smaller_cheaper: Cited,
    no_difference: Cited,
    bcr_below_one: Cited,
    threshold_none: Cited,
    sensitivity_flip: Cited,
    sensitivity_robust: Cited,
}

#[derive(Debug, Clone, Deserialize)]
struct EconContent {
    sources: BTreeMap<String, DocSource>,
    defaults: Defaults,
    definitions: Vec<Definition>,
    perspectives: Vec<PerspectiveDef>,
    interpretation: Interpretation,
}

// ---------------------------------------------------------------- arguments

/// Whose costs and benefits are counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Perspective {
    /// The program's own budget (default).
    #[default]
    Program,
    /// Payer or health care sector spending.
    Payer,
    /// Everyone affected, inside or outside health care.
    Societal,
}

impl Perspective {
    fn id(self) -> &'static str {
        match self {
            Perspective::Program => "program",
            Perspective::Payer => "payer",
            Perspective::Societal => "societal",
        }
    }
}

/// A comparison alternative (usual care, no program, another program).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ComparatorArg {
    /// Comparator total cost (counted in year 1), or use `costs_by_year`.
    pub cost: Option<f64>,
    /// Comparator cost in each year (year 1 first).
    pub costs_by_year: Vec<f64>,
    /// Comparator total outcomes (counted in year 1), or use `outcomes_by_year`. Use 0 for "no
    /// program".
    pub outcomes: Option<f64>,
    /// Comparator outcomes in each year.
    pub outcomes_by_year: Vec<f64>,
}

/// A range for one-way sensitivity analysis.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SensitivityRange {
    /// The number to vary.
    pub parameter: Parameter,
    /// Lower value to test (for totals: the total over the horizon; the yearly amounts are
    /// scaled; the discount rate is in percent).
    pub low: f64,
    /// Higher value to test.
    pub high: f64,
}

/// Arguments for `economic_evaluation_calculator`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct EconomicArgs {
    /// Name of the program (names the chart file).
    pub program_name: Option<String>,
    /// `program` (default), `payer`, or `societal`: whose costs and benefits are counted. Say
    /// which when you report results.
    pub perspective: Perspective,
    /// People served.
    pub participants: Option<f64>,
    /// Total program cost (counted in year 1), or use `costs_by_year`.
    pub program_cost: Option<f64>,
    /// Program cost in each year (year 1 first). The longest list sets the horizon (up to 30).
    pub costs_by_year: Vec<f64>,
    /// Total outcomes achieved (cases averted, people who quit smoking; counted in year 1), or
    /// use `outcomes_by_year`.
    pub outcomes: Option<f64>,
    /// Outcomes in each year.
    pub outcomes_by_year: Vec<f64>,
    /// Dollar value of one outcome, from a source you can cite. Leave out to skip benefit-cost
    /// measures.
    pub benefit_per_outcome: Option<f64>,
    /// Other monetized benefits (savings, revenue) counted in year 1, or use
    /// `other_benefits_by_year`.
    pub other_benefits: Option<f64>,
    /// Other monetized benefits in each year.
    pub other_benefits_by_year: Vec<f64>,
    /// An alternative to compare with, for the incremental cost-effectiveness ratio (ICER).
    pub comparator: Option<ComparatorArg>,
    /// Discount rate in percent for costs and dollar benefits (default 3).
    pub discount_rate_percent: Option<f64>,
    /// Discount rate for outcome counts in percent (default: the same as the cost rate).
    pub outcome_discount_rate_percent: Option<f64>,
    /// Also discount year 1 by one year (default false: year 1 is at the start and not
    /// discounted).
    pub first_year_discounted: Option<bool>,
    /// What a decision maker would pay for one outcome. Only used if you give it (net monetary
    /// benefit); the tool applies no threshold of its own.
    pub willingness_to_pay: Option<f64>,
    /// Ranges for the one-way sensitivity analysis. Numbers you leave out get the default
    /// range (20 percent either way, and 0 to 7 percent for the discount rate).
    pub sensitivity: Vec<SensitivityRange>,
    /// Result tracked in the tornado table: `benefit_cost_ratio`, `return_on_investment`,
    /// `net_benefit`, `cost_per_outcome`, `cost_per_participant`, or `icer`. Default: the
    /// benefit-cost ratio when benefits are given, otherwise the ICER with a comparator, otherwise
    /// cost per outcome.
    pub sensitivity_metric: Option<Metric>,
    /// Run the sensitivity analysis (default true).
    pub run_sensitivity: Option<bool>,
    /// Write the tornado chart to /data/output/charts (default true).
    pub write_chart: Option<bool>,
    /// Chart format: png, svg, or both (default both).
    pub chart_format: Option<ChartFormat>,
}

// ---------------------------------------------------------------- results

/// What was costed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CostsOut {
    pub total_undiscounted: f64,
    pub present_value: f64,
    pub per_participant_undiscounted: Option<f64>,
    pub per_participant_present_value: Option<f64>,
}

/// Outcomes and the cost per outcome (no comparator).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutcomesOut {
    pub total_undiscounted: f64,
    pub present_value: f64,
    /// Average cost per outcome against doing nothing (not an incremental ratio).
    pub cost_per_outcome_undiscounted: Option<f64>,
    pub cost_per_outcome_present_value: Option<f64>,
}

/// Benefit measures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BenefitsOut {
    pub total_undiscounted: f64,
    pub present_value: f64,
    /// Benefits minus cost, in present value.
    pub net_benefit: Option<f64>,
    /// Present-value benefits divided by present-value cost.
    pub benefit_cost_ratio: Option<f64>,
    /// Net benefit divided by cost.
    pub return_on_investment: Option<f64>,
    /// First year the cumulative discounted net benefit is zero or more.
    pub payback_year: Option<usize>,
}

/// Break-even values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BreakEvenOut {
    /// Dollar value of one outcome at which benefits equal cost (benefit-cost ratio of 1).
    pub value_per_outcome: Option<f64>,
    /// Share of the outcomes you expect that must occur for benefits to equal cost (at your
    /// value per outcome); above 100 percent means the program does not break even.
    pub share_of_expected_outcomes_needed: Option<f64>,
}

/// Comparison with the alternative.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComparisonOut {
    pub incremental_cost: f64,
    pub incremental_outcomes: f64,
    pub classification: Comparison,
    pub classification_text: String,
    /// Incremental cost-effectiveness ratio ($ per outcome), when it is meaningful.
    pub icer: Option<f64>,
    /// Net monetary benefit at the willingness to pay you gave.
    pub net_monetary_benefit: Option<f64>,
}

/// One row of the tornado table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TornadoRow {
    pub parameter: String,
    pub base_value: f64,
    pub low_value: f64,
    pub high_value: f64,
    /// The tracked result at the low value ("not defined" when it has no meaning there).
    pub result_at_low: Option<f64>,
    pub result_at_high: Option<f64>,
    /// Absolute difference between the two results.
    pub swing: f64,
    /// True when the conclusion (favorable or not) differs inside this range.
    pub changes_conclusion: bool,
    /// True when the range was the tool's default (20 percent either way), not yours.
    pub default_range: bool,
}

/// The sensitivity analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SensitivityOut {
    pub metric: String,
    pub base_result: Option<f64>,
    /// Rows sorted by swing, largest first.
    pub tornado: Vec<TornadoRow>,
    /// Numbers that were varied but did not change this result.
    pub no_effect: Vec<String>,
    pub chart: Option<ChartFiles>,
    /// Text alternative for the chart: the same facts in sentences.
    pub chart_text_alternative: String,
    pub note: String,
}

/// Discounting facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DiscountOut {
    pub rate_percent: f64,
    pub outcome_rate_percent: f64,
    pub horizon_years: usize,
    pub first_year_discounted: bool,
    pub timing_note: String,
    /// Where the default rate comes from, when the default was used.
    pub rate_source: String,
}

/// Definition of a measure used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DefinitionOut {
    pub measure: String,
    pub formula: String,
    pub meaning: String,
    pub citation: String,
    pub confidence: econtent::Confidence,
}

/// Result of `economic_evaluation_calculator`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EconomicResult {
    pub program: String,
    pub perspective: String,
    pub perspective_text: String,
    pub discounting: DiscountOut,
    pub costs: CostsOut,
    pub outcomes: Option<OutcomesOut>,
    pub benefits: Option<BenefitsOut>,
    pub break_even: Option<BreakEvenOut>,
    pub comparison: Option<ComparisonOut>,
    pub sensitivity: Option<SensitivityOut>,
    /// Plain-language reading, with the limits of what the numbers show.
    pub interpretation: Vec<String>,
    pub definitions: Vec<DefinitionOut>,
}

// ---------------------------------------------------------------- helpers

const MAX_NAME: usize = 120;

fn check_num(name: &str, v: f64, positive: bool) -> Result<()> {
    if !v.is_finite() || v < 0.0 || (positive && v == 0.0) || v > 1e15 {
        return Err(PhnError::InvalidInput(format!(
            "{name} must be {} number",
            if positive {
                "a positive"
            } else {
                "zero or a positive"
            }
        )));
    }
    Ok(())
}

fn stream(name: &str, total: Option<f64>, by_year: &[f64]) -> Result<Vec<f64>> {
    if total.is_some() && !by_year.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "give {name} as a total or by year, not both"
        )));
    }
    let v: Vec<f64> = if let Some(t) = total {
        vec![t]
    } else {
        by_year.to_vec()
    };
    if v.len() > MAX_HORIZON {
        return Err(PhnError::InvalidInput(format!(
            "{name} has {} years; the horizon is at most {MAX_HORIZON}",
            v.len()
        )));
    }
    for x in &v {
        check_num(name, *x, false)?;
    }
    Ok(v)
}

fn money(v: f64) -> String {
    super::budget::fmt_money(v)
}

fn ratio(v: f64) -> String {
    md::num(v, 2)
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn opt2(v: Option<f64>) -> Option<f64> {
    v.map(r2)
}

fn default_ranges(
    m: &Model,
    d: &Defaults,
    metric: Metric,
    given: &[SensitivityRange],
) -> Vec<(Parameter, f64, f64, bool)> {
    let mut out: Vec<(Parameter, f64, f64, bool)> = given
        .iter()
        .map(|g| (g.parameter, g.low, g.high, false))
        .collect();
    let swing = d.sensitivity_swing_percent / 100.0;
    for p in Parameter::ALL {
        if out.iter().any(|(q, ..)| *q == p) {
            continue;
        }
        let Some(b) = p.base(m) else { continue };
        let (lo, hi) = match p {
            Parameter::DiscountRate => (
                d.discount_rate_sensitivity_low_percent,
                d.discount_rate_sensitivity_high_percent,
            ),
            _ => (b * (1.0 - swing), b * (1.0 + swing)),
        };
        // Leave out numbers the tracked result cannot depend on.
        if p == Parameter::Participants && metric != Metric::CostPerParticipant {
            continue;
        }
        out.push((p, lo, hi, true));
    }
    out
}

fn tornado_text(metric: Metric, base: Option<f64>, rows: &[SensRow]) -> String {
    let fmt = |v: Option<f64>| v.map(ratio).unwrap_or_else(|| "not defined".into());
    let mut s = format!(
        "Tornado chart of the {}: base value {}. Bars are sorted from the number that moves the result most to the one that moves it least. ",
        metric.label(),
        fmt(base)
    );
    for r in rows {
        s.push_str(&format!(
            "{}: from {} to {} gives {} to {}. ",
            r.parameter.label(),
            ratio(r.low_value),
            ratio(r.high_value),
            fmt(r.result_low),
            fmt(r.result_high)
        ));
    }
    s.trim_end().to_string()
}

// ---------------------------------------------------------------- the tool

/// Economic evaluation calculator.
pub fn economic_evaluation_calculator(
    ctx: &Context,
    args: EconomicArgs,
) -> Result<Envelope<EconomicResult>> {
    let c: EconContent = econtent::load(ECONOMIC_CONTENT)?;
    let d = &c.defaults;
    let program = match args
        .program_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) if s.chars().count() > MAX_NAME => {
            return Err(PhnError::InvalidInput(format!(
                "program_name is longer than {MAX_NAME} characters"
            )));
        }
        Some(s) => s.to_string(),
        None => "Program".into(),
    };
    let costs = stream("program cost", args.program_cost, &args.costs_by_year)?;
    if costs.iter().sum::<f64>() <= 0.0 {
        return Err(PhnError::InvalidInput(
            "give the program cost (program_cost, or costs_by_year): it must be more than zero"
                .into(),
        ));
    }
    let outcomes = stream("outcomes", args.outcomes, &args.outcomes_by_year)?;
    let other = stream(
        "other benefits",
        args.other_benefits,
        &args.other_benefits_by_year,
    )?;
    for (name, v) in [
        ("participants", args.participants),
        ("benefit_per_outcome", args.benefit_per_outcome),
        ("willingness_to_pay", args.willingness_to_pay),
    ] {
        if let Some(x) = v {
            check_num(name, x, name == "participants")?;
        }
    }
    if args.benefit_per_outcome.is_some() && outcomes.iter().sum::<f64>() <= 0.0 {
        return Err(PhnError::InvalidInput(
            "benefit_per_outcome needs outcomes (outcomes or outcomes_by_year)".into(),
        ));
    }
    let (comp_costs, comp_outcomes) = match &args.comparator {
        None => (None, None),
        Some(cmp) => {
            let cc = stream("comparator cost", cmp.cost, &cmp.costs_by_year)?;
            let co = stream("comparator outcomes", cmp.outcomes, &cmp.outcomes_by_year)?;
            if cc.is_empty() || co.is_empty() {
                return Err(PhnError::InvalidInput(
                    "a comparator needs both a cost and outcomes (use outcomes 0 for no program)"
                        .into(),
                ));
            }
            (Some(cc), Some(co))
        }
    };
    if comp_costs.is_some() && outcomes.iter().sum::<f64>() <= 0.0 {
        return Err(PhnError::InvalidInput(
            "a comparison needs the program's outcomes".into(),
        ));
    }
    let rate_pct = args
        .discount_rate_percent
        .unwrap_or(d.discount_rate_percent);
    let out_rate_pct = args.outcome_discount_rate_percent.unwrap_or(rate_pct);
    for (n, r) in [
        ("discount_rate_percent", rate_pct),
        ("outcome_discount_rate_percent", out_rate_pct),
    ] {
        if !r.is_finite() || !(0.0..=100.0).contains(&r) {
            return Err(PhnError::InvalidInput(format!(
                "{n} must be between 0 and 100"
            )));
        }
    }
    let fy = args
        .first_year_discounted
        .unwrap_or(d.first_year_discounted);
    let model = Model {
        costs,
        outcomes,
        other_benefits: other,
        benefit_per_outcome: args.benefit_per_outcome,
        participants: args.participants,
        comp_costs,
        comp_outcomes,
        rate: rate_pct / 100.0,
        outcome_rate: out_rate_pct / 100.0,
        first_year_discounted: fy,
        willingness_to_pay: args.willingness_to_pay,
    };
    let horizon = [
        model.costs.len(),
        model.outcomes.len(),
        model.other_benefits.len(),
    ]
    .into_iter()
    .chain(model.comp_costs.as_ref().map(Vec::len))
    .chain(model.comp_outcomes.as_ref().map(Vec::len))
    .max()
    .unwrap_or(1)
    .max(1);
    let x = metrics(&model);

    // ---- result parts
    let has_outcomes = x.total_outcomes > 0.0;
    let costs_out = CostsOut {
        total_undiscounted: r2(x.total_cost),
        present_value: r2(x.pv_cost),
        per_participant_undiscounted: opt2(x.cost_per_participant),
        per_participant_present_value: opt2(x.cost_per_participant_pv),
    };
    let outcomes_out = has_outcomes.then(|| OutcomesOut {
        total_undiscounted: r2(x.total_outcomes),
        present_value: r2(x.pv_outcomes),
        cost_per_outcome_undiscounted: opt2(x.cost_per_outcome),
        cost_per_outcome_present_value: opt2(x.cost_per_outcome_pv),
    });
    let benefits_out = x.has_benefits.then(|| BenefitsOut {
        total_undiscounted: r2(x.total_benefits),
        present_value: r2(x.pv_benefits),
        net_benefit: opt2(x.net_benefit),
        benefit_cost_ratio: x.bcr.map(|v| (v * 1000.0).round() / 1000.0),
        return_on_investment: x.roi.map(|v| (v * 1000.0).round() / 1000.0),
        payback_year: x.payback_year,
    });
    let break_even_out = x
        .break_even_value_per_outcome
        .is_some()
        .then(|| BreakEvenOut {
            value_per_outcome: opt2(x.break_even_value_per_outcome),
            share_of_expected_outcomes_needed: x
                .break_even_outcome_share
                .map(|v| (v * 1000.0).round() / 1000.0),
        });
    let comparison_out = match (x.comparison, x.delta_cost, x.delta_outcomes) {
        (Some(cl), Some(dc), Some(de)) => Some(ComparisonOut {
            incremental_cost: r2(dc),
            incremental_outcomes: r2(de),
            classification: cl,
            classification_text: cl.label().to_string(),
            icer: opt2(x.icer),
            net_monetary_benefit: opt2(x.nmb),
        }),
        _ => None,
    };

    // ---- sensitivity
    let mut caveats: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let want_sens = args.run_sensitivity.unwrap_or(true);
    let metric = args.sensitivity_metric.unwrap_or(if x.has_benefits {
        Metric::BenefitCostRatio
    } else if x.icer.is_some() || x.comparison.is_some() {
        Metric::Icer
    } else if has_outcomes {
        Metric::CostPerOutcome
    } else {
        Metric::CostPerParticipant
    });
    let mut sensitivity_out = None;
    let mut flips: Vec<String> = Vec::new();
    let mut defaulted = false;
    if want_sens {
        let base_value = metric.value(&x);
        if base_value.is_none() {
            return Err(PhnError::InvalidInput(format!(
                "the {} is not defined for these inputs ({}); choose another sensitivity_metric or add the missing numbers",
                metric.label(),
                match metric {
                    Metric::Icer =>
                        "it needs a comparator whose result is more or less costly and more or less effective, not dominant or dominated",
                    Metric::CostPerParticipant => "it needs participants",
                    Metric::CostPerOutcome => "it needs outcomes",
                    _ => "it needs benefits: benefit_per_outcome or other_benefits",
                }
            )));
        }
        let ranges = default_ranges(&model, d, metric, &args.sensitivity);
        for g in &args.sensitivity {
            if !g.low.is_finite() || !g.high.is_finite() || g.low < 0.0 || g.high < 0.0 {
                return Err(PhnError::InvalidInput(
                    "sensitivity ranges must be zero or positive numbers".into(),
                ));
            }
            if Parameter::base(g.parameter, &model).is_none() {
                return Err(PhnError::InvalidInput(format!(
                    "sensitivity on '{}' needs that number in your inputs",
                    g.parameter.label()
                )));
            }
        }
        let tuples: Vec<(Parameter, f64, f64)> =
            ranges.iter().map(|(p, l, h, _)| (*p, *l, *h)).collect();
        let rows = one_way(&model, metric, &tuples);
        let mut no_effect: Vec<String> = rows
            .iter()
            .filter(|r| r.swing == 0.0)
            .map(|r| r.parameter.label().to_string())
            .collect();
        no_effect.sort();
        let rows: Vec<SensRow> = rows.into_iter().filter(|r| r.swing > 0.0).collect();
        defaulted = ranges
            .iter()
            .any(|(p, _, _, dflt)| *dflt && rows.iter().any(|r| r.parameter == *p));
        flips = rows
            .iter()
            .filter(|r| r.flips)
            .map(|r| r.parameter.label().to_lowercase())
            .collect();
        let tornado: Vec<TornadoRow> = rows
            .iter()
            .map(|r| TornadoRow {
                parameter: r.parameter.label().to_string(),
                base_value: r2(r.base_value),
                low_value: r2(r.low_value),
                high_value: r2(r.high_value),
                result_at_low: r.result_low.map(|v| (v * 1000.0).round() / 1000.0),
                result_at_high: r.result_high.map(|v| (v * 1000.0).round() / 1000.0),
                swing: (r.swing * 1000.0).round() / 1000.0,
                changes_conclusion: r.flips,
                default_range: ranges
                    .iter()
                    .any(|(p, _, _, dflt)| *p == r.parameter && *dflt),
            })
            .collect();
        let text_alt = tornado_text(metric, base_value, &rows);
        // Chart: one row per number; the point is the base result, the line spans the low and
        // high results.
        let mut chart = None;
        if args.write_chart.unwrap_or(true) && !rows.is_empty() {
            let base = base_value.unwrap_or(0.0);
            let mut spec = ChartSpec::new(
                ChartKind::Dot,
                format!("One-way sensitivity of the {}", metric.label()),
                rows.iter()
                    .map(|r| r.parameter.label().to_string())
                    .collect(),
            );
            spec.subtitle = Some(format!(
                "Base result {}; each number varied alone",
                ratio(base)
            ));
            spec.x_label = Some(metric.label().to_string());
            spec.series = vec![Series {
                name: "Base result".into(),
                values: rows.iter().map(|_| Some(base)).collect(),
                lower: Some(
                    rows.iter()
                        .map(|r| {
                            Some(
                                r.result_low
                                    .unwrap_or(base)
                                    .min(r.result_high.unwrap_or(base)),
                            )
                        })
                        .collect(),
                ),
                upper: Some(
                    rows.iter()
                        .map(|r| {
                            Some(
                                r.result_low
                                    .unwrap_or(base)
                                    .max(r.result_high.unwrap_or(base)),
                            )
                        })
                        .collect(),
                ),
                suppressed: Vec::new(),
            }];
            spec.include_zero = Some(false);
            spec.note = Some("Bars show the result when the number is set to its low and high value, others held at base. Sorted by swing.".into());
            let stem = format!(
                "tornado_{}",
                phn_local::sandbox::sanitize_file_name(&program)
            );
            match write_chart(ctx, &spec, &stem, args.chart_format.unwrap_or_default()) {
                Ok((f, _)) => {
                    files.extend(f.png.iter().chain(f.svg.iter()).cloned());
                    chart = Some(f);
                }
                Err(e) => caveats.push(format!(
                    "The tornado chart was not written: {}",
                    error_message(&e)
                )),
            }
        }
        sensitivity_out = Some(SensitivityOut {
            metric: metric.label().to_string(),
            base_result: base_value.map(|v| (v * 1000.0).round() / 1000.0),
            tornado,
            no_effect,
            chart,
            chart_text_alternative: text_alt,
            note: d
                .sensitivity_swing_note
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        });
    }

    // ---- interpretation
    let it = &c.interpretation;
    let mut reading: Vec<String> = Vec::new();
    let fill = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let persp = c
        .perspectives
        .iter()
        .find(|p| p.id == args.perspective.id())
        .cloned()
        .unwrap_or_else(|| c.perspectives[0].clone());
    reading.push(format!(
        "Perspective: {} {}",
        persp.label,
        fill(&persp.text)
    ));
    if let Some(cpp) = x.cost_per_participant_pv {
        reading.push(format!(
            "The program costs {} per participant ({} in present value). That is an average and does not show that anyone benefited.",
            money(x.cost_per_participant.unwrap_or(cpp)),
            money(cpp)
        ));
    }
    if let Some(cpo) = x.cost_per_outcome_pv {
        reading.push(format!(
            "Counting {:.1} outcomes (present value {:.1}), the average cost is {} per outcome in present value.",
            x.total_outcomes,
            x.pv_outcomes,
            money(cpo)
        ));
        if x.comparison.is_none() {
            reading.push(fill(&it.no_comparator.text));
        }
    }
    if let (Some(b), Some(nb)) = (x.bcr, x.net_benefit) {
        reading.push(format!(
            "In present value the benefits you entered are {} against a cost of {}: a net benefit of {} and a benefit-cost ratio of {} ({} of benefit counted for each $1 spent).",
            money(x.pv_benefits),
            money(x.pv_cost),
            money(nb),
            ratio(b),
            money(b)
        ));
        if b < 1.0 {
            reading.push(fill(&it.bcr_below_one.text));
        }
    }
    if let Some(cl) = x.comparison {
        let (dc, de) = (x.delta_cost.unwrap_or(0.0), x.delta_outcomes.unwrap_or(0.0));
        let head = format!(
            "Compared with the alternative the program is {}: it costs {} {} and achieves {:.1} {} outcomes (present values).",
            cl.label(),
            money(dc.abs()),
            if dc >= 0.0 { "more" } else { "less" },
            de.abs(),
            if de >= 0.0 { "more" } else { "fewer" }
        );
        reading.push(head);
        match cl {
            Comparison::MoreCostlyMoreEffective => reading.push(format!(
                "The incremental cost-effectiveness ratio is {} for each additional outcome. Whether that is worth paying is a decision for the people who fund and use the program.",
                money(x.icer.unwrap_or(0.0))
            )),
            Comparison::LessCostlyLessEffective => reading.push(format!(
                "{} The ratio is {} saved for each outcome given up.",
                fill(&it.smaller_cheaper.text),
                money(x.icer.unwrap_or(0.0))
            )),
            Comparison::Dominant => reading.push(fill(&it.dominant.text)),
            Comparison::Dominated => reading.push(fill(&it.dominated.text)),
            Comparison::NoDifference => reading.push(fill(&it.no_difference.text)),
        }
    }
    match x.nmb {
        Some(v) => reading.push(format!(
            "At the willingness to pay you entered ({} per outcome) the net monetary benefit is {}. That threshold is yours, not the tool's.",
            money(args.willingness_to_pay.unwrap_or(0.0)),
            money(v)
        )),
        None if x.comparison.is_some() => reading.push(fill(&it.threshold_none.text)),
        None => {}
    }
    if want_sens {
        if flips.is_empty() {
            if metric.favorable(&x).is_some() {
                reading.push(fill(&it.sensitivity_robust.text));
            }
        } else {
            reading
                .push(fill(&it.sensitivity_flip.text).replace("{parameters}", &flips.join(", ")));
        }
    }
    for a in &it.always {
        reading.push(fill(&a.text));
    }

    // ---- caveats, sources, privacy
    let rate_is_default = args.discount_rate_percent.is_none();
    if rate_is_default {
        caveats.push(format!(
            "Discount rate {}% is the default: {}",
            d.discount_rate_percent,
            fill(&d.discount_rate_note)
        ));
    }
    caveats.push(fill(&d.timing_note));
    if defaulted {
        caveats.push(fill(&d.sensitivity_swing_note));
    }
    let uneven = [
        model.costs.len(),
        model.outcomes.len(),
        model.other_benefits.len(),
    ]
    .into_iter()
    .filter(|l| *l != 0 && *l != horizon)
    .count();
    if uneven > 0 {
        caveats.push(format!("Some lists are shorter than the {horizon}-year horizon; the missing years were counted as zero."));
    }
    caveats.push("This is a calculation from the numbers you entered; it does not check that the program caused the outcomes or that the dollar values are right. Decision support only: the decision rests with the people who fund and run the program.".into());
    caveats.extend(econtent::review_caveats(&[ECONOMIC_CONTENT]));

    let mut s = format!(
        "**{program}** ({} perspective, {horizon}-year horizon, {rate_pct}% discount rate)\n\n",
        persp.label
    );
    let mut rows: Vec<Vec<String>> = vec![vec![
        "Total cost".into(),
        money(x.total_cost),
        money(x.pv_cost),
    ]];
    if x.total_outcomes > 0.0 {
        rows.push(vec![
            "Outcomes".into(),
            format!("{:.1}", x.total_outcomes),
            format!("{:.1}", x.pv_outcomes),
        ]);
    }
    if x.has_benefits {
        rows.push(vec![
            "Benefits".into(),
            money(x.total_benefits),
            money(x.pv_benefits),
        ]);
    }
    s.push_str(&md::table(
        &["Measure", "Undiscounted", "Present value"],
        &rows,
    ));
    s.push('\n');
    let mut key: Vec<String> = Vec::new();
    if let Some(v) = x.cost_per_participant_pv {
        key.push(format!("cost per participant {}", money(v)));
    }
    if let Some(v) = x.cost_per_outcome_pv {
        key.push(format!("cost per outcome {}", money(v)));
    }
    if let Some(v) = x.bcr {
        key.push(format!("benefit-cost ratio {}", ratio(v)));
    }
    if let Some(v) = x.roi {
        key.push(format!("return on investment {}", md::num(v * 100.0, 0)) + "%");
    }
    if let Some(v) = x.icer {
        key.push(format!("ICER {} per outcome", money(v)));
    }
    if let Some(v) = x.break_even_value_per_outcome {
        key.push(format!("break-even value per outcome {}", money(v)));
    }
    if !key.is_empty() {
        s.push_str(&format!("Key results: {}.\n\n", key.join("; ")));
    }
    for r in &reading {
        s.push_str(&format!("- {r}\n"));
    }
    if let Some(so) = &sensitivity_out {
        if !so.tornado.is_empty() {
            s.push_str(&format!(
                "\nTornado table ({}), largest swing first:\n\n",
                so.metric
            ));
            let rows: Vec<Vec<String>> = so
                .tornado
                .iter()
                .map(|t| {
                    vec![
                        t.parameter.clone(),
                        format!("{} to {}", ratio(t.low_value), ratio(t.high_value)),
                        t.result_at_low.map(ratio).unwrap_or_else(|| "n/a".into()),
                        t.result_at_high.map(ratio).unwrap_or_else(|| "n/a".into()),
                        if t.changes_conclusion {
                            "yes".into()
                        } else {
                            "no".into()
                        },
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &[
                    "Number varied",
                    "Range",
                    "Result at low",
                    "Result at high",
                    "Changes conclusion",
                ],
                &rows,
            ));
        }
    }

    let mut keys: Vec<&str> = vec!["tool-definition", "ajpm-2018", "jama-2016"];
    if x.has_benefits || rate_is_default {
        keys.push("mmwr-2024");
    }
    let mut sources = econtent::sources_for(keys, &c.sources);
    sources.push(Source::computed(
        "Economic evaluation arithmetic (present value, ratios, one-way sensitivity)",
        "This server",
    ));

    let texts = roster::argument_strings(&args, &[]);
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut privacy = Privacy {
        files_written: files,
        identifiers_detected: found.clone(),
        ..Privacy::none()
    };
    privacy.notes.push("No local file is read. Only the numbers you typed are used; the chart holds no personal information.".into());
    if !found.is_empty() {
        privacy.notes.push("Contact details were found in the text of your request; an economic evaluation needs only totals.".into());
    }

    let wanted: Vec<&str> = {
        let mut w = vec!["cost_per_participant", "cost_per_outcome", "discounting"];
        if x.has_benefits {
            w.extend(["net_benefit", "bcr", "roi", "break_even"]);
        }
        if x.comparison.is_some() {
            w.push("icer");
        }
        w
    };
    let definitions: Vec<DefinitionOut> = c
        .definitions
        .iter()
        .filter(|df| wanted.contains(&df.id.as_str()))
        .map(|df| DefinitionOut {
            measure: df.label.clone(),
            formula: df.formula.clone(),
            meaning: df.text.clone(),
            citation: format!(
                "{}, {}",
                c.sources
                    .get(&df.source)
                    .map(|x| x.title.as_str())
                    .unwrap_or(&df.source),
                df.section
            ),
            confidence: df.confidence,
        })
        .collect();
    let result = EconomicResult {
        program,
        perspective: persp.label.clone(),
        perspective_text: fill(&persp.text),
        discounting: DiscountOut {
            rate_percent: rate_pct,
            outcome_rate_percent: out_rate_pct,
            horizon_years: horizon,
            first_year_discounted: fy,
            timing_note: fill(&d.timing_note),
            rate_source: if rate_is_default {
                format!(
                    "default {}% from {}, {} (confidence {:?})",
                    d.discount_rate_percent,
                    c.sources
                        .get(&d.discount_rate_source)
                        .map(|s| s.title.as_str())
                        .unwrap_or(&d.discount_rate_source),
                    d.discount_rate_section,
                    d.discount_rate_confidence
                )
            } else {
                "your rate".into()
            },
        },
        costs: costs_out,
        outcomes: outcomes_out,
        benefits: benefits_out,
        break_even: break_even_out,
        comparison: comparison_out,
        sensitivity: sensitivity_out,
        interpretation: reading,
        definitions,
    };
    Ok(Envelope::new(s, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_loads() {
        let c: EconContent = econtent::load(ECONOMIC_CONTENT).unwrap();
        assert_eq!(c.defaults.discount_rate_percent, 3.0);
        assert_eq!(c.interpretation.always.len(), 3);
        assert!(c.perspectives.iter().any(|p| p.id == "societal"));
        assert!(c.definitions.iter().any(|d| d.id == "icer"));
    }

    #[test]
    fn streams_refuse_both_forms_and_negatives() {
        assert!(stream("x", Some(1.0), &[2.0]).is_err());
        assert!(stream("x", None, &[-1.0]).is_err());
        assert_eq!(stream("x", Some(5.0), &[]).unwrap(), vec![5.0]);
        assert!(stream("x", None, &vec![1.0; 31]).is_err());
    }
}
