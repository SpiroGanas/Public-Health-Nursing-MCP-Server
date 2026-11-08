//! VAX-03 `forecast_immunizations_due`: CDC CDSi evaluation and forecast.
//!
//! Two modes:
//! * **One patient** typed into the request (date of birth, sex, doses,
//!   conditions). The date of birth is used for the calculation and never
//!   repeated: the response shows ages and dose dates.
//! * **A file** in `/data` with one row per dose, keyed by a person
//!   identifier. Per-person forecasts and dose evaluations are written to
//!   `/data/output`; the response holds only suppressed aggregate counts.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use phn_cdsi::data::{
    CDSI_URL, LOGIC_SPEC_DATE, LOGIC_SPEC_URL, LOGIC_SPEC_VERSION, SUPPORTING_DATA_PUBLISHED,
    SUPPORTING_DATA_URL, SUPPORTING_DATA_VERSION, SupportingData,
};
use phn_cdsi::names::{Resolved, resolve};
use phn_cdsi::{
    DoseInput, EngineResult, EvalStatus, Gender, Observation, PatientInput, SeriesStatus,
    VaccineGroupForecast,
};
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// CDC test cases the engine passes (see docs/cdsi-test-results.md).
pub const CDSI_TEST_CASES_PASSED: usize = 1_365;
/// CDC test cases run (healthy childhood/adult v4.46 and underlying conditions v4.6).
pub const CDSI_TEST_CASES_TOTAL: usize = 1_401;

const MAX_DOSES: usize = 100;
const MAX_PERSONS: usize = 50_000;

/// A dose given.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DoseArg {
    /// CVX code (e.g. "110"), vaccine type (e.g. "DTaP-HepB-IPV"), or trade name (e.g. "Pediarix").
    pub vaccine: String,
    /// Date given, YYYY-MM-DD.
    pub date: String,
}

/// A condition or other patient observation.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ConditionArg {
    /// CDSi observation code (e.g. "007") or its title (e.g. "Pregnant", "Anatomical or functional asplenia", "Health care personnel").
    pub condition: String,
    /// Date of the observation, YYYY-MM-DD, when an interval is measured from it (e.g. onset of pregnancy).
    #[serde(default)]
    pub date: Option<String>,
}

/// Arguments for `forecast_immunizations_due`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ForecastImmunizationsArgs {
    /// Date of birth, YYYY-MM-DD (one-patient mode). Used for the calculation only; the response shows ages, not the date of birth.
    pub date_of_birth: Option<String>,
    /// Sex: "female", "male", or "unknown" (default unknown). The HPV series differ by sex.
    pub sex: Option<String>,
    /// Date to assess, YYYY-MM-DD (default today).
    pub assessment_date: Option<String>,
    /// Doses given (one-patient mode), up to 100.
    pub doses: Vec<DoseArg>,
    /// Conditions and other observations (risk indications, contraindications, evidence of immunity).
    pub conditions: Vec<ConditionArg>,
    /// Born in the United States. Some evidence of immunity (varicella, born before 1980) applies only to U.S.-born people; unknown when omitted.
    pub born_in_us: Option<bool>,
    /// Limit the answer to these vaccine groups (e.g. "DTaP", "MMR", "HPV", "pneumococcal", "flu").
    pub vaccine_groups: Vec<String>,
    /// File mode: a CSV or spreadsheet in /data with one row per dose (person ID, date of birth, vaccine, date given; sex optional).
    pub file: Option<String>,
    /// Sheet name for spreadsheets (default: first sheet).
    pub sheet: Option<String>,
    /// File mode: person identifier column (detected when omitted).
    pub id_column: Option<String>,
    /// File mode: date-of-birth column (detected when omitted).
    pub dob_column: Option<String>,
    /// File mode: date-given column (detected when omitted).
    pub date_column: Option<String>,
    /// File mode: vaccine column, CVX code or name (detected when omitted).
    pub vaccine_column: Option<String>,
    /// File mode: sex column (optional; detected when omitted).
    pub sex_column: Option<String>,
}

/// Where the patient stands for a vaccine group on the assessment date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DueStatus {
    /// Past the past-due date.
    Overdue,
    /// On or after the recommended date.
    DueNow,
    /// Can be given now (on or after the earliest date) but not yet recommended.
    CanGiveNow,
    /// Earliest date is in the future.
    DueLater,
    /// Series complete.
    Complete,
    /// Evidence of immunity.
    Immune,
    /// Contraindicated.
    Contraindicated,
    /// Too old to start or finish the series.
    AgedOut,
    /// Not recommended at this time (past immunization history or past the season's end).
    NotRecommended,
}

impl DueStatus {
    /// Plain-language label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Overdue => "Overdue",
            Self::DueNow => "Due now",
            Self::CanGiveNow => "Can give now (not yet due)",
            Self::DueLater => "Due later",
            Self::Complete => "Complete",
            Self::Immune => "Immune",
            Self::Contraindicated => "Contraindicated",
            Self::AgedOut => "Aged out",
            Self::NotRecommended => "Not recommended now",
        }
    }
}

/// The engine and supporting data used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CdsiEngineInfo {
    /// CDSi Logic Specification version implemented.
    pub logic_specification: String,
    /// CDSi Supporting Data version embedded.
    pub supporting_data: String,
    /// Supporting Data publication month.
    pub supporting_data_published: String,
    /// CDC test cases passed.
    pub test_cases_passed: usize,
    /// CDC test cases run.
    pub test_cases_total: usize,
}

/// Forecast for one vaccine group and series group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GroupForecast {
    /// Vaccine group (e.g. "DTaP/Tdap/Td", "MMR").
    pub vaccine_group: String,
    /// CDSi series group number.
    pub series_group: String,
    /// "Standard" or "Risk" series.
    pub series_type: String,
    /// Best patient series behind the forecast.
    pub series: Vec<String>,
    /// CDSi status (Not Complete, Complete, Immune, Contraindicated, Aged Out, Not Recommended).
    pub status: String,
    /// Where the patient stands on the assessment date.
    pub due_status: DueStatus,
    /// Next target dose number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_dose_number: Option<usize>,
    /// Earliest date the next dose would be valid (no grace period).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest: Option<String>,
    /// Recommended date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<String>,
    /// Past-due date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub past_due: Option<String>,
    /// Latest date (day before the maximum age).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    /// Vaccine types the supporting data recommends for the next dose, when it names them.
    pub recommended_vaccines: Vec<String>,
    /// CDSi forecast reasons.
    pub reasons: Vec<String>,
}

/// One dose's evaluation for one vaccine group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DoseEvaluationOut {
    /// Dose number in the order entered (1-based).
    pub dose: usize,
    /// Date given.
    pub date: String,
    /// Age when given.
    pub age_at_dose: String,
    /// Vaccine as entered.
    pub vaccine: String,
    /// CVX code it was resolved to.
    pub cvx: String,
    /// Vaccine group evaluated.
    pub vaccine_group: String,
    /// Valid, Not Valid, Extraneous, or Sub-standard.
    pub status: String,
    /// Target dose it counted toward, when valid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts_as_dose: Option<usize>,
    /// Reasons (too young, too soon, grace period, live virus conflict, ...).
    pub reasons: Vec<String>,
}

