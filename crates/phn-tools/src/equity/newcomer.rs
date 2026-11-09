//! EQ-11 `newcomer_health_navigator`: refugee and immigrant health — CDC
//! domestic screening components with age rules and the 90-day window,
//! Massachusetts programs (DPH, ORI: RHAP, RHPP, RMA), MassHealth coverage by
//! immigration status including the October 2026 federal changes, and family
//! shelter resources. Uses no identifiers; writes an optional blank screening
//! checklist (with agency translations) to `/data/output`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::core::practice::{self, CitedNote, CitedText, Confidence, ContentSource};
use crate::md;

/// Content id.
pub const NEWCOMER_CONTENT: &str = "newcomer-health";

#[derive(Debug, Clone, Deserialize)]
struct ScreeningTopic {
    id: String,
    label: String,
    items: Vec<CitedText>,
}

#[derive(Debug, Clone, Deserialize)]
struct Program {
    id: String,
    name: String,
    description: String,
    contact: String,
    source: String,
    section: String,
    #[serde(default)]
    confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Deserialize)]
struct Status {
    id: String,
    label: String,
    masshealth_group: String,
    oct_2026_change: String,
    ori_eligible: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct NewcomerContent {
    meta: ContentMeta,
    sources: BTreeMap<String, ContentSource>,
    screening: Vec<ScreeningTopic>,
    programs: Vec<Program>,
    ori_eligibility: CitedText,
    statuses: Vec<Status>,
    coverage_rules: Vec<CitedText>,
    shelter: Vec<CitedText>,
    language_access: CitedText,
}

static CONTENT: LazyLock<NewcomerContent> = LazyLock::new(|| {
    practice::load(NEWCOMER_CONTENT).expect("newcomer content matches the Rust types")
});

fn content() -> &'static NewcomerContent {
    &CONTENT
}

/// What to cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NewcomerTopic {
    /// CDC domestic medical screening (all components).
    Screening,
    /// Massachusetts refugee programs (RHAP, RHPP, RMA, ORI, DPH).
    Programs,
    /// MassHealth coverage by immigration status.
    Coverage,
    /// Emergency Assistance family shelter and related resources.
    Shelter,
    /// Everything.
    All,
}

/// Arguments for `newcomer_health_navigator`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct NewcomerArgs {
    /// screening, programs, coverage, shelter, or all (default all).
    pub topics: Vec<NewcomerTopic>,
    /// Immigration status for coverage: refugee, asylee, cuban_haitian_entrant,
    /// special_immigrant_visa, trafficking_victim, withholding_of_removal,
    /// cofa_migrant, lpr_5_years_or_more, lpr_under_5_years, parolee_1_year_plus,
    /// parolee_under_1_year, temporary_protected_status,
    /// asylum_applicant_with_work_permit, asylum_applicant_no_work_permit, daca,
    /// nonimmigrant_visa, or undocumented.
    pub status: Option<String>,
    /// Age in years (screening age rules and coverage).
    pub age_years: Option<u32>,
    /// Pregnant or within 12 months postpartum.
    pub pregnant: bool,
    /// Has a disability (coverage rules for adults).
    pub disabled: bool,
    /// U.S. arrival date, YYYY-MM-DD (90-day screening window).
    pub arrival_date: Option<String>,
    /// The person's preferred language (interpreter reminder; an agency
    /// translation of the checklist is used from
    /// /data/templates/<language>/newcomer_checklist.md when present).
    pub preferred_language: Option<String>,
    /// Write a blank domestic screening checklist (DOCX) to /data/output.
    pub write_checklist: bool,
    /// Evaluate dates as of this day (default today).
    pub as_of: Option<String>,
}

/// A screening component, with whether it applies at the age given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreeningComponent {
    pub id: String,
    pub label: String,
    /// Age-based note ("applies: ...") when an age was given.
    pub applies: Option<String>,
    pub guidance: Vec<CitedNote>,
}

/// A program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramOut {
    pub id: String,
    pub name: String,
    pub description: String,
    pub contact: String,
    pub citation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// Coverage assessment for a status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageAssessment {
    pub status: String,
    pub status_label: String,
    pub masshealth_group: String,
    /// group_1 (not affected), group_2 (affected), group_2_after_5_years, or not_listed.
    pub october_2026_change: String,
    pub ori_eligible: bool,
    /// The likely coverage path from the rules (MassHealth decides).
    pub likely_coverage: String,
    pub rules: Vec<CitedNote>,
}

