//! EPR-07 `continuity_of_operations_planner`: a continuity of operations
//! (COOP) plan for a Massachusetts board of health or health department,
//! organized by FEMA's continuity plan elements (essential functions, orders
//! of succession, delegations of authority, continuity facilities,
//! communications, essential records, human capital, test/training/exercise,
//! devolution, reconstitution) and phases, seeded with board of health
//! essential functions and their legal authority, with a pandemic annex.
//!
//! Staff contacts follow ADR 0009: names and positions may come in the
//! request; phone numbers and emails come only from a roster file in /data and
//! go only into the plan and workbook written to /data/output.

use std::collections::BTreeMap;

use chrono::{Datelike, Months, NaiveDate};
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, Cited, CitedOut, Confidence};
use super::roster::{self, Roster, RosterSummary, RosterTally};
use crate::context::Context;
use crate::md;

/// Content id of `content/epr/continuity.yaml`.
pub const CONTINUITY: &str = "epr-continuity";

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct CoopSource {
    title: String,
    publisher: String,
    url: String,
    effective_date: Option<String>,
    retrieved: Option<String>,
}

impl CoopSource {
    fn to_source(&self) -> Source {
        Source {
            title: self.title.clone(),
            publisher: self.publisher.clone(),
            url: Some(self.url.clone()),
            effective_date: self.effective_date.clone(),
            retrieved: self.retrieved.clone(),
            mode: SourceMode::Snapshot,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Element {
    id: String,
    label: String,
    description: String,
    checklist: Vec<String>,
    source: String,
    section: String,
    #[allow(dead_code)]
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct Phase {
    #[allow(dead_code)]
    id: String,
    label: String,
    timing: String,
    actions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Tier {
    tier: u8,
    label: String,
    rto: String,
    #[allow(dead_code)]
    rto_hours: u32,
    description: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FunctionSeed {
    id: String,
    label: String,
    description: String,
    authority: String,
    source: String,
    tier: u8,
    minimum_staff: u32,
    dependencies: Vec<String>,
    incident_dependent: bool,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct SuccessionDefault {
    position: String,
    successors: Vec<String>,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DelegationSeed {
    #[allow(dead_code)]
    id: String,
    authority: String,
    delegated_to: String,
    trigger: String,
    limits: String,
    legal_basis: String,
    source: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordSeed {
    name: String,
    kind: String,
    protection: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CommSeed {
    mode: String,
    #[serde(rename = "use")]
    purpose: String,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TteSeed {
    activity: String,
    kind: String,
    frequency: String,
    months: Vec<u32>,
    owner: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DevolutionSeed {
    triggers: Vec<String>,
    partners: Vec<String>,
    agreement: Cited,
    steps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Absenteeism {
    value: f64,
    unit: String,
    #[allow(dead_code)]
    text: String,
    source: String,
    section: String,
    #[allow(dead_code)]
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct HumanCapitalSeed {
    absenteeism: Absenteeism,
    actions: Vec<Cited>,
}

/// `content/epr/continuity.yaml`.
#[derive(Debug, Clone, Deserialize)]
struct Continuity {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, CoopSource>,
    elements: Vec<Element>,
    phases: Vec<Phase>,
    tiers: Vec<Tier>,
    tiers_note: String,
    functions: Vec<FunctionSeed>,
    succession_defaults: Vec<SuccessionDefault>,
    succession_rules: Vec<Cited>,
    delegations: Vec<DelegationSeed>,
    delegation_note: String,
    records: Vec<RecordSeed>,
    communications: Vec<CommSeed>,
    tte: Vec<TteSeed>,
    tte_note: String,
    devolution: DevolutionSeed,
    reconstitution: Vec<String>,
    human_capital: HumanCapitalSeed,
    phep_links: Vec<Cited>,
    pandemic_annex: Vec<Cited>,
}

impl Continuity {
    fn title(&self, key: &str) -> String {
        self.sources
            .get(key)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| key.to_string())
    }

    fn cite(&self, items: &[Cited]) -> Vec<CitedOut> {
        items
            .iter()
            .map(|c| CitedOut {
                text: c.text.clone(),
                citation: format!("{}, {}", self.title(&c.source), c.section),
                confidence: c.confidence,
            })
            .collect()
    }

    fn tier(&self, t: u8) -> Option<&Tier> {
        self.tiers.iter().find(|x| x.tier == t)
    }
}

// ---------------------------------------------------------------- arguments

/// A change to a seeded essential function, or a new one.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct FunctionInput {
    /// Id of a seeded function to change (listed in the result's `essential_functions`); omit
    /// to add a new function.
    pub id: Option<String>,
    /// Name of a new function (or a new name for a seeded one).
    pub label: Option<String>,
    pub description: Option<String>,
    /// Priority tier 1-4: 1 continue without interruption (or within 12 hours), 2 within 24-72
    /// hours, 3 within 1-2 weeks, 4 may be deferred up to 30 days.
    pub tier: Option<u8>,
    /// Recovery time objective in your words (e.g. "4 hours"); default comes from the tier.
    pub rto: Option<String>,
    /// Minimum staff needed to perform the function.
    pub minimum_staff: Option<u32>,
    /// Legal or policy authority.
    pub authority: Option<String>,
    /// Systems, equipment, records, or partners the function depends on.
    pub dependencies: Vec<String>,
    /// Position that leads the function (e.g. "Public Health Nurse").
    pub lead_position: Option<String>,
}

/// One key position and its successors.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SuccessionInput {
    /// The position (e.g. "Health Director").
    pub position: String,
    /// The person who holds it now (optional; names may be given, but never phone numbers or
    /// emails: those come from the roster file).
    pub incumbent: Option<String>,
    /// Successors in order, by position title or name (at least three recommended).
    pub successors: Vec<String>,
    /// Trigger or note for this position.
    pub note: Option<String>,
}

/// A delegation of authority to add or change.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DelegationInput {
    /// The authority delegated (e.g. "Sign isolation and quarantine orders").
    pub authority: String,
    /// The position that receives it.
    pub delegated_to: Option<String>,
    /// When it takes effect.
    pub trigger: Option<String>,
    /// Limits, duration, and reporting.
    pub limits: Option<String>,
    /// Statute, regulation, bylaw, or vote that allows it.
    pub legal_basis: Option<String>,
}

/// A continuity facility.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SiteInput {
    /// Name of the site (e.g. "Senior Center conference room").
    pub name: String,
    /// "alternate" (default), "telework", or "devolution".
    pub kind: Option<String>,
    /// Town or facility address.
    pub location: Option<String>,
    /// Workstations or people it can hold.
    pub capacity: Option<u32>,
    /// Agreement in place (e.g. "MOU with the school district, signed 2025").
    pub agreement: Option<String>,
    /// Hours to make the site ready.
    pub ready_within_hours: Option<f64>,
    pub notes: Option<String>,
}

/// An essential record.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct RecordInput {
    pub name: String,
    /// Kind (emergency operating record, legal and financial record, state system).
    pub kind: Option<String>,
    /// How it is protected and reached from the alternate site.
    pub protection: Option<String>,
    /// Position responsible.
    pub owner_position: Option<String>,
}

/// A communications system.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CommInput {
    /// The system (e.g. "Everbridge town alerting").
    pub mode: String,
    /// What it is used for.
    pub purpose: Option<String>,
    pub note: Option<String>,
}

/// Arguments for `continuity_of_operations_planner`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CoopArgs {
    /// Agency name for the plan title (default: your letterhead's agency name, or "Board of
    /// Health" with the town).
    pub agency_name: Option<String>,
    /// Massachusetts city or town (default: your saved town).
    pub town: Option<String>,
    /// Changes to the seeded board of health essential functions, or new functions.
    pub functions: Vec<FunctionInput>,
    /// Ids of seeded functions to leave out (e.g. "seasonal_permits").
    pub exclude_functions: Vec<String>,
    /// Orders of succession: each key position with its successors in order. Without this the
    /// plan uses template positions to replace.
    pub succession: Vec<SuccessionInput>,
    /// Delegations of authority to add (the c. 111 section 30 emergency agent, burial permit
    /// agents, emergency procurement, and isolation orders are drafted already).
    pub delegations: Vec<DelegationInput>,
    /// Alternate sites, telework, and devolution sites.
    pub alternate_sites: Vec<SiteInput>,
    /// Essential records to add to the default list.
    pub essential_records: Vec<RecordInput>,
    /// Communications systems to add to the default list.
    pub communications: Vec<CommInput>,
    /// Number of department staff, for the absenteeism check.
    pub staff_count: Option<u32>,
    /// Planning absenteeism at the peak, in percent (default 40, the federal pandemic planning
    /// assumption).
    pub absenteeism_percent: Option<f64>,
    /// Devolution partner (e.g. "Neighboring town's board of health under an intermunicipal
    /// agreement").
    pub devolution_partner: Option<String>,
    /// Include the pandemic and infectious disease annex (default true).
    pub pandemic_annex: Option<bool>,
    /// Plan date (YYYY-MM-DD; default today). The test, training, and exercise schedule starts
    /// from it.
    pub plan_date: Option<String>,
    /// A staff roster CSV/XLSX in /data (name, position, agency, phones, email, radio). Contacts
    /// are filled only into the plan and workbook in /data/output and never returned.
    pub roster_path: Option<String>,
    /// Sheet name when the roster is a workbook.
    pub roster_sheet: Option<String>,
    /// Files to write to /data/output: "both" (default: Word plan and Excel workbook), "docx",
    /// "xlsx", or "none".
    pub output: Option<String>,
}

// ---------------------------------------------------------------- result

/// One essential function in the plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EssentialFunctionOut {
    pub id: String,
    pub label: String,
    pub description: String,
    pub tier: u8,
    pub tier_label: String,
    /// Recovery time objective.
    pub rto: String,
    pub minimum_staff: u32,
    pub authority: String,
    pub dependencies: Vec<String>,
    pub lead_position: Option<String>,
    /// True for functions needed only in some incidents (dispensing, shelters).
    pub incident_dependent: bool,
    /// "seed" (tool default to confirm), "seed_changed", or "your_input".
    pub basis: String,
    /// Developer confidence in a seeded entry's authority and description.
    pub confidence: Option<Confidence>,
    /// Source of a seeded entry.
    pub citation: Option<String>,
}

/// Functions and staff by tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TierOut {
    pub tier: u8,
    pub label: String,
    pub rto: String,
    pub description: String,
    pub functions: usize,
    /// Sum of minimum staff for the tier's functions that are always needed (one person may
    /// cover several functions).
    pub minimum_staff: u32,
}

/// An order of succession.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuccessionOut {
    pub position: String,
    pub incumbent: Option<String>,
    pub successors: Vec<String>,
    pub depth: usize,
    pub meets_three_deep: bool,
    /// "your_input" or "template".
    pub basis: String,
    pub note: Option<String>,
}

/// A delegation of authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DelegationOut {
    pub authority: String,
    pub delegated_to: String,
    pub trigger: String,
    pub limits: String,
    pub legal_basis: String,
    /// "drafted" (template to confirm with town counsel) or "your_input".
    pub basis: String,
    pub confidence: Option<Confidence>,
}

/// A continuity facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SiteOut {
    pub name: String,
    /// alternate, telework, or devolution.
    pub kind: String,
    pub location: Option<String>,
    pub capacity: Option<u32>,
    pub agreement: Option<String>,
    pub ready_within_hours: Option<f64>,
    pub notes: Option<String>,
    /// Problems to fix (no agreement, slower than the Tier 1 recovery time).
    pub issues: Vec<String>,
}

/// An essential record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecordOut {
    pub name: String,
    pub kind: String,
    pub protection: String,
    pub owner_position: Option<String>,
    /// "default" or "your_input".
    pub basis: String,
}

