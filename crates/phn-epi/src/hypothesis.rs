//! Classical hypothesis tests, matching R's defaults (checked in
//! `tests/golden.rs`).
//!
//! * t tests: Welch (default), pooled-variance, paired.
//! * Mann–Whitney U (Wilcoxon rank-sum) and Wilcoxon signed-rank: exact
//!   distribution for small samples without ties, otherwise the normal
//!   approximation with tie and continuity corrections (R `wilcox.test`).
//! * One-way ANOVA (equal variances) and Welch's ANOVA (`oneway.test`);
//!   Kruskal–Wallis with the tie correction.
//! * Chi-square test of independence for r × c tables, with Cramér's V.
//! * Pearson correlation (t test, Fisher-z interval) and Spearman
//!   correlation (t approximation, as `cor.test(exact = FALSE)`).
//! * Shapiro–Wilk normality test (Royston 1995, algorithm AS R94) and the
//!   Brown–Forsythe (median-centered Levene) test of equal variances.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dist::{
    p_chi_square, p_f, p_normal_two_sided, p_t_two_sided, phi, t_quantile, z, z_two_sided,
};

/// A test result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Test {
    pub name: String,
    pub statistic: f64,
    /// Name of the statistic (t, W, V, F, H, χ², r, ρ).
    pub statistic_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df2: Option<f64>,
    pub p_value: f64,
    /// Effect estimate (difference in means, correlation, …), when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper: Option<f64>,
    /// Effect size (Cohen's d, η², Cramér's V, rank-biserial r), when computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_size_name: Option<String>,
    /// How the p-value was computed (exact, normal approximation, …).
    pub method: String,
}

impl Test {
    fn new(name: &str, stat_name: &str, statistic: f64, p_value: f64, method: &str) -> Self {
        Self {
            name: name.into(),
            statistic,
            statistic_name: stat_name.into(),
            df: None,
            df2: None,
            p_value,
            estimate: None,
            lower: None,
            upper: None,
            effect_size: None,
            effect_size_name: None,
            method: method.into(),
        }
    }
}

/// Descriptive statistics for one group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Describe {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub median: f64,
    pub q1: f64,
    pub q3: f64,
    pub min: f64,
    pub max: f64,
}

pub fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len() as f64
}

pub fn variance(x: &[f64]) -> f64 {
    let m = mean(x);
    x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (x.len() as f64 - 1.0)
}

/// Quantile with R's default (type 7) definition.
pub fn quantile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let h = (n as f64 - 1.0) * p;
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
}

pub fn describe(x: &[f64]) -> Describe {
    let mut s = x.to_vec();
    s.sort_by(f64::total_cmp);
    Describe {
        n: x.len(),
        mean: mean(x),
        sd: if x.len() > 1 {
            variance(x).sqrt()
        } else {
            f64::NAN
        },
        median: quantile(&s, 0.5),
        q1: quantile(&s, 0.25),
        q3: quantile(&s, 0.75),
        min: s[0],
        max: s[s.len() - 1],
    }
}

/// Mid-ranks (ties get the average rank) and the sizes of tie groups.
pub fn ranks(x: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut idx: Vec<usize> = (0..x.len()).collect();
    idx.sort_by(|a, b| x[*a].total_cmp(&x[*b]));
    let mut r = vec![0.0; x.len()];
    let mut ties = Vec::new();
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && x[idx[j + 1]] == x[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            r[idx[k]] = avg;
        }
        if j > i {
            ties.push((j - i + 1) as f64);
        }
        i = j + 1;
    }
    (r, ties)
}

// --------------------------------------------------------------------- t tests

