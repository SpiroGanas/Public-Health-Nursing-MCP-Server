//! MCP integration tests for the Phase 14 water tools (ENV-03 drinking water
//! quality, CHR-07 community water fluoridation): offline calls fall back to
//! the dated snapshots and say so, output matches each tool's schema, problems
//! the nurse can fix are tool-level errors, positions are shown side by side,
//! and the brief lands in the output folder.

mod common;

use common::{everything, start, summary};
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

#[tokio::test]
async fn drinking_water_lookup_falls_back_to_the_snapshot_offline() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "drinking_water_quality_lookup",
        json!({"town": "Boxborough"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["town"], "Boxborough");
    assert_eq!(d["sdwis"]["mode"], "snapshot");
    assert_eq!(d["pfas6"]["mode"], "snapshot");
    assert!(caveats(&s).contains("COULD NOT BE REACHED LIVE"));
    assert!(caveats(&s).contains("TOOL DEFAULT"));
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    let systems = d["systems"].as_array().unwrap();
    assert!(!systems.is_empty());
    assert!(systems.iter().all(|x| x["system_type"] == "CWS"));
    assert!(d["non_community_not_shown"].as_u64().unwrap() > 0);
    // Boxborough is largely on private wells and on MassDEP's arsenic/uranium list.
    assert_eq!(d["likely_private_wells"], true);
    assert_eq!(
        d["private_well_guidance"]["arsenic_uranium_town"],
        Value::Bool(true)
    );
    // Sources carry their mode.
    let modes: Vec<&str> = s["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["mode"].as_str().unwrap())
        .collect();
    assert!(modes.contains(&"snapshot"));
    // No operator contact details: SDWIS rows are reduced to system fields.
    let text = s.to_string();
    assert!(!text.contains("@"), "an email address leaked");

    // Non-community systems on request; a system by PWSID.
    let s = call_ok(
        &h,
        "drinking_water_quality_lookup",
        json!({"town": "Boxborough", "include_non_community": true, "years": 10}),
    )
    .await;
    assert_eq!(s["data"]["non_community_not_shown"], 0);
    assert!(
        s["data"]["systems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["system_type"] != "CWS")
    );
    let s = call_ok(
        &h,
        "drinking_water_quality_lookup",
        json!({"system": "MA2002000", "private_wells": false}),
    )
    .await;
    assert_eq!(s["data"]["town"], "Acton");
    assert_eq!(s["data"]["systems"].as_array().unwrap().len(), 1);
    assert!(s["data"]["private_well_guidance"].is_null());

    // Problems the nurse can fix are tool errors.
    let e = call_err(
        &h,
        "drinking_water_quality_lookup",
        json!({"town": "Bostn"}),
    )
    .await;
    assert!(e.contains("Boston"), "{e}");
    call_err(
        &h,
        "drinking_water_quality_lookup",
        json!({"town": "Norfolk County"}),
    )
    .await;
    call_err(
        &h,
        "drinking_water_quality_lookup",
        json!({"town": "Acton", "years": 0}),
    )
    .await;
    call_err(
        &h,
        "drinking_water_quality_lookup",
        json!({"system": "MA9999999"}),
    )
    .await;
}

#[tokio::test]
async fn fluoridation_status_shows_positions_side_by_side_and_writes_a_brief() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "community_water_fluoridation_status",
        json!({"town": "Amherst"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["town"]["dph_2023_status"], "fluoridated");
    assert!(
        d["town"]["cdc_systems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["status"] == "Adjusted")
    );
    let bodies: Vec<&str> = d["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["body"].as_str().unwrap())
        .collect();
    for b in [
        "Centers for Disease Control and Prevention",
        "U.S. Environmental Protection Agency",
        "U.S. Food and Drug Administration",
        "American Academy of Pediatrics",
    ] {
        assert!(bodies.contains(&b), "{b} missing");
    }
    assert!(
        d["positions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| !p["date"].as_str().unwrap().is_empty())
    );
    assert!(!d["evidence"].as_array().unwrap().is_empty());
    assert!(!d["massachusetts_law"].as_array().unwrap().is_empty());
    assert!(!d["positions_not_included"].as_array().unwrap().is_empty());
    assert!(
        d["statewide"]["percent_population_fluoridated"]
            .as_f64()
            .unwrap()
            > 50.0
    );
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    assert!(d["brief_file"].is_null());

    // Statewide only, and a DOCX brief.
    let s = call_ok(
        &h,
        "community_water_fluoridation_status",
        json!({"town": "Massachusetts"}),
    )
    .await;
    assert!(s["data"]["town"].is_null());
    let s = call_ok(
        &h,
        "community_water_fluoridation_status",
        json!({"town": "Fall River", "brief": true}),
    )
    .await;
    let f = s["data"]["brief_file"].as_str().unwrap();
    assert!(
        h.data()
            .join("output")
            .join(f.rsplit('/').next().unwrap())
            .exists()
    );
    assert_eq!(s["privacy"]["files_written"][0], f);

    let e = call_err(
        &h,
        "community_water_fluoridation_status",
        json!({"town": "Hampshire County"}),
    )
    .await;
    assert!(e.contains("town by town"), "{e}");
    call_err(
        &h,
        "community_water_fluoridation_status",
        json!({"town": "Atlantis"}),
    )
    .await;
}
