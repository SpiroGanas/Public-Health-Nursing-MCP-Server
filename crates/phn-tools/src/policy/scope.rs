//! POL-06 `nursing_scope_of_practice_guide`: answers scope, delegation, and
//! standing-order questions by pointing to the parts of 244 CMR (Board of
//! Registration in Nursing regulations), M.G.L. c. 112, section 80B, and Board
//! advisory rulings that bear on them, each with a citation, a short quotation,
//! and a paraphrase, plus the Board's own decision questions. It is a partial
//! snapshot of the sources that could be read (the coverage is part of every
//! result). It never says that a specific act is within a nurse's scope of
//! practice: the Board decides, and the answer depends on the facts.

use std::collections::{BTreeMap, BTreeSet};

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Cited, Confidence, DocSource};
use crate::md;
use crate::policy::shared::{contains_phrase, optional_text, stem_words};

/// Content id of `content/policy/nursing_scope_of_practice.yaml`.
pub const SCOPE: &str = "policy-nursing-scope-of-practice";

const MAX_QUESTION: usize = 500;
const DEFAULT_ENTRIES: usize = 12;
const MAX_ENTRIES: usize = 40;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadScope {
    pub read_in_full: Vec<String>,
    pub read_in_part: Vec<String>,
    pub not_read: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuidelineQuestion {
    pub number: u8,
    pub question: String,
    pub if_yes: String,
    pub if_no: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guidelines {
    pub intro: Cited,
    pub questions: Vec<GuidelineQuestion>,
    pub framework_steps: Vec<String>,
    pub framework_source: String,
    pub framework_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferralDef {
    pub text: String,
    pub contacts: Vec<String>,
    pub source: String,
    pub contact_source: String,
    pub retrieved: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicDef {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryDef {
    pub id: String,
    pub topics: Vec<String>,
    pub citation: String,
    pub source: String,
    pub section: String,
    pub applies_to: Vec<String>,
    pub quote: String,
    pub paraphrase: String,
    pub confidence: Confidence,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RulingDef {
    pub number: String,
    pub title: String,
    pub area: String,
    pub issued: Option<String>,
    pub last_revised: Option<String>,
    pub scope: Option<String>,
    pub url: String,
    pub read: bool,
}

/// `content/policy/nursing_scope_of_practice.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub read_scope: ReadScope,
    pub decision_guidelines: Guidelines,
    pub referral: ReferralDef,
    pub topics: Vec<TopicDef>,
    pub entries: Vec<EntryDef>,
    pub advisory_rulings: Vec<RulingDef>,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- arguments and results

/// A nursing role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Registered nurse.
    Rn,
    /// Licensed practical nurse.
    Lpn,
    /// Advanced practice registered nurse.
    Aprn,
    /// Do not filter by role.
    Any,
}

impl Role {
    fn key(self) -> Option<&'static str> {
        match self {
            Role::Rn => Some("rn"),
            Role::Lpn => Some("lpn"),
            Role::Aprn => Some("aprn"),
            Role::Any => None,
        }
    }
}

/// Arguments for `nursing_scope_of_practice_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ScopeArgs {
    /// Your scope, delegation, or standing-order question in your own words ("Can an LPN give
    /// flu vaccines at a clinic under a standing order?", "What must a delegating nurse do before
    /// delegating?"). Describe the situation in general terms; no names of people.
    pub question: Option<String>,
    /// Topics to look up by id or name instead of (or with) a question: scope_framework, rn_lpn,
    /// delegation, delegation_medication, standing_orders, vaccines, otc_school, harm_reduction,
    /// aprn, research, licensure, management_charge, conduct_reporting, map_program,
    /// new_procedures. Leave both empty to list the topic index.
    pub topics: Vec<String>,
    /// Show provisions that apply to this role (rn, lpn, aprn). Default: any.
    pub role: Option<Role>,
    /// Maximum provisions returned (default 12, at most 40).
    pub max_results: Option<u32>,
    /// List every advisory ruling on the Board's list, including those not read (default: only
    /// rulings cited by the provisions shown, plus listed rulings whose titles match your words).
    pub list_all_rulings: Option<bool>,
}

/// A topic in the index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TopicOut {
    pub id: String,
    pub label: String,
    /// How it matched: "question", "topic argument", or "index".
    pub matched_by: String,
}

/// One provision with its citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EntryOut {
    pub id: String,
    /// For example "244 CMR 3.05(2)(b)" or "Advisory Ruling 9804".
    pub citation: String,
    /// Title of the source document and its date.
    pub source: String,
    pub url: String,
    pub applies_to: Vec<String>,
    /// A quotation of 25 words or fewer from the source.
    pub quote: String,
    /// The guide's own summary of the provision (read the source before relying on it).
    pub paraphrase: String,
    pub confidence: Confidence,
    pub note: Option<String>,
}

/// One of the Board's six decision questions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DecisionQuestion {
    pub number: u8,
    pub question: String,
    pub if_yes: String,
    pub if_no: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// The Board's decision making guidelines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DecisionGuide {
    pub intro: String,
    pub questions: Vec<DecisionQuestion>,
    /// The Board's eight-box flow chart.
    pub framework_steps: Vec<String>,
    pub framework_note: String,
}

