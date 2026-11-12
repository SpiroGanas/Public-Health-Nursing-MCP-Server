//! PLAN-08 `draft_grant_narrative`: a needs statement, project narrative, and sustainability section
//! drafted from templates and the facts the nurse supplies, organized by the funder's scoring
//! criteria.
//!
//! Template plus local fill (ADR 0008): the server never writes a statistic or a program fact. Every
//! number comes from a `data_points` entry (shown with its geography, source, and year), every
//! program fact comes from `facts`, and anything missing is a `[NURSE: ...]` placeholder. The
//! draft is written to /data/output as DOCX and is also returned, because it holds only what the
//! nurse passed in and this project's template wording.

use std::collections::{BTreeMap, BTreeSet};

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_local::phi::{self, Identifier};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::funding_announcement::{CriterionType, criterion_types, type_for};
use crate::context::Context;
use crate::epr::content;
use crate::md;

/// Content id of `content/planning/narrative_templates.yaml`.
pub const TEMPLATES: &str = "planning-narrative-templates";
const MAX_CRITERIA: usize = 30;
const MAX_DATA_POINTS: usize = 40;
const MAX_LIST_ITEMS: usize = 15;
const MAX_FIELD_CHARS: usize = 4000;
const MAX_LABEL_CHARS: usize = 120;

/// A section the tool can draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeSection {
    /// The health problem, who is affected, cited data, and the service gap.
    NeedsStatement,
    /// Goal, objectives, activities, timeline, staffing, partners, and evaluation.
    ProjectNarrative,
    /// How the work continues after the award.
    Sustainability,
}

impl NarrativeSection {
    fn id(self) -> &'static str {
        match self {
            Self::NeedsStatement => "needs_statement",
            Self::ProjectNarrative => "project_narrative",
            Self::Sustainability => "sustainability",
        }
    }
    fn from_id(s: &str) -> Option<Self> {
        match s {
            "needs_statement" => Some(Self::NeedsStatement),
            "project_narrative" => Some(Self::ProjectNarrative),
            "sustainability" => Some(Self::Sustainability),
            _ => None,
        }
    }
}

/// A scoring criterion from the announcement.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CriterionInput {
    /// The criterion's name as the announcement words it (a short label).
    pub label: String,
    /// Its points, when the announcement gives them.
    pub points: Option<f64>,
    /// Which section answers it, to override the automatic choice: needs_statement,
    /// project_narrative, or sustainability.
    pub section: Option<NarrativeSection>,
}

/// A statistic you supply, with where it came from. The tool formats it; it never changes the value.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DataPoint {
    /// What was measured, for example "Adults with diabetes".
    pub label: String,
    /// The value exactly as you want it shown, for example "11.4%" or "1,203 residents".
    pub value: String,
    /// The place it describes, for example "Springfield" or "Hampden County". Default: your community.
    pub geography: Option<String>,
    /// Where it came from, for example "CDC PLACES" or "MA DPH Community Health Data". Required for a citation.
    pub source: Option<String>,
    /// The year or date of the data, for example "2023". Required for a citation.
    pub as_of: Option<String>,
    /// An optional comparison you want shown, for example "state: 9.1%".
    pub comparison: Option<String>,
}

/// Program facts you supply; every field is optional and nothing is invented for a missing one.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ProgramFacts {
    /// The health problem in a sentence or two.
    pub health_problem: Option<String>,
    /// Who is most affected.
    pub population_served: Option<String>,
    /// Which groups carry the greatest burden (only if your data show it).
    pub disparities: Option<String>,
    /// Services that exist now.
    pub current_services: Option<String>,
    /// The gap this project fills.
    pub gap: Option<String>,
    /// The project goal.
    pub goal: Option<String>,
    /// Objectives (see write_smartie_objectives), one per entry.
    pub objectives: Vec<String>,
    /// Activities, one per entry.
    pub activities: Vec<String>,
    /// The project period and order of work.
    pub timeline: Option<String>,
    /// The evidence for the approach, with source and year.
    pub evidence: Option<String>,
    /// Roles and time on the project.
    pub staffing: Option<String>,
    /// Partner organizations and what each does, one per entry.
    pub partners: Vec<String>,
    /// The evaluation approach.
    pub evaluation: Option<String>,
    /// Your agency's relevant experience and systems.
    pub organizational_capacity: Option<String>,
    /// Funding after the award.
    pub sustainability_funding: Option<String>,
    /// Policy or system changes that keep the work going.
    pub sustainability_policy: Option<String>,
    /// Partners who continue parts of the work.
    pub sustainability_partners: Option<String>,
    /// In-kind and local support.
    pub in_kind: Option<String>,
    /// How evaluation results will be used.
    pub results_use: Option<String>,
}

