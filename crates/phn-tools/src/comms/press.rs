//! COMM-04 `draft_press_materials`: structures for a press release, media
//! advisory, talking points, or spokesperson Q&A, filled from the facts given
//! (missing parts are marked for the nurse), checks against CDC's CERC media
//! guidance (5 Ws, contact block, date, release time or embargo, one page,
//! sentence length, loaded words), readability, a privacy check, safe
//! messaging for suicide topics, and an optional Word document in
//! /data/output.

use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Document, from_markdown};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, CAMPAIGNS};
use super::readability::{AnalyzeOptions, analyze, default_target_grade};
use super::safe_messaging::{self, SafeMessagingReport};
use super::social::CAMPAIGN;
use crate::context::Context;

/// The facts of the story (who, what, when, where, why, how).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StoryFacts {
    pub who: Option<String>,
    pub what: Option<String>,
    pub when: Option<String>,
    pub r#where: Option<String>,
    pub why: Option<String>,
    pub how: Option<String>,
}

/// A quote from an official (use titles; never quote clients or patients).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct Quote {
    /// Name and title of the official, e.g. "Health Director Jane Doe, RN".
    pub speaker: String,
    pub text: String,
}

/// The media contact.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct MediaContact {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    /// A 24-hour number for reporters, if different.
    pub after_hours_phone: Option<String>,
}

/// Arguments for `draft_press_materials`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PressArgs {
    /// "press_release" (default), "media_advisory", "talking_points", or "qa".
    pub kind: Option<String>,
    /// Headline (press release) or event name (advisory).
    pub headline: Option<String>,
    pub facts: StoryFacts,
    /// Main messages (talking points), most important first.
    pub key_messages: Vec<String>,
    /// What people should do (call to action).
    pub call_to_action: Option<String>,
    pub quotes: Vec<Quote>,
    pub contact: MediaContact,
    /// Release date (YYYY-MM-DD; default today).
    pub release_date: Option<String>,
    /// Embargo until this date and time; omit for "For immediate release".
    pub embargo_until: Option<String>,
    /// A draft to check instead of (or as well as) building the structure.
    pub draft_text: Option<String>,
    /// Reading grade to aim for (default 8).
    pub target_grade: Option<f64>,
    /// Write the draft structure as a Word document to /data/output ("docx").
    pub export: Option<String>,
}

/// One check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PressCheck {
    pub id: String,
    pub passed: bool,
    pub detail: String,
    /// CERC chapter and page, or the tool rule.
    pub source: String,
}

/// Result of `draft_press_materials`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PressResult {
    pub kind: String,
    /// The document in Markdown, with [BRACKETED] parts to complete.
    pub draft_markdown: String,
    /// Parts still missing.
    pub missing: Vec<String>,
    pub checks: Vec<PressCheck>,
    pub reading_grade: f64,
    pub words: usize,
    pub safe_messaging: Option<SafeMessagingReport>,
    pub cerc_principles: Vec<String>,
    /// Questions a spokesperson should prepare for (CERC).
    pub questions_to_prepare: Vec<String>,
    pub export_file: Option<String>,
}

const LOADED: &[&str] = &[
    "devastating",
    "terrifying",
    "horrific",
    "shocking",
    "alarming",
    "catastrophic",
    "deadly",
    "tragic",
    "unprecedented",
    "skyrocketing",
    "plague",
    "panic",
];

