//! EPI-04 `calculate_sample_size`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::sample_size as ss;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Kind of study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StudyType {
    /// Survey estimating a proportion (e.g. vaccination coverage) within a margin of error.
    SurveyProportion,
    /// Survey estimating a mean (e.g. average screen time) within a margin of error.
    SurveyMean,
    /// Compare two proportions (cohort, trial, before/after with independent samples).
    TwoProportions,
    /// Case-control study: controls' exposure prevalence and the odds ratio to detect.
    CaseControl,
    /// Compare two means.
    TwoMeans,
    /// Power of a planned two-proportion comparison with given group sizes.
    PowerTwoProportions,
    /// Power of a planned two-mean comparison with given group sizes.
    PowerTwoMeans,
}

/// Arguments for `calculate_sample_size`. Proportions may be given as 0–1 or as percentages (e.g. 35).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SampleSizeArgs {
    pub study_type: StudyType,
    /// Surveys: expected proportion (use 0.5 when unknown — the most conservative).
    #[serde(default)]
    pub expected_proportion: Option<f64>,
    /// Surveys: margin of error (half-width of the confidence interval), e.g. 0.05 or 5.
    #[serde(default)]
    pub margin_of_error: Option<f64>,
    /// Surveys: size of the population sampled (applies the finite population correction).
    #[serde(default)]
    pub population_size: Option<f64>,
    /// Two proportions: proportion with the outcome in group 1 (exposed/intervention).
    #[serde(default)]
    pub p1: Option<f64>,
    /// Two proportions: proportion in group 2 (unexposed/comparison).
    #[serde(default)]
    pub p2: Option<f64>,
    /// Two proportions alternative: relative risk to detect (with p2).
    #[serde(default)]
    pub relative_risk: Option<f64>,
    /// Case-control: odds ratio to detect.
    #[serde(default)]
    pub odds_ratio: Option<f64>,
    /// Case-control: proportion of controls exposed.
    #[serde(default)]
    pub control_exposure: Option<f64>,
    /// Means: difference to detect and standard deviation (same units).
    #[serde(default)]
    pub difference: Option<f64>,
    #[serde(default)]
    pub standard_deviation: Option<f64>,
    /// Power calculations: planned size of group 1 and group 2.
    #[serde(default)]
    pub n1: Option<f64>,
    #[serde(default)]
    pub n2: Option<f64>,
    /// Confidence level for surveys (default 0.95).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Significance level for comparisons (default 0.05, two-sided).
    #[serde(default)]
    pub alpha: Option<f64>,
    /// Power for comparisons (default 0.80).
    #[serde(default)]
    pub power: Option<f64>,
    /// Group 2 size ÷ group 1 size (controls per case), default 1.
    #[serde(default)]
    pub ratio: Option<f64>,
    /// One-sided test (default false).
    #[serde(default)]
    pub one_sided: Option<bool>,
    /// Apply Fleiss's continuity correction to two-proportion sizes (default true).
    #[serde(default)]
    pub continuity_correction: Option<bool>,
    /// Design effect for cluster or complex samples (default 1).
    #[serde(default)]
    pub design_effect: Option<f64>,
    /// Cluster sampling: people per cluster and intraclass correlation (computes the design effect).
    #[serde(default)]
    pub cluster_size: Option<f64>,
    #[serde(default)]
    pub icc: Option<f64>,
    /// Expected response (or retention) rate, e.g. 0.7 or 70 (default 1).
    #[serde(default)]
    pub response_rate: Option<f64>,
}

/// Result of `calculate_sample_size`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SampleSizeResult {
    pub study_type: StudyType,
    /// Group 1 (or the whole survey).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group1: Option<ss::Rounded>,
    /// Group 2 (comparisons only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group2: Option<ss::Rounded>,
    /// Total to recruit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_to_recruit: Option<u64>,
    /// Power (power calculations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power: Option<f64>,
    /// Design effect applied.
    pub design_effect: f64,
    /// Clusters needed (cluster sampling).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clusters: Option<u64>,
    pub assumptions: Vec<String>,
    pub formula: String,
}

