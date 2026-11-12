//! MCP integration tests for the Phase 17 budget tools (PLAN-09 `build_program_budget`, PLAN-10
//! `economic_evaluation_calculator`): outputs match their schemas, the arithmetic is right,
//! problems the nurse can fix are tool errors, salaries stay out of the response, files are
//! refused when they hold identifiers, documents land in the output folder, and content is
//! marked pending review.

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

fn lines() -> Value {
    json!([
        {"category": "personnel", "item": "Public health nurse", "annual_salary": 80000,
         "effort_percent": 50},
        {"category": "equipment", "item": "Vaccine refrigerator", "unit_cost": 12000},
        {"category": "supplies", "item": "Clinic supplies", "unit_cost": 3000}
    ])
}

#[tokio::test]
async fn one_year_budget_has_the_right_totals_and_writes_both_files() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_program_budget",
        json!({"project_name": "Clinic nursing", "line_items": lines(), "fringe_rate_percent": 25}),
    )
    .await;
    let d = &s["data"];
    let fed = &d["federal_request"]["total"];
    assert_eq!(fed["personnel"], 40000.0);
    assert_eq!(fed["fringe_benefits"], 10000.0);
    assert_eq!(fed["total_direct"], 65000.0);
    // The equipment is outside the MTDC base: 40,000 + 10,000 + 3,000, at the 15% de minimis rate.
    assert_eq!(fed["indirect_base"], 53000.0);
    assert_eq!(fed["indirect"], 7950.0);
    assert_eq!(fed["total"], 72950.0);
    assert_eq!(d["settings"]["indirect_rate_percent"], 15.0);
    assert_eq!(
        d["settings"]["indirect_base"],
        "Modified total direct costs (MTDC)"
    );
    assert_eq!(d["arithmetic_check_passed"], true);
    let wb = h.data().join(d["workbook"].as_str().unwrap());
    let doc = h.data().join(d["justification"].as_str().unwrap());
    assert!(wb.exists() && doc.exists());
    assert!(d["justification_placeholders"].as_u64().unwrap() >= 5);
    // The workbook's summary sheet holds the same totals (cached results of its formulas; the
    // formulas themselves are checked by recalculating in LibreOffice, see
    // crates/phn-tools/tests/fixtures/generators/check_budget_workbook.py).
    let t = phn_local::table::read_table(&wb, Some("Budget summary")).unwrap();
    let total_row = t
        .rows
        .iter()
        .find(|r| r.first().map(String::as_str) == Some("Total (6k)"))
        .expect("a Total (6k) row");
    assert_eq!(total_row[1].parse::<f64>().unwrap(), 72950.0);
    let li = phn_local::table::read_table(&wb, Some("Line items")).unwrap();
    assert!(li.rows.len() >= 15, "spare rows are ready for new lines");
    // Sources are dated, the rate's revision is stated, and the content is pending review.
    assert!(pending(&s));
    assert!(caveats(&s).contains("15 percent"));
    assert!(caveats(&s).contains("10 percent before the October 1, 2024 revision"));
    assert!(
        s["sources"].as_array().unwrap().iter().any(|x| x["title"]
            .as_str()
            .unwrap()
            .contains("200.414")
            && x["effective_date"].as_str().is_some())
    );
    assert_eq!(s["privacy"]["row_level_data"], false);
    // One personnel position: the response warns that its total reflects that salary.
    assert!(caveats(&s).contains("one position"));
}

#[tokio::test]
async fn escalation_subaward_cap_cost_share_and_match_check() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_program_budget",
        json!({
            "line_items": [
                {"category": "personnel", "item": "Coordinator", "annual_salary": 60000, "effort_percent": 100},
                {"category": "contractual", "item": "Partner", "unit_cost": 30000, "subaward": true},
                {"category": "other", "item": "Gift cards", "unit_cost": 5000,
                 "exclude_from_mtdc": "participant_support"},
                {"category": "supplies", "item": "Donated supplies", "unit_cost": 4000, "funding": "cost_share"}
            ],
            "years": 3, "fringe_rate_percent": 20, "salary_escalation_percent": 3,
            "required_cost_share": {"percent": 10, "basis": "federal_request"},
            "write_workbook": false, "write_justification": false
        }),
    )
    .await;
    let d = &s["data"];
    let years = d["federal_request"]["years"].as_array().unwrap();
    assert_eq!(years.len(), 3);
    // Year 1 MTDC: salary 60,000 + fringe 12,000 + 30,000 of the subaward (first 50,000 over the
    // award counts: 30,000 then 20,000 then 0); gift cards are excluded.
    assert_eq!(years[0]["indirect_base"], 102000.0);
    assert_eq!(years[1]["indirect_base"], 61800.0 + 12360.0 + 20000.0);
    assert_eq!(years[2]["indirect_base"], 63654.0 + 12730.8);
    assert_eq!(d["cost_share"]["total"]["supplies"], 12000.0);
    // Cost share lines carry no indirect costs by default.
    assert_eq!(d["cost_share"]["total"]["indirect"], 0.0);
    let m = &d["cost_share_check"];
    assert_eq!(m["met"], false);
    assert_eq!(m["budgeted_cost_share"], 12000.0);
    assert!(d["workbook"].is_null() && d["justification"].is_null());
}

