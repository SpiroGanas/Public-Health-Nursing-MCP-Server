//! MCP integration tests for the Phase 16 quality improvement tool (EBP-05): calls succeed and
//! match the output schema, scaffolds and files are produced, run chart rules find a planted shift,
//! small counts are hidden and nothing identifying is returned from a measure file, problems the
//! nurse can fix are tool errors, and the content-pending caveat is present.

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

fn file(h: &common::Harness, display: &str) -> std::path::PathBuf {
    h.data().join(display)
}

#[tokio::test]
async fn a_blank_request_returns_scaffolds_checklist_and_files() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "plan_quality_improvement", json!({})).await;
    let d = &s["data"];
    assert_eq!(d["questions"].as_array().unwrap().len(), 3);
    assert_eq!(d["questions"][1]["question"], "How will we know that a change is an improvement?");
    assert_eq!(d["driver_diagram"]["is_scaffold"], true);
    assert_eq!(d["fishbone"]["is_scaffold"], true);
    assert_eq!(d["process_map"]["is_scaffold"], true);
    assert!(d["driver_diagram"]["mermaid"].as_str().unwrap().starts_with("flowchart LR"));
    assert!(d["fishbone"]["mermaid"].as_str().unwrap().starts_with("mindmap"));
    assert_eq!(d["squire"]["items"].as_array().unwrap().len(), 18);
    assert!(d["squire"]["license_note"].as_str().unwrap().contains("copyrighted"));
    assert!(d["analysis"].is_null());
    let files: Vec<&str> = d["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
    assert!(files.iter().any(|f| f.ends_with("_plan.docx")));
    assert!(files.iter().any(|f| f.ends_with("_tracker.xlsx")));
    for f in &files {
        assert!(f.starts_with("output/"), "{f}");
        assert!(file(&h, f).exists(), "{f}");
    }
    // The SVGs are accessible: role, a title, and a description.
    let svg_path = files.iter().find(|f| f.ends_with("driver_diagram.svg")).unwrap();
    let svg = std::fs::read_to_string(file(&h, svg_path)).unwrap();
    assert!(svg.contains("role=\"img\"") && svg.contains("<title") && svg.contains("<desc"));
    assert!(pending(&s));
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert!(s["notice"].as_str().is_some());
    assert!(caveats(&s).contains("IHI"));
}

#[tokio::test]
async fn a_full_plan_builds_the_aim_diagrams_and_pdsa_checks() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "plan_quality_improvement",
        json!({
            "project_title": "Vaccine reminders",
            "aim": {"direction": "increase", "what": "the share of two-year-olds up to date",
                    "population": "children 24-35 months at the Elm Street clinic",
                    "baseline": 68, "goal": 80, "unit": "%", "by": "2027-06-30"},
            "measures": [
                {"name": "Percent up to date", "kind": "outcome", "numerator": "children with all doses",
                 "denominator": "children seen", "data_source": "registry report", "frequency": "monthly",
                 "owner": "Clinic nurse"},
                {"name": "Percent with a reminder sent", "kind": "process"},
                {"name": "Nurse minutes per visit", "kind": "balancing"}],
            "drivers": [{"primary": "Reliable reminders",
                         "secondary": [{"name": "Accurate contact list",
                                        "change_ideas": ["Verify phone numbers at every visit"]}]}],
            "fishbone": {"problem": "Many children are behind on vaccines",
                         "categories": [{"name": "People", "causes": ["Staff unsure who owns reminders"]}]},
            "process_steps": [
                {"name": "Child arrives", "kind": "start"},
                {"name": "Record reviewed?", "kind": "decision", "no_goes_to": "Child arrives", "pain_point": true},
                {"name": "Vaccines given", "kind": "end"}],
            "pdsa_cycles": [
                {"change": "Call 5 families", "prediction": "3 of 5 book", "scale": "one nurse, one week",
                 "objective": "learn if calls work", "who": "nurse", "when": "March",
                 "data_to_collect": "visits booked"}],
            "squire_status": [{"item": 6, "status": "done", "where_addressed": "Introduction"}]
        }),
    )
    .await;
    let d = &s["data"];
    assert_eq!(
        d["aim"]["statement"],
        "We will increase the share of two-year-olds up to date for children 24-35 months at the Elm Street clinic from 68% to 80% by June 30, 2027."
    );
    assert_eq!(d["driver_diagram"]["is_scaffold"], false);
    assert!(d["driver_diagram"]["mermaid"].as_str().unwrap().contains("Verify phone numbers"));
    assert!(d["fishbone"]["outline"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("Staff unsure")));
    let pm = d["process_map"]["mermaid"].as_str().unwrap();
    assert!(pm.contains("N2 -->|No| N1") && pm.contains("class N2 pain"));
    let checks = d["pdsa"]["checks"].as_array().unwrap();
    let status = |id: &str| checks.iter().find(|c| c["id"] == id).unwrap()["status"].as_str().unwrap().to_string();
    assert_eq!(status("prediction"), "met");
    assert_eq!(status("small_scale"), "met");
    assert_eq!(status("linked_cycles"), "judgment");
    assert_eq!(d["squire"]["done"], 1);
    assert_eq!(d["squire"]["items"][5]["status"], "done");
    assert!(d["measures"]["checks"].as_array().unwrap().iter().all(|c| c["status"] != "missing"));
    assert!(!d["measures"]["measures"][1]["missing"].as_array().unwrap().is_empty());
    // Files carry the project name.
    assert!(d["files"].as_array().unwrap().iter().all(|f| f.as_str().unwrap().contains("Vaccine_reminders")));
    // The DOCX passes the server's own accessibility check (headings in order, alt text, headers).
    let plan = d["files"].as_array().unwrap().iter().find(|f| f.as_str().unwrap().ends_with("_plan.docx")).unwrap().as_str().unwrap().to_string();
    let r = h.call("check_document_accessibility", json!({"path": plan})).await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let report = r.structured_content.unwrap();
    let failed: Vec<String> = report["data"]["report"]["checks"].as_array().unwrap().iter()
        .filter(|c| c["status"] == "fail")
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    assert!(failed.is_empty(), "accessibility failures: {failed:?}");
}

