//! Golden tests for `calculate_growth_percentiles` (MCH-03).
//!
//! * CDC 2000 charts: every published percentile (3rd to 97th) in CDC's data files
//!   (`snapshots/cdc_growth_*.csv`, copied verbatim from cdc.gov) is reproduced from the LMS
//!   parameters, as CDC's data-file page describes.
//! * CDC 2022 extended BMI: the published percentiles above the 95th (98th to 99.99th) and the
//!   BMI values at z-scores up to 5 are reproduced with CDC's sigma regression, and the four
//!   worked examples on CDC's extended-BMI data-file page match.
//! * WHO 2006 standards: interpolating CDC's daily WHO file (`who_growth_by_age.csv`,
//!   `who_growth_wfl.csv`) at 30.4375 × month days reproduces WHO's published monthly L, M, S
//!   and the −3 to +3 SD values (to WHO's 0.1-unit rounding). The WHO tables come from
//!   `tests/fixtures/who_monthly_zscores.json` (generator in `tests/fixtures/generators/`).

use phn_tools::mch::growth::lms::{
    self, DAYS_PER_MONTH, GrowthTables, Lms, Sex, TableTexts, cdc_bmi_at_percentile, cdc_bmi_score,
    phi, phi_inv,
};

const WTAGE: &str = include_str!("../../../snapshots/cdc_growth_wtage.csv");
const STATAGE: &str = include_str!("../../../snapshots/cdc_growth_statage.csv");
const HCAGEINF: &str = include_str!("../../../snapshots/cdc_growth_hcageinf.csv");
const BMI: &str = include_str!("../../../snapshots/cdc_growth_bmi_age_2022.csv");
const WHO_AGE: &str = include_str!("../../../snapshots/who_growth_by_age.csv");
const WHO_WFL: &str = include_str!("../../../snapshots/who_growth_wfl.csv");

fn tables() -> GrowthTables {
    GrowthTables::parse(&TableTexts {
        who_by_age: WHO_AGE,
        who_wfl: WHO_WFL,
        cdc_wtage: WTAGE,
        cdc_statage: STATAGE,
        cdc_bmi: BMI,
        cdc_hcageinf: HCAGEINF,
    })
    .unwrap()
}

/// Rows of a CDC file as header -> value maps.
fn rows(text: &str) -> Vec<std::collections::HashMap<String, f64>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .unwrap()
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    rdr.records()
        .map(|r| {
            let r = r.unwrap();
            headers
                .iter()
                .cloned()
                .zip(r.iter().map(|v| v.parse::<f64>().unwrap()))
                .collect()
        })
        .collect()
}

fn lms_of(r: &std::collections::HashMap<String, f64>) -> Lms {
    Lms {
        l: r["l"],
        m: r["m"],
        s: r["s"],
    }
}

fn sex_of(r: &std::collections::HashMap<String, f64>) -> Sex {
    if r["sex"] == 1.0 {
        Sex::Male
    } else {
        Sex::Female
    }
}

#[test]
fn cdc_2000_percentiles_reproduce_from_lms() {
    let pct = ["p3", "p5", "p10", "p25", "p50", "p75", "p90", "p95", "p97"];
    let mut n = 0;
    for (name, text) in [
        ("wtage", WTAGE),
        ("statage", STATAGE),
        ("hcageinf", HCAGEINF),
    ] {
        for r in rows(text) {
            let lms = lms_of(&r);
            for p in pct {
                let q: f64 = p[1..].parse().unwrap();
                let want = r[p];
                let got = lms.value(phi_inv(q / 100.0));
                assert!(
                    ((got - want) / want).abs() < 1e-7,
                    "{name} sex {} age {}: {p} got {got}, CDC {want}",
                    r["sex"],
                    r["agemos"]
                );
                // And back: the published value scores at the percentile.
                let z = lms.z_cdc(want);
                if lms.l.abs() >= 0.01 {
                    assert!((phi(z) * 100.0 - q).abs() < 1e-5, "{name} {p} inverse");
                }
                n += 1;
            }
        }
    }
    assert!(n > 8000);
}

