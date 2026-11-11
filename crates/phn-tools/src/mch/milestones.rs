//! MCH-02 `check_developmental_milestones`.
//!
//! CDC "Learn the Signs. Act Early." milestones (2022 revision) for a child's age, flags for
//! milestones not yet seen or lost skills, the act-early steps, AAP screening ages, and
//! Massachusetts Early Intervention referral with the EI programs that serve the family's town
//! (snapshot `ma-ei-programs`).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self as mc, CitedText, NoteOut, SourceRef};
use crate::context::Context;
use crate::md;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Domains {
    social_emotional: Vec<String>,
    language_communication: Vec<String>,
    cognitive: Vec<String>,
    movement_physical: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgeChecklist {
    age_months: u32,
    label: String,
    url: String,
    updated: String,
    domains: Domains,
}

#[derive(Debug, Clone, Deserialize)]
struct Screening {
    general_months: Vec<u32>,
    autism_months: Vec<u32>,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MilestoneContent {
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    definition: CitedText,
    share_with_doctor: Vec<String>,
    act_early: Vec<CitedText>,
    screening: Screening,
    early_intervention: Vec<CitedText>,
    ei_town_aliases: BTreeMap<String, String>,
    ages: Vec<AgeChecklist>,
}

static CONTENT: LazyLock<MilestoneContent> =
    LazyLock::new(|| mc::load::<MilestoneContent>("mch-developmental-milestones"));

fn content() -> &'static MilestoneContent {
    &CONTENT
}

const DOMAINS: [(&str, &str); 4] = [
    ("social", "Social/emotional"),
    ("language", "Language/communication"),
    (
        "cognitive",
        "Cognitive (learning, thinking, problem-solving)",
    ),
    ("movement", "Movement/physical development"),
];

impl AgeChecklist {
    fn domain(&self, i: usize) -> &[String] {
        match i {
            0 => &self.domains.social_emotional,
            1 => &self.domains.language_communication,
            2 => &self.domains.cognitive,
            _ => &self.domains.movement_physical,
        }
    }
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `check_developmental_milestones`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct MilestoneArgs {
    /// Child's age in months (decimals allowed, e.g. 9.5). CDC checklists cover 2 months to
    /// 5 years; the checklist for the most recent age reached is used.
    pub age_months: f64,
    /// Gestational age at birth in weeks, if born early (under 37 weeks shows the checklist for
    /// the corrected age as well).
    #[serde(default)]
    pub gestational_age_weeks: Option<u32>,
    /// Milestone IDs the child does not yet do (IDs come from this tool's checklist, e.g.
    /// "18m-language-2").
    #[serde(default)]
    pub not_yet: Vec<String>,
    /// The child has lost skills he or she once had.
    #[serde(default)]
    pub lost_skills: bool,
    /// The family or nurse has other developmental concerns.
    #[serde(default)]
    pub concern: bool,
    /// The family's city or town, to list the Early Intervention programs that serve it.
    #[serde(default)]
    pub town: Option<String>,
}

/// One milestone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MilestoneItem {
    /// Stable ID to pass back in not_yet (age-domain-number).
    pub id: String,
    pub domain: String,
    pub text: String,
    /// True when listed in not_yet.
    pub not_yet: bool,
}

/// A CDC checklist for one age.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Checklist {
    pub age_label: String,
    pub age_months: u32,
    pub source_url: String,
    pub source_updated: String,
    pub milestones: Vec<MilestoneItem>,
}

/// An Early Intervention program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EiProgram {
    pub name: String,
    pub phone: String,
    pub website: String,
    pub address: String,
    /// The cities and towns it serves as DPH lists them (Boston programs list neighborhood
    /// exceptions).
    pub catchment_area: String,
}

