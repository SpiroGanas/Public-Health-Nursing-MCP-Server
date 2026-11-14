//! MCP integration tests for the Phase 16 interventions tools (PLAN-03, PLAN-04, PLAN-11):
//! offline calls succeed and match their output schemas, ratings are shown only when the
//! source wording gives them, problems the nurse can fix are tool errors, the performance tool
//! returns only aggregates with small counts suppressed, and its report, workbook, and charts
//! land in the output folder.

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

fn measure<'a>(s: &'a Value, name: &str) -> &'a Value {
    s["data"]["measures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["measure"] == name)
        .unwrap_or_else(|| panic!("no measure {name}"))
}

// ---------------------------------------------------------------- PLAN-03

#[tokio::test]
async fn evidence_search_uses_the_snapshot_offline_and_cites_it() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "colorectal cancer screening"}),
    )
    .await;
    let d = &s["data"];
    let matches = d["matches"].as_array().unwrap();
    assert!(!matches.is_empty() && matches.len() <= 8);
    assert!(d["matched_total"].as_u64().unwrap() as usize >= matches.len());
    assert_eq!(d["live"]["attempted"], false);
    assert!(d["live"]["status"].as_str().unwrap().contains("offline"));
    assert!(
        d["snapshot"]["retrieved"]
            .as_str()
            .unwrap()
            .starts_with("2026-")
    );
    assert!(d["snapshot"]["records"].as_u64().unwrap() > 100);
    for m in matches {
        assert!(
            m["pubmed_url"]
                .as_str()
                .unwrap()
                .starts_with("https://pubmed.ncbi.nlm.nih.gov/")
        );
        assert_eq!(m["source_mode"], "snapshot");
        let year: i32 = m["published"].as_str().unwrap()[..4].parse().unwrap();
        assert!((1900..=2100).contains(&year));
        assert!(!m["evidence_rating"].as_str().unwrap().is_empty());
    }
    // The colorectal screening papers are in the snapshot.
    assert!(matches.iter().any(|m| {
        m["title"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("colorectal")
    }));
    // Both sites that could not be read are named with what to check.
    let not_read: Vec<&str> = d["not_read"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert!(not_read.iter().any(|n| n.contains("Community Guide")));
    assert!(not_read.iter().any(|n| n.contains("What Works for Health")));
    let c = caveats(&s);
    assert!(c.contains("not the Community Guide's list of recommendations"));
    assert!(c.contains("What Works for Health"));
    assert!(pending(&s));
    assert!(
        s["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["mode"] == "snapshot"
                && x["retrieved"].as_str().unwrap().starts_with("2026-"))
    );
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn a_rating_is_shown_only_when_the_paper_gives_it() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "alcohol", "max_results": 20, "include_live": false}),
    )
    .await;
    let mut with_words = 0;
    for m in s["data"]["matches"].as_array().unwrap() {
        let terms = m["rating_terms"].as_array().unwrap();
        match m["rating_basis"].as_str().unwrap() {
            "abstract wording" => {
                assert!(!terms.is_empty());
                assert!(!m["rating_quotes"].as_array().unwrap().is_empty());
                with_words += 1;
            }
            "not in the PubMed record" => {
                assert!(terms.is_empty());
                assert!(
                    m["evidence_rating"]
                        .as_str()
                        .unwrap()
                        .contains("Community Guide page")
                );
            }
            "title wording" => assert!(terms.is_empty()),
            other => panic!("unknown basis {other}"),
        }
    }
    assert!(with_words > 0);
    // The filters keep only papers whose abstracts use the words.
    let strong = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "alcohol", "evidence": "strong_or_recommended", "max_results": 20,
               "include_live": false}),
    )
    .await;
    let list = strong["data"]["matches"].as_array().unwrap();
    assert!(!list.is_empty());
    for m in list {
        let t: Vec<&str> = m["rating_terms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        assert!(
            t.iter()
                .any(|x| ["strong_evidence", "sufficient_evidence", "recommended"].contains(x)),
            "{m}"
        );
    }
    let ins = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "obesity", "evidence": "insufficient", "include_live": false}),
    )
    .await;
    for m in ins["data"]["matches"].as_array().unwrap() {
        assert!(
            m["rating_terms"]
                .to_string()
                .contains("insufficient_evidence")
        );
    }
}

