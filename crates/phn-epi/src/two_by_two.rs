//! 2×2 tables: measures of association, exact and asymptotic tests, and
//! Mantel–Haenszel stratified analysis.
//!
//! Layout (rows = exposure, columns = outcome):
//!
//! ```text
//!               Cases (ill)   Non-cases (well)   Total
//! Exposed            a               b            n1 = a + b
//! Unexposed          c               d            n0 = c + d
//! Total          m1 = a + c      m0 = b + d          N
//! ```
//!
//! Methods (each verified against R in `tests/golden.rs`):
//! * risk ratio with the Katz log-scale (Wald) interval; risk difference
//!   with the Wald interval; odds ratio with the Woolf log-scale interval;
//! * conditional maximum-likelihood odds ratio with Fisher's exact and
//!   mid-p exact intervals (as `fisher.test` and `epitools::oddsratio.midp`);
//! * Pearson chi-square (uncorrected), Yates-corrected chi-square (R's
//!   `chisq.test` form), Mantel–Haenszel chi-square; Fisher's exact test
//!   (two-sided by R's "probabilities no larger than observed" rule, and
//!   one-sided) and the two-sided mid-p;
//! * attributable fraction in the exposed and population attributable
//!   fraction (Levin's formula for cohort and cross-sectional designs,
//!   Miettinen's case-based formula for case-control designs);
//! * Mantel–Haenszel summary odds ratio (Robins–Breslow–Greenland interval),
//!   summary risk ratio (Greenland–Robins interval), the MH test of no
//!   association (continuity-corrected, as R `mantelhaen.test`), and the
//!   Breslow–Day test of homogeneity with Tarone's correction.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dist::{bisect, ln_choose, p_chi_square, z_two_sided};

/// Study design: decides which measures are meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum Design {
    /// Cohort or outbreak attack-rate table: risks, risk ratio, risk difference.
    #[default]
    Cohort,
    /// Case-control: odds ratio only (risks are not estimable).
    CaseControl,
    /// Cross-sectional survey: prevalence ratio and prevalence odds ratio.
    CrossSectional,
}

/// A 2×2 table of counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Table {
    /// Exposed cases.
    pub a: u64,
    /// Exposed non-cases.
    pub b: u64,
    /// Unexposed cases.
    pub c: u64,
    /// Unexposed non-cases.
    pub d: u64,
}

impl Table {
    pub fn new(a: u64, b: u64, c: u64, d: u64) -> Self {
        Self { a, b, c, d }
    }
    pub fn n1(&self) -> u64 {
        self.a + self.b
    }
    pub fn n0(&self) -> u64 {
        self.c + self.d
    }
    pub fn m1(&self) -> u64 {
        self.a + self.c
    }
    pub fn m0(&self) -> u64 {
        self.b + self.d
    }
    pub fn total(&self) -> u64 {
        self.a + self.b + self.c + self.d
    }
    fn f(&self) -> (f64, f64, f64, f64) {
        (self.a as f64, self.b as f64, self.c as f64, self.d as f64)
    }
    /// Expected counts under independence, in a, b, c, d order.
    pub fn expected(&self) -> [f64; 4] {
        let n = self.total() as f64;
        let (n1, n0, m1, m0) = (
            self.n1() as f64,
            self.n0() as f64,
            self.m1() as f64,
            self.m0() as f64,
        );
        [n1 * m1 / n, n1 * m0 / n, n0 * m1 / n, n0 * m0 / n]
    }
}

/// A point estimate with a confidence interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Estimate {
    pub value: f64,
    /// Lower confidence limit (may be 0).
    pub lower: f64,
    /// Upper confidence limit (`null` when infinite).
    pub upper: Option<f64>,
    /// How the interval was computed.
    pub method: String,
}

impl Estimate {
    fn new(value: f64, lower: f64, upper: f64, method: &str) -> Self {
        Self {
            value,
            lower,
            upper: upper.is_finite().then_some(upper),
            method: method.into(),
        }
    }
}

/// A test statistic and its p-value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TestResult {
    pub name: String,
    /// Statistic (chi-square), when the test has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statistic: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df: Option<f64>,
    pub p_value: f64,
}

