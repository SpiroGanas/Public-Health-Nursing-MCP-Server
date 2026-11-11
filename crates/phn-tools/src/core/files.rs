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
            let finding = findings.iter().find(|f| f.index == i);
            match finding.map(|f| f.kind) {
                // A birth year that shows an age of 90 or more is itself withheld.
                Some(Identifier::BirthDate) => match phi::parse_date(v) {
                    Some(d) => {
                        let today = chrono::Local::now().date_naive();
                        if phn_local::deid::age_on(d, today) >= 90 {
                            "90+".into()
                        } else {
                            d.format("%Y").to_string()
                        }
                    }
                    None => "[REDACTED: date]".into(),
                },
                Some(Identifier::Date) => phi::parse_date(v)
                    .map(|d| d.format("%Y").to_string())
                    .or_else(|| phi::plain_year(v).map(|y| y.to_string()))
                    .unwrap_or_else(|| "[REDACTED: date]".into()),
                // Same rule as de-identification: three digits, 000 for the
                // low-population prefixes, leading zero restored.
                Some(Identifier::ZipCode) => match phn_local::deid::zip3(v.trim()) {
                    Some(z) => format!("{z}xx"),
                    None => "[REDACTED]".into(),
                },
                Some(Identifier::Age) => {
                    let years = finding.is_none_or(|f| phi::age_header_is_years(&f.column));
                    let lead: String = v
                        .trim()
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    match lead.parse::<f64>() {
                        Ok(a) if years && a > 89.0 => "90+".into(),
                        _ => v.clone(),
                    }
                }
                Some(k) => format!("[REDACTED: {}]", k.label()),
                None => {
                    if v.parse::<f64>().is_ok() {
                        // Long numbers in a column that was not recognized are
                        // most likely record, phone, or ID numbers.
                        if v.chars().filter(char::is_ascii_digit).count() >= 6 {
                            "[REDACTED: number]".into()
                        } else {
                            v.clone()
                        }
                    } else if v.len() < 6 {
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
        let generic: usize = extracted
            .tables
            .iter()
            .map(phn_local::table::generic_header_count)
            .sum();
        if generic > 0 {
            caveats.push(format!(
                "{generic} column(s) are shown as column_N because the header was blank or looked \
                 like data or an identifier (for example a file with no header row, or names used \
                 as column headers)."
            ));
        }
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
        // Headings and titles are returned too, so they count as well.
        let headings = extracted
            .sections
            .iter()
            .map(|s| s.heading.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let heading_signal = phi::redact_text(&headings, requested)
            .redacted
            .keys()
            .any(|k| client_signals.contains(k));
        // A table (a Word table, or tab-, pipe- or semicolon-separated text)
        // with a client-identifier column is a client roster even though no
        // value carries its own label.
        let table_signal = extracted
            .tables
            .iter()
            .chain(delimited_table(&extracted.text).iter())
            .any(|t| {
                phi::scan_table(t).iter().any(|f| {
                    client_signals.contains(&f.kind) || f.kind == Identifier::OtherIdNumber
                })
            });
        // Several names next to as many dates or street addresses reads as
        // a list of people. Staff contact lists (names with phone numbers) do not match.
        let seen = |k: Identifier| {
            red.redacted.get(&k).copied().unwrap_or(0)
                + red.detected_only.get(&k).copied().unwrap_or(0)
        };
        let density_signal = seen(Identifier::Name) >= 3
            && seen(Identifier::Date) + seen(Identifier::StreetAddress) >= 3;
        let escalated = requested == RedactionLevel::Standard
            && (red.redacted.keys().any(|k| client_signals.contains(k))
                || heading_signal
                || table_signal
                || density_signal);
        let level = if escalated {
            RedactionLevel::Strict
        } else {
            requested
        };
        if escalated {
            red = phi::redact_text(&extracted.text, RedactionLevel::Strict);
            caveats.push(
                "This document contains client identifiers (such as a date of birth, record \
                 number, or SSN) or looks like a list of people, so strict redaction was applied: \
                 names, phone numbers, addresses, and dates were also removed."
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
                    // Headings are returned, so they are redacted like the body.
                    heading: phi::redact_text(&s.heading, level).text,
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

/// Read text as a table when it is laid out as one: at least three lines with
/// the same number (two or more) of tabs, pipes, or semicolons. Commas are not
/// used because ordinary sentences contain them.
fn delimited_table(text: &str) -> Option<Table> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(5_000)
        .collect();
    for d in ['\t', '|', ';'] {
        let counts: Vec<usize> = lines.iter().map(|l| l.matches(d).count()).collect();
        let Some(&width) = counts.iter().find(|&&c| c >= 2) else {
            continue;
        };
        let mut rows: Vec<Vec<String>> = lines
            .iter()
            .zip(&counts)
            .filter(|(_, c)| **c == width)
            .map(|(l, _)| l.split(d).map(|c| c.trim().to_string()).collect())
            .collect();
        if rows.len() >= 3 {
            let headers = rows.remove(0);
            return Some(Table {
                name: "text".into(),
                headers,
                rows,
                truncated: false,
            });
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join("data").join(name), text).unwrap();
        }
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn read(ctx: &Context, path: &str, include_rows: bool) -> String {
        let env = read_local_file(
            ctx,
            ReadLocalFileArgs {
                path: Some(path.into()),
                include_rows,
                ..ReadLocalFileArgs::default()
            },
        )
        .unwrap();
        format!(
            "{}\n{}",
            env.to_markdown(),
            serde_json::to_string(&env).unwrap()
        )
    }

    #[test]
    fn headings_are_redacted_like_the_body() {
        let (_d, ctx) = ctx_with(&[(
            "plan.md",
            "# Health Care Plan: Maria Garcia, DOB 03/02/2015\nTake inhaler before gym.\n",
        )]);
        let out = read(&ctx, "plan.md", false);
        for secret in ["Garcia", "03/02/2015"] {
            assert!(!out.contains(secret), "{secret} leaked: {out}");
        }
    }

    #[test]
    fn a_roster_table_in_a_text_file_escalates_to_strict() {
        let (_d, ctx) = ctx_with(&[(
            "roster.txt",
            "Name\tDOB\tPhone\nMaria Garcia\t03/02/2015\t413-555-0101\nJames Smith\t11/30/2012\t617-555-0199\n",
        )]);
        let out = read(&ctx, "roster.txt", false);
        for secret in ["Garcia", "Smith", "03/02/2015", "413-555-0101"] {
            assert!(!out.contains(secret), "{secret} leaked: {out}");
        }
        // A staff contact list (names and phones only) keeps standard redaction.
        let (_d2, ctx2) = ctx_with(&[(
            "contacts.txt",
            "Role\tName\tPhone\nSupervisor\tMaria Garcia\t413-555-0101\nBackup\tJames Smith\t617-555-0199\n",
        )]);
        assert!(read(&ctx2, "contacts.txt", false).contains("413-555-0101"));
    }

    #[test]
    fn a_file_without_a_header_row_is_not_echoed_as_column_names() {
        let (_d, ctx) = ctx_with(&[(
            "nohdr.csv",
            "Maria,Garcia,3/7/2015,413-555-0101,12 Elm St\nJames,Smith,11/30/2012,617-555-0199,4 Oak Ave\n",
        )]);
        let out = read(&ctx, "nohdr.csv", false);
        for secret in ["Garcia", "413-555-0101", "12 Elm St", "3/7/2015"] {
            assert!(!out.contains(secret), "{secret} leaked: {out}");
        }
        assert!(out.contains("column_1") && out.contains("no header row"));
    }

    #[test]
    fn masked_rows_follow_the_stated_rules() {
        let f = |index: usize, column: &str, kind| ColumnFinding {
            column: column.into(),
            index,
            kind,
            confidence: phi::Confidence::High,
            evidence: String::new(),
        };
        let findings = vec![
            f(0, "ZIP", Identifier::ZipCode),
            f(1, "DOB", Identifier::BirthDate),
            f(2, "Age", Identifier::Age),
            f(3, "Age (months)", Identifier::Age),
        ];
        let row: Vec<String> = ["1002", "1930-05-01", "92 yrs", "96", "4135550101", "12"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            mask_row(&row, &findings),
            vec!["010xx", "90+", "90+", "96", "[REDACTED: number]", "12"]
        );
        let border: Vec<String> = vec!["03601".into()];
        assert_eq!(mask_row(&border, &findings[..1]), vec!["000xx"]);
    }
}
