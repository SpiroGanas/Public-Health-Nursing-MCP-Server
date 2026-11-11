//! MCH-07 `mandated_reporter_51a_guide`.
//!
//! Massachusetts child abuse and neglect reporting under M.G.L. c. 119, s. 51A: who must
//! report, the immediate oral report and the written report within 48 hours, what the report
//! contains, protections and penalties, definitions, domestic violence guidance, and DCF's
//! process. The written-report narrative follows "template + local fill": the tool returns the
//! template (sections and field names) and, when pointed at the nurse's notes in /data, fills a
//! draft written to /data/output. Nothing from the notes is ever returned.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::{Duration, NaiveDateTime};
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self as mc, CitedText, NoteOut, SourceRef};
use crate::context::Context;
use crate::md;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Step {
    step: u32,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TemplateSection {
    heading: String,
    fields: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Template {
    title: String,
    sections: Vec<TemplateSection>,
    related_concerns: Vec<String>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReportingContent {
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    who_must_report: Vec<CitedText>,
    when_to_report: Vec<CitedText>,
    how_to_report: Vec<Step>,
    report_contents: Vec<CitedText>,
    penalties_and_protections: Vec<CitedText>,
    definitions: Vec<CitedText>,
    domestic_violence: Vec<CitedText>,
    dcf_process: Vec<CitedText>,
    warning_signs: Vec<CitedText>,
    documentation_tips: Vec<CitedText>,
    hotline: String,
    template: Template,
}

static CONTENT: LazyLock<ReportingContent> =
    LazyLock::new(|| mc::load::<ReportingContent>("mch-mandated-reporting"));

fn content() -> &'static ReportingContent {
    &CONTENT
}

// ---------------------------------------------------------------- arguments

/// Kinds of concern (never names or details).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConcernType {
    PhysicalAbuse,
    SexualAbuse,
    EmotionalAbuse,
    Neglect,
    SexualExploitation,
    HumanTrafficking,
    PrenatalSubstanceExposure,
    DomesticViolence,
    ChildDeath,
}

/// Parts of the guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportingTopic {
    Who,
    When,
    How,
    Contents,
    Protections,
    Definitions,
    DomesticViolence,
    Process,
    WarningSigns,
    Documentation,
}

/// Arguments for `mandated_reporter_51a_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct MandatedReporterArgs {
    /// Parts to include (default all): who, when, how, contents, protections, definitions,
    /// domestic_violence, process, warning_signs, documentation.
    #[serde(default)]
    pub topics: Vec<ReportingTopic>,
    /// Kinds of concern, to tailor the guidance (physical_abuse, sexual_abuse,
    /// emotional_abuse, neglect, sexual_exploitation, human_trafficking,
    /// prenatal_substance_exposure, domestic_violence, child_death). Do not send names or
    /// details of the family.
    #[serde(default)]
    pub concerns: Vec<ConcernType>,
    /// True when the nurse works for an agency, school, or facility with a person in charge
    /// who can be notified instead.
    #[serde(default)]
    pub works_for_facility: Option<bool>,
    /// When the oral report was made (YYYY-MM-DD HH:MM), to compute the 48-hour deadline for
    /// the written report.
    #[serde(default)]
    pub oral_report_time: Option<String>,
    /// Include the written-report narrative template (sections and field names).
    #[serde(default)]
    pub narrative_template: bool,
    /// The nurse's notes in /data for the written report: a .txt/.md file with "Field name:
    /// value" lines (field names from the template), or a CSV/XLSX with one row per report and
    /// columns named like the fields. A draft (DOCX) is written to /data/output; nothing from
    /// the notes is returned.
    #[serde(default)]
    pub notes_path: Option<String>,
    /// Sheet name for an XLSX notes file.
    #[serde(default)]
    pub sheet: Option<String>,
}

// ---------------------------------------------------------------- results

