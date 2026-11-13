//! POL-04 `draft_policy_brief`: drafts a one-page or longer policy brief (DOCX and/or Markdown) in
//! the order of the Eightfold Path of policy analysis (Bardach, as described in open-access
//! articles): problem, evidence, options, criteria, options compared against the criteria (an
//! accessible table), trade-offs, recommendation. It is a template with local fill: the structure
//! and prompts come from the content; every fact, number, and citation is text the nurse supplies
//! or a `[NURSE: ...]` placeholder. The tool never invents facts or statistics. It runs the
//! readability check on the prose the nurse supplied.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::comms::readability::{AnalyzeOptions, analyze, default_target_grade};
use crate::context::Context;
use crate::epr::content::{self, Confidence, DocSource};
use crate::md;
use crate::policy::shared::{clean_text, refuse_identifiers, write_docx};

/// Content id of `content/policy/policy_brief.yaml`.
pub const BRIEF: &str = "policy-policy-brief";

const MAX_TITLE: usize = 150;
const MAX_TEXT: usize = 2000;
const MAX_SHORT: usize = 300;
const MAX_EVIDENCE: usize = 12;
const MAX_OPTIONS: usize = 6;
const MAX_CRITERIA: usize = 8;
const MAX_RATINGS: usize = 60;
const MIN_WORDS_FOR_READABILITY: usize = 30;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadScope {
    pub read_in_full: Vec<String>,
    pub read_in_part: Vec<String>,
    pub not_read: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceQuote {
    pub id: String,
    pub source: String,
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDef {
    pub number: u8,
    pub name: String,
    pub description: String,
    pub brief_section: String,
    pub confidence: Confidence,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CriterionDef {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub source: Option<String>,
    pub source_kind: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionDef {
    pub id: String,
    pub heading: String,
    pub step: u8,
    pub one_page: bool,
    pub prompt: String,
    pub placeholder: String,
}

/// `content/policy/policy_brief.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub read_scope: ReadScope,
    pub evidence_quotes: Vec<EvidenceQuote>,
    pub steps: Vec<StepDef>,
    pub criteria: Vec<CriterionDef>,
    pub rating_scale: Vec<Choice>,
    pub sections: Vec<SectionDef>,
    pub audiences: Vec<Choice>,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- arguments

/// Length of the brief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BriefLength {
    /// One page: summary, problem, evidence, options, the comparison table, recommendation.
    #[default]
    OnePage,
    /// Longer brief: adds criteria, trade-offs, implementation and evaluation, references.
    Full,
}

/// Output files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BriefOutput {
    /// A Word document in /data/output (default).
    #[default]
    Docx,
    /// A Markdown file in /data/output.
    Markdown,
    /// Both.
    Both,
    /// Only the summary in the response; no file.
    None,
}

/// A rating for a cell of the comparison table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Strong,
    Mixed,
    Weak,
    Unknown,
}

impl Rating {
    fn label(self) -> &'static str {
        match self {
            Rating::Strong => "Strong",
            Rating::Mixed => "Mixed",
            Rating::Weak => "Weak",
            Rating::Unknown => "Unknown",
        }
    }
}

/// A piece of evidence you supply (for example copied from the output of
/// `find_evidence_based_interventions` or `build_evidence_synthesis_table`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EvidenceIn {
    /// The finding in plain words, with any number exactly as the source gives it.
    pub finding: String,
    /// Where it comes from (journal or agency, year, link). Leave empty to get a placeholder.
    pub citation: Option<String>,
    /// How strong you judge it to be (for example "systematic review", "local survey").
    pub strength: Option<String>,
}

/// An option you supply.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct OptionIn {
    /// A short name ("Adopt a flavored tobacco restriction").
    pub name: String,
    /// What the option is, in a sentence or two.
    pub description: Option<String>,
}

/// A rating of one option against one criterion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RatingIn {
    /// The option's name as given in `options`.
    pub option: String,
    /// The criterion (an id or name from the menu, or one you added in `criteria`).
    pub criterion: String,
    pub rating: Rating,
    /// Why, in a sentence, with the evidence it rests on.
    pub reason: Option<String>,
}