/// A communications system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommOut {
    pub mode: String,
    pub purpose: String,
    pub note: String,
    /// "default" or "your_input".
    pub basis: String,
}

/// Staffing and absenteeism.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HumanCapitalOut {
    pub staff_count: Option<u32>,
    pub absenteeism_percent: f64,
    /// Where the absenteeism figure comes from.
    pub absenteeism_basis: String,
    /// Staff expected at work at the peak (rounded down).
    pub staff_available_at_peak: Option<u32>,
    pub tier1_minimum_staff: u32,
    pub tier1_and_2_minimum_staff: u32,
    /// Plain-language result of the check.
    pub check: String,
    pub actions: Vec<CitedOut>,
}

/// A plan phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhaseOut {
    pub label: String,
    pub timing: String,
    pub actions: Vec<String>,
}

/// One scheduled test, training, exercise, or review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TteOut {
    /// Month (YYYY-MM).
    pub month: String,
    pub activity: String,
    /// test, training, exercise, or review.
    pub kind: String,
    pub frequency: String,
    pub owner: String,
}

/// Devolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DevolutionOut {
    pub partner: Option<String>,
    pub triggers: Vec<String>,
    pub partners_to_consider: Vec<String>,
    pub steps: Vec<String>,
    pub agreement: CitedOut,
}

/// Status of one FEMA continuity element in this draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GapOut {
    pub element: String,
    /// What the element covers.
    pub description: String,
    /// "provided" (from your input), "partial", "drafted" (tool template to confirm), or
    /// "missing" (needs your input).
    pub status: String,
    pub detail: String,
    /// What the element needs (FEMA).
    pub checklist: Vec<String>,
    /// Source of the element and checklist.
    pub citation: String,
}

/// Result of `continuity_of_operations_planner`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoopResult {
    pub agency: String,
    pub town: Option<String>,
    pub plan_date: String,
    pub essential_functions: Vec<EssentialFunctionOut>,
    pub tiers: Vec<TierOut>,
    pub succession: Vec<SuccessionOut>,
    pub succession_rules: Vec<CitedOut>,
    pub delegations: Vec<DelegationOut>,
    pub continuity_facilities: Vec<SiteOut>,
    pub communications: Vec<CommOut>,
    pub essential_records: Vec<RecordOut>,
    pub human_capital: HumanCapitalOut,
    pub phases: Vec<PhaseOut>,
    pub devolution: DevolutionOut,
    pub reconstitution: Vec<String>,
    pub tte_schedule: Vec<TteOut>,
    pub pandemic_annex: Vec<CitedOut>,
    /// How the plan meets CDC PHEP capability expectations for continuity.
    pub phep_links: Vec<CitedOut>,
    /// FEMA continuity elements: what is provided, drafted, or missing.
    pub gaps: Vec<GapOut>,
    /// Roster use (counts only; contacts are only in the files).
    pub roster: Option<RosterSummary>,
    pub plan_file: Option<String>,
    pub workbook_file: Option<String>,
}

