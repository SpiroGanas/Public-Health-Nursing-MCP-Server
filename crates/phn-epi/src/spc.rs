//! Statistical process control: run charts and Shewhart control charts.
//!
//! Run charts use the median as the center line and the Anhøj rules (Anhøj J,
//! Olesen AV. PLoS One 2014;9:e113825; the default in R `qicharts2`):
//! * unusually long run: the longest run of points on one side of the median
//!   exceeds round(log₂(n) + 3), where n excludes points on the median;
//! * too few crossings: the median is crossed fewer than the 5th percentile
//!   of Binomial(n − 1, 0.5) times.
//!
//! Control charts (Provost LP, Murray SK. *The Health Care Data Guide*, 2011):
//! * XmR (individuals): limits mean ± 2.66 × average moving range
//!   (σ = MR̄ / d₂, d₂ = 1.128);
//! * p (proportions, varying denominators), u (rates per unit, varying
//!   exposure), and c (counts, constant opportunity) with 3-sigma limits;
//! * special-cause rules: (1) a point outside the limits; (2) eight or more
//!   consecutive points on one side of the center line (points on the line
//!   are skipped); (3) six consecutive points all increasing or all
//!   decreasing (repeated values are skipped); (4) two of three consecutive
//!   points in the outer third (beyond 2σ) on the same side; (5) fifteen
//!   consecutive points in the inner third (within 1σ).
//!
//! Limits can be computed from a baseline (the first k points) and
//! recomputed for each phase after a documented process change.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use statrs::distribution::{Binomial, DiscreteCDF};

/// Chart type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    /// Run chart (median center line, Anhøj rules).
    Run,
    /// Individuals and moving range (XmR / I chart).
    Xmr,
    /// Proportion defective (numerator / denominator).
    P,
    /// Rate per unit of exposure (count / exposure).
    U,
    /// Count with constant opportunity.
    C,
}

/// Special-cause rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    OutsideLimits,
    Shift,
    Trend,
    TwoOfThreeOuterThird,
    FifteenInnerThird,
    LongRun,
    TooFewCrossings,
}

impl Rule {
    pub fn describe(self) -> &'static str {
        match self {
            Rule::OutsideLimits => "point outside the control limits",
            Rule::Shift => "8 or more consecutive points on one side of the center line",
            Rule::Trend => "6 consecutive points all increasing or all decreasing",
            Rule::TwoOfThreeOuterThird => {
                "2 of 3 consecutive points beyond 2 sigma on the same side"
            }
            Rule::FifteenInnerThird => "15 consecutive points within 1 sigma of the center line",
            Rule::LongRun => "unusually long run on one side of the median (Anhøj)",
            Rule::TooFewCrossings => "too few crossings of the median (Anhøj)",
        }
    }
}

/// One plotted point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub value: f64,
    pub center: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lcl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ucl: Option<f64>,
    pub signals: Vec<Rule>,
    /// Phase number (0-based).
    pub phase: usize,
}

/// A chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Chart {
    pub chart_type: ChartType,
    pub points: Vec<Point>,
    /// Phase summaries: run-chart statistics or control-chart limits.
    pub phases: Vec<PhaseSummary>,
    /// Average moving range (XmR only), per phase.
    pub warnings: Vec<String>,
}

/// Per-phase summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhaseSummary {
    pub start: usize,
    pub end: usize,
    pub center: f64,
    /// Points used to compute the center and limits.
    pub baseline_points: usize,
    /// XmR: average moving range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_moving_range: Option<f64>,
    /// XmR: upper limit of the moving-range chart (3.267 × MR̄).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moving_range_ucl: Option<f64>,
    /// Run charts: useful observations (not on the median).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub useful_observations: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_run: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longest_run_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crossings: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crossings_limit: Option<usize>,
}

/// d₂ for subgroups of 2 (moving ranges of two points): the conventional
/// table value used by qcc and *The Health Care Data Guide* (3 / 1.128 = 2.66).
pub const D2: f64 = 1.128;
/// D₄ for subgroups of 2 (moving-range chart upper limit factor).
pub const D4: f64 = 3.267;

/// Inputs.
#[derive(Debug, Clone)]
pub struct Input {
    pub chart: ChartType,
    /// Measures (run, XmR), numerators (p, u), or counts (c).
    pub values: Vec<f64>,
    /// Denominators (p) or exposure units (u).
    pub denominators: Option<Vec<f64>>,
    /// Indices where a new phase starts (after a process change).
    pub phase_starts: Vec<usize>,
    /// Number of points at the start of each phase used for the center and
    /// limits (default: all points in the phase).
    pub baseline: Option<usize>,
}

