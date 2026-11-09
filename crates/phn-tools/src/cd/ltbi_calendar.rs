//! CD-12 `ltbi_treatment_calendar`: a dose calendar, monitoring visits, the
//! completion window, and an adherence check for a latent TB infection
//! regimen (3HP, 4R, 3HR, 6H, 9H, twice-weekly 6H/9H), from the 2020
//! NTCA/CDC guidelines, the 2000 ATS/CDC completion criteria, and DPH's
//! regimen page. Writes an adherence tracker (XLSX) and a patient calendar
//! (DOCX) to `/data/output`. It never calculates doses (plan item CD-16).

use chrono::{Datelike, Months, NaiveDate, Weekday};
use phn_core::{Envelope, PhnError, Privacy, Result};
use rust_xlsxwriter::{Format, Formula, Workbook, XlsxError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::tb_content::{self, Administration, Frequency, Regimen, RegimenId, TbContent};
use crate::context::Context;
use crate::core::practice::{self, CitedNote, Confidence};
use crate::md;

/// Arguments for `ltbi_treatment_calendar`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct LtbiCalendarArgs {
    /// Regimen: 3HP, 4R, 3HR, 6H, 9H, 6H_twice_weekly, or 9H_twice_weekly
    /// (names such as "rifampin" or "isoniazid rifapentine" also work).
    pub regimen: String,
    /// First dose date, YYYY-MM-DD.
    pub start_date: String,
    /// dot, vdot, or sat (default: DPH's mode where DPH lists the regimen,
    /// DOT for twice-weekly regimens, otherwise self-administered).
    pub administration: Option<Administration>,
    /// Dosing weekdays for twice-weekly regimens (default ["monday", "thursday"]).
    pub twice_weekly_days: Vec<String>,
    /// Dates doses were taken, YYYY-MM-DD, for the adherence check (dates only).
    pub doses_taken: Vec<String>,
    /// Evaluate adherence as of this date (default today).
    pub as_of: Option<String>,
    /// Write the adherence tracker (XLSX) and patient calendar (DOCX) to
    /// /data/output (default true; skipped with a note when no data folder is mounted).
    pub write_files: Option<bool>,
    /// Language for the patient calendar's instructions. English is built in;
    /// other languages use your agency's translation in
    /// /data/templates/<language>/ltbi_calendar.md when it exists.
    pub language: Option<String>,
}

/// A monitoring visit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Visit {
    pub date: String,
    /// Treatment week (1 = first week).
    pub week: i64,
    pub purpose: String,
}

/// The completion rule with its deadline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CompletionRule {
    pub doses_required: u32,
    /// Last day to complete the required doses (null when no window is published).
    pub deadline: Option<String>,
    /// Doses closer together than this do not count (3HP: more than 72 hours apart).
    pub min_days_between_doses: Option<i64>,
    pub text: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// Adherence status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdherenceStatus {
    /// The required doses are done within the window.
    Complete,
    OnTrack,
    /// Behind schedule but the window can still be met.
    BehindSchedule,
    /// The required doses can no longer fit in the window.
    CannotCompleteInWindow,
    /// No window is published for this regimen; doses are counted only.
    NoWindow,
}

/// Adherence check from the dose dates given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Adherence {
    pub as_of: String,
    pub doses_counted: u32,
    /// Doses not counted (before the start, or too soon after the previous dose).
    pub doses_not_counted: u32,
    pub doses_remaining: u32,
    /// Doses scheduled on or before `as_of`.
    pub doses_expected_by_now: u32,
    pub last_dose: Option<String>,
    /// Longest gap between doses, or since the last dose, in days.
    pub longest_gap_days: i64,
    /// A gap of more than 2 months: rule out TB disease before resuming.
    pub interruption_over_2_months: bool,
    /// Finish date if every remaining dose is taken on schedule from `as_of`.
    pub projected_completion: Option<String>,
    pub status: AdherenceStatus,
    pub notes: Vec<String>,
}

/// Which language the patient calendar uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PatientLanguage {
    pub requested: String,
    pub used: String,
    /// The agency translation used (path relative to /data), if any.
    pub agency_translation: Option<String>,
}

/// Result of `ltbi_treatment_calendar`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LtbiCalendar {
    pub regimen: RegimenId,
    pub regimen_name: String,
    pub drugs: Vec<String>,
    pub administration: Administration,
    /// "preferred" or "alternative" (CDC/NTCA 2020).
    pub cdc_rank: String,
    /// Whether DPH's Massachusetts regimen page lists this regimen.
    pub dph_listed: bool,
    pub dph_administration: Option<String>,
    pub start_date: String,
    pub total_doses: u32,
    /// Date of the last scheduled dose.
    pub scheduled_end: String,
    pub completion: CompletionRule,
    /// Every scheduled dose date (weekly and twice-weekly regimens only; daily
    /// regimens are one dose per day from start to scheduled_end).
    pub dose_dates: Vec<String>,
    pub monitoring_visits: Vec<Visit>,
    pub adherence: Option<Adherence>,
    pub recommended_for: Vec<String>,
    pub not_recommended_for: Vec<String>,
    pub before_starting: Vec<CitedNote>,
    pub monitoring: Vec<CitedNote>,
    pub stop_and_call: CitedNote,
    pub education: Vec<CitedNote>,
    pub adherence_support: Vec<CitedNote>,
    pub interruption: CitedNote,
    pub post_treatment: CitedNote,
    /// Always states that doses are not calculated.
    pub dosing: String,
    pub patient_language: PatientLanguage,
    /// Files written (paths relative to /data).
    pub files: Vec<String>,
}