/// Result of `newcomer_health_navigator`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewcomerResult {
    /// Last day of the 90-day screening window, when arrival_date was given.
    pub screening_due_by: Option<String>,
    pub days_since_arrival: Option<i64>,
    pub screening: Vec<ScreeningComponent>,
    pub programs: Vec<ProgramOut>,
    pub ori_eligibility: Option<CitedNote>,
    pub coverage: Option<CoverageAssessment>,
    /// Coverage rules, when no status was given.
    pub coverage_rules: Vec<CitedNote>,
    pub shelter: Vec<CitedNote>,
    pub language_access: CitedNote,
    /// Language of the checklist file, and the agency translation used.
    pub checklist_language: Option<String>,
    pub files: Vec<String>,
}

fn find_status(q: &str) -> Option<&'static Status> {
    let c = content();
    let n = phn_core::text::normalize(q).replace([' ', '-'], "_");
    c.statuses
        .iter()
        .find(|s| s.id == n)
        .or_else(|| {
            let aliases: &[(&str, &str)] = &[
                ("green_card", "lpr_5_years_or_more"),
                ("lpr", "lpr_5_years_or_more"),
                ("tps", "temporary_protected_status"),
                ("siv", "special_immigrant_visa"),
                ("parole", "parolee_1_year_plus"),
                ("humanitarian_parole", "parolee_1_year_plus"),
                ("asylum_seeker", "asylum_applicant_no_work_permit"),
                ("asylum_applicant", "asylum_applicant_no_work_permit"),
                ("cofa", "cofa_migrant"),
                ("cuban_haitian", "cuban_haitian_entrant"),
                ("no_status", "undocumented"),
                ("other_noncitizen", "undocumented"),
                ("student_visa", "nonimmigrant_visa"),
            ];
            aliases
                .iter()
                .find(|(a, _)| *a == n)
                .and_then(|(_, id)| c.statuses.iter().find(|s| &s.id == id))
        })
        .or_else(|| {
            c.statuses.iter().find(|s| {
                phn_core::text::normalize(&s.label).starts_with(&phn_core::text::normalize(q))
            })
        })
}

fn applies(id: &str, age: Option<u32>, pregnant: bool) -> Option<String> {
    let a = age?;
    Some(match id {
        "tuberculosis" => {
            if a < 2 {
                "under 2: TST if no recent negative overseas test".into()
            } else {
                "2 or older: IGRA if no recent negative overseas test".into()
            }
        }
        "lead" => {
            if a <= 6 {
                "yes: blood lead test now and again in 3-6 months".into()
            } else if a <= 16 || pregnant {
                "yes: blood lead test".into()
            } else if a == 17 {
                "only with suspected exposure".into()
            } else {
                "not routine (unless pregnant or lactating)".into()
            }
        }
        "hiv" => {
            if (13..=64).contains(&a) {
                "yes: opt-out screening recommended".into()
            } else {
                "encouraged (outside 13-64)".into()
            }
        }
        "hepatitis" => {
            if a >= 18 || pregnant {
                "hepatitis B and C screening".into()
            } else {
                "hepatitis B screening; hepatitis C if risk factors".into()
            }
        }
        "history_physical" => {
            let mut v = vec!["history and examination"];
            if a >= 3 {
                v.push("blood pressure and vision");
            }
            if a >= 4 {
                v.push("hearing");
            }
            v.join("; ")
        }
        "mental_health" => {
            if a >= 18 {
                "standardized adult screening tool".into()
            } else {
                "structured or semi-structured assessment".into()
            }
        }
        "cancer" => {
            if a >= 21 {
                "per USPSTF by age and sex".into()
            } else {
                "generally not yet (USPSTF ages)".into()
            }
        }
        _ => return None,
    })
}

