//! CHR-04 `preventive_services_lookup`: USPSTF grade A and B recommendations
//! that fit a person's age, sex at birth, pregnancy status, and a few risk
//! factors, with the grade and date, the Task Force's own population wording,
//! plain-language client summaries, and Massachusetts program options.
//!
//! * The recommendations come from the bundled `uspstf-ab-recommendations`
//!   snapshot (read from the Task Force's published statements through PubMed;
//!   the USPSTF site and API were not reachable from the build network).
//! * The criteria the tool can apply (age, sex, pregnancy, and a few risk
//!   factors) and the client summaries are in
//!   `content/chronic/preventive_services.yaml`. Anything else in the published
//!   population is shown for the nurse to apply.
//! * ODPHP's MyHealthfinder is fetched live, when online, as a cross-check.
//!
//! The inputs carry no names or identifiers. Nothing is read from local files.

use std::collections::{BTreeMap, BTreeSet};

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::preventive::{self, MyHfTopic, Recommendation, Statement};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::comms::readability::{AnalyzeOptions, analyze, default_target_grade};
use crate::context::Context;
use crate::epr::content::{self, Confidence, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/chronic/preventive_services.yaml`.
pub const PREVENTIVE_SERVICES: &str = "chronic-preventive-services";

// ---------------------------------------------------------------- content

/// One alternative way a recommendation applies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criteria {
    pub age_min: Option<u32>,
    pub age_max: Option<u32>,
    #[serde(default = "any")]
    pub sex: String,
    #[serde(default = "any")]
    pub pregnancy: String,
    #[serde(default)]
    pub all_of: Vec<String>,
}

fn any() -> String {
    "any".into()
}

/// The project's layer on one snapshot recommendation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecDef {
    pub id: String,
    pub category: String,
    pub label: String,
    pub criteria: Vec<Criteria>,
    pub assumption: Option<String>,
    pub confirm: String,
    pub client_summary: String,
    pub programs: Vec<String>,
}

/// A Massachusetts program site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SiteOut {
    pub name: String,
    pub place: String,
    pub phone: String,
}

/// A Massachusetts program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramDef {
    pub id: String,
    pub name: String,
    pub offered_by: String,
    pub summary: String,
    pub eligibility: String,
    pub contact: String,
    pub sites: Vec<SiteOut>,
    pub url: String,
    pub sources: Vec<String>,
    pub confidence: Confidence,
}

/// `content/chronic/preventive_services.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreventiveContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub general_notes: Vec<String>,
    pub programs: Vec<ProgramDef>,
    pub recommendations: Vec<RecDef>,
}

// ---------------------------------------------------------------- arguments and results

/// Sex assigned at birth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SexAtBirth {
    Female,
    Male,
}

impl SexAtBirth {
    fn label(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
        }
    }
}

/// Pregnancy status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PregnancyStatus {
    NotPregnant,
    Pregnant,
    /// After the end of a pregnancy (the USPSTF statements say "postpartum").
    Postpartum,
}

impl PregnancyStatus {
    fn label(self) -> &'static str {
        match self {
            Self::NotPregnant => "not pregnant",
            Self::Pregnant => "pregnant",
            Self::Postpartum => "postpartum",
        }
    }
}

/// Tobacco use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TobaccoUse {
    Current,
    Former,
    Never,
}

/// Cardiovascular risk factors named in the USPSTF statins and diet-and-activity statements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CardiovascularFactor {
    Hypertension,
    ElevatedBloodPressure,
    Dyslipidemia,
    Diabetes,
    MetabolicSyndrome,
}

/// Areas of "increased risk" the USPSTF statements use. The clinician decides who is at
/// increased risk, using the statement's own risk descriptions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskArea {
    Hiv,
    HepatitisB,
    Syphilis,
    SexuallyTransmittedInfections,
    ChlamydiaGonorrhea,
    Tuberculosis,
    Falls,
    BreastCancer,
    /// Personal or family history of breast, ovarian, tubal, or peritoneal cancer, or ancestry
    /// associated with BRCA1/2 changes.
    BrcaRelatedCancer,
    Preeclampsia,
    PerinatalDepression,
    OsteoporoticFracture,
}

impl RiskArea {
    fn flag(self) -> &'static str {
        match self {
            Self::Hiv => "risk_hiv",
            Self::HepatitisB => "risk_hepatitis_b",
            Self::Syphilis => "risk_syphilis",
            Self::SexuallyTransmittedInfections => "risk_sti",
            Self::ChlamydiaGonorrhea => "risk_chlamydia_gonorrhea",
            Self::Tuberculosis => "risk_tuberculosis",
            Self::Falls => "risk_falls",
            Self::BreastCancer => "risk_breast_cancer",
            Self::BrcaRelatedCancer => "risk_brca",
            Self::Preeclampsia => "risk_preeclampsia",
            Self::PerinatalDepression => "risk_perinatal_depression",
            Self::OsteoporoticFracture => "risk_osteoporotic_fracture",
        }
    }
}

/// Risk factors the tool can use. Leave out what you do not know: the recommendations that
/// depend on it are listed under "needs review" instead of being matched or dropped.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct RiskFactors {
    /// Tobacco use: current, former, or never.
    pub tobacco_use: Option<TobaccoUse>,
    /// Lifetime smoking in pack-years (packs per day times years smoked).
    pub pack_years: Option<f64>,
    /// For a former smoker: years since quitting.
    pub years_since_quit: Option<f64>,
    /// Adult body mass index (kg/m2). For a child or teen use bmi_percentile.
    pub bmi: Option<f64>,
    /// Child or teen BMI percentile for age and sex (0 to 100).
    pub bmi_percentile: Option<f64>,
    /// Known cardiovascular risk factors. Give an empty list when the person has none; leave it
    /// out when unknown.
    pub cardiovascular_risk_factors: Option<Vec<CardiovascularFactor>>,
    /// Estimated 10-year cardiovascular disease risk in percent (from your risk calculator).
    pub ten_year_cvd_risk_percent: Option<f64>,
    /// Currently or ever sexually active (as the USPSTF statements use the term).
    pub sexually_active: Option<bool>,
    /// Postmenopausal.
    pub postmenopausal: Option<bool>,
    /// Fair skin type (burns easily).
    pub fair_skin: Option<bool>,
    /// The home water supply is low in fluoride (see community_water_fluoridation_status).
    pub home_water_low_in_fluoride: Option<bool>,
    /// Areas where the clinician judges the person to be at increased risk.
    pub increased_risk_for: Vec<RiskArea>,
    /// Areas where the clinician judges the person NOT to be at increased risk.
    pub not_at_increased_risk_for: Vec<RiskArea>,
}

