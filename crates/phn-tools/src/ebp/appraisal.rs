//! The engine behind `appraise_evidence` (EBP-03): content types for
//! `content/ebp/appraisal_checklists.yaml` and `content/ebp/grade_certainty.yaml`,
//! study-design identification from words in a text, answer validation and
//! tallies, the overall ratings that the Cochrane 2011 tool, ROBINS-I, and
//! AMSTAR 2 define, and the GRADE and GRADE-CERQual arithmetic.
//!
//! Nothing here answers an appraisal question. Answers come from the nurse;
//! the engine records them, checks that they are allowed, and applies each
//! instrument's published rule for combining them.

use std::collections::{BTreeMap, BTreeSet};

use phn_content::ContentMeta;
use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::epr::content::Confidence;

/// Content id of `content/ebp/appraisal_checklists.yaml`.
pub const CHECKLISTS: &str = "ebp-appraisal-checklists";
/// Content id of `content/ebp/grade_certainty.yaml`.
pub const GRADE: &str = "ebp-grade-certainty";

// ---------------------------------------------------------------- checklist content

/// A source listed in a content file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AppraisalSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
    #[serde(default)]
    pub licence: String,
    pub read: String,
    pub retrieved: String,
}

impl AppraisalSource {
    /// As an envelope citation (bundled content, so mode `snapshot`).
    pub fn to_source(&self) -> Source {
        Source {
            title: format!("{} ({}; licence: {})", self.title, self.read, self.licence),
            publisher: self.publisher.clone(),
            url: Some(self.url.clone()),
            effective_date: None,
            retrieved: Some(self.retrieved.clone()),
            mode: SourceMode::Snapshot,
        }
    }
}

/// CASP link and licence note.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CaspDef {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub licence_note: String,
    pub checklists_note: String,
}

/// An answer scale.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ScaleDef {
    pub label: String,
    pub values: Vec<String>,
    pub concern: Vec<String>,
    pub meaning: String,
}

/// A word or phrase that suggests a design.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Signal {
    pub phrase: String,
    pub weight: u32,
}

/// A design with a checklist.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DesignDef {
    pub id: String,
    pub label: String,
    pub how_to_tell: String,
    pub instrument: String,
    pub overall_method: String,
    pub starts_certainty: String,
    pub signals: Vec<Signal>,
}

/// A design recognised but without a checklist.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UnsupportedDef {
    pub id: String,
    pub label: String,
    pub signals: Vec<Signal>,
    pub pointer: String,
    pub source: Option<String>,
}

/// One appraisal question.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QuestionDef {
    pub id: String,
    pub designs: Vec<String>,
    pub domain: String,
    pub text: String,
    #[serde(default)]
    pub look_for: Option<String>,
    pub response: String,
    #[serde(default)]
    pub in_overall: bool,
    #[serde(default)]
    pub critical: bool,
    #[serde(default)]
    pub na_allowed: bool,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

/// How an instrument combines answers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OverallMethodDef {
    pub label: String,
    pub rule: String,
    pub source: String,
    pub section: String,
    pub note: String,
}

/// `content/ebp/appraisal_checklists.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChecklistContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, AppraisalSource>,
    pub casp: CaspDef,
    pub scales: BTreeMap<String, ScaleDef>,
    pub designs: Vec<DesignDef>,
    pub unsupported_designs: Vec<UnsupportedDef>,
    pub questions: Vec<QuestionDef>,
    pub overall_methods: BTreeMap<String, OverallMethodDef>,
}

impl ChecklistContent {
    /// The checklist design with this id.
    pub fn design(&self, id: &str) -> Option<&DesignDef> {
        self.designs.iter().find(|d| d.id == id)
    }

    /// Questions for a design, in file order.
    pub fn questions_for(&self, design: &str) -> Vec<&QuestionDef> {
        self.questions
            .iter()
            .filter(|q| q.designs.iter().any(|d| d == design))
            .collect()
    }

    /// Answers a question accepts.
    pub fn allowed_answers(&self, q: &QuestionDef) -> Vec<String> {
        let scale = &self.scales[&q.response];
        scale
            .values
            .iter()
            .filter(|v| v.as_str() != "not_applicable" || self.na_ok(q))
            .cloned()
            .collect()
    }

    fn na_ok(&self, q: &QuestionDef) -> bool {
        match q.response.as_str() {
            "yes_no" => true,
            "yes_partial_no" => q.na_allowed,
            _ => false,
        }
    }

    /// Source title and section, as shown to the nurse.
    pub fn citation(&self, q: &QuestionDef) -> String {
        let title = self
            .sources
            .get(&q.source)
            .map(|s| s.title.as_str())
            .unwrap_or(q.source.as_str());
        format!("{title}, {}", q.section)
    }
}

// ---------------------------------------------------------------- GRADE content

/// A source listed in the GRADE content file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GradeSource {
    pub title: String,
    pub publisher: String,
    pub url: String,
    pub read: String,
    pub retrieved: String,
}

impl GradeSource {
    /// As an envelope citation.
    pub fn to_source(&self) -> Source {
        Source {
            title: format!("{} ({})", self.title, self.read),
            publisher: self.publisher.clone(),
            url: Some(self.url.clone()),
            effective_date: None,
            retrieved: Some(self.retrieved.clone()),
            mode: SourceMode::Snapshot,
        }
    }
}

/// A certainty level.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LevelDef {
    pub id: String,
    pub rank: i32,
    pub label: String,
}

/// A starting level by evidence type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StartDef {
    pub evidence_type: String,
    pub label: String,
    pub start: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

/// A cited statement.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NoteDef {
    pub text: String,
    pub source: String,
    #[serde(default)]
    pub section: String,
}

/// A GRADE domain.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GradeDomainDef {
    pub id: String,
    pub direction: String,
    pub max_change: i32,
    pub label: String,
    pub ask: String,
    pub consider: Vec<NoteDef>,
    pub confidence: Confidence,
}

/// A CERQual component.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CerqualComponentDef {
    pub id: String,
    pub label: String,
    pub definition: String,
}

/// A CERQual concern level.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CerqualConcernDef {
    pub id: String,
    pub label: String,
}

/// A CERQual confidence level.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CerqualConfidenceDef {
    pub id: String,
    pub label: String,
    pub definition: String,
}

/// The CERQual block.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CerqualDef {
    pub start: String,
    pub start_note: String,
    pub source: String,
    pub components: Vec<CerqualComponentDef>,
    pub definitions_source: String,
    pub definitions_section: String,
    pub concern_levels: Vec<CerqualConcernDef>,
    pub concern_source: String,
    pub confidence_levels: Vec<CerqualConfidenceDef>,
    pub confidence_source: String,
    pub confidence_section: String,
    pub notes: Vec<NoteDef>,
}

