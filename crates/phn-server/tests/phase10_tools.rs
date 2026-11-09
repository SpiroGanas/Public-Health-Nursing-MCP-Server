//! MCP integration tests for the Phase 10 tools (healthy aging and mental
//! health screening): successful calls validate against each tool's output
//! schema, problems the nurse can fix are tool-level errors, clinical and
//! legal answers carry the decision-support notice and review caveats, files
//! land in the output folder, and local details never come back.

mod common;

use std::collections::BTreeSet;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

async fn call_ok(h: &common::Harness, name: &str, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h.call(name, args.clone()).await;
    assert_eq!(r.is_error, Some(false), "{name} {args}: {}", everything(&r));
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

fn caveats(s: &Value) -> String {
    s["caveats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn pending(s: &Value) -> bool {
    caveats(s).contains("has not yet been reviewed by the subject matter expert")
}

fn decision_support(s: &Value) -> bool {
    s["notice"]
        .as_str()
        .is_some_and(|d| d.contains("Decision support only"))
}

// ------------------------------------------------------------------ MH-02

#[tokio::test]
async fn screening_scores_alerts_and_catalog() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "score_mental_health_screen", json!({})).await;
    assert_eq!(s["data"]["instruments"].as_array().unwrap().len(), 7);
    let s = call_ok(
        &h,
        "score_mental_health_screen",
        json!({"instrument": "PHQ-9", "responses": [2, 2, 1, 2, 1, 1, 1, 0, 1]}),
    )
    .await;
    assert_eq!(s["data"]["scored"]["total_score"], 11);
    assert_eq!(s["data"]["scored"]["severity"], "moderate");
    assert_eq!(s["data"]["safety_alerts"][0]["id"], "suicide_item_positive");
    assert!(s["summary"].as_str().unwrap().contains("SAFETY ALERT"));
    assert!(decision_support(&s) && pending(&s));
    let s = call_ok(
        &h,
        "score_mental_health_screen",
        json!({"instrument": "asq", "responses": ["no", "no", "yes", "no", "yes"]}),
    )
    .await;
    assert_eq!(s["data"]["scored"]["outcome"], "acute_positive");
    assert_eq!(s["data"]["safety_alerts"][0]["urgency"], "immediate");
    assert_eq!(s["data"]["license"]["wording_bundled"], true);
    // EPDS and C-SSRS wording is not bundled.
    let s = call_ok(
        &h,
        "score_mental_health_screen",
        json!({"instrument": "epds", "responses": [1, 1, 1, 1, 1, 1, 1, 1, 1, 0]}),
    )
    .await;
    assert_eq!(s["data"]["license"]["wording_bundled"], false);
    assert!(
        s["data"]["scored"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i.get("wording").is_none())
    );
    let e = call_err(
        &h,
        "score_mental_health_screen",
        json!({"instrument": "gad7", "responses": [4]}),
    )
    .await;
    assert!(e.contains("out of range"), "{e}");
    let e = call_err(
        &h,
        "score_mental_health_screen",
        json!({"instrument": "beck"}),
    )
    .await;
    assert!(e.contains("not a supported instrument"), "{e}");
}

#[tokio::test]
async fn screening_file_returns_suppressed_counts_only() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "score_mental_health_screen",
        json!({"instrument": "phq9", "file": "phq9_screens.csv"}),
    )
    .await;
    let b = &s["data"]["batch"];
    assert_eq!(b["rows_read"], 8);
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    let out = b["output_file"].as_str().unwrap();
    assert!(h.data().join(out).exists(), "{out}");
    let r = h
        .call(
            "score_mental_health_screen",
            json!({"instrument": "phq9", "file": "phq9_screens.csv"}),
        )
        .await;
    assert!(leaks(&everything(&r), "data/phq9_screens.csv", None).is_empty());
    // The fixture's identifier columns are detected.
    let t = phn_local::table::read_table(&common::corpus().join("data/phq9_screens.csv"), None)
        .unwrap();
    let flagged: BTreeSet<String> = phn_local::phi::scan_table(&t)
        .into_iter()
        .map(|f| f.column)
        .collect();
    for col in ["Last Name", "First Name", "DOB", "Phone"] {
        assert!(flagged.contains(col), "{col} not flagged: {flagged:?}");
    }
}

// ------------------------------------------------------------------ MH-01

#[tokio::test]
async fn crisis_resources_for_a_town() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "find_crisis_resources",
        json!({"town": "Worcester", "language": "Spanish"}),
    )
    .await;
    assert_eq!(s["data"]["cbhc_match"], "listed");
    assert!(
        s["data"]["local_cbhcs"][0]["name"]
            .as_str()
            .unwrap()
            .contains("Worcester")
    );
    let lines: Vec<&str> = s["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    for id in [
        "lifeline_988",
        "bhhl",
        "mobile_crisis",
        "crisis_text_line",
        "emergency_911",
    ] {
        assert!(lines.contains(&id), "{id}");
    }
    assert!(
        s["data"]["language_options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o.as_str().unwrap().contains("press 2"))
    );
    // A town with no published CBHC list falls back to the county.
    let s = call_ok(&h, "find_crisis_resources", json!({"town": "Framingham"})).await;
    assert_eq!(s["data"]["cbhc_match"], "same_county");
    let e = call_err(&h, "find_crisis_resources", json!({"town": "Worchester"})).await;
    assert!(e.contains("Worcester"));
}

// ------------------------------------------------------------------ AGE-01