/// A template section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateSectionOut {
    pub heading: String,
    /// Field names (use them as "Field name: value" lines or as column names).
    pub fields: Vec<String>,
}

/// The narrative template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NarrativeTemplate {
    pub title: String,
    pub sections: Vec<TemplateSectionOut>,
    /// DCF's "related concerns" checklist; name any that apply in the Related Concerns field.
    pub related_concerns_checklist: Vec<String>,
    pub instructions: Vec<String>,
    pub citation: String,
}

/// Counts for one filled draft (field names only, never values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DraftSummary {
    /// Draft number (1-based, in file order).
    pub draft: usize,
    pub file: String,
    pub fields_filled: usize,
    /// Template fields left blank (marked "?" in the draft, as DCF's form asks).
    pub fields_missing: Vec<String>,
}

/// Result of `mandated_reporter_51a_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MandatedReporterResult {
    pub hotline: String,
    pub steps: Vec<NoteOut>,
    /// Deadline for the written report (oral report time + 48 hours), when given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written_report_due: Option<String>,
    /// Guidance by topic.
    pub guidance: BTreeMap<String, Vec<NoteOut>>,
    /// Notes specific to the concerns given.
    pub concern_notes: Vec<NoteOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<NarrativeTemplate>,
    /// Drafts written from the nurse's notes (counts and field names only).
    pub drafts: Vec<DraftSummary>,
    /// Labels or columns in the notes that did not match a template field (count only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unmatched_labels: Option<usize>,
}

// ---------------------------------------------------------------- template fill

fn norm(s: &str) -> String {
    phn_core::text::normalize(s)
}

fn all_fields() -> Vec<&'static str> {
    content()
        .template
        .sections
        .iter()
        .flat_map(|s| s.fields.iter().map(String::as_str))
        .collect()
}

/// Parse "Field: value" notes; lines that do not start with a template field continue the
/// previous field. Returns the record and the number of unmatched labels.
fn parse_notes(text: &str) -> (BTreeMap<String, String>, usize) {
    let fields = all_fields();
    let mut rec: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut unmatched = 0usize;
    for line in text.lines() {
        let l = line.trim_end();
        if let Some((k, v)) = l.split_once(':') {
            let nk = norm(k.trim_start_matches(['-', '*', '#', ' ']));
            if let Some(f) = fields.iter().find(|f| norm(f) == nk) {
                current = Some((*f).to_string());
                rec.entry((*f).to_string()).or_default().push_str(v.trim());
                continue;
            } else if k.split_whitespace().count() <= 4 {
                // Looks like a label; only the count is ever reported.
                unmatched += 1;
            }
        }
        if let Some(c) = &current
            && !l.trim().is_empty()
        {
            let e = rec.entry(c.clone()).or_default();
            if !e.is_empty() {
                e.push('\n');
            }
            e.push_str(l.trim());
        }
    }
    (rec, unmatched)
}

/// Records read from the notes, unmatched labels, the file label, and identifier categories.
type NotesRead = (Vec<BTreeMap<String, String>>, usize, String, Vec<String>);

