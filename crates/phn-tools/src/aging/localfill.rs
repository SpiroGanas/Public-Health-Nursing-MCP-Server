//! Template + local fill for documents about a person (Phase 10: elder and
//! disabled-adult protective services report narratives, advance care
//! planning handouts).
//!
//! The tool returns the template and guidance to the model. When the nurse
//! points it at a CSV/XLSX in /data with the person's details (one row per
//! document, columns named like the template's `{{Fields}}`), the documents
//! are filled locally and written to /data/output. Only counts, field names,
//! and generic file names come back; never a value from the file.
//!
//! Translations: a document can use an agency-approved translation from
//! `/data/templates/<language>/<template_id>.md` (Markdown with the same
//! `{{Fields}}`), chosen by a `Language` column in the file. When there is
//! no agency translation for a row's language, the English template is used
//! and the result counts it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use phn_core::{PhnError, Result};
use phn_docs::model::Document;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// What to fill.
pub struct FillSpec<'a> {
    /// Template id; agency translations are `<template_id>.md`.
    pub template_id: &'a str,
    /// The built-in English template with `{{Field}}` placeholders.
    pub english: Document,
    /// CSV/XLSX in /data, one row per document.
    pub file: &'a str,
    pub sheet: Option<&'a str>,
    /// "docx" or "pdf".
    pub format: &'a str,
    /// Output file name stem (generic; never a person's name).
    pub output_stem: &'a str,
    /// Look for agency translations by a Language column.
    pub translatable: bool,
}

/// Documents written for one language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageCount {
    pub language: String,
    pub documents: usize,
    /// "agency translation" or "English (no agency translation found)".
    pub template_used: String,
}

/// What was written (counts and names only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FillOutcome {
    pub file: String,
    pub rows_read: usize,
    pub documents_written: usize,
    pub output_files: Vec<String>,
    /// Template fields found as columns in the file.
    pub fields_filled_from_file: Vec<String>,
    /// Template fields with no column (left as a blank line to complete by hand).
    pub fields_left_blank: Vec<String>,
    /// Filled fields whose cell was empty, summed over all documents.
    pub empty_cells: usize,
    pub languages: Vec<LanguageCount>,
    /// Categories of identifiers in the file (never values).
    pub identifiers_detected: Vec<String>,
}

const BLANK: &str = "____________________";

/// Language aliases for folder names.
fn language_keys(lang: &str) -> Vec<String> {
    let n = phn_core::text::normalize(lang);
    let mut keys = vec![n.clone(), n.replace(' ', "_"), n.replace(' ', "-")];
    let alias: &[(&str, &[&str])] = &[
        ("spanish", &["es", "espanol"]),
        ("portuguese", &["pt", "pt br", "portugues"]),
        ("haitian creole", &["ht", "kreyol", "haitian", "creole"]),
        ("vietnamese", &["vi"]),
        ("chinese", &["zh", "mandarin", "cantonese"]),
        ("arabic", &["ar"]),
        ("russian", &["ru"]),
        ("khmer", &["km"]),
        ("french", &["fr"]),
        ("cape verdean creole", &["kea", "kriolu"]),
    ];
    for (name, codes) in alias {
        if n == *name || codes.contains(&n.as_str()) {
            keys.push((*name).to_string());
            keys.push(name.replace(' ', "_"));
            keys.extend(codes.iter().map(|c| (*c).to_string()));
        }
    }
    keys
}

fn agency_template(ctx: &Context, lang: &str, template_id: &str) -> Option<(Document, PathBuf)> {
    let base = ctx.config.paths.data_dir.join("templates");
    let entries = std::fs::read_dir(&base).ok()?;
    let keys = language_keys(lang);
    for e in entries.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let folder = phn_core::text::normalize(&e.file_name().to_string_lossy());
        if !keys.iter().any(|k| phn_core::text::normalize(k) == folder) {
            continue;
        }
        let p = e.path().join(format!("{template_id}.md"));
        if let Ok(text) = std::fs::read_to_string(&p) {
            let mut blocks = phn_docs::model::from_markdown(&text);
            let mut title = template_id.replace('_', " ");
            if let Some(phn_docs::model::Block::Heading { level: 1, text }) = blocks.first() {
                title = text.clone();
                blocks.remove(0);
            }
            return Some((
                Document {
                    title,
                    subtitle: None,
                    language: lang.to_string(),
                    author: None,
                    letterhead: None,
                    logo: None,
                    blocks,
                },
                p,
            ));
        }
    }
    None
}

