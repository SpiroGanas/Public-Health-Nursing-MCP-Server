//! CHR-03 `diabetes_prevention_finder`: scores the CDC Prediabetes Risk Test from the points
//! CDC publishes, checks the CDC eligibility rules for the National Diabetes Prevention Program
//! lifestyle change program, lists the Massachusetts program sites DPH publishes for a town,
//! county, or the state (bundled snapshot), explains the delivery modes and the Medicare
//! Diabetes Prevention Program (from CMS), and writes a plain-language client handout
//! (reading grade checked) to /data/output. Scores from a local file of responses are
//! summarized only as suppressed counts.
//!
//! What is NOT here: CDC's registry of recognized organizations could not be reached, so no
//! site's CDC recognition status or delivery mode is stated, and online or distance-learning
//! providers are not listed (the tool says how to find them on CDC's page). No source was
//! found for MassHealth coverage, so none is stated.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::dpp::{self, DppPrograms, DppSite};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::screen_common::{
    AGE_GROUPS, AGE_MISSING, CountTable, MAX_AGE, age_group, count_table, find_column, tally,
    yes_no_column,
};
use crate::comms::readability::{AnalyzeOptions, analyze, default_target_grade};
use crate::context::Context;
use crate::epr::area::{self, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/chronic/diabetes_prevention.yaml`.
pub const DIABETES_PREVENTION: &str = "chronic-diabetes-prevention";

/// Blank written into the handout where a value is not known.
const BLANK: &str = "____________";

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointDef {
    pub when: String,
    pub points: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskItemDef {
    pub id: String,
    pub label: String,
    pub question: String,
    pub points: Vec<PointDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BmiDef {
    pub formula: String,
    pub example: String,
    pub overweight_from: f64,
    pub asian_overweight_from: f64,
    pub obese_from: f64,
    pub severe_from: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskTestDef {
    pub name: String,
    pub high_risk_at: i64,
    pub maximum: i64,
    pub scoring_note: String,
    pub source: String,
    pub items: Vec<RiskItemDef>,
    pub bmi: BmiDef,
    pub ethnicity_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EligibilityDef {
    pub source: String,
    pub must_meet_all: Vec<String>,
    pub must_meet_one: Vec<String>,
    pub medicare_note: String,
    pub a1c_range: [f64; 2],
    pub fasting_range: [f64; 2],
    pub two_hour_range: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryDef {
    pub source: String,
    pub text: String,
    pub how_to_filter: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramDef {
    pub source: String,
    pub facts: Vec<Cited>,
    pub delivery_modes: DeliveryDef,
    pub registry_gap: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MedicareDef {
    pub source: String,
    pub facts: Vec<Cited>,
    pub finder: String,
    pub eligibility_gap: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassHealthDef {
    pub statement: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoutDef {
    pub id: String,
    pub title: String,
    pub sources: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notices {
    pub decision_support: String,
    pub copyright: String,
}

/// `content/chronic/diabetes_prevention.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DppContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub risk_test: RiskTestDef,
    pub eligibility: EligibilityDef,
    pub program: ProgramDef,
    pub medicare: MedicareDef,
    pub masshealth: MassHealthDef,
    pub handout: HandoutDef,
    pub notices: Notices,
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `diabetes_prevention_finder`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DiabetesPreventionArgs {
    /// Age in years (adults only) for the risk test and eligibility.
    pub age: Option<f64>,
    /// "man" or "woman": how CDC's test scores sex ("male" and "female" are accepted).
    pub sex: Option<String>,
    /// A woman who has had gestational diabetes (adds a point; also an eligibility route).
    /// Default for a woman when not given: no, and the result says so.
    pub gestational_diabetes: Option<bool>,
    /// A mother, father, sister, or brother with diabetes.
    pub family_history: Option<bool>,
    /// Ever diagnosed with high blood pressure.
    pub high_blood_pressure: Option<bool>,
    /// Physically active (CDC's test asks this; "no" adds a point).
    pub physically_active: Option<bool>,
    /// Height in inches (with weight_pounds, to work out BMI).
    pub height_inches: Option<f64>,
    /// Weight in pounds.
    pub weight_pounds: Option<f64>,
    /// BMI, if you already have it (used instead of height and weight).
    pub bmi: Option<f64>,
    /// Asian American: CDC's test uses a lower BMI cut point (23 instead of 25).
    pub asian_american: Option<bool>,
    /// Previously diagnosed with type 1 or type 2 diabetes (an eligibility exclusion).
    pub diabetes_diagnosis: Option<bool>,
    /// Pregnant (an eligibility exclusion).
    pub pregnant: Option<bool>,
    /// A1C (percent) from a blood test in the past year (eligibility route).
    pub a1c_percent: Option<f64>,
    /// Fasting plasma glucose (mg/dL) from the past year.
    pub fasting_glucose_mg_dl: Option<f64>,
    /// 2-hour plasma glucose (mg/dL) after a 75 g glucose load, from the past year.
    pub two_hour_glucose_mg_dl: Option<f64>,
    /// A Massachusetts city or town, a county, or "Massachusetts" for the program sites.
    /// Default: your saved town, or the state when none is saved.
    pub area: Option<String>,
    /// A screening-event file in /data with one row per person (age, sex, family history, high
    /// blood pressure, physically active, height and weight or BMI; optionally gestational
    /// diabetes and Asian American). Only suppressed counts by result and age group are
    /// returned.
    pub responses_path: Option<String>,
    /// Sheet name when responses_path is an XLSX workbook.
    pub sheet: Option<String>,
    /// Write a plain-language client handout (reading grade 8 or lower, checked) to
    /// /data/output, with the person's score filled in when one was worked out (default false).
    pub handout: Option<bool>,
    /// Handout format: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
}

/// One scored risk factor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScoredItem {
    pub factor: String,
    pub answer: String,
    pub points: i64,
}

/// The risk test result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RiskTestResult {
    pub score: i64,
    pub maximum: i64,
    pub high_risk: bool,
    pub items: Vec<ScoredItem>,
    pub bmi: f64,
    pub interpretation: String,
    pub notes: Vec<String>,
}

/// One eligibility requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequirementStatus {
    pub requirement: String,
    /// "met", "not met", or "unknown" (not given).
    pub status: String,
    pub detail: String,
}

/// CDC eligibility for the lifestyle change program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EligibilityResult {
    /// "Meets CDC eligibility", "Does not meet CDC eligibility", or "Cannot tell from what was given".
    pub overall: String,
    pub all_of: Vec<RequirementStatus>,
    pub one_of: Vec<RequirementStatus>,
    pub medicare_note: String,
}

/// A program site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SiteOut {
    pub site_name: String,
    /// The place name as DPH printed it.
    pub listed_place: String,
    pub municipality: Option<String>,
    pub county: Option<String>,
    pub address: String,
    /// Organization phone, only when DPH names no contact person.
    pub organization_phone: Option<String>,
    pub organization_email: Option<String>,
    /// Where to get the contact when DPH names a person (their name is not kept).
    pub contact_note: String,
}

