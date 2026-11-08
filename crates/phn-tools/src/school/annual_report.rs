//! SCH-07 `build_school_health_annual_report`.
//!
//! Compiles a school year's statistics from local files: a health office
//! visit log (encounters by type, dispositions and the returned-to-class
//! rate, dismissals, 911 calls, injuries and head injuries, medication
//! visits), an optional medication administration log, and an optional
//! screening roster (screened, referred, and completed referrals, plus BMI
//! counts by grade, sex, and category with DPH's data-quality checks). It
//! writes a workbook laid out by DPH's published reporting categories and a
//! summary for the school committee to the output folder, and returns only
//! suppressed aggregates.
//!
//! DPH does not publish its current Status Report form, so the workbook
//! follows what DPH does publish (see `content/school/annual_report.yaml`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::localdata;
use crate::md;
use crate::school::bmi::BmiCategory;
use crate::school::calendar::{Grade, SchoolYear};
use crate::school::content::{self, CitedOut, REPORTING, outs};
use crate::school::screenings::{self, BmiRow, ScreenStatus, ScreeningRow};
use crate::school::templates::DocFormat;

/// Arguments for `build_school_health_annual_report`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AnnualReportArgs {
    /// Health office visit log in /data (one row per visit): Date (required),
    /// Reason or Category, Disposition (returned to class, dismissed, 911,
    /// other), and optionally Grade, Type (student or staff), 911 Type
    /// (medical or behavioral), Injury, Head Injury, Student ID (used only to
    /// count distinct students; never returned).
    pub visit_log_path: Option<String>,
    pub visit_log_sheet: Option<String>,
    /// Medication administration log in /data (one row per dose): Date and
    /// Type (scheduled or PRN), optionally Given To (student or staff).
    pub medication_log_path: Option<String>,
    pub medication_log_sheet: Option<String>,
    /// Screening roster with results (the same file track_school_screenings reads).
    pub screening_path: Option<String>,
    pub screening_sheet: Option<String>,
    /// School year, e.g. "2025-2026" (default: the school year that ended most
    /// recently on or before as_of, or the current one if as_of is in it and
    /// the log has dates in it).
    pub school_year: Option<String>,
    /// Evaluation date (default today).
    pub as_of: Option<String>,
    /// Student enrollment, for rates per student.
    pub enrollment: Option<u32>,
    /// School or district name for the report title.
    pub school_name: Option<String>,
    /// Format of the school committee summary: docx (default) or pdf.
    pub summary_format: Option<DocFormat>,
}

/// One month of health office activity (suppressed counts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MonthRow {
    pub month: String,
    pub encounters: String,
    pub health_maintenance: String,
    pub injury_first_aid: String,
    pub mental_behavioral: String,
    pub miscellaneous: String,
    pub returned_to_class: String,
    pub dismissed_illness: String,
    pub dismissed_injury: String,
    pub emergency_911_medical: String,
    pub emergency_911_behavioral: String,
    pub other_disposition: String,
    pub head_injuries: String,
    pub staff_encounters: String,
}

/// Year totals and rates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct YearTotals {
    pub student_encounters: u64,
    pub staff_encounters: u64,
    /// Returned to class / assessed encounters with a disposition, percent.
    pub returned_to_class_pct: Option<f64>,
    pub dismissal_pct: Option<f64>,
    /// Share of dismissals for illness, percent.
    pub dismissals_for_illness_pct: Option<f64>,
    /// Distinct students who visited at least once / enrollment, percent.
    pub students_visiting_pct: Option<f64>,
    /// Student encounters per enrolled student per month.
    pub encounters_per_student_month: Option<f64>,
    /// Medication doses by type ("*" when suppressed).
    pub medication_doses: BTreeMap<String, String>,
}

/// BMI data-quality check for one grade (DPH ranges).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BmiQuality {
    pub grade: String,
    pub measured: usize,
    pub heights_out_of_range_pct: f64,
    pub weights_out_of_range_pct: f64,
    /// True when more than 1 percent are outside DPH's ranges (review before reporting).
    pub review: bool,
}

/// Result of `build_school_health_annual_report`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnnualReportResult {
    pub school_year: String,
    pub months: Vec<MonthRow>,
    pub totals: Option<YearTotals>,
    pub screenings: Vec<ScreeningRow>,
    pub bmi: Vec<BmiRow>,
    pub bmi_quality: Vec<BmiQuality>,
    /// DPH reports this school may owe, with who submits and when.
    pub dph_reports: Vec<DphReportOut>,
    pub definitions: Vec<CitedOut>,
    pub statewide_reference: Vec<CitedOut>,
    pub format_note: CitedOut,
    pub workbook: String,
    pub committee_summary: String,
    pub suppression_threshold: u32,
    pub notes: Vec<String>,
}

/// A DPH report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DphReportOut {
    pub name: String,
    pub who_submits: String,
    pub due: String,
    pub content: Vec<String>,
    pub citation: String,
}

// ------------------------------------------------------------------ visit log

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    HealthMaintenance,
    Injury,
    Mental,
    Misc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disposition {
    Returned,
    Dismissed,
    Ems,
    Other,
    Unknown,
}

