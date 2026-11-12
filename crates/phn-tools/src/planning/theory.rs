//! PLAN-04 `apply_behavior_change_theory`: maps a program's problem, population, observed
//! barriers, and planned components onto the constructs of health behavior and behavior change
//! theories, with intervention strategies and measure ideas for each construct.
//!
//! * The library of theories is `content/planning/behavior_theories.yaml` (eight theories and
//!   frameworks; constructs paraphrased from open-access reviews and a CDC page that were read,
//!   each strategy marked as coming from a source or as this project's own suggestion).
//! * Matching is by whole words and phrases ("signals" in the content). A construct is flagged
//!   only when the nurse's own words match its signals; it is a lead, not a diagnosis.
//!
//! The inputs describe a population and a program, never a person. Nothing is read from local
//! files and nothing is sent anywhere.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::textmatch::{contains_phrase_stemmed, stems};
use crate::context::Context;
use crate::epr::content::{self, Confidence, DocSource};
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/planning/behavior_theories.yaml`.
pub const BEHAVIOR_THEORIES: &str = "planning-behavior-theories";

/// Most theories shown when the tool chooses them.
const AUTO_THEORIES: usize = 3;
const MAX_TEXT: usize = 600;
const MAX_ITEM: usize = 300;
const MAX_ITEMS: usize = 15;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Choice {
    text: String,
    source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct StrategyDef {
    text: String,
    basis: String,
    source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MeasureDef {
    text: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ConstructDef {
    id: String,
    name: String,
    definition: String,
    confidence: Confidence,
    signals: Vec<String>,
    strategies: Vec<StrategyDef>,
    measures: Vec<MeasureDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct TheoryDef {
    id: String,
    name: String,
    abbreviation: String,
    level: String,
    kind: String,
    summary: String,
    best_for: Vec<String>,
    limits: Vec<String>,
    sources: Vec<String>,
    constructs: Vec<ConstructDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct TheoryContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, DocSource>,
    how_to_choose: Vec<Choice>,
    theories: Vec<TheoryDef>,
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `apply_behavior_change_theory`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TheoryArgs {
    /// The health problem or behavior, in a sentence or two, for example "Low breast cancer
    /// screening among women over 50 in our town, mostly because of cost and not knowing where to
    /// go". Describe a population, never a person.
    pub problem: String,
    /// Who the program serves (age, language, community, setting), for example "Portuguese- and
    /// Spanish-speaking women aged 50 to 74". Optional.
    pub population: Option<String>,
    /// Where the program runs, for example "community health center" or "elementary school".
    pub setting: Option<String>,
    /// Barriers and facilitators you found (from an assessment, focus group, or experience), one
    /// per entry, in your own words. Each is matched to constructs. Up to 15.
    pub barriers: Option<Vec<String>>,
    /// Activities you plan or run, one per entry, for example "text message reminders" or
    /// "peer navigators". Each is matched to strategies that fit it. Up to 15.
    pub program_components: Option<Vec<String>>,
    /// Theories to use, by id, abbreviation, or name (hbm, sct, tpb, ttm, sem, doi, comb, sdt), or
    /// "all". Default: the tool picks up to three whose constructs best match your words, and
    /// adds the Socio-Ecological Model when none of them looks beyond the individual.
    pub theories: Option<Vec<String>>,
    /// Write the mapping to a Word or PDF worksheet in /data/output (default false). The file
    /// repeats only what you typed here and the theory content.
    pub write_document: Option<bool>,
    /// docx (default) or pdf, when `write_document` is true.
    pub format: Option<DocFormat>,
}

/// A strategy for a construct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StrategyOut {
    pub text: String,
    /// "source" (taken from a published table in the cited paper) or "authored" (this
    /// project's own suggestion, not linked to the construct by a source).
    pub basis: String,
    /// Title of the source, when basis is "source".
    pub source: Option<String>,
}

/// A measure idea for a construct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasureOut {
    pub text: String,
    /// knowledge, belief, behavior, process, or environment.
    #[serde(rename = "type")]
    pub kind: String,
}

/// One construct with what the tool found in the nurse's words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConstructOut {
    pub id: String,
    pub name: String,
    pub definition: String,
    /// Confidence that the definition matches the sources read (high, medium, or low).
    pub confidence: Confidence,
    /// Words or phrases in what you typed that point to this construct (empty when none).
    pub matched_signals: Vec<String>,
    pub strategies: Vec<StrategyOut>,
    pub measures: Vec<MeasureOut>,
}

/// One theory with its constructs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TheoryOut {
    pub id: String,
    pub name: String,
    pub abbreviation: String,
    /// individual, interpersonal, multilevel, organization and community, and so on.
    pub level: String,
    /// explanatory theory, change theory, or framework.
    pub kind: String,
    pub summary: String,
    /// Why this theory is shown.
    pub why_shown: String,
    pub best_for: Vec<String>,
    pub limits: Vec<String>,
    /// Constructs whose signals matched what you typed.
    pub matched_construct_count: usize,
    /// Constructs, the matched ones first.
    pub constructs: Vec<ConstructOut>,
    pub sources: Vec<String>,
}

/// One construct a barrier points to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConstructRef {
    pub theory: String,
    pub construct: String,
    pub signals: Vec<String>,
}

/// A barrier mapped to constructs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BarrierRow {
    pub barrier: String,
    pub constructs: Vec<ConstructRef>,
    /// True when no construct's signals matched: judge it yourself.
    pub no_match: bool,
}

/// One possible fit between a planned component and a construct's strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentFit {
    pub theory: String,
    pub construct: String,
    pub strategy: String,
    /// Words shared by the component and the strategy.
    pub shared_words: Vec<String>,
}

/// A planned component mapped to strategies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentRow {
    pub component: String,
    pub possible_fits: Vec<ComponentFit>,
    /// True when no strategy shares a content word with the component.
    pub no_match: bool,
}

/// A theory in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TheoryListing {
    pub id: String,
    pub name: String,
    pub abbreviation: String,
    pub kind: String,
    pub level: String,
    pub constructs: usize,
}

/// Result of `apply_behavior_change_theory`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TheoryResult {
    pub problem: String,
    pub population: Option<String>,
    pub setting: Option<String>,
    /// How the shown theories were chosen.
    pub selection: String,
    pub theories: Vec<TheoryOut>,
    pub barrier_map: Vec<BarrierRow>,
    pub component_map: Vec<ComponentRow>,
    /// Notes on choosing among theories, each with its source.
    pub how_to_choose: Vec<String>,
    /// Every theory in the library, for choosing others with `theories`.
    pub library: Vec<TheoryListing>,
    /// Worksheet in the output folder, when requested.
    pub worksheet_file: Option<String>,
}

// ---------------------------------------------------------------- logic

fn clean_text(label: &str, s: &str, max: usize) -> Result<String> {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > max {
        return Err(PhnError::InvalidInput(format!(
            "{label} is too long (at most {max} characters)"
        )));
    }
    Ok(t)
}

/// Refuse text that looks like it identifies a person.
fn refuse_identifiers(label: &str, text: &str) -> Result<()> {
    use phn_local::phi::{Identifier as I, scan_text};
    let kinds: Vec<&str> = scan_text(text)
        .into_iter()
        .filter(|f| {
            matches!(
                f.kind,
                I::Ssn
                    | I::Phone
                    | I::Email
                    | I::StreetAddress
                    | I::MedicalRecordNumber
                    | I::HealthPlanNumber
                    | I::BirthDate
            )
        })
        .map(|f| f.kind.label())
        .collect();
    if kinds.is_empty() {
        Ok(())
    } else {
        Err(PhnError::Privacy(format!(
            "{label} appears to contain {}. Describe the population and the program, not an \
             individual client.",
            kinds.join(", ")
        )))
    }
}

fn list_arg(label: &str, v: &Option<Vec<String>>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for s in v.iter().flatten() {
        let t = clean_text(label, s, MAX_ITEM)?;
        if t.is_empty() {
            continue;
        }
        refuse_identifiers(label, &t)?;
        out.push(t);
    }
    if out.len() > MAX_ITEMS {
        return Err(PhnError::InvalidInput(format!(
            "{label} has {} entries; give at most {MAX_ITEMS}",
            out.len()
        )));
    }
    Ok(out)
}

/// Signals of a construct found in the text.
fn matched_signals(c: &ConstructDef, text: &str) -> Vec<String> {
    c.signals
        .iter()
        .filter(|s| contains_phrase_stemmed(text, s))
        .cloned()
        .collect()
}

fn resolve<'a>(c: &'a TheoryContent, wanted: &[String]) -> Result<Vec<&'a TheoryDef>> {
    if wanted.iter().any(|w| w.trim().eq_ignore_ascii_case("all")) {
        return Ok(c.theories.iter().collect());
    }
    let mut out: Vec<&TheoryDef> = Vec::new();
    for w in wanted {
        let k = w.trim().to_lowercase();
        let found = c.theories.iter().find(|t| {
            t.id == k
                || t.abbreviation.to_lowercase() == k
                || t.name.to_lowercase() == k
                || t.abbreviation.to_lowercase().replace('-', "") == k.replace('-', "")
        });
        match found {
            Some(t) => {
                if !out.iter().any(|x| x.id == t.id) {
                    out.push(t);
                }
            }
            None => {
                return Err(PhnError::InvalidInput(format!(
                    "'{w}' is not a theory in the library. Use one of: {} (or \"all\")",
                    c.theories
                        .iter()
                        .map(|t| format!("{} ({})", t.id, t.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
    }
    Ok(out)
}

fn out_construct(c: &ConstructDef, matched: Vec<String>, sources: &BTreeMap<String, DocSource>) -> ConstructOut {
    ConstructOut {
        id: c.id.clone(),
        name: c.name.clone(),
        definition: c.definition.clone(),
        confidence: c.confidence,
        matched_signals: matched,
        strategies: c
            .strategies
            .iter()
            .map(|s| StrategyOut {
                text: s.text.clone(),
                basis: s.basis.clone(),
                source: s
                    .source
                    .as_ref()
                    .map(|k| sources.get(k).map_or(k.clone(), |d| d.title.clone())),
            })
            .collect(),
        measures: c
            .measures
            .iter()
            .map(|m| MeasureOut {
                text: m.text.clone(),
                kind: m.kind.clone(),
            })
            .collect(),
    }
}

/// Apply behavior change theory to a program.
pub fn apply_behavior_change_theory(
    ctx: &Context,
    args: TheoryArgs,
) -> Result<Envelope<TheoryResult>> {
    let problem = clean_text("problem", &args.problem, MAX_TEXT)?;
    if problem.is_empty() {
        return Err(PhnError::InvalidInput(
            "problem is required: describe the health problem or behavior and what is going \
             wrong, in a sentence or two"
                .into(),
        ));
    }
    refuse_identifiers("problem", &problem)?;
    let population = match &args.population {
        Some(p) if !p.trim().is_empty() => {
            let t = clean_text("population", p, MAX_TEXT)?;
            refuse_identifiers("population", &t)?;
            Some(t)
        }
        _ => None,
    };
    let setting = match &args.setting {
        Some(p) if !p.trim().is_empty() => {
            let t = clean_text("setting", p, MAX_ITEM)?;
            refuse_identifiers("setting", &t)?;
            Some(t)
        }
        _ => None,
    };
    let barriers = list_arg("barriers", &args.barriers)?;
    let components = list_arg("program_components", &args.program_components)?;

    let c: TheoryContent = content::load(BEHAVIOR_THEORIES)?;

    // What the nurse said about the problem: problem, population, setting, and barriers.
    let mut said = vec![problem.clone()];
    said.extend(population.clone());
    said.extend(setting.clone());
    said.extend(barriers.iter().cloned());
    let said_all = said.join(". ");

    // Choose the theories.
    let hits = |t: &TheoryDef| -> usize {
        t.constructs
            .iter()
            .map(|k| matched_signals(k, &said_all).len())
            .sum()
    };
    let (chosen, selection, mut why): (Vec<&TheoryDef>, String, BTreeMap<String, String>) =
        if let Some(w) = args.theories.as_ref().filter(|w| !w.is_empty()) {
            let v = resolve(&c, w)?;
            let why = v
                .iter()
                .map(|t| (t.id.clone(), "you asked for it".to_string()))
                .collect();
            (v, "Theories you asked for.".into(), why)
        } else {
            let mut scored: Vec<(&TheoryDef, usize)> =
                c.theories.iter().map(|t| (t, hits(t))).collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            let top: Vec<(&TheoryDef, usize)> = scored
                .into_iter()
                .filter(|(_, h)| *h > 0)
                .take(AUTO_THEORIES)
                .collect();
            if top.len() < 2 {
                let defaults = ["hbm", "sct", "sem"];
                let v: Vec<&TheoryDef> = defaults
                    .iter()
                    .filter_map(|id| c.theories.iter().find(|t| t.id == *id))
                    .collect();
                let why = v
                    .iter()
                    .map(|t| {
                        (
                            t.id.clone(),
                            "shown by default: too few of your words matched any theory's \
                             signals, so these are common starting points, not a match"
                                .to_string(),
                        )
                    })
                    .collect();
                (
                    v,
                    "Too few of your words matched a construct's signals to pick theories, so a \
                     common starting set is shown (not a recommendation). Add barriers in your \
                     own words, or name theories with `theories`."
                        .into(),
                    why,
                )
            } else {
                let why = top
                    .iter()
                    .map(|(t, h)| {
                        (
                            t.id.clone(),
                            format!("{h} word(s) or phrase(s) in your description match its constructs"),
                        )
                    })
                    .collect();
                (
                    top.into_iter().map(|(t, _)| t).collect(),
                    format!(
                        "The {AUTO_THEORIES} theories whose constructs match the most words in \
                         your description."
                    ),
                    why,
                )
            }
        };
    let mut chosen = chosen;
    let mut selection = selection;
    if args.theories.as_ref().is_none_or(|w| w.is_empty())
        && !chosen.iter().any(|t| t.level.contains("multilevel"))
        && let Some(sem) = c.theories.iter().find(|t| t.id == "sem")
    {
        chosen.push(sem);
        why.insert(
            sem.id.clone(),
            "added because none of the other theories looks beyond the person; most programs \
             also need action in the setting, the community, or policy"
                .into(),
        );
        selection.push_str(" The Socio-Ecological Model was added as a multilevel companion.");
    }

    // Build the output per theory.
    let mut theories_out = Vec::new();
    for t in &chosen {
        let mut cons: Vec<(usize, ConstructOut)> = t
            .constructs
            .iter()
            .map(|k| {
                let m = matched_signals(k, &said_all);
                (m.len(), out_construct(k, m, &c.sources))
            })
            .collect();
        let matched_count = cons.iter().filter(|(n, _)| *n > 0).count();
        // Matched first (most matches first), the rest in the library's order.
        cons.sort_by(|a, b| b.0.cmp(&a.0));
        theories_out.push(TheoryOut {
            id: t.id.clone(),
            name: t.name.clone(),
            abbreviation: t.abbreviation.clone(),
            level: t.level.clone(),
            kind: t.kind.clone(),
            summary: t.summary.clone(),
            why_shown: why.get(&t.id).cloned().unwrap_or_default(),
            best_for: t.best_for.clone(),
            limits: t.limits.clone(),
            matched_construct_count: matched_count,
            constructs: cons.into_iter().map(|(_, k)| k).collect(),
            sources: t
                .sources
                .iter()
                .map(|k| c.sources.get(k).map_or(k.clone(), |d| d.title.clone()))
                .collect(),
        });
    }

    // Barrier map over the shown theories.
    let mut barrier_map = Vec::new();
    for b in &barriers {
        let mut refs = Vec::new();
        for t in &chosen {
            for k in &t.constructs {
                let m = matched_signals(k, b);
                if !m.is_empty() {
                    refs.push(ConstructRef {
                        theory: t.abbreviation.clone(),
                        construct: k.name.clone(),
                        signals: m,
                    });
                }
            }
        }
        refs.sort_by(|a, b| b.signals.len().cmp(&a.signals.len()));
        refs.truncate(6);
        barrier_map.push(BarrierRow {
            barrier: b.clone(),
            no_match: refs.is_empty(),
            constructs: refs,
        });
    }

    // Component map: shared content words with a strategy's text.
    let mut component_map = Vec::new();
    for comp in &components {
        let cw = stems(comp);
        let mut fits: Vec<ComponentFit> = Vec::new();
        for t in &chosen {
            for k in &t.constructs {
                for s in &k.strategies {
                    let sw = stems(&s.text);
                    let shared: Vec<String> = cw.intersection(&sw).cloned().collect();
                    if !shared.is_empty() {
                        fits.push(ComponentFit {
                            theory: t.abbreviation.clone(),
                            construct: k.name.clone(),
                            strategy: s.text.clone(),
                            shared_words: shared,
                        });
                    }
                }
            }
        }
        fits.sort_by(|a, b| b.shared_words.len().cmp(&a.shared_words.len()));
        fits.truncate(5);
        component_map.push(ComponentRow {
            component: comp.clone(),
            no_match: fits.is_empty(),
            possible_fits: fits,
        });
    }

    let library: Vec<TheoryListing> = c
        .theories
        .iter()
        .map(|t| TheoryListing {
            id: t.id.clone(),
            name: t.name.clone(),
            abbreviation: t.abbreviation.clone(),
            kind: t.kind.clone(),
            level: t.level.clone(),
            constructs: t.constructs.len(),
        })
        .collect();
    let how_to_choose: Vec<String> = c
        .how_to_choose
        .iter()
        .map(|h| match &h.source {
            Some(k) => format!(
                "{} ({})",
                h.text,
                c.sources.get(k).map_or(k.as_str(), |d| d.title.as_str())
            ),
            None => format!("{} (this project's guidance, not from a source)", h.text),
        })
        .collect();

    let mut result = TheoryResult {
        problem: problem.clone(),
        population: population.clone(),
        setting: setting.clone(),
        selection,
        theories: theories_out,
        barrier_map,
        component_map,
        how_to_choose,
        library,
        worksheet_file: None,
    };

    // Optional worksheet.
    let mut files_written = Vec::new();
    if args.write_document.unwrap_or(false) {
        let md = worksheet_markdown(&result);
        let format = args.format.unwrap_or_default();
        let (dir, _) = templates::output_folder(ctx, "behavior_theory_worksheet")?;
        let doc = templates::document(ctx, &md, "en-US", "Behavior change theory worksheet");
        let f = templates::write_in(
            ctx,
            &dir,
            &format!("behavior_theory_worksheet.{}", format.ext()),
            &format.render(&doc)?,
        )?;
        files_written.push(f.clone());
        result.worksheet_file = Some(f);
    }

    let summary = summary(&result);

    // Sources: the sources of the theories shown, plus the cited strategy sources.
    let mut keys: Vec<&str> = Vec::new();
    for t in &chosen {
        keys.extend(t.sources.iter().map(String::as_str));
    }
    let sources: Vec<Source> = content::sources_for(keys, &c.sources);

    let mut caveats = vec![
        "A theory describes how change may work; it is not evidence that an intervention \
         works. For what has been shown to work for a health topic, use \
         find_evidence_based_interventions."
            .to_string(),
        "Constructs were paraphrased from open-access reviews and a CDC page that were read \
         (listed in sources). The original theory papers were not available and were not read; \
         check a definition there before quoting it in a grant or paper."
            .into(),
        "A strategy marked 'authored' is this project's suggestion in plain language; no \
         source linked it to the construct. Strategies marked 'source' come from a table in the \
         cited paper. Judge each for your population."
            .into(),
        "Measure ideas say what to count or ask; they are not validated instruments. Use \
         validated items from the literature where they exist, and check each instrument's \
         license before copying it."
            .into(),
        "Constructs are flagged by matching your words to a list of signal words, so a flag is a \
         lead, not a diagnosis, and a barrier with no match may still fit a construct. Your \
         assessment data should decide which constructs to target."
            .into(),
    ];
    if args.theories.is_none_or(|w| w.is_empty()) && result.selection.starts_with("Too few") {
        caveats.push(
            "No theory was chosen from your words; the default set is only a starting point."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[BEHAVIOR_THEORIES]));

    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            row_level_data: false,
            files_written,
            notes: vec![
                "Only the text you typed (a population and program description) was used. No \
                 local files were read and nothing was sent to any outside service."
                    .into(),
            ],
            ..Privacy::none()
        }))
}

fn strategy_line(s: &StrategyOut) -> String {
    match &s.source {
        Some(src) => format!("{} [from: {}]", s.text, src),
        None => format!("{} [tool's suggestion]", s.text),
    }
}

fn worksheet_markdown(r: &TheoryResult) -> String {
    let mut m = String::from("# Behavior change theory worksheet\n\n");
    m.push_str(&format!("**Problem:** {}\n\n", r.problem));
    if let Some(p) = &r.population {
        m.push_str(&format!("**Population:** {p}\n\n"));
    }
    if let Some(s) = &r.setting {
        m.push_str(&format!("**Setting:** {s}\n\n"));
    }
    m.push_str(&format!("{}\n\n", r.selection));
    for t in &r.theories {
        m.push_str(&format!("## {} ({})\n\n{}\n\n", t.name, t.kind, t.summary));
        let rows: Vec<Vec<String>> = t
            .constructs
            .iter()
            .map(|k| {
                vec![
                    if k.matched_signals.is_empty() {
                        k.name.clone()
                    } else {
                        format!("{} (matches your description)", k.name)
                    },
                    k.definition.clone(),
                    k.strategies
                        .iter()
                        .map(strategy_line)
                        .collect::<Vec<_>>()
                        .join(" / "),
                    k.measures
                        .iter()
                        .map(|x| x.text.clone())
                        .collect::<Vec<_>>()
                        .join(" / "),
                ]
            })
            .collect();
        m.push_str(&md::table(
            &["Construct", "What it means", "Strategies", "Measure ideas"],
            &rows,
        ));
        m.push('\n');
    }
    m.push_str(
        "Theory describes how change may work; it is not evidence that an intervention works. \
         Strategies marked as the tool's suggestion are drafts to judge for your population, and \
         the measure ideas are not validated instruments.\n",
    );
    m
}

fn summary(r: &TheoryResult) -> String {
    let mut s = format!("**Behavior change theory for:** {}\n\n", r.problem);
    s.push_str(&format!("{}\n\n", r.selection));
    for t in &r.theories {
        s.push_str(&format!(
            "### {} ({}; {})\n\n{}\n\n*Why shown:* {}.\n\n",
            t.name, t.kind, t.level, t.summary, t.why_shown
        ));
        let shown: Vec<&ConstructOut> = if t.matched_construct_count > 0 {
            t.constructs
                .iter()
                .filter(|k| !k.matched_signals.is_empty())
                .collect()
        } else {
            t.constructs.iter().collect()
        };
        let rows: Vec<Vec<String>> = shown
            .iter()
            .map(|k| {
                vec![
                    k.name.clone(),
                    k.definition.clone(),
                    k.strategies
                        .iter()
                        .map(strategy_line)
                        .collect::<Vec<_>>()
                        .join("; "),
                    k.measures
                        .iter()
                        .map(|x| x.text.clone())
                        .collect::<Vec<_>>()
                        .join("; "),
                ]
            })
            .collect();
        if t.matched_construct_count > 0 {
            s.push_str(&format!(
                "Constructs that match your words ({} of {}):\n\n",
                t.matched_construct_count,
                t.constructs.len()
            ));
        } else {
            s.push_str("No construct matched your words; all constructs are listed:\n\n");
        }
        s.push_str(&md::table(
            &["Construct", "What it means", "Strategies", "Measure ideas"],
            &rows,
        ));
        if t.matched_construct_count > 0 && t.matched_construct_count < t.constructs.len() {
            let others: Vec<&str> = t
                .constructs
                .iter()
                .filter(|k| k.matched_signals.is_empty())
                .map(|k| k.name.as_str())
                .collect();
            s.push_str(&format!(
                "\nOther constructs (details in the data): {}.\n",
                others.join(", ")
            ));
        }
        s.push('\n');
    }
    if !r.barrier_map.is_empty() {
        s.push_str("**Your barriers and the constructs they point to:**\n\n");
        let rows: Vec<Vec<String>> = r
            .barrier_map
            .iter()
            .map(|b| {
                vec![
                    b.barrier.clone(),
                    if b.no_match {
                        "no construct matched; judge it yourself".to_string()
                    } else {
                        b.constructs
                            .iter()
                            .map(|c| format!("{}: {}", c.theory, c.construct))
                            .collect::<Vec<_>>()
                            .join("; ")
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(&["Barrier", "Constructs"], &rows));
        s.push('\n');
    }
    if !r.component_map.is_empty() {
        s.push_str("**Your planned components and strategies they may fit (shared words only):**\n\n");
        let rows: Vec<Vec<String>> = r
            .component_map
            .iter()
            .map(|c| {
                vec![
                    c.component.clone(),
                    if c.no_match {
                        "no strategy shares a word with it".to_string()
                    } else {
                        c.possible_fits
                            .iter()
                            .map(|f| format!("{}: {}", f.theory, f.construct))
                            .collect::<Vec<_>>()
                            .join("; ")
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(&["Component", "Possible constructs"], &rows));
        s.push('\n');
    }
    if let Some(f) = &r.worksheet_file {
        s.push_str(&format!("Worksheet written to `{f}`.\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> TheoryContent {
        content::load(BEHAVIOR_THEORIES).unwrap()
    }

    #[test]
    fn library_is_complete_and_consistent() {
        let c = content();
        assert_eq!(c.theories.len(), 8);
        let mut ids = std::collections::BTreeSet::new();
        for t in &c.theories {
            assert!(ids.insert(t.id.clone()), "duplicate theory {}", t.id);
            assert!(!t.sources.is_empty());
            for s in &t.sources {
                assert!(c.sources.contains_key(s), "{}: unknown source {s}", t.id);
            }
            let mut kids = std::collections::BTreeSet::new();
            for k in &t.constructs {
                assert!(kids.insert(k.id.clone()), "{}: duplicate construct {}", t.id, k.id);
                assert!(k.signals.len() >= 3, "{} {}", t.id, k.id);
                for st in &k.strategies {
                    match st.basis.as_str() {
                        "source" => {
                            let key = st.source.as_ref().expect("source strategy has a source");
                            assert!(
                                t.sources.contains(key),
                                "{} {}: strategy source {key} is not a source of the theory",
                                t.id,
                                k.id
                            );
                        }
                        "authored" => assert!(st.source.is_none()),
                        b => panic!("unknown basis {b}"),
                    }
                }
            }
        }
        for h in &c.how_to_choose {
            if let Some(k) = &h.source {
                assert!(c.sources.contains_key(k));
            }
        }
    }

    #[test]
    fn signals_do_not_match_by_substring() {
        let c = content();
        let hbm = c.theories.iter().find(|t| t.id == "hbm").unwrap();
        let barriers = hbm
            .constructs
            .iter()
            .find(|k| k.id == "perceived_barriers")
            .unwrap();
        assert!(!matched_signals(barriers, "the costume party").contains(&"cost".to_string()));
        assert!(matched_signals(barriers, "Cost is a problem and there is no ride").len() >= 2);
        // Plural forms match.
        assert!(!matched_signals(barriers, "many hours are inconvenient").is_empty());
    }

    #[test]
    fn resolve_accepts_ids_abbreviations_and_names() {
        let c = content();
        let v = resolve(&c, &["HBM".into(), "social cognitive theory".into(), "com-b".into()]).unwrap();
        assert_eq!(v.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["hbm", "sct", "comb"]);
        assert_eq!(resolve(&c, &["all".into()]).unwrap().len(), 8);
        let e = resolve(&c, &["magic".into()]).unwrap_err().to_string();
        assert!(e.contains("hbm"), "{e}");
    }

    #[test]
    fn identifiers_in_the_description_are_refused() {
        assert!(refuse_identifiers("problem", "Women over 50 skip mammograms").is_ok());
        let e = refuse_identifiers("problem", "Call Jane at 617-555-0142 or jane@example.org")
            .unwrap_err()
            .to_string();
        assert!(e.contains("individual"), "{e}");
    }

    #[test]
    fn strategy_sources_come_from_the_paper() {
        let c = content();
        let sct = c.theories.iter().find(|t| t.id == "sct").unwrap();
        let k = sct.constructs.iter().find(|k| k.id == "self_efficacy").unwrap();
        assert!(k.strategies.iter().all(|s| s.basis == "source"));
    }
}
