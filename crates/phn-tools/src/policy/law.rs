//! POL-02 `search_public_health_law`: searches a consolidated corpus of Massachusetts public health
//! law (M.G.L. chapter 111, 105 CMR, 244 CMR) and, optionally, the board of health regulations in
//! the nurse's local folder, and answers with citations (section number, link, dates) and short
//! excerpts (25 words or fewer). It never returns long text and never says what the law requires.
//!
//! The bundled corpus is a section index for everything and full text for part of it; every hit
//! says which. An optional larger corpus in `law_corpus/` (data or cache folder) is used when its
//! checksums match. Local regulation files go through the Phase 1 policy index
//! (`phn_local::policy_index`); files that look like client records are refused.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::law::{self, Chapter, Corpus, LoadedCorpus, Provision};
use phn_local::phi::{Identifier, RedactionLevel, redact_text, scan_text};
use phn_local::policy_index::PolicyIndex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content;
use crate::md;
use crate::policy::scope::{SCOPE, ScopeContent};
use crate::policy::shared::{clean_text, contains_phrase, refuse_identifiers, stem_words};

/// Content id of `content/policy/public_law_search.yaml`.
pub const LAW: &str = "policy-public-law-search";

const MAX_QUERY: usize = 300;
const DEFAULT_RESULTS: usize = 8;
const MAX_RESULTS: usize = 25;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusInfo {
    pub label: String,
    pub snapshot_id: String,
    pub coverage: String,
    pub not_included: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalCfg {
    pub default_folder: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtendedCfg {
    pub folders: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicDef {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
    pub cites: Vec<String>,
    pub related_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferralDef {
    pub text: String,
    pub contacts: Vec<String>,
}

/// `content/policy/public_law_search.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LawContent {
    pub meta: ContentMeta,
    pub corpus: BTreeMap<String, CorpusInfo>,
    pub local: LocalCfg,
    pub extended: ExtendedCfg,
    pub excerpt_words: usize,
    pub topics: Vec<TopicDef>,
    pub referral: ReferralDef,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- arguments and results

/// A source to search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LawSource {
    /// M.G.L. chapter 111 (Public Health).
    #[serde(rename = "mgl_c111")]
    MglC111,
    /// 105 CMR (Department of Public Health regulations).
    #[serde(rename = "cmr_105")]
    Cmr105,
    /// 244 CMR (Board of Registration in Nursing regulations).
    #[serde(rename = "cmr_244")]
    Cmr244,
    /// Your board of health's regulations in your local folder.
    Local,
}

impl LawSource {
    fn key(self) -> &'static str {
        match self {
            LawSource::MglC111 => "mgl_c111",
            LawSource::Cmr105 => "cmr_105",
            LawSource::Cmr244 => "cmr_244",
            LawSource::Local => "local",
        }
    }
}

/// Arguments for `search_public_health_law`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct LawSearchArgs {
    /// What to look for: a citation ("105 CMR 300.200", "M.G.L. c. 111, § 95", "244 CMR 3.05",
    /// "105 CMR 410") or words ("board of health order to quarantine", "lead paint abatement
    /// duty of owner", "delegation to unlicensed personnel"). Describe the legal question in
    /// general terms; no names of people.
    pub query: String,
    /// Sources to search (default: all four): mgl_c111, cmr_105, cmr_244, local.
    pub sources: Vec<LawSource>,
    /// Maximum hits (default 8, at most 25).
    pub max_results: Option<u32>,
    /// Folder under /data that holds your board of health regulations (default: the folder
    /// named in the content, `local_regulations`, if it exists). Documents only (PDF, Word,
    /// Markdown, text, HTML); files that look like client records are refused.
    pub local_folder: Option<String>,
    /// Return only sections whose text is bundled (skip index-only sections). Default false.
    pub full_text_only: Option<bool>,
}

/// One hit with its citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LawHit {
    pub rank: u32,
    /// mgl_c111, cmr_105, cmr_244, or local.
    pub source: String,
    pub source_label: String,
    /// For example "105 CMR 300.200" or "M.G.L. c. 111, § 95"; for a local file, the file and section.
    pub citation: String,
    pub title: String,
    /// Link to the section (statutes) or the chapter page on mass.gov (regulations).
    pub url: Option<String>,
    /// Link to the chapter PDF (regulations).
    pub pdf_url: Option<String>,
    pub chapter_title: Option<String>,
    /// Date shown on the chapter's page (regulations), when there is one.
    pub chapter_date: Option<String>,
    /// Date the bundled text was retrieved.
    pub retrieved: String,
    /// "full_text" (text is bundled), "index_only" (title and link only), or "local_file".
    pub coverage: String,
    /// A short quotation (25 words or fewer), when the text is available.
    pub excerpt: Option<String>,
    pub matched_terms: Vec<String>,
    /// "citation" or "keywords".
    pub match_kind: String,
    /// Pointers to related provisions or tools (for example the scope of practice guide for 244 CMR).
    pub related: Vec<String>,
}

