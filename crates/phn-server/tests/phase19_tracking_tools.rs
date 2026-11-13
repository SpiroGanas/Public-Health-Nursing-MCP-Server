//! MCP integration tests for the Phase 19 tracking tools (POL-01 `track_ma_legislation`, POL-03
//! `monitor_federal_rules`): offline calls match their output schemas, the legislation tool uses
//! the dated snapshot and says so, the live-only federal tool says live data is unavailable instead
//! of listing anything, content is pending review, problems the nurse can fix are tool errors,
//! identifiers are refused without being echoed, and files land in the output folder.

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
async fn legislation_offline_uses_the_dated_snapshot_and_says_so() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "track_ma_legislation",
        json!({"topics": ["vaccines"], "max_bills": 5, "as_of": "2026-11-13"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["data_mode"], "snapshot");
    assert_eq!(d["general_court"], 194);
    let as_of = d["as_of"].as_str().unwrap();
    assert!(d["mode_note"].as_str().unwrap().contains(as_of));
    let bills = d["bills"].as_array().unwrap();
    assert!(!bills.is_empty() && bills.len() <= 5);
    for b in bills {
        assert_eq!(b["data_mode"], "snapshot");
        assert!(
            b["url"]
                .as_str()
                .unwrap()
                .starts_with("https://malegislature.gov/Bills/194/")
        );
        assert!(
            b["topics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t == "vaccines")
        );
        assert!(b["stage_label"].as_str().is_some());
        if let Some(e) = b["excerpt"].as_object() {
            assert!(
                e["text"]
                    .as_str()
                    .unwrap()
                    .replace('…', " ")
                    .split_whitespace()
                    .count()
                    <= 25
            );
        }
        assert!(b.get("primary_sponsor").is_none());
    }
    assert!(!d["analysis_questions"].as_array().unwrap().is_empty());
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    assert!(caveats(&s).contains("SNAPSHOT DATA") && caveats(&s).contains("TITLES"));
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["mode"] == "snapshot")
    );
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn legislation_topic_index_bill_numbers_and_filters() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "track_ma_legislation", json!({})).await;
    let topics = s["data"]["topics"].as_array().unwrap();
    assert!(topics.len() >= 15 && s["data"]["bills"].as_array().unwrap().is_empty());
    assert!(
        topics
            .iter()
            .any(|t| t["id"] == "mental_health" && t["bills_matched"].as_u64().unwrap() > 50)
    );
    // A bill from the snapshot by number, and one that is not in it.
    let first = call_ok(
        &h,
        "track_ma_legislation",
        json!({"topics": ["tobacco_nicotine"], "max_bills": 1}),
    )
    .await;
    let number = first["data"]["bills"][0]["bill_number"]
        .as_str()
        .unwrap()
        .to_string();
    let s = call_ok(
        &h,
        "track_ma_legislation",
        json!({"bill_numbers": [number.to_lowercase(), "H1"]}),
    )
    .await;
    assert_eq!(s["data"]["bills"][0]["bill_number"], number);
    assert!(
        s["data"]["not_found"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "H1")
            || s["data"]["bills"].as_array().unwrap().len() == 2
    );
    // Stage filter: every bill shown has the stage.
    let s = call_ok(
        &h,
        "track_ma_legislation",
        json!({"topics": ["mental_health"], "stages": ["study_or_new_draft"], "max_bills": 40}),
    )
    .await;
    for b in s["data"]["bills"].as_array().unwrap() {
        assert_eq!(b["stage"], "study_or_new_draft");
    }
}

