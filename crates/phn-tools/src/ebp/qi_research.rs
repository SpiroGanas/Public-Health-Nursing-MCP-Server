//! EBP-07 `qi_vs_research_determination`: helps a nurse sort a project into
//! quality improvement, program evaluation, public health practice or
//! surveillance, or possible human subjects research, using the definitions in
//! 45 CFR 46.102, CDC's guidelines for defining public health research and
//! non-research, and the quality improvement ethics literature. The nurse
//! answers screening questions; the tool shows its reasoning for each answer,
//! never decides silently, and drafts a project description and IRB
//! determination request (template + local fill: placeholders `[NURSE: ...]`)
//! in /data/output. The institution's IRB decides.

use std::collections::{BTreeMap, BTreeSet};

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, DocSource};
use crate::md;
use crate::policy::shared::{optional_text, write_docx};

/// Content id of `content/ebp/qi_research_determination.yaml`.
pub const QI_RESEARCH: &str = "ebp-qi-research-determination";

const MAX_TITLE: usize = 150;
const MAX_TEXT: usize = 2000;
const MAX_NOTE: usize = 400;

// ---------------------------------------------------------------- content

/// A definition with a short quotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionDef {
    pub id: String,
    pub term: String,
    pub quote: String,
    pub paraphrase: String,
    pub source: String,
    pub section: String,
    pub confidence: content::Confidence,
}

/// A category of project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryDef {
    pub id: String,
    pub label: String,
    pub summary: String,
    pub typical_features: Vec<Cited>,
    pub irb_note: String,
}

/// Which way an answer points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// Points toward research.
    Research,
    /// Points away from research.
    NonResearch,
    /// Points toward the person being a human subject (46.102(e)).
    HumanSubject,
    /// Context only; does not move the reading.
    Info,
    /// No effect.
    None,
}

/// How much a question matters for a provisional reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Needed {
    Required,
    Recommended,
    Optional,
}

/// A screening question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionDef {
    pub id: String,
    pub stage: String,
    pub prompt: String,
    pub help: String,
    pub yes_means: Signal,
    pub no_means: Signal,
    pub needed: Needed,
    pub basis: Vec<Cited>,
}

/// Steps after each reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NextStepsDef {
    pub possible_research: Vec<String>,
    pub qi: Vec<String>,
    pub program_evaluation: Vec<String>,
    pub public_health_practice: Vec<String>,
    pub undetermined: Vec<String>,
}

/// A section of the draft document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocSectionDef {
    pub heading: String,
    pub prompts: Vec<String>,
}

/// The document template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocDef {
    pub title: String,
    pub filename: String,
    pub intro: String,
    pub placeholders: Vec<String>,
    pub sections: Vec<DocSectionDef>,
    pub answers_heading: String,
    pub answers_note: String,
}

/// `content/ebp/qi_research_determination.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QiContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub definitions: Vec<DefinitionDef>,
    pub categories: Vec<CategoryDef>,
    pub questions: Vec<QuestionDef>,
    pub what_irb_decides: String,
    pub next_steps: NextStepsDef,
    pub document: DocDef,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- arguments and results

/// An answer to a screening question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Yes,
    No,
    /// You are not sure. Unsure answers are not counted; they are listed for follow-up.
    Unsure,
}

impl Answer {
    fn label(self) -> &'static str {
        match self {
            Answer::Yes => "yes",
            Answer::No => "no",
            Answer::Unsure => "unsure",
        }
    }
}

/// One answer from the nurse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionAnswer {
    /// The question id from a previous call's `reasoning` list (for example
    /// "generalizable_intent"). Call with no answers to list all questions.
    pub question_id: String,
    pub answer: Answer,
    /// Optional note in your own words on why (a short phrase; no names of people).
    pub note: Option<String>,
}

