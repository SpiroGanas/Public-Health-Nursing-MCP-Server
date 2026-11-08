//! Golden tests for `score_mental_health_screen` (MH-02): every case in
//! `fixtures/phase10/screening_golden.json` was computed by hand from the
//! published scoring rules cited in the fixture's description.

use phn_tools::Context;
use phn_tools::bh::screening::{EpdsEntry, ItemResponse, ScreenArgs};
use serde_json::Value;

fn ctx() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
    (dir, ctx)
}

fn response(v: &Value) -> Option<ItemResponse> {
    match v {
        Value::Null => None,
        Value::Number(n) => Some(ItemResponse::Number(n.as_i64().unwrap())),
        Value::Bool(b) => Some(ItemResponse::YesNo(*b)),
        Value::String(s) => Some(ItemResponse::Text(s.clone())),
        other => panic!("bad response {other}"),
    }
}

#[test]
fn screening_scores_match_hand_computed_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/phase10/screening_golden.json")).unwrap();
    let (_d, ctx) = ctx();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() >= 45);
    for (i, c) in cases.iter().enumerate() {
        let args = ScreenArgs {
            instrument: Some(c["instrument"].as_str().unwrap().to_string()),
            responses: c["responses"].as_array().unwrap().iter().map(response).collect(),
            epds_entered_as: c
                .get("epds_entered_as")
                .and_then(Value::as_str)
                .map(|s| match s {
                    "option_positions" => EpdsEntry::OptionPositions,
                    _ => EpdsEntry::Scores,
                }),
            age_years: c.get("age_years").and_then(Value::as_u64).map(|a| a as u32),
            refused: c.get("refused").and_then(Value::as_bool).unwrap_or(false),
            asq_attempt_within_12_months: c
                .get("asq_attempt_within_12_months")
                .and_then(Value::as_bool),
            cssrs_behavior_past_3_months: c
                .get("cssrs_behavior_past_3_months")
                .and_then(Value::as_bool),
            ..ScreenArgs::default()
        };
        let env = phn_tools::bh::score_mental_health_screen(&ctx, args)
            .unwrap_or_else(|e| panic!("case {i} {c}: {e}"));
        let v = serde_json::to_value(&env.data).unwrap();
        let scored = &v["scored"];
        assert_eq!(scored["total_score"], c["total"], "case {i} total: {c}");
        assert_eq!(scored["severity"], c["severity"], "case {i} severity: {c}");
        assert_eq!(scored["outcome"], c["outcome"], "case {i} outcome: {c}");
        let alerts: Vec<&str> = v["safety_alerts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect();
        let expected: Vec<&str> = c["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert_eq!(alerts, expected, "case {i} alerts: {c}");
        // Every alert has cited steps, and a decision-support notice is attached.
        for a in v["safety_alerts"].as_array().unwrap() {
            assert!(!a["steps"].as_array().unwrap().is_empty());
        }
        assert!(env.notice.is_some());
    }
}
