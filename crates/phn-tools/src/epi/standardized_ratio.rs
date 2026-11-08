//! EPI-05 `compute_standardized_ratio`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::standardize::{self, Indirect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epi::two_by_two::fmt_p;
use crate::md;

/// One stratum (age group, or age × sex).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RatioStratumInput {
    /// Stratum label, e.g. "45-54 female".
    pub stratum: String,
    /// Local population (or person-years) in this stratum.
    pub population: f64,
    /// Reference rate for this stratum, per `per` (e.g. the statewide rate per 100,000).
    #[serde(default)]
    pub reference_rate: Option<f64>,
    /// Alternatively, reference count and population (the rate is computed from them).
    #[serde(default)]
    pub reference_count: Option<f64>,
    #[serde(default)]
    pub reference_population: Option<f64>,
    /// Observed local count in this stratum (optional; the total is what matters).
    #[serde(default)]
    pub observed: Option<u64>,
}

/// One area or group.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RatioAreaInput {
    /// Area label, e.g. "Town A".
    pub label: String,
    pub strata: Vec<RatioStratumInput>,
    /// Total observed count (required unless every stratum gives `observed`).
    #[serde(default)]
    pub observed_total: Option<u64>,
}

/// Arguments for `compute_standardized_ratio`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct StandardizedRatioArgs {
    /// One or more areas, each with its strata.
    pub areas: Vec<RatioAreaInput>,
    /// Rate base for `reference_rate` (default 100,000).
    #[serde(default)]
    pub per: Option<f64>,
    /// Confidence level, 0.80–0.999 (default 0.95).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// "incidence" (SIR, default) or "mortality" (SMR); only changes the labels.
    #[serde(default)]
    pub measure: Option<String>,
    /// What was counted, e.g. "bladder cancer, 2015–2019".
    #[serde(default)]
    pub event_description: Option<String>,
    /// Name of the reference population, e.g. "Massachusetts".
    #[serde(default)]
    pub reference_name: Option<String>,
}

/// One area's ratio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AreaRatio {
    pub label: String,
    pub result: Indirect,
    /// Expected count in each stratum.
    pub expected_by_stratum: Vec<(String, f64)>,
    /// Plain-language classification of the ratio.
    pub classification: String,
}

/// Result of `compute_standardized_ratio`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StandardizedRatioResult {
    pub measure: String,
    pub confidence: f64,
    pub areas: Vec<AreaRatio>,
    pub method: String,
}

