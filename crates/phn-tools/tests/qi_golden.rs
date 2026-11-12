//! Golden tests for the run chart rules and control chart limits behind
//! `plan_quality_improvement` (EBP-05).
//!
//! * `fixtures/qi_run_chart_golden.json` was computed from the rule definitions by an
//!   independent Python program (`fixtures/generators/qi_run_chart_golden.py`: itertools.groupby,
//!   exact integer counts, brute-force enumeration of arrangements).
//! * `fixtures/qi_anhoj_table1.json` is Table 1 of the published run chart paper (PLoS One
//!   2014;9:e113825, CC BY 4.0), extracted by the same script from the paper's PubMed Central XML.

use phn_epi::spc::{self, ChartType, Input};
use phn_tools::ebp::qi::rules;
use serde_json::Value;

fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn indices(v: &Value) -> Vec<usize> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as usize)
        .collect()
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/qi_run_chart_golden.json")).unwrap()
}

#[test]
fn run_chart_rules_match_the_independent_computation() {
    let fx = fixture();
    let cases = fx["rule_cases"].as_array().unwrap();
    assert!(cases.len() >= 12);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let values = floats(&c["values"]);
        let baseline = c["baseline"].as_u64().map(|b| b as usize);
        let e = &c["expected"];
        let p = rules::analyze(&values, &[], baseline).unwrap();
        assert_eq!(p.phases.len(), 1, "{name}");
        let ph = &p.phases[0];
        assert!(
            (ph.median - e["median"].as_f64().unwrap()).abs() < 1e-12,
            "{name} median"
        );
        assert_eq!(
            ph.runs.points_above,
            e["above"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert_eq!(
            ph.runs.points_below,
            e["below"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert_eq!(
            ph.on_median,
            e["on_median"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert_eq!(
            ph.longest_run,
            e["longest_run"].as_u64().unwrap() as usize,
            "{name} longest run"
        );
        let mut shift = p.shift_points();
        shift.sort_unstable();
        assert_eq!(shift, indices(&e["shift_points"]), "{name} shift points");
        assert_eq!(
            ph.longest_trend,
            e["longest_trend"].as_u64().unwrap() as usize,
            "{name} longest trend"
        );
        let mut trend = p.trend_points();
        trend.sort_unstable();
        trend.dedup();
        assert_eq!(trend, indices(&e["trend_points"]), "{name} trend points");
        assert_eq!(
            ph.runs.observed,
            e["runs"].as_u64().unwrap() as usize,
            "{name} runs"
        );
        assert_eq!(
            ph.runs.too_few_at_most,
            e["signal_if_runs_at_most"].as_u64().map(|x| x as usize),
            "{name} too-few limit"
        );
        assert_eq!(
            ph.runs.too_many_at_least,
            e["signal_if_runs_at_least"].as_u64().map(|x| x as usize),
            "{name} too-many limit"
        );
        assert_eq!(
            ph.runs.too_few,
            e["too_few_runs"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            ph.runs.too_many,
            e["too_many_runs"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            p.astronomical_points(),
            indices(&e["astronomical"]),
            "{name} astronomical"
        );
    }
}

#[test]
fn the_named_cases_show_what_each_rule_is_for() {
    let fx = fixture();
    let by_name = |n: &str| -> Vec<f64> {
        let c = fx["rule_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == n)
            .unwrap();
        floats(&c["values"])
    };
    // A step change: one shift of 10 points on each side, two runs, too few runs.
    let p = rules::analyze(&by_name("step_change"), &[], None).unwrap();
    assert_eq!(p.phases[0].shifts.len(), 2);
    assert!(p.phases[0].runs.too_few && !p.phases[0].runs.too_many);
    // Ties count once in a trend: 1, 2, 2, 3, 4, 5 is a trend of 5 points.
    let p = rules::analyze(&by_name("trend_with_tie"), &[], None).unwrap();
    assert_eq!(p.phases[0].trends.len(), 1);
    assert_eq!(p.phases[0].trends[0].length, 5);
    assert_eq!(p.phases[0].trends[0].direction, "up");
    // An alternating series has too many runs and no shift or trend.
    let p = rules::analyze(&by_name("alternating"), &[], None).unwrap();
    assert!(p.phases[0].runs.too_many && p.phases[0].shifts.is_empty());
    // An isolated extreme point is the only astronomical candidate.
    let p = rules::analyze(&by_name("outlier"), &[], None).unwrap();
    assert_eq!(p.astronomical_points(), vec![7]);
    // Points on the median are skipped, not run breakers.
    let p = rules::analyze(&by_name("on_median"), &[], None).unwrap();
    assert!(p.phases[0].on_median >= 3 && p.phases[0].longest_run >= 6);
}

#[test]
fn a_new_phase_gets_its_own_median() {
    let v: Vec<f64> = vec![
        4., 5., 4., 6., 5., 4., 5., 6., 9., 10., 9., 11., 10., 9., 10., 11.,
    ];
    let p = rules::analyze(&v, &[8], None).unwrap();
    assert_eq!(p.phases.len(), 2);
    assert_eq!((p.phases[0].start, p.phases[0].end), (0, 7));
    assert_eq!((p.phases[1].start, p.phases[1].end), (8, 15));
    assert!((p.phases[0].median - 5.0).abs() < 1e-12);
    assert!((p.phases[1].median - 10.0).abs() < 1e-12);
    // Against one median the second half is a shift; with a median per phase it is not.
    let one = rules::analyze(&v, &[], Some(8)).unwrap();
    assert_eq!(one.phases[0].shifts.len(), 1);
    // The run starts at point 8 (index 7, a 6 above the median of 5) and runs to the end.
    assert_eq!(
        (one.phases[0].shifts[0].first, one.phases[0].shifts[0].last),
        (7, 15)
    );
    assert!(p.shift_points().is_empty());
}

#[test]
fn runs_limits_match_exact_enumeration() {
    let fx = fixture();
    for c in fx["runs_limit_cases"].as_array().unwrap() {
        let n1 = c["n_above"].as_u64().unwrap() as usize;
        let n2 = c["n_below"].as_u64().unwrap() as usize;
        let (few, many) = rules::runs_limits(n1, n2);
        assert_eq!(
            few,
            c["signal_if_at_most"].as_u64().map(|x| x as usize),
            "{n1}/{n2} too few"
        );
        assert_eq!(
            many,
            c["signal_if_at_least"].as_u64().map(|x| x as usize),
            "{n1}/{n2} too many"
        );
    }
    // No signal is possible when every point is on one side.
    assert_eq!(rules::runs_limits(0, 12), (None, None));
}

/// The Anhøj prediction limits computed by the project's SPC engine equal the limits published
/// in Table 1 of PLoS One 2014;9:e113825 for 10 to 100 useful observations.
#[test]
fn anhoj_prediction_limits_match_the_published_table() {
    let table: Value = serde_json::from_str(include_str!("fixtures/qi_anhoj_table1.json")).unwrap();
    let rows = table["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 91);
    for r in rows {
        let n = r["n"].as_u64().unwrap() as usize;
        // n points off the median (below, above) and one or two on it, so the median is exact.
        let mut values: Vec<f64> = Vec::new();
        let (below, above, on) = if n % 2 == 0 {
            (n / 2, n / 2, 1)
        } else {
            ((n - 1) / 2, (n + 1) / 2, 2)
        };
        for i in 0..below.max(above) {
            if i < above {
                values.push(10.0 + 1.0 + i as f64);
            }
            if i < below {
                values.push(10.0 - 1.0 - i as f64);
            }
        }
        for _ in 0..on {
            values.push(10.0);
        }
        let chart = spc::chart(&Input {
            chart: ChartType::Run,
            values,
            denominators: None,
            phase_starts: vec![],
            baseline: None,
        })
        .unwrap();
        let ph = &chart.phases[0];
        assert_eq!(ph.useful_observations, Some(n), "n = {n}");
        assert_eq!(
            ph.longest_run_limit.unwrap() as u64,
            r["longest_run_limit"].as_u64().unwrap(),
            "longest run limit, n = {n}"
        );
        assert_eq!(
            ph.crossings_limit.unwrap() as u64,
            r["crossings_limit"].as_u64().unwrap(),
            "crossings limit, n = {n}"
        );
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

#[test]
fn control_chart_limits_match_the_independent_computation() {
    let fx = fixture();
    let ctl = &fx["control"];
    for c in ctl["xmr"].as_array().unwrap() {
        let values = floats(&c["values"]);
        let baseline = c["baseline"].as_u64().map(|b| b as usize);
        let chart = spc::chart(&Input {
            chart: ChartType::Xmr,
            values: values.clone(),
            denominators: None,
            phase_starts: vec![],
            baseline,
        })
        .unwrap();
        let e = &c["expected"];
        for p in &chart.points {
            assert!(close(p.center, e["center"].as_f64().unwrap()));
            assert!(close(p.lcl.unwrap(), e["lcl"].as_f64().unwrap()));
            assert!(close(p.ucl.unwrap(), e["ucl"].as_f64().unwrap()));
        }
    }
    for c in ctl["p"].as_array().unwrap() {
        let chart = spc::chart(&Input {
            chart: ChartType::P,
            values: floats(&c["numerators"]),
            denominators: Some(floats(&c["denominators"])),
            phase_starts: vec![],
            baseline: None,
        })
        .unwrap();
        let e = &c["expected"];
        let (lcl, ucl) = (floats(&e["lcl"]), floats(&e["ucl"]));
        for (i, p) in chart.points.iter().enumerate() {
            assert!(close(p.center, e["center"].as_f64().unwrap()));
            assert!(close(p.lcl.unwrap(), lcl[i]), "p lcl {i}");
            assert!(close(p.ucl.unwrap(), ucl[i]), "p ucl {i}");
        }
    }
    for c in ctl["u"].as_array().unwrap() {
        let chart = spc::chart(&Input {
            chart: ChartType::U,
            values: floats(&c["counts"]),
            denominators: Some(floats(&c["exposure"])),
            phase_starts: vec![],
            baseline: None,
        })
        .unwrap();
        let e = &c["expected"];
        let (lcl, ucl) = (floats(&e["lcl"]), floats(&e["ucl"]));
        for (i, p) in chart.points.iter().enumerate() {
            assert!(close(p.center, e["center"].as_f64().unwrap()));
            assert!(close(p.lcl.unwrap(), lcl[i]), "u lcl {i}");
            assert!(close(p.ucl.unwrap(), ucl[i]), "u ucl {i}");
        }
    }
    for c in ctl["c"].as_array().unwrap() {
        let chart = spc::chart(&Input {
            chart: ChartType::C,
            values: floats(&c["counts"]),
            denominators: None,
            phase_starts: vec![],
            baseline: None,
        })
        .unwrap();
        let e = &c["expected"];
        for p in &chart.points {
            assert!(close(p.center, e["center"].as_f64().unwrap()));
            assert!(close(p.lcl.unwrap(), e["lcl"].as_f64().unwrap()));
            assert!(close(p.ucl.unwrap(), e["ucl"].as_f64().unwrap()));
        }
    }
}
