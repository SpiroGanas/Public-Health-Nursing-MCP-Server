//! MCP integration tests for the Phase 16 program design tools (PLAN-01
//! `build_logic_model`, PLAN-02 `write_smartie_objectives`, PLAN-05
//! `build_evaluation_plan`): offline calls succeed and match their output schemas, the
//! content-pending caveat is shown, problems the nurse can fix are tool errors, files land
//! in the output folder, Healthy People values fall back to the dated snapshot, and a staff
//! roster's names and contacts stay in the plan file.

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

fn docx_text(h: &common::Harness, display: &str) -> String {
    let p = h.data().join(display);
    assert!(p.exists(), "{display} was not written");
    phn_local::docx::read_docx(&p)
        .expect("readable DOCX")
        .plain_text()
}

const LOGIC: &str = "build_logic_model";
const SMARTIE: &str = "write_smartie_objectives";
const EVAL: &str = "build_evaluation_plan";

fn model() -> Value {
    json!({
        "program_name": "Postpartum home visiting program",
        "population": "first-time parents in Springfield",
        "stage": "implementation",
        "inputs": ["Two public health nurses"],
        "activities": ["Provide a nurse home visit within 14 days of discharge"],
        "outputs": ["Home visits completed"],
        "short_term_outcomes": ["Parents know the warning signs after birth"],
        "intermediate_outcomes": ["Increase the share of parents who attend the postpartum visit"],
        "long_term_outcomes": ["Reduce preventable postpartum emergency visits"],
        "assumptions": ["Parents accept a home visit when a hospital nurse recommends it"],
        "external_factors": ["Changes in state home visiting funding"]
    })
}

#[tokio::test]
async fn the_three_tools_are_listed_in_the_planning_toolset() {
    let h = start(|c| c.offline = true).await;
    let tools = h.client.list_all_tools().await.unwrap();
    for n in [LOGIC, SMARTIE, EVAL] {
        let t = tools
            .iter()
            .find(|t| t.name == n)
            .unwrap_or_else(|| panic!("{n}"));
        assert!(t.output_schema.is_some(), "{n}");
        // The tool descriptions are written for the model and say what the tool will not do.
        assert!(t.description.as_ref().unwrap().len() > 300, "{n}");
    }
}

#[tokio::test]
async fn logic_model_draws_mermaid_svg_and_writes_files() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, LOGIC, model()).await;
    let d = &s["data"];
    assert_eq!(d["status"], "draft_ready", "{}", d["findings"]);
    assert!(
        d["diagram"]["mermaid"]
            .as_str()
            .unwrap()
            .starts_with("flowchart LR")
    );
    let svg = d["diagram"]["svg"].as_str().unwrap();
    assert!(svg.contains("role=\"img\"") && svg.contains("<title id=\"diagram-title\">"));
    assert!(
        d["diagram"]["alt_text"]
            .as_str()
            .unwrap()
            .contains("6 columns")
    );
    assert!(
        d["diagram"]["text_description"]
            .as_str()
            .unwrap()
            .contains("An arrow leads to Activities.")
    );
    assert!(d["if_then"].as_array().unwrap().len() >= 5);
    assert!(pending(&s));
    // CDC's sources are cited with dates.
    for src in s["sources"].as_array().unwrap() {
        assert!(
            src["retrieved"].as_str().unwrap().starts_with("2026-"),
            "{src}"
        );
    }
    // The files are in the output folder; the table has the components.
    let f = &d["files"];
    for k in ["document", "svg", "png", "mermaid"] {
        let p = h.data().join(f[k].as_str().unwrap());
        assert!(p.is_file(), "{k}");
    }
    let t = docx_text(&h, f["document"].as_str().unwrap());
    assert!(t.contains("Postpartum home visiting program"));
    assert!(t.contains("Two public health nurses"));
    assert!(t.contains("Assumptions"));
    let files = s["privacy"]["files_written"].as_array().unwrap();
    assert_eq!(files.len(), 4);
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn logic_model_reports_gaps_and_refuses_nothing_to_draw() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        LOGIC,
        json!({"program_name": "Thin", "activities": ["Newsletter"],
               "short_term_outcomes": ["Provide a class"], "write_documents": false}),
    )
    .await;
    assert_eq!(s["data"]["status"], "needs_work");
    assert!(s["data"]["files"].is_null());
    assert!(
        s["privacy"]["files_written"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
    let ids: Vec<&str> = s["data"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"outcome_is_activity") && ids.contains(&"missing_inputs"),
        "{ids:?}"
    );
    let e = call_err(&h, LOGIC, json!({"program_name": "X"})).await;
    assert!(e.contains("at least the activities"), "{e}");
    let e = call_err(&h, LOGIC, json!({"activities": ["Provide x"]})).await;
    assert!(e.contains("program's name"), "{e}");
}