/// Indirect standardization (SIR/SMR) with exact Poisson intervals.
pub fn compute_standardized_ratio(
    ctx: &Context,
    args: StandardizedRatioArgs,
) -> Result<Envelope<StandardizedRatioResult>> {
    if args.areas.is_empty() || args.areas.len() > 400 {
        return Err(PhnError::InvalidInput("provide 1 to 400 areas".into()));
    }
    let per = args.per.unwrap_or(100_000.0);
    let confidence = args.confidence.unwrap_or(0.95);
    if !(0.80..=0.999).contains(&confidence) || !(per.is_finite() && per > 0.0) {
        return Err(PhnError::InvalidInput(
            "confidence must be between 0.80 and 0.999 and per must be positive".into(),
        ));
    }
    let smr = args
        .measure
        .as_deref()
        .is_some_and(|m| m.to_ascii_lowercase().contains("mort"));
    let abbr = if smr { "SMR" } else { "SIR" };
    let threshold = ctx.suppression_threshold();
    let mut areas = Vec::new();
    for a in &args.areas {
        if a.strata.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "'{}' has no strata",
                a.label
            )));
        }
        let mut expected = 0.0;
        let mut by = Vec::new();
        for s in &a.strata {
            let rate = match (s.reference_rate, s.reference_count, s.reference_population) {
                (Some(r), _, _) if r >= 0.0 => r / per,
                (None, Some(c), Some(p)) if c >= 0.0 && p > 0.0 => c / p,
                _ => {
                    return Err(PhnError::InvalidInput(format!(
                        "stratum '{}' of '{}' needs a reference_rate, or a reference_count and \
                         reference_population",
                        s.stratum, a.label
                    )));
                }
            };
            if !(s.population.is_finite() && s.population >= 0.0) {
                return Err(PhnError::InvalidInput(format!(
                    "population for '{}' in '{}' must be zero or more",
                    s.stratum, a.label
                )));
            }
            let e = s.population * rate;
            expected += e;
            by.push((s.stratum.clone(), e));
        }
        let observed = match a.observed_total {
            Some(o) => o,
            None => {
                if a.strata.iter().all(|s| s.observed.is_some()) {
                    a.strata.iter().map(|s| s.observed.unwrap_or(0)).sum()
                } else {
                    return Err(PhnError::InvalidInput(format!(
                        "'{}' needs observed_total (or an observed count in every stratum)",
                        a.label
                    )));
                }
            }
        };
        if expected <= 0.0 {
            return Err(PhnError::InvalidInput(format!(
                "the expected count for '{}' is zero; check the reference rates and populations",
                a.label
            )));
        }
        let r = standardize::indirect(observed, expected, confidence);
        let classification = if r.lower > 1.0 {
            "significantly higher than expected".to_string()
        } else if r.upper < 1.0 {
            "significantly lower than expected".to_string()
        } else {
            "not statistically different from expected".to_string()
        };
        areas.push(AreaRatio {
            label: a.label.clone(),
            result: r,
            expected_by_stratum: by,
            classification,
        });
    }
    let pctl = format!("{:.0}%", confidence * 100.0);
    let refn = args
        .reference_name
        .clone()
        .unwrap_or_else(|| "the reference population".into());
    let mut summary = format!(
        "**Standardized {} ratios ({abbr})**{} — expected counts use {refn}'s stratum-specific rates\n\n",
        if smr { "mortality" } else { "incidence" },
        args.event_description
            .as_ref()
            .map(|d| format!(" for {d}"))
            .unwrap_or_default()
    );
    let rows: Vec<Vec<String>> = areas
        .iter()
        .map(|a| {
            let r = &a.result;
            vec![
                a.label.clone(),
                r.observed.to_string(),
                md::num(r.expected, 1),
                format!("{:.2} ({:.2}–{:.2})", r.ratio, r.lower, r.upper),
                fmt_p(r.p_value),
                a.classification.clone(),
            ]
        })
        .collect();
    summary.push_str(&md::table(
        &[
            "Area",
            "Observed",
            "Expected",
            &format!("{abbr} ({pctl} CI)"),
            "Exact p",
            "Interpretation",
        ],
        &rows,
    ));
    summary.push_str(&format!(
        "\n{abbr} = observed ÷ expected; 1.00 means the same as {refn} after accounting for the \
         strata (usually age and sex).\n"
    ));
    let mut caveats = vec![
        format!(
            "An {abbr} compares each area with {refn}, not areas with each other: two areas' \
             ratios are not directly comparable because their populations have different \
             structures."
        ),
        "For community cancer concerns, an elevated ratio alone does not show a common cause; \
         follow CDC/CSTE guidance for investigating suspected cancer clusters (cancer types with a \
         plausible shared cause, latency, residence history), and remember that many areas \
         tested will show some 'significant' results by chance."
            .into(),
    ];
    let small: Vec<&str> = areas
        .iter()
        .filter(|a| a.result.observed > 0 && a.result.observed < u64::from(threshold))
        .map(|a| a.label.as_str())
        .collect();
    if !small.is_empty() {
        caveats.push(format!(
            "Observed counts below your suppression threshold ({threshold}) must be suppressed \
             before publishing: {}.",
            small.join(", ")
        ));
    }
    if areas.iter().any(|a| a.result.expected < 5.0) {
        caveats.push(
            "Expected counts below 5 give very wide intervals; consider combining years or areas."
                .into(),
        );
    }
    Ok(Envelope::new(
        summary,
        StandardizedRatioResult {
            measure: abbr.into(),
            confidence,
            areas,
            method: "Indirect standardization: expected = Σ local populationᵢ × reference rateᵢ; \
                     exact Poisson confidence interval for the observed count divided by the \
                     expected count; exact two-sided Poisson p-value (as R poisson.test)."
                .into(),
        },
    )
    .source(Source::computed(
        "Indirect standardization with exact Poisson limits (Breslow NE, Day NE. Statistical \
         Methods in Cancer Research, Vol. II, IARC 1987)",
        "PHN MCP server calculation (verified against R poisson.test)",
    ))
    .caveats(caveats)
    .privacy(Privacy {
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    }))
}