/// Welch's (unequal variances) or the pooled two-sample t test; estimate is mean(x) − mean(y).
pub fn t_test(x: &[f64], y: &[f64], equal_var: bool, confidence: f64) -> Test {
    let (nx, ny) = (x.len() as f64, y.len() as f64);
    let (mx, my) = (mean(x), mean(y));
    let (vx, vy) = (variance(x), variance(y));
    let (se, df) = if equal_var {
        let sp = ((nx - 1.0) * vx + (ny - 1.0) * vy) / (nx + ny - 2.0);
        ((sp * (1.0 / nx + 1.0 / ny)).sqrt(), nx + ny - 2.0)
    } else {
        let (a, b) = (vx / nx, vy / ny);
        (
            (a + b).sqrt(),
            if a + b > 0.0 {
                (a + b).powi(2) / (a * a / (nx - 1.0) + b * b / (ny - 1.0))
            } else {
                // No variation in either group: the Welch df is 0/0. Use the
                // pooled df so the result is defined (p = 1 when the means
                // are equal, p = 0 when they differ).
                nx + ny - 2.0
            },
        )
    };
    let d = mx - my;
    // Identical constant groups: 0/0 is "no difference", not undefined.
    let t = if se == 0.0 && d == 0.0 { 0.0 } else { d / se };
    let q = t_quantile(1.0 - (1.0 - confidence) / 2.0, df);
    let sp = (((nx - 1.0) * vx + (ny - 1.0) * vy) / (nx + ny - 2.0)).sqrt();
    let mut r = Test::new(
        if equal_var {
            "Two-sample t test (equal variances)"
        } else {
            "Welch two-sample t test"
        },
        "t",
        t,
        p_t_two_sided(t, df),
        "t distribution",
    );
    r.df = Some(df);
    r.estimate = Some(d);
    r.lower = Some(d - q * se);
    r.upper = Some(d + q * se);
    r.effect_size = (sp > 0.0).then(|| d / sp);
    r.effect_size_name = r.effect_size.map(|_| "Cohen's d (pooled SD)".into());
    r
}

/// Paired t test; estimate is the mean of x − y.
pub fn paired_t_test(x: &[f64], y: &[f64], confidence: f64) -> Test {
    let d: Vec<f64> = x.iter().zip(y).map(|(a, b)| a - b).collect();
    let n = d.len() as f64;
    let m = mean(&d);
    let sd = variance(&d).sqrt();
    let se = sd / n.sqrt();
    // Every pair unchanged: 0/0 is "no change", not undefined.
    let t = if se == 0.0 && m == 0.0 { 0.0 } else { m / se };
    let q = t_quantile(1.0 - (1.0 - confidence) / 2.0, n - 1.0);
    let mut r = Test::new(
        "Paired t test",
        "t",
        t,
        p_t_two_sided(t, n - 1.0),
        "t distribution",
    );
    r.df = Some(n - 1.0);
    r.estimate = Some(m);
    r.lower = Some(m - q * se);
    r.upper = Some(m + q * se);
    r.effect_size = (sd > 0.0).then(|| m / sd);
    r.effect_size_name = r.effect_size.map(|_| "Cohen's d (paired)".into());
    r
}

// ------------------------------------------------------------- rank tests

/// Number of ways to choose `m` of 1..=N (N = m + n) with each rank sum,
/// indexed by W = rank sum − m(m+1)/2 (the distribution of R's `pwilcox`).
fn wilcox_counts(m: usize, n: usize) -> Vec<f64> {
    // table[k][w]: ways to choose k of the ranks seen so far with W-offset w.
    // Taking rank `item` as the k-th chosen adds item − k to W (0 ≤ · ≤ n).
    let max_w = m * n;
    let mut table = vec![vec![0.0f64; max_w + 1]; m + 1];
    table[0][0] = 1.0;
    for item in 1..=m + n {
        for k in (1..=m.min(item)).rev() {
            let add = item - k;
            if add > n {
                continue;
            }
            for w in (add..=max_w).rev() {
                table[k][w] += table[k - 1][w - add];
            }
        }
    }
    table[m].clone()
}

/// Two-sided p-value for a rank statistic centered at zero (`zz`) with
/// standard deviation `sigma`, using the continuity correction of R's
/// `wilcox.test`: 0.5 toward zero, and none when the statistic is exactly at
/// its expected value. (`f64::signum(0.0)` is 1.0, unlike R's `sign(0)`, so
/// the sign is taken explicitly.) When every value is tied (`sigma` = 0)
/// the data carry no evidence of a difference and p = 1.
fn p_rank_normal(zz: f64, sigma: f64) -> f64 {
    if sigma.is_nan() || sigma <= 0.0 {
        return 1.0;
    }
    let corr = if zz > 0.0 {
        0.5
    } else if zz < 0.0 {
        -0.5
    } else {
        0.0
    };
    p_normal_two_sided((zz - corr) / sigma)
}