/// `content/ebp/grade_certainty.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GradeContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, GradeSource>,
    pub scope_note: String,
    pub levels: Vec<LevelDef>,
    pub starting_levels: Vec<StartDef>,
    pub domains: Vec<GradeDomainDef>,
    pub overall_notes: Vec<NoteDef>,
    pub cerqual: CerqualDef,
}

impl GradeContent {
    fn level_by_rank(&self, rank: i32) -> &LevelDef {
        self.levels
            .iter()
            .find(|l| l.rank == rank)
            .unwrap_or(&self.levels[0])
    }

    fn level(&self, id: &str) -> Option<&LevelDef> {
        self.levels.iter().find(|l| l.id == id)
    }

    /// Source title for a key.
    pub fn title(&self, key: &str) -> String {
        self.sources
            .get(key)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| key.to_string())
    }
}

// ---------------------------------------------------------------- design identification

/// Study designs with a checklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StudyDesign {
    /// Randomized controlled trial.
    RandomizedTrial,
    /// Non-randomized study of an intervention: quasi-experimental, before-and-after,
    /// controlled before-after, interrupted time series.
    NonRandomizedIntervention,
    /// Cohort study.
    Cohort,
    /// Case-control study.
    CaseControl,
    /// Cross-sectional study or survey.
    CrossSectional,
    /// Systematic review or meta-analysis.
    SystematicReview,
    /// Qualitative study.
    Qualitative,
    /// Quality improvement or program improvement report.
    QualityImprovement,
}

impl StudyDesign {
    /// The id used in the content file.
    pub fn id(self) -> &'static str {
        match self {
            StudyDesign::RandomizedTrial => "randomized_trial",
            StudyDesign::NonRandomizedIntervention => "non_randomized_intervention",
            StudyDesign::Cohort => "cohort",
            StudyDesign::CaseControl => "case_control",
            StudyDesign::CrossSectional => "cross_sectional",
            StudyDesign::SystematicReview => "systematic_review",
            StudyDesign::Qualitative => "qualitative",
            StudyDesign::QualityImprovement => "quality_improvement",
        }
    }
}

/// A design suggested by the words in a text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DesignCandidate {
    /// Design id (a value for the `design` argument when it has a checklist).
    pub design: String,
    pub label: String,
    /// Sum of the weights of the matched words (a rough strength of evidence, not a probability).
    pub score: u32,
    /// Words from this tool's fixed word list that appear in the text (never excerpts of the text).
    pub matched_words: Vec<String>,
    /// False for designs the tool recognises but has no questions for.
    pub has_checklist: bool,
}

/// Lower-case, replace punctuation with spaces, and join "non-randomised" into one word so
/// that it does not count as "randomised".
pub fn normalize_for_signals(text: &str) -> String {
    let lowered: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect();
    let joined = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(" {} ", joined).replace(" non random", " nonrandom")
}

fn score_signals(padded: &str, signals: &[Signal]) -> (u32, Vec<String>) {
    let mut score = 0;
    let mut hit = Vec::new();
    for s in signals {
        let needle = format!(" {}", s.phrase);
        if padded.contains(&needle) {
            score += s.weight;
            hit.push(s.phrase.clone());
        }
    }
    (score, hit)
}

/// Rank the designs by how many of their words appear in the text. Designs with
/// no match are left out. Ties keep the order of the content file.
pub fn identify_design(c: &ChecklistContent, text: &str) -> Vec<DesignCandidate> {
    let padded = normalize_for_signals(text);
    let mut v: Vec<DesignCandidate> = Vec::new();
    for d in &c.designs {
        let (score, matched) = score_signals(&padded, &d.signals);
        if score > 0 {
            v.push(DesignCandidate {
                design: d.id.clone(),
                label: d.label.clone(),
                score,
                matched_words: matched,
                has_checklist: true,
            });
        }
    }
    for d in &c.unsupported_designs {
        let (score, matched) = score_signals(&padded, &d.signals);
        if score > 0 {
            v.push(DesignCandidate {
                design: d.id.clone(),
                label: d.label.clone(),
                score,
                matched_words: matched,
                has_checklist: false,
            });
        }
    }
    v.sort_by_key(|c| std::cmp::Reverse(c.score));
    v
}

/// How sure the word list is: "likely" when the top design scores at least 4 and at
/// least twice the runner-up, "possible" when there is a match, else "none".
pub fn identification_strength(candidates: &[DesignCandidate]) -> &'static str {
    match candidates {
        [] => "none",
        [top] => {
            if top.score >= 4 {
                "likely"
            } else {
                "possible"
            }
        }
        [top, second, ..] => {
            if top.score >= 4 && top.score >= 2 * second.score {
                "likely"
            } else {
                "possible"
            }
        }
    }
}

// ---------------------------------------------------------------- answers and tallies

/// One of the nurse's answers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct AnswerIn {
    /// Question id from the checklist (for example `rct_sequence`).
    pub question: String,
    /// The answer: a value the question accepts (`allowed_answers` in the checklist): for most
    /// questions yes, partly, no, cant_tell, not_applicable; for risk-of-bias questions low,
    /// unclear, high (or low, moderate, serious, critical, no_information); for systematic review
    /// questions yes, partial_yes, no (and not_applicable where a meta-analysis is needed).
    pub answer: String,
    /// Optional note: what in the report supports your answer (a short paraphrase or location,
    /// not client information).
    pub note: Option<String>,
}

/// A question with the nurse's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOut {
    pub id: String,
    pub domain: String,
    pub text: String,
    /// What to look for in the report.
    pub look_for: Option<String>,
    /// "yes_no", "yes_partial_no", "risk_3", or "risk_4".
    pub answer_scale: String,
    pub allowed_answers: Vec<String>,
    /// True for AMSTAR 2's critical domains.
    pub critical: bool,
    /// True when the answer feeds the instrument's own overall rating.
    pub feeds_overall: bool,
    /// The source, section, and how sure this project is that the question matches it.
    pub citation: String,
    pub confidence: Confidence,
    /// The nurse's answer (null until given).
    pub answer: Option<String>,
    pub note: Option<String>,
}

/// Counts of answers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Tally {
    pub questions: usize,
    pub answered: usize,
    pub unanswered: usize,
    /// Answers by value, for example {"yes": 7, "no": 2}.
    pub by_answer: BTreeMap<String, usize>,
}

/// A question to look at again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Concern {
    pub question: String,
    pub domain: String,
    pub answer: String,
    pub note: Option<String>,
}