/// Arguments for `draft_grant_narrative`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct NarrativeArgs {
    /// The project's name.
    pub project_title: Option<String>,
    /// The funder or program, for example "CDC Public Health Infrastructure NOFO".
    pub funder: Option<String>,
    /// Your agency's name. Default: the agency name in your letterhead settings.
    pub applicant: Option<String>,
    /// The community the project serves, for example "Amherst". Default: your saved town.
    pub community: Option<String>,
    /// Sections to draft: needs_statement, project_narrative, sustainability. Default: all three.
    pub sections: Option<Vec<NarrativeSection>>,
    /// Scoring criteria, typed in (label and points). The draft is organized to answer them.
    pub criteria: Option<Vec<CriterionInput>>,
    /// Instead of typing criteria: the XLSX checklist that `analyze_funding_announcement` wrote
    /// (for example "output/funding_checklist_my-nofo.xlsx"); its "Scoring criteria" sheet is read.
    pub criteria_file: Option<String>,
    /// Statistics you supply, each with a source and year. The needs statement cites only these.
    pub data_points: Option<Vec<DataPoint>>,
    /// Program facts you supply.
    pub facts: Option<ProgramFacts>,
    /// The announcement's page limit for the narrative, as a reminder in the draft.
    pub page_limit: Option<u32>,
    /// File name stem for the DOCX (default: from the project title).
    pub output_name: Option<String>,
    /// Write the DOCX to /data/output (default true).
    pub write_document: Option<bool>,
}

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct SlotDef {
    id: String,
    heading: String,
    fact: Option<String>,
    kind: String,
    placeholder: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SectionDef {
    id: String,
    heading: String,
    purpose: String,
    slots: Vec<SlotDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct Templates {
    sections: Vec<SectionDef>,
    criterion_prompts: BTreeMap<String, Vec<String>>,
}

// ---------------------------------------------------------------- output

/// A criterion and where it is answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CriterionOut {
    pub label: String,
    pub points: Option<f64>,
    /// The criterion type this project assigned (need, approach, evaluation, ...).
    pub criterion_type: Option<String>,
    /// The section that answers it, or "other" when it is not drafted here.
    pub answered_in: String,
}

/// One part of a section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SlotOut {
    pub heading: String,
    /// True when your facts filled it; false when it is a placeholder.
    pub filled: bool,
}

/// One drafted section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionDraft {
    pub id: String,
    pub heading: String,
    /// The scoring criteria this section answers, with points.
    pub criteria: Vec<String>,
    pub slots: Vec<SlotOut>,
    /// The draft text (Markdown). Placeholders look like [NURSE: ...].
    pub text: String,
    pub placeholders: usize,
    pub words: usize,
}

/// Result of `draft_grant_narrative`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NarrativeResult {
    pub project_title: Option<String>,
    pub funder: Option<String>,
    /// typed, checklist_file, or none (a generic skeleton).
    pub criteria_from: String,
    pub criteria: Vec<CriterionOut>,
    /// Criteria not drafted here (budget, forms), with where to handle them.
    pub criteria_not_drafted: Vec<String>,
    pub sections: Vec<SectionDraft>,
    pub data_points_used: usize,
    /// Data points left out of the draft because they lack a source or year are still shown with a
    /// placeholder; this counts them.
    pub data_points_missing_citation: usize,
    pub placeholder_count: usize,
    pub words: usize,
    pub page_limit_note: Option<String>,
    /// The DOCX written to /data/output.
    pub files: Vec<String>,
}

// ---------------------------------------------------------------- input checks

fn too_long(field: &str, s: &str) -> Result<()> {
    if s.chars().count() > MAX_FIELD_CHARS {
        return Err(PhnError::InvalidInput(format!(
            "{field} is longer than {MAX_FIELD_CHARS} characters: summarize it"
        )));
    }
    Ok(())
}

/// Refuse client identifiers; the narrative needs community-level facts only.
fn check_text(field: &str, s: &str) -> Result<()> {
    too_long(field, s)?;
    let bad = phi::scan_text(s).iter().any(|f| {
        f.confidence == phi::Confidence::High
            && matches!(
                f.kind,
                Identifier::Ssn
                    | Identifier::MedicalRecordNumber
                    | Identifier::HealthPlanNumber
                    | Identifier::AccountNumber
                    | Identifier::BirthDate
            )
    });
    if bad {
        return Err(PhnError::Privacy(format!(
            "{field} contains what looks like a client identifier (a record or social security number, or a date of birth). A grant narrative uses community-level facts only: remove it."
        )));
    }
    Ok(())
}