/// Mann–Whitney U / Wilcoxon rank-sum test (as R `wilcox.test(x, y)`).
/// The statistic is W = rank sum of x − nx(nx+1)/2.
pub fn mann_whitney(x: &[f64], y: &[f64]) -> Test {
    let (nx, ny) = (x.len(), y.len());
    let all: Vec<f64> = x.iter().chain(y).copied().collect();
    let (r, ties) = ranks(&all);
    let w = r[..nx].iter().sum::<f64>() - (nx * (nx + 1)) as f64 / 2.0;
    let exact = nx < 50 && ny < 50 && ties.is_empty();
    let (p, method) = if exact {
        let counts = wilcox_counts(nx, ny);
        let total: f64 = counts.iter().sum();
        let wi = w.round() as usize;
        let le: f64 = counts[..=wi].iter().sum::<f64>() / total;
        let ge: f64 = counts[wi..].iter().sum::<f64>() / total;
        ((2.0 * le.min(ge)).min(1.0), "exact distribution")
    } else {
        let n = (nx + ny) as f64;
        let (nxf, nyf) = (nx as f64, ny as f64);
        let zz = w - nxf * nyf / 2.0;
        let tie: f64 = ties.iter().map(|t| t * t * t - t).sum();
        let sigma = (nxf * nyf / 12.0 * ((n + 1.0) - tie / (n * (n - 1.0)))).sqrt();
        (
            p_rank_normal(zz, sigma),
            "normal approximation with continuity correction",
        )
    };
    let mut t = Test::new("Wilcoxon rank-sum (Mann–Whitney U) test", "W", w, p, method);
    t.effect_size = Some(1.0 - 2.0 * w / (nx * ny) as f64);
    t.effect_size_name = Some("rank-biserial correlation (y vs x)".into());
    t
}

/// Wilcoxon signed-rank test on paired differences (as R `wilcox.test(paired = TRUE)`).
pub fn wilcoxon_signed_rank(x: &[f64], y: &[f64]) -> Test {
    let d: Vec<f64> = x.iter().zip(y).map(|(a, b)| a - b).collect();
    let zeros = d.iter().filter(|v| **v == 0.0).count();
    let nz: Vec<f64> = d.iter().copied().filter(|v| *v != 0.0).collect();
    let n = nz.len();
    let abs: Vec<f64> = nz.iter().map(|v| v.abs()).collect();
    let (r, ties) = ranks(&abs);
    let v: f64 = nz
        .iter()
        .zip(&r)
        .filter(|(d, _)| **d > 0.0)
        .map(|(_, r)| r)
        .sum();
    let exact = n < 50 && ties.is_empty() && zeros == 0;
    let (p, method) = if exact {
        // Count subsets of 1..=n by sum.
        let max = n * (n + 1) / 2;
        let mut c = vec![0.0f64; max + 1];
        c[0] = 1.0;
        for k in 1..=n {
            for s in (k..=max).rev() {
                c[s] += c[s - k];
            }
        }
        let total: f64 = c.iter().sum();
        let vi = v.round() as usize;
        let le = c[..=vi].iter().sum::<f64>() / total;
        let ge = c[vi..].iter().sum::<f64>() / total;
        ((2.0 * le.min(ge)).min(1.0), "exact distribution")
    } else {
        let nf = n as f64;
        let zz = v - nf * (nf + 1.0) / 4.0;
        let tie: f64 = ties.iter().map(|t| t * t * t - t).sum();
        let sigma = (nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0 - tie / 48.0).sqrt();
        (
            p_rank_normal(zz, sigma),
            "normal approximation with continuity correction",
        )
    };
    Test::new("Wilcoxon signed-rank test", "V", v, p, method)
}

// ------------------------------------------------------------- k groups