/// An advisory ruling on the Board's list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RulingOut {
    pub number: String,
    pub title: String,
    /// "nursing practice" or "nursing education".
    pub area: String,
    pub issued: Option<String>,
    pub last_revised: Option<String>,
    pub scope: Option<String>,
    pub url: String,
    /// True when the ruling was read for this guide; false when only its title is known.
    pub read: bool,
}

/// What was and was not read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Coverage {
    /// Always true: this is a partial snapshot.
    pub partial: bool,
    pub snapshot_date: String,
    pub read_in_full: Vec<String>,
    pub read_in_part: Vec<String>,
    pub not_read: Vec<String>,
    pub rulings_listed: usize,
    pub rulings_read: usize,
}

/// Where to take the question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Referral {
    pub text: String,
    pub contacts: Vec<String>,
}

/// Result of `nursing_scope_of_practice_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScopeResult {
    /// Always "citations_and_decision_questions": the guide does not say that an act is in scope.
    pub answer_type: String,
    pub matched_topics: Vec<TopicOut>,
    pub provisions: Vec<EntryOut>,
    /// Provisions that matched but were cut by max_results.
    pub more_provisions: usize,
    pub decision_guide: DecisionGuide,
    pub advisory_rulings: Vec<RulingOut>,
    /// The topic index (given when nothing matched or nothing was asked).
    pub topic_index: Vec<TopicOut>,
    pub coverage: Coverage,
    pub refer_to_board: Referral,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- logic

