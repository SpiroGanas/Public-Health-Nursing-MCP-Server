//! EBP-03 `appraise_evidence`: guided critical appraisal of a study the nurse
//! describes or supplies. The tool identifies the study design from the text
//! (the nurse confirms it), lists the design-specific appraisal questions,
//! records the nurse's answers and applies each instrument's own rule for an
//! overall rating, and structures a GRADE (or GRADE-CERQual) judgment of a
//! body of evidence. It never answers a question itself.
//!
//! Study text pasted or read from a file in /data is scanned for identifiers
//! with the local PHI detector, analysed in memory only, and never returned;
//! only words from this tool's fixed design word list are reported.

use std::collections::BTreeSet;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::crossref::{self, Work};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_local::extract::FileKind;
use phn_local::phi::{self, Identifier, RedactionLevel};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::appraisal::{
    self, AnswerIn, CerqualIn, CerqualOut, ChecklistContent, Concern, DesignCandidate,
    GradeContent, GradeIn, GradeOut, OverallRating, QuestionOut, StudyDesign, Tally,
};
use crate::context::Context;
use crate::epr::content;
use crate::md;
use crate::school::templates;

/// Longest study text read (characters).
const MAX_TEXT_CHARS: usize = 300_000;
/// Largest file read (bytes).
const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

// ---------------------------------------------------------------- arguments and results

/// Arguments for `appraise_evidence`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AppraiseEvidenceArgs {
    /// The study as the nurse describes it: a pasted abstract, methods section, or full text. Used
    /// only to suggest the study design; it is analysed in memory and never returned. Give this
    /// or `path`, or name the design.
    pub text: Option<String>,
    /// A published study (TXT, Markdown, DOCX, PDF, or HTML) in /data, used like `text`. A file
    /// that looks like a client record (Social Security number, date of birth, medical record
    /// number) is refused.
    pub path: Option<String>,
    /// A short label for the study in your record (for example "School asthma program trial").
    /// Defaults to the DOI's title, the file name, or "Study being appraised".
    pub title: Option<String>,
    /// The study's DOI (10.xxxx/...). Looked up in Crossref for the citation when online, which
    /// sends only the DOI; offline, or when Crossref has no record, the DOI is used as entered and
    /// the response says so. The title also helps identify the design.
    pub doi: Option<String>,
    /// The study design, to override or confirm the suggestion. Required when you give answers.
    pub design: Option<StudyDesign>,
    /// Your answers to the design's questions. The tool records them, checks they are allowed,
    /// and applies the instrument's published rule for an overall rating where one exists. Leave
    /// empty first to get the questions.
    pub answers: Vec<AnswerIn>,
    /// Randomized trials only: the question ids of the risk-of-bias domains you treat as key
    /// domains for the overall rating (the 2011 Cochrane tool leaves the choice to you). Default:
    /// all seven.
    pub key_domains: Vec<String>,
    /// A GRADE certainty judgment for a body of evidence on one outcome (not for a single
    /// study): your rating change for each domain, added to the starting level.
    pub grade: Option<GradeIn>,
    /// A GRADE-CERQual assessment of one finding from a synthesis of qualitative studies: your
    /// concern level for each of the four components and your overall confidence.
    pub cerqual: Option<CerqualIn>,
    /// Write your answers and ratings as a Word record to /data/output. Default: true when you
    /// give answers, a GRADE judgment, or a CERQual assessment; otherwise false.
    pub write_record: Option<bool>,
}

/// The DOI and where its details came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DoiInfo {
    /// The DOI as normalized from what was entered.
    pub doi: String,
    pub link: String,
    /// "crossref" when Crossref supplied the details, "doi_as_entered" otherwise.
    pub source_used: String,
    /// Why Crossref was not used (offline mode, no record, or a network problem).
    pub fallback_reason: Option<String>,
    /// "live" or "cache" when Crossref was used.
    pub data_mode: Option<String>,
    pub as_of: Option<String>,
    pub metadata: Option<Work>,
}

/// What was supplied about the study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StudyInfo {
    pub label: String,
    /// "pasted text", the file read in /data, or null when only a DOI or design was given.
    pub text_source: Option<String>,
    /// Words in the text (the text itself is never returned).
    pub words: Option<usize>,
    pub doi: Option<DoiInfo>,
}

/// A design without a checklist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UnsupportedOut {
    pub design: String,
    pub label: String,
    pub pointer: String,
}

/// A design with a checklist, for the nurse's choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DesignOption {
    pub design: String,
    pub label: String,
    pub how_to_tell: String,
    pub instrument: String,
}

/// The design identification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DesignOut {
    /// The design used (null when it could not be identified and you did not give one).
    pub chosen: Option<String>,
    pub label: Option<String>,
    /// "nurse" (you gave it), "text" (suggested from the words), or "none".
    pub basis: String,
    /// For a suggestion: "likely", "possible", or "none". A suggestion is never final: confirm it.
    pub strength: String,
    pub how_to_tell: Option<String>,
    pub instrument: Option<String>,
    /// Designs whose words appear in the text, strongest first.
    pub candidates: Vec<DesignCandidate>,
    pub unsupported: Option<UnsupportedOut>,
    /// Designs that have a checklist.
    pub available_designs: Vec<DesignOption>,
}

