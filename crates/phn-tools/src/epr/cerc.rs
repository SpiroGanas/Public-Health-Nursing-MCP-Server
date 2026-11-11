//! EPR-10 `crisis_risk_communication_builder`: CDC's Crisis and Emergency Risk
//! Communication (CERC) model as a working tool.
//!
//! For a hazard in the pre-scripted library (or any hazard the nurse describes)
//! and a CERC phase, the tool returns
//!
//! * a **message map** (three key messages, three supporting facts each) checked
//!   against the message-mapping rules (27 words / 9 seconds / 3 messages,
//!   empathy first, grade level, short sentences, no absolutes, positive action
//!   steps);
//! * a **holding statement** built from what is known, not known, and being done;
//! * **FAQs**, **spokesperson preparation** (tips, bridges, the questions reporters
//!   ask, media holding lines), audience concerns, and the phase's CERC tasks;
//! * a **language plan** (ADR 0009): the town's priority languages (EQ-03), the
//!   agency's approved translations in `/data/templates/<language>/cerc_<id>.md`
//!   when present, and otherwise a translator brief per language. Nothing is
//!   machine-translated (ADR 0008).
//!
//! A Word packet, the English template, and the translations or briefs are
//! written to `/data/output` on request. Staff contact details for the media
//! line come only from a roster file and go only into the packet (ADR 0009).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock, from_markdown};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self as epr_content, Cited, CitedOut, Confidence, DocSource, HazardActions};
use super::roster::{self, Roster, RosterSummary, RosterTally};
use crate::comms::press::loaded_words;
use crate::comms::readability::{AnalyzeOptions, analyze, default_target_grade};
use crate::comms::safe_messaging::{self, SafeMessagingReport};
use crate::comms::social::CAMPAIGN;
use crate::comms::translation::{self, TranslatorBrief};
use crate::context::Context;
use crate::school::templates::{self, DocFormat, TemplateResolver};

/// Content id of `content/epr/cerc.yaml`.
pub const CERC: &str = "epr-cerc";

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Fact {
    text: String,
    source: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyMessageDef {
    text: String,
    supporting: Vec<Fact>,
}

#[derive(Debug, Clone, Deserialize)]
struct FaqDef {
    question: String,
    answer: String,
    source: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PhaseNotes {
    #[serde(default)]
    preparation: Option<String>,
    #[serde(default)]
    initial: Option<String>,
    #[serde(default)]
    maintenance: Option<String>,
    #[serde(default)]
    resolution: Option<String>,
}

impl PhaseNotes {
    fn get(&self, phase: &str) -> Option<&String> {
        match phase {
            "preparation" => self.preparation.as_ref(),
            "initial" => self.initial.as_ref(),
            "maintenance" => self.maintenance.as_ref(),
            "resolution" => self.resolution.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LibraryEntry {
    id: String,
    label: String,
    aliases: Vec<String>,
    hazard_refs: Vec<String>,
    placeholders: Vec<String>,
    question: String,
    empathy: String,
    key_messages: Vec<KeyMessageDef>,
    actions: Vec<String>,
    faqs: Vec<FaqDef>,
    audiences: Vec<String>,
    phase_notes: PhaseNotes,
    related_tools: Vec<String>,
    sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PhaseDef {
    id: String,
    label: String,
    aliases: Vec<String>,
    tasks: Vec<Cited>,
}

#[derive(Debug, Clone, Deserialize)]
struct Rule {
    id: String,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MapLimits {
    key_messages: usize,
    supporting_per_key: usize,
    max_key_words_total: usize,
    max_seconds: f64,
    words_per_second: f64,
    max_average_sentence_words: f64,
    spokesperson_key_point_words: usize,
    spokesperson_key_point_seconds: usize,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
struct InitialDef {
    elements: Vec<Cited>,
    media_holding_lines: Vec<Cited>,
    first_48_hours: Vec<Cited>,
}

#[derive(Debug, Clone, Deserialize)]
struct SpokesDef {
    tips: Vec<Cited>,
    bridges: Vec<String>,
    bridges_source: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MediaQuestions {
    source: String,
    section: String,
    questions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AudienceDef {
    id: String,
    label: String,
    concerns: Vec<String>,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CercContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, DocSource>,
    phases: Vec<PhaseDef>,
    throughout: Vec<Cited>,
    pitfalls: Vec<Cited>,
    psychology: Vec<Cited>,
    message_map: MapLimits,
    absolutes: Vec<String>,
    negatives: Vec<String>,
    initial_statement: InitialDef,
    spokesperson: SpokesDef,
    media_questions: MediaQuestions,
    audiences: Vec<AudienceDef>,
    audience_reminders: Vec<Cited>,
    library: Vec<LibraryEntry>,
}

static CONTENT: LazyLock<CercContent> =
    LazyLock::new(|| epr_content::load(CERC).expect("content/epr/cerc.yaml matches its schema"));

// ---------------------------------------------------------------- arguments

/// A key message with its supporting facts (your own map to check).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct KeyMessageInput {
    /// The key message (aim for about 9 words).
    pub text: String,
    /// Up to three supporting facts.
    pub supporting: Vec<String>,
}

/// Arguments for `crisis_risk_communication_builder`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CercArgs {
    /// Pre-scripted message set: "measles_exposure", "boil_water", "extreme_heat",
    /// "coastal_storm_power_outage", "shelter_in_place", "respiratory_pandemic", or
    /// "mass_dispensing_pod" (words like "measles", "hurricane", or "POD" also work). For any
    /// other hazard use `hazard_description`. Omit both to list the library.
    pub hazard: Option<String>,
    /// A hazard that is not in the library, in a few words (e.g. "water main break").
    pub hazard_description: Option<String>,
    /// CERC phase: "preparation", "initial" (default), "maintenance", "resolution", or "all".
    pub phase: Option<String>,
    /// City, town, or county for language priorities (default: your saved town).
    pub area: Option<String>,
    /// Your organization (default: the letterhead's agency name).
    pub organization: Option<String>,
    /// What happened, as far as it is confirmed, and when.
    pub what_happened: Option<String>,
    /// Facts that are confirmed.
    pub known: Vec<String>,
    /// What is not known yet.
    pub unknown: Vec<String>,
    /// What your agency and partners are doing.
    pub being_done: Vec<String>,
    /// What the public should do (replaces the message set's action steps).
    pub public_actions: Vec<String>,
    /// Values for the message set's {{placeholders}}, e.g. {"location": "Main Street Library",
    /// "exposure_times": "November 3, 1 to 4 p.m."}. Places, times, and events only; never
    /// client names or details.
    pub details: BTreeMap<String, String>,
    /// Where the public gets updates (website, public hotline, or 2-1-1).
    pub info_source: Option<String>,
    /// When the next update will come (e.g. "by 4 p.m. today").
    pub next_update: Option<String>,
    /// Audiences to prepare for (default: the message set's audiences).
    pub audiences: Vec<String>,
    /// Spokesperson's name and title (contact numbers come from roster_path, never from chat).
    pub spokesperson: Option<String>,
    /// Your own message map to check: up to 3 key messages with up to 3 supporting facts each.
    pub message_map: Vec<KeyMessageInput>,
    /// The opening statement of empathy for your own map.
    pub empathy: Option<String>,
    /// A draft message to check against the CERC and message-mapping rules.
    pub draft_text: Option<String>,
    /// Reading grade to aim for (default 8).
    pub target_grade: Option<f64>,
    /// Languages to plan for instead of the town's ACS priorities (e.g. ["Spanish", "Portuguese"]).
    pub languages: Vec<String>,
    /// "docx" writes a Word packet, the English template, and agency translations or
    /// translator briefs for each language to /data/output.
    pub export: Option<String>,
    /// A staff roster CSV/XLSX in /data; the media contact's phone and email are written into
    /// the Word packet only and never returned.
    pub roster_path: Option<String>,
    /// Roster position of the media contact (default "Public Information Officer").
    pub media_contact_position: Option<String>,
}

// ---------------------------------------------------------------- results

/// A pre-scripted message set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LibraryItem {
    pub id: String,
    pub label: String,
    /// Details to supply (`details`), written as {{name}} in the messages.
    pub placeholders: Vec<String>,
    pub related_tools: Vec<String>,
}

/// CERC tasks for one phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhaseTasks {
    pub phase: String,
    pub label: String,
    pub tasks: Vec<CitedOut>,
    /// What to say in this phase for the chosen hazard.
    pub hazard_note: Option<String>,
}

/// A supporting fact with its citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SupportingFact {
    pub text: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// One key message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MapMessage {
    pub text: String,
    pub words: usize,
    pub supporting: Vec<SupportingFact>,
}

/// A message map (Covello's template as used by CDC/ATSDR and EPA).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MessageMap {
    /// The question or concern the map answers.
    pub question: String,
    /// Opening statement of empathy (not counted in the 27 words).
    pub empathy: String,
    pub key_messages: Vec<MapMessage>,
    pub total_key_words: usize,
    /// Speaking time at 3 words per second.
    pub estimated_seconds: f64,
    /// "library", "yours", or "built_from_inputs".
    pub origin: String,
}

/// One check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CercCheck {
    pub id: String,
    pub passed: bool,
    pub detail: String,
    /// CERC chapter or message-mapping source, or "tool rule".
    pub source: String,
}

/// A frequently asked question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Faq {
    pub question: String,
    pub answer: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// Spokesperson preparation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpokespersonPrep {
    pub spokesperson: Option<String>,
    /// CERC's limit for the key point of an answer.
    pub key_point_limit: String,
    pub tips: Vec<CitedOut>,
    /// Phrases to bridge back to the key message.
    pub bridges: Vec<String>,
    pub bridges_source: String,
    /// Questions to prepare answers for (CERC and EPA's list of reporters' questions).
    pub questions_to_prepare: Vec<String>,
    pub questions_source: String,
    /// What to say to reporters before a statement is ready.
    pub media_holding_lines: Vec<CitedOut>,
}

/// An audience and its concerns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudienceNote {
    pub audience: String,
    /// Typical concerns (CERC Table 1), when the audience matches one of its groups.
    pub concerns: Vec<String>,
    pub citation: Option<String>,
}

/// One language in the plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguagePlanRow {
    pub language: String,
    /// Residents 5+ who speak English less than "very well" (ACS), when from the town's data.
    pub lep_estimate: Option<f64>,
    pub geography: Option<String>,
    /// "agency_translation" (your approved translation was found) or "translator_brief".
    pub plan: String,
    pub script_note: Option<String>,
    /// File written to /data/output (when exported).
    pub file: Option<String>,
}

