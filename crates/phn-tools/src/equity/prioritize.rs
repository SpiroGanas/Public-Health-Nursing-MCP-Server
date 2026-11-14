//! EQ-09 `prioritize_health_issues`: ranks community health issues with the
//! Hanlon method (basic priority rating and the PEARL test), a strategy grid,
//! and nominal group scoring, and writes the method and the rationale to a
//! document in /data/output.
//!
//! The arithmetic is plain and checked against hand-computed values (see the
//! tests and `tests/fixtures/prioritize_golden.json`). The formula is the
//! original Hanlon and Pickett (1984) equation as quoted by the Pan American
//! Health Organization's 2019 paper:
//!
//! ```text
//! BPR = ((A + B) x C / 3) x D
//! ```
//!
//! A (size of the problem) 0-10, B (seriousness) 0-20, C (effectiveness of the
//! intervention) 0-10, D (feasibility) 1 only when all five PEARL factors are
//! yes. Nothing here reads local files and no vote is tied to a person: the
//! nominal group takes aggregate counts.

use std::collections::BTreeSet;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/equity/prioritization.yaml`.
pub const PRIORITIZATION: &str = "equity-prioritization";

/// Most issues one call may rank.
pub const MAX_ISSUES: usize = 60;
/// Longest issue name or rationale, in characters.
const MAX_NAME_CHARS: usize = 200;
const MAX_TEXT_CHARS: usize = 1500;
/// Scores closer than this are tied (floating point noise only).
const TIE_EPSILON: f64 = 1e-9;
/// Adjacent scores within this share of the higher one are called close.
const CLOSE_SHARE: f64 = 0.05;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ComponentDef {
    id: String,
    label: String,
    min: f64,
    max: f64,
    question: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FactorDef {
    id: String,
    label: String,
    #[serde(default)]
    max: Option<f64>,
    question: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct VariantDef {
    id: String,
    label: String,
    formula: String,
    implemented: bool,
    note: String,
    source: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct HanlonDef {
    formula: String,
    formula_source: String,
    formula_section: String,
    formula_confidence: Confidence,
    divisor_note: String,
    components: Vec<ComponentDef>,
    seriousness_factors: Vec<FactorDef>,
    seriousness_factors_note: String,
    pearl: Vec<FactorDef>,
    pearl_note: String,
    variants: Vec<VariantDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct NominalDef {
    steps: Vec<Cited>,
    scoring_text: String,
    scoring_confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct QuadrantDef {
    id: String,
    label: String,
    important: bool,
    feasible: bool,
    advice: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct GridDef {
    text: String,
    confidence: Confidence,
    source: String,
    section: String,
    quadrants: Vec<QuadrantDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PrioritizationContent {
    sources: std::collections::BTreeMap<String, DocSource>,
    method_notes: Vec<Cited>,
    hanlon: HanlonDef,
    nominal_group: NominalDef,
    strategy_grid: GridDef,
    not_verified: Vec<String>,
}

// ---------------------------------------------------------------- arguments

/// The five PEARL answers for one issue.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PearlAnswers {
    /// Propriety: is it appropriate for the health department and its partners to act on this?
    pub propriety: Option<bool>,
    /// Economics: is an intervention economically feasible?
    pub economics: Option<bool>,
    /// Acceptability: does the community accept and want an intervention?
    pub acceptability: Option<bool>,
    /// Resources: are resources available?
    pub resources: Option<bool>,
    /// Legality: does the law allow the intervention?
    pub legality: Option<bool>,
}

impl PearlAnswers {
    fn list(&self) -> [(&'static str, &'static str, Option<bool>); 5] {
        [
            ("propriety", "Propriety", self.propriety),
            ("economics", "Economics", self.economics),
            ("acceptability", "Acceptability", self.acceptability),
            ("resources", "Resources", self.resources),
            ("legality", "Legality", self.legality),
        ]
    }
}

/// One community health issue and its ratings. Leave a rating out when it has not been
/// decided; the issue is then listed as incomplete, not scored as zero.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct IssueInput {
    /// Name of the issue, for example "Opioid overdose" or "Housing cost burden". A community
    /// issue, never a person.
    pub name: String,
    /// What the issue covers (optional).
    pub description: Option<String>,
    /// Hanlon A, size of the problem, 0 to 10 (decimals allowed). Rate it from your data, for
    /// example the percent of the population affected, and record your anchors in the rationale.
    pub size: Option<f64>,
    /// Hanlon B, seriousness, 0 to 20 in total. Give this OR the four factors below.
    pub seriousness: Option<f64>,
    /// Urgency, 0 to 5 (part of seriousness).
    pub urgency: Option<f64>,
    /// Severity of the consequences, 0 to 5 (part of seriousness).
    pub severity: Option<f64>,
    /// Economic loss, 0 to 5 (part of seriousness).
    pub economic_loss: Option<f64>,
    /// Effect on other people, 0 to 5 (part of seriousness).
    pub impact_on_others: Option<f64>,
    /// Hanlon C, effectiveness of the best available intervention, 0 to 10.
    pub effectiveness: Option<f64>,
    /// PEARL answers (true = yes). Any "no" makes the issue not feasible.
    pub pearl: Option<PearlAnswers>,
    /// Nominal group result for this issue: how many participants gave it rank 1, rank 2, and
    /// so on (a list such as [4, 3, 1]). Totals only, never who voted. Leave it out when the
    /// issue was not part of the vote; use [] or zeros when it got no votes.
    pub rank_counts: Option<Vec<u32>>,
    /// Why you rated it this way: data used, who agreed, what was debated. Written to the
    /// document.
    pub rationale: Option<String>,
}

/// How the PEARL test is used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PearlMode {
    /// The original equation: the score is multiplied by D (1 or 0), so an issue that fails
    /// PEARL scores 0 and ranks last (default).
    #[default]
    Multiply,
    /// PEARL is a screen applied first: issues that fail are listed as screened out and are
    /// not ranked (Neiger revision).
    Prescreen,
    /// Rank on the basic priority rating alone; PEARL answers are shown but not used.
    Off,
}

/// Setup of the nominal group vote.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct NominalGroup {
    /// How many people voted.
    pub participants: u32,
    /// How many issues each person ranked (for example 3 for a top 3). Rank 1 earns this many
    /// points, the last rank earns 1.
    pub ranks_per_participant: u32,
}

/// Arguments for `prioritize_health_issues`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PrioritizeArgs {
    /// The issues to rank (2 to 60).
    pub issues: Vec<IssueInput>,
    /// The community or assessment these issues belong to, for the document title (optional).
    pub area: Option<String>,
    /// How PEARL is used: "multiply" (default), "prescreen", or "off".
    pub pearl_mode: Option<PearlMode>,
    /// Setup of the nominal group vote. Required when any issue has rank_counts.
    pub nominal_group: Option<NominalGroup>,
    /// Where the strategy grid splits importance and feasibility, from 0.1 to 0.9 (default 0.5).
    pub grid_threshold: Option<f64>,
    /// Write the method, results, and rationale to a document in /data/output (default true).
    pub write_document: Option<bool>,
    /// Document format: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
}

// ---------------------------------------------------------------- results

/// One PEARL answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PearlAnswerOut {
    pub factor: String,
    /// True (yes), false (no), or null (not answered).
    pub answer: Option<bool>,
}

/// Where an issue falls on the strategy grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GridPlacement {
    /// (A + B) / 30, from 0 to 1.
    pub importance: f64,
    /// C / 10, from 0 to 1.
    pub feasibility: f64,
    pub quadrant: String,
    pub advice: String,
}

/// Nominal group score of one issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NominalScore {
    /// Sum over ranks of (N + 1 - rank) x count.
    pub points: u32,
    /// Participants who ranked the issue at all.
    pub participants_ranking_it: u32,
    /// Participants who ranked it first.
    pub first_choice: u32,
}

/// Result for one issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IssueResult {
    pub name: String,
    pub description: Option<String>,
    pub rationale: Option<String>,
    /// A.
    pub size: Option<f64>,
    /// B (given, or the sum of the four factors).
    pub seriousness: Option<f64>,
    /// C.
    pub effectiveness: Option<f64>,
    pub pearl: Vec<PearlAnswerOut>,
    /// "passes", "fails" (names of failed factors in `pearl_failed`), "incomplete", or "not used".
    pub pearl_status: String,
    pub pearl_failed: Vec<String>,
    /// D: 1 or 0, when it could be decided.
    pub feasibility_d: Option<u8>,
    /// (A + B) x C / 3, before PEARL.
    pub basic_priority_rating: Option<f64>,
    /// BPR x D (or BPR alone when PEARL is off or used as a screen).
    pub priority_score: Option<f64>,
    /// "ranked", "screened_out", "incomplete", or "not_scored" (no Hanlon ratings given and
    /// not needed because only the nominal group was used).
    pub hanlon_status: String,
    /// Rank by priority score (1 = highest); tied issues share a rank.
    pub hanlon_rank: Option<u32>,
    pub tied_with: Vec<String>,
    /// Ratings still needed to score the issue.
    pub missing: Vec<String>,
    pub grid: Option<GridPlacement>,
    pub nominal: Option<NominalScore>,
    pub nominal_rank: Option<u32>,
    pub nominal_tied_with: Vec<String>,
}

/// Result of `prioritize_health_issues`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrioritizeResult {
    /// The equation used.
    pub formula: String,
    pub pearl_mode: PearlMode,
    /// Issues in Hanlon rank order, then screened-out, then incomplete.
    pub issues: Vec<IssueResult>,
    pub ranked_count: usize,
    pub screened_out_count: usize,
    pub incomplete_count: usize,
    /// Groups of issues with the same priority score.
    pub ties: Vec<Vec<String>>,
    /// Adjacent scores within 5% of each other (a small change in a rating could swap them).
    pub close_calls: Vec<String>,
    pub tie_guidance: Option<String>,
    pub grid_threshold: f64,
    pub grid_note: String,
    /// Nominal group order (issues with votes), when a vote was entered.
    pub nominal_order: Vec<String>,
    /// Issues whose Hanlon rank and nominal group rank differ by three or more places.
    pub method_disagreements: Vec<String>,
    pub method_notes: Vec<CitedOut>,
    /// What could not be verified in the method sources.
    pub not_verified: Vec<String>,
    /// The document written, when one was.
    pub document: Option<String>,
}

// ---------------------------------------------------------------- arithmetic

/// Hanlon basic priority rating before PEARL: (A + B) x C / 3.
///
/// Worked example: A = 8, B = 14, C = 6 gives (8 + 14) x 6 / 3 = 22 x 6 / 3 = 44.
pub fn basic_priority_rating(a: f64, b: f64, c: f64) -> f64 {
    (a + b) * c / 3.0
}

/// Nominal group points: with `n` ranks per participant, rank r earns n + 1 - r points.
///
/// Worked example: n = 3 and counts [4, 3, 1] give 4 x 3 + 3 x 2 + 1 x 1 = 19.
pub fn nominal_points(counts: &[u32], n: u32) -> u64 {
    counts
        .iter()
        .enumerate()
        .map(|(i, &c)| u64::from(n - i as u32) * u64::from(c))
        .sum()
}

/// Competition ranks (1, 2, 2, 4) for scores listed in any order; scores within
/// `TIE_EPSILON` are tied. Returns one rank per input, highest score first.
pub fn competition_ranks(scores: &[f64]) -> Vec<u32> {
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&i, &j| scores[j].total_cmp(&scores[i]));
    let mut ranks = vec![0u32; scores.len()];
    let mut rank = 0u32;
    for (pos, &i) in order.iter().enumerate() {
        if pos == 0 || (scores[order[pos - 1]] - scores[i]).abs() > TIE_EPSILON {
            rank = pos as u32 + 1;
        }
        ranks[i] = rank;
    }
    ranks
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn chars_len(s: &str) -> usize {
    s.chars().count()
}

fn check_range(issue: &str, field: &str, v: Option<f64>, max: f64) -> Result<Option<f64>> {
    match v {
        None => Ok(None),
        Some(x) if !x.is_finite() => Err(PhnError::InvalidInput(format!(
            "'{issue}': {field} must be a number"
        ))),
        Some(x) if !(0.0..=max).contains(&x) => Err(PhnError::InvalidInput(format!(
            "'{issue}': {field} is {x}; it must be between 0 and {max}"
        ))),
        Some(x) => Ok(Some(x)),
    }
}

/// Seriousness B: the total, or the sum of the four factors. Returns the value and the factors
/// still missing.
fn seriousness(i: &IssueInput) -> Result<(Option<f64>, Vec<String>)> {
    let name = i.name.as_str();
    let total = check_range(name, "seriousness", i.seriousness, 20.0)?;
    let factors = [
        ("urgency", check_range(name, "urgency", i.urgency, 5.0)?),
        ("severity", check_range(name, "severity", i.severity, 5.0)?),
        (
            "economic loss",
            check_range(name, "economic_loss", i.economic_loss, 5.0)?,
        ),
        (
            "effect on others",
            check_range(name, "impact_on_others", i.impact_on_others, 5.0)?,
        ),
    ];
    let any_factor = factors.iter().any(|(_, v)| v.is_some());
    if total.is_some() && any_factor {
        return Err(PhnError::InvalidInput(format!(
            "'{name}': give seriousness (0-20) or the four seriousness factors (0-5 each), not both"
        )));
    }
    if let Some(t) = total {
        return Ok((Some(t), Vec::new()));
    }
    if any_factor {
        let missing: Vec<String> = factors
            .iter()
            .filter(|(_, v)| v.is_none())
            .map(|(n, _)| format!("seriousness factor: {n}"))
            .collect();
        if missing.is_empty() {
            return Ok((Some(factors.iter().filter_map(|(_, v)| *v).sum()), missing));
        }
        return Ok((None, missing));
    }
    Ok((None, vec!["seriousness (B)".into()]))
}

/// Competition rank and tie lists for the entries of `scored` (index, score).
fn rank_with_ties(scored: &[(usize, f64)]) -> Vec<(usize, u32, Vec<usize>)> {
    let scores: Vec<f64> = scored.iter().map(|s| s.1).collect();
    let ranks = competition_ranks(&scores);
    scored
        .iter()
        .enumerate()
        .map(|(k, &(idx, _))| {
            let ties = scored
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k && ranks[*j] == ranks[k])
                .map(|(_, &(other, _))| other)
                .collect();
            (idx, ranks[k], ties)
        })
        .collect()
}

// ---------------------------------------------------------------- the tool

struct Draft {
    r: IssueResult,
    /// The unrounded priority score, used for ranking.
    raw: Option<f64>,
}

fn validate_text(label: &str, s: &str, max: usize) -> Result<()> {
    if chars_len(s) > max {
        return Err(PhnError::InvalidInput(format!(
            "{label} is longer than {max} characters"
        )));
    }
    Ok(())
}

/// Rank community health issues.
pub fn prioritize_health_issues(
    ctx: &Context,
    args: PrioritizeArgs,
) -> Result<Envelope<PrioritizeResult>> {
    let c: PrioritizationContent = content::load(PRIORITIZATION)?;
    let n_issues = args.issues.len();
    if n_issues < 2 {
        return Err(PhnError::InvalidInput(
            "give at least two issues to rank (name, plus size, seriousness, effectiveness, and \
             the five PEARL answers for the Hanlon method, and/or rank_counts for a nominal \
             group vote)"
                .into(),
        ));
    }
    if n_issues > MAX_ISSUES {
        return Err(PhnError::InvalidInput(format!(
            "{n_issues} issues given; rank at most {MAX_ISSUES} at a time"
        )));
    }
    let mode = args.pearl_mode.unwrap_or_default();
    let threshold = args.grid_threshold.unwrap_or(0.5);
    if !(0.1..=0.9).contains(&threshold) || !threshold.is_finite() {
        return Err(PhnError::InvalidInput(
            "grid_threshold must be between 0.1 and 0.9".into(),
        ));
    }
    // Names: present, bounded, unique ignoring case and spacing.
    let mut seen = BTreeSet::new();
    for (k, i) in args.issues.iter().enumerate() {
        let name = i.name.trim();
        if name.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "issue {} has no name",
                k + 1
            )));
        }
        validate_text("an issue name", name, MAX_NAME_CHARS)?;
        if let Some(d) = &i.description {
            validate_text(&format!("the description of '{name}'"), d, MAX_TEXT_CHARS)?;
        }
        if let Some(r) = &i.rationale {
            validate_text(&format!("the rationale for '{name}'"), r, MAX_TEXT_CHARS)?;
        }
        if !seen.insert(phn_core::text::normalize(name)) {
            return Err(PhnError::InvalidInput(format!(
                "two issues are named '{name}'; give each issue a different name"
            )));
        }
    }
    // Nominal group setup.
    let any_votes = args.issues.iter().any(|i| i.rank_counts.is_some());
    let nominal = match (&args.nominal_group, any_votes) {
        (None, true) => {
            return Err(PhnError::InvalidInput(
                "rank_counts need nominal_group: give participants and ranks_per_participant"
                    .into(),
            ));
        }
        (Some(ng), _) => {
            if ng.participants == 0 || ng.participants > 10_000 {
                return Err(PhnError::InvalidInput(
                    "nominal_group.participants must be between 1 and 10,000".into(),
                ));
            }
            if ng.ranks_per_participant == 0 || ng.ranks_per_participant > 20 {
                return Err(PhnError::InvalidInput(
                    "nominal_group.ranks_per_participant must be between 1 and 20".into(),
                ));
            }
            Some(ng.clone())
        }
        (None, false) => None,
    };
    if let Some(ng) = &nominal {
        let n = ng.ranks_per_participant as usize;
        let mut column_totals = vec![0u64; n];
        for i in &args.issues {
            if let Some(rc) = &i.rank_counts {
                if rc.len() > n {
                    return Err(PhnError::InvalidInput(format!(
                        "'{}': rank_counts has {} places but each participant ranked {n}",
                        i.name.trim(),
                        rc.len()
                    )));
                }
                for (r, &v) in rc.iter().enumerate() {
                    column_totals[r] += u64::from(v);
                }
            }
        }
        for (r, &t) in column_totals.iter().enumerate() {
            if t > u64::from(ng.participants) {
                return Err(PhnError::InvalidInput(format!(
                    "rank {} was given {t} times in total, but only {} participants voted; check \
                     the counts",
                    r + 1,
                    ng.participants
                )));
            }
        }
    }

    // Per-issue calculation.
    let mut caveats: Vec<String> = Vec::new();
    let mut drafts: Vec<Draft> = Vec::new();
    let quadrant = |important: bool, feasible: bool| {
        c.strategy_grid
            .quadrants
            .iter()
            .find(|q| q.important == important && q.feasible == feasible)
    };
    for i in &args.issues {
        let name = i.name.trim().to_string();
        let a = check_range(&name, "size", i.size, 10.0)?;
        let cc = check_range(&name, "effectiveness", i.effectiveness, 10.0)?;
        let (b, b_missing) = seriousness(i)?;
        let mut missing: Vec<String> = Vec::new();
        if a.is_none() {
            missing.push("size (A)".into());
        }
        missing.extend(b_missing);
        if cc.is_none() {
            missing.push("effectiveness (C)".into());
        }
        // PEARL.
        let answers = i.pearl.clone().unwrap_or_default();
        let list = answers.list();
        let failed: Vec<String> = list
            .iter()
            .filter(|(_, _, v)| *v == Some(false))
            .map(|(_, l, _)| (*l).to_string())
            .collect();
        let unanswered: Vec<&str> = list
            .iter()
            .filter(|(_, _, v)| v.is_none())
            .map(|(_, l, _)| *l)
            .collect();
        let pearl_status = if mode == PearlMode::Off {
            "not used"
        } else if !failed.is_empty() {
            "fails"
        } else if unanswered.is_empty() {
            "passes"
        } else {
            "incomplete"
        };
        let d: Option<u8> = match pearl_status {
            "passes" => Some(1),
            "fails" => Some(0),
            _ => None,
        };
        let bpr = match (a, b, cc) {
            (Some(a), Some(b), Some(c)) => Some(basic_priority_rating(a, b, c)),
            _ => None,
        };
        let has_hanlon_input = a.is_some()
            || b.is_some()
            || cc.is_some()
            || i.pearl.is_some()
            || i.urgency.is_some()
            || i.severity.is_some()
            || i.economic_loss.is_some()
            || i.impact_on_others.is_some()
            || i.seriousness.is_some();
        let (status, score): (&str, Option<f64>) = if !has_hanlon_input && nominal.is_some() {
            ("not_scored", None)
        } else {
            match (mode, pearl_status) {
                (PearlMode::Prescreen, "fails") => ("screened_out", None),
                (PearlMode::Multiply, "fails") => ("ranked", Some(0.0)),
                _ => {
                    let mut need = missing.clone();
                    if mode != PearlMode::Off && pearl_status == "incomplete" {
                        need.extend(unanswered.iter().map(|l| format!("PEARL: {l}")));
                    }
                    match (bpr, need.is_empty()) {
                        (Some(v), true) => {
                            let s = if mode == PearlMode::Multiply {
                                v * f64::from(d.unwrap_or(1))
                            } else {
                                v
                            };
                            ("ranked", Some(s))
                        }
                        _ => {
                            missing = need;
                            ("incomplete", None)
                        }
                    }
                }
            }
        };
        // A failed factor needs no other rating (D = 0 makes the product 0), so the missing
        // list is cleared for ranked and screened-out issues.
        if status != "incomplete" {
            missing.clear();
        }
        let grid = match (a, b, cc) {
            (Some(a), Some(b), Some(c)) => {
                let importance = (a + b) / 30.0;
                let feasibility = c / 10.0;
                quadrant(importance >= threshold, feasibility >= threshold).map(|q| GridPlacement {
                    importance: round2(importance),
                    feasibility: round2(feasibility),
                    quadrant: q.label.clone(),
                    advice: q.advice.clone(),
                })
            }
            _ => None,
        };
        let nominal_score = match (&nominal, &i.rank_counts) {
            (Some(ng), Some(rc)) => Some(NominalScore {
                points: nominal_points(rc, ng.ranks_per_participant) as u32,
                participants_ranking_it: rc.iter().sum(),
                first_choice: rc.first().copied().unwrap_or(0),
            }),
            _ => None,
        };
        drafts.push(Draft {
            raw: score,
            r: IssueResult {
                name,
                description: i.description.clone().filter(|s| !s.trim().is_empty()),
                rationale: i.rationale.clone().filter(|s| !s.trim().is_empty()),
                size: a,
                seriousness: b,
                effectiveness: cc,
                pearl: list
                    .iter()
                    .map(|(_, l, v)| PearlAnswerOut {
                        factor: (*l).to_string(),
                        answer: *v,
                    })
                    .collect(),
                pearl_status: pearl_status.into(),
                pearl_failed: failed,
                feasibility_d: d,
                basic_priority_rating: bpr.map(round2),
                priority_score: score.map(round2),
                hanlon_status: status.into(),
                hanlon_rank: None,
                tied_with: Vec::new(),
                missing,
                grid,
                nominal: nominal_score,
                nominal_rank: None,
                nominal_tied_with: Vec::new(),
            },
        });
    }

    // Hanlon ranking (on unrounded scores).
    let scored: Vec<(usize, f64)> = drafts
        .iter()
        .enumerate()
        .filter_map(|(k, d)| d.raw.map(|v| (k, v)))
        .collect();
    let mut tie_groups: Vec<Vec<String>> = Vec::new();
    for (idx, rank, ties) in rank_with_ties(&scored) {
        drafts[idx].r.hanlon_rank = Some(rank);
        let names: Vec<String> = ties.iter().map(|&t| drafts[t].r.name.clone()).collect();
        if !names.is_empty() {
            let mut group = vec![drafts[idx].r.name.clone()];
            group.extend(names.iter().cloned());
            group.sort();
            if !tie_groups.contains(&group) {
                tie_groups.push(group);
            }
        }
        drafts[idx].r.tied_with = names;
    }
    // Close calls between adjacent distinct scores.
    let mut by_score = scored.clone();
    by_score.sort_by(|x, y| y.1.total_cmp(&x.1));
    let mut close_calls = Vec::new();
    for w in by_score.windows(2) {
        let (hi, lo) = (w[0].1, w[1].1);
        if hi - lo > TIE_EPSILON && hi > 0.0 && (hi - lo) <= CLOSE_SHARE * hi {
            close_calls.push(format!(
                "{} ({:.2}) and {} ({:.2}) differ by {:.2} points ({:.1}% of the higher score)",
                drafts[w[0].0].r.name,
                hi,
                drafts[w[1].0].r.name,
                lo,
                hi - lo,
                (hi - lo) / hi * 100.0
            ));
        }
    }
    // Nominal ranking.
    let nominal_scored: Vec<(usize, f64)> = drafts
        .iter()
        .enumerate()
        .filter_map(|(k, d)| d.r.nominal.as_ref().map(|n| (k, f64::from(n.points))))
        .collect();
    for (idx, rank, ties) in rank_with_ties(&nominal_scored) {
        drafts[idx].r.nominal_rank = Some(rank);
        drafts[idx].r.nominal_tied_with = ties.iter().map(|&t| drafts[t].r.name.clone()).collect();
    }
    let mut nominal_order: Vec<(usize, f64)> = nominal_scored.clone();
    nominal_order.sort_by(|x, y| y.1.total_cmp(&x.1));
    let nominal_order_names: Vec<String> = nominal_order
        .iter()
        .map(|&(k, _)| drafts[k].r.name.clone())
        .collect();
    // Disagreement between methods.
    let mut disagreements = Vec::new();
    for d in &drafts {
        if let (Some(h), Some(n)) = (d.r.hanlon_rank, d.r.nominal_rank)
            && h.abs_diff(n) >= 3
        {
            disagreements.push(format!(
                "{}: Hanlon rank {h}, nominal group rank {n}",
                d.r.name
            ));
        }
    }

    // Order: ranked by rank then input order; screened out; incomplete; not scored.
    let order_key = |r: &IssueResult| -> (u8, u32) {
        match r.hanlon_status.as_str() {
            "ranked" => (0, r.hanlon_rank.unwrap_or(u32::MAX)),
            "screened_out" => (1, 0),
            "incomplete" => (2, 0),
            _ => (3, r.nominal_rank.unwrap_or(u32::MAX)),
        }
    };
    let mut issues: Vec<IssueResult> = drafts.into_iter().map(|d| d.r).collect();
    issues.sort_by_key(order_key); // stable: ties keep the order they were entered
    let ranked_count = issues
        .iter()
        .filter(|i| i.hanlon_status == "ranked")
        .count();
    let screened_out_count = issues
        .iter()
        .filter(|i| i.hanlon_status == "screened_out")
        .count();
    let incomplete_count = issues
        .iter()
        .filter(|i| i.hanlon_status == "incomplete")
        .count();
    if ranked_count == 0 && nominal_order_names.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "no issue could be scored: {} issue(s) are missing ratings (for example '{}' needs {}). \
             Each issue needs size (A), seriousness (B, or its four factors), effectiveness (C), \
             and the five PEARL answers; or give rank_counts for a nominal group vote.",
            incomplete_count,
            issues
                .iter()
                .find(|i| i.hanlon_status == "incomplete")
                .map(|i| i.name.as_str())
                .unwrap_or("an issue"),
            issues
                .iter()
                .find(|i| i.hanlon_status == "incomplete")
                .map(|i| i.missing.join(", "))
                .unwrap_or_default()
        )));
    }

    // Caveats.
    if !tie_groups.is_empty() {
        caveats.push(format!(
            "{} group(s) of issues are tied on priority score and share a rank; the tool does not \
             break ties. Discuss them with your partners.",
            tie_groups.len()
        ));
    }
    if incomplete_count > 0 {
        caveats.push(format!(
            "{incomplete_count} issue(s) are missing ratings and are not ranked (listed after the \
             ranked issues with what each still needs); a missing rating is not treated as zero."
        ));
    }
    if mode == PearlMode::Off {
        caveats.push(
            "PEARL was not used: the ranking is on the basic priority rating alone, so issues the \
             community cannot act on may rank high."
                .into(),
        );
    }
    if !disagreements.is_empty() {
        caveats.push(
            "Hanlon and nominal group ranks differ by three or more places for some issues; the \
             ranks are not combined, because the methods answer different questions (data-driven \
             scoring and the group's judgment)."
                .into(),
        );
    }
    if let Some(ng) = &nominal {
        let ranked_issues = issues.iter().filter(|i| i.nominal.is_some()).count();
        if ranked_issues < n_issues {
            caveats.push(format!(
                "{} issue(s) have no nominal group counts and are not in the nominal ranking.",
                n_issues - ranked_issues
            ));
        }
        let _ = ng;
    }
    // Identifier check on free text (the tool is for community issues, not people).
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for i in &args.issues {
        for text in [
            Some(i.name.as_str()),
            i.description.as_deref(),
            i.rationale.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            for f in phn_local::phi::scan_text(text) {
                if f.confidence == phn_local::phi::Confidence::High {
                    kinds.insert(f.kind.label().to_string());
                }
            }
        }
    }
    if !kinds.is_empty() {
        caveats.push(format!(
            "An issue name, description, or rationale looks like it contains {}. This tool ranks \
             community issues; leave out names and other identifying details, because everything \
             you type is sent to the AI service.",
            kinds.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    let divisor = c
        .hanlon
        .divisor_note
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    caveats.push(divisor);
    caveats.push(
        "Ratings are judgments: record how each was decided. NACCHO's own Hanlon worksheet could \
         not be read when this tool was built; the formula is the original Hanlon and Pickett \
         equation as quoted in the 2019 PAHO paper. Check it against the worksheet your \
         partners use."
            .into(),
    );
    caveats.extend(content::review_caveats(&[PRIORITIZATION]));

    let method_notes = content::cite_all(&c.method_notes, &c.sources);
    let tie_guidance = if tie_groups.is_empty() {
        None
    } else {
        Some(
            "Tied issues are shown in the order they were entered. To separate them, look first \
             at effectiveness (C): where two problems score the same, the one with a more \
             effective intervention returns more for the same effort; then at the strategy grid \
             and the nominal group result; or decide by discussion and record why."
                .to_string(),
        )
    };
    let grid_note = format!(
        "{} Threshold {:.2} on both axes. {}",
        c.strategy_grid
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        threshold,
        "This grid is this project's own (low confidence): the published sources name strategy \
         grids but do not describe one."
    );

    let mut result = PrioritizeResult {
        formula: c.hanlon.formula.clone(),
        pearl_mode: mode,
        issues,
        ranked_count,
        screened_out_count,
        incomplete_count,
        ties: tie_groups,
        close_calls,
        tie_guidance,
        grid_threshold: threshold,
        grid_note,
        nominal_order: nominal_order_names,
        method_disagreements: disagreements,
        method_notes,
        not_verified: c.not_verified.clone(),
        document: None,
    };

    // Document.
    let mut privacy = Privacy::none();
    if args.write_document.unwrap_or(true) {
        match ctx.sandbox() {
            Ok(_) => {
                let md_text =
                    document_markdown(&result, &c, args.area.as_deref(), nominal.as_ref());
                let title = match args
                    .area
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    Some(a) => format!("Health issue priorities: {a}"),
                    None => "Health issue priorities".to_string(),
                };
                let doc = templates::document(ctx, &md_text, "en-US", &title);
                let format = args.format.unwrap_or_default();
                let (dir, shown) = templates::output_folder(ctx, "health_issue_priorities")?;
                let file = templates::write_in(
                    ctx,
                    &dir,
                    &format!("health_issue_priorities.{}", format.ext()),
                    &format.render(&doc)?,
                )?;
                privacy.files_written.push(file.clone());
                privacy.notes.push(format!(
                    "The method, ratings, and rationale were written to {shown} (nothing in it \
                     comes from your files)."
                ));
                result.document = Some(file);
            }
            Err(e) => caveats.push(format!(
                "The document was not written: {e} Ranking results are above."
            )),
        }
    }

    let summary = summary(&result, &c);
    let mut sources: Vec<Source> =
        content::sources_for(["rpsp-2019", "rsp-2017", "jphmp-2026"], &c.sources);
    sources.push(Source::computed(
        "Hanlon basic priority rating, PEARL test, strategy grid, and nominal group scoring",
        "Calculated by this server from the ratings you gave",
    ));
    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn fmt_opt(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "—".into())
}

fn status_label(i: &IssueResult) -> String {
    match i.hanlon_status.as_str() {
        "ranked" => i
            .hanlon_rank
            .map(|r| {
                if i.tied_with.is_empty() {
                    r.to_string()
                } else {
                    format!("{r} (tied)")
                }
            })
            .unwrap_or_default(),
        "screened_out" => "screened out (PEARL)".into(),
        "incomplete" => "incomplete".into(),
        _ => "—".into(),
    }
}

fn summary(r: &PrioritizeResult, c: &PrioritizationContent) -> String {
    let mut s = format!(
        "**Health issue priorities** — Hanlon method ({}): {}\n\n",
        match r.pearl_mode {
            PearlMode::Multiply => "PEARL multiplies the score",
            PearlMode::Prescreen => "PEARL screens issues first",
            PearlMode::Off => "PEARL not used",
        },
        c.hanlon.formula
    );
    s.push_str(&format!(
        "{} ranked, {} screened out, {} incomplete.\n\n",
        r.ranked_count, r.screened_out_count, r.incomplete_count
    ));
    let rows: Vec<Vec<String>> = r
        .issues
        .iter()
        .map(|i| {
            vec![
                status_label(i),
                i.name.clone(),
                fmt_opt(i.size),
                fmt_opt(i.seriousness),
                fmt_opt(i.effectiveness),
                i.pearl_status.clone(),
                fmt_opt(i.basic_priority_rating),
                fmt_opt(i.priority_score),
                i.grid
                    .as_ref()
                    .map(|g| g.quadrant.clone())
                    .unwrap_or_else(|| "—".into()),
                i.nominal_rank
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "—".into()),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Rank",
            "Issue",
            "A size",
            "B seriousness",
            "C effectiveness",
            "PEARL",
            "BPR",
            "Score",
            "Grid",
            "Nominal rank",
        ],
        &rows,
    ));
    let incomplete: Vec<&IssueResult> = r
        .issues
        .iter()
        .filter(|i| i.hanlon_status == "incomplete")
        .collect();
    if !incomplete.is_empty() {
        s.push_str("\n**Still needed**\n");
        for i in incomplete {
            s.push_str(&format!("- {}: {}\n", i.name, i.missing.join(", ")));
        }
    }
    if !r.ties.is_empty() {
        s.push_str("\n**Ties**\n");
        for t in &r.ties {
            s.push_str(&format!("- {}\n", t.join(", ")));
        }
        if let Some(g) = &r.tie_guidance {
            s.push_str(&format!("\n{g}\n"));
        }
    }
    if !r.close_calls.is_empty() {
        s.push_str("\n**Close calls (a small change in one rating could swap them)**\n");
        for t in &r.close_calls {
            s.push_str(&format!("- {t}\n"));
        }
    }
    if !r.nominal_order.is_empty() {
        s.push_str(&format!(
            "\n**Nominal group order:** {}\n",
            r.nominal_order.join(" > ")
        ));
    }
    if let Some(d) = &r.document {
        s.push_str(&format!("\nDocument written: {d}\n"));
    }
    s
}

fn document_markdown(
    r: &PrioritizeResult,
    c: &PrioritizationContent,
    area: Option<&str>,
    nominal: Option<&NominalGroup>,
) -> String {
    let mut m = String::new();
    m.push_str(&format!(
        "# Health issue priorities{}\n\n",
        area.map(str::trim)
            .filter(|a| !a.is_empty())
            .map(|a| format!(": {a}"))
            .unwrap_or_default()
    ));
    m.push_str("## Method\n\n");
    m.push_str(&format!(
        "Issues were ranked with the Hanlon method. Equation: {} (A size of the problem 0-10, B \
         seriousness 0-20, C effectiveness of the intervention 0-10, D feasibility from the PEARL \
         test: 1 when propriety, economics, acceptability, resources, and legality are all yes, 0 \
         otherwise). {}\n\n",
        c.hanlon.formula,
        c.hanlon
            .divisor_note
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    ));
    m.push_str(&format!(
        "PEARL mode: {}.\n\n",
        match r.pearl_mode {
            PearlMode::Multiply =>
                "the score is multiplied by D (an issue that fails PEARL scores 0)",
            PearlMode::Prescreen => "issues that fail PEARL were screened out before ranking",
            PearlMode::Off => "not used",
        }
    ));
    m.push_str("PEARL questions:\n\n");
    for p in &c.hanlon.pearl {
        m.push_str(&format!("- **{}**: {}\n", p.label, p.question));
    }
    m.push('\n');
    m.push_str("## Ranking\n\n");
    let rows: Vec<Vec<String>> = r
        .issues
        .iter()
        .map(|i| {
            vec![
                status_label(i),
                i.name.clone(),
                fmt_opt(i.size),
                fmt_opt(i.seriousness),
                fmt_opt(i.effectiveness),
                i.pearl_status.clone(),
                fmt_opt(i.basic_priority_rating),
                fmt_opt(i.priority_score),
            ]
        })
        .collect();
    m.push_str(&md::table(
        &["Rank", "Issue", "A", "B", "C", "PEARL", "BPR", "Score"],
        &rows,
    ));
    m.push('\n');
    let failed: Vec<&IssueResult> = r
        .issues
        .iter()
        .filter(|i| !i.pearl_failed.is_empty())
        .collect();
    if !failed.is_empty() {
        m.push_str("## PEARL results\n\n");
        for i in failed {
            m.push_str(&format!(
                "- {}: answered no to {}.\n",
                i.name,
                i.pearl_failed.join(", ")
            ));
        }
        m.push('\n');
    }
    let incomplete: Vec<&IssueResult> = r
        .issues
        .iter()
        .filter(|i| i.hanlon_status == "incomplete")
        .collect();
    if !incomplete.is_empty() {
        m.push_str("## Issues not yet ranked\n\n");
        for i in incomplete {
            m.push_str(&format!("- {}: needs {}.\n", i.name, i.missing.join(", ")));
        }
        m.push('\n');
    }
    if !r.ties.is_empty() {
        m.push_str("## Ties\n\n");
        for t in &r.ties {
            m.push_str(&format!("- {} share a rank.\n", t.join(", ")));
        }
        if let Some(g) = &r.tie_guidance {
            m.push_str(&format!("\n{g}\n"));
        }
        m.push('\n');
    }
    m.push_str("## Strategy grid\n\n");
    m.push_str(&format!("{}\n\n", r.grid_note));
    let grid_rows: Vec<Vec<String>> = r
        .issues
        .iter()
        .filter_map(|i| {
            i.grid.as_ref().map(|g| {
                vec![
                    i.name.clone(),
                    format!("{:.2}", g.importance),
                    format!("{:.2}", g.feasibility),
                    g.quadrant.clone(),
                    g.advice.clone(),
                ]
            })
        })
        .collect();
    if !grid_rows.is_empty() {
        m.push_str(&md::table(
            &[
                "Issue",
                "Importance",
                "Feasibility",
                "Quadrant",
                "What it suggests",
            ],
            &grid_rows,
        ));
        m.push('\n');
    }
    if let Some(ng) = nominal {
        m.push_str("## Nominal group vote\n\n");
        m.push_str(&format!(
            "{} participants each ranked their top {}. {}\n\n",
            ng.participants,
            ng.ranks_per_participant,
            c.nominal_group
                .scoring_text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        ));
        let rows: Vec<Vec<String>> = r
            .issues
            .iter()
            .filter_map(|i| {
                i.nominal.as_ref().map(|n| {
                    vec![
                        i.nominal_rank.map(|x| x.to_string()).unwrap_or_default(),
                        i.name.clone(),
                        n.points.to_string(),
                        n.first_choice.to_string(),
                        n.participants_ranking_it.to_string(),
                    ]
                })
            })
            .collect();
        m.push_str(&md::table(
            &[
                "Rank",
                "Issue",
                "Points",
                "First choices",
                "Participants ranking it",
            ],
            &rows,
        ));
        m.push('\n');
    }
    m.push_str("## Rationale\n\n");
    let mut any = false;
    for i in &r.issues {
        if i.rationale.is_some() || i.description.is_some() {
            any = true;
            m.push_str(&format!("**{}**\n\n", i.name));
            if let Some(d) = &i.description {
                m.push_str(&format!("{}\n\n", d.trim()));
            }
            if let Some(x) = &i.rationale {
                m.push_str(&format!("{}\n\n", x.trim()));
            }
        }
    }
    if !any {
        m.push_str("No rationale was recorded. Add why each rating was given before this document is shared.\n\n");
    }
    m.push_str("## Sources and limits\n\n");
    for k in ["rpsp-2019", "rsp-2017", "jphmp-2026"] {
        if let Some(s) = c.sources.get(k) {
            m.push_str(&format!("- {} ({}), {}\n", s.title, s.publisher, s.url));
        }
    }
    m.push_str("\nNot verified when this tool was built:\n\n");
    for n in &r.not_verified {
        m.push_str(&format!("- {n}\n"));
    }
    m
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

    fn pearl_yes() -> PearlAnswers {
        PearlAnswers {
            propriety: Some(true),
            economics: Some(true),
            acceptability: Some(true),
            resources: Some(true),
            legality: Some(true),
        }
    }

    fn issue(name: &str, a: f64, b: f64, c: f64) -> IssueInput {
        IssueInput {
            name: name.into(),
            size: Some(a),
            seriousness: Some(b),
            effectiveness: Some(c),
            pearl: Some(pearl_yes()),
            ..Default::default()
        }
    }

    fn run(args: PrioritizeArgs) -> PrioritizeResult {
        let (_d, ctx) = ctx();
        let args = PrioritizeArgs {
            write_document: Some(false),
            ..args
        };
        prioritize_health_issues(&ctx, args).unwrap().data
    }

    #[test]
    fn basic_priority_rating_matches_hand_computed_values() {
        // (8 + 14) x 6 / 3 = 22 x 6 / 3 = 132 / 3 = 44
        assert_eq!(basic_priority_rating(8.0, 14.0, 6.0), 44.0);
        // (10 + 20) x 10 / 3 = 300 / 3 = 100, the largest possible score
        assert_eq!(basic_priority_rating(10.0, 20.0, 10.0), 100.0);
        // (0 + 0) x 10 / 3 = 0
        assert_eq!(basic_priority_rating(0.0, 0.0, 10.0), 0.0);
        // (6.5 + 12) x 4 / 3 = 18.5 x 4 / 3 = 74 / 3 = 24.666...
        assert!((basic_priority_rating(6.5, 12.0, 4.0) - 74.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn published_case_study_values_follow_from_the_formula() {
        // Table 5 of the 2017 Portuguese case study lists (A + B) x C x D for five problems
        // (its effectiveness scale is 0.5-1.5 and it does not divide by 3). Multiplying our
        // rating by 3 must give the published numbers for the same A, B, C, D.
        for (a, b, c, d, published) in [
            (10.0, 9.0, 1.0, 1.0, 19.0),
            (10.0, 7.0, 1.0, 1.0, 17.0),
            (10.0, 8.0, 1.0, 1.0, 18.0),
            (10.0, 10.0, 0.5, 1.0, 10.0),
            (6.0, 9.0, 1.0, 1.0, 15.0),
        ] {
            let ours = basic_priority_rating(a, b, c) * d * 3.0;
            assert!((ours - published).abs() < 1e-9, "{a} {b} {c}");
        }
    }

    #[test]
    fn nominal_points_are_computed_by_hand() {
        // n = 3: 4 x 3 + 3 x 2 + 1 x 1 = 19
        assert_eq!(nominal_points(&[4, 3, 1], 3), 19);
        // 3 x 3 + 2 x 2 + 4 x 1 = 17
        assert_eq!(nominal_points(&[3, 2, 4], 3), 17);
        // 2 x 3 + 3 x 2 + 3 x 1 = 15
        assert_eq!(nominal_points(&[2, 3, 3], 3), 15);
        // fewer places than n: only rank 1 and 2 used
        assert_eq!(nominal_points(&[5], 5), 25);
        assert_eq!(nominal_points(&[], 3), 0);
    }

    #[test]
    fn competition_ranks_share_ties() {
        assert_eq!(
            competition_ranks(&[44.0, 30.0, 30.0, 10.0]),
            vec![1, 2, 2, 4]
        );
        assert_eq!(competition_ranks(&[5.0, 5.0, 5.0]), vec![1, 1, 1]);
        assert_eq!(competition_ranks(&[1.0, 3.0, 2.0]), vec![3, 1, 2]);
        // floating point noise is not a difference: 0.1 + 0.2 vs 0.3
        assert_eq!(competition_ranks(&[0.1 + 0.2, 0.3]), vec![1, 1]);
    }

    #[test]
    fn ranks_issues_and_shares_ties() {
        let r = run(PrioritizeArgs {
            issues: vec![
                issue("Low", 2.0, 4.0, 3.0),       // (2+4)x3/3 = 6
                issue("High", 8.0, 14.0, 6.0),     // 44
                issue("Middle A", 5.0, 10.0, 6.0), // (5+10)x6/3 = 30
                issue("Middle B", 6.0, 9.0, 6.0),  // (6+9)x6/3 = 30
            ],
            ..Default::default()
        });
        let by = |n: &str| r.issues.iter().find(|i| i.name == n).unwrap();
        assert_eq!(by("High").priority_score, Some(44.0));
        assert_eq!(by("High").hanlon_rank, Some(1));
        assert_eq!(by("Middle A").priority_score, Some(30.0));
        assert_eq!(by("Middle A").hanlon_rank, Some(2));
        assert_eq!(by("Middle B").hanlon_rank, Some(2));
        assert_eq!(by("Middle A").tied_with, vec!["Middle B".to_string()]);
        assert_eq!(by("Low").hanlon_rank, Some(4));
        assert_eq!(
            r.ties,
            vec![vec!["Middle A".to_string(), "Middle B".to_string()]]
        );
        assert!(r.tie_guidance.is_some());
        // Order: rank order; tied issues keep the order entered.
        let names: Vec<&str> = r.issues.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["High", "Middle A", "Middle B", "Low"]);
    }

    #[test]
    fn failed_pearl_scores_zero_when_multiplying_and_is_excluded_when_screening() {
        let mut failing = issue("Not feasible", 9.0, 18.0, 9.0); // BPR (27 x 9 / 3) = 81
        failing.pearl = Some(PearlAnswers {
            legality: Some(false),
            ..pearl_yes()
        });
        let args = |mode| PrioritizeArgs {
            issues: vec![issue("Feasible", 4.0, 6.0, 3.0), failing.clone()], // (10 x 3 / 3) = 10
            pearl_mode: Some(mode),
            ..Default::default()
        };
        let multiply = run(args(PearlMode::Multiply));
        let nf = multiply
            .issues
            .iter()
            .find(|i| i.name == "Not feasible")
            .unwrap();
        assert_eq!(nf.basic_priority_rating, Some(81.0));
        assert_eq!(nf.priority_score, Some(0.0));
        assert_eq!(nf.hanlon_rank, Some(2));
        assert_eq!(nf.pearl_failed, vec!["Legality".to_string()]);
        assert_eq!(multiply.issues[0].name, "Feasible");

        let prescreen = run(args(PearlMode::Prescreen));
        let nf = prescreen
            .issues
            .iter()
            .find(|i| i.name == "Not feasible")
            .unwrap();
        assert_eq!(nf.hanlon_status, "screened_out");
        assert_eq!(nf.hanlon_rank, None);
        assert_eq!(prescreen.ranked_count, 1);
        assert_eq!(prescreen.screened_out_count, 1);

        let off = run(args(PearlMode::Off));
        let nf = off
            .issues
            .iter()
            .find(|i| i.name == "Not feasible")
            .unwrap();
        assert_eq!(nf.priority_score, Some(81.0));
        assert_eq!(nf.hanlon_rank, Some(1));
        assert_eq!(nf.pearl_status, "not used");
    }

    #[test]
    fn missing_ratings_are_listed_not_scored_as_zero() {
        let mut partial = issue("Partial", 5.0, 10.0, 5.0);
        partial.effectiveness = None;
        let mut no_pearl = issue("No PEARL", 5.0, 10.0, 5.0);
        no_pearl.pearl = Some(PearlAnswers {
            propriety: Some(true),
            economics: Some(true),
            ..Default::default()
        });
        let r = run(PrioritizeArgs {
            issues: vec![issue("Complete", 3.0, 3.0, 3.0), partial, no_pearl],
            ..Default::default()
        });
        assert_eq!(r.ranked_count, 1);
        assert_eq!(r.incomplete_count, 2);
        let p = r.issues.iter().find(|i| i.name == "Partial").unwrap();
        assert_eq!(p.hanlon_status, "incomplete");
        assert_eq!(p.missing, vec!["effectiveness (C)".to_string()]);
        assert_eq!(p.priority_score, None);
        // The basic rating cannot be computed without C.
        assert_eq!(p.basic_priority_rating, None);
        let n = r.issues.iter().find(|i| i.name == "No PEARL").unwrap();
        assert_eq!(n.hanlon_status, "incomplete");
        assert_eq!(
            n.missing,
            vec![
                "PEARL: Acceptability".to_string(),
                "PEARL: Resources".to_string(),
                "PEARL: Legality".to_string()
            ]
        );
        // The basic rating (before PEARL) is still shown: (5 + 10) x 5 / 3 = 25.
        assert_eq!(n.basic_priority_rating, Some(25.0));
        // Complete issues come first.
        assert_eq!(r.issues[0].name, "Complete");
    }

    #[test]
    fn seriousness_can_be_the_sum_of_four_factors() {
        let mut i = IssueInput {
            name: "Factors".into(),
            size: Some(5.0),
            urgency: Some(4.0),
            severity: Some(3.0),
            economic_loss: Some(2.0),
            impact_on_others: Some(1.0),
            effectiveness: Some(6.0),
            pearl: Some(pearl_yes()),
            ..Default::default()
        };
        // B = 4 + 3 + 2 + 1 = 10; BPR = (5 + 10) x 6 / 3 = 30
        let other = issue("Other", 1.0, 1.0, 1.0);
        let r = run(PrioritizeArgs {
            issues: vec![i.clone(), other.clone()],
            ..Default::default()
        });
        let f = r.issues.iter().find(|x| x.name == "Factors").unwrap();
        assert_eq!(f.seriousness, Some(10.0));
        assert_eq!(f.priority_score, Some(30.0));
        // Three of four factors: incomplete, naming the missing one.
        i.impact_on_others = None;
        let r = run(PrioritizeArgs {
            issues: vec![i, other],
            ..Default::default()
        });
        let f = r.issues.iter().find(|x| x.name == "Factors").unwrap();
        assert_eq!(f.hanlon_status, "incomplete");
        assert_eq!(
            f.missing,
            vec!["seriousness factor: effect on others".to_string()]
        );
    }

    #[test]
    fn nominal_group_scores_ranks_and_checks_consistency() {
        let mk = |name: &str, counts: Vec<u32>| IssueInput {
            name: name.into(),
            rank_counts: Some(counts),
            ..Default::default()
        };
        let r = run(PrioritizeArgs {
            issues: vec![
                mk("X", vec![4, 3, 1]),
                mk("Y", vec![3, 2, 4]),
                mk("Z", vec![2, 3, 3]),
                mk("W", vec![1, 2, 2]),
            ],
            nominal_group: Some(NominalGroup {
                participants: 10,
                ranks_per_participant: 3,
            }),
            ..Default::default()
        });
        let by = |n: &str| r.issues.iter().find(|i| i.name == n).unwrap();
        assert_eq!(by("X").nominal.as_ref().unwrap().points, 19);
        assert_eq!(by("Y").nominal.as_ref().unwrap().points, 17);
        assert_eq!(by("Z").nominal.as_ref().unwrap().points, 15);
        // 1 x 3 + 2 x 2 + 2 x 1 = 9
        assert_eq!(by("W").nominal.as_ref().unwrap().points, 9);
        assert_eq!(by("X").nominal_rank, Some(1));
        assert_eq!(by("W").nominal_rank, Some(4));
        assert_eq!(r.nominal_order, vec!["X", "Y", "Z", "W"]);
        assert_eq!(by("Y").nominal.as_ref().unwrap().participants_ranking_it, 9);
        assert_eq!(by("X").nominal.as_ref().unwrap().first_choice, 4);
        // Only the vote was given: the issues are not Hanlon-scored and that is not an error.
        assert_eq!(r.ranked_count, 0);
        assert_eq!(by("X").hanlon_status, "not_scored");

        // Rank 1 given 11 times by 10 participants is impossible.
        let (_d, ctx) = ctx();
        let bad = prioritize_health_issues(
            &ctx,
            PrioritizeArgs {
                issues: vec![mk("A", vec![6]), mk("B", vec![5])],
                nominal_group: Some(NominalGroup {
                    participants: 10,
                    ranks_per_participant: 1,
                }),
                write_document: Some(false),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(bad.to_string().contains("only 10 participants"), "{bad}");
    }

    #[test]
    fn nominal_ties_and_missing_votes_are_explicit() {
        let mk = |name: &str, counts: Option<Vec<u32>>| IssueInput {
            name: name.into(),
            rank_counts: counts,
            ..Default::default()
        };
        let r = run(PrioritizeArgs {
            issues: vec![
                mk("A", Some(vec![2, 0])), // 2 x 2 = 4
                mk("B", Some(vec![0, 4])), // 4 x 1 = 4
                mk("C", Some(vec![])),     // 0 votes: ranked last at 0
                mk("D", None),             // not in the vote
            ],
            nominal_group: Some(NominalGroup {
                participants: 4,
                ranks_per_participant: 2,
            }),
            ..Default::default()
        });
        let by = |n: &str| r.issues.iter().find(|i| i.name == n).unwrap();
        assert_eq!(by("A").nominal_rank, Some(1));
        assert_eq!(by("B").nominal_rank, Some(1));
        assert_eq!(by("A").nominal_tied_with, vec!["B".to_string()]);
        assert_eq!(by("C").nominal_rank, Some(3));
        assert_eq!(by("C").nominal.as_ref().unwrap().points, 0);
        assert_eq!(by("D").nominal_rank, None);
        assert!(by("D").nominal.is_none());
    }

    #[test]
    fn methods_are_shown_side_by_side_and_disagreements_flagged() {
        let mut a = issue("Alpha", 9.0, 18.0, 9.0); // 81
        a.rank_counts = Some(vec![0, 0, 0, 0, 1]);
        let mut b = issue("Beta", 1.0, 2.0, 3.0); // 3
        b.rank_counts = Some(vec![5, 0, 0, 0, 0]);
        let r = run(PrioritizeArgs {
            issues: vec![a, b],
            nominal_group: Some(NominalGroup {
                participants: 5,
                ranks_per_participant: 5,
            }),
            ..Default::default()
        });
        // Hanlon: Alpha 1, Beta 2; nominal: Beta (25 points) 1, Alpha (1 point) 2: a gap of 1,
        // not flagged.
        assert!(r.method_disagreements.is_empty());
        let alpha = r.issues.iter().find(|i| i.name == "Alpha").unwrap();
        assert_eq!(alpha.hanlon_rank, Some(1));
        assert_eq!(alpha.nominal_rank, Some(2));
    }

    #[test]
    fn grid_quadrants_follow_the_threshold() {
        let r = run(PrioritizeArgs {
            issues: vec![
                issue("Big and doable", 8.0, 16.0, 8.0), // importance 0.80, feasibility 0.8
                issue("Big and hard", 8.0, 16.0, 2.0),   // 0.80, 0.2
                issue("Small and doable", 1.0, 3.0, 9.0), // 0.13, 0.9
                issue("Small and hard", 1.0, 3.0, 1.0),  // 0.13, 0.1
            ],
            ..Default::default()
        });
        let q = |n: &str| {
            r.issues
                .iter()
                .find(|i| i.name == n)
                .unwrap()
                .grid
                .as_ref()
                .unwrap()
                .quadrant
                .clone()
        };
        assert_eq!(q("Big and doable"), "Act now");
        assert_eq!(q("Big and hard"), "Important but hard");
        assert_eq!(q("Small and doable"), "Quick win");
        assert_eq!(q("Small and hard"), "Revisit later");
    }

    #[test]
    fn close_calls_are_flagged() {
        let r = run(PrioritizeArgs {
            issues: vec![
                issue("One", 8.0, 14.0, 6.0),  // 44
                issue("Two", 8.0, 14.0, 5.9),  // 22 x 5.9 / 3 = 43.2666...
                issue("Three", 2.0, 2.0, 2.0), // 2.666...
            ],
            ..Default::default()
        });
        assert_eq!(r.close_calls.len(), 1, "{:?}", r.close_calls);
        assert!(r.close_calls[0].starts_with("One (44.00) and Two (43.27)"));
    }

    #[test]
    fn rejects_bad_input_with_helpful_messages() {
        let (_d, ctx) = ctx();
        let call = |issues: Vec<IssueInput>| {
            prioritize_health_issues(
                &ctx,
                PrioritizeArgs {
                    issues,
                    write_document: Some(false),
                    ..Default::default()
                },
            )
            .unwrap_err()
            .to_string()
        };
        assert!(call(vec![issue("Only one", 1.0, 1.0, 1.0)]).contains("at least two"));
        assert!(
            call(vec![
                issue("Same", 1.0, 1.0, 1.0),
                issue("  same ", 2.0, 2.0, 2.0)
            ])
            .contains("two issues are named")
        );
        assert!(
            call(vec![issue("A", 11.0, 1.0, 1.0), issue("B", 1.0, 1.0, 1.0)])
                .contains("size is 11")
        );
        assert!(
            call(vec![issue("A", 1.0, 21.0, 1.0), issue("B", 1.0, 1.0, 1.0)])
                .contains("seriousness is 21")
        );
        assert!(
            call(vec![issue("A", 1.0, 1.0, -1.0), issue("B", 1.0, 1.0, 1.0)])
                .contains("effectiveness is -1")
        );
        assert!(
            call(vec![
                issue("A", f64::NAN, 1.0, 1.0),
                issue("B", 1.0, 1.0, 1.0)
            ])
            .contains("must be a number")
        );
        let mut both = issue("A", 1.0, 1.0, 1.0);
        both.urgency = Some(1.0);
        assert!(call(vec![both, issue("B", 1.0, 1.0, 1.0)]).contains("not both"));
        let mut unnamed = issue("", 1.0, 1.0, 1.0);
        unnamed.name = "   ".into();
        assert!(call(vec![unnamed, issue("B", 1.0, 1.0, 1.0)]).contains("has no name"));
        // Nothing to score.
        let e = call(vec![
            IssueInput {
                name: "A".into(),
                size: Some(1.0),
                ..Default::default()
            },
            IssueInput {
                name: "B".into(),
                size: Some(2.0),
                ..Default::default()
            },
        ]);
        assert!(e.contains("no issue could be scored"), "{e}");
        // Votes without a setup.
        let e = call(vec![
            IssueInput {
                name: "A".into(),
                rank_counts: Some(vec![1]),
                ..Default::default()
            },
            issue("B", 1.0, 1.0, 1.0),
        ]);
        assert!(e.contains("need nominal_group"), "{e}");
    }

    #[test]
    fn non_ascii_names_and_long_text_are_handled_by_characters() {
        let r = run(PrioritizeArgs {
            issues: vec![
                issue("Salud mental en adolescentes 🧠", 5.0, 10.0, 5.0),
                issue("Acceso a vivienda — costo", 6.0, 10.0, 5.0),
            ],
            ..Default::default()
        });
        assert_eq!(r.ranked_count, 2);
        let (_d, ctx) = ctx();
        let long = "é".repeat(MAX_NAME_CHARS + 1);
        let e = prioritize_health_issues(
            &ctx,
            PrioritizeArgs {
                issues: vec![issue(&long, 1.0, 1.0, 1.0), issue("B", 1.0, 1.0, 1.0)],
                write_document: Some(false),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("longer than 200"), "{e}");
    }

    #[test]
    fn writes_a_document_without_returning_the_rationale_of_other_people() {
        let (dir, ctx) = ctx();
        let mut a = issue("Housing cost burden", 7.0, 12.0, 5.0);
        a.rationale = Some("ACS shows 38% of renters pay 30% or more of income on housing.".into());
        let env = prioritize_health_issues(
            &ctx,
            PrioritizeArgs {
                issues: vec![a, issue("Food insecurity", 5.0, 9.0, 6.0)],
                area: Some("Springfield".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let file = env.data.document.clone().expect("a document");
        assert!(file.starts_with("output/health_issue_priorities"), "{file}");
        assert!(file.ends_with(".docx"));
        assert!(dir.path().join("data").join(&file).exists());
        assert_eq!(env.privacy.files_written, vec![file]);
        assert!(!env.privacy.row_level_data);
        assert!(env.to_markdown().contains("Hanlon"));
    }
}
