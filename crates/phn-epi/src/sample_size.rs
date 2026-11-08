//! Sample size and power.
//!
//! * Surveys estimating a proportion or a mean with a margin of error, with
//!   the finite population correction and a design effect (the formula used
//!   by CDC Epi Info StatCalc "Population survey"; with p = 0.5 and d = 0.05 it
//!   reproduces Krejcie and Morgan's 1970 table).
//! * Two independent proportions (cohort, trial, cross-sectional, or
//!   case-control via the control exposure prevalence and odds ratio):
//!   Fleiss, Levin, and Paik's normal-approximation formula with optional
//!   continuity correction and unequal group sizes.
//! * Two independent means: normal approximation plus Guenther's z²/4
//!   correction, which tracks the exact t-based answer within one subject.
//! * Power for given sample sizes, cluster design effects, and inflation
//!   for expected non-response.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dist::{phi, z};

fn z_alpha(alpha: f64, two_sided: bool) -> f64 {
    z(1.0 - if two_sided { alpha / 2.0 } else { alpha })
}

/// Survey sample size for a proportion.
///
/// `p` expected proportion, `margin` half-width of the interval (both 0–1),
/// `population` the sampling frame size (finite population correction),
/// `design_effect` ≥ 1.
pub fn survey_proportion(
    p: f64,
    margin: f64,
    confidence: f64,
    population: Option<f64>,
    design_effect: f64,
) -> f64 {
    let z = z(1.0 - (1.0 - confidence) / 2.0);
    let pq = p * (1.0 - p);
    match population {
        Some(n) => design_effect * n * pq / ((margin * margin / (z * z)) * (n - 1.0) + pq),
        None => design_effect * z * z * pq / (margin * margin),
    }
}

/// Survey sample size for a mean (`sd` and `margin` in the measure's units).
pub fn survey_mean(
    sd: f64,
    margin: f64,
    confidence: f64,
    population: Option<f64>,
    design_effect: f64,
) -> f64 {
    let z = z(1.0 - (1.0 - confidence) / 2.0);
    let n0 = (z * sd / margin).powi(2);
    let n = match population {
        Some(big_n) => n0 / (1.0 + (n0 - 1.0) / big_n),
        None => n0,
    };
    design_effect * n
}

/// Group sizes for comparing two proportions: (n1, n2) with n2 = ratio × n1.
///
/// Fleiss JL, Levin B, Paik MC. Statistical Methods for Rates and
/// Proportions, 3rd ed. (2003), eq. 4.19 and the continuity correction 4.20.
pub fn two_proportions(
    p1: f64,
    p2: f64,
    alpha: f64,
    power: f64,
    ratio: f64,
    two_sided: bool,
    continuity: bool,
) -> (f64, f64) {
    let r = ratio;
    let za = z_alpha(alpha, two_sided);
    let zb = z(power);
    let pbar = (p1 + r * p2) / (r + 1.0);
    let qbar = 1.0 - pbar;
    let d = (p1 - p2).abs();
    let num =
        za * ((r + 1.0) * pbar * qbar).sqrt() + zb * (r * p1 * (1.0 - p1) + p2 * (1.0 - p2)).sqrt();
    let mut n1 = num * num / (r * d * d);
    if continuity {
        n1 = n1 / 4.0 * (1.0 + (1.0 + 2.0 * (r + 1.0) / (n1 * r * d)).sqrt()).powi(2);
    }
    (n1, r * n1)
}

/// Power to detect p1 vs p2 with group sizes n1, n2 (no continuity correction).
pub fn power_two_proportions(
    p1: f64,
    p2: f64,
    n1: f64,
    n2: f64,
    alpha: f64,
    two_sided: bool,
) -> f64 {
    let r = n2 / n1;
    let za = z_alpha(alpha, two_sided);
    let pbar = (p1 + r * p2) / (r + 1.0);
    let d = (p1 - p2).abs();
    let zb = (d * (r * n1).sqrt() - za * ((r + 1.0) * pbar * (1.0 - pbar)).sqrt())
        / (r * p1 * (1.0 - p1) + p2 * (1.0 - p2)).sqrt();
    phi(zb)
}

