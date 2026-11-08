//! EQ-04 `measure_health_disparities`.
//!
//! Measures implemented here (the shared epi engine does not provide them yet):
//!
//! * **Rate ratio** vs a reference group, with the exact conditional CI (the
//!   count in the group, given both groups' total, is binomial; Clopper–Pearson
//!   limits transformed to the ratio scale, as R `poisson.test`) and the
//!   log-scale (Wald) CI `exp(ln RR ± z·√(1/a + 1/b))`.
//! * **Rate difference** with the Poisson variance `a/n₁² + b/n₀²`.
//! * **Index of disparity** (Pearcy JN, Keppel KG. A summary measure of health
//!   disparity. *Public Health Rep* 2002;117:273–280): the mean absolute
//!   difference of the other groups' rates from the reference rate, as a
//!   percentage of the reference rate.
//! * **Between-group variance** (Harper S, et al. An overview of methods for
//!   monitoring social disparities in cancer. *Am J Epidemiol* 2008; NCI
//!   HD*Calc): `Σ pⱼ (rⱼ − μ)²` with population shares `pⱼ` and the total
//!   rate `μ`; SE by the first-order delta method.
//! * **Population attributable fraction**: excess events if every group had
//!   the reference rate, `(O − r_ref·N) / O`, with a CI on `ln(1 − PAF)`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::rates::{self, Rate, RateFlag};
use phn_local::phi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use statrs::distribution::{Beta, ContinuousCDF, Normal};

use crate::context::Context;
use crate::md;

/// One group.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DisparityGroupInput {
    /// Group label, e.g. "Hispanic or Latino" or "Springfield".
    pub label: String,
    /// Number of events.
    pub count: u64,
    /// Population or person-time.
    pub population: f64,
}

/// How to choose the reference group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceChoice {
    /// The most favorable rate (lowest for adverse outcomes, highest for favorable ones). Default.
    #[default]
    MostFavorable,
    /// The largest group.
    Largest,
    /// A named group (give `reference_label`).
    Named,
}

/// Arguments for `measure_health_disparities`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DisparityArgs {
    /// Groups with counts and populations (2–50 groups). Give either `groups` or `path`.
    #[serde(default)]
    pub groups: Option<Vec<DisparityGroupInput>>,
    /// An aggregated CSV/XLSX table in /data with one row per group.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sheet: Option<String>,
    /// Column with group labels (default: first column).
    #[serde(default)]
    pub group_column: Option<String>,
    /// Column with event counts (default: a column named count/cases/events/deaths).
    #[serde(default)]
    pub count_column: Option<String>,
    /// Column with populations (default: a column named population/pop/denominator).
    #[serde(default)]
    pub population_column: Option<String>,
    /// most_favorable (default), largest, or named.
    #[serde(default)]
    pub reference: Option<ReferenceChoice>,
    /// The reference group's label when `reference` is "named".
    #[serde(default)]
    pub reference_label: Option<String>,
    /// True when higher rates are better (screening, vaccination). Default false (adverse outcome).
    #[serde(default)]
    pub favorable_outcome: Option<bool>,
    /// Rate base (default 100000).
    #[serde(default)]
    pub per: Option<f64>,
    /// Confidence level, 0.80–0.99 (default 0.95).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// What was counted, e.g. "asthma emergency visits, 2023".
    #[serde(default)]
    pub event_description: Option<String>,
}

/// A comparison with the reference group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Comparison {
    pub label: String,
    pub rate_ratio: Option<f64>,
    /// Exact conditional (binomial) CI.
    pub rate_ratio_exact_ci: Option<(f64, f64)>,
    /// Log-scale (Wald) CI.
    pub rate_ratio_log_ci: Option<(f64, f64)>,
    /// Rate difference (per `per`).
    pub rate_difference: f64,
    pub rate_difference_ci: (f64, f64),
    /// Events above (positive) or below the reference rate.
    pub excess_events: f64,
}