/// Arguments for `qi_vs_research_determination`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct QiResearchArgs {
    /// Short project title for the draft document (no names of people).
    pub project_title: Option<String>,
    /// A few sentences describing the project (purpose, setting, population, activities). Put
    /// in the draft document only. Describe groups, never individual people.
    pub description: Option<String>,
    /// The IRB or human research protection office the request goes to, as your institution
    /// names it. Left as a placeholder when not given. The tool does not know any institution's
    /// policy or forms.
    pub reviewing_office: Option<String>,
    /// Your answers to the screening questions. Leave empty to get the question list. The tool
    /// never answers for you; questions left unanswered are listed, and required ones must be
    /// answered before it gives a provisional reading.
    pub answers: Vec<QuestionAnswer>,
    /// Write the draft project description and IRB determination request (DOCX) to /data/output
    /// (default true). It contains only what you typed here plus placeholders.
    pub write_document: Option<bool>,
}

/// Where the reading stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// No answers yet: the questions are listed.
    QuestionsOnly,
    /// Required questions are unanswered or unsure: no reading is given.
    NeedsAnswers,
    /// A provisional reading from your answers (not a determination).
    Provisional,
}

/// What an answer did to the reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    PointsToResearch,
    PointsToNonResearch,
    PointsToHumanSubject,
    Informational,
    NoEffect,
    Unsure,
    Unanswered,
}

/// The reasoning for one question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionReasoning {
    pub id: String,
    /// purpose, burden, authority, human_subjects, or context.
    pub stage: String,
    pub prompt: String,
    pub help: String,
    pub needed: Needed,
    pub answer: Option<Answer>,
    pub note: Option<String>,
    pub effect: Effect,
    /// Why the question matters, with citations.
    pub basis: Vec<CitedOut>,
}

/// Whether the project would involve human subjects if it is research.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HumanSubjectsReading {
    /// "likely", "unlikely", "unknown", or "not_applicable" (not research on these answers).
    pub status: String,
    pub note: String,
}

/// The provisional reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Classification {
    /// qi, program_evaluation, public_health_practice, possible_research, or undetermined.
    pub category: String,
    pub label: String,
    pub summary: String,
    pub human_subjects: HumanSubjectsReading,
    /// Questions whose answers point toward research.
    pub research_signals: Vec<String>,
    /// Questions whose answers point away from research.
    pub non_research_signals: Vec<String>,
    /// Points that complicate the reading (overlap, conflict, unsure answers).
    pub cautions: Vec<String>,
    /// Always: this is a provisional reading from your answers, not a determination.
    pub not_a_determination: String,
}

/// A category description from the content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoryOut {
    pub id: String,
    pub label: String,
    pub summary: String,
    pub typical_features: Vec<CitedOut>,
    pub irb_note: String,
}

/// A definition from the regulation or guidance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DefinitionOut {
    pub term: String,
    /// A quotation of 25 words or fewer.
    pub quote: String,
    pub paraphrase: String,
    pub citation: String,
    pub confidence: content::Confidence,
}

/// Result of `qi_vs_research_determination`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QiResearchResult {
    pub status: Status,
    pub classification: Option<Classification>,
    /// One entry per question, in order, with the answer (if any) and the reasoning.
    pub reasoning: Vec<QuestionReasoning>,
    /// Required questions that still need a yes or no.
    pub needs_answers: Vec<String>,
    /// Questions you answered unsure.
    pub unsure: Vec<String>,
    pub next_steps: Vec<String>,
    pub categories: Vec<CategoryOut>,
    pub definitions: Vec<DefinitionOut>,
    /// What the IRB decides, and what this tool does not.
    pub what_the_irb_decides: String,
    /// Where the draft document was written (none when not requested).
    pub document_file: Option<String>,
    /// What was not verified.
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- logic

/// The reading from a set of answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub status: Status,
    pub category: &'static str,
    pub human_subjects: &'static str,
    pub research_signals: Vec<String>,
    pub non_research_signals: Vec<String>,
    pub needs_answers: Vec<String>,
    pub unsure: Vec<String>,
    pub cautions: Vec<String>,
}

