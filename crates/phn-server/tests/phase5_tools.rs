//! MCP integration tests for the Phase 5 tools (immunizations I): successful
//! calls validate against each tool's output schema, problems the nurse can
//! fix are tool-level errors, clinical answers carry the decision-support
//! notice and review caveats, files land in the output folder, and the school
//! roster check returns suppressed counts only.

mod common;

use std::collections::BTreeSet;

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

fn decision_support(s: &Value) -> bool {
    s["notice"]
        .as_str()
        .is_some_and(|d| d.contains(DECISION_SUPPORT))
}

// ------------------------------------------------------------------ VAX-01

#[tokio::test]
async fn schedule_lists_each_body_with_editions_and_minimums() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "get_immunization_schedule", json!({})).await;
    let vaccines = s["data"]["vaccines"].as_array().unwrap();
    assert!(vaccines.len() >= 15, "overview lists every vaccine");
    assert!(!s["data"]["editions"].as_array().unwrap().is_empty());
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));

    let s = call_ok(
        &h,
        "get_immunization_schedule",
        json!({"vaccine": "MMR", "include_contraindications": true}),
    )
    .await;
    let v = &s["data"]["vaccines"][0];
    assert_eq!(v["id"], "mmr");
    let doses = v["minimums"][0]["doses"].as_array().unwrap();
    assert_eq!(doses[0]["min_age"], "12 months");
    assert!(
        v["contraindications"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("Pregnancy"))
    );
    // Default basis: DPH, then the professional society, then CDC/ACIP.
    let bodies: Vec<&str> = v["routine"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies.last(), Some(&"cdc_acip"), "{bodies:?}");

    // Switching the saved basis to CDC/ACIP puts the federal position first.
    let c = call_ok(
        &h,
        "configure_jurisdiction",
        json!({"vaccine_recommendation_basis": "cdc_acip"}),
    )
    .await;
    assert!(md(&c).contains("Vaccine recommendation basis: cdc_acip"));
    let s = call_ok(&h, "get_immunization_schedule", json!({"vaccine": "MMR"})).await;
    assert_eq!(s["data"]["vaccines"][0]["routine"][0]["body"], "cdc_acip");
    assert_eq!(s["data"]["basis"], "cdc_acip");
    // And "default" clears it.
    call_ok(
        &h,
        "configure_jurisdiction",
        json!({"vaccine_recommendation_basis": "default"}),
    )
    .await;
    let s = call_ok(&h, "get_immunization_schedule", json!({"vaccine": "MMR"})).await;
    assert_eq!(s["data"]["basis"], "dph_then_professional_then_cdc");

    let s = call_ok(
        &h,
        "get_immunization_schedule",
        json!({"vaccine": "Shingrix", "population": "adult"}),
    )
    .await;
    assert_eq!(s["data"]["population"], "adults");

    call_err(&h, "get_immunization_schedule", json!({"vaccine": "zzzz"})).await;
    call_err(
        &h,
        "get_immunization_schedule",
        json!({"population": "toddlers and pets"}),
    )
    .await;
    call_err(
        &h,
        "configure_jurisdiction",
        json!({"vaccine_recommendation_basis": "whatever"}),
    )
    .await;
}

// ------------------------------------------------------------------ VAX-02

#[tokio::test]
async fn compare_shows_every_body_and_highlights_differences() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "compare_vaccine_recommendations", json!({})).await;
    let topics = s["data"]["topics"].as_array().unwrap();
    assert!(topics.len() >= 10);
    assert!(topics.iter().any(|t| t["differs"] == true));

    let s = call_ok(
        &h,
        "compare_vaccine_recommendations",
        json!({"query": "hepatitis B birth dose"}),
    )
    .await;
    let t = &s["data"]["topics"][0];
    assert_eq!(t["id"], "hepb_infant");
    assert_eq!(t["differs"], true);
    assert_eq!(t["basis_body"], "dph");
    let bodies: BTreeSet<&str> = t["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["body"].as_str().unwrap())
        .collect();
    assert!(bodies.contains("dph") && bodies.contains("aap") && bodies.contains("cdc_acip"));
    for p in t["positions"].as_array().unwrap() {
        assert!(p["date"].is_string(), "each position carries its date: {p}");
        assert!(p["source"]["url"].as_str().unwrap().starts_with("https://"));
    }
    assert!(md(&s).contains("differ"));
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));

    // A per-call override: show_all names no single basis.
    let s = call_ok(
        &h,
        "compare_vaccine_recommendations",
        json!({"query": "hepatitis B birth dose", "basis": "show_all"}),
    )
    .await;
    assert!(s["data"]["topics"][0]["basis_body"].is_null());

    // Population filter.
    let s = call_ok(
        &h,
        "compare_vaccine_recommendations",
        json!({"population": "pregnancy"}),
    )
    .await;
    assert!(
        s["data"]["topics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["population"] == "pregnancy" || t["population"] == "all")
    );

    call_err(
        &h,
        "compare_vaccine_recommendations",
        json!({"population": "pets"}),
    )
    .await;
    call_err(
        &h,
        "compare_vaccine_recommendations",
        json!({"query": "hep B", "basis": "astrology"}),
    )
    .await;
}

