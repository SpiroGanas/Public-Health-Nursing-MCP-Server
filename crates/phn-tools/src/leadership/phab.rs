//! LEAD-04 `phab_accreditation_crosswalk`: maps the documents in a local folder to the domains
//! and standards of the PHAB Standards & Measures (Version 2022) and shows which standards have
//! evidence and which have gaps.
//!
//! It reuses the Phase 1 document index (`phn_local::policy_index`, the tantivy index behind
//! `search_agency_policies`), so documents are searched the same way. PHAB's own text is
//! copyrighted and could not be read from the build network, so the bundled content holds only
//! domain names, standard numbers and short titles (as CDC's toolkit shows them) and this
//! project's keyword cues; measure IDs come only from a file the nurse supplies.
//!
//! Privacy: documents are the nurse's own agency documents. A file that looks like a client
//! record (labeled birth date, record or health plan number, street addresses, "Client name:"
//! fields) or like an interview or meeting transcript is left out entirely; passages shown are at
//! most 20 words, with names and contact details redacted; the response names documents by the
//! nurse's own file names and never quotes more.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_local::extract::{FileKind, extract};
use phn_local::phi::{Identifier, RedactionLevel, redact_text, scan_text};
use phn_local::policy_index::{PolicyHit, PolicyIndex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::strategy_docs::{
    DocBuilder, OutFormat, clip, formats_or_default, tidy, tidy_opt, write_documents,
};
use crate::context::Context;
use crate::epr::content::{self, DocSource};
use crate::md;

/// Content id of `content/leadership/phab_standards.yaml`.
pub const PHAB_STANDARDS: &str = "leadership-phab-standards";

const MAX_FILES: usize = 2_000;
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_MEASURES: usize = 400;
const MAX_SNIPPET_WORDS: usize = 20;
const HITS_PER_CUE: usize = 10;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct MeasureCols {
    id: String,
    title: String,
    keywords: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DocType {
    id: String,
    label: String,
    cues: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MatchRules {
    strong: String,
    some: String,
    none: String,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PhabInfo {
    version_label: String,
    version_check: String,
    license_note: String,
    link: String,
    measures_note: String,
    measure_columns: MeasureCols,
    document_types_note: String,
    document_types: Vec<DocType>,
    match_rules: MatchRules,
}

#[derive(Debug, Clone, Deserialize)]
struct StandardDef {
    id: String,
    title: String,
    cues: Vec<String>,
    document_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DomainDef {
    id: u8,
    name: String,
    standards: Vec<StandardDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct PhabContent {
    sources: BTreeMap<String, DocSource>,
    phab: PhabInfo,
    domains: Vec<DomainDef>,
    not_verified: Vec<String>,
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `phab_accreditation_crosswalk`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PhabArgs {
    /// Folder under /data holding the documents to map (policies, plans, reports, minutes):
    /// PDF, DOCX, Markdown, text, or HTML, including subfolders. Default: the policy folder
    /// (normally /data/policies).
    pub folder: Option<String>,
    /// Limit the crosswalk to these PHAB domains (1 to 10). Default: all ten.
    pub domains: Vec<u8>,
    /// A CSV/XLSX in /data with PHAB measures from your own copy of the Standards & Measures:
    /// columns "Measure ID" (for example 1.1.1), "Short title", and optionally "Keywords"
    /// (separated by semicolons). The tool is not given PHAB's measure text; this is how
    /// measure-level evidence is mapped. The list stays in your files; the response shows only
    /// IDs and evidence levels.
    pub measures_path: Option<String>,
    /// Sheet name when the measures file is a workbook.
    pub measures_sheet: Option<String>,
    /// Most documents listed per standard (1 to 10, default 5).
    pub max_documents_per_standard: Option<usize>,
    /// Write the crosswalk to /data/output (default true).
    pub write_documents: Option<bool>,
    /// "docx" and/or "xlsx" (default both).
    pub formats: Vec<OutFormat>,
}

/// How much evidence the documents give for a standard.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// No cue was found in any indexed document.
    None,
    /// At least one cue was found.
    Some,
    /// Two or more cues in one section, or cues in three or more sections.
    Strong,
}

impl Evidence {
    fn label(self) -> &'static str {
        match self {
            Evidence::None => "none found (gap)",
            Evidence::Some => "some",
            Evidence::Strong => "strong",
        }
    }
}

/// A document that matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocMatch {
    /// File path relative to the folder searched (your own file name).
    pub file: String,
    pub sections_matched: usize,
    pub cues: Vec<String>,
    /// A passage of at most 20 words with names and contact details redacted (the best match
    /// in this document), when one was found in the text.
    pub passage: Option<String>,
    /// The file name itself suggests this kind of document (for example "Strategic Plan").
    pub file_name_suggests: Option<String>,
}

/// One standard's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StandardResult {
    pub id: String,
    pub domain: u8,
    pub title: String,
    pub evidence: Evidence,
    pub cues_matched: Vec<String>,
    pub documents_total: usize,
    pub documents: Vec<DocMatch>,
}

/// One domain's summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainSummary {
    pub id: u8,
    pub name: String,
    pub standards: usize,
    pub strong: usize,
    pub some: usize,
    pub none: usize,
}

/// A measure from the nurse's own list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasureResult {
    pub id: String,
    /// The standard the measure number belongs to (for example 1.1), when it follows PHAB's numbering.
    pub standard: Option<String>,
    pub evidence: Evidence,
    pub documents: Vec<String>,
}

/// The measures file's summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasuresOut {
    pub path: String,
    pub measures: usize,
    pub strong: usize,
    pub some: usize,
    pub none: usize,
    pub results: Vec<MeasureResult>,
}

