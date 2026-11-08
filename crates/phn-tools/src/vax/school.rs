//! VAX-04 `check_ma_school_immunization_requirements`.
//!
//! A rule engine for the Massachusetts school immunization requirements
//! (105 CMR 220.000 and DPH's annual requirements document). Doses are first
//! validated against CDC minimum ages and intervals (with the 4-day grace
//! period unless turned off, and the 28-day rule between different live
//! vaccines), then each requirement's options are checked. Proof of immunity
//! (laboratory evidence, varicella history, college birth-year rules) and
//! exemptions are applied in DPH's "count each child once" order: doses or
//! immunity first, then an exemption, then not met.
//!
//! Batch mode reads a roster in /data, writes per-student results to the
//! output folder, and returns only suppressed counts by grade and requirement.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::vax::content::{self, ReqRule, Requirement, SCHED, SCHOOL_REQS, Series, Special};
use crate::vax::dates::{self, Span};

/// An exemption on file.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ExemptionInput {
    /// Vaccine the exemption covers (e.g. "mmr", "varicella", "all").
    pub vaccine: String,
    /// "medical" or "religious".
    pub kind: String,
}

/// College enrollment details.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CollegeInput {
    /// Enrolled in a health science program (birth-year rules do not apply).
    pub health_science: bool,
    /// Full-time student (default true).
    pub full_time: Option<bool>,
    /// Born in the United States (needed for the pre-1957 MMR and pre-1980
    /// varicella rules).
    pub born_in_us: Option<bool>,
}

/// Arguments for `check_ma_school_immunization_requirements`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SchoolCheckArgs {
    /// Grade the student is entering: "PK" (child care/preschool, also K0/K1),
    /// "K", "1"-"12", "college", or "ungraded" (rules by age).
    pub grade: Option<String>,
    /// Date of birth (YYYY-MM-DD). Required for one student. Used only for the
    /// calculation and never repeated in the answer.
    pub date_of_birth: Option<String>,
    /// Date to evaluate on (default today), e.g. the first day of school.
    pub as_of: Option<String>,
    /// Dose dates by vaccine, e.g. {"dtap": ["2020-01-02", ...], "mmr": [...]}.
    /// Keys: dtap, dt, dtap_ipv (Kinrix/Quadracel), dtap_hepb_ipv (Pediarix),
    /// dtap_ipv_hib (Pentacel), dtap_ipv_hib_hepb (Vaxelis), polio/ipv, hepb,
    /// heplisav_b, hib, mmr, mmrv (ProQuad), varicella, tdap, td, menacwy,
    /// menabcwy (Penbraya/Penmenvy). MenB does not count for any requirement.
    pub doses: BTreeMap<String, Vec<String>>,
    /// Laboratory evidence of immunity on file: any of hepb, measles, mumps,
    /// rubella, varicella.
    pub lab_immunity: Vec<String>,
    /// A reliable history of chickenpox (diagnosis, or a parent's description
    /// interpreted by a physician, NP, PA, or designee).
    pub varicella_history: bool,
    /// Medical or religious exemptions on file.
    pub exemptions: Vec<ExemptionInput>,
    /// College students only.
    pub college: Option<CollegeInput>,
    /// Residential student in a school that combines preschool-grade 8 with
    /// grades 9-12 (meningococcal requirement applies).
    pub residential_combined_school: bool,
    /// Apply the CDC 4-day grace period (default true; see caveats).
    pub apply_grace_period: Option<bool>,
    /// Batch mode: a roster CSV/XLSX in /data (one row per student). Results per
    /// student go to /data/output; only counts by grade are returned.
    pub roster_path: Option<String>,
    /// Sheet name for an XLSX roster.
    pub sheet: Option<String>,
}

/// Status of one requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReqStatus {
    /// Requirement satisfied by doses or proof of immunity.
    Met,
    /// Not satisfied and no exemption on file.
    NotMet,
    /// Not yet required at the student's age (the date it becomes due is given).
    NotYetDue,
    /// Medical exemption on file (renew annually).
    ExemptMedical,
    /// Religious exemption on file.
    ExemptReligious,
    /// Does not apply to this student.
    NotApplicable,
    /// The rule cannot be decided from the information given; a person must review.
    NeedsReview,
}

impl ReqStatus {
    /// Label for tables.
    pub fn label(self) -> &'static str {
        match self {
            ReqStatus::Met => "met",
            ReqStatus::NotMet => "not met",
            ReqStatus::NotYetDue => "not yet due",
            ReqStatus::ExemptMedical => "medical exemption",
            ReqStatus::ExemptReligious => "religious exemption",
            ReqStatus::NotApplicable => "not applicable",
            ReqStatus::NeedsReview => "needs review",
        }
    }

    const ALL: [ReqStatus; 7] = [
        ReqStatus::Met,
        ReqStatus::NotMet,
        ReqStatus::NotYetDue,
        ReqStatus::ExemptMedical,
        ReqStatus::ExemptReligious,
        ReqStatus::NeedsReview,
        ReqStatus::NotApplicable,
    ];
}

/// A dose that does not count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InvalidDose {
    pub date: String,
    pub reason: String,
}

/// The result for one requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequirementResult {
    pub requirement_id: String,
    pub label: String,
    pub status: ReqStatus,
    /// Why, in plain language.
    pub reason: String,
    /// Number of valid doses counted.
    pub valid_doses: usize,
    pub invalid_doses: Vec<InvalidDose>,
    /// Earliest date the next needed dose could be given (minimum age and
    /// interval, without the grace period), when a dose is missing.
    pub earliest_next_dose: Option<String>,
    /// True when the next dose can be given on the evaluation date.
    pub eligible_now: bool,
    /// DPH requirement text.
    pub requirement_text: String,
}

/// A count in the batch summary (after suppression).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CountRow {
    pub grade: String,
    pub requirement: String,
    /// Counts by status; "<n" or "*" when suppressed.
    pub counts: BTreeMap<String, String>,
}

/// Batch-mode summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BatchSummary {
    pub file: String,
    pub students_read: usize,
    pub students_evaluated: usize,
    /// Rows that could not be evaluated (problems are listed in the output file).
    pub rows_with_problems: usize,
    pub counts: Vec<CountRow>,
    pub output_file: String,
    pub suppression_threshold: u32,
}

/// Result of `check_ma_school_immunization_requirements`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SchoolCheckResult {
    pub school_year: String,
    pub setting: String,
    pub evaluated_on: String,
    pub grace_period_applied: bool,
    /// One student's results (single mode).
    pub requirements: Vec<RequirementResult>,
    /// Batch summary (roster mode).
    pub batch: Option<BatchSummary>,
    pub exemption_rules: Vec<String>,
    pub notes: Vec<String>,
}

// ------------------------------------------------------------------ inputs

/// Antigen groups a dose contributes to.
fn antigens_for_key(key: &str) -> Option<&'static [&'static str]> {
    let k = phn_core::text::normalize(key).replace(' ', "_");
    Some(match k.as_str() {
        "dtap" | "dt" | "daptacel" | "infanrix" | "dtp" => &["dtap"],
        "dtap_ipv" | "kinrix" | "quadracel" => &["dtap", "polio"],
        "dtap_hepb_ipv" | "pediarix" => &["dtap", "hepb", "polio"],
        "dtap_ipv_hib" | "pentacel" => &["dtap", "polio", "hib"],
        "dtap_ipv_hib_hepb" | "vaxelis" => &["dtap", "polio", "hib", "hepb"],
        "ipv" | "polio" | "opv" | "ipol" => &["polio"],
        "hepb" | "hep_b" | "hepatitis_b" | "engerix" | "engerix_b" | "recombivax" | "recombivax_hb" => &["hepb"],
        "heplisav" | "heplisav_b" | "hepb_cpg" => &["heplisav"],
        "hib" | "acthib" | "hiberix" | "pedvaxhib" => &["hib"],
        "mmr" | "m_m_r_ii" | "mmr_ii" | "priorix" => &["mmr"],
        "mmrv" | "proquad" => &["mmr", "varicella", "mmrv"],
        "varicella" | "var" | "varivax" | "chickenpox" => &["varicella"],
        "tdap" | "adacel" | "boostrix" => &["tdap"],
        "td" | "tenivac" | "tdvax" => &["td"],
        "menacwy" | "mcv4" | "menveo" | "menquadfi" | "menactra" | "meningococcal" => &["menacwy"],
        "menabcwy" | "penbraya" | "penmenvy" => &["menacwy"],
        "menb" | "bexsero" | "trumenba" => &["menb"],
        _ => return None,
    })
}

