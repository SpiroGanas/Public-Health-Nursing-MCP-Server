//! MCP integration tests for the Phase 16 evidence tools (EBP-03, EBP-04):
//! output matches the schemas, the design is suggested from words and never
//! final, the nurse's answers are recorded and never invented, each
//! instrument's overall rule is applied, offline DOI handling falls back to the
//! DOI as entered, client records are refused, arithmetic errors in the PRISMA
//! counts are tool errors, and the files hold only what was typed.

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

fn question<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["data"]["checklist"]["questions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["id"] == id)
        .unwrap_or_else(|| panic!("no question {id}"))
}

fn output_file(h: &common::Harness, display: &str) -> std::path::PathBuf {
    h.data().join(display)
}

fn docx_text(path: &std::path::Path) -> String {
    phn_local::docx::read_docx(path).unwrap().plain_text()
}

const RCT_IDS: [&str; 7] = [
    "rct_sequence",
    "rct_concealment",
    "rct_blind_participants",
    "rct_blind_outcome",
    "rct_attrition",
    "rct_reporting",
    "rct_other",
];

// ---------------------------------------------------------------- appraise_evidence

#[tokio::test]
async fn a_study_file_suggests_the_design_and_lists_the_questions_without_echoing_the_text() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"path": "study_abstract.txt"}),
    )
    .await;
    let d = &s["data"]["design"];
    assert_eq!(d["chosen"], "randomized_trial");
    assert_eq!(d["basis"], "text");
    assert_eq!(d["strength"], "likely");
    let words: Vec<&str> = d["candidates"][0]["matched_words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert!(words.contains(&"randomly assigned") && words.contains(&"allocation concealment"));
    // The checklist is returned unanswered, with sources on every question.
    let qs = s["data"]["checklist"]["questions"].as_array().unwrap();
    assert_eq!(qs.len(), 13);
    assert!(qs.iter().all(|q| q["answer"].is_null()));
    assert!(
        qs.iter()
            .all(|q| q["citation"].as_str().unwrap().contains(", "))
    );
    assert_eq!(s["data"]["checklist"]["tally"]["answered"], 0);
    assert!(s["data"]["checklist"]["overall"]["rating"].is_null());
    // GRADE domains are offered, not rated.
    assert_eq!(
        s["data"]["certainty"]["grade_guide"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(s["data"]["certainty"]["starting_level_for_design"], "high");
    // The abstract's contact details and its text are never returned.
    let all = everything(
        &h.call("appraise_evidence", json!({"path": "study_abstract.txt"}))
            .await,
    );
    assert!(leaks(&all, "data/study_abstract.txt", None).is_empty());
    assert!(!all.contains("Emergency department visits per 100"));
    // The privacy block says what was found and that nothing was returned.
    assert_eq!(s["privacy"]["row_level_data"], false);
    let cats = s["privacy"]["identifiers_detected"].to_string();
    assert!(cats.contains("email") && cats.contains("phone"), "{cats}");
    assert!(
        s["data"]["study"]["text_source"]
            .as_str()
            .unwrap()
            .contains("study_abstract.txt")
    );
    assert!(s["data"]["record_file"].is_null());
    // CASP is a link with the licence flagged as unverified; the review is pending.
    assert!(
        s["data"]["casp"]["licence_note"]
            .as_str()
            .unwrap()
            .contains("NOT verified")
    );
    assert!(caveats(&s).contains("not the CASP checklists"));
    assert!(pending(&s));
    // Every source is dated and from bundled content.
    for src in s["sources"].as_array().unwrap() {
        let mode = src["mode"].as_str().unwrap();
        assert!(mode == "snapshot" || mode == "local", "{src}");
        assert!(src["retrieved"].as_str().is_some() || mode == "local");
    }
}

#[tokio::test]
async fn pasted_text_names_the_design_and_ambiguous_or_missing_designs_are_handled() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"text": "We searched four databases following PRISMA and pooled the trials in a \
                        meta-analysis of randomised trials."}),
    )
    .await;
    assert_eq!(s["data"]["design"]["chosen"], "systematic_review");
    assert_eq!(
        s["data"]["checklist"]["questions"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    assert_eq!(s["data"]["study"]["text_source"], "pasted text");
    // A design without a checklist is named, with a pointer.
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"text": "Sensitivity and specificity of the index test against a reference standard."}),
    )
    .await;
    assert!(s["data"]["design"]["chosen"].is_null());
    assert_eq!(
        s["data"]["design"]["unsupported"]["design"],
        "diagnostic_accuracy"
    );
    assert!(s["data"]["checklist"].is_null());
    // No design words: the choices are listed.
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"text": "A report about the county and its schools."}),
    )
    .await;
    assert!(s["data"]["design"]["chosen"].is_null());
    assert_eq!(
        s["data"]["design"]["available_designs"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert!(caveats(&s).contains("could not be identified"));
    // Naming the design gives its questions without any text.
    let s = call_ok(&h, "appraise_evidence", json!({"design": "qualitative"})).await;
    assert_eq!(s["data"]["design"]["basis"], "nurse");
    assert_eq!(
        s["data"]["checklist"]["questions"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    // Qualitative studies get the CERQual guide, not GRADE.
    assert_eq!(
        s["data"]["certainty"]["cerqual_guide"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(
        s["data"]["certainty"]["grade_guide"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn answers_are_recorded_the_overall_rule_applies_and_a_record_is_written() {
    let h = start(|_| {}).await;
    let mut answers: Vec<Value> = RCT_IDS
        .iter()
        .map(|id| json!({"question": id, "answer": "low"}))
        .collect();
    answers[1] = json!({"question": "rct_concealment", "answer": "unclear",
                         "note": "Sealed envelopes mentioned, no detail"});
    answers.push(json!({"question": "rct_harms", "answer": "cant_tell"}));
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "randomized_trial", "title": "School asthma trial",
               "answers": answers}),
    )
    .await;
    let o = &s["data"]["checklist"]["overall"];
    assert_eq!(o["rating"], "unclear");
    assert_eq!(o["complete"], true);
    assert!(o["citation"].as_str().unwrap().contains("Table 3"));
    assert_eq!(question(&s, "rct_concealment")["answer"], "unclear");
    assert_eq!(
        question(&s, "rct_concealment")["note"],
        "Sealed envelopes mentioned, no detail"
    );
    let concerns: Vec<&str> = s["data"]["checklist"]["concerns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["question"].as_str().unwrap())
        .collect();
    assert_eq!(concerns, vec!["rct_concealment", "rct_harms"]);
    assert_eq!(s["data"]["checklist"]["tally"]["answered"], 8);
    assert_eq!(s["data"]["checklist"]["tally"]["unanswered"], 5);
    // Without concealment as a key domain, every key domain is low.
    let keys: Vec<&str> = RCT_IDS
        .iter()
        .copied()
        .filter(|i| *i != "rct_concealment")
        .collect();
    let s2 = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "randomized_trial", "answers": answers, "key_domains": keys,
               "write_record": false}),
    )
    .await;
    assert_eq!(s2["data"]["checklist"]["overall"]["rating"], "low");
    assert!(s2["data"]["record_file"].is_null());
    // The Word record holds the answers and the study label.
    let f = s["data"]["record_file"].as_str().unwrap();
    assert!(f.contains("evidence_appraisal"), "{f}");
    let text = docx_text(&output_file(&h, f));
    assert!(text.contains("School asthma trial"));
    assert!(text.contains("Sealed envelopes mentioned, no detail"));
    assert!(text.contains("did not answer any appraisal question"));
    assert!(text.contains("Table 3") || text.contains("risk of bias"));
    assert_eq!(s["privacy"]["files_written"][0], f);
}

#[tokio::test]
async fn systematic_review_overall_confidence_follows_amstar_2() {
    let h = start(|_| {}).await;
    let ids = [
        "sr_01_pico",
        "sr_02_protocol",
        "sr_03_designs",
        "sr_04_search",
        "sr_05_selection",
        "sr_06_extraction",
        "sr_07_excluded",
        "sr_08_described",
        "sr_09_rob",
        "sr_10_funding",
        "sr_11_meta",
        "sr_12_meta_rob",
        "sr_13_rob_interpretation",
        "sr_14_heterogeneity",
        "sr_15_publication_bias",
        "sr_16_conflicts",
    ];
    let answers = |no: &[&str]| -> Vec<Value> {
        ids.iter()
            .map(|id| json!({"question": id, "answer": if no.contains(id) { "no" } else { "yes" }}))
            .collect()
    };
    let rating = |s: &Value| s["data"]["checklist"]["overall"]["rating"].clone();
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "systematic_review", "answers": answers(&[]), "write_record": false}),
    )
    .await;
    assert_eq!(rating(&s), "high");
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "systematic_review", "answers": answers(&["sr_02_protocol"]), "write_record": false}),
    )
    .await;
    assert_eq!(rating(&s), "low");
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "systematic_review",
               "answers": answers(&["sr_02_protocol", "sr_09_rob"]), "write_record": false}),
    )
    .await;
    assert_eq!(rating(&s), "critically_low");
    assert!(caveats(&s).contains("partial yes"));
    // A review with no meta-analysis can mark the three meta-analysis items not applicable.
    let mut a = answers(&[]);
    for item in a.iter_mut() {
        if ["sr_11_meta", "sr_12_meta_rob", "sr_15_publication_bias"]
            .contains(&item["question"].as_str().unwrap())
        {
            item["answer"] = json!("not_applicable");
        }
    }
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "systematic_review", "answers": a, "write_record": false}),
    )
    .await;
    assert_eq!(rating(&s), "high");
}