/// A topic starting point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TopicOut {
    pub id: String,
    pub label: String,
    /// Citations to start with, each with its title.
    pub start_with: Vec<String>,
    pub related_tools: Vec<String>,
}

/// What a source covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageOut {
    pub source: String,
    pub label: String,
    /// "bundled", "cache snapshot override", or "extended".
    pub origin: String,
    pub retrieved: String,
    pub chapters: usize,
    pub sections: usize,
    pub full_text_sections: usize,
    pub index_only_sections: usize,
    pub coverage: String,
    pub not_included: Vec<String>,
}

/// What happened with the local folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocalOut {
    pub searched: bool,
    pub folder: Option<String>,
    pub files_indexed: usize,
    pub sections_indexed: usize,
    /// Files left out because they look like client records (never named).
    pub refused_client_like_files: usize,
    /// Spreadsheets and other tables in the folder; they are not searched.
    pub tables_ignored: usize,
    pub note: String,
}

/// Result of `search_public_health_law`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LawResult {
    pub query: String,
    /// "citation lookup", "chapter lookup", or "keyword search".
    pub interpretation: String,
    pub hits: Vec<LawHit>,
    /// Matches beyond max_results.
    pub more_hits: usize,
    /// Topic starting points that match the words of the query.
    pub topics: Vec<TopicOut>,
    pub local: LocalOut,
    pub coverage: Vec<CoverageOut>,
    pub refer_to: ReferralOut,
    pub limits: Vec<String>,
}

/// Where to take the question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReferralOut {
    pub text: String,
    pub contacts: Vec<String>,
}

// ---------------------------------------------------------------- text helpers