/// Arguments for `preventive_services_lookup`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PreventiveArgs {
    /// Age in whole years (0 to 120). Required. Do not send a name, date of birth, or any other
    /// identifier.
    pub age_years: Option<u32>,
    /// Sex assigned at birth: "female" or "male". Required. The USPSTF recommendations are
    /// written by sex at birth or by organs present; for transgender and nonbinary people apply
    /// each one to the organs and risks the person has.
    pub sex_at_birth: Option<SexAtBirth>,
    /// not_pregnant, pregnant, or postpartum. When left out the person is treated as not
    /// pregnant and the pregnancy-specific recommendations are not evaluated (the response says
    /// how many).
    pub pregnancy_status: Option<PregnancyStatus>,
    /// Risk factors that decide some recommendations.
    pub risk_factors: RiskFactors,
    /// Also ask ODPHP's MyHealthfinder (live, no key) for its list for this age, sex, and
    /// pregnancy, as a cross-check that also covers vaccines and other non-USPSTF services
    /// (default true; skipped offline).
    pub include_myhealthfinder: Option<bool>,
    /// Write a plain-language handout with the client summaries to /data/output (default false).
    /// It has no name or identifier; the nurse adds those by hand if wanted.
    pub write_handout: Option<bool>,
    /// Handout format: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
}

/// The person the answer is for (as understood).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PersonUsed {
    pub age_years: u32,
    pub sex_at_birth: String,
    /// The status used ("not pregnant (assumed: not given)" when left out).
    pub pregnancy: String,
    /// The risk factors that were given.
    pub risk_factors_given: Vec<String>,
}

/// One recommendation in the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecommendationOut {
    pub id: String,
    pub label: String,
    pub category: String,
    /// USPSTF grade: "A" or "B".
    pub grade: String,
    /// The Task Force's name for the topic.
    pub topic: String,
    /// Journal publication date of the statement that carries this grade (YYYY-MM-DD).
    pub published: String,
    pub statement_title: String,
    /// Link to the statement (DOI).
    pub url: String,
    /// The Task Force's population wording, when the statement has one.
    pub population: Option<String>,
    /// The Task Force's recommendation sentence, as in the statement's abstract.
    pub recommendation: String,
    /// Text the statement's abstract gives after its graded sentences (scope notes), when there
    /// is any.
    pub statement_notes: Option<String>,
    /// What the tool could not check: apply it yourself.
    pub confirm: String,
    /// Where the tool chose an age range the statement does not give.
    pub assumption: Option<String>,
    /// Plain-language summary for the client (project draft, pending review).
    pub client_summary: String,
    /// Inputs that were missing (needs-review list only).
    pub missing_information: Vec<String>,
    /// Ids of the Massachusetts programs that relate to it.
    pub program_ids: Vec<String>,
}

/// A Massachusetts program option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramOut {
    pub id: String,
    pub name: String,
    pub offered_by: String,
    pub summary: String,
    pub eligibility: String,
    pub contact: String,
    pub sites: Vec<SiteOut>,
    pub url: String,
    pub confidence: String,
    /// Recommendation ids it relates to in this answer.
    pub related_recommendations: Vec<String>,
}

/// About the bundled snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotInfo {
    /// Date the snapshot was built from the published statements.
    pub retrieved: String,
    pub recommendations: usize,
    pub statements: usize,
    pub not_included: Vec<String>,
}

/// MyHealthfinder's list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MyHealthfinderOut {
    /// "live" or "cache".
    pub mode: String,
    /// Date retrieved.
    pub as_of: String,
    pub topics: Vec<MyHfTopic>,
    pub note: String,
}

/// Result of `preventive_services_lookup`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PreventiveResult {
    pub person: PersonUsed,
    /// Recommendations whose checkable criteria are all met (apply the "confirm" text yourself).
    pub applies: Vec<RecommendationOut>,
    /// Recommendations that fit on age, sex, and pregnancy but depend on a risk factor that was
    /// not given.
    pub needs_review: Vec<RecommendationOut>,
    /// How many of the bundled A and B recommendations do not fit this person.
    pub not_matched_count: usize,
    /// Pregnancy-specific recommendations that were not evaluated because pregnancy status was
    /// not given.
    pub pregnancy_specific_not_evaluated: Vec<String>,
    pub massachusetts_programs: Vec<ProgramOut>,
    pub snapshot: SnapshotInfo,
    /// MyHealthfinder's list, when it was reached.
    pub myhealthfinder: Option<MyHealthfinderOut>,
    /// Handout written to /data/output.
    pub handout_file: Option<String>,
    /// Reading grade of the handout (median of three formulas).
    pub handout_reading_grade: Option<f64>,
    pub scope_notes: Vec<String>,
}

// ---------------------------------------------------------------- evaluation

/// A fact that may be known or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tri {
    Yes,
    No,
    Unknown,
}

impl Tri {
    fn of(b: Option<bool>) -> Self {
        match b {
            Some(true) => Tri::Yes,
            Some(false) => Tri::No,
            None => Tri::Unknown,
        }
    }
}

