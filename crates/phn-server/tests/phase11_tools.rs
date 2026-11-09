//! MCP integration tests for the Phase 11 tools (substance use, crisis
//! resources, and health communication): successful calls validate against
//! each tool's output schema, problems the nurse can fix are tool-level
//! errors, clinical answers carry the decision-support notice and review
//! caveats, files land in the output folder, and identifiers in local files
//! never come back.

mod common;

use common::{everything, leaks, start, summary};
use serde_json::{Value, json};

const DECISION_SUPPORT: &str = "Decision support only";

async fn call_ok(h: &common::Harness, name: &str, args: Value) -> Value {
    let tools = h.client.list_all_tools().await.unwrap();
    let r = h.call(name, args.clone()).await;
    assert_eq!(r.is_error, Some(false), "{name} {args}: {}", everything(&r));
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

fn md(s: &Value) -> &str {
    s["summary"].as_str().unwrap()
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

fn has_pending_review_caveat(s: &Value) -> bool {
    caveats(s).contains("has not yet been reviewed by the subject matter expert")
}

#[allow(dead_code)]
fn decision_support(s: &Value) -> bool {
    s["notice"]
        .as_str()
        .is_some_and(|d| d.contains(DECISION_SUPPORT))
}

// ------------------------------------------------------------------ COMM-01

const FLYER: &str = "# Protect your family from the flu\n\n\
Get a flu shot every year. It is the best way to protect yourself and the people around you.\n\n\
## What you need to know\n\n\
Influenza surveillance data are utilized by the Department to monitor the prevalence of \
respiratory illness, and individuals with chronic conditions are at elevated risk of \
complications that may necessitate hospitalization, so vaccination is recommended by CDC \
for all persons aged 6 months and older without contraindications.\n\n\
- Wash your hands often.\n- Stay home when you are sick.\n- Cover your cough.\n\n\
About 1 in 5 people get the flu each year. Call your doctor if you have a fever over 100.4 F.";

#[tokio::test]
async fn readability_scores_flags_and_clear_communication_index() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "check_readability", json!({"text": FLYER})).await;
    let d = &s["data"];
    assert_eq!(d["checked"], "pasted text");
    assert_eq!(d["counts"]["headings"], 2);
    assert_eq!(d["counts"]["lists"], 1);
    assert_eq!(d["counts"]["list_items"], 3);
    let ids: Vec<&str> = d["scores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            "flesch_reading_ease",
            "flesch_kincaid_grade",
            "smog",
            "gunning_fog"
        ]
    );
    // The long jargon-filled sentence is flagged with CDC alternatives.
    assert_eq!(d["flags"]["long_sentence_count"], 1);
    let jargon: Vec<&str> = d["flags"]["jargon"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["term"].as_str().unwrap())
        .collect();
    for t in ["Surveillance", "Utilize", "Prevalence"] {
        assert!(jargon.contains(&t), "{t} missing from {jargon:?}");
    }
    assert!(d["flags"]["passive_voice_count"].as_u64().unwrap() >= 1);
    assert!(d["flags"]["numbers"]["ratios"].as_u64().unwrap() >= 1);
    // The Clear Communication Index needs the nurse's answers; suggestions are offered.
    let cci = &d["clear_communication_index"];
    assert!(cci["score"].is_null());
    assert_eq!(cci["before_you_begin"].as_array().unwrap().len(), 4);
    let item8 = &cci["items"][7];
    assert_eq!(item8["number"], 8);
    assert_eq!(item8["suggestion"], "check");
    assert!(md(&s).contains("Estimated reading level"));
    assert!(md(&s).contains("Clear Communication Index"));
    assert!(has_pending_review_caveat(&s));
    assert!(s["notice"].is_null(), "not a clinical answer");
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["url"] == "https://www.cdc.gov/ccindex/pdf/full-index-score-sheet.pdf")
    );

    // With every answer, the Index is scored (hand-computed: 15 of 19 = 78.9).
    let answers = json!({"1": "yes", "2": "yes", "3": "yes", "4": "no", "5": "yes", "6": "yes",
        "7": "no", "8": "yes", "9": "yes", "10": "yes", "11": "yes", "12": "yes", "13": "yes",
        "14": "yes", "C": "yes", "15": "yes", "16": "no", "17": "yes", "D": "yes", "18": "yes",
        "19": "na", "20": "yes"});
    let s = call_ok(
        &h,
        "check_readability",
        json!({"text": FLYER, "cci_answers": answers, "target_grade": 6}),
    )
    .await;
    let cci = &s["data"]["clear_communication_index"];
    assert_eq!(cci["score"], 78.9);
    assert_eq!(cci["passing"], false);
    assert_eq!(s["data"]["grade_estimate"]["target_grade"], 6.0);
    assert!(md(&s).contains("Score **79**"));
}

