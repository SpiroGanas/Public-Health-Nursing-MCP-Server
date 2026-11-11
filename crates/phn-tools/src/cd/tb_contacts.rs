//! CD-11 `plan_tb_contact_investigation`: applies the 2005 NTCA/CDC contact
//! investigation guidelines: the infectious period (Table 2), whether to
//! investigate, contact priorities (Figures 2-4), time frames (Table 3), and
//! the testing schedule with the retest 8-10 weeks after last exposure. It
//! writes a tracking workbook (XLSX, with priority and due-date formulas) to
//! `/data/output`; with a local contact list it fills the workbook there and
//! returns only suppressed counts and expansion signals.

use std::collections::BTreeMap;

use chrono::{Months, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use rust_xlsxwriter::{DataValidation, Format, Formula, Workbook, XlsxError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::tb_content::{self, CaseType, Clinic, Condition, Priority, TbContent};
use crate::context::Context;
use crate::core::practice::{self, CitedNote};
use crate::md;

/// Anatomic site of the index patient's disease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiseaseSite {
    #[default]
    Pulmonary,
    Laryngeal,
    Pleural,
    /// Extrapulmonary disease only (no pulmonary, laryngeal, or pleural site).
    ExtrapulmonaryOnly,
}

/// Chest radiograph finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CxrFinding {
    /// Abnormal and consistent with TB disease.
    ConsistentWithTb,
    /// Abnormal but not consistent with TB disease.
    AbnormalNotConsistent,
    Normal,
    NotDone,
}

/// The index patient (clinical facts only; no names or identifiers).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct IndexCase {
    /// pulmonary (default), laryngeal, pleural, or extrapulmonary_only.
    pub site: DiseaseSite,
    /// The patient has TB symptoms (cough, fever, weight loss, night sweats...).
    pub symptomatic: bool,
    /// AFB seen on sputum (or other respiratory specimen) smear.
    pub afb_smear_positive: Option<bool>,
    /// Cavity on plain chest radiograph.
    pub cavitary: Option<bool>,
    /// consistent_with_tb, abnormal_not_consistent, normal, or not_done.
    pub chest_radiograph: Option<CxrFinding>,
    /// Approved NAA test for M. tuberculosis positive.
    pub naa_positive: Option<bool>,
    /// Culture positive for M. tuberculosis.
    pub culture_positive: Option<bool>,
    /// Multidrug-resistant TB (known or suspected).
    pub mdr_tb: bool,
    /// Index patient's age in years (transmission from children under 10 is unusual).
    pub age_years: Option<u32>,
    /// Date TB symptoms began (YYYY-MM-DD).
    pub symptom_onset_date: Option<String>,
    /// Date of the first positive finding consistent with TB (abnormal chest
    /// radiograph, positive smear), YYYY-MM-DD.
    pub first_positive_finding_date: Option<String>,
    /// Date TB was suspected or diagnosed, YYYY-MM-DD.
    pub suspected_diagnosis_date: Option<String>,
    /// Date effective TB treatment began, YYYY-MM-DD.
    pub treatment_start_date: Option<String>,
    /// Date airborne isolation began, YYYY-MM-DD (exposures after it are unlikely).
    pub isolation_start_date: Option<String>,
    /// Date the TB program closed the infectious period (criteria met), YYYY-MM-DD.
    pub infectious_period_end_date: Option<String>,
}

/// Where a group of contacts was exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExposureSetting {
    /// Same household.
    Household,
    Workplace,
    School,
    ChildCare,
    /// Congregate living (correctional facility, shelter, nursing home...).
    CongregateLiving,
    /// Present during bronchoscopy, sputum induction, or autopsy.
    MedicalProcedure,
    /// Other health care exposure.
    HealthCare,
    Social,
    Travel,
    #[default]
    Other,
}

impl ExposureSetting {
    fn label(self) -> &'static str {
        match self {
            ExposureSetting::Household => "household",
            ExposureSetting::Workplace => "workplace",
            ExposureSetting::School => "school",
            ExposureSetting::ChildCare => "child care",
            ExposureSetting::CongregateLiving => "congregate living",
            ExposureSetting::MedicalProcedure => "medical procedure",
            ExposureSetting::HealthCare => "health care",
            ExposureSetting::Social => "social",
            ExposureSetting::Travel => "travel",
            ExposureSetting::Other => "other",
        }
    }
}

/// Age band of a group of contacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
pub enum AgeBand {
    #[serde(rename = "under_5")]
    Under5,
    #[serde(rename = "5_to_15")]
    From5To15,
    #[serde(rename = "16_plus")]
    From16,
    #[default]
    #[serde(rename = "unknown")]
    Unknown,
}

/// A group of contacts who share exposure facts (counts only, no names).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ContactGroup {
    /// household, workplace, school, child_care, congregate_living,
    /// medical_procedure, health_care, social, travel, or other.
    pub setting: ExposureSetting,
    /// Number of contacts in the group.
    pub count: u32,
    /// under_5, 5_to_15, 16_plus, or unknown.
    pub age_band: AgeBand,
    /// HIV infection or other immunosuppression (e.g., >15 mg/day prednisone for
    /// >4 weeks, chemotherapy, antirejection drugs, TNF-alpha antagonists).
    pub medical_risk: bool,
    /// Exposure exceeds the duration/environment limits your program sets for
    /// high-priority contacts.
    pub exceeds_high_priority_limits: bool,
    /// Exposure exceeds the limits your program sets for medium-priority contacts.
    pub exceeds_medium_priority_limits: bool,
    /// Last exposure to the index patient, YYYY-MM-DD (for the retest date).
    pub last_exposure_date: Option<String>,
    /// Documented previous positive TB test or TB disease.
    pub prior_positive_test: bool,
}

/// Arguments for `plan_tb_contact_investigation`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TbContactArgs {
    /// The index patient's clinical facts (no names or identifiers).
    pub index_case: IndexCase,
    /// Groups of contacts by setting, with counts and risk facts only.
    pub contact_groups: Vec<ContactGroup>,
    /// A CSV/XLSX contact list in /data (one row per contact). Priorities and
    /// due dates are written to /data/output; only counts are returned.
    pub contact_file: Option<String>,
    /// Sheet name for an XLSX contact list.
    pub sheet: Option<String>,
    /// Date contacts were listed, YYYY-MM-DD (default today); starts the time frames.
    pub listed_date: Option<String>,
    /// Write the tracking workbook to /data/output (default true; skipped with
    /// a note when no data folder is mounted).
    pub write_workbook: Option<bool>,
    /// Blank rows in the workbook (default 50, at most 500).
    pub template_rows: Option<u32>,
}

/// Plan for one group of contacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GroupPlan {
    /// 1-based position in `contact_groups`.
    pub group: usize,
    pub setting: ExposureSetting,
    pub count: u32,
    /// Priority, or null when no investigation is recommended.
    pub priority: Option<Priority>,
    /// The rule that set the priority (or "none of the factors").
    pub decided_by: String,
    /// Initial encounter (test, symptom review) due by this date.
    pub initial_encounter_due: Option<String>,
    /// Business days from initial encounter to complete medical evaluation.
    pub evaluation_business_days: Option<u32>,
    /// Earliest date a test can exclude infection (8 weeks after last exposure).
    pub retest_window_opens: Option<String>,
    /// End of the 8-10 week window.
    pub retest_window_closes: Option<String>,
    /// What to do for this group.
    pub actions: Vec<String>,
}

/// Aggregated results of a contact file (no row-level data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContactFileSummary {
    pub file: String,
    pub rows_read: usize,
    pub contacts_classified: usize,
    pub rows_with_problems: usize,
    /// Contacts whose age or date of birth was present but could not be read
    /// (the age rules were not applied to them, so their priority may be too low).
    #[serde(default)]
    pub ages_not_read: usize,
    /// Contacts by priority ("*" = suppressed).
    pub by_priority: BTreeMap<String, String>,
    /// High-priority contacts with a recorded test result ("*" = suppressed).
    pub high_priority_tested: String,
    /// High-priority contacts with a positive test ("*" = suppressed).
    pub high_priority_positive: String,
    /// Percent positive among tested high-priority contacts, shown only when
    /// both counts are at or above the suppression threshold.
    pub high_priority_positive_pct: Option<f64>,
    /// Signals that the guidelines say warrant considering expansion.
    pub expansion_signals: Vec<String>,
    pub output_file: String,
    pub suppression_threshold: u32,
}

/// DPH TB program contact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TbProgramContact {
    pub division: String,
    pub address: String,
    pub phone_business_hours: String,
    pub phone_after_hours: String,
    pub fax: String,
}