/// Everything computed for one 2×2 table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TwoByTwo {
    pub table: Table,
    pub design: Design,
    pub confidence: f64,
    /// Risk (attack rate) among the exposed, a / (a + b). Cohort and cross-sectional only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_exposed: Option<f64>,
    /// Risk among the unexposed, c / (c + d).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_unexposed: Option<f64>,
    /// Risk (prevalence) ratio with the Katz log interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_ratio: Option<Estimate>,
    /// Risk difference (exposed − unexposed) with the Wald interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_difference: Option<Estimate>,
    /// Sample odds ratio (ad/bc) with the Woolf log interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub odds_ratio: Option<Estimate>,
    /// Conditional maximum-likelihood odds ratio with Fisher's exact interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub odds_ratio_exact: Option<Estimate>,
    /// Conditional MLE odds ratio with the mid-p exact interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub odds_ratio_mid_p: Option<Estimate>,
    /// Attributable fraction among the exposed, (RR − 1)/RR (OR for case-control).
    /// Reported as the preventable fraction 1 − RR when the measure is below 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributable_fraction_exposed: Option<f64>,
    /// True when the exposure appears protective and the fraction above is preventable.
    pub protective: bool,
    /// Population attributable fraction (Levin, or Miettinen for case-control).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub population_attributable_fraction: Option<f64>,
    /// Pearson, Yates-corrected, and Mantel–Haenszel chi-square tests.
    pub chi_square: Vec<TestResult>,
    /// Fisher's exact test (two-sided and one-sided) and the two-sided mid-p.
    pub exact: Vec<TestResult>,
    /// Smallest expected cell count under independence.
    pub min_expected: f64,
    /// Plain-language warnings (zero cells, small expected counts).
    pub warnings: Vec<String>,
}

