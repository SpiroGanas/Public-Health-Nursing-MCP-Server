//! Aberration detection for case and syndrome count series (CD-10).
//!
//! * **EARS C1, C2, C3** (CDC Early Aberration Reporting System; Hutwagner
//!   et al. 2003, Fricker et al. 2008). C1 compares today's count with the
//!   mean and standard deviation of the previous 7 periods; C2 uses the 7
//!   periods before a 2-period guard band (t−9 … t−3); C3 sums the positive
//!   parts of `C2 − 1` over the current and two previous periods.
//!   C1 and C2 values match R `surveillance::earsC`. C3 follows the published
//!   definition (the current period plus two previous); `surveillance::earsC`
//!   computes its C3 bound from the two previous periods only, so the two
//!   can differ (see `tests/aberration_golden.rs`).
//! * **CUSUM** on Poisson-standardized counts, `(x − m)/√m`, with reference
//!   value `k` and decision interval `h`, as `surveillance::algo.cusum`
//!   (`trans = "standard"`): `S_t = max(0, S_{t−1} + x_t − k)`, signal when
//!   `S_t ≥ h`.
//! * **Farrington-type** detection: the flexible Farrington algorithm with the
//!   improvements of Noufaily et al. (2013) (`surveillance::farringtonFlexible`):
//!   a quasi-Poisson log-linear model fit to the same season in `b` past years
//!   (a window of ±`w` periods) plus, with `no_periods > 1`, the rest of each
//!   year as a seasonal factor; down-weighting of past outbreaks by Anscombe
//!   residuals; an optional linear trend kept only when significant, `b ≥ 3`,
//!   and not extrapolating; and a threshold from the negative-binomial
//!   quantile (Noufaily) or the 2/3-power transformed normal (original
//!   Farrington, "delta").
//!
//! Time indices are 0-based in this API; internally the Farrington model uses
//! 1-based time like R so that fitted values, standard errors, and p-values
//! can be compared with R directly.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use statrs::function::beta::beta_reg;
use statrs::function::gamma::gamma_ur;

use crate::dist::{p_t_two_sided, z};

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// Sample standard deviation (n − 1 denominator, as R's `sd`).
fn sd(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return f64::NAN;
    }
    let m = mean(v);
    (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt()
}

/// Standardized deviation that tolerates a zero standard deviation:
/// `+∞` when the count is above a flat baseline, `0` when equal.
fn standardize(x: f64, m: f64, s: f64) -> f64 {
    if s > 0.0 {
        (x - m) / s
    } else if x > m {
        f64::INFINITY
    } else if x < m {
        f64::NEG_INFINITY
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// EARS
// ---------------------------------------------------------------------------

/// EARS method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EarsMethod {
    /// Baseline: the 7 periods immediately before the current one.
    C1,
    /// Baseline: 7 periods before a 2-period guard band (t−9 … t−3).
    C2,
    /// Sum of max(0, C2 − 1) over the current and two previous periods.
    C3,
}

impl EarsMethod {
    /// Threshold used by CDC's EARS: 3 (standard deviations) for C1 and C2,
    /// 2 for C3.
    pub fn classic_threshold(self) -> f64 {
        match self {
            EarsMethod::C1 | EarsMethod::C2 => 3.0,
            EarsMethod::C3 => 2.0,
        }
    }

    /// Lag of the newest baseline value (1 for C1, 3 for C2 and C3).
    fn lag(self) -> usize {
        match self {
            EarsMethod::C1 => 1,
            EarsMethod::C2 | EarsMethod::C3 => 3,
        }
    }
}

/// Settings for EARS.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EarsParams {
    /// Baseline length in periods (EARS uses 7; at least 3).
    pub baseline: usize,
    /// Floor for the baseline standard deviation (0 = no floor).
    pub min_sigma: f64,
    /// Signal when the statistic exceeds this value.
    pub threshold: f64,
}

impl EarsParams {
    /// EARS defaults for a method: 7-period baseline, no sigma floor, and the
    /// classic threshold.
    pub fn classic(method: EarsMethod) -> Self {
        Self {
            baseline: 7,
            min_sigma: 0.0,
            threshold: method.classic_threshold(),
        }
    }
}