/// A state-supported TB clinic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClinicOut {
    #[serde(flatten)]
    pub clinic: Clinic,
    pub county: Option<String>,
    /// In the configured jurisdiction's county.
    pub in_your_county: bool,
}

/// Result of `plan_tb_contact_investigation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TbContactPlan {
    /// Whether the guidelines recommend a contact investigation.
    pub investigation_recommended: bool,
    pub investigation_reason: String,
    pub case_type: Option<CaseType>,
    pub case_type_label: Option<String>,
    /// Infectious period start (YYYY-MM-DD) and the rule used.
    pub infectious_period_start: Option<String>,
    pub infectious_period_rule: Option<String>,
    /// Closed date if given, else null (still open).
    pub infectious_period_end: Option<String>,
    /// Earliest date the period could close (>2 weeks of effective treatment).
    pub earliest_possible_end: Option<String>,
    /// Ask about contacts exposed from the start through this date.
    pub exposure_window_through: Option<String>,
    pub end_criteria: Vec<CitedNote>,
    pub listed_date: String,
    pub contact_groups: Vec<GroupPlan>,
    pub contact_file: Option<ContactFileSummary>,
    pub testing: Vec<CitedNote>,
    pub special_groups: Vec<CitedNote>,
    pub expansion_triggers: Vec<String>,
    pub interview_settings: CitedNote,
    pub reporting: Vec<CitedNote>,
    pub dph_tb_program: TbProgramContact,
    pub tb_clinics: Vec<ClinicOut>,
    /// Workbook path relative to /data, when written.
    pub workbook: Option<String>,
}

fn months_before(d: NaiveDate, m: u32) -> NaiveDate {
    d.checked_sub_months(Months::new(m)).unwrap_or(d)
}

/// The index case type, or why no investigation is recommended.
fn case_type(ic: &IndexCase) -> std::result::Result<(CaseType, String), String> {
    if ic.site == DiseaseSite::ExtrapulmonaryOnly {
        return Err(
            "Extrapulmonary-only TB rarely transmits (exceptions: aerosol-generating procedures \
             such as autopsy, embalming, or irrigation of a draining abscess). A contact \
             investigation is generally not indicated; consider a source-case investigation for a \
             young child."
                .into(),
        );
    }
    let smear = ic.afb_smear_positive == Some(true);
    let cavity = ic.cavitary == Some(true);
    if smear || cavity {
        let why = if smear && ic.naa_positive == Some(false) {
            "AFB smear positive, but an approved NAA test was negative: the guidelines recommend \
             an investigation for a positive smear unless the NAA test is negative, so confirm \
             with the TB program."
                .to_string()
        } else {
            format!(
                "{} — investigation recommended (highest priority).",
                match (smear, cavity) {
                    (true, true) => "AFB smear positive with a cavity",
                    (true, false) => "AFB smear positive",
                    _ => "Cavity on chest radiograph",
                }
            )
        };
        return Ok((CaseType::SmearPositiveOrCavitary, why));
    }
    match ic.chest_radiograph {
        Some(CxrFinding::ConsistentWithTb) => Ok((
            CaseType::SmearNegativeCxrConsistent,
            "AFB smear negative with a chest radiograph consistent with TB — consider an \
             investigation (a positive NAA test supports starting one)."
                .into(),
        )),
        Some(CxrFinding::AbnormalNotConsistent) => Ok((
            CaseType::AbnormalCxrNotConsistent,
            "Suspected TB with an abnormal chest radiograph not consistent with TB — lower \
             priority; investigations around minimal findings are typically not started except \
             for vulnerable contacts or outbreaks."
                .into(),
        )),
        _ if ic.culture_positive == Some(true) || ic.naa_positive == Some(true) => Ok((
            CaseType::SmearNegativeCxrConsistent,
            "AFB smear negative but NAA- or culture-positive pulmonary TB — the tool uses the \
             smear-negative tree (Figure 3); confirm with the TB program."
                .into(),
        )),
        _ => Err(
            "Smear negative, no cavity, and no chest radiograph consistent with TB: investigations \
             typically should not be started for suspected TB with minimal findings, except in \
             outbreaks, for especially vulnerable contacts, or for source-case investigations. \
             Give the chest radiograph finding (and NAA/culture results) if known."
                .into(),
        ),
    }
}

struct Period {
    start: Option<NaiveDate>,
    rule: Option<String>,
    end: Option<NaiveDate>,
    earliest_end: Option<NaiveDate>,
    window_through: Option<NaiveDate>,
}

fn infectious_period(c: &TbContent, ic: &IndexCase, caveats: &mut Vec<String>) -> Result<Period> {
    let onset = practice::date_arg("symptom_onset_date", &ic.symptom_onset_date)?;
    let finding = practice::date_arg(
        "first_positive_finding_date",
        &ic.first_positive_finding_date,
    )?;
    let dx = practice::date_arg("suspected_diagnosis_date", &ic.suspected_diagnosis_date)?;
    let tx = practice::date_arg("treatment_start_date", &ic.treatment_start_date)?;
    let iso = practice::date_arg("isolation_start_date", &ic.isolation_start_date)?;
    let end = practice::date_arg("infectious_period_end_date", &ic.infectious_period_end_date)?;
    let rules = &c.contact_investigation.infectious_period.start_rules;
    let rule = |id: &str| rules.iter().find(|r| r.id == id);
    let smear_or_cavity = ic.afb_smear_positive == Some(true) || ic.cavitary == Some(true);
    // A symptom onset date means the patient has (or had) TB symptoms, even
    // when `symptomatic` was left at its default of false.
    let symptomatic = ic.symptomatic || onset.is_some();
    if !ic.symptomatic && onset.is_some() {
        caveats.push(
            "A symptom onset date was given, so the index patient was treated as symptomatic \
             (infectious period from 3 months before symptom onset or the first positive \
             finding, whichever is earlier), although symptomatic was not set to true. If the \
             patient never had TB symptoms, leave symptom_onset_date out."
                .into(),
        );
    }
    let (rule_id, anchor) = if symptomatic {
        let a = match (onset, finding) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        ("symptomatic", a)
    } else if smear_or_cavity {
        ("asymptomatic_smear_or_cavity", finding.or(dx))
    } else {
        ("asymptomatic_other", dx.or(finding))
    };
    let r = rule(rule_id).ok_or_else(|| PhnError::Internal(format!("rule {rule_id}")))?;
    let start = anchor.map(|a| match (r.months_before, r.weeks_before) {
        (Some(m), _) => months_before(a, m),
        (None, Some(w)) => a - chrono::Duration::weeks(i64::from(w)),
        _ => a,
    });
    if start.is_none() {
        caveats.push(format!(
            "The infectious period start could not be computed: give {}.",
            match rule_id {
                "symptomatic" => "the symptom onset date and/or the first positive finding date",
                "asymptomatic_smear_or_cavity" => "the first positive finding date",
                _ => "the suspected diagnosis date",
            }
        ));
    }
    if rule_id == "asymptomatic_smear_or_cavity" && finding.is_none() && dx.is_some() {
        caveats.push(
            "No first positive finding date was given, so the suspected diagnosis date was used \
             as the anchor."
                .into(),
        );
    }
    if let (Some(s), Some(e)) = (start, end)
        && e < s
    {
        return Err(PhnError::InvalidInput(
            "infectious_period_end_date is before the infectious period starts".into(),
        ));
    }
    let earliest_end = tx.map(|t| t + chrono::Duration::days(15));
    let window_through = match (end, iso) {
        (Some(e), Some(i)) => Some(e.min(i)),
        (e, i) => e.or(i),
    };
    Ok(Period {
        start,
        rule: Some(format!("{} ({})", r.start, r.section)),
        end,
        earliest_end,
        window_through,
    })
}

fn group_conditions(g: &ContactGroup) -> impl Fn(Condition) -> bool + '_ {
    move |c| match c {
        Condition::Household => g.setting == ExposureSetting::Household,
        Condition::AgeUnder5 => g.age_band == AgeBand::Under5,
        Condition::Age5To15 => g.age_band == AgeBand::From5To15,
        Condition::MedicalRisk => g.medical_risk,
        Condition::MedicalProcedure => g.setting == ExposureSetting::MedicalProcedure,
        Condition::CongregateSetting => g.setting == ExposureSetting::CongregateLiving,
        Condition::ExceedsHighPriorityLimits => g.exceeds_high_priority_limits,
        Condition::ExceedsMediumPriorityLimits => g.exceeds_medium_priority_limits,
    }
}