/// One-way ANOVA (equal variances), with η².
pub fn anova(groups: &[Vec<f64>]) -> Test {
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let gm = mean(&all);
    let ssb: f64 = groups
        .iter()
        .map(|g| g.len() as f64 * (mean(g) - gm).powi(2))
        .sum();
    let ssw: f64 = groups
        .iter()
        .map(|g| {
            let m = mean(g);
            g.iter().map(|v| (v - m).powi(2)).sum::<f64>()
        })
        .sum();
    let (df1, df2) = ((groups.len() - 1) as f64, (all.len() - groups.len()) as f64);
    // Every value identical: 0/0 is "no difference" (F = 0, p = 1).
    let f = if ssb == 0.0 && ssw == 0.0 {
        0.0
    } else {
        (ssb / df1) / (ssw / df2)
    };
    let mut t = Test::new("One-way ANOVA", "F", f, p_f(f, df1, df2), "F distribution");
    t.df = Some(df1);
    t.df2 = Some(df2);
    t.effect_size = (ssb + ssw > 0.0).then(|| ssb / (ssb + ssw));
    t.effect_size_name = t.effect_size.map(|_| "eta squared".into());
    t
}

/// Welch's one-way ANOVA (unequal variances), as R `oneway.test`.
pub fn welch_anova(groups: &[Vec<f64>]) -> Test {
    let k = groups.len() as f64;
    let n: Vec<f64> = groups.iter().map(|g| g.len() as f64).collect();
    let m: Vec<f64> = groups.iter().map(|g| mean(g)).collect();
    let w: Vec<f64> = groups
        .iter()
        .zip(&n)
        .map(|(g, n)| n / variance(g))
        .collect();
    let sw: f64 = w.iter().sum();
    let mw: f64 = w.iter().zip(&m).map(|(w, m)| w * m).sum::<f64>() / sw;
    let tmp: f64 = w
        .iter()
        .zip(&n)
        .map(|(w, n)| (1.0 - w / sw).powi(2) / (n - 1.0))
        .sum::<f64>()
        / (k * k - 1.0);
    let num: f64 = w
        .iter()
        .zip(&m)
        .map(|(w, m)| w * (m - mw).powi(2))
        .sum::<f64>()
        / (k - 1.0);
    let f = num / (1.0 + 2.0 * (k - 2.0) * tmp);
    let df2 = 1.0 / (3.0 * tmp);
    let mut t = Test::new(
        "Welch one-way ANOVA (unequal variances)",
        "F",
        f,
        p_f(f, k - 1.0, df2),
        "F distribution",
    );
    t.df = Some(k - 1.0);
    t.df2 = Some(df2);
    t
}

/// Kruskal–Wallis rank-sum test with the tie correction.
pub fn kruskal_wallis(groups: &[Vec<f64>]) -> Test {
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let (r, ties) = ranks(&all);
    let n = all.len() as f64;
    let mut pos = 0;
    let mut sum = 0.0;
    for g in groups {
        let rs: f64 = r[pos..pos + g.len()].iter().sum();
        sum += rs * rs / g.len() as f64;
        pos += g.len();
    }
    let h0 = 12.0 / (n * (n + 1.0)) * sum - 3.0 * (n + 1.0);
    let tie: f64 = ties.iter().map(|t| t * t * t - t).sum();
    let h = h0 / (1.0 - tie / (n * n * n - n));
    let df = (groups.len() - 1) as f64;
    let mut t = Test::new(
        "Kruskal–Wallis rank-sum test",
        "H",
        h,
        p_chi_square(h, df),
        "chi-square approximation",
    );
    t.df = Some(df);
    t
}

/// Chi-square test of independence for an r × c table of counts, with
/// Cramér's V. Returns the test and the smallest expected count.
pub fn chi_square_table(table: &[Vec<f64>]) -> (Test, f64) {
    let rows: Vec<f64> = table.iter().map(|r| r.iter().sum()).collect();
    let cols: Vec<f64> = (0..table[0].len())
        .map(|j| table.iter().map(|r| r[j]).sum())
        .collect();
    let n: f64 = rows.iter().sum();
    let mut x2 = 0.0;
    let mut min_e = f64::INFINITY;
    for (i, r) in table.iter().enumerate() {
        for (j, o) in r.iter().enumerate() {
            let e = rows[i] * cols[j] / n;
            min_e = min_e.min(e);
            if e > 0.0 {
                x2 += (o - e).powi(2) / e;
            }
        }
    }
    // Rows and columns with a zero total contribute no cells, so they do not
    // count toward the degrees of freedom (or Cramér's V).
    let used_rows = rows.iter().filter(|v| **v > 0.0).count();
    let used_cols = cols.iter().filter(|v| **v > 0.0).count();
    let df = (used_rows.saturating_sub(1) * used_cols.saturating_sub(1)) as f64;
    let mut t = Test::new(
        "Pearson chi-square test of independence",
        "χ²",
        x2,
        p_chi_square(x2, df),
        "chi-square approximation",
    );
    t.df = Some(df);
    let k = used_rows.min(used_cols).saturating_sub(1) as f64;
    t.effect_size = (k > 0.0 && n > 0.0).then(|| (x2 / (n * k)).sqrt());
    t.effect_size_name = Some("Cramér's V".into());
    (t, min_e)
}

