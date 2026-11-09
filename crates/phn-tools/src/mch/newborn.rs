//! MCH-01 `plan_newborn_postpartum_visit`.
//!
//! A home visit plan and teaching points for a newborn and postpartum visit: feeding and
//! weight, jaundice, safe sleep (AAP 2022), car seats (M.G.L. c. 90, s. 7AA), Massachusetts
//! newborn blood spot and hearing screening, and CDC's urgent maternal warning signs (Hear Her).
//! Optional DOCX visit plan and family teaching sheet (no identifiers; English or an agency
//! translation).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self as mc, CitedText, NoteOut, SourceRef, Translation};
use crate::context::Context;
use crate::md;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Feeding {
    breast: Vec<CitedText>,
    formula: Vec<CitedText>,
    all: Vec<CitedText>,
}

#[derive(Debug, Clone, Deserialize)]
struct Sign {
    sign: String,
    detail: String,
}

#[derive(Debug, Clone, Deserialize)]
struct WarningSigns {
    intro: String,
    source: String,
    section: String,
    signs: Vec<Sign>,
}

#[derive(Debug, Clone, Deserialize)]
struct NewbornContent {
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    feeding: Feeding,
    weight: CitedText,
    jaundice: Vec<CitedText>,
    safe_sleep: Vec<CitedText>,
    car_seat: Vec<CitedText>,
    newborn_screening: Vec<CitedText>,
    maternal_warning_signs: WarningSigns,
}

static CONTENT: LazyLock<NewbornContent> =
    LazyLock::new(|| mc::load::<NewbornContent>("mch-newborn-postpartum"));

fn content() -> &'static NewbornContent {
    &CONTENT
}

// ---------------------------------------------------------------- arguments and results

/// How the baby is fed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeedingMethod {
    Breast,
    Formula,
    Mixed,
}

/// Newborn hearing screen result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HearingScreen {
    Passed,
    NotPassed,
    NotDone,
    Unknown,
}

/// Arguments for `plan_newborn_postpartum_visit`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct NewbornVisitArgs {
    /// Baby's age in days at the visit.
    pub baby_age_days: u32,
    /// Gestational age at birth in weeks (for context; the AAP jaundice guideline covers 35
    /// weeks or more).
    #[serde(default)]
    pub gestational_age_weeks: Option<u32>,
    /// breast, formula, or mixed.
    #[serde(default)]
    pub feeding: Option<FeedingMethod>,
    /// Birth weight in grams (or use birth_weight_lb and birth_weight_oz).
    #[serde(default)]
    pub birth_weight_g: Option<f64>,
    #[serde(default)]
    pub birth_weight_lb: Option<f64>,
    #[serde(default)]
    pub birth_weight_oz: Option<f64>,
    /// Today's weight in grams (or use weight_lb and weight_oz).
    #[serde(default)]
    pub weight_g: Option<f64>,
    #[serde(default)]
    pub weight_lb: Option<f64>,
    #[serde(default)]
    pub weight_oz: Option<f64>,
    /// The baby went home within 24 hours of birth (a second blood spot is due by 48 hours).
    #[serde(default)]
    pub discharged_within_24_hours: bool,
    /// Newborn hearing screen: passed, not_passed, not_done, or unknown.
    #[serde(default)]
    pub hearing_screen: Option<HearingScreen>,
    /// Write a visit plan and a family teaching sheet (DOCX) to /data/output.
    #[serde(default)]
    pub documents: bool,
    /// Family's language for the teaching sheet; an agency translation in
    /// /data/templates/<language>/newborn_postpartum_teaching.docx (or .md) is used if present.
    #[serde(default)]
    pub language: Option<String>,
}

/// A section of the visit plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanSection {
    pub heading: String,
    /// What to assess or check.
    pub assess: Vec<String>,
    /// Teaching points with citations.
    pub teach: Vec<NoteOut>,
}

/// A maternal warning sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WarningSign {
    pub sign: String,
    pub detail: String,
}

