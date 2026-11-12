//! CHR-05 `asthma_home_assessment`: the CDC/EPA/HUD home asthma trigger
//! checklist for home visitors, asthma control categories (DPH's printed
//! adaptation of NHLBI's EPR-3), an asthma action plan template (NHLBI form),
//! and Massachusetts home-based asthma services.
//!
//! * Housing-code facts (105 CMR 410.000 for moisture and mold, pests, heat,
//!   and smoke and CO alarms) are not repeated: the checklist sections point at
//!   conditions in the Phase 14 housing content and the tool reads them from
//!   there at run time.
//! * The Asthma Control Test is licensed and is not reproduced.
//! * A filled action plan for a person uses template + local fill (ADR 0008):
//!   the template comes from the content; the details come from a local file
//!   and are written only to /data/output.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::env::housing::{CodeItem, HOUSING_CONDITIONS, HousingContent};
use crate::epr::content::{self, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/chronic/asthma_home.yaml`.
pub const ASTHMA_HOME: &str = "chronic-asthma-home";

/// Blank written into a plan where a value is not known.
const BLANK: &str = "____________";

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemDef {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionDef {
    pub id: String,
    pub label: String,
    /// Id of a condition in `content/env/housing_conditions.yaml`.
    pub housing: Option<String>,
    pub items: Vec<ItemDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModuleDef {
    pub id: String,
    pub label: String,
    pub how_to_read: Option<String>,
    pub sections: Vec<SectionDef>,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlossaryItem {
    pub trigger: String,
    pub found: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TriggerActions {
    pub trigger: String,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChecklistDef {
    pub title: String,
    pub how_to_use: String,
    pub glossary: Vec<GlossaryItem>,
    pub modules: Vec<ModuleDef>,
    pub epa_trigger_actions: Vec<TriggerActions>,
    pub evidence_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDef {
    pub id: String,
    pub label: String,
    pub question: String,
    pub well: String,
    pub not_well: String,
    pub very_poorly: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlDef {
    pub title: String,
    pub basis: String,
    pub variables: Vec<VariableDef>,
    pub limits: Vec<String>,
    pub act_note: String,
    pub act_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeakFlowDef {
    pub green_percent_of_best: u32,
    pub yellow_low_percent_of_best: u32,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FieldDef {
    pub field: String,
    pub label: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionPlanDef {
    pub title: String,
    pub source_note: String,
    pub peak_flow: PeakFlowDef,
    pub fields: Vec<FieldDef>,
    pub markdown: String,
    pub trigger_guidance_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramDef {
    pub id: String,
    pub name: String,
    pub location: String,
    pub areas: Vec<String>,
    pub towns_served: Vec<String>,
    pub pediatric: bool,
    pub adult: bool,
    pub older_adult: bool,
    pub url: String,
    pub url_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryDef {
    pub title: String,
    pub publisher: String,
    pub last_updated_by_dph: String,
    pub retrieved: String,
    pub url: String,
    pub note: String,
    pub programs: Vec<ProgramDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OtherResource {
    pub name: String,
    pub what: String,
    pub contact: String,
    pub url: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServicesDef {
    pub directory: DirectoryDef,
    pub model_note: String,
    pub other: Vec<OtherResource>,
    pub not_verified: Vec<String>,
}

/// `content/chronic/asthma_home.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AsthmaContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub housing_links: BTreeMap<String, String>,
    pub checklist: ChecklistDef,
    pub control: ControlDef,
    pub action_plan: ActionPlanDef,
    pub services: ServicesDef,
}

// ---------------------------------------------------------------- arguments and results

/// A part of the assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AsthmaSection {
    /// The home asthma trigger checklist (CDC, EPA, HUD).
    Checklist,
    /// Asthma control categories and the classification of the answers given.
    Control,
    /// The asthma action plan template (NHLBI) and the filled plans from a local file.
    ActionPlan,
    /// Massachusetts home-based asthma services.
    Services,
}

/// How much asthma limited usual activities in the past 12 months.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivityLimit {
    NotAtAll,
    ALittle,
    AModerateAmount,
    ALot,
}

/// Answers for the asthma control categories. No names or identifiers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ControlAnswers {
    /// Days with any asthma symptoms in the past 30 days (0 to 30).
    pub symptom_days_past_30: Option<u32>,
    /// With symptoms on all 30 days: were they there throughout the day?
    pub symptoms_throughout_the_day: Option<bool>,
    /// Nights in the past 30 days that asthma made it hard to stay asleep (0 to 30).
    pub nights_waking_past_30: Option<u32>,
    /// Rescue (short-acting) inhaler uses per day, on average (for example 3 uses a week is
    /// 0.43). Leave out with no_inhaler_medicine_past_3_months when no inhaler medicine was used.
    pub rescue_uses_per_day: Option<f64>,
    /// No prescription asthma inhaler medicine at all in the past 3 months.
    pub no_inhaler_medicine_past_3_months: Option<bool>,
    /// Limits on usual activities because of asthma in the past 12 months.
    pub activity_limitation_past_12_months: Option<ActivityLimit>,
    /// Age in years. The categories are DPH's table for ages 12 and older; under 12 the tool
    /// still classifies but says EPR-3's categories for children differ.
    pub age_years: Option<u32>,
}

/// Arguments for `asthma_home_assessment`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AsthmaHomeArgs {
    /// Parts to return: checklist, control, action_plan, services. Default: all four.
    pub sections: Vec<AsthmaSection>,
    /// Massachusetts city or town, to mark which home visiting programs list it or are located
    /// there (DPH lists regions, not towns). Default: your saved town.
    pub town: Option<String>,
    /// Answers to classify asthma control (screening level). Only counts and categories; no
    /// identifiers.
    pub control_answers: Option<ControlAnswers>,
    /// Write the blank home visit checklist and the blank asthma action plan template to
    /// /data/output (default true).
    pub write_documents: Option<bool>,
    /// Document format: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
    /// A CSV/XLSX file in /data with one row per person and the prescriber's plan details
    /// (columns such as name, doctor, doctor phone, hospital phone, best peak flow, daily
    /// medicine, quick relief medicine, puffs, oral steroid, triggers). Filled asthma action
    /// plans are written to /data/output only; nothing from the file is returned. Medicines and
    /// doses are written only if they are in the file; the server never chooses them.
    pub clients_path: Option<String>,
}