/// Analyze one 2×2 table.
///
/// # Panics
/// Panics if `confidence` is not in (0, 1).
pub fn analyze(t: Table, design: Design, confidence: f64) -> TwoByTwo {
    assert!(confidence > 0.0 && confidence < 1.0);
    let zc = z_two_sided(confidence);
    let (a, b, c, d) = t.f();
    let (n1, n0, m1, n) = (
        t.n1() as f64,
        t.n0() as f64,
        t.m1() as f64,
        t.total() as f64,
    );
    let mut warnings = Vec::new();
    let risks_ok = design != Design::CaseControl && t.n1() > 0 && t.n0() > 0;
    let (risk_exposed, risk_unexposed) = if risks_ok {
        (Some(a / n1), Some(c / n0))
    } else {
        (None, None)
    };

    let mut risk_ratio = None;
    let mut risk_difference = None;
    if risks_ok {
        let (r1, r0) = (a / n1, c / n0);
        if t.a > 0 && t.c > 0 {
            let rr = r1 / r0;
            let se = (1.0 / a - 1.0 / n1 + 1.0 / c - 1.0 / n0).sqrt();
            risk_ratio = Some(Estimate::new(
                rr,
                (rr.ln() - zc * se).exp(),
                (rr.ln() + zc * se).exp(),
                "Katz log-scale (Wald) interval",
            ));
        } else {
            warnings.push(
                "A zero count among exposed or unexposed cases makes the risk ratio 0 or \
                 undefined; use the exact odds ratio interval, or describe the attack rates."
                    .into(),
            );
        }
        let rd = r1 - r0;
        let se = (r1 * (1.0 - r1) / n1 + r0 * (1.0 - r0) / n0).sqrt();
        risk_difference = Some(Estimate::new(
            rd,
            rd - zc * se,
            rd + zc * se,
            "Wald interval",
        ));
    }

    let odds_ratio = if t.a > 0 && t.b > 0 && t.c > 0 && t.d > 0 {
        let or = (a * d) / (b * c);
        let se = (1.0 / a + 1.0 / b + 1.0 / c + 1.0 / d).sqrt();
        Some(Estimate::new(
            or,
            (or.ln() - zc * se).exp(),
            (or.ln() + zc * se).exp(),
            "Woolf log-scale (Wald) interval",
        ))
    } else {
        warnings.push(
            "A zero cell makes the sample odds ratio and its Woolf interval undefined; the \
             exact (conditional MLE) odds ratio and interval are reported instead."
                .into(),
        );
        None
    };

    let hyper = Hyper::new(&t);
    let alpha = 1.0 - confidence;
    let (exact_or, mid_or) = match &hyper {
        Some(h) => {
            let mle = h.mle(t.a);
            let lo = h.lower_limit(t.a, alpha / 2.0, false);
            let hi = h.upper_limit(t.a, alpha / 2.0, false);
            let lo_m = h.lower_limit(t.a, alpha / 2.0, true);
            let hi_m = h.upper_limit(t.a, alpha / 2.0, true);
            (
                Some(Estimate::new(
                    mle,
                    lo,
                    hi,
                    "conditional maximum likelihood; Fisher exact interval",
                )),
                Some(Estimate::new(
                    mle,
                    lo_m,
                    hi_m,
                    "conditional maximum likelihood; mid-p exact interval",
                )),
            )
        }
        None => (None, None),
    };

    // Attributable fractions.
    let measure = match design {
        Design::CaseControl => odds_ratio
            .as_ref()
            .map(|e| e.value)
            .or(exact_or.as_ref().map(|e| e.value)),
        _ => risk_ratio.as_ref().map(|e| e.value),
    };
    let mut protective = false;
    let mut af = None;
    let mut paf = None;
    if let Some(m) = measure.filter(|m| m.is_finite() && *m > 0.0) {
        if m >= 1.0 {
            af = Some((m - 1.0) / m);
            paf = Some(match design {
                Design::CaseControl => {
                    // Miettinen: proportion of cases exposed × AF in the exposed.
                    (a / m1) * (m - 1.0) / m
                }
                _ => {
                    let pe = n1 / n;
                    pe * (m - 1.0) / (1.0 + pe * (m - 1.0))
                }
            });
        } else {
            protective = true;
            af = Some(1.0 - m);
        }
    }

    // Chi-square tests.
    let e = t.expected();
    let min_expected = e.iter().copied().fold(f64::INFINITY, f64::min);
    let obs = [a, b, c, d];
    let mut chi_square = Vec::new();
    if e.iter().all(|x| *x > 0.0) {
        let pearson: f64 = obs
            .iter()
            .zip(e.iter())
            .map(|(o, e)| (o - e).powi(2) / e)
            .sum();
        let yates_k = obs
            .iter()
            .zip(e.iter())
            .map(|(o, e)| (o - e).abs())
            .fold(f64::INFINITY, f64::min)
            .min(0.5);
        let yates: f64 = obs
            .iter()
            .zip(e.iter())
            .map(|(o, e)| ((o - e).abs() - yates_k).powi(2) / e)
            .sum();
        let mh = pearson * (n - 1.0) / n;
        for (name, s) in [
            ("Pearson chi-square (uncorrected)", pearson),
            ("Yates continuity-corrected chi-square", yates),
            ("Mantel–Haenszel chi-square", mh),
        ] {
            chi_square.push(TestResult {
                name: name.into(),
                statistic: Some(s),
                df: Some(1.0),
                p_value: p_chi_square(s, 1.0),
            });
        }
    }
    if min_expected < 5.0 {
        warnings.push(format!(
            "The smallest expected count is {min_expected:.1} (below 5), so chi-square p-values \
             are unreliable; use Fisher's exact test."
        ));
    }

    let mut exact = Vec::new();
    if let Some(h) = &hyper {
        let (two, less, greater, mid) = h.fisher_p(t.a);
        exact.push(TestResult {
            name: "Fisher exact, two-sided".into(),
            statistic: None,
            df: None,
            p_value: two,
        });
        exact.push(TestResult {
            name: "Fisher exact, one-sided (exposure increases risk)".into(),
            statistic: None,
            df: None,
            p_value: greater,
        });
        exact.push(TestResult {
            name: "Fisher exact, one-sided (exposure decreases risk)".into(),
            statistic: None,
            df: None,
            p_value: less,
        });
        exact.push(TestResult {
            name: "Mid-p exact, two-sided".into(),
            statistic: None,
            df: None,
            p_value: mid,
        });
    }
    if t.total() == 0 || t.n1() == 0 || t.n0() == 0 || t.m1() == 0 || t.m0() == 0 {
        warnings.push(
            "A whole row or column is zero, so no measure of association can be estimated.".into(),
        );
    }

    TwoByTwo {
        table: t,
        design,
        confidence,
        risk_exposed,
        risk_unexposed,
        risk_ratio,
        risk_difference,
        odds_ratio,
        odds_ratio_exact: exact_or,
        odds_ratio_mid_p: mid_or,
        attributable_fraction_exposed: af,
        protective,
        population_attributable_fraction: paf,
        chi_square,
        exact,
        min_expected,
        warnings,
    }
}

