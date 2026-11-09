//! AGE-07 `advance_care_planning_toolkit`: the Massachusetts health care
//! proxy (M.G.L. c.201D) with a fillable template, MOLST (flagged as not
//! verified), The Conversation Project (link only), and a facilitator script
//! and handout for community advance care planning sessions.
//!
//! Template + local fill: with `details_file`, proxy forms are filled locally
//! and written to /data/output, using an agency translation from
//! `/data/templates/<language>/health_care_proxy.md` when the row's Language
//! has one. Session materials (script, handout, blank proxy) can be written
//! as DOCX or PDF.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{self as c10, ACP, CitedOut, CitedText, Confidence, SourceRef, cite_all};
use crate::aging::localfill::{self, FillOutcome, FillSpec};
use crate::context::Context;

#[derive(Debug, Clone, Deserialize)]
struct TemplateSection {
    heading: String,
    fields: Vec<String>,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProxyTemplate {
    title: String,
    confidence: Confidence,
    sections: Vec<TemplateSection>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Molst {
    status: String,
    confidence: Confidence,
    text: String,
    warning: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ConversationProject {
    url: String,
    text: String,
    #[allow(dead_code)]
    source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct Segment {
    pub title: String,
    pub minutes: u32,
    pub script: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Script {
    confidence: Confidence,
    note: String,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, Deserialize)]
struct AcpContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    proxy_rules: Vec<CitedText>,
    proxy_template: ProxyTemplate,
    after_signing: Vec<String>,
    molst: Molst,
    conversation_project: ConversationProject,
    facilitator_script: Script,
    handout_points: Vec<String>,
}

static CONTENT: LazyLock<AcpContent> =
    LazyLock::new(|| c10::load(ACP).expect("advance care planning content parses"));

/// Arguments for `advance_care_planning_toolkit`. No names or identifiers
/// here: use `details_file` for filled forms.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AcpArgs {
    /// Length of a community session in minutes (default 60); the script's
    /// segments are scaled to fit.
    pub session_minutes: Option<u32>,
    /// Write session materials (facilitator script, handout, blank proxy) to
    /// /data/output: "docx" or "pdf".
    pub materials_format: Option<String>,
    /// Template + local fill: a CSV/XLSX in /data, one row per person, with
    /// columns named like the proxy fields (Principal Name, Agent Name, ...)
    /// and optionally Language. Filled proxies go to /data/output only.
    pub details_file: Option<String>,
    /// Sheet name for an XLSX details file.
    pub sheet: Option<String>,
    /// Format for filled proxies: "docx" (default) or "pdf".
    pub format: Option<String>,
}

/// A proxy template section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProxySectionOut {
    pub heading: String,
    pub fields: Vec<String>,
    pub text: String,
}

/// MOLST status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MolstOut {
    /// verified or not_verified.
    pub status: String,
    pub confidence: Confidence,
    pub text: String,
    pub warning: String,
}

/// Result of `advance_care_planning_toolkit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcpResult {
    pub proxy_rules: Vec<CitedOut>,
    pub proxy_template_title: String,
    pub proxy_template: Vec<ProxySectionOut>,
    /// Column names for a details file.
    pub details_file_columns: Vec<String>,
    pub after_signing: Vec<String>,
    pub molst: MolstOut,
    pub conversation_project_url: String,
    pub conversation_project: String,
    pub facilitator_script: Vec<Segment>,
    pub session_minutes: u32,
    pub handout_points: Vec<String>,
    /// Where agency translations of the proxy template go.
    pub translation_templates: String,
    pub materials_files: Vec<String>,
    pub local_fill: Option<FillOutcome>,
}

