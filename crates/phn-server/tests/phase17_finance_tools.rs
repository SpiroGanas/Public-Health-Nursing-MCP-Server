//! MCP integration tests for the Phase 17 finance tools (LEAD-01, LEAD-02): results match the
//! independent Python computation (generator committed beside the expected values), the budget
//! tool returns category totals only (no vendors, employees, account codes, or one-person
//! salary lines), personnel amounts are hidden when the group is small, problems the nurse can
//! fix are tool errors, and the workbook and report land in the output folder.

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
        "fixtures/phase17_finance/expected_finance.json"
    ))
    .unwrap()
}

fn close(got: &Value, want: &Value, what: &str) {
    match (got.as_f64(), want.as_f64()) {
        (Some(g), Some(w)) => assert!((g - w).abs() <= 0.011, "{what}: got {g}, expected {w}"),
        (None, None) => {}
        _ => panic!("{what}: got {got}, expected {want}"),
    }
}

const LINE_FIELDS: [&str; 13] = [
    "budgeted",
    "actual",
    "encumbered",
    "remaining",
    "percent_of_budget",
    "expected_by_pace",
    "variance_vs_pace",
    "points_vs_pace",
    "forecast_straight_line",
    "forecast_run_rate",
    "projected_vs_budget_straight_line",
    "projected_vs_budget_run_rate",
    "status",
];

fn compare_line(got: &Value, want: &Value, what: &str) {
    for f in LINE_FIELDS {
        if f == "status" {
            if !want["status"].is_null() {
                assert_eq!(got["status"], want["status"], "{what} status");
            }
            continue;
        }
        // The tool leaves out run-rate fields for files without months; null in the reference.
        let g = got.get(f).cloned().unwrap_or(Value::Null);
        close(&g, &want[f], &format!("{what} {f}"));
    }
}

fn line<'a>(s: &'a Value, section: &str, category: &str) -> &'a Value {
    s["data"][section]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["category"] == category)
        .unwrap_or_else(|| panic!("no {section} line {category}"))
}