#[tokio::test]
async fn related_words_and_kind_filters_work() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "smoking", "include_live": false, "max_results": 20}),
    )
    .await;
    assert!(
        s["data"]["topics_recognized"]
            .to_string()
            .contains("Tobacco")
    );
    assert!(s["data"]["matches"].as_array().unwrap().iter().any(|m| {
        m["title"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("tobacco")
            || m["title"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("smoke")
    }));
    let e = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "cardiovascular", "kinds": ["economic_review"], "include_live": false}),
    )
    .await;
    for m in e["data"]["matches"].as_array().unwrap() {
        assert_eq!(m["kind"], "systematic economic review");
    }
    // Old papers carry a reminder to check for updates.
    let old = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "alcohol sales", "include_live": false, "max_results": 20}),
    )
    .await;
    assert!(old["data"]["matches"].as_array().unwrap().iter().any(|m| {
        m["update_check"]
            .as_str()
            .is_some_and(|u| u.contains("updated or archived"))
    }));
    // The words the nurse typed count more than the synonyms the tool adds: a search for
    // fluoride varnish finds the 2025 school fluoride review in the first three.
    let fv = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "fluoride varnish", "include_live": false}),
    )
    .await;
    assert!(
        fv["data"]["matches"]
            .as_array()
            .unwrap()
            .iter()
            .take(3)
            .any(|m| m["published"].as_str().unwrap().starts_with("2025")),
        "{}",
        fv["data"]["matches"]
    );
    // A topic with no words in the snapshot says so and points to the sites.
    let none = call_ok(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "zxqv", "include_live": false}),
    )
    .await;
    assert!(none["data"]["matches"].as_array().unwrap().is_empty());
    assert!(caveats(&none).contains("No paper matched"));
}

#[tokio::test]
async fn evidence_search_errors_are_helpful() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "  "}),
    )
    .await;
    assert!(e.contains("topic is required"), "{e}");
    let e = call_err(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "x".repeat(300)}),
    )
    .await;
    assert!(e.contains("too long"), "{e}");
    let e = call_err(
        &h,
        "find_evidence_based_interventions",
        json!({"topic": "Maria Garcia 413-555-0100"}),
    )
    .await;
    assert!(e.contains("identifier"), "{e}");
    assert!(!e.contains("413-555-0100"));
}

// ---------------------------------------------------------------- PLAN-04

#[tokio::test]
async fn theory_maps_barriers_and_components_and_labels_every_strategy() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "apply_behavior_change_theory",
        json!({
            "problem": "Low mammography screening among Portuguese-speaking women over 50",
            "population": "Women 50 to 74",
            "barriers": ["cost", "no ride to the clinic", "family doesn't talk about it",
                         "forgot to book", "xylophone"],
            "program_components": ["text message reminders", "peer navigators", "quilting"]
        }),
    )
    .await;
    let d = &s["data"];
    let theories = d["theories"].as_array().unwrap();
    assert!((2..=4).contains(&theories.len()), "{}", theories.len());
    // Some theory looks beyond the individual.
    assert!(
        theories
            .iter()
            .any(|t| t["level"].as_str().unwrap().contains("multilevel"))
    );
    for t in theories {
        assert!(!t["sources"].as_array().unwrap().is_empty());
        for k in t["constructs"].as_array().unwrap() {
            assert!(!k["definition"].as_str().unwrap().is_empty());
            for st in k["strategies"].as_array().unwrap() {
                let basis = st["basis"].as_str().unwrap();
                assert!(basis == "source" || basis == "authored");
                assert_eq!(basis == "source", st["source"].is_string(), "{st}");
            }
            assert!(!k["measures"].as_array().unwrap().is_empty());
        }
    }
    let barrier = |text: &str| {
        d["barrier_map"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["barrier"] == text)
            .unwrap()
            .clone()
    };
    assert_eq!(barrier("cost")["no_match"], false);
    assert_eq!(barrier("no ride to the clinic")["no_match"], false);
    assert_eq!(barrier("forgot to book")["no_match"], false);
    assert_eq!(barrier("xylophone")["no_match"], true);
    let comp = |text: &str| {
        d["component_map"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["component"] == text)
            .unwrap()
            .clone()
    };
    assert_eq!(comp("text message reminders")["no_match"], false);
    assert_eq!(comp("quilting")["no_match"], true);
    assert_eq!(d["library"].as_array().unwrap().len(), 8);
    let c = caveats(&s);
    assert!(c.contains("not evidence that an intervention works"));
    assert!(c.contains("were not read"));
    assert!(c.contains("not validated instruments"));
    assert!(pending(&s));
    assert_eq!(s["privacy"]["row_level_data"], false);
}

