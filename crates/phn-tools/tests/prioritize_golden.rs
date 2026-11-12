//! Golden tests for `prioritize_health_issues` (EQ-09): every value in
//! `fixtures/prioritize_golden.json` was computed in exact rational arithmetic by
//! `fixtures/generators/prioritize_golden.py` from the published Hanlon equation
//! BPR = ((A + B) x C / 3) x D, independently of the Rust code.

use phn_tools::Context;
use phn_tools::equity::prioritize::{
    IssueInput, NominalGroup, PearlAnswers, PearlMode, PrioritizeArgs, basic_priority_rating,
    nominal_points, prioritize_health_issues,
};
use serde_json::Value;

fn ctx() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
    (dir, ctx)
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/prioritize_golden.json")).unwrap()
}

#[test]
fn basic_priority_rating_matches_exact_arithmetic() {
    let f = fixture();
    let cases = f["hanlon"].as_array().unwrap();
    assert!(cases.len() >= 10);
    for c in cases {
        let got = basic_priority_rating(
            c["a"].as_f64().unwrap(),
            c["b"].as_f64().unwrap(),
            c["c"].as_f64().unwrap(),
        );
        let want = c["bpr"].as_f64().unwrap();
        assert!((got - want).abs() < 1e-9, "{c}: got {got}");
    }
}

#[test]
fn rankings_match_for_every_pearl_mode() {
    let f = fixture();
    let (_d, ctx) = ctx();
    let cases = f["rankings"].as_array().unwrap();
    assert!(cases.len() >= 12);
    for (n, case) in cases.iter().enumerate() {
        let mode = match case["mode"].as_str().unwrap() {
            "multiply" => PearlMode::Multiply,
            "prescreen" => PearlMode::Prescreen,
            _ => PearlMode::Off,
        };
        let issues: Vec<IssueInput> = case["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                let p: Vec<bool> = i["pearl"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b.as_bool().unwrap())
                    .collect();
                IssueInput {
                    name: i["name"].as_str().unwrap().into(),
                    size: i["a"].as_f64(),
                    seriousness: i["b"].as_f64(),
                    effectiveness: i["c"].as_f64(),
                    pearl: Some(PearlAnswers {
                        propriety: Some(p[0]),
                        economics: Some(p[1]),
                        acceptability: Some(p[2]),
                        resources: Some(p[3]),
                        legality: Some(p[4]),
                    }),
                    ..Default::default()
                }
            })
            .collect();
        let r = prioritize_health_issues(
            &ctx,
            PrioritizeArgs {
                issues,
                pearl_mode: Some(mode),
                write_document: Some(false),
                ..Default::default()
            },
        );
        let r = match r {
            Ok(env) => env.data,
            Err(e) => {
                // Only when nothing at all could be ranked (every issue screened out).
                assert!(
                    case["expected"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|e| e["status"] == "screened_out"),
                    "case {n}: {e}"
                );
                continue;
            }
        };
        for e in case["expected"].as_array().unwrap() {
            let got = r
                .issues
                .iter()
                .find(|i| i.name == e["name"].as_str().unwrap())
                .unwrap();
            assert_eq!(
                got.hanlon_status,
                e["status"].as_str().unwrap(),
                "case {n} {e}"
            );
            match e["score"].as_f64() {
                Some(want) => {
                    // The tool rounds scores to two decimals.
                    let got_score = got.priority_score.unwrap();
                    assert!(
                        (got_score - want).abs() < 0.0051,
                        "case {n} {e}: {got_score}"
                    );
                    assert_eq!(
                        got.hanlon_rank,
                        e["rank"].as_u64().map(|x| x as u32),
                        "case {n} {e}"
                    );
                }
                None => assert_eq!(got.priority_score, None, "case {n} {e}"),
            }
        }
    }
}

#[test]
fn nominal_group_points_and_ranks_match() {
    let f = fixture();
    let (_d, ctx) = ctx();
    for (n, case) in f["nominal"].as_array().unwrap().iter().enumerate() {
        let ranks_pp = case["ranks_per_participant"].as_u64().unwrap() as u32;
        let counts = case["counts"].as_object().unwrap();
        for (topic, cs) in counts {
            let v: Vec<u32> = cs
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u32)
                .collect();
            assert_eq!(
                nominal_points(&v, ranks_pp),
                case["points"][topic].as_u64().unwrap(),
                "case {n} {topic}"
            );
        }
        let issues: Vec<IssueInput> = counts
            .iter()
            .map(|(topic, cs)| IssueInput {
                name: topic.clone(),
                rank_counts: Some(
                    cs.as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_u64().unwrap() as u32)
                        .collect(),
                ),
                ..Default::default()
            })
            .collect();
        let r = prioritize_health_issues(
            &ctx,
            PrioritizeArgs {
                issues,
                nominal_group: Some(NominalGroup {
                    participants: case["participants"].as_u64().unwrap() as u32,
                    ranks_per_participant: ranks_pp,
                }),
                write_document: Some(false),
                ..Default::default()
            },
        )
        .unwrap()
        .data;
        for (topic, want) in case["ranks"].as_object().unwrap() {
            let got = r.issues.iter().find(|i| &i.name == topic).unwrap();
            assert_eq!(
                got.nominal_rank,
                Some(want.as_u64().unwrap() as u32),
                "case {n} {topic}"
            );
        }
    }
}
