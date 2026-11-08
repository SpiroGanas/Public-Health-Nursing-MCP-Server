//! SCH-02 `track_school_screenings`.
//!
//! Reads a local roster with screening results (one row per student), works
//! out which required screenings each student is due for in the school year
//! (105 CMR 200.000 grade rules, via SCH-01's content), and computes
//! completion, referral, and referral follow-up rates by grade. Per-student
//! results go to an XLSX in the output folder; referral letters can be
//! mail-merged in each family's language (agency translations from
//! `/data/templates/<language>/`, English otherwise). Only suppressed counts
//! and rates are returned.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::localdata::{self, LocalTable};
use crate::md;
use crate::school::bmi::{self, BmiCategory, Sex};
use crate::school::calendar::{Grade, SchoolYear, age_years};
use crate::school::content::{self, Frequency, SCREENINGS, Screening};
use crate::school::screening_requirements::band_for;
use crate::school::templates::{self, DocFormat, LanguageCount, TemplateResolver};

/// Arguments for `track_school_screenings`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TrackScreeningsArgs {
    /// CSV or XLSX in /data, one row per student: Grade (required), Date of
    /// Birth, Sex, Home Language, New Student, and per screening a date, a
    /// result (pass, refer, absent, refused, exempt), and a referral-completed
    /// column, e.g. "Vision Date", "Vision Result", "Vision Referral Completed".
    /// BMI uses "Height (in)", "Weight (lb)" (or cm/kg), or "BMI Percentile" /
    /// "BMI Category". Names and IDs are copied to the output file only.
    pub path: String,
    /// Sheet name for an XLSX file.
    pub sheet: Option<String>,
    /// Evaluation date (default today); it sets the school year (July-June).
    pub as_of: Option<String>,
    /// Screenings to track: vision, hearing, bmi, postural, kindergarten vision
    /// certificate (default: every one with columns in the file).
    pub screenings: Option<Vec<String>>,
    /// Write referral letters (vision, hearing, postural) for students whose
    /// referral follow-up is not complete, in each family's language when the
    /// agency has a translation in /data/templates/<language>/ (default false).
    pub write_letters: Option<bool>,
    /// Letter format: docx (default) or pdf.
    pub letter_format: Option<DocFormat>,
    /// School name for the letters (or a "School" column in the file).
    pub school_name: Option<String>,
    /// Your name, as it should appear on the letters.
    pub nurse_name: Option<String>,
    /// Your phone number for the letters.
    pub nurse_phone: Option<String>,
}

/// Status of one screening for one student.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ScreenStatus {
    /// Screened and passed (or measured, for BMI).
    Passed,
    /// Did not pass; referral follow-up not yet documented.
    ReferredPending,
    /// Did not pass; follow-up exam documented.
    ReferralCompleted,
    /// Attempted but not completed (absent, refused, needs rescreen).
    Incomplete,
    /// Required but no result in the window.
    Due,
    /// Religious exemption or written opt-out.
    Exempt,
    /// Not required for this grade this year.
    NotRequired,
}

impl ScreenStatus {
    pub fn label(self) -> &'static str {
        match self {
            ScreenStatus::Passed => "passed",
            ScreenStatus::ReferredPending => "referred, follow-up pending",
            ScreenStatus::ReferralCompleted => "referred, follow-up done",
            ScreenStatus::Incomplete => "incomplete",
            ScreenStatus::Due => "due",
            ScreenStatus::Exempt => "exempt or opted out",
            ScreenStatus::NotRequired => "not required",
        }
    }
}

/// Suppressed counts and rates for one grade (or all grades) and screening.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreeningRow {
    pub screening: String,
    pub grade: String,
    /// Students required to be screened this year (including exempt).
    pub required: String,
    pub screened: String,
    pub referred: String,
    pub referrals_completed: String,
    pub follow_up_pending: String,
    pub incomplete: String,
    pub exempt: String,
    pub due: String,
    /// Screened / (required - exempt), percent; "*" when a count is suppressed.
    pub completion_pct: String,
    /// Referred / screened, percent.
    pub referral_pct: String,
    /// Referrals completed / referred, percent.
    pub follow_up_pct: String,
}

/// BMI category counts (suppressed) by grade and sex, for DPH's BMI report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BmiRow {
    pub grade: String,
    pub sex: String,
    pub measured: String,
    /// Category label -> count ("*" when suppressed).
    pub categories: BTreeMap<String, String>,
}

/// Letters written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LettersSummary {
    pub folder: String,
    pub letters: usize,
    pub files: Vec<String>,
    pub by_language: Vec<LanguageCount>,
    /// Letters that used English because the agency has no translation for the family's language.
    pub english_fallback: usize,
    /// Template placeholders the agency template used that the tool does not fill.
    pub unknown_placeholders: Vec<String>,
}

/// Result of `track_school_screenings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrackScreeningsResult {
    pub file: String,
    pub school_year: String,
    pub evaluated_on: String,
    pub students_read: usize,
    pub students_evaluated: usize,
    pub rows_with_problems: usize,
    pub screenings: Vec<String>,
    pub rows: Vec<ScreeningRow>,
    pub bmi: Vec<BmiRow>,
    pub output_file: String,
    pub letters: Option<LettersSummary>,
    /// Columns ignored for privacy (SBIRT results must never be identifiable).
    pub ignored_columns: Vec<String>,
    pub suppression_threshold: u32,
    pub notes: Vec<String>,
}

// ------------------------------------------------------------------ parsing

/// Result recorded in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recorded {
    Pass,
    Refer,
    Incomplete,
    Exempt,
}

fn parse_result(s: &str) -> Option<Recorded> {
    let t = phn_core::text::normalize(s);
    if t.is_empty() {
        return None;
    }
    let has = |w: &[&str]| w.iter().any(|x| t == *x || t.starts_with(&format!("{x} ")));
    if has(&[
        "pass",
        "passed",
        "p",
        "normal",
        "wnl",
        "ok",
        "on file",
        "received",
        "yes",
        "y",
        "complete",
        "completed",
        "measured",
        "done",
    ]) {
        Some(Recorded::Pass)
    } else if has(&[
        "refer",
        "referred",
        "referral",
        "fail",
        "failed",
        "f",
        "r",
        "abnormal",
        "did not pass",
        "positive",
    ]) {
        Some(Recorded::Refer)
    } else if has(&[
        "exempt",
        "religious",
        "opt out",
        "opted out",
        "optout",
        "declined by parent",
        "waived",
    ]) {
        Some(Recorded::Exempt)
    } else if has(&[
        "absent",
        "refused",
        "rescreen",
        "retest",
        "incomplete",
        "unable",
        "not done",
        "could not test",
        "no",
    ]) {
        Some(Recorded::Incomplete)
    } else {
        None
    }
}

