//! LEAD-03 `assess_foundational_public_health_services`: a gap analysis against the
//! Massachusetts local public health standards (G.L. c. 111, s. 27D, the State Action for Public
//! Health Excellence law enacted in 2020) and the Foundational Public Health Services framework,
//! with shared-service options.
//!
//! Four sets of standards are rated by the nurse (the tool never fills in an answer):
//!
//! * `performance`: the mandated local responsibilities in DPH's Performance Standards workbook
//!   (snapshot `ma-performance-standards`);
//! * `workforce`: DPH's workforce standards by position and inspection type (same snapshot);
//! * `fphs`: the 56 Headline Responsibilities of the Foundational Public Health Services
//!   framework as DPH hosts them (`content/leadership/fphs_framework.yaml`);
//! * `statute`: the program and capability terms in s. 27D(a) that have no single FPHS twin.
//!
//! First call (no statuses): a self-assessment worksheet (XLSX) and guide (DOCX) in
//! /data/output. The nurse fills the Status column and calls again with `assessment_path`
//! (or types a few `statuses`): the tool returns the gap table, priority view, and shared-service
//! options and writes them to /data/output. Notes the nurse writes stay in those files.

use std::collections::{BTreeMap, BTreeSet};

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::strategy::{self, PerformanceStandards, SsaRow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::strategy_docs::{
    DocBuilder, OutFormat, clip, formats_or_default, pct, tidy, tidy_opt, write_documents,
};
use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, DocSource};
use crate::epr::roster;
use crate::md;

/// Content id of `content/leadership/fphs_framework.yaml`.
pub const FPHS_FRAMEWORK: &str = "leadership-fphs-framework";

const MAX_STATUSES: usize = 400;
const MAX_FILE_ROWS: usize = 2_000;
const PRIORITY_LIST_IN_RESPONSE: usize = 25;
const NOTE_CHARS: usize = 400;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Law {
    naming_check: String,
    provisions: Vec<Cited>,
    statute_terms: Vec<StatuteTerm>,
}