/// The (noncentral) hypergeometric distribution of `a` given the margins.
struct Hyper {
    lo: u64,
    hi: u64,
    /// ln of the central probabilities' numerators, ln C(m1,x) + ln C(m0,n1−x).
    ln_w: Vec<f64>,
}

impl Hyper {
    fn new(t: &Table) -> Option<Self> {
        let (m1, m0, n1) = (t.m1(), t.m0(), t.n1());
        if t.total() == 0 || n1 == 0 || t.n0() == 0 || m1 == 0 || m0 == 0 {
            return None;
        }
        let lo = n1.saturating_sub(m0);
        let hi = n1.min(m1);
        let ln_w = (lo..=hi)
            .map(|x| ln_choose(m1, x) + ln_choose(m0, n1 - x))
            .collect();
        Some(Self { lo, hi, ln_w })
    }

    /// Probabilities for each support point at odds ratio exp(log_psi).
    fn probs(&self, log_psi: f64) -> Vec<f64> {
        let logs: Vec<f64> = self
            .ln_w
            .iter()
            .enumerate()
            .map(|(i, w)| w + (self.lo + i as u64) as f64 * log_psi)
            .collect();
        let max = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let ex: Vec<f64> = logs.iter().map(|l| (l - max).exp()).collect();
        let s: f64 = ex.iter().sum();
        ex.into_iter().map(|e| e / s).collect()
    }

    fn mean(&self, log_psi: f64) -> f64 {
        self.probs(log_psi)
            .iter()
            .enumerate()
            .map(|(i, p)| (self.lo + i as u64) as f64 * p)
            .sum()
    }

    /// P(X <= x) (or with mid-p, P(X < x) + P(x)/2).
    fn cdf(&self, x: u64, log_psi: f64, mid: bool) -> f64 {
        let p = self.probs(log_psi);
        let k = (x - self.lo) as usize;
        let below: f64 = p[..k].iter().sum();
        if mid {
            below + 0.5 * p[k]
        } else {
            below + p[k]
        }
    }

    /// P(X >= x) (or with mid-p, P(X > x) + P(x)/2).
    fn sf(&self, x: u64, log_psi: f64, mid: bool) -> f64 {
        let p = self.probs(log_psi);
        let k = (x - self.lo) as usize;
        let above: f64 = p[k + 1..].iter().sum();
        if mid {
            above + 0.5 * p[k]
        } else {
            above + p[k]
        }
    }

    fn mle(&self, x: u64) -> f64 {
        if x == self.lo {
            return 0.0;
        }
        if x == self.hi {
            return f64::INFINITY;
        }
        bisect(-60.0, 60.0, |lp| self.mean(lp) - x as f64)
            .map(f64::exp)
            .unwrap_or(f64::NAN)
    }

    fn lower_limit(&self, x: u64, alpha: f64, mid: bool) -> f64 {
        if x == self.lo {
            return 0.0;
        }
        bisect(-60.0, 60.0, |lp| self.sf(x, lp, mid) - alpha)
            .map(f64::exp)
            .unwrap_or(0.0)
    }

    fn upper_limit(&self, x: u64, alpha: f64, mid: bool) -> f64 {
        if x == self.hi {
            return f64::INFINITY;
        }
        bisect(-60.0, 60.0, |lp| self.cdf(x, lp, mid) - alpha)
            .map(f64::exp)
            .unwrap_or(f64::INFINITY)
    }

    /// (two-sided, less, greater, two-sided mid-p) at odds ratio 1.
    fn fisher_p(&self, x: u64) -> (f64, f64, f64, f64) {
        let p = self.probs(0.0);
        let k = (x - self.lo) as usize;
        let rel = 1.0 + 1e-7;
        let two: f64 = p.iter().filter(|q| **q <= p[k] * rel).sum();
        let less: f64 = p[..=k].iter().sum();
        let greater: f64 = p[k..].iter().sum();
        let mid_less = less - 0.5 * p[k];
        let mid_greater = greater - 0.5 * p[k];
        let mid = (2.0 * mid_less.min(mid_greater)).min(1.0);
        (two.min(1.0), less.min(1.0), greater.min(1.0), mid)
    }
}

// ---------------------------------------------------------------- stratified