// ------------------------------------------------------------------ VAX-04

fn kindergartner(dtap: &[&str]) -> Value {
    json!({
        "grade": "K",
        "date_of_birth": "2020-03-15",
        "as_of": "2026-09-01",
        "doses": {
            "dtap": dtap,
            "polio": ["2020-05-15", "2020-07-15", "2021-03-15", "2024-03-20"],
            "hepb": ["2020-03-16", "2020-05-15", "2020-09-15"],
            "mmr": ["2021-03-20", "2024-03-20"],
            "varicella": ["2021-03-20", "2024-03-20"]
        }
    })
}

fn status_of<'a>(s: &'a Value, label: &str) -> &'a Value {
    s["data"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["label"].as_str().unwrap().starts_with(label))
        .unwrap_or_else(|| panic!("no requirement {label}: {}", s["data"]))
}

#[tokio::test]
async fn school_check_one_student() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "check_ma_school_immunization_requirements",
        kindergartner(&[
            "2020-05-15",
            "2020-07-15",
            "2020-09-15",
            "2021-06-15",
            "2024-03-20",
        ]),
    )
    .await;
    for r in s["data"]["requirements"].as_array().unwrap() {
        assert_eq!(r["status"], "met", "{r}");
    }
    assert_eq!(s["data"]["setting"], "k_6");
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));
    // The date of birth is used but not repeated.
    assert!(!everything_json(&s).contains("2020-03-15"));

    // Fifth DTaP 10 days before the 4th birthday does not count; the earliest
    // valid repeat is the 4th birthday, which has passed.
    let s = call_ok(
        &h,
        "check_ma_school_immunization_requirements",
        kindergartner(&[
            "2020-05-15",
            "2020-07-15",
            "2020-09-15",
            "2021-06-15",
            "2024-03-05",
        ]),
    )
    .await;
    let d = status_of(&s, "DTaP");
    assert_eq!(d["status"], "not_met");
    assert_eq!(d["invalid_doses"][0]["date"], "2024-03-05");
    assert_eq!(d["eligible_now"], true);
    assert!(md(&s).contains("Doses that do not count"));

    // Four days early counts under the grace period; not when it is turned off.
    let early = kindergartner(&[
        "2020-05-15",
        "2020-07-15",
        "2020-09-15",
        "2021-06-15",
        "2024-03-11",
    ]);
    let s = call_ok(
        &h,
        "check_ma_school_immunization_requirements",
        early.clone(),
    )
    .await;
    assert_eq!(status_of(&s, "DTaP")["status"], "met");
    let mut strict = early;
    strict["apply_grace_period"] = json!(false);
    let s = call_ok(&h, "check_ma_school_immunization_requirements", strict).await;
    assert_eq!(status_of(&s, "DTaP")["status"], "not_met");
    assert_eq!(s["data"]["grace_period_applied"], false);

    // Exemptions are applied after doses and proof of immunity.
    let s = call_ok(
        &h,
        "check_ma_school_immunization_requirements",
        json!({"grade": "7", "date_of_birth": "2013-10-11", "as_of": "2026-09-02",
               "doses": {"mmr": ["2014-10-14", "2017-11-10"]},
               "varicella_history": true,
               "exemptions": [{"vaccine": "menacwy", "kind": "religious"}]}),
    )
    .await;
    assert_eq!(status_of(&s, "Varicella")["status"], "met");
    assert_eq!(status_of(&s, "Meningococcal")["status"], "exempt_religious");
    assert_eq!(status_of(&s, "Tdap")["status"], "not_met");

    for bad in [
        json!({"grade": "K"}),
        json!({"grade": "K", "date_of_birth": "15/03/2020"}),
        json!({"grade": "kindergarten-ish", "date_of_birth": "2020-03-15"}),
        json!({"grade": "K", "date_of_birth": "2020-03-15", "as_of": "2026-09-01",
               "doses": {"mmr": ["2019-01-01"]}}),
        json!({"grade": "K", "date_of_birth": "2020-03-15", "as_of": "2026-09-01",
               "exemptions": [{"vaccine": "mmr", "kind": "philosophical"}]}),
    ] {
        call_err(&h, "check_ma_school_immunization_requirements", bad).await;
    }
}