#[tokio::test]
async fn grade_and_cerqual_use_the_nurses_judgments() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "cohort",
               "grade": {"outcome": "Low birth weight",
                         "judgments": [{"domain": "risk_of_bias", "change": -1, "reason": "No control of confounders in 2 of 4"},
                                       {"domain": "large_effect", "change": 1}]},
               "write_record": false}),
    )
    .await;
    let g = &s["data"]["certainty"]["grade"];
    assert_eq!(g["start"], "low");
    assert_eq!(g["total_change"], 0);
    assert_eq!(g["computed"], "low");
    assert!(
        s["data"]["certainty"]["grade_guide"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(caveats(&s).contains("body of evidence"));
    // A systematic review needs the evidence type; trials start high.
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"design": "systematic_review", "grade": {"outcome": "x"}}),
    )
    .await;
    assert!(e.contains("evidence_type"), "{e}");
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "systematic_review",
               "grade": {"outcome": "Visits", "evidence_type": "randomized_trials",
                         "judgments": [{"domain": "imprecision", "change": -1}, {"domain": "inconsistency", "change": -1}],
                         "final_rating": "moderate", "final_reason": "Both concerns are borderline"},
               "write_record": true}),
    )
    .await;
    let g = &s["data"]["certainty"]["grade"];
    assert_eq!(g["computed"], "low");
    assert_eq!(g["final_rating"], "moderate");
    assert_eq!(g["final_differs"], true);
    let text = docx_text(&output_file(&h, s["data"]["record_file"].as_str().unwrap()));
    assert!(text.contains("GRADE certainty") && text.contains("Both concerns are borderline"));
    // Bad GRADE input is a tool error.
    for bad in [
        json!({"outcome": "x", "evidence_type": "randomized_trials", "judgments": [{"domain": "risk_of_bias", "change": 1}]}),
        json!({"outcome": "x", "evidence_type": "randomized_trials", "judgments": [{"domain": "vibes", "change": -1}]}),
        json!({"outcome": "x", "evidence_type": "randomized_trials", "final_rating": "low"}),
    ] {
        call_err(
            &h,
            "appraise_evidence",
            json!({"design": "randomized_trial", "grade": bad}),
        )
        .await;
    }
    // GRADE is for effects; qualitative findings use CERQual.
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"design": "qualitative", "grade": {"outcome": "x", "evidence_type": "observational"}}),
    )
    .await;
    assert!(e.contains("cerqual"));
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "qualitative",
               "cerqual": {"finding": "Parents value evening hours",
                           "concerns": [{"component": "adequacy", "level": "moderate", "reason": "Few studies"},
                                        {"component": "relevance", "level": "none"}],
                           "overall_confidence": "low"},
               "write_record": false}),
    )
    .await;
    let c = &s["data"]["certainty"]["cerqual"];
    assert_eq!(c["overall_confidence"], "Low confidence");
    assert_eq!(c["missing_components"].as_array().unwrap().len(), 2);
    assert!(
        c["overall_definition"]
            .as_str()
            .unwrap()
            .contains("possible")
    );
}