fn opt(field: &str, v: &Option<String>) -> Result<Option<String>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            check_text(field, s)?;
            Ok(Some(s.to_string()))
        }
        None => Ok(None),
    }
}

fn list(field: &str, v: &[String]) -> Result<Vec<String>> {
    if v.len() > MAX_LIST_ITEMS {
        return Err(PhnError::InvalidInput(format!(
            "{field} has more than {MAX_LIST_ITEMS} entries: group them"
        )));
    }
    let mut out = Vec::new();
    for s in v {
        let t = s.trim();
        if !t.is_empty() {
            check_text(field, t)?;
            out.push(t.to_string());
        }
    }
    Ok(out)
}

fn clip(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        s
    } else {
        let cut: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", cut.trim_end())
    }
}

fn read_criteria_file(ctx: &Context, path: &str) -> Result<Vec<CriterionInput>> {
    let sandbox = ctx.sandbox()?;
    let file = sandbox.resolve_existing(path)?;
    let label = sandbox.display_path(&file);
    if !matches!(
        phn_local::extract::FileKind::of(&file),
        phn_local::extract::FileKind::Spreadsheet
    ) {
        return Err(PhnError::Unsupported(format!(
            "'{label}' is not a spreadsheet: criteria_file is the XLSX checklist written by analyze_funding_announcement"
        )));
    }
    let tables = phn_local::table::read_tables(&file)?;
    let t = tables
        .iter()
        .find(|t| t.name.to_lowercase().contains("scoring"))
        .ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{label}' has no \"Scoring criteria\" sheet: use the checklist written by analyze_funding_announcement, or type the criteria"
            ))
        })?;
    let col = |name: &str| t.headers.iter().position(|h| h.to_lowercase().contains(name));
    let (Some(lc), pc) = (col("criterion"), col("points")) else {
        return Err(PhnError::InvalidInput(format!(
            "the \"Scoring criteria\" sheet in '{label}' has no Criterion column"
        )));
    };
    let sc = col("narrative");
    let mut out = Vec::new();
    for r in &t.rows {
        let l = r.get(lc).map(|s| s.trim()).unwrap_or("");
        if l.is_empty() {
            continue;
        }
        out.push(CriterionInput {
            label: clip(l, MAX_LABEL_CHARS),
            points: pc.and_then(|c| r.get(c)).and_then(|p| p.trim().parse::<f64>().ok()).filter(|p| (0.0..=1000.0).contains(p)),
            section: sc.and_then(|c| r.get(c)).and_then(|s| NarrativeSection::from_id(s.trim())),
        });
    }
    if out.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "the \"Scoring criteria\" sheet in '{label}' has no rows"
        )));
    }
    Ok(out)
}

// ---------------------------------------------------------------- drafting

fn placeholder(text: &str, community: &str) -> String {
    format!("[NURSE: {}]", text.replace("{community}", community))
}

fn data_line(d: &DataPoint, community: &str, missing: &mut usize) -> (String, Option<String>) {
    let geo = d.geography.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(community);
    let src = d.source.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let when = d.as_of.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if src.is_none() || when.is_none() {
        *missing += 1;
    }
    let mut cite = Vec::new();
    cite.push(geo.to_string());
    cite.push(match (src, when) {
        (Some(s), Some(w)) => format!("{s}, {w}"),
        (Some(s), None) => format!("{s}, [NURSE: add the year]"),
        (None, Some(w)) => format!("[NURSE: add the source], {w}"),
        (None, None) => "[NURSE: add the source and year]".into(),
    });
    let cmp = d
        .comparison
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|c| format!("; comparison: {c}"))
        .unwrap_or_default();
    let line = format!("**{}:** {} ({}){}", d.label.trim(), d.value.trim(), cite.join("; "), cmp);
    let reference = match (src, when) {
        (Some(s), Some(w)) => Some(format!("{s}, {w}")),
        _ => None,
    };
    (line, reference)
}

fn count_placeholders(text: &str) -> usize {
    text.matches("[NURSE:").count()
}

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

struct Facts {
    map: BTreeMap<&'static str, String>,
    lists: BTreeMap<&'static str, Vec<String>>,
}

