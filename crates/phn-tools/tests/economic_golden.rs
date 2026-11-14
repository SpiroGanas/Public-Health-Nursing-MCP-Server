//! Golden tests for the economic evaluation arithmetic behind `economic_evaluation_calculator`
//! (PLAN-10).
//!
//! * `fixtures/economic_golden.json` is computed by an independent Python program with exact
//!   rational arithmetic (`fixtures/generators/economic_golden.py`), including a bisection search
//!   for the break-even value and a rescaling-based one-way sensitivity.
//! * The `published` section holds the benefit-cost results of CDC's MMWR 2024;73(31):682-685,
//!   Table 2 (routine childhood immunizations, 1994-2023): societal cost $268 billion and
//!   benefits $2,931 billion (net savings $2,663 billion, benefit-cost ratio 10.9); payer cost $240
//!   billion and benefits $780 billion (net savings $540 billion, ratio 3.3). The authors'
//!   unrounded inputs are not published, so ratios agree to the printed one-decimal rounding.

use phn_tools::planning::economic::calc::{Comparison, Metric, Model, Parameter, metrics, one_way};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/economic_golden.json")).unwrap()
}

fn vec_of(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn model(c: &Value) -> Model {
    Model {
        costs: vec_of(&c["costs"]),
        outcomes: vec_of(&c["outcomes"]),
        other_benefits: vec_of(&c["other_benefits"]),
        benefit_per_outcome: c["benefit_per_outcome"].as_f64(),
        participants: c["participants"].as_f64(),
        comp_costs: c["comp_costs"].as_array().map(|_| vec_of(&c["comp_costs"])),
        comp_outcomes: c["comp_outcomes"]
            .as_array()
            .map(|_| vec_of(&c["comp_outcomes"])),
        rate: c["rate"].as_f64().unwrap(),
        outcome_rate: c["outcome_rate"].as_f64().unwrap(),
        first_year_discounted: c["first_year_discounted"].as_bool().unwrap(),
        willingness_to_pay: c["willingness_to_pay"].as_f64(),
    }
}

fn close(got: Option<f64>, want: &Value, what: &str) {
    match want.as_str() {
        None => assert!(
            got.is_none() || want.is_null(),
            "{what}: expected none, got {got:?}"
        ),
        Some(w) => {
            let w: f64 = w.parse().unwrap();
            let g = got.unwrap_or_else(|| panic!("{what}: expected {w}, got none"));
            assert!(
                (g - w).abs() <= 1e-9 * w.abs().max(1.0),
                "{what}: {g} vs {w}"
            );
        }
    }
}

#[test]
fn metrics_match_the_independent_computation() {
    let fx = fixture();
    let cases = fx["cases"].as_array().unwrap();
    assert!(cases.len() >= 10);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let x = metrics(&model(c));
        let e = &c["expected"];
        close(Some(x.pv_cost), &e["pv_cost"], &format!("{name} pv_cost"));
        close(
            Some(x.pv_outcomes),
            &e["pv_outcomes"],
            &format!("{name} pv_outcomes"),
        );
        close(
            Some(x.total_cost),
            &e["total_cost"],
            &format!("{name} total_cost"),
        );
        close(
            x.cost_per_outcome_pv,
            &e["cost_per_outcome_pv"],
            &format!("{name} cost per outcome"),
        );
        close(
            x.cost_per_participant,
            &e["cost_per_participant"],
            &format!("{name} cost per participant"),
        );
        close(
            x.net_benefit,
            &e["net_benefit"],
            &format!("{name} net benefit"),
        );
        close(x.bcr, &e["bcr"], &format!("{name} bcr"));
        close(x.roi, &e["roi"], &format!("{name} roi"));
        close(x.icer, &e["icer"], &format!("{name} icer"));
        close(x.nmb, &e["nmb"], &format!("{name} nmb"));
        if let Some(y) = e["payback_year"].as_u64() {
            assert_eq!(x.payback_year, Some(y as usize), "{name} payback");
        }
        if !e["break_even_value"].is_null() {
            // The tool uses a closed form; the generator used bisection.
            let w: f64 = e["break_even_value"].as_str().unwrap().parse().unwrap();
            let g = x.break_even_value_per_outcome.unwrap();
            assert!(
                (g - w).abs() <= 1e-6 * w.abs().max(1.0),
                "{name} break-even {g} vs {w}"
            );
        }
        if let Some(cl) = e["classification"].as_str() {
            let want = match cl {
                "dominant" => Comparison::Dominant,
                "dominated" => Comparison::Dominated,
                "more_costly_more_effective" => Comparison::MoreCostlyMoreEffective,
                "less_costly_less_effective" => Comparison::LessCostlyLessEffective,
                _ => Comparison::NoDifference,
            };
            assert_eq!(x.comparison, Some(want), "{name} classification");
        }
        if let Some(dc) = e.get("delta_cost").filter(|v| v.is_string()) {
            close(x.delta_cost, dc, &format!("{name} delta cost"));
            close(
                x.delta_outcomes,
                &e["delta_outcomes"],
                &format!("{name} delta outcomes"),
            );
        }
    }
}

