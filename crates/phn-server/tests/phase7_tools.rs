//! MCP integration tests for the Phase 7 tools (school health): successful
//! calls validate against each tool's output schema, problems the nurse can
//! fix are tool-level errors, clinical and legal answers carry the
//! decision-support notice and review caveats, local-file tools return only
//! suppressed aggregates and write identifiable output to /data/output, and
//! the synthetic fixtures' identifier columns are detected.

mod common;

use std::collections::BTreeSet;

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

fn decision_support(s: &Value) -> bool {
    s["notice"]
        .as_str()
        .is_some_and(|n| n.starts_with(DECISION_SUPPORT))
}

fn pending(s: &Value) -> bool {
    caveats(s).contains("has not yet been reviewed")
}

/// No count from 1 to threshold - 1 appears in a list of string cells.
fn no_small_counts(cells: impl IntoIterator<Item = String>, threshold: u64) {
    for c in cells {
        if let Ok(n) = c.parse::<u64>() {
            assert!(n == 0 || n >= threshold, "unsuppressed small count {n}");
        }
    }
}

// ------------------------------------------------------------------ SCH-01

#[tokio::test]
async fn screening_requirements_by_grade_and_screening() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "get_ma_school_screening_requirements",
        json!({"grade": "7"}),
    )
    .await;
    let screenings = s["data"]["screenings"].as_array().unwrap();
    let get = |id: &str| screenings.iter().find(|x| x["id"] == id).unwrap().clone();
    assert!(
        get("vision")["for_grade"]["status"]
            .as_str()
            .unwrap()
            .contains("once")
    );
    assert!(
        get("height_weight_bmi")["for_grade"]["status"]
            .as_str()
            .unwrap()
            .starts_with("required")
    );
    assert!(
        get("postural")["for_grade"]["status"]
            .as_str()
            .unwrap()
            .starts_with("required")
    );
    assert!(decision_support(&s) && pending(&s));
    assert!(md(&s).contains("200.400"));
    let s = call_ok(
        &h,
        "get_ma_school_screening_requirements",
        json!({"screening": "SBIRT", "include_details": true}),
    )
    .await;
    assert_eq!(s["data"]["screenings"][0]["id"], "sbirt");
    assert!(caveats(&s).contains("97(c)"));
    let s = call_ok(&h, "get_ma_school_screening_requirements", json!({})).await;
    assert_eq!(s["data"]["grade_table"].as_array().unwrap().len(), 14);
    call_err(
        &h,
        "get_ma_school_screening_requirements",
        json!({"grade": "14"}),
    )
    .await;
    call_err(
        &h,
        "get_ma_school_screening_requirements",
        json!({"screening": "zzzz"}),
    )
    .await;
}

// ------------------------------------------------------------------ SCH-02

#[tokio::test]
async fn screening_tracker_returns_suppressed_rates_and_writes_letters_locally() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "track_school_screenings",
        json!({"path": "school_screenings.csv", "as_of": "2026-11-15", "write_letters": true,
               "school_name": "Aldergrove Elementary", "nurse_name": "School Nurse", "nurse_phone": "x0000"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["school_year"], "2026-2027");
    assert_eq!(d["students_read"], 28);
    let out = d["output_file"].as_str().unwrap();
    assert!(out.starts_with("output/") && h.data().join(out).exists());
    let threshold = d["suppression_threshold"].as_u64().unwrap();
    for r in d["rows"].as_array().unwrap() {
        no_small_counts(
            [
                "required",
                "screened",
                "referred",
                "referrals_completed",
                "follow_up_pending",
                "incomplete",
                "exempt",
                "due",
            ]
            .iter()
            .map(|k| r[*k].as_str().unwrap().to_string()),
            threshold,
        );
    }
    for b in d["bmi"].as_array().unwrap() {
        no_small_counts(
            b["categories"]
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_str().unwrap().to_string()),
            threshold,
        );
    }
    let letters = &d["letters"];
    assert!(letters["letters"].as_u64().unwrap() >= 1);
    assert!(letters["english_fallback"].as_u64().unwrap() >= 1);
    for f in letters["files"].as_array().unwrap() {
        assert!(h.data().join(f.as_str().unwrap()).exists());
    }
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert!(decision_support(&s));
    let found = leaks(&s.to_string(), "data/school_screenings.csv", None);
    assert!(found.is_empty(), "identifiers returned: {found:?}");
    // The letters themselves carry names (they are local files).
    let combined: String = letters["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f.as_str().unwrap().ends_with(".docx"))
        .map(|f| {
            let bytes = std::fs::read(h.data().join(f.as_str().unwrap())).unwrap();
            phn_local::docx::parse_docx(&bytes)
                .map(|d| d.plain_text())
                .unwrap_or_default()
        })
        .collect();
    assert!(!leaks(&combined, "data/school_screenings.csv", Some(&["name"])).is_empty());
    // Wrong file: a tool error that lists column names only.
    let e = call_err(
        &h,
        "track_school_screenings",
        json!({"path": "line_list.csv"}),
    )
    .await;
    assert!(leaks(&e, "data/line_list.csv", None).is_empty());
    call_err(
        &h,
        "track_school_screenings",
        json!({"path": "school_screenings.csv", "screenings": ["sbirt"]}),
    )
    .await;
}

