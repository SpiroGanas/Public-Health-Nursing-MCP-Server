//! VAX-06 `vaccine_temperature_excursion_helper`.

use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::vax::content::{self, STORE, ThresholdKind};

/// A vaccine in the affected unit (no patient information).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AffectedVaccine {
    /// Vaccine or product name.
    pub name: String,
    pub manufacturer: Option<String>,
    pub lot: Option<String>,
    pub expiration: Option<String>,
    pub doses: Option<u32>,
    /// "state" (state-supplied/VFC) or "private".
    pub funding: Option<String>,
}

/// Arguments for `vaccine_temperature_excursion_helper`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ExcursionArgs {
    /// "refrigerator", "freezer", or "ultra_cold".
    pub unit_type: String,
    /// Lowest temperature recorded during the event.
    pub min_temp: Option<f64>,
    /// Highest temperature recorded during the event.
    pub max_temp: Option<f64>,
    /// "C" (default) or "F".
    pub temp_unit: Option<String>,
    /// Cumulative minutes out of range within the 24-hour period.
    pub minutes_out_of_range: Option<u32>,
    /// State-supplied (MA Vaccine Program) vaccine in the unit (default true).
    pub state_supplied: Option<bool>,
    /// Open multidose vials were in the unit.
    pub open_multidose_vials: bool,
    /// When the excursion was discovered (free text or date-time).
    pub discovered_at: Option<String>,
    /// Vaccines in the unit, for the worksheet.
    pub vaccines: Vec<AffectedVaccine>,
    /// Write an excursion worksheet to /data/output: "xlsx" or "docx".
    pub worksheet: Option<String>,
}

/// How the readings compare with the MA Vaccine Program thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExcursionClass {
    /// Readings are within the normal operating range.
    InRange,
    /// Out of range but below the DPH reporting threshold (keep monitoring;
    /// still follow your SOP and the manufacturer's guidance).
    OutOfRangeBelowReportingThreshold,
    /// Reportable to the MA Vaccine Program (cumulative 30 minutes in 24 hours).
    ReportableCumulative,
    /// Reportable to the MA Vaccine Program immediately, regardless of duration.
    ReportableImmediate,
    /// Not enough information to compare with the thresholds.
    Undetermined,
}

/// Result of `vaccine_temperature_excursion_helper`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExcursionResult {
    pub unit_type: String,
    pub min_temp_c: Option<f64>,
    pub max_temp_c: Option<f64>,
    pub operating_range: String,
    pub classification: ExcursionClass,
    /// Thresholds that the readings met.
    pub triggered: Vec<String>,
    pub cdc_steps: Vec<String>,
    pub dph_steps: Vec<String>,
    pub contacts: Vec<String>,
    /// Always states that only the manufacturer decides viability.
    pub viability: String,
    pub worksheet_file: Option<String>,
}

fn to_c(v: f64, unit: &str) -> f64 {
    if unit == "F" { (v - 32.0) * 5.0 / 9.0 } else { v }
}