fn personnel(s: &Value) -> &Value {
    s["data"]["expenses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["is_personnel"] == true)
        .expect("a personnel line")
}

const SEEDS: [&str; 3] = [
    "data/health_dept_budget.csv",
    "data/health_dept_budget_small.csv",
    "data/health_dept_ledger_monthly.csv",
];

fn budget_args() -> Value {
    json!({
        "path": "health_dept_budget.csv", "as_of_date": "2026-10-31",
        "positions_column": "Positions", "department_name": "Board of Health",
        "population": 24000, "indirect_rate_percent": 10,
        "services": [
            {"name": "Immunization clinics", "volume": 640, "planned_annual_volume": 1800,
             "costs": [{"category": "Vaccines and Medical Supplies"},
                       {"category": "Personnel", "share_percent": 25}]},
            {"name": "Inspections", "volume": 210,
             "costs": [{"category": "Supplies", "share_percent": 50},
                       {"category": "Travel and Training"}]}
        ]
    })
}

// ---------------------------------------------------------------- LEAD-01

#[tokio::test]
async fn budget_matches_the_independent_reference_values() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "analyze_health_department_budget", budget_args()).await;
    let exp = expected();
    let e = &exp["summary"];
    let d = &s["data"];
    // Fiscal year: July 1 start by default, days counted with the as-of date.
    assert_eq!(d["fiscal_year"]["label"], "FY2027");
    assert_eq!(d["fiscal_year"]["start"], e["fiscal_year"]["start"]);
    assert_eq!(d["fiscal_year"]["end"], e["fiscal_year"]["end"]);
    assert_eq!(d["fiscal_year"]["days_elapsed"], e["fiscal_year"]["days_elapsed"]);
    assert_eq!(d["fiscal_year"]["days_in_year"], 365);
    close(
        &d["fiscal_year"]["percent_elapsed"],
        &e["fiscal_year"]["percent_elapsed"],
        "percent elapsed",
    );
    assert_eq!(d["fiscal_year"]["start_month_is_default"], true);
    assert_eq!(d["layout"], "one row per account line");
    // Every expense category matches, with personnel as one combined row.
    for (cat, want) in e["expenses"].as_object().unwrap() {
        let got = if cat == "PERSONNEL" {
            personnel(&s)
        } else {
            line(&s, "expenses", cat)
        };
        compare_line(got, want, cat);
    }
    assert_eq!(
        d["expenses"].as_array().unwrap().len(),
        e["expenses"].as_object().unwrap().len(),
        "the four salary and overtime lines are one row"
    );
    assert_eq!(d["personnel"]["amounts_shown"], true);
    assert_eq!(d["personnel"]["position_count_basis"], "positions column");
    assert_eq!(d["personnel"]["categories_combined"], 2);
    compare_line(&d["expense_totals"], &e["expense_totals"], "expense totals");
    assert_eq!(d["expense_totals"]["excludes_personnel"], false);
    for (cat, want) in e["revenue"].as_object().unwrap() {
        compare_line(line(&s, "revenue", cat), want, cat);
    }
    compare_line(&d["revenue_totals"], &e["revenue_totals"], "revenue totals");
    // Revenue mix, including the Public Health Excellence group.
    for (g, want) in e["mix"].as_object().unwrap() {
        let got = d["revenue_mix"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["group_id"] == g.as_str())
            .unwrap_or_else(|| panic!("no group {g}"));
        close(&got["budgeted"], &want["budgeted"], g);
        close(&got["actual"], &want["actual"], g);
        close(
            &got["percent_of_budgeted_revenue"],
            &want["percent_of_budgeted"],
            g,
        );
        close(
            &got["percent_of_actual_revenue"],
            &want["percent_of_actual"],
            g,
        );
    }
    assert_eq!(
        d["unclassified_revenue_labels"],
        json!(["Sale of publications"])
    );
    // Net position.
    close(&d["net"]["budgeted"], &e["net"]["budgeted"], "net budgeted");
    close(&d["net"]["actual_ytd"], &e["net"]["actual_ytd"], "net ytd");
    // Cost per service with allocation shares and the indirect rate.
    for (name, want) in e["services"].as_object().unwrap() {
        let got = d["services"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["service"] == name.as_str())
            .unwrap();
        close(&got["total_cost"], &want["total_cost"], name);
        close(&got["cost_per_unit"], &want["cost_per_unit"], name);
        close(
            got.get("budgeted_cost_per_unit").unwrap_or(&Value::Null),
            &want["budgeted_cost_per_unit"],
            name,
        );
    }
    // Per resident, beside DPH's published figures (medium confidence for the estimate).
    let pc = &d["per_capita"];
    close(
        &pc["municipal_appropriation_budget_per_resident"],
        &json!(160000.0 / 24000.0),
        "municipal per resident",
    );
    let pubs = pc["published"].as_array().unwrap();
    assert!(pubs.iter().any(|p| p["dollars_per_resident"] == 36.98 && p["confidence"] == "medium"));
    assert!(pubs.iter().any(|p| p["dollars_per_resident"] == 17.56));
    // No month column: no run rate, said plainly.
    assert!(d["expenses"][0].get("forecast_run_rate").is_none());
    assert!(
        d["forecast_assumptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a.as_str().unwrap().contains("Run rate: not available"))
    );
    // PHE content is cited and pending review.
    let phe = d["public_health_excellence"].as_array().unwrap();
    assert!(phe.iter().any(|p| p["text"].as_str().unwrap().contains("26.6 million")));
    assert!(phe.iter().all(|p| p["citation"].as_str().unwrap().contains(',')));
    assert!(pending(&s));
    assert!(s["notice"].is_string(), "decision support notice");
    let src: Vec<&str> = s["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["title"].as_str().unwrap())
        .collect();
    assert!(src.contains(&"Public Health Excellence Grant Program"));
    assert!(src.iter().any(|t| t.contains("Municipal Finance Glossary")));
    for x in s["sources"].as_array().unwrap() {
        if x["mode"] == "snapshot" {
            assert!(x["retrieved"].as_str().unwrap().starts_with("2026-"));
        }
    }
}

#[tokio::test]
async fn a_monthly_ledger_gives_the_run_rate_forecast() {
    let h = start(|c| c.offline = true).await;
    let exp = expected();
    for (key, as_of) in [("ledger", "2026-10-31"), ("ledger_mid_month", "2026-10-15")] {
        let s = call_ok(
            &h,
            "analyze_health_department_budget",
            json!({"path": "health_dept_ledger_monthly.csv", "period_column": "Month",
                   "account_column": "Account", "positions_column": "Positions",
                   "as_of_date": as_of, "write_workbook": false, "write_report": false}),
        )
        .await;
        let e = &exp[key];
        let d = &s["data"];
        assert_eq!(d["layout"], "monthly ledger", "{key}");
        close(
            &d["fiscal_year"]["percent_elapsed"],
            &e["percent_elapsed"],
            key,
        );
        for (cat, want) in e["expenses"].as_object().unwrap() {
            let got = if cat == "PERSONNEL" {
                personnel(&s)
            } else {
                line(&s, "expenses", cat)
            };
            compare_line(got, want, &format!("{key} {cat}"));
        }
        for (cat, want) in e["revenue"].as_object().unwrap() {
            compare_line(line(&s, "revenue", cat), want, &format!("{key} {cat}"));
        }
        compare_line(&d["expense_totals"], &e["expense_totals"], key);
        compare_line(&d["revenue_totals"], &e["revenue_totals"], key);
        // The three most recent complete months feed the run rate.
        let text = d["forecast_assumptions"].to_string();
        for m in e["recent_months"].as_array().unwrap() {
            assert!(text.contains(m.as_str().unwrap()), "{key}: {text}");
        }
        // The annual budget is counted once per account, not once per month.
        assert_eq!(line(&s, "expenses", "Supplies")["budgeted"], 12000.0);
        assert_eq!(line(&s, "expenses", "Contracted Services")["budgeted"], 30000.0);
        let everything = everything_text(&s);
        for f in SEEDS {
            assert!(leaks(&everything, f, None).is_empty(), "{key}: {f}");
        }
    }
    // The mid-month file leaves out no rows (October is the as-of month), and a date in
    // September leaves October out.
    let s = call_ok(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_ledger_monthly.csv", "period_column": "Month",
               "account_column": "Account", "positions_column": "Positions",
               "as_of_date": "2026-09-30", "write_workbook": false, "write_report": false}),
    )
    .await;
    assert_eq!(s["data"]["rows_outside_year"], 8);
    assert_eq!(line(&s, "expenses", "Supplies")["actual"], 1500.0);
    // A federal year that has already ended before July: the same rows fall outside it.
    let s = call_ok(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_ledger_monthly.csv", "period_column": "Month",
               "account_column": "Account", "positions_column": "Positions",
               "as_of_date": "2026-10-31", "fiscal_year_start_month": 10,
               "write_workbook": false, "write_report": false}),
    )
    .await;
    assert_eq!(s["data"]["fiscal_year"]["label"], "FY2027");
    assert_eq!(s["data"]["fiscal_year"]["start"], "2026-10-01");
    assert_eq!(s["data"]["fiscal_year"]["start_month_is_default"], false);
    assert_eq!(line(&s, "expenses", "Supplies")["actual"], 900.0);
}