#[tokio::test]
async fn legislation_errors_are_tool_errors_and_identifiers_are_not_echoed() {
    let h = start(|c| c.offline = true).await;
    assert!(
        call_err(&h, "track_ma_legislation", json!({"topics": ["astrology"]}))
            .await
            .contains("not a topic")
    );
    assert!(
        call_err(
            &h,
            "track_ma_legislation",
            json!({"bill_numbers": ["HD2050"]})
        )
        .await
        .contains("bill number")
    );
    assert!(
        call_err(&h, "track_ma_legislation", json!({"stages": ["done"]}))
            .await
            .contains("not a stage")
    );
    let e = call_err(
        &h,
        "track_ma_legislation",
        json!({"keywords": ["the client at 837 Elm Street 413-555-0100"]}),
    )
    .await;
    assert!(!e.contains("413-555-0100") && !e.contains("837 Elm"));
    assert!(
        call_err(
            &h,
            "track_ma_legislation",
            json!({"topics": ["vaccines"], "as_of": "0021-01-01"})
        )
        .await
        .contains("1900")
    );
    assert!(
        call_err(
            &h,
            "track_ma_legislation",
            json!({"topics": ["vaccines"], "watchlist_path": "no_such.csv"})
        )
        .await
        .len()
            > 5
    );
}

