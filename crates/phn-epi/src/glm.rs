//! Generalized linear models fitted by iteratively reweighted least squares.
//!
//! Families and links follow R's `glm` (and `MASS::glm.nb` for the negative
//! binomial), so coefficients, standard errors, deviances, and AIC agree with
//! R; `tests/golden.rs` checks this.
//!
//! * Gaussian, identity link (ordinary or weighted least squares), t tests.
//! * Poisson, log link, optional offset (log person-time), z tests.
//! * Binomial, logit link, successes out of trials, z tests.
//! * Negative binomial (NB2, variance μ + μ²/θ), log link, θ estimated by
//!   maximum likelihood alternating with the IRLS fit, as `glm.nb`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use statrs::function::gamma::{digamma, ln_gamma};

use crate::dist::{p_normal_two_sided, p_t_two_sided, t_quantile, z_two_sided};
use crate::linalg::{cholesky, cholesky_inverse, cholesky_solve, weighted_normal_equations};

/// Model family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Gaussian,
    Poisson,
    Binomial,
    NegativeBinomial,
}

/// Data for a fit. `x` rows include the intercept column when wanted.
#[derive(Debug, Clone, Default)]
pub struct GlmData {
    /// Response: values (Gaussian), counts (Poisson, NB), or successes (binomial).
    pub y: Vec<f64>,
    /// Design matrix, one row per observation.
    pub x: Vec<Vec<f64>>,
    /// Coefficient names (same length as each row of `x`).
    pub names: Vec<String>,
    /// Offset on the linear-predictor scale (e.g. ln population).
    pub offset: Option<Vec<f64>>,
    /// Trials per observation (binomial only).
    pub trials: Option<Vec<f64>>,
}

/// One coefficient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Coefficient {
    pub name: String,
    pub estimate: f64,
    pub std_error: f64,
    /// z (or t for Gaussian) statistic.
    pub statistic: f64,
    pub p_value: f64,
    pub lower: f64,
    pub upper: f64,
}

/// Fitted probabilities this close to 0 or 1 signal separation.
const SEPARATION_EPS: f64 = 1e-7;

/// A fitted model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlmFit {
    pub family: Family,
    pub coefficients: Vec<Coefficient>,
    /// Covariance matrix of the coefficients.
    #[schemars(skip)]
    #[serde(skip)]
    pub covariance: Vec<Vec<f64>>,
    pub deviance: f64,
    pub null_deviance: f64,
    pub df_residual: f64,
    pub df_null: f64,
    pub log_likelihood: f64,
    pub aic: f64,
    pub bic: f64,
    /// Dispersion used for standard errors (1 except Gaussian).
    pub dispersion: f64,
    /// Pearson chi-square; ÷ df_residual well above 1 suggests overdispersion.
    pub pearson_chi2: f64,
    /// Negative binomial θ (smaller = more overdispersion) and its SE.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta_std_error: Option<f64>,
    pub iterations: usize,
    pub converged: bool,
    /// Fitted means.
    #[schemars(skip)]
    #[serde(skip)]
    pub fitted: Vec<f64>,
    pub n: usize,
    pub confidence: f64,
}

/// Fit a GLM. Errors describe problems the analyst can fix (collinearity,
/// invalid responses, too few observations).
pub fn fit(data: &GlmData, family: Family, confidence: f64) -> Result<GlmFit, String> {
    validate(data, family)?;
    match family {
        Family::NegativeBinomial => fit_nb(data, confidence),
        f => {
            let core = irls(data, f, None, None)?;
            finish(data, f, core, None, None, confidence)
        }
    }
}

