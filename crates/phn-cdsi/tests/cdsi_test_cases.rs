//! Runs every CDC CDSi test case (healthy childhood and adult v4.46, and
//! underlying conditions v4.6) through the engine.
//!
//! * The fixture `fixtures/cdsi_test_cases.json` is produced from CDC's
//!   workbooks by `examples/convert_cdsi_test_cases.rs` (see its docs).
//! * A case passes when every expected dose evaluation status for the
//!   case's vaccine group, the vaccine group status, the forecast dose
//!   number, and the earliest, recommended, and past-due dates all match.
//! * `fixtures/cdsi_known_exceptions.json` lists every case that does not
//!   pass, with the reason. The test fails on any failure that is not
//!   listed (a regression) and on any listed case that now passes (remove
//!   it from the list), so the exceptions stay exact.
//! * `docs/cdsi-test-results.md` is generated from the same run. The test
//!   fails when it is out of date; regenerate it with
//!   `PHN_UPDATE_CDSI_REPORT=1 cargo test -p phn-cdsi --test cdsi_test_cases`.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use phn_cdsi::{
    DoseInput, EngineResult, EvalStatus, Gender, Observation, PatientInput, SeriesStatus,
    VaccineGroupForecast,
};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/cdsi_test_cases.json");
const EXCEPTIONS: &str = include_str!("fixtures/cdsi_known_exceptions.json");

