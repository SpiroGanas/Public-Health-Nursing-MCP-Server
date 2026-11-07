//! Golden tests: exact Poisson intervals against SciPy reference values
//! (`tests/fixtures/poisson_exact_ci.json`, generated with
//! `scipy.stats.chi2.ppf`, SciPy 1.18).

use phn_epi::rates::{poisson_exact_ci, rate};

#[derive(serde::Deserialize)]
struct Case {
    count: u64,
    confidence: f64,
    lower: f64,
    upper: f64,
}

#[derive(serde::Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

fn rel(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        a.abs()
    } else {
        ((a - b) / b).abs()
    }
}

#[test]
fn exact_poisson_intervals_match_scipy() {
    let f: Fixture = serde_json::from_str(include_str!("fixtures/poisson_exact_ci.json")).unwrap();
    assert!(f.cases.len() >= 60);
    for c in &f.cases {
        let (lo, hi) = poisson_exact_ci(c.count, c.confidence);
        assert!(
            rel(lo, c.lower) < 1e-7 && rel(hi, c.upper) < 1e-7,
            "count {} at {}: got ({lo}, {hi}), expected ({}, {})",
            c.count,
            c.confidence,
            c.lower,
            c.upper
        );
    }
}

#[test]
fn rate_interval_is_count_interval_scaled() {
    // 25 cases in a town of 40,000, per 100,000.
    let r = rate("Town", 25, 40_000.0, 100_000.0, 0.95, 5);
    let (lo, hi) = poisson_exact_ci(25, 0.95);
    assert!((r.rate - 62.5).abs() < 1e-12);
    assert!(rel(r.lower, lo * 2.5) < 1e-12);
    assert!(rel(r.upper, hi * 2.5) < 1e-12);
}
