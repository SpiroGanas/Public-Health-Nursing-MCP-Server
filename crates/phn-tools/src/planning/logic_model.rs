//! PLAN-01 `build_logic_model`: a logic model (inputs, activities, outputs, short-term,
//! intermediate, and long-term outcomes, assumptions, and external factors) built from the
//! nurse's program description, checked against CDC's Program Evaluation Framework (Step 2),
//! and drawn as Mermaid text and an accessible SVG, with a one-page Word table.
//!
//! The model (the AI) drafts the components from the nurse's description; this tool
//! structures them, checks the wording and the coverage with rules read from
//! `content/planning/logic_model.yaml`, and renders the diagram with the reusable
//! [`super::diagram`] module. Nothing is computed about people or health.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::diagram::{Diagram, DiagramBand, DiagramColumn, Tone};
use crate::context::Context;
use crate::epr::content::{self, DocSource};
use crate::epr::roster;
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/planning/logic_model.yaml`.
pub const LOGIC_MODEL: &str = "planning-logic-model";

/// Most items accepted in one list, and the longest one item may be (characters).
const MAX_ITEMS: usize = 30;
const MAX_ITEM_CHARS: usize = 600;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Component {
    id: String,
    label: String,
    definition: String,
    prompts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ContextRow {
    id: String,
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StageDef {
    id: String,
    label: String,
    definition: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CheckDefaults {
    max_items_per_column: usize,
    max_item_words: usize,
    change_verbs: Vec<String>,
    activity_verbs: Vec<String>,
    activity_like_outcome_starts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CheckDef {
    id: String,
    severity: Severity,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct IfThen {
    order: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Tip {
    tip: String,
    how_applied: String,
}

/// `content/planning/logic_model.yaml`.
#[derive(Debug, Clone, Deserialize)]
struct LogicModelContent {
    sources: BTreeMap<String, DocSource>,
    components: Vec<Component>,
    context_rows: Vec<ContextRow>,
    stages: Vec<StageDef>,
    visualization_tips: Vec<Tip>,
    if_then: IfThen,
    check_defaults: CheckDefaults,
    checks: Vec<CheckDef>,
    review_questions: Vec<String>,
    narrative_headings: Vec<String>,
}

// ---------------------------------------------------------------- arguments and results

/// Stage of program development (CDC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Early in development; activities not yet fully implemented.
    Planning,
    /// The program is being implemented now.
    Implementation,
    /// A mature program that has run for an extended period.
    Maintenance,
}

impl Stage {
    pub fn id(self) -> &'static str {
        match self {
            Stage::Planning => "planning",
            Stage::Implementation => "implementation",
            Stage::Maintenance => "maintenance",
        }
    }
}

/// Arguments for `build_logic_model`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct LogicModelArgs {
    /// The program's name, for the title ("Postpartum home visiting program").
    pub program_name: String,
    /// Who the program serves (the population of focus), for the subtitle and the narrative.
    pub population: Option<String>,
    /// The need or problem the program addresses, in one or two sentences (goes in the
    /// narrative, not the diagram).
    pub need: Option<String>,
    /// Stage of development: planning, implementation, or maintenance.
    pub stage: Option<Stage>,
    /// Inputs: resources the program needs (staff, partners, funding, materials, data,
    /// evidence base). One short phrase each.
    pub inputs: Vec<String>,
    /// Activities: what the program does with those resources, each starting with an action
    /// word ("Provide home visits to ...").
    pub activities: Vec<String>,
    /// Outputs: the countable products of the activities ("Home visits completed").
    pub outputs: Vec<String>,
    /// Short-term outcomes: who or what changes first (knowledge, access, skills, policies).
    pub short_term_outcomes: Vec<String>,
    /// Intermediate outcomes: changes that follow, such as behavior, practice, or service use.
    pub intermediate_outcomes: Vec<String>,
    /// Long-term outcomes: the broadest changes the program contributes to (health status,
    /// population-level change).
    pub long_term_outcomes: Vec<String>,
    /// Assumptions: why the team expects the activities to lead to the outcomes, with the
    /// evidence if there is one.
    pub assumptions: Vec<String>,
    /// External (contextual) factors outside the program that could help or hinder it.
    pub external_factors: Vec<String>,
    /// Write a Word (or PDF) table, the SVG and PNG diagram, and the Mermaid text to
    /// /data/output (default true).
    pub write_documents: Option<bool>,
    /// Document format for the table: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
    /// Return the SVG source in the result (default true; the file is written either way).
    pub include_svg: Option<bool>,
}

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Fix before using the model.
    NeedsWork,
    /// A suggestion to consider.
    Consider,
}

/// One finding from the structure and wording checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub message: String,
}

/// One column of the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColumnOut {
    pub id: String,
    pub label: String,
    /// CDC's definition of the component.
    pub definition: String,
    pub items: Vec<String>,
    /// Questions to ask the nurse when the column is thin.
    pub prompts: Vec<String>,
}

/// A section of the narrative scaffold CDC pairs with the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NarrativeSection {
    pub heading: String,
    pub text: String,
}

/// The diagram in each form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DiagramOut {
    /// Mermaid `flowchart LR` source.
    pub mermaid: String,
    /// Self-contained accessible SVG (when include_svg is true).
    pub svg: Option<String>,
    /// Short alt text for the image.
    pub alt_text: String,
    /// The full content of the diagram as plain sentences (use beside the image).
    pub text_description: String,
}

/// Files written for the model (relative to /data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogicModelFiles {
    pub folder: String,
    pub document: Option<String>,
    pub svg: Option<String>,
    pub png: Option<String>,
    pub mermaid: Option<String>,
}

/// Result of `build_logic_model`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogicModelResult {
    pub program_name: String,
    pub population: Option<String>,
    pub stage: Option<String>,
    pub columns: Vec<ColumnOut>,
    pub assumptions: Vec<String>,
    pub external_factors: Vec<String>,
    /// The model read as if-then statements, as in CDC's example.
    pub if_then: Vec<String>,
    /// "draft_ready" when no check needs work, else "needs_work".
    pub status: String,
    pub findings: Vec<Finding>,
    /// Questions CDC's checklist asks of a draft model.
    pub review_questions: Vec<String>,
    pub narrative: Vec<NarrativeSection>,
    pub diagram: DiagramOut,
    pub files: Option<LogicModelFiles>,
    /// CDC's visualization tips and how this diagram applies them.
    pub visualization_notes: Vec<String>,
}

// ---------------------------------------------------------------- helpers

fn tidy(items: &[String], what: &str) -> Result<Vec<String>> {
    if items.len() > MAX_ITEMS {
        return Err(PhnError::InvalidInput(format!(
            "{what} has {} items; a logic model fits on one page, so give at most {MAX_ITEMS} (6 or fewer reads best)",
            items.len()
        )));
    }
    let mut out = Vec::new();
    for s in items {
        let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > MAX_ITEM_CHARS {
            return Err(PhnError::InvalidInput(format!(
                "an item in {what} is longer than {MAX_ITEM_CHARS} characters; use a short phrase per box"
            )));
        }
        out.push(t);
    }
    Ok(out)
}

fn first_words(s: &str, n: usize) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()))
        .take(n)
        .collect()
}

fn words_of(s: &str) -> Vec<String> {
    first_words(s, usize::MAX)
}

fn fill(template: &str, vals: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (k, v) in vals {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

fn join_short(items: &[String]) -> String {
    let take: Vec<String> = items
        .iter()
        .take(3)
        .map(|s| {
            let t = s.trim().trim_end_matches('.').to_string();
            if t.chars().count() > 120 {
                format!("{}...", t.chars().take(117).collect::<String>().trim_end())
            } else {
                t
            }
        })
        .collect();
    let mut s = take.join("; ");
    if items.len() > 3 {
        s.push_str(&format!(" (and {} more)", items.len() - 3));
    }
    s
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) if c.as_str().chars().next().is_some_and(|n| !n.is_uppercase()) => {
            f.to_lowercase().collect::<String>() + c.as_str()
        }
        _ => s.to_string(),
    }
}

// ---------------------------------------------------------------- the tool

/// Build a logic model.
pub fn build_logic_model(
    ctx: &Context,
    args: LogicModelArgs,
) -> Result<Envelope<LogicModelResult>> {
    let c: LogicModelContent = content::load(LOGIC_MODEL)?;
    let name = args
        .program_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return Err(PhnError::InvalidInput(
            "give the program's name (program_name)".into(),
        ));
    }
    if name.chars().count() > 150 {
        return Err(PhnError::InvalidInput(
            "program_name is longer than 150 characters; use the short name".into(),
        ));
    }
    let lists: [(&str, &Vec<String>); 6] = [
        ("inputs", &args.inputs),
        ("activities", &args.activities),
        ("outputs", &args.outputs),
        ("short_term_outcomes", &args.short_term_outcomes),
        ("intermediate_outcomes", &args.intermediate_outcomes),
        ("long_term_outcomes", &args.long_term_outcomes),
    ];
    let mut cols: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (key, list) in lists {
        let id = match key {
            "short_term_outcomes" => "short_term",
            "intermediate_outcomes" => "intermediate",
            "long_term_outcomes" => "long_term",
            k => k,
        };
        cols.insert(id, tidy(list, key)?);
    }
    let assumptions = tidy(&args.assumptions, "assumptions")?;
    let external = tidy(&args.external_factors, "external_factors")?;
    if cols.values().all(Vec::is_empty) && assumptions.is_empty() && external.is_empty() {
        return Err(PhnError::InvalidInput(
            "give at least the activities and the outcomes (and, if you can, inputs, outputs, assumptions, and external factors); \
             the tool structures and checks what you give it and does not invent program content"
                .into(),
        ));
    }
    let population = args
        .population
        .as_deref()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty());
    let need = args
        .need
        .as_deref()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty());

    // ---- checks
    let defaults = &c.check_defaults;
    let def = |id: &str| -> &CheckDef {
        c.checks
            .iter()
            .find(|k| k.id == id)
            .unwrap_or_else(|| panic!("check {id} is missing from the content"))
    };
    let mut findings: Vec<Finding> = Vec::new();
    let mut add = |id: &str, vals: &[(&str, String)]| {
        let d = def(id);
        findings.push(Finding {
            id: d.id.clone(),
            severity: d.severity,
            message: fill(&d.message, vals),
        });
    };
    let label_of = |id: &str| -> String {
        c.components
            .iter()
            .find(|x| x.id == id)
            .map(|x| x.label.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let n_of = |id: &str| cols.get(id).map_or(0, Vec::len);
    if n_of("activities") == 0 {
        add("missing_activities", &[]);
    }
    if n_of("short_term") + n_of("intermediate") + n_of("long_term") == 0 {
        add("missing_outcomes", &[]);
    }
    if n_of("inputs") == 0 {
        add("missing_inputs", &[]);
    }
    let outcome_ids = ["short_term", "intermediate", "long_term"];
    let some_outcome = outcome_ids.iter().any(|i| n_of(i) > 0);
    if some_outcome {
        let missing: Vec<String> = outcome_ids
            .iter()
            .filter(|i| n_of(i) == 0)
            .map(|i| label_of(i).to_lowercase())
            .collect();
        if !missing.is_empty() {
            let verb = if missing.len() == 1 { "was" } else { "were" };
            add(
                "outcome_sequence",
                &[("missing", format!("{} {verb}", missing.join(" and ")))],
            );
        }
    }
    if n_of("outputs") == 0 {
        add("no_outputs", &[]);
    } else if n_of("outputs") < n_of("activities") {
        add(
            "fewer_outputs_than_activities",
            &[
                ("outputs", n_of("outputs").to_string()),
                ("activities", n_of("activities").to_string()),
            ],
        );
    }
    for comp in &c.components {
        let items = cols.get(comp.id.as_str()).cloned().unwrap_or_default();
        if items.len() > defaults.max_items_per_column {
            add(
                "too_many_items",
                &[
                    ("column", comp.label.clone()),
                    ("count", items.len().to_string()),
                    ("limit", defaults.max_items_per_column.to_string()),
                ],
            );
        }
        let mut seen: Vec<String> = Vec::new();
        for (i, it) in items.iter().enumerate() {
            let words = it.split_whitespace().count();
            if words > defaults.max_item_words {
                add(
                    "long_item",
                    &[
                        ("column", comp.label.clone()),
                        ("number", (i + 1).to_string()),
                        ("words", words.to_string()),
                        ("limit", defaults.max_item_words.to_string()),
                    ],
                );
            }
            let key = it.to_lowercase();
            if seen.contains(&key) {
                add(
                    "duplicate_item",
                    &[("column", comp.label.clone()), ("text", it.clone())],
                );
            }
            seen.push(key);
            let first = first_words(it, 1).into_iter().next().unwrap_or_default();
            if outcome_ids.contains(&comp.id.as_str()) {
                if defaults.activity_like_outcome_starts.contains(&first) {
                    add(
                        "outcome_is_activity",
                        &[
                            ("column", comp.label.clone()),
                            ("number", (i + 1).to_string()),
                            ("word", first.clone()),
                        ],
                    );
                } else if !words_of(it)
                    .iter()
                    .any(|w| defaults.change_verbs.contains(w))
                {
                    add(
                        "outcome_without_change",
                        &[
                            ("column", comp.label.clone()),
                            ("number", (i + 1).to_string()),
                        ],
                    );
                }
            }
            if comp.id == "activities" {
                let lead = first_words(it, 2);
                if !lead.iter().any(|w| defaults.activity_verbs.contains(w)) {
                    add("activity_not_action", &[("number", (i + 1).to_string())]);
                }
            }
        }
    }
    if assumptions.is_empty() {
        add("no_assumptions", &[]);
    }
    if external.is_empty() {
        add("no_external_factors", &[]);
    }
    if args.stage.is_none() {
        add("no_stage", &[]);
    }
    if population.is_none() {
        add("no_population", &[]);
    }
    let status = if findings.iter().any(|f| f.severity == Severity::NeedsWork) {
        "needs_work"
    } else {
        "draft_ready"
    };
    findings.sort_by_key(|f| f.severity == Severity::Consider);

    // ---- columns and diagram
    let columns: Vec<ColumnOut> = c
        .components
        .iter()
        .map(|comp| ColumnOut {
            id: comp.id.clone(),
            label: comp.label.clone(),
            definition: comp.definition.clone(),
            items: cols.get(comp.id.as_str()).cloned().unwrap_or_default(),
            prompts: comp.prompts.clone(),
        })
        .collect();
    let stage_label = args.stage.and_then(|s| {
        c.stages
            .iter()
            .find(|d| d.id == s.id())
            .map(|d| d.label.clone())
    });
    let mut subtitle_parts = vec!["Logic model".to_string()];
    if let Some(p) = &population {
        subtitle_parts.push(format!("for {p}"));
    }
    if let Some(s) = &stage_label {
        subtitle_parts.push(format!("({} stage)", s.to_lowercase()));
    }
    let diagram = Diagram {
        title: name.clone(),
        subtitle: Some(subtitle_parts.join(" ")),
        columns: columns
            .iter()
            .map(|col| {
                let work = matches!(col.id.as_str(), "inputs" | "activities" | "outputs");
                DiagramColumn {
                    id: col.id.clone(),
                    heading: col.label.clone(),
                    items: col.items.clone(),
                    tone: if work { Tone::Work } else { Tone::Result },
                    group: Some(if work { "Planned work" } else { "Intended results" }.into()),
                }
            })
            .collect(),
        bands: c
            .context_rows
            .iter()
            .map(|r| DiagramBand {
                id: r.id.clone(),
                heading: r.label.clone(),
                items: if r.id == "assumptions" {
                    assumptions.clone()
                } else {
                    external.clone()
                },
            })
            .collect(),
        footer: Some(
            "Arrows show the if-then flow between columns. Layout follows CDC's Program Evaluation Framework, Step 2."
                .into(),
        ),
    };
    let mermaid = diagram.to_mermaid();
    let svg = diagram.to_svg();
    let alt_text = diagram.alt_text();
    let text_description = diagram.text_description();

    // ---- if-then
    let mut if_then: Vec<String> = Vec::new();
    let present: Vec<&ColumnOut> = c
        .if_then
        .order
        .iter()
        .filter_map(|id| columns.iter().find(|x| &x.id == id))
        .filter(|x| !x.items.is_empty())
        .collect();
    for pair in present.windows(2) {
        if_then.push(format!(
            "If {} ({}), then {} ({}).",
            pair[0].label.to_lowercase(),
            join_short(&pair[0].items),
            pair[1].label.to_lowercase(),
            join_short(&pair[1].items)
        ));
    }

    // ---- narrative scaffold
    let blank = |what: &str| format!("[Add {what}.]");
    let mut narrative: Vec<NarrativeSection> = Vec::new();
    for h in &c.narrative_headings {
        let text = match h.as_str() {
            "Need" => need
                .clone()
                .unwrap_or_else(|| blank("the need or problem the program addresses")),
            "Inputs" => list_text(&cols, &["inputs"], "the inputs"),
            "Activities" => list_text(&cols, &["activities"], "the activities"),
            "Outcomes" => list_text(
                &cols,
                &["short_term", "intermediate", "long_term"],
                "the outcomes",
            ),
            "Contextual factors" => {
                if external.is_empty() {
                    blank("the contextual factors")
                } else {
                    external.join("; ")
                }
            }
            "Stage of development" => match args.stage {
                Some(s) => {
                    let d = c.stages.iter().find(|d| d.id == s.id());
                    format!(
                        "{}. {}",
                        d.map(|d| d.label.as_str()).unwrap_or(s.id()),
                        d.map(|d| d.definition.as_str()).unwrap_or("")
                    )
                }
                None => {
                    blank("the stage of development (planning, implementation, or maintenance)")
                }
            },
            _ => String::new(),
        };
        narrative.push(NarrativeSection {
            heading: h.clone(),
            text,
        });
    }

    // ---- files
    let write = args.write_documents.unwrap_or(true);
    let mut files_written: Vec<String> = Vec::new();
    let mut files: Option<LogicModelFiles> = None;
    if write {
        let (dir, shown) = templates::output_folder(
            ctx,
            &format!(
                "logic_model_{}",
                phn_local::sandbox::sanitize_file_name(&name).replace(' ', "_")
            ),
        )?;
        let format = args.format.unwrap_or_default();
        let md_doc = document_markdown(
            &name,
            &subtitle_parts.join(" "),
            &columns,
            &assumptions,
            &external,
            &if_then,
            &narrative,
            &c.review_questions,
            &findings,
        );
        let doc = templates::document(ctx, &md_doc, "en-US", &name);
        let document = templates::write_in(
            ctx,
            &dir,
            &format!("logic_model.{}", format.ext()),
            &format.render(&doc)?,
        )?;
        let svg_file = templates::write_in(ctx, &dir, "logic_model.svg", svg.as_bytes())?;
        let png_file = templates::write_in(
            ctx,
            &dir,
            "logic_model.png",
            &phn_docs::svg::svg_to_png(&svg, 2.0)?,
        )?;
        let mmd = templates::write_in(ctx, &dir, "logic_model.mmd", mermaid.as_bytes())?;
        files_written.extend([
            document.clone(),
            svg_file.clone(),
            png_file.clone(),
            mmd.clone(),
        ]);
        files = Some(LogicModelFiles {
            folder: shown,
            document: Some(document),
            svg: Some(svg_file),
            png: Some(png_file),
            mermaid: Some(mmd),
        });
    }

    // ---- response
    let visualization_notes: Vec<String> = c
        .visualization_tips
        .iter()
        .map(|t| format!("{} {}", t.tip, t.how_applied))
        .collect();
    let mut s = format!("**Logic model: {name}**\n\n");
    if let Some(p) = &population {
        s.push_str(&format!("Population: {p}. "));
    }
    if let Some(l) = &stage_label {
        s.push_str(&format!("Stage: {l}. "));
    }
    s.push_str(&format!(
        "Status: **{}** ({} finding(s)).\n\n",
        if status == "needs_work" {
            "needs work"
        } else {
            "draft ready for review"
        },
        findings.len()
    ));
    let rows: Vec<Vec<String>> = columns
        .iter()
        .map(|col| {
            vec![
                col.label.clone(),
                if col.items.is_empty() {
                    "_none given_".into()
                } else {
                    col.items.join("; ")
                },
            ]
        })
        .chain([
            vec![
                "Assumptions".to_string(),
                if assumptions.is_empty() {
                    "_none given_".into()
                } else {
                    assumptions.join("; ")
                },
            ],
            vec![
                "External factors".to_string(),
                if external.is_empty() {
                    "_none given_".into()
                } else {
                    external.join("; ")
                },
            ],
        ])
        .collect();
    s.push_str(&md::table(&["Component", "Content"], &rows));
    if !if_then.is_empty() {
        s.push_str("\n**Read as if-then statements**\n\n");
        for l in &if_then {
            s.push_str(&format!("- {l}\n"));
        }
    }
    if !findings.is_empty() {
        s.push_str("\n**Checks**\n\n");
        for f in &findings {
            s.push_str(&format!(
                "- {} {}\n",
                if f.severity == Severity::NeedsWork {
                    "**Needs work:**"
                } else {
                    "Consider:"
                },
                f.message
            ));
        }
    }
    s.push_str("\n**Diagram (Mermaid)**\n\n```mermaid\n");
    s.push_str(&mermaid);
    s.push_str("```\n");
    s.push_str(&format!("\n_Alt text:_ {alt_text}\n"));
    if let Some(f) = &files {
        s.push_str(&format!(
            "\nFiles in {}: {}.\n",
            f.folder,
            [
                f.document.as_deref(),
                f.svg.as_deref(),
                f.png.as_deref(),
                f.mermaid.as_deref()
            ]
            .iter()
            .flatten()
            .map(|p| p.rsplit('/').next().unwrap_or(p))
            .collect::<Vec<_>>()
            .join(", ")
        ));
    }

    // Identifiers typed into the description are noticed, never repeated.
    let texts = roster::argument_strings(&args, &["format", "write_documents", "include_svg"]);
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut caveats: Vec<String> = Vec::new();
    caveats.push(
        "The components are what you and the AI drafted from your description; the tool checks their structure and wording against CDC's Step 2 guidance but does not know whether the program logic is sound. Share the draft with interest holders (people served, staff, partners, and skeptics) before finalizing, as CDC advises."
            .into(),
    );
    caveats.push(
        "The wording checks (action words, change words, box counts and lengths) are this project's heuristics, not CDC rules. CDC treats outputs as indicators of the activities that fit best with the evaluation measures; the Outputs column is here because logic models often include one."
            .into(),
    );
    caveats.push(
        "The diagram is wide (one column per component) so it reads on a slide or a landscape page. The Word file carries the same content as a table that fits one portrait page. The Mermaid text has no assumptions-to-outcomes arrows; arrows connect columns, not single boxes."
            .into(),
    );
    if !found.is_empty() {
        caveats.push(format!(
            "Your description appears to contain {}. A logic model describes a program, not people: remove personal details before sharing it.",
            found.join(", ").to_lowercase()
        ));
    }
    caveats.extend(content::review_caveats(&[LOGIC_MODEL]));
    let src_keys = [
        "cdc-action-guide",
        "cdc-step2",
        "cdc-step2-checklist",
        "cdc-mmwr-2024",
    ];
    let sources = content::sources_for(src_keys.iter().copied(), &c.sources);
    let mut notes = Vec::new();
    if write {
        notes.push("The files hold only the program description you gave; nothing else was read from your folders.".into());
    }
    Ok(Envelope::new(
        s,
        LogicModelResult {
            program_name: name,
            population,
            stage: stage_label,
            columns,
            assumptions,
            external_factors: external,
            if_then,
            status: status.into(),
            findings,
            review_questions: c.review_questions.clone(),
            narrative,
            diagram: DiagramOut {
                mermaid,
                svg: args.include_svg.unwrap_or(true).then_some(svg),
                alt_text,
                text_description,
            },
            files,
            visualization_notes,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: found,
        files_written,
        notes,
        ..Privacy::none()
    }))
}

