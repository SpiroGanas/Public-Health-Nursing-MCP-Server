//! COMM-02 `translate_health_material`: the translation workflow, without
//! bundled translations (product-owner decision). The tool
//!
//! * lists the language priorities for the town from EQ-03 (ACS limited
//!   English proficiency by language);
//! * reports agency-approved translations found in `/data/templates/<language>/`;
//! * builds a translator brief: numbered segments, glossary terms found in the
//!   source (from the agency's glossary CSV), and items that must not change
//!   (numbers, dates, phone numbers, web addresses);
//! * checks a translated draft against the glossary and those items;
//! * compares an English back-translation with the source (numbers, dates,
//!   negations, glossary terms, missing sentences); and
//! * labels every draft for review by a qualified translator, optionally
//!   writing a review worksheet (XLSX) to /data/output.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, NamedSource, TRANSLATION};
use super::textstats;
use crate::context::Context;
use crate::equity::{LanguageAccessArgs, language_access_profile};

#[derive(Debug, Clone, Deserialize)]
struct Cited {
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TranslationContent {
    meta: ContentMeta,
    sources: BTreeMap<String, NamedSource>,
    draft_label: String,
    workflow: Vec<Cited>,
    pitfalls: Vec<Cited>,
}

static CONTENT: LazyLock<TranslationContent> = LazyLock::new(|| {
    content::load(TRANSLATION).expect("content/comms/translation.yaml matches its schema")
});

/// A recognized language: (key, display name, BCP 47 tag, aliases, script note).
type Language = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    Option<&'static str>,
);
/// Glossary rows (English term, approved translation) and the file label.
type Glossary = (Vec<(String, Option<String>)>, String);

/// Languages the tool recognizes.
const LANGUAGES: &[Language] = &[
    (
        "spanish",
        "Spanish",
        "es-US",
        &["es", "espanol", "español"],
        None,
    ),
    (
        "portuguese",
        "Portuguese",
        "pt-BR",
        &["pt", "pt-br", "brazilian portuguese", "português"],
        None,
    ),
    (
        "haitian_creole",
        "Haitian Creole",
        "ht",
        &["ht", "kreyol", "kreyòl", "haitian", "creole"],
        None,
    ),
    (
        "cape_verdean_creole",
        "Cape Verdean Creole",
        "kea",
        &["kea", "kriolu", "cape verdean"],
        None,
    ),
    (
        "vietnamese",
        "Vietnamese",
        "vi",
        &["vi", "tiếng việt"],
        None,
    ),
    (
        "chinese_simplified",
        "Chinese (Simplified)",
        "zh-Hans",
        &["zh", "zh-hans", "chinese", "mandarin", "simplified chinese"],
        Some("CJK characters: export as DOCX; PDF cannot render them (ADR 0003)."),
    ),
    (
        "chinese_traditional",
        "Chinese (Traditional)",
        "zh-Hant",
        &["zh-hant", "cantonese", "traditional chinese"],
        Some("CJK characters: export as DOCX; PDF cannot render them (ADR 0003)."),
    ),
    (
        "arabic",
        "Arabic",
        "ar",
        &["ar"],
        Some("Right-to-left script: export as DOCX; PDF cannot render it (ADR 0003)."),
    ),
    ("russian", "Russian", "ru", &["ru"], None),
    (
        "khmer",
        "Khmer",
        "km",
        &["km", "cambodian"],
        Some("Khmer script: export as DOCX; PDF cannot render it (ADR 0003)."),
    ),
    ("french", "French", "fr", &["fr"], None),
    (
        "korean",
        "Korean",
        "ko",
        &["ko"],
        Some("Korean script: export as DOCX; PDF cannot render it."),
    ),
    ("somali", "Somali", "so", &["so"], None),
    (
        "nepali",
        "Nepali",
        "ne",
        &["ne"],
        Some("Devanagari script: export as DOCX."),
    ),
    ("swahili", "Swahili", "sw", &["sw"], None),
    ("polish", "Polish", "pl", &["pl"], None),
    ("italian", "Italian", "it", &["it"], None),
    ("greek", "Greek", "el", &["el"], None),
    (
        "hindi",
        "Hindi",
        "hi",
        &["hi"],
        Some("Devanagari script: export as DOCX."),
    ),
    (
        "gujarati",
        "Gujarati",
        "gu",
        &["gu"],
        Some("Gujarati script: export as DOCX."),
    ),
    (
        "amharic",
        "Amharic",
        "am",
        &["am"],
        Some("Ethiopic script: export as DOCX."),
    ),
    (
        "tigrinya",
        "Tigrinya",
        "ti",
        &["ti"],
        Some("Ethiopic script: export as DOCX."),
    ),
    (
        "dari",
        "Dari",
        "prs",
        &["prs"],
        Some("Arabic script: export as DOCX."),
    ),
    (
        "pashto",
        "Pashto",
        "ps",
        &["ps"],
        Some("Arabic script: export as DOCX."),
    ),
    ("ukrainian", "Ukrainian", "uk", &["uk"], None),
    ("albanian", "Albanian", "sq", &["sq"], None),
    (
        "bengali",
        "Bengali",
        "bn",
        &["bn", "bangla"],
        Some("Bengali script: export as DOCX."),
    ),
    (
        "urdu",
        "Urdu",
        "ur",
        &["ur"],
        Some("Arabic script: export as DOCX."),
    ),
    (
        "japanese",
        "Japanese",
        "ja",
        &["ja"],
        Some("Japanese script: export as DOCX."),
    ),
    ("turkish", "Turkish", "tr", &["tr"], None),
    (
        "persian",
        "Persian (Farsi)",
        "fa",
        &["fa", "farsi"],
        Some("Arabic script: export as DOCX."),
    ),
];