// ------------------------------------------------------------------ SCH-03

#[tokio::test]
async fn medication_guide_cites_105_cmr_210() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "school_medication_administration_guide",
        json!({"question": "Can a trained teacher give stock epinephrine on a field trip?"}),
    )
    .await;
    let ids: BTreeSet<String> = s["data"]["topics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        ids.contains("epinephrine") && ids.contains("field_trips"),
        "{ids:?}"
    );
    assert!(md(&s).contains("210.010"));
    assert!(decision_support(&s) && pending(&s));
    let s = call_ok(&h, "school_medication_administration_guide", json!({})).await;
    assert!(s["data"]["index"].as_array().unwrap().len() >= 15);
    call_err(
        &h,
        "school_medication_administration_guide",
        json!({"topic": "zzzz"}),
    )
    .await;
}

// ------------------------------------------------------------------ SCH-04

#[tokio::test]
async fn health_plan_template_and_local_fill() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_individualized_health_plan",
        json!({"condition": "asthma"}),
    )
    .await;
    assert!(
        s["data"]["placeholders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "student_name")
    );
    assert!(decision_support(&s));
    let s = call_ok(
        &h,
        "build_individualized_health_plan",
        json!({"condition": "food allergy", "student_file": "health_plan_students.csv"}),
    )
    .await;
    let docs = &s["data"]["documents"];
    assert_eq!(docs["students"], 2);
    assert_eq!(docs["rows_skipped"], 3);
    for f in docs["files"].as_array().unwrap() {
        assert!(h.data().join(f.as_str().unwrap()).exists());
    }
    let found = leaks(&s.to_string(), "data/health_plan_students.csv", None);
    assert!(found.is_empty(), "identifiers returned: {found:?}");
    call_err(
        &h,
        "build_individualized_health_plan",
        json!({"condition": "gout"}),
    )
    .await;
}

// ------------------------------------------------------------------ SCH-05

#[tokio::test]
async fn concussion_plan_and_log() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "concussion_return_to_learn_tracker",
        json!({"injury_date": "2026-10-02", "diagnosis_date": "2026-10-03", "returned_to_school": "2026-10-06",
               "symptoms": ["headache"], "as_of": "2026-10-08"}),
    )
    .await;
    assert!(
        s["data"]["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["date"] == "2026-10-05")
    );
    assert!(md(&s).contains("201.010"));
    assert!(decision_support(&s) && pending(&s));
    let s = call_ok(
        &h,
        "concussion_return_to_learn_tracker",
        json!({"tracker_path": "concussion_log.csv", "write_plans": true, "as_of": "2026-11-01"}),
    )
    .await;
    let log = &s["data"]["log"];
    assert_eq!(log["injuries"], 6);
    assert!(h.data().join(log["output_file"].as_str().unwrap()).exists());
    let plans = &s["data"]["plans"];
    assert!(plans["plans"].as_u64().unwrap() >= 3);
    let found = leaks(&s.to_string(), "data/concussion_log.csv", None);
    assert!(found.is_empty(), "identifiers returned: {found:?}");
    call_err(
        &h,
        "concussion_return_to_learn_tracker",
        json!({"injury_date": "2026-10-02", "diagnosis_date": "2026-09-01"}),
    )
    .await;
}

// ------------------------------------------------------------------ SCH-06