#[tokio::test]
async fn steadi_assessment_and_plan_document() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "steadi_fall_risk_assessment",
        json!({"stay_independent": [true, false, true, true, false, true, false, false, false, true, false, false],
               "age_years": 82, "sex": "male", "tug_seconds": 14.2, "chair_stands": 7,
               "home_hazards": ["throw rugs"], "plan_format": "pdf"}),
    )
    .await;
    assert_eq!(s["data"]["screen"]["score"], 6);
    assert_eq!(s["data"]["overall"], "at_risk");
    assert!(s["data"]["plan_file"].as_str().unwrap().ends_with(".pdf"));
    assert!(decision_support(&s) && pending(&s));
    let e = call_err(&h, "steadi_fall_risk_assessment", json!({})).await;
    assert!(e.contains("Stay Independent"));
}

// ------------------------------------------------------------------ AGE-03

#[tokio::test]
async fn protective_services_routes_and_fills_locally() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "elder_protective_services_guide",
        json!({"age_years": 84, "concern": "financial exploitation", "reporter_role": "public health nurse",
               "details_file": "protective_report_details.csv"}),
    )
    .await;
    assert_eq!(s["data"]["pathways"][0]["hotline"], "(800) 922-2275");
    assert_eq!(s["data"]["mandated_reporter"], true);
    assert_eq!(s["data"]["local_fill"]["documents_written"], 2);
    assert!(decision_support(&s));
    let r = h
        .call(
            "elder_protective_services_guide",
            json!({"age_years": 84, "details_file": "protective_report_details.csv"}),
        )
        .await;
    assert!(leaks(&everything(&r), "data/protective_report_details.csv", None).is_empty());
    let s = call_ok(
        &h,
        "elder_protective_services_guide",
        json!({"age_years": 35, "has_disability": true}),
    )
    .await;
    assert_eq!(s["data"]["pathways"][0]["id"], "dppc");
}

// ------------------------------------------------------------------ AGE-02

#[tokio::test]
async fn aging_services_for_a_town() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "find_ma_aging_services", json!({"town": "Pittsfield"})).await;
    assert_eq!(s["data"]["asaps"].as_array().unwrap().len(), 1);
    assert!(s["data"]["shine"]["phone"].as_str().is_some());
    assert!(!s["data"]["meals"].as_array().unwrap().is_empty());
    assert!(pending(&s));
    let e = call_err(&h, "find_ma_aging_services", json!({})).await;
    assert!(e.contains("city or town"));
}

// ------------------------------------------------------------------ AGE-04

#[tokio::test]
async fn coverage_options_screen() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "explain_older_adult_coverage_options",
        json!({"age_years": 70, "household_size": 2, "monthly_income": 3900, "has_medicare_a_and_b": true, "town": "Barnstable"}),
    )
    .await;
    assert_eq!(s["data"]["msp_level"], "SLMB/QI");
    assert_eq!(s["data"]["medicare_year"], 2026);
    assert!(
        s["data"]["pace_in_county"]
            .as_str()
            .unwrap()
            .contains("not currently available")
    );
    assert!(decision_support(&s));
}

// ------------------------------------------------------------------ AGE-06

#[tokio::test]
async fn nursing_home_lookup_offline() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "nursing_home_quality_lookup",
        json!({"county": "Berkshire", "sort_by": "overall_rating"}),
    )
    .await;
    assert_eq!(s["data"]["mode"], "snapshot");
    assert!(s["data"]["matched"].as_u64().unwrap() >= 5);
    let ccn = s["data"]["facilities"][0]["ccn"]
        .as_str()
        .unwrap()
        .to_string();
    let s = call_ok(&h, "nursing_home_quality_lookup", json!({"ccn": ccn})).await;
    assert_eq!(s["data"]["matched"], 1);
    assert!(
        s["data"]["infection_control_tags"]["F0880"]
            .as_str()
            .is_some()
    );
    let e = call_err(
        &h,
        "nursing_home_quality_lookup",
        json!({"county": "Atlantis"}),
    )
    .await;
    assert!(e.contains("county"));
}

// ------------------------------------------------------------------ AGE-05

#[tokio::test]
async fn older_adult_vulnerability_with_map() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "map_older_adult_vulnerability",
        json!({"area": "Franklin County", "rank_by": "no_vehicle", "map": true, "limit": 5}),
    )
    .await;
    assert_eq!(s["data"]["subarea_level"], "town");
    assert_eq!(s["data"]["subareas"].as_array().unwrap().len(), 5);
    assert!(!s["privacy"]["files_written"].as_array().unwrap().is_empty());
    assert_eq!(s["privacy"]["row_level_data"], false);
}

// ------------------------------------------------------------------ AGE-07

#[tokio::test]
async fn advance_care_planning_materials_and_local_fill() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "advance_care_planning_toolkit",
        json!({"session_minutes": 45, "materials_format": "docx", "details_file": "proxy_details.csv"}),
    )
    .await;
    assert_eq!(s["data"]["session_minutes"], 45);
    assert_eq!(s["data"]["molst"]["status"], "not_verified");
    assert_eq!(s["data"]["local_fill"]["documents_written"], 3);
    assert!(decision_support(&s));
    let text = everything(
        &h.call(
            "advance_care_planning_toolkit",
            json!({"details_file": "proxy_details.csv"}),
        )
        .await,
    );
    assert!(leaks(&text, "data/proxy_details.csv", None).is_empty());
    assert!(!text.contains("https://www.molst-ma.org"));
}
