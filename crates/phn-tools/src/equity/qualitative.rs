//! EQ-10 `analyze_qualitative_data`: helps analyze focus group and key informant
//! transcripts the nurse saves in /data.
//!
//! Privacy design (the transcripts are the most identifying files this server
//! reads, so this tool is stricter than the others):
//!
//! * Transcripts are read only on this machine. Nothing from them is sent
//!   anywhere except what this tool returns, which is only counts: a scrub
//!   report (how many names and other identifiers were removed, by kind), the
//!   codebook (the nurse's own or the project's starter list), and for each
//!   theme the number of different participants who raised it. A theme that
//!   fewer than three different participants raised is withheld, so one person
//!   cannot be picked out from a rare theme. No transcript sentence, speaker
//!   label, or file name is returned, and there is no option to return one.
//! * Speaker labels become P1, P2, ... within each transcript. Names are
//!   removed in three layers: names from the speaker labels and from a names
//!   file the nurse keeps in /data, capitalized words in the middle of a
//!   sentence (names, organizations, and places that are not Massachusetts
//!   towns), and the server's pattern detector (phone numbers, email, street
//!   addresses, dates, record numbers, titled names).
//! * The coded excerpt matrix (de-identified excerpts under each code) and a
//!   theme summary are written to /data/output only.
//!
//! Keyword coding is a heuristic: it finds words, not meaning. The nurse can
//! supply her own codebook. Read the excerpts before reporting any finding.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, TableBlock};
use phn_local::extract::{FileKind, extract};
use phn_local::phi::{self, RedactionLevel};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Confidence};
use crate::md;
use crate::school::templates;

/// Content id of `content/equity/qualitative_codebook.yaml`.
pub const QUALITATIVE_CODEBOOK: &str = "equity-qualitative-codebook";

/// Most transcript files in one call.
pub const MAX_TRANSCRIPTS: usize = 60;
/// Largest transcript file (bytes).
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Most words analyzed in one call.
const MAX_WORDS: usize = 600_000;
/// Most names read from a names file.
const MAX_NAMES: usize = 5_000;
/// Most codes in a codebook.
const MAX_CODES: usize = 200;
/// A turn longer than this (characters) is split into segments.
const SPLIT_ABOVE: usize = 1_200;
const SEGMENT_TARGET: usize = 800;
/// Candidate new codes reported.
const MAX_CANDIDATES: usize = 15;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CodeDef {
    code: String,
    theme: String,
    definition: String,
    keywords: Vec<String>,
    confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct QualContent {
    min_voices_floor: u32,
    excerpt_max_chars: usize,
    facilitator_labels: Vec<String>,
    non_speaker_labels: Vec<String>,
    keep_capitalized: Vec<String>,
    stopwords: Vec<String>,
    codes: Vec<CodeDef>,
}

// ---------------------------------------------------------------- arguments

/// Arguments for `analyze_qualitative_data`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct QualitativeArgs {
    /// Transcript files or folders in /data (TXT, MD, DOCX, PDF with text, or HTML), for example
    /// ["focus_groups/"] or ["kii_01.docx", "kii_02.docx"]. Speaker turns written as
    /// "Name: what they said" (or "P3:", "Speaker 2:") are read as turns; text without speaker
    /// labels is treated as one speaker. At most 60 files. Transcripts are read only on this
    /// machine; the response has counts only.
    pub transcript_paths: Vec<String>,
    /// Your own codebook (CSV, XLSX, YAML, or YML in /data) with columns code, definition,
    /// keywords (separated by semicolons; a trailing * matches any ending, so transport*
    /// matches transportation), and optionally theme. Default: the project's starter codebook
    /// (a draft).
    pub codebook_path: Option<String>,
    /// A names file in /data (TXT with one name per line, or a CSV/XLSX with name columns): the
    /// participants, staff, and anyone else named in the transcripts. The names are removed
    /// wherever they appear and are never returned. Optional but recommended.
    pub names_path: Option<String>,
    /// Extra speaker labels that are the facilitator or interviewer, such as "Moderator" or
    /// "Dana (note taker)": role words or initials rather than names are best. Their turns are
    /// not coded. "Interviewer", "Facilitator", "Moderator", and "Q" are always recognized.
    pub facilitator_labels: Vec<String>,
    /// Fewest different participants who must raise a theme before its counts are shown
    /// (default 3, the minimum allowed).
    pub min_voices: Option<u32>,
    /// Write the coded excerpt matrix (XLSX) and a theme summary (DOCX) to /data/output
    /// (default true). They hold de-identified excerpts.
    pub write_outputs: Option<bool>,
    /// Also write each de-identified transcript as a text file to /data/output (default false).
    pub write_deidentified_transcripts: Option<bool>,
}

// ---------------------------------------------------------------- results

/// A count of identifiers removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScrubCount {
    pub kind: String,
    pub removed: usize,
}

/// What was removed from the transcripts (counts only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScrubReport {
    /// Speaker labels replaced by P1, P2, ... (turns).
    pub speaker_labels_replaced: usize,
    /// Names from your names file that were looked for.
    pub names_from_file: usize,
    /// Removed identifiers by kind (names and other capitalized words, phone numbers, ...).
    pub removed: Vec<ScrubCount>,
    /// Total removed.
    pub total_removed: usize,
    /// Identifier-like text still found after scrubbing (removed on a second pass); the second
    /// pass count, not a measure of what a person would still recognize.
    pub second_pass_removed: usize,
    pub method: String,
}

/// Counts for one transcript (it is not named: T1, T2, ... follow the order given).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TranscriptCounts {
    pub id: String,
    pub words: usize,
    /// Different participants (not counting the facilitator).
    pub participants: usize,
    pub participant_segments: usize,
    pub coded_segments: usize,
}

/// A code in the codebook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CodeOut {
    pub code: String,
    pub theme: String,
    pub definition: String,
    pub keywords: Vec<String>,
}

/// Counts for one code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThemeCount {
    pub code: String,
    pub theme: String,
    /// False when fewer than `min_voices` different participants raised it: the counts are
    /// withheld (null).
    pub reported: bool,
    /// Different participants (across all transcripts) whose words matched this code.
    pub participants: Option<usize>,
    /// Transcripts in which it came up.
    pub transcripts: Option<usize>,
    /// Coded segments (a participant's turn, or part of a long turn).
    pub segments: Option<usize>,
    /// Codebook keywords that matched for at least `min_voices` participants (the keywords are
    /// yours, not quotes).
    pub keywords_matched: Vec<String>,
    /// One plain sentence, built from the counts and the code's definition.
    pub summary: String,
}

/// A rollup by theme (group of codes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThemeGroup {
    pub theme: String,
    pub codes: Vec<String>,
    pub reported: bool,
    /// Different participants who raised any code in the theme.
    pub participants: Option<usize>,
}

/// A word that came up often but is not in the codebook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CandidateTerm {
    pub term: String,
    pub participants: usize,
}

/// Where the codebook came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CodebookOut {
    /// "starter (project draft, low confidence)" or "your file".
    pub source: String,
    pub codes: Vec<CodeOut>,
}

/// Result of `analyze_qualitative_data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QualitativeResult {
    pub transcripts: Vec<TranscriptCounts>,
    pub total_words: usize,
    /// Different participants across all transcripts (a person in two transcripts counts twice:
    /// speaker labels are not matched across files).
    pub total_participants: usize,
    pub facilitator_turns_excluded: usize,
    pub participant_segments: usize,
    pub coded_segments: usize,
    pub scrub: ScrubReport,
    pub codebook: CodebookOut,
    /// The smallest number of different participants required to show a theme.
    pub min_voices: u32,
    pub themes: Vec<ThemeCount>,
    pub theme_groups: Vec<ThemeGroup>,
    /// Themes withheld because too few participants raised them (their labels are not shown).
    pub themes_withheld: usize,
    /// Words used by at least `min_voices` participants that no code matched, as ideas for new
    /// codes (lower-case words only; names are excluded).
    pub candidate_terms: Vec<CandidateTerm>,
    /// Files written to /data/output (de-identified).
    pub files: Vec<String>,
    pub method_note: String,
}

// ---------------------------------------------------------------- parsing