/// One evaluated period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EarsPoint {
    /// 0-based position in the series.
    pub index: usize,
    pub observed: f64,
    /// Baseline mean (for C3: of the current period's C2 baseline).
    pub baseline_mean: f64,
    /// Baseline standard deviation after the floor.
    pub baseline_sd: f64,
    /// C1, C2, or C3 statistic (may be infinite when the baseline is flat).
    pub statistic: f64,
    /// Smallest count that would have signaled (C1 and C2 only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_bound: Option<f64>,
    pub alarm: bool,
}

fn ears_c(counts: &[f64], t: usize, method: EarsMethod, p: &EarsParams) -> (f64, f64, f64) {
    let lag = method.lag();
    let base: Vec<f64> = (lag..lag + p.baseline).map(|l| counts[t - l]).collect();
    let m = mean(&base);
    let s = sd(&base).max(p.min_sigma);
    (standardize(counts[t], m, s), m, s)
}

/// Run an EARS method over every period that has enough history.
///
/// C1 needs `baseline` earlier periods, C2 `baseline + 2`, C3 `baseline + 4`.
pub fn ears(counts: &[f64], method: EarsMethod, p: &EarsParams) -> Result<Vec<EarsPoint>, String> {
    if p.baseline < 3 {
        return Err("the EARS baseline must be at least 3 periods".into());
    }
    if counts.iter().any(|c| !c.is_finite() || *c < 0.0) {
        return Err("counts must be non-negative numbers".into());
    }
    let first = match method {
        EarsMethod::C1 => p.baseline,
        EarsMethod::C2 => p.baseline + 2,
        EarsMethod::C3 => p.baseline + 4,
    };
    if counts.len() <= first {
        return Err(format!(
            "EARS {method:?} needs more than {first} periods of data (got {})",
            counts.len()
        ));
    }
    let mut out = Vec::with_capacity(counts.len() - first);
    for t in first..counts.len() {
        match method {
            EarsMethod::C1 | EarsMethod::C2 => {
                let (stat, m, s) = ears_c(counts, t, method, p);
                let ub = m + p.threshold * s;
                out.push(EarsPoint {
                    index: t,
                    observed: counts[t],
                    baseline_mean: m,
                    baseline_sd: s,
                    statistic: stat,
                    upper_bound: s.is_finite().then_some(ub),
                    alarm: stat > p.threshold,
                });
            }
            EarsMethod::C3 => {
                let mut c3 = 0.0;
                let mut cur = (0.0, 0.0, 0.0);
                for back in 0..3 {
                    let r = ears_c(counts, t - back, EarsMethod::C2, p);
                    if back == 0 {
                        cur = r;
                    }
                    c3 += (r.0 - 1.0).max(0.0);
                }
                out.push(EarsPoint {
                    index: t,
                    observed: counts[t],
                    baseline_mean: cur.1,
                    baseline_sd: cur.2,
                    statistic: c3,
                    upper_bound: None,
                    alarm: c3 > p.threshold,
                });
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// CUSUM
// ---------------------------------------------------------------------------

/// Settings for the Poisson-standardized CUSUM.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CusumParams {
    /// Reference value (allowance) in standardized units.
    pub k: f64,
    /// Decision interval: signal when the CUSUM reaches `h`.
    pub h: f64,
    /// Number of initial periods used to estimate the in-control mean
    /// (ignored when `mean` is given). Monitoring starts after them.
    pub baseline_periods: usize,
    /// Known in-control mean per period, if any.
    pub mean: Option<f64>,
    /// Reset the CUSUM to zero after a signal (the recorded value is then 0).
    pub reset: bool,
}

/// One monitored period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CusumPoint {
    pub index: usize,
    pub observed: f64,
    /// `(x − m)/√m`.
    pub standardized: f64,
    pub cusum: f64,
    /// Smallest count that would have signaled, `⌈√m (h + k − S_{t−1}) + m⌉`.
    pub upper_bound: f64,
    pub alarm: bool,
}

/// CUSUM result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CusumResult {
    /// In-control mean used.
    pub mean: f64,
    pub points: Vec<CusumPoint>,
}

/// Poisson-standardized CUSUM (as `surveillance::algo.cusum`, `trans = "standard"`).
pub fn cusum(counts: &[f64], p: &CusumParams) -> Result<CusumResult, String> {
    if counts.iter().any(|c| !c.is_finite() || *c < 0.0) {
        return Err("counts must be non-negative numbers".into());
    }
    if !(p.k >= 0.0 && p.h > 0.0) {
        return Err("CUSUM needs k ≥ 0 and h > 0".into());
    }
    let start = if p.mean.is_some() {
        p.baseline_periods.min(counts.len())
    } else {
        p.baseline_periods
    };
    if p.mean.is_none() && (start < 2 || start >= counts.len()) {
        return Err(format!(
            "CUSUM needs a baseline of at least 2 periods and at least one period after it \
             (baseline {start}, series {})",
            counts.len()
        ));
    }
    let m = match p.mean {
        Some(m) => m,
        None => mean(&counts[..start]),
    };
    if !(m.is_finite() && m > 0.0) {
        return Err(
            "the in-control mean is zero: CUSUM on standardized counts needs a baseline with \
             some events (use EARS, or give a small expected mean)"
                .into(),
        );
    }
    let mut s_prev = 0.0;
    let mut points = Vec::new();
    for (t, &x) in counts.iter().enumerate().skip(start) {
        let z = (x - m) / m.sqrt();
        let ub = (m.sqrt() * (p.h + p.k - s_prev) + m).ceil().max(0.0);
        let mut s = (s_prev + z - p.k).max(0.0);
        let alarm = s >= p.h;
        if alarm && p.reset {
            // As algo.cusum: the reset value is what is recorded.
            s = 0.0;
        }
        points.push(CusumPoint {
            index: t,
            observed: x,
            standardized: z,
            cusum: s,
            upper_bound: ub,
            alarm,
        });
        s_prev = s;
    }
    Ok(CusumResult { mean: m, points })
}

// ---------------------------------------------------------------------------
// Farrington (flexible, Noufaily improvements)
// ---------------------------------------------------------------------------

/// How the Farrington upper threshold is computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdMethod {
    /// Original Farrington: normal approximation on the 2/3-power scale with
    /// the prediction error ("delta" in R).
    Delta,
    /// Noufaily: negative-binomial quantile around the predicted mean.
    #[default]
    NbPlugin,
    /// Noufaily: negative-binomial quantile around the upper (1 − α) limit of
    /// the predicted mean ("muan" in R).
    Muan,
}