struct Schedule {
    encounter_due: Option<NaiveDate>,
    evaluation_days: Option<u32>,
    window_opens: Option<NaiveDate>,
    window_closes: Option<NaiveDate>,
    actions: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn schedule(
    c: &TbContent,
    ct: CaseType,
    p: Priority,
    listed: NaiveDate,
    last_exposure: Option<NaiveDate>,
    under_5: bool,
    medical_risk: bool,
    prior_positive: bool,
) -> Schedule {
    let ci = &c.contact_investigation;
    let tf = ci.timeframe(ct, p);
    let encounter_due = tf.map(|t| practice::add_business_days(listed, t.encounter_business_days));
    let wmin = ci.testing.window_weeks_min;
    let wmax = ci.testing.window_weeks_max;
    let window_opens = last_exposure.map(|d| d + chrono::Duration::weeks(wmin));
    let window_closes = last_exposure.map(|d| d + chrono::Duration::weeks(wmax));
    let mut actions = Vec::new();
    actions.push(
        "Symptom review at the first encounter; anyone with TB symptoms needs immediate \
         evaluation for TB disease."
            .to_string(),
    );
    if prior_positive {
        actions.push(
            "Documented previous positive test or TB disease: do not retest; evaluate for symptoms \
             and with a chest radiograph if being considered for treatment; immunocompromised \
             contacts get a full course of LTBI treatment after disease is excluded."
                .into(),
        );
        return Schedule {
            encounter_due,
            evaluation_days: tf.map(|t| t.evaluation_business_days),
            window_opens,
            window_closes,
            actions,
        };
    }
    let first_test_after_window = match (encounter_due, window_opens) {
        (Some(e), Some(w)) => e >= w,
        _ => false,
    };
    match p {
        Priority::High | Priority::Medium => {
            actions.push(format!(
                "Test (TST or IGRA) at the initial encounter{}; induration of {} mm or more is \
                 positive for a contact.",
                encounter_due
                    .map(|d| format!(
                        ", by {} ({} business days from listing)",
                        d,
                        tf.map(|t| t.encounter_business_days).unwrap_or(0)
                    ))
                    .unwrap_or_default(),
                ci.testing.positive_tst_mm
            ));
            if first_test_after_window {
                actions.push(
                    "The first test falls 8 or more weeks after the last exposure, so a negative \
                     result needs no repeat."
                        .into(),
                );
            } else {
                actions.push(match (window_opens, window_closes) {
                    (Some(o), Some(cl)) => format!(
                        "If the first test is negative, repeat it 8-10 weeks after the last \
                         exposure: {o} to {cl}."
                    ),
                    _ => "If the first test is negative and fewer than 8 weeks have passed since \
                          the last exposure, repeat it 8-10 weeks after the last exposure (give \
                          last_exposure_date for the dates)."
                        .into(),
                });
            }
            if let Some(t) = tf {
                actions.push(format!(
                    "Complete the medical evaluation within {} business days of the initial \
                     encounter (Table 3).",
                    t.evaluation_business_days
                ));
            }
        }
        Priority::Low => {
            actions.push(match (window_opens, window_closes) {
                (Some(o), Some(cl)) => format!(
                    "Low priority: if tested, a single test at the end of the window period is \
                     preferred: {o} to {cl}."
                ),
                _ => "Low priority: if tested, a single test after the window period (8-10 weeks \
                      after last exposure) is preferred."
                    .into(),
            });
            actions.push(
                "Test low-priority contacts only when objectives for high- and medium-priority \
                 contacts are being met or there is evidence of transmission."
                    .into(),
            );
        }
    }
    if under_5 {
        actions.push(
            "Children under 5: full evaluation including chest radiograph regardless of the \
             test; if the first test is negative and fewer than 8 weeks have passed, start window \
             prophylaxis after TB disease is excluded, retest at 8-10 weeks, and stop if negative."
                .into(),
        );
    }
    if medical_risk {
        actions.push(
            "HIV or immunosuppression: evaluate as for young children; a full course of LTBI \
             treatment is recommended after TB disease is excluded even if a test 8 or more weeks \
             after exposure is negative."
                .into(),
        );
    }
    actions.push(
        "Positive test: examination and chest radiograph; if TB disease is excluded, offer a full \
         course of LTBI treatment (see ltbi_treatment_calendar)."
            .into(),
    );
    Schedule {
        encounter_due,
        evaluation_days: tf.map(|t| t.evaluation_business_days),
        window_opens,
        window_closes,
        actions,
    }
}

fn xerr(e: XlsxError) -> PhnError {
    PhnError::Internal(format!("XLSX: {e}"))
}

fn col_letter(c: u16) -> String {
    let mut n = u32::from(c) + 1;
    let mut s = String::new();
    while n > 0 {
        let r = ((n - 1) % 26) as u8;
        s.insert(0, (b'A' + r) as char);
        n = (n - 1) / 26;
    }
    s
}

// Workbook column layout (0-based).
const C_SETTING: u16 = 3;
const C_AGE: u16 = 4;
const C_HOUSEHOLD: u16 = 5;
const C_MEDRISK: u16 = 6;
const C_PROCEDURE: u16 = 7;
const C_CONGREGATE: u16 = 8;
const C_EXCEEDS_HIGH: u16 = 9;
const C_EXCEEDS_MED: u16 = 10;
const C_PRIORITY: u16 = 11;
const C_LISTED: u16 = 12;
const C_ENCOUNTER_DUE: u16 = 13;
const C_ENCOUNTER: u16 = 14;
const C_EVAL_DUE: u16 = 15;
const C_LAST_EXPOSURE: u16 = 16;
const C_WINDOW_OPENS: u16 = 17;
const C_WINDOW_CLOSES: u16 = 18;

const HEADERS: &[&str] = &[
    "Contact ID",
    "Name",
    "Phone",
    "Setting",
    "Age (years)",
    "Household (Y/N)",
    "HIV or immunosuppressed (Y/N)",
    "Exposed during bronchoscopy, sputum induction, or autopsy (Y/N)",
    "Exposed in congregate setting (Y/N)",
    "Exceeds high-priority duration limits (Y/N)",
    "Exceeds medium-priority duration limits (Y/N)",
    "Priority (computed)",
    "Date listed",
    "Initial encounter due (computed)",
    "Initial encounter date",
    "Medical evaluation due (computed)",
    "Last exposure date",
    "Retest window opens (8 weeks)",
    "Retest window closes (10 weeks)",
    "Test 1 date",
    "Test 1 type (TST/IGRA)",
    "Test 1 result",
    "Test 2 date",
    "Test 2 result",
    "Chest radiograph date",
    "Chest radiograph result",
    "Evaluation outcome",
    "LTBI regimen",
    "Treatment start",
    "Treatment completed",
    "Notes",
];

/// Excel formula for the priority tree of a case type, for row `r` (1-based);
/// `extra` = identifier columns beyond the default three.
fn priority_formula(c: &TbContent, ct: CaseType, r: u32, extra: u16) -> String {
    let yes = |col: u16| format!("UPPER(LEFT({}{r},1))=\"Y\"", col_letter(col + extra));
    let age = col_letter(C_AGE + extra);
    let cond = |k: Condition| match k {
        Condition::Household => yes(C_HOUSEHOLD),
        Condition::AgeUnder5 => format!("AND(ISNUMBER({age}{r}),{age}{r}<5)"),
        Condition::Age5To15 => format!("AND(ISNUMBER({age}{r}),{age}{r}>=5,{age}{r}<=15)"),
        Condition::MedicalRisk => yes(C_MEDRISK),
        Condition::MedicalProcedure => yes(C_PROCEDURE),
        Condition::CongregateSetting => yes(C_CONGREGATE),
        Condition::ExceedsHighPriorityLimits => yes(C_EXCEEDS_HIGH),
        Condition::ExceedsMediumPriorityLimits => yes(C_EXCEEDS_MED),
    };
    let tree = c.contact_investigation.tree(ct);
    let mut f = format!("\"{}\"", title(tree.otherwise));
    for rule in tree.rules.iter().rev() {
        f = format!(
            "IF({},\"{}\",{f})",
            cond(rule.condition),
            title(rule.priority)
        );
    }
    format!(
        "=IF(COUNTA({}{r}:{}{r})=0,\"\",{f})",
        col_letter(C_SETTING + extra),
        col_letter(C_EXCEEDS_MED + extra)
    )
}

fn title(p: Priority) -> &'static str {
    match p {
        Priority::High => "High",
        Priority::Medium => "Medium",
        Priority::Low => "Low",
    }
}

/// A contact row prepared from a local file (identifiers stay in this struct
/// and the workbook only).
struct FileContact {
    ids: Vec<String>,
    setting: String,
    age: Option<f64>,
    flags: [bool; 6],
    listed: NaiveDate,
    last_exposure: Option<NaiveDate>,
    test1: Option<String>,
    test2: Option<String>,
    outcome: Option<String>,
    priority: Priority,
}