fn date(v: &Value) -> Option<NaiveDate> {
    v.as_str()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

/// Map the workbook's vaccine group labels to supporting-data names.
fn vaccine_group(label: &str) -> String {
    let l = label.trim();
    let mapped = match l.to_ascii_uppercase().as_str() {
        "DTAP" | "TD" | "TDAP" => "DTaP/Tdap/Td",
        "POL" | "IPOL" | "POLIO" => "Polio",
        "PCV" | "PNEUMOCOCCAL" => "Pneumococcal",
        "VAR" | "VARICELLA" => "Varicella",
        "ROTA" | "ROTAVIRUS" => "Rotavirus",
        "MENB" | "MENINGOCOCCAL B" => "Meningococcal B",
        "MCV" | "MENINGOCOCCAL" => "Meningococcal",
        "FLU" | "INFLUENZA" => "Influenza",
        "ZOSTER" => "Zoster",
        "HIB" => "Hib",
        "HEPB" => "HepB",
        "HEPA" => "HepA",
        _ => l,
    };
    mapped.to_string()
}

fn norm(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

#[derive(Debug, Clone)]
struct Outcome {
    id: String,
    suite: String,
    group: String,
    eval_type: String,
    forecast_type: String,
    mismatches: Vec<String>,
}

/// Choose the vaccine group forecast a case is about. CDSi can produce more
/// than one forecast for a vaccine group, one per series group (for example
/// the childhood and the age-50+ pneumococcal series, or a standard and a
/// risk series). The workbooks give one expected forecast per case, so the
/// harness picks, in order: the series type the suite is about (risk for
/// the underlying-conditions suite, standard for the healthy suite); the
/// forecast whose series counted the most valid doses; a series the
/// patient is old enough to start (minimum age to start); a forecast that
/// still needs a dose; the lowest series group number. Dose evaluations
/// are compared separately, against the series type each dose names.
fn pick<'a>(
    case: &Value,
    patient: &PatientInput,
    result: &EngineResult,
    forecasts: &[&'a VaccineGroupForecast],
) -> Option<&'a VaccineGroupForecast> {
    let want_risk = case["suite"] == "underlying_conditions";
    let sd = phn_cdsi::SupportingData::get();
    forecasts.iter().copied().min_by_key(|f| {
        let valid: usize = f
            .patient_series
            .iter()
            .map(|&s| result.patient_series[s].valid_doses)
            .sum();
        let too_young = f.patient_series.iter().any(|&s| {
            let ps = &result.patient_series[s];
            sd.antigens
                .get(&ps.antigen)
                .and_then(|a| a.series.iter().find(|x| x.name == ps.series_name))
                .and_then(|x| x.min_age_to_start)
                .is_some_and(|m| m.add_to(patient.dob) > patient.assessment_date)
        });
        (
            (f.series_type == "Risk") != want_risk,
            std::cmp::Reverse(valid),
            too_young,
            f.status != SeriesStatus::NotComplete,
            f.series_group.parse::<u32>().unwrap_or(u32::MAX),
        )
    })
}

fn run_case(case: &Value) -> Outcome {
    let id = case["id"].as_str().unwrap_or("?").to_string();
    let group = vaccine_group(case["vaccine_group"].as_str().unwrap_or(""));
    let mut out = Outcome {
        id,
        suite: case["suite"].as_str().unwrap_or("").to_string(),
        group: group.clone(),
        eval_type: case["evaluation_test_type"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string(),
        forecast_type: case["forecast_test_type"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string(),
        mismatches: Vec::new(),
    };
    let (Some(dob), Some(assessment_date)) = (date(&case["dob"]), date(&case["assessment_date"]))
    else {
        out.mismatches
            .push("fixture lacks DOB or assessment date".into());
        return out;
    };
    let gender = match case["gender"]
        .as_str()
        .unwrap_or("")
        .to_ascii_uppercase()
        .as_str()
    {
        "F" => Gender::Female,
        "M" => Gender::Male,
        _ => Gender::Unknown,
    };
    let doses: Vec<DoseInput> = case["doses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let mut dose = DoseInput::new(date(&d["date"])?, d["cvx"].as_str()?);
            dose.mvx = d["mvx"].as_str().map(str::to_string);
            Some(dose)
        })
        .collect();
    let observations = case["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            Some(Observation {
                code: o["code"].as_str()?.to_string(),
                date: date(&o["date"]),
            })
        })
        .collect();
    let patient = PatientInput {
        dob,
        gender,
        // The test cases assume U.S. birth.
        born_in_us: Some(true),
        observations,
        doses,
        assessment_date,
    };
    let result: EngineResult = phn_cdsi::run(&patient);
    let forecasts = result.group_forecasts(&group);
    // PHN_CDSI_CASE=<id> prints the engine's full result for one case.
    if std::env::var("PHN_CDSI_CASE").is_ok_and(|v| v == out.id) {
        for f in &forecasts {
            eprintln!("VG {f:?}");
        }
        for ps in &result.patient_series {
            if forecasts.iter().any(|f| {
                f.patient_series
                    .iter()
                    .any(|&s| result.patient_series[s].antigen == ps.antigen)
            }) {
                eprintln!(
                    "PS {} | {} | g{} {} best={} | {:?} n={:?} {:?} | reasons {:?}",
                    ps.antigen,
                    ps.series_name,
                    ps.series_group,
                    ps.series_type,
                    ps.best,
                    ps.status,
                    ps.forecast_dose_number,
                    ps.dates,
                    ps.forecast_reasons
                );
                for e in &ps.evaluations {
                    eprintln!(
                        "     dose {} {} cvx {} {:?} target {:?} {:?}",
                        e.dose_index, e.date, e.cvx, e.status, e.target_dose, e.reasons
                    );
                }
            }
        }
    }
    let Some(vg) = pick(case, &patient, &result, &forecasts) else {
        out.mismatches
            .push(format!("no vaccine group forecast for {group}"));
        return out;
    };

    // Dose evaluations: combine the antigens of the group's best series of
    // the series type the case names (else the chosen forecast's series).
    // Doses of other vaccine groups are checked against their own group.
    for (k, d) in case["doses"].as_array().into_iter().flatten().enumerate() {
        let Some(expected) = d["expected_status"].as_str() else {
            continue;
        };
        let typed: Vec<usize> = match d["series_type"].as_str() {
            Some(st) => forecasts
                .iter()
                .filter(|f| f.series_type.eq_ignore_ascii_case(st))
                .flat_map(|f| f.patient_series.iter().copied())
                .collect(),
            None => Vec::new(),
        };
        let statuses_in = |members: &[usize]| -> Vec<EvalStatus> {
            members
                .iter()
                .flat_map(|&s| result.patient_series[s].evaluations.iter())
                .filter(|e| e.dose_index == k)
                .map(|e| e.status)
                .collect()
        };
        let mut statuses = statuses_in(&typed);
        if statuses.is_empty() {
            statuses = statuses_in(&vg.patient_series);
        }
        if statuses.is_empty() {
            let all_best: Vec<usize> = result
                .vaccine_groups
                .iter()
                .flat_map(|f| f.patient_series.iter().copied())
                .collect();
            statuses = statuses_in(&all_best);
        }
        let actual = if statuses.is_empty() {
            "(not evaluated)"
        } else if statuses.contains(&EvalStatus::NotValid) {
            "Not Valid"
        } else if statuses.contains(&EvalStatus::SubStandard) {
            "Sub-standard"
        } else if statuses.contains(&EvalStatus::Extraneous) {
            "Extraneous"
        } else {
            "Valid"
        };
        if norm(expected) != norm(actual) {
            out.mismatches.push(format!(
                "dose {} status: expected {expected}, got {actual}",
                k + 1
            ));
        }
    }
    let expected_status = case["series_status"].as_str().unwrap_or("");
    if norm(expected_status) != norm(vg.status.label()) {
        out.mismatches.push(format!(
            "series status: expected {expected_status}, got {}",
            vg.status.label()
        ));
    }
    let expected_n = case["forecast_number"]
        .as_str()
        .and_then(|s| s.trim().parse::<usize>().ok());
    let actual_n = if vg.status == SeriesStatus::NotComplete {
        vg.forecast_dose_number
    } else {
        None
    };
    if expected_n != actual_n {
        out.mismatches.push(format!(
            "forecast dose number: expected {expected_n:?}, got {actual_n:?}"
        ));
    }
    for (field, actual) in [
        ("earliest", vg.dates.earliest),
        ("recommended", vg.dates.recommended),
        ("past_due", vg.dates.past_due),
    ] {
        let expected = date(&case[field]);
        if expected != actual {
            out.mismatches.push(format!(
                "{field}: expected {}, got {}",
                expected.map_or("-".into(), |d| d.to_string()),
                actual.map_or("-".into(), |d| d.to_string())
            ));
        }
    }
    out
}