/// Normalized dose history of one student.
#[derive(Debug, Clone, Default)]
struct History {
    /// antigen -> sorted dates
    by_antigen: BTreeMap<&'static str, Vec<NaiveDate>>,
    /// Live-vaccine events: (date, includes MMR, includes varicella).
    live: Vec<(NaiveDate, bool, bool)>,
    unknown_keys: Vec<String>,
    menb_given: bool,
}

fn build_history(
    doses: &BTreeMap<String, Vec<String>>,
    dob: NaiveDate,
    as_of: NaiveDate,
) -> Result<History> {
    let mut h = History::default();
    let mut live: BTreeMap<NaiveDate, (bool, bool)> = BTreeMap::new();
    for (key, dates_in) in doses {
        let Some(antigens) = antigens_for_key(key) else {
            h.unknown_keys.push(key.clone());
            continue;
        };
        for s in dates_in {
            let d = dates::require_date(&format!("{key} dose date"), s)?;
            if d < dob {
                return Err(PhnError::InvalidInput(format!(
                    "a {key} dose date is before the date of birth"
                )));
            }
            if d > as_of {
                return Err(PhnError::InvalidInput(format!(
                    "a {key} dose date is after the evaluation date ({as_of}); check the dates or set as_of"
                )));
            }
            for a in antigens.iter() {
                match *a {
                    "menb" => h.menb_given = true,
                    "mmrv" => {}
                    _ => h.by_antigen.entry(a).or_default().push(d),
                }
            }
            if antigens.contains(&"mmr") || antigens.contains(&"varicella") {
                let e = live.entry(d).or_default();
                e.0 |= antigens.contains(&"mmr");
                e.1 |= antigens.contains(&"varicella");
            }
        }
    }
    for v in h.by_antigen.values_mut() {
        v.sort();
    }
    h.live = live.into_iter().map(|(d, (m, v))| (d, m, v)).collect();
    Ok(h)
}

// ------------------------------------------------------------------ validation

struct Validated {
    valid: Vec<NaiveDate>,
    invalid: Vec<InvalidDose>,
    /// Most recent invalid dose (not counting same-day duplicates); a repeat
    /// dose is spaced from it by the minimum interval (CDC).
    last_invalid: Option<NaiveDate>,
}

fn span(s: &str) -> Span {
    Span::parse(s).unwrap_or_default()
}

/// Validate doses against a series' minimum ages and intervals.
fn validate(
    dob: NaiveDate,
    doses: &[NaiveDate],
    series: &Series,
    grace: bool,
    live_conflict: impl Fn(NaiveDate) -> Option<NaiveDate>,
) -> Validated {
    let mut valid: Vec<NaiveDate> = Vec::new();
    let mut invalid = Vec::new();
    for &d in doses {
        if valid.last() == Some(&d) {
            invalid.push(InvalidDose {
                date: d.to_string(),
                reason: "same date as another dose of this vaccine (counted once)".into(),
            });
            continue;
        }
        let n = valid.len();
        let rule = &series.doses[n.min(series.doses.len() - 1)];
        let min_age_date = dates::date_at_age(dob, span(&rule.min_age));
        if !dates::meets(d, min_age_date, grace) {
            invalid.push(InvalidDose {
                date: d.to_string(),
                reason: format!(
                    "given {} day(s) before the minimum age for dose {} ({})",
                    dates::days_early(d, min_age_date),
                    n + 1,
                    rule.min_age
                ),
            });
            continue;
        }
        if let (Some(prev), Some(iv)) = (valid.last(), &rule.min_interval) {
            let due = dates::add(*prev, span(iv));
            if !dates::meets(d, due, grace && !rule.no_grace_interval) {
                invalid.push(InvalidDose {
                    date: d.to_string(),
                    reason: format!(
                        "given {} day(s) before the minimum interval of {iv} after the previous dose",
                        dates::days_early(d, due)
                    ),
                });
                continue;
            }
        }
        if let (Some(first), Some(iv)) = (valid.first(), &rule.min_interval_from_first) {
            let due = dates::add(*first, span(iv));
            if !dates::meets(d, due, grace) {
                invalid.push(InvalidDose {
                    date: d.to_string(),
                    reason: format!("given before the minimum interval of {iv} after dose 1"),
                });
                continue;
            }
        }
        if let Some(other) = live_conflict(d) {
            invalid.push(InvalidDose {
                date: d.to_string(),
                reason: format!(
                    "given {} day(s) after a different live vaccine ({other}); live vaccines not given the same day must be at least 28 days apart",
                    (d - other).num_days()
                ),
            });
            continue;
        }
        valid.push(d);
    }
    let last_invalid = invalid
        .iter()
        .filter(|x: &&InvalidDose| !x.reason.starts_with("same date"))
        .filter_map(|x| dates::parse_date(&x.date))
        .max()
        .filter(|li| valid.last().is_none_or(|v| li > v));
    Validated {
        valid,
        invalid,
        last_invalid,
    }
}

/// Earliest date for dose number `n + 1` of a series (planning values, no
/// grace). A dose that did not count pushes the repeat out by the minimum
/// interval from that invalid dose (CDC General Best Practice Guidelines).
fn next_dose_date(
    dob: NaiveDate,
    valid: &[NaiveDate],
    last_invalid: Option<NaiveDate>,
    series: &Series,
) -> NaiveDate {
    let n = valid.len();
    let rule = &series.doses[n.min(series.doses.len() - 1)];
    let mut earliest = dates::date_at_age(
        dob,
        span(rule.plan_min_age.as_deref().unwrap_or(&rule.min_age)),
    );
    if let Some(prev) = valid.last()
        && let Some(iv) = rule.plan_min_interval.as_ref().or(rule.min_interval.as_ref())
    {
        earliest = earliest.max(dates::add(*prev, span(iv)));
    }
    if let (Some(first), Some(iv)) = (valid.first(), &rule.min_interval_from_first) {
        earliest = earliest.max(dates::add(*first, span(iv)));
    }
    if let Some(inv) = last_invalid {
        let iv = rule
            .plan_min_interval
            .as_ref()
            .or(rule.min_interval.as_ref())
            .map(|s| span(s))
            .unwrap_or(Span::weeks(4));
        earliest = earliest.max(dates::add(inv, iv));
    }
    earliest
}

// ------------------------------------------------------------------ student evaluation

/// Everything needed to evaluate one student.
#[derive(Debug, Clone)]
pub struct Student {
    pub grade: String,
    pub dob: NaiveDate,
    pub doses: BTreeMap<String, Vec<String>>,
    pub lab_immunity: BTreeSet<String>,
    pub varicella_history: bool,
    /// vaccine -> "medical" | "religious"
    pub exemptions: BTreeMap<String, String>,
    pub college: Option<CollegeInput>,
    pub residential_combined_school: bool,
}

