//! MCH-04 `childhood_lead_screening_guide`.
//!
//! The Massachusetts lead screening schedule (105 CMR 460.050), DPH's high-risk community list,
//! venous confirmation timing for capillary results, follow-up testing and actions by venous
//! blood lead level (DPH CLPPP), reporting duties, the Lead Law, and family teaching, with an
//! optional family teaching sheet (DOCX, English or an agency-approved translation).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_content::reportable::Confidence;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self as mc, CitedText, NoteOut, SourceRef, Translation};
use super::lead_data::{self, LeadTownRow};
use crate::context::Context;
use crate::md;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Definition {
    term: String,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AgeBand {
    age_months_from: f64,
    age_months_to: f64,
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Schedule {
    routine: Vec<AgeBand>,
    high_risk_community: AgeBand,
    source: String,
    section: String,
    entry: CitedText,
    sample: CitedText,
    provider_duty: CitedText,
}

#[derive(Debug, Clone, Deserialize)]
struct Criterion {
    id: RiskFactor,
    text: String,
    schedule: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct HighRiskList {
    year: u32,
    period: String,
    method: String,
    source: String,
    section: String,
    towns: Vec<String>,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Band {
    from: f64,
    to: Option<f64>,
    timing: String,
}

#[derive(Debug, Clone, Deserialize)]
struct VenousBand {
    from: f64,
    to: Option<f64>,
    category: String,
    provider: String,
    environmental: String,
    case_management: String,
    retest: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Teaching {
    topic: String,
    text: String,
    source: String,
    section: String,
    #[serde(default)]
    confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Deserialize)]
struct LeadContent {
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    definitions: Vec<Definition>,
    screening_schedule: Schedule,
    high_risk_criteria: Vec<Criterion>,
    high_risk_communities: HighRiskList,
    capillary_confirmation: Vec<Band>,
    capillary_confirmation_source: String,
    capillary_confirmation_section: String,
    venous_actions: Vec<VenousBand>,
    venous_actions_source: String,
    venous_actions_section: String,
    specialty_clinics: CitedText,
    reporting: Vec<CitedText>,
    lead_law: Vec<CitedText>,
    teaching: Vec<Teaching>,
    contacts: Vec<CitedText>,
}

static CONTENT: LazyLock<LeadContent> =
    LazyLock::new(|| mc::load::<LeadContent>("mch-lead-screening"));

fn content() -> &'static LeadContent {
    &CONTENT
}

// ---------------------------------------------------------------- arguments

/// Blood sample type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SampleType {
    /// Fingerstick (or heelstick): a screening result that may need venous confirmation.
    Capillary,
    /// Blood drawn from a vein: confirmed.
    Venous,
}

/// High-risk factors in 105 CMR 460.050(D).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiskFactor {
    /// A sibling or other child in the household is lead poisoned.
    HouseholdPoisoned,
    /// Lives in a pre-1978 home undergoing renovation.
    Renovation,
    /// Lives in a pre-1978 home with deteriorated paint or plaster.
    DeterioratedPaint,
    /// The health care provider judges the child to be at high risk.
    ClinicalJudgment,
}

/// Arguments for `childhood_lead_screening_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct LeadGuideArgs {
    /// Child's age in months, to show which screenings are due (e.g. 18).
    #[serde(default)]
    pub child_age_months: Option<f64>,
    /// City or town where the child lives (checks DPH's high-risk community list and the
    /// town's published screening rate).
    #[serde(default)]
    pub town: Option<String>,
    /// Blood lead level in micrograms per deciliter (ug/dL).
    #[serde(default)]
    pub blood_lead_level: Option<f64>,
    /// "capillary" (fingerstick) or "venous" (required with blood_lead_level).
    #[serde(default)]
    pub sample_type: Option<SampleType>,
    /// High-risk factors: household_poisoned, renovation, deteriorated_paint, clinical_judgment.
    #[serde(default)]
    pub risk_factors: Vec<RiskFactor>,
    /// Write a family teaching sheet (DOCX) to /data/output.
    #[serde(default)]
    pub teaching_sheet: bool,
    /// Family's language for the teaching sheet (default English). An agency-approved
    /// translation in /data/templates/<language>/lead_family_teaching.docx (or .md) is used
    /// when present; otherwise the English sheet is written and the result says so.
    #[serde(default)]
    pub language: Option<String>,
}