/// Program sites for the area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramsOut {
    pub area: String,
    pub area_level: String,
    pub in_area: Vec<SiteOut>,
    /// Other sites in the same county (when the area is a town).
    pub same_county: Vec<SiteOut>,
    /// Sites outside Massachusetts on DPH's list (statewide requests only).
    pub outside_massachusetts: Vec<SiteOut>,
    pub sites_listed_statewide: usize,
    /// What the list is, its vintage, and what it does not say.
    pub source_note: String,
    pub registry_gap: String,
    pub verify_links: Vec<String>,
}

/// Delivery modes and how to find online options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeliveryOut {
    pub modes: CitedOut,
    pub how_to_find_online_and_distance_options: String,
    pub find_a_program_url: String,
    pub registry_url: String,
}

/// Medicare Diabetes Prevention Program facts (CMS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MedicareOut {
    pub facts: Vec<CitedOut>,
    pub finder: String,
    pub eligibility_gap: String,
}

/// Aggregates from a local responses file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesSummary {
    pub source_file: String,
    pub columns_found: Vec<String>,
    pub tables: Vec<CountTable>,
    pub suppression_threshold: u32,
    pub notes: Vec<String>,
}

/// The handout written to /data/output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HandoutOut {
    pub title: String,
    pub file: String,
    pub reading_grade: f64,
    pub target_grade: f64,
    pub meets_target: bool,
}

/// Result of `diabetes_prevention_finder`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DiabetesPreventionResult {
    pub risk_test: Option<RiskTestResult>,
    pub eligibility: Option<EligibilityResult>,
    pub programs: ProgramsOut,
    pub program_facts: Vec<CitedOut>,
    pub delivery: DeliveryOut,
    pub medicare: MedicareOut,
    pub masshealth: String,
    pub responses: Option<ResponsesSummary>,
    pub handout: Option<HandoutOut>,
    pub decision_support_notice: String,
}

// ---------------------------------------------------------------- scoring

/// Inputs to the risk test, validated.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskInputs {
    pub age: f64,
    pub woman: bool,
    pub gestational_diabetes: bool,
    pub family_history: bool,
    pub high_blood_pressure: bool,
    pub physically_active: bool,
    pub bmi: f64,
    pub asian_american: bool,
}

/// BMI from height (inches) and weight (pounds).
pub fn bmi_from(height_inches: f64, weight_pounds: f64) -> f64 {
    703.0 * weight_pounds / (height_inches * height_inches)
}

/// The CDC points for each factor and the total.
pub fn score(i: &RiskInputs) -> (i64, Vec<ScoredItem>) {
    let age_pts = if i.age < 40.0 {
        0
    } else if i.age < 50.0 {
        1
    } else if i.age < 60.0 {
        2
    } else {
        3
    };
    let sex_pts = i64::from(!i.woman || i.gestational_diabetes);
    let bmi = (i.bmi * 10.0).round() / 10.0;
    let first = if i.asian_american { 23.0 } else { 25.0 };
    let bmi_pts = if bmi >= 40.0 {
        3
    } else if bmi >= 30.0 {
        2
    } else if bmi >= first {
        1
    } else {
        0
    };
    let yn = |b: bool| if b { "yes" } else { "no" }.to_string();
    let items = vec![
        ScoredItem {
            factor: "Age".into(),
            answer: format!("{}", i.age.floor() as i64),
            points: age_pts,
        },
        ScoredItem {
            factor: "Sex".into(),
            answer: if !i.woman {
                "man".into()
            } else if i.gestational_diabetes {
                "woman with gestational diabetes".into()
            } else {
                "woman".into()
            },
            points: sex_pts,
        },
        ScoredItem {
            factor: "Family history of diabetes".into(),
            answer: yn(i.family_history),
            points: i64::from(i.family_history),
        },
        ScoredItem {
            factor: "High blood pressure".into(),
            answer: yn(i.high_blood_pressure),
            points: i64::from(i.high_blood_pressure),
        },
        ScoredItem {
            factor: "Physically active".into(),
            answer: yn(i.physically_active),
            points: i64::from(!i.physically_active),
        },
        ScoredItem {
            factor: "BMI".into(),
            answer: format!(
                "{bmi:.1}{}",
                if i.asian_american {
                    " (Asian American cut points)"
                } else {
                    ""
                }
            ),
            points: bmi_pts,
        },
    ];
    (items.iter().map(|x| x.points).sum(), items)
}

fn parse_sex(s: &str) -> Result<bool> {
    match phn_core::text::normalize(s).as_str() {
        "woman" | "female" | "f" | "w" => Ok(true),
        "man" | "male" | "m" => Ok(false),
        _ => Err(PhnError::InvalidInput(
            "sex must be \"man\" or \"woman\": CDC's Prediabetes Risk Test scores only those two \
             answers (and, for a woman, gestational diabetes)"
                .into(),
        )),
    }
}

fn check_age(a: f64) -> Result<()> {
    if !a.is_finite() || !(0.0..=MAX_AGE).contains(&a) {
        return Err(PhnError::InvalidInput(format!(
            "age must be between 0 and {MAX_AGE} years"
        )));
    }
    if a < 18.0 {
        return Err(PhnError::InvalidInput(
            "CDC's Prediabetes Risk Test and the National DPP lifestyle change program are for \
             adults (the program requires age 18 or older), so this tool does not score anyone \
             under 18"
                .into(),
        ));
    }
    Ok(())
}

/// Validate the BMI (given or from height and weight).
fn bmi_value(args: &DiabetesPreventionArgs) -> Result<Option<(f64, &'static str)>> {
    if let Some(b) = args.bmi {
        if !b.is_finite() || !(10.0..=100.0).contains(&b) {
            return Err(PhnError::InvalidInput(
                "bmi must be between 10 and 100".into(),
            ));
        }
        return Ok(Some((b, "given")));
    }
    match (args.height_inches, args.weight_pounds) {
        (Some(h), Some(w)) => {
            if !h.is_finite() || !(36.0..=96.0).contains(&h) {
                return Err(PhnError::InvalidInput(
                    "height_inches must be between 36 and 96 inches".into(),
                ));
            }
            if !w.is_finite() || !(50.0..=800.0).contains(&w) {
                return Err(PhnError::InvalidInput(
                    "weight_pounds must be between 50 and 800 pounds".into(),
                ));
            }
            Ok(Some((bmi_from(h, w), "from height and weight")))
        }
        (None, None) => Ok(None),
        _ => Err(PhnError::InvalidInput(
            "give both height_inches and weight_pounds (or a bmi)".into(),
        )),
    }
}

struct Scored {
    inputs: RiskInputs,
    result: RiskTestResult,
}

