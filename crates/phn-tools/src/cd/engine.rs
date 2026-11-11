//! Date-rule engine for isolation, quarantine, and exclusion (shared by
//! CD-03, CD-06, and CD-07).
//!
//! Day counting follows `conventions.day_counting` in
//! `content/cd/disease_rules.yaml`: day 0 is the anchor date; "through day N"
//! means excluded on day N and back on day N + 1; for "N days of treatment"
//! the first full day of treatment is day 1, so return is on start + N;
//! "H hours after" with dates only gives the earliest calendar date on which
//! the hours could have passed (anchor + ⌊H/24⌋ days).

use chrono::{Datelike, Days, NaiveDate};
use phn_content::disease_rules::{
    self, Action, Anchor, DayRef, Disease, DiseaseRules, Role, Rule, Setting, StoolSpecimens, When,
};
use phn_content::reportable::Confidence;
use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Dates known about the person and the exposure (ISO strings parsed).
#[derive(Debug, Clone, Default)]
pub struct DateFacts {
    pub onset: Option<NaiveDate>,
    pub positive_specimen: Option<NaiveDate>,
    pub first_exposure: Option<NaiveDate>,
    pub last_exposure: Option<NaiveDate>,
    pub case_onset: Option<NaiveDate>,
    pub last_case_onset: Option<NaiveDate>,
    pub treatment_start: Option<NaiveDate>,
    pub symptom_resolution: Option<NaiveDate>,
}

/// Parse an optional ISO date argument.
pub fn parse_date_arg(name: &str, v: &Option<String>) -> Result<Option<NaiveDate>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| {
                PhnError::InvalidInput(format!("{name} '{s}' is not a date (YYYY-MM-DD)"))
            })?;
            // "%Y" also reads "26" as the year 26 and signed years far outside
            // the calendar; date arithmetic on those can overflow.
            if !(1900..=2100).contains(&d.year()) {
                return Err(PhnError::InvalidInput(format!(
                    "{name} '{s}' must be a date between 1900 and 2100 (YYYY-MM-DD, four-digit year)"
                )));
            }
            Ok(Some(d))
        }
    }
}

/// Format a date for output.
pub fn fmt(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Weekday and date, e.g. "Tue 2026-11-10".
pub fn fmt_long(d: NaiveDate) -> String {
    d.format("%a %Y-%m-%d").to_string()
}

/// Add (or subtract) days.
pub fn add(d: NaiveDate, days: i64) -> NaiveDate {
    if days >= 0 {
        d.checked_add_days(Days::new(days as u64)).unwrap_or(d)
    } else {
        d.checked_sub_days(Days::new((-days) as u64)).unwrap_or(d)
    }
}

impl DateFacts {
    /// The date of an anchor, if known. A single exposure date serves as both
    /// first and last exposure (a discrete exposure).
    pub fn anchor(&self, a: Anchor) -> Option<NaiveDate> {
        match a {
            Anchor::Onset => self.onset,
            Anchor::PositiveSpecimen => self.positive_specimen,
            Anchor::FirstExposure => self.first_exposure.or(self.last_exposure),
            Anchor::LastExposure => self.last_exposure.or(self.first_exposure),
            Anchor::CaseOnset => self.case_onset,
            Anchor::LastCaseOnset => self.last_case_onset,
            Anchor::TreatmentStart => self.treatment_start,
            Anchor::SymptomResolution => self.symptom_resolution,
        }
    }

    /// Validate the order of the dates.
    pub fn validate(&self) -> Result<()> {
        if let (Some(a), Some(b)) = (self.first_exposure, self.last_exposure)
            && a > b
        {
            return Err(PhnError::InvalidInput(
                "first_exposure_date is after last_exposure_date".into(),
            ));
        }
        if let (Some(a), Some(b)) = (self.onset, self.symptom_resolution)
            && b < a
        {
            return Err(PhnError::InvalidInput(
                "symptom_resolution_date is before onset_date".into(),
            ));
        }
        if let (Some(a), Some(b)) = (self.case_onset, self.last_case_onset)
            && b < a
        {
            return Err(PhnError::InvalidInput(
                "last_case_onset_date is before case_onset_date".into(),
            ));
        }
        Ok(())
    }
}

/// Facts that decide which rules apply.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Facts {
    /// The person has symptoms (for a contact: is symptomatic now).
    #[serde(default)]
    pub symptomatic: Option<bool>,
    /// The contact lacks evidence of immunity (unvaccinated, under-vaccinated, no titer).
    #[serde(default)]
    pub susceptible: Option<bool>,
    /// The contact is taking post-exposure antibiotic prophylaxis.
    #[serde(default)]
    pub received_prophylaxis: Option<bool>,
    /// The contact received immune globulin (IG, VZIG, IVIG).
    #[serde(default)]
    pub received_immune_globulin: Option<bool>,
    /// The case is part of an outbreak (stricter food handler rules).
    #[serde(default)]
    pub outbreak: Option<bool>,
    /// Exposure was continuous (household) or several cases occurred in the setting.
    #[serde(default)]
    pub exposure_continuous: Option<bool>,
    /// Lesions (varicella vesicles) are present.
    #[serde(default)]
    pub lesions_present: Option<bool>,
    /// The person has diarrhea now.
    #[serde(default)]
    pub diarrhea: Option<bool>,
}