/// Result of `plan_newborn_postpartum_visit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewbornVisitResult {
    pub baby_age_days: u32,
    /// Percent change from birth weight (negative = loss), when both weights were given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_change_percent: Option<f64>,
    /// Items needing follow-up from the inputs (repeat blood spot, hearing follow-up, ...).
    pub follow_up: Vec<String>,
    pub sections: Vec<PlanSection>,
    pub maternal_warning_signs_intro: String,
    pub maternal_warning_signs: Vec<WarningSign>,
    pub maternal_warning_signs_citation: String,
    /// Files written to /data/output.
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teaching_sheet_language: Option<String>,
}

// ---------------------------------------------------------------- logic

fn grams(g: Option<f64>, lb: Option<f64>, oz: Option<f64>, what: &str) -> Result<Option<f64>> {
    let v = match (g, lb, oz) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            return Err(PhnError::InvalidInput(format!(
                "give {what} in grams or in pounds/ounces, not both"
            )));
        }
        (Some(g), None, None) => Some(g),
        (None, None, None) => None,
        (None, lb, oz) => Some((lb.unwrap_or(0.0) * 16.0 + oz.unwrap_or(0.0)) * 28.349_523_125),
    };
    if let Some(x) = v
        && !(300.0..=10_000.0).contains(&x)
    {
        return Err(PhnError::InvalidInput(format!(
            "{what} should be between 300 and 10,000 grams"
        )));
    }
    Ok(v)
}

fn sections(args: &NewbornVisitArgs) -> Vec<PlanSection> {
    let c = content();
    let s = &c.sources;
    let mut feeding = Vec::new();
    match args.feeding {
        Some(FeedingMethod::Breast) => feeding.extend(mc::notes(s, &c.feeding.breast)),
        Some(FeedingMethod::Formula) => feeding.extend(mc::notes(s, &c.feeding.formula)),
        _ => {
            feeding.extend(mc::notes(s, &c.feeding.breast));
            feeding.extend(mc::notes(s, &c.feeding.formula));
        }
    }
    feeding.extend(mc::notes(s, &c.feeding.all));
    vec![
        PlanSection {
            heading: "Feeding and weight".into(),
            assess: vec![
                "How the baby is fed, how often in 24 hours, and how feeds go (latch, sucking and swallowing, or ounces of formula).".into(),
                "Hunger and fullness cues, and whether a sleepy baby is woken to feed.".into(),
                "Weight compared with birth weight (nude or dry diaper).".into(),
                "Vitamin D drops for a baby getting breast milk or less than 32 ounces of formula a day.".into(),
            ],
            teach: {
                let mut v = feeding;
                v.push(mc::note(s, &c.weight));
                v
            },
        },
        PlanSection {
            heading: "Jaundice".into(),
            assess: vec![
                "Whether bilirubin was checked before discharge and the follow-up the discharge plan set.".into(),
                "Yellow color of skin or eyes, when it started, and whether it is spreading; sleepiness or poor feeding.".into(),
            ],
            teach: mc::notes(s, &c.jaundice),
        },
        PlanSection {
            heading: "Safe sleep".into(),
            assess: vec![
                "Where and how the baby sleeps: back, firm flat surface, own crib or bassinet in the parents' room, nothing soft in the sleep area.".into(),
                "Smoke, nicotine, alcohol, or drug exposure in the home.".into(),
            ],
            teach: mc::notes(s, &c.safe_sleep),
        },
        PlanSection {
            heading: "Car seat".into(),
            assess: vec!["A car seat for the baby, installed and used following the seat's and vehicle's instructions.".into()],
            teach: mc::notes(s, &c.car_seat),
        },
        PlanSection {
            heading: "Newborn screening".into(),
            assess: vec![
                "Blood spot collected at 24-48 hours (and a second specimen if discharged within 24 hours); any repeat specimen requested.".into(),
                "Hearing screen result before discharge and any follow-up appointment.".into(),
            ],
            teach: mc::notes(s, &c.newborn_screening),
        },
        PlanSection {
            heading: "Postpartum health (birthing parent)".into(),
            assess: vec![
                "Each urgent maternal warning sign (CDC Hear Her), including mood and thoughts of harming self or baby.".into(),
                "Bleeding, pain, swelling of a leg or arm, headache, vision, breathing.".into(),
                "Who to call and where to go; that the family knows to say the birth was within the past year.".into(),
            ],
            teach: vec![NoteOut {
                text: c.maternal_warning_signs.intro.clone(),
                citation: mc::cite(
                    s,
                    &c.maternal_warning_signs.source,
                    &c.maternal_warning_signs.section,
                ),
                confidence: None,
            }],
        },
    ]
}