/// One step of a projected catch-up schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CatchUpStep {
    pub vaccine_group: String,
    /// Target dose number.
    pub dose_number: usize,
    /// Earliest date for this dose if every earlier dose in the projection
    /// is given on its own earliest date (the first is no earlier than the
    /// assessment date).
    pub earliest: String,
    /// Recommended date in the same projection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<String>,
    /// Vaccine type assumed for the projection (CVX), from the supporting
    /// data's preferable vaccines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaccine: Option<String>,
}

/// Position of a CVX code among the candidate vaccines of open series.
fn order_of(open: &[&phn_cdsi::PatientSeriesResult], cvx: &str) -> usize {
    open.iter()
        .flat_map(|p| p.candidate_vaccines.iter())
        .position(|c| c.cvx == cvx)
        .unwrap_or(usize::MAX)
}

/// Project the remaining doses of one vaccine group forecast: give the next
/// dose on its earliest date (not before the assessment date) with a
/// preferable vaccine, re-run the engine on that date, and repeat.
fn project_catch_up(
    input: &PatientInput,
    first: &EngineResult,
    f: &VaccineGroupForecast,
) -> Vec<CatchUpStep> {
    let mut steps = Vec::new();
    let mut patient = input.clone();
    let mut r = first.clone();
    let mut current = f.clone();
    let mut last_date = input.assessment_date;
    for _ in 0..6 {
        if current.status != SeriesStatus::NotComplete {
            break;
        }
        let Some(earliest) = current.dates.earliest else {
            break;
        };
        let date = earliest.max(last_date);
        // Stop at routine doses more than two years away (e.g. adolescent
        // boosters): the projection is for catching up.
        if !steps.is_empty() && date > input.assessment_date + chrono::Duration::days(730) {
            break;
        }
        // A preferable vaccine for the dose: the one that is a candidate
        // for the most antigens still needing a dose (e.g. Tdap over Td
        // when pertussis is due too), from the group's best series.
        let open: Vec<&phn_cdsi::PatientSeriesResult> = current
            .patient_series
            .iter()
            .filter_map(|&s| r.patient_series.get(s))
            .filter(|p| p.status == SeriesStatus::NotComplete)
            .collect();
        let vaccine = open
            .iter()
            .flat_map(|p| p.candidate_vaccines.iter())
            .max_by_key(|v| {
                let covers = open
                    .iter()
                    .filter(|p| p.candidate_vaccines.iter().any(|c| c.cvx == v.cvx))
                    .count();
                // Prefer earlier entries on ties (max_by_key keeps the last).
                (covers, std::cmp::Reverse(order_of(&open, &v.cvx)))
            })
            .cloned();
        steps.push(CatchUpStep {
            vaccine_group: current.vaccine_group.clone(),
            dose_number: current.forecast_dose_number.unwrap_or(steps.len() + 1),
            earliest: date.to_string(),
            recommended: current.dates.recommended.map(|d| d.max(date).to_string()),
            vaccine: vaccine
                .as_ref()
                .map(|v| format!("{} (CVX {})", v.vaccine_type, v.cvx)),
        });
        let Some(v) = vaccine else {
            break;
        };
        patient.doses.push(DoseInput::new(date, v.cvx.clone()));
        patient.assessment_date = date;
        last_date = date;
        r = phn_cdsi::run(&patient);
        match r
            .group_forecasts(&current.vaccine_group)
            .into_iter()
            .find(|g| g.series_group == current.series_group)
        {
            Some(g) => current = g.clone(),
            None => break,
        }
    }
    steps
}

/// One-patient result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PatientForecast {
    /// Age on the assessment date.
    pub age_at_assessment: String,
    pub sex: String,
    pub doses_entered: usize,
    /// Forecasts that apply, most urgent first.
    pub forecasts: Vec<GroupForecast>,
    /// Vaccine groups that do not apply at this age or now (aged out, not recommended).
    pub not_applicable: Vec<String>,
    /// Dose-by-dose evaluation.
    pub doses: Vec<DoseEvaluationOut>,
    /// Remaining doses with minimum intervals.
    pub catch_up: Vec<CatchUpStep>,
    /// Conditions recognized (title and code).
    pub conditions: Vec<String>,
    /// Contraindications that apply.
    pub contraindications: Vec<String>,
    /// Evidence of immunity found.
    pub evidence_of_immunity: Vec<String>,
    /// Administrative guidance from the supporting data for the series used.
    pub guidance: Vec<String>,
}

/// Counts for one vaccine group in file mode (null = suppressed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FileGroupCounts {
    pub vaccine_group: String,
    pub overdue: Option<u64>,
    pub due_now: Option<u64>,
    pub can_give_now: Option<u64>,
    pub due_later: Option<u64>,
    pub complete_or_immune: Option<u64>,
    /// Contraindicated, aged out, or not recommended.
    pub other: Option<u64>,
}

/// File-mode summary (aggregates only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FileForecastSummary {
    /// File read (relative to /data).
    pub file: String,
    pub persons: usize,
    pub dose_rows: usize,
    /// Rows without a usable person ID, date of birth, or date given.
    pub rows_skipped: usize,
    /// Rows whose vaccine could not be matched to a CVX code (details in the output file).
    pub unrecognized_vaccine_rows: usize,
    /// Columns used.
    pub columns: BTreeMap<String, String>,
    /// Counts of persons by vaccine group and due status; suppressed cells are null.
    pub by_group: Vec<FileGroupCounts>,
    pub suppression_threshold: u32,
    /// Files written to the output folder.
    pub files_written: Vec<String>,
}

/// Result of `forecast_immunizations_due`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ForecastImmunizationsResult {
    /// "patient" or "file".
    pub mode: String,
    pub assessment_date: String,
    pub engine: CdsiEngineInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patient: Option<PatientForecast>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileForecastSummary>,
}

// ----------------------------------------------------------------- helpers