const DOSING_NOTE: &str = "This tool does not calculate medication doses (weight-based dosing \
is plan item CD-16, not built). Use the prescriber's order and the CDC regimen table or DPH's \
regimen page.";

/// Parse a regimen name.
fn parse_regimen(s: &str) -> Result<RegimenId> {
    let n = phn_core::text::normalize(s).replace(['-', '_'], " ");
    let twice = n.contains("twice") || n.contains("biweekly") || n.contains("2x");
    let id = match n.replace(' ', "").as_str() {
        "3hp"
        | "isoniazidrifapentine"
        | "inhrpt"
        | "rifapentineisoniazid"
        | "isoniazidandrifapentine" => RegimenId::R3HP,
        "4r" | "rifampin" | "rif" | "4rif" | "rifampicin" => RegimenId::R4R,
        "3hr" | "isoniazidrifampin" | "inhrif" | "isoniazidandrifampin" => RegimenId::R3HR,
        "6h" | "6inh" if !twice => RegimenId::R6H,
        "9h" | "9inh" | "isoniazid" | "inh" if !twice => RegimenId::R9H,
        _ if n.starts_with("6h") || (n.contains("6 month") && n.contains("isoniazid")) => {
            if twice {
                RegimenId::R6HTwiceWeekly
            } else {
                RegimenId::R6H
            }
        }
        _ if n.starts_with("9h") || (n.contains("9 month") && n.contains("isoniazid")) => {
            if twice {
                RegimenId::R9HTwiceWeekly
            } else {
                RegimenId::R9H
            }
        }
        _ => {
            return Err(PhnError::InvalidInput(format!(
                "unknown regimen '{s}'; use 3HP, 4R, 3HR, 6H, 9H, 6H_twice_weekly, or \
                 9H_twice_weekly"
            )));
        }
    };
    Ok(id)
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    let l = s.trim().to_ascii_lowercase();
    let k: String = l.chars().take(3).collect();
    Some(match k.as_str() {
        "mon" => Weekday::Mon,
        "tue" => Weekday::Tue,
        "wed" => Weekday::Wed,
        "thu" => Weekday::Thu,
        "fri" => Weekday::Fri,
        "sat" => Weekday::Sat,
        "sun" => Weekday::Sun,
        _ => return None,
    })
}

/// Scheduled dose dates.
fn schedule(r: &Regimen, start: NaiveDate, days: &[Weekday]) -> Vec<NaiveDate> {
    let n = r.total_doses as usize;
    match r.frequency {
        Frequency::Daily => (0..n)
            .map(|i| start + chrono::Duration::days(i as i64))
            .collect(),
        Frequency::OnceWeekly => (0..n)
            .map(|i| start + chrono::Duration::weeks(i as i64))
            .collect(),
        Frequency::TwiceWeekly => {
            let mut out = Vec::with_capacity(n);
            let mut d = start;
            while out.len() < n {
                if days.contains(&d.weekday()) {
                    out.push(d);
                }
                d += chrono::Duration::days(1);
            }
            out
        }
    }
}

fn deadline(r: &Regimen, start: NaiveDate) -> Option<NaiveDate> {
    // "Within N months/days" of the first dose: the last day that still counts.
    match (r.completion.within_days, r.completion.within_months) {
        (Some(d), _) => Some(start + chrono::Duration::days(d - 1)),
        (None, Some(m)) => start
            .checked_add_months(Months::new(m))
            .map(|d| d - chrono::Duration::days(1)),
        _ => None,
    }
}

fn visits(r: &Regimen, start: NaiveDate, end: NaiveDate) -> Vec<Visit> {
    let mut out = Vec::new();
    if let Some(weeks) = &r.monitoring_visit_weeks {
        for w in weeks {
            let d = start + chrono::Duration::weeks(i64::from(*w) - 1);
            out.push(Visit {
                date: d.to_string(),
                week: i64::from(*w),
                purpose: if *w == 1 {
                    "Start: clinical evaluation, baseline labs if indicated, education".into()
                } else {
                    "Medical evaluation and monitoring (DPH schedule)".into()
                },
            });
        }
    } else {
        let mut m = 0u32;
        loop {
            let Some(d) = start.checked_add_months(Months::new(m)) else {
                break;
            };
            if d > end {
                break;
            }
            out.push(Visit {
                date: d.to_string(),
                week: (d - start).num_days() / 7 + 1,
                purpose: if m == 0 {
                    "Start: clinical evaluation, baseline labs if indicated, education".into()
                } else {
                    "Monthly assessment: adherence, TB symptoms, adverse reactions".into()
                },
            });
            m += 1;
        }
    }
    out.push(Visit {
        date: end.to_string(),
        week: (end - start).num_days() / 7 + 1,
        purpose: "Last scheduled dose: confirm completion; give the patient documentation of \
                  tests, chest radiograph, medications, and treatment duration"
            .into(),
    });
    out
}

