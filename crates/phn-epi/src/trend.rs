//! Trend analysis for counts and rates over time.
//!
//! * Annual (or per-period) percent change from a log-linear Poisson or
//!   negative binomial model with an offset for population:
//!   APC = 100 (e^β − 1).
//! * Joinpoint-style segmented regression: continuous piecewise log-linear
//!   models with 0…K joinpoints at observed time points (grid search, as the
//!   NCI Joinpoint program does by default), the number of joinpoints chosen
//!   by BIC, segment APCs, and the average annual percent change (AAPC).
//!   NCI Joinpoint uses permutation tests by default; BIC is one of its
//!   documented alternatives. Intervals are conditional on the joinpoint
//!   locations, so they are somewhat too narrow.
//! * Interrupted time series (segmented regression at a known date): level
//!   change (rate ratio) and slope change.
//! * Seasonality: one annual harmonic (sine and cosine) for sub-annual data,
//!   tested with a likelihood-ratio test.
//! * Mann–Kendall trend test with Sen's slope (nonparametric check), and the
//!   Durbin–Watson statistic on Pearson residuals (autocorrelation check).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dist::{p_chi_square, p_normal_two_sided};
use crate::glm::{Family, GlmData, GlmFit, fit};

/// Which count model to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelChoice {
    /// Poisson, switching to negative binomial when the Pearson dispersion exceeds 1.5.
    #[default]
    Auto,
    Poisson,
    NegativeBinomial,
}

/// A time series of counts.
#[derive(Debug, Clone)]
pub struct Series {
    /// Time in years (e.g. 2015, 2016, … or 2020.0, 2020.083, … for months).
    pub time: Vec<f64>,
    pub counts: Vec<f64>,
    /// Population or person-time; without it the counts themselves are modeled.
    pub population: Option<Vec<f64>>,
}

/// Options.
#[derive(Debug, Clone)]
pub struct Options {
    pub model: ModelChoice,
    /// Add an annual harmonic (for monthly or weekly data).
    pub seasonal: bool,
    /// Maximum joinpoints to consider (0 disables the search).
    pub max_joinpoints: usize,
    /// Minimum observations in each segment, counting shared joinpoints.
    pub min_segment_points: usize,
    /// Known interruption times (interrupted time series), in years.
    pub interruptions: Vec<f64>,
    pub confidence: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            model: ModelChoice::Auto,
            seasonal: false,
            max_joinpoints: 3,
            min_segment_points: 3,
            interruptions: Vec::new(),
            confidence: 0.95,
        }
    }
}

/// A percent change with its interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PercentChange {
    /// Percent change per year.
    pub percent: f64,
    pub lower: f64,
    pub upper: f64,
    pub p_value: f64,
}

/// One segment of a joinpoint model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub apc: PercentChange,
}

/// The selected joinpoint model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JoinpointModel {
    /// Joinpoint locations (times), empty for a straight line.
    pub joinpoints: Vec<f64>,
    pub segments: Vec<Segment>,
    /// Average annual percent change over the whole period.
    pub aapc: PercentChange,
    /// BIC of the best model for each number of joinpoints tried.
    pub bic_by_joinpoints: Vec<f64>,
}

/// Interrupted time series estimates for one interruption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Interruption {
    pub at: f64,
    /// Immediate change in level as a rate ratio (after ÷ expected without change).
    pub level_ratio: f64,
    pub level_lower: f64,
    pub level_upper: f64,
    pub level_p: f64,
    /// Percent change per year before and after.
    pub apc_before: PercentChange,
    pub apc_after: PercentChange,
    /// p-value for the change in slope.
    pub slope_change_p: f64,
}

/// Mann–Kendall test and Sen's slope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MannKendall {
    pub s: f64,
    pub variance: f64,
    pub z: f64,
    pub p_value: f64,
    /// Kendall's tau (tau-b with ties).
    pub tau: f64,
    /// Median of pairwise slopes, in the series' units per year.
    pub sens_slope: f64,
}