#[tokio::test]
async fn smartie_checks_parts_and_uses_the_healthy_people_snapshot_offline() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        SMARTIE,
        json!({
            "objectives": [{
                "measure": "proportion of adults with diabetes who had an eye exam in the past year",
                "direction": "increase", "population": "adults with diabetes at the health center",
                "baseline": 60.0, "baseline_period": "2025", "target": 75.0, "unit": "percent",
                "target_date": "2027-12-31", "data_source": "health center EHR report",
                "healthy_people_id": "D-04"
            }, {"measure": "screening rate"}],
            "healthy_people_queries": ["tobacco use adults"]
        }),
    )
    .await;
    let d = &s["data"];
    let first = &d["objectives"][0];
    assert_eq!(first["statement_complete"], true);
    let hp = &first["healthy_people"];
    assert_eq!(hp["objective"]["id"], "D-04");
    assert_eq!(hp["objective"]["mode"], "snapshot");
    assert_eq!(hp["objective"]["national_target_value"], 70.3);
    assert!(hp["comparison"].as_str().unwrap().contains("70.3 percent"));
    let second = &d["objectives"][1];
    let flags: Vec<&str> = second["flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    for f in [
        "missing_baseline",
        "missing_target",
        "missing_date",
        "missing_population",
        "missing_data_source",
        "missing_equity_dimension",
    ] {
        assert!(flags.contains(&f), "{f} in {flags:?}");
    }
    assert!(
        second["drafted_statement"]
            .as_str()
            .unwrap()
            .contains("[baseline]")
    );
    assert!(d["gaps"]["missing_baseline"].as_u64().unwrap() >= 1);
    let tobacco = &d["healthy_people_lookups"][0]["matches"];
    assert!(
        tobacco
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "TU-01"),
        "{tobacco}"
    );
    // The snapshot is cited with its date, and its use is stated.
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["mode"] == "snapshot"
                && x["title"].as_str().unwrap().contains("Healthy People 2030")
                && x["retrieved"].is_string())
    );
    assert!(caveats(&s).contains("bundled snapshot"));
    assert!(pending(&s));
    assert_eq!(d["elements"].as_array().unwrap().len(), 7);
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn smartie_errors_are_tool_errors_and_a_worksheet_can_be_written() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, SMARTIE, json!({})).await;
    assert!(e.contains("at least one objective"), "{e}");
    let e = call_err(
        &h,
        SMARTIE,
        json!({"healthy_people_queries": ["x".repeat(200)]}),
    )
    .await;
    assert!(e.contains("150 characters"), "{e}");
    let s = call_ok(
        &h,
        SMARTIE,
        json!({"goal": "Improve care", "write_document": true,
               "objectives": [{"measure": "visits", "direction": "increase"}]}),
    )
    .await;
    let f = s["data"]["file"].as_str().unwrap();
    let t = docx_text(&h, f);
    assert!(t.contains("Improve care") && t.contains("Specific"));
    // An unknown Healthy People ID is a note, not a crash.
    let s = call_ok(
        &h,
        SMARTIE,
        json!({"objectives": [{"healthy_people_id": "ZZ-99"}]}),
    )
    .await;
    assert!(caveats(&s).contains("ZZ-99"));
}

