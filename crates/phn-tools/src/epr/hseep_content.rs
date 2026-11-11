//! Typed access to the Phase 13 HSEEP content (`content/epr/capabilities.yaml`,
//! `hseep.yaml`, `exercise_scenarios.yaml`) and helpers shared by
//! `design_hseep_exercise` (EPR-08) and `build_after_action_report` (EPR-09):
//! capability lookup across the CDC PHEP and FEMA core capability frameworks,
//! this project's PHEP → FEMA mapping, SMART checks, and document helpers.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_core::Source;
use phn_docs::model::{Block, Inline, TableBlock};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, DocSource};

/// Content ids.
pub const CAPABILITIES: &str = "epr-capabilities";
pub const HSEEP: &str = "epr-hseep";
pub const SCENARIOS: &str = "epr-exercise-scenarios";

// ---------------------------------------------------------------- capabilities

#[derive(Debug, Clone, Deserialize)]
pub struct IdName {
    pub id: String,
    pub name: String,
}

/// One CDC PHEP capability.
#[derive(Debug, Clone, Deserialize)]
pub struct PhepCap {
    pub id: String,
    pub number: u8,
    pub name: String,
    pub domain: String,
    pub tier: u8,
    pub definition: String,
    pub functions: Vec<String>,
    /// FEMA core capability ids (this project's mapping).
    pub fema_core: Vec<String>,
    pub source: String,
    pub section: String,
}