fn validate(d: &GlmData, family: Family) -> Result<(), String> {
    let n = d.y.len();
    let p = d.names.len();
    if d.x.len() != n || d.x.iter().any(|r| r.len() != p) {
        return Err("the design matrix does not match the data".into());
    }
    if n <= p {
        return Err(format!(
            "{n} observations are not enough to estimate {p} coefficients"
        ));
    }
    if d.y.iter().any(|v| !v.is_finite()) || d.x.iter().flatten().any(|v| !v.is_finite()) {
        return Err("the data contain missing or non-numeric values".into());
    }
    match family {
        Family::Poisson | Family::NegativeBinomial => {
            if d.y.iter().any(|v| *v < 0.0) {
                return Err("counts cannot be negative".into());
            }
            if d.y.iter().all(|v| *v == 0.0) {
                return Err(
                    "every count is zero, so there are no events to model a rate or trend from"
                        .into(),
                );
            }
        }
        Family::Binomial => {
            let t = d
                .trials
                .as_ref()
                .ok_or("binomial models need the number of trials")?;
            if t.len() != n
                || d.y
                    .iter()
                    .zip(t)
                    .any(|(y, t)| *t <= 0.0 || *y < 0.0 || y > t)
            {
                return Err("each outcome must be between 0 and its number of trials".into());
            }
        }
        Family::Gaussian => {}
    }
    if let Some(o) = &d.offset
        && (o.len() != n || o.iter().any(|v| !v.is_finite()))
    {
        return Err("the offset must be finite for every observation".into());
    }
    Ok(())
}

struct Core {
    beta: Vec<f64>,
    mu: Vec<f64>,
    xtwx_inv: Vec<Vec<f64>>,
    iterations: usize,
    converged: bool,
}

/// Response on the modeling scale (proportions for binomial) and prior weights.
fn response(d: &GlmData, family: Family) -> (Vec<f64>, Vec<f64>) {
    match family {
        Family::Binomial => {
            let t = d.trials.as_ref().expect("validated");
            (d.y.iter().zip(t).map(|(y, t)| y / t).collect(), t.clone())
        }
        _ => (d.y.clone(), vec![1.0; d.y.len()]),
    }
}

fn linkinv(family: Family, eta: f64) -> f64 {
    match family {
        Family::Gaussian => eta,
        Family::Poisson | Family::NegativeBinomial => eta.clamp(-700.0, 700.0).exp(),
        Family::Binomial => {
            let e = (-eta).clamp(-700.0, 700.0).exp();
            (1.0 / (1.0 + e)).clamp(f64::EPSILON, 1.0 - f64::EPSILON)
        }
    }
}

fn link(family: Family, mu: f64) -> f64 {
    match family {
        Family::Gaussian => mu,
        Family::Poisson | Family::NegativeBinomial => mu.ln(),
        Family::Binomial => (mu / (1.0 - mu)).ln(),
    }
}

/// dη/dμ
fn dlink(family: Family, mu: f64) -> f64 {
    match family {
        Family::Gaussian => 1.0,
        Family::Poisson | Family::NegativeBinomial => 1.0 / mu,
        Family::Binomial => 1.0 / (mu * (1.0 - mu)),
    }
}

fn variance(family: Family, mu: f64, theta: Option<f64>) -> f64 {
    match family {
        Family::Gaussian => 1.0,
        Family::Poisson => mu,
        Family::Binomial => mu * (1.0 - mu),
        Family::NegativeBinomial => mu + mu * mu / theta.expect("theta"),
    }
}

fn ylogy(y: f64, mu: f64) -> f64 {
    if y > 0.0 { y * (y / mu).ln() } else { 0.0 }
}

fn unit_deviance(family: Family, y: f64, mu: f64, theta: Option<f64>) -> f64 {
    match family {
        Family::Gaussian => (y - mu).powi(2),
        Family::Poisson => 2.0 * (ylogy(y, mu) - (y - mu)),
        Family::Binomial => 2.0 * (ylogy(y, mu) + ylogy(1.0 - y, 1.0 - mu)),
        Family::NegativeBinomial => {
            let th = theta.expect("theta");
            2.0 * (ylogy(y, mu) - (y + th) * ((y + th) / (mu + th)).ln())
        }
    }
}

fn deviance(family: Family, y: &[f64], mu: &[f64], w: &[f64], theta: Option<f64>) -> f64 {
    y.iter()
        .zip(mu)
        .zip(w)
        .map(|((y, m), w)| w * unit_deviance(family, *y, *m, theta))
        .sum()
}