fn run_all() -> (Value, Vec<Outcome>) {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture is JSON");
    let outcomes = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(run_case)
        .collect();
    (fixture, outcomes)
}

fn known_exceptions() -> BTreeMap<String, (String, String)> {
    let v: Value = serde_json::from_str(EXCEPTIONS).expect("exceptions are JSON");
    v["exceptions"]
        .as_array()
        .expect("exceptions array")
        .iter()
        .map(|e| {
            (
                e["id"].as_str().expect("id").to_string(),
                (
                    e["category"].as_str().unwrap_or("").to_string(),
                    e["reason"].as_str().unwrap_or("").to_string(),
                ),
            )
        })
        .collect()
}

fn pct(pass: usize, total: usize) -> String {
    if total == 0 {
        "—".into()
    } else {
        format!("{:.1}%", 100.0 * pass as f64 / total as f64)
    }
}

fn breakdown(
    outcomes: &[Outcome],
    key: impl Fn(&Outcome) -> String,
) -> BTreeMap<String, (usize, usize)> {
    let mut m: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for o in outcomes {
        let e = m.entry(key(o)).or_default();
        e.1 += 1;
        if o.mismatches.is_empty() {
            e.0 += 1;
        }
    }
    m
}

fn table(title: &str, rows: &BTreeMap<String, (usize, usize)>) -> String {
    let mut s = format!("| {title} | Cases | Pass | Pass rate |\n|---|---:|---:|---:|\n");
    for (k, (p, t)) in rows {
        let label = if k.is_empty() { "(blank)" } else { k.as_str() };
        s.push_str(&format!("| {label} | {t} | {p} | {} |\n", pct(*p, *t)));
    }
    s
}