#[tokio::test]
async fn readability_reads_files_and_never_echoes_identifiers() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "check_readability", json!({"path": "client_note.txt"})).await;
    assert_eq!(s["data"]["checked"], "client_note.txt");
    assert!(
        !s["privacy"]["identifiers_detected"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let r = h
        .call("check_readability", json!({"path": "client_note.txt"}))
        .await;
    let found = leaks(&everything(&r), "data/client_note.txt", None);
    assert!(found.is_empty(), "leaked: {found:?}");

    // A DOCX with real headings and a bulleted list.
    let doc = phn_docs::model::Document {
        title: "Lead".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: None,
        logo: None,
        blocks: phn_docs::model::from_markdown(
            "# Lead and your child\n\nLead can hurt your child's brain. Test your child at ages 1 and 2.\n\n- Wash hands.\n- Wipe floors.",
        ),
    };
    std::fs::write(
        h.data().join("lead.docx"),
        phn_docs::docx::to_docx(&doc).unwrap(),
    )
    .unwrap();
    let s = call_ok(&h, "check_readability", json!({"path": "lead.docx"})).await;
    assert!(s["data"]["counts"]["headings"].as_u64().unwrap() >= 1);
    assert_eq!(s["data"]["counts"]["list_items"], 2);
}

#[tokio::test]
async fn readability_rejects_bad_input() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, "check_readability", json!({})).await;
    assert!(e.contains("give the draft"), "{e}");
    let e = call_err(
        &h,
        "check_readability",
        json!({"text": "Hi.", "path": "x.txt"}),
    )
    .await;
    assert!(e.contains("not both"), "{e}");
    let e = call_err(
        &h,
        "check_readability",
        json!({"text": "Get a shot.", "cci_answers": {"3": "maybe"}}),
    )
    .await;
    assert!(e.contains("not yes, no, or na"), "{e}");
    let e = call_err(
        &h,
        "check_readability",
        json!({"text": "Get a shot.", "cci_answers": {"5": "na"}}),
    )
    .await;
    assert!(e.contains("cannot be 'na'"), "{e}");
    let e = call_err(&h, "check_readability", json!({"path": "line_list.csv"})).await;
    assert!(e.contains("reads DOCX"), "{e}");
}

// ------------------------------------------------------------------ MH-03

