//! Letter and plan templates with local fill and agency translations.
//!
//! The server ships English templates only (Markdown with `{{placeholders}}`).
//! An agency can add its own approved translations as
//! `/data/templates/<language>/<kind>.md`, where `<language>` is the language
//! name or code used in the nurse's files (for example `spanish`, `es`,
//! `portuguese`, `pt`, `haitian-creole`, `ht`). When a family's language has
//! no agency translation, the English template is used and the fallback is
//! counted. Machine-drafted translations are never produced.
//!
//! Filled documents are written to the output folder only; tools return
//! counts and generic file names, never the filled values.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use phn_core::{PhnError, Result};
use phn_docs::model::{Document, from_markdown};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// A template kind the server knows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateInfo {
    /// File name stem: `/data/templates/<language>/<kind>.md`.
    pub kind: String,
    pub description: String,
    /// Placeholders the tool fills, written as `{{name}}` in the template.
    pub placeholders: Vec<String>,
}

/// Language codes and names used to match template folders.
const LANGUAGES: &[(&str, &str, &[&str])] = &[
    ("en", "en-US", &["english"]),
    (
        "es",
        "es-US",
        &["spanish", "espanol", "español", "castellano"],
    ),
    (
        "pt",
        "pt-BR",
        &[
            "portuguese",
            "portugues",
            "português",
            "brazilian portuguese",
        ],
    ),
    (
        "ht",
        "ht",
        &[
            "haitian creole",
            "haitian-creole",
            "creole",
            "kreyol",
            "kreyòl",
        ],
    ),
    (
        "kea",
        "kea",
        &["cape verdean", "cape verdean creole", "kriolu"],
    ),
    ("zh", "zh", &["chinese", "mandarin", "cantonese"]),
    ("vi", "vi", &["vietnamese"]),
    ("ar", "ar", &["arabic"]),
    ("ru", "ru", &["russian"]),
    ("km", "km", &["khmer", "cambodian"]),
    ("fr", "fr", &["french"]),
    ("so", "so", &["somali"]),
    ("ne", "ne", &["nepali"]),
    ("sw", "sw", &["swahili"]),
    ("ko", "ko", &["korean"]),
    ("ja", "ja", &["japanese"]),
    ("hi", "hi", &["hindi"]),
    ("tl", "tl", &["tagalog", "filipino"]),
    ("pl", "pl", &["polish"]),
    ("it", "it", &["italian"]),
    ("el", "el", &["greek"]),
    ("tr", "tr", &["turkish"]),
    ("uk", "uk", &["ukrainian"]),
    ("dari", "prs", &["dari"]),
    ("ps", "ps", &["pashto"]),
];

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Folder names that may hold templates for a language value from a file.
fn candidates(language: &str) -> Vec<String> {
    let s = slug(language);
    let mut out = vec![s.clone()];
    let base = s.split('-').next().unwrap_or("").to_string();
    for (code, _, names) in LANGUAGES {
        let name_slugs: Vec<String> = names.iter().map(|n| slug(n)).collect();
        if s == *code || base == *code || name_slugs.contains(&s) {
            out.push((*code).to_string());
            out.extend(name_slugs);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// True for English (or blank) language values.
pub fn is_english(language: &str) -> bool {
    let s = slug(language);
    s.is_empty() || s == "en" || s.starts_with("en-") || s == "english"
}

/// A language tag for the document (BCP 47), when known.
fn bcp47(language: &str) -> Option<&'static str> {
    let c = candidates(language);
    LANGUAGES
        .iter()
        .find(|(code, _, _)| c.iter().any(|x| x == code))
        .map(|(_, tag, _)| *tag)
}

/// A display label for a language value ("Spanish", "English").
pub fn language_label(language: &str) -> String {
    if is_english(language) {
        return "English".into();
    }
    let c = candidates(language);
    for (code, _, names) in LANGUAGES {
        if c.iter().any(|x| x == code) {
            let n = names[0];
            let mut ch = n.chars();
            return match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                None => (*code).to_string(),
            };
        }
    }
    language.trim().to_string()
}

/// Agency template folders found in /data/templates (folder name -> path).
fn template_folders(ctx: &Context) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    let Ok(sandbox) = ctx.sandbox() else {
        return out;
    };
    let Ok(dir) = sandbox.resolve_existing("templates") else {
        return out;
    };
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                out.insert(slug(&e.file_name().to_string_lossy()), e.path());
            }
        }
    }
    out
}

