//! EPI-01 `calculate_rates`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::rates::{self, Rate, RateFlag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// One group (town, age group, year, …).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RateGroupInput {
    /// Group label, e.g. "Amherst" or "65+".
    pub label: String,
    /// Number of events (cases, deaths, visits).
    pub count: u64,
    /// Population or person-time at risk.
    pub population: f64,
}

/// Arguments for `calculate_rates`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CalculateRatesArgs {
    /// One or more groups. Groups should not overlap if a total is wanted.
    pub groups: Vec<RateGroupInput>,
    /// Rate base: 1000, 10000, or 100000 (default 100000).
    #[serde(default)]
    pub per: Option<f64>,
    /// Confidence level between 0.80 and 0.999 (default 0.95).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Add a crude total across groups (default true when there are 2+ groups).
    #[serde(default)]
    pub include_total: Option<bool>,
    /// What was counted, e.g. "pertussis cases, 2024".
    #[serde(default)]
    pub event_description: Option<String>,
}

/// Result of `calculate_rates`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RatesResult {
    pub per: f64,
    pub confidence: f64,
    pub method: String,
    pub rates: Vec<Rate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Rate>,
    pub suppression_threshold: u32,
}

const METHOD: &str = "Exact (Garwood) Poisson confidence interval: lower = χ²(α/2; 2x)/2, \
upper = χ²(1−α/2; 2x+2)/2, scaled by per/denominator.";

/// Crude and category-specific rates with exact Poisson confidence intervals.
pub fn calculate_rates(ctx: &Context, args: CalculateRatesArgs) -> Result<Envelope<RatesResult>> {
    if args.groups.is_empty() {
        return Err(PhnError::InvalidInput("provide at least one group".into()));
    }
    if args.groups.len() > 500 {
        return Err(PhnError::InvalidInput("at most 500 groups per call".into()));
    }
    let per = args.per.unwrap_or(100_000.0);
    if !(per.is_finite() && per > 0.0) {
        return Err(PhnError::InvalidInput(
            "per must be a positive number".into(),
        ));
    }
    let confidence = args.confidence.unwrap_or(0.95);
    if !(0.80..=0.999).contains(&confidence) {
        return Err(PhnError::InvalidInput(
            "confidence must be between 0.80 and 0.999".into(),
        ));
    }
    for g in &args.groups {
        if !(g.population.is_finite() && g.population > 0.0) {
            return Err(PhnError::InvalidInput(format!(
                "population for '{}' must be a positive number",
                g.label
            )));
        }
    }
    let threshold = ctx.suppression_threshold();
    let computed: Vec<Rate> = args
        .groups
        .iter()
        .map(|g| rates::rate(&g.label, g.count, g.population, per, confidence, threshold))
        .collect();
    let include_total = args.include_total.unwrap_or(args.groups.len() > 1);
    let total = include_total.then(|| {
        let count: u64 = args.groups.iter().map(|g| g.count).sum();
        let pop: f64 = args.groups.iter().map(|g| g.population).sum();
        rates::rate("Total", count, pop, per, confidence, threshold)
    });

    let max_rate = computed
        .iter()
        .chain(total.iter())
        .map(|r| r.upper)
        .fold(0.0, f64::max);
    let dec = rates::display_decimals(max_rate) as usize;
    let pct = (confidence * 100.0).round();
    let row = |r: &Rate| {
        let mut notes = Vec::new();
        for f in &r.flags {
            notes.push(match f {
                RateFlag::ZeroCount => "zero events",
                RateFlag::Unreliable => "unreliable (<20 events)",
                RateFlag::BelowSuppressionThreshold => "suppress before sharing",
                RateFlag::CountExceedsDenominator => "count exceeds population",
            });
        }
        vec![
            r.label.clone(),
            md::num(r.count as f64, 0),
            md::num(r.denominator, 0),
            md::num(r.rate, dec),
            format!("{} – {}", md::num(r.lower, dec), md::num(r.upper, dec)),
            notes.join("; "),
        ]
    };
    let mut rows: Vec<Vec<String>> = computed.iter().map(row).collect();
    if let Some(t) = &total {
        let mut r = row(t);
        r[0] = "**Total**".into();
        rows.push(r);
    }
    let per_label = md::num(per, 0);
    let mut summary = format!(
        "**Rates per {per_label}**{} with exact {pct}% confidence intervals\n\n",
        args.event_description
            .as_ref()
            .map(|d| format!(" — {d}"))
            .unwrap_or_default()
    );
    summary.push_str(&md::table(
        &[
            "Group",
            "Count",
            "Population",
            "Rate",
            &format!("{pct}% CI"),
            "Notes",
        ],
        &rows,
    ));

    let mut caveats = Vec::new();
    let unreliable: Vec<&str> = computed
        .iter()
        .filter(|r| r.flags.contains(&RateFlag::Unreliable))
        .map(|r| r.label.as_str())
        .collect();
    if !unreliable.is_empty() {
        caveats.push(format!(
            "Rates based on fewer than 20 events are statistically unreliable (relative standard \
             error ≥ 23%): {}. Consider combining years or areas.",
            unreliable.join(", ")
        ));
    }
    let small: Vec<&str> = computed
        .iter()
        .filter(|r| r.flags.contains(&RateFlag::BelowSuppressionThreshold))
        .map(|r| r.label.as_str())
        .collect();
    if !small.is_empty() {
        caveats.push(format!(
            "Counts below your suppression threshold ({threshold}) must be suppressed before \
             publishing, together with any rate computed from them: {}. Use \
             apply_small_cell_suppression on the table you plan to share.",
            small.join(", ")
        ));
    }
    if total.is_some() {
        caveats.push(
            "The total is a crude rate that assumes the groups do not overlap and together cover \
             the whole population."
                .into(),
        );
    }
    caveats.push(
        "Crude and category-specific rates are not age-adjusted; differences in age structure \
         can explain differences between towns."
            .into(),
    );

    Ok(Envelope::new(
        summary,
        RatesResult {
            per,
            confidence,
            method: METHOD.into(),
            rates: computed,
            total,
            suppression_threshold: threshold,
        },
    )
    .source(Source::computed(
        "Exact Poisson confidence limits (Garwood F. Fiducial limits for the Poisson distribution. Biometrika 1936;28:437-442)",
        "PHN MCP server calculation",
    ))
    .caveats(caveats)
    .privacy(Privacy {
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    }))
}
