//! EPR-08 `design_hseep_exercise`: designs an HSEEP-consistent exercise
//! (seminar, workshop, tabletop, game, drill, functional, or full-scale) for a
//! Massachusetts local health department: SMART objectives mapped to the CDC
//! PHEP capabilities and FEMA core capabilities, a scenario (three fully
//! developed tabletop scenarios for Pilot 3, six seeds, or the nurse's own),
//! modules with injects and discussion questions, a schedule, and a planning
//! timeline. Writes the Situation Manual and Facilitator Guide (discussion
//! exercises) or Exercise Plan and MSEL (operations exercises), Exercise
//! Evaluation Guides, a Participant Feedback Form, and a sign-in sheet to
//! /data/output. Staff contact details come only from a local roster file
//! (ADR 0009) and appear only in those files.

use std::collections::BTreeSet;

use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::area;
use super::hseep_content::{
    self as hc, CAPABILITIES, CapabilityRef, DOCTRINE, ExerciseType, FemaCap, HSEEP, LIBRARY,
    PhepCap, SCENARIOS, SmartCheck, bullets, heading, labeled, numbered, para, table,
};
use super::roster::{self, Roster, RosterSummary, RosterTally};
use crate::context::Context;
use crate::md;
use crate::school::templates::{DocFormat, output_folder, write_in};

/// An exercise objective.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ObjectiveInput {
    /// The objective, written SMART: who does what, how well, by when, under which plan
    /// (e.g. "Within 24 hours of notification, discuss how ... in accordance with ...").
    pub text: String,
    /// Capabilities it tests: CDC PHEP numbers or names ("13", "PHEP 8", "Mass Care") and/or
    /// FEMA core capabilities ("Public Information and Warning"). FEMA capabilities are added
    /// from this project's PHEP mapping when only PHEP ones are given; when none are given,
    /// PHEP capabilities are suggested from the wording.
    pub capabilities: Vec<String>,
}

/// A person or position taking part. Names may be given; phone numbers and emails come
/// only from `roster_path`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ParticipantInput {
    /// Position or title, e.g. "Health Director", "School Nurse Leader", "Lead Evaluator".
    pub role: String,
    /// Name (optional).
    pub name: Option<String>,
    pub organization: Option<String>,
    /// Exercise role: "player" (default), "facilitator", "evaluator", "controller",
    /// "simulator", "observer", or "planner".
    pub exercise_role: Option<String>,
}

/// Arguments for `design_hseep_exercise`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ExerciseArgs {
    /// "tabletop" (default), "seminar", "workshop", "game", "drill", "functional", or "full_scale".
    pub exercise_type: Option<String>,
    /// Scenario: "measles_outbreak", "coastal_storm_power_outage", or "extreme_heat" (fully
    /// developed), or a seed: "mass_dispensing_anthrax", "pandemic_influenza", "cyberattack",
    /// "water_contamination", "mass_casualty", "hazmat_release". Words such as "measles",
    /// "hurricane", or "heat" also work.
    pub scenario: Option<String>,
    /// Your own scenario in a few sentences (fictional; no real people). Used instead of a
    /// library scenario; modules get placeholders for your injects.
    pub custom_scenario: Option<String>,
    /// Exercise name (default built from the town, scenario, and type).
    pub exercise_name: Option<String>,
    /// Town or county (default: your saved town).
    pub area: Option<String>,
    /// Sponsoring agency (default: your letterhead agency, or "<town> Board of Health").
    pub agency: Option<String>,
    /// Sponsor or grant program line for the overview (e.g. "CDC PHEP cooperative agreement").
    pub sponsor: Option<String>,
    /// Exercise date (YYYY-MM-DD); used for the schedule and planning timeline.
    pub date: Option<String>,
    /// Start time (HH:MM, 24-hour; default 09:00).
    pub start_time: Option<String>,
    /// Length in hours (default by type: tabletop 3.5).
    pub duration_hours: Option<f64>,
    /// Venue.
    pub location: Option<String>,
    /// What exercise play is limited to (scope), e.g. "the first 72 hours of the response".
    pub scope: Option<String>,
    /// Objectives (default: the scenario's suggested SMART objectives).
    pub objectives: Vec<ObjectiveInput>,
    /// Extra capabilities to exercise beyond those of the objectives.
    pub capabilities: Vec<String>,
    /// Participating organizations (e.g. "DPH Region 4AB", "Medical Reserve Corps").
    pub organizations: Vec<String>,
    /// Participants by position (names optional).
    pub participants: Vec<ParticipantInput>,
    /// Exercise point of contact (exercise director); contact details come from the roster.
    pub point_of_contact: Option<ParticipantInput>,
    /// A staff roster CSV/XLSX in /data (name, position, phone, email columns). Contact details
    /// are filled into the documents in /data/output only and never returned.
    pub roster_path: Option<String>,
    /// Sheet name when the roster is a spreadsheet.
    pub roster_sheet: Option<String>,
    /// Documents to write to /data/output: "docx" (default), "pdf", "both", or "none".
    pub documents: Option<String>,
}

/// An objective with its capabilities and SMART check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectiveOut {
    pub number: usize,
    pub text: String,
    pub capabilities: Vec<CapabilityRef>,
    pub smart: SmartCheck,
    /// True when the objective is the scenario's suggestion (not yours).
    pub suggested: bool,
    pub notes: Vec<String>,
}

/// An inject (scenario update).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InjectOut {
    /// Scenario time, e.g. "Day 1, 4:30 p.m.".
    pub time: String,
    pub text: String,
}

/// A discussion question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOut {
    pub group: String,
    pub text: String,
    /// PHEP capabilities the question collects evidence for, e.g. "PHEP 13".
    pub capabilities: Vec<String>,
}

/// A module of the exercise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModuleOut {
    pub number: u32,
    pub title: String,
    pub timeframe: String,
    pub injects: Vec<InjectOut>,
    pub key_issues: Vec<String>,
    pub questions: Vec<QuestionOut>,
}

/// One line of the exercise-day schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleItem {
    pub time: String,
    pub activity: String,
    pub minutes: u32,
}

/// A planning or after-action milestone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Milestone {
    pub milestone: String,
    /// "8 weeks before", "Exercise day", "Within 30 days after".
    pub when: String,
    /// Calendar date when the exercise date is known.
    pub date: Option<String>,
    pub purpose: String,
    /// True when the timing is a tool default rather than HSEEP doctrine.
    pub tool_default: bool,
}

/// The scenario used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenarioOut {
    /// "developed", "seed", "custom", or "none".
    pub kind: String,
    pub id: Option<String>,
    pub title: String,
    pub hazard: String,
    pub summary: String,
    /// Cited facts the scenario relies on.
    pub facts: Vec<String>,
}

/// A scenario in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenarioOption {
    pub id: String,
    pub title: String,
    /// True for fully developed scenarios (modules, injects, questions).
    pub developed: bool,
}

/// Counts of participants by exercise role (names are not returned).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ParticipantCounts {
    pub players: usize,
    pub facilitators: usize,
    pub evaluators: usize,
    pub controllers: usize,
    pub observers: usize,
    pub other: usize,
    pub organizations: usize,
}

/// A document of the exercise package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentOut {
    pub name: String,
    pub purpose: String,
    pub distribution: String,
    /// Files written (relative to /data), if any.
    pub files: Vec<String>,
}

/// Result of `design_hseep_exercise`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExerciseResult {
    pub exercise_name: String,
    pub exercise_type: String,
    /// "discussion" or "operations".
    pub category: String,
    pub type_definition: String,
    pub area: String,
    pub agency: String,
    pub date: Option<String>,
    pub location: Option<String>,
    pub duration_hours: f64,
    pub scenario: ScenarioOut,
    pub objectives: Vec<ObjectiveOut>,
    /// All capabilities exercised (PHEP and FEMA).
    pub capabilities: Vec<CapabilityRef>,
    /// National Preparedness Goal mission areas (focus areas).
    pub mission_areas: Vec<String>,
    pub functional_groups: Vec<String>,
    pub modules: Vec<ModuleOut>,
    pub schedule: Vec<ScheduleItem>,
    pub planning_timeline: Vec<Milestone>,
    pub documents: Vec<DocumentOut>,
    pub participants: ParticipantCounts,
    pub roster: Option<RosterSummary>,
    pub available_scenarios: Vec<ScenarioOption>,
    /// Other tools that support this scenario.
    pub related_tools: Vec<String>,
    pub warnings: Vec<String>,
    pub output_folder: Option<String>,
}

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn exercise_type(q: Option<&str>) -> Result<&'static ExerciseType> {
    let id = match q
        .map(|s| {
            s.trim()
                .to_lowercase()
                .replace(['-', ' '], "_")
                .replace("exercise", "")
                .trim_matches('_')
                .to_string()
        })
        .as_deref()
    {
        None | Some("") | Some("tabletop") | Some("ttx") | Some("table_top") => "tabletop",
        Some("seminar") => "seminar",
        Some("workshop") => "workshop",
        Some("game") => "game",
        Some("drill") => "drill",
        Some("functional") | Some("fe") => "functional",
        Some("full_scale") | Some("fullscale") | Some("fse") | Some("full") => "full_scale",
        Some(other) => {
            return Err(PhnError::InvalidInput(format!(
                "exercise_type '{other}' is not an HSEEP exercise type; use seminar, workshop, \
                 tabletop, game, drill, functional, or full_scale"
            )));
        }
    };
    DOCTRINE
        .exercise_types
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| PhnError::Internal(format!("exercise type {id} missing from content")))
}