#[derive(Debug, Clone, Deserialize)]
struct StatuteTerm {
    id: String,
    kind: String,
    name: String,
    fphs_area: Option<String>,
    own_item: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct HeadlineDef {
    id: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AreaDef {
    id: String,
    kind: String,
    name: String,
    headline_responsibilities: Vec<HeadlineDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct FrameworkDef {
    community_specific_note: String,
    areas: Vec<AreaDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelDef {
    label: String,
    description: String,
    source: String,
    section: String,
    confidence: content::Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct SupportDef {
    name: String,
    description: String,
    source: String,
    section: String,
    confidence: content::Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct SharedDef {
    intro: String,
    models: Vec<ModelDef>,
    supports: Vec<SupportDef>,
    decision_questions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct IdLabelDef {
    id: String,
    label: String,
    #[serde(default)]
    definition: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PriorityRuleDef {
    tier: u8,
    label: String,
    rule: String,
    rationale: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FphsContent {
    sources: BTreeMap<String, DocSource>,
    law: Law,
    framework: FrameworkDef,
    shared_services: SharedDef,
    status_scale: Vec<IdLabelDef>,
    status_scale_note: String,
    delivered_by_options: Vec<IdLabelDef>,
    priority_rules: Vec<PriorityRuleDef>,
    priority_note: String,
    not_verified: Vec<String>,
}

// ---------------------------------------------------------------- standards catalogue

/// A set of standards.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StdSet {
    /// Mandated local responsibilities in DPH's Performance Standards workbook.
    Performance,
    /// DPH's workforce standards by position and inspection type.
    Workforce,
    /// The Foundational Public Health Services framework (Headline Responsibilities).
    Fphs,
    /// Program and capability terms in G.L. c. 111, s. 27D(a) with no single FPHS counterpart.
    Statute,
}

impl StdSet {
    const ALL: [StdSet; 4] = [
        StdSet::Performance,
        StdSet::Workforce,
        StdSet::Statute,
        StdSet::Fphs,
    ];

    fn id(self) -> &'static str {
        match self {
            StdSet::Performance => "performance",
            StdSet::Workforce => "workforce",
            StdSet::Fphs => "fphs",
            StdSet::Statute => "statute",
        }
    }

    fn label(self) -> &'static str {
        match self {
            StdSet::Performance => "Performance Standards: mandated local responsibilities",
            StdSet::Workforce => "Workforce standards",
            StdSet::Fphs => "Foundational Public Health Services framework",
            StdSet::Statute => "Program terms in G.L. c. 111, s. 27D(a)",
        }
    }
}

/// Why a standard matters, for the priority tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Basis {
    Mandate,
    Workforce,
    Statute,
    Capability,
    Area,
}

impl Basis {
    fn label(self) -> &'static str {
        match self {
            Basis::Mandate => "legal mandate (statute or regulation)",
            Basis::Workforce => "workforce standard",
            Basis::Statute => "program named in G.L. c. 111, s. 27D(a)",
            Basis::Capability => "FPHS foundational capability",
            Basis::Area => "FPHS foundational area",
        }
    }
}

#[derive(Debug, Clone)]
struct Standard {
    id: String,
    set: StdSet,
    area: String,
    title: String,
    citation: String,
    basis: Basis,
    outdated: bool,
}

fn catalogue(c: &FphsContent, p: &PerformanceStandards) -> Vec<Standard> {
    let mut v = Vec::new();
    for m in &p.mandates {
        v.push(Standard {
            id: m.id.clone(),
            set: StdSet::Performance,
            area: m.area.clone(),
            title: if m.summary.is_empty() {
                format!("See the citation ({})", m.citation)
            } else {
                m.summary.clone()
            },
            citation: m.citation.clone(),
            basis: Basis::Mandate,
            outdated: m.marked_outdated,
        });
    }
    for w in &p.workforce_positions {
        v.push(Standard {
            id: w.id.clone(),
            set: StdSet::Workforce,
            area: "Position".into(),
            title: format!(
                "{}: staff in this position meet the required credentials and training. Required at hire: {}. Required after hire: {}.",
                w.position,
                join_or_none(&w.required_at_hire),
                join_or_none(&w.required_after_hire)
            ),
            citation: "Performance Standards workbook, Workforce Standards sheet".into(),
            basis: Basis::Workforce,
            outdated: false,
        });
    }
    for w in &p.workforce_inspection_types {
        v.push(Standard {
            id: w.id.clone(),
            set: StdSet::Workforce,
            area: "Inspection type".into(),
            title: format!(
                "{}: inspectors have the required training. Required: {}.",
                w.inspection_type,
                join_or_none(&w.required)
            ),
            citation: "Performance Standards workbook, Workforce Standards sheet".into(),
            basis: Basis::Workforce,
            outdated: false,
        });
    }
    for t in c.law.statute_terms.iter().filter(|t| t.own_item) {
        v.push(Standard {
            id: t.id.clone(),
            set: StdSet::Statute,
            area: if t.kind == "program" {
                "Public health programs".into()
            } else {
                "Foundational capabilities".into()
            },
            title: format!(
                "{}: the {} named in the statute is available to residents{}",
                capitalize(&t.name),
                t.kind,
                t.fphs_area
                    .as_deref()
                    .map(|a| format!(" (related FPHS area {a})"))
                    .unwrap_or_default()
            ),
            citation: "G.L. c. 111, s. 27D(a)".into(),
            basis: Basis::Statute,
            outdated: false,
        });
    }
    for a in &c.framework.areas {
        let basis = if a.kind == "capability" {
            Basis::Capability
        } else {
            Basis::Area
        };
        for h in &a.headline_responsibilities {
            v.push(Standard {
                id: h.id.clone(),
                set: StdSet::Fphs,
                area: format!(
                    "{} {} ({})",
                    a.id,
                    a.name,
                    if a.kind == "capability" {
                        "capability"
                    } else {
                        "area"
                    }
                ),
                title: h.text.trim_end_matches('.').to_string(),
                citation: format!(
                    "FPHS Operational Definitions {}",
                    h.id.trim_start_matches("FPHS-")
                ),
                basis,
                outdated: false,
            });
        }
    }
    v
}

fn join_or_none(v: &[String]) -> String {
    if v.is_empty() {
        "none listed".into()
    } else {
        v.join("; ")
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------- statuses

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Met,
    Partial,
    NotMet,
    NotApplicable,
}

impl Status {
    fn id(self) -> &'static str {
        match self {
            Status::Met => "met",
            Status::Partial => "partial",
            Status::NotMet => "not_met",
            Status::NotApplicable => "not_applicable",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Status::Met => "Met",
            Status::Partial => "Partly met",
            Status::NotMet => "Not met",
            Status::NotApplicable => "Not applicable",
        }
    }
}

fn parse_status(s: &str) -> Option<Status> {
    let n: String = s
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    match n.as_str() {
        "met" | "yes" | "fully met" | "in place" | "complete" => Some(Status::Met),
        "partial" | "partially met" | "partly met" | "partially" | "some" | "in progress" => {
            Some(Status::Partial)
        }
        "not met" | "no" | "gap" | "not in place" | "none" => Some(Status::NotMet),
        "not applicable" | "n a" | "na" | "does not apply" => Some(Status::NotApplicable),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Delivery {
    Own,
    Shared,
    Contract,
    State,
    NotDelivered,
}

impl Delivery {
    fn id(self) -> &'static str {
        match self {
            Delivery::Own => "own",
            Delivery::Shared => "shared",
            Delivery::Contract => "contract",
            Delivery::State => "state",
            Delivery::NotDelivered => "not_delivered",
        }
    }
}

fn parse_delivery(s: &str) -> Option<Delivery> {
    let n = s.trim().to_lowercase().replace(['_', '-'], " ");
    match n.as_str() {
        "own" | "own department" | "in house" | "us" => Some(Delivery::Own),
        "shared" | "shared service" | "shared services" | "district" | "ssa" => {
            Some(Delivery::Shared)
        }
        "contract" | "contracted" | "agreement" => Some(Delivery::Contract),
        "state" | "state program" => Some(Delivery::State),
        "not delivered" | "none" | "nobody" => Some(Delivery::NotDelivered),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserPriority {
    High,
    Medium,
    Low,
}

fn parse_priority(s: &str) -> Option<UserPriority> {
    match s.trim().to_lowercase().as_str() {
        "high" | "h" | "1" => Some(UserPriority::High),
        "medium" | "med" | "m" | "2" => Some(UserPriority::Medium),
        "low" | "l" | "3" => Some(UserPriority::Low),
        _ => None,
    }
}

impl UserPriority {
    fn tier(self) -> u8 {
        match self {
            UserPriority::High => 1,
            UserPriority::Medium => 2,
            UserPriority::Low => 3,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Rating {
    status: Option<Status>,
    delivery: Option<Delivery>,
    priority: Option<UserPriority>,
    note: String,
}

/// Priority tier of a gap (1 = address first), from the rules in the content file.
fn tier(basis: Basis, status: Status) -> Option<u8> {
    match (status, basis) {
        (Status::NotMet, Basis::Mandate) => Some(1),
        (Status::NotMet, Basis::Workforce | Basis::Statute | Basis::Capability) => Some(2),
        (Status::NotMet, Basis::Area) => Some(3),
        (Status::Partial, Basis::Mandate) => Some(2),
        (Status::Partial, _) => Some(3),
        _ => None,
    }
}

// ---------------------------------------------------------------- arguments and results

/// One status the nurse enters directly.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StatusEntry {
    /// The standard's ID from the worksheet (PS-ENV-012, WF-POS-05, FPHS-01.03, STAT-10).
    pub standard_id: String,
    /// "met", "partial", "not_met", or "not_applicable" (also "yes", "no", "partly met", "n/a").
    pub status: String,
    /// Who delivers it: "own", "shared", "contract", "state", or "not_delivered".
    pub delivered_by: Option<String>,
    /// Your own priority: "high", "medium", or "low". It replaces the tool's tier for that item.
    pub priority: Option<String>,
    /// A note for the output files (not repeated in the response).
    pub note: Option<String>,
}

/// Arguments for `assess_foundational_public_health_services`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct FphsArgs {
    /// Your city or town, to show the shared service arrangement DPH lists for it. Default:
    /// your saved town (nothing is looked up when none is saved).
    pub town: Option<String>,
    /// Which standards to include: "performance" (mandated local responsibilities), "workforce",
    /// "statute" (program terms in G.L. c. 111, s. 27D(a)), "fphs" (Foundational Public Health
    /// Services). Default: all four.
    pub sets: Vec<StdSet>,
    /// Statuses you record yourself, one per standard. The tool never invents a status.
    pub statuses: Vec<StatusEntry>,
    /// A CSV/XLSX in /data: the self-assessment worksheet this tool wrote, with its Status (and
    /// optionally Delivered by, Priority, Notes) columns filled in. Only the ID, Status,
    /// Delivered by, Priority, and Notes columns are read; nothing from it is returned except
    /// counts and the public standard IDs.
    pub assessment_path: Option<String>,
    /// Sheet name when the worksheet is a workbook (default: the first sheet, "Self-assessment").
    pub assessment_sheet: Option<String>,
    /// Write the worksheet or gap analysis to /data/output (default true).
    pub write_documents: Option<bool>,
    /// "docx" and/or "xlsx" (default both).
    pub formats: Vec<OutFormat>,
}

/// Counts by status.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatusCounts {
    pub standards: usize,
    pub met: usize,
    pub partial: usize,
    pub not_met: usize,
    pub not_applicable: usize,
    pub not_rated: usize,
}

impl StatusCounts {
    fn add(&mut self, st: Option<Status>) {
        self.standards += 1;
        match st {
            Some(Status::Met) => self.met += 1,
            Some(Status::Partial) => self.partial += 1,
            Some(Status::NotMet) => self.not_met += 1,
            Some(Status::NotApplicable) => self.not_applicable += 1,
            None => self.not_rated += 1,
        }
    }

    fn rated_applicable(&self) -> usize {
        self.met + self.partial + self.not_met
    }
}

/// One set's summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetSummary {
    pub set: StdSet,
    pub label: String,
    #[serde(flatten)]
    pub counts: StatusCounts,
    /// Share of rated, applicable standards that are met (blank when none are rated).
    pub percent_met_of_rated: Option<f64>,
}

/// One area's summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AreaSummary {
    pub set: StdSet,
    pub area: String,
    #[serde(flatten)]
    pub counts: StatusCounts,
}

/// A gap in the priority view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Gap {
    pub id: String,
    pub set: StdSet,
    pub area: String,
    pub standard: String,
    pub citation: String,
    pub basis: String,
    pub status: String,
    /// 1 = address first, 2 = address next, 3 = plan for.
    pub tier: u8,
    /// "tool rule" or "your priority".
    pub tier_from: String,
    pub delivered_by: Option<String>,
    /// The workbook marks the standard * (OLRH has comments it is outdated).
    pub marked_outdated: bool,
}

/// A count by label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CountOut {
    pub label: String,
    pub count: usize,
}

/// A shared service arrangement DPH lists for a municipality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArrangementOut {
    pub town: String,
    pub arrangement: String,
    pub lead_entity: String,
    pub training_hub: String,
    pub member_count: usize,
    pub members: Vec<String>,
}

/// Shared service options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SharedOut {
    pub intro: String,
    pub town: Option<String>,
    /// DPH's list entry for the town (none when the list shows no arrangement or no town was given).
    pub arrangement: Option<ArrangementOut>,
    pub arrangement_note: String,
    pub models: Vec<CitedOut>,
    pub supports: Vec<CitedOut>,
    pub decision_questions: Vec<String>,
}

/// What was read from the worksheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssessmentFile {
    pub path: String,
    pub rows_read: usize,
    pub rows_with_status: usize,
    pub unrecognized_id_rows: usize,
    pub unrecognized_status_rows: usize,
    pub duplicate_rows: usize,
}