static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*[\[(]?\d{1,2}:\d{2}(?::\d{2})?(?:\.\d+)?[\])]?\s*[-–—]?\s*").unwrap()
});
static LABEL_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?P<label>[^:\n]{1,40}?)\s*:\s*(?P<text>.*)$").unwrap());
static CAP_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\p{Lu}\p{Ll}+(?:['’\-]\p{L}+)*\b").unwrap());
static MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(?:REDACTED: [a-z /-]+|NAME)\]").unwrap());

/// One speaker turn before scrubbing.
#[derive(Debug, Clone, PartialEq)]
struct RawTurn {
    label: Option<String>,
    text: String,
}

fn words_of(label: &str) -> Vec<String> {
    label
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '-'))
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// A line-start "Label:" is a candidate speaker label.
fn candidate_label(line: &str, non_speaker: &HashSet<String>) -> Option<(String, String)> {
    let stripped = TIMESTAMP.replace(line, "");
    let c = LABEL_LINE.captures(&stripped)?;
    let label = c.name("label")?.as_str().trim().to_string();
    let text = c.name("text")?.as_str().to_string();
    if label.is_empty() || label.chars().count() > 40 {
        return None;
    }
    if label
        .chars()
        .any(|ch| matches!(ch, '.' | '?' | '!' | ',' | ';' | '/' | '"' | '“'))
    {
        return None;
    }
    let ws: Vec<&str> = label.split_whitespace().collect();
    if ws.is_empty() || ws.len() > 3 {
        return None;
    }
    // Every word starts with a capital letter or a digit (or is a bracketed role).
    let ok = ws.iter().all(|w| {
        let first = w.trim_start_matches(['(', '[']).chars().next();
        first.is_some_and(|ch| ch.is_uppercase() || ch.is_ascii_digit())
            || w.starts_with('(')
            || w.starts_with('[')
    });
    if !ok {
        return None;
    }
    if non_speaker.contains(&words_of(&label).join(" ")) {
        return None;
    }
    Some((label, text))
}

/// Split a transcript into turns. Lines that start "Label:" begin a turn when at least two such
/// lines exist; other lines continue the turn before them. Without that structure, each
/// paragraph is a turn from one unlabeled speaker.
fn parse_turns(text: &str, non_speaker: &HashSet<String>) -> (Vec<RawTurn>, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let candidates = lines
        .iter()
        .filter(|l| candidate_label(l, non_speaker).is_some())
        .count();
    let mut turns: Vec<RawTurn> = Vec::new();
    if candidates >= 2 {
        let mut preamble = String::new();
        for line in &lines {
            if let Some((label, t)) = candidate_label(line, non_speaker) {
                turns.push(RawTurn {
                    label: Some(label),
                    text: t.trim().to_string(),
                });
            } else if let Some(last) = turns.last_mut() {
                let t = line.trim();
                if !t.is_empty() {
                    if !last.text.is_empty() {
                        last.text.push(' ');
                    }
                    last.text.push_str(t);
                }
            } else if !line.trim().is_empty() {
                preamble.push_str(line.trim());
                preamble.push('\n');
            }
        }
        if !preamble.trim().is_empty() {
            turns.insert(
                0,
                RawTurn {
                    label: None,
                    text: preamble.trim().to_string(),
                },
            );
        }
        return (turns, true);
    }
    let mut para = String::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !para.trim().is_empty() {
                turns.push(RawTurn {
                    label: None,
                    text: para.trim().to_string(),
                });
            }
            para.clear();
        } else {
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(line.trim());
        }
    }
    (turns, false)
}

// ---------------------------------------------------------------- scrubbing

struct ScrubSettings {
    keep: HashSet<String>,
    towns: HashSet<String>,
}

#[derive(Default)]
struct ScrubTally {
    names: usize,
    labels: usize,
    by_kind: BTreeMap<String, usize>,
    second_pass: usize,
}

impl ScrubTally {
    fn add(&mut self, kind: &str, n: usize) {
        if n > 0 {
            *self.by_kind.entry(kind.to_string()).or_default() += n;
        }
    }
}

fn base_word(w: &str) -> String {
    w.split(['\'', '’', '-'])
        .next()
        .unwrap_or(w)
        .to_lowercase()
}

/// Title abbreviations: a period after one does not end a sentence ("Dr. Rivera").
const TITLES: [&str; 12] = [
    "dr", "mr", "mrs", "ms", "miss", "mx", "prof", "rev", "fr", "sr", "jr", "st",
];

/// True when the match starts a sentence (start of text, or after . ? ! or a quote).
fn at_sentence_start(text: &str, start: usize) -> bool {
    let before = text[..start].trim_end_matches(|c: char| {
        c.is_whitespace() || matches!(c, '"' | '“' | '‘' | '\'' | '(' | '[' | '—' | '–' | '-')
    });
    if before.is_empty() {
        return true;
    }
    if let Some(stem) = before.strip_suffix('.') {
        // The word before the period: a title or an initial is not a sentence end.
        let word: String = stem
            .chars()
            .rev()
            .take_while(|c| c.is_alphabetic())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if word.chars().count() == 1 || TITLES.contains(&word.to_lowercase().as_str()) {
            return false;
        }
        return true;
    }
    // A colon is not a sentence end here: a name after "Note taker:" must be removed.
    before.ends_with(['?', '!', '…'])
}

/// Pass 1: collect capitalized words that appear in the middle of sentences.
fn collect_mid_sentence(text: &str, s: &ScrubSettings, out: &mut HashSet<String>) {
    for m in CAP_WORD.find_iter(text) {
        let w = base_word(m.as_str());
        if s.keep.contains(&w) || s.towns.contains(&w) {
            continue;
        }
        if !at_sentence_start(text, m.start()) {
            out.insert(w);
        }
    }
}

/// Replace names (known, and capitalized words that are not kept) with [NAME].
fn scrub_names(
    text: &str,
    known: &HashSet<String>,
    propagated: &HashSet<String>,
    s: &ScrubSettings,
    tally: &mut ScrubTally,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for m in CAP_WORD.find_iter(text) {
        let w = base_word(m.as_str());
        let replace = if known.contains(&w) {
            true
        } else if s.keep.contains(&w) || s.towns.contains(&w) {
            false
        } else if at_sentence_start(text, m.start()) {
            propagated.contains(&w)
        } else {
            true
        };
        if replace {
            out.push_str(&text[pos..m.start()]);
            out.push_str("[NAME]");
            pos = m.end();
            tally.names += 1;
        }
    }
    out.push_str(&text[pos..]);
    // Known names in other cases (all lower case or all capitals) and without a capital.
    out
}

/// Replace known names that the capitalization pass cannot see (lower case, upper case).
fn scrub_known_any_case(
    text: &str,
    known_re: Option<&Regex>,
    tally: &mut ScrubTally,
) -> String {
    match known_re {
        Some(re) => {
            let n = re.find_iter(text).count();
            tally.names += n;
            re.replace_all(text, "[NAME]").into_owned()
        }
        None => text.to_string(),
    }
}

fn known_regex(known: &HashSet<String>) -> Option<Regex> {
    if known.is_empty() {
        return None;
    }
    let mut v: Vec<&String> = known.iter().collect();
    v.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    let alt = v
        .iter()
        .map(|w| regex::escape(w))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&format!(r"(?i)\b(?:{alt})\b")).ok()
}

/// Pattern-based removal (phone, email, address, dates, record numbers, titled names, ...),
/// repeated until nothing the detector recognizes is left.
fn scrub_patterns(text: &str, tally: &mut ScrubTally) -> String {
    let first = phi::redact_text(text, RedactionLevel::Strict);
    for (k, n) in &first.redacted {
        tally.add(k.label(), *n);
    }
    let mut current = first.text;
    for _ in 0..3 {
        let again = phi::redact_text(&current, RedactionLevel::Strict);
        let n: usize = again.redacted.values().sum();
        if n == 0 {
            break;
        }
        tally.second_pass += n;
        for (k, c) in &again.redacted {
            tally.add(k.label(), *c);
        }
        current = again.text;
    }
    current
}

// ---------------------------------------------------------------- coding

#[derive(Debug, Clone)]
struct Keyword {
    code: usize,
    shown: String,
    tokens: Vec<(String, bool)>,
}