impl Facts {
    fn get(&self, name: &str) -> Option<bool> {
        match name {
            "symptomatic" => self.symptomatic,
            "susceptible" => self.susceptible,
            "received_prophylaxis" => self.received_prophylaxis,
            "received_immune_globulin" => self.received_immune_globulin,
            "outbreak" => self.outbreak,
            "exposure_continuous" => self.exposure_continuous,
            "lesions_present" => self.lesions_present,
            "diarrhea" => self.diarrhea,
            _ => None,
        }
    }
}

pub(crate) fn condition_label(name: &str, value: bool) -> String {
    let (yes, no) = match name {
        "symptomatic" => ("has symptoms", "has no symptoms"),
        "susceptible" => ("lacks evidence of immunity", "has evidence of immunity"),
        "received_prophylaxis" => ("is taking prophylaxis", "is not taking prophylaxis"),
        "received_immune_globulin" => (
            "received immune globulin",
            "did not receive immune globulin",
        ),
        "outbreak" => ("outbreak circumstances", "not an outbreak"),
        "exposure_continuous" => (
            "continuous exposure or several cases",
            "a single, discrete exposure",
        ),
        "lesions_present" => ("lesions present", "no lesions"),
        "diarrhea" => ("has diarrhea", "no diarrhea"),
        _ => ("yes", "no"),
    };
    if value { yes } else { no }.to_string()
}

/// A rule evaluated for one person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RuleResult {
    pub rule_id: String,
    /// Who the rule is about.
    pub applies_to: String,
    /// Requested settings this rule covers.
    pub settings: Vec<Setting>,
    pub action: Action,
    /// True when every condition of the rule is known to hold; false when it
    /// depends on facts that were not given (see `conditions_unknown`).
    pub definite: bool,
    /// Conditions that must hold for the rule to apply but were not given.
    pub conditions_unknown: Vec<String>,
    /// First day of exclusion (YYYY-MM-DD), when the rule sets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// Last day excluded or restricted (YYYY-MM-DD), when computable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_excluded_date: Option<String>,
    /// Earliest date the person may return (YYYY-MM-DD), when computable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_return_date: Option<String>,
    /// Estimated (typical) return date when the end depends on clinical criteria.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typical_return_date: Option<String>,
    /// How the dates were computed, in words.
    pub computation: Vec<String>,
    /// Dates that would be needed to compute this rule but were not given.
    pub missing_dates: Vec<String>,
    /// Clinical or laboratory criteria that cannot be computed from dates.
    pub criteria: Vec<String>,
    /// Stool specimen requirements, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specimens: Option<String>,
    /// The source's wording (quoted when `verbatim`).
    pub wording: String,
    pub verbatim: bool,
    pub citation: String,
    /// Source id in the content file.
    pub source: String,
    pub confidence: Confidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn day_ref(d: &DayRef, facts: &DateFacts, missing: &mut Vec<String>) -> Option<NaiveDate> {
    match facts.anchor(d.anchor) {
        Some(a) => Some(add(a, i64::from(d.day))),
        None => {
            let name = anchor_arg(d.anchor).to_string();
            if !missing.contains(&name) {
                missing.push(name);
            }
            None
        }
    }
}