fn report(
    fixture: &Value,
    outcomes: &[Outcome],
    exceptions: &BTreeMap<String, (String, String)>,
) -> String {
    let total = outcomes.len();
    let pass = outcomes.iter().filter(|o| o.mismatches.is_empty()).count();
    let mut s = String::new();
    s.push_str("# CDSi test-case results\n\n");
    s.push_str(
        "Generated by `crates/phn-cdsi/tests/cdsi_test_cases.rs` (run \
         `PHN_UPDATE_CDSI_REPORT=1 cargo test -p phn-cdsi --test cdsi_test_cases` to refresh). \
         Do not edit by hand; the test fails when this file is out of date.\n\n",
    );
    s.push_str(&format!(
        "- Engine: CDSi Logic Specification for ACIP Recommendations v{} ({}), Supporting Data v{} ({}).\n",
        phn_cdsi::data::LOGIC_SPEC_VERSION,
        phn_cdsi::data::LOGIC_SPEC_DATE,
        phn_cdsi::data::SUPPORTING_DATA_VERSION,
        phn_cdsi::data::SUPPORTING_DATA_PUBLISHED,
    ));
    for src in fixture["sources"].as_array().into_iter().flatten() {
        s.push_str(&format!(
            "- Test cases: `{}` (sheet \"{}\", {} cases, SHA-256 `{}`).\n",
            src["file"].as_str().unwrap_or(""),
            src["sheet"].as_str().unwrap_or(""),
            src["cases"],
            src["sha256"].as_str().unwrap_or(""),
        ));
    }
    s.push_str(
        "- Source: <https://www.cdc.gov/iis/cdsi/index.html>.\n\
         - A case passes when every expected dose evaluation status for its vaccine group, the \
         vaccine group (series) status, the forecast dose number, and the earliest, recommended, \
         and past-due dates all match. Patients are assumed U.S.-born.\n\n",
    );
    s.push_str(&format!(
        "## Overall\n\n**{pass} of {total} cases pass ({}).** {} cases are documented exceptions \
         in `crates/phn-cdsi/tests/fixtures/cdsi_known_exceptions.json`.\n\n",
        pct(pass, total),
        exceptions.len()
    ));
    s.push_str("## By suite\n\n");
    s.push_str(&table("Suite", &breakdown(outcomes, |o| o.suite.clone())));
    s.push_str("\n## By vaccine group\n\n");
    s.push_str(&table(
        "Vaccine group",
        &breakdown(outcomes, |o| {
            format!("{} ({})", o.group, o.suite.replace('_', " "))
        }),
    ));
    s.push_str("\n## By evaluation test type\n\n");
    s.push_str(&table(
        "Evaluation test type",
        &breakdown(outcomes, |o| o.eval_type.clone()),
    ));
    s.push_str("\n## By forecast test type\n\n");
    s.push_str(&table(
        "Forecast test type",
        &breakdown(outcomes, |o| o.forecast_type.clone()),
    ));
    let mut cats: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (id, (cat, _)) in exceptions {
        cats.entry(cat.clone()).or_default().push(id);
    }
    s.push_str("\n## Documented exceptions by category\n\n| Category | Cases |\n|---|---:|\n");
    for (cat, ids) in &cats {
        s.push_str(&format!("| {cat} | {} |\n", ids.len()));
    }
    s.push_str("\n## Failing cases\n\n");
    let failing: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| !o.mismatches.is_empty())
        .collect();
    if failing.is_empty() {
        s.push_str("None.\n");
    } else {
        s.push_str("| Case | Vaccine group | Category | Mismatches |\n|---|---|---|---|\n");
        for o in failing {
            let cat = exceptions
                .get(&o.id)
                .map(|(c, _)| c.as_str())
                .unwrap_or("(not documented)");
            s.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                o.id,
                o.group,
                cat,
                o.mismatches.join("; ").replace('|', "/")
            ));
        }
    }
    s
}

#[test]
fn cdc_cdsi_test_cases() {
    let (fixture, outcomes) = run_all();
    assert!(outcomes.len() > 1_000, "fixture has every case");
    let exceptions = known_exceptions();
    let failing: BTreeSet<&str> = outcomes
        .iter()
        .filter(|o| !o.mismatches.is_empty())
        .map(|o| o.id.as_str())
        .collect();
    let unexpected: Vec<String> = outcomes
        .iter()
        .filter(|o| !o.mismatches.is_empty() && !exceptions.contains_key(&o.id))
        .map(|o| format!("{} ({}): {}", o.id, o.group, o.mismatches.join("; ")))
        .collect();
    let fixed: Vec<&String> = exceptions
        .keys()
        .filter(|id| !failing.contains(id.as_str()))
        .collect();

    let doc = report(&fixture, &outcomes, &exceptions);
    let doc_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/cdsi-test-results.md"
    );
    if std::env::var("PHN_UPDATE_CDSI_REPORT").is_ok_and(|v| v == "1") {
        std::fs::write(doc_path, &doc).expect("write report");
    }
    if std::env::var("PHN_CDSI_DUMP").is_ok_and(|v| v == "1") {
        for u in &unexpected {
            eprintln!("UNEXPECTED {u}");
        }
        for f in &fixed {
            eprintln!("FIXED {f}");
        }
    }
    assert!(
        unexpected.is_empty(),
        "{} CDSi test cases fail and are not in cdsi_known_exceptions.json:\n{}",
        unexpected.len(),
        unexpected.join("\n")
    );
    assert!(
        fixed.is_empty(),
        "these known exceptions now pass; remove them from cdsi_known_exceptions.json: {fixed:?}"
    );
    let committed = std::fs::read_to_string(doc_path).unwrap_or_default();
    assert!(
        committed == doc,
        "docs/cdsi-test-results.md is out of date; run \
         PHN_UPDATE_CDSI_REPORT=1 cargo test -p phn-cdsi --test cdsi_test_cases"
    );
}
