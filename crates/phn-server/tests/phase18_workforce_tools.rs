//! MCP integration tests for the Phase 18 workforce tools (LEAD-06, LEAD-07, LEAD-08): team
//! results and credential statuses match the independent Python computation (generator committed
//! beside the expected values), the small-group rule holds, names are refused, the notice
//! calculator gives hand-verified dates, minutes are drafted without echoing the notes, standing
//! orders carry no bundled clinical content, and files land in the output folder.

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

fn expected() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/phase18_workforce/expected_workforce.json"
    ))
    .unwrap()
}

fn unzip(file: &std::path::Path, member: &str) -> String {
    let o = std::process::Command::new("unzip")
        .args(["-p", file.to_str().unwrap(), member])
        .output()
        .expect("unzip");
    String::from_utf8_lossy(&o.stdout).to_string()
}

const SEEDS: [&str; 3] = [
    "data/workforce_ratings.csv",
    "data/workforce_credentials.csv",
    "data/board_meeting_notes.txt",
];

fn no_codes(text: &str) {
    let re = regex::Regex::new(r"\b[SN]0[1-7]\b").unwrap();
    assert!(!re.is_match(text), "a staff code came back: {text}");
}

// ---------------------------------------------------------------- LEAD-06

#[tokio::test]
async fn blank_self_assessment_has_formulas_and_hides_small_groups() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "assess_workforce_competencies", json!({})).await;
    assert_eq!(s["data"]["mode"], "template");
    assert!(pending(&s));
    assert_eq!(s["data"]["domains"].as_array().unwrap().len(), 8);
    assert_eq!(s["data"]["tiers"].as_array().unwrap().len(), 3);
    assert_eq!(s["data"]["rating_scale"].as_array().unwrap().len(), 4);
    assert_eq!(s["data"]["minimum_group_size"], 3);
    assert!(s["data"]["team"].is_null());
    let nv = serde_json::to_string(&s["data"]["not_verified"]).unwrap();
    assert!(nv.contains("Competency statements"), "{nv}");
    let book = h.data().join(s["data"]["workbook_file"].as_str().unwrap());
    let plan = h.data().join(s["data"]["plan_file"].as_str().unwrap());
    assert!(book.exists() && book.extension().unwrap() == "xlsx");
    assert!(plan.exists() && plan.extension().unwrap() == "docx");
    // Same output folder for both files.
    assert_eq!(book.parent(), plan.parent());
    let team = unzip(&book, "xl/worksheets/sheet4.xml");
    assert!(team.contains("AVERAGEIFS(") && team.contains("COUNTIFS("));
    assert!(team.contains("suppressed"));
    assert!(team.contains("Assumptions!$B$3"));
    let cred = unzip(&book, "xl/worksheets/sheet6.xml");
    assert!(
        cred.contains("EDATE(C5,F5)"),
        "{}",
        &cred[..cred.len().min(1500)]
    );
    assert!(cred.contains("overdue"));
    let wbx = unzip(&book, "xl/workbook.xml");
    for n in [
        "Ratings",
        "Team results",
        "Training plan",
        "Credentials",
        "Standards",
    ] {
        assert!(wbx.contains(n), "{n}");
    }
    let doc = unzip(&plan, "word/document.xml");
    assert!(doc.contains("Rating scale") && doc.contains("Knowledgeable"));
    // The second framework.
    let s = call_ok(
        &h,
        "assess_workforce_competencies",
        json!({"framework": "cphn_2018"}),
    )
    .await;
    let d = serde_json::to_string(&s["data"]["domains"]).unwrap();
    assert!(
        d.contains("Cultural Competency Skills") && d.contains("Financial Planning"),
        "{d}"
    );
}