/// The argument name that supplies an anchor.
pub fn anchor_arg(a: Anchor) -> &'static str {
    match a {
        Anchor::Onset => "onset_date",
        Anchor::PositiveSpecimen => "positive_specimen_date",
        Anchor::FirstExposure => "first_exposure_date",
        Anchor::LastExposure => "last_exposure_date",
        Anchor::CaseOnset => "case_onset_date",
        Anchor::LastCaseOnset => "last_case_onset_date",
        Anchor::TreatmentStart => "treatment_start_date",
        Anchor::SymptomResolution => "symptom_resolution_date",
    }
}

fn specimens_text(s: &StoolSpecimens) -> String {
    let mut parts = vec![format!(
        "{} negative stool specimen{}",
        s.count,
        if s.count == 1 { "" } else { "s" }
    )];
    if let Some(h) = s.hours_apart {
        parts.push(format!("collected at least {h} hours apart"));
    }
    if s.after_symptom_resolution == Some(true) {
        parts.push("collected after diarrhea has resolved".into());
    }
    if let Some(h) = s.hours_after_antimicrobial {
        parts.push(format!(
            "the first collected at least {h} hours after the last antimicrobial dose (if treated)"
        ));
    }
    if let Some(d) = s.min_days_after_onset {
        parts.push(format!("the first no sooner than {d} days after onset"));
    }
    parts.join(", ")
}