fn facts_of(f: &ProgramFacts) -> Result<Facts> {
    let mut map = BTreeMap::new();
    let mut put = |key: &'static str, v: &Option<String>| -> Result<()> {
        if let Some(s) = opt(key, v)? {
            map.insert(key, s);
        }
        Ok(())
    };
    put("health_problem", &f.health_problem)?;
    put("population_served", &f.population_served)?;
    put("disparities", &f.disparities)?;
    put("current_services", &f.current_services)?;
    put("gap", &f.gap)?;
    put("goal", &f.goal)?;
    put("timeline", &f.timeline)?;
    put("evidence", &f.evidence)?;
    put("staffing", &f.staffing)?;
    put("evaluation", &f.evaluation)?;
    put("organizational_capacity", &f.organizational_capacity)?;
    put("sustainability_funding", &f.sustainability_funding)?;
    put("sustainability_policy", &f.sustainability_policy)?;
    put("sustainability_partners", &f.sustainability_partners)?;
    put("in_kind", &f.in_kind)?;
    put("results_use", &f.results_use)?;
    let mut lists = BTreeMap::new();
    lists.insert("objectives", list("objectives", &f.objectives)?);
    lists.insert("activities", list("activities", &f.activities)?);
    lists.insert("partners", list("partners", &f.partners)?);
    Ok(Facts { map, lists })
}