fn irls(
    d: &GlmData,
    family: Family,
    theta: Option<f64>,
    mu_start: Option<&[f64]>,
) -> Result<Core, String> {
    let (y, pw) = response(d, family);
    let n = y.len();
    let off = d.offset.clone().unwrap_or_else(|| vec![0.0; n]);
    let mut mu: Vec<f64> = match mu_start {
        Some(m) => m.to_vec(),
        None => y
            .iter()
            .zip(&pw)
            .map(|(y, w)| match family {
                Family::Gaussian => *y,
                Family::Poisson | Family::NegativeBinomial => y + 0.1,
                Family::Binomial => (w * y + 0.5) / (w + 1.0),
            })
            .collect(),
    };
    let mut eta: Vec<f64> = mu.iter().map(|m| link(family, *m)).collect();
    let mut dev_old = f64::INFINITY;
    let mut beta = vec![0.0; d.names.len()];
    let mut converged = false;
    let mut iterations = 0;
    let mut l_final = None;
    for it in 1..=100 {
        iterations = it;
        let mut z = vec![0.0; n];
        let mut w = vec![0.0; n];
        for i in 0..n {
            let g = dlink(family, mu[i]);
            z[i] = (eta[i] - off[i]) + (y[i] - mu[i]) * g;
            w[i] = pw[i] / (g * g * variance(family, mu[i], theta));
        }
        let (xtwx, xtwz) = weighted_normal_equations(&d.x, &w, &z);
        let l = cholesky(&xtwx).ok_or(
            "the predictors are collinear (one is a combination of others) or a group has no \
             variation; remove or combine predictors",
        )?;
        let new_beta = cholesky_solve(&l, &xtwz);
        for i in 0..n {
            eta[i] = off[i]
                + d.x[i]
                    .iter()
                    .zip(&new_beta)
                    .map(|(a, b)| a * b)
                    .sum::<f64>();
            mu[i] = linkinv(family, eta[i]);
        }
        beta = new_beta;
        let dev = deviance(family, &y, &mu, &pw, theta);
        if !dev.is_finite() {
            return Err("the model did not converge (the fitted values diverged)".into());
        }
        let change = (dev - dev_old).abs() / (dev.abs() + 0.1);
        // Converged in R's sense (`glm.control(epsilon = 1e-8)`). Iterations
        // continue to 1e-12 for extra precision; when the fit is nearly
        // exact, rounding in the deviance can keep the change just above
        // that, which is not a failure to converge.
        converged = change < 1e-8;
        if change < 1e-12 {
            l_final = Some(l);
            break;
        }
        dev_old = dev;
        l_final = Some(l);
    }
    // Covariance from the weights at the final estimates.
    let mut w = vec![0.0; n];
    for i in 0..n {
        let g = dlink(family, mu[i]);
        w[i] = pw[i] / (g * g * variance(family, mu[i], theta));
    }
    let (xtwx, _) = weighted_normal_equations(&d.x, &w, &vec![0.0; n]);
    let l = cholesky(&xtwx)
        .or(l_final)
        .ok_or("the model could not be fitted")?;
    Ok(Core {
        beta,
        mu,
        xtwx_inv: cholesky_inverse(&l),
        iterations,
        converged,
    })
}

fn log_likelihood(family: Family, d: &GlmData, mu: &[f64], theta: Option<f64>, dev: f64) -> f64 {
    match family {
        Family::Gaussian => {
            let n = d.y.len() as f64;
            -0.5 * n * ((2.0 * std::f64::consts::PI * dev / n).ln() + 1.0)
        }
        Family::Poisson => {
            d.y.iter()
                .zip(mu)
                .map(|(y, m)| y * m.ln() - m - ln_gamma(y + 1.0))
                .sum()
        }
        Family::Binomial => {
            let t = d.trials.as_ref().expect("validated");
            d.y.iter()
                .zip(t)
                .zip(mu)
                .map(|((y, t), m)| {
                    ln_gamma(t + 1.0) - ln_gamma(y + 1.0) - ln_gamma(t - y + 1.0)
                        + if *y > 0.0 { y * m.ln() } else { 0.0 }
                        + if t - y > 0.0 {
                            (t - y) * (1.0 - m).ln()
                        } else {
                            0.0
                        }
                })
                .sum()
        }
        Family::NegativeBinomial => {
            let th = theta.expect("theta");
            d.y.iter()
                .zip(mu)
                .map(|(y, m)| {
                    ln_gamma(th + y) - ln_gamma(th) - ln_gamma(y + 1.0)
                        + th * (th / (th + m)).ln()
                        + if *y > 0.0 {
                            y * (m / (th + m)).ln()
                        } else {
                            0.0
                        }
                })
                .sum()
        }
    }
}

