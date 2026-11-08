//! COMM-01 `check_readability`: reading-level formulas (Flesch Reading Ease,
//! Flesch-Kincaid, SMOG, Gunning Fog), plain-language flags (long sentences,
//! CDC Everyday Words jargon, possible passive voice, unexplained acronyms,
//! long paragraphs and lists), and the CDC Clear Communication Index.
//!
//! [`analyze`] is shared with the other communication tools (social media
//! posts, press materials, postvention messages), which check their drafts
//! with the same rules.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_local::extract::FileKind;
use phn_local::phi::{self, RedactionLevel};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, Confidence, NamedSource, READABILITY};
use super::textstats::{self, BlockKind, Counts, Parsed, Sentence};
use crate::context::Context;
use crate::md;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Formula {
    id: String,
    name: String,
    formula: String,
    #[allow(dead_code)]
    scale: String,
    source: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct Band {
    min: f64,
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FleschBands {
    bands: Vec<Band>,
}

#[derive(Debug, Clone, Deserialize)]
struct SmogRow {
    min: usize,
    max: usize,
    grade: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct SmogTable {
    section: String,
    accuracy: String,
    rows: Vec<SmogRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct Thresholds {
    default_target_grade: f64,
    default_target_grade_note: String,
    long_sentence_words: usize,
    long_sentence_source: String,
    paragraph_max_sentences: usize,
    paragraph_source: String,
    list_max_items: usize,
    list_source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Guidance {
    text: String,
    source: String,
    section: String,
}

/// A Clear Communication Index item.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CciItem {
    pub number: u8,
    pub part: String,
    pub area: String,
    pub guide_page: u32,
    pub na_allowed: bool,
    pub yes_scores: u8,
    pub question: String,
    pub guidance: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CciPart {
    id: String,
    name: String,
    applies_question: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Cci {
    passing_score: f64,
    passing_source: String,
    rules: Vec<String>,
    intro_questions: Vec<String>,
    parts: Vec<CciPart>,
    items: Vec<CciItem>,
}

/// A jargon term with CDC's everyday alternative.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Term {
    pub term: String,
    #[serde(rename = "match")]
    pub forms: Vec<String>,
    pub plain: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub example_before: Option<String>,
    #[serde(default)]
    pub example_after: Option<String>,
    pub check_context: bool,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReadabilityContent {
    meta: ContentMeta,
    sources: BTreeMap<String, NamedSource>,
    formulas: Vec<Formula>,
    flesch_bands: FleschBands,
    smog_table: SmogTable,
    thresholds: Thresholds,
    guidance: Vec<Guidance>,
    cci: Cci,
    extra_terms: Vec<Term>,
    jargon: Vec<Term>,
}

static CONTENT: LazyLock<ReadabilityContent> = LazyLock::new(|| {
    content::load(READABILITY).expect("content/comms/readability.yaml matches its schema")
});

// ---------------------------------------------------------------- arguments and results

/// Arguments for `check_readability`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ReadabilityArgs {
    /// The draft to check (plain text or Markdown; `#` headings and `-` or `1.` lists are
    /// recognized). Give this or `path`.
    pub text: Option<String>,
    /// A DOCX, HTML, PDF, TXT, or Markdown file in /data to check instead of `text`. Quoted
    /// excerpts from a file are passed through the identifier redactor.
    pub path: Option<String>,
    /// Reading grade to aim for. Default 8 (a tool default; set your agency's standard).
    pub target_grade: Option<f64>,
    /// Your answers to the CDC Clear Communication Index: keys "1" to "20" with "yes" or
    /// "no" ("na" allowed for 19 and 20), plus "C" and "D" with "yes"/"no" for whether the
    /// material has numbers (Part C) and risk information (Part D). The Index is scored when
    /// every item that applies has an answer; until then the tool shows its suggestions.
    pub cci_answers: BTreeMap<String, String>,
    /// Most flags to list for each kind of problem (default 10, at most 50).
    pub max_flags: Option<usize>,
}

/// Counts behind the scores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextCounts {
    pub words: usize,
    /// Sentences in paragraphs and list items (headings are not counted).
    pub sentences: usize,
    pub syllables: usize,
    /// Words of 3 or more syllables (SMOG).
    pub polysyllabic_words: usize,
    /// Gunning's complex words (Fog index).
    pub complex_words: usize,
    pub paragraphs: usize,
    pub headings: usize,
    pub lists: usize,
    pub list_items: usize,
    pub average_words_per_sentence: f64,
    pub average_syllables_per_word: f64,
}

/// One formula's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Score {
    /// "flesch_reading_ease", "flesch_kincaid_grade", "smog", or "gunning_fog".
    pub id: String,
    pub name: String,
    pub value: f64,
    /// Plain-language meaning (grade, or the Flesch ease band).
    pub interpretation: String,
    pub formula: String,
    /// Citation of the original publication.
    pub source: String,
}

/// SMOG details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SmogDetail {
    /// McLaughlin's formula, scaled to 30 sentences.
    pub formula_grade: f64,
    /// Polysyllabic words counted for the conversion table (in a 30-sentence sample, or all
    /// sentences scaled to 30 when there are fewer).
    pub table_polysyllables: usize,
    /// Grade from the SMOG conversion table (13 = college level); null when out of range.
    pub table_grade: Option<u32>,
    /// How the sample was taken.
    pub sample: String,
}

/// Fry's short-document averages (CDC Simply Put, Appendix C).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FryAverages {
    pub sentences_per_100_words: f64,
    pub syllables_per_100_words: f64,
}

/// The overall grade estimate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradeEstimate {
    /// Median of the Flesch-Kincaid, SMOG, and Fog grades.
    pub median_grade: f64,
    pub lowest_grade: f64,
    pub highest_grade: f64,
    pub target_grade: f64,
    pub meets_target: bool,
}

/// A sentence that was flagged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SentenceFlag {
    /// 1-based position among counted sentences.
    pub sentence: usize,
    pub words: usize,
    /// The start of the sentence (redacted when the text came from a file).
    pub excerpt: String,
    /// What was found (for passive voice, the verb phrase).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub suggestion: String,
}

/// A jargon term found in the text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JargonFlag {
    pub term: String,
    /// Forms found in the text.
    pub found: Vec<String>,
    pub count: usize,
    /// CDC's everyday alternative(s).
    pub plain_alternative: Option<String>,
    pub note: Option<String>,
    /// CDC's example sentence, before and after.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_after: Option<String>,
    /// The word is often fine; check how it is used.
    pub check_context: bool,
    /// "CDC Everyday Words" or "CDC Simply Put".
    pub source: String,
}

/// An acronym or abbreviation that is not spelled out at its first use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcronymFlag {
    pub acronym: String,
    pub count: usize,
    /// "never spelled out" or "spelled out after its first use".
    pub problem: String,
}

/// A word and how often it appears.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WordCount {
    pub word: String,
    pub syllables: usize,
    pub count: usize,
}

/// A paragraph or list that is too long.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StructureFlag {
    /// "paragraph" or "list".
    pub kind: String,
    /// 1-based position among paragraphs or lists.
    pub position: usize,
    /// Sentences (paragraph) or items (list).
    pub size: usize,
    pub limit: usize,
    pub suggestion: String,
}

/// Numbers in the text (for Clear Communication Index Parts C and D).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct NumberSummary {
    pub numbers: usize,
    pub percentages: usize,
    pub decimals: usize,
    /// Fractions and "x in y" or "x out of y" statements (probabilities).
    pub ratios: usize,
}

