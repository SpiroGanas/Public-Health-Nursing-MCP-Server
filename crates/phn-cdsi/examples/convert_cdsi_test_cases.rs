//! Convert CDC's CDSi test-case workbooks into the JSON fixture used by
//! `tests/cdsi_test_cases.rs`.
//!
//! ```text
//! cargo run -p phn-cdsi --example convert_cdsi_test_cases -- \
//!     cdsi-healthy-childhood-and-adult-test-cases-v4.46.xlsx \
//!     CDSi-underlying-conditions-test-cases-v4.6.xlsx \
//!     > crates/phn-cdsi/tests/fixtures/cdsi_test_cases.json
//! ```
//!
//! Download the workbooks from <https://www.cdc.gov/iis/cdsi/index.html>.
//! Every row of the "FITS Exported TestCases" sheet (healthy childhood and
//! adult) and the "Underlying Condition Test Cases" sheet is converted; the
//! output records each workbook's SHA-256 so the fixture can be traced to
//! the exact download. Dates become ISO 8601; empty cells are omitted.
//!
//! With `--names-out PATH` the converter also writes the vaccine names the
//! workbooks pair with a CVX code (trade names such as "PEDIARIX"), keeping
//! only names that always map to the same CVX code. The engine uses that
//! file (`data/vaccine-names-from-test-cases.json`) to resolve names typed
//! by the nurse.

use std::collections::{BTreeMap, BTreeSet};