/// Validate the nurse's answers against the design's questions and attach them.
pub fn apply_answers(
    c: &ChecklistContent,
    design: &str,
    answers: &[AnswerIn],
    redact: &dyn Fn(&str) -> String,
) -> Result<Vec<QuestionOut>> {
    let questions = c.questions_for(design);
    let mut given: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    for a in answers {
        let id = a.question.trim().to_lowercase();
        let Some(q) = questions.iter().find(|q| q.id == id) else {
            let ids: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not a question for this design. Questions: {}",
                a.question.trim(),
                ids.join(", ")
            )));
        };
        let value = a
            .answer
            .trim()
            .to_lowercase()
            .replace([' ', '-', '\''], "_");
        let value = match value.as_str() {
            "cant_tell" | "cannot_tell" | "can_t_tell" | "cant tell" => "cant_tell".to_string(),
            "not_applicable" | "n/a" | "na" => "not_applicable".to_string(),
            _ => value,
        };
        let allowed = c.allowed_answers(q);
        if !allowed.contains(&value) {
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not an answer for question {}. Allowed: {}",
                a.answer.trim(),
                q.id,
                allowed.join(", ")
            )));
        }
        if given.contains_key(&q.id) {
            return Err(PhnError::InvalidInput(format!(
                "question {} is answered more than once",
                q.id
            )));
        }
        let note = a
            .note
            .as_deref()
            .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|n| !n.is_empty())
            .map(|n| redact(&n.chars().take(500).collect::<String>()));
        given.insert(q.id.clone(), (value, note));
    }
    Ok(questions
        .into_iter()
        .map(|q| {
            let (answer, note) = match given.get(&q.id) {
                Some((a, n)) => (Some(a.clone()), n.clone()),
                None => (None, None),
            };
            QuestionOut {
                id: q.id.clone(),
                domain: q.domain.clone(),
                text: q.text.clone(),
                look_for: q.look_for.clone(),
                answer_scale: q.response.clone(),
                allowed_answers: c.allowed_answers(q),
                critical: q.critical,
                feeds_overall: q.in_overall,
                citation: c.citation(q),
                confidence: q.confidence,
                answer,
                note,
            }
        })
        .collect())
}

/// Count the answers.
pub fn tally(questions: &[QuestionOut]) -> Tally {
    let mut t = Tally {
        questions: questions.len(),
        ..Tally::default()
    };
    for q in questions {
        match &q.answer {
            Some(a) => {
                t.answered += 1;
                *t.by_answer.entry(a.clone()).or_default() += 1;
            }
            None => t.unanswered += 1,
        }
    }
    t
}

/// Answers that deserve a second look (the scale's concern values).
pub fn concerns(c: &ChecklistContent, questions: &[QuestionOut]) -> Vec<Concern> {
    questions
        .iter()
        .filter_map(|q| {
            let a = q.answer.as_ref()?;
            let flagged = c.scales.get(&q.answer_scale)?.concern.contains(a);
            flagged.then(|| Concern {
                question: q.id.clone(),
                domain: q.domain.clone(),
                answer: a.clone(),
                note: q.note.clone(),
            })
        })
        .collect()
}

// ---------------------------------------------------------------- overall ratings

/// The overall rating an instrument defines, when the nurse has answered enough.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OverallRating {
    /// Instrument and table the rule comes from.
    pub method: String,
    pub rule: String,
    /// The rating (null when answers are missing and the rule cannot be applied).
    pub rating: Option<String>,
    /// False when questions that feed the rating are unanswered.
    pub complete: bool,
    /// A rating reached before all answers were in (a rule that already applies, such as one
    /// critical flaw); it can only get worse, never better, with more answers.
    pub provisional: bool,
    /// Why this rating, in words.
    pub basis: String,
    /// Questions that feed the rating but have no answer.
    pub unanswered: Vec<String>,
    pub note: String,
    pub citation: String,
}

fn question_ids(qs: &[&QuestionOut]) -> Vec<String> {
    qs.iter().map(|q| q.id.clone()).collect()
}