/// Draft a grant narrative.
pub fn draft_grant_narrative(ctx: &Context, args: NarrativeArgs) -> Result<Envelope<NarrativeResult>> {
    let templates: Templates = content::load(TEMPLATES)?;
    let types: Vec<CriterionType> = criterion_types()?;

    // Inputs.
    let project_title = opt("project_title", &args.project_title)?;
    let funder = opt("funder", &args.funder)?;
    let settings = ctx.settings();
    let applicant = opt("applicant", &args.applicant)?
        .or_else(|| settings.letterhead.as_ref().map(|l| l.agency_name.clone()));
    let community = opt("community", &args.community)?
        .or_else(|| settings.jurisdiction.town.clone())
        .unwrap_or_else(|| "your community".to_string());
    let facts = facts_of(&args.facts.clone().unwrap_or_default())?;
    let mut sections: Vec<NarrativeSection> = args
        .sections
        .clone()
        .unwrap_or_else(|| vec![NarrativeSection::NeedsStatement, NarrativeSection::ProjectNarrative, NarrativeSection::Sustainability]);
    sections.sort();
    sections.dedup();
    if sections.is_empty() {
        return Err(PhnError::InvalidInput("sections is empty: name at least one of needs_statement, project_narrative, sustainability".into()));
    }
    let data_points = args.data_points.clone().unwrap_or_default();
    if data_points.len() > MAX_DATA_POINTS {
        return Err(PhnError::InvalidInput(format!("at most {MAX_DATA_POINTS} data_points")));
    }
    for d in &data_points {
        if d.label.trim().is_empty() || d.value.trim().is_empty() {
            return Err(PhnError::InvalidInput("every data point needs a label and a value".into()));
        }
        for (f, v) in [("data_points label", d.label.as_str()), ("data_points value", d.value.as_str())] {
            check_text(f, v)?;
        }
        for (f, v) in [("data_points geography", &d.geography), ("data_points source", &d.source), ("data_points as_of", &d.as_of), ("data_points comparison", &d.comparison)] {
            opt(f, v)?;
        }
    }

    // Criteria: typed, or from the checklist file.
    let (inputs, criteria_from): (Vec<CriterionInput>, &str) = match (&args.criteria, &args.criteria_file) {
        (Some(c), _) if !c.is_empty() => (c.clone(), "typed"),
        (_, Some(p)) if !p.trim().is_empty() => (read_criteria_file(ctx, p)?, "checklist_file"),
        _ => (Vec::new(), "none"),
    };
    if inputs.len() > MAX_CRITERIA {
        return Err(PhnError::InvalidInput(format!("at most {MAX_CRITERIA} criteria")));
    }
    let mut criteria: Vec<CriterionOut> = Vec::new();
    let mut by_section: BTreeMap<NarrativeSection, Vec<usize>> = BTreeMap::new();
    let mut not_drafted: Vec<String> = Vec::new();
    for c in &inputs {
        let label = clip(c.label.trim(), MAX_LABEL_CHARS);
        if label.is_empty() {
            continue;
        }
        check_text("criteria label", &label)?;
        if let Some(p) = c.points
            && !(0.0..=1000.0).contains(&p)
        {
            return Err(PhnError::InvalidInput(format!("points for '{label}' must be between 0 and 1000")));
        }
        let t = type_for(&label, &types);
        let section = c.section.or_else(|| t.and_then(|t| NarrativeSection::from_id(&t.narrative_section)));
        let answered_in = match section {
            Some(s) if sections.contains(&s) => {
                by_section.entry(s).or_default().push(criteria.len());
                s.id().to_string()
            }
            Some(s) => {
                not_drafted.push(format!("{label}: answered in the {} section, which you did not ask for.", s.id().replace('_', " ")));
                "other".into()
            }
            None => {
                let what = match t.map(|t| t.id.as_str()) {
                    Some("budget") => "build the budget and budget narrative with build_program_budget",
                    Some("cover") => "complete the forms, letters, and attachments the announcement lists",
                    _ => "this project could not tell which narrative section it belongs to; assign it with the section field",
                };
                not_drafted.push(format!("{label}: {what}."));
                "other".into()
            }
        };
        criteria.push(CriterionOut { label, points: c.points, criterion_type: t.map(|t| t.id.clone()), answered_in });
    }

    // Draft each section.
    let mut drafts: Vec<SectionDraft> = Vec::new();
    let mut missing_cite = 0usize;
    for sec in &sections {
        let def = templates
            .sections
            .iter()
            .find(|d| d.id == sec.id())
            .ok_or_else(|| PhnError::Internal(format!("template for {} is missing", sec.id())))?;
        let mut text = String::new();
        let mut slot_out = Vec::new();
        let crit_idx = by_section.get(sec).cloned().unwrap_or_default();
        let crit_labels: Vec<String> = crit_idx
            .iter()
            .map(|i| {
                let c = &criteria[*i];
                match c.points {
                    Some(p) => format!("{} ({} points)", c.label, md::num(p, if p.fract() == 0.0 { 0 } else { 1 })),
                    None => c.label.clone(),
                }
            })
            .collect();
        if !crit_idx.is_empty() {
            text.push_str("_Scoring criteria this section answers:_\n\n");
            for l in &crit_labels {
                text.push_str(&format!("- {l}\n"));
            }
            let mut ids: Vec<&str> = crit_idx.iter().filter_map(|i| criteria[*i].criterion_type.as_deref()).collect();
            ids.sort();
            ids.dedup();
            let prompts: Vec<&String> = ids.iter().filter_map(|id| templates.criterion_prompts.get(*id)).flatten().collect();
            if !prompts.is_empty() {
                text.push_str("\n_What reviewers look for (this project's writing prompts, not the funder's words):_\n\n");
                for p in prompts {
                    text.push_str(&format!("- {p}\n"));
                }
            }
            text.push('\n');
        }
        let mut references: BTreeSet<String> = BTreeSet::new();
        for slot in &def.slots {
            text.push_str(&format!("### {}\n\n", slot.heading));
            let mut filled = false;
            match slot.kind.as_str() {
                "data" => {
                    if data_points.is_empty() {
                        text.push_str(&placeholder(&slot.placeholder, &community));
                        text.push_str("\n\n");
                    } else {
                        for d in &data_points {
                            let (line, reference) = data_line(d, &community, &mut missing_cite);
                            text.push_str(&format!("- {line}\n"));
                            if let Some(r) = reference {
                                references.insert(r);
                            }
                        }
                        text.push('\n');
                        filled = true;
                    }
                }
                "list" => {
                    let items = slot.fact.as_deref().and_then(|k| facts.lists.get(k)).cloned().unwrap_or_default();
                    if items.is_empty() {
                        text.push_str(&placeholder(&slot.placeholder, &community));
                        text.push_str("\n\n");
                    } else {
                        for i in items {
                            text.push_str(&format!("- {i}\n"));
                        }
                        text.push('\n');
                        filled = true;
                    }
                }
                _ => match slot.fact.as_deref().and_then(|k| facts.map.get(k)) {
                    Some(f) => {
                        text.push_str(f);
                        text.push_str("\n\n");
                        filled = true;
                    }
                    None => {
                        text.push_str(&placeholder(&slot.placeholder, &community));
                        text.push_str("\n\n");
                    }
                },
            }
            slot_out.push(SlotOut { heading: slot.heading.clone(), filled });
        }
        if !references.is_empty() {
            text.push_str("### Data sources\n\n");
            for r in &references {
                text.push_str(&format!("- {r}\n"));
            }
            text.push('\n');
        }
        drafts.push(SectionDraft {
            id: def.id.clone(),
            heading: def.heading.clone(),
            criteria: crit_labels,
            placeholders: count_placeholders(&text),
            words: words(&text),
            slots: slot_out,
            text: text.trim_end().to_string(),
        });
    }

    let placeholder_count: usize = drafts.iter().map(|d| d.placeholders).sum();
    let total_words: usize = drafts.iter().map(|d| d.words).sum();
    let page_limit_note = args.page_limit.filter(|p| (1..=500).contains(p)).map(|p| {
        format!(
            "The announcement's limit is {p} pages. This draft is {total_words} words including prompts and placeholders; count the pages after you replace the placeholders and delete the prompts, using the announcement's font, spacing, and margins."
        )
    });
    if let Some(p) = args.page_limit
        && !(1..=500).contains(&p)
    {
        return Err(PhnError::InvalidInput("page_limit must be between 1 and 500".into()));
    }

    let mut files = Vec::new();
    if args.write_document.unwrap_or(true) {
        let doc = narrative_document(ctx, &project_title, &funder, &applicant, &community, &criteria, &drafts, &page_limit_note, placeholder_count);
        let sandbox = ctx.sandbox()?;
        let stem = args
            .output_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("grant_narrative_{}", slug(project_title.as_deref().unwrap_or("draft"))));
        let docx = sandbox.new_output_file(&format!("{}.docx", stem.trim_end_matches(".docx")))?;
        std::fs::write(&docx, phn_docs::docx::to_docx(&doc)?)?;
        files.push(sandbox.display_path(&docx));
    }

    let result = NarrativeResult {
        project_title: project_title.clone(),
        funder: funder.clone(),
        criteria_from: criteria_from.into(),
        criteria,
        criteria_not_drafted: not_drafted,
        sections: drafts,
        data_points_used: if sections.contains(&NarrativeSection::NeedsStatement) { data_points.len() } else { 0 },
        data_points_missing_citation: if sections.contains(&NarrativeSection::NeedsStatement) { missing_cite } else { 0 },
        placeholder_count,
        words: total_words,
        page_limit_note,
        files: files.clone(),
    };
    let summary = summary_markdown(&result);
    let mut caveats = vec![
        "This is a template filled only with the facts and statistics you supplied: the server added no data, no program details, and no citations of its own. Every [NURSE: ...] placeholder needs your text or must be deleted.".to_string(),
        "Check the draft against the announcement's scoring criteria, page limit, and format rules before you submit; a funder's own instructions come first.".to_string(),
    ];
    if result.data_points_missing_citation > 0 {
        caveats.push(format!(
            "{} statistic(s) lack a source or year and show a placeholder: a number without a source and date should not go in an application.",
            result.data_points_missing_citation
        ));
    }
    caveats.extend(content::review_caveats(&[TEMPLATES]));
    Ok(Envelope::new(summary, result)
        .sources(vec![
            Source::computed("Grant narrative skeletons (this project's templates)", "PHN MCP server"),
        ])
        .caveats(caveats)
        .privacy(Privacy {
            row_level_data: false,
            files_written: files,
            notes: vec!["The draft contains only the text and statistics you passed in plus template wording; client identifiers in the input are refused.".into()],
            ..Privacy::none()
        })
        .decision_support())
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let t: String = out.trim_matches('-').chars().take(40).collect();
    if t.is_empty() { "draft".into() } else { t }
}

