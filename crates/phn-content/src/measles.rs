//! Measles exposure content (`content/cd/measles_exposure.yaml`): evidence of
//! immunity, post-exposure prophylaxis, and priority groups.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContentMeta;
use crate::disease_rules::RuleSource;

/// Text with a citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedText {
    pub text: String,
    pub source: String,
    pub section: String,
}

/// Evidence-of-immunity criteria for one group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImmunityCriteria {
    pub id: String,
    pub applies_to: String,
    pub doses_required: u32,
    pub born_before_1957_counts: bool,
    pub criteria: Vec<String>,
    pub source: String,
    pub section: String,
    pub quote: String,
}

/// MMR post-exposure window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MmrPep {
    pub within_hours: u32,
    pub anchor: String,
    pub min_age_months: u32,
    pub text: String,
    pub source: String,
    pub section: String,
}

/// Immune globulin post-exposure window and doses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IgPep {
    pub within_days: u32,
    pub anchor: String,
    pub igim_ml_per_kg: f64,
    pub igim_max_ml: f64,
    pub igiv_mg_per_kg: f64,
    pub text: String,
    pub source: String,
    pub section: String,
}

/// Interval from IG to MMR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MmrAfterIg {
    pub months_after_igim: u32,
    pub months_after_igiv: u32,
    pub text: String,
    pub source: String,
    pub section: String,
}

/// Post-exposure prophylaxis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeaslesPep {
    pub mmr: MmrPep,
    pub immune_globulin: IgPep,
    pub mmr_after_ig: MmrAfterIg,
}

/// A priority group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PriorityGroup {
    pub id: String,
    pub label: String,
    pub recommendation: String,
    pub source: String,
    pub section: String,
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeaslesContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, RuleSource>,
    pub evidence_of_immunity: Vec<ImmunityCriteria>,
    pub immunity_notes: Vec<CitedText>,
    pub pep: MeaslesPep,
    pub priority_groups: Vec<PriorityGroup>,
    pub exclusion_summary: Vec<CitedText>,
}

static CONTENT: LazyLock<MeaslesContent> = LazyLock::new(|| {
    let item = crate::ITEMS
        .iter()
        .find(|i| i.id == "ma-measles-exposure")
        .expect("measles item");
    let v = crate::yaml_to_json(item.yaml).expect("measles YAML parses");
    serde_json::from_value(v).expect("measles content matches the Rust types")
});

/// The bundled content.
pub fn content() -> &'static MeaslesContent {
    &CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_with_known_sources() {
        let c = content();
        assert_eq!(c.pep.mmr.within_hours, 72);
        assert_eq!(c.pep.immune_globulin.within_days, 6);
        assert_eq!(c.pep.immune_globulin.igim_max_ml, 15.0);
        for e in &c.evidence_of_immunity {
            assert!(c.sources.contains_key(&e.source), "{}", e.source);
        }
        for g in &c.priority_groups {
            assert!(c.sources.contains_key(&g.source), "{}", g.source);
        }
        assert!(
            c.evidence_of_immunity
                .iter()
                .any(|e| e.id == "health_care_personnel" && !e.born_before_1957_counts)
        );
    }
}