/// Score the test when any scoring input was given; error when some are missing.
fn score_args(c: &DppContent, args: &DiabetesPreventionArgs) -> Result<Option<Scored>> {
    let bmi = bmi_value(args)?;
    let any = args.age.is_some()
        || args.sex.is_some()
        || args.family_history.is_some()
        || args.high_blood_pressure.is_some()
        || args.physically_active.is_some()
        || bmi.is_some();
    if !any {
        return Ok(None);
    }
    let mut missing: Vec<&str> = Vec::new();
    if args.age.is_none() {
        missing.push("age");
    }
    if args.sex.is_none() {
        missing.push("sex");
    }
    if args.family_history.is_none() {
        missing.push("family_history");
    }
    if args.high_blood_pressure.is_none() {
        missing.push("high_blood_pressure");
    }
    if args.physically_active.is_none() {
        missing.push("physically_active");
    }
    if bmi.is_none() {
        missing.push("bmi (or height_inches and weight_pounds)");
    }
    if !missing.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "to score the Prediabetes Risk Test, also give: {}",
            missing.join(", ")
        )));
    }
    let age = args.age.unwrap_or_default();
    check_age(age)?;
    let woman = parse_sex(args.sex.as_deref().unwrap_or_default())?;
    let (bmi_v, bmi_how) = bmi.unwrap_or((0.0, ""));
    let inputs = RiskInputs {
        age,
        woman,
        gestational_diabetes: woman && args.gestational_diabetes.unwrap_or(false),
        family_history: args.family_history.unwrap_or(false),
        high_blood_pressure: args.high_blood_pressure.unwrap_or(false),
        physically_active: args.physically_active.unwrap_or(false),
        bmi: bmi_v,
        asian_american: args.asian_american.unwrap_or(false),
    };
    let (total, items) = score(&inputs);
    let high = total >= c.risk_test.high_risk_at;
    let mut notes = vec![format!(
        "BMI {:.1} ({bmi_how}; {}).",
        bmi_v, c.risk_test.bmi.formula
    )];
    if woman && args.gestational_diabetes.is_none() {
        notes.push(
            "Gestational diabetes was not given, so it is scored as no (CDC adds 1 point for a \
             woman who has had it)."
                .into(),
        );
    }
    if !woman && args.gestational_diabetes == Some(true) {
        notes.push("Gestational diabetes applies to women and was ignored.".into());
    }
    if args.asian_american.is_none() {
        notes.push(
            "Asian American was not given, so the BMI cut points for other groups were used \
             (CDC uses 23 instead of 25 for Asian American people)."
                .into(),
        );
    }
    notes.push(
        "The tool applies CDC's BMI cut points to a BMI rounded to one decimal; CDC's online \
         test may use a height and weight table, so a score at a cut point can differ (not \
         verified)."
            .into(),
    );
    Ok(Some(Scored {
        inputs,
        result: RiskTestResult {
            score: total,
            maximum: c.risk_test.maximum,
            high_risk: high,
            items,
            bmi: (bmi_v * 10.0).round() / 10.0,
            interpretation: if high {
                format!(
                    "A score of {} (5 or higher) is considered high risk for having prediabetes \
                     (CDC). Only a blood test can diagnose it: suggest a blood glucose test, and \
                     the lifestyle change program if prediabetes is confirmed.",
                    total
                )
            } else {
                format!(
                    "A score of {total} is below CDC's high-risk cut-off of 5. This does not rule \
                     out prediabetes: CDC advises regular screening (a blood test can diagnose it)."
                )
            },
            notes,
        },
    }))
}

// ---------------------------------------------------------------- eligibility

fn status(ok: Option<bool>) -> &'static str {
    match ok {
        Some(true) => "met",
        Some(false) => "not met",
        None => "unknown",
    }
}

fn in_range(v: f64, r: [f64; 2]) -> bool {
    v >= r[0] && v <= r[1]
}

