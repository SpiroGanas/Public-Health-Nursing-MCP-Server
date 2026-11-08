//! Typed access to the Phase 12 reference content (`content/epr/`,
//! `content/env/`). Each file is embedded through `phn_content::ITEMS`, so it
//! is also an MCP resource and is checked by `cargo xtask validate-content`.

use std::collections::BTreeMap;

use phn_content::{ContentMeta, ReviewStatus};
use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Content ids.
pub const HAZARDS: &str = "epr-hazard-health-actions";
pub const DISPENSING: &str = "epr-dispensing-site";
pub const AIR_QUALITY: &str = "env-air-quality";
pub const HEAT: &str = "env-heat-health";
pub const HVA: &str = "epr-hazard-vulnerability";
pub const SHELTER: &str = "epr-functional-needs-shelter";
pub const POWER: &str = "epr-electricity-dependence";
pub const CASPER: &str = "epr-casper";

/// Parse a content item into `T`.
pub fn load<T: DeserializeOwned>(id: &str) -> Result<T> {
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

/// The review caveats for content items (empty once reviewed).
pub fn review_caveats(ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| meta(id).ok())
        .filter(|m| m.review.status == ReviewStatus::Pending)
        .filter_map(|m| m.review_caveat())
        .collect()
}

/// Developer confidence in a drafted entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// A source document listed in a content file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub effective_date: Option<String>,
    pub retrieved: String,
}

impl DocSource {
    /// As an envelope citation (bundled content, so mode `snapshot`).
    pub fn to_source(&self) -> Source {
        Source {
            title: self.title.clone(),
            publisher: self.publisher.clone(),
            url: Some(self.url.clone()),
            effective_date: self.effective_date.clone(),
            retrieved: Some(self.retrieved.clone()),
            mode: SourceMode::Snapshot,
        }
    }
}

/// A statement with its citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cited {
    pub text: String,
    /// Key into the file's `sources`.
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

/// A cited statement as returned by a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedOut {
    pub text: String,
    /// Source title and section.
    pub citation: String,
    pub confidence: Confidence,
}

/// Turn cited statements into tool output, resolving source titles.
pub fn cite_all(items: &[Cited], sources: &BTreeMap<String, DocSource>) -> Vec<CitedOut> {
    items
        .iter()
        .map(|c| CitedOut {
            text: c.text.clone(),
            citation: format!(
                "{}, {}",
                sources
                    .get(&c.source)
                    .map(|s| s.title.as_str())
                    .unwrap_or(c.source.as_str()),
                c.section
            ),
            confidence: c.confidence,
        })
        .collect()
}

/// Envelope citations for the given source keys (deduplicated, in order).
pub fn sources_for<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    sources: &BTreeMap<String, DocSource>,
) -> Vec<Source> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for k in keys {
        if seen.contains(&k) {
            continue;
        }
        seen.push(k);
        if let Some(s) = sources.get(k) {
            out.push(s.to_source());
        }
    }
    out
}

/// Markdown bullet for a cited statement.
pub fn bullet(c: &CitedOut) -> String {
    let flag = match c.confidence {
        Confidence::Low => " **(low confidence)**",
        _ => "",
    };
    format!("- {}{flag} _({})_\n", c.text, c.citation)
}

// ---------------------------------------------------------------- hazards (EPR-05)

/// One hazard with its NWS events and guidance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hazard {
    pub id: String,
    pub label: String,
    pub nws_events: Vec<String>,
    pub fema_incident_types: Vec<String>,
    pub health_effects: Vec<Cited>,
    pub at_risk: Vec<Cited>,
    pub public_health_actions: Vec<Cited>,
    pub public_messages: Vec<String>,
    pub message_sources: Vec<String>,
    pub related_tools: Vec<String>,
    #[serde(default)]
    pub secondary: Vec<String>,
}

/// A hazard that follows others (power outage, carbon monoxide).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecondaryHazard {
    pub id: String,
    pub label: String,
    pub health_effects: Vec<Cited>,
    pub public_health_actions: Vec<Cited>,
    pub public_messages: Vec<String>,
    pub message_sources: Vec<String>,
}

/// `content/epr/hazard_health_actions.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HazardActions {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub general_actions: Vec<Cited>,
    pub hazards: Vec<Hazard>,
    pub secondary_hazards: Vec<SecondaryHazard>,
    pub fema_incident_labels: BTreeMap<String, String>,
}

impl HazardActions {
    /// The hazard an NWS event belongs to (case-insensitive exact match).
    pub fn for_event(&self, event: &str) -> Option<&Hazard> {
        let e = event.trim().to_ascii_lowercase();
        self.hazards
            .iter()
            .find(|h| h.nws_events.iter().any(|n| n.to_ascii_lowercase() == e))
    }

    /// A hazard by id, label, or NWS event name.
    pub fn find(&self, query: &str) -> Option<&Hazard> {
        let q = phn_core::text::normalize(query);
        if q.is_empty() {
            return None;
        }
        self.hazards
            .iter()
            .find(|h| phn_core::text::normalize(&h.id) == q)
            .or_else(|| self.for_event(query))
            .or_else(|| {
                self.hazards.iter().find(|h| {
                    let l = phn_core::text::normalize(&h.label);
                    l.contains(&q)
                        || phn_core::text::normalize(&h.id.replace('_', " ")).contains(&q)
                })
            })
    }