#[tokio::test]
async fn overdose_data_for_a_town_its_county_and_the_state_offline() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "get_overdose_data",
        json!({"area": "Worcester", "compare_with": ["Pittsfield"]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["level"], "town");
    let towns = d["towns"].as_array().unwrap();
    assert_eq!(towns.len(), 2);
    let w = &towns[0];
    assert_eq!(w["town"], "Worcester");
    assert_eq!(w["years"].as_array().unwrap().len(), 8);
    // DPH June 2023 table, Table 1: Worcester residents, 2022 = 140.
    assert_eq!(w["years"][7]["deaths"], 140);
    assert!(w["rate_2018_2022"]["rate_per_100k"].as_f64().unwrap() > 0.0);
    assert!(w["compared_with_state"].as_str().is_some());
    // CDC county counts come from the bundled snapshot when offline.
    let counties: Vec<&str> = d["counties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["area"].as_str().unwrap())
        .collect();
    assert!(
        counties.iter().any(|c| c.starts_with("Worcester")),
        "{counties:?}"
    );
    assert!(
        counties.iter().any(|c| c.starts_with("Berkshire")),
        "{counties:?}"
    );
    assert_eq!(d["state_and_us"].as_array().unwrap().len(), 2);
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["mode"] == "snapshot"
                && x["url"].as_str().unwrap_or("").contains("gb4e-yj24"))
    );
    assert!(caveats(&s).contains("bundled snapshot"));
    assert!(has_pending_review_caveat(&s));
    assert!(md(&s).contains("DPH statewide findings for 2024"));
    assert!(md(&s).contains("BSAS dashboard"));

    // Statewide by drug class.
    let s = call_ok(
        &h,
        "get_overdose_data",
        json!({"area": "Massachusetts", "drug": "fentanyl"}),
    )
    .await;
    assert_eq!(s["data"]["level"], "state");
    assert_eq!(s["data"]["by_drug"].as_array().unwrap().len(), 8);
    assert!(
        s["data"]["state_and_us"][0]["measure"]
            .as_str()
            .unwrap()
            .contains("fentanyl")
    );

    // A small town: no rate under 5 deaths; occurrence basis.
    let s = call_ok(
        &h,
        "get_overdose_data",
        json!({"area": "Mount Washington", "basis": "occurrence"}),
    )
    .await;
    assert_eq!(s["data"]["towns"][0]["basis"], "occurrence");
    assert!(s["data"]["towns"][0]["rate_2018_2022"].is_null());
    let s = call_ok(&h, "get_overdose_data", json!({"area": "Alford"})).await;
    let r = &s["data"]["towns"][0]["rate_2018_2022"];
    assert!(r["rate_per_100k"].is_null());
    assert!(r["flags"][0].as_str().unwrap().contains("fewer than 5"));
    // DPH spells it "North Attleboro".
    call_ok(
        &h,
        "get_overdose_data",
        json!({"area": "North Attleborough"}),
    )
    .await;
}

#[tokio::test]
async fn overdose_data_rejects_bad_input() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, "get_overdose_data", json!({"area": "Springfeld Ohio"})).await;
    assert!(e.contains("not a Massachusetts"), "{e}");
    let e = call_err(&h, "get_overdose_data", json!({"drug": "kratom"})).await;
    assert!(e.contains("not recognized"), "{e}");
    let e = call_err(&h, "get_overdose_data", json!({"basis": "work"})).await;
    assert!(e.contains("residence or occurrence"), "{e}");
}

// ------------------------------------------------------------------ MH-07

#[tokio::test]
async fn youth_survey_defaults_to_mental_health_with_us_comparison() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "get_youth_risk_behavior_data", json!({})).await;
    let d = &s["data"];
    let first = &d["indicators"][0];
    assert_eq!(first["year"], 2025);
    assert_eq!(first["school_level"], "high");
    assert!(first["topic"].as_str().unwrap().contains("Mental health"));
    // Female students: higher than male students (MYHS 2025 sadness: 36.2 vs 18.4).
    let flagged = d["indicators"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|i| i["groups"].as_array().unwrap().iter())
        .any(|g| {
            g["group"] == "Female"
                && g["versus_reference"]
                    .as_str()
                    .is_some_and(|v| v.contains("higher"))
        });
    assert!(flagged);
    let cdc = d["cdc_comparison"].as_array().unwrap();
    assert!(cdc.len() >= 4, "{cdc:?}");
    assert!(cdc[0]["years"].as_array().unwrap().len() >= 2);
    assert!(d["crisis_line"].as_str().unwrap().contains("988"));
    assert!(caveats(&s).contains("bundled snapshot"));
    assert!(has_pending_review_caveat(&s));
    assert!(md(&s).contains("Massachusetts vs the U.S."));
}

