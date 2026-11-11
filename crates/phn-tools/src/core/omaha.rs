//! CORE-01 `omaha_system_coder`: maps a visit narrative or care plan to
//! Omaha System problems (with domain, modifiers, and signs/symptoms),
//! interventions (category and target), and the Knowledge-Behavior-Status
//! rating scale. The narrative is analyzed locally and never returned: the
//! result holds codes and the vocabulary terms that matched.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::core::practice::{self, CitedNote, CitedText, ContentSource};
use crate::md;

/// Content id.
pub const OMAHA_CONTENT: &str = "omaha-system";

#[derive(Debug, Clone, Deserialize)]
struct DomainDef {
    id: String,
    label: String,
    definition: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Labeled {
    id: String,
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Modifiers {
    scope: Vec<Labeled>,
    #[serde(rename = "type")]
    kind: Vec<Labeled>,
    note: CitedText,
}

#[derive(Debug, Clone, Deserialize)]
struct Problem {
    code: String,
    name: String,
    domain: String,
    signs_symptoms_verified: Vec<String>,
    signs_symptoms_draft: Vec<String>,
    cues: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Category {
    code: String,
    name: String,
    definition: String,
    cues: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Target {
    code: String,
    name: String,
    cues: Vec<String>,
}

/// A rating anchor.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct RatingAnchor {
    pub value: u8,
    pub label: String,
    /// Seen in the omahasystem.org case studies (others are from the 2005 book, unverified).
    pub verified: bool,
}

/// A Knowledge, Behavior, or Status scale.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct Concept {
    pub id: String,
    pub label: String,
    pub definition: String,
    pub ratings: Vec<RatingAnchor>,
}

#[derive(Debug, Clone, Deserialize)]
struct RatingScale {
    concepts: Vec<Concept>,
    note: CitedText,
}

#[derive(Debug, Clone, Deserialize)]
struct OmahaContent {
    meta: ContentMeta,
    sources: BTreeMap<String, ContentSource>,
    license: CitedText,
    domains: Vec<DomainDef>,
    modifiers: Modifiers,
    problems: Vec<Problem>,
    intervention_categories: Vec<Category>,
    targets: Vec<Target>,
    rating_scale: RatingScale,
}

static CONTENT: LazyLock<OmahaContent> =
    LazyLock::new(|| practice::load(OMAHA_CONTENT).expect("Omaha content matches the Rust types"));

fn content() -> &'static OmahaContent {
    &CONTENT
}

/// A nurse's Knowledge-Behavior-Status rating for one problem.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct KbsRating {
    /// Problem name or code ("Medication regimen" or "42").
    pub problem: String,
    /// Knowledge 1-5.
    pub knowledge: Option<u8>,
    /// Behavior 1-5.
    pub behavior: Option<u8>,
    /// Status 1-5.
    pub status: Option<u8>,
    /// Earlier (for example admission) Knowledge rating, to show change.
    pub baseline_knowledge: Option<u8>,
    pub baseline_behavior: Option<u8>,
    pub baseline_status: Option<u8>,
}

/// Arguments for `omaha_system_coder`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct OmahaArgs {
    /// Visit narrative or care plan text to code. It is analyzed and never
    /// returned; prefer `path` for notes that contain identifiers.
    pub text: Option<String>,
    /// A note file in /data (TXT, MD, DOCX, PDF, HTML) to code instead of `text`.
    pub path: Option<String>,
    /// Look up a problem, target, or intervention category by name or code
    /// (for example "medication regimen", "42", "case management").
    pub lookup: Option<String>,
    /// Knowledge-Behavior-Status ratings to check and label (1-5 each).
    pub ratings: Vec<KbsRating>,
    /// Most problems to return (default 8).
    pub max_problems: Option<usize>,
    /// Write a blank Omaha System care plan (DOCX) for the suggested problems
    /// to /data/output.
    pub write_care_plan: bool,
}

/// An intervention suggestion (category and target).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct InterventionMatch {
    pub category_code: String,
    pub category: String,
    pub target_code: String,
    pub target: String,
    /// Sentences supporting the pair.
    pub sentences: usize,
}

