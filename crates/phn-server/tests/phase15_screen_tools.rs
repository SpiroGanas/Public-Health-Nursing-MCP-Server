//! MCP integration tests for the Phase 15 screening tools (CHR-02
//! `community_bp_screening_toolkit`, CHR-03 `diabetes_prevention_finder`): offline calls
//! succeed and match each tool's output schema, tool defaults and unverified items are labelled,
//! problems the nurse can fix are tool errors, files land in the output folder, and local files
//! come back only as suppressed counts or inside letters.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

async fn call_ok(h: &common::Harness, name: &str, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h.call(name, args.clone()).await;
    assert_eq!(r.is_error, Some(false), "{name} {args}: {}", everything(&r));
    assert!(!summary(&r).trim_start().starts_with('{'));
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

fn output_file(h: &common::Harness, display: &str) -> std::path::PathBuf {
    h.data().join(display)
}

fn count(table: &Value, label_starts: &str) -> String {
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["label"].as_str().unwrap().starts_with(label_starts))
        .unwrap_or_else(|| panic!("no row {label_starts} in {table}"))["count"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn bp_kit_is_labelled_dated_and_written_to_the_output_folder() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "community_bp_screening_toolkit", json!({})).await;
    let d = &s["data"];
    assert!(
        d["guideline"]
            .as_str()
            .unwrap()
            .contains("retires and replaces")
    );
    let cats: Vec<&str> = d["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        cats,
        vec!["normal", "elevated", "stage1", "stage2", "severe"]
    );
    // Every category states its confidence; stage ranges are medium, CDC's are high.
    let conf = |id: &str| {
        d["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["confidence"]
            .clone()
    };
    assert_eq!(conf("normal"), "high");
    assert_eq!(conf("stage2"), "medium");
    // Referral tiers are the project's defaults, never presented as guideline text.
    let tiers = d["referral_tiers"].as_array().unwrap();
    assert_eq!(tiers.len(), 5);
    assert!(
        tiers
            .iter()
            .all(|t| t["status"] == "tool default, pending review")
    );
    // Technique steps that were not found at a source say so.
    let tech = d["technique"].as_array().unwrap();
    assert!(tech.iter().any(|t| t["verified"] == false));
    assert!(tech.iter().any(|t| t["verified"] == true));
    // Severe range: the sources' wording and the pregnancy thresholds.
    let sev = &d["severe_range"];
    assert!(sev["pregnancy"].as_str().unwrap().contains("160/110"));
    assert!(!sev["source_wording"].as_array().unwrap().is_empty());
    assert!(pending(&s));
    assert!(caveats(&s).contains("tool default"));
    assert!(s["notice"].as_str().is_some());
    // Files: log template (XLSX and CSV), checklist, reference card, 3 letters, handout.
    let files = d["files"].as_array().unwrap();
    assert_eq!(files.len(), 8);
    for f in files {
        assert!(output_file(&h, f["file"].as_str().unwrap()).exists(), "{f}");
        if let Some(ok) = f["meets_target"].as_bool() {
            assert!(ok, "{f}");
        }
    }
    assert!(s["sources"].as_array().unwrap().iter().any(|x| {
        x["publisher"]
            .as_str()
            .unwrap()
            .contains("Centers for Disease Control")
            && x["effective_date"].is_string()
    }));
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn bp_readings_are_classified_and_bad_input_is_a_tool_error() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "community_bp_screening_toolkit",
        json!({"readings": [{"systolic": 146, "diastolic": 92}, {"systolic": 142, "diastolic": 90}]}),
    )
    .await;
    let r = &s["data"]["reading"];
    assert_eq!(r["rounded"], "144/91");
    assert_eq!(r["category_id"], "stage2");
    assert_eq!(r["referral"]["id"], "refer");
    // Typed readings do not write the kit.
    assert!(s["data"]["kit_folder"].is_null());
    let s = call_ok(
        &h,
        "community_bp_screening_toolkit",
        json!({"readings": [{"systolic": 190, "diastolic": 112}], "symptoms": true}),
    )
    .await;
    assert_eq!(s["data"]["reading"]["referral"]["id"], "emergency");
    assert!(
        s["data"]["reading"]["note"]
            .as_str()
            .unwrap()
            .contains("Only one reading")
    );
    let s = call_ok(
        &h,
        "community_bp_screening_toolkit",
        json!({"readings": [{"systolic": 142, "diastolic": 88}], "pregnant": true}),
    )
    .await;
    assert_eq!(s["data"]["reading"]["category_id"], "pregnancy_high");
    // Errors the nurse can fix.
    for args in [
        json!({"readings": [{"systolic": 80, "diastolic": 120}]}),
        json!({"readings": [{"systolic": 120, "diastolic": 80}], "age": 12}),
        json!({"age": 50}),
        json!({"write_letters": true}),
        json!({"log_path": "does_not_exist.csv"}),
    ] {
        call_err(&h, "community_bp_screening_toolkit", args).await;
    }
}

