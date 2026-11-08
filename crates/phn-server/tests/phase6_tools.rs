//! Phase 6 MCP integration tests: VAX-03 forecast_immunizations_due,
//! CD-08 get_notifiable_disease_trends, CD-09
//! get_respiratory_and_wastewater_surveillance.

mod common;

use std::collections::BTreeSet;

use common::{Harness, everything, leaks, start, summary};
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

async fn assert_schema_valid(h: &Harness, name: &str, r: &CallToolResult) {
    assert_eq!(r.is_error, Some(false), "{name}: {}", summary(r));
    let md = summary(r);
    assert!(!md.trim().is_empty() && !md.trim_start().starts_with('{'));
    let tools = h.client.list_all_tools().await.unwrap();
    let tool = tools.iter().find(|t| t.name == name).expect("tool listed");
    let schema = Value::Object((**tool.output_schema.as_ref().unwrap()).clone());
    let v = jsonschema::validator_for(&schema).unwrap();
    let structured = r.structured_content.clone().expect("structured content");
    let errors: Vec<String> = v
        .iter_errors(&structured)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .take(5)
        .collect();
    assert!(errors.is_empty(), "{name}: {errors:#?}");
}

fn data(r: &CallToolResult) -> Value {
    r.structured_content.clone().unwrap()["data"].clone()
}

fn forecast<'a>(d: &'a Value, group: &str) -> &'a Value {
    d["patient"]["forecasts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["vaccine_group"] == group)
        .unwrap_or_else(|| panic!("no {group} forecast in {d}"))
}

#[tokio::test]
async fn forecast_for_one_infant_is_correct_and_does_not_echo_the_date_of_birth() {
    let h = start(|_| {}).await;
    // CDC test case 2013-0002: DTaP #2 at 10 weeks - 5 days is too young.
    let args = json!({
        "date_of_birth": "2026-06-01",
        "sex": "female",
        "assessment_date": "2026-08-05",
        "doses": [
            {"vaccine": "DTaP, unspecified formulation", "date": "2026-07-10"},
            {"vaccine": "107", "date": "2026-08-05"}
        ],
        "vaccine_groups": ["DTaP"]
    });
    let r = h.call("forecast_immunizations_due", args).await;
    assert_schema_valid(&h, "forecast_immunizations_due", &r).await;
    let d = data(&r);
    let f = forecast(&d, "DTaP/Tdap/Td");
    assert_eq!(f["next_dose_number"], 2);
    assert_eq!(f["earliest"], "2026-09-02");
    assert_eq!(f["recommended"], "2026-10-01");
    assert_eq!(f["past_due"], "2026-11-28");
    let doses = d["patient"]["doses"].as_array().unwrap();
    assert_eq!(doses[0]["status"], "Valid");
    assert_eq!(doses[1]["status"], "Not Valid");
    assert!(
        doses[1]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x.as_str().unwrap().contains("too young"))
    );
    let all = everything(&r);
    assert!(!all.contains("2026-06-01"), "date of birth echoed");
    assert!(all.contains("Decision support only"));
    assert!(all.contains("MIIS"));
    assert!(all.contains("compare_vaccine_recommendations"));
}

