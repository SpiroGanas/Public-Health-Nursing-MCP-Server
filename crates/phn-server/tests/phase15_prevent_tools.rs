//! MCP integration tests for the Phase 15 preventive-services and asthma tools
//! (CHR-04, CHR-05): offline calls succeed and match their output schemas, every
//! recommendation carries a grade and a date, the content-pending caveat is
//! shown, problems the nurse can fix are tool errors, blank forms land in the
//! output folder, and asthma plans for people are filled only inside the files
//! in /data/output.

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

fn ids(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

fn output_file(h: &common::Harness, display: &str) -> std::path::PathBuf {
    h.data().join(display)
}

fn docx_text(h: &common::Harness, display: &str) -> String {
    let p = output_file(h, display);
    assert!(p.exists(), "{display} was not written");
    phn_local::docx::read_docx(&p)
        .expect("readable DOCX")
        .plain_text()
}

const LOOKUP: &str = "preventive_services_lookup";
const ASTHMA: &str = "asthma_home_assessment";

#[tokio::test]
async fn former_smoker_gets_dated_graded_recommendations_offline() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        LOOKUP,
        json!({"age_years": 52, "sex_at_birth": "female",
               "risk_factors": {"tobacco_use": "former", "pack_years": 25, "years_since_quit": 8,
                                "bmi": 31.0}}),
    )
    .await;
    let d = &s["data"];
    let applies = ids(&d["applies"]);
    for want in [
        "lung_cancer_screening",
        "colorectal_cancer_screening_50_75",
        "breast_cancer_screening",
        "cervical_cancer_screening_30_65",
        "hypertension_screening_adults",
        "hepatitis_c_screening",
        "prediabetes_diabetes_screening",
        "obesity_behavioral_intervention_adults",
    ] {
        assert!(applies.contains(&want.to_string()), "{want} in {applies:?}");
    }
    // Not for this person: 45 to 49, men's AAA screening, pregnancy items.
    for not in [
        "colorectal_cancer_screening_45_49",
        "aaa_screening_men_65_75_ever_smoked",
        "syphilis_screening_pregnancy",
    ] {
        assert!(!applies.contains(&not.to_string()), "{not}");
    }
    // Every recommendation has a grade of A or B, a statement date, the Task Force's wording,
    // a client summary, and a DOI link.
    for r in d["applies"].as_array().unwrap() {
        let grade = r["grade"].as_str().unwrap();
        assert!(grade == "A" || grade == "B", "{r}");
        let date = r["published"].as_str().unwrap();
        assert!(date.len() == 10 && date >= "2016-01-01" && date <= "2026-12-31", "{date}");
        assert!(r["recommendation"].as_str().unwrap().contains("recommendation)"));
        assert!(!r["client_summary"].as_str().unwrap().is_empty());
        assert!(r["url"].as_str().unwrap().starts_with("https://doi.org/10."));
        assert!(!r["confirm"].as_str().unwrap().is_empty());
    }
    let lung = d["applies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "lung_cancer_screening")
        .unwrap();
    assert_eq!(lung["grade"], "B");
    assert_eq!(lung["published"], "2021-03-09");
    assert!(
        lung["population"]
            .as_str()
            .unwrap()
            .contains("20 pack-year")
    );
    // Statins fit on age but need the cardiovascular risk information: needs review, with the
    // missing input named.
    let statin = d["needs_review"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "statin_primary_prevention")
        .expect("statin needs review");
    assert!(
        statin["missing_information"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m.as_str().unwrap().contains("ten_year_cvd_risk_percent"))
    );
    // Massachusetts options.
    let programs = ids(&d["massachusetts_programs"]);
    assert!(programs.contains(&"mbccp".to_string()));
    assert!(programs.contains(&"mcrccp".to_string()));
    assert!(programs.contains(&"mtcp_quitline".to_string()));
    let mbccp = d["massachusetts_programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "mbccp")
        .unwrap();
    assert!(mbccp["contact"].as_str().unwrap().contains("(877) 414-4447"));
    assert!(mbccp["eligibility"].as_str().unwrap().contains("250%"));
    // Offline: MyHealthfinder was not asked, and the answer says so.
    assert!(d["myhealthfinder"].is_null());
    let c = caveats(&s);
    assert!(c.contains("MyHealthfinder"));
    assert!(c.contains("grade A and B list only"));
    assert!(c.contains("could not be reached"));
    assert!(pending(&s));
    assert!(s["notice"].as_str().is_some());
    // The snapshot is cited with its date and mode.
    assert!(s["sources"].as_array().unwrap().iter().any(|x| {
        x["mode"] == "snapshot"
            && x["title"].as_str().unwrap().contains("USPSTF A and B")
            && x["retrieved"].as_str().is_some()
    }));
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert!(s["privacy"]["files_written"].is_null() || s["privacy"]["files_written"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn pregnancy_status_decides_the_pregnancy_recommendations() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        LOOKUP,
        json!({"age_years": 29, "sex_at_birth": "female", "pregnancy_status": "pregnant",
               "risk_factors": {"tobacco_use": "current",
                                "increased_risk_for": ["preeclampsia"]}}),
    )
    .await;
    let a = ids(&s["data"]["applies"]);
    for want in [
        "hepatitis_b_screening_pregnancy",
        "syphilis_screening_pregnancy",
        "hiv_screening_pregnancy",
        "gestational_diabetes_screening",
        "preeclampsia_aspirin",
        "tobacco_cessation_pregnancy",
        "folic_acid_supplementation",
    ] {
        assert!(a.contains(&want.to_string()), "{want} in {a:?}");
    }
    // Recommendations for non-pregnant adults do not apply.
    assert!(!a.contains(&"tobacco_cessation_adults".to_string()));
    assert!(!a.contains(&"prediabetes_diabetes_screening".to_string()));
    assert_eq!(s["data"]["person"]["pregnancy"], "pregnant");
    assert!(s["data"]["pregnancy_specific_not_evaluated"].as_array().unwrap().is_empty());

    // Pregnancy status left out: treated as not pregnant, and the tool says how many pregnancy
    // recommendations it did not evaluate.
    let s = call_ok(&h, LOOKUP, json!({"age_years": 29, "sex_at_birth": "female"})).await;
    let n = s["data"]["pregnancy_specific_not_evaluated"].as_array().unwrap().len();
    assert!(n >= 8, "{n}");
    assert!(caveats(&s).contains("Pregnancy status was not given"));
    assert!(
        s["data"]["person"]["pregnancy"]
            .as_str()
            .unwrap()
            .contains("assumed")
    );
}

#[tokio::test]
async fn missing_risk_factors_move_a_recommendation_to_needs_review_never_drop_it() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, LOOKUP, json!({"age_years": 70, "sex_at_birth": "male"})).await;
    let d = &s["data"];
    let review = ids(&d["needs_review"]);
    for want in ["lung_cancer_screening", "aaa_screening_men_65_75_ever_smoked", "falls_prevention_exercise"] {
        assert!(review.contains(&want.to_string()), "{want} in {review:?}");
    }
    // Saying he never smoked removes lung and AAA screening (not matched), not falls.
    let s = call_ok(
        &h,
        LOOKUP,
        json!({"age_years": 70, "sex_at_birth": "male",
               "risk_factors": {"tobacco_use": "never"}}),
    )
    .await;
    let review = ids(&s["data"]["needs_review"]);
    let applies = ids(&s["data"]["applies"]);
    assert!(!review.contains(&"lung_cancer_screening".to_string()));
    assert!(!applies.contains(&"lung_cancer_screening".to_string()));
    assert!(!review.contains(&"aaa_screening_men_65_75_ever_smoked".to_string()));
    assert!(review.contains(&"falls_prevention_exercise".to_string()));
    // Women's screening is not listed for a man.
    assert!(!applies.contains(&"breast_cancer_screening".to_string()));
}

