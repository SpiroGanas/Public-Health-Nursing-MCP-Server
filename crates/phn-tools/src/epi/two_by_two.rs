//! EPI-03 `analyze_two_by_two`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::two_by_two::{self, Design, Estimate, Stratified, Table, TwoByTwo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// One 2×2 table (exposure in rows, outcome in columns).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TableInput {
    /// Exposed people who became ill (cases).
    pub exposed_cases: u64,
    /// Exposed people who did not become ill (non-cases or controls).
    pub exposed_noncases: u64,
    /// Unexposed cases.
    pub unexposed_cases: u64,
    /// Unexposed non-cases (or controls).
    pub unexposed_noncases: u64,
    /// Stratum label when several tables are given (e.g. "Age < 18").
    #[serde(default)]
    pub label: Option<String>,
}

impl TableInput {
    fn table(&self) -> Table {
        Table::new(
            self.exposed_cases,
            self.exposed_noncases,
            self.unexposed_cases,
            self.unexposed_noncases,
        )
    }
}

/// Arguments for `analyze_two_by_two`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TwoByTwoArgs {
    /// One table, or one per stratum for a Mantel–Haenszel analysis.
    pub tables: Vec<TableInput>,
    /// cohort (attack rates, risk ratio; default), case_control (odds ratio
    /// only), or cross_sectional (prevalence ratio and prevalence odds ratio).
    #[serde(default)]
    pub design: Option<Design>,
    /// Confidence level, 0.80–0.999 (default 0.95).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// What the exposure was, e.g. "ate potato salad".
    #[serde(default)]
    pub exposure: Option<String>,
    /// What the outcome was, e.g. "gastrointestinal illness".
    #[serde(default)]
    pub outcome: Option<String>,
}

/// Result of `analyze_two_by_two`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TwoByTwoResult {
    pub design: Design,
    pub confidence: f64,
    /// Each table's analysis (stratum-specific when several are given).
    pub tables: Vec<TwoByTwo>,
    /// Mantel–Haenszel summary when two or more strata are given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stratified: Option<Stratified>,
    /// Plain-language interpretation.
    pub interpretation: String,
}

/// Largest count accepted in one cell. Far above any real table (it is
/// about three times the U.S. population); larger values are typing errors
/// and would only waste time.
const MAX_CELL: u64 = 1_000_000_000;

fn fmt_est(e: &Estimate, d: usize) -> String {
    format!(
        "{} ({} – {})",
        md::num(e.value, d),
        md::num(e.lower, d),
        e.upper.map_or("∞".into(), |u| md::num(u, d))
    )
}

fn pct(x: f64) -> String {
    format!("{:.1}%", 100.0 * x)
}

