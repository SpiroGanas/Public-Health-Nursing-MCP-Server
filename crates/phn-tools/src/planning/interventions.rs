//! PLAN-03 `find_evidence_based_interventions`: finds Community Guide systematic reviews,
//! economic reviews, and Task Force recommendation statements for a health topic, with the
//! evidence wording each paper's abstract uses, the source and date, and any implementation
//! notes the abstract gives.
//!
//! * The records come from the bundled `cpstf-publications` snapshot (the journal papers PubMed
//!   attributes to the Community Preventive Services Task Force, read through PubMed by
//!   `xtask/scripts/build_interventions_snapshots.py`). The Community Guide and What Works for
//!   Health sites could not be reached from the build network, so neither is bundled and the
//!   tool says so and tells the nurse where to check.
//! * Newer papers are looked up live in PubMed when the server is online; on any failure the
//!   snapshot answer stands alone and says so.
//! * A rating is shown only when the paper's own wording gives it; the tool never infers one.
//!
//! The inputs are a topic, population, and setting. Nothing is read from local files.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::cpstf::{self, CpstfRecord, RatingTerm, RecordKind, TitleSignal};
use phn_data::eutils::Eutils;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::textmatch::{contains_phrase_stemmed, stems};
use crate::context::Context;
use crate::epr::content;
use crate::md;

/// Content id of `content/planning/evidence_sources.yaml`.
pub const EVIDENCE_SOURCES: &str = "planning-evidence-sources";

const DEFAULT_RESULTS: u32 = 8;
const MAX_RESULTS: u32 = 20;
const MAX_TOPIC: usize = 200;
/// Papers older than this many years get a note to check for updates.
const OLD_YEARS: i32 = 8;
/// A paper must score at least this on the topic words to be listed.
const MIN_SCORE: u32 = 2;
const LIVE_SEARCH_MAX: u32 = 20;
const LIVE_FETCH_MAX: usize = 12;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct RatingKey {
    term: RatingTerm,
    label: String,
    meaning: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NotRead {
    #[allow(dead_code)]
    id: String,
    name: String,
    url: String,
    reason: String,
    how_to_check: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Setting {
    #[allow(dead_code)]
    id: String,
    terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Topic {
    #[allow(dead_code)]
    id: String,
    label: String,
    terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EvidenceContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    rating_key: Vec<RatingKey>,
    not_read: Vec<NotRead>,
    settings: Vec<Setting>,
    topics: Vec<Topic>,
}

// ---------------------------------------------------------------- arguments and results

/// Which papers to keep by the evidence wording in the abstract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFilter {
    /// Every paper that matches the topic (default).
    #[default]
    Any,
    /// Papers whose abstract uses "strong evidence", "sufficient evidence", or says the Task
    /// Force recommends.
    StrongOrRecommended,
    /// Papers whose abstract says there is insufficient evidence (useful to see what is not yet
    /// established).
    Insufficient,
}

/// Arguments for `find_evidence_based_interventions`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct EvidenceArgs {
    /// The health topic or problem, for example "childhood obesity", "tobacco cessation",
    /// "colorectal cancer screening", or "falls". Required.
    pub topic: String,
    /// Population, for example "adolescents", "older adults", or "low-income families".
    /// Optional; used to rank papers, not to exclude them.
    pub population: Option<String>,
    /// Setting, for example "school", "worksite", "health care", "community", or "home".
    /// Optional; used to rank papers.
    pub setting: Option<String>,
    /// Keep only these kinds of paper: recommendation, systematic_review, economic_review.
    /// Default: all.
    pub kinds: Option<Vec<RecordKind>>,
    /// Keep papers by the evidence wording in their abstract: any (default),
    /// strong_or_recommended, or insufficient.
    pub evidence: Option<EvidenceFilter>,
    /// How many papers to list (default 8, at most 20).
    pub max_results: Option<u32>,
    /// Also search PubMed live for papers newer than the snapshot (default true; skipped when
    /// the server is offline). Only the topic words are sent.
    pub include_live: Option<bool>,
}

/// One paper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceMatch {
    pub pmid: String,
    pub title: String,
    pub journal: Option<String>,
    /// Journal publication date (YYYY or YYYY-MM).
    pub published: String,
    /// What kind of Community Guide paper this is.
    pub kind: String,
    /// The evidence wording in the abstract (or title), in the paper's own terms, or a note that
    /// the PubMed record does not give one. Never inferred.
    pub evidence_rating: String,
    /// Rating words found in the abstract.
    pub rating_terms: Vec<RatingTerm>,
    /// "abstract wording", "title wording", or "not in the PubMed record".
    pub rating_basis: String,
    /// Short quotations from the abstract that carry the rating words.
    pub rating_quotes: Vec<String>,
    /// Short excerpt of the abstract's results.
    pub finding: Option<String>,
    /// Short excerpt of the abstract's conclusion.
    pub conclusion: Option<String>,
    /// What the abstract says about implementation, barriers, or cost, when it says anything.
    pub implementation_notes: Option<String>,
    /// Topic words that matched this paper's title, MeSH headings, or abstract excerpts.
    pub matched_on: Vec<String>,
    /// "snapshot" or "live" (a PubMed record newer than the snapshot).
    pub source_mode: String,
    /// A reminder to check for updates when the paper is old.
    pub update_check: Option<String>,
    pub pubmed_url: String,
    pub doi_url: Option<String>,
}