fn everything_text(s: &Value) -> String {
    format!("{}\n{}", s["summary"], s)
}

#[tokio::test]
async fn a_personnel_group_with_fewer_than_three_positions_is_hidden_everywhere() {
    let h = start(|c| c.offline = true).await;
    let exp = expected();
    let e = &exp["small"];
    let s = call_ok(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_budget_small.csv", "as_of_date": "2026-10-31",
               "positions_column": "Positions", "department_name": "Board of Health",
               "services": [{"name": "Immunization clinics", "volume": 300,
                             "costs": [{"category": "Supplies"},
                                       {"category": "Personnel", "share_percent": 50}]}]}),
    )
    .await;
    let d = &s["data"];
    assert_eq!(d["personnel"]["amounts_shown"], false);
    let p = personnel(&s);
    assert_eq!(p["hidden"], true);
    for f in ["budgeted", "actual", "remaining", "forecast_straight_line"] {
        assert!(p[f].is_null(), "{f} must be hidden");
    }
    // Totals leave personnel out and equal the reference value without it.
    assert_eq!(d["expense_totals"]["excludes_personnel"], true);
    compare_line(&d["expense_totals"], &e["expense_totals"], "small totals");
    assert!(d["net"].is_null());
    // A service that depends on the hidden pool is withheld.
    let sv = &d["services"][0];
    assert!(sv["cost_per_unit"].is_null());
    assert!(sv["withheld_reason"].as_str().unwrap().contains("personnel"));
    assert_eq!(s["privacy"]["suppression_applied"], true);
    assert_eq!(s["privacy"]["row_level_data"], false);
    assert!(
        d["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["message"].as_str().unwrap().contains("hidden"))
    );
    // No salary figure appears in the response, the workbook, or the report: not the lines
    // (101,000; 64,000; 33,900; 21,300) and not the pool (165,000 budgeted; 55,200 spent).
    let secrets = ["101000", "64000", "33900", "21300", "165000", "55200", "101,000", "165,000", "55,200"];
    let all = everything_text(&s);
    for x in secrets {
        assert!(!all.contains(x), "response shows {x}");
    }
    let mut files_text = String::new();
    for f in ["workbook_file", "report_file"] {
        let path = h.data().join(d[f].as_str().unwrap());
        assert!(path.exists(), "{f}");
        let parts = if f == "workbook_file" {
            vec!["xl/worksheets/sheet2.xml", "xl/worksheets/sheet1.xml", "xl/sharedStrings.xml"]
        } else {
            vec!["word/document.xml"]
        };
        for part in parts {
            let o = std::process::Command::new("unzip")
                .args(["-p", path.to_str().unwrap(), part])
                .output()
                .expect("unzip");
            files_text.push_str(&String::from_utf8_lossy(&o.stdout));
        }
    }
    assert!(files_text.contains("Supplies"));
    for x in secrets {
        assert!(!files_text.contains(x), "files show {x}");
    }
    for f in SEEDS {
        assert!(leaks(&files_text, f, None).is_empty(), "files leak {f}");
        assert!(leaks(&all, f, None).is_empty(), "response leaks {f}");
    }
}