#[tokio::test]
async fn team_results_and_credentials_match_the_python_computation() {
    let h = start(|c| c.offline = true).await;
    let want = expected();
    let s = call_ok(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "workforce_ratings.csv",
               "credentials_path": "workforce_credentials.csv", "as_of_date": "2026-11-12"}),
    )
    .await;
    assert!(pending(&s));
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert_eq!(s["privacy"]["suppression_applied"], true);
    let t = &s["data"]["team"];
    assert_eq!(t["team_size"], want["team_size"].to_string());
    let got = t["domains"].as_array().unwrap();
    for (g, w) in got.iter().zip(want["domains"].as_array().unwrap()) {
        assert_eq!(g["domain"], w["domain"]);
        if w["mean"].is_null() {
            assert!(g["mean_rating"].is_null(), "{g}");
            assert_eq!(g["suppressed"], true);
            continue;
        }
        assert_eq!(g["raters"], w["raters"], "{g}");
        assert!(
            (g["mean_rating"].as_f64().unwrap() - w["mean"].as_f64().unwrap()).abs() < 0.006,
            "{g} {w}"
        );
        assert!(
            (g["gap_to_target"].as_f64().unwrap() - w["gap"].as_f64().unwrap()).abs() < 0.011,
            "{g} {w}"
        );
        assert_eq!(g["staff_below_target"], w["below"], "{g}");
    }
    let overall = t["overall_mean_rating"].as_f64().unwrap();
    assert!((overall - want["overall"].as_f64().unwrap()).abs() < 0.006);
    // Tier 1 has four staff and is shown; tier 2 has two and is not.
    let tiers = t["tiers"].as_array().unwrap();
    let t1 = tiers.iter().find(|x| x["tier"] == 1).unwrap();
    assert_eq!(t1["staff"], want["tiers"]["1"]["staff"]);
    assert!(
        (t1["mean_rating"].as_f64().unwrap() - want["tiers"]["1"]["mean"].as_f64().unwrap()).abs()
            < 0.006
    );
    let t2 = tiers.iter().find(|x| x["tier"] == 2).unwrap();
    assert_eq!(t2["suppressed"], true);
    assert!(t2["staff"].is_null() && t2["mean_rating"].is_null());
    // The plan is ranked by gap: Management and Finance (0.8) first.
    let plan = s["data"]["training_plan"].as_array().unwrap();
    assert_eq!(plan[0]["domain"], "Management and Finance Skills");
    assert_eq!(plan[0]["priority"], "Medium");
    assert!(!plan[0]["suggested_steps"].as_array().unwrap().is_empty());
    // Credentials: MAVEN is shown for 7 nurses; the others hide counts of one or two.
    let sums = s["data"]["credentials"]["summaries"].as_array().unwrap();
    let maven = sums
        .iter()
        .find(|x| x["requirement"] == "MAVEN-trained")
        .unwrap();
    let w = &want["credential_summary"]["MAVEN-trained"];
    assert_eq!(maven["staff"], w["staff"]);
    assert_eq!(maven["complete"], w["complete"]);
    assert_eq!(maven["needs_attention"], w["attention"]);
    assert_eq!(maven["upcoming"], w["upcoming"]);
    assert_eq!(maven["position"], "Public health nurse");
    for label in ["Foundations course", "ICS 100 and NIMS 700"] {
        let x = sums
            .iter()
            .find(|x| x["requirement"] == label && x["position"] == "Public health nurse")
            .unwrap();
        assert_eq!(x["suppressed"], true, "{label}");
        assert!(x["complete"].is_null() && x["needs_attention"].is_null() && x["staff"].is_null());
    }
    // One inspector: no counts at all.
    assert!(
        sums.iter()
            .filter(|x| x["position"].as_str().unwrap().starts_with("Inspector"))
            .all(|x| x["suppressed"] == true)
    );
    // One volunteer matched no DPH position (count of one: hidden).
    assert_eq!(
        s["data"]["credentials"]["staff_position_not_matched"],
        "fewer than 3"
    );
    // No code, name, or phone anywhere in the response.
    let all = everything(&h.call("assess_workforce_competencies",
        json!({"ratings_path": "workforce_ratings.csv", "credentials_path": "workforce_credentials.csv",
               "as_of_date": "2026-11-12", "write_documents": false})).await);
    no_codes(&all);
    for f in SEEDS {
        assert!(leaks(&all, f, None).is_empty(), "{f}");
    }
    // The workbook has individual rows (codes) and the statuses the reference computed.
    let book = h.data().join(s["data"]["workbook_file"].as_str().unwrap());
    let cred = unzip(&book, "xl/worksheets/sheet6.xml");
    let n = want["credential_statuses"]["Foundations course"]["N04"]
        .as_str()
        .unwrap();
    assert_eq!(n, "overdue");
    assert!(cred.contains("overdue") && cred.contains("due soon"));
    let strings = unzip(&book, "xl/sharedStrings.xml");
    assert!(strings.contains("N04"));
    let ratings = unzip(&book, "xl/sharedStrings.xml");
    assert!(ratings.contains("S01"));
    let plan_doc = h.data().join(s["data"]["plan_file"].as_str().unwrap());
    let doc = unzip(&plan_doc, "word/document.xml");
    assert!(doc.contains("Management and Finance Skills"));
    no_codes(&doc);
    for f in SEEDS {
        assert!(leaks(&doc, f, None).is_empty(), "plan leaks {f}");
    }
}