/// A checklist item as returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistSectionOut {
    pub id: String,
    pub label: String,
    /// Housing condition this section relates to (see housing_reference).
    pub housing_condition: Option<String>,
    pub items: Vec<ItemDef>,
}

/// A checklist module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistModuleOut {
    pub id: String,
    pub label: String,
    pub how_to_read: Option<String>,
    pub sections: Vec<ChecklistSectionOut>,
    pub key_points: Vec<String>,
    pub actions: Vec<String>,
}

/// The checklist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistOut {
    pub title: String,
    pub how_to_use: String,
    pub source: String,
    pub glossary: Vec<GlossaryItem>,
    pub modules: Vec<ChecklistModuleOut>,
    /// EPA's "Actions You Can Take" for each trigger.
    pub epa_trigger_actions: Vec<TriggerActions>,
    pub evidence_note: String,
}

/// A Phase 14 housing condition the checklist points at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HousingRef {
    pub condition_id: String,
    pub label: String,
    /// Why the checklist points at it.
    pub relates_to: String,
    /// True when 105 CMR 410.630(A) always deems it a condition that may endanger health.
    pub urgent: bool,
    pub urgent_note: String,
    /// The code sections, read from the housing content (not copied here).
    pub code: Vec<CodeItem>,
}

/// One control item's category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ControlItemResult {
    pub id: String,
    pub label: String,
    pub answer: String,
    /// "well controlled", "not well controlled", or "very poorly controlled".
    pub category: String,
    /// Wording of the category from DPH's table.
    pub criterion: String,
}

/// The classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ControlClassification {
    /// The most severe category among the items answered.
    pub overall: String,
    pub items: Vec<ControlItemResult>,
    pub not_answered: Vec<String>,
    pub notes: Vec<String>,
}

/// One row of DPH's table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ControlVariableOut {
    pub id: String,
    pub label: String,
    pub question: String,
    pub well_controlled: String,
    pub not_well_controlled: String,
    pub very_poorly_controlled: String,
}

/// Control section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ControlOut {
    pub title: String,
    pub basis: String,
    pub table: Vec<ControlVariableOut>,
    pub limits: Vec<String>,
    pub act_note: String,
    pub act_url: String,
    pub classification: Option<ControlClassification>,
}

/// Action plan section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActionPlanOut {
    pub title: String,
    pub source_note: String,
    pub peak_flow_note: String,
    /// Plan fields and the file columns (aliases) that fill them.
    pub fields: Vec<FieldDef>,
    /// The template with {{placeholders}}.
    pub template_markdown: String,
}

/// A program, with how it relates to the town.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramOut {
    pub name: String,
    pub location: String,
    /// Regions DPH lists as served.
    pub areas_served: Vec<String>,
    /// Towns DPH names (only Revitalize CDC's entry names towns).
    pub towns_named: Vec<String>,
    pub pediatric: bool,
    pub adult: bool,
    pub older_adult: bool,
    pub url: String,
    pub url_note: Option<String>,
    /// "names your town", "located in your town", or "regional: check coverage with the program".
    pub fit_for_town: String,
}

/// Services section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ServicesOut {
    pub directory_title: String,
    /// When DPH last updated the directory (as DPH states it).
    pub directory_updated_by_dph: String,
    pub retrieved: String,
    pub directory_url: String,
    pub town: Option<String>,
    pub programs: Vec<ProgramOut>,
    pub directory_note: String,
    pub model_note: String,
    pub other_resources: Vec<OtherResource>,
    pub not_verified: Vec<String>,
}

/// Filled action plans (counts only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlansWritten {
    pub source_file: String,
    pub folder: String,
    pub people: usize,
    pub file: String,
    /// Plan fields the file supplied.
    pub fields_found: Vec<String>,
    /// Plan fields that stayed blank (counted over all plans).
    pub blank_fields: usize,
    pub unknown_placeholders: Vec<String>,
}

/// Result of `asthma_home_assessment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AsthmaHomeResult {
    pub checklist: Option<ChecklistOut>,
    /// Phase 14 housing content the checklist points at, read at run time.
    pub housing_reference: Vec<HousingRef>,
    pub control: Option<ControlOut>,
    pub action_plan: Option<ActionPlanOut>,
    pub services: Option<ServicesOut>,
    /// Blank forms written to /data/output.
    pub documents: Vec<String>,
    pub filled_plans: Option<PlansWritten>,
    pub related_tools: Vec<String>,
}

// ---------------------------------------------------------------- control classification

fn rank(category: &str) -> u8 {
    match category {
        "well controlled" => 0,
        "not well controlled" => 1,
        _ => 2,
    }
}

fn categorize(c: &ControlDef, id: &str, category: &str) -> (String, String) {
    let v = c.variables.iter().find(|v| v.id == id);
    let crit = v
        .map(|v| match category {
            "well controlled" => v.well.clone(),
            "not well controlled" => v.not_well.clone(),
            _ => v.very_poorly.clone(),
        })
        .unwrap_or_default();
    (category.to_string(), crit)
}

