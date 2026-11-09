//! MCP integration tests for the Phase 9 tools (PHN practice toolkit and
//! case management): successful calls validate against each tool's output
//! schema, problems the nurse can fix are tool-level errors, clinical answers
//! carry the decision-support notice and review caveats, files land in the
//! output folder, and local data comes back only as counts.

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
    caveats(s).contains("REVIEW STATUS")
}

fn decision_support(s: &Value) -> bool {
    s["notice"]
        .as_str()
        .is_some_and(|d| d.contains(DECISION_SUPPORT))
}

fn files(s: &Value) -> Vec<String> {
    s["privacy"]["files_written"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

// ------------------------------------------------------------------ CD-11

#[tokio::test]
async fn tb_contact_plan_assigns_priorities_and_writes_a_workbook() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "plan_tb_contact_investigation",
        json!({
            "index_case": {"symptomatic": true, "afb_smear_positive": true, "cavitary": true,
                           "symptom_onset_date": "2026-06-10",
                           "first_positive_finding_date": "2026-08-01"},
            "contact_groups": [
                {"setting": "household", "count": 4, "age_band": "16_plus",
                 "last_exposure_date": "2026-08-05"},
                {"setting": "child_care", "count": 6, "age_band": "under_5"},
                {"setting": "workplace", "count": 20, "age_band": "16_plus"}
            ],
            "listed_date": "2026-08-07"
        }),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["investigation_recommended"], true);
    assert_eq!(d["infectious_period_start"], "2026-03-10");
    let g = d["contact_groups"].as_array().unwrap();
    assert_eq!(g[0]["priority"], "high");
    assert_eq!(g[0]["retest_window_opens"], "2026-09-30");
    assert_eq!(g[1]["priority"], "high");
    assert_eq!(g[2]["priority"], "low");
    assert!(decision_support(&s) && pending(&s));
    let wb = d["workbook"].as_str().unwrap();
    assert!(h.data().join(wb).exists());
    assert!(!d["dph_tb_program"]["phone_business_hours"].as_str().unwrap().is_empty());

    // A local contact list: counts and expansion signals only.
    let r = h
        .call(
            "plan_tb_contact_investigation",
            json!({
                "index_case": {"symptomatic": true, "afb_smear_positive": true,
                               "symptom_onset_date": "2026-06-10"},
                "contact_file": "tb_contacts.csv", "listed_date": "2026-08-07"
            }),
        )
        .await;
    assert!(leaks(&everything(&r), "data/tb_contacts.csv", None).is_empty());
    let s = r.structured_content.unwrap();
    let fs = &s["data"]["contact_file"];
    assert_eq!(fs["contacts_classified"], 12);
    assert!(
        fs["expansion_signals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x.as_str().unwrap().contains("under 5"))
    );
    assert_eq!(s["privacy"]["suppression_applied"], true);

    let e = call_err(
        &h,
        "plan_tb_contact_investigation",
        json!({"index_case": {"symptom_onset_date": "June 10"}}),
    )
    .await;
    assert!(e.contains("YYYY-MM-DD"));
}

// ------------------------------------------------------------------ CD-12

#[tokio::test]
async fn ltbi_calendar_has_windows_visits_and_no_doses() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "ltbi_treatment_calendar",
        json!({"regimen": "3HP", "start_date": "2026-01-05",
               "doses_taken": ["2026-01-05", "2026-01-12"], "as_of": "2026-01-14",
               "language": "Somali"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["total_doses"], 12);
    assert_eq!(d["completion"]["deadline"], "2026-04-26");
    assert_eq!(d["adherence"]["doses_counted"], 2);
    assert_eq!(d["adherence"]["status"], "on_track");
    assert_eq!(d["patient_language"]["used"], "English");
    assert!(d["dosing"].as_str().unwrap().contains("does not calculate"));
    assert_eq!(files(&s).len(), 2);
    assert!(decision_support(&s) && pending(&s));
    let e = call_err(
        &h,
        "ltbi_treatment_calendar",
        json!({"regimen": "2 months of rifampin and pyrazinamide", "start_date": "2026-01-05"}),
    )
    .await;
    assert!(e.contains("unknown regimen"));
}

// ------------------------------------------------------------------ EQ-05

#[tokio::test]
async fn hrsn_screen_scores_domains_and_batches_with_suppression() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "screen_health_related_social_needs",
        json!({"answers": {"1": "not steady", "3": "often true", "5": "no", "6": "no",
                           "7": 2, "8": 2, "9": 1, "10": 1}, "age_years": 30}),
    )
    .await;
    let pos: Vec<&str> = s["data"]["positive_domains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(pos.contains(&"living_situation") && pos.contains(&"food"));
    assert!(!pos.contains(&"safety"));
    assert!(decision_support(&s) && pending(&s));

    let s = call_ok(
        &h,
        "screen_health_related_social_needs",
        json!({"path": "hrsn_screenings.csv", "group_by": "Site"}),
    )
    .await;
    assert_eq!(s["data"]["batch"]["screenings"], 15);
    assert_eq!(files(&s).len(), 1);
    assert!(leaks(&s.to_string(), "data/hrsn_screenings.csv", None).is_empty());

    let e = call_err(
        &h,
        "screen_health_related_social_needs",
        json!({"answers": {"q99": "yes"}}),
    )
    .await;
    assert!(e.contains("not an AHC item"));
}