// ---------------------------------------------------------------- results

/// A screening that applies to the child.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreeningItem {
    pub label: String,
    /// "due now", "upcoming", "past", or "applies" (when no age was given).
    pub status: String,
    pub citation: String,
}

/// The town's high-risk status and published screening figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownLead {
    pub town: String,
    pub high_risk_community: bool,
    pub list_year: u32,
    /// Rank on DPH's list (1 = highest high-risk score).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    /// Data year for the screening figures.
    pub data_year: String,
    /// Percent of children 9-47 months screened.
    pub percent_screened: String,
    /// Percent of screened children with an estimated confirmed level of 5 ug/dL or more.
    pub percent_5_or_more: String,
    /// Percent with a confirmed level of 10 ug/dL or more.
    pub percent_10_or_more: String,
    pub percent_pre1978_housing: String,
    /// Statewide comparison values.
    pub state_percent_screened: String,
    pub state_percent_5_or_more: String,
}

/// Interpretation of a blood lead result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResultGuidance {
    pub blood_lead_level: f64,
    pub sample_type: SampleType,
    /// Regulatory category (venous) or "unconfirmed capillary result".
    pub category: String,
    /// When to confirm with a venous sample (capillary results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<String>,
    /// Actions for this venous level (or the level a confirmed result would fall in).
    pub provider_follow_up: String,
    pub environmental: String,
    pub case_management: String,
    pub retest: String,
    /// Reporting duty for this result.
    pub reporting: String,
    pub citation: String,
}

/// Result of `childhood_lead_screening_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeadGuideResult {
    pub definitions: Vec<NoteOut>,
    pub screening: Vec<ScreeningItem>,
    pub high_risk_screening: Vec<NoteOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub town: Option<TownLead>,
    /// DPH's current high-risk communities (in rank order).
    pub high_risk_communities: Vec<String>,
    pub high_risk_list_year: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ResultGuidance>,
    pub sample_collection: NoteOut,
    /// Who must screen (physicians, facilities, HMOs) and coverage.
    pub provider_duty: NoteOut,
    pub reporting: Vec<NoteOut>,
    pub lead_law: Vec<NoteOut>,
    pub family_teaching: Vec<NoteOut>,
    pub contacts: Vec<NoteOut>,
    /// Teaching sheet written to /data/output, when asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teaching_sheet_file: Option<String>,
    /// Language of the teaching sheet ("English" or the agency translation's folder).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teaching_sheet_language: Option<String>,
}

// ---------------------------------------------------------------- logic

fn in_band(x: f64, from: f64, to: Option<f64>) -> bool {
    // Bands are written as 3.5-4.9, 5-9.9, ...: a value belongs to the band whose lower bound
    // it reaches and whose next band it does not.
    x >= from && to.is_none_or(|t| x < t + 0.1 - 1e-9)
}

fn venous_band(x: f64) -> &'static VenousBand {
    let c = content();
    c.venous_actions
        .iter()
        .rev()
        .find(|b| x >= b.from)
        .unwrap_or(&c.venous_actions[0])
}

fn capillary_band(x: f64) -> Option<&'static Band> {
    content()
        .capillary_confirmation
        .iter()
        .find(|b| in_band(x, b.from, b.to))
}

fn screening_items(age: Option<f64>, high_risk_town: bool) -> Vec<ScreeningItem> {
    let c = content();
    let sch = &c.screening_schedule;
    let cite = mc::cite(&c.sources, &sch.source, &sch.section);
    let mut bands: Vec<&AgeBand> = sch.routine.iter().collect();
    if high_risk_town {
        bands.push(&sch.high_risk_community);
    }
    bands
        .into_iter()
        .map(|b| ScreeningItem {
            label: b.label.clone(),
            status: match age {
                None => "applies".into(),
                Some(a) if a < b.age_months_from => "upcoming".into(),
                Some(a) if a < b.age_months_to + 1.0 => "due now (if not yet done)".into(),
                Some(_) => "past (confirm it was done)".into(),
            },
            citation: cite.clone(),
        })
        .collect()
}