fn everything_json(s: &Value) -> String {
    s.to_string()
}

#[tokio::test]
async fn school_roster_check_returns_suppressed_counts_and_writes_details_to_output() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "check_ma_school_immunization_requirements",
        json!({"roster_path": "school_roster.csv", "as_of": "2026-09-02"}),
    )
    .await;
    let b = &s["data"]["batch"];
    assert_eq!(b["students_read"], 24);
    assert_eq!(b["students_evaluated"], 23);
    assert_eq!(b["rows_with_problems"], 1);
    let out = b["output_file"].as_str().unwrap();
    assert!(out.starts_with("output/"), "{out}");
    assert!(h.data().join(out).exists());
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert_eq!(s["privacy"]["files_written"][0], out);
    assert!(
        s["privacy"]["identifiers_detected"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
    // Counts only: grade x requirement, with small cells suppressed.
    let counts = b["counts"].as_array().unwrap();
    let grades: BTreeSet<&str> = counts
        .iter()
        .map(|c| c["grade"].as_str().unwrap())
        .collect();
    assert_eq!(grades, BTreeSet::from(["PK", "K", "7", "11"]));
    let threshold = b["suppression_threshold"].as_u64().unwrap();
    for c in counts {
        for (_, v) in c["counts"].as_object().unwrap() {
            let v = v.as_str().unwrap();
            if let Ok(n) = v.parse::<u64>() {
                assert!(n == 0 || n >= threshold, "unsuppressed small count {c}");
            } else {
                assert_eq!(v, "*");
            }
        }
    }
    assert!(md(&s).contains("suppressed"));
    assert!(decision_support(&s));
    let found = leaks(&everything_json(&s), "data/school_roster.csv", None);
    assert!(found.is_empty(), "roster identifiers returned: {found:?}");

    // A file without the needed columns is a tool-level error that lists
    // column names only.
    let e = call_err(
        &h,
        "check_ma_school_immunization_requirements",
        json!({"roster_path": "line_list.csv"}),
    )
    .await;
    assert!(e.contains("Grade"));
    assert!(leaks(&e, "data/line_list.csv", None).is_empty());
    call_err(
        &h,
        "check_ma_school_immunization_requirements",
        json!({"roster_path": "no_such_roster.csv"}),
    )
    .await;
}

#[tokio::test]
async fn phi_detector_flags_the_roster_identifier_columns() {
    let data = common::corpus().join("data");
    let t = phn_local::table::read_table(&data.join("school_roster.csv"), None).unwrap();
    let flagged: BTreeSet<String> = phn_local::phi::scan_table(&t)
        .into_iter()
        .map(|f| f.column)
        .collect();
    for s in common::seeded()
        .iter()
        .filter(|s| s.file == "data/school_roster.csv")
    {
        let col = s.column.as_ref().unwrap();
        assert!(
            flagged.contains(col),
            "column '{col}' ({}) not flagged; flagged {flagged:?}",
            s.kind
        );
    }
}

// ------------------------------------------------------------------ VAX-05

#[tokio::test]
async fn coverage_ranks_schools_and_compares_with_the_us() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "analyze_vaccination_coverage",
        json!({"town": "Northampton"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["measure"], "mmr");
    assert_eq!(d["threshold"]["pct"], 95.0);
    assert!(d["schools_in_scope"].as_u64().unwrap() >= d["schools_with_rates"].as_u64().unwrap());
    assert!(!d["comparison"].as_array().unwrap().is_empty());
    assert!(caveats(&s).contains("fewer than 30"));
    let sources = s["sources"].as_array().unwrap();
    assert!(
        sources
            .iter()
            .any(|x| x["url"].as_str().unwrap_or("").contains("mass.gov"))
    );
    assert!(
        sources
            .iter()
            .any(|x| x["url"].as_str().unwrap_or("").contains("data.cdc.gov"))
    );

    // Statewide exemptions, highest first.
    let s = call_ok(
        &h,
        "analyze_vaccination_coverage",
        json!({"statewide": true, "measure": "exemption", "limit": 10}),
    )
    .await;
    let ranked = s["data"]["ranked"].as_array().unwrap();
    assert_eq!(ranked.len(), 10);
    let pcts: Vec<f64> = ranked.iter().map(|r| r["pct"].as_f64().unwrap()).collect();
    assert!(pcts.windows(2).all(|w| w[0] >= w[1]), "{pcts:?}");

    // Grade 7 MenACWY with an explicit threshold flags low schools.
    let s = call_ok(
        &h,
        "analyze_vaccination_coverage",
        json!({"grade": "7", "measure": "men", "county": "Hampshire", "threshold_pct": 90}),
    )
    .await;
    for r in s["data"]["ranked"].as_array().unwrap() {
        assert_eq!(
            r["below_threshold"].as_bool().unwrap(),
            r["pct"].as_f64().unwrap() < 90.0
        );
    }

    // Influenza: Massachusetts vs U.S. only.
    let s = call_ok(
        &h,
        "analyze_vaccination_coverage",
        json!({"measure": "influenza", "statewide": true}),
    )
    .await;
    assert!(!s["data"]["comparison"].as_array().unwrap().is_empty());

    for bad in [
        json!({"measure": "smallpox"}),
        json!({"grade": "3"}),
        json!({"grade": "7", "measure": "dtap"}),
        json!({"measure": "tdap"}),
        json!({"town": "Gotham"}),
        json!({"threshold_pct": 150}),
    ] {
        call_err(&h, "analyze_vaccination_coverage", bad).await;
    }
}

#[tokio::test]
async fn coverage_of_local_school_counts_withholds_small_schools() {
    let h = start(|c| c.offline = true).await;
    std::fs::write(
        h.data().join("school_counts.csv"),
        "school,enrolled,mmr\nMaple Elementary,80,71\nRiverside Elementary,45,44\nHilltop Montessori,12,9\nCentral School,60,60\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "analyze_vaccination_coverage",
        json!({"statewide": true, "local_path": "school_counts.csv"}),
    )
    .await;
    let l = &s["data"]["local"];
    assert_eq!(l["schools"], 4);
    assert_eq!(l["schools_released"], 3);
    assert_eq!(l["schools_withheld_under_30"], 1);
    assert_eq!(l["ranked"][0]["school"], "Maple Elementary");
    assert_eq!(l["ranked"][0]["below_threshold"], true);
    assert!(!everything_json(&s).contains("Hilltop"));
    assert_eq!(s["privacy"]["row_level_data"], false);

    // A student-level file is refused.
    let e = call_err(
        &h,
        "analyze_vaccination_coverage",
        json!({"statewide": true, "local_path": "school_roster.csv"}),
    )
    .await;
    assert!(e.contains("student-level"));
    assert!(leaks(&e, "data/school_roster.csv", None).is_empty());
}

// ------------------------------------------------------------------ VAX-06

#[tokio::test]
async fn excursion_helper_classifies_and_writes_a_worksheet() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "vaccine_temperature_excursion_helper",
        json!({"unit_type": "refrigerator", "min_temp": 1.0, "max_temp": 9.5,
               "minutes_out_of_range": 45, "worksheet": "xlsx",
               "vaccines": [{"name": "MMR", "lot": "AB123", "doses": 10, "funding": "state"}]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["classification"], "reportable_cumulative");
    assert!(!d["cdc_steps"].as_array().unwrap().is_empty());
    assert!(!d["dph_steps"].as_array().unwrap().is_empty());
    assert!(d["viability"].as_str().unwrap().contains("manufacturer"));
    let f = d["worksheet_file"].as_str().unwrap();
    assert!(h.data().join(f).exists());
    assert_eq!(s["privacy"]["files_written"][0], f);
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));

    // Readings in Fahrenheit are converted; a short excursion stays below the
    // reporting threshold.
    let s = call_ok(
        &h,
        "vaccine_temperature_excursion_helper",
        json!({"unit_type": "fridge", "min_temp": 38, "max_temp": 48,
               "temp_unit": "F", "minutes_out_of_range": 10}),
    )
    .await;
    assert_eq!(
        s["data"]["classification"],
        "out_of_range_below_reporting_threshold"
    );
    assert!((s["data"]["max_temp_c"].as_f64().unwrap() - 8.9).abs() < 0.05);

    let s = call_ok(
        &h,
        "vaccine_temperature_excursion_helper",
        json!({"unit_type": "refrigerator", "min_temp": 3.0, "max_temp": 6.0}),
    )
    .await;
    assert_eq!(s["data"]["classification"], "in_range");

    let s = call_ok(
        &h,
        "vaccine_temperature_excursion_helper",
        json!({"unit_type": "freezer", "worksheet": "docx"}),
    )
    .await;
    assert_eq!(s["data"]["classification"], "undetermined");
    assert!(
        s["data"]["worksheet_file"]
            .as_str()
            .unwrap()
            .ends_with(".docx")
    );

    for bad in [
        json!({"unit_type": "oven"}),
        json!({"unit_type": "refrigerator", "temp_unit": "K"}),
        json!({"unit_type": "refrigerator", "min_temp": 9, "max_temp": 2}),
        json!({"unit_type": "refrigerator", "max_temp": 400}),
        json!({"unit_type": "refrigerator", "worksheet": "pdf"}),
    ] {
        call_err(&h, "vaccine_temperature_excursion_helper", bad).await;
    }
}