/// What the snapshot is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotInfo {
    /// Date the snapshot was built from PubMed.
    pub retrieved: String,
    pub records: usize,
    pub not_included: Vec<String>,
}

/// What the live PubMed search did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LiveInfo {
    pub attempted: bool,
    /// What happened, in plain words.
    pub status: String,
    /// Papers PubMed returned that are not in the snapshot.
    pub new_papers: usize,
}

/// A rating word and what it means.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RatingKeyOut {
    pub term: RatingTerm,
    pub label: String,
    pub meaning: String,
}

/// A source the tool could not read, and how to check it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NotReadOut {
    pub name: String,
    pub url: String,
    pub reason: String,
    pub how_to_check: String,
}

/// Result of `find_evidence_based_interventions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceResult {
    pub topic: String,
    pub population: Option<String>,
    pub setting: Option<String>,
    /// Topic groups the tool recognized in your words (their synonyms were searched too).
    pub topics_recognized: Vec<String>,
    pub matches: Vec<EvidenceMatch>,
    /// How many papers matched before the limit.
    pub matched_total: usize,
    pub snapshot: SnapshotInfo,
    pub live: LiveInfo,
    pub rating_key: Vec<RatingKeyOut>,
    /// What to check outside this tool: the Community Guide and What Works for Health.
    pub not_read: Vec<NotReadOut>,
}

// ---------------------------------------------------------------- matching

struct Query {
    /// Stemmed words of the topic and the synonyms of recognized topics.
    topic_stems: BTreeSet<String>,
    /// Phrases for the live search.
    phrases: Vec<String>,
    topics_recognized: Vec<String>,
    boost_stems: BTreeSet<String>,
}

fn clean_text(label: &str, s: &str, max: usize) -> Result<String> {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > max {
        return Err(PhnError::InvalidInput(format!(
            "{label} is too long (at most {max} characters)"
        )));
    }
    Ok(t)
}

fn build_query(
    c: &EvidenceContent,
    topic: &str,
    population: Option<&str>,
    setting: Option<&str>,
) -> Query {
    let mut topic_stems = stems(topic);
    let mut phrases = vec![topic.to_string()];
    let mut recognized = Vec::new();
    for t in &c.topics {
        let hit = t.terms.iter().any(|x| contains_phrase_stemmed(topic, x))
            || contains_phrase_stemmed(topic, &t.label);
        if hit {
            recognized.push(t.label.clone());
            for term in &t.terms {
                topic_stems.extend(stems(term));
            }
            for term in t.terms.iter().take(6) {
                if !phrases.iter().any(|p| p.eq_ignore_ascii_case(term)) {
                    phrases.push(term.clone());
                }
            }
        }
    }
    let mut boost = BTreeSet::new();
    if let Some(p) = population {
        boost.extend(stems(p));
    }
    if let Some(s) = setting {
        boost.extend(stems(s));
        for st in &c.settings {
            if st.terms.iter().any(|x| contains_phrase_stemmed(s, x)) {
                for x in &st.terms {
                    boost.extend(stems(x));
                }
            }
        }
    }
    // Topic words are never also boosts.
    boost.retain(|b| !topic_stems.contains(b));
    Query {
        topic_stems,
        phrases,
        topics_recognized: recognized,
        boost_stems: boost,
    }
}