/// Build a chart.
pub fn chart(input: &Input) -> Result<Chart, String> {
    let n = input.values.len();
    if n < 2 {
        return Err("a chart needs at least 2 points".into());
    }
    if input.values.iter().any(|v| !v.is_finite()) {
        return Err("every value must be a number".into());
    }
    let ratio = matches!(input.chart, ChartType::P | ChartType::U);
    if ratio {
        let d = input
            .denominators
            .as_ref()
            .ok_or("p and u charts need a denominator for each point")?;
        if d.len() != n || d.iter().any(|v| *v <= 0.0) {
            return Err("each point needs a denominator greater than zero".into());
        }
        if input.chart == ChartType::P && input.values.iter().zip(d).any(|(x, n)| x < &0.0 || x > n)
        {
            return Err("p-chart numerators must be between 0 and the denominator".into());
        }
    }
    if matches!(input.chart, ChartType::C | ChartType::U) && input.values.iter().any(|v| *v < 0.0) {
        return Err("counts cannot be negative".into());
    }
    let mut starts: Vec<usize> = input
        .phase_starts
        .iter()
        .copied()
        .filter(|s| *s > 0 && *s < n)
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let mut bounds = vec![0];
    bounds.extend(starts);
    bounds.push(n);
    let mut points = Vec::with_capacity(n);
    let mut phases = Vec::new();
    let mut warnings = Vec::new();
    for (ph, w) in bounds.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let base_end = input.baseline.map_or(b, |k| (a + k).min(b));
        if base_end - a < 2 {
            return Err(format!("phase {} needs at least 2 baseline points", ph + 1));
        }
        let (mut pts, summary) = match input.chart {
            ChartType::Run => run_phase(&input.values[a..b], base_end - a),
            _ => control_phase(input, a, b, base_end),
        };
        for p in &mut pts {
            p.phase = ph;
        }
        let mut summary = summary;
        summary.start = a;
        summary.end = b - 1;
        if base_end - a < 12 && input.chart != ChartType::Run {
            warnings.push(format!(
                "Phase {} limits rest on {} points; at least 20 are recommended (limits from fewer \
                 than 12 points are trial limits).",
                ph + 1,
                base_end - a
            ));
        }
        points.extend(pts);
        phases.push(summary);
    }
    if input.chart == ChartType::Run && n < 10 {
        warnings.push(
            "Run-chart rules need about 10 or more points to detect signals reliably.".into(),
        );
    }
    Ok(Chart {
        chart_type: input.chart,
        points,
        phases,
        warnings,
    })
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

/// Anhøj limits for n useful observations: (longest-run limit, minimum crossings).
pub fn anhoej_limits(useful: usize) -> (usize, usize) {
    let longest = ((useful.max(1) as f64).log2() + 3.0).round() as usize;
    let crossings = if useful >= 2 {
        Binomial::new(0.5, (useful - 1) as u64)
            .map(|b| {
                // R qbinom(0.05, n − 1, 0.5): smallest k with P(X ≤ k) ≥ 0.05.
                (0..useful as u64).find(|k| b.cdf(*k) >= 0.05).unwrap_or(0) as usize
            })
            .unwrap_or(0)
    } else {
        0
    };
    (longest, crossings)
}

fn run_phase(v: &[f64], base: usize) -> (Vec<Point>, PhaseSummary) {
    let med = median(&v[..base]);
    // Sides: +1 above, −1 below, 0 on the median (ignored).
    let sides: Vec<i8> = v
        .iter()
        .map(|x| {
            if *x > med {
                1
            } else if *x < med {
                -1
            } else {
                0
            }
        })
        .collect();
    let useful: Vec<(usize, i8)> = sides
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, s)| *s != 0)
        .collect();
    let mut runs: Vec<(usize, usize)> = Vec::new(); // (start index in useful, length)
    for (k, (_, s)) in useful.iter().enumerate() {
        match runs.last_mut() {
            Some((st, len)) if useful[*st].1 == *s => *len += 1,
            _ => runs.push((k, 1)),
        }
        let _ = k;
    }
    let longest = runs.iter().map(|r| r.1).max().unwrap_or(0);
    let crossings = runs.len().saturating_sub(1);
    let (lim_run, lim_cross) = anhoej_limits(useful.len());
    let mut points: Vec<Point> = v
        .iter()
        .map(|x| Point {
            value: *x,
            center: med,
            lcl: None,
            ucl: None,
            signals: vec![],
            phase: 0,
        })
        .collect();
    for (st, len) in &runs {
        if *len > lim_run {
            for k in *st..st + len {
                points[useful[k].0].signals.push(Rule::LongRun);
            }
        }
    }
    if crossings < lim_cross {
        for p in &mut points {
            if !p.signals.contains(&Rule::TooFewCrossings) {
                p.signals.push(Rule::TooFewCrossings);
            }
        }
    }
    (
        points,
        PhaseSummary {
            start: 0,
            end: 0,
            center: med,
            baseline_points: base,
            average_moving_range: None,
            moving_range_ucl: None,
            useful_observations: Some(useful.len()),
            longest_run: Some(longest),
            longest_run_limit: Some(lim_run),
            crossings: Some(crossings),
            crossings_limit: Some(lim_cross),
        },
    )
}