fn finish(
    d: &GlmData,
    family: Family,
    core: Core,
    theta: Option<f64>,
    theta_se: Option<f64>,
    confidence: f64,
) -> Result<GlmFit, String> {
    let (y, pw) = response(d, family);
    let n = y.len();
    let p = d.names.len();
    let dev = deviance(family, &y, &core.mu, &pw, theta);
    let df_res = (n - p) as f64;
    let pearson: f64 = (0..n)
        .map(|i| pw[i] * (y[i] - core.mu[i]).powi(2) / variance(family, core.mu[i], theta))
        .sum();
    let dispersion = if family == Family::Gaussian {
        dev / df_res
    } else {
        1.0
    };
    let crit = if family == Family::Gaussian {
        t_quantile(1.0 - (1.0 - confidence) / 2.0, df_res)
    } else {
        z_two_sided(confidence)
    };
    let coefficients = (0..p)
        .map(|j| {
            let se = (dispersion * core.xtwx_inv[j][j]).sqrt();
            let est = core.beta[j];
            let stat = est / se;
            Coefficient {
                name: d.names[j].clone(),
                estimate: est,
                std_error: se,
                statistic: stat,
                p_value: if family == Family::Gaussian {
                    p_t_two_sided(stat, df_res)
                } else {
                    p_normal_two_sided(stat)
                },
                lower: est - crit * se,
                upper: est + crit * se,
            }
        })
        .collect();
    let covariance = core
        .xtwx_inv
        .iter()
        .map(|r| r.iter().map(|v| v * dispersion).collect())
        .collect();
    // Null deviance: intercept-only model (with the offset), when there is an intercept.
    let has_intercept = d.x.iter().all(|r| (r[0] - 1.0).abs() < 1e-12);
    let null_deviance = if has_intercept {
        let nd = GlmData {
            y: d.y.clone(),
            x: vec![vec![1.0]; n],
            names: vec!["(Intercept)".into()],
            offset: d.offset.clone(),
            trials: d.trials.clone(),
        };
        irls(&nd, family, theta, None)
            .map(|c| deviance(family, &y, &c.mu, &pw, theta))
            .unwrap_or(f64::NAN)
    } else {
        deviance(
            family,
            &y,
            &vec![if family == Family::Binomial { 0.5 } else { 0.0 }; n],
            &pw,
            theta,
        )
    };
    let ll = log_likelihood(family, d, &core.mu, theta, dev);
    let k = p as f64
        + match family {
            Family::Gaussian | Family::NegativeBinomial => 1.0,
            _ => 0.0,
        };
    Ok(GlmFit {
        family,
        coefficients,
        covariance,
        deviance: dev,
        null_deviance,
        df_residual: df_res,
        df_null: (n - usize::from(has_intercept)) as f64,
        log_likelihood: ll,
        aic: -2.0 * ll + 2.0 * k,
        bic: -2.0 * ll + k * (n as f64).ln(),
        dispersion,
        pearson_chi2: pearson,
        theta,
        theta_std_error: theta_se,
        iterations: core.iterations,
        converged: core.converged,
        fitted: core.mu,
        n,
        confidence,
    })
}

/// Trigamma function ψ₁(x) for x > 0 (recurrence then asymptotic series).
pub fn trigamma(mut x: f64) -> f64 {
    let mut acc = 0.0;
    while x < 10.0 {
        acc += 1.0 / (x * x);
        x += 1.0;
    }
    let x2 = 1.0 / (x * x);
    acc + 1.0 / x
        + x2 / 2.0
        + (1.0 / x) * x2 * (1.0 / 6.0 - x2 * (1.0 / 30.0 - x2 * (1.0 / 42.0 - x2 * (1.0 / 30.0))))
}

/// Maximum-likelihood θ for given means (as `MASS::theta.ml`), with its SE.
fn theta_ml(y: &[f64], mu: &[f64]) -> Option<(f64, f64)> {
    let n = y.len() as f64;
    let denom: f64 = y.iter().zip(mu).map(|(y, m)| (y / m - 1.0).powi(2)).sum();
    let mut th = n / denom;
    if !th.is_finite() || th <= 0.0 {
        th = 1.0;
    }
    let mut info = 0.0;
    for _ in 0..200 {
        th = th.abs();
        let score: f64 = y
            .iter()
            .zip(mu)
            .map(|(y, m)| {
                digamma(th + y) - digamma(th) + th.ln() + 1.0 - (th + m).ln() - (y + th) / (m + th)
            })
            .sum();
        info = y
            .iter()
            .zip(mu)
            .map(|(y, m)| {
                -trigamma(th + y) + trigamma(th) - 1.0 / th + 2.0 / (m + th)
                    - (y + th) / (m + th).powi(2)
            })
            .sum();
        let del = score / info;
        th += del;
        if !th.is_finite() || th > 1e8 {
            return None;
        }
        if del.abs() <= 1e-12 * th.max(1.0) {
            break;
        }
    }
    (info > 0.0).then(|| (th, (1.0 / info).sqrt()))
}