/// Every flag.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Flags {
    pub long_sentences: Vec<SentenceFlag>,
    /// Sentences over the limit (all of them, before `max_flags`).
    pub long_sentence_count: usize,
    pub jargon: Vec<JargonFlag>,
    pub passive_voice: Vec<SentenceFlag>,
    pub passive_voice_count: usize,
    pub acronyms: Vec<AcronymFlag>,
    /// Frequent words of 4 or more syllables.
    pub long_words: Vec<WordCount>,
    pub long_paragraphs: Vec<StructureFlag>,
    pub long_lists: Vec<StructureFlag>,
    /// Sentences that say what not to do ("Do not ...", "Never ...").
    pub negative_instructions: Vec<SentenceFlag>,
    pub numbers: NumberSummary,
}

/// How a Clear Communication Index item stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CciItemResult {
    pub number: u8,
    pub part: String,
    pub area: String,
    pub question: String,
    pub guidance: String,
    /// CDC User Guide page with examples.
    pub user_guide_page: u32,
    /// Your answer ("yes", "no", "na"), if given.
    pub answer: Option<String>,
    /// Points earned (null when not answered or not applicable).
    pub points: Option<u8>,
    /// The tool's suggestion from the text, when it has one ("yes", "no", "na", or "check").
    pub suggestion: Option<String>,
    pub suggestion_reason: Option<String>,
}

/// The Clear Communication Index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CciResult {
    /// The four questions to answer before scoring.
    pub before_you_begin: Vec<String>,
    /// CDC's scoring rules.
    pub rules: Vec<String>,
    /// The question that decides whether each of Parts B, C, and D applies.
    pub part_questions: BTreeMap<String, String>,
    pub items: Vec<CciItemResult>,
    /// Parts that apply: A always; B, C, D from your answers (null = not yet known).
    pub parts_apply: BTreeMap<String, Option<bool>>,
    /// Points earned / points possible × 100, when every applicable item is answered.
    pub score: Option<f64>,
    pub points_earned: Option<u32>,
    pub points_possible: Option<u32>,
    /// 90 or higher passes.
    pub passing: Option<bool>,
    /// Items that still need an answer.
    pub unanswered: Vec<u8>,
    pub notes: Vec<String>,
}

/// Result of `check_readability`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReadabilityResult {
    /// "pasted text" or the file checked.
    pub checked: String,
    pub counts: TextCounts,
    pub scores: Vec<Score>,
    pub smog: SmogDetail,
    pub fry: FryAverages,
    pub grade_estimate: GradeEstimate,
    pub flags: Flags,
    pub clear_communication_index: CciResult,
    /// Plain-language tips from CDC and ODPHP guidance.
    pub tips: Vec<String>,
    /// How the counts and scores are computed.
    pub method: String,
}

// ---------------------------------------------------------------- analysis

/// Options for [`analyze`].
#[derive(Debug, Clone)]
pub struct AnalyzeOptions {
    pub target_grade: f64,
    pub max_flags: usize,
    /// Redact identifiers in excerpts (text read from a file).
    pub redact_excerpts: bool,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            target_grade: CONTENT.thresholds.default_target_grade,
            max_flags: 10,
            redact_excerpts: false,
        }
    }
}

/// The analysis behind `check_readability` (without the Clear Communication Index).
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub parsed: Parsed,
    pub counts: TextCounts,
    pub scores: Vec<Score>,
    pub smog: SmogDetail,
    pub fry: FryAverages,
    pub grade: GradeEstimate,
    pub flags: Flags,
}

/// The default reading-grade target.
pub fn default_target_grade() -> f64 {
    CONTENT.thresholds.default_target_grade
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Flesch Reading Ease.
pub fn flesch_reading_ease(c: &Counts) -> f64 {
    let (w, s) = (c.words.max(1) as f64, c.sentences.max(1) as f64);
    206.835 - 1.015 * (w / s) - 84.6 * (c.syllables as f64 / w)
}

/// Flesch-Kincaid grade level.
pub fn flesch_kincaid_grade(c: &Counts) -> f64 {
    let (w, s) = (c.words.max(1) as f64, c.sentences.max(1) as f64);
    0.39 * (w / s) + 11.8 * (c.syllables as f64 / w) - 15.59
}

/// SMOG grade by McLaughlin's formula, scaled to 30 sentences.
pub fn smog_grade(polysyllables: usize, sentences: usize) -> f64 {
    1.0430 * (polysyllables as f64 * 30.0 / sentences.max(1) as f64).sqrt() + 3.1291
}

/// Gunning Fog index.
pub fn gunning_fog(c: &Counts) -> f64 {
    let (w, s) = (c.words.max(1) as f64, c.sentences.max(1) as f64);
    0.4 * ((w / s) + 100.0 * (c.complex_words as f64 / w))
}

/// Grade from the SMOG conversion table for a polysyllable count in 30 sentences.
pub fn smog_table_grade(polysyllables_in_30: usize) -> Option<u32> {
    CONTENT
        .smog_table
        .rows
        .iter()
        .find(|r| (r.min..=r.max).contains(&polysyllables_in_30))
        .map(|r| r.grade)
}

/// Fry's averages per 100 words.
pub fn fry_averages(words: usize, sentences: usize, syllables: usize) -> FryAverages {
    let w = words.max(1) as f64;
    FryAverages {
        sentences_per_100_words: round1(sentences as f64 * 100.0 / w),
        syllables_per_100_words: round1(syllables as f64 * 100.0 / w),
    }
}

fn flesch_band(v: f64) -> String {
    CONTENT
        .flesch_bands
        .bands
        .iter()
        .find(|b| v >= b.min)
        .map(|b| b.label.clone())
        .unwrap_or_else(|| "very difficult".into())
}

fn source_label(key: &str) -> String {
    CONTENT
        .sources
        .get(key)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| key.to_string())
}