fn validate_control(a: &ControlAnswers) -> Result<()> {
    if a.symptom_days_past_30.is_some_and(|d| d > 30) {
        return Err(PhnError::InvalidInput(
            "symptom_days_past_30 must be between 0 and 30".into(),
        ));
    }
    if a.nights_waking_past_30.is_some_and(|d| d > 30) {
        return Err(PhnError::InvalidInput(
            "nights_waking_past_30 must be between 0 and 30".into(),
        ));
    }
    if let Some(r) = a.rescue_uses_per_day
        && (!r.is_finite() || !(0.0..=50.0).contains(&r))
    {
        return Err(PhnError::InvalidInput(
            "rescue_uses_per_day must be between 0 and 50 uses a day".into(),
        ));
    }
    if a.age_years.is_some_and(|y| y > 120) {
        return Err(PhnError::InvalidInput(
            "age_years must be between 0 and 120".into(),
        ));
    }
    if a.no_inhaler_medicine_past_3_months == Some(true)
        && a.rescue_uses_per_day.is_some_and(|r| r > 0.0)
    {
        return Err(PhnError::InvalidInput(
            "no_inhaler_medicine_past_3_months is true but rescue_uses_per_day is above 0".into(),
        ));
    }
    Ok(())
}

/// Classify the answers with DPH's table. The category is the most severe among the items
/// answered. Where the table's ranges meet (0.29 and 1.00 rescue uses a day) the more severe
/// category is used.
pub fn classify(c: &ControlDef, a: &ControlAnswers) -> Result<ControlClassification> {
    validate_control(a)?;
    let mut items = Vec::new();
    let mut not_answered = Vec::new();
    let mut notes = Vec::new();

    match a.symptom_days_past_30 {
        Some(d) => {
            let cat = if d <= 8 {
                "well controlled"
            } else if d < 30 {
                "not well controlled"
            } else {
                match a.symptoms_throughout_the_day {
                    Some(true) => "very poorly controlled",
                    Some(false) => "not well controlled",
                    None => {
                        notes.push(
                            "Symptoms on all 30 days: say whether they last throughout the day \
                             (symptoms_throughout_the_day); not well controlled was used, and \
                             very poorly controlled applies if they do."
                                .to_string(),
                        );
                        "not well controlled"
                    }
                }
            };
            let (category, criterion) = categorize(c, "symptoms", cat);
            items.push(ControlItemResult {
                id: "symptoms".into(),
                label: "Days with asthma symptoms in the past 30 days".into(),
                answer: format!("{d} days"),
                category,
                criterion,
            });
        }
        None => not_answered.push("symptoms".to_string()),
    }
    match a.nights_waking_past_30 {
        Some(n) => {
            let cat = if n <= 2 {
                "well controlled"
            } else if n <= 12 {
                "not well controlled"
            } else {
                "very poorly controlled"
            };
            let (category, criterion) = categorize(c, "nights", cat);
            items.push(ControlItemResult {
                id: "nights".into(),
                label: "Nights asthma made it hard to stay asleep (past 30 days)".into(),
                answer: format!("{n} nights"),
                category,
                criterion,
            });
        }
        None => not_answered.push("nights".to_string()),
    }
    let saba = match (a.no_inhaler_medicine_past_3_months, a.rescue_uses_per_day) {
        (Some(true), _) => Some((
            "well controlled",
            "no asthma inhaler medicine in the past 3 months".to_string(),
        )),
        (_, Some(r)) => {
            let cat = if r < 0.29 {
                "well controlled"
            } else if r < 1.0 {
                "not well controlled"
            } else {
                "very poorly controlled"
            };
            Some((cat, format!("{r} rescue inhaler uses per day")))
        }
        _ => None,
    };
    match saba {
        Some((cat, answer)) => {
            let (category, criterion) = categorize(c, "saba", cat);
            items.push(ControlItemResult {
                id: "saba".into(),
                label: "Rescue inhaler uses per day".into(),
                answer,
                category,
                criterion,
            });
        }
        None => not_answered.push("saba".to_string()),
    }
    match a.activity_limitation_past_12_months {
        Some(l) => {
            let (cat, answer) = match l {
                ActivityLimit::NotAtAll => ("well controlled", "not at all"),
                ActivityLimit::ALittle => ("not well controlled", "a little"),
                ActivityLimit::AModerateAmount => ("not well controlled", "a moderate amount"),
                ActivityLimit::ALot => ("very poorly controlled", "a lot"),
            };
            let (category, criterion) = categorize(c, "interference", cat);
            items.push(ControlItemResult {
                id: "interference".into(),
                label: "Limits on usual activities (past 12 months)".into(),
                answer: answer.into(),
                category,
                criterion,
            });
        }
        None => not_answered.push("interference".to_string()),
    }
    if items.is_empty() {
        return Err(PhnError::InvalidInput(
            "control_answers has no answers: give at least one of symptom_days_past_30, \
             nights_waking_past_30, rescue_uses_per_day (or no_inhaler_medicine_past_3_months), \
             activity_limitation_past_12_months"
                .into(),
        ));
    }
    let overall = items
        .iter()
        .map(|i| i.category.as_str())
        .max_by_key(|c| rank(c))
        .unwrap_or("well controlled")
        .to_string();
    if a.age_years.is_some_and(|y| y < 12) {
        notes.push(
            "The person is under 12. DPH applied these ages-12-and-older cut points to children \
             in its survey; EPR-3's categories for ages 0 to 4 and 5 to 11 differ and are not \
             bundled. Treat this as a rough screen."
                .into(),
        );
    }
    if !not_answered.is_empty() {
        notes.push(format!(
            "Not answered: {}. 'Well controlled' means only that the items answered were in the \
             well-controlled range.",
            not_answered.join(", ")
        ));
    }
    notes.push(
        "Lung function and exacerbations (oral steroid courses, emergency visits, hospital \
         stays) are not part of this table: ask about them separately. This is a screening \
         classification, not a treatment step decision."
            .into(),
    );
    Ok(ControlClassification {
        overall,
        items,
        not_answered,
        notes,
    })
}

// ---------------------------------------------------------------- action plan fill

fn find_column(headers: &[String], aliases: &[String], field: &str) -> Option<usize> {
    let norm: Vec<String> = headers
        .iter()
        .map(|h| phn_core::text::normalize(h))
        .collect();
    std::iter::once(field.replace('_', " "))
        .chain(aliases.iter().cloned())
        .find_map(|a| {
            let a = phn_core::text::normalize(&a);
            norm.iter().position(|h| *h == a)
        })
}

