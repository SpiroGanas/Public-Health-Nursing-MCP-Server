//! MCP integration tests for the Phase 18 ethics tools (EBP-07
//! `qi_vs_research_determination`, POL-05 `public_health_ethics_analysis`,
//! POL-06 `nursing_scope_of_practice_guide`): outputs match their schemas, the
//! QI tool never reads for the nurse and gives no reading without the required
//! answers, the ethics tool never recommends, the scope guide cites and
//! refers (never says an act is in scope), content is pending review, problems
//! the nurse can fix are tool errors, identifiers are refused without being
//! echoed, and documents land in the output folder.

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
    let path = h.data().join(display);
    assert!(path.exists(), "{display} was not written");
    let out = std::process::Command::new("unzip")
        .args(["-p", path.to_str().unwrap(), "word/document.xml"])
        .output()
        .expect("unzip");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn ans(id: &str, a: &str) -> Value {
    json!({"question_id": id, "answer": a})
}

// ---------------------------------------------------------------- EBP-07

#[tokio::test]
async fn qi_with_no_answers_lists_the_questions_and_writes_a_template() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "qi_vs_research_determination", json!({})).await;
    let d = &s["data"];
    assert_eq!(d["status"], "questions_only");
    assert!(d["classification"].is_null());
    let q = d["reasoning"].as_array().unwrap();
    assert_eq!(q.len(), 14);
    assert!(
        q.iter()
            .all(|x| x["answer"].is_null() && x["effect"] == "unanswered")
    );
    assert!(q.iter().all(|x| !x["basis"].as_array().unwrap().is_empty()));
    assert_eq!(d["needs_answers"].as_array().unwrap().len(), 3);
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    assert!(d["what_the_irb_decides"].as_str().unwrap().contains("IRB"));
    let text = docx_text(&h, d["document_file"].as_str().unwrap());
    assert!(
        text.contains("[NURSE: project title]"),
        "template placeholders"
    );
    assert!(
        text.contains("not yet available"),
        "no provisional reading yet"
    );
    // Sources carry dates and cite the regulation and CDC's guidelines.
    let titles: String = s["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["title"].as_str().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        titles.contains("46.102")
            && titles.contains("Guidelines for Defining Public Health Research")
    );
    assert!(s["sources"].as_array().unwrap().iter().all(
        |x| x["effective_date"].is_string() || x["title"].as_str().unwrap().contains("M.G.L.")
    ));
}

#[tokio::test]
async fn qi_gives_no_reading_until_the_required_questions_are_answered() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "qi_vs_research_determination",
        json!({"answers": [ans("local_improvement", "yes"), ans("generalizable_intent", "unsure")],
               "write_document": false}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["status"], "needs_answers");
    assert!(d["classification"].is_null());
    assert_eq!(
        d["needs_answers"],
        json!(["generalizable_intent", "new_intervention", "extra_burden"])
    );
    assert_eq!(d["unsure"], json!(["generalizable_intent"]));
    assert!(d["document_file"].is_null());
    assert!(s["summary"].as_str().unwrap().contains("No reading yet"));
}

