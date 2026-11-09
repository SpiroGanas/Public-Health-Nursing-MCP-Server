//! AGE-03 `elder_protective_services_guide`: Massachusetts reporting rules for
//! abuse, neglect, financial exploitation, and self-neglect of adults 60 and
//! older (Adult Protective Services, M.G.L. c.19A) and of adults 18 to 59
//! with disabilities (DPPC, M.G.L. c.19C), plus a report-narrative template.
//!
//! Template + local fill: the narrative template is returned with guidance;
//! with `details_file` (a CSV/XLSX in /data, one row per report), the
//! narratives are filled and written to /data/output only. Identifying
//! details are never accepted as arguments.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{
    self as c10, CitedOut, CitedText, Confidence, PROTECTIVE, SourceRef, cite_all, cite_one,
};
use crate::aging::localfill::{self, FillOutcome, FillSpec};
use crate::context::Context;
use crate::md;

#[derive(Debug, Clone, Deserialize)]
struct PathwayDef {
    id: String,
    name: String,
    who: String,
    age_min: Option<u32>,
    age_max: Option<u32>,
    #[allow(dead_code)]
    setting: String,
    hotline: String,
    hours: String,
    how: String,
    online: Option<String>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Definition {
    term: String,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Reporters {
    list: String,
    trigger: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MandatedReporters {
    elder: Reporters,
    disabled: Reporters,
}

#[derive(Debug, Clone, Deserialize)]
struct Timeline {
    step: String,
    when: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Sign {
    #[serde(rename = "type")]
    kind: String,
    signs: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NarrativeSection {
    heading: String,
    fields: Vec<String>,
    guidance: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Narrative {
    title: String,
    sections: Vec<NarrativeSection>,
    confidence: Confidence,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProtectiveContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    pathways: Vec<PathwayDef>,
    emergency: Vec<CitedText>,
    definitions: Vec<Definition>,
    mandated_reporters: MandatedReporters,
    duties: Vec<CitedText>,
    report_contents: CitedText,
    timelines: Vec<Timeline>,
    protections: Vec<CitedText>,
    penalties: Vec<CitedText>,
    after_report: Vec<CitedText>,
    self_neglect: Vec<CitedText>,
    signs: Vec<Sign>,
    narrative_template: Narrative,
}

static CONTENT: LazyLock<ProtectiveContent> =
    LazyLock::new(|| c10::load(PROTECTIVE).expect("protective services content parses"));

/// Where the person lives or was harmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CareSetting {
    /// Home or other community setting (includes assisted living residences).
    Community,
    /// Nursing home, rest home, or hospital.
    Facility,
}

/// Arguments for `elder_protective_services_guide`. Never give names,
/// addresses, or other identifying details here: put them in `details_file`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ProtectiveArgs {
    /// Age of the person of concern in years (60+ goes to Adult Protective
    /// Services; 18-59 with a disability to DPPC).
    pub age_years: Option<u32>,
    /// For ages 18-59: the person has a disability and depends on others for
    /// daily living needs.
    pub has_disability: Option<bool>,
    /// community (home, assisted living) or facility (nursing home, rest
    /// home, hospital).
    pub setting: Option<CareSetting>,
    /// The concern: physical, sexual, emotional, caregiver neglect,
    /// financial exploitation, or self-neglect (free text is matched).
    pub concern: Option<String>,
    /// The person is in immediate danger.
    pub emergency: bool,
    /// The reporter's role (e.g. "public health nurse") to say whether they
    /// are a mandated reporter.
    pub reporter_role: Option<String>,
    /// Template + local fill: a CSV/XLSX in /data with one row per report and
    /// columns named like the template fields (Name, Address, Approximate
    /// Age, Observations, ...). Narratives are written to /data/output only.
    pub details_file: Option<String>,
    /// Sheet name for an XLSX details file.
    pub sheet: Option<String>,
    /// Format for filled narratives: "docx" (default) or "pdf".
    pub format: Option<String>,
}

/// A reporting pathway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PathwayOut {
    pub id: String,
    pub name: String,
    pub who: String,
    /// Ages covered, when the pathway is age-based.
    pub ages: Option<String>,
    pub hotline: String,
    pub hours: String,
    pub how: String,
    pub online: Option<String>,
    pub citation: String,
}

/// A step and when it happens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimelineOut {
    pub step: String,
    pub when: String,
    pub citation: String,
}

/// A narrative template section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateSection {
    pub heading: String,
    /// Field names for the details file columns ({{Field}} placeholders).
    pub fields: Vec<String>,
    pub guidance: String,
}

/// Result of `elder_protective_services_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProtectiveResult {
    pub pathways: Vec<PathwayOut>,
    /// Why these pathways were chosen.
    pub routing: String,
    pub emergency_steps: Vec<CitedOut>,
    /// Whether the reporter's role is a mandated reporter (null if no role given).
    pub mandated_reporter: Option<bool>,
    pub mandated_reporter_lists: Vec<CitedOut>,
    pub duties: Vec<CitedOut>,
    pub report_contents: CitedOut,
    pub timelines: Vec<TimelineOut>,
    pub definitions: Vec<CitedOut>,
    pub signs: Vec<CitedOut>,
    pub self_neglect: Vec<CitedOut>,
    pub after_report: Vec<CitedOut>,
    pub protections_and_penalties: Vec<CitedOut>,
    pub narrative_template: Vec<TemplateSection>,
    /// Column names for a details file (one row per report).
    pub details_file_columns: Vec<String>,
    pub local_fill: Option<FillOutcome>,
}