/// An answer scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScaleOut {
    pub scale: String,
    pub label: String,
    pub allowed: Vec<String>,
    pub meaning: String,
}

/// The checklist with the nurse's answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistOut {
    pub design: String,
    pub instrument: String,
    pub scales: Vec<ScaleOut>,
    pub questions: Vec<QuestionOut>,
    pub tally: Tally,
    /// Answers that deserve a second look (no, partly, can't tell, unclear, high, serious, ...).
    pub concerns: Vec<Concern>,
    /// The instrument's own overall rating, when it defines one (randomized trials, non-randomized
    /// intervention studies, systematic reviews).
    pub overall: Option<OverallRating>,
}

/// One GRADE domain, as a question to answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradeGuideDomain {
    pub domain: String,
    pub label: String,
    /// "down" (a reason to rate down) or "up".
    pub direction: String,
    pub max_change: i32,
    pub ask: String,
    pub consider: Vec<String>,
}

/// A CERQual component, as a question to answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CerqualGuideComponent {
    pub component: String,
    pub label: String,
    pub definition: String,
}

/// Certainty of a body of evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CertaintyOut {
    /// What a single study can and cannot tell you.
    pub scope_note: String,
    /// The GRADE starting level for the design (high for randomized trials, low for observational
    /// designs); null when it depends on the studies a review includes.
    pub starting_level_for_design: Option<String>,
    pub grade: Option<GradeOut>,
    /// The GRADE domains to judge (when you gave no GRADE or CERQual judgment).
    pub grade_guide: Vec<GradeGuideDomain>,
    pub cerqual: Option<CerqualOut>,
    /// The CERQual components to judge (for qualitative studies, when you gave no judgment).
    pub cerqual_guide: Vec<CerqualGuideComponent>,
    /// Levels a GRADE rating can take.
    pub levels: Vec<String>,
}

/// CASP link and licence note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaspOut {
    pub title: String,
    pub url: String,
    pub licence_note: String,
    pub checklists_note: String,
}

/// Result of `appraise_evidence`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AppraiseEvidenceResult {
    pub study: StudyInfo,
    pub design: DesignOut,
    /// The questions with your answers (null when no design is chosen or the design has no checklist).
    pub checklist: Option<ChecklistOut>,
    pub certainty: Option<CertaintyOut>,
    pub casp: CaspOut,
    /// The Word record written to /data/output.
    pub record_file: Option<String>,
    pub next_steps: Vec<String>,
}

// ---------------------------------------------------------------- DOI lookup

/// What a DOI lookup found (or why it fell back).
#[derive(Debug, Clone)]
pub struct DoiOutcome {
    /// The DOI, normalized.
    pub doi: String,
    pub lookup: std::result::Result<crossref::Lookup, String>,
}

/// Look up the DOI in Crossref, when one was given. A DOI that is not shaped like one is an
/// error; Crossref being unreachable, or having no record, is not: the DOI as entered is used.
pub async fn lookup_doi(ctx: &Context, doi: Option<&str>) -> Result<Option<DoiOutcome>> {
    let Some(raw) = doi.map(str::trim).filter(|d| !d.is_empty()) else {
        return Ok(None);
    };
    let Some(norm) = crossref::normalize_doi(raw) else {
        return Err(PhnError::InvalidInput(
            "doi must look like 10.xxxx/suffix (a leading https://doi.org/ is fine)".into(),
        ));
    };
    if ctx.http.is_offline() {
        return Ok(Some(DoiOutcome {
            doi: norm,
            lookup: Err("offline mode is on".into()),
        }));
    }
    let r = crossref::lookup(&ctx.http, ctx.cache.as_ref(), &norm)
        .await
        .map_err(|e| e.to_string());
    Ok(Some(DoiOutcome { doi: norm, lookup: r }))
}

fn doi_info(o: &DoiOutcome) -> DoiInfo {
    let link = format!("https://doi.org/{}", o.doi);
    match &o.lookup {
        Ok(l) => DoiInfo {
            doi: o.doi.clone(),
            link,
            source_used: "crossref".into(),
            fallback_reason: l.fallback_reason.clone(),
            data_mode: Some(l.mode.label().to_string()),
            as_of: Some(l.as_of.clone()),
            metadata: Some(l.work.clone()),
        },
        Err(reason) => DoiInfo {
            doi: o.doi.clone(),
            link,
            source_used: "doi_as_entered".into(),
            fallback_reason: Some(reason.clone()),
            data_mode: None,
            as_of: None,
            metadata: None,
        },
    }
}

// ---------------------------------------------------------------- reading the study text

struct StudyText {
    text: String,
    label: String,
    from_file: bool,
    notes: Vec<String>,
}

