//! CHR-02 `community_bp_screening_toolkit`: a community blood pressure screening kit built
//! from bundled content. It gives the measurement technique checklist, the current ACC/AHA
//! blood pressure categories for adults (the 2025 guideline retired and replaced the 2017 one),
//! the severe-range wording and this project's referral tiers (labelled "tool default, pending
//! review"), a screening log template, follow-up letter templates, and a plain-language client
//! handout (checked with the COMM-01 readability engine). It classifies readings typed into the
//! request (numbers only, no person). A local screening log the nurse supplies is summarized
//! only as suppressed counts by blood pressure category, age group, referral status, and number
//! of readings; follow-up letters filled from the log's rows go to /data/output only
//! (template + local fill, ADR 0008).

use std::collections::{BTreeMap, BTreeSet};

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::screen_common::{
    AGE_GROUPS, AGE_MISSING, CountTable, MAX_AGE, age_group, count_table, find_column, tally,
};
use crate::comms::readability::{AnalyzeOptions, analyze, default_target_grade};
use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/chronic/bp_screening.yaml`.
pub const BP_SCREENING: &str = "chronic-bp-screening";

/// Blank written into a letter or handout where a value is not known.
const BLANK: &str = "____________";

/// Label for a row whose age is under 18.
const UNDER_18: &str = "Under 18 (adult categories do not apply)";
const NOT_CLASSIFIABLE: &str = "No usable reading";

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryDef {
    pub id: String,
    pub label: String,
    pub letter_label: String,
    pub systolic: String,
    pub diastolic: String,
    pub meaning: String,
    pub plain: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PregnancyDef {
    pub note: String,
    pub high_systolic: i64,
    pub high_diastolic: i64,
    pub severe_systolic: i64,
    pub severe_diastolic: i64,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierDef {
    pub id: String,
    pub applies_to: String,
    pub action: String,
    pub symptoms_note: Option<String>,
    pub status: String,
    pub confidence: Confidence,
    pub log_value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TechniqueDef {
    pub group: String,
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingRules {
    pub average_note: String,
    pub plausible_systolic: [i64; 2],
    pub plausible_diastolic: [i64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogColumn {
    pub name: String,
    pub description: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogTemplateDef {
    pub title: String,
    pub instructions: Vec<String>,
    pub columns: Vec<LogColumn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetterDef {
    pub kind: String,
    pub description: String,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LettersDef {
    pub placeholders: Vec<String>,
    pub routine: LetterDef,
    pub refer: LetterDef,
    pub urgent: LetterDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoutDef {
    pub id: String,
    pub title: String,
    pub sources: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notices {
    pub decision_support: String,
    pub copyright: String,
}

/// `content/chronic/bp_screening.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BpContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub guideline_note: String,
    pub categories: Vec<CategoryDef>,
    pub pregnancy: PregnancyDef,
    pub severe_range_wording: Vec<Cited>,
    pub referral_tiers: Vec<TierDef>,
    pub referral_status_values: Vec<String>,
    pub technique: Vec<TechniqueDef>,
    pub reading_rules: ReadingRules,
    pub log_template: LogTemplateDef,
    pub letters: LettersDef,
    pub handout: HandoutDef,
    pub notices: Notices,
}

// ---------------------------------------------------------------- arguments and results

/// One blood pressure reading (mm Hg).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct BpReading {
    /// Systolic (top) number.
    pub systolic: f64,
    /// Diastolic (bottom) number.
    pub diastolic: f64,
}

/// Arguments for `community_bp_screening_toolkit`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct BpToolkitArgs {
    /// Readings to classify (numbers only; no name or other detail). Two or more readings are
    /// averaged. Adults only.
    pub readings: Vec<BpReading>,
    /// Age in years of the person the readings are from, if you want the check that the adult
    /// categories apply (they do not for under 18).
    pub age: Option<f64>,
    /// True when the person is pregnant: the adult categories are not used; CDC's pregnancy
    /// thresholds (140/90 and 160/110) are shown instead.
    pub pregnant: Option<bool>,
    /// True when the person has symptoms (chest pain, trouble breathing, sudden severe
    /// headache, and similar): changes the referral tier for a severe-range reading.
    pub symptoms: Option<bool>,
    /// A screening log CSV/XLSX in /data (one row per screening: age, systolic and diastolic
    /// readings, referral status). Only suppressed counts by category, age group, referral
    /// status, and number of readings are returned.
    pub log_path: Option<String>,
    /// Sheet name when log_path is an XLSX workbook (default: the first sheet).
    pub sheet: Option<String>,
    /// Fill follow-up letters from the rows of log_path (needs a name column; address and town
    /// columns are used when present) and write them to /data/output only (default false).
    pub write_letters: Option<bool>,
    /// Write the blank kit to /data/output: log template (XLSX and CSV), technique checklist,
    /// reference card, letter templates, and client handout. Default: true when neither
    /// readings nor log_path is given.
    pub write_documents: Option<bool>,
    /// Document format for the checklist, reference card, letters, and handout: "docx"
    /// (default) or "pdf".
    pub format: Option<DocFormat>,
}

/// A category as returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoryOut {
    pub id: String,
    pub label: String,
    pub systolic: String,
    pub diastolic: String,
    pub meaning: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// A referral tier (this project's default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TierOut {
    pub id: String,
    pub applies_to: String,
    pub action: String,
    pub symptoms_note: Option<String>,
    /// Always "tool default, pending review".
    pub status: String,
    pub confidence: Confidence,
    /// The word to use for Referral status in the log.
    pub log_value: String,
}

/// One technique step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TechniqueOut {
    pub group: String,
    pub text: String,
    pub citation: String,
    pub confidence: Confidence,
    /// False when the step was not found at a source and is the project's draft.
    pub verified: bool,
}

/// The severe range and pregnancy thresholds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SevereRange {
    /// The rule this kit applies (tool default for the any-reading rule).
    pub rule: String,
    /// What the sources say, in their own words.
    pub source_wording: Vec<CitedOut>,
    pub pregnancy: String,
}

/// The classification of readings typed into the request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReadingResult {
    pub readings_used: usize,
    pub average_systolic: f64,
    pub average_diastolic: f64,
    /// Average rounded to whole mm Hg (used for the category).
    pub rounded: String,
    /// Category id (normal, elevated, stage1, stage2, severe), or a pregnancy band.
    pub category_id: String,
    pub category: String,
    pub meaning: String,
    /// True when any single reading is above 180 or above 120 (above 160 / 110 in pregnancy).
    pub any_reading_in_severe_range: bool,
    pub pregnant: bool,
    pub referral: TierOut,
    pub note: String,
}

/// A file written to /data/output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KitFile {
    pub what: String,
    pub file: String,
    /// Median reading grade (COMM-01 engine), for letters and the handout.
    pub reading_grade: Option<f64>,
    pub meets_target: Option<bool>,
}

/// The screening log template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogTemplateOut {
    pub title: String,
    pub instructions: Vec<String>,
    pub columns: Vec<LogColumn>,
    pub referral_status_values: Vec<String>,
}

/// A letter template the nurse can review (filled only from a local file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LetterTemplateOut {
    pub kind: String,
    pub description: String,
    pub placeholders: Vec<String>,
    pub markdown: String,
}

/// Aggregates from a local screening log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogSummary {
    pub source_file: String,
    /// Which logical columns were found (for example "systolic 1", "age", "referral status").
    pub columns_found: Vec<String>,
    /// Suppressed one-way count tables.
    pub tables: Vec<CountTable>,
    pub suppression_threshold: u32,
    pub notes: Vec<String>,
}

/// Letters filled from a local screening log (counts and file names only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LettersWritten {
    pub source_file: String,
    pub folder: String,
    pub file: String,
    /// Letters written (one per row with a name and a usable reading).
    pub letters: usize,
    /// Rows that got no letter (no name, no usable reading, or under 18): the count, or
    /// "fewer than N" when it is small.
    pub rows_without_letter: String,
    /// Template fields left blank because the file had no value.
    pub blank_fields: usize,
    pub fields_found: Vec<String>,
}