#[tokio::test]
async fn teams_under_three_get_one_total_or_nothing() {
    let h = start(|c| c.offline = true).await;
    std::fs::write(
        h.data().join("two_staff.csv"),
        "Staff Code,Tier,Domain,Rating\nA1,1,1,2\nA2,1,1,4\nA1,1,2,3\nA2,1,2,3\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "two_staff.csv", "write_documents": false}),
    )
    .await;
    let t = &s["data"]["team"];
    assert_eq!(t["team_size"], "fewer than 3");
    assert!((t["overall_mean_rating"].as_f64().unwrap() - 3.0).abs() < 1e-9);
    assert!(
        t["domains"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["mean_rating"].is_null())
    );
    assert!(
        t["tiers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["suppressed"] == true)
    );
    let text = everything(
        &h.call(
            "assess_workforce_competencies",
            json!({"ratings_path": "two_staff.csv", "write_documents": false}),
        )
        .await,
    );
    assert!(!text.contains("A1") && !text.contains("A2"));
    std::fs::write(
        h.data().join("one_staff.csv"),
        "Staff Code,Tier,Domain,Rating\nA1,1,1,2\n",
    )
    .unwrap();
    let s = call_ok(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "one_staff.csv", "write_documents": false}),
    )
    .await;
    assert!(s["data"]["team"]["overall_mean_rating"].is_null());
    assert!(summary_of(&s).contains("fewer than 3"));
}

fn summary_of(s: &Value) -> String {
    s["summary"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn names_and_bad_cells_are_refused_without_echoing_values() {
    let h = start(|c| c.offline = true).await;
    // A roster with a Name column used as the staff column.
    let e = call_err(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "staff_roster.csv", "staff_column": "Name",
               "domain_column": "ICS Position", "rating_column": "Radio"}),
    )
    .await;
    assert!(e.contains("code"), "{e}");
    assert!(leaks(&e, "data/staff_roster.csv", None).is_empty(), "{e}");
    // A client line list.
    let e = call_err(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "line_list.csv", "staff_column": "Last Name",
               "domain_column": "Town", "rating_column": "Age"}),
    )
    .await;
    assert!(leaks(&e, "data/line_list.csv", None).is_empty(), "{e}");
    // Phone and email columns as ratings.
    let e = call_err(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "workforce_ratings.csv", "rating_column": "Supervisor Email"}),
    )
    .await;
    assert!(
        leaks(&e, "data/workforce_ratings.csv", None).is_empty(),
        "{e}"
    );
    // A rating of 7 and a domain that does not exist.
    std::fs::write(
        h.data().join("bad_rating.csv"),
        "Staff Code,Tier,Domain,Rating\nA1,1,1,7\n",
    )
    .unwrap();
    let e = call_err(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "bad_rating.csv"}),
    )
    .await;
    assert!(e.contains("not 1 to 4"), "{e}");
    std::fs::write(
        h.data().join("bad_domain.csv"),
        "Staff Code,Tier,Domain,Rating\nA1,1,Astrology,3\n",
    )
    .unwrap();
    let e = call_err(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "bad_domain.csv"}),
    )
    .await;
    assert!(e.contains("matches none"), "{e}");
    call_err(
        &h,
        "assess_workforce_competencies",
        json!({"target_rating": 5}),
    )
    .await;
    call_err(
        &h,
        "assess_workforce_competencies",
        json!({"ratings_path": "nope.csv"}),
    )
    .await;
    // Credentials with a bad hire date.
    std::fs::write(
        h.data().join("bad_hire.csv"),
        "Staff Code,Position,Hire Date,Credential,Completed Date\nN1,Public health nurse,someday,,\n",
    )
    .unwrap();
    let e = call_err(
        &h,
        "assess_workforce_competencies",
        json!({"credentials_path": "bad_hire.csv"}),
    )
    .await;
    assert!(e.contains("YYYY-MM-DD"), "{e}");
}

