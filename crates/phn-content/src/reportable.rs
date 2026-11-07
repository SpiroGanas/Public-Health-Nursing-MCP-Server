//! Massachusetts reportable diseases and conditions (105 CMR 300.000).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_core::text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContentMeta;

/// How quickly a condition must be reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Timeframe {
    /// Immediately by telephone, on suspicion.
    Immediate,
    /// Within 24 hours.
    #[serde(rename = "within_24_hours")]
    Within24Hours,
    /// A program-specific channel or schedule (see notes).
    Other,
}

impl Timeframe {
    /// Short label for summaries.
    pub fn label(self) -> &'static str {
        match self {
            Timeframe::Immediate => "IMMEDIATELY by telephone (on suspicion)",
            Timeframe::Within24Hours => "within 24 hours",
            Timeframe::Other => "per the DPH program's requirements (see notes)",
        }
    }
}

/// Who is required to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Reporter {
    HealthCareProvider,
    Laboratory,
    LocalBoardOfHealth,
}

impl Reporter {
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            Reporter::HealthCareProvider => "health care providers",
            Reporter::Laboratory => "laboratories",
            Reporter::LocalBoardOfHealth => "the local board of health",
        }
    }
}

/// Where a report goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportTo {
    LocalBoardOfHealth,
    Dph,
}

impl ReportTo {
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            ReportTo::LocalBoardOfHealth => "local board of health",
            ReportTo::Dph => "Massachusetts DPH",
        }
    }
}

/// Developer confidence in a draft entry (resolved by subject matter review).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Broad grouping of a condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    SelectAgent,
    VaccinePreventable,
    InvasiveBacterial,
    Enteric,
    Vectorborne,
    Zoonotic,
    Respiratory,
    Bloodborne,
    SexuallyTransmitted,
    Mycobacterial,
    HealthcareAssociated,
    Environmental,
    Other,
}

/// One reportable condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Condition {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    #[serde(default)]
    pub lab_terms: Vec<String>,
    pub category: Category,
    pub timeframe: Timeframe,
    pub reporters: Vec<Reporter>,
    pub report_to: Vec<ReportTo>,
    pub guide_chapter: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub confidence: Confidence,
}

impl Condition {
    /// Every string the condition can be matched on.
    pub fn match_keys(&self) -> Vec<String> {
        let mut keys = Vec::with_capacity(1 + self.aliases.len() + self.lab_terms.len());
        keys.push(self.name.clone());
        keys.extend(self.aliases.iter().cloned());
        keys.extend(self.lab_terms.iter().cloned());
        keys
    }
}

/// A phone contact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Contact {
    pub name: String,
    pub phone: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Explanatory text for each timeframe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeframeText {
    pub immediate: String,
    pub within_24_hours: String,
    pub other: String,
}

impl TimeframeText {
    /// Text for one timeframe.
    pub fn for_timeframe(&self, t: Timeframe) -> &str {
        match t {
            Timeframe::Immediate => &self.immediate,
            Timeframe::Within24Hours => &self.within_24_hours,
            Timeframe::Other => &self.other,
        }
    }
}

/// Explanatory text for each reporter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReporterText {
    pub health_care_provider: String,
    pub laboratory: String,
    pub local_board_of_health: String,
}

/// The whole content file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReportableContent {
    pub meta: ContentMeta,
    pub contacts: BTreeMap<String, Contact>,
    pub timeframes: TimeframeText,
    pub reporters: ReporterText,
    pub conditions: Vec<Condition>,
}

static CONTENT: LazyLock<ReportableContent> = LazyLock::new(|| {
    let item = crate::ITEMS
        .iter()
        .find(|i| i.id == "ma-reportable-conditions")
        .expect("reportable conditions item is registered");
    let v = crate::yaml_to_json(item.yaml).expect("reportable_conditions.yaml parses");
    serde_json::from_value(v).expect("reportable_conditions.yaml matches the Rust types")
});

/// The bundled reportable-conditions content.
pub fn content() -> &'static ReportableContent {
    &CONTENT
}

/// A ranked lookup result.
#[derive(Debug, Clone)]
pub struct LookupHit<'a> {
    pub condition: &'a Condition,
    pub score: f64,
    pub matched_on: String,
}

/// Minimum score for a confident match.
pub const CONFIDENT_MATCH: f64 = 0.84;
/// Minimum score for a "did you mean" candidate.
pub const POSSIBLE_MATCH: f64 = 0.70;

/// Rank conditions against a disease name, organism, or lab result.
pub fn lookup(query: &str) -> Vec<LookupHit<'static>> {
    let conditions = &content().conditions;
    text::rank(query, conditions, Condition::match_keys, POSSIBLE_MATCH)
        .into_iter()
        .map(|r| LookupHit {
            condition: &conditions[r.index],
            score: r.score,
            matched_on: r.matched_on,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(q: &str) -> &'static str {
        let hits = lookup(q);
        assert!(!hits.is_empty(), "no hit for {q}");
        assert!(
            hits[0].score >= CONFIDENT_MATCH,
            "weak hit for {q}: {}",
            hits[0].score
        );
        &hits[0].condition.id
    }

    #[test]
    fn content_loads_with_unique_ids() {
        let c = content();
        assert!(c.conditions.len() > 60);
        let mut ids: Vec<_> = c.conditions.iter().map(|c| &c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.conditions.len());
        assert_eq!(c.contacts["dph_epidemiology"].phone, "617-983-6800");
    }

    #[test]
    fn finds_by_name_alias_and_lab_term() {
        assert_eq!(top("measles"), "measles");
        assert_eq!(top("Whooping cough"), "pertussis");
        assert_eq!(top("Salmonella Typhi isolated from blood"), "typhoid-fever");
        assert_eq!(top("salmonella"), "salmonellosis");
        assert_eq!(top("positive HBsAg"), "hepatitis-b");
        assert_eq!(top("E. coli O157:H7"), "stec");
        assert_eq!(top("pertusis"), "pertussis");
        assert_eq!(top("Neisseria meningitidis"), "meningococcal-disease");
        assert_eq!(top("rabies"), "rabies-human");
    }

    #[test]
    fn unrelated_queries_do_not_match_confidently() {
        for q in ["head lice", "sprained ankle", "asthma"] {
            let hits = lookup(q);
            assert!(
                hits.first().is_none_or(|h| h.score < CONFIDENT_MATCH),
                "{q} matched {:?}",
                hits.first().map(|h| (&h.condition.id, h.score))
            );
        }
    }
}
