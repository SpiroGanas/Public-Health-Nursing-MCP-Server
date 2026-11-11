//! CD-07 `assess_measles_exposure`.

use chrono::NaiveDate;
use phn_content::disease_rules::{self, Role, Setting};
use phn_content::measles::{self, ImmunityCriteria};
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::engine::{
    self, DateFacts, Facts, RuleResult, add, applicable_rules, evaluate, fmt, fmt_long,
    parse_date_arg,
};
use crate::context::Context;
use crate::md;

/// Which evidence-of-immunity standard applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImmunityGroup {
    /// Preschool children (12 months and older) and adults not at high risk: 1 dose.
    General,
    /// K-12 students, college students, international travelers: 2 doses.
    SchoolCollegeTravel,
    /// Health care personnel: 2 doses; birth before 1957 does not count.
    HealthCarePersonnel,
}

impl ImmunityGroup {
    fn id(self) -> &'static str {
        match self {
            ImmunityGroup::General => "general",
            ImmunityGroup::SchoolCollegeTravel => "school_college_travel",
            ImmunityGroup::HealthCarePersonnel => "health_care_personnel",
        }
    }
}

/// Arguments for `assess_measles_exposure`. Describe one exposed person
/// without identifiers (no names or dates of birth; birth year and age are enough).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct MeaslesExposureArgs {
    /// First day the person shared air space with the case during the infectious
    /// period (YYYY-MM-DD). A single exposure date can go here alone.
    #[serde(default)]
    pub first_exposure_date: Option<String>,
    /// Last day of exposure (YYYY-MM-DD), if exposed on several days.
    #[serde(default)]
    pub last_exposure_date: Option<String>,
    /// The case's rash onset (YYYY-MM-DD), to check the exposure fell in the
    /// infectious period (4 days before through 4 days after rash onset).
    #[serde(default)]
    pub case_rash_onset_date: Option<String>,
    /// Continuous exposure (household) or several cases in the setting.
    #[serde(default)]
    pub exposure_continuous: Option<bool>,
    /// Rash onset of the last case in the setting (for continuous exposure).
    #[serde(default)]
    pub last_case_onset_date: Option<String>,
    /// Where the person works, studies, or is cared for: child_care, school,
    /// food_handler, health_care_worker, or other (default other).
    #[serde(default)]
    pub setting: Option<Setting>,
    /// Evidence-of-immunity standard (default from the setting: health care →
    /// health_care_personnel; school → school_college_travel; otherwise general).
    #[serde(default)]
    pub immunity_group: Option<ImmunityGroup>,
    /// Year of birth (used for the born-before-1957 criterion).
    #[serde(default)]
    pub birth_year: Option<i32>,
    /// Age in months (for infants and young children).
    #[serde(default)]
    pub age_months: Option<u32>,
    /// Documented doses of measles-containing vaccine given at 12 months or
    /// older (doses before the first birthday do not count).
    #[serde(default)]
    pub documented_mmr_doses: Option<u32>,
    /// A positive measles IgG (laboratory evidence of immunity).
    #[serde(default)]
    pub positive_measles_igg: Option<bool>,
    /// Laboratory-confirmed measles in the past.
    #[serde(default)]
    pub lab_confirmed_measles: Option<bool>,
    /// Currently pregnant.
    #[serde(default)]
    pub pregnant: Option<bool>,
    /// Severely immunocompromised (per the clinician).
    #[serde(default)]
    pub severely_immunocompromised: Option<bool>,
    /// Weight in kilograms, to calculate an immune globulin dose.
    #[serde(default)]
    pub weight_kg: Option<f64>,
    /// Already received immune globulin after this exposure.
    #[serde(default)]
    pub received_immune_globulin: Option<bool>,
    /// Assessment date (YYYY-MM-DD; default today), for whether the PEP windows are open.
    #[serde(default)]
    pub as_of: Option<String>,
}

