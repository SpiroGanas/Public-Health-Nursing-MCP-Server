//! EPI-02 `age_adjust_rates`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_epi::standardize::{
    self, AgeGroup, DirectAdjusted, RateComparison, StandardBasis, Stratum, parse_age_group,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Counts and population for one age group.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AgeStratumInput {
    /// Age group label, e.g. "<1", "1-4", "45–54", "85+", "65 and over".
    pub age_group: String,
    /// Number of events (cases, deaths, hospitalizations).
    pub count: u64,
    /// Population (or person-time) in this age group.
    pub population: f64,
}

/// One population to adjust (a town, a year, a county).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AgeAdjustGroupInput {
    /// Label, e.g. "Springfield 2024".
    pub label: String,
    /// Age-specific counts and populations; groups must not overlap.
    pub strata: Vec<AgeStratumInput>,
}

/// A custom standard population for one age group.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct StandardInput {
    pub age_group: String,
    pub population: f64,
}

/// Arguments for `age_adjust_rates`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AgeAdjustArgs {
    /// One or more populations to age-adjust.
    pub groups: Vec<AgeAdjustGroupInput>,
    /// Custom standard population (default: the 2000 U.S. standard population,
    /// used by NCHS, CDC, and Massachusetts DPH).
    #[serde(default)]
    pub custom_standard: Option<Vec<StandardInput>>,
    /// Rate base (default 100,000).
    #[serde(default)]
    pub per: Option<f64>,
    /// Confidence level, 0.80–0.999 (default 0.95).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Compare every other group with this one (label).
    #[serde(default)]
    pub reference_group: Option<String>,
    /// What was counted, e.g. "opioid-related overdose deaths".
    #[serde(default)]
    pub event_description: Option<String>,
}

/// One adjusted group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdjustedGroup {
    pub label: String,
    pub result: DirectAdjusted,
    /// Comparison with the reference group, when requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vs_reference: Option<RateComparison>,
    /// Fewer than 20 events: the adjusted rate is statistically unreliable.
    pub unreliable: bool,
    /// The total count is below the suppression threshold.
    pub below_suppression_threshold: bool,
}

/// Result of `age_adjust_rates`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgeAdjustResult {
    pub per: f64,
    pub confidence: f64,
    pub standard: String,
    /// NCHS Master List or single-year standard (2000 U.S. standard only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_basis: Option<StandardBasis>,
    pub groups: Vec<AdjustedGroup>,
    pub method: String,
}

const METHOD: &str = "Direct standardization: Σ wᵢ·(countᵢ/populationᵢ) with standard weights wᵢ; \
gamma-method confidence interval (Fay & Feuer, Stat Med 1997;16:791–801); crude rates with exact \
Poisson intervals; comparisons with a z-test on the difference and a log-scale interval for the ratio.";

/// True when the age groups run from age 0 to an open-ended oldest group
/// with no gaps, so the adjusted rate covers all ages.
fn covers_all_ages(ages: &[AgeGroup]) -> bool {
    let mut sorted = ages.to_vec();
    sorted.sort_by_key(|a| a.start);
    let mut next = 0u32;
    for (i, a) in sorted.iter().enumerate() {
        if a.start != next {
            return false;
        }
        match a.end {
            Some(e) => next = e.saturating_add(1),
            None => return i + 1 == sorted.len(),
        }
    }
    false
}