#[allow(clippy::too_many_arguments)]
fn adherence(
    r: &Regimen,
    start: NaiveDate,
    sched: &[NaiveDate],
    taken: &[NaiveDate],
    as_of: NaiveDate,
    deadline: Option<NaiveDate>,
    interruption_days: i64,
    days: &[Weekday],
) -> Adherence {
    let mut taken: Vec<NaiveDate> = taken.to_vec();
    taken.sort();
    taken.dedup();
    let mut counted: Vec<NaiveDate> = Vec::new();
    let mut not_counted = 0u32;
    let mut notes = Vec::new();
    let min_gap = r.completion.min_days_between_doses;
    for d in taken {
        if d < start {
            not_counted += 1;
            continue;
        }
        if let (Some(g), Some(prev)) = (min_gap, counted.last())
            && (d - *prev).num_days() < g
        {
            not_counted += 1;
            continue;
        }
        if r.frequency == Frequency::Daily && counted.last() == Some(&d) {
            continue;
        }
        counted.push(d);
    }
    if not_counted > 0 {
        notes.push(format!(
            "{not_counted} dose date(s) were not counted (before the start date{}).",
            if min_gap.is_some() {
                ", or less than 72 hours after the previous dose"
            } else {
                ""
            }
        ));
    }
    let required = r.completion.doses_required;
    let n = counted.len() as u32;
    let remaining = required.saturating_sub(n);
    let expected = sched.iter().filter(|d| **d <= as_of).count() as u32;
    let mut longest_gap = 0i64;
    let mut prev = None::<NaiveDate>;
    for d in &counted {
        if let Some(p) = prev {
            longest_gap = longest_gap.max((*d - p).num_days());
        }
        prev = Some(*d);
    }
    let since_last = counted
        .last()
        .map(|l| (as_of - *l).num_days())
        .unwrap_or((as_of - start).num_days());
    if remaining > 0 {
        longest_gap = longest_gap.max(since_last);
    }
    let interruption = longest_gap > interruption_days;
    if interruption {
        notes.push(
            "There is a gap of more than 2 months: a medical examination to rule out TB disease \
             is indicated before treatment resumes; continue or restart the regimen as the \
             clinician decides."
                .into(),
        );
    }
    // Projected completion: remaining doses on schedule from as_of.
    let projected = if remaining == 0 {
        counted.get(required as usize - 1).copied()
    } else {
        let next_ok = counted
            .last()
            .map(|l| *l + chrono::Duration::days(min_gap.unwrap_or(1).max(1)))
            .unwrap_or(start)
            .max(as_of);
        let k = i64::from(remaining) - 1;
        Some(match r.frequency {
            Frequency::Daily => next_ok + chrono::Duration::days(k),
            Frequency::OnceWeekly => next_ok + chrono::Duration::weeks(k),
            Frequency::TwiceWeekly => {
                let mut d = next_ok;
                let mut c = 0;
                loop {
                    if days.contains(&d.weekday()) {
                        c += 1;
                        if c == remaining {
                            break d;
                        }
                    }
                    d += chrono::Duration::days(1);
                }
            }
        })
    };
    let status = match (remaining, deadline, projected) {
        (0, Some(dl), Some(p)) if p <= dl => AdherenceStatus::Complete,
        (0, None, _) => AdherenceStatus::Complete,
        (0, Some(_), _) => {
            notes.push(
                "The required number of doses was reached only after the completion window \
                 closed; discuss with the clinician whether treatment counts as complete."
                    .into(),
            );
            AdherenceStatus::CannotCompleteInWindow
        }
        (_, None, _) => AdherenceStatus::NoWindow,
        (_, Some(dl), Some(p)) if p > dl => AdherenceStatus::CannotCompleteInWindow,
        _ if n + 1 < expected.min(required) => AdherenceStatus::BehindSchedule,
        _ => AdherenceStatus::OnTrack,
    };
    if status == AdherenceStatus::CannotCompleteInWindow && remaining > 0 {
        notes.push(
            "The remaining doses cannot be taken within the completion window: the clinician \
             decides whether to extend, restart, or change the regimen."
                .into(),
        );
    }
    Adherence {
        as_of: as_of.to_string(),
        doses_counted: n,
        doses_not_counted: not_counted,
        doses_remaining: remaining,
        doses_expected_by_now: expected,
        last_dose: counted.last().map(|d| d.to_string()),
        longest_gap_days: longest_gap,
        interruption_over_2_months: interruption,
        projected_completion: projected.map(|d| d.to_string()),
        status,
        notes,
    }
}

fn xerr(e: XlsxError) -> PhnError {
    PhnError::Internal(format!("XLSX: {e}"))
}

fn to_xl(d: NaiveDate) -> std::result::Result<rust_xlsxwriter::ExcelDateTime, XlsxError> {
    rust_xlsxwriter::ExcelDateTime::from_ymd(d.year() as u16, d.month() as u8, d.day() as u8)
}