/// Accept 0–1 or a percentage.
fn prop(v: Option<f64>, name: &str) -> Result<f64> {
    let v =
        v.ok_or_else(|| PhnError::InvalidInput(format!("{name} is required for this study type")))?;
    let p = if v > 1.0 && v <= 100.0 { v / 100.0 } else { v };
    if !(p > 0.0 && p < 1.0) {
        return Err(PhnError::InvalidInput(format!(
            "{name} must be between 0 and 1 (or 0–100%)"
        )));
    }
    Ok(p)
}

fn need(v: Option<f64>, name: &str) -> Result<f64> {
    v.filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "{name} must be a positive number for this study type"
        ))
    })
}

/// Sample size and power calculations.
pub fn calculate_sample_size(
    _ctx: &Context,
    args: SampleSizeArgs,
) -> Result<Envelope<SampleSizeResult>> {
    let confidence = args.confidence.unwrap_or(0.95);
    let alpha = args.alpha.unwrap_or(0.05);
    let power = args
        .power
        .map(|p| if p > 1.0 { p / 100.0 } else { p })
        .unwrap_or(0.80);
    if !(0.5..=0.999).contains(&confidence)
        || !(0.0001..=0.2).contains(&alpha)
        || !(0.5..0.9999).contains(&power)
    {
        return Err(PhnError::InvalidInput(
            "use confidence 0.50–0.999, alpha 0.0001–0.20, and power 0.50–0.9999".into(),
        ));
    }
    let ratio = args.ratio.unwrap_or(1.0);
    if !(ratio > 0.0 && ratio <= 20.0) {
        return Err(PhnError::InvalidInput(
            "ratio must be between 0 and 20".into(),
        ));
    }
    let two_sided = !args.one_sided.unwrap_or(false);
    let cc = args.continuity_correction.unwrap_or(true);
    let response = match args.response_rate {
        Some(r) if r > 1.0 && r <= 100.0 => r / 100.0,
        Some(r) if r > 0.0 && r <= 1.0 => r,
        Some(_) => {
            return Err(PhnError::InvalidInput(
                "response_rate must be between 0 and 1 (or 0–100%)".into(),
            ));
        }
        None => 1.0,
    };
    let mut deff = args.design_effect.unwrap_or(1.0);
    let mut assumptions = Vec::new();
    if let (Some(m), Some(icc)) = (args.cluster_size, args.icc) {
        if !(m >= 1.0 && (0.0..1.0).contains(&icc)) {
            return Err(PhnError::InvalidInput(
                "cluster_size must be ≥ 1 and icc between 0 and 1".into(),
            ));
        }
        deff = ss::cluster_design_effect(m, icc);
        assumptions.push(format!("Design effect 1 + ({m} − 1) × {icc} = {deff:.2}"));
    } else if deff != 1.0 {
        assumptions.push(format!("Design effect {deff:.2} (supplied)"));
    }
    if deff < 1.0 {
        return Err(PhnError::InvalidInput(
            "design_effect must be at least 1".into(),
        ));
    }
    let side = if two_sided { "two-sided" } else { "one-sided" };
    let (g1, g2, pw, formula): (Option<f64>, Option<f64>, Option<f64>, String);
    match args.study_type {
        StudyType::SurveyProportion => {
            let p = prop(
                args.expected_proportion.or(Some(0.5)),
                "expected_proportion",
            )?;
            let d = prop(args.margin_of_error, "margin_of_error")?;
            if let Some(n) = args.population_size
                && n < 2.0
            {
                return Err(PhnError::InvalidInput(
                    "population_size must be at least 2".into(),
                ));
            }
            g1 = Some(ss::survey_proportion(
                p,
                d,
                confidence,
                args.population_size,
                deff,
            ));
            (g2, pw) = (None, None);
            assumptions.push(format!(
                "Expected proportion {:.1}%, margin ±{:.1} percentage points, {:.0}% confidence",
                p * 100.0,
                d * 100.0,
                confidence * 100.0
            ));
            formula =
                "n = DEFF·N·p(1−p) / [(d²/z²)(N−1) + p(1−p)] (CDC Epi Info StatCalc population \
                       survey); without N, n = DEFF·z²·p(1−p)/d²"
                    .into();
        }
        StudyType::SurveyMean => {
            let sd = need(args.standard_deviation, "standard_deviation")?;
            let d = need(args.margin_of_error, "margin_of_error")?;
            g1 = Some(ss::survey_mean(
                sd,
                d,
                confidence,
                args.population_size,
                deff,
            ));
            (g2, pw) = (None, None);
            assumptions.push(format!(
                "SD {sd}, margin ±{d}, {:.0}% confidence",
                confidence * 100.0
            ));
            formula = "n₀ = (z·σ/d)²; n = DEFF·n₀ / (1 + (n₀ − 1)/N)".into();
        }
        StudyType::TwoProportions | StudyType::CaseControl => {
            let (p1, p2) = if args.study_type == StudyType::CaseControl {
                let p0 = prop(args.control_exposure, "control_exposure")?;
                let or = need(args.odds_ratio, "odds_ratio")?;
                assumptions.push(format!(
                    "Controls exposed {:.1}%; odds ratio {or} implies {:.1}% of cases exposed",
                    p0 * 100.0,
                    ss::case_exposure_from_or(or, p0) * 100.0
                ));
                (ss::case_exposure_from_or(or, p0), p0)
            } else {
                let p2 = prop(args.p2, "p2")?;
                let p1 = match (args.p1, args.relative_risk) {
                    (Some(_), _) => prop(args.p1, "p1")?,
                    (None, Some(rr)) if rr > 0.0 && rr * p2 < 1.0 => rr * p2,
                    _ => {
                        return Err(PhnError::InvalidInput(
                            "give p1, or relative_risk with p2".into(),
                        ));
                    }
                };
                assumptions.push(format!(
                    "Outcome in group 1 {:.1}%, group 2 {:.1}%",
                    p1 * 100.0,
                    p2 * 100.0
                ));
                (p1, p2)
            };
            if (p1 - p2).abs() < 1e-9 {
                return Err(PhnError::InvalidInput(
                    "the two proportions must differ".into(),
                ));
            }
            let (n1, n2) = ss::two_proportions(p1, p2, alpha, power, ratio, two_sided, cc);
            (g1, g2, pw) = (Some(n1 * deff), Some(n2 * deff), None);
            assumptions.push(format!(
                "α = {alpha} ({side}), power {:.0}%, group 2 : group 1 = {ratio}",
                power * 100.0
            ));
            formula = format!(
                "Fleiss, Levin & Paik (2003) eq. 4.19{} for unequal groups",
                if cc {
                    " with the continuity correction (4.20)"
                } else {
                    ""
                }
            );
        }
        StudyType::TwoMeans => {
            let d = need(args.difference.map(f64::abs), "difference")?;
            let sd = need(args.standard_deviation, "standard_deviation")?;
            let (n1, n2) = ss::two_means(d, sd, alpha, power, ratio, two_sided);
            (g1, g2, pw) = (Some(n1 * deff), Some(n2 * deff), None);
            assumptions.push(format!("Difference {d}, SD {sd} (effect size d = {:.2}); α = {alpha} ({side}), power {:.0}%", d / sd, power * 100.0));
            formula = "n₁ = (1 + 1/k)(σ(z₁₋α + z₁₋β)/Δ)² + z₁₋α²/4 (normal approximation with \
                       Guenther's correction for the t test)"
                .into();
        }
        StudyType::PowerTwoProportions => {
            let p1 = prop(args.p1, "p1")?;
            let p2 = prop(args.p2, "p2")?;
            let (n1, n2) = (need(args.n1, "n1")?, need(args.n2.or(args.n1), "n2")?);
            pw = Some(ss::power_two_proportions(
                p1,
                p2,
                n1 / deff,
                n2 / deff,
                alpha,
                two_sided,
            ));
            (g1, g2) = (None, None);
            assumptions.push(format!(
                "{n1} and {n2} per group; {:.1}% vs {:.1}%; α = {alpha} ({side})",
                p1 * 100.0,
                p2 * 100.0
            ));
            formula =
                "Fleiss normal approximation solved for power (no continuity correction)".into();
        }
        StudyType::PowerTwoMeans => {
            let d = need(args.difference.map(f64::abs), "difference")?;
            let sd = need(args.standard_deviation, "standard_deviation")?;
            let (n1, n2) = (need(args.n1, "n1")?, need(args.n2.or(args.n1), "n2")?);
            pw = Some(ss::power_two_means(
                d,
                sd,
                n1 / deff,
                n2 / deff,
                alpha,
                two_sided,
            ));
            (g1, g2) = (None, None);
            assumptions.push(format!(
                "{n1} and {n2} per group; difference {d}, SD {sd}; α = {alpha} ({side})"
            ));
            formula = "Normal approximation: power = Φ(Δ/(σ√(1/n₁+1/n₂)) − z₁₋α)".into();
        }
    }
    if response < 1.0 {
        assumptions.push(format!(
            "Inflated for an expected response/retention rate of {:.0}%",
            response * 100.0
        ));
    }
    let group1 = g1.map(|n| ss::round_up(n, response));
    let group2 = g2.map(|n| ss::round_up(n, response));
    let total = match (&group1, &group2) {
        (Some(a), Some(b)) => Some(a.to_recruit + b.to_recruit),
        (Some(a), None) => Some(a.to_recruit),
        _ => None,
    };
    let clusters = match (args.cluster_size, total) {
        (Some(m), Some(t)) if m >= 1.0 => Some((t as f64 / m).ceil() as u64),
        _ => None,
    };

    let mut summary = String::from("**Sample size**\n\n");
    if let Some(p) = pw {
        summary = format!("**Power: {:.1}%**\n\n", p * 100.0);
        if p < 0.8 {
            summary.push_str("Below the usual 80% target: a real difference of this size could easily be missed.\n\n");
        }
    }
    let mut rows = Vec::new();
    let label = |s: &str| s.to_string();
    match (&group1, &group2) {
        (Some(a), Some(b)) => {
            let (l1, l2) = if args.study_type == StudyType::CaseControl {
                ("Cases", "Controls")
            } else {
                ("Group 1", "Group 2")
            };
            rows.push(vec![
                label(l1),
                md::num(a.exact, 1),
                a.needed.to_string(),
                a.to_recruit.to_string(),
            ]);
            rows.push(vec![
                label(l2),
                md::num(b.exact, 1),
                b.needed.to_string(),
                b.to_recruit.to_string(),
            ]);
            rows.push(vec![
                label("**Total**"),
                String::new(),
                (a.needed + b.needed).to_string(),
                (a.to_recruit + b.to_recruit).to_string(),
            ]);
        }
        (Some(a), None) => rows.push(vec![
            label("Respondents"),
            md::num(a.exact, 1),
            a.needed.to_string(),
            a.to_recruit.to_string(),
        ]),
        _ => {}
    }
    if !rows.is_empty() {
        summary.push_str(&md::table(
            &["", "Formula value", "Needed (rounded up)", "To recruit"],
            &rows,
        ));
    }
    if let Some(c) = clusters {
        summary.push_str(&format!(
            "\nClusters needed: **{c}** (of about {} people each).\n",
            args.cluster_size.unwrap_or(1.0)
        ));
    }
    summary.push_str("\n**Assumptions**\n");
    for a in &assumptions {
        summary.push_str(&format!("- {a}\n"));
    }
    summary.push_str(&format!("\nMethod: {formula}.\n"));

    let mut caveats = vec![
        "Sample size depends heavily on the assumed proportions, SD, and effect size; try a few \
         plausible values (a sensitivity analysis) and justify them from prior data or literature."
            .to_string(),
    ];
    if matches!(
        args.study_type,
        StudyType::SurveyProportion | StudyType::SurveyMean
    ) && deff == 1.0
    {
        caveats.push(
            "Assumes simple random sampling. Cluster samples (schools, households) need a design \
             effect; 1.5–2 is common when the ICC is unknown."
                .into(),
        );
    }
    if matches!(
        args.study_type,
        StudyType::TwoMeans | StudyType::PowerTwoMeans
    ) {
        caveats.push(
            "For very skewed outcomes, nonparametric tests need roughly 15% more participants."
                .into(),
        );
    }
    Ok(Envelope::new(
        summary,
        SampleSizeResult {
            study_type: args.study_type,
            group1,
            group2,
            total_to_recruit: total,
            power: pw,
            design_effect: deff,
            clusters,
            assumptions,
            formula,
        },
    )
    .sources([
        Source::computed(
            "Fleiss JL, Levin B, Paik MC. Statistical Methods for Rates and Proportions, 3rd ed. \
             Wiley; 2003 (two proportions); CDC Epi Info StatCalc (population survey); Guenther WC. \
             Am Stat 1981;35:243-244 (two means)",
            "PHN MCP server calculation (verified against R power.prop.test, power.t.test, and \
             Krejcie & Morgan 1970)",
        ),
    ])
    .caveats(caveats)
    .privacy(Privacy::none()))
}