fn cite(key: &str, section: &str) -> String {
    c10::citation(&CONTENT.sources, key, section)
}

fn concern_kind(s: &str) -> Option<&'static str> {
    let n = phn_core::text::normalize(s);
    let has = |k: &[&str]| k.iter().any(|w| n.contains(w));
    Some(
        if has(&[
            "self neglect",
            "self-neglect",
            "hoard",
            "cannot care",
            "can t care",
        ]) {
            "Self-neglect"
        } else if has(&["financ", "money", "scam", "exploit", "theft", "stole"]) {
            "Financial abuse"
        } else if has(&["sexual", "rape", "assault sex"]) {
            "Sexual abuse"
        } else if has(&["caregiver neglect", "caretaker neglect", "neglect"]) {
            "Caregiver neglect"
        } else if has(&["emotional", "verbal", "threat", "intimidat", "humiliat"]) {
            "Emotional abuse"
        } else if has(&["physical", "hit", "bruise", "slap", "push", "restrain"]) {
            "Physical abuse, neglect, or mistreatment"
        } else {
            return None;
        },
    )
}

fn is_mandated(role: &str) -> Option<bool> {
    let n = phn_core::text::normalize(role);
    if n.is_empty() {
        return None;
    }
    let roles = [
        "nurse",
        "physician",
        "doctor",
        "physician assistant",
        "social worker",
        "police",
        "firefighter",
        "emt",
        "emergency medical",
        "psychologist",
        "dentist",
        "podiatrist",
        "osteopath",
        "physical therapist",
        "occupational therapist",
        "coroner",
        "council on aging",
        "outreach worker",
        "probation",
        "family counselor",
        "medical intern",
        "home health",
        "homemaker",
        "assisted living",
        "animal control",
    ];
    Some(roles.iter().any(|r| n.contains(r)))
}

fn template_document(title: &str, sections: &[TemplateSection]) -> Document {
    let mut blocks = vec![Block::Paragraph(vec![Inline::plain(
        "Prepared for a report under M.G.L. c.19A s.15 (adults 60+) or c.19C s.10 (adults 18-59 with disabilities). Keep with the verbal report record; the written report or EOAI mandated reporter form is due within 48 hours of a phone report.",
    )])];
    for s in sections {
        blocks.push(Block::Heading {
            level: 2,
            text: s.heading.clone(),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some(s.heading.clone()),
            headers: vec!["Item".into(), "Details".into()],
            rows: s
                .fields
                .iter()
                .map(|f| vec![f.clone(), format!("{{{{{f}}}}}")])
                .collect(),
        }));
        blocks.push(Block::Paragraph(vec![Inline {
            text: format!("Guidance: {}", s.guidance),
            italic: true,
            ..Inline::default()
        }]));
    }
    Document {
        title: title.into(),
        subtitle: Some(
            "Confidential: contains identifying information. Store per agency policy.".into(),
        ),
        language: "en-US".into(),
        author: None,
        letterhead: None,
        logo: None,
        blocks,
    }
}

