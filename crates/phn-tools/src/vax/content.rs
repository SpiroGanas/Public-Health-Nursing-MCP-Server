//! Typed access to the immunization reference content in `content/vax/`.
//!
//! Each file is embedded through `phn_content::ITEMS` (so it is also an MCP
//! resource and is checked by `cargo xtask validate-content`) and parsed once
//! into the types below.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::{ContentMeta, ReviewStatus};
use phn_core::{PhnError, Result, Source, SourceMode, VaccineRecommendationBasis};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;

/// Content ids.
pub const SCHEDULE: &str = "vax-schedule";
pub const RECOMMENDATIONS: &str = "vax-recommendations";
pub const SCHOOL: &str = "ma-school-immunization-requirements";
pub const STORAGE: &str = "vax-storage-handling";
pub const PRODUCTS: &str = "vax-products";
pub const CLINIC: &str = "vax-clinic-planning";
pub const CONFIDENCE: &str = "vax-confidence-guide";
pub const THRESHOLDS: &str = "vax-coverage-thresholds";

fn load<T: DeserializeOwned>(id: &str) -> Result<T> {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PhnError::Internal(format!("content item {id} is not registered")))?;
    let v = phn_content::yaml_to_json(item.yaml).map_err(PhnError::Internal)?;
    serde_json::from_value(v).map_err(|e| PhnError::Internal(format!("{}: {e}", item.path)))
}

/// The meta block of a content item.
pub fn meta(id: &str) -> Result<ContentMeta> {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PhnError::Internal(format!("content item {id} is not registered")))?;
    phn_content::item_meta(item).map_err(PhnError::Internal)
}

/// A citation for bundled content (mode `snapshot`).
pub fn content_source(id: &str) -> Result<Source> {
    let m = meta(id)?;
    Ok(Source {
        title: m.source.title.clone(),
        publisher: m.source.publisher.clone(),
        url: m.source.url.clone(),
        effective_date: m.effective_date.clone(),
        retrieved: Some(m.prepared.clone()),
        mode: SourceMode::Snapshot,
    })
}

/// The review caveat for a content item (empty once reviewed).
pub fn review_caveats(ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| meta(id).ok())
        .filter(|m| m.review.status == ReviewStatus::Pending)
        .filter_map(|m| m.review_caveat())
        .collect()
}

/// A source cited inside content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
}

/// Developer confidence in a drafted entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// An issuing body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    /// Massachusetts DPH.
    Dph,
    /// CDC / ACIP (federal).
    CdcAcip,
    /// American Academy of Pediatrics.
    Aap,
    /// American College of Obstetricians and Gynecologists.
    Acog,
    /// American Academy of Family Physicians.
    Aafp,
    /// Northeast Public Health Collaborative.
    Nephc,
}

impl Body {
    /// Short label.
    pub fn short(self) -> &'static str {
        match self {
            Body::Dph => "DPH",
            Body::CdcAcip => "CDC/ACIP",
            Body::Aap => "AAP",
            Body::Acog => "ACOG",
            Body::Aafp => "AAFP",
            Body::Nephc => "NEPHC",
        }
    }
}

/// Population a recommendation applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Population {
    Infants,
    Children,
    ChildrenAdolescents,
    Adolescents,
    Adults,
    Pregnancy,
    All,
}

impl Population {
    /// The professional society that speaks for this population.
    pub fn society(self) -> Body {
        match self {
            Population::Pregnancy => Body::Acog,
            Population::Adults => Body::Aafp,
            _ => Body::Aap,
        }
    }

    /// Label.
    pub fn label(self) -> &'static str {
        match self {
            Population::Infants => "infants",
            Population::Children => "children",
            Population::ChildrenAdolescents => "children and adolescents",
            Population::Adolescents => "adolescents",
            Population::Adults => "adults",
            Population::Pregnancy => "pregnancy",
            Population::All => "all ages",
        }
    }
}

/// Order in which bodies are consulted for the "basis" recommendation.
pub fn basis_order(basis: VaccineRecommendationBasis, population: Population) -> Vec<Body> {
    let society = population.society();
    let mut societies = vec![society];
    for b in [Body::Aap, Body::Acog, Body::Aafp] {
        if !societies.contains(&b) {
            societies.push(b);
        }
    }
    match basis {
        VaccineRecommendationBasis::DphThenProfessionalThenCdc | VaccineRecommendationBasis::ShowAll => {
            let mut v = vec![Body::Dph];
            v.extend(societies);
            v.push(Body::CdcAcip);
            v
        }
        VaccineRecommendationBasis::ProfessionalSocieties => {
            let mut v = societies;
            v.push(Body::Dph);
            v.push(Body::CdcAcip);
            v
        }
        VaccineRecommendationBasis::CdcAcip => {
            let mut v = vec![Body::CdcAcip, Body::Dph];
            v.extend(societies);
            v
        }
    }
}