/// Result of `community_bp_screening_toolkit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BpToolkitResult {
    /// Which guideline the categories come from and how much of it was verified.
    pub guideline: String,
    pub categories: Vec<CategoryOut>,
    pub severe_range: SevereRange,
    /// Referral tiers: this project's defaults, pending review.
    pub referral_tiers: Vec<TierOut>,
    pub technique: Vec<TechniqueOut>,
    pub reading: Option<ReadingResult>,
    pub log_template: LogTemplateOut,
    pub letter_templates: Vec<LetterTemplateOut>,
    pub handout_title: String,
    pub log_summary: Option<LogSummary>,
    pub letters: Option<LettersWritten>,
    pub kit_folder: Option<String>,
    pub files: Vec<KitFile>,
    pub decision_support_notice: String,
}

// ---------------------------------------------------------------- classification

/// Adult categories, lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cat {
    Normal,
    Elevated,
    Stage1,
    Stage2,
    Severe,
}

impl Cat {
    pub fn id(self) -> &'static str {
        match self {
            Cat::Normal => "normal",
            Cat::Elevated => "elevated",
            Cat::Stage1 => "stage1",
            Cat::Stage2 => "stage2",
            Cat::Severe => "severe",
        }
    }
}

/// Category of an average reading in whole mm Hg: the higher category applies when the two
/// numbers fall in different ones. Severe range is above 180 or above 120.
pub fn classify(systolic: i64, diastolic: i64) -> Cat {
    if systolic > 180 || diastolic > 120 {
        Cat::Severe
    } else if systolic >= 140 || diastolic >= 90 {
        Cat::Stage2
    } else if systolic >= 130 || diastolic >= 80 {
        Cat::Stage1
    } else if systolic >= 120 {
        Cat::Elevated
    } else {
        Cat::Normal
    }
}

/// Letter tier for a category.
fn letter_tier(c: Cat) -> &'static str {
    match c {
        Cat::Normal | Cat::Elevated => "routine",
        Cat::Stage1 | Cat::Stage2 => "refer",
        Cat::Severe => "urgent",
    }
}

fn plausible(c: &BpContent, sys: f64, dia: f64) -> bool {
    let (sl, sh) = (
        c.reading_rules.plausible_systolic[0] as f64,
        c.reading_rules.plausible_systolic[1] as f64,
    );
    let (dl, dh) = (
        c.reading_rules.plausible_diastolic[0] as f64,
        c.reading_rules.plausible_diastolic[1] as f64,
    );
    sys.is_finite()
        && dia.is_finite()
        && (sl..=sh).contains(&sys)
        && (dl..=dh).contains(&dia)
        && dia < sys
}

fn cat_def<'a>(c: &'a BpContent, cat: Cat) -> &'a CategoryDef {
    c.categories
        .iter()
        .find(|d| d.id == cat.id())
        .expect("content has every category (checked by the unit test)")
}

fn tier_def<'a>(c: &'a BpContent, id: &str) -> &'a TierDef {
    c.referral_tiers
        .iter()
        .find(|t| t.id == id)
        .expect("content has every referral tier (checked by the unit test)")
}

fn tier_out(t: &TierDef) -> TierOut {
    TierOut {
        id: t.id.clone(),
        applies_to: t.applies_to.clone(),
        action: t.action.clone(),
        symptoms_note: t.symptoms_note.clone(),
        status: t.status.clone(),
        confidence: t.confidence,
        log_value: t.log_value.clone(),
    }
}

/// Referral tier for an adult category (symptoms change only the severe range).
fn adult_tier<'a>(c: &'a BpContent, cat: Cat, symptoms: bool) -> &'a TierDef {
    tier_def(
        c,
        match cat {
            Cat::Severe if symptoms => "emergency",
            Cat::Severe => "same_day",
            Cat::Stage2 => "refer",
            Cat::Stage1 => "advise",
            Cat::Normal | Cat::Elevated => "inform",
        },
    )
}