fn read_study_text(ctx: &Context, args: &AppraiseEvidenceArgs) -> Result<Option<StudyText>> {
    let text = args.text.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let path = args.path.as_deref().map(str::trim).filter(|p| !p.is_empty());
    match (text, path) {
        (Some(_), Some(_)) => Err(PhnError::InvalidInput(
            "give either text or path, not both".into(),
        )),
        (Some(t), None) => {
            if t.chars().count() > MAX_TEXT_CHARS {
                return Err(PhnError::InvalidInput(format!(
                    "the text is longer than {MAX_TEXT_CHARS} characters; give the abstract and methods"
                )));
            }
            Ok(Some(StudyText {
                text: t.to_string(),
                label: "pasted text".into(),
                from_file: false,
                notes: Vec::new(),
            }))
        }
        (None, Some(p)) => {
            let sandbox = ctx.sandbox()?;
            let file = sandbox.resolve_existing(p)?;
            let label = sandbox.display_path(&file);
            let kind = FileKind::of(&file);
            if !matches!(
                kind,
                FileKind::Text | FileKind::Markdown | FileKind::Docx | FileKind::Pdf | FileKind::Html
            ) {
                return Err(PhnError::Unsupported(
                    "appraise_evidence reads TXT, Markdown, DOCX, PDF, and HTML files".into(),
                ));
            }
            let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            if size > MAX_FILE_BYTES {
                return Err(PhnError::InvalidInput(format!(
                    "'{label}' is larger than {} MB; give the abstract and methods instead",
                    MAX_FILE_BYTES / (1024 * 1024)
                )));
            }
            let e = phn_local::extract::extract(&file)?;
            if e.text.chars().count() > MAX_TEXT_CHARS {
                return Err(PhnError::InvalidInput(format!(
                    "'{label}' has more than {MAX_TEXT_CHARS} characters; give the abstract and methods"
                )));
            }
            Ok(Some(StudyText {
                text: e.text,
                label,
                from_file: true,
                notes: e.notes,
            }))
        }
        (None, None) => Ok(None),
    }
}

/// Refuse text that looks like a client record, and list the identifier categories found.
fn scan_study_text(text: &str, label: &str) -> Result<Vec<String>> {
    let findings = phi::scan_text(text);
    let record_like: BTreeSet<&str> = findings
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                Identifier::Ssn
                    | Identifier::BirthDate
                    | Identifier::MedicalRecordNumber
                    | Identifier::HealthPlanNumber
            )
        })
        .map(|f| f.kind.label())
        .collect();
    if !record_like.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "{label} contains what look like client identifiers ({}). appraise_evidence is for \
             published studies: remove the client details, or give the study's abstract instead",
            record_like.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    let mut cats: Vec<String> = findings.iter().map(|f| f.kind.label().to_string()).collect();
    cats.sort();
    cats.dedup();
    Ok(cats)
}

// ---------------------------------------------------------------- building the result

fn options(c: &ChecklistContent) -> Vec<DesignOption> {
    c.designs
        .iter()
        .map(|d| DesignOption {
            design: d.id.clone(),
            label: d.label.clone(),
            how_to_tell: d.how_to_tell.clone(),
            instrument: d.instrument.clone(),
        })
        .collect()
}

fn grade_guide(g: &GradeContent) -> Vec<GradeGuideDomain> {
    g.domains
        .iter()
        .map(|d| GradeGuideDomain {
            domain: d.id.clone(),
            label: d.label.clone(),
            direction: d.direction.clone(),
            max_change: d.max_change,
            ask: d.ask.clone(),
            consider: d
                .consider
                .iter()
                .map(|n| format!("{} ({}, {})", n.text, g.title(&n.source), n.section))
                .collect(),
        })
        .collect()
}

fn cerqual_guide(g: &GradeContent) -> Vec<CerqualGuideComponent> {
    g.cerqual
        .components
        .iter()
        .map(|c| CerqualGuideComponent {
            component: c.id.clone(),
            label: c.label.clone(),
            definition: c.definition.clone(),
        })
        .collect()
}

fn answer_label(a: &str) -> String {
    a.replace('_', " ")
}

/// A new paragraph block.
fn para(text: impl Into<String>) -> Block {
    Block::Paragraph(vec![Inline::plain(text)])
}

fn heading(level: u8, text: &str) -> Block {
    Block::Heading {
        level,
        text: text.to_string(),
    }
}

fn bullets(items: Vec<String>) -> Block {
    Block::List {
        ordered: false,
        items: items.into_iter().map(|t| vec![Inline::plain(t)]).collect(),
    }
}