#[tokio::test]
async fn evaluation_plan_drafts_questions_and_writes_a_plan_with_blanks() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        EVAL,
        json!({
            "program_name": "Postpartum home visiting program",
            "population": "first-time parents in Springfield",
            "stage": "maintenance",
            "purpose": "Decide whether to expand to a second hospital.",
            "intended_users": ["Health director"], "intended_uses": ["Decide on expansion"],
            "logic_model": {
                "activities": ["Provide a nurse home visit within 14 days of discharge"],
                "outputs": ["Home visits completed"],
                "short_term_outcomes": ["Parents know the warning signs after birth"],
                "long_term_outcomes": ["Reduce preventable postpartum emergency visits"]
            },
            "disaggregate_by": ["language"], "lenses": ["re_aim"],
            "start_date": "2026-07-01", "end_date": "2027-06-30"
        }),
    )
    .await;
    let d = &s["data"];
    let types: Vec<&str> = d["evaluation_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["evaluation_type"].as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["process", "outcome", "impact"]);
    let matrix = d["matrix"].as_array().unwrap();
    assert!(
        matrix
            .iter()
            .any(|r| r["question_origin"] == "CDC example question")
    );
    assert!(
        matrix
            .iter()
            .all(|r| !r["data_sources"].as_array().unwrap().is_empty())
    );
    assert_eq!(d["timeline"].as_array().unwrap().len(), 6);
    assert_eq!(d["reaim"].as_array().unwrap().len(), 5);
    assert!(
        d["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["id"] == "suggestions_to_confirm")
    );
    assert!(pending(&s));
    assert!(caveats(&s).contains("Small numbers"));
    let t = docx_text(&h, d["file"].as_str().unwrap());
    for want in [
        "Step 1. Assess the context",
        "Step 6. Act on findings",
        "[analysis to be performed]",
        "RE-AIM lens",
        "Postpartum home visiting program",
    ] {
        assert!(t.contains(want), "{want}");
    }
    let e = call_err(&h, EVAL, json!({"program_name": "X"})).await;
    assert!(e.contains("logic model"), "{e}");
    let e = call_err(
        &h,
        EVAL,
        json!({"program_name": "X", "rows": [{"question": "To what extent did it work?"}],
               "start_date": "2027-01-01", "end_date": "2026-01-01"}),
    )
    .await;
    assert!(e.contains("after start_date"), "{e}");
}

#[tokio::test]
async fn evaluation_plan_roster_names_stay_in_the_file() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        EVAL,
        json!({"program_name": "Leak check", "roles": ["Incident Commander"],
               "roster_path": "staff_roster.csv",
               "logic_model": {"activities": ["Provide home visits"]}}),
    )
    .await;
    assert!(leaks(&s.to_string(), "data/staff_roster.csv", None).is_empty());
    let roster = &s["data"]["roster"];
    assert_eq!(roster["matched"], 1);
    // The plan file has the name beside the role, and no phone number or email.
    let t = docx_text(&h, s["data"]["file"].as_str().unwrap());
    assert!(
        t.contains("Incident Commander (Peregrine Ashdown-Vell)"),
        "{t}"
    );
    assert!(
        !t.contains("781-555"),
        "contact details must stay out of the plan"
    );
    assert!(!t.contains("example.org"));
    assert!(s["privacy"]["files_written"].as_array().unwrap().len() == 1);
    // A client line list is not a roster: nothing from it is echoed.
    let r = h
        .call(
            EVAL,
            json!({"program_name": "Leak check", "roster_path": "line_list.csv",
                   "logic_model": {"activities": ["Provide home visits"]}}),
        )
        .await;
    assert!(leaks(&everything(&r), "data/line_list.csv", None).is_empty());
}

#[tokio::test]
async fn contact_details_typed_into_a_description_are_flagged_not_repeated() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        LOGIC,
        json!({"program_name": "Flag check", "need": "Call the family at 617-555-0142",
               "activities": ["Provide visits"], "write_documents": false}),
    )
    .await;
    assert!(
        !s["privacy"]["identifiers_detected"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!caveats(&s).contains("617-555-0142"));
}
