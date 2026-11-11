//! EPR-01 `generate_ics_forms`: FEMA ICS forms (201, 202, 203, 204, 205,
//! 205A, 206, 207, 208, 213, 213RR, 214, 215A) filled from incident details
//! and assembled into an Incident Action Plan (DOCX and/or PDF), with blank
//! forms on request.
//!
//! The forms keep FEMA's titles, block numbers, and block labels (NIMS ICS
//! Forms Booklet, v3/v3.1, in `content/epr/ics_forms.yaml`) but are laid out
//! as accessible documents; they are adaptations, not FEMA's fillable PDFs.
//!
//! Privacy (ADR 0009): staff names and positions come from the request; phone
//! numbers, emails, and radio call signs come only from a roster file in
//! /data (`roster_path`) and are written only to the documents in
//! /data/output. Nothing read from the roster is returned: the result lists
//! file names, which blocks are still blank, and counts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use chrono::NaiveDateTime;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, ImageBlock, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self as econtent, Cited, CitedOut, Confidence, DocSource};
use super::roster::{self, Roster, RosterSummary, StaffContact};
use crate::context::Context;
use crate::school::templates::{DocFormat, output_folder, write_in};

/// Content id of the ICS forms file.
pub const ICS_CONTENT: &str = "epr-ics-forms";

/// Shown in a blank cell or line of a form to be completed by hand.
const BLANK: &str = "________________";

/// Argument fields not scanned for typed-in contact details: file paths, dates and
/// times, form, incident, and request numbers, radio frequencies and tones, and the
/// places and facility numbers that belong on the forms (the whole medical plan).
const CONTACT_SCAN_SKIP: &[&str] = &[
    "forms",
    "format",
    "incident_type",
    "incident_number",
    "request_number",
    "period_start",
    "period_end",
    "initiated",
    "prepared_at",
    "date_time",
    "time",
    "ordered",
    "eta",
    "requested_arrival",
    "quantity",
    "zone_group",
    "channel_number",
    "rx_freq",
    "rx_tone",
    "tx_freq",
    "tx_tone",
    "mode",
    "medical_plan",
    "site_safety_plan_location",
    "delivery_location",
    "staging_area",
    "map_path",
    "roster_path",
    "roster_sheet",
];

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct IcsContent {
    sources: BTreeMap<String, DocSource>,
    iap: IapInfo,
    forms: Vec<FormDef>,
    incident_types: Vec<IncidentType>,
}

#[derive(Debug, Clone, Deserialize)]
struct IapInfo {
    #[allow(dead_code)]
    typical: Vec<String>,
    #[allow(dead_code)]
    optional: Vec<String>,
    default_set: Vec<String>,
    order: Vec<String>,
    source: String,
    section: String,
    adaptation_note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FormDef {
    id: String,
    title: String,
    version: String,
    prepared_by: String,
    iap_role: String,
    purpose: String,
    #[allow(dead_code)]
    source: String,
    blocks: Vec<BlockDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct BlockDef {
    number: String,
    label: String,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    fields: Vec<String>,
    required: bool,
    instructions: String,
    #[serde(default)]
    part: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct HazardDef {
    incident_area: String,
    hazard: String,
    mitigation: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct IncidentType {
    id: String,
    label: String,
    aliases: Vec<String>,
    command_emphasis: String,
    objectives: Vec<Cited>,
    hazards: Vec<HazardDef>,
    safety_message: Vec<Cited>,
}

static CONTENT: LazyLock<IcsContent> = LazyLock::new(|| {
    econtent::load(ICS_CONTENT).expect("content/epr/ics_forms.yaml matches its schema")
});

fn form_def(id: &str) -> &'static FormDef {
    CONTENT
        .forms
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("ICS {id} is defined in content/epr/ics_forms.yaml"))
}

/// Every form the tool writes, in catalog order.
pub const FORM_IDS: &[&str] = &[
    "201", "202", "203", "204", "205", "205A", "206", "207", "208", "213", "213RR", "214", "215A",
];

// ---------------------------------------------------------------- arguments

/// A person on a form: a name and/or an ICS position. Never include phone
/// numbers or emails; those come from roster_path.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct Person {
    /// Name (at least first initial and last name), e.g. "A. Rivera".
    pub name: Option<String>,
    /// ICS position or title, e.g. "Planning Section Chief".
    pub position: Option<String>,
}

/// Who holds an ICS position.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PositionAssignment {
    /// ICS position, e.g. "Incident Commander", "PIO", "Medical Unit Leader", "Public Health
    /// Nurse". Several people may hold the same position.
    pub position: String,
    /// Name (optional: when omitted, the roster file's holder of the position is used in the
    /// documents).
    pub name: Option<String>,
    /// Agency, for Unified Command or agency representatives.
    pub agency: Option<String>,
}

/// A timed entry (ICS 201 block 8 or ICS 214 block 7).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TimedEntry {
    /// Date and/or time, e.g. "2026-11-09 08:30" or "0830".
    pub time: Option<String>,
    /// What happened or is planned. Do not include client names or details.
    pub text: String,
}

/// A resource line on the ICS 201 Resource Summary (block 10).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ResourceLine {
    /// Number and kind or type of resource ordered, e.g. "2 vaccination strike teams".
    pub resource: String,
    pub identifier: Option<String>,
    pub ordered: Option<String>,
    pub eta: Option<String>,
    pub arrived: Option<bool>,
    /// Location, assignment, or status.
    pub notes: Option<String>,
}

/// A resource assigned to a Division or Group (ICS 204 block 5).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AssignedResource {
    /// Resource identifier, e.g. "PHN Team 1" (TBD if not yet known).
    pub identifier: String,
    /// Leader's name (contact comes from the roster).
    pub leader: Option<String>,
    /// Total persons, including the leader.
    pub persons: Option<u32>,
    /// Reporting location, special equipment and supplies, remarks.
    pub notes: Option<String>,
}

/// A communications line on an ICS 204 (block 8).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CommLine {
    /// Name or function, e.g. "Command" or "Division A Supervisor".
    pub name_or_function: String,
    /// Radio channel, talkgroup, or "cell" (numbers come from the roster), e.g. "CH 2 Tactical".
    pub channel: Option<String>,
}

/// One Division or Group assignment: one ICS 204 each.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DivisionAssignment {
    pub branch: Option<String>,
    /// Division (geographic), e.g. "Division A" or "Town Hall POD".
    pub division: Option<String>,
    /// Group (functional), e.g. "Contact Tracing Group".
    pub group: Option<String>,
    pub staging_area: Option<String>,
    /// Division/Group Supervisor's name.
    pub supervisor: Option<String>,
    /// Branch Director's name.
    pub branch_director: Option<String>,
    pub resources: Vec<AssignedResource>,
    /// Tactical objectives for this Division or Group this operational period.
    pub work_assignments: Vec<String>,
    /// Safety problems, precautions, dropoff or pickup points.
    pub special_instructions: Option<String>,
    pub communications: Vec<CommLine>,
}

/// A radio channel (ICS 205 block 4).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct RadioChannel {
    pub zone_group: Option<String>,
    pub channel_number: Option<String>,
    /// Command, Tactical, Support, Dispatch, Ground-to-Air, or Air-to-Air.
    pub function: Option<String>,
    /// Channel name or trunked radio system talkgroup.
    pub channel_name: Option<String>,
    /// Branch, Division, Group, or Section using it.
    pub assignment: Option<String>,
    /// Receive frequency, xxx.xxxx followed by N or W.
    pub rx_freq: Option<String>,
    pub rx_tone: Option<String>,
    pub tx_freq: Option<String>,
    pub tx_tone: Option<String>,
    /// A (analog), D (digital), or M (mixed).
    pub mode: Option<String>,
    pub remarks: Option<String>,
}

/// An incident medical aid station (ICS 206 block 3).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AidStation {
    pub name: String,
    pub location: Option<String>,
    /// The station's radio frequency or main number (never a personal number).
    pub contact: Option<String>,
    pub paramedics_on_site: Option<bool>,
}

/// An ambulance service (ICS 206 block 4).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct Ambulance {
    pub service: String,
    pub location: Option<String>,
    /// Dispatch number or frequency.
    pub contact: Option<String>,
    /// "ALS" or "BLS".
    pub level: Option<String>,
    /// True for air transport.
    pub air: bool,
}

/// A hospital that could serve the incident (ICS 206 block 5).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct Hospital {
    pub name: String,
    /// Address (and latitude and longitude if it has a helipad).
    pub address: Option<String>,
    /// Main or emergency department number.
    pub contact: Option<String>,
    pub travel_time_air: Option<String>,
    pub travel_time_ground: Option<String>,
    /// Trauma center level, e.g. "I"; omit if not a trauma center.
    pub trauma_level: Option<String>,
    pub burn_center: Option<bool>,
    pub helipad: Option<bool>,
}

/// The medical plan for responders (ICS 206).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct MedicalPlan {
    pub aid_stations: Vec<AidStation>,
    pub transportation: Vec<Ambulance>,
    pub hospitals: Vec<Hospital>,
    /// Who to contact and how for a responder medical emergency; who manages an incident
    /// within the incident.
    pub special_procedures: Option<String>,
    /// True when aviation assets are used for rescue.
    pub aviation_assets: bool,
}

/// A general message (ICS 213).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct GeneralMessage {
    pub to: Person,
    pub from: Person,
    pub subject: String,
    /// "YYYY-MM-DD HH:MM" (default: when prepared).
    pub date_time: Option<String>,
    /// The message. Keep it concise and free of client details.
    pub message: String,
    pub approved_by: Option<Person>,
}

/// One item on a resource request (ICS 213RR block 4).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct RequestItem {
    pub quantity: Option<String>,
    pub kind: Option<String>,
    pub r#type: Option<String>,
    /// Vital characteristics, brand, specs, experience, size.
    pub description: String,
    /// Requested arrival date and time.
    pub requested_arrival: Option<String>,
}

/// A resource request (ICS 213RR). Blocks 10-19 are for Logistics and Finance.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ResourceRequest {
    pub request_number: Option<String>,
    /// "YYYY-MM-DD HH:MM" (default: when prepared).
    pub date_time: Option<String>,
    pub items: Vec<RequestItem>,
    pub delivery_location: Option<String>,
    pub substitutes: Option<String>,
    pub requested_by: Person,
    /// "urgent", "routine", or "low".
    pub priority: Option<String>,
    /// Name of the approving Section Chief.
    pub section_chief_approval: Option<String>,
}

/// A resource assigned on an activity log (ICS 214 block 6).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct LogResource {
    pub name: String,
    pub position: Option<String>,
    pub home_agency: Option<String>,
}

/// An activity log (ICS 214).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ActivityLog {
    /// Unit or resource, e.g. "Medical Unit" or "PHN Team 2".
    pub unit: String,
    /// The person in charge (name and ICS position).
    pub leader: Person,
    pub home_agency: Option<String>,
    pub resources_assigned: Vec<LogResource>,
    /// Notable activities. Do not include client names or details.
    pub entries: Vec<TimedEntry>,
}

/// A line of the IAP safety analysis (ICS 215A).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SafetyLine {
    /// Branch, Division, or Group where the risk is met.
    pub incident_area: String,
    pub hazards: String,
    pub mitigations: String,
}

