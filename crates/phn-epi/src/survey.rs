//! Weighted proportions from a two-stage cluster sample (CASPER, EPR-11).
//!
//! Each respondent has a sampling weight and a cluster (primary sampling
//! unit). For a categorical answer with categories k:
//!
//! * weighted frequency Ŷₖ = Σ wᵢ yᵢₖ and proportion p̂ₖ = Ŷₖ / Σ wᵢ (a ratio
//!   estimator; respondents with a missing answer are left out);
//! * variance by Taylor series linearization with clusters as primary
//!   sampling units sampled with replacement in one stratum:
//!   zᵢ = wᵢ (yᵢₖ − p̂ₖ) / Σ w, Zₕ = Σᵢ∈ₕ zᵢ, var(p̂ₖ) = m/(m − 1) Σₕ (Zₕ − Z̄)²,
//!   where m is the number of clusters with at least one answer;
//! * confidence interval p̂ ± t(m − 1) · SE, clipped to 0–100%;
//! * design effect DEFF = var(p̂) / [p̂(1 − p̂)/n], the variance relative to a
//!   simple random sample of the same size n.
//!
//! References: K. M. Wolter, *Introduction to Variance Estimation*, 2nd ed.
//! (Springer, 2007), ch. 6 (Taylor series); CDC CASPER Toolkit, 3rd ed.
//! (2019), section 4.2 (weights; present cells with at least 5 observations).
//! Golden tests compare with the `samplics` Taylor-series estimator.

use serde::{Deserialize, Serialize};
use statrs::distribution::{ContinuousCDF, StudentsT};

/// One category's estimate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CategoryEstimate {
    pub category: String,
    /// Unweighted number of respondents.
    pub n: usize,
    /// Weighted frequency (estimated households).
    pub weighted_count: f64,
    /// Weighted percent.
    pub percent: f64,
    /// Standard error of the percent.
    pub se_percent: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
    /// Design effect relative to a simple random sample of the same size.
    pub design_effect: Option<f64>,
}

/// Estimates for one question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProportionTable {
    /// Respondents with an answer.
    pub n: usize,
    /// Clusters with at least one answer (primary sampling units).
    pub clusters: usize,
    /// Degrees of freedom for the intervals (clusters − 1).
    pub df: usize,
    pub sum_of_weights: f64,
    pub confidence: f64,
    pub categories: Vec<CategoryEstimate>,
}

/// Weighted proportions with linearized cluster variance.
///
/// `cluster[i]`, `weight[i]`, and `answer[i]` describe respondent i; `answer`
/// is an index into `labels` (None = missing, left out). Returns None when
/// fewer than two clusters answered or the weights are not positive.
pub fn weighted_proportions(
    cluster: &[usize],
    weight: &[f64],
    answer: &[Option<usize>],
    labels: &[String],
    confidence: f64,
) -> Option<ProportionTable> {
    let rows: Vec<(usize, f64, usize)> = cluster
        .iter()
        .zip(weight)
        .zip(answer)
        .filter_map(|((&c, &w), a)| a.map(|k| (c, w, k)))
        .filter(|(_, _, k)| *k < labels.len())
        .collect();
    if rows.iter().any(|(_, w, _)| !(w.is_finite() && *w > 0.0)) {
        return None;
    }
    let n = rows.len();
    let mut ids: Vec<usize> = rows.iter().map(|r| r.0).collect();
    ids.sort_unstable();
    ids.dedup();
    let m = ids.len();
    if m < 2 || n == 0 {
        return None;
    }
    let total_w: f64 = rows.iter().map(|r| r.1).sum();
    let df = m - 1;
    let t = StudentsT::new(0.0, 1.0, df as f64)
        .ok()?
        .inverse_cdf(1.0 - (1.0 - confidence) / 2.0);
    let mut categories = Vec::with_capacity(labels.len());
    for (k, label) in labels.iter().enumerate() {
        let count = rows.iter().filter(|r| r.2 == k).count();
        let yk: f64 = rows.iter().filter(|r| r.2 == k).map(|r| r.1).sum();
        let p = yk / total_w;
        let mut z = vec![0.0; m];
        for &(c, w, a) in &rows {
            let y = if a == k { 1.0 } else { 0.0 };
            let idx = ids.binary_search(&c).expect("cluster id present");
            z[idx] += w * (y - p) / total_w;
        }
        let zbar = z.iter().sum::<f64>() / m as f64;
        let var = m as f64 / (m as f64 - 1.0) * z.iter().map(|v| (v - zbar).powi(2)).sum::<f64>();
        let se = var.sqrt();
        let srs = p * (1.0 - p) / n as f64;
        categories.push(CategoryEstimate {
            category: label.clone(),
            n: count,
            weighted_count: yk,
            percent: 100.0 * p,
            se_percent: 100.0 * se,
            ci_lower: (100.0 * (p - t * se)).max(0.0),
            ci_upper: (100.0 * (p + t * se)).min(100.0),
            design_effect: (srs > 0.0).then(|| var / srs),
        });
    }
    Some(ProportionTable {
        n,
        clusters: m,
        df,
        sum_of_weights: total_w,
        confidence,
        categories,
    })
}

/// CASPER household weight: households in the sampling frame divided by
/// (interviews completed in the cluster × clusters selected). For a cluster
/// selected more than once, `completed` is the interviews divided by the
/// number of times it was selected (CDC CASPER Toolkit, 3rd ed., sec. 4.2).
pub fn casper_weight(frame_households: f64, completed_in_cluster: f64, clusters_selected: f64) -> f64 {
    frame_households / (completed_in_cluster * clusters_selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolkit_weight_example() {
        // Toolkit Figure 4: 43,214 households, 30 clusters; a cluster with 7
        // interviews has weight 43214 / (7 × 30) ≈ 205.78.
        assert!((casper_weight(43_214.0, 7.0, 30.0) - 205.780_952).abs() < 1e-5);
        assert!((casper_weight(43_214.0, 5.0, 30.0) - 288.093_333).abs() < 1e-5);
    }

    #[test]
    fn equal_weights_and_one_answer_per_cluster_reduce_to_srs_with_replacement() {
        // One respondent per cluster: Taylor variance = n/(n−1) Σ (y − p)²/n² = p(1−p)/(n−1).
        let n = 40;
        let cluster: Vec<usize> = (0..n).collect();
        let weight = vec![10.0; n];
        let answer: Vec<Option<usize>> = (0..n).map(|i| Some(usize::from(i < 10))).collect();
        let labels = vec!["no".to_string(), "yes".to_string()];
        let t = weighted_proportions(&cluster, &weight, &answer, &labels, 0.95).unwrap();
        let yes = &t.categories[1];
        assert!((yes.percent - 25.0).abs() < 1e-12);
        let expect = (0.25f64 * 0.75 / 39.0).sqrt() * 100.0;
        assert!((yes.se_percent - expect).abs() < 1e-10);
        assert!((yes.weighted_count - 100.0).abs() < 1e-12);
        assert!((yes.design_effect.unwrap() - 40.0 / 39.0).abs() < 1e-10);
    }

    #[test]
    fn missing_answers_and_too_few_clusters() {
        let labels = vec!["a".to_string()];
        assert!(weighted_proportions(&[1, 1], &[1.0, 1.0], &[Some(0), None], &labels, 0.95).is_none());
        assert!(
            weighted_proportions(&[1, 2], &[1.0, -1.0], &[Some(0), Some(0)], &labels, 0.95)
                .is_none()
        );
    }
}
