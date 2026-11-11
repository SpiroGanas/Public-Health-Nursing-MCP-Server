//! Isolation, quarantine, and exclusion rules, incubation periods, and
//! infectious periods for communicable diseases (`content/cd/disease_rules.yaml`).
//!
//! The content is a pending draft; see the file's `meta.verification_note`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_core::text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContentMeta;
use crate::reportable::{self, Confidence};

/// A setting the rules distinguish.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    /// Applies in every setting.
    All,
    ChildCare,
    School,
    FoodHandler,
    HealthCareWorker,
    Other,
}

impl Setting {
    /// Plain label.
    pub fn label(self) -> &'static str {
        match self {
            Setting::All => "all settings",
            Setting::ChildCare => "child care",
            Setting::School => "school (K-12)",
            Setting::FoodHandler => "food handler",
            Setting::HealthCareWorker => "health care worker",
            Setting::Other => "other (community, work, public activities)",
        }
    }

    /// The concrete settings (everything except `All`).
    pub const CONCRETE: [Setting; 5] = [
        Setting::ChildCare,
        Setting::School,
        Setting::FoodHandler,
        Setting::HealthCareWorker,
        Setting::Other,
    ];
}

/// Who a rule is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Case,
    Contact,
}

/// The date a day count starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// The disease's onset event for the person (rash, cough, swelling, symptoms).
    Onset,
    PositiveSpecimen,
    FirstExposure,
    LastExposure,
    /// Onset of the case the contact was exposed to.
    CaseOnset,
    /// Onset of the last case in the setting.
    LastCaseOnset,
    TreatmentStart,
    SymptomResolution,
}

impl Anchor {
    /// Plain label.
    pub fn label(self) -> &'static str {
        match self {
            Anchor::Onset => "onset",
            Anchor::PositiveSpecimen => "positive specimen collection",
            Anchor::FirstExposure => "first exposure",
            Anchor::LastExposure => "last exposure",
            Anchor::CaseOnset => "the case's onset",
            Anchor::LastCaseOnset => "onset of the last case",
            Anchor::TreatmentStart => "start of treatment",
            Anchor::SymptomResolution => "symptom resolution",
        }
    }
}

/// A day relative to an anchor (day 0 = the anchor date).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DayRef {
    pub anchor: Anchor,
    pub day: i32,
}

/// Stool specimen requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StoolSpecimens {
    pub count: u32,
    #[serde(default)]
    pub hours_apart: Option<u32>,
    #[serde(default)]
    pub hours_after_antimicrobial: Option<u32>,
    #[serde(default)]
    pub after_symptom_resolution: Option<bool>,
    #[serde(default)]
    pub min_days_after_onset: Option<u32>,
}

/// When a restriction ends.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct End {
    /// Excluded through this day (return the next day).
    #[serde(default)]
    pub through: Option<DayRef>,
    /// Or after this many full days of appropriate treatment, whichever is first.
    #[serde(default)]
    pub or_treatment_days: Option<u32>,
    /// Hours after treatment starts.
    #[serde(default)]
    pub hours_after_treatment_start: Option<u32>,
    /// Hours after symptoms resolve.
    #[serde(default)]
    pub hours_after_symptom_resolution: Option<u32>,
    /// Hours after the positive specimen (the later of this and symptom resolution).
    #[serde(default)]
    pub hours_after_specimen: Option<u32>,
    #[serde(default)]
    pub stool_specimens: Option<StoolSpecimens>,
    /// The usual end when it depends on clinical criteria (an estimate only).
    #[serde(default)]
    pub typical_through: Option<DayRef>,
    /// Criteria that cannot be computed from dates.
    #[serde(default)]
    pub criteria: Vec<String>,
}

/// Facts a rule depends on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct When {
    #[serde(default)]
    pub symptomatic: Option<bool>,
    #[serde(default)]
    pub susceptible: Option<bool>,
    #[serde(default)]
    pub received_prophylaxis: Option<bool>,
    #[serde(default)]
    pub received_immune_globulin: Option<bool>,
    #[serde(default)]
    pub outbreak: Option<bool>,
    #[serde(default)]
    pub exposure_continuous: Option<bool>,
    #[serde(default)]
    pub lesions_present: Option<bool>,
    #[serde(default)]
    pub diarrhea: Option<bool>,
}