fn proxy_document(title: &str, sections: &[ProxySectionOut]) -> Document {
    let mut blocks = Vec::new();
    for s in sections {
        blocks.push(Block::Heading { level: 2, text: s.heading.clone() });
        blocks.push(Block::Paragraph(vec![Inline::plain(s.text.clone())]));
    }
    blocks.push(Block::Paragraph(vec![Inline {
        text: "Drafted from M.G.L. c.201D, s.2. Use your agency's approved form if it has one.".into(),
        italic: true,
        ..Inline::default()
    }]));
    Document {
        title: title.into(),
        subtitle: None,
        language: "en-US".into(),
        author: None,
        letterhead: None,
        logo: None,
        blocks,
    }
}

fn scaled(segments: &[Segment], minutes: u32) -> Vec<Segment> {
    let total: u32 = segments.iter().map(|s| s.minutes).sum();
    if total == 0 || minutes == total {
        return segments.to_vec();
    }
    let mut out: Vec<Segment> = segments
        .iter()
        .map(|s| Segment {
            minutes: ((f64::from(s.minutes) * f64::from(minutes) / f64::from(total)).round() as u32).max(1),
            ..s.clone()
        })
        .collect();
    // Put any rounding difference into the longest segment.
    let sum: i64 = out.iter().map(|s| i64::from(s.minutes)).sum();
    let diff = i64::from(minutes) - sum;
    if let Some(longest) = out.iter_mut().max_by_key(|s| s.minutes) {
        longest.minutes = (i64::from(longest.minutes) + diff).max(1) as u32;
    }
    out
}