/// Seasonality test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Seasonality {
    /// Likelihood-ratio χ² (2 df) for the annual harmonic.
    pub chi_square: f64,
    pub p_value: f64,
    /// Peak-to-trough ratio implied by the harmonic.
    pub peak_to_trough: f64,
    /// Fraction of the year (0–1) at which the fitted peak occurs.
    pub peak_fraction_of_year: f64,
}

/// Everything returned by [`analyze`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrendResult {
    pub model: Family,
    /// Pearson χ² ÷ df of the Poisson model.
    pub poisson_dispersion: f64,
    /// Percent change per year from the single log-linear model.
    pub apc: PercentChange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joinpoint: Option<JoinpointModel>,
    pub interruptions: Vec<Interruption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seasonality: Option<Seasonality>,
    pub mann_kendall: MannKendall,
    /// Durbin–Watson statistic on Pearson residuals of the main model (≈2 means no autocorrelation).
    pub durbin_watson: f64,
    /// Fitted rates (or counts) from the selected model, per unit of population.
    #[schemars(skip)]
    #[serde(skip)]
    pub fitted: Vec<f64>,
    pub warnings: Vec<String>,
}

fn pc(est: f64, lo: f64, hi: f64, p: f64) -> PercentChange {
    PercentChange {
        percent: 100.0 * (est.exp() - 1.0),
        lower: 100.0 * (lo.exp() - 1.0),
        upper: 100.0 * (hi.exp() - 1.0),
        p_value: p,
    }
}

fn wald_p(est: f64, lo: f64, hi: f64, confidence: f64) -> f64 {
    let se = (hi - lo) / (2.0 * crate::dist::z_two_sided(confidence));
    if se > 0.0 {
        p_normal_two_sided(est / se)
    } else {
        f64::NAN
    }
}

struct Design {
    names: Vec<String>,
    rows: Vec<Vec<f64>>,
}

fn design(s: &Series, t0: f64, hinges: &[f64], interruptions: &[f64], seasonal: bool) -> Design {
    let mut names = vec!["(Intercept)".to_string(), "time".to_string()];
    for h in hinges {
        names.push(format!("hinge_{h}"));
    }
    for t in interruptions {
        names.push(format!("level_{t}"));
        names.push(format!("slope_{t}"));
    }
    if seasonal {
        names.push("sin_annual".into());
        names.push("cos_annual".into());
    }
    let rows = s
        .time
        .iter()
        .map(|&t| {
            let mut r = vec![1.0, t - t0];
            for h in hinges {
                r.push((t - h).max(0.0));
            }
            for &it in interruptions {
                let on = if t >= it { 1.0 } else { 0.0 };
                r.push(on);
                r.push(on * (t - it));
            }
            if seasonal {
                let a = 2.0 * std::f64::consts::PI * t;
                r.push(a.sin());
                r.push(a.cos());
            }
            r
        })
        .collect();
    Design { names, rows }
}

fn fit_design(s: &Series, d: Design, family: Family, conf: f64) -> Result<GlmFit, String> {
    let data = GlmData {
        y: s.counts.clone(),
        x: d.rows,
        names: d.names,
        offset: s
            .population
            .as_ref()
            .map(|p| p.iter().map(|v| v.ln()).collect()),
        trials: None,
    };
    fit(&data, family, conf)
}