/// A document and the standards it supports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentUse {
    pub file: String,
    pub standards: Vec<String>,
}

/// What the document index did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexOut {
    pub folder: String,
    pub files_indexed: usize,
    pub sections_indexed: usize,
    pub rebuilt: bool,
    /// Files left out because they look like client records.
    pub left_out_client_like: usize,
    /// Files left out because they look like interview or meeting transcripts.
    pub left_out_transcripts: usize,
    /// Files the index could not read (scanned images, encrypted or damaged files).
    pub unreadable: usize,
}

/// Files written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhabFiles {
    pub folder: String,
    pub files: Vec<String>,
}

/// Result of `phab_accreditation_crosswalk`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhabResult {
    pub phab_version: String,
    pub index: IndexOut,
    pub domains: Vec<DomainSummary>,
    pub standards: Vec<StandardResult>,
    /// Standards with no evidence found.
    pub gaps: Vec<String>,
    pub documents: Vec<DocumentUse>,
    pub measures: Option<MeasuresOut>,
    pub match_rules: Vec<String>,
    pub license_note: String,
    pub phab_link: String,
    pub files: Option<PhabFiles>,
}

// ---------------------------------------------------------------- file classes

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileClass {
    Usable,
    ClientLike,
    Transcript,
    Unreadable,
}

/// Lines such as "Name: text" or "Q: text" from a transcript.
fn speaker_turns(text: &str) -> usize {
    text.lines()
        .filter(|l| {
            let l = l.trim_start();
            let Some((head, rest)) = l.split_once(':') else {
                return false;
            };
            if rest.trim().len() < 12 {
                return false;
            }
            let h = head.trim();
            let words: Vec<&str> = h.split_whitespace().collect();
            if words.is_empty() || words.len() > 3 || h.chars().count() > 30 {
                return false;
            }
            if matches!(h, "Q" | "A") {
                return true;
            }
            words.iter().all(|w| {
                let mut c = w.chars();
                c.next().is_some_and(|f| f.is_uppercase())
                    && c.all(|x| x.is_alphabetic() || x == '\'' || x == '-' || x == '.')
            })
        })
        .count()
}

/// "Client name:", "Patient:", "Child name:" and similar labels of a record.
fn has_record_labels(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim().to_lowercase();
        let Some((head, _)) = l.split_once(':') else {
            return false;
        };
        let words: Vec<&str> = head.split_whitespace().collect();
        words.len() <= 4
            && words.iter().any(|w| {
                matches!(
                    *w,
                    "client"
                        | "patient"
                        | "child"
                        | "student"
                        | "resident"
                        | "member"
                        | "caregiver"
                )
            })
            && words.contains(&"name")
    })
}

/// What a file is, the identifier kinds that made it client-like, and whether a passage from it
/// may be shown (not when it holds phone numbers, email addresses, or street addresses, since the
/// names beside them cannot be recognized reliably).
fn classify(path: &Path) -> (FileClass, Vec<String>, bool) {
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
        return (FileClass::Unreadable, Vec::new(), false);
    }
    let Ok(ex) = extract(path) else {
        return (FileClass::Unreadable, Vec::new(), false);
    };
    let text: String = ex
        .sections
        .iter()
        .flat_map(|s| [s.heading.as_str(), s.text.as_str()])
        .collect::<Vec<_>>()
        .join("\n");
    let findings = scan_text(&text);
    let kinds: BTreeSet<Identifier> = findings.iter().map(|f| f.kind).collect();
    let labels = |ks: &[Identifier]| -> Vec<String> {
        ks.iter()
            .filter(|k| kinds.contains(k))
            .map(|k| k.label().to_string())
            .collect()
    };
    let street = findings
        .iter()
        .filter(|f| f.kind == Identifier::StreetAddress)
        .count();
    let record_kinds = [
        Identifier::BirthDate,
        Identifier::MedicalRecordNumber,
        Identifier::HealthPlanNumber,
    ];
    if record_kinds.iter().any(|k| kinds.contains(k)) || street >= 2 || has_record_labels(&text) {
        let mut l = labels(&record_kinds);
        if street >= 2 {
            l.push(Identifier::StreetAddress.label().to_string());
        }
        return (FileClass::ClientLike, l, false);
    }
    if speaker_turns(&text) >= 5 {
        return (FileClass::Transcript, Vec::new(), false);
    }
    let contacts = [
        Identifier::Phone,
        Identifier::Fax,
        Identifier::Email,
        Identifier::StreetAddress,
    ]
    .iter()
    .any(|k| kinds.contains(k));
    (FileClass::Usable, Vec::new(), !contacts)
}

/// A file name shown to the model: the nurse's own name, unless it carries an identifier.
fn shown_name(rel: &str) -> String {
    let r = redact_text(rel, RedactionLevel::Strict);
    if r.redacted.is_empty() {
        rel.to_string()
    } else {
        "[file name withheld: it looks like it holds a personal detail]".to_string()
    }
}

