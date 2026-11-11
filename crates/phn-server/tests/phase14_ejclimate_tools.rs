//! MCP integration tests for the Phase 14 environmental justice and climate
//! tools (ENV-04, ENV-05): offline calls succeed and match each tool's output
//! schema, problems the nurse can fix are tool-level errors, files land in the
//! output folder, key facts appear, and a local EJScreen file comes back only
//! as averages and counts.

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

#[tokio::test]
async fn ej_screen_reports_a_town_county_and_the_state_offline() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "environmental_justice_screen",
        json!({"area": "Lowell"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["summary"]["ej_block_groups"], 77);
    assert_eq!(d["summary"]["total_block_groups"], 81);
    assert_eq!(d["official_statistics"]["percent_population_in_ej_block_groups"], 96.1);
    assert!(d["english_isolation"]["block_groups"].as_u64().unwrap() > 0);
    assert!(!d["english_isolation"]["top_languages"].as_array().unwrap().is_empty());
    assert_eq!(d["dph_vulnerable_health_criteria"].as_array().unwrap().len(), 4);
    assert!(s["summary"].as_str().unwrap().contains("Language access"));
    assert!(s["notice"].is_string());
    assert!(pending(&s));
    assert!(caveats(&s).contains("EPA removed EJScreen"));

    let s = call_ok(
        &h,
        "environmental_justice_screen",
        json!({"area": "Hampden County", "map": true, "table": true}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["towns"][0]["town"], "Springfield");
    let files = d["map_files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    for f in files {
        assert!(output_file(&h, f.as_str().unwrap()).exists(), "{f}");
    }
    let t = d["table_file"].as_str().unwrap();
    assert!(output_file(&h, t).exists());
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 3);

    let s = call_ok(
        &h,
        "environmental_justice_screen",
        json!({"area": "Massachusetts"}),
    )
    .await;
    assert_eq!(s["data"]["summary"]["ej_block_groups"], 2599);
    assert_eq!(s["data"]["summary"]["towns_with_ej_block_groups"], 187);
}

#[tokio::test]
async fn ej_screen_summarizes_an_ejscreen_file_without_rows() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "environmental_justice_screen",
        json!({"area": "Lowell", "ejscreen_path": "ejscreen_ma_extract.csv"}),
    )
    .await;
    let e = &s["data"]["ejscreen"];
    assert_eq!(e["block_groups_in_area"], 22);
    let pm = e["indicators"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["column"] == "P_PM25")
        .unwrap();
    assert!(pm["mean_percentile"].as_f64().unwrap() > 40.0);
    assert!(
        e["columns_missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "P_NO2")
    );
    let text = serde_json::to_string(&s).unwrap();
    assert!(
        leaks(&text, "data/ejscreen_ma_extract.csv", None).is_empty(),
        "{text}"
    );
    // The extract's reviewer columns are reported as identifiers found.
    assert!(
        !s["privacy"]["identifiers_detected"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let err = call_err(
        &h,
        "environmental_justice_screen",
        json!({"area": "Lowell", "ejscreen_path": "line_list.csv"}),
    )
    .await;
    assert!(leaks(&err, "data/line_list.csv", None).is_empty(), "{err}");
    let err = call_err(
        &h,
        "environmental_justice_screen",
        json!({"area": "Springfeld"}),
    )
    .await;
    assert!(err.contains("Springfield"));
}

#[tokio::test]
async fn climate_profile_works_offline_and_writes_a_document() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "climate_health_vulnerability_profile",
        json!({"area": "Scituate", "document": true}),
    )
    .await;
    let d = &s["data"];
    assert!(d["flood_history"]["claims"].as_u64().unwrap() > 1000);
    assert_eq!(d["planning"]["plans_mode"], "snapshot");
    assert_eq!(d["planning"]["nfip"]["mode"], "snapshot");
    assert!(
        d["most_important"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["indicator"] == "nfip_claims")
    );
    for i in d["heat"].as_array().unwrap() {
        assert!(i["massachusetts"].is_number(), "{i}");
        assert!(i["vintage"].as_str().unwrap().contains("20"));
    }
    let f = d["document_file"].as_str().unwrap();
    assert!(output_file(&h, f).exists());
    assert!(caveats(&s).contains("Tool default"));
    assert!(caveats(&s).contains("bundled snapshot"));
    assert!(pending(&s));
    let err = call_err(
        &h,
        "climate_health_vulnerability_profile",
        json!({"area": "Bristol County"}),
    )
    .await;
    assert!(err.contains("city or town"));
}