impl When {
    /// The conditions as (name, required value) pairs.
    pub fn conditions(&self) -> Vec<(&'static str, bool)> {
        let mut v = Vec::new();
        let mut add = |name, x: Option<bool>| {
            if let Some(b) = x {
                v.push((name, b));
            }
        };
        add("symptomatic", self.symptomatic);
        add("susceptible", self.susceptible);
        add("received_prophylaxis", self.received_prophylaxis);
        add("received_immune_globulin", self.received_immune_globulin);
        add("outbreak", self.outbreak);
        add("exposure_continuous", self.exposure_continuous);
        add("lesions_present", self.lesions_present);
        add("diarrhea", self.diarrhea);
        v
    }
}

/// What the rule requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Exclude from the setting (work, school, child care, public activities).
    Exclude,
    /// Restrict specific duties (food handling, direct care).
    RestrictDuties,
    /// Isolation precautions in a health care setting.
    IsolatePrecautions,
    NoRestriction,
    /// Refer to DPH or a clinician; the server does not compute dates.
    Refer,
    /// No exclusion; prophylaxis or surveillance as appropriate.
    ProphylaxisOnly,
}

impl Action {
    /// Plain label.
    pub fn label(self) -> &'static str {
        match self {
            Action::Exclude => "exclude",
            Action::RestrictDuties => "restrict duties",
            Action::IsolatePrecautions => "isolation precautions",
            Action::NoRestriction => "no restriction",
            Action::Refer => "refer (not computed)",
            Action::ProphylaxisOnly => "no exclusion; prophylaxis or surveillance",
        }
    }
}

/// One rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rule {
    pub id: String,
    pub role: Role,
    pub applies_to: String,
    pub settings: Vec<Setting>,
    #[serde(default)]
    pub when: When,
    pub action: Action,
    #[serde(default)]
    pub start: Option<DayRef>,
    pub end: End,
    /// The source's wording (verbatim when `verbatim` is true).
    pub wording: String,
    pub verbatim: bool,
    pub citation: String,
    /// Source id (see `sources`).
    pub source: String,
    pub confidence: Confidence,
    #[serde(default)]
    pub note: Option<String>,
}

impl Rule {
    /// True when the rule applies to `setting` (directly or through `All`).
    pub fn covers(&self, setting: Setting) -> bool {
        self.settings
            .iter()
            .any(|s| *s == Setting::All || *s == setting)
    }
}

/// A reference into a source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub quote: Option<String>,
}

/// Time unit of an incubation period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Hours,
    Days,
}

/// Incubation period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Incubation {
    pub unit: Unit,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub usual_low: Option<f64>,
    #[serde(default)]
    pub usual_high: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub note: Option<String>,
    pub source: SourceRef,
}

impl Incubation {
    fn to_days(&self, v: f64) -> f64 {
        match self.unit {
            Unit::Hours => v / 24.0,
            Unit::Days => v,
        }
    }
    /// Minimum in days.
    pub fn min_days(&self) -> Option<f64> {
        self.min.map(|v| self.to_days(v))
    }
    /// Maximum in days.
    pub fn max_days(&self) -> Option<f64> {
        self.max.map(|v| self.to_days(v))
    }
    /// A typical (median-like) value in days: the middle of the usual range,
    /// or of the full range when no usual range is given.
    pub fn typical_days(&self) -> Option<f64> {
        let (lo, hi) = match (self.usual_low, self.usual_high) {
            (Some(a), Some(b)) => (a, b),
            (Some(a), None) | (None, Some(a)) => (a, a),
            (None, None) => (self.min?, self.max?),
        };
        Some(self.to_days((lo + hi) / 2.0))
    }
    /// Human-readable range, e.g. "6–72 hours (usually 12–36)".
    pub fn describe(&self) -> String {
        let u = match self.unit {
            Unit::Hours => "hours",
            Unit::Days => "days",
        };
        let f = |x: f64| {
            if x.fract() == 0.0 {
                format!("{x:.0}")
            } else {
                format!("{x}")
            }
        };
        let range = match (self.min, self.max) {
            (Some(a), Some(b)) => format!("{}–{} {u}", f(a), f(b)),
            (None, Some(b)) => format!("up to {} {u}", f(b)),
            (Some(a), None) => format!("{} {u} or more", f(a)),
            (None, None) => String::new(),
        };
        let usual = match (self.usual_low, self.usual_high) {
            (Some(a), Some(b)) if a == b => format!("usually about {} {u}", f(a)),
            (Some(a), Some(b)) => format!("usually {}–{} {u}", f(a), f(b)),
            (None, Some(b)) => format!("usually under {} {u}", f(b)),
            (Some(a), None) => format!("usually about {} {u}", f(a)),
            (None, None) => String::new(),
        };
        match (range.is_empty(), usual.is_empty()) {
            (false, false) => format!("{range} ({usual})"),
            (false, true) => range,
            (true, false) => usual,
            (true, true) => "not stated".into(),
        }
    }
}