/// Analyze a text.
pub fn analyze(text: &str, opts: &AnalyzeOptions) -> Analysis {
    let parsed = textstats::parse(text);
    let c = parsed.counts();
    let sentences: Vec<&Sentence> = parsed
        .counted_sentences()
        .filter(|s| !s.words.is_empty())
        .collect();
    let paragraphs = parsed
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Paragraph)
        .count();
    let headings = parsed
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Heading)
        .count();
    let list_ids: BTreeSet<usize> = parsed.blocks.iter().filter_map(|b| b.list).collect();
    let list_items = parsed
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::ListItem)
        .count();
    let counts = TextCounts {
        words: c.words,
        sentences: c.sentences,
        syllables: c.syllables,
        polysyllabic_words: c.polysyllables,
        complex_words: c.complex_words,
        paragraphs,
        headings,
        lists: list_ids.len(),
        list_items,
        average_words_per_sentence: round1(c.words as f64 / c.sentences.max(1) as f64),
        average_syllables_per_word: (c.syllables as f64 / c.words.max(1) as f64 * 100.0).round()
            / 100.0,
    };

    // SMOG: three chains of 10 sentences when there are 30 or more; otherwise all, scaled.
    let (table_poly, sample) = if sentences.len() >= 30 {
        let n = sentences.len();
        let mid = n / 2 - 5;
        let picked: Vec<&Sentence> = sentences[..10]
            .iter()
            .chain(sentences[mid..mid + 10].iter())
            .chain(sentences[n - 10..].iter())
            .copied()
            .collect();
        (
            textstats::counts_of(picked.into_iter()).polysyllables,
            format!("10 sentences each from the beginning, middle, and end ({n} sentences in all)"),
        )
    } else {
        let scaled = (c.polysyllables as f64 * 30.0 / c.sentences.max(1) as f64).round() as usize;
        (
            scaled,
            format!(
                "all {} sentence(s), polysyllable count scaled to 30 sentences (SMOG is designed for 30 or more)",
                c.sentences
            ),
        )
    };
    let smog = SmogDetail {
        formula_grade: round1(smog_grade(c.polysyllables, c.sentences)),
        table_polysyllables: table_poly,
        table_grade: smog_table_grade(table_poly),
        sample,
    };
    let fre = flesch_reading_ease(&c);
    let fk = flesch_kincaid_grade(&c);
    let fog = gunning_fog(&c);
    let formula = |id: &str| CONTENT.formulas.iter().find(|f| f.id == id);
    let score = |id: &str, value: f64, interpretation: String| {
        let f = formula(id);
        Score {
            id: id.into(),
            name: f.map(|f| f.name.clone()).unwrap_or_else(|| id.into()),
            value: round1(value),
            interpretation,
            formula: f.map(|f| f.formula.clone()).unwrap_or_default(),
            source: f
                .map(|f| {
                    let mut s = source_label(&f.source);
                    if f.confidence != Confidence::High {
                        s.push_str(" (formula details: medium confidence)");
                    }
                    s
                })
                .unwrap_or_default(),
        }
    };
    let grade_text = |g: f64| {
        if g >= 13.0 {
            format!("grade {:.1} (college level)", g)
        } else {
            format!("grade {:.1}", g)
        }
    };
    let scores = vec![
        score("flesch_reading_ease", fre, flesch_band(fre)),
        score("flesch_kincaid_grade", fk, grade_text(fk)),
        score("smog", smog.formula_grade, grade_text(smog.formula_grade)),
        score("gunning_fog", fog, grade_text(fog)),
    ];
    let mut grades = [round1(fk), smog.formula_grade, round1(fog)];
    grades.sort_by(f64::total_cmp);
    let grade = GradeEstimate {
        median_grade: grades[1],
        lowest_grade: grades[0],
        highest_grade: grades[2],
        target_grade: opts.target_grade,
        meets_target: grades[1] <= opts.target_grade,
    };
    let fry = fry_averages(c.words, c.sentences, c.syllables);
    let flags = find_flags(&parsed, &sentences, text, opts);
    Analysis {
        parsed,
        counts,
        scores,
        smog,
        fry,
        grade,
        flags,
    }
}

fn excerpt(s: &str, max_words: usize, redact: bool) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut e = words[..words.len().min(max_words)].join(" ");
    if words.len() > max_words {
        e.push_str(" …");
    }
    if redact {
        phi::redact_text(&e, RedactionLevel::Strict).text
    } else {
        e
    }
}

const BE_VERBS: &[&str] = &[
    "am", "is", "are", "was", "were", "be", "been", "being", "get", "gets", "got", "gotten",
    "isn't", "aren't", "wasn't", "weren't",
];
const ADVERBS: &[&str] = &[
    "not",
    "also",
    "often",
    "usually",
    "always",
    "never",
    "already",
    "still",
    "then",
    "now",
    "just",
    "only",
    "typically",
    "commonly",
    "frequently",
    "rarely",
    "sometimes",
    "first",
];
const IRREGULAR_PARTICIPLES: &[&str] = &[
    "been",
    "born",
    "borne",
    "bought",
    "bound",
    "broken",
    "brought",
    "built",
    "caught",
    "chosen",
    "cut",
    "done",
    "drawn",
    "driven",
    "eaten",
    "fed",
    "felt",
    "forgotten",
    "found",
    "frozen",
    "given",
    "gone",
    "grown",
    "heard",
    "held",
    "hidden",
    "hit",
    "hurt",
    "kept",
    "known",
    "laid",
    "led",
    "left",
    "lent",
    "lost",
    "made",
    "meant",
    "met",
    "paid",
    "put",
    "read",
    "run",
    "said",
    "seen",
    "sent",
    "set",
    "shaken",
    "shown",
    "shut",
    "sold",
    "spent",
    "spoken",
    "spread",
    "stolen",
    "struck",
    "taken",
    "taught",
    "thought",
    "thrown",
    "told",
    "understood",
    "won",
    "worn",
    "written",
    "bitten",
    "beaten",
    "chosen",
    "fallen",
    "forbidden",
    "given",
    "withdrawn",
];
/// Words ending in -ed after "be" that are usually adjectives.
const ED_ADJECTIVES: &[&str] = &[
    "interested",
    "tired",
    "worried",
    "scared",
    "concerned",
    "pleased",
    "married",
    "excited",
    "supposed",
    "used",
    "involved",
    "related",
    "limited",
    "prepared",
    "qualified",
    "experienced",
    "embarrassed",
    "ashamed",
    "confused",
    "satisfied",
    "stressed",
    "covered",
    "insured",
    "vaccinated",
    "infected",
    "exposed",
    "isolated",
    "unvaccinated",
    "uninsured",
];

fn passive_phrase(s: &Sentence) -> Option<String> {
    let toks: Vec<String> = s.words.iter().map(|w| w.text.to_lowercase()).collect();
    for i in 0..toks.len() {
        if !BE_VERBS.contains(&toks[i].as_str()) {
            continue;
        }
        let mut j = i + 1;
        if j < toks.len() && (ADVERBS.contains(&toks[j].as_str()) || toks[j].ends_with("ly")) {
            j += 1;
        }
        let Some(p) = toks.get(j) else { continue };
        let is_participle = IRREGULAR_PARTICIPLES.contains(&p.as_str())
            || (p.len() > 4 && p.ends_with("ed") && !ED_ADJECTIVES.contains(&p.as_str()));
        // "is required/needed/advised to ..." and "... by ..." are passive even with the adjectives.
        let by_follows = toks.get(j + 1).is_some_and(|n| n == "by");
        let adjective_passive = ED_ADJECTIVES.contains(&p.as_str()) && by_follows;
        if is_participle || adjective_passive {
            return Some(
                s.words[i..=j]
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
    }
    None
}

static ACRONYM: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b[A-Z][A-Z0-9&]{1,6}(?:-[0-9]{1,2})?s?\b").expect("valid regex")
});
const ACRONYM_OK: &[&str] = &["OK", "TV", "AM", "PM", "US", "USA", "ID", "I"];

fn find_acronyms(text: &str) -> Vec<AcronymFlag> {
    let mut first: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for m in ACRONYM.find_iter(text) {
        let mut a = m.as_str().to_string();
        if a.ends_with('s') {
            a.pop();
        }
        if a.chars().filter(|c| c.is_ascii_uppercase()).count() < 2
            || ACRONYM_OK.contains(&a.as_str())
            || a.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L'))
        {
            continue;
        }
        // Skip words in an all-capitals line (a shouted heading, not an acronym).
        let line_start = text[..m.start()].rfind('\n').map_or(0, |p| p + 1);
        let line_end = text[m.end()..]
            .find('\n')
            .map_or(text.len(), |p| m.end() + p);
        let line = &text[line_start..line_end];
        let letters: Vec<char> = line.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.len() > 12
            && letters.iter().filter(|c| c.is_uppercase()).count() * 10 > letters.len() * 7
        {
            continue;
        }
        let e = first.entry(a).or_insert((m.start(), 0));
        e.1 += 1;
    }
    let mut out = Vec::new();
    for (a, (pos, count)) in first {
        let paren = format!("({a})");
        let paren_s = format!("({a}s)");
        let expanded_after = format!("{a} (");
        let def = text
            .find(&paren)
            .or_else(|| text.find(&paren_s))
            .or_else(|| text.find(&expanded_after));
        let problem = match def {
            None => "never spelled out",
            Some(d) if d > pos + a.len() + 2 => "spelled out after its first use",
            _ => continue,
        };
        out.push(AcronymFlag {
            acronym: a,
            count,
            problem: problem.into(),
        });
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.acronym.cmp(&b.acronym)));
    out
}