/// Provide advance care planning materials and, optionally, fill proxy forms locally.
pub fn advance_care_planning_toolkit(ctx: &Context, args: AcpArgs) -> Result<Envelope<AcpResult>> {
    let c = &*CONTENT;
    let minutes = args.session_minutes.unwrap_or(60);
    if !(20..=240).contains(&minutes) {
        return Err(PhnError::InvalidInput("session_minutes must be between 20 and 240".into()));
    }
    let script = scaled(&c.facilitator_script.segments, minutes);
    let sections: Vec<ProxySectionOut> = c
        .proxy_template
        .sections
        .iter()
        .map(|s| ProxySectionOut {
            heading: s.heading.clone(),
            fields: s.fields.clone(),
            text: s.text.clone(),
        })
        .collect();
    let columns: Vec<String> = sections.iter().flat_map(|s| s.fields.clone()).collect();
    let mut files_written = Vec::new();
    let mut identifiers = Vec::new();

    // ---- session materials (no identifiers)
    let mut materials = Vec::new();
    if let Some(fmt) = args
        .materials_format
        .as_deref()
        .map(|f| f.trim().to_ascii_lowercase())
        .filter(|f| !f.is_empty() && f != "none")
    {
        if fmt != "docx" && fmt != "pdf" {
            return Err(PhnError::InvalidInput("materials_format must be \"docx\" or \"pdf\"".into()));
        }
        let sandbox = ctx.sandbox()?;
        let letterhead = ctx.settings().letterhead;
        let script_doc = Document {
            title: "Advance care planning session: facilitator script".into(),
            subtitle: Some(format!("{minutes} minutes. {}", c.facilitator_script.note)),
            language: "en-US".into(),
            author: None,
            letterhead: letterhead.clone(),
            logo: None,
            blocks: vec![Block::Table(TableBlock {
                caption: Some("Agenda".into()),
                headers: vec!["Segment".into(), "Minutes".into(), "What to say and do".into()],
                rows: script
                    .iter()
                    .map(|s| vec![s.title.clone(), s.minutes.to_string(), s.script.clone()])
                    .collect(),
            })],
        };
        let handout = Document {
            title: "Planning ahead: your health care proxy".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: letterhead.clone(),
            logo: None,
            blocks: vec![
                Block::List {
                    ordered: false,
                    items: c.handout_points.iter().map(|p| vec![Inline::plain(p.clone())]).collect(),
                },
                Block::Heading { level: 2, text: "After you sign".into() },
                Block::List {
                    ordered: false,
                    items: c.after_signing.iter().map(|p| vec![Inline::plain(p.clone())]).collect(),
                },
                Block::Paragraph(vec![Inline::plain(format!(
                    "Conversation starter guides: {}",
                    c.conversation_project.url
                ))]),
            ],
        };
        let mut blank = proxy_document(&c.proxy_template.title, &sections);
        blank = blank.map_text(&|s: &str| {
            let mut out = s.to_string();
            for f in &columns {
                out = out.replace(&format!("{{{{{f}}}}}"), "____________________");
            }
            out
        });
        blank.letterhead = letterhead;
        for (doc, stem) in [
            (script_doc, "acp_facilitator_script"),
            (handout, "acp_handout"),
            (blank, "health_care_proxy_blank"),
        ] {
            let bytes = if fmt == "docx" { phn_docs::docx::to_docx(&doc)? } else { phn_docs::pdf::to_pdf(&doc)? };
            let p = sandbox.new_output_file(&format!("{stem}.{fmt}"))?;
            std::fs::write(&p, bytes)?;
            materials.push(sandbox.display_path(&p));
        }
        files_written.extend(materials.clone());
    }

    // ---- local fill
    let mut local_fill = None;
    if let Some(file) = args.details_file.as_deref().filter(|f| !f.trim().is_empty()) {
        let fmt = args.format.as_deref().unwrap_or("docx").trim().to_ascii_lowercase();
        let o = localfill::fill_local(
            ctx,
            FillSpec {
                template_id: "health_care_proxy",
                english: proxy_document(&c.proxy_template.title, &sections),
                file,
                sheet: args.sheet.as_deref(),
                format: &fmt,
                output_stem: "health_care_proxy",
                translatable: true,
            },
        )?;
        files_written.extend(o.output_files.clone());
        identifiers = o.identifiers_detected.clone();
        local_fill = Some(o);
    }

    // ---- markdown
    let mut s = String::from("**Advance care planning in Massachusetts**\n\n**Health care proxy (M.G.L. c.201D)**\n");
    for r in &c.proxy_rules {
        s.push_str(&format!("- {}\n", r.text));
    }
    s.push_str(&format!(
        "\n**Proxy template** ({} sections; fields for a details file: {}).\n",
        sections.len(),
        columns.join(", ")
    ));
    s.push_str(&format!("\n**MOLST:** {} **Warning:** {}\n", c.molst.text.split_whitespace().collect::<Vec<_>>().join(" "), c.molst.warning));
    s.push_str(&format!("\n**Conversation guides:** {} ({})\n", c.conversation_project.url, c.conversation_project.text.split_whitespace().collect::<Vec<_>>().join(" ")));
    s.push_str(&format!("\n**Community session ({minutes} minutes)**\n"));
    for seg in &script {
        s.push_str(&format!("- {} ({} min): {}\n", seg.title, seg.minutes, seg.script));
    }
    if !materials.is_empty() {
        s.push_str(&format!("\nSession materials written: {}.\n", materials.join(", ")));
    }
    if let Some(o) = &local_fill {
        s.push_str(&localfill::outcome_markdown(o));
    }
    s.push_str("\nTranslations: put agency-approved translations of the proxy template in `/data/templates/<language>/health_care_proxy.md` (same {{fields}}); rows whose Language has none use English, and the result says how many.\n");

    let mut sources = vec![c10::content_source(ACP)?];
    sources.extend(c10::sources_for(
        &c.sources,
        c.proxy_rules
            .iter()
            .map(|r| r.source.as_str())
            .chain([c.proxy_template.source.as_str(), "conversation-project", "mgl-201d-1"]),
    ));
    if let Some(o) = &local_fill {
        sources.push(phn_core::Source::local_file(&o.file));
    }
    let mut caveats = vec![
        "Legal decision support, not legal advice. The proxy template is drafted from the statute's requirements; use your agency's or health system's approved form if it has one, and refer complex questions to an attorney.".to_string(),
        format!(
            "MOLST is {} ({} confidence): {}",
            c.molst.status.replace('_', " "),
            c.molst.confidence.label(),
            c.molst.warning
        ),
        format!(
            "The facilitator script ({} confidence) and proxy template ({} confidence, {}) need the nurse's review.",
            c.facilitator_script.confidence.label(),
            c.proxy_template.confidence.label(),
            c10::citation(&c.sources, &c.proxy_template.source, &c.proxy_template.section)
        ),
    ];
    caveats.extend(c10::review_caveats(&[ACP]));
    Ok(Envelope::new(
        s,
        AcpResult {
            proxy_rules: cite_all(&c.sources, &c.proxy_rules),
            proxy_template_title: c.proxy_template.title.clone(),
            proxy_template: sections,
            details_file_columns: columns,
            after_signing: c.after_signing.clone(),
            molst: MolstOut {
                status: c.molst.status.clone(),
                confidence: c.molst.confidence,
                text: c.molst.text.split_whitespace().collect::<Vec<_>>().join(" "),
                warning: c.molst.warning.clone(),
            },
            conversation_project_url: c.conversation_project.url.clone(),
            conversation_project: c.conversation_project.text.split_whitespace().collect::<Vec<_>>().join(" "),
            facilitator_script: script,
            session_minutes: minutes,
            handout_points: c.handout_points.clone(),
            translation_templates: "/data/templates/<language>/health_care_proxy.md".into(),
            materials_files: materials,
            local_fill,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        identifiers_detected: identifiers,
        files_written,
        notes: vec!["Names and other details stay in your files; filled forms are written to the output folder and never returned.".into()],
        ..Privacy::none()
    })
    .decision_support())
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
    fn content_sources_are_known_and_script_scales() {
        let c = &*CONTENT;
        let keys: Vec<&str> = c.proxy_rules.iter().map(|r| r.source.as_str()).collect();
        c10::assert_keys_known(&c.sources, &keys, "acp");
        for m in [30, 60, 90] {
            let s = scaled(&c.facilitator_script.segments, m);
            assert_eq!(s.iter().map(|x| x.minutes).sum::<u32>(), m);
        }
        assert!(!c.molst.text.contains("https://www.molst-ma.org"));
    }

    #[test]
    fn fills_proxies_with_agency_translation_and_english_fallback() {
        let (d, ctx) = ctx();
        std::fs::create_dir_all(d.path().join("data/templates/spanish")).unwrap();
        std::fs::write(
            d.path().join("data/templates/spanish/health_care_proxy.md"),
            "# Poder para atención médica\n\nYo, {{Principal Name}}, nombro a {{Agent Name}}.\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("data/proxies.csv"),
            "Principal Name,Agent Name,Language\nOdalys Fenwick-Trask,Ingo Vashti,Spanish\nPorter Quillfeather,Mabel Ostrander,Khmer\n",
        )
        .unwrap();
        let r = advance_care_planning_toolkit(
            &ctx,
            AcpArgs { details_file: Some("proxies.csv".into()), materials_format: Some("docx".into()), ..Default::default() },
        )
        .unwrap();
        let json = serde_json::to_string(&r).unwrap();
        for name in ["Fenwick", "Vashti", "Quillfeather", "Ostrander"] {
            assert!(!json.contains(name), "{name} leaked");
        }
        let o = r.data.local_fill.unwrap();
        assert_eq!(o.documents_written, 2);
        let spanish = o.languages.iter().find(|l| l.language == "Spanish").unwrap();
        assert_eq!(spanish.template_used, "agency translation");
        let khmer = o.languages.iter().find(|l| l.language == "Khmer").unwrap();
        assert!(khmer.template_used.starts_with("English"));
        assert_eq!(r.data.materials_files.len(), 3);
        assert_eq!(r.privacy.files_written.len(), 5);
        assert!(advance_care_planning_toolkit(&ctx, AcpArgs { session_minutes: Some(5), ..Default::default() }).is_err());
    }
}
