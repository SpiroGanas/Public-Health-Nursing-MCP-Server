//! AGE-04 `explain_older_adult_coverage_options`: a plain-language comparison
//! of Medicare (with the current year's premiums and deductibles), Medicare
//! Savings Programs, MassHealth for people 65+, the Frail Elder Waiver, PACE,
//! Senior Care Options, and One Care, with an optional eligibility screen from
//! the facts given. Content: `content/aging/coverage_options.yaml` (pending
//! review; figures carry their effective dates).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{self as c10, COVERAGE, CitedOut, CitedText, SourceRef, cite_all};
use crate::context::Context;
use crate::md;

#[derive(Debug, Clone, Deserialize)]
struct PartDef {
    part: String,
    covers: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CostDef {
    item: String,
    amount: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct IrmaaRow {
    pub individual: String,
    pub joint: String,
    pub total_premium: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Irmaa {
    note: String,
    rows: Vec<IrmaaRow>,
    source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Medicare {
    year: u32,
    effective_date: String,
    next_update: String,
    parts: Vec<PartDef>,
    costs: Vec<CostDef>,
    irmaa: Irmaa,
}

#[derive(Debug, Clone, Deserialize)]
struct FplRow {
    household: u32,
    pct100: f64,
    pct190: f64,
    pct225: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct Fpl {
    effective_date: String,
    note: String,
    rows: Vec<FplRow>,
    source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProgramDef {
    id: String,
    name: String,
    who: String,
    benefits: Vec<String>,
    assets: String,
    apply: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Partial {
    county: String,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PaceCounties {
    available: Vec<String>,
    partial: Vec<Partial>,
    not_available: Vec<String>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CoverageContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    medicare: Medicare,
    fpl_monthly: Fpl,
    programs: Vec<ProgramDef>,
    pace_counties: PaceCounties,
    help: Vec<CitedText>,
}

static CONTENT: LazyLock<CoverageContent> =
    LazyLock::new(|| c10::load(COVERAGE).expect("coverage content parses"));

/// FEW asset/income references used in the screen.
const PACE_FBR_300: f64 = 2982.0;
const MASSHEALTH_ASSETS_SINGLE: f64 = 2000.0;
const MASSHEALTH_ASSETS_COUPLE: f64 = 3000.0;

/// Where the person lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Living {
    /// Own home or a family member's home.
    Community,
    AssistedLiving,
    NursingHome,
}

/// Arguments for `explain_older_adult_coverage_options`. All facts are
/// optional; give what is known for a screen. No names or identifiers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CoverageArgs {
    pub age_years: Option<u32>,
    /// People in the household for the income test (1 or 2 are bundled).
    pub household_size: Option<u32>,
    /// Gross monthly household income in dollars (a rough screen; MassHealth
    /// counts income by its own rules).
    pub monthly_income: Option<f64>,
    /// Countable assets in dollars.
    pub countable_assets: Option<f64>,
    pub has_medicare_a_and_b: Option<bool>,
    pub has_masshealth_standard: Option<bool>,
    pub has_disability: Option<bool>,
    /// Needs a nursing home level of care (FEW and PACE require it).
    pub nursing_home_level_of_care: Option<bool>,
    pub living: Option<Living>,
    /// Has private health insurance (for example from a job); One Care excludes it.
    pub has_private_insurance: Option<bool>,
    /// Town, to check PACE availability by county (default: configured town).
    pub town: Option<String>,
}

/// A Medicare cost item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CostOut {
    pub item: String,
    pub amount: String,
    pub citation: String,
}

/// Screening status for a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScreenStatus {
    LikelyEligible,
    MayBeEligible,
    ProbablyNotEligible,
    NeedMoreInformation,
}

impl ScreenStatus {
    fn label(self) -> &'static str {
        match self {
            ScreenStatus::LikelyEligible => "likely eligible",
            ScreenStatus::MayBeEligible => "may be eligible",
            ScreenStatus::ProbablyNotEligible => "probably not eligible",
            ScreenStatus::NeedMoreInformation => "need more information",
        }
    }
}

/// A program with its rules and the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramOut {
    pub id: String,
    pub name: String,
    pub who: String,
    pub benefits: Vec<String>,
    pub assets: String,
    pub apply: String,
    pub citation: String,
    pub screen: ScreenStatus,
    pub reasons: Vec<String>,
}

/// Result of `explain_older_adult_coverage_options`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageResult {
    pub medicare_year: u32,
    pub medicare_effective: String,
    pub medicare_parts: Vec<CitedOut>,
    pub medicare_costs: Vec<CostOut>,
    pub part_b_income_adjustments: Vec<IrmaaRow>,
    /// QMB, SLMB/QI, or over the MSP limit (when income was given).
    pub msp_level: Option<String>,
    /// The FPL figures used (effective date and monthly amounts).
    pub fpl_used: Option<String>,
    pub programs: Vec<ProgramOut>,
    /// PACE availability in the town's county.
    pub pace_in_county: Option<String>,
    pub help: Vec<CitedOut>,
}

fn fpl_row(household: u32) -> Option<&'static FplRow> {
    CONTENT
        .fpl_monthly
        .rows
        .iter()
        .find(|r| r.household == household)
}