/// Result of `check_developmental_milestones`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MilestoneResult {
    pub checklist: Checklist,
    /// The checklist for the corrected age, when the child was born early and the ages differ.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrected_age_checklist: Option<Checklist>,
    /// The next CDC checklist age.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_checklist: Option<String>,
    /// True when any milestone is not yet met, skills were lost, or there is a concern.
    pub act_early: bool,
    pub not_yet_count: usize,
    /// IDs in not_yet that are not on this checklist.
    pub unknown_ids: Vec<String>,
    pub act_early_steps: Vec<NoteOut>,
    /// Standardized screening due at this age (AAP schedule, as stated by CDC).
    pub screening_due: Vec<String>,
    pub screening_note: NoteOut,
    pub early_intervention: Vec<NoteOut>,
    pub ei_programs: Vec<EiProgram>,
    pub questions_for_the_doctor: Vec<String>,
    pub definition: NoteOut,
}

// ---------------------------------------------------------------- logic

fn checklist_for(months: f64) -> Option<&'static AgeChecklist> {
    content()
        .ages
        .iter()
        .rev()
        .find(|a| months + 1e-9 >= f64::from(a.age_months))
}

fn build(a: &AgeChecklist, not_yet: &[String]) -> Checklist {
    let mut milestones = Vec::new();
    for (i, (key, label)) in DOMAINS.iter().enumerate() {
        for (n, text) in a.domain(i).iter().enumerate() {
            let id = format!("{}m-{key}-{}", a.age_months, n + 1);
            milestones.push(MilestoneItem {
                not_yet: not_yet.iter().any(|x| x.eq_ignore_ascii_case(&id)),
                id,
                domain: (*label).to_string(),
                text: text.clone(),
            });
        }
    }
    Checklist {
        age_label: a.label.clone(),
        age_months: a.age_months,
        source_url: a.url.clone(),
        source_updated: a.updated.clone(),
        milestones,
    }
}

/// EI programs whose catchment area names the town.
fn ei_programs(ctx: &Context, town: &str) -> Result<(Vec<EiProgram>, Source)> {
    let (text, entry, _) = ctx
        .snapshots
        .file_text("ma-ei-programs", "ma_ei_programs.csv")?;
    let text = text.trim_start_matches('\u{feff}');
    let alias = content().ei_town_aliases.get(town).cloned();
    let wanted: Vec<String> = std::iter::once(town.to_string())
        .chain(alias)
        .map(|t| t.to_lowercase())
        .collect();
    let mut out = Vec::new();
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    for r in rdr.records() {
        let r = r.map_err(|e| PhnError::Internal(format!("EI program snapshot: {e}")))?;
        let get = |i: usize| r.get(i).unwrap_or("").trim().to_string();
        let catchment = get(4);
        // Match whole entries after removing parenthetical notes ("Boston (except ...)").
        let mut plain = String::new();
        let mut depth = 0;
        for ch in catchment.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                c if depth == 0 => plain.push(c),
                _ => {}
            }
        }
        let hit = plain
            .split([',', ';', '.', '\n'])
            .map(|s| s.trim().to_lowercase())
            .any(|s| wanted.contains(&s));
        if hit {
            out.push(EiProgram {
                name: get(0),
                website: get(1),
                phone: get(2),
                address: get(3).replace('\n', ", ").replace("  ", " "),
                catchment_area: catchment.replace('\n', " "),
            });
        }
    }
    Ok((out, entry.to_source()))
}