/// Settings for the Farrington-type algorithm (names follow
/// `surveillance::farringtonFlexible`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FarringtonParams {
    /// Periods per year: 52 (weekly) or 12 (monthly).
    pub freq: usize,
    /// Years of history used.
    pub b: usize,
    /// Half-width of the window around the same period in past years.
    pub w: usize,
    /// Down-weight past outbreaks (Anscombe residual reweighting).
    pub reweight: bool,
    /// Anscombe residuals above this are down-weighted (2.58 Noufaily; 1 original).
    pub weights_threshold: f64,
    /// One-sided significance level of the threshold.
    pub alpha: f64,
    /// Consider a linear time trend.
    pub trend: bool,
    /// Keep the trend only when its p-value is below this.
    pub p_threshold_trend: f64,
    /// Require at least this many cases ...
    pub limit_cases: f64,
    /// ... in this many most recent periods (including the current one) to signal.
    pub limit_periods: usize,
    /// Seasonal factor levels (1 = original Farrington; 10 = Noufaily).
    pub no_periods: usize,
    /// Most recent periods left out of the baseline (default `w`; Noufaily uses 26 weeks).
    pub past_periods_not_included: usize,
    pub threshold_method: ThresholdMethod,
}

impl FarringtonParams {
    /// The improved (Noufaily) configuration for weekly data.
    pub fn noufaily_weekly() -> Self {
        Self {
            freq: 52,
            b: 5,
            w: 3,
            reweight: true,
            weights_threshold: 2.58,
            alpha: 0.05,
            trend: true,
            p_threshold_trend: 0.05,
            limit_cases: 5.0,
            limit_periods: 4,
            no_periods: 10,
            past_periods_not_included: 26,
            threshold_method: ThresholdMethod::NbPlugin,
        }
    }

    /// Smallest series length for the first evaluated period.
    pub fn min_history(&self) -> usize {
        self.freq * self.b + self.w + 1
    }
}