fn tokens_of(text: &str) -> Vec<String> {
    let t = MARKER.replace_all(text, " ");
    t.replace('’', "'")
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

fn compile_keyword(code: usize, kw: &str) -> Option<Keyword> {
    let tokens: Vec<(String, bool)> = kw
        .replace('’', "'")
        .to_lowercase()
        .split_whitespace()
        .map(|w| {
            let prefix = w.ends_with('*');
            let core = w.trim_end_matches('*').trim_matches('\'').to_string();
            (core, prefix)
        })
        .filter(|(c, _)| !c.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    Some(Keyword {
        code,
        shown: kw.trim().to_string(),
        tokens,
    })
}

struct Coder {
    keywords: Vec<Keyword>,
    by_first: HashMap<String, Vec<usize>>,
    prefix_first: Vec<usize>,
}

impl Coder {
    fn new(codes: &[CodeDef]) -> Coder {
        let mut keywords = Vec::new();
        for (ci, c) in codes.iter().enumerate() {
            for k in &c.keywords {
                if let Some(kw) = compile_keyword(ci, k) {
                    keywords.push(kw);
                }
            }
        }
        let mut by_first: HashMap<String, Vec<usize>> = HashMap::new();
        let mut prefix_first = Vec::new();
        for (i, k) in keywords.iter().enumerate() {
            let (t, prefix) = &k.tokens[0];
            if *prefix {
                prefix_first.push(i);
            } else {
                by_first.entry(t.clone()).or_default().push(i);
            }
        }
        Coder {
            keywords,
            by_first,
            prefix_first,
        }
    }

    /// Keywords (by index) that match somewhere in the tokens.
    fn matches(&self, toks: &[String]) -> BTreeSet<usize> {
        let mut hit = BTreeSet::new();
        let tail_ok = |k: &Keyword, at: usize| -> bool {
            k.tokens.iter().enumerate().all(|(j, (t, prefix))| {
                toks.get(at + j).is_some_and(|w| {
                    if *prefix {
                        w.starts_with(t.as_str())
                    } else {
                        w == t
                    }
                })
            })
        };
        for (i, w) in toks.iter().enumerate() {
            if let Some(list) = self.by_first.get(w) {
                for &ki in list {
                    if tail_ok(&self.keywords[ki], i) {
                        hit.insert(ki);
                    }
                }
            }
            for &ki in &self.prefix_first {
                if tail_ok(&self.keywords[ki], i) {
                    hit.insert(ki);
                }
            }
        }
        hit
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        if matches!(c, '.' | '?' | '!') && chars.get(i + 1).is_none_or(|n| n.is_whitespace()) {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Cut a long turn into segments of whole sentences.
fn split_segment(text: &str) -> Vec<String> {
    if text.chars().count() <= SPLIT_ABOVE {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for s in split_sentences(text) {
        if !cur.is_empty() && cur.chars().count() + s.chars().count() > SEGMENT_TARGET {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&s);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Cut to at most `max` characters at a word boundary, adding an ellipsis.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let trimmed = match cut.rfind(char::is_whitespace) {
        Some(i) if i > max / 2 => cut[..i].to_string(),
        _ => cut,
    };
    format!("{}…", trimmed.trim_end())
}

/// The sentences of a segment that hold a matched keyword (at most two), shortened.
fn excerpt(text: &str, matched: &Keyword, max: usize) -> String {
    let sentences = split_sentences(text);
    let mut picked: Vec<&String> = Vec::new();
    for s in &sentences {
        let toks = tokens_of(s);
        let probe = Coder {
            keywords: vec![matched.clone()],
            by_first: HashMap::new(),
            prefix_first: vec![0],
        };
        if !probe.matches(&toks).is_empty() {
            picked.push(s);
            if picked.len() == 2 {
                break;
            }
        }
    }
    let joined = if picked.is_empty() {
        text.to_string()
    } else {
        picked
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    shorten(&joined, max)
}

// ---------------------------------------------------------------- local files

fn load_codebook(ctx: &Context, path: &str, c: &QualContent) -> Result<(Vec<CodeDef>, String)> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let label = sandbox.display_path(&p);
    let ext = phn_local::table::extension(&p);
    let mut codes: Vec<CodeDef> = Vec::new();
    if matches!(ext.as_str(), "yaml" | "yml") {
        let text = phn_local::table::decode_text(&std::fs::read(&p)?);
        let v = phn_content::yaml_to_json(&text).map_err(|e| PhnError::Parse {
            path: p.clone(),
            message: e,
        })?;
        let arr = v
            .get("codes")
            .and_then(|x| x.as_array())
            .or_else(|| v.as_array())
            .ok_or_else(|| {
                PhnError::InvalidInput(
                    "the YAML codebook must be a list of codes (code, definition, keywords, \
                     theme), or have a 'codes' list"
                        .into(),
                )
            })?;
        for item in arr {
            let code = item
                .get("code")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let kws: Vec<String> = match item.get("keywords") {
                Some(serde_json::Value::Array(a)) => a
                    .iter()
                    .filter_map(|k| k.as_str().map(|s| s.trim().to_string()))
                    .collect(),
                Some(serde_json::Value::String(s)) => split_keywords(s),
                _ => Vec::new(),
            };
            codes.push(CodeDef {
                code,
                theme: item
                    .get("theme")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                definition: item
                    .get("definition")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                keywords: kws,
                confidence: Confidence::Medium,
            });
        }
    } else if matches!(
        FileKind::of(&p),
        FileKind::Csv | FileKind::Spreadsheet
    ) {
        let t = phn_local::table::read_table(&p, None)?;
        let norm = |s: &str| phn_core::text::normalize(s);
        let find = |names: &[&str]| -> Option<usize> {
            t.headers
                .iter()
                .position(|h| names.iter().any(|n| norm(h) == *n))
        };
        let code_i = find(&["code", "code name", "codes"]);
        let kw_i = find(&["keywords", "keyword", "key words", "terms", "search terms"]);
        let (Some(code_i), Some(kw_i)) = (code_i, kw_i) else {
            return Err(PhnError::InvalidInput(format!(
                "'{label}' is not a codebook: it needs columns named code and keywords (and \
                 optionally definition and theme). Columns found: {}",
                t.headers
                    .iter()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };
        let def_i = find(&["definition", "description", "meaning"]);
        let theme_i = find(&["theme", "category", "domain", "group"]);
        for row in &t.rows {
            let get = |i: Option<usize>| {
                i.and_then(|i| row.get(i))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default()
            };
            let code = get(Some(code_i));
            if code.is_empty() && get(Some(kw_i)).is_empty() {
                continue;
            }
            codes.push(CodeDef {
                code,
                theme: get(theme_i),
                definition: get(def_i),
                keywords: split_keywords(&get(Some(kw_i))),
                confidence: Confidence::Medium,
            });
        }
    } else {
        return Err(PhnError::Unsupported(format!(
            "'{label}': a codebook must be a CSV, XLSX, YAML, or YML file"
        )));
    }
    codes.retain(|c| !c.code.is_empty());
    if codes.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' has no codes: each row needs a code name and keywords"
        )));
    }
    if codes.len() > MAX_CODES {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' has {} codes; use at most {MAX_CODES}",
            codes.len()
        )));
    }
    let mut seen = BTreeSet::new();
    for c in &mut codes {
        if c.code.chars().count() > 60 {
            return Err(PhnError::InvalidInput(
                "a code name is longer than 60 characters".into(),
            ));
        }
        if !seen.insert(c.code.to_lowercase()) {
            return Err(PhnError::InvalidInput(format!(
                "the code '{}' appears twice in the codebook",
                c.code
            )));
        }
        if c.keywords.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "the code '{}' has no keywords",
                c.code
            )));
        }
        if c.theme.is_empty() {
            c.theme = c.code.clone();
        }
    }
    let _ = c;
    Ok((codes, label))
}

fn split_keywords(s: &str) -> Vec<String> {
    s.split([';', '|', '\n'])
        .map(|k| k.trim().trim_matches('"').trim().to_string())
        .filter(|k| !k.is_empty())
        .collect()
}

/// Names from a local file (TXT: one per line; CSV/XLSX: name columns, else every cell). Never
/// returned.
fn load_names(ctx: &Context, path: &str) -> Result<HashSet<String>> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let mut raw: Vec<String> = Vec::new();
    match FileKind::of(&p) {
        FileKind::Csv | FileKind::Spreadsheet => {
            for t in phn_local::table::read_tables(&p)? {
                let name_cols: Vec<usize> = t
                    .headers
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| phn_core::text::normalize(h).contains("name"))
                    .map(|(i, _)| i)
                    .collect();
                for row in &t.rows {
                    if name_cols.is_empty() {
                        raw.extend(row.iter().cloned());
                    } else {
                        raw.extend(name_cols.iter().filter_map(|&i| row.get(i).cloned()));
                    }
                }
            }
        }
        FileKind::Text | FileKind::Markdown | FileKind::Docx => {
            raw.extend(extract(&p)?.text.lines().map(str::to_string));
        }
        _ => {
            return Err(PhnError::Unsupported(
                "a names file must be TXT, CSV, XLSX, or DOCX".into(),
            ));
        }
    }
    let mut out = HashSet::new();
    for cell in raw {
        for w in cell.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '’')) {
            let w = w.trim_matches(['\'', '’']);
            if w.chars().count() >= 2 && !w.chars().all(|c| c.is_ascii_digit()) {
                out.insert(base_word(w));
            }
            if out.len() > MAX_NAMES {
                return Err(PhnError::InvalidInput(format!(
                    "the names file has more than {MAX_NAMES} names; keep only the people who \
                     are named in these transcripts"
                )));
            }
        }
    }
    // Words that are also ordinary words are kept out of the names list only by the user.
    Ok(out)
}