#[tokio::test]
async fn bp_log_is_summarized_as_suppressed_counts_and_letters_stay_local() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "community_bp_screening_toolkit",
        json!({"log_path": "bp_screening_log.csv", "write_letters": true}),
    )
    .await;
    let text = serde_json::to_string(&s).unwrap();
    let found = leaks(&text, "data/bp_screening_log.csv", None);
    assert!(found.is_empty(), "returned identifiers: {found:?}");
    let log = &s["data"]["log_summary"];
    let tables = log["tables"].as_array().unwrap();
    assert_eq!(tables.len(), 4);
    // 32 rows: the total is shown, and every shown count is a number or "*".
    assert_eq!(tables[0]["total"], "32");
    for t in tables {
        for r in t["rows"].as_array().unwrap() {
            let c = r["count"].as_str().unwrap();
            assert!(c == "*" || c.parse::<u32>().is_ok(), "{c}");
        }
    }
    // Normal readings are common in the log; the severe range is small, so it is suppressed.
    assert!(count(&tables[0], "Normal").parse::<u32>().is_ok());
    // The under-18 row and the row without a reading are outside the adult categories.
    let l = &s["data"]["letters"];
    assert_eq!(l["letters"], 30);
    assert!(output_file(&h, l["file"].as_str().unwrap()).exists());
    assert!(l["file"].as_str().unwrap().contains("output"));
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert!(
        !s["privacy"]["identifiers_detected"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(pending(&s));
    // The blank kit is not written when a log is summarized.
    assert!(s["data"]["kit_folder"].is_null());
}

#[tokio::test]
async fn diabetes_risk_test_is_scored_with_cdcs_points() {
    let h = start(|c| c.offline = true).await;
    // 54-year-old man, 5 ft 10 and 240 lb (BMI 34.4): age 2, man 1, family 1, hypertension 1,
    // inactive 1, BMI 2 = 8.
    let s = call_ok(
        &h,
        "diabetes_prevention_finder",
        json!({"age": 54, "sex": "man", "family_history": true, "high_blood_pressure": true,
               "physically_active": false, "height_inches": 70, "weight_pounds": 240,
               "diabetes_diagnosis": false, "pregnant": false, "area": "Fall River"}),
    )
    .await;
    let r = &s["data"]["risk_test"];
    assert_eq!(r["score"], 8);
    assert_eq!(r["maximum"], 10);
    assert_eq!(r["high_risk"], true);
    assert_eq!(r["bmi"], 34.4);
    assert_eq!(s["data"]["eligibility"]["overall"], "Meets CDC eligibility");
    assert!(pending(&s));
    // CDC and CMS sources are dated; DPH's list is a snapshot.
    let sources = s["sources"].as_array().unwrap();
    assert!(sources.iter().any(|x| {
        x["publisher"]
            .as_str()
            .unwrap()
            .contains("Centers for Medicare")
    }));
    assert!(sources.iter().any(|x| {
        x["mode"] == "snapshot"
            && x["publisher"]
                .as_str()
                .unwrap()
                .contains("Department of Public Health")
    }));
    // An Asian American woman with BMI 24 and gestational diabetes.
    let s = call_ok(
        &h,
        "diabetes_prevention_finder",
        json!({"age": 45, "sex": "woman", "gestational_diabetes": true, "family_history": false,
               "high_blood_pressure": false, "physically_active": true, "bmi": 24,
               "asian_american": true, "diabetes_diagnosis": false, "pregnant": false}),
    )
    .await;
    assert_eq!(s["data"]["risk_test"]["score"], 3);
    assert_eq!(s["data"]["eligibility"]["overall"], "Meets CDC eligibility");
    // Missing answers, children, and unusable sex are tool errors.
    for args in [
        json!({"age": 50}),
        json!({"age": 15, "sex": "man", "family_history": false, "high_blood_pressure": false,
               "physically_active": true, "bmi": 22}),
        json!({"age": 50, "sex": "other", "family_history": false, "high_blood_pressure": false,
               "physically_active": true, "bmi": 22}),
        json!({"area": "Atlantis"}),
        json!({"sheet": "Sheet1"}),
    ] {
        call_err(&h, "diabetes_prevention_finder", args).await;
    }
}

#[tokio::test]
async fn diabetes_programs_are_listed_honestly_and_without_individual_contacts() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "diabetes_prevention_finder",
        json!({"area": "Fall River", "handout": true}),
    )
    .await;
    let p = &s["data"]["programs"];
    assert_eq!(p["area_level"], "town");
    assert_eq!(p["in_area"].as_array().unwrap().len(), 4);
    assert!(!p["same_county"].as_array().unwrap().is_empty());
    assert_eq!(p["sites_listed_statewide"], 73);
    assert!(
        p["source_note"]
            .as_str()
            .unwrap()
            .contains("no update date")
    );
    assert!(
        p["registry_gap"]
            .as_str()
            .unwrap()
            .contains("could not be read")
    );
    let text = caveats(&s);
    assert!(text.contains("CDC") && text.contains("registry"));
    // No person's name or email from DPH's contact column.
    let all = serde_json::to_string(&s).unwrap();
    for who in ["Bermudez", "Potvin", "Charleus", "bermudezp@", "SueKP@"] {
        assert!(!all.contains(who), "{who}");
    }
    // Statements that could not be sourced are stated as such.
    assert!(
        s["data"]["masshealth"]
            .as_str()
            .unwrap()
            .contains("No source was found")
    );
    assert!(
        s["data"]["medicare"]["facts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["text"].as_str().unwrap().contains("Part B"))
    );
    assert!(s["data"]["delivery"]["how_to_find_online_and_distance_options"].is_string());
    // The handout is written and readable.
    let hd = &s["data"]["handout"];
    assert_eq!(hd["meets_target"], true);
    assert!(output_file(&h, hd["file"].as_str().unwrap()).exists());
    // Statewide: the New Hampshire site is listed separately.
    let s = call_ok(
        &h,
        "diabetes_prevention_finder",
        json!({"area": "Massachusetts"}),
    )
    .await;
    assert_eq!(
        s["data"]["programs"]["outside_massachusetts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn diabetes_responses_are_counted_with_suppression_only() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "diabetes_prevention_finder",
        json!({"area": "Holyoke", "responses_path": "prediabetes_responses.csv"}),
    )
    .await;
    let text = serde_json::to_string(&s).unwrap();
    let found = leaks(&text, "data/prediabetes_responses.csv", None);
    assert!(found.is_empty(), "returned identifiers: {found:?}");
    let t = &s["data"]["responses"]["tables"][0];
    assert_eq!(t["total"], "26");
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert_eq!(s["privacy"]["row_level_data"], false);
    // A file without the test's columns is refused without echoing values.
    let e = call_err(
        &h,
        "diabetes_prevention_finder",
        json!({"area": "Holyoke", "responses_path": "line_list.csv"}),
    )
    .await;
    assert!(leaks(&e, "data/line_list.csv", None).is_empty());
}