// ---------------------------------------------------------------- LEAD-07

fn notice_args(date: &str, time: &str) -> Value {
    json!({"action": "notice_check", "meeting_date": date, "meeting_time": time,
           "location": "Town Hall", "town": "Cambridge",
           "topics": [{"title": "Seasonal influenza clinic schedule and staffing"}]})
}

#[tokio::test]
async fn notice_deadlines_match_the_hand_verified_dates() {
    let h = start(|c| c.offline = true).await;
    for (date, time, post_by) in [
        ("2026-11-16", "18:00", "2026-11-12 18:00"),
        ("2026-11-17", "6:00 pm", "2026-11-13 18:00"),
        ("2026-11-13", "10:00", "2026-11-10 10:00"),
        ("2026-11-30", "09:00", "2026-11-25 09:00"),
        ("2026-12-28", "12:00", "2026-12-23 12:00"),
        ("2027-07-06", "17:00", "2027-07-01 17:00"),
        ("2028-06-20", "08:00", "2028-06-15 08:00"),
    ] {
        let s = call_ok(
            &h,
            "board_of_health_meeting_assistant",
            notice_args(date, time),
        )
        .await;
        assert_eq!(s["data"]["notice"]["post_by_iso"], post_by, "{date} {time}");
        assert!(pending(&s));
        assert_eq!(
            s["notice"]
                .as_str()
                .map(|n| n.starts_with("Decision support")),
            Some(true)
        );
    }
    // Veterans Day is listed as a day left out.
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        notice_args("2026-11-13", "10:00"),
    )
    .await;
    let days = serde_json::to_string(&s["data"]["notice"]["excluded_days_passed"]).unwrap();
    assert!(days.contains("Veterans Day"), "{days}");
    // Suffolk county: Evacuation Day is also left out (Boston), not for Cambridge.
    let mut a = notice_args("2026-03-18", "12:00");
    let s = call_ok(&h, "board_of_health_meeting_assistant", a.clone()).await;
    assert_eq!(s["data"]["notice"]["post_by_iso"], "2026-03-16 12:00");
    a["town"] = json!("Boston");
    let s = call_ok(&h, "board_of_health_meeting_assistant", a.clone()).await;
    assert_eq!(s["data"]["notice"]["post_by_iso"], "2026-03-13 12:00");
    assert_eq!(s["data"]["notice"]["body_is_suffolk_county"], true);
    a["town"] = json!("Cambridge");
    a["suffolk_county"] = json!(true);
    let s = call_ok(&h, "board_of_health_meeting_assistant", a).await;
    assert_eq!(s["data"]["notice"]["post_by_iso"], "2026-03-13 12:00");
}