/// Read the notes file: one record from a text file, one per row from a table.
fn read_notes(ctx: &Context, path: &str, sheet: Option<&str>) -> Result<NotesRead> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let label = sandbox.display_path(&p);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "csv" | "xlsx" | "xls" | "tsv") {
        let (table, label) = crate::core::files::load_table(ctx, path, sheet)?;
        let fields = all_fields();
        let mut map: Vec<(usize, &str)> = Vec::new();
        let mut unmatched = 0usize;
        for (i, h) in table.headers.iter().enumerate() {
            match fields.iter().find(|f| norm(f) == norm(h)) {
                Some(f) => map.push((i, f)),
                None => unmatched += 1,
            }
        }
        if map.is_empty() {
            return Err(PhnError::InvalidInput(
                "no columns in the notes file match the template's field names (ask for \
                 narrative_template to see them)"
                    .into(),
            ));
        }
        if table.rows.len() > 50 {
            return Err(PhnError::InvalidInput("at most 50 reports per call".into()));
        }
        let kinds = phn_local::phi::scan_table(&table)
            .into_iter()
            .map(|f| f.kind.label().to_string())
            .collect();
        let recs = table
            .rows
            .iter()
            .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
            .map(|r| {
                map.iter()
                    .filter_map(|(i, f)| {
                        r.get(*i)
                            .map(|v| v.trim())
                            .filter(|v| !v.is_empty())
                            .map(|v| ((*f).to_string(), v.to_string()))
                    })
                    .collect()
            })
            .collect();
        return Ok((recs, unmatched, label, kinds));
    }
    if !matches!(ext.as_str(), "txt" | "md" | "text" | "markdown") {
        return Err(PhnError::InvalidInput(
            "notes_path must be a .txt or .md file with \"Field name: value\" lines, or a \
             CSV/XLSX with one row per report"
                .into(),
        ));
    }
    let text = std::fs::read_to_string(&p)
        .map_err(|e| PhnError::InvalidInput(format!("cannot read the notes file ({e})")))?;
    let (rec, unmatched) = parse_notes(&text);
    if rec.is_empty() {
        return Err(PhnError::InvalidInput(
            "no lines in the notes file start with a template field name followed by a colon \
             (for example \"Nature and Extent: ...\"); ask for narrative_template to see the \
             field names"
                .into(),
        ));
    }
    let kinds = phn_local::phi::scan_text(&text)
        .into_iter()
        .map(|f| f.kind.label().to_string())
        .collect();
    Ok((vec![rec], unmatched, label, kinds))
}

fn draft_document(ctx: &Context, rec: &BTreeMap<String, String>) -> (Document, usize, Vec<String>) {
    let c = content();
    let t = &c.template;
    let mut blocks = vec![Block::Paragraph(vec![Inline::plain(
        "DRAFT prepared from the nurse's notes for the written report required within 48 \
         hours of the oral report (M.G.L. c. 119, s. 51A). Review every entry, then submit it \
         through DCF's online mandated reporter form or on DCF's Child Abuse Reporting Form to \
         the area office that serves the child's town. '?' marks information that is uncertain \
         or unknown, as DCF's form asks.",
    )])];
    let mut filled = 0usize;
    let mut missing = Vec::new();
    for sec in &t.sections {
        blocks.push(Block::Heading {
            level: 2,
            text: sec.heading.clone(),
        });
        for f in &sec.fields {
            let v = rec.get(f).map(String::as_str).unwrap_or("").trim();
            let shown = if v.is_empty() {
                missing.push(f.clone());
                "?".to_string()
            } else {
                filled += 1;
                v.to_string()
            };
            if f == "Related Concerns" {
                let lower = shown.to_lowercase();
                let items: Vec<Vec<Inline>> = t
                    .related_concerns
                    .iter()
                    .map(|rc| {
                        let mark = if lower.contains(&rc.to_lowercase()) {
                            "[x]"
                        } else {
                            "[ ]"
                        };
                        vec![Inline::plain(format!("{mark} {rc}"))]
                    })
                    .collect();
                blocks.push(Block::Paragraph(vec![Inline {
                    text: format!("{f}:"),
                    bold: true,
                    ..Inline::default()
                }]));
                blocks.push(Block::List {
                    ordered: false,
                    items,
                });
                continue;
            }
            let mut runs = vec![Inline {
                text: format!("{f}: "),
                bold: true,
                ..Inline::default()
            }];
            runs.push(Inline::plain(shown));
            blocks.push(Block::Paragraph(runs));
        }
    }
    blocks.push(Block::Heading {
        level: 2,
        text: "Signature of reporter".into(),
    });
    blocks.push(Block::Paragraph(vec![Inline::plain(
        "Signature: ______________________________   Date: ______________",
    )]));
    (
        Document {
            title: t.title.clone(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks,
        },
        filled,
        missing,
    )
}

// ---------------------------------------------------------------- the tool

fn topic_key(t: ReportingTopic) -> &'static str {
    match t {
        ReportingTopic::Who => "who_must_report",
        ReportingTopic::When => "when_to_report",
        ReportingTopic::How => "how_to_report",
        ReportingTopic::Contents => "report_contents",
        ReportingTopic::Protections => "penalties_and_protections",
        ReportingTopic::Definitions => "definitions",
        ReportingTopic::DomesticViolence => "domestic_violence",
        ReportingTopic::Process => "dcf_process",
        ReportingTopic::WarningSigns => "warning_signs",
        ReportingTopic::Documentation => "documentation_tips",
    }
}

