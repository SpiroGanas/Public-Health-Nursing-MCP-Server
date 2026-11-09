//! MCH-03 `calculate_growth_percentiles`.
//!
//! Percentiles and z-scores from the WHO Child Growth Standards (birth to 24 months) and the
//! CDC 2000 growth charts (2 to 20 years), with CDC's 2022 extended BMI-for-age method above
//! the 95th percentile and corrected age for prematurity. One child's measurements (one or
//! several visits) are scored in the request; a file of many children in /data is scored in
//! batch, with per-child results written to /data/output and only suppressed counts returned.

pub mod lms;

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use phn_content::ContentMeta;
use phn_content::reportable::Confidence;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use self::lms::{DAYS_PER_MONTH, GrowthTables, Lms, Sex, TableTexts};
use super::content::{self as mc, CitedText, NoteOut, SourceRef};
use crate::context::Context;
use crate::md;
use crate::vax::dates::parse_date;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct CutPoints {
    low_percentile: f64,
    high_percentile: f64,
    text: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct BmiCategoryDef {
    category: String,
    label: String,
    rule: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtendedBmiDef {
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtremeCuts {
    who_length: [f64; 2],
    who_weight: [f64; 2],
    who_other: [f64; 2],
    cdc_height: [f64; 2],
    cdc_weight: [f64; 2],
    cdc_bmi: [f64; 2],
    cdc_head: [f64; 2],
}

#[derive(Debug, Clone, Deserialize)]
struct ExtremeValues {
    who: CitedText,
    cdc: CitedText,
    cut_points: ExtremeCuts,
}

#[derive(Debug, Clone, Deserialize)]
struct CorrectedAgeRule {
    preterm_below_weeks: f64,
    term_weeks: f64,
    correct_until_months: f64,
    text: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct GrowthContent {
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    chart_selection: Vec<CitedText>,
    who_cut_points: CutPoints,
    cdc_cut_points: CutPoints,
    bmi_categories: Vec<BmiCategoryDef>,
    extended_bmi: ExtendedBmiDef,
    extreme_values: ExtremeValues,
    corrected_age: CorrectedAgeRule,
    measurement_notes: Vec<CitedText>,
}

static CONTENT: LazyLock<GrowthContent> =
    LazyLock::new(|| mc::load::<GrowthContent>("mch-growth-interpretation"));

fn content() -> &'static GrowthContent {
    &CONTENT
}

// ---------------------------------------------------------------- arguments

/// How length or height was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    /// Lying down (length); expected before 24 months.
    Recumbent,
    /// Standing (height or stature); expected from 24 months.
    Standing,
}

/// One set of measurements at one visit.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct GrowthMeasurement {
    /// Age at the measurement in days (best for newborns). Give one of age_days, age_months,
    /// age_years, or measurement_date (with birth_date).
    #[serde(default)]
    pub age_days: Option<f64>,
    /// Age in months (decimals allowed, e.g. 9.5).
    #[serde(default)]
    pub age_months: Option<f64>,
    /// Age in years (decimals allowed, e.g. 4.25).
    #[serde(default)]
    pub age_years: Option<f64>,
    /// Date of the measurement (YYYY-MM-DD), used with birth_date to compute exact age. Dates
    /// are used only for the calculation and are not repeated in the result.
    #[serde(default)]
    pub measurement_date: Option<String>,
    /// Weight in kilograms.
    #[serde(default)]
    pub weight_kg: Option<f64>,
    /// Weight in pounds (add weight_oz for pounds and ounces).
    #[serde(default)]
    pub weight_lb: Option<f64>,
    /// Ounces, added to weight_lb (e.g. 7 lb 6 oz).
    #[serde(default)]
    pub weight_oz: Option<f64>,
    /// Length (under 24 months) or standing height in centimeters.
    #[serde(default)]
    pub length_or_height_cm: Option<f64>,
    /// Length or height in inches.
    #[serde(default)]
    pub length_or_height_in: Option<f64>,
    /// Head circumference in centimeters (scored to 36 months).
    #[serde(default)]
    pub head_circumference_cm: Option<f64>,
    /// Head circumference in inches.
    #[serde(default)]
    pub head_circumference_in: Option<f64>,
    /// How length/height was measured (default: recumbent under 24 months, standing after).
    #[serde(default)]
    pub position: Option<Position>,
}

/// Arguments for `calculate_growth_percentiles`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct GrowthArgs {
    /// The child's sex: "male" or "female" (required unless measurements_path is used).
    #[serde(default)]
    pub sex: Option<Sex>,
    /// One or more visits for the same child, oldest first (weight, length/height, head
    /// circumference). Several visits show the change in z-score between visits.
    #[serde(default)]
    pub measurements: Vec<GrowthMeasurement>,
    /// Birth date (YYYY-MM-DD), only needed when measurements give measurement_date instead of
    /// an age. Not repeated in the result.
    #[serde(default)]
    pub birth_date: Option<String>,
    /// Gestational age at birth in completed weeks (e.g. 32). Under 37 weeks, corrected age is
    /// used until 24 months of chronological age.
    #[serde(default)]
    pub gestational_age_weeks: Option<u32>,
    /// Extra days of gestational age (e.g. 32 weeks 4 days -> 4).
    #[serde(default)]
    pub gestational_age_days: Option<u32>,
    /// Set false to score preterm infants at chronological age (default true).
    #[serde(default)]
    pub correct_for_prematurity: Option<bool>,
    /// Batch mode: a CSV/XLSX in /data with one row per child-visit (columns such as sex,
    /// age_months or date_of_birth + measurement_date, weight_kg or weight_lb, height_cm or
    /// height_in, head_circumference_cm, gestational_age_weeks). Per-row results are written
    /// to /data/output; only suppressed counts are returned.
    #[serde(default)]
    pub measurements_path: Option<String>,
    /// Sheet name for an XLSX file.
    #[serde(default)]
    pub sheet: Option<String>,
}

// ---------------------------------------------------------------- results

/// The extended BMI details (CDC 2022) for BMI above the 95th percentile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExtendedBmiOut {
    /// True when the extended method set the percentile and z-score.
    pub extended_method_used: bool,
    /// 95th percentile BMI for sex and age.
    pub bmi_95th_percentile: f64,
    /// BMI as a percentage of the 95th percentile (120% = severe obesity threshold).
    pub percent_of_95th_percentile: f64,
    /// The 2000-chart LMS z-score (for comparison with older records).
    pub lms_z_score: f64,
    /// Dispersion parameter sigma for sex and age.
    pub sigma: f64,
}

/// One growth indicator for one visit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndicatorResult {
    /// For example "Weight-for-age".
    pub indicator: String,
    /// Chart used: "WHO 2006" or "CDC 2000" (with "+ 2022 extended BMI").
    pub chart: String,
    /// The measurement (kg, cm, or kg/m²).
    pub value: f64,
    pub unit: String,
    pub z_score: f64,
    /// Percentile (0–100).
    pub percentile: f64,
    /// Median for sex and age (or length).
    pub median: f64,
    /// Screening flags (e.g. "below the 2.3rd percentile (−2 SD)").
    pub flags: Vec<String>,
    /// BMI category for children 2 and older.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended_bmi: Option<ExtendedBmiOut>,
    /// True when the value is extreme enough to check for a measurement or entry error.
    pub check_measurement: bool,
}

