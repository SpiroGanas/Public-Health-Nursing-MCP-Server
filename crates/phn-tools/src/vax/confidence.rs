//! VAX-09 `vaccine_confidence_conversation_guide`.

use phn_core::text;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::vax::content::{self, GUIDE};
use crate::vax::readability::flesch_kincaid;

/// Arguments for `vaccine_confidence_conversation_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ConfidenceArgs {
    /// The concern in the person's words, e.g. "worried about autism", "too many
    /// shots", "HPV at 11", "flu shot during pregnancy", "the CDC changed the schedule".
    pub concern: String,
    /// Target reading grade level for what you say (default 8).
    pub target_grade: Option<f64>,
    /// Language of the conversation (default English). Other languages get
    /// links to authoritative translated resources, never machine translation.
    pub language: Option<String>,
    /// Most facts to include (default 3; fewer keeps it short).
    pub max_facts: Option<usize>,
}

/// One cited talking point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TalkingPoint {
    pub text: String,
    pub source_title: String,
    pub source_publisher: String,
    pub source_url: String,
    pub source_date: Option<String>,
}

/// Reading level of the script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReadingLevel {
    pub flesch_kincaid_grade: f64,
    pub target_grade: f64,
    pub words: usize,
    pub sentences: usize,
    pub meets_target: bool,
    pub method: String,
}

/// A translated resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TranslatedLink {
    pub language: String,
    pub title: String,
    pub url: String,
}

/// Result of `vaccine_confidence_conversation_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConfidenceGuideResult {
    pub concern_id: String,
    pub concern_title: String,
    pub open_question: String,
    pub reflection: String,
    pub ask_permission: String,
    pub talking_points: Vec<TalkingPoint>,
    /// Where federal guidance differs from DPH, AAP, and Immunize.org.
    pub divergence: Option<String>,
    pub next_step: String,
    pub principles: Vec<String>,
    pub reading_level: ReadingLevel,
    pub language: String,
    /// True when a qualified interpreter or translator is needed.
    pub translator_needed: bool,
    pub translated_resources: Vec<TranslatedLink>,
    /// Other concerns this guide covers.
    pub other_topics: Vec<String>,
}