/// Supported document files under the folder, as paths relative to it (forward slashes).
fn walk(sandbox: &phn_local::Sandbox, folder_label: &str) -> Result<Vec<String>> {
    let (entries, _) = phn_local::extract::list_files(sandbox, folder_label, true, MAX_FILES)?;
    let prefix = if folder_label.is_empty() || folder_label == "." {
        String::new()
    } else {
        format!("{}/", folder_label.trim_end_matches('/'))
    };
    let mut out: Vec<String> = entries
        .into_iter()
        .filter(|e| !e.is_dir)
        .filter(|e| {
            matches!(
                e.kind,
                FileKind::Pdf
                    | FileKind::Docx
                    | FileKind::Markdown
                    | FileKind::Text
                    | FileKind::Html
            )
        })
        .map(|e| e.path.strip_prefix(&prefix).unwrap_or(&e.path).to_string())
        .collect();
    out.sort();
    Ok(out)
}

fn name_words(rel: &str) -> String {
    let stem = rel.rsplit('/').next().unwrap_or(rel);
    let stem = stem.rsplit_once('.').map(|(s, _)| s).unwrap_or(stem);
    stem.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------- matching

/// First `MAX_SNIPPET_WORDS` words around the first highlighted term, markers removed and
/// names and contact details redacted.
fn passage(hit: &PolicyHit) -> Option<String> {
    let words: Vec<&str> = hit.snippet.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    let first = words.iter().position(|w| w.contains("**")).unwrap_or(0);
    let start = first.saturating_sub(MAX_SNIPPET_WORDS / 4);
    let slice: Vec<String> = words
        .iter()
        .skip(start)
        .take(MAX_SNIPPET_WORDS)
        .map(|w| w.replace("**", ""))
        .collect();
    let text = slice.join(" ");
    let red = redact_text(&text, RedactionLevel::Strict).text;
    let t = tidy(&red);
    if t.is_empty() { None } else { Some(t) }
}

#[derive(Debug, Default, Clone)]
struct Acc {
    /// (file, section) -> cues
    sections: BTreeMap<(String, String), BTreeSet<String>>,
    /// file -> cues
    files: BTreeMap<String, BTreeSet<String>>,
    /// file -> (cue, passage)
    passages: BTreeMap<String, String>,
    /// file -> what the file name suggests
    name_hits: BTreeMap<String, String>,
}

impl Acc {
    fn evidence(&self) -> Evidence {
        if self.sections.is_empty() && self.name_hits.is_empty() {
            return Evidence::None;
        }
        let strong_section = self.sections.values().any(|c| c.len() >= 2);
        if strong_section || self.sections.len() >= 3 {
            Evidence::Strong
        } else {
            Evidence::Some
        }
    }

    fn all_files(&self) -> BTreeSet<&String> {
        self.files.keys().chain(self.name_hits.keys()).collect()
    }
}

fn run_cues(
    index: &PolicyIndex,
    cues: &[String],
    usable: &BTreeSet<String>,
    no_passages: &BTreeSet<String>,
    cache: &mut BTreeMap<String, Vec<PolicyHit>>,
) -> Result<Acc> {
    let mut acc = Acc::default();
    for cue in cues {
        let q = format!("\"{}\"", cue.replace('"', ""));
        if !cache.contains_key(&q) {
            cache.insert(q.clone(), index.search(&q, HITS_PER_CUE)?);
        }
        for h in &cache[&q] {
            if !usable.contains(&h.file) {
                continue;
            }
            acc.sections
                .entry((h.file.clone(), h.section.clone()))
                .or_default()
                .insert(cue.clone());
            acc.files
                .entry(h.file.clone())
                .or_default()
                .insert(cue.clone());
            if !no_passages.contains(&h.file)
                && !acc.passages.contains_key(&h.file)
                && let Some(p) = passage(h)
            {
                acc.passages.insert(h.file.clone(), p);
            }
        }
    }
    Ok(acc)
}

fn norm_cue(s: &str) -> String {
    tidy(&s.to_lowercase())
}

// ---------------------------------------------------------------- measures file

struct MeasureIn {
    id: String,
    title: String,
    cues: Vec<String>,
}

const STOP: &[&str] = &[
    "the",
    "and",
    "for",
    "that",
    "with",
    "from",
    "this",
    "are",
    "has",
    "have",
    "into",
    "their",
    "its",
    "all",
    "any",
    "use",
    "used",
    "using",
    "health",
    "public",
    "department",
    "population",
    "measure",
    "document",
    "documentation",
    "program",
    "programs",
    "services",
    "service",
];

fn read_measures(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    cols: &MeasureCols,
) -> Result<(Vec<MeasureIn>, String, Source)> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let t = &lt.table;
    let norm = |s: &str| s.trim().to_lowercase();
    let find = |names: &[&str]| {
        t.headers
            .iter()
            .position(|h| names.contains(&norm(h).as_str()))
    };
    let id_names = [norm(&cols.id), "id".to_string(), "measure".to_string()];
    let id_refs: Vec<&str> = id_names.iter().map(String::as_str).collect();
    let title_names = [norm(&cols.title), "title".to_string()];
    let title_refs: Vec<&str> = title_names.iter().map(String::as_str).collect();
    let kw_names = [norm(&cols.keywords), "keyword".to_string()];
    let kw_refs: Vec<&str> = kw_names.iter().map(String::as_str).collect();
    let (Some(id_col), Some(title_col)) = (find(&id_refs), find(&title_refs)) else {
        return Err(PhnError::InvalidInput(format!(
            "the measures file needs a column named \"{}\" and one named \"{}\" (and optionally \"{}\")",
            cols.id, cols.title, cols.keywords
        )));
    };
    let kw_col = find(&kw_refs);
    for idx in [Some(id_col), Some(title_col), kw_col]
        .into_iter()
        .flatten()
    {
        if let Some(f) = lt.findings.iter().find(|f| f.index == idx) {
            return Err(PhnError::Privacy(format!(
                "column '{}' appears to contain {}: this does not look like a list of PHAB \
                 measures, so the file was not used",
                f.column,
                f.kind.label()
            )));
        }
    }
    if t.rows.len() > MAX_MEASURES {
        return Err(PhnError::InvalidInput(format!(
            "the measures file has {} rows; the limit is {MAX_MEASURES}",
            t.rows.len()
        )));
    }
    let mut out = Vec::new();
    for row in &t.rows {
        let cell = |i: usize| row.get(i).map(|s| s.trim()).unwrap_or("");
        let id = tidy(cell(id_col));
        if id.is_empty() {
            continue;
        }
        let title = tidy(cell(title_col));
        let mut cues: Vec<String> = kw_col
            .map(|i| {
                cell(i)
                    .split([';', '\n'])
                    .map(norm_cue)
                    .filter(|c| c.chars().count() >= 3)
                    .collect()
            })
            .unwrap_or_default();
        if cues.is_empty() {
            let mut seen = BTreeSet::new();
            for w in title
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.chars().count() >= 4 && !STOP.contains(w))
            {
                if seen.insert(w.to_string()) {
                    cues.push(w.to_string());
                }
                if cues.len() >= 6 {
                    break;
                }
            }
        }
        cues.truncate(8);
        out.push(MeasureIn { id, title, cues });
    }
    Ok((out, lt.label.clone(), lt.source()))
}

