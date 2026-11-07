//! EPI-08 `deidentify_dataset`.

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_local::deid::{
    self, BirthDateHandling, ColumnReport, DeidMode, DeidOptions, RareCombinations,
};
use phn_local::phi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Output file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeidOutput {
    #[default]
    Csv,
    Xlsx,
}

/// Arguments for `deidentify_dataset`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DeidentifyArgs {
    /// CSV or spreadsheet in /data.
    pub path: String,
    pub sheet: Option<String>,
    /// "safe_harbor" (default) or "limited_data_set" (keeps dates, town, ZIP; needs a data use agreement).
    pub mode: Option<DeidMode>,
    /// Replace dates of birth with "age" (default) or "year".
    pub birth_date: Option<BirthDateHandling>,
    /// Date for computing ages, YYYY-MM-DD (default today).
    pub reference_date: Option<String>,
    /// Columns to keep unchanged even if flagged (the output is then NOT de-identified).
    pub keep_columns: Vec<String>,
    /// Extra columns to drop.
    pub drop_columns: Vec<String>,
    /// Replace ID columns with new sequential codes (default true) instead of dropping them.
    pub replace_ids_with_codes: Option<bool>,
    /// Keep free-text columns after automated redaction (default false: drop them).
    pub scrub_free_text: bool,
    /// Output type (default csv).
    pub output: Option<DeidOutput>,
}

/// Result of `deidentify_dataset` (a report; the data itself is written to a file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeidentifyResult {
    pub input: String,
    pub output_file: String,
    pub report_file: String,
    pub mode: DeidMode,
    pub rows: usize,
    pub columns_in: usize,
    pub columns_out: usize,
    pub safe_harbor_compliant: bool,
    pub columns: Vec<ColumnReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rare_combinations: Option<RareCombinations>,
    pub warnings: Vec<String>,
}

