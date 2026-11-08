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
    /// Immediately by telephone, on suspicion (105 CMR 300.131-300.136, 300.175,
    /// 300.181, or a DPH designation for an immediate call).
    Immediate,
    /// Immediately, and in no case more than 24 hours after diagnosis
    /// (105 CMR 300.100), or within 24 hours (300.135, 300.170, 300.180).
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
            Timeframe::Within24Hours => "as soon as possible, and within 24 hours",
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
    /// Sections of 105 CMR 300.000 (or other named source) the entry was
    /// transcribed from, e.g. "105 CMR 300.100; 300.170".
    pub citation: String,
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
        // Conditions added in the 2026-11-07 re-check.
        assert_eq!(top("West Nile virus"), "west-nile-virus");
        assert_eq!(top("aseptic meningitis"), "meningitis-viral");
        assert_eq!(top("Reye syndrome"), "reye-syndrome");
        assert_eq!(top("C. diff"), "clostridioides-difficile");
        assert_eq!(top("alpha-gal syndrome"), "alpha-gal-syndrome");
        assert_eq!(top("norovirus"), "norovirus");
        assert_eq!(top("syphilis"), "syphilis");
    }

    #[test]
    fn every_name_and_alias_finds_its_own_entry() {
        for c in &content().conditions {
            for key in std::iter::once(&c.name).chain(&c.aliases) {
                let hits = lookup(key);
                assert_eq!(
                    hits.first().map(|h| h.condition.id.as_str()),
                    Some(c.id.as_str()),
                    "\"{key}\" should find {}",
                    c.id
                );
            }
        }
    }

    /// Facts transcribed from 105 CMR 300.000 (Mass. Register #1470, 5/27/22)
    /// and DPH's provider reporting list (June 2026) in the 2026-11-07 re-check.
    /// If one fails, the content changed: confirm the change against the
    /// regulation and DPH's current lists before updating this test.
    #[test]
    fn entries_match_the_regulation_text() {
        let by_id = |id: &str| {
            content()
                .conditions
                .iter()
                .find(|c| c.id == id)
                .unwrap_or_else(|| panic!("{id} missing"))
        };
        // DPH's immediate-call list includes these ...
        for id in [
            "measles",
            "tetanus",
            "group-a-streptococcus-invasive",
            "hemolytic-uremic-syndrome",
            "tuberculosis",
        ] {
            assert_eq!(by_id(id).timeframe, Timeframe::Immediate, "{id}");
        }
        // ... but not these, which fall under 300.100's 24-hour limit.
        for id in ["pertussis", "mumps", "brucellosis", "q-fever"] {
            assert_eq!(by_id(id).timeframe, Timeframe::Within24Hours, "{id}");
        }
        // 300.180: tuberculosis, HIV, and sexually transmitted infections go
        // directly to DPH, not to the local board of health.
        for id in ["tuberculosis", "hiv", "syphilis", "gonorrhea", "chlamydia"] {
            assert_eq!(by_id(id).report_to, vec![ReportTo::Dph], "{id}");
        }
        // 300.100 age limits.
        assert!(
            by_id("group-b-streptococcus-invasive")
                .name
                .contains("younger than one year")
        );
        assert!(
            by_id("streptococcus-pneumoniae-invasive")
                .name
                .contains("younger than 18")
        );
        // Every entry cites its source.
        assert!(
            content()
                .conditions
                .iter()
                .all(|c| !c.citation.trim().is_empty())
        );
        // Not named in the regulation or in DPH's lists.
        for id in ["leptospirosis", "candida-auris"] {
            assert!(content().conditions.iter().all(|c| c.id != id), "{id}");
        }
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