/// What the scenario argument selected.
enum Selected {
    Developed(&'static hc::Scenario),
    Seed(&'static hc::Seed),
}

fn select_scenario(q: &str) -> Option<Selected> {
    let n = q.trim().to_lowercase().replace(['-', ' '], "_");
    if let Some(s) = LIBRARY.scenarios.iter().find(|s| s.id == n) {
        return Some(Selected::Developed(s));
    }
    if let Some(s) = LIBRARY.seeds.iter().find(|s| s.id == n) {
        return Some(Selected::Seed(s));
    }
    let t = q.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    let dev = |id: &str| {
        LIBRARY
            .scenarios
            .iter()
            .find(|s| s.id == id)
            .map(Selected::Developed)
    };
    let seed = |id: &str| {
        LIBRARY
            .seeds
            .iter()
            .find(|s| s.id == id)
            .map(Selected::Seed)
    };
    if has(&["measles", "rubeola"]) {
        dev("measles_outbreak")
    } else if has(&[
        "storm",
        "hurricane",
        "nor'easter",
        "noreaster",
        "flood",
        "power outage",
        "blackout",
    ]) {
        dev("coastal_storm_power_outage")
    } else if has(&["heat"]) {
        dev("extreme_heat")
    } else if has(&["anthrax", "pod", "dispens"]) {
        seed("mass_dispensing_anthrax")
    } else if has(&["pandemic", "influenza", "flu"]) {
        seed("pandemic_influenza")
    } else if has(&["cyber", "ransomware"]) {
        seed("cyberattack")
    } else if has(&["water", "boil"]) {
        seed("water_contamination")
    } else if has(&["casualty", "mci", "shooting", "collapse"]) {
        seed("mass_casualty")
    } else if has(&["hazmat", "chemical", "chlorine", "spill"]) {
        seed("hazmat_release")
    } else {
        None
    }
}

/// Scenario output, default PHEP ids, suggested objectives, functional groups,
/// related tools, and the library id.
type ScenarioParts = (
    ScenarioOut,
    Vec<String>,
    Vec<hc::SuggestedObjective>,
    Vec<String>,
    Vec<String>,
    Option<String>,
);

fn short_title(id: &str) -> &'static str {
    match id {
        "measles_outbreak" => "Measles Response",
        "coastal_storm_power_outage" => "Coastal Storm and Power Outage",
        "extreme_heat" => "Extreme Heat",
        "mass_dispensing_anthrax" => "Mass Dispensing",
        "pandemic_influenza" => "Pandemic Influenza",
        "cyberattack" => "Cyber Disruption",
        "water_contamination" => "Drinking Water Emergency",
        "mass_casualty" => "Mass Casualty",
        "hazmat_release" => "Hazardous Materials",
        _ => "Public Health Emergency",
    }
}

fn type_label(t: &ExerciseType) -> String {
    match t.id.as_str() {
        "tabletop" => "Tabletop Exercise".into(),
        "functional" => "Functional Exercise".into(),
        "full_scale" => "Full-Scale Exercise".into(),
        _ => t.name.clone(),
    }
}

fn clock(t: NaiveTime) -> String {
    let s = t.format("%-I:%M %p").to_string();
    s.replace("AM", "a.m.").replace("PM", "p.m.")
}

fn exercise_role(p: &ParticipantInput) -> &'static str {
    match p
        .exercise_role
        .as_deref()
        .map(|r| r.trim().to_lowercase())
        .as_deref()
    {
        Some(r) if r.starts_with("facilitat") => "Facilitator",
        Some(r) if r.contains("evaluat") || r.contains("note") => "Evaluator",
        Some(r) if r.starts_with("control") || r.starts_with("simulat") => "Controller",
        Some(r) if r.starts_with("observ") => "Observer",
        Some(r) if r.starts_with("plan") => "Planner",
        _ => {
            let role = p.role.to_lowercase();
            if role.contains("evaluator") {
                "Evaluator"
            } else if role.contains("facilitator") {
                "Facilitator"
            } else if role.contains("controller") {
                "Controller"
            } else {
                "Player"
            }
        }
    }
}

fn phep_tags(ids: &[String]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| hc::phep(id))
        .map(|p| format!("PHEP {}", p.number))
        .collect()
}

/// Generic discussion questions drawn from capability functions (seed and custom scenarios).
fn generic_modules(pheps: &[&PhepCap], agency: &str) -> Vec<ModuleOut> {
    let phases = [
        ("Initial response", "First 24 hours", 0usize..2usize),
        ("Sustained response", "Days 2-7", 2..4),
        ("Recovery and demobilization", "Week 2 onward", 4..6),
    ];
    phases
        .iter()
        .enumerate()
        .map(|(i, (title, timeframe, range))| {
            let mut questions = Vec::new();
            for p in pheps {
                let fs: Vec<&String> = p
                    .functions
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| range.contains(k) || (i == 2 && *k + 1 == p.functions.len()))
                    .map(|(_, f)| f)
                    .collect();
                for f in fs {
                    let mut lower = f.clone();
                    if let Some(c) = lower.get(0..1) {
                        let c = c.to_lowercase();
                        lower.replace_range(0..1, &c);
                    }
                    questions.push(QuestionOut {
                        group: "All participants".into(),
                        text: format!("How would the {agency} {lower}? Who is responsible, and which plan or procedure applies?"),
                        capabilities: vec![format!("PHEP {}", p.number)],
                    });
                }
            }
            ModuleOut {
                number: i as u32 + 1,
                title: title.to_string(),
                timeframe: timeframe.to_string(),
                injects: vec![InjectOut {
                    time: "[Scenario time]".into(),
                    text: "[Write the scenario update for this module: what has happened, what players know, and what decision is needed.]".into(),
                }],
                key_issues: vec!["[Summarize the key issues raised by this module.]".into()],
                questions,
            }
        })
        .collect()
}

fn add_caps(
    m: &hc::Mapped,
    all_phep: &mut Vec<&'static PhepCap>,
    all_fema: &mut Vec<&'static FemaCap>,
) {
    for p in &m.phep {
        if !all_phep.iter().any(|x| x.id == p.id) {
            all_phep.push(p);
        }
    }
    for f in &m.fema {
        if !all_fema.iter().any(|x| x.id == f.id) {
            all_fema.push(f);
        }
    }
}

fn push_objective(
    text: String,
    m: &hc::Mapped,
    suggested: bool,
    objectives: &mut Vec<ObjectiveOut>,
) {
    let mut caps: Vec<CapabilityRef> = m.phep.iter().map(|p| hc::phep_ref(p)).collect();
    caps.extend(m.fema.iter().map(|f| hc::fema_ref(f)));
    let mut notes = m.notes.clone();
    if m.phep.is_empty() {
        notes.push("No PHEP capability: add one (number 1-15 or name).".into());
    }
    if m.fema.is_empty() {
        notes.push("No FEMA core capability: add one.".into());
    }
    objectives.push(ObjectiveOut {
        number: objectives.len() + 1,
        smart: hc::smart_check(&text),
        text,
        capabilities: caps,
        suggested,
        notes,
    });
}

fn push_item(label: String, mins: i64, schedule: &mut Vec<ScheduleItem>, t: &mut NaiveTime) {
    schedule.push(ScheduleItem {
        time: clock(*t),
        activity: label,
        minutes: mins.max(0) as u32,
    });
    *t += Duration::minutes(mins.max(0));
}

/// MSEL event numbers for injects listed module by module, counting within each
/// module: "1-1", "1-2", "2-1".
fn msel_event_ids(modules: impl IntoIterator<Item = u32>) -> Vec<String> {
    let mut last: Option<u32> = None;
    let mut seq = 0usize;
    modules
        .into_iter()
        .map(|mn| {
            if last != Some(mn) {
                last = Some(mn);
                seq = 0;
            }
            seq += 1;
            format!("{mn}-{seq}")
        })
        .collect()
}

fn overview_rows(r: &ExerciseResult, sponsor: &str, scope: &str, poc: &str) -> Vec<Vec<String>> {
    let caps = |fw: &str| {
        r.capabilities
            .iter()
            .filter(|c| c.framework == fw)
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
            .join("; ")
    };
    vec![
        vec!["Exercise name".into(), r.exercise_name.clone()],
        vec![
            "Exercise date".into(),
            r.date.clone().unwrap_or_else(|| "[Date]".into()),
        ],
        vec![
            "Scope".into(),
            format!(
                "This exercise is a {}, planned for {} hours at {}. Exercise play is limited to {}.",
                r.exercise_type.to_lowercase(),
                md::num(r.duration_hours, 1),
                r.location.clone().unwrap_or_else(|| "[location]".into()),
                scope
            ),
        ],
        vec!["Focus area(s)".into(), r.mission_areas.join(", ")],
        vec!["PHEP capabilities".into(), caps("PHEP")],
        vec!["FEMA core capabilities".into(), caps("FEMA")],
        vec![
            "Objectives".into(),
            r.objectives
                .iter()
                .map(|o| format!("{}. {}", o.number, o.text))
                .collect::<Vec<_>>()
                .join(" "),
        ],
        vec!["Threat or hazard".into(), r.scenario.hazard.clone()],
        vec!["Scenario".into(), r.scenario.summary.clone()],
        vec!["Sponsor".into(), sponsor.to_string()],
        vec![
            "Participating organizations".into(),
            if r.participants.organizations == 0 {
                "[List participating organizations (Appendix B)]".into()
            } else {
                format!(
                    "{} organizations (see Appendix B)",
                    r.participants.organizations
                )
            },
        ],
        vec!["Point of contact".into(), poc.to_string()],
    ]
}