/// Apply the instrument's rule. `key_domains` (risk-of-bias questions that count as key domains)
/// applies to the 2011 Cochrane tool only.
pub fn overall_rating(
    c: &ChecklistContent,
    design: &DesignDef,
    questions: &[QuestionOut],
    key_domains: &[String],
) -> Result<Option<OverallRating>> {
    let Some(def) = c.overall_methods.get(&design.overall_method) else {
        return Ok(None);
    };
    let citation = format!(
        "{}, {}",
        c.sources
            .get(&def.source)
            .map(|s| s.title.as_str())
            .unwrap_or(def.source.as_str()),
        def.section
    );
    let feeding: Vec<&QuestionOut> = questions.iter().filter(|q| q.feeds_overall).collect();
    let mut out = OverallRating {
        method: def.label.clone(),
        rule: def.rule.clone(),
        rating: None,
        complete: true,
        provisional: false,
        basis: String::new(),
        unanswered: Vec::new(),
        note: def.note.clone(),
        citation,
    };
    match design.overall_method.as_str() {
        "rob_table3" => {
            let keys: Vec<&QuestionOut> = if key_domains.is_empty() {
                feeding.clone()
            } else {
                let wanted: BTreeSet<String> = key_domains
                    .iter()
                    .map(|k| k.trim().to_lowercase())
                    .collect();
                for k in &wanted {
                    if !feeding.iter().any(|q| &q.id == k) {
                        let ids: Vec<&str> = feeding.iter().map(|q| q.id.as_str()).collect();
                        return Err(PhnError::InvalidInput(format!(
                            "'{k}' is not a risk-of-bias domain of this design. Domains: {}",
                            ids.join(", ")
                        )));
                    }
                }
                feeding
                    .iter()
                    .filter(|q| wanted.contains(&q.id))
                    .copied()
                    .collect()
            };
            let answered: Vec<&str> = keys.iter().filter_map(|q| q.answer.as_deref()).collect();
            let missing: Vec<&QuestionOut> = keys
                .iter()
                .filter(|q| q.answer.is_none())
                .copied()
                .collect();
            out.unanswered = question_ids(&missing);
            out.complete = missing.is_empty();
            let high = answered.iter().filter(|a| **a == "high").count();
            let unclear = answered.iter().filter(|a| **a == "unclear").count();
            if high > 0 {
                out.rating = Some("high".into());
                out.provisional = !out.complete;
                out.basis = format!("{high} key domain(s) judged high risk of bias.");
            } else if out.complete {
                if unclear > 0 {
                    out.rating = Some("unclear".into());
                    out.basis = format!(
                        "No key domain is high risk, but {unclear} are unclear (too little reported or risk not known)."
                    );
                } else {
                    out.rating = Some("low".into());
                    out.basis = "Every key domain is low risk of bias.".into();
                }
            } else {
                out.basis = format!(
                    "{} key domain(s) are unanswered; no key domain answered so far is high risk.",
                    missing.len()
                );
            }
        }
        "robins_table2" => {
            let answered: Vec<&str> = feeding.iter().filter_map(|q| q.answer.as_deref()).collect();
            let missing: Vec<&QuestionOut> = feeding
                .iter()
                .filter(|q| q.answer.is_none())
                .copied()
                .collect();
            out.unanswered = question_ids(&missing);
            out.complete = missing.is_empty();
            let count = |v: &str| answered.iter().filter(|a| **a == v).count();
            let (crit, ser, noinfo, low, moderate) = (
                count("critical"),
                count("serious"),
                count("no_information"),
                count("low"),
                count("moderate"),
            );
            if crit > 0 {
                out.rating = Some("critical".into());
                out.provisional = !out.complete;
                out.basis = format!("{crit} domain(s) judged critical risk of bias.");
            } else if ser > 0 {
                out.rating = Some("serious".into());
                out.provisional = !out.complete;
                out.basis = format!(
                    "{ser} domain(s) judged serious risk of bias and none critical{}.",
                    if out.complete { "" } else { " so far" }
                );
            } else if !out.complete {
                out.basis = format!(
                    "{} domain(s) are unanswered; none answered so far is serious or critical.",
                    missing.len()
                );
            } else if noinfo > 0 {
                out.rating = Some("no_information".into());
                out.basis = format!(
                    "No domain is serious or critical, but {noinfo} have no information; the table says a judgment is still required."
                );
            } else if moderate > 0 {
                out.rating = Some("moderate".into());
                out.basis =
                    format!("Every domain is low or moderate ({moderate} moderate, {low} low).");
            } else {
                out.rating = Some("low".into());
                out.basis = "Every domain is low risk of bias.".into();
            }
        }
        "amstar2_box2" => {
            let missing: Vec<&QuestionOut> = feeding
                .iter()
                .filter(|q| q.answer.is_none())
                .copied()
                .collect();
            out.unanswered = question_ids(&missing);
            out.complete = missing.is_empty();
            let no = |q: &&&QuestionOut| q.answer.as_deref() == Some("no");
            let critical_flaws = feeding.iter().filter(|q| q.critical).filter(no).count();
            let weaknesses = feeding.iter().filter(|q| !q.critical).filter(no).count();
            let partial = feeding
                .iter()
                .filter(|q| q.answer.as_deref() == Some("partial_yes"))
                .count();
            let counts = format!(
                "{critical_flaws} critical flaw(s) and {weaknesses} non-critical weakness(es) answered no; {partial} partial yes (listed, not counted)"
            );
            if critical_flaws >= 2 {
                out.rating = Some("critically_low".into());
                out.provisional = !out.complete;
                out.basis = format!("More than one critical flaw: {counts}.");
            } else if !out.complete {
                out.basis = format!(
                    "{} item(s) are unanswered, so the rating cannot be completed yet ({counts}).",
                    missing.len()
                );
            } else if critical_flaws == 1 {
                out.rating = Some("low".into());
                out.basis = format!("One critical flaw: {counts}.");
            } else if weaknesses <= 1 {
                out.rating = Some("high".into());
                out.basis =
                    format!("No critical flaw and at most one non-critical weakness: {counts}.");
            } else {
                out.rating = Some("moderate".into());
                out.basis =
                    format!("No critical flaw but more than one non-critical weakness: {counts}.");
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(out))
}

// ---------------------------------------------------------------- GRADE

/// A GRADE judgment for one domain.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct GradeJudgmentIn {
    /// Domain: risk_of_bias, inconsistency, indirectness, imprecision, publication_bias (rate
    /// down), or large_effect, dose_response, plausible_confounding (rate up).
    pub domain: String,
    /// Levels to change: 0, -1, or -2 to rate down; 0, 1, or 2 to rate up (large_effect up to 2,
    /// the other rate-up domains up to 1).
    pub change: i32,
    /// Why (a sentence for your record).
    pub reason: Option<String>,
}

/// Input for a GRADE certainty rating of a body of evidence for one outcome.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GradeIn {
    /// The outcome the rating is for (for example "hospital admissions at 12 months").
    pub outcome: String,
    /// "randomized_trials" (start high) or "observational" (start low). Defaults from the study
    /// design when it is a trial or an observational design; required for a systematic review.
    pub evidence_type: Option<String>,
    /// Your judgment for each domain you considered; domains you leave out count as no change.
    pub judgments: Vec<GradeJudgmentIn>,
    /// Your final rating when it differs from the arithmetic (high, moderate, low, very_low).
    pub final_rating: Option<String>,
    /// Why your final rating differs.
    pub final_reason: Option<String>,
}

/// One domain's judgment and its source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradeDomainOut {
    pub domain: String,
    pub label: String,
    pub direction: String,
    pub change: i32,
    pub reason: Option<String>,
    /// Source of the domain's guidance.
    pub citation: String,
}

/// The GRADE rating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradeOut {
    pub outcome: String,
    pub evidence_type: String,
    pub start: String,
    pub start_citation: String,
    pub domains: Vec<GradeDomainOut>,
    /// Sum of the changes (negative = rated down).
    pub total_change: i32,
    /// Start level plus total change, held between very low and high.
    pub computed: String,
    pub computed_label: String,
    /// True when the sum went past very low or high and was held there.
    pub held_at_limit: bool,
    /// Your final rating (the computed one unless you gave another).
    pub final_rating: String,
    pub final_differs: bool,
    pub final_reason: Option<String>,
    pub notes: Vec<String>,
}

