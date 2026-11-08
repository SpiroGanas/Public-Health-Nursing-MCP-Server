//! MCP integration tests for the Phase 3 tools (communicable disease I):
//! successful calls validate against each tool's output schema, problems the
//! nurse can fix are tool-level errors, clinical answers carry the
//! decision-support notice, local line lists return aggregates only, and the
//! `investigate_outbreak` prompt lists and renders.

mod common;

use common::{everything, leaks, start, summary};
use rmcp::model::GetPromptRequestParams;
use serde_json::{Value, json};

const SUPPER: &str = "data/church_supper_outbreak.csv";

async fn call_ok(h: &common::Harness, name: &str, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h.call(name, args.clone()).await;
    assert_eq!(r.is_error, Some(false), "{name} {args}: {}", summary(&r));
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

fn has_notice(v: &Value) -> bool {
    v["notice"]
        .as_str()
        .is_some_and(|n| n.contains("Decision support"))
}

#[tokio::test]
async fn guide_search_quotes_passages_with_citations() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "search_ma_disease_control_guide",
        json!({"query": "food handler", "disease": "Salmonella", "kind": "guide_chapter"}),
    )
    .await;
    let p = &s["data"]["passages"][0];
    assert_eq!(p["document_id"], "guide-salmonellosis-non-typhoid");
    assert!(
        p["url"]
            .as_str()
            .unwrap()
            .starts_with("https://www.mass.gov/doc/")
    );
    assert!(p["citation"].as_str().unwrap().contains("Section"));
    assert!(has_notice(&s));
    let s = call_ok(
        &h,
        "search_ma_disease_control_guide",
        json!({"disease": "measles", "kind": "guide_chapter"}),
    )
    .await;
    assert!(
        s["data"]["outline"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o.as_str().unwrap().contains("Controlling Further Spread"))
    );
    let s = call_ok(
        &h,
        "search_ma_disease_control_guide",
        json!({"list_documents": true, "kind": "fact_sheet"}),
    )
    .await;
    assert!(s["data"]["documents"].as_array().unwrap().len() > 40);
    let e = call_err(&h, "search_ma_disease_control_guide", json!({})).await;
    assert!(e.contains("give a query"));
    let e = call_err(
        &h,
        "search_ma_disease_control_guide",
        json!({"query": "x", "disease": "zzzzqqq"}),
    )
    .await;
    assert!(e.contains("no guide chapter"));
}

#[tokio::test]
async fn isolation_and_exclusion_dates() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "calculate_isolation_and_exclusion",
        json!({"disease": "whooping cough", "settings": ["school"], "onset_date": "2026-01-01",
               "treatment_start_date": "2026-01-05"}),
    )
    .await;
    let r = &s["data"]["rules"][0];
    assert_eq!(r["earliest_return_date"], "2026-01-10");
    assert_eq!(r["verbatim"], true);
    assert!(r["citation"].as_str().unwrap().contains("105 CMR 300.200"));
    assert!(has_notice(&s));
    assert!(s["caveats"][0].as_str().unwrap().contains("PENDING-REVIEW"));
    // A susceptible measles contact in child care.
    let s = call_ok(
        &h,
        "calculate_isolation_and_exclusion",
        json!({"disease": "measles", "role": "contact", "settings": ["child_care"],
               "first_exposure_date": "2026-03-02", "susceptible": true,
               "exposure_continuous": false}),
    )
    .await;
    let r = &s["data"]["rules"][0];
    assert_eq!(r["start_date"], "2026-03-07");
    assert_eq!(r["last_excluded_date"], "2026-03-23");
    let e = call_err(
        &h,
        "calculate_isolation_and_exclusion",
        json!({"disease": "measles", "onset_date": "March 2"}),
    )
    .await;
    assert!(e.contains("YYYY-MM-DD"));
    let e = call_err(
        &h,
        "calculate_isolation_and_exclusion",
        json!({"disease": "not a disease at all"}),
    )
    .await;
    assert!(e.contains("Diseases covered"));
}

#[tokio::test]
async fn outbreak_line_list_finds_the_vehicle_without_leaking() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "analyze_outbreak_line_list",
        json!({"path": "church_supper_outbreak.csv", "disease": "salmonellosis",
               "exposure_datetime": "2026-05-02 18:00"}),
    )
    .await;
    assert_eq!(s["data"]["likely_vehicles"][0], "Stuffing");
    assert_eq!(s["data"]["cases"], 34);
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(
        s["data"]["exposure_window"]["known_exposure_in_window"],
        true
    );
    let r = h
        .call(
            "analyze_outbreak_line_list",
            json!({"path": "church_supper_outbreak.csv"}),
        )
        .await;
    assert!(leaks(&everything(&r), SUPPER, None).is_empty());
    // A case definition narrows the cases.
    let s = call_ok(
        &h,
        "analyze_outbreak_line_list",
        json!({"path": "church_supper_outbreak.csv", "case_values": ["Confirmed"]}),
    )
    .await;
    assert!(s["data"]["cases"].as_u64().unwrap() < 34);
    assert!(s["data"]["excluded"].as_u64().unwrap() > 0);
    // Identifier columns are refused.
    let e = call_err(
        &h,
        "analyze_outbreak_line_list",
        json!({"path": "church_supper_outbreak.csv", "town_column": "Last Name"}),
    )
    .await;
    assert!(e.contains("privacy") || e.contains("identif"), "{e}");
    assert!(leaks(&e, SUPPER, None).is_empty());
    let e = call_err(
        &h,
        "analyze_outbreak_line_list",
        json!({"path": "missing.csv"}),
    )
    .await;
    assert!(!e.is_empty());
}