#[tokio::test]
async fn watchlist_saves_to_the_output_folder_and_reads_back() {
    let h = start(|c| c.offline = true).await;
    std::fs::write(
        h.data().join("my_watchlist.csv"),
        "kind,value,note\ntopic,school_child_care,\nkeyword,heat illness,athletes\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "track_ma_legislation",
        json!({"watchlist_path": "my_watchlist.csv", "save_watchlist": true, "max_bills": 3}),
    )
    .await;
    let w = &s["data"]["watchlist"];
    assert_eq!(w["read_from"], "my_watchlist.csv");
    assert_eq!(w["topics"][0], "school_child_care");
    let file = w["saved_file"].as_str().unwrap();
    assert!(file.contains("output/") && file.ends_with(".csv"), "{file}");
    assert_eq!(s["privacy"]["files_written"][0], file);
    let text = std::fs::read_to_string(h.data().join(file)).unwrap();
    assert!(
        text.starts_with("kind,value,note,title,stage") && text.contains("topic,school_child_care")
    );
    // It reads back as a watchlist.
    let again = call_ok(
        &h,
        "track_ma_legislation",
        json!({"watchlist_path": file, "max_bills": 3}),
    )
    .await;
    assert!(
        again["data"]["watchlist"]["topics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "school_child_care")
    );
    assert!(
        !again["data"]["watchlist"]["bills"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn federal_offline_says_live_data_is_unavailable_and_lists_nothing() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "monitor_federal_rules", json!({"as_of": "2026-11-13"})).await;
    let d = &s["data"];
    assert_eq!(d["data_status"], "unavailable");
    assert!(d["documents"].as_array().unwrap().is_empty());
    assert!(d["as_of"].is_null());
    assert!(
        d["status_note"]
            .as_str()
            .unwrap()
            .contains("LIVE DATA UNAVAILABLE")
    );
    assert!(
        summary_has(
            &h,
            "monitor_federal_rules",
            json!({"as_of": "2026-11-13"}),
            "LIVE DATA UNAVAILABLE"
        )
        .await
    );
    assert!(caveats(&s).contains("no open comment periods"));
    assert_eq!(d["query"]["agencies"].as_array().unwrap().len(), 6);
    assert!(d["regulations_gov"]["note"].as_str().is_some());
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    assert!(!d["manual_links"].as_array().unwrap().is_empty());
}

async fn summary_has(h: &common::Harness, name: &str, args: Value, needle: &str) -> bool {
    let r = h.call(name, args).await;
    summary(&r).contains(needle)
}

#[tokio::test]
async fn federal_outline_from_a_description_is_an_accessible_word_file() {
    let h = start(|c| c.offline = true).await;
    std::fs::write(
        h.data().join("fill.csv"),
        "field,value\norganization,Example Town Health Department\nposition,support with changes\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "monitor_federal_rules",
        json!({"draft_comment_outline": true, "document_title": "Proposed rule on lead in drinking water",
               "comment_deadline": "2026-12-14", "docket_id": "EPA-HQ-OW-2026-0001",
               "agency_name": "Environmental Protection Agency", "fill_path": "fill.csv", "as_of": "2026-11-13"}),
    )
    .await;
    let o = &s["data"]["outline"];
    assert_eq!(o["countdown"]["days_remaining"], 31);
    assert_eq!(o["fields_filled"], 2);
    assert_eq!(o["sections"].as_array().unwrap().len(), 9);
    let file = o["file"].as_str().unwrap().to_string();
    assert!(file.ends_with(".docx") && h.data().join(&file).exists());
    assert_eq!(s["privacy"]["files_written"][0], file.as_str());
    let all = everything(
        &h.call(
            "monitor_federal_rules",
            json!({"draft_comment_outline": true, "document_title": "T", "fill_path": "fill.csv"}),
        )
        .await,
    );
    assert!(
        !all.contains("Example Town Health Department") && !all.contains("support with changes")
    );
    let r = h
        .call("check_document_accessibility", json!({"path": file}))
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let report = r.structured_content.unwrap();
    let failed: Vec<String> = report["data"]["report"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["status"] == "fail")
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    assert!(failed.is_empty(), "accessibility failures {failed:?}");
}

#[tokio::test]
async fn federal_errors_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    assert!(
        call_err(&h, "monitor_federal_rules", json!({"agencies": ["navy"]}))
            .await
            .contains("not an agency id")
    );
    assert!(
        call_err(
            &h,
            "monitor_federal_rules",
            json!({"document_types": ["memo"]})
        )
        .await
        .contains("document type")
    );
    assert!(
        call_err(
            &h,
            "monitor_federal_rules",
            json!({"draft_comment_outline": true})
        )
        .await
        .contains("document_number")
    );
    assert!(
        call_err(
            &h,
            "monitor_federal_rules",
            json!({"published_since": "2026-13-01"})
        )
        .await
        .contains("not a date")
    );
    let e = call_err(
        &h,
        "monitor_federal_rules",
        json!({"topic": "family at 837 Elm Street"}),
    )
    .await;
    assert!(!e.contains("837 Elm"));
    std::fs::write(
        h.data().join("clients.csv"),
        "First Name,Last Name,field,value\nAnselma,Quiddington,organization,x\n",
    )
    .unwrap();
    let e = call_err(
        &h,
        "monitor_federal_rules",
        json!({"draft_comment_outline": true, "document_title": "T", "fill_path": "clients.csv"}),
    )
    .await;
    assert!(e.contains("list of people") && !e.contains("Quiddington"));
}

#[tokio::test]
async fn saved_watchlists_never_hold_cells_a_spreadsheet_would_run_as_formulas() {
    let h = start(|c| c.offline = true).await;
    std::fs::write(
        h.data().join("risky_watchlist.csv"),
        "kind,value,note\nkeyword,=HYPERLINK(\"http://example.org\"),\nkeyword,+cmd,\ntopic,school_child_care,\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "track_ma_legislation",
        json!({"watchlist_path": "risky_watchlist.csv", "save_watchlist": true, "max_bills": 3}),
    )
    .await;
    let file = s["data"]["watchlist"]["saved_file"].as_str().unwrap();
    let text = std::fs::read_to_string(h.data().join(file)).unwrap();
    for line in text.lines().skip(1) {
        let first_cells: Vec<&str> = line.split(',').collect();
        for cell in first_cells {
            let cell = cell.trim_matches('"');
            assert!(
                !cell.starts_with(['=', '+', '-', '@']),
                "formula-like cell in saved watchlist: {cell}"
            );
        }
    }
    assert!(text.contains("'=HYPERLINK"), "{text}");
    // The safe copy still reads back as the same keyword.
    let again = call_ok(
        &h,
        "track_ma_legislation",
        json!({"watchlist_path": file, "max_bills": 3}),
    )
    .await;
    let kws: Vec<String> = again["data"]["watchlist"]["keywords"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    assert!(kws.iter().any(|k| k.starts_with("=HYPERLINK")), "{kws:?}");
}