const ALL_TOPICS: [ReportingTopic; 10] = [
    ReportingTopic::Who,
    ReportingTopic::When,
    ReportingTopic::How,
    ReportingTopic::Contents,
    ReportingTopic::Protections,
    ReportingTopic::Definitions,
    ReportingTopic::DomesticViolence,
    ReportingTopic::Process,
    ReportingTopic::WarningSigns,
    ReportingTopic::Documentation,
];

fn topic_notes(t: ReportingTopic) -> Vec<NoteOut> {
    let c = content();
    let s = &c.sources;
    match t {
        ReportingTopic::Who => mc::notes(s, &c.who_must_report),
        ReportingTopic::When => mc::notes(s, &c.when_to_report),
        ReportingTopic::How => steps(),
        ReportingTopic::Contents => mc::notes(s, &c.report_contents),
        ReportingTopic::Protections => mc::notes(s, &c.penalties_and_protections),
        ReportingTopic::Definitions => mc::notes(s, &c.definitions),
        ReportingTopic::DomesticViolence => mc::notes(s, &c.domestic_violence),
        ReportingTopic::Process => mc::notes(s, &c.dcf_process),
        ReportingTopic::WarningSigns => mc::notes(s, &c.warning_signs),
        ReportingTopic::Documentation => mc::notes(s, &c.documentation_tips),
    }
}

fn steps() -> Vec<NoteOut> {
    let c = content();
    c.how_to_report
        .iter()
        .map(|st| NoteOut {
            text: format!("{}. {}", st.step, st.text),
            citation: mc::cite(&c.sources, &st.source, &st.section),
            confidence: None,
        })
        .collect()
}

fn concern_notes(concerns: &[ConcernType], facility: Option<bool>) -> Vec<NoteOut> {
    let c = content();
    let s = &c.sources;
    let mut out = Vec::new();
    for k in concerns {
        match k {
            ConcernType::PrenatalSubstanceExposure => {
                out.extend(
                    c.when_to_report
                        .iter()
                        .filter(|n| n.text.contains("prenatal") || n.text.contains("addictive"))
                        .map(|n| mc::note(s, n)),
                );
            }
            ConcernType::DomesticViolence => out.extend(mc::notes(s, &c.domestic_violence)),
            ConcernType::ChildDeath => out.extend(
                c.how_to_report
                    .iter()
                    .filter(|st| st.text.contains("died"))
                    .map(|st| NoteOut {
                        text: st.text.clone(),
                        citation: mc::cite(s, &st.source, &st.section),
                        confidence: None,
                    }),
            ),
            ConcernType::SexualExploitation | ConcernType::HumanTrafficking => out.push(NoteOut {
                text: "A sexually exploited child or a human trafficking victim is reportable \
                       whether or not the person responsible is a caregiver; DCF must notify \
                       the district attorney and local law enforcement when it finds sexual \
                       abuse or exploitation or trafficking."
                    .into(),
                citation: mc::cite(
                    s,
                    "mgl-51a",
                    "s. 51A(a)(iii)-(iv); DCF guide, Referrals to the District Attorney",
                ),
                confidence: None,
            }),
            _ => {}
        }
    }
    if facility == Some(true) {
        out.extend(
            c.how_to_report
                .iter()
                .filter(|st| st.text.contains("person in charge"))
                .map(|st| NoteOut {
                    text: st.text.clone(),
                    citation: mc::cite(s, &st.source, &st.section),
                    confidence: None,
                }),
        );
    }
    out.dedup_by(|a, b| a.text == b.text);
    out
}