/// "1.1.1 A" -> "1.1".
fn standard_of(measure_id: &str) -> Option<String> {
    let mut parts = measure_id.split(|c: char| c == '.' || c.is_whitespace());
    let a = parts.next()?;
    let b = parts.next()?;
    if a.chars().all(|c| c.is_ascii_digit())
        && !a.is_empty()
        && b.chars().all(|c| c.is_ascii_digit())
        && !b.is_empty()
    {
        Some(format!("{a}.{b}"))
    } else {
        None
    }
}

// ---------------------------------------------------------------- the tool

/// Crosswalk the documents in a folder to the PHAB standards.
pub fn phab_accreditation_crosswalk(ctx: &Context, args: PhabArgs) -> Result<Envelope<PhabResult>> {
    let c: PhabContent = content::load(PHAB_STANDARDS)?;
    for d in &args.domains {
        if !(1..=10).contains(d) {
            return Err(PhnError::InvalidInput(format!(
                "domain {d} is not a PHAB domain; use 1 to 10"
            )));
        }
    }
    let max_docs = args.max_documents_per_standard.unwrap_or(5).clamp(1, 10);

    // ---- folder and index (shared with search_agency_policies)
    let sandbox = ctx.sandbox()?;
    let (folder_path, folder_label, index_name) = match tidy_opt(&args.folder) {
        Some(f) => {
            let p = sandbox.resolve_existing(&f)?;
            let label = sandbox.display_path(&p);
            let key: String = label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            (p, label, format!("policies-{key}"))
        }
        None => {
            let p = ctx.config.paths.policy_dir.clone();
            let label = sandbox.display_path(&p);
            (
                p,
                if label.is_empty() {
                    "policies".into()
                } else {
                    label
                },
                "policies".to_string(),
            )
        }
    };
    let index_dir = ctx.config.paths.cache_dir.join("index").join(index_name);
    let writable = std::fs::create_dir_all(&index_dir).is_ok();
    let index = PolicyIndex::open_or_build(&folder_path, writable.then_some(index_dir.as_path()))?;

    // ---- which files may be used
    let files = walk(&sandbox, &folder_label)?;
    let mut usable: BTreeSet<String> = BTreeSet::new();
    let mut left_client = 0usize;
    let mut left_transcript = 0usize;
    let mut unreadable = 0usize;
    let mut left_out: Vec<(String, String)> = Vec::new();
    let mut identifier_kinds: BTreeSet<String> = BTreeSet::new();
    let mut no_passages: BTreeSet<String> = BTreeSet::new();
    for rel in &files {
        let (class, kinds, passages_ok) = classify(&folder_path.join(rel));
        match class {
            FileClass::Usable => {
                usable.insert(rel.clone());
                if !passages_ok {
                    no_passages.insert(rel.clone());
                }
            }
            FileClass::ClientLike => {
                left_client += 1;
                identifier_kinds.extend(kinds);
                left_out.push((shown_name(rel), "looks like a client record".into()));
            }
            FileClass::Transcript => {
                left_transcript += 1;
                left_out.push((
                    shown_name(rel),
                    "looks like an interview or meeting transcript".into(),
                ));
            }
            FileClass::Unreadable => {
                unreadable += 1;
                left_out.push((shown_name(rel), "could not be read as text".into()));
            }
        }
    }
    if usable.is_empty() {
        return Err(PhnError::NotFound(format!(
            "no usable documents in /data/{folder_label}: {} file(s) looked like client records \
             or transcripts and {} could not be read. Put your agency's plans, policies, reports, \
             and minutes (PDF, DOCX, Markdown, text, or HTML) in a folder and give it as folder.",
            left_client + left_transcript,
            unreadable
        )));
    }

    // ---- standards
    let domain_filter: BTreeSet<u8> = args.domains.iter().copied().collect();
    let mut cache: BTreeMap<String, Vec<PolicyHit>> = BTreeMap::new();
    let mut standards: Vec<StandardResult> = Vec::new();
    let mut by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let dt_by_id: BTreeMap<&str, &DocType> = c
        .phab
        .document_types
        .iter()
        .map(|d| (d.id.as_str(), d))
        .collect();
    let names: BTreeMap<&String, String> = usable.iter().map(|f| (f, name_words(f))).collect();
    for d in &c.domains {
        if !domain_filter.is_empty() && !domain_filter.contains(&d.id) {
            continue;
        }
        for s in &d.standards {
            let cues: Vec<String> = s.cues.iter().map(|x| norm_cue(x)).collect();
            let mut acc = run_cues(&index, &cues, &usable, &no_passages, &mut cache)?;
            // File names that name the kind of document this standard calls for.
            for dt_id in &s.document_types {
                if let Some(dt) = dt_by_id.get(dt_id.as_str()) {
                    for (file, nw) in &names {
                        if dt.cues.iter().any(|cu| {
                            let cu = norm_cue(cu);
                            cu.chars().count() >= 4 && nw.contains(&cu)
                        }) {
                            acc.name_hits
                                .entry((*file).clone())
                                .or_insert_with(|| dt.label.clone());
                        }
                    }
                }
            }
            let evidence = acc.evidence();
            let mut docs: Vec<DocMatch> = acc
                .all_files()
                .into_iter()
                .map(|f| {
                    let cues_f: Vec<String> = acc
                        .files
                        .get(f)
                        .map(|v| v.iter().cloned().collect())
                        .unwrap_or_default();
                    let sections = acc.sections.keys().filter(|(ff, _)| ff == f).count();
                    DocMatch {
                        file: shown_name(f),
                        sections_matched: sections,
                        cues: cues_f,
                        passage: acc.passages.get(f).cloned(),
                        file_name_suggests: acc.name_hits.get(f).cloned(),
                    }
                })
                .collect();
            docs.sort_by(|a, b| {
                (b.cues.len() + b.sections_matched + usize::from(b.file_name_suggests.is_some()))
                    .cmp(
                        &(a.cues.len()
                            + a.sections_matched
                            + usize::from(a.file_name_suggests.is_some())),
                    )
                    .then(a.file.cmp(&b.file))
            });
            let total = docs.len();
            for f in acc.all_files() {
                by_file.entry(f.clone()).or_default().insert(s.id.clone());
            }
            let cues_matched: Vec<String> = acc
                .files
                .values()
                .flatten()
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            docs.truncate(max_docs);
            standards.push(StandardResult {
                id: s.id.clone(),
                domain: d.id,
                title: s.title.clone(),
                evidence,
                cues_matched,
                documents_total: total,
                documents: docs,
            });
        }
    }

    // ---- optional measures from the nurse's own file
    let mut measures_out = None;
    let mut measure_titles: BTreeMap<String, String> = BTreeMap::new();
    let mut sources: Vec<Source> = Vec::new();
    if let Some(mp) = tidy_opt(&args.measures_path) {
        let (list, label, src) = read_measures(
            ctx,
            &mp,
            tidy_opt(&args.measures_sheet).as_deref(),
            &c.phab.measure_columns,
        )?;
        sources.push(src);
        let mut results = Vec::new();
        for m in &list {
            let cues: Vec<String> = m.cues.iter().map(|x| norm_cue(x)).collect();
            let acc = run_cues(&index, &cues, &usable, &no_passages, &mut cache)?;
            // Measures built from single title words need two different words in one section
            // to count as strong, like any other cue list; one word is "some".
            let docs: Vec<String> = acc
                .all_files()
                .into_iter()
                .take(max_docs)
                .map(|f| shown_name(f))
                .collect();
            measure_titles.insert(m.id.clone(), m.title.clone());
            results.push(MeasureResult {
                standard: standard_of(&m.id),
                id: m.id.clone(),
                evidence: acc.evidence(),
                documents: docs,
            });
        }
        let count = |e: Evidence| results.iter().filter(|r| r.evidence == e).count();
        measures_out = Some(MeasuresOut {
            path: label,
            measures: results.len(),
            strong: count(Evidence::Strong),
            some: count(Evidence::Some),
            none: count(Evidence::None),
            results,
        });
    }

    // ---- summaries
    let domains: Vec<DomainSummary> = c
        .domains
        .iter()
        .filter(|d| domain_filter.is_empty() || domain_filter.contains(&d.id))
        .map(|d| {
            let mine: Vec<&StandardResult> =
                standards.iter().filter(|s| s.domain == d.id).collect();
            let n = |e: Evidence| mine.iter().filter(|s| s.evidence == e).count();
            DomainSummary {
                id: d.id,
                name: d.name.clone(),
                standards: mine.len(),
                strong: n(Evidence::Strong),
                some: n(Evidence::Some),
                none: n(Evidence::None),
            }
        })
        .collect();
    let gaps: Vec<String> = standards
        .iter()
        .filter(|s| s.evidence == Evidence::None)
        .map(|s| s.id.clone())
        .collect();
    let mut documents: Vec<DocumentUse> = by_file
        .into_iter()
        .map(|(f, st)| DocumentUse {
            file: shown_name(&f),
            standards: st.into_iter().collect(),
        })
        .collect();
    documents.sort_by(|a, b| {
        b.standards
            .len()
            .cmp(&a.standards.len())
            .then(a.file.cmp(&b.file))
    });

    // ---- documents
    let write = args.write_documents.unwrap_or(true);
    let formats = formats_or_default(&args.formats);
    let mut written_files = None;
    let mut files_written = Vec::new();
    if write {
        let doc = build_document(
            ctx,
            &c,
            &folder_label,
            &domains,
            &standards,
            &gaps,
            &documents,
            measures_out.as_ref(),
            &measure_titles,
            &left_out,
        );
        let (folder, written) = write_documents(
            ctx,
            "phab_crosswalk",
            "phab_crosswalk",
            &doc,
            &formats,
            Some("Crosswalk"),
        )?;
        files_written.extend(written.clone());
        written_files = Some(PhabFiles {
            folder,
            files: written,
        });
    }

    // ---- response
    let n = |e: Evidence| standards.iter().filter(|s| s.evidence == e).count();
    let mut s = format!(
        "**PHAB {} crosswalk: {} documents searched in /data/{folder_label}.** {} of {} standards have strong evidence, {} some, {} none found.\n\n",
        c.phab.version_label,
        usable.len(),
        n(Evidence::Strong),
        standards.len(),
        n(Evidence::Some),
        n(Evidence::None)
    );
    let rows: Vec<Vec<String>> = standards
        .iter()
        .map(|r| {
            vec![
                r.id.clone(),
                clip(&r.title, 70),
                r.evidence.label().to_string(),
                if r.documents.is_empty() {
                    "-".into()
                } else {
                    r.documents
                        .iter()
                        .take(2)
                        .map(|d| d.file.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                },
            ]
        })
        .collect();
    s.push_str(&md::table(
        &["Standard", "Title", "Evidence", "Documents"],
        &rows,
    ));
    if !gaps.is_empty() {
        s.push_str(&format!("\nGaps (no cue found): {}.\n", gaps.join(", ")));
    }
    if left_client + left_transcript + unreadable > 0 {
        s.push_str(&format!(
            "\nLeft out: {} file(s) that look like client records, {} that look like transcripts, {} unreadable.\n",
            left_client, left_transcript, unreadable
        ));
    }
    if let Some(m) = &measures_out {
        s.push_str(&format!(
            "\nMeasures from your file: {} ({} strong, {} some, {} none).\n",
            m.measures, m.strong, m.some, m.none
        ));
    }
    if let Some(f) = &written_files {
        s.push_str(&format!("\nCrosswalk written to {}.\n", f.folder));
    }

    let mut caveats = vec![
        "A keyword match shows that a document mentions a topic, not that it meets a PHAB measure. PHAB decides by its own documentation requirements, dates, and approvals; check each standard in PHAB's document.".to_string(),
        c.phab.license_note.clone(),
        c.phab.measures_note.clone(),
        c.phab.version_check.clone(),
        format!("{} {}", c.phab.document_types_note, c.phab.match_rules.note),
        "The tool does not read document dates, approval status, or whether a document is current (PHAB expects, for example, a community health assessment and improvement plan dated within five years).".to_string(),
        "Passages are at most 20 words, with names and contact details redacted. Files that look like client records or transcripts are left out; check the left-out list in the output file.".to_string(),
    ];
    if !writable {
        caveats.push(
            "The /cache volume is not writable, so the index is rebuilt on every search.".into(),
        );
    }
    if index.stats.skipped.len() > unreadable {
        caveats.push(format!(
            "{} file(s) could not be indexed (scanned images, damaged, or empty).",
            index.stats.skipped.len()
        ));
    }
    caveats.extend(content::review_caveats(&[PHAB_STANDARDS]));
    caveats.extend(c.not_verified.iter().map(|x| format!("Not verified: {x}")));

    let mut srcs =
        content::sources_for(["cdc-crosswalk", "cdc-drivers", "phab-sm-2022"], &c.sources);
    srcs.push(Source {
        title: format!("Documents in /data/{folder_label} (your own)"),
        publisher: "Your agency's documents (/data)".into(),
        url: None,
        effective_date: None,
        retrieved: None,
        mode: SourceMode::Local,
    });
    srcs.extend(sources);
    let match_rules = vec![
        format!("Strong: {}", c.phab.match_rules.strong),
        format!("Some: {}", c.phab.match_rules.some),
        format!("None: {}", c.phab.match_rules.none),
    ];
    Ok(Envelope::new(
        s,
        PhabResult {
            phab_version: c.phab.version_label.clone(),
            index: IndexOut {
                folder: folder_label,
                files_indexed: index.stats.files_indexed,
                sections_indexed: index.stats.sections_indexed,
                rebuilt: index.stats.rebuilt,
                left_out_client_like: left_client,
                left_out_transcripts: left_transcript,
                unreadable,
            },
            domains,
            standards,
            gaps,
            documents: documents.into_iter().take(15).collect(),
            measures: measures_out,
            match_rules,
            license_note: c.phab.license_note.clone(),
            phab_link: c.phab.link.clone(),
            files: written_files,
        },
    )
    .sources(srcs)
    .caveats(caveats)
    .decision_support()
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: identifier_kinds.into_iter().collect(),
        files_written,
        notes: vec![
            "Documents are searched locally; the response has your file names, evidence levels, and passages of at most 20 words with names and contact details redacted. Files that look like client records or transcripts were not used.".into(),
        ],
        ..Privacy::none()
    }))
}