/// Change in z-score since the previous visit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ZChange {
    pub indicator: String,
    /// Days between the visits (age used).
    pub days: f64,
    pub z_change: f64,
    /// Measurement change (kg or cm) between the visits.
    pub value_change: f64,
    pub unit: String,
}

/// Results for one visit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VisitResult {
    /// 1-based position in the request.
    pub visit: usize,
    /// Chronological age, e.g. "9 months 12 days" or "7 years 8 months".
    pub chronological_age: String,
    pub chronological_age_months: f64,
    /// "corrected" or "chronological".
    pub age_used: String,
    /// Corrected age in months when used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrected_age_months: Option<f64>,
    /// BMI (kg/m²) when weight and length/height were given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bmi: Option<f64>,
    pub indicators: Vec<IndicatorResult>,
    pub changes_since_previous: Vec<ZChange>,
    /// Measurements that could not be scored, and why.
    pub problems: Vec<String>,
}

/// Counts for batch mode (suppressed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GrowthCountRow {
    pub age_group: String,
    /// Category label -> count (or "*" when suppressed).
    pub counts: BTreeMap<String, String>,
}

/// Batch summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GrowthBatch {
    pub file: String,
    pub rows_read: usize,
    pub rows_scored: usize,
    pub rows_with_problems: usize,
    /// Rows with at least one value flagged as a possible measurement error.
    pub rows_to_recheck: String,
    /// Weight-status counts by age group (WHO weight-for-length under 2; CDC BMI from 2).
    pub counts: Vec<GrowthCountRow>,
    pub output_file: String,
    pub suppression_threshold: u32,
}

/// Result of `calculate_growth_percentiles`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GrowthResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sex: Option<String>,
    pub visits: Vec<VisitResult>,
    /// The corrected-age rule applied (when gestational age was given).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrected_age_rule: Option<NoteOut>,
    /// Chart selection, cut points, and method notes with citations.
    pub interpretation_notes: Vec<NoteOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<GrowthBatch>,
}

// ---------------------------------------------------------------- scoring

/// Load and parse the bundled (or newer cached) growth tables, with their citations.
pub fn load_tables(ctx: &Context) -> Result<(GrowthTables, Vec<Source>)> {
    let snap = &ctx.snapshots;
    let (who_age, who_entry, _) =
        snap.file_text("who-growth-standards", "who_growth_by_age.csv")?;
    let (who_wfl, _, _) = snap.file_text("who-growth-standards", "who_growth_wfl.csv")?;
    let (wt, cdc_entry, _) = snap.file_text("cdc-growth-charts", "cdc_growth_wtage.csv")?;
    let (st, _, _) = snap.file_text("cdc-growth-charts", "cdc_growth_statage.csv")?;
    let (bmi, _, _) = snap.file_text("cdc-growth-charts", "cdc_growth_bmi_age_2022.csv")?;
    let (hc, _, _) = snap.file_text("cdc-growth-charts", "cdc_growth_hcageinf.csv")?;
    let tables = GrowthTables::parse(&TableTexts {
        who_by_age: &who_age,
        who_wfl: &who_wfl,
        cdc_wtage: &wt,
        cdc_statage: &st,
        cdc_bmi: &bmi,
        cdc_hcageinf: &hc,
    })?;
    Ok((tables, vec![who_entry.to_source(), cdc_entry.to_source()]))
}

/// One visit's inputs in canonical units.
#[derive(Debug, Clone, Default)]
struct Inputs {
    chrono_days: f64,
    weight_kg: Option<f64>,
    length_cm: Option<f64>,
    head_cm: Option<f64>,
    position: Option<Position>,
}

/// Gestational age at birth in days.
fn ga_days(weeks: Option<u32>, days: Option<u32>) -> Option<f64> {
    weeks.map(|w| f64::from(w) * 7.0 + f64::from(days.unwrap_or(0)))
}

fn age_label(days: f64) -> String {
    let months = days / DAYS_PER_MONTH;
    if days < 0.0 {
        return format!("{days:.0} days");
    }
    if months < 1.0 {
        return format!("{days:.0} days");
    }
    if months < 24.0 {
        let whole = months.floor();
        let rem = (days - whole * DAYS_PER_MONTH).round();
        return format!("{whole:.0} months {rem:.0} days");
    }
    let years = (months / 12.0).floor();
    let rem = (months - years * 12.0).floor();
    format!("{years:.0} years {rem:.0} months")
}

fn pct_label(p: f64) -> String {
    if p < 0.1 {
        "<0.1".into()
    } else if p > 99.9 {
        ">99.9".into()
    } else {
        format!("{p:.1}")
    }
}

fn in_range(z: f64, r: [f64; 2], upper_inclusive: bool) -> bool {
    if upper_inclusive {
        z >= r[0] && z < r[1]
    } else {
        z >= r[0] && z <= r[1]
    }
}

struct Scorer {
    sex: Sex,
}

impl Scorer {
    fn who(
        &self,
        indicator: &str,
        lms: Lms,
        x: f64,
        unit: &str,
        restricted: bool,
    ) -> IndicatorResult {
        let c = content();
        let z = if restricted {
            lms.z_who_restricted(x)
        } else {
            lms.z(x)
        };
        let p = lms::phi(z) * 100.0;
        let cuts = &c.extreme_values.cut_points;
        let biv = match indicator {
            "Length-for-age" => !in_range(z, cuts.who_length, true),
            "Weight-for-age" => !in_range(z, cuts.who_weight, true),
            _ => !in_range(z, cuts.who_other, false),
        };
        let mut flags = Vec::new();
        let (lo, hi) = (
            c.who_cut_points.low_percentile,
            c.who_cut_points.high_percentile,
        );
        if p <= lo {
            flags.push(format!(
                "at or below the {lo}th percentile (−2 SD){}",
                match indicator {
                    "Weight-for-age" => ": low weight-for-age",
                    "Length-for-age" => ": short length-for-age",
                    "Weight-for-length" => ": low weight-for-length",
                    "Head circumference-for-age" => ": small head circumference",
                    _ => "",
                }
            ));
        } else if p >= hi {
            flags.push(format!(
                "at or above the {hi}th percentile (+2 SD){}",
                match indicator {
                    "Weight-for-length" => ": high weight-for-length",
                    "Head circumference-for-age" => ": large head circumference",
                    "Weight-for-age" => ": high weight-for-age",
                    "Length-for-age" => ": tall length-for-age",
                    _ => "",
                }
            ));
        }
        if biv {
            flags.push("extreme value: re-measure and check the entry (WHO cut points)".into());
        }
        IndicatorResult {
            indicator: indicator.into(),
            chart: "WHO 2006".into(),
            value: x,
            unit: unit.into(),
            z_score: z,
            percentile: p,
            median: lms.m,
            flags,
            category: None,
            extended_bmi: None,
            check_measurement: biv,
        }
    }

    fn cdc(&self, indicator: &str, lms: Lms, x: f64, unit: &str, biv: [f64; 2]) -> IndicatorResult {
        let c = content();
        let z = lms.z_cdc(x);
        let p = lms::phi(z) * 100.0;
        let mz = lms.modified_z(x);
        let extreme = !(mz >= biv[0] && mz <= biv[1]);
        let mut flags = Vec::new();
        let (lo, hi) = (
            c.cdc_cut_points.low_percentile,
            c.cdc_cut_points.high_percentile,
        );
        if p <= lo {
            flags.push(format!("at or below the {lo}th percentile"));
        } else if p >= hi {
            flags.push(format!("at or above the {hi}th percentile"));
        }
        if extreme {
            flags.push(format!(
                "extreme value (modified z {mz:.1}): re-measure and check the entry"
            ));
        }
        IndicatorResult {
            indicator: indicator.into(),
            chart: "CDC 2000".into(),
            value: x,
            unit: unit.into(),
            z_score: z,
            percentile: p,
            median: lms.m,
            flags,
            category: None,
            extended_bmi: None,
            check_measurement: extreme,
        }
    }

