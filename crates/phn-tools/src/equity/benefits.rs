//! EQ-06 `estimate_benefit_eligibility`.
//!
//! An income screen for WIC, SNAP, MassHealth, the Health Safety Net, and home energy
//! assistance (HEAP/LIHEAP) from household size and income, using the current federal poverty
//! guidelines and each program's published limits (`content/equity/benefit_eligibility.yaml`).
//! It is a screen, not an eligibility determination.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_content::reportable::Confidence;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::mch::content::{self as mc, CitedText, NoteOut, SourceRef};
use crate::md;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Fpl {
    year: u32,
    area: String,
    base: Vec<f64>,
    each_additional: f64,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Tier {
    percent_fpl: f64,
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Program {
    id: String,
    name: String,
    who: String,
    income_basis: String,
    limit_type: String,
    #[serde(default)]
    annual: Vec<f64>,
    #[serde(default)]
    monthly: Vec<f64>,
    #[serde(default)]
    each_additional: Option<f64>,
    percent_fpl: Option<f64>,
    #[serde(default)]
    max_benefit_monthly: Vec<f64>,
    #[serde(default)]
    max_benefit_each_additional: Option<f64>,
    #[serde(default)]
    tiers: Vec<Tier>,
    effective: String,
    automatic: String,
    apply: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct BenefitContent {
    meta: ContentMeta,
    refresh_schedule: String,
    sources: BTreeMap<String, SourceRef>,
    fpl: Fpl,
    programs: Vec<Program>,
    general_caveats: Vec<CitedText>,
}

static CONTENT: LazyLock<BenefitContent> =
    LazyLock::new(|| mc::load::<BenefitContent>("equity-benefit-eligibility"));

fn content() -> &'static BenefitContent {
    &CONTENT
}

/// The federal poverty guideline (annual) for a household size.
pub fn fpl_annual(size: u32) -> f64 {
    let f = &content().fpl;
    let n = size.max(1) as usize;
    if n <= f.base.len() {
        f.base[n - 1]
    } else {
        f.base[f.base.len() - 1] + f.each_additional * (n - f.base.len()) as f64
    }
}

fn table_value(v: &[f64], add: Option<f64>, size: u32) -> Option<f64> {
    let n = size.max(1) as usize;
    if n <= v.len() {
        Some(v[n - 1])
    } else {
        add.map(|a| v[v.len() - 1] + a * (n - v.len()) as f64)
    }
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `estimate_benefit_eligibility`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct BenefitArgs {
    /// Number of people in the household (for MassHealth, count an unborn child with
    /// `pregnant`; see the result's notes).
    pub household_size: u32,
    /// Household gross income per year (before taxes). Give this or monthly_income.
    #[serde(default)]
    pub annual_income: Option<f64>,
    /// Household gross income per month (before taxes).
    #[serde(default)]
    pub monthly_income: Option<f64>,
    /// Someone in the household is pregnant (MassHealth counts the unborn child or children).
    #[serde(default)]
    pub pregnant: bool,
    /// Number of unborn children when pregnant (default 1).
    #[serde(default)]
    pub unborn_children: Option<u32>,
    /// A child under 5 lives in the household (WIC).
    #[serde(default)]
    pub child_under_5: bool,
    /// A baby under 1 lives in the household (MassHealth infants).
    #[serde(default)]
    pub infant_under_1: bool,
    /// Children under 19 live in the household (MassHealth children; parents' coverage).
    #[serde(default)]
    pub children_under_19: bool,
    /// Someone is breastfeeding or recently gave birth (WIC).
    #[serde(default)]
    pub postpartum_or_breastfeeding: bool,
    /// The household already receives SNAP, MassHealth, or TAFDC (WIC automatic income
    /// eligibility).
    #[serde(default)]
    pub receives_snap_masshealth_or_tafdc: bool,
    /// A young adult 19 or 20 years old is in the household (MassHealth young adults).
    #[serde(default)]
    pub young_adult_19_20: bool,
    /// The household pays for heat, including heat included in rent (HEAP).
    #[serde(default)]
    pub pays_for_heat: Option<bool>,
}

/// One program's screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramScreen {
    pub program: String,
    /// "likely income-eligible", "over the income limit", "automatically income-eligible",
    /// or "does not apply to this household".
    pub result: String,
    /// Who the program is for.
    pub who: String,
    /// Annual income limit for this household size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annual_limit: Option<f64>,
    /// Monthly income limit for this household size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_limit: Option<f64>,
    /// Limit as a percent of the federal poverty guideline, when defined that way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_fpl: Option<f64>,
    /// The tier that applies (e.g. Health Safety Net with or without a deductible).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Household size used for this program.
    pub household_size_used: u32,
    /// "gross" or "magi" (Modified Adjusted Gross Income).
    pub income_basis: String,
    /// SNAP maximum monthly benefit for the household size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_monthly_benefit: Option<f64>,
    pub effective: String,
    pub notes: Vec<String>,
    pub how_to_apply: String,
    pub citation: String,
    /// Developer confidence for draft content.
    pub confidence: Confidence,
}

/// Result of `estimate_benefit_eligibility`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BenefitResult {
    pub household_size: u32,
    pub annual_income: f64,
    pub monthly_income: f64,
    pub fpl_year: u32,
    /// Federal poverty guideline for the household size (48 states and DC).
    pub fpl_annual: f64,
    /// Household income as a percent of the poverty guideline.
    pub percent_of_fpl: f64,
    pub programs: Vec<ProgramScreen>,
    pub general_notes: Vec<NoteOut>,
    /// When each program's limits change.
    pub refresh_schedule: String,
}

// ---------------------------------------------------------------- logic

fn money(x: f64) -> String {
    format!("${}", md::num(x.round(), 0))
}

/// Screen a household's income against WIC, SNAP, MassHealth, HSN, and HEAP limits.
pub fn estimate_benefit_eligibility(
    _ctx: &Context,
    args: BenefitArgs,
) -> Result<Envelope<BenefitResult>> {
    let c = content();
    if !(1..=20).contains(&args.household_size) {
        return Err(PhnError::InvalidInput(
            "household_size should be between 1 and 20".into(),
        ));
    }
    let annual = match (args.annual_income, args.monthly_income) {
        (Some(_), Some(_)) => {
            return Err(PhnError::InvalidInput(
                "give annual_income or monthly_income, not both".into(),
            ));
        }
        (Some(a), None) => a,
        (None, Some(m)) => m * 12.0,
        (None, None) => {
            return Err(PhnError::InvalidInput(
                "give the household's gross income: annual_income or monthly_income".into(),
            ));
        }
    };
    if !annual.is_finite() || annual < 0.0 {
        return Err(PhnError::InvalidInput("income must be zero or more".into()));
    }
    let size = args.household_size;
    let unborn = if args.pregnant {
        args.unborn_children.unwrap_or(1).max(1)
    } else {
        0
    };
    let fpl = fpl_annual(size);
    let pct = annual / fpl * 100.0;
    let mut programs = Vec::new();
    for p in &c.programs {
        // Who the program applies to, from the household facts given.
        let applies = match p.id.as_str() {
            "wic" => {
                args.pregnant
                    || args.child_under_5
                    || args.infant_under_1
                    || args.postpartum_or_breastfeeding
            }
            "masshealth_pregnant_infant" => args.pregnant || args.infant_under_1,
            "masshealth_child" => {
                args.children_under_19 || args.infant_under_1 || args.child_under_5
            }
            "masshealth_parent" => {
                args.children_under_19 || args.infant_under_1 || args.child_under_5
            }
            "masshealth_young_adult" => args.young_adult_19_20,
            "heap" => args.pays_for_heat != Some(false),
            _ => true,
        };
        // MassHealth counts the unborn child in the MAGI household.
        let size_used = if p.id == "masshealth_pregnant_infant" {
            size + unborn
        } else {
            size
        };
        let fpl_used = fpl_annual(size_used);
        let (annual_limit, monthly_limit) = match p.limit_type.as_str() {
            "table_annual" => {
                let a = table_value(&p.annual, p.each_additional, size_used);
                (a, a.map(|x| x / 12.0))
            }
            "table_monthly" => {
                let m = table_value(&p.monthly, p.each_additional, size_used);
                (m.map(|x| x * 12.0), m)
            }
            _ => {
                let a = p.percent_fpl.map(|q| fpl_used * q / 100.0);
                (a, a.map(|x| x / 12.0))
            }
        };
        let mut notes = Vec::new();
        let income_ok = match (p.limit_type.as_str(), annual_limit, monthly_limit) {
            ("table_monthly", _, Some(m)) => Some(annual / 12.0 <= m + 1e-9),
            (_, Some(a), _) => Some(annual <= a + 1e-9),
            _ => None,
        };
        if annual_limit.is_none() {
            notes.push(format!(
                "No published limit for a household of {size_used}; ask the program."
            ));
        }
        let tier = if p.tiers.is_empty() || income_ok != Some(true) {
            None
        } else {
            let pct_used = annual / fpl_used * 100.0;
            p.tiers
                .iter()
                .find(|t| pct_used <= t.percent_fpl + 1e-9)
                .map(|t| t.label.clone())
        };
        let automatic = p.id == "wic" && args.receives_snap_masshealth_or_tafdc;
        let result = if !applies {
            "does not apply to this household (from the facts given)".to_string()
        } else if automatic {
            "automatically income-eligible".to_string()
        } else {
            match income_ok {
                Some(true) => "likely income-eligible".into(),
                Some(false) => "over the income limit".into(),
                None => "check with the program".into(),
            }
        };
        if p.id == "masshealth_pregnant_infant" && args.pregnant {
            notes.push(format!(
                "Household size {size_used} includes {unborn} unborn child(ren), as MassHealth \
                 counts them."
            ));
        }
        if p.id == "wic" && args.pregnant {
            notes.push(
                "Ask the local WIC program how the pregnancy counts in household size.".into(),
            );
        }
        if p.income_basis == "magi" {
            notes.push(
                "MassHealth uses MAGI for the MAGI household; deductions and who is counted can \
                 change the result."
                    .into(),
            );
        }
        if !p.automatic.is_empty() {
            notes.push(p.automatic.clone());
        }
        let max_benefit = if p.max_benefit_monthly.is_empty() {
            None
        } else {
            table_value(
                &p.max_benefit_monthly,
                p.max_benefit_each_additional,
                size_used,
            )
        };
        programs.push(ProgramScreen {
            program: p.name.clone(),
            result,
            who: p.who.clone(),
            annual_limit,
            monthly_limit,
            percent_fpl: p.percent_fpl,
            tier,
            household_size_used: size_used,
            income_basis: p.income_basis.clone(),
            max_monthly_benefit: max_benefit,
            effective: p.effective.clone(),
            notes,
            how_to_apply: p.apply.clone(),
            citation: mc::cite(&c.sources, &p.source, &p.section),
            confidence: p.confidence,
        });
    }

    // ---- Markdown
    let mut s = format!(
        "**Benefit eligibility screen (income only)** — household of {size}{}, income {} a year \
         ({} a month) = **{}% of the {} federal poverty guideline** ({} for {size}).\n\n",
        if unborn > 0 {
            format!(" plus {unborn} unborn")
        } else {
            String::new()
        },
        money(annual),
        money(annual / 12.0),
        md::num(pct, 0),
        c.fpl.year,
        money(fpl),
    );
    let rows: Vec<Vec<String>> = programs
        .iter()
        .map(|p| {
            vec![
                p.program.clone(),
                match (&p.tier, p.result.as_str()) {
                    (Some(t), "likely income-eligible") => format!("likely income-eligible: {t}"),
                    _ => p.result.clone(),
                },
                match (p.monthly_limit, p.annual_limit) {
                    (Some(m), Some(a)) => format!("{} / month ({} / year)", money(m), money(a)),
                    _ => "—".into(),
                },
                p.percent_fpl
                    .map(|q| format!("{q}% FPL"))
                    .unwrap_or_else(|| "60% of state median income".into()),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &["Program", "Screen", "Income limit (household)", "Basis"],
        &rows,
    ));
    if let Some(snap) = programs.iter().find(|p| p.program.starts_with("SNAP"))
        && let Some(b) = snap.max_monthly_benefit
    {
        s.push_str(&format!(
            "\nSNAP maximum monthly benefit for {size}: {} (actual benefits depend on income and \
             expenses).\n",
            money(b)
        ));
    }
    s.push_str(
        "\nThis is a screen, not a determination: encourage the family to apply. Each program's \
         'how to apply' is in `programs`.\n",
    );

    let mut caveats = vec![mc::review_caveat(&c.meta, "benefit eligibility")];
    caveats.extend(c.general_caveats.iter().map(|n| n.text.clone()));
    caveats.push(format!("Limits change every year: {}", c.refresh_schedule));
    if programs.iter().any(|p| {
        p.confidence != Confidence::High
            && p.result != "does not apply to this household (from the facts given)"
    }) {
        caveats.push(
            "SNAP's effective date is assumed (DTA's chart is undated): confirm with DTA.".into(),
        );
    }
    let mut keys: Vec<&str> = c.programs.iter().map(|p| p.source.as_str()).collect();
    keys.push(&c.fpl.source);
    let mut general_notes = mc::notes(&c.sources, &c.general_caveats);
    general_notes.push(NoteOut {
        text: format!(
            "Poverty guidelines used: {} ({}); Massachusetts programs use these, not the Alaska \
             or Hawaii guidelines.",
            c.fpl.year, c.fpl.area
        ),
        citation: mc::cite(&c.sources, &c.fpl.source, &c.fpl.section),
        confidence: None,
    });
    let data = BenefitResult {
        household_size: size,
        annual_income: annual,
        monthly_income: annual / 12.0,
        fpl_year: c.fpl.year,
        fpl_annual: fpl,
        percent_of_fpl: pct,
        programs,
        general_notes,
        refresh_schedule: c.refresh_schedule.clone(),
    };
    Ok(Envelope::new(s, data)
        .sources(mc::sources_for(&c.sources, keys))
        .source(Source::computed(
            "Percent of federal poverty guideline and program limits by household size",
            "PHN MCP server",
        ))
        .caveats(caveats)
        .privacy(Privacy {
            notes: vec!["Household size and income are used only for this calculation.".into()],
            ..Privacy::none()
        })
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn published_tables_match_their_fpl_multiples() {
        // WIC is 185% and SNAP 200% of the 2026 poverty guidelines (rounded up).
        let c = content();
        // The published rows (1-8 people); larger households add the published increment.
        for size in 1..=8u32 {
            let f = fpl_annual(size);
            let wic = c.programs.iter().find(|p| p.id == "wic").unwrap();
            let w = table_value(&wic.annual, wic.each_additional, size).unwrap();
            assert!(
                (w - (f * 1.85).ceil()).abs() <= 1.0,
                "WIC {size}: {w} vs {}",
                f * 1.85
            );
            let snap = c.programs.iter().find(|p| p.id == "snap").unwrap();
            let m = table_value(&snap.monthly, snap.each_additional, size).unwrap();
            assert!(
                (m - (f * 2.0 / 12.0)).abs() <= 1.0,
                "SNAP {size}: {m} vs {}",
                f / 6.0
            );
        }
        let snap = c.programs.iter().find(|p| p.id == "snap").unwrap();
        assert!((snap.each_additional.unwrap() - 5_680.0 * 2.0 / 12.0).abs() <= 1.0);
        let wic = c.programs.iter().find(|p| p.id == "wic").unwrap();
        assert!((wic.each_additional.unwrap() - 5_680.0 * 1.85).abs() <= 1.0);
        assert_eq!(fpl_annual(4), 33_000.0);
        assert_eq!(fpl_annual(10), 55_720.0 + 2.0 * 5_680.0);
        for p in &c.programs {
            assert!(c.sources.contains_key(&p.source), "{}", p.source);
        }
    }

    #[test]
    fn family_of_three_with_a_baby() {
        let (_d, c) = ctx();
        let env = estimate_benefit_eligibility(
            &c,
            BenefitArgs {
                household_size: 3,
                monthly_income: Some(3_500.0),
                infant_under_1: true,
                child_under_5: true,
                children_under_19: true,
                pays_for_heat: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert!((d.percent_of_fpl - 42_000.0 / 27_320.0 * 100.0).abs() < 1e-9);
        let get = |name: &str| {
            d.programs
                .iter()
                .find(|p| p.program.starts_with(name))
                .unwrap()
        };
        assert_eq!(get("WIC").result, "likely income-eligible");
        assert_eq!(get("SNAP").result, "likely income-eligible");
        assert_eq!(
            get("MassHealth Standard (pregnant").result,
            "likely income-eligible"
        );
        assert_eq!(
            get("MassHealth Standard (parents").result,
            "over the income limit"
        );
        let hsn = get("Health Safety Net");
        assert!(hsn.tier.as_ref().unwrap().contains("deductible"));
        assert_eq!(get("Home Energy").result, "likely income-eligible");
        assert!(env.notice.is_some());
        assert!(env.summary.contains("154% of the 2026"));
    }

    #[test]
    fn pregnancy_counts_unborn_for_masshealth_and_wic_is_automatic() {
        let (_d, c) = ctx();
        let env = estimate_benefit_eligibility(
            &c,
            BenefitArgs {
                household_size: 1,
                annual_income: Some(40_000.0),
                pregnant: true,
                receives_snap_masshealth_or_tafdc: true,
                ..Default::default()
            },
        )
        .unwrap();
        let mh = env
            .data
            .programs
            .iter()
            .find(|p| p.program.contains("pregnant"))
            .unwrap();
        assert_eq!(mh.household_size_used, 2);
        assert_eq!(mh.result, "likely income-eligible"); // 200% of 21,640 = 43,280
        let wic = env
            .data
            .programs
            .iter()
            .find(|p| p.program.starts_with("WIC"))
            .unwrap();
        assert_eq!(wic.result, "automatically income-eligible");
    }

    #[test]
    fn bad_inputs() {
        let (_d, c) = ctx();
        assert!(
            estimate_benefit_eligibility(
                &c,
                BenefitArgs {
                    household_size: 2,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("income")
        );
        assert!(
            estimate_benefit_eligibility(
                &c,
                BenefitArgs {
                    household_size: 0,
                    annual_income: Some(1.0),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
