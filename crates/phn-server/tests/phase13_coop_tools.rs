//! MCP integration tests for EPR-07 `continuity_of_operations_planner`:
//! output matches the schema, problems the nurse can fix are tool-level
//! errors, the plan and workbook land in the output folder, and staff
//! contacts from the roster appear only in those files (ADR 0009).

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

#[tokio::test]
async fn coop_plan_drafts_elements_and_keeps_contacts_in_files() {
    let h = start(|_| {}).await;
    let args = json!({
        "agency_name": "Wilmington Board of Health",
        "town": "Wilmington",
        "plan_date": "2026-11-09",
        "succession": [
            {"position": "Incident Commander",
             "successors": ["Operations Section Chief", "Planning Section Chief", "Health Agent"]},
            {"position": "Health Agent",
             "successors": ["Public Health Nurse", "Logistics Section Chief"]}
        ],
        "alternate_sites": [
            {"name": "Town library meeting room", "location": "Wilmington",
             "agreement": "MOU with the library trustees", "ready_within_hours": 8},
            {"name": "Staff telework", "kind": "telework"}
        ],
        "staff_count": 6,
        "devolution_partner": "Neighboring board of health (intermunicipal agreement)",
        "roster_path": "staff_roster.csv"
    });
    let r = h
        .call("continuity_of_operations_planner", args.clone())
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let all = everything(&r);
    let found = leaks(&all, "data/staff_roster.csv", None);
    assert!(found.is_empty(), "roster values leaked: {found:?}");

    let s = call_ok(&h, "continuity_of_operations_planner", args).await;
    let d = &s["data"];
    assert_eq!(d["gaps"].as_array().unwrap().len(), 10);
    let status = |e: &str| {
        d["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["element"] == e)
            .map(|g| g["status"].as_str().unwrap().to_string())
            .unwrap()
    };
    assert_eq!(status("Orders of succession"), "partial");
    assert_eq!(status("Continuity facilities"), "provided");
    assert_eq!(status("Human capital"), "provided");
    // 6 staff at 40% absence leaves 3 at the peak.
    assert_eq!(d["human_capital"]["staff_available_at_peak"], 3);
    assert_eq!(d["roster"]["staff"], 12);
    assert!(d["roster"]["matched"].as_u64().unwrap() >= 6);
    assert!(
        d["delegations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["legal_basis"].as_str().unwrap().contains("section 30"))
    );
    assert!(caveats(&s).contains("town counsel"));
    assert!(s["notice"].is_string(), "decision support notice");
    assert_eq!(s["privacy"]["row_level_data"], false);
    let files = s["privacy"]["files_written"].as_array().unwrap();
    assert_eq!(files.len(), 2);

    // The plan on disk has the roster's names and contacts.
    let plan = h.data().join(d["plan_file"].as_str().unwrap());
    let text = phn_local::docx::read_docx(&plan).unwrap().plain_text();
    assert!(
        text.contains("Peregrine Ashdown-Vell"),
        "incident commander name"
    );
    assert!(text.contains("781-555-0119"), "health agent work phone");
    assert!(text.contains("thessaly.brightwater@example.org"));
    assert!(text.contains("Annex A. Pandemic"));
    let wb = h.data().join(d["workbook_file"].as_str().unwrap());
    let sheets = phn_local::table::read_tables(&wb).unwrap();
    assert!(
        sheets
            .iter()
            .any(|t| t.headers.iter().any(|x| x == "Cell phone"))
    );
}

#[tokio::test]
async fn coop_input_problems_are_tool_errors() {
    let h = start(|_| {}).await;
    let e = call_err(
        &h,
        "continuity_of_operations_planner",
        json!({"exclude_functions": ["everything"]}),
    )
    .await;
    assert!(e.contains("not a seeded function"), "{e}");
    let e = call_err(
        &h,
        "continuity_of_operations_planner",
        json!({"functions": [{"label": "Rabies clinic"}]}),
    )
    .await;
    assert!(e.contains("needs a tier"), "{e}");
    let e = call_err(
        &h,
        "continuity_of_operations_planner",
        json!({"alternate_sites": [{"name": "Garage", "kind": "bunker"}]}),
    )
    .await;
    assert!(e.contains("alternate, telework, or devolution"), "{e}");
    let e = call_err(
        &h,
        "continuity_of_operations_planner",
        json!({"roster_path": "missing_roster.csv"}),
    )
    .await;
    assert!(!e.is_empty());
    // A file without a name or position column is refused without echoing its values.
    let e = call_err(
        &h,
        "continuity_of_operations_planner",
        json!({"roster_path": "counts_by_town.csv"}),
    )
    .await;
    assert!(e.contains("needs a name column"), "{e}");
}