/// The Word record of the nurse's appraisal.
#[allow(clippy::too_many_arguments)]
fn record_document(
    ctx: &Context,
    study: &StudyInfo,
    design: &DesignOut,
    checklist: &Option<ChecklistOut>,
    certainty: &Option<CertaintyOut>,
    caveats: &[String],
    casp: &CaspOut,
) -> Document {
    let today = chrono::Local::now().date_naive().to_string();
    let mut blocks = vec![
        para(format!(
            "Prepared {today}. This record holds the appraiser's own answers and judgments; the \
             tool that produced it did not answer any appraisal question."
        )),
        heading(2, "Study"),
    ];
    let mut lines = vec![format!("Label: {}", study.label)];
    if let Some(src) = &study.text_source {
        lines.push(format!("Text used to suggest the design: {src}"));
    }
    if let Some(d) = &study.doi {
        lines.push(format!("DOI: {} ({})", d.doi, d.link));
        match &d.metadata {
            Some(w) => {
                if let Some(t) = &w.title {
                    lines.push(format!("Title (Crossref): {t}"));
                }
                let mut cite = Vec::new();
                if !w.authors.is_empty() {
                    cite.push(format!(
                        "{}{}",
                        w.authors.join(", "),
                        if w.author_count > w.authors.len() { ", et al." } else { "" }
                    ));
                }
                if let Some(j) = &w.container_title {
                    cite.push(j.clone());
                }
                if let Some(y) = w.year {
                    cite.push(y.to_string());
                }
                if !cite.is_empty() {
                    lines.push(format!("Citation details (Crossref): {}", cite.join("; ")));
                }
            }
            None => lines.push(format!(
                "Citation details: not looked up ({}). The DOI is shown as entered.",
                d.fallback_reason.clone().unwrap_or_default()
            )),
        }
    }
    blocks.push(bullets(lines));
    blocks.push(heading(2, "Design"));
    blocks.push(para(match (&design.label, design.basis.as_str()) {
        (Some(l), "nurse") => format!("{l} (chosen by the appraiser)."),
        (Some(l), _) => format!("{l} (suggested from the text; strength {}; confirm it).", design.strength),
        _ => "Design not determined.".to_string(),
    }));
    if let Some(c) = checklist {
        blocks.push(heading(2, "Checklist and answers"));
        blocks.push(para(format!("Instrument: {}.", c.instrument)));
        blocks.push(Block::Table(TableBlock {
            caption: Some("Appraisal questions and the appraiser's answers".into()),
            headers: ["Domain", "Question", "Answer", "Note", "Source"]
                .map(String::from)
                .to_vec(),
            rows: c
                .questions
                .iter()
                .map(|q| {
                    vec![
                        q.domain.clone(),
                        q.text.clone(),
                        q.answer.as_deref().map(answer_label).unwrap_or_else(|| "not answered".into()),
                        q.note.clone().unwrap_or_default(),
                        q.citation.clone(),
                    ]
                })
                .collect(),
        }));
        blocks.push(para(format!(
            "{} of {} questions answered.",
            c.tally.answered, c.tally.questions
        )));
        if !c.concerns.is_empty() {
            blocks.push(heading(3, "Answers to look at again"));
            blocks.push(bullets(
                c.concerns
                    .iter()
                    .map(|k| format!("{} ({}): {}", k.question, k.domain, answer_label(&k.answer)))
                    .collect(),
            ));
        }
        if let Some(o) = &c.overall {
            blocks.push(heading(3, &o.method));
            blocks.push(para(format!("Rule: {}", o.rule)));
            blocks.push(para(match (&o.rating, o.provisional) {
                (Some(r), true) => format!("Rating so far: {} (provisional; {} unanswered). {}", answer_label(r), o.unanswered.len(), o.basis),
                (Some(r), false) => format!("Rating: {}. {}", answer_label(r), o.basis),
                (None, _) => format!("No rating yet. {}", o.basis),
            }));
            blocks.push(para(o.note.clone()));
        }
    }
    if let Some(ct) = certainty {
        if let Some(g) = &ct.grade {
            blocks.push(heading(2, "GRADE certainty of evidence"));
            blocks.push(para(format!(
                "Outcome: {}. Evidence type: {} (starts {}). Sum of changes {:+}: {}.{}",
                g.outcome,
                answer_label(&g.evidence_type),
                answer_label(&g.start),
                g.total_change,
                g.computed_label,
                if g.final_differs {
                    format!(" The appraiser's final rating is {} ({}).", answer_label(&g.final_rating), g.final_reason.clone().unwrap_or_default())
                } else {
                    String::new()
                }
            )));
            if !g.domains.is_empty() {
                blocks.push(Block::Table(TableBlock {
                    caption: Some("GRADE domain judgments".into()),
                    headers: ["Domain", "Change (levels)", "Reason"].map(String::from).to_vec(),
                    rows: g
                        .domains
                        .iter()
                        .map(|d| vec![d.label.clone(), format!("{:+}", d.change), d.reason.clone().unwrap_or_default()])
                        .collect(),
                }));
            }
        }
        if let Some(c) = &ct.cerqual {
            blocks.push(heading(2, "GRADE-CERQual confidence in a qualitative finding"));
            blocks.push(para(format!("Finding: {}", c.finding)));
            blocks.push(Block::Table(TableBlock {
                caption: Some("CERQual components".into()),
                headers: ["Component", "Concern", "Reason"].map(String::from).to_vec(),
                rows: c
                    .components
                    .iter()
                    .map(|k| {
                        vec![
                            k.label.clone(),
                            k.concern_label.clone().unwrap_or_else(|| "not assessed".into()),
                            k.reason.clone().unwrap_or_default(),
                        ]
                    })
                    .collect(),
            }));
            blocks.push(para(match &c.overall_confidence {
                Some(o) => format!("Overall: {o}. {}", c.overall_definition.clone().unwrap_or_default()),
                None => "Overall confidence: not yet judged.".into(),
            }));
        }
    }
    blocks.push(heading(2, "About this record"));
    blocks.push(bullets(caveats.to_vec()));
    blocks.push(para(format!(
        "CASP checklists are available at {}. {}",
        casp.url, casp.licence_note
    )));
    Document {
        title: format!("Critical appraisal record: {}", study.label),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

/// Run the appraisal. `doi` is the outcome of [`lookup_doi`] for `args.doi`.
pub fn appraise_evidence(
    ctx: &Context,
    args: AppraiseEvidenceArgs,
    doi: Option<DoiOutcome>,
) -> Result<Envelope<AppraiseEvidenceResult>> {
    let c: ChecklistContent = content::load(appraisal::CHECKLISTS)?;
    let g: GradeContent = content::load(appraisal::GRADE)?;
    let has_answers = !args.answers.is_empty();
    let has_certainty = args.grade.is_some() || args.cerqual.is_some();
    if args.text.as_deref().is_none_or(|t| t.trim().is_empty())
        && args.path.as_deref().is_none_or(|p| p.trim().is_empty())
        && args.doi.as_deref().is_none_or(|d| d.trim().is_empty())
        && args.design.is_none()
    {
        return Err(PhnError::InvalidInput(
            "give the study as text or a file path (or a DOI), or name the design to get its \
             appraisal questions"
                .into(),
        ));
    }
    if (has_answers || !args.key_domains.is_empty()) && args.design.is_none() {
        return Err(PhnError::InvalidInput(
            "give the design (the `design` argument) with your answers: the question ids belong to a design, and the text-based suggestion is never final".into(),
        ));
    }
    if args.grade.is_some() && args.design == Some(StudyDesign::Qualitative) {
        return Err(PhnError::InvalidInput(
            "GRADE rates evidence about effects; for findings from qualitative studies use cerqual"
                .into(),
        ));
    }

    // The study text (analysed here, never returned).
    let study_text = read_study_text(ctx, &args)?;
    let mut identifiers: Vec<String> = Vec::new();
    let mut analysis_text = String::new();
    if let Some(t) = &study_text {
        identifiers = scan_study_text(&t.text, &t.label)?;
        analysis_text = phi::redact_text(&t.text, RedactionLevel::Strict).text;
    }
    let doi_out = doi.as_ref().map(doi_info);
    if let Some(Ok(l)) = doi.as_ref().map(|o| &o.lookup) {
        // Public metadata: the title helps identify the design.
        for t in [&l.work.title, &l.work.container_title].into_iter().flatten() {
            analysis_text.push('\n');
            analysis_text.push_str(t);
        }
    }

    // Design.
    let mut candidates = if analysis_text.trim().is_empty() {
        Vec::new()
    } else {
        appraisal::identify_design(&c, &analysis_text)
    };
    let strength = appraisal::identification_strength(&candidates).to_string();
    let (chosen, basis, unsupported) = match args.design {
        Some(d) => (Some(d.id().to_string()), "nurse", None),
        None => match candidates.first() {
            Some(top) if top.has_checklist => (Some(top.design.clone()), "text", None),
            Some(top) => {
                let u = c.unsupported_designs.iter().find(|u| u.id == top.design);
                (
                    None,
                    "text",
                    u.map(|u| UnsupportedOut {
                        design: u.id.clone(),
                        label: u.label.clone(),
                        pointer: u.pointer.clone(),
                    }),
                )
            }
            None => (None, "none", None),
        },
    };
    let def = chosen.as_deref().and_then(|id| c.design(id));
    let design_out = DesignOut {
        chosen: chosen.clone(),
        label: def.map(|d| d.label.clone()),
        basis: basis.to_string(),
        strength: if basis == "nurse" { "confirmed by you".into() } else { strength.clone() },
        how_to_tell: def.map(|d| d.how_to_tell.clone()),
        instrument: def.map(|d| d.instrument.clone()),
        candidates: std::mem::take(&mut candidates),
        unsupported,
        available_designs: options(&c),
    };

    // Checklist and answers.
    let redact = |note: &str| -> String {
        if phi::scan_text(note).iter().any(|f| f.kind.is_direct()) {
            phi::redact_text(note, RedactionLevel::Strict).text
        } else {
            note.to_string()
        }
    };
    let mut notes_redacted = false;
    let checklist = match def {
        Some(d) => {
            let flagged = |s: &str| phi::scan_text(s).iter().any(|f| f.kind.is_direct());
            notes_redacted = args
                .answers
                .iter()
                .any(|a| a.note.as_deref().is_some_and(flagged));
            let qs = appraisal::apply_answers(&c, &d.id, &args.answers, &redact)?;
            let tally = appraisal::tally(&qs);
            let concerns = appraisal::concerns(&c, &qs);
            let overall = appraisal::overall_rating(&c, d, &qs, &args.key_domains)?;
            let mut used: Vec<&str> = qs.iter().map(|q| q.answer_scale.as_str()).collect();
            used.sort();
            used.dedup();
            Some(ChecklistOut {
                design: d.id.clone(),
                instrument: d.instrument.clone(),
                scales: used
                    .into_iter()
                    .map(|s| {
                        let sc = &c.scales[s];
                        ScaleOut {
                            scale: s.to_string(),
                            label: sc.label.clone(),
                            allowed: sc.values.clone(),
                            meaning: sc.meaning.clone(),
                        }
                    })
                    .collect(),
                questions: qs,
                tally,
                concerns,
                overall,
            })
        }
        None => None,
    };

    // Certainty.
    let qualitative = chosen.as_deref() == Some("qualitative");
    let start_for_design = def.map(|d| d.starts_certainty.as_str()).filter(|s| *s != "none");
    let certainty = if chosen.is_none() && !has_certainty {
        None
    } else {
        let grade_out = match &args.grade {
            Some(gi) => Some(appraisal::grade_rating(&g, gi, start_for_design)?),
            None => None,
        };
        let cerqual_out = match &args.cerqual {
            Some(ci) => Some(appraisal::cerqual_assessment(&g, ci)?),
            None => None,
        };
        Some(CertaintyOut {
            scope_note: g.scope_note.split_whitespace().collect::<Vec<_>>().join(" "),
            starting_level_for_design: start_for_design.map(str::to_string),
            grade_guide: if args.grade.is_none() && args.cerqual.is_none() && !qualitative {
                grade_guide(&g)
            } else {
                Vec::new()
            },
            cerqual_guide: if args.grade.is_none() && args.cerqual.is_none() && qualitative {
                cerqual_guide(&g)
            } else {
                Vec::new()
            },
            grade: grade_out,
            cerqual: cerqual_out,
            levels: g.levels.iter().map(|l| l.label.clone()).collect(),
        })
    };

    // Study label and details.
    let label = args
        .title
        .as_deref()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .map(|t| t.chars().take(150).collect::<String>())
        .or_else(|| {
            doi.as_ref()
                .and_then(|o| o.lookup.as_ref().ok())
                .and_then(|l| l.work.title.clone())
        })
        .or_else(|| {
            study_text.as_ref().filter(|t| t.from_file).map(|t| {
                t.label.rsplit('/').next().unwrap_or(&t.label).to_string()
            })
        })
        .unwrap_or_else(|| "Study being appraised".into());
    let study = StudyInfo {
        label,
        text_source: study_text.as_ref().map(|t| t.label.clone()),
        words: study_text.as_ref().map(|t| t.text.split_whitespace().count()),
        doi: doi_out,
    };

    // Caveats.
    let mut caveats: Vec<String> = vec![
        "This tool supports your own critical appraisal. It records your answers and applies each \
         instrument's published rule; it never answers a question for you, and appraisal \
         judgments (including GRADE) are not calculations."
            .into(),
        format!(
            "These are not the CASP checklists: the questions are this project's own wording of \
             items from the open-access sources named on each question. {}",
            c.casp.licence_note.split_whitespace().collect::<Vec<_>>().join(" ")
        ),
    ];
    if let Some(d) = def {
        match d.overall_method.as_str() {
            "rob_table3" => caveats.push(
                "Randomized trial questions follow the 2011 Cochrane risk-of-bias tool, whose \
                 successor (RoB 2) was not read for this tool; the 2011 paper says to judge each \
                 main outcome and not to use quality scores."
                    .into(),
            ),
            "robins_table2" => caveats.push(
                "The non-randomized questions are ROBINS-I's seven bias domains, not its \
                 signalling questions (in the paper's supplement, not read); use the full tool \
                 for a formal assessment."
                    .into(),
            ),
            "amstar2_box2" => caveats.push(
                "AMSTAR 2's authors recommend against adding item ratings into a score and call \
                 its overall-confidence scheme advisory; how the tool counts a 'partial yes' was \
                 not verified (the user guide was not read), so partial yes answers are listed, \
                 not counted."
                    .into(),
            ),
            _ => {}
        }
        if matches!(d.id.as_str(), "cohort" | "case_control" | "cross_sectional" | "quality_improvement") {
            caveats.push(
                "STROBE and SQUIRE 2.0 are reporting guidelines; their authors say they are not \
                 quality scores. The answers show what the report tells you and what you must \
                 judge for yourself; no total is calculated."
                    .into(),
            );
        }
        if d.id == "qualitative" {
            caveats.push(
                "The qualitative questions follow one health-education appraisal guide, not a \
                 published instrument (confidence medium); CASP and other qualitative checklists \
                 differ."
                    .into(),
            );
        }
    }
    if basis == "text" && chosen.is_some() {
        caveats.push(format!(
            "The design was suggested from words in the text (strength: {strength}). Confirm it \
             with the `design` argument before relying on the checklist; matched words are listed, \
             and the text itself is never returned."
        ));
    }
    if chosen.is_none() && design_out.unsupported.is_none() {
        caveats.push(
            "The design could not be identified from the text. Choose one with the `design` \
             argument (the available designs are listed)."
                .into(),
        );
    }
    if let Some(d) = &study.doi {
        if d.source_used == "doi_as_entered" {
            caveats.push(format!(
                "Crossref was not used ({}); the DOI is shown as entered and no citation details \
                 were looked up.",
                d.fallback_reason.clone().unwrap_or_default()
            ));
        } else if let Some(r) = &d.fallback_reason {
            caveats.push(format!("Crossref details came from {}: {r}.", d.data_mode.clone().unwrap_or_default()));
        }
    }
    if certainty.as_ref().is_some_and(|c| c.grade.is_some()) {
        caveats.push(
            "GRADE rates a body of evidence for one outcome, not a single study. The sums use the \
             rating changes you chose; the GRADE content was drawn from journal abstracts and may \
             not state every detail (see the content review note)."
                .into(),
        );
    }
    if notes_redacted {
        caveats.push(
            "A note you gave looked like it held a personal identifier and was redacted in the \
             response and the record."
                .into(),
        );
    }
    if let Some(t) = &study_text {
        caveats.extend(t.notes.iter().cloned());
    }
    caveats.extend(content::review_caveats(&[appraisal::CHECKLISTS, appraisal::GRADE]));

    // Record file.
    let casp = CaspOut {
        title: c.casp.title.clone(),
        url: c.casp.url.clone(),
        licence_note: c.casp.licence_note.split_whitespace().collect::<Vec<_>>().join(" "),
        checklists_note: c.casp.checklists_note.split_whitespace().collect::<Vec<_>>().join(" "),
    };
    let write = args.write_record.unwrap_or(has_answers || has_certainty);
    let mut files_written = Vec::new();
    let mut record_file = None;
    if write {
        if checklist.is_none() && !has_certainty {
            return Err(PhnError::InvalidInput(
                "there is nothing to record yet: choose a design and give answers".into(),
            ));
        }
        let doc = record_document(ctx, &study, &design_out, &checklist, &certainty, &caveats, &casp);
        let (dir, _) = templates::output_folder(ctx, "evidence_appraisal")?;
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let f = templates::write_in(ctx, &dir, "appraisal_record.docx", &bytes)?;
        files_written.push(f.clone());
        record_file = Some(f);
    }

    // Next steps.
    let mut next_steps = Vec::new();
    match (&checklist, chosen.as_deref()) {
        (Some(cl), _) if cl.tally.answered == 0 => next_steps.push(
            "Read the study, then call this tool again with `design` and your `answers` (question \
             id and answer for each). Unanswered questions stay listed."
                .into(),
        ),
        (Some(cl), _) if cl.tally.unanswered > 0 => next_steps.push(format!(
            "{} question(s) are still unanswered; add them to `answers` to complete the record.",
            cl.tally.unanswered
        )),
        _ => {}
    }
    if chosen.is_none() {
        next_steps.push("Name the design (`design`) to get its questions.".into());
    }
    if !has_certainty && chosen.is_some() {
        next_steps.push(if qualitative {
            "If you are summarising qualitative studies, judge each CERQual component and pass \
             `cerqual`."
                .into()
        } else {
            "If you are summarising several studies for one outcome, judge each GRADE domain and \
             pass `grade`."
                .into()
        });
    }

    // Summary.
    let mut s = format!("**Critical appraisal: {}**\n\n", study.label);
    match (&design_out.label, basis) {
        (Some(l), "nurse") => s.push_str(&format!("Design: **{l}** (your choice). Instrument: {}.\n\n", design_out.instrument.clone().unwrap_or_default())),
        (Some(l), _) => s.push_str(&format!(
            "Design suggested from the text: **{l}** (strength: {strength}); confirm it. Instrument: {}.\n\n",
            design_out.instrument.clone().unwrap_or_default()
        )),
        _ => {
            if let Some(u) = &design_out.unsupported {
                s.push_str(&format!(
                    "The text suggests a **{}**, which has no checklist here. {}\n\n",
                    u.label, u.pointer
                ));
            } else {
                s.push_str("The design could not be identified; choose one of:\n\n");
                for o in &design_out.available_designs {
                    s.push_str(&format!("- `{}`: {}\n", o.design, o.label));
                }
                s.push('\n');
            }
        }
    }
    if let Some(d) = &study.doi {
        match &d.metadata {
            Some(w) => s.push_str(&format!(
                "DOI {} (Crossref, {}): {}{}.\n\n",
                d.doi,
                d.data_mode.clone().unwrap_or_default(),
                w.title.clone().unwrap_or_default(),
                w.year.map(|y| format!(", {y}")).unwrap_or_default()
            )),
            None => s.push_str(&format!("DOI {} (as entered; not looked up).\n\n", d.doi)),
        }
    }
    if let Some(cl) = &checklist {
        s.push_str(&format!(
            "{} of {} questions answered.\n\n",
            cl.tally.answered, cl.tally.questions
        ));
        let rows: Vec<Vec<String>> = cl
            .questions
            .iter()
            .map(|q| {
                vec![
                    q.id.clone(),
                    q.text.clone(),
                    q.answer.as_deref().map(answer_label).unwrap_or_else(|| format!("({})", q.allowed_answers.join(" / ").replace('_', " "))),
                ]
            })
            .collect();
        s.push_str(&md::table(&["Question id", "Question", "Your answer"], &rows));
        if !cl.concerns.is_empty() {
            s.push_str("\n**Look again at:** ");
            s.push_str(
                &cl.concerns
                    .iter()
                    .map(|k| format!("{} ({})", k.question, answer_label(&k.answer)))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
            s.push('\n');
        }
        if let Some(o) = &cl.overall {
            s.push_str(&format!("\n**{}:** ", o.method));
            match (&o.rating, o.provisional) {
                (Some(r), true) => s.push_str(&format!("{} so far (provisional). {}", answer_label(r), o.basis)),
                (Some(r), false) => s.push_str(&format!("**{}**. {}", answer_label(r), o.basis)),
                (None, _) => s.push_str(&format!("not yet. {}", o.basis)),
            }
            s.push('\n');
        }
    }
    if let Some(ct) = &certainty {
        if let Some(go) = &ct.grade {
            s.push_str(&format!(
                "\n**GRADE certainty for \"{}\":** {} (starts {}, changes {:+}).{}\n",
                go.outcome,
                go.computed_label,
                answer_label(&go.start),
                go.total_change,
                if go.final_differs {
                    format!(" Your final rating: {}.", answer_label(&go.final_rating))
                } else {
                    String::new()
                }
            ));
        } else if !ct.grade_guide.is_empty() {
            s.push_str("\n**GRADE (for a body of evidence on one outcome):** judge these domains and pass `grade`: ");
            s.push_str(
                &ct.grade_guide
                    .iter()
                    .map(|d| format!("{} ({})", d.domain, d.direction))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            s.push_str(".\n");
        }
        if let Some(co) = &ct.cerqual {
            s.push_str(&format!(
                "\n**GRADE-CERQual for the finding:** {}.\n",
                co.overall_confidence.clone().unwrap_or_else(|| "overall confidence not yet judged".into())
            ));
        } else if !ct.cerqual_guide.is_empty() {
            s.push_str("\n**GRADE-CERQual (for findings from a synthesis of qualitative studies):** judge methodological_limitations, coherence, adequacy, and relevance, then pass `cerqual`.\n");
        }
    }
    s.push_str(&format!("\nCASP checklists (link only): {}\n", casp.url));
    if let Some(f) = &record_file {
        s.push_str(&format!("\nRecord written to {f}.\n"));
    }

    // Sources.
    let mut sources: Vec<Source> = Vec::new();
    let mut keys: Vec<&str> = Vec::new();
    if let Some(cl) = &checklist {
        for q in c.questions_for(&cl.design) {
            keys.push(q.source.as_str());
        }
        if let Some(d) = def
            && let Some(m) = c.overall_methods.get(&d.overall_method)
        {
            keys.push(m.source.as_str());
        }
    }
    let mut seen = BTreeSet::new();
    for k in keys {
        if seen.insert(k)
            && let Some(src) = c.sources.get(k)
        {
            sources.push(src.to_source());
        }
    }
    if certainty.is_some() {
        let mut gk: Vec<&str> = Vec::new();
        if let Some(ct) = &certainty {
            if ct.grade.is_some() || !ct.grade_guide.is_empty() {
                for st in &g.starting_levels {
                    gk.push(st.source.as_str());
                }
                for d in &g.domains {
                    for n in &d.consider {
                        gk.push(n.source.as_str());
                    }
                }
                for n in &g.overall_notes {
                    gk.push(n.source.as_str());
                }
            }
            if ct.cerqual.is_some() || !ct.cerqual_guide.is_empty() {
                gk.push(g.cerqual.source.as_str());
                gk.push(g.cerqual.definitions_source.as_str());
                gk.push(g.cerqual.confidence_source.as_str());
            }
        }
        for k in gk {
            if seen.insert(k)
                && let Some(src) = g.sources.get(k)
            {
                sources.push(src.to_source());
            }
        }
    }
    if let Some(Ok(l)) = doi.as_ref().map(|o| &o.lookup) {
        sources.push(Source {
            title: format!("Crossref REST API, work record for DOI {}", l.work.doi),
            publisher: "Crossref".into(),
            url: Some(l.work.url.clone()),
            effective_date: None,
            retrieved: Some(phn_data::fetch::local_date(&l.as_of)),
            mode: l.mode,
        });
    }
    if let Some(t) = &study_text
        && t.from_file
    {
        sources.push(Source::local_file(&t.label));
    }

    let mut privacy = Privacy {
        row_level_data: false,
        identifiers_detected: identifiers.clone(),
        files_written,
        ..Privacy::none()
    };
    if let Some(t) = &study_text {
        privacy.notes.push(format!(
            "The text from {} was analysed in memory to suggest the design; none of it is returned \
             (only words from the tool's fixed design word list).",
            if t.from_file { "the file" } else { "the pasted text" }
        ));
    }
    if !identifiers.is_empty() {
        privacy.notes.push(
            "Possible identifiers were found in the text (in a published paper these are usually \
             author names, emails, or web addresses). They were removed before analysis."
                .into(),
        );
    }
    if write {
        privacy.notes.push(
            "The Word record holds only your answers, notes, and the study label you gave; it was \
             written to /data/output."
                .into(),
        );
    }
    if let Some(d) = &study.doi {
        privacy.notes.push(if d.source_used == "crossref" {
            "Only the DOI was sent to Crossref; no study text, notes, or other details were sent."
                .into()
        } else {
            "Nothing was sent to Crossref.".into()
        });
    }

    Ok(Envelope::new(
        s,
        AppraiseEvidenceResult {
            study,
            design: design_out,
            checklist,
            certainty,
            casp,
            record_file,
            next_steps,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy)
    .decision_support())
}