fn parse_date(field: &str, s: &str) -> Result<NaiveDate> {
    let t = s.trim();
    let t = t.get(..10).filter(|p| p.len() == 10 && p.as_bytes()[4] == b'-').unwrap_or(t);
    NaiveDate::parse_from_str(t, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(t, "%m/%d/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(t, "%m-%d-%Y"))
        .map_err(|_| PhnError::InvalidInput(format!("{field} must be a date (YYYY-MM-DD), got '{s}'")))
}

fn parse_sex(s: Option<&str>) -> Gender {
    match s.map(|x| x.trim().to_ascii_lowercase()).as_deref() {
        Some("f" | "female" | "woman" | "girl") => Gender::Female,
        Some("m" | "male" | "man" | "boy") => Gender::Male,
        _ => Gender::Unknown,
    }
}

/// Age between two dates as "2 years 3 months" / "4 months 2 weeks" / "10 days".
pub fn age_text(dob: NaiveDate, on: NaiveDate) -> String {
    if on < dob {
        return "before birth".into();
    }
    let mut months = (on.year() - dob.year()) * 12 + on.month() as i32 - dob.month() as i32;
    if on.day() < dob.day() {
        months -= 1;
    }
    let anchor = phn_cdsi::duration::Span {
        years: 0,
        months,
        weeks: 0,
        days: 0,
    }
    .add_to(dob);
    let days = (on - anchor).num_days().max(0);
    let (y, m) = (months / 12, months % 12);
    let plural = |n: i64, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    if y >= 2 {
        if m == 0 {
            plural(y.into(), "year")
        } else {
            format!("{} {}", plural(y.into(), "year"), plural(m.into(), "month"))
        }
    } else if months >= 1 {
        let w = days / 7;
        if w == 0 {
            plural(months.into(), "month")
        } else {
            format!("{} {}", plural(months.into(), "month"), plural(w, "week"))
        }
    } else {
        plural((on - dob).num_days(), "day")
    }
}

/// Map a vaccine group name or common alias to the supporting data's name.
pub fn vaccine_group_name(text: &str) -> Option<String> {
    let k: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    let alias = match k.as_str() {
        "dtap" | "tdap" | "td" | "dtaptdaptd" | "tetanus" | "diphtheria" | "pertussis" | "dt" => {
            "DTaP/Tdap/Td"
        }
        "polio" | "ipv" | "opv" => "Polio",
        "hib" | "haemophilusinfluenzaetypeb" => "Hib",
        "pcv" | "pneumococcal" | "pneumo" | "ppsv" | "ppsv23" => "Pneumococcal",
        "hepb" | "hepatitisb" => "HepB",
        "hepa" | "hepatitisa" => "HepA",
        "mmr" | "measles" | "mumps" | "rubella" => "MMR",
        "varicella" | "var" | "chickenpox" | "vzv" => "Varicella",
        "rotavirus" | "rota" | "rv" => "Rotavirus",
        "hpv" | "humanpapillomavirus" => "HPV",
        "menacwy" | "meningococcal" | "mcv4" | "menacwy4" | "mening" => "Meningococcal",
        "menb" | "meningococcalb" => "Meningococcal B",
        "flu" | "influenza" => "Influenza",
        "covid" | "covid19" | "sarscov2" => "COVID-19",
        "rsv" | "nirsevimab" => "RSV",
        "zoster" | "shingles" | "rzv" => "Zoster",
        _ => "",
    };
    if !alias.is_empty() {
        return Some(alias.to_string());
    }
    SupportingData::get()
        .vaccine_group(text)
        .map(|g| g.name.clone())
}

/// Resolve a condition to a CDSi observation code and title.
fn resolve_condition(text: &str) -> Result<(String, String)> {
    let obs = &SupportingData::get().schedule.observations;
    let t = text.trim();
    let code = if t.len() <= 3 && t.chars().all(|c| c.is_ascii_digit()) {
        format!("{t:0>3}")
    } else {
        t.to_string()
    };
    if let Some(o) = obs.get(&code) {
        return Ok((o.code.clone(), o.title.clone()));
    }
    let norm = |s: &str| -> String {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect()
    };
    let k = norm(t);
    if let Some(o) = obs.values().find(|o| norm(&o.title) == k) {
        return Ok((o.code.clone(), o.title.clone()));
    }
    let contains: Vec<_> = obs
        .values()
        .filter(|o| k.len() >= 4 && norm(&o.title).contains(&k))
        .collect();
    if contains.len() == 1 {
        return Ok((contains[0].code.clone(), contains[0].title.clone()));
    }
    let candidates: Vec<String> = if contains.is_empty() {
        let titles: Vec<&str> = obs.values().map(|o| o.title.as_str()).collect();
        phn_core::text::rank(t, &titles, |s| vec![s.to_string()], 0.5)
            .into_iter()
            .take(6)
            .map(|r| {
                let o = obs.values().nth(r.index).expect("ranked index");
                format!("{} ({})", o.title, o.code)
            })
            .collect()
    } else {
        contains
            .iter()
            .take(8)
            .map(|o| format!("{} ({})", o.title, o.code))
            .collect()
    };
    Err(PhnError::InvalidInput(format!(
        "condition '{t}' did not match one CDSi observation. {} Give the code or the exact title.",
        if candidates.is_empty() {
            "No close match.".to_string()
        } else {
            format!("Closest: {}.", candidates.join("; "))
        }
    )))
}

fn resolve_vaccine(text: &str) -> Result<(String, String)> {
    match resolve(text) {
        Resolved::Cvx { cvx, label } => Ok((cvx, label)),
        Resolved::Ambiguous(options) => Err(PhnError::InvalidInput(format!(
            "vaccine '{text}' matches more than one CVX code: {}. Give the CVX code.",
            options
                .iter()
                .map(|(c, l)| format!("{l} (CVX {c})"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        Resolved::Unknown { suggestions } => Err(PhnError::InvalidInput(format!(
            "vaccine '{text}' is not a CVX code or a vaccine name in the CDSi supporting data.{} \
             Give the CVX code (lookup_vaccine_product can find it).",
            if suggestions.is_empty() {
                String::new()
            } else {
                format!(" Did you mean: {}?", suggestions.join(", "))
            }
        ))),
    }
}

fn due_status(f: &VaccineGroupForecast, assess: NaiveDate) -> DueStatus {
    match f.status {
        SeriesStatus::Complete => DueStatus::Complete,
        SeriesStatus::Immune => DueStatus::Immune,
        SeriesStatus::Contraindicated => DueStatus::Contraindicated,
        SeriesStatus::AgedOut => DueStatus::AgedOut,
        SeriesStatus::NotRecommended => DueStatus::NotRecommended,
        SeriesStatus::NotComplete => {
            if f.dates.past_due.is_some_and(|p| assess > p) {
                DueStatus::Overdue
            } else if f.dates.recommended.is_some_and(|r| assess >= r) {
                DueStatus::DueNow
            } else if f.dates.earliest.is_some_and(|e| assess >= e) {
                DueStatus::CanGiveNow
            } else {
                DueStatus::DueLater
            }
        }
    }
}

/// True when the patient is younger than the minimum age to start every
/// series behind a forecast (e.g. the 50+ pneumococcal series for a child).
fn too_young_to_start(r: &EngineResult, f: &VaccineGroupForecast, dob: NaiveDate) -> bool {
    let sd = SupportingData::get();
    !f.patient_series.is_empty()
        && f.patient_series.iter().all(|&s| {
            let ps = &r.patient_series[s];
            sd.antigens
                .get(&ps.antigen)
                .and_then(|a| a.series.iter().find(|x| x.name == ps.series_name))
                .and_then(|x| x.min_age_to_start)
                .is_some_and(|m| m.add_to(dob) > r.assessment_date)
        })
}

fn valid_doses(r: &EngineResult, f: &VaccineGroupForecast) -> usize {
    f.patient_series
        .iter()
        .map(|&s| r.patient_series[s].valid_doses)
        .sum()
}

/// The forecast that best describes a vaccine group for this patient: the
/// one whose series counted the most valid doses, that the patient is old
/// enough to start, that still needs a dose, with the lowest series group.
fn primary<'a>(r: &'a EngineResult, group: &str, dob: NaiveDate) -> Option<&'a VaccineGroupForecast> {
    r.group_forecasts(group).into_iter().min_by_key(|f| {
        (
            std::cmp::Reverse(valid_doses(r, f)),
            too_young_to_start(r, f, dob),
            f.status != SeriesStatus::NotComplete,
            f.series_group.parse::<u32>().unwrap_or(u32::MAX),
        )
    })
}

fn group_forecast(r: &EngineResult, f: &VaccineGroupForecast) -> GroupForecast {
    let d = |x: Option<NaiveDate>| x.map(|d| d.to_string());
    GroupForecast {
        vaccine_group: f.vaccine_group.clone(),
        series_group: f.series_group.clone(),
        series_type: f.series_type.clone(),
        series: f
            .patient_series
            .iter()
            .map(|&s| r.patient_series[s].series_name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        status: f.status.label().to_string(),
        due_status: due_status(f, r.assessment_date),
        next_dose_number: f.forecast_dose_number,
        earliest: d(f.dates.earliest),
        recommended: d(f.dates.recommended),
        past_due: d(f.dates.past_due),
        latest: d(f.dates.latest),
        recommended_vaccines: f
            .recommended_vaccines
            .iter()
            .map(|v| format!("{} (CVX {})", v.vaccine_type, v.cvx))
            .collect(),
        reasons: f.forecast_reasons.clone(),
    }
}

fn eval_label(s: EvalStatus) -> &'static str {
    s.label()
}

fn engine_info() -> CdsiEngineInfo {
    CdsiEngineInfo {
        logic_specification: format!("{LOGIC_SPEC_VERSION} ({LOGIC_SPEC_DATE})"),
        supporting_data: SUPPORTING_DATA_VERSION.into(),
        supporting_data_published: SUPPORTING_DATA_PUBLISHED.into(),
        test_cases_passed: CDSI_TEST_CASES_PASSED,
        test_cases_total: CDSI_TEST_CASES_TOTAL,
    }
}

fn sources() -> Vec<Source> {
    vec![
        Source {
            title: format!(
                "CDSi Supporting Data v{SUPPORTING_DATA_VERSION} (antigen series and schedule supporting data)"
            ),
            publisher: "CDC, National Center for Immunization and Respiratory Diseases".into(),
            url: Some(SUPPORTING_DATA_URL.into()),
            effective_date: Some(SUPPORTING_DATA_PUBLISHED.into()),
            retrieved: Some("2026-11-07".into()),
            mode: SourceMode::Snapshot,
        },
        Source {
            title: format!(
                "Clinical Decision Support for Immunization (CDSi): Logic Specification for ACIP Recommendations v{LOGIC_SPEC_VERSION}"
            ),
            publisher: "CDC, National Center for Immunization and Respiratory Diseases".into(),
            url: Some(LOGIC_SPEC_URL.into()),
            effective_date: Some(LOGIC_SPEC_DATE.into()),
            retrieved: Some("2026-11-07".into()),
            mode: SourceMode::Computed,
        },
    ]
}

fn standard_caveats(r: Option<&EngineResult>) -> Vec<String> {
    let mut c = vec![
        "Decision support only. Verify the dose history against the patient's MIIS \
         (Massachusetts Immunization Information System) record and current guidance before \
         acting; the forecast is only as complete as the doses entered."
            .to_string(),
        "CDSi encodes CDC/ACIP recommendations only. Your default basis for recommendations is \
         Massachusetts DPH, then AAP/ACOG, then CDC; since 2025, ACIP-based forecasts may differ \
         from DPH and AAP/ACOG positions. Use compare_vaccine_recommendations to check a \
         vaccine before acting."
            .to_string(),
        format!(
            "Engine: CDSi Logic Specification v{LOGIC_SPEC_VERSION} with Supporting Data \
             v{SUPPORTING_DATA_VERSION} ({SUPPORTING_DATA_PUBLISHED}). It passes {} of {} CDC \
             test cases (all 1,064 \
             healthy childhood and adult cases; the 36 exceptions are older underlying-condition \
             cases, listed in docs/cdsi-test-results.md). The engine and its documentation have \
             not yet been reviewed by the nurse (status: pending).",
            md::num(CDSI_TEST_CASES_PASSED as f64, 0),
            md::num(CDSI_TEST_CASES_TOTAL as f64, 0)
        ),
        "Doses are judged against ACIP minimum ages and intervals with the 4-day grace period. \
         Massachusetts school and child-care requirements can differ; use \
         check_ma_school_immunization_requirements for those."
            .to_string(),
    ];
    if let Some(r) = r {
        let seasonal = r
            .patient_series
            .iter()
            .filter(|p| p.best)
            .any(|p| {
                p.forecast_reasons
                    .iter()
                    .any(|x| x.contains("seasonal recommendation end"))
            });
        if seasonal {
            c.push(
                "A seasonal series (influenza, COVID-19, or RSV) shows 'not recommended now' \
                 because the season in the embedded supporting data has ended. CDC publishes new \
                 seasonal dates in later supporting-data versions; check the current season's \
                 CDC and DPH guidance."
                    .into(),
            );
        }
        let scdm = r
            .patient_series
            .iter()
            .filter(|p| p.best)
            .any(|p| p.series_name.contains("Shared Clinical Decision"));
        if scdm {
            c.push(
                "Some forecasts come from shared clinical decision-making series (for example \
                 MenB or COVID-19): a dose is an option to discuss, not a routine \
                 recommendation."
                    .into(),
            );
        }
    }
    c
}

// ------------------------------------------------------------------- tool

/// Evaluate a dose history and forecast the doses due (CDC CDSi).
pub fn forecast_immunizations_due(
    ctx: &Context,
    args: ForecastImmunizationsArgs,
) -> Result<Envelope<ForecastImmunizationsResult>> {
    let assess = match &args.assessment_date {
        Some(d) => parse_date("assessment_date", d)?,
        None => chrono::Local::now().date_naive(),
    };
    let groups_filter: Vec<String> = args
        .vaccine_groups
        .iter()
        .map(|g| {
            vaccine_group_name(g).ok_or_else(|| {
                PhnError::InvalidInput(format!(
                    "unknown vaccine group '{g}'. Use names like DTaP, Polio, Hib, \
                     pneumococcal, HepB, MMR, varicella, HepA, rotavirus, HPV, MenACWY, MenB, \
                     influenza, COVID-19, RSV, or zoster."
                ))
            })
        })
        .collect::<Result<_>>()?;
    let mut observations = Vec::new();
    let mut condition_labels = Vec::new();
    for c in &args.conditions {
        let (code, title) = resolve_condition(&c.condition)?;
        let date = match &c.date {
            Some(d) => Some(parse_date("condition date", d)?),
            None => None,
        };
        condition_labels.push(format!("{title} ({code})"));
        observations.push(Observation { code, date });
    }
    match &args.file {
        Some(file) => {
            if args.date_of_birth.is_some() || !args.doses.is_empty() {
                return Err(PhnError::InvalidInput(
                    "use either file (many people) or date_of_birth and doses (one person), \
                     not both"
                        .into(),
                ));
            }
            file_mode(ctx, &args, file, assess, &groups_filter, observations)
        }
        None => patient_mode(&args, assess, &groups_filter, observations, condition_labels),
    }
}

fn patient_mode(
    args: &ForecastImmunizationsArgs,
    assess: NaiveDate,
    groups_filter: &[String],
    observations: Vec<Observation>,
    condition_labels: Vec<String>,
) -> Result<Envelope<ForecastImmunizationsResult>> {
    let dob = match &args.date_of_birth {
        Some(d) => parse_date("date_of_birth", d)?,
        None => {
            return Err(PhnError::InvalidInput(
                "give date_of_birth (and doses), or a file in /data for many people".into(),
            ));
        }
    };
    if dob > assess {
        return Err(PhnError::InvalidInput(
            "the date of birth is after the assessment date".into(),
        ));
    }
    if args.doses.len() > MAX_DOSES {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_DOSES} doses per call"
        )));
    }
    let gender = parse_sex(args.sex.as_deref());
    let mut doses = Vec::new();
    let mut entered = Vec::new();
    for (i, d) in args.doses.iter().enumerate() {
        let date = parse_date(&format!("dose {} date", i + 1), &d.date)?;
        if date < dob {
            return Err(PhnError::InvalidInput(format!(
                "dose {} ({}) is dated before the date of birth",
                i + 1,
                d.vaccine
            )));
        }
        if date > assess {
            return Err(PhnError::InvalidInput(format!(
                "dose {} ({}) is dated after the assessment date",
                i + 1,
                d.vaccine
            )));
        }
        let (cvx, label) = resolve_vaccine(&d.vaccine)?;
        doses.push(DoseInput::new(date, cvx.clone()));
        entered.push((d.vaccine.trim().to_string(), cvx, label));
    }
    let input = PatientInput {
        dob,
        gender,
        born_in_us: args.born_in_us,
        observations,
        doses,
        assessment_date: assess,
    };
    let r = phn_cdsi::run(&input);

    // Vaccine groups to report.
    let mut groups: Vec<String> = Vec::new();
    for f in &r.vaccine_groups {
        if (groups_filter.is_empty() || groups_filter.contains(&f.vaccine_group))
            && !groups.contains(&f.vaccine_group)
        {
            groups.push(f.vaccine_group.clone());
        }
    }
    let mut forecasts = Vec::new();
    let mut not_applicable = Vec::new();
    for g in &groups {
        let all = r.group_forecasts(g);
        let shown: Vec<&VaccineGroupForecast> = all
            .iter()
            .copied()
            .filter(|f| {
                let inactive = matches!(
                    f.status,
                    SeriesStatus::AgedOut | SeriesStatus::NotRecommended
                );
                let others_active = all.iter().any(|o| {
                    !std::ptr::eq(*o, *f)
                        && !matches!(o.status, SeriesStatus::AgedOut | SeriesStatus::NotRecommended)
                });
                // Hide inactive and not-yet-startable series groups when
                // another forecast for the group applies.
                !((inactive || too_young_to_start(&r, f, dob)) && others_active)
            })
            .collect();
        let all_inactive = shown
            .iter()
            .all(|f| matches!(f.status, SeriesStatus::AgedOut | SeriesStatus::NotRecommended));
        if all_inactive {
            let why = shown
                .iter()
                .map(|f| f.status.label().to_lowercase())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join("/");
            not_applicable.push(format!("{g} ({why})"));
            if !groups_filter.is_empty() {
                forecasts.extend(shown.iter().map(|f| group_forecast(&r, f)));
            }
            continue;
        }
        forecasts.extend(shown.iter().map(|f| group_forecast(&r, f)));
    }
    forecasts.sort_by(|a, b| {
        (a.due_status, a.earliest.clone(), a.vaccine_group.clone()).cmp(&(
            b.due_status,
            b.earliest.clone(),
            b.vaccine_group.clone(),
        ))
    });

    // Dose evaluations: per vaccine group, from its primary forecast.
    let sd = SupportingData::get();
    let mut dose_rows = Vec::new();
    for (i, (vaccine, cvx, _label)) in entered.iter().enumerate() {
        let mut groups_for_dose: Vec<String> = Vec::new();
        if let Some(e) = sd.schedule.cvx_map.get(cvx) {
            for a in &e.associations {
                if let Some(g) = sd.vaccine_group_of(&a.antigen)
                    && !groups_for_dose.contains(&g.name)
                {
                    groups_for_dose.push(g.name.clone());
                }
            }
        }
        for g in groups_for_dose {
            if !groups_filter.is_empty() && !groups_filter.contains(&g) {
                continue;
            }
            let Some(f) = primary(&r, &g, dob) else {
                continue;
            };
            let evals: Vec<&phn_cdsi::DoseEvaluation> = f
                .patient_series
                .iter()
                .flat_map(|&s| r.patient_series[s].evaluations.iter())
                .filter(|e| e.dose_index == i)
                .collect();
            if evals.is_empty() {
                continue;
            }
            let status = if evals.iter().any(|e| e.status == EvalStatus::NotValid) {
                EvalStatus::NotValid
            } else if evals.iter().any(|e| e.status == EvalStatus::SubStandard) {
                EvalStatus::SubStandard
            } else if evals.iter().any(|e| e.status == EvalStatus::Extraneous) {
                EvalStatus::Extraneous
            } else {
                EvalStatus::Valid
            };
            let mut reasons: Vec<String> = Vec::new();
            for e in &evals {
                for reason in &e.reasons {
                    let text = if evals.len() > 1 && status != EvalStatus::Valid {
                        format!("{}: {reason}", e.antigen)
                    } else {
                        reason.clone()
                    };
                    if !reasons.contains(&text) {
                        reasons.push(text);
                    }
                }
            }
            let counts_as = (status == EvalStatus::Valid)
                .then(|| evals.iter().filter_map(|e| e.target_dose).min())
                .flatten();
            let date = input.doses[i].date;
            dose_rows.push(DoseEvaluationOut {
                dose: i + 1,
                date: date.to_string(),
                age_at_dose: age_text(dob, date),
                vaccine: vaccine.clone(),
                cvx: cvx.clone(),
                vaccine_group: g,
                status: eval_label(status).into(),
                counts_as_dose: counts_as,
                reasons,
            });
        }
    }

    // Catch-up schedule for forecasts that can be acted on now (overdue,
    // due, or eligible): project each remaining dose by re-running the
    // engine as if the previous dose were given on its earliest date.
    let mut catch_up = Vec::new();
    for f in r.vaccine_groups.iter().filter(|f| {
        f.status == SeriesStatus::NotComplete
            && forecasts.iter().any(|g| {
                g.vaccine_group == f.vaccine_group
                    && g.series_group == f.series_group
                    && g.due_status <= DueStatus::CanGiveNow
            })
    }) {
        catch_up.extend(project_catch_up(&input, &r, f));
    }

    let mut guidance: Vec<String> = Vec::new();
    for f in r.vaccine_groups.iter().filter(|f| {
        forecasts
            .iter()
            .any(|g| g.vaccine_group == f.vaccine_group && g.series_group == f.series_group)
            && f.status == SeriesStatus::NotComplete
    }) {
        for &s in &f.patient_series {
            for g in &r.patient_series[s].admin_guidance {
                let text = format!("{}: {g}", f.vaccine_group);
                if !guidance.contains(&text) {
                    guidance.push(text);
                }
            }
        }
    }
    let contraindications: Vec<String> = r
        .contraindications
        .iter()
        .map(|c| format!("{} ({}): {}", c.antigen, c.observation_code, c.text))
        .collect();
    let immunity: Vec<String> = r
        .immunity
        .iter()
        .map(|c| format!("{}: {}", c.antigen, c.text))
        .collect();

    // Markdown summary.
    let age = age_text(dob, assess);
    let mut s = format!(
        "**Immunization forecast (CDC CDSi, ACIP schedule)** — assessed {assess}; age {age}; \
         {} dose(s) entered{}.\n\n",
        entered.len(),
        if condition_labels.is_empty() {
            String::new()
        } else {
            format!("; conditions: {}", condition_labels.join(", "))
        }
    );
    // Doses first due more than a year from now go on one line.
    let horizon = assess + chrono::Duration::days(365);
    let far = |f: &GroupForecast| {
        f.due_status == DueStatus::DueLater
            && f.earliest
                .as_deref()
                .and_then(|e| NaiveDate::parse_from_str(e, "%Y-%m-%d").ok())
                .is_some_and(|e| e > horizon)
    };
    let later: Vec<String> = forecasts
        .iter()
        .filter(|f| far(f))
        .map(|f| {
            format!(
                "{} (from {})",
                f.vaccine_group,
                f.earliest.clone().unwrap_or_default()
            )
        })
        .collect();
    let rows: Vec<Vec<String>> = forecasts
        .iter()
        .filter(|f| !far(f))
        .map(|f| {
            vec![
                if f.series_type == "Risk" {
                    format!("{} (risk series)", f.vaccine_group)
                } else {
                    f.vaccine_group.clone()
                },
                f.due_status.label().into(),
                f.next_dose_number.map(|n| n.to_string()).unwrap_or_default(),
                f.earliest.clone().unwrap_or_default(),
                f.recommended.clone().unwrap_or_default(),
                f.past_due.clone().unwrap_or_default(),
            ]
        })
        .collect();
    if rows.is_empty() {
        s.push_str("No vaccine group forecasts for the groups requested.\n");
    } else {
        s.push_str(&md::table(
            &["Vaccine group", "Status", "Next dose", "Earliest", "Recommended", "Past due"],
            &rows,
        ));
    }
    if !later.is_empty() {
        s.push_str(&format!("\nDue later (more than a year away): {}.\n", later.join(", ")));
    }
    if !not_applicable.is_empty() {
        s.push_str(&format!(
            "\nNot applicable now: {}.\n",
            not_applicable.join(", ")
        ));
    }
    if !dose_rows.is_empty() {
        s.push_str("\n**Dose evaluation**\n\n");
        let rows: Vec<Vec<String>> = dose_rows
            .iter()
            .map(|d| {
                vec![
                    d.dose.to_string(),
                    d.date.clone(),
                    d.age_at_dose.clone(),
                    format!("{} (CVX {})", d.vaccine, d.cvx),
                    d.vaccine_group.clone(),
                    match d.counts_as_dose {
                        Some(n) => format!("{} (dose {n})", d.status),
                        None => d.status.clone(),
                    },
                    d.reasons.join("; "),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["#", "Date", "Age", "Vaccine", "Group", "Status", "Notes"],
            &rows,
        ));
    }
    if !catch_up.is_empty() {
        s.push_str(
            "\n**Projected catch-up schedule** (each dose on its earliest valid date, starting \
             today)\n\n",
        );
        let rows: Vec<Vec<String>> = catch_up
            .iter()
            .map(|c| {
                vec![
                    c.vaccine_group.clone(),
                    c.dose_number.to_string(),
                    c.earliest.clone(),
                    c.recommended.clone().unwrap_or_default(),
                    c.vaccine.clone().unwrap_or_default(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Vaccine group", "Dose", "Earliest", "Recommended", "Vaccine assumed"],
            &rows,
        ));
        s.push_str(
            "\nThe projection re-runs the forecast after each assumed dose, so doses that \
             become unnecessary with age are dropped. Each vaccine group is projected on its \
             own: live vaccines not given on the same day must be at least 28 days apart. Real \
             visits will differ; re-run the forecast after each visit.\n",
        );
    }
    if !contraindications.is_empty() {
        s.push_str(&format!(
            "\n**Contraindications:** {}\n",
            contraindications.join(" ")
        ));
    }
    if !immunity.is_empty() {
        s.push_str(&format!(
            "\n**Evidence of immunity:** {}\n",
            immunity.join("; ")
        ));
    }

    let mut caveats = standard_caveats(Some(&r));
    if !r.unrecognized_cvx.is_empty() {
        caveats.push(format!(
            "These CVX codes are not in the CDSi supporting data and were not evaluated: {}.",
            r.unrecognized_cvx.join(", ")
        ));
    }
    if args.born_in_us.is_none() && dob < NaiveDate::from_ymd_opt(1980, 1, 1).unwrap_or(dob) {
        caveats.push(
            "Country of birth was not given, so birth in the U.S. before 1980 (evidence of \
             varicella immunity for most adults) was not applied. Set born_in_us if known."
                .into(),
        );
    }
    if matches!(parse_sex(args.sex.as_deref()), Gender::Unknown) {
        caveats.push(
            "Sex was not given; HPV is forecast with the series CDSi uses for females and \
             unknown sex."
                .into(),
        );
    }
    let env = Envelope::new(
        s,
        ForecastImmunizationsResult {
            mode: "patient".into(),
            assessment_date: assess.to_string(),
            engine: engine_info(),
            patient: Some(PatientForecast {
                age_at_assessment: age,
                sex: parse_sex(args.sex.as_deref()).label().into(),
                doses_entered: entered.len(),
                forecasts,
                not_applicable,
                doses: dose_rows,
                catch_up,
                conditions: condition_labels,
                contraindications,
                evidence_of_immunity: immunity,
                guidance,
            }),
            file: None,
        },
    )
    .sources(sources())
    .caveats(caveats)
    .privacy(Privacy {
        notes: vec![
            "Built only from the dose history in the request. The date of birth is used for \
             the calculation and is not repeated; ages are shown instead."
                .into(),
        ],
        ..Privacy::none()
    })
    .decision_support();
    Ok(env)
}

// -------------------------------------------------------------- file mode

const ID_COLUMNS: &[&str] = &[
    "person id", "person_id", "client id", "client_id", "patient id", "patient_id", "id",
    "mrn", "record id", "record_id", "student id", "student_id", "case id", "case_id",
];
const DOB_COLUMNS: &[&str] = &[
    "dob", "date of birth", "date_of_birth", "birth date", "birthdate", "birth_date",
];
const DATE_COLUMNS: &[&str] = &[
    "date given", "date_given", "date administered", "date_administered", "administered",
    "vaccination date", "vaccination_date", "admin date", "admin_date", "dose date",
    "dose_date", "date",
];
const VACCINE_COLUMNS: &[&str] = &[
    "cvx", "cvx code", "cvx_code", "vaccine code", "vaccine_code", "vaccine", "vaccine name",
    "vaccine_name", "product", "trade name", "trade_name",
];
const SEX_COLUMNS: &[&str] = &["sex", "gender"];

fn find_column(
    table: &phn_local::table::Table,
    explicit: Option<&str>,
    candidates: &[&str],
    what: &str,
    required: bool,
) -> Result<Option<usize>> {
    if let Some(name) = explicit {
        return table.column_index(name).map(Some).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "{what} column '{name}' is not in the file. Columns: {}",
                table.headers.join(", ")
            ))
        });
    }
    for c in candidates {
        if let Some(i) = table.column_index(c) {
            return Ok(Some(i));
        }
    }
    if required {
        return Err(PhnError::InvalidInput(format!(
            "could not find the {what} column. Columns: {}. Name it with the matching \
             *_column argument.",
            table.headers.join(", ")
        )));
    }
    Ok(None)
}