#[tokio::test]
async fn theory_choice_all_unknown_and_defaults() {
    let h = start(|c| c.offline = true).await;
    let all = call_ok(
        &h,
        "apply_behavior_change_theory",
        json!({"problem": "Low flu vaccination", "theories": ["all"]}),
    )
    .await;
    assert_eq!(all["data"]["theories"].as_array().unwrap().len(), 8);
    let named = call_ok(
        &h,
        "apply_behavior_change_theory",
        json!({"problem": "Low flu vaccination", "theories": ["HBM", "com-b"]}),
    )
    .await;
    let ids: Vec<&str> = named["data"]["theories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["hbm", "comb"]);
    // Few signal words: a default starting set, said not to be a recommendation.
    let d = call_ok(
        &h,
        "apply_behavior_change_theory",
        json!({"problem": "Participation is low"}),
    )
    .await;
    assert!(
        d["data"]["selection"]
            .as_str()
            .unwrap()
            .contains("not a recommendation")
    );
    assert!(caveats(&d).contains("No theory was chosen"));
    let e = call_err(
        &h,
        "apply_behavior_change_theory",
        json!({"problem": "x", "theories": ["magic"]}),
    )
    .await;
    assert!(e.contains("hbm"), "{e}");
    let e = call_err(&h, "apply_behavior_change_theory", json!({"problem": " "})).await;
    assert!(e.contains("problem is required"), "{e}");
}

#[tokio::test]
async fn theory_refuses_identifiers_and_writes_a_worksheet() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        "apply_behavior_change_theory",
        json!({"problem": "Maria Garcia, 837 Elm Street, skips visits",
               "barriers": ["call 413-555-0100"]}),
    )
    .await;
    assert!(e.contains("not an individual"), "{e}");
    assert!(!e.contains("837 Elm") && !e.contains("413-555-0100"));
    let s = call_ok(
        &h,
        "apply_behavior_change_theory",
        json!({"problem": "Low screening", "barriers": ["cost"], "write_document": true}),
    )
    .await;
    let f = s["data"]["worksheet_file"].as_str().unwrap();
    assert!(f.ends_with(".docx"), "{f}");
    assert!(h.data().join(f).exists(), "{f}");
    assert!(s["privacy"]["files_written"].as_array().unwrap().len() == 1);
}

// ---------------------------------------------------------------- PLAN-11

fn targets() -> Value {
    json!([
        {"measure": "Clients enrolled", "target": 400},
        {"measure": "Participants screened", "kind": "percent",
         "denominator": "Participants eligible", "target": 80},
        {"measure": "Sessions held", "target": 130},
        {"measure": "Housing referrals", "target": 60}
    ])
}

fn expected() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/phase16_interventions/expected_performance.json"
    ))
    .unwrap()
}