#[tokio::test]
async fn handout_is_written_without_identifiers_and_checked_for_reading_grade() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        LOOKUP,
        json!({"age_years": 47, "sex_at_birth": "female", "write_handout": true,
               "risk_factors": {"tobacco_use": "never"}}),
    )
    .await;
    let d = &s["data"];
    let f = d["handout_file"].as_str().expect("handout file");
    let text = docx_text(&h, f);
    assert!(text.contains("mammogram"));
    assert!(text.contains("A and B recommendations"));
    assert!(d["handout_reading_grade"].as_f64().unwrap() > 0.0);
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 1);
    let found = leaks(&everything(&h.call(LOOKUP, json!({"age_years": 47, "sex_at_birth": "female"})).await), "data/line_list.csv", None);
    assert!(found.is_empty(), "{found:?}");
}

#[tokio::test]
async fn preventive_lookup_input_errors_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, LOOKUP, json!({"sex_at_birth": "female"})).await;
    assert!(e.contains("age_years"), "{e}");
    let e = call_err(&h, LOOKUP, json!({"age_years": 40})).await;
    assert!(e.contains("sex_at_birth"), "{e}");
    let e = call_err(&h, LOOKUP, json!({"age_years": 130, "sex_at_birth": "male"})).await;
    assert!(e.contains("age_years"), "{e}");
    let e = call_err(
        &h,
        LOOKUP,
        json!({"age_years": 30, "sex_at_birth": "male", "pregnancy_status": "pregnant"}),
    )
    .await;
    assert!(e.contains("sex_at_birth female"), "{e}");
    let e = call_err(
        &h,
        LOOKUP,
        json!({"age_years": 30, "sex_at_birth": "female",
               "risk_factors": {"bmi": 1000}}),
    )
    .await;
    assert!(e.contains("bmi"), "{e}");
    let e = call_err(
        &h,
        LOOKUP,
        json!({"age_years": 30, "sex_at_birth": "female",
               "risk_factors": {"increased_risk_for": ["hiv"], "not_at_increased_risk_for": ["hiv"]}}),
    )
    .await;
    assert!(e.contains("both"), "{e}");
}