fn has(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| {
        if w.len() <= 3 {
            t.split_whitespace().any(|x| x == *w)
        } else {
            t.contains(w)
        }
    })
}

/// Encounter category from a reason (tool default keywords; see caveats).
fn category(reason: &str) -> Category {
    let t = phn_core::text::normalize(reason);
    if has(
        &t,
        &[
            "injur",
            "first aid",
            "fall",
            "fell",
            "cut",
            "bruise",
            "sprain",
            "bump",
            "bleed",
            "scrape",
            "laceration",
            "splinter",
            "burn",
            "bite",
            "head injury",
            "concussion",
        ],
    ) {
        Category::Injury
    } else if has(
        &t,
        &[
            "anxi",
            "stress",
            "mental",
            "behavior",
            "behaviour",
            "counsel",
            "panic",
            "emotional",
            "depress",
            "suicid",
            "self harm",
            "crisis",
            "grief",
        ],
    ) {
        Category::Mental
    } else if has(
        &t,
        &[
            "medication",
            "med",
            "meds",
            "ill",
            "illness",
            "sick",
            "headache",
            "stomach",
            "fever",
            "asthma",
            "diabet",
            "glucose",
            "insulin",
            "chronic",
            "procedure",
            "nausea",
            "vomit",
            "cough",
            "sore",
            "ear",
            "rash",
            "health education",
            "health ed",
            "seizure",
            "allerg",
            "inhaler",
            "cramps",
            "dizz",
            "tooth",
            "dental",
            "eye",
        ],
    ) {
        Category::HealthMaintenance
    } else {
        Category::Misc
    }
}

fn disposition(s: &str) -> Disposition {
    let t = phn_core::text::normalize(s);
    if t.is_empty() {
        Disposition::Unknown
    } else if has(
        &t,
        &["911", "ems", "ambulance", "emergency department", "er"],
    ) {
        Disposition::Ems
    } else if has(
        &t,
        &[
            "dismiss",
            "sent home",
            "home",
            "picked up",
            "early release",
            "left school",
        ],
    ) {
        Disposition::Dismissed
    } else if has(&t, &["class", "returned", "rtc", "back to class"]) {
        Disposition::Returned
    } else {
        Disposition::Other
    }
}

fn col(h: &[String], names: &[&str]) -> Option<usize> {
    screenings::find_col(h, names).or_else(|| {
        h.iter().position(|x| {
            let n = phn_core::text::normalize(x);
            names.iter().any(|k| k.len() > 3 && n.contains(k))
        })
    })
}

fn cellv(row: &[String], i: Option<usize>) -> &str {
    i.and_then(|i| row.get(i))
        .map(String::as_str)
        .unwrap_or("")
        .trim()
}

const MONTH_FIELDS: usize = 13;

struct VisitStats {
    by_month: BTreeMap<(i32, u32), [u64; MONTH_FIELDS]>,
    distinct_students: Option<usize>,
    label: String,
    identifiers: Vec<String>,
    undated: usize,
    unknown_disposition: u64,
}

fn read_visits(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    year: SchoolYear,
) -> Result<VisitStats> {
    let lt = localdata::load(ctx, path, sheet)?;
    let h = &lt.table.headers;
    let date = col(h, &["date", "visit date", "encounter date"]).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "the visit log needs a 'Date' column. Columns found: {}",
            h.join(", ")
        ))
    })?;
    let reason = col(
        h,
        &[
            "reason",
            "category",
            "complaint",
            "visit reason",
            "chief complaint",
            "type of visit",
        ],
    );
    let disp = col(h, &["disposition", "outcome", "result"]);
    let who = col(
        h,
        &["person type", "visitor type", "type", "student or staff"],
    );
    let ems_type = col(h, &["911 type", "emergency type", "ems type"]);
    let injury = col(h, &["injury", "injury report"]);
    let head = col(h, &["head injury", "concussion"]);
    let id = col(h, &["student id", "id", "sasid", "student number"]);
    if reason.is_none() && disp.is_none() {
        return Err(PhnError::InvalidInput(
            "the visit log needs a 'Reason' (or 'Category') column and a 'Disposition' column"
                .into(),
        ));
    }
    let mut by_month: BTreeMap<(i32, u32), [u64; MONTH_FIELDS]> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut undated = 0usize;
    let mut unknown = 0u64;
    for row in &lt.table.rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let Some(d) = crate::vax::dates::parse_date(cellv(row, Some(date))) else {
            undated += 1;
            continue;
        };
        if !year.contains(d) {
            continue;
        }
        let m = by_month.entry((d.year(), d.month())).or_default();
        let staff =
            who.is_some_and(|i| phn_core::text::normalize(cellv(row, Some(i))).contains("staff"));
        if staff {
            m[12] += 1;
            continue;
        }
        m[0] += 1;
        let r = cellv(row, reason);
        let mut cat = category(r);
        if injury.is_some_and(|i| localdata::truthy(cellv(row, Some(i))) == Some(true)) {
            cat = Category::Injury;
        }
        match cat {
            Category::HealthMaintenance => m[1] += 1,
            Category::Injury => m[2] += 1,
            Category::Mental => m[3] += 1,
            Category::Misc => m[4] += 1,
        }
        match disposition(cellv(row, disp)) {
            Disposition::Returned => m[5] += 1,
            Disposition::Dismissed => {
                if cat == Category::Injury {
                    m[7] += 1;
                } else {
                    m[6] += 1;
                }
            }
            Disposition::Ems => {
                let behavioral = ems_type
                    .map(|i| phn_core::text::normalize(cellv(row, Some(i))))
                    .map(|t| t.contains("behav") || t.contains("mental") || t.contains("psych"))
                    .unwrap_or(cat == Category::Mental);
                if behavioral {
                    m[9] += 1;
                } else {
                    m[8] += 1;
                }
            }
            Disposition::Other => m[10] += 1,
            Disposition::Unknown => unknown += 1,
        }
        let head_injury = head
            .is_some_and(|i| localdata::truthy(cellv(row, Some(i))) == Some(true))
            || phn_core::text::normalize(r).contains("head injur")
            || phn_core::text::normalize(r).contains("concussion");
        if head_injury {
            m[11] += 1;
        }
        if let Some(i) = id {
            let v = cellv(row, Some(i));
            if !v.is_empty() {
                ids.insert(v.to_string());
            }
        }
    }
    Ok(VisitStats {
        by_month,
        distinct_students: id.map(|_| ids.len()),
        label: lt.label.clone(),
        identifiers: lt.identifier_categories(),
        undated,
        unknown_disposition: unknown,
    })
}