#[tokio::test]
async fn salaries_in_a_local_file_never_come_back() {
    let h = start(|_| {}).await;
    let r = h
        .call(
            "build_program_budget",
            json!({"line_items_path": "program_budget_lines.csv", "fringe_rate_percent": 24.5,
                   "years": 3, "salary_escalation_percent": 2,
                   "indirect": {"method": "negotiated", "rate_percent": 28.5, "base": "mtdc"}}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let all = everything(&r);
    let found = leaks(&all, "data/program_budget_lines.csv", None);
    assert!(found.is_empty(), "salaries returned: {found:?}");
    for salary in ["84,500", "52,750", "97,250", "84500", "52750", "97250"] {
        assert!(!all.contains(salary), "{salary} appears in the response");
    }
    let s = r.structured_content.unwrap();
    assert_eq!(s["data"]["line_counts"][0]["category"], "Personnel");
    assert_eq!(s["data"]["line_counts"][0]["federal_lines"], 2);
    assert_eq!(s["data"]["line_counts"][0]["cost_share_lines"], 1);
    assert!(s["privacy"]["files_written"].as_array().unwrap().len() == 2);
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["mode"] == "local"),
        "the file is cited as a local source"
    );
}

#[tokio::test]
async fn files_with_identifier_columns_are_refused_without_echoing_values() {
    let h = start(|_| {}).await;
    for file in ["staff_roster.csv", "line_list.csv", "food_pantries.csv"] {
        let e = call_err(
            &h,
            "build_program_budget",
            json!({"line_items_path": file, "fringe_rate_percent": 20}),
        )
        .await;
        assert!(e.contains("appears to contain"), "{file}: {e}");
        let found = leaks(&e, &format!("data/{file}"), None);
        assert!(found.is_empty(), "{file}: {found:?}");
    }
}

#[tokio::test]
async fn problems_the_nurse_can_fix_are_tool_errors() {
    let h = start(|_| {}).await;
    // De minimis above the published limit.
    let e = call_err(
        &h,
        "build_program_budget",
        json!({"line_items": lines(), "fringe_rate_percent": 25,
               "indirect": {"method": "de_minimis", "rate_percent": 20}}),
    )
    .await;
    assert!(e.contains("at most 15"), "{e}");
    // De minimis on another base.
    let e = call_err(
        &h,
        "build_program_budget",
        json!({"line_items": lines(), "fringe_rate_percent": 25,
               "indirect": {"method": "de_minimis", "base": "total_direct"}}),
    )
    .await;
    assert!(e.contains("modified total direct costs"), "{e}");
    // Personnel with no fringe rate.
    let e = call_err(&h, "build_program_budget", json!({"line_items": lines()})).await;
    assert!(e.contains("fringe"), "{e}");
    // Both inputs.
    let e = call_err(
        &h,
        "build_program_budget",
        json!({"line_items": lines(), "line_items_path": "program_budget_lines.csv", "fringe_rate_percent": 25}),
    )
    .await;
    assert!(e.contains("not both"), "{e}");
    // A line outside the budget years.
    let e = call_err(
        &h,
        "build_program_budget",
        json!({"line_items": [{"category": "supplies", "item": "X", "unit_cost": 5, "last_year": 3}], "years": 2}),
    )
    .await;
    assert!(e.contains("last_year") || e.contains("first_year"), "{e}");
    // Too many years, a negotiated rate with no rate, a personnel line with no salary.
    call_err(
        &h,
        "build_program_budget",
        json!({"line_items": lines(), "fringe_rate_percent": 25, "years": 6}),
    )
    .await;
    call_err(
        &h,
        "build_program_budget",
        json!({"line_items": lines(), "fringe_rate_percent": 25, "indirect": {"method": "negotiated"}}),
    )
    .await;
    call_err(
        &h,
        "build_program_budget",
        json!({"line_items": [{"category": "personnel", "item": "Nurse", "effort_percent": 50}], "fringe_rate_percent": 20}),
    )
    .await;
    // The justification needs lines.
    call_err(
        &h,
        "build_program_budget",
        json!({"write_justification": true}),
    )
    .await;
}

#[tokio::test]
async fn no_line_items_writes_a_blank_workbook() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_program_budget",
        json!({"project_name": "Blank", "years": 2}),
    )
    .await;
    let d = &s["data"];
    assert!(h.data().join(d["workbook"].as_str().unwrap()).exists());
    assert!(d["justification"].is_null());
    assert_eq!(d["federal_request"]["total"]["total"], 0.0);
    assert!(summary_text(&s).contains("blank workbook"));
}