#[tokio::test]
async fn asthma_checklist_points_at_the_housing_content_and_writes_blank_forms() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, ASTHMA, json!({"town": "Springfield"})).await;
    let d = &s["data"];
    let modules: Vec<String> = d["checklist"]["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(modules, vec!["core", "dust_mites", "mold_moisture"]);
    // The 105 CMR 410 sections come from the Phase 14 housing content.
    let refs = d["housing_reference"].as_array().unwrap();
    let mold = refs.iter().find(|r| r["condition_id"] == "moisture_mold").unwrap();
    assert!(
        mold["code"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["section"].as_str().unwrap().starts_with("410.500"))
    );
    let pests = refs.iter().find(|r| r["condition_id"] == "pests").unwrap();
    assert_eq!(pests["urgent"], true);
    // Both content files are drafts: the pending caveat appears for each.
    let c = caveats(&s);
    assert!(c.contains("Asthma home assessment"));
    assert!(c.contains("Housing conditions and health"));
    assert!(pending(&s));
    // The ACT is not reproduced and the response says why.
    assert!(d["control"]["act_note"].as_str().unwrap().contains("licensed"));
    assert!(c.contains("Asthma Control Test is licensed"));
    // Blank forms in the output folder.
    let docs: Vec<&str> = d["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert_eq!(docs.len(), 2);
    let checklist = docx_text(&h, docs[0]);
    assert!(checklist.contains("Visit date"));
    assert!(checklist.contains("cockroaches"));
    let plan = docx_text(&h, docs[1]);
    assert!(plan.contains("Green zone"));
    assert!(plan.contains("Red zone"));
    assert!(plan.contains("____"));
    // No medicine names are written into the blank plan.
    assert!(!plan.to_lowercase().contains("albuterol"));
    // Sources are dated.
    assert!(s["sources"].as_array().unwrap().iter().any(|x| {
        x["title"].as_str().unwrap().contains("Checklist for Home Visitors")
            && x["effective_date"] == "2018-06-01"
            && x["retrieved"] == "2026-11-12"
    }));
    // Services: Revitalize CDC names Springfield; programs carry DPH's regions.
    let v = &d["services"];
    assert_eq!(v["town"], "Springfield");
    let programs = v["programs"].as_array().unwrap();
    assert_eq!(programs.len(), 7);
    assert_eq!(programs[0]["name"], "Revitalize CDC");
    assert_eq!(programs[0]["fit_for_town"], "names your town");
    assert_eq!(programs[1]["name"], "Caring Health Center");
    assert_eq!(programs[1]["fit_for_town"], "located in your town");
    assert!(programs.last().unwrap()["fit_for_town"].as_str().unwrap().starts_with("regional"));
    assert!(v["directory_updated_by_dph"].as_str().unwrap().contains("2025"));
    assert!(!v["not_verified"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn asthma_control_answers_are_classified_by_the_most_severe_item() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        ASTHMA,
        json!({"sections": ["control"], "write_documents": false,
               "control_answers": {"symptom_days_past_30": 12, "nights_waking_past_30": 4,
                                   "rescue_uses_per_day": 0.2, "age_years": 9}}),
    )
    .await;
    let k = &s["data"]["control"]["classification"];
    assert_eq!(k["overall"], "not well controlled");
    assert_eq!(k["items"].as_array().unwrap().len(), 3);
    assert!(k["not_answered"].as_array().unwrap().iter().any(|x| x == "interference"));
    assert!(k["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("under 12")));
    assert!(s["data"]["checklist"].is_null());
    assert!(s["data"]["documents"].as_array().unwrap().is_empty());
    let s = call_ok(
        &h,
        ASTHMA,
        json!({"sections": ["control"], "write_documents": false,
               "control_answers": {"nights_waking_past_30": 13}}),
    )
    .await;
    assert_eq!(s["data"]["control"]["classification"]["overall"], "very poorly controlled");
}

#[tokio::test]
async fn asthma_input_errors_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(&h, ASTHMA, json!({"town": "Springfeld"})).await;
    assert!(e.contains("Springfield"), "{e}");
    let e = call_err(&h, ASTHMA, json!({"sections": ["checklist"], "control_answers": {"symptom_days_past_30": 3}})).await;
    assert!(e.contains("control"), "{e}");
    let e = call_err(&h, ASTHMA, json!({"sections": ["control"], "control_answers": {}})).await;
    assert!(e.contains("no answers"), "{e}");
    let e = call_err(&h, ASTHMA, json!({"sections": ["control"], "control_answers": {"symptom_days_past_30": 40}})).await;
    assert!(e.contains("between 0 and 30"), "{e}");
    let e = call_err(&h, ASTHMA, json!({"sections": ["services"], "clients_path": "asthma_plans.csv"})).await;
    assert!(e.contains("action_plan"), "{e}");
    let e = call_err(&h, ASTHMA, json!({"sections": ["action_plan"], "clients_path": "counts_by_town.csv"})).await;
    assert!(e.contains("no column the asthma action plan can use"), "{e}");
    let e = call_err(&h, ASTHMA, json!({"sections": ["action_plan"], "clients_path": "no_such_file.csv"})).await;
    assert!(!e.is_empty());
}

#[tokio::test]
async fn action_plans_are_filled_locally_and_never_returned() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            ASTHMA,
            json!({"sections": ["action_plan"], "clients_path": "asthma_plans.csv",
                   "write_documents": false}),
        )
        .await;
    assert_eq!(r.is_error, Some(false), "{}", everything(&r));
    let found = leaks(&everything(&r), "data/asthma_plans.csv", None);
    assert!(found.is_empty(), "returned identifiers: {found:?}");
    let s = r.structured_content.clone().unwrap();
    let p = &s["data"]["filled_plans"];
    assert_eq!(p["people"], 3);
    assert!(p["blank_fields"].as_u64().unwrap() > 0);
    assert!(p["fields_found"].as_array().unwrap().iter().any(|f| f == "best_peak_flow"));
    assert!(s["privacy"]["identifiers_detected"].as_array().map(|a| !a.is_empty()).unwrap_or(false));
    assert_eq!(s["privacy"]["row_level_data"], false);
    // The plans are in the output folder with the details from the file, the peak flow zones
    // worked out with NHLBI's percentages, and a blank where the file had no value.
    let text = docx_text(&h, p["file"].as_str().unwrap());
    assert!(text.contains("Odalys Fenwickle"));
    assert!(text.contains("Example rescue inhaler"));
    assert!(text.contains("256 or more"), "{text}");
    assert!(text.contains("160 to 255"));
    assert!(text.contains("less than 160"));
    assert!(text.contains("360 or more"));
    // The second person has no best peak flow and no oral steroid: blanks, not invented values.
    assert!(text.contains("____"));
    // The server never writes a medicine or dose that is not in the file.
    assert!(!text.to_lowercase().contains("prednisone"));
}

#[tokio::test]
async fn person_files_without_plan_columns_are_refused_without_echoing_values() {
    let h = start(|c| c.offline = true).await;
    let r = h
        .call(
            ASTHMA,
            json!({"sections": ["action_plan"], "clients_path": "tb_contacts.csv",
                   "write_documents": false}),
        )
        .await;
    let out = everything(&r);
    let found = leaks(&out, "data/tb_contacts.csv", None);
    assert!(found.is_empty(), "{found:?}");
}
