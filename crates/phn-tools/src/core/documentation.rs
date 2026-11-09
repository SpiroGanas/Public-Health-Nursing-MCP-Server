//! CORE-03 `nursing_documentation_template`: SOAP, DAR, and narrative
//! templates for home visits, clinics, case investigations, and DOT visits,
//! with the required elements (date and time, signature and credentials,
//! consent, language, assessment, interventions, teaching, response,
//! follow-up, ...) and a check of a local note that never returns its text.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::core::practice::{self, CitedNote, CitedText, Confidence, ContentSource};
use crate::md;

/// Content id.
pub const DOC_CONTENT: &str = "nursing-documentation-templates";

/// Documentation format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocFormat {
    /// Subjective, Objective, Assessment, Plan.
    #[default]
    Soap,
    /// Focus charting: Data, Action, Response.
    Dar,
    /// Narrative note.
    Narrative,
}

/// Visit type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisitType {
    #[default]
    HomeVisit,
    Clinic,
    CaseInvestigation,
    /// Directly observed therapy visit (TB/LTBI).
    DotVisit,
}

impl VisitType {
    fn key(self) -> &'static str {
        match self {
            VisitType::HomeVisit => "home_visit",
            VisitType::Clinic => "clinic",
            VisitType::CaseInvestigation => "case_investigation",
            VisitType::DotVisit => "dot_visit",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Section {
    id: String,
    heading: String,
    aliases: Vec<String>,
    prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Format {
    id: String,
    label: String,
    basis: String,
    confidence: Confidence,
    sections: Vec<Section>,
}

#[derive(Debug, Clone, Deserialize)]
struct VisitInfo {
    id: String,
    label: String,
    prompts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Element {
    id: String,
    label: String,
    applies_to: Vec<String>,
    detector: String,
    cues: Vec<String>,
    source: String,
    section: String,
    #[serde(default)]
    confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Deserialize)]
struct DocContent {
    meta: ContentMeta,
    sources: BTreeMap<String, ContentSource>,
    standards: Vec<CitedText>,
    formats: Vec<Format>,
    visit_types: Vec<VisitInfo>,
    elements: Vec<Element>,
}

static CONTENT: LazyLock<DocContent> = LazyLock::new(|| {
    practice::load(DOC_CONTENT).expect("documentation content matches the Rust types")
});

fn content() -> &'static DocContent {
    &CONTENT
}

/// Arguments for `nursing_documentation_template`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DocTemplateArgs {
    /// soap (default), dar, or narrative.
    pub format: Option<DocFormat>,
    /// home_visit (default), clinic, case_investigation, or dot_visit.
    pub visit_type: Option<VisitType>,
    /// Write the blank template (DOCX and Markdown) to /data/output.
    pub write_template: bool,
    /// A note file in /data (TXT, MD, DOCX, PDF) to check for the required
    /// elements. The note's text is never returned.
    pub check_path: Option<String>,
    /// Element ids to leave out of the check (for example "consent" when your
    /// agency documents consent elsewhere).
    pub skip_elements: Vec<String>,
}

/// A template section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateSection {
    pub heading: String,
    pub prompt: String,
}

/// A required element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RequiredElement {
    pub id: String,
    pub label: String,
    pub citation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// Result of checking one element in a note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElementCheck {
    pub id: String,
    pub label: String,
    pub present: bool,
    /// Template section where it was first found, if the note has sections.
    pub found_in_section: Option<String>,
}

/// Note check summary (no note text).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NoteCheck {
    pub file: String,
    pub words: usize,
    /// Format whose section headings were found (or "narrative").
    pub detected_format: String,
    pub sections_found: Vec<String>,
    pub elements: Vec<ElementCheck>,
    pub missing: Vec<String>,
    /// Identifier kinds the PHI detector found, with counts (values not shown).
    pub identifiers_detected: BTreeMap<String, usize>,
}

