//! CD-03 `calculate_isolation_and_exclusion`.

use phn_content::disease_rules::{self, Action, Role, Setting};
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::engine::{
    self, DateFacts, Facts, RuleResult, applicable_rules, evaluate, find_disease, fmt,
    infectious_dates, parse_date_arg,
};
use crate::context::Context;
use crate::md;

/// Arguments for `calculate_isolation_and_exclusion`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct IsolationArgs {
    /// Disease or condition, e.g. "pertussis", "measles", "Salmonella", "norovirus".
    pub disease: String,
    /// "case" (the ill or infected person; default) or "contact" (an exposed person).
    #[serde(default)]
    pub role: Option<Role>,
    /// Settings to evaluate: child_care, school, food_handler, health_care_worker,
    /// other (default: all five).
    #[serde(default)]
    pub settings: Option<Vec<Setting>>,
    /// Onset of the disease's onset event for this person (rash, cough, gland
    /// swelling, symptoms), YYYY-MM-DD. For a symptomatic contact, their own onset.
    #[serde(default)]
    pub onset_date: Option<String>,
    /// Date the positive specimen was collected (YYYY-MM-DD).
    #[serde(default)]
    pub positive_specimen_date: Option<String>,
    /// For contacts: first day exposed during the case's infectious period.
    #[serde(default)]
    pub first_exposure_date: Option<String>,
    /// For contacts: last day exposed (a single exposure date may be given here alone).
    #[serde(default)]
    pub last_exposure_date: Option<String>,
    /// For contacts: onset date of the case they were exposed to.
    #[serde(default)]
    pub case_onset_date: Option<String>,
    /// Onset of the last case in the setting (continuous exposure or several cases).
    #[serde(default)]
    pub last_case_onset_date: Option<String>,
    /// First full day of appropriate antimicrobial treatment (YYYY-MM-DD).
    #[serde(default)]
    pub treatment_start_date: Option<String>,
    /// Date symptoms (for GI illness: diarrhea and vomiting) resolved.
    #[serde(default)]
    pub symptom_resolution_date: Option<String>,
    /// Facts that decide which rules apply; leave unknown facts out and the
    /// answer will show the rules that depend on them.
    #[serde(default, flatten)]
    pub facts: Facts,
    /// Report whether the person is still excluded on this date (YYYY-MM-DD).
    #[serde(default)]
    pub as_of: Option<String>,
}

/// Disease background used for the dates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DiseaseInfo {
    pub id: String,
    pub name: String,
    /// What "onset" means for this disease.
    pub onset_event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incubation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infectious_period: Option<String>,
    /// Infectious period in dates, when the onset (or specimen) date is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infectious_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infectious_through: Option<String>,
    /// Developer confidence in this disease's draft entry.
    pub confidence: phn_content::reportable::Confidence,
    /// False when the server refers instead of computing (tuberculosis).
    pub computed: bool,
    pub notes: Vec<String>,
}

/// Status on the `as_of` date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AsOfStatus {
    pub date: String,
    /// Rules (definite, with dates) under which the person is excluded or restricted that day.
    pub excluded_under: Vec<String>,
    /// Plain-language status.
    pub status: String,
}

/// Result of `calculate_isolation_and_exclusion`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IsolationResult {
    pub disease: DiseaseInfo,
    pub role: Role,
    pub settings: Vec<Setting>,
    /// Rules that apply (definite) or may apply (depending on unknown facts).
    pub rules: Vec<RuleResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<AsOfStatus>,
    /// The day-counting convention used.
    pub day_counting: String,
}