struct TranscriptText {
    text: String,
}

fn collect_transcripts(ctx: &Context, paths: &[String]) -> Result<Vec<TranscriptText>> {
    let sandbox = ctx.sandbox()?;
    let mut files: Vec<PathBuf> = Vec::new();
    for p in paths {
        let resolved = sandbox.resolve_existing(p)?;
        if resolved.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(&resolved)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|f| {
                    f.is_file()
                        && matches!(
                            FileKind::of(f),
                            FileKind::Text
                                | FileKind::Markdown
                                | FileKind::Docx
                                | FileKind::Pdf
                                | FileKind::Html
                        )
                })
                .collect();
            inside.sort();
            files.extend(inside);
        } else {
            files.push(resolved);
        }
    }
    if files.is_empty() {
        return Err(PhnError::InvalidInput(
            "no transcript files were found: give transcript_paths (TXT, MD, DOCX, PDF, or HTML \
             files, or a folder of them, in /data)"
                .into(),
        ));
    }
    if files.len() > MAX_TRANSCRIPTS {
        return Err(PhnError::InvalidInput(format!(
            "{} transcript files found; analyze at most {MAX_TRANSCRIPTS} at a time",
            files.len()
        )));
    }
    let mut out = Vec::new();
    for (k, f) in files.iter().enumerate() {
        let kind = FileKind::of(f);
        if !matches!(
            kind,
            FileKind::Text | FileKind::Markdown | FileKind::Docx | FileKind::Pdf | FileKind::Html
        ) {
            return Err(PhnError::Unsupported(format!(
                "transcript {} is not a document: transcripts must be TXT, MD, DOCX, PDF (with \
                 text), or HTML files. Spreadsheets and data files are not read here.",
                k + 1
            )));
        }
        let len = std::fs::metadata(f)?.len();
        if len > MAX_FILE_BYTES {
            return Err(PhnError::InvalidInput(format!(
                "transcript {} is larger than {} MB",
                k + 1,
                MAX_FILE_BYTES / 1024 / 1024
            )));
        }
        let ex = extract(f)?;
        if ex.text.trim().is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "transcript {} has no text (a scanned PDF needs OCR, which this server does not \
                 have)",
                k + 1
            )));
        }
        out.push(TranscriptText { text: ex.text });
    }
    Ok(out)
}

// ---------------------------------------------------------------- analysis

struct Segment {
    transcript: usize,
    voice: usize,
    /// P1, P2, ...
    speaker: String,
    text: String,
    codes: BTreeMap<usize, usize>,
}

#[derive(Default)]
struct CodeAgg {
    voices: HashSet<(usize, usize)>,
    transcripts: HashSet<usize>,
    segments: usize,
    keyword_voices: BTreeMap<usize, HashSet<(usize, usize)>>,
}