/// Column positions for one screening.
#[derive(Debug, Clone, Default)]
pub(crate) struct ScreeningCols {
    pub date: Option<usize>,
    pub result: Option<usize>,
    pub follow_up: Option<usize>,
    pub exempt: Option<usize>,
    // BMI only
    pub height: Option<usize>,
    pub weight: Option<usize>,
    pub percentile: Option<usize>,
    pub category: Option<usize>,
    pub metric_height: bool,
    pub metric_weight: bool,
}

impl ScreeningCols {
    fn used(&self) -> Vec<usize> {
        [
            self.date,
            self.result,
            self.follow_up,
            self.exempt,
            self.height,
            self.weight,
            self.percentile,
            self.category,
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn any(&self) -> bool {
        !self.used().is_empty()
    }
}

fn norm(h: &str) -> String {
    phn_core::text::normalize(h)
}

/// Which screening a header belongs to.
fn header_screening(n: &str) -> Option<&'static str> {
    if n.contains("sbirt") || n.contains("crafft") || n.contains("substance") {
        return Some("sbirt");
    }
    if (n.contains("vision")
        && (n.contains("cert") || n.contains("kindergarten") || n.starts_with("k ")))
        || n.starts_with("kvc")
    {
        return Some("kindergarten_vision_certificate");
    }
    if n.contains("vision") || n.contains("visual") || n.contains("eye exam") {
        return Some("vision");
    }
    if n.contains("hearing") || n.contains("audio") {
        return Some("hearing");
    }
    if n.contains("postur")
        || n.contains("scoliosis")
        || n.contains("spine")
        || n.contains("spinal")
    {
        return Some("postural");
    }
    if n.contains("bmi") || n.contains("height") || n.contains("weight") {
        return Some("height_weight_bmi");
    }
    None
}

/// Find screening columns in a header row.
pub(crate) fn screening_columns(
    headers: &[String],
) -> (BTreeMap<&'static str, ScreeningCols>, Vec<usize>) {
    let mut out: BTreeMap<&'static str, ScreeningCols> = BTreeMap::new();
    let mut sbirt = Vec::new();
    for (i, h) in headers.iter().enumerate() {
        let n = norm(h);
        let Some(id) = header_screening(&n) else {
            continue;
        };
        if id == "sbirt" {
            sbirt.push(i);
            continue;
        }
        let c = out.entry(id).or_default();
        if id == "height_weight_bmi" {
            if n.contains("height") {
                c.height = Some(i);
                c.metric_height = n.contains("cm") || n.contains("centimet");
                continue;
            }
            if n.contains("weight") && !n.contains("category") && !n.contains("status") {
                c.weight = Some(i);
                c.metric_weight = n.contains("kg") || n.contains("kilo");
                continue;
            }
            if n.contains("percentile") || n.contains("pctl") {
                c.percentile = Some(i);
                continue;
            }
            if n.contains("category") || n.contains("class") || n.contains("status") {
                c.category = Some(i);
                continue;
            }
        }
        if n.contains("follow")
            || n.contains("complet")
            || n.contains("returned")
            || n.contains("exam date")
        {
            c.follow_up = Some(i);
        } else if n.contains("exempt") || n.contains("opt") || n.contains("waiv") {
            c.exempt = Some(i);
        } else if n.contains("date") || n.contains("measured on") || n.contains("screened on") {
            c.date = Some(i);
        } else if c.result.is_none() {
            c.result = Some(i);
        }
    }
    out.retain(|_, c| c.any());
    (out, sbirt)
}

pub(crate) fn find_col(headers: &[String], names: &[&str]) -> Option<usize> {
    headers.iter().position(|h| {
        let n = norm(h);
        names.iter().any(|x| n == *x)
    })
}

fn find_col_containing(headers: &[String], names: &[&str]) -> Option<usize> {
    find_col(headers, names).or_else(|| {
        headers.iter().position(|h| {
            let n = norm(h);
            names.iter().any(|x| n.contains(x))
        })
    })
}

/// Standard columns of a student roster.
#[derive(Debug, Clone, Default)]
pub(crate) struct RosterCols {
    pub grade: usize,
    pub dob: Option<usize>,
    pub sex: Option<usize>,
    pub language: Option<usize>,
    pub new_student: Option<usize>,
    pub school: Option<usize>,
    pub student_name: Option<usize>,
    pub first_name: Option<usize>,
    pub last_name: Option<usize>,
    pub parent: Option<usize>,
}

pub(crate) fn roster_columns(headers: &[String]) -> Result<RosterCols> {
    let grade =
        find_col(headers, &["grade", "grade level", "gr", "current grade"]).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "the file needs a 'Grade' column (PK, K, 1-12, or ungraded). Columns found: {}",
                headers.join(", ")
            ))
        })?;
    Ok(RosterCols {
        grade,
        dob: find_col(
            headers,
            &[
                "date of birth",
                "dob",
                "birth date",
                "birthdate",
                "date birth",
            ],
        ),
        sex: find_col(headers, &["sex", "gender", "sex at birth"]),
        language: find_col_containing(
            headers,
            &[
                "home language",
                "primary language",
                "family language",
                "preferred language",
                "language",
            ],
        ),
        new_student: find_col_containing(
            headers,
            &["new student", "new entrant", "new to district", "new"],
        ),
        school: find_col(headers, &["school", "school name", "building"]),
        student_name: find_col(headers, &["student name", "student", "name", "full name"]),
        first_name: find_col(
            headers,
            &["first name", "first", "given name", "student first name"],
        ),
        last_name: find_col(
            headers,
            &[
                "last name",
                "last",
                "surname",
                "family name",
                "student last name",
            ],
        ),
        parent: find_col_containing(
            headers,
            &[
                "parent name",
                "guardian name",
                "parent guardian",
                "parent",
                "guardian",
            ],
        ),
    })
}

/// Columns that identify a student (copied to the output file only).
pub(crate) fn identifier_columns(lt: &LocalTable, skip: &BTreeSet<usize>) -> Vec<usize> {
    lt.table
        .headers
        .iter()
        .enumerate()
        .filter(|(i, h)| {
            !skip.contains(i)
                && (lt
                    .findings
                    .iter()
                    .any(|f| &f.column == *h && f.kind.is_direct())
                    || {
                        let n = norm(h);
                        n.contains("name")
                            || n.contains("student id")
                            || n == "id"
                            || n.contains("sasid")
                    })
        })
        .map(|(i, _)| i)
        .collect()
}

// ------------------------------------------------------------------ evaluation

/// One student's evaluation for one screening.
#[derive(Debug, Clone)]
pub(crate) struct Evaluated {
    pub screening: &'static str,
    pub status: ScreenStatus,
    pub date: Option<NaiveDate>,
    pub detail: String,
    pub bmi: Option<bmi::BmiResult>,
    pub bmi_category: Option<BmiCategory>,
}

