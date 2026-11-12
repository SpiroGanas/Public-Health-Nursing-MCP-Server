//! The economic evaluation arithmetic for `economic_evaluation_calculator` (PLAN-10): present
//! value, cost per participant and per outcome, ICER with dominance, net benefit, benefit-cost
//! ratio, return on investment, break-even, and one-way sensitivity.
//!
//! Golden tests: `tests/economic_golden.rs` compares these functions with the published benefit-cost
//! results in CDC's MMWR 2024;73(31) Table 2 and with
//! `tests/fixtures/economic_golden.json`, computed by an independent Python program
//! (`tests/fixtures/generators/economic_golden.py`, exact fractions and a brute-force scan).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Everything the formulas need. Streams are indexed by year (index 0 is year 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub costs: Vec<f64>,
    pub outcomes: Vec<f64>,
    /// Monetized benefits that are not outcome counts times a value (savings, revenue).
    pub other_benefits: Vec<f64>,
    /// Dollar value of one outcome.
    pub benefit_per_outcome: Option<f64>,
    pub participants: Option<f64>,
    pub comp_costs: Option<Vec<f64>>,
    pub comp_outcomes: Option<Vec<f64>>,
    /// Discount rate for costs and dollar benefits (a fraction).
    pub rate: f64,
    /// Discount rate for outcome counts (a fraction).
    pub outcome_rate: f64,
    /// When false, year 1 is not discounted.
    pub first_year_discounted: bool,
    /// Willingness to pay per outcome, from the user.
    pub willingness_to_pay: Option<f64>,
}

/// Present value of a stream: year t (1-based) is divided by (1 + rate)^(t - 1), or ^t when the
/// first year is discounted too.
pub fn present_value(stream: &[f64], rate: f64, first_year_discounted: bool) -> f64 {
    let shift = if first_year_discounted { 1 } else { 0 };
    stream
        .iter()
        .enumerate()
        .map(|(i, v)| v / (1.0 + rate).powi((i + shift) as i32))
        .sum()
}

/// How the program compares with the comparator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    /// Costs less and achieves more: no ratio is meaningful.
    Dominant,
    /// Costs more and achieves less: no ratio is meaningful.
    Dominated,
    /// Costs more and achieves more: the ICER is the extra cost per extra outcome.
    MoreCostlyMoreEffective,
    /// Costs less and achieves less: the ratio is the money saved per outcome given up.
    LessCostlyLessEffective,
    /// Same outcomes: no incremental ratio.
    NoDifference,
}

impl Comparison {
    pub fn label(self) -> &'static str {
        match self {
            Comparison::Dominant => "dominant (costs less, achieves more)",
            Comparison::Dominated => "dominated (costs more, achieves less)",
            Comparison::MoreCostlyMoreEffective => "more costly, more effective",
            Comparison::LessCostlyLessEffective => "less costly, less effective",
            Comparison::NoDifference => "no difference in outcomes",
        }
    }
}

/// Classify an incremental cost and effect.
pub fn classify(delta_cost: f64, delta_effect: f64) -> Comparison {
    const EPS: f64 = 1e-12;
    if delta_effect.abs() <= EPS {
        Comparison::NoDifference
    } else if delta_effect > 0.0 {
        if delta_cost <= 0.0 {
            Comparison::Dominant
        } else {
            Comparison::MoreCostlyMoreEffective
        }
    } else if delta_cost >= 0.0 {
        Comparison::Dominated
    } else {
        Comparison::LessCostlyLessEffective
    }
}