/// Result of `nursing_documentation_template`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocTemplateResult {
    pub format: DocFormat,
    pub format_label: String,
    /// Confidence in the format structure (a tool default, not a cited standard).
    pub structure_confidence: Confidence,
    pub visit_type: VisitType,
    pub visit_label: String,
    pub sections: Vec<TemplateSection>,
    pub visit_prompts: Vec<String>,
    pub required_elements: Vec<RequiredElement>,
    pub standards: Vec<CitedNote>,
    /// Markdown template, ready to paste into an EHR or document.
    pub template_markdown: String,
    pub check: Option<NoteCheck>,
    pub files: Vec<String>,
}

fn has_date(text: &str) -> bool {
    const MONTHS: &[&str] = &[
        "jan",
        "feb",
        "mar",
        "apr",
        "may",
        "jun",
        "jul",
        "aug",
        "sep",
        "sept",
        "oct",
        "nov",
        "dec",
        "january",
        "february",
        "march",
        "april",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let tokens: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .map(|t| t.trim_matches('.').to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    for (i, t) in tokens.iter().enumerate() {
        let parts: Vec<&str> = t.split(['/', '-']).collect();
        if parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return true;
        }
        if MONTHS.contains(&t.as_str())
            && tokens
                .get(i + 1)
                .is_some_and(|n| n.chars().take_while(char::is_ascii_digit).count() >= 1)
        {
            return true;
        }
    }
    false
}

fn has_time(text: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .any(|t| {
            let t = t.trim_matches('.');
            match t.split_once(':') {
                Some((h, m)) => {
                    let m: String = m.chars().take(2).collect();
                    !h.is_empty()
                        && h.len() <= 2
                        && h.chars().all(|c| c.is_ascii_digit())
                        && m.len() == 2
                        && m.chars().all(|c| c.is_ascii_digit())
                }
                None => false,
            }
        })
}

fn has_credential(text: &str) -> bool {
    const CREDS: &[&str] = &[
        "RN", "PHN", "BSN", "MSN", "DNP", "APRN", "NP", "LPN", "LVN", "CNS", "CPNP", "FNP",
    ];
    text.split(|c: char| {
        c.is_whitespace() || c == ',' || c == ';' || c == '/' || c == '(' || c == ')'
    })
    .map(|t| t.replace('.', ""))
    .any(|t| CREDS.contains(&t.as_str()))
}

/// Find section headings in the note: a line that is an alias (optionally
/// followed by ':' or '-' and text).
fn sections_in(text: &str, f: &Format) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    let mut offset = 0usize;
    for line in text.lines() {
        let l = line.trim().to_lowercase();
        let head: String = l
            .split([':', '-', '–', '—'])
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(|c: char| c == '*' || c == '#')
            .trim()
            .to_string();
        let has_sep = l.contains(':') || l.contains(" - ") || l.len() == head.len();
        if has_sep {
            for s in &f.sections {
                if s.aliases.iter().any(|a| a == &head)
                    || phn_core::text::normalize(&s.heading) == phn_core::text::normalize(&head)
                {
                    found.push((s.id.clone(), offset));
                    break;
                }
            }
        }
        offset += line.len() + 1;
    }
    found
}

