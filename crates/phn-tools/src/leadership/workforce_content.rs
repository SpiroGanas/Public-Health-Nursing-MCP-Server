//! Typed access to the Phase 18 workforce content (all pending review):
//! `content/leadership/workforce_competencies.yaml` (LEAD-06),
//! `open_meeting_law.yaml` and `ma_legal_holidays.yaml` (LEAD-07), and
//! `standing_orders.yaml` (LEAD-08).

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{PhnError, Result};
use serde::Deserialize;

pub use crate::epr::content::{Cited, Confidence, DocSource};
use crate::epr::content::{load, review_caveats, sources_for};
use phn_core::Source;

pub const COMPETENCIES: &str = "leadership-workforce-competencies";
pub const OML: &str = "leadership-open-meeting-law";
pub const HOLIDAYS: &str = "leadership-ma-legal-holidays";
pub const STANDING_ORDERS: &str = "leadership-standing-orders";

/// Review caveats for the given content ids (empty once reviewed).
pub fn caveats(ids: &[&str]) -> Vec<String> {
    review_caveats(ids)
}

/// Envelope citations for source keys of one content file.
pub fn cite(keys: &[&str], sources: &BTreeMap<String, DocSource>) -> Vec<Source> {
    sources_for(keys.iter().copied(), sources)
}

/// An item that was looked for and not verified or not bundled.
#[derive(Debug, Clone, Deserialize)]
pub struct NotVerified {
    pub item: String,
    pub detail: String,
}

// ---------------------------------------------------------------- competencies

