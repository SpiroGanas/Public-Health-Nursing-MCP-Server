//! MCP integration tests for the Phase 2 tools (epidemiology and
//! biostatistics engine): successful calls validate against each tool's
//! output schema, problems the nurse can fix are tool-level errors, charts
//! land in the output folder, and local-data analyses return aggregates only.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

async fn call_ok(h: &common::Harness, name: &str, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h.call(name, args.clone()).await;
    assert_eq!(r.is_error, Some(false), "{name} {args}: {}", summary(&r));
    let md = summary(&r);
    assert!(
        !md.trim_start().starts_with('{'),
        "{name}: first block should be markdown"
    );
    let structured = r.structured_content.clone().expect("structured content");
    let tool = tools.iter().find(|t| t.name == name).unwrap();
    let schema = Value::Object((**tool.output_schema.as_ref().unwrap()).clone());
    let v = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = v
        .iter_errors(&structured)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .take(5)
        .collect();
    assert!(
        errors.is_empty(),
        "{name} output does not match its schema: {errors:#?}"
    );
    structured
}

async fn call_err(h: &common::Harness, name: &str, args: Value) -> String {
    let r = h.call(name, args.clone()).await;
    assert_eq!(
        r.is_error,
        Some(true),
        "{name} {args} should fail: {}",
        summary(&r)
    );
    everything(&r)
}

#[tokio::test]
async fn two_by_two_and_stratified_analysis() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "analyze_two_by_two",
        json!({"tables": [{"exposed_cases": 30, "exposed_noncases": 10, "unexposed_cases": 5, "unexposed_noncases": 25}],
               "exposure": "ate potato salad", "outcome": "vomiting or diarrhea"}),
    )
    .await;
    let t = &s["data"]["tables"][0];
    assert!((t["risk_ratio"]["value"].as_f64().unwrap() - 4.5).abs() < 1e-12);
    assert!(s["summary"].as_str().unwrap().contains("Interpretation"));
    let s = call_ok(
        &h,
        "analyze_two_by_two",
        json!({"tables": [
            {"exposed_cases": 20, "exposed_noncases": 80, "unexposed_cases": 10, "unexposed_noncases": 90, "label": "Children"},
            {"exposed_cases": 40, "exposed_noncases": 60, "unexposed_cases": 25, "unexposed_noncases": 75, "label": "Adults"}],
               "design": "case_control"}),
    )
    .await;
    assert!(
        s["data"]["stratified"]["mh_odds_ratio"]["value"]
            .as_f64()
            .unwrap()
            > 1.0
    );
    assert!(s["data"]["stratified"].get("mh_risk_ratio").is_none());
    let e = call_err(&h, "analyze_two_by_two", json!({"tables": []})).await;
    assert!(e.contains("at least one"));
}

#[tokio::test]
async fn age_adjustment_and_standardized_ratio() {
    let h = start(|_| {}).await;
    let strata = |scale: f64| {
        json!([
            {"age_group": "0-17", "count": (2.0 * scale) as u64, "population": 9000},
            {"age_group": "18-44", "count": (10.0 * scale) as u64, "population": 14000},
            {"age_group": "45-64", "count": (25.0 * scale) as u64, "population": 11000},
            {"age_group": "65+", "count": (40.0 * scale) as u64, "population": 6000}
        ])
    };
    let s = call_ok(
        &h,
        "age_adjust_rates",
        json!({"groups": [{"label": "Town A", "strata": strata(1.0)}, {"label": "Massachusetts", "strata": strata(1.5)}],
               "reference_group": "Massachusetts", "event_description": "hospitalizations"}),
    )
    .await;
    let g = &s["data"]["groups"][0];
    assert!(g["result"]["rate"].as_f64().unwrap() > 0.0);
    assert!(g["vs_reference"]["ratio"].as_f64().unwrap() < 1.0);
    assert_eq!(s["data"]["standard_basis"], "nchs_master_list");
    let e = call_err(&h, "age_adjust_rates", json!({"groups": [{"label": "X", "strata": [{"age_group": "adults", "count": 1, "population": 10}]}]})).await;
    assert!(e.contains("cannot read age group"));

    let s = call_ok(
        &h,
        "compute_standardized_ratio",
        json!({"areas": [{"label": "Town A", "observed_total": 18, "strata": [
            {"stratum": "0-44", "population": 20000, "reference_rate": 10},
            {"stratum": "45-64", "population": 8000, "reference_rate": 60},
            {"stratum": "65+", "population": 4000, "reference_rate": 120}]}],
               "reference_name": "Massachusetts", "event_description": "bladder cancer, 2015–2019"}),
    )
    .await;
    let r = &s["data"]["areas"][0]["result"];
    assert!((r["expected"].as_f64().unwrap() - 11.6).abs() < 1e-9);
    let e = call_err(
        &h,
        "compute_standardized_ratio",
        json!({"areas": [{"label": "T", "strata": [{"stratum": "a", "population": 10}]}]}),
    )
    .await;
    assert!(e.contains("reference_rate"));
}