#[tokio::test]
async fn an_inline_run_chart_finds_a_planted_shift_with_both_rule_sets() {
    let h = start(|c| c.offline = true).await;
    let mut v = vec![4., 5., 4., 6., 5., 4., 5., 6., 4., 5., 4., 5.];
    v.extend([9., 10., 9., 11., 10., 9., 10., 11.]);
    let labels: Vec<String> = (1..=20).map(|i| format!("M{i}")).collect();
    let s = call_ok(
        &h,
        "plan_quality_improvement",
        json!({"sections": ["analysis"], "values": v, "labels": labels, "baseline_points": 12,
               "measure_name": "Reminder calls per week", "aim": {"direction": "increase"}}),
    )
    .await;
    let a = &s["data"]["analysis"];
    let findings = a["findings"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["rule_set"] == "ihi" && f["rule"] == "shift" && f["periods"] == "M13 to M20"));
    assert!(findings.iter().any(|f| f["rule_set"] == "anhoej" && f["rule"] == "unusually long run"));
    assert_eq!(a["phases"][0]["center"], 5.0);
    assert!(a["text_alternative"].as_str().unwrap().contains("Run chart: Reminder calls per week"));
    let chart = &a["chart"];
    let svg = std::fs::read_to_string(file(&h, chart["svg"].as_str().unwrap())).unwrap();
    assert!(svg.contains("<title") && svg.contains("<desc"));
    assert!(file(&h, chart["png"].as_str().unwrap()).exists());
    assert!(a["mermaid_chart"].as_str().unwrap().starts_with("xychart-beta"));
    // The IHI rules and the Anhøj rules are described, with their caveat about the Perla rules.
    assert!(caveats(&s).contains("Perla"));
    assert!(s["sources"].as_array().unwrap().iter().any(|x| x["mode"] == "computed"));
    // A control chart from the same series is a different chart type with limits.
    let s = call_ok(
        &h,
        "plan_quality_improvement",
        json!({"sections": ["analysis"], "values": [33, 41, 32, 30, 30, 35, 36, 37, 28, 39, 43, 39],
               "denominators": [50, 60, 47, 43, 46, 54, 54, 57, 43, 47, 53, 49], "chart_type": "p",
               "write_plan": false, "write_tracker": false}),
    )
    .await;
    let a = &s["data"]["analysis"];
    assert_eq!(a["chart_type"], "p");
    assert!(a["rows"][0]["ucl"].as_f64().unwrap() > a["rows"][0]["lcl"].as_f64().unwrap());
}