fn plan_document(ctx: &Context, title: &str, secs: &[PlanSection], with_assess: bool) -> Document {
    let c = content();
    let mut blocks = Vec::new();
    for sec in secs {
        blocks.push(Block::Heading {
            level: 2,
            text: sec.heading.clone(),
        });
        if with_assess && !sec.assess.is_empty() {
            blocks.push(Block::Paragraph(vec![Inline {
                text: "Check:".into(),
                bold: true,
                ..Inline::default()
            }]));
            blocks.push(Block::List {
                ordered: false,
                items: sec
                    .assess
                    .iter()
                    .map(|a| vec![Inline::plain(format!("[ ] {a}"))])
                    .collect(),
            });
            blocks.push(Block::Paragraph(vec![Inline {
                text: "Teach:".into(),
                bold: true,
                ..Inline::default()
            }]));
        }
        blocks.push(Block::List {
            ordered: false,
            items: sec
                .teach
                .iter()
                .map(|n| {
                    if with_assess {
                        vec![Inline::plain(format!("{} ({})", n.text, n.citation))]
                    } else {
                        vec![Inline::plain(n.text.clone())]
                    }
                })
                .collect(),
        });
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Urgent maternal warning signs: get medical care right away".into(),
    });
    blocks.push(Block::Paragraph(vec![Inline::plain(
        c.maternal_warning_signs.intro.clone(),
    )]));
    blocks.push(Block::List {
        ordered: false,
        items: c
            .maternal_warning_signs
            .signs
            .iter()
            .map(|sg| {
                vec![
                    Inline {
                        text: format!("{}: ", sg.sign),
                        bold: true,
                        ..Inline::default()
                    },
                    Inline::plain(sg.detail.clone()),
                ]
            })
            .collect(),
    });
    blocks.push(Block::Paragraph(vec![Inline::plain(format!(
        "Sources: CDC (Hear Her; safe sleep; infant feeding), American Academy of Pediatrics \
         2022 policies, 105 CMR 270.000, M.G.L. c. 111, s. 67F and c. 90, s. 7AA; retrieved {}. \
         Draft pending review by the public health nurse.",
        c.meta.prepared
    ))]));
    Document {
        title: title.into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

/// Plan a newborn and postpartum home visit.
pub fn plan_newborn_postpartum_visit(
    ctx: &Context,
    args: NewbornVisitArgs,
) -> Result<Envelope<NewbornVisitResult>> {
    let c = content();
    if args.baby_age_days > 365 {
        return Err(PhnError::InvalidInput(
            "baby_age_days should be 0 to 365 (this plan is for the newborn and postpartum \
             period)"
                .into(),
        ));
    }
    if let Some(g) = args.gestational_age_weeks
        && !(22..=44).contains(&g)
    {
        return Err(PhnError::InvalidInput(
            "gestational_age_weeks should be between 22 and 44".into(),
        ));
    }
    let bw = grams(
        args.birth_weight_g,
        args.birth_weight_lb,
        args.birth_weight_oz,
        "birth weight",
    )?;
    let w = grams(
        args.weight_g,
        args.weight_lb,
        args.weight_oz,
        "today's weight",
    )?;
    let change = match (bw, w) {
        (Some(b), Some(t)) => Some((t - b) / b * 100.0),
        _ => None,
    };
    let mut follow_up = Vec::new();
    if args.discharged_within_24_hours && args.baby_age_days <= 7 {
        follow_up.push(
            "Discharged within 24 hours: confirm the second newborn blood spot was collected \
             within 48 hours of birth (105 CMR 270.007(A))."
                .into(),
        );
    }
    match args.hearing_screen {
        Some(HearingScreen::NotPassed) => follow_up.push(
            "Hearing screen not passed: confirm the diagnostic hearing evaluation at a \
             DPH-approved center is scheduled (M.G.L. c. 111, s. 67F)."
                .into(),
        ),
        Some(HearingScreen::NotDone) | Some(HearingScreen::Unknown) => follow_up.push(
            "Hearing screen not done or unknown: ask the baby's provider to arrange it \
             (M.G.L. c. 111, s. 67F)."
                .into(),
        ),
        _ => {}
    }
    if let Some(ch) = change {
        follow_up.push(format!(
            "Weight is {:.1}% {} birth weight at {} days: share with the baby's provider (this \
             tool does not judge the change; use calculate_growth_percentiles for WHO \
             percentiles).",
            ch.abs(),
            if ch < 0.0 { "below" } else { "above" },
            args.baby_age_days
        ));
    }
    if args.gestational_age_weeks.is_some_and(|g| g < 35) {
        follow_up.push(
            "Born before 35 weeks: the AAP 2022 jaundice guideline covers 35 weeks or more; \
             follow the baby's provider and the NICU discharge plan."
                .into(),
        );
    }
    let secs = sections(&args);

    // Optional documents.
    let mut files = Vec::new();
    let mut lang_used = None;
    let mut caveats = vec![mc::review_caveat(&c.meta, "newborn and postpartum")];
    if args.documents {
        let sandbox = ctx.sandbox()?;
        let plan = plan_document(ctx, "Newborn and postpartum home visit plan", &secs, true);
        let out = sandbox.new_output_file("newborn_postpartum_visit_plan.docx")?;
        std::fs::write(&out, phn_docs::docx::to_docx(&plan)?)?;
        files.push(sandbox.display_path(&out));
        let mut wrote = false;
        if let Some(lang) = args
            .language
            .as_deref()
            .filter(|l| !mc::is_english(Some(l)))
        {
            if let Some((t, rel)) = mc::agency_translation(ctx, lang, "newborn_postpartum_teaching")
            {
                let out = sandbox.new_output_file(&format!(
                    "newborn_postpartum_teaching_{}.docx",
                    phn_local::sandbox::sanitize_file_name(lang)
                ))?;
                match t {
                    Translation::Docx(p) => {
                        std::fs::copy(&p, &out)?;
                    }
                    Translation::Markdown(text) => {
                        let mut d = plan_document(ctx, "Caring for you and your baby", &[], false);
                        d.blocks = phn_docs::model::from_markdown(&text);
                        d.language = lang.to_string();
                        std::fs::write(&out, phn_docs::docx::to_docx(&d)?)?;
                    }
                }
                files.push(sandbox.display_path(&out));
                lang_used = Some(format!("{lang} (agency translation {rel})"));
                wrote = true;
            } else {
                caveats.push(format!(
                    "No agency-approved {lang} translation was found (expected \
                     /data/templates/{}/newborn_postpartum_teaching.docx or .md), so the \
                     English teaching sheet was written. Use a qualified interpreter or your \
                     agency's translation process; this server does not machine-translate.",
                    mc::language_folders(lang)[0]
                ));
            }
        }
        if !wrote {
            let teach = plan_document(ctx, "Caring for you and your baby", &secs, false);
            let out = sandbox.new_output_file("newborn_postpartum_teaching.docx")?;
            std::fs::write(&out, phn_docs::docx::to_docx(&teach)?)?;
            files.push(sandbox.display_path(&out));
            lang_used = Some("English".into());
        }
    }

    // ---- Markdown
    let mut s = format!(
        "**Newborn and postpartum home visit plan** (baby {} days old{})\n\n",
        args.baby_age_days,
        match args.feeding {
            Some(FeedingMethod::Breast) => ", breastfed",
            Some(FeedingMethod::Formula) => ", formula-fed",
            Some(FeedingMethod::Mixed) => ", breast milk and formula",
            None => "",
        }
    );
    if !follow_up.is_empty() {
        s.push_str("**Follow up**\n\n");
        for f in &follow_up {
            s.push_str(&format!("- {f}\n"));
        }
        s.push('\n');
    }
    for sec in &secs {
        s.push_str(&format!("**{}**\n\n", sec.heading));
        for a in &sec.assess {
            s.push_str(&format!("- [ ] {a}\n"));
        }
        for n in &sec.teach {
            s.push_str(&mc::bullet(n));
        }
        s.push('\n');
    }
    s.push_str("**Urgent maternal warning signs (CDC Hear Her)**\n\n");
    s.push_str(&md::table(
        &["Sign", "What it looks like"],
        &c.maternal_warning_signs
            .signs
            .iter()
            .map(|sg| vec![sg.sign.clone(), sg.detail.clone()])
            .collect::<Vec<_>>(),
    ));
    if !files.is_empty() {
        s.push_str(&format!(
            "\nDocuments written: {} (no information about the family).\n",
            files
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    caveats.push(
        "Jaundice items are LOW confidence: the AAP 2022 hyperbilirubinemia guideline's text \
         could not be retrieved. Follow the baby's discharge plan and provider for bilirubin \
         follow-up and treatment thresholds."
            .into(),
    );
    caveats.push(
        "This plan supports, and does not replace, the agency's home visiting protocol and the \
         family's health care providers. For an emergency, call 911."
            .into(),
    );
    let mut privacy = Privacy::none();
    if !files.is_empty() {
        privacy.files_written = files.clone();
        privacy.notes =
            vec!["The documents are generic and contain no information about the family.".into()];
    }
    let keys: Vec<&str> = c.sources.keys().map(String::as_str).collect();
    let data = NewbornVisitResult {
        baby_age_days: args.baby_age_days,
        weight_change_percent: change,
        follow_up,
        sections: secs,
        maternal_warning_signs_intro: c.maternal_warning_signs.intro.clone(),
        maternal_warning_signs: c
            .maternal_warning_signs
            .signs
            .iter()
            .map(|sg| WarningSign {
                sign: sg.sign.clone(),
                detail: sg.detail.clone(),
            })
            .collect(),
        maternal_warning_signs_citation: mc::cite(
            &c.sources,
            &c.maternal_warning_signs.source,
            &c.maternal_warning_signs.section,
        ),
        files,
        teaching_sheet_language: lang_used,
    };
    Ok(Envelope::new(s, data)
        .sources(mc::sources_for(&c.sources, keys))
        .source(Source::computed(
            "Percent weight change from birth weight",
            "PHN MCP server",
        ))
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn content_is_cited() {
        let c = content();
        for n in c
            .jaundice
            .iter()
            .chain(&c.safe_sleep)
            .chain(&c.car_seat)
            .chain(&c.newborn_screening)
            .chain(&c.feeding.all)
            .chain(&c.feeding.breast)
            .chain(&c.feeding.formula)
        {
            assert!(c.sources.contains_key(&n.source), "{}", n.source);
        }
        assert!(c.maternal_warning_signs.signs.len() >= 13);
    }

    #[test]
    fn plan_with_weight_screening_and_documents() {
        let (d, c) = ctx();
        let env = plan_newborn_postpartum_visit(
            &c,
            NewbornVisitArgs {
                baby_age_days: 4,
                feeding: Some(FeedingMethod::Breast),
                birth_weight_lb: Some(7.0),
                birth_weight_oz: Some(8.0),
                weight_g: Some(3130.0),
                discharged_within_24_hours: true,
                hearing_screen: Some(HearingScreen::NotPassed),
                documents: true,
                language: Some("Haitian Creole".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let ch = env.data.weight_change_percent.unwrap();
        assert!((ch - (3130.0 - 3401.94) / 3401.94 * 100.0).abs() < 0.01);
        assert!(
            env.data
                .follow_up
                .iter()
                .any(|f| f.contains("second newborn blood spot"))
        );
        assert!(
            env.data
                .follow_up
                .iter()
                .any(|f| f.contains("diagnostic hearing"))
        );
        assert_eq!(env.data.files.len(), 2);
        for f in &env.data.files {
            assert!(d.path().join("data").join(f).exists());
        }
        assert_eq!(env.data.teaching_sheet_language.as_deref(), Some("English"));
        assert!(env.caveats.iter().any(|c| c.contains("Haitian Creole")));
        assert!(env.summary.contains("Hear Her"));
        assert!(env.notice.is_some());
    }

    #[test]
    fn bad_inputs() {
        let (_d, c) = ctx();
        let e = plan_newborn_postpartum_visit(
            &c,
            NewbornVisitArgs {
                baby_age_days: 3,
                weight_g: Some(3000.0),
                weight_lb: Some(6.0),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("not both"));
    }
}