/// Build a motivational-interviewing response to a vaccine concern.
pub fn vaccine_confidence_conversation_guide(
    _ctx: &Context,
    args: ConfidenceArgs,
) -> Result<Envelope<ConfidenceGuideResult>> {
    let q = args.concern.trim();
    if q.is_empty() {
        return Err(PhnError::InvalidInput("concern is empty; describe the question or worry in a few words".into()));
    }
    let target = args.target_grade.unwrap_or(8.0);
    if !(3.0..=16.0).contains(&target) {
        return Err(PhnError::InvalidInput("target_grade must be between 3 and 16".into()));
    }
    let max_facts = args.max_facts.unwrap_or(3).clamp(1, 6);
    let ranked = text::rank(
        q,
        &GUIDE.concerns,
        |c| {
            let mut k = vec![c.title.clone(), c.id.replace('_', " ")];
            k.extend(c.aliases.iter().cloned());
            k
        },
        0.78,
    );
    let Some(best) = ranked.first() else {
        return Err(PhnError::NotFound(format!(
            "no conversation guide matches '{q}'. Topics: {}",
            GUIDE.concerns.iter().map(|c| c.title.clone()).collect::<Vec<_>>().join("; ")
        )));
    };
    let c = &GUIDE.concerns[best.index];
    let points: Vec<TalkingPoint> = c
        .facts
        .iter()
        .take(max_facts)
        .map(|f| {
            let s = GUIDE.sources.get(&f.source);
            TalkingPoint {
                text: f.text.clone(),
                source_title: s.map(|s| s.title.clone()).unwrap_or_default(),
                source_publisher: s.map(|s| s.publisher.clone()).unwrap_or_default(),
                source_url: s.map(|s| s.url.clone()).unwrap_or_default(),
                source_date: s.and_then(|s| s.date.clone()),
            }
        })
        .collect();
    let spoken = format!(
        "{}\n{}\n{}\n{}\n{}",
        c.open_question,
        c.reflection,
        c.permission,
        points.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n"),
        c.next_step
    );
    let r = flesch_kincaid(&spoken);
    let reading_level = ReadingLevel {
        flesch_kincaid_grade: r.fk_grade,
        target_grade: target,
        words: r.words,
        sentences: r.sentences,
        meets_target: r.fk_grade <= target,
        method: "Flesch-Kincaid grade = 0.39 × words/sentence + 11.8 × syllables/word − 15.59, syllables estimated by vowel groups (about ±1 grade).".into(),
    };

    let language = args.language.as_deref().map(str::trim).filter(|l| !l.is_empty()).unwrap_or("English").to_string();
    let english = language.eq_ignore_ascii_case("english") || language.eq_ignore_ascii_case("en");
    let lang_norm = text::normalize(&language);
    let translated_resources: Vec<TranslatedLink> = if english {
        Vec::new()
    } else {
        GUIDE
            .translated_resources
            .iter()
            .filter(|t| {
                let l = text::normalize(&t.language);
                l == "many" || l.contains(&lang_norm) || lang_norm.contains(&l)
            })
            .map(|t| TranslatedLink { language: t.language.clone(), title: t.title.clone(), url: t.url.clone() })
            .collect()
    };

    let mut summary = format!("**Conversation guide: {}**\n\n", c.title);
    summary.push_str(&format!("1. **Ask (open question):** \"{}\"\n", c.open_question));
    summary.push_str(&format!("2. **Reflect and affirm:** \"{}\"\n", c.reflection.trim()));
    summary.push_str(&format!("3. **Ask permission:** \"{}\"\n", c.permission));
    summary.push_str("4. **Offer information** (one or two points, then check understanding):\n");
    for p in &points {
        summary.push_str(&format!("   - {} — _{} ({}{})_\n", p.text.trim(), p.source_publisher, p.source_title, p.source_date.as_ref().map(|d| format!(", {d}")).unwrap_or_default()));
    }
    summary.push_str(&format!("5. **Summarize and agree on a next step:** {}\n", c.next_step));
    if let Some(d) = &c.divergence {
        summary.push_str(&format!("\n⚠ **Where guidance differs:** {}\n", d.trim()));
    }
    summary.push_str(&format!(
        "\nReading level of the script: Flesch-Kincaid grade {:.1} (target {target}){}.\n",
        r.fk_grade,
        if reading_level.meets_target { "" } else { " — above target; use shorter sentences and everyday words" }
    ));
    if !english {
        summary.push_str(&format!(
            "\n**Language: {language}.** This server does not machine-translate clinical text. Use a qualified medical interpreter for the conversation and a qualified translator for written materials. Authoritative translated resources:\n"
        ));
        for t in &translated_resources {
            summary.push_str(&format!("- {} ({}): {}\n", t.title, t.language, t.url));
        }
    }

    let mut sources: Vec<Source> = Vec::new();
    for p in &points {
        if sources.iter().any(|s| s.url.as_deref() == Some(p.source_url.as_str())) {
            continue;
        }
        sources.push(Source {
            title: p.source_title.clone(),
            publisher: p.source_publisher.clone(),
            url: Some(p.source_url.clone()),
            effective_date: p.source_date.clone(),
            retrieved: content::meta(content::CONFIDENCE).ok().map(|m| m.prepared),
            mode: SourceMode::Snapshot,
        });
    }
    sources.push(content::content_source(content::CONFIDENCE)?);
    let mut caveats = vec![
        "The conversation wording is a draft for a clinician to adapt; facts come from the cited sources. Respect the person's autonomy and document the discussion per your agency's policy.".to_string(),
    ];
    if !english {
        caveats.push("A qualified interpreter or translator is needed; translated text was not generated.".into());
    }
    caveats.extend(content::review_caveats(&[content::CONFIDENCE]));
    Ok(Envelope::new(
        summary,
        ConfidenceGuideResult {
            concern_id: c.id.clone(),
            concern_title: c.title.clone(),
            open_question: c.open_question.clone(),
            reflection: c.reflection.trim().to_string(),
            ask_permission: c.permission.clone(),
            talking_points: points,
            divergence: c.divergence.as_ref().map(|d| d.trim().to_string()),
            next_step: c.next_step.clone(),
            principles: GUIDE.mi_principles.clone(),
            reading_level,
            language,
            translator_needed: !english,
            translated_resources,
            other_topics: GUIDE.concerns.iter().filter(|o| o.id != c.id).map(|o| o.title.clone()).collect(),
        },
    )
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
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn guide(ctx: &Context, concern: &str) -> ConfidenceGuideResult {
        vaccine_confidence_conversation_guide(
            ctx,
            ConfidenceArgs {
                concern: concern.into(),
                ..Default::default()
            },
        )
        .unwrap()
        .data
    }

    #[test]
    fn concerns_match_in_everyday_words() {
        let (_d, ctx) = ctx();
        assert_eq!(guide(&ctx, "too many shots at once").concern_id, "too_many_too_soon");
        assert_eq!(guide(&ctx, "what's in vaccines, aluminum?").concern_id, "ingredients");
        assert_eq!(guide(&ctx, "vaccines during pregnancy").concern_id, "pregnancy");
        assert_eq!(guide(&ctx, "flu shot").concern_id, "influenza");
        let g = guide(&ctx, "autism");
        assert!(g.talking_points.len() <= 3);
        assert!(g.talking_points.iter().all(|p| !p.source_url.is_empty()));
        assert!(!g.translator_needed);
    }

    #[test]
    fn every_fact_is_cited_and_every_script_is_scored() {
        let (_d, ctx) = ctx();
        for c in &GUIDE.concerns {
            for f in &c.facts {
                assert!(GUIDE.sources.contains_key(&f.source), "{}: unknown source {}", c.id, f.source);
            }
            let g = guide(&ctx, &c.title);
            assert_eq!(g.concern_id, c.id);
            assert!(g.reading_level.flesch_kincaid_grade > 0.0);
        }
    }
}