/// A suggested problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProblemMatch {
    pub code: String,
    pub name: String,
    pub domain: String,
    /// Sentences that mention the problem.
    pub sentences: usize,
    /// Vocabulary terms (this tool's cues) that matched; never text from the note.
    pub matched_terms: Vec<String>,
    /// Cue terms that appeared only in negated phrases ("denies ...").
    pub negated_terms: Vec<String>,
    /// individual, family, or community (suggestion; confirm).
    pub suggested_scope: String,
    /// health_promotion, potential, or actual (suggestion; confirm).
    pub suggested_type: String,
    /// Omaha signs/symptoms for this problem seen on omahasystem.org.
    pub signs_symptoms: Vec<String>,
    /// Further 2005 signs/symptoms (developer draft; verify in the book).
    pub signs_symptoms_unverified: Vec<String>,
    /// Interventions found in the same sentences.
    pub interventions: Vec<InterventionMatch>,
}

/// A problem whose cue terms appeared only after a negation word (no, not,
/// denies, without, ...). It may be ruled out ("denies fever") or it may be
/// the problem itself ("no income", "not taking insulin"): the nurse decides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NegatedProblem {
    pub code: String,
    pub name: String,
    pub domain: String,
    /// Sentences where the problem's terms appeared only in negated phrases.
    pub sentences: usize,
    /// Vocabulary terms (this tool's cues) that were negated; never text from the note.
    pub negated_terms: Vec<String>,
}

/// A looked-up term.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LookupHit {
    /// problem, target, or category.
    pub kind: String,
    pub code: String,
    pub name: String,
    pub detail: String,
    pub signs_symptoms: Vec<String>,
}

/// A labeled rating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RatingOut {
    pub problem_code: String,
    pub problem: String,
    /// Concept -> "3 (Basic knowledge)".
    pub ratings: BTreeMap<String, String>,
    /// Concept -> change from baseline (+1, -2, ...).
    pub change: BTreeMap<String, i32>,
}

/// An intervention category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoryInfo {
    pub code: String,
    pub name: String,
    pub definition: String,
}

/// Result of `omaha_system_coder`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OmahaResult {
    /// The note file analyzed, if any.
    pub source_file: Option<String>,
    pub sentences_analyzed: usize,
    pub problems: Vec<ProblemMatch>,
    /// Problems mentioned only as absent or negated ("no income", "denies fever"). Review
    /// each: an absent need is often the problem itself. They are not in `problems`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentioned_as_absent_or_negated: Vec<NegatedProblem>,
    /// Interventions found in sentences without a problem cue.
    pub unlinked_interventions: Vec<InterventionMatch>,
    pub lookup: Vec<LookupHit>,
    pub ratings: Vec<RatingOut>,
    pub rating_scale: Vec<Concept>,
    pub rating_scale_note: CitedNote,
    pub intervention_categories: Vec<CategoryInfo>,
    pub modifiers_note: CitedNote,
    pub license: CitedNote,
    pub care_plan: Option<String>,
}

const NEGATIONS: &[&str] = &[
    "no", "denies", "denied", "without", "not", "never", "negative", "none", "nor",
];

/// Whole-word (or prefix, for cues ending in '*') match positions of `cue`
/// in padded words `s`, and whether every occurrence is negated.
fn cue_hit(s: &str, cue: &str) -> Option<bool> {
    let c = practice::padded_words(cue);
    let c = c.trim();
    if c.is_empty() {
        return None;
    }
    let needle = format!(" {c} ");
    let mut found = false;
    let mut all_negated = true;
    let mut start = 0;
    while let Some(pos) = s[start..].find(&needle) {
        let at = start + pos;
        found = true;
        let before: Vec<&str> = s[..at].split_whitespace().rev().take(3).collect();
        if !before.iter().any(|w| NEGATIONS.contains(w)) {
            all_negated = false;
        }
        start = at + 1;
    }
    found.then_some(all_negated)
}

fn sentences(text: &str) -> Vec<String> {
    text.split(['.', '!', '?', ';', '\n'])
        .map(practice::padded_words)
        .filter(|s| s.split_whitespace().count() >= 2)
        .collect()
}