#[tokio::test]
async fn epi_curve_is_saved_and_described() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "generate_epi_curve",
        json!({"path": "church_supper_outbreak.csv", "disease": "salmonellosis",
               "exposure_datetime": "2026-05-02 18:00", "stratify_by": "Case Status"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["interval_hours"], 6.0);
    assert_eq!(d["pattern"], "point_source");
    let png = d["chart"]["png"].as_str().unwrap();
    assert!(h.data().join(png).exists(), "{png}");
    assert!(
        s["privacy"]["files_written"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == png)
    );
    // Inline counts.
    let s = call_ok(
        &h,
        "generate_epi_curve",
        json!({"onset_counts": [{"onset": "2026-02-01", "count": 2}, {"onset": "2026-02-03", "count": 6},
                                {"onset": "2026-02-04", "count": 9}, {"onset": "2026-02-05", "count": 3}],
               "disease": "norovirus"}),
    )
    .await;
    assert_eq!(s["data"]["cases"], 20);
    let e = call_err(&h, "generate_epi_curve", json!({})).await;
    assert!(e.contains("onset_counts"));
}

#[tokio::test]
async fn contact_tracing_plan_writes_a_blank_template() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "plan_contact_tracing",
        json!({"disease": "measles", "onset_date": "2026-04-10"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["infectious_from"], "2026-04-06");
    assert_eq!(d["infectious_through"], "2026-04-14");
    let t = d["template"].as_str().unwrap();
    assert!(h.data().join(t).exists());
    assert!(has_notice(&s));
    assert!(
        d["per_contact_dates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["label"].as_str().unwrap().starts_with("PEP deadline"))
    );
    let e = call_err(
        &h,
        "plan_contact_tracing",
        json!({"disease": "measles", "template_rows": 0}),
    )
    .await;
    assert!(e.contains("template_rows"));
}

#[tokio::test]
async fn measles_exposure_assessment() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "assess_measles_exposure",
        json!({"first_exposure_date": "2026-03-02", "case_rash_onset_date": "2026-03-01",
               "setting": "health_care_worker", "birth_year": 1950, "as_of": "2026-03-03"}),
    )
    .await;
    let d = &s["data"];
    assert_ne!(d["immunity"]["status"], "evidence_of_immunity");
    assert_eq!(d["exposure_in_infectious_period"], true);
    assert_eq!(d["pep"][0]["open"], true);
    assert!(has_notice(&s));
    let e = call_err(&h, "assess_measles_exposure", json!({"setting": "school"})).await;
    assert!(e.contains("first_exposure_date"));
}

#[tokio::test]
async fn aberration_detection() {
    let h = start(|_| {}).await;
    let mut counts: Vec<f64> = (0..20).map(|i| 4.0 + (i % 3) as f64).collect();
    counts.push(19.0);
    let s = call_ok(
        &h,
        "detect_aberrations",
        json!({"counts": counts, "start_date": "2026-01-04", "series_name": "GI illness ED visits",
               "chart": true}),
    )
    .await;
    let d = &s["data"];
    assert!(
        d["latest_signals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "ears_c1")
    );
    let png = d["chart"]["png"].as_str().unwrap();
    assert!(h.data().join(png).exists());
    let e = call_err(&h, "detect_aberrations", json!({"counts": [1, 2, 3]})).await;
    assert!(e.contains("at least 8"));
}

#[tokio::test]
async fn infection_control_guidance() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "infection_control_for_congregate_settings",
        json!({"pathogen": "C. diff", "setting": "long_term_care"}),
    )
    .await;
    assert_eq!(s["data"]["epa_lists"][0]["list"], "K");
    assert!(has_notice(&s));
    let s = call_ok(
        &h,
        "infection_control_for_congregate_settings",
        json!({"pathogen": "head lice", "setting": "school"}),
    )
    .await;
    assert_eq!(s["data"]["settings"][0]["health_care_setting"], false);
    let e = call_err(
        &h,
        "infection_control_for_congregate_settings",
        json!({"pathogen": ""}),
    )
    .await;
    assert!(e.contains("pathogen"));
}

#[tokio::test]
async fn investigate_outbreak_prompt_lists_and_renders() {
    let h = start(|_| {}).await;
    let prompts = h.client.list_all_prompts().await.unwrap();
    let p = prompts
        .iter()
        .find(|p| p.name == "investigate_outbreak")
        .expect("prompt listed");
    assert!(
        p.arguments
            .as_ref()
            .unwrap()
            .iter()
            .any(|a| a.name == "disease")
    );
    let mut args = serde_json::Map::new();
    args.insert("disease".into(), json!("norovirus"));
    args.insert("setting".into(), json!("long-term care"));
    let r = h
        .client
        .get_prompt(GetPromptRequestParams::new("investigate_outbreak").with_arguments(args))
        .await
        .unwrap();
    let text = r.messages[0].content.as_text().unwrap().text.clone();
    assert!(text.contains("norovirus"));
    assert!(text.contains("analyze_outbreak_line_list"));
    assert!(text.contains("617-983-6800"));
    assert!(
        h.client
            .get_prompt(GetPromptRequestParams::new("no_such_prompt"))
            .await
            .is_err()
    );
}