#[tokio::test]
async fn problems_the_nurse_can_fix_are_tool_errors() {
    let h = start(|_| {}).await;
    // Nothing to work from.
    assert!(
        call_err(&h, "appraise_evidence", json!({}))
            .await
            .contains("give the study")
    );
    // Answers need the design.
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"text": "A randomized trial.", "answers": [{"question": "rct_sequence", "answer": "low"}]}),
    )
    .await;
    assert!(e.contains("give the design"));
    // Wrong question, wrong scale value, duplicate answer.
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"design": "randomized_trial", "answers": [{"question": "sr_01_pico", "answer": "yes"}]}),
    )
    .await;
    assert!(e.contains("not a question for this design"));
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"design": "randomized_trial", "answers": [{"question": "rct_sequence", "answer": "yes"}]}),
    )
    .await;
    assert!(e.contains("Allowed: low, unclear, high"), "{e}");
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"design": "randomized_trial",
               "answers": [{"question": "rct_harms", "answer": "yes"}, {"question": "rct_harms", "answer": "no"}]}),
    )
    .await;
    assert!(e.contains("more than once"));
    // Both text and path; an unknown file; a data file that is not a document.
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"text": "x randomized", "path": "study_abstract.txt"}),
    )
    .await;
    assert!(e.contains("either text or path"));
    assert!(
        call_err(&h, "appraise_evidence", json!({"path": "nope.txt"}))
            .await
            .contains("nope.txt")
    );
    assert!(
        call_err(&h, "appraise_evidence", json!({"path": "line_list.csv"}))
            .await
            .contains("TXT, Markdown")
    );
    // Overlong text; a bad DOI.
    let long = "word ".repeat(70_000);
    assert!(
        call_err(&h, "appraise_evidence", json!({"text": long}))
            .await
            .contains("longer than")
    );
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"doi": "not a doi", "design": "cohort"}),
    )
    .await;
    assert!(e.contains("doi must look like"));
    // Nothing to record without a design; with one, a blank worksheet is written.
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"text": "A report about schools.", "write_record": true}),
    )
    .await;
    assert!(e.contains("nothing to record"), "{e}");
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "cohort", "write_record": true}),
    )
    .await;
    let text = docx_text(&output_file(&h, s["data"]["record_file"].as_str().unwrap()));
    assert!(text.contains("not answered") && text.contains("0 of 16 questions answered"));
}