/// All results for one set of inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    pub total_cost: f64,
    pub pv_cost: f64,
    pub total_outcomes: f64,
    pub pv_outcomes: f64,
    pub cost_per_participant: Option<f64>,
    pub cost_per_participant_pv: Option<f64>,
    pub cost_per_outcome: Option<f64>,
    pub cost_per_outcome_pv: Option<f64>,
    pub has_benefits: bool,
    pub total_benefits: f64,
    pub pv_benefits: f64,
    pub net_benefit: Option<f64>,
    pub bcr: Option<f64>,
    pub roi: Option<f64>,
    /// First year (1-based) in which the cumulative discounted net benefit is zero or more.
    pub payback_year: Option<usize>,
    /// Dollar value of one outcome at which the benefits equal the cost (BCR = 1).
    pub break_even_value_per_outcome: Option<f64>,
    /// Share of the expected outcomes needed for benefits to equal the cost (1.0 = all).
    pub break_even_outcome_share: Option<f64>,
    pub delta_cost: Option<f64>,
    pub delta_outcomes: Option<f64>,
    pub comparison: Option<Comparison>,
    pub icer: Option<f64>,
    /// Net monetary benefit at the user's willingness to pay (incremental when there is a
    /// comparator).
    pub nmb: Option<f64>,
}

fn pad(v: &[f64], n: usize) -> Vec<f64> {
    let mut o = v.to_vec();
    o.resize(n, 0.0);
    o
}

/// Compute every metric.
pub fn metrics(m: &Model) -> Metrics {
    let n = m
        .costs
        .len()
        .max(m.outcomes.len())
        .max(m.other_benefits.len())
        .max(m.comp_costs.as_ref().map_or(0, Vec::len))
        .max(m.comp_outcomes.as_ref().map_or(0, Vec::len))
        .max(1);
    let costs = pad(&m.costs, n);
    let outcomes = pad(&m.outcomes, n);
    let other = pad(&m.other_benefits, n);
    let fy = m.first_year_discounted;
    let pv_cost = present_value(&costs, m.rate, fy);
    let total_cost: f64 = costs.iter().sum();
    let pv_outcomes = present_value(&outcomes, m.outcome_rate, fy);
    let total_outcomes: f64 = outcomes.iter().sum();
    let div = |a: f64, b: f64| if b > 0.0 { Some(a / b) } else { None };

    let has_benefits = m.benefit_per_outcome.is_some() || other.iter().any(|x| *x != 0.0);
    let bpo = m.benefit_per_outcome.unwrap_or(0.0);
    let benefit_stream: Vec<f64> = (0..n).map(|i| outcomes[i] * bpo + other[i]).collect();
    let total_benefits: f64 = benefit_stream.iter().sum();
    let pv_benefits = present_value(&benefit_stream, m.rate, fy);
    let net_benefit = has_benefits.then(|| pv_benefits - pv_cost);
    let bcr = if has_benefits {
        div(pv_benefits, pv_cost)
    } else {
        None
    };
    let roi = net_benefit.and_then(|nb| div(nb, pv_cost));
    let payback_year = if has_benefits {
        let mut cum = 0.0;
        let shift = if fy { 1 } else { 0 };
        let mut found = None;
        for i in 0..n {
            cum += (benefit_stream[i] - costs[i]) / (1.0 + m.rate).powi((i + shift) as i32);
            if cum >= -1e-9
                && (i > 0 || benefit_stream.iter().chain(costs.iter()).any(|x| *x != 0.0))
            {
                found = Some(i + 1);
                break;
            }
        }
        found
    } else {
        None
    };
    // Break-even: the value per outcome (or share of outcomes) at which PV benefits = PV cost.
    let pv_out_money = present_value(&outcomes, m.rate, fy);
    let pv_other = present_value(&other, m.rate, fy);
    let break_even_value_per_outcome = if pv_out_money > 0.0 && pv_cost > 0.0 {
        Some(((pv_cost - pv_other) / pv_out_money).max(0.0))
    } else {
        None
    };
    let break_even_outcome_share = match m.benefit_per_outcome {
        Some(b) if b > 0.0 && pv_out_money > 0.0 => {
            Some(((pv_cost - pv_other) / (b * pv_out_money)).max(0.0))
        }
        _ => None,
    };

    // Comparator.
    let (delta_cost, delta_outcomes, comparison, icer) = match (&m.comp_costs, &m.comp_outcomes) {
        (Some(cc), Some(co)) => {
            let ccost = present_value(&pad(cc, n), m.rate, fy);
            let ceff = present_value(&pad(co, n), m.outcome_rate, fy);
            let dc = pv_cost - ccost;
            let de = pv_outcomes - ceff;
            let class = classify(dc, de);
            let icer = match class {
                Comparison::MoreCostlyMoreEffective | Comparison::LessCostlyLessEffective => {
                    Some(dc / de)
                }
                _ => None,
            };
            (Some(dc), Some(de), Some(class), icer)
        }
        _ => (None, None, None, None),
    };
    let nmb = m
        .willingness_to_pay
        .map(|l| match (delta_cost, delta_outcomes) {
            (Some(dc), Some(de)) => l * de - dc,
            _ => l * pv_outcomes - pv_cost,
        });

    Metrics {
        total_cost,
        pv_cost,
        total_outcomes,
        pv_outcomes,
        cost_per_participant: m.participants.and_then(|p| div(total_cost, p)),
        cost_per_participant_pv: m.participants.and_then(|p| div(pv_cost, p)),
        cost_per_outcome: div(total_cost, total_outcomes),
        cost_per_outcome_pv: div(pv_cost, pv_outcomes),
        has_benefits,
        total_benefits,
        pv_benefits,
        net_benefit,
        bcr,
        roi,
        payback_year,
        break_even_value_per_outcome,
        break_even_outcome_share,
        delta_cost,
        delta_outcomes,
        comparison,
        icer,
        nmb,
    }
}