#[test]
fn one_way_sensitivity_matches_the_rescaling_computation() {
    let fx = fixture();
    for c in fx["cases"].as_array().unwrap() {
        let Some(rows) = c.get("sensitivity_bcr") else {
            continue;
        };
        let m = model(c);
        let param = |s: &str| match s {
            "program_cost" => Parameter::ProgramCost,
            "benefit_per_outcome" => Parameter::BenefitPerOutcome,
            "outcomes" => Parameter::Outcomes,
            _ => Parameter::DiscountRate,
        };
        let ranges: Vec<(Parameter, f64, f64)> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    param(r["parameter"].as_str().unwrap()),
                    r["low"].as_f64().unwrap(),
                    r["high"].as_f64().unwrap(),
                )
            })
            .collect();
        let got = one_way(&m, Metric::BenefitCostRatio, &ranges);
        for r in rows.as_array().unwrap() {
            let p = param(r["parameter"].as_str().unwrap());
            let row = got.iter().find(|g| g.parameter == p);
            let lo: f64 = r["bcr_low"].as_str().unwrap().parse().unwrap();
            let hi: f64 = r["bcr_high"].as_str().unwrap().parse().unwrap();
            match row {
                Some(row) => {
                    assert!((row.result_low.unwrap() - lo).abs() < 1e-9, "{p:?} low");
                    assert!((row.result_high.unwrap() - hi).abs() < 1e-9, "{p:?} high");
                }
                // A number that does not change the result is left out of the tornado rows.
                None => panic!("{p:?} is missing"),
            }
        }
    }
}

#[test]
fn published_immunization_benefit_cost_results() {
    // CDC MMWR 2024;73(31), Table 2 (billions of dollars, already discounted by the authors).
    let fx = fixture();
    for p in fx["published"]["cases"].as_array().unwrap() {
        let name = p["name"].as_str().unwrap();
        let m = Model {
            costs: vec![p["cost"].as_f64().unwrap()],
            outcomes: vec![],
            other_benefits: vec![p["benefits"].as_f64().unwrap()],
            benefit_per_outcome: None,
            participants: None,
            comp_costs: None,
            comp_outcomes: None,
            rate: 0.0,
            outcome_rate: 0.0,
            first_year_discounted: false,
            willingness_to_pay: None,
        };
        let x = metrics(&m);
        assert_eq!(
            x.net_benefit.unwrap(),
            p["published_net"].as_f64().unwrap(),
            "{name} net savings"
        );
        let published = p["published_bcr"].as_f64().unwrap();
        assert!(
            (x.bcr.unwrap() - published).abs() <= 0.05 + 1e-9,
            "{name}: bcr {} vs published {published}",
            x.bcr.unwrap()
        );
        // ROI is net benefit over cost: the benefit-cost ratio minus one.
        assert!((x.roi.unwrap() - (x.bcr.unwrap() - 1.0)).abs() < 1e-12);
    }
}