// ----------------------------------------------------------------- recommendations

#[derive(Debug, Clone, Deserialize)]
pub struct BodyInfo {
    pub name: String,
    pub short: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    pub body: Body,
    pub summary: String,
    pub date: Option<String>,
    pub source: CitedSource,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Topic {
    pub id: String,
    pub title: String,
    pub vaccine: String,
    pub population: Population,
    pub aliases: Vec<String>,
    pub positions: Vec<Position>,
    pub differs: bool,
    pub difference: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recommendations {
    pub bodies: BTreeMap<String, BodyInfo>,
    pub context: Vec<String>,
    pub topics: Vec<Topic>,
}

pub static RECS: LazyLock<Recommendations> =
    LazyLock::new(|| load(RECOMMENDATIONS).expect("vax-recommendations parses"));

// ----------------------------------------------------------------- schedule

#[derive(Debug, Clone, Deserialize)]
pub struct Edition {
    pub id: String,
    pub body: Body,
    pub population: String,
    pub title: String,
    pub date: Option<String>,
    pub url: String,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SeriesDose {
    pub dose: u32,
    pub min_age: String,
    #[serde(default)]
    pub min_interval: Option<String>,
    #[serde(default)]
    pub min_interval_from_first: Option<String>,
    #[serde(default)]
    pub plan_min_age: Option<String>,
    #[serde(default)]
    pub plan_min_interval: Option<String>,
    #[serde(default)]
    pub no_grace_interval: bool,
    #[serde(default)]
    pub recommended_age: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Series {
    pub id: String,
    pub label: String,
    pub doses: Vec<SeriesDose>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Routine {
    pub body: Body,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VaccineEntry {
    pub id: String,
    pub name: String,
    pub abbreviations: Vec<String>,
    pub aliases: Vec<String>,
    pub series: Vec<String>,
    pub routine: Vec<Routine>,
    pub contraindications: Vec<String>,
    pub precautions: Vec<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Schedule {
    pub editions: Vec<Edition>,
    pub general_rules: Vec<Rule>,
    pub series: Vec<Series>,
    pub vaccines: Vec<VaccineEntry>,
}

impl Schedule {
    /// A series by id.
    pub fn series(&self, id: &str) -> Option<&Series> {
        self.series.iter().find(|s| s.id == id)
    }
}

pub static SCHED: LazyLock<Schedule> =
    LazyLock::new(|| load(SCHEDULE).expect("vax-schedule parses"));

// ----------------------------------------------------------------- school

#[derive(Debug, Clone, Deserialize)]
pub struct RuleOption {
    pub min_doses: usize,
    #[serde(default)]
    pub last_dose_min_age: Option<String>,
    #[serde(default)]
    pub last_dose_min_interval: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Special {
    Hib,
    Tdap,
    #[serde(rename = "men_7_10")]
    Men710,
    #[serde(rename = "men_11_12")]
    Men1112,
    MenCollege,
    ResidentialMeningococcal,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReqRule {
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub special: Option<Special>,
    #[serde(default)]
    pub max_age_years: Option<u32>,
    #[serde(default)]
    pub options: Vec<RuleOption>,
    #[serde(default)]
    pub lab_evidence: Vec<String>,
    #[serde(default)]
    pub history: bool,
    #[serde(default)]
    pub heplisav_two_dose: bool,
    #[serde(default)]
    pub us_born_before_year: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Requirement {
    pub id: String,
    pub vaccine: String,
    pub label: String,
    #[serde(default)]
    pub grades: Vec<String>,
    pub text: String,
    pub rule: ReqRule,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchoolSetting {
    pub id: String,
    pub label: String,
    pub grades: Vec<String>,
    pub applies: String,
    #[serde(default)]
    pub min_age_years: Option<u32>,
    pub requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProofOfImmunity {
    pub id: String,
    pub text: String,
    pub applies_to: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExemptionText {
    pub text: String,
    pub renewal: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WaiverText {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Exemptions {
    pub medical: ExemptionText,
    pub religious: ExemptionText,
    pub meningococcal_waiver: WaiverText,
    pub outbreak_exclusion: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GracePeriod {
    pub default_applies: bool,
    pub text: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchoolRequirements {
    pub school_year: String,
    pub general: Vec<Rule>,
    pub proof_of_immunity: Vec<ProofOfImmunity>,
    pub exemptions: Exemptions,
    pub enrollment_rules: Vec<Rule>,
    pub grace_period: GracePeriod,
    pub flags: Vec<Rule>,
    pub settings: Vec<SchoolSetting>,
}

pub static SCHOOL_REQS: LazyLock<SchoolRequirements> =
    LazyLock::new(|| load(SCHOOL).expect("ma-school-immunization-requirements parses"));

// ----------------------------------------------------------------- storage

#[derive(Debug, Clone, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub alternate_email: String,
    pub fax: String,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OperatingRange {
    pub unit: String,
    pub range_c: Vec<Option<f64>>,
    pub range_f: Vec<Option<f64>>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdKind {
    AtOrAboveC,
    AtOrBelowC,
    BelowC,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Threshold {
    pub kind: ThresholdKind,
    pub value: f64,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cumulative {
    pub minutes: u32,
    pub window_hours: u32,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportingThreshold {
    pub unit: String,
    pub immediate: Vec<Threshold>,
    pub cumulative: Option<Cumulative>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Definition {
    pub term: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub order: u32,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Storage {
    pub contacts: Vec<Contact>,
    pub operating_ranges: Vec<OperatingRange>,
    pub reporting_thresholds: Vec<ReportingThreshold>,
    pub definitions: Vec<Definition>,
    pub cdc_steps: Vec<Step>,
    pub dph_steps: Vec<Step>,
    pub power_failure: String,
    pub frozen_storage_note: String,
    pub viability_statement: String,
    pub worksheet_fields: Vec<String>,
}

pub static STORE: LazyLock<Storage> =
    LazyLock::new(|| load(STORAGE).expect("vax-storage-handling parses"));

// ----------------------------------------------------------------- products

#[derive(Debug, Clone, Deserialize)]
pub struct BeyondUse {
    pub definition: String,
    pub calculation: Vec<String>,
    pub examples: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProductInfo {
    pub trade_name: String,
    pub manufacturer: String,
    pub vaccine: String,
    pub licensed_ages: String,
    pub storage: String,
    pub ages_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VisEdition {
    pub vaccine: String,
    pub title: String,
    pub date: Option<String>,
    pub interim: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VisInfo {
    pub page_date: Option<String>,
    pub translations_url: String,
    pub translation_note: String,
    pub editions: Vec<VisEdition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VisKeywords {
    pub vaccine: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Products {
    pub beyond_use: BeyondUse,
    pub storage_classes: BTreeMap<String, String>,
    pub products: Vec<ProductInfo>,
    pub vis: VisInfo,
    pub vis_keywords: Vec<VisKeywords>,
}

pub static PRODUCT_INFO: LazyLock<Products> =
    LazyLock::new(|| load(PRODUCTS).expect("vax-products parses"));

// ----------------------------------------------------------------- clinic

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// The tool's own planning default (no published source).
    ToolDefault,
    /// Taken from a cited source.
    Source,
    /// Calculated from other inputs.
    Computed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Assumption {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub basis: Basis,
    #[serde(default)]
    pub source: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Station {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Kit {
    pub first_line: Vec<String>,
    pub optional: Vec<String>,
    pub supplies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DoseBand {
    pub age_group: String,
    pub weight_lb: String,
    pub min_kg: f64,
    pub max_kg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution_dose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoinjector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dose: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImDoseRule {
    pub children: String,
    pub adults: String,
    pub repeat: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Epinephrine {
    pub im_dose_rule: ImDoseRule,
    pub weight_preferred_note: String,
    pub bands: Vec<DoseBand>,
    pub minimum_solution_dose_ml: f64,
    pub intranasal: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Diphenhydramine {
    pub rule: String,
    pub max_single_dose_note: String,
    pub bands: Vec<DoseBand>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SupplyItem {
    pub item: String,
    pub per_dose: f64,
    pub basis: Basis,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClinicPlanning {
    pub assumptions: Vec<Assumption>,
    pub stations: Vec<Station>,
    pub layout_guidance: Vec<String>,
    pub layout_basis: Basis,
    pub anaphylaxis_kit: Kit,
    pub anaphylaxis_protocol: Vec<Step>,
    pub epinephrine: Epinephrine,
    pub diphenhydramine: Diphenhydramine,
    pub supplies_per_dose: Vec<SupplyItem>,
    pub sharps_container_doses: u32,
    pub sharps_container_basis: Basis,
    pub reconciliation_columns: Vec<String>,
}

impl ClinicPlanning {
    /// An assumption by id.
    pub fn assumption(&self, id: &str) -> Option<&Assumption> {
        self.assumptions.iter().find(|a| a.id == id)
    }
}

pub static CLINIC_INFO: LazyLock<ClinicPlanning> =
    LazyLock::new(|| load(CLINIC).expect("vax-clinic-planning parses"));

// ----------------------------------------------------------------- confidence

#[derive(Debug, Clone, Deserialize)]
pub struct GuideSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Fact {
    pub text: String,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Concern {
    pub id: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub open_question: String,
    pub reflection: String,
    pub permission: String,
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub divergence: Option<String>,
    pub next_step: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranslatedResource {
    pub language: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfidenceGuide {
    pub sources: BTreeMap<String, GuideSource>,
    pub mi_principles: Vec<String>,
    pub translated_resources: Vec<TranslatedResource>,
    pub concerns: Vec<Concern>,
}

pub static GUIDE: LazyLock<ConfidenceGuide> =
    LazyLock::new(|| load(CONFIDENCE).expect("vax-confidence-guide parses"));

// ----------------------------------------------------------------- thresholds

#[derive(Debug, Clone, Deserialize)]
pub struct ThresholdEntry {
    pub id: String,
    pub disease: String,
    pub vaccines: Vec<String>,
    pub r0: Option<String>,
    pub threshold_pct_low: Option<f64>,
    pub threshold_pct_high: Option<f64>,
    pub flag_default_pct: f64,
    pub flag_basis: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Thresholds {
    pub thresholds: Vec<ThresholdEntry>,
    pub measure_map: BTreeMap<String, Vec<String>>,
    pub dph_small_school_rule: String,
}

pub static THRESH: LazyLock<Thresholds> =
    LazyLock::new(|| load(THRESHOLDS).expect("vax-coverage-thresholds parses"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vax_content_item_parses_into_its_types() {
        assert!(!RECS.topics.is_empty());
        assert!(SCHED.series("dtap").is_some());
        assert_eq!(SCHOOL_REQS.settings.len(), 4);
        assert!(!STORE.cdc_steps.is_empty());
        assert!(!PRODUCT_INFO.vis.editions.is_empty());
        assert!(CLINIC_INFO.assumption("observation_minutes").is_some());
        assert!(!GUIDE.concerns.is_empty());
        assert!(!THRESH.thresholds.is_empty());
        // Every fact cites a known source.
        for c in &GUIDE.concerns {
            for f in &c.facts {
                assert!(GUIDE.sources.contains_key(&f.source), "{} cites {}", c.id, f.source);
            }
        }
        // Every series named by a vaccine or requirement exists.
        for v in &SCHED.vaccines {
            for s in &v.series {
                assert!(SCHED.series(s).is_some(), "{} names unknown series {s}", v.id);
            }
        }
        for st in &SCHOOL_REQS.settings {
            for r in &st.requirements {
                if let Some(s) = &r.rule.series {
                    assert!(SCHED.series(s).is_some(), "{} names unknown series {s}", r.id);
                }
                assert!(r.rule.series.is_some() || r.rule.special.is_some(), "{}", r.id);
            }
        }
        // Every duration string parses.
        for s in &SCHED.series {
            for d in &s.doses {
                for v in [Some(&d.min_age), d.min_interval.as_ref(), d.plan_min_age.as_ref(), d.plan_min_interval.as_ref(), d.min_interval_from_first.as_ref()].into_iter().flatten() {
                    assert!(crate::vax::dates::Span::parse(v).is_some(), "{}: {v}", s.id);
                }
            }
        }
    }

    #[test]
    fn basis_order_follows_the_setting() {
        use VaccineRecommendationBasis as B;
        assert_eq!(basis_order(B::DphThenProfessionalThenCdc, Population::Pregnancy)[..2], [Body::Dph, Body::Acog]);
        assert_eq!(basis_order(B::CdcAcip, Population::Children)[0], Body::CdcAcip);
        assert_eq!(basis_order(B::ProfessionalSocieties, Population::Adults)[0], Body::Aafp);
        assert_eq!(*basis_order(B::DphThenProfessionalThenCdc, Population::Infants).last().unwrap(), Body::CdcAcip);
    }
}