/// Explain protective services reporting and, optionally, fill narratives locally.
pub fn elder_protective_services_guide(
    ctx: &Context,
    args: ProtectiveArgs,
) -> Result<Envelope<ProtectiveResult>> {
    let c = &*CONTENT;
    let path = |id: &str| c.pathways.iter().find(|p| p.id == id);
    let (ids, routing): (Vec<&str>, String) = match (args.age_years, args.setting) {
        (Some(a), _) if a < 18 => {
            return Err(PhnError::InvalidInput(
                "this tool covers adults; for a child (under 18), report to the Department of Children and Families under M.G.L. c.119, s.51A".into(),
            ));
        }
        (_, Some(CareSetting::Facility)) => (
            vec!["dph_facility"],
            "In a nursing home, rest home, or hospital, Adult Protective Services does not investigate; report abuse of a patient by facility staff to DPH. For an adult 18-59 with a disability, DPPC also takes reports in any setting.".into(),
        ),
        (Some(a), _) if a >= 60 => (
            vec!["elder_aps"],
            format!("Age {a}: Adult Protective Services (M.G.L. c.19A) covers adults 60 and older living in the community."),
        ),
        (Some(a), _) if args.has_disability == Some(false) => {
            return Ok(no_pathway(a));
        }
        (Some(a), _) => (
            vec!["dppc"],
            format!("Age {a} (18-59): the Disabled Persons Protection Commission (M.G.L. c.19C) covers adults with an intellectual disability or another disability that makes them wholly or partly dependent on others for daily living needs{}.", if args.has_disability.is_none() { "; confirm that this applies" } else { "" }),
        ),
        (None, _) => (
            vec!["elder_aps", "dppc", "dph_facility"],
            "No age was given: adults 60+ in the community go to Adult Protective Services, adults 18-59 with disabilities to DPPC, and abuse by nursing home or hospital staff to DPH.".into(),
        ),
    };
    let mut ids = ids;
    if args.setting == Some(CareSetting::Facility)
        && args.age_years.is_some_and(|a| (18..60).contains(&a))
        && args.has_disability != Some(false)
    {
        ids.push("dppc");
    }
    let pathways: Vec<PathwayOut> = ids
        .iter()
        .filter_map(|id| path(id))
        .map(|p| PathwayOut {
            id: p.id.clone(),
            name: p.name.clone(),
            who: p.who.clone(),
            ages: match (p.age_min, p.age_max) {
                (Some(a), Some(b)) => Some(format!("{a} to {b}")),
                (Some(a), None) => Some(format!("{a} and older")),
                _ => None,
            },
            hotline: p.hotline.clone(),
            hours: p.hours.clone(),
            how: p.how.trim().to_string(),
            online: p.online.clone(),
            citation: cite(&p.source, &p.section),
        })
        .collect();
    let elder = ids.contains(&"elder_aps");
    let dppc = ids.contains(&"dppc");
    let mandated = args.reporter_role.as_deref().and_then(is_mandated);
    let mut lists = Vec::new();
    if elder || ids.len() > 1 {
        lists.push(CitedOut {
            text: format!(
                "Adults 60+: {} Duty: {}",
                c.mandated_reporters.elder.list.trim(),
                c.mandated_reporters.elder.trigger
            ),
            citation: cite(
                &c.mandated_reporters.elder.source,
                &c.mandated_reporters.elder.section,
            ),
            confidence: None,
        });
    }
    if dppc || ids.len() > 1 {
        lists.push(CitedOut {
            text: format!(
                "Adults 18-59 with disabilities: {} Duty: {}",
                c.mandated_reporters.disabled.list.trim(),
                c.mandated_reporters.disabled.trigger
            ),
            citation: cite(
                &c.mandated_reporters.disabled.source,
                &c.mandated_reporters.disabled.section,
            ),
            confidence: None,
        });
    }
    let duties: Vec<CitedOut> = c
        .duties
        .iter()
        .filter(|d| {
            let elder_only = d.source.starts_with("mgl-19a");
            let dppc_only = d.source.starts_with("mgl-19c");
            (elder || !elder_only) && (dppc || !dppc_only) || ids.len() > 1
        })
        .map(|d| cite_one(&c.sources, d))
        .collect();
    let timelines: Vec<TimelineOut> = c
        .timelines
        .iter()
        .filter(|t| {
            elder || ids.len() > 1 || t.step.starts_with("Verbal") || t.step.starts_with("Written")
        })
        .map(|t| TimelineOut {
            step: t.step.clone(),
            when: t.when.clone(),
            citation: cite(&t.source, &t.section),
        })
        .collect();
    let definitions: Vec<CitedOut> = c
        .definitions
        .iter()
        .filter(|d| {
            let dp = d.term.contains("DPPC");
            (dp && (dppc || ids.len() > 1)) || (!dp && (elder || ids.len() > 1))
        })
        .map(|d| CitedOut {
            text: format!("{}: {}", d.term, d.text),
            citation: cite(&d.source, &d.section),
            confidence: None,
        })
        .collect();
    let wanted = args.concern.as_deref().and_then(concern_kind);
    let signs: Vec<CitedOut> = c
        .signs
        .iter()
        .filter(|s| wanted.is_none_or(|w| s.kind == w))
        .map(|s| CitedOut {
            text: format!("{}: {}", s.kind, s.signs),
            citation: cite(&s.source, &s.kind),
            confidence: None,
        })
        .collect();
    let self_neglect = if wanted.is_none() || wanted == Some("Self-neglect") {
        cite_all(&c.sources, &c.self_neglect)
    } else {
        Vec::new()
    };
    let mut protections = cite_all(&c.sources, &c.protections);
    protections.extend(cite_all(&c.sources, &c.penalties));
    let sections: Vec<TemplateSection> = c
        .narrative_template
        .sections
        .iter()
        .map(|s| TemplateSection {
            heading: s.heading.clone(),
            fields: s.fields.clone(),
            guidance: s.guidance.clone(),
        })
        .collect();
    let columns: Vec<String> = sections.iter().flat_map(|s| s.fields.clone()).collect();

    // ---- optional local fill
    let mut local_fill = None;
    let mut files_written = Vec::new();
    let mut identifiers = Vec::new();
    if let Some(file) = args
        .details_file
        .as_deref()
        .filter(|f| !f.trim().is_empty())
    {
        let fmt = args
            .format
            .as_deref()
            .unwrap_or("docx")
            .trim()
            .to_ascii_lowercase();
        let outcome = localfill::fill_local(
            ctx,
            FillSpec {
                template_id: "protective_services_report_narrative",
                english: template_document(&c.narrative_template.title, &sections),
                file,
                sheet: args.sheet.as_deref(),
                format: &fmt,
                output_stem: "protective_services_report_narrative",
                translatable: false,
            },
        )?;
        files_written = outcome.output_files.clone();
        identifiers = outcome.identifiers_detected.clone();
        local_fill = Some(outcome);
    }

    // ---- markdown
    let mut s = String::from(
        "**Reporting suspected abuse, neglect, or self-neglect of an adult (Massachusetts)**\n\n",
    );
    if args.emergency {
        s.push_str("**Emergency:** ");
        s.push_str(
            &c.emergency
                .iter()
                .map(|e| e.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
        );
        s.push_str("\n\n");
    }
    s.push_str(&format!("{routing}\n\n"));
    for p in &pathways {
        s.push_str(&format!(
            "- **{}** — **{}** ({}). {}\n",
            p.name, p.hotline, p.hours, p.how
        ));
    }
    match mandated {
        Some(true) => s.push_str(&format!("\nAs a {} you are a **mandated reporter**: report immediately by phone (or online for elder reports), and in writing within 48 hours.\n", args.reporter_role.as_deref().unwrap_or("professional"))),
        Some(false) => s.push_str("\nThat role is not on the statutory mandated reporter lists, but anyone may report; check the lists below and your employer's policy.\n"),
        None => {}
    }
    s.push('\n');
    s.push_str(&md::table(
        &["Step", "When"],
        &timelines
            .iter()
            .map(|t| vec![t.step.clone(), t.when.clone()])
            .collect::<Vec<_>>(),
    ));
    s.push_str(&format!(
        "\n**What a report contains:** {}\n",
        c.report_contents.text.trim()
    ));
    if let Some(w) = wanted {
        if let Some(sg) = signs.first() {
            s.push_str(&format!(
                "\n**Signs ({w}):** {}\n",
                sg.text.split_once(": ").map(|x| x.1).unwrap_or_default()
            ));
        }
    }
    if !self_neglect.is_empty() && wanted == Some("Self-neglect") {
        s.push_str("\n**Self-neglect**\n");
        for x in &self_neglect {
            s.push_str(&format!("- {}\n", x.text));
        }
    }
    s.push_str(&format!(
        "\n**Report narrative template** (sections: {}). To fill it without sharing details here, put one row per report in a CSV/XLSX in /data with these columns and call again with `details_file`: {}.\n",
        sections.iter().map(|x| x.heading.clone()).collect::<Vec<_>>().join("; "),
        columns.join(", ")
    ));
    if let Some(o) = &local_fill {
        s.push_str(&localfill::outcome_markdown(o));
    }
    s.push_str("\n_After a report: the older adult chooses whether to accept a service plan; Adult Protective Services does not proceed without consent._\n");

    let mut keys: Vec<&str> = pathways
        .iter()
        .filter_map(|p| path(&p.id).map(|d| d.source.as_str()))
        .collect();
    keys.extend([
        "mgl-19a-14",
        "mgl-19a-15",
        "mgl-19a-16",
        "mgl-19a-18",
        "eoai-mandated",
        "eoai-signs",
    ]);
    if dppc {
        keys.extend(["mgl-19c-1", "mgl-19c-10", "mgl-19c-11"]);
    }
    if args.emergency {
        keys.push("dph-988-faq");
    }
    let mut sources = vec![c10::content_source(PROTECTIVE)?];
    sources.extend(c10::sources_for(&c.sources, keys));
    if let Some(o) = &local_fill {
        sources.push(phn_core::Source::local_file(&o.file));
    }
    let mut caveats = vec![
        "Legal decision support, not legal advice: follow your agency's policy and consult counsel on hard cases.".to_string(),
        format!(
            "The narrative template's sections follow the report content listed in {}; the documentation tips are general practice ({} confidence).",
            cite(&c.narrative_template.source, &c.narrative_template.section),
            c.narrative_template.confidence.label()
        ),
        "651 CMR 5.00 (the EOAI regulation) and the DPPC web site could not be read when this content was drafted; timelines come from the statute and EOAI's pages.".to_string(),
    ];
    caveats.extend(c10::review_caveats(&[PROTECTIVE]));
    let mut privacy_notes = vec!["Do not type names or other identifying details into the chat; use details_file so they stay on your computer.".to_string()];
    if local_fill.is_some() {
        privacy_notes.push("Filled narratives were written to the output folder; only counts and field names are returned.".into());
    }
    Ok(Envelope::new(
        s,
        ProtectiveResult {
            pathways,
            routing,
            emergency_steps: cite_all(&c.sources, &c.emergency),
            mandated_reporter: mandated,
            mandated_reporter_lists: lists,
            duties,
            report_contents: cite_one(&c.sources, &c.report_contents),
            timelines,
            definitions,
            signs,
            self_neglect,
            after_report: cite_all(&c.sources, &c.after_report),
            protections_and_penalties: protections,
            narrative_template: sections,
            details_file_columns: columns,
            local_fill,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        identifiers_detected: identifiers,
        files_written,
        notes: privacy_notes,
        ..Privacy::none()
    })
    .decision_support())
}

fn no_pathway(age: u32) -> Envelope<ProtectiveResult> {
    let c = &*CONTENT;
    let s = format!(
        "**No Massachusetts protective services agency covers this person**\n\nAt age {age} without a disability that makes them dependent on others, neither Adult Protective Services (60+) nor DPPC (18-59 with disabilities) applies. If a crime may have occurred or someone is in danger, contact the police (911 in an emergency).\n"
    );
    Envelope::new(
        s,
        ProtectiveResult {
            pathways: Vec::new(),
            routing: format!("Age {age} without a qualifying disability."),
            emergency_steps: cite_all(&c.sources, &c.emergency),
            mandated_reporter: None,
            mandated_reporter_lists: Vec::new(),
            duties: Vec::new(),
            report_contents: cite_one(&c.sources, &c.report_contents),
            timelines: Vec::new(),
            definitions: c
                .definitions
                .iter()
                .filter(|d| d.term.contains("Disabled person") || d.term == "Older adult")
                .map(|d| CitedOut {
                    text: format!("{}: {}", d.term, d.text),
                    citation: cite(&d.source, &d.section),
                    confidence: None,
                })
                .collect(),
            signs: Vec::new(),
            self_neglect: Vec::new(),
            after_report: Vec::new(),
            protections_and_penalties: Vec::new(),
            narrative_template: Vec::new(),
            details_file_columns: Vec::new(),
            local_fill: None,
        },
    )
    .sources(c10::sources_for(&c.sources, ["mgl-19a-14", "mgl-19c-1"]))
    .caveats(c10::review_caveats(&[PROTECTIVE]))
    .decision_support()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn content_sources_are_known() {
        let c = &*CONTENT;
        let mut keys: Vec<&str> = c.pathways.iter().map(|p| p.source.as_str()).collect();
        keys.extend(c.definitions.iter().map(|d| d.source.as_str()));
        keys.extend(c.timelines.iter().map(|d| d.source.as_str()));
        keys.extend(c.signs.iter().map(|d| d.source.as_str()));
        for v in [
            &c.duties,
            &c.protections,
            &c.penalties,
            &c.after_report,
            &c.self_neglect,
            &c.emergency,
        ] {
            keys.extend(v.iter().map(|d| d.source.as_str()));
        }
        keys.push(&c.narrative_template.source);
        c10::assert_keys_known(&c.sources, &keys, "protective services");
    }

    #[test]
    fn routes_by_age_disability_and_setting() {
        let (_d, ctx) = ctx();
        let ids = |a: ProtectiveArgs| {
            elder_protective_services_guide(&ctx, a)
                .unwrap()
                .data
                .pathways
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(ProtectiveArgs {
                age_years: Some(72),
                ..Default::default()
            }),
            vec!["elder_aps"]
        );
        assert_eq!(
            ids(ProtectiveArgs {
                age_years: Some(45),
                has_disability: Some(true),
                ..Default::default()
            }),
            vec!["dppc"]
        );
        assert_eq!(
            ids(ProtectiveArgs {
                age_years: Some(84),
                setting: Some(CareSetting::Facility),
                ..Default::default()
            }),
            vec!["dph_facility"]
        );
        assert_eq!(ids(ProtectiveArgs::default()).len(), 3);
        assert!(
            ids(ProtectiveArgs {
                age_years: Some(40),
                has_disability: Some(false),
                ..Default::default()
            })
            .is_empty()
        );
        assert!(
            elder_protective_services_guide(
                &ctx,
                ProtectiveArgs {
                    age_years: Some(12),
                    ..Default::default()
                }
            )
            .is_err()
        );
        let r = elder_protective_services_guide(
            &ctx,
            ProtectiveArgs {
                age_years: Some(70),
                reporter_role: Some("public health nurse".into()),
                concern: Some("hoarding, not eating".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.mandated_reporter, Some(true));
        assert!(!r.data.self_neglect.is_empty());
        assert!(r.data.timelines.iter().any(|t| t.when.contains("48 hours")));
        assert!(
            r.data
                .details_file_columns
                .contains(&"Approximate Age".to_string())
        );
    }

    #[test]
    fn details_file_is_filled_locally_without_echoing_values() {
        let (d, ctx) = ctx();
        std::fs::write(
            d.path().join("data/report.csv"),
            "Name,Address,Approximate Age,Observations\nZebulon Quackenbush,12 Elm Way,81,Unwashed dishes\n",
        )
        .unwrap();
        let r = elder_protective_services_guide(
            &ctx,
            ProtectiveArgs {
                age_years: Some(81),
                details_file: Some("report.csv".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("Quackenbush") && !json.contains("Elm Way"));
        let o = r.data.local_fill.unwrap();
        assert_eq!(o.documents_written, 1);
        assert!(o.fields_left_blank.contains(&"Caretaker Name".to_string()));
        assert_eq!(r.privacy.files_written.len(), 1);
    }
}