#[tokio::test]
async fn performance_matches_the_independent_reference_values() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Month",
               "periods_in_program": 12, "targets": targets(), "program_name": "Healthy Start"}),
    )
    .await;
    let exp = expected();
    let e = |m: &str| exp["measures"][m].clone();
    assert_eq!(s["data"]["layout"], "wide");
    assert_eq!(s["data"]["periods"], 12);
    assert_eq!(s["data"]["first_period"], "2025-07");
    assert_eq!(s["data"]["last_period"], "2026-06");
    // Cumulative count: total, progress, pace, status.
    let m = measure(&s, "Clients enrolled");
    assert_eq!(m["current"], e("Clients enrolled")["current"]);
    assert_eq!(
        m["progress_percent"],
        e("Clients enrolled")["progress_percent"]
    );
    assert_eq!(
        m["expected_by_now"],
        e("Clients enrolled")["expected_by_now"]
    );
    assert_eq!(m["status"], e("Clients enrolled")["status"]);
    assert!(m["trend"]["signals"].as_array().unwrap().is_empty());
    assert_eq!(e("Clients enrolled")["run_chart"]["signal"], false);
    // A percentage is pooled, not the mean of the period percentages.
    let m = measure(&s, "Participants screened");
    let pooled = e("Participants screened")["current"].as_f64().unwrap();
    assert_eq!(m["current"].as_f64().unwrap(), pooled);
    assert_ne!(
        pooled,
        e("Participants screened")["mean_of_period_percentages"]
            .as_f64()
            .unwrap()
    );
    assert_eq!(
        m["progress_percent"],
        e("Participants screened")["progress_percent"]
    );
    assert_eq!(m["status"], e("Participants screened")["status"]);
    assert_eq!(m["trend"]["chart"], "p chart");
    // A steady rise is flagged by the run-chart rules, as the reference computes.
    let m = measure(&s, "Sessions held");
    assert_eq!(e("Sessions held")["run_chart"]["signal"], true);
    assert!(!m["trend"]["signals"].as_array().unwrap().is_empty(), "{m}");
    assert_eq!(m["trend"]["chart"], "run chart");
    assert_eq!(m["status"], e("Sessions held")["status"]);
    assert_eq!(m["current"], e("Sessions held")["current"]);
    // Small counts are hidden, and so is everything that would reveal them.
    let m = measure(&s, "Housing referrals");
    assert_eq!(m["withheld"], true);
    assert!(m["current"].is_null() && m["progress_percent"].is_null());
    assert!(m["trend"]["center"].is_null());
    assert!(m["status"].as_str().unwrap().starts_with("Not shown"));
    let hidden: Vec<usize> = m["periods"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, p)| p["suppressed"] == true)
        .map(|(i, p)| {
            assert!(p["value"].is_null());
            i
        })
        .collect();
    let want: Vec<usize> = e("Housing referrals")["hidden_periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as usize)
        .collect();
    assert_eq!(hidden, want);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert_eq!(s["privacy"]["suppression_threshold"], exp["threshold"]);
    assert_eq!(s["privacy"]["row_level_data"], false);
    // Nothing from the unread contact columns.
    let all = everything(
        &h.call(
            "track_program_performance",
            json!({"path": "program_metrics.csv",
        "period_column": "Month", "targets": targets()}),
        )
        .await,
    );
    assert!(leaks(&all, "data/program_metrics.csv", None).is_empty());
}

#[tokio::test]
async fn performance_writes_a_report_a_workbook_with_live_formulas_and_charts() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Month",
               "periods_in_program": 12, "targets": targets(), "program_name": "Healthy Start"}),
    )
    .await;
    let d = &s["data"];
    let report = h.data().join(d["report_file"].as_str().unwrap());
    let book = h.data().join(d["workbook_file"].as_str().unwrap());
    assert!(report.exists() && report.extension().unwrap() == "docx");
    assert!(book.exists() && book.extension().unwrap() == "xlsx");
    let files = s["privacy"]["files_written"].as_array().unwrap();
    assert!(files.len() >= 6, "{files:?}");
    for m in d["measures"].as_array().unwrap() {
        let png = m["chart"]["png"].as_str().expect("a chart per measure");
        assert!(h.data().join(png).exists());
        assert!(!m["chart"]["alt_text"].as_str().unwrap().is_empty());
    }
    // The workbook's Summary sheet is made of formulas over the Data sheet.
    let sheet1 = std::process::Command::new("unzip")
        .args(["-p", book.to_str().unwrap(), "xl/worksheets/sheet1.xml"])
        .output()
        .expect("unzip");
    let xml = String::from_utf8_lossy(&sheet1.stdout).to_string();
    assert!(
        xml.contains("<f>SUM(Data!B$2:B$101)</f>"),
        "{}",
        &xml[..xml.len().min(2000)]
    );
    assert!(xml.contains("COUNT(Data!B$2:B$101)"));
    assert!(xml.contains("\"On track\""));
    let sheet2 = std::process::Command::new("unzip")
        .args(["-p", book.to_str().unwrap(), "xl/worksheets/sheet2.xml"])
        .output()
        .expect("unzip");
    let xml2 = String::from_utf8_lossy(&sheet2.stdout).to_string();
    // The percent column is a formula down to row 101, and the data are numbers.
    assert!(
        xml2.contains("<f>IF(OR(D2=\"\",D2=0),\"\",C2/D2*100)</f>"),
        "{}",
        &xml2[..xml2.len().min(2500)]
    );
    assert!(xml2.contains("<f>IF(OR(D101=\"\""));
    // The report has the full values, including the counts the response hides.
    let doc = std::process::Command::new("unzip")
        .args(["-p", report.to_str().unwrap(), "word/document.xml"])
        .output()
        .expect("unzip");
    let text = String::from_utf8_lossy(&doc.stdout).to_string();
    assert!(text.contains("Housing referrals") && text.contains("Healthy Start"));
    assert!(!text.contains("Philomena") && !text.contains("Marguerite"));
}