    fn cdc_bmi(&self, lms: Lms, age_months: f64, bmi: f64) -> IndicatorResult {
        let c = content();
        let s = lms::cdc_bmi_score(lms, self.sex, age_months, bmi);
        let cuts = c.extreme_values.cut_points.cdc_bmi;
        let extreme = !(s.modified_z >= cuts[0] && s.modified_z <= cuts[1]);
        let label = |id: &str| {
            c.bmi_categories
                .iter()
                .find(|b| b.category == id)
                .map(|b| b.label.clone())
                .unwrap_or_else(|| id.to_string())
        };
        // Categories use the 2000-chart percentile up to the 95th (identical to the extended
        // percentile there) and the % of the 95th percentile above it.
        let category = if s.lms_percentile < 5.0 {
            label("underweight")
        } else if s.lms_percentile < 85.0 {
            label("healthy_weight")
        } else if s.lms_percentile < 95.0 {
            label("overweight")
        } else if s.pct_of_p95 >= 140.0 || bmi >= 40.0 {
            label("severe_obesity_class3")
        } else if s.pct_of_p95 >= 120.0 || bmi >= 35.0 {
            label("severe_obesity")
        } else {
            label("obesity")
        };
        let mut flags = Vec::new();
        if extreme {
            flags.push(format!(
                "extreme value (modified z {:.1}): re-measure and check the entry",
                s.modified_z
            ));
        }
        IndicatorResult {
            indicator: "BMI-for-age".into(),
            chart: if s.extended {
                "CDC 2000 + 2022 extended BMI".into()
            } else {
                "CDC 2000".into()
            },
            value: bmi,
            unit: "kg/m²".into(),
            z_score: s.z,
            percentile: s.percentile,
            median: s.p50,
            flags,
            category: Some(category),
            extended_bmi: Some(ExtendedBmiOut {
                extended_method_used: s.extended,
                bmi_95th_percentile: s.p95,
                percent_of_95th_percentile: s.pct_of_p95,
                lms_z_score: s.lms_z,
                sigma: s.sigma,
            }),
            check_measurement: extreme,
        }
    }
}

/// Score one visit. `scoring_days` is the age used (corrected or chronological).
fn score_visit(
    t: &GrowthTables,
    sex: Sex,
    inp: &Inputs,
    scoring_days: f64,
    problems: &mut Vec<String>,
) -> (Option<f64>, Vec<IndicatorResult>) {
    let c = content();
    let sc = Scorer { sex };
    let mut out = Vec::new();
    let months = scoring_days / DAYS_PER_MONTH;
    let who = scoring_days < 730.5;
    let bmi = match (inp.weight_kg, inp.length_cm) {
        (Some(w), Some(l)) if l > 0.0 => Some(w / (l / 100.0).powi(2)),
        _ => None,
    };
    if months >= 240.0 {
        problems.push(
            "age is 20 years or older: the CDC growth charts end at 20 years (use adult BMI \
             categories)"
                .into(),
        );
        return (bmi, out);
    }
    let pos = inp.position;
    // The WHO daily file switches to standing height at 731 days; WHO scoring stops at 730.
    let wd = scoring_days.min(730.0);
    if who {
        if let Some(w) = inp.weight_kg {
            match t.who_weight.at(sex, wd) {
                Some(l) => out.push(sc.who("Weight-for-age", l, w, "kg", true)),
                None => problems.push("weight-for-age: age outside the WHO table".into()),
            }
        }
        if let Some(len) = inp.length_cm {
            if pos == Some(Position::Standing) {
                problems.push(
                    "standing height before 24 months: the WHO length charts assume recumbent \
                     length; measure lying down"
                        .into(),
                );
            }
            match t.who_length.at(sex, wd) {
                Some(l) => out.push(sc.who("Length-for-age", l, len, "cm", false)),
                None => problems.push("length-for-age: age outside the WHO table".into()),
            }
            if let Some(w) = inp.weight_kg {
                match t.who_wfl.at(sex, len) {
                    Some(l) => out.push(sc.who("Weight-for-length", l, w, "kg", true)),
                    None => problems.push(format!(
                        "weight-for-length: the WHO table covers lengths 45-110 cm ({len:.1} cm \
                         given)"
                    )),
                }
            }
            if let Some(b) = bmi {
                match t.who_bmi.at(sex, wd) {
                    Some(l) => out.push(sc.who("BMI-for-age", l, b, "kg/m²", true)),
                    None => problems.push("BMI-for-age: age outside the WHO table".into()),
                }
            }
        }
        if let Some(h) = inp.head_cm {
            match t.who_head.at(sex, wd) {
                Some(l) => out.push(sc.who("Head circumference-for-age", l, h, "cm", false)),
                None => problems.push("head circumference: age outside the WHO table".into()),
            }
        }
    } else {
        let cuts = &c.extreme_values.cut_points;
        if let Some(w) = inp.weight_kg {
            match t.cdc_weight.at(sex, months) {
                Some(l) => out.push(sc.cdc("Weight-for-age", l, w, "kg", cuts.cdc_weight)),
                None => problems.push("weight-for-age: age outside the CDC table".into()),
            }
        }
        if let Some(h) = inp.length_cm {
            if pos == Some(Position::Recumbent) {
                problems.push(
                    "recumbent length at 24 months or older: the CDC stature charts assume \
                     standing height"
                        .into(),
                );
            }
            match t.cdc_stature.at(sex, months) {
                Some(l) => out.push(sc.cdc("Stature-for-age", l, h, "cm", cuts.cdc_height)),
                None => problems.push("stature-for-age: age outside the CDC table".into()),
            }
        }
        if let Some(b) = bmi {
            match t.cdc_bmi.at(sex, months) {
                Some(l) => out.push(sc.cdc_bmi(l, months, b)),
                None => problems.push("BMI-for-age: age outside the CDC table".into()),
            }
        }
        if let Some(h) = inp.head_cm {
            match t.cdc_head.at(sex, months) {
                Some(l) => {
                    out.push(sc.cdc("Head circumference-for-age", l, h, "cm", cuts.cdc_head))
                }
                None => problems.push(
                    "head circumference is charted only to 36 months (CDC infant chart)".into(),
                ),
            }
        }
    }
    (bmi, out)
}