// ---------------------------------------------------------------- helpers

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn norm(s: &str) -> String {
    phn_core::text::normalize(s)
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn tier_check(t: u8, what: &str) -> Result<u8> {
    if (1..=4).contains(&t) {
        Ok(t)
    } else {
        Err(PhnError::InvalidInput(format!(
            "{what}: tier must be 1, 2, 3, or 4 (got {t})"
        )))
    }
}

fn site_kind(k: Option<&str>) -> Result<String> {
    let k = k.map(norm).unwrap_or_default();
    Ok(match k.as_str() {
        "" | "alternate" | "alternate site" | "alternate facility" | "relocation" => "alternate",
        "telework" | "remote" | "home" | "virtual" => "telework",
        "devolution" | "devolution site" | "partner" => "devolution",
        other => {
            return Err(PhnError::InvalidInput(format!(
                "site kind '{other}' is not recognized; use alternate, telework, or devolution"
            )));
        }
    }
    .to_string())
}

fn outputs(o: Option<&str>) -> Result<(bool, bool)> {
    match o.map(norm).as_deref() {
        None | Some("") | Some("both") => Ok((true, true)),
        Some("docx") | Some("word") => Ok((true, false)),
        Some("xlsx") | Some("excel") => Ok((false, true)),
        Some("none") => Ok((false, false)),
        Some(other) => Err(PhnError::InvalidInput(format!(
            "output '{other}' is not recognized; use both, docx, xlsx, or none"
        ))),
    }
}

/// Argument fields not scanned for typed-in contact details: the town, dates, file
/// paths, function ids, and facility locations (a facility's address belongs in the plan).
const CONTACT_SCAN_SKIP: &[&str] = &[
    "town",
    "plan_date",
    "roster_path",
    "roster_sheet",
    "output",
    "location",
    "exclude_functions",
    "id",
];

fn para(s: impl Into<String>) -> Block {
    Block::Paragraph(vec![Inline::plain(s)])
}

fn heading(level: u8, s: impl Into<String>) -> Block {
    Block::Heading {
        level,
        text: s.into(),
    }
}

fn list(items: impl IntoIterator<Item = String>) -> Block {
    Block::List {
        ordered: false,
        items: items.into_iter().map(|s| vec![Inline::plain(s)]).collect(),
    }
}

fn table(caption: &str, headers: &[&str], rows: Vec<Vec<String>>) -> Block {
    Block::Table(TableBlock {
        caption: Some(caption.to_string()),
        headers: headers.iter().map(|h| h.to_string()).collect(),
        rows,
    })
}

// ---------------------------------------------------------------- tool

/// Build a continuity of operations plan.
pub fn continuity_of_operations_planner(
    ctx: &Context,
    args: CoopArgs,
) -> Result<Envelope<CoopResult>> {
    let c: Continuity = content::load(CONTINUITY)?;
    let (write_docx, write_xlsx) = outputs(args.output.as_deref())?;

    // ---- town, agency, date
    let town = match opt(&args.town) {
        Some(q) => Some(ctx.ma.town(q).map(|m| m.name.clone()).ok_or_else(|| {
            let s: Vec<String> = ctx
                .ma
                .suggest_towns(q, 3)
                .into_iter()
                .map(|m| m.name.clone())
                .collect();
            PhnError::NotFound(format!(
                "'{q}' is not a Massachusetts city or town{}",
                if s.is_empty() {
                    String::new()
                } else {
                    format!(". Did you mean {}?", s.join(", "))
                }
            ))
        })?),
        None => ctx.settings().jurisdiction.town.clone(),
    };
    let letterhead = ctx.settings().letterhead;
    let agency = opt(&args.agency_name)
        .map(str::to_string)
        .or_else(|| letterhead.as_ref().map(|l| l.agency_name.clone()))
        .unwrap_or_else(|| match &town {
            Some(t) => format!("{t} Board of Health"),
            None => "Board of Health".to_string(),
        });
    let plan_date = match opt(&args.plan_date) {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| {
            PhnError::InvalidInput(format!("plan_date '{d}' is not a date (YYYY-MM-DD)"))
        })?,
        None => chrono::Local::now().date_naive(),
    };
    let absenteeism = args
        .absenteeism_percent
        .unwrap_or(c.human_capital.absenteeism.value);
    if !(0.0..=100.0).contains(&absenteeism) || absenteeism.is_nan() {
        return Err(PhnError::InvalidInput(
            "absenteeism_percent must be between 0 and 100".into(),
        ));
    }

    // ---- essential functions
    let seed_ids: Vec<&str> = c.functions.iter().map(|f| f.id.as_str()).collect();
    for x in &args.exclude_functions {
        if !seed_ids.contains(&slug(x).as_str()) {
            return Err(PhnError::InvalidInput(format!(
                "exclude_functions: '{x}' is not a seeded function; ids are {}",
                seed_ids.join(", ")
            )));
        }
    }
    let excluded: Vec<String> = args.exclude_functions.iter().map(|x| slug(x)).collect();
    let mut functions: Vec<EssentialFunctionOut> = c
        .functions
        .iter()
        .filter(|f| !excluded.contains(&f.id))
        .map(|f| {
            let t = c.tier(f.tier);
            EssentialFunctionOut {
                id: f.id.clone(),
                label: f.label.clone(),
                description: f.description.clone(),
                tier: f.tier,
                tier_label: t.map(|t| t.label.clone()).unwrap_or_default(),
                rto: t.map(|t| t.rto.clone()).unwrap_or_default(),
                minimum_staff: f.minimum_staff,
                authority: f.authority.clone(),
                dependencies: f.dependencies.clone(),
                lead_position: None,
                incident_dependent: f.incident_dependent,
                basis: "seed".into(),
                confidence: Some(f.confidence),
                citation: Some(c.title(&f.source)),
            }
        })
        .collect();
    for (i, fi) in args.functions.iter().enumerate() {
        let what = format!("functions[{i}]");
        let target = opt(&fi.id).map(slug);
        let existing = target
            .as_ref()
            .and_then(|id| functions.iter().position(|f| &f.id == id));
        if let (Some(id), None) = (&target, existing) {
            if excluded.contains(id) {
                return Err(PhnError::InvalidInput(format!(
                    "{what}: '{id}' is also in exclude_functions"
                )));
            }
            if opt(&fi.label).is_none() {
                return Err(PhnError::InvalidInput(format!(
                    "{what}: '{id}' is not a seeded function; give a label to add a new one (seeded ids: {})",
                    seed_ids.join(", ")
                )));
            }
        }
        let tier = fi.tier.map(|t| tier_check(t, &what)).transpose()?;
        match existing {
            Some(pos) => {
                let f = &mut functions[pos];
                if let Some(l) = opt(&fi.label) {
                    f.label = l.to_string();
                }
                if let Some(d) = opt(&fi.description) {
                    f.description = d.to_string();
                }
                if let Some(t) = tier {
                    f.tier = t;
                    if let Some(tt) = c.tier(t) {
                        f.tier_label = tt.label.clone();
                        f.rto = tt.rto.clone();
                    }
                }
                if let Some(r) = opt(&fi.rto) {
                    f.rto = r.to_string();
                }
                if let Some(m) = fi.minimum_staff {
                    f.minimum_staff = m;
                }
                if let Some(a) = opt(&fi.authority) {
                    f.authority = a.to_string();
                }
                if !fi.dependencies.is_empty() {
                    f.dependencies = fi.dependencies.clone();
                }
                if let Some(p) = opt(&fi.lead_position) {
                    f.lead_position = Some(p.to_string());
                }
                f.basis = "seed_changed".into();
            }
            None => {
                let label = opt(&fi.label).ok_or_else(|| {
                    PhnError::InvalidInput(format!(
                        "{what}: give the id of a seeded function to change, or a label to add a new function"
                    ))
                })?;
                let t = tier.ok_or_else(|| {
                    PhnError::InvalidInput(format!(
                        "{what}: a new function ('{label}') needs a tier (1-4)"
                    ))
                })?;
                let tt = c.tier(t);
                let mut id = target.clone().unwrap_or_else(|| slug(label));
                if functions.iter().any(|f| f.id == id) {
                    id = format!("{id}_{}", i + 1);
                }
                functions.push(EssentialFunctionOut {
                    id,
                    label: label.to_string(),
                    description: opt(&fi.description).unwrap_or_default().to_string(),
                    tier: t,
                    tier_label: tt.map(|x| x.label.clone()).unwrap_or_default(),
                    rto: opt(&fi.rto)
                        .map(str::to_string)
                        .or_else(|| tt.map(|x| x.rto.clone()))
                        .unwrap_or_default(),
                    minimum_staff: fi.minimum_staff.unwrap_or(1),
                    authority: opt(&fi.authority)
                        .unwrap_or("Agency policy (add the authority)")
                        .to_string(),
                    dependencies: fi.dependencies.clone(),
                    lead_position: opt(&fi.lead_position).map(str::to_string),
                    incident_dependent: false,
                    basis: "your_input".into(),
                    confidence: None,
                    citation: None,
                });
            }
        }
    }
    functions.sort_by_key(|f| f.tier);
    if functions.is_empty() {
        return Err(PhnError::InvalidInput(
            "every essential function was excluded; keep or add at least one".into(),
        ));
    }
    let tiers: Vec<TierOut> = c
        .tiers
        .iter()
        .map(|t| {
            let fs: Vec<&EssentialFunctionOut> =
                functions.iter().filter(|f| f.tier == t.tier).collect();
            TierOut {
                tier: t.tier,
                label: t.label.clone(),
                rto: t.rto.clone(),
                description: t.description.clone(),
                functions: fs.len(),
                minimum_staff: fs
                    .iter()
                    .filter(|f| !f.incident_dependent)
                    .map(|f| f.minimum_staff)
                    .sum(),
            }
        })
        .collect();

    // ---- succession
    let succession: Vec<SuccessionOut> = if args.succession.is_empty() {
        c.succession_defaults
            .iter()
            .map(|d| SuccessionOut {
                position: d.position.clone(),
                incumbent: None,
                successors: d.successors.clone(),
                depth: d.successors.len(),
                meets_three_deep: d.successors.len() >= 3,
                basis: "template".into(),
                note: Some(d.note.clone()),
            })
            .collect()
    } else {
        let mut v = Vec::new();
        for (i, s) in args.succession.iter().enumerate() {
            let position = s.position.trim();
            if position.is_empty() {
                return Err(PhnError::InvalidInput(format!(
                    "succession[{i}]: give the position (e.g. \"Health Director\")"
                )));
            }
            let successors: Vec<String> = s
                .successors
                .iter()
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
            v.push(SuccessionOut {
                position: position.to_string(),
                incumbent: opt(&s.incumbent).map(str::to_string),
                depth: successors.len(),
                meets_three_deep: successors.len() >= 3,
                successors,
                basis: "your_input".into(),
                note: opt(&s.note).map(str::to_string),
            });
        }
        v
    };

    // ---- delegations
    let mut delegations: Vec<DelegationOut> = c
        .delegations
        .iter()
        .map(|d| DelegationOut {
            authority: d.authority.clone(),
            delegated_to: d.delegated_to.clone(),
            trigger: d.trigger.clone(),
            limits: d.limits.clone(),
            legal_basis: d.legal_basis.clone(),
            basis: "drafted".into(),
            confidence: Some(d.confidence),
        })
        .collect();
    for (i, d) in args.delegations.iter().enumerate() {
        if d.authority.trim().is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "delegations[{i}]: describe the authority delegated"
            )));
        }
        delegations.push(DelegationOut {
            authority: d.authority.trim().to_string(),
            delegated_to: opt(&d.delegated_to).unwrap_or("[POSITION]").to_string(),
            trigger: opt(&d.trigger).unwrap_or("[TRIGGER]").to_string(),
            limits: opt(&d.limits)
                .unwrap_or("[LIMITS, DURATION, AND REPORTING]")
                .to_string(),
            legal_basis: opt(&d.legal_basis)
                .unwrap_or("[LEGAL BASIS: confirm with town counsel]")
                .to_string(),
            basis: "your_input".into(),
            confidence: None,
        });
    }

    // ---- facilities
    let tier1_hours = c.tier(1).map(|t| t.rto_hours as f64).unwrap_or(12.0);
    let mut facilities = Vec::new();
    for (i, s) in args.alternate_sites.iter().enumerate() {
        if s.name.trim().is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "alternate_sites[{i}]: give the site's name"
            )));
        }
        let kind = site_kind(s.kind.as_deref())?;
        let mut issues = Vec::new();
        if kind != "telework" && opt(&s.agreement).is_none() {
            issues.push(
                "No agreement recorded: put a memorandum of understanding in place if the agency does not control the site."
                    .to_string(),
            );
        }
        if let Some(h) = s.ready_within_hours
            && h > tier1_hours
        {
            issues.push(format!(
                "Ready in {h} hours, later than the Tier 1 recovery time ({tier1_hours} hours): plan telework or a faster site for Tier 1 functions."
            ));
        }
        facilities.push(SiteOut {
            name: s.name.trim().to_string(),
            kind,
            location: opt(&s.location).map(str::to_string),
            capacity: s.capacity,
            agreement: opt(&s.agreement).map(str::to_string),
            ready_within_hours: s.ready_within_hours,
            notes: opt(&s.notes).map(str::to_string),
            issues,
        });
    }

    // ---- records and communications
    let mut records: Vec<RecordOut> = c
        .records
        .iter()
        .map(|r| RecordOut {
            name: r.name.clone(),
            kind: r.kind.clone(),
            protection: r.protection.clone(),
            owner_position: None,
            basis: "default".into(),
        })
        .collect();
    for r in &args.essential_records {
        if r.name.trim().is_empty() {
            continue;
        }
        records.push(RecordOut {
            name: r.name.trim().to_string(),
            kind: opt(&r.kind).unwrap_or("Essential record").to_string(),
            protection: opt(&r.protection)
                .unwrap_or("[HOW IT IS PROTECTED AND REACHED FROM THE ALTERNATE SITE]")
                .to_string(),
            owner_position: opt(&r.owner_position).map(str::to_string),
            basis: "your_input".into(),
        });
    }
    let mut communications: Vec<CommOut> = c
        .communications
        .iter()
        .map(|m| CommOut {
            mode: m.mode.clone(),
            purpose: m.purpose.clone(),
            note: m.note.clone(),
            basis: "default".into(),
        })
        .collect();
    for m in &args.communications {
        if m.mode.trim().is_empty() {
            continue;
        }
        communications.push(CommOut {
            mode: m.mode.trim().to_string(),
            purpose: opt(&m.purpose).unwrap_or_default().to_string(),
            note: opt(&m.note).unwrap_or_default().to_string(),
            basis: "your_input".into(),
        });
    }

    // ---- human capital
    let t1: u32 = tiers
        .iter()
        .filter(|t| t.tier == 1)
        .map(|t| t.minimum_staff)
        .sum();
    let t12: u32 = tiers
        .iter()
        .filter(|t| t.tier <= 2)
        .map(|t| t.minimum_staff)
        .sum();
    // Staff at work round down. Multiplying before dividing (and a tiny allowance) keeps
    // whole results whole: 10 staff at 80% absence is 2, not 1.9999999999999996 -> 1.
    let available = args
        .staff_count
        .map(|n| (n as f64 * (100.0 - absenteeism) / 100.0 + 1e-9).floor() as u32);
    let check = match available {
        None => "Give staff_count to check staffing at the planning absenteeism rate.".to_string(),
        Some(a) if a < t1 => format!(
            "At {absenteeism}% absence about {a} staff remain, fewer than the Tier 1 minimum ({t1}). Arrange devolution, mutual aid, and Medical Reserve Corps support now, and cross-train."
        ),
        Some(a) if a < t12 => format!(
            "At {absenteeism}% absence about {a} staff remain: enough for the Tier 1 minimum ({t1}) but not Tiers 1 and 2 together ({t12}). Defer Tier 3 and 4 work and line up surge staff."
        ),
        Some(a) => format!(
            "At {absenteeism}% absence about {a} staff remain, covering the Tier 1 and 2 minimums ({t12}). Check by name that each function has two trained people."
        ),
    };
    let ab = &c.human_capital.absenteeism;
    let human_capital = HumanCapitalOut {
        staff_count: args.staff_count,
        absenteeism_percent: absenteeism,
        absenteeism_basis: if args.absenteeism_percent.is_some() {
            "your_input".into()
        } else {
            format!(
                "{} {} ({}, {})",
                ab.value,
                ab.unit,
                c.title(&ab.source),
                ab.section
            )
        },
        staff_available_at_peak: available,
        tier1_minimum_staff: t1,
        tier1_and_2_minimum_staff: t12,
        check,
        actions: c.cite(&c.human_capital.actions),
    };

    // ---- phases, devolution, reconstitution, TT&E
    let phases: Vec<PhaseOut> = c
        .phases
        .iter()
        .map(|p| PhaseOut {
            label: p.label.clone(),
            timing: p.timing.clone(),
            actions: p.actions.clone(),
        })
        .collect();
    let devolution = DevolutionOut {
        partner: opt(&args.devolution_partner).map(str::to_string),
        triggers: c.devolution.triggers.clone(),
        partners_to_consider: c.devolution.partners.clone(),
        steps: c.devolution.steps.clone(),
        agreement: c
            .cite(std::slice::from_ref(&c.devolution.agreement))
            .remove(0),
    };
    let start = plan_date.with_day(1).unwrap_or(plan_date);
    let mut tte_schedule: Vec<TteOut> = Vec::new();
    for t in &c.tte {
        for m in &t.months {
            let d = start.checked_add_months(Months::new(*m)).unwrap_or(start);
            tte_schedule.push(TteOut {
                month: d.format("%Y-%m").to_string(),
                activity: t.activity.clone(),
                kind: t.kind.clone(),
                frequency: t.frequency.clone(),
                owner: t.owner.clone(),
            });
        }
    }
    tte_schedule.sort_by(|a, b| a.month.cmp(&b.month));
    let pandemic = args.pandemic_annex.unwrap_or(true);
    let pandemic_annex = if pandemic {
        c.cite(&c.pandemic_annex)
    } else {
        Vec::new()
    };

    // ---- gaps
    let element = |id: &str| c.elements.iter().find(|e| e.id == id);
    let mut gaps = Vec::new();
    let mut gap = |id: &str, status: &str, detail: String| {
        if let Some(e) = element(id) {
            gaps.push(GapOut {
                element: e.label.clone(),
                description: e.description.clone(),
                status: status.into(),
                detail,
                checklist: e.checklist.clone(),
                citation: format!("{}, {}", c.title(&e.source), e.section),
            });
        }
    };
    let changed = functions.iter().filter(|f| f.basis != "seed").count();
    gap(
        "essential_functions",
        if changed > 0 { "partial" } else { "drafted" },
        format!(
            "{} functions ({changed} from your input); confirm tiers, recovery times, and staff with a business impact analysis.",
            functions.len()
        ),
    );
    let shallow: Vec<&str> = succession
        .iter()
        .filter(|s| !s.meets_three_deep)
        .map(|s| s.position.as_str())
        .collect();
    gap(
        "orders_of_succession",
        if args.succession.is_empty() {
            "drafted"
        } else if shallow.is_empty() {
            "provided"
        } else {
            "partial"
        },
        if args.succession.is_empty() {
            "Template positions only; give your key positions and at least three successors each."
                .into()
        } else if shallow.is_empty() {
            format!(
                "{} position(s), each at least three deep.",
                succession.len()
            )
        } else {
            format!("Fewer than three successors for: {}.", shallow.join(", "))
        },
    );
    gap(
        "delegations_of_authority",
        if args.delegations.is_empty() {
            "drafted"
        } else {
            "partial"
        },
        format!(
            "{} delegation(s) drafted for town counsel to confirm; add any your bylaws or board votes create.",
            delegations.len()
        ),
    );
    let kinds: Vec<&str> = facilities.iter().map(|f| f.kind.as_str()).collect();
    let site_issues = facilities.iter().filter(|f| !f.issues.is_empty()).count();
    gap(
        "continuity_facilities",
        if facilities.is_empty() {
            "missing"
        } else if kinds.contains(&"alternate") && kinds.contains(&"telework") && site_issues == 0 {
            "provided"
        } else {
            "partial"
        },
        if facilities.is_empty() {
            "No alternate site given: name at least one alternate site and your telework arrangements.".into()
        } else {
            let mut d = format!("{} site(s): {}.", facilities.len(), kinds.join(", "));
            if !kinds.contains(&"alternate") {
                d.push_str(" Add a physical alternate site.");
            }
            if !kinds.contains(&"telework") {
                d.push_str(" Add telework arrangements.");
            }
            if site_issues > 0 {
                d.push_str(&format!(" {site_issues} site(s) have issues to fix."));
            }
            d
        },
    );
    gap(
        "continuity_communications",
        if args.communications.is_empty() {
            "drafted"
        } else {
            "partial"
        },
        format!(
            "{} system(s); confirm each works from the alternate site.",
            communications.len()
        ),
    );
    gap(
        "essential_records",
        if args.essential_records.is_empty() {
            "drafted"
        } else {
            "partial"
        },
        format!(
            "{} record(s); give each an owner and test remote access.",
            records.len()
        ),
    );
    gap(
        "human_capital",
        if args.staff_count.is_some() {
            "provided"
        } else {
            "missing"
        },
        human_capital.check.clone(),
    );
    gap(
        "tte",
        "drafted",
        format!(
            "{} scheduled activities in the first year (tool defaults).",
            tte_schedule.len()
        ),
    );
    gap(
        "devolution",
        if devolution.partner.is_some() {
            if kinds.contains(&"devolution") {
                "provided"
            } else {
                "partial"
            }
        } else {
            "missing"
        },
        match &devolution.partner {
            Some(p) => format!("Partner: {p}. Put a written agreement in place (M.G.L. c. 40, section 4A)."),
            None => "No devolution partner named: name a neighboring board of health or health district and its triggers.".into(),
        },
    );
    gap(
        "reconstitution",
        "drafted",
        "Steps drafted; name a reconstitution manager.".into(),
    );

    // ---- roster (contacts only into files)
    let roster = match opt(&args.roster_path) {
        Some(p) => Some(Roster::load(ctx, p, opt(&args.roster_sheet))?),
        None => None,
    };
    let mut tally = RosterTally::default();
    // `used` holds the roster rows already listed for one key position, so that a title
    // held by several people (or repeated in the order of succession) gives the next
    // holder each time, never the incumbent as their own successor or one person twice.
    let mut lookup = |who: &str, used: &mut Vec<usize>| -> (String, String) {
        let Some(r) = roster.as_ref() else {
            return (String::new(), String::new());
        };
        let index = |e: &roster::StaffContact| r.entries.iter().position(|x| std::ptr::eq(x, e));
        // A successor may be a person's name or a position title.
        let hit = r.find(Some(who), None).or_else(|| {
            r.holders(who)
                .into_iter()
                .find(|e| index(*e).is_none_or(|i| !used.contains(&i)))
        });
        match hit {
            Some(e) => {
                if let Some(i) = index(e) {
                    used.push(i);
                }
                tally.matched += 1;
                (e.name.clone(), e.contact_line())
            }
            None => {
                tally.unmatched += 1;
                (String::new(), String::new())
            }
        }
    };
    let mut succession_rows: Vec<Vec<String>> = Vec::new();
    for s in &succession {
        let mut used: Vec<usize> = Vec::new();
        let (n, contact) = lookup(s.incumbent.as_deref().unwrap_or(&s.position), &mut used);
        succession_rows.push(vec![
            s.position.clone(),
            "Incumbent".into(),
            s.incumbent.clone().unwrap_or_else(|| n.clone()),
            contact,
        ]);
        for (k, who) in s.successors.iter().enumerate() {
            let (n, contact) = lookup(who, &mut used);
            let shown = if n.is_empty() || norm(&n) == norm(who) {
                who.clone()
            } else {
                format!("{who}: {n}")
            };
            succession_rows.push(vec![
                s.position.clone(),
                format!("Successor {}", k + 1),
                shown,
                contact,
            ]);
        }
    }
    let roster_summary = roster
        .as_ref()
        .map(|r| r.summary(tally.matched, tally.unmatched));

    // ---- documents
    let function_rows: Vec<Vec<String>> = functions
        .iter()
        .map(|f| {
            vec![
                f.tier.to_string(),
                f.label.clone(),
                f.rto.clone(),
                f.minimum_staff.to_string(),
                f.lead_position.clone().unwrap_or_default(),
                f.authority.clone(),
                f.dependencies.join("; "),
                if f.incident_dependent {
                    "When activated".into()
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    let delegation_rows: Vec<Vec<String>> = delegations
        .iter()
        .map(|d| {
            vec![
                d.authority.clone(),
                d.delegated_to.clone(),
                d.trigger.clone(),
                d.limits.clone(),
                d.legal_basis.clone(),
            ]
        })
        .collect();
    let record_rows: Vec<Vec<String>> = records
        .iter()
        .map(|r| {
            vec![
                r.name.clone(),
                r.kind.clone(),
                r.owner_position.clone().unwrap_or_default(),
                r.protection.clone(),
            ]
        })
        .collect();
    let tte_rows: Vec<Vec<String>> = tte_schedule
        .iter()
        .map(|t| {
            vec![
                t.month.clone(),
                t.activity.clone(),
                t.kind.clone(),
                t.frequency.clone(),
                t.owner.clone(),
                String::new(),
            ]
        })
        .collect();
    let site_rows: Vec<Vec<String>> = facilities
        .iter()
        .map(|s| {
            vec![
                s.name.clone(),
                s.kind.clone(),
                s.location.clone().unwrap_or_default(),
                s.capacity.map(|x| x.to_string()).unwrap_or_default(),
                s.agreement.clone().unwrap_or_default(),
                s.ready_within_hours
                    .map(|x| format!("{x}"))
                    .unwrap_or_default(),
                s.issues.join(" "),
            ]
        })
        .collect();
    let comm_rows: Vec<Vec<String>> = communications
        .iter()
        .map(|m| vec![m.mode.clone(), m.purpose.clone(), m.note.clone()])
        .collect();
    let calldown_rows: Vec<Vec<String>> = roster
        .as_ref()
        .map(|r| {
            r.entries
                .iter()
                .map(|e| {
                    vec![
                        e.name.clone(),
                        e.position.clone(),
                        e.agency.clone(),
                        e.work_phone.clone(),
                        e.cell_phone.clone(),
                        e.email.clone(),
                        e.radio.clone(),
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    let succession_headers = ["Position", "Order", "Successor", "Contact (from roster)"];
    let function_headers = [
        "Tier",
        "Essential function",
        "Recovery time objective",
        "Minimum staff",
        "Lead position",
        "Authority",
        "Depends on",
        "Incident-dependent",
    ];
    let delegation_headers = [
        "Authority",
        "Delegated to",
        "Trigger",
        "Limits and reporting",
        "Legal basis",
    ];
    let record_headers = ["Record", "Kind", "Owner", "Protection and remote access"];
    let tte_headers = [
        "Month",
        "Activity",
        "Kind",
        "Frequency",
        "Owner",
        "Done (date)",
    ];
    let site_headers = [
        "Site",
        "Kind",
        "Location",
        "Capacity",
        "Agreement",
        "Ready within (hours)",
        "Issues",
    ];
    let calldown_headers = [
        "Name",
        "Position",
        "Agency",
        "Work phone",
        "Cell phone",
        "Email",
        "Radio",
    ];

    let mut files_written = Vec::new();
    let mut plan_file = None;
    let mut workbook_file = None;
    let title = format!("Continuity of operations plan — {agency}");
    if write_docx {
        let mut b: Vec<Block> = Vec::new();
        b.push(para(format!(
            "Draft prepared {} for review and adoption by the board of health. Decision support only: confirm every item with your business impact analysis, and every delegation of authority with town counsel.",
            plan_date.format("%B %-d, %Y")
        )));
        b.push(heading(1, "1. Purpose, scope, and authorities"));
        b.push(para(format!(
            "This plan describes how {agency}{} continues its essential functions when an emergency or disruption affects its staff, facilities, systems, or records, and how it returns to normal operations. It follows the continuity plan elements in FEMA's Continuity Guidance Circular and supports CDC Public Health Emergency Preparedness Capability 3 (Emergency Operations Coordination).",
            town.as_ref().map(|t| format!(" ({t})")).unwrap_or_default()
        )));
        b.push(list(
            c.cite(&c.phep_links)
                .into_iter()
                .map(|x| format!("{} ({})", x.text, x.citation)),
        ));
        b.push(heading(1, "2. Concept of operations"));
        for p in &phases {
            b.push(heading(2, format!("{} ({})", p.label, p.timing)));
            b.push(list(p.actions.clone()));
        }
        b.push(heading(1, "3. Essential functions"));
        b.push(para(c.tiers_note.clone()));
        b.push(table(
            "Essential functions by priority tier",
            &function_headers,
            function_rows.clone(),
        ));
        b.push(heading(1, "4. Orders of succession"));
        b.push(list(
            c.cite(&c.succession_rules).into_iter().map(|x| x.text),
        ));
        b.push(table(
            "Orders of succession",
            &succession_headers,
            succession_rows.clone(),
        ));
        b.push(heading(1, "5. Delegations of authority"));
        b.push(para(c.delegation_note.clone()));
        b.push(table(
            "Delegations of authority",
            &delegation_headers,
            delegation_rows.clone(),
        ));
        b.push(heading(1, "6. Continuity facilities"));
        if site_rows.is_empty() {
            b.push(para(
                "[NAME AT LEAST ONE ALTERNATE SITE AND YOUR TELEWORK ARRANGEMENTS.]",
            ));
        } else {
            b.push(table(
                "Continuity facilities",
                &site_headers,
                site_rows.clone(),
            ));
        }
        b.push(heading(1, "7. Continuity communications"));
        b.push(table(
            "Communications systems",
            &["System", "Use", "Note"],
            comm_rows.clone(),
        ));
        b.push(heading(1, "8. Essential records"));
        b.push(table(
            "Essential records",
            &record_headers,
            record_rows.clone(),
        ));
        b.push(heading(1, "9. Human capital"));
        b.push(para(format!(
            "Planning absenteeism: {absenteeism}% at the peak ({}). {}",
            human_capital.absenteeism_basis, human_capital.check
        )));
        b.push(list(human_capital.actions.iter().map(|x| x.text.clone())));
        b.push(heading(1, "10. Test, training, and exercise program"));
        b.push(para(c.tte_note.clone()));
        b.push(table(
            "Test, training, and exercise schedule",
            &tte_headers,
            tte_rows.clone(),
        ));
        b.push(heading(1, "11. Devolution of control and direction"));
        b.push(para(format!(
            "Devolution partner: {}",
            devolution
                .partner
                .clone()
                .unwrap_or_else(|| "[NAME A DEVOLUTION PARTNER]".into())
        )));
        b.push(heading(2, "Triggers"));
        b.push(list(devolution.triggers.clone()));
        b.push(heading(2, "Steps"));
        b.push(list(devolution.steps.clone()));
        b.push(para(format!(
            "{} ({})",
            devolution.agreement.text, devolution.agreement.citation
        )));
        b.push(heading(1, "12. Reconstitution"));
        b.push(list(c.reconstitution.clone()));
        if pandemic {
            b.push(heading(1, "Annex A. Pandemic and infectious disease"));
            b.push(list(
                pandemic_annex
                    .iter()
                    .map(|x| format!("{} ({})", x.text, x.citation)),
            ));
        }
        b.push(heading(1, "Annex B. Staff call-down list"));
        if calldown_rows.is_empty() {
            b.push(para(
                "[ADD THE STAFF CALL-DOWN LIST, OR RUN THE TOOL WITH A ROSTER FILE (roster_path).]",
            ));
        } else {
            b.push(para(
                "Internal document: contains staff contact details. Update quarterly.",
            ));
            b.push(table(
                "Staff call-down list",
                &calldown_headers,
                calldown_rows.clone(),
            ));
        }
        b.push(heading(1, "Record of review and approval"));
        b.push(table(
            "Approval and changes",
            &[
                "Date",
                "Change or review",
                "Approved by (name and title)",
                "Signature",
            ],
            vec![vec![
                plan_date.format("%Y-%m-%d").to_string(),
                "Draft prepared".into(),
                String::new(),
                String::new(),
            ]],
        ));
        let doc = Document {
            title: title.clone(),
            subtitle: Some("Draft for review".into()),
            language: "en-US".into(),
            author: None,
            letterhead: letterhead.clone(),
            logo: None,
            blocks: b,
        };
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let sb = ctx.sandbox()?;
        let path = sb.new_output_file("continuity_of_operations_plan.docx")?;
        std::fs::write(&path, bytes)?;
        let disp = sb.display_path(&path);
        files_written.push(disp.clone());
        plan_file = Some(disp);
    }
    if write_xlsx {
        let mut blocks = vec![
            table("Essential functions", &function_headers, function_rows),
            table("Succession", &succession_headers, succession_rows),
            table("Delegations", &delegation_headers, delegation_rows),
            table("Essential records", &record_headers, record_rows),
            table("TT&E schedule", &tte_headers, tte_rows),
            table("Communications", &["System", "Use", "Note"], comm_rows),
        ];
        if !site_rows.is_empty() {
            blocks.push(table("Facilities", &site_headers, site_rows));
        }
        if !calldown_rows.is_empty() {
            blocks.push(table("Call-down list", &calldown_headers, calldown_rows));
        }
        let doc = Document {
            title: title.clone(),
            subtitle: Some("Planning workbook".into()),
            language: "en-US".into(),
            author: None,
            letterhead: letterhead.clone(),
            logo: None,
            blocks,
        };
        let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
        let sb = ctx.sandbox()?;
        let path = sb.new_output_file("continuity_of_operations_workbook.xlsx")?;
        std::fs::write(&path, bytes)?;
        let disp = sb.display_path(&path);
        files_written.push(disp.clone());
        workbook_file = Some(disp);
    }

    // ---- privacy
    // Every text field of the request (positions, sites, records, and systems included).
    let texts = roster::argument_strings(&args, CONTACT_SCAN_SKIP);
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut privacy = Privacy::none();
    privacy.files_written = files_written.clone();
    if roster.is_some() && !files_written.is_empty() {
        privacy.notes.push(
            "The plan and workbook include staff names and contact details from your roster; they were written to /data/output only. Store them as internal emergency planning documents."
                .into(),
        );
    }
    if !found.is_empty() {
        privacy.identifiers_detected = found.clone();
        privacy
            .notes
            .push(roster::CONTACTS_IN_ARGUMENTS_NOTE.into());
    }

    // ---- summary
    let mut s = format!("**{title}**");
    if let Some(t) = &town {
        s.push_str(&format!(" ({t})"));
    }
    s.push_str(&format!(
        "\n\nDraft dated {}. Organized by FEMA's ten continuity elements; seeded with Massachusetts board of health functions and their authority.\n\n",
        plan_date.format("%B %-d, %Y")
    ));
    s.push_str("**Essential functions**\n\n");
    s.push_str(&md::table(
        &[
            "Tier",
            "Function",
            "Recovery time",
            "Min. staff",
            "Authority",
        ],
        &functions
            .iter()
            .map(|f| {
                vec![
                    f.tier.to_string(),
                    format!(
                        "{}{}",
                        f.label,
                        if f.incident_dependent {
                            " (when activated)"
                        } else {
                            ""
                        }
                    ),
                    f.rto.clone(),
                    f.minimum_staff.to_string(),
                    f.authority.clone(),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    s.push_str(&format!(
        "\nTiers and staff are tool defaults to confirm with a business impact analysis.\n\n**Orders of succession** ({})\n\n",
        if args.succession.is_empty() {
            "template positions; give yours"
        } else {
            "from your input"
        }
    ));
    for x in &succession {
        s.push_str(&format!(
            "- {}: {}{}\n",
            x.position,
            x.successors.join(" → "),
            if x.meets_three_deep {
                ""
            } else {
                " **(fewer than three successors)**"
            }
        ));
    }
    s.push_str(&format!(
        "\n**Delegations of authority:** {} drafted, including the board's emergency agent (M.G.L. c. 111, section 30: acts for the board in an emergency and reports within two days). Town counsel must confirm each.\n\n",
        delegations.len()
    ));
    s.push_str(&format!("**Human capital:** {}\n\n", human_capital.check));
    s.push_str("**Plan elements**\n\n");
    s.push_str(&md::table(
        &["Element", "Status", "What to do"],
        &gaps
            .iter()
            .map(|g| vec![g.element.clone(), g.status.clone(), g.detail.clone()])
            .collect::<Vec<_>>(),
    ));
    s.push_str("\n**Next test, training, and exercise activities**\n\n");
    for t in tte_schedule.iter().take(4) {
        s.push_str(&format!("- {}: {} ({})\n", t.month, t.activity, t.owner));
    }
    if let Some(r) = &roster_summary {
        s.push_str(&format!(
            "\n**Roster:** {} staff read from `{}`; {} position(s) or people matched and {} not matched. Names and contacts from the roster are only in the files.\n",
            r.staff, r.file, r.matched, r.unmatched
        ));
    }
    if !files_written.is_empty() {
        s.push_str(&format!(
            "\nWritten to the output folder: {}.\n",
            files_written
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !found.is_empty() {
        s.push_str(&format!("\n{}\n", roster::CONTACTS_IN_ARGUMENTS_NOTE));
    }

    // ---- sources and caveats
    let mut keys: Vec<String> = vec!["fema-cgc".into(), "fema-template".into(), "cdc-phep".into()];
    keys.extend(
        c.functions
            .iter()
            .filter(|f| functions.iter().any(|x| x.id == f.id))
            .map(|f| f.source.clone()),
    );
    keys.extend(c.delegations.iter().map(|d| d.source.clone()));
    keys.push(c.devolution.agreement.source.clone());
    keys.push(ab.source.clone());
    if pandemic {
        keys.extend(c.pandemic_annex.iter().map(|x| x.source.clone()));
    }
    let mut seen = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    for k in keys {
        if seen.contains(&k) {
            continue;
        }
        if let Some(src) = c.sources.get(&k) {
            sources.push(src.to_source());
        }
        seen.push(k);
    }
    if let Some(r) = &roster {
        sources.push(r.source());
    }
    let mut caveats = vec![
        "This is a draft for the board of health to review and adopt. Essential function tiers, recovery times, minimum staff, and test frequencies are tool defaults; set them from your own business impact analysis.".to_string(),
        "FEMA's Continuity Guidance Circular and continuity plan template could not be retrieved when this content was drafted; the element checklists follow the developer's knowledge of them (medium confidence).".to_string(),
        "Delegations of authority, isolation and quarantine orders, and emergency procurement have legal effect: town counsel must confirm the legal basis and form before adoption.".to_string(),
    ];
    if functions
        .iter()
        .any(|f| f.confidence == Some(Confidence::Low))
    {
        caveats.push(
            "Functions marked low confidence (agency policy items such as routine clinics and wellness checks) are suggestions only."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[CONTINUITY]));

    let data = CoopResult {
        agency,
        town,
        plan_date: plan_date.format("%Y-%m-%d").to_string(),
        essential_functions: functions,
        tiers,
        succession,
        succession_rules: c.cite(&c.succession_rules),
        delegations,
        continuity_facilities: facilities,
        communications,
        essential_records: records,
        human_capital,
        phases,
        devolution,
        reconstitution: c.reconstitution.clone(),
        tte_schedule,
        pandemic_annex,
        phep_links: c.cite(&c.phep_links),
        gaps,
        roster: roster_summary,
        plan_file,
        workbook_file,
    };
    Ok(Envelope::new(s, data)
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
        std::fs::write(
            dir.path().join("data/roster.csv"),
            "Name,Position,Cell Phone,Email\n\
             Ada Quell,Health Director,508-555-0101,ada@example.org\n\
             Bo Ren,Health Agent,508-555-0102,bo@example.org\n",
        )
        .unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn content_loads_and_is_complete() {
        let c: Continuity = content::load(CONTINUITY).unwrap();
        assert_eq!(c.elements.len(), 10);
        assert_eq!(c.phases.len(), 4);
        assert_eq!(c.tiers.len(), 4);
        for f in &c.functions {
            assert!(c.sources.contains_key(&f.source), "{}", f.source);
            assert!((1..=4).contains(&f.tier));
        }
        for d in &c.succession_defaults {
            assert!(d.successors.len() >= 3);
        }
        assert_eq!(c.human_capital.absenteeism.value, 40.0);
    }

    #[test]
    fn default_plan_drafts_every_element() {
        let (_d, ctx) = ctx();
        let r = continuity_of_operations_planner(
            &ctx,
            CoopArgs {
                town: Some("Wilmington".into()),
                plan_date: Some("2026-11-09".into()),
                output: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert_eq!(d.agency, "Wilmington Board of Health");
        assert_eq!(d.gaps.len(), 10);
        let status = |e: &str| {
            d.gaps
                .iter()
                .find(|g| g.element == e)
                .map(|g| g.status.clone())
                .unwrap()
        };
        assert_eq!(status("Continuity facilities"), "missing");
        assert_eq!(status("Devolution of control and direction"), "missing");
        assert_eq!(status("Orders of succession"), "drafted");
        assert!(d.essential_functions[0].tier == 1);
        assert!(
            d.essential_functions
                .iter()
                .any(|f| f.id == "burial_permits" && f.tier == 2)
        );
        assert!(
            d.delegations
                .iter()
                .any(|x| x.legal_basis.contains("section 30"))
        );
        assert_eq!(d.tte_schedule.first().unwrap().month, "2026-11");
        assert!(d.tte_schedule.iter().any(|t| t.month == "2027-10"));
        assert!(!d.pandemic_annex.is_empty());
        assert!(d.plan_file.is_none() && d.workbook_file.is_none());
        assert!(r.notice.is_some());
        assert!(r.summary.contains("Essential functions"));
    }

    #[test]
    fn inputs_change_functions_staffing_and_gaps() {
        let (_d, ctx) = ctx();
        let r = continuity_of_operations_planner(
            &ctx,
            CoopArgs {
                functions: vec![
                    FunctionInput {
                        id: Some("routine_immunization".into()),
                        tier: Some(2),
                        ..Default::default()
                    },
                    FunctionInput {
                        label: Some("Animal bite follow-up".into()),
                        tier: Some(1),
                        minimum_staff: Some(1),
                        ..Default::default()
                    },
                ],
                exclude_functions: vec!["seasonal_permits".into()],
                succession: vec![SuccessionInput {
                    position: "Health Director".into(),
                    successors: vec!["Health Agent".into(), "Public Health Nurse".into()],
                    ..Default::default()
                }],
                alternate_sites: vec![
                    SiteInput {
                        name: "Library meeting room".into(),
                        agreement: Some("MOU 2025".into()),
                        ready_within_hours: Some(24.0),
                        ..Default::default()
                    },
                    SiteInput {
                        name: "Home telework".into(),
                        kind: Some("telework".into()),
                        ..Default::default()
                    },
                ],
                staff_count: Some(10),
                devolution_partner: Some("Neighboring board of health".into()),
                plan_date: Some("2026-11-09".into()),
                output: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &r.data;
        assert!(
            !d.essential_functions
                .iter()
                .any(|f| f.id == "seasonal_permits")
        );
        let imm = d
            .essential_functions
            .iter()
            .find(|f| f.id == "routine_immunization")
            .unwrap();
        assert_eq!((imm.tier, imm.basis.as_str()), (2, "seed_changed"));
        assert!(
            d.essential_functions
                .iter()
                .any(|f| f.id == "animal_bite_follow_up" && f.basis == "your_input")
        );
        assert!(!d.succession[0].meets_three_deep);
        assert_eq!(d.human_capital.staff_available_at_peak, Some(6));
        let site = &d.continuity_facilities[0];
        assert!(
            site.issues
                .iter()
                .any(|i| i.contains("later than the Tier 1"))
        );
        let g = |e: &str| {
            d.gaps
                .iter()
                .find(|g| g.element == e)
                .unwrap()
                .status
                .clone()
        };
        assert_eq!(g("Orders of succession"), "partial");
        assert_eq!(g("Human capital"), "provided");
        assert_eq!(g("Devolution of control and direction"), "partial");
    }

    #[test]
    fn roster_contacts_go_only_to_files() {
        let (dir, ctx) = ctx();
        let r = continuity_of_operations_planner(
            &ctx,
            CoopArgs {
                succession: vec![SuccessionInput {
                    position: "Health Director".into(),
                    successors: vec![
                        "Health Agent".into(),
                        "Public Health Nurse".into(),
                        "Board of Health Chair".into(),
                    ],
                    ..Default::default()
                }],
                roster_path: Some("roster.csv".into()),
                plan_date: Some("2026-11-09".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let json = serde_json::to_string(&r).unwrap();
        for v in ["Ada Quell", "Bo Ren", "555-0101", "ada@example.org"] {
            assert!(!json.contains(v), "{v} leaked");
        }
        let roster = r.data.roster.as_ref().unwrap();
        assert_eq!((roster.staff, roster.matched, roster.unmatched), (2, 2, 2));
        let plan = dir
            .path()
            .join("data")
            .join(r.data.plan_file.as_ref().unwrap());
        let text = phn_local::docx::read_docx(&plan).unwrap().plain_text();
        assert!(text.contains("Ada Quell") && text.contains("508-555-0102"));
        assert!(r.data.workbook_file.as_ref().unwrap().ends_with(".xlsx"));
        assert!(!r.privacy.files_written.is_empty());
    }

    #[test]
    fn contacts_in_arguments_are_flagged() {
        let (_d, ctx) = ctx();
        let r = continuity_of_operations_planner(
            &ctx,
            CoopArgs {
                succession: vec![SuccessionInput {
                    position: "Health Director".into(),
                    incumbent: Some("Pat Doe".into()),
                    note: Some("cell 508-555-0177".into()),
                    successors: vec!["A".into(), "B".into(), "C".into()],
                }],
                output: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.privacy.identifiers_detected.is_empty());
        assert!(r.summary.contains("roster file"));
        // Fields that were not scanned before (a communications system) are scanned too;
        // a facility's location is not.
        let r = continuity_of_operations_planner(
            &ctx,
            CoopArgs {
                communications: vec![CommInput {
                    mode: "Director's cell 508-555-0177".into(),
                    ..Default::default()
                }],
                output: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.privacy.identifiers_detected.is_empty());
        let r = continuity_of_operations_planner(
            &ctx,
            CoopArgs {
                alternate_sites: vec![SiteInput {
                    name: "Senior Center".into(),
                    location: Some("12 Main Street".into()),
                    ..Default::default()
                }],
                output: Some("none".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.privacy.identifiers_detected.is_empty());
    }

    #[test]
    fn staff_at_work_rounds_down_without_losing_a_whole_person() {
        let (_d, ctx) = ctx();
        let at_peak = |staff: u32, pct: f64| {
            continuity_of_operations_planner(
                &ctx,
                CoopArgs {
                    staff_count: Some(staff),
                    absenteeism_percent: Some(pct),
                    output: Some("none".into()),
                    ..Default::default()
                },
            )
            .unwrap()
            .data
            .human_capital
            .staff_available_at_peak
        };
        assert_eq!(at_peak(10, 80.0), Some(2));
        assert_eq!(at_peak(90, 30.0), Some(63));
        assert_eq!(at_peak(10, 40.0), Some(6));
        // Still rounds down.
        assert_eq!(at_peak(7, 40.0), Some(4));
        assert_eq!(at_peak(3, 50.0), Some(1));
    }

    #[test]
    fn a_repeated_title_gives_the_next_holder_not_the_incumbent() {
        let (dir, ctx) = ctx();
        std::fs::write(
            dir.path().join("data/nurses.csv"),
            "Name,Position,Cell Phone\n\
             Ana One,Public Health Nurse,508-555-0111\n\
             Bea Two,Public Health Nurse,508-555-0112\n",
        )
        .unwrap();
        let r = continuity_of_operations_planner(
            &ctx,
            CoopArgs {
                succession: vec![SuccessionInput {
                    position: "Public Health Nurse".into(),
                    successors: vec!["Public Health Nurse".into(), "Public Health Nurse".into()],
                    ..Default::default()
                }],
                roster_path: Some("nurses.csv".into()),
                plan_date: Some("2026-11-09".into()),
                output: Some("docx".into()),
                ..Default::default()
            },
        )
        .unwrap();
        // Incumbent and first successor are found; no third nurse exists for the second.
        let roster = r.data.roster.as_ref().unwrap();
        assert_eq!((roster.matched, roster.unmatched), (2, 1));
        let plan = dir
            .path()
            .join("data")
            .join(r.data.plan_file.as_ref().unwrap());
        let text = phn_local::docx::read_docx(&plan).unwrap().plain_text();
        // Each nurse's number appears once in the succession table and once in the call-down list.
        assert_eq!(text.matches("508-555-0111").count(), 2, "{text}");
        assert_eq!(text.matches("508-555-0112").count(), 2, "{text}");
    }

    #[test]
    fn bad_input_is_explained() {
        let (_d, ctx) = ctx();
        let e = |a: CoopArgs| {
            continuity_of_operations_planner(&ctx, a)
                .unwrap_err()
                .to_string()
        };
        assert!(
            e(CoopArgs {
                exclude_functions: vec!["nope".into()],
                ..Default::default()
            })
            .contains("not a seeded function")
        );
        assert!(
            e(CoopArgs {
                functions: vec![FunctionInput {
                    label: Some("New".into()),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .contains("needs a tier")
        );
        assert!(
            e(CoopArgs {
                functions: vec![FunctionInput {
                    id: Some("disease_reporting".into()),
                    tier: Some(7),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .contains("tier must be")
        );
        assert!(
            e(CoopArgs {
                town: Some("Atlantis".into()),
                ..Default::default()
            })
            .contains("not a Massachusetts")
        );
        assert!(
            e(CoopArgs {
                output: Some("pdf".into()),
                ..Default::default()
            })
            .contains("not recognized")
        );
        assert!(
            e(CoopArgs {
                absenteeism_percent: Some(140.0),
                ..Default::default()
            })
            .contains("between 0 and 100")
        );
    }
}