/// Flags the criteria can name, worked out from the risk factors.
fn flag(name: &str, rf: &RiskFactors) -> Tri {
    let cvd_has = |set: &[CardiovascularFactor]| {
        rf.cardiovascular_risk_factors
            .as_ref()
            .is_some_and(|l| l.iter().any(|f| set.contains(f)))
    };
    match name {
        "tobacco_current" => match rf.tobacco_use {
            Some(TobaccoUse::Current) => Tri::Yes,
            Some(_) => Tri::No,
            None => Tri::Unknown,
        },
        "ever_smoked" => match rf.tobacco_use {
            Some(TobaccoUse::Current | TobaccoUse::Former) => Tri::Yes,
            Some(TobaccoUse::Never) => Tri::No,
            None => Tri::Unknown,
        },
        "tobacco_current_or_quit_15y" => match rf.tobacco_use {
            Some(TobaccoUse::Current) => Tri::Yes,
            Some(TobaccoUse::Never) => Tri::No,
            Some(TobaccoUse::Former) => match rf.years_since_quit {
                Some(y) => Tri::of(Some(y <= 15.0)),
                None => Tri::Unknown,
            },
            None => Tri::Unknown,
        },
        "pack_years_20" => match (rf.pack_years, rf.tobacco_use) {
            (Some(p), _) => Tri::of(Some(p >= 20.0)),
            (None, Some(TobaccoUse::Never)) => Tri::No,
            _ => Tri::Unknown,
        },
        "bmi_25" => rf.bmi.map_or(Tri::Unknown, |b| Tri::of(Some(b >= 25.0))),
        "bmi_30" => rf.bmi.map_or(Tri::Unknown, |b| Tri::of(Some(b >= 30.0))),
        "bmi_percentile_95" => rf
            .bmi_percentile
            .map_or(Tri::Unknown, |p| Tri::of(Some(p >= 95.0))),
        "cvd_risk_10pct" => rf
            .ten_year_cvd_risk_percent
            .map_or(Tri::Unknown, |r| Tri::of(Some(r >= 10.0))),
        // Statins: dyslipidemia, diabetes, hypertension, or smoking.
        "cvd_factor_statin" => {
            use CardiovascularFactor::*;
            if cvd_has(&[Dyslipidemia, Diabetes, Hypertension])
                || rf.tobacco_use == Some(TobaccoUse::Current)
            {
                Tri::Yes
            } else if rf.cardiovascular_risk_factors.is_some() && rf.tobacco_use.is_some() {
                Tri::No
            } else {
                Tri::Unknown
            }
        }
        // Diet and activity counseling: high or elevated blood pressure, dyslipidemia, metabolic
        // syndrome, or a 10-year risk of 7.5% or more.
        "cvd_factor_counseling" => {
            use CardiovascularFactor::*;
            if cvd_has(&[
                Hypertension,
                ElevatedBloodPressure,
                Dyslipidemia,
                MetabolicSyndrome,
            ]) || rf.ten_year_cvd_risk_percent.is_some_and(|r| r >= 7.5)
            {
                Tri::Yes
            } else if rf.cardiovascular_risk_factors.is_some()
                || rf.ten_year_cvd_risk_percent.is_some()
            {
                Tri::No
            } else {
                Tri::Unknown
            }
        }
        "sexually_active" => Tri::of(rf.sexually_active),
        "postmenopausal" => Tri::of(rf.postmenopausal),
        "fair_skin" => Tri::of(rf.fair_skin),
        "low_fluoride_water" => Tri::of(rf.home_water_low_in_fluoride),
        other => {
            if rf.increased_risk_for.iter().any(|a| a.flag() == other) {
                Tri::Yes
            } else if rf
                .not_at_increased_risk_for
                .iter()
                .any(|a| a.flag() == other)
            {
                Tri::No
            } else {
                Tri::Unknown
            }
        }
    }
}

/// What to supply when a flag is unknown.
fn flag_hint(name: &str) -> String {
    match name {
        "tobacco_current" | "ever_smoked" => "tobacco_use".into(),
        "tobacco_current_or_quit_15y" => {
            "tobacco_use (and years_since_quit for a former smoker)".into()
        }
        "pack_years_20" => "pack_years".into(),
        "bmi_25" | "bmi_30" => "bmi".into(),
        "bmi_percentile_95" => "bmi_percentile".into(),
        "cvd_risk_10pct" => "ten_year_cvd_risk_percent".into(),
        "cvd_factor_statin" => "cardiovascular_risk_factors and tobacco_use".into(),
        "cvd_factor_counseling" => {
            "cardiovascular_risk_factors or ten_year_cvd_risk_percent".into()
        }
        "sexually_active" => "sexually_active".into(),
        "postmenopausal" => "postmenopausal".into(),
        "fair_skin" => "fair_skin".into(),
        "low_fluoride_water" => "home_water_low_in_fluoride".into(),
        other => match other.strip_prefix("risk_") {
            Some(area) => format!("increased_risk_for or not_at_increased_risk_for: {area}"),
            None => other.to_string(),
        },
    }
}

/// The result of checking one alternative.
#[derive(Debug, Clone, PartialEq)]
enum AltResult {
    NoMatch,
    Match,
    Undetermined(Vec<String>),
    PregnancyNotGiven,
}

fn check_alt(
    c: &Criteria,
    age: u32,
    sex: SexAtBirth,
    pregnancy: Option<PregnancyStatus>,
    rf: &RiskFactors,
) -> AltResult {
    if c.age_min.is_some_and(|m| age < m) || c.age_max.is_some_and(|m| age > m) {
        return AltResult::NoMatch;
    }
    match (c.sex.as_str(), sex) {
        ("female", SexAtBirth::Male) | ("male", SexAtBirth::Female) => return AltResult::NoMatch,
        _ => {}
    }
    match c.pregnancy.as_str() {
        "pregnant" => match pregnancy {
            Some(PregnancyStatus::Pregnant) => {}
            Some(_) => return AltResult::NoMatch,
            None => return AltResult::PregnancyNotGiven,
        },
        "pregnant_or_postpartum" => match pregnancy {
            Some(PregnancyStatus::Pregnant | PregnancyStatus::Postpartum) => {}
            Some(PregnancyStatus::NotPregnant) => return AltResult::NoMatch,
            None => return AltResult::PregnancyNotGiven,
        },
        "not_pregnant" if pregnancy == Some(PregnancyStatus::Pregnant) => {
            return AltResult::NoMatch;
        }
        _ => {}
    }
    let mut missing = Vec::new();
    for f in &c.all_of {
        match flag(f, rf) {
            Tri::No => return AltResult::NoMatch,
            Tri::Unknown => missing.push(flag_hint(f)),
            Tri::Yes => {}
        }
    }
    if missing.is_empty() {
        AltResult::Match
    } else {
        AltResult::Undetermined(missing)
    }
}

/// How a recommendation fits a person.
#[derive(Debug, Clone, PartialEq)]
enum Fit {
    Applies,
    NeedsReview(Vec<String>),
    PregnancyNotGiven,
    No,
}

fn fit(
    def: &RecDef,
    age: u32,
    sex: SexAtBirth,
    pregnancy: Option<PregnancyStatus>,
    rf: &RiskFactors,
) -> Fit {
    let results: Vec<AltResult> = def
        .criteria
        .iter()
        .map(|c| check_alt(c, age, sex, pregnancy, rf))
        .collect();
    if results.contains(&AltResult::Match) {
        return Fit::Applies;
    }
    let mut missing: Vec<String> = Vec::new();
    let mut any_undetermined = false;
    for r in &results {
        if let AltResult::Undetermined(m) = r {
            any_undetermined = true;
            for x in m {
                if !missing.contains(x) {
                    missing.push(x.clone());
                }
            }
        }
    }
    if any_undetermined {
        return Fit::NeedsReview(missing);
    }
    if results.contains(&AltResult::PregnancyNotGiven) {
        return Fit::PregnancyNotGiven;
    }
    Fit::No
}