/// Result of `measure_health_disparities`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DisparityResult {
    pub per: f64,
    pub confidence: f64,
    pub reference_group: String,
    pub favorable_outcome: bool,
    /// Rates with exact Poisson CIs (suppressed groups from local files are omitted).
    pub rates: Vec<Rate>,
    pub comparisons: Vec<Comparison>,
    /// Index of disparity (%).
    pub index_of_disparity: Option<f64>,
    /// Between-group variance (squared rate units).
    pub between_group_variance: f64,
    pub between_group_variance_se: f64,
    /// Total (population-weighted) rate.
    pub total_rate: f64,
    /// Events above what the reference rate would give (adverse outcomes).
    pub excess_events: f64,
    /// Population attributable fraction (0–1) with CI.
    pub attributable_fraction: Option<f64>,
    pub attributable_fraction_ci: Option<(f64, f64)>,
    /// Groups left out because their counts were below the suppression threshold.
    pub suppressed_groups: usize,
    pub methods: Vec<String>,
}

/// Exact conditional CI for the rate ratio (a/n1)/(b/n0).
pub fn rate_ratio_exact_ci(a: u64, n1: f64, b: u64, n0: f64, confidence: f64) -> (f64, f64) {
    let total = a + b;
    let alpha = 1.0 - confidence;
    let (af, tf) = (a as f64, total as f64);
    let p_lo = if a == 0 {
        0.0
    } else {
        Beta::new(af, tf - af + 1.0)
            .map(|d| d.inverse_cdf(alpha / 2.0))
            .unwrap_or(0.0)
    };
    let p_hi = if a == total {
        1.0
    } else {
        Beta::new(af + 1.0, tf - af)
            .map(|d| d.inverse_cdf(1.0 - alpha / 2.0))
            .unwrap_or(1.0)
    };
    let to_rr = |p: f64| {
        if p >= 1.0 {
            f64::INFINITY
        } else {
            p / (1.0 - p) * n0 / n1
        }
    };
    (to_rr(p_lo), to_rr(p_hi))
}

fn z(confidence: f64) -> f64 {
    Normal::new(0.0, 1.0)
        .expect("standard normal")
        .inverse_cdf(1.0 - (1.0 - confidence) / 2.0)
}

#[derive(Debug, Clone)]
struct G {
    label: String,
    a: u64,
    n: f64,
}