/// Resolve the chronological age in days for one visit.
fn chrono_days(m: &GrowthMeasurement, birth: Option<NaiveDate>) -> Result<f64> {
    let given = [
        m.age_days.is_some(),
        m.age_months.is_some(),
        m.age_years.is_some(),
        m.measurement_date.is_some(),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    if given > 1 {
        return Err(PhnError::InvalidInput(
            "give only one of age_days, age_months, age_years, or measurement_date per visit"
                .into(),
        ));
    }
    let d = if let Some(d) = m.age_days {
        d
    } else if let Some(mo) = m.age_months {
        mo * DAYS_PER_MONTH
    } else if let Some(y) = m.age_years {
        y * 12.0 * DAYS_PER_MONTH
    } else if let Some(ds) = &m.measurement_date {
        let Some(b) = birth else {
            return Err(PhnError::InvalidInput(
                "measurement_date needs birth_date (or give age_months instead)".into(),
            ));
        };
        let on = parse_date(ds).ok_or_else(|| {
            PhnError::InvalidInput("measurement_date is not a date (use YYYY-MM-DD)".into())
        })?;
        (on - b).num_days() as f64
    } else {
        return Err(PhnError::InvalidInput(
            "each visit needs the child's age: age_days, age_months, age_years, or \
             measurement_date with birth_date"
                .into(),
        ));
    };
    if !d.is_finite() || d < 0.0 {
        return Err(PhnError::InvalidInput(
            "age must be zero or more (check the dates)".into(),
        ));
    }
    Ok(d)
}

fn positive(v: Option<f64>, what: &str) -> Result<Option<f64>> {
    match v {
        Some(x) if !x.is_finite() || x <= 0.0 => Err(PhnError::InvalidInput(format!(
            "{what} must be a positive number"
        ))),
        other => Ok(other),
    }
}

fn inputs(m: &GrowthMeasurement, birth: Option<NaiveDate>) -> Result<Inputs> {
    let chrono = chrono_days(m, birth)?;
    let weight = match (
        positive(m.weight_kg, "weight_kg")?,
        m.weight_lb,
        m.weight_oz,
    ) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            return Err(PhnError::InvalidInput(
                "give weight in kilograms or in pounds/ounces, not both".into(),
            ));
        }
        (Some(kg), None, None) => Some(kg),
        (None, lb, oz) if lb.is_some() || oz.is_some() => {
            let total_lb = lb.unwrap_or(0.0) + oz.unwrap_or(0.0) / 16.0;
            positive(Some(total_lb), "weight")?.map(|x| x * 0.453_592_37)
        }
        _ => None,
    };
    let length = match (
        positive(m.length_or_height_cm, "length_or_height_cm")?,
        positive(m.length_or_height_in, "length_or_height_in")?,
    ) {
        (Some(_), Some(_)) => {
            return Err(PhnError::InvalidInput(
                "give length/height in centimeters or inches, not both".into(),
            ));
        }
        (Some(cm), None) => Some(cm),
        (None, Some(i)) => Some(i * 2.54),
        _ => None,
    };
    let head = match (
        positive(m.head_circumference_cm, "head_circumference_cm")?,
        positive(m.head_circumference_in, "head_circumference_in")?,
    ) {
        (Some(_), Some(_)) => {
            return Err(PhnError::InvalidInput(
                "give head circumference in centimeters or inches, not both".into(),
            ));
        }
        (Some(cm), None) => Some(cm),
        (None, Some(i)) => Some(i * 2.54),
        _ => None,
    };
    if weight.is_none() && length.is_none() && head.is_none() {
        return Err(PhnError::InvalidInput(
            "each visit needs at least one measurement: weight, length/height, or head \
             circumference"
                .into(),
        ));
    }
    Ok(Inputs {
        chrono_days: chrono,
        weight_kg: weight,
        length_cm: length,
        head_cm: head,
        position: m.position,
    })
}

/// The age used for scoring: corrected for prematurity when the rule applies.
/// Returns (days, corrected?) or an explanation when the corrected age is before term.
fn scoring_age(
    chrono: f64,
    ga: Option<f64>,
    correct: bool,
) -> std::result::Result<(f64, bool), String> {
    let rule = &content().corrected_age;
    match ga {
        Some(ga)
            if correct
                && ga < rule.preterm_below_weeks * 7.0
                && chrono < rule.correct_until_months * DAYS_PER_MONTH =>
        {
            let corrected = chrono - (rule.term_weeks * 7.0 - ga);
            if corrected < 0.0 {
                Err(format!(
                    "corrected age is {:.0} days before term (40 weeks); the WHO charts start \
                     at term, so plot on a preterm (postmenstrual-age) growth chart until then",
                    -corrected
                ))
            } else {
                Ok((corrected, true))
            }
        }
        _ => Ok((chrono, false)),
    }
}

fn changes(prev: &(f64, Vec<IndicatorResult>), cur: &(f64, Vec<IndicatorResult>)) -> Vec<ZChange> {
    cur.1
        .iter()
        .filter_map(|r| {
            let p = prev
                .1
                .iter()
                .find(|q| q.indicator == r.indicator && q.chart == r.chart)?;
            Some(ZChange {
                indicator: r.indicator.clone(),
                days: cur.0 - prev.0,
                z_change: r.z_score - p.z_score,
                value_change: r.value - p.value,
                unit: r.unit.clone(),
            })
        })
        .collect()
}

fn interpretation_notes() -> Vec<NoteOut> {
    let c = content();
    let s = &c.sources;
    let mut v = mc::notes(s, &c.chart_selection);
    v.push(NoteOut {
        text: c.who_cut_points.text.clone(),
        citation: mc::cite(s, &c.who_cut_points.source, &c.who_cut_points.section),
        confidence: Some(c.who_cut_points.confidence),
    });
    v.push(NoteOut {
        text: c.cdc_cut_points.text.clone(),
        citation: mc::cite(s, &c.cdc_cut_points.source, &c.cdc_cut_points.section),
        confidence: Some(c.cdc_cut_points.confidence),
    });
    for b in &c.bmi_categories {
        v.push(NoteOut {
            text: format!("{}: {}.", b.label, b.rule),
            citation: mc::cite(s, &b.source, &b.section),
            confidence: None,
        });
    }
    v.push(NoteOut {
        text: c.extended_bmi.text.clone(),
        citation: mc::cite(s, &c.extended_bmi.source, &c.extended_bmi.section),
        confidence: None,
    });
    v.push(mc::note(s, &c.extreme_values.who));
    v.push(mc::note(s, &c.extreme_values.cdc));
    v.extend(mc::notes(s, &c.measurement_notes));
    v
}

fn corrected_rule_note() -> NoteOut {
    let c = content();
    let r = &c.corrected_age;
    NoteOut {
        text: r.text.clone(),
        citation: mc::cite(&c.sources, &r.source, &r.section),
        confidence: Some(r.confidence),
    }
}

fn content_sources() -> Vec<Source> {
    let c = content();
    c.sources.values().map(SourceRef::to_source).collect()
}

fn base_caveats() -> Vec<String> {
    let c = content();
    vec![
        mc::review_caveat(&c.meta, "growth interpretation"),
        "Percentiles are screening information, not a diagnosis: interpret them with the \
         child's growth over time, feeding, health history, and the parents' size, and refer to \
         the child's health care provider for concerns."
            .into(),
        "Accurate measurement matters: weigh infants nude or in a dry diaper, measure length \
         lying down before 24 months and standing height after, and re-measure any value \
         flagged as extreme."
            .into(),
    ]
}

fn fmt_z(z: f64) -> String {
    format!("{z:+.2}")
}

// ---------------------------------------------------------------- the tool

