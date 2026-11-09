//! MH-06 `plan_school_postvention`: steps for a school after a suicide (from
//! AFSP/SPRC *After a Suicide: A Toolkit for Schools*), talking points,
//! message templates for staff, students, parents, the media, and social
//! media (chosen by whether the cause of death is confirmed), and a
//! safe-messaging check of a draft.
//!
//! Template + local fill: templates carry `{{placeholders}}`; when the nurse
//! points to a local file with the details, the filled documents are written
//! to /data/output and only file names and counts come back. The tool never
//! accepts names as arguments and never generates method details.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Document, from_markdown};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::youth_survey::crisis_line;
use crate::comms::content::{self, NamedSource, POSTVENTION, SAFE_MESSAGING};
use crate::comms::safe_messaging::{self, SafeMessagingReport};
use crate::context::Context;

#[derive(Debug, Clone, Deserialize)]
struct Step {
    phase: String,
    text: String,
    page: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct TalkingPoint {
    text: String,
    page: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Template {
    id: String,
    title: String,
    cause_status: String,
    toolkit_page: u32,
    markdown: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PostventionContent {
    meta: ContentMeta,
    sources: std::collections::BTreeMap<String, NamedSource>,
    license: String,
    steps: Vec<Step>,
    talking_points: Vec<TalkingPoint>,
    templates: Vec<Template>,
}

static CONTENT: LazyLock<PostventionContent> = LazyLock::new(|| {
    content::load(POSTVENTION).expect("content/bh/postvention.yaml matches its schema")
});

const MESSAGE_TYPES: &[&str] = &[
    "staff_meeting",
    "student_notification",
    "parent_letter",
    "media_statement",
    "social_media_post",
];

/// Arguments for `plan_school_postvention`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PostventionArgs {
    /// What is known about the cause of death: "unconfirmed" (default), "ruled_suicide", or
    /// "family_declined_disclosure" (the family asked that the cause not be shared).
    pub cause_status: Option<String>,
    /// Steps to show: "first_24_hours", "first_week", "ongoing", or "all" (default).
    pub phase: Option<String>,
    /// Message templates to return: any of "staff_meeting", "student_notification",
    /// "parent_letter", "media_statement", "social_media_post" (default: staff meeting, student
    /// notification, and parent letter). Never put names in arguments.
    pub messages: Vec<String>,
    /// A draft message to check against safe-messaging rules.
    pub draft_text: Option<String>,
    /// A draft in /data (DOCX, TXT, Markdown, HTML, PDF) to check instead of `draft_text`.
    pub draft_path: Option<String>,
    /// A CSV/XLSX in /data whose column names match the template placeholders (School Name,
    /// Student Name, Principal Name, Date of Notice, Counseling Location, ...). Each row fills
    /// the templates as Word documents in /data/output; the details are never returned.
    pub details_path: Option<String>,
}

/// A step in the postvention plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PostventionStep {
    /// "first_24_hours", "first_week", or "ongoing".
    pub phase: String,
    pub text: String,
    /// Page in After a Suicide: A Toolkit for Schools (2nd ed.).
    pub toolkit_page: u32,
}

/// A message template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MessageTemplate {
    pub id: String,
    pub title: String,
    pub cause_status: String,
    /// Markdown with {{placeholders}}.
    pub markdown: String,
    pub placeholders: Vec<String>,
    /// Toolkit page of the corresponding sample (use the toolkit's own text from the PDF with
    /// its credit line, or this draft).
    pub toolkit_page: u32,
    /// Safe-messaging problems in the template itself (should be none).
    pub safe_messaging_problems: usize,
}

/// Result of `plan_school_postvention`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PostventionResult {
    pub cause_status: String,
    pub steps: Vec<PostventionStep>,
    pub talking_points: Vec<PostventionStep>,
    pub templates: Vec<MessageTemplate>,
    /// Safe-messaging check of the draft, when one was given.
    pub draft_check: Option<SafeMessagingReport>,
    /// Filled documents written to /data/output (names only).
    pub files_written: Vec<String>,
    pub records_filled: usize,
    /// Placeholders left empty because the file had no value.
    pub empty_values: usize,
    pub crisis_line: String,
    pub toolkit_license: String,
}