fn effect_of(q: &QuestionDef, a: Option<Answer>) -> Effect {
    let sig = match a {
        None => return Effect::Unanswered,
        Some(Answer::Unsure) => return Effect::Unsure,
        Some(Answer::Yes) => q.yes_means,
        Some(Answer::No) => q.no_means,
    };
    match sig {
        Signal::Research => Effect::PointsToResearch,
        Signal::NonResearch => Effect::PointsToNonResearch,
        Signal::HumanSubject => Effect::PointsToHumanSubject,
        Signal::Info => Effect::Informational,
        Signal::None => Effect::NoEffect,
    }
}

/// Turn answers into a provisional reading. Never decides for the nurse:
/// without a yes or no on every required question there is no reading.
pub fn read_answers(questions: &[QuestionDef], answers: &BTreeMap<String, Answer>) -> Reading {
    let get = |id: &str| answers.get(id).copied();
    let mut needs = Vec::new();
    let mut unsure = Vec::new();
    for q in questions {
        match get(&q.id) {
            None if q.needed == Needed::Required => needs.push(q.id.clone()),
            Some(Answer::Unsure) => {
                unsure.push(q.id.clone());
                if q.needed == Needed::Required {
                    needs.push(q.id.clone());
                }
            }
            _ => {}
        }
    }
    let mut research = Vec::new();
    let mut non_research = Vec::new();
    for q in questions {
        match effect_of(q, get(&q.id)) {
            Effect::PointsToResearch => research.push(q.id.clone()),
            Effect::PointsToNonResearch => non_research.push(q.id.clone()),
            _ => {}
        }
    }
    let mut r = Reading {
        status: Status::Provisional,
        category: "undetermined",
        human_subjects: "not_applicable",
        research_signals: research,
        non_research_signals: non_research,
        needs_answers: needs.clone(),
        unsure: unsure.clone(),
        cautions: Vec::new(),
    };
    if !needs.is_empty() {
        r.status = Status::NeedsAnswers;
        return r;
    }
    let yes = |id: &str| get(id) == Some(Answer::Yes);
    let no = |id: &str| get(id) == Some(Answer::No);

    if !r.research_signals.is_empty() {
        r.category = "possible_research";
        r.human_subjects =
            if yes("interaction_or_intervention") || yes("identifiable_private_information") {
                "likely"
            } else if no("interaction_or_intervention") && no("identifiable_private_information") {
                "unlikely"
            } else {
                "unknown"
            };
        if yes("local_improvement") && yes("generalizable_intent") {
            r.cautions.push(
                "Your answers fit both quality improvement and research. The Hastings Center group \
                 proposed an overlap category for such projects and a customized IRB process; ask \
                 your IRB how it handles them."
                    .into(),
            );
        }
        if yes("public_health_authority") && yes("generalizable_intent") {
            r.cautions.push(
                "You described public health authority work and also an intent to produce \
                 generalizable knowledge. CDC says the primary intent decides, and that added \
                 activities designed to generate generalizable knowledge are research; ask the IRB \
                 which parts, if any, are research."
                    .into(),
            );
        }
        if yes("research_component") {
            r.cautions.push(
                "If one component is designed to produce generalizable knowledge, CDC treats the \
                 whole project as research unless the components are separable; say which parts \
                 can be separated."
                    .into(),
            );
        }
    } else if yes("public_health_authority") {
        r.category = "public_health_practice";
    } else if yes("established_program") {
        r.category = "program_evaluation";
        if yes("local_improvement") {
            r.cautions.push(
                "Your project is also local improvement work; program evaluation and quality \
                 improvement overlap, and both are generally not research when the intent is not \
                 generalizable knowledge."
                    .into(),
            );
        }
    } else if yes("local_improvement") {
        r.category = "qi";
    }
    if r.category != "possible_research" && yes("publication_plan") {
        r.cautions.push(
            "You plan to publish or present. CDC says publication does not by itself make a \
             project research, but re-check your intent: if it has become producing knowledge for \
             use elsewhere, answer the first questions again."
                .into(),
        );
    }
    if r.category != "possible_research" {
        let risky: Vec<String> = questions
            .iter()
            .filter(|q| {
                q.yes_means == Signal::Research && get(&q.id) == Some(Answer::Unsure)
                    || q.no_means == Signal::Research && get(&q.id) == Some(Answer::Unsure)
            })
            .map(|q| q.id.clone())
            .collect();
        if !risky.is_empty() {
            r.cautions.push(format!(
                "You were unsure about: {}. A different answer to any of them could point toward \
                 research, so resolve them or ask the IRB.",
                risky.join(", ")
            ));
        }
    }
    r
}