struct TrackerSpec<'a> {
    c: &'a TbContent,
    r: &'a Regimen,
    admin: Administration,
    sched: &'a [NaiveDate],
    visits: &'a [Visit],
    deadline: Option<NaiveDate>,
    taken: &'a [NaiveDate],
}

fn build_tracker(t: &TrackerSpec<'_>) -> std::result::Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let head = Format::new().set_bold().set_text_wrap();
    let bold = Format::new().set_bold();
    let date = Format::new().set_num_format("yyyy-mm-dd");
    let wrap = Format::new().set_text_wrap();
    let ws = wb.add_worksheet();
    ws.set_name("Doses")?;
    ws.set_header(format!(
        "&LLTBI adherence tracker: {}&RConfidential",
        t.r.id.code()
    ));
    ws.write_string_with_format(0, 0, "Patient name / ID (fill in locally)", &bold)?;
    ws.write_string(0, 3, "")?;
    let headers = [
        "Dose #",
        "Scheduled date",
        "Weekday",
        "Taken (Y/N)",
        "Date taken",
        "Observed by (DOT/vDOT/self)",
        "Side-effect check done (Y/N)",
        "Symptoms reported",
        "Initials",
        "Notes",
    ];
    for (i, h) in headers.iter().enumerate() {
        ws.write_string_with_format(2, i as u16, *h, &head)?;
        ws.set_column_width(i as u16, if i >= 7 { 24 } else { 14 })?;
    }
    for (i, d) in t.sched.iter().enumerate() {
        let r = 3 + i as u32;
        ws.write_number(r, 0, (i + 1) as f64)?;
        ws.write_date_with_format(r, 1, &to_xl(*d)?, &date)?;
        ws.write_string(r, 2, d.format("%a").to_string())?;
        if t.taken.contains(d) {
            ws.write_string(r, 3, "Y")?;
            ws.write_date_with_format(r, 4, &to_xl(*d)?, &date)?;
        }
    }
    let last = 3 + t.sched.len() as u32;
    let sr = last + 1;
    ws.write_string_with_format(sr, 0, "Doses taken", &bold)?;
    ws.write_formula(sr, 3, Formula::new(format!("=COUNTIF(D4:D{last},\"Y\")")))?;
    ws.write_string_with_format(sr + 1, 0, "Doses required to complete", &bold)?;
    ws.write_number(sr + 1, 3, f64::from(t.r.completion.doses_required))?;
    ws.write_string_with_format(sr + 2, 0, "Doses remaining", &bold)?;
    ws.write_formula(
        sr + 2,
        3,
        Formula::new(format!("=MAX(0,D{}-D{})", sr + 2, sr + 1)),
    )?;
    ws.write_string_with_format(sr + 3, 0, "Complete by (window)", &bold)?;
    match t.deadline {
        Some(d) => {
            ws.write_date_with_format(sr + 3, 3, &to_xl(d)?, &date)?;
        }
        None => {
            ws.write_string(
                sr + 3,
                3,
                "no published window: agree one with the TB program",
            )?;
        }
    }
    ws.set_freeze_panes(3, 0)?;

    let ws = wb.add_worksheet();
    ws.set_name("Visits")?;
    for (i, h) in ["Date", "Week", "Purpose", "Done (date)", "Findings"]
        .iter()
        .enumerate()
    {
        ws.write_string_with_format(0, i as u16, *h, &head)?;
    }
    ws.set_column_width(0, 12)?;
    ws.set_column_width(2, 70)?;
    ws.set_column_width(4, 40)?;
    for (i, v) in t.visits.iter().enumerate() {
        let r = 1 + i as u32;
        if let Ok(d) = NaiveDate::parse_from_str(&v.date, "%Y-%m-%d") {
            ws.write_date_with_format(r, 0, &to_xl(d)?, &date)?;
        }
        ws.write_number(r, 1, v.week as f64)?;
        ws.write_string_with_format(r, 2, &v.purpose, &wrap)?;
    }

    let ws = wb.add_worksheet();
    ws.set_name("Regimen and rules")?;
    ws.set_column_width(0, 30)?;
    ws.set_column_width(1, 100)?;
    let mut rows: Vec<(String, String)> = vec![
        (
            "Regimen".into(),
            format!("{} ({})", t.r.name, t.r.id.code()),
        ),
        ("Administration".into(), t.admin.label().into()),
        ("Total doses scheduled".into(), t.r.total_doses.to_string()),
        (
            "Completion".into(),
            format!(
                "{} Source: {}",
                t.r.completion.text,
                practice::cite(
                    &t.c.sources,
                    &t.r.completion.source,
                    &t.r.completion.section
                )
            ),
        ),
        ("Interruptions".into(), t.c.ltbi.interruption.text.clone()),
        ("Stop and call".into(), t.c.ltbi.stop_and_call.text.clone()),
        ("Dosing".into(), DOSING_NOTE.into()),
    ];
    for m in &t.r.monitoring {
        rows.push(("Monitoring".into(), m.text.clone()));
    }
    for (i, (k, v)) in rows.iter().enumerate() {
        ws.write_string_with_format(i as u32, 0, k, &bold)?;
        ws.write_string_with_format(i as u32, 1, v, &wrap)?;
    }
    wb.save_to_buffer()
}