/// Core computation (rates in units of `per`).
fn compute(
    groups: &[G],
    reference: usize,
    per: f64,
    confidence: f64,
    threshold: u32,
    favorable: bool,
) -> DisparityResult {
    let zc = z(confidence);
    let r: Vec<f64> = groups.iter().map(|g| g.a as f64 / g.n).collect();
    let rr = r[reference];
    let refg = &groups[reference];
    let rates_out: Vec<Rate> = groups
        .iter()
        .map(|g| rates::rate(&g.label, g.a, g.n, per, confidence, threshold))
        .collect();
    let mut comparisons = Vec::new();
    for (j, g) in groups.iter().enumerate() {
        if j == reference {
            continue;
        }
        let ratio = (rr > 0.0).then(|| r[j] / rr);
        let exact = (refg.a > 0 || g.a > 0)
            .then(|| rate_ratio_exact_ci(g.a, g.n, refg.a, refg.n, confidence))
            .filter(|_| refg.a > 0);
        let log_ci = (g.a > 0 && refg.a > 0).then(|| {
            let se = (1.0 / g.a as f64 + 1.0 / refg.a as f64).sqrt();
            let l = (r[j] / rr).ln();
            ((l - zc * se).exp(), (l + zc * se).exp())
        });
        let rd = (r[j] - rr) * per;
        let rd_se = (g.a as f64 / g.n.powi(2) + refg.a as f64 / refg.n.powi(2)).sqrt() * per;
        comparisons.push(Comparison {
            label: g.label.clone(),
            rate_ratio: ratio,
            rate_ratio_exact_ci: exact,
            rate_ratio_log_ci: log_ci,
            rate_difference: rd,
            rate_difference_ci: (rd - zc * rd_se, rd + zc * rd_se),
            excess_events: g.a as f64 - rr * g.n,
        });
    }
    let others: Vec<f64> = (0..groups.len())
        .filter(|j| *j != reference)
        .map(|j| (r[j] - rr).abs())
        .collect();
    let index_of_disparity = (rr > 0.0 && !others.is_empty())
        .then(|| others.iter().sum::<f64>() / others.len() as f64 / rr * 100.0);
    let total_n: f64 = groups.iter().map(|g| g.n).sum();
    let total_a: u64 = groups.iter().map(|g| g.a).sum();
    let p: Vec<f64> = groups.iter().map(|g| g.n / total_n).collect();
    let rp: Vec<f64> = r.iter().map(|x| x * per).collect();
    let mu: f64 = p.iter().zip(&rp).map(|(p, r)| p * r).sum();
    let bgv: f64 = p.iter().zip(&rp).map(|(p, r)| p * (r - mu).powi(2)).sum();
    let bgv_var: f64 = groups
        .iter()
        .enumerate()
        .map(|(j, g)| {
            let var_r = g.a as f64 / g.n.powi(2) * per * per;
            (2.0 * p[j] * (rp[j] - mu)).powi(2) * var_r
        })
        .sum();
    let o = total_a as f64;
    let expected = rr * total_n;
    let excess = o - expected;
    let (paf, paf_ci) = if o > 0.0 && refg.a > 0 && !favorable {
        let paf = excess / o;
        let b = o - refg.a as f64;
        let v = b / (refg.a as f64 * o);
        let l = (1.0 - paf).ln();
        (
            Some(paf),
            Some((1.0 - (l + zc * v.sqrt()).exp(), 1.0 - (l - zc * v.sqrt()).exp())),
        )
    } else {
        (None, None)
    };
    DisparityResult {
        per,
        confidence,
        reference_group: refg.label.clone(),
        favorable_outcome: favorable,
        rates: rates_out,
        comparisons,
        index_of_disparity,
        between_group_variance: bgv,
        between_group_variance_se: bgv_var.sqrt(),
        total_rate: mu,
        excess_events: excess,
        attributable_fraction: paf,
        attributable_fraction_ci: paf_ci,
        suppressed_groups: 0,
        methods: vec![
            "Rate ratio: exact conditional CI (binomial, Clopper–Pearson; as R poisson.test) and log-scale Wald CI exp(ln RR ± z√(1/a+1/b)).".into(),
            "Rate difference: (r₁ − r₀) ± z√(a/n₁² + b/n₀²), times the rate base.".into(),
            "Index of disparity (Pearcy & Keppel 2002): mean |rⱼ − r_ref| over the non-reference groups ÷ r_ref × 100.".into(),
            "Between-group variance (Harper et al.; NCI HD*Calc): Σ pⱼ(rⱼ − μ)², pⱼ = population share, μ = total rate; SE by the first-order delta method √Σ[2pⱼ(rⱼ − μ)]²·Var(rⱼ).".into(),
            "Population attributable fraction: (O − r_ref·N)/O, the share of events that would not occur if every group had the reference rate; CI on ln(1 − PAF) with variance B/(a_ref·O).".into(),
        ],
    }
}

fn find_col(headers: &[String], given: &Option<String>, names: &[&str], what: &str) -> Result<usize> {
    if let Some(g) = given {
        return headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(g.trim()))
            .ok_or_else(|| {
                PhnError::InvalidInput(format!(
                    "{what} column '{g}' not found; columns are: {}",
                    headers.join(", ")
                ))
            });
    }
    headers
        .iter()
        .position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
        .ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "could not find the {what} column; name it with {what}_column (columns: {})",
                headers.join(", ")
            ))
        })
}