/// Emotionally loaded words found in lower-cased text (CERC: remove adjectives and
/// loaded words). Shared with EPR-10 `crisis_risk_communication_builder`.
///
/// A loaded word matches a whole word or the start of one ("panic", "panicked"),
/// never the inside of another word ("Hispanic").
pub(crate) fn loaded_words(lower: &str) -> Vec<&'static str> {
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    LOADED
        .iter()
        .copied()
        .filter(|w| tokens.iter().any(|t| t.starts_with(*w)))
        .collect()
}

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn build(kind: &str, a: &PressArgs, date: &str) -> (String, Vec<String>) {
    let mut missing = Vec::new();
    let mut need = |label: &str, v: Option<&str>| -> String {
        match v {
            Some(x) => x.to_string(),
            None => {
                missing.push(label.to_string());
                format!("[{}]", label.to_uppercase())
            }
        }
    };
    let f = &a.facts;
    let contact = format!(
        "Contact: {} · {} · {}{}",
        need("contact name", opt(&a.contact.name)),
        need("contact phone", opt(&a.contact.phone)),
        need("contact email", opt(&a.contact.email)),
        opt(&a.contact.after_hours_phone)
            .map(|p| format!(" · 24-hour media line: {p}"))
            .unwrap_or_default()
    );
    let release = match opt(&a.embargo_until) {
        Some(e) => format!("EMBARGOED UNTIL {e}"),
        None => "FOR IMMEDIATE RELEASE".into(),
    };
    let md = match kind {
        "media_advisory" => {
            format!(
                "# Media advisory: {}\n\n{contact}\n\n{release} · {date}\n\n**What:** {}\n\n**When:** {}\n\n**Where:** {}\n\n**Who:** {}\n\n**Why:** {}\n\n**Topics:** {}\n",
                need("event name", opt(&a.headline)),
                need("what (media opportunity or press conference)", opt(&f.what)),
                need("date and time", opt(&f.when)),
                need("place", opt(&f.r#where)),
                need("who will appear (name and title)", opt(&f.who)),
                need("why it matters", opt(&f.why)),
                if a.key_messages.is_empty() {
                    need("topics to be covered", None)
                } else {
                    a.key_messages.join("; ")
                },
            )
        }
        "talking_points" => {
            let mut s = format!("# Talking points: {}\n\n", need("topic", opt(&a.headline)));
            if a.key_messages.is_empty() {
                s.push_str(&format!(
                    "1. {}\n2. [SUPPORTING FACT]\n3. [WHAT PEOPLE SHOULD DO]\n",
                    need("main message", None)
                ));
            } else {
                for (i, m) in a.key_messages.iter().enumerate() {
                    s.push_str(&format!("{}. {m}\n", i + 1));
                }
            }
            s.push_str(&format!(
                "\n**What people should do:** {}\n\n**What we know and what we don't know yet:** {}\n",
                need("call to action", opt(&a.call_to_action)),
                need("known and unknown", opt(&f.how))
            ));
            s
        }
        "qa" => {
            let mut s = format!(
                "# Spokesperson Q&A: {}\n\n",
                need("topic", opt(&a.headline))
            );
            for q in CAMPAIGN.spokesperson_questions.iter() {
                s.push_str(&format!("**Q: {q}**\n\nA: [ANSWER]\n\n"));
            }
            s.push_str(&format!(
                "**Bridge back to the main message:** {}\n",
                a.key_messages
                    .first()
                    .cloned()
                    .unwrap_or_else(|| need("main message", None))
            ));
            s
        }
        _ => {
            let lead = format!(
                "{} {} {} {}.",
                need("who", opt(&f.who)),
                need("what", opt(&f.what)),
                need("when", opt(&f.when)),
                need("where", opt(&f.r#where))
            );
            let mut s = format!(
                "{contact}\n\n{release}\n\n# {}\n\n{date}. {lead} {}\n\n",
                need("headline", opt(&a.headline)),
                need("why it matters", opt(&f.why))
            );
            if let Some(c) = opt(&a.call_to_action) {
                s.push_str(&format!("{c}\n\n"));
            } else {
                s.push_str(&format!("{}\n\n", need("what people should do", None)));
            }
            if let Some(h) = opt(&f.how) {
                s.push_str(&format!("{h}\n\n"));
            }
            for q in &a.quotes {
                s.push_str(&format!(
                    "\"{}\" said {}.\n\n",
                    q.text.trim(),
                    q.speaker.trim()
                ));
            }
            if a.quotes.is_empty() {
                s.push_str("\"[QUOTE FROM AN OFFICIAL]\" said [NAME, TITLE].\n\n");
                missing.push("quote".into());
            }
            s.push_str("###\n");
            s
        }
    };
    (md, missing)
}

/// Draft and check press materials.
pub fn draft_press_materials(ctx: &Context, args: PressArgs) -> Result<Envelope<PressResult>> {
    let c = &*CAMPAIGN;
    let kind = match args
        .kind
        .as_deref()
        .map(|k| k.trim().to_lowercase().replace([' ', '-'], "_"))
    {
        None => "press_release".to_string(),
        Some(k) if k.is_empty() => "press_release".to_string(),
        Some(k) => match k.as_str() {
            "press_release" | "news_release" | "release" => "press_release".into(),
            "media_advisory" | "advisory" => "media_advisory".into(),
            "talking_points" | "message_map" => "talking_points".into(),
            "qa" | "q_and_a" | "q&a" | "spokesperson_qa" => "qa".into(),
            other => {
                return Err(PhnError::InvalidInput(format!(
                    "kind '{other}' is not recognized; use press_release, media_advisory, talking_points, or qa"
                )));
            }
        },
    };
    let date = match opt(&args.release_date) {
        Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| {
                PhnError::InvalidInput(format!("release_date '{d}' is not a date (YYYY-MM-DD)"))
            })?
            .format("%B %-d, %Y")
            .to_string(),
        None => chrono::Local::now()
            .date_naive()
            .format("%B %-d, %Y")
            .to_string(),
    };
    let target = args.target_grade.unwrap_or_else(default_target_grade);
    let (built, missing) = build(&kind, &args, &date);
    let draft = opt(&args.draft_text).map(str::to_string);
    let checked = draft.clone().unwrap_or_else(|| built.clone());

    // ---- checks
    let a = analyze(
        &checked,
        &AnalyzeOptions {
            target_grade: target,
            max_flags: 5,
            redact_excerpts: false,
        },
    );
    let lower = checked.to_lowercase();
    let src = |id: &str| {
        c.press_release_checklist
            .iter()
            .chain(c.media_advisory_elements.iter())
            .find(|x| x.id == id)
            .map(|x| format!("CERC: Working with the Media, {} ({})", x.section, x.text))
            .unwrap_or_else(|| "tool rule".into())
    };
    let mut checks = Vec::new();
    let has_phone = regex::Regex::new(r"\(?\d{3}\)?[-. ]\d{3}[-. ]\d{4}")
        .expect("valid regex")
        .is_match(&checked);
    let has_email = checked.contains('@');
    checks.push(PressCheck {
        id: "contact_block".into(),
        passed: has_phone && has_email && !checked.contains("[CONTACT"),
        detail: if has_phone && has_email {
            "contact phone and email present".into()
        } else {
            "add a contact name, phone, and email at the top (and a 24-hour media number)".into()
        },
        source: src("contact_block"),
    });
    if kind == "press_release" || kind == "media_advisory" {
        let has_date = regex::Regex::new(r"(?i)\b(january|february|march|april|may|june|july|august|september|october|november|december)\s+\d{1,2},\s+\d{4}|\d{4}-\d{2}-\d{2}")
            .expect("valid regex")
            .is_match(&checked);
        checks.push(PressCheck {
            id: "date".into(),
            passed: has_date,
            detail: if has_date {
                "dated".into()
            } else {
                "add the release date".into()
            },
            source: src("date"),
        });
        let rel = lower.contains("for immediate release") || lower.contains("embargo");
        checks.push(PressCheck {
            id: "release_time".into(),
            passed: rel,
            detail: if rel {
                "release time stated".into()
            } else {
                "say 'For immediate release' or give the embargo time".into()
            },
            source: src("release_time"),
        });
        let ws: Vec<(&str, bool)> = vec![
            ("who", opt(&args.facts.who).is_some()),
            ("what", opt(&args.facts.what).is_some()),
            (
                "when",
                opt(&args.facts.when).is_some() || lower.contains(" on ") || lower.contains(" at "),
            ),
            (
                "where",
                opt(&args.facts.r#where).is_some()
                    || lower.contains(" at ")
                    || lower.contains(" in "),
            ),
            (
                "why",
                opt(&args.facts.why).is_some()
                    || lower.contains("because")
                    || lower.contains("to protect")
                    || lower.contains("to help"),
            ),
        ];
        let lacking: Vec<&str> = ws.iter().filter(|(_, ok)| !ok).map(|(w, _)| *w).collect();
        checks.push(PressCheck {
            id: "five_ws".into(),
            passed: lacking.is_empty() && !checked.contains("[WHO]") && !checked.contains("[WHAT]"),
            detail: if lacking.is_empty() {
                "who, what, when, where, and why are covered".into()
            } else {
                format!("check: {}", lacking.join(", "))
            },
            source: src("five_ws"),
        });
    }
    if kind == "press_release" {
        let words = a.counts.words;
        checks.push(PressCheck {
            id: "one_page".into(),
            passed: words <= 500,
            detail: format!("{words} words (about one page is 400 to 500 words)"),
            source: src("one_page"),
        });
        let headline_ok = opt(&args.headline).is_some()
            || draft
                .as_deref()
                .is_some_and(|d| d.lines().any(|l| l.trim_start().starts_with('#')));
        checks.push(PressCheck {
            id: "headline".into(),
            passed: headline_ok,
            detail: if headline_ok {
                "headline present".into()
            } else {
                "add an active-voice headline".into()
            },
            source: src("headline"),
        });
    }
    checks.push(PressCheck {
        id: "short_sentences".into(),
        passed: a.flags.long_sentence_count == 0,
        detail: format!("{} sentence(s) over 20 words", a.flags.long_sentence_count),
        source: src("short_sentences"),
    });
    let loaded: Vec<&str> = loaded_words(&lower);
    checks.push(PressCheck {
        id: "no_loaded_words".into(),
        passed: loaded.is_empty(),
        detail: if loaded.is_empty() {
            "no emotionally loaded words found".into()
        } else {
            format!("remove loaded words: {}", loaded.join(", "))
        },
        source: src("no_loaded_words"),
    });
    let ids = phn_local::phi::scan_text(&checked);
    let id_kinds: std::collections::BTreeSet<String> = ids
        .iter()
        .filter(|f| {
            use phn_local::phi::Identifier as I;
            // Officials' names, the media contact, dates, and places belong in a release.
            matches!(
                f.kind,
                I::Ssn
                    | I::MedicalRecordNumber
                    | I::HealthPlanNumber
                    | I::BirthDate
                    | I::AccountNumber
                    | I::LicenseNumber
                    | I::StreetAddress
                    | I::Age
            )
        })
        .map(|f| f.kind.label().to_string())
        .collect();
    checks.push(PressCheck {
        id: "privacy_check".into(),
        passed: id_kinds.is_empty(),
        detail: if id_kinds.is_empty() {
            "no client identifiers detected (still check names, ages, and places that could identify a person)".into()
        } else {
            format!("possible identifiers: {}; remove anything that could identify a client", id_kinds.iter().cloned().collect::<Vec<_>>().join(", "))
        },
        source: src("privacy_check"),
    });
    let suicide = lower.contains("suicid");
    let safe = suicide.then(|| safe_messaging::check(&checked));

    // ---- export
    let mut privacy = Privacy::none();
    let mut export_file = None;
    if let Some(fmt) = args
        .export
        .as_deref()
        .map(|f| f.trim().to_lowercase())
        .filter(|f| !f.is_empty() && f != "none")
    {
        if fmt != "docx" {
            return Err(PhnError::InvalidInput("export must be \"docx\"".into()));
        }
        let doc = Document {
            title: match kind.as_str() {
                "media_advisory" => "Media advisory",
                "talking_points" => "Talking points",
                "qa" => "Spokesperson Q&A",
                _ => "Press release",
            }
            .into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: from_markdown(&checked),
        };
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let sb = ctx.sandbox()?;
        let path = sb.new_output_file(&format!("{kind}.docx"))?;
        std::fs::write(&path, bytes)?;
        let disp = sb.display_path(&path);
        privacy.files_written.push(disp.clone());
        export_file = Some(disp);
    }
    if !id_kinds.is_empty() {
        privacy.identifiers_detected = id_kinds.iter().cloned().collect();
        privacy.notes.push(
            "Possible client identifiers were found in the text; remove them before release."
                .into(),
        );
    }

    // ---- summary
    let failed: Vec<&PressCheck> = checks.iter().filter(|c| !c.passed).collect();
    let mut summary = format!(
        "**{}** ({} words, reading grade {:.1}; target {target})\n\n",
        match kind.as_str() {
            "media_advisory" => "Media advisory",
            "talking_points" => "Talking points",
            "qa" => "Spokesperson Q&A",
            _ => "Press release",
        },
        a.counts.words,
        a.grade.median_grade
    );
    if draft.is_none() {
        summary.push_str("```markdown\n");
        summary.push_str(&built);
        summary.push_str("```\n\n");
        if !missing.is_empty() {
            summary.push_str(&format!(
                "Complete the bracketed parts: {}.\n\n",
                missing.join(", ")
            ));
        }
    }
    if failed.is_empty() {
        summary.push_str("All CERC checks passed.\n");
    } else {
        summary.push_str("**Checks to fix**\n");
        for f in &failed {
            summary.push_str(&format!("- {}: {}\n", f.id.replace('_', " "), f.detail));
        }
    }
    if let Some(s) = &safe {
        summary.push_str(&format!(
            "\n**Safe messaging (suicide):** {} problem(s), {} must be fixed.\n",
            s.findings.len(),
            s.must_fix
        ));
        for f in &s.findings {
            summary.push_str(&format!("- {} {}\n", f.message, f.fix));
        }
    }
    summary.push_str(&format!(
        "\n**CERC principles:** {}.\n",
        c.cerc_principles
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if kind == "qa" || kind == "talking_points" {
        summary.push_str(&format!(
            "**Spokesperson tips (CERC):** {}\n",
            c.spokesperson_tips.join(" ")
        ));
    }
    if let Some(f) = &export_file {
        summary.push_str(&format!("\nWord document written to `{f}`.\n"));
    }

    let mut sources = content::cite(
        &c.sources,
        &["cerc_media", "cerc_intro", "cerc_card"],
        &c.meta.prepared,
    );
    if safe.is_some() {
        sources.extend(safe_messaging::sources());
    }
    let mut caveats = vec![
        "This is a structure to complete and clear through your agency's approval process; facts, quotes, and numbers must come from you and be verified.".to_string(),
        "Never include client names or details that could identify a person; follow HIPAA and agency policy (CERC privacy check).".to_string(),
    ];
    caveats.extend(content::review_caveats(&[CAMPAIGNS]));
    Ok(Envelope::new(
        summary,
        PressResult {
            kind,
            draft_markdown: built,
            missing,
            checks,
            reading_grade: a.grade.median_grade,
            words: a.counts.words,
            safe_messaging: safe,
            cerc_principles: c
                .cerc_principles
                .iter()
                .map(|p| format!("{}: {}", p.name, p.text))
                .collect(),
            questions_to_prepare: c.spokesperson_questions.clone(),
            export_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
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
    fn loaded_words_match_whole_words_not_parts_of_other_words() {
        assert!(loaded_words("outreach to hispanic residents of the city").is_empty());
        assert_eq!(
            loaded_words("a devastating outbreak; residents panicked."),
            vec!["devastating", "panic"]
        );
        assert_eq!(loaded_words("no need to panic"), vec!["panic"]);
    }

    #[test]
    fn press_release_structure_and_checks() {
        let (_d, ctx) = ctx();
        let r = draft_press_materials(
            &ctx,
            PressArgs {
                headline: Some("Free flu shots at Town Hall".into()),
                facts: StoryFacts {
                    who: Some("The Springfield Health Department".into()),
                    what: Some("will offer free flu shots".into()),
                    when: Some("on Saturday, November 14, from 9 a.m. to noon".into()),
                    r#where: Some("at Town Hall".into()),
                    why: Some("Flu shots protect you and your family.".into()),
                    how: None,
                },
                call_to_action: Some("No appointment is needed.".into()),
                quotes: vec![Quote {
                    speaker: "Health Director Alex Lee".into(),
                    text: "Getting a flu shot is the best way to stay healthy this winter.".into(),
                }],
                contact: MediaContact {
                    name: Some("Pat Kim".into()),
                    phone: Some("413-555-0199".into()),
                    email: Some("press@example.org".into()),
                    after_hours_phone: None,
                },
                release_date: Some("2026-11-09".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert!(d.missing.is_empty(), "{:?}", d.missing);
        assert!(d.draft_markdown.contains("FOR IMMEDIATE RELEASE"));
        assert!(d.draft_markdown.contains("November 9, 2026"));
        for id in [
            "contact_block",
            "date",
            "release_time",
            "five_ws",
            "one_page",
            "headline",
        ] {
            assert!(
                d.checks.iter().any(|c| c.id == id && c.passed),
                "{id}: {:?}",
                d.checks
            );
        }
        // A bare draft fails the structural checks.
        let r = draft_press_materials(
            &ctx,
            PressArgs {
                draft_text: Some("A devastating outbreak is spreading.".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let failed: Vec<&str> = r
            .data
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.id.as_str())
            .collect();
        assert!(
            failed.contains(&"contact_block")
                && failed.contains(&"no_loaded_words")
                && failed.contains(&"release_time"),
            "{failed:?}"
        );
    }
}