fn eligibility(
    c: &DppContent,
    args: &DiabetesPreventionArgs,
    scored: &Option<Scored>,
) -> Option<EligibilityResult> {
    let given = args.age.is_some()
        || args.diabetes_diagnosis.is_some()
        || args.pregnant.is_some()
        || args.a1c_percent.is_some()
        || args.fasting_glucose_mg_dl.is_some()
        || args.two_hour_glucose_mg_dl.is_some()
        || args.gestational_diabetes.is_some()
        || scored.is_some();
    if !given {
        return None;
    }
    let e = &c.eligibility;
    let age_ok = args.age.map(|a| a >= 18.0);
    let bmi_ok = scored
        .as_ref()
        .map(|s| s.result.bmi >= if s.inputs.asian_american { 23.0 } else { 25.0 });
    let dx_ok = args.diabetes_diagnosis.map(|d| !d);
    let preg_ok = args.pregnant.map(|p| !p);
    let all_of = vec![
        RequirementStatus {
            requirement: e.must_meet_all[0].clone(),
            status: status(age_ok).into(),
            detail: args
                .age
                .map(|a| format!("age {}", a.floor() as i64))
                .unwrap_or_else(|| "age not given".into()),
        },
        RequirementStatus {
            requirement: e.must_meet_all[1].clone(),
            status: status(bmi_ok).into(),
            detail: scored
                .as_ref()
                .map(|s| {
                    format!(
                        "BMI {:.1}{}",
                        s.result.bmi,
                        if s.inputs.asian_american {
                            " (Asian American cut point 23)"
                        } else {
                            " (cut point 25; 23 if Asian American)"
                        }
                    )
                })
                .unwrap_or_else(|| "BMI not given (give the risk test answers or a bmi)".into()),
        },
        RequirementStatus {
            requirement: e.must_meet_all[2].clone(),
            status: status(dx_ok).into(),
            detail: "from diabetes_diagnosis".into(),
        },
        RequirementStatus {
            requirement: e.must_meet_all[3].clone(),
            status: status(preg_ok).into(),
            detail: "from pregnant".into(),
        },
    ];
    // One-of routes.
    let tests: Vec<(&str, f64, [f64; 2])> = [
        ("A1C", args.a1c_percent, e.a1c_range),
        (
            "fasting plasma glucose",
            args.fasting_glucose_mg_dl,
            e.fasting_range,
        ),
        (
            "2-hour plasma glucose",
            args.two_hour_glucose_mg_dl,
            e.two_hour_range,
        ),
    ]
    .into_iter()
    .filter_map(|(n, v, r)| v.filter(|x| x.is_finite()).map(|v| (n, v, r)))
    .collect();
    let blood_ok = if tests.is_empty() {
        None
    } else {
        Some(tests.iter().any(|(_, v, r)| in_range(*v, *r)))
    };
    let blood_detail = if tests.is_empty() {
        "no blood test result given".to_string()
    } else {
        tests
            .iter()
            .map(|(n, v, r)| {
                format!(
                    "{n} {v} ({} the prediabetes range {} to {})",
                    if in_range(*v, *r) { "in" } else { "outside" },
                    r[0],
                    r[1]
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let gdm_ok = args.gestational_diabetes;
    let risk_ok = scored.as_ref().map(|s| s.result.high_risk);
    let one_of = vec![
        RequirementStatus {
            requirement: e.must_meet_one[0].clone(),
            status: status(blood_ok).into(),
            detail: blood_detail,
        },
        RequirementStatus {
            requirement: e.must_meet_one[1].clone(),
            status: status(gdm_ok).into(),
            detail: "from gestational_diabetes".into(),
        },
        RequirementStatus {
            requirement: e.must_meet_one[2].clone(),
            status: status(risk_ok).into(),
            detail: scored
                .as_ref()
                .map(|s| format!("score {} of {}", s.result.score, s.result.maximum))
                .unwrap_or_else(|| "risk test not scored".into()),
        },
    ];
    let any_not_met_all = all_of.iter().any(|r| r.status == "not met");
    let all_met = all_of.iter().all(|r| r.status == "met");
    let one_met = one_of.iter().any(|r| r.status == "met");
    let one_unknown = one_of.iter().any(|r| r.status == "unknown");
    let overall = if any_not_met_all {
        "Does not meet CDC eligibility"
    } else if all_met && one_met {
        "Meets CDC eligibility"
    } else if all_met && !one_unknown {
        "Does not meet CDC eligibility"
    } else {
        "Cannot tell from what was given"
    };
    Some(EligibilityResult {
        overall: overall.into(),
        all_of,
        one_of,
        medicare_note: e.medicare_note.clone(),
    })
}

// ---------------------------------------------------------------- programs

fn site_out(s: &DppSite) -> SiteOut {
    let city = if s.city.is_empty() {
        String::new()
    } else {
        format!(", {}", s.city)
    };
    let tail = format!(
        "{city}{}{}",
        if s.state.is_empty() {
            String::new()
        } else {
            format!(", {}", s.state)
        },
        if s.zip.is_empty() {
            String::new()
        } else {
            format!(" {}", s.zip)
        }
    );
    SiteOut {
        site_name: s.site_name.clone(),
        listed_place: s.listed_place.clone(),
        municipality: Some(s.municipality.clone()).filter(|m| !m.is_empty()),
        county: Some(s.county.clone()).filter(|m| !m.is_empty()),
        address: format!("{}{tail}", s.street),
        organization_phone: s.org_phone.clone(),
        organization_email: s.org_email.clone(),
        contact_note: if s.has_named_contact {
            "DPH lists a program contact person for this site; the name, email, and phone of a \
             person are not kept here. Use DPH's list (link below) or call the organization."
                .into()
        } else if s.org_phone.is_none() && s.org_email.is_none() {
            "No contact is listed by DPH for this site.".into()
        } else {
            "Organization contact as listed by DPH.".into()
        },
    }
}

fn programs_for(
    ctx: &Context,
    c: &DppContent,
    area_arg: Option<&str>,
) -> Result<(ProgramsOut, Source)> {
    let settings = ctx.settings();
    let q = match area_arg {
        Some(q) if !q.trim().is_empty() => Some(q.to_string()),
        _ => settings.jurisdiction.town.clone(),
    };
    let a = area::resolve(ctx, q.as_deref(), true)?;
    let data = DppPrograms::load(&ctx.snapshots)?;
    let mut in_area = Vec::new();
    let mut same_county = Vec::new();
    let mut outside = Vec::new();
    for s in &data.sites {
        let o = site_out(s);
        if s.municipality.is_empty() {
            outside.push(o);
            continue;
        }
        match a.level {
            AreaLevel::State => in_area.push(o),
            AreaLevel::County => {
                if Some(&s.county) == a.county.as_ref() {
                    in_area.push(o)
                }
            }
            AreaLevel::Town => {
                if Some(&s.municipality) == a.town.as_ref() {
                    in_area.push(o)
                } else if Some(&s.county) == a.county.as_ref() {
                    same_county.push(o)
                }
            }
        }
    }
    if a.level != AreaLevel::State {
        outside.clear();
    }
    let level = match a.level {
        AreaLevel::State => "state",
        AreaLevel::County => "county",
        AreaLevel::Town => "town",
    };
    let out = ProgramsOut {
        area: a.name.clone(),
        area_level: level.into(),
        in_area,
        same_county,
        outside_massachusetts: outside,
        sites_listed_statewide: data.sites.len(),
        source_note: format!(
            "These are the sites on the Massachusetts Department of Public Health's list of \
             \"Active MA DPPs\" ({}). The list shows no update date and does not say which sites \
             are CDC-recognized, how each program is delivered (in person or online), or its \
             schedule: call a site before referring.",
            data.vintage
        ),
        registry_gap: c
            .program
            .registry_gap
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        verify_links: vec![
            format!(
                "DPH list of programs (with contacts): {}",
                dpp::DPH_LIST_URL
            ),
            format!(
                "CDC registry of recognized organizations: {}",
                dpp::CDC_REGISTRY_URL
            ),
            format!(
                "CDC Find a Lifestyle Change Program (filters by delivery mode and language): {}",
                dpp::CDC_FIND_URL
            ),
        ],
    };
    Ok((out, data.source))
}

// ---------------------------------------------------------------- local responses

const AGE: &[&str] = &["age", "age years", "age in years"];
const SEX: &[&str] = &["sex", "gender", "sex assigned at birth"];
const FAMILY: &[&str] = &[
    "family history",
    "family history of diabetes",
    "family diabetes",
    "family member with diabetes",
];
const HBP: &[&str] = &[
    "high blood pressure",
    "hypertension",
    "hbp",
    "high bp",
    "diagnosed high blood pressure",
];
const ACTIVE: &[&str] = &[
    "physically active",
    "physical activity",
    "active",
    "exercise",
];
const HEIGHT: &[&str] = &["height inches", "height in", "height", "height in inches"];
const WEIGHT: &[&str] = &[
    "weight pounds",
    "weight lbs",
    "weight lb",
    "weight",
    "weight in pounds",
];
const BMI: &[&str] = &["bmi", "body mass index"];
const GDM: &[&str] = &[
    "gestational diabetes",
    "gdm",
    "history of gestational diabetes",
];
const ASIAN: &[&str] = &["asian american", "asian", "asian american race"];

fn summarize_responses(
    c: &DppContent,
    lt: &crate::localdata::LocalTable,
    threshold: u32,
) -> Result<ResponsesSummary> {
    let h = &lt.table.headers;
    let checked = |aliases: &[&str]| -> Result<Option<usize>> {
        match find_column(h, aliases) {
            Some(i) => lt.column(&h[i]).map(Some),
            // Not present.
            None => Ok(None),
        }
    };
    // Yes/no answers: header words such as "history" alone do not make a column free text.
    let yes_no = |aliases: &[&str]| -> Result<(Option<usize>, Vec<Option<bool>>)> {
        match find_column(h, aliases) {
            Some(i) => Ok((Some(i), yes_no_column(lt, i)?)),
            None => Ok((None, vec![None; lt.table.rows.len()])),
        }
    };
    let age = checked(AGE)?;
    let sex = checked(SEX)?;
    let (fam, fam_v) = yes_no(FAMILY)?;
    let (hbp, hbp_v) = yes_no(HBP)?;
    let (active, active_v) = yes_no(ACTIVE)?;
    let height = checked(HEIGHT)?;
    let weight = checked(WEIGHT)?;
    let bmi = checked(BMI)?;
    let (gdm, gdm_v) = yes_no(GDM)?;
    let (asian, asian_v) = yes_no(ASIAN)?;
    let has_bmi = bmi.is_some() || (height.is_some() && weight.is_some());
    if age.is_none()
        || sex.is_none()
        || fam.is_none()
        || hbp.is_none()
        || active.is_none()
        || !has_bmi
    {
        return Err(PhnError::InvalidInput(format!(
            "'{}' does not have every column the Prediabetes Risk Test needs: age, sex, family \
             history, high blood pressure, physically active, and either BMI or height (inches) \
             and weight (pounds). The columns in the file are: {}",
            lt.label,
            h.join(", ")
        )));
    }
    let n = lt.table.rows.len();
    let num = |i: Option<usize>| -> Vec<Option<f64>> {
        match i {
            Some(i) => lt.numeric(i).0,
            None => vec![None; n],
        }
    };
    let (ages, heights, weights, bmis) = (num(age), num(height), num(weight), num(bmi));
    let cell = |r: usize, i: Option<usize>| -> String {
        i.and_then(|i| lt.table.rows[r].get(i))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let mut by_result: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_age: BTreeMap<String, u64> = BTreeMap::new();
    let mut counted = 0usize;
    for r in 0..n {
        if lt.table.rows[r].iter().all(|v| v.trim().is_empty()) {
            continue;
        }
        counted += 1;
        let a = ages[r];
        let ag = match a {
            Some(a) if (0.0..=MAX_AGE).contains(&a) => age_group(a).to_string(),
            _ => AGE_MISSING.to_string(),
        };
        *by_age.entry(ag).or_default() += 1;
        let bmi_v = bmis[r].or_else(|| match (heights[r], weights[r]) {
            (Some(hh), Some(w)) if (36.0..=96.0).contains(&hh) && (50.0..=800.0).contains(&w) => {
                Some(bmi_from(hh, w))
            }
            _ => None,
        });
        let woman = parse_sex(&cell(r, sex)).ok();
        let inputs = match (a, woman, fam_v[r], hbp_v[r], active_v[r], bmi_v) {
            (Some(a), Some(w), Some(f), Some(hp), Some(ac), Some(b))
                if (18.0..=MAX_AGE).contains(&a) && (10.0..=100.0).contains(&b) =>
            {
                Some(RiskInputs {
                    age: a,
                    woman: w,
                    gestational_diabetes: w && gdm_v[r].unwrap_or(false),
                    family_history: f,
                    high_blood_pressure: hp,
                    physically_active: ac,
                    bmi: b,
                    asian_american: asian_v[r].unwrap_or(false),
                })
            }
            _ => None,
        };
        let label = match inputs {
            Some(i) => {
                if score(&i).0 >= c.risk_test.high_risk_at {
                    "High risk (score 5 or higher)"
                } else {
                    "Not high risk (score 0 to 4)"
                }
            }
            None => "Could not be scored",
        };
        *by_result.entry(label.to_string()).or_default() += 1;
    }
    if counted == 0 {
        return Err(PhnError::InvalidInput(format!(
            "'{}' has no rows",
            lt.label
        )));
    }
    let mut age_order: Vec<&str> = AGE_GROUPS.to_vec();
    age_order.push(AGE_MISSING);
    let age_counts: Vec<(String, u64)> = tally(&age_order, &by_age)
        .into_iter()
        .filter(|(l, k)| *k > 0 || l != AGE_MISSING)
        .collect();
    let tables = vec![
        count_table(
            "Prediabetes Risk Test result",
            &tally(
                &[
                    "High risk (score 5 or higher)",
                    "Not high risk (score 0 to 4)",
                    "Could not be scored",
                ],
                &by_result,
            ),
            threshold,
        ),
        count_table("Age group", &age_counts, threshold),
    ];
    let mut found = Vec::new();
    for (name, idx) in [
        ("age", age),
        ("sex", sex),
        ("family history", fam),
        ("high blood pressure", hbp),
        ("physically active", active),
        ("height", height),
        ("weight", weight),
        ("bmi", bmi),
        ("gestational diabetes", gdm),
        ("asian american", asian),
    ] {
        if idx.is_some() {
            found.push(name.to_string());
        }
    }
    let mut notes = vec![
        "Counts from 1 to 4 and cells that would reveal them are shown as * (suppression \
         threshold from your settings). Tables are one-way on purpose."
            .to_string(),
        "A row is scored only when age (18 or older), sex (man or woman), family history, high \
         blood pressure, physical activity (yes or no), and a BMI (or height in inches and \
         weight in pounds) are all usable; other rows are counted as could not be scored."
            .to_string(),
    ];
    if gdm.is_none() {
        notes.push("No gestational diabetes column: scored as no for women.".into());
    }
    if asian.is_none() {
        notes.push(
            "No Asian American column: the BMI cut points for other groups were used.".into(),
        );
    }
    Ok(ResponsesSummary {
        source_file: lt.label.clone(),
        columns_found: found,
        tables,
        suppression_threshold: threshold,
        notes,
    })
}

// ---------------------------------------------------------------- handout

fn handout_values(score: Option<i64>, phone: &Option<String>) -> BTreeMap<String, String> {
    let mut v = BTreeMap::new();
    v.insert(
        "date".to_string(),
        chrono::Local::now().date_naive().to_string(),
    );
    v.insert(
        "score".to_string(),
        score.map(|s| s.to_string()).unwrap_or_default(),
    );
    v.insert(
        "program_phone".to_string(),
        phone.clone().unwrap_or_default(),
    );
    v
}

fn grade_of(markdown: &str) -> (f64, f64, bool) {
    let target = default_target_grade();
    let a = analyze(
        markdown,
        &AnalyzeOptions {
            target_grade: target,
            max_flags: 3,
            redact_excerpts: false,
        },
    );
    (a.grade.median_grade, target, a.grade.median_grade <= target)
}

fn write_handout(
    ctx: &Context,
    c: &DppContent,
    score_value: Option<i64>,
    format: DocFormat,
    phone: &Option<String>,
) -> Result<HandoutOut> {
    // Reading grade is measured on the handout with typical values filled in.
    let mut sample = handout_values(Some(6), &Some("413-555-0100".into()));
    sample.insert("date".into(), "2026-11-12".into());
    let (sample_md, _) = templates::fill(&c.handout.markdown, &sample, BLANK);
    let (grade, target, ok) = grade_of(&sample_md);
    let values = handout_values(score_value, phone);
    let (filled, _) = templates::fill(&c.handout.markdown, &values, BLANK);
    let doc = templates::document(ctx, &filled, "en-US", &c.handout.title);
    let (dir, _) = templates::output_folder(ctx, "diabetes_prevention")?;
    let file = templates::write_in(
        ctx,
        &dir,
        &format!("prediabetes_client_handout.{}", format.ext()),
        &format.render(&doc)?,
    )?;
    Ok(HandoutOut {
        title: c.handout.title.clone(),
        file,
        reading_grade: grade,
        target_grade: target,
        meets_target: ok,
    })
}

// ---------------------------------------------------------------- the tool

/// Diabetes prevention finder.
pub fn diabetes_prevention_finder(
    ctx: &Context,
    args: DiabetesPreventionArgs,
) -> Result<Envelope<DiabetesPreventionResult>> {
    let c: DppContent = content::load(DIABETES_PREVENTION)?;
    if args.sheet.is_some() && args.responses_path.is_none() {
        return Err(PhnError::InvalidInput(
            "sheet is used with responses_path (an XLSX workbook in /data)".into(),
        ));
    }
    for (name, v, lo, hi) in [
        ("a1c_percent", args.a1c_percent, 3.0, 20.0),
        (
            "fasting_glucose_mg_dl",
            args.fasting_glucose_mg_dl,
            30.0,
            700.0,
        ),
        (
            "two_hour_glucose_mg_dl",
            args.two_hour_glucose_mg_dl,
            30.0,
            1000.0,
        ),
    ] {
        if let Some(x) = v
            && (!x.is_finite() || !(lo..=hi).contains(&x))
        {
            return Err(PhnError::InvalidInput(format!(
                "{name} must be between {lo} and {hi}"
            )));
        }
    }
    if let Some(a) = args.age {
        check_age(a)?;
    }
    let settings = ctx.settings();
    let phone = settings
        .jurisdiction
        .board_of_health_phone
        .clone()
        .filter(|p| !p.trim().is_empty());
    let threshold = ctx.suppression_threshold();
    let mut caveats: Vec<String> = Vec::new();

    let scored = score_args(&c, &args)?;
    let elig = eligibility(&c, &args, &scored);
    let (programs, dph_source) = programs_for(ctx, &c, args.area.as_deref())?;

    let mut identifiers = Vec::new();
    let mut responses = None;
    let mut local_source = None;
    if let Some(p) = &args.responses_path {
        let lt = crate::localdata::load(ctx, p, args.sheet.as_deref())?;
        identifiers = lt.identifier_categories();
        responses = Some(summarize_responses(&c, &lt, threshold)?);
        local_source = Some(lt.source());
    }

    let mut files_written = Vec::new();
    let handout = if args.handout.unwrap_or(false) {
        let h = write_handout(
            ctx,
            &c,
            scored.as_ref().map(|s| s.result.score),
            args.format.unwrap_or_default(),
            &phone,
        )?;
        files_written.push(h.file.clone());
        if !h.meets_target {
            caveats.push(format!(
                "The handout reads at grade {:.1}, above the target of {}; simplify it before use.",
                h.reading_grade, h.target_grade
            ));
        }
        if phone.is_none() {
            caveats.push(
                "No board of health or program phone is saved, so the handout has a blank line \
                 for it; save it with configure_jurisdiction to fill it in."
                    .into(),
            );
        }
        Some(h)
    } else {
        None
    };

    let program_facts = content::cite_all(&c.program.facts, &c.sources);
    let delivery = DeliveryOut {
        modes: CitedOut {
            text: c.program.delivery_modes.text.clone(),
            citation: format!(
                "{}, Find a Program",
                c.sources
                    .get(&c.program.delivery_modes.source)
                    .map(|s| s.title.as_str())
                    .unwrap_or("CDC")
            ),
            confidence: Confidence::High,
        },
        how_to_find_online_and_distance_options: c.program.delivery_modes.how_to_filter.clone(),
        find_a_program_url: dpp::CDC_FIND_URL.into(),
        registry_url: dpp::CDC_REGISTRY_URL.into(),
    };
    let medicare = MedicareOut {
        facts: content::cite_all(&c.medicare.facts, &c.sources),
        finder: c.medicare.finder.clone(),
        eligibility_gap: c.medicare.eligibility_gap.clone(),
    };
    let masshealth = c
        .masshealth
        .statement
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let summary = summary_text(
        &c,
        &scored,
        &elig,
        &programs,
        &program_facts,
        &delivery,
        &medicare,
        &masshealth,
        &responses,
        &handout,
    );

    caveats.push(
        c.notices
            .decision_support
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    );
    caveats.push(
        c.notices
            .copyright
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    );
    caveats.push(programs.registry_gap.clone());
    caveats.push(
        "Each program site on DPH's list is shown as listed: DPH's contact column names \
         program coordinators, so only organization-level phone numbers and emails are bundled \
         (individual names, emails, and phone numbers are not)."
            .into(),
    );
    caveats.extend(content::review_caveats(&[DIABETES_PREVENTION]));

    let mut keys: Vec<&str> = vec![
        "cdc-test-scoring",
        "cdc-test-about",
        "cdc-test-pdf",
        "cdc-dpp-eligibility",
        "cdc-dpp-details",
        "cdc-dpp-find",
        "cdc-dpp-overview",
        "cms-mdpp",
        "dph-dpp",
        "cdc-copyright",
    ];
    keys.dedup();
    let mut sources = content::sources_for(keys, &c.sources);
    sources.push(dph_source);
    if let Some(ls) = local_source {
        sources.push(ls);
    }
    let mut notes = Vec::new();
    if responses.is_some() {
        notes.push(
            "Counts from your responses file are suppressed one-way tables; no row or value \
             from the file is returned."
                .into(),
        );
    }
    let identifiers: Vec<String> = identifiers
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(Envelope::new(
        summary,
        DiabetesPreventionResult {
            risk_test: scored.map(|s| s.result),
            eligibility: elig,
            programs,
            program_facts,
            delivery,
            medicare,
            masshealth,
            responses: responses.clone(),
            handout,
            decision_support_notice: c
                .notices
                .decision_support
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: responses.is_some(),
        suppression_threshold: responses.as_ref().map(|_| threshold),
        identifiers_detected: identifiers,
        files_written,
        notes,
    })
    .decision_support())
}

#[allow(clippy::too_many_arguments)]
fn summary_text(
    c: &DppContent,
    scored: &Option<Scored>,
    elig: &Option<EligibilityResult>,
    programs: &ProgramsOut,
    facts: &[CitedOut],
    delivery: &DeliveryOut,
    medicare: &MedicareOut,
    masshealth: &str,
    responses: &Option<ResponsesSummary>,
    handout: &Option<HandoutOut>,
) -> String {
    let mut s = String::from(
        "**Diabetes prevention finder** (CDC Prediabetes Risk Test and National DPP; decision support)\n\n",
    );
    if let Some(sc) = scored {
        let r = &sc.result;
        let rows: Vec<Vec<String>> = r
            .items
            .iter()
            .map(|i| vec![i.factor.clone(), i.answer.clone(), i.points.to_string()])
            .collect();
        s.push_str(&format!(
            "**Prediabetes Risk Test: {} of {}** ({})\n\n",
            r.score,
            r.maximum,
            if r.high_risk {
                "high risk"
            } else {
                "below the high-risk cut-off"
            }
        ));
        s.push_str(&md::table(&["Factor", "Answer", "Points"], &rows));
        s.push_str(&format!("\n{}\n\n", r.interpretation));
        for n in &r.notes {
            s.push_str(&format!("- {n}\n"));
        }
        s.push('\n');
    }
    if let Some(e) = elig {
        s.push_str(&format!("**CDC program eligibility: {}**\n\n", e.overall));
        let rows: Vec<Vec<String>> = e
            .all_of
            .iter()
            .map(|r| ("All of", r))
            .chain(e.one_of.iter().map(|r| ("One of", r)))
            .map(|(g, r)| {
                vec![
                    g.to_string(),
                    r.requirement.clone(),
                    r.status.clone(),
                    r.detail.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Rule", "Requirement", "Status", "Detail"],
            &rows,
        ));
        s.push_str(&format!("\n{}\n\n", e.medicare_note));
    }
    s.push_str(&format!(
        "**Program sites: {}** ({} site(s) in the area; {} statewide on DPH's list)\n\n",
        programs.area,
        programs.in_area.len(),
        programs.sites_listed_statewide
    ));
    let site_rows = |v: &[SiteOut]| -> Vec<Vec<String>> {
        v.iter()
            .map(|x| {
                vec![
                    x.site_name.clone(),
                    x.address.clone(),
                    x.organization_phone
                        .clone()
                        .unwrap_or_else(|| "see DPH list".into()),
                ]
            })
            .collect()
    };
    if programs.in_area.is_empty() {
        s.push_str(&format!(
            "DPH's list has no site in {}. See the same-county sites below, online and distance options, and CDC's finder.\n\n",
            programs.area
        ));
    } else {
        s.push_str(&md::table(
            &["Site", "Address", "Organization phone"],
            &site_rows(&programs.in_area),
        ));
        s.push('\n');
    }
    if !programs.same_county.is_empty() {
        s.push_str("Other sites in the same county:\n\n");
        s.push_str(&md::table(
            &["Site", "Address", "Organization phone"],
            &site_rows(&programs.same_county),
        ));
        s.push('\n');
    }
    s.push_str(&format!(
        "{}\n\n{}\n\n",
        programs.source_note, programs.registry_gap
    ));
    s.push_str("**The program (CDC)**\n\n");
    for f in facts {
        s.push_str(&content::bullet(f));
    }
    s.push_str(&format!(
        "\n**Delivery modes (CDC):** {}\n\n{}\n\n",
        delivery.modes.text, delivery.how_to_find_online_and_distance_options
    ));
    s.push_str("**Medicare Diabetes Prevention Program (CMS)**\n\n");
    for f in &medicare.facts {
        s.push_str(&content::bullet(f));
    }
    s.push_str(&format!(
        "\n{} {}\n\n",
        medicare.finder, medicare.eligibility_gap
    ));
    s.push_str(&format!("**MassHealth:** {masshealth}\n\n"));
    if let Some(r) = responses {
        s.push_str(&format!(
            "**Your responses file** `{}` (suppressed counts only; threshold {}):\n\n",
            r.source_file, r.suppression_threshold
        ));
        for t in &r.tables {
            let rows: Vec<Vec<String>> = t
                .rows
                .iter()
                .map(|x| vec![x.label.clone(), x.count.clone()])
                .collect();
            s.push_str(&format!("_{}_\n\n", t.title));
            s.push_str(&md::table(&["Group", "Count"], &rows));
            s.push_str(&format!("\nAll rows: {}\n\n", t.total));
        }
        for n in &r.notes {
            s.push_str(&format!("{n}\n\n"));
        }
    }
    if let Some(h) = handout {
        s.push_str(&format!(
            "**Client handout** written to `{}` (reading grade {:.1}; target {}).\n\n",
            h.file, h.reading_grade, h.target_grade
        ));
    }
    let _ = c;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> DppContent {
        content::load(DIABETES_PREVENTION).unwrap()
    }

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn inputs() -> RiskInputs {
        RiskInputs {
            age: 30.0,
            woman: true,
            gestational_diabetes: false,
            family_history: false,
            high_blood_pressure: false,
            physically_active: true,
            bmi: 21.0,
            asian_american: false,
        }
    }

    #[test]
    fn content_loads_and_matches_the_scoring_code() {
        let c = content();
        assert_eq!(c.risk_test.high_risk_at, 5);
        assert_eq!(c.risk_test.maximum, 10);
        // The largest point value of each item adds to the maximum.
        let max: i64 = c
            .risk_test
            .items
            .iter()
            .map(|i| i.points.iter().map(|p| p.points).max().unwrap())
            .sum();
        assert_eq!(max, c.risk_test.maximum);
        // The code reaches the same maximum.
        let worst = RiskInputs {
            age: 70.0,
            woman: false,
            gestational_diabetes: false,
            family_history: true,
            high_blood_pressure: true,
            physically_active: false,
            bmi: 45.0,
            asian_american: false,
        };
        assert_eq!(score(&worst).0, 10);
        assert_eq!(score(&inputs()).0, 0);
        for f in c.program.facts.iter().chain(&c.medicare.facts) {
            assert!(c.sources.contains_key(&f.source), "{}", f.source);
        }
        assert_eq!(c.meta.review.status, phn_content::ReviewStatus::Pending);
    }

    /// CDC's published example: 5 feet 3 inches and 120 pounds is a BMI of 21 (normal range).
    #[test]
    fn bmi_matches_cdcs_example() {
        let b = bmi_from(63.0, 120.0);
        assert_eq!(b.round(), 21.0);
        assert!((18.5..=24.9).contains(&b));
    }

    #[test]
    fn points_follow_cdcs_table() {
        let mut i = inputs();
        for (age, want) in [
            (39.0, 0),
            (40.0, 1),
            (49.0, 1),
            (50.0, 2),
            (59.0, 2),
            (60.0, 3),
            (85.0, 3),
        ] {
            i.age = age;
            assert_eq!(score(&i).1[0].points, want, "age {age}");
        }
        i.age = 30.0;
        // Sex: man 1, woman 0, woman with gestational diabetes 1.
        i.woman = false;
        assert_eq!(score(&i).1[1].points, 1);
        i.woman = true;
        assert_eq!(score(&i).1[1].points, 0);
        i.gestational_diabetes = true;
        assert_eq!(score(&i).1[1].points, 1);
        i.gestational_diabetes = false;
        // BMI points, with the lower cut point for Asian American people.
        for (bmi, asian, want) in [
            (24.9, false, 0),
            (25.0, false, 1),
            (29.9, false, 1),
            (30.0, false, 2),
            (39.9, false, 2),
            (40.0, false, 3),
            (22.9, true, 0),
            (23.0, true, 1),
            (29.9, true, 1),
            (30.0, true, 2),
        ] {
            i.bmi = bmi;
            i.asian_american = asian;
            assert_eq!(score(&i).1[5].points, want, "bmi {bmi} asian {asian}");
        }
        // Activity: yes 0, no 1; family history and high blood pressure: yes 1.
        let mut j = inputs();
        j.physically_active = false;
        j.family_history = true;
        j.high_blood_pressure = true;
        assert_eq!(score(&j).0, 3);
        // Five points is high risk.
        j.age = 50.0;
        j.woman = false;
        assert_eq!(score(&j).0, 6);
    }

    #[test]
    fn the_score_needs_every_answer_and_adults() {
        let c = content();
        let err = score_args(
            &c,
            &DiabetesPreventionArgs {
                age: Some(55.0),
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("sex"));
        let err = score_args(
            &c,
            &DiabetesPreventionArgs {
                age: Some(15.0),
                sex: Some("woman".into()),
                family_history: Some(false),
                high_blood_pressure: Some(false),
                physically_active: Some(true),
                bmi: Some(22.0),
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("adults"));
        assert!(
            score_args(&c, &DiabetesPreventionArgs::default())
                .unwrap()
                .is_none()
        );
        assert!(parse_sex("nonbinary").is_err());
    }

    fn full(age: f64, bmi: f64) -> DiabetesPreventionArgs {
        DiabetesPreventionArgs {
            age: Some(age),
            sex: Some("man".into()),
            family_history: Some(true),
            high_blood_pressure: Some(true),
            physically_active: Some(false),
            bmi: Some(bmi),
            ..Default::default()
        }
    }

    #[test]
    fn eligibility_follows_cdcs_rules() {
        let c = content();
        let args = DiabetesPreventionArgs {
            diabetes_diagnosis: Some(false),
            pregnant: Some(false),
            ..full(52.0, 31.0)
        };
        let sc = score_args(&c, &args).unwrap();
        let e = eligibility(&c, &args, &sc).unwrap();
        assert_eq!(e.overall, "Meets CDC eligibility");
        // Existing diabetes excludes.
        let args = DiabetesPreventionArgs {
            diabetes_diagnosis: Some(true),
            pregnant: Some(false),
            ..full(52.0, 31.0)
        };
        let sc = score_args(&c, &args).unwrap();
        assert_eq!(
            eligibility(&c, &args, &sc).unwrap().overall,
            "Does not meet CDC eligibility"
        );
        // Nothing known about diabetes or pregnancy: cannot tell.
        let args = full(52.0, 31.0);
        let sc = score_args(&c, &args).unwrap();
        assert_eq!(
            eligibility(&c, &args, &sc).unwrap().overall,
            "Cannot tell from what was given"
        );
        // BMI 24 is below 25 (but meets 23 for an Asian American person).
        let mut low = DiabetesPreventionArgs {
            diabetes_diagnosis: Some(false),
            pregnant: Some(false),
            a1c_percent: Some(6.0),
            ..full(52.0, 24.0)
        };
        let sc = score_args(&c, &low).unwrap();
        assert_eq!(
            eligibility(&c, &low, &sc).unwrap().overall,
            "Does not meet CDC eligibility"
        );
        low.asian_american = Some(true);
        let sc = score_args(&c, &low).unwrap();
        assert_eq!(
            eligibility(&c, &low, &sc).unwrap().overall,
            "Meets CDC eligibility"
        );
        // A blood test outside the prediabetes range, a low score, and no gestational history.
        let args = DiabetesPreventionArgs {
            age: Some(30.0),
            sex: Some("woman".into()),
            family_history: Some(false),
            high_blood_pressure: Some(false),
            physically_active: Some(true),
            bmi: Some(27.0),
            diabetes_diagnosis: Some(false),
            pregnant: Some(false),
            a1c_percent: Some(5.2),
            gestational_diabetes: Some(false),
            ..Default::default()
        };
        let sc = score_args(&c, &args).unwrap();
        assert_eq!(
            eligibility(&c, &args, &sc).unwrap().overall,
            "Does not meet CDC eligibility"
        );
    }

    #[test]
    fn programs_are_listed_for_a_town_and_its_county() {
        let (_d, ctx) = ctx();
        let c = content();
        let (p, _) = programs_for(&ctx, &c, Some("Fall River")).unwrap();
        assert_eq!(p.area_level, "town");
        assert_eq!(p.in_area.len(), 4);
        assert!(
            p.same_county
                .iter()
                .any(|s| s.site_name.contains("Attleboro")
                    || s.municipality.as_deref() == Some("Taunton"))
        );
        assert!(p.source_note.contains("no update date"));
        // No person's name or personal email is kept.
        let json = serde_json::to_string(&p).unwrap();
        for who in [
            "Bermudez",
            "Potvin",
            "Charleus",
            "Souza",
            "@frfsa.org",
            "@ymcasc.org",
        ] {
            assert!(!json.contains(who), "{who}");
        }
        let (p, _) = programs_for(&ctx, &c, Some("Massachusetts")).unwrap();
        assert_eq!(p.area_level, "state");
        assert_eq!(p.in_area.len() + p.outside_massachusetts.len(), 73);
        assert_eq!(p.outside_massachusetts.len(), 1);
        let (p, _) = programs_for(&ctx, &c, Some("Berkshire County")).unwrap();
        assert!(
            p.in_area
                .iter()
                .all(|s| s.county.as_deref() == Some("Berkshire"))
        );
    }

    #[test]
    fn the_handout_meets_the_reading_target() {
        let c = content();
        let mut v = handout_values(Some(6), &Some("413-555-0100".into()));
        v.insert("date".into(), "2026-11-12".into());
        let (filled, unknown) = templates::fill(&c.handout.markdown, &v, BLANK);
        assert!(unknown.is_empty(), "{unknown:?}");
        let (grade, target, ok) = grade_of(&filled);
        assert!(ok, "handout reads at grade {grade} (target {target})");
    }

    #[test]
    fn responses_are_counted_with_suppression() {
        let (dir, ctx) = ctx();
        let mut s = String::from(
            "Name,Age,Sex,Family history,High blood pressure,Physically active,Height,Weight\n",
        );
        for i in 0..12 {
            s.push_str(&format!(
                "Ottilie Fernsby,{},woman,no,no,yes,64,130\n",
                25 + i
            ));
        }
        for i in 0..8 {
            s.push_str(&format!(
                "Bartholomew Quince,{},man,yes,yes,no,70,260\n",
                55 + i
            ));
        }
        s.push_str("Cornelius Ashgrove,50,man,yes,no,yes,,\n");
        std::fs::write(dir.path().join("data/events.csv"), s).unwrap();
        let env = diabetes_prevention_finder(
            &ctx,
            DiabetesPreventionArgs {
                area: Some("Springfield".into()),
                responses_path: Some("events.csv".into()),
                ..Default::default()
            },
        );
        // The Name column is an identifier: it is never read, so the file is accepted.
        let env = env.unwrap();
        let r = env.data.responses.as_ref().unwrap();
        let t = &r.tables[0];
        let get = |p: &str| {
            t.rows
                .iter()
                .find(|x| x.label.starts_with(p))
                .unwrap()
                .count
                .clone()
        };
        assert_eq!(get("Not high risk"), "12");
        // One row could not be scored: that count is hidden, and the smallest other count is
        // hidden with it so the pair cannot be recovered from the total.
        assert_eq!(get("Could not"), "*");
        assert_eq!(get("High risk"), "*");
        assert_eq!(t.total, "21");
        let json = serde_json::to_string(&env).unwrap();
        for secret in ["Ottilie", "Fernsby", "Quince", "Ashgrove"] {
            assert!(!json.contains(secret), "{secret} leaked");
        }
        assert!(env.privacy.suppression_applied);
        assert!(!env.privacy.row_level_data);
    }

    #[test]
    fn a_handout_and_the_finder_without_a_person() {
        let (_d, ctx) = ctx();
        let env = diabetes_prevention_finder(
            &ctx,
            DiabetesPreventionArgs {
                area: Some("Springfield".into()),
                handout: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(env.data.risk_test.is_none());
        assert!(env.data.handout.as_ref().unwrap().meets_target);
        assert!(env.data.masshealth.contains("No source was found"));
        assert!(
            env.data.programs.registry_gap.contains("could not be read")
                || env.data.programs.registry_gap.contains("could")
        );
    }
}