#[tokio::test]
async fn qi_readings_follow_the_answers_and_show_the_reasoning() {
    let h = start(|c| c.offline = true).await;
    let no3 = [
        ans("generalizable_intent", "no"),
        ans("new_intervention", "no"),
        ans("extra_burden", "no"),
    ];
    // Quality improvement.
    let mut a = no3.to_vec();
    a.push(ans("local_improvement", "yes"));
    let s = call_ok(
        &h,
        "qi_vs_research_determination",
        json!({"answers": a, "project_title": "Clinic reminder process", "reviewing_office": "Our human research protection office"}),
    )
    .await;
    let cl = &s["data"]["classification"];
    assert_eq!(s["data"]["status"], "provisional");
    assert_eq!(cl["category"], "qi");
    assert!(
        cl["not_a_determination"]
            .as_str()
            .unwrap()
            .contains("IRB decides")
    );
    assert!(s["summary"].as_str().unwrap().contains("not a"));
    let text = docx_text(&h, s["data"]["document_file"].as_str().unwrap());
    assert!(text.contains("Clinic reminder process"));
    assert!(text.contains("Our human research protection office"));
    assert!(text.contains("Provisional reading from these answers"));
    // Possible research with the human subject reading and the overlap caution.
    let mut a = no3.to_vec();
    a.retain(|x| x["question_id"] != "generalizable_intent");
    a.push(ans("generalizable_intent", "yes"));
    a.push(ans("local_improvement", "yes"));
    a.push(ans("identifiable_private_information", "yes"));
    a.push(
        json!({"question_id": "publication_plan", "answer": "yes", "note": "conference poster"}),
    );
    let s = call_ok(
        &h,
        "qi_vs_research_determination",
        json!({"answers": a, "write_document": false}),
    )
    .await;
    let cl = &s["data"]["classification"];
    assert_eq!(cl["category"], "possible_research");
    assert_eq!(cl["human_subjects"]["status"], "likely");
    assert_eq!(cl["research_signals"], json!(["generalizable_intent"]));
    assert!(
        cl["cautions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("overlap category"))
    );
    let steps = s["data"]["next_steps"].as_array().unwrap();
    assert!(
        steps
            .iter()
            .any(|x| x.as_str().unwrap().contains("244 CMR 9.03(18)"))
    );
    let publication = s["data"]["reasoning"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == "publication_plan")
        .unwrap();
    assert_eq!(publication["note"], "conference poster");
    assert_eq!(publication["effect"], "informational");
    // Public health surveillance under authority.
    let mut a = no3.to_vec();
    a.push(ans("public_health_authority", "yes"));
    a.push(ans("beyond_reporting", "no"));
    let s = call_ok(
        &h,
        "qi_vs_research_determination",
        json!({"answers": a, "write_document": false}),
    )
    .await;
    assert_eq!(
        s["data"]["classification"]["category"],
        "public_health_practice"
    );
    // Program evaluation.
    let mut a = no3.to_vec();
    a.push(ans("established_program", "yes"));
    let s = call_ok(
        &h,
        "qi_vs_research_determination",
        json!({"answers": a, "write_document": false}),
    )
    .await;
    assert_eq!(
        s["data"]["classification"]["category"],
        "program_evaluation"
    );
}

#[tokio::test]
async fn qi_errors_are_tool_errors_and_identifiers_are_not_echoed() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        "qi_vs_research_determination",
        json!({"answers": [ans("nonsense", "yes")]}),
    )
    .await;
    assert!(e.contains("not a question id") && e.contains("generalizable_intent"));
    let e = call_err(
        &h,
        "qi_vs_research_determination",
        json!({"answers": [ans("extra_burden", "yes"), ans("extra_burden", "no")]}),
    )
    .await;
    assert!(e.contains("more than once"));
    for (field, text) in [
        (
            "description",
            "Clients live at 837 Elm Street and call 413-555-0100",
        ),
        ("project_title", "Review of 929-87-6117 charts"),
    ] {
        let e = call_err(&h, "qi_vs_research_determination", json!({ field: text })).await;
        assert!(e.contains("privacy guardrail"), "{e}");
        assert!(
            !e.contains("Elm Street") && !e.contains("413-555-0100") && !e.contains("929-87-6117")
        );
    }
    let e = call_err(
        &h,
        "qi_vs_research_determination",
        json!({"answers": [{"question_id": "extra_burden", "answer": "yes", "note": "call maria.garcia@example.org"}]}),
    )
    .await;
    assert!(!e.contains("maria.garcia"));
    let e = call_err(
        &h,
        "qi_vs_research_determination",
        json!({"description": "x".repeat(2001)}),
    )
    .await;
    assert!(e.contains("too long"));
}

// ---------------------------------------------------------------- POL-05