fn template_out() -> NarrativeTemplate {
    let c = content();
    let t = &c.template;
    NarrativeTemplate {
        title: t.title.clone(),
        sections: t
            .sections
            .iter()
            .map(|s| TemplateSectionOut {
                heading: s.heading.clone(),
                fields: s.fields.clone(),
            })
            .collect(),
        related_concerns_checklist: t.related_concerns.clone(),
        instructions: vec![
            "Write the notes in a .txt or .md file in your data folder, one 'Field name: value' \
             line per field (a value can continue on the following lines), or in a CSV/XLSX \
             with one row per report and these field names as column headers. Then call this \
             tool with notes_path; the filled draft is saved in your output folder and its \
             contents are not shown in the chat."
                .into(),
            "Do not paste the family's names or details into the chat: the AI vendor receives \
             everything typed there."
                .into(),
        ]
        .into_iter()
        .chain(c.documentation_tips.iter().map(|d| d.text.clone()))
        .collect(),
        citation: mc::cite(&c.sources, &t.source, &t.section),
    }
}

/// Massachusetts mandated reporting guidance and the written-report narrative.
pub fn mandated_reporter_51a_guide(
    ctx: &Context,
    args: MandatedReporterArgs,
) -> Result<Envelope<MandatedReporterResult>> {
    let c = content();
    let due = match &args.oral_report_time {
        Some(t) => {
            let parsed = [
                "%Y-%m-%d %H:%M",
                "%Y-%m-%dT%H:%M",
                "%Y-%m-%d %H:%M:%S",
                "%m/%d/%Y %H:%M",
            ]
            .iter()
            .find_map(|f| NaiveDateTime::parse_from_str(t.trim(), f).ok())
            .ok_or_else(|| {
                PhnError::InvalidInput("oral_report_time should look like 2026-11-08 14:30".into())
            })?;
            let due = parsed
                .checked_add_signed(Duration::hours(48))
                .ok_or_else(|| PhnError::InvalidInput("oral_report_time is out of range".into()))?;
            Some(due.format("%Y-%m-%d %H:%M").to_string())
        }
        None => None,
    };
    let topics: Vec<ReportingTopic> = if args.topics.is_empty() {
        ALL_TOPICS.to_vec()
    } else {
        args.topics.clone()
    };
    let mut guidance = BTreeMap::new();
    for t in &topics {
        guidance.insert(topic_key(*t).to_string(), topic_notes(*t));
    }
    let concern = concern_notes(&args.concerns, args.works_for_facility);

    // Optional local fill.
    let mut drafts = Vec::new();
    let mut unmatched = None;
    let mut privacy = Privacy::none();
    let mut extra_sources: Vec<Source> = Vec::new();
    if let Some(path) = &args.notes_path {
        let (records, unm, label, mut kinds) = read_notes(ctx, path, args.sheet.as_deref())?;
        let sandbox = ctx.sandbox()?;
        for (i, rec) in records.iter().enumerate() {
            let (doc, filled, missing) = draft_document(ctx, rec);
            let out = sandbox.new_output_file("51a_written_report_draft.docx")?;
            std::fs::write(&out, phn_docs::docx::to_docx(&doc)?)?;
            let disp = sandbox.display_path(&out);
            privacy.files_written.push(disp.clone());
            drafts.push(DraftSummary {
                draft: i + 1,
                file: disp,
                fields_filled: filled,
                fields_missing: missing,
            });
        }
        unmatched = Some(unm);
        kinds.sort();
        kinds.dedup();
        privacy.identifiers_detected = kinds;
        privacy.notes.push(
            "The drafts contain identifying details from your notes and are only in the output \
             folder; nothing from the notes is included in this response."
                .into(),
        );
        extra_sources.push(Source::local_file(label));
    }

    // ---- Markdown
    let mut s = String::from(
        "**Mandated reporting of child abuse or neglect (M.G.L. c. 119, s. 51A)**\n\n",
    );
    s.push_str(&format!(
        "As a nurse you are a mandated reporter. **Call DCF immediately** (area office for the \
         child's town during business hours; nights, weekends, and holidays the Child-At-Risk \
         Hotline **{}**), then **file the written report within 48 hours**. If a child is in \
         immediate danger, call 911.\n\n",
        c.hotline
    ));
    if let Some(d) = &due {
        s.push_str(&format!(
            "**Written report due by {d}** (48 hours after the call).\n\n"
        ));
    }
    s.push_str("**Steps**\n\n");
    for st in steps() {
        s.push_str(&mc::bullet(&st));
    }
    if !concern.is_empty() {
        s.push_str("\n**For this kind of concern**\n\n");
        for n in &concern {
            s.push_str(&mc::bullet(n));
        }
    }
    for t in &topics {
        if *t == ReportingTopic::How {
            continue;
        }
        let notes = &guidance[topic_key(*t)];
        if notes.is_empty() {
            continue;
        }
        s.push_str(&format!(
            "\n**{}**\n\n",
            match t {
                ReportingTopic::Who => "Who must report",
                ReportingTopic::When => "When to report",
                ReportingTopic::How => "How to report",
                ReportingTopic::Contents => "What the report contains",
                ReportingTopic::Protections => "Penalties and protections",
                ReportingTopic::Definitions => "Definitions (110 CMR 2.00)",
                ReportingTopic::DomesticViolence => "Domestic violence",
                ReportingTopic::Process => "What DCF does next",
                ReportingTopic::WarningSigns => "Warning signs",
                ReportingTopic::Documentation => "Documenting",
            }
        ));
        for n in notes {
            s.push_str(&mc::bullet(n));
        }
    }
    let template = (args.narrative_template || args.notes_path.is_some()).then(template_out);
    if let Some(t) = &template
        && args.notes_path.is_none()
    {
        s.push_str("\n**Written-report narrative template**\n\n");
        s.push_str(&md::table(
            &["Section", "Fields"],
            &t.sections
                .iter()
                .map(|x| vec![x.heading.clone(), x.fields.join("; ")])
                .collect::<Vec<_>>(),
        ));
        s.push_str(&format!("\n{}\n", t.instructions[0]));
    }
    if !drafts.is_empty() {
        s.push_str(&format!(
            "\n**Drafts written** ({}; contents not shown):\n\n",
            drafts.len()
        ));
        for d in &drafts {
            s.push_str(&format!(
                "- `{}`: {} field(s) filled; {} marked '?'{}\n",
                d.file,
                d.fields_filled,
                d.fields_missing.len(),
                if d.fields_missing.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", d.fields_missing.join(", "))
                }
            ));
        }
        if unmatched.is_some_and(|u| u > 0) {
            s.push_str(&format!(
                "\n{} label(s) or column(s) in the notes did not match a template field and \
                 were kept with the previous field (text files) or left out (tables).\n",
                unmatched.unwrap_or(0)
            ));
        }
    }

    let mut caveats = vec![mc::review_caveat(&c.meta, "mandated reporting")];
    caveats.push(
        "This is guidance, not legal advice. When unsure whether to report, call DCF: the \
         screener decides; you need reasonable cause to believe, not proof. Follow your \
         agency's policy, and consult your supervisor or counsel for legal questions."
            .into(),
    );
    if args
        .concerns
        .contains(&ConcernType::PrenatalSubstanceExposure)
    {
        caveats.push(
            "Prenatal substance exposure: the statute and DCF's guide disagree (see the \
             concern notes). Confirm current DCF practice before deciding."
                .into(),
        );
    }
    if !drafts.is_empty() {
        caveats.push(
            "Drafts are prepared from your notes for your review; DCF accepts the written \
             report through its online form or its Child Abuse Reporting Form."
                .into(),
        );
    }
    let mut keys: Vec<&str> = c.sources.keys().map(String::as_str).collect();
    keys.sort();
    let data = MandatedReporterResult {
        hotline: c.hotline.clone(),
        steps: steps(),
        written_report_due: due,
        guidance,
        concern_notes: concern,
        template,
        drafts,
        unmatched_labels: unmatched,
    };
    Ok(Envelope::new(s, data)
        .sources(extra_sources)
        .sources(mc::sources_for(&c.sources, keys))
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
            .who_must_report
            .iter()
            .chain(&c.when_to_report)
            .chain(&c.report_contents)
            .chain(&c.definitions)
        {
            assert!(c.sources.contains_key(&n.source), "{}", n.source);
        }
        assert!(all_fields().contains(&"Nature and Extent"));
    }

    #[test]
    fn guidance_deadline_and_concerns() {
        let (_d, c) = ctx();
        let env = mandated_reporter_51a_guide(
            &c,
            MandatedReporterArgs {
                oral_report_time: Some("2026-11-06 15:20".into()),
                concerns: vec![
                    ConcernType::PrenatalSubstanceExposure,
                    ConcernType::DomesticViolence,
                ],
                works_for_facility: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            env.data.written_report_due.as_deref(),
            Some("2026-11-08 15:20")
        );
        assert!(env.summary.contains("48 hours"));
        assert!(
            env.data
                .concern_notes
                .iter()
                .any(|n| n.text.contains("prenatal"))
        );
        assert!(
            env.data
                .concern_notes
                .iter()
                .any(|n| n.text.contains("person in charge"))
        );
        assert!(env.notice.is_some());
        assert_eq!(env.data.guidance.len(), 10);
    }

    #[test]
    fn notes_fill_a_draft_without_returning_details() {
        let (d, c) = ctx();
        std::fs::write(
            d.path().join("data/notes.txt"),
            "Child Name: Wrenna Quillfeather\nChild Age or Date of Birth: 4\nNature and Extent: \
             Bruises on both upper arms.\nThe child said \"Dad grabbed me\".\nMom said: she \
             fell.\nActions Taken: Called DCF screener.\nRelated Concerns: Domestic Violence\n",
        )
        .unwrap();
        let env = mandated_reporter_51a_guide(
            &c,
            MandatedReporterArgs {
                notes_path: Some("notes.txt".into()),
                topics: vec![ReportingTopic::How],
                ..Default::default()
            },
        )
        .unwrap();
        let all = serde_json::to_string(&env).unwrap();
        assert!(
            !all.contains("Quillfeather") && !all.contains("Wrenna") && !all.contains("Bruises")
        );
        assert_eq!(env.data.drafts.len(), 1);
        let dr = &env.data.drafts[0];
        assert_eq!(dr.fields_filled, 5);
        assert!(dr.fields_missing.contains(&"Caregiver 1 Name".to_string()));
        assert!(d.path().join("data").join(&dr.file).exists());
        assert!(env.privacy.files_written.contains(&dr.file));
        let (rec, _) = parse_notes("Nature and Extent: a\nline two\nMom said: b\nActions Taken: c");
        assert_eq!(rec["Nature and Extent"], "a\nline two\nMom said: b");
    }

    #[test]
    fn bad_inputs() {
        let (d, c) = ctx();
        let e = mandated_reporter_51a_guide(
            &c,
            MandatedReporterArgs {
                oral_report_time: Some("yesterday".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("2026-11-08 14:30"));
        std::fs::write(d.path().join("data/empty.txt"), "nothing useful here").unwrap();
        let e = mandated_reporter_51a_guide(
            &c,
            MandatedReporterArgs {
                notes_path: Some("empty.txt".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("field name"));
    }
}