/// A student row.
#[derive(Debug, Clone)]
pub(crate) struct StudentEval {
    pub row: usize,
    pub grade: Grade,
    pub sex: Option<Sex>,
    pub language: String,
    pub results: Vec<Evaluated>,
}

/// The analysis of a whole roster.
pub(crate) struct RosterAnalysis {
    pub lt: LocalTable,
    pub cols: RosterCols,
    pub screening_cols: BTreeMap<&'static str, ScreeningCols>,
    pub sbirt_cols: Vec<usize>,
    pub id_cols: Vec<usize>,
    pub students: Vec<StudentEval>,
    pub problems: Vec<(usize, String)>,
    pub year: SchoolYear,
    pub as_of: NaiveDate,
    pub undated_assumed_current: usize,
    pub tracked: Vec<&'static Screening>,
}

fn window_start(
    sc: &Screening,
    grade: Grade,
    age: Option<i32>,
    year: SchoolYear,
) -> Option<(NaiveDate, Frequency)> {
    let band = band_for(sc, grade, age)?;
    let start = match band.frequency {
        Frequency::Annual => year.start(),
        Frequency::OnceInBand => {
            let first = match grade {
                Grade::Ungraded => {
                    let lo = band.ages.map(|a| a[0] as i32).unwrap_or(0);
                    return Some((year.minus(age.unwrap_or(lo) - lo).start(), band.frequency));
                }
                _ => band
                    .grades
                    .iter()
                    .filter_map(|g| Grade::parse(g).ok())
                    .map(Grade::ordinal)
                    .min()
                    .unwrap_or(grade.ordinal()),
            };
            year.minus(grade.ordinal() - first).start()
        }
        Frequency::SchoolEntry => NaiveDate::from_ymd_opt(1900, 1, 1).expect("date"),
        _ => return None,
    };
    Some((start, band.frequency))
}

fn cell(row: &[String], i: Option<usize>) -> &str {
    i.and_then(|i| row.get(i))
        .map(String::as_str)
        .unwrap_or("")
        .trim()
}

fn truthy_or_date(s: &str) -> bool {
    !s.is_empty()
        && (localdata::truthy(s) == Some(true)
            || crate::vax::dates::parse_date(s).is_some()
            || matches!(parse_result(s), Some(Recorded::Pass)))
}

fn number(s: &str) -> Option<f64> {
    let t: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

#[allow(clippy::too_many_arguments)]
fn evaluate_one(
    sc: &'static Screening,
    c: &ScreeningCols,
    row: &[String],
    grade: Grade,
    age: Option<i32>,
    new_student: bool,
    dob: Option<NaiveDate>,
    sex: Option<Sex>,
    year: SchoolYear,
    as_of: NaiveDate,
    undated: &mut usize,
) -> Evaluated {
    let mut e = Evaluated {
        screening: sc.id.as_str(),
        status: ScreenStatus::NotRequired,
        date: None,
        detail: String::new(),
        bmi: None,
        bmi_category: None,
    };
    let mut window = window_start(sc, grade, age, year);
    if window.is_none() && new_student && sc.new_students.is_some() {
        window = Some((year.start(), Frequency::Annual));
    }
    let date = crate::vax::dates::parse_date(cell(row, c.date));
    e.date = date;
    let exempt = c.exempt.is_some_and(|i| truthy_or_date(cell(row, Some(i))));
    let mut recorded = parse_result(cell(row, c.result));
    // BMI: a measurement, percentile, or category means measured.
    if sc.id == "height_weight_bmi" {
        let cat = BmiCategory::parse(cell(row, c.category));
        let pct = number(cell(row, c.percentile)).filter(|p| (0.0..=100.0).contains(p));
        let h = number(cell(row, c.height));
        let w = number(cell(row, c.weight));
        let when = date.unwrap_or(as_of);
        let computed = match (h, w, dob, sex) {
            (Some(h), Some(w), Some(d), Some(s)) => bmi::evaluate(
                s,
                bmi::age_months(d, when),
                h,
                w,
                c.metric_height || c.metric_weight,
            ),
            _ => None,
        };
        e.bmi = computed;
        e.bmi_category = cat
            .or(pct.map(BmiCategory::from_percentile))
            .or(computed.map(|r| r.category));
        if (h.is_some() && w.is_some()) || cat.is_some() || pct.is_some() {
            if !matches!(recorded, Some(Recorded::Exempt)) {
                recorded = Some(Recorded::Pass);
            }
        } else if matches!(recorded, Some(Recorded::Refer)) {
            recorded = Some(Recorded::Pass);
        }
    }
    let Some((start, _freq)) = window else {
        e.status = ScreenStatus::NotRequired;
        if recorded.is_some() {
            e.detail = "not a required grade this year (result recorded)".into();
        }
        return e;
    };
    if exempt || matches!(recorded, Some(Recorded::Exempt)) {
        e.status = ScreenStatus::Exempt;
        return e;
    }
    let in_window = match date {
        Some(d) => d >= start && d <= as_of.max(year.end()),
        None => {
            if recorded.is_some() {
                *undated += 1;
            }
            true
        }
    };
    if !in_window {
        e.status = ScreenStatus::Due;
        e.detail = format!(
            "last recorded {} is before the window starting {start}",
            date.map(|d| d.to_string()).unwrap_or_default()
        );
        return e;
    }
    e.status = match recorded {
        Some(Recorded::Pass) => ScreenStatus::Passed,
        Some(Recorded::Refer) => {
            if c.follow_up
                .is_some_and(|i| truthy_or_date(cell(row, Some(i))))
            {
                ScreenStatus::ReferralCompleted
            } else {
                ScreenStatus::ReferredPending
            }
        }
        Some(Recorded::Incomplete) => ScreenStatus::Incomplete,
        Some(Recorded::Exempt) => ScreenStatus::Exempt,
        None => ScreenStatus::Due,
    };
    e
}

/// Read and evaluate a roster.
pub(crate) fn analyze(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    as_of: NaiveDate,
    wanted: Option<&[String]>,
) -> Result<RosterAnalysis> {
    let lt = localdata::load(ctx, path, sheet)?;
    let headers = lt.table.headers.clone();
    let cols = roster_columns(&headers)?;
    let (mut screening_cols, sbirt_cols) = screening_columns(&headers);
    if let Some(w) = wanted
        && !w.is_empty()
    {
        let mut keep = BTreeSet::new();
        for q in w {
            let found = crate::school::screening_requirements::find_screening(q);
            let Some(s) = found.first() else {
                return Err(PhnError::InvalidInput(format!(
                    "screening '{q}' is not recognized; use vision, hearing, bmi, postural, or kindergarten vision certificate"
                )));
            };
            if !s.trackable {
                return Err(PhnError::InvalidInput(format!(
                    "{} cannot be tracked per student{}",
                    s.name,
                    if s.id == "sbirt" {
                        ": SBIRT responses may not be recorded in any form that identifies the student (M.G.L. c. 71, section 97(c))"
                    } else {
                        ""
                    }
                )));
            }
            keep.insert(s.id.as_str());
        }
        screening_cols.retain(|k, _| keep.contains(k));
    }
    if screening_cols.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "no screening columns were recognized. Name them like 'Vision Date', 'Vision Result', \
             'Vision Referral Completed', 'Hearing Result', 'Postural Result', 'Height (in)', \
             'Weight (lb)', or 'BMI Category'. Columns found: {}",
            headers.join(", ")
        )));
    }
    let tracked: Vec<&'static Screening> = SCREENINGS
        .screenings
        .iter()
        .filter(|s| screening_cols.contains_key(s.id.as_str()))
        .collect();
    let mut skip: BTreeSet<usize> = screening_cols.values().flat_map(|c| c.used()).collect();
    skip.extend(sbirt_cols.iter().copied());
    skip.insert(cols.grade);
    for i in [
        cols.dob,
        cols.sex,
        cols.language,
        cols.new_student,
        cols.school,
    ]
    .into_iter()
    .flatten()
    {
        skip.insert(i);
    }
    let id_cols: Vec<usize> = identifier_columns(&lt, &skip)
        .into_iter()
        .filter(|i| !sbirt_cols.contains(i))
        .collect();
    let year = SchoolYear::of(as_of);
    let mut students = Vec::new();
    let mut problems = Vec::new();
    let mut undated = 0usize;
    for (ri, row) in lt.table.rows.iter().enumerate() {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let grade = match Grade::parse(cell(row, Some(cols.grade))) {
            Ok(g) => g,
            Err(_) => {
                problems.push((ri, "grade is missing or not recognized".to_string()));
                continue;
            }
        };
        let dob = crate::vax::dates::parse_date(cell(row, cols.dob));
        let age = dob.map(|d| age_years(d, as_of));
        if grade == Grade::Ungraded && age.is_none() {
            problems.push((ri, "ungraded student without a date of birth".to_string()));
            continue;
        }
        let sex = cols.sex.and_then(|i| Sex::parse(cell(row, Some(i))));
        let new_student = cols
            .new_student
            .is_some_and(|i| localdata::truthy(cell(row, Some(i))) == Some(true));
        let language = cols
            .language
            .map(|i| cell(row, Some(i)).to_string())
            .unwrap_or_default();
        let mut results = Vec::new();
        for sc in &tracked {
            let c = &screening_cols[sc.id.as_str()];
            results.push(evaluate_one(
                sc,
                c,
                row,
                grade,
                age,
                new_student,
                dob,
                sex,
                year,
                as_of,
                &mut undated,
            ));
        }
        students.push(StudentEval {
            row: ri,
            grade,
            sex,
            language,
            results,
        });
    }
    Ok(RosterAnalysis {
        lt,
        cols,
        screening_cols,
        sbirt_cols,
        id_cols,
        students,
        problems,
        year,
        as_of,
        undated_assumed_current: undated,
        tracked,
    })
}