#[tokio::test]
async fn without_a_position_count_salary_accounts_are_a_labelled_proxy() {
    let h = start(|c| c.offline = true).await;
    // The big file has four salary and overtime accounts; with no positions column the count
    // of accounts is the proxy, and the response says so.
    let s = call_ok(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_budget.csv", "as_of_date": "2026-10-31",
               "account_column": "Account", "write_workbook": false, "write_report": false}),
    )
    .await;
    let p = &s["data"]["personnel"];
    assert_eq!(p["amounts_shown"], true);
    assert!(p["position_count_basis"].as_str().unwrap().contains("proxy"));
    assert!(caveats(&s).contains("can overstate it"));
    // The nurse's own headcount overrides the proxy.
    let s = call_ok(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_budget.csv", "as_of_date": "2026-10-31",
               "personnel_headcount": 2, "write_workbook": false, "write_report": false}),
    )
    .await;
    assert_eq!(s["data"]["personnel"]["amounts_shown"], false);
    assert_eq!(s["data"]["personnel"]["position_count_basis"], "headcount you gave");
}

#[tokio::test]
async fn budget_writes_a_workbook_with_live_formulas_and_a_summary_report() {
    let h = start(|c| c.offline = true).await;
    let mut args = budget_args();
    args["period_column"] = Value::Null;
    let s = call_ok(&h, "analyze_health_department_budget", args).await;
    let d = &s["data"];
    let report = h.data().join(d["report_file"].as_str().unwrap());
    let book = h.data().join(d["workbook_file"].as_str().unwrap());
    assert!(report.exists() && report.extension().unwrap() == "docx");
    assert!(book.exists() && book.extension().unwrap() == "xlsx");
    assert_eq!(s["privacy"]["files_written"].as_array().unwrap().len(), 2);
    let sheet = |n: u8| {
        let o = std::process::Command::new("unzip")
            .args(["-p", book.to_str().unwrap(), &format!("xl/worksheets/sheet{n}.xml")])
            .output()
            .expect("unzip");
        String::from_utf8_lossy(&o.stdout).to_string()
    };
    // Assumptions: the share of year elapsed is a formula on dates.
    let a = sheet(1);
    assert!(a.contains("<f>EDATE(B3,12)-B3</f>"), "{}", &a[..a.len().min(1500)]);
    assert!(a.contains("<f>MIN(1,(B4-B3+1)/B5)</f>"));
    // Expenses: variance, pace, status, and forecast are formulas over the inputs.
    let x = sheet(2);
    assert!(x.contains("<f>B5-C5-D5</f>"), "{}", &x[..x.len().min(2500)]);
    assert!(x.contains("<f>B5*Assumptions!$B$6</f>"));
    assert!(x.contains("Assumptions!$B$7"));
    assert!(x.contains("\"Over budget\""));
    assert!(x.contains("<f>IF(Assumptions!$B$6=0,&quot;&quot;,C5/Assumptions!$B$6)</f>")
        || x.contains("C5/Assumptions!$B$6"));
    assert!(x.contains("SUM(B5:B"), "totals are SUM formulas");
    assert!(sheet(3).contains("Assumptions!$B$6"));
    assert!(sheet(4).contains("SUM(B5:B"));
    // The report has the totals and the cited PHE statements, and no vendors or employees.
    let doc = std::process::Command::new("unzip")
        .args(["-p", report.to_str().unwrap(), "word/document.xml"])
        .output()
        .expect("unzip");
    let text = String::from_utf8_lossy(&doc.stdout).to_string();
    assert!(text.contains("Board of Health") && text.contains("FY2027"));
    assert!(text.contains("Public Health Excellence"));
    assert!(text.contains("Revenue mix"));
    for f in SEEDS {
        assert!(leaks(&text, f, None).is_empty(), "report leaks {f}");
    }
    // pdf format and no files are options.
    let s = call_ok(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_budget.csv", "as_of_date": "2026-10-31", "format": "pdf",
               "write_workbook": false}),
    )
    .await;
    assert!(s["data"]["workbook_file"].is_null());
    assert!(s["data"]["report_file"].as_str().unwrap().ends_with(".pdf"));
}