fn reporting_for(level: f64, sample: SampleType) -> String {
    if level >= 10.0 && sample == SampleType::Venous {
        "Lead poisoning: the provider or laboratory reports to DPH within 3 working days \
         (105 CMR 460.040(A)); every result is also reported electronically within one week."
            .into()
    } else {
        "Every blood lead result is reported to DPH CLPPP electronically within one week of \
         analysis (105 CMR 460.040(B)); a laboratory that analyzes the sample reports it."
            .into()
    }
}

fn result_guidance(level: f64, sample: SampleType) -> ResultGuidance {
    let c = content();
    let vb = venous_band(level);
    let (category, confirmation) = match sample {
        SampleType::Venous => (vb.category.clone(), None),
        SampleType::Capillary => (
            if level >= 3.5 {
                "Unconfirmed capillary result at or above the CDC reference value".to_string()
            } else {
                "Capillary result below the CDC reference value".to_string()
            },
            Some(match capillary_band(level) {
                Some(b) => b.timing.clone(),
                None => {
                    "No confirmation needed below 3.5 ug/dL; continue routine screening.".into()
                }
            }),
        ),
    };
    let citation = match sample {
        SampleType::Venous => mc::cite(
            &c.sources,
            &c.venous_actions_source,
            &c.venous_actions_section,
        ),
        SampleType::Capillary => format!(
            "{}; {}",
            mc::cite(
                &c.sources,
                &c.capillary_confirmation_source,
                &c.capillary_confirmation_section
            ),
            mc::cite(&c.sources, "cmr460", "460.050(B)")
        ),
    };
    ResultGuidance {
        blood_lead_level: level,
        sample_type: sample,
        category,
        confirmation,
        provider_follow_up: vb.provider.clone(),
        environmental: vb.environmental.clone(),
        case_management: vb.case_management.clone(),
        retest: vb.retest.clone(),
        reporting: reporting_for(level, sample),
        citation,
    }
}

fn town_lead(rows: &[LeadTownRow], name: &str) -> Option<TownLead> {
    let c = content();
    let r = rows.iter().find(|r| r.town == name)?;
    let state = rows.iter().find(|r| r.town == "Total for MA")?;
    Some(TownLead {
        town: r.town.clone(),
        high_risk_community: c.high_risk_communities.towns.iter().any(|t| t == name),
        list_year: c.high_risk_communities.year,
        rank: r.hr_rank.parse().ok(),
        data_year: r.data_year.clone(),
        percent_screened: r.pct_screened.clone(),
        percent_5_or_more: r.est_confirmed_5plus_pct.clone(),
        percent_10_or_more: r.confirmed_10plus_pct.clone(),
        percent_pre1978_housing: r.pct_pre1978_housing.clone(),
        state_percent_screened: state.pct_screened.clone(),
        state_percent_5_or_more: state.est_confirmed_5plus_pct.clone(),
    })
}

fn teaching_notes() -> Vec<NoteOut> {
    let c = content();
    c.teaching
        .iter()
        .map(|t| NoteOut {
            text: format!("{}: {}", t.topic, t.text),
            citation: mc::cite(&c.sources, &t.source, &t.section),
            confidence: t.confidence,
        })
        .collect()
}