// ---------------------------------------------------------------- the tool

fn category<'a>(c: &'a QiContent, id: &str) -> Result<&'a CategoryDef> {
    c.categories
        .iter()
        .find(|x| x.id == id)
        .ok_or_else(|| PhnError::Internal(format!("category {id} missing from content")))
}

fn human_subjects_note(status: &str) -> &'static str {
    match status {
        "likely" => {
            "Your answers point to a human subject under 45 CFR 46.102(e): intervention or \
             interaction with living people, or identifiable private information. If the project \
             is research, the IRB decides whether it is exempt (46.104) or needs review."
        }
        "unlikely" => {
            "You answered no to both human subject questions. Research that does not involve human \
             subjects as 46.102(e) defines them may not need IRB review under the regulation, but \
             the IRB or its designee makes that call, and some institutions require a review \
             anyway."
        }
        "unknown" => {
            "Answer the two human subject questions (interaction or intervention; identifiable \
             private information) to see whether the project would involve human subjects."
        }
        _ => {
            "The answers do not point to research, so the human subject question does not arise now."
        }
    }
}

fn next_steps_for<'a>(c: &'a QiContent, cat: &str) -> &'a [String] {
    match cat {
        "possible_research" => &c.next_steps.possible_research,
        "qi" => &c.next_steps.qi,
        "program_evaluation" => &c.next_steps.program_evaluation,
        "public_health_practice" => &c.next_steps.public_health_practice,
        _ => &c.next_steps.undetermined,
    }
}

fn effect_word(e: Effect) -> &'static str {
    match e {
        Effect::PointsToResearch => "points toward research",
        Effect::PointsToNonResearch => "points away from research",
        Effect::PointsToHumanSubject => "points to a human subject",
        Effect::Informational => "context only",
        Effect::NoEffect => "no effect",
        Effect::Unsure => "unsure (not counted)",
        Effect::Unanswered => "not answered",
    }
}