#[tokio::test]
async fn budget_refuses_vendor_employee_and_identifier_columns_without_echoing_values() {
    let h = start(|c| c.offline = true).await;
    for (col_arg, col) in [
        ("category_column", "Vendor"),
        ("category_column", "Employee"),
        ("account_column", "Vendor Contact Email"),
        ("source_column", "Vendor Phone"),
    ] {
        let mut args = json!({"path": "health_dept_budget.csv", "as_of_date": "2026-10-31"});
        args[col_arg] = json!(col);
        let e = call_err(&h, "analyze_health_department_budget", args).await;
        assert!(
            e.contains("never reads") || e.contains("cannot be analyzed"),
            "{col}: {e}"
        );
        assert!(leaks(&e, "data/health_dept_budget.csv", None).is_empty(), "{col}: {e}");
    }
    // A client line list is not a budget: its identifier columns are refused when named, and the
    // error never shows a value.
    for args in [
        json!({"path": "line_list.csv", "category_column": "Last Name", "budget_column": "Age",
               "actual_column": "Age"}),
        json!({"path": "line_list.csv", "category_column": "Town", "budget_column": "Phone",
               "actual_column": "Age"}),
        json!({"path": "line_list.csv", "category_column": "Town", "budget_column": "Age",
               "actual_column": "SSN"}),
    ] {
        let e = call_err(&h, "analyze_health_department_budget", args).await;
        assert!(leaks(&e, "data/line_list.csv", None).is_empty(), "{e}");
    }
    // Without names, the line list has no budget column to find.
    let e = call_err(
        &h,
        "analyze_health_department_budget",
        json!({"path": "line_list.csv"}),
    )
    .await;
    assert!(leaks(&e, "data/line_list.csv", None).is_empty(), "{e}");
}

#[tokio::test]
async fn budget_explains_problems_the_nurse_can_fix() {
    let h = start(|c| c.offline = true).await;
    let base = json!({"path": "health_dept_budget.csv", "as_of_date": "2026-10-31"});
    let with = |k: &str, v: Value| {
        let mut a = base.clone();
        a[k] = v;
        a
    };
    let cases = [
        (json!({}), "path is required"),
        (with("as_of_date", json!("10/31/2026")), "as_of_date"),
        (with("as_of_date", json!("3026-10-31")), "as_of_date"),
        (with("fiscal_year_start_month", json!(13)), "fiscal_year_start_month"),
        (with("pace_tolerance_points", json!(-1)), "pace_tolerance_points"),
        (with("run_rate_months", json!(0)), "run_rate_months"),
        (with("population", json!(0)), "population"),
        (with("budget_column", json!("Nope")), "no column named"),
        (with("period_column", json!("Account")), "are not months"),
        (with("indirect_rate_percent", json!(500)), "indirect_rate_percent"),
        (
            with("services", json!([{"name": "X", "volume": 10, "costs": [{"category": "Nope"}]}])),
            "Categories:",
        ),
        (
            with("services", json!([{"name": "X", "volume": 0, "costs": [{"category": "Supplies"}]}])),
            "volume must be more than 0",
        ),
        (
            with("services", json!([{"name": "X", "volume": 5, "costs": []}])),
            "at least one cost category",
        ),
        (
            with("services", json!([{"name": "X", "volume": 5, "costs": [
                {"category": "Salaries and Wages"}, {"category": "Overtime"}]}])),
            "one combined pool",
        ),
        (
            with("revenue_group_overrides", json!([{"label": "x", "group": "bogus"}])),
            "is not one of",
        ),
        (
            with("services", json!([{"name": "Call 413-555-0199", "volume": 5,
                                     "costs": [{"category": "Supplies"}]}])),
            "identifier",
        ),
        (with("path", json!("../etc/passwd")), ""),
        (with("path", json!("client_note.txt")), ""),
    ];
    for (args, needle) in cases {
        let e = call_err(&h, "analyze_health_department_budget", args.clone()).await;
        assert!(e.contains(needle), "{args}: {e}");
        assert!(!e.contains("413-555-0199"), "{e}");
    }
}

