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

// ---------------------------------------------------------------- heat (ENV-01)

/// A HeatRisk level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatRiskLevel {
    pub level: u8,
    pub name: String,
    pub color: String,
    pub meaning: Vec<String>,
    pub who_at_risk: Vec<String>,
    pub actions: Vec<String>,
}

/// Activation steps for a HeatRisk level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activation {
    pub level: u8,
    pub actions: Vec<Cited>,
}

/// An outreach script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutreachScript {
    pub id: String,
    pub title: String,
    pub lines: Vec<String>,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

/// A heat illness and how to respond.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeatIllness {
    pub name: String,
    pub signs: String,
    pub response: String,
}

/// A local-file column and the names it may have.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnAlias {
    pub field: String,
    pub aliases: Vec<String>,
}

/// A labeled link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LinkDef {
    pub label: String,
    pub url: String,
}

/// `content/env/heat.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heat {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub heatrisk_levels: Vec<HeatRiskLevel>,
    pub heatrisk_source: String,
    pub heatrisk_section: String,
    pub heat_sensitive_groups: Vec<Cited>,
    pub activation: Vec<Activation>,
    pub outreach_scripts: Vec<OutreachScript>,
    pub schools_and_child_care: Vec<Cited>,
    pub outdoor_workers: Vec<Cited>,
    pub heat_illness: Vec<HeatIllness>,
    pub heat_illness_source: String,
    pub heat_illness_section: String,
    pub medications_and_power: Vec<Cited>,
    pub cooling_center_columns: Vec<ColumnAlias>,
    pub links: Vec<LinkDef>,
}

// ---------------------------------------------------------------- HVA (EPR-06)

/// The HVA method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HvaMethod {
    pub summary: String,
    pub formula: String,
    pub basis: String,
    pub note: String,
    pub phep_link: Cited,
}

/// One point on a scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScalePoint {
    pub score: u8,
    pub label: String,
    pub description: String,
}

/// Seed mappings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedMapping {
    pub likelihood: BTreeMap<String, u8>,
    pub human_consequence: BTreeMap<String, u8>,
    pub note: String,
}

/// HVA scales.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HvaScales {
    pub probability: Vec<ScalePoint>,
    pub impact: Vec<ScalePoint>,
    pub preparedness_gap: Vec<ScalePoint>,
    pub seed_mapping: SeedMapping,
}

/// ResilientMass statewide ratings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResilientMassRatings {
    pub likelihood: String,
    pub human: String,
    pub economic: String,
    pub environment: String,
    pub warning: String,
    pub impact_scale: String,
}

/// A hazard in the HVA list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HvaHazard {
    pub id: String,
    pub label: String,
    pub category: String,
    #[serde(default)]
    pub resilientmass: Option<ResilientMassRatings>,
    pub fema_incident_types: Vec<String>,
    pub nws_hazards: Vec<String>,
    pub nri_prefixes: Vec<String>,
}

/// Expected NRI columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NriColumns {
    pub county_id_columns: Vec<String>,
    pub tract_id_columns: Vec<String>,
    pub overall_score: String,
    pub overall_rating: String,
    pub hazard_score_suffix: String,
    pub hazard_rating_suffix: String,
    pub hazard_frequency_suffix: String,
    pub note: String,
}

/// `content/epr/hva.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hva {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub method: HvaMethod,
    pub scales: HvaScales,
    pub hazards: Vec<HvaHazard>,
    pub hazard_source: String,
    pub hazard_section: String,
    pub nri: NriColumns,
}

// ---------------------------------------------------------------- shelter intake (EPR-03)

/// An intake question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IntakeQuestion {
    pub id: String,
    pub text: String,
    /// "yes_no" or "text".
    pub kind: String,
}

/// A CMIST domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CmistDomain {
    pub id: String,
    pub letter: String,
    pub label: String,
    pub questions: Vec<IntakeQuestion>,
}

/// `content/epr/functional_needs_shelter.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shelter {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub cmist_definition: Cited,
    pub mass_care_tasks: Vec<Cited>,
    pub domains: Vec<CmistDomain>,
    pub questions_source: String,
    pub questions_section: String,
    pub dispositions: Vec<String>,
    pub electric_equipment: CitedList,
    pub staffing_defaults: Vec<Assumption>,
}

// ---------------------------------------------------------------- emPOWER (EPR-04)

/// `content/epr/electricity_dependence.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Power {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub definition: Cited,
    pub coverage: Cited,
    pub suppression: Cited,
    pub equipment: CitedList,
    pub datasets: Vec<Cited>,
    pub actions: Vec<Cited>,
    pub utilities: CitedList,
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