    /// FEMA incident type label.
    pub fn incident_label(&self, incident: &str) -> String {
        self.fema_incident_labels
            .get(incident)
            .cloned()
            .unwrap_or_else(|| incident.to_string())
    }
}

// ---------------------------------------------------------------- shared shapes

/// A planning assumption with its basis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Assumption {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    /// "tool_default" (no published source), "source" (cited), or "computed".
    pub basis: String,
    #[serde(default)]
    pub source: Option<String>,
    pub note: String,
}

/// A cited list of short items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CitedList {
    pub source: String,
    pub section: String,
    pub items: Vec<String>,
}

// ---------------------------------------------------------------- dispensing (EPR-02)

/// A station in a dispensing model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationDef {
    pub id: String,
    pub label: String,
    pub minutes_per_client: f64,
    pub fraction_of_clients: f64,
    pub basis: String,
    pub note: String,
}

/// A dispensing model (oral prophylaxis or vaccination).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispensingModel {
    pub id: String,
    pub label: String,
    pub stations: Vec<StationDef>,
    pub observation_minutes: f64,
    #[serde(default)]
    pub observation_note: Option<String>,
}

/// `content/epr/dispensing.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dispensing {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub definitions: Vec<Cited>,
    pub activation_elements: CitedList,
    pub access_considerations: Vec<Cited>,
    pub planning_defaults: Vec<Assumption>,
    pub models: Vec<DispensingModel>,
    pub support_roles: CitedList,
}

impl Dispensing {
    /// A planning default by id.
    pub fn default_value(&self, id: &str) -> Result<&Assumption> {
        self.planning_defaults
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| PhnError::Internal(format!("planning default {id} missing")))
    }
}

// ---------------------------------------------------------------- air quality (ENV-02)

/// An AQI category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AqiCategoryDef {
    pub id: String,
    pub name: String,
    pub min: u32,
    pub max: u32,
    pub color: String,
    pub rgb: Vec<u8>,
    pub health_concern: Cited,
}

/// Breakpoints for one pollutant and averaging time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreakpointTable {
    pub pollutant: String,
    pub averaging: String,
    pub unit: String,
    pub truncate_decimals: u32,
    /// [concentration low, concentration high, AQI low, AQI high]
    pub rows: Vec<[f64; 4]>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Sensitive groups for a pollutant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensitiveGroup {
    pub pollutant: String,
    pub groups: String,
    pub source: String,
    pub section: String,
}

/// A protective action for an audience at a category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AqAction {
    pub audience: String,
    pub category: String,
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

/// `content/env/air_quality.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirQuality {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub categories: Vec<AqiCategoryDef>,
    pub breakpoints: Vec<BreakpointTable>,
    pub breakpoint_source: String,
    pub breakpoint_section: String,
    pub pm25_change: Cited,
    pub sensitive_groups: Vec<SensitiveGroup>,
    pub actions: Vec<AqAction>,
    pub tips: Vec<Cited>,
}

impl AirQuality {
    /// The category for an AQI value (values above 500 are Hazardous).
    pub fn category_for(&self, aqi: u32) -> &AqiCategoryDef {
        self.categories
            .iter()
            .find(|c| aqi >= c.min && aqi <= c.max)
            .unwrap_or_else(|| self.categories.last().expect("categories"))
    }

    /// A category by AirNow name ("Unhealthy for Sensitive Groups") or id.
    pub fn category_named(&self, name: &str) -> Option<&AqiCategoryDef> {
        let n = phn_core::text::normalize(name);
        self.categories.iter().find(|c| {
            phn_core::text::normalize(&c.name) == n || phn_core::text::normalize(&c.id) == n
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hazard_content_loads_and_maps_events() {
        let h: HazardActions = load(HAZARDS).unwrap();
        assert_eq!(h.for_event("Heat Advisory").unwrap().id, "extreme_heat");
        assert_eq!(
            h.for_event("excessive heat warning").unwrap().id,
            "extreme_heat"
        );
        assert_eq!(h.for_event("Flash Flood Warning").unwrap().id, "flood");
        assert_eq!(h.find("winter storm").unwrap().id, "winter_storm");
        assert_eq!(h.find("tornado").unwrap().id, "tornado");
        assert!(h.for_event("Small Craft Advisory").is_none());
        // Every cited statement points at a listed source.
        for hz in &h.hazards {
            for c in hz
                .health_effects
                .iter()
                .chain(&hz.at_risk)
                .chain(&hz.public_health_actions)
            {
                assert!(h.sources.contains_key(&c.source), "{} {}", hz.id, c.source);
            }
            for s in &hz.secondary {
                assert!(h.secondary_hazards.iter().any(|x| &x.id == s));
            }
            for s in &hz.message_sources {
                assert!(h.sources.contains_key(s), "{} {s}", hz.id);
            }
        }
        // Every event name is unique across hazards.
        let mut all: Vec<String> = h
            .hazards
            .iter()
            .flat_map(|x| x.nws_events.iter().map(|e| e.to_ascii_lowercase()))
            .collect();
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
    }
}
