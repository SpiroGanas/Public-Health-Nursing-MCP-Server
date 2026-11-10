//! EPI-10 `run_statistical_test`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::glm::{self, Family, GlmData};
use phn_epi::hypothesis::{self as h, Test};
use phn_epi::two_by_two::{self, Design, Table};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epi::two_by_two::fmt_p;
use crate::localdata::{self, LocalTable};
use crate::md;

/// Tests that can be requested (default: chosen automatically).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestChoice {
    Auto,
    WelchT,
    StudentT,
    PairedT,
    MannWhitney,
    WilcoxonSignedRank,
    Anova,
    WelchAnova,
    KruskalWallis,
    ChiSquare,
    FisherExact,
    Pearson,
    Spearman,
    LinearRegression,
    LogisticRegression,
}

/// Values for one group.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GroupValues {
    pub name: String,
    pub values: Vec<f64>,
}

/// Paired measurements (same people measured twice).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PairedValues {
    pub before: Vec<f64>,
    pub after: Vec<f64>,
}

/// A contingency table of counts.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CountTable {
    pub row_labels: Vec<String>,
    pub column_labels: Vec<String>,
    /// counts[row][column]
    pub counts: Vec<Vec<f64>>,
}

/// Arguments for `run_statistical_test`. Give ONE kind of data: `groups`,
/// `paired`, `table`, `x` and `y`, or a local file with column names.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StatTestArgs {
    /// The question in words (echoed in the answer), e.g. "Do flu-shot clinics differ in wait time?".
    pub question: Option<String>,
    /// Test to run (default auto).
    pub test: Option<TestChoice>,
    /// Inline: two or more groups of measurements.
    pub groups: Option<Vec<GroupValues>>,
    /// Inline: paired measurements.
    pub paired: Option<PairedValues>,
    /// Inline: a contingency table of counts.
    pub table: Option<CountTable>,
    /// Inline: two measurements per person, for correlation or simple regression.
    pub x: Option<Vec<f64>>,
    pub y: Option<Vec<f64>>,
    pub x_name: Option<String>,
    pub y_name: Option<String>,
    /// Local file (CSV/XLSX in /data). Only aggregate results are returned.
    pub path: Option<String>,
    pub sheet: Option<String>,
    /// File: the outcome column.
    pub outcome: Option<String>,
    /// File: a grouping column (categories) to compare.
    pub group: Option<String>,
    /// File: predictor columns for regression (numeric or category).
    pub predictors: Option<Vec<String>>,
    /// File: second measurement column for a paired comparison with `outcome`.
    pub paired_with: Option<String>,
    /// File, logistic regression: the outcome value that counts as the event
    /// (default: "yes"/1, or the less common of two values).
    pub outcome_event: Option<String>,
    /// Confidence level, 0.80–0.999 (default 0.95).
    pub confidence: Option<f64>,
}

/// Summary of one group (suppressed for small groups from local files).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GroupSummary {
    pub name: String,
    pub n: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q1: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q3: Option<f64>,
}

/// An assumption check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssumptionCheck {
    pub check: String,
    pub result: String,
    pub met: bool,
}

/// One regression term.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegressionTerm {
    pub term: String,
    pub estimate: f64,
    pub lower: f64,
    pub upper: f64,
    pub p_value: f64,
    /// Odds ratio and interval (logistic regression).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub odds_ratio: Option<(f64, f64, f64)>,
}

/// Regression results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegressionResult {
    pub kind: String,
    pub n: usize,
    pub terms: Vec<RegressionTerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_squared: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjusted_r_squared: Option<f64>,
    /// Overall test: F (linear) or likelihood-ratio χ² (logistic).
    pub overall: Test,
    pub aic: f64,
    /// Problems with the fit that make the estimates unreliable (for example
    /// a predictor that perfectly separates the outcome).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Result of `run_statistical_test`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatTestResult {
    pub data_description: String,
    pub selected_test: String,
    /// Why this test was chosen.
    pub rationale: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<Test>,
    /// Other tests run as sensitivity checks.
    pub alternatives: Vec<Test>,
    pub assumptions: Vec<AssumptionCheck>,
    pub groups: Vec<GroupSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regression: Option<RegressionResult>,
    pub interpretation: String,
    /// Observations used and excluded for missing values.
    pub n_used: usize,
    pub n_missing: usize,
}

struct Ctx {
    conf: f64,
    local: bool,
    threshold: u32,
}

fn normal_enough(x: &[f64]) -> (bool, Option<Test>) {
    if x.len() >= 30 {
        return (true, h::shapiro_wilk(x));
    }
    match h::shapiro_wilk(x) {
        Some(t) => (t.p_value >= 0.05, Some(t)),
        None => (false, None),
    }
}

fn summarize(name: &str, x: &[f64], c: &Ctx) -> Result<GroupSummary> {
    if c.local && x.len() < c.threshold as usize {
        return Err(PhnError::Privacy(format!(
            "group '{name}' has fewer than {} observations; results for groups this small could \
             identify people. Combine categories or analyze the data locally.",
            c.threshold
        )));
    }
    let d = h::describe(x);
    Ok(GroupSummary {
        name: name.into(),
        n: d.n,
        mean: Some(d.mean),
        sd: (d.n > 1).then_some(d.sd),
        median: Some(d.median),
        q1: Some(d.q1),
        q3: Some(d.q3),
    })
}