#[tokio::test]
async fn client_records_are_refused_without_echoing_them() {
    let h = start(|_| {}).await;
    let e = call_err(&h, "appraise_evidence", json!({"path": "client_note.txt"})).await;
    assert!(e.contains("client identifiers"), "{e}");
    assert!(leaks(&e, "data/client_note.txt", None).is_empty(), "{e}");
    // Pasted client details are refused too.
    let e = call_err(
        &h,
        "appraise_evidence",
        json!({"text": "Client DOB: 04/17/1958, SSN 921-55-0193 randomized"}),
    )
    .await;
    assert!(e.contains("client identifiers"));
    assert!(!e.contains("921-55-0193") && !e.contains("04/17/1958"));
    // A note with a personal identifier is redacted in the response and the record.
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"design": "randomized_trial",
               "answers": [{"question": "rct_sequence", "answer": "low",
                            "note": "Ask perdita.stonehallow@example.org or call 413-555-0191"}]}),
    )
    .await;
    let all = serde_json::to_string(&s).unwrap();
    let found = leaks(&all, "data/study_abstract.txt", None);
    assert!(found.is_empty(), "{found:?} in {all}");
    assert!(caveats(&s).contains("redacted"));
    let text = docx_text(&output_file(&h, s["data"]["record_file"].as_str().unwrap()));
    assert!(
        leaks(&text, "data/study_abstract.txt", None).is_empty(),
        "{text}"
    );
}