fn flatten(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Words too common to search on.
const STOP: &[&str] = &[
    "a",
    "an",
    "the",
    "of",
    "to",
    "in",
    "on",
    "for",
    "and",
    "or",
    "is",
    "are",
    "be",
    "by",
    "with",
    "can",
    "may",
    "must",
    "does",
    "do",
    "what",
    "which",
    "who",
    "when",
    "how",
    "if",
    "any",
    "that",
    "this",
    "under",
    "from",
    "about",
    "there",
    "their",
    "it",
    "as",
    "at",
    "shall",
    "law",
    "rule",
    "regulation",
    "section",
    "state",
    "massachusetts",
    "cmr",
    "mgl",
    "gl",
];

/// Lower-case alphanumeric tokens with a trailing plural "s" removed.
fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            let w = std::mem::take(cur);
            let w = if w.chars().count() > 3 && w.ends_with('s') && !w.ends_with("ss") {
                w[..w.len() - 1].to_string()
            } else {
                w
            };
            out.push(w);
        }
    };
    for c in s.chars() {
        if c.is_alphanumeric() {
            for l in c.to_lowercase() {
                cur.push(l);
            }
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn query_terms(q: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    tokens(q)
        .into_iter()
        .filter(|t| {
            !STOP.contains(&t.as_str())
                && (t.chars().count() > 1 || t.chars().all(|c| c.is_ascii_digit()))
        })
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

/// An excerpt of at most `max_words` words from `text`, centered on the densest cluster of
/// `terms`. Truncation is marked with an ellipsis.
pub fn excerpt(text: &str, terms: &[String], max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        return words.join(" ");
    }
    let hit: Vec<bool> = words
        .iter()
        .map(|w| {
            let t = tokens(w);
            t.iter().any(|x| terms.contains(x))
        })
        .collect();
    // Window start with the most distinct-ish hits (count hits), earliest on ties.
    let mut best = 0usize;
    let mut best_n = 0usize;
    let mut n: usize = hit[..max_words].iter().filter(|h| **h).count();
    if n > 0 {
        best_n = n;
    }
    for start in 1..=(words.len() - max_words) {
        if hit[start - 1] {
            n -= 1;
        }
        if hit[start + max_words - 1] {
            n += 1;
        }
        if n > best_n {
            best_n = n;
            best = start;
        }
    }
    // Back the window up so the first matched word is a few words in, not at the edge.
    if best_n > 0
        && best > 0
        && let Some(first) = (best..best + max_words).find(|&i| hit[i])
    {
        let lead = first - best;
        if lead > 4 {
            let shift = (lead - 3).min(words.len() - max_words - best);
            best += shift;
        }
    }
    let end = best + max_words;
    let mut s = words[best..end].join(" ");
    if best > 0 {
        s = format!("…{s}");
    }
    if end < words.len() {
        s.push('…');
    }
    s
}

// ---------------------------------------------------------------- citation parsing

/// A citation found in the query.
#[derive(Debug, Clone, PartialEq)]
pub enum Cite {
    /// 105 or 244 CMR: agency, chapter number, optional section number as printed ("300.200").
    Cmr {
        agency: u32,
        chapter: u32,
        section: Option<String>,
    },
    /// M.G.L. c. 111 section.
    Mgl111 { section: String },
}

fn section_key(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Find a citation in a query.
pub fn parse_citation(q: &str) -> Option<Cite> {
    use regex::Regex;
    let cmr = Regex::new(r"(?i)\b(105|244)\s*cmr\s*(\d{1,3})(?:\s*\.\s*(\d{1,3}))?").ok()?;
    if let Some(c) = cmr.captures(q) {
        let agency: u32 = c[1].parse().ok()?;
        let chapter: u32 = c[2].parse().ok()?;
        let section = c.get(3).and_then(|frac| {
            let f = frac.as_str();
            // 300.000 and 300.00 name the chapter, not a section.
            if f.chars().all(|x| x == '0') {
                None
            } else {
                Some(format!("{chapter}.{f}"))
            }
        });
        return Some(Cite::Cmr {
            agency,
            chapter,
            section,
        });
    }
    let mgl = Regex::new(
        r"(?i)(?:\bm\.?\s*g\.?\s*l\.?|\bmgl\b|\bc\.|\bch\.|\bchapter)\s*111\b[\s,]*(?:§+|sections?\b|sec\.?|s\.)?\s*(\d+[a-z]?(?:\s?\d/\d)?)\b",
    )
    .ok()?;
    if let Some(c) = mgl.captures(q) {
        return Some(Cite::Mgl111 {
            section: c[1].to_string(),
        });
    }
    None
}

// ---------------------------------------------------------------- corpus access

struct Loaded {
    source: &'static str,
    label: String,
    origin: String,
    corpus: Corpus,
    entry: Option<phn_data::snapshots::SnapshotEntry>,
}

const ALL_SOURCES: [(&str, LawSource); 3] = [
    ("mgl_c111", LawSource::MglC111),
    ("cmr_105", LawSource::Cmr105),
    ("cmr_244", LawSource::Cmr244),
];

fn load_corpora(ctx: &Context, c: &LawContent, notes: &mut Vec<String>) -> Result<Vec<Loaded>> {
    let bundled: Vec<LoadedCorpus> = law::load_bundled(&ctx.snapshots)?;
    let roots = vec![
        ctx.config.paths.data_dir.clone(),
        ctx.config.paths.cache_dir.clone(),
    ];
    let ext = law::load_extended(&roots);
    notes.extend(ext.notes.clone());
    let mut out = Vec::new();
    for lc in bundled {
        let key = ALL_SOURCES
            .iter()
            .map(|(k, _)| *k)
            .find(|k| *k == lc.corpus.source_id)
            .ok_or_else(|| PhnError::Internal("unknown law source".into()))?;
        let label = c
            .corpus
            .get(key)
            .map(|i| i.label.clone())
            .unwrap_or_else(|| lc.corpus.title.clone());
        let mut origin = lc.origin.clone();
        let mut corpus = lc.corpus.clone();
        if let Some(e) = ext.corpora.iter().find(|e| e.source_id == key) {
            corpus = law::merge(&corpus, e);
            origin = "extended".into();
        }
        out.push(Loaded {
            source: key,
            label,
            origin,
            corpus,
            entry: lc.entry,
        });
    }
    Ok(out)
}

fn chapter_of<'a>(corpus: &'a Corpus, p: &Provision) -> Option<&'a Chapter> {
    corpus.chapters.iter().find(|c| c.ch == p.ch)
}

fn hit_from(
    l: &Loaded,
    p: &Provision,
    terms: &[String],
    kind: &str,
    max_words: usize,
    scope: Option<&ScopeContent>,
) -> LawHit {
    let ch = chapter_of(&l.corpus, p);
    let url = p.u.clone().or_else(|| ch.and_then(|c| c.url.clone()));
    let (coverage, ex) = match &p.x {
        Some(t) if !t.trim().is_empty() => ("full_text", Some(excerpt(t, terms, max_words))),
        _ => ("index_only", None),
    };
    let mut related = Vec::new();
    if l.source == "cmr_244"
        && let Some(s) = scope
    {
        let n = s
            .entries
            .iter()
            .filter(|e| e.citation == p.c || e.citation.starts_with(&format!("{}(", p.c)))
            .count();
        if n > 0 {
            related.push(format!(
                "nursing_scope_of_practice_guide (POL-06) summarizes {n} provision(s) of {} with Board advisory rulings",
                p.c
            ));
        }
    }
    LawHit {
        rank: 0,
        source: l.source.into(),
        source_label: l.label.clone(),
        citation: p.c.clone(),
        title: p.t.clone(),
        url,
        pdf_url: ch.and_then(|c| c.pdf_url.clone()),
        chapter_title: ch.map(|c| c.title.clone()),
        chapter_date: ch.and_then(|c| c.date.clone()),
        retrieved: l.corpus.retrieved.clone(),
        coverage: coverage.into(),
        excerpt: ex,
        matched_terms: terms.to_vec(),
        match_kind: kind.into(),
        related,
    }
}

// ---------------------------------------------------------------- search

struct Scored {
    score: f64,
    li: usize,
    pi: usize,
    matched: Vec<String>,
}

fn keyword_search(
    loaded: &[Loaded],
    selected: &BTreeSet<&str>,
    terms: &[String],
    full_only: bool,
) -> Vec<Scored> {
    if terms.is_empty() {
        return Vec::new();
    }
    let termset: HashSet<&str> = terms.iter().map(String::as_str).collect();
    struct Row {
        li: usize,
        pi: usize,
        title_hits: HashSet<usize>,
        text_tf: HashMap<usize, u32>,
        chapter_hits: HashSet<usize>,
        phrase: f64,
    }
    let idx_of: HashMap<&str, usize> = terms
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_str(), i))
        .collect();
    let mut rows: Vec<Row> = Vec::new();
    let mut total = 0usize;
    for (li, l) in loaded.iter().enumerate() {
        if !selected.contains(l.source) {
            continue;
        }
        let chapter_tokens: HashMap<&str, HashSet<usize>> = l
            .corpus
            .chapters
            .iter()
            .map(|c| {
                let hits = tokens(&c.title)
                    .iter()
                    .filter_map(|t| idx_of.get(t.as_str()).copied())
                    .collect();
                (c.ch.as_str(), hits)
            })
            .collect();
        for (pi, p) in l.corpus.provisions.iter().enumerate() {
            if full_only && p.x.is_none() {
                continue;
            }
            total += 1;
            let tt = tokens(&p.t);
            let title_hits: HashSet<usize> = tt
                .iter()
                .filter_map(|t| idx_of.get(t.as_str()).copied())
                .collect();
            let mut text_tf: HashMap<usize, u32> = HashMap::new();
            let mut phrase = 0.0;
            if let Some(x) = &p.x {
                let xt = tokens(x);
                for t in &xt {
                    if termset.contains(t.as_str()) {
                        *text_tf.entry(idx_of[t.as_str()]).or_default() += 1;
                    }
                }
                if terms.len() >= 2 && xt.windows(terms.len()).any(|w| w == terms) {
                    phrase += 5.0;
                }
            }
            if terms.len() >= 2 && tt.windows(terms.len()).any(|w| w == terms) {
                phrase += 8.0;
            }
            let chapter_hits = if p.x.is_none() {
                chapter_tokens
                    .get(p.ch.as_str())
                    .cloned()
                    .unwrap_or_default()
            } else {
                HashSet::new()
            };
            if title_hits.is_empty() && text_tf.is_empty() && chapter_hits.is_empty() {
                continue;
            }
            rows.push(Row {
                li,
                pi,
                title_hits,
                text_tf,
                chapter_hits,
                phrase,
            });
        }
    }
    // Document frequency of each term over the rows (a term must appear somewhere to count).
    let mut df = vec![0usize; terms.len()];
    for r in &rows {
        for i in 0..terms.len() {
            if r.title_hits.contains(&i)
                || r.text_tf.contains_key(&i)
                || r.chapter_hits.contains(&i)
            {
                df[i] += 1;
            }
        }
    }
    let known: Vec<usize> = (0..terms.len()).filter(|i| df[*i] > 0).collect();
    if known.is_empty() {
        return Vec::new();
    }
    let need = known.len().div_ceil(2).max(1);
    let n = total.max(1) as f64;
    let mut out = Vec::new();
    for r in rows {
        let mut score = r.phrase;
        let mut matched = Vec::new();
        for &i in &known {
            let idf = (1.0 + n / (df[i] as f64 + 0.5)).ln();
            let mut w = 0.0;
            if r.title_hits.contains(&i) {
                w += 3.0;
            }
            if let Some(tf) = r.text_tf.get(&i) {
                w += 1.0 + f64::from(*tf).ln();
            }
            if r.chapter_hits.contains(&i) && w == 0.0 {
                w += 0.6;
            }
            if w > 0.0 {
                matched.push(terms[i].clone());
                score += idf * w;
            }
        }
        if matched.len() >= need {
            out.push(Scored {
                score,
                li: r.li,
                pi: r.pi,
                matched,
            });
        }
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.li.cmp(&b.li))
            .then(a.pi.cmp(&b.pi))
    });
    out
}