/// One evaluated period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FarringtonPoint {
    pub index: usize,
    pub observed: f64,
    /// Expected count from the model (`None` if too few recent cases to evaluate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<f64>,
    /// Upper threshold (`None` if too few recent cases to evaluate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_bound: Option<f64>,
    /// Exceedance score: > 1 means above the threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Probability of a count at least as large as observed under the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p_value: Option<f64>,
    pub alarm: bool,
    /// A time trend was kept in the model.
    pub trend: bool,
    /// Overdispersion (≥ 1).
    pub phi: f64,
    /// Fewer than `limit_cases` in the last `limit_periods`: no signal possible.
    pub too_few_cases: bool,
}

/// A fitted quasi-Poisson model.
struct Fit {
    beta: Vec<f64>,
    /// (X'WX)^-1 (unscaled covariance).
    cov: Vec<Vec<f64>>,
    mu: Vec<f64>,
    hat: Vec<f64>,
    /// Pearson dispersion estimate (unfloored), as `summary.glm`.
    dispersion: f64,
    df_residual: f64,
    converged: bool,
}

/// Solve the symmetric positive definite system by Cholesky; returns the
/// inverse as well. `None` when singular.
fn spd_inverse(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                if s <= 1e-12 * a[i][i].abs().max(1e-300) {
                    return None;
                }
                l[i][i] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    // Invert L, then A^-1 = L^-T L^-1.
    let mut linv = vec![vec![0.0; n]; n];
    for i in 0..n {
        linv[i][i] = 1.0 / l[i][i];
        for j in 0..i {
            let mut s = 0.0;
            for k in j..i {
                s -= l[i][k] * linv[k][j];
            }
            linv[i][j] = s / l[i][i];
        }
    }
    let mut inv = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = 0.0;
            for k in i.max(j)..n {
                s += linv[k][i] * linv[k][j];
            }
            inv[i][j] = s;
            inv[j][i] = s;
        }
    }
    Some(inv)
}

fn mat_vec(a: &[Vec<f64>], v: &[f64]) -> Vec<f64> {
    a.iter()
        .map(|r| r.iter().zip(v).map(|(x, y)| x * y).sum())
        .collect()
}

fn quad(a: &[Vec<f64>], v: &[f64]) -> f64 {
    v.iter().zip(mat_vec(a, v)).map(|(x, y)| x * y).sum()
}

/// Quasi-Poisson log-link fit by iteratively reweighted least squares, as R's
/// `glm.fit` (start at `log(y + 0.1)`, convergence on relative deviance change
/// below 1e-8, at most 25 iterations).
fn fit_quasi_poisson(x: &[Vec<f64>], y: &[f64], prior: &[f64]) -> Option<Fit> {
    let n = y.len();
    let p = x[0].len();
    let eps = f64::EPSILON;
    let mut eta: Vec<f64> = y.iter().map(|v| (v + 0.1).ln()).collect();
    let mut mu: Vec<f64> = eta.iter().map(|e| e.exp().max(eps)).collect();
    let deviance = |mu: &[f64]| -> f64 {
        2.0 * y
            .iter()
            .zip(mu)
            .zip(prior)
            .map(|((&yi, &mi), &wi)| {
                let t = if yi > 0.0 { yi * (yi / mi).ln() } else { 0.0 };
                wi * (t - (yi - mi))
            })
            .sum::<f64>()
    };
    let mut dev_old = deviance(&mu);
    let mut converged = false;
    let mut beta = vec![0.0; p];
    let mut cov = vec![vec![0.0; p]; p];
    let mut w_last = vec![0.0; n];
    for _ in 0..25 {
        // Working response and weights.
        let w: Vec<f64> = (0..n).map(|i| prior[i] * mu[i]).collect();
        let zr: Vec<f64> = (0..n).map(|i| eta[i] + (y[i] - mu[i]) / mu[i]).collect();
        let mut xtwx = vec![vec![0.0; p]; p];
        let mut xtwz = vec![0.0; p];
        for i in 0..n {
            for a in 0..p {
                xtwz[a] += x[i][a] * w[i] * zr[i];
                for b in 0..=a {
                    xtwx[a][b] += x[i][a] * w[i] * x[i][b];
                }
            }
        }
        for a in 0..p {
            for b in 0..a {
                xtwx[b][a] = xtwx[a][b];
            }
        }
        cov = spd_inverse(&xtwx)?;
        beta = mat_vec(&cov, &xtwz);
        w_last = w;
        eta = x
            .iter()
            .map(|r| r.iter().zip(&beta).map(|(a, b)| a * b).sum())
            .collect();
        mu = eta.iter().map(|e| e.exp().max(eps)).collect();
        let dev = deviance(&mu);
        if ((dev - dev_old).abs() / (dev.abs() + 0.1)) < 1e-8 {
            converged = true;
            break;
        }
        dev_old = dev;
    }
    // As R's glm.fit: the covariance, hat values, and working weights come
    // from the last iteration's weights; residuals from the updated mu.
    let hat: Vec<f64> = (0..n).map(|i| w_last[i] * quad(&cov, &x[i])).collect();
    let n_ok = prior.iter().filter(|&&v| v != 0.0).count();
    let df_residual = n_ok as f64 - p as f64;
    let pearson: f64 = (0..n)
        .map(|i| w_last[i] * ((y[i] - mu[i]) / mu[i]).powi(2))
        .sum();
    let dispersion = if df_residual > 0.0 {
        pearson / df_residual
    } else {
        f64::NAN
    };
    Some(Fit {
        beta,
        cov,
        mu,
        hat,
        dispersion,
        df_residual,
        converged,
    })
}