/// Compute isolation, quarantine, and exclusion dates.
pub fn calculate_isolation_and_exclusion(
    _ctx: &Context,
    args: IsolationArgs,
) -> Result<Envelope<IsolationResult>> {
    let content = disease_rules::content();
    let disease = find_disease(&args.disease)?;
    let role = args.role.unwrap_or(Role::Case);
    let mut settings: Vec<Setting> = args
        .settings
        .clone()
        .unwrap_or_else(|| Setting::CONCRETE.to_vec())
        .into_iter()
        .flat_map(|s| {
            if s == Setting::All {
                Setting::CONCRETE.to_vec()
            } else {
                vec![s]
            }
        })
        .collect();
    settings.sort();
    settings.dedup();
    if settings.is_empty() {
        return Err(PhnError::InvalidInput("choose at least one setting".into()));
    }
    let dates = DateFacts {
        onset: parse_date_arg("onset_date", &args.onset_date)?,
        positive_specimen: parse_date_arg("positive_specimen_date", &args.positive_specimen_date)?,
        first_exposure: parse_date_arg("first_exposure_date", &args.first_exposure_date)?,
        last_exposure: parse_date_arg("last_exposure_date", &args.last_exposure_date)?,
        case_onset: parse_date_arg("case_onset_date", &args.case_onset_date)?,
        last_case_onset: parse_date_arg("last_case_onset_date", &args.last_case_onset_date)?,
        treatment_start: parse_date_arg("treatment_start_date", &args.treatment_start_date)?,
        symptom_resolution: parse_date_arg(
            "symptom_resolution_date",
            &args.symptom_resolution_date,
        )?,
    };
    dates.validate()?;
    let as_of = parse_date_arg("as_of", &args.as_of)?;
    let facts = args.facts.clone();

    let mut rules: Vec<RuleResult> = applicable_rules(disease, role, &settings, &facts)
        .into_iter()
        .map(|(r, s)| evaluate(r, s, &facts, &dates))
        .collect();
    rules.sort_by_key(|r| (!r.definite, r.settings.first().copied()));

    let infectious = infectious_dates(disease, &dates);
    let info = DiseaseInfo {
        id: disease.id.clone(),
        name: disease.name.clone(),
        onset_event: disease.onset_event.clone(),
        incubation: disease.incubation.as_ref().map(|i| i.describe()),
        infectious_period: disease.infectious_period.as_ref().map(|i| i.text.clone()),
        infectious_from: infectious.as_ref().map(|i| fmt(i.0)),
        infectious_through: infectious.as_ref().map(|i| fmt(i.1)),
        confidence: disease.confidence,
        computed: disease.compute,
        notes: disease.notes.clone(),
    };

    // ---- Markdown
    let role_label = match role {
        Role::Case => "case",
        Role::Contact => "contact",
    };
    let mut summary = format!(
        "**{} — isolation and exclusion for a {role_label}**\n\n",
        disease.name
    );
    if !disease.compute {
        summary.push_str(
            "⚠ This server does not compute dates for this disease; the rules below are for \
             reference. Refer to the clinician and DPH.\n\n",
        );
    }
    let given: Vec<String> = [
        ("onset", dates.onset),
        ("positive specimen", dates.positive_specimen),
        ("first exposure", dates.first_exposure),
        ("last exposure", dates.last_exposure),
        ("case onset", dates.case_onset),
        ("last case onset", dates.last_case_onset),
        ("treatment start", dates.treatment_start),
        ("symptoms resolved", dates.symptom_resolution),
    ]
    .iter()
    .filter_map(|(n, d)| d.map(|d| format!("{n} {}", fmt(d))))
    .collect();
    if !given.is_empty() {
        summary.push_str(&format!("Dates given: {}.\n\n", given.join("; ")));
    }
    if let Some((s, e, how)) = &infectious {
        summary.push_str(&format!(
            "Infectious period: {} through {} ({how}).\n\n",
            fmt(*s),
            fmt(*e)
        ));
    }
    let mut rows = Vec::new();
    for r in &rules {
        let settings_l: Vec<&str> = r.settings.iter().map(|s| s.label()).collect();
        let when = match (&r.start_date, &r.last_excluded_date) {
            (Some(s), Some(e)) => format!("{s} through {e}"),
            (None, Some(e)) => format!("now through {e}"),
            (Some(s), None) => format!("from {s}"),
            (None, None) => match r.action {
                Action::NoRestriction | Action::ProphylaxisOnly => "—".into(),
                _ => "until criteria met".into(),
            },
        };
        let back = r
            .earliest_return_date
            .clone()
            .map(|d| {
                if r.criteria.is_empty() && r.specimens.is_none() {
                    d
                } else {
                    format!("{d} at the earliest, if criteria met")
                }
            })
            .or_else(|| {
                r.typical_return_date
                    .clone()
                    .map(|d| format!("about {d} (typical; criteria decide)"))
            })
            .unwrap_or_else(|| {
                if r.missing_dates.is_empty() {
                    "criteria below".into()
                } else {
                    format!("needs {}", r.missing_dates.join(", "))
                }
            });
        let applies = if r.definite {
            r.applies_to.clone()
        } else {
            format!(
                "{} — only if {}",
                r.applies_to,
                r.conditions_unknown.join(" and ")
            )
        };
        rows.push(vec![
            settings_l.join(", "),
            applies,
            r.action.label().to_string(),
            when,
            back,
            format!(
                "{} ({} confidence)",
                r.citation,
                engine::confidence_label(r.confidence)
            ),
        ]);
    }
    if rows.is_empty() {
        summary.push_str(&format!(
            "No {role_label} rules apply in the chosen settings with the facts given.\n"
        ));
    } else {
        summary.push_str(&md::table(
            &[
                "Setting",
                "Applies to",
                "Action",
                "Excluded",
                "May return",
                "Citation",
            ],
            &rows,
        ));
    }
    let mut details = String::new();
    for r in &rules {
        let mut lines = Vec::new();
        lines.extend(r.computation.iter().cloned());
        if !r.criteria.is_empty() {
            lines.push(format!("Criteria: {}.", r.criteria.join("; ")));
        }
        if let Some(s) = &r.specimens {
            lines.push(format!("Specimens: {s}."));
        }
        if let Some(n) = &r.note {
            lines.push(format!("Note: {n}"));
        }
        let quote = if r.verbatim {
            format!("“{}”", r.wording)
        } else {
            format!("(paraphrase) {}", r.wording)
        };
        details.push_str(&format!(
            "\n- **{}** ({}): {quote} — {}\n",
            r.applies_to,
            r.settings
                .iter()
                .map(|s| s.label())
                .collect::<Vec<_>>()
                .join(", "),
            r.citation
        ));
        for l in lines {
            details.push_str(&format!("  - {l}\n"));
        }
    }
    if !details.is_empty() {
        summary.push_str("\n**Rule details**\n");
        summary.push_str(&details);
    }

    let as_of_status = as_of.map(|day| {
        let under: Vec<String> = rules
            .iter()
            .filter(|r| r.definite)
            .filter(|r| {
                let start_ok = r
                    .start_date
                    .as_deref()
                    .is_none_or(|s| s <= fmt(day).as_str());
                let end_ok = r
                    .last_excluded_date
                    .as_deref()
                    .is_some_and(|e| e >= fmt(day).as_str());
                start_ok && end_ok
            })
            .map(|r| r.rule_id.clone())
            .collect();
        let status = if under.is_empty() {
            format!(
                "On {}, no date-based exclusion with complete dates is in force (check rules that \
                 depend on criteria or unknown facts).",
                fmt(day)
            )
        } else {
            format!(
                "On {}, the person is still excluded or restricted under {} rule(s).",
                fmt(day),
                under.len()
            )
        };
        AsOfStatus {
            date: fmt(day),
            excluded_under: under,
            status,
        }
    });
    if let Some(a) = &as_of_status {
        summary.push_str(&format!("\n**As of {}:** {}\n", a.date, a.status));
    }
    summary.push_str(&format!(
        "\n_Day counting: {}_\n",
        content.conventions.day_counting.trim()
    ));

    // ---- Sources and caveats
    let mut source_ids: Vec<String> = rules.iter().map(|r| r.source.clone()).collect();
    if let Some(i) = &disease.incubation {
        source_ids.push(i.source.source.clone());
    }
    if let Some(i) = &disease.infectious_period {
        source_ids.push(i.source.source.clone());
    }
    source_ids.sort();
    source_ids.dedup();
    let sources: Vec<_> = source_ids
        .iter()
        .map(|id| engine::rule_source(content, id))
        .collect();
    let mut caveats = vec![engine::pending_caveat(content)];
    if rules.iter().any(|r| !r.definite) {
        caveats.push(
            "Some rules depend on facts that were not given (marked 'only if'); answer those \
             (for example susceptible, outbreak, diarrhea) to narrow the result."
                .into(),
        );
    }
    let missing: Vec<String> = {
        let mut m: Vec<String> = rules.iter().flat_map(|r| r.missing_dates.clone()).collect();
        m.sort();
        m.dedup();
        m
    };
    if !missing.is_empty() {
        caveats.push(format!(
            "Some dates could not be computed; give {} to complete them.",
            missing.join(", ")
        ));
    }
    if rules
        .iter()
        .any(|r| matches!(r.confidence, phn_content::reportable::Confidence::Low))
    {
        caveats.push(
            "At least one rule is LOW confidence (could not be verified against a fetched \
             source); confirm it with DPH before acting."
                .into(),
        );
    }
    caveats.push(
        "105 CMR 300.200 sets minimum periods; the local board of health or DPH may require \
         more (for example in outbreaks or high-risk settings), and health care facilities \
         follow their own infection control policies."
            .into(),
    );

    let data = IsolationResult {
        disease: info,
        role,
        settings,
        rules,
        as_of: as_of_status,
        day_counting: content.conventions.day_counting.trim().to_string(),
    };
    Ok(Envelope::new(summary, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy::none())
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn mumps_case_in_school() {
        let (_d, c) = ctx();
        let env = calculate_isolation_and_exclusion(
            &c,
            IsolationArgs {
                disease: "mumps".into(),
                settings: Some(vec![Setting::School]),
                onset_date: Some("2026-02-10".into()),
                as_of: Some("2026-02-14".into()),
                ..IsolationArgs::default()
            },
        )
        .unwrap();
        let r = &env.data.rules[0];
        assert_eq!(r.earliest_return_date.as_deref(), Some("2026-02-16"));
        assert_eq!(
            env.data.disease.infectious_from.as_deref(),
            Some("2026-02-08")
        );
        assert!(!env.data.as_of.unwrap().excluded_under.is_empty());
        assert!(env.notice.is_some());
        assert!(env.caveats[0].contains("PENDING-REVIEW"));
    }

    #[test]
    fn as_of_counts_exclusions_that_end_hours_after_treatment() {
        let (_d, c) = ctx();
        let args = |as_of: &str| IsolationArgs {
            disease: "strep throat".into(),
            settings: Some(vec![Setting::School]),
            treatment_start_date: Some("2026-11-09".into()),
            as_of: Some(as_of.into()),
            ..IsolationArgs::default()
        };
        // The day treatment starts: still excluded (back 24 hours later).
        let env = calculate_isolation_and_exclusion(&c, args("2026-11-09")).unwrap();
        assert_eq!(
            env.data.rules[0].earliest_return_date.as_deref(),
            Some("2026-11-10")
        );
        let a = env.data.as_of.unwrap();
        assert!(!a.excluded_under.is_empty(), "{}", a.status);
        assert!(a.status.contains("still excluded"));
        // The next day the date-based exclusion is over.
        let env = calculate_isolation_and_exclusion(&c, args("2026-11-10")).unwrap();
        assert!(env.data.as_of.unwrap().excluded_under.is_empty());
    }

    #[test]
    fn unknown_disease_suggests() {
        let (_d, c) = ctx();
        let e = calculate_isolation_and_exclusion(
            &c,
            IsolationArgs {
                disease: "zzzz".into(),
                ..IsolationArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("Diseases covered"));
        let e = calculate_isolation_and_exclusion(
            &c,
            IsolationArgs {
                disease: "measles".into(),
                onset_date: Some("03/02/2026".into()),
                ..IsolationArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("YYYY-MM-DD"));
    }
}