/// The result a sensitivity analysis tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    BenefitCostRatio,
    ReturnOnInvestment,
    NetBenefit,
    CostPerOutcome,
    CostPerParticipant,
    Icer,
}

impl Metric {
    pub fn label(self) -> &'static str {
        match self {
            Metric::BenefitCostRatio => "benefit-cost ratio",
            Metric::ReturnOnInvestment => "return on investment",
            Metric::NetBenefit => "net benefit ($, present value)",
            Metric::CostPerOutcome => "cost per outcome ($, present value)",
            Metric::CostPerParticipant => "cost per participant ($, present value)",
            Metric::Icer => "incremental cost-effectiveness ratio ($ per outcome)",
        }
    }

    /// The metric's value (`None` when it is not defined for these inputs).
    pub fn value(self, x: &Metrics) -> Option<f64> {
        match self {
            Metric::BenefitCostRatio => x.bcr,
            Metric::ReturnOnInvestment => x.roi,
            Metric::NetBenefit => x.net_benefit,
            Metric::CostPerOutcome => x.cost_per_outcome_pv,
            Metric::CostPerParticipant => x.cost_per_participant_pv,
            Metric::Icer => x.icer,
        }
    }

    /// True when the inputs favor the program on this metric (the decision side), if the metric
    /// has a natural decision line.
    pub fn favorable(self, x: &Metrics) -> Option<bool> {
        match self {
            Metric::BenefitCostRatio => x.bcr.map(|v| v >= 1.0),
            Metric::ReturnOnInvestment => x.roi.map(|v| v >= 0.0),
            Metric::NetBenefit => x.net_benefit.map(|v| v >= 0.0),
            Metric::Icer => x.nmb.map(|v| v >= 0.0),
            Metric::CostPerOutcome | Metric::CostPerParticipant => None,
        }
    }
}

/// A number a one-way sensitivity analysis can vary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Parameter {
    /// Total program cost over the horizon (the cost stream is scaled).
    ProgramCost,
    /// Number of participants.
    Participants,
    /// Total outcomes over the horizon (the outcome stream is scaled).
    Outcomes,
    /// Dollar value of one outcome.
    BenefitPerOutcome,
    /// Total other monetized benefits over the horizon (the stream is scaled).
    OtherBenefits,
    /// Total comparator cost over the horizon.
    ComparatorCost,
    /// Total comparator outcomes over the horizon.
    ComparatorOutcomes,
    /// Discount rate in percent (applies to costs, dollar benefits, and outcomes).
    DiscountRate,
}