const FILE_STATUSES: [&str; 6] = [
    "Overdue",
    "Due now",
    "Can give now",
    "Due later",
    "Complete or immune",
    "Other",
];

fn status_column(d: DueStatus) -> usize {
    match d {
        DueStatus::Overdue => 0,
        DueStatus::DueNow => 1,
        DueStatus::CanGiveNow => 2,
        DueStatus::DueLater => 3,
        DueStatus::Complete | DueStatus::Immune => 4,
        _ => 5,
    }
}

const ROUTINE_GROUPS: &[&str] = &[
    "DTaP/Tdap/Td",
    "Polio",
    "Hib",
    "Pneumococcal",
    "HepB",
    "MMR",
    "Varicella",
    "HepA",
    "Rotavirus",
    "HPV",
    "Meningococcal",
    "Meningococcal B",
    "Influenza",
    "COVID-19",
    "RSV",
    "Zoster",
];

fn file_mode(
    ctx: &Context,
    args: &ForecastImmunizationsArgs,
    file: &str,
    assess: NaiveDate,
    groups_filter: &[String],
    observations: Vec<Observation>,
) -> Result<Envelope<ForecastImmunizationsResult>> {
    let (table, label) = crate::core::files::load_table(ctx, file, args.sheet.as_deref())?;
    let id_col = find_column(&table, args.id_column.as_deref(), ID_COLUMNS, "person ID", true)?
        .unwrap_or_default();
    let dob_col = find_column(&table, args.dob_column.as_deref(), DOB_COLUMNS, "date of birth", true)?
        .unwrap_or_default();
    let date_col = find_column(&table, args.date_column.as_deref(), DATE_COLUMNS, "date given", true)?
        .unwrap_or_default();
    let vac_col = find_column(&table, args.vaccine_column.as_deref(), VACCINE_COLUMNS, "vaccine", true)?
        .unwrap_or_default();
    let sex_col = find_column(&table, args.sex_column.as_deref(), SEX_COLUMNS, "sex", false)?;
    if [id_col, dob_col, date_col, vac_col].iter().collect::<BTreeSet<_>>().len() < 4 {
        return Err(PhnError::InvalidInput(
            "the person ID, date of birth, date given, and vaccine columns must be different"
                .into(),
        ));
    }
    let cell = |r: &Vec<String>, c: usize| r.get(c).map(|s| s.trim().to_string()).unwrap_or_default();

    struct Person {
        dob: Option<NaiveDate>,
        sex: Gender,
        doses: Vec<(NaiveDate, String, String)>,
    }
    let mut persons: BTreeMap<String, Person> = BTreeMap::new();
    let mut rows_skipped = 0usize;
    let mut unrecognized_rows = 0usize;
    let mut unrecognized: BTreeMap<String, usize> = BTreeMap::new();
    let mut resolved_cache: BTreeMap<String, Option<String>> = BTreeMap::new();
    for row in &table.rows {
        let id = cell(row, id_col);
        let dob = parse_date("dob", &cell(row, dob_col)).ok();
        let given = parse_date("date", &cell(row, date_col)).ok();
        let vaccine = cell(row, vac_col);
        if id.is_empty() || dob.is_none() || given.is_none() || vaccine.is_empty() {
            rows_skipped += 1;
            continue;
        }
        let cvx = resolved_cache
            .entry(vaccine.clone())
            .or_insert_with(|| match resolve(&vaccine) {
                Resolved::Cvx { cvx, .. } => Some(cvx),
                _ => None,
            })
            .clone();
        let p = persons.entry(id).or_insert(Person {
            dob,
            sex: Gender::Unknown,
            doses: Vec::new(),
        });
        if let Some(c) = sex_col {
            let g = parse_sex(Some(&cell(row, c)));
            if g != Gender::Unknown {
                p.sex = g;
            }
        }
        match cvx {
            Some(cvx) => p.doses.push((given.unwrap_or(assess), cvx, vaccine)),
            None => {
                unrecognized_rows += 1;
                *unrecognized.entry(vaccine).or_default() += 1;
            }
        }
    }
    if persons.len() > MAX_PERSONS {
        return Err(PhnError::InvalidInput(format!(
            "the file has {} people; the limit is {MAX_PERSONS} per call",
            persons.len()
        )));
    }
    let groups: Vec<String> = if groups_filter.is_empty() {
        ROUTINE_GROUPS.iter().map(|s| s.to_string()).collect()
    } else {
        groups_filter.to_vec()
    };
    let mut counts: BTreeMap<String, [u64; 6]> = BTreeMap::new();
    let mut season_ended = false;
    let mut forecast_rows: Vec<Vec<String>> = Vec::new();
    let mut dose_rows: Vec<Vec<String>> = Vec::new();
    for (id, p) in &persons {
        let Some(dob) = p.dob else { continue };
        let input = PatientInput {
            dob,
            gender: p.sex,
            born_in_us: args.born_in_us,
            observations: observations.clone(),
            doses: p
                .doses
                .iter()
                .filter(|(d, _, _)| *d <= assess && *d >= dob)
                .map(|(d, cvx, _)| DoseInput::new(*d, cvx.clone()))
                .collect(),
            assessment_date: assess,
        };
        let kept: Vec<&(NaiveDate, String, String)> = p
            .doses
            .iter()
            .filter(|(d, _, _)| *d <= assess && *d >= dob)
            .collect();
        let r = phn_cdsi::run(&input);
        for g in &groups {
            let Some(f) = primary(&r, g, dob) else {
                continue;
            };
            season_ended |= f
                .forecast_reasons
                .iter()
                .any(|x| x.contains("seasonal recommendation end"));
            let ds = due_status(f, assess);
            counts.entry(g.clone()).or_insert([0; 6])[status_column(ds)] += 1;
            let d = |x: Option<NaiveDate>| x.map(|d| d.to_string()).unwrap_or_default();
            forecast_rows.push(vec![
                id.clone(),
                g.clone(),
                f.status.label().into(),
                ds.label().into(),
                f.forecast_dose_number.map(|n| n.to_string()).unwrap_or_default(),
                d(f.dates.earliest),
                d(f.dates.recommended),
                d(f.dates.past_due),
            ]);
            for &s in &f.patient_series {
                for e in &r.patient_series[s].evaluations {
                    let entered = kept.get(e.dose_index).map(|k| k.2.clone()).unwrap_or_default();
                    dose_rows.push(vec![
                        id.clone(),
                        g.clone(),
                        e.antigen.clone(),
                        e.date.to_string(),
                        entered,
                        e.cvx.clone(),
                        e.status.label().into(),
                        e.reasons.join("; "),
                    ]);
                }
            }
        }
    }
    for (v, n) in &unrecognized {
        dose_rows.push(vec![
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            v.clone(),
            String::new(),
            "Not evaluated".into(),
            format!("vaccine not recognized ({n} row(s)); give a CVX code"),
        ]);
    }

    // Write per-person results to the output folder.
    let sandbox = ctx.sandbox()?;
    let stem = label
        .rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.').map(|(s, _)| s.to_string()))
        .unwrap_or_else(|| "immunizations".into());
    let write = |name: &str, headers: &[&str], rows: Vec<Vec<String>>| -> Result<String> {
        let path = sandbox.new_output_file(name)?;
        let block = phn_docs::model::TableBlock {
            caption: None,
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows,
        };
        std::fs::write(&path, phn_docs::csv_out::table_to_csv(&block)?)?;
        Ok(sandbox.display_path(&path))
    };
    let f1 = write(
        &format!("{stem}_forecast_{assess}.csv"),
        &[
            "Person ID",
            "Vaccine group",
            "CDSi status",
            "Due status",
            "Next dose",
            "Earliest",
            "Recommended",
            "Past due",
        ],
        forecast_rows,
    )?;
    let f2 = write(
        &format!("{stem}_dose_evaluations_{assess}.csv"),
        &[
            "Person ID",
            "Vaccine group",
            "Antigen",
            "Date given",
            "Vaccine as entered",
            "CVX",
            "Evaluation",
            "Reasons",
        ],
        dose_rows,
    )?;

    // Aggregate counts with suppression.
    let threshold = ctx.suppression_threshold();
    let names: Vec<String> = groups
        .iter()
        .filter(|g| counts.contains_key(*g))
        .cloned()
        .collect();
    let matrix: Vec<Vec<Option<u64>>> = names
        .iter()
        .map(|g| counts[g].iter().map(|&n| Some(n)).collect())
        .collect();
    let sup = suppress(
        &matrix,
        &SuppressionOptions {
            threshold,
            ..SuppressionOptions::default()
        },
    );
    let shown = |r: usize, c: usize| (!sup.is_hidden(r, c)).then(|| matrix[r][c].unwrap_or(0));
    let by_group: Vec<FileGroupCounts> = names
        .iter()
        .enumerate()
        .map(|(r, g)| FileGroupCounts {
            vaccine_group: g.clone(),
            overdue: shown(r, 0),
            due_now: shown(r, 1),
            can_give_now: shown(r, 2),
            due_later: shown(r, 3),
            complete_or_immune: shown(r, 4),
            other: shown(r, 5),
        })
        .collect();
    let fmt = |v: Option<u64>| v.map_or_else(|| format!("<{threshold}*"), |n| n.to_string());
    let rows: Vec<Vec<String>> = by_group
        .iter()
        .map(|g| {
            vec![
                g.vaccine_group.clone(),
                fmt(g.overdue),
                fmt(g.due_now),
                fmt(g.can_give_now),
                fmt(g.due_later),
                fmt(g.complete_or_immune),
                fmt(g.other),
            ]
        })
        .collect();
    let dose_count: usize = persons.values().map(|p| p.doses.len()).sum();
    let mut s = format!(
        "**Immunization forecasts for {} people** from {label} (CDC CDSi, ACIP schedule), \
         assessed {assess}. {dose_count} dose rows evaluated.\n\n",
        persons.len()
    );
    let mut headers = vec!["Vaccine group"];
    headers.extend(FILE_STATUSES.iter());
    s.push_str(&md::table(&headers, &rows));
    s.push_str(&format!(
        "\nCounts are people. `<{threshold}*` = suppressed (fewer than {threshold}, or hidden \
         to protect another cell).\n\nPer-person forecasts: /data/{f1}\nDose evaluations: \
         /data/{f2}\n"
    ));
    let mut caveats = standard_caveats(None);
    if rows_skipped > 0 {
        caveats.push(format!(
            "{rows_skipped} row(s) were skipped for a missing person ID, date of birth, date \
             given, or vaccine."
        ));
    }
    if unrecognized_rows > 0 {
        caveats.push(format!(
            "{unrecognized_rows} row(s) had a vaccine that could not be matched to a CVX code \
             and were not evaluated; the dose-evaluation file lists them. Use CVX codes."
        ));
    }
    if !observations.is_empty() {
        caveats.push("The conditions given were applied to every person in the file.".into());
    }
    if season_ended {
        caveats.push(
            "Seasonal series (influenza, COVID-19, or RSV) are counted as 'other' (not \
             recommended now) because the season in the embedded supporting data has ended. \
             Check the current season's CDC and DPH guidance."
                .into(),
        );
    }
    let mut columns = BTreeMap::new();
    columns.insert("person_id".into(), table.headers[id_col].clone());
    columns.insert("date_of_birth".into(), table.headers[dob_col].clone());
    columns.insert("date_given".into(), table.headers[date_col].clone());
    columns.insert("vaccine".into(), table.headers[vac_col].clone());
    if let Some(c) = sex_col {
        columns.insert("sex".into(), table.headers[c].clone());
    }
    let files = vec![f1.clone(), f2.clone()];
    Ok(Envelope::new(
        s,
        ForecastImmunizationsResult {
            mode: "file".into(),
            assessment_date: assess.to_string(),
            engine: engine_info(),
            patient: None,
            file: Some(FileForecastSummary {
                file: label.clone(),
                persons: persons.len(),
                dose_rows: dose_count,
                rows_skipped,
                unrecognized_vaccine_rows: unrecognized_rows,
                columns,
                by_group,
                suppression_threshold: threshold,
                files_written: files.clone(),
            }),
        },
    )
    .source(Source::local_file(label))
    .sources(sources())
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: vec!["person identifier".into(), "date of birth".into()],
        files_written: files,
        notes: vec![
            "Per-person forecasts and dose evaluations (with person IDs) were written to the \
             output folder only; the response holds counts."
                .into(),
        ],
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_and_aliases() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(age_text(d("2026-01-15"), d("2026-01-25")), "10 days");
        assert_eq!(age_text(d("2026-01-15"), d("2026-03-29")), "2 months 2 weeks");
        assert_eq!(age_text(d("2020-01-15"), d("2026-03-29")), "6 years 2 months");
        assert_eq!(vaccine_group_name("tdap").as_deref(), Some("DTaP/Tdap/Td"));
        assert_eq!(vaccine_group_name("Chickenpox").as_deref(), Some("Varicella"));
        assert_eq!(vaccine_group_name("Yellow Fever").as_deref(), Some("Yellow Fever"));
        assert!(vaccine_group_name("nonsense").is_none());
        assert_eq!(resolve_condition("007").unwrap().0, "007");
        assert_eq!(resolve_condition("7").unwrap().0, "007");
        assert_eq!(resolve_condition("pregnant").unwrap().0, "007");
        assert!(resolve_condition("zzzz").is_err());
    }
}
