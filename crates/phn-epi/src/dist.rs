//! Distribution helpers shared by the engine (thin wrappers over `statrs`).

use statrs::distribution::{ChiSquared, ContinuousCDF, FisherSnedecor, Normal, StudentsT};
use statrs::function::gamma::ln_gamma;

/// Standard normal quantile.
pub fn z(p: f64) -> f64 {
    Normal::standard().inverse_cdf(p)
}

/// Two-sided critical value for a confidence level, e.g. 1.959964 for 0.95.
pub fn z_two_sided(confidence: f64) -> f64 {
    z(1.0 - (1.0 - confidence) / 2.0)
}

/// Standard normal CDF.
pub fn phi(x: f64) -> f64 {
    Normal::standard().cdf(x)
}

/// Two-sided p-value for a standard normal statistic.
pub fn p_normal_two_sided(zstat: f64) -> f64 {
    (2.0 * Normal::standard().sf(zstat.abs())).min(1.0)
}

/// Upper-tail p-value of a chi-square statistic.
pub fn p_chi_square(stat: f64, df: f64) -> f64 {
    if stat == f64::INFINITY && df > 0.0 {
        return 0.0;
    }
    if !(stat.is_finite()) || stat < 0.0 || df <= 0.0 {
        return f64::NAN;
    }
    ChiSquared::new(df).map(|d| d.sf(stat)).unwrap_or(f64::NAN)
}

/// Chi-square quantile.
pub fn chi_square_quantile(p: f64, df: f64) -> f64 {
    ChiSquared::new(df)
        .map(|d| d.inverse_cdf(p))
        .unwrap_or(f64::NAN)
}

/// Two-sided p-value for a t statistic.
pub fn p_t_two_sided(t: f64, df: f64) -> f64 {
    // An infinite statistic (a perfect correlation, or groups that differ
    // with no variation inside them) has p = 0, not "undefined".
    if t.is_infinite() && df > 0.0 {
        return 0.0;
    }
    if !t.is_finite() || df <= 0.0 {
        return f64::NAN;
    }
    StudentsT::new(0.0, 1.0, df)
        .map(|d| (2.0 * d.sf(t.abs())).min(1.0))
        .unwrap_or(f64::NAN)
}

/// t quantile.
pub fn t_quantile(p: f64, df: f64) -> f64 {
    StudentsT::new(0.0, 1.0, df)
        .map(|d| d.inverse_cdf(p))
        .unwrap_or(f64::NAN)
}

/// Upper-tail p-value of an F statistic.
pub fn p_f(f: f64, df1: f64, df2: f64) -> f64 {
    if f == f64::INFINITY && df1 > 0.0 && df2 > 0.0 {
        return 0.0;
    }
    if !f.is_finite() || f < 0.0 || df1 <= 0.0 || df2 <= 0.0 {
        return f64::NAN;
    }
    FisherSnedecor::new(df1, df2)
        .map(|d| d.sf(f))
        .unwrap_or(f64::NAN)
}

/// ln(n choose k).
pub fn ln_choose(n: u64, k: u64) -> f64 {
    if k > n {
        return f64::NEG_INFINITY;
    }
    ln_gamma(n as f64 + 1.0) - ln_gamma(k as f64 + 1.0) - ln_gamma((n - k) as f64 + 1.0)
}

/// Find a root of a monotone function on [lo, hi] by bisection
/// (`f(lo)` and `f(hi)` must differ in sign).
pub fn bisect(mut lo: f64, mut hi: f64, f: impl Fn(f64) -> f64) -> Option<f64> {
    let mut flo = f(lo);
    let fhi = f(hi);
    if !(flo.is_finite() && fhi.is_finite()) || flo.signum() == fhi.signum() {
        return None;
    }
    for _ in 0..300 {
        let mid = 0.5 * (lo + hi);
        let fm = f(mid);
        if fm == 0.0 || (hi - lo).abs() < 1e-13 * (1.0 + mid.abs()) {
            return Some(mid);
        }
        if fm.signum() == flo.signum() {
            lo = mid;
            flo = fm;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_values() {
        assert!((z_two_sided(0.95) - 1.959_963_985).abs() < 1e-8);
        assert!((p_chi_square(3.841_458_82, 1.0) - 0.05).abs() < 1e-8);
        assert!((ln_choose(10, 3) - 120f64.ln()).abs() < 1e-10);
        let r = bisect(0.0, 3.0, |x| x * x - 2.0).unwrap();
        assert!((r - 2f64.sqrt()).abs() < 1e-10);
        assert!((p_t_two_sided(2.228_138_85, 10.0) - 0.05).abs() < 1e-7);
    }
}