impl Parameter {
    pub const ALL: [Parameter; 8] = [
        Parameter::ProgramCost,
        Parameter::Participants,
        Parameter::Outcomes,
        Parameter::BenefitPerOutcome,
        Parameter::OtherBenefits,
        Parameter::ComparatorCost,
        Parameter::ComparatorOutcomes,
        Parameter::DiscountRate,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Parameter::ProgramCost => "Program cost",
            Parameter::Participants => "Participants",
            Parameter::Outcomes => "Outcomes achieved",
            Parameter::BenefitPerOutcome => "Value of one outcome",
            Parameter::OtherBenefits => "Other benefits (savings)",
            Parameter::ComparatorCost => "Comparator cost",
            Parameter::ComparatorOutcomes => "Comparator outcomes",
            Parameter::DiscountRate => "Discount rate (%)",
        }
    }

    /// The parameter's base value in the model (`None` when the model does not have it).
    pub fn base(self, m: &Model) -> Option<f64> {
        let sum = |v: &[f64]| v.iter().sum::<f64>();
        match self {
            Parameter::ProgramCost => Some(sum(&m.costs)).filter(|v| *v > 0.0),
            Parameter::Participants => m.participants,
            Parameter::Outcomes => Some(sum(&m.outcomes)).filter(|v| *v > 0.0),
            Parameter::BenefitPerOutcome => m.benefit_per_outcome,
            Parameter::OtherBenefits => Some(sum(&m.other_benefits)).filter(|v| *v > 0.0),
            Parameter::ComparatorCost => m.comp_costs.as_ref().map(|v| sum(v)).filter(|v| *v > 0.0),
            Parameter::ComparatorOutcomes => m
                .comp_outcomes
                .as_ref()
                .map(|v| sum(v))
                .filter(|v| *v > 0.0),
            Parameter::DiscountRate => Some(m.rate * 100.0),
        }
    }

    /// A copy of the model with the parameter set to `value` (streams are scaled to the new
    /// total; the discount rate sets both rates).
    pub fn apply(self, m: &Model, value: f64) -> Model {
        let mut x = m.clone();
        let scale = |v: &mut Vec<f64>, new_total: f64| {
            let t: f64 = v.iter().sum();
            if t > 0.0 {
                let f = new_total / t;
                v.iter_mut().for_each(|e| *e *= f);
            }
        };
        match self {
            Parameter::ProgramCost => scale(&mut x.costs, value),
            Parameter::Participants => x.participants = Some(value),
            Parameter::Outcomes => scale(&mut x.outcomes, value),
            Parameter::BenefitPerOutcome => x.benefit_per_outcome = Some(value),
            Parameter::OtherBenefits => scale(&mut x.other_benefits, value),
            Parameter::ComparatorCost => {
                if let Some(v) = x.comp_costs.as_mut() {
                    scale(v, value)
                }
            }
            Parameter::ComparatorOutcomes => {
                if let Some(v) = x.comp_outcomes.as_mut() {
                    scale(v, value)
                }
            }
            Parameter::DiscountRate => {
                x.rate = value / 100.0;
                x.outcome_rate = value / 100.0;
            }
        }
        x
    }
}

/// One row of a tornado table.
#[derive(Debug, Clone, PartialEq)]
pub struct SensRow {
    pub parameter: Parameter,
    pub base_value: f64,
    pub low_value: f64,
    pub high_value: f64,
    pub result_low: Option<f64>,
    pub result_high: Option<f64>,
    /// Absolute difference between the two results (0 when either is undefined).
    pub swing: f64,
    /// True when the low and high values land on different sides of the decision line.
    pub flips: bool,
}

