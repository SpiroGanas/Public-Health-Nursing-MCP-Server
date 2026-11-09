//! MCP integration tests for the Phase 12 tools (emergency preparedness I):
//! successful calls validate against each tool's output schema, problems the
//! nurse can fix are tool-level errors, live sources degrade clearly when
//! offline, files land in the output folder, and local files come back only
//! as suppressed aggregates.

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
    h.data()
        .join("output")
        .join(display.rsplit('/').next().unwrap())
}

#[tokio::test]
async fn hazard_alerts_say_when_live_alerts_are_unavailable() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "get_active_hazard_alerts",
        json!({"area": "Barnstable County", "guidance_for": ["winter storm"]}),
    )
    .await;
    assert_eq!(s["data"]["alerts_status"], "unavailable");
    assert!(caveats(&s).contains("COULD NOT BE RETRIEVED"));
    let ids: Vec<&str> = s["data"]["hazards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids[0], "winter_storm");
    assert!(ids.contains(&"carbon_monoxide"));
    assert!(!s["data"]["declarations"].as_array().unwrap().is_empty());
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    let e = call_err(
        &h,
        "get_active_hazard_alerts",
        json!({"guidance_for": ["volcano"]}),
    )
    .await;
    assert!(e.contains("no hazard guidance matches"));
    let e = call_err(
        &h,
        "get_active_hazard_alerts",
        json!({"area": "Springfeld"}),
    )
    .await;
    assert!(e.contains("Springfield"));
}

#[tokio::test]
async fn dispensing_model_sizes_stations_and_writes_a_worksheet() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "model_dispensing_site_throughput",
        json!({"population": 30000, "sites": 3, "operating_hours": 24, "worksheet": true}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["feasible"], true);
    assert!(d["staff_per_shift_per_site"].as_u64().unwrap() > 10);
    assert!(output_file(&h, d["worksheet_file"].as_str().unwrap()).exists());
    assert!(caveats(&s).contains("tool default"));
    let e = call_err(
        &h,
        "model_dispensing_site_throughput",
        json!({"population": 1000}),
    )
    .await;
    assert!(e.contains("operating_hours"));
    let e = call_err(
        &h,
        "model_dispensing_site_throughput",
        json!({"population": 1000, "operating_hours": 10, "max_utilization": 2}),
    )
    .await;
    assert!(e.contains("max_utilization"));
}

#[tokio::test]
async fn air_quality_gives_category_actions_and_converts_concentrations() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "air_quality_protective_guidance",
        json!({"area": "Lowell", "aqi": 165, "pollutant": "PM2.5"}),
    )
    .await;
    assert_eq!(s["data"]["guidance_category"], "Unhealthy");
    let g = &s["data"]["guidance"][0];
    assert!(g["schools"][0].as_str().unwrap().contains("indoors"));
    assert!(
        g["sensitive_groups"][0]
            .as_str()
            .unwrap()
            .starts_with("PM2.5")
    );
    let s = call_ok(
        &h,
        "air_quality_protective_guidance",
        json!({"pollutant": "PM10", "concentration": 210}),
    )
    .await;
    assert_eq!(s["data"]["computed"]["aqi"], 128);
    let e = call_err(
        &h,
        "air_quality_protective_guidance",
        json!({"pollutant": "pollen", "concentration": 3}),
    )
    .await;
    assert!(e.contains("PM2.5"));
}