/// Work out a GRADE rating from the nurse's domain judgments.
pub fn grade_rating(
    g: &GradeContent,
    input: &GradeIn,
    design_start: Option<&str>,
) -> Result<GradeOut> {
    let outcome = input
        .outcome
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if outcome.is_empty() {
        return Err(PhnError::InvalidInput(
            "grade needs the outcome the rating is for".into(),
        ));
    }
    let evidence_type = match input
        .evidence_type
        .as_deref()
        .map(|s| s.trim().to_lowercase().replace([' ', '-'], "_"))
        .filter(|s| !s.is_empty())
    {
        Some(t) => t,
        None => match design_start {
            Some("high") => "randomized_trials".into(),
            Some("low") => "observational".into(),
            _ => {
                return Err(PhnError::InvalidInput(
                    "grade needs evidence_type (randomized_trials or observational): a systematic \
                     review can summarise either, and the starting level depends on which"
                        .into(),
                ));
            }
        },
    };
    let start_def = g
        .starting_levels
        .iter()
        .find(|s| s.evidence_type == evidence_type)
        .ok_or_else(|| {
            let ids: Vec<&str> = g
                .starting_levels
                .iter()
                .map(|s| s.evidence_type.as_str())
                .collect();
            PhnError::InvalidInput(format!("evidence_type must be one of: {}", ids.join(", ")))
        })?;
    let start_level = g
        .level(&start_def.start)
        .ok_or_else(|| PhnError::Internal("GRADE content: unknown start level".into()))?;
    let mut seen = BTreeSet::new();
    let mut domains = Vec::new();
    let mut total = 0;
    for j in &input.judgments {
        let id = j.domain.trim().to_lowercase().replace([' ', '-'], "_");
        let Some(d) = g.domains.iter().find(|d| d.id == id) else {
            let ids: Vec<&str> = g.domains.iter().map(|d| d.id.as_str()).collect();
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not a GRADE domain. Domains: {}",
                j.domain.trim(),
                ids.join(", ")
            )));
        };
        if !seen.insert(id.clone()) {
            return Err(PhnError::InvalidInput(format!(
                "domain {id} is judged more than once"
            )));
        }
        let (lo, hi) = if d.direction == "down" {
            (-d.max_change, 0)
        } else {
            (0, d.max_change)
        };
        if j.change < lo || j.change > hi {
            return Err(PhnError::InvalidInput(format!(
                "change for {id} must be between {lo} and {hi} ({})",
                if d.direction == "down" {
                    "a reason to rate down"
                } else {
                    "a reason to rate up"
                }
            )));
        }
        total += j.change;
        let reason = j
            .reason
            .as_deref()
            .map(|r| r.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|r| !r.is_empty())
            .map(|r| r.chars().take(500).collect::<String>());
        domains.push(GradeDomainOut {
            domain: d.id.clone(),
            label: d.label.clone(),
            direction: d.direction.clone(),
            change: j.change,
            reason,
            citation: d
                .consider
                .first()
                .map(|n| format!("{}, {}", g.title(&n.source), n.section))
                .unwrap_or_default(),
        });
    }
    let raw = start_level.rank + total;
    let clamped = raw.clamp(1, 4);
    let computed = g.level_by_rank(clamped);
    let final_def = match input
        .final_rating
        .as_deref()
        .map(|s| s.trim().to_lowercase().replace([' ', '-'], "_"))
        .filter(|s| !s.is_empty())
    {
        Some(f) => Some(g.level(&f).ok_or_else(|| {
            PhnError::InvalidInput("final_rating must be high, moderate, low, or very_low".into())
        })?),
        None => None,
    };
    let final_level = final_def.unwrap_or(computed);
    let final_differs = final_level.id != computed.id;
    let final_reason = input
        .final_reason
        .as_deref()
        .map(|r| r.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|r| !r.is_empty())
        .map(|r| r.chars().take(500).collect::<String>());
    if final_differs && final_reason.is_none() {
        return Err(PhnError::InvalidInput(
            "your final_rating differs from the arithmetic: say why in final_reason".into(),
        ));
    }
    let mut notes: Vec<String> = g
        .overall_notes
        .iter()
        .map(|n| format!("{} ({}, {})", n.text, g.title(&n.source), n.section))
        .collect();
    notes.insert(
        0,
        g.scope_note
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    );
    Ok(GradeOut {
        outcome,
        evidence_type,
        start: start_level.id.clone(),
        start_citation: format!("{}, {}", g.title(&start_def.source), start_def.section),
        domains,
        total_change: total,
        computed: computed.id.clone(),
        computed_label: computed.label.clone(),
        held_at_limit: raw != clamped,
        final_rating: final_level.id.clone(),
        final_differs,
        final_reason,
        notes,
    })
}

// ---------------------------------------------------------------- GRADE-CERQual

/// A CERQual concern for one component.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct CerqualConcernIn {
    /// methodological_limitations, coherence, adequacy, or relevance.
    pub component: String,
    /// none (no or very minor concerns), minor, moderate, or serious.
    pub level: String,
    /// What the concern is (a sentence for your record).
    pub reason: Option<String>,
}

/// Input for a GRADE-CERQual assessment of one finding from a synthesis of qualitative studies.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CerqualIn {
    /// The review finding being assessed (a short statement).
    pub finding: String,
    /// Your concern level for each of the four components.
    pub concerns: Vec<CerqualConcernIn>,
    /// Your overall confidence in the finding: high, moderate, low, or very_low. The tool does not
    /// work it out; CERQual leaves it to the reviewers' judgment.
    pub overall_confidence: Option<String>,
    /// Why you chose that confidence.
    pub overall_reason: Option<String>,
}

/// One component's concern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CerqualComponentOut {
    pub component: String,
    pub label: String,
    pub definition: String,
    /// Your concern level (null if not given).
    pub concern: Option<String>,
    pub concern_label: Option<String>,
    pub reason: Option<String>,
}

/// The CERQual assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CerqualOut {
    pub finding: String,
    pub components: Vec<CerqualComponentOut>,
    /// Components with no concern level yet.
    pub missing_components: Vec<String>,
    /// Your overall confidence, with its definition (null when you did not give one).
    pub overall_confidence: Option<String>,
    pub overall_definition: Option<String>,
    pub overall_reason: Option<String>,
    pub notes: Vec<String>,
}

