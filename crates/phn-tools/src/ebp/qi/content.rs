//! Typed access to `content/ebp/quality_improvement.yaml`.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use serde::Deserialize;

use crate::epr::content::{Cited, Confidence, DocSource};

/// Content id of `content/ebp/quality_improvement.yaml`.
pub const QI_CONTENT: &str = "ebp-quality-improvement";

#[derive(Debug, Clone, Deserialize)]
pub struct Question {
    pub id: String,
    pub number: u8,
    pub question: String,
    pub how_the_tool_helps: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mfi {
    pub summary: Cited,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Attribute {
    pub id: String,
    pub label: String,
    pub note: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AimDef {
    pub attributes: Vec<Attribute>,
    pub template: String,
    pub examples: Vec<Cited>,
    pub sub_aims_note: Cited,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeasureTypeDef {
    pub id: String,
    pub label: String,
    pub plain: String,
    pub prompts: Vec<String>,
    pub example: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub id: String,
    pub label: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriverDef {
    pub description: Cited,
    pub columns: Vec<String>,
    pub reading_tip: Cited,
    pub change_package_note: Cited,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CategoryDef {
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FishboneDef {
    pub description: Cited,
    pub default_categories: Vec<CategoryDef>,
    pub category_note: Cited,
    pub steps: Vec<Cited>,
    pub caution: Cited,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessMapDef {
    pub description: Cited,
    pub tips: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StageDef {
    pub id: String,
    pub label: String,
    pub questions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QualityCheckDef {
    pub id: String,
    pub label: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PdsaDef {
    pub description: Cited,
    pub stages: Vec<StageDef>,
    pub act_options: Cited,
    pub quality_checks: Vec<QualityCheckDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleDef {
    pub id: String,
    pub label: String,
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleSetDef {
    pub id: String,
    pub label: String,
    pub rules: Vec<RuleDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunChartDef {
    pub description: Cited,
    pub steps: Vec<Cited>,
    pub rule_sets: Vec<RuleSetDef>,
    pub comparison_note: Cited,
    pub signal_note: Cited,
    pub minimum_points: usize,
    pub preferred_points: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SquireItemDef {
    pub number: u8,
    pub name: String,
    pub section: String,
    pub purpose: String,
    pub ask: String,
    pub tool_section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SquireDef {
    pub url: String,
    pub license_note: String,
    pub cite_as: String,
    pub note_to_authors: Cited,
    pub items: Vec<SquireItemDef>,
}

/// `content/ebp/quality_improvement.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct QiContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub model_for_improvement: Mfi,
    pub aim: AimDef,
    pub measure_types: Vec<MeasureTypeDef>,
    pub operational_definition_fields: Vec<FieldDef>,
    pub operational_definition_note: Cited,
    pub driver_diagram: DriverDef,
    pub fishbone: FishboneDef,
    pub process_map: ProcessMapDef,
    pub pdsa: PdsaDef,
    pub run_chart: RunChartDef,
    pub squire: SquireDef,
}

/// A rule from the content by set and id.
impl QiContent {
    pub fn rule(&self, set: &str, id: &str) -> Option<&RuleDef> {
        self.run_chart
            .rule_sets
            .iter()
            .find(|s| s.id == set)?
            .rules
            .iter()
            .find(|r| r.id == id)
    }
}
