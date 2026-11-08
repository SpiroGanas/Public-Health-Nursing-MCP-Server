//! Typed access to `content/cd/tuberculosis.yaml` (CD-11 and CD-12).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::practice::{CitedText, Confidence, ContentSource};

/// Content id.
pub const TB_CONTENT: &str = "tb-investigation-and-ltbi";

/// Index-case type that selects the contact-priority tree (2005 Figures 2-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseType {
    /// Cavity on chest radiograph or AFB-positive sputum smear (Figure 2).
    SmearPositiveOrCavitary,
    /// AFB smear negative, chest radiograph consistent with TB (Figure 3).
    SmearNegativeCxrConsistent,
    /// AFB smear, NAA, and culture negative; abnormal chest radiograph not
    /// consistent with TB (Figure 4).
    AbnormalCxrNotConsistent,
}

/// Contact priority.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    /// Label.
    pub fn label(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }
}

/// A condition tested by the priority trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Household,
    #[serde(rename = "age_under_5")]
    AgeUnder5,
    MedicalRisk,
    MedicalProcedure,
    CongregateSetting,
    ExceedsHighPriorityLimits,
    #[serde(rename = "age_5_to_15")]
    Age5To15,
    ExceedsMediumPriorityLimits,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartRule {
    pub id: String,
    pub when: String,
    pub start: String,
    pub months_before: Option<u32>,
    pub weeks_before: Option<u32>,
    pub anchor: String,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfectiousPeriod {
    pub start_rules: Vec<StartRule>,
    pub earlier_start_note: CitedText,
    pub end_criteria: Vec<CitedText>,
    pub stricter_end: CitedText,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeRule {
    pub condition: Condition,
    pub priority: Priority,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriorityTree {
    pub case_type: CaseType,
    pub label: String,
    pub source: String,
    pub section: String,
    pub rules: Vec<TreeRule>,
    pub otherwise: Priority,
}

impl PriorityTree {
    /// The priority for a contact with these conditions, and the rule that decided it.
    pub fn classify(&self, has: impl Fn(Condition) -> bool) -> (Priority, Option<Condition>) {
        for r in &self.rules {
            if has(r.condition) {
                return (r.priority, Some(r.condition));
            }
        }
        (self.otherwise, None)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConditionInfo {
    pub id: Condition,
    pub label: String,
    pub definition: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Timeframe {
    pub case_type: String,
    pub priority: Priority,
    pub encounter_business_days: u32,
    pub evaluation_business_days: u32,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Testing {
    pub window_weeks_min: i64,
    pub window_weeks_max: i64,
    pub positive_tst_mm: u32,
    pub notes: Vec<CitedText>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpecialGroup {
    pub id: String,
    pub text: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Expansion {
    pub intro: String,
    pub triggers: Vec<String>,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataGroup {
    pub heading: String,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexCaseData {
    pub source: String,
    pub section: String,
    pub groups: Vec<DataGroup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContactInvestigation {
    pub transmission_factors: Vec<CitedText>,
    pub initiate: Vec<CitedText>,
    pub infectious_period: InfectiousPeriod,
    pub priority_trees: Vec<PriorityTree>,
    pub conditions: Vec<ConditionInfo>,
    pub exposure_notes: Vec<CitedText>,
    pub timeframes: Vec<Timeframe>,
    pub timeframe_note: CitedText,
    pub testing: Testing,
    pub special_groups: Vec<SpecialGroup>,
    pub expansion: Expansion,
    pub index_case_data: IndexCaseData,
    pub interview_settings: CitedText,
}

impl ContactInvestigation {
    /// The tree for a case type.
    pub fn tree(&self, t: CaseType) -> &PriorityTree {
        self.priority_trees
            .iter()
            .find(|p| p.case_type == t)
            .expect("every case type has a priority tree (checked by tests)")
    }

    /// Initial-encounter and evaluation time frames for a priority.
    pub fn timeframe(&self, t: CaseType, p: Priority) -> Option<&Timeframe> {
        let key = match t {
            CaseType::SmearPositiveOrCavitary => "smear_positive_or_cavitary",
            CaseType::SmearNegativeCxrConsistent => "smear_negative_cxr_consistent",
            CaseType::AbnormalCxrNotConsistent => "abnormal_cxr_not_consistent",
        };
        self.timeframes
            .iter()
            .find(|f| f.priority == p && f.case_type == key)
            .or_else(|| {
                self.timeframes
                    .iter()
                    .find(|f| f.priority == p && f.case_type == "any")
            })
    }

    /// Label of a condition.
    pub fn condition_label(&self, c: Condition) -> String {
        self.conditions
            .iter()
            .find(|i| i.id == c)
            .map(|i| i.label.clone())
            .unwrap_or_else(|| format!("{c:?}"))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct Clinic {
    pub name: String,
    pub organization: String,
    pub address: String,
    pub town: String,
    pub phone: String,
    pub referrals: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MaProgram {
    pub division: String,
    pub address: String,
    pub phone_business_hours: String,
    pub phone_after_hours: String,
    pub fax: String,
    pub reporting: Vec<CitedText>,
    pub source: String,
    pub clinics_source: String,
    pub clinics: Vec<Clinic>,
}

/// Regimen identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum RegimenId {
    #[serde(rename = "3HP")]
    R3HP,
    #[serde(rename = "4R")]
    R4R,
    #[serde(rename = "3HR")]
    R3HR,
    #[serde(rename = "6H")]
    R6H,
    #[serde(rename = "6H_twice_weekly")]
    R6HTwiceWeekly,
    #[serde(rename = "9H")]
    R9H,
    #[serde(rename = "9H_twice_weekly")]
    R9HTwiceWeekly,
}

impl RegimenId {
    /// Short code as written in guidelines.
    pub fn code(self) -> &'static str {
        match self {
            RegimenId::R3HP => "3HP",
            RegimenId::R4R => "4R",
            RegimenId::R3HR => "3HR",
            RegimenId::R6H => "6H",
            RegimenId::R6HTwiceWeekly => "6H (twice weekly)",
            RegimenId::R9H => "9H",
            RegimenId::R9HTwiceWeekly => "9H (twice weekly)",
        }
    }
}

/// Dosing frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Daily,
    OnceWeekly,
    TwiceWeekly,
}

/// Administration mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Administration {
    /// In-person directly observed therapy.
    Dot,
    /// Video directly observed therapy.
    Vdot,
    /// Self-administered therapy.
    Sat,
}

impl Administration {
    /// Label.
    pub fn label(self) -> &'static str {
        match self {
            Administration::Dot => "directly observed (DOT)",
            Administration::Vdot => "video directly observed (vDOT)",
            Administration::Sat => "self-administered (SAT)",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Completion {
    pub doses_required: u32,
    pub within_days: Option<i64>,
    pub min_days_between_doses: Option<i64>,
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Regimen {
    pub id: RegimenId,
    pub name: String,
    pub drugs: Vec<String>,
    pub frequency: Frequency,
    pub interval_days: Option<i64>,
    pub total_doses: u32,
    pub duration_text: String,
    pub cdc_rank: String,
    pub dph_listed: bool,
    pub dph_administration: Option<String>,
    pub allowed_administration: Vec<Administration>,
    pub completion: Completion,
    pub recommended_for: Vec<String>,
    pub not_recommended_for: Vec<String>,
    pub monitoring: Vec<CitedText>,
    pub monitoring_visit_weeks: Option<Vec<u32>>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Interruption {
    pub text: String,
    pub source: String,
    pub section: String,
    pub interruption_days: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ltbi {
    pub before_starting: Vec<CitedText>,
    pub regimens: Vec<Regimen>,
    pub monitoring: Vec<CitedText>,
    pub stop_and_call: CitedText,
    pub education: Vec<CitedText>,
    pub adherence: Vec<CitedText>,
    pub interruption: Interruption,
    pub post_treatment: CitedText,
}

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
pub struct TbContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, ContentSource>,
    pub contact_investigation: ContactInvestigation,
    pub ma_program: MaProgram,
    pub ltbi: Ltbi,
}

static CONTENT: LazyLock<TbContent> = LazyLock::new(|| {
    crate::core::practice::load(TB_CONTENT).expect("tuberculosis content matches the Rust types")
});

/// The bundled content.
pub fn content() -> &'static TbContent {
    &CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_loads_and_references_known_sources() {
        let c = content();
        let ci = &c.contact_investigation;
        for t in [
            CaseType::SmearPositiveOrCavitary,
            CaseType::SmearNegativeCxrConsistent,
            CaseType::AbnormalCxrNotConsistent,
        ] {
            let tree = ci.tree(t);
            assert!(c.sources.contains_key(&tree.source));
        }
        for r in &c.ltbi.regimens {
            assert!(c.sources.contains_key(&r.completion.source), "{:?}", r.id);
            for s in &r.sources {
                assert!(c.sources.contains_key(s), "{:?} {s}", r.id);
            }
            for m in &r.monitoring {
                assert!(c.sources.contains_key(&m.source));
            }
        }
        for n in ci.testing.notes.iter().chain(&ci.initiate) {
            assert!(c.sources.contains_key(&n.source));
        }
        assert_eq!(c.ltbi.regimens.len(), 7);
        assert!(c.ma_program.clinics.len() >= 10);
    }

    #[test]
    fn figure_2_tree_follows_the_published_order() {
        let tree = content()
            .contact_investigation
            .tree(CaseType::SmearPositiveOrCavitary);
        // A 10-year-old with no other factor is medium priority.
        let (p, why) = tree.classify(|c| c == Condition::Age5To15);
        assert_eq!((p, why), (Priority::Medium, Some(Condition::Age5To15)));
        // Household beats everything.
        assert_eq!(tree.classify(|_| true).0, Priority::High);
        assert_eq!(tree.classify(|_| false), (Priority::Low, None));
    }
}