#[tokio::test]
async fn revenue_groups_can_be_placed_by_hand_and_a_file_without_types_skips_revenue() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_budget.csv", "as_of_date": "2026-10-31",
               "positions_column": "Positions",
               "revenue_group_overrides": [{"label": "sale of publications", "group": "fees"}],
               "write_workbook": false, "write_report": false}),
    )
    .await;
    assert!(s["data"]["unclassified_revenue_labels"].as_array().unwrap().is_empty());
    let fees = s["data"]["revenue_mix"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["group_id"] == "fees")
        .unwrap();
    close(&fees["budgeted"], &json!(42000.0 + 8000.0 + 1000.0), "fees with the override");
    // A type column that is not revenue or expense is explained.
    let e = call_err(
        &h,
        "analyze_health_department_budget",
        json!({"path": "health_dept_budget.csv", "type_column": "Fund/Source"}),
    )
    .await;
    assert!(e.contains("not revenue or expense"), "{e}");
}

// ---------------------------------------------------------------- LEAD-02

fn staff_args() -> Value {
    json!({
        "program_name": "Public Health Nursing",
        "services": [
            {"name": "Home visits", "annual_volume": 1200, "minutes_per_unit": 45},
            {"name": "Clinic doses", "annual_volume": 3000, "minutes_per_unit": 6},
            {"name": "TB case management", "annual_volume": 80, "minutes_per_unit": 120},
            {"name": "Communicable disease investigations", "annual_volume": 150, "minutes_per_unit": 60}
        ],
        "current_fte": 1.5, "population": 24000, "clients": 340,
        "scenarios": [
            {"name": "10% growth", "volume_growth_percent": 10},
            {"name": "Add diabetes prevention",
             "added_services": [{"name": "Diabetes prevention sessions", "annual_volume": 120, "minutes_per_unit": 90}]},
            {"name": "Growth plus added", "volume_growth_percent": 5,
             "added_services": [
                {"name": "Diabetes prevention sessions", "annual_volume": 120, "minutes_per_unit": 90},
                {"name": "School vision screening", "annual_volume": 900, "minutes_per_unit": 4}]}
        ]
    })
}

fn compare_staffing(s: &Value, e: &Value, what: &str) {
    let d = &s["data"];
    close(&d["productive_hours"]["productive_hours_per_fte"], &e["productive_hours_per_fte"], what);
    close(&d["productive_hours"]["available_hours"], &e["available_hours"], what);
    close(&d["baseline_hours"], &e["baseline_hours"], what);
    for (name, want) in e["services"].as_object().unwrap() {
        let got = d["services"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["service"] == name.as_str())
            .unwrap();
        close(&got["hours"], &want["hours"], name);
        close(&got["percent_of_hours"], &want["percent_of_hours"], name);
    }
    let got = d["scenarios"].as_array().unwrap();
    let want = e["scenarios"].as_array().unwrap();
    assert_eq!(got.len(), want.len(), "{what}");
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g["name"], w["name"]);
        for f in [
            "added_hours",
            "total_hours",
            "fte_needed",
            "fte_rounded_up",
            "gap_vs_current_fte",
            "residents_per_fte_needed",
            "clients_per_fte_needed",
        ] {
            close(
                g.get(f).unwrap_or(&Value::Null),
                &w[f],
                &format!("{what} {} {f}", w["name"]),
            );
        }
    }
    if let Some(c) = e.get("current") {
        let cur = &d["current"];
        close(&cur["residents_per_fte"], &c["residents_per_fte"], what);
        close(&cur["clients_per_fte"], &c["clients_per_fte"], what);
        close(
            &cur["baseline_workload_percent_of_capacity"],
            &c["baseline_workload_percent_of_capacity"],
            what,
        );
    }
}