/// The English family teaching sheet (no identifiers).
fn teaching_document(ctx: &Context) -> Document {
    let c = content();
    let mut blocks = vec![Block::Paragraph(vec![Inline::plain(
        "Lead can harm a child's brain, growth, hearing, and behavior. Most lead poisoning in \
         Massachusetts comes from old paint in homes built before 1978. Here is how to keep \
         your child safe.",
    )])];
    for t in &c.teaching {
        blocks.push(Block::Heading {
            level: 2,
            text: t.topic.clone(),
        });
        blocks.push(Block::Paragraph(vec![Inline::plain(t.text.clone())]));
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Questions?".into(),
    });
    for n in &c.contacts {
        blocks.push(Block::Paragraph(vec![Inline::plain(n.text.clone())]));
    }
    blocks.push(Block::Paragraph(vec![Inline::plain(format!(
        "Source: Massachusetts Department of Public Health, Childhood Lead Poisoning Prevention \
         Program (mass.gov), retrieved {}. Draft pending review by the public health nurse.",
        c.meta.prepared
    ))]));
    Document {
        title: "Protect your child from lead".into(),
        subtitle: Some("Information for families".into()),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

fn write_teaching_sheet(
    ctx: &Context,
    language: Option<&str>,
) -> Result<(String, String, Option<String>)> {
    let sandbox = ctx.sandbox()?;
    let mut note = None;
    if let Some(lang) = language.filter(|l| !mc::is_english(Some(l))) {
        if let Some((t, rel)) = mc::agency_translation(ctx, lang, "lead_family_teaching") {
            let out = sandbox.new_output_file(&format!(
                "lead_family_teaching_{}.docx",
                phn_local::sandbox::sanitize_file_name(lang)
            ))?;
            match t {
                Translation::Docx(p) => {
                    std::fs::copy(&p, &out)?;
                }
                Translation::Markdown(md_text) => {
                    let mut d = teaching_document(ctx);
                    d.blocks = phn_docs::model::from_markdown(&md_text);
                    d.language = lang.to_string();
                    std::fs::write(&out, phn_docs::docx::to_docx(&d)?)?;
                }
            }
            return Ok((
                sandbox.display_path(&out),
                format!("{lang} (agency translation {rel})"),
                None,
            ));
        }
        note = Some(format!(
            "No agency-approved {lang} translation was found (expected \
             /data/templates/{}/lead_family_teaching.docx or .md), so the English sheet was \
             written. Use a qualified interpreter or your agency's translation process; this \
             server does not machine-translate.",
            mc::language_folders(lang)[0]
        ));
    }
    let out = sandbox.new_output_file("lead_family_teaching.docx")?;
    std::fs::write(&out, phn_docs::docx::to_docx(&teaching_document(ctx))?)?;
    Ok((sandbox.display_path(&out), "English".into(), note))
}

/// Massachusetts childhood lead screening, result follow-up, and family teaching.
pub fn childhood_lead_screening_guide(
    ctx: &Context,
    args: LeadGuideArgs,
) -> Result<Envelope<LeadGuideResult>> {
    let c = content();
    if let Some(a) = args.child_age_months
        && !(0.0..=216.0).contains(&a)
    {
        return Err(PhnError::InvalidInput(
            "child_age_months should be between 0 and 216".into(),
        ));
    }
    let result = match (args.blood_lead_level, args.sample_type) {
        (Some(l), _) if !l.is_finite() || l < 0.0 || l > 300.0 => {
            return Err(PhnError::InvalidInput(
                "blood_lead_level should be in ug/dL, between 0 and 300".into(),
            ));
        }
        (Some(_), None) => {
            return Err(PhnError::InvalidInput(
                "give sample_type (\"capillary\" for a fingerstick or \"venous\") with the blood \
                 lead level: capillary results of 5 ug/dL or more need venous confirmation"
                    .into(),
            ));
        }
        (Some(l), Some(s)) => Some(result_guidance(l, s)),
        (None, _) => None,
    };
    let (rows, snap_source) = lead_data::load(ctx)?;
    let town = match &args.town {
        Some(t) => {
            let Some(m) = ctx.ma.town(t) else {
                let sugg: Vec<String> = ctx
                    .ma
                    .suggest_towns(t, 3)
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                return Err(PhnError::NotFound(format!(
                    "'{t}' is not a Massachusetts city or town{}",
                    if sugg.is_empty() {
                        String::new()
                    } else {
                        format!(". Did you mean {}?", sugg.join(", "))
                    }
                )));
            };
            town_lead(&rows, &m.name)
        }
        None => None,
    };
    let high_risk_town = town.as_ref().is_some_and(|t| t.high_risk_community);
    let screening = screening_items(args.child_age_months, high_risk_town);
    let hr_notes: Vec<NoteOut> = c
        .high_risk_criteria
        .iter()
        .filter(|cr| args.risk_factors.is_empty() || args.risk_factors.contains(&cr.id))
        .map(|cr| NoteOut {
            text: format!("{} Schedule: {}", cr.text, cr.schedule),
            citation: mc::cite(&c.sources, &cr.source, &cr.section),
            confidence: None,
        })
        .collect();
    let definitions: Vec<NoteOut> = c
        .definitions
        .iter()
        .map(|d| NoteOut {
            text: format!("{}: {}", d.term, d.text),
            citation: mc::cite(&c.sources, &d.source, &d.section),
            confidence: None,
        })
        .collect();
    let reporting = mc::notes(&c.sources, &c.reporting);
    let lead_law = mc::notes(&c.sources, &c.lead_law);
    let family_teaching = teaching_notes();
    let mut contacts = mc::notes(&c.sources, &c.contacts);
    contacts.push(mc::note(&c.sources, &c.specialty_clinics));

    let mut caveats = vec![mc::review_caveat(&c.meta, "lead screening")];
    let (sheet, sheet_lang) = if args.teaching_sheet {
        let (f, lang, note) = write_teaching_sheet(ctx, args.language.as_deref())?;
        if let Some(n) = note {
            caveats.push(n);
        }
        (Some(f), Some(lang))
    } else {
        (None, None)
    };

    // ---- Markdown
    let mut s = String::from("**Childhood lead screening (Massachusetts, 105 CMR 460.000)**\n\n");
    if let Some(r) = &result {
        s.push_str(&format!(
            "**Result: {} ug/dL ({}) — {}**\n\n",
            md::num(r.blood_lead_level, 1),
            match r.sample_type {
                SampleType::Capillary => "capillary",
                SampleType::Venous => "venous",
            },
            r.category
        ));
        if let Some(cf) = &r.confirmation {
            s.push_str(&format!("- **Confirm:** {cf}\n"));
        }
        let lead_in = if r.sample_type == SampleType::Capillary {
            "If confirmed at this level — "
        } else {
            ""
        };
        s.push_str(&format!(
            "- **{lead_in}Provider follow-up:** {}\n",
            r.provider_follow_up
        ));
        s.push_str(&format!("- **Environmental:** {}\n", r.environmental));
        s.push_str(&format!("- **Case management:** {}\n", r.case_management));
        s.push_str(&format!("- **Re-test:** {}\n", r.retest));
        s.push_str(&format!("- **Reporting:** {}\n", r.reporting));
        s.push_str(&format!("\n_Source: {}_\n\n", r.citation));
    }
    s.push_str("**Required screening**\n\n");
    s.push_str(&md::table(
        &["Screening", "Status"],
        &screening
            .iter()
            .map(|i| vec![i.label.clone(), i.status.clone()])
            .collect::<Vec<_>>(),
    ));
    s.push_str(&format!(
        "\n{} ({})\n\n",
        c.screening_schedule.entry.text,
        mc::cite(
            &c.sources,
            &c.screening_schedule.entry.source,
            &c.screening_schedule.entry.section
        )
    ));
    if let Some(t) = &town {
        s.push_str(&format!(
            "**{}**: {} DPH's {} high-risk community list{}. In {}, {}% of children 9-47 months \
             were screened (state {}%), and {}% of those screened had an estimated confirmed \
             level of 5 ug/dL or more (state {}%); {}% of housing units were built before \
             1978.\n\n",
            t.town,
            if t.high_risk_community {
                "is on"
            } else {
                "is not on"
            },
            t.list_year,
            t.rank
                .map(|r| format!(" (rank {r} of {})", c.high_risk_communities.towns.len()))
                .unwrap_or_default(),
            t.data_year,
            t.percent_screened,
            t.state_percent_screened,
            t.percent_5_or_more,
            t.state_percent_5_or_more,
            t.percent_pre1978_housing
        ));
    } else {
        s.push_str(&format!(
            "DPH's {} high-risk communities (age-4 screening required): {}.\n\n",
            c.high_risk_communities.year,
            c.high_risk_communities.towns.join(", ")
        ));
    }
    if !args.risk_factors.is_empty() {
        s.push_str("**High-risk screening (more than once a year)**\n\n");
        for n in &hr_notes {
            s.push_str(&mc::bullet(n));
        }
        s.push('\n');
    }
    s.push_str(&format!(
        "**Sample collection:** {}\n\n",
        c.screening_schedule.sample.text
    ));
    if result.is_none() {
        s.push_str("**Follow-up by confirmed (venous) blood lead level**\n\n");
        let rows: Vec<Vec<String>> = c
            .venous_actions
            .iter()
            .map(|b| {
                vec![
                    match b.to {
                        Some(t) => format!("{}-{}", b.from, t),
                        None => format!("{}+", b.from),
                    },
                    b.category.clone(),
                    b.retest.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(&["ug/dL", "Category", "Re-test"], &rows));
        s.push('\n');
    }
    if let Some(f) = &sheet {
        s.push_str(&format!(
            "Family teaching sheet ({}) written to `{f}`.\n",
            sheet_lang.clone().unwrap_or_default()
        ));
    } else {
        s.push_str(
            "Family teaching points (cleaning, blocking peeling paint, nutrition, water, soil, \
             other sources) are in `family_teaching`; set teaching_sheet to write a handout.\n",
        );
    }

    caveats.push(
        "DPH's follow-up table combines Massachusetts requirements (lead poisoning at 10 ug/dL, \
         level of concern 5-9 ug/dL, venous confirmation of capillary results of 5 or more) with \
         CDC's 2021 reference value (3.5 ug/dL) and CDC's recommended schedules. Follow the \
         child's health care provider and CLPPP case management."
            .into(),
    );
    caveats.push(format!(
        "The high-risk community list changes every year ({} list, {}); check DPH's latest \
         annual surveillance report. How DPH chooses them: {} {} ({})",
        c.high_risk_communities.year,
        c.high_risk_communities.period,
        c.high_risk_communities.method,
        c.high_risk_communities.note,
        mc::cite(
            &c.sources,
            &c.high_risk_communities.source,
            &c.high_risk_communities.section
        )
    ));
    if town.as_ref().is_some_and(|t| t.data_year != "2024") {
        caveats.push(
            "This town's screening figures are from 2023: DPH's 2024 report omitted the page \
             for Monson through Northampton."
                .into(),
        );
    }
    let mut keys: Vec<&str> = vec![
        "cmr460",
        "dph-results",
        "dph-requirements",
        "dph-report-2024",
    ];
    keys.extend(c.teaching.iter().map(|t| t.source.as_str()));
    keys.push("dph-lead-law");
    let mut sources = mc::sources_for(&c.sources, keys);
    if town.is_some() {
        sources.push(snap_source);
    }
    let data = LeadGuideResult {
        definitions,
        screening,
        high_risk_screening: hr_notes,
        town,
        high_risk_communities: c.high_risk_communities.towns.clone(),
        high_risk_list_year: c.high_risk_communities.year,
        result,
        sample_collection: mc::note(&c.sources, &c.screening_schedule.sample),
        provider_duty: mc::note(&c.sources, &c.screening_schedule.provider_duty),
        reporting,
        lead_law,
        family_teaching,
        contacts,
        teaching_sheet_file: sheet.clone(),
        teaching_sheet_language: sheet_lang,
    };
    let mut privacy = Privacy::none();
    if let Some(f) = sheet {
        privacy.files_written = vec![f];
        privacy.notes = vec![
            "The teaching sheet is generic and contains no information about the child.".into(),
        ];
    }
    Ok(Envelope::new(s, data)
        .sources(sources)
        .source(Source::computed(
            "Screening status by age (105 CMR 460.050)",
            "PHN MCP server",
        ))
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

/// DPH's high-risk community list (for MCH-05).
pub(crate) fn high_risk_towns() -> (&'static [String], u32, String) {
    let c = content();
    let h = &c.high_risk_communities;
    (
        &h.towns,
        h.year,
        format!(
            "{} {}",
            h.method,
            mc::cite(&c.sources, &h.source, &h.section)
        ),
    )
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
    fn content_bands_are_ordered_and_cited() {
        let c = content();
        for w in c.venous_actions.windows(2) {
            assert!(w[0].from < w[1].from);
        }
        for w in c.capillary_confirmation.windows(2) {
            assert!(w[0].from < w[1].from);
        }
        for t in &c.teaching {
            assert!(c.sources.contains_key(&t.source), "{}", t.source);
        }
        for cr in &c.high_risk_criteria {
            assert!(c.sources.contains_key(&cr.source));
        }
        assert_eq!(c.high_risk_communities.towns.len(), 17);
    }

    #[test]
    fn high_risk_list_matches_the_snapshot() {
        let (_d, c) = ctx();
        let (rows, _) = lead_data::load(&c).unwrap();
        let mut snap: Vec<(u32, String)> = rows
            .iter()
            .filter(|r| r.high_risk())
            .map(|r| (r.hr_rank.parse().unwrap(), r.town.clone()))
            .collect();
        snap.sort();
        let names: Vec<String> = snap.into_iter().map(|(_, t)| t).collect();
        assert_eq!(names, content().high_risk_communities.towns);
    }

    #[test]
    fn capillary_result_needs_confirmation_and_town_is_checked() {
        let (_d, c) = ctx();
        let env = childhood_lead_screening_guide(
            &c,
            LeadGuideArgs {
                child_age_months: Some(26.0),
                town: Some("new bedford".into()),
                blood_lead_level: Some(12.0),
                sample_type: Some(SampleType::Capillary),
                ..Default::default()
            },
        )
        .unwrap();
        let r = env.data.result.as_ref().unwrap();
        assert!(r.confirmation.as_ref().unwrap().contains("1 month"));
        let t = env.data.town.as_ref().unwrap();
        assert!(t.high_risk_community);
        assert_eq!(t.rank, Some(1));
        assert_eq!(env.data.screening.len(), 4);
        assert!(env.data.screening[1].status.starts_with("due now"));
        assert!(env.caveats.iter().any(|c| c.contains("2023")));
        assert!(env.notice.is_some());
    }

    #[test]
    fn venous_bands_and_edges() {
        assert_eq!(
            venous_band(4.9).category,
            "At or above the CDC reference value"
        );
        assert_eq!(
            venous_band(5.0).category,
            "Blood lead level of concern (Massachusetts)"
        );
        assert_eq!(
            venous_band(9.9).category,
            "Blood lead level of concern (Massachusetts)"
        );
        assert_eq!(venous_band(10.0).category, "Lead poisoned");
        assert!(venous_band(72.0).category.contains("emergency"));
        assert!(capillary_band(4.95).unwrap().timing.contains("3 months"));
        assert!(capillary_band(70.0).unwrap().timing.contains("emergency"));
        assert!(capillary_band(2.0).is_none());
        let g = result_guidance(10.0, SampleType::Venous);
        assert!(g.reporting.contains("3 working days"));
    }

    #[test]
    fn teaching_sheet_falls_back_to_english_and_uses_agency_translation() {
        let (d, c) = ctx();
        let env = childhood_lead_screening_guide(
            &c,
            LeadGuideArgs {
                teaching_sheet: true,
                language: Some("Portuguese".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.teaching_sheet_language.as_deref(), Some("English"));
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("No agency-approved Portuguese"))
        );
        let f = env.data.teaching_sheet_file.unwrap();
        assert!(d.path().join("data").join(&f).exists());
        std::fs::create_dir_all(d.path().join("data/templates/spanish")).unwrap();
        std::fs::write(
            d.path()
                .join("data/templates/spanish/lead_family_teaching.md"),
            "# Proteja a su hijo del plomo\n\nTexto aprobado por la agencia.",
        )
        .unwrap();
        let env = childhood_lead_screening_guide(
            &c,
            LeadGuideArgs {
                teaching_sheet: true,
                language: Some("es".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            env.data
                .teaching_sheet_language
                .unwrap()
                .contains("agency translation")
        );
    }

    #[test]
    fn bad_inputs() {
        let (_d, c) = ctx();
        let e = childhood_lead_screening_guide(
            &c,
            LeadGuideArgs {
                blood_lead_level: Some(8.0),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("sample_type"));
        let e = childhood_lead_screening_guide(
            &c,
            LeadGuideArgs {
                town: Some("Worchester".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("Worcester"));
    }
}
