//! CORE-04 `read_local_file`.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_local::extract::{self, FileEntry, FileKind};
use phn_local::phi::{self, ColumnFinding, Identifier, RedactionLevel};
use phn_local::table::Table;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// What to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadAction {
    /// List files in a folder.
    List,
    /// Extract text and tables from a file.
    Read,
}

/// Arguments for `read_local_file`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ReadLocalFileArgs {
    /// "list" or "read". Defaults to "read" for a file and "list" for a folder or no path.
    pub action: Option<ReadAction>,
    /// Path relative to the data folder (/data). Empty lists the top folder.
    pub path: Option<String>,
    /// List subfolders too.
    pub recursive: bool,
    /// Sheet name for spreadsheets (default: all sheets).
    pub sheet: Option<String>,
    /// For CSV/XLSX: include up to `max_rows` sample rows. Identifier columns
    /// are always masked and dates reduced to year. Default false.
    pub include_rows: bool,
    /// Sample rows to include (1–200, default 20).
    pub max_rows: Option<usize>,
    /// Maximum characters of document text to return (default 20,000; max 100,000).
    pub max_chars: Option<usize>,
    /// Redaction for document text: "standard" (SSNs, labeled DOB and record
    /// numbers; default) or "strict" (also names, phones, emails, addresses, dates).
    pub redaction: Option<RedactionLevel>,
}

/// A table preview (never raw identifiers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TablePreview {
    pub name: String,
    pub columns: Vec<String>,
    pub row_count: usize,
    pub possible_identifiers: Vec<ColumnFinding>,
    /// Sample rows with identifiers masked (only when requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rows: Option<Vec<Vec<String>>>,
}

/// A section of a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SectionSummary {
    pub heading: String,
    pub level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    pub chars: usize,
}

/// Result of `read_local_file`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocalFileResult {
    pub action: Option<ReadAction>,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<FileKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<SectionSummary>>,
    /// Extracted text after redaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub text_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tables: Option<Vec<TablePreview>>,
    /// Identifiers redacted from the returned text, by kind.
    pub redacted: BTreeMap<Identifier, usize>,
    /// Identifiers detected but left in the text (standard redaction), by kind.
    pub detected_not_redacted: BTreeMap<Identifier, usize>,
    pub notes: Vec<String>,
}

/// Mask a row for display: identifiers hidden, dates to year, ZIP to 3 digits,
/// ages over 89 top-coded, and identifiers inside other text redacted.
pub fn mask_row(row: &[String], findings: &[ColumnFinding]) -> Vec<String> {
    row.iter()
        .enumerate()
        .map(|(i, v)| {
            if v.trim().is_empty() {
                return String::new();
            }
            match findings.iter().find(|f| f.index == i).map(|f| f.kind) {
                Some(Identifier::Date) | Some(Identifier::BirthDate) => phi::parse_date(v)
                    .map(|d| d.format("%Y").to_string())
                    .unwrap_or_else(|| "[REDACTED: date]".into()),
                Some(Identifier::ZipCode) => {
                    let d: String = v.chars().filter(char::is_ascii_digit).take(3).collect();
                    if d.len() == 3 {
                        format!("{d}xx")
                    } else {
                        "[REDACTED]".into()
                    }
                }
                Some(Identifier::Age) => match v.trim().parse::<f64>() {
                    Ok(a) if a > 89.0 => "90+".into(),
                    _ => v.clone(),
                },
                Some(k) => format!("[REDACTED: {}]", k.label()),
                None => {
                    if v.len() < 6 || v.parse::<f64>().is_ok() {
                        v.clone()
                    } else {
                        phi::redact_text(v, RedactionLevel::Strict).text
                    }
                }
            }
        })
        .collect()
}