#[tokio::test]
async fn ethics_analysis_structures_the_questions_and_never_recommends() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "public_health_ethics_analysis",
        json!({"decision_type": "exclusion from school",
               "decision": "Whether to exclude unimmunized students during a measles outbreak",
               "situation": "Two linked cases in an elementary school; 94 percent coverage",
               "responses": [{"question": "1", "response": "Stop spread to infants and immunocompromised students."},
                             {"question": "effectiveness", "response": "Exclusion for 21 days is the standard control measure."}],
               "alternatives": ["Offer a school-based vaccination clinic first"]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["profile"]["id"], "school_exclusion");
    assert_eq!(d["profile"]["matched"], true);
    let q = d["kass_questions"].as_array().unwrap();
    assert_eq!(q.len(), 6);
    assert_eq!(q[0]["status"], "answered");
    assert_eq!(q[1]["status"], "answered");
    assert_eq!(d["open_questions"], json!([3, 4, 5, 6]));
    let table = d["least_restrictive_table"].as_array().unwrap();
    assert_eq!(table.first().unwrap()["level"], 1);
    let last = table.last().unwrap();
    assert_eq!(last["origin"], "your option");
    assert!(last["level"].is_null());
    let levels: Vec<u64> = table.iter().filter_map(|r| r["level"].as_u64()).collect();
    assert!(levels.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(d["apha_core_values"].as_array().unwrap().len(), 6);
    assert!(d["justification_checklist"].as_array().unwrap().len() >= 6);
    assert!(
        d["no_recommendation"]
            .as_str()
            .unwrap()
            .contains("does not recommend")
    );
    assert!(!d["sources_not_read"].as_array().unwrap().is_empty());
    let c = caveats(&s);
    assert!(c.contains("could not be read") && c.contains("copyrighted"));
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    let text = docx_text(&h, d["document_file"].as_str().unwrap());
    assert!(text.contains("Whether to exclude unimmunized students"));
    assert!(text.contains("Offer a school-based vaccination clinic first"));
    assert!(text.contains("[NURSE:"));
    assert!(text.contains("Stop spread to infants"));
    // No recommendation language in the answer itself.
    let sum = s["summary"].as_str().unwrap().to_lowercase();
    assert!(!sum.contains("we recommend") && !sum.contains("you should exclude"));
}

#[tokio::test]
async fn ethics_analysis_handles_other_decisions_and_bad_input() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "public_health_ethics_analysis",
        json!({"decision_type": "a mandatory flu shot for staff", "write_document": false}),
    )
    .await;
    assert_eq!(s["data"]["profile"]["id"], "general");
    assert_eq!(s["data"]["profile"]["matched"], false);
    assert!(caveats(&s).contains("No specific profile matched"));
    assert!(s["data"]["document_file"].is_null());
    assert_eq!(s["data"]["open_questions"], json!([1, 2, 3, 4, 5, 6]));
    let s = call_ok(
        &h,
        "public_health_ethics_analysis",
        json!({"decision_type": "allocation of scarce vaccine", "write_document": false}),
    )
    .await;
    assert_eq!(s["data"]["profile"]["id"], "scarce_resource_allocation");
    let e = call_err(
        &h,
        "public_health_ethics_analysis",
        json!({"responses": [{"question": "9", "response": "x"}]}),
    )
    .await;
    assert!(e.contains("six questions"));
    let e = call_err(
        &h,
        "public_health_ethics_analysis",
        json!({"responses": [{"question": "1", "response": "a"}, {"question": "goals", "response": "b"}]}),
    )
    .await;
    assert!(e.contains("more than one response"));
    let e = call_err(
        &h,
        "public_health_ethics_analysis",
        json!({"situation": "The family at 837 Elm Street, phone 413-555-0100"}),
    )
    .await;
    assert!(e.contains("privacy guardrail") && !e.contains("Elm Street"));
    let e = call_err(
        &h,
        "public_health_ethics_analysis",
        json!({"alternatives": (0..13).map(|i| format!("option {i}")).collect::<Vec<_>>()}),
    )
    .await;
    assert!(e.contains("at most 12"));
}

// ---------------------------------------------------------------- POL-06