// ---------------------------------------------------------------- local folder

/// Replace phone numbers, e-mail addresses, street addresses, and the identifiers the policy
/// index redacts with a marker.
fn redact_contacts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for f in scan_text(text) {
        let hide = matches!(
            f.kind,
            Identifier::Phone
                | Identifier::Email
                | Identifier::StreetAddress
                | Identifier::Ssn
                | Identifier::BirthDate
                | Identifier::MedicalRecordNumber
                | Identifier::HealthPlanNumber
        );
        if hide && f.start >= pos && f.end <= text.len() {
            out.push_str(&text[pos..f.start]);
            out.push_str("[REDACTED]");
            pos = f.end;
        }
    }
    out.push_str(&text[pos..]);
    out
}

/// Relative paths (to the folder) of documents that look like client records: a date of birth,
/// record number, or health plan number anywhere in the file.
fn client_like_files(folder: &std::path::Path) -> (BTreeSet<String>, usize) {
    let mut refused = BTreeSet::new();
    let mut tables = 0usize;
    for e in walkdir_files(folder) {
        let kind = phn_local::extract::FileKind::of(&e);
        if kind.is_tabular() {
            tables += 1;
            continue;
        }
        let Ok(ex) = phn_local::extract::extract(&e) else {
            continue;
        };
        let all = ex
            .sections
            .iter()
            .flat_map(|s| [s.heading.as_str(), s.text.as_str()])
            .collect::<Vec<_>>()
            .join("\n");
        let client = redact_text(&all, RedactionLevel::Standard)
            .redacted
            .keys()
            .any(|k| {
                matches!(
                    k,
                    Identifier::BirthDate
                        | Identifier::MedicalRecordNumber
                        | Identifier::HealthPlanNumber
                )
            });
        if client {
            let rel = e
                .strip_prefix(folder)
                .unwrap_or(&e)
                .to_string_lossy()
                .replace('\\', "/");
            refused.insert(rel);
        }
    }
    (refused, tables)
}