/// Calculate growth percentiles and z-scores for one child or a file of children.
pub fn calculate_growth_percentiles(
    ctx: &Context,
    args: GrowthArgs,
) -> Result<Envelope<GrowthResult>> {
    if let Some(path) = args.measurements_path.clone() {
        if !args.measurements.is_empty() {
            return Err(PhnError::InvalidInput(
                "use either measurements (one child) or measurements_path (a file), not both"
                    .into(),
            ));
        }
        return batch(
            ctx,
            &path,
            args.sheet.as_deref(),
            args.correct_for_prematurity.unwrap_or(true),
        );
    }
    let Some(sex) = args.sex else {
        return Err(PhnError::InvalidInput(
            "give the child's sex (\"male\" or \"female\"); the growth charts are sex-specific"
                .into(),
        ));
    };
    if args.measurements.is_empty() {
        return Err(PhnError::InvalidInput(
            "give at least one visit in measurements, e.g. {\"age_months\": 9, \"weight_kg\": \
             9.7, \"length_or_height_cm\": 72}"
                .into(),
        ));
    }
    if args.measurements.len() > 60 {
        return Err(PhnError::InvalidInput(
            "at most 60 visits per call; use measurements_path for a file".into(),
        ));
    }
    let birth = match &args.birth_date {
        Some(b) => Some(parse_date(b).ok_or_else(|| {
            PhnError::InvalidInput("birth_date is not a date (use YYYY-MM-DD)".into())
        })?),
        None => None,
    };
    if let Some(w) = args.gestational_age_weeks
        && !(22..=44).contains(&w)
    {
        return Err(PhnError::InvalidInput(
            "gestational_age_weeks should be between 22 and 44".into(),
        ));
    }
    if args.gestational_age_days.is_some_and(|d| d > 6) {
        return Err(PhnError::InvalidInput(
            "gestational_age_days is the extra days (0-6) beyond completed weeks".into(),
        ));
    }
    let ga = ga_days(args.gestational_age_weeks, args.gestational_age_days);
    let correct = args.correct_for_prematurity.unwrap_or(true);
    let (tables, snap_sources) = load_tables(ctx)?;

    let mut visits: Vec<VisitResult> = Vec::new();
    let mut scored: Vec<(f64, Vec<IndicatorResult>)> = Vec::new();
    let mut any_corrected = false;
    for (i, m) in args.measurements.iter().enumerate() {
        let inp = inputs(m, birth).map_err(|e| match e {
            PhnError::InvalidInput(msg) => {
                PhnError::InvalidInput(format!("visit {}: {msg}", i + 1))
            }
            other => other,
        })?;
        let mut problems = Vec::new();
        let (age, corrected) = match scoring_age(inp.chrono_days, ga, correct) {
            Ok(v) => v,
            Err(msg) => {
                problems.push(msg);
                visits.push(VisitResult {
                    visit: i + 1,
                    chronological_age: age_label(inp.chrono_days),
                    chronological_age_months: inp.chrono_days / DAYS_PER_MONTH,
                    age_used: "none".into(),
                    corrected_age_months: None,
                    bmi: None,
                    indicators: Vec::new(),
                    changes_since_previous: Vec::new(),
                    problems,
                });
                continue;
            }
        };
        any_corrected |= corrected;
        let (bmi, ind) = score_visit(&tables, sex, &inp, age, &mut problems);
        let cur = (age, ind);
        let ch = scored.last().map(|p| changes(p, &cur)).unwrap_or_default();
        visits.push(VisitResult {
            visit: i + 1,
            chronological_age: age_label(inp.chrono_days),
            chronological_age_months: inp.chrono_days / DAYS_PER_MONTH,
            age_used: if corrected {
                "corrected".into()
            } else {
                "chronological".into()
            },
            corrected_age_months: corrected.then_some(age / DAYS_PER_MONTH),
            bmi,
            indicators: cur.1.clone(),
            changes_since_previous: ch,
            problems,
        });
        scored.push(cur);
    }

    // ---- Markdown
    let mut s = format!(
        "**Growth percentiles — {}{}**\n\n",
        sex.label(),
        match ga {
            Some(g) => format!(
                ", born at {} weeks {} days",
                (g / 7.0).floor(),
                (g % 7.0).round()
            ),
            None => String::new(),
        }
    );
    for v in &visits {
        s.push_str(&format!(
            "**Visit {}** — age {}{}{}\n\n",
            v.visit,
            v.chronological_age,
            v.corrected_age_months
                .map(|c| format!(" (corrected age {} used)", age_label(c * DAYS_PER_MONTH)))
                .unwrap_or_default(),
            v.bmi
                .map(|b| format!("; BMI {b:.1} kg/m²"))
                .unwrap_or_default()
        ));
        if !v.indicators.is_empty() {
            let rows: Vec<Vec<String>> = v
                .indicators
                .iter()
                .map(|r| {
                    let mut notes = r.flags.join("; ");
                    if let Some(cat) = &r.category {
                        notes = if notes.is_empty() {
                            cat.clone()
                        } else {
                            format!("{cat}; {notes}")
                        };
                    }
                    if let Some(e) = &r.extended_bmi
                        && r.percentile >= 95.0
                    {
                        notes.push_str(&format!(
                            "{}{:.0}% of the 95th percentile ({:.1})",
                            if notes.is_empty() { "" } else { "; " },
                            e.percent_of_95th_percentile,
                            e.bmi_95th_percentile
                        ));
                    }
                    vec![
                        r.indicator.clone(),
                        format!("{:.2} {}", r.value, r.unit),
                        pct_label(r.percentile),
                        fmt_z(r.z_score),
                        r.chart.clone(),
                        notes,
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &["Indicator", "Value", "Percentile", "z", "Chart", "Notes"],
                &rows,
            ));
            s.push('\n');
        }
        if !v.changes_since_previous.is_empty() {
            let parts: Vec<String> = v
                .changes_since_previous
                .iter()
                .map(|c| {
                    format!(
                        "{} z {} ({:+.2} {} in {:.0} days)",
                        c.indicator,
                        fmt_z(c.z_change),
                        c.value_change,
                        c.unit,
                        c.days
                    )
                })
                .collect();
            s.push_str(&format!(
                "Change since the previous visit: {}.\n\n",
                parts.join("; ")
            ));
        }
        for p in &v.problems {
            s.push_str(&format!("- Not scored or check: {p}\n"));
        }
        if !v.problems.is_empty() {
            s.push('\n');
        }
    }
    let c = content();
    s.push_str(&format!(
        "WHO standards are used before 24 months (cut points: {}th and {}th percentiles, ±2 SD); \
         CDC 2000 charts from 2 to 20 years, with the 2022 extended BMI-for-age method above the \
         95th percentile.\n",
        c.who_cut_points.low_percentile, c.who_cut_points.high_percentile
    ));

    let mut caveats = base_caveats();
    let corrected_rule = ga.map(|_| corrected_rule_note());
    if let Some(r) = &corrected_rule {
        caveats.push(format!(
            "Corrected age: {} The rule is LOW confidence in this draft: confirm the agency's \
             rule (some programs correct to 36 months for very preterm infants).",
            r.text
        ));
    }
    if any_corrected {
        caveats.push(
            "Scores marked 'corrected age used' compare the child with term-born children of \
             the same corrected age; chronological-age scores will be lower."
                .into(),
        );
    }
    if visits.iter().flat_map(|v| &v.indicators).any(|r| {
        r.extended_bmi
            .as_ref()
            .is_some_and(|e| e.extended_method_used)
    }) {
        caveats.push(
            "BMI above the 95th percentile uses CDC's 2022 extended method; the 2000-chart LMS \
             z-score (lms_z_score) is shown for comparison with older records and is not \
             meaningful at very high BMI."
                .into(),
        );
    }
    let data = GrowthResult {
        sex: Some(sex.label().into()),
        visits,
        corrected_age_rule: corrected_rule,
        interpretation_notes: interpretation_notes(),
        batch: None,
    };
    Ok(Envelope::new(s, data)
        .sources(snap_sources)
        .sources(content_sources())
        .source(Source::computed(
            "LMS z-scores and percentiles (CDC SAS program methods)",
            "PHN MCP server",
        ))
        .caveats(caveats)
        .privacy(Privacy {
            notes: vec![
                "Birth and measurement dates are used only to compute age and are not returned."
                    .into(),
            ],
            ..Privacy::none()
        })
        .decision_support())
}

// ---------------------------------------------------------------- batch mode

fn find_col(t: &phn_local::table::Table, names: &[&str]) -> Option<usize> {
    let norm = |s: &str| {
        phn_core::text::normalize(s)
            .replace(' ', "_")
            .trim_matches('_')
            .to_string()
    };
    t.headers
        .iter()
        .position(|h| names.iter().any(|n| norm(h) == *n))
}

fn num(s: &str) -> Option<f64> {
    s.trim()
        .replace(',', "")
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
}

/// Weight-status group for counting: WHO weight-for-length under 24 months, CDC BMI category
/// from 24 months.
fn status_of(v: &[IndicatorResult]) -> Option<String> {
    let c = content();
    if let Some(b) = v
        .iter()
        .find(|r| r.indicator == "BMI-for-age" && r.chart.starts_with("CDC"))
    {
        return b.category.clone();
    }
    let w = v.iter().find(|r| r.indicator == "Weight-for-length")?;
    Some(if w.percentile <= c.who_cut_points.low_percentile {
        "Low weight-for-length (WHO)".into()
    } else if w.percentile >= c.who_cut_points.high_percentile {
        "High weight-for-length (WHO)".into()
    } else {
        "Weight-for-length within ±2 SD (WHO)".into()
    })
}

fn age_group(months: f64) -> &'static str {
    if months < 24.0 {
        "Under 2 years"
    } else if months < 72.0 {
        "2-5 years"
    } else if months < 144.0 {
        "6-11 years"
    } else {
        "12-19 years"
    }
}