/// Infectious period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InfectiousPeriod {
    #[serde(default)]
    pub anchor: Option<Anchor>,
    #[serde(default)]
    pub start_day: Option<i32>,
    #[serde(default)]
    pub end_day: Option<i32>,
    #[serde(default)]
    pub ends_after_treatment_days: Option<u32>,
    #[serde(default)]
    pub ends_hours_after_treatment: Option<u32>,
    pub text: String,
    pub source: SourceRef,
}

/// Contact monitoring period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContactMonitoring {
    pub days_after_last_exposure: u32,
    pub text: String,
    pub source: SourceRef,
}

/// A post-exposure prophylaxis window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pep {
    pub name: String,
    pub anchor: Anchor,
    pub within_hours: u32,
    pub text: String,
    pub source: String,
    pub section: String,
}

/// One disease.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Disease {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    #[serde(default)]
    pub reportable_condition_id: Option<String>,
    /// Corpus document id of the DPH guide chapter.
    #[serde(default)]
    pub guide_document: Option<String>,
    /// What "onset" means for this disease (rash onset, cough onset, ...).
    pub onset_event: String,
    pub confidence: Confidence,
    /// False when the server refers instead of computing (tuberculosis).
    pub compute: bool,
    pub incubation: Option<Incubation>,
    pub infectious_period: Option<InfectiousPeriod>,
    #[serde(default)]
    pub contact_monitoring: Option<ContactMonitoring>,
    #[serde(default)]
    pub contact_definition: Option<String>,
    #[serde(default)]
    pub contact_testing: Option<String>,
    #[serde(default)]
    pub pep: Vec<Pep>,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A cited source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RuleSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub effective_date: Option<String>,
    pub retrieved: String,
}

/// Day-counting conventions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Conventions {
    pub day_counting: String,
    pub anchors: BTreeMap<String, String>,
    pub settings: BTreeMap<String, String>,
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiseaseRules {
    pub meta: ContentMeta,
    pub conventions: Conventions,
    pub sources: BTreeMap<String, RuleSource>,
    pub diseases: Vec<Disease>,
}

static CONTENT: LazyLock<DiseaseRules> = LazyLock::new(|| {
    let item = crate::ITEMS
        .iter()
        .find(|i| i.id == "ma-disease-rules")
        .expect("disease rules item");
    let v = crate::yaml_to_json(item.yaml).expect("disease rules YAML parses");
    serde_json::from_value(v).expect("disease rules match the Rust types")
});

/// The bundled rules.
pub fn content() -> &'static DiseaseRules {
    &CONTENT
}

/// A disease match.
#[derive(Debug, Clone, Copy)]
pub struct DiseaseMatch<'a> {
    pub disease: &'a Disease,
    pub score: f64,
}