/// Files written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FphsFiles {
    pub folder: String,
    pub files: Vec<String>,
}

/// Result of `assess_foundational_public_health_services`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FphsResult {
    /// "worksheet" (no statuses yet) or "gap_analysis".
    pub mode: String,
    pub standards_in_scope: usize,
    pub sets: Vec<SetSummary>,
    pub areas: Vec<AreaSummary>,
    pub gap_counts_by_tier: Vec<CountOut>,
    pub total_gaps: usize,
    /// The highest-priority gaps (the files hold all of them).
    pub priority_gaps: Vec<Gap>,
    pub delivered_by: Vec<CountOut>,
    pub shared_services: SharedOut,
    pub assessment_file: Option<AssessmentFile>,
    pub status_scale: Vec<String>,
    pub priority_rules: Vec<String>,
    pub law: Vec<CitedOut>,
    pub naming_check: String,
    pub files: Option<FphsFiles>,
}

// ---------------------------------------------------------------- reading the worksheet

fn header_is(h: &str, names: &[&str]) -> bool {
    let n = h.trim().to_lowercase().replace(['_', '-'], " ");
    names.iter().any(|x| n == *x)
}

fn read_worksheet(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    known: &BTreeSet<String>,
) -> Result<(BTreeMap<String, Rating>, AssessmentFile, Vec<Source>)> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let t = &lt.table;
    let find = |names: &[&str]| t.headers.iter().position(|h| header_is(h, names));
    let (Some(id_col), Some(status_col)) = (find(&["id", "standard id"]), find(&["status"])) else {
        return Err(PhnError::InvalidInput(
            "that file is not the self-assessment worksheet: it needs a column named ID (the \
             standard's ID, such as PS-ENV-012 or FPHS-01.03) and a column named Status. Run this \
             tool without statuses to write the worksheet, fill in its Status column, then give \
             the file here."
                .into(),
        ));
    };
    let delivery_col = find(&["delivered by", "delivery"]);
    let priority_col = find(&["priority", "your priority"]);
    let note_col = find(&["notes", "note", "evidence"]);
    // Identifier columns among those that are read are refused (a client file is not a worksheet).
    // The ID column of the worksheet holds only public standard IDs, which the detector may
    // mistake for record numbers; a column with anything else in it is checked as usual.
    let id_values_are_standard_ids = t
        .rows
        .iter()
        .filter_map(|r| r.get(id_col))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .all(|v| {
            known.contains(v)
                || ["PS-", "WF-", "FPHS-", "STAT-"]
                    .iter()
                    .any(|p| v.starts_with(p))
        });
    for idx in [Some(id_col), Some(status_col), delivery_col, priority_col]
        .into_iter()
        .flatten()
    {
        if idx == id_col && id_values_are_standard_ids {
            continue;
        }
        if let Some(f) = lt.findings.iter().find(|f| f.index == idx) {
            return Err(PhnError::Privacy(format!(
                "column '{}' appears to contain {}: this does not look like the self-assessment \
                 worksheet, so the file was not used",
                f.column,
                f.kind.label()
            )));
        }
    }
    if t.rows.len() > MAX_FILE_ROWS {
        return Err(PhnError::InvalidInput(format!(
            "the file has {} rows; the worksheet has about 170",
            t.rows.len()
        )));
    }
    let mut out: BTreeMap<String, Rating> = BTreeMap::new();
    let mut f = AssessmentFile {
        path: lt.label.clone(),
        rows_read: 0,
        rows_with_status: 0,
        unrecognized_id_rows: 0,
        unrecognized_status_rows: 0,
        duplicate_rows: 0,
    };
    for row in &t.rows {
        let cell = |i: usize| row.get(i).map(|s| s.trim()).unwrap_or("");
        let id = cell(id_col);
        if id.is_empty() {
            continue;
        }
        f.rows_read += 1;
        if !known.contains(id) {
            f.unrecognized_id_rows += 1;
            continue;
        }
        let status_text = cell(status_col);
        if status_text.is_empty() {
            continue; // not yet rated
        }
        let Some(status) = parse_status(status_text) else {
            f.unrecognized_status_rows += 1;
            continue;
        };
        if out.contains_key(id) {
            f.duplicate_rows += 1;
            continue;
        }
        f.rows_with_status += 1;
        out.insert(
            id.to_string(),
            Rating {
                status: Some(status),
                delivery: delivery_col.and_then(|i| parse_delivery(cell(i))),
                priority: priority_col.and_then(|i| parse_priority(cell(i))),
                note: note_col
                    .map(|i| clip(&tidy(cell(i)), NOTE_CHARS))
                    .unwrap_or_default(),
            },
        );
    }
    let src = lt.source();
    Ok((out, f, vec![src]))
}