fn walkdir_files(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    let mut depth_guard = 0usize;
    while let Some(d) = stack.pop() {
        depth_guard += 1;
        if depth_guard > 2000 {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                out.push(e.path());
            }
        }
    }
    out.sort();
    out
}

struct LocalHits {
    out: LocalOut,
    hits: Vec<LawHit>,
}

#[allow(clippy::too_many_arguments)]
fn search_local(
    ctx: &Context,
    folder_arg: &Option<String>,
    cfg: &LocalCfg,
    query: &str,
    terms: &[String],
    max: usize,
    max_words: usize,
    explicit: bool,
) -> Result<LocalHits> {
    let sandbox = ctx.sandbox()?;
    let (path, label) = match folder_arg.as_deref().map(str::trim) {
        Some(f) if !f.is_empty() => {
            let p = sandbox.resolve_existing(f)?;
            if !p.is_dir() {
                return Err(PhnError::InvalidInput(
                    "local_folder must be a folder of documents, not a file. Put your board of \
                     health regulations (PDF, Word, Markdown, or text) in a folder under your \
                     data folder."
                        .into(),
                ));
            }
            let l = sandbox.display_path(&p);
            (p, l)
        }
        _ => {
            let p = ctx.config.paths.data_dir.join(&cfg.default_folder);
            if !p.is_dir() {
                let note = if explicit {
                    format!(
                        "No local regulations folder was found. Create '{}' in your data folder and copy your board of health's regulations into it, or pass local_folder.",
                        cfg.default_folder
                    )
                } else {
                    format!(
                        "No '{}' folder in your data folder, so only the state law was searched.",
                        cfg.default_folder
                    )
                };
                return Ok(LocalHits {
                    out: LocalOut {
                        searched: false,
                        folder: None,
                        files_indexed: 0,
                        sections_indexed: 0,
                        refused_client_like_files: 0,
                        tables_ignored: 0,
                        note,
                    },
                    hits: Vec::new(),
                });
            }
            let l = sandbox.display_path(&p);
            (p, l)
        }
    };
    let (refused, tables) = client_like_files(&path);
    let key: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let index_dir = ctx
        .config
        .paths
        .cache_dir
        .join("index")
        .join(format!("law-{key}"));
    let writable = std::fs::create_dir_all(&index_dir).is_ok();
    let index = PolicyIndex::open_or_build(&path, writable.then_some(index_dir.as_path()))?;
    let raw = index.search(query, (max * 3).clamp(5, 50))?;
    let mut hits = Vec::new();
    for h in raw {
        if refused.contains(&h.file) {
            continue;
        }
        let snippet = redact_contacts(&h.snippet);
        let ex = excerpt(&snippet.replace("**", ""), terms, max_words);
        hits.push(LawHit {
            rank: 0,
            source: "local".into(),
            source_label: "Your local regulations".into(),
            citation: redact_contacts(&h.citation),
            title: redact_contacts(&h.section),
            url: None,
            pdf_url: None,
            chapter_title: None,
            chapter_date: None,
            retrieved: String::new(),
            coverage: "local_file".into(),
            excerpt: Some(ex),
            matched_terms: terms.to_vec(),
            match_kind: "keywords".into(),
            related: vec![format!("File: {}", redact_contacts(&h.file))],
        });
    }
    let mut note = format!(
        "{} document(s) searched in your container from {}.",
        index.stats.files_indexed.saturating_sub(refused.len()),
        label
    );
    if !refused.is_empty() {
        note.push_str(&format!(
            " {} file(s) that look like client records were refused and not searched.",
            refused.len()
        ));
    }
    Ok(LocalHits {
        out: LocalOut {
            searched: true,
            folder: Some(label),
            files_indexed: index.stats.files_indexed.saturating_sub(refused.len()),
            sections_indexed: index.stats.sections_indexed,
            refused_client_like_files: refused.len(),
            tables_ignored: tables,
            note,
        },
        hits,
    })
}