/// Arguments for `draft_policy_brief`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct BriefArgs {
    /// Working title of the brief.
    pub title: Option<String>,
    /// one_page (default) or full.
    pub length: Option<BriefLength>,
    /// Who will read it: board_of_health, select_board_or_council, school_committee,
    /// agency_leadership, legislators, other (or your own words).
    pub audience: Option<String>,
    /// The problem, in your words: what is happening, to whom, and how you know.
    pub problem: Option<String>,
    /// Evidence you have gathered. The tool does not search for evidence or check it.
    pub evidence: Vec<EvidenceIn>,
    /// The options to compare (two to four is usual; include continuing the current approach).
    pub options: Vec<OptionIn>,
    /// Criteria to compare them on: ids or names from the menu (health_impact, equity,
    /// economic_impact, safety, feasibility, justice_inclusion, affordability,
    /// political_acceptability, legal_authority, evidence_strength) or your own words. Default:
    /// health_impact, equity, economic_impact, feasibility.
    pub criteria: Vec<String>,
    /// Ratings for cells of the comparison table; cells you leave out get a placeholder.
    pub ratings: Vec<RatingIn>,
    /// What you ask the reader to decide.
    pub request: Option<String>,
    /// The trade-offs between the options (full brief).
    pub tradeoffs: Option<String>,
    /// Your recommendation, with first steps.
    pub recommendation: Option<String>,
    /// What to write: docx (default), markdown, both, or none.
    pub output: Option<BriefOutput>,
}

// ---------------------------------------------------------------- results

/// How a step of the Eightfold Path stands in the draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StepOut {
    pub number: u8,
    pub name: String,
    pub description: String,
    /// Section of the brief where the step appears.
    pub brief_section: String,
    /// "supplied", "partly supplied", or "placeholder".
    pub status: String,
}

/// A section of the draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionOut {
    pub id: String,
    pub heading: String,
    pub step: u8,
    /// "supplied", "partly supplied", or "placeholder".
    pub status: String,
    pub placeholders: usize,
}

/// A criterion in the draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CriterionOut {
    pub id: String,
    pub name: String,
    pub prompt: String,
    /// Where the criterion comes from (a published analysis that used it, this project's
    /// suggestion, or you).
    pub origin: String,
}

/// One row of the comparison table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MatrixRow {
    pub option: String,
    /// One cell per criterion, in order: the rating and reason, or a placeholder.
    pub cells: Vec<String>,
}

/// The comparison table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MatrixOut {
    pub criteria: Vec<String>,
    pub rows: Vec<MatrixRow>,
    /// Always true: ratings are words, so the table can be read without color.
    pub accessible: bool,
}

/// The readability check of the prose you supplied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReadabilityOut {
    pub checked: bool,
    pub words: usize,
    pub sentences: usize,
    /// Median of the Flesch-Kincaid, SMOG, and Fog grades.
    pub median_grade: Option<f64>,
    pub target_grade: f64,
    pub meets_target: Option<bool>,
    pub long_sentences: usize,
    pub passive_sentences: usize,
    /// Plain-language suggestions, or why the check was not run.
    pub notes: Vec<String>,
}

/// Result of `draft_policy_brief`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BriefResult {
    pub title: String,
    pub length: String,
    pub audience: String,
    pub eightfold_path: Vec<StepOut>,
    pub sections: Vec<SectionOut>,
    pub criteria: Vec<CriterionOut>,
    pub comparison: MatrixOut,
    /// Number of [NURSE: ...] placeholders left in the draft.
    pub placeholders_remaining: usize,
    pub readability: ReadabilityOut,
    /// Files written to /data/output (display paths).
    pub document_files: Vec<String>,
    /// What the method sources say, quoted (25 words or fewer each).
    pub method_quotes: Vec<String>,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- helpers