#[tokio::test]
async fn absenteeism_flags_the_gi_spike_without_returning_names() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "monitor_school_absenteeism",
        json!({"path": "school_absence_log.csv"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["school_days"], 26);
    let signals = d["signals"].as_array().unwrap();
    assert!(
        signals
            .iter()
            .any(|x| x["school"] == "Aldergrove Elementary"
                && x["syndrome"].as_str().unwrap().starts_with("Gastro")),
        "{signals:?}"
    );
    assert!(md(&s).contains("300.134"));
    assert!(h.data().join(d["output_file"].as_str().unwrap()).exists());
    let found = leaks(&s.to_string(), "data/school_absence_log.csv", None);
    assert!(found.is_empty(), "identifiers returned: {found:?}");
    let s = call_ok(
        &h,
        "monitor_school_absenteeism",
        json!({"path": "school_absence_log.csv", "group_column": "Grade", "syndromes": ["gi"]}),
    )
    .await;
    assert!(
        s["data"]["schools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| !x["group"].is_null())
    );
    // Grouping by a name column is refused.
    call_err(
        &h,
        "monitor_school_absenteeism",
        json!({"path": "school_absence_log.csv", "group_column": "Student Name"}),
    )
    .await;
}

// ------------------------------------------------------------------ SCH-07

#[tokio::test]
async fn annual_report_from_local_logs() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_school_health_annual_report",
        json!({"visit_log_path": "health_office_visits.csv", "screening_path": "school_screenings.csv",
               "school_year": "2025-2026", "enrollment": 420, "school_name": "Aldergrove Elementary"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["school_year"], "2025-2026");
    let t = &d["totals"];
    assert!(t["student_encounters"].as_u64().unwrap() > 500);
    let rtc = t["returned_to_class_pct"].as_f64().unwrap();
    assert!(rtc > 50.0 && rtc < 100.0);
    assert!(h.data().join(d["workbook"].as_str().unwrap()).exists());
    assert!(
        h.data()
            .join(d["committee_summary"].as_str().unwrap())
            .exists()
    );
    assert!(d["dph_reports"].as_array().unwrap().len() >= 8);
    for f in [
        "data/health_office_visits.csv",
        "data/school_screenings.csv",
    ] {
        let found = leaks(&s.to_string(), f, None);
        assert!(found.is_empty(), "identifiers returned: {found:?}");
    }
    // The committee summary file contains aggregates only.
    let bytes = std::fs::read(h.data().join(d["committee_summary"].as_str().unwrap())).unwrap();
    let text = phn_local::docx::parse_docx(&bytes).unwrap().plain_text();
    assert!(leaks(&text, "data/health_office_visits.csv", None).is_empty());
    call_err(&h, "build_school_health_annual_report", json!({})).await;
}

// ------------------------------------------------------------------ SCH-08

#[tokio::test]
async fn child_care_checklist_and_worksheet() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "child_care_health_consultation_checklist",
        json!({"program_type": "large_group", "findings": [{"item": "med_training", "status": "not_met"}],
               "write_worksheet": true}),
    )
    .await;
    assert_eq!(s["data"]["gaps"].as_array().unwrap().len(), 1);
    assert!(
        h.data()
            .join(s["data"]["worksheet"].as_str().unwrap())
            .exists()
    );
    assert!(md(&s).contains("7.11"));
    assert!(decision_support(&s) && pending(&s));
    call_err(
        &h,
        "child_care_health_consultation_checklist",
        json!({"domains": ["parking"]}),
    )
    .await;
}

// ------------------------------------------------------------------ fixtures

#[tokio::test]
async fn phi_detector_flags_the_phase7_identifier_columns() {
    let data = common::corpus().join("data");
    for file in [
        "school_screenings.csv",
        "school_absence_log.csv",
        "health_office_visits.csv",
        "concussion_log.csv",
        "health_plan_students.csv",
    ] {
        let t = phn_local::table::read_table(&data.join(file), None).unwrap();
        let flagged: BTreeSet<String> = phn_local::phi::scan_table(&t)
            .into_iter()
            .map(|f| f.column)
            .collect();
        for s in common::seeded()
            .iter()
            .filter(|s| s.file == format!("data/{file}"))
        {
            let col = s.column.as_ref().unwrap();
            assert!(
                flagged.contains(col),
                "{file}: column '{col}' ({}) not flagged; flagged {flagged:?}",
                s.kind
            );
        }
    }
}

#[tokio::test]
async fn school_toolset_is_in_the_school_nurse_profile() {
    let h = start(|c| c.env_profile = Some(phn_core::Profile::SchoolNurse)).await;
    let tools = h.client.list_all_tools().await.unwrap();
    let names: BTreeSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    for t in [
        "get_ma_school_screening_requirements",
        "track_school_screenings",
        "school_medication_administration_guide",
        "build_individualized_health_plan",
        "concussion_return_to_learn_tracker",
        "monitor_school_absenteeism",
        "build_school_health_annual_report",
        "child_care_health_consultation_checklist",
    ] {
        assert!(
            names.contains(t),
            "{t} missing from the school_nurse profile"
        );
    }
}
