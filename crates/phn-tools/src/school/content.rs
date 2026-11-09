//! Typed access to the school health reference content in `content/school/`.
//!
//! Each file is embedded through `phn_content::ITEMS` (so it is also an MCP
//! resource and is checked by `cargo xtask validate-content`) and parsed once
//! into the types below. Every statement carries a source key and section so
//! tools can cite it.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use crate::vax::content::Confidence;
use crate::vax::content::review_caveats;

/// Content ids.
pub const SCREENING: &str = "ma-school-screening-requirements";
pub const BMI: &str = "cdc-bmi-for-age";
pub const MEDICATION: &str = "ma-school-medication-administration";
pub const CONCUSSION: &str = "ma-school-concussion";
pub const ABSENTEEISM: &str = "ma-school-absenteeism";
pub const HEALTH_PLANS: &str = "ma-school-health-plans";
pub const ANNUAL_REPORT: &str = "ma-school-annual-report";
pub const CHILD_CARE: &str = "ma-child-care-health-checklist";

fn load<T: DeserializeOwned>(id: &str) -> Result<T> {
    let item = phn_content::ITEMS
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PhnError::Internal(format!("content item {id} is not registered")))?;
    let v = phn_content::yaml_to_json(item.yaml).map_err(PhnError::Internal)?;
    serde_json::from_value(v).map_err(|e| PhnError::Internal(format!("{}: {e}", item.path)))
}

/// A source listed in a content file.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceRef {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub effective_date: Option<String>,
    pub retrieved: String,
}

/// Sources of one content file, by key.
pub type Sources = BTreeMap<String, SourceRef>;

/// A statement with its citation.
#[derive(Debug, Clone, Deserialize)]
pub struct Cited {
    pub text: String,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

/// A statement as returned by a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedOut {
    pub text: String,
    /// Source title and section.
    pub citation: String,
    pub url: Option<String>,
    /// Set when the drafter could not fully verify the statement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// "Title, section" for a source key.
pub fn citation(sources: &Sources, key: &str, section: &str) -> String {
    match sources.get(key) {
        Some(s) => format!("{}, {section}", s.title),
        None => format!("{key}, {section}"),
    }
}

impl Cited {
    /// Resolve the citation against the file's sources.
    pub fn out(&self, sources: &Sources) -> CitedOut {
        CitedOut {
            text: self.text.trim().to_string(),
            citation: citation(sources, &self.source, &self.section),
            url: sources.get(&self.source).map(|s| s.url.clone()),
            confidence: self.confidence.filter(|c| !matches!(c, Confidence::High)),
        }
    }
}

/// Resolve a list of cited statements.
pub fn outs(v: &[Cited], sources: &Sources) -> Vec<CitedOut> {
    v.iter().map(|c| c.out(sources)).collect()
}

/// Citations for the given source keys (deduplicated, in order).
pub fn sources_for<'a>(sources: &Sources, keys: impl IntoIterator<Item = &'a str>) -> Vec<Source> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for k in keys {
        if seen.contains(&k) {
            continue;
        }
        seen.push(k);
        if let Some(s) = sources.get(k) {
            out.push(Source {
                title: s.title.clone(),
                publisher: s.publisher.clone(),
                url: Some(s.url.clone()),
                effective_date: s.effective_date.clone(),
                retrieved: Some(s.retrieved.clone()),
                mode: SourceMode::Snapshot,
            });
        }
    }
    out
}

/// The review caveat for content items (empty once reviewed).
pub fn review(ids: &[&str]) -> Vec<String> {
    review_caveats(ids)
}

/// A low- or medium-confidence note for markdown, e.g. " (medium confidence)".
pub fn confidence_suffix(c: Option<Confidence>) -> &'static str {
    match c {
        Some(Confidence::Low) => " _(LOW confidence: verify)_",
        Some(Confidence::Medium) => " _(medium confidence)_",
        _ => "",
    }
}

// ----------------------------------------------------------------- SCH-01/02 screenings

/// How often a screening is required within a grade band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    /// Every year in each listed grade.
    Annual,
    /// Once at some point while the student is in the band.
    OnceInBand,
    /// At school entry (for example the kindergarten vision certificate).
    SchoolEntry,
    /// Grades chosen by the district (SBIRT).
    DistrictSelected,
    /// At entry and then every 3 or 4 years (physical examinations).
    EntryAndInterval,
    /// Triggered by an event (employment certificate, athletics).
    Event,
}