#[derive(Debug, Clone, Deserialize)]
pub struct Domain {
    pub number: u32,
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Framework {
    pub id: String,
    pub label: String,
    pub short_label: String,
    pub owner: String,
    pub edition: String,
    pub domains: Vec<Domain>,
    pub domains_confidence: Confidence,
    #[serde(default)]
    pub domains_note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tier {
    pub id: String,
    pub number: u32,
    pub label: String,
    pub text: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RatingLevel {
    pub value: u32,
    pub label: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NumDefault {
    pub value: f64,
    pub basis: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompetencyDefaults {
    pub target_rating: NumDefault,
    pub minimum_group_size: NumDefault,
    pub due_soon_days: NumDefault,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrainingAction {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrainingActions {
    pub note: String,
    #[serde(flatten)]
    pub rest: BTreeMap<String, serde_yaml_ng_free::Skip>,
}

/// Placeholder type so `flatten` ignores the list entries; `training_actions` is read as a raw
/// list below.
pub mod serde_yaml_ng_free {
    use serde::Deserialize;
    #[derive(Debug, Clone, Deserialize)]
    #[serde(untagged)]
    pub enum Skip {
        Any(serde_json::Value),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrosswalkPair {
    pub core_competencies_2021: u32,
    pub cphn_2018: u32,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Crosswalk {
    pub note: String,
    pub pairs: Vec<CrosswalkPair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PositionAlias {
    pub position_id: String,
    pub words: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UniversalItem {
    pub text: String,
    pub label: String,
    pub months_from_hire: u32,
    pub kind: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CredentialRules {
    pub snapshot_id: String,
    pub note: String,
    pub confidence: Confidence,
    pub position_aliases: Vec<PositionAlias>,
    pub universal_items: Vec<UniversalItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompetencyContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub frameworks: Vec<Framework>,
    pub tiers: Vec<Tier>,
    pub rating_scale: Vec<RatingLevel>,
    pub rating_scale_note: String,
    pub defaults: CompetencyDefaults,
    pub training_use: Vec<Cited>,
    pub training_actions: Vec<serde_json::Value>,
    pub crosswalk: Crosswalk,
    pub credentials: CredentialRules,
    pub not_verified: Vec<NotVerified>,
}

impl CompetencyContent {
    pub fn load() -> Result<Self> {
        let mut c: Self = load(COMPETENCIES)?;
        // `tiers` holds three tier objects and a note object in the YAML; the loader above reads
        // only what is typed, so the note is not needed here.
        c.tiers.retain(|t| t.number >= 1);
        Ok(c)
    }

    pub fn framework(&self, id: &str) -> Result<&Framework> {
        self.frameworks
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| PhnError::Internal(format!("framework {id} missing")))
    }

    /// Training step texts (the list items are `{id, text}` maps; the trailing note is skipped).
    pub fn actions(&self) -> Vec<TrainingAction> {
        self.training_actions
            .iter()
            .filter_map(|v| serde_json::from_value::<TrainingAction>(v.clone()).ok())
            .collect()
    }
}

// ---------------------------------------------------------------- open meeting law

#[derive(Debug, Clone, Deserialize)]
pub struct Purpose {
    pub number: u32,
    pub summary: String,
    pub chair_declaration_required: bool,
    pub special_requirements: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecSession {
    pub purposes: Vec<Purpose>,
    pub steps: Vec<String>,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
    pub steps_source: String,
    pub steps_section: String,
    pub only_for_listed_purposes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NoticeItem {
    pub id: String,
    pub text: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Question {
    pub question: String,
    pub answer: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Notice {
    pub rule_text: String,
    pub source: String,
    pub section: String,
    pub hours_required: i64,
    pub contents: Vec<NoticeItem>,
    pub common_questions: Vec<Question>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MinutesElement {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Conditional {
    pub when: String,
    pub text: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MinutesApproval {
    pub text: String,
    pub source: String,
    pub section: String,
    pub days_to_approve: i64,
    pub days_to_release_on_request: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Minutes {
    pub required_elements: Vec<MinutesElement>,
    pub source: String,
    pub section: String,
    pub conditional: Vec<Conditional>,
    pub approval: MinutesApproval,
    pub no_secret_ballot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublicBody {
    pub text: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteParticipation {
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OmlContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub public_body: PublicBody,
    pub notice: Notice,
    pub executive_session: ExecSession,
    pub minutes: Minutes,
    pub chair_duties: Vec<Cited>,
    pub remote_participation: RemoteParticipation,
    pub not_verified: Vec<NotVerified>,
}

impl OmlContent {
    pub fn load() -> Result<Self> {
        load(OML)
    }
}

// ---------------------------------------------------------------- holidays

#[derive(Debug, Clone, Deserialize)]
pub struct HolidayRow {
    pub date: String,
    pub weekday: String,
    pub name: String,
    pub scope: String,
    pub rule: String,
    pub moved_from: Option<String>,
    pub second_source: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HolidayContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub suffolk_county_note: String,
    pub holidays: Vec<HolidayRow>,
}

impl HolidayContent {
    pub fn load() -> Result<Self> {
        load(HOLIDAYS)
    }
}

// ---------------------------------------------------------------- standing orders

#[derive(Debug, Clone, Deserialize)]
pub struct RequiredElement {
    pub id: String,
    pub number: u32,
    pub text: String,
    pub section_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdText {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VaccineOutline {
    pub note: String,
    pub steps: Vec<String>,
    pub source: String,
    pub second_source: String,
    pub license_statement: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DphItem {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DphOrders {
    pub note: String,
    pub items: Vec<DphItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderType {
    pub id: String,
    pub label: String,
    pub uses: Vec<String>,
    pub sections: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkeletonSection {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub clinical: bool,
    #[serde(default)]
    pub element: Option<String>,
    pub guidance: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SectionalChecks {
    pub menu_phrases: Vec<String>,
    pub alteration_phrases: Vec<String>,
    pub prescribing_phrases: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StandingOrderContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub required_elements: Vec<RequiredElement>,
    pub limits: Vec<IdText>,
    pub limits_note: String,
    pub vaccine_elements: Vec<IdText>,
    pub nurse_manager_note: String,
    pub outline_vaccine: VaccineOutline,
    pub dph_orders: DphOrders,
    pub order_types: Vec<OrderType>,
    pub skeleton: Vec<SkeletonSection>,
    pub sectional_checks: SectionalChecks,
    pub not_verified: Vec<NotVerified>,
}

impl StandingOrderContent {
    pub fn load() -> Result<Self> {
        load(STANDING_ORDERS)
    }

    pub fn section(&self, id: &str) -> Option<&SkeletonSection> {
        self.skeleton.iter().find(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn competency_content_loads_with_eight_domains_per_framework() {
        let c = CompetencyContent::load().unwrap();
        assert_eq!(c.frameworks.len(), 2);
        for f in &c.frameworks {
            assert_eq!(f.domains.len(), 8, "{}", f.id);
        }
        assert_eq!(c.tiers.len(), 3);
        assert_eq!(c.rating_scale.len(), 4);
        assert_eq!(c.defaults.minimum_group_size.value, 3.0);
        assert!(!c.actions().is_empty());
        assert_eq!(c.crosswalk.pairs.len(), 8);
    }

    #[test]
    fn oml_content_has_ten_purposes_and_resolves_sources() {
        let c = OmlContent::load().unwrap();
        assert_eq!(c.executive_session.purposes.len(), 10);
        assert_eq!(c.notice.hours_required, 48);
        for q in &c.notice.common_questions {
            assert!(c.sources.contains_key(&q.source), "{}", q.source);
        }
        for n in &c.notice.contents {
            assert!(c.sources.contains_key(&n.source), "{}", n.source);
        }
        assert!(c.sources.contains_key(&c.minutes.source));
    }

    #[test]
    fn standing_order_skeleton_covers_every_element_and_type_section() {
        let c = StandingOrderContent::load().unwrap();
        assert_eq!(c.required_elements.len(), 10);
        for e in &c.required_elements {
            for s in &e.section_ids {
                assert!(c.section(s).is_some(), "{s}");
            }
        }
        for t in &c.order_types {
            for s in &t.sections {
                assert!(c.section(s).is_some(), "{} {s}", t.id);
            }
        }
        // No clinical text is bundled: every clinical section is only a placeholder.
        for s in c.skeleton.iter().filter(|s| s.clinical) {
            assert!(s.placeholder.starts_with("[NURSE:"), "{}", s.id);
        }
    }
}