/// Score a paper and say which topic words matched.
fn score(q: &Query, title: &str, mesh: &[String], text: &str) -> (u32, Vec<String>, u32) {
    let title_s = stems(title);
    let mesh_s = stems(&mesh.join(" "));
    let text_s = stems(text);
    let mut base = 0u32;
    let mut matched = Vec::new();
    for p in &q.topic_stems {
        let pts = if title_s.contains(p) {
            3
        } else if mesh_s.contains(p) {
            2
        } else if text_s.contains(p) {
            1
        } else {
            0
        };
        if pts > 0 {
            base += pts;
            matched.push(p.clone());
        }
    }
    let mut boost = 0u32;
    for b in &q.boost_stems {
        if title_s.contains(b) || mesh_s.contains(b) || text_s.contains(b) {
            boost += 1;
        }
    }
    (base, matched, boost)
}

fn rating_label(r: &CpstfRecord) -> (String, String) {
    rating_label_of(&r.rating_terms, r.title_signal, r.has_abstract)
}

fn rating_label_of(
    terms: &[RatingTerm],
    title_signal: Option<TitleSignal>,
    has_abstract: bool,
) -> (String, String) {
    if !terms.is_empty() {
        let mut parts: Vec<&str> = Vec::new();
        for t in [
            RatingTerm::RecommendedAgainst,
            RatingTerm::StrongEvidence,
            RatingTerm::SufficientEvidence,
            RatingTerm::InsufficientEvidence,
        ] {
            if terms.contains(&t) {
                parts.push(t.label());
            }
        }
        if parts.is_empty() {
            parts.push(RatingTerm::Recommended.label());
        }
        let mixed = parts.len() > 1;
        let mut s = parts.join("; ");
        if mixed {
            s.push_str(" (the abstract gives different wording for different interventions)");
        }
        return (s, "abstract wording".into());
    }
    match title_signal {
        Some(TitleSignal::Recommended) => (
            "The title says the intervention is recommended; the PubMed record has no abstract \
             wording on the strength of evidence"
                .into(),
            "title wording".into(),
        ),
        Some(TitleSignal::RecommendedAgainst) => (
            "The title says the Task Force recommends against it; the PubMed record has no \
             abstract wording on the strength of evidence"
                .into(),
            "title wording".into(),
        ),
        None => (
            if has_abstract {
                "The abstract does not state a rating; check the Community Guide page".into()
            } else {
                "The PubMed record has no abstract and the title gives no rating; check the \
                 Community Guide page"
                    .into()
            },
            "not in the PubMed record".into(),
        ),
    }
}

fn passes_evidence(terms: &[RatingTerm], f: EvidenceFilter) -> bool {
    match f {
        EvidenceFilter::Any => true,
        EvidenceFilter::StrongOrRecommended => terms.iter().any(|t| {
            matches!(
                t,
                RatingTerm::StrongEvidence | RatingTerm::SufficientEvidence | RatingTerm::Recommended
            )
        }),
        EvidenceFilter::Insufficient => terms.contains(&RatingTerm::InsufficientEvidence),
    }
}

fn update_note(published: &str, this_year: i32) -> Option<String> {
    let y: i32 = published.get(..4)?.parse().ok()?;
    let age = this_year - y;
    (age >= OLD_YEARS).then(|| {
        format!(
            "Published {age} years ago. The Task Force may have updated or archived this \
             recommendation since; check its Community Guide page."
        )
    })
}

fn snapshot_match(
    r: &CpstfRecord,
    matched_on: Vec<String>,
    this_year: i32,
    live: bool,
) -> EvidenceMatch {
    let (rating, basis) = rating_label(r);
    EvidenceMatch {
        pmid: r.pmid.clone(),
        title: r.title.clone(),
        journal: r.journal.clone(),
        published: r.published.clone(),
        kind: r.kind.label().to_string(),
        evidence_rating: rating,
        rating_terms: r.rating_terms.clone(),
        rating_basis: basis,
        rating_quotes: r.rating_sentences.clone(),
        finding: r.finding.clone(),
        conclusion: r.conclusion.clone(),
        implementation_notes: r.implementation.clone(),
        matched_on,
        source_mode: if live { "live" } else { "snapshot" }.into(),
        update_check: update_note(&r.published, this_year),
        pubmed_url: r.pubmed_url(),
        doi_url: r.doi_url(),
    }
}