fn likely_coverage(s: &Status, age: Option<u32>, pregnant: bool, disabled: bool) -> String {
    if pregnant {
        return "Pregnant or postpartum: full MassHealth benefits during pregnancy and for 12 \
                months postpartum if otherwise eligible, in any noncitizen group (not affected by \
                the October 2026 changes)."
            .into();
    }
    let child = age.is_some_and(|a| a < 21);
    if child {
        return if s.id == "undocumented" {
            if age.is_some_and(|a| a < 19) {
                "Child under 19 without a qualifying status: may be able to get the Children's \
                 Medical Security Plan (CMSP); the Health Safety Net and MassHealth Limited \
                 (emergency services) may also apply."
                    .into()
            } else {
                "Age 19-20 without a qualifying status: MassHealth Limited (emergency services) \
                 and the Health Safety Net generally."
                    .into()
            }
        } else {
            "Child under 21 who is lawfully present or PRUCOL: may be eligible for comprehensive \
             MassHealth benefits (children are protected from the October 2026 changes)."
                .into()
        };
    }
    match s.oct_2026_change.as_str() {
        "group_2" | "group_2_after_5_years" => {
            let base = if s.oct_2026_change == "group_2_after_5_years" {
                "After the five-year bar is met this status is in Group 2: "
            } else {
                "Group 2 (affected from October 1, 2026): "
            };
            match (age, disabled) {
                (Some(a), _) if a >= 65 => {
                    format!("{base}age 65 or older generally gets MassHealth Family Assistance.")
                }
                (_, true) => format!(
                    "{base}an adult with a disability generally gets MassHealth Family Assistance."
                ),
                (Some(_), false) => format!(
                    "{base}an adult 21-64 without a disability generally gets only MassHealth \
                     Limited (emergency services) and/or the Health Safety Net. Recently arrived \
                     refugees not eligible for MassHealth may qualify for Refugee Medical \
                     Assistance (ORI)."
                ),
                (None, false) => format!(
                    "{base}adults 21-64 without a disability generally get MassHealth Limited \
                     and/or the Health Safety Net; with a disability or 65 and older, Family \
                     Assistance. Give age_years for a specific answer."
                ),
            }
        }
        "group_1" => "Group 1 (not affected by the October 2026 changes): eligibility rules are \
                      unchanged; MassHealth determines the coverage type from age, disability, and \
                      income."
            .into(),
        _ if s.id == "undocumented" => "Adult without a qualifying status: MassHealth Limited \
                                        (emergency services) and the Health Safety Net generally; \
                                        confirm with MassHealth."
            .into(),
        _ => format!(
            "{}: not changed by EOM 26-10. The coverage type (for example Family Assistance or \
             Limited) depends on the group, age, disability, and income; MassHealth decides.",
            s.masshealth_group
        ),
    }
}