#[tokio::test]
async fn sample_size_and_power() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "calculate_sample_size",
        json!({"study_type": "survey_proportion", "margin_of_error": 5, "population_size": 1000}),
    )
    .await;
    assert_eq!(s["data"]["group1"]["needed"], 278);
    let s = call_ok(
        &h,
        "calculate_sample_size",
        json!({"study_type": "case_control", "odds_ratio": 2.0, "control_exposure": 0.3, "ratio": 2, "response_rate": 80}),
    )
    .await;
    assert!(
        s["data"]["group2"]["needed"].as_u64().unwrap()
            > s["data"]["group1"]["needed"].as_u64().unwrap()
    );
    let s = call_ok(&h, "calculate_sample_size", json!({"study_type": "power_two_means", "difference": 5, "standard_deviation": 10, "n1": 64})).await;
    assert!((s["data"]["power"].as_f64().unwrap() - 0.8).abs() < 0.02);
    let e = call_err(
        &h,
        "calculate_sample_size",
        json!({"study_type": "two_means"}),
    )
    .await;
    assert!(e.contains("difference"));
}

#[tokio::test]
async fn trends_with_joinpoint_and_chart() {
    let h = start(|_| {}).await;
    let periods: Vec<Value> = (0..14)
        .map(|i| {
            let c = if i < 7 { 40 + 6 * i } else { 76 - 4 * (i - 7) };
            json!({"label": format!("{}", 2012 + i), "count": c, "population": 60000})
        })
        .collect();
    let s = call_ok(
        &h,
        "analyze_trends",
        json!({"periods": periods, "event_description": "Lyme disease cases"}),
    )
    .await;
    let jp = &s["data"]["analysis"]["joinpoint"];
    assert_eq!(jp["joinpoints"].as_array().unwrap().len(), 1, "{jp}");
    let png = s["data"]["chart"]["png"].as_str().unwrap();
    assert!(
        h.data().join(png.trim_start_matches("data/")).exists() || h.dir.path().join(png).exists(),
        "{png}"
    );
    let monthly: Vec<Value> = (0..36)
        .map(|i| json!({"label": format!("{}-{:02}", 2023 + i / 12, i % 12 + 1), "count": 50 + (if i % 12 < 3 { 30 } else { 0 })}))
        .collect();
    let s = call_ok(
        &h,
        "analyze_trends",
        json!({"periods": monthly, "time_unit": "month", "chart": false}),
    )
    .await;
    assert!(
        s["data"]["analysis"]["seasonality"]["p_value"]
            .as_f64()
            .unwrap()
            < 0.05
    );
    let e = call_err(
        &h,
        "analyze_trends",
        json!({"periods": [{"label": "2020", "count": 1}]}),
    )
    .await;
    assert!(e.contains("between 3 and 520"));
}