// ------------------------------------------------------------------ aggregation

#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    required: u64,
    passed: u64,
    referred_pending: u64,
    referral_completed: u64,
    incomplete: u64,
    exempt: u64,
    due: u64,
}

impl Tally {
    fn add(&mut self, s: ScreenStatus) {
        match s {
            ScreenStatus::NotRequired => return,
            ScreenStatus::Passed => self.passed += 1,
            ScreenStatus::ReferredPending => self.referred_pending += 1,
            ScreenStatus::ReferralCompleted => self.referral_completed += 1,
            ScreenStatus::Incomplete => self.incomplete += 1,
            ScreenStatus::Exempt => self.exempt += 1,
            ScreenStatus::Due => self.due += 1,
        }
        self.required += 1;
    }

    fn screened(&self) -> u64 {
        self.passed + self.referred_pending + self.referral_completed
    }

    fn referred(&self) -> u64 {
        self.referred_pending + self.referral_completed
    }

    /// [required, screened, referred, completed, pending, incomplete, exempt, due]
    fn cells(&self) -> Vec<Option<u64>> {
        [
            self.required,
            self.screened(),
            self.referred(),
            self.referral_completed,
            self.referred_pending,
            self.incomplete,
            self.exempt,
            self.due,
        ]
        .into_iter()
        .map(Some)
        .collect()
    }
}

fn pct(num: u64, den: u64) -> String {
    if den == 0 {
        "—".into()
    } else {
        format!("{:.1}", 100.0 * num as f64 / den as f64)
    }
}

fn grade_sort_key(g: &str) -> i32 {
    Grade::parse(g).map(Grade::ordinal).unwrap_or(100)
}

/// Suppressed rows by grade (plus an "All grades" row) for each screening.
pub(crate) fn screening_rows(a: &RosterAnalysis, threshold: u32) -> (Vec<ScreeningRow>, bool) {
    let mut out = Vec::new();
    let mut any = false;
    for sc in &a.tracked {
        let mut by_grade: BTreeMap<String, Tally> = BTreeMap::new();
        let mut total = Tally::default();
        for st in &a.students {
            if let Some(r) = st.results.iter().find(|r| r.screening == sc.id) {
                if r.status == ScreenStatus::NotRequired {
                    continue;
                }
                by_grade.entry(st.grade.label()).or_default().add(r.status);
                total.add(r.status);
            }
        }
        if total.required == 0 {
            continue;
        }
        let mut grades: Vec<String> = by_grade.keys().cloned().collect();
        grades.sort_by_key(|g| grade_sort_key(g));
        let mut labels = grades.clone();
        labels.push("All grades".into());
        let mut tallies: Vec<Tally> = grades.iter().map(|g| by_grade[g]).collect();
        tallies.push(total);
        let mut matrix: Vec<Vec<Option<u64>>> = tallies.iter().map(|t| t.cells()).collect();
        let sup = suppress(
            &matrix,
            &SuppressionOptions {
                threshold,
                last_row_is_total: labels.len() > 1,
                ..SuppressionOptions::default()
            },
        );
        any |= sup.primary + sup.complementary > 0;
        for (ri, t) in tallies.iter().enumerate() {
            let shown = |ci: usize| -> String {
                if sup.is_hidden(ri, ci) {
                    "*".into()
                } else {
                    matrix[ri][ci].unwrap_or(0).to_string()
                }
            };
            let hidden = |ci: usize| sup.is_hidden(ri, ci);
            let den_completion = t.required - t.exempt;
            out.push(ScreeningRow {
                screening: sc.name.clone(),
                grade: labels[ri].clone(),
                required: shown(0),
                screened: shown(1),
                referred: shown(2),
                referrals_completed: shown(3),
                follow_up_pending: shown(4),
                incomplete: shown(5),
                exempt: shown(6),
                due: shown(7),
                completion_pct: if hidden(1) || hidden(0) || hidden(6) {
                    "*".into()
                } else {
                    pct(t.screened(), den_completion)
                },
                referral_pct: if hidden(2) || hidden(1) {
                    "*".into()
                } else {
                    pct(t.referred(), t.screened())
                },
                follow_up_pct: if hidden(3) || hidden(2) {
                    "*".into()
                } else {
                    pct(t.referral_completed, t.referred())
                },
            });
        }
        matrix.clear();
    }
    (out, any)
}