#[tokio::test]
async fn youth_survey_topics_levels_and_groups() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "get_youth_risk_behavior_data",
        json!({"topic": "vaping", "school_level": "middle", "by": ["grade"]}),
    )
    .await;
    let inds = s["data"]["indicators"].as_array().unwrap();
    assert!(!inds.is_empty());
    for i in inds {
        assert_eq!(i["school_level"], "middle");
        assert!(
            i["indicator"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("vap")
        );
        for g in i["groups"].as_array().unwrap() {
            assert_eq!(g["group_type"], "Grade");
        }
    }
    assert!(s["data"]["cdc_comparison"].as_array().unwrap().is_empty());
    let s = call_ok(
        &h,
        "get_youth_risk_behavior_data",
        json!({"topic": "bullied", "survey": "MYRBS", "compare_us": false}),
    )
    .await;
    assert!(
        s["data"]["indicators"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["survey"] == "MYRBS")
    );
    let e = call_err(
        &h,
        "get_youth_risk_behavior_data",
        json!({"topic": "skateboarding helmets"}),
    )
    .await;
    assert!(e.contains("Topics:"), "{e}");
    let e = call_err(
        &h,
        "get_youth_risk_behavior_data",
        json!({"school_level": "college"}),
    )
    .await;
    assert!(e.contains("high, middle, or both"), "{e}");
}

// ------------------------------------------------------------------ MH-04

#[tokio::test]
async fn naloxone_resources_town_status_script_and_handout() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "find_naloxone_and_harm_reduction",
        json!({"town": "Pittsfield", "handout": "docx"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["syringe_services"]["town"], "Pittsfield");
    assert_eq!(d["syringe_services"]["approved"], true);
    assert!(
        d["syringe_services"]["approved_in_county"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "North Adams")
    );
    assert_eq!(
        d["standing_order_formulations"].as_array().unwrap().len(),
        5
    );
    assert!(d["teaching_script"].as_array().unwrap().len() >= 6);
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));
    let file = d["handout_file"].as_str().unwrap();
    assert!(file.starts_with("output/"), "{file}");
    assert!(h.data().join(file).exists());
    assert_eq!(s["privacy"]["files_written"][0], file);
    assert!(md(&s).contains("(800) 327-5050"));
    assert!(md(&s).contains("Call 911"));
    // The handout passes the server's own accessibility check.
    let a = call_ok(&h, "check_document_accessibility", json!({"path": file})).await;
    assert_eq!(a["data"]["report"]["failed"], 0, "{}", md(&a));

    let s = call_ok(
        &h,
        "find_naloxone_and_harm_reduction",
        json!({"town": "Wilmington", "include_teaching_script": false}),
    )
    .await;
    assert_eq!(s["data"]["syringe_services"]["approved"], false);
    assert!(s["data"]["teaching_script"].as_array().unwrap().is_empty());
    let e = call_err(
        &h,
        "find_naloxone_and_harm_reduction",
        json!({"town": "Springfeld"}),
    )
    .await;
    assert!(e.contains("Did you mean"), "{e}");
}

// ------------------------------------------------------------------ MH-05