// ------------------------------------------------------------- correlation

/// Pearson correlation with the t test and Fisher-z interval.
pub fn pearson(x: &[f64], y: &[f64], confidence: f64) -> Test {
    let n = x.len() as f64;
    let (mx, my) = (mean(x), mean(y));
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let syy: f64 = y.iter().map(|b| (b - my).powi(2)).sum();
    let r = sxy / (sxx * syy).sqrt();
    let df = n - 2.0;
    let t = r * (df / (1.0 - r * r)).sqrt();
    let mut out = Test::new(
        "Pearson correlation",
        "t",
        t,
        p_t_two_sided(t, df),
        "t distribution; Fisher-z interval",
    );
    out.df = Some(df);
    out.estimate = Some(r);
    if n > 3.0 {
        let zr = r.atanh();
        let h = z_two_sided(confidence) / (n - 3.0).sqrt();
        out.lower = Some((zr - h).tanh());
        out.upper = Some((zr + h).tanh());
    }
    out
}

/// Spearman rank correlation with the t approximation (R `cor.test(method =
/// "spearman", exact = FALSE)` uses the same statistic).
pub fn spearman(x: &[f64], y: &[f64]) -> Test {
    let (rx, _) = ranks(x);
    let (ry, _) = ranks(y);
    let mut p = pearson(&rx, &ry, 0.95);
    let rho = p.estimate.unwrap_or(f64::NAN);
    let n = x.len() as f64;
    p.name = "Spearman rank correlation".into();
    // R reports S = (n³ − n)(1 − ρ)/6; the p-value uses the t approximation.
    p.statistic = (n * n * n - n) * (1.0 - rho) / 6.0;
    p.statistic_name = "S".into();
    p.df = None;
    p.method = "t approximation".into();
    p.lower = None;
    p.upper = None;
    p.estimate = Some(rho);
    p
}

// ------------------------------------------------------------- assumptions

fn poly(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, v| acc * x + v)
}