/// Evaluate one rule.
pub fn evaluate(
    rule: &Rule,
    settings: Vec<Setting>,
    facts: &Facts,
    dates: &DateFacts,
) -> RuleResult {
    let mut conditions_unknown = Vec::new();
    for (name, value) in rule.when.conditions() {
        if facts.get(name).is_none() {
            conditions_unknown.push(condition_label(name, value));
        }
    }
    let mut missing = Vec::new();
    let mut computation = Vec::new();
    let start = rule.start.as_ref().and_then(|s| {
        let d = day_ref(s, dates, &mut missing);
        if let Some(d) = d {
            computation.push(format!(
                "Starts day {} after {} ({}).",
                s.day,
                s.anchor.label(),
                fmt(d)
            ));
        }
        d
    });
    let end = &rule.end;
    let mut last_excluded: Option<NaiveDate> = None;
    let mut earliest_return: Option<NaiveDate> = None;
    if let Some(t) = &end.through
        && let Some(d) = day_ref(t, dates, &mut missing)
    {
        last_excluded = Some(d);
        earliest_return = Some(add(d, 1));
        computation.push(format!(
            "Excluded through day {} after {} (day 0 = {}): last day {}, back on {}.",
            t.day,
            t.anchor.label(),
            fmt(facts_anchor(dates, t.anchor)),
            fmt(d),
            fmt(add(d, 1))
        ));
    }
    if let Some(n) = end.or_treatment_days {
        match dates.treatment_start {
            Some(ts) => {
                let ret = add(ts, i64::from(n));
                computation.push(format!(
                    "Or after {n} full days of appropriate treatment (day 1 = {}): back on {}.",
                    fmt(ts),
                    fmt(ret)
                ));
                if earliest_return.is_none_or(|r| ret < r) {
                    earliest_return = Some(ret);
                    last_excluded = Some(add(ret, -1));
                }
            }
            None => computation.push(format!(
                "Or after {n} full days of appropriate treatment (give treatment_start_date to \
                 compute this)."
            )),
        }
    }
    if let Some(h) = end.hours_after_treatment_start {
        match dates.treatment_start {
            Some(ts) => {
                let ret = add(ts, i64::from(h / 24));
                computation.push(format!(
                    "{h} hours after treatment started ({}): earliest {} if treatment started at \
                     the start of the day; otherwise the next day. Check the time of the first dose.",
                    fmt(ts),
                    fmt(ret)
                ));
                earliest_return = Some(earliest_return.map_or(ret, |r| r.max(ret)));
            }
            None => {
                if !missing.iter().any(|m| m == "treatment_start_date") {
                    missing.push("treatment_start_date".into());
                }
            }
        }
    }
    if let Some(h) = end.hours_after_symptom_resolution {
        match dates.symptom_resolution {
            Some(sr) => {
                let ret = add(sr, i64::from(h / 24));
                computation.push(format!(
                    "{h} hours after symptoms resolved ({}): earliest {} (later if symptoms \
                     resolved late in the day).",
                    fmt(sr),
                    fmt(ret)
                ));
                earliest_return = Some(earliest_return.map_or(ret, |r| r.max(ret)));
            }
            None => {
                if !missing.iter().any(|m| m == "symptom_resolution_date") {
                    missing.push("symptom_resolution_date".into());
                }
            }
        }
    }
    if let Some(h) = end.hours_after_specimen {
        match dates.positive_specimen {
            Some(ps) => {
                let ret = add(ps, i64::from(h / 24));
                computation.push(format!(
                    "And at least {h} hours after the positive specimen ({}): earliest {}.",
                    fmt(ps),
                    fmt(ret)
                ));
                earliest_return = Some(earliest_return.map_or(ret, |r| r.max(ret)));
            }
            None => {
                if !missing.iter().any(|m| m == "positive_specimen_date") {
                    missing.push("positive_specimen_date".into());
                }
            }
        }
    }
    // Hours-based criteria set (or delay) the earliest return date; keep the
    // last excluded day in step with it so "still excluded on a date" checks work.
    if let Some(r) = earliest_return {
        let day_before = add(r, -1);
        if last_excluded.is_none_or(|l| l < day_before) {
            last_excluded = Some(day_before);
        }
    }
    let mut typical = None;
    if let Some(t) = &end.typical_through {
        let mut m2 = Vec::new();
        if let Some(d) = day_ref(t, dates, &mut m2) {
            typical = Some(add(d, 1));
            computation.push(format!(
                "Typically through day {} after {} ({}), so usually back about {} — but only when \
                 the criteria are met.",
                t.day,
                t.anchor.label(),
                fmt(d),
                fmt(add(d, 1))
            ));
        }
    }
    let specimens = end.stool_specimens.as_ref().map(specimens_text);
    // A restriction that ends only on criteria or specimens has no computable end.
    let has_noncomputable_end = !end.criteria.is_empty() || specimens.is_some();
    if has_noncomputable_end && end.through.is_none() && earliest_return.is_some() {
        computation.push(
            "The date above is the earliest possible return; the criteria below must also be met."
                .into(),
        );
    }
    if matches!(
        rule.action,
        Action::NoRestriction | Action::ProphylaxisOnly | Action::Refer
    ) && computation.is_empty()
    {
        computation.push(match rule.action {
            Action::Refer => "Not computed: refer (see wording and note).".to_string(),
            _ => "No exclusion dates to compute.".to_string(),
        });
    }
    RuleResult {
        rule_id: rule.id.clone(),
        applies_to: rule.applies_to.clone(),
        settings,
        action: rule.action,
        definite: conditions_unknown.is_empty(),
        conditions_unknown,
        start_date: start.map(fmt),
        last_excluded_date: last_excluded.map(fmt),
        earliest_return_date: earliest_return.map(fmt),
        typical_return_date: typical.map(fmt),
        computation,
        missing_dates: missing,
        criteria: end.criteria.clone(),
        specimens,
        wording: rule.wording.clone(),
        verbatim: rule.verbatim,
        citation: rule.citation.clone(),
        source: rule.source.clone(),
        confidence: rule.confidence,
        note: rule.note.clone(),
    }
}

fn facts_anchor(dates: &DateFacts, a: Anchor) -> NaiveDate {
    dates.anchor(a).unwrap_or_default()
}