/// Newcomer (refugee and immigrant) health navigation.
pub fn newcomer_health_navigator(
    ctx: &Context,
    args: NewcomerArgs,
) -> Result<Envelope<NewcomerResult>> {
    let c = content();
    let today = practice::date_arg("as_of", &args.as_of)?
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    let arrival = practice::date_arg("arrival_date", &args.arrival_date)?;
    if arrival.is_some_and(|a| a > today) {
        return Err(PhnError::InvalidInput(
            "arrival_date is after today (or as_of)".into(),
        ));
    }
    let topics: Vec<NewcomerTopic> =
        if args.topics.is_empty() || args.topics.contains(&NewcomerTopic::All) {
            vec![
                NewcomerTopic::Screening,
                NewcomerTopic::Programs,
                NewcomerTopic::Coverage,
                NewcomerTopic::Shelter,
            ]
        } else {
            args.topics.clone()
        };
    let want = |t: NewcomerTopic| topics.contains(&t);
    let status = match args.status.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(q) => Some(find_status(q).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "unknown status '{q}'; use one of: {}",
                c.statuses
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?),
        None => None,
    };
    let mut caveats: Vec<String> = practice::review_caveat(&c.meta).into_iter().collect();
    let mut keys: Vec<String> = vec!["cdc-domestic".into()];

    let due: Option<NaiveDate> = arrival.map(|a| a + chrono::Duration::days(90));
    let days_since = arrival.map(|a| (today - a).num_days());
    let screening: Vec<ScreeningComponent> = if want(NewcomerTopic::Screening) {
        c.screening
            .iter()
            .map(|t| {
                keys.extend(t.items.iter().map(|i| i.source.clone()));
                ScreeningComponent {
                    id: t.id.clone(),
                    label: t.label.clone(),
                    applies: applies(&t.id, args.age_years, args.pregnant),
                    guidance: practice::notes(&t.items, &c.sources),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let programs: Vec<ProgramOut> = if want(NewcomerTopic::Programs) {
        c.programs
            .iter()
            .map(|p| {
                keys.push(p.source.clone());
                ProgramOut {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    description: p.description.clone(),
                    contact: p.contact.clone(),
                    citation: practice::cite(&c.sources, &p.source, &p.section),
                    confidence: p.confidence,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    if programs
        .iter()
        .any(|p| p.confidence == Some(Confidence::Low))
    {
        caveats.push(
            "ORI's pages disagree on how long Refugee Medical Assistance lasts (8 vs 4 months); \
             confirm with ORI."
                .into(),
        );
    }
    let coverage_rules = practice::notes(&c.coverage_rules, &c.sources);
    let coverage = if want(NewcomerTopic::Coverage) {
        keys.extend(c.coverage_rules.iter().map(|r| r.source.clone()));
        keys.push("mh-groups".into());
        status.map(|s| CoverageAssessment {
            status: s.id.clone(),
            status_label: s.label.clone(),
            masshealth_group: s.masshealth_group.clone(),
            october_2026_change: s.oct_2026_change.clone(),
            ori_eligible: s.ori_eligible,
            likely_coverage: likely_coverage(s, args.age_years, args.pregnant, args.disabled),
            rules: coverage_rules.clone(),
        })
    } else {
        None
    };
    let shelter = if want(NewcomerTopic::Shelter) {
        keys.extend(c.shelter.iter().map(|r| r.source.clone()));
        practice::notes(&c.shelter, &c.sources)
    } else {
        Vec::new()
    };

    // ---- Language and checklist
    let lang = args
        .preferred_language
        .clone()
        .filter(|l| !l.trim().is_empty());
    let mut files = Vec::new();
    let mut checklist_language = None;
    if args.write_checklist {
        let translation = lang
            .as_deref()
            .and_then(|l| practice::agency_template(ctx, l, "newcomer_checklist.md"));
        let mut blocks = vec![practice::para(
            "Name / ID: (local record)    Arrival date: __________    Interpreter language: __________",
        )];
        if let Some((text, p)) = &translation {
            blocks.extend(phn_docs::model::from_markdown(
                &practice::fill_placeholders(
                    text,
                    &[(
                        "screening_due_by",
                        due.map(|d| d.to_string()).unwrap_or_default(),
                    )],
                ),
            ));
            checklist_language = Some(format!(
                "{} (agency translation {p})",
                lang.clone().unwrap_or_default()
            ));
        } else {
            checklist_language = Some("English".into());
            if let Some(l) = lang
                .as_deref()
                .filter(|l| practice::language_folder(l).is_some())
            {
                caveats.push(format!(
                    "No agency translation was found for '{l}' (expected \
                     templates/{}/newcomer_checklist.md in the data folder); the checklist is in \
                     English. Use a qualified interpreter; do not use machine translation.",
                    practice::language_folder(l).unwrap_or_default()
                ));
            }
        }
        for t in &c.screening {
            blocks.push(practice::heading(2, t.label.clone()));
            blocks.push(practice::bullets(
                t.items.iter().map(|i| format!("☐ {}", i.text)),
            ));
        }
        let doc = practice::document(ctx, "Domestic refugee health screening checklist", blocks);
        files = practice::write_document(ctx, &doc, "newcomer_screening_checklist", &["docx"])?;
    }

    // ---- Markdown
    let mut s = String::from("**Newcomer health navigator**\n\n");
    if let (Some(d), Some(n)) = (due, days_since) {
        s.push_str(&format!(
            "Arrived {n} day(s) ago; the CDC domestic screening window ends **{d}** (90 days).{}\n\n",
            if n > 90 {
                " The window has passed: complete the screening as soon as possible."
            } else {
                ""
            }
        ));
    }
    if let Some(l) = &lang {
        s.push_str(&format!(
            "Use a qualified medical interpreter for {l}; do not rely on family members or \
             machine translation.\n\n"
        ));
    }
    if !screening.is_empty() {
        s.push_str("**Domestic medical screening (CDC)**\n\n");
        let rows: Vec<Vec<String>> = screening
            .iter()
            .map(|t| {
                vec![
                    t.label.clone(),
                    t.applies.clone().unwrap_or_else(|| "—".into()),
                    t.guidance
                        .first()
                        .map(|g| g.text.clone())
                        .unwrap_or_default(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Component", "At this age", "Key point"],
            &rows,
        ));
    }
    if let Some(cv) = &coverage {
        s.push_str(&format!(
            "\n**MassHealth coverage — {}** ({}; ORI-eligible: {})\n\n{}\n",
            cv.status_label,
            cv.masshealth_group,
            if cv.ori_eligible { "yes" } else { "no" },
            cv.likely_coverage
        ));
    } else if want(NewcomerTopic::Coverage) {
        s.push_str(
            "\n**MassHealth coverage:** give `status` (and age, pregnancy, disability) for the \
             likely path. From October 1, 2026, adult refugees, asylees, and several other \
             statuses lose comprehensive MassHealth (children under 21 and pregnant or \
             postpartum people keep it).\n",
        );
    }
    if !programs.is_empty() {
        s.push_str("\n**Massachusetts programs**\n");
        for p in &programs {
            s.push_str(&format!(
                "- **{}** — {} Contact: {}\n",
                p.name, p.description, p.contact
            ));
        }
    }
    if !shelter.is_empty() {
        s.push_str("\n**Family shelter and basic needs**\n");
        for n in &shelter {
            s.push_str(&format!("- {}\n", n.text));
        }
    }
    if !files.is_empty() {
        s.push_str(&format!("\n**Checklist:** {}\n", files.join(", ")));
    }
    caveats.push(
        "Immigration-related benefit rules changed in 2025-2026 and are still changing; check \
         the MassHealth noncitizen page and current EOMs, and refer to an immigration legal \
         service (ORI's list) for status questions. MassHealth determines eligibility."
            .into(),
    );
    caveats.push(
        "Screening guidance is CDC's for refugees; local protocols (RHAP) may adapt it. Clinical \
         decisions belong to the screening provider."
            .into(),
    );
    keys.push(c.language_access.source.clone());
    let sources = practice::sources_for(&c.sources, keys.iter().map(String::as_str));
    let data = NewcomerResult {
        screening_due_by: due.map(|d| d.to_string()),
        days_since_arrival: days_since,
        screening,
        programs,
        ori_eligibility: want(NewcomerTopic::Programs)
            .then(|| practice::note(&c.ori_eligibility, &c.sources)),
        coverage_rules: if coverage.is_none() && want(NewcomerTopic::Coverage) {
            coverage_rules
        } else {
            Vec::new()
        },
        coverage,
        shelter,
        language_access: practice::note(&c.language_access, &c.sources),
        checklist_language,
        files: files.clone(),
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            files_written: files,
            notes: vec![
                "No identifiers are used; give status and age only. The checklist is blank for \
                 the person's details."
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

    #[test]
    fn refugee_adult_after_october_2026_gets_limited_or_hsn() {
        let (_d, c) = ctx(false);
        let env = newcomer_health_navigator(
            &c,
            NewcomerArgs {
                status: Some("refugee".into()),
                age_years: Some(34),
                arrival_date: Some("2026-10-01".into()),
                as_of: Some("2026-11-08".into()),
                ..NewcomerArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.screening_due_by.as_deref(), Some("2026-12-30"));
        assert_eq!(d.days_since_arrival, Some(38));
        let cv = d.coverage.as_ref().unwrap();
        assert_eq!(cv.october_2026_change, "group_2");
        assert!(cv.likely_coverage.contains("Limited"));
        assert!(cv.ori_eligible);
        let lead = d.screening.iter().find(|s| s.id == "lead").unwrap();
        assert!(lead.applies.as_deref().unwrap().starts_with("not routine"));
        assert!(env.notice.is_some());
    }

    #[test]
    fn children_pregnancy_and_group_1_are_protected() {
        let (_d, c) = ctx(false);
        let run = |status: &str, age: u32, pregnant: bool| {
            newcomer_health_navigator(
                &c,
                NewcomerArgs {
                    status: Some(status.into()),
                    age_years: Some(age),
                    pregnant,
                    topics: vec![NewcomerTopic::Coverage],
                    ..NewcomerArgs::default()
                },
            )
            .unwrap()
            .data
            .coverage
            .unwrap()
            .likely_coverage
        };
        assert!(run("asylee", 10, false).contains("comprehensive"));
        assert!(run("asylee", 30, true).contains("12 months postpartum"));
        assert!(run("Cuban/Haitian entrant", 40, false).contains("Group 1"));
        assert!(run("undocumented", 8, false).contains("CMSP"));
        assert!(run("tps", 40, false).contains("not changed"));
        assert!(run("asylee", 70, false).contains("Family Assistance"));
        let e = newcomer_health_navigator(
            &c,
            NewcomerArgs {
                status: Some("martian".into()),
                ..NewcomerArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("refugee"));
    }

    #[test]
    fn checklist_falls_back_to_english() {
        let (_d, c) = ctx(true);
        let env = newcomer_health_navigator(
            &c,
            NewcomerArgs {
                write_checklist: true,
                preferred_language: Some("Dari".into()),
                topics: vec![NewcomerTopic::Screening],
                ..NewcomerArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.files.len(), 1);
        assert_eq!(env.data.checklist_language.as_deref(), Some("English"));
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("templates/dari/newcomer_checklist.md"))
        );
        assert!(env.data.coverage.is_none());
    }
}