/// The conclusion of an unstructured-label abstract ("CONCLUSIONS: ...").
fn conclusion_of(abstract_text: &str) -> Option<String> {
    let upper = abstract_text.to_uppercase();
    let at = ["CONCLUSIONS: ", "CONCLUSION: "]
        .iter()
        .find_map(|k| upper.find(k).map(|p| (p, k.len())))?;
    let after = &abstract_text[at.0 + at.1..];
    // The text runs to the next all-caps label ("WORD: ") or the end.
    let mut end = after.len();
    let bytes: Vec<(usize, char)> = after.char_indices().collect();
    for (i, (_, ch)) in bytes.iter().enumerate() {
        if *ch == ':' && i >= 4 {
            let label_start = bytes[..i]
                .iter()
                .rposition(|(_, c)| *c == ' ')
                .map_or(0, |p| p + 1);
            let label: String = bytes[label_start..i].iter().map(|(_, c)| *c).collect();
            if label.len() >= 3 && label.chars().all(|c| c.is_uppercase()) {
                end = bytes[label_start].0;
                break;
            }
        }
    }
    let text = after[..end].trim();
    if text.is_empty() {
        return None;
    }
    let mut out = String::new();
    for s in cpstf::sentences(text) {
        if out.chars().count() + s.chars().count() > 380 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&s);
    }
    (!out.is_empty()).then_some(out)
}

/// Turn a live PubMed article into a record (None when it is not a review or recommendation).
fn live_record(a: &phn_data::eutils::Article) -> Option<CpstfRecord> {
    let kind = cpstf::kind_of(&a.title)?;
    let year = a.year.as_ref()?;
    let y: i32 = year.parse().ok()?;
    if !(1900..=2100).contains(&y) {
        return None;
    }
    let (terms, sentences) = cpstf::rating_terms(&a.abstract_text);
    Some(CpstfRecord {
        pmid: a.pmid.clone(),
        title: a.title.clone(),
        journal: Some(a.journal_abbreviation.clone()).filter(|j| !j.is_empty()),
        published: year.clone(),
        doi: a.doi.clone(),
        kind,
        title_signal: None,
        has_abstract: !a.abstract_text.is_empty(),
        mesh: Vec::new(),
        rating_terms: terms,
        rating_sentences: sentences
            .into_iter()
            .map(|s| s.chars().take(380).collect())
            .collect(),
        finding: None,
        conclusion: conclusion_of(&a.abstract_text),
        implementation: None,
    })
}

// ---------------------------------------------------------------- the tool