#[tokio::test]
async fn staffing_matches_the_independent_reference_values() {
    let h = start(|c| c.offline = true).await;
    let exp = expected();
    let s = call_ok(&h, "model_staffing_needs", staff_args()).await;
    compare_staffing(&s, &exp["staffing_default"], "default hours");
    // The default hours are placeholders, and the response says so.
    let basis: Vec<&str> = s["data"]["productive_hours"]["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["basis"].as_str().unwrap())
        .collect();
    assert!(basis[0].contains("arithmetic"));
    assert!(basis[1..].iter().all(|b| b.contains("tool default")), "{basis:?}");
    // Custom hours and a half-FTE rounding step.
    let mut args = staff_args();
    args["paid_hours_per_year"] = json!(1950);
    args["holiday_hours"] = json!(80);
    args["vacation_hours"] = json!(100);
    args["sick_personal_hours"] = json!(56);
    args["training_hours"] = json!(16);
    args["non_service_percent"] = json!(15);
    args["current_fte"] = json!(2);
    args["rounding_step_fte"] = json!(0.5);
    args["scenarios"] = json!([{"name": "10% growth", "volume_growth_percent": 10}]);
    let s = call_ok(&h, "model_staffing_needs", args).await;
    compare_staffing(&s, &exp["staffing_custom"], "custom hours");
    assert!(
        s["data"]["productive_hours"]["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["basis"] == "you gave this")
    );
}

#[tokio::test]
async fn staffing_bundles_no_benchmark_and_compares_only_with_the_nurses_own() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "model_staffing_needs", staff_args()).await;
    let d = &s["data"];
    assert_eq!(d["bundled_benchmarks"], "none");
    assert!(d["benchmark_comparison"].is_null());
    let nb: Vec<&str> = d["benchmarks_not_bundled"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["item"].as_str().unwrap())
        .collect();
    assert!(nb.iter().any(|n| n.contains("nurse to population ratio")));
    assert!(nb.iter().any(|n| n.contains("NACCHO")));
    assert!(summary_text(&s).contains("No public health nurse staffing benchmark is bundled"));
    assert!(caveats(&s).contains("No public health nurse staffing benchmark is bundled"));
    assert!(pending(&s));
    // The nurse's own benchmark is compared and labelled hers.
    let mut args = staff_args();
    args["benchmark"] = json!({"label": "Agency target", "source": "Board of Health policy 2025",
                               "population_per_fte": 8000, "clients_per_fte": 200});
    let s = call_ok(&h, "model_staffing_needs", args).await;
    let b = &s["data"]["benchmark_comparison"];
    close(&b["fte_implied_by_population"], &json!(3.0), "implied FTE");
    close(&b["fte_implied_by_clients"], &json!(1.7), "implied FTE");
    assert!(b["note"].as_str().unwrap().contains("did not verify"));
    assert!(summary_text(&s).contains("Board of Health policy 2025"));
    // A benchmark per resident needs a population.
    let mut args = staff_args();
    args["population"] = Value::Null;
    args["benchmark"] = json!({"label": "x", "source": "y", "population_per_fte": 8000});
    let e = call_err(&h, "model_staffing_needs", args).await;
    assert!(e.contains("needs population"), "{e}");
}