#[tokio::test]
async fn statistical_tests_inline_and_local() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "run_statistical_test",
        json!({"question": "Do the clinics differ in wait time?",
               "groups": [{"name": "North", "values": [12, 15, 14, 10, 13, 18, 16, 11]},
                          {"name": "South", "values": [20, 22, 19, 25, 21, 18, 24, 23]}]}),
    )
    .await;
    assert_eq!(s["data"]["selected_test"], "WelchT");
    assert!(s["data"]["primary"]["p_value"].as_f64().unwrap() < 0.001);
    let s = call_ok(&h, "run_statistical_test", json!({"table": {"row_labels": ["Vaccinated", "Unvaccinated"], "column_labels": ["Ill", "Well"], "counts": [[2, 48], [9, 41]]}})).await;
    assert_eq!(s["data"]["selected_test"], "ChiSquare");
    // Local file: numeric outcome by group; aggregates only.
    let s = call_ok(
        &h,
        "run_statistical_test",
        json!({"path": "line_list.csv", "outcome": "Age", "group": "Hospitalized"}),
    )
    .await;
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert!(
        s["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g.get("min").is_none())
    );
    // Logistic regression on a local file (schema-checked; aggregates only).
    let s = call_ok(
        &h,
        "run_statistical_test",
        json!({"path": "line_list.csv", "outcome": "Hospitalized", "predictors": ["Age"]}),
    )
    .await;
    assert_eq!(s["data"]["selected_test"], "LogisticRegression");
    assert!(s["data"]["regression"]["terms"].as_array().unwrap().len() == 2);
    // Identifier columns are refused.
    let e = call_err(
        &h,
        "run_statistical_test",
        json!({"path": "line_list.csv", "outcome": "Age", "group": "Last Name"}),
    )
    .await;
    assert!(e.contains("cannot be analyzed"), "{e}");
    let e = call_err(
        &h,
        "run_statistical_test",
        json!({"groups": [{"name": "a", "values": [1, 2]}], "x": [1, 2]}),
    )
    .await;
    assert!(e.contains("exactly one kind"));
}

#[tokio::test]
async fn charts_and_spc_charts_are_written_to_output() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "create_chart",
        json!({"chart_type": "bar", "title": "Pertussis cases by town", "categories": ["Amherst", "Hadley", "Pelham"],
               "series": [{"name": "Cases", "values": [12, 3, 7]}], "suppress_small_counts": true}),
    )
    .await;
    assert_eq!(s["data"]["suppressed_values"], 1);
    assert!(
        s["data"]["files"]["alt_text"]
            .as_str()
            .unwrap()
            .contains("suppressed")
    );
    let png = s["data"]["files"]["png"].as_str().unwrap();
    assert!(png.starts_with("output/charts/"), "{png}");
    assert!(h.data().join(png).exists());

    let s = call_ok(
        &h,
        "create_spc_chart",
        json!({"chart_type": "p", "title": "Doses documented within 24 hours",
               "labels": ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
               "values": [80, 82, 78, 85, 81, 79, 60, 83, 84, 80, 82, 81],
               "denominators": [100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100]}),
    )
    .await;
    assert!(
        s["data"]["signals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x.as_str().unwrap().starts_with("Jul"))
    );
    // From the local line list, counted by month.
    let s = call_ok(
        &h,
        "create_spc_chart",
        json!({"path": "line_list.csv", "date_column": "Onset Date", "period": "month", "measure": "count", "chart_type": "run"}),
    )
    .await;
    assert_eq!(s["data"]["rows"].as_array().unwrap().len(), 2);
    let e = call_err(&h, "create_chart", json!({"chart_type": "line", "title": "", "categories": ["a"], "series": [{"name": "x", "values": [1]}]})).await;
    assert!(e.contains("title"));
}

#[tokio::test]
async fn local_analyses_do_not_leak_identifiers() {
    let h = start(|_| {}).await;
    for args in [
        json!({"path": "line_list.csv", "outcome": "Hospitalized", "group": "Diagnosis"}),
        json!({"path": "line_list.csv", "outcome": "Hospitalized", "predictors": ["Age", "Sex"]}),
    ] {
        let r = h.call("run_statistical_test", args.clone()).await;
        let found = leaks(&everything(&r), "data/line_list.csv", None);
        assert!(found.is_empty(), "{args}: {found:?}");
    }
}