/// Which DPH setting applies to a grade label (and age, for ungraded classes).
pub fn setting_for(grade: &str, age_years: i32) -> Result<(&'static str, String)> {
    let g = grade.trim().to_ascii_uppercase();
    let g = g.trim_start_matches("GRADE").trim().to_string();
    let label = |s: &str| s.to_string();
    Ok(match g.as_str() {
        "PK" | "PRE-K" | "PREK" | "K0" | "K1" | "PRESCHOOL" | "CHILDCARE" | "CHILD CARE" | "DAYCARE" => {
            ("childcare_preschool", label("PK"))
        }
        "K" | "K2" | "KG" | "KINDERGARTEN" => ("k_6", label("K")),
        "1" | "2" | "3" | "4" | "5" | "6" | "01" | "02" | "03" | "04" | "05" | "06" => {
            ("k_6", g.trim_start_matches('0').to_string())
        }
        "7" | "8" | "9" | "10" | "11" | "12" | "07" | "08" | "09" => ("grades_7_12", g.trim_start_matches('0').to_string()),
        "COLLEGE" | "UNIVERSITY" | "POSTSECONDARY" | "13" => ("college", label("college")),
        "UNGRADED" | "U" | "SP" | "SPED" => {
            if age_years >= 16 {
                ("grades_7_12", label("11"))
            } else if age_years >= 12 {
                ("grades_7_12", label("7"))
            } else if age_years >= 5 {
                ("k_6", label("K"))
            } else {
                ("childcare_preschool", label("PK"))
            }
        }
        _ => {
            return Err(PhnError::InvalidInput(format!(
                "grade '{grade}' is not recognized; use PK, K, 1-12, college, or ungraded"
            )));
        }
    })
}

struct Eval<'a> {
    st: &'a Student,
    hist: History,
    as_of: NaiveDate,
    grace: bool,
}