/// Check a note's text against the elements for a visit type.
fn check_text(
    text: &str,
    visit: VisitType,
    skip: &[String],
) -> (String, Vec<String>, Vec<ElementCheck>) {
    let c = content();
    // Detect the format with the most section headings.
    let mut best: (usize, &Format, Vec<(String, usize)>) = (0, &c.formats[2], Vec::new());
    for f in &c.formats {
        let s = sections_in(text, f);
        let distinct = {
            let mut v: Vec<&String> = s.iter().map(|x| &x.0).collect();
            v.sort();
            v.dedup();
            v.len()
        };
        if distinct >= 2 && distinct > best.0 {
            best = (distinct, f, s);
        }
    }
    let (_, fmt, secs) = best;
    let section_at = |pos: usize| -> Option<String> {
        secs.iter()
            .rfind(|(_, o)| *o <= pos)
            .and_then(|(id, _)| fmt.sections.iter().find(|s| &s.id == id))
            .map(|s| s.heading.clone())
    };
    let lower = text.to_lowercase();
    let padded = practice::padded_words(text);
    let mut checks = Vec::new();
    for e in c
        .elements
        .iter()
        .filter(|e| e.applies_to.iter().any(|a| a == visit.key()))
        .filter(|e| !skip.contains(&e.id))
    {
        let (present, pos) = match e.detector.as_str() {
            "date_time" => (has_date(text) && has_time(text), None),
            "credential" => {
                let p = has_credential(text)
                    || e.cues
                        .iter()
                        .any(|q| padded.contains(&practice::padded_words(q)));
                (p, None)
            }
            _ => {
                let mut pos = None;
                for q in &e.cues {
                    let needle = practice::padded_words(q);
                    let needle = needle.trim();
                    if needle.is_empty() {
                        continue;
                    }
                    if padded.contains(&format!(" {needle} ")) {
                        pos = lower.find(needle).or(Some(0));
                        break;
                    }
                }
                (pos.is_some(), pos)
            }
        };
        checks.push(ElementCheck {
            id: e.id.clone(),
            label: e.label.clone(),
            present,
            found_in_section: if present {
                pos.and_then(section_at)
            } else {
                None
            },
        });
    }
    let mut found: Vec<String> = secs
        .iter()
        .filter_map(|(id, _)| fmt.sections.iter().find(|s| &s.id == id))
        .map(|s| s.heading.clone())
        .collect();
    found.dedup();
    let detected = if secs.is_empty() {
        "narrative (no section headings found)".to_string()
    } else {
        fmt.label.clone()
    };
    (detected, found, checks)
}

fn template_markdown(f: &Format, v: &VisitInfo, elements: &[RequiredElement]) -> String {
    let mut s = format!(
        "# {} note — {}\n\nDate: ________  Time: ______  Client: (local record)  Language/interpreter: \
         ________________\n\n",
        f.label, v.label
    );
    for sec in &f.sections {
        s.push_str(&format!("## {}\n\n_{}_\n\n\n", sec.heading, sec.prompt));
    }
    s.push_str("## Visit-specific prompts\n\n");
    for p in &v.prompts {
        s.push_str(&format!("- [ ] {p}\n"));
    }
    s.push_str("\n## Required elements checklist\n\n");
    for e in elements {
        s.push_str(&format!("- [ ] {}\n", e.label));
    }
    s.push_str("\nSignature (name as licensed, credentials): ______________________\n");
    s
}