/// Sigma for the point at an index (constant or varying with the denominator).
type SigmaFn = Box<dyn Fn(usize) -> f64>;

fn control_phase(input: &Input, a: usize, b: usize, base_end: usize) -> (Vec<Point>, PhaseSummary) {
    let v = &input.values;
    let den = input.denominators.as_deref();
    let (center, sigma_at, mr): (f64, SigmaFn, Option<f64>) = match input.chart {
        ChartType::Xmr => {
            let base = &v[a..base_end];
            let m = base.iter().sum::<f64>() / base.len() as f64;
            let mrbar =
                base.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (base.len() - 1) as f64;
            let s = mrbar / D2;
            (m, Box::new(move |_| s), Some(mrbar))
        }
        ChartType::P => {
            let d = den.expect("validated");
            let pbar = v[a..base_end].iter().sum::<f64>() / d[a..base_end].iter().sum::<f64>();
            let d = d.to_vec();
            (
                pbar,
                Box::new(move |i| (pbar * (1.0 - pbar) / d[i]).sqrt()),
                None,
            )
        }
        ChartType::U => {
            let d = den.expect("validated");
            let ubar = v[a..base_end].iter().sum::<f64>() / d[a..base_end].iter().sum::<f64>();
            let d = d.to_vec();
            (ubar, Box::new(move |i| (ubar / d[i]).sqrt()), None)
        }
        ChartType::C => {
            let base = &v[a..base_end];
            let c = base.iter().sum::<f64>() / base.len() as f64;
            (c, Box::new(move |_| c.sqrt()), None)
        }
        ChartType::Run => unreachable!(),
    };
    let value = |i: usize| match input.chart {
        ChartType::P | ChartType::U => v[i] / den.expect("validated")[i],
        _ => v[i],
    };
    let mut pts: Vec<Point> = (a..b)
        .map(|i| {
            let s = sigma_at(i);
            let mut lcl = center - 3.0 * s;
            let mut ucl = center + 3.0 * s;
            if matches!(input.chart, ChartType::P | ChartType::U | ChartType::C) {
                lcl = lcl.max(0.0);
            }
            if input.chart == ChartType::P {
                ucl = ucl.min(1.0);
            }
            Point {
                value: value(i),
                center,
                lcl: Some(lcl),
                ucl: Some(ucl),
                signals: vec![],
                phase: 0,
            }
        })
        .collect();
    let sig: Vec<f64> = (a..b).map(&sigma_at).collect();
    apply_rules(&mut pts, &sig);
    (
        pts,
        PhaseSummary {
            start: 0,
            end: 0,
            center,
            baseline_points: base_end - a,
            average_moving_range: mr,
            moving_range_ucl: mr.map(|m| D4 * m),
            useful_observations: None,
            longest_run: None,
            longest_run_limit: None,
            crossings: None,
            crossings_limit: None,
        },
    )
}

fn mark(points: &mut [Point], range: std::ops::Range<usize>, rule: Rule) {
    for p in &mut points[range] {
        if !p.signals.contains(&rule) {
            p.signals.push(rule);
        }
    }
}