#[tokio::test]
async fn posting_time_and_notice_contents_are_checked() {
    let h = start(|c| c.offline = true).await;
    let mut a = notice_args("2026-11-16", "18:00");
    a["posted_at"] = json!("2026-11-12 18:00");
    let s = call_ok(&h, "board_of_health_meeting_assistant", a.clone()).await;
    let p = &s["data"]["notice"]["posted_check"];
    assert_eq!(p["compliant"], true);
    assert_eq!(p["countable_hours"], 48.0);
    a["posted_at"] = json!("2026-11-13 18:00");
    let s = call_ok(&h, "board_of_health_meeting_assistant", a.clone()).await;
    let p = &s["data"]["notice"]["posted_check"];
    assert_eq!(p["compliant"], false);
    assert_eq!(p["countable_hours"], 24.0);
    assert_eq!(p["shortfall_minutes"], 1440);
    // Too-general topics, a missing executive session topic, and remote access.
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "notice_check", "meeting_date": "2026-12-08", "meeting_time": "19:00",
               "remote_meeting": true, "topics": [{"title": "New business"}],
               "executive_session_purposes": [3, 1]}),
    )
    .await;
    let checks = serde_json::to_string(&s["data"]["notice"]["checks"]).unwrap();
    assert!(checks.contains("may be too general"), "{checks}");
    assert!(checks.contains("must be listed as a topic"), "{checks}");
    assert!(checks.contains("clear information to access"), "{checks}");
    assert!(checks.contains("notified in writing"), "{checks}");
    assert!(checks.contains("detrimental effect"), "{checks}");
    // An emergency has no 48-hour deadline.
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "notice_check", "meeting_date": "2026-12-08", "meeting_time": "19:00",
               "emergency": true, "location": "Town Hall", "topics": [{"title": "Boil water order response"}]}),
    )
    .await;
    assert!(s["data"]["notice"]["post_by"].is_null());
    // A Sunday meeting is flagged.
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        notice_args("2026-11-15", "10:00"),
    )
    .await;
    assert_eq!(s["data"]["notice"]["meeting_on_excluded_day"], true);
}

#[tokio::test]
async fn notice_errors_are_tool_errors() {
    let h = start(|c| c.offline = true).await;
    call_err(&h, "board_of_health_meeting_assistant", json!({})).await;
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "notice_check"}),
    )
    .await;
    // Outside the bundled holiday list.
    let e = call_err(
        &h,
        "board_of_health_meeting_assistant",
        notice_args("2029-02-06", "18:00"),
    )
    .await;
    assert!(e.contains("2026-01-01 to 2028-12-31"), "{e}");
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        notice_args("2026-01-02", "10:00"),
    )
    .await;
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        notice_args("2026-13-01", "10:00"),
    )
    .await;
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        notice_args("2026-12-01", "25:00"),
    )
    .await;
    let mut a = notice_args("2026-12-01", "18:00");
    a["town"] = json!("Atlantis");
    call_err(&h, "board_of_health_meeting_assistant", a).await;
    let mut a = notice_args("2026-12-01", "18:00");
    a["executive_session_purposes"] = json!([11]);
    call_err(&h, "board_of_health_meeting_assistant", a).await;
    // Identifiers in a topic.
    let mut a = notice_args("2026-12-01", "18:00");
    a["topics"] = json!([{"title": "Complaint from 12 Larkspur Lane, call 413-555-0188"}]);
    let e = call_err(&h, "board_of_health_meeting_assistant", a).await;
    assert!(!e.contains("Larkspur") && !e.contains("0188"), "{e}");
}