#[tokio::test]
async fn performance_without_a_year_length_does_not_assess_pace_and_reads_long_files() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Month",
               "targets": [{"measure": "Clients enrolled", "target": 400}],
               "write_report": false, "write_workbook": false, "write_charts": false}),
    )
    .await;
    assert_eq!(measure(&s, "Clients enrolled")["status"], "Not yet met");
    assert!(caveats(&s).contains("periods_in_program was not given"));
    assert!(s["data"]["report_file"].is_null() && s["data"]["workbook_file"].is_null());
    // Long layout, quarterly text periods, a percentage from a denominator column.
    let l = call_ok(
        &h,
        "track_program_performance",
        json!({"path": "program_outcomes_long.csv", "period_column": "Quarter",
               "measure_column": "Outcome", "value_column": "Value",
               "denominator_column": "Denominator", "periods_in_program": 4,
               "targets": [
                 {"measure": "Adults with controlled blood pressure", "kind": "percent", "target": 70},
                 {"measure": "Home visits completed", "target": 240},
                 {"measure": "Falls prevention class graduates", "kind": "percent", "target": 60}],
               "write_report": false, "write_workbook": false, "write_charts": false}),
    )
    .await;
    assert_eq!(l["data"]["layout"], "long");
    assert_eq!(l["data"]["first_period"], "2025 Q3");
    assert_eq!(l["data"]["last_period"], "2026 Q2");
    let bp = measure(&l, "Adults with controlled blood pressure");
    let pooled = (38.0 + 41.0 + 47.0 + 52.0) / (60.0 + 62.0 + 65.0 + 66.0) * 100.0;
    assert!(
        (bp["current"].as_f64().unwrap() - pooled).abs() < 0.006,
        "{bp}"
    );
    assert_eq!(bp["status"], "Met");
    // 215 visits against a pace of 240 is 89.6%, just under the 90% rule.
    let hv = measure(&l, "Home visits completed");
    assert_eq!(hv["current"], 215.0);
    assert_eq!(hv["status"], "Behind");
    // 3 of 10 and 4 of 12 graduates are small counts: hidden, and the totals withheld.
    let falls = measure(&l, "Falls prevention class graduates");
    assert_eq!(falls["withheld"], true);
    assert!(falls["current"].is_null());
    let all = everything(
        &h.call(
            "track_program_performance",
            json!({"path": "program_outcomes_long.csv",
        "period_column": "Quarter", "measure_column": "Outcome", "value_column": "Value",
        "targets": [{"measure": "Home visits completed", "target": 240}]}),
        )
        .await,
    );
    assert!(leaks(&all, "data/program_outcomes_long.csv", None).is_empty());
}

