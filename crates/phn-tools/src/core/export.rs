//! CORE-06 `export_document`, including the local mail-merge pattern.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::accessibility::{self, Report};
use phn_docs::model::{self, Block, Document};
use phn_docs::{csv_out, docx, mailmerge, pdf, svg, xlsx};
use phn_local::sandbox::sanitize_file_name;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// Output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Docx,
    Pdf,
    Xlsx,
    Csv,
    Svg,
    Png,
}

impl ExportFormat {
    fn ext(self) -> &'static str {
        match self {
            ExportFormat::Docx => "docx",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Csv => "csv",
            ExportFormat::Svg => "svg",
            ExportFormat::Png => "png",
        }
    }
}

/// Mail-merge output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    /// One document with a page break between records (default).
    #[default]
    Combined,
    /// One file per record, numbered (file names never contain data values).
    OneFilePerRecord,
}

/// Arguments for `export_document`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ExportDocumentArgs {
    /// Document title (also set as the file's title property).
    pub title: String,
    /// Optional subtitle.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Body in Markdown: # headings, paragraphs, **bold**, lists, [links](https://…),
    /// pipe tables (a line "Table: caption" right before a table sets its caption),
    /// images as ![alt text](path/in/data.png), and "\pagebreak".
    pub content_markdown: String,
    /// Formats to write (default ["docx"]). XLSX and CSV export the tables.
    #[serde(default)]
    pub formats: Option<Vec<ExportFormat>>,
    /// Base file name without extension (default: from the title).
    #[serde(default)]
    pub file_name: Option<String>,
    /// Language tag, e.g. "en-US", "es-US", "pt-BR" (default "en-US").
    #[serde(default)]
    pub language: Option<String>,
    /// Use the configured agency letterhead (default true when one is configured).
    #[serde(default)]
    pub use_letterhead: Option<bool>,
    /// For CSV with several tables: which table (1-based).
    #[serde(default)]
    pub table_index: Option<usize>,
    /// Mail-merge: a CSV/XLSX file in /data whose columns fill {{placeholders}}
    /// in the title and content. Merged documents are written to the output
    /// folder and never returned.
    #[serde(default)]
    pub merge_data_path: Option<String>,
    /// Sheet for the merge data.
    #[serde(default)]
    pub merge_sheet: Option<String>,
    /// Combined document (default) or one file per record.
    #[serde(default)]
    pub merge_mode: Option<MergeMode>,
}

/// One written file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WrittenFile {
    /// Path relative to /data.
    pub path: String,
    pub format: ExportFormat,
    pub bytes: u64,
}

/// Mail-merge summary (counts only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MergeSummary {
    pub records: usize,
    pub fields: Vec<String>,
    /// Placeholders that were filled with an empty value.
    pub empty_values: usize,
}

/// Result of `export_document`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExportResult {
    pub files: Vec<WrittenFile>,
    /// Accessibility self-check of DOCX/PDF output (first file of each type).
    pub accessibility: Vec<Report>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge: Option<MergeSummary>,
}

fn render(doc: &Document, format: ExportFormat, table_index: Option<usize>) -> Result<Vec<u8>> {
    match format {
        ExportFormat::Docx => docx::to_docx(doc),
        ExportFormat::Pdf => pdf::to_pdf(doc),
        ExportFormat::Xlsx => xlsx::to_xlsx(doc),
        ExportFormat::Csv => csv_out::to_csv(doc, table_index),
        ExportFormat::Svg => Ok(svg::to_svg(doc).into_bytes()),
        ExportFormat::Png => svg::to_png(doc),
    }
}

fn self_check(bytes: &[u8], format: ExportFormat) -> Option<Report> {
    match format {
        ExportFormat::Docx => phn_local::docx::parse_docx(bytes)
            .ok()
            .map(|d| accessibility::check_docx(&d)),
        ExportFormat::Pdf => phn_local::pdf::parse_pdf(bytes)
            .ok()
            .map(|i| accessibility::check_pdf(&i)),
        _ => None,
    }
}