/// Measures of association and tests for 2×2 tables.
pub fn analyze_two_by_two(ctx: &Context, args: TwoByTwoArgs) -> Result<Envelope<TwoByTwoResult>> {
    if args.tables.is_empty() {
        return Err(PhnError::InvalidInput(
            "provide at least one 2×2 table".into(),
        ));
    }
    if args.tables.len() > 50 {
        return Err(PhnError::InvalidInput("at most 50 strata".into()));
    }
    let confidence = args.confidence.unwrap_or(0.95);
    if !(0.80..=0.999).contains(&confidence) {
        return Err(PhnError::InvalidInput(
            "confidence must be between 0.80 and 0.999".into(),
        ));
    }
    let design = args.design.unwrap_or_default();
    for t in &args.tables {
        let tb = t.table();
        if [tb.a, tb.b, tb.c, tb.d].iter().any(|v| *v > MAX_CELL) {
            return Err(PhnError::InvalidInput(format!(
                "a table cell is larger than {}; check the counts",
                md::num(MAX_CELL as f64, 0)
            )));
        }
        if tb.total() == 0 {
            return Err(PhnError::InvalidInput("a table has no people in it".into()));
        }
    }
    let threshold = ctx.suppression_threshold();
    let results: Vec<TwoByTwo> = args
        .tables
        .iter()
        .map(|t| two_by_two::analyze(t.table(), design, confidence))
        .collect();
    let stratified = (args.tables.len() > 1).then(|| {
        let tables: Vec<Table> = args.tables.iter().map(TableInput::table).collect();
        two_by_two::mantel_haenszel(&tables, design, confidence)
    });

    let pctl = format!("{:.0}%", confidence * 100.0);
    let exposure = args.exposure.clone().unwrap_or_else(|| "Exposed".into());
    let outcome = args.outcome.clone().unwrap_or_else(|| "outcome".into());
    let mut summary = format!(
        "**2×2 analysis ({})** — exposure: {exposure}; outcome: {outcome}\n\n",
        match design {
            Design::Cohort => "cohort / attack rates",
            Design::CaseControl => "case-control",
            Design::CrossSectional => "cross-sectional",
        }
    );
    for (i, (t, r)) in args.tables.iter().zip(&results).enumerate() {
        if args.tables.len() > 1 {
            summary.push_str(&format!(
                "### Stratum {}: {}\n\n",
                i + 1,
                t.label.clone().unwrap_or_else(|| format!("{}", i + 1))
            ));
        }
        let tb = t.table();
        let (cases, non) = match design {
            Design::CaseControl => ("Cases", "Controls"),
            _ => ("Ill", "Not ill"),
        };
        let mut rows = vec![
            vec![
                "Exposed".to_string(),
                tb.a.to_string(),
                tb.b.to_string(),
                tb.n1().to_string(),
                r.risk_exposed.map(pct).unwrap_or_default(),
            ],
            vec![
                "Unexposed".to_string(),
                tb.c.to_string(),
                tb.d.to_string(),
                tb.n0().to_string(),
                r.risk_unexposed.map(pct).unwrap_or_default(),
            ],
        ];
        rows.push(vec![
            "Total".into(),
            tb.m1().to_string(),
            tb.m0().to_string(),
            tb.total().to_string(),
            String::new(),
        ]);
        let rate_head = if design == Design::CrossSectional {
            "Prevalence"
        } else {
            "Attack rate"
        };
        summary.push_str(&md::table(&["", cases, non, "Total", rate_head], &rows));
        let mut m = Vec::new();
        if let Some(e) = &r.risk_ratio {
            let name = if design == Design::CrossSectional {
                "Prevalence ratio"
            } else {
                "Risk ratio"
            };
            m.push(vec![name.into(), fmt_est(e, 2), e.method.clone()]);
        }
        if let Some(e) = &r.risk_difference {
            m.push(vec![
                "Risk difference".into(),
                format!(
                    "{} ({} – {})",
                    pct(e.value),
                    pct(e.lower),
                    e.upper.map_or("∞".into(), pct)
                ),
                e.method.clone(),
            ]);
        }
        if let Some(e) = &r.odds_ratio {
            m.push(vec!["Odds ratio".into(), fmt_est(e, 2), e.method.clone()]);
        }
        if let Some(e) = &r.odds_ratio_exact {
            m.push(vec![
                "Odds ratio (exact)".into(),
                fmt_est(e, 2),
                e.method.clone(),
            ]);
        }
        if let Some(af) = r.attributable_fraction_exposed {
            m.push(vec![
                if r.protective {
                    "Preventable fraction (exposed)"
                } else {
                    "Attributable fraction (exposed)"
                }
                .into(),
                pct(af),
                String::new(),
            ]);
        }
        if let Some(paf) = r.population_attributable_fraction {
            m.push(vec![
                "Population attributable fraction".into(),
                pct(paf),
                match design {
                    Design::CaseControl => "Miettinen (proportion of cases exposed × AF)",
                    _ => "Levin (assumes the exposure prevalence represents the population)",
                }
                .into(),
            ]);
        }
        summary.push_str(&format!("\n**Measures ({pctl} CI)**\n\n"));
        summary.push_str(&md::table(&["Measure", "Estimate (CI)", "Method"], &m));
        let tests: Vec<Vec<String>> = r
            .chi_square
            .iter()
            .chain(r.exact.iter())
            .map(|t| {
                vec![
                    t.name.clone(),
                    t.statistic
                        .map(|s| md::num(s, 3))
                        .unwrap_or_else(|| "—".into()),
                    fmt_p(t.p_value),
                ]
            })
            .collect();
        summary.push_str("\n**Tests**\n\n");
        summary.push_str(&md::table(&["Test", "Statistic", "p-value"], &tests));
        for w in &r.warnings {
            summary.push_str(&format!("\n⚠ {w}\n"));
        }
        summary.push('\n');
    }
    if let Some(s) = &stratified {
        summary.push_str(&format!(
            "### Mantel–Haenszel summary across {} strata\n\n",
            s.strata
        ));
        let mut rows = Vec::new();
        if let Some(e) = &s.crude_risk_ratio {
            rows.push(vec!["Crude risk ratio".to_string(), fmt_est(e, 2)]);
        }
        if let Some(e) = &s.mh_risk_ratio {
            rows.push(vec!["Adjusted (MH) risk ratio".into(), fmt_est(e, 2)]);
        }
        if let Some(e) = &s.crude_odds_ratio {
            rows.push(vec!["Crude odds ratio".into(), fmt_est(e, 2)]);
        }
        if let Some(e) = &s.mh_odds_ratio {
            rows.push(vec!["Adjusted (MH) odds ratio".into(), fmt_est(e, 2)]);
        }
        if let Some(t) = &s.mh_test {
            rows.push(vec![
                t.name.clone(),
                format!(
                    "χ² = {:.3}, {}",
                    t.statistic.unwrap_or(f64::NAN),
                    fmt_p(t.p_value)
                ),
            ]);
        }
        if let Some(t) = &s.breslow_day {
            rows.push(vec![
                t.name.clone(),
                format!(
                    "χ² = {:.3}, {}",
                    t.statistic.unwrap_or(f64::NAN),
                    fmt_p(t.p_value)
                ),
            ]);
        }
        summary.push_str(&md::table(&["Measure", "Value"], &rows));
        summary.push('\n');
    }
    let interpretation = interpret(
        design,
        &results,
        stratified.as_ref(),
        confidence,
        &exposure,
        &outcome,
    );
    summary.push_str(&format!("**Interpretation:** {interpretation}\n"));

    let mut caveats = vec![
        "Statistical association is not proof of causation: consider bias (recall, selection), \
         confounding, the dose–response pattern, and biological plausibility."
            .to_string(),
    ];
    let small: bool = args.tables.iter().any(|t| {
        let tb = t.table();
        [tb.a, tb.b, tb.c, tb.d]
            .iter()
            .any(|v| *v > 0 && *v < u64::from(threshold))
    });
    if small {
        caveats.push(format!(
            "Some cells are below your suppression threshold ({threshold}). These counts came from \
             your request and are shown here for analysis; suppress them before publishing \
             (apply_small_cell_suppression)."
        ));
    }
    if design == Design::CaseControl {
        caveats.push(
            "In a case-control study only the odds ratio is valid; risks and risk ratios cannot be \
             estimated because the number of controls was chosen by the investigator."
                .into(),
        );
    }

    Ok(Envelope::new(
        summary,
        TwoByTwoResult {
            design,
            confidence,
            tables: results,
            stratified,
            interpretation,
        },
    )
    .sources([Source::computed(
        "2×2 table methods: Katz (risk ratio) and Woolf (odds ratio) log intervals; Fisher's exact \
             and mid-p tests; conditional MLE odds ratio; Mantel–Haenszel with \
             Robins–Breslow–Greenland and Greenland–Robins variances; Breslow–Day test with \
             Tarone's correction (Rothman, Greenland, Lash. Modern Epidemiology, 3rd ed.)",
        "PHN MCP server calculation (verified against R)",
    )])
    .caveats(caveats)
    .privacy(Privacy {
        suppression_threshold: Some(threshold),
        notes: vec!["Counts were supplied in the request; no local files were read.".into()],
        ..Privacy::none()
    }))
}