/// Arguments for `generate_ics_forms`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct IcsArgs {
    /// Forms to write: ICS numbers ("201", "202", "203", "204", "205", "205A", "206", "207",
    /// "208", "213", "213RR", "214", "215A"), "iap" (the default: 202, 203, 204, 205, 205A, 206,
    /// 207, 208 assembled into an Incident Action Plan), or "all".
    pub forms: Vec<String>,
    /// Incident name (block 1 of every form).
    pub incident_name: Option<String>,
    pub incident_number: Option<String>,
    /// Kind of incident, for suggested objectives, command emphasis, safety message, and safety
    /// analysis: measles_outbreak, respiratory_outbreak, mass_dispensing, coastal_storm,
    /// extreme_heat, flooding, or hazmat (words like "measles", "POD", or "hurricane" work).
    pub incident_type: Option<String>,
    /// Put the incident type's suggestions into empty blocks (202 objectives and command
    /// emphasis, 208 safety message, 215A lines), marked for review. Default false: the
    /// suggestions are returned for discussion only.
    pub use_suggestions: bool,
    /// Operational period start, "YYYY-MM-DD HH:MM" (local time, 24-hour clock).
    pub period_start: Option<String>,
    /// Operational period end, "YYYY-MM-DD HH:MM".
    pub period_end: Option<String>,
    /// ICS 201 date and time initiated (default: period_start).
    pub initiated: Option<String>,
    /// When the forms are prepared, "YYYY-MM-DD HH:MM" (default: now).
    pub prepared_at: Option<String>,
    /// Who prepares the forms (name and ICS position).
    pub prepared_by: Option<Person>,
    /// Incident Commander approving the ICS 202 (default: the Incident Commander in organization).
    pub approved_by: Option<Person>,
    /// Incident objectives in priority order (SMART: specific, measurable, action-oriented,
    /// realistic, time-sensitive).
    pub objectives: Vec<String>,
    pub command_emphasis: Option<String>,
    /// General situational awareness: weather, incident conditions, general safety message.
    pub situational_awareness: Option<String>,
    /// ICS 201 block 5: situation summary and health and safety briefing.
    pub situation_summary: Option<String>,
    /// ICS 201 block 8: current and planned actions, strategies, and tactics.
    pub actions: Vec<TimedEntry>,
    pub site_safety_plan_required: Option<bool>,
    pub site_safety_plan_location: Option<String>,
    /// ICS 208 safety message, safety plan, or site safety plan text.
    pub safety_message: Option<String>,
    /// Who holds each ICS position (names, positions, agencies only). When empty and a roster is
    /// given, the roster's position column is used, in the documents only.
    pub organization: Vec<PositionAssignment>,
    /// ICS 201 block 10 resource summary.
    pub resources: Vec<ResourceLine>,
    /// Division and Group assignments (one ICS 204 each).
    pub assignments: Vec<DivisionAssignment>,
    /// ICS 205 radio channels.
    pub radio_channels: Vec<RadioChannel>,
    /// ICS 205 block 5 special instructions.
    pub radio_instructions: Option<String>,
    /// ICS 206 medical plan for responders.
    pub medical_plan: MedicalPlan,
    /// ICS 213 general messages (one form each).
    pub messages: Vec<GeneralMessage>,
    /// ICS 213RR resource requests (one form each).
    pub resource_requests: Vec<ResourceRequest>,
    /// ICS 214 activity logs (one form each).
    pub activity_logs: Vec<ActivityLog>,
    /// ICS 215A safety analysis lines.
    pub safety_analysis: Vec<SafetyLine>,
    /// Other IAP attachments to list on the ICS 202 (e.g. "Weather forecast", "Clinic floor plan").
    pub other_attachments: Vec<String>,
    /// A PNG or JPEG map in /data for ICS 201 block 4 and the IAP map page.
    pub map_path: Option<String>,
    /// What the map shows (alt text for screen readers).
    pub map_description: Option<String>,
    /// A CSV or XLSX staff roster in /data (name, position, agency, work phone, cell phone,
    /// email, radio). Contacts are written only to the documents in /data/output.
    pub roster_path: Option<String>,
    /// Sheet name in an XLSX roster.
    pub roster_sheet: Option<String>,
    /// "docx" (default), "pdf", or "both".
    pub format: Option<String>,
    /// Also write blank copies of the requested forms (with FEMA's block instructions).
    pub blank_forms: bool,
}

// ---------------------------------------------------------------- results

/// A required block left blank on a form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlankBlock {
    /// "3. Objective(s)" (with the copy, e.g. "Division A: 6. Work Assignments").
    pub block: String,
    /// FEMA's instruction for the block.
    pub instructions: String,
}

/// One form written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FormReport {
    /// ICS number, e.g. "204".
    pub form: String,
    pub title: String,
    /// FEMA form version followed (v3 or v3.1).
    pub version: String,
    /// Copies in the file (one per Division/Group, message, request, or log).
    pub copies: usize,
    /// Files written (relative to /data).
    pub files: Vec<String>,
    /// Required blocks still blank: ask for them or complete by hand. Signatures are always by hand.
    pub blank_required: Vec<BlankBlock>,
    pub blocks_filled: usize,
    pub blocks_total: usize,
}

/// An IAP checklist item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistItem {
    pub item: String,
    pub done: bool,
}

/// The assembled Incident Action Plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IapReport {
    pub files: Vec<String>,
    /// Forms in IAP order, e.g. "ICS 202 Incident Objectives".
    pub contents: Vec<String>,
    pub checklist: Vec<ChecklistItem>,
}

/// A suggested ICS 215A line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuggestedHazard {
    pub incident_area: String,
    pub hazard: String,
    pub mitigation: String,
    pub citation: String,
    pub confidence: Confidence,
}

/// Suggestions for an incident type (drafts to adapt, not decisions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IncidentSuggestions {
    pub incident_type: String,
    pub label: String,
    pub command_emphasis: String,
    /// SMART objective templates: complete the [BRACKETED] parts.
    pub objectives: Vec<CitedOut>,
    pub safety_analysis: Vec<SuggestedHazard>,
    pub safety_message: Vec<CitedOut>,
    /// True when they were put into empty blocks of the forms (use_suggestions).
    pub used_in_forms: bool,
}

/// A form in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FormInfo {
    pub form: String,
    pub title: String,
    pub version: String,
    pub typically_prepared_by: String,
    /// "typical", "optional", or "not_typical" in an IAP (FEMA's forms list).
    pub iap_role: String,
    pub purpose: String,
}

/// Result of `generate_ics_forms`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IcsResult {
    pub incident_name: Option<String>,
    /// "11/09/2026 07:00 to 11/09/2026 19:00".
    pub operational_period: Option<String>,
    /// Folder in /data/output holding the documents.
    pub output_folder: String,
    pub forms: Vec<FormReport>,
    pub iap: Option<IapReport>,
    pub blank_files: Vec<String>,
    /// Counts only; no roster values are returned.
    pub roster: Option<RosterSummary>,
    pub suggestions: Option<IncidentSuggestions>,
    /// Incident types with suggestions.
    pub incident_types: Vec<String>,
    pub catalog: Vec<FormInfo>,
    pub adaptation_note: String,
}

// ---------------------------------------------------------------- helpers

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn nz(s: &str) -> Option<&str> {
    Some(s.trim()).filter(|v| !v.is_empty())
}

fn yes_no(b: Option<bool>) -> String {
    match b {
        Some(true) => "Yes".into(),
        Some(false) => "No".into(),
        None => String::new(),
    }
}

fn parse_dt(label: &str, s: &str) -> Result<NaiveDateTime> {
    let t = s.trim();
    for f in [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(t, f) {
            return Ok(d);
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return Ok(d.and_hms_opt(0, 0, 0).expect("midnight is valid"));
    }
    Err(PhnError::InvalidInput(format!(
        "{label} '{t}' is not a date and time; use YYYY-MM-DD HH:MM (24-hour clock)"
    )))
}

fn date_s(d: &NaiveDateTime) -> String {
    d.format("%m/%d/%Y").to_string()
}

fn time_s(d: &NaiveDateTime) -> String {
    d.format("%H:%M").to_string()
}

/// Canonical key for an ICS position (abbreviations expanded, shorthand words
/// normalized), so "Comms Unit Ldr.", "Communications Unit" and
/// "communications unit leader" all match.
fn canon(position: &str) -> String {
    let base = roster::normalize_position(position);
    let words: Vec<String> = base
        .split_whitespace()
        .map(|w| {
            match w {
                "ldr" => "leader",
                "dir" => "director",
                "spt" => "support",
                "comms" | "comm" => "communications",
                "info" => "information",
                "ops" => "operations",
                "admin" => "administration",
                "comp" => "compensation",
                "mgr" => "manager",
                "ics" => "",
                other => other,
            }
            .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect();
    let mut s = words.join(" ");
    // A unit name alone means its leader ("Medical Unit" -> "medical unit leader").
    if s.ends_with(" unit") {
        s.push_str(" leader");
    }
    let s = roster::normalize_position(&s);
    match s.as_str() {
        "ic" | "ic ucs" | "ic uc" | "incident commander s" | "incident commanders" => {
            "incident commander".into()
        }
        "public information officer" | "public info officer" => "public information officer".into(),
        "finance administration section chief" | "finance section chief" => {
            "finance administration section chief".into()
        }
        "compensation claims unit leader" => "compensation claims unit leader".into(),
        "staging area" => "staging area manager".into(),
        "air ops branch director" => "air operations branch director".into(),
        _ => s,
    }
}

/// People and their resolved names and contacts (from the roster, for local
/// documents only).
struct People<'r> {
    roster: Option<&'r Roster>,
    /// Lookup key -> roster entry (None when not found); each key counted once.
    cache: BTreeMap<String, Option<&'r StaffContact>>,
}

impl<'r> People<'r> {
    fn new(roster: Option<&'r Roster>) -> Self {
        Self {
            roster,
            cache: BTreeMap::new(),
        }
    }

    fn lookup(&mut self, name: Option<&str>, position: Option<&str>) -> Option<&'r StaffContact> {
        let r = self.roster?;
        let name = name.map(str::trim).filter(|s| !s.is_empty());
        let position = position.map(str::trim).filter(|s| !s.is_empty());
        if name.is_none() && position.is_none() {
            return None;
        }
        let key = format!(
            "{}|{}",
            name.map(|n| n.to_lowercase()).unwrap_or_default(),
            position.map(canon).unwrap_or_default()
        );
        // A name that is not on the roster gets no contact (never the position holder's);
        // the position is looked up only when no name was given.
        *self.cache.entry(key).or_insert_with(|| match name {
            Some(_) => r.find(name, None),
            None => position.and_then(|p| {
                let c = canon(p);
                r.entries.iter().find(|e| canon(&e.position) == c)
            }),
        })
    }

    fn matched(&self) -> usize {
        self.cache.values().filter(|v| v.is_some()).count()
    }

    fn unmatched(&self) -> usize {
        self.cache.values().filter(|v| v.is_none()).count()
    }
}

/// How to reach a person, for local documents.
fn contact_methods(c: &StaffContact) -> String {
    let mut parts = Vec::new();
    if !c.radio.trim().is_empty() {
        parts.push(format!("Radio: {}", c.radio.trim()));
    }
    if !c.cell_phone.trim().is_empty() {
        parts.push(format!("Cell: {}", c.cell_phone.trim()));
    }
    if !c.work_phone.trim().is_empty() {
        parts.push(format!("Work: {}", c.work_phone.trim()));
    }
    if !c.alternate_contact.trim().is_empty() {
        parts.push(format!("Alt: {}", c.alternate_contact.trim()));
    }
    if !c.email.trim().is_empty() {
        parts.push(format!("Email: {}", c.email.trim()));
    }
    parts.join("; ")
}

/// Phone and radio for an ICS 204 "Contact Number(s)" cell.
fn phone_and_radio(c: &StaffContact) -> String {
    let p = c.best_phone();
    match (p.is_empty(), c.radio.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => p.to_string(),
        (true, false) => format!("Radio: {}", c.radio.trim()),
        (false, false) => format!("{p}; Radio: {}", c.radio.trim()),
    }
}

/// A resolved organization slot.
#[derive(Debug, Clone)]
struct Holder {
    position: String,
    key: String,
    name: String,
    agency: String,
    /// All contact methods (ICS 205A).
    contact: String,
    /// Phone and radio (ICS 204).
    phone: String,
}

fn person_text(p: &Person) -> String {
    match (opt(&p.name), opt(&p.position)) {
        (Some(n), Some(pos)) => format!("{n}, {pos}"),
        (Some(n), None) => n.to_string(),
        (None, Some(pos)) => pos.to_string(),
        (None, None) => String::new(),
    }
}

// ---------------------------------------------------------------- form builder

/// Builds one copy of one form and tracks which blocks are filled.
struct Builder {
    def: &'static FormDef,
    blank: bool,
    blocks: Vec<Block>,
    filled: BTreeSet<String>,
    copy_label: Option<String>,
}

impl Builder {
    fn new(id: &str, blank: bool) -> Self {
        let def = form_def(id);
        let mut blocks = vec![Block::Heading {
            level: 1,
            text: format!("{} (ICS {})", def.title.to_uppercase(), def.id),
        }];
        blocks.push(Block::Paragraph(vec![Inline::plain(format!(
            "ICS {} {}, adapted (FEMA NIMS ICS Forms Booklet layout; block numbers and labels \
             follow FEMA's form).",
            def.id, def.version
        ))]));
        Self {
            def,
            blank,
            blocks,
            filled: BTreeSet::new(),
            copy_label: None,
        }
    }

    fn bdef(&self, n: &str) -> &'static BlockDef {
        self.def
            .blocks
            .iter()
            .find(|b| b.number == n)
            .unwrap_or_else(|| panic!("ICS {} has block {n} in the content file", self.def.id))
    }

    fn heading(&mut self, n: &str) {
        let b = self.bdef(n);
        let text = match &b.part {
            Some(p) => format!("{n}. {} ({p})", b.label),
            None => format!("{n}. {}", b.label),
        };
        self.blocks.push(Block::Heading { level: 2, text });
        if self.blank
            && let Some(p) = &b.prompt
        {
            self.blocks.push(Block::Paragraph(vec![Inline {
                text: p.clone(),
                italic: true,
                ..Inline::default()
            }]));
        }
    }

    fn mark(&mut self, n: &str, filled: bool) {
        if filled && !self.blank {
            self.filled.insert(n.to_string());
        }
    }

    /// A free-text block.
    fn text(&mut self, n: &str, v: Option<&str>) {
        self.heading(n);
        let v = if self.blank { None } else { v.and_then(nz) };
        self.mark(n, v.is_some());
        let body = v.map(str::to_string).unwrap_or_else(|| BLANK.to_string());
        for para in body.split("\n\n") {
            self.blocks.push(Block::Paragraph(vec![Inline::plain(
                para.trim().to_string(),
            )]));
        }
    }

    /// A list block (numbered).
    fn list(&mut self, n: &str, items: &[String]) {
        self.heading(n);
        let items: Vec<&String> = if self.blank {
            Vec::new()
        } else {
            items.iter().filter(|s| !s.trim().is_empty()).collect()
        };
        self.mark(n, !items.is_empty());
        if items.is_empty() {
            self.blocks
                .push(Block::Paragraph(vec![Inline::plain(BLANK.to_string())]));
        } else {
            self.blocks.push(Block::List {
                ordered: true,
                items: items
                    .into_iter()
                    .map(|s| vec![Inline::plain(s.trim().to_string())])
                    .collect(),
            });
        }
    }

    /// A table block whose columns are the block's fields (or `headers`).
    fn table_with(
        &mut self,
        n: &str,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        blank_rows: usize,
    ) {
        self.heading(n);
        let label = self.bdef(n).label.clone();
        let real: Vec<Vec<String>> = if self.blank {
            Vec::new()
        } else {
            rows.into_iter()
                .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
                .collect()
        };
        self.mark(n, !real.is_empty());
        let mut out = real;
        if out.is_empty() {
            out = (0..blank_rows.max(1))
                .map(|_| vec![String::new(); headers.len()])
                .collect();
        }
        self.blocks.push(Block::Table(TableBlock {
            caption: Some(format!("ICS {} block {n}: {label}", self.def.id)),
            headers,
            rows: out,
        }));
    }

    fn table(&mut self, n: &str, rows: Vec<Vec<String>>, blank_rows: usize) {
        let headers = self.bdef(n).fields.clone();
        self.table_with(n, headers, rows, blank_rows);
    }

    /// A one-row table (operational period, date/time, prepared by). Counts as filled when
    /// any of `required_cols` has a value (signature columns are always by hand).
    fn row(&mut self, n: &str, values: Vec<String>, required_cols: &[usize]) {
        self.heading(n);
        let label = self.bdef(n).label.clone();
        let headers = self.bdef(n).fields.clone();
        let mut values = if self.blank {
            vec![String::new(); headers.len()]
        } else {
            values
        };
        values.resize(headers.len(), String::new());
        let filled = !self.blank
            && !required_cols.is_empty()
            && required_cols
                .iter()
                .all(|&i| values.get(i).is_some_and(|v| !v.trim().is_empty()));
        self.mark(n, filled);
        self.blocks.push(Block::Table(TableBlock {
            caption: Some(format!("ICS {} block {n}: {label}", self.def.id)),
            headers,
            rows: vec![values],
        }));
    }

    /// Position/name pairs (organization blocks).
    fn positions(&mut self, n: &str, rows: Vec<(String, String)>) {
        let any = rows.iter().any(|(_, v)| !v.trim().is_empty());
        self.heading(n);
        let label = self.bdef(n).label.clone();
        let rows: Vec<Vec<String>> = rows
            .into_iter()
            .map(|(p, v)| vec![p, if self.blank { String::new() } else { v }])
            .collect();
        self.mark(n, any);
        self.blocks.push(Block::Table(TableBlock {
            caption: Some(format!("ICS {} block {n}: {label}", self.def.id)),
            headers: vec!["Position".into(), "Name".into()],
            rows,
        }));
    }

    fn para(&mut self, text: impl Into<String>) {
        self.blocks
            .push(Block::Paragraph(vec![Inline::plain(text.into())]));
    }

    /// Footer line, block instructions (blank forms), and completeness.
    fn finish(mut self) -> (Vec<Block>, Vec<BlankBlock>, usize, usize) {
        self.para(format!(
            "ICS {} · {}Page ____ · Original to the Documentation Unit.",
            self.def.id,
            if self.def.iap_role == "not_typical" {
                ""
            } else {
                "IAP "
            }
        ));
        if self.blank {
            self.blocks.push(Block::Heading {
                level: 2,
                text: format!("Instructions for ICS {} (FEMA)", self.def.id),
            });
            self.blocks.push(Block::Table(TableBlock {
                caption: Some(format!("Block instructions for ICS {}", self.def.id)),
                headers: vec!["Block".into(), "Title".into(), "Instructions".into()],
                rows: self
                    .def
                    .blocks
                    .iter()
                    .map(|b| vec![b.number.clone(), b.label.clone(), b.instructions.clone()])
                    .collect(),
            }));
        }
        let prefix = self
            .copy_label
            .as_deref()
            .map(|c| format!("{c}: "))
            .unwrap_or_default();
        let missing = self
            .def
            .blocks
            .iter()
            .filter(|b| b.required && !self.filled.contains(&b.number))
            .map(|b| BlankBlock {
                block: format!("{prefix}{}. {}", b.number, b.label),
                instructions: b.instructions.clone(),
            })
            .collect();
        let filled = self.filled.len();
        (self.blocks, missing, filled, self.def.blocks.len())
    }
}