use calamine::{Data, Reader};
use serde_json::{Map, Value, json};
use sha2::Digest;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let names_out = args.iter().position(|a| a == "--names-out").map(|i| {
        let path = args.get(i + 1).cloned().unwrap_or_default();
        args.drain(i..(i + 2).min(args.len()));
        path
    });
    let paths = args;
    if paths.is_empty() {
        eprintln!("usage: convert_cdsi_test_cases [--names-out PATH] <workbook.xlsx>...");
        std::process::exit(2);
    }
    let mut sources = Vec::new();
    let mut cases = Vec::new();
    for path in &paths {
        let bytes = std::fs::read(path)?;
        let sha = hex::encode(sha2::Sha256::digest(&bytes));
        let file_name = std::path::Path::new(path)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut wb = calamine::open_workbook_auto(path)?;
        let (sheet, suite) = if wb
            .sheet_names()
            .iter()
            .any(|s| s == "Underlying Condition Test Cases")
        {
            ("Underlying Condition Test Cases", "underlying_conditions")
        } else {
            ("FITS Exported TestCases", "healthy")
        };
        let range = wb.worksheet_range(sheet)?;
        let mut rows = range.rows();
        let header: Vec<String> = rows
            .next()
            .ok_or("empty sheet")?
            .iter()
            .map(|c| cell_text(c).unwrap_or_default())
            .collect();
        let mut n = 0;
        for row in rows {
            let get = |name: &str| -> Option<String> {
                let i = header.iter().position(|h| h.eq_ignore_ascii_case(name))?;
                row.get(i).and_then(cell_text)
            };
            let date = |name: &str| -> Option<String> { row_date(&header, row, name) };
            let Some(id) = get("CDC_Test_ID") else {
                continue;
            };
            let mut observations = Vec::new();
            for k in 1..=3 {
                if let Some(code) = get(&format!("Observation_Code_{k}")) {
                    observations.push(json!({
                        "code": code,
                        "text": get(&format!("Observation_Text_{k}")),
                        "date": date(&format!("Observation_Date_{k}")),
                    }));
                }
            }
            let mut doses = Vec::new();
            for k in 1..=7 {
                let Some(given) = date(&format!("Date_Administered_{k}")) else {
                    continue;
                };
                let mut d = Map::new();
                d.insert("date".into(), json!(given));
                for (field, col) in [
                    ("vaccine_name", "Vaccine_Name"),
                    ("cvx", "CVX"),
                    ("mvx", "MVX"),
                    ("expected_status", "Evaluation_Status"),
                    ("expected_reason", "Evaluation_Reason"),
                    ("series_type", "Series_Type"),
                ] {
                    if let Some(v) = get(&format!("{col}_{k}")) {
                        d.insert(field.into(), json!(v));
                    }
                }
                doses.push(Value::Object(d));
            }
            let mut case = Map::new();
            let mut put = |k: &str, v: Option<String>| {
                if let Some(v) = v {
                    case.insert(k.into(), json!(v));
                }
            };
            put("id", Some(id));
            put("suite", Some(suite.to_string()));
            put("name", get("Test_Case_Name"));
            put("dob", date("DOB"));
            put("gender", get("Gender"));
            put("assessment_date", date("Assessment_Date"));
            put("vaccine_group", get("Vaccine_Group"));
            put("series_status", get("Series_Status"));
            put("forecast_number", get("Forecast_#"));
            put("earliest", date("Earliest_Date"));
            put("recommended", date("Recommended_Date"));
            put("past_due", date("Past_Due_Date"));
            put("evaluation_test_type", get("Evaluation_Test_Type"));
            put("forecast_test_type", get("Forecast_Test_Type"));
            put("changed_in_version", get("Changed_In_Version"));
            put("description", get("General_Description"));
            case.insert("observations".into(), Value::Array(observations));
            case.insert("doses".into(), Value::Array(doses));
            cases.push(Value::Object(case));
            n += 1;
        }
        sources.push(json!({
            "file": file_name,
            "sheet": sheet,
            "suite": suite,
            "sha256": sha,
            "cases": n,
        }));
    }
    // Stable order: suite, then id.
    cases.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["suite"].as_str().unwrap_or("").to_string(),
                v["id"].as_str().unwrap_or("").to_string(),
            )
        };
        key(a).cmp(&key(b))
    });
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in &cases {
        *counts
            .entry(c["suite"].as_str().unwrap_or("").to_string())
            .or_default() += 1;
    }
    if let Some(path) = names_out {
        let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for c in &cases {
            for d in c["doses"].as_array().into_iter().flatten() {
                if let (Some(n), Some(cvx)) = (d["vaccine_name"].as_str(), d["cvx"].as_str()) {
                    let name = n.split_whitespace().collect::<Vec<_>>().join(" ");
                    let cvx = cvx.trim().trim_start_matches('0').to_string();
                    names.entry(name.to_uppercase()).or_default().insert(cvx);
                }
            }
        }
        let unambiguous: BTreeMap<String, String> = names
            .into_iter()
            .filter(|(_, c)| c.len() == 1)
            .filter_map(|(n, c)| c.into_iter().next().map(|cvx| (n, cvx)))
            .collect();
        let doc = json!({
            "about": "Vaccine names paired with a single CVX code in CDC's CDSi test-case workbooks (https://www.cdc.gov/iis/cdsi/index.html). Generated by crates/phn-cdsi/examples/convert_cdsi_test_cases.rs --names-out; names that map to more than one CVX code are left out.",
            "sources": sources.clone(),
            "names": unambiguous,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&doc)? + "\n")?;
    }
    // One case per line keeps diffs reviewable.
    println!("{{");
    println!(
        "  \"about\": {},",
        json!(
            "CDC CDSi test cases converted by crates/phn-cdsi/examples/convert_cdsi_test_cases.rs. \
             Source: https://www.cdc.gov/iis/cdsi/index.html (CDC, public domain)."
        )
    );
    println!("  \"sources\": {},", Value::Array(sources));
    println!("  \"counts\": {},", json!(counts));
    println!("  \"cases\": [");
    for (i, c) in cases.iter().enumerate() {
        let comma = if i + 1 < cases.len() { "," } else { "" };
        println!("    {c}{comma}");
    }
    println!("  ]");
    println!("}}");
    Ok(())
}

fn cell_text(c: &Data) -> Option<String> {
    let s = match c {
        Data::Empty => return None,
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
        Data::Float(f) => f.to_string(),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => serial_to_iso(dt.as_f64()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(_) => return None,
    };
    (!s.is_empty()).then_some(s)
}

fn row_date(header: &[String], row: &[Data], name: &str) -> Option<String> {
    let i = header.iter().position(|h| h.eq_ignore_ascii_case(name))?;
    match row.get(i)? {
        Data::DateTime(dt) => Some(serial_to_iso(dt.as_f64())),
        Data::Float(f) => Some(serial_to_iso(*f)),
        Data::DateTimeIso(s) => Some(s.chars().take(10).collect()),
        Data::String(s) => {
            let s = s.trim();
            chrono::NaiveDate::parse_from_str(s, "%m/%d/%Y")
                .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d"))
                .ok()
                .map(|d| d.to_string())
        }
        _ => None,
    }
}

fn serial_to_iso(serial: f64) -> String {
    let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).expect("valid date");
    (base + chrono::Duration::days(serial.floor() as i64)).to_string()
}