/// Format a p-value for display.
pub fn fmt_p(p: f64) -> String {
    if !p.is_finite() {
        "—".into()
    } else if p < 0.001 {
        "p < 0.001".into()
    } else {
        format!("p = {p:.3}")
    }
}

fn interpret(
    design: Design,
    r: &[TwoByTwo],
    s: Option<&Stratified>,
    conf: f64,
    exposure: &str,
    outcome: &str,
) -> String {
    let pctl = format!("{:.0}%", conf * 100.0);
    if let Some(s) = s {
        let est = if design == Design::CaseControl {
            &s.mh_odds_ratio
        } else {
            &s.mh_risk_ratio
        };
        let name = if design == Design::CaseControl {
            "odds ratio"
        } else {
            "risk ratio"
        };
        let mut out = match est {
            Some(e) => format!(
                "Adjusted for the strata, the Mantel–Haenszel {name} is {:.2} ({pctl} CI {:.2}–{}).",
                e.value,
                e.lower,
                e.upper.map_or("∞".into(), |u| format!("{u:.2}"))
            ),
            None => "The summary measure could not be estimated (zero cells).".into(),
        };
        if let Some(bd) = &s.breslow_day
            && bd.p_value < 0.05
        {
            out.push_str(
                " The Breslow–Day test suggests the association differs between strata (effect \
                 modification); report stratum-specific estimates rather than one summary.",
            );
        }
        if let Some(c) = s.crude_vs_adjusted_change_pct
            && c.abs() >= 10.0
        {
            out.push_str(&format!(
                " The crude and adjusted odds ratios differ by {c:.0}%, which suggests confounding \
                 by the stratifying variable."
            ));
        }
        return out;
    }
    let r = &r[0];
    let (est, name) = match design {
        Design::CaseControl => (
            r.odds_ratio.as_ref().or(r.odds_ratio_exact.as_ref()),
            "odds ratio",
        ),
        Design::CrossSectional => (r.risk_ratio.as_ref(), "prevalence ratio"),
        Design::Cohort => (r.risk_ratio.as_ref(), "risk ratio"),
    };
    let Some(e) = est else {
        return "A zero cell prevents a point estimate; describe the attack rates and use the exact \
                odds ratio interval."
            .into();
    };
    let excludes_one = e.lower > 1.0 || e.upper.is_some_and(|u| u < 1.0);
    let (test_name, p) = match r.exact.first() {
        Some(t) => ("Fisher's exact", t.p_value),
        // Exact methods are skipped for very large tables.
        None => (
            "chi-square",
            r.chi_square.first().map(|t| t.p_value).unwrap_or(f64::NAN),
        ),
    };
    let mut out = format!(
        "People with the exposure ({exposure}) had {:.2} times the {} of {outcome} ({pctl} CI {:.2}–{}; \
         {test_name} {}).",
        e.value,
        if design == Design::CaseControl {
            "odds"
        } else {
            "risk"
        },
        e.lower,
        e.upper.map_or("∞".into(), |u| format!("{u:.2}")),
        fmt_p(p)
    );
    out.push_str(if excludes_one {
        " The interval excludes 1, so chance alone is an unlikely explanation."
    } else {
        " The interval includes 1, so the data are compatible with no association."
    });
    if let (Some(a), Some(b)) = (r.risk_exposed, r.risk_unexposed)
        && design == Design::Cohort
    {
        out.push_str(&format!(
            " Attack rates: {} exposed vs {} unexposed.",
            pct(a),
            pct(b)
        ));
    }
    let _ = name;
    out
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

    fn one_table(a: u64, b: u64, c: u64, d: u64) -> TwoByTwoArgs {
        TwoByTwoArgs {
            tables: vec![TableInput {
                exposed_cases: a,
                exposed_noncases: b,
                unexposed_cases: c,
                unexposed_noncases: d,
                label: None,
            }],
            design: None,
            confidence: None,
            exposure: None,
            outcome: None,
        }
    }

    #[test]
    fn very_large_tables_are_analyzed_without_exact_methods() {
        let (_dir, ctx) = ctx();
        // A statewide-size table: about 3.5 million possible arrangements.
        let r = analyze_two_by_two(&ctx, one_table(2_000_000, 5_000_000, 1_500_000, 5_500_000))
            .unwrap();
        assert!(r.data.tables[0].exact.is_empty());
        assert!(r.data.tables[0].risk_ratio.is_some());
        assert!(r.summary.contains("exact methods"), "{}", r.summary);
        assert!(
            r.data.interpretation.contains("chi-square p < 0.001"),
            "{}",
            r.data.interpretation
        );
        // An ordinary table still reports Fisher's exact test.
        let r = analyze_two_by_two(&ctx, one_table(30, 10, 5, 25)).unwrap();
        assert!(r.data.interpretation.contains("Fisher's exact p < 0.001"));
        // Counts no real table could hold are refused.
        let e = analyze_two_by_two(&ctx, one_table(5_000_000_000, 10, 5, 25))
            .unwrap_err()
            .to_string();
        assert!(e.contains("check the counts"), "{e}");
    }
}