fn text(label: &str, t: &Option<String>, max: usize) -> Result<Option<String>> {
    match t {
        Some(s) if !s.trim().is_empty() => {
            let c = clean_text(label, s, max)?;
            refuse_identifiers(label, &c)?;
            Ok(Some(c))
        }
        _ => Ok(None),
    }
}

fn norm(s: &str) -> String {
    phn_core::text::normalize(&s.replace('_', " "))
}

/// A sentence-ending period if the text has no end punctuation.
fn ended(s: &str) -> String {
    let t = s.trim();
    if t.ends_with(['.', '!', '?']) {
        t.to_string()
    } else {
        format!("{t}.")
    }
}

fn placeholders_in(s: &str) -> usize {
    s.matches("[NURSE:").count()
}

struct ResolvedCriterion {
    id: String,
    name: String,
    prompt: String,
    origin: String,
}

fn resolve_criteria(c: &BriefContent, asked: &[String]) -> Result<Vec<ResolvedCriterion>> {
    let defaults = ["health_impact", "equity", "economic_impact", "feasibility"];
    let list: Vec<String> = if asked.is_empty() {
        defaults.iter().map(|s| s.to_string()).collect()
    } else {
        asked.to_vec()
    };
    let mut out: Vec<ResolvedCriterion> = Vec::new();
    for raw in &list {
        let n = norm(raw);
        let found = c
            .criteria
            .iter()
            .find(|k| norm(&k.id) == n || norm(&k.name) == n);
        let r = match found {
            Some(k) => ResolvedCriterion {
                id: k.id.clone(),
                name: k.name.clone(),
                prompt: k.prompt.clone(),
                origin: match &k.source {
                    Some(src) => format!(
                        "{} ({})",
                        k.source_kind,
                        c.sources.get(src).map(|s| s.title.as_str()).unwrap_or(src)
                    ),
                    None => k.source_kind.clone(),
                },
            },
            None => ResolvedCriterion {
                id: n.replace(' ', "_"),
                name: raw.clone(),
                prompt: "[NURSE: what this criterion means for this decision]".into(),
                origin: "your own criterion".into(),
            },
        };
        if !out.iter().any(|x| x.id == r.id) {
            out.push(r);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- the tool

/// Draft the brief.
pub fn draft_policy_brief(ctx: &Context, args: BriefArgs) -> Result<Envelope<BriefResult>> {
    let c: BriefContent = content::load(BRIEF)?;
    let length = args.length.unwrap_or_default();
    let output = args.output.unwrap_or_default();

    let title = text("title", &args.title, MAX_TITLE)?
        .unwrap_or_else(|| "[NURSE: policy brief title]".into());
    let audience_raw = text("audience", &args.audience, MAX_SHORT)?;
    let audience = audience_raw
        .as_deref()
        .map(|a| {
            c.audiences
                .iter()
                .find(|x| norm(&x.value) == norm(a) || norm(&x.label) == norm(a))
                .map(|x| x.label.clone())
                .unwrap_or_else(|| a.to_string())
        })
        .unwrap_or_else(|| "[NURSE: audience]".into());
    let problem = text("problem", &args.problem, MAX_TEXT)?;
    let request = text("request", &args.request, MAX_TEXT)?;
    let tradeoffs = text("tradeoffs", &args.tradeoffs, MAX_TEXT)?;
    let recommendation = text("recommendation", &args.recommendation, MAX_TEXT)?;

    if args.evidence.len() > MAX_EVIDENCE {
        return Err(PhnError::InvalidInput(format!(
            "give at most {MAX_EVIDENCE} evidence items"
        )));
    }
    if args.options.len() > MAX_OPTIONS {
        return Err(PhnError::InvalidInput(format!(
            "compare at most {MAX_OPTIONS} options; a brief is clearer with two to four"
        )));
    }
    if args.criteria.len() > MAX_CRITERIA {
        return Err(PhnError::InvalidInput(format!(
            "use at most {MAX_CRITERIA} criteria"
        )));
    }
    if args.ratings.len() > MAX_RATINGS {
        return Err(PhnError::InvalidInput(format!(
            "give at most {MAX_RATINGS} ratings"
        )));
    }
    let mut evidence: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for e in &args.evidence {
        let f = clean_text("evidence finding", &e.finding, MAX_TEXT)?;
        if f.is_empty() {
            continue;
        }
        refuse_identifiers("evidence finding", &f)?;
        evidence.push((
            f,
            text("evidence citation", &e.citation, MAX_SHORT)?,
            text("evidence strength", &e.strength, MAX_SHORT)?,
        ));
    }
    let mut options: Vec<(String, Option<String>)> = Vec::new();
    for o in &args.options {
        let n = clean_text("option name", &o.name, MAX_SHORT)?;
        if n.is_empty() {
            continue;
        }
        refuse_identifiers("option name", &n)?;
        if options.iter().any(|(x, _)| norm(x) == norm(&n)) {
            return Err(PhnError::InvalidInput(
                "two options have the same name; give each option its own name".into(),
            ));
        }
        options.push((n, text("option description", &o.description, MAX_TEXT)?));
    }
    for k in &args.criteria {
        let k = clean_text("criterion", k, MAX_SHORT)?;
        refuse_identifiers("criterion", &k)?;
    }
    let asked_criteria: Vec<String> = args
        .criteria
        .iter()
        .map(|k| k.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|k| !k.is_empty())
        .collect();
    let criteria = resolve_criteria(&c, &asked_criteria)?;

    // Ratings: each must name a given option and a criterion in the table.
    let mut cells: BTreeMap<(usize, usize), (Rating, Option<String>)> = BTreeMap::new();
    for r in &args.ratings {
        let oi = options
            .iter()
            .position(|(n, _)| norm(n) == norm(&r.option))
            .ok_or_else(|| {
                PhnError::InvalidInput(
                    "a rating names an option that is not in `options`; use the option's name exactly"
                        .into(),
                )
            })?;
        let cn = norm(&r.criterion);
        let ci = criteria
            .iter()
            .position(|k| norm(&k.id) == cn || norm(&k.name) == cn)
            .ok_or_else(|| {
                PhnError::InvalidInput(
                    "a rating names a criterion that is not in the table; add it to `criteria` or use its id or name"
                        .into(),
                )
            })?;
        cells.insert((oi, ci), (r.rating, text("rating reason", &r.reason, MAX_SHORT)?));
    }

    // ---- sections
    let one_page = length == BriefLength::OnePage;
    let defs: Vec<&SectionDef> = c.sections.iter().filter(|s| !one_page || s.one_page).collect();
    let mut md_doc = String::new();
    md_doc.push_str(&format!("# {title}\n\n"));
    md_doc.push_str(&format!(
        "_Draft {} policy brief for {audience}. Method: the Eightfold Path of policy analysis. Every [NURSE: ...] placeholder must be completed or removed, and every number and citation checked, before the brief is used._\n\n",
        if one_page { "one-page" } else { "full" }
    ));
    let mut sections_out: Vec<SectionOut> = Vec::new();
    let mut total_placeholders = 0usize;

    let matrix = build_matrix(&options, &criteria, &cells);

    for s in defs {
        let mut body = String::new();
        let status: &str;
        match s.id.as_str() {
            "summary" => {
                let mut supplied = 0;
                if let Some(p) = &problem {
                    body.push_str(&format!("{}\n\n", ended(p)));
                    supplied += 1;
                }
                if let Some(r) = &request {
                    body.push_str(&format!("**Decision requested:** {}\n\n", ended(r)));
                    supplied += 1;
                } else {
                    body.push_str("**Decision requested:** [NURSE: the decision you are asking for]\n\n");
                }
                if let Some(r) = &recommendation {
                    body.push_str(&format!("**Recommendation:** {}\n\n", ended(r)));
                    supplied += 1;
                }
                status = if supplied >= 2 { "partly supplied" } else { "placeholder" };
                if supplied == 0 {
                    body.push_str(&format!("{}\n\n", s.placeholder));
                }
            }
            "problem" => match &problem {
                Some(p) => {
                    body.push_str(&format!("{}\n\n", ended(p)));
                    status = "supplied";
                }
                None => {
                    body.push_str(&format!("{}\n\n_Prompt: {}_\n\n", s.placeholder, s.prompt));
                    status = "placeholder";
                }
            },
            "evidence" => {
                if evidence.is_empty() {
                    body.push_str(&format!("{}\n\n_Prompt: {}_\n\n", s.placeholder, s.prompt));
                    status = "placeholder";
                } else {
                    let mut missing = false;
                    for (f, cit, st) in &evidence {
                        let mut line = format!("- {}", ended(f));
                        match cit {
                            Some(cc) => line.push_str(&format!(" _Source: {cc}._")),
                            None => {
                                line.push_str(" [NURSE: citation for this finding]");
                                missing = true;
                            }
                        }
                        if let Some(x) = st {
                            line.push_str(&format!(" Strength: {x}."));
                        }
                        body.push_str(&line);
                        body.push('\n');
                    }
                    body.push('\n');
                    status = if missing { "partly supplied" } else { "supplied" };
                }
            }
            "options" => {
                if options.is_empty() {
                    body.push_str("1. [NURSE: option A]\n2. [NURSE: option B]\n3. Continue the current approach [NURSE: describe the current approach, or delete this option]\n\n");
                    body.push_str(&format!("_Prompt: {}_\n\n", s.prompt));
                    status = "placeholder";
                } else {
                    let mut missing = false;
                    for (i, (n, d)) in options.iter().enumerate() {
                        match d {
                            Some(d) => body.push_str(&format!("{}. **{}**: {}\n", i + 1, n, ended(d))),
                            None => {
                                body.push_str(&format!("{}. **{}**: [NURSE: describe this option]\n", i + 1, n));
                                missing = true;
                            }
                        }
                    }
                    body.push('\n');
                    status = if missing { "partly supplied" } else { "supplied" };
                }
            }
            "criteria" => {
                for k in &criteria {
                    body.push_str(&format!("- **{}**: {} _({})_\n", k.name, k.prompt, k.origin));
                }
                body.push_str(&format!("\n{}\n\n", s.placeholder));
                status = "partly supplied";
            }
            "matrix" => {
                let mut headers: Vec<&str> = vec!["Option"];
                headers.extend(matrix.criteria.iter().map(String::as_str));
                let rows: Vec<Vec<String>> = matrix
                    .rows
                    .iter()
                    .map(|r| {
                        let mut v = vec![r.option.clone()];
                        v.extend(r.cells.iter().cloned());
                        v
                    })
                    .collect();
                body.push_str(&md::table(&headers, &rows));
                body.push_str("\nRatings: Strong, Mixed, Weak, or Unknown, with the reason; read the words, not the position in the table.\n\n");
                let filled = args.ratings.len();
                let cells_total = matrix.rows.len() * matrix.criteria.len();
                status = if filled == 0 {
                    "placeholder"
                } else if filled >= cells_total {
                    "supplied"
                } else {
                    "partly supplied"
                };
            }
            "tradeoffs" => match &tradeoffs {
                Some(t) => {
                    body.push_str(&format!("{}\n\n", ended(t)));
                    status = "supplied";
                }
                None => {
                    body.push_str(&format!("{}\n\n_Prompt: {}_\n\n", s.placeholder, s.prompt));
                    status = "placeholder";
                }
            },
            "recommendation" => match &recommendation {
                Some(r) => {
                    body.push_str(&format!("{}\n\n", ended(r)));
                    body.push_str("[NURSE: first steps, who is responsible, and what would change this recommendation]\n\n");
                    status = "partly supplied";
                }
                None => {
                    body.push_str(&format!("{}\n\n_Prompt: {}_\n\n", s.placeholder, s.prompt));
                    status = "placeholder";
                }
            },
            "references" => {
                let mut any = false;
                for (_, cit, _) in &evidence {
                    if let Some(cc) = cit {
                        body.push_str(&format!("- {cc}\n"));
                        any = true;
                    }
                }
                body.push_str("- Method: the Eightfold Path of policy analysis (Bardach, 2012, as cited in African Journal of Disability, 2018), described here from open-access articles; the book itself was not read.\n\n");
                if !any {
                    body.push_str(&format!("{}\n\n", s.placeholder));
                }
                status = if any { "partly supplied" } else { "placeholder" };
            }
            _ => {
                body.push_str(&format!("{}\n\n_Prompt: {}_\n\n", s.placeholder, s.prompt));
                status = "placeholder";
            }
        }
        md_doc.push_str(&format!("## {}\n\n{}", s.heading, body));
        let ph = placeholders_in(&body);
        total_placeholders += ph;
        sections_out.push(SectionOut {
            id: s.id.clone(),
            heading: s.heading.clone(),
            step: s.step,
            status: status.into(),
            placeholders: ph,
        });
    }
    total_placeholders += placeholders_in(&title) + placeholders_in(&audience);

    // ---- eightfold path status
    let status_of = |section: &str| -> String {
        sections_out
            .iter()
            .find(|x| x.id == section)
            .map(|x| x.status.clone())
            .unwrap_or_else(|| "not in this length".into())
    };
    let steps: Vec<StepOut> = c
        .steps
        .iter()
        .map(|st| StepOut {
            number: st.number,
            name: st.name.clone(),
            description: st.description.clone(),
            brief_section: st.brief_section.clone(),
            status: status_of(&st.brief_section),
        })
        .collect();

    // ---- readability of the prose the nurse supplied (placeholders and tables left out)
    let mut prose: Vec<String> = Vec::new();
    for t in [&problem, &request, &tradeoffs, &recommendation].into_iter().flatten() {
        prose.push(ended(t));
    }
    for (f, _, _) in &evidence {
        prose.push(ended(f));
    }
    for (_, d) in &options {
        if let Some(d) = d {
            prose.push(ended(d));
        }
    }
    for (_, (_, r)) in &cells {
        if let Some(r) = r {
            prose.push(ended(r));
        }
    }
    let readability = readability_of(&prose.join("\n\n"));

    // ---- files
    let mut files: Vec<String> = Vec::new();
    if matches!(output, BriefOutput::Docx | BriefOutput::Both) {
        files.push(write_docx(ctx, "policy_brief", &title, &md_doc)?);
    }
    if matches!(output, BriefOutput::Markdown | BriefOutput::Both) {
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file("policy_brief.md")?;
        std::fs::write(&path, &md_doc)?;
        files.push(sandbox.display_path(&path));
    }

    let method_quotes: Vec<String> = c
        .evidence_quotes
        .iter()
        .filter(|q| matches!(q.id.as_str(), "steps_1_4" | "steps_5_8" | "outcomes_matrix"))
        .map(|q| {
            let t = c.sources.get(&q.source).map(|s| s.title.as_str()).unwrap_or("");
            format!("\"{}\" ({t})", q.quote)
        })
        .collect();

    let summary = summary_text(
        &title,
        one_page,
        &audience,
        &steps,
        &sections_out,
        total_placeholders,
        &matrix,
        &readability,
        &files,
    );

    let mut sources = content::sources_for(
        ["namibia_2018", "fluoride_2026", "collins_2005", "skiff_2020"],
        &c.sources,
    );
    sources.truncate(4);
    let mut caveats = vec![
        "This is a template filled only with what you supplied. It does not research the problem, check your numbers, or choose the recommendation; every fact, number, and citation must come from you, and each [NURSE: ...] placeholder must be completed or removed."
            .to_string(),
        "Bardach's book was not read. The steps are described from two open-access articles and two abstracts (see sources); the criteria are those the articles used, plus two marked as this project's suggestions."
            .to_string(),
        "The readability check covers only the prose you supplied; tables and placeholders are left out."
            .to_string(),
    ];
    if criteria.iter().any(|k| k.origin.contains("suggestion")) {
        caveats.push("One or more criteria are this project's drafting suggestions, not from a source read.".into());
    }
    caveats.extend(content::review_caveats(&[BRIEF]));

    Ok(Envelope::new(
        summary,
        BriefResult {
            title,
            length: if one_page { "one_page" } else { "full" }.into(),
            audience,
            eightfold_path: steps,
            sections: sections_out,
            criteria: criteria
                .iter()
                .map(|k| CriterionOut {
                    id: k.id.clone(),
                    name: k.name.clone(),
                    prompt: k.prompt.clone(),
                    origin: k.origin.clone(),
                })
                .collect(),
            comparison: matrix,
            placeholders_remaining: total_placeholders,
            readability,
            document_files: files.clone(),
            method_quotes,
            limits: c.limits.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        files_written: files,
        notes: vec![
            "No local files are read. What you type appears in the draft in /data/output and in this response; describe groups and programs, not individual people (identifiers are refused)."
                .into(),
        ],
        ..Privacy::none()
    }))
}

fn build_matrix(
    options: &[(String, Option<String>)],
    criteria: &[ResolvedCriterion],
    cells: &BTreeMap<(usize, usize), (Rating, Option<String>)>,
) -> MatrixOut {
    let names: Vec<(String, Option<String>)> = if options.is_empty() {
        vec![
            ("[NURSE: option A]".into(), None),
            ("[NURSE: option B]".into(), None),
            ("Continue the current approach".into(), None),
        ]
    } else {
        options.to_vec()
    };
    let rows = names
        .iter()
        .enumerate()
        .map(|(oi, (n, _))| MatrixRow {
            option: n.clone(),
            cells: (0..criteria.len())
                .map(|ci| match cells.get(&(oi, ci)) {
                    Some((r, Some(reason))) => format!("{}: {}", r.label(), reason),
                    Some((r, None)) => format!("{}: [NURSE: reason and evidence]", r.label()),
                    None => "[NURSE: rating (strong, mixed, weak, unknown) and reason]".into(),
                })
                .collect(),
        })
        .collect();
    MatrixOut {
        criteria: criteria.iter().map(|k| k.name.clone()).collect(),
        rows,
        accessible: true,
    }
}

fn readability_of(prose: &str) -> ReadabilityOut {
    let target = default_target_grade();
    let a = analyze(prose, &AnalyzeOptions::default());
    if a.counts.words < MIN_WORDS_FOR_READABILITY {
        return ReadabilityOut {
            checked: false,
            words: a.counts.words,
            sentences: a.counts.sentences,
            median_grade: None,
            target_grade: target,
            meets_target: None,
            long_sentences: 0,
            passive_sentences: 0,
            notes: vec![format!(
                "Not enough of your own prose to score ({} words; at least {MIN_WORDS_FOR_READABILITY} are needed). Add the problem, evidence, and recommendation, then run check_readability on the finished draft.",
                a.counts.words
            )],
        };
    }
    let mut notes = Vec::new();
    if !a.grade.meets_target {
        notes.push(format!(
            "The prose reads at about grade {:.1}; the target is grade {:.1}. Shorten sentences and replace technical terms with everyday words, then check again with check_readability.",
            a.grade.median_grade, target
        ));
    } else {
        notes.push(format!(
            "The prose reads at about grade {:.1}, within the target of grade {:.1}.",
            a.grade.median_grade, target
        ));
    }
    if a.flags.long_sentence_count > 0 {
        notes.push(format!(
            "{} long sentence(s); split them.",
            a.flags.long_sentence_count
        ));
    }
    if a.flags.passive_voice_count > 0 {
        notes.push(format!(
            "{} sentence(s) in the passive voice; say who does what.",
            a.flags.passive_voice_count
        ));
    }
    ReadabilityOut {
        checked: true,
        words: a.counts.words,
        sentences: a.counts.sentences,
        median_grade: Some((a.grade.median_grade * 10.0).round() / 10.0),
        target_grade: target,
        meets_target: Some(a.grade.meets_target),
        long_sentences: a.flags.long_sentence_count,
        passive_sentences: a.flags.passive_voice_count,
        notes,
    }
}

#[allow(clippy::too_many_arguments)]
fn summary_text(
    title: &str,
    one_page: bool,
    audience: &str,
    steps: &[StepOut],
    sections: &[SectionOut],
    placeholders: usize,
    matrix: &MatrixOut,
    readability: &ReadabilityOut,
    files: &[String],
) -> String {
    let mut s = format!(
        "**Draft {} policy brief: {title}** (for {audience})\n\n",
        if one_page { "one-page" } else { "full" }
    );
    s.push_str(
        "A template filled with what you supplied; nothing in it comes from the tool except the \
         structure. Complete every [NURSE: ...] placeholder and check every number and citation.\n\n",
    );
    let rows: Vec<Vec<String>> = steps
        .iter()
        .map(|st| {
            vec![
                st.number.to_string(),
                st.name.clone(),
                st.brief_section.clone(),
                st.status.clone(),
            ]
        })
        .collect();
    s.push_str(&md::table(&["Step", "Eightfold Path step", "Brief section", "Status"], &rows));
    s.push('\n');
    s.push_str(&format!(
        "Sections: {}. Placeholders left: {placeholders}. Comparison table: {} option(s) by {} criteria.\n\n",
        sections
            .iter()
            .map(|x| format!("{} ({})", x.heading, x.status))
            .collect::<Vec<_>>()
            .join("; "),
        matrix.rows.len(),
        matrix.criteria.len()
    ));
    for n in &readability.notes {
        s.push_str(&format!("- Readability: {n}\n"));
    }
    if files.is_empty() {
        s.push_str("\nNo file was written (output: none).\n");
    } else {
        s.push_str(&format!("\nDraft written to: {}\n", files.join(", ")));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_steps_sections_and_criteria_are_consistent() {
        let c: BriefContent = content::load(BRIEF).unwrap();
        assert_eq!(c.steps.len(), 8);
        for (i, s) in c.steps.iter().enumerate() {
            assert_eq!(s.number as usize, i + 1);
            assert!(c.sections.iter().any(|x| x.id == s.brief_section), "{}", s.name);
            assert!(c.sources.contains_key(&s.source));
        }
        for q in &c.evidence_quotes {
            assert!(q.quote.split_whitespace().count() <= 25, "{}", q.id);
            assert!(c.sources.contains_key(&q.source));
        }
        for k in &c.criteria {
            if let Some(s) = &k.source {
                assert!(c.sources.contains_key(s));
            } else {
                assert!(k.source_kind.contains("suggestion") && k.confidence == Confidence::Low);
            }
        }
    }

    #[test]
    fn criteria_resolve_by_id_and_name_and_allow_custom_ones() {
        let c: BriefContent = content::load(BRIEF).unwrap();
        let r = resolve_criteria(
            &c,
            &["health_impact".into(), "Equity".into(), "Community trust".into(), "equity".into()],
        )
        .unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[2].origin, "your own criterion");
        assert_eq!(resolve_criteria(&c, &[]).unwrap().len(), 4);
    }

    #[test]
    fn empty_matrix_has_placeholders_and_filled_cells_use_words() {
        let c: BriefContent = content::load(BRIEF).unwrap();
        let crit = resolve_criteria(&c, &[]).unwrap();
        let m = build_matrix(&[], &crit, &BTreeMap::new());
        assert_eq!(m.rows.len(), 3);
        assert!(m.rows[0].cells.iter().all(|x| x.contains("[NURSE:")));
        let opts = vec![("Option one".to_string(), None)];
        let mut cells = BTreeMap::new();
        cells.insert((0, 1), (Rating::Mixed, Some("some evidence".to_string())));
        let m = build_matrix(&opts, &crit, &cells);
        assert_eq!(m.rows[0].cells[1], "Mixed: some evidence");
        assert!(m.accessible);
    }

    #[test]
    fn readability_needs_enough_text() {
        let r = readability_of("Short text.");
        assert!(!r.checked && r.median_grade.is_none());
        let long = "The board of health should act. Many families live in homes with mold. Mold can make asthma worse. \
                    We ask the board to adopt a rule. The rule would help renters get repairs. Staff would inspect each home."
            .to_string();
        let r = readability_of(&long);
        assert!(r.checked && r.median_grade.is_some());
    }
}
