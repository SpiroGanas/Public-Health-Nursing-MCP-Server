//! SCH-03 `school_medication_administration_guide`.
//!
//! Answers questions about medication administration in Massachusetts
//! schools from 105 CMR 210.000 (as amended 12/5/2025) and M.G.L. c. 71,
//! section 54B: orders and consent, delegation to unlicensed staff, field
//! trips, self-carry, storage, documentation, errors, epinephrine, naloxone,
//! other rescue medications, and DPH registration. Every point is cited by
//! section.

use std::collections::{BTreeMap, BTreeSet};

use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::school::content::{self, CitedOut, MEDS, MedTopic, confidence_suffix, outs};

/// Arguments for `school_medication_administration_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct MedicationGuideArgs {
    /// The question in plain language, e.g. "Can a teacher give an EpiPen on a
    /// field trip?" or "How long can we keep medication at school?".
    pub question: Option<String>,
    /// A topic id or name instead of a question: scope_and_definitions,
    /// policies, registration, delegation, orders_and_consent,
    /// medication_plan, field_trips, self_administration, training, storage,
    /// documentation, medication_errors, epinephrine, opioid_antagonist,
    /// emergency_rescue_medications, nurse_judgment. Omit both to list topics.
    pub topic: Option<String>,
    /// Most topics to return for a question (default 4, at most 6).
    pub max_topics: Option<usize>,
}

/// One topic with its cited points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MedTopicOut {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub points: Vec<CitedOut>,
}

/// A topic in the index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TopicIndexEntry {
    pub id: String,
    pub title: String,
}

/// Result of `school_medication_administration_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MedicationGuideResult {
    pub question: Option<String>,
    pub topics: Vec<MedTopicOut>,
    /// All topics (shown when nothing matched or nothing was asked).
    pub index: Vec<TopicIndexEntry>,
    pub regulation: String,
}

/// How many topics list each keyword (shared keywords count for less).
fn keyword_counts() -> BTreeMap<String, usize> {
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    for t in &MEDS.topics {
        let mut seen = BTreeSet::new();
        for k in &t.keywords {
            let kn = phn_core::text::normalize(k);
            if seen.insert(kn.clone()) {
                *m.entry(kn).or_default() += 1;
            }
        }
    }
    m
}

fn score(t: &MedTopic, q: &str, counts: &BTreeMap<String, usize>) -> f64 {
    let mut s = 0.0;
    let words: Vec<&str> = q.split_whitespace().collect();
    for k in &t.keywords {
        let kn = phn_core::text::normalize(k);
        if kn.is_empty() {
            continue;
        }
        let weight = 1.0 / counts.get(&kn).copied().unwrap_or(1).max(1) as f64;
        if kn.contains(' ') {
            if q.contains(&kn) {
                s += 3.0 * weight;
            }
        } else if words
            .iter()
            .any(|w| *w == kn || (kn.len() >= 5 && w.starts_with(&kn)))
        {
            s += 2.0 * weight;
        }
    }
    let title = phn_core::text::normalize(&t.title);
    for w in title.split_whitespace().filter(|w| w.len() >= 5) {
        if words.contains(&w) {
            s += 1.0;
        }
    }
    s
}

fn find_topic(q: &str) -> Option<&'static MedTopic> {
    let n = phn_core::text::normalize(q).replace(' ', "_");
    let topics = &MEDS.topics;
    topics
        .iter()
        .find(|t| t.id == n || phn_core::text::normalize(&t.title).replace(' ', "_") == n)
        .or_else(|| {
            topics
                .iter()
                .map(|t| {
                    (
                        t,
                        phn_core::text::similarity(
                            &phn_core::text::normalize(q),
                            &phn_core::text::normalize(&t.title),
                        )
                        .max(phn_core::text::similarity(&n, &t.id)),
                    )
                })
                .filter(|(_, s)| *s >= 0.6)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(t, _)| t)
        })
}