// ---------------------------------------------------------------- incident data

struct Incident<'a> {
    args: &'a IcsArgs,
    name: String,
    number: String,
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
    prepared: NaiveDateTime,
    initiated: Option<NaiveDateTime>,
    holders: Vec<Holder>,
    prepared_name: String,
    prepared_position: String,
    approver: String,
    suggestion: Option<&'static IncidentType>,
    use_suggestions: bool,
    map: Option<(Vec<u8>, String, String)>,
}

impl Incident<'_> {
    fn period(&self) -> Vec<String> {
        vec![
            self.start.as_ref().map(date_s).unwrap_or_default(),
            self.end.as_ref().map(date_s).unwrap_or_default(),
            self.start.as_ref().map(time_s).unwrap_or_default(),
            self.end.as_ref().map(time_s).unwrap_or_default(),
        ]
    }

    fn prepared_row(&self, with_position: bool) -> Vec<String> {
        let dt = format!("{} {}", date_s(&self.prepared), time_s(&self.prepared));
        if with_position {
            vec![
                self.prepared_name.clone(),
                self.prepared_position.clone(),
                String::new(),
                dt,
            ]
        } else {
            vec![self.prepared_name.clone(), String::new(), dt]
        }
    }

    fn holders_for(&self, key: &str) -> Vec<&Holder> {
        self.holders.iter().filter(|h| h.key == key).collect()
    }

    fn names_for(&self, key: &str) -> String {
        self.holders_for(key)
            .iter()
            .map(|h| {
                if h.agency.is_empty() {
                    h.name.clone()
                } else {
                    format!("{} ({})", h.name, h.agency)
                }
            })
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    fn objectives(&self) -> Vec<String> {
        if !self.args.objectives.is_empty() {
            return self.args.objectives.clone();
        }
        match (self.use_suggestions, self.suggestion) {
            (true, Some(t)) => t
                .objectives
                .iter()
                .map(|o| format!("{} (suggested; review)", o.text))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn command_emphasis(&self) -> Option<String> {
        opt(&self.args.command_emphasis)
            .map(str::to_string)
            .or_else(|| match (self.use_suggestions, self.suggestion) {
                (true, Some(t)) => Some(format!("{} (suggested; review)", t.command_emphasis)),
                _ => None,
            })
    }

    fn safety_message(&self) -> Option<String> {
        opt(&self.args.safety_message)
            .map(str::to_string)
            .or_else(|| match (self.use_suggestions, self.suggestion) {
                (true, Some(t)) if !t.safety_message.is_empty() => Some(format!(
                    "{} (suggested; the Safety Officer reviews this message)",
                    t.safety_message
                        .iter()
                        .map(|m| m.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                )),
                _ => None,
            })
    }

    fn safety_lines(&self) -> Vec<Vec<String>> {
        if !self.args.safety_analysis.is_empty() {
            return self
                .args
                .safety_analysis
                .iter()
                .map(|l| {
                    vec![
                        l.incident_area.clone(),
                        l.hazards.clone(),
                        l.mitigations.clone(),
                    ]
                })
                .collect();
        }
        match (self.use_suggestions, self.suggestion) {
            (true, Some(t)) => t
                .hazards
                .iter()
                .map(|h| {
                    vec![
                        h.incident_area.clone(),
                        h.hazard.clone(),
                        format!("{} (suggested; review)", h.mitigation),
                    ]
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------- the forms

type Built = (Vec<Block>, Vec<BlankBlock>, usize, usize);

fn form_201(inc: &Incident, blank: bool) -> Built {
    let mut b = Builder::new("201", blank);
    b.text("1", nz(&inc.name));
    b.text("2", nz(&inc.number));
    let init = inc.initiated.or(inc.start);
    b.row(
        "3",
        vec![
            init.as_ref().map(date_s).unwrap_or_default(),
            init.as_ref().map(time_s).unwrap_or_default(),
        ],
        &[0],
    );
    b.heading("4");
    match (&inc.map, blank) {
        (Some((bytes, path, alt)), false) => {
            b.blocks.push(Block::Image(ImageBlock {
                path: path.clone(),
                alt: alt.clone(),
                data: Some(bytes.clone()),
            }));
            b.mark("4", true);
        }
        _ => b.para(format!("{BLANK} (attach the map or sketch)")),
    }
    b.text("5", opt(&inc.args.situation_summary));
    b.row("6", inc.prepared_row(true), &[0, 1]);
    b.list("7", &inc.objectives());
    let rows: Vec<Vec<String>> = inc
        .args
        .actions
        .iter()
        .map(|a| vec![a.time.clone().unwrap_or_default(), a.text.clone()])
        .collect();
    b.table("8", rows, 6);
    let org = &b.bdef("9").fields;
    let rows: Vec<(String, String)> = org
        .iter()
        .map(|f| (f.clone(), inc.names_for(&canon(f))))
        .collect();
    b.positions("9", rows);
    let rows: Vec<Vec<String>> = inc
        .args
        .resources
        .iter()
        .map(|r| {
            vec![
                r.resource.clone(),
                r.identifier.clone().unwrap_or_default(),
                r.ordered.clone().unwrap_or_default(),
                r.eta.clone().unwrap_or_default(),
                if r.arrived == Some(true) {
                    "X".into()
                } else {
                    String::new()
                },
                r.notes.clone().unwrap_or_default(),
            ]
        })
        .collect();
    b.table("10", rows, 8);
    b.finish()
}

fn form_202(inc: &Incident, blank: bool, iap_forms: &[String]) -> Built {
    let mut b = Builder::new("202", blank);
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    b.list("3", &inc.objectives());
    b.heading("4");
    let ce = inc.command_emphasis();
    let sa = opt(&inc.args.situational_awareness).map(str::to_string);
    b.mark("4", ce.is_some() || sa.is_some());
    b.para(if blank {
        BLANK.to_string()
    } else {
        ce.unwrap_or_else(|| BLANK.to_string())
    });
    b.blocks.push(Block::Heading {
        level: 3,
        text: "General Situational Awareness".into(),
    });
    b.para(if blank {
        BLANK.to_string()
    } else {
        sa.unwrap_or_else(|| BLANK.to_string())
    });
    b.row(
        "5",
        vec![
            yes_no(inc.args.site_safety_plan_required),
            opt(&inc.args.site_safety_plan_location)
                .unwrap_or("")
                .to_string(),
        ],
        &[0],
    );
    // Block 6: which items are included in the IAP.
    let included = iap_forms;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for f in &b.bdef("6").fields {
        let id = f.strip_prefix("ICS ").unwrap_or("");
        let yes = if blank {
            String::new()
        } else if !id.is_empty() {
            if included.iter().any(|x| x == id) {
                "Yes".into()
            } else {
                "No".into()
            }
        } else if f.starts_with("Map") {
            if inc.map.is_some() {
                "Yes".into()
            } else {
                "Attach".into()
            }
        } else if f.starts_with("Other") {
            inc.args.other_attachments.join("; ")
        } else if f.starts_with("Weather") {
            if inc
                .args
                .other_attachments
                .iter()
                .any(|a| a.to_lowercase().contains("weather"))
            {
                "Yes".into()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        rows.push(vec![f.clone(), yes]);
    }
    b.table_with("6", vec!["Item".into(), "Included".into()], rows, 1);
    if included.is_empty() {
        b.filled.remove("6");
    }
    b.row(
        "7",
        vec![
            inc.prepared_name.clone(),
            inc.prepared_position.clone(),
            String::new(),
        ],
        &[0, 1],
    );
    b.row(
        "8",
        vec![inc.approver.clone(), String::new(), String::new()],
        &[0],
    );
    b.para(format!(
        "Date/Time prepared: {} {}",
        if blank {
            String::new()
        } else {
            date_s(&inc.prepared)
        },
        if blank {
            String::new()
        } else {
            time_s(&inc.prepared)
        }
    ));
    b.finish()
}

fn form_203(inc: &Incident, blank: bool) -> Built {
    let mut b = Builder::new("203", blank);
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    let slot = |section: &str, field: &str| -> String {
        // Position keys for the generic labels in each section.
        let f = field.to_lowercase();
        match (section, f.as_str()) {
            ("3", "ic/ucs") => "incident commander".into(),
            ("3", "deputy") => "deputy incident commander".into(),
            (_, "chief") => match section {
                "5" => "planning section chief".into(),
                "6" => "logistics section chief".into(),
                "7" => "operations section chief".into(),
                _ => "finance administration section chief".into(),
            },
            (_, "deputy") => match section {
                "5" => "deputy planning section chief".into(),
                "6" => "deputy logistics section chief".into(),
                "7" => "deputy operations section chief".into(),
                _ => "deputy finance administration section chief".into(),
            },
            ("5", "technical specialists") => "technical specialist".into(),
            _ => canon(field),
        }
    };
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut rows3 = Vec::new();
    for f in &b.bdef("3").fields.clone() {
        let k = slot("3", f);
        used.insert(k.clone());
        rows3.push((f.clone(), inc.names_for(&k)));
    }
    b.positions("3", rows3);
    // Block 4: agency representatives.
    let reps: Vec<Vec<String>> = inc
        .holders
        .iter()
        .filter(|h| h.key.contains("agency representative") || h.key.contains("agency rep"))
        .map(|h| vec![h.agency.clone(), h.name.clone()])
        .collect();
    for h in inc.holders.iter() {
        if h.key.contains("agency rep") {
            used.insert(h.key.clone());
        }
    }
    b.table("4", reps, 3);
    for sec in ["5", "6"] {
        let mut rows = Vec::new();
        for f in &b.bdef(sec).fields.clone() {
            let k = slot(sec, f);
            used.insert(k.clone());
            rows.push((f.clone(), inc.names_for(&k)));
        }
        b.positions(sec, rows);
    }
    // Block 7: Operations, with Divisions/Groups from the assignments.
    let mut rows = Vec::new();
    for f in ["Chief", "Deputy", "Staging Area"] {
        let k = slot("7", f);
        used.insert(k.clone());
        rows.push((f.to_string(), inc.names_for(&k)));
    }
    let mut branches: Vec<(String, Vec<&DivisionAssignment>)> = Vec::new();
    for a in &inc.args.assignments {
        let br = opt(&a.branch).unwrap_or("").to_string();
        match branches.iter_mut().find(|(b, _)| *b == br) {
            Some((_, v)) => v.push(a),
            None => branches.push((br, vec![a])),
        }
    }
    if branches.is_empty() {
        rows.push(("Branch".into(), String::new()));
        rows.push(("Branch Director".into(), String::new()));
        rows.push(("Division/Group".into(), String::new()));
    }
    for (br, divs) in &branches {
        rows.push(("Branch".into(), br.clone()));
        let dir = divs
            .iter()
            .filter_map(|d| opt(&d.branch_director))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(" / ");
        rows.push(("Branch Director".into(), dir));
        for d in divs {
            let id = div_label(d);
            rows.push((
                format!("Division/Group: {id}"),
                opt(&d.supervisor).unwrap_or("").to_string(),
            ));
        }
    }
    for f in ["Air Operations Branch", "Air Ops Branch Dir."] {
        let k = canon("air operations branch director");
        used.insert(k.clone());
        let v = if f.starts_with("Air Ops") {
            inc.names_for(&k)
        } else {
            String::new()
        };
        rows.push((f.to_string(), v));
    }
    used.insert("division group supervisor".into());
    used.insert("branch director".into());
    b.positions("7", rows);
    let mut rows = Vec::new();
    for f in &b.bdef("8").fields.clone() {
        let k = slot("8", f);
        used.insert(k.clone());
        rows.push((f.clone(), inc.names_for(&k)));
    }
    b.positions("8", rows);
    // Positions that are not on the standard form (an adaptation, as FEMA allows).
    let others: Vec<Vec<String>> = inc
        .holders
        .iter()
        .filter(|h| !used.contains(&h.key))
        .map(|h| vec![h.position.clone(), h.name.clone(), h.agency.clone()])
        .collect();
    if !others.is_empty() && !blank {
        b.blocks.push(Block::Heading {
            level: 2,
            text: "Other assigned positions (adapted addition)".into(),
        });
        b.blocks.push(Block::Table(TableBlock {
            caption: Some("Positions not on the standard ICS 203".into()),
            headers: vec!["Position".into(), "Name".into(), "Agency".into()],
            rows: others,
        }));
    }
    b.row("9", inc.prepared_row(true), &[0, 1]);
    b.finish()
}

fn div_label(d: &DivisionAssignment) -> String {
    [&d.division, &d.group, &d.staging_area]
        .iter()
        .filter_map(|v| opt(v))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn form_204(
    inc: &Incident,
    people: &mut People,
    a: Option<&DivisionAssignment>,
    blank: bool,
) -> Built {
    let mut b = Builder::new("204", blank);
    let empty = DivisionAssignment::default();
    let a = a.unwrap_or(&empty);
    let label = div_label(a);
    if !label.is_empty() && !blank {
        b.copy_label = Some(label);
    }
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    let hdr = b.bdef("3").fields.clone();
    b.table_with(
        "3",
        hdr,
        vec![vec![
            opt(&a.branch).unwrap_or("").to_string(),
            opt(&a.division).unwrap_or("").to_string(),
            opt(&a.group).unwrap_or("").to_string(),
            opt(&a.staging_area).unwrap_or("").to_string(),
        ]],
        1,
    );
    // Block 4: Operations personnel with contact numbers from the roster.
    let osc = inc
        .holders_for("operations section chief")
        .first()
        .map(|h| (h.name.clone(), h.phone.clone()));
    let mut person_row = |role: &str, name: Option<&str>, position: &str| -> Vec<String> {
        let c = people.lookup(name, if name.is_some() { None } else { Some(position) });
        let shown = name
            .map(str::to_string)
            .or_else(|| c.map(|c| c.name.clone()))
            .unwrap_or_default();
        vec![
            role.to_string(),
            shown,
            c.map(phone_and_radio).unwrap_or_default(),
        ]
    };
    let mut rows = Vec::new();
    match osc {
        Some((n, c)) => rows.push(vec!["Operations Section Chief".into(), n, c]),
        None => rows.push(person_row(
            "Operations Section Chief",
            None,
            "operations section chief",
        )),
    }
    rows.push(person_row(
        "Branch Director",
        opt(&a.branch_director),
        "branch director",
    ));
    rows.push(person_row(
        "Division/Group Supervisor",
        opt(&a.supervisor),
        "division group supervisor",
    ));
    let filled4 = rows.iter().any(|r| !r[1].trim().is_empty());
    b.table_with(
        "4",
        vec!["Role".into(), "Name".into(), "Contact Number(s)".into()],
        rows,
        3,
    );
    if !filled4 {
        b.filled.remove("4");
    }
    let rows: Vec<Vec<String>> = a
        .resources
        .iter()
        .map(|r| {
            let c = people.lookup(opt(&r.leader), None);
            vec![
                r.identifier.clone(),
                r.leader.clone().unwrap_or_default(),
                r.persons.map(|p| p.to_string()).unwrap_or_default(),
                c.map(|c| c.best_phone().to_string()).unwrap_or_default(),
                r.notes.clone().unwrap_or_default(),
            ]
        })
        .collect();
    b.table("5", rows, 6);
    b.list("6", &a.work_assignments);
    b.text("7", opt(&a.special_instructions));
    let rows: Vec<Vec<String>> = a
        .communications
        .iter()
        .map(|c| {
            vec![
                c.name_or_function.clone(),
                c.channel.clone().unwrap_or_default(),
            ]
        })
        .collect();
    b.table("8", rows, 4);
    b.row("9", inc.prepared_row(true), &[0, 1]);
    b.finish()
}

fn form_205(inc: &Incident, blank: bool) -> Built {
    let mut b = Builder::new("205", blank);
    b.text("1", nz(&inc.name));
    b.row(
        "2",
        vec![date_s(&inc.prepared), time_s(&inc.prepared)],
        &[0],
    );
    b.row("3", inc.period(), &[0, 1, 2, 3]);
    let rows: Vec<Vec<String>> = inc
        .args
        .radio_channels
        .iter()
        .map(|c| {
            [
                &c.zone_group,
                &c.channel_number,
                &c.function,
                &c.channel_name,
                &c.assignment,
                &c.rx_freq,
                &c.rx_tone,
                &c.tx_freq,
                &c.tx_tone,
                &c.mode,
                &c.remarks,
            ]
            .iter()
            .map(|v| (*v).clone().unwrap_or_default())
            .collect()
        })
        .collect();
    b.table("4", rows, 8);
    b.text("5", opt(&inc.args.radio_instructions));
    let coml = inc.names_for("communications unit leader");
    b.row(
        "6",
        vec![
            coml,
            String::new(),
            format!("{} {}", date_s(&inc.prepared), time_s(&inc.prepared)),
        ],
        &[0],
    );
    b.finish()
}

fn form_205a(inc: &Incident, people: &mut People, blank: bool) -> (Built, bool) {
    let mut b = Builder::new("205A", blank);
    // Every assigned person: organization plus Division/Group personnel.
    let mut list: Vec<(String, String, String)> = inc
        .holders
        .iter()
        .map(|h| (h.position.clone(), h.name.clone(), h.contact.clone()))
        .collect();
    for a in &inc.args.assignments {
        let where_ = div_label(a);
        for (role, n) in [
            ("Division/Group Supervisor", opt(&a.supervisor)),
            ("Branch Director", opt(&a.branch_director)),
        ] {
            if let Some(n) = n {
                let c = people.lookup(Some(n), None);
                list.push((
                    if where_.is_empty() {
                        role.to_string()
                    } else {
                        format!("{role}, {where_}")
                    },
                    n.to_string(),
                    c.map(contact_methods).unwrap_or_default(),
                ));
            }
        }
        for r in &a.resources {
            if let Some(n) = opt(&r.leader) {
                let c = people.lookup(Some(n), None);
                list.push((
                    format!("Leader, {}", r.identifier),
                    n.to_string(),
                    c.map(contact_methods).unwrap_or_default(),
                ));
            }
        }
    }
    let mut seen = BTreeSet::new();
    list.retain(|(p, n, _)| seen.insert(format!("{}|{}", n.to_lowercase(), p.to_lowercase())));
    // Alphabetized by name (last word first).
    list.sort_by_key(|(_, n, _)| {
        let last = n.split_whitespace().last().unwrap_or("").to_lowercase();
        (last, n.to_lowercase())
    });
    let sensitive = !blank && list.iter().any(|(_, _, c)| !c.is_empty());
    if sensitive {
        b.blocks.insert(
            1,
            Block::Paragraph(vec![Inline {
                text: "SENSITIVE: CONTAINS STAFF CONTACT INFORMATION. NOT FOR PUBLIC RELEASE."
                    .into(),
                bold: true,
                ..Inline::default()
            }]),
        );
    }
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    let rows: Vec<Vec<String>> = list.into_iter().map(|(p, n, c)| vec![p, n, c]).collect();
    b.table("3", rows, 12);
    b.row("4", inc.prepared_row(true), &[0, 1]);
    (b.finish(), sensitive)
}

fn form_206(inc: &Incident, blank: bool) -> Built {
    let mut b = Builder::new("206", blank);
    let m = &inc.args.medical_plan;
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    let rows = m
        .aid_stations
        .iter()
        .map(|s| {
            vec![
                s.name.clone(),
                s.location.clone().unwrap_or_default(),
                s.contact.clone().unwrap_or_default(),
                yes_no(s.paramedics_on_site),
            ]
        })
        .collect();
    b.table("3", rows, 4);
    let rows = m
        .transportation
        .iter()
        .map(|t| {
            vec![
                format!(
                    "{}{}",
                    t.service,
                    if t.air { " (air)" } else { " (ground)" }
                ),
                t.location.clone().unwrap_or_default(),
                t.contact.clone().unwrap_or_default(),
                t.level.clone().unwrap_or_default().to_uppercase(),
            ]
        })
        .collect();
    b.table("4", rows, 3);
    let rows = m
        .hospitals
        .iter()
        .map(|h| {
            vec![
                h.name.clone(),
                h.address.clone().unwrap_or_default(),
                h.contact.clone().unwrap_or_default(),
                h.travel_time_air.clone().unwrap_or_default(),
                h.travel_time_ground.clone().unwrap_or_default(),
                match opt(&h.trauma_level) {
                    Some(l) => format!("Yes, Level {l}"),
                    None => "No".into(),
                },
                yes_no(h.burn_center),
                yes_no(h.helipad),
            ]
        })
        .collect();
    b.table("5", rows, 4);
    b.text("6", opt(&m.special_procedures));
    if !blank {
        b.para(format!(
            "Aviation assets are utilized for rescue: {}",
            if m.aviation_assets {
                "Yes (coordinate with Air Operations)"
            } else {
                "No"
            }
        ));
    }
    b.row(
        "7",
        vec![inc.names_for("medical unit leader"), String::new()],
        &[0],
    );
    b.row(
        "8",
        vec![
            inc.names_for("safety officer"),
            String::new(),
            String::new(),
        ],
        &[0],
    );
    b.finish()
}

/// The ICS 207 chart as an indented table (supervisor relationships shown by
/// indentation; FEMA's wall chart is a drawing).
fn form_207(inc: &Incident, blank: bool) -> Built {
    let mut b = Builder::new("207", blank);
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    let chart: &[(&str, usize)] = &[
        ("Incident Commander(s)", 0),
        ("Public Information Officer", 1),
        ("Safety Officer", 1),
        ("Liaison Officer", 1),
        ("Operations Section Chief", 1),
        ("Staging Area Manager", 2),
        ("Planning Section Chief", 1),
        ("Resources Unit Ldr.", 2),
        ("Situation Unit Ldr.", 2),
        ("Documentation Unit Ldr.", 2),
        ("Demobilization Unit Ldr.", 2),
        ("Logistics Section Chief", 1),
        ("Support Branch Dir.", 2),
        ("Supply Unit Ldr.", 3),
        ("Facilities Unit Ldr.", 3),
        ("Ground Spt. Unit Ldr.", 3),
        ("Service Branch Dir.", 2),
        ("Comms Unit Ldr.", 3),
        ("Medical Unit Ldr.", 3),
        ("Food Unit Ldr.", 3),
        ("Finance/Admin Section Chief", 1),
        ("Time Unit Ldr.", 2),
        ("Procurement Unit Ldr.", 2),
        ("Comp./Claims Unit Ldr.", 2),
        ("Cost Unit Ldr.", 2),
    ];
    let mut used = BTreeSet::new();
    let mut rows: Vec<(String, String)> = Vec::new();
    for (label, depth) in chart {
        let k = canon(label);
        used.insert(k.clone());
        rows.push((format!("{}{label}", "— ".repeat(*depth)), inc.names_for(&k)));
        // A deputy reports to the position it deputizes for.
        let dk = format!("deputy {k}");
        if inc.holders.iter().any(|h| h.key == dk) {
            rows.push((
                format!("{}Deputy", "— ".repeat(*depth + 1)),
                inc.names_for(&dk),
            ));
            used.insert(dk);
        }
        if *label == "Staging Area Manager" {
            // Operations: Branch Directors, then their Divisions and Groups; Divisions and
            // Groups without a Branch report to the Operations Section Chief.
            let mut branches: Vec<(String, Vec<&DivisionAssignment>)> = Vec::new();
            for a in &inc.args.assignments {
                let br = opt(&a.branch).unwrap_or("").to_string();
                match branches.iter_mut().find(|(b, _)| *b == br) {
                    Some((_, v)) => v.push(a),
                    None => branches.push((br, vec![a])),
                }
            }
            for (br, divs) in &branches {
                let div_depth = if br.is_empty() {
                    2
                } else {
                    let dir = divs
                        .iter()
                        .filter_map(|d| opt(&d.branch_director))
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(" / ");
                    rows.push((format!("— — Branch Director: {br}"), dir));
                    3
                };
                for a in divs {
                    rows.push((
                        format!("{}Division/Group: {}", "— ".repeat(div_depth), div_label(a)),
                        opt(&a.supervisor).unwrap_or("").to_string(),
                    ));
                }
            }
        }
    }
    // Positions that are not on FEMA's chart: listed apart, because their reporting line
    // is not known (an indented row would read as reporting to the Incident Commander).
    let mut others: Vec<(String, String)> = Vec::new();
    for h in &inc.holders {
        if !used.contains(&h.key) {
            used.insert(h.key.clone());
            others.push((h.position.clone(), inc.names_for(&h.key)));
        }
    }
    if !others.is_empty() {
        rows.push((
            "Other assigned positions (reporting line not shown; adapted addition)".into(),
            String::new(),
        ));
        rows.extend(others);
    }
    b.positions("3", rows);
    b.row("4", inc.prepared_row(true), &[0, 1]);
    b.finish()
}

fn form_208(inc: &Incident, blank: bool) -> Built {
    let mut b = Builder::new("208", blank);
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    b.text("3", inc.safety_message().as_deref());
    b.row(
        "4",
        vec![
            yes_no(inc.args.site_safety_plan_required),
            opt(&inc.args.site_safety_plan_location)
                .unwrap_or("")
                .to_string(),
        ],
        &[0],
    );
    let so = inc.names_for("safety officer");
    let (name, pos) = if so.is_empty() {
        (inc.prepared_name.clone(), inc.prepared_position.clone())
    } else {
        (so, "Safety Officer".to_string())
    };
    b.row(
        "5",
        vec![
            name,
            pos,
            String::new(),
            format!("{} {}", date_s(&inc.prepared), time_s(&inc.prepared)),
        ],
        &[0, 1],
    );
    b.finish()
}

fn form_213(inc: &Incident, m: Option<&GeneralMessage>, blank: bool) -> Result<Built> {
    let mut b = Builder::new("213", blank);
    let empty = GeneralMessage::default();
    let m = m.unwrap_or(&empty);
    if !blank && !m.subject.trim().is_empty() {
        b.copy_label = Some(format!("Message \"{}\"", m.subject.trim()));
    }
    let when = match opt(&m.date_time) {
        Some(s) => parse_dt("message date_time", s)?,
        None => inc.prepared,
    };
    b.text("1", nz(&inc.name));
    b.text("2", nz(&person_text(&m.to)));
    b.text("3", nz(&person_text(&m.from)));
    b.text("4", nz(&m.subject));
    b.text("5", Some(&date_s(&when)));
    b.text("6", Some(&time_s(&when)));
    b.text("7", nz(&m.message));
    let ap = m.approved_by.clone().unwrap_or_default();
    b.row(
        "8",
        vec![
            opt(&ap.name).unwrap_or("").to_string(),
            String::new(),
            opt(&ap.position).unwrap_or("").to_string(),
        ],
        &[0],
    );
    b.text("9", None);
    b.row("10", Vec::new(), &[]);
    Ok(b.finish())
}

fn form_213rr(inc: &Incident, r: Option<&ResourceRequest>, blank: bool) -> Result<Built> {
    let mut b = Builder::new("213RR", blank);
    let empty = ResourceRequest::default();
    let r = r.unwrap_or(&empty);
    if !blank {
        b.copy_label = opt(&r.request_number).map(|n| format!("Request {n}"));
    }
    let when = match opt(&r.date_time) {
        Some(s) => parse_dt("request date_time", s)?,
        None => inc.prepared,
    };
    b.text("1", nz(&inc.name));
    b.text("2", Some(&format!("{} {}", date_s(&when), time_s(&when))));
    b.text("3", opt(&r.request_number));
    let rows: Vec<Vec<String>> = r
        .items
        .iter()
        .map(|i| {
            vec![
                i.quantity.clone().unwrap_or_default(),
                i.kind.clone().unwrap_or_default(),
                i.r#type.clone().unwrap_or_default(),
                i.description.clone(),
                i.requested_arrival.clone().unwrap_or_default(),
                String::new(),
                String::new(),
            ]
        })
        .collect();
    b.table("4", rows, 5);
    b.text("5", opt(&r.delivery_location));
    b.text("6", opt(&r.substitutes));
    b.text("7", nz(&person_text(&r.requested_by)));
    let pr = opt(&r.priority).map(|p| p.to_lowercase());
    if let Some(p) = &pr
        && !["urgent", "routine", "low"].contains(&p.as_str())
    {
        return Err(PhnError::InvalidInput(format!(
            "priority '{p}' is not recognized; use urgent, routine, or low"
        )));
    }
    let pr = pr.map(|p| {
        let mut c = p.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    });
    b.text("8", pr.as_deref());
    b.text("9", opt(&r.section_chief_approval));
    for n in ["10", "11", "12", "13", "14", "15", "16", "17", "18", "19"] {
        b.text(n, None);
    }
    Ok(b.finish())
}

fn form_214(inc: &Incident, l: Option<&ActivityLog>, blank: bool) -> Built {
    let mut b = Builder::new("214", blank);
    let empty = ActivityLog::default();
    let l = l.unwrap_or(&empty);
    if !blank && !l.unit.trim().is_empty() {
        b.copy_label = Some(l.unit.trim().to_string());
    }
    b.text("1", nz(&inc.name));
    b.row("2", inc.period(), &[0, 1, 2, 3]);
    b.text("3", nz(&l.unit));
    b.text("4", nz(&person_text(&l.leader)));
    b.text("5", opt(&l.home_agency));
    let rows = l
        .resources_assigned
        .iter()
        .map(|r| {
            vec![
                r.name.clone(),
                r.position.clone().unwrap_or_default(),
                r.home_agency.clone().unwrap_or_default(),
            ]
        })
        .collect();
    b.table("6", rows, 6);
    let rows = l
        .entries
        .iter()
        .map(|e| vec![e.time.clone().unwrap_or_default(), e.text.clone()])
        .collect();
    b.table("7", rows, 16);
    let leader_name = opt(&l.leader.name)
        .map(str::to_string)
        .unwrap_or_else(|| inc.prepared_name.clone());
    let leader_pos = opt(&l.leader.position)
        .map(str::to_string)
        .unwrap_or_else(|| inc.prepared_position.clone());
    b.row(
        "8",
        vec![
            leader_name,
            leader_pos,
            String::new(),
            format!("{} {}", date_s(&inc.prepared), time_s(&inc.prepared)),
        ],
        &[0, 1],
    );
    b.finish()
}

fn form_215a(inc: &Incident, blank: bool) -> Built {
    let mut b = Builder::new("215A", blank);
    b.text("1", nz(&inc.name));
    b.text("2", nz(&inc.number));
    b.row(
        "3",
        vec![date_s(&inc.prepared), time_s(&inc.prepared)],
        &[0],
    );
    b.row("4", inc.period(), &[0, 1, 2, 3]);
    let rows = inc.safety_lines();
    // Blocks 5-7 are the columns of one table.
    let any = !blank && !rows.is_empty();
    b.blocks.push(Block::Heading {
        level: 2,
        text: "5. Incident Area · 6. Hazards/Risks · 7. Mitigations".into(),
    });
    let shown = if any {
        rows
    } else {
        (0..8).map(|_| vec![String::new(); 3]).collect()
    };
    b.blocks.push(Block::Table(TableBlock {
        caption: Some(
            "ICS 215A blocks 5 to 7: incident areas, hazards and risks, mitigations".into(),
        ),
        headers: vec![
            "5. Incident Area".into(),
            "6. Hazards/Risks".into(),
            "7. Mitigations".into(),
        ],
        rows: shown,
    }));
    for n in ["5", "6", "7"] {
        b.mark(n, any);
    }
    let so = inc.names_for("safety officer");
    let osc = inc.names_for("operations section chief");
    b.row(
        "8",
        vec![so.clone(), osc, String::new(), String::new()],
        &[0],
    );
    b.finish()
}

// ---------------------------------------------------------------- the tool

fn wanted_forms(args: &IcsArgs) -> Result<(Vec<String>, bool)> {
    let defaults = &CONTENT.iap.default_set;
    if args.forms.is_empty() {
        return Ok((defaults.clone(), true));
    }
    let mut out: Vec<String> = Vec::new();
    let mut iap = false;
    for f in &args.forms {
        let k = f
            .trim()
            .to_uppercase()
            .replace("ICS", "")
            .replace([' ', '-', '_'], "");
        match k.as_str() {
            "IAP" => {
                iap = true;
                out.extend(defaults.iter().cloned());
            }
            "ALL" => out.extend(FORM_IDS.iter().map(|s| s.to_string())),
            other => match FORM_IDS.iter().find(|id| **id == other) {
                Some(id) => out.push(id.to_string()),
                None => {
                    return Err(PhnError::InvalidInput(format!(
                        "form '{f}' is not supported; use iap, all, or one of {}",
                        FORM_IDS.join(", ")
                    )));
                }
            },
        }
    }
    let mut seen = BTreeSet::new();
    out.retain(|f| seen.insert(f.clone()));
    // Catalog order.
    out.sort_by_key(|f| FORM_IDS.iter().position(|x| x == f).unwrap_or(99));
    // An IAP is assembled whenever two or more IAP forms are requested.
    let iap_count = out.iter().filter(|f| CONTENT.iap.order.contains(f)).count();
    Ok((out, iap || iap_count >= 2))
}

fn find_type(s: &str) -> Option<&'static IncidentType> {
    let n = s
        .trim()
        .to_lowercase()
        .replace(['-', '_'], " ")
        .replace('\'', "");
    CONTENT.incident_types.iter().find(|t| {
        t.id.replace('_', " ") == n
            || t.label.to_lowercase() == n
            || t.aliases
                .iter()
                .any(|a| a.replace('\'', "") == n || n.split_whitespace().any(|w| w == a))
    })
}

fn render_set(fmt: &str) -> Result<Vec<DocFormat>> {
    match fmt {
        "" | "docx" | "word" => Ok(vec![DocFormat::Docx]),
        "pdf" => Ok(vec![DocFormat::Pdf]),
        "both" => Ok(vec![DocFormat::Docx, DocFormat::Pdf]),
        other => Err(PhnError::InvalidInput(format!(
            "format '{other}' is not supported; use docx, pdf, or both"
        ))),
    }
}

fn file_stem(id: &str) -> String {
    let def = form_def(id);
    format!(
        "ICS_{}_{}",
        id,
        def.title
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_")
    )
}

fn doc(title: String, subtitle: Option<String>, blocks: Vec<Block>) -> Document {
    Document {
        title,
        subtitle,
        language: "en-US".into(),
        author: None,
        letterhead: None,
        logo: None,
        blocks,
    }
}

/// Write a document in each format; returns display paths.
fn write_doc(
    ctx: &Context,
    dir: &Path,
    stem: &str,
    d: &Document,
    formats: &[DocFormat],
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for f in formats {
        let bytes = f.render(d)?;
        out.push(write_in(ctx, dir, &format!("{stem}.{}", f.ext()), &bytes)?);
    }
    Ok(out)
}

/// Fill FEMA ICS forms and assemble an Incident Action Plan.
pub fn generate_ics_forms(ctx: &Context, args: IcsArgs) -> Result<Envelope<IcsResult>> {
    let c = &*CONTENT;
    let (forms, assemble) = wanted_forms(&args)?;
    let formats = render_set(&args.format.as_deref().unwrap_or("").trim().to_lowercase())?;

    // Dates.
    let start = opt(&args.period_start)
        .map(|s| parse_dt("period_start", s))
        .transpose()?;
    let end = opt(&args.period_end)
        .map(|s| parse_dt("period_end", s))
        .transpose()?;
    if let (Some(s), Some(e)) = (start, end)
        && e <= s
    {
        return Err(PhnError::InvalidInput(
            "period_end must be after period_start".into(),
        ));
    }
    let prepared = match opt(&args.prepared_at) {
        Some(s) => parse_dt("prepared_at", s)?,
        None => chrono::Local::now().naive_local(),
    };
    let initiated = opt(&args.initiated)
        .map(|s| parse_dt("initiated", s))
        .transpose()?;

    // Incident type.
    let mut caveats: Vec<String> = Vec::new();
    let suggestion = match opt(&args.incident_type) {
        Some(t) => {
            let found = find_type(t);
            if found.is_none() {
                caveats.push(format!(
                    "Incident type '{t}' has no suggestions; known types: {}.",
                    c.incident_types
                        .iter()
                        .map(|t| t.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            found
        }
        None => None,
    };

    // Roster (contacts for local documents only).
    let roster = match opt(&args.roster_path) {
        Some(p) => Some(Roster::load(ctx, p, opt(&args.roster_sheet))?),
        None => None,
    };
    let mut people = People::new(roster.as_ref());

    // Organization: from the request, or from the roster's positions.
    let mut holders = Vec::new();
    let org_from_roster = args.organization.is_empty() && roster.is_some();
    if org_from_roster {
        if let Some(r) = roster.as_ref() {
            for e in r.entries.iter().filter(|e| !e.position.trim().is_empty()) {
                holders.push(Holder {
                    position: e.position.clone(),
                    key: canon(&e.position),
                    name: e.name.clone(),
                    agency: e.agency.clone(),
                    contact: contact_methods(e),
                    phone: phone_and_radio(e),
                });
            }
        }
    } else {
        for a in &args.organization {
            if a.position.trim().is_empty() {
                return Err(PhnError::InvalidInput(
                    "every organization entry needs a position".into(),
                ));
            }
            let name = opt(&a.name);
            let hit = people.lookup(name, Some(&a.position));
            holders.push(Holder {
                position: a.position.trim().to_string(),
                key: canon(&a.position),
                name: name
                    .map(str::to_string)
                    .or_else(|| hit.map(|h| h.name.clone()))
                    .unwrap_or_default(),
                agency: opt(&a.agency)
                    .map(str::to_string)
                    .or_else(|| hit.map(|h| h.agency.clone()))
                    .unwrap_or_default(),
                contact: hit.map(contact_methods).unwrap_or_default(),
                phone: hit.map(phone_and_radio).unwrap_or_default(),
            });
        }
    }

    let first = |key: &str| {
        holders
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.name.clone())
    };
    let pb = args.prepared_by.clone().unwrap_or_default();
    let prepared_position = opt(&pb.position).map(str::to_string).unwrap_or_else(|| {
        if first("planning section chief").is_some() {
            "Planning Section Chief".into()
        } else {
            String::new()
        }
    });
    let prepared_name = opt(&pb.name)
        .map(str::to_string)
        .or_else(|| {
            opt(&pb.position)
                .and_then(|p| first(&canon(p)))
                .or_else(|| first("planning section chief"))
        })
        .unwrap_or_default();
    let approver = args
        .approved_by
        .as_ref()
        .and_then(|p| opt(&p.name).map(str::to_string))
        .or_else(|| first("incident commander"))
        .unwrap_or_default();

    // Map.
    let map = match opt(&args.map_path) {
        Some(p) => {
            let sb = ctx.sandbox()?;
            let path = sb.resolve_existing(p)?;
            let bytes = std::fs::read(&path)?;
            if phn_docs::image::inspect(&bytes).is_none() {
                return Err(PhnError::Unsupported(format!(
                    "map '{p}' must be a PNG or JPEG image"
                )));
            }
            let alt = opt(&args.map_description)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "Incident map for {}",
                        opt(&args.incident_name).unwrap_or("the incident")
                    )
                });
            Some((bytes, p.to_string(), alt))
        }
        None => None,
    };

    let inc = Incident {
        args: &args,
        name: opt(&args.incident_name).unwrap_or("").to_string(),
        number: opt(&args.incident_number).unwrap_or("").to_string(),
        start,
        end,
        prepared,
        initiated,
        holders,
        prepared_name,
        prepared_position,
        approver,
        suggestion,
        use_suggestions: args.use_suggestions,
        map,
    };

    // Contact details or client identifiers typed anywhere in the request (every text
    // field: people's names, leaders, message recipients, notes), except file paths, dates
    // and times, form and request numbers, radio frequencies, and the facility numbers and
    // addresses of the medical plan.
    let texts = roster::argument_strings(&args, CONTACT_SCAN_SKIP);
    let flagged = roster::contacts_in_arguments(texts.iter().map(String::as_str));

    // Output folder: a plain ASCII slug of the incident name, so a slash or a long
    // accented name cannot change or break the folder name.
    let slug: String = inc
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    let folder_name = format!(
        "ics_forms_{}",
        if slug.is_empty() {
            "incident".to_string()
        } else {
            slug.chars().take(80).collect()
        }
    );
    let (dir, shown_dir) = output_folder(ctx, &folder_name)?;
    let mut files_written = vec![shown_dir.clone()];
    let mut reports: Vec<FormReport> = Vec::new();
    let mut iap_parts: Vec<(String, Vec<Block>)> = Vec::new();
    let mut sensitive_205a = false;
    let iap_forms: Vec<String> = if assemble {
        c.iap
            .order
            .iter()
            .filter(|f| forms.contains(f))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    for id in &forms {
        let def = form_def(id);
        let mut copies: Vec<Built> = Vec::new();
        match id.as_str() {
            "201" => copies.push(form_201(&inc, false)),
            "202" => copies.push(form_202(&inc, false, &iap_forms)),
            "203" => copies.push(form_203(&inc, false)),
            "204" => {
                if args.assignments.is_empty() {
                    copies.push(form_204(&inc, &mut people, None, false));
                } else {
                    for a in &args.assignments {
                        copies.push(form_204(&inc, &mut people, Some(a), false));
                    }
                }
            }
            "205" => copies.push(form_205(&inc, false)),
            "205A" => {
                let (b, s) = form_205a(&inc, &mut people, false);
                sensitive_205a = s;
                copies.push(b);
            }
            "206" => copies.push(form_206(&inc, false)),
            "207" => copies.push(form_207(&inc, false)),
            "208" => copies.push(form_208(&inc, false)),
            "213" => {
                if args.messages.is_empty() {
                    copies.push(form_213(&inc, None, false)?);
                } else {
                    for m in &args.messages {
                        copies.push(form_213(&inc, Some(m), false)?);
                    }
                }
            }
            "213RR" => {
                if args.resource_requests.is_empty() {
                    copies.push(form_213rr(&inc, None, false)?);
                } else {
                    for r in &args.resource_requests {
                        copies.push(form_213rr(&inc, Some(r), false)?);
                    }
                }
            }
            "214" => {
                if args.activity_logs.is_empty() {
                    copies.push(form_214(&inc, None, false));
                } else {
                    for l in &args.activity_logs {
                        copies.push(form_214(&inc, Some(l), false));
                    }
                }
            }
            "215A" => copies.push(form_215a(&inc, false)),
            _ => unreachable!("form ids are validated"),
        }
        let n = copies.len();
        let mut blocks = Vec::new();
        let mut blank_required = Vec::new();
        let (mut filled, mut total) = (0, 0);
        for (i, (bl, miss, f, t)) in copies.into_iter().enumerate() {
            if i > 0 {
                blocks.push(Block::PageBreak);
            }
            blocks.extend(bl);
            blank_required.extend(miss);
            filled += f;
            total += t;
        }
        let title = format!("ICS {id} {}", def.title);
        let subtitle = Some(if inc.name.is_empty() {
            "Incident name not given".to_string()
        } else {
            inc.name.clone()
        });
        let d = doc(title.clone(), subtitle, blocks.clone());
        let files = write_doc(ctx, &dir, &file_stem(id), &d, &formats)?;
        files_written.extend(files.iter().cloned());
        if iap_forms.contains(id) {
            iap_parts.push((id.clone(), blocks));
        }
        reports.push(FormReport {
            form: id.clone(),
            title: def.title.clone(),
            version: def.version.clone(),
            copies: n,
            files,
            blank_required,
            blocks_filled: filled,
            blocks_total: total,
        });
    }

    // The Incident Action Plan.
    let mut iap = None;
    if assemble && !iap_parts.is_empty() {
        let mut blocks = vec![
            Block::Heading {
                level: 1,
                text: "INCIDENT ACTION PLAN".into(),
            },
            Block::Table(TableBlock {
                caption: Some("Incident Action Plan cover".into()),
                headers: vec!["Item".into(), "Entry".into()],
                rows: vec![
                    vec!["Incident Name".into(), inc.name.clone()],
                    vec!["Incident Number".into(), inc.number.clone()],
                    vec![
                        "Operational Period".into(),
                        period_text(start, end).unwrap_or_default(),
                    ],
                    vec![
                        "Prepared by".into(),
                        [inc.prepared_name.as_str(), inc.prepared_position.as_str()]
                            .iter()
                            .filter(|s| !s.is_empty())
                            .copied()
                            .collect::<Vec<_>>()
                            .join(", "),
                    ],
                    vec![
                        "Approved by Incident Commander".into(),
                        if inc.approver.is_empty() {
                            BLANK.into()
                        } else {
                            format!("{} (signature: {BLANK})", inc.approver)
                        },
                    ],
                ],
            }),
            Block::Heading {
                level: 2,
                text: "Contents".into(),
            },
        ];
        let mut contents: Vec<String> = iap_parts
            .iter()
            .map(|(id, _)| format!("ICS {id} {}", form_def(id).title))
            .collect();
        contents.push("Map/Chart".into());
        blocks.push(Block::List {
            ordered: true,
            items: contents
                .iter()
                .map(|s| vec![Inline::plain(s.clone())])
                .collect(),
        });
        blocks.push(Block::Paragraph(vec![Inline::plain(
            c.iap.adaptation_note.clone(),
        )]));
        for (_, part) in &iap_parts {
            blocks.push(Block::PageBreak);
            blocks.extend(part.iter().cloned());
        }
        blocks.push(Block::PageBreak);
        blocks.push(Block::Heading {
            level: 1,
            text: "MAP/CHART".into(),
        });
        match &inc.map {
            Some((bytes, path, alt)) => blocks.push(Block::Image(ImageBlock {
                path: path.clone(),
                alt: alt.clone(),
                data: Some(bytes.clone()),
            })),
            None => blocks.push(Block::Paragraph(vec![Inline::plain(format!(
                "{BLANK} Attach the incident map here (north at the top, with incident \
                 facilities, Divisions, and Groups marked)."
            ))])),
        }
        let title = if inc.name.is_empty() {
            "Incident Action Plan".to_string()
        } else {
            format!("Incident Action Plan: {}", inc.name)
        };
        let d = doc(title, period_text(start, end), blocks);
        let files = write_doc(ctx, &dir, "Incident_Action_Plan", &d, &formats)?;
        files_written.extend(files.iter().cloned());
        let has = |id: &str| reports.iter().any(|r| r.form == id);
        let complete = |id: &str| {
            reports
                .iter()
                .find(|r| r.form == id)
                .is_some_and(|r| r.blank_required.is_empty())
        };
        let checklist = vec![
            ChecklistItem {
                item: "Operational period set (ICS 202 block 2)".into(),
                done: start.is_some() && end.is_some(),
            },
            ChecklistItem {
                item: "SMART objectives in priority order (ICS 202 block 3)".into(),
                done: !inc.objectives().is_empty(),
            },
            ChecklistItem {
                item: "Incident Commander named to approve and sign the ICS 202".into(),
                done: !inc.approver.is_empty(),
            },
            ChecklistItem {
                item: "Organization staffed (ICS 203 and 207)".into(),
                done: !inc.holders.is_empty(),
            },
            ChecklistItem {
                item: "One ICS 204 for each Division or Group".into(),
                done: has("204") && !args.assignments.is_empty(),
            },
            ChecklistItem {
                item: "Radio channels (ICS 205)".into(),
                done: has("205") && !args.radio_channels.is_empty(),
            },
            ChecklistItem {
                item: "Staff contact list from a roster file (ICS 205A)".into(),
                done: has("205A") && roster.is_some(),
            },
            ChecklistItem {
                item: "Responder medical plan with hospitals and transport (ICS 206)".into(),
                done: complete("206"),
            },
            ChecklistItem {
                item: "Safety message reviewed by the Safety Officer (ICS 208)".into(),
                done: has("208") && opt(&args.safety_message).is_some(),
            },
            ChecklistItem {
                item: "Map attached".into(),
                done: inc.map.is_some(),
            },
            ChecklistItem {
                item: "Signatures added by hand (ICS 202 block 8, ICS 206 blocks 7-8)".into(),
                done: false,
            },
        ];
        iap = Some(IapReport {
            files,
            contents,
            checklist,
        });
    }

    // Blank forms.
    let mut blank_files = Vec::new();
    if args.blank_forms {
        let bdir = dir.join("blank");
        std::fs::create_dir_all(&bdir)?;
        for id in &forms {
            let (blocks, _, _, _) = match id.as_str() {
                "201" => form_201(&inc, true),
                "202" => form_202(&inc, true, &[]),
                "203" => form_203(&inc, true),
                "204" => form_204(&inc, &mut People::new(None), None, true),
                "205" => form_205(&inc, true),
                "205A" => form_205a(&inc, &mut People::new(None), true).0,
                "206" => form_206(&inc, true),
                "207" => form_207(&inc, true),
                "208" => form_208(&inc, true),
                "213" => form_213(&inc, None, true)?,
                "213RR" => form_213rr(&inc, None, true)?,
                "214" => form_214(&inc, None, true),
                "215A" => form_215a(&inc, true),
                _ => unreachable!("form ids are validated"),
            };
            let d = doc(
                format!("ICS {id} {} (blank)", form_def(id).title),
                None,
                blocks,
            );
            let files = write_doc(
                ctx,
                &bdir,
                &format!("{}_blank", file_stem(id)),
                &d,
                &formats,
            )?;
            blank_files.extend(files);
        }
        files_written.extend(blank_files.iter().cloned());
    }

    let roster_summary = roster
        .as_ref()
        .map(|r| r.summary(people.matched(), people.unmatched()));

    // Suggestions.
    let suggestions = suggestion.map(|t| IncidentSuggestions {
        incident_type: t.id.clone(),
        label: t.label.clone(),
        command_emphasis: t.command_emphasis.clone(),
        objectives: econtent::cite_all(&t.objectives, &c.sources),
        safety_analysis: t
            .hazards
            .iter()
            .map(|h| SuggestedHazard {
                incident_area: h.incident_area.clone(),
                hazard: h.hazard.clone(),
                mitigation: h.mitigation.clone(),
                citation: format!(
                    "{}, {}",
                    c.sources
                        .get(&h.source)
                        .map(|s| s.title.as_str())
                        .unwrap_or(&h.source),
                    h.section
                ),
                confidence: h.confidence,
            })
            .collect(),
        safety_message: econtent::cite_all(&t.safety_message, &c.sources),
        used_in_forms: args.use_suggestions,
    });

    // ---- summary
    let period = period_text(start, end);
    let mut s = format!(
        "**ICS forms: {}**{}\n\n",
        if inc.name.is_empty() {
            "(incident name not given)"
        } else {
            inc.name.as_str()
        },
        period
            .as_deref()
            .map(|p| format!(" — operational period {p}"))
            .unwrap_or_default()
    );
    s.push_str(&format!("Written to `{shown_dir}`:\n\n"));
    for r in &reports {
        s.push_str(&format!(
            "- **ICS {} {}** ({}{}): {} of {} blocks filled",
            r.form,
            r.title,
            r.version,
            if r.copies > 1 {
                format!(", {} copies", r.copies)
            } else {
                String::new()
            },
            r.blocks_filled,
            r.blocks_total
        ));
        if r.blank_required.is_empty() {
            s.push_str("; required blocks complete\n");
        } else {
            s.push_str(&format!(
                "; still blank: {}\n",
                r.blank_required
                    .iter()
                    .map(|b| b.block.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    if let Some(i) = &iap {
        s.push_str(&format!(
            "\n**Incident Action Plan** assembled ({}): {}.\n",
            i.files
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", "),
            i.contents.join(", ")
        ));
        let todo: Vec<&str> = i
            .checklist
            .iter()
            .filter(|c| !c.done)
            .map(|c| c.item.as_str())
            .collect();
        if !todo.is_empty() {
            s.push_str(&format!("\nBefore briefing: {}.\n", todo.join("; ")));
        }
    }
    if !blank_files.is_empty() {
        s.push_str(&format!(
            "\nBlank forms with FEMA's block instructions: {} file(s) in `{shown_dir}/blank`.\n",
            blank_files.len()
        ));
    }
    if let Some(r) = &roster_summary {
        s.push_str(&format!(
            "\n**Staff roster** `{}`: {} staff; {} people or positions matched, {} not found \
             (their contact cells are blank). Phone numbers and emails were written only to the \
             documents{}.\n",
            r.file,
            r.staff,
            r.matched,
            r.unmatched,
            if org_from_roster {
                "; the organization was taken from the roster's position column"
            } else {
                ""
            }
        ));
    } else if forms.iter().any(|f| f == "205A" || f == "204") {
        s.push_str(
            "\nNo roster file was given, so contact numbers are blank on the ICS 204 and 205A. \
             Give roster_path (a CSV/XLSX of staff with phones and emails in /data) to fill them \
             locally.\n",
        );
    }
    if sensitive_205a {
        s.push_str("The ICS 205A is marked as sensitive (staff contact information).\n");
    }
    if let Some(sg) = &suggestions {
        s.push_str(&format!(
            "\n**Suggestions for {}** ({}):\n\n- Command emphasis: {}\n",
            sg.label,
            if sg.used_in_forms {
                "put into empty blocks, marked \"suggested; review\""
            } else {
                "not put into the forms; pass them as objectives or safety_analysis after review, or set use_suggestions"
            },
            sg.command_emphasis
        ));
        for o in &sg.objectives {
            s.push_str(&format!("- Objective: {}\n", o.text));
        }
        for h in &sg.safety_analysis {
            s.push_str(&format!(
                "- 215A: {} — {} → {}{}\n",
                h.incident_area,
                h.hazard,
                h.mitigation,
                if h.confidence == Confidence::Low {
                    " (low confidence)"
                } else {
                    ""
                }
            ));
        }
    }
    s.push_str(
        "\nSignatures are added by hand. Block numbers and labels follow FEMA's ICS forms \
         (v3/v3.1); the layout is an accessible adaptation, not FEMA's fillable PDF.\n",
    );

    // ---- sources, caveats, privacy
    let mut keys: Vec<&str> = vec!["fema-ics-forms", c.iap.source.as_str()];
    if let Some(t) = suggestion {
        keys.extend(t.objectives.iter().map(|o| o.source.as_str()));
        keys.extend(t.hazards.iter().map(|h| h.source.as_str()));
        keys.extend(t.safety_message.iter().map(|h| h.source.as_str()));
    }
    let mut sources: Vec<Source> = econtent::sources_for(keys, &c.sources);
    if let Some(r) = &roster {
        sources.push(r.source());
    }
    caveats.push(format!(
        "{} ({}, {}).",
        c.iap.adaptation_note,
        c.sources
            .get(&c.iap.source)
            .map(|s| s.title.as_str())
            .unwrap_or("FEMA"),
        c.iap.section
    ));
    caveats.push(
        "These are planning documents for the Incident Commander and staff to review, correct, \
         and approve; check every entry before the operational period briefing."
            .into(),
    );
    if suggestion.is_some() {
        caveats.push(
            "Suggested objectives, command emphasis, safety messages, and hazards are drafts \
             (most are this project's suggestions, marked by confidence); the Incident Commander \
             and Safety Officer decide."
                .into(),
        );
    }
    if roster.is_some() || forms.iter().any(|f| f == "205A") {
        caveats.push(
            "Documents with staff contact information (ICS 204, 205A, and the IAP) are for \
             incident personnel only; FEMA advises sensitivity when distributing cell phone \
             numbers."
                .into(),
        );
    }
    caveats.extend(econtent::review_caveats(&[ICS_CONTENT]));

    let mut privacy = Privacy::none();
    privacy.files_written = files_written;
    if roster.is_some() {
        privacy.notes.push(
            "Staff contact details from the roster file were written only to the documents in \
             the output folder; no roster values are included in this response."
                .into(),
        );
    }
    if !flagged.is_empty() {
        privacy.identifiers_detected = flagged;
        privacy
            .notes
            .push(roster::CONTACTS_IN_ARGUMENTS_NOTE.to_string());
        privacy.notes.push(
            "Remove any client names or details from messages and activity logs; they are \
             incident records."
                .into(),
        );
    }

    let result = IcsResult {
        incident_name: (!inc.name.is_empty()).then(|| inc.name.clone()),
        operational_period: period,
        output_folder: shown_dir,
        forms: reports,
        iap,
        blank_files,
        roster: roster_summary,
        suggestions,
        incident_types: c.incident_types.iter().map(|t| t.id.clone()).collect(),
        catalog: c
            .forms
            .iter()
            .map(|f| FormInfo {
                form: f.id.clone(),
                title: f.title.clone(),
                version: f.version.clone(),
                typically_prepared_by: f.prepared_by.clone(),
                iap_role: f.iap_role.clone(),
                purpose: f.purpose.clone(),
            })
            .collect(),
        adaptation_note: c.iap.adaptation_note.clone(),
    };
    let mut env = Envelope::new(s, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy);
    if suggestion.is_some() {
        env = env.decision_support();
    }
    Ok(env)
}

fn period_text(start: Option<NaiveDateTime>, end: Option<NaiveDateTime>) -> Option<String> {
    match (start, end) {
        (Some(s), Some(e)) => Some(format!(
            "{} {} to {} {}",
            date_s(&s),
            time_s(&s),
            date_s(&e),
            time_s(&e)
        )),
        (Some(s), None) => Some(format!("from {} {}", date_s(&s), time_s(&s))),
        (None, Some(e)) => Some(format!("to {} {}", date_s(&e), time_s(&e))),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(
            dir.path().join("data/roster.csv"),
            "Name,ICS Position,Agency,Cell Phone,Email,Radio\n\
             Ada Quell,Incident Commander,Health,508-555-0101,ada@example.org,HD-1\n\
             Bo Renfrew,Operations Section Chief,Health,508-555-0102,bo@example.org,HD-2\n\
             Cy Dunmore,Safety Officer,Fire,508-555-0103,cy@example.org,\n\
             Di Prentiss,Medical Unit Leader,MRC,508-555-0104,,MRC-1\n",
        )
        .unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn docx_text(path: &Path) -> String {
        phn_local::docx::read_docx(path).unwrap().plain_text()
    }

    #[test]
    fn content_covers_every_block_the_code_uses() {
        // Building every form (filled and blank) panics if a block number is missing.
        let (_d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["all".into()],
                blank_forms: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.forms.len(), 13);
        assert_eq!(r.data.blank_files.len(), 13);
        assert_eq!(CONTENT.forms.len(), FORM_IDS.len());
        for id in FORM_IDS {
            let def = form_def(id);
            assert!(!def.blocks.is_empty());
        }
        // Every form's blocks are numbered 1..n without gaps.
        for f in &CONTENT.forms {
            for (i, b) in f.blocks.iter().enumerate() {
                assert_eq!(b.number, (i + 1).to_string(), "ICS {}", f.id);
            }
        }
    }

    #[test]
    fn iap_with_roster_fills_contacts_locally_and_returns_none() {
        let (d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                incident_name: Some("Elm Street Measles".into()),
                incident_type: Some("measles".into()),
                period_start: Some("2026-11-09 07:00".into()),
                period_end: Some("2026-11-09 19:00".into()),
                prepared_at: Some("2026-11-09 06:00".into()),
                prepared_by: Some(Person {
                    name: Some("E. Planner".into()),
                    position: Some("Planning Section Chief".into()),
                }),
                objectives: vec!["Notify all exposed contacts by 1800.".into()],
                organization: vec![
                    PositionAssignment {
                        position: "IC".into(),
                        name: None,
                        agency: None,
                    },
                    PositionAssignment {
                        position: "Operations Section Chief".into(),
                        name: Some("Bo Renfrew".into()),
                        agency: None,
                    },
                    PositionAssignment {
                        position: "Safety".into(),
                        name: None,
                        agency: None,
                    },
                    PositionAssignment {
                        position: "Medical Unit".into(),
                        name: None,
                        agency: None,
                    },
                    PositionAssignment {
                        position: "Public Health Nurse".into(),
                        name: Some("F. Nurse".into()),
                        agency: None,
                    },
                ],
                assignments: vec![DivisionAssignment {
                    group: Some("Contact Tracing Group".into()),
                    supervisor: Some("F. Nurse".into()),
                    work_assignments: vec!["Call every contact on the list.".into()],
                    ..Default::default()
                }],
                roster_path: Some("roster.csv".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let data = &r.data;
        let ids: Vec<&str> = data.forms.iter().map(|f| f.form.as_str()).collect();
        assert_eq!(
            ids,
            vec!["202", "203", "204", "205", "205A", "206", "207", "208"]
        );
        let iap = data.iap.as_ref().unwrap();
        assert_eq!(iap.contents.len(), 9);
        // Nothing from the roster is in the response.
        let json = serde_json::to_string(&r).unwrap();
        for v in [
            "508-555-01",
            "ada@example.org",
            "Ada Quell",
            "Cy Dunmore",
            "Di Prentiss",
            "MRC-1",
        ] {
            assert!(!json.contains(v), "{v} leaked");
            assert!(!r.summary.contains(v), "{v} leaked in summary");
        }
        let rs = data.roster.as_ref().unwrap();
        assert_eq!(rs.staff, 4);
        assert!(rs.matched >= 3, "{rs:?}");
        // ... but they are in the documents.
        let folder = d.path().join("data").join(&data.output_folder);
        let t205a = docx_text(&folder.join("ICS_205A_Communications_List.docx"));
        assert!(t205a.contains("508-555-0101") && t205a.contains("Ada Quell"));
        assert!(t205a.contains("NOT FOR PUBLIC RELEASE"));
        let t203 = docx_text(&folder.join("ICS_203_Organization_Assignment_List.docx"));
        assert!(t203.contains("Cy Dunmore"), "safety officer from roster");
        assert!(t203.contains("Public Health Nurse"), "adapted addition");
        let t204 = docx_text(&folder.join("ICS_204_Assignment_List.docx"));
        assert!(t204.contains("508-555-0102"), "OSC contact on the 204");
        let tiap = docx_text(&folder.join("Incident_Action_Plan.docx"));
        assert!(tiap.contains("INCIDENT OBJECTIVES (ICS 202)"));
        assert!(tiap.contains("MAP/CHART"));
        // Completeness: 202 has objectives, op period, prepared by, approver (IC from roster).
        let f202 = data.forms.iter().find(|f| f.form == "202").unwrap();
        assert!(f202.blank_required.is_empty(), "{:?}", f202.blank_required);
        // 206 has no hospitals or transport.
        let f206 = data.forms.iter().find(|f| f.form == "206").unwrap();
        assert!(
            f206.blank_required
                .iter()
                .any(|b| b.block.contains("Hospitals"))
        );
        // Suggestions returned, not used.
        let sg = data.suggestions.as_ref().unwrap();
        assert_eq!(sg.incident_type, "measles_outbreak");
        assert!(!sg.used_in_forms);
        assert!(r.notice.is_some());
    }

    #[test]
    fn contacts_in_arguments_are_flagged() {
        let (_d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["213".into()],
                messages: vec![GeneralMessage {
                    subject: "Callback".into(),
                    message: "Call me at 508-555-0199.".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.privacy.identifiers_detected.is_empty());
        assert!(
            r.privacy
                .notes
                .iter()
                .any(|n| n.contains("roster file in /data"))
        );
        assert!(r.data.iap.is_none());
    }

    #[test]
    fn a_name_not_on_the_roster_gets_no_contact() {
        let (d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["205A".into()],
                incident_name: Some("Roster Check".into()),
                organization: vec![PositionAssignment {
                    position: "Safety Officer".into(),
                    name: Some("A. Rivera".into()),
                    agency: None,
                }],
                roster_path: Some("roster.csv".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let rs = r.data.roster.as_ref().unwrap();
        assert_eq!((rs.matched, rs.unmatched), (0, 1), "{rs:?}");
        let folder = d.path().join("data").join(&r.data.output_folder);
        let t = docx_text(&folder.join("ICS_205A_Communications_List.docx"));
        assert!(t.contains("A. Rivera"), "{t}");
        // The roster's Safety Officer is someone else: none of that person's details appear.
        assert!(
            !t.contains("508-555-0103") && !t.contains("cy@example.org"),
            "{t}"
        );
        assert!(!t.contains("Cy Dunmore"), "{t}");
    }

    #[test]
    fn contacts_typed_into_any_field_are_flagged() {
        let (_d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["204".into()],
                assignments: vec![DivisionAssignment {
                    division: Some("Gym Clinic".into()),
                    supervisor: Some("Dana Lee 617-555-0123".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            r.privacy
                .identifiers_detected
                .iter()
                .any(|k| k.contains("phone")),
            "{:?}",
            r.privacy.identifiers_detected
        );
        // Facility numbers in the medical plan belong on the ICS 206 and are not flagged.
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["206".into()],
                medical_plan: MedicalPlan {
                    hospitals: vec![Hospital {
                        name: "Regional Medical Center".into(),
                        contact: Some("617-555-0100".into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.privacy.identifiers_detected.is_empty());
    }

    #[test]
    fn output_folder_is_a_slug_of_the_incident_name() {
        let (_d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["213".into()],
                incident_name: Some("Measles 11/09".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            r.data.output_folder.ends_with("ics_forms_measles_11_09"),
            "{}",
            r.data.output_folder
        );
        // A long name with accented letters is shortened without failing.
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["213".into()],
                incident_name: Some("Brote de sarampión en la escuela ".repeat(6)),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.data.output_folder.contains("ics_forms_brote_de_sarampi"));
    }

    #[test]
    fn org_chart_shows_branches_deputies_and_other_positions() {
        let (d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["207".into()],
                incident_name: Some("Chart Check".into()),
                organization: vec![
                    PositionAssignment {
                        position: "Incident Commander".into(),
                        name: Some("G. Lead".into()),
                        agency: None,
                    },
                    PositionAssignment {
                        position: "Deputy Incident Commander".into(),
                        name: Some("H. Second".into()),
                        agency: None,
                    },
                    PositionAssignment {
                        position: "Public Health Nurse".into(),
                        name: Some("F. Nurse".into()),
                        agency: None,
                    },
                ],
                assignments: vec![
                    DivisionAssignment {
                        branch: Some("Clinic Branch".into()),
                        division: Some("Gym Clinic".into()),
                        branch_director: Some("J. Director".into()),
                        supervisor: Some("K. Super".into()),
                        ..Default::default()
                    },
                    DivisionAssignment {
                        group: Some("Contact Tracing Group".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap();
        let folder = d.path().join("data").join(&r.data.output_folder);
        let t = docx_text(&folder.join("ICS_207_Incident_Organization_Chart.docx"));
        assert!(t.contains("— — Branch Director: Clinic Branch"), "{t}");
        assert!(t.contains("J. Director"), "{t}");
        assert!(t.contains("— — — Division/Group: Gym Clinic"), "{t}");
        assert!(
            t.contains("— — Division/Group: Contact Tracing Group"),
            "{t}"
        );
        assert!(t.contains("— Deputy") && t.contains("H. Second"), "{t}");
        assert!(t.contains("Other assigned positions"), "{t}");
        assert!(!t.contains("(added)"), "{t}");
    }

    #[test]
    fn suggestions_fill_empty_blocks_when_asked() {
        let (d, ctx) = ctx();
        let r = generate_ics_forms(
            &ctx,
            IcsArgs {
                forms: vec!["202".into(), "208".into(), "215A".into()],
                incident_type: Some("hurricane".into()),
                use_suggestions: true,
                format: Some("both".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let data = &r.data;
        assert_eq!(
            data.suggestions.as_ref().unwrap().incident_type,
            "coastal_storm"
        );
        let f215 = data.forms.iter().find(|f| f.form == "215A").unwrap();
        assert!(
            !f215
                .blank_required
                .iter()
                .any(|b| b.block.contains("Hazards")),
            "{:?}",
            f215.blank_required
        );
        assert_eq!(f215.files.len(), 2, "docx and pdf");
        let folder = d.path().join("data").join(&data.output_folder);
        let t = docx_text(&folder.join("ICS_215A_Incident_Action_Plan_Safety_Analysis.docx"));
        assert!(t.contains("20 feet") && t.contains("suggested; review"));
        // An IAP is assembled from 202 + 208 + 215A.
        assert!(data.iap.is_some());
    }

    #[test]
    fn rejects_bad_input() {
        let (_d, ctx) = ctx();
        for args in [
            IcsArgs {
                forms: vec!["209".into()],
                ..Default::default()
            },
            IcsArgs {
                period_start: Some("tomorrow morning".into()),
                ..Default::default()
            },
            IcsArgs {
                period_start: Some("2026-11-09 19:00".into()),
                period_end: Some("2026-11-09 07:00".into()),
                ..Default::default()
            },
            IcsArgs {
                format: Some("rtf".into()),
                ..Default::default()
            },
            IcsArgs {
                forms: vec!["213RR".into()],
                resource_requests: vec![ResourceRequest {
                    priority: Some("whenever".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
        ] {
            assert!(generate_ics_forms(&ctx, args).is_err());
        }
    }

    #[test]
    fn positions_are_canonical() {
        assert_eq!(canon("Comms Unit Ldr."), "communications unit leader");
        assert_eq!(canon("Communications Unit"), "communications unit leader");
        assert_eq!(canon("PIO"), "public information officer");
        assert_eq!(canon("Public Info. Officer"), "public information officer");
        assert_eq!(
            canon("Finance/Admin Section Chief"),
            "finance administration section chief"
        );
        assert_eq!(canon("Incident Commander(s)"), "incident commander");
        assert_eq!(canon("Staging Area"), "staging area manager");
        assert_eq!(canon("Ground Spt. Unit Ldr."), "ground support unit leader");
        assert_eq!(
            canon("Comp./Claims Unit Ldr."),
            "compensation claims unit leader"
        );
        assert_eq!(canon("Comp/Claims Unit"), "compensation claims unit leader");
        assert!(find_type("POD").is_some_and(|t| t.id == "mass_dispensing"));
        assert!(find_type("measles_outbreak").is_some());
        assert!(find_type("nor'easter").is_some_and(|t| t.id == "coastal_storm"));
        assert!(find_type("asteroid").is_none());
    }
}