/// Rules for `role` that apply in any of `settings` given the facts, most
/// specific first; rules contradicted by a fact, or superseded by a more
/// specific rule that fully applies, are dropped.
pub fn applicable_rules<'a>(
    disease: &'a Disease,
    role: Role,
    settings: &[Setting],
    facts: &Facts,
) -> Vec<(&'a Rule, Vec<Setting>)> {
    let candidates: Vec<(&Rule, Vec<Setting>)> = disease
        .rules
        .iter()
        .filter(|r| r.role == role)
        .filter(|r| {
            r.when
                .conditions()
                .iter()
                .all(|(name, v)| facts.get(name).is_none_or(|f| f == *v))
        })
        .map(|r| {
            let s: Vec<Setting> = settings.iter().copied().filter(|s| r.covers(*s)).collect();
            (r, s)
        })
        .filter(|(_, s)| !s.is_empty())
        .collect();
    let fully = |r: &Rule| {
        r.when
            .conditions()
            .iter()
            .all(|(name, _)| facts.get(name).is_some())
    };
    let subset = |a: &When, b: &When| {
        let ca = a.conditions();
        let cb = b.conditions();
        ca.len() < cb.len() && ca.iter().all(|c| cb.contains(c))
    };
    let mut out = Vec::new();
    for (r, s) in &candidates {
        // Settings where a more specific, fully applicable rule takes over.
        let remaining: Vec<Setting> = s
            .iter()
            .copied()
            .filter(|setting| {
                !candidates.iter().any(|(o, os)| {
                    !std::ptr::eq(*o, *r)
                        && os.contains(setting)
                        && fully(o)
                        && subset(&r.when, &o.when)
                })
            })
            .collect();
        if !remaining.is_empty() {
            out.push((*r, remaining));
        }
    }
    out
}

/// A citation for a rule source id.
pub fn rule_source(content: &DiseaseRules, id: &str) -> Source {
    match content.sources.get(id) {
        Some(s) => Source {
            title: s.title.clone(),
            publisher: s.publisher.clone(),
            url: Some(s.url.clone()),
            effective_date: s.effective_date.clone(),
            retrieved: Some(s.retrieved.clone()),
            mode: SourceMode::Snapshot,
        },
        None => Source {
            title: id.to_string(),
            publisher: "Massachusetts Department of Public Health".into(),
            url: None,
            effective_date: None,
            retrieved: None,
            mode: SourceMode::Snapshot,
        },
    }
}

/// Find a disease or explain what is available.
pub fn find_disease(query: &str) -> Result<&'static Disease> {
    if let Some(d) = disease_rules::lookup(query) {
        return Ok(d);
    }
    let suggestions: Vec<String> = disease_rules::find(query)
        .into_iter()
        .take(5)
        .filter(|m| m.score > 0.3)
        .map(|m| m.disease.name.clone())
        .collect();
    let all: Vec<&str> = disease_rules::content()
        .diseases
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    Err(PhnError::NotFound(format!(
        "no isolation and exclusion rules for '{query}'.{} Diseases covered: {}. For anything \
         else, use lookup_reportable_condition and the DPH Guide (search_ma_disease_control_guide).",
        if suggestions.is_empty() {
            String::new()
        } else {
            format!(
                " Did you mean: {}? Give the specific disease name: general terms, and names \
                 that fit more than one disease, are not matched.",
                suggestions.join(", ")
            )
        },
        all.join(", ")
    )))
}

/// The infectious period in dates, if computable.
pub fn infectious_dates(d: &Disease, dates: &DateFacts) -> Option<(NaiveDate, NaiveDate, String)> {
    let ip = d.infectious_period.as_ref()?;
    let anchor = ip.anchor?;
    let a = dates.anchor(anchor)?;
    let start = add(a, i64::from(ip.start_day?));
    let mut end = add(a, i64::from(ip.end_day?));
    let mut how = format!(
        "day {} to day {} relative to {} ({})",
        ip.start_day.unwrap_or(0),
        ip.end_day.unwrap_or(0),
        d.onset_event,
        fmt(a)
    );
    if let (Some(n), Some(ts)) = (ip.ends_after_treatment_days, dates.treatment_start) {
        let t_end = add(ts, i64::from(n) - 1);
        if t_end < end {
            end = t_end.max(start);
            how.push_str(&format!(
                "; shortened by treatment: through day {n} of treatment ({})",
                fmt(t_end)
            ));
        }
    }
    Some((start, end, how))
}

