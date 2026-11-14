//! Data for `find_evidence_based_interventions` (PLAN-03).
//!
//! * The bundled `cpstf-publications` snapshot: the journal papers PubMed attributes to the
//!   Community Preventive Services Task Force (Community Guide systematic reviews, economic
//!   reviews, and Task Force recommendation statements), read through PubMed by
//!   `xtask/scripts/build_interventions_snapshots.py`. The Community Guide and What Works for
//!   Health sites could not be reached from the build network, so there is no adapter for
//!   either; the PubMed record is the only live source.
//! * A live PubMed query for newer papers (Corporate Author clause plus the nurse's topic).
//!
//! Rating words are never inferred: a record carries a rating term only when its abstract uses
//! the words ([`rating_terms`] is the same rule as the builder's).

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::{SnapshotEntry, SnapshotStore};

pub const SNAPSHOT_ID: &str = "cpstf-publications";
pub const FILE: &str = "cpstf_publications.json";
/// PubMed Corporate Author clause for the Task Force (both names it has used).
pub const AUTHOR_CLAUSE: &str = "(\"Community Preventive Services Task Force\"[Corporate Author] OR \"Task Force on Community Preventive Services\"[Corporate Author])";

/// Kind of paper, from its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// A Task Force recommendation statement (or a combined report that includes one).
    Recommendation,
    /// A Community Guide systematic review of effectiveness.
    SystematicReview,
    /// A Community Guide systematic review of economic evidence.
    EconomicReview,
}

impl RecordKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Recommendation => "Task Force recommendation statement",
            Self::SystematicReview => "systematic review of effectiveness",
            Self::EconomicReview => "systematic economic review",
        }
    }
}

/// Rating words found in an abstract.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RatingTerm {
    StrongEvidence,
    SufficientEvidence,
    InsufficientEvidence,
    Recommended,
    RecommendedAgainst,
}

impl RatingTerm {
    pub fn label(self) -> &'static str {
        match self {
            Self::StrongEvidence => "strong evidence of effectiveness",
            Self::SufficientEvidence => "sufficient evidence of effectiveness",
            Self::InsufficientEvidence => "insufficient evidence to determine effectiveness",
            Self::Recommended => "Task Force recommends",
            Self::RecommendedAgainst => "Task Force recommends against",
        }
    }
}

/// What a title says about direction, when it says so clearly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TitleSignal {
    Recommended,
    RecommendedAgainst,
}

/// One paper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CpstfRecord {
    pub pmid: String,
    pub title: String,
    pub journal: Option<String>,
    /// Journal publication date: YYYY or YYYY-MM.
    pub published: String,
    pub doi: Option<String>,
    pub kind: RecordKind,
    pub title_signal: Option<TitleSignal>,
    pub has_abstract: bool,
    /// MeSH headings PubMed assigned (up to 14).
    pub mesh: Vec<String>,
    pub rating_terms: Vec<RatingTerm>,
    /// Abstract sentences that carry the rating words (short quotations).
    pub rating_sentences: Vec<String>,
    /// Short excerpt of the abstract's results.
    pub finding: Option<String>,
    /// Short excerpt of the abstract's conclusion.
    pub conclusion: Option<String>,
    /// Abstract sentence(s) on implementation, barriers, or cost, when the abstract has them.
    pub implementation: Option<String>,
}

impl CpstfRecord {
    pub fn pubmed_url(&self) -> String {
        format!("https://pubmed.ncbi.nlm.nih.gov/{}/", self.pmid)
    }

    pub fn doi_url(&self) -> Option<String> {
        self.doi.as_ref().map(|d| format!("https://doi.org/{d}"))
    }

    /// Four-digit publication year.
    pub fn year(&self) -> Option<i32> {
        self.published.get(..4)?.parse().ok()
    }
}

/// The bundled snapshot file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CpstfSnapshot {
    pub format_version: u32,
    pub description: String,
    pub retrieved: String,
    pub query: String,
    pub pubmed_count: usize,
    pub not_included: Vec<String>,
    pub records: Vec<CpstfRecord>,
}

