//! MCP integration tests for the Phase 8 tools (maternal, child, and family health):
//! successful calls validate against each tool's output schema, problems the nurse can fix
//! are tool-level errors, clinical and legal answers carry the decision-support notice and
//! review caveats, files land in the output folder, and tools that read local files return
//! only suppressed counts.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

const DECISION_SUPPORT: &str = "Decision support only";

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

fn md(s: &Value) -> &str {
    s["summary"].as_str().unwrap()
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

fn notice(s: &Value) -> &str {
    s["notice"].as_str().unwrap_or("")
}

// ------------------------------------------------------------------ MCH-03

#[tokio::test]
async fn growth_percentiles_for_one_child_and_a_file() {
    let h = start(|_| {}).await;
    // A 9-month-old boy (CDC's worked example weight) with length and head circumference.
    let s = call_ok(
        &h,
        "calculate_growth_percentiles",
        json!({"sex": "male", "measurements": [
            {"age_months": 6, "weight_kg": 8.1, "length_or_height_cm": 68.0},
            {"age_months": 9, "weight_kg": 9.7, "length_or_height_cm": 72.0, "head_circumference_cm": 45.0}
        ]}),
    )
    .await;
    assert!(md(&s).contains("Weight-for-age"));
    assert!(md(&s).contains("WHO 2006"));
    assert!(notice(&s).contains(DECISION_SUPPORT));
    assert!(caveats(&s).contains("PENDING-REVIEW"));
    let visits = s["data"]["visits"].as_array().unwrap();
    assert_eq!(visits.len(), 2);
    assert!(
        !visits[1]["changes_since_previous"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A 10-year-old girl with obesity: CDC chart, BMI category, extended method.
    let s = call_ok(
        &h,
        "calculate_growth_percentiles",
        json!({"sex": "female", "measurements": [
            {"age_years": 10, "weight_lb": 110, "length_or_height_in": 55}
        ]}),
    )
    .await;
    let ind = s["data"]["visits"][0]["indicators"].as_array().unwrap();
    let bmi = ind
        .iter()
        .find(|i| i["indicator"] == "BMI-for-age")
        .unwrap();
    assert!(bmi["chart"].as_str().unwrap().contains("extended"));
    assert!(
        bmi["category"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("obesity")
    );

    // Batch mode: per-row results to the output folder, counts only in the response.
    let r = h
        .call(
            "calculate_growth_percentiles",
            json!({"measurements_path": "growth_measurements.csv"}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let all = everything(&r);
    assert!(leaks(&all, "data/growth_measurements.csv", None).is_empty());
    let s = r.structured_content.unwrap();
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    let out = s["data"]["batch"]["output_file"].as_str().unwrap();
    assert!(h.data().join(out).exists());
    assert_eq!(s["data"]["batch"]["rows_with_problems"], 2);

    // Errors the nurse can fix.
    let e = call_err(
        &h,
        "calculate_growth_percentiles",
        json!({"measurements": [{"age_months": 9, "weight_kg": 9.7}]}),
    )
    .await;
    assert!(e.contains("sex"));
    let e = call_err(
        &h,
        "calculate_growth_percentiles",
        json!({"sex": "female", "measurements": [{"age_months": 9, "weight_kg": 9.7, "weight_lb": 21}]}),
    )
    .await;
    assert!(e.contains("not both"));
}

// ------------------------------------------------------------------ MCH-04

#[tokio::test]
async fn lead_screening_guide_result_town_and_teaching_sheet() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "childhood_lead_screening_guide",
        json!({"child_age_months": 30, "town": "Lowell", "blood_lead_level": 7,
               "sample_type": "capillary", "teaching_sheet": true, "language": "Vietnamese"}),
    )
    .await;
    assert!(md(&s).contains("Confirm"));
    assert_eq!(s["data"]["town"]["high_risk_community"], true);
    assert!(notice(&s).contains(DECISION_SUPPORT));
    assert!(caveats(&s).contains("No agency-approved Vietnamese"));
    let f = s["data"]["teaching_sheet_file"].as_str().unwrap();
    assert!(h.data().join(f).exists());
    assert_eq!(s["privacy"]["files_written"][0], f);
    // Overview with no inputs lists the high-risk communities and the follow-up table.
    let s = call_ok(&h, "childhood_lead_screening_guide", json!({})).await;
    assert!(md(&s).contains("New Bedford"));
    assert!(md(&s).contains("Re-test"));
    let e = call_err(
        &h,
        "childhood_lead_screening_guide",
        json!({"blood_lead_level": 12}),
    )
    .await;
    assert!(e.contains("sample_type"));
}

// ------------------------------------------------------------------ MCH-07

#[tokio::test]
async fn mandated_reporter_guide_and_local_fill() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "mandated_reporter_51a_guide",
        json!({"oral_report_time": "2026-11-06 15:20", "concerns": ["neglect"], "narrative_template": true}),
    )
    .await;
    assert!(md(&s).contains("48 hours"));
    assert_eq!(s["data"]["written_report_due"], "2026-11-08 15:20");
    assert!(notice(&s).contains(DECISION_SUPPORT));
    assert!(caveats(&s).contains("PENDING-REVIEW"));
    assert!(s["data"]["template"]["sections"].as_array().unwrap().len() >= 8);

    // Local fill: a draft in the output folder, nothing from the notes in the response.
    let r = h
        .call(
            "mandated_reporter_51a_guide",
            json!({"notes_path": "51a_notes.txt"}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let all = everything(&r);
    assert!(leaks(&all, "data/51a_notes.txt", None).is_empty(), "{all}");
    assert!(!all.contains("oval bruises") && !all.contains("squeezed"));
    let s = r.structured_content.unwrap();
    let d = &s["data"]["drafts"][0];
    let f = d["file"].as_str().unwrap();
    assert!(h.data().join(f).exists());
    assert!(d["fields_filled"].as_u64().unwrap() >= 14);
    assert_eq!(s["privacy"]["files_written"][0], f);

    let e = call_err(
        &h,
        "mandated_reporter_51a_guide",
        json!({"notes_path": "counts_by_town.csv"}),
    )
    .await;
    assert!(e.contains("field names"));
}

// ------------------------------------------------------------------ MCH-02

#[tokio::test]
async fn developmental_milestones_checklist_and_referral() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "check_developmental_milestones",
        json!({"age_months": 24, "not_yet": ["24m-language-1"], "town": "Brockton"}),
    )
    .await;
    assert_eq!(s["data"]["checklist"]["age_months"], 24);
    assert_eq!(s["data"]["act_early"], true);
    assert!(!s["data"]["ei_programs"].as_array().unwrap().is_empty());
    assert!(md(&s).contains("Early Intervention"));
    assert!(notice(&s).contains(DECISION_SUPPORT));
    let e = call_err(
        &h,
        "check_developmental_milestones",
        json!({"age_months": 80}),
    )
    .await;
    assert!(e.contains("between 0 and 72"));
}

// ------------------------------------------------------------------ MCH-01

#[tokio::test]
async fn newborn_postpartum_visit_plan_and_documents() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "plan_newborn_postpartum_visit",
        json!({"baby_age_days": 5, "feeding": "breast", "birth_weight_g": 3400, "weight_g": 3150,
               "hearing_screen": "not_passed", "documents": true}),
    )
    .await;
    assert!(md(&s).contains("Safe sleep"));
    assert!(md(&s).contains("Hear Her"));
    assert!(s["data"]["weight_change_percent"].as_f64().unwrap() < 0.0);
    assert!(notice(&s).contains(DECISION_SUPPORT));
    let files = s["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    for f in files {
        assert!(h.data().join(f.as_str().unwrap()).exists());
    }
    let e = call_err(
        &h,
        "plan_newborn_postpartum_visit",
        json!({"baby_age_days": 400}),
    )
    .await;
    assert!(e.contains("0 to 365"));
}

// ------------------------------------------------------------------ EQ-06

#[tokio::test]
async fn benefit_eligibility_screen() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "estimate_benefit_eligibility",
        json!({"household_size": 4, "annual_income": 52000, "child_under_5": true,
               "children_under_19": true, "pays_for_heat": true}),
    )
    .await;
    assert!(md(&s).contains("% of the 2026 federal poverty guideline"));
    assert_eq!(s["data"]["fpl_annual"], 33000.0);
    assert!(s["data"]["programs"].as_array().unwrap().len() >= 8);
    assert!(notice(&s).contains(DECISION_SUPPORT));
    assert!(caveats(&s).contains("not an eligibility determination"));
    let e = call_err(
        &h,
        "estimate_benefit_eligibility",
        json!({"household_size": 4}),
    )
    .await;
    assert!(e.contains("income"));
}