/// One FEMA core capability.
#[derive(Debug, Clone, Deserialize)]
pub struct FemaCap {
    pub id: String,
    pub name: String,
    pub mission_areas: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilitiesContent {
    pub sources: BTreeMap<String, DocSource>,
    pub phep_domains: Vec<IdName>,
    pub phep: Vec<PhepCap>,
    pub mission_areas: Vec<IdName>,
    pub fema_core: Vec<FemaCap>,
    pub mapping_note: String,
}

// ---------------------------------------------------------------- HSEEP doctrine

#[derive(Debug, Clone, Deserialize)]
pub struct CiteRef {
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExerciseType {
    pub id: String,
    pub name: String,
    pub category: String,
    pub definition: String,
    pub purpose: String,
    pub default_hours: f64,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DocumentInfo {
    pub id: String,
    pub name: String,
    pub categories: Vec<String>,
    pub distribution: String,
    pub description: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanningMeeting {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub operations_only: bool,
    pub weeks_before_discussion: Option<u32>,
    pub weeks_before_operations: Option<u32>,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmartGuidance {
    pub letter: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmartObjective {
    pub guidance: Vec<SmartGuidance>,
    pub example: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmartCorrective {
    pub example: String,
    pub guiding_questions: Vec<String>,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Element {
    pub id: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rating {
    pub code: String,
    pub label: String,
    pub definition: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Role {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: content::Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimelineStep {
    pub step: String,
    pub when: String,
    pub source: String,
    pub section: String,
    pub tool_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScalePoint {
    pub value: u8,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Factor {
    pub id: String,
    pub text: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedbackForm {
    pub scale: Vec<ScalePoint>,
    pub factors: Vec<Factor>,
    pub open_questions: Vec<String>,
    pub roles: Vec<String>,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MselColumn {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HseepContent {
    pub sources: BTreeMap<String, DocSource>,
    pub exercise_types: Vec<ExerciseType>,
    pub documents: Vec<DocumentInfo>,
    pub planning_meetings: Vec<PlanningMeeting>,
    pub planning_timeline_note: String,
    pub smart_objective: SmartObjective,
    pub smart_corrective_action: SmartCorrective,
    pub capability_elements: Vec<Element>,
    pub ratings: Vec<Rating>,
    pub participant_roles: Vec<Role>,
    pub exercise_guidelines: Vec<String>,
    pub facilitator_tips: Vec<String>,
    pub evaluation_questions: Vec<String>,
    pub root_cause_questions: Vec<String>,
    pub observation_categories: Vec<Element>,
    pub aar_rules: Vec<Rule>,
    pub after_action_timeline: Vec<TimelineStep>,
    pub feedback_form: FeedbackForm,
    pub msel_columns: Vec<MselColumn>,
    pub hotwash_agenda: Vec<String>,
    pub hotwash_note: String,
}

// ---------------------------------------------------------------- scenarios

#[derive(Debug, Clone, Deserialize)]
pub struct SuggestedObjective {
    pub text: String,
    pub phep: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Fact {
    pub text: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Inject {
    pub time: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Question {
    pub group: String,
    pub text: String,
    pub phep: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Module {
    pub number: u32,
    pub title: String,
    pub timeframe: String,
    pub injects: Vec<Inject>,
    pub key_issues: Vec<String>,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub title: String,
    pub hazard: String,
    pub mission_areas: Vec<String>,
    pub summary: String,
    pub phep: Vec<String>,
    pub functional_groups: Vec<String>,
    pub suggested_objectives: Vec<SuggestedObjective>,
    pub facts: Vec<Fact>,
    pub related_tools: Vec<String>,
    pub modules: Vec<Module>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Seed {
    pub id: String,
    pub title: String,
    pub hazard: String,
    pub summary: String,
    pub phep: Vec<String>,
    #[serde(default)]
    pub fema: Vec<String>,
    pub related_tools: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioContent {
    pub sources: BTreeMap<String, DocSource>,
    pub scenarios: Vec<Scenario>,
    pub seeds: Vec<Seed>,
}

/// The capability frameworks.
pub static CAPS: LazyLock<CapabilitiesContent> = LazyLock::new(|| {
    content::load(CAPABILITIES).expect("content/epr/capabilities.yaml is validated in tests")
});
/// HSEEP doctrine and templates.
pub static DOCTRINE: LazyLock<HseepContent> =
    LazyLock::new(|| content::load(HSEEP).expect("content/epr/hseep.yaml is validated in tests"));
/// Exercise scenarios and seeds.
pub static LIBRARY: LazyLock<ScenarioContent> = LazyLock::new(|| {
    content::load(SCENARIOS).expect("content/epr/exercise_scenarios.yaml is validated in tests")
});

// ---------------------------------------------------------------- capability lookup

/// A capability as returned by the tools.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityRef {
    /// "PHEP" (CDC Public Health Emergency Preparedness and Response) or "FEMA" (core capability).
    pub framework: String,
    /// "phep-13" or a FEMA core capability id such as "public_information_and_warning".
    pub id: String,
    /// Display name, e.g. "PHEP 13: Public Health Surveillance and Epidemiological Investigation".
    pub name: String,
}

/// A resolved capability.
#[derive(Debug, Clone, Copy)]
pub enum Cap<'a> {
    Phep(&'a PhepCap),
    Fema(&'a FemaCap),
}

impl Cap<'_> {
    /// As a tool output reference.
    pub fn to_ref(self) -> CapabilityRef {
        match self {
            Cap::Phep(p) => phep_ref(p),
            Cap::Fema(f) => fema_ref(f),
        }
    }
}

pub fn phep_ref(p: &PhepCap) -> CapabilityRef {
    CapabilityRef {
        framework: "PHEP".into(),
        id: p.id.clone(),
        name: format!("PHEP {}: {}", p.number, p.name),
    }
}

pub fn fema_ref(f: &FemaCap) -> CapabilityRef {
    CapabilityRef {
        framework: "FEMA".into(),
        id: f.id.clone(),
        name: f.name.clone(),
    }
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .replace('&', " and ")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A PHEP capability by id ("phep-13").
pub fn phep(id: &str) -> Option<&'static PhepCap> {
    CAPS.phep.iter().find(|p| p.id == id)
}

/// A FEMA core capability by id.
pub fn fema(id: &str) -> Option<&'static FemaCap> {
    CAPS.fema_core.iter().find(|f| f.id == id)
}

/// Resolve a capability named by the user: a PHEP number or id ("13", "phep-13",
/// "PHEP 13", "Capability 13"), a PHEP name, a FEMA core capability id or name,
/// or an unambiguous part of a name. Exact matches win over partial ones.
pub fn resolve(query: &str) -> std::result::Result<Cap<'static>, String> {
    let q = norm(query);
    if q.is_empty() {
        return Err("an empty capability name".into());
    }
    // PHEP by number.
    let num = q
        .strip_prefix("phep capability ")
        .or_else(|| q.strip_prefix("phep "))
        .or_else(|| q.strip_prefix("capability "))
        .unwrap_or(&q);
    // The display form these tools return ("PHEP 13: Public Health Surveillance and
    // Epidemiological Investigation") is accepted back: after a "PHEP" or "Capability"
    // prefix, a leading number names the capability.
    let lead = if num.len() < q.len() {
        num.split(' ').next().unwrap_or(num)
    } else {
        num
    };
    if let Ok(n) = lead.parse::<u8>() {
        return CAPS
            .phep
            .iter()
            .find(|p| p.number == n)
            .map(Cap::Phep)
            .ok_or_else(|| format!("'{query}': PHEP capabilities are numbered 1 to 15"));
    }
    // Exact names and ids.
    if let Some(p) = CAPS
        .phep
        .iter()
        .find(|p| norm(&p.name) == q || norm(&p.id) == q)
    {
        return Ok(Cap::Phep(p));
    }
    if let Some(f) = CAPS
        .fema_core
        .iter()
        .find(|f| norm(&f.name) == q || norm(&f.id) == q)
    {
        return Ok(Cap::Fema(f));
    }
    // Strip a framework prefix and try again ("PHEP: Mass Care", "FEMA Planning").
    for prefix in [
        "phep ",
        "fema core capability ",
        "fema ",
        "core capability ",
    ] {
        if let Some(rest) = q.strip_prefix(prefix)
            && let Ok(c) = resolve(rest)
        {
            return Ok(c);
        }
    }
    // Partial matches.
    let mut hits: Vec<Cap<'static>> = Vec::new();
    for p in &CAPS.phep {
        if norm(&p.name).contains(&q) {
            hits.push(Cap::Phep(p));
        }
    }
    for f in &CAPS.fema_core {
        if norm(&f.name).contains(&q) {
            hits.push(Cap::Fema(f));
        }
    }
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(format!(
            "'{query}' is not a PHEP or FEMA core capability (use a PHEP number 1-15 or a name)"
        )),
        _ => Err(format!(
            "'{query}' matches several capabilities: {}",
            hits.iter()
                .map(|c| c.to_ref().name)
                .collect::<Vec<_>>()
                .join("; ")
        )),
    }
}

/// FEMA core capabilities mapped from PHEP capabilities (this project's mapping), in order, without duplicates.
pub fn fema_for(pheps: &[&PhepCap]) -> Vec<&'static FemaCap> {
    let mut out: Vec<&'static FemaCap> = Vec::new();
    for p in pheps {
        for id in &p.fema_core {
            if let Some(f) = fema(id)
                && !out.iter().any(|o| o.id == f.id)
            {
                out.push(f);
            }
        }
    }
    out
}

/// PHEP capabilities whose mapping includes a FEMA core capability.
pub fn phep_for(f: &FemaCap) -> Vec<&'static PhepCap> {
    CAPS.phep
        .iter()
        .filter(|p| p.fema_core.iter().any(|id| id == &f.id))
        .collect()
}

/// Keyword cues used to suggest PHEP capabilities when none are given.
const CUES: &[(&str, &[&str])] = &[
    (
        "phep-1",
        &[
            "at-risk",
            "vulnerable",
            "access and functional",
            "community partner",
            "outreach",
            "wellness check",
        ],
    ),
    (
        "phep-2",
        &[
            "recover",
            "restore",
            "restoring",
            "restoration",
            "return to normal",
        ],
    ),
    (
        "phep-3",
        &[
            "incident command",
            "ics",
            "activate",
            "activation",
            "emergency operations",
            "eoc",
            "unified command",
            "demobiliz",
        ],
    ),
    (
        "phep-4",
        &[
            "public information",
            "message",
            "messaging",
            "alert",
            "warning",
            "media",
            "press",
            "spokesperson",
            "notify the public",
            "public notification",
        ],
    ),
    (
        "phep-5",
        &["fatalit", "death", "morgue", "remains", "medical examiner"],
    ),
    (
        "phep-6",
        &[
            "information sharing",
            "share information",
            "situational awareness",
            "common operating picture",
            "health alert",
            "notify partners",
        ],
    ),
    (
        "phep-7",
        &[
            "shelter",
            "mass care",
            "cooling center",
            "warming center",
            "congregate",
        ],
    ),
    (
        "phep-8",
        &[
            "dispens",
            "pod",
            "prophylaxis",
            "vaccinat",
            "countermeasure",
            "clinic",
            "mmr",
            "antibiotic",
            "antiviral",
        ],
    ),
    (
        "phep-9",
        &[
            "materiel",
            "inventory",
            "supply",
            "supplies",
            "cold chain",
            "stockpile",
            "sns",
        ],
    ),
    (
        "phep-10",
        &[
            "surge",
            "hospital",
            "emergency department",
            "ems",
            "dialysis",
        ],
    ),
    (
        "phep-11",
        &[
            "isolation",
            "quarantine",
            "exclusion",
            "exclude",
            "school closure",
            "closure",
            "social distanc",
            "boil water",
            "do-not-drink",
            "decontamination",
        ],
    ),
    (
        "phep-12",
        &["laboratory", "lab test", "specimen", "testing"],
    ),
    (
        "phep-13",
        &[
            "surveillance",
            "investigation",
            "contact tracing",
            "contacts",
            "case",
            "epidemiolog",
            "monitor",
        ],
    ),
    (
        "phep-14",
        &[
            "responder safety",
            "staff safety",
            "ppe",
            "protective equipment",
            "responder health",
        ],
    ),
    ("phep-15", &["volunteer", "mrc", "medical reserve corps"]),
];

/// PHEP capabilities suggested by keywords in an objective (to confirm with the user).
pub fn infer_phep(text: &str) -> Vec<&'static PhepCap> {
    let t = format!(" {} ", text.to_lowercase());
    let mut out = Vec::new();
    for (id, cues) in CUES {
        let hit = cues.iter().any(|c| {
            // Cues match at the start of a word, never inside one ("media" is not found in
            // "immediately", "closure" in "disclosure", or "press" in "express").
            let pattern = if c.len() <= 4 {
                // Short cues must be whole words ("ics", "pod", "ems", "eoc").
                format!(r"\b{}\b", regex::escape(c))
            } else if c.len() <= 6 {
                // Short words with their endings only ("alerts", "clinics"), so that
                // "press" is not found in "pressure" or "clinic" in "clinical".
                format!(r"\b{}(s|es|d|ed|ing)?\b", regex::escape(c))
            } else {
                // Longer cues and stems ("vaccinat", "dispens") begin a word.
                format!(r"\b{}", regex::escape(c))
            };
            Regex::new(&pattern)
                .map(|re| re.is_match(&t))
                .unwrap_or(false)
        });
        if hit && let Some(p) = phep(id) {
            out.push(p);
        }
    }
    out
}

/// The mission areas (Prevention ... Recovery) covered by FEMA capabilities, in doctrine order.
pub fn mission_areas(femas: &[&FemaCap]) -> Vec<String> {
    CAPS.mission_areas
        .iter()
        .filter(|m| femas.iter().any(|f| f.mission_areas.contains(&m.id)))
        .map(|m| m.name.clone())
        .collect()
}

/// Capabilities resolved for one objective, with notes on what was added.
#[derive(Debug, Clone, Default)]
pub struct Mapped {
    pub phep: Vec<&'static PhepCap>,
    pub fema: Vec<&'static FemaCap>,
    /// Notes for the user (added from mapping, inferred, unrecognized names).
    pub notes: Vec<String>,
    /// Capability names that could not be resolved.
    pub unresolved: Vec<String>,
}

/// Resolve the capabilities given for an objective, completing PHEP ↔ FEMA:
/// FEMA capabilities are added from the PHEP mapping, a PHEP capability is
/// added when exactly one maps to a given FEMA capability, and when nothing is
/// given, PHEP capabilities are inferred from the objective's words.
pub fn map_objective(text: &str, given: &[String]) -> Mapped {
    let mut m = Mapped::default();
    for g in given.iter().map(|g| g.trim()).filter(|g| !g.is_empty()) {
        match resolve(g) {
            Ok(Cap::Phep(p)) => {
                if !m.phep.iter().any(|x| x.id == p.id) {
                    m.phep.push(p);
                }
            }
            Ok(Cap::Fema(f)) => {
                if !m.fema.iter().any(|x| x.id == f.id) {
                    m.fema.push(f);
                }
            }
            Err(e) => {
                m.unresolved.push(g.to_string());
                m.notes.push(e);
            }
        }
    }
    if m.phep.is_empty() && m.fema.is_empty() {
        let inferred = infer_phep(text);
        if !inferred.is_empty() {
            m.notes.push(format!(
                "PHEP capabilities suggested from the objective's wording (confirm): {}",
                inferred
                    .iter()
                    .map(|p| format!("{} {}", p.number, p.name))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            m.phep = inferred;
        }
    }
    if m.phep.is_empty() {
        for f in m.fema.clone() {
            let cands = phep_for(f);
            if cands.len() == 1 {
                m.notes.push(format!(
                    "Added PHEP {} {} (the only PHEP capability mapped to {})",
                    cands[0].number, cands[0].name, f.name
                ));
                m.phep.push(cands[0]);
            }
        }
    }
    if m.fema.is_empty() && !m.phep.is_empty() {
        m.fema = fema_for(&m.phep);
        if !m.fema.is_empty() {
            m.notes.push(format!(
                "FEMA core capabilities added from this project's PHEP mapping: {}",
                m.fema
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    m
}

// ---------------------------------------------------------------- SMART checks

/// A heuristic SMART check of an objective or corrective action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SmartCheck {
    /// Names who does what (an action verb and enough detail).
    pub specific: bool,
    /// Has a number, standard, or "all/each" that an evaluator can observe.
    pub measurable: bool,
    /// Only the planning team can judge achievability; always null.
    pub achievable: Option<bool>,
    /// Ties to a plan, policy, procedure, or guidance.
    pub relevant: bool,
    /// Gives a time frame or due date.
    pub time_bound: bool,
    /// What to add.
    pub suggestions: Vec<String>,
}

impl SmartCheck {
    /// True when every machine-checkable letter passed.
    pub fn passed(&self) -> bool {
        self.specific && self.measurable && self.relevant && self.time_bound
    }

    /// Short label such as "S M R T" or "S ? R -".
    pub fn label(&self) -> String {
        let f = |b: bool, c: &str| if b { c.to_string() } else { "-".into() };
        format!(
            "{}{}?{}{}",
            f(self.specific, "S"),
            f(self.measurable, "M"),
            f(self.relevant, "R"),
            f(self.time_bound, "T")
        )
    }
}

const VERBS: &[&str] = &[
    "demonstrate",
    "discuss",
    "identify",
    "assess",
    "validate",
    "coordinate",
    "activate",
    "develop",
    "notify",
    "issue",
    "establish",
    "implement",
    "open",
    "dispense",
    "conduct",
    "deliver",
    "review",
    "evaluate",
    "determine",
    "communicate",
    "test",
    "describe",
    "examine",
    "track",
    "staff",
    "deploy",
    "provide",
    "share",
    "release",
    "draft",
    "reach",
    "contact",
    "screen",
    "inspect",
    "monitor",
    "explore",
    "exercise",
    "revise",
    "update",
    "train",
    "hold",
    "write",
    "create",
    "purchase",
    "procure",
    "acquire",
    "complete",
    "schedule",
    "adopt",
    "designate",
    "assign",
    "install",
    "replace",
    "recruit",
    "sign",
    "add",
    "build",
    "set up",
    "stand up",
    "check",
    "verify",
    "confirm",
    "publish",
    "distribute",
    "prepare",
    "brief",
];

static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\d|%|\b(one|two|three|four|five|six|seven|eight|nine|ten|twelve|all|each|every|both|first|second|third)\b")
        .expect("valid regex")
});
static TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(within|by|before|after|during|until|no later than|hours?|minutes?|days?|weeks?|months?|operational periods?|quarter|annually|monthly|weekly|deadline)\b|\d{4}-\d{2}-\d{2}|\b(january|february|march|april|june|july|august|september|october|november|december)\b")
        .expect("valid regex")
});
static RELEVANT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(in accordance with|according to|consistent with|per|plans?|polic(y|ies)|procedures?|protocols?|annex|sops?|guidance|guidelines?|standards?|agreements?|regulations?|cmr|m\.g\.l\.|framework|toolkit|checklist)\b")
        .expect("valid regex")
});

/// Check an exercise objective against HSEEP's SMART guidance (word-pattern
/// heuristics; the planning team makes the final call).
pub fn smart_check(text: &str) -> SmartCheck {
    let lower = text.to_lowercase();
    let words = text.split_whitespace().count();
    let has_verb = VERBS.iter().any(|v| {
        Regex::new(&format!(r"\b{}", regex::escape(v)))
            .map(|re| re.is_match(&lower))
            .unwrap_or(false)
    });
    let specific = has_verb && words >= 8;
    let measurable = NUMBER.is_match(text);
    let time_bound = TIME.is_match(text);
    let relevant = RELEVANT.is_match(text);
    let mut suggestions = Vec::new();
    if !specific {
        suggestions.push(
            "Specific: say who will do what, starting with an action verb (demonstrate, discuss, notify, activate)."
                .into(),
        );
    }
    if !measurable {
        suggestions.push(
            "Measurable: add a quantity or standard an evaluator can observe (for example, all exposed contacts, 3 languages, 100% of shelter residents)."
                .into(),
        );
    }
    if !relevant {
        suggestions.push(
            "Relevant: name the plan, policy, procedure, or guidance being tested (\"in accordance with the agency's ... plan\")."
                .into(),
        );
    }
    if !time_bound {
        suggestions.push(
            "Time-bound: add a time frame (for example, within 2 hours of notification, during the first operational period)."
                .into(),
        );
    }
    SmartCheck {
        specific,
        measurable,
        achievable: None,
        relevant,
        time_bound,
        suggestions,
    }
}

/// Replace `{{town}}` and `{{agency}}` in scenario text.
pub fn fill(text: &str, town: &str, agency: &str) -> String {
    text.replace("{{town}}", town).replace("{{agency}}", agency)
}

/// Normalize a capability element name to its POETE id.
pub fn element(s: &str) -> Option<&'static Element> {
    let n = norm(s);
    DOCTRINE
        .capability_elements
        .iter()
        .find(|e| {
            n == e.id
                || n == norm(&e.name)
                // A single letter counts only when one element starts with it: "E" could be
                // Equipment or Exercise, so it is not guessed.
                || (n.len() == 1
                    && e.id.starts_with(&n)
                    && DOCTRINE
                        .capability_elements
                        .iter()
                        .filter(|x| x.id.starts_with(&n))
                        .count()
                        == 1)
        })
        .or_else(|| match n.as_str() {
            "plan" | "plans" | "policy" | "procedure" => DOCTRINE
                .capability_elements
                .iter()
                .find(|e| e.id == "planning"),
            "org" | "organisation" | "staffing" => DOCTRINE
                .capability_elements
                .iter()
                .find(|e| e.id == "organization"),
            "equip" | "supplies" => DOCTRINE
                .capability_elements
                .iter()
                .find(|e| e.id == "equipment"),
            "train" => DOCTRINE
                .capability_elements
                .iter()
                .find(|e| e.id == "training"),
            "exercises" | "drill" => DOCTRINE
                .capability_elements
                .iter()
                .find(|e| e.id == "exercise"),
            _ => None,
        })
}

// ---------------------------------------------------------------- citations

/// Cite sources from the HSEEP content files by key.
pub fn cite(keys: &[(&str, &str)]) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    for (file, key) in keys {
        let map = match *file {
            CAPABILITIES => &CAPS.sources,
            HSEEP => &DOCTRINE.sources,
            _ => &LIBRARY.sources,
        };
        if let Some(s) = map.get(*key) {
            let src = s.to_source();
            if !out.iter().any(|o| o.title == src.title) {
                out.push(src);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- document helpers

/// A plain paragraph.
pub fn para(text: impl Into<String>) -> Block {
    Block::Paragraph(vec![Inline::plain(text)])
}

/// A paragraph with a bold label: "Label: text".
pub fn labeled(label: &str, text: impl Into<String>) -> Block {
    Block::Paragraph(vec![
        Inline {
            text: format!("{label}: "),
            bold: true,
            ..Inline::default()
        },
        Inline::plain(text),
    ])
}

pub fn heading(level: u8, text: impl Into<String>) -> Block {
    Block::Heading {
        level,
        text: text.into(),
    }
}

pub fn bullets(items: impl IntoIterator<Item = String>) -> Block {
    Block::List {
        ordered: false,
        items: items.into_iter().map(|t| vec![Inline::plain(t)]).collect(),
    }
}

pub fn numbered(items: impl IntoIterator<Item = String>) -> Block {
    Block::List {
        ordered: true,
        items: items.into_iter().map(|t| vec![Inline::plain(t)]).collect(),
    }
}

pub fn table(caption: &str, headers: &[&str], rows: Vec<Vec<String>>) -> Block {
    Block::Table(TableBlock {
        caption: Some(caption.to_string()),
        headers: headers.iter().map(|h| h.to_string()).collect(),
        rows,
    })
}

/// "phep-13, phep-4" style list of capability display names.
pub fn names(refs: &[CapabilityRef]) -> String {
    refs.iter()
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_loads_and_is_consistent() {
        assert_eq!(CAPS.phep.len(), 15);
        assert_eq!(CAPS.fema_core.len(), 32);
        for p in &CAPS.phep {
            assert!(
                CAPS.phep_domains.iter().any(|d| d.id == p.domain),
                "{}",
                p.id
            );
            for f in &p.fema_core {
                assert!(fema(f).is_some(), "{} maps to unknown {f}", p.id);
            }
            assert_eq!(p.id, format!("phep-{}", p.number));
            assert!(CAPS.sources.contains_key(&p.source));
        }
        for f in &CAPS.fema_core {
            for m in &f.mission_areas {
                assert!(CAPS.mission_areas.iter().any(|a| &a.id == m));
            }
        }
        assert_eq!(DOCTRINE.exercise_types.len(), 7);
        assert_eq!(DOCTRINE.ratings.len(), 4);
        assert_eq!(DOCTRINE.capability_elements.len(), 5);
        for r in &DOCTRINE.aar_rules {
            assert!(DOCTRINE.sources.contains_key(&r.source), "{}", r.id);
        }
        for s in &LIBRARY.scenarios {
            for id in s
                .phep
                .iter()
                .chain(s.suggested_objectives.iter().flat_map(|o| o.phep.iter()))
                .chain(
                    s.modules
                        .iter()
                        .flat_map(|m| m.questions.iter().flat_map(|q| q.phep.iter())),
                )
            {
                assert!(phep(id).is_some(), "{}: {id}", s.id);
            }
            for q in s.modules.iter().flat_map(|m| m.questions.iter()) {
                assert!(
                    s.functional_groups.contains(&q.group),
                    "{}: group {}",
                    s.id,
                    q.group
                );
            }
            for f in &s.facts {
                assert!(LIBRARY.sources.contains_key(&f.source), "{}", s.id);
            }
            for o in &s.suggested_objectives {
                let c = smart_check(&o.text);
                assert!(c.passed(), "{}: {} {:?}", s.id, o.text, c.suggestions);
            }
        }
        for s in &LIBRARY.seeds {
            for id in &s.phep {
                assert!(phep(id).is_some(), "{}: {id}", s.id);
            }
            for id in &s.fema {
                assert!(fema(id).is_some(), "{}: {id}", s.id);
            }
        }
    }

    #[test]
    fn resolves_capabilities_by_number_name_and_part() {
        let id = |q: &str| resolve(q).map(|c| c.to_ref().id).unwrap();
        assert_eq!(id("13"), "phep-13");
        assert_eq!(id("PHEP 4"), "phep-4");
        assert_eq!(id("phep-8"), "phep-8");
        assert_eq!(id("Capability 15"), "phep-15");
        assert_eq!(id("Mass Care"), "phep-7");
        assert_eq!(id("Mass Care Services"), "mass_care_services");
        assert_eq!(
            id("Public Information and Warning"),
            "public_information_and_warning"
        );
        assert_eq!(
            id("public_health_healthcare_and_ems"),
            "public_health_healthcare_and_ems"
        );
        assert_eq!(id("volunteer"), "phep-15");
        assert_eq!(id("FEMA: Planning"), "planning");
        assert!(resolve("16").is_err());
        assert!(resolve("information").unwrap_err().contains("several"));
        assert!(resolve("teleportation").is_err());
        // The display names the tools return resolve when passed back in.
        for p in &CAPS.phep {
            assert_eq!(id(&phep_ref(p).name), p.id, "{}", phep_ref(p).name);
        }
        for f in &CAPS.fema_core {
            assert_eq!(id(&fema_ref(f).name), f.id, "{}", f.name);
        }
        assert_eq!(
            id("PHEP 13: Public Health Surveillance and Epidemiological Investigation"),
            "phep-13"
        );
        assert!(resolve("PHEP 16: Something Else").is_err());
    }

    #[test]
    fn cues_match_words_not_parts_of_words() {
        let ids = |t: &str| -> Vec<String> { infer_phep(t).iter().map(|p| p.id.clone()).collect() };
        // "immediately" holds "media", "disclosure" holds "closure", "clinical" holds
        // "clinic", and "pressure" and "express" hold "press".
        let got = ids("Immediately report suspect cases to the state health department.");
        assert!(!got.contains(&"phep-4".to_string()), "{got:?}");
        let got = ids("Review the clinical disclosure form under pressure and express thanks.");
        assert!(
            !got.contains(&"phep-4".to_string())
                && !got.contains(&"phep-8".to_string())
                && !got.contains(&"phep-11".to_string()),
            "{got:?}"
        );
        // Whole words, plurals, and stems still match.
        let got = ids("Send alerts to the media and restore services after school closures.");
        assert!(
            got.contains(&"phep-4".to_string())
                && got.contains(&"phep-2".to_string())
                && got.contains(&"phep-11".to_string()),
            "{got:?}"
        );
        let got = ids("Open vaccination clinics and dispensing sites.");
        assert!(got.contains(&"phep-8".to_string()), "{got:?}");
    }

    #[test]
    fn maps_and_infers_capabilities() {
        let m = map_objective("x", &["13".into()]);
        assert_eq!(m.phep[0].id, "phep-13");
        assert!(m.fema.iter().any(|f| f.id == "situational_assessment"));
        let m = map_objective("x", &["Fatality Management Services".into()]);
        assert_eq!(m.phep[0].id, "phep-5");
        let m = map_objective(
            "Discuss how the health department will open a POD and use MRC volunteers",
            &[],
        );
        let ids: Vec<&str> = m.phep.iter().map(|p| p.id.as_str()).collect();
        assert!(
            ids.contains(&"phep-8") && ids.contains(&"phep-15"),
            "{ids:?}"
        );
        assert!(!m.fema.is_empty());
        let m = map_objective("x", &["nonsense capability".into()]);
        assert_eq!(m.unresolved.len(), 1);
    }

    #[test]
    fn smart_check_flags_missing_parts() {
        let c = smart_check(&DOCTRINE.smart_objective.example);
        assert!(c.passed(), "{c:?}");
        let c = smart_check("Improve communication.");
        assert!(!c.specific && !c.measurable && !c.time_bound && !c.relevant);
        assert_eq!(c.suggestions.len(), 4);
        assert_eq!(c.label(), "--?--");
        assert!(element("P").is_some_and(|e| e.id == "planning"));
        assert!(element("Training").is_some_and(|e| e.id == "training"));
        assert!(element("budget").is_none());
        // "E" alone could be Equipment or Exercise, so it is not guessed.
        assert!(element("E").is_none());
        assert!(element("T").is_some_and(|e| e.id == "training"));
        assert!(element("Equipment").is_some_and(|e| e.id == "equipment"));
    }
}