// ------------------------------------------------------------------ VAX-07

#[tokio::test]
async fn product_lookup_by_name_code_manufacturer_and_ndc() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "lookup_vaccine_product", json!({"query": "Shingrix"})).await;
    let m = &s["data"]["matches"][0];
    assert_eq!(m["cvx"], "187");
    assert!(!m["vis"].as_array().unwrap().is_empty());
    assert_eq!(s["data"]["cvx_table_mode"], "snapshot");
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));

    // CVX code: only marketed NDCs unless include_inactive.
    let s = call_ok(&h, "lookup_vaccine_product", json!({"query": "CVX 03"})).await;
    let m = &s["data"]["matches"][0];
    assert_eq!(m["cvx"], "03");
    assert!(
        m["ndcs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["end_date"].is_null() || n["end_date"].as_str().unwrap() > "2026-11-01")
    );
    let s2 = call_ok(
        &h,
        "lookup_vaccine_product",
        json!({"query": "03", "include_inactive": true}),
    )
    .await;
    assert!(
        s2["data"]["matches"][0]["ndc_total"].as_u64().unwrap() > m["ndc_total"].as_u64().unwrap()
    );

    // MVX code.
    let s = call_ok(&h, "lookup_vaccine_product", json!({"query": "SKB"})).await;
    assert!(
        s["data"]["manufacturer"]
            .as_str()
            .unwrap()
            .contains("GlaxoSmithKline")
    );

    // NDC in 10- or 11-digit form.
    let s = call_ok(
        &h,
        "lookup_vaccine_product",
        json!({"query": "00006-4681-00"}),
    )
    .await;
    assert_eq!(s["data"]["matches"][0]["cvx"], "03");

    // Misspelled trade name still matches.
    let s = call_ok(&h, "lookup_vaccine_product", json!({"query": "Shingricks"})).await;
    assert_eq!(s["data"]["matches"][0]["cvx"], "187");

    call_err(&h, "lookup_vaccine_product", json!({"query": ""})).await;
    call_err(
        &h,
        "lookup_vaccine_product",
        json!({"query": "99999-9999-99"}),
    )
    .await;
}