/// How each language will be reached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguagePlan {
    pub area: Option<String>,
    /// Template name agencies use for approved translations: /data/templates/<language>/<kind>.md.
    pub template_kind: Option<String>,
    pub rows: Vec<LanguagePlanRow>,
    pub how_to_add_translations: String,
}

/// Result of `crisis_risk_communication_builder`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CercResult {
    /// Library id, or null for a described hazard or a library listing.
    pub hazard_id: Option<String>,
    pub hazard_label: String,
    /// Phase(s) covered.
    pub phase: String,
    /// The pre-scripted library (always listed).
    pub library: Vec<LibraryItem>,
    pub phase_tasks: Vec<PhaseTasks>,
    /// Tasks for every phase (community engagement, decision-making, evaluation, PHEP Capability 4).
    pub throughout: Vec<CitedOut>,
    pub message_map: Option<MessageMap>,
    /// Initial (holding) statement in Markdown, with [BRACKETED] parts to complete.
    pub holding_statement: Option<String>,
    /// The English public message: the source for translations.
    pub public_message: Option<String>,
    pub actions: Vec<String>,
    /// Other protective messages for the hazard (shared with get_active_hazard_alerts).
    pub additional_protective_messages: Vec<String>,
    pub faqs: Vec<Faq>,
    pub spokesperson: SpokespersonPrep,
    pub audiences: Vec<AudienceNote>,
    pub audience_reminders: Vec<CitedOut>,
    pub initial_message_elements: Vec<CitedOut>,
    pub first_48_hours: Vec<CitedOut>,
    pub pitfalls: Vec<String>,
    pub psychology: Vec<CitedOut>,
    pub message_rules: Vec<String>,
    pub map_checks: Vec<CercCheck>,
    pub statement_checks: Vec<CercCheck>,
    pub draft_checks: Vec<CercCheck>,
    /// Reading grade of the public message.
    pub reading_grade: Option<f64>,
    pub draft_reading_grade: Option<f64>,
    pub target_grade: f64,
    pub safe_messaging: Option<SafeMessagingReport>,
    /// Placeholders and statement parts still to complete.
    pub missing: Vec<String>,
    pub language_plan: LanguagePlan,
    pub roster: Option<RosterSummary>,
    pub related_tools: Vec<String>,
    pub files: Vec<String>,
}

// ---------------------------------------------------------------- helpers

const PHASE_ORDER: [&str; 4] = ["preparation", "initial", "maintenance", "resolution"];

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn nonempty(v: &[String]) -> Vec<String> {
    v.iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn source_title(key: &str) -> String {
    if key == "local" {
        return "your incident details".into();
    }
    CONTENT
        .sources
        .get(key)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| key.to_string())
}

fn cited(items: &[Cited]) -> Vec<CitedOut> {
    epr_content::cite_all(items, &CONTENT.sources)
}

/// Resolve a library entry from an id, alias, or free text.
fn find_entry(q: &str) -> Option<&'static LibraryEntry> {
    find_entry_exact(q).or_else(|| {
        // An alias as whole words inside the text ("measles at the library"), never as
        // part of a word: "hundreds" does not hold "eds", nor "flush" "flu".
        let n = format!(" {} ", phn_core::text::normalize(q));
        CONTENT.library.iter().find(|e| {
            e.aliases.iter().any(|a| {
                let a = phn_core::text::normalize(a);
                a.len() > 2 && n.contains(&format!(" {a} "))
            })
        })
    })
}

/// A library entry whose id or alias is exactly the text given.
fn find_entry_exact(q: &str) -> Option<&'static LibraryEntry> {
    let n = phn_core::text::normalize(q);
    if n.is_empty() {
        return None;
    }
    let lib = &CONTENT.library;
    lib.iter()
        .find(|e| phn_core::text::normalize(&e.id.replace('_', " ")) == n)
        .or_else(|| {
            lib.iter()
                .find(|e| e.aliases.iter().any(|a| phn_core::text::normalize(a) == n))
        })
}

/// True for a whole-text `[BRACKETED]` placeholder (a part still to be written).
fn is_bracket_pad(s: &str) -> bool {
    let t = s.trim();
    t.len() > 2 && t.starts_with('[') && t.ends_with(']')
}

/// Supporting facts of a key message that are written (not bracketed placeholders).
fn written_facts(k: &MapMessage) -> usize {
    k.supporting
        .iter()
        .filter(|s| !is_bracket_pad(&s.text))
        .count()
}

fn parse_phases(p: Option<&str>) -> Result<(String, Vec<&'static PhaseDef>)> {
    let c = &*CONTENT;
    let raw = p.map(|s| s.trim().to_lowercase()).unwrap_or_default();
    if raw.is_empty() {
        let d = c
            .phases
            .iter()
            .find(|p| p.id == "initial")
            .expect("initial");
        return Ok(("initial".into(), vec![d]));
    }
    if raw == "all" || raw == "every" {
        let mut v: Vec<&PhaseDef> = c.phases.iter().collect();
        v.sort_by_key(|p| PHASE_ORDER.iter().position(|x| *x == p.id));
        return Ok(("all".into(), v));
    }
    let key = raw.replace([' ', '-'], "_");
    c.phases
        .iter()
        .find(|p| p.id == key || p.aliases.iter().any(|a| a.replace([' ', '-'], "_") == key))
        .map(|p| (p.id.clone(), vec![p]))
        .ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "phase '{raw}' is not recognized; use preparation, initial, maintenance, resolution, or all"
            ))
        })
}

/// Terms found in text at word boundaries (case-insensitive; curly apostrophes allowed).
fn find_terms(text: &str, terms: &[String]) -> Vec<String> {
    let lower = text.to_lowercase().replace('’', "'");
    let mut found = Vec::new();
    for t in terms {
        let t = t.to_lowercase();
        let mut start = 0;
        while let Some(i) = lower[start..].find(&t) {
            let a = start + i;
            let b = a + t.len();
            let before = lower[..a].chars().next_back();
            let after = lower[b..].chars().next();
            let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
            if boundary(before) && boundary(after) {
                found.push(t.clone());
                break;
            }
            start = b;
        }
    }
    found
}