fn placeholders(md: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").expect("valid regex");
    let set: BTreeSet<String> = re.captures_iter(md).map(|c| c[1].to_string()).collect();
    set.into_iter().collect()
}

fn read_draft(ctx: &Context, path: &str) -> Result<(String, String)> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let label = sandbox.display_path(&p);
    let e = phn_local::extract::extract(&p)?;
    if e.kind.is_tabular() {
        return Err(PhnError::Unsupported(
            "draft_path should be a document (DOCX, TXT, Markdown, HTML, or PDF)".into(),
        ));
    }
    Ok((e.text, label))
}

/// Plan a school's response after a suicide and check messages.
pub fn plan_school_postvention(
    ctx: &Context,
    args: PostventionArgs,
) -> Result<Envelope<PostventionResult>> {
    let c = &*CONTENT;
    let cause = match args
        .cause_status
        .as_deref()
        .map(|s| s.trim().to_lowercase().replace([' ', '-'], "_"))
    {
        None => "unconfirmed".to_string(),
        Some(s) if s.is_empty() => "unconfirmed".to_string(),
        Some(s) => match s.as_str() {
            "unconfirmed" | "unknown" | "under_investigation" | "pending" => "unconfirmed".into(),
            "ruled_suicide" | "confirmed" | "suicide" | "confirmed_suicide" => {
                "ruled_suicide".into()
            }
            "family_declined_disclosure" | "family_declined" | "not_disclosed" | "private" => {
                "family_declined_disclosure".into()
            }
            other => {
                return Err(PhnError::InvalidInput(format!(
                    "cause_status '{other}' is not recognized; use unconfirmed, ruled_suicide, or family_declined_disclosure"
                )));
            }
        },
    };
    let phase = match args
        .phase
        .as_deref()
        .map(|s| s.trim().to_lowercase().replace([' ', '-'], "_"))
    {
        None => "all".to_string(),
        Some(s) if s.is_empty() || s == "all" => "all".into(),
        Some(s) if ["first_24_hours", "first_week", "ongoing"].contains(&s.as_str()) => s,
        Some(other) => {
            return Err(PhnError::InvalidInput(format!(
                "phase '{other}' is not recognized; use first_24_hours, first_week, ongoing, or all"
            )));
        }
    };
    let wanted: Vec<String> = if args.messages.is_empty() {
        vec![
            "staff_meeting".into(),
            "student_notification".into(),
            "parent_letter".into(),
        ]
    } else {
        let mut v = Vec::new();
        for m in &args.messages {
            let k = m.trim().to_lowercase().replace([' ', '-'], "_");
            if !MESSAGE_TYPES.contains(&k.as_str()) {
                return Err(PhnError::InvalidInput(format!(
                    "message '{m}' is not recognized; use {}",
                    MESSAGE_TYPES.join(", ")
                )));
            }
            if !v.contains(&k) {
                v.push(k);
            }
        }
        v
    };

    let steps: Vec<PostventionStep> = c
        .steps
        .iter()
        .filter(|s| phase == "all" || s.phase == phase)
        .map(|s| PostventionStep {
            phase: s.phase.clone(),
            text: s.text.clone(),
            toolkit_page: s.page,
        })
        .collect();
    let talking_points: Vec<PostventionStep> = c
        .talking_points
        .iter()
        .map(|t| PostventionStep {
            phase: "talking_point".into(),
            text: t.text.clone(),
            toolkit_page: t.page,
        })
        .collect();
    let chosen: Vec<&Template> = wanted
        .iter()
        .filter_map(|w| {
            c.templates
                .iter()
                .find(|t| &t.id == w && t.cause_status == cause)
                .or_else(|| {
                    c.templates
                        .iter()
                        .find(|t| &t.id == w && t.cause_status == "any")
                })
        })
        .collect();
    let templates: Vec<MessageTemplate> = chosen
        .iter()
        .map(|t| MessageTemplate {
            id: t.id.clone(),
            title: t.title.clone(),
            cause_status: t.cause_status.clone(),
            markdown: t.markdown.clone(),
            placeholders: placeholders(&t.markdown),
            toolkit_page: t.toolkit_page,
            safe_messaging_problems: safe_messaging::check(&t.markdown)
                .findings
                .iter()
                .filter(|f| f.severity != "missing")
                .count(),
        })
        .collect();

    // ---- draft check
    let mut privacy = Privacy::none();
    let mut sources_extra = Vec::new();
    let draft = match (
        args.draft_text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty()),
        args.draft_path
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty()),
    ) {
        (Some(_), Some(_)) => {
            return Err(PhnError::InvalidInput(
                "give draft_text or draft_path, not both".into(),
            ));
        }
        (Some(t), None) => Some(t.to_string()),
        (None, Some(p)) => {
            let (t, label) = read_draft(ctx, p)?;
            sources_extra.push(phn_core::Source::local_file(&label));
            Some(t)
        }
        (None, None) => None,
    };
    let draft_check = draft.as_deref().map(safe_messaging::check);

    // ---- local fill
    let mut files_written = Vec::new();
    let mut records = 0usize;
    let mut empty = 0usize;
    if let Some(path) = args
        .details_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (table, label) = crate::core::files::load_table(ctx, path, None)?;
        if table.rows.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "'{label}' has no data rows"
            )));
        }
        if table.rows.len() > 20 {
            return Err(PhnError::InvalidInput(format!(
                "'{label}' has {} rows; give one row per school notice (at most 20)",
                table.rows.len()
            )));
        }
        let sandbox = ctx.sandbox()?;
        for t in &chosen {
            let doc = Document {
                title: t.title.clone(),
                subtitle: None,
                language: "en-US".into(),
                author: None,
                letterhead: ctx.settings().letterhead,
                logo: None,
                blocks: from_markdown(&t.markdown),
            };
            let binding = phn_docs::mailmerge::bind(&doc, &table)?;
            for (i, row) in table.rows.iter().enumerate() {
                let (filled, n_empty) = phn_docs::mailmerge::fill(&doc, &binding, row);
                empty += n_empty;
                let bytes = phn_docs::docx::to_docx(&filled)?;
                let out =
                    sandbox.new_output_file(&format!("postvention_{}_{}.docx", t.id, i + 1))?;
                std::fs::write(&out, bytes)?;
                files_written.push(sandbox.display_path(&out));
            }
        }
        records = table.rows.len();
        sources_extra.push(phn_core::Source::local_file(&label));
        privacy.files_written = files_written.clone();
        privacy.notes.push(format!(
            "Details from {label} were merged into {} document(s) in the output folder; no names or details are included in this response.",
            files_written.len()
        ));
    }

    // ---- summary
    let crisis = crisis_line();
    let phase_label = |p: &str| match p {
        "first_24_hours" => "First 24 hours",
        "first_week" => "First week",
        _ => "Ongoing",
    };
    let mut summary = format!(
        "**School postvention plan** (AFSP/SPRC *After a Suicide: A Toolkit for Schools*, 2nd ed.). Cause of death: {}.\n\n",
        match cause.as_str() {
            "ruled_suicide" => "ruled a suicide, and the family agrees it can be shared",
            "family_declined_disclosure" => "the family asked that it not be shared",
            _ => "not yet confirmed",
        }
    );
    let mut last = "";
    for s in &steps {
        if s.phase != last {
            summary.push_str(&format!("\n**{}**\n", phase_label(&s.phase)));
            last = &s.phase;
        }
        summary.push_str(&format!("- {} (p. {})\n", s.text, s.toolkit_page));
    }
    summary.push_str("\n**Talking with students**\n");
    for t in &talking_points {
        summary.push_str(&format!("- {} (p. {})\n", t.text, t.toolkit_page));
    }
    summary.push_str("\n**Message templates** (drafts that follow the toolkit's structure; placeholders in {{ }} are filled locally):\n");
    for t in &templates {
        summary.push_str(&format!(
            "- {} ({}): placeholders {}\n",
            t.title,
            t.id,
            t.placeholders
                .iter()
                .map(|p| format!("{{{{{p}}}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(d) = &draft_check {
        summary.push_str(&format!(
            "\n**Safe-messaging check of the draft:** {} problem(s), {} must be fixed.\n",
            d.findings.len(),
            d.must_fix
        ));
        for f in &d.findings {
            summary.push_str(&format!(
                "- **{}** {}{} Fix: {}\n",
                match f.severity.as_str() {
                    "must_fix" => "MUST FIX",
                    "missing" => "MISSING",
                    _ => "SHOULD FIX",
                },
                f.message,
                if f.sentences.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (sentence{} {})",
                        if f.sentences.len() > 1 { "s" } else { "" },
                        f.sentences
                            .iter()
                            .map(|n| n.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
                f.fix
            ));
        }
        summary.push_str("Before using it, also confirm: ");
        summary.push_str(&d.checklist.join(" "));
        summary.push('\n');
    }
    if !files_written.is_empty() {
        summary.push_str(&format!(
            "\nFilled {} document(s) for {records} record(s) in the output folder: {}.{}\n",
            files_written.len(),
            files_written.join(", "),
            if empty > 0 {
                format!(" {empty} placeholder value(s) were empty; complete them before sending.")
            } else {
                String::new()
            }
        ));
    }
    summary.push_str(&format!("\n_{crisis}_\n"));

    let mut sources = content::cite(&c.sources, &["toolkit", "ma_988"], &c.meta.prepared);
    sources.extend(sources_extra);
    let mut caveats = vec![
        "The templates are drafts written for this server following the toolkit's structure; the toolkit's own sample statements can be used directly from the PDF with its copyright notice and the sprc.org and afsp.org addresses. Adapting or excerpting the toolkit needs permission from EDC (permissions@edc.org).".to_string(),
        "The school's crisis team, the family's wishes, and district policy decide what is shared. Confirm the cause of death before calling it a suicide.".to_string(),
        "The toolkit (2018) lists the former Lifeline number; the templates use 988.".to_string(),
        "The safe-messaging check uses word patterns and cannot judge tone; a person must review every message.".to_string(),
    ];
    caveats.extend(content::review_caveats(&[POSTVENTION, SAFE_MESSAGING]));
    Ok(Envelope::new(
        summary,
        PostventionResult {
            cause_status: cause,
            steps,
            talking_points,
            templates,
            draft_check,
            files_written,
            records_filled: records,
            empty_values: empty,
            crisis_line: crisis,
            toolkit_license: c.license.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy)
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_passes_the_safe_messaging_rules() {
        for t in &CONTENT.templates {
            let r = safe_messaging::check(&t.markdown);
            let bad: Vec<&str> = r
                .findings
                .iter()
                .filter(|f| f.severity != "missing")
                .map(|f| f.rule.as_str())
                .collect();
            assert!(bad.is_empty(), "{} ({}): {bad:?}", t.id, t.cause_status);
            assert!(t.markdown.contains("988"), "{} lacks 988", t.id);
        }
        // Each cause status has student and parent templates.
        for cs in ["ruled_suicide", "unconfirmed", "family_declined_disclosure"] {
            for id in ["student_notification", "parent_letter"] {
                assert!(
                    CONTENT
                        .templates
                        .iter()
                        .any(|t| t.id == id && t.cause_status == cs),
                    "{id} {cs}"
                );
            }
        }
    }
}