/// Shapiro–Wilk W test for normality (Royston 1995, AS R94), 3 ≤ n ≤ 5000.
pub fn shapiro_wilk(x: &[f64]) -> Option<Test> {
    let n = x.len();
    if !(3..=5000).contains(&n) {
        return None;
    }
    let mut s = x.to_vec();
    s.sort_by(f64::total_cmp);
    let range = s[n - 1] - s[0];
    if range < 1e-19 * s[0].abs().max(1.0) {
        return None;
    }
    let nn2 = n / 2;
    let an = n as f64;
    let mut a = vec![0.0; nn2 + 1]; // 1-based
    if n == 3 {
        a[1] = 0.5f64.sqrt();
    } else {
        let an25 = an + 0.25;
        let m: Vec<f64> = (0..=nn2)
            .map(|i| {
                if i == 0 {
                    0.0
                } else {
                    z((i as f64 - 0.375) / an25)
                }
            })
            .collect();
        let summ2 = 2.0 * m[1..].iter().map(|v| v * v).sum::<f64>();
        let ssumm2 = summ2.sqrt();
        let rsn = 1.0 / an.sqrt();
        let c1 = [0.0, 0.221157, -0.147981, -2.07119, 4.434685, -2.706056];
        let c2 = [0.0, 0.042981, -0.293762, -1.752461, 5.682633, -3.582633];
        let a1 = poly(&c1, rsn) - m[1] / ssumm2;
        let (i1, fac) = if n > 5 {
            let a2 = -m[2] / ssumm2 + poly(&c2, rsn);
            a[2] = a2;
            (
                3,
                ((summ2 - 2.0 * m[1] * m[1] - 2.0 * m[2] * m[2])
                    / (1.0 - 2.0 * a1 * a1 - 2.0 * a2 * a2))
                    .sqrt(),
            )
        } else {
            (
                2,
                ((summ2 - 2.0 * m[1] * m[1]) / (1.0 - 2.0 * a1 * a1)).sqrt(),
            )
        };
        a[1] = a1;
        for i in i1..=nn2 {
            a[i] = -m[i] / fac;
        }
    }
    let mean = mean(&s);
    let ssq: f64 = s.iter().map(|v| (v - mean).powi(2)).sum();
    let num: f64 = (1..=nn2).map(|i| a[i] * (s[n - i] - s[i - 1])).sum();
    let w = (num * num / ssq).min(1.0);
    let p = if n == 3 {
        let pi6 = 6.0 / std::f64::consts::PI;
        let stqr = std::f64::consts::PI / 3.0;
        (pi6 * (w.sqrt().asin() - stqr)).max(0.0)
    } else {
        let w1 = (1.0 - w).ln();
        let xx = an.ln();
        let (y, m, sd) = if n <= 11 {
            let gamma = poly(&[-2.273, 0.459], an);
            if w1 >= gamma {
                return Some(Test::new(
                    "Shapiro–Wilk normality test",
                    "W",
                    w,
                    1e-99,
                    "Royston (1995) approximation",
                ));
            }
            (
                -(gamma - w1).ln(),
                poly(&[0.544, -0.39978, 0.025054, -6.714e-4], an),
                poly(&[1.3822, -0.77857, 0.062767, -0.0020322], an).exp(),
            )
        } else {
            (
                w1,
                poly(&[-1.5861, -0.31082, -0.083751, 0.0038915], xx),
                poly(&[-0.4803, -0.082676, 0.0030302], xx).exp(),
            )
        };
        1.0 - phi((y - m) / sd)
    };
    Some(Test::new(
        "Shapiro–Wilk normality test",
        "W",
        w,
        p,
        "Royston (1995) approximation",
    ))
}