/// Parse the snapshot text and check it is consistent.
pub fn parse_snapshot(text: &str) -> Result<CpstfSnapshot> {
    let snap: CpstfSnapshot = serde_json::from_str(text)
        .map_err(|e| PhnError::Internal(format!("Community Guide snapshot: {e}")))?;
    let mut seen = std::collections::BTreeSet::new();
    for r in &snap.records {
        if !seen.insert(r.pmid.as_str()) {
            return Err(PhnError::Internal(format!(
                "Community Guide snapshot: PubMed ID {} appears twice",
                r.pmid
            )));
        }
        match r.year() {
            Some(y) if (1900..=2100).contains(&y) => {}
            _ => {
                return Err(PhnError::Internal(format!(
                    "Community Guide snapshot: {} has publication date {}",
                    r.pmid, r.published
                )));
            }
        }
    }
    Ok(snap)
}

/// Load the bundled (or newer cached) snapshot with its manifest entry.
pub fn load_snapshot(store: &SnapshotStore) -> Result<(CpstfSnapshot, SnapshotEntry)> {
    let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
    Ok((parse_snapshot(&text)?, entry))
}

/// Split text into sentences at ". ", "? " or "! " followed by a capital, digit, or "(".
pub fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if matches!(chars[i], '.' | '?' | '!')
            && chars.get(i + 1) == Some(&' ')
            && chars
                .get(i + 2)
                .is_some_and(|c| c.is_uppercase() || c.is_ascii_digit() || *c == '(')
        {
            out.push(chars[start..=i].iter().collect::<String>());
            start = i + 2;
            i += 2;
            continue;
        }
        i += 1;
    }
    if start < chars.len() {
        out.push(chars[start..].iter().collect());
    }
    out
}

/// Rating words in one sentence (the builder's patterns, written without a regex library).
fn sentence_terms(sentence: &str) -> Vec<RatingTerm> {
    let s = sentence.to_lowercase();
    let mut v = Vec::new();
    // "recommend(s|ed|ation) against"
    let against = ["recommend", "recommends", "recommended", "recommendation"]
        .iter()
        .any(|w| s.contains(&format!("{w} against")));
    if against {
        v.push(RatingTerm::RecommendedAgainst);
    }
    if s.contains("insufficient evidence") {
        v.push(RatingTerm::InsufficientEvidence);
    }
    if s.contains("strong evidence") {
        v.push(RatingTerm::StrongEvidence);
    }
    // "sufficient evidence" not preceded by "in".
    let mut from = 0usize;
    while let Some(p) = s[from..].find("sufficient evidence") {
        let at = from + p;
        if !s[..at].ends_with("in") {
            v.push(RatingTerm::SufficientEvidence);
            break;
        }
        from = at + "sufficient evidence".len();
    }
    // "Task Force ... recommend(s|ed)" not followed by "against", within the sentence.
    if let Some(tf) = s.find("task force") {
        let rest = &s[tf..];
        let mut pos = 0usize;
        while let Some(p) = rest[pos..].find("recommend") {
            let at = pos + p;
            let after = &rest[at + "recommend".len()..];
            let word_end = after
                .find(|c: char| !c.is_alphabetic())
                .unwrap_or(after.len());
            let suffix = &after[..word_end];
            let tail = after[word_end..].trim_start();
            if matches!(suffix, "" | "s" | "ed") && !tail.starts_with("against") {
                v.push(RatingTerm::Recommended);
                break;
            }
            pos = at + "recommend".len();
        }
    }
    v
}

/// Rating words found in an abstract, in the order of [`RatingTerm`], and the sentences that
/// carry them (at most three).
pub fn rating_terms(abstract_text: &str) -> (Vec<RatingTerm>, Vec<String>) {
    let mut terms: Vec<RatingTerm> = Vec::new();
    let mut found = Vec::new();
    for s in sentences(abstract_text) {
        let t = sentence_terms(&s);
        if t.is_empty() {
            continue;
        }
        for x in t {
            if !terms.contains(&x) {
                terms.push(x);
            }
        }
        if found.len() < 3 {
            found.push(s);
        }
    }
    terms.sort();
    (terms, found)
}

/// Kind of paper from its title (the builder's rule, without the methods and other kinds,
/// which are not bundled). `None` for a paper that is not a review or recommendation.
pub fn kind_of(title: &str) -> Option<RecordKind> {
    let t = title.to_lowercase();
    if [
        "methods for",
        "community guide methods",
        "model for linking",
        "priority topics",
    ]
    .iter()
    .any(|p| t.contains(p))
    {
        return None;
    }
    if t.contains("recommendation") || t.contains("recommended") || t.contains("recommends") {
        return Some(RecordKind::Recommendation);
    }
    if t.contains("economic") {
        return Some(RecordKind::EconomicReview);
    }
    [
        "systematic review",
        "meta-analysis",
        "review",
        "effectiveness",
        "effects of",
        "economics of",
    ]
    .iter()
    .any(|p| t.contains(p))
    .then_some(RecordKind::SystematicReview)
}