#[tokio::test]
async fn agenda_writes_a_document_with_staff_pages() {
    let h = start(|c| c.offline = true).await;
    let mut a = notice_args("2026-12-07", "18:30");
    a["action"] = json!("agenda");
    a["executive_session_purposes"] = json!([3]);
    a["topics"] = json!([
        {"title": "Seasonal influenza clinic schedule and staffing", "presenter_role": "Public health nurse", "minutes": 15, "action_requested": true},
        {"title": "Executive session under c. 30A, s. 21(a)(3): litigation strategy"}
    ]);
    let s = call_ok(&h, "board_of_health_meeting_assistant", a).await;
    assert_eq!(s["data"]["agenda"]["topics"], 2);
    assert_eq!(s["data"]["agenda"]["planned_minutes"], 15);
    let file = h.data().join(s["data"]["document_file"].as_str().unwrap());
    assert!(file.exists() && file.extension().unwrap() == "docx");
    let doc = unzip(&file, "word/document.xml");
    assert!(
        doc.contains("Post the notice by") && doc.contains("December 3, 2026"),
        "{}",
        &doc[..doc.len().min(2000)]
    );
    assert!(doc.contains("For staff: not for posting"));
    assert!(doc.contains("Executive session script") && doc.contains("roll call"));
    assert!(doc.contains("CLERK: record when the notice is posted"));
}

#[tokio::test]
async fn staff_report_template_and_oml_guide() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "staff_report", "report_title": "Flu clinic plan",
               "meeting_date": "2026-12-07",
               "report_sections": [{"heading": "Background", "text": "The clinics run in October."},
                                   {"heading": "Fiscal impact", "text": "No new cost."}]}),
    )
    .await;
    assert_eq!(s["data"]["staff_report"]["sections_drafted"], 2);
    // "Fiscal impact" does not match a standard heading, so it is added as a section.
    assert_eq!(s["data"]["staff_report"]["sections_placeholder"], 5);
    let file = h.data().join(s["data"]["document_file"].as_str().unwrap());
    let doc = unzip(&file, "word/document.xml");
    assert!(doc.contains("The clinics run in October") && doc.contains("[NURSE:"));
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "staff_report",
               "report_sections": [{"heading": "Findings", "text": "Maria Garcia, DOB 1953-04-11, lives at 837 Elm Street"}]}),
    )
    .await;
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "oml_guide"}),
    )
    .await;
    let g = &s["data"]["oml_guide"];
    assert_eq!(
        g["executive_session_purposes"].as_array().unwrap().len(),
        10
    );
    assert!(g["notice_rule"].as_str().unwrap().contains("48 hours"));
    assert!(g["legal_holidays"].as_array().unwrap().len() >= 40);
    assert!(
        g["remote_participation"]
            .as_str()
            .unwrap()
            .contains("June 30, 2027")
    );
}

#[tokio::test]
async fn minutes_are_drafted_from_notes_without_returning_them() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "minutes", "notes_path": "board_meeting_notes.txt"}),
    )
    .await;
    let m = &s["data"]["minutes"];
    assert_eq!(m["items"], 2);
    assert_eq!(m["votes_recorded"], 3);
    assert_eq!(m["documents_listed"], 1);
    assert_eq!(m["executive_session_in_notes"], true);
    assert_eq!(m["remote_members_in_notes"], true);
    assert_eq!(m["approve_by_30_days"], "2027-01-06");
    let parts = m["parts"].as_array().unwrap();
    let status = |name: &str| {
        parts
            .iter()
            .find(|p| p["part"].as_str().unwrap().starts_with(name))
            .unwrap_or_else(|| panic!("{name}"))["status"]
            .clone()
    };
    assert_eq!(status("Date"), "found");
    assert_eq!(status("Members present"), "found");
    assert_eq!(status("Decisions and votes"), "found");
    // The "3-0" vote with a remote member is not a roll call.
    assert_eq!(status("Remote members named"), "check");
    assert_eq!(status("Executive session"), "found");
    // The PHI detector saw the resident's line (line 9), and no value came back.
    assert!(
        m["identifier_lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == 9),
        "{m}"
    );
    assert!(!m["identifier_categories"].as_array().unwrap().is_empty());
    // Meeting dates and exhibit dates are not reported as identifiers.
    assert_eq!(m["identifier_lines"], json!([9]), "{m}");
    let all = everything(&h.call(
        "board_of_health_meeting_assistant",
        json!({"action": "minutes", "notes_path": "board_meeting_notes.txt", "write_document": false}),
    ).await);
    assert!(
        leaks(&all, "data/board_meeting_notes.txt", None).is_empty(),
        "{all}"
    );
    assert!(!all.contains("three clinic dates") && !all.contains("abutters"));
    // The draft minutes carry the notes and a warning.
    let file = h.data().join(s["data"]["document_file"].as_str().unwrap());
    let doc = unzip(&file, "word/document.xml");
    assert!(doc.contains("three clinic dates") && doc.contains("CHECK BEFORE RELEASE"));
    assert!(doc.contains("Members participating remotely"));
    assert!(doc.contains("Executive session"));
    // Typed notes work too; both sources together do not.
    let s = call_ok(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "minutes", "notes_text": "Item: Budget\nThe board discussed the grant.\nVote: roll call A yes B yes C yes",
               "meeting_date": "2026-12-07", "write_document": false}),
    )
    .await;
    assert_eq!(s["data"]["minutes"]["items"], 1);
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "minutes", "notes_text": "x", "notes_path": "board_meeting_notes.txt"}),
    )
    .await;
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "minutes"}),
    )
    .await;
    call_err(
        &h,
        "board_of_health_meeting_assistant",
        json!({"action": "minutes", "notes_path": "line_list.csv"}),
    )
    .await;
}