struct WorkbookSpec<'a> {
    c: &'a TbContent,
    ct: CaseType,
    period: &'a [(String, String)],
    contacts: &'a [FileContact],
    id_headers: &'a [String],
    blank_rows: u32,
    review: String,
}

fn build_workbook(w: &WorkbookSpec<'_>) -> std::result::Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let head = Format::new().set_bold().set_text_wrap();
    let bold = Format::new().set_bold();
    let date = Format::new().set_num_format("yyyy-mm-dd");
    let wrap = Format::new().set_text_wrap();
    let tree = w.c.contact_investigation.tree(w.ct);
    let tf_high = w.c.contact_investigation.timeframe(w.ct, Priority::High);
    let tf_med = w.c.contact_investigation.timeframe(w.ct, Priority::Medium);
    let enc = |p: Option<&tb_content::Timeframe>| p.map(|t| t.encounter_business_days).unwrap_or(0);
    let ev = |p: Option<&tb_content::Timeframe>| p.map(|t| t.evaluation_business_days).unwrap_or(0);
    // Identifier columns from a contact file replace Contact ID/Name/Phone;
    // every later column shifts right by `extra`.
    let n_ids = w.id_headers.len().max(3) as u16;
    let extra = n_ids - 3;
    let x = |c: u16| c + extra;

    let ws = wb.add_worksheet();
    ws.set_name("Contacts")?;
    ws.set_header("&LTB contact investigation&RConfidential: keep in /data/output");
    ws.set_footer("&LPage &P of &N&R&D");
    let mut headers: Vec<String> = Vec::new();
    if w.id_headers.is_empty() {
        headers.extend(HEADERS[..3].iter().map(|s| s.to_string()));
    } else {
        headers.extend(w.id_headers.iter().cloned());
        while headers.len() < 3 {
            headers.push(HEADERS[headers.len()].to_string());
        }
    }
    headers.extend(HEADERS[3..].iter().map(|s| s.to_string()));
    for (i, h) in headers.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, h, &head)?;
        ws.set_column_width(i as u16, if (i as u16) < n_ids { 18 } else { 14 })?;
    }
    for col in [
        C_LISTED,
        C_ENCOUNTER_DUE,
        C_ENCOUNTER,
        C_EVAL_DUE,
        C_LAST_EXPOSURE,
        C_WINDOW_OPENS,
        C_WINDOW_CLOSES,
        19,
        22,
        24,
        28,
        29,
    ] {
        ws.set_column_format(x(col), &date)?;
    }
    let total = w.contacts.len() as u32 + w.blank_rows;
    let yn = DataValidation::new().allow_list_strings(&["Y", "N"])?;
    ws.add_data_validation(1, x(C_HOUSEHOLD), total.max(1), x(C_EXCEEDS_MED), &yn)?;
    let to_xl = |d: NaiveDate| {
        use chrono::Datelike;
        rust_xlsxwriter::ExcelDateTime::from_ymd(d.year() as u16, d.month() as u8, d.day() as u8)
    };
    for r in 1..=total {
        let row1 = r + 1; // Excel row number
        let l = |c: u16| format!("{}{row1}", col_letter(x(c)));
        let mut pf = Formula::new(priority_formula(w.c, w.ct, row1, extra));
        if let Some(fc) = w.contacts.get((r - 1) as usize) {
            for (i, v) in fc.ids.iter().enumerate() {
                ws.write_string(r, i as u16, v)?;
            }
            ws.write_string(r, x(C_SETTING), &fc.setting)?;
            if let Some(a) = fc.age {
                ws.write_number(r, x(C_AGE), a)?;
            }
            for (i, f) in fc.flags.iter().enumerate() {
                ws.write_string(r, x(C_HOUSEHOLD) + i as u16, if *f { "Y" } else { "N" })?;
            }
            ws.write_date_with_format(r, x(C_LISTED), &to_xl(fc.listed)?, &date)?;
            if let Some(d) = fc.last_exposure {
                ws.write_date_with_format(r, x(C_LAST_EXPOSURE), &to_xl(d)?, &date)?;
            }
            if let Some(t) = &fc.test1 {
                ws.write_string(r, x(21), t)?;
            }
            if let Some(t) = &fc.test2 {
                ws.write_string(r, x(23), t)?;
            }
            if let Some(t) = &fc.outcome {
                ws.write_string(r, x(26), t)?;
            }
            pf = pf.set_result(title(fc.priority));
        }
        ws.write_formula(r, x(C_PRIORITY), pf)?;
        let p = l(C_PRIORITY);
        ws.write_formula_with_format(
            r,
            x(C_ENCOUNTER_DUE),
            Formula::new(format!(
                "=IF(OR({ls}=\"\",{p}=\"\",{p}=\"Low\"),\"\",IF({p}=\"High\",WORKDAY({ls},{}),WORKDAY({ls},{})))",
                enc(tf_high),
                enc(tf_med),
                ls = l(C_LISTED)
            )),
            &date,
        )?;
        ws.write_formula_with_format(
            r,
            x(C_EVAL_DUE),
            Formula::new(format!(
                "=IF(OR({e}=\"\",{p}=\"\",{p}=\"Low\"),\"\",IF({p}=\"High\",WORKDAY({e},{}),WORKDAY({e},{})))",
                ev(tf_high),
                ev(tf_med),
                e = l(C_ENCOUNTER)
            )),
            &date,
        )?;
        let le = l(C_LAST_EXPOSURE);
        ws.write_formula_with_format(
            r,
            x(C_WINDOW_OPENS),
            Formula::new(format!("=IF({le}=\"\",\"\",{le}+56)")),
            &date,
        )?;
        ws.write_formula_with_format(
            r,
            x(C_WINDOW_CLOSES),
            Formula::new(format!("=IF({le}=\"\",\"\",{le}+70)")),
            &date,
        )?;
    }
    ws.set_freeze_panes(1, n_ids.min(2))?;

    // Sheet 2: index case form (filled in locally) and the rules used.
    let ws = wb.add_worksheet();
    ws.set_name("Index case and rules")?;
    ws.set_column_width(0, 42)?;
    ws.set_column_width(1, 90)?;
    let mut row = 0;
    ws.write_string_with_format(row, 0, "Infectious period and window", &bold)?;
    row += 1;
    for (k, v) in w.period {
        ws.write_string(row, 0, k)?;
        ws.write_string_with_format(row, 1, v, &wrap)?;
        row += 1;
    }
    row += 1;
    ws.write_string_with_format(row, 0, format!("Priority rules ({})", tree.section), &bold)?;
    ws.write_string_with_format(row, 1, &tree.label, &wrap)?;
    row += 1;
    for rule in &tree.rules {
        ws.write_string(
            row,
            0,
            w.c.contact_investigation.condition_label(rule.condition),
        )?;
        ws.write_string(row, 1, format!("{} priority", title(rule.priority)))?;
        row += 1;
    }
    ws.write_string(row, 0, "None of the above")?;
    ws.write_string(row, 1, format!("{} priority", title(tree.otherwise)))?;
    row += 2;
    ws.write_string_with_format(
        row,
        0,
        "Minimal data about the index patient (2005 guidelines, Table 4)",
        &bold,
    )?;
    ws.write_string_with_format(row, 1, "Enter locally; never paste into the chat", &bold)?;
    row += 1;
    for g in &w.c.contact_investigation.index_case_data.groups {
        ws.write_string_with_format(row, 0, &g.heading, &bold)?;
        row += 1;
        for it in &g.items {
            ws.write_string(row, 0, it)?;
            row += 1;
        }
    }
    row += 1;
    ws.write_string_with_format(row, 0, "Review status", &bold)?;
    ws.write_string_with_format(row, 1, &w.review, &wrap)?;
    row += 1;
    ws.write_string_with_format(row, 0, "Privacy", &bold)?;
    ws.write_string_with_format(
        row,
        1,
        "This workbook holds identifiable information once filled in. Keep it in the output \
         folder or your agency's secure storage; do not paste its contents into the chat.",
        &wrap,
    )?;
    wb.save_to_buffer()
}

/// A date cell from a contact file. Years outside 1900-2100 (for example a
/// two-digit year misread as the year 26) are treated as unreadable.
fn file_date(cell: &str) -> Option<NaiveDate> {
    use chrono::Datelike;
    phn_local::phi::parse_date(cell).filter(|d| (1900..=2100).contains(&d.year()))
}

/// Age in years from an Age cell: a plain number of years, or a number with
/// a unit ("8 months", "18 mo", "6 wk", "3 years").
fn parse_age_years(cell: &str) -> Option<f64> {
    let s = cell.trim().to_ascii_lowercase();
    if let Ok(x) = s.parse::<f64>() {
        return Some(x).filter(|x| x.is_finite() && *x >= 0.0);
    }
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let n: f64 = s[..split].parse().ok()?;
    let unit = s[split..].trim();
    let per_year = if unit.starts_with('y') {
        1.0
    } else if unit.starts_with('m') {
        12.0
    } else if unit.starts_with('w') {
        52.18
    } else if unit.starts_with('d') {
        365.25
    } else {
        return None;
    };
    Some(n / per_year)
}