#[tokio::test]
async fn heat_plan_writes_a_checklist_without_returning_cooling_center_contacts() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            "heat_health_action_planner",
            json!({"area": "Springfield", "plan_for_level": 4, "cooling_centers_path": "cooling_centers.csv"}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    assert!(leaks(&everything(&r), "data/cooling_centers.csv", None).is_empty());
    let s = r.structured_content.unwrap();
    let d = &s["data"];
    assert_eq!(d["activation_level"], 4);
    assert_eq!(d["cooling_centers"]["sites"], 4);
    assert_eq!(d["cooling_centers"]["marked_accessible"], 3);
    assert!(output_file(&h, d["checklist_file"].as_str().unwrap()).exists());
    assert!(
        d["population"]["electricity_dependent"]["electricity_dependent"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(!d["outreach_scripts"].as_array().unwrap().is_empty());
    let e = call_err(
        &h,
        "heat_health_action_planner",
        json!({"area": "Massachusetts"}),
    )
    .await;
    assert!(e.contains("town"));
}

#[tokio::test]
async fn hva_ranks_scored_hazards_and_writes_a_workbook() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "hazard_vulnerability_assessment",
        json!({"area": "Plymouth County", "scores": [
            {"hazard": "hurricane", "probability": 2, "human": 3, "property": 3, "operations": 2,
             "preparedness": 2, "internal_response": 2, "external_response": 1},
            {"hazard": "power outage", "probability": 3, "human": 2, "property": 1, "operations": 2,
             "preparedness": 2, "internal_response": 2, "external_response": 2}
        ]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["scored"], 2);
    assert_eq!(d["hazards"][0]["id"], "extended_power_outage");
    assert_eq!(d["hazards"][0]["relative_risk_percent"], 61.1);
    assert!(output_file(&h, d["workbook_file"].as_str().unwrap()).exists());
    assert!(caveats(&s).contains("National Risk Index"));
    let e = call_err(
        &h,
        "hazard_vulnerability_assessment",
        json!({"scores": [{"hazard": "hurricane", "probability": 5}]}),
    )
    .await;
    assert!(e.contains("probability"));
}

#[tokio::test]
async fn shelter_intake_fills_forms_locally_and_returns_suppressed_counts() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            "functional_needs_shelter_intake",
            json!({"area": "Brockton", "expected_residents": 120, "intake_path": "shelter_registrations.csv", "blank_form": "both"}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    assert!(leaks(&everything(&r), "data/shelter_registrations.csv", None).is_empty());
    let s = r.structured_content.unwrap();
    let b = &s["data"]["batch"];
    assert_eq!(b["registrants"], 12);
    assert_eq!(b["languages"]["English"], "6");
    assert_eq!(b["languages"]["Spanish"], "<5");
    assert!(b["forms_in_english_fallback"].as_u64().unwrap() >= 5);
    assert!(output_file(&h, b["output_file"].as_str().unwrap()).exists());
    assert_eq!(s["data"]["blank_forms"].as_array().unwrap().len(), 2);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert!(!s["data"]["needs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn electricity_dependence_by_county_and_refuses_individual_files() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "get_electricity_dependent_population",
        json!({"area": "Suffolk County"}),
    )
    .await;
    assert_eq!(s["data"]["mode"], "snapshot");
    assert!(s["data"]["electricity_dependent"].as_f64().unwrap() > 1000.0);
    assert!(!s["data"]["zip_codes"].as_array().unwrap().is_empty());
    let e = call_err(
        &h,
        "get_electricity_dependent_population",
        json!({"local_path": "shelter_registrations.csv"}),
    )
    .await;
    assert!(e.contains("individual-level"));
}

#[tokio::test]
async fn casper_plan_is_reproducible_and_analysis_is_weighted() {
    let h = start(|c| c.offline = true).await;
    let a = call_ok(
        &h,
        "plan_casper_assessment",
        json!({"area": "Fall River", "seed": 99, "materials": false}),
    )
    .await;
    let b = call_ok(
        &h,
        "plan_casper_assessment",
        json!({"area": "Fall River", "seed": 99, "materials": false}),
    )
    .await;
    assert_eq!(a["data"]["clusters"], b["data"]["clusters"]);
    assert_eq!(a["data"]["seed"], 99);
    let r = h
        .call(
            "plan_casper_assessment",
            json!({"area": "Fall River", "seed": 99, "results_path": "casper_results.csv",
                   "questions": ["damage", "has_generator"], "tracking": {"attempted": 400, "contacted": 260}}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    assert!(leaks(&everything(&r), "data/casper_results.csv", None).is_empty());
    let s = r.structured_content.unwrap();
    let an = &s["data"]["analysis"];
    assert_eq!(an["completed_interviews"], 198);
    assert_eq!(an["weighted"], true);
    assert_eq!(an["completion_rate"], 94.3);
    let q = &an["questions"][0];
    assert!(
        q["estimates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["ci_lower"].is_number())
    );
    assert_eq!(s["data"]["files"].as_array().unwrap().len(), 2);
    let e = call_err(
        &h,
        "plan_casper_assessment",
        json!({"area": "Fall River", "results_path": "casper_results.csv", "questions": ["Callback Phone"]}),
    )
    .await;
    assert!(
        e.contains("cannot be analyzed") || e.contains("identifiers"),
        "{e}"
    );
}
