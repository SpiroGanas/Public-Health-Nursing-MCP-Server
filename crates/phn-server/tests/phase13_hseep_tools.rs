//! MCP integration tests for the Phase 13 HSEEP tools: design_hseep_exercise
//! (EPR-08) and build_after_action_report (EPR-09). Successful calls validate
//! against each tool's output schema, problems the nurse can fix are
//! tool-level errors, documents land in the output folder, staff contact
//! details from the roster stay in those files, and feedback forms come back
//! only as suppressed aggregates.

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
    // Contacts from the roster never come back.
    let all = everything(&r);
    let found = leaks(&all, "data/staff_roster.csv", None);
    assert!(
        found.is_empty(),
        "{name} returned roster details: {found:?}"
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

fn docx_text(h: &common::Harness, display: &str) -> String {
    let bytes = std::fs::read(h.data().join(display)).unwrap();
    phn_local::docx::parse_docx(&bytes).unwrap().plain_text()
}

#[tokio::test]
async fn measles_tabletop_package_for_pilot_3() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "design_hseep_exercise",
        json!({
            "scenario": "measles_outbreak",
            "area": "Amherst",
            "date": "2027-03-17",
            "location": "Amherst Town Hall",
            "organizations": ["DPH Office of Preparedness and Emergency Management", "Amherst Public Schools", "Medical Reserve Corps"],
            "participants": [
                {"role": "Incident Commander"},
                {"role": "Public Information Officer"},
                {"role": "Medical Unit Leader", "exercise_role": "evaluator"},
                {"role": "Planning Section Chief", "exercise_role": "facilitator"}
            ],
            "point_of_contact": {"role": "Incident Commander"},
            "roster_path": "staff_roster.csv"
        }),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["exercise_type"], "Tabletop Exercise");
    assert_eq!(d["scenario"]["kind"], "developed");
    assert_eq!(d["objectives"].as_array().unwrap().len(), 5);
    assert_eq!(d["modules"].as_array().unwrap().len(), 3);
    assert!(
        d["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["framework"] == "FEMA")
    );
    assert_eq!(d["roster"]["matched"], 5);
    assert_eq!(d["participants"]["evaluators"], 1);
    assert!(caveats(&s).contains("fictional exercise content"));
    assert!(caveats(&s).contains("has not yet been reviewed by the subject matter expert"));
    let docs = d["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 5);
    let file = |name: &str| {
        docs.iter()
            .find(|x| x["name"].as_str().unwrap().starts_with(name))
            .unwrap()["files"][0]
            .as_str()
            .unwrap()
            .to_string()
    };
    // The SitMan has the scenario; the POC's contact details are only in the file.
    let sitman = docx_text(&h, &file("Situation Manual"));
    assert!(sitman.contains("Amherst Elementary School"));
    assert!(sitman.contains("peregrine.ashdownvell@example.org"));
    // EEGs carry the evaluator's roster contact.
    let eeg = docx_text(&h, &file("Exercise Evaluation Guide"));
    assert!(eeg.contains("ignatius.hollowmere@example.org"));
    assert!(eeg.contains("PHEP 13"));
    let sign_in = docx_text(&h, &file("Sign-in"));
    assert!(sign_in.contains("Thessaly Brightwater"));
}

#[tokio::test]
async fn functional_exercise_writes_msel_workbook() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "design_hseep_exercise",
        json!({"exercise_type": "functional", "scenario": "heat", "area": "Lowell", "documents": "both"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["category"], "operations");
    let files: Vec<String> = d["documents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|x| x["files"].as_array().unwrap().iter())
        .map(|f| f.as_str().unwrap().to_string())
        .collect();
    assert!(files.iter().any(|f| f.ends_with("msel.xlsx")));
    assert!(files.iter().any(|f| f.ends_with("exercise_plan.pdf")));
    for f in &files {
        assert!(h.data().join(f).exists(), "{f}");
    }
}

#[tokio::test]
async fn exercise_input_problems_are_tool_errors() {
    let h = start(|_| {}).await;
    let e = call_err(&h, "design_hseep_exercise", json!({"area": "Amherst"})).await;
    assert!(e.contains("needs a scenario"), "{e}");
    let e = call_err(
        &h,
        "design_hseep_exercise",
        json!({"scenario": "volcano", "area": "Amherst"}),
    )
    .await;
    assert!(e.contains("not in the library"), "{e}");
    let e = call_err(
        &h,
        "design_hseep_exercise",
        json!({"scenario": "measles", "date": "next March"}),
    )
    .await;
    assert!(e.contains("YYYY-MM-DD"), "{e}");
}

#[tokio::test]
async fn after_action_report_with_feedback_forms() {
    let h = start(|_| {}).await;
    let args = json!({
        "name": "Amherst Measles Response Tabletop Exercise",
        "exercise_type": "tabletop exercise",
        "start_date": "2027-03-17",
        "area": "Amherst",
        "objectives": [
            {"text": "Within 24 hours of DPH notification, discuss how exposed contacts will be identified and notified, in accordance with the DPH measles guide.",
             "capabilities": ["13"], "ratings": [{"capability": "13", "rating": "S"}]},
            {"text": "Within 4 hours of confirmation, discuss how a public message will be approved and released through the agency's public information procedures.",
             "capabilities": ["4"], "ratings": [{"capability": "4", "rating": "M"}]}
        ],
        "observations": [
            {"kind": "strength", "objective": 1, "capability": "13",
             "statement": "Players set the exposure windows from the infectious period within minutes."},
            {"kind": "area_for_improvement", "objective": 1, "capability": "13",
             "statement": "There is no agreed way to obtain urgent care patient lists after hours.",
             "reference": "Communicable disease procedures", "analysis": "No after-hours contacts exist for urgent care centers."},
            {"kind": "area_for_improvement", "objective": 2, "capability": "Emergency Public Information and Warning",
             "statement": "Translated measles messages were not available.",
             "reference": "Public information procedures", "analysis": "Templates exist only in English.",
             "recommendation": "Prepare translated templates for three measles messages."}
        ],
        "corrective_actions": [
            {"area_for_improvement": 1, "action": "Develop an after-hours contact list for the 4 urgent care centers in town",
             "element": "planning", "organization": "Amherst Board of Health", "poc_position": "Incident Commander",
             "start_date": "2027-04-01", "completion_date": "2027-06-30"},
            {"area_for_improvement": 2, "action": "Prepare translated templates for 3 measles messages in Spanish and Portuguese",
             "element": "planning", "organization": "Amherst Board of Health", "poc_position": "Public Information Officer",
             "start_date": "2027-04-01", "completion_date": "2027-05-31"}
        ],
        "participants": [{"group": "local", "count": 14}, {"group": "state", "count": 2}],
        "point_of_contact": {"position": "Incident Commander"},
        "roster_path": "staff_roster.csv",
        "feedback_path": "exercise_feedback.csv"
    });
    let r = h.call("build_after_action_report", args.clone()).await;
    let all = everything(&r);
    let found = leaks(&all, "data/exercise_feedback.csv", None);
    assert!(found.is_empty(), "feedback identifiers returned: {found:?}");
    for comment in [
        "vaccine supply delays",
        "Call me about joining",
        "next exercise in the spring",
    ] {
        assert!(!all.contains(comment), "comment returned: {comment}");
    }
    let s = call_ok(&h, "build_after_action_report", args).await;
    let d = &s["data"];
    assert_eq!(d["areas_for_improvement"], 2);
    let checks: Vec<(&str, bool)> = d["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["id"].as_str().unwrap(), c["passed"].as_bool().unwrap()))
        .collect();
    assert!(
        checks.iter().all(|(_, ok)| *ok),
        "all checks should pass: {checks:?}"
    );
    let f = &d["feedback"];
    assert_eq!(f["forms"], 14);
    assert_eq!(f["factors"].as_array().unwrap().len(), 8);
    assert!(f["comments"].as_u64().unwrap() > 0);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    let files = d["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    let aar = docx_text(&h, files[0].as_str().unwrap());
    assert!(aar.contains("Improvement Plan") && aar.contains("Performed with Major Challenges"));
    assert!(aar.contains("peregrine.ashdownvell@example.org"));
}

#[tokio::test]
async fn after_action_report_flags_names_and_bad_input() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_after_action_report",
        json!({
            "kind": "incident",
            "name": "Spring flooding response",
            "objectives": [{"text": "Open the shelter within 6 hours of the request, in accordance with the shelter plan.", "capabilities": ["Mass Care"]}],
            "observations": [{"kind": "area_for_improvement", "capability": "Mass Care",
                              "statement": "Nurse Maria Garcia could not reach the shelter manager at 413-555-0199."}],
            "documents": "none"
        }),
    )
    .await;
    let d = &s["data"];
    let failed: Vec<&str> = d["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["passed"] == false)
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(failed.contains(&"non_attribution") && failed.contains(&"client_identifiers"));
    assert!(failed.contains(&"afi_has_corrective_action"));
    assert!(caveats(&s).contains("real incident"));
    let e = call_err(
        &h,
        "build_after_action_report",
        json!({"observations": [{"kind": "rumor", "capability": "4", "statement": "x"}]}),
    )
    .await;
    assert!(e.contains("strength or area_for_improvement"), "{e}");
    let e = call_err(&h, "build_after_action_report", json!({})).await;
    assert!(e.contains("objectives"), "{e}");
}