/// Templates resolved for one document kind across languages.
pub struct TemplateResolver<'a> {
    kind: &'a str,
    english: String,
    folders: BTreeMap<String, PathBuf>,
    cache: BTreeMap<String, Option<String>>,
}

/// Which template a letter used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// Markdown with placeholders.
    pub markdown: String,
    /// Display label of the family's language.
    pub language: String,
    /// True when an agency translation was used.
    pub translated: bool,
    /// BCP 47 tag of the document language.
    pub tag: String,
}

impl<'a> TemplateResolver<'a> {
    /// Resolver for a kind with its bundled English template.
    pub fn new(ctx: &Context, kind: &'a str, english: String) -> Self {
        Self {
            kind,
            english,
            folders: template_folders(ctx),
            cache: BTreeMap::new(),
        }
    }

    fn read_translation(&self, language: &str) -> Option<String> {
        for c in candidates(language) {
            if let Some(dir) = self.folders.get(&c) {
                for ext in ["md", "txt", "markdown"] {
                    let p: PathBuf = dir.join(format!("{}.{ext}", self.kind));
                    if p.is_file()
                        && let Ok(bytes) = std::fs::read(&p)
                    {
                        let t = phn_local::table::decode_text(&bytes);
                        if !t.trim().is_empty() {
                            return Some(t);
                        }
                    }
                }
            }
        }
        None
    }

    /// The template for a family's language (agency translation or English).
    pub fn choose(&mut self, language: &str) -> Chosen {
        let label = language_label(language);
        if is_english(language) {
            return Chosen {
                markdown: self.english.clone(),
                language: label,
                translated: false,
                tag: "en-US".into(),
            };
        }
        let key = slug(language);
        if !self.cache.contains_key(&key) {
            let t = self.read_translation(language);
            self.cache.insert(key.clone(), t);
        }
        match self.cache.get(&key).cloned().flatten() {
            Some(md) => Chosen {
                markdown: md,
                language: label,
                translated: true,
                tag: bcp47(language).unwrap_or("en-US").to_string(),
            },
            None => Chosen {
                markdown: self.english.clone(),
                language: label,
                translated: false,
                tag: "en-US".into(),
            },
        }
    }
}

/// Make a value safe to place in Markdown (one line, no table pipes).
fn clean(v: &str) -> String {
    let one: String = v
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    one.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "/")
        .replace("{{", "")
        .replace("}}", "")
}

fn key(s: &str) -> String {
    phn_core::text::normalize(s).replace(' ', "_")
}