/// Peak flow lines from the best peak flow (percentages are NHLBI's: 80 and 50).
pub fn peak_flow_lines(best: Option<&str>, p: &PeakFlowDef) -> (String, String, String) {
    let blank = (
        format!("Peak flow: more than {BLANK} ({} percent or more of my best peak flow)", p.green_percent_of_best),
        format!(
            "Peak flow: {BLANK} to {BLANK} ({} to {} percent of my best peak flow)",
            p.yellow_low_percent_of_best,
            p.green_percent_of_best - 1
        ),
        format!(
            "Peak flow: less than {BLANK} (under {} percent of my best peak flow)",
            p.yellow_low_percent_of_best
        ),
    );
    let Some(b) = best.and_then(|s| {
        s.trim()
            .trim_end_matches(|c: char| c.is_alphabetic() || c == '/' || c == ' ')
            .replace(',', "")
            .parse::<f64>()
            .ok()
    }) else {
        return blank;
    };
    if !b.is_finite() || b <= 0.0 || b > 1000.0 {
        return blank;
    }
    let green = (b * f64::from(p.green_percent_of_best) / 100.0).round() as i64;
    let red = (b * f64::from(p.yellow_low_percent_of_best) / 100.0).round() as i64;
    (
        format!(
            "Peak flow: {green} or more ({} percent or more of my best peak flow of {})",
            p.green_percent_of_best, b as i64
        ),
        format!(
            "Peak flow: {red} to {} ({} to {} percent of my best peak flow)",
            green - 1,
            p.yellow_low_percent_of_best,
            p.green_percent_of_best - 1
        ),
        format!(
            "Peak flow: less than {red} (under {} percent of my best peak flow)",
            p.yellow_low_percent_of_best
        ),
    )
}

// ---------------------------------------------------------------- documents

fn checklist_markdown(c: &ChecklistDef) -> String {
    let mut md = format!("# {}\n\n{}\n\n", c.title, c.how_to_use.trim());
    md.push_str("Visit date: ____________    Visitor: ____________\n\n");
    for m in &c.modules {
        md.push_str(&format!("## {}\n\n", m.label));
        if let Some(h) = &m.how_to_read {
            md.push_str(&format!("{}\n\n", h.trim()));
        }
        for s in &m.sections {
            md.push_str(&format!("### {}\n\n", s.label));
            for i in &s.items {
                md.push_str(&format!("- [ ] {}", i.question));
                if !i.options.is_empty() {
                    md.push_str(&format!(" Options: {}.", i.options.join("; ")));
                }
                md.push_str("\n");
                for a in &i.actions {
                    md.push_str(&format!("  - Action: {a}\n"));
                }
            }
            md.push('\n');
        }
        if !m.key_points.is_empty() {
            md.push_str("### Key points\n\n");
            for k in &m.key_points {
                md.push_str(&format!("- {k}\n"));
            }
            md.push('\n');
        }
        if !m.actions.is_empty() {
            md.push_str("### Action steps\n\n");
            for k in &m.actions {
                md.push_str(&format!("- {k}\n"));
            }
            md.push('\n');
        }
        md.push_str("Summary notes: ______________________________________________\n\n");
    }
    md.push_str(
        "Source: Home Characteristics and Asthma Triggers, Checklist for Home Visitors \
         (CDC, EPA, HUD, June 2018), shortened. Use the original for the full wording.\n",
    );
    md
}

/// The template with every placeholder blank (or the generic peak flow lines).
fn blank_plan(a: &ActionPlanDef) -> String {
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    for f in &a.fields {
        values.insert(f.field.clone(), String::new());
    }
    let (g, y, r) = peak_flow_lines(None, &a.peak_flow);
    values.insert("green_peak_flow".into(), g);
    values.insert("yellow_peak_flow".into(), y);
    values.insert("red_peak_flow".into(), r);
    templates::fill(&a.markdown, &values, BLANK).0
}

// ---------------------------------------------------------------- the tool