/// Answer a question about medication administration in schools.
pub fn school_medication_administration_guide(
    _ctx: &Context,
    args: MedicationGuideArgs,
) -> Result<Envelope<MedicationGuideResult>> {
    let c = &*MEDS;
    let src = &c.sources;
    let max = args.max_topics.unwrap_or(4).clamp(1, 6);
    let index: Vec<TopicIndexEntry> = c
        .topics
        .iter()
        .map(|t| TopicIndexEntry {
            id: t.id.clone(),
            title: t.title.clone(),
        })
        .collect();
    let mut chosen: Vec<&MedTopic> = Vec::new();
    if let Some(t) = args
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let found = find_topic(t).ok_or_else(|| {
            PhnError::NotFound(format!(
                "no topic matches '{t}'. Topics: {}",
                c.topics
                    .iter()
                    .map(|x| x.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        chosen.push(found);
    }
    let question = args
        .question
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(String::from);
    if let Some(q) = &question {
        let qn = phn_core::text::normalize(q);
        let counts = keyword_counts();
        let mut scored: Vec<(&MedTopic, f64)> = c
            .topics
            .iter()
            .map(|t| (t, score(t, &qn, &counts)))
            .filter(|(_, s)| *s >= 1.0)
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (t, _) in scored.into_iter().take(max) {
            if !chosen.iter().any(|x| x.id == t.id) {
                chosen.push(t);
            }
        }
    }
    let topics: Vec<MedTopicOut> = chosen
        .iter()
        .map(|t| MedTopicOut {
            id: t.id.clone(),
            title: t.title.clone(),
            summary: t.summary.trim().to_string(),
            points: outs(&t.points, src),
        })
        .collect();

    // ---- Markdown
    let mut s = String::from(
        "**Medication administration in Massachusetts schools (105 CMR 210.000, amended 12/5/2025)**\n\n",
    );
    if let Some(q) = &question {
        s.push_str(&format!("Question: _{}_\n\n", q.replace('_', " ")));
    }
    if topics.is_empty() {
        if question.is_some() {
            s.push_str("No topic matched the question closely. Ask about one of these topics:\n\n");
        } else {
            s.push_str("Topics covered (ask a question or pick a topic):\n\n");
        }
        for t in &index {
            s.push_str(&format!("- `{}` — {}\n", t.id, t.title));
        }
    }
    for t in &topics {
        s.push_str(&format!("**{}**\n\n{}\n\n", t.title, t.summary));
        for p in &t.points {
            s.push_str(&format!(
                "- {} ({}){}\n",
                p.text,
                p.citation,
                confidence_suffix(p.confidence)
            ));
        }
        s.push('\n');
    }
    if !topics.is_empty() {
        let others: Vec<&str> = index
            .iter()
            .filter(|i| !topics.iter().any(|t| t.id == i.id))
            .map(|i| i.id.as_str())
            .collect();
        s.push_str(&format!("Other topics: {}\n", others.join(", ")));
    }
    let mut keys: Vec<&str> = vec!["cmr210"];
    for t in &chosen {
        keys.extend(t.points.iter().map(|p| p.source.as_str()));
    }
    let mut caveats = content::review(&[content::MEDICATION]);
    caveats.push(
        "105 CMR 210.000 sets minimum standards; your school committee's medication policy, the \
         Board of Registration in Nursing's rules (244 CMR), and your Medication Program Manager \
         govern local practice. EEC-licensed child care follows 606 CMR 7.11 instead."
            .into(),
    );
    caveats.push(
        "Board of Registration in Nursing advisory rulings (AR 92-05 on over-the-counter drugs, AR \
         9324 on medication orders) are cited by name only; read the current rulings."
            .into(),
    );
    Ok(Envelope::new(
        s,
        MedicationGuideResult {
            question,
            topics,
            index,
            regulation: "105 CMR 210.000 (Mass. Register #1562, 12/5/2025)".into(),
        },
    )
    .sources(content::sources_for(src, keys))
    .caveats(caveats)
    .privacy(Privacy::none())
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn ask(q: &str) -> Vec<String> {
        let (_d, c) = ctx();
        school_medication_administration_guide(
            &c,
            MedicationGuideArgs {
                question: Some(q.into()),
                ..Default::default()
            },
        )
        .unwrap()
        .data
        .topics
        .into_iter()
        .map(|t| t.id)
        .collect()
    }

    #[test]
    fn questions_route_to_the_right_sections() {
        assert!(
            ask("Can a teacher give an EpiPen on a field trip?")
                .contains(&"epinephrine".to_string())
        );
        assert!(
            ask("can a teacher give an epipen on a field trip")
                .contains(&"field_trips".to_string())
        );
        assert_eq!(
            ask("How long can we store medication? Refrigerator temperature?")[0],
            "storage"
        );
        assert!(ask("Who can give Narcan at school?").contains(&"opioid_antagonist".to_string()));
        assert!(
            ask("does a verbal order need to be followed up in writing")
                .contains(&"orders_and_consent".to_string())
        );
        assert!(
            ask("can the secretary give medications under delegation")
                .contains(&"delegation".to_string())
        );
        assert!(
            ask("student wants to self carry an inhaler")
                .contains(&"self_administration".to_string())
        );
    }

    #[test]
    fn topics_are_cited_and_index_is_returned() {
        let (_d, c) = ctx();
        let e = school_medication_administration_guide(
            &c,
            MedicationGuideArgs {
                topic: Some("medication_errors".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(e.data.topics[0].id, "medication_errors");
        assert!(
            e.data.topics[0]
                .points
                .iter()
                .any(|p| p.citation.contains("210.009(B)"))
        );
        assert!(
            e.data.topics[0]
                .points
                .iter()
                .all(|p| !p.citation.is_empty())
        );
        assert!(e.notice.is_some());
        let e = school_medication_administration_guide(&c, MedicationGuideArgs::default()).unwrap();
        assert!(e.data.topics.is_empty() && e.data.index.len() >= 15);
        assert!(
            school_medication_administration_guide(
                &c,
                MedicationGuideArgs {
                    topic: Some("zzzz".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