/// Directly age-adjusted rates with gamma confidence intervals.
pub fn age_adjust_rates(ctx: &Context, args: AgeAdjustArgs) -> Result<Envelope<AgeAdjustResult>> {
    if args.groups.is_empty() || args.groups.len() > 200 {
        return Err(PhnError::InvalidInput("provide 1 to 200 groups".into()));
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
    let custom: Option<Vec<(AgeGroup, f64)>> = match &args.custom_standard {
        Some(s) if !s.is_empty() => Some(
            s.iter()
                .map(|x| {
                    let g = parse_age_group(&x.age_group).ok_or_else(|| {
                        PhnError::InvalidInput(format!("cannot read age group '{}'", x.age_group))
                    })?;
                    if !(x.population.is_finite() && x.population > 0.0) {
                        return Err(PhnError::InvalidInput(format!(
                            "standard population for '{}' must be positive",
                            x.age_group
                        )));
                    }
                    Ok((g, x.population))
                })
                .collect::<Result<_>>()?,
        ),
        _ => None,
    };
    let threshold = ctx.suppression_threshold();
    let mut basis_used: Option<StandardBasis> = None;
    let mut out = Vec::new();
    // Age groups used by each group, sorted, to check coverage and consistency.
    let mut group_ages: Vec<Vec<AgeGroup>> = Vec::new();
    for g in &args.groups {
        if g.strata.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "'{}' has no age groups",
                g.label
            )));
        }
        let mut ages = Vec::new();
        for s in &g.strata {
            let a = parse_age_group(&s.age_group).ok_or_else(|| {
                PhnError::InvalidInput(format!(
                    "cannot read age group '{}' in '{}'; use labels like <1, 1-4, 45-54, 85+",
                    s.age_group, g.label
                ))
            })?;
            if !(s.population.is_finite() && s.population >= 0.0) {
                return Err(PhnError::InvalidInput(format!(
                    "population for {} in '{}' must be zero or more",
                    s.age_group, g.label
                )));
            }
            if s.population == 0.0 && s.count > 0 {
                return Err(PhnError::InvalidInput(format!(
                    "{} in '{}' has events but no population",
                    s.age_group, g.label
                )));
            }
            ages.push(a);
        }
        let weights: Vec<f64> = match &custom {
            Some(c) => ages
                .iter()
                .map(|a| {
                    c.iter()
                        .find(|(g, _)| g == a)
                        .map(|(_, w)| *w)
                        .ok_or_else(|| {
                            PhnError::InvalidInput(format!(
                                "the custom standard has no age group {} (used in '{}')",
                                a.label(),
                                g.label
                            ))
                        })
                })
                .collect::<Result<_>>()?,
            None => {
                let (w, basis) =
                    standardize::us2000_weights(&ages).map_err(PhnError::InvalidInput)?;
                if basis == StandardBasis::SingleAges || basis_used.is_none() {
                    basis_used = Some(basis);
                }
                w
            }
        };
        if g.strata.iter().all(|s| s.population == 0.0) {
            return Err(PhnError::InvalidInput(format!(
                "'{}' has no population",
                g.label
            )));
        }
        let strata: Vec<Stratum> = g
            .strata
            .iter()
            .zip(&weights)
            .map(|(s, w)| Stratum {
                count: s.count,
                population: s.population,
                standard: *w,
            })
            .collect();
        ages.sort_by_key(|a| (a.start, a.end));
        group_ages.push(ages);
        let r = standardize::direct_adjust(&strata, per, confidence);
        out.push(AdjustedGroup {
            label: g.label.clone(),
            unreliable: r.total_count < 20,
            below_suppression_threshold: r.total_count > 0 && r.total_count < u64::from(threshold),
            result: r,
            vs_reference: None,
        });
    }
    let mut ref_index: Option<usize> = None;
    if let Some(refl) = &args.reference_group {
        let Some(ri) = out
            .iter()
            .position(|g| g.label.eq_ignore_ascii_case(refl.trim()))
        else {
            return Err(PhnError::InvalidInput(format!(
                "reference group '{refl}' is not one of the groups"
            )));
        };
        ref_index = Some(ri);
        let reference = out[ri].result.clone();
        for (i, g) in out.iter_mut().enumerate() {
            if i != ri {
                g.vs_reference = standardize::compare(&g.result, &reference, confidence);
            }
        }
    }

    let dec =
        phn_epi::rates::display_decimals(out.iter().map(|g| g.result.upper).fold(0.0, f64::max))
            as usize;
    let pctl = format!("{:.0}%", confidence * 100.0);
    let per_l = md::num(per, 0);
    let mut summary = format!(
        "**Age-adjusted rates per {per_l}**{} ({pctl} CIs)\n\n",
        args.event_description
            .as_ref()
            .map(|d| format!(" — {d}"))
            .unwrap_or_default()
    );
    let has_ref = args.reference_group.is_some();
    let mut headers = vec![
        "Group",
        "Events",
        "Crude rate (CI)",
        "Age-adjusted rate (CI)",
        "Notes",
    ];
    if has_ref {
        headers.insert(4, "Ratio vs reference (CI)");
    }
    let rows: Vec<Vec<String>> = out
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let r = &g.result;
            let mut notes = Vec::new();
            if g.unreliable {
                notes.push("unreliable (<20 events)");
            }
            if g.below_suppression_threshold {
                notes.push("suppress before sharing");
            }
            let mut row = vec![
                g.label.clone(),
                md::num(r.total_count as f64, 0),
                format!(
                    "{} ({}–{})",
                    md::num(r.crude_rate, dec),
                    md::num(r.crude_lower, dec),
                    md::num(r.crude_upper, dec)
                ),
                format!(
                    "{} ({}–{})",
                    md::num(r.rate, dec),
                    md::num(r.lower, dec),
                    md::num(r.upper, dec)
                ),
                notes.join("; "),
            ];
            if has_ref {
                row.insert(
                    4,
                    match &g.vs_reference {
                        Some(c) => format!(
                            "{:.2} ({:.2}–{:.2}); {}",
                            c.ratio,
                            c.ratio_lower,
                            c.ratio_upper,
                            crate::epi::two_by_two::fmt_p(c.p_value)
                        ),
                        None if Some(i) == ref_index => "reference".into(),
                        // A ratio needs events in both groups.
                        None => "not estimable (a rate of zero)".into(),
                    },
                );
            }
            row
        })
        .collect();
    summary.push_str(&md::table(&headers, &rows));
    let standard = if custom.is_some() {
        "custom standard population (supplied in the request)".to_string()
    } else {
        "2000 U.S. standard population".to_string()
    };
    summary.push_str(&format!("\nStandard: {standard}.\n"));

    let mut caveats = Vec::new();
    let unreliable: Vec<&str> = out
        .iter()
        .filter(|g| g.unreliable)
        .map(|g| g.label.as_str())
        .collect();
    if !unreliable.is_empty() {
        caveats.push(format!(
            "Age-adjusted rates based on fewer than 20 events are unreliable and should be \
             interpreted with caution or not reported: {}.",
            unreliable.join(", ")
        ));
    }
    let small: Vec<&str> = out
        .iter()
        .filter(|g| g.below_suppression_threshold)
        .map(|g| g.label.as_str())
        .collect();
    if !small.is_empty() {
        caveats.push(format!(
            "Totals below your suppression threshold ({threshold}) should be suppressed with their \
             rates before publishing: {}.",
            small.join(", ")
        ));
    }
    if basis_used == Some(StandardBasis::SingleAges) {
        caveats.push(
            "Your age groups split groups of the NCHS Master List, so weights come from the \
             single-year 2000 standard population; they differ from NCHS published weights by a \
             few millionths, which does not change the rates in practice."
                .into(),
        );
    }
    let partial: Vec<&str> = out
        .iter()
        .zip(&group_ages)
        .filter(|(_, ages)| !covers_all_ages(ages))
        .map(|(g, _)| g.label.as_str())
        .collect();
    if !partial.is_empty() {
        caveats.push(format!(
            "The age groups given do not run from age 0 to an open-ended oldest group (such as \
             85+) without gaps for: {}. The standard weights were rescaled to the ages given, so \
             these rates describe only those ages and are not comparable with all-ages \
             age-adjusted rates published elsewhere. If an age group was left out by mistake, \
             add it and run this again.",
            partial.join(", ")
        ));
    }
    if group_ages.windows(2).any(|w| w[0] != w[1]) {
        caveats.push(
            "The groups do not all use the same age groups, so their rates were adjusted with \
             different weights. Use the same age groups for every group before comparing them."
                .into(),
        );
    }
    if out.iter().any(|g| g.result.empty_strata > 0) {
        caveats.push(
            "Age groups with zero population were dropped and the remaining standard weights \
             rescaled; rates then describe only the ages present."
                .into(),
        );
    }
    caveats.push(
        "Age-adjusted rates are for comparison only (between places or years using the same \
         standard and age groups); they are not the actual burden. Report crude rates and counts \
         alongside them."
            .into(),
    );
    if has_ref {
        caveats.push(
            "Comparisons use normal approximations; for very small counts prefer judging overlap \
             of the gamma intervals, and avoid many pairwise tests."
                .into(),
        );
    }

    let mut sources = vec![Source::computed(
        "Direct age adjustment with gamma intervals (Fay MP, Feuer EJ. Stat Med 1997;16:791-801)",
        "PHN MCP server calculation (verified against R epitools::ageadjust.direct)",
    )];
    if custom.is_none() {
        sources.push(Source {
            title: "Age adjustment using the 2000 projected U.S. population (Healthy People 2010 \
                    Statistical Notes No. 20), Master List; 2000 U.S. standard population by \
                    single year (Census P25-1130)"
                .into(),
            publisher: "National Center for Health Statistics; NCI SEER Program".into(),
            url: Some("https://www.cdc.gov/nchs/data/statnt/statnt20.pdf".into()),
            effective_date: Some("2001-01".into()),
            retrieved: Some("2026-11-07".into()),
            mode: SourceMode::Snapshot,
        });
    }
    Ok(Envelope::new(
        summary,
        AgeAdjustResult {
            per,
            confidence,
            standard,
            standard_basis: if custom.is_none() { basis_used } else { None },
            groups: out,
            method: METHOD.into(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let mut c = phn_core::Config::for_root(dir.path());
        c.offline = true;
        (dir, Context::new(c).unwrap())
    }

    fn group(label: &str, strata: &[(&str, u64, f64)]) -> AgeAdjustGroupInput {
        AgeAdjustGroupInput {
            label: label.into(),
            strata: strata
                .iter()
                .map(|(a, c, p)| AgeStratumInput {
                    age_group: (*a).into(),
                    count: *c,
                    population: *p,
                })
                .collect(),
        }
    }

    fn run(groups: Vec<AgeAdjustGroupInput>, reference: Option<&str>) -> Envelope<AgeAdjustResult> {
        let (_dir, ctx) = ctx();
        age_adjust_rates(
            &ctx,
            AgeAdjustArgs {
                groups,
                custom_standard: None,
                per: None,
                confidence: None,
                reference_group: reference.map(str::to_string),
                event_description: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn coverage_check() {
        let g = |s: &str| parse_age_group(s).unwrap();
        assert!(covers_all_ages(&[g("65+"), g("0-44"), g("45-64")]));
        assert!(covers_all_ages(&[g("0+")]));
        assert!(!covers_all_ages(&[g("0-4"), g("15-24"), g("25+")]));
        assert!(!covers_all_ages(&[g("0-44"), g("45-64")]));
        assert!(!covers_all_ages(&[g("18-44"), g("45+")]));
    }

    #[test]
    fn only_the_reference_group_is_labeled_reference() {
        let ages: [(&'static str, f64); 3] =
            [("0-44", 60_000.0), ("45-64", 30_000.0), ("65+", 10_000.0)];
        let with = |counts: [u64; 3]| -> Vec<(&'static str, u64, f64)> {
            ages.iter()
                .zip(counts)
                .map(|((a, p), c)| (*a, c, *p))
                .collect()
        };
        let r = run(
            vec![
                group("Town A", &with([30, 60, 90])),
                group("Town B", &with([0, 0, 0])),
                group("Town C", &with([20, 70, 80])),
            ],
            Some("Town A"),
        );
        // Town B has no events: no ratio can be estimated. It used to be
        // labeled "reference" like Town A.
        assert!(r.data.groups[1].vs_reference.is_none());
        assert!(r.data.groups[2].vs_reference.is_some());
        let line = |town: &str| {
            r.summary
                .lines()
                .find(|l| l.contains(town))
                .unwrap_or_default()
                .to_string()
        };
        assert!(line("Town A").contains("reference"), "{}", r.summary);
        assert!(line("Town B").contains("not estimable"), "{}", r.summary);
        assert!(!line("Town B").contains("reference"), "{}", r.summary);
        // Complete, matching age groups: no coverage or consistency caveat.
        assert!(
            !r.caveats
                .iter()
                .any(|c| c.contains("without gaps") || c.contains("same age groups")),
            "{:?}",
            r.caveats
        );
    }

    #[test]
    fn gaps_and_mismatched_age_groups_are_flagged() {
        let r = run(
            vec![
                // Ages 5–14 are missing.
                group(
                    "Gap Town",
                    &[
                        ("0-4", 2, 5_000.0),
                        ("15-24", 5, 12_000.0),
                        ("25+", 80, 60_000.0),
                    ],
                ),
                group(
                    "Whole Town",
                    &[
                        ("0-14", 4, 15_000.0),
                        ("15-24", 5, 12_000.0),
                        ("25+", 80, 60_000.0),
                    ],
                ),
            ],
            None,
        );
        let gap = r
            .caveats
            .iter()
            .find(|c| c.contains("without gaps"))
            .expect("coverage caveat");
        assert!(
            gap.contains("Gap Town") && !gap.contains("Whole Town"),
            "{gap}"
        );
        assert!(r.caveats.iter().any(|c| c.contains("same age groups")));
    }
}