#[tokio::test]
async fn treatment_search_filters_offline_snapshot() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "find_substance_use_treatment",
        json!({"town": "Springfield", "services": ["buprenorphine"], "payment": "masshealth", "max_results": 50}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["town"], "Springfield");
    let n = d["matching"].as_u64().unwrap();
    assert!(n >= 3, "{n}");
    assert!(d["facilities_in_radius"].as_u64().unwrap() >= n);
    for f in d["facilities"].as_array().unwrap() {
        assert!(f["miles"].as_f64().unwrap() <= 15.0);
        assert!(
            f["moud"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m == "buprenorphine")
        );
        assert!(
            f["payment"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p == "Medicaid")
        );
    }
    assert!(caveats(&s).contains("bundled statewide copy"));
    assert!(md(&s).contains("(800) 327-5050"));
    assert!(s["sources"].as_array().unwrap().iter().any(|x| {
        x["mode"] == "snapshot"
            && x["url"]
                .as_str()
                .unwrap_or("")
                .contains("findtreatment.gov/locator")
    }));
    // Methadone is offered only at a few sites; Spanish narrows further.
    let s2 = call_ok(
        &h,
        "find_substance_use_treatment",
        json!({"town": "Springfield", "services": ["methadone"], "language": "spanish"}),
    )
    .await;
    assert!(s2["data"]["matching"].as_u64().unwrap() <= n);
    let e = call_err(
        &h,
        "find_substance_use_treatment",
        json!({"town": "Springfield", "services": ["acupuncture"]}),
    )
    .await;
    assert!(e.contains("not recognized"), "{e}");
    let e = call_err(&h, "find_substance_use_treatment", json!({})).await;
    assert!(e.contains("give a town"), "{e}");
}

// ------------------------------------------------------------------ MH-06

#[tokio::test]
async fn postvention_steps_templates_check_and_local_fill() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "plan_school_postvention",
        json!({"cause_status": "unconfirmed", "phase": "first_24_hours", "messages": ["student_notification", "parent_letter", "media_statement"]}),
    )
    .await;
    let d = &s["data"];
    assert!(
        d["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["phase"] == "first_24_hours")
    );
    let t = d["templates"].as_array().unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!(t[0]["cause_status"], "unconfirmed");
    assert!(t.iter().all(|x| x["safe_messaging_problems"] == 0));
    assert!(
        t[0]["placeholders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "Student Name")
    );
    assert!(md(&s).contains("homerooms"));
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));
    assert!(d["crisis_line"].as_str().unwrap().contains("988"));

    // A draft with problems: flagged by sentence, method words never echoed.
    let s = call_ok(
        &h,
        "plan_school_postvention",
        json!({"draft_text": "Our student committed suicide because of bullying. The body was found near the railroad bridge. He is at peace now."}),
    )
    .await;
    let check = &s["data"]["draft_check"];
    assert!(check["must_fix"].as_u64().unwrap() >= 1);
    let rules: Vec<&str> = check["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule"].as_str().unwrap())
        .collect();
    for r in [
        "outdated_language",
        "simple_cause",
        "death_scene",
        "glamorizing",
        "help_resources",
    ] {
        assert!(rules.contains(&r), "{r} missing: {rules:?}");
    }
    assert!(
        !md(&s).contains("railroad"),
        "method details are never echoed"
    );

    // Local fill: documents written, details never returned.
    let r = h
        .call(
            "plan_school_postvention",
            json!({"cause_status": "ruled_suicide", "details_path": "postvention_details.csv", "messages": ["student_notification", "parent_letter", "staff_meeting"]}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let found = leaks(&everything(&r), "data/postvention_details.csv", None);
    assert!(found.is_empty(), "leaked: {found:?}");
    let s = r.structured_content.unwrap();
    let files = s["data"]["files_written"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(s["data"]["records_filled"], 1);
    let f = files[1].as_str().unwrap();
    let docx = phn_local::docx::read_docx(&h.data().join(f)).unwrap();
    assert!(docx.plain_text().contains("Thessaly Brandquist"));
    assert!(docx.plain_text().contains("988"));

    let e = call_err(
        &h,
        "plan_school_postvention",
        json!({"cause_status": "maybe"}),
    )
    .await;
    assert!(e.contains("not recognized"), "{e}");
    let e = call_err(
        &h,
        "plan_school_postvention",
        json!({"details_path": "exclusion_letters.csv", "messages": ["parent_letter"]}),
    )
    .await;
    assert!(e.contains("do not match any column"), "{e}");
}