/// MSP level for a monthly income and household size (None when no row).
pub fn msp_level(monthly_income: f64, household: u32) -> Option<&'static str> {
    let r = fpl_row(household)?;
    Some(if monthly_income <= r.pct190 {
        "QMB"
    } else if monthly_income <= r.pct225 {
        "SLMB/QI"
    } else {
        "over the MSP income limit"
    })
}

fn money(v: f64) -> String {
    format!("${}", md::num(v, 0))
}

/// Explain coverage options and screen eligibility from the facts given.
pub fn explain_older_adult_coverage_options(
    ctx: &Context,
    args: CoverageArgs,
) -> Result<Envelope<CoverageResult>> {
    let c = &*CONTENT;
    if let Some(v) = args.monthly_income.filter(|v| *v < 0.0 || !v.is_finite()) {
        return Err(PhnError::InvalidInput(format!(
            "monthly_income {v} must be zero or more"
        )));
    }
    if args.monthly_income.is_some_and(|v| v > 100_000.0) {
        return Err(PhnError::InvalidInput(
            "monthly_income looks like an annual figure; give monthly income".into(),
        ));
    }
    if args.age_years.is_some_and(|a| a > 120) {
        return Err(PhnError::InvalidInput(
            "age_years must be 120 or less".into(),
        ));
    }
    let household = args.household_size.unwrap_or(1).max(1);
    let couple = household >= 2;
    let county = match c10::town_or_configured(ctx, args.town.as_deref()) {
        Some(t) => Some(c10::resolve_town(ctx, &t)?.county.clone()),
        None => None,
    };
    let age = args.age_years;
    let income = args.monthly_income;
    let assets = args.countable_assets;
    let level = income.and_then(|i| msp_level(i, household));
    let fpl_used = fpl_row(household).map(|r| {
        format!(
            "Household of {household}: 100% FPL {}/month, 190% {}/month, 225% {}/month (effective {}).",
            money(r.pct100),
            money(r.pct190),
            money(r.pct225),
            c.fpl_monthly.effective_date
        )
    });

    let mut programs = Vec::new();
    for p in &c.programs {
        let mut reasons = Vec::new();
        let mut status = ScreenStatus::MayBeEligible;
        let not = |r: String, reasons: &mut Vec<String>| {
            reasons.push(r);
            ScreenStatus::ProbablyNotEligible
        };
        match p.id.as_str() {
            "msp" => {
                if args.has_medicare_a_and_b == Some(false) {
                    status = not("MSPs are for people with Medicare.".into(), &mut reasons);
                } else if let Some(l) = level {
                    if l == "over the MSP income limit" {
                        status = not(
                            format!(
                                "Income {} is above 225% FPL for a household of {household}.",
                                money(income.unwrap_or(0.0))
                            ),
                            &mut reasons,
                        );
                    } else {
                        status = ScreenStatus::LikelyEligible;
                        reasons.push(format!("Income {} is within the {l} limit for a household of {household}; there is no asset limit.", money(income.unwrap_or(0.0))));
                    }
                } else if income.is_some() {
                    status = ScreenStatus::NeedMoreInformation;
                    reasons.push(format!("Only households of 1 or 2 are bundled; check MassHealth's FPL chart for a household of {household}."));
                } else {
                    status = ScreenStatus::NeedMoreInformation;
                    reasons.push("Give monthly household income to screen (QMB at or below 190% FPL, SLMB/QI at or below 225%).".into());
                }
            }
            "masshealth_65" => {
                let limit = if couple {
                    MASSHEALTH_ASSETS_COUPLE
                } else {
                    MASSHEALTH_ASSETS_SINGLE
                };
                if age.is_some_and(|a| a < 65) {
                    status = not("This summary covers people 65 and older; younger adults have other MassHealth coverage types.".into(), &mut reasons);
                } else if income.is_some_and(|i| fpl_row(household).is_some_and(|r| i > r.pct225)) {
                    status = not("Income is above 225% FPL.".into(), &mut reasons);
                } else if assets.is_some_and(|a| a > limit) {
                    status = not(
                        format!(
                            "Countable assets {} are above the {} limit for certain MassHealth benefits (the MSP has no asset limit).",
                            money(assets.unwrap_or(0.0)),
                            money(limit)
                        ),
                        &mut reasons,
                    );
                } else if income.is_some() && assets.is_some() {
                    reasons.push(format!("Income at or below 225% FPL and assets at or below {}; MassHealth makes the determination.", money(limit)));
                } else {
                    status = ScreenStatus::NeedMoreInformation;
                    reasons.push("Give monthly income and countable assets to screen.".into());
                }
            }
            "few" => {
                let age_ok = match (age, args.has_disability) {
                    (Some(a), _) if a >= 65 => Some(true),
                    (Some(a), Some(true)) if (60..65).contains(&a) => Some(true),
                    (Some(a), None) if (60..65).contains(&a) => None,
                    (Some(_), _) => Some(false),
                    (None, _) => None,
                };
                if age_ok == Some(false) {
                    status = not(
                        "FEW is for people 65+, or 60-64 with a disability.".into(),
                        &mut reasons,
                    );
                } else if args.nursing_home_level_of_care == Some(false) {
                    status = not(
                        "FEW requires a nursing facility level of care.".into(),
                        &mut reasons,
                    );
                } else if matches!(
                    args.living,
                    Some(Living::AssistedLiving | Living::NursingHome)
                ) {
                    status = not("FEW participants live in the community (own or family home, or congregate housing), not assisted living, rest homes, or nursing facilities.".into(), &mut reasons);
                } else if age_ok == Some(true) && args.nursing_home_level_of_care == Some(true) {
                    reasons.push("Age and level of care fit; MassHealth Standard community financial rules (with special waiver rules) still apply.".into());
                } else {
                    status = ScreenStatus::NeedMoreInformation;
                    reasons.push("Needs age (65+, or 60-64 with a disability), nursing facility level of care, and community living.".into());
                }
            }
            "pace" => {
                let county_ok = county.as_deref().map(|cn| {
                    if c.pace_counties.available.iter().any(|x| x == cn) {
                        Some(true)
                    } else if c.pace_counties.partial.iter().any(|x| x.county == cn) {
                        None
                    } else {
                        Some(false)
                    }
                });
                if age.is_some_and(|a| a < 55) {
                    status = not("PACE is for people 55 and older.".into(), &mut reasons);
                } else if age.is_some_and(|a| a < 65) && args.has_disability == Some(false) {
                    status = not(
                        "At 55-64 PACE needs a disability determination.".into(),
                        &mut reasons,
                    );
                } else if args.nursing_home_level_of_care == Some(false) {
                    status = not(
                        "PACE requires state certification of eligibility for nursing home care."
                            .into(),
                        &mut reasons,
                    );
                } else if county_ok == Some(Some(false)) {
                    status = not(
                        format!(
                            "PACE is not currently available in {} County.",
                            county.clone().unwrap_or_default()
                        ),
                        &mut reasons,
                    );
                } else {
                    if county_ok == Some(None) {
                        reasons.push(format!(
                            "PACE is available in part of {} County; check the service area.",
                            county.clone().unwrap_or_default()
                        ));
                    }
                    match (income, assets) {
                        (Some(i), Some(a)) if !couple && i <= PACE_FBR_300 && a <= MASSHEALTH_ASSETS_SINGLE => {
                            reasons.push(format!("Income at or below 300% of the federal benefit rate ({}/month) and assets at or below $2,000: MassHealth may pay the PACE premium.", money(PACE_FBR_300)));
                        }
                        (Some(_), Some(_)) => reasons.push("MassHealth may not pay the premium at this income or asset level; private pay and Medicare-only options exist.".into()),
                        _ => {}
                    }
                    if args.nursing_home_level_of_care != Some(true) {
                        status = ScreenStatus::NeedMoreInformation;
                        reasons.push(
                            "PACE requires certification of nursing home level of care.".into(),
                        );
                    }
                }
            }
            "sco" => {
                if age.is_some_and(|a| a < 65) {
                    status = not("SCO is for people 65 and older.".into(), &mut reasons);
                } else if args.has_medicare_a_and_b == Some(false) {
                    status = not(
                        "Since January 1, 2026, SCO members must have Medicare Parts A and B."
                            .into(),
                        &mut reasons,
                    );
                } else if args.has_masshealth_standard == Some(false) {
                    status = not("SCO requires MassHealth Standard.".into(), &mut reasons);
                } else if age.is_some()
                    && args.has_medicare_a_and_b == Some(true)
                    && args.has_masshealth_standard == Some(true)
                {
                    status = ScreenStatus::LikelyEligible;
                    reasons.push("65+, Medicare A and B, and MassHealth Standard: likely eligible if a SCO plan serves the area.".into());
                } else {
                    status = ScreenStatus::NeedMoreInformation;
                    reasons.push(
                        "Needs age 65+, Medicare Parts A and B, and MassHealth Standard.".into(),
                    );
                }
            }
            "one_care" => {
                if age.is_some_and(|a| !(21..=64).contains(&a)) {
                    status = not("One Care is for people 21 to 64.".into(), &mut reasons);
                } else if args.has_disability == Some(false) {
                    status = not(
                        "One Care is for people with a disability.".into(),
                        &mut reasons,
                    );
                } else if args.has_private_insurance == Some(true) {
                    status = not(
                        "People with private health insurance cannot join One Care.".into(),
                        &mut reasons,
                    );
                } else if args.has_medicare_a_and_b == Some(false) {
                    status = not(
                        "One Care requires Medicare Parts A and B.".into(),
                        &mut reasons,
                    );
                } else {
                    status = ScreenStatus::NeedMoreInformation;
                    reasons.push("Needs age 21-64, a disability, Medicare A and B, MassHealth Standard or CommonHealth, and no HCBS waiver.".into());
                }
            }
            _ => {}
        }
        programs.push(ProgramOut {
            id: p.id.clone(),
            name: p.name.clone(),
            who: p.who.clone(),
            benefits: p.benefits.clone(),
            assets: p.assets.clone(),
            apply: p.apply.clone(),
            citation: c10::citation(&c.sources, &p.source, &p.section),
            screen: status,
            reasons,
        });
    }
    let pace_in_county = county.as_deref().map(|cn| {
        if c.pace_counties.available.iter().any(|x| x == cn) {
            format!(
                "PACE is available in {cn} County ({}).",
                c10::citation(
                    &c.sources,
                    &c.pace_counties.source,
                    &c.pace_counties.section
                )
            )
        } else if let Some(p) = c.pace_counties.partial.iter().find(|x| x.county == cn) {
            format!("{cn} County: {}", p.note)
        } else if c.pace_counties.not_available.iter().any(|x| x == cn) {
            format!("PACE is not currently available in {cn} County.")
        } else {
            format!("{cn} County is not in the PACE service-area list; check with MassHealth.")
        }
    });

    let parts: Vec<CitedOut> = c
        .medicare
        .parts
        .iter()
        .map(|p| CitedOut {
            text: format!("{}: {}", p.part, p.covers),
            citation: c10::citation(&c.sources, &p.source, ""),
            confidence: None,
        })
        .collect();
    let costs: Vec<CostOut> = c
        .medicare
        .costs
        .iter()
        .map(|x| CostOut {
            item: x.item.clone(),
            amount: x.amount.clone(),
            citation: c10::citation(&c.sources, &x.source, ""),
        })
        .collect();

    // ---- markdown
    let mut s = format!(
        "**Coverage options for older adults in Massachusetts**\n\n**Medicare {} costs** (effective {}):\n",
        c.medicare.year, c.medicare.effective_date
    );
    s.push_str(&md::table(
        &["Item", "Amount"],
        &costs
            .iter()
            .map(|x| vec![x.item.clone(), x.amount.clone()])
            .collect::<Vec<_>>(),
    ));
    s.push_str(&format!(
        "\n{} The highest total Part B premium is {}.\n",
        c.medicare.irmaa.note,
        c.medicare
            .irmaa
            .rows
            .last()
            .map(|r| r.total_premium.clone())
            .unwrap_or_default()
    ));
    if let (Some(l), Some(i)) = (level, income) {
        s.push_str(&format!("\n**Medicare Savings Program screen:** income {} for a household of {household} → **{l}**. {}\n", money(i), fpl_used.clone().unwrap_or_default()));
    }
    s.push_str("\n**Programs**\n");
    let rows: Vec<Vec<String>> = programs
        .iter()
        .map(|p| {
            vec![
                p.name.clone(),
                p.screen.label().to_string(),
                p.reasons.join(" "),
            ]
        })
        .collect();
    s.push_str(&md::table(&["Program", "Screen", "Why"], &rows));
    if let Some(p) = &pace_in_county {
        s.push_str(&format!("\n{p}\n"));
    }
    s.push_str("\n**Help:** ");
    s.push_str(
        &c.help
            .iter()
            .map(|h| h.text.clone())
            .collect::<Vec<_>>()
            .join(" "),
    );
    s.push('\n');

    let mut sources = vec![c10::content_source(COVERAGE)?];
    let mut keys: Vec<&str> = vec![
        "cms-2026-ab",
        "cms-2026-partd",
        "ma-what-is-medicare",
        c.fpl_monthly.source.as_str(),
        c.medicare.irmaa.source.as_str(),
    ];
    keys.extend(c.programs.iter().map(|p| p.source.as_str()));
    keys.push(c.pace_counties.source.as_str());
    keys.push("ma-sco-2026");
    sources.extend(c10::sources_for(&c.sources, keys));
    if income.is_some() || assets.is_some() {
        sources.push(Source::computed(
            "Eligibility screen from the facts given",
            "PHN MCP server",
        ));
    }
    let mut caveats = vec![
        "A screen, not an eligibility decision: MassHealth and Medicare decide, using countable income and assets under their own rules. SHINE counselors can help (free).".to_string(),
        format!("Medicare amounts are for {}; {}", c.medicare.year, c.medicare.next_update),
        format!("MassHealth FPL figures: {} {}", c.fpl_monthly.effective_date, c.fpl_monthly.note),
    ];
    if household > 2 && income.is_some() {
        caveats.push("Only households of 1 and 2 are bundled for the MSP income test.".into());
    }
    caveats.extend(c10::review_caveats(&[COVERAGE]));
    Ok(Envelope::new(
        s,
        CoverageResult {
            medicare_year: c.medicare.year,
            medicare_effective: c.medicare.effective_date.clone(),
            medicare_parts: parts,
            medicare_costs: costs,
            part_b_income_adjustments: c.medicare.irmaa.rows.clone(),
            msp_level: level.map(String::from),
            fpl_used,
            programs,
            pace_in_county,
            help: cite_all(&c.sources, &c.help),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        notes: vec!["Only the facts given (age, income, assets, coverage) were used; no names or identifiers.".into()],
        ..Privacy::none()
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn content_sources_are_known() {
        let c = &*CONTENT;
        let mut keys: Vec<&str> = c.programs.iter().map(|p| p.source.as_str()).collect();
        keys.extend(c.medicare.parts.iter().map(|p| p.source.as_str()));
        keys.extend(c.medicare.costs.iter().map(|p| p.source.as_str()));
        keys.extend(c.help.iter().map(|p| p.source.as_str()));
        keys.extend([
            c.fpl_monthly.source.as_str(),
            c.pace_counties.source.as_str(),
            c.medicare.irmaa.source.as_str(),
        ]);
        c10::assert_keys_known(&c.sources, &keys, "coverage");
    }

    #[test]
    fn msp_levels_at_the_boundaries() {
        // 2026 MassHealth figures (effective March 1, 2026): 190% FPL $2,527 and 225% $2,993 for one person.
        assert_eq!(msp_level(2527.0, 1), Some("QMB"));
        assert_eq!(msp_level(2528.0, 1), Some("SLMB/QI"));
        assert_eq!(msp_level(2993.0, 1), Some("SLMB/QI"));
        assert_eq!(msp_level(2994.0, 1), Some("over the MSP income limit"));
        assert_eq!(msp_level(3427.0, 2), Some("QMB"));
        assert_eq!(msp_level(4058.0, 2), Some("SLMB/QI"));
        assert_eq!(msp_level(1000.0, 3), None);
    }

    #[test]
    fn screens_programs_from_facts() {
        let (_d, ctx) = ctx();
        let r = explain_older_adult_coverage_options(
            &ctx,
            CoverageArgs {
                age_years: Some(78),
                household_size: Some(1),
                monthly_income: Some(1800.0),
                countable_assets: Some(1500.0),
                has_medicare_a_and_b: Some(true),
                has_masshealth_standard: Some(true),
                nursing_home_level_of_care: Some(true),
                living: Some(Living::Community),
                town: Some("Springfield".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let st = |id: &str| r.data.programs.iter().find(|p| p.id == id).unwrap().screen;
        assert_eq!(r.data.msp_level.as_deref(), Some("QMB"));
        assert_eq!(st("msp"), ScreenStatus::LikelyEligible);
        assert_eq!(st("sco"), ScreenStatus::LikelyEligible);
        assert_eq!(st("one_care"), ScreenStatus::ProbablyNotEligible);
        assert_eq!(st("few"), ScreenStatus::MayBeEligible);
        assert!(
            r.data
                .pace_in_county
                .as_deref()
                .unwrap()
                .contains("Hampden")
        );
        assert!(
            r.data
                .medicare_costs
                .iter()
                .any(|x| x.amount == "$202.90 per month")
        );
        let r = explain_older_adult_coverage_options(
            &ctx,
            CoverageArgs {
                town: Some("Nantucket".into()),
                age_years: Some(70),
                ..Default::default()
            },
        )
        .unwrap();
        let pace = r.data.programs.iter().find(|p| p.id == "pace").unwrap();
        assert_eq!(pace.screen, ScreenStatus::ProbablyNotEligible);
        assert!(
            explain_older_adult_coverage_options(
                &ctx,
                CoverageArgs {
                    monthly_income: Some(-5.0),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            explain_older_adult_coverage_options(
                &ctx,
                CoverageArgs {
                    monthly_income: Some(240_000.0),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