// ---------------------------------------------------------------- the tool

/// Run the gap analysis (or write the worksheet when no statuses are given).
pub fn assess_foundational_public_health_services(
    ctx: &Context,
    args: FphsArgs,
) -> Result<Envelope<FphsResult>> {
    let c: FphsContent = content::load(FPHS_FRAMEWORK)?;
    let (perf, perf_entry) = strategy::load_performance(&ctx.snapshots)?;
    let (ssa, ssa_entry) = strategy::load_ssa(&ctx.snapshots)?;

    // ---- scope
    let sets: Vec<StdSet> = if args.sets.is_empty() {
        StdSet::ALL.to_vec()
    } else {
        let mut v = args.sets.clone();
        v.sort();
        v.dedup();
        v
    };
    let all = catalogue(&c, &perf);
    let in_scope: Vec<&Standard> = all.iter().filter(|s| sets.contains(&s.set)).collect();
    let known: BTreeSet<String> = in_scope.iter().map(|s| s.id.clone()).collect();

    // ---- ratings: worksheet file first, typed statuses override
    let mut ratings: BTreeMap<String, Rating> = BTreeMap::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut assessment_file = None;
    if let Some(p) = tidy_opt(&args.assessment_path) {
        let (r, f, s) =
            read_worksheet(ctx, &p, tidy_opt(&args.assessment_sheet).as_deref(), &known)?;
        ratings = r;
        assessment_file = Some(f);
        sources.extend(s);
    }
    if args.statuses.len() > MAX_STATUSES {
        return Err(PhnError::InvalidInput(format!(
            "{} statuses given; give at most {MAX_STATUSES} (or use assessment_path)",
            args.statuses.len()
        )));
    }
    let mut bad_ids = Vec::new();
    let mut bad_status = Vec::new();
    for e in &args.statuses {
        let id = tidy(&e.standard_id);
        if !known.contains(&id) {
            bad_ids.push(clip(&id, 40));
            continue;
        }
        let Some(status) = parse_status(&e.status) else {
            bad_status.push(id);
            continue;
        };
        ratings.insert(
            id,
            Rating {
                status: Some(status),
                delivery: e.delivered_by.as_deref().and_then(parse_delivery),
                priority: e.priority.as_deref().and_then(parse_priority),
                note: e
                    .note
                    .as_deref()
                    .map(|n| clip(&tidy(n), NOTE_CHARS))
                    .unwrap_or_default(),
            },
        );
    }
    if !bad_ids.is_empty() {
        bad_ids.truncate(10);
        return Err(PhnError::InvalidInput(format!(
            "not a standard ID in the chosen sets: {}. IDs look like PS-ENV-012, WF-POS-05, \
             FPHS-01.03, or STAT-10; run the tool without statuses to see the worksheet.",
            bad_ids.join(", ")
        )));
    }
    if !bad_status.is_empty() {
        bad_status.truncate(10);
        return Err(PhnError::InvalidInput(format!(
            "unrecognized status for {}. Use met, partial, not_met, or not_applicable.",
            bad_status.join(", ")
        )));
    }
    if let Some(f) = &assessment_file
        && f.unrecognized_status_rows > 0
    {
        return Err(PhnError::InvalidInput(format!(
            "{} row(s) of the worksheet have a Status the tool does not recognize. Use met, \
             partial, not_met, or not_applicable (also yes, no, partly met, n/a).",
            f.unrecognized_status_rows
        )));
    }
    let mode = if ratings.is_empty() {
        "worksheet"
    } else {
        "gap_analysis"
    };

    // ---- town and shared services
    let town_query = tidy_opt(&args.town).or_else(|| ctx.settings().jurisdiction.town);
    let mut town_name = None;
    if let Some(q) = &town_query {
        match ctx.ma.town(q) {
            Some(m) => town_name = Some(m.name.clone()),
            None => {
                let sug: Vec<String> = ctx
                    .ma
                    .suggest_towns(q, 3)
                    .into_iter()
                    .map(|m| m.name.clone())
                    .collect();
                return Err(PhnError::NotFound(format!(
                    "'{}' is not a Massachusetts city or town{}",
                    clip(q, 60),
                    if sug.is_empty() {
                        String::new()
                    } else {
                        format!(". Did you mean {}?", sug.join(", "))
                    }
                )));
            }
        }
    }
    let shared = shared_out(&c, &ssa, town_name.as_deref());

    // ---- analysis
    let mut set_counts: BTreeMap<StdSet, StatusCounts> = BTreeMap::new();
    let mut area_counts: BTreeMap<(StdSet, String), StatusCounts> = BTreeMap::new();
    let mut gaps: Vec<Gap> = Vec::new();
    let mut delivery_counts: BTreeMap<Delivery, usize> = BTreeMap::new();
    for s in &in_scope {
        let r = ratings.get(&s.id);
        let st = r.and_then(|r| r.status);
        set_counts.entry(s.set).or_default().add(st);
        area_counts
            .entry((s.set, s.area.clone()))
            .or_default()
            .add(st);
        if let Some(d) = r.and_then(|r| r.delivery) {
            *delivery_counts.entry(d).or_default() += 1;
        }
        if let (Some(status), Some(r)) = (st, r)
            && let Some(auto) = tier(s.basis, status)
        {
            let (t, from) = match r.priority {
                Some(p) => (p.tier(), "your priority"),
                None => (auto, "tool rule"),
            };
            gaps.push(Gap {
                id: s.id.clone(),
                set: s.set,
                area: s.area.clone(),
                standard: clip(&s.title, 300),
                citation: s.citation.clone(),
                basis: s.basis.label().to_string(),
                status: status.label().to_string(),
                tier: t,
                tier_from: from.to_string(),
                delivered_by: r.delivery.map(|d| d.id().to_string()),
                marked_outdated: s.outdated,
            });
        }
    }
    gaps.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(a.set.cmp(&b.set))
            .then(a.id.cmp(&b.id))
    });
    let set_summaries: Vec<SetSummary> = sets
        .iter()
        .map(|s| {
            let counts = set_counts.get(s).cloned().unwrap_or_default();
            SetSummary {
                set: *s,
                label: s.label().to_string(),
                percent_met_of_rated: pct(counts.met, counts.rated_applicable()),
                counts,
            }
        })
        .collect();
    let area_summaries: Vec<AreaSummary> = area_counts
        .into_iter()
        .map(|((set, area), counts)| AreaSummary { set, area, counts })
        .collect();
    let tier_counts: Vec<CountOut> = (1..=3u8)
        .map(|t| CountOut {
            label: c
                .priority_rules
                .iter()
                .find(|r| r.tier == t)
                .map(|r| format!("Tier {t}: {}", r.label))
                .unwrap_or_else(|| format!("Tier {t}")),
            count: gaps.iter().filter(|g| g.tier == t).count(),
        })
        .collect();
    let delivered: Vec<CountOut> = delivery_counts
        .iter()
        .map(|(d, n)| CountOut {
            label: c
                .delivered_by_options
                .iter()
                .find(|o| o.id == d.id())
                .map(|o| o.label.clone())
                .unwrap_or_else(|| d.id().to_string()),
            count: *n,
        })
        .collect();

    // ---- documents
    let write = args.write_documents.unwrap_or(true);
    let formats = formats_or_default(&args.formats);
    let mut files = None;
    let mut files_written = Vec::new();
    if write {
        let doc = build_document(
            ctx,
            &c,
            &perf_entry.retrieved,
            mode,
            &in_scope,
            &ratings,
            &set_summaries,
            &gaps,
            &shared,
            &area_summaries,
        );
        let (folder, written) = write_documents(
            ctx,
            "fphs_assessment",
            if mode == "worksheet" {
                "fphs_self_assessment"
            } else {
                "fphs_gap_analysis"
            },
            &doc,
            &formats,
            Some("Self-assessment"),
        )?;
        files_written.extend(written.clone());
        files = Some(FphsFiles {
            folder,
            files: written,
        });
    }

    // ---- response
    let mut s = String::new();
    if mode == "worksheet" {
        s.push_str(&format!(
            "**Self-assessment worksheet: {} standards in {} set(s).** No statuses were given, so \
             nothing is rated. The tool never fills in an answer: rate each standard yourself \
             (met, partial, not_met, or not_applicable) in the worksheet's Status column, then run \
             this tool again with assessment_path.\n\n",
            in_scope.len(),
            sets.len()
        ));
        let rows: Vec<Vec<String>> = set_summaries
            .iter()
            .map(|x| {
                vec![
                    x.label.clone(),
                    x.counts.standards.to_string(),
                    source_label(x.set, &c, &perf_entry.retrieved),
                ]
            })
            .collect();
        s.push_str(&md::table(&["Set", "Standards", "Source"], &rows));
    } else {
        s.push_str(&format!(
            "**Gap analysis: {} rated of {} standards; {} gap(s).**\n\n",
            ratings.len(),
            in_scope.len(),
            gaps.len()
        ));
        let rows: Vec<Vec<String>> = set_summaries
            .iter()
            .map(|x| {
                vec![
                    x.label.clone(),
                    x.counts.met.to_string(),
                    x.counts.partial.to_string(),
                    x.counts.not_met.to_string(),
                    x.counts.not_applicable.to_string(),
                    x.counts.not_rated.to_string(),
                    x.percent_met_of_rated
                        .map(|p| format!("{p}%"))
                        .unwrap_or_else(|| "-".into()),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Set",
                "Met",
                "Partly met",
                "Not met",
                "Not applicable",
                "Not rated",
                "% met of rated",
            ],
            &rows,
        ));
        s.push_str("\n**Priority view**\n\n");
        for t in &tier_counts {
            s.push_str(&format!("- {}: {}\n", t.label, t.count));
        }
        if !gaps.is_empty() {
            s.push_str("\nFirst gaps (all are in the files):\n\n");
            for g in gaps.iter().take(8) {
                s.push_str(&format!(
                    "- **{}** ({}; tier {}): {}\n",
                    g.id,
                    g.status.to_lowercase(),
                    g.tier,
                    clip(&g.standard, 140)
                ));
            }
        }
    }
    s.push_str(&format!(
        "\n**Shared services.** {}\n",
        shared.arrangement_note
    ));
    s.push_str(&format!(
        "\nBoards of health report compliance to DPH each year by August 31 (G.L. c. 111, s. 27D(d)(2)). The law is the 2020 SAPHE law, G.L. c. 111, s. 27D: {}\n",
        clip(&tidy(&c.law.naming_check), 260)
    ));
    if let Some(f) = &files {
        s.push_str(&format!("\nFiles written to {}.\n", f.folder));
    }

    // ---- caveats and privacy
    let texts = roster::argument_strings(
        &args,
        &[
            "assessment_path",
            "assessment_sheet",
            "write_documents",
            "formats",
            "town",
            "sets",
        ],
    );
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut caveats = vec![
        "The tool records and totals what you tell it; every status is yours. Ratings, priorities, and shared-service ideas are decision support, not a compliance finding, and DPH has not scored this worksheet.".to_string(),
        format!(
            "Performance Standards snapshot: DPH's workbook (published 2023-10-11, last modified per the file {}), read {}. DPH says the next version will include the FPHS framework; the citation, not DPH's one-line summary, controls what a standard requires.",
            perf.source_last_modified, perf_entry.retrieved
        ),
        c.priority_note.clone(),
        c.status_scale_note.clone(),
        c.framework.community_specific_note.clone(),
    ];
    if perf.mandates.iter().any(|m| m.marked_outdated) {
        caveats.push("The workbook marks some standards * (OLRH has received comments that they are outdated); they are flagged in the files.".into());
    }
    if let Some(f) = &assessment_file
        && (f.unrecognized_id_rows > 0 || f.duplicate_rows > 0)
    {
        caveats.push(format!(
            "Worksheet rows skipped: {} with an ID the tool does not know and {} repeated IDs (the first row was used).",
            f.unrecognized_id_rows, f.duplicate_rows
        ));
    }
    if !found.is_empty() {
        caveats.push(format!(
            "Your inputs appear to contain {}. A department assessment describes services, not people: remove personal details before sharing the files.",
            found.join(", ").to_lowercase()
        ));
    }
    caveats.extend(content::review_caveats(&[FPHS_FRAMEWORK]));
    caveats.extend(c.not_verified.iter().map(|n| format!("Not verified: {n}")));

    let mut keys: Vec<&str> = vec![
        "statute",
        "dph-fphs",
        "fphs-opdefs",
        "dph-performance-standards",
        "dph-phe-grants",
        "dph-saphe-archive",
        "dph-press-2023",
    ];
    if sets.contains(&StdSet::Workforce) {
        keys.push("dph-workforce");
    }
    let mut sources_out = content::sources_for(keys, &c.sources);
    sources_out.push(perf_entry.to_source());
    sources_out.push(ssa_entry.to_source());
    sources_out.extend(sources);
    let notes = if assessment_file.is_some() {
        vec!["Only the ID, Status, Delivered by, Priority, and Notes columns of your worksheet were read; the response has counts and public standard IDs. Notes are in the output files only.".into()]
    } else {
        Vec::new()
    };
    let law: Vec<CitedOut> = content::cite_all(&c.law.provisions, &c.sources);
    let status_scale: Vec<String> = c
        .status_scale
        .iter()
        .map(|x| format!("{}: {}", x.label, x.definition))
        .collect();
    let priority_rules: Vec<String> = c
        .priority_rules
        .iter()
        .map(|r| format!("Tier {} ({}): {} {}", r.tier, r.label, r.rule, r.rationale))
        .collect();
    let top: Vec<Gap> = gaps
        .iter()
        .take(PRIORITY_LIST_IN_RESPONSE)
        .cloned()
        .collect();
    Ok(Envelope::new(
        s,
        FphsResult {
            mode: mode.to_string(),
            standards_in_scope: in_scope.len(),
            sets: set_summaries,
            areas: area_summaries,
            gap_counts_by_tier: tier_counts,
            total_gaps: gaps.len(),
            priority_gaps: top,
            delivered_by: delivered,
            shared_services: shared,
            assessment_file,
            status_scale,
            priority_rules,
            law,
            naming_check: tidy(&c.law.naming_check),
            files,
        },
    )
    .sources(sources_out)
    .caveats(caveats)
    .decision_support()
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: found,
        files_written,
        notes,
        ..Privacy::none()
    }))
}