#[tokio::test]
async fn performance_counts_records_by_month_and_never_returns_a_row() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "track_program_performance",
        json!({"path": "line_list.csv", "period_column": "Onset Date", "count_rows": true,
               "targets": [{"measure": "Records", "target": 50}],
               "write_report": false, "write_workbook": false, "write_charts": false}),
    )
    .await;
    assert_eq!(s["data"]["layout"], "records counted");
    let m = measure(&s, "Records");
    let periods = m["periods"].as_array().unwrap();
    assert!(!periods.is_empty());
    for p in periods {
        let label = p["period"].as_str().unwrap();
        assert!(label.len() == 7 && label.as_bytes()[4] == b'-', "{label}");
    }
    let all = everything(
        &h.call(
            "track_program_performance",
            json!({"path": "line_list.csv",
        "period_column": "Onset Date", "count_rows": true,
        "targets": [{"measure": "Records", "target": 50}]}),
        )
        .await,
    );
    assert!(leaks(&all, "data/line_list.csv", None).is_empty());
    assert!(!all.contains("C-2026"));
    assert!(caveats(&s).contains("Rows were counted per period"));
    // Records with a numerator-free quarterly roll-up.
    let q = call_ok(
        &h,
        "track_program_performance",
        json!({"path": "line_list.csv", "period_column": "Onset Date", "count_rows": true,
               "date_period": "quarter", "targets": [{"measure": "Records", "target": 50}],
               "write_report": false, "write_workbook": false, "write_charts": false}),
    )
    .await;
    assert!(
        measure(&q, "Records")["periods"].as_array().unwrap()[0]["period"]
            .as_str()
            .unwrap()
            .contains('Q')
    );
}

#[tokio::test]
async fn performance_refuses_identifier_columns_and_explains_problems() {
    let h = start(|c| c.offline = true).await;
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "line_list.csv", "period_column": "Onset Date",
               "measure_column": "Last Name", "targets": [{"measure": "Cases", "target": 1}]}),
    )
    .await;
    assert!(e.contains("cannot be analyzed"), "{e}");
    assert!(leaks(&e, "data/line_list.csv", None).is_empty());
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "line_list.csv", "period_column": "Onset Date",
               "targets": [{"measure": "SSN", "target": 1}]}),
    )
    .await;
    assert!(e.contains("cannot be analyzed"), "{e}");
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Contact Person",
               "targets": [{"measure": "Clients enrolled", "target": 1}]}),
    )
    .await;
    assert!(e.contains("cannot be analyzed"), "{e}");
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Month", "targets": []}),
    )
    .await;
    assert!(e.contains("at least one target"), "{e}");
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Month",
               "targets": [{"measure": "Screening", "kind": "percent", "target": 80}]}),
    )
    .await;
    assert!(e.contains("needs a denominator"), "{e}");
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Month",
               "targets": [{"measure": "Not a column", "target": 5}]}),
    )
    .await;
    assert!(e.contains("Columns you can use"), "{e}");
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "program_outcomes_long.csv", "period_column": "Quarter",
               "measure_column": "Outcome", "value_column": "Value",
               "targets": [{"measure": "Missing outcome", "target": 5}]}),
    )
    .await;
    assert!(e.contains("Check the spelling"), "{e}");
    // The values of the column are never listed (they could be names).
    assert!(
        !e.contains("Home visits completed") && !e.contains("Adults with"),
        "{e}"
    );
    // A column that is not a period (here, a column of staff names the detector does not
    // recognize) is refused without echoing a value.
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "program_outcomes_long.csv", "period_column": "Reported By",
               "count_rows": true, "targets": [{"measure": "Records", "target": 5}]}),
    )
    .await;
    assert!(e.contains("do not look like reporting periods"), "{e}");
    assert!(
        leaks(&e, "data/program_outcomes_long.csv", None).is_empty(),
        "{e}"
    );
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "program_metrics.csv", "period_column": "Month",
               "on_track_fraction": 1.5, "targets": [{"measure": "Clients enrolled", "target": 5}]}),
    )
    .await;
    assert!(e.contains("on_track_fraction"), "{e}");
    let e = call_err(
        &h,
        "track_program_performance",
        json!({"path": "nonexistent.csv", "period_column": "Month",
               "targets": [{"measure": "Clients enrolled", "target": 5}]}),
    )
    .await;
    assert!(!e.is_empty());
}