#[test]
fn cdc_table_lookup_matches_rows_and_interpolates() {
    let t = tables();
    for r in rows(WTAGE) {
        let l = t.cdc_weight.at(sex_of(&r), r["agemos"]).unwrap();
        assert!((l.m - r["m"]).abs() < 1e-12);
    }
    // Midway between 24.5 and 25.5 months.
    let a = t.cdc_stature.at(Sex::Female, 24.5).unwrap();
    let b = t.cdc_stature.at(Sex::Female, 25.5).unwrap();
    let mid = t.cdc_stature.at(Sex::Female, 25.0).unwrap();
    assert!((mid.m - (a.m + b.m) / 2.0).abs() < 1e-12);
}

#[test]
fn extended_bmi_table_reproduces() {
    let ptiles = [
        ("p3", 3.0),
        ("p5", 5.0),
        ("p10", 10.0),
        ("p25", 25.0),
        ("p50", 50.0),
        ("p75", 75.0),
        ("p85", 85.0),
        ("p90", 90.0),
        ("p95", 95.0),
        ("p98", 98.0),
        ("p99", 99.0),
        ("p99_9", 99.9),
        ("p99_99", 99.99),
    ];
    let zs = [
        ("z2neg", -2.0),
        ("z1_5neg", -1.5),
        ("z1neg", -1.0),
        ("z0_5neg", -0.5),
        ("z0", 0.0),
        ("z0_5", 0.5),
        ("z1", 1.0),
        ("z1_5", 1.5),
        ("z2", 2.0),
        ("z2_5", 2.5),
        ("z3", 3.0),
        ("z3_5", 3.5),
        ("z4", 4.0),
        ("z4_5", 4.5),
        ("z5", 5.0),
    ];
    for r in rows(BMI) {
        let lms = lms_of(&r);
        let sex = sex_of(&r);
        let age = r["agemos"];
        // CDC's table rounds to 4 decimals.
        for (k, p) in ptiles {
            let got = cdc_bmi_at_percentile(lms, sex, age, p);
            assert!(
                (got - r[k]).abs() <= 5.01e-5,
                "sex {} age {age} {k}: got {got}, CDC {}",
                r["sex"],
                r[k]
            );
        }
        for (k, z) in zs {
            let got = cdc_bmi_at_percentile(lms, sex, age, phi(z) * 100.0);
            assert!(
                (got - r[k]).abs() <= 5.01e-5,
                "sex {} age {age} {k}: got {got}, CDC {}",
                r["sex"],
                r[k]
            );
        }
        // The regression sigma matches the table's (rounded) sigma.
        assert!((lms::extended_sigma(sex, age) - r["sigma"]).abs() < 6e-5);
        // 120% of the 95th percentile.
        let p95 = cdc_bmi_at_percentile(lms, sex, age, 95.0);
        assert!((1.2 * p95 - r["pct120ofp95"]).abs() < 5.01e-6 * 10.0);
    }
}

#[test]
fn extended_bmi_worked_examples_from_cdc() {
    let t = tables();
    // 1. Girl, 114.5 months, BMI 21.2: below P95 (22.3979); z = 1.4215, percentile 92.2.
    let l = t.cdc_bmi.at(Sex::Female, 114.5).unwrap();
    assert!((l.l - -2.257782149).abs() < 1e-12 && (l.m - 16.57626713).abs() < 1e-9);
    let s = cdc_bmi_score(l, Sex::Female, 114.5, 21.2);
    assert!(!s.extended);
    assert!((s.p95 - 22.3979).abs() < 5e-5);
    assert!((s.z - 1.4215).abs() < 5e-5, "{}", s.z);
    assert!((s.percentile - 92.2).abs() < 0.05);
    // 2. Boy, 50.5 months, BMI 22.6: above P95 (17.8219), sigma 2.398288;
    //    percentile 99.7683, z 2.83.
    let l = t.cdc_bmi.at(Sex::Male, 50.5).unwrap();
    let s = cdc_bmi_score(l, Sex::Male, 50.5, 22.6);
    assert!(s.extended);
    assert!((s.p95 - 17.8219).abs() < 5e-5);
    assert!((s.sigma - 2.398288).abs() < 5e-7);
    assert!((s.percentile - 99.7683).abs() < 5e-5, "{}", s.percentile);
    assert!((s.z - 2.83).abs() < 0.005);
    // 3. Girl, 184.5 months, z 1.5: BMI 27.2.
    let l = t.cdc_bmi.at(Sex::Female, 184.5).unwrap();
    assert!((l.l - -2.060616513).abs() < 1e-12);
    let b = cdc_bmi_at_percentile(l, Sex::Female, 184.5, phi(1.5) * 100.0);
    assert!((b - 27.2).abs() < 0.05, "{b}");
    // 4. Boy, 92.5 months, 98.7th percentile: sigma 3.8373, P95 19.7477, BMI 24.1.
    let l = t.cdc_bmi.at(Sex::Male, 92.5).unwrap();
    assert!((lms::extended_sigma(Sex::Male, 92.5) - 3.837343).abs() < 5e-7);
    let b = cdc_bmi_at_percentile(l, Sex::Male, 92.5, 98.7);
    assert!((b - 24.1).abs() < 0.05, "{b}");
}

