//! Infection control content for congregate settings
//! (`content/cd/infection_control.yaml`): precautions, PPE, settings, and EPA
//! disinfectant lists by pathogen.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_core::text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContentMeta;
use crate::disease_rules::RuleSource;
use crate::reportable::Confidence;

/// Text with a citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Note {
    pub text: String,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
}

/// A precaution type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrecautionType {
    pub id: String,
    pub label: String,
    pub definition: String,
    pub ppe: Vec<String>,
    pub source: String,
    pub section: String,
}

/// An EPA list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EpaList {
    pub id: String,
    pub title: String,
    pub url: String,
}

/// Congregate setting kinds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CongregateSetting {
    LongTermCare,
    School,
    ChildCare,
    Shelter,
    DayProgram,
}

impl CongregateSetting {
    /// Key used in `setting_notes`.
    pub fn key(self) -> &'static str {
        match self {
            CongregateSetting::LongTermCare => "long_term_care",
            CongregateSetting::School => "school",
            CongregateSetting::ChildCare => "child_care",
            CongregateSetting::Shelter => "shelter",
            CongregateSetting::DayProgram => "day_program",
        }
    }
}

/// A setting and its general notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SettingInfo {
    pub id: CongregateSetting,
    pub label: String,
    pub health_care_setting: bool,
    pub notes: Vec<Note>,
}

/// Infection control guidance for one pathogen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pathogen {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub precautions: String,
    pub duration: String,
    pub appendix_a_comment: String,
    pub epa_lists: Vec<String>,
    pub hand_hygiene: String,
    pub source: String,
    pub section: String,
    #[serde(default)]
    pub confidence: Option<Confidence>,
    /// Setting key (`all`, `long_term_care`, ...) to notes.
    #[serde(default)]
    pub setting_notes: BTreeMap<String, Vec<Note>>,
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfectionControl {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, RuleSource>,
    pub precaution_types: Vec<PrecautionType>,
    pub epa_lists: Vec<EpaList>,
    pub disinfectant_basics: Vec<Note>,
    pub settings: Vec<SettingInfo>,
    pub pathogens: Vec<Pathogen>,
}

static CONTENT: LazyLock<InfectionControl> = LazyLock::new(|| {
    let item = crate::ITEMS
        .iter()
        .find(|i| i.id == "ma-infection-control")
        .expect("infection control item");
    let v = crate::yaml_to_json(item.yaml).expect("infection control YAML parses");
    serde_json::from_value(v).expect("infection control content matches the Rust types")
});

/// The bundled content.
pub fn content() -> &'static InfectionControl {
    &CONTENT
}

/// Find a pathogen by id, name, or alias; best match first with its score.
pub fn find(query: &str) -> Vec<(&'static Pathogen, f64)> {
    let q = text::normalize(query);
    let mut out: Vec<(&'static Pathogen, f64)> = content()
        .pathogens
        .iter()
        .map(|p| {
            let mut names = vec![p.id.replace('_', " "), p.name.clone()];
            names.extend(p.aliases.iter().cloned());
            let best = names
                .iter()
                .map(|n| {
                    let nn = text::normalize(n);
                    if nn == q {
                        1.0
                    } else if nn.len() >= 3 && (q.contains(&nn) || nn.starts_with(&q)) {
                        0.9
                    } else {
                        text::similarity(&q, &nn)
                    }
                })
                .fold(0.0, f64::max);
            (p, best)
        })
        .filter(|(_, s)| *s > 0.0)
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_references_known_lists_and_sources() {
        let c = content();
        let lists: Vec<&str> = c.epa_lists.iter().map(|l| l.id.as_str()).collect();
        for p in &c.pathogens {
            assert!(c.sources.contains_key(&p.source), "{}", p.id);
            for l in &p.epa_lists {
                assert!(lists.contains(&l.as_str()), "{}: list {l}", p.id);
            }
            for (k, notes) in &p.setting_notes {
                for n in notes {
                    assert!(
                        c.sources.contains_key(&n.source),
                        "{}/{k}: {}",
                        p.id,
                        n.source
                    );
                }
            }
        }
        for s in &c.settings {
            for n in &s.notes {
                assert!(c.sources.contains_key(&n.source), "{}", n.source);
            }
        }
        assert_eq!(find("norovirus")[0].0.id, "norovirus");
        assert_eq!(find("C. diff")[0].0.id, "c_difficile");
        assert_eq!(find("shigella")[0].0.id, "enteric_bacteria");
    }
}