/// Asthma home assessment.
pub fn asthma_home_assessment(
    ctx: &Context,
    args: AsthmaHomeArgs,
) -> Result<Envelope<AsthmaHomeResult>> {
    let c: AsthmaContent = content::load(ASTHMA_HOME)?;
    let mut sections = args.sections.clone();
    if sections.is_empty() {
        sections = vec![
            AsthmaSection::Checklist,
            AsthmaSection::Control,
            AsthmaSection::ActionPlan,
            AsthmaSection::Services,
        ];
    }
    sections.sort();
    sections.dedup();
    let has = |s: AsthmaSection| sections.contains(&s);
    if args.control_answers.is_some() && !has(AsthmaSection::Control) {
        return Err(PhnError::InvalidInput(
            "control_answers needs the control section: add \"control\" to sections".into(),
        ));
    }
    if args.clients_path.is_some() && !has(AsthmaSection::ActionPlan) {
        return Err(PhnError::InvalidInput(
            "clients_path fills action plans: add \"action_plan\" to sections".into(),
        ));
    }
    let classification = match (&args.control_answers, has(AsthmaSection::Control)) {
        (Some(a), true) => Some(classify(&c.control, a)?),
        _ => None,
    };
    // Town (for services).
    let settings = ctx.settings();
    let town: Option<String> = match args.town.as_deref().map(str::trim) {
        Some(q) if !q.is_empty() => match ctx.ma.town(q) {
            Some(m) => Some(m.name.clone()),
            None => {
                let sug: Vec<String> = ctx
                    .ma
                    .suggest_towns(q, 3)
                    .into_iter()
                    .map(|m| m.name.clone())
                    .collect();
                return Err(PhnError::NotFound(format!(
                    "'{q}' is not a Massachusetts city or town{}",
                    if sug.is_empty() {
                        String::new()
                    } else {
                        format!(" (did you mean {}?)", sug.join(", "))
                    }
                )));
            }
        },
        _ => settings
            .jurisdiction
            .town
            .as_deref()
            .and_then(|t| ctx.ma.town(t))
            .map(|m| m.name.clone()),
    };

    let mut caveats: Vec<String> = Vec::new();
    let mut keys: Vec<&str> = Vec::new();
    let mut files_written: Vec<String> = Vec::new();
    let mut documents: Vec<String> = Vec::new();
    let format = args.format.unwrap_or_default();
    let write_docs = args.write_documents.unwrap_or(true);

    // Checklist and housing references (read from the Phase 14 content).
    let mut checklist_out = None;
    let mut housing_ref: Vec<HousingRef> = Vec::new();
    if has(AsthmaSection::Checklist) {
        keys.extend(["checklist", "epa-triggers", "nhlbi-2020-update"]);
        let housing: HousingContent = content::load(HOUSING_CONDITIONS)?;
        let mut wanted: Vec<String> = Vec::new();
        for m in &c.checklist.modules {
            for s in &m.sections {
                if let Some(h) = &s.housing
                    && !wanted.contains(h)
                {
                    wanted.push(h.clone());
                }
            }
        }
        for id in wanted {
            let cond = housing.conditions.iter().find(|x| x.id == id).ok_or_else(|| {
                PhnError::Internal(format!(
                    "the asthma checklist points at housing condition '{id}', which is not in the \
                     housing content"
                ))
            })?;
            housing_ref.push(HousingRef {
                condition_id: id.clone(),
                label: cond.label.clone(),
                relates_to: c.housing_links.get(&id).cloned().unwrap_or_default(),
                urgent: cond.urgent,
                urgent_note: cond.urgent_note.split_whitespace().collect::<Vec<_>>().join(" "),
                code: cond.code.clone(),
            });
        }
        caveats.extend(content::review_caveats(&[HOUSING_CONDITIONS]));
        checklist_out = Some(ChecklistOut {
            title: c.checklist.title.clone(),
            how_to_use: c.checklist.how_to_use.split_whitespace().collect::<Vec<_>>().join(" "),
            source: c.sources.get("checklist").map(|s| s.title.clone()).unwrap_or_default(),
            glossary: c.checklist.glossary.clone(),
            modules: c
                .checklist
                .modules
                .iter()
                .map(|m| ChecklistModuleOut {
                    id: m.id.clone(),
                    label: m.label.clone(),
                    how_to_read: m
                        .how_to_read
                        .as_ref()
                        .map(|h| h.split_whitespace().collect::<Vec<_>>().join(" ")),
                    sections: m
                        .sections
                        .iter()
                        .map(|s| ChecklistSectionOut {
                            id: s.id.clone(),
                            label: s.label.clone(),
                            housing_condition: s.housing.clone(),
                            items: s.items.clone(),
                        })
                        .collect(),
                    key_points: m.key_points.clone(),
                    actions: m.actions.clone(),
                })
                .collect(),
            epa_trigger_actions: c.checklist.epa_trigger_actions.clone(),
            evidence_note: c.checklist.evidence_note.split_whitespace().collect::<Vec<_>>().join(" "),
        });
        caveats.push(
            "The checklist questions and action steps are shortened from the CDC/EPA/HUD \
             checklist (June 2018); check the original before quoting. Housing code sections come \
             from the housing conditions content (housing_conditions_health_guide) and are \
             decision support, not legal advice."
                .into(),
        );
    }

    // Control.
    let control_out = if has(AsthmaSection::Control) {
        keys.push("dph-asthma-mgmt");
        keys.push("nhlbi-2020-update");
        caveats.push(
            "Control categories are DPH's printed adaptation of NHLBI's EPR-3 for a population \
             survey (30-day recall, ages 12 and older cut points). EPR-3 and GINA were not \
             reachable when this was built, so the clinical grids are not bundled. The Asthma \
             Control Test is licensed (QualityMetric) and is not reproduced."
                .into(),
        );
        Some(ControlOut {
            title: c.control.title.clone(),
            basis: c.control.basis.split_whitespace().collect::<Vec<_>>().join(" "),
            table: c
                .control
                .variables
                .iter()
                .map(|v| ControlVariableOut {
                    id: v.id.clone(),
                    label: v.label.clone(),
                    question: v.question.clone(),
                    well_controlled: v.well.clone(),
                    not_well_controlled: v.not_well.clone(),
                    very_poorly_controlled: v.very_poorly.clone(),
                })
                .collect(),
            limits: c.control.limits.clone(),
            act_note: c.control.act_note.split_whitespace().collect::<Vec<_>>().join(" "),
            act_url: c.control.act_url.clone(),
            classification: classification.clone(),
        })
    } else {
        None
    };

    // Action plan: template and local fill.
    let mut identifiers = Vec::new();
    let mut local_source: Option<Source> = None;
    let mut plans_written = None;
    let plan_out = if has(AsthmaSection::ActionPlan) {
        keys.extend(["nhlbi-plan", "epa-plan-page"]);
        caveats.push(
            "The asthma action plan is completed with the person's prescriber. This server never \
             chooses medicines or doses: they appear in a filled plan only if they are in the \
             local file you supply, and the plan should be checked by the prescriber before it is \
             given out."
                .into(),
        );
        if let Some(p) = &args.clients_path {
            let lt = crate::localdata::load(ctx, p, None)?;
            identifiers = lt.identifier_categories();
            let headers = &lt.table.headers;
            let cols: Vec<(&FieldDef, Option<usize>)> = c
                .action_plan
                .fields
                .iter()
                .map(|f| (f, find_column(headers, &f.aliases, &f.field)))
                .collect();
            let found: Vec<String> = cols
                .iter()
                .filter(|(_, i)| i.is_some())
                .map(|(f, _)| f.field.clone())
                .collect();
            if found.is_empty() {
                return Err(PhnError::InvalidInput(format!(
                    "'{}' has no column the asthma action plan can use (for example name, \
                     doctor, doctor phone, hospital phone, best peak flow, daily medicine, \
                     quick relief medicine, quick relief puffs, oral steroid, triggers); it has \
                     {} column(s) with other names",
                    lt.label,
                    headers.len()
                )));
            }
            let (dir, shown) = templates::output_folder(ctx, "asthma_action_plans")?;
            let today = chrono::Local::now().date_naive().to_string();
            let mut docs = Vec::new();
            let mut blank_fields = 0usize;
            let mut unknown: std::collections::BTreeSet<String> = Default::default();
            for row in &lt.table.rows {
                if row.iter().all(|v| v.trim().is_empty()) {
                    continue;
                }
                let mut v: BTreeMap<String, String> = BTreeMap::new();
                for (f, i) in &cols {
                    let val = i
                        .and_then(|i| row.get(i))
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    if val.is_empty() {
                        blank_fields += 1;
                    }
                    v.insert(f.field.clone(), val);
                }
                if v.get("plan_date").is_none_or(|d| d.is_empty()) {
                    v.insert("plan_date".into(), today.clone());
                }
                let (g, y, r) =
                    peak_flow_lines(v.get("best_peak_flow").map(String::as_str), &c.action_plan.peak_flow);
                v.insert("green_peak_flow".into(), g);
                v.insert("yellow_peak_flow".into(), y);
                v.insert("red_peak_flow".into(), r);
                let (filled, unk) = templates::fill(&c.action_plan.markdown, &v, BLANK);
                unknown.extend(unk);
                docs.push(templates::document(ctx, &filled, "en-US", &c.action_plan.title));
            }
            if docs.is_empty() {
                return Err(PhnError::InvalidInput(format!(
                    "'{}' has no rows with data",
                    lt.label
                )));
            }
            let people = docs.len();
            let combined = templates::combine("Asthma action plans", "en-US", docs, ctx);
            let file = templates::write_in(
                ctx,
                &dir,
                &format!("asthma_action_plans.{}", format.ext()),
                &format.render(&combined)?,
            )?;
            files_written.push(file.clone());
            plans_written = Some(PlansWritten {
                source_file: lt.label.clone(),
                folder: shown,
                people,
                file,
                fields_found: found,
                blank_fields,
                unknown_placeholders: unknown.into_iter().collect(),
            });
            local_source = Some(lt.source());
        }
        Some(ActionPlanOut {
            title: c.action_plan.title.clone(),
            source_note: c.action_plan.source_note.split_whitespace().collect::<Vec<_>>().join(" "),
            peak_flow_note: c.action_plan.peak_flow.note.clone(),
            fields: c.action_plan.fields.clone(),
            template_markdown: c.action_plan.markdown.clone(),
        })
    } else {
        None
    };

    // Services.
    let services_out = if has(AsthmaSection::Services) {
        keys.extend(["dph-chw-visiting", "dph-asthma-programs", "dph-asthma-resources", "dph-apcp"]);
        let norm = |s: &str| phn_core::text::normalize(s);
        let mut programs: Vec<(u8, ProgramOut)> = c
            .services
            .directory
            .programs
            .iter()
            .map(|p| {
                let (rankv, fit) = match &town {
                    Some(t)
                        if p.towns_served.iter().any(|x| norm(x) == norm(t)) =>
                    {
                        (0, "names your town".to_string())
                    }
                    Some(t)
                        if norm(&p.location) == norm(t)
                            || (norm(t) == "boston" && norm(&p.location) == "east boston") =>
                    {
                        (1, "located in your town".to_string())
                    }
                    _ => (2, "regional: check coverage with the program".to_string()),
                };
                (
                    rankv,
                    ProgramOut {
                        name: p.name.clone(),
                        location: p.location.clone(),
                        areas_served: p.areas.clone(),
                        towns_named: p.towns_served.clone(),
                        pediatric: p.pediatric,
                        adult: p.adult,
                        older_adult: p.older_adult,
                        url: p.url.clone(),
                        url_note: p.url_note.clone(),
                        fit_for_town: fit,
                    },
                )
            })
            .collect();
        programs.sort_by_key(|(r, _)| *r);
        caveats.push(format!(
            "The home visiting directory is DPH's, last updated by DPH in {}; programs change, \
             DPH lists regions rather than towns, and eligibility varies by program. Confirm \
             with the program before referring.",
            c.services.directory.last_updated_by_dph
        ));
        if town.is_none() {
            caveats.push(
                "No town was given or saved, so programs are not matched to a town.".into(),
            );
        }
        Some(ServicesOut {
            directory_title: c.services.directory.title.clone(),
            directory_updated_by_dph: c.services.directory.last_updated_by_dph.clone(),
            retrieved: c.services.directory.retrieved.clone(),
            directory_url: c.services.directory.url.clone(),
            town: town.clone(),
            programs: programs.into_iter().map(|(_, p)| p).collect(),
            directory_note: c.services.directory.note.split_whitespace().collect::<Vec<_>>().join(" "),
            model_note: c.services.model_note.split_whitespace().collect::<Vec<_>>().join(" "),
            other_resources: c.services.other.clone(),
            not_verified: c.services.not_verified.clone(),
        })
    } else {
        None
    };

    // Blank forms.
    if write_docs && (has(AsthmaSection::Checklist) || has(AsthmaSection::ActionPlan)) {
        let (dir, _) = templates::output_folder(ctx, "asthma_home_forms")?;
        if has(AsthmaSection::Checklist) {
            let doc = templates::document(
                ctx,
                &checklist_markdown(&c.checklist),
                "en-US",
                &c.checklist.title,
            );
            documents.push(templates::write_in(
                ctx,
                &dir,
                &format!("asthma_home_visit_checklist.{}", format.ext()),
                &format.render(&doc)?,
            )?);
        }
        if has(AsthmaSection::ActionPlan) {
            let doc = templates::document(
                ctx,
                &blank_plan(&c.action_plan),
                "en-US",
                &c.action_plan.title,
            );
            documents.push(templates::write_in(
                ctx,
                &dir,
                &format!("asthma_action_plan_template.{}", format.ext()),
                &format.render(&doc)?,
            )?);
        }
        files_written.extend(documents.iter().cloned());
    }

    caveats.extend(content::review_caveats(&[ASTHMA_HOME]));
    let mut sources = content::sources_for(keys, &c.sources);
    if let Some(ls) = local_source {
        sources.push(ls);
    }
    let s = summary(
        &checklist_out,
        &housing_ref,
        &control_out,
        &plan_out,
        &services_out,
        &documents,
        &plans_written,
    );
    let mut notes = Vec::new();
    if plans_written.is_some() {
        notes.push(
            "Details from your file were written into the action plans in /data/output only; \
             the response has counts and the file name only."
                .into(),
        );
    }
    if args.control_answers.is_some() {
        notes.push(
            "Control answers are counts and categories typed in the request; do not include \
             names."
                .into(),
        );
    }
    Ok(Envelope::new(
        s,
        AsthmaHomeResult {
            checklist: checklist_out,
            housing_reference: housing_ref,
            control: control_out,
            action_plan: plan_out,
            services: services_out,
            documents,
            filled_plans: plans_written,
            related_tools: vec![
                "housing_conditions_health_guide (105 CMR 410 sections, inspection requests)".into(),
                "air_quality_protective_guidance (outdoor air and asthma)".into(),
            ],
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: identifiers,
        files_written,
        notes,
        ..Privacy::none()
    })
    .decision_support())
}