fn load_groups(ctx: &Context, args: &DisparityArgs) -> Result<(Vec<G>, String)> {
    let path = args.path.as_deref().unwrap_or_default();
    let (table, label) = crate::core::files::load_table(ctx, path, args.sheet.as_deref())?;
    if table.rows.len() > 200 {
        return Err(PhnError::Privacy(
            "this looks like record-level data (over 200 rows). Give one row per group with a \
             count and a population; aggregate first."
                .into(),
        ));
    }
    let direct: Vec<String> = phi::scan_table(&table)
        .into_iter()
        .filter(|f| f.kind.is_direct() && f.kind != phi::Identifier::FreeText)
        .map(|f| format!("{} ({})", f.column, f.kind.label()))
        .collect();
    if !direct.is_empty() {
        return Err(PhnError::Privacy(format!(
            "this looks like record-level data with identifiers ({}). Give one row per group \
             with a count and a population.",
            direct.join(", ")
        )));
    }
    let gcol = match &args.group_column {
        Some(_) => find_col(&table.headers, &args.group_column, &[], "group")?,
        None => 0,
    };
    let ccol = find_col(
        &table.headers,
        &args.count_column,
        &["count", "counts", "cases", "events", "deaths", "n", "numerator"],
        "count",
    )?;
    let pcol = find_col(
        &table.headers,
        &args.population_column,
        &["population", "pop", "denominator", "person-years", "person years", "persons"],
        "population",
    )?;
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (i, r) in table.rows.iter().enumerate() {
        let label = r.get(gcol).cloned().unwrap_or_default();
        let l = label.trim().to_ascii_lowercase();
        if l.is_empty() || l == "total" || l == "all" {
            continue;
        }
        if !seen.insert(l.clone()) {
            return Err(PhnError::Privacy(format!(
                "group '{label}' appears more than once; this looks like record-level data. Give \
                 one row per group."
            )));
        }
        let num = |c: usize| -> Option<f64> {
            r.get(c)
                .and_then(|v| v.trim().replace(',', "").parse::<f64>().ok())
        };
        let (Some(a), Some(n)) = (num(ccol), num(pcol)) else {
            return Err(PhnError::InvalidInput(format!(
                "row {}: count and population must be numbers",
                i + 2
            )));
        };
        if a < 0.0 || a.fract() != 0.0 {
            return Err(PhnError::InvalidInput(format!(
                "row {}: counts must be whole, non-negative numbers",
                i + 2
            )));
        }
        out.push(G {
            label,
            a: a as u64,
            n,
        });
    }
    Ok((out, label))
}

