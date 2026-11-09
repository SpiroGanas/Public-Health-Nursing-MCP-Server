//! MCP integration tests for EPR-01 `generate_ics_forms` (Phase 13): the
//! output validates against the tool's schema, forms and the Incident Action
//! Plan land in the output folder, staff contacts from the roster appear only
//! in the documents (ADR 0009), and problems the nurse can fix are tool-level
//! errors.

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
    // Nothing from the staff roster reaches the client.
    let found = leaks(&everything(&r), "data/staff_roster.csv", None);
    assert!(found.is_empty(), "{name} returned roster values: {found:?}");
    structured
}

fn docx_text(h: &common::Harness, display: &str) -> String {
    phn_local::docx::read_docx(&h.data().join(display))
        .unwrap()
        .plain_text()
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

fn measles_iap() -> Value {
    json!({
        "incident_name": "Maple Grove School Measles",
        "incident_number": "2026-BOH-014",
        "incident_type": "measles_outbreak",
        "period_start": "2026-11-10 07:00",
        "period_end": "2026-11-10 19:00",
        "prepared_at": "2026-11-09 20:00",
        "objectives": [
            "Identify and notify all students and staff exposed at Maple Grove School on 11/6 by 1200.",
            "Offer MMR to every eligible susceptible contact at the 1400 clinic."
        ],
        "situational_awareness": "Clear and cold; clinic in the school gym.",
        "safety_message": "Only staff with documented measles immunity work with contacts. Wear a fit-tested N95.",
        "organization": [
            {"position": "Incident Commander"},
            {"position": "PIO"},
            {"position": "Operations Section Chief"},
            {"position": "Planning Section Chief"},
            {"position": "Safety Officer"},
            {"position": "Medical Unit Leader"},
            {"position": "Public Health Nurse"}
        ],
        "assignments": [
            {"group": "Contact Tracing Group", "work_assignments": ["Call every exposed household by 1200."],
             "resources": [{"identifier": "PHN Team 1", "persons": 3}],
             "communications": [{"name_or_function": "Command", "channel": "CH 1"}]},
            {"division": "Gym Clinic", "work_assignments": ["Run the MMR clinic from 1400 to 1800."]}
        ],
        "radio_channels": [
            {"channel_number": "1", "function": "Command", "channel_name": "BOH CMD", "assignment": "Command", "mode": "A"}
        ],
        "medical_plan": {
            "transportation": [{"service": "Town Fire EMS", "location": "Station 1", "contact": "911", "level": "ALS"}],
            "hospitals": [{"name": "Regional Medical Center", "address": "1 Hospital Way", "contact": "Main line", "travel_time_ground": "12 min", "trauma_level": "II", "burn_center": false, "helipad": true}],
            "special_procedures": "Call 911, then notify the Safety Officer."
        },
        "roster_path": "staff_roster.csv",
        "format": "both"
    })
}

#[tokio::test]
async fn iap_is_written_with_roster_contacts_only_in_the_documents() {
    let h = start(|_| {}).await;
    let s = call_ok(&h, "generate_ics_forms", measles_iap()).await;
    let d = &s["data"];
    let forms: Vec<&str> = d["forms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["form"].as_str().unwrap())
        .collect();
    assert_eq!(
        forms,
        vec!["202", "203", "204", "205", "205A", "206", "207", "208"]
    );
    // One ICS 204 per Division/Group; DOCX and PDF for each form.
    let f204 = d["forms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["form"] == "204")
        .unwrap();
    assert_eq!(f204["copies"], 2);
    assert_eq!(f204["files"].as_array().unwrap().len(), 2);
    let iap = &d["iap"];
    let iap_files: Vec<&str> = iap["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert_eq!(iap_files.len(), 2);
    for f in &iap_files {
        assert!(h.data().join(f).exists(), "{f}");
    }
    let roster = &d["roster"];
    assert_eq!(roster["staff"], 12);
    assert!(roster["matched"].as_u64().unwrap() >= 6, "{roster}");
    assert!(s["privacy"]["files_written"].as_array().unwrap().len() >= 17);
    assert!(s["privacy"]["identifiers_detected"].is_null());
    // The roster's names and contacts are in the documents on disk.
    let docx = iap_files.iter().find(|f| f.ends_with(".docx")).unwrap();
    let text = docx_text(&h, docx);
    for v in [
        "Peregrine Ashdown-Vell",
        "781-555-0130",
        "thessaly.brightwater@example.org",
        "Barnaby Cresswold",
        "NOT FOR PUBLIC RELEASE",
        "INCIDENT OBJECTIVES (ICS 202)",
        "Maple Grove School Measles",
        "Regional Medical Center",
        "11/10/2026",
    ] {
        assert!(text.contains(v), "IAP is missing {v}");
    }
    // Completeness: the 206 is complete; the 205A lists contacts; suggestions are offered.
    let f206 = d["forms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["form"] == "206")
        .unwrap();
    assert_eq!(
        f206["blank_required"].as_array().unwrap().len(),
        0,
        "{f206}"
    );
    assert_eq!(d["suggestions"]["incident_type"], "measles_outbreak");
    assert_eq!(d["suggestions"]["used_in_forms"], false);
    assert!(s["notice"].is_string(), "decision-support notice");
    assert!(caveats(&s).contains("adapt"));
    // The IAP passes the server's own accessibility check (DOCX and PDF).
    for f in &iap_files {
        let r = h
            .call("check_document_accessibility", json!({"path": f}))
            .await;
        assert_eq!(r.is_error, Some(false), "{}", summary(&r));
        let report = &r.structured_content.unwrap()["data"]["report"];
        assert_eq!(report["failed"], 0, "{f}: {report}");
        assert_eq!(report["warnings"], 0, "{f}: {report}");
    }
    assert!(caveats(&s).contains("has not yet been reviewed by the subject matter expert"));
}

#[tokio::test]
async fn single_forms_report_blank_required_blocks_and_write_blanks() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "generate_ics_forms",
        json!({
            "forms": ["201", "213RR", "214"],
            "incident_name": "Heat Emergency",
            "resource_requests": [{
                "request_number": "RR-3",
                "items": [{"quantity": "200", "kind": "Supplies", "description": "Bottled water, 16.9 oz cases"}],
                "delivery_location": "Senior Center cooling site",
                "requested_by": {"name": "L. Quinn", "position": "Logistics Section Chief"},
                "priority": "urgent"
            }],
            "activity_logs": [{"unit": "Medical Unit", "leader": {"name": "R. Diaz", "position": "Medical Unit Leader"},
                               "entries": [{"time": "0900", "text": "Opened first aid station at the cooling site."}]}],
            "blank_forms": true
        }),
    )
    .await;
    let d = &s["data"];
    assert!(d["iap"].is_null(), "no IAP for non-IAP forms");
    assert_eq!(d["blank_files"].as_array().unwrap().len(), 3);
    let f201 = d["forms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["form"] == "201")
        .unwrap();
    let blanks: Vec<&str> = f201["blank_required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["block"].as_str().unwrap())
        .collect();
    assert!(
        blanks.iter().any(|b| b.contains("Date/Time Initiated")),
        "{blanks:?}"
    );
    assert!(
        blanks
            .iter()
            .any(|b| b.contains("Current and Planned Objectives"))
    );
    let rr = d["forms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["form"] == "213RR")
        .unwrap();
    assert_eq!(rr["blank_required"].as_array().unwrap().len(), 0, "{rr}");
    let file = rr["files"][0].as_str().unwrap();
    let text = docx_text(&h, file);
    assert!(text.contains("Bottled water") && text.contains("Urgent"));
    let blank = d["blank_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .find(|f| f.contains("ICS_214"))
        .unwrap();
    let text = docx_text(&h, blank);
    assert!(text.contains("Instructions for ICS 214 (FEMA)"));
    assert!(text.contains("Notable Activities"));
}

#[tokio::test]
async fn contacts_typed_into_the_request_are_flagged() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "generate_ics_forms",
        json!({"forms": ["213"], "messages": [{"subject": "Clinic staffing",
               "message": "Call the clinic lead at 413-555-0177 tonight."}]}),
    )
    .await;
    let ids = s["privacy"]["identifiers_detected"].as_array().unwrap();
    assert!(!ids.is_empty());
    assert!(
        s["privacy"]["notes"]
            .to_string()
            .contains("roster file in /data")
    );
}

#[tokio::test]
async fn bad_input_is_a_tool_error() {
    let h = start(|_| {}).await;
    for (args, expect) in [
        (json!({"forms": ["209"]}), "not supported"),
        (json!({"period_start": "next Tuesday"}), "YYYY-MM-DD HH:MM"),
        (
            json!({"period_start": "2026-11-10 19:00", "period_end": "2026-11-10 07:00"}),
            "after period_start",
        ),
        (
            json!({"roster_path": "missing_roster.csv"}),
            "missing_roster.csv",
        ),
        (json!({"organization": [{"position": " "}]}), "position"),
    ] {
        let r = h.call("generate_ics_forms", args.clone()).await;
        assert_eq!(r.is_error, Some(true), "{args}: {}", summary(&r));
        assert!(
            everything(&r).contains(expect),
            "{args}: {}",
            everything(&r)
        );
    }
    // A roster that is not a staff list is refused without echoing its values.
    let r = h
        .call(
            "generate_ics_forms",
            json!({"roster_path": "counts_by_town.csv"}),
        )
        .await;
    assert_eq!(r.is_error, Some(true));
    assert!(everything(&r).contains("needs a name column"));
}