fn source_label(set: StdSet, c: &FphsContent, perf_retrieved: &str) -> String {
    match set {
        StdSet::Performance | StdSet::Workforce => {
            format!("DPH Performance Standards workbook (snapshot read {perf_retrieved})")
        }
        StdSet::Fphs => c
            .sources
            .get("fphs-opdefs")
            .map(|s| s.title.clone())
            .unwrap_or_default(),
        StdSet::Statute => "G.L. c. 111, s. 27D(a)".into(),
    }
}

fn shared_out(c: &FphsContent, ssa: &[SsaRow], town: Option<&str>) -> SharedOut {
    let (arrangement, note) = match town {
        None => (
            None,
            "No town was given and none is saved, so DPH's shared service arrangement list was not checked. Give a town or save yours with configure_jurisdiction.".to_string(),
        ),
        Some(t) => match ssa.iter().find(|r| r.municipality.eq_ignore_ascii_case(t)) {
            Some(r) if !r.arrangement.is_empty() => {
                let members: Vec<String> = ssa
                    .iter()
                    .filter(|x| x.arrangement == r.arrangement)
                    .map(|x| x.municipality.clone())
                    .collect();
                let note = format!(
                    "DPH's list shows {t} in the {} (lead: {}{}), with {} municipalities. Ask the arrangement which gaps it can cover before planning to fill them yourself.",
                    r.arrangement,
                    if r.lead_entity.is_empty() { "not listed" } else { &r.lead_entity },
                    if r.training_hub.is_empty() { String::new() } else { format!("; training hub: {}", r.training_hub) },
                    members.len()
                );
                (
                    Some(ArrangementOut {
                        town: t.to_string(),
                        arrangement: r.arrangement.clone(),
                        lead_entity: r.lead_entity.clone(),
                        training_hub: r.training_hub.clone(),
                        member_count: members.len(),
                        members,
                    }),
                    note,
                )
            }
            Some(r) => (
                None,
                format!(
                    "DPH's list shows no shared service arrangement for {t}{}. It may be a standalone grantee or not funded; confirm with the Office of Local and Regional Health.",
                    if r.training_hub.is_empty() { String::new() } else { format!(" (training hub: {})", r.training_hub) }
                ),
            ),
            None => (
                None,
                format!("{t} is not on DPH's shared service arrangement list; confirm with the Office of Local and Regional Health."),
            ),
        },
    };
    let cite = |source: &str, section: &str| {
        format!(
            "{}, {section}",
            c.sources
                .get(source)
                .map(|s| s.title.as_str())
                .unwrap_or(source)
        )
    };
    SharedOut {
        intro: tidy(&c.shared_services.intro),
        town: town.map(str::to_string),
        arrangement,
        arrangement_note: note,
        models: c
            .shared_services
            .models
            .iter()
            .map(|m| CitedOut {
                text: format!("{}: {}", m.label, tidy(&m.description)),
                citation: cite(&m.source, &m.section),
                confidence: m.confidence,
            })
            .collect(),
        supports: c
            .shared_services
            .supports
            .iter()
            .map(|m| CitedOut {
                text: format!("{}: {}", m.name, tidy(&m.description)),
                citation: cite(&m.source, &m.section),
                confidence: m.confidence,
            })
            .collect(),
        decision_questions: c.shared_services.decision_questions.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_document(
    ctx: &Context,
    c: &FphsContent,
    perf_retrieved: &str,
    mode: &str,
    in_scope: &[&Standard],
    ratings: &BTreeMap<String, Rating>,
    sets: &[SetSummary],
    gaps: &[Gap],
    shared: &SharedOut,
    areas: &[AreaSummary],
) -> phn_docs::model::Document {
    let mut b = DocBuilder::new();
    b.heading(1, "About this document");
    b.para(
        "A self-assessment of local public health standards in Massachusetts: the mandated local \
         responsibilities in DPH's Performance Standards, DPH's workforce standards, the \
         Foundational Public Health Services (FPHS) framework, and program terms named in G.L. \
         c. 111, s. 27D(a). Every status in this document was entered by you; the tool only \
         organizes it.",
    );
    b.labeled("Law:", tidy(&c.law.naming_check));
    b.labeled(
        "Reporting:",
        "Boards of health report compliance with the standards to DPH each year by August 31 (G.L. c. 111, s. 27D(d)(2)).",
    );
    b.labeled(
        "Snapshot:",
        format!("The Performance Standards come from DPH's workbook, read {perf_retrieved}. Status: draft, not reviewed by the subject matter expert."),
    );
    b.heading(1, "How to rate each standard");
    let scale: Vec<String> = c
        .status_scale
        .iter()
        .map(|x| format!("{} ({}): {}", x.label, x.id, x.definition))
        .collect();
    b.list(&scale);
    b.para(c.status_scale_note.trim());
    let delivery: Vec<String> = c
        .delivered_by_options
        .iter()
        .map(|x| format!("{} ({})", x.label, x.id))
        .collect();
    b.labeled("Delivered by (optional):", delivery.join("; "));
    b.labeled(
        "Priority (optional):",
        "high, medium, or low; your priority replaces the tool's tier for that item.",
    );
    b.labeled(
        "Using the worksheet:",
        "Fill the Status column of the Self-assessment table (the first worksheet of the workbook), save it as CSV or XLSX in your data folder, and give it to the tool as assessment_path.",
    );
    if mode == "gap_analysis" {
        b.heading(1, "Results by set");
        let srows: Vec<Vec<String>> = sets
            .iter()
            .map(|x| {
                vec![
                    x.label.clone(),
                    x.counts.standards.to_string(),
                    x.counts.met.to_string(),
                    x.counts.partial.to_string(),
                    x.counts.not_met.to_string(),
                    x.counts.not_applicable.to_string(),
                    x.counts.not_rated.to_string(),
                    x.percent_met_of_rated
                        .map(|p| format!("{p}%"))
                        .unwrap_or_default(),
                ]
            })
            .collect();
        b.table(
            "Results by set",
            &[
                "Set",
                "Standards",
                "Met",
                "Partly met",
                "Not met",
                "Not applicable",
                "Not rated",
                "% met of rated",
            ],
            srows,
        );
        b.heading(1, "Priority view");
        let rules: Vec<String> = c
            .priority_rules
            .iter()
            .map(|r| format!("Tier {} ({}): {} {}", r.tier, r.label, r.rule, r.rationale))
            .collect();
        b.list(&rules);
        b.para(c.priority_note.trim());
        let grows: Vec<Vec<String>> = gaps
            .iter()
            .map(|g| {
                vec![
                    g.tier.to_string(),
                    g.id.clone(),
                    g.status.clone(),
                    g.area.clone(),
                    g.standard.clone(),
                    g.citation.clone(),
                    g.basis.clone(),
                    g.delivered_by.clone().unwrap_or_default(),
                    g.tier_from.clone(),
                    ratings
                        .get(&g.id)
                        .map(|r| r.note.clone())
                        .unwrap_or_default(),
                ]
            })
            .collect();
        b.table(
            "Priority view",
            &[
                "Tier",
                "ID",
                "Status",
                "Area",
                "Standard",
                "Citation",
                "Why it matters",
                "Delivered by",
                "Tier from",
                "Notes",
            ],
            grows,
        );
        b.heading(1, "Results by area");
        let arows: Vec<Vec<String>> = areas
            .iter()
            .map(|a| {
                vec![
                    a.set.id().to_string(),
                    a.area.clone(),
                    a.counts.standards.to_string(),
                    a.counts.met.to_string(),
                    a.counts.partial.to_string(),
                    a.counts.not_met.to_string(),
                    a.counts.not_applicable.to_string(),
                    a.counts.not_rated.to_string(),
                ]
            })
            .collect();
        b.table(
            "Results by area",
            &[
                "Set",
                "Area",
                "Standards",
                "Met",
                "Partly met",
                "Not met",
                "Not applicable",
                "Not rated",
            ],
            arows,
        );
    }
    b.heading(1, "Shared service options");
    b.para(shared.intro.clone());
    b.labeled("Your arrangement:", shared.arrangement_note.clone());
    if let Some(a) = &shared.arrangement {
        b.labeled("Member municipalities:", a.members.join(", "));
    }
    let models: Vec<String> = shared
        .models
        .iter()
        .map(|m| format!("{} ({}, {})", m.text, m.citation, conf_label(m.confidence)))
        .collect();
    b.list(&models);
    b.heading(2, "Support available");
    let supports: Vec<String> = shared
        .supports
        .iter()
        .map(|m| format!("{} ({})", m.text, m.citation))
        .collect();
    b.list(&supports);
    b.heading(2, "Questions to decide who delivers a service");
    b.list(&shared.decision_questions);
    b.heading(1, "Method and limits");
    b.list(&[
        "Statuses, priorities, and delivery choices are yours. The tool does not score compliance.",
        "Tiers follow the rules above and are a starting point for discussion, not DPH's ranking.",
        "The citation in the workbook controls what a mandate requires; DPH's one-line summary is shortened.",
        "FPHS items are Headline Responsibilities from DPH's copy of the FPHS operational definitions; the framework is not yet part of the Performance Standards.",
    ]);
    b.heading(1, "Appendix: every standard and its status");
    let rows: Vec<Vec<String>> = in_scope
        .iter()
        .map(|s| {
            let r = ratings.get(&s.id);
            vec![
                s.id.clone(),
                s.set.id().to_string(),
                s.area.clone(),
                format!(
                    "{}{}",
                    clip(&s.title, 400),
                    if s.outdated {
                        " (marked * outdated in the workbook)"
                    } else {
                        ""
                    }
                ),
                s.citation.clone(),
                r.and_then(|r| r.status)
                    .map(|x| x.id().to_string())
                    .unwrap_or_default(),
                r.and_then(|r| r.delivery)
                    .map(|x| x.id().to_string())
                    .unwrap_or_default(),
                r.and_then(|r| r.priority)
                    .map(|p| match p {
                        UserPriority::High => "high",
                        UserPriority::Medium => "medium",
                        UserPriority::Low => "low",
                    })
                    .unwrap_or_default()
                    .to_string(),
                r.map(|r| r.note.clone()).unwrap_or_default(),
            ]
        })
        .collect();
    b.table(
        "Self-assessment",
        &[
            "ID",
            "Set",
            "Area",
            "Standard",
            "Source",
            "Status",
            "Delivered by",
            "Priority",
            "Notes",
        ],
        rows,
    );
    b.finish(
        ctx,
        if mode == "worksheet" {
            "Self-assessment worksheet: local public health standards"
        } else {
            "Gap analysis: local public health standards"
        },
        Some("Massachusetts, G.L. c. 111, s. 27D; draft for the nurse's review".into()),
    )
}

fn conf_label(c: content::Confidence) -> &'static str {
    match c {
        content::Confidence::High => "high confidence",
        content::Confidence::Medium => "medium confidence",
        content::Confidence::Low => "low confidence",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join("data").join(name), text).unwrap();
        }
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn catalogue_has_every_standard_once() {
        let c: FphsContent = content::load(FPHS_FRAMEWORK).unwrap();
        let (p, _) = strategy::load_performance(&phn_data::SnapshotStore::bundled()).unwrap();
        let all = catalogue(&c, &p);
        // 93 mandates + 13 workforce + 4 statute-only terms + 56 FPHS headline responsibilities.
        assert_eq!(all.len(), 93 + 13 + 4 + 56);
        let ids: BTreeSet<&str> = all.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), all.len());
        assert!(ids.contains("FPHS-01.03"));
        assert!(ids.contains("STAT-10"));
        assert!(ids.contains("WF-POS-05"));
    }

    #[test]
    fn statuses_and_priorities_parse_leniently() {
        assert_eq!(parse_status("Partly met"), Some(Status::Partial));
        assert_eq!(parse_status(" NOT_MET "), Some(Status::NotMet));
        assert_eq!(parse_status("n/a"), Some(Status::NotApplicable));
        assert_eq!(parse_status("yes"), Some(Status::Met));
        assert_eq!(parse_status("maybe"), None);
        assert_eq!(parse_priority("High"), Some(UserPriority::High));
        assert_eq!(parse_delivery("Shared services"), Some(Delivery::Shared));
    }

    #[test]
    fn tiers_follow_the_rules() {
        assert_eq!(tier(Basis::Mandate, Status::NotMet), Some(1));
        assert_eq!(tier(Basis::Mandate, Status::Partial), Some(2));
        assert_eq!(tier(Basis::Capability, Status::NotMet), Some(2));
        assert_eq!(tier(Basis::Area, Status::NotMet), Some(3));
        assert_eq!(tier(Basis::Workforce, Status::Partial), Some(3));
        assert_eq!(tier(Basis::Mandate, Status::Met), None);
        assert_eq!(tier(Basis::Mandate, Status::NotApplicable), None);
    }

    #[test]
    fn worksheet_mode_rates_nothing_and_writes_files() {
        let (dir, ctx) = ctx_with(&[]);
        let r = assess_foundational_public_health_services(
            &ctx,
            FphsArgs {
                town: Some("Amherst".into()),
                ..FphsArgs::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.mode, "worksheet");
        assert_eq!(r.data.standards_in_scope, 166);
        assert_eq!(r.data.total_gaps, 0);
        assert!(
            r.data
                .sets
                .iter()
                .all(|s| s.counts.not_rated == s.counts.standards)
        );
        let a = r.data.shared_services.arrangement.as_ref().unwrap();
        assert_eq!(a.town, "Amherst");
        let f = r.data.files.unwrap();
        assert_eq!(f.files.len(), 2);
        for rel in &f.files {
            assert!(dir.path().join("data").join(rel).exists(), "{rel}");
        }
    }

    #[test]
    fn typed_statuses_make_a_gap_analysis_with_priorities() {
        let (_dir, ctx) = ctx_with(&[]);
        let st = |id: &str, s: &str| StatusEntry {
            standard_id: id.into(),
            status: s.into(),
            ..StatusEntry::default()
        };
        let r = assess_foundational_public_health_services(
            &ctx,
            FphsArgs {
                sets: vec![StdSet::Performance, StdSet::Fphs],
                statuses: vec![
                    st("PS-DIS-001", "met"),
                    st("PS-DIS-002", "not_met"),
                    st("PS-DIS-003", "partial"),
                    st("FPHS-09.01", "not met"),
                    st("FPHS-11.01", "not_met"),
                    st("PS-DIS-004", "n/a"),
                    StatusEntry {
                        standard_id: "FPHS-01.01".into(),
                        status: "partial".into(),
                        priority: Some("high".into()),
                        note: Some("Zebrafinch private note".into()),
                        ..StatusEntry::default()
                    },
                ],
                write_documents: Some(false),
                ..FphsArgs::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.mode, "gap_analysis");
        assert_eq!(r.data.standards_in_scope, 93 + 56);
        let perf = r
            .data
            .sets
            .iter()
            .find(|s| s.set == StdSet::Performance)
            .unwrap();
        assert_eq!(
            (
                perf.counts.met,
                perf.counts.partial,
                perf.counts.not_met,
                perf.counts.not_applicable
            ),
            (1, 1, 1, 1)
        );
        // 1 of 3 rated and applicable.
        assert_eq!(perf.percent_met_of_rated, Some(33.3));
        // Tier 1: PS-DIS-002 (mandate not met) and the user-prioritized FPHS-01.01.
        let t1: Vec<&str> = r
            .data
            .priority_gaps
            .iter()
            .filter(|g| g.tier == 1)
            .map(|g| g.id.as_str())
            .collect();
        assert!(
            t1.contains(&"PS-DIS-002") && t1.contains(&"FPHS-01.01"),
            "{t1:?}"
        );
        let g = r
            .data
            .priority_gaps
            .iter()
            .find(|g| g.id == "FPHS-01.01")
            .unwrap();
        assert_eq!(g.tier_from, "your priority");
        // Capability not met = tier 2, area not met = tier 3.
        let tier_of = |id: &str| {
            r.data
                .priority_gaps
                .iter()
                .find(|g| g.id == id)
                .unwrap()
                .tier
        };
        assert_eq!(tier_of("FPHS-09.01"), 3);
        assert_eq!(tier_of("PS-DIS-003"), 2);
        // The note is not in the response.
        let all = serde_json::to_string(&r).unwrap();
        assert!(!all.contains("Zebrafinch"), "note leaked");
    }

    #[test]
    fn worksheet_round_trip_through_a_csv() {
        let csv = "ID,Set,Standard,Status,Delivered by,Priority,Notes\n\
                   PS-ADM-001,performance,Annual report,met,own,,Done\n\
                   PS-ADM-002,performance,Regulations,not_met,shared,high,Call the district\n\
                   PS-ADM-003,performance,Comments,,,,\n\
                   PS-XXX-999,performance,Unknown,met,,,\n";
        let (_dir, ctx) = ctx_with(&[("assessment.csv", csv)]);
        let r = assess_foundational_public_health_services(
            &ctx,
            FphsArgs {
                sets: vec![StdSet::Performance],
                assessment_path: Some("assessment.csv".into()),
                write_documents: Some(false),
                ..FphsArgs::default()
            },
        )
        .unwrap();
        let f = r.data.assessment_file.as_ref().unwrap();
        assert_eq!(
            (f.rows_read, f.rows_with_status, f.unrecognized_id_rows),
            (4, 2, 1)
        );
        assert_eq!(r.data.total_gaps, 1);
        assert_eq!(
            r.data.priority_gaps[0].delivered_by.as_deref(),
            Some("shared")
        );
        assert_eq!(r.data.delivered_by.len(), 2);
    }

    #[test]
    fn a_client_file_is_not_a_worksheet() {
        let csv = "First Name,Last Name,DOB,Status\nMaria,Garcia,1953-04-11,active\n";
        let (_dir, ctx) = ctx_with(&[("clients.csv", csv)]);
        let e = assess_foundational_public_health_services(
            &ctx,
            FphsArgs {
                assessment_path: Some("clients.csv".into()),
                ..FphsArgs::default()
            },
        )
        .unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("self-assessment worksheet"), "{msg}");
        assert!(!msg.contains("Garcia") && !msg.contains("Maria"));
    }

    #[test]
    fn bad_input_is_a_clear_error() {
        let (_dir, ctx) = ctx_with(&[]);
        let e = assess_foundational_public_health_services(
            &ctx,
            FphsArgs {
                statuses: vec![StatusEntry {
                    standard_id: "NOPE-1".into(),
                    status: "met".into(),
                    ..StatusEntry::default()
                }],
                ..FphsArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("not a standard ID"));
        let e = assess_foundational_public_health_services(
            &ctx,
            FphsArgs {
                statuses: vec![StatusEntry {
                    standard_id: "PS-ADM-001".into(),
                    status: "perhaps".into(),
                    ..StatusEntry::default()
                }],
                ..FphsArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("unrecognized status"));
        let e = assess_foundational_public_health_services(
            &ctx,
            FphsArgs {
                town: Some("Springfeild".into()),
                ..FphsArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("Did you mean Springfield"));
    }
}