fn list_text(cols: &BTreeMap<&str, Vec<String>>, ids: &[&str], what: &str) -> String {
    let items: Vec<String> = ids
        .iter()
        .flat_map(|i| cols.get(i).cloned().unwrap_or_default())
        .collect();
    if items.is_empty() {
        format!("[Add {what}.]")
    } else {
        items
            .iter()
            .map(|s| lower_first(s.trim_end_matches('.')))
            .map(|s| {
                let mut c = s.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[allow(clippy::too_many_arguments)]
fn document_markdown(
    name: &str,
    subtitle: &str,
    columns: &[ColumnOut],
    assumptions: &[String],
    external: &[String],
    if_then: &[String],
    narrative: &[NarrativeSection],
    review_questions: &[String],
    findings: &[Finding],
) -> String {
    let mut m = format!("# {name}\n\n{subtitle}. Draft for review with interest holders.\n\n");
    let mut rows: Vec<Vec<String>> = columns
        .iter()
        .map(|c| {
            vec![
                c.label.clone(),
                if c.items.is_empty() {
                    "[none given]".into()
                } else {
                    c.items
                        .iter()
                        .enumerate()
                        .map(|(i, t)| format!("{}. {t}", i + 1))
                        .collect::<Vec<_>>()
                        .join(" ")
                },
            ]
        })
        .collect();
    rows.push(vec![
        "Assumptions".into(),
        if assumptions.is_empty() {
            "[none given]".into()
        } else {
            assumptions
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{}. {t}", i + 1))
                .collect::<Vec<_>>()
                .join(" ")
        },
    ]);
    rows.push(vec![
        "External factors".into(),
        if external.is_empty() {
            "[none given]".into()
        } else {
            external
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{}. {t}", i + 1))
                .collect::<Vec<_>>()
                .join(" ")
        },
    ]);
    m.push_str("Table: Logic model components\n\n");
    m.push_str(&md::table(&["Component", "Content"], &rows));
    if !if_then.is_empty() {
        m.push_str("\n## How to read it\n\n");
        for l in if_then {
            m.push_str(&format!("- {l}\n"));
        }
    }
    m.push_str("\n## Program narrative\n\n");
    for s in narrative {
        m.push_str(&format!("**{}.** {}\n\n", s.heading, s.text));
    }
    let open: Vec<&Finding> = findings.iter().collect();
    if !open.is_empty() {
        m.push_str("## To resolve before sharing\n\n");
        for f in open {
            m.push_str(&format!("- {}\n", f.message));
        }
        m.push('\n');
    }
    m.push_str("## Questions to review the draft\n\n");
    for q in review_questions {
        m.push_str(&format!("- {q}\n"));
    }
    m.push_str(
        "\nSource: CDC Program Evaluation Framework Action Guide (Step 2, Describe the program).\n",
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn good() -> LogicModelArgs {
        LogicModelArgs {
            program_name: "Postpartum home visiting program".into(),
            population: Some("first-time parents in Springfield".into()),
            need: Some("Few first-time parents get a postpartum visit within two weeks.".into()),
            stage: Some(Stage::Implementation),
            inputs: vec![
                "Two public health nurses".into(),
                "Referral partnership with the birthing hospital".into(),
            ],
            activities: vec![
                "Provide a nurse home visit within 14 days of discharge".into(),
                "Train hospital staff to refer eligible parents".into(),
            ],
            outputs: vec![
                "Home visits completed".into(),
                "Hospital staff trained".into(),
            ],
            short_term_outcomes: vec!["Parents know the warning signs after birth".into()],
            intermediate_outcomes: vec![
                "Increase the share of parents who attend the postpartum visit".into(),
            ],
            long_term_outcomes: vec!["Reduce preventable postpartum emergency visits".into()],
            assumptions: vec![
                "Parents accept a home visit when a hospital nurse recommends it".into(),
            ],
            external_factors: vec!["Changes in state home visiting funding".into()],
            write_documents: Some(false),
            ..LogicModelArgs::default()
        }
    }

    #[test]
    fn a_complete_model_is_draft_ready_with_if_then_statements() {
        let (_d, ctx) = ctx();
        let r = build_logic_model(&ctx, good()).unwrap();
        assert_eq!(r.data.status, "draft_ready", "{:?}", r.data.findings);
        assert!(r.data.if_then.len() >= 5, "{:?}", r.data.if_then);
        assert!(r.data.if_then[0].starts_with("If inputs (Two public health nurses"));
        assert!(r.data.diagram.mermaid.starts_with("flowchart LR"));
        assert!(
            r.data
                .diagram
                .svg
                .as_deref()
                .unwrap()
                .contains("role=\"img\"")
        );
        assert!(r.data.diagram.alt_text.contains("6 columns"));
        assert!(r.summary.contains("```mermaid"));
        assert!(r.data.files.is_none());
        // CDC's sources are cited with dates.
        assert!(r.sources.iter().all(|s| s.retrieved.is_some()));
        assert!(r.sources.iter().any(|s| s.title.contains("Action Guide")));
    }

    #[test]
    fn thin_models_get_specific_findings() {
        let (_d, ctx) = ctx();
        let r = build_logic_model(
            &ctx,
            LogicModelArgs {
                program_name: "Thin".into(),
                activities: vec!["Weekly newsletter".into()],
                short_term_outcomes: vec!["Hold a community meeting".into()],
                write_documents: Some(false),
                ..LogicModelArgs::default()
            },
        )
        .unwrap();
        let ids: Vec<&str> = r.data.findings.iter().map(|f| f.id.as_str()).collect();
        for want in [
            "missing_inputs",
            "no_outputs",
            "no_assumptions",
            "no_external_factors",
            "no_stage",
            "no_population",
            "outcome_sequence",
            "outcome_is_activity",
            "activity_not_action",
        ] {
            assert!(ids.contains(&want), "{want} not in {ids:?}");
        }
        assert_eq!(r.data.status, "needs_work");
        // Findings needing work come first.
        assert_eq!(r.data.findings[0].severity, Severity::NeedsWork);
        let seq = r
            .data
            .findings
            .iter()
            .find(|f| f.id == "outcome_sequence")
            .unwrap();
        assert!(
            seq.message
                .contains("intermediate outcomes and long-term outcomes were")
        );
    }

    #[test]
    fn limits_and_empty_input_are_errors() {
        let (_d, ctx) = ctx();
        let e = build_logic_model(&ctx, LogicModelArgs::default()).unwrap_err();
        assert!(e.to_string().contains("program's name"));
        let e = build_logic_model(
            &ctx,
            LogicModelArgs {
                program_name: "X".into(),
                ..LogicModelArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("at least the activities"));
        let e = build_logic_model(
            &ctx,
            LogicModelArgs {
                program_name: "X".into(),
                activities: vec!["Provide x".into(); 31],
                ..LogicModelArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("at most 30"));
        let e = build_logic_model(
            &ctx,
            LogicModelArgs {
                program_name: "X".into(),
                activities: vec!["é".repeat(601)],
                ..LogicModelArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("600 characters"));
    }

    #[test]
    fn too_many_boxes_and_duplicates_are_flagged() {
        let (_d, ctx) = ctx();
        let mut a = good();
        a.inputs = (1..=8).map(|i| format!("Resource {i}")).collect();
        a.activities
            .push("Provide a nurse home visit within 14 days of discharge".into());
        let r = build_logic_model(&ctx, a).unwrap();
        let ids: Vec<&str> = r.data.findings.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"too_many_items"));
        assert!(ids.contains(&"duplicate_item"));
    }

    #[test]
    fn writes_a_table_the_diagram_files_and_mermaid() {
        let (dir, ctx) = ctx();
        let mut a = good();
        a.write_documents = Some(true);
        let r = build_logic_model(&ctx, a).unwrap();
        let f = r.data.files.as_ref().unwrap();
        assert_eq!(r.privacy.files_written.len(), 4);
        for rel in [&f.document, &f.svg, &f.png, &f.mermaid] {
            let rel = rel.as_ref().unwrap();
            assert!(dir.path().join("data").join(rel).is_file(), "{rel}");
        }
        let docx =
            std::fs::read(dir.path().join("data").join(f.document.as_ref().unwrap())).unwrap();
        assert!(docx.starts_with(b"PK"));
        let svg =
            std::fs::read_to_string(dir.path().join("data").join(f.svg.as_ref().unwrap())).unwrap();
        assert!(svg.contains("Postpartum home visiting program"));
        let png = std::fs::read(dir.path().join("data").join(f.png.as_ref().unwrap())).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        // A second run never overwrites.
        let mut again = good();
        again.write_documents = Some(true);
        let r2 = build_logic_model(&ctx, again).unwrap();
        assert_ne!(r2.data.files.unwrap().folder, f.folder);
    }

    /// Writes the sample diagram where a person can look at it: `PHN_DIAGRAM_OUT=/some/dir cargo
    /// test -p phn-tools write_sample_diagram_for_viewing`.
    #[test]
    fn write_sample_diagram_for_viewing() {
        let Ok(dir) = std::env::var("PHN_DIAGRAM_OUT") else {
            return;
        };
        let (_d, ctx) = ctx();
        let r = build_logic_model(&ctx, good()).unwrap();
        let svg = r.data.diagram.svg.unwrap();
        std::fs::write(format!("{dir}/logic_model.svg"), &svg).unwrap();
        std::fs::write(
            format!("{dir}/logic_model.png"),
            phn_docs::svg::svg_to_png(&svg, 1.5).unwrap(),
        )
        .unwrap();
        std::fs::write(format!("{dir}/logic_model.mmd"), r.data.diagram.mermaid).unwrap();
    }

    #[test]
    fn pdf_format_works() {
        let (dir, ctx) = ctx();
        let mut a = good();
        a.write_documents = Some(true);
        a.format = Some(DocFormat::Pdf);
        let r = build_logic_model(&ctx, a).unwrap();
        let p = r.data.files.unwrap().document.unwrap();
        assert!(p.ends_with(".pdf"));
        assert!(
            std::fs::read(dir.path().join("data").join(p))
                .unwrap()
                .starts_with(b"%PDF")
        );
    }

    #[test]
    fn contact_details_in_the_description_are_flagged_not_repeated() {
        let (_d, ctx) = ctx();
        let mut a = good();
        a.need = Some("Call the family at 617-555-0142 about visits".into());
        let r = build_logic_model(&ctx, a).unwrap();
        assert!(!r.privacy.identifiers_detected.is_empty());
        assert!(
            r.caveats
                .iter()
                .any(|c| c.contains("remove personal details"))
        );
        // The caveat names the category, never the value.
        assert!(!r.caveats.iter().any(|c| c.contains("555-0142")));
    }

    #[test]
    fn content_is_pending_with_a_review_caveat() {
        let (_d, ctx) = ctx();
        let r = build_logic_model(&ctx, good()).unwrap();
        assert!(
            r.caveats.iter().any(
                |c| c.to_lowercase().contains("pending") || c.to_lowercase().contains("review")
            )
        );
    }
}