impl Frequency {
    /// Plain label.
    pub fn label(self) -> &'static str {
        match self {
            Frequency::Annual => "every year",
            Frequency::OnceInBand => "once in the grade band",
            Frequency::SchoolEntry => "at school entry",
            Frequency::DistrictSelected => "grades selected by the district",
            Frequency::EntryAndInterval => "at entry, then every 3 or 4 years",
            Frequency::Event => "when the event occurs",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegalBasis {
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleBand {
    pub band: String,
    pub grades: Vec<String>,
    pub frequency: Frequency,
    #[serde(default)]
    pub ages: Option<[u32; 2]>,
    pub text: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Screening {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub trackable: bool,
    pub referral_letters: bool,
    pub legal_basis: Vec<LegalBasis>,
    pub schedule: Vec<ScheduleBand>,
    #[serde(default)]
    pub new_students: Option<Cited>,
    pub who: Vec<Cited>,
    pub protocol: Vec<Cited>,
    pub referral: Vec<Cited>,
    pub parent_notification: Vec<Cited>,
    pub exemptions: Vec<Cited>,
    pub reporting: Vec<Cited>,
    pub confidentiality: Vec<Cited>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreeningContent {
    pub sources: Sources,
    pub general: Vec<Cited>,
    pub screenings: Vec<Screening>,
}

impl ScreeningContent {
    /// A screening by id.
    pub fn get(&self, id: &str) -> Option<&Screening> {
        self.screenings.iter().find(|s| s.id == id)
    }
}

pub static SCREENINGS: LazyLock<ScreeningContent> =
    LazyLock::new(|| load(SCREENING).expect("school screening content parses"));

// ----------------------------------------------------------------- BMI reference

#[derive(Debug, Clone, Deserialize)]
pub struct BmiCategory {
    pub id: String,
    pub label: String,
    pub rule: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BmiReference {
    pub sources: Sources,
    /// [sex, agemos, L, M, S, P5, P85, P95]
    pub rows: Vec<[f64; 8]>,
    pub categories: Vec<BmiCategory>,
    pub notes: Vec<Cited>,
}

pub static BMI_REF: LazyLock<BmiReference> =
    LazyLock::new(|| load(BMI).expect("CDC BMI-for-age content parses"));

// ----------------------------------------------------------------- SCH-03 medication

#[derive(Debug, Clone, Deserialize)]
pub struct MedTopic {
    pub id: String,
    pub title: String,
    pub keywords: Vec<String>,
    pub summary: String,
    pub points: Vec<Cited>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MedicationContent {
    pub sources: Sources,
    pub topics: Vec<MedTopic>,
}

pub static MEDS: LazyLock<MedicationContent> =
    LazyLock::new(|| load(MEDICATION).expect("school medication content parses"));

// ----------------------------------------------------------------- SCH-05 concussion

#[derive(Debug, Clone, Deserialize)]
pub struct ConcussionForm {
    pub id: String,
    pub name: String,
    pub who_completes: String,
    pub when: String,
    pub url: String,
    #[serde(default)]
    pub translations: Vec<String>,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Notification {
    pub who: String,
    pub when: String,
    pub how: String,
    #[serde(default)]
    pub deadline_business_days: Option<u32>,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stage {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub min_days: Option<u32>,
    #[serde(default)]
    pub max_days: Option<u32>,
    pub text: String,
    #[serde(default)]
    pub activities: Vec<String>,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Accommodation {
    pub effect: String,
    pub problem: String,
    pub supports: Vec<String>,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SymptomMonitoring {
    pub symptoms: Vec<String>,
    pub scale: String,
    pub frequency: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConcussionContent {
    pub sources: Sources,
    pub scope: Vec<Cited>,
    pub forms: Vec<ConcussionForm>,
    pub training: Vec<Cited>,
    pub removal_from_play: Vec<Cited>,
    pub notification: Vec<Notification>,
    pub reentry_plan: Vec<Cited>,
    pub clearance: Vec<Cited>,
    pub return_to_learn: Vec<Stage>,
    pub return_to_play_dph: Vec<Stage>,
    pub return_to_play_cdc: Vec<Stage>,
    pub accommodations: Vec<Accommodation>,
    pub danger_signs: Vec<Cited>,
    pub symptom_monitoring: SymptomMonitoring,
    pub persistent_symptoms: Vec<Cited>,
    pub nurse_responsibilities: Vec<Cited>,
    pub records_and_reporting: Vec<Cited>,
    pub divergences: Vec<Cited>,
}

pub static CONCUSSION_INFO: LazyLock<ConcussionContent> =
    LazyLock::new(|| load(CONCUSSION).expect("school concussion content parses"));

// ----------------------------------------------------------------- SCH-04 health plans

#[derive(Debug, Clone, Deserialize)]
pub struct Condition {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: Cited,
    pub recognition: Vec<Cited>,
    pub emergency_steps: Vec<Cited>,
    pub call_911: Vec<Cited>,
    pub assessment: Vec<Cited>,
    pub goals: Vec<Cited>,
    pub interventions: Vec<Cited>,
    pub accommodations: Vec<Cited>,
    pub medications: Vec<Cited>,
    pub plan_elements: Vec<Cited>,
    pub family_resources: Vec<Cited>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthPlanContent {
    pub sources: Sources,
    pub process: Vec<Cited>,
    pub legal: Vec<Cited>,
    pub conditions: Vec<Condition>,
}

pub static PLANS: LazyLock<HealthPlanContent> =
    LazyLock::new(|| load(HEALTH_PLANS).expect("school health plan content parses"));

// ----------------------------------------------------------------- SCH-06 absenteeism

#[derive(Debug, Clone, Deserialize)]
pub struct Syndrome {
    pub id: String,
    pub name: String,
    pub definition: String,
    pub keywords: Vec<String>,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControlMeasure {
    pub syndrome: String,
    pub text: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AbsenteeismContent {
    pub sources: Sources,
    pub syndromes: Vec<Syndrome>,
    pub notification: Vec<Cited>,
    pub examples: Vec<Cited>,
    pub control_measures: Vec<ControlMeasure>,
    pub method: Vec<Cited>,
}

pub static ABSENCE: LazyLock<AbsenteeismContent> =
    LazyLock::new(|| load(ABSENTEEISM).expect("school absenteeism content parses"));

// ----------------------------------------------------------------- SCH-07 annual report

#[derive(Debug, Clone, Deserialize)]
pub struct DphReport {
    pub id: String,
    pub name: String,
    pub who_submits: String,
    pub due: String,
    pub content: Vec<String>,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QualityRange {
    pub grade: String,
    pub height_in: [f64; 2],
    pub weight_lb: [f64; 2],
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnualReportContent {
    pub sources: Sources,
    pub reports: Vec<DphReport>,
    pub definitions: Vec<Cited>,
    pub format_note: Cited,
    pub bmi_quality_ranges: Vec<QualityRange>,
    pub statewide_reference: Vec<Cited>,
}

pub static REPORTING: LazyLock<AnnualReportContent> =
    LazyLock::new(|| load(ANNUAL_REPORT).expect("school annual report content parses"));

// ----------------------------------------------------------------- SCH-08 child care

#[derive(Debug, Clone, Deserialize)]
pub struct Domain {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub domain: String,
    pub requirement: String,
    pub evidence: Vec<String>,
    pub applies_to: Vec<String>,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChildCareContent {
    pub sources: Sources,
    pub domains: Vec<Domain>,
    pub items: Vec<ChecklistItem>,
    pub consultant_role: Vec<Cited>,
    pub related: Vec<Cited>,
}

pub static CHILD_CARE_INFO: LazyLock<ChildCareContent> =
    LazyLock::new(|| load(CHILD_CARE).expect("child care checklist content parses"));

#[cfg(test)]
mod tests {
    use super::*;

    fn check_sources(name: &str, sources: &Sources, keys: &[&str]) {
        for k in keys {
            assert!(sources.contains_key(*k), "{name}: unknown source {k}");
        }
    }

    #[test]
    fn every_file_loads_and_cites_known_sources() {
        let s = &*SCREENINGS;
        assert!(s.get("vision").is_some() && s.get("hearing").is_some());
        for sc in &s.screenings {
            let mut keys: Vec<&str> = sc.schedule.iter().map(|b| b.source.as_str()).collect();
            keys.extend(sc.legal_basis.iter().map(|b| b.source.as_str()));
            for list in [
                &sc.who,
                &sc.protocol,
                &sc.referral,
                &sc.parent_notification,
                &sc.exemptions,
                &sc.reporting,
                &sc.confidentiality,
            ] {
                keys.extend(list.iter().map(|c| c.source.as_str()));
            }
            check_sources(&sc.id, &s.sources, &keys);
        }
        assert!(BMI_REF.rows.len() > 400);
        let m = &*MEDS;
        for t in &m.topics {
            let keys: Vec<&str> = t.points.iter().map(|c| c.source.as_str()).collect();
            check_sources(&t.id, &m.sources, &keys);
        }
        let c = &*CONCUSSION_INFO;
        assert_eq!(c.return_to_play_dph.len(), 6);
        assert_eq!(c.symptom_monitoring.symptoms.len(), 22);
        let p = &*PLANS;
        for cond in &p.conditions {
            let mut keys = vec![cond.summary.source.as_str()];
            for list in [
                &cond.recognition,
                &cond.emergency_steps,
                &cond.call_911,
                &cond.goals,
                &cond.interventions,
                &cond.accommodations,
                &cond.plan_elements,
            ] {
                keys.extend(list.iter().map(|c| c.source.as_str()));
            }
            check_sources(&cond.id, &p.sources, &keys);
        }
        let a = &*ABSENCE;
        assert!(a.syndromes.iter().any(|s| s.id == "gi"));
        let r = &*REPORTING;
        assert!(r.reports.iter().any(|x| x.id == "bmi_report"));
        let cc = &*CHILD_CARE_INFO;
        for i in &cc.items {
            assert!(cc.domains.iter().any(|d| d.id == i.domain), "{}", i.id);
            check_sources(&i.id, &cc.sources, &[i.source.as_str()]);
        }
    }
}
