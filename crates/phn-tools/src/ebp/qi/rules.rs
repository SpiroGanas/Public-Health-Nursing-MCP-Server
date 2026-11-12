//! Run chart rules in the IHI/Perla style (shift, trend, too few or too many runs, astronomical
//! point) for `plan_quality_improvement` (EBP-05).
//!
//! The Anhøj rules (unusually long run, too few crossings) and the control chart limits are
//! computed by `phn_epi::spc`; this module adds the rules taught in IHI's Model for Improvement
//! material, which `phn_epi::spc` does not have. Definitions and sources are in
//! `content/ebp/quality_improvement.yaml`; the golden tests compare this code with an independent
//! Python computation (`tests/fixtures/generators/qi_run_chart_golden.py`).
//!
//! * The median is taken from the baseline points. A point exactly on the median neither adds to
//!   a run nor breaks it.
//! * Shift: [`SHIFT_LEN`] or more consecutive points on one side of the median.
//! * Trend: [`TREND_LEN`] or more consecutive points all going up or all going down; a repeated
//!   value counts once.
//! * Runs: the number of runs is compared with the exact distribution of the number of runs
//!   (Wald–Wolfowitz) for the observed numbers of points above and below the median, with at most
//!   [`TAIL`] probability in each tail.
//! * Astronomical point: a screen, not Perla's wording (see [`astronomical`]).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Consecutive points on one side of the median that make a shift.
pub const SHIFT_LEN: usize = 6;
/// Consecutive rising or falling points that make a trend.
pub const TREND_LEN: usize = 5;
/// Probability allowed in each tail of the runs distribution.
pub const TAIL: f64 = 0.025;
/// Fewest points for the astronomical-point screen.
pub const ASTRONOMICAL_MIN_POINTS: usize = 10;
/// Modified z-score above which a point is a candidate astronomical point.
pub const ASTRONOMICAL_Z: f64 = 3.5;

/// A stretch of points picked out by a rule (indices into the series).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Span {
    pub first: usize,
    pub last: usize,
    /// Points counted (points on the median, and repeated values in a trend, are not).
    pub length: usize,
    /// "above" or "below" the median for a shift; "up" or "down" for a trend.
    pub direction: String,
    /// Every point in the stretch (series indices); not part of the response.
    #[serde(skip)]
    pub indices: Vec<usize>,
}

/// The runs test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RunsTest {
    /// Runs observed (points on the median skipped).
    pub observed: usize,
    pub points_above: usize,
    pub points_below: usize,
    /// Too few runs when the observed number is at most this (none when it cannot signal).
    pub too_few_at_most: Option<usize>,
    /// Too many runs when the observed number is at least this (none when it cannot signal).
    pub too_many_at_least: Option<usize>,
    pub too_few: bool,
    pub too_many: bool,
}

/// Results for one phase (the points from one median to the next).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PerlaPhase {
    /// Index of the first and last point of the phase.
    pub start: usize,
    pub end: usize,
    /// Median of the baseline points.
    pub median: f64,
    pub baseline_points: usize,
    /// Points not on the median.
    pub useful: usize,
    pub on_median: usize,
    pub longest_run: usize,
    /// Every shift (runs of [`SHIFT_LEN`] or more).
    pub shifts: Vec<Span>,
    pub longest_trend: usize,
    pub trends: Vec<Span>,
    pub runs: RunsTest,
    /// Candidate astronomical points (series indices).
    pub astronomical: Vec<usize>,
}

/// Signals for every point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Perla {
    pub phases: Vec<PerlaPhase>,
}

impl Perla {
    /// Series indices in a shift.
    pub fn shift_points(&self) -> Vec<usize> {
        self.phases
            .iter()
            .flat_map(|p| p.shifts.iter().flat_map(|s| s.indices.iter().copied()))
            .collect()
    }
    /// Series indices in a trend.
    pub fn trend_points(&self) -> Vec<usize> {
        self.phases
            .iter()
            .flat_map(|p| p.trends.iter().flat_map(|s| s.indices.iter().copied()))
            .collect()
    }
    /// Series indices of candidate astronomical points.
    pub fn astronomical_points(&self) -> Vec<usize> {
        self.phases
            .iter()
            .flat_map(|p| p.astronomical.iter().copied())
            .collect()
    }
}