/// Mantel–Haenszel analysis across strata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stratified {
    pub strata: usize,
    pub confidence: f64,
    /// Crude (collapsed) odds ratio, Woolf interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crude_odds_ratio: Option<Estimate>,
    /// Crude risk ratio (cohort designs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crude_risk_ratio: Option<Estimate>,
    /// MH summary odds ratio, Robins–Breslow–Greenland interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mh_odds_ratio: Option<Estimate>,
    /// MH summary risk ratio, Greenland–Robins interval (cohort designs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mh_risk_ratio: Option<Estimate>,
    /// MH test of no association (continuity-corrected, 1 df).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mh_test: Option<TestResult>,
    /// Breslow–Day test of homogeneity of odds ratios, with Tarone's correction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breslow_day: Option<TestResult>,
    /// Percent change from the crude to the MH odds ratio (a common
    /// confounding screen: ≥10% suggests confounding by the stratifier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crude_vs_adjusted_change_pct: Option<f64>,
    pub warnings: Vec<String>,
}

/// Mantel–Haenszel summary measures across strata of 2×2 tables.
pub fn mantel_haenszel(strata: &[Table], design: Design, confidence: f64) -> Stratified {
    assert!(confidence > 0.0 && confidence < 1.0);
    let zc = z_two_sided(confidence);
    let mut warnings = Vec::new();
    let used: Vec<&Table> = strata.iter().filter(|t| t.total() > 1).collect();
    if used.len() < strata.len() {
        warnings.push(format!(
            "{} stratum/strata with fewer than 2 people contribute nothing and were skipped.",
            strata.len() - used.len()
        ));
    }
    let crude = strata.iter().fold(Table::new(0, 0, 0, 0), |acc, t| {
        Table::new(acc.a + t.a, acc.b + t.b, acc.c + t.c, acc.d + t.d)
    });
    let crude_res = analyze(crude, design, confidence);

    // MH odds ratio with RBG variance.
    let (mut r, mut s, mut pr, mut ps_qr, mut qs) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for t in &used {
        let (a, b, c, d) = t.f();
        let n = t.total() as f64;
        let (ri, si) = (a * d / n, b * c / n);
        let (pi, qi) = ((a + d) / n, (b + c) / n);
        r += ri;
        s += si;
        pr += pi * ri;
        ps_qr += pi * si + qi * ri;
        qs += qi * si;
    }
    let mh_or = (r > 0.0 && s > 0.0).then(|| {
        let or = r / s;
        let var = pr / (2.0 * r * r) + ps_qr / (2.0 * r * s) + qs / (2.0 * s * s);
        let se = var.sqrt();
        Estimate::new(
            or,
            (or.ln() - zc * se).exp(),
            (or.ln() + zc * se).exp(),
            "Mantel–Haenszel; Robins–Breslow–Greenland interval",
        )
    });

    // MH risk ratio with Greenland–Robins variance.
    let mh_rr = if design == Design::CaseControl {
        None
    } else {
        let (mut num, mut den, mut v) = (0.0, 0.0, 0.0);
        for t in &used {
            let (a, _, c, _) = t.f();
            let (n1, n0, m1, n) = (
                t.n1() as f64,
                t.n0() as f64,
                t.m1() as f64,
                t.total() as f64,
            );
            num += a * n0 / n;
            den += c * n1 / n;
            v += (m1 * n1 * n0 - a * c * n) / (n * n);
        }
        (num > 0.0 && den > 0.0).then(|| {
            let rr = num / den;
            let se = (v / (num * den)).sqrt();
            Estimate::new(
                rr,
                (rr.ln() - zc * se).exp(),
                (rr.ln() + zc * se).exp(),
                "Mantel–Haenszel; Greenland–Robins interval",
            )
        })
    };

    // MH test (as R mantelhaen.test, correct = TRUE).
    let (mut delta, mut var) = (0.0, 0.0);
    for t in &used {
        let (a, _, _, _) = t.f();
        let (n1, n0, m1, m0, n) = (
            t.n1() as f64,
            t.n0() as f64,
            t.m1() as f64,
            t.m0() as f64,
            t.total() as f64,
        );
        delta += a - n1 * m1 / n;
        var += n1 * n0 * m1 * m0 / (n * n * (n - 1.0));
    }
    let mh_test = (var > 0.0).then(|| {
        let yates = if delta.abs() >= 0.5 { 0.5 } else { 0.0 };
        let stat = (delta.abs() - yates).powi(2) / var;
        TestResult {
            name: "Mantel–Haenszel chi-square (continuity-corrected)".into(),
            statistic: Some(stat),
            df: Some(1.0),
            p_value: p_chi_square(stat, 1.0),
        }
    });

    let breslow_day = mh_or
        .as_ref()
        .and_then(|or| breslow_day_tarone(&used, or.value));
    if used
        .iter()
        .any(|t| t.a == 0 || t.b == 0 || t.c == 0 || t.d == 0)
    {
        warnings.push(
            "Some strata contain zero cells; summary estimates remain valid but the \
             Breslow–Day test can be unreliable with sparse strata."
                .into(),
        );
    }
    let change = match (&crude_res.odds_ratio, &mh_or) {
        (Some(c), Some(m)) => Some(100.0 * (c.value - m.value) / m.value),
        _ => None,
    };

    Stratified {
        strata: strata.len(),
        confidence,
        crude_odds_ratio: crude_res.odds_ratio,
        crude_risk_ratio: crude_res.risk_ratio,
        mh_odds_ratio: mh_or,
        mh_risk_ratio: mh_rr,
        mh_test,
        breslow_day,
        crude_vs_adjusted_change_pct: change,
        warnings,
    }
}