const CATEGORY_ORDER: &[&str] = &[
    "cancer",
    "heart_metabolic",
    "tobacco",
    "infectious",
    "mental_health",
    "pregnancy",
    "children",
    "older_adults",
    "injury_violence",
];

fn category_label(c: &str) -> &'static str {
    match c {
        "cancer" => "Cancer screening and prevention",
        "heart_metabolic" => "Heart health, blood sugar, and weight",
        "tobacco" => "Tobacco",
        "infectious" => "Infections",
        "mental_health" => "Mental health, alcohol, and drug use",
        "pregnancy" => "Pregnancy and postpartum",
        "children" => "Children and adolescents",
        "older_adults" => "Older adults",
        "injury_violence" => "Safety and violence",
        _ => "Other",
    }
}

fn category_rank(c: &str) -> usize {
    CATEGORY_ORDER.iter().position(|x| *x == c).unwrap_or(99)
}

// ---------------------------------------------------------------- validation

fn check_number(name: &str, v: Option<f64>, lo: f64, hi: f64) -> Result<()> {
    if let Some(x) = v
        && (!x.is_finite() || x < lo || x > hi)
    {
        return Err(PhnError::InvalidInput(format!(
            "{name} must be between {lo} and {hi}"
        )));
    }
    Ok(())
}

fn validate(args: &PreventiveArgs) -> Result<(u32, SexAtBirth)> {
    let age = args.age_years.ok_or_else(|| {
        PhnError::InvalidInput("age_years is required (whole years, 0 to 120)".into())
    })?;
    if age > 120 {
        return Err(PhnError::InvalidInput(
            "age_years must be between 0 and 120".into(),
        ));
    }
    let sex = args.sex_at_birth.ok_or_else(|| {
        PhnError::InvalidInput("sex_at_birth is required: \"female\" or \"male\"".into())
    })?;
    if let Some(p) = args.pregnancy_status
        && p != PregnancyStatus::NotPregnant
    {
        if sex == SexAtBirth::Male {
            return Err(PhnError::InvalidInput(
                "pregnancy_status pregnant or postpartum needs sex_at_birth female (the \
                 recommendations are keyed to sex at birth; use that for the organs present)"
                    .into(),
            ));
        }
        if age < 10 {
            return Err(PhnError::InvalidInput(
                "pregnancy_status pregnant or postpartum is not accepted under age 10".into(),
            ));
        }
    }
    let rf = &args.risk_factors;
    check_number("pack_years", rf.pack_years, 0.0, 300.0)?;
    check_number("years_since_quit", rf.years_since_quit, 0.0, 100.0)?;
    check_number("bmi", rf.bmi, 8.0, 100.0)?;
    check_number("bmi_percentile", rf.bmi_percentile, 0.0, 100.0)?;
    check_number(
        "ten_year_cvd_risk_percent",
        rf.ten_year_cvd_risk_percent,
        0.0,
        100.0,
    )?;
    for a in &rf.increased_risk_for {
        if rf.not_at_increased_risk_for.contains(a) {
            return Err(PhnError::InvalidInput(format!(
                "{} is listed both as increased risk and not at increased risk",
                a.flag().trim_start_matches("risk_")
            )));
        }
    }
    Ok((age, sex))
}

fn risk_factors_given(rf: &RiskFactors) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(t) = rf.tobacco_use {
        v.push(format!(
            "tobacco use: {}",
            match t {
                TobaccoUse::Current => "current",
                TobaccoUse::Former => "former",
                TobaccoUse::Never => "never",
            }
        ));
    }
    if let Some(p) = rf.pack_years {
        v.push(format!("pack-years: {p}"));
    }
    if let Some(y) = rf.years_since_quit {
        v.push(format!("years since quitting: {y}"));
    }
    if let Some(b) = rf.bmi {
        v.push(format!("BMI: {b}"));
    }
    if let Some(p) = rf.bmi_percentile {
        v.push(format!("BMI percentile: {p}"));
    }
    if let Some(l) = &rf.cardiovascular_risk_factors {
        v.push(format!("cardiovascular risk factors listed: {}", l.len()));
    }
    if let Some(r) = rf.ten_year_cvd_risk_percent {
        v.push(format!("10-year CVD risk: {r}%"));
    }
    for (name, val) in [
        ("sexually active", rf.sexually_active),
        ("postmenopausal", rf.postmenopausal),
        ("fair skin", rf.fair_skin),
        ("home water low in fluoride", rf.home_water_low_in_fluoride),
    ] {
        if let Some(b) = val {
            v.push(format!("{name}: {}", if b { "yes" } else { "no" }));
        }
    }
    for a in &rf.increased_risk_for {
        v.push(format!(
            "increased risk: {}",
            a.flag().trim_start_matches("risk_")
        ));
    }
    for a in &rf.not_at_increased_risk_for {
        v.push(format!(
            "not at increased risk: {}",
            a.flag().trim_start_matches("risk_")
        ));
    }
    v
}

// ---------------------------------------------------------------- the tool

fn out_rec(
    def: &RecDef,
    rec: &Recommendation,
    st: &Statement,
    missing: Vec<String>,
) -> RecommendationOut {
    RecommendationOut {
        id: def.id.clone(),
        label: def.label.clone(),
        category: def.category.clone(),
        grade: rec.grade.clone(),
        topic: rec.topic.clone(),
        published: st.published.clone(),
        statement_title: st.title.clone(),
        url: st.url.clone(),
        population: st.population.clone(),
        recommendation: rec.recommendation.clone(),
        statement_notes: st.additional_text.clone(),
        confirm: def.confirm.clone(),
        assumption: def.assumption.clone(),
        client_summary: def.client_summary.clone(),
        missing_information: missing,
        program_ids: def.programs.clone(),
    }
}

fn sort_recs(v: &mut [RecommendationOut]) {
    v.sort_by(|a, b| {
        category_rank(&a.category)
            .cmp(&category_rank(&b.category))
            .then(a.grade.cmp(&b.grade))
            .then(a.label.cmp(&b.label))
    });
}