fn summary_text(s: &Value) -> String {
    s["summary"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn contacts_typed_into_the_request_are_reported() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_program_budget",
        json!({"line_items": [{"category": "supplies", "item": "Call 413-555-0199 for quote", "unit_cost": 5}],
               "write_workbook": false, "write_justification": false}),
    )
    .await;
    assert!(
        !s["privacy"]["identifiers_detected"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------- PLAN-10

#[tokio::test]
async fn economic_evaluation_with_benefits_discounting_and_sensitivity() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "economic_evaluation_calculator",
        json!({"program_name": "Home visiting", "costs_by_year": [100000, 100000, 100000],
               "outcomes_by_year": [40, 40, 40], "benefit_per_outcome": 3500, "participants": 600,
               "sensitivity": [{"parameter": "benefit_per_outcome", "low": 2000, "high": 4200}]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["costs"]["total_undiscounted"], 300000.0);
    assert!((d["costs"]["present_value"].as_f64().unwrap() - 291346.97).abs() < 0.01);
    assert_eq!(d["costs"]["per_participant_undiscounted"], 500.0);
    assert_eq!(d["outcomes"]["cost_per_outcome_present_value"], 2500.0);
    assert!((d["benefits"]["benefit_cost_ratio"].as_f64().unwrap() - 1.4).abs() < 1e-9);
    assert!((d["benefits"]["return_on_investment"].as_f64().unwrap() - 0.4).abs() < 1e-9);
    assert!((d["break_even"]["value_per_outcome"].as_f64().unwrap() - 2500.0).abs() < 0.01);
    assert_eq!(d["discounting"]["rate_percent"], 3.0);
    assert!(
        d["discounting"]["rate_source"]
            .as_str()
            .unwrap()
            .contains("MMWR")
    );
    let sens = &d["sensitivity"];
    assert!(!sens["tornado"].as_array().unwrap().is_empty());
    assert!(
        sens["chart_text_alternative"]
            .as_str()
            .unwrap()
            .contains("Tornado chart")
    );
    let value_row = sens["tornado"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["parameter"] == "Value of one outcome")
        .unwrap();
    assert_eq!(value_row["default_range"], false);
    // At 2,000 per outcome the benefit-cost ratio is 0.8: the conclusion changes in this range.
    assert_eq!(value_row["changes_conclusion"], true);
    let chart = &sens["chart"];
    let svg = h.data().join(chart["svg"].as_str().unwrap());
    let svg_text = std::fs::read_to_string(&svg).unwrap();
    assert!(svg_text.contains("<title") && svg_text.contains("<desc"));
    assert!(h.data().join(chart["png"].as_str().unwrap()).exists());
    assert!(pending(&s));
    assert!(caveats(&s).contains("Discount rate 3% is the default"));
    assert!(caveats(&s).contains("not discounted"));
    let text = d["interpretation"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("does not check that the program caused"));
    assert!(text.contains("conclusion changes"));
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn icer_cases_and_dominance_are_handled() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": 120000, "outcomes": 30,
               "comparator": {"cost": 40000, "outcomes": 10}, "discount_rate_percent": 0,
               "write_chart": false}),
    )
    .await;
    let c = &s["data"]["comparison"];
    assert_eq!(c["classification"], "more_costly_more_effective");
    assert_eq!(c["icer"], 4000.0);
    assert!(
        caveats(&s).contains("threshold")
            || s["data"]["interpretation"]
                .to_string()
                .contains("threshold")
    );
    // A dominant program: no ratio is reported.
    let s = call_ok(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": 60000, "outcomes": 35,
               "comparator": {"cost": 90000, "outcomes": 20}, "run_sensitivity": false}),
    )
    .await;
    let c = &s["data"]["comparison"];
    assert_eq!(c["classification"], "dominant");
    assert!(c["icer"].is_null());
    assert!(
        s["data"]["interpretation"]
            .to_string()
            .contains("negative ratio")
    );
}

#[tokio::test]
async fn economic_evaluation_errors_are_tool_errors() {
    let h = start(|_| {}).await;
    call_err(&h, "economic_evaluation_calculator", json!({"outcomes": 5})).await;
    call_err(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": -5}),
    )
    .await;
    call_err(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": 5, "costs_by_year": [1.0]}),
    )
    .await;
    call_err(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": 5, "benefit_per_outcome": 3}),
    )
    .await;
    call_err(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": 5, "outcomes": 1, "discount_rate_percent": 120}),
    )
    .await;
    // An ICER sensitivity with no comparator.
    call_err(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": 5, "outcomes": 1, "sensitivity_metric": "icer"}),
    )
    .await;
}

#[tokio::test]
async fn cost_per_participant_only_still_works() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "economic_evaluation_calculator",
        json!({"program_cost": 25000, "participants": 500, "write_chart": false}),
    )
    .await;
    assert_eq!(s["data"]["costs"]["per_participant_present_value"], 50.0);
    assert!(s["data"]["outcomes"].is_null() && s["data"]["benefits"].is_null());
}