#[tokio::test]
async fn offline_a_doi_is_used_as_entered_and_the_response_says_so() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "appraise_evidence",
        json!({"doi": "https://doi.org/10.1136/bmj.n71", "design": "systematic_review"}),
    )
    .await;
    let d = &s["data"]["study"]["doi"];
    assert_eq!(d["doi"], "10.1136/bmj.n71");
    assert_eq!(d["source_used"], "doi_as_entered");
    assert!(d["metadata"].is_null());
    assert!(d["fallback_reason"].as_str().unwrap().contains("offline"));
    assert!(caveats(&s).contains("Crossref was not used"));
    assert!(
        s["privacy"]["notes"]
            .to_string()
            .contains("Nothing was sent to Crossref")
    );
    // No Crossref source is cited.
    assert!(!s["sources"].to_string().contains("Crossref REST API"));
    // A DOI with nothing else is enough to start (the design is then unknown).
    let s = call_ok(&h, "appraise_evidence", json!({"doi": "10.1000/xyz123"})).await;
    assert!(s["data"]["design"]["chosen"].is_null());
}

// ---------------------------------------------------------------- build_evidence_synthesis_table

fn prisma_ok() -> Value {
    json!({"records_identified": 1333,
           "identified_by_source": [{"source": "PubMed", "records": 800}, {"source": "Other database", "records": 533}],
           "duplicates_removed": 241, "records_screened": 1092, "records_excluded": 1050,
           "reports_not_retrieved": 2, "reports_assessed": 40,
           "reports_excluded": [{"reason": "Wrong population", "reports": 20},
                                {"reason": "No comparison group", "reports": 14}],
           "studies_included": 6})
}

fn studies() -> Value {
    json!([
        {"citation": "Fictional trial A", "year": 2019, "design": "Randomized trial", "population": "Elementary students with asthma",
         "sample_size": 1208, "intervention": "Nurse-led self-management", "comparator": "usual care",
         "outcome": "ED visits", "effect": "RR 0.73 (0.58 to 0.93)", "quality_rating": "Low", "quality_tool": "Cochrane 2011"},
        {"citation": "Fictional trial B", "year": 2021, "design": "randomized trial", "sample_size": 300,
         "outcome": "ED visits", "effect": "RR 0.90", "quality_rating": "Unclear", "quality_tool": "Cochrane 2011"},
        {"citation": "Fictional cohort C", "year": 2020, "design": "Cohort", "sample_size": 5000,
         "outcome": "Admissions", "effect": "OR 0.80", "quality_rating": "Moderate", "quality_tool": "ROBINS-I"},
        {"citation": "Fictional qualitative D", "design": "Qualitative"}
    ])
}