#[tokio::test]
async fn scope_guide_cites_the_rules_and_refers_to_the_board() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "nursing_scope_of_practice_guide",
        json!({"question": "Can an LPN give flu vaccines at a clinic under a standing order?"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["answer_type"], "citations_and_decision_questions");
    let ids: Vec<&str> = d["matched_topics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"vaccines") && ids.contains(&"standing_orders"),
        "{ids:?}"
    );
    let prov = d["provisions"].as_array().unwrap();
    let cites: Vec<&str> = prov
        .iter()
        .map(|p| p["citation"].as_str().unwrap())
        .collect();
    assert!(
        cites.contains(&"Advisory Ruling 9804") && cites.contains(&"Advisory Ruling 9324"),
        "{cites:?}"
    );
    for p in prov {
        assert!(p["quote"].as_str().unwrap().split_whitespace().count() <= 25);
        assert!(p["url"].as_str().unwrap().starts_with("https://"));
        assert!(p["source"].as_str().unwrap().len() > 10);
    }
    assert_eq!(
        d["decision_guide"]["questions"].as_array().unwrap().len(),
        6
    );
    assert_eq!(
        d["decision_guide"]["framework_steps"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    let rulings = d["advisory_rulings"].as_array().unwrap();
    assert!(rulings.iter().any(|r| r["number"] == "AR 9804"
        && r["read"] == true
        && r["last_revised"] == "2015-09-09"));
    // The coverage says the guide is partial; the answer never says an act is in scope.
    assert_eq!(d["coverage"]["partial"], true);
    assert!(
        d["coverage"]["rulings_listed"].as_u64().unwrap()
            > d["coverage"]["rulings_read"].as_u64().unwrap()
    );
    assert!(
        d["refer_to_board"]["contacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("973-0800"))
    );
    let c = caveats(&s);
    assert!(c.contains("PARTIAL") && c.contains("Decision support, not legal advice"));
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    let sum = s["summary"].as_str().unwrap();
    assert!(sum.contains("does not say that a particular"));
    assert!(sum.contains("Ask the Board"));
    assert!(!sum.contains("is within your scope"));
}

#[tokio::test]
async fn scope_guide_filters_by_role_topic_and_lists_titles_it_did_not_read() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "nursing_scope_of_practice_guide",
        json!({"topics": ["aprn"], "role": "lpn"}),
    )
    .await;
    // No APRN-only provision may come back for an LPN.
    assert!(s["data"]["provisions"].as_array().unwrap().iter().all(|p| {
        p["applies_to"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "lpn")
    }));
    let s = call_ok(
        &h,
        "nursing_scope_of_practice_guide",
        json!({"topics": ["delegation"], "max_results": 3}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["provisions"].as_array().unwrap().len(), 3);
    assert!(d["more_provisions"].as_u64().unwrap() > 0);
    let s = call_ok(
        &h,
        "nursing_scope_of_practice_guide",
        json!({"question": "Can a nurse do infusion therapy?"}),
    )
    .await;
    let rulings = s["data"]["advisory_rulings"].as_array().unwrap();
    let infusion = rulings
        .iter()
        .find(|r| r["number"] == "AR 9204")
        .expect("infusion ruling listed by title");
    assert_eq!(infusion["read"], false);
    assert!(infusion["issued"].is_null());
    assert!(s["data"]["provisions"].as_array().unwrap().is_empty());
    assert!(caveats(&s).contains("No provision in the snapshot matched"));
    assert!(!s["data"]["topic_index"].as_array().unwrap().is_empty());
    let s = call_ok(
        &h,
        "nursing_scope_of_practice_guide",
        json!({"list_all_rulings": true}),
    )
    .await;
    assert!(s["data"]["advisory_rulings"].as_array().unwrap().len() == 34);
    // With nothing asked, the topic index is the answer.
    let s = call_ok(&h, "nursing_scope_of_practice_guide", json!({})).await;
    assert!(s["data"]["provisions"].as_array().unwrap().is_empty());
    assert_eq!(s["data"]["topic_index"].as_array().unwrap().len(), 15);
    let e = call_err(
        &h,
        "nursing_scope_of_practice_guide",
        json!({"question": "Call 413-555-0100 about it"}),
    )
    .await;
    assert!(e.contains("privacy guardrail") && !e.contains("413-555-0100"));
    assert!(
        leaks(
            &everything(
                &h.call(
                    "nursing_scope_of_practice_guide",
                    json!({"topics": ["research"]})
                )
                .await
            ),
            "data/line_list.csv",
            None
        )
        .is_empty()
    );
}