fn parse_result(cell: &str) -> Option<bool> {
    let s = cell.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("pos") || s == "+" || s == "reactive" {
        return Some(true);
    }
    if s.starts_with("neg") || s == "-" || s == "nonreactive" {
        return Some(false);
    }
    // A number is a TST induration in mm.
    s.trim_end_matches("mm")
        .trim()
        .parse::<f64>()
        .ok()
        .map(|mm| mm >= 5.0)
}

struct FileOutcome {
    summary: ContactFileSummary,
    contacts: Vec<FileContact>,
    id_headers: Vec<String>,
    identifier_kinds: Vec<String>,
}

fn read_contact_file(
    ctx: &Context,
    c: &TbContent,
    ct: CaseType,
    path: &str,
    sheet: Option<&str>,
    listed_default: NaiveDate,
) -> Result<FileOutcome> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let t = &lt.table;
    let col = |names: &[&str]| practice::find_col(t, names);
    let age_c = col(&["age", "age_years", "age_yrs"]);
    let dob_c = col(&["dob", "date_of_birth", "birth_date"]);
    let setting_c = col(&[
        "setting",
        "exposure_setting",
        "relationship",
        "contact_type",
    ]);
    let hh_c = col(&["household", "household_contact"]);
    let mr_c = col(&[
        "medical_risk",
        "immunocompromised",
        "hiv",
        "hiv_or_immunosuppressed",
    ]);
    let proc_c = col(&["medical_procedure", "procedure_exposure"]);
    let cong_c = col(&["congregate", "congregate_setting"]);
    let eh_c = col(&["exceeds_high_limits", "exceeds_high_priority_limits"]);
    let em_c = col(&["exceeds_medium_limits", "exceeds_medium_priority_limits"]);
    let le_c = col(&["last_exposure", "last_exposure_date"]);
    let listed_c = col(&["date_listed", "listed_date", "listed"]);
    let t1_c = col(&["test1_result", "test_1_result", "tst1", "igra1", "test1"]);
    let t2_c = col(&["test2_result", "test_2_result", "tst2", "igra2", "test2"]);
    let out_c = col(&["outcome", "evaluation_outcome", "tb_disease", "diagnosis"]);
    let prior_c = col(&["prior_positive", "previous_positive", "prior_positive_test"]);
    if age_c.is_none() && dob_c.is_none() && hh_c.is_none() && setting_c.is_none() {
        let usable: Vec<&str> = [
            "Age",
            "Setting",
            "Household",
            "Medical Risk",
            "Medical Procedure",
            "Congregate",
            "Exceeds High Limits",
            "Exceeds Medium Limits",
            "Last Exposure",
            "Date Listed",
            "Test1 Result",
            "Test2 Result",
            "Outcome",
            "Prior Positive",
        ]
        .to_vec();
        return Err(PhnError::InvalidInput(format!(
            "the contact list needs at least an Age (or DOB), Household, or Setting column. \
             Recognized columns: {}",
            usable.join(", ")
        )));
    }
    let inputs: Vec<usize> = [
        age_c, dob_c, setting_c, hh_c, mr_c, proc_c, cong_c, eh_c, em_c, le_c, listed_c, t1_c,
        t2_c, out_c, prior_c,
    ]
    .into_iter()
    .flatten()
    .collect();
    let id_cols = practice::identifier_columns(t, &lt.findings, &inputs);
    let id_headers: Vec<String> = id_cols.iter().map(|&i| t.headers[i].clone()).collect();
    let tree = c.contact_investigation.tree(ct);
    let mut contacts = Vec::new();
    let mut problems = 0usize;
    let mut ages_not_read = 0usize;
    let mut tested_high = 0u64;
    let mut positive_high = 0u64;
    let mut signals: Vec<String> = Vec::new();
    for row in &t.rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let cell = |i: Option<usize>| {
            i.and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let listed = file_date(&cell(listed_c)).unwrap_or(listed_default);
        let age = parse_age_years(&cell(age_c)).or_else(|| {
            file_date(&cell(dob_c))
                .map(|d| (listed - d).num_days() as f64 / 365.25)
                .filter(|a| *a >= 0.0)
                .map(|a| a.floor())
        });
        // An age or birth date that is present but unreadable must not pass
        // silently: the under-5 and 5-15 rules cannot be applied to that row.
        if age.is_none() && (!cell(age_c).is_empty() || !cell(dob_c).is_empty()) {
            ages_not_read += 1;
        }
        let setting = cell(setting_c);
        let sn = phn_core::text::normalize(&setting);
        let household = practice::yes(&cell(hh_c)) || sn.contains("household");
        let medical_risk = practice::yes(&cell(mr_c));
        let procedure = practice::yes(&cell(proc_c))
            || sn.contains("bronchoscopy")
            || sn.contains("autopsy")
            || sn.contains("sputum induction");
        let congregate = practice::yes(&cell(cong_c))
            || [
                "shelter",
                "correction",
                "jail",
                "prison",
                "nursing home",
                "congregate",
            ]
            .iter()
            .any(|k| sn.contains(k));
        let eh = practice::yes(&cell(eh_c));
        let em = practice::yes(&cell(em_c));
        if age.is_none() && hh_c.is_none() && setting.is_empty() {
            problems += 1;
            continue;
        }
        let has = |k: Condition| match k {
            Condition::Household => household,
            Condition::AgeUnder5 => age.is_some_and(|a| a < 5.0),
            Condition::Age5To15 => age.is_some_and(|a| (5.0..=15.0).contains(&a)),
            Condition::MedicalRisk => medical_risk,
            Condition::MedicalProcedure => procedure,
            Condition::CongregateSetting => congregate,
            Condition::ExceedsHighPriorityLimits => eh,
            Condition::ExceedsMediumPriorityLimits => em,
        };
        let (priority, _) = tree.classify(has);
        let t1 = parse_result(&cell(t1_c));
        let t2 = parse_result(&cell(t2_c));
        let outcome = cell(out_c);
        let disease = {
            let o = outcome.to_ascii_lowercase();
            let says_disease = (o.contains("disease") || o.contains("active"))
                && !o.contains("no ")
                && !o.starts_with("not");
            let disease_column =
                out_c.is_some_and(|i| phn_core::text::normalize(&t.headers[i]).contains("disease"));
            says_disease || (disease_column && practice::yes(&outcome))
        };
        let positive = t1 == Some(true) || t2 == Some(true);
        if priority == Priority::High && (t1.is_some() || t2.is_some()) {
            tested_high += 1;
            if positive {
                positive_high += 1;
            }
        }
        if age.is_some_and(|a| a < 5.0) && positive {
            push_once(&mut signals, "Infection in a contact under 5 years old");
        }
        if t1 == Some(false) && t2 == Some(true) {
            push_once(
                &mut signals,
                "A contact's test changed from negative to positive between the first and second test",
            );
        }
        if priority == Priority::Low && disease {
            push_once(
                &mut signals,
                "TB disease in a contact assigned low priority",
            );
        }
        if disease && priority != Priority::Low {
            push_once(
                &mut signals,
                "TB disease in a contact (possible secondary case): review for secondary transmission",
            );
        }
        contacts.push(FileContact {
            ids: id_cols
                .iter()
                .map(|&i| row.get(i).cloned().unwrap_or_default())
                .collect(),
            setting,
            age,
            flags: [household, medical_risk, procedure, congregate, eh, em],
            listed,
            last_exposure: file_date(&cell(le_c)),
            test1: (!cell(t1_c).is_empty()).then(|| cell(t1_c)),
            test2: (!cell(t2_c).is_empty()).then(|| cell(t2_c)),
            outcome: (!outcome.is_empty()).then_some(outcome),
            priority,
        });
    }
    let threshold = ctx.suppression_threshold();
    let counts: Vec<(String, u64)> = [Priority::High, Priority::Medium, Priority::Low]
        .iter()
        .map(|p| {
            (
                p.label().to_string(),
                contacts.iter().filter(|c| c.priority == *p).count() as u64,
            )
        })
        .collect();
    let (shown, _) = practice::suppressed_counts(&counts, threshold);
    let small = |n: u64| {
        if n > 0 && n < u64::from(threshold) {
            "*".to_string()
        } else {
            n.to_string()
        }
    };
    let pct = (tested_high >= u64::from(threshold) && positive_high >= u64::from(threshold))
        .then(|| (positive_high as f64 / tested_high as f64 * 1000.0).round() / 10.0);
    if tested_high > 0 && positive_high as f64 / tested_high as f64 >= 0.10 {
        push_once(
            &mut signals,
            "10% or more of tested high-priority contacts are positive (compare with the expected \
             rate in a similar population; the guideline threshold is 10% or twice that rate, \
             whichever is greater)",
        );
    }
    let mut kinds = lt.identifier_categories();
    kinds.dedup();
    Ok(FileOutcome {
        summary: ContactFileSummary {
            file: lt.label.clone(),
            rows_read: t.rows.len(),
            contacts_classified: contacts.len(),
            rows_with_problems: problems,
            ages_not_read,
            by_priority: shown.into_iter().collect(),
            high_priority_tested: small(tested_high),
            high_priority_positive: small(positive_high),
            high_priority_positive_pct: pct,
            expansion_signals: signals,
            output_file: String::new(),
            suppression_threshold: threshold,
        },
        contacts,
        id_headers,
        identifier_kinds: kinds,
    })
}