#[allow(clippy::too_many_arguments)]
fn narrative_document(
    ctx: &Context,
    title: &Option<String>,
    funder: &Option<String>,
    applicant: &Option<String>,
    community: &str,
    criteria: &[CriterionOut],
    drafts: &[SectionDraft],
    page_note: &Option<String>,
    placeholders: usize,
) -> Document {
    let p = |t: &str| Block::Paragraph(vec![Inline::plain(t)]);
    let mut blocks = vec![
        p(&format!(
            "Draft prepared {} for {}{}. Community: {}. Replace every [NURSE: ...] placeholder with your own text (or delete the paragraph) and delete the italic prompts before you submit. Nothing in this draft was invented: the statistics and program facts are the ones you supplied.",
            chrono::Local::now().date_naive(),
            applicant.clone().unwrap_or_else(|| "[NURSE: applicant agency]".into()),
            funder.as_ref().map(|f| format!(" in response to {f}")).unwrap_or_default(),
            community
        )),
        p(&format!("{placeholders} placeholder(s) to complete.")),
    ];
    if let Some(n) = page_note {
        blocks.push(p(n));
    }
    if !criteria.is_empty() {
        blocks.push(Block::Heading { level: 2, text: "Scoring criteria and where each is answered".into() });
        blocks.push(Block::Table(TableBlock {
            caption: Some("Criteria".into()),
            headers: vec!["Criterion".into(), "Points".into(), "Answered in".into()],
            rows: criteria
                .iter()
                .map(|c| vec![c.label.clone(), c.points.map(|x| md::num(x, if x.fract() == 0.0 { 0 } else { 1 })).unwrap_or_default(), c.answered_in.replace('_', " ")])
                .collect(),
        }));
    }
    for d in drafts {
        blocks.push(Block::Heading { level: 1, text: d.heading.clone() });
        blocks.extend(phn_docs::model::from_markdown(&d.text));
    }
    Document {
        title: format!("Grant application draft — {}", title.clone().unwrap_or_else(|| "[NURSE: project title]".into())),
        subtitle: Some("Needs statement, project narrative, and sustainability (draft)".into()),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

fn summary_markdown(r: &NarrativeResult) -> String {
    let mut s = format!(
        "**Grant narrative draft{}**\n\n",
        r.project_title.as_ref().map(|t| format!(": {t}")).unwrap_or_default()
    );
    s.push_str(&md::table(
        &["Section", "Parts filled", "Placeholders", "Words", "Criteria answered"],
        &r.sections
            .iter()
            .map(|d| {
                vec![
                    d.heading.clone(),
                    format!("{} of {}", d.slots.iter().filter(|x| x.filled).count(), d.slots.len()),
                    d.placeholders.to_string(),
                    d.words.to_string(),
                    d.criteria.len().to_string(),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    s.push_str(&format!(
        "\n{} placeholder(s) need your text; {} statistic(s) used, {} without a full citation.\n",
        r.placeholder_count, r.data_points_used, r.data_points_missing_citation
    ));
    if r.criteria_from == "none" {
        s.push_str("\nNo scoring criteria were given, so the draft follows a generic skeleton: pass criteria (typed, or from the checklist file) to organize it around what the funder scores.\n");
    } else {
        s.push_str(&format!("\nCriteria came from {}.\n", r.criteria_from.replace('_', " ")));
    }
    for n in &r.criteria_not_drafted {
        s.push_str(&format!("- Not drafted here: {n}\n"));
    }
    if let Some(n) = &r.page_limit_note {
        s.push_str(&format!("\n{n}\n"));
    }
    if !r.files.is_empty() {
        s.push_str(&format!("\nDraft written to {}.\n", r.files.join(" and ")));
    }
    s.push_str("\nThe full draft text is in the data (sections[].text).\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn crit(label: &str, points: f64) -> CriterionInput {
        CriterionInput { label: label.into(), points: Some(points), section: None }
    }

    #[test]
    fn content_loads_and_slots_have_facts() {
        let t: Templates = content::load(TEMPLATES).unwrap();
        assert_eq!(t.sections.len(), 3);
        for s in &t.sections {
            assert!(NarrativeSection::from_id(&s.id).is_some());
            assert!(s.slots.iter().all(|x| x.kind == "data" || x.fact.is_some()));
        }
        for ty in criterion_types().unwrap() {
            assert!(t.criterion_prompts.contains_key(&ty.id), "no prompts for {}", ty.id);
        }
    }

    #[test]
    fn drafts_from_criteria_without_inventing_anything() {
        let (d, c) = ctx();
        let args = NarrativeArgs {
            project_title: Some("Neighborhood Asthma Home Visits".into()),
            funder: Some("Example Foundation".into()),
            applicant: Some("Test Town Health Department".into()),
            community: Some("Test Town".into()),
            criteria: Some(vec![crit("Statement of Need", 20.0), crit("Approach", 30.0), crit("Sustainability", 10.0), crit("Budget", 15.0), crit("Cover Letter and Forms", 0.0)]),
            data_points: Some(vec![
                DataPoint { label: "Children with asthma".into(), value: "11.4%".into(), geography: Some("Test Town".into()), source: Some("School nurse survey".into()), as_of: Some("2025".into()), comparison: Some("state 9.1%".into()) },
                DataPoint { label: "Households below the poverty line".into(), value: "18%".into(), geography: None, source: None, as_of: None, comparison: None },
            ]),
            facts: Some(ProgramFacts {
                health_problem: Some("Asthma sends children to the emergency department.".into()),
                objectives: vec!["Visit 40 homes by June 2027".into()],
                ..Default::default()
            }),
            page_limit: Some(12),
            ..Default::default()
        };
        let r = draft_grant_narrative(&c, args).unwrap();
        let d3 = &r.data;
        assert_eq!(d3.sections.len(), 3);
        assert_eq!(d3.criteria_from, "typed");
        let needs = &d3.sections[0];
        assert!(needs.text.contains("Asthma sends children to the emergency department."));
        assert!(needs.text.contains("**Children with asthma:** 11.4% (Test Town; School nurse survey, 2025; comparison: state 9.1%)"));
        assert!(needs.text.contains("[NURSE: add the source and year]"));
        assert!(needs.text.contains("Test Town")); // geography default for the second point
        assert!(needs.criteria.iter().any(|x| x.starts_with("Statement of Need (20 points)")));
        assert_eq!(d3.data_points_used, 2);
        assert_eq!(d3.data_points_missing_citation, 1);
        // A placeholder appears for the missing facts, never an invented one.
        assert!(needs.text.contains("[NURSE: Say what is missing"));
        let project = &d3.sections[1];
        assert!(project.text.contains("- Visit 40 homes by June 2027"));
        assert!(project.text.contains("[NURSE: List each activity"));
        assert!(d3.criteria_not_drafted.iter().any(|n| n.starts_with("Budget") && n.contains("build_program_budget")));
        assert!(d3.criteria_not_drafted.iter().any(|n| n.starts_with("Cover Letter")));
        assert!(d3.placeholder_count > 10);
        assert!(d3.page_limit_note.as_deref().unwrap().contains("12 pages"));
        // The DOCX exists and has the placeholder text.
        let f = &d3.files[0];
        let bytes = std::fs::read(d.path().join("data").join(f)).unwrap();
        let doc = phn_local::docx::parse_docx(&bytes).unwrap();
        assert!(doc.plain_text().contains("Statement of need"));
        assert!(doc.plain_text().contains("[NURSE:"));
        // Nothing in the draft names a number that was not supplied.
        for n in ["11.4%", "18%", "40 homes"] {
            assert!(r.summary.contains("placeholder") && (needs.text.contains(n) || project.text.contains(n)));
        }
        assert!(r.caveats.iter().any(|c| c.contains("lack a source or year")));
        assert!(r.caveats.iter().any(|c| c.contains("not yet been reviewed")));
    }

    #[test]
    fn a_checklist_file_supplies_the_criteria() {
        let (d, c) = ctx();
        std::fs::create_dir_all(d.path().join("data/output")).unwrap();
        let doc = Document {
            title: "t".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: None,
            logo: None,
            blocks: vec![Block::Table(TableBlock {
                caption: Some("Scoring criteria".into()),
                headers: ["#", "Criterion", "Points", "Type", "Narrative section", "Where"].map(String::from).to_vec(),
                rows: vec![
                    ["1", "Statement of Need", "20", "need", "needs_statement", "p1"].map(String::from).to_vec(),
                    ["2", "Evaluation Plan", "25", "evaluation", "project_narrative", "p2"].map(String::from).to_vec(),
                ],
            })],
        };
        std::fs::write(d.path().join("data/output/checklist.xlsx"), phn_docs::xlsx::to_xlsx(&doc).unwrap()).unwrap();
        let r = draft_grant_narrative(
            &c,
            NarrativeArgs {
                criteria_file: Some("output/checklist.xlsx".into()),
                sections: Some(vec![NarrativeSection::NeedsStatement]),
                write_document: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.criteria_from, "checklist_file");
        assert_eq!(r.data.criteria.len(), 2);
        assert_eq!(r.data.criteria[0].points, Some(20.0));
        // The evaluation criterion belongs to a section that was not requested.
        assert!(r.data.criteria_not_drafted.iter().any(|n| n.starts_with("Evaluation Plan")));
        assert!(r.data.files.is_empty());
        // A missing sheet or wrong type is a clear error.
        std::fs::write(d.path().join("data/plain.csv"), "a,b\n1,2\n").unwrap();
        let e = draft_grant_narrative(&c, NarrativeArgs { criteria_file: Some("plain.csv".into()), ..Default::default() }).unwrap_err();
        assert!(e.to_string().contains("not a spreadsheet"));
    }

    #[test]
    fn client_identifiers_and_bad_input_are_refused() {
        let (_d, c) = ctx();
        let e = draft_grant_narrative(
            &c,
            NarrativeArgs {
                facts: Some(ProgramFacts { health_problem: Some("Case: SSN 123-45-6789 had asthma".into()), ..Default::default() }),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("health_problem") && !e.to_string().contains("123-45"));
        let e = draft_grant_narrative(&c, NarrativeArgs { sections: Some(vec![]), ..Default::default() }).unwrap_err();
        assert!(e.to_string().contains("sections"));
        let e = draft_grant_narrative(&c, NarrativeArgs { page_limit: Some(0), write_document: Some(false), ..Default::default() }).unwrap_err();
        assert!(e.to_string().contains("page_limit"));
        let e = draft_grant_narrative(
            &c,
            NarrativeArgs { data_points: Some(vec![DataPoint { label: "x".into(), ..Default::default() }]), ..Default::default() },
        )
        .unwrap_err();
        assert!(e.to_string().contains("label and a value"));
        let e = draft_grant_narrative(
            &c,
            NarrativeArgs { facts: Some(ProgramFacts { goal: Some("g".repeat(5000)), ..Default::default() }), ..Default::default() },
        )
        .unwrap_err();
        assert!(e.to_string().contains("4000"));
    }

    #[test]
    fn no_criteria_gives_a_generic_skeleton() {
        let (_d, c) = ctx();
        let r = draft_grant_narrative(&c, NarrativeArgs { write_document: Some(false), ..Default::default() }).unwrap();
        assert_eq!(r.data.criteria_from, "none");
        assert!(r.summary.contains("generic skeleton"));
        assert!(r.data.sections.iter().all(|s| s.criteria.is_empty()));
        assert!(r.data.sections[0].text.contains("[NURSE: Add three to five cited statistics for your community"));
    }
}