// ---------------------------------------------------------------- LEAD-08

#[tokio::test]
async fn a_vaccine_order_carries_the_nurses_text_and_signature_placeholders() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "draft_standing_orders",
        json!({"order_type": "vaccine", "title": "Seasonal influenza vaccine", "agency_name": "Town Board of Health",
               "effective_date": "2026-10-01", "review_interval_months": 12,
               "sections": [
                 {"section_id": "inclusion_criteria", "text": "Everyone six months of age and older.",
                  "source_citation": "Example guideline, 2026-27 edition"},
                 {"section_id": "documentation", "text": "Record the VIS edition and date, lot number, and site in the clinic record."},
                 {"section_id": "emergency", "text": "Call 911 for anaphylaxis and follow the agency protocol.",
                  "source_citation": "Agency anaphylaxis protocol, 2026"}
               ]}),
    )
    .await;
    assert!(pending(&s));
    let d = &s["data"];
    assert_eq!(d["bundled_clinical_content"], "none");
    assert_eq!(d["drafted_sections"], 3);
    assert_eq!(d["dates"]["next_review_due"], "2027-10-01");
    assert!(d["dph_orders_to_check_first"].as_array().unwrap().len() >= 4);
    let incl = d["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["section_id"] == "inclusion_criteria")
        .unwrap();
    assert_eq!(incl["status"], "drafted");
    assert_eq!(incl["source_cited"], true);
    let auth = d["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["section_id"] == "authorization")
        .unwrap();
    assert_eq!(auth["completed_by"], "medical_director");
    assert_eq!(auth["status"], "placeholder");
    // Element 4 (the signature) stays open until the medical director signs.
    let e4 = d["ar9324_elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["number"] == 4)
        .unwrap();
    assert_eq!(e4["status"], "open");
    assert_eq!(d["vaccine_elements"].as_array().unwrap().len(), 9);
    // The response does not echo the nurse's text.
    let all = everything(&h.call("draft_standing_orders", json!({"order_type": "vaccine",
        "sections": [{"section_id": "inclusion_criteria", "text": "Everyone six months of age and older.",
                      "source_citation": "Example guideline, 2026-27 edition"}], "write_document": false})).await);
    assert!(!all.contains("six months of age"), "{all}");
    let file = h.data().join(d["document_file"].as_str().unwrap());
    let doc = unzip(&file, "word/document.xml");
    assert!(doc.contains("Everyone six months of age and older."));
    assert!(doc.contains("Example guideline, 2026-27 edition"));
    assert!(
        doc.contains("[MEDICAL DIRECTOR: signature]") && doc.contains("[NURSE: name and title]")
    );
    assert!(doc.contains("[NURSE: contraindications and precautions"));
    assert!(doc.contains("Immunize.org"));
    // No bundled vaccine or drug names or doses in a draft with no sections.
    let s = call_ok(
        &h,
        "draft_standing_orders",
        json!({"order_type": "medication"}),
    )
    .await;
    let file = h.data().join(s["data"]["document_file"].as_str().unwrap());
    let doc = unzip(&file, "word/document.xml");
    for word in [
        "mg",
        "mL",
        "epinephrine",
        "naloxone",
        "Fluzone",
        "intramuscular",
    ] {
        assert!(!doc.contains(word), "{word} appears in an empty draft");
    }
    assert_eq!(
        s["data"]["placeholder_sections"],
        s["data"]["sections"].as_array().unwrap().len()
    );
    assert!(
        s["data"]["dph_orders_to_check_first"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn standing_order_rules_are_enforced() {
    let h = start(|c| c.offline = true).await;
    // Clinical text needs a cited source.
    let e = call_err(
        &h,
        "draft_standing_orders",
        json!({"order_type": "vaccine", "sections": [{"section_id": "contraindications_precautions", "text": "Severe allergy."}]}),
    )
    .await;
    assert!(e.contains("source_citation"), "{e}");
    // The medical director signs; the authorization block cannot be typed.
    call_err(&h, "draft_standing_orders",
        json!({"order_type": "activity", "sections": [{"section_id": "authorization", "text": "Signed Dr. X"}]})).await;
    // A section that does not belong to the type.
    let e = call_err(&h, "draft_standing_orders",
        json!({"order_type": "activity", "sections": [{"section_id": "dose_route", "text": "x", "source_citation": "y"}]})).await;
    assert!(e.contains("not a section of this activity"), "{e}");
    call_err(&h, "draft_standing_orders", json!({})).await;
    call_err(
        &h,
        "draft_standing_orders",
        json!({"order_type": "vaccine", "review_interval_months": 12}),
    )
    .await;
    call_err(&h, "draft_standing_orders",
        json!({"order_type": "vaccine", "effective_date": "2026-10-01", "review_interval_months": 99})).await;
    call_err(
        &h,
        "draft_standing_orders",
        json!({"order_type": "vaccine", "effective_date": "1800-10-01"}),
    )
    .await;
    // Identifiers are refused without being repeated.
    let e = call_err(
        &h,
        "draft_standing_orders",
        json!({"order_type": "activity", "sections": [{"section_id": "title_scope",
               "text": "For Maria Garcia, DOB 04/11/1953, 837 Elm Street, MRN400000"}]}),
    )
    .await;
    assert!(!e.contains("837 Elm") && !e.contains("MRN400000"), "{e}");
    // Wording that conflicts with Advisory Ruling 9324's limits is flagged.
    let s = call_ok(
        &h,
        "draft_standing_orders",
        json!({"order_type": "medication", "write_document": false,
               "sections": [{"section_id": "procedure_notes",
                             "text": "The nurse may choose between the two products and may increase the dose; refill as needed.",
                             "source_citation": "Example protocol, 2026"}]}),
    )
    .await;
    let limits: Vec<&str> = s["data"]["wording_flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["limit"].as_str().unwrap())
        .collect();
    for l in ["menu_of_choices", "change_to_the_order", "prescribing"] {
        assert!(limits.contains(&l), "{l} {limits:?}");
    }
    assert!(s["data"]["document_file"].is_null());
}

#[tokio::test]
async fn schemas_describe_the_three_tools() {
    let h = start(|c| c.offline = true).await;
    let tools = h.client.list_all_tools().await.unwrap();
    for n in [
        "assess_workforce_competencies",
        "board_of_health_meeting_assistant",
        "draft_standing_orders",
    ] {
        let t = tools.iter().find(|t| t.name == n).unwrap();
        assert!(t.output_schema.is_some(), "{n}");
        let desc = t.description.as_deref().unwrap_or_default();
        assert!(desc.contains("pending expert review"), "{n}");
        assert!(
            desc.contains("Decision support") || desc.contains("decision support"),
            "{n}"
        );
    }
}