/// Run every analysis.
pub fn analyze(s: &Series, o: &Options) -> Result<TrendResult, String> {
    let n = s.time.len();
    if n < 3 || s.counts.len() != n || s.population.as_ref().is_some_and(|p| p.len() != n) {
        return Err(
            "a trend needs at least 3 time points, each with a count (and population)".into(),
        );
    }
    if s.time.windows(2).any(|w| w[1] <= w[0]) {
        return Err("time points must be in increasing order without duplicates".into());
    }
    if s.population
        .as_ref()
        .is_some_and(|p| p.iter().any(|v| *v <= 0.0))
    {
        return Err("every population must be greater than zero".into());
    }
    let conf = o.confidence;
    let t0 = s.time[0];
    let mut warnings = Vec::new();

    // Main log-linear model and the family decision.
    let pois = fit_design(
        s,
        design(s, t0, &[], &[], o.seasonal),
        Family::Poisson,
        conf,
    )?;
    let dispersion = pois.overdispersion();
    let family = match o.model {
        ModelChoice::Poisson => Family::Poisson,
        ModelChoice::NegativeBinomial => Family::NegativeBinomial,
        ModelChoice::Auto if dispersion > 1.5 && pois.df_residual >= 3.0 => {
            Family::NegativeBinomial
        }
        ModelChoice::Auto => Family::Poisson,
    };
    let fit_fam = |d: Design| -> Result<GlmFit, String> {
        match fit_design(s, d, family, conf) {
            Ok(f) => Ok(f),
            Err(e) if family == Family::NegativeBinomial => Err(format!(
                "the negative binomial model could not be fitted ({e}); try model = poisson"
            )),
            Err(e) => Err(e),
        }
    };
    let main = if family == Family::Poisson {
        pois.clone()
    } else {
        fit_fam(design(s, t0, &[], &[], o.seasonal))?
    };
    if family == Family::Poisson && dispersion > 1.5 {
        warnings.push(format!(
            "The counts vary more than a Poisson model expects (dispersion {dispersion:.2}); \
             intervals may be too narrow. Consider model = negative_binomial."
        ));
    }
    let (b, lo, hi) = main.linear_combination(&unit(main.coefficients.len(), 1));
    let apc = pc(b, lo, hi, main.coefficients[1].p_value);

    // Seasonality (likelihood-ratio test of the harmonic).
    // Tested within the model that also holds any interruption terms.
    let mut its_sorted = o.interruptions.clone();
    its_sorted.sort_by(f64::total_cmp);
    let seasonality = if o.seasonal {
        let with = if its_sorted.is_empty() {
            main.clone()
        } else {
            fit_fam(design(s, t0, &[], &its_sorted, true))?
        };
        let without = fit_fam(design(s, t0, &[], &its_sorted, false))?;
        let chi = (2.0 * (with.log_likelihood - without.log_likelihood)).max(0.0);
        let k = with.coefficients.len();
        let (bs, bc) = (
            with.coefficients[k - 2].estimate,
            with.coefficients[k - 1].estimate,
        );
        let amp = (bs * bs + bc * bc).sqrt();
        // sin(2πt)·bs + cos(2πt)·bc = amp·cos(2πt − φ) with φ = atan2(bs, bc).
        let phase = bs.atan2(bc);
        Some(Seasonality {
            chi_square: chi,
            p_value: p_chi_square(chi, 2.0),
            peak_to_trough: (2.0 * amp).exp(),
            peak_fraction_of_year: (phase / (2.0 * std::f64::consts::PI)).rem_euclid(1.0),
        })
    } else {
        None
    };

    // Joinpoint search.
    let joinpoint = if o.max_joinpoints > 0 && o.interruptions.is_empty() {
        Some(joinpoint_search(s, o, family, t0)?)
    } else {
        None
    };

    // Interrupted time series.
    let mut interruptions = Vec::new();
    if !o.interruptions.is_empty() {
        let mut its = o.interruptions.clone();
        its.sort_by(f64::total_cmp);
        for it in &its {
            let before = s.time.iter().filter(|t| **t < it - 1e-9).count();
            let after = n - before;
            if before < 3 || after < 3 {
                return Err(format!(
                    "an interruption at {it} needs at least 3 time points before and after it"
                ));
            }
        }
        let f = fit_fam(design(s, t0, &[], &its, o.seasonal))?;
        let p = f.coefficients.len();
        for (k, it) in its.iter().enumerate() {
            let li = 2 + 2 * k;
            let lvl = &f.coefficients[li];
            // Slope before this interruption = time + earlier slope changes.
            let mut cb = unit(p, 1);
            for j in 0..k {
                cb[3 + 2 * j] = 1.0;
            }
            let mut ca = cb.clone();
            ca[li + 1] = 1.0;
            let (eb, lb, hb) = f.linear_combination(&cb);
            let (ea, la, ha) = f.linear_combination(&ca);
            interruptions.push(Interruption {
                at: *it,
                level_ratio: lvl.estimate.exp(),
                level_lower: lvl.lower.exp(),
                level_upper: lvl.upper.exp(),
                level_p: lvl.p_value,
                apc_before: pc(eb, lb, hb, wald_p(eb, lb, hb, conf)),
                apc_after: pc(ea, la, ha, wald_p(ea, la, ha, conf)),
                slope_change_p: f.coefficients[li + 1].p_value,
            });
        }
    }

    // Diagnostics on the main model.
    let rate: Vec<f64> = match &s.population {
        Some(p) => s.counts.iter().zip(p).map(|(c, p)| c / p).collect(),
        None => s.counts.clone(),
    };
    let mann_kendall = mann_kendall(&s.time, &rate);
    let resid: Vec<f64> = s
        .counts
        .iter()
        .zip(&main.fitted)
        .map(|(y, m)| {
            let v = match family {
                Family::NegativeBinomial => m + m * m / main.theta.unwrap_or(f64::INFINITY),
                _ => *m,
            };
            (y - m) / v.sqrt()
        })
        .collect();
    let num: f64 = resid.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
    let den: f64 = resid.iter().map(|r| r * r).sum();
    let durbin_watson = if den > 0.0 { num / den } else { f64::NAN };
    if durbin_watson < 1.5 || durbin_watson > 2.5 {
        warnings.push(format!(
            "Successive residuals are correlated (Durbin–Watson {durbin_watson:.2}; about 2 means \
             none), so p-values and intervals may be too optimistic."
        ));
    }
    if s.counts.iter().sum::<f64>() < 20.0 {
        warnings.push("Fewer than 20 events in total: trend estimates are very imprecise.".into());
    }
    let fitted = match (&joinpoint, &s.population) {
        _ if !interruptions.is_empty() => {
            let f = fit_fam(design(s, t0, &[], &o.interruptions, o.seasonal))?;
            scale(&f.fitted, s.population.as_deref())
        }
        (Some(jp), _) => {
            let f = fit_fam(design(s, t0, &jp.joinpoints, &[], o.seasonal))?;
            scale(&f.fitted, s.population.as_deref())
        }
        _ => scale(&main.fitted, s.population.as_deref()),
    };
    Ok(TrendResult {
        model: family,
        poisson_dispersion: dispersion,
        apc,
        joinpoint,
        interruptions,
        seasonality,
        mann_kendall,
        durbin_watson,
        fitted,
        warnings,
    })
}

