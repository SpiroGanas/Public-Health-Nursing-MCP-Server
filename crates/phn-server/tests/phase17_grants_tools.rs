//! MCP integration tests for the Phase 17 grant tools (PLAN-06, PLAN-07, PLAN-08): offline calls
//! succeed and match their output schemas, the federal search says it is in snapshot mode, the
//! announcement reader finds what a synthetic notice and request for response contain and says what
//! it could not find, contact people are never returned, the narrative draft shows only the facts
//! and statistics it was given, and problems the nurse can fix are tool errors.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

async fn offline() -> common::Harness {
    start(|c| c.offline = true).await
}

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

const CONTACT_VALUES: [&str; 8] = [
    "Veronika Haldane",
    "Osric Penhallow",
    "Veronika",
    "Haldane",
    "Penhallow",
    "413-555-0171",
    "413-555-0172",
    "@example.org",
];

fn no_contacts(text: &str) {
    for v in CONTACT_VALUES {
        assert!(!text.contains(v), "{v} was returned");
    }
}

// ---------------------------------------------------------------- PLAN-06

#[tokio::test]
async fn the_search_works_offline_and_says_it_is_in_snapshot_mode() {
    let h = offline().await;
    let s = call_ok(&h, "search_grant_opportunities", json!({})).await;
    assert!(pending(&s));
    let d = &s["data"];
    assert_eq!(d["federal"]["mode"], "snapshot");
    assert_eq!(d["federal"]["matched"], 0);
    let reason = d["federal"]["reason"].as_str().unwrap();
    assert!(reason.contains("PHN_GRANTS_GOV_API_KEY") || reason.contains("offline"));
    assert_eq!(d["commbuys"]["mode"], "snapshot");
    assert_eq!(d["commbuys"]["as_of"], "2026-11-09");
    assert!(d["commbuys"]["read"].as_u64().unwrap() > 100);
    // The sources carry the list's date and mode.
    let src = s["sources"].as_array().unwrap();
    assert!(
        src.iter()
            .any(|x| x["title"].as_str().unwrap().contains("COMMBUYS")
                && x["mode"] == "snapshot"
                && x["effective_date"] == "2026-11-09")
    );
    assert!(
        summary(&h.call("search_grant_opportunities", json!({})).await).contains("mode snapshot")
    );
    // The places to look and the Massachusetts facts are there, with their confidence.
    assert!(d["where_to_look"].as_array().unwrap().len() >= 5);
    assert!(
        d["massachusetts_notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["text"]
                .as_str()
                .unwrap()
                .contains("unsolicited proposals"))
    );
    assert!(
        d["reach_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.as_str().unwrap().contains("grants.gov"))
    );
    // Every listed opportunity has a closing date in the window and says how to find it.
    for o in d["opportunities"].as_array().unwrap() {
        assert!(o["how_to_find"].as_str().unwrap().contains("COMMBUYS"));
        assert_eq!(o["eligibility_stated"], false);
    }
    // The privacy block says nothing identifying is sent.
    assert!(
        s["privacy"]["notes"][0]
            .as_str()
            .unwrap()
            .contains("topic words")
    );
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn the_search_filters_and_reports_what_it_could_not_apply() {
    let h = offline().await;
    let s = call_ok(
        &h,
        "search_grant_opportunities",
        json!({"source": "commbuys", "closes_after": "2000-01-01", "closes_before": "2100-01-01",
               "topic": "mosquito control", "min_award": 50000, "eligibility": "local_health_agency"}),
    )
    .await;
    let d = &s["data"];
    assert!(d["federal"].is_null());
    let opps = d["opportunities"].as_array().unwrap();
    assert_eq!(opps.len(), 1, "{opps:?}");
    assert!(opps[0]["title"].as_str().unwrap().contains("Mosquito"));
    let not_applied = d["filters_not_applied"].as_array().unwrap();
    assert!(
        not_applied
            .iter()
            .any(|f| f.as_str().unwrap().contains("Award amount"))
    );
    assert!(
        not_applied
            .iter()
            .any(|f| f.as_str().unwrap().contains("Eligibility"))
    );
    // A grants-only search lists only grant-like titles.
    let s = call_ok(
        &h,
        "search_grant_opportunities",
        json!({"source": "commbuys", "closes_after": "2000-01-01", "grants_only": true}),
    )
    .await;
    let opps = s["data"]["opportunities"].as_array().unwrap();
    assert!(!opps.is_empty());
    assert!(opps.iter().all(|o| o["kind"] == "grant_or_funding"));
    // The limit holds.
    let s = call_ok(
        &h,
        "search_grant_opportunities",
        json!({"source": "commbuys", "closes_after": "2000-01-01", "limit": 3}),
    )
    .await;
    assert!(s["data"]["opportunities"].as_array().unwrap().len() <= 3);
}

#[tokio::test]
async fn the_search_refuses_bad_input_without_echoing_it() {
    let h = offline().await;
    let e = call_err(
        &h,
        "search_grant_opportunities",
        json!({"closes_after": "soon"}),
    )
    .await;
    assert!(e.contains("closes_after"));
    let e = call_err(
        &h,
        "search_grant_opportunities",
        json!({"closes_after": "2027-03-01", "closes_before": "2027-01-01"}),
    )
    .await;
    assert!(e.contains("ends before"));
    let e = call_err(
        &h,
        "search_grant_opportunities",
        json!({"topic": "asthma for patient 123-45-6789"}),
    )
    .await;
    assert!(e.contains("identifier") && !e.contains("123-45-6789"));
    let e = call_err(
        &h,
        "search_grant_opportunities",
        json!({"topic": "x".repeat(150)}),
    )
    .await;
    assert!(e.contains("100"));
}

// ---------------------------------------------------------------- PLAN-07

#[tokio::test]
async fn the_synthetic_nofo_is_read_into_a_checklist() {
    let h = offline().await;
    let s = call_ok(
        &h,
        "analyze_funding_announcement",
        json!({"path": "synthetic_nofo.docx"}),
    )
    .await;
    assert!(pending(&s));
    let d = &s["data"];
    assert_eq!(d["kind"], "federal_nofo");
    assert!(
        d["identifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i == "SYN-HPP-2027-001")
    );
    assert_eq!(d["primary_deadline"]["date"], "2027-02-26");
    assert_eq!(d["primary_deadline"]["kind"], "submission");
    let kinds: Vec<(&str, &str)> = d["deadlines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                x["kind"].as_str().unwrap(),
                x["date"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert!(kinds.contains(&("intent", "2026-12-18")), "{kinds:?}");
    assert!(kinds.contains(&("conference", "2027-01-07")), "{kinds:?}");
    assert!(kinds.contains(&("award", "2027-05-14")), "{kinds:?}");
    // Page limits: 15 pages for the narrative, 5 for the budget narrative, 300 words for the abstract.
    let limits: Vec<(u64, &str)> = d["page_limits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["limit"].as_u64().unwrap(), l["unit"].as_str().unwrap()))
        .collect();
    assert!(limits.contains(&(15, "pages")), "{limits:?}");
    assert!(limits.contains(&(5, "pages")), "{limits:?}");
    assert!(limits.contains(&(300, "words")), "{limits:?}");
    assert!(!d["format_rules"].as_array().unwrap().is_empty());
    // Attachments and forms.
    let atts: Vec<&str> = d["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["label"].as_str().unwrap())
        .collect();
    assert!(
        atts.iter()
            .any(|a| a.starts_with("Attachment 1: Work Plan")),
        "{atts:?}"
    );
    assert!(
        atts.iter().any(|a| a.starts_with("Attachment 3")),
        "{atts:?}"
    );
    assert!(
        d["forms_named"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "SF-424")
    );
    // Scoring: five criteria worth 100 points, mapped to narrative sections.
    let crit = d["criteria"].as_array().unwrap();
    assert_eq!(crit.len(), 5, "{crit:?}");
    assert_eq!(d["points_total"], 100.0);
    assert_eq!(crit[0]["label"], "Statement of Need");
    assert_eq!(crit[0]["points"], 20.0);
    assert_eq!(crit[0]["narrative_section"], "needs_statement");
    assert_eq!(crit[4]["narrative_section"], "sustainability");
    // Eligibility names the right applicant types and excludes others.
    let cats = d["applicant_categories"].as_array().unwrap();
    assert!(
        cats.iter()
            .any(|c| c.as_str().unwrap().contains("Municipality"))
    );
    assert!(
        d["eligibility"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["excludes"] == true)
    );
    // Amounts, term, method.
    assert!(
        d["amounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["amount_usd"] == 300000.0)
    );
    assert!(
        d["terms"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t.as_str().unwrap().contains("36 months"))
    );
    assert!(
        d["submission_methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m.as_str().unwrap().contains("Grants.gov"))
    );
    // The outline follows the criteria in order and ends with the attachments.
    let outline = d["outline"].as_array().unwrap();
    assert_eq!(outline[0]["heading"], "Statement of Need");
    assert_eq!(outline[1]["points"], 30.0);
    assert_eq!(
        outline.last().unwrap()["heading"],
        "Required forms and attachments"
    );
    // Counts agree with the lists.
    assert_eq!(d["counts"]["criteria"], 5);
    assert_eq!(d["counts"]["criteria_with_points"], 5);
    // Contact people are never returned; none of the data is a long passage.
    no_contacts(&everything(
        &h.call(
            "analyze_funding_announcement",
            json!({"path": "synthetic_nofo.docx"}),
        )
        .await,
    ));
    assert!(
        leaks(
            &everything(
                &h.call(
                    "analyze_funding_announcement",
                    json!({"path": "synthetic_nofo.docx"})
                )
                .await
            ),
            "data/synthetic_nofo.docx",
            None
        )
        .is_empty()
    );
    for c in crit {
        assert!(c["label"].as_str().unwrap().chars().count() <= 100);
    }
    assert!(s["privacy"]["notes"][0].as_str().unwrap().contains("not"));
    // The checklist files exist and contain the criteria.
    let files = d["checklist_files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    for f in files {
        assert!(h.data().join(f.as_str().unwrap()).exists());
    }
    let docx = std::fs::read(h.data().join(files[0].as_str().unwrap())).unwrap();
    let doc = phn_local::docx::parse_docx(&docx).unwrap();
    let text = doc.plain_text();
    assert!(text.contains("Statement of Need") && text.contains("Attachment 1"));
    no_contacts(&text);
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn the_synthetic_rfr_pdf_is_read_and_its_limits_are_stated() {
    let h = offline().await;
    let s = call_ok(
        &h,
        "analyze_funding_announcement",
        json!({"path": "synthetic_rfr.pdf", "write_checklist": false}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["kind"], "ma_rfr");
    assert!(
        d["identifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i.as_str().unwrap().starts_with("BD-27-9999"))
    );
    // Criteria are numbered components with rating words, not points.
    let crit = d["criteria"].as_array().unwrap();
    assert_eq!(crit.len(), 5, "{crit:?}");
    assert!(crit.iter().all(|c| c["points"].is_null()));
    assert!(
        crit.iter()
            .any(|c| c["label"] == "Experience and Qualifications Narrative")
    );
    assert!(
        d["rating_scale"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "Highly Advantageous")
    );
    assert!(
        d["page_limits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["limit"] == 8 && l["unit"] == "pages")
    );
    assert_eq!(d["outline"][0]["basis"], "submission component");
    // The dates in the procurement calendar.
    let dates: Vec<String> = d["deadlines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["date"].as_str().map(str::to_string))
        .collect();
    assert!(dates.contains(&"2027-01-15".to_string()), "{dates:?}");
    assert!(dates.contains(&"2026-12-04".to_string()), "{dates:?}");
    assert!(
        d["submission_methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m.as_str().unwrap().contains("COMMBUYS"))
    );
    // What was not found is said.
    let nf = d["not_found"].as_array().unwrap();
    assert!(
        nf.iter()
            .any(|n| n.as_str().unwrap().contains("no point values"))
    );
    assert!(d["checklist_files"].as_array().unwrap().is_empty());
    no_contacts(&everything(
        &h.call(
            "analyze_funding_announcement",
            json!({"path": "synthetic_rfr.pdf"}),
        )
        .await,
    ));
    assert!(d["items_withheld"].as_u64().unwrap() <= 5);
}

#[tokio::test]
async fn the_announcement_reader_refuses_what_it_cannot_read() {
    let h = offline().await;
    let e = call_err(&h, "analyze_funding_announcement", json!({"path": ""})).await;
    assert!(e.contains("path is required"));
    let e = call_err(
        &h,
        "analyze_funding_announcement",
        json!({"path": "no_such_file.pdf"}),
    )
    .await;
    assert!(e.to_lowercase().contains("not found") || e.to_lowercase().contains("no such"));
    let e = call_err(
        &h,
        "analyze_funding_announcement",
        json!({"path": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("not a document"));
    assert!(leaks(&e, "data/line_list.csv", None).is_empty());
    let e = call_err(
        &h,
        "analyze_funding_announcement",
        json!({"path": "../../etc/passwd"}),
    )
    .await;
    assert!(e.to_lowercase().contains("path") || e.to_lowercase().contains("not allowed"));
    // A text file with almost nothing in it.
    std::fs::write(h.data().join("tiny.txt"), "Short note.").unwrap();
    let e = call_err(
        &h,
        "analyze_funding_announcement",
        json!({"path": "tiny.txt"}),
    )
    .await;
    assert!(e.contains("almost no readable text"));
    // A client record saved as the announcement is refused.
    let mut rec = String::new();
    for i in 0..4 {
        rec.push_str(&format!(
            "Client record: SSN 900-00-123{i}, born 1/2/195{i}, MRN: 00123{i}\n"
        ));
    }
    rec.push_str(&"More record text follows here. ".repeat(20));
    std::fs::write(h.data().join("record.txt"), rec).unwrap();
    let e = call_err(
        &h,
        "analyze_funding_announcement",
        json!({"path": "record.txt"}),
    )
    .await;
    assert!(e.contains("client record") && !e.contains("900-00"));
}

#[tokio::test]
async fn an_announcement_with_nothing_findable_lists_every_gap() {
    let h = offline().await;
    std::fs::write(
        h.data().join("letter.txt"),
        "Dear colleagues, we are writing to tell you about a community program that serves families in our region. ".repeat(6),
    )
    .unwrap();
    let s = call_ok(
        &h,
        "analyze_funding_announcement",
        json!({"path": "letter.txt", "write_checklist": false}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["kind"], "unclear");
    assert!(d["primary_deadline"].is_null());
    assert_eq!(d["counts"]["criteria"], 0);
    let nf: Vec<&str> = d["not_found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    for key in [
        "submission deadline",
        "eligibility",
        "limit",
        "attachments",
        "scoring criteria",
    ] {
        assert!(
            nf.iter().any(|n| n.contains(key)),
            "{key} missing from {nf:?}"
        );
    }
    assert!(
        d["outline"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["basis"].as_str().unwrap().contains("default outline"))
    );
    assert!(
        summary(
            &h.call(
                "analyze_funding_announcement",
                json!({"path": "letter.txt", "write_checklist": false})
            )
            .await
        )
        .contains("Could not find")
    );
}

// ---------------------------------------------------------------- PLAN-08

#[tokio::test]
async fn the_narrative_follows_the_checklist_and_adds_no_numbers_of_its_own() {
    let h = offline().await;
    let a = call_ok(
        &h,
        "analyze_funding_announcement",
        json!({"path": "synthetic_nofo.docx"}),
    )
    .await;
    let xlsx = a["data"]["checklist_files"][1]
        .as_str()
        .unwrap()
        .to_string();
    let args = json!({
        "project_title": "Asthma Home Visits",
        "funder": "Synthetic Office of Prevention Programs",
        "community": "Test Town",
        "applicant": "Test Town Health Department",
        "criteria_file": xlsx,
        "data_points": [
            {"label": "Children with asthma", "value": "11.4%", "geography": "Test Town",
             "source": "School nurse survey", "as_of": "2025", "comparison": "state 9.1%"},
            {"label": "Families below the poverty line", "value": "18%"}
        ],
        "facts": {"health_problem": "Asthma sends children to the emergency department.",
                  "objectives": ["Visit 40 homes by June 2027"],
                  "partners": ["Test Town Public Schools: referrals"]},
        "page_limit": 15
    });
    let s = call_ok(&h, "draft_grant_narrative", args.clone()).await;
    assert!(pending(&s));
    let d = &s["data"];
    assert_eq!(d["criteria_from"], "checklist_file");
    assert_eq!(d["criteria"].as_array().unwrap().len(), 5);
    let ids: Vec<&str> = d["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["needs_statement", "project_narrative", "sustainability"]
    );
    let needs = &d["sections"][0];
    assert!(
        needs["criteria"][0]
            .as_str()
            .unwrap()
            .starts_with("Statement of Need (20 points)")
    );
    let text = needs["text"].as_str().unwrap();
    assert!(text.contains("Asthma sends children to the emergency department."));
    assert!(text.contains("**Children with asthma:** 11.4% (Test Town; School nurse survey, 2025; comparison: state 9.1%)"));
    assert!(text.contains("[NURSE: add the source and year]"));
    assert_eq!(d["data_points_missing_citation"], 1);
    assert!(d["placeholder_count"].as_u64().unwrap() > 8);
    assert!(d["page_limit_note"].as_str().unwrap().contains("15 pages"));
    // The draft contains no digit string that was not in the input or in the checklist file the
    // criteria came from (the tool invents no numbers).
    let checklist_points: Vec<String> = a["data"]["criteria"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["points"].to_string())
        .collect();
    let input = format!("{} {}", args, checklist_points.join(" "));
    let re = regex::Regex::new(r"\d[\d,.]*%?").unwrap();
    for sec in d["sections"].as_array().unwrap() {
        for m in re.find_iter(sec["text"].as_str().unwrap()) {
            let t = m.as_str().trim_end_matches(['.', ',']);
            assert!(
                input.contains(t),
                "'{t}' in the draft is not from the input"
            );
        }
    }
    // The DOCX holds the same text and placeholders.
    let f = d["files"][0].as_str().unwrap();
    let doc = phn_local::docx::parse_docx(&std::fs::read(h.data().join(f)).unwrap()).unwrap();
    let plain = doc.plain_text();
    assert!(
        plain.contains("Statement of need")
            && plain.contains("[NURSE:")
            && plain.contains("Asthma Home Visits")
    );
    assert_eq!(s["privacy"]["files_written"][0], *f);
    assert!(caveats(&s).contains("lack a source or year"));
}

#[tokio::test]
async fn the_narrative_without_criteria_is_a_generic_skeleton_and_refuses_client_data() {
    let h = offline().await;
    let s = call_ok(
        &h,
        "draft_grant_narrative",
        json!({"write_document": false, "sections": ["sustainability"]}),
    )
    .await;
    assert_eq!(s["data"]["criteria_from"], "none");
    assert_eq!(s["data"]["sections"].as_array().unwrap().len(), 1);
    assert!(
        summary(
            &h.call("draft_grant_narrative", json!({"write_document": false}))
                .await
        )
        .contains("generic skeleton")
    );
    assert!(s["data"]["files"].as_array().unwrap().is_empty());
    // Typed criteria for budget and forms are listed as not drafted here.
    let s = call_ok(
        &h,
        "draft_grant_narrative",
        json!({"write_document": false,
               "criteria": [{"label": "Budget and Budget Justification", "points": 15},
                            {"label": "Sustainability", "points": 10}]}),
    )
    .await;
    assert!(
        s["data"]["criteria_not_drafted"][0]
            .as_str()
            .unwrap()
            .contains("build_program_budget")
    );
    assert_eq!(
        s["data"]["sections"][2]["criteria"][0],
        "Sustainability (10 points)"
    );
    // Errors the nurse can fix.
    let e = call_err(
        &h,
        "draft_grant_narrative",
        json!({"facts": {"health_problem": "Client SSN 123-45-6789 has asthma"}}),
    )
    .await;
    assert!(e.contains("health_problem") && !e.contains("123-45"));
    let e = call_err(
        &h,
        "draft_grant_narrative",
        json!({"criteria_file": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("not a spreadsheet") && leaks(&e, "data/line_list.csv", None).is_empty());
    let e = call_err(&h, "draft_grant_narrative", json!({"sections": []})).await;
    assert!(e.contains("sections"));
    let e = call_err(
        &h,
        "draft_grant_narrative",
        json!({"page_limit": 0, "write_document": false}),
    )
    .await;
    assert!(e.contains("page_limit"));
    let e = call_err(
        &h,
        "draft_grant_narrative",
        json!({"data_points": [{"label": "x", "value": ""}]}),
    )
    .await;
    assert!(e.contains("label and a value"));
}

#[tokio::test]
async fn the_grant_tools_are_planning_tools_and_the_prompt_lists() {
    let h = offline().await;
    let tools = h.client.list_all_tools().await.unwrap();
    for name in [
        "search_grant_opportunities",
        "analyze_funding_announcement",
        "draft_grant_narrative",
    ] {
        let t = tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name}"));
        assert!(t.description.as_ref().unwrap().len() > 200);
        assert!(
            phn_tools::registry::tools()
                .any(|m| m.name == name && m.toolset == phn_core::Toolset::Planning)
        );
    }
    let prompts = h.client.list_all_prompts().await.unwrap();
    assert!(
        prompts
            .iter()
            .any(|p| p.name == "prepare_grant_application")
    );
}

/// Network check, run by hand: `cargo test -p phn-server --test phase17_grants_tools live_ -- --ignored
/// --nocapture`. The COMMBUYS weekly list is read from mass.gov when it is reachable (the live list
/// changes every week, so only its shape is checked); the federal search is skipped without a key.
#[tokio::test]
#[ignore]
async fn live_commbuys_list_is_read_when_mass_gov_is_reachable() {
    let h = start(|c| c.offline = false).await;
    let s = call_ok(
        &h,
        "search_grant_opportunities",
        json!({"source": "commbuys", "closes_after": "2000-01-01", "limit": 100}),
    )
    .await;
    let cb = &s["data"]["commbuys"];
    println!(
        "mode {} as_of {} read {} reason {}",
        cb["mode"], cb["as_of"], cb["read"], cb["reason"]
    );
    println!(
        "{}",
        summary(
            &h.call("search_grant_opportunities", json!({"source": "commbuys"}))
                .await
        )
    );
}