/// CDC milestones for a child's age, act-early steps, and EI referral.
pub fn check_developmental_milestones(
    ctx: &Context,
    args: MilestoneArgs,
) -> Result<Envelope<MilestoneResult>> {
    let c = content();
    let age = args.age_months;
    if !age.is_finite() || !(0.0..=72.0).contains(&age) {
        return Err(PhnError::InvalidInput(
            "age_months should be between 0 and 72 (CDC checklists cover 2 months to 5 years)"
                .into(),
        ));
    }
    let Some(a) = checklist_for(age) else {
        return Err(PhnError::InvalidInput(
            "CDC's first checklist is at 2 months; for a younger baby, ask about feeding, \
             alertness, and the family's concerns, and recheck at 2 months"
                .into(),
        ));
    };
    let checklist = build(a, &args.not_yet);
    let mut corrected_below_first = false;
    let corrected = match args.gestational_age_weeks {
        Some(ga) if !(22..=44).contains(&ga) => {
            return Err(PhnError::InvalidInput(
                "gestational_age_weeks should be between 22 and 44".into(),
            ));
        }
        Some(ga) if ga < 37 && age < 24.0 => {
            let corr = age - f64::from(40 - ga) * 7.0 / 30.4375;
            let found = checklist_for(corr);
            corrected_below_first = found.is_none();
            found
                .filter(|b| b.age_months != a.age_months)
                .map(|b| build(b, &args.not_yet))
        }
        _ => None,
    };
    let known: Vec<&str> = checklist
        .milestones
        .iter()
        .chain(corrected.iter().flat_map(|k| k.milestones.iter()))
        .map(|m| m.id.as_str())
        .collect();
    let unknown_ids: Vec<String> = args
        .not_yet
        .iter()
        .filter(|x| !known.iter().any(|k| k.eq_ignore_ascii_case(x)))
        .cloned()
        .collect();
    // Count "not yet" items on both checklists: an item marked on the corrected-age
    // checklist is a reason to act early too.
    let corrected_not_yet = corrected
        .as_ref()
        .map_or(0, |k| k.milestones.iter().filter(|m| m.not_yet).count());
    let not_yet_count =
        checklist.milestones.iter().filter(|m| m.not_yet).count() + corrected_not_yet;
    let act_early = not_yet_count > 0 || args.lost_skills || args.concern;
    let next = c
        .ages
        .iter()
        .find(|x| f64::from(x.age_months) > age + 1e-9)
        .map(|x| x.label.clone());
    let s_cfg = &c.screening;
    let near = |m: u32| (age - f64::from(m)).abs() < 1.5;
    let mut screening_due: Vec<String> = s_cfg
        .general_months
        .iter()
        .filter(|m| near(**m))
        .map(|m| format!("General developmental screening with a standardized tool ({m} months)"))
        .collect();
    screening_due.extend(
        s_cfg
            .autism_months
            .iter()
            .filter(|m| near(**m))
            .map(|m| format!("Autism screening ({m} months)")),
    );
    if act_early && screening_due.is_empty() {
        screening_due
            .push("Developmental screening now: a parent or provider has a concern".into());
    }
    let town = match &args.town {
        Some(t) => Some(ctx.ma.town(t).map(|m| m.name.clone()).ok_or_else(|| {
            let sugg: Vec<String> = ctx
                .ma
                .suggest_towns(t, 3)
                .iter()
                .map(|m| m.name.clone())
                .collect();
            PhnError::NotFound(format!(
                "'{t}' is not a Massachusetts city or town{}",
                if sugg.is_empty() {
                    String::new()
                } else {
                    format!(". Did you mean {}?", sugg.join(", "))
                }
            ))
        })?),
        None => None,
    };
    let (programs, ei_source) = match &town {
        Some(t) => {
            let (p, s) = ei_programs(ctx, t)?;
            (p, Some(s))
        }
        None => (Vec::new(), None),
    };
    let act_steps = mc::notes(&c.sources, &c.act_early);
    let ei = mc::notes(&c.sources, &c.early_intervention);

    // ---- Markdown
    let mut s = format!(
        "**Developmental milestones — {} checklist** (child is {} months; CDC \"Learn the Signs. \
         Act Early.\", updated {})\n\n",
        checklist.age_label,
        md::num(age, 1),
        checklist.source_updated
    );
    let render = |ck: &Checklist, s: &mut String| {
        let rows: Vec<Vec<String>> = ck
            .milestones
            .iter()
            .map(|m| {
                vec![
                    m.id.clone(),
                    m.domain.clone(),
                    m.text.clone(),
                    if m.not_yet {
                        "not yet".into()
                    } else {
                        String::new()
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["ID", "Area", "Most children by this age", "Status"],
            &rows,
        ));
    };
    render(&checklist, &mut s);
    if let Some(k) = &corrected {
        s.push_str(&format!(
            "\nBorn at {} weeks: the checklist for the **corrected age ({})** is also shown in \
             `corrected_age_checklist`. CDC's checklists ask whether the child was born early; \
             decide with the child's provider which age to use.\n",
            args.gestational_age_weeks.unwrap_or_default(),
            k.age_label
        ));
    }
    if act_early {
        s.push_str(&format!(
            "\n**Act early**{}{}\n\n",
            if not_yet_count > 0 {
                format!(": {not_yet_count} milestone(s) not yet")
            } else {
                String::new()
            },
            if args.lost_skills {
                "; lost skills (always act early)"
            } else {
                ""
            }
        ));
        for n in &act_steps {
            s.push_str(&mc::bullet(n));
        }
        if age < 36.0 {
            s.push_str(
                "\n**Massachusetts Early Intervention** (birth to 3): anyone, including the \
                 nurse with the family's agreement, can refer by contacting the program for the \
                 family's town; there is no cost to families.\n",
            );
        } else {
            s.push_str(
                "\nThe child is 3 or older: request an evaluation for preschool special \
                 education from the local public school district.\n",
            );
        }
    } else {
        s.push_str(
            "\nNo milestones marked \"not yet\". Pass the IDs of any milestones the child does \
             not yet do in `not_yet` (or set `lost_skills` or `concern`) to get the act-early \
             steps.\n",
        );
    }
    if !screening_due.is_empty() {
        s.push_str(&format!(
            "\n**Screening due:** {}.\n",
            screening_due.join("; ")
        ));
    }
    if let Some(t) = &town {
        if programs.is_empty() {
            s.push_str(&format!(
                "\nNo EI program in DPH's directory lists {t}; use DPH's EI program search.\n"
            ));
        } else {
            s.push_str(&format!("\n**EI programs serving {t}**\n\n"));
            s.push_str(&md::table(
                &["Program", "Phone", "Website"],
                &programs
                    .iter()
                    .map(|p| vec![p.name.clone(), p.phone.clone(), p.website.clone()])
                    .collect::<Vec<_>>(),
            ));
        }
    }
    if let Some(n) = &next {
        s.push_str(&format!("\nNext checklist: {n}.\n"));
    }

    let mut caveats = vec![mc::review_caveat(&c.meta, "developmental milestones")];
    if corrected_not_yet > 0 {
        caveats.push(format!(
            "{corrected_not_yet} of the milestone(s) marked not yet are on the corrected-age \
             checklist; they are counted and the act-early steps are shown."
        ));
    }
    if corrected_below_first {
        caveats.push(
            "The child was born early and the corrected age is under 2 months, before CDC's \
             first checklist, so only the checklist for the chronological age is shown. Decide \
             with the child's provider which age to use."
                .into(),
        );
    }
    caveats.push(format!(
        "{} Milestones describe what 75% or more of children do by that age; a missed milestone \
         is a reason to act early, not a diagnosis.",
        c.definition.text
    ));
    if !unknown_ids.is_empty() {
        caveats.push(format!(
            "{} ID(s) in not_yet are not on this checklist and were ignored: {}.",
            unknown_ids.len(),
            unknown_ids.join(", ")
        ));
    }
    if town.is_some() {
        caveats.push(
            "EI programs come from DPH's directory (September 2026); confirm the program and \
             phone number with DPH's EI program search."
                .into(),
        );
    }
    let mut sources = mc::sources_for(
        &c.sources,
        ["cdc-milestones", "cdc-concerned", "ma-ei-about"],
    );
    sources.push(Source {
        title: format!("Milestones by {} (CDC)", checklist.age_label),
        publisher: "CDC National Center on Birth Defects and Developmental Disabilities".into(),
        url: Some(checklist.source_url.clone()),
        effective_date: Some(checklist.source_updated.clone()),
        retrieved: Some(c.meta.prepared.clone()),
        mode: phn_core::SourceMode::Snapshot,
    });
    if let Some(src) = ei_source {
        sources.push(src);
    }
    let data = MilestoneResult {
        checklist,
        corrected_age_checklist: corrected,
        next_checklist: next,
        act_early,
        not_yet_count,
        unknown_ids,
        act_early_steps: act_steps,
        screening_due,
        screening_note: NoteOut {
            text: s_cfg.text.clone(),
            citation: mc::cite(&c.sources, &s_cfg.source, &s_cfg.section),
            confidence: None,
        },
        early_intervention: ei,
        ei_programs: programs,
        questions_for_the_doctor: c.share_with_doctor.clone(),
        definition: mc::note(&c.sources, &c.definition),
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
    fn twelve_checklists_with_four_domains() {
        let c = content();
        assert_eq!(c.ages.len(), 12);
        for a in &c.ages {
            for i in 0..4 {
                assert!(!a.domain(i).is_empty(), "{} domain {i}", a.label);
            }
        }
        assert_eq!(checklist_for(10.0).unwrap().age_months, 9);
        assert_eq!(checklist_for(12.0).unwrap().age_months, 12);
        assert!(checklist_for(1.0).is_none());
    }

    #[test]
    fn not_yet_triggers_act_early_and_ei_for_town() {
        let (_d, c) = ctx();
        let env = check_developmental_milestones(
            &c,
            MilestoneArgs {
                age_months: 18.5,
                not_yet: vec!["18m-language-1".into(), "99m-x-1".into()],
                town: Some("Foxborough".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert!(d.act_early);
        assert_eq!(d.not_yet_count, 1);
        assert_eq!(d.unknown_ids, vec!["99m-x-1".to_string()]);
        assert!(d.screening_due.iter().any(|s| s.contains("Autism")));
        assert!(!d.ei_programs.is_empty(), "Foxboro alias");
        assert!(env.summary.contains("Act early"));
    }

    #[test]
    fn every_town_has_an_ei_program() {
        let (_d, c) = ctx();
        let mut missing = Vec::new();
        for m in &c.ma.municipalities {
            if ei_programs(&c, &m.name).unwrap().0.is_empty() {
                missing.push(m.name.clone());
            }
        }
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn preterm_shows_corrected_checklist() {
        let (_d, c) = ctx();
        let env = check_developmental_milestones(
            &c,
            MilestoneArgs {
                age_months: 6.0,
                gestational_age_weeks: Some(30),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.checklist.age_months, 6);
        assert_eq!(
            env.data
                .corrected_age_checklist
                .as_ref()
                .unwrap()
                .age_months,
            2
        );
        assert!(!env.data.act_early);
        let e = check_developmental_milestones(
            &c,
            MilestoneArgs {
                age_months: 1.0,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("2 months"));
    }

    #[test]
    fn not_yet_on_the_corrected_age_checklist_counts() {
        let (_d, c) = ctx();
        // 9.5 months, born at 30 weeks: corrected age about 7 months (6-month checklist).
        let env = check_developmental_milestones(
            &c,
            MilestoneArgs {
                age_months: 9.5,
                gestational_age_weeks: Some(30),
                not_yet: vec!["6m-language-1".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            env.data
                .corrected_age_checklist
                .as_ref()
                .unwrap()
                .age_months,
            6
        );
        assert!(env.data.act_early);
        assert_eq!(env.data.not_yet_count, 1);
        assert!(env.data.unknown_ids.is_empty());
        // 3 months, born at 28 weeks: the corrected age is before CDC's first checklist.
        let env = check_developmental_milestones(
            &c,
            MilestoneArgs {
                age_months: 3.0,
                gestational_age_weeks: Some(28),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(env.data.corrected_age_checklist.is_none());
        assert!(env.caveats.iter().any(|x| x.contains("under 2 months")));
    }
}