fn read_medications(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    year: SchoolYear,
) -> Result<(BTreeMap<String, u64>, String, Vec<String>)> {
    let lt = localdata::load(ctx, path, sheet)?;
    let h = &lt.table.headers;
    let date = col(h, &["date", "date given", "administration date"]).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "the medication log needs a 'Date' column. Columns found: {}",
            h.join(", ")
        ))
    })?;
    let kind = col(h, &["type", "dose type", "order type", "scheduled prn"]);
    let to = col(h, &["given to", "person type", "student or staff"]);
    let mut out: BTreeMap<String, u64> = BTreeMap::new();
    for row in &lt.table.rows {
        let Some(d) = crate::vax::dates::parse_date(cellv(row, Some(date))) else {
            continue;
        };
        if !year.contains(d) {
            continue;
        }
        let k = phn_core::text::normalize(cellv(row, kind));
        let staff =
            to.is_some_and(|i| phn_core::text::normalize(cellv(row, Some(i))).contains("staff"));
        let key = if staff {
            "staff doses"
        } else if k.contains("prn") || k.contains("as needed") {
            "PRN doses"
        } else if k.contains("sched") || k.contains("daily") || k.contains("routine") {
            "scheduled doses"
        } else {
            "other or unspecified doses"
        };
        *out.entry(key.to_string()).or_default() += 1;
    }
    Ok((out, lt.label.clone(), lt.identifier_categories()))
}

fn month_label(y: i32, m: u32) -> String {
    let names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{} {y}", names[(m - 1) as usize])
}

fn pct(n: u64, d: u64) -> Option<f64> {
    (d > 0).then(|| (1000.0 * n as f64 / d as f64).round() / 10.0)
}

// ------------------------------------------------------------------ tool