/// Exposure prevalence among cases implied by an odds ratio and the
/// prevalence among controls.
pub fn case_exposure_from_or(or: f64, p_controls: f64) -> f64 {
    or * p_controls / (1.0 + p_controls * (or - 1.0))
}

/// Group sizes for comparing two means: (n1, n2) with n2 = ratio × n1.
pub fn two_means(
    difference: f64,
    sd: f64,
    alpha: f64,
    power: f64,
    ratio: f64,
    two_sided: bool,
) -> (f64, f64) {
    let za = z_alpha(alpha, two_sided);
    let zb = z(power);
    let n1 = (1.0 + 1.0 / ratio) * (sd * (za + zb) / difference).powi(2) + za * za / 4.0;
    (n1, ratio * n1)
}

/// Power to detect a difference in means with group sizes n1, n2 (normal approximation).
pub fn power_two_means(
    difference: f64,
    sd: f64,
    n1: f64,
    n2: f64,
    alpha: f64,
    two_sided: bool,
) -> f64 {
    let za = z_alpha(alpha, two_sided);
    let se = sd * (1.0 / n1 + 1.0 / n2).sqrt();
    phi(difference.abs() / se - za)
}

/// Design effect for cluster sampling: 1 + (m − 1) × ICC.
pub fn cluster_design_effect(cluster_size: f64, icc: f64) -> f64 {
    1.0 + (cluster_size - 1.0) * icc
}

/// A sample size rounded up, with non-response inflation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rounded {
    /// Unrounded value from the formula.
    pub exact: f64,
    /// Rounded up.
    pub needed: u64,
    /// Rounded up after dividing by the expected response (or retention) rate.
    pub to_recruit: u64,
}

/// Round a sample size up and inflate for an expected response rate (0–1].
pub fn round_up(n: f64, response_rate: f64) -> Rounded {
    let needed = (n - 1e-9).ceil().max(1.0) as u64;
    Rounded {
        exact: n,
        needed,
        to_recruit: (needed as f64 / response_rate - 1e-9).ceil() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn krejcie_morgan_table() {
        // Krejcie RV, Morgan DW. Determining sample size for research activities.
        // Educ Psychol Meas 1970;30:607-610. They used chi-square(1) = 3.841
        // (z = 1.95996); p = 0.5, d = 0.05.
        for (n_pop, want) in [
            (100.0, 80),
            (500.0, 217),
            (1000.0, 278),
            (10_000.0, 370),
            (100_000.0, 383),
        ] {
            let n = survey_proportion(0.5, 0.05, 0.95, Some(n_pop), 1.0);
            // The published table rounds to the nearest whole number; the
            // tool itself rounds up (conservative).
            assert_eq!(n.round() as u64, want, "N = {n_pop}: {n}");
        }
        assert_eq!(
            round_up(survey_proportion(0.5, 0.05, 0.95, None, 1.0), 1.0).needed,
            385
        );
        assert_eq!(round_up(384.2, 0.8).to_recruit, 482);
    }

    #[test]
    fn continuity_correction_increases_n_and_power_inverts() {
        let (a, _) = two_proportions(0.10, 0.05, 0.05, 0.8, 1.0, true, false);
        let (b, _) = two_proportions(0.10, 0.05, 0.05, 0.8, 1.0, true, true);
        assert!(b > a);
        let pw = power_two_proportions(0.10, 0.05, a, a, 0.05, true);
        assert!((pw - 0.8).abs() < 1e-9);
        let (n1, n2) = two_proportions(0.3, 0.2, 0.05, 0.9, 2.0, true, false);
        assert!((n2 - 2.0 * n1).abs() < 1e-9);
        assert!((power_two_proportions(0.3, 0.2, n1, n2, 0.05, true) - 0.9).abs() < 1e-9);
        assert!((case_exposure_from_or(2.0, 0.2) - 1.0 / 3.0).abs() < 1e-12);
        assert!((cluster_design_effect(20.0, 0.05) - 1.95).abs() < 1e-12);
    }
}