/// Month-by-month calendar tables for the patient.
fn month_tables(sched: &[NaiveDate], visits: &[Visit]) -> Vec<phn_docs::model::Block> {
    let mut out = Vec::new();
    let (Some(first), Some(last)) = (sched.first(), sched.last()) else {
        return out;
    };
    let visit_dates: Vec<NaiveDate> = visits
        .iter()
        .filter_map(|v| NaiveDate::parse_from_str(&v.date, "%Y-%m-%d").ok())
        .collect();
    let mut month = NaiveDate::from_ymd_opt(first.year(), first.month(), 1).expect("valid date");
    while month <= *last {
        let next = month
            .checked_add_months(Months::new(1))
            .expect("valid month");
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut week = vec![String::new(); 7];
        let mut d = month;
        while d < next {
            let col = d.weekday().num_days_from_sunday() as usize;
            let mut cell = d.day().to_string();
            if sched.contains(&d) {
                cell.push_str(" ☐ dose");
            }
            if visit_dates.contains(&d) {
                cell.push_str(" (visit)");
            }
            week[col] = cell;
            if col == 6 {
                rows.push(std::mem::replace(&mut week, vec![String::new(); 7]));
            }
            d += chrono::Duration::days(1);
        }
        if week.iter().any(|c| !c.is_empty()) {
            rows.push(week);
        }
        out.push(practice::table(
            &month.format("%B %Y").to_string(),
            &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
            rows,
        ));
        month = next;
    }
    out
}

fn english_instructions(c: &TbContent, r: &Regimen, deadline: Option<NaiveDate>) -> Vec<String> {
    let mut v = vec![
        format!(
            "Take your TB medicine on each day marked \"dose\" and check the box. You need {} \
             doses{}.",
            r.completion.doses_required,
            deadline.map(|d| format!(" by {d}")).unwrap_or_default()
        ),
        "Keep your appointments (marked \"visit\"). Bring this calendar.".into(),
    ];
    if r.drugs.iter().any(|d| d.starts_with("rif")) {
        v.push(
            "Rifampin and rifapentine can turn urine, sweat, and tears orange. This is harmless. \
             Soft contact lenses can be stained."
                .into(),
        );
    }
    v.push(c.ltbi.stop_and_call.text.clone());
    v
}