fn scale(fitted: &[f64], pop: Option<&[f64]>) -> Vec<f64> {
    match pop {
        Some(p) => fitted.iter().zip(p).map(|(f, p)| f / p).collect(),
        None => fitted.to_vec(),
    }
}

fn unit(p: usize, i: usize) -> Vec<f64> {
    let mut c = vec![0.0; p];
    c[i] = 1.0;
    c
}

/// Recommended maximum joinpoints for n points (as NCI Joinpoint's guidance).
pub fn recommended_max_joinpoints(n: usize) -> usize {
    n.saturating_sub(2) / 5
}

fn joinpoint_search(
    s: &Series,
    o: &Options,
    family: Family,
    t0: f64,
) -> Result<JoinpointModel, String> {
    let n = s.time.len();
    let max_k = o.max_joinpoints.min(recommended_max_joinpoints(n)).min(3);
    let min_pts = o.min_segment_points.max(2);
    // Candidate joinpoints: interior observed times, thinned to at most 40.
    let mut cand: Vec<usize> = ((min_pts - 1)..n.saturating_sub(min_pts - 1)).collect();
    if cand.len() > 40 {
        let step = cand.len() as f64 / 40.0;
        cand = (0..40).map(|i| cand[(i as f64 * step) as usize]).collect();
    }
    let extra = if family == Family::NegativeBinomial {
        1.0
    } else {
        0.0
    } + if o.seasonal { 2.0 } else { 0.0 };
    let bic = |f: &GlmFit, k: usize| {
        -2.0 * f.log_likelihood + (2.0 + 2.0 * k as f64 + extra) * (n as f64).ln()
    };

    let mut best: Vec<(f64, Vec<usize>)> = Vec::new();
    let base = fit_design(s, design(s, t0, &[], &[], o.seasonal), family, o.confidence)?;
    best.push((bic(&base, 0), vec![]));
    for k in 1..=max_k {
        let mut best_k: Option<(f64, Vec<usize>)> = None;
        let mut combo = Vec::with_capacity(k);
        let mut visit = |c: &[usize]| {
            let hinges: Vec<f64> = c.iter().map(|i| s.time[*i]).collect();
            if let Ok(f) = fit_design(
                s,
                design(s, t0, &hinges, &[], o.seasonal),
                family,
                o.confidence,
            ) {
                let b = bic(&f, k);
                if best_k.as_ref().is_none_or(|(bb, _)| b < *bb) {
                    best_k = Some((b, c.to_vec()));
                }
            }
        };
        combinations(&cand, k, min_pts - 1, 0, &mut combo, &mut visit);
        match best_k {
            Some(b) => best.push(b),
            None => break,
        }
    }
    let (chosen_k, (_, idx)) = best
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.0.total_cmp(&b.1.0))
        .map(|(k, b)| (k, b.clone()))
        .expect("at least the straight line");
    let hinges: Vec<f64> = idx.iter().map(|i| s.time[*i]).collect();
    let f = fit_design(
        s,
        design(s, t0, &hinges, &[], o.seasonal),
        family,
        o.confidence,
    )?;
    let p = f.coefficients.len();
    let mut bounds = vec![s.time[0]];
    bounds.extend(hinges.iter().copied());
    bounds.push(s.time[n - 1]);
    let mut segments = Vec::new();
    let mut avg = vec![0.0; p];
    let total = s.time[n - 1] - s.time[0];
    for j in 0..=chosen_k {
        let mut c = unit(p, 1);
        for h in 0..j {
            c[2 + h] = 1.0;
        }
        let (e, l, u) = f.linear_combination(&c);
        let w = (bounds[j + 1] - bounds[j]) / total;
        for (a, ci) in avg.iter_mut().zip(&c) {
            *a += w * ci;
        }
        segments.push(Segment {
            start: bounds[j],
            end: bounds[j + 1],
            apc: pc(e, l, u, wald_p(e, l, u, o.confidence)),
        });
    }
    let (e, l, u) = f.linear_combination(&avg);
    Ok(JoinpointModel {
        joinpoints: hinges,
        segments,
        aapc: pc(e, l, u, wald_p(e, l, u, o.confidence)),
        bic_by_joinpoints: best.iter().map(|b| b.0).collect(),
    })
}