/// Fill one document per row and write them to /data/output.
pub fn fill_local(ctx: &Context, spec: FillSpec<'_>) -> Result<FillOutcome> {
    let (table, label) = crate::core::files::load_table(ctx, spec.file, spec.sheet)?;
    if table.rows.is_empty() {
        return Err(PhnError::InvalidInput(format!("'{label}' has no data rows")));
    }
    if spec.format != "docx" && spec.format != "pdf" {
        return Err(PhnError::InvalidInput("format must be \"docx\" or \"pdf\"".into()));
    }
    if table.rows.len() > 500 {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' has {} rows; fill at most 500 documents at a time",
            table.rows.len()
        )));
    }
    let findings = phn_local::phi::scan_table(&table);
    let mut identifiers: Vec<String> = findings.iter().map(|f| f.kind.label().to_string()).collect();
    identifiers.sort();
    identifiers.dedup();
    let lang_col = spec
        .translatable
        .then(|| {
            table.headers.iter().position(|h| {
                matches!(
                    phn_core::text::normalize(h).as_str(),
                    "language" | "preferred language" | "primary language"
                )
            })
        })
        .flatten();
    let sandbox = ctx.sandbox()?;
    let mut written = Vec::new();
    let mut empty_cells = 0;
    let mut by_lang: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    let mut filled_fields = std::collections::BTreeSet::new();
    let mut blank_fields = std::collections::BTreeSet::new();
    for (i, row) in table.rows.iter().enumerate() {
        let lang = lang_col
            .and_then(|c| row.get(c))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "English".to_string());
        let english = phn_core::text::normalize(&lang) == "english" || phn_core::text::normalize(&lang) == "en";
        let (mut template, translated) = if english {
            (spec.english.clone(), false)
        } else {
            match agency_template(ctx, &lang, spec.template_id) {
                Some((d, _)) => (d, true),
                None => (spec.english.clone(), false),
            }
        };
        template.letterhead = ctx.settings().letterhead;
        let names = phn_docs::mailmerge::placeholders(&template);
        let mut binding = std::collections::HashMap::new();
        let mut missing = Vec::new();
        for n in &names {
            match table
                .headers
                .iter()
                .position(|h| phn_core::text::normalize(h) == phn_core::text::normalize(n))
            {
                Some(ix) => {
                    binding.insert(n.clone(), ix);
                    filled_fields.insert(n.clone());
                }
                None => {
                    missing.push(n.clone());
                    blank_fields.insert(n.clone());
                }
            }
        }
        if !missing.is_empty() {
            template = template.map_text(&|s: &str| {
                let mut out = s.to_string();
                for m in &missing {
                    out = out.replace(&format!("{{{{{m}}}}}"), BLANK);
                }
                out
            });
        }
        let (doc, empty) = phn_docs::mailmerge::fill(&template, &binding, row);
        empty_cells += empty;
        let bytes = if spec.format == "docx" {
            phn_docs::docx::to_docx(&doc)?
        } else {
            phn_docs::pdf::to_pdf(&doc)?
        };
        let path = sandbox.new_output_file(&format!("{}_{:03}.{}", spec.output_stem, i + 1, spec.format))?;
        std::fs::write(&path, bytes)?;
        written.push(sandbox.display_path(&path));
        let key = if english { "English".to_string() } else { phn_core::text::title_case(&lang) };
        let e = by_lang.entry(key).or_insert((0, translated));
        e.0 += 1;
        e.1 &= translated;
    }
    let languages = by_lang
        .into_iter()
        .map(|(language, (documents, translated))| LanguageCount {
            template_used: if language == "English" {
                "English".into()
            } else if translated {
                "agency translation".into()
            } else {
                "English (no agency translation found)".into()
            },
            language,
            documents,
        })
        .collect();
    Ok(FillOutcome {
        file: label,
        rows_read: table.rows.len(),
        documents_written: written.len(),
        output_files: written,
        fields_filled_from_file: filled_fields.into_iter().collect(),
        fields_left_blank: blank_fields.into_iter().collect(),
        empty_cells,
        languages,
        identifiers_detected: identifiers,
    })
}

/// Markdown lines describing a fill outcome (counts only).
pub fn outcome_markdown(o: &FillOutcome) -> String {
    let mut s = format!(
        "\n**Filled locally:** {} document(s) from {} row(s) in `{}` were written to the output folder (contents not shown).\n",
        o.documents_written, o.rows_read, o.file
    );
    if !o.fields_left_blank.is_empty() {
        s.push_str(&format!(
            "- Fields with no matching column (left blank to complete by hand): {}\n",
            o.fields_left_blank.join(", ")
        ));
    }
    if o.empty_cells > 0 {
        s.push_str(&format!("- Empty cells used: {}\n", o.empty_cells));
    }
    for l in &o.languages {
        s.push_str(&format!("- {}: {} document(s), {}\n", l.language, l.documents, l.template_used));
    }
    s
}