fn summary(
    checklist: &Option<ChecklistOut>,
    housing: &[HousingRef],
    control: &Option<ControlOut>,
    plan: &Option<ActionPlanOut>,
    services: &Option<ServicesOut>,
    documents: &[String],
    plans: &Option<PlansWritten>,
) -> String {
    let mut s = String::from("**Asthma home assessment**\n\n");
    if let Some(c) = checklist {
        let items: usize = c
            .modules
            .iter()
            .flat_map(|m| &m.sections)
            .map(|x| x.items.len())
            .sum();
        s.push_str(&format!(
            "- **Home trigger checklist** (CDC, EPA, HUD): {} modules ({}), {items} items with action steps.\n",
            c.modules.len(),
            c.modules.iter().map(|m| m.label.as_str()).collect::<Vec<_>>().join("; "),
        ));
        if !housing.is_empty() {
            s.push_str(&format!(
                "  - Housing code (from housing_conditions_health_guide content): {}.\n",
                housing
                    .iter()
                    .map(|h| format!("{} ({})", h.label, h.code.iter().map(|x| x.section.as_str()).take(3).collect::<Vec<_>>().join(", ")))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    if let Some(c) = control {
        s.push_str("- **Asthma control categories** (DPH's table, adapted from NHLBI EPR-3; screening level):\n\n");
        let rows: Vec<Vec<String>> = c
            .table
            .iter()
            .map(|v| {
                vec![
                    v.label.clone(),
                    v.well_controlled.clone(),
                    v.not_well_controlled.clone(),
                    v.very_poorly_controlled.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(&["Item", "Well controlled", "Not well controlled", "Very poorly controlled"], &rows));
        if let Some(k) = &c.classification {
            s.push_str(&format!("\n  Classification of the answers given: **{}**", k.overall));
            for i in &k.items {
                s.push_str(&format!("; {}: {} ({})", i.id, i.category, i.answer));
            }
            s.push_str(".\n");
        }
    }
    if let Some(p) = plan {
        s.push_str(&format!(
            "- **{}** template (NHLBI): green, yellow, and red zones; {} fields to fill.\n",
            p.title,
            p.fields.len()
        ));
    }
    if let Some(p) = plans {
        s.push_str(&format!(
            "  - {} filled action plan(s) written to {} (details are not shown).\n",
            p.people, p.file
        ));
    }
    if let Some(v) = services {
        s.push_str(&format!(
            "- **Massachusetts home-based asthma services**: {} programs in DPH's directory (updated by DPH {}){}.\n",
            v.programs.len(),
            v.directory_updated_by_dph,
            match &v.town {
                Some(t) => format!(
                    "; {} name or are located in {t}",
                    v.programs.iter().filter(|p| !p.fit_for_town.starts_with("regional")).count()
                ),
                None => String::new(),
            }
        ));
    }
    for d in documents {
        s.push_str(&format!("- Blank form written to {d}.\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> AsthmaContent {
        content::load(ASTHMA_HOME).unwrap()
    }

    #[test]
    fn content_loads_and_points_at_real_housing_conditions() {
        let c = content();
        let housing: HousingContent = content::load(HOUSING_CONDITIONS).unwrap();
        let mut n = 0;
        for m in &c.checklist.modules {
            for s in &m.sections {
                if let Some(h) = &s.housing {
                    n += 1;
                    assert!(housing.conditions.iter().any(|x| &x.id == h), "{h}");
                    assert!(c.housing_links.contains_key(h), "{h}");
                }
            }
        }
        assert!(n >= 4);
        // The checklist does not copy housing code text.
        let yaml = phn_content::ITEMS
            .iter()
            .find(|i| i.id == ASTHMA_HOME)
            .unwrap()
            .yaml;
        assert!(!yaml.contains("410.550(A)"));
    }

    #[test]
    fn ids_are_unique_and_every_item_has_a_question() {
        let c = content();
        let mut ids = std::collections::BTreeSet::new();
        for m in &c.checklist.modules {
            for s in &m.sections {
                for i in &s.items {
                    assert!(ids.insert(i.id.clone()), "{}", i.id);
                    assert!(!i.question.trim().is_empty());
                }
            }
        }
        // No Asthma Control Test wording: its questions and answer scale are not bundled.
        let yaml = phn_content::ITEMS
            .iter()
            .find(|i| i.id == ASTHMA_HOME)
            .unwrap()
            .yaml
            .to_lowercase();
        assert!(!yaml.contains("how much of the time did your asthma keep you from getting as much done"));
    }

    #[test]
    fn plan_template_placeholders_are_all_fields_or_peak_flow_lines() {
        let c = content();
        let (_, unknown) = {
            let mut v: BTreeMap<String, String> = c
                .action_plan
                .fields
                .iter()
                .map(|f| (f.field.clone(), String::new()))
                .collect();
            for k in ["green_peak_flow", "yellow_peak_flow", "red_peak_flow"] {
                v.insert(k.into(), String::new());
            }
            templates::fill(&c.action_plan.markdown, &v, BLANK)
        };
        assert!(unknown.is_empty(), "{unknown:?}");
    }

    #[test]
    fn classification_uses_dph_cut_points() {
        let c = content().control;
        let cat = |a: ControlAnswers| classify(&c, &a).unwrap().overall;
        // Symptom days: 8 is the last well-controlled value, 9 is not well controlled.
        assert_eq!(
            cat(ControlAnswers { symptom_days_past_30: Some(8), ..Default::default() }),
            "well controlled"
        );
        assert_eq!(
            cat(ControlAnswers { symptom_days_past_30: Some(9), ..Default::default() }),
            "not well controlled"
        );
        assert_eq!(
            cat(ControlAnswers {
                symptom_days_past_30: Some(30),
                symptoms_throughout_the_day: Some(true),
                ..Default::default()
            }),
            "very poorly controlled"
        );
        assert_eq!(
            cat(ControlAnswers {
                symptom_days_past_30: Some(30),
                symptoms_throughout_the_day: Some(false),
                ..Default::default()
            }),
            "not well controlled"
        );
        // Nights: 2, 3, 12, 13.
        let nights = |n| cat(ControlAnswers { nights_waking_past_30: Some(n), ..Default::default() });
        assert_eq!(nights(2), "well controlled");
        assert_eq!(nights(3), "not well controlled");
        assert_eq!(nights(12), "not well controlled");
        assert_eq!(nights(13), "very poorly controlled");
        // Rescue use; the table's ranges meet at 0.29 and 1.00 and the more severe is used.
        let saba = |r| cat(ControlAnswers { rescue_uses_per_day: Some(r), ..Default::default() });
        assert_eq!(saba(0.28), "well controlled");
        assert_eq!(saba(0.29), "not well controlled");
        assert_eq!(saba(0.99), "not well controlled");
        assert_eq!(saba(1.0), "very poorly controlled");
        assert_eq!(
            cat(ControlAnswers {
                no_inhaler_medicine_past_3_months: Some(true),
                ..Default::default()
            }),
            "well controlled"
        );
        // Activity limits.
        assert_eq!(
            cat(ControlAnswers {
                activity_limitation_past_12_months: Some(ActivityLimit::AModerateAmount),
                ..Default::default()
            }),
            "not well controlled"
        );
        // The most severe item decides.
        let k = classify(
            &c,
            &ControlAnswers {
                symptom_days_past_30: Some(2),
                nights_waking_past_30: Some(14),
                rescue_uses_per_day: Some(0.1),
                activity_limitation_past_12_months: Some(ActivityLimit::NotAtAll),
                age_years: Some(8),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(k.overall, "very poorly controlled");
        assert_eq!(k.items.len(), 4);
        assert!(k.notes.iter().any(|n| n.contains("under 12")));
    }

    #[test]
    fn classification_rejects_bad_input() {
        let c = content().control;
        assert!(classify(&c, &ControlAnswers::default()).is_err());
        assert!(classify(&c, &ControlAnswers { symptom_days_past_30: Some(31), ..Default::default() }).is_err());
        assert!(classify(&c, &ControlAnswers { nights_waking_past_30: Some(40), ..Default::default() }).is_err());
        assert!(classify(&c, &ControlAnswers { rescue_uses_per_day: Some(f64::NAN), ..Default::default() }).is_err());
        assert!(classify(
            &c,
            &ControlAnswers {
                no_inhaler_medicine_past_3_months: Some(true),
                rescue_uses_per_day: Some(2.0),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn peak_flow_zones_use_nhlbi_percentages() {
        let p = content().action_plan.peak_flow;
        let (g, y, r) = peak_flow_lines(Some("400"), &p);
        assert!(g.contains("320 or more"), "{g}");
        assert!(y.contains("200 to 319"), "{y}");
        assert!(r.contains("less than 200"), "{r}");
        // Units and commas are tolerated; nonsense and out-of-range values leave blanks.
        let (g, _, _) = peak_flow_lines(Some("450 L/min"), &p);
        assert!(g.contains("360 or more"), "{g}");
        for bad in [None, Some(""), Some("abc"), Some("-5"), Some("0"), Some("5000")] {
            let (g, _, _) = peak_flow_lines(bad, &p);
            assert!(g.contains(BLANK), "{bad:?}");
        }
    }

    #[test]
    fn blank_plan_has_blank_lines_and_no_medicines() {
        let c = content();
        let md = blank_plan(&c.action_plan);
        assert!(md.contains(BLANK));
        assert!(!md.contains("{{"));
        for drug in ["albuterol", "prednisone", "budesonide", "fluticasone"] {
            assert!(!md.to_lowercase().contains(drug));
        }
    }

    #[test]
    fn directory_matches_what_dph_lists() {
        let c = content().services.directory;
        assert_eq!(c.programs.len(), 7);
        let rev = c.programs.iter().find(|p| p.id == "revitalize_cdc").unwrap();
        assert_eq!(rev.towns_served, vec!["Springfield", "Holyoke", "Chicopee"]);
        assert!(rev.pediatric && rev.adult && rev.older_adult);
        let bch = c.programs.iter().find(|p| p.id == "boston_childrens").unwrap();
        assert!(bch.pediatric && !bch.adult);
    }
}