fn push_once(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

fn clinics(ctx: &Context, c: &TbContent) -> Vec<ClinicOut> {
    let home_county = ctx
        .settings()
        .jurisdiction
        .town
        .and_then(|t| ctx.ma.town(&t).map(|m| m.county.clone()));
    let mut out: Vec<ClinicOut> = c
        .ma_program
        .clinics
        .iter()
        .map(|cl| {
            let county = ctx.ma.town(&cl.town).map(|m| m.county.clone());
            ClinicOut {
                in_your_county: county.is_some() && county == home_county,
                county,
                clinic: cl.clone(),
            }
        })
        .collect();
    out.sort_by_key(|o| !o.in_your_county);
    out
}

/// Plan a TB contact investigation.
pub fn plan_tb_contact_investigation(
    ctx: &Context,
    args: TbContactArgs,
) -> Result<Envelope<TbContactPlan>> {
    let c = tb_content::content();
    let ci = &c.contact_investigation;
    let ic = &args.index_case;
    let today = chrono::Local::now().date_naive();
    let listed = practice::date_arg("listed_date", &args.listed_date)?.unwrap_or(today);
    let rows = args.template_rows.unwrap_or(50);
    if rows > 500 {
        return Err(PhnError::InvalidInput(
            "template_rows must be at most 500".into(),
        ));
    }
    if args.contact_groups.len() > 50 {
        return Err(PhnError::InvalidInput(
            "give at most 50 contact groups (use contact_file for longer lists)".into(),
        ));
    }
    let mut caveats: Vec<String> = practice::review_caveat(&c.meta).into_iter().collect();
    let period = infectious_period(c, ic, &mut caveats)?;
    let decision = case_type(ic);
    let (investigate, reason, ct) = match &decision {
        Ok((t, why)) => (true, why.clone(), Some(*t)),
        Err(why) => (false, why.clone(), None),
    };
    if ic.age_years.is_some_and(|a| a < 10) {
        caveats.push(
            "Transmission from children under 10 is unusual; investigations around pediatric \
             cases are usually source-case investigations (finding who infected the child)."
                .into(),
        );
    }
    if ic.mdr_tb {
        caveats.push(
            "Multidrug-resistant TB can extend infectiousness when treatment is ineffective; the \
             LTBI regimens in ltbi_treatment_calendar do not apply to contacts of MDR TB. Consult \
             DPH."
                .into(),
        );
    }

    // ---- Contact groups
    let mut groups = Vec::new();
    for (i, g) in args.contact_groups.iter().enumerate() {
        let last = practice::date_arg(
            &format!("contact_groups[{}].last_exposure_date", i + 1),
            &g.last_exposure_date,
        )?;
        let (priority, decided_by, sched) = match ct {
            Some(t) => {
                let (p, why) = ci.tree(t).classify(group_conditions(g));
                let s = schedule(
                    c,
                    t,
                    p,
                    listed,
                    last,
                    g.age_band == AgeBand::Under5,
                    g.medical_risk,
                    g.prior_positive_test,
                );
                (
                    Some(p),
                    why.map(|k| ci.condition_label(k))
                        .unwrap_or_else(|| "none of the factors".into()),
                    Some(s),
                )
            }
            None => (None, "no investigation recommended".into(), None),
        };
        if g.age_band == AgeBand::Unknown && ct.is_some() {
            push_once(
                &mut caveats,
                "Some groups have an unknown age band: children under 5 (and 5-15 for smear-positive \
                 cases) change the priority, so split groups by age.",
            );
        }
        groups.push(GroupPlan {
            group: i + 1,
            setting: g.setting,
            count: g.count,
            priority,
            decided_by,
            initial_encounter_due: sched
                .as_ref()
                .and_then(|s| s.encounter_due)
                .map(|d| d.to_string()),
            evaluation_business_days: sched.as_ref().and_then(|s| s.evaluation_days),
            retest_window_opens: sched
                .as_ref()
                .and_then(|s| s.window_opens)
                .map(|d| d.to_string()),
            retest_window_closes: sched
                .as_ref()
                .and_then(|s| s.window_closes)
                .map(|d| d.to_string()),
            actions: sched.map(|s| s.actions).unwrap_or_default(),
        });
    }

    // ---- Period rows (for the summary and the workbook)
    let mut period_rows: Vec<(String, String)> = Vec::new();
    period_rows.push((
        "Case type".into(),
        ct.map(|t| ci.tree(t).label.clone())
            .unwrap_or_else(|| "No investigation recommended".into()),
    ));
    if let (Some(s), Some(r)) = (period.start, &period.rule) {
        period_rows.push(("Infectious period starts".into(), format!("{s} — {r}")));
    }
    period_rows.push((
        "Infectious period ends".into(),
        match (period.end, period.earliest_end) {
            (Some(e), _) => e.to_string(),
            (None, Some(e)) => format!(
                "open; earliest possible {e} (more than 2 weeks of effective treatment), when \
                 symptoms have diminished and smears are improving"
            ),
            (None, None) => "open (give treatment_start_date or infectious_period_end_date)".into(),
        },
    ));
    if let (Some(s), Some(w)) = (period.start, period.window_through) {
        period_rows.push((
            "Ask about contacts exposed".into(),
            format!("{s} through {w}"),
        ));
    } else if let Some(s) = period.start {
        period_rows.push((
            "Ask about contacts exposed".into(),
            format!("{s} through the present (period still open)"),
        ));
    }
    period_rows.push(("Contacts listed".into(), listed.to_string()));

    // ---- Contact file
    let mut file_summary = None;
    let mut file_contacts: Vec<FileContact> = Vec::new();
    let mut id_headers = Vec::new();
    let mut identifiers_detected = Vec::new();
    if let Some(path) = args
        .contact_file
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        let Some(t) = ct else {
            return Err(PhnError::InvalidInput(format!(
                "no contact priorities apply: {reason}"
            )));
        };
        let out = read_contact_file(ctx, c, t, path, args.sheet.as_deref(), listed)?;
        if out.summary.ages_not_read > 0 {
            caveats.push(format!(
                "{} contact(s) have an age or date of birth that could not be read, so the \
                 under-5 and 5-15 priority rules were not applied to them and their priority may \
                 be too low. Enter age in whole years or as \"8 months\", or a date of birth with \
                 a four-digit year, and run the tool again.",
                out.summary.ages_not_read
            ));
        }
        file_contacts = out.contacts;
        id_headers = out.id_headers;
        identifiers_detected = out.identifier_kinds;
        file_summary = Some(out.summary);
    }

    // ---- Workbook
    let mut workbook = None;
    let mut files_written = Vec::new();
    let want = args.write_workbook.unwrap_or(true) || file_summary.is_some();
    if want && let Some(t) = ct {
        match ctx.sandbox() {
            Ok(sb) => {
                let bytes = build_workbook(&WorkbookSpec {
                    c,
                    ct: t,
                    period: &period_rows,
                    contacts: &file_contacts,
                    id_headers: &id_headers,
                    blank_rows: if file_summary.is_some() { 20 } else { rows },
                    review: practice::review_caveat(&c.meta).unwrap_or_default(),
                })
                .map_err(xerr)?;
                let path =
                    sb.new_output_file(&format!("tb_contact_investigation_{listed}.xlsx"))?;
                std::fs::write(&path, bytes)?;
                let shown = sb.display_path(&path);
                files_written.push(shown.clone());
                if let Some(fs) = file_summary.as_mut() {
                    fs.output_file = shown.clone();
                }
                workbook = Some(shown);
            }
            Err(e) if args.write_workbook.is_none() && file_summary.is_none() => {
                caveats.push(format!(
                    "The tracking workbook was not written because no data folder is mounted ({e})."
                ));
            }
            Err(e) => return Err(e),
        }
    }

    // ---- Markdown
    let mut s = String::from("**TB contact investigation plan**\n\n");
    let verdict = if investigate { "Yes." } else { "Not usually." };
    s.push_str(&format!("**Investigate?** {verdict} {reason}\n\n"));
    let pr: Vec<Vec<String>> = period_rows
        .iter()
        .map(|(k, v)| vec![k.clone(), v.clone()])
        .collect();
    s.push_str(&md::table(&["", ""], &pr));
    if let Some(t) = ct {
        let tree = ci.tree(t);
        let rule_text: Vec<String> = tree
            .rules
            .iter()
            .map(|r| {
                format!(
                    "{} → {}",
                    ci.condition_label(r.condition),
                    r.priority.label()
                )
            })
            .collect();
        s.push_str(&format!(
            "\n**Priority rules ({}):** {}; otherwise {}.\n",
            tree.section,
            rule_text.join("; "),
            tree.otherwise.label()
        ));
    }
    if !groups.is_empty() {
        s.push_str("\n**Contact groups**\n\n");
        let rows: Vec<Vec<String>> = groups
            .iter()
            .map(|g| {
                vec![
                    g.group.to_string(),
                    g.setting.label().to_string(),
                    g.count.to_string(),
                    g.priority
                        .map(|p| p.label().to_string())
                        .unwrap_or_else(|| "—".into()),
                    g.decided_by.clone(),
                    g.initial_encounter_due
                        .clone()
                        .unwrap_or_else(|| "—".into()),
                    match (&g.retest_window_opens, &g.retest_window_closes) {
                        (Some(a), Some(b)) => format!("{a} to {b}"),
                        _ => "8-10 weeks after last exposure".into(),
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "#",
                "Setting",
                "Contacts",
                "Priority",
                "Because",
                "First encounter due",
                "Retest window",
            ],
            &rows,
        ));
    }
    if let Some(fs) = &file_summary {
        s.push_str(&format!(
            "\n**Contact list `{}`:** {} contact(s) classified{}. By priority: high {}, medium {}, \
             low {}. High-priority contacts tested: {}, positive: {}{}. Per-contact priorities and \
             due dates are in `{}` (not shown here).\n",
            fs.file,
            fs.contacts_classified,
            if fs.rows_with_problems > 0 {
                format!(
                    "; {} row(s) lacked age, household, and setting",
                    fs.rows_with_problems
                )
            } else {
                String::new()
            },
            fs.by_priority.get("high").cloned().unwrap_or_default(),
            fs.by_priority.get("medium").cloned().unwrap_or_default(),
            fs.by_priority.get("low").cloned().unwrap_or_default(),
            fs.high_priority_tested,
            fs.high_priority_positive,
            fs.high_priority_positive_pct
                .map(|p| format!(" ({p}%)"))
                .unwrap_or_default(),
            fs.output_file
        ));
        if fs.expansion_signals.is_empty() {
            s.push_str("No expansion signals in the recorded results.\n");
        } else {
            s.push_str("\n**Consider expanding the investigation:**\n");
            for sig in &fs.expansion_signals {
                s.push_str(&format!("- {sig}\n"));
            }
        }
    }
    s.push_str(&format!(
        "\n**Testing:** test high- and medium-priority contacts at the first encounter; a \
         negative test less than 8 weeks after the last exposure is repeated 8-10 weeks after it; \
         {} mm or more is positive for contacts. Children under 5 and immunocompromised contacts \
         get a full evaluation with chest radiograph and window prophylaxis.\n",
        ci.testing.positive_tst_mm
    ));
    s.push_str(&format!(
        "\n**DPH TB program:** {} — {} (business hours), {} (after hours). TB disease must be \
         reported immediately; positive LTBI tests are reported online.\n",
        c.ma_program.division, c.ma_program.phone_business_hours, c.ma_program.phone_after_hours
    ));
    let tb_clinics = clinics(ctx, c);
    let local: Vec<String> = tb_clinics
        .iter()
        .filter(|o| o.in_your_county)
        .map(|o| format!("{} ({}, {})", o.clinic.name, o.clinic.town, o.clinic.phone))
        .collect();
    if !local.is_empty() {
        s.push_str(&format!(
            "\n**State-supported TB clinics in your county:** {}\n",
            local.join("; ")
        ));
    }
    if let Some(w) = &workbook {
        s.push_str(&format!(
            "\n**Workbook:** `{w}` — one row per contact; enter the Y/N risk facts, age, date \
             listed, and last exposure and the priority, due dates, and retest window fill in \
             (WORKDAY counts Monday-Friday; add holidays yourself). Names and phone numbers stay \
             in that file.\n"
        ));
    }

    // ---- Caveats and sources
    caveats.push(
        "Priority assignments are approximations from imperfect information; reassess them as \
         results come in and review results at least weekly. Duration/environment limits are \
         set by your program (no national cut-off)."
            .into(),
    );
    caveats.push(
        "Business-day time frames skip weekends only; holidays are not known to the tool.".into(),
    );
    if ct.is_some() && args.contact_groups.is_empty() && file_summary.is_none() {
        caveats.push(
            "No contacts were given: list contact groups (counts and risk facts, no names) or \
             point contact_file at a local list to get priorities and dates."
                .into(),
        );
    }
    let mut keys = vec![
        "cdc-ci-2005",
        "dph-tb",
        "dph-tb-report",
        "dph-tb-outpatient",
    ];
    if ic.mdr_tb {
        keys.push("cdc-ltbi-2020");
    }
    let mut sources: Vec<Source> = practice::sources_for(&c.sources, keys);
    if let Some(fs) = &file_summary {
        sources.insert(0, Source::local_file(&fs.file));
    }
    let threshold = ctx.suppression_threshold();
    let mut privacy = Privacy {
        row_level_data: false,
        suppression_applied: file_summary.is_some(),
        suppression_threshold: file_summary.as_ref().map(|_| threshold),
        identifiers_detected,
        files_written,
        notes: vec![
            "Only clinical facts and counts are used; the workbook is blank or filled locally. Do \
             not paste names or contact details into the chat."
                .into(),
        ],
    };
    if file_summary.is_some() {
        privacy.notes.push(
            "Per-contact results were written to the output folder; only suppressed counts and \
             expansion signals are returned."
                .into(),
        );
    }
    let data = TbContactPlan {
        investigation_recommended: investigate,
        investigation_reason: reason,
        case_type: ct,
        case_type_label: ct.map(|t| ci.tree(t).label.clone()),
        infectious_period_start: period.start.map(|d| d.to_string()),
        infectious_period_rule: period.rule,
        infectious_period_end: period.end.map(|d| d.to_string()),
        earliest_possible_end: period.earliest_end.map(|d| d.to_string()),
        exposure_window_through: period.window_through.map(|d| d.to_string()),
        end_criteria: practice::notes(&ci.infectious_period.end_criteria, &c.sources)
            .into_iter()
            .chain([
                practice::note(&ci.infectious_period.stricter_end, &c.sources),
                practice::note(&ci.infectious_period.earlier_start_note, &c.sources),
            ])
            .collect(),
        listed_date: listed.to_string(),
        contact_groups: groups,
        contact_file: file_summary,
        testing: practice::notes(&ci.testing.notes, &c.sources)
            .into_iter()
            .chain([practice::note(&ci.timeframe_note, &c.sources)])
            .collect(),
        special_groups: ci
            .special_groups
            .iter()
            .map(|g| CitedNote {
                text: g.text.clone(),
                citation: practice::cite(&c.sources, &g.source, &g.section),
                confidence: None,
            })
            .collect(),
        expansion_triggers: std::iter::once(ci.expansion.intro.clone())
            .chain(ci.expansion.triggers.iter().cloned())
            .collect(),
        interview_settings: practice::note(&ci.interview_settings, &c.sources),
        reporting: practice::notes(&c.ma_program.reporting, &c.sources),
        dph_tb_program: TbProgramContact {
            division: c.ma_program.division.clone(),
            address: c.ma_program.address.clone(),
            phone_business_hours: c.ma_program.phone_business_hours.clone(),
            phone_after_hours: c.ma_program.phone_after_hours.clone(),
            fax: c.ma_program.fax.clone(),
        },
        tb_clinics,
        workbook,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy)
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

    fn smear_positive() -> IndexCase {
        IndexCase {
            symptomatic: true,
            afb_smear_positive: Some(true),
            cavitary: Some(true),
            chest_radiograph: Some(CxrFinding::ConsistentWithTb),
            symptom_onset_date: Some("2026-06-10".into()),
            first_positive_finding_date: Some("2026-08-01".into()),
            treatment_start_date: Some("2026-08-05".into()),
            ..IndexCase::default()
        }
    }

    #[test]
    fn infectious_period_starts_three_months_before_the_earlier_date() {
        let (_d, c) = ctx(false);
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: smear_positive(),
                listed_date: Some("2026-08-07".into()),
                write_workbook: Some(false),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert!(d.investigation_recommended);
        assert_eq!(d.case_type, Some(CaseType::SmearPositiveOrCavitary));
        // Onset June 10 is earlier than the first finding: start March 10.
        assert_eq!(d.infectious_period_start.as_deref(), Some("2026-03-10"));
        // More than 2 weeks of treatment from Aug 5.
        assert_eq!(d.earliest_possible_end.as_deref(), Some("2026-08-20"));
        assert!(d.workbook.is_none());
        assert!(env.notice.is_some());
    }

    #[test]
    fn asymptomatic_without_smear_or_cavity_uses_four_weeks_before_diagnosis() {
        let (_d, c) = ctx(false);
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: IndexCase {
                    chest_radiograph: Some(CxrFinding::ConsistentWithTb),
                    afb_smear_positive: Some(false),
                    suspected_diagnosis_date: Some("2026-09-30".into()),
                    ..IndexCase::default()
                },
                write_workbook: Some(false),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        assert_eq!(
            env.data.infectious_period_start.as_deref(),
            Some("2026-09-02")
        );
        assert_eq!(
            env.data.case_type,
            Some(CaseType::SmearNegativeCxrConsistent)
        );
    }

    #[test]
    fn groups_get_figure_priorities_time_frames_and_retest_windows() {
        let (_d, c) = ctx(false);
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: smear_positive(),
                listed_date: Some("2026-08-07".into()), // a Friday
                write_workbook: Some(false),
                contact_groups: vec![
                    ContactGroup {
                        setting: ExposureSetting::Household,
                        count: 3,
                        age_band: AgeBand::From16,
                        last_exposure_date: Some("2026-08-05".into()),
                        ..ContactGroup::default()
                    },
                    ContactGroup {
                        setting: ExposureSetting::School,
                        count: 25,
                        age_band: AgeBand::From5To15,
                        ..ContactGroup::default()
                    },
                    ContactGroup {
                        setting: ExposureSetting::Workplace,
                        count: 12,
                        age_band: AgeBand::From16,
                        ..ContactGroup::default()
                    },
                ],
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        let g = &env.data.contact_groups;
        assert_eq!(g[0].priority, Some(Priority::High));
        // 7 business days after Friday Aug 7 = Tuesday Aug 18.
        assert_eq!(g[0].initial_encounter_due.as_deref(), Some("2026-08-18"));
        assert_eq!(g[0].evaluation_business_days, Some(5));
        assert_eq!(g[0].retest_window_opens.as_deref(), Some("2026-09-30"));
        assert_eq!(g[0].retest_window_closes.as_deref(), Some("2026-10-14"));
        assert_eq!(g[1].priority, Some(Priority::Medium));
        assert_eq!(g[1].initial_encounter_due.as_deref(), Some("2026-08-27"));
        assert_eq!(g[2].priority, Some(Priority::Low));
        assert!(g[2].initial_encounter_due.is_none());
    }

    #[test]
    fn figure_3_household_is_medium_and_extrapulmonary_is_not_investigated() {
        let (_d, c) = ctx(false);
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: IndexCase {
                    symptomatic: true,
                    afb_smear_positive: Some(false),
                    chest_radiograph: Some(CxrFinding::ConsistentWithTb),
                    symptom_onset_date: Some("2026-05-01".into()),
                    ..IndexCase::default()
                },
                contact_groups: vec![ContactGroup {
                    setting: ExposureSetting::Household,
                    count: 2,
                    age_band: AgeBand::From16,
                    ..ContactGroup::default()
                }],
                write_workbook: Some(false),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.contact_groups[0].priority, Some(Priority::Medium));
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: IndexCase {
                    site: DiseaseSite::ExtrapulmonaryOnly,
                    ..IndexCase::default()
                },
                write_workbook: Some(false),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        assert!(!env.data.investigation_recommended);
    }

    #[test]
    fn workbook_has_priority_formulas() {
        let (dir, c) = ctx(true);
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: smear_positive(),
                template_rows: Some(5),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        let w = env.data.workbook.unwrap();
        let tables = phn_local::table::read_tables(&dir.path().join("data").join(&w)).unwrap();
        assert!(tables[0].headers.iter().any(|h| h == "Priority (computed)"));
        let f = priority_formula(
            tb_content::content(),
            CaseType::SmearPositiveOrCavitary,
            2,
            0,
        );
        assert!(f.starts_with("=IF(COUNTA(D2:K2)=0"));
        assert!(f.contains("\"Medium\""));
        assert_eq!(env.privacy.files_written.len(), 1);
    }

    #[test]
    fn symptom_onset_date_makes_the_case_symptomatic() {
        let (_d, c) = ctx(false);
        // `symptomatic` left out, but an onset date given: Table 2's
        // 3-months-before rule applies, not 4 weeks before diagnosis.
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: IndexCase {
                    chest_radiograph: Some(CxrFinding::ConsistentWithTb),
                    afb_smear_positive: Some(false),
                    symptom_onset_date: Some("2026-08-01".into()),
                    suspected_diagnosis_date: Some("2026-10-01".into()),
                    ..IndexCase::default()
                },
                write_workbook: Some(false),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        assert_eq!(
            env.data.infectious_period_start.as_deref(),
            Some("2026-05-01")
        );
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("treated as symptomatic"))
        );
    }

    #[test]
    fn ages_in_months_are_read_and_unreadable_ages_are_counted() {
        assert_eq!(parse_age_years("3"), Some(3.0));
        assert!(parse_age_years("8 months").is_some_and(|a| a < 1.0));
        assert!(parse_age_years("18 mo").is_some_and(|a| (1.4..1.6).contains(&a)));
        assert_eq!(parse_age_years("4 years"), Some(4.0));
        assert_eq!(parse_age_years("toddler"), None);
        assert_eq!(parse_age_years(""), None);
        // A two-digit year read as the year 21 is not a usable birth date.
        assert!(file_date("0021-03-05").is_none());
        assert!(file_date("2021-03-05").is_some());

        let (dir, c) = ctx(true);
        let mut csv = String::from("Contact ID,First Name,Last Name,Age,Household,Setting\n");
        for i in 0..6 {
            csv.push_str(&format!("K{i},Zebulon{i},Quixote,8 months,N,friend\n"));
        }
        csv.push_str("K9,Ysolde,Quixote,toddler,Y,home\n");
        std::fs::write(dir.path().join("data/contacts.csv"), csv).unwrap();
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: smear_positive(),
                contact_file: Some("contacts.csv".into()),
                listed_date: Some("2026-08-07".into()),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        let fs = env.data.contact_file.as_ref().unwrap();
        // Infants recorded in months are under 5: high priority, not low.
        assert_eq!(fs.by_priority.get("high").map(String::as_str), Some("7"));
        assert_eq!(fs.ages_not_read, 1);
        assert!(env.caveats.iter().any(|c| c.contains("could not be read")));
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("Zebulon") && !json.contains("toddler"));
    }

    #[test]
    fn contact_file_returns_counts_and_signals_only() {
        let (dir, c) = ctx(true);
        let mut csv = String::from(
            "Contact ID,First Name,Last Name,Phone,Age,Household,Setting,Last Exposure,Test1 Result,Test2 Result\n",
        );
        for i in 0..8 {
            csv.push_str(&format!(
                "K{i},Zebulon{i},Quixote,413-555-01{i:02},{},Y,home,2026-08-01,{},\n",
                30 + i,
                if i < 3 { "12 mm" } else { "0" }
            ));
        }
        csv.push_str("K9,Ysolde,Quixote,413-555-0199,3,Y,home,2026-08-01,neg,pos\n");
        std::fs::write(dir.path().join("data/contacts.csv"), csv).unwrap();
        let env = plan_tb_contact_investigation(
            &c,
            TbContactArgs {
                index_case: smear_positive(),
                contact_file: Some("contacts.csv".into()),
                listed_date: Some("2026-08-07".into()),
                ..TbContactArgs::default()
            },
        )
        .unwrap();
        let fs = env.data.contact_file.as_ref().unwrap();
        assert_eq!(fs.contacts_classified, 9);
        assert_eq!(fs.by_priority.get("high").map(String::as_str), Some("9"));
        assert!(fs.expansion_signals.iter().any(|s| s.contains("under 5")));
        assert!(
            fs.expansion_signals
                .iter()
                .any(|s| s.contains("negative to positive"))
        );
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("Zebulon") && !json.contains("413-555"));
        assert!(env.privacy.suppression_applied);
    }
}