static NUMBER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\d[\d,]*(?:\.\d+)?").expect("valid regex"));
static PERCENT: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)\d\s*(?:%|percent\b)").expect("valid regex"));
static DECIMAL: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\d\.\d").expect("valid regex"));
static RATIO: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)\b\d+\s*/\s*\d+\b|\b(?:\d+|one|two|three|four|five|six|seven|eight|nine|ten)\s+(?:in|out of)\s+(?:\d[\d,]*|ten|a hundred|a thousand)\b")
        .expect("valid regex")
});

fn number_summary(text: &str) -> NumberSummary {
    NumberSummary {
        numbers: NUMBER.find_iter(text).count(),
        percentages: PERCENT.find_iter(text).count(),
        decimals: DECIMAL.find_iter(text).count(),
        ratios: RATIO.find_iter(text).count(),
    }
}

/// Jargon terms (CDC Everyday Words and Simply Put) found in a text, most frequent first.
pub(crate) fn find_jargon(parsed: &Parsed) -> Vec<JargonFlag> {
    // Lower-case tokens of every block, headings included.
    let mut tokens: Vec<Vec<String>> = Vec::new();
    for b in &parsed.blocks {
        for s in &b.sentences {
            tokens.push(s.words.iter().map(|w| w.text.to_lowercase()).collect());
        }
    }
    let inflects = |token: &str, base: &str| -> bool {
        if token == base {
            return true;
        }
        let Some(rest) = token.strip_prefix(base) else {
            if let Some(stem) = base.strip_suffix('e') {
                return token.strip_prefix(stem).is_some_and(|r| r == "ing");
            }
            if let Some(stem) = base.strip_suffix('y') {
                return token
                    .strip_prefix(stem)
                    .is_some_and(|r| matches!(r, "ies" | "ied"));
            }
            return false;
        };
        matches!(rest, "s" | "es" | "d" | "ed" | "ing")
    };
    let all_terms = CONTENT.jargon.iter().chain(CONTENT.extra_terms.iter());
    let mut found: Vec<(&Term, BTreeSet<String>, usize)> = Vec::new();
    // Longest phrases first so "risk factor" wins over "risk".
    let mut forms: Vec<(&Term, Vec<&str>)> = Vec::new();
    for t in all_terms {
        for f in &t.forms {
            forms.push((t, f.split(' ').collect()));
        }
    }
    forms.sort_by_key(|f| std::cmp::Reverse(f.1.len()));
    let mut used: Vec<Vec<bool>> = tokens.iter().map(|t| vec![false; t.len()]).collect();
    for (t, parts) in &forms {
        for (si, toks) in tokens.iter().enumerate() {
            if toks.len() < parts.len() {
                continue;
            }
            for start in 0..=toks.len() - parts.len() {
                if used[si][start..start + parts.len()].iter().any(|u| *u) {
                    continue;
                }
                let last = parts.len() - 1;
                let ok = parts.iter().enumerate().all(|(k, p)| {
                    if k == last {
                        inflects(&toks[start + k], p)
                    } else {
                        toks[start + k] == *p
                    }
                });
                if ok {
                    for u in &mut used[si][start..start + parts.len()] {
                        *u = true;
                    }
                    let phrase = toks[start..start + parts.len()].join(" ");
                    match found.iter_mut().find(|(ft, _, _)| std::ptr::eq(*ft, *t)) {
                        Some((_, set, n)) => {
                            set.insert(phrase);
                            *n += 1;
                        }
                        None => found.push((t, BTreeSet::from([phrase]), 1)),
                    }
                }
            }
        }
    }
    let mut out: Vec<JargonFlag> = found
        .into_iter()
        .map(|(t, set, n)| JargonFlag {
            term: t.term.clone(),
            found: set.into_iter().collect(),
            count: n,
            plain_alternative: t.plain.clone(),
            note: t.note.clone(),
            example_before: t.example_before.clone(),
            example_after: t.example_after.clone(),
            check_context: t.check_context,
            source: if t.source == "simply_put" {
                "CDC Simply Put".into()
            } else {
                "CDC Everyday Words".into()
            },
        })
        .collect();
    out.sort_by(|a, b| {
        a.check_context
            .cmp(&b.check_context)
            .then(b.count.cmp(&a.count))
            .then(a.term.cmp(&b.term))
    });
    out
}

fn find_flags(
    parsed: &Parsed,
    sentences: &[&Sentence],
    text: &str,
    opts: &AnalyzeOptions,
) -> Flags {
    let th = &CONTENT.thresholds;
    let max = opts.max_flags;
    let red = opts.redact_excerpts;
    let mut long: Vec<SentenceFlag> = sentences
        .iter()
        .enumerate()
        .filter(|(_, s)| s.words.len() > th.long_sentence_words)
        .map(|(i, s)| SentenceFlag {
            sentence: i + 1,
            words: s.words.len(),
            excerpt: excerpt(&s.text, 14, red),
            detail: None,
            suggestion: format!(
                "Split into shorter sentences with one idea each (aim for {} words or fewer).",
                th.long_sentence_words
            ),
        })
        .collect();
    let long_count = long.len();
    long.sort_by(|a, b| b.words.cmp(&a.words).then(a.sentence.cmp(&b.sentence)));
    long.truncate(max);
    long.sort_by_key(|f| f.sentence);

    let passive_all: Vec<SentenceFlag> = sentences
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            passive_phrase(s).map(|p| SentenceFlag {
                sentence: i + 1,
                words: s.words.len(),
                excerpt: excerpt(&s.text, 14, red),
                detail: Some(if red { phi::redact_text(&p, RedactionLevel::Strict).text } else { p }),
                suggestion: "Possible passive voice: say who does the action (\"Smoking causes heart disease\", not \"Heart disease is caused by smoking\").".into(),
            })
        })
        .collect();
    let passive_count = passive_all.len();
    let passive: Vec<SentenceFlag> = passive_all.into_iter().take(max).collect();

    let negative: Vec<SentenceFlag> = sentences
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let first: Vec<String> = s
                .words
                .iter()
                .take(2)
                .map(|w| w.text.to_lowercase())
                .collect();
            matches!(
                first.first().map(String::as_str),
                Some("never" | "don't" | "don’t")
            ) || (first.len() == 2 && first[0] == "do" && first[1] == "not")
        })
        .take(max)
        .map(|(i, s)| SentenceFlag {
            sentence: i + 1,
            words: s.words.len(),
            excerpt: excerpt(&s.text, 14, red),
            detail: None,
            suggestion: "Where you can, say what to do instead of what not to do (CDC Simply Put)."
                .into(),
        })
        .collect();

    let mut word_counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for s in sentences {
        for (i, w) in s.words.iter().enumerate() {
            let plain_word = w
                .text
                .chars()
                .all(|c| c.is_alphabetic() || c == '-' || c == '\'' || c == '’');
            // Words from a file are never echoed if they could be names.
            let safe = !red
                || (!w.capitalized && !phi::is_first_name(&w.text) && !phi::is_surname(&w.text));
            if plain_word && safe && !w.is_number && w.syllables >= 4 && !(w.capitalized && i > 0) {
                let e = word_counts
                    .entry(w.text.to_lowercase())
                    .or_insert((w.syllables, 0));
                e.1 += 1;
            }
        }
    }
    let mut long_words: Vec<WordCount> = word_counts
        .into_iter()
        .map(|(word, (syllables, count))| WordCount {
            word,
            syllables,
            count,
        })
        .collect();
    long_words.sort_by(|a, b| b.count.cmp(&a.count).then(b.syllables.cmp(&a.syllables)));
    long_words.truncate(max);

    let mut long_paragraphs = Vec::new();
    let mut p_index = 0;
    for b in &parsed.blocks {
        if b.kind == BlockKind::Paragraph {
            p_index += 1;
            if b.sentences.len() > th.paragraph_max_sentences {
                long_paragraphs.push(StructureFlag {
                    kind: "paragraph".into(),
                    position: p_index,
                    size: b.sentences.len(),
                    limit: th.paragraph_max_sentences,
                    suggestion: "Break it into shorter paragraphs, one topic each, with headings."
                        .into(),
                });
            }
        }
    }
    long_paragraphs.truncate(max);
    let mut list_sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for b in &parsed.blocks {
        if let Some(l) = b.list {
            *list_sizes.entry(l).or_default() += 1;
        }
    }
    let long_lists: Vec<StructureFlag> = list_sizes
        .iter()
        .filter(|(_, n)| **n > th.list_max_items)
        .map(|(l, n)| StructureFlag {
            kind: "list".into(),
            position: *l,
            size: *n,
            limit: th.list_max_items,
            suggestion: "Break the list into shorter lists under subheadings (3 to 7 items each)."
                .into(),
        })
        .collect();

    let mut jargon = find_jargon(parsed);
    jargon.truncate(max.max(1) * 2);
    let mut acronyms = find_acronyms(text);
    if red {
        // From a file, report only letter acronyms (an ID number could look like "AB1234").
        acronyms.retain(|a| {
            let core = a.acronym.split('-').next().unwrap_or_default();
            core.chars().all(|c| c.is_ascii_uppercase())
        });
    }
    acronyms.truncate(max);

    Flags {
        long_sentences: long,
        long_sentence_count: long_count,
        jargon,
        passive_voice: passive,
        passive_voice_count: passive_count,
        acronyms,
        long_words,
        long_paragraphs,
        long_lists,
        negative_instructions: negative,
        numbers: number_summary(text),
    }
}

