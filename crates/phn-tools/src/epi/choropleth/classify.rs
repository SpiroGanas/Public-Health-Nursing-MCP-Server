//! Class breaks for choropleth maps: quantiles and Jenks natural breaks.
//!
//! * **Quantile**: each class holds about the same number of areas. The upper
//!   bound of class *i* (1-based) is the value at sorted position
//!   `ceil(i·n/k)`.
//! * **Natural breaks**: Fisher's (1958) exact optimal partition of the sorted
//!   values into *k* contiguous classes minimizing the total within-class sum
//!   of squared deviations (the criterion Jenks popularized), by dynamic
//!   programming in O(k·n²).
//!
//! When there are fewer distinct values than classes, the number of classes
//! is reduced so that no class is empty.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Classification method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Jenks natural breaks (Fisher's exact optimization).
    #[default]
    NaturalBreaks,
    /// Equal-count classes.
    Quantile,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Self::NaturalBreaks => "natural breaks (Jenks/Fisher)",
            Self::Quantile => "quantiles",
        }
    }
}

/// Upper bounds of each class (ascending; the last is the maximum).
pub fn breaks(values: &[f64], k: usize, method: Method) -> Vec<f64> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    v.sort_by(f64::total_cmp);
    if v.is_empty() {
        return Vec::new();
    }
    let mut distinct = v.clone();
    distinct.dedup();
    let k = k.clamp(1, distinct.len().max(1));
    if k == 1 {
        return vec![*v.last().expect("non-empty")];
    }
    let mut b = match method {
        Method::Quantile => quantile_breaks(&v, k),
        Method::NaturalBreaks => fisher_breaks(&v, k),
    };
    b.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);
    if let Some(last) = b.last_mut() {
        *last = *v.last().expect("non-empty");
    }
    b
}

fn quantile_breaks(sorted: &[f64], k: usize) -> Vec<f64> {
    let n = sorted.len();
    (1..=k)
        .map(|i| {
            let pos = (i * n).div_ceil(k).clamp(1, n);
            sorted[pos - 1]
        })
        .collect()
}

/// Fisher's exact optimal partition (sorted input).
fn fisher_breaks(sorted: &[f64], k: usize) -> Vec<f64> {
    let n = sorted.len();
    // Prefix sums for O(1) segment sums of squared deviations.
    let mut s1 = vec![0.0; n + 1];
    let mut s2 = vec![0.0; n + 1];
    for (i, x) in sorted.iter().enumerate() {
        s1[i + 1] = s1[i] + x;
        s2[i + 1] = s2[i] + x * x;
    }
    let ssd = |a: usize, b: usize| -> f64 {
        // values a..b (exclusive end)
        let m = (b - a) as f64;
        let s = s1[b] - s1[a];
        (s2[b] - s2[a]) - s * s / m
    };
    // cost[c][j]: best cost of the first j values in c+1 classes.
    let mut cost = vec![vec![f64::INFINITY; n + 1]; k];
    let mut cut = vec![vec![0usize; n + 1]; k];
    for j in 1..=n {
        cost[0][j] = ssd(0, j);
    }
    for c in 1..k {
        for j in (c + 1)..=n {
            let mut best = f64::INFINITY;
            let mut arg = c;
            for i in c..j {
                let v = cost[c - 1][i] + ssd(i, j);
                if v < best {
                    best = v;
                    arg = i;
                }
            }
            cost[c][j] = best;
            cut[c][j] = arg;
        }
    }
    // Walk back the cuts.
    let mut ends = vec![n; k];
    let mut j = n;
    for c in (1..k).rev() {
        let i = cut[c][j];
        ends[c - 1] = i;
        j = i;
    }
    ends.iter().map(|&e| sorted[e.max(1) - 1]).collect()
}

/// Class index (0-based) of a value given upper bounds.
pub fn class_of(value: f64, breaks: &[f64]) -> usize {
    breaks
        .iter()
        .position(|b| value <= *b + 1e-12)
        .unwrap_or(breaks.len().saturating_sub(1))
}

/// Goodness of variance fit (GVF): 1 − SDCM/SDAM. 1 is a perfect fit.
pub fn goodness_of_variance_fit(values: &[f64], breaks: &[f64]) -> f64 {
    let n = values.len() as f64;
    if n == 0.0 || breaks.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / n;
    let sdam: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    if sdam == 0.0 {
        return 1.0;
    }
    let mut groups: Vec<Vec<f64>> = vec![Vec::new(); breaks.len()];
    for v in values {
        groups[class_of(*v, breaks)].push(*v);
    }
    let sdcm: f64 = groups
        .iter()
        .filter(|g| !g.is_empty())
        .map(|g| {
            let m = g.iter().sum::<f64>() / g.len() as f64;
            g.iter().map(|v| (v - m).powi(2)).sum::<f64>()
        })
        .sum();
    1.0 - sdcm / sdam
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_split_evenly() {
        let v: Vec<f64> = (1..=10).map(f64::from).collect();
        assert_eq!(breaks(&v, 5, Method::Quantile), vec![2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(breaks(&v, 3, Method::Quantile), vec![4.0, 7.0, 10.0]);
    }

    #[test]
    fn natural_breaks_find_obvious_clusters() {
        let v = [1.0, 1.2, 1.1, 10.0, 10.5, 9.8, 20.0, 21.0, 19.5];
        assert_eq!(breaks(&v, 3, Method::NaturalBreaks), vec![1.2, 10.5, 21.0]);
        let b = breaks(&v, 3, Method::NaturalBreaks);
        assert!(goodness_of_variance_fit(&v, &b) > 0.99);
        assert_eq!(class_of(10.0, &b), 1);
        assert_eq!(class_of(0.5, &b), 0);
    }

    /// Golden test against R `classInt` (styles "jenks" and "fisher"); the
    /// fixture records exactly how the reference was produced.
    #[test]
    fn natural_breaks_golden() {
        let x = [
            1.0, 2.0, 4.0, 5.0, 7.0, 9.0, 10.0, 12.0, 13.0, 15.0, 17.0, 18.0, 19.0, 22.0, 24.0,
            25.0, 28.0, 29.0, 30.0,
        ];
        let b = breaks(&x, 5, Method::NaturalBreaks);
        let fixture: Vec<f64> = serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../tests/fixtures/phase4/fisher_breaks.json"
        ))
        .unwrap()["upper_bounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        assert_eq!(b, fixture);
    }

    #[test]
    fn too_few_distinct_values_reduce_classes() {
        let v = [3.0, 3.0, 3.0, 8.0];
        assert_eq!(breaks(&v, 5, Method::NaturalBreaks), vec![3.0, 8.0]);
        assert_eq!(breaks(&[4.0], 5, Method::Quantile), vec![4.0]);
    }
}