#[tokio::test]
async fn synthesis_builds_the_table_summary_diagram_and_files() {
    let h = start(|_| {}).await;
    let s = call_ok(
        &h,
        "build_evidence_synthesis_table",
        json!({"question": "School asthma programs", "studies": studies(), "prisma": prisma_ok()}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["rows"].as_array().unwrap().len(), 4);
    let sm = &d["summary"];
    assert_eq!(sm["studies"], 4);
    assert_eq!(sm["studies_with_sample_size"], 3);
    assert_eq!(sm["total_participants"], 6508);
    assert_eq!(sm["median_sample_size"], 1208.0);
    let by_design: Vec<(String, u64)> = sm["by_design"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                x["label"].as_str().unwrap().to_string(),
                x["studies"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        by_design,
        vec![
            ("Randomized trial".to_string(), 2),
            ("Cohort".to_string(), 1),
            ("Qualitative".to_string(), 1)
        ]
    );
    // The flow, with defaults named, and the diagram in three forms.
    let p = &d["prisma"];
    assert_eq!(p["flow"]["reports_sought"], 42);
    assert_eq!(p["flow"]["reports_excluded_total"], 34);
    assert!(
        p["flow"]["defaults_used"]
            .to_string()
            .contains("reports_sought")
    );
    assert!(p["mermaid"].as_str().unwrap().contains("flowchart TD"));
    let svg = p["svg"].as_str().unwrap();
    assert!(svg.contains("role=\"img\"") && svg.contains("<title") && svg.contains("<desc"));
    assert!(!svg.contains("<script"));
    assert!(p["text_alternative"].as_array().unwrap().len() >= 5);
    // The table has 4 studies and the diagram says 6 were included: a caveat, not an error.
    assert!(caveats(&s).contains("The table has 4 studies but the PRISMA counts say 6"));
    // Files: Word, Excel, SVG, Mermaid.
    let files: Vec<&str> = d["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert_eq!(files.len(), 4, "{files:?}");
    for ext in ["docx", "xlsx", "svg", "mmd"] {
        assert!(files.iter().any(|f| f.ends_with(ext)), "{ext} in {files:?}");
    }
    for f in &files {
        assert!(output_file(&h, f).exists(), "{f}");
    }
    let docx = files.iter().find(|f| f.ends_with("docx")).unwrap();
    let text = docx_text(&output_file(&h, docx));
    assert!(text.contains("School asthma programs") && text.contains("Fictional trial A"));
    assert!(text.contains("1,333 records identified"));
    assert!(text.contains("did not look up, rate, or interpret"));
    // The workbook reads back with the table, the summary figures, and the PRISMA counts.
    let xlsx = files.iter().find(|f| f.ends_with("xlsx")).unwrap();
    let path = output_file(&h, xlsx);
    let t = phn_local::table::read_table(&path, Some("Synthesis table")).unwrap();
    assert_eq!(t.rows.len(), 4);
    assert_eq!(t.rows[0][0], "Fictional trial A");
    let total = phn_local::table::read_table(&path, Some("Summary")).unwrap();
    let flat: Vec<String> = total
        .rows
        .iter()
        .chain(std::iter::once(&total.headers))
        .map(|r| r.join("|"))
        .collect();
    let joined = flat.join("\n");
    assert!(
        joined.contains("Total participants (sum of n)|6508"),
        "{joined}"
    );
    assert!(joined.contains("Number of studies|4"), "{joined}");
    let pr = phn_local::table::read_table(&path, Some("PRISMA counts")).unwrap();
    let ok_flags = pr
        .rows
        .iter()
        .filter(|r| r.last().map(String::as_str) == Some("OK"))
        .count();
    assert!(ok_flags >= 4, "{:?}", pr.rows);
    // The SVG file is the same as the one returned.
    let svg_file = files.iter().find(|f| f.ends_with("svg")).unwrap();
    assert_eq!(
        std::fs::read_to_string(output_file(&h, svg_file)).unwrap(),
        svg
    );
    assert!(pending(&s));
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn prisma_arithmetic_errors_are_tool_errors_and_nothing_is_written() {
    let h = start(|_| {}).await;
    let mut bad = prisma_ok();
    bad["records_screened"] = json!(1000);
    let e = call_err(
        &h,
        "build_evidence_synthesis_table",
        json!({"studies": studies(), "prisma": bad}),
    )
    .await;
    assert!(e.contains("The PRISMA counts do not add up"), "{e}");
    assert!(
        e.contains(
            "records_screened is 1000 but 1333 identified - 241 duplicates - 0 other removed = 1092"
        ),
        "{e}"
    );
    let mut bad = prisma_ok();
    bad["reports_assessed"] = json!(50);
    bad["reports_excluded"][0]["reports"] = json!(60);
    let e = call_err(
        &h,
        "build_evidence_synthesis_table",
        json!({"studies": studies(), "prisma": bad}),
    )
    .await;
    assert!(
        e.contains("reports_assessed is 50 but 42 sought - 2 not retrieved = 40"),
        "{e}"
    );
    assert!(e.contains("exclusion reasons add up to 74"), "{e}");
    // Nothing was written (the output folder was never created).
    assert!(!h.data().join("output/evidence_synthesis").exists());
    // Negative counts, an empty exclusion reason, a missing reports total for 2 reports per study.
    let mut neg = prisma_ok();
    neg["duplicates_removed"] = json!(-1);
    let e = call_err(&h, "build_evidence_synthesis_table", json!({"prisma": neg})).await;
    assert!(e.contains("duplicates_removed is -1"));
    let mut multi = prisma_ok();
    multi["studies_included"] = json!(5);
    let e = call_err(
        &h,
        "build_evidence_synthesis_table",
        json!({"prisma": multi}),
    )
    .await;
    assert!(e.contains("give reports_of_included_studies"));
    // With the reports total the same counts are fine.
    let mut ok = prisma_ok();
    ok["studies_included"] = json!(5);
    ok["reports_of_included_studies"] = json!(6);
    call_ok(
        &h,
        "build_evidence_synthesis_table",
        json!({"prisma": ok, "write_files": false}),
    )
    .await;
    // Bad studies and nothing at all.
    let e = call_err(
        &h,
        "build_evidence_synthesis_table",
        json!({"studies": [{"citation": "", "sample_size": -3, "year": 1800}]}),
    )
    .await;
    assert!(e.contains("needs a citation") && e.contains("sample_size -3") && e.contains("1800"));
    assert!(
        call_err(&h, "build_evidence_synthesis_table", json!({}))
            .await
            .contains("give the studies")
    );
}

#[tokio::test]
async fn synthesis_options_counts_only_one_format_and_no_files() {
    let h = start(|_| {}).await;
    // Counts only, Word only: the diagram is described and drawn; no workbook.
    let s = call_ok(
        &h,
        "build_evidence_synthesis_table",
        json!({"prisma": prisma_ok(), "format": "docx"}),
    )
    .await;
    let files = s["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert!(!files.iter().any(|f| f.as_str().unwrap().ends_with("xlsx")));
    assert!(s["data"]["summary"].is_null());
    // Studies only, Excel only: no diagram files.
    let s = call_ok(
        &h,
        "build_evidence_synthesis_table",
        json!({"studies": studies(), "format": "xlsx"}),
    )
    .await;
    let files = s["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].as_str().unwrap().ends_with("xlsx"));
    assert!(s["data"]["prisma"].is_null());
    // No files requested: nothing written, everything still returned.
    let s = call_ok(
        &h,
        "build_evidence_synthesis_table",
        json!({"studies": studies(), "prisma": prisma_ok(), "write_files": false}),
    )
    .await;
    assert!(s["data"]["files"].as_array().unwrap().is_empty());
    assert!(
        s["privacy"]["files_written"]
            .as_array()
            .is_none_or(|a| a.is_empty())
    );
    // Quality ratings from different tools are counted separately.
    let ratings: Vec<(String, String)> = s["data"]["summary"]["by_quality"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| {
            (
                q["quality_tool"].as_str().unwrap().into(),
                q["quality_rating"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert!(ratings.contains(&("Cochrane 2011".into(), "Low".into())));
    assert!(ratings.contains(&("ROBINS-I".into(), "Moderate".into())));
    assert!(ratings.contains(&("(not stated)".into(), "(not stated)".into())));
}