/// Build an LTBI treatment calendar and adherence check.
pub fn ltbi_treatment_calendar(
    ctx: &Context,
    args: LtbiCalendarArgs,
) -> Result<Envelope<LtbiCalendar>> {
    let c = tb_content::content();
    let id = parse_regimen(&args.regimen)?;
    let r = c
        .ltbi
        .regimens
        .iter()
        .find(|x| x.id == id)
        .ok_or_else(|| PhnError::Internal(format!("regimen {id:?} missing")))?;
    let start = practice::date_arg("start_date", &Some(args.start_date.clone()))?
        .ok_or_else(|| PhnError::InvalidInput("start_date is required (YYYY-MM-DD)".into()))?;
    let as_of = practice::date_arg("as_of", &args.as_of)?
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    let days: Vec<Weekday> = if args.twice_weekly_days.is_empty() {
        vec![Weekday::Mon, Weekday::Thu]
    } else {
        args.twice_weekly_days
            .iter()
            .map(|d| {
                parse_weekday(d)
                    .ok_or_else(|| PhnError::InvalidInput(format!("'{d}' is not a weekday name")))
            })
            .collect::<Result<_>>()?
    };
    if r.frequency == Frequency::TwiceWeekly && (days.len() != 2 || days[0] == days[1]) {
        return Err(PhnError::InvalidInput(
            "twice_weekly_days must name two different weekdays".into(),
        ));
    }
    let mut caveats: Vec<String> = practice::review_caveat(&c.meta).into_iter().collect();
    let admin = args.administration.unwrap_or(match r.frequency {
        Frequency::TwiceWeekly => Administration::Dot,
        _ if r
            .dph_administration
            .as_deref()
            .is_some_and(|a| a.contains("DOT")) =>
        {
            Administration::Dot
        }
        _ => Administration::Sat,
    });
    if !r.allowed_administration.contains(&admin) {
        caveats.push(format!(
            "{} is not an option for {}: twice-weekly regimens must be directly observed.",
            admin.label(),
            r.id.code()
        ));
    }
    if r.id == RegimenId::R3HP && admin == Administration::Sat {
        caveats.push(
            "DPH recommends that all 3HP doses be directly observed; CDC (2018) allows \
             self-administration for people aged 2 and older with clinical monitoring."
                .into(),
        );
    }
    if !r.dph_listed {
        caveats.push(format!(
            "DPH's Massachusetts regimen page lists 3HP, 4R, and 9H; {} is a CDC/NTCA 2020 \
             regimen that DPH's page does not list. Confirm with the DPH TB program.",
            r.id.code()
        ));
    }
    let sched = schedule(r, start, &days);
    let end = *sched.last().expect("at least one dose");
    let dl = deadline(r, start);
    if dl.is_none() {
        caveats.push(format!(
            "No completion window is published for {} in the sources checked; agree one with \
             the TB program.",
            r.id.code()
        ));
    }
    if r.completion.confidence != Confidence::High {
        caveats.push(format!(
            "The completion rule for {} is {} confidence: {}",
            r.id.code(),
            match r.completion.confidence {
                Confidence::Low => "LOW",
                _ => "medium",
            },
            r.completion.text
        ));
    }
    let visits = visits(r, start, end);

    let taken: Vec<NaiveDate> = args
        .doses_taken
        .iter()
        .enumerate()
        .map(|(i, d)| {
            practice::date_arg(&format!("doses_taken[{}]", i + 1), &Some(d.clone()))?
                .ok_or_else(|| PhnError::InvalidInput(format!("doses_taken[{}] is empty", i + 1)))
        })
        .collect::<Result<_>>()?;
    if taken.iter().any(|d| *d > as_of) {
        return Err(PhnError::InvalidInput(
            "doses_taken includes dates after as_of".into(),
        ));
    }
    let adh = (!taken.is_empty()).then(|| {
        adherence(
            r,
            start,
            &sched,
            &taken,
            as_of,
            dl,
            c.ltbi.interruption.interruption_days,
            &days,
        )
    });

    // ---- Patient language
    let requested = args.language.clone().unwrap_or_else(|| "English".into());
    let translation = args.language.as_deref().and_then(|l| {
        practice::language_folder(l)
            .and_then(|_| practice::agency_template(ctx, l, "ltbi_calendar.md"))
    });
    let english_needed = practice::language_folder(&requested).is_some() && translation.is_none();
    if english_needed {
        caveats.push(format!(
            "No agency translation was found for '{requested}' (expected \
             templates/{}/ltbi_calendar.md in the data folder), so the patient calendar is in \
             English. Review it with the patient through a qualified interpreter; do not use \
             machine translation.",
            practice::language_folder(&requested).unwrap_or_default()
        ));
    }

    // ---- Files
    let mut files = Vec::new();
    if args.write_files.unwrap_or(true) {
        match ctx.sandbox() {
            Ok(sb) => {
                let bytes = build_tracker(&TrackerSpec {
                    c,
                    r,
                    admin,
                    sched: &sched,
                    visits: &visits,
                    deadline: dl,
                    taken: &taken,
                })
                .map_err(xerr)?;
                let path = sb.new_output_file(&format!(
                    "ltbi_{}_tracker_{start}.xlsx",
                    r.id.code().replace([' ', '(', ')'], "")
                ))?;
                std::fs::write(&path, bytes)?;
                files.push(sb.display_path(&path));

                let mut blocks = vec![
                    practice::para(
                        "Name: ______________________________    Date of birth: ______________",
                    ),
                    practice::para(format!(
                        "Medicine: {} — {} doses, {}. First dose {start}; last scheduled dose {end}.",
                        r.name,
                        r.total_doses,
                        admin.label()
                    )),
                    practice::heading(2, "Instructions"),
                ];
                match &translation {
                    Some((text, _)) => {
                        let filled = practice::fill_placeholders(
                            text,
                            &[
                                ("regimen", r.name.clone()),
                                ("start_date", start.to_string()),
                                ("doses", r.completion.doses_required.to_string()),
                                (
                                    "completion_deadline",
                                    dl.map(|d| d.to_string()).unwrap_or_default(),
                                ),
                                ("program_phone", c.ma_program.phone_business_hours.clone()),
                            ],
                        );
                        blocks.extend(phn_docs::model::from_markdown(&filled));
                    }
                    None => blocks.push(practice::bullets(english_instructions(c, r, dl))),
                }
                blocks.push(practice::heading(2, "Calendar"));
                blocks.extend(month_tables(&sched, &visits));
                let doc = practice::document(ctx, "TB medicine calendar", blocks);
                files.extend(practice::write_document(
                    ctx,
                    &doc,
                    &format!(
                        "ltbi_{}_calendar_{start}",
                        r.id.code().replace([' ', '(', ')'], "")
                    ),
                    &["docx"],
                )?);
            }
            Err(e) if args.write_files.is_none() => caveats.push(format!(
                "The tracker and calendar were not written because no data folder is mounted ({e})."
            )),
            Err(e) => return Err(e),
        }
    }

    // ---- Markdown
    let mut s = format!(
        "**LTBI treatment calendar: {} ({})**\n\n",
        r.name,
        r.id.code()
    );
    let mut rows = vec![
        vec!["Administration".into(), admin.label().to_string()],
        vec![
            "CDC/NTCA 2020".into(),
            format!(
                "{}{}",
                r.cdc_rank,
                if r.dph_listed {
                    "; listed by DPH"
                } else {
                    "; not on DPH's regimen page"
                }
            ),
        ],
        vec!["First dose".into(), start.to_string()],
        vec![
            "Doses".into(),
            format!(
                "{} scheduled; {} required to complete",
                r.total_doses, r.completion.doses_required
            ),
        ],
        vec!["Last scheduled dose".into(), end.to_string()],
        vec![
            "Complete by".into(),
            dl.map(|d| d.to_string())
                .unwrap_or_else(|| "no published window".into()),
        ],
    ];
    if let Some(g) = r.completion.min_days_between_doses {
        rows.push(vec![
            "Spacing".into(),
            format!("doses must be more than {} hours apart to count", g * 24),
        ]);
    }
    s.push_str(&md::table(&["", ""], &rows));
    s.push_str(&format!(
        "\n_Completion: {} ({})_\n",
        r.completion.text,
        practice::cite(&c.sources, &r.completion.source, &r.completion.section)
    ));
    s.push_str("\n**Monitoring visits**\n\n");
    let vr: Vec<Vec<String>> = visits
        .iter()
        .map(|v| vec![v.date.clone(), v.week.to_string(), v.purpose.clone()])
        .collect();
    s.push_str(&md::table(&["Date", "Week", "Purpose"], &vr));
    if let Some(a) = &adh {
        s.push_str(&format!(
            "\n**Adherence as of {}:** {} dose(s) counted of {} required ({} expected by now); {} \
             remaining; status **{:?}**{}.\n",
            a.as_of,
            a.doses_counted,
            r.completion.doses_required,
            a.doses_expected_by_now,
            a.doses_remaining,
            a.status,
            a.projected_completion
                .as_ref()
                .map(|p| format!("; projected completion {p}"))
                .unwrap_or_default()
        ));
        for n in &a.notes {
            s.push_str(&format!("- {n}\n"));
        }
    }
    s.push_str(&format!("\n**Dosing:** {DOSING_NOTE}\n"));
    if !files.is_empty() {
        s.push_str(&format!(
            "\n**Files:** {} (adherence tracker and patient calendar; fill in the name locally).\n",
            files.join(", ")
        ));
    }

    caveats.push(DOSING_NOTE.into());
    caveats.push(
        "Start LTBI treatment only after TB disease is excluded. Assess at least monthly for \
         adherence, TB symptoms, and adverse reactions."
            .into(),
    );
    let mut keys: Vec<&str> = r.sources.iter().map(String::as_str).collect();
    keys.extend([
        r.completion.source.as_str(),
        c.ltbi.interruption.source.as_str(),
        c.ltbi.stop_and_call.source.as_str(),
    ]);
    let sources = practice::sources_for(&c.sources, keys);
    let data = LtbiCalendar {
        regimen: r.id,
        regimen_name: r.name.clone(),
        drugs: r.drugs.clone(),
        administration: admin,
        cdc_rank: r.cdc_rank.clone(),
        dph_listed: r.dph_listed,
        dph_administration: r.dph_administration.clone(),
        start_date: start.to_string(),
        total_doses: r.total_doses,
        scheduled_end: end.to_string(),
        completion: CompletionRule {
            doses_required: r.completion.doses_required,
            deadline: dl.map(|d| d.to_string()),
            min_days_between_doses: r.completion.min_days_between_doses,
            text: r.completion.text.clone(),
            citation: practice::cite(&c.sources, &r.completion.source, &r.completion.section),
            confidence: r.completion.confidence,
        },
        dose_dates: if r.frequency == Frequency::Daily {
            Vec::new()
        } else {
            sched.iter().map(|d| d.to_string()).collect()
        },
        monitoring_visits: visits,
        adherence: adh,
        recommended_for: r.recommended_for.clone(),
        not_recommended_for: r.not_recommended_for.clone(),
        before_starting: practice::notes(&c.ltbi.before_starting, &c.sources),
        monitoring: practice::notes(&r.monitoring, &c.sources)
            .into_iter()
            .chain(practice::notes(&c.ltbi.monitoring, &c.sources))
            .collect(),
        stop_and_call: practice::note(&c.ltbi.stop_and_call, &c.sources),
        education: practice::notes(&c.ltbi.education, &c.sources),
        adherence_support: practice::notes(&c.ltbi.adherence, &c.sources),
        interruption: CitedNote {
            text: c.ltbi.interruption.text.clone(),
            citation: practice::cite(
                &c.sources,
                &c.ltbi.interruption.source,
                &c.ltbi.interruption.section,
            ),
            confidence: None,
        },
        post_treatment: practice::note(&c.ltbi.post_treatment, &c.sources),
        dosing: DOSING_NOTE.into(),
        patient_language: PatientLanguage {
            requested: requested.clone(),
            used: if translation.is_some() {
                requested.clone()
            } else {
                "English".into()
            },
            agency_translation: translation.map(|(_, p)| p),
        },
        files: files.clone(),
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            files_written: files,
            notes: vec![
                "The calendar and tracker are blank for the patient's name; fill them in \
                 locally. Only dates were used."
                    .into(),
            ],
            ..Privacy::none()
        })
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(with_data: bool) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        if with_data {
            std::fs::create_dir_all(dir.path().join("data")).unwrap();
        }
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn args(regimen: &str, start: &str) -> LtbiCalendarArgs {
        LtbiCalendarArgs {
            regimen: regimen.into(),
            start_date: start.into(),
            write_files: Some(false),
            ..LtbiCalendarArgs::default()
        }
    }

    #[test]
    fn three_hp_has_twelve_weekly_doses_and_a_16_week_window() {
        let (_d, c) = ctx(false);
        let env = ltbi_treatment_calendar(&c, args("3HP", "2026-01-05")).unwrap();
        let d = &env.data;
        assert_eq!(d.dose_dates.len(), 12);
        assert_eq!(d.scheduled_end, "2026-03-23");
        // 16 weeks = 112 days from the first dose, inclusive.
        assert_eq!(d.completion.deadline.as_deref(), Some("2026-04-26"));
        assert_eq!(d.completion.doses_required, 11);
        assert_eq!(d.administration, Administration::Dot);
        // DPH schedule: medical evaluation weeks 1, 5, 9.
        assert_eq!(d.monitoring_visits[1].date, "2026-02-02");
        assert!(env.summary.contains("does not calculate"));
    }

    #[test]
    fn four_r_window_is_six_months_and_regimen_names_parse() {
        let (_d, c) = ctx(false);
        let env = ltbi_treatment_calendar(&c, args("rifampin", "2026-01-15")).unwrap();
        assert_eq!(env.data.regimen, RegimenId::R4R);
        assert_eq!(env.data.scheduled_end, "2026-05-14");
        assert_eq!(env.data.completion.deadline.as_deref(), Some("2026-07-14"));
        assert_eq!(
            parse_regimen("9H twice weekly").unwrap(),
            RegimenId::R9HTwiceWeekly
        );
        assert_eq!(parse_regimen("6h").unwrap(), RegimenId::R6H);
        assert!(parse_regimen("4 months of magic").is_err());
        // 3HR has no published window.
        let env = ltbi_treatment_calendar(&c, args("3HR", "2026-01-15")).unwrap();
        assert!(env.data.completion.deadline.is_none());
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("No completion window"))
        );
    }

    #[test]
    fn twice_weekly_uses_the_chosen_days() {
        let (_d, c) = ctx(false);
        let mut a = args("9H_twice_weekly", "2026-01-06"); // Tuesday
        a.twice_weekly_days = vec!["tue".into(), "fri".into()];
        let env = ltbi_treatment_calendar(&c, a).unwrap();
        assert_eq!(env.data.dose_dates.len(), 76);
        assert_eq!(env.data.dose_dates[0], "2026-01-06");
        assert_eq!(env.data.dose_dates[1], "2026-01-09");
        assert_eq!(env.data.administration, Administration::Dot);
    }

    #[test]
    fn adherence_counts_spacing_gaps_and_window() {
        let (_d, c) = ctx(false);
        let mut a = args("3HP", "2026-01-05");
        // Doses weeks 1-3, a dose 2 days later (not counted), then nothing for 10 weeks.
        a.doses_taken = vec![
            "2026-01-05".into(),
            "2026-01-12".into(),
            "2026-01-19".into(),
            "2026-01-21".into(),
        ];
        a.as_of = Some("2026-03-30".into());
        let env = ltbi_treatment_calendar(&c, a).unwrap();
        let adh = env.data.adherence.unwrap();
        assert_eq!(adh.doses_counted, 3);
        assert_eq!(adh.doses_not_counted, 1);
        assert_eq!(adh.doses_remaining, 8);
        // 8 more weekly doses from Mar 30 end May 18, after the Apr 26 deadline.
        assert_eq!(adh.status, AdherenceStatus::CannotCompleteInWindow);
        assert!(adh.interruption_over_2_months);

        let mut a = args("4R", "2026-01-01");
        a.doses_taken = (0..30)
            .map(|i| {
                (NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(i))
                    .to_string()
            })
            .collect();
        a.as_of = Some("2026-01-31".into());
        let env = ltbi_treatment_calendar(&c, a).unwrap();
        let adh = env.data.adherence.unwrap();
        assert_eq!(adh.status, AdherenceStatus::OnTrack);
        assert_eq!(adh.projected_completion.as_deref(), Some("2026-04-30"));
    }

    #[test]
    fn files_are_written_and_translations_fall_back_to_english() {
        let (dir, c) = ctx(true);
        let mut a = args("4R", "2026-01-15");
        a.write_files = None;
        a.language = Some("Portuguese".into());
        let env = ltbi_treatment_calendar(&c, a).unwrap();
        assert_eq!(env.data.files.len(), 2);
        assert_eq!(env.data.patient_language.used, "English");
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("templates/portuguese/ltbi_calendar.md"))
        );
        // With an agency translation it is used.
        let tdir = dir.path().join("data/templates/portuguese");
        std::fs::create_dir_all(&tdir).unwrap();
        std::fs::write(
            tdir.join("ltbi_calendar.md"),
            "Tome {{doses}} doses de {{regimen}} a partir de {{start_date}}.",
        )
        .unwrap();
        let mut a = args("4R", "2026-01-15");
        a.write_files = None;
        a.language = Some("Portuguese".into());
        let env = ltbi_treatment_calendar(&c, a).unwrap();
        assert_eq!(env.data.patient_language.used, "Portuguese");
        assert!(env.data.patient_language.agency_translation.is_some());
    }
}