const AGE_GROUPS: [&str; 4] = ["Under 2 years", "2-5 years", "6-11 years", "12-19 years"];

fn batch(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    correct: bool,
) -> Result<Envelope<GrowthResult>> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let crate::localdata::LocalTable {
        table,
        label,
        findings,
    } = lt;
    let sex_col = find_col(&table, &["sex", "gender"]);
    let Some(sex_col) = sex_col else {
        return Err(PhnError::InvalidInput(format!(
            "the file needs a 'sex' column (M/F), an age column (age_months, age_days, or \
             age_years) or date_of_birth + measurement_date, and weight_kg/weight_lb, \
             height_cm/height_in, or head_circumference_cm. Columns found: {}",
            table.headers.join(", ")
        )));
    };
    let age_m = find_col(&table, &["age_months", "age_in_months", "agemos"]);
    let age_d = find_col(&table, &["age_days", "age_in_days", "agedays"]);
    let age_y = find_col(&table, &["age_years", "age_in_years", "age"]);
    let dob = find_col(&table, &["date_of_birth", "dob", "birth_date", "birthdate"]);
    let mdate = find_col(
        &table,
        &[
            "measurement_date",
            "date_measured",
            "visit_date",
            "date_of_measurement",
            "screening_date",
        ],
    );
    let wkg = find_col(&table, &["weight_kg", "wt_kg", "weight"]);
    let wlb = find_col(&table, &["weight_lb", "weight_lbs", "wt_lb"]);
    let hcm = find_col(
        &table,
        &[
            "height_cm",
            "length_cm",
            "stature_cm",
            "length_or_height_cm",
            "height",
            "length",
        ],
    );
    let hin = find_col(&table, &["height_in", "length_in", "height_inches"]);
    let hc = find_col(
        &table,
        &[
            "head_circumference_cm",
            "head_circ_cm",
            "hc_cm",
            "head_circumference",
        ],
    );
    let gaw = find_col(
        &table,
        &["gestational_age_weeks", "ga_weeks", "gestational_age"],
    );
    if age_m.is_none() && age_d.is_none() && age_y.is_none() && (dob.is_none() || mdate.is_none()) {
        return Err(PhnError::InvalidInput(
            "the file needs an age column (age_months, age_days, or age_years) or both \
             date_of_birth and measurement_date"
                .into(),
        ));
    }
    if wkg.is_none() && wlb.is_none() && hcm.is_none() && hin.is_none() && hc.is_none() {
        return Err(PhnError::InvalidInput(
            "the file needs measurement columns: weight_kg or weight_lb, height_cm or \
             height_in, head_circumference_cm"
                .into(),
        ));
    }
    let used: Vec<usize> = [sex_col]
        .into_iter()
        .chain(
            [age_m, age_d, age_y, dob, mdate, wkg, wlb, hcm, hin, hc, gaw]
                .into_iter()
                .flatten(),
        )
        .collect();
    // Identifier columns are copied to the output file only.
    let id_cols: Vec<usize> = table
        .headers
        .iter()
        .enumerate()
        .filter(|(i, h)| {
            !used.contains(i)
                && (findings
                    .iter()
                    .any(|f| &f.column == *h && f.kind.is_direct())
                    || {
                        let n = phn_core::text::normalize(h);
                        n.contains("name") || n.contains(" id") || n == "id" || n.contains("mrn")
                    })
        })
        .map(|(i, _)| i)
        .collect();
    let (tables, snap_sources) = load_tables(ctx)?;

    let mut out_rows: Vec<Vec<String>> = Vec::new();
    let mut problems_rows: Vec<Vec<String>> = Vec::new();
    let mut counts: BTreeMap<(usize, String), u64> = BTreeMap::new();
    let mut statuses: Vec<String> = Vec::new();
    let mut scored_rows = 0usize;
    let mut recheck = 0u64;
    for (ri, row) in table.rows.iter().enumerate() {
        let cell = |i: Option<usize>| {
            i.and_then(|i| row.get(i))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
        };
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let ids: Vec<String> = id_cols
            .iter()
            .map(|&i| row.get(i).cloned().unwrap_or_default())
            .collect();
        let fail = |msg: String, problems_rows: &mut Vec<Vec<String>>| {
            let mut r = vec![(ri + 2).to_string()];
            r.extend(ids.clone());
            r.push(msg);
            problems_rows.push(r);
        };
        let Some(sex) = cell(Some(sex_col)).and_then(Sex::parse) else {
            fail("sex is missing or not M/F".into(), &mut problems_rows);
            continue;
        };
        let birth = cell(dob).and_then(parse_date);
        let m = GrowthMeasurement {
            age_days: if age_m.is_none() {
                cell(age_d).and_then(num)
            } else {
                None
            },
            age_months: cell(age_m).and_then(num),
            age_years: if age_m.is_none() && age_d.is_none() {
                cell(age_y).and_then(num)
            } else {
                None
            },
            measurement_date: if age_m.is_none() && age_d.is_none() && age_y.is_none() {
                cell(mdate).map(str::to_string)
            } else {
                None
            },
            weight_kg: cell(wkg).and_then(num),
            weight_lb: if wkg.is_none() {
                cell(wlb).and_then(num)
            } else {
                None
            },
            weight_oz: None,
            length_or_height_cm: cell(hcm).and_then(num),
            length_or_height_in: if hcm.is_none() {
                cell(hin).and_then(num)
            } else {
                None
            },
            head_circumference_cm: cell(hc).and_then(num),
            head_circumference_in: None,
            position: None,
        };
        let inp = match inputs(&m, birth) {
            Ok(i) => i,
            Err(e) => {
                fail(e.to_string(), &mut problems_rows);
                continue;
            }
        };
        let ga = cell(gaw).and_then(num).map(|w| w * 7.0);
        let (age, corrected) = match scoring_age(inp.chrono_days, ga, correct) {
            Ok(v) => v,
            Err(msg) => {
                fail(msg, &mut problems_rows);
                continue;
            }
        };
        let mut probs = Vec::new();
        let (bmi, ind) = score_visit(&tables, sex, &inp, age, &mut probs);
        if ind.is_empty() {
            fail(
                if probs.is_empty() {
                    "nothing could be scored".into()
                } else {
                    probs.join("; ")
                },
                &mut problems_rows,
            );
            continue;
        }
        scored_rows += 1;
        let flagged = ind.iter().any(|r| r.check_measurement);
        if flagged {
            recheck += 1;
        }
        let months = age / DAYS_PER_MONTH;
        let g = AGE_GROUPS
            .iter()
            .position(|a| *a == age_group(months))
            .unwrap_or(0);
        if let Some(st) = status_of(&ind) {
            if !statuses.contains(&st) {
                statuses.push(st.clone());
            }
            *counts.entry((g, st)).or_default() += 1;
        }
        let find = |name: &str| ind.iter().find(|r| r.indicator == name);
        let mut line = vec![(ri + 2).to_string()];
        line.extend(ids.clone());
        line.push(age_label(inp.chrono_days));
        line.push(if corrected {
            age_label(age)
        } else {
            String::new()
        });
        for name in [
            "Weight-for-age",
            "Length-for-age",
            "Stature-for-age",
            "Weight-for-length",
            "BMI-for-age",
            "Head circumference-for-age",
        ] {
            match find(name) {
                Some(r) => {
                    line.push(pct_label(r.percentile));
                    line.push(format!("{:.2}", r.z_score));
                }
                None => {
                    line.push(String::new());
                    line.push(String::new());
                }
            }
        }
        line.push(bmi.map(|b| format!("{b:.1}")).unwrap_or_default());
        line.push(status_of(&ind).unwrap_or_default());
        line.push(
            ind.iter()
                .flat_map(|r| r.flags.iter().map(move |f| format!("{}: {f}", r.indicator)))
                .chain(probs)
                .collect::<Vec<_>>()
                .join("; "),
        );
        line.push(if flagged { "yes".into() } else { String::new() });
        out_rows.push(line);
    }

    // ---- Output file (identifiable).
    let id_headers: Vec<String> = id_cols.iter().map(|&i| table.headers[i].clone()).collect();
    let mut h1 = vec!["Row".to_string()];
    h1.extend(id_headers.clone());
    h1.extend(
        [
            "Chronological age",
            "Corrected age used",
            "Weight-for-age percentile",
            "Weight-for-age z",
            "Length-for-age percentile (WHO)",
            "Length-for-age z",
            "Stature-for-age percentile (CDC)",
            "Stature-for-age z",
            "Weight-for-length percentile (WHO)",
            "Weight-for-length z",
            "BMI-for-age percentile",
            "BMI-for-age z",
            "Head circumference percentile",
            "Head circumference z",
            "BMI",
            "Weight status",
            "Flags",
            "Re-measure",
        ]
        .map(String::from),
    );
    let mut h2 = vec!["Row".to_string()];
    h2.extend(id_headers);
    h2.push("Problem".into());
    let mut blocks = vec![Block::Paragraph(vec![Inline::plain(
        "Growth percentiles: WHO 2006 standards before 24 months, CDC 2000 charts from 2 to 20 \
         years with the 2022 extended BMI-for-age method. Decision support only; percentiles \
         are screening information. Draft content pending the nurse's review.",
    )])];
    blocks.push(Block::Table(TableBlock {
        caption: Some("Results".into()),
        headers: h1,
        rows: out_rows,
    }));
    if !problems_rows.is_empty() {
        blocks.push(Block::Table(TableBlock {
            caption: Some("Rows not scored".into()),
            headers: h2,
            rows: problems_rows.clone(),
        }));
    }
    let doc = Document {
        title: "Growth percentiles".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    };
    let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
    let sandbox = ctx.sandbox()?;
    let out = sandbox.new_output_file("growth_percentiles.xlsx")?;
    std::fs::write(&out, bytes)?;
    let out_display = sandbox.display_path(&out);

    // ---- Suppressed counts by age group and weight status.
    let threshold = ctx.suppression_threshold();
    let groups: Vec<usize> = (0..AGE_GROUPS.len())
        .filter(|g| counts.keys().any(|(k, _)| k == g))
        .collect();
    // Order statuses: WHO groups first, then CDC categories in content order.
    let c = content();
    let mut order: Vec<String> = vec![
        "Low weight-for-length (WHO)".into(),
        "Weight-for-length within ±2 SD (WHO)".into(),
        "High weight-for-length (WHO)".into(),
    ];
    order.extend(c.bmi_categories.iter().map(|b| b.label.clone()));
    statuses.sort_by_key(|s| order.iter().position(|o| o == s).unwrap_or(usize::MAX));
    let matrix: Vec<Vec<Option<u64>>> = groups
        .iter()
        .map(|g| {
            statuses
                .iter()
                .map(|s| Some(*counts.get(&(*g, s.clone())).unwrap_or(&0)))
                .collect()
        })
        .collect();
    let sup = suppress(
        &matrix,
        &SuppressionOptions {
            threshold,
            ..SuppressionOptions::default()
        },
    );
    let mut count_rows = Vec::new();
    for (ri, g) in groups.iter().enumerate() {
        let mut m = BTreeMap::new();
        for (ci, s) in statuses.iter().enumerate() {
            let v = matrix[ri][ci].unwrap_or(0);
            m.insert(
                s.clone(),
                if sup.is_hidden(ri, ci) {
                    "*".into()
                } else {
                    v.to_string()
                },
            );
        }
        count_rows.push(GrowthCountRow {
            age_group: AGE_GROUPS[*g].into(),
            counts: m,
        });
    }
    let recheck_label = if recheck > 0 && recheck < u64::from(threshold) {
        format!("fewer than {threshold}")
    } else {
        recheck.to_string()
    };

    let mut s = format!(
        "**Growth percentiles — file of measurements**\n\n{scored_rows} of {} row(s) scored{}. \
         Per-row results were written to `{out_display}` (not shown here).\n\n",
        scored_rows + problems_rows.len(),
        if problems_rows.is_empty() {
            String::new()
        } else {
            format!(
                "; {} row(s) could not be scored (listed in the output file)",
                problems_rows.len()
            )
        }
    );
    if !count_rows.is_empty() {
        let mut headers = vec!["Age group"];
        headers.extend(statuses.iter().map(String::as_str));
        let rows: Vec<Vec<String>> = count_rows
            .iter()
            .map(|r| {
                let mut v = vec![r.age_group.clone()];
                v.extend(
                    statuses
                        .iter()
                        .map(|s| r.counts.get(s).cloned().unwrap_or_default()),
                );
                v
            })
            .collect();
        s.push_str(&md::table(&headers, &rows));
        if sup.primary + sup.complementary > 0 {
            s.push_str(&format!(
                "\n`*` = suppressed (counts from 1 to {} and cells that would reveal them).\n",
                threshold - 1
            ));
        }
    }
    s.push_str(&format!(
        "\nRows with a value to re-measure (extreme z-score): {recheck_label}.\n"
    ));
    let mut caveats = base_caveats();
    caveats.push(
        "Counts are by row (child-visit). Weight status is WHO weight-for-length (±2 SD) under \
         2 years and the CDC BMI category from 2 years."
            .into(),
    );
    if correct && gaw.is_some() {
        caveats.push(format!(
            "Corrected age was used for rows with gestational age under 37 weeks until 24 \
             months (low-confidence draft rule: {}).",
            content().corrected_age.text
        ));
    }
    let data = GrowthResult {
        sex: None,
        visits: Vec::new(),
        corrected_age_rule: gaw.map(|_| corrected_rule_note()),
        interpretation_notes: interpretation_notes(),
        batch: Some(GrowthBatch {
            file: label.clone(),
            rows_read: table.rows.len(),
            rows_scored: scored_rows,
            rows_with_problems: problems_rows.len(),
            rows_to_recheck: recheck_label,
            counts: count_rows,
            output_file: out_display.clone(),
            suppression_threshold: threshold,
        }),
    };
    let mut kinds: Vec<String> = findings
        .iter()
        .map(|f| f.kind.label().to_string())
        .collect();
    kinds.sort();
    kinds.dedup();
    Ok(Envelope::new(s, data)
        .source(Source::local_file(label))
        .sources(snap_sources)
        .sources(content_sources())
        .caveats(caveats)
        .privacy(Privacy {
            row_level_data: false,
            suppression_applied: true,
            suppression_threshold: Some(threshold),
            identifiers_detected: kinds,
            files_written: vec![out_display],
            notes: vec![
                "Only suppressed counts by age group and weight status are returned; per-row \
                 results are in the output file."
                    .into(),
            ],
        })
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn visit(age_months: f64, kg: Option<f64>, cm: Option<f64>) -> GrowthMeasurement {
        GrowthMeasurement {
            age_months: Some(age_months),
            weight_kg: kg,
            length_or_height_cm: cm,
            ..Default::default()
        }
    }

    #[test]
    fn content_loads_and_cites_known_sources() {
        let c = content();
        for n in interpretation_notes() {
            assert!(!n.citation.is_empty());
        }
        assert!(c.sources.contains_key(&c.corrected_age.source));
        assert_eq!(c.bmi_categories.len(), 6);
    }

    #[test]
    fn infant_uses_who_and_flags_low_weight() {
        let (_d, c) = ctx();
        let env = calculate_growth_percentiles(
            &c,
            GrowthArgs {
                sex: Some(Sex::Male),
                measurements: vec![visit(9.0, Some(7.0), Some(70.0))],
                ..Default::default()
            },
        )
        .unwrap();
        let v = &env.data.visits[0];
        let w = v
            .indicators
            .iter()
            .find(|r| r.indicator == "Weight-for-age")
            .unwrap();
        assert_eq!(w.chart, "WHO 2006");
        assert!(w.z_score < -2.0);
        assert!(w.flags[0].contains("low weight-for-age"));
        assert!(
            v.indicators
                .iter()
                .any(|r| r.indicator == "Weight-for-length")
        );
        assert!(env.summary.contains("Visit 1"));
        assert!(env.notice.is_some());
    }

    #[test]
    fn cdc_extended_bmi_example_boy_50_5_months() {
        // CDC extended BMI data-file page: a boy aged 50.5 months with BMI 22.6 is above the
        // 95th percentile (17.8219); percentile 99.7683, z 2.83.
        let (_d, c) = ctx();
        let h = 1.0;
        let w = 22.6 * h * h;
        let env = calculate_growth_percentiles(
            &c,
            GrowthArgs {
                sex: Some(Sex::Male),
                measurements: vec![visit(50.5, Some(w), Some(100.0 * h))],
                ..Default::default()
            },
        )
        .unwrap();
        let b = env.data.visits[0]
            .indicators
            .iter()
            .find(|r| r.indicator == "BMI-for-age")
            .unwrap();
        assert!((b.percentile - 99.7683).abs() < 0.0001, "{}", b.percentile);
        assert!((b.z_score - 2.83).abs() < 0.005);
        let e = b.extended_bmi.as_ref().unwrap();
        assert!(e.extended_method_used);
        assert!((e.bmi_95th_percentile - 17.8219).abs() < 0.0001);
        assert_eq!(b.category.as_deref(), Some("Severe obesity (class 2)"));
    }

    #[test]
    fn corrected_age_for_preterm_infant_and_before_term() {
        let (_d, c) = ctx();
        let env = calculate_growth_percentiles(
            &c,
            GrowthArgs {
                sex: Some(Sex::Female),
                measurements: vec![
                    GrowthMeasurement {
                        age_days: Some(14.0),
                        weight_kg: Some(1.9),
                        ..Default::default()
                    },
                    visit(4.0, Some(5.2), Some(58.0)),
                ],
                gestational_age_weeks: Some(30),
                ..Default::default()
            },
        )
        .unwrap();
        let v = &env.data.visits;
        assert!(v[0].problems[0].contains("before term"));
        assert_eq!(v[1].age_used, "corrected");
        let corr = v[1].corrected_age_months.unwrap();
        assert!((corr - (4.0 * DAYS_PER_MONTH - 70.0) / DAYS_PER_MONTH).abs() < 1e-9);
        assert!(env.caveats.iter().any(|c| c.contains("LOW confidence")));
    }

    #[test]
    fn serial_visits_report_z_change_and_units_convert() {
        let (_d, c) = ctx();
        let env = calculate_growth_percentiles(
            &c,
            GrowthArgs {
                sex: Some(Sex::Female),
                measurements: vec![
                    GrowthMeasurement {
                        age_months: Some(2.0),
                        weight_lb: Some(11.0),
                        weight_oz: Some(4.0),
                        length_or_height_in: Some(22.5),
                        ..Default::default()
                    },
                    visit(4.0, Some(6.0), Some(60.0)),
                ],
                ..Default::default()
            },
        )
        .unwrap();
        let v = &env.data.visits;
        let w0 = v[0]
            .indicators
            .iter()
            .find(|r| r.indicator == "Weight-for-age")
            .unwrap();
        assert!((w0.value - 11.25 * 0.453_592_37).abs() < 1e-9);
        assert!(!v[1].changes_since_previous.is_empty());
    }

    #[test]
    fn bad_inputs_are_tool_errors() {
        let (_d, c) = ctx();
        let e = calculate_growth_percentiles(&c, GrowthArgs::default()).unwrap_err();
        assert!(e.to_string().contains("sex"));
        let e = calculate_growth_percentiles(
            &c,
            GrowthArgs {
                sex: Some(Sex::Male),
                measurements: vec![GrowthMeasurement {
                    age_months: Some(5.0),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("at least one measurement"));
        let e = calculate_growth_percentiles(
            &c,
            GrowthArgs {
                sex: Some(Sex::Male),
                measurements: vec![visit(250.0, Some(70.0), None)],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(e.data.visits[0].problems[0].contains("20 years"));
    }
}