/// Markdown for the draft document.
fn document_markdown(
    c: &QiContent,
    args_title: &Option<String>,
    description: &Option<String>,
    office: &Option<String>,
    reasoning: &[QuestionReasoning],
    classification: &Option<Classification>,
) -> String {
    let d = &c.document;
    let fill = |s: &str| -> String {
        let mut out = s.to_string();
        if let Some(t) = args_title {
            out = out.replace("[NURSE: project title]", t);
        }
        if let Some(o) = office {
            out = out.replace(
                "[NURSE: IRB or human research protection office that will review this]",
                o,
            );
        }
        out
    };
    let mut m = format!(
        "# {}\n\n{}\n\n",
        d.title,
        d.intro.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    for s in &d.sections {
        m.push_str(&format!("## {}\n\n", s.heading));
        for p in &s.prompts {
            m.push_str(&format!("- {}\n", fill(p)));
        }
        if s.heading.starts_with("2.")
            && let Some(desc) = description
        {
            m.push_str(&format!(
                "- Project description (from the project lead): {desc}\n"
            ));
        }
        m.push('\n');
    }
    m.push_str(&format!("## {}\n\n", d.answers_heading));
    m.push_str(&format!(
        "{}\n\n",
        d.answers_note
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    ));
    let rows: Vec<Vec<String>> = reasoning
        .iter()
        .map(|q| {
            vec![
                q.prompt.clone(),
                q.answer
                    .map(|a| a.label().to_string())
                    .unwrap_or_else(|| "[NURSE: answer]".into()),
                q.note.clone().unwrap_or_default(),
                effect_word(q.effect).to_string(),
            ]
        })
        .collect();
    m.push_str(&md::table(
        &["Question", "Project lead's answer", "Note", "Reading"],
        &rows,
    ));
    m.push('\n');
    match classification {
        Some(cl) => {
            m.push_str(&format!(
                "Provisional reading from these answers (not a determination): **{}**. {}\n\n",
                cl.label, cl.summary
            ));
            for x in &cl.cautions {
                m.push_str(&format!("- {x}\n"));
            }
            m.push('\n');
        }
        None => m.push_str(
            "Provisional reading: not yet available; required questions still need answers.\n\n",
        ),
    }
    m.push_str(&format!(
        "_{}_\n",
        c.what_irb_decides
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    ));
    m
}

/// Run the determination.
pub fn qi_vs_research_determination(
    ctx: &Context,
    args: QiResearchArgs,
) -> Result<Envelope<QiResearchResult>> {
    let c: QiContent = content::load(QI_RESEARCH)?;

    let title = optional_text("project_title", &args.project_title, MAX_TITLE)?;
    let description = optional_text("description", &args.description, MAX_TEXT)?;
    let office = optional_text("reviewing_office", &args.reviewing_office, MAX_TITLE)?;

    // Validate and collect answers.
    let valid: BTreeSet<&str> = c.questions.iter().map(|q| q.id.as_str()).collect();
    let mut answers: BTreeMap<String, Answer> = BTreeMap::new();
    let mut notes: BTreeMap<String, String> = BTreeMap::new();
    for a in &args.answers {
        let id = a.question_id.trim().to_lowercase().replace([' ', '-'], "_");
        if !valid.contains(id.as_str()) {
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not a question id. Valid ids: {}",
                a.question_id.chars().take(60).collect::<String>(),
                c.questions
                    .iter()
                    .map(|q| q.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if answers.insert(id.clone(), a.answer).is_some() {
            return Err(PhnError::InvalidInput(format!(
                "question '{id}' is answered more than once"
            )));
        }
        if let Some(n) = optional_text("note", &a.note, MAX_NOTE)? {
            notes.insert(id, n);
        }
    }

    let reading = read_answers(&c.questions, &answers);
    let status = if args.answers.is_empty() {
        Status::QuestionsOnly
    } else {
        reading.status
    };

    let reasoning: Vec<QuestionReasoning> = c
        .questions
        .iter()
        .map(|q| {
            let a = answers.get(&q.id).copied();
            QuestionReasoning {
                id: q.id.clone(),
                stage: q.stage.clone(),
                prompt: q.prompt.clone(),
                help: q.help.clone(),
                needed: q.needed,
                answer: a,
                note: notes.get(&q.id).cloned(),
                effect: effect_of(q, a),
                basis: content::cite_all(&q.basis, &c.sources),
            }
        })
        .collect();

    let classification = if status == Status::Provisional {
        let cat = category(&c, reading.category)?;
        Some(Classification {
            category: cat.id.clone(),
            label: cat.label.clone(),
            summary: cat.summary.split_whitespace().collect::<Vec<_>>().join(" "),
            human_subjects: HumanSubjectsReading {
                status: reading.human_subjects.to_string(),
                note: human_subjects_note(reading.human_subjects)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            },
            research_signals: reading.research_signals.clone(),
            non_research_signals: reading.non_research_signals.clone(),
            cautions: reading.cautions.clone(),
            not_a_determination: "This is a provisional reading from your answers. It is not a \
                                  determination; your institution's IRB decides."
                .into(),
        })
    } else {
        None
    };

    let next_steps: Vec<String> = match &classification {
        Some(cl) => next_steps_for(&c, &cl.category).to_vec(),
        None => vec![
            "Answer the questions marked required (generalizable_intent, new_intervention, \
             extra_burden) with yes or no; answer the others you can."
                .into(),
        ],
    };

    let categories: Vec<CategoryOut> = c
        .categories
        .iter()
        .map(|x| CategoryOut {
            id: x.id.clone(),
            label: x.label.clone(),
            summary: x.summary.split_whitespace().collect::<Vec<_>>().join(" "),
            typical_features: content::cite_all(&x.typical_features, &c.sources),
            irb_note: x.irb_note.split_whitespace().collect::<Vec<_>>().join(" "),
        })
        .collect();
    let definitions: Vec<DefinitionOut> = c
        .definitions
        .iter()
        .map(|d| DefinitionOut {
            term: d.term.clone(),
            quote: d.quote.clone(),
            paraphrase: d
                .paraphrase
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            citation: format!(
                "{}, {}",
                c.sources
                    .get(&d.source)
                    .map(|s| s.title.as_str())
                    .unwrap_or(d.source.as_str()),
                d.section
            ),
            confidence: d.confidence,
        })
        .collect();

    // Draft document (placeholders; nothing about any person).
    let mut files_written = Vec::new();
    let mut document_file = None;
    if args.write_document.unwrap_or(true) {
        let md = document_markdown(
            &c,
            &title,
            &description,
            &office,
            &reasoning,
            &classification,
        );
        let f = write_docx(ctx, &c.document.filename, &c.document.title, &md)?;
        files_written.push(f.clone());
        document_file = Some(f);
    }

    let what = c
        .what_irb_decides
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let summary = summary_text(
        &c,
        status,
        &classification,
        &reasoning,
        &reading,
        &next_steps,
        &document_file,
    );

    // Sources: everything cited by the questions shown, the definitions, and the categories.
    let mut keys: Vec<&str> = Vec::new();
    for q in &c.questions {
        keys.extend(q.basis.iter().map(|b| b.source.as_str()));
    }
    for d in &c.definitions {
        keys.push(d.source.as_str());
    }
    for k in ["cfr46_102", "cdc1999", "cfr46_109", "cfr46_104"] {
        keys.insert(0, k);
    }
    let sources = content::sources_for(keys, &c.sources);

    let mut caveats = vec![what.clone()];
    caveats.extend(c.limits.iter().cloned());
    caveats.extend(content::review_caveats(&[QI_RESEARCH]));

    Ok(Envelope::new(
        summary,
        QiResearchResult {
            status,
            classification,
            reasoning,
            needs_answers: reading.needs_answers,
            unsure: reading.unsure,
            next_steps,
            categories,
            definitions,
            what_the_irb_decides: what,
            document_file,
            limits: c.limits.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        files_written,
        notes: vec![
            "No local files are read. The project title, description, and notes you type appear \
             only in the draft document in /data/output; describe groups, not individual people."
                .into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

fn summary_text(
    c: &QiContent,
    status: Status,
    cl: &Option<Classification>,
    reasoning: &[QuestionReasoning],
    reading: &Reading,
    next_steps: &[String],
    document: &Option<String>,
) -> String {
    let mut s = String::from(
        "**QI, program evaluation, or research? A screening aid (the IRB decides)**\n\n",
    );
    match status {
        Status::QuestionsOnly => {
            s.push_str(
                "Answer these questions with yes, no, or unsure (call again with `answers`). Each \
                 shows why it matters; nothing is decided for you.\n\n",
            );
        }
        Status::NeedsAnswers => {
            s.push_str(&format!(
                "No reading yet: these required questions need a yes or no: **{}**.\n\n",
                reading.needs_answers.join(", ")
            ));
        }
        Status::Provisional => {
            if let Some(cl) = cl {
                s.push_str(&format!(
                    "**Provisional reading from your answers: {}.** {} _This is not a \
                     determination._\n\n",
                    cl.label, cl.summary
                ));
                s.push_str(&format!("Human subjects: {}\n\n", cl.human_subjects.note));
                for x in &cl.cautions {
                    s.push_str(&format!("- Note: {x}\n"));
                }
                if !cl.cautions.is_empty() {
                    s.push('\n');
                }
            }
        }
    }
    let rows: Vec<Vec<String>> = reasoning
        .iter()
        .map(|q| {
            vec![
                q.id.clone(),
                q.prompt.clone(),
                match q.needed {
                    Needed::Required => "required",
                    Needed::Recommended => "recommended",
                    Needed::Optional => "optional",
                }
                .to_string(),
                q.answer
                    .map(|a| a.label().to_string())
                    .unwrap_or_else(|| "-".into()),
                effect_word(q.effect).to_string(),
                q.basis
                    .first()
                    .map(|b| b.citation.clone())
                    .unwrap_or_default(),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Id",
            "Question",
            "Needed",
            "Answer",
            "Reading",
            "Main source",
        ],
        &rows,
    ));
    s.push_str("\n**Next steps**\n\n");
    for n in next_steps {
        s.push_str(&format!("- {n}\n"));
    }
    s.push_str(&format!(
        "\n_{}_\n",
        c.what_irb_decides
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    ));
    if let Some(d) = document {
        s.push_str(&format!(
            "\nDraft project description and IRB determination request written to `{d}`; fill the \
             [NURSE: ...] placeholders or move the content to your institution's form.\n"
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> QiContent {
        content::load(QI_RESEARCH).unwrap()
    }

    fn answers(pairs: &[(&str, Answer)]) -> BTreeMap<String, Answer> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    use Answer::{No, Unsure, Yes};

    fn base_no() -> Vec<(&'static str, Answer)> {
        vec![
            ("generalizable_intent", No),
            ("new_intervention", No),
            ("extra_burden", No),
        ]
    }

    #[test]
    fn content_loads_and_is_consistent() {
        let c = content();
        let ids: BTreeSet<&str> = c.questions.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(ids.len(), c.questions.len(), "duplicate question ids");
        for q in &c.questions {
            for b in &q.basis {
                assert!(
                    c.sources.contains_key(&b.source),
                    "{} -> {}",
                    q.id,
                    b.source
                );
            }
        }
        for d in &c.definitions {
            assert!(c.sources.contains_key(&d.source));
            assert!(d.quote.split_whitespace().count() <= 25, "{}", d.id);
        }
        for cat in [
            "possible_research",
            "qi",
            "program_evaluation",
            "public_health_practice",
            "undetermined",
        ] {
            assert!(c.categories.iter().any(|x| x.id == cat), "{cat}");
        }
        assert_eq!(
            c.questions
                .iter()
                .filter(|q| q.needed == Needed::Required)
                .map(|q| q.id.as_str())
                .collect::<Vec<_>>(),
            vec!["generalizable_intent", "new_intervention", "extra_burden"]
        );
    }

    #[test]
    fn no_reading_without_the_required_answers() {
        let c = content();
        let r = read_answers(&c.questions, &answers(&[("local_improvement", Yes)]));
        assert_eq!(r.status, Status::NeedsAnswers);
        assert_eq!(
            r.needs_answers,
            vec!["generalizable_intent", "new_intervention", "extra_burden"]
        );
        // Unsure counts as not answered for a required question.
        let r = read_answers(
            &c.questions,
            &answers(&[
                ("generalizable_intent", Unsure),
                ("new_intervention", No),
                ("extra_burden", No),
            ]),
        );
        assert_eq!(r.status, Status::NeedsAnswers);
        assert_eq!(r.needs_answers, vec!["generalizable_intent"]);
        assert_eq!(r.unsure, vec!["generalizable_intent"]);
    }

    #[test]
    fn local_improvement_with_no_research_signals_is_qi() {
        let c = content();
        let mut a = base_no();
        a.push(("local_improvement", Yes));
        a.push(("participant_benefit", Yes));
        let r = read_answers(&c.questions, &answers(&a));
        assert_eq!(r.status, Status::Provisional);
        assert_eq!(r.category, "qi");
        assert!(r.research_signals.is_empty());
        assert_eq!(r.human_subjects, "not_applicable");
    }

    #[test]
    fn established_program_feedback_is_program_evaluation() {
        let c = content();
        let mut a = base_no();
        a.push(("established_program", Yes));
        a.push(("local_improvement", Yes));
        let r = read_answers(&c.questions, &answers(&a));
        assert_eq!(r.category, "program_evaluation");
        assert_eq!(r.cautions.len(), 1);
    }

    #[test]
    fn authorized_surveillance_is_public_health_practice() {
        let c = content();
        let mut a = base_no();
        a.push(("public_health_authority", Yes));
        a.push(("beyond_reporting", No));
        let r = read_answers(&c.questions, &answers(&a));
        assert_eq!(r.category, "public_health_practice");
    }

    #[test]
    fn any_research_signal_gives_possible_research() {
        let c = content();
        for (id, v) in [
            ("generalizable_intent", Yes),
            ("new_intervention", Yes),
            ("extra_burden", Yes),
        ] {
            let mut a: Vec<(&str, Answer)> =
                base_no().into_iter().filter(|(k, _)| *k != id).collect();
            a.push((id, v));
            a.push(("local_improvement", Yes));
            let r = read_answers(&c.questions, &answers(&a));
            assert_eq!(r.category, "possible_research", "{id}");
            assert!(r.research_signals.contains(&id.to_string()));
        }
        // Participants not benefiting, and extra data beyond reporting, also point to research.
        let mut a = base_no();
        a.push(("participant_benefit", No));
        assert_eq!(
            read_answers(&c.questions, &answers(&a)).category,
            "possible_research"
        );
        let mut a = base_no();
        a.push(("public_health_authority", Yes));
        a.push(("beyond_reporting", Yes));
        assert_eq!(
            read_answers(&c.questions, &answers(&a)).category,
            "possible_research"
        );
    }

    #[test]
    fn research_outranks_authority_and_flags_the_conflict_and_overlap() {
        let c = content();
        let a = answers(&[
            ("generalizable_intent", Yes),
            ("new_intervention", No),
            ("extra_burden", No),
            ("public_health_authority", Yes),
            ("local_improvement", Yes),
        ]);
        let r = read_answers(&c.questions, &a);
        assert_eq!(r.category, "possible_research");
        assert_eq!(r.cautions.len(), 2, "{:?}", r.cautions);
    }

    #[test]
    fn human_subjects_follow_the_two_regulatory_prongs() {
        let c = content();
        let hs = |extra: &[(&'static str, Answer)]| {
            let mut a = base_no();
            a.retain(|(k, _)| *k != "generalizable_intent");
            a.push(("generalizable_intent", Yes));
            a.extend_from_slice(extra);
            read_answers(&c.questions, &answers(&a)).human_subjects
        };
        assert_eq!(hs(&[("interaction_or_intervention", Yes)]), "likely");
        assert_eq!(
            hs(&[
                ("identifiable_private_information", Yes),
                ("interaction_or_intervention", No)
            ]),
            "likely"
        );
        assert_eq!(
            hs(&[
                ("interaction_or_intervention", No),
                ("identifiable_private_information", No)
            ]),
            "unlikely"
        );
        assert_eq!(hs(&[("interaction_or_intervention", No)]), "unknown");
        assert_eq!(hs(&[]), "unknown");
    }

    #[test]
    fn unsure_on_a_research_question_adds_a_caution_and_publication_is_not_decisive() {
        let c = content();
        let mut a = base_no();
        a.push(("local_improvement", Yes));
        a.push(("participant_benefit", Unsure));
        a.push(("publication_plan", Yes));
        let r = read_answers(&c.questions, &answers(&a));
        assert_eq!(r.category, "qi");
        assert_eq!(r.cautions.len(), 2, "{:?}", r.cautions);
        assert!(r.cautions[1].contains("participant_benefit"));
    }

    #[test]
    fn answers_that_point_nowhere_are_undetermined() {
        let c = content();
        let r = read_answers(&c.questions, &answers(&base_no()));
        assert_eq!(r.status, Status::Provisional);
        assert_eq!(r.category, "undetermined");
    }
}
