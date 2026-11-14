//! Golden tests for the budget arithmetic behind `build_program_budget` (PLAN-09).
//!
//! `fixtures/budget_golden.json` is computed by an independent Python program with exact decimal
//! arithmetic (`fixtures/generators/budget_golden.py`). The workbook's live formulas are checked
//! against the same numbers by recalculating them in LibreOffice:
//!
//! ```sh
//! BUDGET_DIR=/tmp/budget_wb cargo test -p phn-tools --test budget_golden -- --ignored
//! python3 -I crates/phn-tools/tests/fixtures/generators/check_budget_workbook.py /tmp/budget_wb
//! ```

use phn_tools::planning::budget::calc::{
    Block, Category, Funding, IndirectBase, Line, MatchBasis, MtdcExclusion, Rates, Treatment,
    compute,
};
use phn_tools::planning::budget::workbook::{WorkbookInput, build};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/budget_golden.json")).unwrap()
}

fn category(s: &str) -> Category {
    serde_json::from_value(Value::String(s.into())).unwrap()
}

fn lines(case: &Value) -> Vec<Line> {
    let n = case["years"].as_u64().unwrap() as usize;
    case["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| Line {
            category: category(l["category"].as_str().unwrap()),
            item: l["item"].as_str().unwrap().into(),
            funding: serde_json::from_value(l["funding"].clone()).unwrap(),
            quantity: l["quantity"].as_f64().unwrap(),
            unit_cost: l["unit_cost"].as_f64().unwrap(),
            unit: String::new(),
            fringe: l["fringe"].as_f64(),
            escalate: l["escalate"].as_bool().unwrap(),
            first_year: l["first_year"].as_u64().unwrap() as usize,
            last_year: l["last_year"].as_u64().map(|v| v as usize).unwrap_or(n),
            treatment: match l["treatment"].as_str().unwrap() {
                "Excluded" => Treatment::Excluded,
                "Subaward" => Treatment::Subaward,
                _ => Treatment::Included,
            },
            exclusion: l["treatment"]
                .as_str()
                .filter(|t| *t == "Excluded" && l["category"] != "equipment")
                .map(|_| MtdcExclusion::ParticipantSupport),
        })
        .collect()
}

fn rates(case: &Value) -> Rates {
    Rates {
        years: case["years"].as_u64().unwrap() as usize,
        salary_escalation: case["salary_escalation"].as_f64().unwrap(),
        other_escalation: case["other_escalation"].as_f64().unwrap(),
        fringe: case["fringe"].as_f64().unwrap(),
        indirect_rate: case["indirect_rate"].as_f64().unwrap(),
        indirect_base: serde_json::from_value::<IndirectBase>(case["indirect_base"].clone())
            .unwrap(),
        indirect_on_cost_share: case["indirect_on_cost_share"].as_bool().unwrap(),
        subaward_cap: case["subaward_cap"].as_f64().unwrap(),
    }
}

fn required(case: &Value) -> Option<(f64, MatchBasis)> {
    case["required_match"].as_array().map(|a| {
        (
            a[0].as_f64().unwrap(),
            serde_json::from_value(a[1].clone()).unwrap(),
        )
    })
}

fn nums(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().parse().unwrap())
        .collect()
}

fn check_block(name: &str, which: &str, got: &Block, exp: &Value) {
    let pairs: Vec<(&str, &[f64])> = vec![
        ("personnel", &got.personnel),
        ("fringe", &got.fringe),
        ("travel", &got.travel),
        ("equipment", &got.equipment),
        ("supplies", &got.supplies),
        ("contractual", &got.contractual),
        ("other", &got.other),
        ("direct", &got.direct),
        ("indirect_base", &got.indirect_base),
        ("indirect", &got.indirect),
        ("total", &got.total),
    ];
    for (k, v) in pairs {
        let e = nums(&exp[k]);
        assert_eq!(e.len(), v.len(), "{name} {which} {k}");
        for (i, (a, b)) in v.iter().zip(&e).enumerate() {
            assert!(
                (a - b).abs() < 1e-4,
                "{name} {which} {k} year {}: {a} vs {b}",
                i + 1
            );
        }
    }
}

#[test]
fn budget_arithmetic_matches_the_independent_computation() {
    let fx = fixture();
    let cases = fx["cases"].as_array().unwrap();
    assert!(cases.len() >= 5);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let ls = lines(case);
        let c = compute(&ls, &rates(case), required(case)).unwrap();
        check_block(name, "federal", &c.federal, &case["expected"]["federal"]);
        check_block(
            name,
            "cost_share",
            &c.cost_share,
            &case["expected"]["cost_share"],
        );
        assert!(c.self_check(&ls), "{name}");
        if let Some(m) = case["expected"].get("match") {
            let mc = c.match_check.unwrap();
            let f = |k: &str| m[k].as_str().unwrap().parse::<f64>().unwrap();
            assert!(
                (mc.required - f("required")).abs() < 0.006,
                "{name} required"
            );
            assert!(
                (mc.budgeted - f("budgeted")).abs() < 1e-4,
                "{name} budgeted"
            );
            assert!(
                (mc.shortfall - f("shortfall")).abs() < 0.006,
                "{name} shortfall"
            );
        }
    }
}

#[test]
fn funding_enum_reads_the_fixture_spelling() {
    let f: Funding = serde_json::from_value(Value::String("cost_share".into())).unwrap();
    assert_eq!(f, Funding::CostShare);
}

/// Writes one workbook per golden case to the folder in `BUDGET_DIR`, for
/// `fixtures/generators/check_budget_workbook.py`.
#[test]
#[ignore = "writes files for an external LibreOffice check"]
fn write_golden_workbooks() {
    let Ok(dir) = std::env::var("BUDGET_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for case in fixture()["cases"].as_array().unwrap() {
        let ls = lines(case);
        let r = rates(case);
        let req = required(case);
        let c = compute(&ls, &r, req).unwrap();
        let bytes = build(&WorkbookInput {
            title: "Golden",
            lines: &ls,
            rates: &r,
            method_label: "Negotiated rate",
            required_match: req,
            computed: &c,
            read_me: &["Golden case".to_string()],
        })
        .unwrap();
        let name = case["name"].as_str().unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.xlsx")),
            bytes,
        )
        .unwrap();
    }
}