/// Median of a non-empty slice.
pub fn median(values: &[f64]) -> f64 {
    let mut s = values.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

fn ln_factorials(n: usize) -> Vec<f64> {
    let mut v = vec![0.0; n + 1];
    for i in 2..=n {
        v[i] = v[i - 1] + (i as f64).ln();
    }
    v
}

/// Exact distribution of the number of runs for `n1` and `n2` points of two kinds, as the
/// probability of each count `0..=n1+n2` (index = runs).
fn runs_pmf(n1: usize, n2: usize) -> Vec<f64> {
    let n = n1 + n2;
    let lf = ln_factorials(n);
    let ln_c = |a: usize, b: usize| lf[a] - lf[b] - lf[a - b];
    let ln_total = ln_c(n, n1);
    let c = |a: i64, b: i64| -> Option<f64> {
        (a >= 0 && b >= 0 && b <= a).then(|| ln_c(a as usize, b as usize))
    };
    let (a, b) = (n1 as i64, n2 as i64);
    let mut pmf = vec![0.0; n + 1];
    for (r, slot) in pmf.iter_mut().enumerate().skip(2) {
        let k = (r / 2) as i64;
        let p = if r % 2 == 0 {
            match (c(a - 1, k - 1), c(b - 1, k - 1)) {
                (Some(x), Some(y)) => (2f64.ln() + x + y - ln_total).exp(),
                _ => 0.0,
            }
        } else {
            let t1 = match (c(a - 1, k), c(b - 1, k - 1)) {
                (Some(x), Some(y)) => (x + y - ln_total).exp(),
                _ => 0.0,
            };
            let t2 = match (c(a - 1, k - 1), c(b - 1, k)) {
                (Some(x), Some(y)) => (x + y - ln_total).exp(),
                _ => 0.0,
            };
            t1 + t2
        };
        *slot = p;
    }
    pmf
}

/// Limits for the number of runs given the numbers of points above and below the median:
/// (too few when at most, too many when at least). `None` when the test cannot signal.
pub fn runs_limits(n1: usize, n2: usize) -> (Option<usize>, Option<usize>) {
    if n1 == 0 || n2 == 0 {
        return (None, None);
    }
    let pmf = runs_pmf(n1, n2);
    let eps = 1e-12;
    let mut lower = None;
    let mut cum = 0.0;
    for (r, p) in pmf.iter().enumerate() {
        cum += p;
        if cum <= TAIL + eps {
            if *p > 0.0 || lower.is_some() {
                lower = Some(r);
            }
        } else {
            break;
        }
    }
    let mut upper = None;
    let mut cum = 0.0;
    for (r, p) in pmf.iter().enumerate().rev() {
        cum += p;
        if cum <= TAIL + eps {
            if *p > 0.0 || upper.is_some() {
                upper = Some(r);
            }
        } else {
            break;
        }
    }
    // A bound on a count that cannot occur (below 2 runs) is no signal at all.
    (lower.filter(|r| *r >= 2), upper)
}

/// Screen for a candidate astronomical point: the modified z-score of a point against the other
/// points, `0.6745 × (x − median) / MAD`, is above [`ASTRONOMICAL_Z`] in size. Perla's rule says
/// an astronomical point is one that is obviously different from the rest and leaves the
/// decision to the viewer; this is a repeatable stand-in, not the published definition. It is
/// skipped for fewer than [`ASTRONOMICAL_MIN_POINTS`] points and when the other points have no
/// spread.
pub fn astronomical(values: &[f64]) -> Vec<usize> {
    if values.len() < ASTRONOMICAL_MIN_POINTS {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..values.len() {
        let rest: Vec<f64> = values
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, v)| *v)
            .collect();
        let m = median(&rest);
        let devs: Vec<f64> = rest.iter().map(|x| (x - m).abs()).collect();
        let mad = median(&devs);
        if mad <= 0.0 {
            continue;
        }
        if (0.6745 * (values[i] - m) / mad).abs() > ASTRONOMICAL_Z {
            out.push(i);
        }
    }
    out
}