/// Brown–Forsythe test of equal variances (Levene's test centered on group medians).
pub fn brown_forsythe(groups: &[Vec<f64>]) -> Test {
    let dev: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| {
            let mut s = g.clone();
            s.sort_by(f64::total_cmp);
            let med = quantile(&s, 0.5);
            g.iter().map(|v| (v - med).abs()).collect()
        })
        .collect();
    let mut t = anova(&dev);
    // Named "Levene (median-centered)" in output, as R car::leveneTest and
    // SciPy do; the eponym "Brown" collides with surname detection.
    t.name = "Levene's test (median-centered) of equal variances".into();
    t.effect_size = None;
    t.effect_size_name = None;
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_with_ties() {
        let (r, t) = ranks(&[3.0, 1.0, 3.0, 2.0]);
        assert_eq!(r, vec![3.5, 1.0, 3.5, 2.0]);
        assert_eq!(t, vec![2.0]);
    }

    #[test]
    fn wilcox_distribution_sums_to_choose() {
        let c = wilcox_counts(3, 4);
        assert_eq!(c.iter().sum::<f64>(), 35.0); // C(7,3)
        assert_eq!(c.len(), 13);
        assert_eq!(c[0], 1.0);
        assert_eq!(c[12], 1.0);
    }

    #[test]
    fn quantile_type_7() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert!((quantile(&s, 0.25) - 1.75).abs() < 1e-12);
        assert!((quantile(&s, 0.5) - 2.5).abs() < 1e-12);
    }

    #[test]
    fn rank_tests_without_a_difference_give_p_one() {
        // Every value tied in both groups: sigma = 0. This used to give p = 0.
        let same = mann_whitney(&[5.0, 5.0, 5.0], &[5.0, 5.0, 5.0]);
        assert_eq!(same.p_value, 1.0, "{same:?}");
        // W exactly at its expected value (8 of 16) with ties: R's
        // wilcox.test applies no continuity correction and gives p = 1
        // (sign(0) = 0); the old code gave 0.876.
        let x = [1.0, 2.0, 2.0, 3.0];
        let mw = mann_whitney(&x, &x);
        assert_eq!(mw.statistic, 8.0);
        assert!((mw.p_value - 1.0).abs() < 1e-12, "{mw:?}");
        // Signed rank: differences 1, −1, 2, −2 and one zero (which forces
        // the normal approximation); V = 5 = n(n+1)/4, so p = 1 (was 0.853).
        let sr = wilcoxon_signed_rank(&[1.0, 0.0, 2.0, 0.0, 3.0], &[0.0, 1.0, 0.0, 2.0, 3.0]);
        assert_eq!(sr.statistic, 5.0);
        assert!((sr.p_value - 1.0).abs() < 1e-12, "{sr:?}");
        // Before and after identical for every pair (n = 0 after dropping
        // zeros): p = 1, not 0.
        let none = wilcoxon_signed_rank(&[4.0, 4.0, 4.0], &[4.0, 4.0, 4.0]);
        assert_eq!(none.p_value, 1.0, "{none:?}");
        // A real difference with ties is still corrected toward zero:
        // W = 0, zz = −8, sigma = √(16/12 · (9 − 24/56)), z = −7.5/sigma.
        let apart = mann_whitney(&[1.0, 1.0, 2.0, 2.0], &[3.0, 3.0, 4.0, 4.0]);
        let sigma = (16.0f64 / 12.0 * (9.0 - 24.0 / 56.0)).sqrt();
        assert!((apart.p_value - p_normal_two_sided(-7.5 / sigma)).abs() < 1e-12);
        assert!(apart.p_value < 0.05);
    }

    #[test]
    fn tests_on_data_without_variation_are_defined() {
        // Identical constant groups: no difference, p = 1 (not NaN).
        let w = t_test(&[5.0, 5.0, 5.0], &[5.0, 5.0, 5.0], false, 0.95);
        assert_eq!(w.statistic, 0.0);
        assert!((w.p_value - 1.0).abs() < 1e-12, "{w:?}");
        assert_eq!(w.df, Some(4.0));
        assert!(w.effect_size.is_none() && w.effect_size_name.is_none());
        assert_eq!((w.lower, w.upper), (Some(0.0), Some(0.0)));
        // Constant groups that differ: an infinite statistic has p = 0.
        let d = t_test(&[5.0, 5.0, 5.0], &[7.0, 7.0, 7.0], false, 0.95);
        assert!(d.statistic.is_infinite() && d.p_value == 0.0, "{d:?}");
        let p = paired_t_test(&[4.0, 4.0, 4.0], &[4.0, 4.0, 4.0], 0.95);
        assert_eq!(p.statistic, 0.0);
        assert!((p.p_value - 1.0).abs() < 1e-12 && p.effect_size.is_none());
        let a = anova(&[vec![2.0; 3], vec![2.0; 3], vec![2.0; 3]]);
        assert_eq!(a.statistic, 0.0);
        assert!((a.p_value - 1.0).abs() < 1e-12 && a.effect_size.is_none());
        // The median-centered Levene test on such data reports equal variances.
        assert!((brown_forsythe(&[vec![2.0; 3], vec![2.0; 3]]).p_value - 1.0).abs() < 1e-12);
    }

    #[test]
    fn chi_square_ignores_empty_rows_and_columns() {
        // A 3×2 table whose middle row is empty is the 2×2 table
        // [[10, 20], [20, 10]]: χ² = 60·(100 − 400)²/30⁴ = 6.667 on 1 df
        // (the old code used 2 df), Cramér's V = √(6.667/60) = 0.333.
        let (t, min_e) = chi_square_table(&[vec![10.0, 20.0], vec![0.0, 0.0], vec![20.0, 10.0]]);
        assert_eq!(t.df, Some(1.0));
        assert!((t.statistic - 20.0 / 3.0).abs() < 1e-9);
        assert!((t.p_value - p_chi_square(20.0 / 3.0, 1.0)).abs() < 1e-12);
        assert!((t.p_value - 0.009_823).abs() < 1e-5, "{}", t.p_value);
        assert!((t.effect_size.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(min_e, 0.0);
    }
}