fn fit_nb(d: &GlmData, confidence: f64) -> Result<GlmFit, String> {
    let mut core = irls(d, Family::Poisson, None, None)?;
    let (mut th, mut th_se) = theta_ml(&d.y, &core.mu).ok_or(
        "the data show no overdispersion (the negative binomial reduces to Poisson); use a \
         Poisson model",
    )?;
    let mut ll_old = f64::NEG_INFINITY;
    for _ in 0..100 {
        core = irls(d, Family::NegativeBinomial, Some(th), Some(&core.mu))?;
        let t0 = th;
        let (t, se) = theta_ml(&d.y, &core.mu).ok_or(
            "the data show no overdispersion (θ grows without bound); use a Poisson model",
        )?;
        th = t;
        th_se = se;
        let (y, pw) = response(d, Family::NegativeBinomial);
        let dev = deviance(Family::NegativeBinomial, &y, &core.mu, &pw, Some(th));
        let ll = log_likelihood(Family::NegativeBinomial, d, &core.mu, Some(th), dev);
        if (ll - ll_old).abs() < 1e-11 && (t0 - th).abs() < 1e-10 * th.max(1.0) {
            break;
        }
        ll_old = ll;
    }
    let core = irls(d, Family::NegativeBinomial, Some(th), Some(&core.mu))?;
    finish(
        d,
        Family::NegativeBinomial,
        core,
        Some(th),
        Some(th_se),
        confidence,
    )
}

impl GlmFit {
    /// Estimate and CI of a linear combination cᵀβ (on the link scale).
    pub fn linear_combination(&self, c: &[f64]) -> (f64, f64, f64) {
        let est: f64 = c
            .iter()
            .zip(&self.coefficients)
            .map(|(a, b)| a * b.estimate)
            .sum();
        let se = crate::linalg::quad_form(c, &self.covariance).sqrt();
        let crit = if self.family == Family::Gaussian {
            t_quantile(1.0 - (1.0 - self.confidence) / 2.0, self.df_residual)
        } else {
            z_two_sided(self.confidence)
        };
        (est, est - crit * se, est + crit * se)
    }

    /// True when a logistic model has fitted probabilities that are
    /// numerically 0 or 1. This is the sign of complete or quasi-complete
    /// separation: some predictor (or combination) predicts the outcome
    /// perfectly, the maximum-likelihood estimate does not exist, and the
    /// reported coefficients, standard errors, and Wald p-values are not
    /// usable (R warns "fitted probabilities numerically 0 or 1 occurred").
    pub fn has_separation(&self) -> bool {
        self.family == Family::Binomial
            && self
                .fitted
                .iter()
                .any(|m| *m < SEPARATION_EPS || *m > 1.0 - SEPARATION_EPS)
    }

    /// The coefficient with this name.
    pub fn coef(&self, name: &str) -> Option<&Coefficient> {
        self.coefficients.iter().find(|c| c.name == name)
    }