/// Lower-case confidence word.
pub fn confidence_label(c: Confidence) -> &'static str {
    match c {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
    }
}

/// Review-status caveat shared by tools built on the disease rules.
pub fn pending_caveat(content: &DiseaseRules) -> String {
    format!(
        "REVIEW STATUS: the disease rules are a {} draft (content version {}, prepared {}) built \
         from the cited DPH, 105 CMR 300.000, and CDC documents. The nurse must verify each rule \
         against the current regulation and DPH guidance before relying on it (review due {}).",
        match content.meta.review.status {
            phn_content::ReviewStatus::Pending => "PENDING-REVIEW",
            phn_content::ReviewStatus::Reviewed => "reviewed",
        },
        content.meta.version,
        content.meta.prepared,
        content.meta.review.review_due
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
    }

    #[test]
    fn measles_case_counts_rash_onset_as_day_zero() {
        let m = find_disease("measles").unwrap();
        let rules = applicable_rules(m, Role::Case, &Setting::CONCRETE, &Facts::default());
        let (r, s) = rules.iter().find(|(r, _)| r.id == "case_all").unwrap();
        let dates = DateFacts {
            onset: d("2026-03-02"),
            ..DateFacts::default()
        };
        let res = evaluate(r, s.clone(), &Facts::default(), &dates);
        assert_eq!(res.last_excluded_date.as_deref(), Some("2026-03-06"));
        assert_eq!(res.earliest_return_date.as_deref(), Some("2026-03-07"));
    }

    #[test]
    fn pertussis_treatment_shortens_exclusion() {
        let p = find_disease("whooping cough").unwrap();
        let rules = applicable_rules(p, Role::Case, &[Setting::School], &Facts::default());
        let (r, s) = &rules[0];
        let dates = DateFacts {
            onset: d("2026-01-01"),
            treatment_start: d("2026-01-05"),
            ..DateFacts::default()
        };
        let res = evaluate(r, s.clone(), &Facts::default(), &dates);
        // Day 1 = Jan 5, so five full days end Jan 9; back Jan 10 (before Jan 23).
        assert_eq!(res.earliest_return_date.as_deref(), Some("2026-01-10"));
        assert_eq!(res.last_excluded_date.as_deref(), Some("2026-01-09"));
        // Without treatment: through day 21.
        let res = evaluate(
            r,
            s.clone(),
            &Facts::default(),
            &DateFacts {
                onset: d("2026-01-01"),
                ..DateFacts::default()
            },
        );
        assert_eq!(res.earliest_return_date.as_deref(), Some("2026-01-23"));
    }

    #[test]
    fn measles_susceptible_contact_days_5_to_21_and_continuous_supersedes() {
        let m = find_disease("measles").unwrap();
        let facts = Facts {
            susceptible: Some(true),
            exposure_continuous: Some(false),
            ..Facts::default()
        };
        let rules = applicable_rules(m, Role::Contact, &[Setting::School], &facts);
        let (r, s) = rules
            .iter()
            .find(|(r, _)| r.id == "contact_susceptible")
            .unwrap();
        let dates = DateFacts {
            first_exposure: d("2026-04-01"),
            ..DateFacts::default()
        };
        let res = evaluate(r, s.clone(), &facts, &dates);
        assert_eq!(res.start_date.as_deref(), Some("2026-04-06"));
        assert_eq!(res.last_excluded_date.as_deref(), Some("2026-04-22"));
        assert_eq!(res.earliest_return_date.as_deref(), Some("2026-04-23"));
        assert!(res.definite);
        // Continuous exposure: the specific rule replaces the general one.
        let facts = Facts {
            susceptible: Some(true),
            exposure_continuous: Some(true),
            ..Facts::default()
        };
        let ids: Vec<&str> = applicable_rules(m, Role::Contact, &[Setting::School], &facts)
            .iter()
            .map(|(r, _)| r.id.as_str())
            .collect();
        assert!(ids.contains(&"contact_susceptible_continuous"), "{ids:?}");
        assert!(!ids.contains(&"contact_susceptible"), "{ids:?}");
        // Immune contact: no susceptible rules.
        let facts = Facts {
            susceptible: Some(false),
            ..Facts::default()
        };
        assert!(
            applicable_rules(m, Role::Contact, &[Setting::School], &facts)
                .iter()
                .all(|(r, _)| r.when.susceptible != Some(true))
        );
    }

    #[test]
    fn hours_based_rules_set_the_last_excluded_day() {
        // Strep throat: back 24 hours after treatment starts, so still
        // excluded on the day treatment starts.
        let g = find_disease("strep throat").unwrap();
        let rules = applicable_rules(g, Role::Case, &[Setting::School], &Facts::default());
        let (r, s) = &rules[0];
        let dates = DateFacts {
            treatment_start: d("2026-11-09"),
            ..DateFacts::default()
        };
        let res = evaluate(r, s.clone(), &Facts::default(), &dates);
        assert_eq!(res.earliest_return_date.as_deref(), Some("2026-11-10"));
        assert_eq!(res.last_excluded_date.as_deref(), Some("2026-11-09"));
        // Without the date, nothing is computed and the date is asked for.
        let res = evaluate(r, s.clone(), &Facts::default(), &DateFacts::default());
        assert!(res.last_excluded_date.is_none());
        assert_eq!(res.missing_dates, vec!["treatment_start_date".to_string()]);
    }

    #[test]
    fn ambiguous_or_general_disease_names_are_not_matched() {
        for q in ["gastroenteritis", "h flu", "cholera", "hepatitis", "hep"] {
            let e = find_disease(q).unwrap_err().to_string();
            assert!(e.contains("Diseases covered"), "{q}: {e}");
        }
        assert!(
            find_disease("hepatitis")
                .unwrap_err()
                .to_string()
                .contains("Hepatitis B")
        );
        assert_eq!(find_disease("viral gastroenteritis").unwrap().id, "norovirus");
    }

    #[test]
    fn date_arguments_must_have_a_plausible_four_digit_year() {
        assert!(parse_date_arg("onset_date", &Some("2026-03-02".into())).is_ok());
        for bad in ["26-03-02", "+262142-12-31", "0026-05-03", "2101-01-01"] {
            assert!(
                parse_date_arg("onset_date", &Some(bad.into())).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn unknown_conditions_are_reported() {
        let s = find_disease("salmonella").unwrap();
        let rules = applicable_rules(s, Role::Case, &[Setting::FoodHandler], &Facts::default());
        assert!(rules.len() >= 2);
        let res = evaluate(
            rules[0].0,
            rules[0].1.clone(),
            &Facts::default(),
            &DateFacts::default(),
        );
        assert!(!res.definite);
        assert!(res.specimens.is_some());
    }

    #[test]
    fn every_rule_evaluates_without_panicking() {
        let c = disease_rules::content();
        let dates = DateFacts {
            onset: d("2026-05-01"),
            positive_specimen: d("2026-05-02"),
            first_exposure: d("2026-04-20"),
            last_exposure: d("2026-04-25"),
            case_onset: d("2026-04-22"),
            last_case_onset: d("2026-04-28"),
            treatment_start: d("2026-05-03"),
            symptom_resolution: d("2026-05-06"),
        };
        for dis in &c.diseases {
            for r in &dis.rules {
                let res = evaluate(r, vec![Setting::Other], &Facts::default(), &dates);
                assert!(res.missing_dates.is_empty(), "{}/{}", dis.id, r.id);
                if r.end.through.is_some() {
                    assert!(res.earliest_return_date.is_some(), "{}/{}", dis.id, r.id);
                }
            }
            let _ = infectious_dates(dis, &dates);
        }
    }
}