/// Suppressed BMI category counts by grade and sex.
pub(crate) fn bmi_rows(a: &RosterAnalysis, threshold: u32) -> (Vec<BmiRow>, bool, usize) {
    let mut counts: BTreeMap<(String, String), [u64; 4]> = BTreeMap::new();
    let mut without_category = 0usize;
    for st in &a.students {
        let Some(r) = st
            .results
            .iter()
            .find(|r| r.screening == "height_weight_bmi" && r.status == ScreenStatus::Passed)
        else {
            continue;
        };
        let Some(cat) = r.bmi_category else {
            without_category += 1;
            continue;
        };
        let sex = st
            .sex
            .map(|s| s.label().to_string())
            .unwrap_or_else(|| "not recorded".into());
        let e = counts.entry((st.grade.label(), sex)).or_default();
        let idx = BmiCategory::ALL.iter().position(|c| *c == cat).unwrap_or(0);
        e[idx] += 1;
    }
    if counts.is_empty() {
        return (Vec::new(), false, without_category);
    }
    let mut keys: Vec<(String, String)> = counts.keys().cloned().collect();
    keys.sort_by_key(|(g, s)| (grade_sort_key(g), s.clone()));
    let matrix: Vec<Vec<Option<u64>>> = keys
        .iter()
        .map(|k| {
            let c = counts[k];
            let mut v: Vec<Option<u64>> = vec![Some(c.iter().sum())];
            v.extend(c.iter().map(|x| Some(*x)));
            v
        })
        .collect();
    let sup = suppress(
        &matrix,
        &SuppressionOptions {
            threshold,
            ..SuppressionOptions::default()
        },
    );
    let any = sup.primary + sup.complementary > 0;
    let rows = keys
        .iter()
        .enumerate()
        .map(|(ri, (g, s))| {
            let show = |ci: usize| {
                if sup.is_hidden(ri, ci) {
                    "*".to_string()
                } else {
                    matrix[ri][ci].unwrap_or(0).to_string()
                }
            };
            BmiRow {
                grade: g.clone(),
                sex: s.clone(),
                measured: show(0),
                categories: BmiCategory::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.label().to_string(), show(i + 1)))
                    .collect(),
            }
        })
        .collect();
    (rows, any, without_category)
}

// ------------------------------------------------------------------ letters

fn letter_kind(id: &str) -> Option<&'static str> {
    match id {
        "vision" => Some("vision_referral"),
        "hearing" => Some("hearing_referral"),
        "postural" => Some("postural_referral"),
        _ => None,
    }
}

/// The bundled English letter for a screening referral.
pub fn english_letter(kind: &str) -> String {
    match kind {
        "vision_referral" => VISION_LETTER.to_string(),
        "hearing_referral" => HEARING_LETTER.to_string(),
        _ => POSTURAL_LETTER.to_string(),
    }
}

/// Placeholders the letters use.
pub const LETTER_PLACEHOLDERS: &[&str] = &[
    "student_name",
    "parent_name",
    "grade",
    "school",
    "screening",
    "screening_date",
    "nurse_name",
    "nurse_phone",
    "today",
];

const VISION_LETTER: &str = "# Vision screening: follow-up needed

{{today}}

Dear {{parent_name}},

{{student_name}} (grade {{grade}}) had a vision screening at {{school}} on {{screening_date}} and did not pass. A screening is not a diagnosis. It means your child should have a complete eye examination by an eye doctor (an ophthalmologist or optometrist).

Please:

1. Make an appointment for a comprehensive eye examination.
2. Ask the eye doctor to send a written report to the school nurse. Massachusetts law (M.G.L. c. 71, section 57) asks the examiner to send the school a report of the findings and any recommended classroom adjustments, such as glasses or preferential seating.
3. Call me if you need help finding an eye doctor or paying for the visit.

Thank you,

{{nurse_name}}, School Nurse
{{nurse_phone}}
";

const HEARING_LETTER: &str = "# Hearing screening: follow-up needed

{{today}}

Dear {{parent_name}},

{{student_name}} (grade {{grade}}) had a hearing screening at {{school}} on {{screening_date}} and did not pass. A screening is not a diagnosis. It shows that your child's hearing should be checked by your child's health care provider or an audiologist.

Please:

1. Make an appointment with your child's health care provider.
2. Share the results with the school nurse so we can support your child in class.
3. Call me if you need help finding a provider.

Thank you,

{{nurse_name}}, School Nurse
{{nurse_phone}}
";

const POSTURAL_LETTER: &str = "# Postural screening: follow-up recommended

{{today}}

Dear {{parent_name}},

{{student_name}} (grade {{grade}}) took part in the postural screening at {{school}} on {{screening_date}}. The screening found a sign that should be checked by a physician. This screening is not a diagnosis; many findings turn out to need only observation.

Please:

1. Make an appointment with your family physician, pediatrician, or an orthopedist.
2. Let me know which doctor you choose so I can send them the screening findings.
3. Call me with any questions.

Thank you,

{{nurse_name}}, School Nurse
{{nurse_phone}}
";

fn student_name(a: &RosterAnalysis, row: &[String]) -> String {
    if let Some(i) = a.cols.student_name {
        return cell(row, Some(i)).to_string();
    }
    let f = cell(row, a.cols.first_name);
    let l = cell(row, a.cols.last_name);
    format!("{f} {l}").trim().to_string()
}