/// The PubMed query for a topic: the Task Force Corporate Author clause AND the topic words.
pub fn live_query(topic_terms: &[String]) -> String {
    let words: Vec<String> = topic_terms
        .iter()
        .map(|t| t.trim().replace(['"', '[', ']', '(', ')'], " "))
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"[Title/Abstract]"))
        .collect();
    if words.is_empty() {
        return AUTHOR_CLAUSE.to_string();
    }
    format!("{AUTHOR_CLAUSE} AND ({})", words.join(" OR "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> CpstfSnapshot {
        let (text, _, _) = SnapshotStore::bundled()
            .file_text(SNAPSHOT_ID, FILE)
            .unwrap();
        parse_snapshot(&text).unwrap()
    }

    #[test]
    fn bundled_snapshot_parses_and_has_records() {
        let s = snap();
        assert!(s.records.len() > 100, "{}", s.records.len());
        assert!(s.retrieved.starts_with("2026-"));
        assert!(!s.not_included.is_empty());
    }

    #[test]
    fn rating_terms_match_the_builder_on_every_bundled_sentence() {
        // The builder stored the sentences that carry rating words and the terms it found;
        // the Rust rule must find exactly the same terms in those sentences.
        for r in &snap().records {
            let joined = r.rating_sentences.join(" ");
            let (got, _) = rating_terms(&joined);
            let mut want = r.rating_terms.clone();
            want.sort();
            // The stored sentences are the first three; terms found only in later sentences
            // are a subset of the record's terms, so every term found here must be in the
            // record, and a record with sentences must have found something here.
            for t in &got {
                assert!(want.contains(t), "{} {t:?} not in {want:?}", r.pmid);
            }
            if !want.is_empty() && !r.rating_sentences.is_empty() {
                assert!(!got.is_empty(), "{}: {:?}", r.pmid, r.rating_sentences);
            }
            if r.rating_sentences.len() < 3 {
                assert_eq!(got, want, "{}", r.pmid);
            }
        }
    }

    #[test]
    fn rating_words_need_the_words() {
        let (t, s) = rating_terms(
            "Results were mixed. The Community Preventive Services Task Force recommends \
             team-based care on the basis of strong evidence of effectiveness. There was \
             insufficient evidence to determine the effect of incentives.",
        );
        assert_eq!(
            t,
            vec![
                RatingTerm::StrongEvidence,
                RatingTerm::InsufficientEvidence,
                RatingTerm::Recommended
            ]
        );
        assert_eq!(s.len(), 2);
        let (t, _) = rating_terms("The Task Force recommends against this policy.");
        assert_eq!(t, vec![RatingTerm::RecommendedAgainst]);
        let (t, s) = rating_terms("The intervention reduced caries by 20%.");
        assert!(t.is_empty() && s.is_empty());
        // "insufficient evidence" is not "sufficient evidence".
        let (t, _) = rating_terms("There is insufficient evidence.");
        assert_eq!(t, vec![RatingTerm::InsufficientEvidence]);
    }

    #[test]
    fn kinds_follow_the_title() {
        assert_eq!(
            kind_of("Recommendation for use of immunization information systems"),
            Some(RecordKind::Recommendation)
        );
        assert_eq!(
            kind_of("Economics of Team-based Care: A Community Guide Systematic Review"),
            Some(RecordKind::EconomicReview)
        );
        assert_eq!(
            kind_of("Smokefree policies to reduce tobacco use. A systematic review."),
            Some(RecordKind::SystematicReview)
        );
        assert_eq!(kind_of("Methods for conducting systematic reviews"), None);
    }

    #[test]
    fn live_query_has_the_author_clause_and_quoted_phrases() {
        let q = live_query(&["school health".into(), "vision \"test\"".into()]);
        assert!(q.starts_with(AUTHOR_CLAUSE));
        assert!(q.contains("\"school health\"[Title/Abstract]"));
        assert!(q.contains("\"vision test\"[Title/Abstract]"));
        assert_eq!(live_query(&[]), AUTHOR_CLAUSE);
    }

    #[test]
    fn sentence_splitting_is_char_safe() {
        let s = sentences("Café programs worked. Évaluation of 3 studies (n=5). Done");
        assert_eq!(s.len(), 3, "{s:?}");
    }
}
