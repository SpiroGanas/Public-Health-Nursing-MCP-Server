//! Crude and category-specific rates with exact Poisson confidence intervals.
//!
//! The interval for a count `x` is Garwood's exact interval:
//!
//! ```text
//! lower = χ²(α/2; 2x) / 2            (0 when x = 0)
//! upper = χ²(1 − α/2; 2(x + 1)) / 2
//! ```
//!
//! The rate interval divides each bound by the denominator and multiplies by
//! the rate base (per 1,000, per 100,000, …). Golden tests compare these
//! values with SciPy reference output.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use statrs::distribution::{ChiSquared, ContinuousCDF};

/// Counts below this are flagged as statistically unreliable (relative
/// standard error of 23% or more), following NCHS practice.
pub const UNRELIABLE_COUNT: u64 = 20;

/// Exact (Garwood) Poisson confidence interval for a count.
///
/// # Panics
/// Panics if `confidence` is not strictly between 0 and 1.
pub fn poisson_exact_ci(count: u64, confidence: f64) -> (f64, f64) {
    assert!(
        confidence > 0.0 && confidence < 1.0,
        "confidence must be in (0, 1)"
    );
    let alpha = 1.0 - confidence;
    let x = count as f64;
    let lower = if count == 0 {
        0.0
    } else {
        ChiSquared::new(2.0 * x)
            .expect("df > 0")
            .inverse_cdf(alpha / 2.0)
            / 2.0
    };
    let upper = ChiSquared::new(2.0 * (x + 1.0))
        .expect("df > 0")
        .inverse_cdf(1.0 - alpha / 2.0)
        / 2.0;
    (lower, upper)
}

/// Reliability or disclosure flags on a rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RateFlag {
    /// No events; the rate is 0 and only the upper bound is informative.
    ZeroCount,
    /// Fewer than 20 events: relative standard error ≥ 23%; interpret with caution.
    Unreliable,
    /// Count is below the suppression threshold; do not publish without suppression.
    BelowSuppressionThreshold,
    /// Count exceeds the denominator; check that the inputs are right.
    CountExceedsDenominator,
}

/// One rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rate {
    /// Group label (or "Total").
    pub label: String,
    /// Number of events.
    pub count: u64,
    /// Denominator (population or person-time).
    pub denominator: f64,
    /// Rate per `per` population.
    pub rate: f64,
    /// Lower confidence limit.
    pub lower: f64,
    /// Upper confidence limit.
    pub upper: f64,
    /// Relative standard error of the count (1/√x), when x > 0.
    pub relative_standard_error: Option<f64>,
    /// Reliability and disclosure flags.
    pub flags: Vec<RateFlag>,
}

/// Compute a rate per `per` with an exact interval.
///
/// `suppression_threshold`: counts from 1 to threshold − 1 are flagged.
pub fn rate(
    label: impl Into<String>,
    count: u64,
    denominator: f64,
    per: f64,
    confidence: f64,
    suppression_threshold: u32,
) -> Rate {
    assert!(denominator > 0.0, "denominator must be positive");
    let (lo, hi) = poisson_exact_ci(count, confidence);
    let scale = per / denominator;
    let mut flags = Vec::new();
    if count == 0 {
        flags.push(RateFlag::ZeroCount);
    } else if count < UNRELIABLE_COUNT {
        flags.push(RateFlag::Unreliable);
    }
    if count > 0 && count < u64::from(suppression_threshold) {
        flags.push(RateFlag::BelowSuppressionThreshold);
    }
    if count as f64 > denominator {
        flags.push(RateFlag::CountExceedsDenominator);
    }
    Rate {
        label: label.into(),
        count,
        denominator,
        rate: count as f64 * scale,
        lower: lo * scale,
        upper: hi * scale,
        relative_standard_error: (count > 0).then(|| 1.0 / (count as f64).sqrt()),
        flags,
    }
}

/// Round to a number of significant decimals for display.
pub fn round_to(x: f64, decimals: u32) -> f64 {
    let f = 10f64.powi(decimals as i32);
    (x * f).round() / f
}

/// Pick a display precision for a set of rates (more decimals for small rates).
pub fn display_decimals(max_rate: f64) -> u32 {
    if max_rate >= 100.0 {
        1
    } else if max_rate >= 1.0 {
        2
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textbook_interval_for_five_events() {
        let (lo, hi) = poisson_exact_ci(5, 0.95);
        assert!((lo - 1.6235).abs() < 1e-4, "{lo}");
        assert!((hi - 11.6683).abs() < 1e-4, "{hi}");
    }

    #[test]
    fn rate_scales_interval_and_flags_small_counts() {
        let r = rate("Town A", 3, 12_000.0, 100_000.0, 0.95, 5);
        assert!((r.rate - 25.0).abs() < 1e-9);
        assert!(r.lower < r.rate && r.rate < r.upper);
        assert!(r.flags.contains(&RateFlag::Unreliable));
        assert!(r.flags.contains(&RateFlag::BelowSuppressionThreshold));
        let z = rate("Town B", 0, 5_000.0, 1_000.0, 0.95, 5);
        assert_eq!(z.lower, 0.0);
        assert!(z.flags.contains(&RateFlag::ZeroCount));
        assert!(z.relative_standard_error.is_none());
        let big = rate("State", 5_000, 7_000_000.0, 100_000.0, 0.95, 5);
        assert!(big.flags.is_empty());
    }
}