/// Find evidence-based interventions.
pub async fn find_evidence_based_interventions(
    ctx: &Context,
    args: EvidenceArgs,
) -> Result<Envelope<EvidenceResult>> {
    let topic = clean_text("topic", &args.topic, MAX_TOPIC)?;
    if topic.is_empty() {
        return Err(PhnError::InvalidInput(
            "topic is required: a health topic or problem such as \"childhood obesity\" or \
             \"colorectal cancer screening\""
                .into(),
        ));
    }
    let population = match &args.population {
        Some(p) if !p.trim().is_empty() => Some(clean_text("population", p, MAX_TOPIC)?),
        _ => None,
    };
    let setting = match &args.setting {
        Some(p) if !p.trim().is_empty() => Some(clean_text("setting", p, MAX_TOPIC)?),
        _ => None,
    };
    for (label, text) in [
        ("topic", Some(&topic)),
        ("population", population.as_ref()),
        ("setting", setting.as_ref()),
    ] {
        if let Some(t) = text
            && !phn_local::phi::scan_text(t).is_empty()
        {
            return Err(PhnError::Privacy(format!(
                "{label} appears to contain an identifier. Give a health topic, not information \
                 about a person."
            )));
        }
    }
    let max = args
        .max_results
        .unwrap_or(DEFAULT_RESULTS)
        .clamp(1, MAX_RESULTS) as usize;
    let filter = args.evidence.unwrap_or_default();
    let kinds: Option<BTreeSet<&'static str>> = args
        .kinds
        .as_ref()
        .filter(|k| !k.is_empty())
        .map(|k| k.iter().map(|x| x.label()).collect());

    let c: EvidenceContent = content::load(EVIDENCE_SOURCES)?;
    let (snap, entry) = cpstf::load_snapshot(&ctx.snapshots)?;
    let q = build_query(&c, &topic, population.as_deref(), setting.as_deref());
    let this_year = chrono::Utc::now().year();

    // Snapshot records.
    struct Hit {
        record: CpstfRecord,
        base: u32,
        boost: u32,
        matched: Vec<String>,
        live: bool,
    }
    let mut hits: Vec<Hit> = Vec::new();
    let known: BTreeSet<&str> = snap.records.iter().map(|r| r.pmid.as_str()).collect();
    for r in &snap.records {
        let text = [
            r.rating_sentences.join(" "),
            r.finding.clone().unwrap_or_default(),
            r.conclusion.clone().unwrap_or_default(),
        ]
        .join(" ");
        let (base, matched, boost) = score(&q, &r.title, &r.mesh, &text);
        if base >= MIN_SCORE {
            hits.push(Hit {
                record: r.clone(),
                base,
                boost,
                matched,
                live: false,
            });
        }
    }

    // Live PubMed search for papers newer than the snapshot.
    let mut live = LiveInfo {
        attempted: false,
        status: String::new(),
        new_papers: 0,
    };
    let mut live_sources: Vec<(String, SourceMode, String)> = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    if args.include_live.unwrap_or(true) {
        if ctx.http.is_offline() {
            live.status = "Not attempted: the server is offline (PHN_OFFLINE), so only the \
                           snapshot was searched."
                .into();
        } else {
            live.attempted = true;
            let query = cpstf::live_query(&q.phrases);
            let eu = Eutils::new();
            let outcome = async {
                let (found, prov, _) = eu
                    .search(&ctx.http, ctx.cache.as_ref(), &query, LIVE_SEARCH_MAX, "pub_date")
                    .await?;
                let new_ids: Vec<String> = found
                    .ids
                    .iter()
                    .filter(|i| !known.contains(i.as_str()))
                    .take(LIVE_FETCH_MAX)
                    .cloned()
                    .collect();
                let (arts, prov2) = eu.fetch(&ctx.http, ctx.cache.as_ref(), &new_ids).await?;
                Ok::<_, PhnError>((arts, prov, prov2))
            }
            .await;
            match outcome {
                Ok((arts, prov, _)) => {
                    let mut added = 0usize;
                    for a in &arts {
                        if let Some(rec) = live_record(a) {
                            let text = [
                                rec.rating_sentences.join(" "),
                                rec.conclusion.clone().unwrap_or_default(),
                                a.abstract_text.clone(),
                            ]
                            .join(" ");
                            let (base, matched, boost) = score(&q, &rec.title, &rec.mesh, &text);
                            if base >= MIN_SCORE {
                                live_sources.push((
                                    rec.pmid.clone(),
                                    prov.mode,
                                    prov.as_of.clone(),
                                ));
                                hits.push(Hit {
                                    record: rec,
                                    base,
                                    boost,
                                    matched,
                                    live: true,
                                });
                                added += 1;
                            }
                        }
                    }
                    live.new_papers = added;
                    live.status = format!(
                        "PubMed was searched ({}); {added} paper(s) not in the snapshot matched.",
                        prov.mode.label()
                    );
                }
                Err(e) => {
                    live.status = format!(
                        "PubMed could not be reached ({}); the snapshot answer stands alone.",
                        phn_data::apifetch::reason(&e)
                    );
                    caveats.push(live.status.clone());
                }
            }
        }
    } else {
        live.status = "Not attempted: include_live was false.".into();
    }

    // Filter, rank, and cut.
    hits.retain(|h| {
        passes_evidence(&h.record.rating_terms, filter)
            && kinds
                .as_ref()
                .is_none_or(|k| k.contains(h.record.kind.label()))
    });
    hits.sort_by(|a, b| {
        (b.base + b.boost)
            .cmp(&(a.base + a.boost))
            .then(b.record.published.cmp(&a.record.published))
            .then(a.record.pmid.cmp(&b.record.pmid))
    });
    let matched_total = hits.len();
    hits.truncate(max);
    let matches: Vec<EvidenceMatch> = hits
        .iter()
        .map(|h| snapshot_match(&h.record, h.matched.clone(), this_year, h.live))
        .collect();

    let snapshot = SnapshotInfo {
        retrieved: snap.retrieved.clone(),
        records: snap.records.len(),
        not_included: snap.not_included.clone(),
    };
    let rating_key: Vec<RatingKeyOut> = c
        .rating_key
        .iter()
        .map(|k| RatingKeyOut {
            term: k.term,
            label: k.label.clone(),
            meaning: k.meaning.clone(),
        })
        .collect();
    let not_read: Vec<NotReadOut> = c
        .not_read
        .iter()
        .map(|n| NotReadOut {
            name: n.name.clone(),
            url: n.url.clone(),
            reason: n.reason.clone(),
            how_to_check: n.how_to_check.clone(),
        })
        .collect();

    let result = EvidenceResult {
        topic: topic.clone(),
        population: population.clone(),
        setting: setting.clone(),
        topics_recognized: q.topics_recognized.clone(),
        matches,
        matched_total,
        snapshot,
        live,
        rating_key,
        not_read,
    };

    // Sources.
    let mut sources = vec![entry.to_source()];
    let live_by_pmid: BTreeMap<&str, (&SourceMode, &String)> = live_sources
        .iter()
        .map(|(p, m, d)| (p.as_str(), (m, d)))
        .collect();
    for m in &result.matches {
        let (mode, retrieved) = match live_by_pmid.get(m.pmid.as_str()) {
            Some((mode, as_of)) => (**mode, phn_data::fetch::local_date(as_of)),
            None => (SourceMode::Snapshot, snap.retrieved.clone()),
        };
        sources.push(Source {
            title: m.title.clone(),
            publisher: format!(
                "Community Preventive Services Task Force paper ({})",
                m.journal.as_deref().unwrap_or("journal not given")
            ),
            url: Some(m.doi_url.clone().unwrap_or_else(|| m.pubmed_url.clone())),
            effective_date: Some(m.published.clone()),
            retrieved: Some(retrieved),
            mode,
        });
    }

    // Caveats.
    caveats.push(
        "This is not the Community Guide's list of recommendations. It holds the journal papers \
         PubMed attributes to the Community Preventive Services Task Force, so a recommendation \
         posted only on thecommunityguide.org is missing, and no match does not mean no \
         recommendation exists. Check the Community Guide page for the Task Force's current \
         finding, applicability, barriers, and economic evidence before you rely on a paper."
            .into(),
    );
    caveats.push(
        "What Works for Health (County Health Rankings and Roadmaps) is not included: its site \
         could not be reached when this tool was built, so none of its strategies or evidence \
         ratings are bundled. Search it yourself for your topic."
            .into(),
    );
    caveats.push(
        "An evidence rating is shown only when the paper's own title or abstract uses the words \
         (strong evidence, sufficient evidence, insufficient evidence, the Task Force \
         recommends). Many abstracts report findings and not the Task Force's rating, and a paper \
         can cover several interventions with different ratings. 'Insufficient evidence' is not \
         evidence that an intervention does not work."
            .into(),
    );
    caveats.push(format!(
        "Papers were read from PubMed on {} (dates are journal publication dates). Excerpts are \
         short quotations from abstracts; see the journal for the full text and its terms.",
        snap.retrieved
    ));
    if result.matches.is_empty() {
        caveats.push(
            "No paper matched. Try other words for the topic (for example 'tobacco' and \
             'smoking' are both recognized), drop the evidence filter, or search the Community \
             Guide directly."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[EVIDENCE_SOURCES]));

    let summary = summary(&result);
    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            row_level_data: false,
            notes: vec![if args.include_live.unwrap_or(true) && !ctx.http.is_offline() {
                "Only the topic words you gave were sent to PubMed for the live search; no \
                 local files were read."
                    .to_string()
            } else {
                "The search ran on the bundled snapshot; nothing was sent anywhere and no \
                 local files were read."
                    .to_string()
            }],
            ..Privacy::none()
        }))
}