// ---------------------------------------------------------------- Clear Communication Index

fn normalize_answer(item: &CciItem, raw: &str) -> Result<String> {
    let a = raw.trim().to_ascii_lowercase();
    let a = match a.as_str() {
        "yes" | "y" | "1" | "true" => "yes",
        "no" | "n" | "0" | "false" => "no",
        "na" | "n/a" | "not applicable" => "na",
        _ => {
            return Err(PhnError::InvalidInput(format!(
                "Clear Communication Index item {}: answer '{raw}' is not yes, no, or na",
                item.number
            )));
        }
    };
    if a == "na" && !item.na_allowed {
        return Err(PhnError::InvalidInput(format!(
            "Clear Communication Index item {} cannot be 'na' (only items 19 and 20 can)",
            item.number
        )));
    }
    Ok(a.to_string())
}

fn gate(answers: &BTreeMap<String, String>, key: &str) -> Result<Option<bool>> {
    match answers
        .iter()
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        .map(|(_, v)| v.trim().to_ascii_lowercase())
    {
        None => Ok(None),
        Some(v) if matches!(v.as_str(), "yes" | "y" | "true" | "1") => Ok(Some(true)),
        Some(v) if matches!(v.as_str(), "no" | "n" | "false" | "0") => Ok(Some(false)),
        Some(v) => Err(PhnError::InvalidInput(format!(
            "Clear Communication Index Part {key}: '{v}' is not yes or no"
        ))),
    }
}

/// Score the Clear Communication Index from answers, with suggestions from the analysis.
pub fn score_cci(
    answers: &BTreeMap<String, String>,
    analysis: Option<&Analysis>,
) -> Result<CciResult> {
    let cci = &CONTENT.cci;
    for k in answers.keys() {
        let k = k.trim();
        let known = k.eq_ignore_ascii_case("C")
            || k.eq_ignore_ascii_case("D")
            || k.parse::<u8>().is_ok_and(|n| (1..=20).contains(&n));
        if !known {
            return Err(PhnError::InvalidInput(format!(
                "cci_answers key '{k}' is not an item number (1-20) or a part (C, D)"
            )));
        }
    }
    let mut given: BTreeMap<u8, String> = BTreeMap::new();
    for item in &cci.items {
        if let Some((_, v)) = answers
            .iter()
            .find(|(k, _)| k.trim().parse::<u8>().ok() == Some(item.number))
        {
            given.insert(item.number, normalize_answer(item, v)?);
        }
    }
    let answered = |n: u8| given.get(&n).map(String::as_str);
    let any_answered = |r: std::ops::RangeInclusive<u8>| r.clone().any(|n| given.contains_key(&n));
    let mut notes = Vec::new();

    // Which parts apply.
    let b = match answered(12) {
        Some("yes") => Some(true),
        Some(_) => Some(false),
        None => None,
    };
    let c = match gate(answers, "C")? {
        Some(v) => Some(v),
        None if any_answered(15..=17) => Some(true),
        None => match analysis {
            Some(a) if a.flags.numbers.numbers == 0 => {
                notes.push("No numbers were found, so Part C (numbers) is treated as not applying; answer \"C\": \"yes\" if that is wrong.".into());
                Some(false)
            }
            _ => None,
        },
    };
    let d = match gate(answers, "D")? {
        Some(v) => Some(v),
        None if any_answered(18..=20) => Some(true),
        None => None,
    };
    let mut parts_apply = BTreeMap::new();
    parts_apply.insert("A".to_string(), Some(true));
    parts_apply.insert("B".to_string(), b);
    parts_apply.insert("C".to_string(), c);
    parts_apply.insert("D".to_string(), d);

    let suggestions = analysis.map(cci_suggestions).unwrap_or_default();
    let mut items = Vec::new();
    let mut earned = 0u32;
    let mut possible = 0u32;
    let mut unanswered = Vec::new();
    for item in &cci.items {
        let applies = match item.part.as_str() {
            "A" => Some(true),
            "B" if item.number == 12 => Some(true),
            p => parts_apply.get(p).copied().flatten(),
        };
        let mut answer = answered(item.number).map(str::to_string);
        // Score-sheet rules: "No" to item 1 scores 0 for items 2-4; "No" to 1 or 5 makes 6 "No".
        let forced_no = (matches!(item.number, 2..=4) && answered(1) == Some("no"))
            || (item.number == 6 && (answered(1) == Some("no") || answered(5) == Some("no")));
        if forced_no && answer.as_deref() != Some("no") {
            if answer.is_some() {
                notes.push(format!(
                    "Item {} is scored 0 because item {} was answered no (score-sheet rule).",
                    item.number,
                    if item.number == 6 && answered(1) != Some("no") {
                        5
                    } else {
                        1
                    }
                ));
            }
            answer = Some("no".into());
        }
        let points = match (applies, answer.as_deref()) {
            (Some(true), Some("na")) => None,
            (Some(true), Some(a)) => {
                let p = if a == "yes" {
                    item.yes_scores
                } else {
                    1 - item.yes_scores
                };
                possible += 1;
                earned += u32::from(p);
                Some(p)
            }
            (Some(true), None) => {
                unanswered.push(item.number);
                None
            }
            (None, None) => {
                unanswered.push(item.number);
                None
            }
            _ => None,
        };
        let (suggestion, reason) = suggestions
            .get(&item.number)
            .cloned()
            .map(|(s, r)| (Some(s), Some(r)))
            .unwrap_or((None, None));
        items.push(CciItemResult {
            number: item.number,
            part: item.part.clone(),
            area: item.area.clone(),
            question: item.question.clone(),
            guidance: item.guidance.clone(),
            user_guide_page: item.guide_page,
            answer,
            points,
            suggestion,
            suggestion_reason: reason,
        });
    }
    // Part B counts only when item 12 is yes.
    if b == Some(false) {
        if let Some(i12) = items.iter_mut().find(|i| i.number == 12)
            && i12.points.is_some()
        {
            possible -= 1;
            i12.points = None;
        }
        unanswered.retain(|n| !(12..=14).contains(n));
        notes.push("Item 12 is no, so Part B is not scored.".into());
    }
    let complete = unanswered.is_empty() && parts_apply.values().all(Option::is_some);
    if !complete && parts_apply.values().any(Option::is_none) {
        notes.push("Answer item 12 (Part B), \"C\" (numbers), and \"D\" (risk) so the tool knows which parts apply.".into());
    }
    let score = complete.then(|| round1(f64::from(earned) / f64::from(possible.max(1)) * 100.0));
    let part_questions = cci
        .parts
        .iter()
        .filter_map(|p| {
            p.applies_question
                .as_ref()
                .map(|q| (p.id.clone(), format!("Part {} ({}): {q}", p.id, p.name)))
        })
        .collect();
    Ok(CciResult {
        before_you_begin: cci.intro_questions.clone(),
        rules: cci.rules.clone(),
        part_questions,
        items,
        parts_apply,
        score,
        points_earned: complete.then_some(earned),
        points_possible: complete.then_some(possible),
        passing: score.map(|s| s >= cci.passing_score),
        unanswered,
        notes,
    })
}