impl Eval<'_> {
    fn age(&self) -> i32 {
        dates::age_years(self.st.dob, self.as_of)
    }

    fn live_conflict_for(&self, antigen: &str) -> impl Fn(NaiveDate) -> Option<NaiveDate> + '_ {
        let want_mmr = antigen == "mmr";
        let want_var = antigen == "varicella";
        move |d: NaiveDate| {
            if !(want_mmr || want_var) {
                return None;
            }
            self.hist
                .live
                .iter()
                .filter(|(e, m, v)| {
                    *e < d && (d - *e).num_days() < 28 && {
                        // A different live vaccine: for an MMR dose, an event with
                        // varicella only; for a varicella dose, MMR only.
                        if want_mmr { *v && !*m } else { *m && !*v }
                    }
                })
                .map(|(e, _, _)| *e)
                .next_back()
        }
    }

    fn validated(&self, antigen: &str, series_id: &str) -> Option<(Validated, &'static Series)> {
        let series = SCHED.series(series_id)?;
        let doses = self.hist.by_antigen.get(antigen).cloned().unwrap_or_default();
        let conflict = self.live_conflict_for(antigen);
        Some((validate(self.st.dob, &doses, series, self.grace, conflict), series))
    }

    fn result(&self, req: &Requirement, status: ReqStatus, reason: String) -> RequirementResult {
        RequirementResult {
            requirement_id: req.id.clone(),
            label: req.label.clone(),
            status,
            reason,
            valid_doses: 0,
            invalid_doses: Vec::new(),
            earliest_next_dose: None,
            eligible_now: false,
            requirement_text: req.text.clone(),
        }
    }

    fn with_next(&self, mut r: RequirementResult, next: NaiveDate) -> RequirementResult {
        let next = next.max(self.st.dob);
        r.eligible_now = next <= self.as_of;
        r.earliest_next_dose = Some(next.to_string());
        r
    }

    fn exemption(&self, vaccine: &str) -> Option<ReqStatus> {
        let keys = [vaccine.to_string(), "all".to_string()];
        keys.iter().find_map(|k| {
            self.st.exemptions.get(k).map(|kind| {
                if kind.starts_with("med") {
                    ReqStatus::ExemptMedical
                } else {
                    ReqStatus::ExemptReligious
                }
            })
        })
    }

    /// Apply proof of immunity, then exemptions, to a "not met" result.
    fn finish(&self, req: &Requirement, mut r: RequirementResult) -> RequirementResult {
        if r.status != ReqStatus::NotMet {
            return r;
        }
        let rule = &req.rule;
        if !rule.lab_evidence.is_empty() {
            let have: Vec<&String> = rule
                .lab_evidence
                .iter()
                .filter(|a| self.st.lab_immunity.contains(*a))
                .collect();
            if have.len() == rule.lab_evidence.len() {
                r.status = ReqStatus::Met;
                r.reason = "laboratory evidence of immunity on file".into();
                r.earliest_next_dose = None;
                r.eligible_now = false;
                return r;
            } else if !have.is_empty() {
                r.status = ReqStatus::NeedsReview;
                r.reason = format!(
                    "{}; laboratory evidence is on file only for {} (evidence for {} is needed)",
                    r.reason,
                    have.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                    rule.lab_evidence.join(", ")
                );
                return r;
            }
        }
        if rule.history && self.st.varicella_history {
            r.status = ReqStatus::Met;
            r.reason = "reliable history of chickenpox on file".into();
            r.earliest_next_dose = None;
            r.eligible_now = false;
            return r;
        }
        if let (Some(year), Some(c)) = (rule.us_born_before_year, &self.st.college) {
            use chrono::Datelike;
            if !c.health_science && c.born_in_us == Some(true) && self.st.dob.year() < year {
                r.status = ReqStatus::Met;
                r.reason = format!("born in the U.S. before {year} (acceptable for non-health-science students)");
                r.earliest_next_dose = None;
                r.eligible_now = false;
                return r;
            }
        }
        if let Some(ex) = self.exemption(&req.vaccine) {
            r.status = ex;
            r.reason = format!("{} ({})", ex.label(), r.reason);
        }
        r
    }

    fn evaluate(&self, req: &Requirement) -> RequirementResult {
        let rule = &req.rule;
        let r = match rule.special {
            Some(s) => self.special(req, s),
            None => self.series_rule(req, rule),
        };
        self.finish(req, r)
    }

    fn series_rule(&self, req: &Requirement, rule: &ReqRule) -> RequirementResult {
        let series_id = rule.series.as_deref().unwrap_or(&req.vaccine);
        let antigen = req.vaccine.as_str();
        let Some((v, series)) = self.validated(antigen, series_id) else {
            return self.result(req, ReqStatus::NeedsReview, "rule not available".into());
        };
        // Heplisav-B two-dose route (grades 7-12 and college).
        if rule.heplisav_two_dose
            && let Some(hep) = self.hist.by_antigen.get("heplisav")
            && let Some(hs) = SCHED.series("hepb_heplisav")
        {
            let hv = validate(self.st.dob, hep, hs, self.grace, |_| None);
            if hv.valid.len() >= 2 {
                let mut r = self.result(req, ReqStatus::Met, "2 valid doses of Heplisav-B given at 18 or older".into());
                r.valid_doses = 2;
                r.invalid_doses = hv.invalid;
                return r;
            }
        }
        let mut best: Option<(usize, String, Option<&content::RuleOption>)> = None;
        for opt in &rule.options {
            let have = v.valid.len();
            if have >= opt.min_doses {
                let last = *v.valid.last().expect("at least one dose");
                let mut fail = Vec::new();
                if let Some(a) = &opt.last_dose_min_age {
                    let due = dates::date_at_age(self.st.dob, span(a));
                    if !dates::meets(last, due, self.grace) {
                        fail.push(format!("the last dose was given before age {a}"));
                    }
                }
                if let Some(iv) = &opt.last_dose_min_interval
                    && v.valid.len() >= 2
                {
                    let prev = v.valid[v.valid.len() - 2];
                    let due = dates::add(prev, span(iv));
                    if !dates::meets(last, due, self.grace) {
                        fail.push(format!("the last dose was less than {iv} after the previous dose"));
                    }
                }
                if fail.is_empty() {
                    let mut r = self.result(
                        req,
                        ReqStatus::Met,
                        format!("{have} valid dose(s) meet the requirement"),
                    );
                    r.valid_doses = have;
                    r.invalid_doses = v.invalid;
                    return r;
                }
                let remaining = 1;
                if best.as_ref().is_none_or(|b| remaining < b.0) {
                    best = Some((remaining, fail.join("; "), Some(opt)));
                }
            } else {
                let remaining = opt.min_doses - have;
                if best.as_ref().is_none_or(|b| remaining < b.0) {
                    best = Some((
                        remaining,
                        format!("{have} valid dose(s); {} needed", opt.min_doses),
                        Some(opt),
                    ));
                }
            }
        }
        let (remaining, why, opt) = best.unwrap_or((1, "no rule options".into(), None));
        let mut next = next_dose_date(self.st.dob, &v.valid, v.last_invalid, series);
        if remaining == 1
            && let Some(o) = opt
        {
            if let Some(a) = &o.last_dose_min_age {
                next = next.max(dates::date_at_age(self.st.dob, span(a)));
            }
            if let (Some(iv), Some(prev)) = (&o.last_dose_min_interval, v.valid.last()) {
                next = next.max(dates::add(*prev, span(iv)));
            }
        }
        let mut r = self.result(
            req,
            ReqStatus::NotMet,
            format!("{why}; {remaining} more dose(s) needed"),
        );
        r.valid_doses = v.valid.len();
        r.invalid_doses = v.invalid;
        self.with_next(r, next)
    }

    fn men_doses(&self) -> Option<(Validated, &'static Series)> {
        self.validated("menacwy", "menacwy")
    }

    fn special(&self, req: &Requirement, s: Special) -> RequirementResult {
        let dob = self.st.dob;
        match s {
            Special::Hib => self.hib(req),
            Special::Tdap => self.tdap(req),
            Special::Men710 => {
                let Some((v, _)) = self.men_doses() else {
                    return self.result(req, ReqStatus::NeedsReview, "rule not available".into());
                };
                let tenth = dates::date_at_age(dob, Span::years(10));
                let ok: Vec<&NaiveDate> = v.valid.iter().filter(|d| dates::meets(**d, tenth, self.grace)).collect();
                let mut r = if ok.is_empty() {
                    let mut r = self.result(
                        req,
                        ReqStatus::NotMet,
                        if v.valid.is_empty() {
                            "no MenACWY or MenABCWY dose on record".into()
                        } else {
                            "no MenACWY dose on or after the 10th birthday".into()
                        },
                    );
                    r = self.with_next(r, tenth.max(v.valid.last().map(|d| dates::add(*d, Span::weeks(8))).unwrap_or(tenth)));
                    r
                } else {
                    self.result(req, ReqStatus::Met, "a MenACWY dose was given on or after the 10th birthday".into())
                };
                r.valid_doses = ok.len();
                r.invalid_doses = v.invalid;
                if self.hist.menb_given {
                    r.reason.push_str(" (MenB doses do not count)");
                }
                r
            }
            Special::Men1112 => {
                let Some((v, _)) = self.men_doses() else {
                    return self.result(req, ReqStatus::NeedsReview, "rule not available".into());
                };
                let sixteenth = dates::date_at_age(dob, Span::years(16));
                let ok = v.valid.iter().filter(|d| dates::meets(**d, sixteenth, self.grace)).count();
                let mut r = if ok > 0 {
                    self.result(req, ReqStatus::Met, "a MenACWY dose was given on or after the 16th birthday".into())
                } else if self.age() < 16 {
                    let mut r = self.result(
                        req,
                        ReqStatus::NotYetDue,
                        "students who are 15 in grade 11 are in compliance until they turn 16; a dose on or after the 16th birthday will be needed".into(),
                    );
                    r.earliest_next_dose = Some(
                        sixteenth
                            .max(v.valid.last().map(|d| dates::add(*d, Span::weeks(8))).unwrap_or(sixteenth))
                            .to_string(),
                    );
                    r
                } else {
                    let r = self.result(
                        req,
                        ReqStatus::NotMet,
                        "no valid MenACWY dose on or after the 16th birthday".into(),
                    );
                    self.with_next(r, sixteenth.max(v.valid.last().map(|d| dates::add(*d, Span::weeks(8))).unwrap_or(sixteenth)))
                };
                r.valid_doses = v.valid.len();
                r.invalid_doses = v.invalid;
                r
            }
            Special::MenCollege => {
                let full_time = self.st.college.as_ref().and_then(|c| c.full_time).unwrap_or(true);
                if !full_time || self.age() > 21 {
                    return self.result(
                        req,
                        ReqStatus::NotApplicable,
                        "the MenACWY requirement applies to full-time students 21 or younger".into(),
                    );
                }
                let Some((v, _)) = self.men_doses() else {
                    return self.result(req, ReqStatus::NeedsReview, "rule not available".into());
                };
                let sixteenth = dates::date_at_age(dob, Span::years(16));
                let ok = v.valid.iter().filter(|d| dates::meets(**d, sixteenth, self.grace)).count();
                let mut r = if ok > 0 {
                    self.result(req, ReqStatus::Met, "a MenACWY dose was given on or after the 16th birthday".into())
                } else {
                    let r = self.result(
                        req,
                        ReqStatus::NotMet,
                        "no MenACWY dose on or after the 16th birthday (the student may instead sign the MDPH meningococcal information and waiver form)".into(),
                    );
                    self.with_next(r, sixteenth.max(v.valid.last().map(|d| dates::add(*d, Span::weeks(8))).unwrap_or(sixteenth)))
                };
                r.valid_doses = ok;
                r.invalid_doses = v.invalid;
                r
            }
            Special::ResidentialMeningococcal => {
                if !self.st.residential_combined_school {
                    return self.result(
                        req,
                        ReqStatus::NotApplicable,
                        "applies only to residential students in schools that combine preschool-grade 8 with grades 9-12".into(),
                    );
                }
                self.result(
                    req,
                    ReqStatus::NeedsReview,
                    "residential student in a combined school: DPH applies the grade 7-10 and 11-12 meningococcal requirements; confirm with DPH how they apply at this age".into(),
                )
            }
        }
    }

    fn hib(&self, req: &Requirement) -> RequirementResult {
        let dob = self.st.dob;
        let Some((v, series)) = self.validated("hib", "hib") else {
            return self.result(req, ReqStatus::NeedsReview, "rule not available".into());
        };
        let at = |m: u32| dates::date_at_age(dob, Span::months(m));
        let after_15m = v.valid.iter().any(|d| dates::meets(*d, at(15), self.grace));
        let after_12m: Vec<&NaiveDate> = v.valid.iter().filter(|d| dates::meets(**d, at(12), self.grace)).collect();
        let n = v.valid.len();
        let (status, reason) = if after_15m {
            (ReqStatus::Met, "a valid Hib dose was given at 15 months or older".to_string())
        } else if after_12m.len() >= 2 {
            (ReqStatus::Met, "two valid Hib doses at 12 months or older".to_string())
        } else if n >= 4 && !after_12m.is_empty() {
            (ReqStatus::Met, "4-dose series with a booster at 12 months or older".to_string())
        } else if n >= 3 && !after_12m.is_empty() {
            (
                ReqStatus::NeedsReview,
                "3 doses with a booster at 12 months or older: complete if PedvaxHIB (PRP-OMP) was used for the primary doses; a 4th dose is needed for other products".to_string(),
            )
        } else {
            (ReqStatus::NotMet, format!("{n} valid Hib dose(s); a dose at 12 months or older is needed (number of doses depends on product and age at start)"))
        };
        let mut r = self.result(req, status, reason);
        r.valid_doses = n;
        r.invalid_doses = v.invalid;
        if status == ReqStatus::NotMet {
            if self.age() >= 5 {
                r.status = ReqStatus::NeedsReview;
                r.reason.push_str(
                    "; CDC catch-up guidance does not routinely recommend Hib at 5 years or older - confirm with DPH whether this requirement applies",
                );
                return r;
            }
            let next = next_dose_date(dob, &v.valid, v.last_invalid, series).max(at(12));
            return self.with_next(r, next);
        }
        r
    }

    fn tdap(&self, req: &Requirement) -> RequirementResult {
        let dob = self.st.dob;
        let Some((v, series)) = self.validated("tdap", "tdap") else {
            return self.result(req, ReqStatus::NeedsReview, "rule not available".into());
        };
        let tetanus_doses: usize = ["dtap", "tdap", "td"]
            .iter()
            .map(|a| self.hist.by_antigen.get(a).map(Vec::len).unwrap_or(0))
            .sum();
        if v.valid.is_empty() {
            let r = self.result(req, ReqStatus::NotMet, "no Tdap dose at 7 years or older on record".into());
            return self.with_next(r, next_dose_date(dob, &[], v.last_invalid, series));
        }
        let mut r = if tetanus_doses < 3 {
            self.result(
                req,
                ReqStatus::NeedsReview,
                format!(
                    "a Tdap dose at 7 or older is on record, but only {tetanus_doses} tetanus-containing dose(s) in total; confirm a DTaP primary series or age-appropriate catch-up"
                ),
            )
        } else {
            self.result(req, ReqStatus::Met, "a Tdap dose at 7 years or older and a primary series on record".into())
        };
        r.valid_doses = v.valid.len();
        r.invalid_doses = v.invalid;
        if let Some(last) = v.valid.last()
            && dates::add(*last, Span::years(10)) <= self.as_of
        {
            r.reason.push_str("; it has been 10 or more years since the last Tdap: Td or Tdap should be given");
        }
        r
    }
}