fn clean(s: &str, max: usize) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// Record the nurse's CERQual concerns and overall confidence.
pub fn cerqual_assessment(g: &GradeContent, input: &CerqualIn) -> Result<CerqualOut> {
    let cq = &g.cerqual;
    let finding = clean(&input.finding, 400);
    if finding.is_empty() {
        return Err(PhnError::InvalidInput(
            "cerqual needs the review finding being assessed".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut given: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    for c in &input.concerns {
        let comp = c.component.trim().to_lowercase().replace([' ', '-'], "_");
        let comp = if comp == "methodological" || comp == "limitations" {
            "methodological_limitations".to_string()
        } else if comp == "adequacy_of_data" {
            "adequacy".to_string()
        } else {
            comp
        };
        if !cq.components.iter().any(|x| x.id == comp) {
            let ids: Vec<&str> = cq.components.iter().map(|x| x.id.as_str()).collect();
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not a CERQual component. Components: {}",
                c.component.trim(),
                ids.join(", ")
            )));
        }
        let level = c.level.trim().to_lowercase();
        if !cq.concern_levels.iter().any(|l| l.id == level) {
            let ids: Vec<&str> = cq.concern_levels.iter().map(|l| l.id.as_str()).collect();
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not a concern level. Levels: {}",
                c.level.trim(),
                ids.join(", ")
            )));
        }
        if !seen.insert(comp.clone()) {
            return Err(PhnError::InvalidInput(format!(
                "component {comp} is assessed more than once"
            )));
        }
        let reason = Some(clean(c.reason.as_deref().unwrap_or(""), 500)).filter(|r| !r.is_empty());
        given.insert(comp, (level, reason));
    }
    let components: Vec<CerqualComponentOut> = cq
        .components
        .iter()
        .map(|c| {
            let (concern, reason) = match given.get(&c.id) {
                Some((l, r)) => (Some(l.clone()), r.clone()),
                None => (None, None),
            };
            let concern_label = concern.as_ref().and_then(|l| {
                cq.concern_levels
                    .iter()
                    .find(|x| &x.id == l)
                    .map(|x| x.label.clone())
            });
            CerqualComponentOut {
                component: c.id.clone(),
                label: c.label.clone(),
                definition: c.definition.clone(),
                concern,
                concern_label,
                reason,
            }
        })
        .collect();
    let missing: Vec<String> = components
        .iter()
        .filter(|c| c.concern.is_none())
        .map(|c| c.component.clone())
        .collect();
    let overall = match input
        .overall_confidence
        .as_deref()
        .map(|s| s.trim().to_lowercase().replace([' ', '-'], "_"))
        .filter(|s| !s.is_empty())
    {
        Some(o) => Some(
            cq.confidence_levels
                .iter()
                .find(|l| l.id == o)
                .ok_or_else(|| {
                    PhnError::InvalidInput(
                        "overall_confidence must be high, moderate, low, or very_low".into(),
                    )
                })?,
        ),
        None => None,
    };
    let mut notes = vec![format!(
        "{} ({}, paper 2)",
        cq.start_note,
        g.title(&cq.source)
    )];
    notes.extend(
        cq.notes
            .iter()
            .map(|n| format!("{} ({}, {})", n.text, g.title(&n.source), n.section)),
    );
    Ok(CerqualOut {
        finding,
        components,
        missing_components: missing,
        overall_confidence: overall.map(|o| o.label.clone()),
        overall_definition: overall.map(|o| o.definition.clone()),
        overall_reason: Some(clean(input.overall_reason.as_deref().unwrap_or(""), 500))
            .filter(|r| !r.is_empty()),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epr::content;

    fn checklists() -> ChecklistContent {
        content::load(CHECKLISTS).unwrap()
    }

    fn grade() -> GradeContent {
        content::load(GRADE).unwrap()
    }

    fn ans(q: &str, a: &str) -> AnswerIn {
        AnswerIn {
            question: q.into(),
            answer: a.into(),
            note: None,
        }
    }

    fn no_redact(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn content_is_consistent() {
        let c = checklists();
        let g = grade();
        // Every question names a real design, scale, and source.
        for q in &c.questions {
            assert!(c.scales.contains_key(&q.response), "{}", q.id);
            assert!(c.sources.contains_key(&q.source), "{}", q.id);
            for d in &q.designs {
                assert!(c.design(d).is_some(), "{} names unknown design {d}", q.id);
            }
        }
        // Every design has questions, and a defined overall method when it names one.
        for d in &c.designs {
            assert!(!c.questions_for(&d.id).is_empty(), "{}", d.id);
            assert!(
                d.overall_method == "none" || c.overall_methods.contains_key(&d.overall_method)
            );
        }
        // Question ids are unique.
        let ids: BTreeSet<&str> = c.questions.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(ids.len(), c.questions.len());
        // Sources named by the GRADE content exist.
        for d in &g.domains {
            for n in &d.consider {
                assert!(g.sources.contains_key(&n.source), "{}", d.id);
            }
        }
        assert_eq!(g.cerqual.components.len(), 4);
    }

    #[test]
    fn amstar2_has_sixteen_items_and_seven_critical_domains() {
        let c = checklists();
        let sr = c.questions_for("systematic_review");
        assert_eq!(sr.len(), 16);
        let critical: Vec<&str> = sr
            .iter()
            .filter(|q| q.critical)
            .map(|q| q.id.as_str())
            .collect();
        // AMSTAR 2 box 1: items 2, 4, 7, 9, 11, 13, 15.
        assert_eq!(
            critical,
            vec![
                "sr_02_protocol",
                "sr_04_search",
                "sr_07_excluded",
                "sr_09_rob",
                "sr_11_meta",
                "sr_13_rob_interpretation",
                "sr_15_publication_bias"
            ]
        );
        // Cochrane 2011 tool: six domains plus "other".
        assert_eq!(
            c.questions_for("randomized_trial")
                .iter()
                .filter(|q| q.in_overall)
                .count(),
            7
        );
        // ROBINS-I: seven domains.
        assert_eq!(
            c.questions_for("non_randomized_intervention")
                .iter()
                .filter(|q| q.in_overall)
                .count(),
            7
        );
    }

    #[test]
    fn design_is_suggested_from_words_and_non_randomised_is_not_randomised() {
        let c = checklists();
        let rct = identify_design(
            &c,
            "Participants were randomly assigned to the program or usual care; allocation \
             concealment used sealed envelopes.",
        );
        assert_eq!(rct[0].design, "randomized_trial");
        assert_eq!(identification_strength(&rct), "likely");
        assert!(
            rct[0]
                .matched_words
                .contains(&"randomly assigned".to_string())
        );
        let nrsi = identify_design(
            &c,
            "A non-randomized, quasi-experimental pre-post study with a comparison group.",
        );
        assert_eq!(nrsi[0].design, "non_randomized_intervention");
        assert!(!nrsi.iter().any(|d| d.design == "randomized_trial"));
        let sr = identify_design(
            &c,
            "We searched databases following PRISMA and pooled randomised trials in a meta-analysis.",
        );
        assert_eq!(sr[0].design, "systematic_review");
        let qual = identify_design(
            &c,
            "Semi-structured interviews and focus groups; thematic analysis until saturation.",
        );
        assert_eq!(qual[0].design, "qualitative");
        let dx = identify_design(
            &c,
            "Sensitivity and specificity against a reference standard.",
        );
        assert_eq!(dx[0].design, "diagnostic_accuracy");
        assert!(!dx[0].has_checklist);
        assert!(identify_design(&c, "Nothing here says what kind of study it is.").is_empty());
        assert_eq!(identification_strength(&[]), "none");
    }

    #[test]
    fn answers_are_validated() {
        let c = checklists();
        let q = apply_answers(
            &c,
            "randomized_trial",
            &[ans("rct_sequence", "Low"), ans("RCT_harms", "Can't tell")],
            &no_redact,
        )
        .unwrap();
        assert_eq!(
            q.iter()
                .find(|x| x.id == "rct_sequence")
                .unwrap()
                .answer
                .as_deref(),
            Some("low")
        );
        assert_eq!(
            q.iter()
                .find(|x| x.id == "rct_harms")
                .unwrap()
                .answer
                .as_deref(),
            Some("cant_tell")
        );
        // Wrong scale value, unknown question, duplicate.
        assert!(
            apply_answers(
                &c,
                "randomized_trial",
                &[ans("rct_sequence", "yes")],
                &no_redact
            )
            .is_err()
        );
        assert!(
            apply_answers(
                &c,
                "randomized_trial",
                &[ans("obs_bias", "yes")],
                &no_redact
            )
            .is_err()
        );
        assert!(
            apply_answers(
                &c,
                "randomized_trial",
                &[ans("rct_harms", "yes"), ans("rct_harms", "no")],
                &no_redact
            )
            .is_err()
        );
        // not_applicable only where a meta-analysis is needed.
        assert!(
            apply_answers(
                &c,
                "systematic_review",
                &[ans("sr_11_meta", "not_applicable")],
                &no_redact
            )
            .is_ok()
        );
        assert!(
            apply_answers(
                &c,
                "systematic_review",
                &[ans("sr_04_search", "not_applicable")],
                &no_redact
            )
            .is_err()
        );
        let t = tally(&q);
        assert_eq!((t.answered, t.unanswered), (2, t.questions - 2));
        let cs = concerns(&c, &q);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].question, "rct_harms");
    }

    fn all_answers(c: &ChecklistContent, design: &str, value: &str) -> Vec<AnswerIn> {
        c.questions_for(design)
            .iter()
            .filter(|q| q.in_overall)
            .map(|q| ans(&q.id, value))
            .collect()
    }

    #[test]
    fn cochrane_summary_follows_table_3() {
        let c = checklists();
        let d = c.design("randomized_trial").unwrap();
        let run = |answers: Vec<AnswerIn>, keys: &[&str]| {
            let q = apply_answers(&c, "randomized_trial", &answers, &no_redact).unwrap();
            let keys: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
            overall_rating(&c, d, &q, &keys).unwrap().unwrap()
        };
        assert_eq!(
            run(all_answers(&c, "randomized_trial", "low"), &[])
                .rating
                .as_deref(),
            Some("low")
        );
        let mut a = all_answers(&c, "randomized_trial", "low");
        a[1] = ans("rct_concealment", "unclear");
        assert_eq!(run(a.clone(), &[]).rating.as_deref(), Some("unclear"));
        // With concealment not a key domain, the rest are all low.
        let keys = [
            "rct_sequence",
            "rct_blind_participants",
            "rct_blind_outcome",
            "rct_attrition",
            "rct_reporting",
            "rct_other",
        ];
        assert_eq!(run(a, &keys).rating.as_deref(), Some("low"));
        let mut b = all_answers(&c, "randomized_trial", "low");
        b[3] = ans("rct_blind_outcome", "high");
        assert_eq!(run(b, &[]).rating.as_deref(), Some("high"));
        // Incomplete: low so far gives no rating; one high is already a rating, provisional.
        let partial = run(vec![ans("rct_sequence", "low")], &[]);
        assert_eq!(partial.rating, None);
        assert!(!partial.complete && partial.unanswered.len() == 6);
        let early = run(vec![ans("rct_sequence", "high")], &[]);
        assert_eq!(early.rating.as_deref(), Some("high"));
        assert!(early.provisional);
        let q = apply_answers(&c, "randomized_trial", &[], &no_redact).unwrap();
        assert!(overall_rating(&c, d, &q, &["nope".into()]).is_err());
    }

    #[test]
    fn robins_i_overall_follows_table_2() {
        let c = checklists();
        let d = c.design("non_randomized_intervention").unwrap();
        let run = |mut base: Vec<AnswerIn>, change: &[(&str, &str)]| {
            for (q, a) in change {
                base.iter_mut().find(|x| x.question == *q).unwrap().answer = a.to_string();
            }
            let q = apply_answers(&c, "non_randomized_intervention", &base, &no_redact).unwrap();
            overall_rating(&c, d, &q, &[]).unwrap().unwrap()
        };
        let low = || all_answers(&c, "non_randomized_intervention", "low");
        assert_eq!(run(low(), &[]).rating.as_deref(), Some("low"));
        assert_eq!(
            run(low(), &[("nrsi_confounding", "moderate")])
                .rating
                .as_deref(),
            Some("moderate")
        );
        assert_eq!(
            run(
                low(),
                &[
                    ("nrsi_confounding", "serious"),
                    ("nrsi_missing", "moderate")
                ]
            )
            .rating
            .as_deref(),
            Some("serious")
        );
        assert_eq!(
            run(
                low(),
                &[
                    ("nrsi_confounding", "serious"),
                    ("nrsi_missing", "critical")
                ]
            )
            .rating
            .as_deref(),
            Some("critical")
        );
        assert_eq!(
            run(low(), &[("nrsi_missing", "no_information")])
                .rating
                .as_deref(),
            Some("no_information")
        );
        let r = run(vec![ans("nrsi_confounding", "serious")], &[]);
        assert_eq!(r.rating.as_deref(), Some("serious"));
        assert!(r.provisional);
    }

    #[test]
    fn amstar2_overall_follows_box_2() {
        let c = checklists();
        let d = c.design("systematic_review").unwrap();
        let run = |changes: &[(&str, &str)]| {
            let mut a = all_answers(&c, "systematic_review", "yes");
            for (q, v) in changes {
                a.iter_mut().find(|x| x.question == *q).unwrap().answer = v.to_string();
            }
            let q = apply_answers(&c, "systematic_review", &a, &no_redact).unwrap();
            overall_rating(&c, d, &q, &[]).unwrap().unwrap()
        };
        assert_eq!(run(&[]).rating.as_deref(), Some("high"));
        // One non-critical weakness is still high; two make it moderate.
        assert_eq!(
            run(&[("sr_05_selection", "no")]).rating.as_deref(),
            Some("high")
        );
        assert_eq!(
            run(&[("sr_05_selection", "no"), ("sr_06_extraction", "no")])
                .rating
                .as_deref(),
            Some("moderate")
        );
        // A partial yes is listed but not counted.
        let partial = run(&[
            ("sr_05_selection", "partial_yes"),
            ("sr_06_extraction", "partial_yes"),
        ]);
        assert_eq!(partial.rating.as_deref(), Some("high"));
        assert!(partial.basis.contains("2 partial yes"));
        assert_eq!(
            run(&[("sr_04_search", "no")]).rating.as_deref(),
            Some("low")
        );
        assert_eq!(
            run(&[("sr_04_search", "no"), ("sr_02_protocol", "no")])
                .rating
                .as_deref(),
            Some("critically_low")
        );
        assert_eq!(
            run(&[
                ("sr_11_meta", "not_applicable"),
                ("sr_12_meta_rob", "not_applicable"),
                ("sr_15_publication_bias", "not_applicable")
            ])
            .rating
            .as_deref(),
            Some("high")
        );
        // Incomplete answers: two critical flaws already settle it, one does not.
        let q = apply_answers(
            &c,
            "systematic_review",
            &[ans("sr_04_search", "no"), ans("sr_02_protocol", "no")],
            &no_redact,
        )
        .unwrap();
        let r = overall_rating(&c, d, &q, &[]).unwrap().unwrap();
        assert_eq!(r.rating.as_deref(), Some("critically_low"));
        assert!(r.provisional && !r.complete);
        let q = apply_answers(
            &c,
            "systematic_review",
            &[ans("sr_04_search", "no")],
            &no_redact,
        )
        .unwrap();
        assert_eq!(
            overall_rating(&c, d, &q, &[]).unwrap().unwrap().rating,
            None
        );
        // Designs without a rule have no overall rating.
        let cohort = c.design("cohort").unwrap();
        let q = apply_answers(&c, "cohort", &[], &no_redact).unwrap();
        assert!(overall_rating(&c, cohort, &q, &[]).unwrap().is_none());
    }

    fn j(domain: &str, change: i32) -> GradeJudgmentIn {
        GradeJudgmentIn {
            domain: domain.into(),
            change,
            reason: None,
        }
    }

    #[test]
    fn grade_adds_changes_to_the_starting_level() {
        let g = grade();
        let input = GradeIn {
            outcome: "Admissions".into(),
            evidence_type: Some("randomized_trials".into()),
            judgments: vec![j("risk_of_bias", -1), j("imprecision", -1)],
            ..GradeIn::default()
        };
        let out = grade_rating(&g, &input, None).unwrap();
        assert_eq!(
            (out.start.as_str(), out.total_change, out.computed.as_str()),
            ("high", -2, "low")
        );
        // Observational studies start low; a large effect raises them.
        let input = GradeIn {
            outcome: "Admissions".into(),
            judgments: vec![j("large_effect", 1)],
            ..GradeIn::default()
        };
        let out = grade_rating(&g, &input, Some("low")).unwrap();
        assert_eq!(
            (out.start.as_str(), out.computed.as_str()),
            ("low", "moderate")
        );
        // The sum is held between very low and high.
        let input = GradeIn {
            outcome: "x".into(),
            evidence_type: Some("observational".into()),
            judgments: vec![j("risk_of_bias", -2), j("indirectness", -2)],
            ..GradeIn::default()
        };
        let out = grade_rating(&g, &input, None).unwrap();
        assert_eq!(out.computed, "very_low");
        assert!(out.held_at_limit);
    }

    #[test]
    fn grade_refuses_bad_input() {
        let g = grade();
        let base = GradeIn {
            outcome: "x".into(),
            evidence_type: Some("randomized_trials".into()),
            ..GradeIn::default()
        };
        let with = |judgments: Vec<GradeJudgmentIn>| GradeIn {
            judgments,
            ..base.clone()
        };
        // Wrong direction, too many levels, unknown or repeated domain.
        assert!(grade_rating(&g, &with(vec![j("risk_of_bias", 1)]), None).is_err());
        assert!(grade_rating(&g, &with(vec![j("large_effect", -1)]), None).is_err());
        assert!(grade_rating(&g, &with(vec![j("risk_of_bias", -3)]), None).is_err());
        assert!(grade_rating(&g, &with(vec![j("dose_response", 2)]), None).is_err());
        assert!(grade_rating(&g, &with(vec![j("vibes", -1)]), None).is_err());
        assert!(
            grade_rating(
                &g,
                &with(vec![j("risk_of_bias", -1), j("risk_of_bias", -1)]),
                None
            )
            .is_err()
        );
        // No outcome; no way to know the starting level.
        assert!(grade_rating(&g, &GradeIn::default(), None).is_err());
        let no_type = GradeIn {
            outcome: "x".into(),
            ..GradeIn::default()
        };
        assert!(grade_rating(&g, &no_type, None).is_err());
        assert!(grade_rating(&g, &no_type, Some("none")).is_err());
        // A final rating that differs needs a reason; with one it is recorded.
        let differs = GradeIn {
            final_rating: Some("moderate".into()),
            ..base.clone()
        };
        assert!(grade_rating(&g, &differs, None).is_err());
        let differs = GradeIn {
            final_reason: Some("Trials were large and consistent.".into()),
            ..differs
        };
        let out = grade_rating(&g, &differs, None).unwrap();
        assert!(out.final_differs);
        assert_eq!(out.computed, "high");
        assert_eq!(out.final_rating, "moderate");
    }

    #[test]
    fn cerqual_records_concerns_and_leaves_the_overall_level_to_the_nurse() {
        let g = grade();
        let input = CerqualIn {
            finding: "Parents value flexible clinic hours.".into(),
            concerns: vec![
                CerqualConcernIn {
                    component: "Methodological limitations".into(),
                    level: "minor".into(),
                    reason: Some("Two studies had no description of the researcher's role".into()),
                },
                CerqualConcernIn {
                    component: "adequacy".into(),
                    level: "moderate".into(),
                    reason: None,
                },
            ],
            overall_confidence: Some("low".into()),
            overall_reason: None,
        };
        let out = cerqual_assessment(&g, &input).unwrap();
        assert_eq!(out.missing_components, vec!["coherence", "relevance"]);
        assert_eq!(out.overall_confidence.as_deref(), Some("Low confidence"));
        assert!(out.overall_definition.unwrap().contains("possible"));
        let none = cerqual_assessment(
            &g,
            &CerqualIn {
                finding: "x".into(),
                ..CerqualIn::default()
            },
        )
        .unwrap();
        assert!(none.overall_confidence.is_none());
        assert!(cerqual_assessment(&g, &CerqualIn::default()).is_err());
        let bad = CerqualIn {
            finding: "x".into(),
            concerns: vec![CerqualConcernIn {
                component: "vibes".into(),
                level: "minor".into(),
                reason: None,
            }],
            ..CerqualIn::default()
        };
        assert!(cerqual_assessment(&g, &bad).is_err());
        let bad = CerqualIn {
            finding: "x".into(),
            concerns: vec![CerqualConcernIn {
                component: "coherence".into(),
                level: "huge".into(),
                reason: None,
            }],
            ..CerqualIn::default()
        };
        assert!(cerqual_assessment(&g, &bad).is_err());
    }
}