fn apply_rules(p: &mut [Point], sigma: &[f64]) {
    let n = p.len();
    // 1. Outside limits.
    for i in 0..n {
        let (lo, hi) = (
            p[i].lcl.unwrap_or(f64::NEG_INFINITY),
            p[i].ucl.unwrap_or(f64::INFINITY),
        );
        if p[i].value > hi || p[i].value < lo {
            p[i].signals.push(Rule::OutsideLimits);
        }
    }
    // 2. Shift: 8+ on one side. A point exactly on the center line neither
    // counts toward a run nor breaks it (Provost & Murray).
    let side: Vec<i8> = p
        .iter()
        .map(|q| {
            if q.value > q.center {
                1
            } else if q.value < q.center {
                -1
            } else {
                0
            }
        })
        .collect();
    let off_center: Vec<usize> = (0..n).filter(|i| side[*i] != 0).collect();
    let mut a = 0;
    while a < off_center.len() {
        let mut b = a;
        while b < off_center.len() && side[off_center[b]] == side[off_center[a]] {
            b += 1;
        }
        if b - a >= 8 {
            mark(p, off_center[a]..off_center[b - 1] + 1, Rule::Shift);
        }
        a = b;
    }
    // 3. Trend: 6 consecutive points all increasing or all decreasing (5
    // successive differences in one direction). Repeated values neither make
    // nor break a trend: a point equal to the one before it is ignored when
    // counting (Provost & Murray), so a flat series is never a trend.
    let mut distinct: Vec<usize> = Vec::with_capacity(n);
    for i in 0..n {
        if distinct.last().is_none_or(|k| p[*k].value != p[i].value) {
            distinct.push(i);
        }
    }
    let direction = |from: usize, to: usize| -> i8 {
        if p[to].value > p[from].value {
            1
        } else if p[to].value < p[from].value {
            -1
        } else {
            0
        }
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut a = 0;
    while a + 1 < distinct.len() {
        let dir = direction(distinct[a], distinct[a + 1]);
        let mut b = a + 1;
        while b + 1 < distinct.len() && direction(distinct[b], distinct[b + 1]) == dir {
            b += 1;
        }
        if dir != 0 && b - a + 1 >= 6 {
            runs.push((distinct[a], distinct[b]));
        }
        a = b;
    }
    for (from, to) in runs {
        mark(p, from..to + 1, Rule::Trend);
    }
    // 4. Two of three beyond 2σ on the same side.
    for i in 0..n.saturating_sub(2) {
        for s in [1.0, -1.0] {
            let hits: Vec<usize> = (i..i + 3)
                .filter(|k| s * (p[*k].value - p[*k].center) > 2.0 * sigma[*k])
                .collect();
            if hits.len() >= 2 {
                for k in hits {
                    if !p[k].signals.contains(&Rule::TwoOfThreeOuterThird) {
                        p[k].signals.push(Rule::TwoOfThreeOuterThird);
                    }
                }
            }
        }
    }
    // 5. Fifteen consecutive within 1σ.
    let inner: Vec<bool> = (0..n)
        .map(|k| (p[k].value - p[k].center).abs() < sigma[k])
        .collect();
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && inner[j] {
            j += 1;
        }
        if j - i >= 15 {
            mark(p, i..j, Rule::FifteenInnerThird);
        }
        i = if j > i { j } else { i + 1 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anhoej_limits_match_r() {
        // Reference: R 4.3, round(log2(n) + 3) and qbinom(0.05, n - 1, 0.5)
        // (the definitions in Anhøj & Olesen 2014 and qicharts2).
        for (n, run, cross) in [(10, 6, 2), (20, 7, 6), (24, 8, 8), (30, 8, 10), (50, 9, 19)] {
            assert_eq!(anhoej_limits(n), (run, cross), "n = {n}");
        }
    }

    #[test]
    fn xmr_limits_and_signals() {
        let mut v = vec![
            10.0, 12.0, 11.0, 13.0, 9.0, 11.0, 12.0, 10.0, 11.0, 12.0, 30.0,
        ];
        let c = chart(&Input {
            chart: ChartType::Xmr,
            values: v.clone(),
            denominators: None,
            phase_starts: vec![],
            baseline: Some(10),
        })
        .unwrap();
        let ph = &c.phases[0];
        assert!((ph.center - 11.1).abs() < 1e-12);
        assert!(c.points[10].signals.contains(&Rule::OutsideLimits));
        // A trend of six rising points.
        v = vec![5.0, 5.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 5.0];
        let c = chart(&Input {
            chart: ChartType::Xmr,
            values: v,
            denominators: None,
            phase_starts: vec![],
            baseline: None,
        })
        .unwrap();
        assert!(
            c.points[3..9]
                .iter()
                .all(|p| p.signals.contains(&Rule::Trend))
        );
    }

    #[test]
    fn run_chart_long_run() {
        let mut v: Vec<f64> = (0..12)
            .map(|i| if i % 2 == 0 { 10.0 } else { 12.0 })
            .collect();
        v.extend([15.0; 9]);
        let c = chart(&Input {
            chart: ChartType::Run,
            values: v,
            denominators: None,
            phase_starts: vec![],
            baseline: Some(12),
        })
        .unwrap();
        assert!(
            c.points[12..]
                .iter()
                .all(|p| p.signals.contains(&Rule::LongRun))
        );
    }
}
