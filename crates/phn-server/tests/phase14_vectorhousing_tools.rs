//! MCP integration tests for the Phase 14 vectors-and-housing tools (ENV-06,
//! ENV-07): offline calls succeed and match their output schemas, the
//! arbovirus levels are dated and never shown as current when they come from
//! the bundled copy, problems the nurse can fix are tool errors, documents land
//! in the output folder, and household files are used only inside the letters.

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
async fn vector_risk_for_a_town_is_dated_and_not_shown_as_current_offline() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "vector_borne_disease_risk",
        json!({"area": "Middleborough", "audiences": ["schools", "camps"], "notice": true}),
    )
    .await;
    let d = &s["data"];
    let a = &d["arbovirus"];
    assert_eq!(a["data_mode"], "snapshot");
    assert_eq!(a["current"], false);
    let as_of = a["as_of"].as_str().unwrap();
    assert!(d["season_status"].as_str().unwrap().contains(as_of));
    assert_eq!(a["town"]["town"], "Middleborough");
    assert_eq!(a["guidance"].as_array().unwrap().len(), 2);
    assert!(!a["history"].as_array().unwrap().is_empty());
    assert_eq!(d["tickborne"]["geography"], "Plymouth");
    let diseases: Vec<&str> = d["tickborne"]["diseases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["disease"].as_str().unwrap())
        .collect();
    assert!(diseases.contains(&"Lyme disease") && diseases.contains(&"Babesiosis"));
    let audiences: Vec<&str> = d["prevention"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["audience"].as_str().unwrap())
        .collect();
    assert_eq!(audiences, vec!["schools", "camps"]);
    assert!(output_file(&h, d["notice_file"].as_str().unwrap()).exists());
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    assert!(caveats(&s).contains("focal areas"));
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["mode"] == "snapshot" && x["effective_date"].as_str() == Some(as_of))
    );
}

#[tokio::test]
async fn vector_risk_statewide_for_an_earlier_season_and_bad_input() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "vector_borne_disease_risk",
        json!({"area": "Massachusetts", "season": 2024}),
    )
    .await;
    let a = &s["data"]["arbovirus"];
    assert_eq!(a["season"], 2024);
    let total: u64 = a["towns_by_level"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["eee_towns"].as_u64().unwrap())
        .sum();
    assert_eq!(total, 351);
    assert!(
        a["positives"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["virus"] == "EEE" && p["sample_type"] == "Human" && p["towns"] == 0)
    );
    assert_eq!(s["data"]["tickborne"]["ed_visits"].as_array().unwrap().len(), 13);
    let e = call_err(&h, "vector_borne_disease_risk", json!({"season": 2015})).await;
    assert!(e.contains("season must be from"));
    let e = call_err(&h, "vector_borne_disease_risk", json!({"audiences": ["pilots"]})).await;
    assert!(e.contains("unknown audience"));
    let e = call_err(&h, "vector_borne_disease_risk", json!({"area": "Midleborough"})).await;
    assert!(e.contains("Middleborough"));
}

#[tokio::test]
async fn housing_guide_explains_the_code_and_writes_readable_sheets() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "housing_conditions_health_guide",
        json!({"conditions": ["roaches", "a leak in the ceiling"], "area": "Holyoke",
               "languages": ["Spanish"], "format": "pdf"}),
    )
    .await;
    let d = &s["data"];
    let ids: Vec<&str> = d["conditions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["moisture_mold", "pests"]);
    let pests = &d["conditions"][1];
    assert_eq!(pests["urgent"], true);
    assert!(
        pests["code"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["section"] == "410.550(B)")
    );
    assert!(d["regulation"].as_str().unwrap().contains("105 CMR 410.000"));
    assert!(
        d["inspection"]["request"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["text"].as_str().unwrap().contains("1 business day"))
    );
    assert_eq!(
        d["inspection"]["urgent_conditions"].as_array().unwrap().len(),
        18
    );
    let sheets = d["teaching_sheets"].as_array().unwrap();
    assert_eq!(sheets.len(), 3);
    for sh in sheets {
        assert_eq!(sh["meets_target"], true, "{sh}");
        let f = sh["file"].as_str().unwrap();
        assert!(f.ends_with(".pdf"));
        assert!(output_file(&h, f).exists(), "{f}");
        // No agency translation in the corpus: English only, said plainly.
        assert!(sh["translations"][0]["file"].is_null());
    }
    assert!(caveats(&s).contains("does not machine-translate"));
    assert!(caveats(&s).contains("not legal advice"));
    assert!(d["local_housing"]["built_before_1980_pct"].as_f64().unwrap() > 0.0);
    assert!(
        d["tenant_protections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "repair_and_deduct"
                && p["citation"].as_str().unwrap().contains("127L"))
    );
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    let e = call_err(
        &h,
        "housing_conditions_health_guide",
        json!({"conditions": ["zebras"]}),
    )
    .await;
    assert!(e.contains("no housing condition matches"));
    let e = call_err(
        &h,
        "housing_conditions_health_guide",
        json!({"letters": "both"}),
    )
    .await;
    assert!(e.contains("households_path"));
}

#[tokio::test]
async fn housing_letters_are_filled_locally_and_never_returned() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            "housing_conditions_health_guide",
            json!({"conditions": ["heat", "lead"], "households_path": "housing_requests.csv",
                   "teaching_sheets": false}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let found = leaks(&everything(&r), "data/housing_requests.csv", None);
    assert!(found.is_empty(), "returned identifiers: {found:?}");
    let s = r.structured_content.clone().unwrap();
    let l = &s["data"]["letters"];
    assert_eq!(l["households"], 3);
    assert_eq!(l["letters"], 6);
    // Spanish and Portuguese have no agency translation in the corpus.
    assert_eq!(l["english_fallback"], 4);
    for f in l["files"].as_array().unwrap() {
        assert!(output_file(&h, f.as_str().unwrap()).exists());
    }
    assert!(
        !s["privacy"]["identifiers_detected"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(s["privacy"]["row_level_data"], false);
}