/// Fill `{{placeholders}}`. Unknown placeholders are left as `[name]` and
/// returned; blank values become `blank` (e.g. "________").
pub fn fill(
    markdown: &str,
    values: &BTreeMap<String, String>,
    blank: &str,
) -> (String, Vec<String>) {
    let norm: BTreeMap<String, &String> = values.iter().map(|(k, v)| (key(k), v)).collect();
    let mut out = String::with_capacity(markdown.len() + 256);
    let mut unknown = BTreeSet::new();
    let mut rest = markdown;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let name = after[..end].trim();
        match norm.get(&key(name)) {
            Some(v) if !v.trim().is_empty() => out.push_str(&clean(v)),
            Some(_) => out.push_str(blank),
            None => {
                unknown.insert(name.to_string());
                out.push_str(&format!("[{name}]"));
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    (out, unknown.into_iter().collect())
}

/// Build a document from filled Markdown. The first `# heading` becomes the title.
pub fn document(ctx: &Context, markdown: &str, tag: &str, fallback_title: &str) -> Document {
    let mut blocks = from_markdown(markdown);
    let mut title = fallback_title.to_string();
    if let Some(phn_docs::model::Block::Heading { level: 1, text }) = blocks.first() {
        title = text.clone();
        blocks.remove(0);
    }
    Document {
        title,
        subtitle: None,
        language: tag.to_string(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

/// Output document format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocFormat {
    /// Word document (default; supports every script).
    #[default]
    Docx,
    /// PDF (scripts outside the bundled font show as '?').
    Pdf,
}

impl DocFormat {
    pub fn ext(self) -> &'static str {
        match self {
            DocFormat::Docx => "docx",
            DocFormat::Pdf => "pdf",
        }
    }

    /// Render a document.
    pub fn render(self, doc: &Document) -> Result<Vec<u8>> {
        match self {
            DocFormat::Docx => phn_docs::docx::to_docx(doc),
            DocFormat::Pdf => phn_docs::pdf::to_pdf(doc),
        }
    }
}

/// Combine documents into one with page breaks (one per family or student).
pub fn combine(title: &str, tag: &str, docs: Vec<Document>, ctx: &Context) -> Document {
    let mut blocks = Vec::new();
    for (i, d) in docs.into_iter().enumerate() {
        if i > 0 {
            blocks.push(phn_docs::model::Block::PageBreak);
        }
        blocks.push(phn_docs::model::Block::Heading {
            level: 1,
            text: d.title.clone(),
        });
        blocks.extend(d.blocks);
    }
    Document {
        title: title.to_string(),
        subtitle: None,
        language: tag.to_string(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

/// A new folder in the output directory.
pub fn output_folder(ctx: &Context, name: &str) -> Result<(PathBuf, String)> {
    let sandbox = ctx.sandbox()?;
    let dir = sandbox.new_output_file(name)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| PhnError::Config(format!("cannot create {}: {e}", dir.display())))?;
    let shown = sandbox.display_path(&dir);
    Ok((dir, shown))
}

/// Write bytes to a new file in a folder and return its display path. An
/// existing file is never overwritten: a numeric suffix is added instead
/// ("letters_spanish-2.docx"), as the sandbox does for other output files.
pub fn write_in(ctx: &Context, dir: &Path, file: &str, bytes: &[u8]) -> Result<String> {
    use std::io::Write;
    let sandbox = ctx.sandbox()?;
    let clean = phn_local::sandbox::sanitize_file_name(file);
    let (stem, ext) = match clean.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (clean.clone(), String::new()),
    };
    for n in 1..10_000 {
        let p = if n == 1 {
            dir.join(&clean)
        } else {
            dir.join(format!("{stem}-{n}{ext}"))
        };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(mut f) => {
                f.write_all(bytes)?;
                return Ok(sandbox.display_path(&p));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(PhnError::Internal(
        "too many files with the same name".into(),
    ))
}

/// Count of letters by language, with whether an agency translation was used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageCount {
    pub language: String,
    pub documents: usize,
    /// True when the agency's translation in /data/templates was used.
    pub agency_translation: bool,
}

/// How to add an agency translation, for tool output.
pub fn translation_help(kinds: &[&str]) -> String {
    format!(
        "To use your agency's approved translations, save them as \
         /data/templates/<language>/<kind>.md (language name or code, e.g. spanish or es; \
         kinds: {}). Copy the English template written to the output folder and keep the \
         {{{{placeholders}}}}. This server never machine-translates letters.",
        kinds.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_match_codes_and_names() {
        assert!(candidates("Spanish").contains(&"es".to_string()));
        assert!(candidates("es-US").contains(&"spanish".to_string()));
        assert!(candidates("Haitian Creole").contains(&"ht".to_string()));
        assert!(is_english("English") && is_english("") && is_english("en-US"));
        assert_eq!(language_label("pt-BR"), "Portuguese");
        assert_eq!(language_label("Klingon"), "Klingon");
    }

    #[test]
    fn write_in_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let (out, _) = output_folder(&ctx, "letters").unwrap();
        let a = write_in(&ctx, &out, "letters_hmong.docx", b"first").unwrap();
        let b = write_in(&ctx, &out, "letters_hmong.docx", b"second").unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("letters_hmong-2.docx"), "{b}");
        assert_eq!(
            std::fs::read(out.join("letters_hmong.docx")).unwrap(),
            b"first"
        );
    }

    #[test]
    fn fill_replaces_known_and_reports_unknown() {
        let mut v = BTreeMap::new();
        v.insert("student_name".to_string(), "Ana\nLima | x".to_string());
        v.insert("grade".to_string(), String::new());
        let (out, unknown) = fill(
            "Dear family of {{ Student Name }}, grade {{grade}}. {{nurse}}",
            &v,
            "____",
        );
        assert_eq!(out, "Dear family of Ana Lima / x, grade ____. [nurse]");
        assert_eq!(unknown, vec!["nurse".to_string()]);
    }
}