/// The result of analysing text (no files): used by the tool and by the tests.
struct Analysis {
    transcripts: Vec<TranscriptCounts>,
    segments: Vec<Segment>,
    deidentified: Vec<String>,
    tally: ScrubTally,
    facilitator_turns: usize,
    total_words: usize,
    voices: HashSet<(usize, usize)>,
    candidate_counts: HashMap<String, HashSet<(usize, usize)>>,
    aggs: Vec<CodeAgg>,
    names_seen: HashSet<String>,
    keyword_shown: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn analyze(
    texts: &[String],
    codes: &[CodeDef],
    cfg: &QualContent,
    extra_facilitators: &[String],
    names: &HashSet<String>,
    towns: HashSet<String>,
) -> Analysis {
    let non_speaker: HashSet<String> = cfg
        .non_speaker_labels
        .iter()
        .map(|s| words_of(s).join(" "))
        .collect();
    let facilitator_words: HashSet<String> = cfg
        .facilitator_labels
        .iter()
        .chain(extra_facilitators.iter())
        .flat_map(|s| {
            let w = words_of(s);
            // "note taker" is a role of two words; keep it whole and as words.
            let mut v = vec![w.join(" ")];
            v.extend(w);
            v
        })
        .collect();
    let settings = ScrubSettings {
        keep: cfg.keep_capitalized.iter().map(|s| s.to_lowercase()).collect(),
        towns,
    };
    let stop: HashSet<&str> = cfg.stopwords.iter().map(String::as_str).collect();
    let coder = Coder::new(codes);
    let mut a = Analysis {
        transcripts: Vec::new(),
        segments: Vec::new(),
        deidentified: Vec::new(),
        tally: ScrubTally::default(),
        facilitator_turns: 0,
        total_words: 0,
        voices: HashSet::new(),
        candidate_counts: HashMap::new(),
        aggs: (0..codes.len()).map(|_| CodeAgg::default()).collect(),
        names_seen: HashSet::new(),
        keyword_shown: coder.keywords.iter().map(|k| k.shown.clone()).collect(),
    };
    for (t, text) in texts.iter().enumerate() {
        let (turns, labeled) = parse_turns(text, &non_speaker);
        // Voices and known names for this transcript.
        let mut voice_of: HashMap<String, usize> = HashMap::new();
        let mut known: HashSet<String> = names.clone();
        let is_facilitator = |label: &str| -> bool {
            let w = words_of(label);
            let joined = w.join(" ");
            facilitator_words.contains(&joined)
                || w.iter()
                    .filter(|x| !x.chars().all(|c| c.is_ascii_digit()))
                    .any(|x| facilitator_words.contains(x))
        };
        for turn in &turns {
            if let Some(label) = &turn.label {
                for w in words_of(label) {
                    let anonymous = w.chars().any(|c| c.is_ascii_digit())
                        || matches!(
                            w.as_str(),
                            "participant" | "speaker" | "respondent" | "interviewee" | "p" | "r"
                        );
                    if w.chars().count() >= 2 && !anonymous && !facilitator_words.contains(&w) {
                        known.insert(w);
                    }
                }
            }
        }
        a.names_seen.extend(known.iter().cloned());
        let known_re = known_regex(&known);
        // Pass 1: capitalized words in the middle of sentences, across the whole transcript.
        let mut propagated: HashSet<String> = HashSet::new();
        for turn in &turns {
            collect_mid_sentence(&turn.text, &settings, &mut propagated);
        }
        let mut tc = TranscriptCounts {
            id: format!("T{}", t + 1),
            words: 0,
            participants: 0,
            participant_segments: 0,
            coded_segments: 0,
        };
        let mut deid = String::new();
        let mut participants_here: HashSet<usize> = HashSet::new();
        for turn in &turns {
            // Role and pseudonym.
            let (speaker, facilitator, voice): (String, bool, Option<usize>) =
                match (&turn.label, labeled) {
                    (Some(label), true) => {
                        if is_facilitator(label) {
                            ("Facilitator".to_string(), true, None)
                        } else {
                            let key = words_of(label).join(" ");
                            let next = voice_of.len();
                            let v = *voice_of.entry(key).or_insert(next);
                            a.tally.labels += 1;
                            (format!("P{}", v + 1), false, Some(v))
                        }
                    }
                    (None, true) => ("Preamble".to_string(), true, None),
                    (_, false) => {
                        let next = voice_of.len();
                        let v = *voice_of.entry("P1".into()).or_insert(next);
                        (format!("P{}", v + 1), false, Some(v))
                    }
                };
            // Scrub.
            let step1 = scrub_names(&turn.text, &known, &propagated, &settings, &mut a.tally);
            let step2 = scrub_known_any_case(&step1, known_re.as_ref(), &mut a.tally);
            let clean = scrub_patterns(&step2, &mut a.tally);
            let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
            deid.push_str(&format!("{speaker}: {clean}\n\n"));
            let words = clean.split_whitespace().count();
            tc.words += words;
            a.total_words += words;
            if facilitator {
                a.facilitator_turns += 1;
                continue;
            }
            let Some(v) = voice else { continue };
            participants_here.insert(v);
            a.voices.insert((t, v));
            for seg_text in split_segment(&clean) {
                let toks = tokens_of(&seg_text);
                if toks.len() < 3 {
                    continue;
                }
                tc.participant_segments += 1;
                // Candidate terms.
                for w in toks.iter().collect::<HashSet<_>>() {
                    if w.chars().count() >= 4
                        && w.chars().all(|c| c.is_alphabetic())
                        && !stop.contains(w.as_str())
                        && !propagated.contains(w.as_str())
                        && !known.contains(w.as_str())
                        && !phi::is_first_name(w)
                    {
                        a.candidate_counts
                            .entry(w.clone())
                            .or_default()
                            .insert((t, v));
                    }
                }
                let hits = coder.matches(&toks);
                let mut seg_codes: BTreeMap<usize, usize> = BTreeMap::new();
                for ki in hits {
                    let kw = &coder.keywords[ki];
                    seg_codes.entry(kw.code).or_insert(ki);
                    a.aggs[kw.code]
                        .keyword_voices
                        .entry(ki)
                        .or_default()
                        .insert((t, v));
                }
                if !seg_codes.is_empty() {
                    tc.coded_segments += 1;
                }
                for &c in seg_codes.keys() {
                    let agg = &mut a.aggs[c];
                    agg.voices.insert((t, v));
                    agg.transcripts.insert(t);
                    agg.segments += 1;
                }
                a.segments.push(Segment {
                    transcript: t,
                    voice: v,
                    speaker: speaker.clone(),
                    text: seg_text,
                    codes: seg_codes,
                });
            }
        }
        tc.participants = participants_here.len();
        a.transcripts.push(tc);
        a.deidentified.push(deid);
    }
    a
}

// ---------------------------------------------------------------- the tool

fn pluralize(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Analyze focus group and key informant transcripts.
pub fn analyze_qualitative_data(
    ctx: &Context,
    args: QualitativeArgs,
) -> Result<Envelope<QualitativeResult>> {
    let cfg: QualContent = content::load(QUALITATIVE_CODEBOOK)?;
    if args.transcript_paths.is_empty() {
        return Err(PhnError::InvalidInput(
            "give transcript_paths: TXT, MD, DOCX, PDF, or HTML transcript files (or a folder of \
             them) saved in /data"
                .into(),
        ));
    }
    let min_voices = args.min_voices.unwrap_or(cfg.min_voices_floor);
    if min_voices < cfg.min_voices_floor {
        return Err(PhnError::Privacy(format!(
            "min_voices cannot be lower than {}: a theme raised by fewer than {} different \
             participants could point to one person, so its counts are never shown",
            cfg.min_voices_floor, cfg.min_voices_floor
        )));
    }
    let (codes, codebook_source, mut sources) = match &args.codebook_path {
        Some(p) => {
            let (codes, label) = load_codebook(ctx, p, &cfg)?;
            (
                codes,
                "your codebook file".to_string(),
                vec![Source::local_file(label)],
            )
        }
        None => (
            cfg.codes.clone(),
            "the project's starter codebook (a draft, low confidence; replace it with your own)"
                .to_string(),
            Vec::new(),
        ),
    };
    let names = match &args.names_path {
        Some(p) => load_names(ctx, p)?,
        None => HashSet::new(),
    };
    let names_from_file = names.len();
    let transcripts = collect_transcripts(ctx, &args.transcript_paths)?;
    let texts: Vec<String> = transcripts.into_iter().map(|t| t.text).collect();
    let word_estimate: usize = texts.iter().map(|t| t.split_whitespace().count()).sum();
    if word_estimate > MAX_WORDS {
        return Err(PhnError::InvalidInput(format!(
            "the transcripts have about {word_estimate} words; analyze at most {MAX_WORDS} at a time"
        )));
    }
    let towns: HashSet<String> = ctx
        .ma
        .municipalities
        .iter()
        .flat_map(|t| {
            t.name
                .split([' ', '-'])
                .chain(t.county.split(' '))
                .map(base_word)
                .collect::<Vec<_>>()
        })
        .collect();
    let a = analyze(&texts, &codes, &cfg, &args.facilitator_labels, &names, towns);
    if a.segments.is_empty() {
        return Err(PhnError::InvalidInput(
            "no participant text was found to code. Transcripts need speaker turns written as \
             \"Name: what they said\" (or P1:, Speaker 2:) or ordinary paragraphs, and the \
             facilitator's turns are not coded"
                .into(),
        ));
    }

    // Counts and suppression.
    let mut themes: Vec<ThemeCount> = Vec::new();
    let mut withheld = 0usize;
    let mut by_theme: BTreeMap<String, (Vec<usize>, HashSet<(usize, usize)>)> = BTreeMap::new();
    for (ci, code) in codes.iter().enumerate() {
        let agg = &a.aggs[ci];
        let entry = by_theme.entry(code.theme.clone()).or_default();
        entry.0.push(ci);
        entry.1.extend(agg.voices.iter().copied());
        if agg.segments == 0 {
            // A code nobody raised: not a rare theme, simply absent; say 0 participants only
            // when no participant raised it (disclosure-safe).
            themes.push(ThemeCount {
                code: code.code.clone(),
                theme: code.theme.clone(),
                reported: true,
                participants: Some(0),
                transcripts: Some(0),
                segments: Some(0),
                keywords_matched: Vec::new(),
                summary: format!(
                    "No participant's words matched this code's keywords ({}).",
                    code.definition.trim_end_matches('.')
                ),
            });
            continue;
        }
        if (agg.voices.len() as u32) < min_voices {
            withheld += 1;
            continue;
        }
        let mut kws: Vec<(usize, usize)> = agg
            .keyword_voices
            .iter()
            .filter(|(_, v)| v.len() as u32 >= min_voices)
            .map(|(k, v)| (*k, v.len()))
            .collect();
        kws.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
        let total_voices = a.voices.len();
        themes.push(ThemeCount {
            code: code.code.clone(),
            theme: code.theme.clone(),
            reported: true,
            participants: Some(agg.voices.len()),
            transcripts: Some(agg.transcripts.len()),
            segments: Some(agg.segments),
            keywords_matched: kws
                .iter()
                .take(6)
                .map(|(k, _)| a.keyword_shown[*k].clone())
                .collect(),
            summary: format!(
                "{} of {} participants raised this in {} of {} transcripts ({}). {}",
                agg.voices.len(),
                total_voices,
                agg.transcripts.len(),
                a.transcripts.len(),
                pluralize(agg.segments, "coded segment", "coded segments"),
                code.definition
            ),
        });
    }
    // Most-raised first, then by code name.
    themes.sort_by(|x, y| {
        y.participants
            .unwrap_or(0)
            .cmp(&x.participants.unwrap_or(0))
            .then(x.code.cmp(&y.code))
    });
    let theme_groups: Vec<ThemeGroup> = by_theme
        .into_iter()
        .filter(|(_, (cis, v))| cis.iter().any(|&ci| a.aggs[ci].segments > 0) && !v.is_empty())
        .map(|(theme, (cis, v))| {
            let reported = v.len() as u32 >= min_voices;
            ThemeGroup {
                theme,
                codes: if reported {
                    cis.iter().map(|&ci| codes[ci].code.clone()).collect()
                } else {
                    Vec::new()
                },
                reported,
                participants: reported.then_some(v.len()),
            }
        })
        .collect();
    let groups_withheld = theme_groups.iter().filter(|g| !g.reported).count();
    let theme_groups: Vec<ThemeGroup> = theme_groups
        .into_iter()
        .filter(|g| g.reported)
        .map(|mut g| {
            g.codes.sort();
            g
        })
        .collect();
    let _ = groups_withheld;

    // Candidate terms.
    let coded_tokens: HashSet<String> = codes
        .iter()
        .flat_map(|c| c.keywords.iter())
        .flat_map(|k| words_of(k))
        .collect();
    let mut cand: Vec<CandidateTerm> = a
        .candidate_counts
        .iter()
        .filter(|(w, v)| {
            v.len() as u32 >= min_voices
                && !coded_tokens.contains(w.as_str())
                && !coded_tokens.iter().any(|k| w.starts_with(k.as_str()) && k.len() >= 5)
                && !a.names_seen.contains(w.as_str())
        })
        .map(|(w, v)| CandidateTerm {
            term: w.clone(),
            participants: v.len(),
        })
        .collect();
    cand.sort_by(|x, y| y.participants.cmp(&x.participants).then(x.term.cmp(&y.term)));
    cand.truncate(MAX_CANDIDATES);

    // Scrub report.
    let mut removed: Vec<ScrubCount> = a
        .tally
        .by_kind
        .iter()
        .map(|(k, n)| ScrubCount {
            kind: k.clone(),
            removed: *n,
        })
        .collect();
    if a.tally.names > 0 {
        removed.push(ScrubCount {
            kind: "name, organization, or place (capitalized word or name from your list)".into(),
            removed: a.tally.names,
        });
    }
    removed.sort_by(|x, y| y.removed.cmp(&x.removed).then(x.kind.cmp(&y.kind)));
    let total_removed: usize = removed.iter().map(|r| r.removed).sum();
    let scrub = ScrubReport {
        speaker_labels_replaced: a.tally.labels,
        names_from_file,
        total_removed,
        second_pass_removed: a.tally.second_pass,
        removed,
        method: "Speaker labels became P1, P2, ...; names from the labels and your names file were \
                 removed everywhere; capitalized words inside sentences that are not Massachusetts \
                 towns or common labels (names, organizations, places) became [NAME]; phone \
                 numbers, email and street addresses, dates, ZIP codes, record numbers, and titled \
                 names were removed with the server's identifier detector; the result was \
                 scanned again."
            .into(),
    };

    let mut transcripts_out = a.transcripts.clone();
    let participants_total = a.voices.len();
    let participant_segments: usize = transcripts_out.iter().map(|t| t.participant_segments).sum();
    let coded_segments: usize = transcripts_out.iter().map(|t| t.coded_segments).sum();
    // Small transcripts (one or two participants) are not described separately.
    for t in &mut transcripts_out {
        let _ = t;
    }

    // Outputs.
    let mut files: Vec<String> = Vec::new();
    let mut privacy = Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(min_voices),
        ..Privacy::none()
    };
    let mut caveats: Vec<String> = Vec::new();
    if args.write_outputs.unwrap_or(true) {
        match ctx.sandbox() {
            Ok(_) => {
                let paths = &args.transcript_paths;
                let (dir, shown) = templates::output_folder(ctx, "qualitative_analysis")?;
                let matrix = matrix_document(ctx, &a, &codes, &cfg, paths, min_voices);
                files.push(templates::write_in(
                    ctx,
                    &dir,
                    "coded_excerpt_matrix.xlsx",
                    &phn_docs::xlsx::to_xlsx(&matrix)?,
                )?);
                let summary_doc = summary_document(
                    ctx,
                    &a,
                    &codes,
                    &themes,
                    &codebook_source,
                    min_voices,
                    &cfg,
                    &scrub,
                );
                files.push(templates::write_in(
                    ctx,
                    &dir,
                    "theme_summary.docx",
                    &phn_docs::docx::to_docx(&summary_doc)?,
                )?);
                if args.write_deidentified_transcripts.unwrap_or(false) {
                    for (i, d) in a.deidentified.iter().enumerate() {
                        files.push(templates::write_in(
                            ctx,
                            &dir,
                            &format!("deidentified_T{}.txt", i + 1),
                            d.as_bytes(),
                        )?);
                    }
                }
                privacy.files_written = files.clone();
                privacy.notes.push(format!(
                    "De-identified excerpts, the transcript key (which file is T1, T2, ...), and \
                     theme details were written to {shown} only; read them before sharing. Nothing \
                     from the transcripts is in this response except counts."
                ));
            }
            Err(e) => caveats.push(format!("Output files were not written: {e}")),
        }
    }
    privacy.notes.push(
        "Transcripts were read on this machine. This response has counts, the codebook, and \
         theme summaries built from the counts; no sentence, speaker label, or file name from a \
         transcript is returned, and themes raised by fewer than the minimum number of \
         participants are withheld."
            .into(),
    );
    let mut kinds: Vec<String> = a.tally.by_kind.keys().cloned().collect();
    if a.tally.names > 0 {
        kinds.push("name".into());
    }
    kinds.sort();
    kinds.dedup();
    privacy.identifiers_detected = kinds;

    // Caveats.
    caveats.push(
        "Keyword coding is a heuristic: it finds the words in the codebook, not their meaning \
         (it cannot tell a complaint from a compliment or read negation or sarcasm), and a \
         participant's turn gets every code whose keywords appear in it. Treat the counts as a \
         starting point, read the coded excerpts, and recode by hand before reporting a finding."
            .into(),
    );
    caveats.push(
        "De-identification by patterns, name lists, and capitalization is a heuristic and can \
         miss details that point to a person (a unique job, a rare event, a nickname, a place \
         written in lower case). It also over-removes: organization and place names that are not \
         Massachusetts towns became [NAME]. Read the de-identified output before it is shared."
            .into(),
    );
    caveats.push(format!(
        "A theme is shown only when at least {min_voices} different participants raised it; {} \
         {} withheld. Counts are participants across transcripts: a person who spoke in two \
         transcripts counts twice.",
        withheld,
        if withheld == 1 { "theme was" } else { "themes were" }
    ));
    if codebook_source.starts_with("the project's") {
        caveats.push(
            "No codebook was given, so the project's starter codebook was used. It is a draft \
             written for this server (low confidence, not checked against any published \
             codebook): use your own codebook_path for a real analysis."
                .into(),
        );
    }
    if a.facilitator_turns == 0 && texts.len() > 1 {
        caveats.push(
            "No facilitator or interviewer turns were recognized. If the facilitator's questions \
             were coded as participant text, add their label to facilitator_labels."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[QUALITATIVE_CODEBOOK]));

    let result = QualitativeResult {
        transcripts: transcripts_out,
        total_words: a.total_words,
        total_participants: participants_total,
        facilitator_turns_excluded: a.facilitator_turns,
        participant_segments,
        coded_segments,
        scrub,
        codebook: CodebookOut {
            source: codebook_source,
            codes: codes
                .iter()
                .map(|c| CodeOut {
                    code: c.code.clone(),
                    theme: c.theme.clone(),
                    definition: c.definition.clone(),
                    keywords: c.keywords.clone(),
                })
                .collect(),
        },
        min_voices,
        themes,
        theme_groups,
        themes_withheld: withheld,
        candidate_terms: cand,
        files,
        method_note: "Segments are speaker turns (long turns are split at sentence ends). A code \
                      applies to a segment when one of its keywords appears as a whole word \
                      (or a prefix, for keywords ending in *)."
            .into(),
    };
    let summary = summary_text(&result);
    sources.push(Source::computed(
        "Keyword coding of transcripts with name scrubbing and rare-theme suppression",
        "Calculated on this machine from your files",
    ));
    for p in &args.transcript_paths {
        sources.push(Source::local_file(p.clone()));
    }
    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn summary_text(r: &QualitativeResult) -> String {
    let mut s = format!(
        "**Qualitative analysis: {} transcript(s), {} participants (voices), {} words**\n\n",
        r.transcripts.len(),
        r.total_participants,
        md::num(r.total_words as f64, 0)
    );
    s.push_str(&format!(
        "{} participant segments, {} coded; {} facilitator turns left out. Scrubbing removed {} \
         identifiers (names, organizations, places, phone numbers, dates, and similar).\n\n",
        r.participant_segments, r.coded_segments, r.facilitator_turns_excluded, r.scrub.total_removed
    ));
    let rows: Vec<Vec<String>> = r
        .themes
        .iter()
        .filter(|t| t.participants.unwrap_or(0) > 0)
        .map(|t| {
            vec![
                t.code.clone(),
                t.theme.clone(),
                t.participants.map(|n| n.to_string()).unwrap_or_default(),
                t.transcripts.map(|n| n.to_string()).unwrap_or_default(),
                t.segments.map(|n| n.to_string()).unwrap_or_default(),
            ]
        })
        .collect();
    if rows.is_empty() {
        s.push_str("No theme was raised by enough participants to be shown.\n");
    } else {
        s.push_str(&md::table(
            &["Code", "Theme", "Participants", "Transcripts", "Segments"],
            &rows,
        ));
    }
    s.push_str(&format!(
        "\n{} theme(s) withheld because fewer than {} different participants raised them.\n",
        r.themes_withheld, r.min_voices
    ));
    if !r.candidate_terms.is_empty() {
        s.push_str(&format!(
            "\n**Words raised by {} or more participants that no code matched (ideas for new \
             codes):** {}\n",
            r.min_voices,
            r.candidate_terms
                .iter()
                .map(|c| format!("{} ({})", c.term, c.participants))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !r.files.is_empty() {
        s.push_str(&format!(
            "\nWritten to /data/output (de-identified): {}\n",
            r.files.join(", ")
        ));
    }
    s
}

fn doc(ctx: &Context, title: &str, blocks: Vec<Block>) -> Document {
    Document {
        title: title.to_string(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

fn table_block(caption: &str, headers: &[&str], mut rows: Vec<Vec<String>>) -> Block {
    if rows.is_empty() {
        rows.push(vec!["(none)".to_string(); headers.len().max(1)]);
    }
    Block::Table(TableBlock {
        caption: Some(caption.to_string()),
        headers: headers.iter().map(|h| h.to_string()).collect(),
        rows,
    })
}

/// The coded excerpt matrix workbook (de-identified excerpts, key, codebook).
fn matrix_document(
    ctx: &Context,
    a: &Analysis,
    codes: &[CodeDef],
    cfg: &QualContent,
    paths: &[String],
    min_voices: u32,
) -> Document {
    // Rows: one per segment and code.
    let coder = Coder::new(codes);
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (n, seg) in a.segments.iter().enumerate() {
        for (&ci, &ki) in &seg.codes {
            rows.push(vec![
                format!("S{}", n + 1),
                format!("T{}", seg.transcript + 1),
                seg.speaker.clone(),
                codes[ci].code.clone(),
                codes[ci].theme.clone(),
                coder.keywords[ki].shown.clone(),
                excerpt(&seg.text, &coder.keywords[ki], cfg.excerpt_max_chars),
            ]);
        }
    }
    let by_t: Vec<Vec<String>> = codes
        .iter()
        .enumerate()
        .map(|(ci, c)| {
            let mut row = vec![c.code.clone()];
            for t in 0..a.transcripts.len() {
                let n = a
                    .segments
                    .iter()
                    .filter(|s| s.transcript == t && s.codes.contains_key(&ci))
                    .count();
                row.push(n.to_string());
            }
            row
        })
        .collect();
    let mut headers: Vec<String> = vec!["Code".into()];
    headers.extend((0..a.transcripts.len()).map(|t| format!("T{}", t + 1)));
    let header_refs: Vec<&str> = headers.iter().map(String::as_str).collect();
    let key: Vec<Vec<String>> = a
        .transcripts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            vec![
                t.id.clone(),
                if paths.len() == a.transcripts.len() {
                    paths[i].clone()
                } else {
                    format!("file {} of the folder(s) {}", i + 1, paths.join(", "))
                },
                t.participants.to_string(),
                t.words.to_string(),
            ]
        })
        .collect();
    doc(
        ctx,
        "Coded excerpt matrix (de-identified)",
        vec![
            table_block(
                "Coded excerpts",
                &[
                    "Segment",
                    "Transcript",
                    "Participant",
                    "Code",
                    "Theme",
                    "Keyword matched",
                    "Excerpt (de-identified)",
                ],
                rows,
            ),
            table_block("Segments by code and transcript", &header_refs, by_t),
            table_block(
                "Codebook",
                &["Code", "Theme", "Definition", "Keywords"],
                codes
                    .iter()
                    .map(|c| {
                        vec![
                            c.code.clone(),
                            c.theme.clone(),
                            c.definition.clone(),
                            c.keywords.join("; "),
                        ]
                    })
                    .collect(),
            ),
            table_block(
                "Transcript key (local use only)",
                &["Transcript", "File as you gave it", "Participants", "Words"],
                key,
            ),
            table_block(
                "Read me",
                &["Note"],
                vec![
                    vec![format!(
                        "Excerpts are the sentences with a matched keyword, de-identified by this \
                         server (names, organizations, places, phone numbers, dates, and similar \
                         removed). Participants are P1, P2, ... within each transcript. Themes \
                         raised by fewer than {min_voices} participants are suppressed in the \
                         tool's response but their rows are here: do not quote them in anything \
                         shared."
                    )],
                    vec![
                        "Keyword coding is a heuristic. Read each excerpt and recode by hand."
                            .to_string(),
                    ],
                ],
            ),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn summary_document(
    ctx: &Context,
    a: &Analysis,
    codes: &[CodeDef],
    themes: &[ThemeCount],
    codebook_source: &str,
    min_voices: u32,
    cfg: &QualContent,
    scrub: &ScrubReport,
) -> Document {
    let mut md_text = String::new();
    md_text.push_str("# Qualitative analysis: theme summary (de-identified)\n\n");
    md_text.push_str(&format!(
        "{} transcript(s); {} participants; {} words. Codebook: {codebook_source}.\n\n",
        a.transcripts.len(),
        a.voices.len(),
        a.total_words
    ));
    md_text.push_str("## Method\n\n");
    md_text.push_str(&format!("{}\n\n", scrub.method));
    md_text.push_str(
        "Segments are speaker turns. A code applies to a segment when one of its keywords appears. \
         Keyword coding is a heuristic: read the excerpts and recode by hand before reporting a \
         finding.\n\n",
    );
    md_text.push_str(&format!(
        "Themes raised by fewer than {min_voices} different participants are not shown.\n\n"
    ));
    md_text.push_str("## Themes\n\n");
    let coder = Coder::new(codes);
    for t in themes.iter().filter(|t| t.participants.unwrap_or(0) > 0) {
        md_text.push_str(&format!("### {} ({})\n\n{}\n\n", t.code, t.theme, t.summary));
        // Up to three example excerpts, each from a different participant.
        let ci = codes.iter().position(|c| c.code == t.code);
        if let Some(ci) = ci {
            let mut seen: HashSet<(usize, usize)> = HashSet::new();
            let mut shown = 0;
            for seg in &a.segments {
                if let Some(&ki) = seg.codes.get(&ci)
                    && seen.insert((seg.transcript, seg.voice))
                {
                    md_text.push_str(&format!(
                        "- {} (T{}, {}): {}\n",
                        t.code,
                        seg.transcript + 1,
                        seg.speaker,
                        excerpt(&seg.text, &coder.keywords[ki], cfg.excerpt_max_chars)
                    ));
                    shown += 1;
                    if shown == 3 {
                        break;
                    }
                }
            }
            md_text.push('\n');
        }
    }
    md_text.push_str("## Codebook\n\n");
    md_text.push_str(&md::table(
        &["Code", "Theme", "Definition", "Keywords"],
        &codes
            .iter()
            .map(|c| {
                vec![
                    c.code.clone(),
                    c.theme.clone(),
                    c.definition.clone(),
                    c.keywords.join("; "),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    templates::document(ctx, &md_text, "en-US", "Qualitative analysis: theme summary")
}

/// Used by tests: the speaker-turn parser.
#[cfg(test)]
fn turns_for_test(text: &str) -> (Vec<(Option<String>, String)>, bool) {
    let cfg: QualContent = content::load(QUALITATIVE_CODEBOOK).unwrap();
    let ns: HashSet<String> = cfg.non_speaker_labels.iter().map(|s| words_of(s).join(" ")).collect();
    let (t, l) = parse_turns(text, &ns);
    (t.into_iter().map(|x| (x.label, x.text)).collect(), l)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> QualContent {
        content::load(QUALITATIVE_CODEBOOK).unwrap()
    }

    fn run(texts: &[&str], names: &[&str]) -> Analysis {
        let c = cfg();
        let texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let names: HashSet<String> = names.iter().map(|s| s.to_lowercase()).collect();
        analyze(&texts, &c.codes, &c, &[], &names, ["springfield".to_string()].into())
    }

    #[test]
    fn starter_codebook_loads_and_keywords_compile() {
        let c = cfg();
        assert!(c.codes.len() >= 15);
        assert_eq!(c.min_voices_floor, 3);
        let coder = Coder::new(&c.codes);
        for code in &c.codes {
            assert!(
                coder.keywords.iter().any(|k| c.codes[k.code].code == code.code),
                "{} has no usable keyword",
                code.code
            );
        }
    }

    #[test]
    fn parses_labeled_turns_and_continuation_lines() {
        let (turns, labeled) = turns_for_test(
            "Focus group notes\nInterviewer: Tell me about getting care.\nMaria: The bus is\nslow and late.\n\nJames: I agree.\n",
        );
        assert!(labeled);
        assert_eq!(turns[0], (None, "Focus group notes".to_string()));
        assert_eq!(turns[1].0.as_deref(), Some("Interviewer"));
        assert_eq!(turns[2], (Some("Maria".to_string()), "The bus is slow and late.".to_string()));
        assert_eq!(turns[3].0.as_deref(), Some("James"));
        // One "Label:" line is not a transcript structure; sentences with a colon stay text.
        let (turns, labeled) = turns_for_test("Another thing: the bus is late.\n\nSecond paragraph here.");
        assert!(!labeled);
        assert_eq!(turns.len(), 2);
        // Timestamps before labels are ignored.
        let (turns, _) = turns_for_test("[00:01:02] Rosa: hello there friend\n[00:01:09] Ana: hi\n");
        assert_eq!(turns[0].0.as_deref(), Some("Rosa"));
        // Lines like "Note:" are not speakers.
        let (turns, _) = turns_for_test("Note: recorded on tape\nRosa: hi\nAna: hello\n");
        assert_eq!(turns.iter().filter(|t| t.0.as_deref() == Some("Note")).count(), 0);
    }

    #[test]
    fn names_are_removed_from_text_labels_and_the_names_list() {
        let a = run(
            &[
                "Facilitator: What helps your family?\n\
                 Rosa: My neighbor Ana Garcia drove me to the clinic. Rosa is what everyone calls me. Call 413-555-0188 or write rosa.test@example.org.\n\
                 Priya: Thanks to Mr. Okafor at Lowell General we could afford the bus. Dr. Nguyen helped too.\n\
                 Rosa: Rosa said the rent is too high on 12 Elm Street, MA 01101.",
            ],
            &["Okafor"],
        );
        let all = a.deidentified.join("\n");
        for leaked in [
            "Rosa", "Priya", "Garcia", "Ana", "Okafor", "Nguyen", "413-555-0188", "example.org",
            "Elm Street", "01101", "rosa.test",
        ] {
            assert!(!all.contains(leaked), "{leaked} leaked: {all}");
        }
        assert!(all.contains("P1:") && all.contains("P2:"));
        assert!(all.contains("Facilitator:"));
        assert_eq!(a.voices.len(), 2);
        assert!(a.tally.names > 5);
        // Facilitator's turn is not coded.
        assert_eq!(a.facilitator_turns, 1);
        // Ordinary words survive.
        assert!(all.contains("rent is too high"));
        // The town survives.
    }

    #[test]
    fn towns_months_and_languages_are_kept() {
        let a = run(
            &["P1: I live in Springfield and speak Spanish at home. We moved in March.\nP2: Yes, in Springfield too."],
            &[],
        );
        let all = a.deidentified.join("\n");
        assert!(all.contains("Springfield") && all.contains("Spanish") && all.contains("March"), "{all}");
    }

    #[test]
    fn mid_sentence_names_are_propagated_to_sentence_starts() {
        let a = run(
            &["P1: I asked my friend Zoltan about it.\nP2: Zoltan told me something else.\nP1: Zoltan is kind."],
            &[],
        );
        let all = a.deidentified.join("\n");
        assert!(!all.contains("Zoltan"), "{all}");
    }

    #[test]
    fn keyword_coding_matches_whole_words_prefixes_and_phrases() {
        let c = cfg();
        let coder = Coder::new(&c.codes);
        let code_of = |text: &str| -> BTreeSet<String> {
            coder
                .matches(&tokens_of(text))
                .into_iter()
                .map(|k| c.codes[coder.keywords[k].code].code.clone())
                .collect()
        };
        assert!(code_of("There is no transportation to the clinic").contains("TRANSPORT"));
        assert!(code_of("There is no transportation to the clinic").contains("ACCESS_CARE"));
        // Whole words only: "carpet" is not "car"-anything and "buses" is listed.
        assert!(!code_of("The carpet was ruined").contains("TRANSPORT"));
        assert!(code_of("The buses never come").contains("TRANSPORT"));
        // Phrase and apostrophe handling (curly apostrophe).
        assert!(code_of("I can’t pay the bills").contains("COST"));
        assert!(code_of("It is hard to find a primary care doctor").contains("ACCESS_CARE"));
        // No match.
        assert!(code_of("The weather was nice yesterday").is_empty());
    }

    #[test]
    fn counts_distinct_participants_not_segments() {
        // P1 talks about the bus three times; P2 and P3 once each.
        let a = run(
            &[
                "P1: The bus is late.\nP1: The bus never comes on weekends.\nP1: I take the bus to work.\n\
                 P2: There is no bus near us.\nP3: The bus costs too much money anyway.",
            ],
            &[],
        );
        let c = cfg();
        let ci = c.codes.iter().position(|x| x.code == "TRANSPORT").unwrap();
        assert_eq!(a.aggs[ci].voices.len(), 3);
        assert_eq!(a.aggs[ci].segments, 5);
        assert_eq!(a.aggs[ci].transcripts.len(), 1);
    }

    #[test]
    fn long_turns_are_split_at_sentence_ends_by_characters() {
        let sentence = "Esta es una oración larga sobre el autobús y la clínica. ";
        let text = sentence.repeat(40);
        let parts = split_segment(&text);
        assert!(parts.len() > 1);
        for p in &parts {
            assert!(p.chars().count() <= SEGMENT_TARGET + 80, "{}", p.chars().count());
            assert!(p.ends_with('.'));
        }
        assert_eq!(shorten("héllo wörld ünicode", 8), "héllo…");
        assert_eq!(shorten("short", 10), "short");
    }

    #[test]
    fn reading_files_enforces_guardrails() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("group.txt"), "Interviewer: What is hard?\nA: Rent.\nB: Rent and the bus.\nC: The bus and food costs.\n").unwrap();
        std::fs::write(data.join("table.csv"), "a,b\n1,2\n").unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let call = |args: QualitativeArgs| analyze_qualitative_data(&ctx, args);
        let e = call(QualitativeArgs::default()).unwrap_err().to_string();
        assert!(e.contains("transcript_paths"), "{e}");
        let e = call(QualitativeArgs {
            transcript_paths: vec!["table.csv".into()],
            ..Default::default()
        })
        .unwrap_err()
        .to_string();
        assert!(e.contains("not a document"), "{e}");
        let e = call(QualitativeArgs {
            transcript_paths: vec!["group.txt".into()],
            min_voices: Some(2),
            ..Default::default()
        })
        .unwrap_err()
        .to_string();
        assert!(e.contains("cannot be lower than 3"), "{e}");
        let e = call(QualitativeArgs {
            transcript_paths: vec!["group.txt".into()],
            codebook_path: Some("table.csv".into()),
            ..Default::default()
        })
        .unwrap_err()
        .to_string();
        assert!(e.contains("not a codebook"), "{e}");
        let e = call(QualitativeArgs {
            transcript_paths: vec!["missing.txt".into()],
            ..Default::default()
        })
        .unwrap_err()
        .to_string();
        assert!(e.contains("not found"), "{e}");
    }

    #[test]
    fn rare_themes_are_withheld_and_files_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("fg1.txt"),
            "Interviewer: What makes it hard to stay healthy?\n\
             Marisol: The rent is too high and the landlord ignores the mold.\n\
             Chandra: Rent. And I cannot afford the bus pass.\n\
             Dmitri: Rent eats everything. Also the opioid problem on our street scares me.\n\
             Marisol: Housing is the main thing for me.\n",
        )
        .unwrap();
        std::fs::write(data.join("names.txt"), "Marisol Quintero\nChandra Patel\nDmitri Volkov\n").unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = analyze_qualitative_data(
            &ctx,
            QualitativeArgs {
                transcript_paths: vec!["fg1.txt".into()],
                names_path: Some("names.txt".into()),
                write_deidentified_transcripts: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let r = &env.data;
        let theme = |code: &str| r.themes.iter().find(|t| t.code == code);
        // Housing: all three participants (rent, landlord, mold, housing).
        let h = theme("HOUSING").expect("HOUSING reported");
        assert_eq!(h.participants, Some(3));
        assert_eq!(h.segments, Some(4));
        // Substance use: one participant only, so it is withheld entirely.
        assert!(theme("SUBSTANCE_USE").is_none());
        assert!(r.themes_withheld >= 1);
        // Cost: Chandra said afford; Dmitri's "eats everything" has no keyword; 1 voice: withheld.
        assert!(theme("COST").is_none());
        // Nothing from the transcript is in the response.
        let text = serde_json::to_string(&env).unwrap() + &env.to_markdown();
        for leaked in ["Marisol", "Chandra", "Dmitri", "Quintero", "Patel", "Volkov", "landlord ignores"] {
            assert!(!text.contains(leaked), "{leaked} in the response");
        }
        assert!(!text.contains("fg1.txt") || env.sources.iter().any(|s| s.title == "fg1.txt"));
        // Files exist and the de-identified transcript has no names.
        assert_eq!(r.files.len(), 3, "{:?}", r.files);
        let deid = std::fs::read_to_string(data.join(&r.files[2])).unwrap();
        for leaked in ["Marisol", "Chandra", "Dmitri", "Quintero"] {
            assert!(!deid.contains(leaked), "{leaked} in {deid}");
        }
        assert!(deid.contains("P1:") && deid.contains("P3:"));
        assert!(env.privacy.suppression_applied);
        assert!(!env.privacy.row_level_data);
        assert_eq!(r.total_participants, 3);
        assert_eq!(r.facilitator_turns_excluded, 1);
    }

    #[test]
    fn your_own_codebook_replaces_the_starter() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("kii.txt"),
            "Q: What do you see?\nA: Parking at the clinic is a problem. Parking and the stairs.\n",
        )
        .unwrap();
        std::fs::write(data.join("book.csv"), "code,definition,keywords,theme\nPARKING,Parking problems,parking; stairs,Built environment\n").unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = analyze_qualitative_data(
            &ctx,
            QualitativeArgs {
                transcript_paths: vec!["kii.txt".into()],
                codebook_path: Some("book.csv".into()),
                write_outputs: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.codebook.codes.len(), 1);
        assert_eq!(env.data.codebook.codes[0].code, "PARKING");
        // One participant: the theme is present but below three, so withheld.
        assert!(env.data.themes.iter().all(|t| t.code != "PARKING" || t.participants == Some(0)));
        assert_eq!(env.data.themes_withheld, 1);
        assert!(env.data.files.is_empty());
    }
}