#[tokio::test]
async fn a_measure_file_returns_only_period_results_and_hides_small_counts() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "plan_quality_improvement",
        json!({"sections": ["analysis"], "path": "qi_clinic_measure.csv", "period_column": "Month",
               "numerator_column": "Children Up To Date", "denominator_column": "Children Seen",
               "baseline_points": 9, "phase_starts": ["2025-10"], "measure_name": "% up to date"}),
    )
    .await;
    let a = &s["data"]["analysis"];
    assert_eq!(a["periods"], 20);
    let april = a["rows"].as_array().unwrap().iter().find(|r| r["label"] == "2026-04").unwrap();
    assert_eq!(april["suppressed"], true);
    assert!(april["value"].is_null());
    assert_eq!(a["values_withheld"], true);
    assert!(a["rows"].as_array().unwrap().iter().all(|r| r.get("center").is_none()));
    assert!(a["phases"].as_array().unwrap().iter().all(|p| p.get("center").is_none()));
    assert!(a["mermaid_chart"].is_null());
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert_eq!(s["privacy"]["suppression_threshold"], 5);
    assert!(!s["privacy"]["identifiers_detected"].as_array().unwrap().is_empty());
    let all = everything(&h.call("plan_quality_improvement",
        json!({"sections": ["analysis"], "path": "qi_clinic_measure.csv", "period_column": "Month",
               "numerator_column": "Children Up To Date", "denominator_column": "Children Seen"})).await);
    assert!(leaks(&all, "data/qi_clinic_measure.csv", None).is_empty(), "{all}");
    // An event file groups rows by date: weekly counts of a line list, small counts hidden, and
    // nothing from the identifier columns returned.
    let r = h
        .call(
            "plan_quality_improvement",
            json!({"sections": ["analysis"], "path": "line_list.csv", "date_column": "Onset Date",
                   "period": "week", "measure_kind": "count", "write_plan": false,
                   "write_tracker": false}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    assert!(leaks(&everything(&r), "data/line_list.csv", None).is_empty());
    let s = r.structured_content.unwrap();
    assert!(s["data"]["analysis"]["periods"].as_u64().unwrap() >= 4);
    assert_eq!(s["privacy"]["row_level_data"], false);
    // Too few periods for a chart is a tool error, not a crash.
    let e = call_err(
        &h,
        "plan_quality_improvement",
        json!({"sections": ["analysis"], "path": "church_supper_outbreak.csv",
               "date_column": "Onset Date", "period": "week"}),
    )
    .await;
    assert!(e.contains("4 to 600 periods"));
}

#[tokio::test]
async fn identifier_columns_and_bad_input_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    for col in ["Contact Person", "Contact Cell", "Contact Email"] {
        let e = call_err(
            &h,
            "plan_quality_improvement",
            json!({"sections": ["analysis"], "path": "qi_clinic_measure.csv", "period_column": "Month",
                   "numerator_column": col}),
        )
        .await;
        assert!(leaks(&e, "data/qi_clinic_measure.csv", None).is_empty(), "{e}");
    }
    let e = call_err(&h, "plan_quality_improvement", json!({"sections": ["analysis"]})).await;
    assert!(e.contains("needs your measure data"));
    let e = call_err(
        &h,
        "plan_quality_improvement",
        json!({"process_steps": [{"name": "A", "kind": "decision", "no_goes_to": "Z"}]}),
    )
    .await;
    assert!(e.contains("not a step number"));
    let e = call_err(&h, "plan_quality_improvement", json!({"squire_status": [{"item": 0}]})).await;
    assert!(e.contains("between 1 and 18"));
    let e = call_err(&h, "plan_quality_improvement", json!({"aim": {"by": "1850-01-01"}})).await;
    assert!(e.contains("1900 and 2100"));
    let e = call_err(
        &h,
        "plan_quality_improvement",
        json!({"sections": ["analysis"], "values": [1, 2, 3, 4, 5, 6], "path": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("either inline values"));
    let e = call_err(
        &h,
        "plan_quality_improvement",
        json!({"sections": ["analysis"], "path": "../outside.csv", "period_column": "A", "numerator_column": "B"}),
    )
    .await;
    assert!(!e.is_empty());
}

#[tokio::test]
async fn contact_details_typed_into_the_request_are_flagged() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "plan_quality_improvement",
        json!({"sections": ["measures"],
               "measures": [{"name": "Percent up to date", "kind": "outcome", "owner": "Call Pat at 413-555-0199"}]}),
    )
    .await;
    assert!(!s["privacy"]["identifiers_detected"].as_array().unwrap().is_empty());
    assert!(s["privacy"]["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("roles")));
}

#[tokio::test]
async fn the_tool_is_in_the_ebp_toolset_and_is_not_open_world() {
    let h = start(|c| c.offline = true).await;
    let tools = h.client.list_all_tools().await.unwrap();
    let t = tools.iter().find(|t| t.name == "plan_quality_improvement").unwrap();
    let ann = t.annotations.as_ref().unwrap();
    assert_eq!(ann.open_world_hint, Some(false));
    assert_eq!(ann.read_only_hint, Some(false));
    assert!(phn_tools::registry::tool("plan_quality_improvement").is_some());
}