fn classify_readings(c: &BpContent, args: &BpToolkitArgs) -> Result<ReadingResult> {
    let pregnant = args.pregnant.unwrap_or(false);
    if let Some(a) = args.age {
        if !(0.0..=MAX_AGE).contains(&a) || !a.is_finite() {
            return Err(PhnError::InvalidInput(format!(
                "age must be between 0 and {MAX_AGE} years"
            )));
        }
        if a < 18.0 {
            return Err(PhnError::InvalidInput(
                "the ACC/AHA categories in this kit are for adults (the guideline covers adults); \
                 blood pressure in children and teenagers is judged differently, so this tool \
                 does not classify it"
                    .into(),
            ));
        }
    }
    for (i, r) in args.readings.iter().enumerate() {
        if !plausible(c, r.systolic, r.diastolic) {
            return Err(PhnError::InvalidInput(format!(
                "reading {} is not plausible ({} over {}): systolic must be {}-{} mm Hg, \
                 diastolic {}-{}, and the diastolic number must be lower than the systolic one",
                i + 1,
                r.systolic,
                r.diastolic,
                c.reading_rules.plausible_systolic[0],
                c.reading_rules.plausible_systolic[1],
                c.reading_rules.plausible_diastolic[0],
                c.reading_rules.plausible_diastolic[1],
            )));
        }
    }
    let n = args.readings.len();
    let avg_s = args.readings.iter().map(|r| r.systolic).sum::<f64>() / n as f64;
    let avg_d = args.readings.iter().map(|r| r.diastolic).sum::<f64>() / n as f64;
    let (rs, rd) = (avg_s.round() as i64, avg_d.round() as i64);
    let symptoms = args.symptoms.unwrap_or(false);
    let mut note = c.reading_rules.average_note.clone();
    if n == 1 {
        note.push_str(
            " Only one reading was given: the guideline and CDC advise more than one reading \
             (CDC: at least two, 1 or 2 minutes apart), so repeat it before acting on a \
             borderline result.",
        );
    }
    if pregnant {
        let p = &c.pregnancy;
        let sev = |s: f64, d: f64| s >= p.severe_systolic as f64 || d >= p.severe_diastolic as f64;
        let any = args.readings.iter().any(|r| sev(r.systolic, r.diastolic));
        let (id, label, tier) = if rs >= p.severe_systolic || rd >= p.severe_diastolic || any {
            (
                "pregnancy_severe",
                format!(
                    "Severe range in pregnancy ({} / {} or higher)",
                    p.severe_systolic, p.severe_diastolic
                ),
                if symptoms { "emergency" } else { "same_day" },
            )
        } else if rs >= p.high_systolic || rd >= p.high_diastolic {
            (
                "pregnancy_high",
                format!(
                    "High in pregnancy ({} / {} or higher)",
                    p.high_systolic, p.high_diastolic
                ),
                "refer",
            )
        } else {
            (
                "pregnancy_below_140_90",
                format!(
                    "Below {} / {} (pregnancy)",
                    p.high_systolic, p.high_diastolic
                ),
                "inform",
            )
        };
        note.push(' ');
        note.push_str(&p.note);
        return Ok(ReadingResult {
            readings_used: n,
            average_systolic: round1(avg_s),
            average_diastolic: round1(avg_d),
            rounded: format!("{rs}/{rd}"),
            category_id: id.into(),
            category: label,
            meaning: p.note.clone(),
            any_reading_in_severe_range: any,
            pregnant: true,
            referral: tier_out(tier_def(c, tier)),
            note,
        });
    }
    let mut cat = classify(rs, rd);
    let any_severe = args
        .readings
        .iter()
        .any(|r| classify(r.systolic.round() as i64, r.diastolic.round() as i64) == Cat::Severe);
    if any_severe {
        cat = Cat::Severe;
    }
    let d = cat_def(c, cat);
    Ok(ReadingResult {
        readings_used: n,
        average_systolic: round1(avg_s),
        average_diastolic: round1(avg_d),
        rounded: format!("{rs}/{rd}"),
        category_id: cat.id().into(),
        category: d.label.clone(),
        meaning: d.meaning.clone(),
        any_reading_in_severe_range: any_severe,
        pregnant: false,
        referral: tier_out(adult_tier(c, cat, symptoms)),
        note,
    })
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------- local log

const SYS1: &[&str] = &[
    "systolic 1",
    "systolic1",
    "sbp 1",
    "sbp1",
    "sys 1",
    "systolic reading 1",
    "reading 1 systolic",
    "systolic",
    "sbp",
    "systolic bp",
    "sys",
    "bp systolic",
];
const DIA1: &[&str] = &[
    "diastolic 1",
    "diastolic1",
    "dbp 1",
    "dbp1",
    "dia 1",
    "diastolic reading 1",
    "reading 1 diastolic",
    "diastolic",
    "dbp",
    "diastolic bp",
    "dia",
    "bp diastolic",
];
const SYS2: &[&str] = &[
    "systolic 2",
    "systolic2",
    "sbp 2",
    "sbp2",
    "sys 2",
    "systolic reading 2",
    "reading 2 systolic",
];
const DIA2: &[&str] = &[
    "diastolic 2",
    "diastolic2",
    "dbp 2",
    "dbp2",
    "dia 2",
    "diastolic reading 2",
    "reading 2 diastolic",
];
const BP1: &[&str] = &[
    "blood pressure",
    "bp",
    "bp reading",
    "bp 1",
    "bp1",
    "reading 1",
    "blood pressure 1",
];
const BP2: &[&str] = &["bp 2", "bp2", "reading 2", "blood pressure 2", "second reading"];
const AGE: &[&str] = &["age", "age years", "age in years"];
const REFERRAL: &[&str] = &[
    "referral status",
    "referral",
    "referred",
    "referral made",
    "disposition",
];
const NAME: &[&str] = &[
    "name",
    "participant name",
    "full name",
    "participant",
    "client name",
    "resident",
];
const ADDRESS: &[&str] = &["address", "street address", "street", "mailing address"];
const TOWN: &[&str] = &["town", "city", "city or town", "municipality"];
const DATE: &[&str] = &["screening date", "date of screening", "visit date", "date"];
const SITE: &[&str] = &["site", "location", "screening site", "event"];

/// "128/82" -> (128, 82).
fn parse_combined(s: &str) -> Option<(f64, f64)> {
    let (a, b) = s.trim().split_once('/')?;
    let a = a.trim().parse::<f64>().ok()?;
    let b = b.trim().parse::<f64>().ok()?;
    Some((a, b))
}

/// What one log row contributes.
struct RowBp {
    /// None when no usable reading.
    cat: Option<Cat>,
    avg: Option<(i64, i64)>,
    readings: usize,
    under18: bool,
}

#[allow(clippy::too_many_arguments)]
fn row_bp(
    c: &BpContent,
    lt: &crate::localdata::LocalTable,
    row: usize,
    cols: &LogCols,
    sys1: &[Option<f64>],
    dia1: &[Option<f64>],
    sys2: &[Option<f64>],
    dia2: &[Option<f64>],
    ages: &[Option<f64>],
) -> RowBp {
    // Candidate readings (systolic, diastolic), each possibly incomplete.
    let mut candidates: Vec<(Option<f64>, Option<f64>)> = Vec::new();
    let mut bad = false;
    if cols.sys1.is_some() && cols.dia1.is_some() {
        candidates.push((sys1[row], dia1[row]));
    }
    if cols.sys2.is_some() && cols.dia2.is_some() {
        candidates.push((sys2[row], dia2[row]));
    }
    for idx in [cols.bp1, cols.bp2].into_iter().flatten() {
        let cell = lt.table.rows[row].get(idx).map(String::as_str).unwrap_or("");
        if cell.trim().is_empty() {
            continue;
        }
        match parse_combined(cell) {
            Some((s, d)) => candidates.push((Some(s), Some(d))),
            None => bad = true,
        }
    }
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    for (s, d) in candidates {
        match (s, d) {
            (Some(s), Some(d)) if plausible(c, s, d) => pairs.push((s, d)),
            (None, None) => {}
            _ => bad = true,
        }
    }
    let under18 = ages
        .get(row)
        .copied()
        .flatten()
        .is_some_and(|a| (0.0..18.0).contains(&a));
    if bad || pairs.is_empty() {
        return RowBp {
            cat: None,
            avg: None,
            readings: 0,
            under18,
        };
    }
    let n = pairs.len() as f64;
    let avg_s = pairs.iter().map(|p| p.0).sum::<f64>() / n;
    let avg_d = pairs.iter().map(|p| p.1).sum::<f64>() / n;
    let (rs, rd) = (avg_s.round() as i64, avg_d.round() as i64);
    let mut cat = classify(rs, rd);
    if pairs
        .iter()
        .any(|p| classify(p.0.round() as i64, p.1.round() as i64) == Cat::Severe)
    {
        cat = Cat::Severe;
    }
    RowBp {
        cat: Some(cat),
        avg: Some((rs, rd)),
        readings: pairs.len(),
        under18,
    }
}

/// Column indexes found in the log.
struct LogCols {
    sys1: Option<usize>,
    dia1: Option<usize>,
    sys2: Option<usize>,
    dia2: Option<usize>,
    bp1: Option<usize>,
    bp2: Option<usize>,
    age: Option<usize>,
    referral: Option<usize>,
}

fn locate_columns(lt: &crate::localdata::LocalTable) -> Result<LogCols> {
    let h = &lt.table.headers;
    // Each column that is read goes through the identifier guardrail.
    let checked = |idx: Option<usize>| -> Result<Option<usize>> {
        match idx {
            Some(i) => lt.column(&h[i]).map(Some),
            None => Ok(None),
        }
    };
    let cols = LogCols {
        sys1: checked(find_column(h, SYS1))?,
        dia1: checked(find_column(h, DIA1))?,
        sys2: checked(find_column(h, SYS2))?,
        dia2: checked(find_column(h, DIA2))?,
        bp1: checked(find_column(h, BP1))?,
        bp2: checked(find_column(h, BP2))?,
        age: checked(find_column(h, AGE))?,
        referral: checked(find_column(h, REFERRAL))?,
    };
    let has_pair = cols.sys1.is_some() && cols.dia1.is_some();
    if !has_pair && cols.bp1.is_none() {
        return Err(PhnError::InvalidInput(format!(
            "'{}' has no blood pressure readings the tool can use. Add systolic and diastolic \
             columns (for example Systolic 1, Diastolic 1, Systolic 2, Diastolic 2) or a Blood \
             pressure column written like 128/82. The columns in the file are: {}",
            lt.label,
            h.join(", ")
        )));
    }
    Ok(cols)
}

fn summarize_log(
    c: &BpContent,
    lt: &crate::localdata::LocalTable,
    threshold: u32,
) -> Result<(LogSummary, LogCols)> {
    let cols = locate_columns(lt)?;
    let n = lt.table.rows.len();
    let numeric = |idx: Option<usize>| -> Vec<Option<f64>> {
        match idx {
            Some(i) => lt.numeric(i).0,
            None => vec![None; n],
        }
    };
    let (sys1, dia1, sys2, dia2, ages) = (
        numeric(cols.sys1),
        numeric(cols.dia1),
        numeric(cols.sys2),
        numeric(cols.dia2),
        numeric(cols.age),
    );
    let referrals: Option<Vec<Option<String>>> = match cols.referral {
        Some(i) => Some(lt.categorical(i)?),
        None => None,
    };
    let mut by_cat: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_age: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_ref: BTreeMap<String, u64> = BTreeMap::new();
    let mut ref_labels: BTreeMap<String, String> = BTreeMap::new();
    let mut by_n: BTreeMap<String, u64> = BTreeMap::new();
    let mut counted = 0usize;
    for r in 0..n {
        if lt.table.rows[r].iter().all(|v| v.trim().is_empty()) {
            continue;
        }
        counted += 1;
        let rb = row_bp(c, lt, r, &cols, &sys1, &dia1, &sys2, &dia2, &ages);
        let label = if rb.under18 {
            UNDER_18.to_string()
        } else {
            match rb.cat {
                Some(cat) => cat_def(c, cat).label.clone(),
                None => NOT_CLASSIFIABLE.to_string(),
            }
        };
        *by_cat.entry(label).or_default() += 1;
        let basis = match rb.readings {
            0 => "No usable reading",
            1 => "One reading",
            _ => "Two or more readings",
        };
        *by_n.entry(basis.to_string()).or_default() += 1;
        let ag = match ages.get(r).copied().flatten() {
            Some(a) if (0.0..=MAX_AGE).contains(&a) => age_group(a).to_string(),
            _ => AGE_MISSING.to_string(),
        };
        *by_age.entry(ag).or_default() += 1;
        if let Some(refs) = &referrals {
            let raw = refs[r].clone().unwrap_or_default();
            let key = phn_core::text::normalize(&raw);
            let (key, shown) = if key.is_empty() {
                ("not recorded".to_string(), "Not recorded".to_string())
            } else {
                (key, raw.trim().to_string())
            };
            ref_labels.entry(key.clone()).or_insert(shown);
            *by_ref.entry(key).or_default() += 1;
        }
    }
    if counted == 0 {
        return Err(PhnError::InvalidInput(format!(
            "'{}' has no screening rows",
            lt.label
        )));
    }
    let mut cat_order: Vec<&str> = c.categories.iter().map(|d| d.label.as_str()).collect();
    cat_order.push(UNDER_18);
    cat_order.push(NOT_CLASSIFIABLE);
    let mut tables = vec![count_table(
        "Blood pressure category (ACC/AHA adult categories, from the readings)",
        &tally(&cat_order, &by_cat),
        threshold,
    )];
    let mut age_order: Vec<&str> = AGE_GROUPS.to_vec();
    age_order.push(AGE_MISSING);
    let age_counts: Vec<(String, u64)> = tally(&age_order, &by_age)
        .into_iter()
        .filter(|(l, n)| *n > 0 || l != AGE_MISSING)
        .collect();
    if cols.age.is_some() {
        tables.push(count_table("Age group", &age_counts, threshold));
    }
    if referrals.is_some() {
        let counts: Vec<(String, u64)> = by_ref
            .iter()
            .map(|(k, n)| (ref_labels[k].clone(), *n))
            .collect();
        tables.push(count_table("Referral status", &counts, threshold));
    }
    tables.push(count_table(
        "Readings used for the category",
        &tally(
            &["Two or more readings", "One reading", "No usable reading"],
            &by_n,
        ),
        threshold,
    ));
    let mut found = Vec::new();
    for (name, idx) in [
        ("systolic 1", cols.sys1),
        ("diastolic 1", cols.dia1),
        ("systolic 2", cols.sys2),
        ("diastolic 2", cols.dia2),
        ("blood pressure (combined)", cols.bp1),
        ("blood pressure 2 (combined)", cols.bp2),
        ("age", cols.age),
        ("referral status", cols.referral),
    ] {
        if idx.is_some() {
            found.push(name.to_string());
        }
    }
    let mut notes = vec![
        "Counts from 1 to 4 and cells that would reveal them are shown as * (suppression \
         threshold from your settings). Tables are one-way on purpose: no table crosses two \
         variables, so a small cell cannot be worked out from another table."
            .to_string(),
        "Categories are worked out from the readings (a category column in your file is not \
         used) with the same rule for every row; the adult categories do not apply to \
         participants under 18, and pregnancy is not recognized in a log."
            .to_string(),
    ];
    if cols.age.is_none() {
        notes.push("The file has no age column, so there is no age group table.".into());
    }
    if referrals.is_none() {
        notes.push("The file has no referral status column, so there is no referral table.".into());
    }
    Ok((
        LogSummary {
            source_file: lt.label.clone(),
            columns_found: found,
            tables,
            suppression_threshold: threshold,
            notes,
        },
        cols,
    ))
}

// ---------------------------------------------------------------- documents

fn sample_values() -> BTreeMap<String, String> {
    let mut v = BTreeMap::new();
    for (k, val) in [
        ("date", "2026-11-12"),
        ("participant_name", "Participant"),
        ("address", "1 Main Street"),
        ("town", "Anytown"),
        ("screening_date", "2026-11-05"),
        ("site", "Community Center"),
        ("systolic", "142"),
        ("diastolic", "88"),
        ("category_label", "high (stage 2 range)"),
        (
            "category_plain",
            "Your blood pressure is high. Please see your health care team soon to check it again.",
        ),
        ("program_phone", "413-555-0100"),
    ] {
        v.insert(k.to_string(), val.to_string());
    }
    v
}

fn grade_of(markdown: &str) -> (f64, bool) {
    let target = default_target_grade();
    let a = analyze(
        markdown,
        &AnalyzeOptions {
            target_grade: target,
            max_flags: 3,
            redact_excerpts: false,
        },
    );
    (a.grade.median_grade, a.grade.median_grade <= target)
}

fn technique_markdown(c: &BpContent) -> String {
    let mut s = String::from("# Blood pressure measurement checklist\n\n");
    s.push_str(
        "Check each step before you record a reading. Steps marked (not verified) are standard \
         practice that this project could not confirm at a source.\n\n",
    );
    let mut groups: Vec<&str> = Vec::new();
    for t in &c.technique {
        if !groups.contains(&t.group.as_str()) {
            groups.push(&t.group);
        }
    }
    for g in groups {
        s.push_str(&format!("## {g}\n\n"));
        for t in c.technique.iter().filter(|t| t.group == g) {
            s.push_str(&format!(
                "- [ ] {}{}\n",
                t.text,
                if t.verified { "" } else { " (not verified)" }
            ));
        }
        s.push('\n');
    }
    s
}

fn reference_document(c: &BpContent, ctx: &Context) -> Document {
    let mut blocks: Vec<Block> = Vec::new();
    blocks.push(Block::Paragraph(vec![Inline::plain(
        c.notices.decision_support.split_whitespace().collect::<Vec<_>>().join(" "),
    )]));
    blocks.push(Block::Heading {
        level: 2,
        text: "Blood pressure categories for adults (ACC/AHA)".into(),
    });
    blocks.push(Block::Table(TableBlock {
        caption: Some("Categories".into()),
        headers: vec![
            "Category".into(),
            "Systolic (top number)".into(),
            "Diastolic (bottom number)".into(),
            "Confidence".into(),
        ],
        rows: c
            .categories
            .iter()
            .map(|d| {
                vec![
                    d.label.clone(),
                    d.systolic.clone(),
                    d.diastolic.clone(),
                    format!("{:?}", d.confidence).to_lowercase(),
                ]
            })
            .collect(),
    }));
    blocks.push(Block::Paragraph(vec![Inline::plain(
        c.guideline_note.split_whitespace().collect::<Vec<_>>().join(" "),
    )]));
    blocks.push(Block::Heading {
        level: 2,
        text: "Referral tiers (tool default, pending review)".into(),
    });
    blocks.push(Block::Table(TableBlock {
        caption: Some("Referral tiers".into()),
        headers: vec!["Applies to".into(), "Action".into(), "Log word".into()],
        rows: c
            .referral_tiers
            .iter()
            .map(|t| vec![t.applies_to.clone(), t.action.clone(), t.log_value.clone()])
            .collect(),
    }));
    blocks.push(Block::Paragraph(vec![Inline::plain(
        c.pregnancy.note.split_whitespace().collect::<Vec<_>>().join(" "),
    )]));
    Document {
        title: "Blood pressure screening reference card".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

fn log_template_document(c: &BpContent, ctx: &Context) -> Document {
    let t = &c.log_template;
    let mut blocks = vec![
        Block::Paragraph(vec![Inline::plain(
            "Instructions (kept on the Notes sheet): use one row per screening visit.",
        )]),
        Block::Table(TableBlock {
            caption: Some("Screening log".into()),
            headers: t.columns.iter().map(|x| x.name.clone()).collect(),
            rows: (0..30)
                .map(|_| vec![String::new(); t.columns.len()])
                .collect(),
        }),
        Block::Heading {
            level: 2,
            text: "How to fill in the log".into(),
        },
        Block::List {
            ordered: false,
            items: t
                .instructions
                .iter()
                .map(|i| vec![Inline::plain(i.clone())])
                .collect(),
        },
        Block::Table(TableBlock {
            caption: Some("Columns".into()),
            headers: vec!["Column".into(), "Group".into(), "What to enter".into()],
            rows: t
                .columns
                .iter()
                .map(|x| vec![x.name.clone(), x.group.clone(), x.description.clone()])
                .collect(),
        }),
    ];
    blocks.push(Block::Paragraph(vec![Inline::plain(format!(
        "Referral status words: {}.",
        c.referral_status_values.join(", ")
    ))]));
    Document {
        title: t.title.clone(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

fn log_template_csv(c: &BpContent) -> Vec<u8> {
    let mut s = c
        .log_template
        .columns
        .iter()
        .map(|x| {
            if x.name.contains(',') {
                format!("\"{}\"", x.name)
            } else {
                x.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    s.push('\n');
    s.into_bytes()
}

fn write_kit(
    ctx: &Context,
    c: &BpContent,
    format: DocFormat,
    boh_phone: &Option<String>,
) -> Result<(String, Vec<KitFile>)> {
    let (dir, shown) = templates::output_folder(ctx, "bp_screening_kit")?;
    let mut files = Vec::new();
    let mut add = |what: &str, file: String, grade: Option<(f64, bool)>| {
        files.push(KitFile {
            what: what.to_string(),
            file,
            reading_grade: grade.map(|g| g.0),
            meets_target: grade.map(|g| g.1),
        });
    };
    // Screening log template: XLSX (a table screen readers announce) and CSV.
    let doc = log_template_document(c, ctx);
    let xlsx = phn_docs::xlsx::to_xlsx(&doc)?;
    add(
        "Screening log template (XLSX)",
        templates::write_in(ctx, &dir, "bp_screening_log_template.xlsx", &xlsx)?,
        None,
    );
    add(
        "Screening log template (CSV, header row only)",
        templates::write_in(
            ctx,
            &dir,
            "bp_screening_log_template.csv",
            &log_template_csv(c),
        )?,
        None,
    );
    // Technique checklist.
    let md = technique_markdown(c);
    let d = templates::document(ctx, &md, "en-US", "Blood pressure measurement checklist");
    add(
        "Measurement technique checklist",
        templates::write_in(
            ctx,
            &dir,
            &format!("bp_technique_checklist.{}", format.ext()),
            &format.render(&d)?,
        )?,
        None,
    );
    // Reference card.
    let d = reference_document(c, ctx);
    add(
        "Reference card: categories and referral tiers (tool default, pending review)",
        templates::write_in(
            ctx,
            &dir,
            &format!("bp_reference_card.{}", format.ext()),
            &format.render(&d)?,
        )?,
        None,
    );
    // Letter templates (blank fields) and the handout.
    let mut values = sample_values();
    let blank_values: BTreeMap<String, String> = values
        .keys()
        .map(|k| (k.clone(), String::new()))
        .collect();
    let phone = boh_phone.clone().unwrap_or_default();
    for (what, def) in [
        ("Follow-up letter template: normal or elevated", &c.letters.routine),
        ("Follow-up letter template: stage 1 or stage 2 range", &c.letters.refer),
        ("Follow-up letter template: severe range (tool default)", &c.letters.urgent),
    ] {
        values.insert("program_phone".into(), "413-555-0100".into());
        let (sample, _) = templates::fill(&def.markdown, &values, BLANK);
        let grade = grade_of(&sample);
        let mut blank = blank_values.clone();
        blank.insert("program_phone".into(), phone.clone());
        let (filled, _) = templates::fill(&def.markdown, &blank, BLANK);
        let d = templates::document(ctx, &filled, "en-US", what);
        add(
            what,
            templates::write_in(
                ctx,
                &dir,
                &format!("{}.{}", def.kind, format.ext()),
                &format.render(&d)?,
            )?,
            Some(grade),
        );
    }
    let mut hv = BTreeMap::new();
    hv.insert("screening_date".to_string(), "2026-11-05".to_string());
    hv.insert("program_phone".to_string(), "413-555-0100".to_string());
    let (sample, _) = templates::fill(&c.handout.markdown, &hv, BLANK);
    let grade = grade_of(&sample);
    let mut blank = BTreeMap::new();
    blank.insert("screening_date".to_string(), String::new());
    blank.insert("program_phone".to_string(), phone);
    let (filled, _) = templates::fill(&c.handout.markdown, &blank, BLANK);
    let d = templates::document(ctx, &filled, "en-US", &c.handout.title);
    add(
        "Client handout: understanding your blood pressure numbers",
        templates::write_in(
            ctx,
            &dir,
            &format!("bp_client_handout.{}", format.ext()),
            &format.render(&d)?,
        )?,
        Some(grade),
    );
    Ok((shown, files))
}

fn write_letters(
    ctx: &Context,
    c: &BpContent,
    lt: &crate::localdata::LocalTable,
    cols: &LogCols,
    format: DocFormat,
    boh_phone: &Option<String>,
    threshold: u32,
) -> Result<LettersWritten> {
    let h = &lt.table.headers;
    let name = find_column(h, NAME);
    let address = find_column(h, ADDRESS);
    let town = find_column(h, TOWN);
    let date = find_column(h, DATE);
    let site = find_column(h, SITE);
    if name.is_none() && address.is_none() {
        return Err(PhnError::InvalidInput(format!(
            "'{}' needs a name or address column to fill letters (for example Name, Address, \
             Town); it has {} column(s) with other names",
            lt.label,
            h.len()
        )));
    }
    let n = lt.table.rows.len();
    let numeric = |idx: Option<usize>| -> Vec<Option<f64>> {
        match idx {
            Some(i) => lt.numeric(i).0,
            None => vec![None; n],
        }
    };
    let (sys1, dia1, sys2, dia2, ages) = (
        numeric(cols.sys1),
        numeric(cols.dia1),
        numeric(cols.sys2),
        numeric(cols.dia2),
        numeric(cols.age),
    );
    let (dir, shown) = templates::output_folder(ctx, "bp_followup_letters")?;
    let today = chrono::Local::now().date_naive().to_string();
    let phone = boh_phone.clone().unwrap_or_default();
    let mut docs: Vec<Document> = Vec::new();
    let mut skipped = 0usize;
    let mut blank_fields = 0usize;
    for r in 0..n {
        let row = &lt.table.rows[r];
        if row.iter().all(|v| v.trim().is_empty()) {
            continue;
        }
        let rb = row_bp(c, lt, r, cols, &sys1, &dia1, &sys2, &dia2, &ages);
        let (Some(cat), Some((s, d))) = (rb.cat, rb.avg) else {
            skipped += 1;
            continue;
        };
        if rb.under18 {
            skipped += 1;
            continue;
        }
        let get = |idx: Option<usize>| -> String {
            idx.and_then(|i| row.get(i))
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        let participant = get(name);
        if participant.is_empty() && get(address).is_empty() {
            skipped += 1;
            continue;
        }
        let def = cat_def(c, cat);
        let mut v: BTreeMap<String, String> = BTreeMap::new();
        v.insert("date".into(), today.clone());
        v.insert("participant_name".into(), participant);
        v.insert("address".into(), get(address));
        v.insert("town".into(), get(town));
        v.insert("screening_date".into(), get(date));
        v.insert("site".into(), get(site));
        v.insert("systolic".into(), s.to_string());
        v.insert("diastolic".into(), d.to_string());
        v.insert("category_label".into(), def.letter_label.clone());
        v.insert("category_plain".into(), def.plain.clone());
        v.insert("program_phone".into(), phone.clone());
        blank_fields += v.values().filter(|x| x.trim().is_empty()).count();
        let letter = match letter_tier(cat) {
            "routine" => &c.letters.routine,
            "refer" => &c.letters.refer,
            _ => &c.letters.urgent,
        };
        let (filled, _) = templates::fill(&letter.markdown, &v, BLANK);
        docs.push(templates::document(ctx, &filled, "en-US", &letter.kind));
    }
    if docs.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "no letters could be written from '{}': a row needs a name or address and a usable \
             blood pressure reading, and participants must be 18 or older",
            lt.label
        )));
    }
    let letters = docs.len();
    let combined = templates::combine("Blood pressure follow-up letters", "en-US", docs, ctx);
    let file = templates::write_in(
        ctx,
        &dir,
        &format!("bp_followup_letters.{}", format.ext()),
        &format.render(&combined)?,
    )?;
    let mut fields = Vec::new();
    for (label, idx) in [
        ("name", name),
        ("address", address),
        ("town", town),
        ("screening date", date),
        ("site", site),
    ] {
        if idx.is_some() {
            fields.push(label.to_string());
        }
    }
    Ok(LettersWritten {
        source_file: lt.label.clone(),
        folder: shown,
        file,
        letters,
        rows_without_letter: if skipped == 0 || skipped as u32 >= threshold {
            skipped.to_string()
        } else {
            format!("fewer than {threshold}")
        },
        blank_fields,
        fields_found: fields,
    })
}

// ---------------------------------------------------------------- the tool

/// Community blood pressure screening toolkit.
pub fn community_bp_screening_toolkit(
    ctx: &Context,
    args: BpToolkitArgs,
) -> Result<Envelope<BpToolkitResult>> {
    let c: BpContent = content::load(BP_SCREENING)?;
    if args.write_letters.unwrap_or(false) && args.log_path.is_none() {
        return Err(PhnError::InvalidInput(
            "letters are filled from a screening log: give log_path (a CSV/XLSX in /data)".into(),
        ));
    }
    if args.sheet.is_some() && args.log_path.is_none() {
        return Err(PhnError::InvalidInput(
            "sheet is used with log_path (an XLSX workbook in /data)".into(),
        ));
    }
    if args.readings.is_empty() && (args.age.is_some() || args.symptoms.is_some() || args.pregnant.is_some()) {
        return Err(PhnError::InvalidInput(
            "age, pregnant, and symptoms describe the readings: give readings as well".into(),
        ));
    }
    let format = args.format.unwrap_or_default();
    let settings = ctx.settings();
    let boh_phone = settings
        .jurisdiction
        .board_of_health_phone
        .clone()
        .filter(|p| !p.trim().is_empty());
    let threshold = ctx.suppression_threshold();
    let mut caveats: Vec<String> = Vec::new();

    let reading = if args.readings.is_empty() {
        None
    } else {
        Some(classify_readings(&c, &args)?)
    };

    // Local log: summary (and optional letters).
    let mut identifiers = Vec::new();
    let mut log_summary = None;
    let mut letters = None;
    let mut local_source = None;
    let mut files_written: Vec<String> = Vec::new();
    if let Some(p) = &args.log_path {
        let lt = crate::localdata::load(ctx, p, args.sheet.as_deref())?;
        identifiers = lt.identifier_categories();
        let (summary, cols) = summarize_log(&c, &lt, threshold)?;
        log_summary = Some(summary);
        if args.write_letters.unwrap_or(false) {
            let l = write_letters(ctx, &c, &lt, &cols, format, &boh_phone, threshold)?;
            files_written.push(l.file.clone());
            if boh_phone.is_none() {
                caveats.push(
                    "No board of health or program phone is saved, so the letters have a blank \
                     line for it; save it with configure_jurisdiction to fill it in."
                        .into(),
                );
            }
            letters = Some(l);
        }
        local_source = Some(lt.source());
    }

    // Blank kit documents.
    let write_docs = args
        .write_documents
        .unwrap_or(args.readings.is_empty() && args.log_path.is_none());
    let (kit_folder, files) = if write_docs {
        let (folder, files) = write_kit(ctx, &c, format, &boh_phone)?;
        files_written.extend(files.iter().map(|f| f.file.clone()));
        (Some(folder), files)
    } else {
        (None, Vec::new())
    };
    for f in files.iter().filter(|f| f.meets_target == Some(false)) {
        caveats.push(format!(
            "'{}' reads at grade {:.1}, above the target of {}; simplify it before use \
             (check_readability shows which sentences).",
            f.what,
            f.reading_grade.unwrap_or_default(),
            default_target_grade()
        ));
    }

    // Output blocks.
    let categories: Vec<CategoryOut> = c
        .categories
        .iter()
        .map(|d| CategoryOut {
            id: d.id.clone(),
            label: d.label.clone(),
            systolic: d.systolic.clone(),
            diastolic: d.diastolic.clone(),
            meaning: d.meaning.clone(),
            citation: format!(
                "{}, {}",
                c.sources
                    .get(&d.source)
                    .map(|s| s.title.as_str())
                    .unwrap_or(d.source.as_str()),
                d.section
            ),
            confidence: d.confidence,
        })
        .collect();
    let referral_tiers: Vec<TierOut> = c.referral_tiers.iter().map(tier_out).collect();
    let technique: Vec<TechniqueOut> = c
        .technique
        .iter()
        .map(|t| TechniqueOut {
            group: t.group.clone(),
            text: t.text.clone(),
            citation: format!(
                "{}, {}",
                c.sources
                    .get(&t.source)
                    .map(|s| s.title.as_str())
                    .unwrap_or(t.source.as_str()),
                t.section
            ),
            confidence: t.confidence,
            verified: t.verified,
        })
        .collect();
    let severe_range = SevereRange {
        rule: format!(
            "Severe range: systolic above 180 or diastolic above 120 on the average or on any \
             single reading (the any-reading rule is a tool default, pending review). {}",
            c.reading_rules.average_note
        ),
        source_wording: content::cite_all(&c.severe_range_wording, &c.sources),
        pregnancy: c.pregnancy.note.split_whitespace().collect::<Vec<_>>().join(" "),
    };
    let log_template = LogTemplateOut {
        title: c.log_template.title.clone(),
        instructions: c.log_template.instructions.clone(),
        columns: c.log_template.columns.clone(),
        referral_status_values: c.referral_status_values.clone(),
    };
    let letter_templates: Vec<LetterTemplateOut> = [&c.letters.routine, &c.letters.refer, &c.letters.urgent]
        .iter()
        .map(|l| LetterTemplateOut {
            kind: l.kind.clone(),
            description: l.description.clone(),
            placeholders: c.letters.placeholders.clone(),
            markdown: l.markdown.clone(),
        })
        .collect();
    let guideline = format!(
        "{} {}",
        c.meta.source.title,
        "retires and replaces the 2017 ACC/AHA guideline. Its full text could not be read here: \
         the stage 1, stage 2, and severe-range thresholds are medium confidence (from \
         peer-reviewed papers that state them), and normal, elevated, and the 130/80 cut-off are \
         from CDC's current pages. Corrections to the guideline were published in 2026 and were \
         not reviewed."
    );

    let summary = summary_text(
        &c,
        &categories,
        &severe_range,
        &referral_tiers,
        &technique,
        &reading,
        &log_summary,
        &letters,
        &kit_folder,
        &files,
    );

    caveats.push(c.notices.decision_support.split_whitespace().collect::<Vec<_>>().join(" "));
    caveats.push(c.notices.copyright.split_whitespace().collect::<Vec<_>>().join(" "));
    caveats.push(
        "Referral tiers and timing are this project's defaults (tool default, pending review) \
         and the guideline sources do not give community screening referral timing: follow your \
         agency's standing orders and medical director."
            .into(),
    );
    caveats.push(
        "Letters are written in English only; to use an agency-approved translation, edit the \
         letter in /data/output (this server never machine-translates)."
            .into(),
    );
    caveats.extend(content::review_caveats(&[BP_SCREENING]));

    let mut keys: Vec<&str> = vec!["aha-acc-2025", "cdc-hbp-about", "cdc-hbp-measure", "cdc-hbp-pregnancy", "review-2025-guideline", "papers-2025-categories", "cdc-copyright"];
    keys.dedup();
    let mut sources: Vec<Source> = content::sources_for(keys, &c.sources);
    if let Some(ls) = local_source {
        sources.push(ls);
    }
    let mut notes = Vec::new();
    if log_summary.is_some() {
        notes.push(
            "Counts from your screening log are suppressed one-way tables; no row, name, \
             address, reading, or date from the file is returned."
                .into(),
        );
    }
    if letters.is_some() {
        notes.push(
            "Names and addresses from your file were written into the letters in /data/output \
             only; the response has counts and file names only."
                .into(),
        );
    }
    let identifiers: Vec<String> = identifiers.into_iter().collect::<BTreeSet<_>>().into_iter().collect();
    Ok(Envelope::new(
        summary,
        BpToolkitResult {
            guideline,
            categories,
            severe_range,
            referral_tiers,
            technique,
            reading,
            log_template,
            letter_templates,
            handout_title: c.handout.title.clone(),
            log_summary: log_summary.clone(),
            letters,
            kit_folder,
            files,
            decision_support_notice: c.notices.decision_support.split_whitespace().collect::<Vec<_>>().join(" "),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: log_summary.is_some(),
        suppression_threshold: log_summary.as_ref().map(|_| threshold),
        identifiers_detected: identifiers,
        files_written,
        notes,
    })
    .decision_support())
}

#[allow(clippy::too_many_arguments)]
fn summary_text(
    c: &BpContent,
    categories: &[CategoryOut],
    severe: &SevereRange,
    tiers: &[TierOut],
    technique: &[TechniqueOut],
    reading: &Option<ReadingResult>,
    log: &Option<LogSummary>,
    letters: &Option<LettersWritten>,
    kit_folder: &Option<String>,
    files: &[KitFile],
) -> String {
    let mut s = String::from("**Community blood pressure screening kit** (adults; decision support)\n\n");
    if let Some(r) = reading {
        s.push_str(&format!(
            "**Reading:** average {} from {} reading(s): **{}**.{}\n\nReferral (tool default, pending review): {} {}\n\n",
            r.rounded,
            r.readings_used,
            r.category,
            if r.any_reading_in_severe_range {
                " At least one reading is in the severe range."
            } else {
                ""
            },
            r.referral.action,
            r.referral
                .symptoms_note
                .clone()
                .unwrap_or_default(),
        ));
        s.push_str(&format!("{}\n\n", r.note));
    }
    let rows: Vec<Vec<String>> = categories
        .iter()
        .map(|d| {
            vec![
                d.label.clone(),
                d.systolic.clone(),
                d.diastolic.clone(),
                format!("{:?}", d.confidence).to_lowercase(),
            ]
        })
        .collect();
    s.push_str("**Categories (ACC/AHA, adults)**\n\n");
    s.push_str(&md::table(
        &["Category", "Systolic", "Diastolic", "Confidence"],
        &rows,
    ));
    s.push_str(&format!("\n{}\n\n", c.guideline_note.split_whitespace().collect::<Vec<_>>().join(" ")));
    s.push_str("**Severe range, in the sources' words**\n\n");
    for w in &severe.source_wording {
        s.push_str(&content::bullet(w));
    }
    s.push_str(&format!("\n{}\n\n", severe.rule));
    s.push_str("**Referral tiers (tool default, pending review)**\n\n");
    for t in tiers {
        s.push_str(&format!("- **{}:** {}\n", t.applies_to, t.action));
    }
    s.push_str("\n**Measurement technique**\n\n");
    let mut group = "";
    for t in technique {
        if t.group != group {
            s.push_str(&format!("\n_{}_\n", t.group));
            group = &t.group;
        }
        s.push_str(&format!(
            "- {}{}\n",
            t.text,
            if t.verified { "" } else { " (not verified)" }
        ));
    }
    s.push('\n');
    if let Some(l) = log {
        s.push_str(&format!(
            "**Your screening log** `{}` (suppressed counts only; threshold {}):\n\n",
            l.source_file, l.suppression_threshold
        ));
        for t in &l.tables {
            let rows: Vec<Vec<String>> = t
                .rows
                .iter()
                .map(|r| vec![r.label.clone(), r.count.clone()])
                .collect();
            s.push_str(&format!("_{}_\n\n", t.title));
            s.push_str(&md::table(&["Group", "Count"], &rows));
            s.push_str(&format!("\nAll rows: {}\n\n", t.total));
        }
        for n in &l.notes {
            s.push_str(&format!("{n}\n\n"));
        }
    }
    if let Some(l) = letters {
        s.push_str(&format!(
            "**Letters:** {} follow-up letter(s) filled from `{}` and written to `{}`; rows without a letter: {}. Nothing from the file is shown here.\n\n",
            l.letters, l.source_file, l.folder, l.rows_without_letter
        ));
    }
    if let Some(f) = kit_folder {
        s.push_str(&format!("**Kit files** written to `{f}`:\n\n"));
        for x in files {
            s.push_str(&format!(
                "- {} (`{}`){}\n",
                x.what,
                x.file,
                x.reading_grade
                    .map(|g| format!(", reading grade {g:.1}"))
                    .unwrap_or_default()
            ));
        }
        s.push('\n');
    } else if log.is_none() && reading.is_none() {
        s.push_str("Set write_documents to true to write the blank kit to /data/output.\n\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> BpContent {
        content::load(BP_SCREENING).unwrap()
    }

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn content_is_complete_and_cites_known_sources() {
        let c = content();
        for cat in [Cat::Normal, Cat::Elevated, Cat::Stage1, Cat::Stage2, Cat::Severe] {
            assert!(c.categories.iter().any(|d| d.id == cat.id()), "{cat:?}");
        }
        for id in ["emergency", "same_day", "refer", "advise", "inform"] {
            let t = c.referral_tiers.iter().find(|t| t.id == id).expect(id);
            assert_eq!(t.status, "tool default, pending review");
            assert!(c.referral_status_values.contains(&t.log_value), "{id}");
        }
        for d in &c.categories {
            assert!(c.sources.contains_key(&d.source), "{}", d.id);
        }
        for t in &c.technique {
            assert!(c.sources.contains_key(&t.source), "{}", t.text);
            // A step that is not verified must say so in the content.
            assert!(t.verified || t.confidence == Confidence::Low, "{}", t.text);
        }
        for w in &c.severe_range_wording {
            assert!(c.sources.contains_key(&w.source));
        }
        // The letter placeholders are the ones the code fills.
        let filled = sample_values();
        for p in &c.letters.placeholders {
            assert!(filled.contains_key(p), "{p}");
        }
        assert_eq!(c.meta.review.status, phn_content::ReviewStatus::Pending);
    }

    /// Thresholds from CDC's pages and the 2025 guideline as reported in the sources listed
    /// in the content file.
    #[test]
    fn categories_follow_the_acc_aha_thresholds() {
        let cases = [
            (119, 79, Cat::Normal),
            (110, 70, Cat::Normal),
            (120, 79, Cat::Elevated),
            (129, 79, Cat::Elevated),
            (129, 80, Cat::Stage1),
            (130, 70, Cat::Stage1),
            (139, 89, Cat::Stage1),
            (125, 85, Cat::Stage1),
            (140, 70, Cat::Stage2),
            (139, 90, Cat::Stage2),
            (180, 120, Cat::Stage2),
            (181, 80, Cat::Severe),
            (150, 121, Cat::Severe),
        ];
        for (s, d, want) in cases {
            assert_eq!(classify(s, d), want, "{s}/{d}");
        }
    }

    #[test]
    fn readings_are_averaged_and_any_severe_reading_counts() {
        let c = content();
        let args = BpToolkitArgs {
            readings: vec![
                BpReading { systolic: 128.0, diastolic: 78.0 },
                BpReading { systolic: 132.0, diastolic: 82.0 },
            ],
            ..Default::default()
        };
        let r = classify_readings(&c, &args).unwrap();
        assert_eq!(r.rounded, "130/80");
        assert_eq!(r.category_id, "stage1");
        assert_eq!(r.referral.id, "advise");
        // One reading above 180 makes the result severe even when the average is lower.
        let args = BpToolkitArgs {
            readings: vec![
                BpReading { systolic: 184.0, diastolic: 100.0 },
                BpReading { systolic: 150.0, diastolic: 92.0 },
            ],
            symptoms: Some(true),
            ..Default::default()
        };
        let r = classify_readings(&c, &args).unwrap();
        assert_eq!(r.category_id, "severe");
        assert!(r.any_reading_in_severe_range);
        assert_eq!(r.referral.id, "emergency");
    }

    #[test]
    fn pregnancy_uses_cdc_thresholds_and_children_are_refused() {
        let c = content();
        let mk = |s: f64, d: f64| BpToolkitArgs {
            readings: vec![BpReading { systolic: s, diastolic: d }],
            pregnant: Some(true),
            ..Default::default()
        };
        assert_eq!(
            classify_readings(&c, &mk(138.0, 88.0)).unwrap().category_id,
            "pregnancy_below_140_90"
        );
        assert_eq!(
            classify_readings(&c, &mk(142.0, 88.0)).unwrap().category_id,
            "pregnancy_high"
        );
        let r = classify_readings(&c, &mk(160.0, 100.0)).unwrap();
        assert_eq!(r.category_id, "pregnancy_severe");
        assert_eq!(r.referral.id, "same_day");
        let kid = BpToolkitArgs {
            readings: vec![BpReading { systolic: 100.0, diastolic: 60.0 }],
            age: Some(12.0),
            ..Default::default()
        };
        assert!(classify_readings(&c, &kid).is_err());
        let bad = BpToolkitArgs {
            readings: vec![BpReading { systolic: 80.0, diastolic: 120.0 }],
            ..Default::default()
        };
        assert!(classify_readings(&c, &bad).is_err());
    }

    #[test]
    fn letters_and_handout_meet_the_reading_target() {
        let c = content();
        let v = sample_values();
        for l in [&c.letters.routine, &c.letters.refer, &c.letters.urgent] {
            let (filled, unknown) = templates::fill(&l.markdown, &v, BLANK);
            assert!(unknown.is_empty(), "{}: {unknown:?}", l.kind);
            let (grade, ok) = grade_of(&filled);
            assert!(ok, "{} reads at grade {grade}", l.kind);
        }
        let mut hv = BTreeMap::new();
        hv.insert("screening_date".to_string(), "2026-11-05".to_string());
        hv.insert("program_phone".to_string(), "413-555-0100".to_string());
        let (filled, unknown) = templates::fill(&c.handout.markdown, &hv, BLANK);
        assert!(unknown.is_empty(), "{unknown:?}");
        let (grade, ok) = grade_of(&filled);
        assert!(ok, "handout reads at grade {grade}");
    }

    fn write_log(dir: &tempfile::TempDir, rows: &[&str]) {
        let mut s = String::from(
            "Participant,Age,Systolic 1,Diastolic 1,Systolic 2,Diastolic 2,Referral status,Name,Address,Town\n",
        );
        for r in rows {
            s.push_str(r);
            s.push('\n');
        }
        std::fs::write(dir.path().join("data/bp_log.csv"), s).unwrap();
    }

    #[test]
    fn a_log_is_summarized_with_suppression_and_no_row_data() {
        let (dir, ctx) = ctx();
        let mut rows: Vec<String> = Vec::new();
        for i in 0..14 {
            rows.push(format!(
                "P{i},{},118,76,120,78,Not referred,Ottilie Fernsby,12 Larkspur Way,Holyoke",
                30 + i
            ));
        }
        for i in 0..8 {
            rows.push(format!(
                "Q{i},{},142,92,138,88,Referred routine,Bartholomew Quince,48 Juniper Row,Holyoke",
                55 + i
            ));
        }
        // Two severe-range participants: small, so suppressed.
        rows.push("R1,66,190,110,186,108,Referred same day,Cornelius Ashgrove,5 Elm Street,Holyoke".into());
        rows.push("R2,16,150,95,,,Not referred,Young Person,7 Oak Street,Holyoke".into());
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        write_log(&dir, &refs);
        let env = community_bp_screening_toolkit(
            &ctx,
            BpToolkitArgs {
                log_path: Some("bp_log.csv".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let log = env.data.log_summary.as_ref().unwrap();
        let cat = &log.tables[0];
        let get = |l: &str| cat.rows.iter().find(|r| r.label.starts_with(l)).unwrap().count.clone();
        assert_eq!(get("Normal"), "14");
        assert_eq!(get("Stage 2"), "8");
        // Small cells are hidden, with at least one more hidden cell.
        assert_eq!(get("Severe"), "*");
        assert!(cat.rows.iter().filter(|r| r.count == "*").count() >= 2);
        assert!(env.privacy.suppression_applied);
        assert!(!env.privacy.row_level_data);
        let json = serde_json::to_string(&env).unwrap();
        for secret in ["Ottilie", "Fernsby", "Larkspur", "Quince", "Juniper", "Ashgrove", "Young Person"] {
            assert!(!json.contains(secret), "{secret} leaked");
        }
        // The kit is not written when a log is summarized.
        assert!(env.data.kit_folder.is_none());
    }

    #[test]
    fn letters_go_to_the_output_folder_only() {
        let (dir, ctx) = ctx();
        write_log(
            &dir,
            &[
                "A1,52,134,84,136,86,Referred routine,Ottilie Fernsby,12 Larkspur Way,Holyoke",
                "A2,47,122,70,,,Not referred,Bartholomew Quince,48 Juniper Row,Holyoke",
                "A3,70,192,112,,,Referred same day,Cornelius Ashgrove,5 Elm Street,Holyoke",
                "A4,15,100,60,,,Not referred,Child Example,7 Oak Street,Holyoke",
            ],
        );
        let env = community_bp_screening_toolkit(
            &ctx,
            BpToolkitArgs {
                log_path: Some("bp_log.csv".into()),
                write_letters: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let l = env.data.letters.as_ref().unwrap();
        assert_eq!(l.letters, 3);
        assert_eq!(l.rows_without_letter, "fewer than 5");
        let json = serde_json::to_string(&env).unwrap();
        for secret in ["Ottilie", "Fernsby", "Larkspur", "Quince", "Juniper", "Ashgrove", "Child Example"] {
            assert!(!json.contains(secret), "{secret} leaked");
        }
        assert!(!env.privacy.files_written.is_empty());
        assert!(env.privacy.files_written.iter().all(|f| f.contains("output")));
    }

    #[test]
    fn a_file_without_readings_is_refused() {
        let (dir, ctx) = ctx();
        std::fs::write(dir.path().join("data/other.csv"), "Name,Age\nA B,50\n").unwrap();
        let err = community_bp_screening_toolkit(
            &ctx,
            BpToolkitArgs {
                log_path: Some("other.csv".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("no blood pressure readings"));
        assert!(!err.to_string().contains("A B"));
    }

    #[test]
    fn the_blank_kit_is_written() {
        let (_dir, ctx) = ctx();
        let env = community_bp_screening_toolkit(&ctx, BpToolkitArgs::default()).unwrap();
        assert!(env.data.kit_folder.is_some());
        assert_eq!(env.data.files.len(), 8);
        for f in &env.data.files {
            if let Some(ok) = f.meets_target {
                assert!(ok, "{}", f.what);
            }
        }
        assert!(env.caveats.iter().any(|x| x.contains("tool default")));
    }
}