fn write_letters(
    ctx: &Context,
    a: &RosterAnalysis,
    args: &TrackScreeningsArgs,
    format: DocFormat,
) -> Result<LettersSummary> {
    if a.cols.student_name.is_none() && a.cols.first_name.is_none() && a.cols.last_name.is_none() {
        return Err(PhnError::InvalidInput(
            "letters need a 'Student Name' column (or 'First Name' and 'Last Name') in the file"
                .into(),
        ));
    }
    let (dir, shown) =
        templates::output_folder(ctx, &format!("school_screening_letters_{}", a.as_of))?;
    // Copy the English templates so the agency can translate them.
    let mut files = Vec::new();
    let mut by_lang: BTreeMap<(String, bool), usize> = BTreeMap::new();
    let mut fallback = 0usize;
    let mut unknown: BTreeSet<String> = BTreeSet::new();
    let mut total = 0usize;
    for sc in &a.tracked {
        let Some(kind) = letter_kind(&sc.id) else {
            continue;
        };
        let mut resolver = TemplateResolver::new(ctx, kind, english_letter(kind));
        // language label -> (tag, translated, documents)
        let mut groups: BTreeMap<String, (String, bool, Vec<Document>)> = BTreeMap::new();
        for st in &a.students {
            let Some(r) = st.results.iter().find(|r| r.screening == sc.id) else {
                continue;
            };
            if r.status != ScreenStatus::ReferredPending {
                continue;
            }
            let row = &a.lt.table.rows[st.row];
            let chosen = resolver.choose(&st.language);
            if !chosen.translated && !templates::is_english(&st.language) {
                fallback += 1;
            }
            let mut v = BTreeMap::new();
            v.insert("student_name".to_string(), student_name(a, row));
            v.insert(
                "parent_name".to_string(),
                a.cols
                    .parent
                    .map(|i| cell(row, Some(i)).to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Parent or Guardian".into()),
            );
            v.insert("grade".to_string(), st.grade.label());
            v.insert(
                "school".to_string(),
                a.cols
                    .school
                    .map(|i| cell(row, Some(i)).to_string())
                    .filter(|s| !s.is_empty())
                    .or_else(|| args.school_name.clone())
                    .unwrap_or_else(|| "school".into()),
            );
            v.insert("screening".to_string(), sc.name.clone());
            v.insert(
                "screening_date".to_string(),
                r.date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "this school year".into()),
            );
            v.insert(
                "nurse_name".to_string(),
                args.nurse_name.clone().unwrap_or_default(),
            );
            v.insert(
                "nurse_phone".to_string(),
                args.nurse_phone.clone().unwrap_or_default(),
            );
            v.insert("today".to_string(), a.as_of.to_string());
            let (filled, unk) = templates::fill(&chosen.markdown, &v, "________");
            unknown.extend(unk);
            let doc = templates::document(ctx, &filled, &chosen.tag, &sc.name);
            let g = groups
                .entry(chosen.language.clone())
                .or_insert_with(|| (chosen.tag.clone(), chosen.translated, Vec::new()));
            g.2.push(doc);
            total += 1;
        }
        for (lang, (tag, translated, docs)) in groups {
            let n = docs.len();
            *by_lang.entry((lang.clone(), translated)).or_default() += n;
            let combined = templates::combine(&format!("{} letters", sc.name), &tag, docs, ctx);
            let bytes = format.render(&combined)?;
            let file = format!(
                "{kind}_{}.{}",
                lang.to_lowercase().replace(' ', "_"),
                format.ext()
            );
            files.push(templates::write_in(ctx, &dir, &file, &bytes)?);
        }
        // English template for translators (no student data).
        let t = templates::write_in(
            ctx,
            &dir,
            &format!("template_english_{kind}.md"),
            english_letter(kind).as_bytes(),
        )?;
        files.push(t);
    }
    Ok(LettersSummary {
        folder: shown,
        letters: total,
        files,
        by_language: by_lang
            .into_iter()
            .map(
                |((language, agency_translation), documents)| LanguageCount {
                    language,
                    documents,
                    agency_translation,
                },
            )
            .collect(),
        english_fallback: fallback,
        unknown_placeholders: unknown.into_iter().collect(),
    })
}

// ------------------------------------------------------------------ output file

fn write_workbook(ctx: &Context, a: &RosterAnalysis) -> Result<String> {
    let headers = &a.lt.table.headers;
    let id_headers: Vec<String> = a.id_cols.iter().map(|&i| headers[i].clone()).collect();
    let mut action = Vec::new();
    let mut all = Vec::new();
    let mut bmi_detail = Vec::new();
    for st in &a.students {
        let row = &a.lt.table.rows[st.row];
        let ids: Vec<String> = a
            .id_cols
            .iter()
            .map(|&i| cell(row, Some(i)).to_string())
            .collect();
        let mut line = vec![(st.row + 2).to_string()];
        line.extend(ids.clone());
        line.push(st.grade.label());
        line.push(templates::language_label(&st.language));
        for r in &st.results {
            line.push(r.status.label().to_string());
            if matches!(
                r.status,
                ScreenStatus::Due | ScreenStatus::ReferredPending | ScreenStatus::Incomplete
            ) {
                let mut x = vec![(st.row + 2).to_string()];
                x.extend(ids.clone());
                x.push(st.grade.label());
                x.push(templates::language_label(&st.language));
                x.push(
                    SCREENINGS
                        .get(r.screening)
                        .map(|s| s.name.clone())
                        .unwrap_or_default(),
                );
                x.push(r.status.label().to_string());
                x.push(r.date.map(|d| d.to_string()).unwrap_or_default());
                x.push(
                    match r.status {
                        ScreenStatus::Due => "screen this year",
                        ScreenStatus::ReferredPending => {
                            "follow up on the referral; record the exam result"
                        }
                        _ => "rescreen or reschedule",
                    }
                    .to_string(),
                );
                x.push(r.detail.clone());
                action.push(x);
            }
            if let Some(b) = r.bmi {
                let mut x = vec![(st.row + 2).to_string()];
                x.extend(ids.clone());
                x.push(st.grade.label());
                x.push(st.sex.map(|s| s.label().to_string()).unwrap_or_default());
                x.push(format!("{:.1}", b.bmi));
                x.push(if b.percentile >= 95.0 {
                    "95th or above".into()
                } else {
                    format!("{:.0}", b.percentile)
                });
                x.push(b.category.label().to_string());
                bmi_detail.push(x);
            }
        }
        all.push(line);
    }
    let mut h_action = vec!["Row".to_string()];
    h_action.extend(id_headers.clone());
    h_action.extend(
        [
            "Grade",
            "Language",
            "Screening",
            "Status",
            "Last date",
            "Next step",
            "Note",
        ]
        .map(String::from),
    );
    let mut h_all = vec!["Row".to_string()];
    h_all.extend(id_headers.clone());
    h_all.extend(["Grade".to_string(), "Language".to_string()]);
    h_all.extend(a.tracked.iter().map(|s| s.name.clone()));
    let mut blocks = vec![
        Block::Paragraph(vec![Inline::plain(format!(
            "School health screenings, {} school year, evaluated on {} under 105 CMR 200.000. \
             Contains student information: keep confidential (FERPA). BMI results may be shared \
             only with the parent or guardian without written permission (105 CMR 200.500(A)).",
            a.year.label(),
            a.as_of
        ))]),
        Block::Table(TableBlock {
            caption: Some("Action list".into()),
            headers: h_action,
            rows: action,
        }),
        Block::Table(TableBlock {
            caption: Some("All students".into()),
            headers: h_all,
            rows: all,
        }),
    ];
    if !bmi_detail.is_empty() {
        let mut h = vec!["Row".to_string()];
        h.extend(id_headers.clone());
        h.extend(["Grade", "Sex", "BMI", "Percentile", "Category"].map(String::from));
        blocks.push(Block::Table(TableBlock {
            caption: Some("BMI (confidential)".into()),
            headers: h,
            rows: bmi_detail,
        }));
    }
    if !a.problems.is_empty() {
        let mut h = vec!["Row".to_string()];
        h.extend(id_headers);
        h.push("Problem".into());
        let rows = a
            .problems
            .iter()
            .map(|(ri, msg)| {
                let row = &a.lt.table.rows[*ri];
                let mut x = vec![(ri + 2).to_string()];
                x.extend(a.id_cols.iter().map(|&i| cell(row, Some(i)).to_string()));
                x.push(msg.clone());
                x
            })
            .collect();
        blocks.push(Block::Table(TableBlock {
            caption: Some("Rows not evaluated".into()),
            headers: h,
            rows,
        }));
    }
    let doc = Document {
        title: "School health screenings".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    };
    let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
    let sandbox = ctx.sandbox()?;
    let out = sandbox.new_output_file(&format!("school_screenings_{}.xlsx", a.as_of))?;
    std::fs::write(&out, bytes)?;
    Ok(sandbox.display_path(&out))
}