/// Seasonal block labels for evaluation time `k` (1-based), as
/// `surveillance:::blocks` with integer time: `None` outside the reference
/// data.
fn blocks(n: usize, k: usize, p: &FarringtonParams) -> Vec<Option<usize>> {
    let (freq, b, w, np) = (p.freq, p.b, p.w, p.no_periods);
    let mut f = vec![None; n + 1]; // 1-based
    for t in (k - w)..=k {
        f[t] = Some(np);
    }
    // Reference weeks, most recent first: k, k − freq, ..., k − b·freq.
    let refs: Vec<usize> = (0..=b).map(|i| k - i * freq).collect();
    for i in 1..=b {
        let rw = refs[i];
        for t in (rw - w)..=(rw + w) {
            f[t] = Some(np);
        }
        if np != 1 {
            let period = refs[i - 1] - 2 * w - 1 - rw;
            let len = period / (np - 1);
            let rest = period % (np - 1);
            let mut lens = vec![len; np - 1];
            for l in lens.iter_mut().take(rest) {
                *l += 1;
            }
            let mut start = rw + w + 1;
            for (j, l) in lens.iter().enumerate() {
                for t in start..start + l {
                    f[t] = Some(j + 1);
                }
                start += l;
            }
        }
    }
    f
}

struct ModelData {
    y: Vec<f64>,
    wtime: Vec<f64>,
    level: Vec<usize>,
    levels: Vec<usize>,
}

fn design(d: &ModelData, trend: bool, factors: bool) -> Vec<Vec<f64>> {
    d.y.iter()
        .enumerate()
        .map(|(i, _)| row(d.wtime[i], d.level[i], &d.levels, trend, factors))
        .collect()
}

fn row(wtime: f64, level: usize, levels: &[usize], trend: bool, factors: bool) -> Vec<f64> {
    let mut r = vec![1.0];
    if trend {
        r.push(wtime);
    }
    if factors {
        for &l in levels.iter().skip(1) {
            r.push(if l == level { 1.0 } else { 0.0 });
        }
    }
    r
}