fn lang_key(s: &str) -> String {
    s.trim().to_lowercase().replace(['-', ' '], "_")
}

fn resolve_language(q: &str) -> Result<&'static Language> {
    let k = lang_key(q);
    LANGUAGES
        .iter()
        .find(|l| {
            l.0 == k
                || lang_key(l.1) == k
                || l.3.iter().any(|a| lang_key(a) == k)
                || lang_key(l.2) == k
        })
        .ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "language '{q}' is not recognized; use one of {}",
                LANGUAGES.iter().map(|l| l.1).collect::<Vec<_>>().join(", ")
            ))
        })
}

/// Arguments for `translate_health_material`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TranslationArgs {
    /// The English source (plain text or Markdown). Give this or `source_path`.
    pub source_text: Option<String>,
    /// The English source as a file in /data (DOCX, TXT, Markdown, HTML, PDF).
    pub source_path: Option<String>,
    /// Target language (e.g. "Spanish", "Portuguese", "Haitian Creole", "Vietnamese",
    /// "Chinese (Simplified)"). Omit to see the town's language priorities only.
    pub language: Option<String>,
    /// The agency's glossary in /data (CSV/XLSX): either a column "english" plus one column per
    /// language (named by language or code, e.g. "spanish" or "es"), or columns "english",
    /// "language", "translation".
    pub glossary_path: Option<String>,
    /// A translated draft to check (pasted).
    pub translation_text: Option<String>,
    /// A translated draft in /data to check instead of `translation_text`.
    pub translation_path: Option<String>,
    /// An English back-translation of the draft, made by someone other than the translator.
    pub back_translation_text: Option<String>,
    /// A back-translation file in /data instead of `back_translation_text`.
    pub back_translation_path: Option<String>,
    /// Town for language priorities (default: your configured town).
    pub town: Option<String>,
    /// Name of the material (file stem) to look for in /data/templates/<language>/.
    pub material_name: Option<String>,
    /// Write a translation review worksheet to /data/output ("xlsx").
    pub worksheet: Option<String>,
}

/// A language with many residents who speak English less than very well.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguagePriority {
    pub language: String,
    /// Residents 5+ who speak English less than "very well" (ACS estimate).
    pub lep_estimate: f64,
    pub lep_moe: Option<f64>,
    /// Where the estimate comes from ("PUMA" detailed languages or the town's language group).
    pub geography: String,
}

/// A glossary term in the source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlossaryUse {
    pub english: String,
    /// The agency's approved translation, when the glossary has one for the language.
    pub approved: Option<String>,
    pub times_in_source: usize,
    /// Times the approved term appears in the translated draft (when checked).
    pub times_in_translation: Option<usize>,
    /// True when the term appears in the source but its approved translation does not appear in the draft.
    pub missing_in_translation: bool,
}

/// An item that must carry over unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InvariantCheck {
    /// "number", "phone", "web_address", "email", or "time".
    pub kind: String,
    pub value: String,
    pub in_translation: Option<bool>,
    pub in_back_translation: Option<bool>,
}

/// A difference found between the source and the back-translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BackTranslationIssue {
    /// "negation", "glossary_term", "missing_sentence", or "sentence_count".
    pub kind: String,
    pub detail: String,
    /// 1-based source sentence, when one applies.
    pub source_sentence: Option<usize>,
}

/// Result of `translate_health_material`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TranslationResult {
    pub language: Option<String>,
    pub language_tag: Option<String>,
    pub script_note: Option<String>,
    pub town: Option<String>,
    pub language_priorities: Vec<LanguagePriority>,
    /// Agency translations found in /data/templates/<language>/ (count by language).
    pub agency_translations: BTreeMap<String, usize>,
    /// The requested material already has an agency translation in that language.
    pub agency_translation_found: Option<String>,
    /// Numbered source segments for the translator.
    pub segments: Vec<String>,
    pub glossary_terms: Vec<GlossaryUse>,
    pub invariants: Vec<InvariantCheck>,
    pub back_translation_issues: Vec<BackTranslationIssue>,
    pub draft_label: String,
    pub workflow: Vec<String>,
    pub pitfalls: Vec<String>,
    pub worksheet_file: Option<String>,
}