/// Immunity status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImmunityStatus {
    /// Meets a criterion for presumptive evidence of immunity.
    EvidenceOfImmunity,
    /// Does not meet any criterion (susceptible).
    NoEvidenceOfImmunity,
    /// Not enough information: treat as susceptible until documented.
    Unknown,
}

/// The immunity assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImmunityAssessment {
    pub status: ImmunityStatus,
    pub group: ImmunityGroup,
    pub group_applies_to: String,
    /// The accepted criteria for this group.
    pub criteria: Vec<String>,
    /// Why the status was reached.
    pub reasoning: Vec<String>,
    pub citation: String,
}

/// A post-exposure prophylaxis window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PepWindow {
    pub product: String,
    /// Last calendar day of the window (YYYY-MM-DD).
    pub deadline: String,
    /// Whether the window is still open on the assessment date.
    pub open: bool,
    /// Whether this product is appropriate for this person (None: depends on facts not given).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indicated: Option<bool>,
    pub explanation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dose: Option<String>,
    pub citation: String,
}

/// A matching priority group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PriorityGroupOut {
    pub label: String,
    pub recommendation: String,
    pub citation: String,
}

/// Result of `assess_measles_exposure`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeaslesExposureResult {
    pub as_of: String,
    pub immunity: ImmunityAssessment,
    pub pep: Vec<PepWindow>,
    /// Exclusion rules for this person (105 CMR 300.200).
    pub exclusion: Vec<RuleResult>,
    pub priority_groups: Vec<PriorityGroupOut>,
    /// Watch for symptoms through this date (21 days after last exposure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor_symptoms_through: Option<String>,
    /// Whether the exposure fell in the case's infectious period (None if not checked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure_in_infectious_period: Option<bool>,
    pub notes: Vec<String>,
}

fn citation(source: &str, section: &str) -> String {
    let c = measles::content();
    let title = c
        .sources
        .get(source)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| source.to_string());
    format!("{title}, {section}")
}

fn assess_immunity(a: &MeaslesExposureArgs, group: ImmunityGroup) -> ImmunityAssessment {
    let c = measles::content();
    let crit: &ImmunityCriteria = c
        .evidence_of_immunity
        .iter()
        .find(|e| e.id == group.id())
        .or_else(|| c.evidence_of_immunity.first())
        .expect("measles content has evidence-of-immunity criteria");
    let mut reasoning = Vec::new();
    let mut status = None;
    // Doses before the first birthday do not count, so a dose count cannot
    // make an infant under 12 months immune.
    let infant = a.age_months.is_some_and(|m| m < 12);
    if a.lab_confirmed_measles == Some(true) {
        reasoning.push("Laboratory confirmation of past measles is evidence of immunity.".into());
        status = Some(ImmunityStatus::EvidenceOfImmunity);
    } else if a.positive_measles_igg == Some(true) {
        reasoning.push("A positive measles IgG is laboratory evidence of immunity.".into());
        status = Some(ImmunityStatus::EvidenceOfImmunity);
    } else if !infant
        && let Some(n) = a.documented_mmr_doses
        && n >= crit.doses_required
    {
        reasoning.push(format!(
            "{n} documented dose(s) of measles-containing vaccine meets the {} required for this \
             group (the first at 12 months or older; two doses at least 28 days apart).",
            crit.doses_required
        ));
        status = Some(ImmunityStatus::EvidenceOfImmunity);
    } else if crit.born_before_1957_counts && a.birth_year.is_some_and(|y| y < 1957) {
        reasoning.push(
            "Born before 1957 counts as presumptive evidence of immunity for this group (it may \
             not for people born outside the U.S.; DPH guide)."
                .into(),
        );
        status = Some(ImmunityStatus::EvidenceOfImmunity);
    }
    if status.is_none() {
        if infant {
            reasoning.push(
                "Infants under 12 months have no evidence of immunity (doses before the first \
                 birthday do not count)."
                    .into(),
            );
            if let Some(n) = a.documented_mmr_doses.filter(|n| *n > 0) {
                reasoning.push(format!(
                    "The {n} dose(s) reported were given before the first birthday and were not \
                     counted."
                ));
            }
            status = Some(ImmunityStatus::NoEvidenceOfImmunity);
        } else if let Some(n) = a.documented_mmr_doses {
            reasoning.push(format!(
                "{n} documented dose(s) is fewer than the {} required for this group.",
                crit.doses_required
            ));
            status = Some(ImmunityStatus::NoEvidenceOfImmunity);
        } else if a.positive_measles_igg == Some(false) {
            reasoning.push("Measles IgG was negative or equivocal.".into());
            status = Some(ImmunityStatus::NoEvidenceOfImmunity);
        }
        if group == ImmunityGroup::HealthCarePersonnel && a.birth_year.is_some_and(|y| y < 1957) {
            reasoning.push(
                "Year of birth is not acceptable evidence of immunity for health care workers."
                    .into(),
            );
        }
    }
    let status = status.unwrap_or_else(|| {
        reasoning.push(
            "No vaccination record, titer, or other evidence was given: treat as susceptible \
             until evidence is documented (physician-diagnosed disease alone does not count)."
                .into(),
        );
        ImmunityStatus::Unknown
    });
    ImmunityAssessment {
        status,
        group,
        group_applies_to: crit.applies_to.clone(),
        criteria: crit.criteria.clone(),
        reasoning,
        citation: citation(&crit.source, &crit.section),
    }
}