/// The fit procedure of `algo.farrington.fitGLM.flexible`: fit, estimate
/// overdispersion, reweight by Anscombe residuals, refit.
fn fit_flexible(d: &ModelData, trend: bool, p: &FarringtonParams) -> Option<(Fit, f64, bool)> {
    let factors = p.no_periods != 1;
    let mut use_trend = trend;
    let mut x = design(d, use_trend, factors);
    let ones = vec![1.0; d.y.len()];
    let mut fit = fit_quasi_poisson(&x, &d.y, &ones)?;
    if !fit.converged && use_trend {
        use_trend = false;
        x = design(d, use_trend, factors);
        fit = fit_quasi_poisson(&x, &d.y, &ones)?;
    }
    if !fit.converged {
        return None;
    }
    let mut phi = fit.dispersion.max(1.0);
    if p.reweight {
        let s: Vec<f64> = (0..d.y.len())
            .map(|i| {
                let (yi, mi) = (d.y[i], fit.mu[i]);
                1.5 * (yi.powf(2.0 / 3.0) * mi.powf(-1.0 / 6.0) - mi.sqrt())
                    / (phi * (1.0 - fit.hat[i])).sqrt()
            })
            .collect();
        let thr = p.weights_threshold;
        let denom: f64 = s
            .iter()
            .map(|&si| if si > thr { si.powi(-2) } else { 1.0 })
            .sum();
        let gamma = s.len() as f64 / denom;
        let omega: Vec<f64> = s
            .iter()
            .map(|&si| if si > thr { gamma * si.powi(-2) } else { gamma })
            .collect();
        fit = fit_quasi_poisson(&x, &d.y, &omega)?;
        phi = fit.dispersion.max(1.0);
        // surveillance 1.26.1 then stores the reweighting weights in
        // `model$weights`, so `summary.glm` and `predict.glm` (trend test and
        // prediction standard error) see a dispersion of
        // sum(omega * ((y - mu)/mu)^2) / df. Reproduced here so results match
        // the reference implementation; phi itself is unaffected.
        let quirk: f64 = (0..d.y.len())
            .map(|i| omega[i] * ((d.y[i] - fit.mu[i]) / fit.mu[i]).powi(2))
            .sum();
        fit.dispersion = quirk / fit.df_residual;
    }
    Some((fit, phi, use_trend))
}

/// Negative-binomial CDF P(X ≤ x) with R's (size, prob) parametrization.
fn pnbinom(x: f64, size: f64, prob: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    beta_reg(size, x.floor() + 1.0, prob)
}

/// Poisson CDF P(X ≤ x).
fn ppois(x: f64, lambda: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    gamma_ur(x.floor() + 1.0, lambda)
}

/// Smallest integer x with F(x) ≥ q (with R's 64·ε fuzz).
fn discrete_quantile(q: f64, mean: f64, sd: f64, cdf: impl Fn(f64) -> f64) -> f64 {
    let target = q * (1.0 - 64.0 * f64::EPSILON);
    let mut x = (mean + z(q) * sd).floor().max(0.0);
    if cdf(x) >= target {
        while x > 0.0 && cdf(x - 1.0) >= target {
            x -= 1.0;
        }
    } else {
        while cdf(x) < target {
            x += 1.0;
        }
    }
    x
}

fn qnbinom(q: f64, size: f64, prob: f64) -> f64 {
    let mean = size * (1.0 - prob) / prob;
    let sd = (mean / prob).sqrt();
    discrete_quantile(q, mean, sd, |x| pnbinom(x, size, prob))
}

fn qpois(q: f64, lambda: f64) -> f64 {
    discrete_quantile(q, lambda, lambda.sqrt(), |x| ppois(x, lambda))
}

/// Normal CDF with mean and sd.
fn pnorm(x: f64, m: f64, s: f64) -> f64 {
    crate::dist::phi((x - m) / s)
}