#[tokio::test]
async fn forecast_resolves_trade_names_and_reports_catch_up() {
    let h = start(|_| {}).await;
    // A 2-year-old with only two infant doses: catch-up is due.
    let r = h
        .call(
            "forecast_immunizations_due",
            json!({
                "date_of_birth": "2024-09-03",
                "sex": "male",
                "assessment_date": "2026-11-07",
                "doses": [
                    {"vaccine": "Engerix B-Peds", "date": "2024-09-04"},
                    {"vaccine": "Pentacel", "date": "2024-11-05"}
                ]
            }),
        )
        .await;
    assert_schema_valid(&h, "forecast_immunizations_due", &r).await;
    let d = data(&r);
    for g in ["DTaP/Tdap/Td", "Polio", "HepB", "MMR", "Varicella"] {
        let f = forecast(&d, g);
        assert!(
            matches!(f["due_status"].as_str(), Some("overdue" | "due_now")),
            "{g}: {f}"
        );
    }
    assert!(!d["patient"]["catch_up"].as_array().unwrap().is_empty());
    // Rotavirus has aged out for a 2-year-old.
    assert!(
        d["patient"]["not_applicable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x.as_str().unwrap().starts_with("Rotavirus"))
    );
}

#[tokio::test]
async fn forecast_uses_conditions_for_risk_series() {
    let h = start(|_| {}).await;
    // CDC test case 2016-UC-0158: adult who smokes, PPSV23 given.
    let r = h
        .call(
            "forecast_immunizations_due",
            json!({
                "date_of_birth": "1997-06-23",
                "sex": "F",
                "assessment_date": "2022-01-25",
                "doses": [{"vaccine": "33", "date": "2016-09-16"}],
                "conditions": [{"condition": "Smoke cigarettes"}],
                "vaccine_groups": ["pneumococcal"]
            }),
        )
        .await;
    assert_schema_valid(&h, "forecast_immunizations_due", &r).await;
    let d = data(&r);
    let risk = d["patient"]["forecasts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["series_type"] == "Risk")
        .expect("risk forecast");
    assert_eq!(risk["next_dose_number"], 2);
    assert_eq!(risk["earliest"], "2017-09-16");
    assert_eq!(d["patient"]["conditions"][0], "Smoke cigarettes (042)");
}

#[tokio::test]
async fn forecast_bad_input_is_a_tool_error() {
    let h = start(|_| {}).await;
    let cases = vec![
        (json!({}), "date_of_birth"),
        (
            json!({"date_of_birth": "2020-13-40"}),
            "date_of_birth must be a date",
        ),
        (
            json!({"date_of_birth": "2020-01-01", "doses": [{"vaccine": "Pedarix", "date": "2020-03-01"}]}),
            "PEDIARIX",
        ),
        (
            json!({"date_of_birth": "2020-01-01", "doses": [{"vaccine": "110", "date": "2019-03-01"}]}),
            "before the date of birth",
        ),
        (
            json!({"date_of_birth": "2020-01-01", "conditions": [{"condition": "zzzz qqq"}]}),
            "did not match",
        ),
        (
            json!({"date_of_birth": "2020-01-01", "vaccine_groups": ["bogus"]}),
            "unknown vaccine group",
        ),
        (
            json!({"file": "immunization_history.csv", "date_of_birth": "2020-01-01"}),
            "not both",
        ),
        (json!({"file": "nope.csv"}), "not found"),
    ];
    for (args, expect) in cases {
        let r = h.call("forecast_immunizations_due", args.clone()).await;
        assert_eq!(r.is_error, Some(true), "{args}");
        let msg = summary(&r);
        assert!(msg.contains(expect), "{args}: {msg}");
    }
}

#[tokio::test]
async fn forecast_file_mode_returns_suppressed_counts_and_writes_per_person_files() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "forecast_immunizations_due",
            json!({"file": "immunization_history.csv", "assessment_date": "2026-11-07"}),
        )
        .await;
    assert_schema_valid(&h, "forecast_immunizations_due", &r).await;
    let d = data(&r);
    let f = &d["file"];
    assert_eq!(f["persons"], 6);
    assert_eq!(f["rows_skipped"], 1);
    assert_eq!(f["unrecognized_vaccine_rows"], 1);
    assert_eq!(f["columns"]["vaccine"], "CVX");
    let env = r.structured_content.clone().unwrap();
    assert_eq!(env["privacy"]["suppression_applied"], true);
    assert_eq!(env["privacy"]["row_level_data"], false);
    let written = env["privacy"]["files_written"].as_array().unwrap();
    assert_eq!(written.len(), 2);
    for w in written {
        let path = h.data().join(w.as_str().unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("IZ-80101"), "per-person file has IDs");
    }
    // Six people: counts from 1 to 4 (default threshold 5) are hidden, and
    // complementary suppression hides more.
    let threshold = f["suppression_threshold"].as_u64().unwrap();
    assert_eq!(threshold, 5);
    let mut hidden = 0;
    for g in f["by_group"].as_array().unwrap() {
        for k in [
            "overdue",
            "due_now",
            "can_give_now",
            "due_later",
            "complete_or_immune",
            "other",
        ] {
            match g[k].as_u64() {
                None => hidden += 1,
                Some(n) => assert!(n == 0 || n >= threshold, "{k} shown below threshold: {g}"),
            }
        }
    }
    assert!(hidden > 0, "some cells are suppressed");
    assert!(summary(&r).contains("<5*"));
    let found = leaks(&everything(&r), "data/immunization_history.csv", None);
    assert!(found.is_empty(), "identifiers returned: {found:?}");
}

#[tokio::test]
async fn forecast_file_counts_are_shown_with_a_low_threshold() {
    let h = start(|_| {}).await;
    h.call("configure_jurisdiction", json!({"suppression_threshold": 2}))
        .await;
    let r = h
        .call(
            "forecast_immunizations_due",
            json!({"file": "immunization_history.csv", "assessment_date": "2026-11-07", "vaccine_groups": ["MMR", "HepB"]}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", summary(&r));
    let d = data(&r);
    assert_eq!(d["file"]["suppression_threshold"], 2);
    let groups: BTreeSet<String> = d["file"]["by_group"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["vaccine_group"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(groups, BTreeSet::from(["HepB".into(), "MMR".into()]));
    for g in d["file"]["by_group"].as_array().unwrap() {
        let shown: u64 = [
            "overdue",
            "due_now",
            "can_give_now",
            "due_later",
            "complete_or_immune",
            "other",
        ]
        .iter()
        .filter_map(|k| g[*k].as_u64())
        .sum();
        assert!(shown <= 6, "{g}");
    }
}

#[test]
fn phi_detector_flags_the_immunization_fixture_identifiers() {
    let path = common::corpus().join("data/immunization_history.csv");
    let t = phn_local::table::read_table(&path, None).unwrap();
    let flagged: BTreeSet<String> = phn_local::phi::scan_table(&t)
        .into_iter()
        .map(|f| f.column)
        .collect();
    for s in common::seeded()
        .iter()
        .filter(|s| s.file == "data/immunization_history.csv")
    {
        let col = s.column.as_ref().unwrap();
        assert!(flagged.contains(col), "column '{col}' not flagged: {flagged:?}");
    }
}