/// Preventive services lookup.
pub async fn preventive_services_lookup(
    ctx: &Context,
    args: PreventiveArgs,
) -> Result<Envelope<PreventiveResult>> {
    let (age, sex) = validate(&args)?;
    let c: PreventiveContent = content::load(PREVENTIVE_SERVICES)?;
    let (snap, snap_entry) = preventive::load_snapshot(&ctx.snapshots)?;
    // A test also checks this; the tool refuses to guess if the two files drift apart.
    for d in &c.recommendations {
        if !snap.recommendations.iter().any(|r| r.id == d.id) {
            return Err(PhnError::Internal(format!(
                "content lists {} but the USPSTF snapshot does not",
                d.id
            )));
        }
    }
    let rf = &args.risk_factors;
    let pregnancy = args.pregnancy_status;
    let mut applies = Vec::new();
    let mut needs_review = Vec::new();
    let mut not_given: Vec<String> = Vec::new();
    let mut not_matched = 0usize;
    for rec in &snap.recommendations {
        let Some(def) = c.recommendations.iter().find(|d| d.id == rec.id) else {
            return Err(PhnError::Internal(format!(
                "the USPSTF snapshot has {} but the content does not",
                rec.id
            )));
        };
        let Some(st) = snap.statement(rec) else {
            continue;
        };
        match fit(def, age, sex, pregnancy, rf) {
            Fit::Applies => applies.push(out_rec(def, rec, st, Vec::new())),
            Fit::NeedsReview(m) => needs_review.push(out_rec(def, rec, st, m)),
            Fit::PregnancyNotGiven => {
                // Only people who could be pregnant are told about these.
                if sex == SexAtBirth::Female && (10..=55).contains(&age) {
                    not_given.push(def.label.clone());
                } else {
                    not_matched += 1;
                }
            }
            Fit::No => not_matched += 1,
        }
    }
    sort_recs(&mut applies);
    sort_recs(&mut needs_review);

    // Massachusetts programs for the recommendations listed.
    let mut programs: Vec<ProgramOut> = Vec::new();
    for r in applies.iter().chain(needs_review.iter()) {
        for pid in &r.program_ids {
            let Some(p) = c.programs.iter().find(|p| &p.id == pid) else {
                continue;
            };
            match programs.iter_mut().find(|x| &x.id == pid) {
                Some(x) => x.related_recommendations.push(r.id.clone()),
                None => programs.push(ProgramOut {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    offered_by: p.offered_by.clone(),
                    summary: p.summary.split_whitespace().collect::<Vec<_>>().join(" "),
                    eligibility: p
                        .eligibility
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                    contact: p.contact.clone(),
                    sites: p.sites.clone(),
                    url: p.url.clone(),
                    confidence: match p.confidence {
                        Confidence::High => "high",
                        Confidence::Medium => "medium",
                        Confidence::Low => "low",
                    }
                    .into(),
                    related_recommendations: vec![r.id.clone()],
                }),
            }
        }
    }

    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = vec![snap_entry.to_source()];
    let mut seen_pmids: BTreeSet<&str> = BTreeSet::new();
    for r in applies.iter().chain(needs_review.iter()) {
        let rec = snap.recommendations.iter().find(|x| x.id == r.id);
        if let Some(st) = rec.and_then(|x| snap.statement(x))
            && seen_pmids.insert(st.pmid.as_str())
        {
            sources.push(Source {
                title: st.title.clone(),
                publisher: format!("U.S. Preventive Services Task Force ({})", st.journal),
                url: Some(st.url.clone()),
                effective_date: Some(st.published.clone()),
                retrieved: Some(snap.retrieved.clone()),
                mode: SourceMode::Snapshot,
            });
        }
    }
    let mut used_keys: Vec<&str> = Vec::new();
    for p in &programs {
        if let Some(def) = c.programs.iter().find(|d| d.id == p.id) {
            used_keys.extend(def.sources.iter().map(String::as_str));
        }
    }
    sources.extend(content::sources_for(used_keys, &c.sources));

    // MyHealthfinder (live cross-check).
    let want_myhf = args.include_myhealthfinder.unwrap_or(true);
    let mut myhf: Option<MyHealthfinderOut> = None;
    if want_myhf {
        if ctx.http.is_offline() {
            caveats.push(
                "MyHealthfinder (ODPHP's federal list, which also covers vaccines and other \
                 non-USPSTF services) was not asked because the server is offline; it has no \
                 offline copy."
                    .into(),
            );
        } else {
            let pregnant = pregnancy == Some(PregnancyStatus::Pregnant);
            match preventive::fetch_myhf(&ctx.http, ctx.cache.as_ref(), age, sex.label(), pregnant)
                .await
            {
                Ok((topics, fetched)) => {
                    let as_of = phn_data::fetch::local_date(&fetched.as_of);
                    sources.push(Source {
                        title: "MyHealthfinder: preventive services by age, sex, and pregnancy"
                            .into(),
                        publisher: "Office of Disease Prevention and Health Promotion (ODPHP), HHS"
                            .into(),
                        url: Some(preventive::MYHF_HOME.into()),
                        effective_date: None,
                        retrieved: Some(as_of.clone()),
                        mode: fetched.mode,
                    });
                    myhf = Some(MyHealthfinderOut {
                        mode: fetched.mode.label().into(),
                        as_of,
                        topics,
                        note: "MyHealthfinder's recommendations come from the USPSTF, the CDC's \
                               ACIP, and HRSA (as advised by the American Academy of Pediatrics \
                               and others), so it lists vaccines and other services this tool \
                               does not. It uses only age, sex, and pregnancy; a topic here that \
                               is not in the lists above may be a non-USPSTF service or a \
                               recommendation outside the A and B grades."
                            .into(),
                    });
                }
                Err(e) => caveats.push(format!(
                    "MyHealthfinder could not be reached ({}); the USPSTF snapshot answer above \
                     is complete without it.",
                    phn_data::apifetch::reason(&e)
                )),
            }
        }
    }

    // Caveats.
    caveats.push(
        "This is the USPSTF grade A and B list only. It is not the full list of recommended \
         preventive services: the CDC/ACIP immunization schedule, the HRSA women's and children's \
         preventive services lists, and grade C recommendations are separate."
            .into(),
    );
    caveats.push(format!(
        "The recommendations were read from the Task Force's published statements (through \
         PubMed) on {}, because the USPSTF web site and API could not be reached; dates are \
         journal publication dates. A statement may have been updated since: check the \
         recommendation on uspreventiveservicestaskforce.org before relying on it.",
        snap.retrieved
    ));
    caveats.push(
        "The tool applies only age, sex at birth, pregnancy status, and the risk factors you \
         gave. The Task Force's full population wording (for example 'average risk', 'no known \
         disease', 'asymptomatic') is shown for each recommendation: apply it, and the 'confirm' \
         text, yourself."
            .into(),
    );
    caveats.push(
        "Recommendations are keyed to sex assigned at birth or the organs present. For \
         transgender and nonbinary clients, apply each one to the organs and risks the person \
         has."
            .into(),
    );
    if sex == SexAtBirth::Female && (10..=55).contains(&age) && pregnancy.is_none() {
        caveats.push(format!(
            "Pregnancy status was not given, so the person was treated as not pregnant and {} \
             pregnancy-specific recommendations were not evaluated ({}). Give pregnancy_status \
             to include them.",
            not_given.len(),
            not_given.join("; ")
        ));
    }
    let assumed: Vec<&str> = applies
        .iter()
        .chain(needs_review.iter())
        .filter(|r| r.assumption.is_some())
        .map(|r| r.label.as_str())
        .collect();
    if !assumed.is_empty() {
        caveats.push(format!(
            "For these recommendations the Task Force's wording gives no age range and the tool \
             chose one (see each 'assumption'): {}.",
            assumed.join("; ")
        ));
    }
    if !needs_review.is_empty() {
        caveats.push(format!(
            "{} recommendation(s) fit on age, sex, and pregnancy but depend on information that \
             was not given (see 'needs review'). Leaving a risk factor out never removes a \
             recommendation; it only moves it to that list.",
            needs_review.len()
        ));
    }
    for n in &snap.not_included {
        caveats.push(n.clone());
    }
    for n in &c.general_notes {
        if !n.starts_with("The tool lists USPSTF grade A and B")
            && !n.starts_with("The tool applies only")
            && !n.starts_with("USPSTF recommendations are written")
        {
            caveats.push(n.clone());
        }
    }
    caveats.extend(content::review_caveats(&[PREVENTIVE_SERVICES]));
    if !programs.is_empty() {
        caveats.push(
            "Massachusetts program details are what the DPH pages said when they were read; the \
             pages show no last-updated date. Confirm eligibility and contact details with the \
             program before referring."
                .into(),
        );
    }

    // Handout (no identifiers).
    let mut handout_file = None;
    let mut handout_grade = None;
    let mut files_written = Vec::new();
    if args.write_handout.unwrap_or(false) {
        let md = handout_markdown(&applies, &needs_review, &snap.retrieved);
        let target = default_target_grade();
        let a = analyze(
            &md,
            &AnalyzeOptions {
                target_grade: target,
                max_flags: 3,
                redact_excerpts: false,
            },
        );
        handout_grade = Some(a.grade.median_grade);
        if a.grade.median_grade > target {
            caveats.push(format!(
                "The handout reads at grade {:.1}, above the target of {target}; simplify it \
                 before use (check_readability shows which sentences).",
                a.grade.median_grade
            ));
        }
        let format = args.format.unwrap_or_default();
        let (dir, _) = templates::output_folder(ctx, "preventive_services_handout")?;
        let doc = templates::document(
            ctx,
            &md,
            "en-US",
            "Preventive health services to talk about",
        );
        let f = templates::write_in(
            ctx,
            &dir,
            &format!("preventive_services_handout.{}", format.ext()),
            &format.render(&doc)?,
        )?;
        files_written.push(f.clone());
        handout_file = Some(f);
    }

    let person = PersonUsed {
        age_years: age,
        sex_at_birth: sex.label().into(),
        pregnancy: match pregnancy {
            Some(p) => p.label().into(),
            None => "not pregnant (assumed: not given)".into(),
        },
        risk_factors_given: risk_factors_given(rf),
    };
    let result = PreventiveResult {
        person,
        applies,
        needs_review,
        not_matched_count: not_matched,
        pregnancy_specific_not_evaluated: not_given,
        massachusetts_programs: programs,
        snapshot: SnapshotInfo {
            retrieved: snap.retrieved.clone(),
            recommendations: snap.recommendations.len(),
            statements: snap.statements.len(),
            not_included: snap.not_included.clone(),
        },
        myhealthfinder: myhf,
        handout_file,
        handout_reading_grade: handout_grade,
        scope_notes: vec![
            "USPSTF grade A and B only.".into(),
            "Inputs are age, sex at birth, pregnancy status, and risk factors; no names or identifiers.".into(),
        ],
    };
    let s = summary(&result);
    let mut notes = vec![
        "The request has only age, sex at birth, pregnancy status, and the risk factors you \
         typed; no local files were read."
            .to_string(),
    ];
    if result.myhealthfinder.is_some() {
        notes.push(
            "Age, sex, and pregnancy status (no name or other identifier) were sent to \
             odphp.health.gov for the MyHealthfinder cross-check."
                .into(),
        );
    }
    Ok(Envelope::new(s, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            row_level_data: false,
            files_written,
            notes,
            ..Privacy::none()
        })
        .decision_support())
}