/// List files in /data, or extract text and tables from one file.
pub fn read_local_file(
    ctx: &Context,
    args: ReadLocalFileArgs,
) -> Result<Envelope<LocalFileResult>> {
    let sandbox = ctx.sandbox()?;
    let rel = args.path.clone().unwrap_or_default();
    let target = if rel.trim().is_empty() {
        sandbox.root().to_path_buf()
    } else {
        sandbox.resolve_existing(&rel)?
    };
    let action = args.action.unwrap_or(if target.is_dir() {
        ReadAction::List
    } else {
        ReadAction::Read
    });

    if action == ReadAction::List {
        let (files, truncated) = extract::list_files(&sandbox, &rel, args.recursive, 500)?;
        let rows: Vec<Vec<String>> = files
            .iter()
            .map(|f| {
                vec![
                    if f.is_dir {
                        format!("{}/", f.path)
                    } else {
                        f.path.clone()
                    },
                    if f.is_dir {
                        "folder".into()
                    } else {
                        format!("{:?}", f.kind).to_lowercase()
                    },
                    if f.is_dir {
                        String::new()
                    } else {
                        md::num(f.bytes as f64, 0)
                    },
                    f.modified.clone().unwrap_or_default(),
                ]
            })
            .collect();
        let shown = if rel.trim().is_empty() {
            "/data".to_string()
        } else {
            format!("/data/{rel}")
        };
        let mut summary = format!("**{} item(s) in {shown}**\n\n", files.len());
        summary.push_str(&md::table(&["Path", "Type", "Bytes", "Modified"], &rows));
        let mut env = Envelope::new(
            summary,
            LocalFileResult {
                action: Some(action),
                path: rel.clone(),
                files: Some(files),
                ..LocalFileResult::default()
            },
        )
        .source(Source::local_file(shown));
        if truncated {
            env = env.caveat("Only the first 500 items are listed.");
        }
        return Ok(env);
    }

    if target.is_dir() {
        return Err(PhnError::InvalidInput(format!(
            "'{rel}' is a folder; use action 'list'"
        )));
    }
    let display = sandbox.display_path(&target);
    let extracted = extract::extract(&target)?;
    let mut data = LocalFileResult {
        action: Some(action),
        path: display.clone(),
        kind: Some(extracted.kind),
        pages: extracted.pages,
        notes: extracted.notes.clone(),
        ..LocalFileResult::default()
    };
    let mut privacy = Privacy::none();
    let mut caveats = Vec::new();
    let mut summary;

    if extracted.kind.is_tabular() {
        let max_rows = args.max_rows.unwrap_or(20).clamp(1, 200);
        let mut previews = Vec::new();
        let mut kinds = std::collections::BTreeSet::new();
        for t in extracted.tables.iter().filter(|t| {
            args.sheet
                .as_ref()
                .is_none_or(|s| t.name.eq_ignore_ascii_case(s))
        }) {
            let findings = phi::scan_table(t);
            kinds.extend(findings.iter().map(|f| f.kind.label().to_string()));
            let sample_rows = args.include_rows.then(|| {
                t.rows
                    .iter()
                    .take(max_rows)
                    .map(|r| mask_row(r, &findings))
                    .collect::<Vec<_>>()
            });
            previews.push(TablePreview {
                name: t.name.clone(),
                columns: t.headers.clone(),
                row_count: t.row_count(),
                possible_identifiers: findings,
                sample_rows,
            });
        }
        if previews.is_empty() {
            return Err(PhnError::NotFound(format!(
                "sheet '{}' in {display}",
                args.sheet.unwrap_or_default()
            )));
        }
        summary = format!("**{display}** — {} table(s)\n\n", previews.len());
        for p in &previews {
            summary.push_str(&format!(
                "- **{}**: {} rows × {} columns ({})\n",
                p.name,
                md::num(p.row_count as f64, 0),
                p.columns.len(),
                p.columns.join(", ")
            ));
            if !p.possible_identifiers.is_empty() {
                summary.push_str(&format!(
                    "  - ⚠ Possible identifiers: {}\n",
                    p.possible_identifiers
                        .iter()
                        .map(|f| format!("{} ({})", f.column, f.kind.label()))
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
            if let Some(rows) = &p.sample_rows {
                summary.push('\n');
                summary.push_str(&md::table(
                    &p.columns.iter().map(String::as_str).collect::<Vec<_>>(),
                    rows,
                ));
            }
        }
        if !args.include_rows {
            summary.push_str(
                "\nRow-level values are not shown. Use profile_local_dataset for counts and \
                 summaries, or include_rows for a masked sample.\n",
            );
        } else {
            privacy.row_level_data = true;
            privacy.notes.push(
                "Sample rows are masked: identifier columns hidden, dates reduced to year, ZIP codes \
                 to three digits, ages over 89 shown as 90+."
                    .into(),
            );
        }
        privacy.identifiers_detected = kinds.into_iter().collect();
        data.tables = Some(previews);
    } else {
        let requested = args.redaction.unwrap_or_default();
        let mut red = phi::redact_text(&extracted.text, requested);
        // A document that contains client identifiers (SSN, labeled date of
        // birth, record or health plan number) is treated as client
        // information: everything detectable is redacted. There is no opt-out.
        let client_signals = [
            Identifier::Ssn,
            Identifier::BirthDate,
            Identifier::MedicalRecordNumber,
            Identifier::HealthPlanNumber,
        ];
        let escalated = requested == RedactionLevel::Standard
            && red.redacted.keys().any(|k| client_signals.contains(k));
        if escalated {
            red = phi::redact_text(&extracted.text, RedactionLevel::Strict);
            caveats.push(
                "This document contains client identifiers (such as a date of birth, record                  number, or SSN), so strict redaction was applied: names, phone numbers,                  addresses, and dates were also removed."
                    .into(),
            );
        }
        let max_chars = args.max_chars.unwrap_or(20_000).clamp(500, 100_000);
        let total = red.text.chars().count();
        let text: String = red.text.chars().take(max_chars).collect();
        data.text_truncated = total > max_chars;
        data.text = Some(text);
        data.sections = Some(
            extracted
                .sections
                .iter()
                .map(|s| SectionSummary {
                    heading: s.heading.clone(),
                    level: s.level,
                    page: s.page,
                    chars: s.text.chars().count(),
                })
                .collect(),
        );
        data.redacted = red.redacted.clone();
        data.detected_not_redacted = red.detected_only.clone();
        summary = format!(
            "**{display}** ({:?}{}) — {} characters{}\n\nThe extracted text is in `data.text`.",
            extracted.kind,
            extracted
                .pages
                .map(|p| format!(", {p} pages"))
                .unwrap_or_default(),
            md::num(total as f64, 0),
            if data.text_truncated {
                format!(" (first {} returned)", md::num(max_chars as f64, 0))
            } else {
                String::new()
            }
        );
        if let Some(secs) = &data.sections {
            let headed: Vec<String> = secs
                .iter()
                .filter(|s| !s.heading.is_empty())
                .take(30)
                .map(|s| {
                    format!(
                        "{}{}",
                        "  ".repeat(s.level.saturating_sub(1) as usize),
                        s.heading
                    )
                })
                .collect();
            if !headed.is_empty() {
                summary.push_str("\n\nSections:\n");
                for h in headed {
                    summary.push_str(&format!("- {h}\n"));
                }
            }
        }
        if !red.redacted.is_empty() {
            let list: Vec<String> = red
                .redacted
                .iter()
                .map(|(k, n)| format!("{n} {}", k.label()))
                .collect();
            privacy
                .notes
                .push(format!("Redacted before returning: {}.", list.join(", ")));
            privacy
                .identifiers_detected
                .extend(red.redacted.keys().map(|k| k.label().to_string()));
        }
        if !red.detected_only.is_empty() {
            let list: Vec<String> = red
                .detected_only
                .iter()
                .map(|(k, n)| format!("{n} {}", k.label()))
                .collect();
            caveats.push(format!(
                "Standard redaction left these in the text: {}. That is expected for agency \
                 documents (staff contacts, dates). If this file contains client information, read \
                 it again with redaction \"strict\".",
                list.join(", ")
            ));
        }
    }
    caveats.extend(extracted.notes);
    Ok(Envelope::new(summary, data)
        .source(Source::local_file(display))
        .caveats(caveats)
        .privacy(privacy))
}

/// Helper for other tools: read one table and refuse when it does not exist.
pub fn load_table(ctx: &Context, path: &str, sheet: Option<&str>) -> Result<(Table, String)> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    if !FileKind::of(&p).is_tabular() {
        return Err(PhnError::InvalidInput(format!(
            "'{path}' is not a CSV or spreadsheet file"
        )));
    }
    let t = phn_local::table::read_table(&p, sheet)?;
    Ok((t, sandbox.display_path(&p)))
}