fn summary_text(s: &Value) -> String {
    s["summary"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn staffing_writes_a_workbook_with_live_formulas_and_a_report() {
    let h = start(|c| c.offline = true).await;
    let s = call_ok(&h, "model_staffing_needs", staff_args()).await;
    let d = &s["data"];
    let book = h.data().join(d["workbook_file"].as_str().unwrap());
    let report = h.data().join(d["report_file"].as_str().unwrap());
    assert!(book.exists() && report.exists());
    let sheet = |n: u8| {
        let o = std::process::Command::new("unzip")
            .args(["-p", book.to_str().unwrap(), &format!("xl/worksheets/sheet{n}.xml")])
            .output()
            .expect("unzip");
        String::from_utf8_lossy(&o.stdout).to_string()
    };
    assert!(sheet(1).contains("<f>(B3-B4-B5-B6-B7)*(1-B8/100)</f>"));
    assert!(sheet(2).contains("<f>B5*C5/60</f>"));
    let sc = sheet(3);
    assert!(sc.contains("Services!$D$9*(1+B5/100)+C5"), "{}", &sc[..sc.len().min(3000)]);
    assert!(sc.contains("D5/Assumptions!$B$9"));
    assert!(sc.contains("ROUNDUP(ROUND(F5/Assumptions!$B$10,6),0)*Assumptions!$B$10"));
    assert!(sc.contains("SUMIF("));
    let doc = std::process::Command::new("unzip")
        .args(["-p", report.to_str().unwrap(), "word/document.xml"])
        .output()
        .expect("unzip");
    let text = String::from_utf8_lossy(&doc.stdout).to_string();
    assert!(text.contains("Productive hours per FTE") && text.contains("Public Health Nursing"));
    assert!(text.contains("Benchmarks that were looked for and not used"));
}

#[tokio::test]
async fn staffing_reads_volumes_from_a_file_with_contact_people_without_returning_them() {
    let h = start(|c| c.offline = true).await;
    let exp = expected();
    let s = call_ok(
        &h,
        "model_staffing_needs",
        json!({"volumes_path": "staffing_volumes.csv", "current_fte": 1.5,
               "population": 24000, "clients": 340,
               "scenarios": [
                {"name": "10% growth", "volume_growth_percent": 10},
                {"name": "Add diabetes prevention",
                 "added_services": [{"name": "Diabetes prevention sessions", "annual_volume": 120, "minutes_per_unit": 90}]},
                {"name": "Growth plus added", "volume_growth_percent": 5,
                 "added_services": [
                    {"name": "Diabetes prevention sessions", "annual_volume": 120, "minutes_per_unit": 90},
                    {"name": "School vision screening", "annual_volume": 900, "minutes_per_unit": 4}]}]}),
    )
    .await;
    compare_staffing(&s, &exp["staffing_default"], "file");
    let all = format!("{s}");
    assert!(leaks(&all, "data/staffing_volumes.csv", None).is_empty(), "{all}");
    assert!(
        s["privacy"]["notes"]
            .to_string()
            .contains("Only the service, volume, and minutes columns")
    );
    assert_eq!(s["privacy"]["row_level_data"], false);
    // Naming a contact column is refused, with no value in the message.
    for col in ["Contact Person", "Contact Email"] {
        let e = call_err(
            &h,
            "model_staffing_needs",
            json!({"volumes_path": "staffing_volumes.csv", "service_column": col}),
        )
        .await;
        assert!(leaks(&e, "data/staffing_volumes.csv", None).is_empty(), "{e}");
    }
}

#[tokio::test]
async fn staffing_explains_problems_the_nurse_can_fix() {
    let h = start(|c| c.offline = true).await;
    let with = |k: &str, v: Value| {
        let mut a = staff_args();
        a[k] = v;
        a
    };
    let cases = [
        (json!({}), "give services"),
        (
            json!({"services": [{"name": "a", "annual_volume": 1, "minutes_per_unit": 1}], "volumes_path": "staffing_volumes.csv"}),
            "not both",
        ),
        (
            json!({"services": [{"name": "a", "annual_volume": 10, "minutes_per_unit": 0}]}),
            "minutes_per_unit",
        ),
        (
            json!({"services": [{"name": "a", "annual_volume": -3, "minutes_per_unit": 5}]}),
            "annual_volume",
        ),
        (
            json!({"services": [{"name": "a", "annual_volume": 3, "minutes_per_unit": 5},
                                {"name": "A", "annual_volume": 3, "minutes_per_unit": 5}]}),
            "listed twice",
        ),
        (
            json!({"services": [{"name": "Visit 413-555-0100", "annual_volume": 3, "minutes_per_unit": 5}]}),
            "identifier",
        ),
        (with("paid_hours_per_year", json!(0)), "paid_hours_per_year"),
        (with("vacation_hours", json!(5000)), "add up to all of the paid hours"),
        (with("non_service_percent", json!(100)), "non_service_percent"),
        (with("rounding_step_fte", json!(2)), "rounding_step_fte"),
        (with("current_fte", json!(-1)), "current_fte"),
        (
            with("scenarios", json!([{"name": "Baseline", "volume_growth_percent": 5}])),
            "repeats a name",
        ),
        (with("scenarios", json!([{"name": "Nothing"}])), "changes nothing"),
        (
            with("scenarios", json!([{"name": "x", "volume_growth_percent": -150}])),
            "volume_growth_percent",
        ),
        (with("volumes_path", json!("line_list.csv")), "not both"),
    ];
    for (args, needle) in cases {
        let e = call_err(&h, "model_staffing_needs", args.clone()).await;
        assert!(e.contains(needle), "{args}: {e}");
        assert!(!e.contains("413-555-0100"), "{e}");
    }
    // A client file as the volumes file: no service column to find, and no value shown.
    let e = call_err(
        &h,
        "model_staffing_needs",
        json!({"volumes_path": "line_list.csv"}),
    )
    .await;
    assert!(leaks(&e, "data/line_list.csv", None).is_empty(), "{e}");
}