// ---------------------------------------------------------------- the tool

/// Run the search.
pub fn search_public_health_law(ctx: &Context, args: LawSearchArgs) -> Result<Envelope<LawResult>> {
    let c: LawContent = content::load(LAW)?;
    let query = clean_text("query", &args.query, MAX_QUERY)?;
    if query.is_empty() {
        return Err(PhnError::InvalidInput(
            "query must not be empty. Give a citation (105 CMR 300.200, M.G.L. c. 111, § 95) or words (board of health quarantine order)."
                .into(),
        ));
    }
    refuse_identifiers("query", &query)?;
    let max = args
        .max_results
        .map(|m| (m as usize).clamp(1, MAX_RESULTS))
        .unwrap_or(DEFAULT_RESULTS);
    let max_words = c.excerpt_words.min(25);
    let explicit_sources = !args.sources.is_empty();
    let selected: BTreeSet<&str> = if explicit_sources {
        args.sources.iter().map(|s| s.key()).collect()
    } else {
        ["mgl_c111", "cmr_105", "cmr_244", "local"]
            .into_iter()
            .collect()
    };
    let full_only = args.full_text_only.unwrap_or(false);

    let mut notes = Vec::new();
    let loaded = load_corpora(ctx, &c, &mut notes)?;
    let scope: Option<ScopeContent> = content::load(SCOPE).ok();

    let terms = query_terms(&query);
    let cite = parse_citation(&query);
    let mut hits: Vec<LawHit> = Vec::new();
    let mut more = 0usize;
    let interpretation: String;

    match &cite {
        Some(Cite::Cmr {
            agency,
            chapter,
            section,
        }) => {
            let key = if *agency == 105 { "cmr_105" } else { "cmr_244" };
            let l = loaded.iter().find(|l| l.source == key).expect("loaded");
            match section {
                Some(sec) => {
                    interpretation = "citation lookup".into();
                    let wanted = format!("{agency} CMR {sec}");
                    for p in l.corpus.provisions.iter().filter(|p| p.c == wanted) {
                        hits.push(hit_from(l, p, &[], "citation", max_words, scope.as_ref()));
                    }
                    if hits.is_empty() {
                        // numbers like 300.2 or 300.20 are not section numbers; try a prefix
                        for p in l
                            .corpus
                            .provisions
                            .iter()
                            .filter(|p| p.n.starts_with(sec.as_str()))
                            .take(max)
                        {
                            hits.push(hit_from(l, p, &[], "citation", max_words, scope.as_ref()));
                        }
                    }
                }
                None => {
                    interpretation = "chapter lookup".into();
                    let ch = chapter.to_string();
                    let all: Vec<&Provision> =
                        l.corpus.provisions.iter().filter(|p| p.ch == ch).collect();
                    more = all.len().saturating_sub(max);
                    for p in all.into_iter().take(max) {
                        hits.push(hit_from(l, p, &[], "citation", max_words, scope.as_ref()));
                    }
                }
            }
        }
        Some(Cite::Mgl111 { section }) => {
            interpretation = "citation lookup".into();
            let l = loaded
                .iter()
                .find(|l| l.source == "mgl_c111")
                .expect("loaded");
            let want = section_key(section);
            for p in l
                .corpus
                .provisions
                .iter()
                .filter(|p| section_key(&p.n) == want)
            {
                hits.push(hit_from(l, p, &[], "citation", max_words, scope.as_ref()));
            }
        }
        None => {
            interpretation = "keyword search".into();
            let scored = keyword_search(&loaded, &selected, &terms, full_only);
            more = scored.len().saturating_sub(max);
            for s in scored.iter().take(max) {
                let l = &loaded[s.li];
                let p = &l.corpus.provisions[s.pi];
                hits.push(hit_from(
                    l,
                    p,
                    &s.matched,
                    "keywords",
                    max_words,
                    scope.as_ref(),
                ));
            }
        }
    }

    // Citation lookups must respect the sources the nurse chose.
    hits.retain(|h| selected.contains(h.source.as_str()));
    if cite.is_some() && full_only {
        hits.retain(|h| h.coverage == "full_text");
    }
    // A citation lookup of a section with text but no matched words: the opening words.
    // (hit_from already gave the first words when no terms are given.)

    // Local regulations.
    let local_wanted = selected.contains("local");
    let local = if local_wanted && cite.is_none() {
        search_local(
            ctx,
            &args.local_folder,
            &c.local,
            &query,
            &terms,
            max,
            max_words,
            explicit_sources || args.local_folder.is_some(),
        )?
    } else {
        LocalHits {
            out: LocalOut {
                searched: false,
                folder: None,
                files_indexed: 0,
                sections_indexed: 0,
                refused_client_like_files: 0,
                tables_ignored: 0,
                note: if local_wanted {
                    "A citation lookup does not search your local folder; use words to search it."
                        .into()
                } else {
                    "Local regulations were not selected.".into()
                },
            },
            hits: Vec::new(),
        }
    };
    let mut local_hits = local.hits;
    local_hits.truncate(max);
    // Interleave: state hits first (they carry dates and links), local after.
    hits.extend(local_hits);
    for (i, h) in hits.iter_mut().enumerate() {
        h.rank = (i + 1) as u32;
    }

    // Topic starting points (words, not citation lookups).
    let words = stem_words(&query);
    let find = |cite: &str| -> Option<String> {
        loaded.iter().find_map(|l| {
            l.corpus
                .provisions
                .iter()
                .find(|p| p.c == cite)
                .map(|p| format!("{} ({})", p.c, p.t))
        })
    };
    let topics: Vec<TopicOut> = if cite.is_some() {
        Vec::new()
    } else {
        c.topics
            .iter()
            .filter(|t| t.keywords.iter().any(|k| contains_phrase(&words, k)))
            .take(3)
            .map(|t| TopicOut {
                id: t.id.clone(),
                label: t.label.clone(),
                start_with: t.cites.iter().filter_map(|x| find(x)).collect(),
                related_tools: t.related_tools.clone(),
            })
            .collect()
    };

    let coverage: Vec<CoverageOut> = loaded
        .iter()
        .map(|l| {
            let full = l.corpus.full_text_count();
            let info = c.corpus.get(l.source);
            CoverageOut {
                source: l.source.into(),
                label: l.label.clone(),
                origin: l.origin.clone(),
                retrieved: l.corpus.retrieved.clone(),
                chapters: l.corpus.chapters.len(),
                sections: l.corpus.provisions.len(),
                full_text_sections: full,
                index_only_sections: l.corpus.provisions.len() - full,
                coverage: info.map(|i| flatten(&i.coverage)).unwrap_or_default(),
                not_included: info.map(|i| i.not_included.clone()).unwrap_or_default(),
            }
        })
        .collect();

    let summary = summary_text(
        &query,
        &interpretation,
        &hits,
        more,
        &topics,
        &local.out,
        &coverage,
    );

    let mut sources: Vec<Source> = Vec::new();
    for l in &loaded {
        if !selected.contains(l.source) || !hits.iter().any(|h| h.source == l.source) {
            continue;
        }
        if let Some(e) = &l.entry {
            let mut s = e.to_source();
            if l.origin == "extended" {
                s.title = format!("{} (larger corpus in law_corpus/)", l.label);
                s.mode = SourceMode::Local;
            }
            sources.push(s);
        }
    }
    if hits.iter().any(|h| h.source == "local") {
        sources.push(Source {
            title: format!(
                "Your local regulations ({})",
                local.out.folder.clone().unwrap_or_default()
            ),
            publisher: "Your board of health (data folder)".into(),
            url: None,
            effective_date: None,
            retrieved: None,
            mode: SourceMode::Local,
        });
    }
    if sources.is_empty() {
        // Searched but found nothing: still say what was searched.
        for l in &loaded {
            if selected.contains(l.source)
                && let Some(e) = &l.entry
            {
                sources.push(e.to_source());
            }
        }
    }

    let mut caveats = vec![
        "Decision support, not legal advice. These are citations and short excerpts, not statements of what the law requires in your situation; read the cited section at its link and ask your agency's counsel."
            .to_string(),
        "The bundled corpus is partial: some sections are in full text and the rest are title and link only (each hit says which). A search that finds nothing does not mean no law applies."
            .to_string(),
        "Regulations come from unofficial copies on mass.gov; the official text is published by the Secretary of the Commonwealth. Check the chapter date and for amendments after the retrieval date."
            .to_string(),
    ];
    if hits.iter().any(|h| h.coverage == "index_only") {
        caveats.push(
            "Some hits are index-only (title and link): the section's text is not bundled, so no excerpt is shown."
                .into(),
        );
    }
    if hits.iter().all(|h| h.source == "local") && !hits.is_empty() {
        caveats.push(
            "Only your local regulations matched. Confirm they are the current adopted versions."
                .into(),
        );
    }
    caveats.extend(notes);
    caveats.extend(content::review_caveats(&[LAW]));

    let mut privacy = Privacy {
        row_level_data: false,
        notes: vec![
            "Free-text queries are checked for identifiers and refused if found. Nothing is written."
                .into(),
        ],
        ..Privacy::none()
    };
    if local.out.searched {
        privacy.notes.push(
            "Your local regulation files were read and indexed inside your container; phone numbers, e-mail and street addresses, and record identifiers are masked in excerpts, and files that look like client records are refused."
                .into(),
        );
    }

    Ok(Envelope::new(
        summary,
        LawResult {
            query,
            interpretation,
            hits,
            more_hits: more,
            topics,
            local: local.out,
            coverage,
            refer_to: ReferralOut {
                text: flatten(&c.referral.text),
                contacts: c.referral.contacts.clone(),
            },
            limits: c.limits.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy)
    .decision_support())
}

fn summary_text(
    query: &str,
    interpretation: &str,
    hits: &[LawHit],
    more: usize,
    topics: &[TopicOut],
    local: &LocalOut,
    coverage: &[CoverageOut],
) -> String {
    let mut s = format!("**Public health law search** ({interpretation}): \"{query}\"\n\n");
    s.push_str(
        "Citations and short excerpts only; this does not say what the law requires in your \
         situation. Read the section at its link and ask your agency's counsel.\n\n",
    );
    if hits.is_empty() {
        s.push_str(
            "No section matched. The bundled corpus is partial; try other words, a citation, or the topic starting points below.\n\n",
        );
    } else {
        let rows: Vec<Vec<String>> = hits
            .iter()
            .map(|h| {
                vec![
                    h.rank.to_string(),
                    h.citation.clone(),
                    h.title.clone(),
                    match h.coverage.as_str() {
                        "full_text" => "full text".to_string(),
                        "index_only" => "index only".to_string(),
                        _ => "your file".to_string(),
                    },
                    h.chapter_date.clone().unwrap_or_else(|| "-".into()),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["#", "Citation", "Title", "Coverage", "Chapter date"],
            &rows,
        ));
        s.push('\n');
        for h in hits {
            if let Some(e) = &h.excerpt {
                s.push_str(&format!("- **{}**: \"{}\"", h.citation, e));
                if let Some(u) = &h.url {
                    s.push_str(&format!(" ({u})"));
                }
                s.push('\n');
            } else if let Some(u) = &h.url {
                s.push_str(&format!(
                    "- **{}** (title and link only): {u}\n",
                    h.citation
                ));
            }
        }
        if more > 0 {
            s.push_str(&format!(
                "\n{more} more match(es); raise max_results or narrow the words.\n"
            ));
        }
        s.push('\n');
    }
    if !topics.is_empty() {
        s.push_str("**Where to start for this topic**\n\n");
        for t in topics {
            s.push_str(&format!("- {}: {}\n", t.label, t.start_with.join("; ")));
        }
        s.push('\n');
    }
    s.push_str(&format!("Local regulations: {}\n\n", local.note));
    s.push_str("**What is in the corpus:** ");
    s.push_str(
        &coverage
            .iter()
            .map(|c| {
                format!(
                    "{} ({} sections, {} in full text; {})",
                    c.label, c.sections, c.full_text_sections, c.retrieved
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
    );
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_are_parsed() {
        assert_eq!(
            parse_citation("what does 105 CMR 300.200 say"),
            Some(Cite::Cmr {
                agency: 105,
                chapter: 300,
                section: Some("300.200".into())
            })
        );
        assert_eq!(
            parse_citation("105CMR300.000"),
            Some(Cite::Cmr {
                agency: 105,
                chapter: 300,
                section: None
            })
        );
        assert_eq!(
            parse_citation("244 CMR 3.05"),
            Some(Cite::Cmr {
                agency: 244,
                chapter: 3,
                section: Some("3.05".into())
            })
        );
        for q in [
            "M.G.L. c. 111, § 95",
            "MGL c.111 s.95",
            "chapter 111 section 95",
            "G.L. c. 111 § 95",
            "ch. 111, sec. 95",
        ] {
            assert_eq!(
                parse_citation(q),
                Some(Cite::Mgl111 {
                    section: "95".into()
                }),
                "{q}"
            );
        }
        assert_eq!(
            parse_citation("M.G.L. c. 111, § 5A 1/2"),
            Some(Cite::Mgl111 {
                section: "5A 1/2".into()
            })
        );
        assert_eq!(parse_citation("chapter 111 public health powers"), None);
        assert_eq!(parse_citation("board of health quarantine"), None);
    }

    #[test]
    fn excerpts_never_exceed_the_word_limit() {
        let text = (1..=200)
            .map(|i| {
                if i == 120 {
                    "quarantine".to_string()
                } else {
                    format!("word{i}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let e = excerpt(&text, &["quarantine".to_string()], 25);
        assert!(e.split_whitespace().count() <= 25, "{e}");
        assert!(e.contains("quarantine"));
        assert!(e.starts_with('…') && e.ends_with('…'));
        let short = excerpt("one two three", &[], 25);
        assert_eq!(short, "one two three");
        let first = excerpt(&text, &[], 25);
        assert!(first.split_whitespace().count() <= 25 && first.starts_with("word1 "));
    }

    #[test]
    fn query_terms_drop_common_words() {
        assert_eq!(
            query_terms("Can the board of health order a quarantine?"),
            vec!["board", "health", "order", "quarantine"]
        );
    }

    #[test]
    fn contact_details_are_masked() {
        let r = redact_contacts(
            "Call the clerk at 413-555-0142 or clerk@town.example.org about the permit.",
        );
        assert!(
            !r.contains("413-555-0142") && !r.contains("clerk@town"),
            "{r}"
        );
        assert!(r.contains("permit"));
    }
}