fn flatten(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn source_label(c: &ScopeContent, key: &str) -> String {
    match c.sources.get(key) {
        Some(s) => match &s.effective_date {
            Some(d) => format!("{} (dated {d})", s.title),
            None => s.title.clone(),
        },
        None => key.to_string(),
    }
}

fn source_url(c: &ScopeContent, key: &str) -> String {
    c.sources.get(key).map(|s| s.url.clone()).unwrap_or_default()
}

/// Topics matched by a question and by topic arguments. Returns the topic
/// ids (in content order) with how each matched.
pub fn match_topics(c: &ScopeContent, question: &str, topic_args: &[String]) -> Vec<(String, &'static str)> {
    let words = stem_words(question);
    let arg_words: Vec<Vec<String>> = topic_args.iter().map(|t| stem_words(t)).collect();
    let mut out = Vec::new();
    for t in &c.topics {
        let by_arg = topic_args.iter().zip(&arg_words).any(|(raw, w)| {
            let norm = phn_core::text::normalize(&raw.replace('_', " "));
            norm == phn_core::text::normalize(&t.id.replace('_', " "))
                || norm == phn_core::text::normalize(&t.label)
                || t.keywords.iter().any(|k| contains_phrase(w, k))
        });
        if by_arg {
            out.push((t.id.clone(), "topic argument"));
        } else if t.keywords.iter().any(|k| contains_phrase(&words, k)) {
            out.push((t.id.clone(), "question"));
        }
    }
    out
}

/// Words too common to match a ruling title on.
const STOP: &[&str] = &[
    "nurse", "nursing", "practice", "scope", "board", "role", "licensed", "registered", "advanced",
    "can", "may", "does", "what", "when", "which", "with", "from", "that", "this", "under", "give",
    "have", "should", "would", "about", "into", "their", "there", "must",
];

fn title_matches(title: &str, query_words: &[String]) -> bool {
    stem_words(title).iter().any(|w| {
        w.chars().count() > 3
            && !STOP.contains(&w.as_str())
            && query_words.iter().any(|q| q == w)
    })
}

// ---------------------------------------------------------------- the tool

/// Run the guide.
pub fn nursing_scope_of_practice_guide(
    _ctx: &Context,
    args: ScopeArgs,
) -> Result<Envelope<ScopeResult>> {
    let c: ScopeContent = content::load(SCOPE)?;
    let question = optional_text("question", &args.question, MAX_QUESTION)?;
    let topic_args: Vec<String> = args
        .topics
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    for t in &topic_args {
        if t.chars().count() > 80 {
            return Err(PhnError::InvalidInput("a topic is too long".into()));
        }
    }
    let max = args
        .max_results
        .map(|m| (m as usize).clamp(1, MAX_ENTRIES))
        .unwrap_or(DEFAULT_ENTRIES);
    let role = args.role.and_then(Role::key);

    let asked = question.is_some() || !topic_args.is_empty();
    let matched = match_topics(&c, question.as_deref().unwrap_or(""), &topic_args);
    let matched_ids: BTreeSet<&str> = matched.iter().map(|(id, _)| id.as_str()).collect();

    // Provisions: those on a matched topic (and role), most matched topics first.
    let mut scored: Vec<(usize, usize, &EntryDef)> = c
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| role.is_none_or(|r| e.applies_to.iter().any(|a| a == r)))
        .filter_map(|(i, e)| {
            let n = e.topics.iter().filter(|t| matched_ids.contains(t.as_str())).count();
            (n > 0).then_some((n, i, e))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let total = scored.len();
    let provisions: Vec<EntryOut> = scored
        .iter()
        .take(max)
        .map(|(_, _, e)| EntryOut {
            id: e.id.clone(),
            citation: e.citation.clone(),
            source: source_label(&c, &e.source),
            url: source_url(&c, &e.source),
            applies_to: e.applies_to.clone(),
            quote: e.quote.clone(),
            paraphrase: e.paraphrase.clone(),
            confidence: e.confidence,
            note: e.note.clone(),
        })
        .collect();

    // Advisory rulings: those cited by the provisions shown, listed ones whose titles match the
    // question, or all of them.
    let words = stem_words(question.as_deref().unwrap_or(""));
    let cited_urls: BTreeSet<String> = provisions.iter().map(|p| p.url.clone()).collect();
    let all = args.list_all_rulings.unwrap_or(false);
    let rulings: Vec<RulingOut> = c
        .advisory_rulings
        .iter()
        .filter(|r| all || cited_urls.contains(&r.url) || (!r.read && title_matches(&r.title, &words)))
        .map(|r| RulingOut {
            number: r.number.clone(),
            title: r.title.clone(),
            area: r.area.clone(),
            issued: r.issued.clone(),
            last_revised: r.last_revised.clone(),
            scope: r.scope.clone(),
            url: r.url.clone(),
            read: r.read,
        })
        .collect();

    let topic_label = |id: &str| {
        c.topics
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.label.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let matched_topics: Vec<TopicOut> = matched
        .iter()
        .map(|(id, how)| TopicOut {
            id: id.clone(),
            label: topic_label(id),
            matched_by: (*how).into(),
        })
        .collect();
    let topic_index: Vec<TopicOut> = if provisions.is_empty() {
        c.topics
            .iter()
            .map(|t| TopicOut {
                id: t.id.clone(),
                label: t.label.clone(),
                matched_by: "index".into(),
            })
            .collect()
    } else {
        Vec::new()
    };

    let g = &c.decision_guidelines;
    let guide = DecisionGuide {
        intro: flatten(&g.intro.text),
        questions: g
            .questions
            .iter()
            .map(|q| DecisionQuestion {
                number: q.number,
                question: q.question.clone(),
                if_yes: q.if_yes.clone(),
                if_no: q.if_no.clone(),
                citation: format!("{}, {}", source_label(&c, &q.source), q.section),
                confidence: q.confidence,
            })
            .collect(),
        framework_steps: g.framework_steps.clone(),
        framework_note: flatten(&g.framework_note),
    };

    let rs = &c.read_scope;
    let coverage = Coverage {
        partial: true,
        snapshot_date: c.referral.retrieved.clone(),
        read_in_full: rs.read_in_full.clone(),
        read_in_part: rs.read_in_part.clone(),
        not_read: rs.not_read.clone(),
        rulings_listed: c.advisory_rulings.len(),
        rulings_read: c.advisory_rulings.iter().filter(|r| r.read).count(),
    };
    let referral = Referral {
        text: flatten(&c.referral.text),
        contacts: c.referral.contacts.clone(),
    };

    let summary = summary_text(
        &c, &question, asked, &matched_topics, &provisions, total, &guide, &rulings, &topic_index, &coverage, &referral,
    );

    let mut keys: Vec<&str> = vec!["born_scope_page", "born_laws_page", "born_ar_list"];
    for e in scored.iter().take(max) {
        keys.push(e.2.source.as_str());
    }
    let sources = content::sources_for(keys, &c.sources);

    let mut caveats = vec![
        "Decision support, not legal advice. This guide shows provisions and the Board's decision \
         questions; it does not say that any specific act is within a nurse's scope of practice. \
         The Board of Registration in Nursing decides questions about its own rules."
            .to_string(),
        format!(
            "PARTIAL snapshot read on {}: {} of the Board's {} advisory rulings and parts of 244 CMR \
             were read; see coverage.",
            coverage.snapshot_date, coverage.rulings_read, coverage.rulings_listed
        ),
        "The mass.gov copies of 244 CMR are not the official version; the Secretary of the \
         Commonwealth publishes the official text."
            .to_string(),
    ];
    if asked && provisions.is_empty() {
        caveats.push(
            "No provision in the snapshot matched your question or topics. That does not mean no \
             rule applies; the sources not read may cover it. Use the topic index, the decision \
             questions, and the Board."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[SCOPE]));

    Ok(Envelope::new(
        summary,
        ScopeResult {
            answer_type: "citations_and_decision_questions".into(),
            matched_topics,
            provisions,
            more_provisions: total.saturating_sub(max),
            decision_guide: guide,
            advisory_rulings: rulings,
            topic_index,
            coverage,
            refer_to_board: referral,
            limits: c.limits.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        notes: vec!["No local files are read and nothing is written.".into()],
        ..Privacy::none()
    })
    .decision_support())
}

#[allow(clippy::too_many_arguments)]
fn summary_text(
    _c: &ScopeContent,
    question: &Option<String>,
    asked: bool,
    matched: &[TopicOut],
    provisions: &[EntryOut],
    total: usize,
    guide: &DecisionGuide,
    rulings: &[RulingOut],
    index: &[TopicOut],
    coverage: &Coverage,
    referral: &Referral,
) -> String {
    let mut s = String::from("**Massachusetts nursing scope of practice: provisions and decision questions (partial snapshot)**\n\n");
    s.push_str(
        "This guide cites the rules and the Board's guidance; it does not say that a particular \
         act is within anyone's scope of practice. The Board decides that, and the answer depends \
         on your education, competence, orders or protocols, setting, and the facts.\n\n",
    );
    if let Some(q) = question {
        s.push_str(&format!("Your question: {q}\n\n"));
    }
    if !matched.is_empty() {
        s.push_str(&format!(
            "Topics matched: {}.\n\n",
            matched.iter().map(|t| t.label.as_str()).collect::<Vec<_>>().join("; ")
        ));
    }
    if provisions.is_empty() {
        if asked {
            s.push_str("No provision in the snapshot matched. Topic index:\n\n");
        } else {
            s.push_str("Topic index (ask about one, or give `topics`):\n\n");
        }
        for t in index {
            s.push_str(&format!("- `{}`: {}\n", t.id, t.label));
        }
        s.push('\n');
    } else {
        s.push_str("**Provisions**\n\n");
        for p in provisions {
            s.push_str(&format!(
                "- **{}** ({}) \"{}\" {} _[{}{}]_\n",
                p.citation,
                p.applies_to.join(", ").to_uppercase(),
                p.quote,
                p.paraphrase,
                p.source,
                match p.confidence {
                    Confidence::High => "",
                    Confidence::Medium => "; medium confidence",
                    Confidence::Low => "; LOW confidence",
                }
            ));
            if let Some(n) = &p.note {
                s.push_str(&format!("  - Note: {n}\n"));
            }
        }
        if total > provisions.len() {
            s.push_str(&format!(
                "\n{} more provision(s) matched; raise max_results to see them.\n",
                total - provisions.len()
            ));
        }
        s.push('\n');
    }
    s.push_str("**The Board's decision making guidelines** (describe the activity, check for a statute or regulation that specifically permits or prohibits it, then answer):\n\n");
    for q in &guide.questions {
        s.push_str(&format!("{}. {}\n", q.number, q.question));
    }
    s.push('\n');
    if !rulings.is_empty() {
        let rows: Vec<Vec<String>> = rulings
            .iter()
            .map(|r| {
                vec![
                    r.number.clone(),
                    r.title.clone(),
                    r.last_revised
                        .clone()
                        .or_else(|| r.issued.clone())
                        .unwrap_or_else(|| "date not read".into()),
                    if r.read { "read" } else { "title only" }.into(),
                ]
            })
            .collect();
        s.push_str("**Advisory rulings**\n\n");
        s.push_str(&md::table(&["Ruling", "Title", "Date", "Status"], &rows));
        s.push('\n');
    }
    s.push_str(&format!(
        "**Coverage:** partial snapshot of {}; {} of {} listed advisory rulings were read. Not \
         read: {}.\n\n",
        coverage.snapshot_date,
        coverage.rulings_read,
        coverage.rulings_listed,
        coverage.not_read.first().cloned().unwrap_or_default()
    ));
    s.push_str(&format!("**Ask the Board:** {} {}\n", referral.text, referral.contacts.join("; ")));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> ScopeContent {
        content::load(SCOPE).unwrap()
    }

    #[test]
    fn content_is_consistent_and_quotes_are_short() {
        let c = content();
        let topics: BTreeSet<&str> = c.topics.iter().map(|t| t.id.as_str()).collect();
        let ids: BTreeSet<&str> = c.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), c.entries.len(), "duplicate entry ids");
        for e in &c.entries {
            assert!(c.sources.contains_key(&e.source), "{} -> {}", e.id, e.source);
            assert!(e.quote.split_whitespace().count() <= 25, "{} quote too long", e.id);
            assert!(!e.quote.trim().is_empty() && !e.paraphrase.trim().is_empty());
            for t in &e.topics {
                assert!(topics.contains(t.as_str()), "{} -> topic {t}", e.id);
            }
            for r in &e.applies_to {
                assert!(["rn", "lpn", "aprn"].contains(&r.as_str()), "{} role {r}", e.id);
            }
        }
        for t in &c.topics {
            assert!(
                c.entries.iter().any(|e| e.topics.contains(&t.id)),
                "topic {} has no provisions",
                t.id
            );
        }
        assert_eq!(c.decision_guidelines.questions.len(), 6);
        assert_eq!(c.decision_guidelines.framework_steps.len(), 8);
        // Every read ruling is cited by at least one provision, by URL.
        for r in c.advisory_rulings.iter().filter(|r| r.read) {
            assert!(
                c.sources.values().any(|s| s.url == r.url),
                "read ruling {} has no source entry",
                r.number
            );
        }
    }

    #[test]
    fn questions_map_to_topics() {
        let c = content();
        let ids = |q: &str| -> Vec<String> {
            match_topics(&c, q, &[]).into_iter().map(|(i, _)| i).collect()
        };
        assert!(ids("Can an LPN give flu vaccines under a standing order?").contains(&"vaccines".to_string()));
        assert!(ids("Can an LPN give flu vaccines under a standing order?").contains(&"standing_orders".to_string()));
        assert!(ids("What must I do before delegating to an unlicensed health aide?").contains(&"delegation".to_string()));
        assert!(ids("naloxone kits at a syringe services program").contains(&"harm_reduction".to_string()));
        assert!(ids("Is a QI project research?").contains(&"research".to_string()));
        assert!(ids("zebra giraffe").is_empty());
        // Topic arguments match by id and by label.
        let by_arg = match_topics(&c, "", &["standing_orders".to_string(), "Immunizations and vaccines".to_string()]);
        assert_eq!(by_arg.len(), 2, "{by_arg:?}");
        assert!(by_arg.iter().all(|(_, how)| *how == "topic argument"));
    }

    #[test]
    fn ruling_titles_match_on_content_words_only() {
        let q = stem_words("Can a nurse do infusion therapy?");
        assert!(title_matches("Infusion Therapy", &q));
        assert!(!title_matches("Management of Pain", &q));
        let q = stem_words("what is the nursing scope of practice");
        assert!(!title_matches("RN and LPN Scope of Practice", &q));
    }
}