    /// Pearson χ² ÷ residual df.
    pub fn overdispersion(&self) -> f64 {
        self.pearson_chi2 / self.df_residual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigamma_values() {
        // ψ₁(1) = π²/6; ψ₁(0.5) = π²/2.
        let pi2 = std::f64::consts::PI.powi(2);
        assert!((trigamma(1.0) - pi2 / 6.0).abs() < 1e-11);
        assert!((trigamma(0.5) - pi2 / 2.0).abs() < 1e-11);
    }

    #[test]
    fn poisson_with_offset_recovers_rate() {
        // Constant rate 2 per 1000 with a perfect fit.
        let pops = [1000.0, 2000.0, 4000.0, 8000.0];
        let d = GlmData {
            y: pops.iter().map(|p| p * 0.002).collect(),
            x: pops.iter().map(|_| vec![1.0]).collect(),
            names: vec!["(Intercept)".into()],
            offset: Some(pops.iter().map(|p: &f64| p.ln()).collect()),
            trials: None,
        };
        let f = fit(&d, Family::Poisson, 0.95).unwrap();
        assert!((f.coefficients[0].estimate.exp() - 0.002).abs() < 1e-12);
        assert!(f.deviance.abs() < 1e-9 && f.converged);
    }

    #[test]
    fn a_nearly_exact_fit_counts_as_converged() {
        // Counts that follow a smooth trend almost exactly: the deviance is
        // tiny, so its rounding error can keep the relative change above
        // 1e-12 for all 100 iterations. That used to leave `converged` false.
        let y: Vec<f64> = (0..8)
            .map(|i| (5000.0 * (0.03 * f64::from(i)).exp()).round())
            .collect();
        let d = GlmData {
            x: (0..8).map(|i| vec![1.0, f64::from(i)]).collect(),
            y,
            names: vec!["(Intercept)".into(), "t".into()],
            ..GlmData::default()
        };
        let f = fit(&d, Family::Poisson, 0.95).unwrap();
        assert!(f.converged, "{} iterations", f.iterations);
        assert!((f.coefficients[1].estimate - 0.03).abs() < 1e-4);
    }

    #[test]
    fn collinear_predictors_are_reported() {
        let d = GlmData {
            y: vec![1.0, 2.0, 3.0, 4.0],
            x: vec![
                vec![1.0, 1.0, 2.0],
                vec![1.0, 2.0, 4.0],
                vec![1.0, 3.0, 6.0],
                vec![1.0, 4.0, 8.0],
            ],
            names: vec!["(Intercept)".into(), "a".into(), "b".into()],
            ..GlmData::default()
        };
        assert!(
            fit(&d, Family::Gaussian, 0.95)
                .unwrap_err()
                .contains("collinear")
        );
    }

    /// One row per person: `exposed_ill` of 10 exposed and `unexposed_ill`
    /// of 10 unexposed people are ill.
    fn exposure_data(exposed_ill: usize, unexposed_ill: usize) -> GlmData {
        let mut d = GlmData {
            names: vec!["(Intercept)".into(), "exposed".into()],
            ..GlmData::default()
        };
        for (exposed, ill) in [(1.0, exposed_ill), (0.0, unexposed_ill)] {
            for i in 0..10 {
                d.y.push(if i < ill { 1.0 } else { 0.0 });
                d.x.push(vec![1.0, exposed]);
            }
        }
        d.trials = Some(vec![1.0; d.y.len()]);
        d
    }

    #[test]
    fn separation_is_flagged() {
        // Exposed 10/10 ill, unexposed 3/10: the exposure predicts illness
        // perfectly among the exposed, so the slope runs off to infinity
        // (about 30 when the iterations stop) with a Wald p-value near 1.
        let f = fit(&exposure_data(10, 3), Family::Binomial, 0.95).unwrap();
        assert!(f.has_separation());
        let slope = f.coef("exposed").unwrap();
        assert!(slope.estimate > 15.0 && slope.p_value > 0.9, "{slope:?}");
        // Exposed 7/10, unexposed 3/10: an ordinary fit, odds ratio
        // (7·7)/(3·3) = 5.44.
        let f = fit(&exposure_data(7, 3), Family::Binomial, 0.95).unwrap();
        assert!(!f.has_separation() && f.converged);
        let slope = f.coef("exposed").unwrap();
        assert!((slope.estimate.exp() - 49.0 / 9.0).abs() < 1e-6);
        // Only logistic models can be separated.
        let d = GlmData {
            y: vec![1.0, 2.0, 4.0, 3.0],
            x: (0..4).map(|i| vec![1.0, f64::from(i)]).collect(),
            names: vec!["(Intercept)".into(), "t".into()],
            ..GlmData::default()
        };
        assert!(!fit(&d, Family::Poisson, 0.95).unwrap().has_separation());
    }

    #[test]
    fn all_zero_counts_are_rejected_with_a_plain_message() {
        let d = GlmData {
            y: vec![0.0; 5],
            x: (0..5).map(|i| vec![1.0, f64::from(i)]).collect(),
            names: vec!["(Intercept)".into(), "t".into()],
            ..GlmData::default()
        };
        for family in [Family::Poisson, Family::NegativeBinomial] {
            let e = fit(&d, family, 0.95).unwrap_err();
            assert!(e.contains("every count is zero"), "{e}");
        }
    }
}