/// Find a disease by id, name, or alias (exact, then fuzzy). Returns the best
/// matches, best first.
pub fn find(query: &str) -> Vec<DiseaseMatch<'static>> {
    let q = text::normalize(query);
    let mut out: Vec<DiseaseMatch<'static>> = Vec::new();
    for d in &content().diseases {
        let mut best: f64 = 0.0;
        let mut names = vec![d.id.replace('_', " "), d.name.clone()];
        names.extend(d.aliases.iter().cloned());
        for n in &names {
            let nn = text::normalize(n);
            let s = if nn == q {
                1.0
            } else if ["non", "not", "no"]
                .iter()
                .any(|w| reportable::contains_phrase(&nn, &format!("{w} {q}")))
            {
                // The name mentions the query only to exclude it ("non-cholera Vibrio").
                0.0
            } else {
                // A prefix counts only at a word boundary ("strep" for "strep
                // throat"), never inside a word ("GAS" in "gastroenteritis").
                let raw = if q.len() >= 3
                    && nn.len() >= 3
                    && (nn.starts_with(&format!("{q} ")) || q.starts_with(&format!("{nn} ")))
                {
                    0.9
                } else {
                    text::similarity(&q, &nn)
                };
                if reportable::short_key_in_longer_query(&q, &nn) {
                    raw.min(0.75)
                } else {
                    raw
                }
            };
            best = best.max(s);
        }
        if best > 0.0 {
            out.push(DiseaseMatch {
                disease: d,
                score: best,
            });
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// Look up a disease; `None` when nothing matches well or the match is
/// ambiguous. An exact id, name, or alias always wins. Otherwise the best
/// match must score at least 0.8, the query must not be very short or made
/// only of generic words ("gastroenteritis", "fever"), and no other disease
/// may score within [`reportable::TIE_MARGIN`] of it ("hepatitis").
pub fn lookup(query: &str) -> Option<&'static Disease> {
    let q = text::normalize(query);
    let matches = find(query);
    let top = matches.first()?;
    if top.score >= 0.999 {
        return Some(top.disease);
    }
    if top.score < 0.8 || q.len() < 4 || reportable::is_generic_query(&q) {
        return None;
    }
    if matches
        .get(1)
        .is_some_and(|m| m.score >= top.score - reportable::TIE_MARGIN)
    {
        return None;
    }
    Some(top.disease)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_is_consistent() {
        let c = content();
        assert!(c.diseases.len() >= 30, "{}", c.diseases.len());
        let mut ids = std::collections::BTreeSet::new();
        for d in &c.diseases {
            assert!(ids.insert(d.id.clone()), "duplicate {}", d.id);
            let mut rule_ids = std::collections::BTreeSet::new();
            for r in &d.rules {
                assert!(rule_ids.insert(&r.id), "{}: duplicate rule {}", d.id, r.id);
                assert!(
                    c.sources.contains_key(&r.source),
                    "{}/{}: unknown source {}",
                    d.id,
                    r.id,
                    r.source
                );
            }
            if let Some(i) = &d.incubation {
                assert!(c.sources.contains_key(&i.source.source), "{}", d.id);
                if let (Some(a), Some(b)) = (i.min, i.max) {
                    assert!(a <= b, "{}", d.id);
                }
            }
            for p in &d.pep {
                assert!(c.sources.contains_key(&p.source), "{}", d.id);
            }
            if d.compute {
                assert!(
                    d.rules.iter().any(|r| r.role == Role::Case),
                    "{} has no case rule",
                    d.id
                );
            }
        }
    }

    #[test]
    fn finds_diseases_by_alias() {
        assert_eq!(lookup("whooping cough").unwrap().id, "pertussis");
        assert_eq!(lookup("chickenpox").unwrap().id, "varicella");
        assert_eq!(lookup("E. coli O157").unwrap().id, "stec");
        assert_eq!(lookup("Salmonella").unwrap().id, "salmonellosis");
        assert_eq!(lookup("norovirus").unwrap().id, "norovirus");
        assert!(lookup("zzzz unknown").is_none());
    }

    #[test]
    fn ambiguous_generic_and_partial_queries_do_not_match() {
        let id = |q: &str| lookup(q).map(|d| d.id.as_str());
        // "GAS" is not a prefix of "gastroenteritis"; a syndrome is not one disease.
        assert_eq!(id("gastroenteritis"), None);
        // "flu" inside a longer phrase is not seasonal influenza.
        assert_eq!(id("h flu"), None);
        assert_ne!(id("stomach flu"), Some("influenza"));
        // "non-cholera Vibrio" does not make cholera vibriosis.
        assert_eq!(id("cholera"), None);
        // Ties and near-ties between diseases.
        for q in ["hepatitis", "hep", "hepatitis E"] {
            assert_eq!(id(q), None, "{q}");
        }
        // Generic words and very short queries.
        for q in ["meningitis", "fever", "tick bite", "h", "s"] {
            assert_eq!(id(q), None, "{q}");
        }
        // The candidates are still ranked for "did you mean".
        assert!(find("hepatitis").len() >= 3);
        // Exact names and legitimate fuzzy matches still work.
        assert_eq!(id("hepatitis B"), Some("hepatitis_b"));
        assert_eq!(id("flu"), Some("influenza"));
        assert_eq!(id("Hib"), Some("haemophilus_influenzae_type_b"));
        assert_eq!(id("viral gastroenteritis"), Some("norovirus"));
        assert_eq!(id("strep"), Some("group_a_streptococcus"));
        assert_eq!(id("e coli"), Some("stec"));
        assert_eq!(id("pertusis"), Some("pertussis"));
        assert_eq!(id("measels"), Some("measles"));
        assert_eq!(id("salmonela"), Some("salmonellosis"));
        for d in &content().diseases {
            for key in std::iter::once(&d.name).chain(&d.aliases) {
                assert_eq!(id(key), Some(d.id.as_str()), "{key}");
            }
        }
    }

    #[test]
    fn incubation_helpers() {
        let s = lookup("salmonellosis")
            .unwrap()
            .incubation
            .as_ref()
            .unwrap();
        assert_eq!(s.max_days(), Some(3.0));
        assert_eq!(s.typical_days(), Some(1.0));
        assert!(s.describe().contains("6–72 hours"));
    }
}