// ------------------------------------------------------------------ VAX-08

#[tokio::test]
async fn clinic_plan_shows_assumptions_kit_and_reconciliation_sheet() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "plan_vaccination_clinic",
        json!({"expected_turnout": 300, "clinic_hours": 4, "vaccines": ["influenza", "COVID-19"],
               "doses_per_person": 1.5, "doses_per_vial": 10, "epinephrine_weight_kg": 20,
               "doses_per_vaccinator_per_hour": 15}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["doses_total"], 450);
    // 450 doses / 4 hours / 15 per vaccinator = 7.5 -> 8 vaccinators.
    let vacc = d["staffing"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["station"] == "Vaccination")
        .unwrap();
    assert_eq!(vacc["people"], 8);
    let over = d["assumptions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "doses_per_vaccinator_per_hour")
        .unwrap();
    assert_eq!(over["basis"], "your_override");
    assert!(d["vials_to_bring"].as_u64().unwrap() >= 45);
    assert!(!d["epinephrine_table"].as_array().unwrap().is_empty());
    let epi = &d["epinephrine_lookup"];
    assert!((epi["calculated_mg"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    let f = d["reconciliation_file"].as_str().unwrap();
    assert!(h.data().join(f).exists());
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));
    assert!(caveats(&s).contains("tool default"));

    // Maximum single epinephrine dose is 0.5 mg.
    let s = call_ok(
        &h,
        "plan_vaccination_clinic",
        json!({"expected_turnout": 20, "clinic_hours": 2, "epinephrine_weight_kg": 90,
               "reconciliation_sheet": false}),
    )
    .await;
    assert!(
        (s["data"]["epinephrine_lookup"]["calculated_mg"]
            .as_f64()
            .unwrap()
            - 0.5)
            .abs()
            < 1e-9
    );
    assert!(s["data"]["reconciliation_file"].is_null());

    for bad in [
        json!({"expected_turnout": 0, "clinic_hours": 4}),
        json!({"expected_turnout": 100, "clinic_hours": 0}),
        json!({"expected_turnout": 100, "clinic_hours": 4, "waste_factor_pct": 300}),
        json!({"expected_turnout": 100, "clinic_hours": 4, "doses_per_vial": 0}),
        json!({"expected_turnout": 100, "clinic_hours": 4, "epinephrine_weight_kg": -3}),
    ] {
        call_err(&h, "plan_vaccination_clinic", bad).await;
    }
}

// ------------------------------------------------------------------ VAX-09

#[tokio::test]
async fn confidence_guide_cites_facts_scores_readability_and_never_translates() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "vaccine_confidence_conversation_guide",
        json!({"concern": "worried about autism"}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["concern_id"], "autism");
    let points = d["talking_points"].as_array().unwrap();
    assert!(!points.is_empty());
    for p in points {
        assert!(p["source_url"].as_str().unwrap().starts_with("https://"));
    }
    let rl = &d["reading_level"];
    assert!(rl["flesch_kincaid_grade"].as_f64().unwrap() > 0.0);
    assert_eq!(d["translator_needed"], false);
    assert!(decision_support(&s));
    assert!(has_pending_review_caveat(&s));

    let s = call_ok(
        &h,
        "vaccine_confidence_conversation_guide",
        json!({"concern": "HPV at 11", "language": "Spanish", "max_facts": 1}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["concern_id"], "hpv");
    assert_eq!(d["talking_points"].as_array().unwrap().len(), 1);
    assert_eq!(d["translator_needed"], true);
    assert!(!d["translated_resources"].as_array().unwrap().is_empty());
    assert!(md(&s).contains("does not machine-translate"));

    for bad in [
        json!({"concern": ""}),
        json!({"concern": "autism", "target_grade": 40}),
        json!({"concern": "qqqq zzzz xxxx"}),
    ] {
        call_err(&h, "vaccine_confidence_conversation_guide", bad).await;
    }
}

// ------------------------------------------------------------------ CORE-07

#[tokio::test]
async fn medication_lookup_offline_says_what_was_not_checked() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "lookup_medication_information",
        json!({"query": "amoxicillin"}),
    )
    .await;
    let sections = s["data"]["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 5);
    assert!(sections.iter().all(|x| x["status"] == "unavailable"));
    let text = md(&s);
    assert!(text.contains("Not checked"));
    assert!(!text.contains("none found"), "{text}");
    assert!(decision_support(&s));

    let s = call_ok(
        &h,
        "lookup_medication_information",
        json!({"query": "oseltamivir", "include": ["label", "recalls"]}),
    )
    .await;
    assert_eq!(s["data"]["sections"].as_array().unwrap().len(), 2);

    call_err(&h, "lookup_medication_information", json!({"query": " "})).await;
    call_err(
        &h,
        "lookup_medication_information",
        json!({"query": "amoxicillin", "include": ["horoscope"]}),
    )
    .await;
}