/// Breslow–Day statistic with Tarone's adjustment, K − 1 df.
fn breslow_day_tarone(strata: &[&Table], or_mh: f64) -> Option<TestResult> {
    if strata.len() < 2 || !(or_mh.is_finite() && or_mh > 0.0) {
        return None;
    }
    let (mut bd, mut sum_diff, mut sum_var) = (0.0, 0.0, 0.0);
    for t in strata {
        let (a, _, _, _) = t.f();
        let (n1, n0, m1) = (t.n1() as f64, t.n0() as f64, t.m1() as f64);
        // Solve (A)(n0 - m1 + A) = OR (n1 - A)(m1 - A) for the expected A.
        let lo = (m1 - n0).max(0.0);
        let hi = n1.min(m1);
        let f = |x: f64| x * (n0 - m1 + x) - or_mh * (n1 - x) * (m1 - x);
        let ea = if hi - lo < 1e-12 {
            lo
        } else {
            bisect(lo, hi, f).unwrap_or(lo)
        };
        let cells = [ea, n1 - ea, m1 - ea, n0 - m1 + ea];
        if cells.iter().any(|c| *c <= 0.0) {
            continue;
        }
        let v = 1.0 / cells.iter().map(|c| 1.0 / c).sum::<f64>();
        bd += (a - ea).powi(2) / v;
        sum_diff += a - ea;
        sum_var += v;
    }
    if sum_var <= 0.0 {
        return None;
    }
    let stat = bd - sum_diff.powi(2) / sum_var;
    let df = (strata.len() - 1) as f64;
    Some(TestResult {
        name: "Breslow–Day test of homogeneity (Tarone-corrected)".into(),
        statistic: Some(stat),
        df: Some(df),
        p_value: p_chi_square(stat, df),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_outbreak_table() {
        // 2×2 from a point-source outbreak: ate potato salad.
        let r = analyze(Table::new(30, 10, 5, 25), Design::Cohort, 0.95);
        assert!((r.risk_exposed.unwrap() - 0.75).abs() < 1e-12);
        assert!((r.risk_unexposed.unwrap() - 1.0 / 6.0).abs() < 1e-12);
        let rr = r.risk_ratio.unwrap();
        assert!((rr.value - 4.5).abs() < 1e-12);
        assert!(rr.lower > 1.0);
        assert!(r.attributable_fraction_exposed.unwrap() > 0.7);
        assert!(!r.protective);
        assert!(r.exact[0].p_value < 0.001);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn zero_cells_fall_back_to_exact_estimates() {
        let r = analyze(Table::new(12, 0, 3, 9), Design::Cohort, 0.95);
        assert!(r.odds_ratio.is_none());
        let ex = r.odds_ratio_exact.unwrap();
        assert!(ex.value.is_infinite() || ex.upper.is_none());
        assert!(!r.warnings.is_empty());
        let cc = analyze(Table::new(5, 7, 9, 11), Design::CaseControl, 0.95);
        assert!(cc.risk_ratio.is_none() && cc.risk_exposed.is_none());
    }

    #[test]
    fn homogeneous_strata_give_small_breslow_day() {
        let s = mantel_haenszel(
            &[Table::new(20, 80, 10, 90), Table::new(40, 60, 25, 75)],
            Design::Cohort,
            0.95,
        );
        let bd = s.breslow_day.unwrap();
        assert!(bd.p_value > 0.05);
        assert!(s.mh_odds_ratio.unwrap().value > 1.0);
    }
}