/// Compile the school health year-end report.
#[allow(clippy::too_many_lines)]
pub fn build_school_health_annual_report(
    ctx: &Context,
    args: AnnualReportArgs,
) -> Result<Envelope<AnnualReportResult>> {
    if args.visit_log_path.is_none()
        && args.screening_path.is_none()
        && args.medication_log_path.is_none()
    {
        return Err(PhnError::InvalidInput(
            "give at least one local file: visit_log_path (health office visits), screening_path \
             (screening roster), or medication_log_path"
                .into(),
        ));
    }
    let as_of = match &args.as_of {
        Some(s) => crate::vax::dates::require_date("as_of", s)?,
        None => chrono::Local::now().date_naive(),
    };
    let year = match &args.school_year {
        Some(s) => SchoolYear::parse(s).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "school_year '{s}' is not recognized; use e.g. 2025-2026"
            ))
        })?,
        None => SchoolYear::of(as_of),
    };
    let threshold = ctx.suppression_threshold();
    let mut sources = Vec::new();
    let mut identifiers: BTreeSet<String> = BTreeSet::new();
    let mut notes = Vec::new();
    let mut any_suppressed = false;

    // ---- Visits
    let visits = match &args.visit_log_path {
        Some(p) => Some(read_visits(ctx, p, args.visit_log_sheet.as_deref(), year)?),
        None => None,
    };
    let mut months = Vec::new();
    let mut month_rows_exact: Vec<Vec<String>> = Vec::new();
    let mut totals: Option<YearTotals> = None;
    let mut tot = [0u64; MONTH_FIELDS];
    if let Some(v) = &visits {
        sources.push(Source::local_file(v.label.clone()));
        identifiers.extend(v.identifiers.iter().cloned());
        let keys: Vec<(i32, u32)> = v.by_month.keys().copied().collect();
        let mut matrix: Vec<Vec<Option<u64>>> = Vec::new();
        for k in &keys {
            let m = v.by_month[k];
            for i in 0..MONTH_FIELDS {
                tot[i] += m[i];
            }
            matrix.push(m.iter().map(|x| Some(*x)).collect());
        }
        matrix.push(tot.iter().map(|x| Some(*x)).collect());
        let sup = suppress(
            &matrix,
            &SuppressionOptions {
                threshold,
                last_row_is_total: true,
                ..SuppressionOptions::default()
            },
        );
        any_suppressed |= sup.primary + sup.complementary > 0;
        let mut labels: Vec<String> = keys.iter().map(|(y, m)| month_label(*y, *m)).collect();
        labels.push("Year".into());
        for (ri, label) in labels.iter().enumerate() {
            let s = |ci: usize| {
                if sup.is_hidden(ri, ci) {
                    "*".to_string()
                } else {
                    matrix[ri][ci].unwrap_or(0).to_string()
                }
            };
            months.push(MonthRow {
                month: label.clone(),
                encounters: s(0),
                health_maintenance: s(1),
                injury_first_aid: s(2),
                mental_behavioral: s(3),
                miscellaneous: s(4),
                returned_to_class: s(5),
                dismissed_illness: s(6),
                dismissed_injury: s(7),
                emergency_911_medical: s(8),
                emergency_911_behavioral: s(9),
                other_disposition: s(10),
                head_injuries: s(11),
                staff_encounters: s(12),
            });
            let mut exact = vec![label.clone()];
            exact.extend(matrix[ri].iter().map(|x| x.unwrap_or(0).to_string()));
            month_rows_exact.push(exact);
        }
        let assessed = tot[5] + tot[6] + tot[7] + tot[8] + tot[9] + tot[10];
        let dismissed = tot[6] + tot[7];
        let n_months = keys.len().max(1) as f64;
        totals = Some(YearTotals {
            student_encounters: tot[0],
            staff_encounters: tot[12],
            returned_to_class_pct: pct(tot[5], assessed),
            dismissal_pct: pct(dismissed, assessed),
            dismissals_for_illness_pct: pct(tot[6], dismissed),
            students_visiting_pct: match (v.distinct_students, args.enrollment) {
                (Some(n), Some(e)) if e > 0 => pct(n as u64, u64::from(e)),
                _ => None,
            },
            encounters_per_student_month: args
                .enrollment
                .filter(|e| *e > 0)
                .map(|e| ((tot[0] as f64 / f64::from(e) / n_months) * 100.0).round() / 100.0),
            medication_doses: BTreeMap::new(),
        });
        if v.undated > 0 {
            notes.push(format!(
                "{} visit(s) had no valid date and were skipped.",
                v.undated
            ));
        }
        if v.unknown_disposition > 0 {
            notes.push(format!(
                "{} student visit(s) had no disposition and are left out of the returned-to-class rate.",
                v.unknown_disposition
            ));
        }
        if tot[0] < u64::from(threshold) * 4 {
            notes.push("Few visits were found for this school year; check the school_year and the dates in the log.".into());
        }
    }

    // ---- Medications
    let mut med_exact: BTreeMap<String, u64> = BTreeMap::new();
    if let Some(p) = &args.medication_log_path {
        let (m, label, ids) = read_medications(ctx, p, args.medication_log_sheet.as_deref(), year)?;
        sources.push(Source::local_file(label));
        identifiers.extend(ids);
        med_exact = m;
        let keys: Vec<String> = med_exact.keys().cloned().collect();
        let matrix: Vec<Vec<Option<u64>>> = keys.iter().map(|k| vec![Some(med_exact[k])]).collect();
        let sup = suppress(
            &matrix,
            &SuppressionOptions {
                threshold,
                complementary: false,
                ..SuppressionOptions::default()
            },
        );
        any_suppressed |= sup.primary > 0;
        let shown: BTreeMap<String, String> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| {
                (
                    k.clone(),
                    if sup.is_hidden(i, 0) {
                        "*".into()
                    } else {
                        med_exact[k].to_string()
                    },
                )
            })
            .collect();
        match &mut totals {
            Some(t) => t.medication_doses = shown,
            None => {
                totals = Some(YearTotals {
                    student_encounters: 0,
                    staff_encounters: 0,
                    returned_to_class_pct: None,
                    dismissal_pct: None,
                    dismissals_for_illness_pct: None,
                    students_visiting_pct: None,
                    encounters_per_student_month: None,
                    medication_doses: shown,
                })
            }
        }
    }

    // ---- Screenings and BMI
    let mut screening_rows = Vec::new();
    let mut bmi = Vec::new();
    let mut bmi_quality = Vec::new();
    let mut bmi_exact: Vec<Vec<String>> = Vec::new();
    let mut screen_exact: Vec<Vec<String>> = Vec::new();
    if let Some(p) = &args.screening_path {
        let eval_on = if year.contains(as_of) {
            as_of
        } else {
            year.end()
        };
        let a = screenings::analyze(ctx, p, args.screening_sheet.as_deref(), eval_on, None)?;
        sources.push(Source::local_file(a.lt.label.clone()));
        identifiers.extend(a.lt.identifier_categories());
        let (rows, s1) = screenings::screening_rows(&a, threshold);
        let (b, s2, without) = screenings::bmi_rows(&a, threshold);
        any_suppressed |= s1 || s2;
        screening_rows = rows
            .into_iter()
            .filter(|r| r.grade == "All grades")
            .collect();
        bmi = b;
        if without > 0 {
            notes.push(format!(
                "{without} BMI measurement(s) had no category and are not in the BMI counts."
            ));
        }
        // Exact counts for the local workbook.
        for sc in &a.tracked {
            let mut t: BTreeMap<ScreenStatus, u64> = BTreeMap::new();
            for st in &a.students {
                if let Some(r) = st.results.iter().find(|r| r.screening == sc.id) {
                    *t.entry(r.status).or_default() += 1;
                }
            }
            let g = |s: ScreenStatus| t.get(&s).copied().unwrap_or(0);
            let screened = g(ScreenStatus::Passed)
                + g(ScreenStatus::ReferredPending)
                + g(ScreenStatus::ReferralCompleted);
            screen_exact.push(vec![
                sc.name.clone(),
                screened.to_string(),
                (g(ScreenStatus::ReferredPending) + g(ScreenStatus::ReferralCompleted)).to_string(),
                g(ScreenStatus::ReferralCompleted).to_string(),
                g(ScreenStatus::Due).to_string(),
                g(ScreenStatus::Exempt).to_string(),
            ]);
        }
        let mut cat_counts: BTreeMap<(String, String), [u64; 4]> = BTreeMap::new();
        let mut quality: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
        let bmi_cols = a
            .screening_cols
            .get("height_weight_bmi")
            .cloned()
            .unwrap_or_default();
        for st in &a.students {
            if let Some(r) = st
                .results
                .iter()
                .find(|r| r.screening == "height_weight_bmi" && r.status == ScreenStatus::Passed)
            {
                if let Some(cat) = r.bmi_category {
                    let sex = st
                        .sex
                        .map(|s| s.label().to_string())
                        .unwrap_or_else(|| "not recorded".into());
                    let e = cat_counts.entry((st.grade.label(), sex)).or_default();
                    e[BmiCategory::ALL.iter().position(|c| *c == cat).unwrap_or(0)] += 1;
                }
                let row = &a.lt.table.rows[st.row];
                let num = |i: Option<usize>| {
                    i.and_then(|i| row.get(i))
                        .and_then(|s| s.trim().parse::<f64>().ok())
                };
                if let Some(q) = REPORTING
                    .bmi_quality_ranges
                    .iter()
                    .find(|q| q.grade == st.grade.label())
                {
                    let e = quality.entry(q.grade.clone()).or_default();
                    e.0 += 1;
                    if let Some(hv) = num(bmi_cols.height) {
                        let hin = if bmi_cols.metric_height {
                            hv / 2.54
                        } else {
                            hv
                        };
                        if hin < q.height_in[0] || hin > q.height_in[1] {
                            e.1 += 1;
                        }
                    }
                    if let Some(wv) = num(bmi_cols.weight) {
                        let wlb = if bmi_cols.metric_weight {
                            wv * 2.204_622_6
                        } else {
                            wv
                        };
                        if wlb < q.weight_lb[0] || wlb > q.weight_lb[1] {
                            e.2 += 1;
                        }
                    }
                }
            }
        }
        for ((g, s), c) in &cat_counts {
            let mut r = vec![g.clone(), s.clone(), c.iter().sum::<u64>().to_string()];
            r.extend(c.iter().map(|x| x.to_string()));
            bmi_exact.push(r);
        }
        let mut qk: Vec<&String> = quality.keys().collect();
        qk.sort_by_key(|g| Grade::parse(g).map(Grade::ordinal).unwrap_or(99));
        for g in qk {
            let (n, ho, wo) = quality[g];
            let hp = 100.0 * ho as f64 / n.max(1) as f64;
            let wp = 100.0 * wo as f64 / n.max(1) as f64;
            bmi_quality.push(BmiQuality {
                grade: g.clone(),
                measured: n,
                heights_out_of_range_pct: (hp * 10.0).round() / 10.0,
                weights_out_of_range_pct: (wp * 10.0).round() / 10.0,
                review: hp > 1.0 || wp > 1.0,
            });
        }
        if a.problems.len() > 0 {
            notes.push(format!(
                "{} roster row(s) could not be evaluated.",
                a.problems.len()
            ));
        }
    }

    // ---- DPH reports and reference
    let r = &*REPORTING;
    let dph_reports: Vec<DphReportOut> = r
        .reports
        .iter()
        .map(|x| DphReportOut {
            name: x.name.clone(),
            who_submits: x.who_submits.trim().to_string(),
            due: x.due.clone(),
            content: x.content.clone(),
            citation: content::citation(&r.sources, &x.source, &x.section),
        })
        .collect();
    let definitions = outs(&r.definitions, &r.sources);
    let statewide = outs(&r.statewide_reference, &r.sources);
    let format_note = r.format_note.out(&r.sources);
    let school = args
        .school_name
        .clone()
        .unwrap_or_else(|| "School health services".into());

    // ---- Workbook (exact counts; aggregate only).
    let mut blocks = vec![Block::Paragraph(vec![Inline::plain(format!(
        "{school}: school health year-end data, {} school year. Categories follow DPH's published \
         Essential School Health Services reporting; match them to DPH's current forms before \
         submitting. Aggregate counts only.",
        year.label()
    ))])];
    if !month_rows_exact.is_empty() {
        blocks.push(Block::Table(TableBlock {
            caption: Some("Monthly activities".into()),
            headers: [
                "Month",
                "Student encounters",
                "Health maintenance",
                "Injury/first aid",
                "Mental/behavioral health",
                "Miscellaneous",
                "Returned to class",
                "Dismissed (illness)",
                "Dismissed (injury)",
                "911 medical",
                "911 behavioral health",
                "Other disposition",
                "Head injuries",
                "Staff encounters",
            ]
            .map(String::from)
            .to_vec(),
            rows: month_rows_exact.clone(),
        }));
    }
    if !med_exact.is_empty() {
        blocks.push(Block::Table(TableBlock {
            caption: Some("Medication doses".into()),
            headers: vec!["Type".into(), "Doses".into()],
            rows: med_exact
                .iter()
                .map(|(k, v)| vec![k.clone(), v.to_string()])
                .collect(),
        }));
    }
    if !screen_exact.is_empty() {
        blocks.push(Block::Table(TableBlock {
            caption: Some("Screenings (Status Report)".into()),
            headers: [
                "Screening",
                "Screened",
                "Referred",
                "Completed referrals",
                "Due, not screened",
                "Exempt",
            ]
            .map(String::from)
            .to_vec(),
            rows: screen_exact,
        }));
    }
    if !bmi_exact.is_empty() {
        let mut hh = vec![
            "Grade".to_string(),
            "Sex".to_string(),
            "Measured".to_string(),
        ];
        hh.extend(BmiCategory::ALL.iter().map(|c| c.label().to_string()));
        blocks.push(Block::Table(TableBlock {
            caption: Some("BMI report (grade, sex, category)".into()),
            headers: hh,
            rows: bmi_exact,
        }));
        blocks.push(Block::Table(TableBlock {
            caption: Some("BMI data quality (DPH ranges)".into()),
            headers: [
                "Grade",
                "Measured",
                "Heights out of range %",
                "Weights out of range %",
                "Review",
            ]
            .map(String::from)
            .to_vec(),
            rows: bmi_quality
                .iter()
                .map(|q| {
                    vec![
                        q.grade.clone(),
                        q.measured.to_string(),
                        format!("{:.1}", q.heights_out_of_range_pct),
                        format!("{:.1}", q.weights_out_of_range_pct),
                        if q.review { "yes".into() } else { "no".into() },
                    ]
                })
                .collect(),
        }));
    }
    blocks.push(Block::Table(TableBlock {
        caption: Some("DPH reports".into()),
        headers: ["Report", "Who submits", "Due", "Source"]
            .map(String::from)
            .to_vec(),
        rows: dph_reports
            .iter()
            .map(|x| {
                vec![
                    x.name.clone(),
                    x.who_submits.clone(),
                    x.due.clone(),
                    x.citation.clone(),
                ]
            })
            .collect(),
    }));
    blocks.push(Block::Table(TableBlock {
        caption: Some("Definitions".into()),
        headers: vec!["Definition".into(), "Source".into()],
        rows: definitions
            .iter()
            .map(|d| vec![d.text.clone(), d.citation.clone()])
            .collect(),
    }));
    let doc = Document {
        title: format!("School health annual data {}", year.label()),
        subtitle: Some(school.clone()),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    };
    let sandbox = ctx.sandbox()?;
    let wb = sandbox.new_output_file(&format!("school_health_annual_{}.xlsx", year.label()))?;
    std::fs::write(&wb, phn_docs::xlsx::to_xlsx(&doc)?)?;
    let workbook = sandbox.display_path(&wb);

    // ---- School committee summary (suppressed aggregates).
    let mut cs = format!(
        "# School health services: year in review\n\n{school}, {} school year.\n\n",
        year.label()
    );
    if let Some(t) = &totals {
        if t.student_encounters > 0 {
            cs.push_str(&format!(
                "School nurses had {} student health encounters{}. ",
                t.student_encounters,
                t.encounters_per_student_month
                    .map(|x| format!(" (about {x:.2} per enrolled student per month)"))
                    .unwrap_or_default()
            ));
            if let Some(p) = t.returned_to_class_pct {
                cs.push_str(&format!(
                    "After assessment, {p:.1}% of students returned to class, so most visits kept students learning without a trip home. "
                ));
            }
            if let Some(p) = t.students_visiting_pct {
                cs.push_str(&format!(
                    "{p:.1}% of enrolled students visited the health office at least once. "
                ));
            }
            cs.push_str("\n\n");
        }
    }
    if !months.is_empty() {
        cs.push_str("| Month | Encounters | Returned to class | Dismissed | 911 calls | Head injuries |\n|---|---|---|---|---|---|\n");
        for m in &months {
            let dis = sum_or_star(&m.dismissed_illness, &m.dismissed_injury);
            let ems = sum_or_star(&m.emergency_911_medical, &m.emergency_911_behavioral);
            cs.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                m.month, m.encounters, m.returned_to_class, dis, ems, m.head_injuries
            ));
        }
        cs.push('\n');
    }
    if !screening_rows.is_empty() {
        cs.push_str("| Screening | Required | Screened | Completion % | Referred | Follow-up % |\n|---|---|---|---|---|---|\n");
        for x in &screening_rows {
            cs.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                x.screening, x.required, x.screened, x.completion_pct, x.referred, x.follow_up_pct
            ));
        }
        cs.push('\n');
    }
    if any_suppressed {
        cs.push_str(&format!(
            "Counts from 1 to {} are shown as * to protect privacy.\n\n",
            threshold - 1
        ));
    }
    cs.push_str(
        "Statewide comparison (DPH Essential School Health Services, 2011-2012): 93.1% of students \
         returned to class after a nursing assessment.\n",
    );
    let fmt = args.summary_format.unwrap_or_default();
    let cdoc = crate::school::templates::document(
        ctx,
        &cs,
        "en-US",
        "School health services: year in review",
    );
    let cpath = sandbox.new_output_file(&format!(
        "school_health_committee_summary_{}.{}",
        year.label(),
        fmt.ext()
    ))?;
    std::fs::write(&cpath, fmt.render(&cdoc)?)?;
    let committee_summary = sandbox.display_path(&cpath);

    // ---- Markdown
    let mut s = format!(
        "**School health year-end report — {} school year**\n\nWorkbook: `{workbook}`. School committee summary: `{committee_summary}`.\n\n",
        year.label()
    );
    if let Some(t) = &totals {
        if t.student_encounters > 0 {
            s.push_str(&format!(
                "- Student encounters: {}; staff encounters: {}\n- Returned to class: {}; dismissed: {}; dismissals for illness: {}\n",
                t.student_encounters,
                t.staff_encounters,
                t.returned_to_class_pct.map(|p| format!("{p:.1}%")).unwrap_or_else(|| "—".into()),
                t.dismissal_pct.map(|p| format!("{p:.1}%")).unwrap_or_else(|| "—".into()),
                t.dismissals_for_illness_pct.map(|p| format!("{p:.1}%")).unwrap_or_else(|| "—".into()),
            ));
            if let Some(p) = t.students_visiting_pct {
                s.push_str(&format!(
                    "- Students who visited at least once: {p:.1}% of enrollment\n"
                ));
            }
        }
        if !t.medication_doses.is_empty() {
            s.push_str(&format!(
                "- Medication doses: {}\n",
                t.medication_doses
                    .iter()
                    .map(|(k, v)| format!("{k} {v}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        s.push('\n');
    }
    if !months.is_empty() {
        let rows: Vec<Vec<String>> = months
            .iter()
            .map(|m| {
                vec![
                    m.month.clone(),
                    m.encounters.clone(),
                    m.health_maintenance.clone(),
                    m.injury_first_aid.clone(),
                    m.mental_behavioral.clone(),
                    m.returned_to_class.clone(),
                    m.dismissed_illness.clone(),
                    m.dismissed_injury.clone(),
                    m.emergency_911_medical.clone(),
                    m.emergency_911_behavioral.clone(),
                    m.head_injuries.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Month",
                "Encounters",
                "Health maint.",
                "Injury",
                "Mental/behav.",
                "Returned",
                "Dismissed (ill)",
                "Dismissed (inj)",
                "911 med",
                "911 behav",
                "Head inj.",
            ],
            &rows,
        ));
    }
    if !screening_rows.is_empty() {
        s.push_str("\n**Screenings (all grades)**\n\n");
        let rows: Vec<Vec<String>> = screening_rows
            .iter()
            .map(|x| {
                vec![
                    x.screening.clone(),
                    x.required.clone(),
                    x.screened.clone(),
                    x.completion_pct.clone(),
                    x.referred.clone(),
                    x.referral_pct.clone(),
                    x.referrals_completed.clone(),
                    x.follow_up_pct.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Screening",
                "Required",
                "Screened",
                "Completion %",
                "Referred",
                "Referral %",
                "Completed",
                "Follow-up %",
            ],
            &rows,
        ));
    }
    if !bmi.is_empty() {
        s.push_str("\n**BMI report (grade, sex, category)**\n\n");
        let mut headers = vec!["Grade", "Sex", "Measured"];
        headers.extend(BmiCategory::ALL.iter().map(|c| c.label()));
        let rows: Vec<Vec<String>> = bmi
            .iter()
            .map(|r| {
                let mut v = vec![r.grade.clone(), r.sex.clone(), r.measured.clone()];
                v.extend(
                    BmiCategory::ALL
                        .iter()
                        .map(|c| r.categories[c.label()].clone()),
                );
                v
            })
            .collect();
        s.push_str(&md::table(&headers, &rows));
        for q in bmi_quality.iter().filter(|q| q.review) {
            s.push_str(&format!(
                "- Grade {}: {:.1}% of heights and {:.1}% of weights are outside DPH's expected range; review the measurements before reporting.\n",
                q.grade, q.heights_out_of_range_pct, q.weights_out_of_range_pct
            ));
        }
    }
    if any_suppressed {
        s.push_str(&format!(
            "\n`*` = suppressed (counts from 1 to {} and cells that would reveal them). Exact counts are in the workbook.\n",
            threshold - 1
        ));
    }
    s.push_str("\n**DPH reports to check**\n\n");
    for x in &dph_reports {
        s.push_str(&format!(
            "- {} — {}; due: {}\n",
            x.name, x.who_submits, x.due
        ));
    }
    s.push_str(&format!("\n{}\n", format_note.text));
    for n in &notes {
        s.push_str(&format!("\n{n}\n"));
    }

    let mut caveats = vec![
        "Visit reasons and dispositions are sorted into DPH's published categories (health \
         maintenance, injury/first aid, mental/behavioral health, miscellaneous; returned to class, \
         dismissed, 911, other) by keywords that are tool defaults; check the workbook against your \
         records."
            .to_string(),
        "For the 105 CMR 201.017 head injury statistics, use concussion_return_to_learn_tracker \
         with your head injury log (it counts Report of Head Injury forms)."
            .into(),
    ];
    caveats.extend(content::review(&[
        content::ANNUAL_REPORT,
        content::SCREENING,
    ]));
    sources.extend(content::sources_for(
        &r.sources,
        [
            "dph-data-tools",
            "eshs-2012",
            "cmr200",
            "dph-bmi-guidelines",
        ],
    ));
    let mut files = vec![workbook.clone(), committee_summary.clone()];
    files.dedup();
    Ok(Envelope::new(
        s,
        AnnualReportResult {
            school_year: year.label(),
            months,
            totals,
            screenings: screening_rows,
            bmi,
            bmi_quality,
            dph_reports,
            definitions,
            statewide_reference: statewide,
            format_note,
            workbook,
            committee_summary,
            suppression_threshold: threshold,
            notes,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: identifiers.into_iter().collect(),
        files_written: files,
        notes: vec!["Only aggregate counts are returned and written; student IDs are used only to count distinct students.".into()],
    })
    .decision_support())
}

fn sum_or_star(a: &str, b: &str) -> String {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => (x + y).to_string(),
        _ => "*".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::fmt::Write as _;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn categories_and_dispositions() {
        assert_eq!(category("fell on playground"), Category::Injury);
        assert_eq!(category("Anxiety"), Category::Mental);
        assert_eq!(category("headache"), Category::HealthMaintenance);
        assert_eq!(
            category("scheduled medication"),
            Category::HealthMaintenance
        );
        assert_eq!(category("lost retainer"), Category::Misc);
        assert_eq!(disposition("Returned to class"), Disposition::Returned);
        assert_eq!(disposition("sent home"), Disposition::Dismissed);
        assert_eq!(disposition("911"), Disposition::Ems);
        assert_eq!(disposition("stayed in health office"), Disposition::Other);
    }

    #[test]
    fn year_report_from_visit_log() {
        let (d, c) = ctx();
        let mut body = String::from("Date,Student ID,Student Name,Grade,Reason,Disposition\n");
        for i in 0..120 {
            let day = NaiveDate::from_ymd_opt(2025, 9, 2).unwrap() + chrono::Duration::days(i * 2);
            let (reason, disp) = match i % 6 {
                0 => ("fell at recess", "returned to class"),
                1 => ("stomach ache", "sent home"),
                2 => ("anxiety", "returned to class"),
                3 => ("headache", "returned to class"),
                4 => ("scheduled medication", "returned to class"),
                _ => ("fever", "dismissed"),
            };
            writeln!(
                body,
                "{day},93{:05},Zed Quorra{},5,{reason},{disp}",
                i % 40,
                i % 40
            )
            .unwrap();
        }
        writeln!(
            body,
            "2025-10-01,9300099,Zed Quorra,5,head injury at recess,911"
        )
        .unwrap();
        std::fs::write(d.path().join("data/visits.csv"), body).unwrap();
        let e = build_school_health_annual_report(
            &c,
            AnnualReportArgs {
                visit_log_path: Some("visits.csv".into()),
                school_year: Some("2025-2026".into()),
                enrollment: Some(400),
                ..Default::default()
            },
        )
        .unwrap();
        let t = e.data.totals.as_ref().unwrap();
        assert_eq!(t.student_encounters, 121);
        assert!(
            (t.returned_to_class_pct.unwrap() - 66.1).abs() < 0.1,
            "{:?}",
            t.returned_to_class_pct
        );
        assert_eq!(t.students_visiting_pct, Some(10.3));
        assert!(e.data.months.iter().any(|m| m.month == "Year"));
        let all = serde_json::to_string(&e).unwrap();
        assert!(!all.contains("Quorra") && !all.contains("9300001"));
        assert!(d.path().join("data").join(&e.data.workbook).exists());
        assert!(
            d.path()
                .join("data")
                .join(&e.data.committee_summary)
                .exists()
        );
        assert!(e.data.dph_reports.iter().any(|r| r.name.starts_with("BMI")));
    }

    #[test]
    fn needs_a_file() {
        let (_d, c) = ctx();
        assert!(build_school_health_annual_report(&c, AnnualReportArgs::default()).is_err());
    }
}