// ------------------------------------------------------------------ CORE-01/02/03

#[tokio::test]
async fn omaha_wheel_and_documentation_never_echo_notes() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "omaha_system_coder",
            json!({"path": "client_note.txt", "write_care_plan": true}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    assert!(leaks(&everything(&r), "data/client_note.txt", None).is_empty());
    let s = call_ok(
        &h,
        "omaha_system_coder",
        json!({"text": "Taught client about medication side effects and set up a pill box. \
                        Referred to food pantry because client cannot pay for food.",
               "ratings": [{"problem": "Medication regimen", "knowledge": 3, "baseline_knowledge": 2}]}),
    )
    .await;
    assert!(
        s["data"]["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["code"] == "42")
    );
    assert_eq!(s["data"]["ratings"][0]["change"]["knowledge"], 1);
    assert!(!s.to_string().contains("pill box. Referred"));

    let s = call_ok(
        &h,
        "classify_intervention_wheel",
        json!({"activities": ["Drafted a smoke-free housing regulation for the board of health",
                              "Flu shots under standing orders at the senior center"]}),
    )
    .await;
    assert_eq!(
        s["data"]["classified"][0]["intervention"],
        "policy_development_enforcement"
    );
    assert_eq!(s["data"]["interventions"].as_array().unwrap().len(), 17);

    let r = h
        .call(
            "nursing_documentation_template",
            json!({"check_path": "client_note.txt", "format": "soap"}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    assert!(leaks(&everything(&r), "data/client_note.txt", None).is_empty());
    let s = r.structured_content.unwrap();
    assert!(!s["data"]["check"]["missing"].as_array().unwrap().is_empty());
    let s = call_ok(
        &h,
        "nursing_documentation_template",
        json!({"format": "dar", "visit_type": "case_investigation", "write_template": true}),
    )
    .await;
    assert_eq!(files(&s).len(), 2);
    let e = call_err(
        &h,
        "omaha_system_coder",
        json!({"lookup": "zzzzqqqq"}),
    )
    .await;
    assert!(e.contains("nothing in the Omaha System"));
}

// ------------------------------------------------------------------ EQ-07

#[tokio::test]
async fn community_resources_search_directory_and_health_centers() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "find_community_resources",
        json!({"town": "Amherst", "service": "food", "language": "Spanish"}),
    )
    .await;
    let r = s["data"]["results"].as_array().unwrap();
    assert_eq!(r[0]["name"], "Pioneer Valley Example Food Pantry");
    let s = call_ok(
        &h,
        "find_community_resources",
        json!({"town": "Worcester", "service": "primary care", "include_county": false}),
    )
    .await;
    assert_eq!(s["data"]["health_center_data_mode"], "snapshot");
    assert!(
        s["data"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["town"] == "Worcester")
    );
    let e = call_err(
        &h,
        "find_community_resources",
        json!({"directory_path": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("list of people"));
}

// ------------------------------------------------------------------ EQ-11

#[tokio::test]
async fn newcomer_navigator_covers_screening_coverage_and_shelter() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "newcomer_health_navigator",
        json!({"status": "asylee", "age_years": 45, "arrival_date": "2026-09-01",
               "as_of": "2026-11-08", "write_checklist": true}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["screening_due_by"], "2026-11-30");
    assert_eq!(d["coverage"]["october_2026_change"], "group_2");
    assert!(!d["shelter"].as_array().unwrap().is_empty());
    assert!(!d["programs"].as_array().unwrap().is_empty());
    assert_eq!(files(&s).len(), 1);
    assert!(decision_support(&s) && pending(&s));
}