/// Walk through a cold-chain excursion and, optionally, write a worksheet.
pub fn vaccine_temperature_excursion_helper(
    ctx: &Context,
    args: ExcursionArgs,
) -> Result<Envelope<ExcursionResult>> {
    let unit_key = match args.unit_type.trim().to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
        "refrigerator" | "fridge" | "ref" => "refrigerator",
        "freezer" => "freezer",
        "ultra_cold" | "ultracold" | "ultra_cold_freezer" | "ult" => "ultra_cold",
        other => {
            return Err(PhnError::InvalidInput(format!(
                "unit_type '{other}' is not recognized; use refrigerator, freezer, or ultra_cold"
            )));
        }
    };
    let tu = args.temp_unit.as_deref().unwrap_or("C").trim().to_ascii_uppercase();
    if tu != "C" && tu != "F" {
        return Err(PhnError::InvalidInput("temp_unit must be \"C\" or \"F\"".into()));
    }
    let min_c = args.min_temp.map(|v| to_c(v, &tu));
    let max_c = args.max_temp.map(|v| to_c(v, &tu));
    for v in [min_c, max_c].into_iter().flatten() {
        if !(-100.0..=60.0).contains(&v) {
            return Err(PhnError::InvalidInput(format!(
                "a temperature of {v:.1} C is outside what a storage unit could record; check the value and unit"
            )));
        }
    }
    if let (Some(a), Some(b)) = (min_c, max_c)
        && a > b
    {
        return Err(PhnError::InvalidInput("min_temp is higher than max_temp".into()));
    }
    let state = args.state_supplied.unwrap_or(true);
    let range = STORE
        .operating_ranges
        .iter()
        .find(|r| r.unit == unit_key)
        .ok_or_else(|| PhnError::Internal("operating range missing".into()))?;
    let thresholds = STORE
        .reporting_thresholds
        .iter()
        .find(|r| r.unit == unit_key)
        .ok_or_else(|| PhnError::Internal("thresholds missing".into()))?;
    let lo = range.range_c.first().copied().flatten();
    let hi = range.range_c.get(1).copied().flatten();
    let fmt_range = |a: Option<f64>, b: Option<f64>, u: &str| match (a, b) {
        (Some(a), Some(b)) => format!("{a} to {b} {u}"),
        (None, Some(b)) => format!("below {b} {u}"),
        (Some(a), None) => format!("above {a} {u}"),
        _ => "see package insert".into(),
    };
    let operating_range = format!(
        "{} ({})",
        fmt_range(lo, hi, "C"),
        fmt_range(range.range_f.first().copied().flatten(), range.range_f.get(1).copied().flatten(), "F")
    );

    let mut triggered = Vec::new();
    for t in &thresholds.immediate {
        let hit = match t.kind {
            ThresholdKind::AtOrAboveC => max_c.is_some_and(|m| m >= t.value),
            ThresholdKind::AtOrBelowC => min_c.is_some_and(|m| m <= t.value),
            ThresholdKind::BelowC => min_c.is_some_and(|m| m < t.value),
        };
        if hit {
            triggered.push(format!("Immediate: {}", t.text));
        }
    }
    let out_of_range = max_c.is_some_and(|m| hi.is_some_and(|h| m > h)) || min_c.is_some_and(|m| lo.is_some_and(|l| m < l));
    let classification = if !triggered.is_empty() {
        ExcursionClass::ReportableImmediate
    } else if min_c.is_none() && max_c.is_none() {
        ExcursionClass::Undetermined
    } else if !out_of_range {
        ExcursionClass::InRange
    } else {
        match (&thresholds.cumulative, args.minutes_out_of_range) {
            (Some(c), Some(m)) if m >= c.minutes => {
                triggered.push(format!("Cumulative: {}", c.text));
                ExcursionClass::ReportableCumulative
            }
            (Some(_), Some(_)) => ExcursionClass::OutOfRangeBelowReportingThreshold,
            (Some(_), None) => ExcursionClass::Undetermined,
            (None, _) => ExcursionClass::ReportableImmediate,
        }
    };

    let cdc_steps: Vec<String> = STORE.cdc_steps.iter().map(|s| s.text.clone()).collect();
    let dph_steps: Vec<String> = STORE.dph_steps.iter().map(|s| s.text.clone()).collect();
    let contacts: Vec<String> = STORE
        .contacts
        .iter()
        .map(|c| format!("{}: {} (email {}; also listed as {}); fax {}. {}", c.name, c.phone, c.email, c.alternate_email, c.fax, c.note))
        .collect();

    let mut summary = format!(
        "**Vaccine temperature excursion — {}**\n\nNormal range: {operating_range}. Readings: min {}, max {}{}.\n\n",
        unit_key.replace('_', "-"),
        min_c.map(|v| format!("{v:.1} C")).unwrap_or_else(|| "not given".into()),
        max_c.map(|v| format!("{v:.1} C")).unwrap_or_else(|| "not given".into()),
        args.minutes_out_of_range
            .map(|m| format!("; {m} cumulative minute(s) out of range in 24 hours"))
            .unwrap_or_default()
    );
    let verdict = match classification {
        ExcursionClass::ReportableImmediate => "**Report to the MA Vaccine Program immediately** (TempX / Temperature Excursion Issue in the MIIS) — the readings meet an immediate-report threshold.",
        ExcursionClass::ReportableCumulative => "**Reportable to the MA Vaccine Program** — out of range for 30 or more cumulative minutes in 24 hours.",
        ExcursionClass::OutOfRangeBelowReportingThreshold => "Out of range, but shorter than DPH's 30-minute reporting threshold. Keep monitoring the unit. CDC treats any reading outside the package-insert range as an excursion, so follow your SOP and check with the manufacturer if in doubt.",
        ExcursionClass::InRange => "The readings are within the normal operating range.",
        ExcursionClass::Undetermined => "Not enough information to compare with DPH's thresholds (give minimum and maximum temperatures and cumulative minutes out of range). Until then, treat it as an excursion: label the vaccines and keep them at the correct temperature.",
    };
    summary.push_str(verdict);
    summary.push('\n');
    if !state {
        summary.push_str("\nThese thresholds are the MA Vaccine Program's for state-supplied vaccine. For privately purchased vaccine, follow the manufacturer and your SOP.\n");
    }
    for t in &triggered {
        summary.push_str(&format!("- {t}\n"));
    }
    if unit_key == "refrigerator" && args.open_multidose_vials && out_of_range {
        if let Some(n) = &thresholds.note {
            summary.push_str(&format!("\nOpen multidose vials — DPH's guideline: {n} Confirm with the manufacturer and the Vaccine Program.\n"));
        }
    }
    if classification != ExcursionClass::InRange {
        summary.push_str("\n**Do now (CDC Vaccine Storage and Handling Toolkit)**\n");
        for (i, s) in cdc_steps.iter().enumerate() {
            summary.push_str(&format!("{}. {s}\n", i + 1));
        }
        if state {
            summary.push_str("\n**MA Vaccine Program requirements for state-supplied vaccine**\n");
            for (i, s) in dph_steps.iter().enumerate() {
                summary.push_str(&format!("{}. {s}\n", i + 1));
            }
        }
        summary.push_str(&format!("\n{}\n", STORE.power_failure));
    }
    summary.push_str(&format!("\n**Contact:** {}\n\n_{}_\n", contacts.join(" "), STORE.viability_statement));

    // Optional worksheet.
    let mut worksheet_file = None;
    let mut files_written = Vec::new();
    if let Some(fmt) = args.worksheet.as_deref().map(|f| f.trim().to_ascii_lowercase()).filter(|f| !f.is_empty() && f != "none") {
        if fmt != "xlsx" && fmt != "docx" {
            return Err(PhnError::InvalidInput("worksheet must be \"xlsx\" or \"docx\"".into()));
        }
        let event_rows = vec![
            vec!["Date and time excursion discovered".into(), args.discovered_at.clone().unwrap_or_default()],
            vec!["Storage unit type".into(), unit_key.replace('_', "-")],
            vec!["Normal operating range".into(), operating_range.clone()],
            vec!["Minimum temperature recorded".into(), args.min_temp.map(|v| format!("{v} {tu}")).unwrap_or_default()],
            vec!["Maximum temperature recorded".into(), args.max_temp.map(|v| format!("{v} {tu}")).unwrap_or_default()],
            vec!["Cumulative minutes out of range (24 hours)".into(), args.minutes_out_of_range.map(|m| m.to_string()).unwrap_or_default()],
            vec!["State-supplied vaccine in unit".into(), if state { "yes".into() } else { "no".into() }],
            vec!["Classification against MA Vaccine Program thresholds".into(), format!("{classification:?}")],
        ];
        let mut extra: Vec<Vec<String>> = STORE
            .worksheet_fields
            .iter()
            .filter(|f| !event_rows.iter().any(|r| r[0].starts_with(f.as_str())))
            .filter(|f| !["Vaccine name", "Manufacturer", "Lot number", "Expiration date", "Number of doses", "Funding (state-supplied/VFC or private)"].contains(&f.as_str()))
            .map(|f| vec![f.clone(), String::new()])
            .collect();
        let mut rows = event_rows;
        rows.append(&mut extra);
        let vaccine_rows: Vec<Vec<String>> = if args.vaccines.is_empty() {
            (0..5).map(|_| vec![String::new(); 8]).collect()
        } else {
            args.vaccines
                .iter()
                .map(|v| {
                    vec![
                        v.name.clone(),
                        v.manufacturer.clone().unwrap_or_default(),
                        v.lot.clone().unwrap_or_default(),
                        v.expiration.clone().unwrap_or_default(),
                        v.doses.map(|d| d.to_string()).unwrap_or_default(),
                        v.funding.clone().unwrap_or_default(),
                        String::new(),
                        String::new(),
                    ]
                })
                .collect()
        };
        let mut steps: Vec<Vec<String>> = cdc_steps.iter().map(|s| vec![format!("CDC: {s}"), String::new()]).collect();
        if state {
            steps.extend(dph_steps.iter().map(|s| vec![format!("MA Vaccine Program: {s}"), String::new()]));
        }
        let doc = Document {
            title: "Vaccine temperature excursion worksheet".into(),
            subtitle: Some("Based on the CDC Vaccine Storage and Handling Toolkit and the MA Vaccine Program guidelines".into()),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                Block::Paragraph(vec![Inline::plain(STORE.viability_statement.clone())]),
                Block::Heading { level: 2, text: "Event".into() },
                Block::Table(TableBlock { caption: Some("Event".into()), headers: vec!["Item".into(), "Details".into()], rows }),
                Block::Heading { level: 2, text: "Affected vaccines".into() },
                Block::Table(TableBlock {
                    caption: Some("Affected vaccines".into()),
                    headers: ["Vaccine", "Manufacturer", "Lot", "Expiration", "Doses", "Funding", "Manufacturer determination", "Final disposition"].map(String::from).to_vec(),
                    rows: vaccine_rows,
                }),
                Block::Heading { level: 2, text: "Steps".into() },
                Block::Table(TableBlock { caption: Some("Steps".into()), headers: vec!["Step".into(), "Done (time, initials)".into()], rows: steps }),
            ],
        };
        let bytes = if fmt == "xlsx" { phn_docs::xlsx::to_xlsx(&doc)? } else { phn_docs::docx::to_docx(&doc)? };
        let sandbox = ctx.sandbox()?;
        let path = sandbox.new_output_file(&format!("temperature_excursion_worksheet.{fmt}"))?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        summary.push_str(&format!("\nWorksheet written to `{disp}`.\n"));
        files_written.push(disp.clone());
        worksheet_file = Some(disp);
    }

    let mut sources = vec![content::content_source(content::STORAGE)?];
    if let Ok(m) = content::meta(content::STORAGE) {
        for r in m.related_sources.iter().take(3) {
            sources.push(phn_core::Source {
                title: r.title.clone(),
                publisher: r.publisher.clone(),
                url: r.url.clone(),
                effective_date: None,
                retrieved: Some(m.prepared.clone()),
                mode: phn_core::SourceMode::Snapshot,
            });
        }
    }
    let mut caveats = vec![
        STORE.viability_statement.clone(),
        "Temperature ranges and reporting thresholds can differ by product (for example some COVID-19, mpox, and RSV products); always check the package insert or EUA fact sheet.".to_string(),
    ];
    caveats.extend(content::review_caveats(&[content::STORAGE]));
    Ok(Envelope::new(
        summary,
        ExcursionResult {
            unit_type: unit_key.into(),
            min_temp_c: min_c.map(|v| (v * 10.0).round() / 10.0),
            max_temp_c: max_c.map(|v| (v * 10.0).round() / 10.0),
            operating_range,
            classification,
            triggered,
            cdc_steps,
            dph_steps,
            contacts,
            viability: STORE.viability_statement.clone(),
            worksheet_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        files_written,
        ..Privacy::none()
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn classify(ctx: &Context, unit: &str, min: Option<f64>, max: Option<f64>, minutes: Option<u32>) -> ExcursionClass {
        vaccine_temperature_excursion_helper(
            ctx,
            ExcursionArgs {
                unit_type: unit.into(),
                min_temp: min,
                max_temp: max,
                minutes_out_of_range: minutes,
                ..Default::default()
            },
        )
        .unwrap()
        .data
        .classification
    }

    #[test]
    fn thresholds_follow_the_ma_vaccine_program_guidelines() {
        let (_d, ctx) = ctx();
        use ExcursionClass::*;
        // Refrigerator: 0 C or 25 C and above are immediate; otherwise 30 cumulative minutes.
        assert_eq!(classify(&ctx, "refrigerator", Some(0.0), Some(5.0), None), ReportableImmediate);
        assert_eq!(classify(&ctx, "refrigerator", Some(3.0), Some(25.0), Some(1)), ReportableImmediate);
        assert_eq!(classify(&ctx, "refrigerator", Some(3.0), Some(12.0), Some(30)), ReportableCumulative);
        assert_eq!(classify(&ctx, "refrigerator", Some(3.0), Some(12.0), Some(29)), OutOfRangeBelowReportingThreshold);
        assert_eq!(classify(&ctx, "refrigerator", Some(1.5), Some(8.0), None), Undetermined);
        assert_eq!(classify(&ctx, "refrigerator", Some(2.0), Some(8.0), None), InRange);
        // Freezer: 0 C and above, or below -50 C, are immediate.
        assert_eq!(classify(&ctx, "freezer", Some(-20.0), Some(0.0), None), ReportableImmediate);
        assert_eq!(classify(&ctx, "freezer", Some(-51.0), Some(-20.0), None), ReportableImmediate);
        assert_eq!(classify(&ctx, "freezer", Some(-50.0), Some(-12.0), Some(45)), ReportableCumulative);
        // Ultra-cold: -50 C or above is immediate.
        assert_eq!(classify(&ctx, "ultra_cold", Some(-80.0), Some(-50.0), None), ReportableImmediate);
        assert_eq!(classify(&ctx, "ultra_cold", Some(-80.0), Some(-60.0), None), InRange);
        assert_eq!(classify(&ctx, "freezer", None, None, None), Undetermined);
    }

    #[test]
    fn fahrenheit_is_converted() {
        assert!((to_c(46.0, "F") - 7.777_777).abs() < 1e-4);
        assert!((to_c(-58.0, "F") + 50.0).abs() < 1e-9);
        assert_eq!(to_c(5.0, "C"), 5.0);
    }
}