fn summary(r: &EvidenceResult) -> String {
    let mut s = format!(
        "**Community Guide evidence for \"{}\"**: {} paper(s) matched ({} listed). Source: papers \
         PubMed attributes to the Community Preventive Services Task Force, snapshot of {}; \
         What Works for Health is not included.\n\n",
        r.topic,
        r.matched_total,
        r.matches.len(),
        r.snapshot.retrieved
    );
    if !r.topics_recognized.is_empty() {
        s.push_str(&format!(
            "Topic groups searched: {}.\n\n",
            r.topics_recognized.join(", ")
        ));
    }
    if r.matches.is_empty() {
        s.push_str("No paper matched. See the caveats for what to try.\n\n");
    } else {
        let rows: Vec<Vec<String>> = r
            .matches
            .iter()
            .map(|m| {
                vec![
                    m.evidence_rating.clone(),
                    m.title.clone(),
                    m.kind.clone(),
                    format!(
                        "{}, {}{}",
                        m.journal.as_deref().unwrap_or("-"),
                        m.published,
                        if m.source_mode == "live" { " (new in PubMed)" } else { "" }
                    ),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Evidence wording (from the paper)", "Paper", "Kind", "Journal, date"],
            &rows,
        ));
        s.push('\n');
        let notes: Vec<String> = r
            .matches
            .iter()
            .filter_map(|m| {
                m.implementation_notes
                    .as_ref()
                    .map(|n| format!("- {} ({}): {n}", m.title, m.published))
            })
            .collect();
        if !notes.is_empty() {
            s.push_str("**Implementation notes from the abstracts:**\n\n");
            s.push_str(&notes.join("\n"));
            s.push_str("\n\n");
        }
    }
    s.push_str(&format!("Live PubMed search: {}\n", r.live.status));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> EvidenceContent {
        content::load(EVIDENCE_SOURCES).unwrap()
    }

    #[test]
    fn content_loads_and_has_the_five_rating_terms() {
        let c = content();
        assert_eq!(c.rating_key.len(), 5);
        assert_eq!(c.not_read.len(), 2);
        assert!(c.topics.len() >= 20);
        assert!(c.not_read.iter().any(|n| n.id == "what_works_for_health"));
    }

    #[test]
    fn topic_synonyms_are_recognized() {
        let c = content();
        let q = build_query(&c, "reduce smoking", None, None);
        assert!(q.topics_recognized.iter().any(|t| t.contains("Tobacco")), "{:?}", q.topics_recognized);
        assert!(q.topic_stems.contains("tobacco"));
        let q = build_query(&c, "kids who drink", Some("adolescents"), Some("school"));
        assert!(q.topics_recognized.iter().any(|t| t == "Alcohol"), "{:?}", q.topics_recognized);
        assert!(q.boost_stems.contains("adolescent") || q.boost_stems.contains("school"));
    }

    #[test]
    fn rating_labels_use_only_the_words_found() {
        let (l, b) = rating_label_of(&[RatingTerm::StrongEvidence], None, true);
        assert_eq!(l, "strong evidence of effectiveness");
        assert_eq!(b, "abstract wording");
        let (l, _) = rating_label_of(
            &[RatingTerm::StrongEvidence, RatingTerm::InsufficientEvidence],
            None,
            true,
        );
        assert!(l.contains("different interventions"), "{l}");
        let (l, b) = rating_label_of(&[], None, false);
        assert!(l.contains("no abstract"), "{l}");
        assert_eq!(b, "not in the PubMed record");
        let (_, b) = rating_label_of(&[], Some(TitleSignal::RecommendedAgainst), false);
        assert_eq!(b, "title wording");
        let (l, _) = rating_label_of(&[RatingTerm::Recommended], None, true);
        assert_eq!(l, "Task Force recommends");
    }

    #[test]
    fn old_papers_get_an_update_note() {
        assert!(update_note("2010-02", 2026).is_some());
        assert!(update_note("2025", 2026).is_none());
        assert!(update_note("bad", 2026).is_none());
    }

    #[test]
    fn conclusions_are_cut_at_the_next_label() {
        let a = "BACKGROUND: x. RESULTS: y. CONCLUSIONS: Interventions worked. They were cheap. \
                 DISCUSSION: later text.";
        assert_eq!(
            conclusion_of(a).as_deref(),
            Some("Interventions worked. They were cheap.")
        );
        assert_eq!(conclusion_of("No labels here."), None);
    }

    #[test]
    fn evidence_filters() {
        assert!(passes_evidence(&[RatingTerm::StrongEvidence], EvidenceFilter::StrongOrRecommended));
        assert!(!passes_evidence(&[RatingTerm::InsufficientEvidence], EvidenceFilter::StrongOrRecommended));
        assert!(passes_evidence(&[RatingTerm::InsufficientEvidence], EvidenceFilter::Insufficient));
        assert!(passes_evidence(&[], EvidenceFilter::Any));
        assert!(!passes_evidence(&[], EvidenceFilter::Insufficient));
    }
}