fn objectives_table(r: &ExerciseResult) -> Block {
    table(
        "Exercise objectives and aligned capabilities",
        &["Objective", "PHEP capability", "FEMA core capability"],
        r.objectives
            .iter()
            .map(|o| {
                let pick = |fw: &str| {
                    o.capabilities
                        .iter()
                        .filter(|c| c.framework == fw)
                        .map(|c| c.name.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                };
                vec![
                    format!("{}. {}", o.number, o.text),
                    pick("PHEP"),
                    pick("FEMA"),
                ]
            })
            .collect(),
    )
}

fn schedule_table(r: &ExerciseResult) -> Block {
    table(
        "Exercise schedule",
        &["Time", "Activity", "Minutes"],
        r.schedule
            .iter()
            .map(|s| vec![s.time.clone(), s.activity.clone(), s.minutes.to_string()])
            .collect(),
    )
}

fn module_blocks(m: &ModuleOut, facilitator: bool) -> Vec<Block> {
    let mut b = vec![
        heading(1, format!("Module {}: {}", m.number, m.title)),
        para(format!("Scenario time frame: {}", m.timeframe)),
        heading(2, "Scenario"),
    ];
    for i in &m.injects {
        b.push(labeled(&i.time, i.text.clone()));
    }
    b.push(heading(2, "Key issues"));
    b.push(bullets(m.key_issues.iter().cloned()));
    b.push(heading(2, "Questions"));
    b.push(para(
        "Based on the information provided, discuss the issues raised in this module. Identify any \
         critical issues, decisions, requirements, or questions that should be addressed at this \
         time. The questions are suggested subjects; there is no requirement to address every one.",
    ));
    let groups: Vec<&String> = {
        let mut seen = Vec::new();
        for q in &m.questions {
            if !seen.contains(&&q.group) {
                seen.push(&q.group);
            }
        }
        seen
    };
    for g in groups {
        b.push(heading(3, g.clone()));
        b.push(numbered(m.questions.iter().filter(|q| &q.group == g).map(
            |q| {
                if facilitator {
                    format!("{} [{}]", q.text, q.capabilities.join(", "))
                } else {
                    q.text.clone()
                }
            },
        )));
    }
    b
}

fn doc(ctx: &Context, title: String, subtitle: Option<String>, blocks: Vec<Block>) -> Document {
    Document {
        title,
        subtitle,
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

/// Design an HSEEP exercise.
pub fn design_hseep_exercise(
    ctx: &Context,
    args: ExerciseArgs,
) -> Result<Envelope<ExerciseResult>> {
    let et = exercise_type(opt(&args.exercise_type))?;
    let operations = et.category == "operations";
    let mut warnings: Vec<String> = Vec::new();

    // Area, town label, agency.
    let ar = area::resolve(ctx, opt(&args.area), true)?;
    let town = if ar.level == area::AreaLevel::State && opt(&args.area).is_none() {
        warnings.push(
            "No town given or saved; documents say [Town]. Give area or save your town with configure_jurisdiction."
                .into(),
        );
        "[Town]".to_string()
    } else {
        ar.name.clone()
    };
    let agency = opt(&args.agency)
        .map(str::to_string)
        .or_else(|| ctx.settings().letterhead.map(|l| l.agency_name))
        .unwrap_or_else(|| format!("{town} Board of Health"));
    let agency = agency
        .strip_prefix("the ")
        .or_else(|| agency.strip_prefix("The "))
        .unwrap_or(&agency)
        .to_string();
    let agency_doc = agency.clone();

    // Scenario.
    let custom = opt(&args.custom_scenario).map(str::to_string);
    let selected = match (opt(&args.scenario), &custom) {
        (Some(q), _) => match select_scenario(q) {
            Some(s) => Some(s),
            None if custom.is_some() => None,
            None => {
                return Err(PhnError::InvalidInput(format!(
                    "scenario '{q}' is not in the library. Use one of: {}; or describe your own in custom_scenario",
                    LIBRARY
                        .scenarios
                        .iter()
                        .map(|s| s.id.as_str())
                        .chain(LIBRARY.seeds.iter().map(|s| s.id.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        },
        (None, _) => None,
    };
    if selected.is_none() && custom.is_none() && !matches!(et.id.as_str(), "seminar" | "workshop") {
        return Err(PhnError::InvalidInput(format!(
            "a {} needs a scenario: give scenario (fully developed: {}; seeds: {}) or describe your own in custom_scenario",
            et.name,
            LIBRARY
                .scenarios
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            LIBRARY
                .seeds
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let fill = |t: &str| hc::fill(t, &town, &agency);

    // Scenario description and default capabilities.
    let (scenario, default_phep, suggested, functional_groups, related_tools, scenario_id): ScenarioParts = match &selected {
        Some(Selected::Developed(s)) if custom.is_none() => (
            ScenarioOut {
                kind: "developed".into(),
                id: Some(s.id.clone()),
                title: s.title.clone(),
                hazard: s.hazard.clone(),
                summary: fill(s.summary.trim()),
                facts: s
                    .facts
                    .iter()
                    .map(|f| {
                        format!(
                            "{} ({}, {})",
                            f.text,
                            LIBRARY
                                .sources
                                .get(&f.source)
                                .map(|x| x.title.as_str())
                                .unwrap_or(&f.source),
                            f.section
                        )
                    })
                    .collect(),
            },
            s.phep.clone(),
            s.suggested_objectives.clone(),
            s.functional_groups.clone(),
            s.related_tools.clone(),
            Some(s.id.clone()),
        ),
        Some(Selected::Seed(s)) if custom.is_none() => (
            ScenarioOut {
                kind: "seed".into(),
                id: Some(s.id.clone()),
                title: s.title.clone(),
                hazard: s.hazard.clone(),
                summary: fill(&s.summary),
                facts: vec![],
            },
            s.phep.clone(),
            vec![],
            vec!["All participants".into()],
            s.related_tools.clone(),
            Some(s.id.clone()),
        ),
        _ => match &custom {
            Some(c) => (
                ScenarioOut {
                    kind: "custom".into(),
                    id: None,
                    title: "Custom scenario".into(),
                    hazard: "[Threat or hazard (e.g. natural/hurricane, technological/radiological release)]"
                        .into(),
                    summary: c.clone(),
                    facts: vec![],
                },
                vec![],
                vec![],
                vec!["All participants".into()],
                vec![],
                None,
            ),
            None => (
                ScenarioOut {
                    kind: "none".into(),
                    id: None,
                    title: "No scenario".into(),
                    hazard: "Not applicable (no scenario)".into(),
                    summary: format!("A {} without a scenario.", et.name.to_lowercase()),
                    facts: vec![],
                },
                vec![],
                vec![],
                vec!["All participants".into()],
                vec![],
                None,
            ),
        },
    };

    // Objectives.
    let given: Vec<&ObjectiveInput> = args
        .objectives
        .iter()
        .filter(|o| !o.text.trim().is_empty())
        .collect();
    let mut objectives: Vec<ObjectiveOut> = Vec::new();
    let mut all_phep: Vec<&'static PhepCap> = Vec::new();
    let mut all_fema: Vec<&'static FemaCap> = Vec::new();
    if !given.is_empty() {
        for o in given {
            let m = hc::map_objective(&o.text, &o.capabilities);
            add_caps(&m, &mut all_phep, &mut all_fema);
            push_objective(o.text.trim().to_string(), &m, false, &mut objectives);
        }
    } else if !suggested.is_empty() {
        for o in &suggested {
            let m = hc::map_objective(&o.text, &o.phep);
            add_caps(&m, &mut all_phep, &mut all_fema);
            push_objective(fill(&o.text), &m, true, &mut objectives);
        }
    } else {
        // One starting objective per default or requested capability.
        let mut pheps: Vec<&'static PhepCap> =
            default_phep.iter().filter_map(|id| hc::phep(id)).collect();
        for c in &args.capabilities {
            if let Ok(hc::Cap::Phep(p)) = hc::resolve(c)
                && !pheps.iter().any(|x| x.id == p.id)
            {
                pheps.push(p);
            }
        }
        if pheps.is_empty() {
            warnings.push(
                "No objectives or capabilities were given: add objectives (with capabilities) to design the exercise around."
                    .into(),
            );
        }
        for p in pheps.iter().take(5) {
            let f = p.functions.first().cloned().unwrap_or_default();
            let mut lower = f.clone();
            if let Some(c) = lower.get(0..1) {
                let c = c.to_lowercase();
                lower.replace_range(0..1, &c);
            }
            let text = format!(
                "Within the first 24 hours of the incident, discuss how the {agency} would {lower}, in accordance with the agency's emergency operations plan."
            );
            let m = hc::map_objective(&text, std::slice::from_ref(&p.id));
            add_caps(&m, &mut all_phep, &mut all_fema);
            push_objective(text, &m, true, &mut objectives);
        }
    }
    for c in &args.capabilities {
        match hc::resolve(c) {
            Ok(hc::Cap::Phep(p)) => {
                if !all_phep.iter().any(|x| x.id == p.id) {
                    all_phep.push(p);
                }
            }
            Ok(hc::Cap::Fema(f)) => {
                if !all_fema.iter().any(|x| x.id == f.id) {
                    all_fema.push(f);
                }
            }
            Err(e) => warnings.push(format!("capabilities: {e}")),
        }
    }
    all_phep.sort_by_key(|p| p.number);
    if objectives.len() > 6 {
        warnings.push(format!(
            "{} objectives: HSEEP advises a reasonable number of objectives so the scenario, conduct, and evaluation stay manageable (most tabletop exercises use 3 to 5).",
            objectives.len()
        ));
    }
    for o in &objectives {
        if !o.smart.passed() {
            warnings.push(format!(
                "Objective {} is not fully SMART ({}): {}",
                o.number,
                o.smart.label(),
                o.smart.suggestions.join(" ")
            ));
        }
        if o.capabilities.iter().all(|c| c.framework != "PHEP")
            || o.capabilities.iter().all(|c| c.framework != "FEMA")
        {
            warnings.push(format!(
                "Objective {} needs at least one PHEP and one FEMA core capability.",
                o.number
            ));
        }
    }
    let mut capabilities: Vec<CapabilityRef> = all_phep.iter().map(|p| hc::phep_ref(p)).collect();
    capabilities.extend(all_fema.iter().map(|f| hc::fema_ref(f)));
    let mission_areas = {
        let mut m = hc::mission_areas(&all_fema);
        if m.is_empty()
            && let Some(Selected::Developed(s)) = &selected
        {
            m = s
                .mission_areas
                .iter()
                .filter_map(|id| hc::CAPS.mission_areas.iter().find(|a| &a.id == id))
                .map(|a| a.name.clone())
                .collect();
        }
        m
    };

    // Modules.
    let modules: Vec<ModuleOut> = match &selected {
        Some(Selected::Developed(s)) if custom.is_none() => s
            .modules
            .iter()
            .map(|m| ModuleOut {
                number: m.number,
                title: m.title.clone(),
                timeframe: m.timeframe.clone(),
                injects: m
                    .injects
                    .iter()
                    .map(|i| InjectOut {
                        time: i.time.clone(),
                        text: fill(&i.text),
                    })
                    .collect(),
                key_issues: m.key_issues.iter().map(|k| fill(k)).collect(),
                questions: m
                    .questions
                    .iter()
                    .map(|q| QuestionOut {
                        group: q.group.clone(),
                        text: fill(&q.text),
                        capabilities: phep_tags(&q.phep),
                    })
                    .collect(),
            })
            .collect(),
        _ if scenario.kind == "none" => vec![],
        _ => generic_modules(&all_phep, &agency),
    };

    // Schedule.
    let hours = args
        .duration_hours
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or(et.default_hours);
    if hours > 12.0 {
        return Err(PhnError::InvalidInput(
            "duration_hours over 12: plan multi-day exercises as separate days".into(),
        ));
    }
    let start = match opt(&args.start_time) {
        Some(t) => NaiveTime::parse_from_str(t, "%H:%M").map_err(|_| {
            PhnError::InvalidInput(format!("start_time '{t}' is not HH:MM (24-hour)"))
        })?,
        None => NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
    };
    let total = (hours * 60.0).round() as i64;
    let mut schedule = Vec::new();
    let mut t = start;
    let push = push_item;
    if operations {
        push(
            "Player briefing and safety brief".into(),
            20,
            &mut schedule,
            &mut t,
        );
        push(
            "Exercise play (MSEL events)".into(),
            (total - 60).max(30),
            &mut schedule,
            &mut t,
        );
        push("Player hotwash".into(), 30, &mut schedule, &mut t);
        push(
            "Closing and participant feedback forms".into(),
            10,
            &mut schedule,
            &mut t,
        );
        push(
            "Controller/evaluator debrief (after the exercise)".into(),
            30,
            &mut schedule,
            &mut t,
        );
    } else {
        schedule.push(ScheduleItem {
            time: clock(start - Duration::minutes(30)),
            activity: "Registration and sign-in".into(),
            minutes: 30,
        });
        push(
            "Welcome, introductions, and exercise overview".into(),
            15,
            &mut schedule,
            &mut t,
        );
        let n = modules.len() as i64;
        if n > 0 {
            let per = ((total - 15 - 30 - 10 - 10 * (n - 1)) / n).max(20);
            for (i, m) in modules.iter().enumerate() {
                push(
                    format!("Module {}: {}", m.number, m.title),
                    per,
                    &mut schedule,
                    &mut t,
                );
                if (i as i64) < n - 1 {
                    push("Break".into(), 10, &mut schedule, &mut t);
                }
            }
        } else {
            push(
                "Presentations and facilitated discussion".into(),
                (total - 55).max(30),
                &mut schedule,
                &mut t,
            );
        }
        push("Hotwash".into(), 30, &mut schedule, &mut t);
        push(
            "Closing comments and participant feedback forms".into(),
            10,
            &mut schedule,
            &mut t,
        );
    }
    // The fixed parts (briefing or welcome, breaks, hotwash, closing) and the minimum of
    // 20 minutes for a module or 30 minutes of play can need more time than was asked for.
    // Registration (before the start) and the controller/evaluator debrief (after the
    // exercise) are outside the exercise's length.
    let planned: i64 = schedule
        .iter()
        .filter(|s| {
            s.activity != "Registration and sign-in"
                && !s.activity.starts_with("Controller/evaluator debrief")
        })
        .map(|s| i64::from(s.minutes))
        .sum();
    if planned > total {
        warnings.push(format!(
            "The schedule below needs {planned} minutes, more than the {total} minutes of duration_hours: set duration_hours to at least {:.2}, or cover fewer modules.",
            planned as f64 / 60.0
        ));
    }

    // Planning timeline.
    let today = chrono::Local::now().date_naive();
    let date = match opt(&args.date) {
        // Years outside 1900-2200 are typing errors (and would overflow the date arithmetic).
        Some(d) => Some(
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .filter(|x| (1900..=2200).contains(&x.year()))
                .ok_or_else(|| {
                    PhnError::InvalidInput(format!("date '{d}' is not a date (YYYY-MM-DD)"))
                })?,
        ),
        None => None,
    };
    let mut planning_timeline = Vec::new();
    let mut passed = Vec::new();
    for pm in &DOCTRINE.planning_meetings {
        let weeks = if operations {
            pm.weeks_before_operations
        } else {
            pm.weeks_before_discussion
        };
        let Some(w) = weeks else { continue };
        if pm.operations_only && !operations {
            continue;
        }
        let d = date.map(|d| d - Duration::weeks(w as i64));
        if let Some(d) = d
            && d < today
        {
            passed.push(pm.name.clone());
        }
        planning_timeline.push(Milestone {
            milestone: pm.name.clone(),
            when: format!("{w} weeks before"),
            date: d.map(|d| d.to_string()),
            purpose: pm.purpose.clone(),
            tool_default: true,
        });
    }
    planning_timeline.push(Milestone {
        milestone: "Exercise conduct".into(),
        when: "Exercise day".into(),
        date: date.map(|d| d.to_string()),
        purpose: format!("{} ({} hours)", et.name, md::num(hours, 1)),
        tool_default: false,
    });
    let days_re = regex::Regex::new(r"(?i)within (\d+) days").expect("valid regex");
    for s in &DOCTRINE.after_action_timeline {
        let days = days_re
            .captures(&s.when)
            .and_then(|c| c[1].parse::<i64>().ok());
        planning_timeline.push(Milestone {
            milestone: s.step.clone(),
            when: s.when.clone(),
            date: match (date, days) {
                (Some(d), Some(n)) => Some((d + Duration::days(n)).to_string()),
                (Some(d), None) => Some(d.to_string()),
                _ => None,
            },
            purpose: String::new(),
            tool_default: s.tool_default,
        });
    }
    if !passed.is_empty() {
        warnings.push(format!(
            "The planning timeline is compressed: {} would already have happened. Combine meetings (HSEEP allows a combined C&O and Initial Planning Meeting) or move the date.",
            passed.join(", ")
        ));
    }

    // Participants.
    let mut counts = ParticipantCounts::default();
    let mut orgs: BTreeSet<String> = args
        .organizations
        .iter()
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .collect();
    for p in &args.participants {
        match exercise_role(p) {
            "Facilitator" => counts.facilitators += 1,
            "Evaluator" => counts.evaluators += 1,
            "Controller" => counts.controllers += 1,
            "Observer" => counts.observers += 1,
            "Player" => counts.players += 1,
            _ => counts.other += 1,
        }
        if let Some(o) = opt(&p.organization) {
            orgs.insert(o.to_string());
        }
    }
    counts.organizations = orgs.len();
    if counts.evaluators == 0 && !args.participants.is_empty() {
        warnings.push(
            "No evaluators are listed: HSEEP calls for a lead evaluator and evaluators or note takers assigned to the objectives."
                .into(),
        );
    }
    if operations && counts.controllers == 0 && !args.participants.is_empty() {
        warnings.push("Operations-based exercises need controllers (and often simulators) to deliver MSEL injects.".into());
    }

    // Privacy: contact details typed anywhere in the request (every text field, including
    // organizations and the exercise name), except the type, scenario id, area, date,
    // time, capability names, and file settings.
    let texts = roster::argument_strings(
        &args,
        &[
            "exercise_type",
            "scenario",
            "area",
            "date",
            "start_time",
            "capabilities",
            "roster_path",
            "roster_sheet",
            "documents",
        ],
    );
    let contact_kinds = roster::contacts_in_arguments(texts.iter().map(String::as_str));

    let exercise_name = opt(&args.exercise_name)
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "{} {} {}",
                town.trim_matches(['[', ']']),
                scenario_id
                    .as_deref()
                    .map(short_title)
                    .unwrap_or("Public Health"),
                type_label(et)
            )
        });
    let category = et.category.clone();
    let mut result = ExerciseResult {
        exercise_name: exercise_name.clone(),
        exercise_type: type_label(et),
        category: category.clone(),
        type_definition: et.definition.clone(),
        area: town.clone(),
        agency: agency.clone(),
        date: date.map(|d| d.to_string()),
        location: opt(&args.location).map(str::to_string),
        duration_hours: hours,
        scenario,
        objectives,
        capabilities,
        mission_areas,
        functional_groups,
        modules,
        schedule,
        planning_timeline,
        documents: vec![],
        participants: counts,
        roster: None,
        available_scenarios: LIBRARY
            .scenarios
            .iter()
            .map(|s| ScenarioOption {
                id: s.id.clone(),
                title: s.title.clone(),
                developed: true,
            })
            .chain(LIBRARY.seeds.iter().map(|s| ScenarioOption {
                id: s.id.clone(),
                title: s.title.clone(),
                developed: false,
            }))
            .collect(),
        related_tools,
        warnings: vec![],
        output_folder: None,
    };

    // Documents.
    let formats: Vec<DocFormat> = match opt(&args.documents).map(|s| s.to_lowercase()).as_deref() {
        None | Some("docx") | Some("word") => vec![DocFormat::Docx],
        Some("pdf") => vec![DocFormat::Pdf],
        Some("both") => vec![DocFormat::Docx, DocFormat::Pdf],
        Some("none") | Some("no") => vec![],
        Some(o) => {
            return Err(PhnError::InvalidInput(format!(
                "documents '{o}': use docx, pdf, both, or none"
            )));
        }
    };
    let roster: Option<Roster> = match opt(&args.roster_path) {
        Some(p) => Some(Roster::load(ctx, p, opt(&args.roster_sheet))?),
        None => None,
    };
    let mut tally = RosterTally::default();
    // Look each participant up once (contacts stay in the documents).
    let people: Vec<Option<&roster::StaffContact>> = args
        .participants
        .iter()
        .map(|p| tally.find(roster.as_ref(), opt(&p.name), Some(&p.role)))
        .collect();
    let mut privacy = Privacy::none();
    let doc_info = |id: &str| DOCTRINE.documents.iter().find(|d| d.id == id);
    let mut doc_list: Vec<(String, Document, Option<Document>)> = Vec::new(); // (file stem, doc, xlsx doc)

    let sponsor = opt(&args.sponsor)
        .map(str::to_string)
        .unwrap_or_else(|| agency_doc.clone());
    let scope = opt(&args.scope)
        .map(str::to_string)
        .unwrap_or_else(|| "[exercise parameters]".into());
    let poc_line = match &args.point_of_contact {
        Some(p) => {
            let found = tally.find(roster.as_ref(), opt(&p.name), Some(&p.role));
            let mut parts = vec![];
            if let Some(n) = opt(&p.name)
                .map(str::to_string)
                .or_else(|| found.map(|f| f.name.clone()))
                .filter(|n| !n.is_empty())
            {
                parts.push(n);
            }
            parts.push(p.role.clone());
            parts.push(
                opt(&p.organization)
                    .map(str::to_string)
                    .unwrap_or_else(|| agency_doc.clone()),
            );
            match found.map(|f| f.contact_line()).filter(|c| !c.is_empty()) {
                Some(c) => parts.push(c),
                None => parts.push("[phone and email]".into()),
            }
            parts.join(", ")
        }
        None => "[Name, title, agency, phone, and email of the exercise director]".into(),
    };
    let overview = table(
        "Exercise overview",
        &["Item", "Detail"],
        overview_rows(&result, &sponsor, &scope, &poc_line),
    );
    let exercise_marking =
        para("EXERCISE MATERIAL: all scenario events, people, and numbers are fictional.");

    if !formats.is_empty() {
        // Situation Manual (discussion) or Exercise Plan (operations).
        if !operations {
            let mut b = vec![
                exercise_marking.clone(),
                para(
                    "This Situation Manual (SitMan) provides exercise participants with the information they need for their roles in the exercise. Some exercise material is intended for the exclusive use of exercise planners, facilitators, and evaluators.",
                ),
                heading(1, "Exercise overview"),
                overview.clone(),
                heading(1, "General information"),
                heading(2, "Exercise objectives and capabilities"),
                objectives_table(&result),
                heading(2, "Participant roles and responsibilities"),
                bullets(
                    DOCTRINE
                        .participant_roles
                        .iter()
                        .map(|r| format!("{}: {}", r.role, r.text)),
                ),
                heading(2, "Exercise structure"),
                para(format!(
                    "This exercise is a facilitated {}. Players will take part in {} module(s): {}. Each module begins with a scenario update; participants then discuss the issues in functional groups, followed by a facilitated plenary discussion.",
                    result.exercise_type.to_lowercase(),
                    result.modules.len(),
                    result
                        .modules
                        .iter()
                        .map(|m| format!("Module {}: {}", m.number, m.title))
                        .collect::<Vec<_>>()
                        .join("; ")
                )),
                labeled("Functional groups", result.functional_groups.join("; ")),
                heading(2, "Exercise guidelines"),
                bullets(DOCTRINE.exercise_guidelines.iter().cloned()),
                heading(2, "Exercise evaluation"),
                para(
                    "Evaluation is based on the exercise objectives and aligned capabilities, capability targets, and critical tasks, documented in Exercise Evaluation Guides (EEGs). Players will be asked to complete participant feedback forms.",
                ),
            ];
            for m in &result.modules {
                b.extend(module_blocks(m, false));
            }
            b.push(heading(1, "Appendix A: Exercise schedule"));
            b.push(schedule_table(&result));
            b.push(heading(1, "Appendix B: Exercise participants"));
            if orgs.is_empty() {
                b.push(para("[List participating organizations by level: federal, state, regional, local, and nongovernmental.]"));
            } else {
                b.push(bullets(orgs.iter().cloned()));
            }
            b.push(heading(1, "Appendix C: Relevant plans"));
            b.push(para("[Insert excerpts from the plans, policies, and procedures tested in this exercise.]"));
            b.push(heading(1, "Appendix D: Acronyms"));
            b.push(table(
                "Acronyms",
                &["Acronym", "Term"],
                [
                    ("DPH", "Massachusetts Department of Public Health"),
                    ("EEG", "Exercise Evaluation Guide"),
                    ("HMCC", "Health and Medical Coordinating Coalition"),
                    ("HSEEP", "Homeland Security Exercise and Evaluation Program"),
                    ("ICS", "Incident Command System"),
                    ("MEMA", "Massachusetts Emergency Management Agency"),
                    ("MRC", "Medical Reserve Corps"),
                    (
                        "PHEP",
                        "Public Health Emergency Preparedness and Response (CDC capabilities)",
                    ),
                    ("SitMan", "Situation Manual"),
                    ("TTX", "Tabletop exercise"),
                ]
                .iter()
                .map(|(a, t)| vec![a.to_string(), t.to_string()])
                .collect(),
            ));
            doc_list.push((
                "situation_manual".into(),
                doc(
                    ctx,
                    format!("{exercise_name}: Situation Manual"),
                    result.date.clone(),
                    b,
                ),
                None,
            ));

            // Facilitator Guide.
            let mut f = vec![
                para(
                    "FACILITATOR GUIDE: do not distribute to players. Use with the Situation Manual and the exercise presentation.",
                ),
                exercise_marking.clone(),
                heading(1, "Exercise overview"),
                overview.clone(),
                heading(1, "Introduction"),
                para(
                    "Facilitators guide exercise play, keep discussion focused on the objectives, and make sure all issues are explored as thoroughly as possible within the available time.",
                ),
                heading(2, "An effective facilitator"),
                bullets(DOCTRINE.facilitator_tips.iter().cloned()),
                heading(2, "Administrative considerations"),
                para(
                    "Discourage side conversations, ask that phones be silenced, and arrange tables to maximize interaction between the facilitator and participants.",
                ),
                heading(1, "Objectives"),
                objectives_table(&result),
                heading(1, "Schedule"),
                schedule_table(&result),
            ];
            if !result.scenario.facts.is_empty() {
                f.push(heading(1, "Scenario facts for the facilitator"));
                f.push(bullets(result.scenario.facts.iter().cloned()));
            }
            for m in &result.modules {
                f.extend(module_blocks(m, true));
                f.push(heading(
                    2,
                    "Probing questions for evaluators and facilitators",
                ));
                f.push(bullets(DOCTRINE.evaluation_questions.iter().cloned()));
            }
            f.push(heading(1, "Hotwash"));
            f.push(numbered(DOCTRINE.hotwash_agenda.iter().cloned()));
            f.push(para(DOCTRINE.hotwash_note.clone()));
            doc_list.push((
                "facilitator_guide".into(),
                doc(
                    ctx,
                    format!("{exercise_name}: Facilitator Guide"),
                    result.date.clone(),
                    f,
                ),
                None,
            ));
        } else {
            let mut b = vec![
                exercise_marking.clone(),
                para("The Exercise Plan (ExPlan) gives players and observers general information about the exercise, including objectives and scope. It does not contain scenario details."),
                heading(1, "Exercise overview"),
                overview.clone(),
                heading(1, "Exercise objectives and capabilities"),
                objectives_table(&result),
                heading(1, "Participant roles and responsibilities"),
                bullets(DOCTRINE.participant_roles.iter().map(|r| format!("{}: {}", r.role, r.text)).chain(std::iter::once(
                    "Controllers and simulators: plan and manage exercise play, deliver MSEL injects, and act for organizations not taking part.".to_string(),
                ))),
                heading(1, "Exercise rules"),
                bullets(DOCTRINE.exercise_guidelines.iter().cloned().chain([
                    "Real-world emergencies take priority. Anyone who sees a real emergency or a safety hazard says \"real-world emergency\"; controllers pause play.".to_string(),
                    "All exercise communications begin and end with \"This is an exercise.\"".to_string(),
                ])),
                heading(1, "Schedule"),
                schedule_table(&result),
                heading(1, "Participating organizations"),
                if orgs.is_empty() {
                    para("[List participating organizations.]")
                } else {
                    bullets(orgs.iter().cloned())
                },
            ];
            b.push(heading(1, "Extent of play"));
            b.push(para(format!("Exercise play is limited to {scope}.")));
            doc_list.push((
                "exercise_plan".into(),
                doc(
                    ctx,
                    format!("{exercise_name}: Exercise Plan"),
                    result.date.clone(),
                    b,
                ),
                None,
            ));

            // MSEL for functional and full-scale exercises.
            if matches!(et.id.as_str(), "functional" | "full_scale") {
                let injects: Vec<(u32, &InjectOut, &ModuleOut)> = result
                    .modules
                    .iter()
                    .flat_map(|m| m.injects.iter().map(move |i| (m.number, i, m)))
                    .collect();
                let play = (total - 60).max(30);
                let step = if injects.is_empty() {
                    0
                } else {
                    play / injects.len() as i64
                };
                let play_start = start + Duration::minutes(20);
                let event_ids = msel_event_ids(injects.iter().map(|(mn, _, _)| *mn));
                let rows: Vec<Vec<String>> = injects
                    .iter()
                    .enumerate()
                    .map(|(k, (_, i, m))| {
                        let at = play_start + Duration::minutes(step * k as i64);
                        let synopsis: String = i
                            .text
                            .split_whitespace()
                            .take(20)
                            .collect::<Vec<_>>()
                            .join(" ");
                        vec![
                            event_ids[k].clone(),
                            result.date.clone().unwrap_or_default(),
                            clock(at),
                            "Inject".into(),
                            "[Phone, email, radio, in person, or simulated media]".into(),
                            synopsis,
                            "[Simulated sender]".into(),
                            "[Player position]".into(),
                            i.text.clone(),
                            m.questions
                                .first()
                                .map(|q| q.text.clone())
                                .unwrap_or_default(),
                            agency_doc.clone(),
                            String::new(),
                            format!("Scenario time: {}", i.time),
                        ]
                    })
                    .collect();
                let headers: Vec<&str> = DOCTRINE
                    .msel_columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect();
                let msel = doc(
                    ctx,
                    format!("{exercise_name}: Master Scenario Events List"),
                    None,
                    vec![
                        table("MSEL", &headers, rows),
                        table(
                            "MSEL column guide",
                            &["Column", "What to enter"],
                            DOCTRINE
                                .msel_columns
                                .iter()
                                .map(|c| vec![c.name.clone(), c.text.clone()])
                                .collect(),
                        ),
                    ],
                );
                doc_list.push(("msel".into(), msel.clone(), Some(msel)));
            }
        }

        // Exercise Evaluation Guides: one per objective-capability (PHEP) pair.
        let evaluators: Vec<(&ParticipantInput, Option<&roster::StaffContact>)> = args
            .participants
            .iter()
            .zip(people.iter().copied())
            .filter(|(p, _)| exercise_role(p) == "Evaluator")
            .collect();
        let mut e: Vec<Block> = vec![exercise_marking.clone()];
        let mut n = 0usize;
        for o in &result.objectives {
            for c in o.capabilities.iter().filter(|c| c.framework == "PHEP") {
                let Some(p) = hc::phep(&c.id) else { continue };
                n += 1;
                if n > 1 {
                    e.push(Block::PageBreak);
                }
                let femas: Vec<&FemaCap> = o
                    .capabilities
                    .iter()
                    .filter(|x| x.framework == "FEMA" && p.fema_core.contains(&x.id))
                    .filter_map(|x| hc::fema(&x.id))
                    .collect();
                e.push(heading(1, format!("EEG {n}: {}", c.name)));
                e.push(labeled("Exercise name", exercise_name.clone()));
                e.push(labeled(
                    "Exercise date",
                    result.date.clone().unwrap_or_else(|| "[Date]".into()),
                ));
                e.push(labeled("Jurisdiction/organization", agency_doc.clone()));
                e.push(labeled(
                    "Venue",
                    result.location.clone().unwrap_or_else(|| "[Venue]".into()),
                ));
                e.push(labeled(
                    "Exercise objective",
                    format!("{}. {}", o.number, o.text),
                ));
                e.push(labeled(
                    "PHEP capability",
                    format!("{}: {}", c.name, p.definition),
                ));
                for f in &femas {
                    e.push(labeled(
                        &format!(
                            "FEMA core capability ({})",
                            f.mission_areas
                                .iter()
                                .map(|m| m[..1].to_uppercase() + &m[1..])
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        format!("{}: {}", f.name, f.description),
                    ));
                }
                e.push(labeled(
                    "Organizational capability target 1",
                    "[Insert a target from your plans: critical task, impact, and time frame]",
                ));
                e.push(para(
                    "Critical tasks (from the PHEP capability functions; tailor to your plans):",
                ));
                e.push(bullets(p.functions.iter().cloned()));
                e.push(labeled(
                    "Source(s)",
                    "[Plan, policy, procedure, or reference]",
                ));
                e.push(table(
                    "Observations and rating",
                    &[
                        "Capability target",
                        "Critical tasks",
                        "Observation notes and explanation of rating",
                        "Target rating (P, S, M, U)",
                    ],
                    vec![vec![
                        "[Target 1]".into(),
                        p.functions.join("; "),
                        String::new(),
                        String::new(),
                    ]],
                ));
                e.push(labeled("Final capability rating", "____"));
                let (ev, found) = if evaluators.is_empty() {
                    (None, None)
                } else {
                    let (v, f) = evaluators[(n - 1) % evaluators.len()];
                    (Some(v), f)
                };
                e.push(labeled(
                    "Evaluator",
                    ev.and_then(|v| opt(&v.name).map(str::to_string))
                        .or_else(|| found.map(|f| f.name.clone()))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "____".into()),
                ));
                e.push(labeled(
                    "Evaluator email",
                    found
                        .map(|f| f.email.clone())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "____".into()),
                ));
                e.push(labeled(
                    "Evaluator phone",
                    found
                        .map(|f| f.best_phone().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "____".into()),
                ));
                e.push(table(
                    "Ratings key",
                    &["Rating", "Definition"],
                    DOCTRINE
                        .ratings
                        .iter()
                        .map(|r| vec![format!("{} ({})", r.label, r.code), r.definition.clone()])
                        .collect(),
                ));
            }
        }
        if n == 0 {
            e.push(para("No PHEP capabilities were mapped to the objectives; add capabilities to build EEGs."));
        }
        doc_list.push((
            "exercise_evaluation_guides".into(),
            doc(
                ctx,
                format!("{exercise_name}: Exercise Evaluation Guides"),
                result.date.clone(),
                e,
            ),
            None,
        ));

        // Participant Feedback Form.
        let ff = &DOCTRINE.feedback_form;
        let mut pf = vec![
            para(
                "Thank you for participating in this exercise. Your observations and comments help us prepare. Comments are treated in a sensitive manner; your name is optional.",
            ),
            heading(1, "Part I: General information"),
            table(
                "Participant information",
                &["Item", "Response"],
                [
                    "Name (optional)",
                    "Agency/organization",
                    "Position title",
                    "Years in present position",
                    "Location during exercise",
                    "Exercises previously taken part in (0, 1-5, 6-10, 11-15, 16+)",
                    &format!("Exercise role ({})", ff.roles.join(", ")),
                ]
                .iter()
                .map(|i| vec![i.to_string(), String::new()])
                .collect(),
            ),
            heading(1, "Part II: Exercise design"),
            para(format!(
                "Rate each statement from 1 to 5: {}.",
                ff.scale
                    .iter()
                    .map(|s| format!("{} = {}", s.value, s.label.to_lowercase()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            table(
                "Assessment factors",
                &["Statement", "1", "2", "3", "4", "5"],
                ff.factors
                    .iter()
                    .map(|f| {
                        vec![
                            f.text.clone(),
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                        ]
                    })
                    .collect(),
            ),
        ];
        for q in ff.open_questions.iter().take(2) {
            pf.push(labeled(q, "________________________________________"));
        }
        pf.push(heading(1, "Part III: Participant feedback"));
        pf.push(table(
            "Strengths I observed",
            &[
                "Strength",
                "Objective",
                "Element (Planning, Organization, Equipment, Training, Exercise)",
            ],
            result
                .objectives
                .iter()
                .map(|o| {
                    vec![
                        String::new(),
                        format!("{}. {}", o.number, o.text),
                        String::new(),
                    ]
                })
                .collect(),
        ));
        pf.push(table(
            "Areas for improvement I observed",
            &[
                "Area for improvement",
                "Capability",
                "Element (Planning, Organization, Equipment, Training, Exercise)",
            ],
            result
                .capabilities
                .iter()
                .filter(|c| c.framework == "PHEP")
                .map(|c| vec![String::new(), c.name.clone(), String::new()])
                .collect(),
        ));
        for q in ff.open_questions.iter().skip(2) {
            pf.push(labeled(q, "________________________________________"));
        }
        doc_list.push((
            "participant_feedback_form".into(),
            doc(
                ctx,
                format!("{exercise_name}: Participant Feedback Form"),
                result.date.clone(),
                pf,
            ),
            None,
        ));

        // Sign-in sheet.
        let mut rows: Vec<Vec<String>> = Vec::new();
        for (p, found) in args.participants.iter().zip(people.iter().copied()) {
            rows.push(vec![
                opt(&p.name)
                    .map(str::to_string)
                    .or_else(|| found.map(|f| f.name.clone()))
                    .unwrap_or_default(),
                opt(&p.organization)
                    .map(str::to_string)
                    .or_else(|| found.map(|f| f.agency.clone()))
                    .unwrap_or_default(),
                p.role.clone(),
                exercise_role(p).to_string(),
                found.map(|f| f.email.clone()).unwrap_or_default(),
                found
                    .map(|f| f.best_phone().to_string())
                    .unwrap_or_default(),
                String::new(),
            ]);
        }
        for _ in 0..12 {
            rows.push(vec![String::new(); 7]);
        }
        doc_list.push((
            "sign_in_sheet".into(),
            doc(
                ctx,
                format!("{exercise_name}: Sign-in Sheet"),
                result.date.clone(),
                vec![table(
                    "Sign-in sheet",
                    &[
                        "Name",
                        "Organization",
                        "Position",
                        "Exercise role",
                        "Email",
                        "Phone",
                        "Signature",
                    ],
                    rows,
                )],
            ),
            None,
        ));

        // Write.
        let slug: String = exercise_name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        let (dir, shown) = output_folder(ctx, &format!("exercise_{slug}"))?;
        let mut written: Vec<(String, Vec<String>)> = Vec::new();
        for (stem, d, x) in &doc_list {
            let mut files = Vec::new();
            if let Some(x) = x {
                files.push(write_in(
                    ctx,
                    &dir,
                    &format!("{stem}.xlsx"),
                    &phn_docs::xlsx::to_xlsx(x)?,
                )?);
            } else {
                for f in &formats {
                    files.push(write_in(
                        ctx,
                        &dir,
                        &format!("{stem}.{}", f.ext()),
                        &f.render(d)?,
                    )?);
                }
            }
            written.push((stem.clone(), files));
        }
        for (stem, files) in written {
            let id = match stem.as_str() {
                "situation_manual" => "sitman",
                "exercise_plan" => "explan",
                "exercise_evaluation_guides" => "eeg",
                "participant_feedback_form" => "feedback_form",
                "sign_in_sheet" => "sign_in",
                other => other,
            };
            let info = doc_info(id);
            privacy.files_written.extend(files.iter().cloned());
            result.documents.push(DocumentOut {
                name: info
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| stem.replace('_', " ")),
                purpose: info.map(|i| i.description.clone()).unwrap_or_default(),
                distribution: info.map(|i| i.distribution.clone()).unwrap_or_default(),
                files,
            });
        }
        result.output_folder = Some(shown);
    } else {
        for id in if operations {
            vec!["explan", "msel", "eeg", "feedback_form", "sign_in"]
        } else {
            vec![
                "sitman",
                "facilitator_guide",
                "eeg",
                "feedback_form",
                "sign_in",
            ]
        } {
            if let Some(i) = doc_info(id) {
                if id == "msel" && !matches!(et.id.as_str(), "functional" | "full_scale") {
                    continue;
                }
                result.documents.push(DocumentOut {
                    name: i.name.clone(),
                    purpose: i.description.clone(),
                    distribution: i.distribution.clone(),
                    files: vec![],
                });
            }
        }
    }
    if let Some(r) = &roster {
        result.roster = Some(r.summary(tally.matched, tally.unmatched));
        privacy.notes.push(
            "Staff contact details from the roster were written only into the documents in the output folder; nothing from the roster is returned here."
                .into(),
        );
    }
    if !contact_kinds.is_empty() {
        privacy.identifiers_detected = contact_kinds.clone();
        privacy
            .notes
            .push(roster::CONTACTS_IN_ARGUMENTS_NOTE.into());
        warnings.push(roster::CONTACTS_IN_ARGUMENTS_NOTE.into());
    }
    result.warnings = warnings;

    // Summary.
    let r = &result;
    let mut s = format!(
        "**{}** — {} ({} exercise, {} hours){}\n\n",
        r.exercise_name,
        r.exercise_type,
        r.category,
        md::num(r.duration_hours, 1),
        r.date
            .as_ref()
            .map(|d| format!(", {d}"))
            .unwrap_or_default()
    );
    s.push_str(&format!(
        "**Scenario ({}):** {} — {}\n\n",
        r.scenario.kind, r.scenario.title, r.scenario.summary
    ));
    s.push_str("**Objectives**\n\n");
    s.push_str(&md::table(
        &["#", "Objective", "Capabilities", "SMART"],
        &r.objectives
            .iter()
            .map(|o| {
                vec![
                    o.number.to_string(),
                    format!(
                        "{}{}",
                        o.text,
                        if o.suggested { " *(suggested)*" } else { "" }
                    ),
                    hc::names(&o.capabilities),
                    o.smart.label(),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    s.push_str(&format!(
        "\nFocus areas: {}. SMART column: S, M, R, T shown when found; A (achievable) is for the planning team to judge.\n\n",
        if r.mission_areas.is_empty() { "none".into() } else { r.mission_areas.join(", ") }
    ));
    if !r.modules.is_empty() {
        s.push_str("**Modules**\n\n");
        for m in &r.modules {
            s.push_str(&format!(
                "- Module {} ({}): {} — {} injects, {} questions\n",
                m.number,
                m.timeframe,
                m.title,
                m.injects.len(),
                m.questions.len()
            ));
        }
        s.push('\n');
    }
    s.push_str("**Schedule**\n\n");
    s.push_str(&md::table(
        &["Time", "Activity", "Minutes"],
        &r.schedule
            .iter()
            .map(|i| vec![i.time.clone(), i.activity.clone(), i.minutes.to_string()])
            .collect::<Vec<_>>(),
    ));
    s.push_str("\n**Planning timeline** (weeks are tool defaults)\n\n");
    s.push_str(&md::table(
        &["Milestone", "When", "Date"],
        &r.planning_timeline
            .iter()
            .map(|m| {
                vec![
                    m.milestone.clone(),
                    m.when.clone(),
                    m.date.clone().unwrap_or_default(),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    if let Some(f) = &r.output_folder {
        s.push_str(&format!("\n**Documents written to `{f}`:**\n"));
        for d in &r.documents {
            s.push_str(&format!(
                "- {} ({}): {}\n",
                d.name,
                d.distribution,
                d.files.join(", ")
            ));
        }
    }
    if let Some(ro) = &r.roster {
        s.push_str(&format!(
            "\nRoster: {} staff read; {} people or positions matched, {} not matched (their contact cells are blank). Contact details appear only in the documents.\n",
            ro.staff, ro.matched, ro.unmatched
        ));
    }
    if !r.warnings.is_empty() {
        s.push_str("\n**Check before the planning meetings**\n");
        for w in &r.warnings {
            s.push_str(&format!("- {w}\n"));
        }
    }
    if !r.related_tools.is_empty() {
        s.push_str(&format!(
            "\nTools that support this scenario: {}. After the exercise, use build_after_action_report.\n",
            r.related_tools.iter().map(|t| format!("`{t}`")).collect::<Vec<_>>().join(", ")
        ));
    }

    let mut sources = hc::cite(&[
        (HSEEP, "hseep-2020"),
        (HSEEP, "hseep-templates"),
        (CAPABILITIES, "phep-2018"),
        (CAPABILITIES, "fema-eegs"),
    ]);
    if r.scenario.kind == "developed"
        && let Some(Selected::Developed(sc)) = &selected
    {
        let keys: Vec<(&str, &str)> = sc
            .facts
            .iter()
            .map(|f| (SCENARIOS, f.source.as_str()))
            .collect();
        sources.extend(hc::cite(&keys));
    }
    if let Some(ro) = &roster {
        sources.push(ro.source());
    }
    let mut caveats = vec![
        "Scenario events, people, and numbers are fictional exercise content written by this project; have your planning team and DPH regional staff vet the scenario, injects, and questions before use.".to_string(),
        format!("PHEP to FEMA core capability links are this project's mapping, not an official crosswalk: {}", hc::CAPS.mapping_note.trim()),
        "Planning meeting weeks, schedule lengths, and after-action dates are tool defaults; HSEEP sets no fixed timeline.".to_string(),
        "SMART checks are word-pattern heuristics; the planning team decides whether each objective is achievable and well framed.".to_string(),
    ];
    if roster.is_some() {
        caveats.push("The documents in the output folder contain staff contact details; store and share them like other internal emergency plans.".into());
    }
    caveats.extend(super::content::review_caveats(&[
        CAPABILITIES,
        HSEEP,
        SCENARIOS,
    ]));
    Ok(Envelope::new(s, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(
            dir.path().join("data/roster.csv"),
            "Name,Position,Cell Phone,Email\nAda Quell,Lead Evaluator,508-555-0101,ada@example.org\nBo Ren,Health Director,508-555-0102,bo@example.org\n",
        )
        .unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn measles_tabletop_uses_suggested_objectives_and_writes_documents() {
        let (dir, ctx) = ctx();
        let r = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                scenario: Some("measles".into()),
                area: Some("Amherst".into()),
                date: Some("2027-03-17".into()),
                location: Some("Town Hall".into()),
                participants: vec![
                    ParticipantInput {
                        role: "Lead Evaluator".into(),
                        name: Some("Ada Quell".into()),
                        exercise_role: Some("evaluator".into()),
                        ..Default::default()
                    },
                    ParticipantInput {
                        role: "School Nurse Leader".into(),
                        organization: Some("Amherst Public Schools".into()),
                        ..Default::default()
                    },
                ],
                point_of_contact: Some(ParticipantInput {
                    role: "Health Director".into(),
                    ..Default::default()
                }),
                roster_path: Some("roster.csv".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert_eq!(d.exercise_type, "Tabletop Exercise");
        assert_eq!(d.scenario.kind, "developed");
        assert_eq!(
            d.exercise_name,
            "Amherst Measles Response Tabletop Exercise"
        );
        assert_eq!(d.objectives.len(), 5);
        assert!(d.objectives.iter().all(|o| o.suggested && o.smart.passed()));
        assert!(
            d.objectives
                .iter()
                .all(|o| o.capabilities.iter().any(|c| c.framework == "PHEP")
                    && o.capabilities.iter().any(|c| c.framework == "FEMA"))
        );
        assert!(d.objectives[0].text.contains("Amherst Board of Health"));
        assert_eq!(d.modules.len(), 3);
        assert!(
            d.modules[0].injects[0]
                .text
                .contains("Amherst Elementary School")
        );
        assert!(d.mission_areas.contains(&"Response".to_string()));
        assert_eq!(d.schedule[0].activity, "Registration and sign-in");
        assert!(d.schedule.iter().any(|s| s.activity == "Hotwash"));
        assert!(
            d.planning_timeline
                .iter()
                .any(|m| m.milestone.starts_with("Initial Planning")
                    && m.date.as_deref() == Some("2027-01-20"))
        );
        let names: Vec<&str> = d.documents.iter().map(|x| x.name.as_str()).collect();
        assert!(
            names.contains(&"Situation Manual (SitMan)") && names.contains(&"Facilitator Guide"),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.contains("MSEL")));
        let ro = d.roster.as_ref().unwrap();
        assert_eq!((ro.matched, ro.unmatched), (2, 1));
        // Contacts only in files, never in the envelope.
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("555-0101")
                && !json.contains("ada@example.org")
                && !json.contains("bo@example.org")
        );
        let eeg = d
            .documents
            .iter()
            .find(|x| x.name.starts_with("Exercise Evaluation"))
            .unwrap();
        let bytes = std::fs::read(dir.path().join("data").join(&eeg.files[0])).unwrap();
        let text = phn_local::docx::parse_docx(&bytes).unwrap().plain_text();
        // The evaluator's roster contact is in the local file only.
        assert!(text.contains("ada@example.org"), "{text}");
        assert!(text.contains("PHEP 13: Public Health Surveillance"));
        assert_eq!(r.privacy.files_written.len(), 5);
    }

    #[test]
    fn functional_exercise_writes_explan_and_msel() {
        let (_d, ctx) = ctx();
        let r = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                exercise_type: Some("Functional exercise".into()),
                scenario: Some("extreme_heat".into()),
                area: Some("Lowell".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert_eq!(d.category, "operations");
        let names: Vec<&str> = d.documents.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"Exercise Plan (ExPlan)"), "{names:?}");
        let msel = d
            .documents
            .iter()
            .find(|x| x.name.contains("MSEL"))
            .unwrap();
        assert!(msel.files[0].ends_with(".xlsx"));
        assert!(
            d.schedule
                .iter()
                .any(|s| s.activity.contains("Controller/evaluator debrief"))
        );
    }

    #[test]
    fn custom_objectives_are_checked_and_mapped() {
        let (_d, ctx) = ctx();
        let r = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                custom_scenario: Some("A norovirus outbreak at a nursing home.".into()),
                area: Some("Pittsfield".into()),
                objectives: vec![
                    ObjectiveInput { text: "Improve communication.".into(), capabilities: vec![] },
                    ObjectiveInput { text: "Within 2 hours of notification, discuss how the nurse will begin case investigation in accordance with the DPH guide.".into(), capabilities: vec!["13".into()] },
                ],
                documents: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert_eq!(d.scenario.kind, "custom");
        assert!(!d.objectives[0].smart.passed());
        assert!(d.objectives[1].smart.passed());
        assert!(
            d.objectives[1]
                .capabilities
                .iter()
                .any(|c| c.id == "phep-13")
        );
        assert!(
            d.warnings
                .iter()
                .any(|w| w.contains("Objective 1 is not fully SMART"))
        );
        assert_eq!(d.modules.len(), 3);
        assert!(d.output_folder.is_none() && r.privacy.files_written.is_empty());
    }

    #[test]
    fn errors_and_contact_warnings() {
        let (_d, ctx) = ctx();
        let e = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                area: Some("Amherst".into()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            e.contains("needs a scenario") && e.contains("measles_outbreak"),
            "{e}"
        );
        let e = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                exercise_type: Some("parade".into()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("not an HSEEP exercise type"), "{e}");
        let r = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                scenario: Some("anthrax".into()),
                area: Some("Worcester".into()),
                point_of_contact: Some(ParticipantInput {
                    role: "Director, cell 508-555-0177".into(),
                    ..Default::default()
                }),
                documents: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.scenario.kind, "seed");
        assert!(r.data.objectives.iter().all(|o| o.suggested));
        assert!(!r.privacy.identifiers_detected.is_empty());
        // A number typed into a field that was not scanned before (organizations).
        let r = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                scenario: Some("anthrax".into()),
                area: Some("Worcester".into()),
                organizations: vec!["Medical Reserve Corps (call 508-555-0177)".into()],
                documents: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.privacy.identifiers_detected.is_empty());
        // Dates far outside any real exercise are refused, not computed with.
        for bad in ["+262143-12-01", "0001-01-01"] {
            assert!(
                design_hseep_exercise(
                    &ctx,
                    ExerciseArgs {
                        scenario: Some("anthrax".into()),
                        area: Some("Worcester".into()),
                        date: Some(bad.into()),
                        documents: Some("none".into()),
                        ..Default::default()
                    },
                )
                .is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_schedule_longer_than_the_duration_is_flagged() {
        let (_d, ctx) = ctx();
        let run = |hours: Option<f64>| {
            design_hseep_exercise(
                &ctx,
                ExerciseArgs {
                    scenario: Some("measles".into()),
                    area: Some("Amherst".into()),
                    duration_hours: hours,
                    documents: Some("none".into()),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        // Three modules of at least 20 minutes, two breaks, welcome, hotwash, and closing.
        let short = run(Some(1.5));
        assert!(
            short
                .data
                .warnings
                .iter()
                .any(|w| w.contains("needs 135 minutes") && w.contains("2.25")),
            "{:?}",
            short.data.warnings
        );
        let usual = run(None);
        assert!(
            !usual
                .data
                .warnings
                .iter()
                .any(|w| w.contains("duration_hours")),
            "{:?}",
            usual.data.warnings
        );
    }

    #[test]
    fn msel_events_are_numbered_within_each_module() {
        assert_eq!(
            msel_event_ids([1, 1, 1, 2, 2, 3]),
            vec!["1-1", "1-2", "1-3", "2-1", "2-2", "3-1"]
        );
    }

    #[test]
    fn a_participant_not_on_the_roster_gets_no_contact() {
        let (dir, ctx) = ctx();
        let r = design_hseep_exercise(
            &ctx,
            ExerciseArgs {
                scenario: Some("measles".into()),
                area: Some("Amherst".into()),
                participants: vec![ParticipantInput {
                    role: "Health Director".into(),
                    name: Some("Dana Lee".into()),
                    ..Default::default()
                }],
                roster_path: Some("roster.csv".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let ro = r.data.roster.as_ref().unwrap();
        assert_eq!((ro.matched, ro.unmatched), (0, 1));
        // The roster's Health Director is someone else: that person's contacts stay out.
        let sheet = r
            .data
            .documents
            .iter()
            .find(|x| x.name.starts_with("Sign-in"))
            .unwrap();
        let bytes = std::fs::read(dir.path().join("data").join(&sheet.files[0])).unwrap();
        let text = phn_local::docx::parse_docx(&bytes).unwrap().plain_text();
        assert!(text.contains("Dana Lee"), "{text}");
        assert!(!text.contains("508-555-0102") && !text.contains("bo@example.org"), "{text}");
    }
}