fn source_title(key: &str) -> String {
    CONTENT
        .sources
        .get(key)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| key.to_string())
}

fn read_text(ctx: &Context, path: &str) -> Result<(String, String)> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let label = sandbox.display_path(&p);
    let e = phn_local::extract::extract(&p)?;
    if e.kind.is_tabular() {
        return Err(PhnError::Unsupported(format!(
            "{label} is a spreadsheet; give a document"
        )));
    }
    Ok((e.text, label))
}

fn pick(
    ctx: &Context,
    text: &Option<String>,
    path: &Option<String>,
    what: &str,
    sources: &mut Vec<Source>,
) -> Result<Option<String>> {
    match (
        text.as_deref().map(str::trim).filter(|t| !t.is_empty()),
        path.as_deref().map(str::trim).filter(|t| !t.is_empty()),
    ) {
        (Some(_), Some(_)) => Err(PhnError::InvalidInput(format!(
            "give the {what} as text or as a path, not both"
        ))),
        (Some(t), None) => Ok(Some(t.to_string())),
        (None, Some(p)) => {
            let (t, label) = read_text(ctx, p)?;
            sources.push(Source::local_file(&label));
            Ok(Some(t))
        }
        (None, None) => Ok(None),
    }
}

/// Glossary: English term -> translation for the language.
fn load_glossary(ctx: &Context, path: &str, lang: Option<&Language>) -> Result<Glossary> {
    let (t, label) = crate::core::files::load_table(ctx, path, None)?;
    let norm = |s: &str| lang_key(s);
    let col = |name: &str| t.headers.iter().position(|h| norm(h) == name);
    let eng = col("english")
        .or_else(|| col("en"))
        .or_else(|| col("term"))
        .ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "{label} needs an 'english' column (columns: {})",
                t.headers.join(", ")
            ))
        })?;
    let mut out = Vec::new();
    let long = col("language").zip(col("translation"));
    for row in &t.rows {
        let e = row
            .get(eng)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if e.is_empty() {
            continue;
        }
        let tr = match (lang, long) {
            (Some(l), Some((lc, tc))) => {
                let rl = row.get(lc).map(|s| s.trim()).unwrap_or_default();
                if resolve_language(rl).is_ok_and(|x| x.0 == l.0) {
                    row.get(tc)
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                } else {
                    continue;
                }
            }
            (Some(l), None) => t
                .headers
                .iter()
                .position(|h| resolve_language(h).is_ok_and(|x| x.0 == l.0))
                .and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            (None, _) => None,
        };
        out.push((e, tr));
    }
    out.sort_by_key(|g| std::cmp::Reverse(g.0.len()));
    // Keep the first row of each English term (duplicates need not be adjacent).
    let mut seen: Vec<String> = Vec::new();
    out.retain(|g| {
        let key = g.0.to_lowercase();
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    Ok((out, label))
}

/// Scripts written without spaces between words (Chinese, Japanese, Korean
/// particles, Thai, Lao, Khmer, Burmese), where a term cannot be matched as a
/// whole word.
fn unspaced_script(c: char) -> bool {
    matches!(
        c as u32,
        0x0E00..=0x0EFF
            | 0x1000..=0x109F
            | 0x1780..=0x17FF
            | 0x3040..=0x30FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
    )
}

/// Times `needle` appears in `hay`, ignoring case. A term matches as a whole
/// word, with an optional plural ending ("vaccine" finds "vaccines", "vacuna"
/// finds "vacunas"); terms in scripts without word spacing match anywhere.
fn count_ci(hay: &str, needle: &str) -> usize {
    let n = needle.trim().to_lowercase();
    if n.is_empty() {
        return 0;
    }
    let h = hay.to_lowercase();
    if n.chars().any(unspaced_script) {
        return h.matches(n.as_str()).count();
    }
    let boundary = |s: &str| s.chars().next().is_none_or(|c| !c.is_alphanumeric());
    let mut count = 0;
    let mut from = 0;
    while let Some(pos) = h[from..].find(n.as_str()) {
        let start = from + pos;
        let end = start + n.len();
        let before_ok = h[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after = &h[end..];
        let plural = if boundary(after) {
            Some(0)
        } else if after.strip_prefix("es").is_some_and(boundary) {
            Some(2)
        } else if after.strip_prefix('s').is_some_and(boundary) {
            Some(1)
        } else {
            None
        };
        match plural {
            Some(extra) if before_ok => {
                count += 1;
                from = end + extra;
            }
            _ => {
                // Step past the first character of this occurrence.
                from = start + h[start..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    count
}

static INVARIANT: LazyLock<Vec<(&'static str, regex::Regex)>> = LazyLock::new(|| {
    [
        ("web_address", r"(?i)\b(?:https?://|www\.)[^\s)]+"),
        ("email", r"(?i)\b[\w.+-]+@[\w-]+\.[\w.]+\b"),
        (
            "phone",
            r"\(?\b\d{3}\)?[-. ]\d{3}[-. ]\d{4}\b|\b\d-\d{3}-\d{3}-\d{4}\b|\b988\b",
        ),
        ("time", r"(?i)\b\d{1,2}:\d{2}\b"),
        ("number", r"\b\d+(?:[.,]\d+)?\b"),
    ]
    .iter()
    .map(|(k, p)| (*k, regex::Regex::new(p).expect("valid regex")))
    .collect()
});

fn invariants(source: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for (kind, re) in INVARIANT.iter() {
        for m in re.find_iter(source) {
            if taken.iter().any(|(s, e)| m.start() < *e && m.end() > *s) {
                continue;
            }
            taken.push((m.start(), m.end()));
            let v = m.as_str().trim_end_matches(['.', ',']).to_string();
            if !out.iter().any(|(k, x)| k == kind && *x == v) {
                out.push((kind.to_string(), v));
            }
        }
    }
    out
}

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

fn contains_invariant(hay: &str, kind: &str, v: &str) -> bool {
    match kind {
        // Numbers and phones may be formatted differently (1,000 vs 1.000; dashes vs dots).
        "number" | "phone" | "time" => {
            let d = digits(v);
            if d.is_empty() {
                return false;
            }
            // A number is one token even with separators inside it, so "1,000",
            // "1.000", "3.5", "3,5", and "9:30" compare by their digits.
            static NUMBER_TOKEN: LazyLock<regex::Regex> = LazyLock::new(|| {
                regex::Regex::new(r"\d+(?:[.,:]\d+)*").expect("valid regex")
            });
            if NUMBER_TOKEN
                .find_iter(hay)
                .any(|m| digits(m.as_str()) == d)
            {
                return true;
            }
            let all: String = hay
                .chars()
                .map(|c| if c.is_ascii_digit() { c } else { ' ' })
                .collect();
            let nospace: String = hay
                .chars()
                .filter(|c| {
                    c.is_ascii_digit()
                        || c.is_whitespace()
                        || *c == '-'
                        || *c == '.'
                        || *c == ','
                        || *c == '('
                        || *c == ')'
                })
                .collect::<String>();
            all.split_whitespace().any(|t| t == d) || digits(&nospace).contains(&d) && d.len() >= 7
        }
        _ => hay.to_lowercase().contains(&v.to_lowercase()),
    }
}

const NEGATIONS: &[&str] = &[
    "not",
    "no",
    "never",
    "don't",
    "doesn't",
    "didn't",
    "won't",
    "can't",
    "cannot",
    "shouldn't",
    "isn't",
    "aren't",
    "without",
    "none",
    "nobody",
    "avoid",
];

fn negations(s: &str) -> usize {
    textstats::words(s)
        .iter()
        .filter(|w| NEGATIONS.contains(&w.text.to_lowercase().replace('’', "'").as_str()))
        .count()
}

fn sentences(text: &str) -> Vec<String> {
    textstats::parse(text)
        .blocks
        .iter()
        .flat_map(|b| b.sentences.iter().map(|s| s.text.clone()))
        .collect()
}

fn word_set(s: &str) -> BTreeSet<String> {
    textstats::words(s)
        .iter()
        .map(|w| w.text.to_lowercase())
        .filter(|w| w.len() > 3)
        .collect()
}

// ---------------------------------------------------------------- shared with EPR-10

/// The town's language priorities from EQ-03: the PUMA's detailed languages with
/// limited-English speakers (largest first, at most 8), or the town's language
/// groups when no detailed rows exist. Returns the rows and the ACS citation, or
/// a caveat explaining why they could not be computed.
pub async fn language_priorities(
    ctx: &Context,
    town: &str,
) -> std::result::Result<(Vec<LanguagePriority>, Vec<Source>), String> {
    let env = language_access_profile(
        ctx,
        LanguageAccessArgs {
            area: Some(town.to_string()),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| format!("Language priorities for {town} could not be computed: {e}"))?;
    let mut rows: Vec<LanguagePriority> = env
        .data
        .puma_languages
        .iter()
        .filter(|r| r.lep.value > 0.0)
        .map(|r| LanguagePriority {
            language: r.language.clone(),
            lep_estimate: r.lep.value.round(),
            lep_moe: r.lep.moe.map(f64::round),
            geography: format!("PUMA(s) {}", env.data.pumas.join(", ")),
        })
        .collect();
    rows.sort_by(|a, b| b.lep_estimate.total_cmp(&a.lep_estimate));
    rows.truncate(8);
    if rows.is_empty() {
        rows = env
            .data
            .groups
            .iter()
            .map(|g| LanguagePriority {
                language: g.language_group.clone(),
                lep_estimate: g.lep.value.round(),
                lep_moe: g.lep.moe.map(f64::round),
                geography: env.data.area.clone(),
            })
            .collect();
    }
    Ok((rows, env.sources.into_iter().take(1).collect()))
}

/// A translator brief for one language: numbered English segments, items that
/// must carry over unchanged, the draft label, and the translation workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TranslatorBrief {
    /// Language as recognized (or as given when the tool does not know it).
    pub language: String,
    /// BCP 47 tag, when known.
    pub language_tag: Option<String>,
    pub script_note: Option<String>,
    /// Numbered English segments to translate.
    pub segments: Vec<String>,
    /// Numbers, phone numbers, web addresses, emails, and times that must not change.
    pub keep_unchanged: Vec<InvariantCheck>,
    pub draft_label: String,
    pub workflow: Vec<String>,
}

/// Build a translator brief for an English source (text supplied in the
/// request, not read from a file). Never translates.
pub fn translator_brief(language: &str, english: &str) -> TranslatorBrief {
    let c = &*CONTENT;
    let known = resolve_language(language).ok();
    TranslatorBrief {
        language: known
            .map(|l| l.1.to_string())
            .unwrap_or_else(|| language.trim().to_string()),
        language_tag: known.map(|l| l.2.to_string()),
        script_note: known.and_then(|l| l.4.map(str::to_string)),
        segments: sentences(english),
        keep_unchanged: invariants(english)
            .into_iter()
            .map(|(kind, value)| InvariantCheck {
                kind,
                value,
                in_translation: None,
                in_back_translation: None,
            })
            .collect(),
        draft_label: c.draft_label.clone(),
        workflow: c
            .workflow
            .iter()
            .map(|w| format!("{} ({}, {})", w.text, source_title(&w.source), w.section))
            .collect(),
    }
}

/// The translation workflow's citations (for tools that build briefs).
pub fn translation_sources() -> Vec<Source> {
    let c = &*CONTENT;
    let mut keys: Vec<&str> = c.workflow.iter().map(|w| w.source.as_str()).collect();
    keys.sort();
    keys.dedup();
    keys.iter()
        .filter_map(|k| c.sources.get(*k))
        .map(|s| s.to_source(&c.meta.prepared))
        .collect()
}

/// Run the translation workflow checks.
pub async fn translate_health_material(
    ctx: &Context,
    args: TranslationArgs,
) -> Result<Envelope<TranslationResult>> {
    let c = &*CONTENT;
    let lang = args
        .language
        .as_deref()
        .filter(|l| !l.trim().is_empty())
        .map(resolve_language)
        .transpose()?;
    let mut sources: Vec<Source> = Vec::new();
    let source_from_file = args
        .source_text
        .as_deref()
        .is_none_or(|t| t.trim().is_empty())
        && args
            .source_path
            .as_deref()
            .is_some_and(|p| !p.trim().is_empty());
    let source = pick(
        ctx,
        &args.source_text,
        &args.source_path,
        "source",
        &mut sources,
    )?;
    let redact = |t: String| -> String {
        if source_from_file {
            phn_local::phi::redact_text(&t, phn_local::phi::RedactionLevel::Strict).text
        } else {
            t
        }
    };
    let translation = pick(
        ctx,
        &args.translation_text,
        &args.translation_path,
        "translation",
        &mut sources,
    )?;
    let back = pick(
        ctx,
        &args.back_translation_text,
        &args.back_translation_path,
        "back-translation",
        &mut sources,
    )?;
    if (translation.is_some() || back.is_some()) && source.is_none() {
        return Err(PhnError::InvalidInput(
            "give the English source too, so the draft can be compared with it".into(),
        ));
    }
    if translation.is_some() && lang.is_none() {
        return Err(PhnError::InvalidInput(
            "give the language of the translated draft".into(),
        ));
    }
    let mut caveats = Vec::new();

    // ---- language priorities (EQ-03)
    let town = args
        .town
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| ctx.settings().jurisdiction.town);
    let mut priorities = Vec::new();
    if let Some(t) = &town {
        match language_priorities(ctx, t).await {
            Ok((rows, src)) => {
                priorities = rows;
                sources.extend(src);
            }
            Err(e) => caveats.push(e),
        }
    }

    // ---- agency translations in /data/templates/<language>/
    let mut agency = BTreeMap::new();
    let mut agency_found = None;
    if let Ok(sb) = ctx.sandbox() {
        let dir = sb.root().join("templates");
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                if !e.path().is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_string();
                let files: Vec<String> = std::fs::read_dir(e.path())
                    .map(|r| {
                        r.flatten()
                            .filter(|f| f.path().is_file())
                            .map(|f| f.file_name().to_string_lossy().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let key = resolve_language(&name)
                    .map(|l| l.1.to_string())
                    .unwrap_or(name.clone());
                if let (Some(l), Some(m)) = (lang, args.material_name.as_deref())
                    && resolve_language(&name).is_ok_and(|x| x.0 == l.0)
                    && let Some(f) = files.iter().find(|f| {
                        lang_key(f.rsplit_once('.').map_or(f.as_str(), |x| x.0)) == lang_key(m)
                    })
                {
                    agency_found = Some(format!("templates/{name}/{f}"));
                }
                *agency.entry(key).or_insert(0) += files.len();
            }
        }
    }

    // ---- source analysis
    let src_sentences = source.as_deref().map(sentences).unwrap_or_default();
    let mut glossary_terms: Vec<GlossaryUse> = Vec::new();
    if let Some(gp) = args
        .glossary_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        let (gl, label) = load_glossary(ctx, gp, lang)?;
        sources.push(Source::local_file(&label));
        if let Some(src) = &source {
            // Longest terms come first; a shorter term is not counted again where
            // it is part of a longer glossary term ("flu" inside "flu shot").
            for (e, tr) in &gl {
                let inside_longer: usize = glossary_terms
                    .iter()
                    .map(|g| g.times_in_source * count_ci(&g.english, e))
                    .sum();
                let n = count_ci(src, e).saturating_sub(inside_longer);
                if n == 0 {
                    continue;
                }
                let in_tr = match (&translation, tr) {
                    (Some(t), Some(a)) => {
                        let inside: usize = glossary_terms
                            .iter()
                            .map(|g| match (g.times_in_translation, g.approved.as_deref()) {
                                (Some(times), Some(longer)) => times * count_ci(longer, a),
                                _ => 0,
                            })
                            .sum();
                        Some(count_ci(t, a).saturating_sub(inside))
                    }
                    _ => None,
                };
                glossary_terms.push(GlossaryUse {
                    english: e.clone(),
                    approved: tr.clone(),
                    times_in_source: n,
                    times_in_translation: in_tr,
                    missing_in_translation: in_tr == Some(0),
                });
            }
        }
        if lang.is_some() && gl.iter().all(|(_, t)| t.is_none()) {
            caveats.push(format!(
                "{label} has no translations for {}; add a column for it.",
                lang.map(|l| l.1).unwrap_or_default()
            ));
        }
    }
    let invs: Vec<InvariantCheck> = source
        .as_deref()
        .map(invariants)
        .unwrap_or_default()
        .into_iter()
        .map(|(kind, value)| InvariantCheck {
            in_translation: translation
                .as_deref()
                .map(|t| contains_invariant(t, &kind, &value)),
            in_back_translation: back
                .as_deref()
                .map(|b| contains_invariant(b, &kind, &value)),
            value: if kind != "number" {
                redact(value.clone())
            } else if source_from_file && digits(&value).len() > 4 {
                "[number withheld: long numbers from files may be identifiers]".into()
            } else {
                value.clone()
            },
            kind,
        })
        .collect();

    // ---- back-translation comparison
    let mut issues = Vec::new();
    if let (Some(src), Some(bt)) = (&source, &back) {
        let bt_sentences = sentences(bt);
        if bt_sentences.len() != src_sentences.len() {
            issues.push(BackTranslationIssue {
                kind: "sentence_count".into(),
                detail: format!(
                    "the source has {} sentences and the back-translation {}",
                    src_sentences.len(),
                    bt_sentences.len()
                ),
                source_sentence: None,
            });
        }
        let bt_sets: Vec<BTreeSet<String>> = bt_sentences.iter().map(|s| word_set(s)).collect();
        for (i, s) in src_sentences.iter().enumerate() {
            let ws = word_set(s);
            if ws.len() < 3 {
                continue;
            }
            let best = bt_sets
                .iter()
                .map(|b| ws.intersection(b).count() as f64 / ws.len() as f64)
                .fold(0.0, f64::max);
            if best < 0.3 {
                issues.push(BackTranslationIssue {
                    kind: "missing_sentence".into(),
                    detail: format!("no sentence in the back-translation shares most words with source sentence {}", i + 1),
                    source_sentence: Some(i + 1),
                });
            }
        }
        let (ns, nb) = (negations(src), negations(bt));
        if ns != nb {
            issues.push(BackTranslationIssue {
                kind: "negation".into(),
                detail: format!("the source has {ns} negation(s) (not, no, never, avoid, ...) and the back-translation {nb}; check that no instruction was reversed"),
                source_sentence: None,
            });
        }
        for g in &glossary_terms {
            if count_ci(bt, &g.english) == 0 {
                issues.push(BackTranslationIssue {
                    kind: "glossary_term".into(),
                    detail: format!(
                        "the glossary term \"{}\" does not come back in the back-translation",
                        g.english
                    ),
                    source_sentence: None,
                });
            }
        }
    }

    // ---- worksheet
    let mut privacy = Privacy::none();
    let mut worksheet_file = None;
    if let Some(fmt) = args
        .worksheet
        .as_deref()
        .map(|f| f.trim().to_lowercase())
        .filter(|f| !f.is_empty() && f != "none")
    {
        if fmt != "xlsx" {
            return Err(PhnError::InvalidInput("worksheet must be \"xlsx\"".into()));
        }
        if src_sentences.is_empty() {
            return Err(PhnError::InvalidInput(
                "give the English source to build a worksheet".into(),
            ));
        }
        let tr_sents = translation.as_deref().map(sentences).unwrap_or_default();
        let bt_sents = back.as_deref().map(sentences).unwrap_or_default();
        let rows: Vec<Vec<String>> = src_sentences
            .iter()
            .enumerate()
            .map(|(i, s)| {
                vec![
                    (i + 1).to_string(),
                    s.clone(),
                    tr_sents.get(i).cloned().unwrap_or_default(),
                    bt_sents.get(i).cloned().unwrap_or_default(),
                    String::new(),
                    String::new(),
                ]
            })
            .collect();
        let doc = Document {
            title: format!(
                "Translation review worksheet{}",
                lang.map(|l| format!(": {}", l.1)).unwrap_or_default()
            ),
            subtitle: Some(c.draft_label.clone()),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                Block::Table(TableBlock {
                    caption: Some("Segments".into()),
                    headers: [
                        "#",
                        "English source",
                        "Translation (draft)",
                        "Back-translation",
                        "Reviewer comments",
                        "Approved (initials, date)",
                    ]
                    .map(String::from)
                    .to_vec(),
                    rows,
                }),
                Block::Table(TableBlock {
                    caption: Some("Glossary terms".into()),
                    headers: ["English", "Approved translation", "In source", "In draft"]
                        .map(String::from)
                        .to_vec(),
                    rows: glossary_terms
                        .iter()
                        .map(|g| {
                            vec![
                                g.english.clone(),
                                g.approved.clone().unwrap_or_default(),
                                g.times_in_source.to_string(),
                                g.times_in_translation
                                    .map(|n| n.to_string())
                                    .unwrap_or_default(),
                            ]
                        })
                        .collect(),
                }),
            ],
        };
        let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
        let sb = ctx.sandbox()?;
        let path = sb.new_output_file(&format!(
            "translation_review_{}.xlsx",
            lang.map(|l| l.0).unwrap_or("draft")
        ))?;
        std::fs::write(&path, bytes)?;
        let disp = sb.display_path(&path);
        privacy.files_written.push(disp.clone());
        worksheet_file = Some(disp);
    }

    // ---- summary
    let mut summary = format!(
        "**Translation workflow{}**\n\n",
        lang.map(|l| format!(": English to {}", l.1))
            .unwrap_or_default()
    );
    summary.push_str(&format!(
        "Label every machine or staff draft: _{}_\n\n",
        c.draft_label
    ));
    if let Some(t) = &town {
        if priorities.is_empty() {
            summary.push_str(&format!("Language priorities for {t}: not available.\n\n"));
        } else {
            summary.push_str(&format!(
                "**Languages with the most residents who speak English less than very well near {t}** (ACS): {}\n\n",
                priorities.iter().map(|p| format!("{} (about {:.0})", p.language, p.lep_estimate)).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    if agency.is_empty() {
        summary.push_str("No agency translations were found in /data/templates/<language>/.\n");
    } else {
        summary.push_str(&format!(
            "Agency translations in /data/templates: {}.\n",
            agency
                .iter()
                .map(|(k, n)| format!("{k} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(f) = &agency_found {
        summary.push_str(&format!("**An agency translation of this material already exists: `{f}`.** Use it instead of a new draft.\n"));
    }
    if let Some(l) = lang
        && let Some(n) = l.4
    {
        summary.push_str(&format!("{n}\n"));
    }
    if !src_sentences.is_empty() {
        summary.push_str(&format!(
            "\nSource: {} segment(s) for the translator.\n",
            src_sentences.len()
        ));
    }
    if !glossary_terms.is_empty() {
        summary.push_str(&format!(
            "\n**Glossary terms in the source:** {}\n",
            glossary_terms
                .iter()
                .map(|g| format!(
                    "{}{}{}",
                    g.english,
                    g.approved
                        .as_ref()
                        .map(|a| format!(" → {a}"))
                        .unwrap_or_default(),
                    if g.missing_in_translation {
                        " **(not in the draft)**"
                    } else {
                        ""
                    }
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let lost_tr: Vec<&InvariantCheck> = invs
        .iter()
        .filter(|i| i.in_translation == Some(false))
        .collect();
    let lost_bt: Vec<&InvariantCheck> = invs
        .iter()
        .filter(|i| i.in_back_translation == Some(false))
        .collect();
    if !invs.is_empty() {
        summary.push_str(&format!(
            "\n**Must carry over unchanged:** {}\n",
            invs.iter()
                .map(|i| i.value.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !lost_tr.is_empty() {
        summary.push_str(&format!(
            "- Not found in the translated draft: {}\n",
            lost_tr
                .iter()
                .map(|i| i.value.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !lost_bt.is_empty() {
        summary.push_str(&format!(
            "- Not found in the back-translation: {}\n",
            lost_bt
                .iter()
                .map(|i| i.value.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if back.is_some() {
        if issues.is_empty() {
            summary.push_str("\nBack-translation check: no differences found in numbers, negations, glossary terms, or sentences. A qualified reviewer still needs to compare meaning and tone.\n");
        } else {
            summary.push_str("\n**Back-translation check**\n");
            for i in &issues {
                summary.push_str(&format!("- {}\n", i.detail));
            }
        }
    }
    summary.push_str("\n**Workflow (CDC Simply Put)**\n");
    for (i, w) in c.workflow.iter().enumerate() {
        summary.push_str(&format!("{}. {}\n", i + 1, w.text));
    }
    if let Some(w) = &worksheet_file {
        summary.push_str(&format!("\nReview worksheet written to `{w}`.\n"));
    }

    sources.extend(content::cite(&c.sources, &["simply_put"], &c.meta.prepared));
    caveats.push("No translation is bundled or produced by this server. Any draft (including one written by the AI assistant) must be reviewed and approved by a qualified translator before use.".into());
    caveats.push("The checks compare numbers, phone numbers, web addresses, negations, glossary terms, and sentence coverage; they cannot judge meaning, tone, or cultural fit.".into());
    caveats.extend(content::review_caveats(&[TRANSLATION]));
    Ok(Envelope::new(
        summary,
        TranslationResult {
            language: lang.map(|l| l.1.to_string()),
            language_tag: lang.map(|l| l.2.to_string()),
            script_note: lang.and_then(|l| l.4.map(str::to_string)),
            town,
            language_priorities: priorities,
            agency_translations: agency,
            agency_translation_found: agency_found,
            segments: src_sentences.into_iter().map(&redact).collect(),
            glossary_terms,
            invariants: invs,
            back_translation_issues: issues,
            draft_label: c.draft_label.clone(),
            workflow: c
                .workflow
                .iter()
                .map(|w| format!("{} ({}, {})", w.text, source_title(&w.source), w.section))
                .collect(),
            pitfalls: c
                .pitfalls
                .iter()
                .map(|p| format!("{} ({}, {})", p.text, source_title(&p.source), p.section))
                .collect(),
            worksheet_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_resolve_by_name_code_and_alias() {
        assert_eq!(resolve_language("Spanish").unwrap().2, "es-US");
        assert_eq!(resolve_language("kreyol").unwrap().0, "haitian_creole");
        assert_eq!(resolve_language("pt-BR").unwrap().0, "portuguese");
        assert!(resolve_language("Klingon").is_err());
    }

    #[test]
    fn invariants_and_negations() {
        let inv = invariants("Call 617-555-0123 or visit www.mass.gov/flu by 5:30. Bring 2 forms.");
        let kinds: Vec<&str> = inv.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            kinds.contains(&"phone")
                && kinds.contains(&"web_address")
                && kinds.contains(&"time")
                && kinds.contains(&"number")
        );
        assert!(contains_invariant(
            "Llame al 617.555.0123",
            "phone",
            "617-555-0123"
        ));
        assert!(!contains_invariant("Traiga 3 formularios", "number", "2"));
        assert_eq!(negations("Do not share. Never use it without help."), 3);
    }

    #[test]
    fn numbers_with_separators_and_times_carry_over() {
        // Unchanged in the draft: found.
        assert!(contains_invariant("Hasta 1,000 personas", "number", "1,000"));
        assert!(contains_invariant("Tome 3.5 mg", "number", "3.5"));
        assert!(contains_invariant("Fiebre de 100.4 grados", "number", "100.4"));
        assert!(contains_invariant("Llegue a las 9:30", "time", "9:30"));
        // Local number formats still match by their digits.
        assert!(contains_invariant("Hasta 1.000 personas", "number", "1,000"));
        assert!(contains_invariant("Tome 3,5 mg", "number", "3.5"));
        // A changed value is still reported.
        assert!(!contains_invariant("Tome 5.3 mg", "number", "3.5"));
        assert!(!contains_invariant("Llegue a las 9:00", "time", "9:30"));
        assert!(!contains_invariant("Hasta 100 personas", "number", "1,000"));
    }

    #[test]
    fn glossary_terms_match_plurals_and_unspaced_scripts() {
        // Whole words with a plural ending; not inside other words.
        assert_eq!(count_ci("Vaccines work. Get a vaccine.", "vaccine"), 2);
        assert_eq!(count_ci("Las vacunas son seguras", "vacuna"), 1);
        assert_eq!(count_ci("The influenza season", "flu"), 0);
        assert_eq!(count_ci("flu, flu and FLU", "flu"), 3);
        // Chinese has no spaces between words: the approved term is found inside the sentence.
        assert_eq!(count_ci("请接种流感疫苗以保护家人", "疫苗"), 1);
        assert_eq!(count_ci("请接种流感疫苗以保护家人", "口罩"), 0);
        // Terms contained in a longer term are counted there.
        assert_eq!(count_ci("flu shot", "flu"), 1);
        assert_eq!(count_ci("", "flu"), 0);
        assert_eq!(count_ci("flu", "  "), 0);
    }
}