/// Export Markdown content to DOCX, PDF, XLSX, CSV, SVG, or PNG in the output folder.
pub fn export_document(ctx: &Context, args: ExportDocumentArgs) -> Result<Envelope<ExportResult>> {
    if args.title.trim().is_empty() {
        return Err(PhnError::InvalidInput("title must not be empty".into()));
    }
    let sandbox = ctx.sandbox()?;
    let settings = ctx.settings();
    let formats = args
        .formats
        .clone()
        .unwrap_or_else(|| vec![ExportFormat::Docx]);
    if formats.is_empty() {
        return Err(PhnError::InvalidInput("choose at least one format".into()));
    }
    let letterhead = match args.use_letterhead {
        Some(false) => None,
        _ => settings.letterhead.clone(),
    };
    let logo = match letterhead.as_ref().and_then(|l| l.logo_path.as_deref()) {
        Some(p) => Some(std::fs::read(sandbox.resolve_existing(p)?)?),
        None => None,
    };

    let mut blocks = model::from_markdown(&args.content_markdown);
    for b in &mut blocks {
        if let Block::Image(img) = b {
            if img.alt.trim().is_empty() {
                return Err(PhnError::InvalidInput(format!(
                    "image '{}' has no alt text; write it as ![description of the image]({})",
                    img.path, img.path
                )));
            }
            let path = sandbox.resolve_existing(&img.path)?;
            let bytes = std::fs::read(&path)?;
            if phn_docs::image::inspect(&bytes).is_none() {
                return Err(PhnError::Unsupported(format!(
                    "image '{}' must be PNG or JPEG",
                    img.path
                )));
            }
            img.data = Some(bytes);
        }
    }
    let doc = Document {
        title: args.title.trim().to_string(),
        subtitle: args.subtitle.clone(),
        language: args.language.clone().unwrap_or_else(|| "en-US".into()),
        author: None,
        letterhead: letterhead.clone(),
        logo,
        blocks,
    };
    let base = sanitize_file_name(args.file_name.as_deref().unwrap_or(&args.title));
    let mut files = Vec::new();
    let mut reports = Vec::new();
    let mut privacy = Privacy::none();
    let mut caveats = Vec::new();
    let mut merge = None;

    let mut write = |doc: &Document,
                     name: &str,
                     format: ExportFormat,
                     dir: Option<&std::path::Path>|
     -> Result<Vec<u8>> {
        let bytes = render(doc, format, args.table_index)?;
        let path = match dir {
            Some(d) => d.join(format!("{name}.{}", format.ext())),
            None => sandbox.new_output_file(&format!("{name}.{}", format.ext()))?,
        };
        std::fs::write(&path, &bytes)?;
        files.push(WrittenFile {
            path: sandbox.display_path(&path),
            format,
            bytes: bytes.len() as u64,
        });
        Ok(bytes)
    };

    if let Some(data_path) = &args.merge_data_path {
        if formats
            .iter()
            .any(|f| !matches!(f, ExportFormat::Docx | ExportFormat::Pdf))
        {
            return Err(PhnError::InvalidInput(
                "mail-merge supports DOCX and PDF output".into(),
            ));
        }
        let (table, data_label) =
            super::files::load_table(ctx, data_path, args.merge_sheet.as_deref())?;
        if table.rows.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "{data_label} has no data rows"
            )));
        }
        if table.rows.len() > 5_000 {
            return Err(PhnError::InvalidInput(
                "mail-merge is limited to 5,000 records per call".into(),
            ));
        }
        let binding = mailmerge::bind(&doc, &table)?;
        let mut empty_total = 0;
        match args.merge_mode.unwrap_or_default() {
            MergeMode::Combined => {
                let (combined, empty) = mailmerge::combined(&doc, &table, &binding);
                empty_total = empty;
                for f in &formats {
                    write(&combined, &format!("{base}_merged"), *f, None)?;
                }
            }
            MergeMode::OneFilePerRecord => {
                let dir = sandbox.new_output_file(&format!("{base}_letters"))?;
                std::fs::create_dir_all(&dir)?;
                for (i, row) in table.rows.iter().enumerate() {
                    let (d, empty) = mailmerge::fill(&doc, &binding, row);
                    empty_total += empty;
                    for f in &formats {
                        write(&d, &format!("{base}_{:04}", i + 1), *f, Some(&dir))?;
                    }
                }
            }
        }
        // The accessibility report quotes document text (title, headings,
        // bold lines), so it is run on the unfilled template, never on a
        // merged document: no value from the data file reaches the result.
        for f in &formats {
            if let Some(r) = self_check(&render(&doc, *f, args.table_index)?, *f) {
                reports.push(r);
            }
        }
        let mut fields: Vec<String> = binding.keys().cloned().collect();
        fields.sort();
        merge = Some(MergeSummary {
            records: table.rows.len(),
            fields,
            empty_values: empty_total,
        });
        privacy.files_written = files.iter().map(|f| f.path.clone()).collect();
        privacy.notes.push(format!(
            "Merged documents may contain identifiable information from {data_label}; they were \
             written to the output folder and their contents are not included here."
        ));
        if empty_total > 0 {
            caveats.push(format!(
                "{empty_total} placeholder(s) were filled with empty values because the data cell was blank."
            ));
        }
    } else {
        for f in &formats {
            let bytes = write(&doc, &base, *f, None)?;
            if let Some(r) = self_check(&bytes, *f) {
                reports.push(r);
            }
        }
    }

    let mut summary = format!("**Exported \"{}\"**\n\n", doc.title);
    if let Some(m) = &merge {
        summary.push_str(&format!(
            "Mail-merge: {} record(s) using fields {}.\n\n",
            m.records,
            m.fields
                .iter()
                .map(|f| format!("{{{{{f}}}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let shown: Vec<&WrittenFile> = files.iter().take(10).collect();
    for f in &shown {
        summary.push_str(&format!(
            "- /data/{} ({:?}, {} KB)\n",
            f.path,
            f.format,
            f.bytes.div_ceil(1024)
        ));
    }
    if files.len() > shown.len() {
        summary.push_str(&format!(
            "- … and {} more file(s)\n",
            files.len() - shown.len()
        ));
    }
    for r in &reports {
        summary.push_str(&format!(
            "\nAccessibility self-check ({}): {} passed, {} failed, {} warning(s), {} manual check(s).",
            r.format.to_uppercase(),
            r.passed,
            r.failed,
            r.warnings,
            r.manual
        ));
    }
    if letterhead.is_none() && settings.letterhead.is_none() {
        caveats.push("No letterhead is configured; set one with configure_jurisdiction.".into());
    }
    if formats
        .iter()
        .any(|f| matches!(f, ExportFormat::Pdf | ExportFormat::Svg | ExportFormat::Png))
        && !doc
            .all_text()
            .chars()
            .all(|c| phn_docs::fonts::has_glyph(c) || c.is_whitespace())
    {
        caveats.push(
            "Some characters are not in the bundled font (for example Chinese, Khmer, or Arabic \
             script) and appear as '?' in PDF/SVG/PNG. Use DOCX for those languages."
                .into(),
        );
    }
    Ok(Envelope::new(
        summary,
        ExportResult {
            files,
            accessibility: reports,
            merge,
        },
    )
    .source(Source::computed(
        "Document generated from your content",
        "PHN MCP server",
    ))
    .caveats(caveats)
    .privacy(privacy))
}
