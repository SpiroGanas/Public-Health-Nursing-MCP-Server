//! MCP integration tests for EPR-10 `crisis_risk_communication_builder`
//! (Phase 13): successful calls validate against the output schema, language
//! priorities work offline from snapshots, agency translations in
//! /data/templates are used and briefs are written otherwise, the media
//! contact from the staff roster reaches only the Word packet, and problems
//! the nurse can fix are tool-level errors.

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

fn docx_text(h: &common::Harness, display: &str) -> String {
    let p = h.data().join(display);
    assert!(p.exists(), "{display} was not written");
    phn_local::docx::read_docx(&p)
        .expect("readable DOCX")
        .plain_text()
}

const TOOL: &str = "crisis_risk_communication_builder";

#[tokio::test]
async fn measles_message_set_works_offline_with_language_priorities() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        TOOL,
        json!({
            "hazard": "measles",
            "area": "Lowell",
            "organization": "Lowell Health Department",
            "details": {"location": "the Pollard Library", "exposure_times": "Tuesday, November 3, 2 to 5 p.m."},
            "what_happened": "A person with measles visited the Pollard Library on November 3.",
            "known": ["One person has measles."],
            "unknown": ["We do not know yet if anyone else is sick."],
            "being_done": ["We are working with the library to reach visitors."],
            "next_update": "by noon tomorrow",
            "info_source": "lowell.example.gov/measles"
        }),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["hazard_id"], "measles_exposure");
    assert_eq!(d["phase"], "initial");
    let map = &d["message_map"];
    assert_eq!(map["key_messages"].as_array().unwrap().len(), 3);
    assert!(
        map["key_messages"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Pollard Library")
    );
    assert!(
        d["missing"].as_array().unwrap().is_empty(),
        "{}",
        d["missing"]
    );
    assert!(
        d["holding_statement"]
            .as_str()
            .unwrap()
            .contains("What we do not know yet")
    );
    // Offline ACS snapshots still give the town's priority languages.
    let rows = d["language_plan"]["rows"].as_array().unwrap();
    assert!(!rows.is_empty(), "no languages for Lowell offline");
    assert!(rows.iter().all(|r| r["plan"] == "translator_brief"));
    assert!(
        rows.iter()
            .any(|r| r["lep_estimate"].as_f64().unwrap_or(0.0) > 0.0)
    );
    assert!(
        !d["spokesperson"]["questions_to_prepare"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let cv = caveats(&s);
    assert!(cv.contains("has not yet been reviewed by the subject matter expert"));
    assert!(cv.contains("machine-translated"));
    assert!(s["notice"].as_str().is_some(), "decision-support notice");
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn agency_translation_is_used_and_briefs_and_packet_are_written() {
    let h = start(|c| c.offline = true).await;
    let dir = h.data().join("templates").join("spanish");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("cerc_measles_exposure.md"),
        "# Exposición al sarampión\n\nLas personas que estuvieron en {{location}} pueden haber estado expuestas.\n\nMás información: {{info_source}}.\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        TOOL,
        json!({
            "hazard": "measles_exposure",
            "languages": ["Spanish", "Portuguese"],
            "details": {"location": "la Biblioteca Central", "exposure_times": "lunes de 1 a 4"},
            "info_source": "salud.example.gov",
            "roster_path": "staff_roster.csv",
            "export": "docx"
        }),
    )
    .await;
    let d = &s["data"];
    let rows = d["language_plan"]["rows"].as_array().unwrap();
    let es = rows.iter().find(|r| r["language"] == "Spanish").unwrap();
    let pt = rows.iter().find(|r| r["language"] == "Portuguese").unwrap();
    assert_eq!(es["plan"], "agency_translation");
    assert_eq!(pt["plan"], "translator_brief");
    let es_text = docx_text(&h, es["file"].as_str().unwrap());
    assert!(es_text.contains("la Biblioteca Central"), "{es_text}");
    let pt_text = docx_text(&h, pt["file"].as_str().unwrap());
    assert!(pt_text.contains("DRAFT TRANSLATION"), "{pt_text}");
    assert!(pt_text.contains("Two MMR doses"), "{pt_text}");
    // The packet carries the PIO's contacts from the roster; the chat does not.
    let files: Vec<&str> = d["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    let packet = files.iter().find(|f| f.contains("cerc_packet_")).unwrap();
    let packet_text = docx_text(&h, packet);
    assert!(
        packet_text.contains("781-555-0111"),
        "PIO phone in the packet"
    );
    assert!(packet_text.contains("Message map"));
    assert!(
        files
            .iter()
            .any(|f| f.ends_with("template_english_cerc_measles_exposure.md"))
    );
    assert_eq!(d["roster"]["matched"], 1);
    let r = h
        .call(
            TOOL,
            json!({"hazard": "measles", "languages": ["Spanish"], "roster_path": "staff_roster.csv", "export": "docx"}),
        )
        .await;
    let found = leaks(&everything(&r), "data/staff_roster.csv", None);
    assert!(found.is_empty(), "roster values returned: {found:?}");
}

#[tokio::test]
async fn own_map_draft_and_contacts_in_the_request_are_checked() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        TOOL,
        json!({
            "hazard_description": "water main break",
            "phase": "all",
            "languages": ["Haitian"],
            "spokesperson": "Health Agent Robin Vale, cell 617-555-0142",
            "message_map": [
                {"text": "Never drink the tap water in the north end of town or at the schools and businesses near the reservoir until we tell you that it is completely safe again.", "supporting": ["Boil it."]}
            ],
            "draft_text": "A devastating break happened."
        }),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["message_map"]["origin"], "yours");
    let failed: Vec<&str> = d["map_checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["passed"] == false)
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    for id in [
        "three_key_messages",
        "twenty_seven_words",
        "no_absolutes",
        "positive_actions",
    ] {
        assert!(failed.contains(&id), "{id}: {failed:?}");
    }
    assert!(
        d["draft_checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "draft_no_loaded_words" && c["passed"] == false)
    );
    assert_eq!(d["phase_tasks"].as_array().unwrap().len(), 4);
    assert_eq!(d["language_plan"]["rows"][0]["plan"], "translator_brief");
    let notes = s["privacy"]["notes"].to_string();
    assert!(notes.contains("roster"), "{notes}");
    assert!(
        s["privacy"]["identifiers_detected"]
            .to_string()
            .contains("phone")
    );
}

#[tokio::test]
async fn library_listing_and_errors() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, TOOL, json!({"phase": "preparation"})).await;
    assert_eq!(s["data"]["library"].as_array().unwrap().len(), 7);
    assert!(s["data"]["message_map"].is_null());
    assert_eq!(s["data"]["phase_tasks"][0]["phase"], "preparation");
    let e = call_err(&h, TOOL, json!({"hazard": "volcano"})).await;
    assert!(e.contains("hazard_description"), "{e}");
    let e = call_err(&h, TOOL, json!({"hazard": "heat", "export": "pdf"})).await;
    assert!(e.contains("docx"), "{e}");
    let e = call_err(&h, TOOL, json!({"hazard": "heat", "phase": "later"})).await;
    assert!(e.contains("preparation"), "{e}");
    let e = call_err(
        &h,
        TOOL,
        json!({"hazard": "heat", "roster_path": "no_such_roster.csv"}),
    )
    .await;
    assert!(e.contains("no_such_roster"), "{e}");
}