/// Analyze one phase (`values` is the phase; indices in the result are relative to it).
/// `baseline` points from the start give the median; `None` uses every point.
fn analyze_one(values: &[f64], baseline: Option<usize>) -> PerlaPhase {
    let k = baseline.unwrap_or(values.len()).clamp(1, values.len());
    let med = median(&values[..k]);
    let side: Vec<i8> = values
        .iter()
        .map(|v| match v.partial_cmp(&med) {
            Some(std::cmp::Ordering::Greater) => 1,
            Some(std::cmp::Ordering::Less) => -1,
            _ => 0,
        })
        .collect();

    // Runs of points on one side of the median (median points skipped).
    let mut runs: Vec<(i8, Vec<usize>)> = Vec::new();
    for (i, s) in side.iter().enumerate() {
        if *s == 0 {
            continue;
        }
        match runs.last_mut() {
            Some((cur, idx)) if *cur == *s => idx.push(i),
            _ => runs.push((*s, vec![i])),
        }
    }
    let longest_run = runs.iter().map(|(_, i)| i.len()).max().unwrap_or(0);
    let shifts: Vec<Span> = runs
        .iter()
        .filter(|(_, i)| i.len() >= SHIFT_LEN)
        .map(|(s, i)| Span {
            first: i[0],
            last: i[i.len() - 1],
            length: i.len(),
            direction: if *s > 0 { "above" } else { "below" }.into(),
            indices: i.clone(),
        })
        .collect();
    let above = side.iter().filter(|s| **s > 0).count();
    let below = side.iter().filter(|s| **s < 0).count();
    let (few, many) = runs_limits(above, below);
    let observed = runs.len();

    // Trends: collapse repeated values, then find maximal monotone stretches.
    let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
    for (i, v) in values.iter().enumerate() {
        match groups.last_mut() {
            Some((g, idx)) if *g == *v => idx.push(i),
            _ => groups.push((*v, vec![i])),
        }
    }
    let mut trends = Vec::new();
    let mut longest_trend = 0;
    if groups.len() >= 2 {
        let dir: Vec<i8> = groups
            .windows(2)
            .map(|w| if w[1].0 > w[0].0 { 1 } else { -1 })
            .collect();
        let mut a = 0;
        while a < dir.len() {
            let mut b = a;
            while b + 1 < dir.len() && dir[b + 1] == dir[a] {
                b += 1;
            }
            let members = &groups[a..=b + 1];
            let distinct = members.len();
            longest_trend = longest_trend.max(distinct);
            if distinct >= TREND_LEN {
                let idx: Vec<usize> = members.iter().flat_map(|(_, i)| i.iter().copied()).collect();
                trends.push(Span {
                    first: idx[0],
                    last: idx[idx.len() - 1],
                    length: distinct,
                    direction: if dir[a] > 0 { "up" } else { "down" }.into(),
                    indices: idx,
                });
            }
            a = b + 1;
        }
    }

    PerlaPhase {
        start: 0,
        end: values.len() - 1,
        median: med,
        baseline_points: k,
        useful: above + below,
        on_median: side.iter().filter(|s| **s == 0).count(),
        longest_run,
        shifts,
        longest_trend,
        trends,
        runs: RunsTest {
            observed,
            points_above: above,
            points_below: below,
            too_few_at_most: few,
            too_many_at_least: many,
            too_few: few.is_some_and(|a| observed <= a),
            too_many: many.is_some_and(|b| observed >= b),
        },
        astronomical: astronomical(values),
    }
}

/// Apply the rules to `values`. `phase_starts` are the indices where a new phase (and a new
/// median) begins after a documented process change; `baseline` is how many points at the start
/// of each phase give its median (`None`: all points in the phase).
pub fn analyze(
    values: &[f64],
    phase_starts: &[usize],
    baseline: Option<usize>,
) -> Result<Perla, String> {
    if values.len() < 2 {
        return Err("a run chart needs at least 2 points".into());
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err("every value must be a number".into());
    }
    let mut starts: Vec<usize> = phase_starts
        .iter()
        .copied()
        .filter(|s| *s > 0 && *s < values.len())
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let mut bounds = vec![0];
    bounds.extend(starts);
    bounds.push(values.len());
    let mut phases = Vec::new();
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b - a < 2 {
            return Err(format!(
                "the phase starting at point {} has fewer than 2 points",
                a + 1
            ));
        }
        let mut p = analyze_one(&values[a..b], baseline);
        p.start = a;
        p.end = b - 1;
        for s in p.shifts.iter_mut().chain(p.trends.iter_mut()) {
            s.first += a;
            s.last += a;
            for i in s.indices.iter_mut() {
                *i += a;
            }
        }
        for i in p.astronomical.iter_mut() {
            *i += a;
        }
        phases.push(p);
    }
    Ok(Perla { phases })
}