fn handout_markdown(
    applies: &[RecommendationOut],
    needs_review: &[RecommendationOut],
    retrieved: &str,
) -> String {
    let mut md = String::from("# Preventive health services to talk about\n\n");
    md.push_str(
        "These are services that national experts recommend for someone your age. Talk with your \
         nurse or clinician about which ones are right for you.\n\n",
    );
    let mut last = "";
    for r in applies {
        if r.category != last {
            md.push_str(&format!("## {}\n\n", category_label(&r.category)));
            last = &r.category;
        }
        md.push_str(&format!("**{}.** {}\n\n", r.label, r.client_summary));
    }
    if !needs_review.is_empty() {
        md.push_str("## Ask whether these apply to you\n\n");
        for r in needs_review {
            md.push_str(&format!("**{}.** {}\n\n", r.label, r.client_summary));
        }
    }
    md.push_str(&format!(
        "This list comes from the A and B recommendations of the U.S. Preventive Services Task \
         Force (read on {retrieved}). It does not include vaccines or every service your \
         clinician may suggest.\n"
    ));
    md
}

fn summary(r: &PreventiveResult) -> String {
    let mut s = format!(
        "**USPSTF grade A and B preventive services** for a {}-year-old, sex at birth {}, {} \
         ({} apply, {} need more information; {} do not fit).\n\n",
        r.person.age_years,
        r.person.sex_at_birth,
        r.person.pregnancy,
        r.applies.len(),
        r.needs_review.len(),
        r.not_matched_count
    );
    let table = |recs: &[RecommendationOut]| {
        let rows: Vec<Vec<String>> = recs
            .iter()
            .map(|x| {
                vec![
                    x.grade.clone(),
                    x.label.clone(),
                    x.published.clone(),
                    if x.missing_information.is_empty() {
                        x.client_summary.clone()
                    } else {
                        format!("needs: {}", x.missing_information.join("; "))
                    },
                ]
            })
            .collect();
        md::table(
            &[
                "Grade",
                "Service",
                "USPSTF statement date",
                "For the client / needs",
            ],
            &rows,
        )
    };
    if r.applies.is_empty() {
        s.push_str("No recommendation matched on the information given.\n\n");
    } else {
        s.push_str(
            "**Applies (apply each 'confirm' note and the USPSTF population yourself):**\n\n",
        );
        s.push_str(&table(&r.applies));
        s.push('\n');
    }
    if !r.needs_review.is_empty() {
        s.push_str(
            "**Needs review (fits on age, sex, and pregnancy; a risk factor was not given):**\n\n",
        );
        s.push_str(&table(&r.needs_review));
        s.push('\n');
    }
    for p in &r.massachusetts_programs {
        s.push_str(&format!(
            "- Massachusetts: **{}** ({}): {}\n",
            p.name, p.contact, p.url
        ));
    }
    if let Some(m) = &r.myhealthfinder {
        let titles: Vec<&str> = m.topics.iter().map(|t| t.short_title.as_str()).collect();
        s.push_str(&format!(
            "\nMyHealthfinder ({}, {}) lists {} topics for this age, sex, and pregnancy: {}.\n",
            m.mode,
            m.as_of,
            titles.len(),
            titles.join("; ")
        ));
    }
    if let Some(f) = &r.handout_file {
        s.push_str(&format!("\nClient handout written to {f}.\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> (PreventiveContent, preventive::UspstfSnapshot) {
        let c: PreventiveContent = content::load(PREVENTIVE_SERVICES).unwrap();
        let (s, _) = preventive::load_snapshot(&phn_data::SnapshotStore::bundled()).unwrap();
        (c, s)
    }

    fn ids(
        age: u32,
        sex: SexAtBirth,
        preg: Option<PregnancyStatus>,
        rf: &RiskFactors,
    ) -> (Vec<String>, Vec<String>) {
        let (c, _) = defs();
        let mut a = Vec::new();
        let mut n = Vec::new();
        for d in &c.recommendations {
            match fit(d, age, sex, preg, rf) {
                Fit::Applies => a.push(d.id.clone()),
                Fit::NeedsReview(_) => n.push(d.id.clone()),
                _ => {}
            }
        }
        (a, n)
    }

    #[test]
    fn content_and_snapshot_list_the_same_recommendations() {
        let (c, s) = defs();
        let a: BTreeSet<_> = c.recommendations.iter().map(|d| &d.id).collect();
        let b: BTreeSet<_> = s.recommendations.iter().map(|r| &r.id).collect();
        assert_eq!(a, b);
        assert_eq!(a.len(), c.recommendations.len());
        for d in &c.recommendations {
            assert!(CATEGORY_ORDER.contains(&d.category.as_str()), "{}", d.id);
            for p in &d.programs {
                assert!(c.programs.iter().any(|x| &x.id == p), "{p}");
            }
            for cr in &d.criteria {
                assert!(
                    matches!(cr.sex.as_str(), "any" | "female" | "male"),
                    "{}",
                    d.id
                );
                assert!(
                    matches!(
                        cr.pregnancy.as_str(),
                        "any" | "pregnant" | "pregnant_or_postpartum" | "not_pregnant"
                    ),
                    "{}",
                    d.id
                );
                if let (Some(lo), Some(hi)) = (cr.age_min, cr.age_max) {
                    assert!(lo <= hi, "{}", d.id);
                }
            }
        }
    }

    #[test]
    fn age_criteria_in_the_content_appear_in_the_published_wording() {
        // Each numeric age bound the tool uses must be in the recommendation sentence or the
        // statement's population text, unless the content says it is an assumption.
        let (c, s) = defs();
        for d in &c.recommendations {
            let rec = s.recommendations.iter().find(|r| r.id == d.id).unwrap();
            let st = s.statement(rec).unwrap();
            let text = format!(
                "{} {} {}",
                rec.recommendation,
                st.population.clone().unwrap_or_default(),
                st.additional_text.clone().unwrap_or_default()
            );
            if d.assumption.is_some() {
                continue;
            }
            for cr in &d.criteria {
                for (bound, v) in [("age_min", cr.age_min), ("age_max", cr.age_max)] {
                    // Zero stands for newborns and infants, which the text gives in words or months.
                    let Some(v) = v.filter(|v| *v > 0) else {
                        continue;
                    };
                    // "Adults" means 18 and older.
                    if bound == "age_min" && v == 18 && text.contains("adults") {
                        continue;
                    }
                    // The text writes the bound, or one past it ("younger than 5", "24 or younger").
                    let found = [v, v + 1, v - 1].iter().any(|n| {
                        text.split(|c: char| !c.is_ascii_digit())
                            .any(|w| w == n.to_string())
                    });
                    assert!(found, "{} {bound}={v} is not in: {text}", d.id);
                }
            }
        }
    }

    #[test]
    fn colorectal_starts_at_45_and_changes_grade_at_50() {
        let rf = RiskFactors::default();
        let (a44, _) = ids(44, SexAtBirth::Male, None, &rf);
        assert!(!a44.iter().any(|i| i.starts_with("colorectal")));
        let (a45, _) = ids(45, SexAtBirth::Male, None, &rf);
        assert!(a45.contains(&"colorectal_cancer_screening_45_49".to_string()));
        let (a50, _) = ids(50, SexAtBirth::Male, None, &rf);
        assert!(a50.contains(&"colorectal_cancer_screening_50_75".to_string()));
        assert!(!a50.contains(&"colorectal_cancer_screening_45_49".to_string()));
        let (a76, _) = ids(76, SexAtBirth::Male, None, &rf);
        assert!(!a76.iter().any(|i| i.starts_with("colorectal")));
    }

    #[test]
    fn lung_screening_needs_pack_years_and_recent_smoking() {
        let mut rf = RiskFactors::default();
        let (a, n) = ids(60, SexAtBirth::Female, None, &rf);
        assert!(!a.contains(&"lung_cancer_screening".to_string()));
        assert!(n.contains(&"lung_cancer_screening".to_string()));
        rf.tobacco_use = Some(TobaccoUse::Former);
        rf.pack_years = Some(25.0);
        rf.years_since_quit = Some(10.0);
        let (a, _) = ids(60, SexAtBirth::Female, None, &rf);
        assert!(a.contains(&"lung_cancer_screening".to_string()));
        rf.years_since_quit = Some(16.0);
        let (a, n) = ids(60, SexAtBirth::Female, None, &rf);
        assert!(!a.contains(&"lung_cancer_screening".to_string()));
        assert!(!n.contains(&"lung_cancer_screening".to_string()));
        rf.years_since_quit = Some(5.0);
        rf.pack_years = Some(19.9);
        let (a, n) = ids(60, SexAtBirth::Female, None, &rf);
        assert!(!a.contains(&"lung_cancer_screening".to_string()));
        assert!(!n.contains(&"lung_cancer_screening".to_string()));
        // Age outside 50 to 80.
        rf.pack_years = Some(30.0);
        let (a, _) = ids(81, SexAtBirth::Female, None, &rf);
        assert!(!a.contains(&"lung_cancer_screening".to_string()));
    }

    #[test]
    fn chlamydia_has_two_age_alternatives() {
        let mut rf = RiskFactors {
            sexually_active: Some(true),
            ..Default::default()
        };
        let (a, _) = ids(22, SexAtBirth::Female, None, &rf);
        assert!(a.contains(&"chlamydia_screening_women".to_string()));
        let (a, n) = ids(30, SexAtBirth::Female, None, &rf);
        assert!(!a.contains(&"chlamydia_screening_women".to_string()));
        assert!(n.contains(&"chlamydia_screening_women".to_string()));
        rf.increased_risk_for = vec![RiskArea::ChlamydiaGonorrhea];
        let (a, _) = ids(30, SexAtBirth::Female, None, &rf);
        assert!(a.contains(&"chlamydia_screening_women".to_string()));
        // The USPSTF found not enough evidence for men: not listed.
        let (a, n) = ids(22, SexAtBirth::Male, None, &rf);
        assert!(!a.contains(&"chlamydia_screening_women".to_string()));
        assert!(!n.contains(&"chlamydia_screening_women".to_string()));
    }

    #[test]
    fn pregnancy_decides_the_pregnancy_recommendations() {
        let rf = RiskFactors::default();
        let (a, _) = ids(28, SexAtBirth::Female, Some(PregnancyStatus::Pregnant), &rf);
        for want in [
            "hepatitis_b_screening_pregnancy",
            "syphilis_screening_pregnancy",
            "hiv_screening_pregnancy",
            "gestational_diabetes_screening",
            "folic_acid_supplementation",
            "breastfeeding_support",
        ] {
            assert!(a.contains(&want.to_string()), "{want}");
        }
        // Not pregnant: none of them, but folic acid still applies.
        let (a, _) = ids(
            28,
            SexAtBirth::Female,
            Some(PregnancyStatus::NotPregnant),
            &rf,
        );
        assert!(!a.contains(&"syphilis_screening_pregnancy".to_string()));
        assert!(a.contains(&"folic_acid_supplementation".to_string()));
        assert!(!a.contains(&"prediabetes_diabetes_screening".to_string()));
        // Postpartum: breastfeeding support and depression prevention (if at risk) but not
        // gestational diabetes.
        let (a, _) = ids(
            28,
            SexAtBirth::Female,
            Some(PregnancyStatus::Postpartum),
            &rf,
        );
        assert!(a.contains(&"breastfeeding_support".to_string()));
        assert!(!a.contains(&"gestational_diabetes_screening".to_string()));
        // Not given: pregnancy recommendations are not evaluated.
        let (c, _) = defs();
        let d = c
            .recommendations
            .iter()
            .find(|d| d.id == "syphilis_screening_pregnancy")
            .unwrap();
        assert_eq!(
            fit(d, 28, SexAtBirth::Female, None, &rf),
            Fit::PregnancyNotGiven
        );
    }

    #[test]
    fn prediabetes_and_statin_use_published_thresholds() {
        let mut rf = RiskFactors {
            bmi: Some(24.9),
            ..Default::default()
        };
        let (a, n) = ids(50, SexAtBirth::Male, None, &rf);
        assert!(!a.contains(&"prediabetes_diabetes_screening".to_string()));
        assert!(!n.contains(&"prediabetes_diabetes_screening".to_string()));
        rf.bmi = Some(25.0);
        let (a, _) = ids(35, SexAtBirth::Male, None, &rf);
        assert!(a.contains(&"prediabetes_diabetes_screening".to_string()));
        let (a, _) = ids(71, SexAtBirth::Male, None, &rf);
        assert!(!a.contains(&"prediabetes_diabetes_screening".to_string()));
        // Statin: factor plus 10-year risk of 10% or more.
        let rf = RiskFactors {
            cardiovascular_risk_factors: Some(vec![CardiovascularFactor::Diabetes]),
            ten_year_cvd_risk_percent: Some(9.9),
            tobacco_use: Some(TobaccoUse::Never),
            ..Default::default()
        };
        let (a, n) = ids(55, SexAtBirth::Female, None, &rf);
        assert!(!a.contains(&"statin_primary_prevention".to_string()));
        assert!(!n.contains(&"statin_primary_prevention".to_string()));
        let rf = RiskFactors {
            ten_year_cvd_risk_percent: Some(10.0),
            ..rf
        };
        let (a, _) = ids(55, SexAtBirth::Female, None, &rf);
        assert!(a.contains(&"statin_primary_prevention".to_string()));
    }

    #[test]
    fn validation_errors_are_clear() {
        let ok = PreventiveArgs {
            age_years: Some(40),
            sex_at_birth: Some(SexAtBirth::Female),
            ..Default::default()
        };
        assert!(validate(&ok).is_ok());
        let mut bad = ok.clone();
        bad.age_years = None;
        assert!(
            validate(&bad)
                .unwrap_err()
                .to_string()
                .contains("age_years")
        );
        let mut bad = ok.clone();
        bad.age_years = Some(121);
        assert!(validate(&bad).is_err());
        let mut bad = ok.clone();
        bad.sex_at_birth = None;
        assert!(
            validate(&bad)
                .unwrap_err()
                .to_string()
                .contains("sex_at_birth")
        );
        let mut bad = ok.clone();
        bad.sex_at_birth = Some(SexAtBirth::Male);
        bad.pregnancy_status = Some(PregnancyStatus::Pregnant);
        assert!(validate(&bad).is_err());
        let mut bad = ok.clone();
        bad.age_years = Some(9);
        bad.pregnancy_status = Some(PregnancyStatus::Pregnant);
        assert!(validate(&bad).is_err());
        let mut bad = ok.clone();
        bad.risk_factors.bmi = Some(f64::NAN);
        assert!(validate(&bad).is_err());
        let mut bad = ok;
        bad.risk_factors.increased_risk_for = vec![RiskArea::Hiv];
        bad.risk_factors.not_at_increased_risk_for = vec![RiskArea::Hiv];
        assert!(validate(&bad).is_err());
    }
}