/// Choose and run a statistical test.
pub fn run_statistical_test(ctx: &Context, args: StatTestArgs) -> Result<Envelope<StatTestResult>> {
    let conf = args.confidence.unwrap_or(0.95);
    if !(0.80..=0.999).contains(&conf) {
        return Err(PhnError::InvalidInput(
            "confidence must be between 0.80 and 0.999".into(),
        ));
    }
    let threshold = ctx.suppression_threshold();
    let choice = args.test.unwrap_or(TestChoice::Auto);
    let modes = [
        args.groups.is_some(),
        args.paired.is_some(),
        args.table.is_some(),
        args.x.is_some() || args.y.is_some(),
        args.path.is_some(),
    ];
    if modes.iter().filter(|m| **m).count() != 1 {
        return Err(PhnError::InvalidInput(
            "give exactly one kind of data: groups, paired, table, x and y, or a local file (path)"
                .into(),
        ));
    }
    let c = Ctx {
        conf,
        local: args.path.is_some(),
        threshold,
    };
    let mut sources = vec![Source::computed(
        "Classical tests as implemented in R (t.test, wilcox.test, oneway.test, kruskal.test, \
         chisq.test, fisher.test, cor.test, shapiro.test, glm); Royston (1995) Shapiro–Wilk",
        "PHN MCP server calculation (verified against R)",
    )];
    let mut privacy = Privacy {
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    };
    let mut n_missing = 0;

    let mut res = if let Some(t) = &args.table {
        let counts = t.counts.clone();
        contingency(&counts, &t.row_labels, &t.column_labels, choice, &c, None)?
    } else if let Some(g) = &args.groups {
        let groups: Vec<(String, Vec<f64>)> = g
            .iter()
            .map(|g| (g.name.clone(), g.values.clone()))
            .collect();
        compare_groups(&groups, choice, &c, "measurement")?
    } else if let Some(p) = &args.paired {
        if p.before.len() != p.after.len() {
            return Err(PhnError::InvalidInput(
                "before and after need the same number of values".into(),
            ));
        }
        paired(&p.before, &p.after, choice, &c)?
    } else if args.x.is_some() || args.y.is_some() {
        let (x, y) = (
            args.x.clone().unwrap_or_default(),
            args.y.clone().unwrap_or_default(),
        );
        if x.len() != y.len() || x.len() < 4 {
            return Err(PhnError::InvalidInput(
                "x and y need the same number of values (at least 4)".into(),
            ));
        }
        let xn = args.x_name.clone().unwrap_or_else(|| "x".into());
        let yn = args.y_name.clone().unwrap_or_else(|| "y".into());
        correlate(&x, &y, &xn, &yn, choice, &c)?
    } else {
        let lt = localdata::load(
            ctx,
            args.path.as_deref().unwrap_or_default(),
            args.sheet.as_deref(),
        )?;
        sources.push(lt.source());
        privacy.identifiers_detected = lt.identifier_categories();
        privacy.notes.push(
            "Only aggregate results are returned; no rows, individual values, minimums, or \
             maximums from the file."
                .into(),
        );
        let (r, miss) = from_file(&lt, &args, choice, &c)?;
        n_missing = miss;
        privacy.suppression_applied = r.1;
        r.0
    };
    res.n_missing = n_missing;

    // Summary.
    let mut s = String::new();
    if let Some(q) = &args.question {
        s.push_str(&format!("**Question:** {q}\n\n"));
    }
    s.push_str(&format!(
        "**Data:** {}\n\n**Test:** {} — {}\n\n",
        res.data_description, res.selected_test, res.rationale
    ));
    if !res.groups.is_empty() {
        let rows: Vec<Vec<String>> = res
            .groups
            .iter()
            .map(|g| {
                vec![
                    g.name.clone(),
                    g.n.to_string(),
                    g.mean.map(|v| md::num(v, 2)).unwrap_or_default(),
                    g.sd.map(|v| md::num(v, 2)).unwrap_or_default(),
                    g.median.map(|v| md::num(v, 2)).unwrap_or_default(),
                    match (g.q1, g.q3) {
                        (Some(a), Some(b)) => format!("{} – {}", md::num(a, 2), md::num(b, 2)),
                        _ => String::new(),
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Group", "n", "Mean", "SD", "Median", "IQR"],
            &rows,
        ));
        s.push('\n');
    }
    if let Some(p) = &res.primary {
        s.push_str(&format!("**Result:** {}\n\n", describe_test(p, conf)));
    }
    if let Some(rg) = &res.regression {
        s.push_str(&regression_markdown(rg, conf));
    }
    if !res.alternatives.is_empty() {
        s.push_str("**Sensitivity checks**\n");
        for a in &res.alternatives {
            s.push_str(&format!("- {}\n", describe_test(a, conf)));
        }
        s.push('\n');
    }
    if !res.assumptions.is_empty() {
        s.push_str("**Assumptions**\n");
        for a in &res.assumptions {
            s.push_str(&format!(
                "- {} {}: {}\n",
                if a.met { "✓" } else { "⚠" },
                a.check,
                a.result
            ));
        }
        s.push('\n');
    }
    s.push_str(&format!("**Interpretation:** {}\n", res.interpretation));

    let caveats = vec![
        "A p-value below 0.05 means the data would be unusual if there were no real difference; it \
         does not measure how large or important the difference is. Look at the estimate and its \
         interval."
            .to_string(),
        "Tests assume independent observations (one record per person, unless paired). Clustered \
         data (children within schools, repeated visits) need methods that account for clustering."
            .into(),
    ];
    Ok(Envelope::new(s, res)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn describe_test(t: &Test, conf: f64) -> String {
    let mut out = format!(
        "{}: {} = {}",
        t.name,
        t.statistic_name,
        md::num(t.statistic, 3)
    );
    if let Some(df) = t.df {
        out.push_str(&format!(
            ", df = {}",
            md::num(df, if (df - df.round()).abs() < 1e-9 { 0 } else { 1 })
        ));
        if let Some(d2) = t.df2 {
            out.push_str(&format!(
                " and {}",
                md::num(d2, if (d2 - d2.round()).abs() < 1e-9 { 0 } else { 1 })
            ));
        }
    }
    out.push_str(&format!(", {}", fmt_p(t.p_value)));
    if let Some(e) = t.estimate {
        out.push_str(&format!("; estimate {}", md::num(e, 3)));
        if let (Some(l), Some(u)) = (t.lower, t.upper) {
            out.push_str(&format!(
                " ({:.0}% CI {} to {})",
                conf * 100.0,
                md::num(l, 3),
                md::num(u, 3)
            ));
        }
    }
    if let (Some(es), Some(n)) = (t.effect_size, &t.effect_size_name) {
        out.push_str(&format!("; {n} = {}", md::num(es, 2)));
    }
    out.push_str(&format!(" [{}]", t.method));
    out
}

fn base(desc: String, selected: &str, why: String) -> StatTestResult {
    StatTestResult {
        data_description: desc,
        selected_test: selected.into(),
        rationale: why,
        primary: None,
        alternatives: Vec::new(),
        assumptions: Vec::new(),
        groups: Vec::new(),
        regression: None,
        interpretation: String::new(),
        n_used: 0,
        n_missing: 0,
    }
}

fn sig_text(p: f64) -> &'static str {
    if p.is_nan() {
        "could not be tested because the data have no variation"
    } else if p < 0.05 {
        "is statistically significant at the 5% level"
    } else {
        "is not statistically significant at the 5% level"
    }
}

fn compare_groups(
    groups: &[(String, Vec<f64>)],
    choice: TestChoice,
    c: &Ctx,
    what: &str,
) -> Result<StatTestResult> {
    if groups.len() < 2 || groups.len() > 20 {
        return Err(PhnError::InvalidInput("compare 2 to 20 groups".into()));
    }
    for (n, v) in groups {
        if v.len() < 2 || v.iter().any(|x| !x.is_finite()) {
            return Err(PhnError::InvalidInput(format!(
                "group '{n}' needs at least 2 numeric values"
            )));
        }
    }
    let summaries: Vec<GroupSummary> = groups
        .iter()
        .map(|(n, v)| summarize(n, v, c))
        .collect::<Result<_>>()?;
    let mut assumptions = Vec::new();
    let mut all_normal = true;
    for (n, v) in groups {
        let (ok, t) = normal_enough(v);
        all_normal &= ok;
        assumptions.push(AssumptionCheck {
            check: format!("Normality in '{n}'"),
            result: match &t {
                Some(t) if v.len() >= 30 => format!("n = {} (≥ 30, so the mean is approximately normal); Shapiro–Wilk W = {:.3}, {}", v.len(), t.statistic, fmt_p(t.p_value)),
                Some(t) => format!("Shapiro–Wilk W = {:.3}, {}", t.statistic, fmt_p(t.p_value)),
                None => "could not be checked".into(),
            },
            met: ok,
        });
    }
    let vals: Vec<Vec<f64>> = groups.iter().map(|g| g.1.clone()).collect();
    let bf = h::brown_forsythe(&vals);
    let equal_var = bf.p_value >= 0.05;
    assumptions.push(AssumptionCheck {
        check: "Equal variances".into(),
        result: format!(
            "Levene (median-centered) F = {:.2}, {}",
            bf.statistic,
            fmt_p(bf.p_value)
        ),
        met: equal_var,
    });
    let n_used: usize = vals.iter().map(Vec::len).sum();
    let desc = format!("{} groups of {what} (n = {n_used})", groups.len());
    let mut r;
    if groups.len() == 2 {
        let (x, y) = (&vals[0], &vals[1]);
        let picked = match choice {
            TestChoice::Auto => {
                if all_normal {
                    TestChoice::WelchT
                } else {
                    TestChoice::MannWhitney
                }
            }
            TestChoice::WelchT | TestChoice::StudentT | TestChoice::MannWhitney => choice,
            other => {
                return Err(PhnError::InvalidInput(format!(
                    "{other:?} does not apply to two independent groups"
                )));
            }
        };
        let run = |t: TestChoice| match t {
            TestChoice::StudentT => h::t_test(x, y, true, c.conf),
            TestChoice::MannWhitney => h::mann_whitney(x, y),
            _ => h::t_test(x, y, false, c.conf),
        };
        let why = match (choice, picked) {
            (TestChoice::Auto, TestChoice::WelchT) => "two independent groups with approximately normal data; Welch's t test does not assume equal variances".to_string(),
            (TestChoice::Auto, _) => "two independent groups where the data are not approximately normal, so a rank-based test is safer".to_string(),
            _ => "requested".to_string(),
        };
        r = base(desc, &format!("{picked:?}"), why);
        let p = run(picked);
        let alts: Vec<TestChoice> = [TestChoice::WelchT, TestChoice::MannWhitney]
            .into_iter()
            .filter(|t| *t != picked)
            .collect();
        r.alternatives = alts.into_iter().map(run).collect();
        r.interpretation = format!(
            "The difference between '{}' and '{}' {} ({}).{}",
            groups[0].0,
            groups[1].0,
            sig_text(p.p_value),
            fmt_p(p.p_value),
            p.estimate
                .map(|e| format!(
                    " Mean difference {} ({:.0}% CI {} to {}).",
                    md::num(e, 2),
                    c.conf * 100.0,
                    md::num(p.lower.unwrap_or(f64::NAN), 2),
                    md::num(p.upper.unwrap_or(f64::NAN), 2)
                ))
                .unwrap_or_else(|| format!(
                    " Medians: {} vs {}.",
                    md::num(summaries[0].median.unwrap_or(f64::NAN), 2),
                    md::num(summaries[1].median.unwrap_or(f64::NAN), 2)
                ))
        );
        r.primary = Some(p);
    } else {
        let picked = match choice {
            TestChoice::Auto => {
                if !all_normal {
                    TestChoice::KruskalWallis
                } else if equal_var {
                    TestChoice::Anova
                } else {
                    TestChoice::WelchAnova
                }
            }
            TestChoice::Anova | TestChoice::WelchAnova | TestChoice::KruskalWallis => choice,
            other => {
                return Err(PhnError::InvalidInput(format!(
                    "{other:?} does not apply to three or more groups"
                )));
            }
        };
        let run = |t: TestChoice| match t {
            TestChoice::Anova => h::anova(&vals),
            TestChoice::WelchAnova => h::welch_anova(&vals),
            _ => h::kruskal_wallis(&vals),
        };
        let why = match (choice, picked) {
            (TestChoice::Auto, TestChoice::Anova) => {
                "three or more groups, approximately normal with similar variances"
            }
            (TestChoice::Auto, TestChoice::WelchAnova) => {
                "three or more groups, approximately normal but with unequal variances"
            }
            (TestChoice::Auto, _) => {
                "three or more groups where the data are not approximately normal"
            }
            _ => "requested",
        };
        r = base(desc, &format!("{picked:?}"), why.into());
        let p = run(picked);
        r.alternatives = [
            TestChoice::Anova,
            TestChoice::WelchAnova,
            TestChoice::KruskalWallis,
        ]
        .into_iter()
        .filter(|t| *t != picked)
        .map(run)
        .collect();
        r.interpretation = format!(
            "Across the {} groups, the difference {} ({}). A significant overall test says at least one group differs; it does not say which — compare specific pairs with an adjusted significance level (e.g. Bonferroni).",
            groups.len(),
            sig_text(p.p_value),
            fmt_p(p.p_value)
        );
        r.primary = Some(p);
    }
    r.assumptions = assumptions;
    r.groups = summaries;
    r.n_used = n_used;
    Ok(r)
}

fn paired(x: &[f64], y: &[f64], choice: TestChoice, c: &Ctx) -> Result<StatTestResult> {
    if x.len() < 3 {
        return Err(PhnError::InvalidInput(
            "a paired comparison needs at least 3 pairs".into(),
        ));
    }
    let d: Vec<f64> = x.iter().zip(y).map(|(a, b)| a - b).collect();
    let (normal, sw) = normal_enough(&d);
    let picked = match choice {
        TestChoice::Auto => {
            if normal {
                TestChoice::PairedT
            } else {
                TestChoice::WilcoxonSignedRank
            }
        }
        TestChoice::PairedT | TestChoice::WilcoxonSignedRank => choice,
        other => {
            return Err(PhnError::InvalidInput(format!(
                "{other:?} does not apply to paired data"
            )));
        }
    };
    let run = |t: TestChoice| {
        if t == TestChoice::PairedT {
            h::paired_t_test(x, y, c.conf)
        } else {
            h::wilcoxon_signed_rank(x, y)
        }
    };
    let mut r = base(
        format!("{} pairs (first − second)", x.len()),
        &format!("{picked:?}"),
        if choice != TestChoice::Auto {
            "requested".into()
        } else if normal {
            "paired measurements whose differences are approximately normal".into()
        } else {
            "paired measurements whose differences are not normal".into()
        },
    );
    r.groups = vec![
        summarize("First", x, c)?,
        summarize("Second", y, c)?,
        summarize("Difference", &d, c)?,
    ];
    r.assumptions.push(AssumptionCheck {
        check: "Normality of the differences".into(),
        result: sw
            .map(|t| format!("Shapiro–Wilk W = {:.3}, {}", t.statistic, fmt_p(t.p_value)))
            .unwrap_or_else(|| "could not be checked".into()),
        met: normal,
    });
    let p = run(picked);
    r.alternatives = vec![run(if picked == TestChoice::PairedT {
        TestChoice::WilcoxonSignedRank
    } else {
        TestChoice::PairedT
    })];
    r.interpretation = format!(
        "The change between the two measurements {} ({}).",
        sig_text(p.p_value),
        fmt_p(p.p_value)
    );
    r.primary = Some(p);
    r.n_used = x.len();
    Ok(r)
}

fn correlate(
    x: &[f64],
    y: &[f64],
    xn: &str,
    yn: &str,
    choice: TestChoice,
    c: &Ctx,
) -> Result<StatTestResult> {
    if choice == TestChoice::LinearRegression {
        let data = GlmData {
            y: y.to_vec(),
            x: x.iter().map(|v| vec![1.0, *v]).collect(),
            names: vec!["(Intercept)".into(), xn.into()],
            ..GlmData::default()
        };
        let mut r = base(
            format!("{} pairs of {xn} and {yn}", x.len()),
            "LinearRegression",
            "requested".into(),
        );
        r.regression = Some(regression(&data, Family::Gaussian, c.conf)?);
        r.interpretation =
            regression_interpretation(r.regression.as_ref().unwrap_or_else(|| unreachable!()));
        r.n_used = x.len();
        return Ok(r);
    }
    let constant = |v: &[f64]| v.windows(2).all(|w| w[0] == w[1]);
    for (name, v) in [(xn, x), (yn, y)] {
        if constant(v) {
            return Err(PhnError::InvalidInput(format!(
                "every value of {name} is the same, so a correlation cannot be calculated"
            )));
        }
    }
    let (nx, _) = normal_enough(x);
    let (ny, _) = normal_enough(y);
    let picked = match choice {
        TestChoice::Auto => {
            if nx && ny {
                TestChoice::Pearson
            } else {
                TestChoice::Spearman
            }
        }
        TestChoice::Pearson | TestChoice::Spearman => choice,
        other => {
            return Err(PhnError::InvalidInput(format!(
                "{other:?} does not apply to two measurements per person"
            )));
        }
    };
    let run = |t: TestChoice| {
        if t == TestChoice::Pearson {
            h::pearson(x, y, c.conf)
        } else {
            h::spearman(x, y)
        }
    };
    let mut r = base(
        format!("{} pairs of {xn} and {yn}", x.len()),
        &format!("{picked:?}"),
        if choice != TestChoice::Auto {
            "requested".into()
        } else if picked == TestChoice::Pearson {
            "both measurements are approximately normal (linear association)".into()
        } else {
            "at least one measurement is not normal, so a rank correlation (monotonic association) is safer".into()
        },
    );
    r.assumptions.push(AssumptionCheck {
        check: format!("Normality of {xn} and {yn}"),
        result: format!(
            "{}, {}",
            if nx {
                "approximately normal"
            } else {
                "not normal"
            },
            if ny {
                "approximately normal"
            } else {
                "not normal"
            }
        ),
        met: nx && ny,
    });
    let p = run(picked);
    let rho = p.estimate.unwrap_or(f64::NAN);
    let strength = match rho.abs() {
        a if a < 0.1 => "negligible",
        a if a < 0.3 => "weak",
        a if a < 0.5 => "moderate",
        _ => "strong",
    };
    r.interpretation = format!(
        "{strength} {} correlation (r = {:.2}); it {} ({}). Correlation does not show cause and effect.",
        if rho >= 0.0 { "positive" } else { "negative" },
        rho,
        sig_text(p.p_value),
        fmt_p(p.p_value)
    );
    r.alternatives = vec![run(if picked == TestChoice::Pearson {
        TestChoice::Spearman
    } else {
        TestChoice::Pearson
    })];
    r.primary = Some(p);
    r.n_used = x.len();
    Ok(r)
}

fn contingency(
    counts: &[Vec<f64>],
    rows: &[String],
    cols: &[String],
    choice: TestChoice,
    c: &Ctx,
    hidden: Option<&dyn Fn(usize, usize) -> bool>,
) -> Result<StatTestResult> {
    if counts.len() < 2 || counts.iter().any(|r| r.len() != counts[0].len()) || counts[0].len() < 2
    {
        return Err(PhnError::InvalidInput(
            "a contingency table needs at least 2 rows and 2 columns of equal length".into(),
        ));
    }
    if counts
        .iter()
        .flatten()
        .any(|v| *v < 0.0 || (v - v.round()).abs() > 1e-9)
    {
        return Err(PhnError::InvalidInput(
            "table cells must be whole-number counts".into(),
        ));
    }
    if rows.len() != counts.len() || cols.len() != counts[0].len() {
        return Err(PhnError::InvalidInput(
            "give one label per row and per column".into(),
        ));
    }
    let (chi, min_e) = h::chi_square_table(counts);
    let is22 = counts.len() == 2 && counts[0].len() == 2;
    let picked = match choice {
        TestChoice::Auto => {
            if is22 && min_e < 5.0 {
                TestChoice::FisherExact
            } else {
                TestChoice::ChiSquare
            }
        }
        TestChoice::FisherExact if !is22 => {
            return Err(PhnError::InvalidInput(
                "Fisher's exact test here is for 2×2 tables; use chi_square for larger tables"
                    .into(),
            ));
        }
        TestChoice::ChiSquare | TestChoice::FisherExact => choice,
        other => {
            return Err(PhnError::InvalidInput(format!(
                "{other:?} does not apply to a table of counts"
            )));
        }
    };
    let fisher = is22.then(|| {
        let t = Table::new(
            counts[0][0] as u64,
            counts[0][1] as u64,
            counts[1][0] as u64,
            counts[1][1] as u64,
        );
        let a = two_by_two::analyze(t, Design::CrossSectional, c.conf);
        let p = a.exact.first().map(|x| x.p_value).unwrap_or(f64::NAN);
        let mut ft = Test {
            name: "Fisher's exact test (two-sided)".into(),
            statistic: a
                .odds_ratio_exact
                .as_ref()
                .map(|o| o.value)
                .unwrap_or(f64::NAN),
            statistic_name: "conditional MLE odds ratio".into(),
            df: None,
            df2: None,
            p_value: p,
            estimate: a.odds_ratio_exact.as_ref().map(|o| o.value),
            lower: a.odds_ratio_exact.as_ref().map(|o| o.lower),
            upper: a.odds_ratio_exact.as_ref().and_then(|o| o.upper),
            effect_size: None,
            effect_size_name: None,
            method: "exact (hypergeometric)".into(),
        };
        if !ft.statistic.is_finite() {
            ft.statistic = 0.0;
        }
        ft
    });
    let n: f64 = counts.iter().flatten().sum();
    let cells = counts.len() * counts[0].len();
    let low = counts
        .iter()
        .enumerate()
        .flat_map(|(i, r)| r.iter().enumerate().map(move |(j, _)| (i, j)))
        .filter(|(i, j)| {
            let rs: f64 = counts[*i].iter().sum();
            let cs: f64 = counts.iter().map(|r| r[*j]).sum();
            rs * cs / n < 5.0
        })
        .count();
    let mut r = base(
        format!(
            "{}×{} table of counts (n = {})",
            counts.len(),
            counts[0].len(),
            n as u64
        ),
        &format!("{picked:?}"),
        match (choice, picked) {
            (TestChoice::Auto, TestChoice::FisherExact) => {
                "2×2 table with an expected count below 5, so the exact test is used".into()
            }
            (TestChoice::Auto, _) => "table of counts with adequate expected counts".into(),
            _ => "requested".into(),
        },
    );
    r.assumptions.push(AssumptionCheck {
        check: "Expected counts".into(),
        result: format!("smallest expected count {min_e:.1}; {low} of {cells} cells below 5"),
        met: low as f64 <= 0.2 * cells as f64 && min_e >= 1.0,
    });
    if !is22 && (low as f64 > 0.2 * cells as f64 || min_e < 1.0) {
        r.assumptions.push(AssumptionCheck {
            check: "Chi-square approximation".into(),
            result: "more than 20% of cells have expected counts below 5 (or one below 1); combine sparse categories".into(),
            met: false,
        });
    }
    let primary = if picked == TestChoice::FisherExact {
        fisher.clone().unwrap_or(chi.clone())
    } else {
        chi.clone()
    };
    if picked == TestChoice::ChiSquare
        && let Some(f) = fisher
    {
        r.alternatives.push(f);
    } else if picked == TestChoice::FisherExact {
        r.alternatives.push(chi.clone());
    }
    // Table of percentages by row (cells hidden by suppression are not shown).
    let mut rows_out = Vec::new();
    for (i, row) in counts.iter().enumerate() {
        let rs: f64 = row.iter().sum();
        let mut cells_out = vec![rows[i].clone()];
        for (j, v) in row.iter().enumerate() {
            let hide = hidden.is_some_and(|f| f(i, j));
            cells_out.push(if hide {
                "suppressed".into()
            } else {
                format!("{} ({:.0}%)", *v as u64, 100.0 * v / rs.max(1.0))
            });
        }
        rows_out.push(cells_out);
    }
    let mut heads: Vec<&str> = vec![""];
    heads.extend(cols.iter().map(String::as_str));
    r.data_description
        .push_str(&format!("\n\n{}", md::table(&heads, &rows_out)));
    r.interpretation = format!(
        "The association between rows and columns {} ({}). Cramér's V = {:.2} ({}).",
        sig_text(primary.p_value),
        fmt_p(primary.p_value),
        chi.effect_size.unwrap_or(f64::NAN),
        match chi.effect_size.unwrap_or(0.0) {
            v if v < 0.1 => "negligible",
            v if v < 0.3 => "small",
            v if v < 0.5 => "medium",
            _ => "large",
        }
    );
    r.primary = Some(primary);
    r.n_used = n as usize;
    let _ = c.local;
    Ok(r)
}

fn regression(data: &GlmData, family: Family, conf: f64) -> Result<RegressionResult> {
    let f = glm::fit(data, family, conf).map_err(PhnError::InvalidInput)?;
    let mut warnings = Vec::new();
    if f.has_separation() {
        warnings.push(
            "A predictor (or a combination of predictors) predicts the outcome perfectly or \
             almost perfectly (separation). The odds ratios, confidence intervals, and p-values \
             below cannot be trusted: a huge odds ratio with a p-value near 1 is a symptom, not \
             a finding. For one yes/no predictor use Fisher's exact test (analyze_two_by_two); \
             otherwise combine sparse categories or drop the predictor."
                .to_string(),
        );
    } else if !f.converged {
        warnings.push(
            "The model did not converge, so the estimates may be unreliable. Simplify the \
             model (fewer predictors or combined categories) or collect more data."
                .to_string(),
        );
    }
    let p = f.coefficients.len() as f64;
    let n = f.n as f64;
    let terms = f
        .coefficients
        .iter()
        .map(|c| RegressionTerm {
            term: c.name.clone(),
            estimate: c.estimate,
            lower: c.lower,
            upper: c.upper,
            p_value: c.p_value,
            odds_ratio: (family == Family::Binomial)
                .then(|| (c.estimate.exp(), c.lower.exp(), c.upper.exp())),
        })
        .collect();
    let (r2, adj, overall) = if family == Family::Gaussian {
        let r2 = 1.0 - f.deviance / f.null_deviance;
        let fstat = ((f.null_deviance - f.deviance) / (p - 1.0)) / (f.deviance / (n - p));
        let mut t = Test {
            name: "Overall F test".into(),
            statistic: fstat,
            statistic_name: "F".into(),
            df: Some(p - 1.0),
            df2: Some(n - p),
            p_value: phn_epi::dist::p_f(fstat, p - 1.0, n - p),
            estimate: None,
            lower: None,
            upper: None,
            effect_size: Some(r2),
            effect_size_name: Some("R²".into()),
            method: "F distribution".into(),
        };
        if p < 2.0 {
            t.p_value = f64::NAN;
        }
        (Some(r2), Some(1.0 - (1.0 - r2) * (n - 1.0) / (n - p)), t)
    } else {
        let lr = f.null_deviance - f.deviance;
        (
            None,
            None,
            Test {
                name: "Likelihood-ratio test (all predictors)".into(),
                statistic: lr,
                statistic_name: "χ²".into(),
                df: Some(p - 1.0),
                df2: None,
                p_value: phn_epi::dist::p_chi_square(lr, p - 1.0),
                estimate: None,
                lower: None,
                upper: None,
                effect_size: None,
                effect_size_name: None,
                method: "chi-square".into(),
            },
        )
    };
    Ok(RegressionResult {
        kind: if family == Family::Gaussian {
            "linear (ordinary least squares)".into()
        } else {
            "logistic".into()
        },
        n: f.n,
        terms,
        r_squared: r2,
        adjusted_r_squared: adj,
        overall,
        aic: f.aic,
        warnings,
    })
}

fn regression_markdown(r: &RegressionResult, conf: f64) -> String {
    let logistic = r.terms.iter().any(|t| t.odds_ratio.is_some());
    let pctl = format!("{:.0}%", conf * 100.0);
    let rows: Vec<Vec<String>> = r
        .terms
        .iter()
        .map(|t| {
            let est = match t.odds_ratio {
                Some((or, l, u)) if t.term != "(Intercept)" => format!(
                    "OR {} ({}–{})",
                    md::num(or, 2),
                    md::num(l, 2),
                    md::num(u, 2)
                ),
                _ => format!(
                    "{} ({} to {})",
                    md::num(t.estimate, 3),
                    md::num(t.lower, 3),
                    md::num(t.upper, 3)
                ),
            };
            vec![t.term.clone(), est, fmt_p(t.p_value)]
        })
        .collect();
    let mut s = format!(
        "**{} regression** (n = {})\n\n",
        if logistic { "Logistic" } else { "Linear" },
        r.n
    );
    for w in &r.warnings {
        s.push_str(&format!("> **Warning:** {w}\n\n"));
    }
    s.push_str(&md::table(
        &["Term", &format!("Estimate ({pctl} CI)"), "p"],
        &rows,
    ));
    if let (Some(r2), Some(a)) = (r.r_squared, r.adjusted_r_squared) {
        s.push_str(&format!("\nR² = {r2:.3} (adjusted {a:.3}); "));
    } else {
        s.push('\n');
    }
    s.push_str(&format!(
        "{}: {} = {:.2}, {}\n\n",
        r.overall.name,
        r.overall.statistic_name,
        r.overall.statistic,
        fmt_p(r.overall.p_value)
    ));
    s
}

fn regression_interpretation(r: &RegressionResult) -> String {
    if !r.warnings.is_empty() {
        return format!("**Do not interpret these estimates.** {}", r.warnings.join(" "));
    }
    let sig: Vec<&str> = r
        .terms
        .iter()
        .filter(|t| t.term != "(Intercept)" && t.p_value < 0.05)
        .map(|t| t.term.as_str())
        .collect();
    let mut out = if sig.is_empty() {
        "No predictor is statistically significant at the 5% level when the others are held constant.".to_string()
    } else {
        format!(
            "Predictors significant at the 5% level, holding the others constant: {}.",
            sig.join(", ")
        )
    };
    if r.terms.iter().any(|t| t.odds_ratio.is_some()) {
        out.push_str(" Odds ratios above 1 mean higher odds of the outcome; for a numeric predictor, per one-unit increase.");
    } else {
        out.push_str(" Each estimate is the average change in the outcome per one-unit change in the predictor (or versus the reference category).");
    }
    out
}

/// File mode. Returns (result, whether suppression hid any cells) and missing count.
fn from_file(
    lt: &LocalTable,
    a: &StatTestArgs,
    choice: TestChoice,
    c: &Ctx,
) -> Result<((StatTestResult, bool), usize)> {
    let outcome = a
        .outcome
        .as_deref()
        .ok_or_else(|| PhnError::InvalidInput("give the outcome column".into()))?;
    let oi = lt.column(outcome)?;
    let (onum, ofrac) = lt.numeric(oi);
    let numeric_outcome = ofrac >= 0.95
        && !matches!(
            choice,
            TestChoice::ChiSquare | TestChoice::FisherExact | TestChoice::LogisticRegression
        );

    // Paired.
    if let Some(pw) = &a.paired_with {
        let pi = lt.column(pw)?;
        let (pnum, _) = lt.numeric(pi);
        let pairs: Vec<(f64, f64)> = onum
            .iter()
            .zip(&pnum)
            .filter_map(|(x, y)| Some(((*x)?, (*y)?)))
            .collect();
        let miss = lt.table.row_count() - pairs.len();
        let (x, y): (Vec<f64>, Vec<f64>) = pairs.into_iter().unzip();
        let mut r = paired(&x, &y, choice, c)?;
        r.data_description = format!(
            "'{outcome}' vs '{pw}' in {} ({} complete pairs)",
            lt.label,
            x.len()
        );
        return Ok(((r, false), miss));
    }

    // Regression.
    if let Some(preds) = a.predictors.as_ref().filter(|p| !p.is_empty()) {
        let logistic = choice == TestChoice::LogisticRegression
            || (!numeric_outcome && choice != TestChoice::LinearRegression);
        let (y, event) = if logistic {
            let cats = lt.categorical(oi)?;
            let mut levels: Vec<String> = cats.iter().flatten().cloned().collect();
            levels.sort();
            levels.dedup();
            let event = match &a.outcome_event {
                Some(e) => levels
                    .iter()
                    .find(|l| l.eq_ignore_ascii_case(e))
                    .cloned()
                    .ok_or_else(|| {
                        PhnError::InvalidInput(format!("'{e}' is not a value of '{outcome}'"))
                    })?,
                None => {
                    if let Some(l) = levels.iter().find(|l| localdata::truthy(l) == Some(true)) {
                        l.clone()
                    } else if levels.len() == 2 {
                        let count = |l: &String| cats.iter().flatten().filter(|v| *v == l).count();
                        if count(&levels[0]) <= count(&levels[1]) {
                            levels[0].clone()
                        } else {
                            levels[1].clone()
                        }
                    } else {
                        return Err(PhnError::InvalidInput(format!(
                            "'{outcome}' has {} values; logistic regression needs two (give outcome_event)",
                            levels.len()
                        )));
                    }
                }
            };
            (
                cats.iter()
                    .map(|v| v.as_ref().map(|v| if *v == event { 1.0 } else { 0.0 }))
                    .collect::<Vec<_>>(),
                Some(event),
            )
        } else {
            (onum.clone(), None)
        };
        // Build the design matrix.
        let mut names = vec!["(Intercept)".to_string()];
        let mut cols: Vec<Vec<Option<f64>>> = Vec::new();
        for p in preds {
            let pi = lt.column(p)?;
            let (num, frac) = lt.numeric(pi);
            if frac >= 0.95 {
                names.push(p.clone());
                cols.push(num);
            } else {
                let cats = lt.categorical(pi)?;
                let mut counts: Vec<(String, usize)> = Vec::new();
                for v in cats.iter().flatten() {
                    match counts.iter_mut().find(|(l, _)| l == v) {
                        Some(x) => x.1 += 1,
                        None => counts.push((v.clone(), 1)),
                    }
                }
                counts.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
                if let Some((l, n)) = counts.iter().find(|(_, n)| *n < c.threshold as usize) {
                    let _ = n;
                    return Err(PhnError::Privacy(format!(
                        "category '{l}' of '{p}' has fewer than {} records; combine it with another category first",
                        c.threshold
                    )));
                }
                for (lvl, _) in counts.iter().skip(1) {
                    names.push(format!("{p}: {lvl} (vs {})", counts[0].0));
                    cols.push(
                        cats.iter()
                            .map(|v| v.as_ref().map(|v| if v == lvl { 1.0 } else { 0.0 }))
                            .collect(),
                    );
                }
            }
        }
        let mut yv = Vec::new();
        let mut xv = Vec::new();
        for i in 0..lt.table.row_count() {
            if let Some(yi) = y[i] {
                let row: Option<Vec<f64>> = std::iter::once(Some(1.0))
                    .chain(cols.iter().map(|c| c[i]))
                    .collect();
                if let Some(row) = row {
                    yv.push(yi);
                    xv.push(row);
                }
            }
        }
        let miss = lt.table.row_count() - yv.len();
        if yv.len() < c.threshold as usize {
            return Err(PhnError::Privacy(
                "too few complete records for a regression".into(),
            ));
        }
        let family = if logistic {
            Family::Binomial
        } else {
            Family::Gaussian
        };
        let data = GlmData {
            trials: logistic.then(|| vec![1.0; yv.len()]),
            y: yv.clone(),
            x: xv,
            names: names.clone(),
            offset: None,
        };
        let reg = regression(&data, family, c.conf)?;
        let mut r = base(
            format!(
                "{} complete records from {}; outcome '{outcome}'{}",
                yv.len(),
                lt.label,
                event
                    .as_ref()
                    .map(|e| format!(" (event = \"{e}\")"))
                    .unwrap_or_default()
            ),
            if logistic {
                "LogisticRegression"
            } else {
                "LinearRegression"
            },
            if logistic {
                "binary outcome with several predictors".into()
            } else {
                "numeric outcome with several predictors".into()
            },
        );
        if logistic {
            let events = yv
                .iter()
                .filter(|v| **v > 0.5)
                .count()
                .min(yv.iter().filter(|v| **v < 0.5).count());
            let epv = events as f64 / (names.len() - 1).max(1) as f64;
            r.assumptions.push(AssumptionCheck {
                check: "Events per predictor".into(),
                result: format!("{epv:.1} (at least 10 recommended)"),
                met: epv >= 10.0,
            });
        }
        r.interpretation = regression_interpretation(&reg);
        r.regression = Some(reg);
        r.n_used = yv.len();
        return Ok(((r, false), miss));
    }

    let gname = a.group.as_deref().ok_or_else(|| {
        PhnError::InvalidInput("give a group column (or predictors, or paired_with)".into())
    })?;
    let gi = lt.column(gname)?;
    let gcats = lt.categorical(gi)?;
    if numeric_outcome {
        let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
        let mut miss = 0;
        for (g, v) in gcats.iter().zip(&onum) {
            match (g, v) {
                (Some(g), Some(v)) => match groups.iter_mut().find(|x| &x.0 == g) {
                    Some(x) => x.1.push(*v),
                    None => groups.push((g.clone(), vec![*v])),
                },
                _ => miss += 1,
            }
        }
        groups.sort_by(|x, y| x.0.cmp(&y.0));
        let mut r = compare_groups(&groups, choice, c, &format!("'{outcome}'"))?;
        r.data_description = format!(
            "'{outcome}' by '{gname}' in {} ({} groups, n = {})",
            lt.label,
            groups.len(),
            r.n_used
        );
        return Ok(((r, false), miss));
    }
    // Categorical × categorical.
    let ocats = lt.categorical(oi)?;
    let mut rl: Vec<String> = gcats.iter().flatten().cloned().collect();
    rl.sort();
    rl.dedup();
    let mut cl: Vec<String> = ocats.iter().flatten().cloned().collect();
    cl.sort();
    cl.dedup();
    let mut counts = vec![vec![0.0; cl.len()]; rl.len()];
    let mut miss = 0;
    for (g, o) in gcats.iter().zip(&ocats) {
        match (g, o) {
            (Some(g), Some(o)) => {
                let i = rl.iter().position(|x| x == g).unwrap_or(0);
                let j = cl.iter().position(|x| x == o).unwrap_or(0);
                counts[i][j] += 1.0;
            }
            _ => miss += 1,
        }
    }
    let cells: Vec<Vec<Option<u64>>> = counts
        .iter()
        .map(|r| r.iter().map(|v| Some(*v as u64)).collect())
        .collect();
    let sup = phn_epi::suppression::suppress(
        &cells,
        &phn_epi::suppression::SuppressionOptions {
            threshold: c.threshold,
            ..Default::default()
        },
    );
    let hidden = |i: usize, j: usize| sup.is_hidden(i, j);
    let any_hidden = (0..rl.len()).any(|i| (0..cl.len()).any(|j| sup.is_hidden(i, j)));
    let mut r = contingency(&counts, &rl, &cl, choice, c, Some(&hidden))?;
    r.data_description = format!(
        "'{gname}' (rows) by '{outcome}' (columns) in {}: {}",
        lt.label, r.data_description
    );
    Ok(((r, any_hidden), miss))
}