/// Documentation templates and a required-elements check.
pub fn nursing_documentation_template(
    ctx: &Context,
    args: DocTemplateArgs,
) -> Result<Envelope<DocTemplateResult>> {
    let c = content();
    let format = args.format.unwrap_or_default();
    let visit = args.visit_type.unwrap_or_default();
    let fkey = match format {
        DocFormat::Soap => "soap",
        DocFormat::Dar => "dar",
        DocFormat::Narrative => "narrative",
    };
    let f = c
        .formats
        .iter()
        .find(|x| x.id == fkey)
        .ok_or_else(|| PhnError::Internal(format!("format {fkey}")))?;
    let v = c
        .visit_types
        .iter()
        .find(|x| x.id == visit.key())
        .ok_or_else(|| PhnError::Internal(format!("visit type {}", visit.key())))?;
    for s in &args.skip_elements {
        if !c.elements.iter().any(|e| &e.id == s) {
            return Err(PhnError::InvalidInput(format!(
                "'{s}' is not an element id; ids: {}",
                c.elements
                    .iter()
                    .map(|e| e.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    let required: Vec<RequiredElement> = c
        .elements
        .iter()
        .filter(|e| e.applies_to.iter().any(|a| a == visit.key()))
        .filter(|e| !args.skip_elements.contains(&e.id))
        .map(|e| RequiredElement {
            id: e.id.clone(),
            label: e.label.clone(),
            citation: practice::cite(&c.sources, &e.source, &e.section),
            confidence: e.confidence,
        })
        .collect();
    let md_template = template_markdown(f, v, &required);
    let mut caveats: Vec<String> = practice::review_caveat(&c.meta).into_iter().collect();
    caveats.push(format!(
        "The {} structure and prompts are tool defaults ({}); follow your agency's \
         documentation policy and EHR where they differ.",
        f.label, f.basis
    ));
    caveats.push(
        "42 CFR 484 (Medicare home health conditions of participation) is cited as a reference \
         standard; it binds only Medicare-certified home health agencies. 244 CMR 9.03 applies to \
         every nurse licensed in Massachusetts."
            .into(),
    );
    let mut sources: Vec<Source> = Vec::new();

    // ---- Check a local note
    let mut check = None;
    if let Some(p) = args.check_path.as_deref().filter(|p| !p.trim().is_empty()) {
        let (text, label) = practice::read_local_note(ctx, p)?;
        let (detected, found, elements) = check_text(&text, visit, &args.skip_elements);
        let mut ids: BTreeMap<String, usize> = BTreeMap::new();
        for finding in phn_local::phi::scan_text(&text) {
            *ids.entry(finding.kind.label().to_string()).or_default() += 1;
        }
        let missing: Vec<String> = elements
            .iter()
            .filter(|e| !e.present)
            .map(|e| e.label.clone())
            .collect();
        sources.push(Source::local_file(&label));
        check = Some(NoteCheck {
            file: label,
            words: text.split_whitespace().count(),
            detected_format: detected,
            sections_found: found,
            elements,
            missing,
            identifiers_detected: ids,
        });
        caveats.push(
            "The element check looks for words and patterns, not meaning: a 'present' element \
             may still be incomplete, and a 'missing' one may be worded differently. Read the \
             note against the checklist."
                .into(),
        );
    }

    // ---- Files
    let mut files = Vec::new();
    if args.write_template {
        let mut blocks = vec![practice::para(
            "Date: ________  Time: ______  Client: (local record)  Language/interpreter: ________",
        )];
        for sec in &f.sections {
            blocks.push(practice::heading(2, sec.heading.clone()));
            blocks.push(practice::para(sec.prompt.clone()));
            blocks.push(practice::para(" "));
        }
        blocks.push(practice::heading(2, "Visit-specific prompts"));
        blocks.push(practice::bullets(
            v.prompts.iter().map(|p| format!("☐ {p}")),
        ));
        blocks.push(practice::heading(2, "Required elements"));
        blocks.push(practice::bullets(
            required.iter().map(|e| format!("☐ {}", e.label)),
        ));
        blocks.push(practice::para(
            "Signature (name as licensed, credentials): ______________________",
        ));
        let doc = practice::document(
            ctx,
            &format!("{} note template — {}", f.label, v.label),
            blocks,
        );
        files = practice::write_document(
            ctx,
            &doc,
            &format!("documentation_template_{fkey}_{}", visit.key()),
            &["docx", "md"],
        )?;
    }

    // ---- Markdown
    let mut s = format!("**{} template — {}**\n\n", f.label, v.label);
    if let Some(ch) = &check {
        s.push_str(&format!(
            "**Check of `{}`** ({} words; format: {}). The note's text is not shown.\n\n",
            ch.file, ch.words, ch.detected_format
        ));
        let rows: Vec<Vec<String>> = ch
            .elements
            .iter()
            .map(|e| {
                vec![
                    e.label.clone(),
                    if e.present { "found" } else { "**missing**" }.into(),
                    e.found_in_section.clone().unwrap_or_default(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Required element", "Result", "Section"],
            &rows,
        ));
        if !ch.identifiers_detected.is_empty() {
            s.push_str(&format!(
                "\nThe note contains identifiers ({}); keep it in your secure record and do not \
                 paste it into the chat.\n",
                ch.identifiers_detected
                    .iter()
                    .map(|(k, n)| format!("{k}: {n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        s.push('\n');
    }
    s.push_str("```markdown\n");
    s.push_str(&md_template);
    s.push_str("```\n");
    if !files.is_empty() {
        s.push_str(&format!("\n**Files:** {}\n", files.join(", ")));
    }
    let mut keys: Vec<&str> = vec!["cmr244"];
    keys.extend(c.elements.iter().map(|e| e.source.as_str()));
    sources.extend(practice::sources_for(&c.sources, keys));
    let mut privacy = Privacy {
        files_written: files.clone(),
        notes: vec!["Templates are blank; client details belong in the local record.".into()],
        ..Privacy::none()
    };
    if let Some(ch) = &check {
        privacy.identifiers_detected = ch.identifiers_detected.keys().cloned().collect();
        privacy.notes.push(
            "The note was read locally; only element results and counts are returned.".into(),
        );
    }
    let data = DocTemplateResult {
        format,
        format_label: f.label.clone(),
        structure_confidence: f.confidence,
        visit_type: visit,
        visit_label: v.label.clone(),
        sections: f
            .sections
            .iter()
            .map(|x| TemplateSection {
                heading: x.heading.clone(),
                prompt: x.prompt.clone(),
            })
            .collect(),
        visit_prompts: v.prompts.clone(),
        required_elements: required,
        standards: practice::notes(&c.standards, &c.sources),
        template_markdown: md_template,
        check,
        files,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
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
    fn templates_for_each_format_and_visit_type() {
        let (_d, c) = ctx(false);
        for (f, n) in [
            (DocFormat::Soap, 4),
            (DocFormat::Dar, 4),
            (DocFormat::Narrative, 4),
        ] {
            let env = nursing_documentation_template(
                &c,
                DocTemplateArgs {
                    format: Some(f),
                    visit_type: Some(VisitType::CaseInvestigation),
                    ..DocTemplateArgs::default()
                },
            )
            .unwrap();
            assert_eq!(env.data.sections.len(), n);
            assert!(
                env.data
                    .required_elements
                    .iter()
                    .any(|e| e.id == "control_measures")
            );
            assert!(env.data.template_markdown.contains("Signature"));
        }
    }

    #[test]
    fn checks_a_note_without_returning_it() {
        let (dir, c) = ctx(true);
        std::fs::write(
            dir.path().join("data/visit.txt"),
            "Home visit 11/05/2026 10:30\nS: Client reports missed two doses; speaks Spanish, \
             interpreter used (Language Line). Verbal consent obtained.\nO: BP 128/82. Pill box \
             empty.\nA: Medication regimen, inconsistent adherence.\nP: Taught dosing schedule; \
             client verbalized understanding with teach-back. Next visit 11/12. \
             Tilda Quackenbush, RN\n",
        )
        .unwrap();
        let env = nursing_documentation_template(
            &c,
            DocTemplateArgs {
                check_path: Some("visit.txt".into()),
                ..DocTemplateArgs::default()
            },
        )
        .unwrap();
        let ch = env.data.check.as_ref().unwrap();
        assert!(ch.detected_format.starts_with("SOAP"));
        let present = |id: &str| ch.elements.iter().find(|e| e.id == id).unwrap().present;
        assert!(present("date_time"));
        assert!(present("signature"));
        assert!(present("consent"));
        assert!(present("language"));
        assert!(present("teaching"));
        assert!(present("follow_up"));
        assert!(!present("safety"));
        let teaching = ch.elements.iter().find(|e| e.id == "teaching").unwrap();
        assert_eq!(teaching.found_in_section.as_deref(), Some("P - Plan"));
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("Quackenbush") && !json.contains("missed two doses"));
    }

    #[test]
    fn writes_template_files_and_rejects_unknown_skips() {
        let (_dir, c) = ctx(true);
        let env = nursing_documentation_template(
            &c,
            DocTemplateArgs {
                format: Some(DocFormat::Dar),
                visit_type: Some(VisitType::DotVisit),
                write_template: true,
                skip_elements: vec!["consent".into()],
                ..DocTemplateArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.files.len(), 2);
        assert!(!env.data.required_elements.iter().any(|e| e.id == "consent"));
        assert!(
            env.data
                .required_elements
                .iter()
                .any(|e| e.id == "dose_observed")
        );
        let e = nursing_documentation_template(
            &c,
            DocTemplateArgs {
                skip_elements: vec!["nonsense".into()],
                ..DocTemplateArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("date_time"));
    }
}