/// The tool's suggestions for items it can judge from the text.
fn cci_suggestions(a: &Analysis) -> BTreeMap<u8, (String, String)> {
    let mut s = BTreeMap::new();
    let f = &a.flags;
    if a.counts.list_items == 0 {
        s.insert(
            8,
            (
                "no".into(),
                "No bulleted or numbered list was found.".into(),
            ),
        );
    } else if !f.long_lists.is_empty() {
        s.insert(
            8,
            (
                "no".into(),
                format!(
                    "A list has more than {} items.",
                    CONTENT.thresholds.list_max_items
                ),
            ),
        );
    } else {
        s.insert(
            8,
            (
                "check".into(),
                "Lists were found; check that they are not only references at the end.".into(),
            ),
        );
    }
    if a.counts.headings == 0 {
        s.insert(9, ("no".into(), "No headings were found (use real headings, such as Markdown # or Word heading styles).".into()));
    } else {
        s.insert(
            9,
            (
                "check".into(),
                "Headings were found; check that each chunk has one idea and its heading matches."
                    .into(),
            ),
        );
    }
    let real_jargon: Vec<&str> = f
        .jargon
        .iter()
        .filter(|j| !j.check_context)
        .map(|j| j.term.as_str())
        .take(5)
        .collect();
    if !real_jargon.is_empty() || !f.acronyms.is_empty() {
        let mut why = Vec::new();
        if !real_jargon.is_empty() {
            why.push(format!("jargon such as {}", real_jargon.join(", ")));
        }
        if !f.acronyms.is_empty() {
            why.push(format!(
                "acronyms not spelled out ({})",
                f.acronyms
                    .iter()
                    .take(5)
                    .map(|a| a.acronym.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        s.insert(
            7,
            (
                "no".into(),
                format!(
                    "Found {}, unless each is explained the first time.",
                    why.join(" and ")
                ),
            ),
        );
    }
    if let Some(first) = f.passive_voice.first().filter(|p| p.sentence <= 3) {
        s.insert(
            6,
            (
                "check".into(),
                format!(
                    "Possible passive voice near the start (sentence {}: \"{}\").",
                    first.sentence,
                    first.detail.clone().unwrap_or_default()
                ),
            ),
        );
    }
    if f.numbers.numbers == 0 {
        s.insert(
            15,
            (
                "na".into(),
                "No numbers were found, so Part C may not apply.".into(),
            ),
        );
    } else if f.numbers.decimals > 0 || f.numbers.percentages > 0 {
        s.insert(
            16,
            (
                "check".into(),
                format!(
                    "{} percentage(s) and {} decimal(s) found; explain what each means.",
                    f.numbers.percentages, f.numbers.decimals
                ),
            ),
        );
    }
    if f.numbers.percentages + f.numbers.ratios > 0 {
        s.insert(20, ("check".into(), "Numeric probabilities (percentages or 'x in y') were found; explain them in words or a visual.".into()));
    } else {
        s.insert(
            20,
            ("na".into(), "No numeric probability was found.".into()),
        );
    }
    s
}

// ---------------------------------------------------------------- the tool

/// Text to check from a local file, as Markdown-like text (headings and lists kept for DOCX).
fn file_text(ctx: &Context, path: &str) -> Result<(String, String, Vec<String>)> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let label = sandbox.display_path(&p);
    let mut notes = Vec::new();
    let text = match FileKind::of(&p) {
        FileKind::Docx => {
            let doc = phn_local::docx::read_docx(&p)?;
            let mut out = String::new();
            for b in &doc.blocks {
                if let phn_local::docx::Block::Paragraph(par) = b {
                    let t = par.text.trim();
                    if t.is_empty() || par.in_table {
                        continue;
                    }
                    if par.heading_level.is_some() {
                        out.push_str(&format!("# {t}\n\n"));
                    } else if par.is_list {
                        out.push_str(&format!("- {t}\n"));
                    } else {
                        out.push_str(&format!("\n{t}\n\n"));
                    }
                }
            }
            out
        }
        FileKind::Text | FileKind::Markdown | FileKind::Pdf | FileKind::Html => {
            let e = phn_local::extract::extract(&p)?;
            notes.extend(e.notes);
            if e.kind == FileKind::Html {
                e.text.lines().collect::<Vec<_>>().join("\n\n")
            } else {
                e.text
            }
        }
        _ => {
            return Err(PhnError::Unsupported(
                "check_readability reads DOCX, HTML, PDF, TXT, and Markdown files".into(),
            ));
        }
    };
    Ok((text, label, notes))
}

fn english_share(parsed: &Parsed) -> Option<f64> {
    const COMMON: &[&str] = &[
        "the", "and", "to", "of", "a", "in", "is", "you", "your", "for", "it", "that", "are",
        "with", "on", "be", "can", "if", "or", "this", "have", "not", "at", "get", "do",
    ];
    let words: Vec<String> = parsed
        .counted_sentences()
        .flat_map(|s| s.words.iter().map(|w| w.text.to_lowercase()))
        .collect();
    if words.len() < 30 {
        return None;
    }
    let n = words
        .iter()
        .filter(|w| COMMON.contains(&w.as_str()))
        .count();
    Some(n as f64 / words.len() as f64)
}

/// Check a draft's reading level and plain language.
pub fn check_readability(
    ctx: &Context,
    args: ReadabilityArgs,
) -> Result<Envelope<ReadabilityResult>> {
    let th = &CONTENT.thresholds;
    let target = args.target_grade.unwrap_or(th.default_target_grade);
    if !(1.0..=18.0).contains(&target) {
        return Err(PhnError::InvalidInput(
            "target_grade must be between 1 and 18".into(),
        ));
    }
    let max_flags = args.max_flags.unwrap_or(10).clamp(1, 50);
    let (text, checked, from_file, mut file_notes) = match (
        args.text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty()),
        args.path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty()),
    ) {
        (Some(_), Some(_)) => {
            return Err(PhnError::InvalidInput(
                "give either text or path, not both".into(),
            ));
        }
        (Some(t), None) => (t.to_string(), "pasted text".to_string(), false, Vec::new()),
        (None, Some(p)) => {
            let (t, label, notes) = file_text(ctx, p)?;
            (t, label, true, notes)
        }
        (None, None) => {
            return Err(PhnError::InvalidInput(
                "give the draft as text, or the path of a file in /data".into(),
            ));
        }
    };
    if text.chars().count() > 200_000 {
        return Err(PhnError::InvalidInput(
            "the text is longer than 200,000 characters; check one section at a time".into(),
        ));
    }
    let opts = AnalyzeOptions {
        target_grade: target,
        max_flags,
        redact_excerpts: from_file,
    };
    let a = analyze(&text, &opts);
    if a.counts.words == 0 {
        return Err(PhnError::InvalidInput(format!(
            "{checked} has no words to check"
        )));
    }
    let cci = score_cci(&args.cci_answers, Some(&a))?;

    // Privacy: identifiers found in a file are reported by category; excerpts are redacted.
    let mut privacy = Privacy::none();
    let found = phi::scan_text(&text);
    if !found.is_empty() {
        let mut cats: Vec<String> = found.iter().map(|f| f.kind.label().to_string()).collect();
        cats.sort();
        cats.dedup();
        privacy.identifiers_detected = cats;
        privacy.notes.push(if from_file {
            "Possible identifiers were found in the file; quoted excerpts are redacted. Remove client details before publishing.".into()
        } else {
            "Possible identifiers were found in the text. Remove client details before publishing.".into()
        });
    }

    // ---- summary
    let g = &a.grade;
    let mut summary = format!(
        "**Readability check: {checked}**\n\nEstimated reading level: **grade {:.1}** (median of Flesch-Kincaid, SMOG, and Fog; range {:.1} to {:.1}). Target: grade {target} — {}.\n\n",
        g.median_grade,
        g.lowest_grade,
        g.highest_grade,
        if g.meets_target {
            "meets the target"
        } else {
            "**above the target**"
        }
    );
    summary.push_str(&md::table(
        &["Measure", "Score", "Meaning"],
        &a.scores
            .iter()
            .map(|s| {
                vec![
                    s.name.clone(),
                    format!("{:.1}", s.value),
                    s.interpretation.clone(),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    summary.push_str(&format!(
        "\n{} words, {} sentences (average {:.1} words per sentence, {:.2} syllables per word), {} paragraph(s), {} heading(s), {} list(s). SMOG table: {} ({}).\n",
        a.counts.words,
        a.counts.sentences,
        a.counts.average_words_per_sentence,
        a.counts.average_syllables_per_word,
        a.counts.paragraphs,
        a.counts.headings,
        a.counts.lists,
        a.smog
            .table_grade
            .map(|g| if g >= 13 { "college level".to_string() } else { format!("grade {g}") })
            .unwrap_or_else(|| "beyond the table (above college level)".into()),
        a.smog.sample
    ));
    let f = &a.flags;
    summary.push_str("\n**What to fix**\n");
    let mut any = false;
    if f.long_sentence_count > 0 {
        any = true;
        summary.push_str(&format!(
            "- {} sentence(s) longer than {} words, e.g. sentence {} ({} words): \"{}\"\n",
            f.long_sentence_count,
            th.long_sentence_words,
            f.long_sentences[0].sentence,
            f.long_sentences[0].words,
            f.long_sentences[0].excerpt
        ));
    }
    let strong: Vec<&JargonFlag> = f.jargon.iter().filter(|j| !j.check_context).collect();
    if !strong.is_empty() {
        any = true;
        summary.push_str("- Jargon with everyday alternatives (CDC Everyday Words): ");
        summary.push_str(
            &strong
                .iter()
                .take(8)
                .map(|j| {
                    format!(
                        "*{}* → {}",
                        j.found.first().cloned().unwrap_or_else(|| j.term.clone()),
                        j.plain_alternative
                            .clone()
                            .unwrap_or_else(|| "see note".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        );
        summary.push('\n');
    }
    let soft: Vec<&str> = f
        .jargon
        .iter()
        .filter(|j| j.check_context)
        .map(|j| j.term.as_str())
        .collect();
    if !soft.is_empty() {
        summary.push_str(&format!(
            "- Check how these are used (often fine, sometimes jargon): {}\n",
            soft.iter().take(10).copied().collect::<Vec<_>>().join(", ")
        ));
    }
    if f.passive_voice_count > 0 {
        any = true;
        summary.push_str(&format!(
            "- {} sentence(s) with possible passive voice, e.g. \"{}\"\n",
            f.passive_voice_count,
            f.passive_voice[0].detail.clone().unwrap_or_default()
        ));
    }
    if !f.acronyms.is_empty() {
        any = true;
        summary.push_str(&format!(
            "- Acronyms to spell out at first use: {}\n",
            f.acronyms
                .iter()
                .map(|a| format!("{} ({})", a.acronym, a.problem))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !f.long_words.is_empty() {
        summary.push_str(&format!(
            "- Long words to replace where you can: {}\n",
            f.long_words
                .iter()
                .take(8)
                .map(|w| w.word.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for p in f.long_paragraphs.iter().chain(f.long_lists.iter()) {
        any = true;
        summary.push_str(&format!(
            "- {} {} has {} {} (limit {}). {}\n",
            if p.kind == "list" {
                "List"
            } else {
                "Paragraph"
            },
            p.position,
            p.size,
            if p.kind == "list" {
                "items"
            } else {
                "sentences"
            },
            p.limit,
            p.suggestion
        ));
    }
    if !f.negative_instructions.is_empty() {
        summary.push_str(&format!(
            "- {} instruction(s) say what not to do; say what to do where you can.\n",
            f.negative_instructions.len()
        ));
    }
    if !any {
        summary.push_str(
            "- No long sentences, jargon, passive voice, or structure problems were flagged.\n",
        );
    }
    summary.push_str("\n**CDC Clear Communication Index**\n");
    match cci.score {
        Some(s) => summary.push_str(&format!(
            "Score **{s:.0}** ({} of {} points) — {}.\n",
            cci.points_earned.unwrap_or(0),
            cci.points_possible.unwrap_or(0),
            if s >= CONTENT.cci.passing_score {
                "passing (90 or higher)"
            } else {
                "below 90: revise the items that scored 0 and score again"
            }
        )),
        None => {
            summary.push_str(&format!(
                "Not scored yet: the Index needs your judgment. Answer items {} in `cci_answers` (yes/no). ",
                compress_ranges(&cci.unanswered)
            ));
            let sug: Vec<String> = cci
                .items
                .iter()
                .filter_map(|i| i.suggestion.as_ref().map(|s| format!("{} → {s}", i.number)))
                .collect();
            if !sug.is_empty() {
                summary.push_str(&format!(
                    "Tool suggestions from the text: {}.",
                    sug.join(", ")
                ));
            }
            summary.push('\n');
        }
    }
    if !cci.items.iter().any(|i| i.points == Some(0)) {
    } else {
        let zeros: Vec<String> = cci
            .items
            .iter()
            .filter(|i| i.points == Some(0))
            .map(|i| format!("{} ({})", i.number, i.area.to_lowercase()))
            .collect();
        summary.push_str(&format!("Items scoring 0: {}.\n", zeros.join(", ")));
    }

    let tips: Vec<String> = CONTENT
        .guidance
        .iter()
        .map(|g| format!("{} ({}, {})", g.text, source_label(&g.source), g.section))
        .collect();

    // ---- sources and caveats
    let meta = &CONTENT.meta;
    let mut sources = content::cite(
        &CONTENT.sources,
        &[
            "cci_score_sheet",
            "cci_user_guide",
            "everyday_words",
            "simply_put",
            "cdc_plain_language",
            "health_literacy_online",
        ],
        &meta.prepared,
    );
    sources.extend(content::cite(
        &CONTENT.sources,
        &[
            "flesch_1948",
            "kincaid_1975",
            "mclaughlin_1969",
            "gunning_1952",
        ],
        &meta.prepared,
    ));
    for s in &mut sources {
        if s.url.is_none() {
            s.mode = phn_core::SourceMode::Computed;
        }
    }
    if from_file {
        sources.insert(0, phn_core::Source::local_file(&checked));
    }
    let mut caveats = vec![
        "Readability formulas count syllables and sentence length only; they do not measure layout, audience fit, or understanding. Test drafts with members of the audience (CDC Clear Communication Index User Guide; Simply Put).".to_string(),
        "Syllables are estimated with rules (about 95% agreement with dictionary counts), so grades can differ by up to about one grade from other software.".to_string(),
        format!("Target grade {target}: {}", if args.target_grade.is_some() { "set by you." } else { th.default_target_grade_note.as_str() }),
    ];
    if a.counts.words < 100 {
        caveats.push(format!(
            "Only {} words: reading-level formulas are unreliable for very short texts (SMOG is designed for 30 sentences; Fry for 100-word samples).",
            a.counts.words
        ));
    }
    if english_share(&a.parsed).is_some_and(|s| s < 0.08) {
        caveats.push("The text does not look like English. These formulas and the jargon list are for English only; for other languages, rely on a qualified translator and testing with readers.".into());
    }
    caveats.push(format!(
        "Thresholds: long sentences over {} words ({}); paragraphs over {} sentences ({}); lists over {} items ({}).",
        th.long_sentence_words, th.long_sentence_source, th.paragraph_max_sentences, th.paragraph_source, th.list_max_items, th.list_source
    ));
    caveats.push(format!(
        "SMOG table: {} {}",
        CONTENT.smog_table.section, CONTENT.smog_table.accuracy
    ));
    caveats.push(format!(
        "Passing Clear Communication Index score: {}",
        CONTENT.cci.passing_source
    ));
    caveats.append(&mut file_notes);
    caveats.extend(content::review_caveats(&[READABILITY]));
    let method = "Text is split into headings, list items, and paragraphs; headings are not counted, and each list item is a sentence. Sentences end at . ! or ? (abbreviations, initials, and decimals excepted). Hyphenated words count once; numbers are one-syllable words. Syllables: vowel groups with silent-e, -le, -ed, -es, and vowel-pair corrections. Flesch Reading Ease = 206.835 - 1.015 ASL - 84.6 ASW; Flesch-Kincaid = 0.39 ASL + 11.8 ASW - 15.59; SMOG = 1.0430 sqrt(polysyllables x 30 / sentences) + 3.1291; Fog = 0.4 (ASL + 100 x complex words / words). The grade estimate is the median of Flesch-Kincaid, SMOG, and Fog. Clear Communication Index points follow the CDC score sheet (item 17 reverse-scored; 'no' to item 1 scores 0 for items 2-4; NA removes items 19-20 from the points possible).".to_string();
    Ok(Envelope::new(
        summary,
        ReadabilityResult {
            checked,
            counts: a.counts.clone(),
            scores: a.scores.clone(),
            smog: a.smog.clone(),
            fry: a.fry.clone(),
            grade_estimate: a.grade.clone(),
            flags: a.flags.clone(),
            clear_communication_index: cci,
            tips,
            method,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}

fn compress_ranges(ns: &[u8]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < ns.len() {
        let mut j = i;
        while j + 1 < ns.len() && ns[j + 1] == ns[j] + 1 {
            j += 1;
        }
        out.push(if j > i {
            format!("{}-{}", ns[i], ns[j])
        } else {
            ns[i].to_string()
        });
        i = j + 1;
    }
    if out.is_empty() {
        "none".into()
    } else {
        out.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_parses_with_twenty_cci_items_and_jargon() {
        assert_eq!(CONTENT.cci.items.len(), 20);
        assert!(CONTENT.jargon.len() >= 100);
        assert_eq!(CONTENT.cci.parts.len(), 4);
        assert!(
            CONTENT
                .cci
                .parts
                .iter()
                .any(|p| p.id == "B" && p.applies_question.is_some())
        );
        assert!(CONTENT.cci.rules.len() >= 4);
        assert!(!CONTENT.cci.parts[0].name.is_empty());
    }

    #[test]
    fn simply_put_passive_and_jargon_examples() {
        let a = analyze(
            "Heart disease and lung cancer are caused by smoking. Smoking causes heart disease and lung cancer. Tests may be needed to find out what's wrong. You may need a test to find out what's wrong. Exposure to the chemical could cause adverse health effects.",
            &AnalyzeOptions::default(),
        );
        let passive: Vec<usize> = a.flags.passive_voice.iter().map(|p| p.sentence).collect();
        assert_eq!(passive, vec![1, 3], "{:?}", a.flags.passive_voice);
        let terms: Vec<&str> = a.flags.jargon.iter().map(|j| j.term.as_str()).collect();
        assert!(terms.contains(&"Exposure, exposed"), "{terms:?}");
        assert!(terms.contains(&"Adverse health effects"), "{terms:?}");
    }

    #[test]
    fn acronyms_need_spelling_out_at_first_use() {
        let f = find_acronyms(
            "Get the RSV shot. Respiratory syncytial virus (RSV) is common. Talk to your health care provider about MMR. Human papillomavirus (HPV) causes cancer. HPV shots work.",
        );
        let got: Vec<(&str, &str)> = f
            .iter()
            .map(|a| (a.acronym.as_str(), a.problem.as_str()))
            .collect();
        assert!(
            got.contains(&("RSV", "spelled out after its first use")),
            "{got:?}"
        );
        assert!(got.contains(&("MMR", "never spelled out")), "{got:?}");
        assert!(!got.iter().any(|(a, _)| *a == "HPV"), "{got:?}");
    }

    #[test]
    fn jargon_matches_phrases_before_words_and_inflections() {
        let p = textstats::parse(
            "Risk factors include smoking. Monitoring is done often. Utilizing this, we utilized more. Use contraception.",
        );
        let j = find_jargon(&p);
        let terms: Vec<&str> = j.iter().map(|j| j.term.as_str()).collect();
        assert!(
            terms.contains(&"Risk factor") && !terms.contains(&"Risk"),
            "{terms:?}"
        );
        assert!(
            terms.contains(&"Monitor") && terms.contains(&"Contraception"),
            "{terms:?}"
        );
        let u = j.iter().find(|j| j.term == "Utilize").unwrap();
        assert_eq!(u.count, 2);
    }
}