/// Run the Farrington-type algorithm for the 0-based periods in `range`
/// (default: every period with enough history).
pub fn farrington(
    counts: &[f64],
    p: &FarringtonParams,
    range: Option<std::ops::Range<usize>>,
) -> Result<Vec<FarringtonPoint>, String> {
    if counts.iter().any(|c| !c.is_finite() || *c < 0.0) {
        return Err("counts must be non-negative numbers".into());
    }
    if p.freq < 2 || p.b < 1 || p.no_periods < 1 {
        return Err("Farrington needs freq ≥ 2, b ≥ 1, and no_periods ≥ 1".into());
    }
    if p.no_periods > 1 && p.freq < 2 * p.w + 1 + p.no_periods {
        return Err("too many seasonal periods for this frequency and window".into());
    }
    if !(0.0 < p.alpha && p.alpha < 0.5) {
        return Err("alpha must be between 0 and 0.5".into());
    }
    let n = counts.len();
    let min_hist = p.min_history();
    if n < min_hist {
        return Err(format!(
            "the Farrington method with b = {} years of history needs at least {min_hist} periods \
             (got {n})",
            p.b
        ));
    }
    let range = range.unwrap_or(min_hist - 1..n);
    if range.start + 1 < min_hist || range.end > n {
        return Err(format!(
            "periods before position {} do not have {} years of history",
            min_hist - 1,
            p.b
        ));
    }
    let factors = p.no_periods != 1;
    let mut out = Vec::new();
    for k0 in range {
        let k = k0 + 1; // 1-based like R
        let blk = blocks(n, k, p);
        let excl_from = k.saturating_sub(p.past_periods_not_included);
        let mut d = ModelData {
            y: Vec::new(),
            wtime: Vec::new(),
            level: Vec::new(),
            levels: Vec::new(),
        };
        for t in 1..=n {
            if t >= excl_from && t <= k {
                continue;
            }
            if let Some(l) = blk[t] {
                d.y.push(counts[t - 1]);
                d.wtime.push(t as f64);
                d.level.push(l);
            }
        }
        let mut levels: Vec<usize> = d.level.clone();
        levels.sort_unstable();
        levels.dedup();
        d.levels = levels;
        let level_now = p.no_periods;
        if factors && !d.levels.contains(&level_now) {
            return Err("internal: the current season level has no reference data".into());
        }

        let fitted = fit_flexible(&d, p.trend, p);
        let Some((mut fit, mut phi, mut has_trend)) = fitted else {
            out.push(FarringtonPoint {
                index: k0,
                observed: counts[k0],
                expected: None,
                upper_bound: None,
                score: None,
                p_value: None,
                alarm: false,
                trend: false,
                phi: f64::NAN,
                too_few_cases: false,
            });
            continue;
        };
        let mut do_trend = false;
        if has_trend {
            let x0 = row(k as f64, level_now, &d.levels, true, factors);
            let eta: f64 = x0.iter().zip(&fit.beta).map(|(a, b)| a * b).sum();
            let se = (fit.cov[1][1] * fit.dispersion).sqrt();
            let pval = p_t_two_sided(fit.beta[1] / se, fit.df_residual);
            let significant = pval < p.p_threshold_trend;
            let max_y = d.y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let no_extrapolation = eta.exp() <= max_y;
            do_trend = p.b >= 3 && significant && no_extrapolation;
            if !do_trend {
                match fit_flexible(&d, false, p) {
                    Some((f, ph, t)) => {
                        fit = f;
                        phi = ph;
                        has_trend = t;
                    }
                    None => {
                        out.push(FarringtonPoint {
                            index: k0,
                            observed: counts[k0],
                            expected: None,
                            upper_bound: None,
                            score: None,
                            p_value: None,
                            alarm: false,
                            trend: false,
                            phi: f64::NAN,
                            too_few_cases: false,
                        });
                        continue;
                    }
                }
            }
        }
        let x0 = row(k as f64, level_now, &d.levels, has_trend, factors);
        let eta0: f64 = x0.iter().zip(&fit.beta).map(|(a, b)| a * b).sum();
        let se_eta0 = (quad(&fit.cov, &x0) * fit.dispersion).sqrt();
        let y = counts[k0];
        let (upper, prob, score, expected) = match p.threshold_method {
            ThresholdMethod::Delta => {
                let mu0 = eta0.exp();
                let se_mu = se_eta0 * mu0;
                let tau = phi + se_mu * se_mu / mu0;
                let e = 2.0 / 3.0;
                let se = (4.0 / 9.0 * mu0.powf(1.0 / 3.0) * tau).sqrt();
                let up = (mu0.powf(e) + z(1.0 - p.alpha) * se).powf(1.0 / e);
                let prob = 1.0 - pnorm(y.powf(e), mu0.powf(e), se);
                let score = (y - mu0) / (up - mu0);
                (Some(up), Some(prob), Some(score), mu0)
            }
            ThresholdMethod::NbPlugin | ThresholdMethod::Muan => {
                let mu_q = if p.threshold_method == ThresholdMethod::NbPlugin {
                    eta0.exp()
                } else {
                    (eta0 + z(1.0 - p.alpha) * se_eta0).exp()
                };
                if !mu_q.is_finite() {
                    (None, None, None, eta0.exp())
                } else {
                    let (up, prob) = if phi > 1.0 {
                        let size = mu_q / (phi - 1.0);
                        let pr = 1.0 / phi;
                        (
                            qnbinom(1.0 - p.alpha, size, pr),
                            1.0 - pnbinom(y - 1.0, size, pr),
                        )
                    } else {
                        (qpois(1.0 - p.alpha, mu_q), 1.0 - ppois(y - 1.0, mu_q))
                    };
                    // R computes the score against the linear predictor here.
                    let score = (y - eta0) / (up - eta0);
                    (Some(up), Some(prob), Some(score), eta0.exp())
                }
            }
        };
        let lo = (k0 + 1).saturating_sub(p.limit_periods);
        let recent: f64 = counts[lo..=k0].iter().sum();
        let enough = recent >= p.limit_cases;
        let alarm = enough && score.is_some_and(|s| s > 1.0) && y != 0.0;
        out.push(FarringtonPoint {
            index: k0,
            observed: y,
            expected: enough.then_some(expected),
            upper_bound: if enough { upper } else { None },
            score: if enough { score } else { None },
            p_value: prob,
            alarm,
            trend: do_trend,
            phi,
            too_few_cases: !enough,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ears_c1_by_hand() {
        let c = [2.0, 3.0, 2.0, 4.0, 3.0, 2.0, 3.0, 12.0];
        let r = ears(&c, EarsMethod::C1, &EarsParams::classic(EarsMethod::C1)).unwrap();
        assert_eq!(r.len(), 1);
        let m = (2.0 + 3.0 + 2.0 + 4.0 + 3.0 + 2.0 + 3.0) / 7.0;
        assert!((r[0].baseline_mean - m).abs() < 1e-12);
        assert!(r[0].alarm);
        assert!(r[0].statistic > 3.0);
    }

    #[test]
    fn ears_flat_baseline_is_infinite_not_nan() {
        let c = [0.0; 8];
        let mut c = c.to_vec();
        c[7] = 1.0;
        let r = ears(&c, EarsMethod::C1, &EarsParams::classic(EarsMethod::C1)).unwrap();
        assert!(r[0].statistic.is_infinite() && r[0].alarm);
        let r = ears(
            &c,
            EarsMethod::C1,
            &EarsParams {
                min_sigma: 0.5,
                ..EarsParams::classic(EarsMethod::C1)
            },
        )
        .unwrap();
        assert!((r[0].statistic - 2.0).abs() < 1e-12 && !r[0].alarm);
    }

    #[test]
    fn cusum_by_hand() {
        let c = [4.0, 4.0, 4.0, 4.0, 8.0, 8.0];
        let r = cusum(
            &c,
            &CusumParams {
                k: 0.5,
                h: 4.0,
                baseline_periods: 4,
                mean: None,
                reset: false,
            },
        )
        .unwrap();
        assert_eq!(r.mean, 4.0);
        assert!((r.points[0].cusum - 1.5).abs() < 1e-12);
        assert!((r.points[1].cusum - 3.0).abs() < 1e-12);
        assert!(!r.points[1].alarm);
    }

    #[test]
    fn discrete_quantiles() {
        // R 4.3.3: qpois(0.95, 3.2) = 6; qnbinom(0.95, 2, 0.4) = 8;
        // pnbinom(4, 2, 0.4) = 0.76672
        assert_eq!(qpois(0.95, 3.2), 6.0);
        assert_eq!(qnbinom(0.95, 2.0, 0.4), 8.0);
        assert!((pnbinom(4.0, 2.0, 0.4) - 0.76672).abs() < 1e-9);
    }

    #[test]
    fn blocks_match_r_layout() {
        // freq 52, b 1, w 3, 10 levels: the reference window gets level 10,
        // the 45 periods between it and the current window get levels 1..9.
        let p = FarringtonParams {
            b: 1,
            ..FarringtonParams::noufaily_weekly()
        };
        let k = 60;
        let f = blocks(70, k, &p);
        assert_eq!(f[k - 52 - 3], Some(10));
        assert_eq!(f[k - 52 + 3], Some(10));
        assert_eq!(f[k - 52 + 4], Some(1));
        assert_eq!(f[k - 4], Some(9));
        assert_eq!(f[k - 3], Some(10));
        // 45 periods into 9 blocks of 5.
        assert_eq!(f[k - 52 + 8], Some(1));
        assert_eq!(f[k - 52 + 9], Some(2));
    }
}