/// Assess one person's measles exposure.
pub fn assess_measles_exposure(
    _ctx: &Context,
    args: MeaslesExposureArgs,
) -> Result<Envelope<MeaslesExposureResult>> {
    let content = measles::content();
    let rules_content = disease_rules::content();
    let first = parse_date_arg("first_exposure_date", &args.first_exposure_date)?;
    let last = parse_date_arg("last_exposure_date", &args.last_exposure_date)?;
    if first.is_none() && last.is_none() {
        return Err(PhnError::InvalidInput(
            "give first_exposure_date (and last_exposure_date if exposed on several days)".into(),
        ));
    }
    let case_onset = parse_date_arg("case_rash_onset_date", &args.case_rash_onset_date)?;
    let last_case = parse_date_arg("last_case_onset_date", &args.last_case_onset_date)?;
    let as_of =
        parse_date_arg("as_of", &args.as_of)?.unwrap_or_else(|| chrono::Local::now().date_naive());
    if let Some(w) = args.weight_kg
        && !(0.5..=400.0).contains(&w)
    {
        return Err(PhnError::InvalidInput(
            "weight_kg must be between 0.5 and 400".into(),
        ));
    }
    if args.birth_year.is_some_and(|y| !(1900..=2100).contains(&y)) {
        return Err(PhnError::InvalidInput("birth_year looks wrong".into()));
    }
    let dates = DateFacts {
        first_exposure: first,
        last_exposure: last,
        case_onset,
        last_case_onset: last_case,
        ..DateFacts::default()
    };
    dates.validate()?;
    let raw_first: NaiveDate = first.or(last).expect("checked above");
    let raw_last: NaiveDate = last.or(first).expect("checked above");
    // Only exposure during the case's infectious period (4 days before through
    // 4 days after rash onset) counts: PEP windows, exclusion, and monitoring
    // are counted from the exposure dates clamped to that period.
    let infectious = case_onset.map(|o| (add(o, -4), add(o, 4)));
    let clamped = infectious.is_some_and(|(s, e)| {
        raw_last >= s && raw_first <= e && (raw_first < s || raw_last > e)
    });
    let (first_exp, last_exp) = match infectious {
        Some((s, e)) if clamped => (raw_first.max(s), raw_last.min(e)),
        _ => (raw_first, raw_last),
    };
    let dates = DateFacts {
        first_exposure: Some(first_exp),
        last_exposure: Some(last_exp),
        ..dates
    };
    let setting = args.setting.unwrap_or(Setting::Other);
    if setting == Setting::All {
        return Err(PhnError::InvalidInput(
            "choose one setting for the person (child_care, school, food_handler, \
             health_care_worker, or other)"
                .into(),
        ));
    }
    let group = args.immunity_group.unwrap_or(match setting {
        Setting::HealthCareWorker => ImmunityGroup::HealthCarePersonnel,
        Setting::School => ImmunityGroup::SchoolCollegeTravel,
        _ => ImmunityGroup::General,
    });
    let immunity = assess_immunity(&args, group);
    let susceptible = immunity.status != ImmunityStatus::EvidenceOfImmunity;

    // ---- PEP
    let pep_c = &content.pep;
    let mmr_deadline = add(first_exp, i64::from(pep_c.mmr.within_hours / 24));
    let ig_deadline = add(first_exp, i64::from(pep_c.immune_globulin.within_days));
    let infant = args.age_months.is_some_and(|m| m < 12);
    let under_6m = args
        .age_months
        .is_some_and(|m| m < pep_c.mmr.min_age_months);
    let pregnant = args.pregnant == Some(true);
    let immunocomp = args.severely_immunocompromised == Some(true);
    let mmr_indicated = if !susceptible || pregnant || immunocomp || under_6m {
        Some(false)
    } else if args.pregnant.is_none() && args.severely_immunocompromised.is_none() {
        None
    } else {
        Some(true)
    };
    let mut mmr_expl = if !susceptible {
        "Not needed for this exposure: the person has evidence of immunity.".to_string()
    } else if pregnant {
        "MMR is contraindicated in pregnancy.".into()
    } else if immunocomp {
        "MMR is contraindicated for severely immunocompromised people.".into()
    } else if under_6m {
        format!(
            "Infants under {} months cannot receive MMR; use immune globulin.",
            pep_c.mmr.min_age_months
        )
    } else {
        format!(
            "MMR within 72 hours of the first exposure can prevent disease (for infants 6–11 \
             months it may be given in place of IG). Exposure on {} means the window ends 72 \
             hours after the exposure time, i.e. on {} at the same clock time.",
            fmt(first_exp),
            fmt(mmr_deadline)
        )
    };
    if susceptible && !pregnant && !immunocomp && as_of > mmr_deadline {
        mmr_expl.push_str(
            " The window has passed, but vaccinate anyway (if not contraindicated) to protect \
             against later exposures; it does not end the exclusion.",
        );
    }
    let mut pep = vec![PepWindow {
        product: "MMR vaccine".into(),
        deadline: fmt(mmr_deadline),
        open: as_of <= mmr_deadline,
        indicated: mmr_indicated,
        explanation: mmr_expl,
        dose: None,
        citation: citation(&pep_c.mmr.source, &pep_c.mmr.section),
    }];
    let ig_indicated = if !susceptible && !immunocomp {
        Some(false)
    } else if infant || pregnant || immunocomp {
        Some(true)
    } else if args.documented_mmr_doses.is_some_and(|n| n >= 1) {
        Some(false)
    } else {
        None
    };
    let ig_product = if infant {
        "IGIM"
    } else if pregnant || immunocomp {
        "IGIV"
    } else {
        "IGIM or IGIV"
    };
    let ig = &pep_c.immune_globulin;
    let dose = args.weight_kg.map(|w| {
        if ig_product == "IGIV" {
            format!(
                "IGIV {} mg/kg × {} kg = {} mg",
                ig.igiv_mg_per_kg,
                md::num(w, 1),
                md::num(ig.igiv_mg_per_kg * w, 0)
            )
        } else {
            let ml = ig.igim_ml_per_kg * w;
            let capped = ml.min(ig.igim_max_ml);
            format!(
                "IGIM {} mL/kg × {} kg = {} mL{}",
                ig.igim_ml_per_kg,
                md::num(w, 1),
                md::num(ml, 1),
                if ml > ig.igim_max_ml {
                    format!(" → maximum {} mL", md::num(capped, 0))
                } else {
                    String::new()
                }
            )
        }
    });
    let ig_expl = match ig_indicated {
        Some(false) if !susceptible => {
            "Not indicated: the person has evidence of immunity.".to_string()
        }
        Some(false) => "Not indicated after 1 dose of measles vaccine at 12 months or older, \
                        unless severely immunocompromised."
            .to_string(),
        Some(true) => format!(
            "{ig_product} within 6 days of exposure can prevent or modify measles (IGIM {} mL/kg, \
             maximum {} mL; IGIV {} mg/kg). Wait {} months after IGIM or {} months after IGIV \
             before MMR. Receiving IG does not shorten the exclusion.",
            ig.igim_ml_per_kg,
            md::num(ig.igim_max_ml, 0),
            ig.igiv_mg_per_kg,
            pep_c.mmr_after_ig.months_after_igim,
            pep_c.mmr_after_ig.months_after_igiv
        ),
        None => "IG within 6 days can prevent or modify measles in non-immune people; it is \
                 recommended for infants under 12 months, pregnant people without evidence of \
                 immunity, and severely immunocompromised people. Give pregnancy and immune \
                 status to narrow this. IG should not be used to control outbreaks."
            .to_string(),
    };
    pep.push(PepWindow {
        product: format!("Immune globulin ({ig_product})"),
        deadline: fmt(ig_deadline),
        open: as_of <= ig_deadline,
        indicated: ig_indicated,
        explanation: ig_expl,
        dose: if ig_indicated == Some(false) {
            None
        } else {
            dose
        },
        citation: citation(&ig.source, &ig.section),
    });

    // ---- Exclusion
    let disease = engine::find_disease("measles")?;
    let facts = Facts {
        susceptible: Some(susceptible),
        exposure_continuous: args.exposure_continuous,
        received_immune_globulin: args.received_immune_globulin,
        ..Facts::default()
    };
    let exclusion: Vec<RuleResult> = applicable_rules(disease, Role::Contact, &[setting], &facts)
        .into_iter()
        .map(|(r, s)| evaluate(r, s, &facts, &dates))
        .collect();

    // ---- Priority groups
    let mut groups = Vec::new();
    let mut add_group = |id: &str| {
        if let Some(g) = content.priority_groups.iter().find(|g| g.id == id) {
            groups.push(PriorityGroupOut {
                label: g.label.clone(),
                recommendation: g.recommendation.clone(),
                citation: citation(&g.source, &g.section),
            });
        }
    };
    if infant {
        add_group("infant_under_12_months");
    }
    if pregnant && susceptible {
        add_group("pregnant_without_immunity");
    }
    if immunocomp {
        add_group("severely_immunocompromised");
    }
    if susceptible && args.age_months.is_some_and(|m| m > 240) {
        add_group("adults_over_20");
    }

    // ---- Infectious period check
    let in_period = case_onset.map(|o| {
        let (s, e) = (add(o, -4), add(o, 4));
        last_exp >= s && first_exp <= e
    });
    let monitor = disease
        .contact_monitoring
        .as_ref()
        .map(|m| fmt(add(last_exp, i64::from(m.days_after_last_exposure))));

    // ---- Markdown
    let mut s = format!(
        "**Measles exposure assessment** (as of {}; exposure {}{}; setting: {})\n\n",
        fmt_long(as_of),
        fmt(first_exp),
        if last_exp != first_exp {
            format!(" to {}", fmt(last_exp))
        } else {
            String::new()
        },
        setting.label()
    );
    if in_period == Some(false) {
        s.push_str(
            "⚠ The exposure dates fall outside the case's infectious period (4 days before \
             through 4 days after rash onset). Confirm the dates; this may not be an exposure.\n\n",
        );
    }
    let clamp_note = match infectious {
        Some((ps, pe)) if clamped => Some(format!(
            "The exposure dates given ({} to {}) extend outside the case's infectious period ({} \
             through {}: 4 days before through 4 days after rash onset on {}). PEP deadlines, \
             exclusion, and monitoring are counted from exposure within that period: {} to {}.",
            fmt(raw_first),
            fmt(raw_last),
            fmt(ps),
            fmt(pe),
            case_onset.map(fmt).unwrap_or_default(),
            fmt(first_exp),
            fmt(last_exp)
        )),
        _ => None,
    };
    if let Some(n) = &clamp_note {
        s.push_str(&format!("{n}\n\n"));
    }
    s.push_str(&format!(
        "**Evidence of immunity: {}** ({})\n\n",
        match immunity.status {
            ImmunityStatus::EvidenceOfImmunity => "yes — presumptive evidence of immunity",
            ImmunityStatus::NoEvidenceOfImmunity => "no — susceptible",
            ImmunityStatus::Unknown => "unknown — treat as susceptible until documented",
        },
        immunity.group_applies_to
    ));
    for r in &immunity.reasoning {
        s.push_str(&format!("- {r}\n"));
    }
    s.push_str(&format!(
        "- Accepted evidence for this group: {}.\n\n",
        immunity.criteria.join("; ")
    ));
    s.push_str("**Post-exposure prophylaxis**\n\n");
    let rows: Vec<Vec<String>> = pep
        .iter()
        .map(|p| {
            vec![
                p.product.clone(),
                format!(
                    "{} ({})",
                    p.deadline,
                    if p.open { "open" } else { "closed" }
                ),
                match p.indicated {
                    Some(true) => "yes".into(),
                    Some(false) => "no".into(),
                    None => "depends (see note)".into(),
                },
                format!(
                    "{}{}",
                    p.explanation,
                    p.dose
                        .as_ref()
                        .map(|d| format!(" Dose: {d}."))
                        .unwrap_or_default()
                ),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &["Product", "Deadline", "Indicated", "Notes"],
        &rows,
    ));
    s.push_str("\n**Exclusion (105 CMR 300.200)**\n\n");
    if exclusion.is_empty() {
        s.push_str("No exclusion applies to this person with the facts given.\n");
    }
    for r in &exclusion {
        let when = match (&r.start_date, &r.last_excluded_date) {
            (Some(a), Some(b)) => format!(
                "exclude {a} through {b}; may return {}",
                r.earliest_return_date.clone().unwrap_or_default()
            ),
            (None, Some(b)) => format!("exclude through {b}"),
            _ if !r.missing_dates.is_empty() => {
                format!("dates need {}", r.missing_dates.join(", "))
            }
            _ => r.action.label().to_string(),
        };
        s.push_str(&format!(
            "- {}{}: **{when}**. “{}” — {}\n",
            r.applies_to,
            if r.definite {
                String::new()
            } else {
                format!(" (only if {})", r.conditions_unknown.join(" and "))
            },
            r.wording,
            r.citation
        ));
        if let Some(n) = &r.note {
            s.push_str(&format!("  - Note: {n}\n"));
        }
    }
    if !groups.is_empty() {
        s.push_str("\n**Priority group(s)**\n\n");
        for g in &groups {
            s.push_str(&format!(
                "- {}: {} ({})\n",
                g.label, g.recommendation, g.citation
            ));
        }
    }
    if let Some(m) = &monitor {
        s.push_str(&format!(
            "\n**Monitoring:** watch for fever, cough, coryza, conjunctivitis, and rash through \
             {m} (21 days after the last exposure). If symptoms start, the person should call \
             ahead before seeking care.\n"
        ));
    }
    let mut notes: Vec<String> = content
        .immunity_notes
        .iter()
        .map(|n| format!("{} ({})", n.text, citation(&n.source, &n.section)))
        .collect();
    notes.extend(
        content
            .exclusion_summary
            .iter()
            .map(|n| format!("{} ({})", n.text, citation(&n.source, &n.section))),
    );
    s.push_str("\n**Program notes**\n\n");
    for n in &notes {
        s.push_str(&format!("- {n}\n"));
    }

    // ---- Sources and caveats
    let mut sources: Vec<Source> = content
        .sources
        .values()
        .map(|s| Source {
            title: s.title.clone(),
            publisher: s.publisher.clone(),
            url: Some(s.url.clone()),
            effective_date: s.effective_date.clone(),
            retrieved: Some(s.retrieved.clone()),
            mode: SourceMode::Snapshot,
        })
        .collect();
    sources.sort_by(|a, b| a.title.cmp(&b.title));
    let mut caveats = vec![
        format!(
            "REVIEW STATUS: the measles exposure content is a PENDING-REVIEW draft (version {}, \
             prepared {}); the ACIP statement is from 2013. {}",
            content.meta.version, content.meta.prepared, content.meta.verification_note
        ),
        engine::pending_caveat(rules_content),
        "PEP windows are counted from the FIRST exposure. With dates only, the 72-hour and \
         6-day deadlines are the calendar dates on which they end; use the exposure time for \
         precision."
            .into(),
        "Measles is reportable immediately by phone: call the local board of health and the DPH epidemiologist on call, (617) 983-6800, 24/7 (DPH measles guide). Coordinate PEP, \
         IG supply, and exclusion decisions with DPH epidemiology."
            .into(),
    ];
    if let Some(n) = clamp_note {
        caveats.push(n);
    }
    let data = MeaslesExposureResult {
        as_of: fmt(as_of),
        immunity,
        pep,
        exclusion,
        priority_groups: groups,
        monitor_symptoms_through: monitor,
        exposure_in_infectious_period: in_period,
        notes,
    };
    Ok(Envelope::new(s, data)
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
    fn unvaccinated_student_within_72_hours() {
        let (_d, c) = ctx();
        let env = assess_measles_exposure(
            &c,
            MeaslesExposureArgs {
                first_exposure_date: Some("2026-03-02".into()),
                setting: Some(Setting::School),
                documented_mmr_doses: Some(0),
                age_months: Some(120),
                pregnant: Some(false),
                severely_immunocompromised: Some(false),
                as_of: Some("2026-03-04".into()),
                ..MeaslesExposureArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.immunity.status, ImmunityStatus::NoEvidenceOfImmunity);
        assert_eq!(d.pep[0].deadline, "2026-03-05");
        assert!(d.pep[0].open && d.pep[0].indicated == Some(true));
        assert_eq!(d.pep[1].deadline, "2026-03-08");
        let ex = d
            .exclusion
            .iter()
            .find(|r| r.rule_id == "contact_susceptible")
            .unwrap();
        assert_eq!(ex.start_date.as_deref(), Some("2026-03-07"));
        assert_eq!(ex.last_excluded_date.as_deref(), Some("2026-03-23"));
        assert_eq!(d.monitor_symptoms_through.as_deref(), Some("2026-03-23"));
        assert!(env.notice.is_some());
    }

    #[test]
    fn infant_with_a_dose_before_the_first_birthday_is_not_immune() {
        let (_d, c) = ctx();
        let env = assess_measles_exposure(
            &c,
            MeaslesExposureArgs {
                first_exposure_date: Some("2026-03-02".into()),
                age_months: Some(9),
                documented_mmr_doses: Some(1),
                setting: Some(Setting::ChildCare),
                as_of: Some("2026-03-03".into()),
                ..MeaslesExposureArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.immunity.status, ImmunityStatus::NoEvidenceOfImmunity);
        assert!(
            d.immunity
                .reasoning
                .iter()
                .any(|r| r.contains("before the first birthday"))
        );
        // Immune globulin is indicated and the exclusion applies.
        assert_eq!(d.pep[1].indicated, Some(true));
        assert!(
            d.exclusion
                .iter()
                .any(|r| r.action == phn_content::disease_rules::Action::Exclude)
        );
    }

    #[test]
    fn exposure_before_the_infectious_period_does_not_start_the_clocks() {
        let (_d, c) = ctx();
        // Household contact: together since March 1; the case's rash began
        // March 10, so the infectious period is March 6 to 14.
        let env = assess_measles_exposure(
            &c,
            MeaslesExposureArgs {
                first_exposure_date: Some("2026-03-01".into()),
                last_exposure_date: Some("2026-03-12".into()),
                case_rash_onset_date: Some("2026-03-10".into()),
                setting: Some(Setting::School),
                documented_mmr_doses: Some(0),
                age_months: Some(120),
                pregnant: Some(false),
                severely_immunocompromised: Some(false),
                as_of: Some("2026-03-08".into()),
                ..MeaslesExposureArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.exposure_in_infectious_period, Some(true));
        assert_eq!(d.pep[0].deadline, "2026-03-09");
        assert!(d.pep[0].open);
        assert_eq!(d.pep[1].deadline, "2026-03-12");
        assert!(d.pep[1].open);
        let ex = d
            .exclusion
            .iter()
            .find(|r| r.rule_id == "contact_susceptible")
            .unwrap();
        assert_eq!(ex.start_date.as_deref(), Some("2026-03-11"));
        assert_eq!(ex.last_excluded_date.as_deref(), Some("2026-04-02"));
        assert!(env.summary.contains("2026-03-06 to 2026-03-12"));
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("infectious period"))
        );
    }

    #[test]
    fn infant_gets_igim_dose_capped() {
        let (_d, c) = ctx();
        let env = assess_measles_exposure(
            &c,
            MeaslesExposureArgs {
                first_exposure_date: Some("2026-03-02".into()),
                age_months: Some(4),
                weight_kg: Some(6.0),
                setting: Some(Setting::ChildCare),
                as_of: Some("2026-03-06".into()),
                ..MeaslesExposureArgs::default()
            },
        )
        .unwrap();
        let ig = &env.data.pep[1];
        assert_eq!(ig.indicated, Some(true));
        assert!(
            ig.dose.as_ref().unwrap().contains("3.0 mL"),
            "{:?}",
            ig.dose
        );
        assert_eq!(env.data.pep[0].indicated, Some(false));
        assert!(!env.data.pep[0].open);
        assert!(!env.data.priority_groups.is_empty());
        // Adult heavy enough to hit the 15 mL cap.
        let env = assess_measles_exposure(
            &c,
            MeaslesExposureArgs {
                first_exposure_date: Some("2026-03-02".into()),
                age_months: Some(400),
                weight_kg: Some(80.0),
                documented_mmr_doses: Some(0),
                as_of: Some("2026-03-06".into()),
                ..MeaslesExposureArgs::default()
            },
        )
        .unwrap();
        assert!(
            env.data.pep[1]
                .dose
                .as_ref()
                .unwrap()
                .contains("maximum 15 mL")
        );
    }

    #[test]
    fn health_care_worker_born_before_1957_is_not_immune_by_birth_year() {
        let (_d, c) = ctx();
        let env = assess_measles_exposure(
            &c,
            MeaslesExposureArgs {
                first_exposure_date: Some("2026-03-02".into()),
                setting: Some(Setting::HealthCareWorker),
                birth_year: Some(1950),
                as_of: Some("2026-03-02".into()),
                ..MeaslesExposureArgs::default()
            },
        )
        .unwrap();
        assert_ne!(env.data.immunity.status, ImmunityStatus::EvidenceOfImmunity);
        let env = assess_measles_exposure(
            &c,
            MeaslesExposureArgs {
                first_exposure_date: Some("2026-03-02".into()),
                birth_year: Some(1950),
                as_of: Some("2026-03-02".into()),
                ..MeaslesExposureArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.immunity.status, ImmunityStatus::EvidenceOfImmunity);
        assert!(
            env.data
                .exclusion
                .iter()
                .all(|r| r.action != phn_content::disease_rules::Action::Exclude)
        );
        assert!(assess_measles_exposure(&c, MeaslesExposureArgs::default()).is_err());
    }
}