/// Evaluate one student; returns the setting id and the results.
pub fn evaluate_student(
    st: &Student,
    as_of: NaiveDate,
    grace: bool,
) -> Result<(&'static str, String, Vec<RequirementResult>, Vec<String>)> {
    let age = dates::age_years(st.dob, as_of);
    if st.dob > as_of {
        return Err(PhnError::InvalidInput("date of birth is after the evaluation date".into()));
    }
    let (setting_id, grade) = setting_for(&st.grade, age)?;
    let setting = SCHOOL_REQS
        .settings
        .iter()
        .find(|s| s.id == setting_id)
        .ok_or_else(|| PhnError::Internal(format!("setting {setting_id} missing")))?;
    let hist = build_history(&st.doses, st.dob, as_of)?;
    let mut notes = Vec::new();
    if !hist.unknown_keys.is_empty() {
        notes.push(format!(
            "Not used (not a Massachusetts school requirement or not recognized): {}",
            hist.unknown_keys.join(", ")
        ));
    }
    if hist.menb_given {
        notes.push("MenB (Bexsero, Trumenba) does not meet the meningococcal requirement.".into());
    }
    let ev = Eval { st, hist, as_of, grace };
    let under_min_age = setting
        .min_age_years
        .is_some_and(|m| age < m as i32);
    let mut out = Vec::new();
    for req in &setting.requirements {
        if !req.grades.is_empty() && !req.grades.iter().any(|g| g.eq_ignore_ascii_case(&grade)) {
            continue;
        }
        if req.rule.special == Some(Special::ResidentialMeningococcal) && !st.residential_combined_school {
            continue;
        }
        if under_min_age {
            let mut r = ev.result(
                req,
                ReqStatus::NotYetDue,
                "attendees under 2 years should be immunized for their age according to the 2026 AAP schedule; these requirements apply from age 2".into(),
            );
            r.earliest_next_dose = Some(dates::date_at_age(st.dob, Span::years(2)).to_string());
            out.push(r);
            continue;
        }
        let r = ev.evaluate(req);
        if r.status == ReqStatus::NotApplicable && req.rule.special == Some(Special::ResidentialMeningococcal) {
            continue;
        }
        out.push(r);
    }
    Ok((setting_id, grade, out, notes))
}

fn setting_label(id: &str) -> String {
    SCHOOL_REQS
        .settings
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.label.clone())
        .unwrap_or_else(|| id.to_string())
}

fn exemption_rules() -> Vec<String> {
    let e = &SCHOOL_REQS.exemptions;
    vec![
        format!("Medical: {}", e.medical.text),
        format!("Religious: {}", e.religious.text),
        format!("Meningococcal waiver: {}", e.meningococcal_waiver.text),
        e.outbreak_exclusion.clone(),
    ]
}

fn base_caveats(grace: bool) -> Vec<String> {
    let mut c = vec![
        format!(
            "Enforcement is local: address questions about exclusion with your district's legal counsel. {}",
            SCHOOL_REQS
                .general
                .iter()
                .find(|g| g.id == "homeless_foster")
                .map(|g| g.text.clone())
                .unwrap_or_default()
        ),
        if grace {
            format!("{} (applied; set apply_grace_period to false to check without it).", SCHOOL_REQS.grace_period.text)
        } else {
            "The 4-day grace period was NOT applied, at your request.".to_string()
        },
        "Earliest next-dose dates use CDC minimum ages and intervals without the grace period; they \
         are the earliest a dose would count, not a catch-up schedule. Verify against the MIIS \
         forecast before scheduling."
            .into(),
    ];
    for f in &SCHOOL_REQS.flags {
        c.push(f.text.clone());
    }
    c.extend(content::review_caveats(&[content::SCHOOL, content::SCHEDULE]));
    c
}

fn sources() -> Result<Vec<Source>> {
    let mut s = vec![content::content_source(content::SCHOOL)?];
    if let Ok(m) = content::meta(content::SCHOOL) {
        for r in m.related_sources.iter().take(2) {
            s.push(Source {
                title: r.title.clone(),
                publisher: r.publisher.clone(),
                url: r.url.clone(),
                effective_date: None,
                retrieved: Some(m.prepared.clone()),
                mode: phn_core::SourceMode::Snapshot,
            });
        }
    }
    s.push(content::content_source(content::SCHEDULE)?);
    Ok(s)
}