/// Disparity measures across groups.
pub fn measure_health_disparities(
    ctx: &Context,
    args: DisparityArgs,
) -> Result<Envelope<DisparityResult>> {
    let per = args.per.unwrap_or(100_000.0);
    if !(per.is_finite() && per > 0.0) {
        return Err(PhnError::InvalidInput("per must be a positive number".into()));
    }
    let confidence = args.confidence.unwrap_or(0.95);
    if !(0.80..=0.99).contains(&confidence) {
        return Err(PhnError::InvalidInput(
            "confidence must be between 0.80 and 0.99".into(),
        ));
    }
    let threshold = ctx.suppression_threshold();
    let (mut groups, local) = match (&args.groups, &args.path) {
        (Some(g), None) => (
            g.iter()
                .map(|x| G {
                    label: x.label.clone(),
                    a: x.count,
                    n: x.population,
                })
                .collect::<Vec<_>>(),
            None,
        ),
        (None, Some(_)) => {
            let (g, l) = load_groups(ctx, &args)?;
            (g, Some(l))
        }
        _ => {
            return Err(PhnError::InvalidInput(
                "give either `groups` (inline) or `path` (an aggregated file in /data)".into(),
            ));
        }
    };
    for g in &groups {
        if !(g.n.is_finite() && g.n > 0.0) {
            return Err(PhnError::InvalidInput(format!(
                "population for '{}' must be a positive number",
                g.label
            )));
        }
        if g.a as f64 > g.n {
            return Err(PhnError::InvalidInput(format!(
                "the count for '{}' exceeds its population",
                g.label
            )));
        }
    }
    // Local files: groups with small counts are left out entirely, so their
    // counts cannot be recovered from any measure in the response.
    let mut suppressed = Vec::new();
    if local.is_some() {
        let (keep, drop): (Vec<G>, Vec<G>) = groups
            .into_iter()
            .partition(|g| g.a == 0 || g.a >= u64::from(threshold));
        groups = keep;
        suppressed = drop;
    }
    if groups.len() < 2 {
        return Err(PhnError::InvalidInput(format!(
            "need at least two groups{}",
            if suppressed.is_empty() {
                String::new()
            } else {
                format!(
                    " after leaving out {} group(s) with counts below the suppression threshold",
                    suppressed.len()
                )
            }
        )));
    }
    if groups.len() > 50 {
        return Err(PhnError::InvalidInput("at most 50 groups".into()));
    }
    let favorable = args.favorable_outcome.unwrap_or(false);
    let reference = match args.reference.unwrap_or_default() {
        ReferenceChoice::MostFavorable => {
            let rates: Vec<f64> = groups.iter().map(|g| g.a as f64 / g.n).collect();
            let mut idx: Vec<usize> = (0..groups.len()).collect();
            idx.sort_by(|&i, &j| {
                let o = rates[i].total_cmp(&rates[j]);
                if favorable { o.reverse() } else { o }
            });
            // A zero rate cannot be a ratio denominator; prefer the most
            // favorable non-zero rate.
            idx.iter()
                .copied()
                .find(|&i| groups[i].a > 0)
                .unwrap_or(idx[0])
        }
        ReferenceChoice::Largest => (0..groups.len())
            .max_by(|&i, &j| groups[i].n.total_cmp(&groups[j].n))
            .unwrap_or(0),
        ReferenceChoice::Named => {
            let name = args.reference_label.as_deref().ok_or_else(|| {
                PhnError::InvalidInput("give reference_label when reference is 'named'".into())
            })?;
            groups
                .iter()
                .position(|g| g.label.trim().eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| {
                    if suppressed.iter().any(|g| g.label.eq_ignore_ascii_case(name.trim())) {
                        PhnError::InvalidInput(format!(
                            "the reference group '{name}' has a count below the suppression \
                             threshold; choose another reference"
                        ))
                    } else {
                        PhnError::InvalidInput(format!("no group is named '{name}'"))
                    }
                })?
        }
    };
    if groups[reference].a == 0 {
        return Err(PhnError::InvalidInput(format!(
            "the reference group '{}' has no events, so ratios cannot be computed; choose another \
             reference",
            groups[reference].label
        )));
    }
    let mut result = compute(&groups, reference, per, confidence, threshold, favorable);
    result.suppressed_groups = suppressed.len();

    // Summary.
    let max_rate = result.rates.iter().map(|r| r.upper).fold(0.0, f64::max);
    let dec = rates::display_decimals(max_rate) as usize;
    let pct = (confidence * 100.0).round();
    let per_label = md::num(per, 0);
    let mut summary = format!(
        "**Health disparities{}** — rates per {per_label}; reference group: **{}** ({} rate)\n\n",
        args.event_description
            .as_ref()
            .map(|d| format!(": {d}"))
            .unwrap_or_default(),
        result.reference_group,
        if args.reference.unwrap_or_default() == ReferenceChoice::MostFavorable {
            "most favorable"
        } else {
            "chosen"
        }
    );
    let fmt_ci = |c: Option<(f64, f64)>| match c {
        Some((l, u)) if u.is_finite() => format!("{:.2} – {:.2}", l, u),
        Some((l, _)) => format!("{l:.2} – ∞"),
        None => "—".into(),
    };
    let mut rows = Vec::new();
    for r in &result.rates {
        let cmp = result.comparisons.iter().find(|c| c.label == r.label);
        let mut notes = Vec::new();
        if r.flags.contains(&RateFlag::Unreliable) {
            notes.push("unreliable (<20 events)");
        }
        if r.flags.contains(&RateFlag::BelowSuppressionThreshold) {
            notes.push("suppress before sharing");
        }
        rows.push(vec![
            r.label.clone(),
            md::num(r.count as f64, 0),
            md::num(r.denominator, 0),
            format!(
                "{} ({} – {})",
                md::num(r.rate, dec),
                md::num(r.lower, dec),
                md::num(r.upper, dec)
            ),
            match cmp {
                Some(c) => format!(
                    "{} ({})",
                    c.rate_ratio.map(|x| format!("{x:.2}")).unwrap_or("—".into()),
                    fmt_ci(c.rate_ratio_exact_ci)
                ),
                None => "1 (reference)".into(),
            },
            match cmp {
                Some(c) => format!(
                    "{} ({} – {})",
                    md::num(c.rate_difference, dec),
                    md::num(c.rate_difference_ci.0, dec),
                    md::num(c.rate_difference_ci.1, dec)
                ),
                None => "0".into(),
            },
            notes.join("; "),
        ]);
    }
    summary.push_str(&md::table(
        &[
            "Group",
            "Events",
            "Population",
            &format!("Rate ({pct}% CI)"),
            &format!("Rate ratio (exact {pct}% CI)"),
            &format!("Rate difference ({pct}% CI)"),
            "Notes",
        ],
        &rows,
    ));
    summary.push_str("\n**Summary measures**\n\n");
    let mut srows = vec![vec![
        "Total rate".to_string(),
        md::num(result.total_rate, dec),
    ]];
    if let Some(id) = result.index_of_disparity {
        srows.push(vec!["Index of disparity".into(), format!("{id:.1}%")]);
    }
    srows.push(vec![
        "Between-group variance".into(),
        format!(
            "{:.1} (SE {:.1})",
            result.between_group_variance, result.between_group_variance_se
        ),
    ]);
    if let (Some(p), Some((l, u))) = (result.attributable_fraction, result.attributable_fraction_ci) {
        srows.push(vec![
            "Excess events if every group had the reference rate".into(),
            md::num(result.excess_events, 1),
        ]);
        srows.push(vec![
            "Population attributable fraction".into(),
            format!("{:.1}% ({pct}% CI {:.1}% – {:.1}%)", p * 100.0, l * 100.0, u * 100.0),
        ]);
    }
    summary.push_str(&md::table(&["Measure", "Value"], &srows));

    let mut caveats = Vec::new();
    let unreliable: Vec<&str> = result
        .rates
        .iter()
        .filter(|r| r.flags.contains(&RateFlag::Unreliable))
        .map(|r| r.label.as_str())
        .collect();
    if !unreliable.is_empty() {
        caveats.push(format!(
            "Rates based on fewer than 20 events are unreliable: {}. Ratios and differences \
             involving them have wide intervals.",
            unreliable.join(", ")
        ));
    }
    let small: Vec<&str> = result
        .rates
        .iter()
        .filter(|r| r.flags.contains(&RateFlag::BelowSuppressionThreshold))
        .map(|r| r.label.as_str())
        .collect();
    if !small.is_empty() {
        caveats.push(format!(
            "Counts below your suppression threshold ({threshold}) must be suppressed before \
             sharing, with any rate or ratio computed from them: {}.",
            small.join(", ")
        ));
    }
    if !suppressed.is_empty() {
        caveats.push(format!(
            "{} group(s) from your file had counts from 1 to {} and were left out of every \
             measure so their counts cannot be recovered; combine years or groups to include \
             them.",
            suppressed.len(),
            threshold - 1
        ));
    }
    if favorable {
        caveats.push(
            "For a favorable outcome (higher is better) the reference is the highest rate; the \
             attributable fraction is not reported."
                .into(),
        );
    }
    caveats.push(
        "Crude rates are not age-adjusted; differences in age structure between groups can \
         create or hide disparities. Age-adjust first when groups differ in age."
            .into(),
    );
    caveats.push(
        "The index of disparity depends on the reference group and on the direction of the \
         outcome; it has no standard confidence interval. The between-group variance SE uses a \
         first-order approximation that tends to be too small when groups have few events."
            .into(),
    );
    let mut sources = vec![
        Source::computed(
            "Pearcy JN, Keppel KG. A summary measure of health disparity. Public Health Rep 2002;117:273-280",
            "PHN MCP server calculation",
        ),
        Source::computed(
            "Harper S, Lynch J, et al. An overview of methods for monitoring social disparities in cancer with an example using trends in lung cancer incidence by area-socioeconomic position and race-ethnicity, 1992-2004. Am J Epidemiol 2008;167:889-899 (between-group variance; NCI HD*Calc)",
            "PHN MCP server calculation",
        ),
        Source::computed(
            "Exact Poisson limits for rates (Garwood 1936) and the conditional binomial interval for rate ratios",
            "PHN MCP server calculation",
        ),
    ];
    let mut privacy = Privacy {
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    };
    if let Some(l) = local {
        sources.push(Source::local_file(l));
        privacy.suppression_applied = true;
    }
    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn golden() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/phase4/disparity_golden.json")).unwrap()
    }

    fn groups() -> Vec<G> {
        [("A", 45, 120_000.0), ("B", 30, 50_000.0), ("C", 12, 15_000.0), ("D", 20, 80_000.0)]
            .into_iter()
            .map(|(l, a, n)| G {
                label: l.into(),
                a,
                n,
            })
            .collect()
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * b.abs().max(1e-12)
    }

    /// Golden test against R (fixture note says exactly how values were produced).
    #[test]
    fn matches_r_reference_values() {
        let g = golden();
        let r = compute(&groups(), 3, 100_000.0, 0.95, 5, false);
        for c in &r.comparisons {
            let e = &g["comparisons"][&c.label];
            let f = |k: &str| e[k].as_f64().unwrap();
            assert!(close(c.rate_ratio.unwrap(), f("rate_ratio"), 1e-9), "{}", c.label);
            let (lo, hi) = c.rate_ratio_exact_ci.unwrap();
            assert!(close(lo, f("exact_lower"), 1e-6), "{} {lo}", c.label);
            assert!(close(hi, f("exact_upper"), 1e-6), "{} {hi}", c.label);
            let (lo, hi) = c.rate_ratio_log_ci.unwrap();
            assert!(close(lo, f("log_lower"), 1e-6) && close(hi, f("log_upper"), 1e-6));
            assert!(close(c.rate_difference, f("rate_difference"), 1e-9));
            assert!(close(c.rate_difference_ci.0, f("rd_lower"), 1e-6));
            assert!(close(c.rate_difference_ci.1, f("rd_upper"), 1e-6));
        }
        let gf = |k: &str| g[k].as_f64().unwrap();
        assert!(close(r.index_of_disparity.unwrap(), gf("index_of_disparity"), 1e-9));
        assert!(close(r.between_group_variance, gf("between_group_variance"), 1e-9));
        assert!(close(r.between_group_variance_se, gf("bgv_se_delta"), 1e-9));
        // The delta-method SE is within 15% of the Monte Carlo SD.
        assert!(close(r.between_group_variance_se, gf("bgv_sd_monte_carlo"), 0.15));
        assert!(close(r.total_rate, gf("total_rate"), 1e-9));
        assert!(close(r.excess_events, gf("excess_cases"), 1e-9));
        assert!(close(r.attributable_fraction.unwrap(), gf("paf"), 1e-9));
        let (lo, hi) = r.attributable_fraction_ci.unwrap();
        assert!(close(lo, gf("paf_lower"), 1e-9) && close(hi, gf("paf_upper"), 1e-9));
    }

    #[test]
    fn reference_choice_and_validation() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let mk = |reference, label: Option<&str>| DisparityArgs {
            groups: Some(
                groups()
                    .into_iter()
                    .map(|g| DisparityGroupInput {
                        label: g.label,
                        count: g.a,
                        population: g.n,
                    })
                    .collect(),
            ),
            path: None,
            sheet: None,
            group_column: None,
            count_column: None,
            population_column: None,
            reference: Some(reference),
            reference_label: label.map(str::to_string),
            favorable_outcome: None,
            per: None,
            confidence: None,
            event_description: None,
        };
        let env = measure_health_disparities(&ctx, mk(ReferenceChoice::MostFavorable, None)).unwrap();
        assert_eq!(env.data.reference_group, "D");
        assert!(env.summary.contains("Index of disparity"));
        let env = measure_health_disparities(&ctx, mk(ReferenceChoice::Largest, None)).unwrap();
        assert_eq!(env.data.reference_group, "A");
        let err = measure_health_disparities(&ctx, mk(ReferenceChoice::Named, Some("Z")))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no group"));
    }
}