fn find_problem(q: &str) -> Option<&'static Problem> {
    let c = content();
    let n = phn_core::text::normalize(q);
    c.problems
        .iter()
        .find(|p| p.code == q.trim() || p.code == format!("{:0>2}", q.trim()))
        .or_else(|| {
            c.problems
                .iter()
                .find(|p| phn_core::text::normalize(&p.name) == n)
        })
        .or_else(|| {
            c.problems
                .iter()
                .map(|p| {
                    (
                        p,
                        phn_core::text::similarity(&n, &phn_core::text::normalize(&p.name)),
                    )
                })
                .filter(|(_, s)| *s >= 0.75)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(p, _)| p)
        })
}

fn label_rating(concept: &str, v: u8) -> Result<String> {
    if !(1..=5).contains(&v) {
        return Err(PhnError::InvalidInput(format!(
            "{concept} ratings run from 1 to 5"
        )));
    }
    let c = content();
    let anchor = c
        .rating_scale
        .concepts
        .iter()
        .find(|k| k.id == concept)
        .and_then(|k| k.ratings.iter().find(|r| r.value == v));
    Ok(match anchor {
        Some(a) => format!("{v} ({})", a.label),
        None => v.to_string(),
    })
}

/// Code a narrative with the Omaha System.
pub fn omaha_system_coder(ctx: &Context, args: OmahaArgs) -> Result<Envelope<OmahaResult>> {
    let c = content();
    let max = args.max_problems.unwrap_or(8).clamp(1, 42);
    let mut caveats: Vec<String> = practice::review_caveat(&c.meta).into_iter().collect();
    let mut sources: Vec<Source> = practice::sources_for(
        &c.sources,
        [
            "omaha-overview",
            "omaha-pcs",
            "omaha-is",
            "omaha-prso",
            "omaha-faq",
        ],
    );

    // ---- Text to analyze (never returned)
    let (text, source_file) = match (&args.text, &args.path) {
        (_, Some(p)) if !p.trim().is_empty() => {
            let (t, label) = practice::read_local_note(ctx, p)?;
            (Some(t), Some(label))
        }
        (Some(t), _) if !t.trim().is_empty() => (Some(t.clone()), None),
        _ => (None, None),
    };
    if let Some(f) = &source_file {
        sources.insert(0, Source::local_file(f));
    }

    // ---- Coding
    let mut problems: Vec<ProblemMatch> = Vec::new();
    let mut negated_only: Vec<NegatedProblem> = Vec::new();
    let mut unlinked: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut n_sent = 0;
    if let Some(t) = &text {
        let sents = sentences(t);
        n_sent = sents.len();
        // problem index -> (sentences, cues, negated cues)
        let mut hits: BTreeMap<usize, (usize, Vec<String>, Vec<String>)> = BTreeMap::new();
        // problem index -> (category, target) -> sentences
        let mut pair_hits: BTreeMap<usize, BTreeMap<(usize, usize), usize>> = BTreeMap::new();
        // problem index -> sentences where its cues appeared only negated
        let mut neg_sentences: BTreeMap<usize, usize> = BTreeMap::new();
        let mut family = vec![false; c.problems.len()];
        let mut risk = vec![false; c.problems.len()];
        let mut promotion = vec![false; c.problems.len()];
        for s in &sents {
            let mut in_sentence: Vec<usize> = Vec::new();
            for (pi, p) in c.problems.iter().enumerate() {
                let mut matched = false;
                let mut negated_here = false;
                for cue in &p.cues {
                    if let Some(neg) = cue_hit(s, cue) {
                        let e = hits.entry(pi).or_default();
                        if neg {
                            negated_here = true;
                            if !e.2.contains(cue) && !e.1.contains(cue) {
                                e.2.push(cue.clone());
                            }
                        } else {
                            matched = true;
                            if !e.1.contains(cue) {
                                e.1.push(cue.clone());
                            }
                            e.2.retain(|x| x != cue);
                        }
                    }
                }
                if !matched && negated_here {
                    *neg_sentences.entry(pi).or_default() += 1;
                }
                if matched {
                    hits.entry(pi).or_default().0 += 1;
                    in_sentence.push(pi);
                    let has = |w: &[&str]| w.iter().any(|x| s.contains(&format!(" {x} ")));
                    family[pi] |= has(&[
                        "family",
                        "mother",
                        "father",
                        "parent",
                        "parents",
                        "household",
                        "children",
                        "spouse",
                        "husband",
                        "wife",
                        "siblings",
                    ]);
                    risk[pi] |= has(&["risk", "potential", "exposed", "exposure"]);
                    promotion[pi] |= has(&["wants", "interested", "goal", "would like", "plans"]);
                }
            }
            let cats: Vec<usize> = c
                .intervention_categories
                .iter()
                .enumerate()
                .filter(|(_, k)| k.cues.iter().any(|q| cue_hit(s, q) == Some(false)))
                .map(|(i, _)| i)
                .collect();
            if cats.is_empty() {
                continue;
            }
            let targets: Vec<usize> = c
                .targets
                .iter()
                .enumerate()
                .filter(|(_, t)| {
                    t.cues.iter().any(|q| cue_hit(s, q) == Some(false))
                        || (t.name != "other" && cue_hit(s, &t.name) == Some(false))
                })
                .map(|(i, _)| i)
                .collect();
            for &k in &cats {
                for &t in &targets {
                    if in_sentence.is_empty() {
                        let key = (
                            c.intervention_categories[k].code.clone(),
                            c.targets[t].code.clone(),
                        );
                        *unlinked.entry(key).or_default() += 1;
                    }
                    for &pi in &in_sentence {
                        *pair_hits.entry(pi).or_default().entry((k, t)).or_default() += 1;
                    }
                }
            }
        }
        let domain_label = |id: &str| {
            c.domains
                .iter()
                .find(|d| d.id == id)
                .map(|d| d.label.clone())
                .unwrap_or_default()
        };
        let mut ranked: Vec<(usize, usize, Vec<String>, Vec<String>)> = hits
            .into_iter()
            .map(|(pi, (n, cues, neg))| (pi, n, cues, neg))
            .collect();
        ranked.sort_by(|a, b| (b.1, b.2.len()).cmp(&(a.1, a.2.len())).then(a.0.cmp(&b.0)));
        // Problems whose cues appeared only after a negation word are not dropped:
        // they are listed apart for the nurse to review.
        for (pi, _, _, neg) in ranked.iter().filter(|r| r.1 == 0 && !r.3.is_empty()) {
            let p = &c.problems[*pi];
            negated_only.push(NegatedProblem {
                code: p.code.clone(),
                name: p.name.clone(),
                domain: domain_label(&p.domain),
                sentences: neg_sentences.get(pi).copied().unwrap_or(0),
                negated_terms: neg.clone(),
            });
        }
        for (pi, n, cues, neg) in ranked.into_iter().filter(|r| r.1 > 0).take(max) {
            let p = &c.problems[pi];
            let mut ints: Vec<InterventionMatch> = pair_hits
                .get(&pi)
                .map(|m| {
                    m.iter()
                        .map(|((k, t), n)| InterventionMatch {
                            category_code: c.intervention_categories[*k].code.clone(),
                            category: c.intervention_categories[*k].name.clone(),
                            target_code: c.targets[*t].code.clone(),
                            target: c.targets[*t].name.clone(),
                            sentences: *n,
                        })
                        .collect()
                })
                .unwrap_or_default();
            ints.sort_by(|a, b| b.sentences.cmp(&a.sentences).then(a.cmp(b)));
            ints.truncate(8);
            problems.push(ProblemMatch {
                code: p.code.clone(),
                name: p.name.clone(),
                domain: domain_label(&p.domain),
                sentences: n,
                matched_terms: cues,
                negated_terms: neg,
                suggested_scope: if family[pi] { "family" } else { "individual" }.into(),
                suggested_type: if risk[pi] && p.signs_symptoms_verified.is_empty() {
                    "potential"
                } else if promotion[pi] && !risk[pi] {
                    "health_promotion"
                } else {
                    "actual"
                }
                .into(),
                signs_symptoms: p.signs_symptoms_verified.clone(),
                signs_symptoms_unverified: p.signs_symptoms_draft.clone(),
                interventions: ints,
            });
        }
        if problems.is_empty() && negated_only.is_empty() {
            caveats.push(
                "No Omaha System problem cues were found in the text; try a fuller narrative or \
                 use lookup to browse problems."
                    .into(),
            );
        }
        if !negated_only.is_empty() {
            caveats.push(
                "Problems listed as mentioned as absent or negated had their terms only after \
                 words such as no, not, denies, or without. Decide for each whether the note \
                 rules the problem out (denies fever) or describes it (no income, not taking a \
                 medication)."
                    .into(),
            );
        }
    }
    let unlinked_interventions: Vec<InterventionMatch> = unlinked
        .into_iter()
        .filter_map(|((kc, tc), n)| {
            let k = c.intervention_categories.iter().find(|x| x.code == kc)?;
            let t = c.targets.iter().find(|x| x.code == tc)?;
            Some(InterventionMatch {
                category_code: kc,
                category: k.name.clone(),
                target_code: tc,
                target: t.name.clone(),
                sentences: n,
            })
        })
        .collect();

    // ---- Lookup
    let mut lookup = Vec::new();
    if let Some(q) = args.lookup.as_deref().filter(|q| !q.trim().is_empty()) {
        let n = phn_core::text::normalize(q);
        if let Some(p) = find_problem(q) {
            lookup.push(LookupHit {
                kind: "problem".into(),
                code: p.code.clone(),
                name: p.name.clone(),
                detail: c
                    .domains
                    .iter()
                    .find(|d| d.id == p.domain)
                    .map(|d| format!("{} domain: {}", d.label, d.definition))
                    .unwrap_or_default(),
                signs_symptoms: p
                    .signs_symptoms_verified
                    .iter()
                    .cloned()
                    .chain(
                        p.signs_symptoms_draft
                            .iter()
                            .map(|s| format!("{s} (unverified)")),
                    )
                    .collect(),
            });
        }
        for k in &c.intervention_categories {
            if phn_core::text::normalize(&k.name).contains(&n) || k.code == q.trim() {
                lookup.push(LookupHit {
                    kind: "category".into(),
                    code: k.code.clone(),
                    name: k.name.clone(),
                    detail: k.definition.clone(),
                    signs_symptoms: Vec::new(),
                });
            }
        }
        for t in &c.targets {
            if n.len() >= 3 && phn_core::text::normalize(&t.name).contains(&n) {
                lookup.push(LookupHit {
                    kind: "target".into(),
                    code: t.code.clone(),
                    name: t.name.clone(),
                    detail: "Intervention Scheme target".into(),
                    signs_symptoms: Vec::new(),
                });
            }
        }
        if lookup.is_empty() {
            return Err(PhnError::NotFound(format!(
                "nothing in the Omaha System matches '{q}'. Problems: {}",
                c.problems
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }

    // ---- Ratings
    let mut ratings_out = Vec::new();
    for r in &args.ratings {
        let p = find_problem(&r.problem).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{}' is not an Omaha System problem name or code (01-42)",
                r.problem
            ))
        })?;
        let mut m = BTreeMap::new();
        let mut change = BTreeMap::new();
        for (concept, v, base) in [
            ("knowledge", r.knowledge, r.baseline_knowledge),
            ("behavior", r.behavior, r.baseline_behavior),
            ("status", r.status, r.baseline_status),
        ] {
            if let Some(v) = v {
                m.insert(concept.to_string(), label_rating(concept, v)?);
                if let Some(b) = base {
                    label_rating(concept, b)?;
                    change.insert(concept.to_string(), i32::from(v) - i32::from(b));
                }
            }
        }
        ratings_out.push(RatingOut {
            problem_code: p.code.clone(),
            problem: p.name.clone(),
            ratings: m,
            change,
        });
    }
    if text.is_none() && args.lookup.is_none() && args.ratings.is_empty() {
        return Err(PhnError::InvalidInput(
            "give a narrative (text or path), a lookup term, or ratings".into(),
        ));
    }

    // ---- Care plan
    let mut care_plan = None;
    let mut files = Vec::new();
    if args.write_care_plan {
        let chosen: Vec<&Problem> = if problems.is_empty() {
            lookup
                .iter()
                .filter(|h| h.kind == "problem")
                .filter_map(|h| c.problems.iter().find(|p| p.code == h.code))
                .collect()
        } else {
            problems
                .iter()
                .filter_map(|m| c.problems.iter().find(|p| p.code == m.code))
                .collect()
        };
        if chosen.is_empty() {
            return Err(PhnError::InvalidInput(
                "no problems to put in a care plan; give a narrative or a problem lookup".into(),
            ));
        }
        let mut blocks = vec![
            practice::para(
                "Client: ____________________   Clinician: ____________________   Date: __________",
            ),
            practice::para(
                "Draft generated from Omaha System terms; confirm each problem, modifier, sign \
                 and symptom, and intervention, and add client-specific information.",
            ),
        ];
        for p in chosen {
            blocks.push(practice::heading(2, format!("{} {} ", p.code, p.name)));
            blocks.push(practice::para(
                "Modifiers: ☐ Individual ☐ Family ☐ Community    ☐ Health promotion ☐ Potential ☐ Actual",
            ));
            let ss: Vec<String> = p
                .signs_symptoms_verified
                .iter()
                .chain(&p.signs_symptoms_draft)
                .map(|s| format!("☐ {s}"))
                .collect();
            if !ss.is_empty() {
                blocks.push(practice::para("Signs/symptoms (if actual):"));
                blocks.push(practice::bullets(ss));
            }
            let suggested: Vec<Vec<String>> = problems
                .iter()
                .find(|m| m.code == p.code)
                .map(|m| {
                    m.interventions
                        .iter()
                        .map(|i| vec![i.category.clone(), i.target.clone(), String::new()])
                        .collect()
                })
                .unwrap_or_default();
            let mut rows = suggested;
            rows.extend((0..3).map(|_| vec![String::new(); 3]));
            blocks.push(practice::table(
                "Interventions",
                &["Category", "Target", "Client-specific information"],
                rows,
            ));
            blocks.push(practice::table(
                "Problem Rating Scale for Outcomes (1-5)",
                &["Concept", "Admission", "Interim", "Dismissal"],
                vec![
                    vec![
                        "Knowledge".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ],
                    vec![
                        "Behavior".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ],
                    vec!["Status".into(), String::new(), String::new(), String::new()],
                ],
            ));
        }
        blocks.push(practice::para(
            "Source: The Omaha System (public domain), omahasystem.org; Martin KS, The Omaha \
             System: A Key to Practice, Documentation, and Information Management (2005).",
        ));
        let doc = practice::document(ctx, "Omaha System care plan (draft)", blocks);
        let written = practice::write_document(ctx, &doc, "omaha_care_plan", &["docx"])?;
        care_plan = written.first().cloned();
        files = written;
    }

    // ---- Markdown
    let mut s = String::from("**Omaha System coding**\n\n");
    if text.is_some() {
        s.push_str(&format!(
            "{} sentence(s) analyzed{}; the text is not repeated here.\n\n",
            n_sent,
            source_file
                .as_ref()
                .map(|f| format!(" from `{f}`"))
                .unwrap_or_default()
        ));
        if !problems.is_empty() {
            let rows: Vec<Vec<String>> = problems
                .iter()
                .map(|p| {
                    vec![
                        format!("{} {}", p.code, p.name),
                        p.domain.clone(),
                        format!(
                            "{}, {}",
                            p.suggested_scope,
                            p.suggested_type.replace('_', " ")
                        ),
                        p.matched_terms.join(", "),
                        p.interventions
                            .iter()
                            .take(4)
                            .map(|i| format!("{}: {}", short_cat(&i.category), i.target))
                            .collect::<Vec<_>>()
                            .join("; "),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &[
                    "Problem",
                    "Domain",
                    "Modifiers (suggested)",
                    "Matched terms",
                    "Interventions (category: target)",
                ],
                &rows,
            ));
        }
        if !negated_only.is_empty() {
            s.push_str(&format!(
                "\n**Mentioned as absent or negated — review** (an absent need, such as no \
                 income or not taking a medication, is often the problem itself): {}.\n",
                negated_only
                    .iter()
                    .map(|p| format!("{} {} ({})", p.code, p.name, p.negated_terms.join(", ")))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !unlinked_interventions.is_empty() {
            s.push_str(&format!(
                "\nOther interventions found: {}.\n",
                unlinked_interventions
                    .iter()
                    .take(8)
                    .map(|i| format!("{}: {}", short_cat(&i.category), i.target))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    for h in &lookup {
        s.push_str(&format!(
            "\n**{} {} ({})** — {}\n",
            h.code, h.name, h.kind, h.detail
        ));
        if !h.signs_symptoms.is_empty() {
            s.push_str(&format!(
                "Signs/symptoms: {}\n",
                h.signs_symptoms.join("; ")
            ));
        }
    }
    if !ratings_out.is_empty() {
        s.push_str("\n**Ratings (Problem Rating Scale for Outcomes)**\n\n");
        let rows: Vec<Vec<String>> = ratings_out
            .iter()
            .map(|r| {
                let cell = |k: &str| {
                    let v = r.ratings.get(k).cloned().unwrap_or_else(|| "—".into());
                    match r.change.get(k) {
                        Some(d) => format!("{v} ({d:+})"),
                        None => v,
                    }
                };
                vec![
                    format!("{} {}", r.problem_code, r.problem),
                    cell("knowledge"),
                    cell("behavior"),
                    cell("status"),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Problem", "Knowledge", "Behavior", "Status"],
            &rows,
        ));
    }
    s.push_str(
        "\nRate Knowledge (what the client knows), Behavior (what the client does), and Status \
         (signs and symptoms) from 1 to 5 for each problem at admission, interim points, and \
         dismissal.\n",
    );
    if let Some(f) = &care_plan {
        s.push_str(&format!(
            "\n**Care plan draft:** `{f}` (blank for client details).\n"
        ));
    }
    caveats.push(
        "Suggestions come from keyword cues and need the nurse's judgment: confirm each problem, \
         its modifiers and signs/symptoms, and each intervention before documenting. Terms must \
         be used as published in the 2005 Omaha System book."
            .into(),
    );
    if problems
        .iter()
        .any(|p| !p.signs_symptoms_unverified.is_empty())
    {
        caveats.push(
            "Signs/symptoms listed as unverified are the developer's draft of the 2005 terms; \
             only those seen on omahasystem.org are verified. Ratings 4 and 5 labels are also \
             unverified."
                .into(),
        );
    }
    let mut privacy = Privacy {
        notes: vec![
            "The narrative was analyzed locally and is not returned; only Omaha System codes, \
             terms, and counts are."
                .into(),
        ],
        files_written: files,
        ..Privacy::none()
    };
    if source_file.is_some() {
        privacy
            .notes
            .push("The note file was read from /data and left unchanged.".into());
    }
    let data = OmahaResult {
        source_file,
        sentences_analyzed: n_sent,
        problems,
        mentioned_as_absent_or_negated: negated_only,
        unlinked_interventions,
        lookup,
        ratings: ratings_out,
        rating_scale: c.rating_scale.concepts.clone(),
        rating_scale_note: practice::note(&c.rating_scale.note, &c.sources),
        intervention_categories: c
            .intervention_categories
            .iter()
            .map(|k| CategoryInfo {
                code: k.code.clone(),
                name: k.name.clone(),
                definition: k.definition.clone(),
            })
            .collect(),
        modifiers_note: CitedNote {
            text: format!(
                "{} Scope: {}. Type: {}.",
                c.modifiers.note.text,
                c.modifiers
                    .scope
                    .iter()
                    .map(|m| format!("{} ({})", m.label, m.id))
                    .collect::<Vec<_>>()
                    .join(", "),
                c.modifiers
                    .kind
                    .iter()
                    .map(|m| format!("{} ({})", m.label, m.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            citation: practice::cite(
                &c.sources,
                &c.modifiers.note.source,
                &c.modifiers.note.section,
            ),
            confidence: None,
        },
        license: practice::note(&c.license, &c.sources),
        care_plan,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn short_cat(name: &str) -> &'static str {
    match name {
        "Teaching, Guidance, and Counseling" => "TGC",
        "Treatments and Procedures" => "TP",
        "Case Management" => "CM",
        _ => "S",
    }
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

    const NOTE: &str = "Home visit with client on LTBI treatment with rifampin. Client reports \
        missed doses last week and has no pill box; taught medication schedule and side effects, \
        set up a pill box. Client denies fever. Lost job last month and cannot pay rent; \
        referred to Mass 211 for fuel assistance and food pantry. Monitored for jaundice.";

    #[test]
    fn codes_problems_and_interventions_without_echoing_text() {
        let (_d, c) = ctx(false);
        let env = omaha_system_coder(
            &c,
            OmahaArgs {
                text: Some(NOTE.into()),
                ..OmahaArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        let med = d
            .problems
            .iter()
            .find(|p| p.name == "Medication regimen")
            .expect("medication regimen");
        assert_eq!(med.code, "42");
        assert!(
            med.interventions
                .iter()
                .any(|i| i.category.starts_with("Teaching")
                    && i.target == "medication action/side effects")
        );
        assert!(d.problems.iter().any(|p| p.name == "Income"));
        // "denies fever" is negated.
        let cd = d
            .problems
            .iter()
            .find(|p| p.name == "Communicable/infectious condition");
        assert!(cd.is_none_or(|p| !p.matched_terms.contains(&"fever".to_string())));
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("Lost job last month"));
        assert!(!json.contains("cannot pay rent;"));
    }

    #[test]
    fn negated_only_problems_are_listed_for_review_not_dropped() {
        let (_d, c) = ctx(false);
        let env = omaha_system_coder(
            &c,
            OmahaArgs {
                text: Some("Client has no income and is not taking her insulin.".into()),
                ..OmahaArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        // "no income" and "not taking her insulin" are the problems themselves:
        // they are kept apart for review, not suggested as confirmed and not lost.
        assert!(!d.problems.iter().any(|p| p.code == "01" || p.code == "42"));
        let income = d
            .mentioned_as_absent_or_negated
            .iter()
            .find(|p| p.name == "Income")
            .expect("income listed for review");
        assert_eq!(income.sentences, 1);
        assert_eq!(income.negated_terms, vec!["income".to_string()]);
        let med = d
            .mentioned_as_absent_or_negated
            .iter()
            .find(|p| p.name == "Medication regimen")
            .expect("medication regimen listed for review");
        assert_eq!(med.code, "42");
        assert!(med.negated_terms.contains(&"insulin".to_string()));
        assert!(env.summary.contains("Mentioned as absent or negated"));
        assert!(env.caveats.iter().any(|c| c.contains("absent or negated")));
        // The note's own words are still not returned.
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("is not taking her"));
    }

    #[test]
    fn lookup_ratings_and_care_plan() {
        let (dir, c) = ctx(true);
        let env = omaha_system_coder(
            &c,
            OmahaArgs {
                lookup: Some("medication regimen".into()),
                ratings: vec![KbsRating {
                    problem: "42".into(),
                    knowledge: Some(3),
                    behavior: Some(2),
                    status: Some(2),
                    baseline_knowledge: Some(2),
                    ..KbsRating::default()
                }],
                write_care_plan: true,
                ..OmahaArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.lookup[0].code, "42");
        assert_eq!(d.ratings[0].ratings["knowledge"], "3 (Basic knowledge)");
        assert_eq!(d.ratings[0].change["knowledge"], 1);
        let f = d.care_plan.as_ref().unwrap();
        assert!(dir.path().join("data").join(f).exists());
        let e = omaha_system_coder(
            &c,
            OmahaArgs {
                ratings: vec![KbsRating {
                    problem: "Pain".into(),
                    knowledge: Some(7),
                    ..KbsRating::default()
                }],
                ..OmahaArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("1 to 5"));
    }

    #[test]
    fn content_has_42_problems_and_76_targets() {
        let c = content();
        assert_eq!(c.problems.len(), 42);
        assert_eq!(c.targets.len(), 76);
        assert_eq!(c.problems[41].name, "Medication regimen");
        assert_eq!(
            find_problem("communicable infectious condition")
                .unwrap()
                .code,
            "34"
        );
    }
}