/// One-way sensitivity: vary each (parameter, low, high), holding the others at their base
/// values. Rows are sorted by swing, largest first.
pub fn one_way(m: &Model, metric: Metric, ranges: &[(Parameter, f64, f64)]) -> Vec<SensRow> {
    let base = metrics(m);
    let base_fav = metric.favorable(&base);
    let mut rows: Vec<SensRow> = ranges
        .iter()
        .filter_map(|(p, lo, hi)| {
            let b = p.base(m)?;
            let ml = metrics(&p.apply(m, *lo));
            let mh = metrics(&p.apply(m, *hi));
            let (rl, rh) = (metric.value(&ml), metric.value(&mh));
            let swing = match (rl, rh) {
                (Some(a), Some(c)) => (a - c).abs(),
                _ => 0.0,
            };
            let flips = match (base_fav, metric.favorable(&ml), metric.favorable(&mh)) {
                (Some(bf), Some(l), Some(h)) => l != bf || h != bf,
                _ => false,
            };
            Some(SensRow {
                parameter: *p,
                base_value: b,
                low_value: *lo,
                high_value: *hi,
                result_low: rl,
                result_high: rh,
                swing,
                flips,
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        b.swing
            .partial_cmp(&a.swing)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model {
            costs: vec![100.0, 100.0, 100.0],
            outcomes: vec![10.0, 10.0, 10.0],
            other_benefits: vec![],
            benefit_per_outcome: Some(20.0),
            participants: Some(50.0),
            comp_costs: None,
            comp_outcomes: None,
            rate: 0.03,
            outcome_rate: 0.03,
            first_year_discounted: false,
            willingness_to_pay: None,
        }
    }

    #[test]
    fn present_value_of_a_level_stream() {
        let pv = present_value(&[100.0, 100.0, 100.0], 0.03, false);
        assert!((pv - (100.0 + 100.0 / 1.03 + 100.0 / 1.03f64.powi(2))).abs() < 1e-12);
        let pv1 = present_value(&[100.0], 0.05, true);
        assert!((pv1 - 100.0 / 1.05).abs() < 1e-12);
        assert_eq!(present_value(&[5.0, 5.0], 0.0, false), 10.0);
    }

    #[test]
    fn ratios_use_present_values() {
        let x = metrics(&model());
        assert!((x.pv_cost - 291.3469).abs() < 1e-3);
        assert!((x.bcr.unwrap() - 2.0).abs() < 1e-12); // 20 x 10 per year vs 100 per year
        assert!((x.roi.unwrap() - 1.0).abs() < 1e-12);
        assert!((x.cost_per_participant.unwrap() - 6.0).abs() < 1e-12);
        assert!((x.break_even_value_per_outcome.unwrap() - 10.0).abs() < 1e-12);
        assert!((x.break_even_outcome_share.unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(x.payback_year, Some(1));
    }

    #[test]
    fn classification_covers_the_four_quadrants() {
        assert_eq!(classify(-5.0, 3.0), Comparison::Dominant);
        assert_eq!(classify(5.0, -3.0), Comparison::Dominated);
        assert_eq!(classify(5.0, 3.0), Comparison::MoreCostlyMoreEffective);
        assert_eq!(classify(-5.0, -3.0), Comparison::LessCostlyLessEffective);
        assert_eq!(classify(5.0, 0.0), Comparison::NoDifference);
        assert_eq!(classify(0.0, 2.0), Comparison::Dominant);
    }

    #[test]
    fn icer_and_net_monetary_benefit() {
        let mut m = model();
        m.rate = 0.0;
        m.outcome_rate = 0.0;
        m.benefit_per_outcome = None;
        m.comp_costs = Some(vec![50.0, 50.0, 50.0]);
        m.comp_outcomes = Some(vec![4.0, 4.0, 4.0]);
        m.willingness_to_pay = Some(60.0);
        let x = metrics(&m);
        assert_eq!(x.delta_cost, Some(150.0));
        assert_eq!(x.delta_outcomes, Some(18.0));
        assert!((x.icer.unwrap() - 150.0 / 18.0).abs() < 1e-12);
        assert!((x.nmb.unwrap() - (60.0 * 18.0 - 150.0)).abs() < 1e-12);
    }

    #[test]
    fn sensitivity_flags_a_flip_and_sorts_by_swing() {
        let m = model();
        let rows = one_way(
            &m,
            Metric::BenefitCostRatio,
            &[
                (Parameter::BenefitPerOutcome, 8.0, 30.0),
                (Parameter::ProgramCost, 240.0, 360.0),
            ],
        );
        // Value of an outcome: BCR 0.8 to 3.0 (swing 2.2); cost: 2.5 to 1.667 (swing 0.83).
        assert_eq!(rows[0].parameter, Parameter::BenefitPerOutcome);
        assert!(rows[0].flips);
        assert!(!rows[1].flips);
    }
}