#[allow(clippy::too_many_arguments)]
fn build_document(
    ctx: &Context,
    c: &PhabContent,
    folder: &str,
    domains: &[DomainSummary],
    standards: &[StandardResult],
    gaps: &[String],
    documents: &[DocumentUse],
    measures: Option<&MeasuresOut>,
    measure_titles: &BTreeMap<String, String>,
    left_out: &[(String, String)],
) -> phn_docs::model::Document {
    let mut b = DocBuilder::new();
    b.heading(1, "About this crosswalk");
    b.para(format!(
        "Documents in /data/{folder} mapped to the {} domains and standards. A match means a \
         document mentions a topic; it does not mean the document meets a PHAB measure.",
        c.phab.version_label
    ));
    b.labeled("PHAB licence:", c.phab.license_note.trim().to_string());
    b.labeled(
        "PHAB document:",
        format!("{} ({})", c.phab.link, tidy(&c.phab.version_check)),
    );
    b.labeled("Measures:", c.phab.measures_note.trim().to_string());
    b.heading(1, "How matches are scored");
    b.list(&[
        format!("Strong: {}", c.phab.match_rules.strong),
        format!("Some: {}", c.phab.match_rules.some),
        format!("None: {}", c.phab.match_rules.none),
        c.phab.match_rules.note.trim().to_string(),
    ]);
    b.heading(1, "Summary by domain");
    b.table(
        "Domains",
        &["Domain", "Name", "Standards", "Strong", "Some", "None"],
        domains
            .iter()
            .map(|d| {
                vec![
                    d.id.to_string(),
                    d.name.clone(),
                    d.standards.to_string(),
                    d.strong.to_string(),
                    d.some.to_string(),
                    d.none.to_string(),
                ]
            })
            .collect(),
    );
    b.heading(1, "Crosswalk by standard");
    b.table(
        "Crosswalk",
        &[
            "Standard",
            "Domain",
            "Short title",
            "Evidence",
            "Documents",
            "Cues matched",
            "Passage (up to 20 words)",
        ],
        standards
            .iter()
            .map(|s| {
                vec![
                    s.id.clone(),
                    s.domain.to_string(),
                    s.title.clone(),
                    s.evidence.label().to_string(),
                    s.documents
                        .iter()
                        .map(|d| d.file.clone())
                        .collect::<Vec<_>>()
                        .join("; "),
                    s.cues_matched.join("; "),
                    s.documents
                        .iter()
                        .find_map(|d| d.passage.clone())
                        .unwrap_or_default(),
                ]
            })
            .collect(),
    );
    b.heading(1, "Gaps: no evidence found");
    if gaps.is_empty() {
        b.para("Every standard in scope has at least one match.");
    } else {
        b.list(
            &standards
                .iter()
                .filter(|s| s.evidence == Evidence::None)
                .map(|s| format!("{}: {}", s.id, s.title))
                .collect::<Vec<_>>(),
        );
    }
    b.heading(1, "Documents and the standards they support");
    b.table(
        "By document",
        &["Document", "Standards"],
        documents
            .iter()
            .map(|d| vec![d.file.clone(), d.standards.join(", ")])
            .collect(),
    );
    if let Some(m) = measures {
        b.heading(1, "Measures from your own list");
        b.table(
            "Measures",
            &[
                "Measure ID",
                "Short title",
                "Standard",
                "Evidence",
                "Documents",
            ],
            m.results
                .iter()
                .map(|r| {
                    vec![
                        r.id.clone(),
                        measure_titles.get(&r.id).cloned().unwrap_or_default(),
                        r.standard.clone().unwrap_or_default(),
                        r.evidence.label().to_string(),
                        r.documents.join("; "),
                    ]
                })
                .collect(),
        );
    }
    b.heading(1, "Files left out");
    if left_out.is_empty() {
        b.para("No files were left out.");
    } else {
        b.para("These files were not used: client records and transcripts are never searched for evidence.");
        b.table(
            "Left out",
            &["File", "Why"],
            left_out
                .iter()
                .map(|(f, w)| vec![f.clone(), w.clone()])
                .collect(),
        );
    }
    b.finish(
        ctx,
        "PHAB accreditation document crosswalk",
        Some(format!(
            "{}; draft for the nurse's review",
            c.phab.version_label
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/policies")).unwrap();
        for (name, text) in files {
            let p = dir.path().join("data").join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, text).unwrap();
        }
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    const CHA: &str = "# Community Health Assessment 2024\nThis community health assessment used a community survey and focus group sessions.\n\
        ## Data sources\nVital statistics and a data dashboard informed the health assessment.\n";
    const PLAN: &str =
        "# Strategic plan 2025-2029\nOur mission and vision guide the strategic priorities.\n";

    #[test]
    fn finds_evidence_and_gaps_with_short_passages() {
        let (_d, ctx) = ctx_with(&[
            ("policies/cha_2024.md", CHA),
            ("policies/strategic_plan.md", PLAN),
        ]);
        let r = phab_accreditation_crosswalk(
            &ctx,
            PhabArgs {
                write_documents: Some(false),
                ..PhabArgs::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.standards.len(), 20);
        let s11 = r.data.standards.iter().find(|s| s.id == "1.1").unwrap();
        assert_eq!(s11.evidence, Evidence::Strong);
        assert_eq!(s11.documents[0].file, "cha_2024.md");
        let s101 = r.data.standards.iter().find(|s| s.id == "10.1").unwrap();
        assert!(s101.evidence >= Evidence::Some);
        let s22 = r.data.standards.iter().find(|s| s.id == "2.2").unwrap();
        assert_eq!(s22.evidence, Evidence::None);
        assert!(r.data.gaps.contains(&"2.2".to_string()));
        for s in &r.data.standards {
            for d in &s.documents {
                if let Some(p) = &d.passage {
                    assert!(p.split_whitespace().count() <= MAX_SNIPPET_WORDS, "{p}");
                    assert!(!p.contains("**"));
                }
            }
        }
        assert!(r.data.documents.iter().any(|d| d.file == "cha_2024.md"));
    }

    #[test]
    fn client_records_and_transcripts_are_left_out() {
        let note = "Home visit note\nClient: Mrs. Rosa Delgado, DOB: 04/17/1958, MRN 00451234.\nCommunity health assessment of her needs.\n";
        let transcript = "Focus group on the community health assessment\n\
            Odalys: I will start with the community health assessment and rent.\n\
            Thandiwe: Same here with the community health assessment honestly.\n\
            Bao: Rent, yes. The community health assessment should ask about buses.\n\
            Cormac: We did a community survey about the community health assessment too.\n\
            Yasmin: The community health assessment should include a focus group.\n";
        let (_d, ctx) = ctx_with(&[
            ("policies/note.txt", note),
            ("policies/focus.txt", transcript),
            ("policies/plan.md", PLAN),
        ]);
        let r = phab_accreditation_crosswalk(
            &ctx,
            PhabArgs {
                write_documents: Some(false),
                ..PhabArgs::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.index.left_out_client_like, 1);
        assert_eq!(r.data.index.left_out_transcripts, 1);
        let all = serde_json::to_string(&r).unwrap();
        for secret in [
            "Delgado",
            "00451234",
            "Odalys",
            "Thandiwe",
            "note.txt",
            "focus.txt",
        ] {
            assert!(!all.contains(secret), "{secret} leaked");
        }
        let s11 = r.data.standards.iter().find(|s| s.id == "1.1").unwrap();
        assert_eq!(s11.evidence, Evidence::None);
    }

    #[test]
    fn measures_file_adds_measure_level_rows_without_titles_in_the_response() {
        let m = "Measure ID,Short title,Keywords\n1.1.1,Zebra assessment cycle,community health assessment; community survey\n2.2.1,Emergency drills,tabletop exercise\n";
        let (_d, ctx) = ctx_with(&[("policies/cha.md", CHA), ("measures.csv", m)]);
        let r = phab_accreditation_crosswalk(
            &ctx,
            PhabArgs {
                measures_path: Some("measures.csv".into()),
                write_documents: Some(false),
                domains: vec![1, 2],
                ..PhabArgs::default()
            },
        )
        .unwrap();
        let ms = r.data.measures.as_ref().unwrap();
        assert_eq!(ms.measures, 2);
        assert_eq!(ms.results[0].standard.as_deref(), Some("1.1"));
        assert_eq!(ms.results[0].evidence, Evidence::Strong);
        assert_eq!(ms.results[1].evidence, Evidence::None);
        assert!(!serde_json::to_string(&r).unwrap().contains("Zebra"));
        assert_eq!(r.data.domains.len(), 2);
    }

    #[test]
    fn writes_the_crosswalk_files() {
        let (dir, ctx) = ctx_with(&[("policies/cha_2024.md", CHA)]);
        let r = phab_accreditation_crosswalk(&ctx, PhabArgs::default()).unwrap();
        let f = r.data.files.unwrap();
        assert_eq!(f.files.len(), 2);
        for rel in &f.files {
            assert!(dir.path().join("data").join(rel).exists(), "{rel}");
        }
    }

    #[test]
    fn errors_are_clear() {
        let (_d, ctx) = ctx_with(&[]);
        let e = phab_accreditation_crosswalk(&ctx, PhabArgs::default()).unwrap_err();
        assert!(e.to_string().contains("no usable documents"), "{e}");
        let e = phab_accreditation_crosswalk(
            &ctx,
            PhabArgs {
                domains: vec![11],
                ..PhabArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("1 to 10"));
        let e = phab_accreditation_crosswalk(
            &ctx,
            PhabArgs {
                folder: Some("nowhere".into()),
                ..PhabArgs::default()
            },
        )
        .unwrap_err();
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn standard_numbers_come_from_measure_ids() {
        assert_eq!(standard_of("1.1.1 A").as_deref(), Some("1.1"));
        assert_eq!(standard_of("10.3.2").as_deref(), Some("10.3"));
        assert_eq!(standard_of("abc"), None);
    }
}