/// Visit every increasing k-combination of `cand` whose consecutive members
/// are at least `gap` index positions apart.
fn combinations(
    cand: &[usize],
    k: usize,
    gap: usize,
    from: usize,
    cur: &mut Vec<usize>,
    visit: &mut impl FnMut(&[usize]),
) {
    if cur.len() == k {
        visit(cur);
        return;
    }
    for i in from..cand.len() {
        if let Some(last) = cur.last()
            && cand[i] < last + gap.max(1)
        {
            continue;
        }
        cur.push(cand[i]);
        combinations(cand, k, gap, i + 1, cur, visit);
        cur.pop();
    }
}

/// Mann–Kendall test (normal approximation with tie correction and continuity
/// correction, as R `trend::mk.test`) and Sen's slope.
pub fn mann_kendall(t: &[f64], x: &[f64]) -> MannKendall {
    let n = x.len();
    let mut s = 0.0;
    let mut slopes = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in i + 1..n {
            s += (x[j] - x[i]).signum() * f64::from(u8::from(x[j] != x[i]));
            slopes.push((x[j] - x[i]) / (t[j] - t[i]));
        }
    }
    // Tie groups.
    let mut sorted = x.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut ties = Vec::new();
    let mut run = 1usize;
    for w in sorted.windows(2) {
        if w[0] == w[1] {
            run += 1;
        } else {
            if run > 1 {
                ties.push(run as f64);
            }
            run = 1;
        }
    }
    if run > 1 {
        ties.push(run as f64);
    }
    let nf = n as f64;
    let var = (nf * (nf - 1.0) * (2.0 * nf + 5.0)
        - ties
            .iter()
            .map(|t| t * (t - 1.0) * (2.0 * t + 5.0))
            .sum::<f64>())
        / 18.0;
    let z = if s > 0.0 {
        (s - 1.0) / var.sqrt()
    } else if s < 0.0 {
        (s + 1.0) / var.sqrt()
    } else {
        0.0
    };
    let n0 = nf * (nf - 1.0) / 2.0;
    let tie_pairs: f64 = ties.iter().map(|t| t * (t - 1.0) / 2.0).sum();
    let tau = s / ((n0 - tie_pairs) * n0).sqrt();
    slopes.sort_by(f64::total_cmp);
    let m = slopes.len();
    let sens = if m == 0 {
        f64::NAN
    } else if m % 2 == 1 {
        slopes[m / 2]
    } else {
        0.5 * (slopes[m / 2 - 1] + slopes[m / 2])
    };
    MannKendall {
        s,
        variance: var,
        z,
        p_value: if var > 0.0 {
            p_normal_two_sided(z)
        } else {
            1.0
        },
        tau,
        sens_slope: sens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series_with_joinpoint() -> Series {
        // Rate rises 8%/year to 2014, then falls 5%/year (noise-free).
        let time: Vec<f64> = (2005..=2024).map(f64::from).collect();
        let pop = vec![100_000.0; time.len()];
        let counts = time
            .iter()
            .map(|t| {
                let lr = if *t <= 2014.0 {
                    (50e-5f64).ln() + (1.08f64).ln() * (t - 2005.0)
                } else {
                    (50e-5f64).ln() + (1.08f64).ln() * 9.0 + (0.95f64).ln() * (t - 2014.0)
                };
                (lr.exp() * 100_000.0).round()
            })
            .collect();
        Series {
            time,
            counts,
            population: Some(pop),
        }
    }

    #[test]
    fn finds_the_joinpoint_and_segment_apcs() {
        let s = series_with_joinpoint();
        let r = analyze(
            &s,
            &Options {
                model: ModelChoice::Poisson,
                ..Options::default()
            },
        )
        .unwrap();
        let jp = r.joinpoint.unwrap();
        assert_eq!(jp.joinpoints, vec![2014.0]);
        assert!(
            (jp.segments[0].apc.percent - 8.0).abs() < 0.5,
            "{:?}",
            jp.segments[0]
        );
        assert!(
            (jp.segments[1].apc.percent + 5.0).abs() < 0.8,
            "{:?}",
            jp.segments[1]
        );
        assert!(jp.aapc.percent > 0.0 && jp.aapc.percent < 8.0);
    }

    #[test]
    fn interrupted_series_level_change() {
        let time: Vec<f64> = (0..24).map(|m| 2020.0 + m as f64 / 12.0).collect();
        let counts: Vec<f64> = (0..24).map(|m| if m < 12 { 100.0 } else { 70.0 }).collect();
        let s = Series {
            time: time.clone(),
            counts,
            population: None,
        };
        let r = analyze(
            &s,
            &Options {
                model: ModelChoice::Poisson,
                interruptions: vec![time[12]],
                max_joinpoints: 0,
                ..Options::default()
            },
        )
        .unwrap();
        let it = &r.interruptions[0];
        assert!((it.level_ratio - 0.7).abs() < 1e-6, "{}", it.level_ratio);
        assert!(it.level_p < 0.001);
    }

    #[test]
    fn mann_kendall_counts_pairs() {
        let mk = mann_kendall(&[1.0, 2.0, 3.0, 4.0, 5.0], &[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(mk.s, 10.0);
        assert!((mk.tau - 1.0).abs() < 1e-12);
        assert!((mk.sens_slope - 1.0).abs() < 1e-12);
        assert_eq!(recommended_max_joinpoints(12), 2);
    }
}