#[derive(serde::Deserialize)]
struct WhoRow {
    indicator: String,
    sex: u8,
    x: f64,
    l: f64,
    m: f64,
    s: f64,
    sd: Vec<f64>,
}

#[derive(serde::Deserialize)]
struct WhoFixture {
    rows: Vec<WhoRow>,
}

#[test]
fn who_daily_tables_reproduce_the_published_monthly_tables() {
    let t = tables();
    let f: WhoFixture =
        serde_json::from_str(include_str!("fixtures/who_monthly_zscores.json")).unwrap();
    assert!(f.rows.len() > 400);
    let mut checked = 0;
    for r in &f.rows {
        let sex = if r.sex == 1 { Sex::Male } else { Sex::Female };
        let (table, x, at_24) = match r.indicator.as_str() {
            "weight_for_age" => (&t.who_weight, r.x * DAYS_PER_MONTH, r.x == 24.0),
            "length_for_age" => (&t.who_length, r.x * DAYS_PER_MONTH, r.x == 24.0),
            "head_circumference_for_age" => (&t.who_head, r.x * DAYS_PER_MONTH, r.x == 24.0),
            "bmi_for_age" => (&t.who_bmi, r.x * DAYS_PER_MONTH, r.x == 24.0),
            "weight_for_length" => (&t.who_wfl, r.x, false),
            other => panic!("unknown indicator {other}"),
        };
        // WHO's 24-month row is at 730.5 days; the daily file switches from length to standing
        // height at 731 days, so the tool (and this test) use day 730 at 24 months.
        let x = if at_24 { 730.0 } else { x };
        let got = table
            .at(sex, x)
            .unwrap_or_else(|| panic!("{} {x}", r.indicator));
        // SD values are rounded to 0.1 by WHO; allow 0.002 more for borderline roundings.
        // WHO publishes L to 4 decimals, M to 4, S to 5: compare M relatively.
        let (m_tol, sd_tol) = if at_24 { (5e-4, 0.07) } else { (3e-5, 0.052) };
        assert!(
            (got.l - r.l).abs() < 1.1e-4
                && ((got.m - r.m) / r.m).abs() < m_tol
                && (got.s - r.s).abs() < 2e-5,
            "{} sex {} x {}: L,M,S {got:?} vs WHO {} {} {}",
            r.indicator,
            r.sex,
            r.x,
            r.l,
            r.m,
            r.s
        );
        for (z, want) in (-3..=3).zip(&r.sd) {
            let v = got.value(f64::from(z));
            assert!(
                (v - want).abs() <= sd_tol,
                "{} sex {} x {} SD {z}: {v} vs WHO {want}",
                r.indicator,
                r.sex,
                r.x
            );
        }
        checked += 1;
    }
    assert_eq!(checked, f.rows.len());
}

#[test]
fn who_restricted_z_beyond_three_sd() {
    // WHO/CDC SAS: weight z beyond +3 = 3 + (x − SD3) / (SD3 − SD2).
    let t = tables();
    let l = t.who_weight.at(Sex::Male, 365.0).unwrap();
    let sd2 = l.value(2.0);
    let sd3 = l.value(3.0);
    let x = sd3 + 0.5 * (sd3 - sd2);
    assert!((l.z_who_restricted(x) - 3.5).abs() < 1e-9);
    let sdm2 = l.value(-2.0);
    let sdm3 = l.value(-3.0);
    let x = sdm3 - 0.25 * (sdm2 - sdm3);
    assert!((l.z_who_restricted(x) + 3.25).abs() < 1e-9);
}