fn word_count(s: &str) -> usize {
    s.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

fn bracket(label: &str) -> String {
    format!("[{}]", label.to_uppercase())
}

/// ACS language name to a name the template folders use ("Chinese (incl.
/// Mandarin, Cantonese)" -> "Chinese", "Haitian" -> "Haitian Creole").
fn folder_language(acs: &str) -> String {
    let base = acs.split(" (").next().unwrap_or(acs);
    let first = base
        .split(',')
        .next()
        .unwrap_or(base)
        .split(" or ")
        .next()
        .unwrap_or(base)
        .trim();
    match first.to_lowercase().as_str() {
        "haitian" => "Haitian Creole".into(),
        _ => first.to_string(),
    }
}

fn file_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// A raw supporting fact: text, source key, confidence.
type RawFact = (String, String, Confidence);

/// Raw (unfilled) pieces of a message set.
struct Raw {
    label: String,
    question: String,
    empathy: String,
    key_messages: Vec<(String, Vec<RawFact>)>,
    actions: Vec<String>,
    origin: &'static str,
}

/// The English public message in Markdown, with {{placeholders}} as given.
fn compose_public_message(r: &Raw) -> String {
    let mut s = format!("# {}\n\nA message from {{{{organization}}}}.\n\n", r.label);
    if !r.empathy.trim().is_empty() {
        s.push_str(&format!("{}\n\n", r.empathy.trim()));
    }
    for (k, sup) in &r.key_messages {
        s.push_str(&format!("**{}**\n\n", k.trim()));
        for (t, _, _) in sup {
            s.push_str(&format!("- {}\n", t.trim()));
        }
        s.push('\n');
    }
    if !r.actions.is_empty() {
        s.push_str("## What you can do\n\n");
        for (i, a) in r.actions.iter().enumerate() {
            s.push_str(&format!("{}. {}\n", i + 1, a.trim()));
        }
        s.push('\n');
    }
    s.push_str("For updates: {{info_source}}.\n");
    s
}

static DOUBLE_STOP: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([[:alpha:]])\.\.(\s|$)").expect("valid regex"));

/// Fill {{placeholders}}; unknown ones stay visible as `[name]` and are listed
/// in `missing`. A value ending in a period ("p.m.") before a sentence's own
/// period does not leave a double period.
fn fill(text: &str, values: &BTreeMap<String, String>, missing: &mut Vec<String>) -> String {
    let (out, unknown) = templates::fill(text, values, "");
    for u in unknown {
        if !missing.contains(&u) {
            missing.push(u);
        }
    }
    DOUBLE_STOP.replace_all(&out, "$1.$2").into_owned()
}

fn check(id: &str, passed: bool, detail: String, source: &str) -> CercCheck {
    CercCheck {
        id: id.into(),
        passed,
        detail,
        source: source.into(),
    }
}

fn rule_source(id: &str) -> String {
    CONTENT
        .message_map
        .rules
        .iter()
        .find(|r| r.id == id)
        .map(|r| format!("{}, {}", source_title(&r.source), r.section))
        .unwrap_or_else(|| "tool rule".into())
}

/// Client-identifier categories in text (officials' names, places, dates, and
/// public numbers belong in crisis messages and are not flagged).
fn client_identifiers(text: &str, include_addresses: bool) -> Vec<String> {
    use phn_local::phi::{Identifier as I, scan_text};
    let mut v: Vec<String> = scan_text(text)
        .into_iter()
        .filter(|f| {
            matches!(
                f.kind,
                I::Ssn
                    | I::MedicalRecordNumber
                    | I::HealthPlanNumber
                    | I::BirthDate
                    | I::AccountNumber
                    | I::LicenseNumber
                    | I::Age
            ) || (include_addresses && f.kind == I::StreetAddress)
        })
        .map(|f| f.kind.label().to_string())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Checks for any finished text (draft or statement).
fn text_checks(text: &str, target: f64, id_prefix: &str) -> (Vec<CercCheck>, f64) {
    let c = &*CONTENT;
    let a = analyze(
        text,
        &AnalyzeOptions {
            target_grade: target,
            max_flags: 5,
            redact_excerpts: false,
        },
    );
    let lower = text.to_lowercase().replace('’', "'");
    let mut out = Vec::new();
    let g = a.grade.median_grade;
    out.push(check(
        &format!("{id_prefix}grade_level"),
        g <= target,
        format!("reading grade {g:.1} (target {target})"),
        &rule_source("grade_level"),
    ));
    out.push(check(
        &format!("{id_prefix}short_sentences"),
        a.flags.long_sentence_count == 0,
        format!(
            "{} sentence(s) over 20 words; aim for about 10 to 12 words on average",
            a.flags.long_sentence_count
        ),
        &rule_source("short_sentences"),
    ));
    let loaded = loaded_words(&lower);
    out.push(check(
        &format!("{id_prefix}no_loaded_words"),
        loaded.is_empty(),
        if loaded.is_empty() {
            "no emotionally loaded words found".into()
        } else {
            format!("remove loaded words: {}", loaded.join(", "))
        },
        "CERC: Working with the Media (remove adjectives and emotionally loaded words)",
    ));
    let abs = find_terms(text, &c.absolutes);
    out.push(check(
        &format!("{id_prefix}no_absolutes"),
        abs.is_empty(),
        if abs.is_empty() {
            "no absolutes found".into()
        } else {
            format!(
                "avoid absolutes: {} (state the level of risk instead)",
                abs.join(", ")
            )
        },
        &rule_source("no_absolutes"),
    ));
    let neg = find_terms(text, &c.negatives);
    out.push(check(
        &format!("{id_prefix}negative_dominance"),
        neg.len() <= 2,
        format!(
            "{} negative phrase(s) ({}); balance each negative with about three positive, solution-oriented statements",
            neg.len(),
            if neg.is_empty() {
                "none".into()
            } else {
                neg.join(", ")
            }
        ),
        &rule_source("negative_dominance"),
    ));
    let empathy = [
        "understand",
        "we know",
        "concern",
        "worr",
        "sorry",
        "difficult",
        "frighten",
        "stressful",
        "hard ",
        "thoughts",
    ]
    .iter()
    .any(|w| lower.contains(w));
    out.push(check(
        &format!("{id_prefix}empathy"),
        empathy,
        if empathy {
            "acknowledges what people are feeling".into()
        } else {
            "open by acknowledging people's concern or fear (express empathy)".into()
        },
        &rule_source("empathy_first"),
    ));
    let uncertainty = lower.contains("not know")
        || lower.contains("don't know")
        || lower.contains("not yet")
        || lower.contains("still learning")
        || lower.contains("finding out")
        || lower.contains("we will share")
        || lower.contains("update");
    out.push(check(
        &format!("{id_prefix}known_unknown_process"),
        uncertainty,
        if uncertainty {
            "says what is not known yet or when updates will come".into()
        } else {
            "say what you know, what you do not know yet, and how and when you will update people"
                .into()
        },
        &rule_source("known_unknown_process"),
    ));
    (out, g)
}

// ---------------------------------------------------------------- the tool

/// Build a CERC message set: message map, holding statement, FAQs, spokesperson
/// preparation, phase tasks, and a language plan.
pub async fn crisis_risk_communication_builder(
    ctx: &Context,
    args: CercArgs,
) -> Result<Envelope<CercResult>> {
    let c = &*CONTENT;
    let (phase_key, phases) = parse_phases(args.phase.as_deref())?;
    let target = args.target_grade.unwrap_or_else(default_target_grade);
    if !(1.0..=16.0).contains(&target) {
        return Err(PhnError::InvalidInput(
            "target_grade must be between 1 and 16".into(),
        ));
    }
    if args.message_map.len() > 3 {
        return Err(PhnError::InvalidInput(format!(
            "a message map has at most 3 key messages (you gave {}); people under stress take in about three",
            args.message_map.len()
        )));
    }
    let export = match args
        .export
        .as_deref()
        .map(|f| f.trim().to_lowercase())
        .filter(|f| !f.is_empty() && f != "none")
    {
        None => false,
        Some(f) if f == "docx" => true,
        Some(_) => return Err(PhnError::InvalidInput("export must be \"docx\"".into())),
    };

    // ---- which hazard
    let entry = match opt(&args.hazard) {
        Some(h) => match find_entry(h) {
            Some(e) => Some(e),
            None if opt(&args.hazard_description).is_some() => None,
            None => {
                return Err(PhnError::InvalidInput(format!(
                    "'{h}' is not in the pre-scripted library ({}); describe it in hazard_description to build a message set from your facts",
                    c.library
                        .iter()
                        .map(|e| e.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        },
        // A free-text description selects a pre-scripted set only when it is exactly a
        // set's name ("measles", "heat wave"). "No heat in senior housing" or "space
        // heater fire" must not bring up the extreme heat messages; a likely set is
        // suggested in a caveat instead.
        None => opt(&args.hazard_description).and_then(find_entry_exact),
    };
    let custom_label = opt(&args.hazard_description)
        .or(opt(&args.hazard))
        .map(str::to_string);
    let listing = entry.is_none()
        && custom_label.is_none()
        && args.message_map.is_empty()
        && opt(&args.draft_text).is_none();

    let library: Vec<LibraryItem> = c
        .library
        .iter()
        .map(|e| LibraryItem {
            id: e.id.clone(),
            label: e.label.clone(),
            placeholders: e.placeholders.clone(),
            related_tools: e.related_tools.clone(),
        })
        .collect();

    let mut sources: Vec<Source> = epr_content::sources_for(
        [
            "cerc_manual",
            "cerc_intro",
            "cerc_psychology",
            "cerc_messages",
            "cerc_plans",
            "cerc_spokesperson",
            "atsdr_message_map",
            "epa_message_mapping",
            "cdc_phep",
        ],
        &c.sources,
    );
    let mut caveats: Vec<String> = Vec::new();
    if entry.is_none()
        && let Some(e) = opt(&args.hazard_description).and_then(find_entry)
    {
        caveats.push(format!(
            "The description mentions a hazard with a pre-scripted message set ({}). It was not used, because the description may be about something else; pass hazard \"{}\" if that set fits.",
            e.label, e.id
        ));
    }
    let mut privacy = Privacy::none();
    let mut missing: Vec<String> = Vec::new();

    // ---- values for placeholders
    let organization = opt(&args.organization).map(str::to_string).or_else(|| {
        ctx.settings()
            .letterhead
            .map(|l| l.agency_name)
            .filter(|n| !n.trim().is_empty())
    });
    let mut values: BTreeMap<String, String> = args
        .details
        .iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(o) = &organization {
        values.insert("organization".into(), o.clone());
    }
    if let Some(i) = opt(&args.info_source) {
        values.insert("info_source".into(), i.to_string());
    }
    if let Some(n) = opt(&args.next_update) {
        values.insert("next_update".into(), n.to_string());
    }

    // ---- raw message set
    let known = nonempty(&args.known);
    let unknown = nonempty(&args.unknown);
    let doing = nonempty(&args.being_done);
    let user_actions = nonempty(&args.public_actions);
    let hazard_label = entry
        .map(|e| e.label.clone())
        .or(custom_label.clone())
        .unwrap_or_else(|| "CERC library".into());
    let raw: Option<Raw> = if listing {
        None
    } else if !args.message_map.is_empty() {
        Some(Raw {
            label: hazard_label.clone(),
            question: entry
                .map(|e| e.question.clone())
                .unwrap_or_else(|| "What is happening, and what should I do?".into()),
            empathy: opt(&args.empathy)
                .map(str::to_string)
                .or(entry.map(|e| e.empathy.clone()))
                .unwrap_or_default(),
            key_messages: args
                .message_map
                .iter()
                .map(|k| {
                    (
                        k.text.trim().to_string(),
                        nonempty(&k.supporting)
                            .into_iter()
                            .map(|s| (s, "local".to_string(), Confidence::High))
                            .collect(),
                    )
                })
                .collect(),
            actions: if user_actions.is_empty() {
                entry.map(|e| e.actions.clone()).unwrap_or_default()
            } else {
                user_actions.clone()
            },
            origin: "yours",
        })
    } else if let Some(e) = entry {
        Some(Raw {
            label: e.label.clone(),
            question: e.question.clone(),
            empathy: opt(&args.empathy)
                .map(str::to_string)
                .unwrap_or_else(|| e.empathy.clone()),
            key_messages: e
                .key_messages
                .iter()
                .map(|k| {
                    (
                        k.text.clone(),
                        k.supporting
                            .iter()
                            .map(|f| (f.text.clone(), f.source.clone(), f.confidence))
                            .collect(),
                    )
                })
                .collect(),
            actions: if user_actions.is_empty() {
                e.actions.clone()
            } else {
                user_actions.clone()
            },
            origin: "library",
        })
    } else {
        // Built from the nurse's facts.
        let local = |v: &[String], n: usize| -> Vec<RawFact> {
            v.iter()
                .skip(n)
                .take(3)
                .map(|s| (s.clone(), "local".to_string(), Confidence::High))
                .collect()
        };
        let pad = |mut v: Vec<RawFact>, label: &str| {
            while v.len() < 3 {
                v.push((bracket(label), "local".to_string(), Confidence::High));
            }
            v
        };
        let k1 = known
            .first()
            .cloned()
            .or_else(|| opt(&args.what_happened).map(str::to_string))
            .unwrap_or_else(|| bracket("key message 1: what happened and who is at risk"));
        let k2 = user_actions
            .first()
            .cloned()
            .unwrap_or_else(|| bracket("key message 2: what people should do"));
        let k3 = match opt(&args.next_update) {
            Some(n) => format!("We will share an update {n}."),
            None => bracket("key message 3: what we are doing and when we will update you"),
        };
        Some(Raw {
            label: hazard_label.clone(),
            question: "What is happening, and what should I do?".into(),
            empathy: opt(&args.empathy).map(str::to_string).unwrap_or_else(|| {
                "We know this is worrying, and we are working to keep you and your family safe."
                    .into()
            }),
            key_messages: vec![
                (k1, pad(local(&known, 1), "supporting fact")),
                (k2, pad(local(&user_actions, 1), "supporting action")),
                (k3, pad(local(&doing, 0), "what we are doing")),
            ],
            actions: user_actions.clone(),
            origin: "built_from_inputs",
        })
    };

    // ---- message map
    let lim = &c.message_map;
    let message_map = raw.as_ref().map(|r| {
        let key_messages: Vec<MapMessage> = r
            .key_messages
            .iter()
            .map(|(k, sup)| {
                let text = fill(k, &values, &mut missing);
                MapMessage {
                    words: word_count(&text),
                    text,
                    supporting: sup
                        .iter()
                        .map(|(t, s, conf)| SupportingFact {
                            text: fill(t, &values, &mut missing),
                            citation: source_title(s),
                            confidence: *conf,
                        })
                        .collect(),
                }
            })
            .collect();
        let total: usize = key_messages.iter().map(|k| k.words).sum();
        MessageMap {
            question: fill(&r.question, &values, &mut missing),
            empathy: fill(&r.empathy, &values, &mut missing),
            total_key_words: total,
            estimated_seconds: (total as f64 / lim.words_per_second * 10.0).round() / 10.0,
            key_messages,
            origin: r.origin.to_string(),
        }
    });
    let actions: Vec<String> = raw
        .as_ref()
        .map(|r| {
            r.actions
                .iter()
                .map(|a| fill(a, &values, &mut missing))
                .collect()
        })
        .unwrap_or_default();
    // Bracketed parts of the map that are still to be written (a map built from too few
    // facts is padded with them) count as missing, like unfilled {{placeholders}}.
    if let Some(m) = &message_map {
        for (i, k) in m.key_messages.iter().enumerate() {
            let mut gaps = Vec::new();
            if is_bracket_pad(&k.text) {
                gaps.push(format!("key message {}", i + 1));
            }
            if k.supporting.iter().any(|s| is_bracket_pad(&s.text)) {
                gaps.push(format!("supporting facts for key message {}", i + 1));
            }
            for g in gaps {
                if !missing.contains(&g) {
                    missing.push(g);
                }
            }
        }
    }

    // ---- map checks
    let mut map_checks = Vec::new();
    if let Some(m) = &message_map {
        let n = m.key_messages.len();
        map_checks.push(check(
            "three_key_messages",
            n == lim.key_messages,
            format!("{n} key message(s); use {}", lim.key_messages),
            &rule_source("three_key_messages"),
        ));
        // Bracketed placeholders are not supporting facts.
        let short: Vec<String> = m
            .key_messages
            .iter()
            .enumerate()
            .filter(|(_, k)| written_facts(k) < lim.supporting_per_key)
            .map(|(i, k)| format!("key message {} has {}", i + 1, written_facts(k)))
            .collect();
        map_checks.push(check(
            "three_supporting_facts",
            short.is_empty(),
            if short.is_empty() {
                format!(
                    "each key message has {} supporting facts",
                    lim.supporting_per_key
                )
            } else {
                format!("add supporting facts: {}", short.join("; "))
            },
            &rule_source("three_supporting_facts"),
        ));
        map_checks.push(check(
            "twenty_seven_words",
            m.total_key_words <= lim.max_key_words_total,
            format!(
                "key messages total {} words (limit {}); move details such as times and addresses into supporting facts",
                m.total_key_words, lim.max_key_words_total
            ),
            &rule_source("twenty_seven_nine_three"),
        ));
        map_checks.push(check(
            "nine_seconds",
            m.estimated_seconds <= lim.max_seconds,
            format!(
                "about {:.0} seconds to say at {} words per second (limit {:.0})",
                m.estimated_seconds, lim.words_per_second, lim.max_seconds
            ),
            &rule_source("twenty_seven_nine_three"),
        ));
        map_checks.push(check(
            "empathy_first",
            !m.empathy.trim().is_empty(),
            if m.empathy.trim().is_empty() {
                "add a short statement of empathy before the key messages".into()
            } else {
                "opens with empathy (not counted in the 27 words)".into()
            },
            &rule_source("empathy_first"),
        ));
        let map_text: String = m
            .key_messages
            .iter()
            .flat_map(|k| {
                std::iter::once(k.text.clone()).chain(k.supporting.iter().map(|s| s.text.clone()))
            })
            .map(|s| {
                let s = s.trim().to_string();
                if s.ends_with(['.', '?', '!']) {
                    s
                } else {
                    format!("{s}.")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let a = analyze(
            &map_text,
            &AnalyzeOptions {
                target_grade: target,
                max_flags: 3,
                redact_excerpts: false,
            },
        );
        let avg = a.counts.words as f64 / a.counts.sentences.max(1) as f64;
        map_checks.push(check(
            "short_sentences",
            avg <= lim.max_average_sentence_words,
            format!(
                "{avg:.1} words per sentence on average (aim for {} or fewer)",
                lim.max_average_sentence_words
            ),
            &rule_source("short_sentences"),
        ));
        map_checks.push(check(
            "grade_level",
            a.grade.median_grade <= target,
            format!(
                "message map reads at grade {:.1} (target {target})",
                a.grade.median_grade
            ),
            &rule_source("grade_level"),
        ));
        let key_and_actions = m
            .key_messages
            .iter()
            .map(|k| k.text.clone())
            .chain(std::iter::once(m.empathy.clone()))
            .chain(actions.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");
        let abs = find_terms(&key_and_actions, &c.absolutes);
        map_checks.push(check(
            "no_absolutes",
            abs.is_empty(),
            if abs.is_empty() {
                "no absolutes in the key messages or action steps".into()
            } else {
                format!("avoid absolutes: {}", abs.join(", "))
            },
            &rule_source("no_absolutes"),
        ));
        let neg_key: Vec<String> = m
            .key_messages
            .iter()
            .map(|k| k.text.clone())
            .chain(actions.iter().cloned())
            .filter(|t| !find_terms(t, &c.negatives).is_empty())
            .collect();
        map_checks.push(check(
            "positive_actions",
            neg_key.is_empty(),
            if neg_key.is_empty() {
                "key messages and action steps are stated positively".into()
            } else {
                format!(
                    "consider stating these positively (sometimes the negative is clearer): {}",
                    neg_key.join(" | ")
                )
            },
            &rule_source("positive_actions"),
        ));
        let na = actions.len();
        map_checks.push(check(
            "actions_in_threes",
            (3..=4).contains(&na),
            format!("{na} action step(s); give three or four"),
            &rule_source("actions_in_threes"),
        ));
    }

    // ---- holding statement
    let spokes_text = opt(&args.spokesperson).map(str::to_string);
    let media_position = opt(&args.media_contact_position)
        .unwrap_or("Public Information Officer")
        .to_string();
    let date = chrono::Local::now()
        .format("%B %-d, %Y, %-I:%M %p")
        .to_string();
    let org_text = organization
        .clone()
        .unwrap_or_else(|| bracket("organization"));
    let holding = |contact: &str, missing: &mut Vec<String>| -> String {
        let mut s = format!(
            "**{org_text}: statement on {}**\n\n{date}\n\n",
            hazard_label.to_lowercase()
        );
        if let Some(m) = &message_map
            && !m.empathy.trim().is_empty()
        {
            s.push_str(&format!("{}\n\n", m.empathy.trim()));
        }
        let what = opt(&args.what_happened)
            .map(str::to_string)
            .unwrap_or_else(|| {
                missing.push("what happened".into());
                bracket("what happened, as far as it is confirmed, and when")
            });
        s.push_str(&format!("{what}\n\n"));
        let list = |label: &str, v: &[String], gap: &str, missing: &mut Vec<String>| -> String {
            if v.is_empty() {
                missing.push(label.to_lowercase());
                format!("**{label}:** {}\n\n", bracket(gap))
            } else {
                format!("**{label}:** {}\n\n", v.join(" "))
            }
        };
        s.push_str(&list("What we know", &known, "what is confirmed", missing));
        s.push_str(&list(
            "What we do not know yet",
            &unknown,
            "what is not known yet, such as how many people are affected",
            missing,
        ));
        s.push_str(&list(
            "What we are doing",
            &doing,
            "what your agency and partners are doing; that a plan is in place",
            missing,
        ));
        if !actions.is_empty() {
            s.push_str("**What you can do:**\n\n");
            for (i, a) in actions.iter().enumerate() {
                s.push_str(&format!("{}. {a}\n", i + 1));
            }
            s.push('\n');
        }
        let next = opt(&args.next_update)
            .map(str::to_string)
            .unwrap_or_else(|| {
                missing.push("next update".into());
                bracket("when, e.g. within 2 hours")
            });
        let info = opt(&args.info_source)
            .map(str::to_string)
            .unwrap_or_else(|| {
                missing.push("info source".into());
                bracket("website, public hotline, or 2-1-1")
            });
        s.push_str(&format!(
            "We will share an update {next}. For information: {info}.\n\n"
        ));
        if let Some(sp) = &spokes_text {
            s.push_str(&format!("Spokesperson: {sp}\n\n"));
        }
        s.push_str(&format!("Media contact: {contact}\n"));
        s
    };

    // ---- roster (media contact for the packet only)
    let mut roster_summary = None;
    let mut packet_contact = None;
    if let Some(rp) = opt(&args.roster_path) {
        let r = Roster::load(ctx, rp, None)?;
        let mut tally = RosterTally::default();
        if let Some(sc) = tally.find(Some(&r), None, Some(&media_position)) {
            let line = sc.contact_line();
            packet_contact = Some(format!(
                "{}{} · {}",
                sc.name,
                if sc.position.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", sc.position)
                },
                if line.is_empty() {
                    "[phone and email]".to_string()
                } else {
                    line
                }
            ));
        }
        roster_summary = Some(r.summary(tally.matched, tally.unmatched));
        sources.push(r.source());
        privacy.notes.push(
            "Staff contact details from the roster are written only to the Word packet in the output folder; none are returned here."
                .into(),
        );
    }
    let chat_contact = if roster_summary.as_ref().is_some_and(|r| r.matched > 0) {
        format!(
            "{media_position} (name, phone, and email from your roster are filled in the Word packet)"
        )
    } else if roster_summary.is_some() {
        format!("[no {media_position} row in the roster: add one, or fill this line by hand]")
    } else {
        format!(
            "[{} NAME, PHONE, AND EMAIL: give roster_path to fill them in the Word packet]",
            media_position.to_uppercase()
        )
    };
    let mut statement_missing = Vec::new();
    let holding_statement = raw
        .as_ref()
        .map(|_| holding(&chat_contact, &mut statement_missing));
    for m in statement_missing {
        if !missing.contains(&m) {
            missing.push(m);
        }
    }

    // ---- public message
    let public_template = raw.as_ref().map(compose_public_message);
    let public_message = public_template
        .as_ref()
        .map(|t| fill(t, &values, &mut missing));
    if organization.is_none() && raw.is_some() {
        caveats.push(
            "Give organization (or set your letterhead with configure_jurisdiction) so messages name your agency."
                .into(),
        );
    }

    // ---- statement checks
    let mut statement_checks = Vec::new();
    let mut reading_grade = None;
    if let (Some(h), Some(p)) = (&holding_statement, &public_message) {
        let (checks, g) = text_checks(p, target, "public_message_");
        reading_grade = Some(g);
        statement_checks.extend(
            checks
                .into_iter()
                .filter(|c| !c.id.ends_with("known_unknown_process") && !c.id.ends_with("empathy")),
        );
        let parts_ok = !known.is_empty() && !unknown.is_empty() && !doing.is_empty();
        statement_checks.push(check(
            "holding_known_unknown_process",
            parts_ok,
            if parts_ok {
                "the holding statement says what is known, not known, and being done".into()
            } else {
                "complete what is known, what is not known yet, and what is being done (bracketed in the statement)".into()
            },
            &rule_source("known_unknown_process"),
        ));
        let next_ok = opt(&args.next_update).is_some() && opt(&args.info_source).is_some();
        statement_checks.push(check(
            "next_update_and_source",
            next_ok,
            if next_ok {
                "says when the next update comes and where to get information".into()
            } else {
                "say when the next update comes and where people can get information".into()
            },
            "CERC: Crisis Communication Plans, Checklist 4-1 (tell media when and where to get updates)",
        ));
        let loaded = loaded_words(&h.to_lowercase());
        statement_checks.push(check(
            "holding_no_loaded_words",
            loaded.is_empty(),
            if loaded.is_empty() {
                "no emotionally loaded words in the holding statement".into()
            } else {
                format!("remove loaded words: {}", loaded.join(", "))
            },
            "CERC: Working with the Media (remove adjectives and emotionally loaded words)",
        ));
        statement_checks.push(check(
            "placeholders_filled",
            missing.is_empty(),
            if missing.is_empty() {
                "every placeholder and statement part is filled".into()
            } else {
                format!("still to complete: {}", missing.join(", "))
            },
            "tool rule",
        ));
    }

    // ---- draft checks
    let (draft_checks, draft_reading_grade) = match opt(&args.draft_text) {
        Some(d) => {
            let (v, g) = text_checks(d, target, "draft_");
            (v, Some(g))
        }
        None => (Vec::new(), None),
    };

    // ---- privacy: client identifiers and staff contacts in the request
    let mut narrative: Vec<String> = Vec::new();
    narrative.extend(opt(&args.what_happened).map(str::to_string));
    narrative.extend(known.iter().cloned());
    narrative.extend(unknown.iter().cloned());
    narrative.extend(doing.iter().cloned());
    narrative.extend(user_actions.iter().cloned());
    narrative.extend(opt(&args.empathy).map(str::to_string));
    narrative.extend(opt(&args.draft_text).map(str::to_string));
    narrative.extend(opt(&args.spokesperson).map(str::to_string));
    for k in &args.message_map {
        narrative.push(k.text.clone());
        narrative.extend(k.supporting.iter().cloned());
    }
    let narrative_text = narrative.join("\n");
    let details_text = args
        .details
        .values()
        .cloned()
        .chain(opt(&args.next_update).map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n");
    let mut ids = client_identifiers(&narrative_text, true);
    ids.extend(client_identifiers(&details_text, false));
    ids.sort();
    ids.dedup();
    let contacts = roster::contacts_in_arguments(
        narrative
            .iter()
            .map(String::as_str)
            .chain(args.details.values().map(String::as_str)),
    );
    statement_checks.push(check(
        "privacy_check",
        ids.is_empty(),
        if ids.is_empty() {
            "no client identifiers detected (still check names, ages, and places that could identify a person)".into()
        } else {
            format!(
                "possible client identifiers: {}; remove anything that could identify a person",
                ids.join(", ")
            )
        },
        "CERC: Working with the Media (check privacy and HIPAA before release)",
    ));
    if !ids.is_empty() {
        privacy.identifiers_detected.extend(ids.iter().cloned());
        privacy.notes.push(
            "Possible client identifiers were found in the request; crisis messages must not identify anyone."
                .into(),
        );
    }
    if !contacts.is_empty() {
        for c in &contacts {
            if !privacy.identifiers_detected.contains(c) {
                privacy.identifiers_detected.push(c.clone());
            }
        }
        privacy
            .notes
            .push(roster::CONTACTS_IN_ARGUMENTS_NOTE.into());
    }

    // ---- safe messaging
    let all_text = format!(
        "{}\n{}\n{}",
        narrative_text,
        public_message.clone().unwrap_or_default(),
        holding_statement.clone().unwrap_or_default()
    );
    let safe = all_text
        .to_lowercase()
        .contains("suicid")
        .then(|| safe_messaging::check(&all_text));
    if safe.is_some() {
        sources.extend(safe_messaging::sources());
    }

    // ---- additional protective messages (shared with EPR-05)
    let mut additional = Vec::new();
    if let Some(e) = entry
        && !e.hazard_refs.is_empty()
    {
        let hz: HazardActions = epr_content::load(epr_content::HAZARDS)?;
        let mut keys: Vec<String> = Vec::new();
        for r in &e.hazard_refs {
            if let Some(h) = hz.hazards.iter().find(|h| &h.id == r) {
                additional.extend(h.public_messages.iter().cloned());
                keys.extend(h.message_sources.iter().cloned());
            } else if let Some(h) = hz.secondary_hazards.iter().find(|h| &h.id == r) {
                additional.extend(h.public_messages.iter().cloned());
                keys.extend(h.message_sources.iter().cloned());
            }
        }
        additional.dedup();
        sources.extend(epr_content::sources_for(
            keys.iter().map(String::as_str),
            &hz.sources,
        ));
        caveats.extend(epr_content::review_caveats(&[epr_content::HAZARDS]));
    }

    // ---- FAQs
    let faqs: Vec<Faq> = entry
        .map(|e| {
            e.faqs
                .iter()
                .map(|f| Faq {
                    question: fill(&f.question, &values, &mut missing),
                    answer: fill(&f.answer, &values, &mut missing),
                    citation: source_title(&f.source),
                    confidence: f.confidence,
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(e) = entry {
        sources.extend(epr_content::sources_for(
            e.sources.iter().map(String::as_str),
            &c.sources,
        ));
    }

    // ---- phases
    let phase_tasks: Vec<PhaseTasks> = phases
        .iter()
        .map(|p| PhaseTasks {
            phase: p.id.clone(),
            label: p.label.clone(),
            tasks: cited(&p.tasks),
            hazard_note: entry
                .and_then(|e| e.phase_notes.get(&p.id))
                .map(|n| fill(n, &values, &mut Vec::new())),
        })
        .collect();

    // ---- audiences
    let wanted: Vec<String> = if args.audiences.iter().any(|a| !a.trim().is_empty()) {
        nonempty(&args.audiences)
    } else if let Some(e) = entry {
        e.audiences.clone()
    } else {
        c.audiences.iter().map(|a| a.label.clone()).collect()
    };
    let audiences: Vec<AudienceNote> = wanted
        .iter()
        .map(|w| {
            let n = phn_core::text::normalize(w);
            let hit = c.audiences.iter().find(|a| {
                let l = phn_core::text::normalize(&a.label);
                let id = a.id.replace('_', " ");
                l == n
                    || n.contains(&id)
                    || n.split(' ')
                        .filter(|t| t.len() > 4)
                        .any(|t| l.split(' ').any(|x| x == t))
            });
            AudienceNote {
                audience: w.clone(),
                concerns: hit.map(|a| a.concerns.clone()).unwrap_or_default(),
                citation: hit.map(|a| format!("{}, {}", source_title(&a.source), a.section)),
            }
        })
        .collect();

    // ---- spokesperson
    let mut questions: Vec<String> = Vec::new();
    for q in CAMPAIGN
        .spokesperson_questions
        .iter()
        .chain(c.media_questions.questions.iter())
    {
        if !questions.iter().any(|x| x.eq_ignore_ascii_case(q)) {
            questions.push(q.clone());
        }
    }
    let spokesperson = SpokespersonPrep {
        spokesperson: spokes_text.clone(),
        key_point_limit: format!(
            "Make the key point first, in about {} seconds and fewer than {} words.",
            lim.spokesperson_key_point_seconds, lim.spokesperson_key_point_words
        ),
        tips: cited(&c.spokesperson.tips),
        bridges: c.spokesperson.bridges.clone(),
        bridges_source: source_title(&c.spokesperson.bridges_source),
        questions_to_prepare: questions,
        questions_source: format!(
            "{}, {}",
            source_title(&c.media_questions.source),
            c.media_questions.section
        ),
        media_holding_lines: cited(&c.initial_statement.media_holding_lines),
    };

    // ---- language plan
    let area = opt(&args.area)
        .map(str::to_string)
        .or_else(|| ctx.settings().jurisdiction.town);
    let template_kind = entry.map(|e| format!("cerc_{}", e.id));
    let mut rows: Vec<LanguagePlanRow> = Vec::new();
    let explicit = nonempty(&args.languages);
    if !listing {
        if !explicit.is_empty() {
            for l in &explicit {
                rows.push(LanguagePlanRow {
                    language: l.clone(),
                    lep_estimate: None,
                    geography: None,
                    plan: String::new(),
                    script_note: None,
                    file: None,
                });
            }
        } else if let Some(a) = &area {
            match translation::language_priorities(ctx, a).await {
                Ok((pri, src)) => {
                    sources.extend(src);
                    for p in pri {
                        rows.push(LanguagePlanRow {
                            language: p.language,
                            lep_estimate: Some(p.lep_estimate),
                            geography: Some(p.geography),
                            plan: String::new(),
                            script_note: None,
                            file: None,
                        });
                    }
                }
                Err(e) => caveats.push(e),
            }
        } else {
            caveats.push(
                "No town was given or saved, so the language plan lists only the languages you named; give area for the town's priority languages."
                    .into(),
            );
        }
        for l in ctx.settings().preferred_languages {
            if !templates::is_english(&l)
                && !rows.iter().any(|r| {
                    folder_language(&r.language).eq_ignore_ascii_case(&folder_language(&l))
                })
            {
                rows.push(LanguagePlanRow {
                    language: l,
                    lep_estimate: None,
                    geography: Some("your saved preferred languages".into()),
                    plan: String::new(),
                    script_note: None,
                    file: None,
                });
            }
        }
        rows.retain(|r| !templates::is_english(&r.language));
    }
    let english_template = public_template.clone().unwrap_or_default();
    let kind_owned = template_kind
        .clone()
        .unwrap_or_else(|| "cerc_custom".into());
    let mut resolver = TemplateResolver::new(ctx, &kind_owned, english_template.clone());
    let mut briefs: Vec<(usize, TranslatorBrief)> = Vec::new();
    let mut agency_docs: Vec<(usize, templates::Chosen)> = Vec::new();
    // An approved translation matches only the unmodified library message. With the
    // nurse's own map, action steps, or empathy statement, the English no longer says
    // what the translation says, so the translation is not used.
    let library_unmodified = raw.as_ref().is_some_and(|r| r.origin == "library")
        && user_actions.is_empty()
        && opt(&args.empathy).is_none();
    let mut stale_translations: Vec<String> = Vec::new();
    for (i, r) in rows.iter_mut().enumerate() {
        let fl = folder_language(&r.language);
        let brief = translation::translator_brief(&fl, public_message.as_deref().unwrap_or(""));
        r.script_note = brief.script_note.clone();
        let chosen = template_kind
            .as_ref()
            .map(|_| resolver.choose(&fl))
            .filter(|ch| ch.translated);
        let chosen = match chosen {
            Some(_) if !library_unmodified => {
                stale_translations.push(r.language.clone());
                None
            }
            other => other,
        };
        match chosen {
            Some(ch) => {
                r.plan = "agency_translation".into();
                agency_docs.push((i, ch));
            }
            None => {
                r.plan = "translator_brief".into();
                if r.language.to_lowercase().starts_with("other") {
                    r.script_note = Some(
                        "A group of languages: confirm which languages your residents speak before translating."
                            .into(),
                    );
                }
                briefs.push((i, brief));
            }
        }
    }
    if !briefs.is_empty() {
        sources.extend(translation::translation_sources());
    }
    if !stale_translations.is_empty() {
        caveats.push(format!(
            "Your agency's approved translation was NOT used for {}: the English message was changed (your own message map, action steps, or empathy statement), so the approved translation no longer matches it. A translator brief was prepared instead; have the changed message translated and approved before release.",
            stale_translations.join(", ")
        ));
    }
    let mut language_plan = LanguagePlan {
        area: area.clone(),
        template_kind: template_kind.clone(),
        rows,
        how_to_add_translations: templates::translation_help(&[kind_owned.as_str()]),
    };

    // ---- export
    let mut files = Vec::new();
    if export && !listing {
        let id = entry.map(|e| e.id.clone()).unwrap_or_else(|| {
            let s = file_slug(&hazard_label);
            if s.is_empty() {
                "custom".into()
            } else {
                s.chars().take(40).collect()
            }
        });
        let (dir, _shown) = templates::output_folder(ctx, &format!("cerc_{id}"))?;
        // English packet.
        let contact = packet_contact
            .clone()
            .unwrap_or_else(|| chat_contact.clone());
        let packet_holding = holding(&contact, &mut Vec::new());
        let mut blocks: Vec<Block> = Vec::new();
        let p = |t: &str| Block::Paragraph(vec![Inline::plain(t.to_string())]);
        blocks.push(Block::Heading {
            level: 1,
            text: "Holding statement".into(),
        });
        blocks.extend(from_markdown(&packet_holding));
        if let Some(m) = &message_map {
            blocks.push(Block::Heading {
                level: 1,
                text: "Message map".into(),
            });
            blocks.push(p(&format!("Question or concern: {}", m.question)));
            if !m.empathy.is_empty() {
                blocks.push(p(&format!("Empathy statement: {}", m.empathy)));
            }
            blocks.push(Block::Table(TableBlock {
                caption: Some(format!(
                    "Key messages ({} words, about {:.0} seconds) and supporting facts",
                    m.total_key_words, m.estimated_seconds
                )),
                headers: [
                    "Key message",
                    "Supporting fact 1",
                    "Supporting fact 2",
                    "Supporting fact 3",
                ]
                .map(String::from)
                .to_vec(),
                rows: m
                    .key_messages
                    .iter()
                    .map(|k| {
                        let mut row = vec![k.text.clone()];
                        for i in 0..3 {
                            row.push(
                                k.supporting
                                    .get(i)
                                    .map(|s| s.text.clone())
                                    .unwrap_or_default(),
                            );
                        }
                        row
                    })
                    .collect(),
            }));
        }
        if let Some(pm) = &public_message {
            blocks.push(Block::Heading {
                level: 1,
                text: "Public message (English source for translation)".into(),
            });
            blocks.extend(from_markdown(&pm.replacen("# ", "## ", 1)));
        }
        if !faqs.is_empty() {
            blocks.push(Block::Heading {
                level: 1,
                text: "Frequently asked questions".into(),
            });
            for f in &faqs {
                blocks.push(Block::Heading {
                    level: 2,
                    text: f.question.clone(),
                });
                blocks.push(p(&f.answer));
            }
        }
        blocks.push(Block::Heading {
            level: 1,
            text: "Spokesperson preparation".into(),
        });
        blocks.push(p(&spokesperson.key_point_limit));
        let list = |v: Vec<String>| Block::List {
            ordered: false,
            items: v.into_iter().map(|t| vec![Inline::plain(t)]).collect(),
        };
        blocks.push(list(
            spokesperson.tips.iter().map(|t| t.text.clone()).collect(),
        ));
        blocks.push(Block::Heading {
            level: 2,
            text: "Bridges back to the key message".into(),
        });
        blocks.push(list(spokesperson.bridges.clone()));
        blocks.push(Block::Heading {
            level: 2,
            text: "Questions to prepare for".into(),
        });
        blocks.push(list(spokesperson.questions_to_prepare.clone()));
        blocks.push(Block::Heading {
            level: 2,
            text: "Holding lines for reporters".into(),
        });
        blocks.push(list(
            spokesperson
                .media_holding_lines
                .iter()
                .map(|t| t.text.clone())
                .collect(),
        ));
        for pt in &phase_tasks {
            blocks.push(Block::Heading {
                level: 1,
                text: format!("CERC tasks: {}", pt.label),
            });
            let mut items: Vec<String> = pt.tasks.iter().map(|t| t.text.clone()).collect();
            if let Some(n) = &pt.hazard_note {
                items.push(format!("For this hazard: {n}"));
            }
            blocks.push(list(items));
        }
        blocks.push(Block::Heading {
            level: 1,
            text: "Audiences".into(),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some(
                "Audiences and their concerns (CERC Messages and Audiences, Table 1)".into(),
            ),
            headers: vec!["Audience".into(), "Typical concerns".into()],
            rows: audiences
                .iter()
                .map(|a| vec![a.audience.clone(), a.concerns.join("; ")])
                .collect(),
        }));
        let all_checks: Vec<&CercCheck> = map_checks
            .iter()
            .chain(statement_checks.iter())
            .chain(draft_checks.iter())
            .collect();
        blocks.push(Block::Heading {
            level: 1,
            text: "Checks".into(),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some("CERC and message-mapping checks".into()),
            headers: vec!["Check".into(), "Result".into(), "Detail".into()],
            rows: all_checks
                .iter()
                .map(|c| {
                    vec![
                        c.id.replace('_', " "),
                        if c.passed { "pass" } else { "fix" }.into(),
                        c.detail.clone(),
                    ]
                })
                .collect(),
        }));
        // Language files.
        for (i, ch) in &agency_docs {
            let lang = language_plan.rows[*i].language.clone();
            let filled = fill(&ch.markdown, &values, &mut Vec::new());
            let doc = templates::document(ctx, &filled, &ch.tag, &hazard_label);
            let bytes = DocFormat::Docx.render(&doc)?;
            let f = templates::write_in(
                ctx,
                &dir,
                &format!("cerc_{id}_{}.docx", file_slug(&folder_language(&lang))),
                &bytes,
            )?;
            language_plan.rows[*i].file = Some(f.clone());
            files.push(f);
        }
        for (i, b) in &briefs {
            let lang = language_plan.rows[*i].language.clone();
            let doc = Document {
                title: format!("Translator brief: {} ({})", hazard_label, b.language),
                subtitle: Some(b.draft_label.clone()),
                language: "en-US".into(),
                author: None,
                letterhead: ctx.settings().letterhead,
                logo: None,
                blocks: vec![
                    Block::Paragraph(vec![Inline::plain(format!(
                        "Translate each numbered segment into {}. Keep the items in the second table exactly as written. {}",
                        b.language,
                        b.script_note.clone().unwrap_or_default()
                    ))]),
                    Block::Table(TableBlock {
                        caption: Some("Segments".into()),
                        headers: ["#", "English", "Translation", "Reviewer comments"]
                            .map(String::from)
                            .to_vec(),
                        rows: b
                            .segments
                            .iter()
                            .enumerate()
                            .map(|(n, s)| {
                                vec![(n + 1).to_string(), s.clone(), String::new(), String::new()]
                            })
                            .collect(),
                    }),
                    Block::Table(TableBlock {
                        caption: Some("Keep unchanged".into()),
                        headers: vec!["Kind".into(), "Value".into()],
                        rows: b
                            .keep_unchanged
                            .iter()
                            .map(|k| vec![k.kind.clone(), k.value.clone()])
                            .collect(),
                    }),
                    Block::Heading {
                        level: 2,
                        text: "Translation workflow".into(),
                    },
                    Block::List {
                        ordered: true,
                        items: b
                            .workflow
                            .iter()
                            .map(|w| vec![Inline::plain(w.clone())])
                            .collect(),
                    },
                ],
            };
            let bytes = DocFormat::Docx.render(&doc)?;
            let f = templates::write_in(
                ctx,
                &dir,
                &format!(
                    "translator_brief_{}.docx",
                    file_slug(&folder_language(&lang))
                ),
                &bytes,
            )?;
            language_plan.rows[*i].file = Some(f.clone());
            files.push(f);
        }
        blocks.push(Block::Heading {
            level: 1,
            text: "Language plan".into(),
        });
        blocks.push(Block::Table(TableBlock {
            caption: Some("Languages and how each will be reached".into()),
            headers: vec![
                "Language".into(),
                "Residents with limited English (ACS)".into(),
                "Plan".into(),
            ],
            rows: language_plan
                .rows
                .iter()
                .map(|r| {
                    vec![
                        r.language.clone(),
                        r.lep_estimate
                            .map(|v| format!("{v:.0}"))
                            .unwrap_or_default(),
                        r.plan.replace('_', " "),
                    ]
                })
                .collect(),
        }));
        blocks.push(p(&language_plan.how_to_add_translations));
        let doc = Document {
            title: format!("CERC message packet: {hazard_label}"),
            subtitle: Some("Draft for review and approval before release".into()),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks,
        };
        let bytes = DocFormat::Docx.render(&doc)?;
        let f = templates::write_in(ctx, &dir, &format!("cerc_packet_{id}.docx"), &bytes)?;
        files.insert(0, f);
        if entry.is_some() {
            let f = templates::write_in(
                ctx,
                &dir,
                &format!("template_english_{kind_owned}.md"),
                english_template.as_bytes(),
            )?;
            files.push(f);
        }
        privacy.files_written.extend(files.iter().cloned());
    }

    // ---- summary
    let mut summary = String::new();
    if listing {
        summary.push_str("**CERC pre-scripted message library**\n\n");
        for e in &c.library {
            summary.push_str(&format!(
                "- `{}`: {}{}\n",
                e.id,
                e.label,
                if e.placeholders.is_empty() {
                    String::new()
                } else {
                    format!(" (details: {})", e.placeholders.join(", "))
                }
            ));
        }
        summary.push_str(
            "\nName a hazard (or describe another one in hazard_description) to build a message map, holding statement, FAQs, spokesperson preparation, and a language plan.\n\n",
        );
    } else {
        summary.push_str(&format!(
            "**CERC message set: {hazard_label}** (phase: {phase_key}; reading grade {}; target {target})\n\n",
            reading_grade
                .map(|g| format!("{g:.1}"))
                .unwrap_or_else(|| "n/a".into())
        ));
        if let Some(m) = &message_map {
            summary.push_str(&format!(
                "**Message map** ({} key words, about {:.0} seconds)\n\n_{}_\n\n{}\n\n",
                m.total_key_words, m.estimated_seconds, m.question, m.empathy
            ));
            for (i, k) in m.key_messages.iter().enumerate() {
                summary.push_str(&format!("{}. **{}**\n", i + 1, k.text));
                for s in &k.supporting {
                    let flag = match s.confidence {
                        Confidence::Low => " (low confidence)",
                        _ => "",
                    };
                    summary.push_str(&format!("   - {}{flag}\n", s.text));
                }
            }
            summary.push('\n');
        }
        if let Some(h) = &holding_statement {
            summary.push_str("**Holding statement**\n\n");
            for line in h.lines() {
                summary.push_str(&format!("> {line}\n"));
            }
            summary.push('\n');
        }
        let failed: Vec<&CercCheck> = map_checks
            .iter()
            .chain(statement_checks.iter())
            .chain(draft_checks.iter())
            .filter(|c| !c.passed)
            .collect();
        if failed.is_empty() {
            summary.push_str("All message-map and CERC checks passed.\n\n");
        } else {
            summary.push_str("**Checks to fix**\n");
            for f in &failed {
                summary.push_str(&format!("- {}: {}\n", f.id.replace('_', " "), f.detail));
            }
            summary.push('\n');
        }
        if !faqs.is_empty() {
            summary.push_str("**FAQs**\n");
            for f in &faqs {
                summary.push_str(&format!("- _{}_ {}\n", f.question, f.answer));
            }
            summary.push('\n');
        }
        if !additional.is_empty() {
            summary.push_str(&format!(
                "**Other protective messages:** {}\n\n",
                additional.join(" ")
            ));
        }
        if let Some(s) = &safe {
            summary.push_str(&format!(
                "**Safe messaging (suicide):** {} problem(s), {} must be fixed.\n\n",
                s.findings.len(),
                s.must_fix
            ));
        }
    }
    for pt in &phase_tasks {
        summary.push_str(&format!("**CERC tasks: {}**\n", pt.label));
        for t in &pt.tasks {
            summary.push_str(&format!("- {}\n", t.text));
        }
        if let Some(n) = &pt.hazard_note {
            summary.push_str(&format!("- For this hazard: {n}\n"));
        }
        summary.push('\n');
    }
    if !listing {
        summary.push_str(&format!(
            "**Spokesperson:** {} Bridges: \"{}\". Prepare for: {}\n\n",
            spokesperson.key_point_limit,
            spokesperson.bridges.first().cloned().unwrap_or_default(),
            spokesperson
                .questions_to_prepare
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        ));
        if language_plan.rows.is_empty() {
            summary.push_str("**Languages:** none listed (give area or languages).\n\n");
        } else {
            summary.push_str("**Language plan** (no machine translation):\n");
            for r in &language_plan.rows {
                summary.push_str(&format!(
                    "- {}{}: {}\n",
                    r.language,
                    r.lep_estimate
                        .map(|v| format!(" ({v:.0} residents with limited English)"))
                        .unwrap_or_default(),
                    if r.plan == "agency_translation" {
                        "your agency's approved translation"
                    } else {
                        "translator brief (no approved translation found)"
                    }
                ));
            }
            summary.push('\n');
        }
        if !missing.is_empty() {
            summary.push_str(&format!("Still to complete: {}.\n\n", missing.join(", ")));
        }
    }
    summary.push_str(&format!(
        "**CERC principles:** {}.\n",
        CAMPAIGN
            .cerc_principles
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if !files.is_empty() {
        summary.push_str(&format!(
            "\nFiles written to the output folder: {}.\n",
            files
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // ---- caveats
    caveats.insert(
        0,
        "These are drafts to adapt and clear through your agency's approval process; verify every fact with your subject matter experts and partners before release (CERC: be right, be credible).".into(),
    );
    if entry.is_some_and(|e| e.id == "mass_dispensing_pod") {
        caveats.push(
            "Mass dispensing messages depend on your POD plan (forms, who may pick up medicine, dosing by weight); items marked low confidence must be replaced with your plan's wording."
                .into(),
        );
    }
    if entry.is_some_and(|e| e.id == "boil_water") {
        caveats.push(
            "Boil water orders are issued by the public water supplier with MassDEP; coordinate wording and timing with them."
                .into(),
        );
    }
    if entry.is_some_and(|e| e.id == "respiratory_pandemic") {
        caveats.push(
            "Vaccine recommendations differ among DPH, professional societies, and CDC; see compare_vaccine_recommendations before naming specific vaccines."
                .into(),
        );
    }
    if !language_plan.rows.is_empty() {
        caveats.push(
            "No message is machine-translated: use your agency's approved translations or a qualified translator, and test translated messages with the community (CERC preparation phase).".into(),
        );
    }
    caveats.extend(epr_content::review_caveats(&[CERC]));
    caveats.extend(crate::comms::content::review_caveats(&[
        crate::comms::content::CAMPAIGNS,
    ]));

    let related_tools = entry.map(|e| e.related_tools.clone()).unwrap_or_default();
    let rules = c
        .message_map
        .rules
        .iter()
        .map(|r| format!("{} ({}, {})", r.text, source_title(&r.source), r.section))
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    sources.retain(|s| seen.insert((s.title.clone(), s.url.clone())));

    Ok(Envelope::new(
        summary,
        CercResult {
            hazard_id: entry.map(|e| e.id.clone()),
            hazard_label,
            phase: phase_key,
            library,
            phase_tasks,
            throughout: cited(&c.throughout),
            message_map,
            holding_statement,
            public_message,
            actions,
            additional_protective_messages: additional,
            faqs,
            spokesperson,
            audiences,
            audience_reminders: cited(&c.audience_reminders),
            initial_message_elements: cited(&c.initial_statement.elements),
            first_48_hours: cited(&c.initial_statement.first_48_hours),
            pitfalls: c.pitfalls.iter().map(|p| p.text.clone()).collect(),
            psychology: cited(&c.psychology),
            message_rules: rules,
            map_checks,
            statement_checks,
            draft_checks,
            reading_grade,
            draft_reading_grade,
            target_grade: target,
            safe_messaging: safe,
            missing,
            language_plan,
            roster: roster_summary,
            related_tools,
            files,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy)
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::create_dir_all(dir.path().join("cache")).unwrap();
        let mut cfg = phn_core::Config::for_root(dir.path());
        cfg.offline = true;
        let ctx = Context::new(cfg).unwrap();
        (dir, ctx)
    }

    fn run(ctx: &Context, args: CercArgs) -> Result<Envelope<CercResult>> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(crisis_risk_communication_builder(ctx, args))
    }

    #[test]
    fn every_library_map_fits_the_message_mapping_rules() {
        let c = &*CONTENT;
        assert_eq!(c.library.len(), 7);
        for e in &c.library {
            assert_eq!(e.key_messages.len(), 3, "{}", e.id);
            let words: usize = e.key_messages.iter().map(|k| word_count(&k.text)).sum();
            assert!(words <= 27, "{} key messages have {words} words", e.id);
            for k in &e.key_messages {
                assert_eq!(k.supporting.len(), 3, "{}: {}", e.id, k.text);
                for s in &k.supporting {
                    assert!(
                        s.source == "local" || c.sources.contains_key(&s.source),
                        "{}: unknown source {}",
                        e.id,
                        s.source
                    );
                }
                assert!(
                    find_terms(&k.text, &c.absolutes).is_empty(),
                    "{}: absolute in {}",
                    e.id,
                    k.text
                );
            }
            assert!((3..=4).contains(&e.actions.len()), "{}", e.id);
            for f in &e.faqs {
                assert!(c.sources.contains_key(&f.source), "{}: {}", e.id, f.source);
            }
            for s in &e.sources {
                assert!(c.sources.contains_key(s), "{}: {s}", e.id);
            }
            // Every placeholder used in the entry is declared.
            let text = format!(
                "{} {} {}",
                e.question,
                e.key_messages
                    .iter()
                    .map(|k| format!(
                        "{} {}",
                        k.text,
                        k.supporting
                            .iter()
                            .map(|s| s.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    ))
                    .collect::<Vec<_>>()
                    .join(" "),
                e.actions.join(" ")
            );
            for cap in text.split("{{").skip(1) {
                let name = cap.split("}}").next().unwrap().trim();
                assert!(
                    e.placeholders.iter().any(|p| p == name)
                        || ["organization", "info_source", "next_update"].contains(&name),
                    "{}: undeclared placeholder {name}",
                    e.id
                );
            }
        }
        // Hazard references resolve in hazard_health_actions.yaml.
        let hz: HazardActions = epr_content::load(epr_content::HAZARDS).unwrap();
        for e in &c.library {
            for r in &e.hazard_refs {
                assert!(
                    hz.hazards.iter().any(|h| &h.id == r)
                        || hz.secondary_hazards.iter().any(|h| &h.id == r),
                    "{}: unknown hazard {r}",
                    e.id
                );
            }
        }
    }

    /// The bundled message sets meet their own rules: short sentences and a
    /// reading grade at or below the default target, in the map and the public
    /// message (placeholders read as one word).
    #[test]
    fn library_messages_are_plain_language() {
        let opts = AnalyzeOptions {
            target_grade: default_target_grade(),
            max_flags: 3,
            redact_excerpts: false,
        };
        let lim = &CONTENT.message_map;
        for e in &CONTENT.library {
            let map: String = e
                .key_messages
                .iter()
                .flat_map(|k| {
                    std::iter::once(k.text.clone())
                        .chain(k.supporting.iter().map(|s| s.text.clone()))
                })
                .map(|s| s.replace("{{", "").replace("}}", ""))
                .map(|s| {
                    if s.ends_with(['.', '?', '!']) {
                        s
                    } else {
                        format!("{s}.")
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let a = analyze(&map, &opts);
            let avg = a.counts.words as f64 / a.counts.sentences.max(1) as f64;
            assert!(
                avg <= lim.max_average_sentence_words,
                "{}: {avg:.1} words per sentence",
                e.id
            );
            assert!(
                a.grade.meets_target,
                "{}: map grade {:.1}",
                e.id, a.grade.median_grade
            );
            let pm = compose_public_message(&Raw {
                label: e.label.clone(),
                question: e.question.clone(),
                empathy: e.empathy.clone(),
                key_messages: e
                    .key_messages
                    .iter()
                    .map(|k| {
                        (
                            k.text.clone(),
                            k.supporting
                                .iter()
                                .map(|f| (f.text.clone(), f.source.clone(), f.confidence))
                                .collect(),
                        )
                    })
                    .collect(),
                actions: e.actions.clone(),
                origin: "library",
            });
            let b = analyze(&pm.replace("{{", "").replace("}}", ""), &opts);
            assert!(
                b.grade.meets_target,
                "{}: public message grade {:.1}",
                e.id, b.grade.median_grade
            );
            assert_eq!(b.flags.long_sentence_count, 0, "{}", e.id);
            assert!(loaded_words(&pm.to_lowercase()).is_empty(), "{}", e.id);
        }
    }

    #[test]
    fn finds_library_entries_by_alias() {
        assert_eq!(find_entry("measles").unwrap().id, "measles_exposure");
        assert_eq!(
            find_entry("Hurricane").unwrap().id,
            "coastal_storm_power_outage"
        );
        assert_eq!(find_entry("POD").unwrap().id, "mass_dispensing_pod");
        assert_eq!(find_entry("boil water").unwrap().id, "boil_water");
        assert!(find_entry("volcano").is_none());
        // Aliases match as whole words, never inside a word.
        assert_eq!(
            find_entry("measles at the library").unwrap().id,
            "measles_exposure"
        );
        for q in [
            "norovirus, hundreds ill",
            "sewage backup, do not flush",
            "space heater fire",
            "fluoride overfeed",
        ] {
            assert!(find_entry(q).is_none(), "{q}");
        }
        // A description selects a set only when it is exactly the set's name.
        assert_eq!(find_entry_exact("heat wave").unwrap().id, "extreme_heat");
        assert!(find_entry_exact("no heat in senior housing").is_none());
    }

    #[test]
    fn a_described_hazard_is_built_from_the_facts_not_a_lookalike_set() {
        let (_d, ctx) = ctx();
        let r = run(
            &ctx,
            CercArgs {
                hazard_description: Some("no heat in senior housing".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.data.hazard_id.is_none());
        assert_eq!(
            r.data.message_map.as_ref().unwrap().origin,
            "built_from_inputs"
        );
        assert!(
            r.caveats
                .iter()
                .any(|c| c.contains("pass hazard \"extreme_heat\"")),
            "{:?}",
            r.caveats
        );
    }

    #[test]
    fn bracketed_parts_of_a_built_map_are_reported_as_missing() {
        let (_d, ctx) = ctx();
        let r = run(
            &ctx,
            CercArgs {
                hazard_description: Some("water main break".into()),
                organization: Some("Riverton Board of Health".into()),
                what_happened: Some("A water main broke on Elm Street at 6 a.m.".into()),
                known: vec!["Water pressure is low in the north end.".into()],
                unknown: vec!["We do not know yet how long repairs will take.".into()],
                being_done: vec!["Crews are repairing the main.".into()],
                public_actions: vec![
                    "Use bottled water for drinking.".into(),
                    "Check on neighbors.".into(),
                    "Call 2-1-1 for help.".into(),
                ],
                next_update: Some("by 4 p.m. today".into()),
                info_source: Some("the town website".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert!(d.public_message.as_ref().unwrap().contains("[SUPPORTING FACT]"));
        assert!(
            d.missing
                .iter()
                .any(|m| m == "supporting facts for key message 1"),
            "{:?}",
            d.missing
        );
        let failed = |checks: &[CercCheck], id: &str| checks.iter().any(|c| c.id == id && !c.passed);
        assert!(failed(&d.map_checks, "three_supporting_facts"));
        assert!(failed(&d.statement_checks, "placeholders_filled"));
        assert!(!r.summary.contains("All message-map and CERC checks passed"));
    }

    #[test]
    fn an_approved_translation_is_used_only_for_the_unmodified_library_message() {
        let (dir, ctx) = ctx();
        let folder = dir.path().join("data/templates/spanish");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("cerc_boil_water.md"),
            "# Aviso de hervir el agua\n\nHierva el agua durante un minuto.\n",
        )
        .unwrap();
        let plan = |actions: Vec<String>| {
            let r = run(
                &ctx,
                CercArgs {
                    hazard: Some("boil_water".into()),
                    languages: vec!["Spanish".into()],
                    public_actions: actions,
                    ..Default::default()
                },
            )
            .unwrap();
            (r.data.language_plan.rows[0].plan.clone(), r.caveats.clone())
        };
        // The library message as written: the approved translation is used.
        let (p, caveats) = plan(vec![]);
        assert_eq!(p, "agency_translation");
        assert!(!caveats.iter().any(|c| c.contains("NOT used")));
        // Changed action steps: the translation no longer matches the English.
        let (p, caveats) = plan(vec!["Do not drink tap water; use bottled water only.".into()]);
        assert_eq!(p, "translator_brief");
        assert!(
            caveats
                .iter()
                .any(|c| c.contains("NOT used for Spanish")),
            "{caveats:?}"
        );
    }

    #[test]
    fn measles_set_fills_details_and_checks_the_map() {
        let (_d, ctx) = ctx();
        let mut details = BTreeMap::new();
        details.insert("location".into(), "the Main Street Library".into());
        details.insert(
            "exposure_times".into(),
            "Monday, November 2, from 1 to 4 p.m.".into(),
        );
        let r = run(
            &ctx,
            CercArgs {
                hazard: Some("measles".into()),
                organization: Some("Springfield Health Department".into()),
                details,
                what_happened: Some(
                    "A person with measles visited the Main Street Library on November 2.".into(),
                ),
                known: vec!["One person has measles.".into()],
                unknown: vec!["We do not know yet whether anyone else is sick.".into()],
                being_done: vec!["We are calling people who were there.".into()],
                next_update: Some("by 5 p.m. tomorrow".into()),
                info_source: Some("springfield.example.gov/measles or 413-555-0100".into()),
                languages: vec!["Spanish".into(), "Portuguese".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert_eq!(d.hazard_id.as_deref(), Some("measles_exposure"));
        let m = d.message_map.as_ref().unwrap();
        assert!(m.key_messages[0].text.contains("Main Street Library"));
        assert!(m.key_messages[0].supporting[0].text.contains("November 2"));
        assert!(
            m.key_messages[0].supporting[0].text.ends_with("p.m."),
            "{}",
            m.key_messages[0].supporting[0].text
        );
        assert!(d.missing.is_empty(), "{:?}", d.missing);
        assert!(
            d.map_checks.iter().all(|c| c.passed),
            "{:?}",
            d.map_checks
                .iter()
                .filter(|c| !c.passed)
                .collect::<Vec<_>>()
        );
        let h = d.holding_statement.as_ref().unwrap();
        assert!(h.contains("What we do not know yet"));
        assert!(h.contains("by 5 p.m. tomorrow"));
        assert!(
            d.public_message
                .as_ref()
                .unwrap()
                .contains("Springfield Health Department")
        );
        assert_eq!(d.language_plan.rows.len(), 2);
        assert!(
            d.language_plan
                .rows
                .iter()
                .all(|r| r.plan == "translator_brief")
        );
        assert_eq!(d.phase, "initial");
        assert_eq!(d.phase_tasks.len(), 1);
        assert!(!d.faqs.is_empty() && !d.spokesperson.bridges.is_empty());
        assert!(r.caveats.iter().any(|c| c.contains("machine-translated")));
    }

    #[test]
    fn custom_hazard_marks_missing_parts_and_checks_a_draft() {
        let (_d, ctx) = ctx();
        let r = run(
            &ctx,
            CercArgs {
                hazard_description: Some("water main break at the high school".into()),
                phase: Some("all".into()),
                draft_text: Some(
                    "A devastating water main break happened. The water is completely safe. You will never be at risk."
                        .into(),
                ),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert!(d.hazard_id.is_none());
        assert_eq!(d.phase_tasks.len(), 4);
        let m = d.message_map.as_ref().unwrap();
        assert_eq!(m.origin, "built_from_inputs");
        assert!(m.key_messages[0].text.starts_with('['));
        assert!(d.missing.iter().any(|x| x == "what happened"));
        let failed: Vec<&str> = d
            .draft_checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.id.as_str())
            .collect();
        assert!(
            failed.contains(&"draft_no_loaded_words")
                && failed.contains(&"draft_no_absolutes")
                && failed.contains(&"draft_empathy"),
            "{failed:?}"
        );
    }

    #[test]
    fn own_map_over_the_limits_fails_the_checks() {
        let (_d, ctx) = ctx();
        let r = run(
            &ctx,
            CercArgs {
                hazard: Some("boil water".into()),
                message_map: vec![
                    KeyMessageInput {
                        text: "Do not drink the tap water in the northern part of town until further notice from the water department and the board of health.".into(),
                        supporting: vec!["Boil it.".into()],
                    },
                    KeyMessageInput {
                        text: "We always guarantee your safety.".into(),
                        supporting: vec![],
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap();
        let failed: Vec<&str> = r
            .data
            .map_checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.id.as_str())
            .collect();
        for id in [
            "three_key_messages",
            "three_supporting_facts",
            "twenty_seven_words",
            "nine_seconds",
            "no_absolutes",
            "positive_actions",
        ] {
            assert!(failed.contains(&id), "{id} should fail: {failed:?}");
        }
        assert_eq!(r.data.message_map.unwrap().origin, "yours");
    }

    #[test]
    fn listing_and_errors() {
        let (_d, ctx) = ctx();
        let r = run(&ctx, CercArgs::default()).unwrap();
        assert_eq!(r.data.library.len(), 7);
        assert!(r.data.message_map.is_none());
        assert!(r.summary.contains("measles_exposure"));
        let e = run(
            &ctx,
            CercArgs {
                hazard: Some("volcano".into()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("hazard_description"), "{e}");
        assert!(
            run(
                &ctx,
                CercArgs {
                    phase: Some("aftermath party".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            run(
                &ctx,
                CercArgs {
                    hazard: Some("heat".into()),
                    export: Some("pdf".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn folder_language_names() {
        assert_eq!(
            folder_language("Chinese (incl. Mandarin, Cantonese)"),
            "Chinese"
        );
        assert_eq!(folder_language("Haitian"), "Haitian Creole");
        assert_eq!(
            folder_language("Amharic, Somali, or other Afro-Asiatic languages"),
            "Amharic"
        );
        assert_eq!(folder_language("Spanish"), "Spanish");
    }
}