/// Write a de-identified copy of a dataset (HIPAA Safe Harbor or limited data set).
pub fn deidentify_dataset(
    ctx: &Context,
    args: DeidentifyArgs,
) -> Result<Envelope<DeidentifyResult>> {
    let (table, label) = crate::core::files::load_table(ctx, &args.path, args.sheet.as_deref())?;
    let reference_date = match &args.reference_date {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| {
            PhnError::InvalidInput(format!("reference_date must be YYYY-MM-DD, got '{d}'"))
        })?,
        None => chrono::Local::now().date_naive(),
    };
    for c in args.keep_columns.iter().chain(args.drop_columns.iter()) {
        if table.column_index(c).is_none() {
            return Err(PhnError::InvalidInput(format!(
                "column '{c}' is not in {label}. Columns: {}",
                table.headers.join(", ")
            )));
        }
    }
    let mode = args.mode.unwrap_or_default();
    let findings = phi::scan_table(&table);
    let threshold = ctx.suppression_threshold();
    let result = deid::deidentify(
        &table,
        &findings,
        &DeidOptions {
            mode,
            reference_date,
            birth_date: args.birth_date.unwrap_or_default(),
            keep_columns: args.keep_columns.clone(),
            drop_columns: args.drop_columns.clone(),
            replace_ids_with_codes: args.replace_ids_with_codes.unwrap_or(true),
            scrub_free_text: args.scrub_free_text,
            k_threshold: threshold,
        },
    );

    // Write the output and a report file.
    let sandbox = ctx.sandbox()?;
    let stem = label
        .rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.').map(|(s, _)| s.to_string()))
        .unwrap_or_else(|| "dataset".into());
    let suffix = match mode {
        DeidMode::SafeHarbor => "deidentified",
        DeidMode::LimitedDataSet => "limited_data_set",
    };
    let block = phn_docs::model::TableBlock {
        caption: Some(format!("{stem} ({suffix})")),
        headers: result.table.headers.clone(),
        rows: result.table.rows.clone(),
    };
    let (out_path, bytes) = match args.output.unwrap_or_default() {
        DeidOutput::Csv => (
            sandbox.new_output_file(&format!("{stem}_{suffix}.csv"))?,
            phn_docs::csv_out::table_to_csv(&block)?,
        ),
        DeidOutput::Xlsx => {
            let doc = phn_docs::model::Document {
                title: format!("{stem} ({suffix})"),
                subtitle: None,
                language: "en-US".into(),
                author: None,
                letterhead: None,
                logo: None,
                blocks: vec![phn_docs::model::Block::Table(block)],
            };
            (
                sandbox.new_output_file(&format!("{stem}_{suffix}.xlsx"))?,
                phn_docs::xlsx::to_xlsx(&doc)?,
            )
        }
    };
    std::fs::write(&out_path, bytes)?;
    let output_file = sandbox.display_path(&out_path);

    let mode_name = match mode {
        DeidMode::SafeHarbor => "HIPAA Safe Harbor (45 CFR 164.514(b)(2))",
        DeidMode::LimitedDataSet => "Limited data set (45 CFR 164.514(e))",
    };
    let rows: Vec<Vec<String>> = result
        .columns
        .iter()
        .map(|c| {
            vec![
                c.column.clone(),
                c.identifier
                    .map(|k| k.label().to_string())
                    .unwrap_or_default(),
                format!("{:?}", c.action).to_lowercase(),
                c.detail.clone(),
            ]
        })
        .collect();
    let mut report = format!(
        "# De-identification report\n\n- Input: /data/{label}\n- Output: /data/{output_file}\n- Method: {mode_name}\n- Date: {}\n- Rows: {}\n- Columns: {} in, {} out\n- Safe Harbor compliant: {}\n\n## Column actions\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
        table.rows.len(),
        table.headers.len(),
        result.table.headers.len(),
        if result.safe_harbor_compliant {
            "yes (automated checks)"
        } else {
            "NO"
        }
    );
    report.push_str(&md::table(
        &["Column", "Identifier", "Action", "Detail"],
        &rows,
    ));
    if !result.warnings.is_empty() {
        report.push_str("\n## Warnings\n\n");
        for w in &result.warnings {
            report.push_str(&format!("- {w}\n"));
        }
    }
    report.push_str(
        "\n## Before sharing\n\nSafe Harbor also requires that you have no actual knowledge that the \
         remaining information could identify a person. Review the output, especially small groups \
         and free text, before releasing it.\n",
    );
    let report_path = sandbox.new_output_file(&format!("{stem}_{suffix}_report.md"))?;
    std::fs::write(&report_path, &report)?;
    let report_file = sandbox.display_path(&report_path);

    let mut summary = format!(
        "**De-identified copy written** to /data/{output_file}\n\nMethod: {mode_name}. {} rows; {} of {} columns kept. {}\n\n",
        table.rows.len(),
        result.table.headers.len(),
        table.headers.len(),
        if result.safe_harbor_compliant {
            "Automated Safe Harbor checks passed.".to_string()
        } else if mode == DeidMode::LimitedDataSet {
            "This is a limited data set, not de-identified data.".to_string()
        } else {
            "⚠ NOT Safe Harbor compliant because identifier columns were kept at your request."
                .to_string()
        }
    );
    summary.push_str(&md::table(
        &["Column", "Identifier", "Action", "Detail"],
        &rows,
    ));
    summary.push_str(&format!("\nA report was written to /data/{report_file}.\n"));
    let mut caveats = result.warnings.clone();
    caveats.push(
        "Automated detection can miss identifiers. Safe Harbor also requires no actual knowledge \
         that the remaining data could identify someone; review before sharing."
            .into(),
    );
    caveats.push(
        "The restricted three-digit ZIP list comes from HHS guidance based on the 2000 Census."
            .into(),
    );
    let mut kinds: Vec<String> = findings
        .iter()
        .map(|f| f.kind.label().to_string())
        .collect();
    kinds.sort();
    kinds.dedup();
    Ok(Envelope::new(
        summary,
        DeidentifyResult {
            input: label.clone(),
            output_file: output_file.clone(),
            report_file: report_file.clone(),
            mode,
            rows: table.rows.len(),
            columns_in: table.headers.len(),
            columns_out: result.table.headers.len(),
            safe_harbor_compliant: result.safe_harbor_compliant,
            columns: result.columns,
            rare_combinations: result.rare_combinations,
            warnings: result.warnings,
        },
    )
    .source(Source::local_file(label))
    .source(
        Source::computed(
            "Guidance Regarding Methods for De-identification of PHI (HIPAA Privacy Rule)",
            "U.S. Department of Health and Human Services, Office for Civil Rights",
        )
        .with_url("https://www.hhs.gov/hipaa/for-professionals/special-topics/de-identification/index.html"),
    )
    .caveats(caveats)
    .privacy(Privacy {
        identifiers_detected: kinds,
        files_written: vec![output_file, report_file],
        notes: vec!["Only the de-identification report is returned; the data stays in the output folder.".into()],
        ..Privacy::none()
    }))
}