// ------------------------------------------------------------------ tool

/// Track required school screenings from a local roster.
pub fn track_school_screenings(
    ctx: &Context,
    args: TrackScreeningsArgs,
) -> Result<Envelope<TrackScreeningsResult>> {
    if args.path.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "give the roster file (CSV or XLSX in /data)".into(),
        ));
    }
    let as_of = match &args.as_of {
        Some(s) => crate::vax::dates::require_date("as_of", s)?,
        None => chrono::Local::now().date_naive(),
    };
    let a = analyze(
        ctx,
        &args.path,
        args.sheet.as_deref(),
        as_of,
        args.screenings.as_deref(),
    )?;
    let threshold = ctx.suppression_threshold();
    let (rows, sup1) = screening_rows(&a, threshold);
    let (bmi, sup2, bmi_without_category) = bmi_rows(&a, threshold);
    let output_file = write_workbook(ctx, &a)?;
    let mut files_written = vec![output_file.clone()];
    let letters = if args.write_letters.unwrap_or(false) {
        let l = write_letters(ctx, &a, &args, args.letter_format.unwrap_or_default())?;
        files_written.extend(l.files.iter().cloned());
        Some(l)
    } else {
        None
    };
    let ignored_columns: Vec<String> = a
        .sbirt_cols
        .iter()
        .map(|&i| a.lt.table.headers[i].clone())
        .collect();

    // ---- Markdown
    let mut s = format!(
        "**School health screenings — {} school year** (evaluated {as_of})\n\n{} of {} student row(s) evaluated{}. Per-student results and an action list were written to `{output_file}` (not shown here).\n\n",
        a.year.label(),
        a.students.len(),
        a.students.len() + a.problems.len(),
        if a.problems.is_empty() {
            String::new()
        } else {
            format!(
                "; {} row(s) could not be evaluated (listed in the output file)",
                a.problems.len()
            )
        },
    );
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.screening.clone(),
                r.grade.clone(),
                r.required.clone(),
                r.screened.clone(),
                r.completion_pct.clone(),
                r.referred.clone(),
                r.referral_pct.clone(),
                r.referrals_completed.clone(),
                r.follow_up_pct.clone(),
                r.due.clone(),
                r.exempt.clone(),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Screening",
            "Grade",
            "Required",
            "Screened",
            "Completion %",
            "Referred",
            "Referral %",
            "Follow-up done",
            "Follow-up %",
            "Due",
            "Exempt",
        ],
        &table_rows,
    ));
    if !bmi.is_empty() {
        s.push_str("\n**BMI categories (for DPH's BMI report)**\n\n");
        let mut headers = vec!["Grade", "Sex", "Measured"];
        headers.extend(BmiCategory::ALL.iter().map(|c| c.label()));
        let br: Vec<Vec<String>> = bmi
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
        s.push_str(&md::table(&headers, &br));
    }
    if sup1 || sup2 {
        s.push_str(&format!(
            "\n`*` = suppressed (counts from 1 to {} and cells that would reveal them; rates are hidden when a count they use is hidden).\n",
            threshold - 1
        ));
    }
    if let Some(l) = &letters {
        s.push_str(&format!(
            "\n**Referral letters:** {} letter(s) written to `{}`",
            l.letters, l.folder
        ));
        if !l.by_language.is_empty() {
            s.push_str(&format!(
                " ({})",
                l.by_language
                    .iter()
                    .map(|x| format!(
                        "{} {}{}",
                        x.documents,
                        x.language,
                        if x.agency_translation {
                            " (agency translation)"
                        } else {
                            ""
                        }
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        s.push_str(".\n");
        if l.english_fallback > 0 {
            s.push_str(&format!(
                "{} letter(s) are in English because your agency has no translation for the family's language. {}\n",
                l.english_fallback,
                templates::translation_help(&["vision_referral", "hearing_referral", "postural_referral"])
            ));
        }
    }

    let mut notes = Vec::new();
    if !ignored_columns.is_empty() {
        notes.push(format!(
            "Column(s) {} look like SBIRT results and were ignored and not copied: M.G.L. c. 71, section 97(c) forbids any record of SBIRT responses that identifies the student. Remove them from this file.",
            ignored_columns.join(", ")
        ));
    }
    if a.undated_assumed_current > 0 {
        notes.push(format!(
            "{} result(s) had no date and were counted as this school year's; add dates for once-per-band screenings.",
            a.undated_assumed_current
        ));
    }
    if bmi_without_category > 0 {
        notes.push(format!(
            "{bmi_without_category} BMI measurement(s) have no category: give BMI Category or Percentile, or height, weight, date of birth, and sex so the tool can compute it."
        ));
    }
    if a.cols.language.is_none() && letters.is_some() {
        notes.push("No home language column was found, so every letter is in English.".into());
    }
    for n in &notes {
        s.push_str(&format!("\n{n}\n"));
    }

    let mut caveats = vec![
        "Once-in-a-band screenings (vision and hearing in grades 6-8 and 9-12) count a screening dated since the student entered the band; students who transferred in may need records from their prior school.".to_string(),
        "Completion counts screened students over those required minus exempt; referral follow-up counts a referral as complete when the follow-up column has a date or yes (DPH: an appointment made and kept).".to_string(),
        "The school year runs July 1 to June 30 in this tool.".to_string(),
    ];
    caveats.extend(content::review(&[content::SCREENING, content::BMI]));
    let mut sources = vec![Source::local_file(a.lt.label.clone())];
    sources.extend(content::sources_for(
        &SCREENINGS.sources,
        ["cmr200", "mgl-c71-s57", "dph-vision-2022"],
    ));
    if !bmi.is_empty() {
        sources.extend(content::sources_for(
            &content::BMI_REF.sources,
            ["cdc-bmiagerev"],
        ));
    }
    let identifiers = a.lt.identifier_categories();
    Ok(Envelope::new(
        s,
        TrackScreeningsResult {
            file: a.lt.label.clone(),
            school_year: a.year.label(),
            evaluated_on: as_of.to_string(),
            students_read: a.lt.table.rows.len(),
            students_evaluated: a.students.len(),
            rows_with_problems: a.problems.len(),
            screenings: a.tracked.iter().map(|s| s.id.clone()).collect(),
            rows,
            bmi,
            output_file,
            letters,
            ignored_columns,
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
        identifiers_detected: identifiers,
        files_written,
        notes: vec![
            "Only counts and rates by grade are returned; names, results, and letters are in the output folder.".into(),
        ],
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(file: &str, body: &str) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data").join(file), body).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    const ROSTER: &str = "Student ID,Last Name,First Name,Grade,Date of Birth,Sex,Home Language,Vision Date,Vision Result,Vision Referral Completed,Hearing Result,Height (in),Weight (lb)\n\
9100001,Zorvath,Quill,K,2021-03-01,F,Spanish,2026-09-20,refer,,pass,,\n\
9100002,Brembly,Oro,K,2021-05-01,M,English,2026-09-20,pass,,pass,,\n\
9100003,Quenby,Iska,1,2020-04-01,F,English,2026-09-21,pass,,refer,45,48\n\
9100004,Thorne,Vale,7,2014-02-01,M,English,2025-10-01,pass,,,60,95\n\
9100005,Marsh,Pell,7,2014-03-01,F,Portuguese,2023-10-01,pass,,,61,100\n\
9100006,Wexley,Juno,4,2017-01-01,M,English,,,,,54,70\n";

    #[test]
    fn statuses_follow_grade_rules_and_windows() {
        let (_d, c) = ctx_with("r.csv", ROSTER);
        let a = analyze(
            &c,
            "r.csv",
            None,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            None,
        )
        .unwrap();
        let get = |row: usize, id: &str| {
            a.students
                .iter()
                .find(|s| s.row == row)
                .unwrap()
                .results
                .iter()
                .find(|r| r.screening == id)
                .unwrap()
                .status
        };
        assert_eq!(get(0, "vision"), ScreenStatus::ReferredPending);
        assert_eq!(get(1, "vision"), ScreenStatus::Passed);
        // Grade 1 hearing referral pending.
        assert_eq!(get(2, "hearing"), ScreenStatus::ReferredPending);
        // Grade 7 vision once in 6-8: 2025-10-01 (grade 6) counts; 2023 does not.
        assert_eq!(get(3, "vision"), ScreenStatus::Passed);
        assert_eq!(get(4, "vision"), ScreenStatus::Due);
        // Grade 7 hearing once in 6-8 with no result: due.
        assert_eq!(get(3, "hearing"), ScreenStatus::Due);
        // Grade 4: vision due (annual through grade 5), hearing not required.
        assert_eq!(get(5, "vision"), ScreenStatus::Due);
        assert_eq!(get(5, "hearing"), ScreenStatus::NotRequired);
        // BMI grade 4 measured with computed category.
        let b = a.students[5]
            .results
            .iter()
            .find(|r| r.screening == "height_weight_bmi")
            .unwrap();
        assert_eq!(b.status, ScreenStatus::Passed);
        assert_eq!(b.bmi_category, Some(BmiCategory::HealthyWeight));
        // Grade 7 BMI with no date counts as measured this year.
        assert_eq!(get(3, "height_weight_bmi"), ScreenStatus::Passed);
    }

    #[test]
    fn tool_returns_suppressed_counts_and_writes_letters() {
        let (d, c) = ctx_with("r.csv", ROSTER);
        let e = track_school_screenings(
            &c,
            TrackScreeningsArgs {
                path: "r.csv".into(),
                as_of: Some("2026-11-01".into()),
                write_letters: Some(true),
                school_name: Some("Elm School".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let all = serde_json::to_string(&e).unwrap();
        for name in ["Zorvath", "Quill", "Brembly", "9100001", "2021-03-01"] {
            assert!(!all.contains(name), "{name} leaked");
        }
        assert!(e.data.rows.iter().any(|r| r.grade == "All grades"));
        let l = e.data.letters.as_ref().unwrap();
        assert_eq!(l.letters, 2);
        assert_eq!(
            l.english_fallback, 1,
            "Spanish family without agency translation"
        );
        assert!(d.path().join("data").join(&e.data.output_file).exists());
        assert!(e.privacy.suppression_applied && !e.privacy.row_level_data);
    }

    #[test]
    fn agency_translation_is_used_when_present() {
        let (d, c) = ctx_with("r.csv", ROSTER);
        let t = d.path().join("data/templates/spanish");
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(
            t.join("vision_referral.md"),
            "# Evaluación de la vista\n\nEstimada familia de {{student_name}} ({{grade}}).\n",
        )
        .unwrap();
        let e = track_school_screenings(
            &c,
            TrackScreeningsArgs {
                path: "r.csv".into(),
                as_of: Some("2026-11-01".into()),
                write_letters: Some(true),
                screenings: Some(vec!["vision".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        let l = e.data.letters.unwrap();
        assert_eq!(l.english_fallback, 0);
        assert!(
            l.by_language
                .iter()
                .any(|x| x.language == "Spanish" && x.agency_translation)
        );
    }

    #[test]
    fn sbirt_columns_are_refused_and_ignored() {
        let body = "Name,Grade,Vision Result,SBIRT Result\nQuill Zorvath,7,pass,positive\n";
        let (_d, c) = ctx_with("s.csv", body);
        let e = track_school_screenings(
            &c,
            TrackScreeningsArgs {
                path: "s.csv".into(),
                as_of: Some("2026-11-01".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(e.data.ignored_columns, vec!["SBIRT Result".to_string()]);
        let err = track_school_screenings(
            &c,
            TrackScreeningsArgs {
                path: "s.csv".into(),
                screenings: Some(vec!["sbirt".into()]),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("97(c)"));
    }
}