/// Check one student's record (or a roster) against Massachusetts school requirements.
pub fn check_ma_school_immunization_requirements(
    ctx: &Context,
    args: SchoolCheckArgs,
) -> Result<Envelope<SchoolCheckResult>> {
    let as_of = match &args.as_of {
        Some(s) => dates::require_date("as_of", s)?,
        None => chrono::Local::now().date_naive(),
    };
    let grace = args
        .apply_grace_period
        .unwrap_or(SCHOOL_REQS.grace_period.default_applies);
    if let Some(path) = &args.roster_path {
        return batch(ctx, path, args.sheet.as_deref(), as_of, grace);
    }
    let grade = args.grade.clone().ok_or_else(|| {
        PhnError::InvalidInput("grade is required (PK, K, 1-12, college, or ungraded)".into())
    })?;
    let dob = dates::require_date(
        "date_of_birth",
        args.date_of_birth
            .as_deref()
            .ok_or_else(|| PhnError::InvalidInput("date_of_birth is required".into()))?,
    )?;
    let st = Student {
        grade,
        dob,
        doses: args.doses.clone(),
        lab_immunity: args.lab_immunity.iter().map(|s| s.trim().to_ascii_lowercase()).collect(),
        varicella_history: args.varicella_history,
        exemptions: args
            .exemptions
            .iter()
            .map(|e| (normalize_vaccine_key(&e.vaccine), e.kind.trim().to_ascii_lowercase()))
            .collect(),
        college: args.college.clone(),
        residential_combined_school: args.residential_combined_school,
    };
    for e in &args.exemptions {
        let k = e.kind.trim().to_ascii_lowercase();
        if !(k.starts_with("med") || k.starts_with("rel")) {
            return Err(PhnError::InvalidInput(format!(
                "exemption kind '{}' must be \"medical\" or \"religious\"",
                e.kind
            )));
        }
    }
    let (setting_id, grade_label, results, mut notes) = evaluate_student(&st, as_of, grace)?;
    if st.exemptions.values().any(|k| k.starts_with("med")) {
        notes.push(SCHOOL_REQS.exemptions.medical.renewal.clone().replace("must", "Medical exemptions must"));
    }

    let mut summary = format!(
        "**Massachusetts school immunization check — {} (grade {grade_label}), {} school year**\n\nEvaluated on {as_of}.\n\n",
        setting_label(setting_id),
        SCHOOL_REQS.school_year
    );
    let rows: Vec<Vec<String>> = results
        .iter()
        .map(|r| {
            vec![
                r.label.clone(),
                r.status.label().to_string(),
                r.reason.clone(),
                match (&r.earliest_next_dose, r.eligible_now) {
                    (Some(_), true) => "now".into(),
                    (Some(d), false) => d.clone(),
                    _ => "—".into(),
                },
            ]
        })
        .collect();
    summary.push_str(&md::table(&["Requirement", "Status", "Why", "Earliest next dose"], &rows));
    let invalid: Vec<String> = results
        .iter()
        .flat_map(|r| r.invalid_doses.iter().map(move |d| format!("{} {}: {}", r.label, d.date, d.reason)))
        .collect();
    if !invalid.is_empty() {
        summary.push_str("\n**Doses that do not count**\n");
        for i in &invalid {
            summary.push_str(&format!("- {i}\n"));
        }
    }
    for n in &notes {
        summary.push_str(&format!("\n{n}\n"));
    }
    Ok(Envelope::new(
        summary,
        SchoolCheckResult {
            school_year: SCHOOL_REQS.school_year.clone(),
            setting: setting_id.to_string(),
            evaluated_on: as_of.to_string(),
            grace_period_applied: grace,
            requirements: results,
            batch: None,
            exemption_rules: exemption_rules(),
            notes,
        },
    )
    .sources(sources()?)
    .caveats(base_caveats(grace))
    .privacy(Privacy {
        notes: vec![
            "The date of birth and dose dates you supplied were used for the calculation; the date of birth is not repeated in the answer.".into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

fn normalize_vaccine_key(k: &str) -> String {
    let n = phn_core::text::normalize(k).replace(' ', "_");
    match n.as_str() {
        "all" | "all_vaccines" => "all".into(),
        "mmrv" | "measles" | "mumps" | "rubella" => "mmr".into(),
        "var" | "chickenpox" => "varicella".into(),
        "ipv" => "polio".into(),
        "dt" | "dtp" => "dtap".into(),
        "td" => "tdap".into(),
        "mcv4" | "meningococcal" | "menabcwy" => "menacwy".into(),
        "hep_b" | "hepatitis_b" | "heplisav_b" => "hepb".into(),
        other => other.to_string(),
    }
}

// ------------------------------------------------------------------ batch mode

fn find_col(t: &phn_local::table::Table, names: &[&str]) -> Option<usize> {
    t.headers.iter().position(|h| {
        let n = phn_core::text::normalize(h).replace(' ', "_");
        names.contains(&n.as_str())
    })
}

fn split_dates(cell: &str) -> Vec<String> {
    cell.split([';', '|', ','])
        .flat_map(|p| p.split_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn truthy(s: &str) -> bool {
    crate::localdata::truthy(s) == Some(true)
}

fn batch(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    as_of: NaiveDate,
    grace: bool,
) -> Result<Envelope<SchoolCheckResult>> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let crate::localdata::LocalTable {
        table,
        label,
        findings,
    } = lt;
    let dob_col = find_col(&table, &["date_of_birth", "dob", "birth_date", "birthdate", "date_birth"]);
    let grade_col = find_col(&table, &["grade", "grade_level", "grade_entering"]);
    let (Some(dob_col), Some(grade_col)) = (dob_col, grade_col) else {
        return Err(PhnError::InvalidInput(format!(
            "the roster needs a 'Grade' column and a 'Date of Birth' column, plus one column per \
             vaccine with dose dates separated by semicolons (dtap, polio, hepb, hib, mmr, mmrv, \
             varicella, tdap, td, menacwy, ...). Columns found: {}",
            table.headers.join(", ")
        )));
    };
    let vaccine_cols: Vec<(usize, String)> = table
        .headers
        .iter()
        .enumerate()
        .filter_map(|(i, h)| antigens_for_key(h).map(|_| (i, h.clone())))
        .collect();
    if vaccine_cols.is_empty() {
        return Err(PhnError::InvalidInput(
            "no vaccine columns were recognized; name them dtap, polio, hepb, hib, mmr, mmrv, \
             varicella, tdap, td, menacwy (or product names such as Pediarix)"
                .into(),
        ));
    }
    let lab_col = find_col(&table, &["lab_immunity", "laboratory_evidence", "titers", "serology"]);
    let hist_col = find_col(&table, &["varicella_history", "chickenpox_history", "history_of_chickenpox"]);
    let ex_col = find_col(&table, &["exemptions", "exemption"]);
    let hs_col = find_col(&table, &["health_science"]);
    let ft_col = find_col(&table, &["full_time"]);
    let us_col = find_col(&table, &["born_in_us", "us_born"]);
    let res_col = find_col(&table, &["residential", "residential_combined_school"]);
    // Identifier columns are copied to the output file only.
    let id_cols: Vec<usize> = table
        .headers
        .iter()
        .enumerate()
        .filter(|(i, h)| {
            *i != dob_col
                && (findings.iter().any(|f| &f.column == *h && f.kind.is_direct())
                    || {
                        let n = phn_core::text::normalize(h);
                        n.contains("name") || n.contains("student id") || n == "id" || n.contains("sasid")
                    })
        })
        .map(|(i, _)| i)
        .collect();

    let mut per_student: Vec<Vec<String>> = Vec::new();
    let mut detail_rows: Vec<Vec<String>> = Vec::new();
    let mut problems: Vec<Vec<String>> = Vec::new();
    // (grade, requirement label) -> status -> count
    let mut counts: BTreeMap<(String, String), BTreeMap<ReqStatus, u64>> = BTreeMap::new();
    let mut req_order: Vec<String> = Vec::new();
    let mut evaluated = 0usize;
    for (ri, row) in table.rows.iter().enumerate() {
        let cell = |i: usize| row.get(i).map(String::as_str).unwrap_or("").trim();
        let ids: Vec<String> = id_cols.iter().map(|&i| cell(i).to_string()).collect();
        let fail = |msg: String, problems: &mut Vec<Vec<String>>| {
            let mut r = vec![(ri + 2).to_string()];
            r.extend(ids.clone());
            r.push(msg);
            problems.push(r);
        };
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let Some(dob) = dates::parse_date(cell(dob_col)) else {
            fail("date of birth is missing or not a date".into(), &mut problems);
            continue;
        };
        let mut doses: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (i, h) in &vaccine_cols {
            let ds = split_dates(cell(*i));
            if !ds.is_empty() {
                doses.entry(h.clone()).or_default().extend(ds);
            }
        }
        let exemptions: BTreeMap<String, String> = ex_col
            .map(|c| {
                cell(c)
                    .split([';', ','])
                    .filter_map(|p| {
                        let (v, k) = p.split_once(':')?;
                        Some((normalize_vaccine_key(v), k.trim().to_ascii_lowercase()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let st = Student {
            grade: cell(grade_col).to_string(),
            dob,
            doses,
            lab_immunity: lab_col
                .map(|c| cell(c).split([';', ',']).map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default(),
            varicella_history: hist_col.is_some_and(|c| truthy(cell(c))),
            exemptions,
            college: Some(CollegeInput {
                health_science: hs_col.is_some_and(|c| truthy(cell(c))),
                full_time: ft_col.map(|c| cell(c).is_empty() || truthy(cell(c))),
                born_in_us: us_col.map(|c| truthy(cell(c))),
            }),
            residential_combined_school: res_col.is_some_and(|c| truthy(cell(c))),
        };
        match evaluate_student(&st, as_of, grace) {
            Ok((_, grade, results, _)) => {
                evaluated += 1;
                let mut line = vec![(ri + 2).to_string()];
                line.extend(ids.clone());
                line.push(grade.clone());
                let all_met = results.iter().all(|r| matches!(r.status, ReqStatus::Met | ReqStatus::NotApplicable | ReqStatus::NotYetDue));
                line.push(if all_met { "yes".into() } else { "no".into() });
                let mut statuses = Vec::new();
                for r in &results {
                    if !req_order.contains(&r.label) {
                        req_order.push(r.label.clone());
                    }
                    *counts
                        .entry((grade.clone(), r.label.clone()))
                        .or_default()
                        .entry(r.status)
                        .or_default() += 1;
                    statuses.push(format!("{}: {}", r.label, r.status.label()));
                    let mut d = vec![(ri + 2).to_string()];
                    d.extend(ids.clone());
                    d.extend([
                        grade.clone(),
                        r.label.clone(),
                        r.status.label().to_string(),
                        r.reason.clone(),
                        r.earliest_next_dose.clone().unwrap_or_default(),
                        r.invalid_doses.iter().map(|x| format!("{} ({})", x.date, x.reason)).collect::<Vec<_>>().join("; "),
                    ]);
                    detail_rows.push(d);
                }
                line.push(statuses.join("; "));
                per_student.push(line);
            }
            Err(e) => fail(e.to_string(), &mut problems),
        }
    }

    // Write per-student results (identifiable) to the output folder.
    let id_headers: Vec<String> = id_cols.iter().map(|&i| table.headers[i].clone()).collect();
    let mut h1 = vec!["Row".to_string()];
    h1.extend(id_headers.clone());
    h1.extend(["Grade".into(), "All requirements met or not yet due".into(), "Requirement statuses".into()]);
    let mut h2 = vec!["Row".to_string()];
    h2.extend(id_headers.clone());
    h2.extend(["Grade", "Requirement", "Status", "Why", "Earliest next dose", "Doses that do not count"].map(String::from));
    let mut h3 = vec!["Row".to_string()];
    h3.extend(id_headers);
    h3.push("Problem".into());
    let mut blocks = vec![
        phn_docs::model::Block::Paragraph(vec![phn_docs::model::Inline::plain(format!(
            "Massachusetts school immunization requirements {} evaluated on {as_of} (grace period {}). Decision support only; verify against the MIIS and DPH's requirements document.",
            SCHOOL_REQS.school_year,
            if grace { "applied" } else { "not applied" }
        ))]),
        phn_docs::model::Block::Table(phn_docs::model::TableBlock {
            caption: Some("Students".into()),
            headers: h1,
            rows: per_student,
        }),
        phn_docs::model::Block::Table(phn_docs::model::TableBlock {
            caption: Some("Requirement details".into()),
            headers: h2,
            rows: detail_rows,
        }),
    ];
    if !problems.is_empty() {
        blocks.push(phn_docs::model::Block::Table(phn_docs::model::TableBlock {
            caption: Some("Rows not evaluated".into()),
            headers: h3,
            rows: problems.clone(),
        }));
    }
    let doc = phn_docs::model::Document {
        title: "School immunization compliance".into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    };
    let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
    let sandbox = ctx.sandbox()?;
    let out = sandbox.new_output_file(&format!("school_immunization_check_{as_of}.xlsx"))?;
    std::fs::write(&out, bytes)?;
    let out_display = sandbox.display_path(&out);

    // Aggregate counts by grade and requirement, with suppression per grade.
    let threshold = ctx.suppression_threshold();
    let statuses: Vec<ReqStatus> = ReqStatus::ALL
        .into_iter()
        .filter(|s| counts.values().any(|m| m.contains_key(s)))
        .collect();
    let mut grades: Vec<String> = counts.keys().map(|(g, _)| g.clone()).collect();
    grades.sort_by_key(|g| grade_rank(g));
    grades.dedup();
    let mut count_rows = Vec::new();
    let mut any_suppressed = false;
    for g in &grades {
        let reqs: Vec<&String> = req_order.iter().filter(|r| counts.contains_key(&(g.clone(), (*r).clone()))).collect();
        let matrix: Vec<Vec<Option<u64>>> = reqs
            .iter()
            .map(|r| {
                let m = &counts[&(g.clone(), (*r).clone())];
                statuses.iter().map(|s| Some(*m.get(s).unwrap_or(&0))).collect()
            })
            .collect();
        let sup = suppress(
            &matrix,
            &SuppressionOptions {
                threshold,
                ..SuppressionOptions::default()
            },
        );
        any_suppressed |= sup.primary + sup.complementary > 0;
        for (ri, r) in reqs.iter().enumerate() {
            let mut m = BTreeMap::new();
            for (ci, s) in statuses.iter().enumerate() {
                let v = matrix[ri][ci].unwrap_or(0);
                m.insert(
                    s.label().to_string(),
                    if sup.is_hidden(ri, ci) { "*".to_string() } else { v.to_string() },
                );
            }
            count_rows.push(CountRow {
                grade: g.clone(),
                requirement: (*r).clone(),
                counts: m,
            });
        }
    }

    let mut summary = format!(
        "**School immunization requirements — roster check ({} school year)**\n\n{evaluated} of {} student row(s) evaluated on {as_of}{}. Per-student results were written to `{out_display}` (not shown here).\n\n",
        SCHOOL_REQS.school_year,
        evaluated + problems.len(),
        if problems.is_empty() { String::new() } else { format!("; {} row(s) could not be evaluated (listed in the output file)", problems.len()) },
    );
    let mut headers = vec!["Grade", "Requirement"];
    let labels: Vec<&str> = statuses.iter().map(|s| s.label()).collect();
    headers.extend(labels.iter().copied());
    let rows: Vec<Vec<String>> = count_rows
        .iter()
        .map(|r| {
            let mut v = vec![r.grade.clone(), r.requirement.clone()];
            v.extend(labels.iter().map(|l| r.counts.get(*l).cloned().unwrap_or_default()));
            v
        })
        .collect();
    summary.push_str(&md::table(&headers, &rows));
    if any_suppressed {
        summary.push_str(&format!(
            "\n`*` = suppressed (counts from 1 to {} and cells that would reveal them).\n",
            threshold - 1
        ));
    }
    let mut caveats = base_caveats(grace);
    caveats.push(
        "Counts are by requirement; a student can appear in several rows. Students marked 'needs \
         review' have records the rules cannot decide (for example Hib product, partial laboratory \
         evidence, or a missing DTaP primary-series history)."
            .into(),
    );
    Ok(Envelope::new(
        summary,
        SchoolCheckResult {
            school_year: SCHOOL_REQS.school_year.clone(),
            setting: "roster".into(),
            evaluated_on: as_of.to_string(),
            grace_period_applied: grace,
            requirements: Vec::new(),
            batch: Some(BatchSummary {
                file: label.clone(),
                students_read: table.rows.len(),
                students_evaluated: evaluated,
                rows_with_problems: problems.len(),
                counts: count_rows,
                output_file: out_display.clone(),
                suppression_threshold: threshold,
            }),
            exemption_rules: exemption_rules(),
            notes: Vec::new(),
        },
    )
    .source(Source::local_file(label))
    .sources(sources()?)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: {
            let mut k: Vec<String> = findings.iter().map(|f| f.kind.label().to_string()).collect();
            k.sort();
            k.dedup();
            k
        },
        files_written: vec![out_display],
        notes: vec!["Only counts by grade and requirement are returned; student-level results are in the output file.".into()],
    })
    .decision_support())
}

fn grade_rank(g: &str) -> (u32, String) {
    match g {
        "PK" => (0, String::new()),
        "K" => (1, String::new()),
        "college" => (20, String::new()),
        other => (other.parse::<u32>().map(|n| n + 1).unwrap_or(30), other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn student(grade: &str, dob: &str, doses: &[(&str, &[&str])]) -> Student {
        Student {
            grade: grade.into(),
            dob: d(dob),
            doses: doses
                .iter()
                .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
                .collect(),
            lab_immunity: BTreeSet::new(),
            varicella_history: false,
            exemptions: BTreeMap::new(),
            college: None,
            residential_combined_school: false,
        }
    }

    fn status(results: &[RequirementResult], id: &str) -> ReqStatus {
        results.iter().find(|r| r.requirement_id == id).unwrap().status
    }

    /// A kindergartner who received every dose on the routine schedule.
    #[test]
    fn routine_kindergartner_meets_every_requirement() {
        let s = student(
            "K",
            "2021-03-10",
            &[
                ("dtap", &["2021-05-10", "2021-07-12", "2021-09-10", "2022-06-10", "2025-03-12"]),
                ("polio", &["2021-05-10", "2021-07-12", "2021-09-10", "2025-03-12"]),
                ("hepb", &["2021-03-11", "2021-05-10", "2021-09-10"]),
                ("mmr", &["2022-03-15", "2025-03-12"]),
                ("varicella", &["2022-03-15", "2025-03-12"]),
            ],
        );
        let (setting, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(setting, "k_6");
        for x in &r {
            assert_eq!(x.status, ReqStatus::Met, "{}: {}", x.label, x.reason);
        }
    }

    #[test]
    fn four_dtap_doses_count_only_when_the_fourth_is_after_the_4th_birthday() {
        let ok = student("K", "2020-01-01", &[("dtap", &["2020-03-01", "2020-05-01", "2020-07-01", "2024-01-03"])]);
        let (_, _, r, _) = evaluate_student(&ok, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "dtap"), ReqStatus::Met);
        let short = student("K", "2020-01-01", &[("dtap", &["2020-03-01", "2020-05-01", "2020-07-01", "2021-03-01"])]);
        let (_, _, r, _) = evaluate_student(&short, d("2026-09-01"), true).unwrap();
        let x = r.iter().find(|x| x.requirement_id == "dtap").unwrap();
        assert_eq!(x.status, ReqStatus::NotMet);
        assert_eq!(x.valid_doses, 4);
        // Dose 5: minimum age 4 years and 6 months after dose 4.
        assert_eq!(x.earliest_next_dose.as_deref(), Some("2024-01-01"));
        assert!(x.eligible_now);
    }

    #[test]
    fn grace_period_counts_a_dose_four_days_early_but_not_five() {
        // MMR dose 1 four days before the first birthday counts with the grace period.
        let four = student("K", "2020-06-15", &[("mmr", &["2021-06-11", "2024-07-01"])]);
        let (_, _, r, _) = evaluate_student(&four, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "mmr"), ReqStatus::Met);
        let (_, _, r, _) = evaluate_student(&four, d("2026-09-01"), false).unwrap();
        let x = r.iter().find(|x| x.requirement_id == "mmr").unwrap();
        assert_eq!(x.status, ReqStatus::NotMet);
        assert!(x.invalid_doses[0].reason.contains("4 day(s) before the minimum age"));
        let five = student("K", "2020-06-15", &[("mmr", &["2021-06-10", "2024-07-01"])]);
        let (_, _, r, _) = evaluate_student(&five, d("2026-09-01"), true).unwrap();
        let x = r.iter().find(|x| x.requirement_id == "mmr").unwrap();
        assert_eq!(x.status, ReqStatus::NotMet);
        assert_eq!(x.valid_doses, 1);
        // Repeat: second dose at least 4 weeks after the valid dose (2024-07-01 + 28 days).
        assert_eq!(x.earliest_next_dose.as_deref(), Some("2024-07-29"));
    }

    #[test]
    fn live_vaccines_given_less_than_28_days_apart_invalidate_the_second() {
        let s = student("K", "2020-01-01", &[("mmr", &["2021-01-05", "2024-02-01"]), ("varicella", &["2021-01-20", "2024-02-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        let v = r.iter().find(|x| x.requirement_id == "varicella").unwrap();
        assert_eq!(v.status, ReqStatus::NotMet);
        assert!(v.invalid_doses[0].reason.contains("different live vaccine"));
        // MMRV counts for both on the same day without conflict.
        let s = student("K", "2020-01-01", &[("mmrv", &["2021-01-05", "2024-02-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "mmr"), ReqStatus::Met);
        assert_eq!(status(&r, "varicella"), ReqStatus::Met);
    }

    #[test]
    fn immunity_and_exemptions_follow_count_each_child_once() {
        let mut s = student("K", "2020-01-01", &[("varicella", &["2021-02-01"])]);
        s.varicella_history = true;
        s.exemptions.insert("mmr".into(), "religious".into());
        s.lab_immunity.insert("hepb".into());
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "varicella"), ReqStatus::Met);
        assert_eq!(status(&r, "mmr"), ReqStatus::ExemptReligious);
        assert_eq!(status(&r, "hepb"), ReqStatus::Met);
        assert_eq!(status(&r, "polio"), ReqStatus::NotMet);
        // Partial MMR lab evidence needs review.
        let mut s = student("K", "2020-01-01", &[]);
        s.lab_immunity.insert("measles".into());
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "mmr"), ReqStatus::NeedsReview);
    }

    #[test]
    fn meningococcal_rules_by_grade() {
        // Grade 7: one dose on or after the 10th birthday.
        let s = student("7", "2014-05-01", &[("menacwy", &["2025-06-01"]), ("tdap", &["2025-06-01"]), ("dtap", &["2014-07-01", "2014-09-01", "2014-11-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "men_7_10"), ReqStatus::Met);
        assert_eq!(status(&r, "tdap"), ReqStatus::Met);
        assert!(r.iter().all(|x| x.requirement_id != "men_11_12"));
        // Grade 11 at age 15: not yet due until the 16th birthday.
        let s = student("11", "2011-01-15", &[("menacwy", &["2022-02-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        let m = r.iter().find(|x| x.requirement_id == "men_11_12").unwrap();
        assert_eq!(m.status, ReqStatus::NotYetDue);
        assert_eq!(m.earliest_next_dose.as_deref(), Some("2027-01-15"));
        // Grade 12, 17 years old, dose only at 12: not met, eligible now.
        let s = student("12", "2009-03-01", &[("menacwy", &["2021-04-01"]), ("menb", &["2025-04-01"])]);
        let (_, _, r, notes) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        let m = r.iter().find(|x| x.requirement_id == "men_11_12").unwrap();
        assert_eq!(m.status, ReqStatus::NotMet);
        assert!(m.eligible_now);
        assert!(notes.iter().any(|n| n.contains("MenB")));
    }

    #[test]
    fn polio_final_dose_rules() {
        // 3 doses with the third after 4 years and 6 months after the second: met.
        let s = student("1", "2019-01-01", &[("polio", &["2019-03-01", "2019-05-01", "2023-02-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "polio"), ReqStatus::Met);
        // 4th dose after 4th birthday but only 5 months after the 3rd: a 5th is required.
        let s = student("1", "2019-01-01", &[("polio", &["2019-03-01", "2019-05-01", "2022-09-01", "2023-02-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        let p = r.iter().find(|x| x.requirement_id == "polio").unwrap();
        assert_eq!(p.status, ReqStatus::NotMet);
        assert_eq!(p.earliest_next_dose.as_deref(), Some("2023-08-01"));
    }

    #[test]
    fn preschool_under_two_and_hib() {
        let s = student("PK", "2025-03-01", &[("hib", &["2025-05-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert!(r.iter().all(|x| x.status == ReqStatus::NotYetDue));
        let s = student("PK", "2023-03-01", &[("hib", &["2023-05-01", "2023-07-01", "2024-07-01"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "hib"), ReqStatus::Met);
        let s = student("PK", "2023-03-01", &[("hib", &["2023-05-01", "2023-07-01", "2024-03-05"])]);
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "hib"), ReqStatus::NeedsReview);
    }

    #[test]
    fn college_birth_year_and_heplisav() {
        let mut s = student("college", "1955-05-01", &[("heplisav_b", &["2024-01-01", "2024-02-15"])]);
        s.college = Some(CollegeInput {
            health_science: false,
            full_time: Some(true),
            born_in_us: Some(true),
        });
        let (_, _, r, _) = evaluate_student(&s, d("2026-09-01"), true).unwrap();
        assert_eq!(status(&r, "mmr"), ReqStatus::Met);
        assert_eq!(status(&r, "varicella"), ReqStatus::Met);
        assert_eq!(status(&r, "hepb"), ReqStatus::Met);
        assert_eq!(status(&r, "men_college"), ReqStatus::NotApplicable);
    }

    #[test]
    fn bad_inputs_are_tool_errors() {
        let s = student("K", "2020-01-01", &[("mmr", &["2019-01-01"])]);
        assert!(evaluate_student(&s, d("2026-09-01"), true).is_err());
        let s = student("K", "2020-01-01", &[("mmr", &["2027-01-01"])]);
        assert!(evaluate_student(&s, d("2026-09-01"), true).is_err());
        let s = student("13th", "2020-01-01", &[]);
        assert!(evaluate_student(&s, d("2026-09-01"), true).is_err());
    }
}
