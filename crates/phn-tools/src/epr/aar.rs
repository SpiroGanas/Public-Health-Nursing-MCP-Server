//! EPR-09 `build_after_action_report`: turns structured hotwash and evaluator
//! findings into an HSEEP After-Action Report and Improvement Plan (AAR/IP) for
//! an exercise or a real incident.
//!
//! Product-owner decision (ADR 0009): the nurse shares de-identified notes in
//! chat and the AI passes structured strengths, areas for improvement, and
//! corrective actions; this tool scans that text for client identifiers and
//! person names (observations are non-attributional) and warns. Participant
//! feedback-form ratings in a CSV/XLSX in /data are totaled locally; only
//! suppressed aggregates are returned and free-text comments are never read
//! back. Point-of-contact phone numbers and emails come only from a roster
//! file and appear only in the files written to /data/output.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document};
use phn_local::phi::{self, Identifier};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::area;
use super::exercise::Milestone;
use super::hseep_content::{
    self as hc, CAPABILITIES, CapabilityRef, DOCTRINE, FemaCap, HSEEP, PhepCap, SmartCheck,
    bullets, heading, labeled, para, table,
};
use super::roster::{self, Roster, RosterSummary, RosterTally};
use crate::context::Context;
use crate::md;
use crate::school::templates::{DocFormat, output_folder, write_in};

/// A capability rating for an objective.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CapabilityRating {
    /// The capability (PHEP number or name, or FEMA core capability).
    pub capability: String,
    /// "P" (performed without challenges), "S" (some challenges), "M" (major challenges), or
    /// "U" (unable to be performed), as determined by the evaluation team.
    pub rating: String,
}

/// An exercise or incident objective.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AarObjective {
    pub text: String,
    /// Capabilities (PHEP numbers or names, FEMA core capabilities); FEMA ones are added
    /// from this project's PHEP mapping when only PHEP ones are given.
    pub capabilities: Vec<String>,
    /// Ratings by capability (P, S, M, U).
    pub ratings: Vec<CapabilityRating>,
}

/// One observation (non-attributional: what happened and why, not who).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ObservationInput {
    /// "strength" or "area_for_improvement".
    pub kind: String,
    /// Objective number (1 = first objective) it relates to.
    pub objective: Option<usize>,
    /// Capability (PHEP number or name, or FEMA core capability).
    pub capability: String,
    /// The observation statement. For an area for improvement, state the problem or gap only;
    /// put the fix in corrective_actions.
    pub statement: String,
    /// Plans, policies, procedures, regulations, or laws that apply.
    pub reference: Option<String>,
    /// Analysis: why it happened and its impact.
    pub analysis: Option<String>,
    /// Root cause, if identified.
    pub root_cause: Option<String>,
    /// Recommendation (becomes a draft corrective action when none is given).
    pub recommendation: Option<String>,
}

/// One corrective action for the Improvement Plan.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CorrectiveActionInput {
    /// Which area for improvement it resolves: 1 = the first area for improvement listed.
    pub area_for_improvement: Option<usize>,
    /// Capability (used to link the action when area_for_improvement is not given).
    pub capability: Option<String>,
    /// The action, written SMART.
    pub action: String,
    /// Capability element: planning, organization, equipment, training, or exercise.
    pub element: Option<String>,
    /// Primary responsible organization.
    pub organization: Option<String>,
    /// Point of contact name (optional) and position; contact details come from roster_path.
    pub poc_name: Option<String>,
    pub poc_position: Option<String>,
    /// Start date (YYYY-MM-DD).
    pub start_date: Option<String>,
    /// Completion date (YYYY-MM-DD).
    pub completion_date: Option<String>,
}

/// Participants by level or organization.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ParticipantCount {
    /// "federal", "state", "regional", "local", "nongovernmental", or an organization name.
    pub group: String,
    pub count: u32,
}

/// The report's point of contact.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ContactInput {
    pub name: Option<String>,
    pub position: String,
    pub organization: Option<String>,
}

/// Arguments for `build_after_action_report`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AarArgs {
    /// Exercise or incident name.
    pub name: Option<String>,
    /// "exercise" (default) or "incident" (a real response).
    pub kind: Option<String>,
    /// Exercise type (tabletop, functional, full-scale, ...).
    pub exercise_type: Option<String>,
    /// Start date (YYYY-MM-DD).
    pub start_date: Option<String>,
    /// End date (YYYY-MM-DD; default the start date).
    pub end_date: Option<String>,
    /// Town or county (default: your saved town).
    pub area: Option<String>,
    /// Agency writing the report (default: letterhead agency or "<town> Board of Health").
    pub agency: Option<String>,
    pub location: Option<String>,
    /// What play or the response covered.
    pub scope: Option<String>,
    /// Threat or hazard.
    pub hazard: Option<String>,
    /// Two or three sentences on the scenario or incident (no client details).
    pub scenario_summary: Option<String>,
    pub sponsor: Option<String>,
    pub objectives: Vec<AarObjective>,
    pub observations: Vec<ObservationInput>,
    pub corrective_actions: Vec<CorrectiveActionInput>,
    pub participants: Vec<ParticipantCount>,
    pub organizations: Vec<String>,
    pub point_of_contact: Option<ContactInput>,
    /// Participant feedback forms as a CSV/XLSX in /data (one row per form; rating columns
    /// 1-5 named like the form's statements or q1-q8). Totaled locally; comments are never read back.
    pub feedback_path: Option<String>,
    pub feedback_sheet: Option<String>,
    /// Staff roster CSV/XLSX in /data for point-of-contact phone and email (files only).
    pub roster_path: Option<String>,
    pub roster_sheet: Option<String>,
    /// Files to write to /data/output: "docx" (default; plus the XLSX improvement plan), "pdf",
    /// "both", or "none".
    pub documents: Option<String>,
}

/// One check of the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AarCheck {
    pub id: String,
    pub passed: bool,
    pub detail: String,
    /// HSEEP doctrine or template reference, or "tool rule".
    pub source: String,
}

/// An objective with capabilities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AarObjectiveOut {
    pub number: usize,
    pub text: String,
    pub capabilities: Vec<CapabilityRef>,
    pub smart: SmartCheck,
    pub notes: Vec<String>,
}

/// Performance of one capability under one objective.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityAnalysis {
    pub objective: usize,
    pub capability: CapabilityRef,
    /// P, S, M, U, or null when not rated.
    pub rating: Option<String>,
    pub strengths: usize,
    pub areas_for_improvement: usize,
}

/// One row of the Improvement Plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImprovementRow {
    /// "1.1" = first area for improvement of the first capability.
    pub number: String,
    pub capability: String,
    pub area_for_improvement: String,
    pub corrective_action: String,
    pub element: String,
    pub organization: String,
    /// Point of contact name and position as given (contacts stay in the files).
    pub poc: String,
    pub start_date: String,
    pub completion_date: String,
    /// True when drafted from the observation's recommendation (confirm at the After-Action Meeting).
    pub drafted_from_recommendation: bool,
    pub issues: Vec<String>,
}

/// Aggregated ratings for one feedback-form statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FactorSummary {
    pub id: String,
    pub statement: String,
    /// Column used (header name).
    pub column: String,
    pub responses: usize,
    /// Mean rating 1-5; null when suppressed (fewer responses than the threshold).
    pub mean: Option<f64>,
    /// Percent agree or strongly agree (4-5); null when suppressed.
    pub percent_agree: Option<f64>,
    /// Count per rating "1".."5" ("*" = suppressed).
    pub counts: BTreeMap<String, String>,
}

/// Participant feedback summary (aggregates only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeedbackSummary {
    pub file: String,
    pub forms: usize,
    pub factors: Vec<FactorSummary>,
    /// Statements with no matching column.
    pub unmatched_statements: Vec<String>,
    /// Forms by exercise role ("*" = suppressed).
    pub roles: BTreeMap<String, String>,
    /// Text columns whose comments were counted but not read back (header names only).
    pub comment_columns: Vec<String>,
    /// Non-empty comments in those columns (left for the planning team to read in the file).
    pub comments: usize,
    /// Columns skipped because they hold identifiers (names, emails).
    pub identifier_columns_skipped: usize,
}

/// Result of `build_after_action_report`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AarResult {
    pub name: String,
    /// "exercise" or "incident".
    pub kind: String,
    pub dates: String,
    pub area: String,
    pub agency: String,
    pub objectives: Vec<AarObjectiveOut>,
    pub mission_areas: Vec<String>,
    pub analysis: Vec<CapabilityAnalysis>,
    pub strengths: usize,
    pub areas_for_improvement: usize,
    pub improvement_plan: Vec<ImprovementRow>,
    pub checks: Vec<AarCheck>,
    pub feedback: Option<FeedbackSummary>,
    pub participants_total: u32,
    pub roster: Option<RosterSummary>,
    /// Identifier categories found in the observations (never their values).
    pub identifiers_found: Vec<String>,
    pub after_action_steps: Vec<Milestone>,
    pub files: Vec<String>,
    pub output_folder: Option<String>,
    pub warnings: Vec<String>,
}

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

fn date(label: &str, s: Option<&str>) -> Result<Option<NaiveDate>> {
    match s {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| {
                PhnError::InvalidInput(format!("{label} '{d}' is not a date (YYYY-MM-DD)"))
            }),
        None => Ok(None),
    }
}

fn is_afi(kind: &str) -> Result<bool> {
    let k = kind.trim().to_lowercase().replace(['-', ' '], "_");
    match k.as_str() {
        "strength" | "strengths" | "s" => Ok(false),
        "area_for_improvement"
        | "areas_for_improvement"
        | "afi"
        | "improvement"
        | "weakness"
        | "gap" => Ok(true),
        other => Err(PhnError::InvalidInput(format!(
            "observation kind '{other}': use strength or area_for_improvement"
        ))),
    }
}

/// Parse one feedback rating cell (1-5 or a scale label).
fn rating_value(cell: &str) -> Option<u8> {
    let t = cell.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f64>()
        && (1.0..=5.0).contains(&v)
        && v.fract() == 0.0
    {
        return Some(v as u8);
    }
    if let Some(d) = t.chars().next().and_then(|c| c.to_digit(10))
        && (1..=5).contains(&d)
        && t[1..].trim_start().starts_with(['-', '=', ' ', '('])
    {
        return Some(d as u8);
    }
    for (label, v) in [
        ("strongly disagree", 1),
        ("strongly agree", 5),
        ("disagree", 2),
        ("neutral", 3),
        ("agree", 4),
    ] {
        if t.starts_with(label) {
            return Some(v);
        }
    }
    None
}

fn role_value(cell: &str) -> Option<&'static str> {
    let t = cell.trim().to_lowercase();
    if t.is_empty() {
        None
    } else if t.contains("player") || t.contains("participant") {
        Some("Player")
    } else if t.contains("facilitat") || t.contains("control") {
        Some("Facilitator/Controller")
    } else if t.contains("evaluat") || t.contains("note") {
        Some("Evaluator")
    } else {
        Some("Other")
    }
}

fn summarize_feedback(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
) -> Result<(FeedbackSummary, Vec<String>, phn_core::Source)> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let threshold = ctx.suppression_threshold();
    let t = &lt.table;
    let forms = t.rows.len();
    let ident: Vec<usize> = lt
        .findings
        .iter()
        // Free-text columns are only counted (never read back), so they are not skipped.
        .filter(|f| f.kind.is_direct() && !matches!(f.kind, Identifier::FreeText))
        .map(|f| f.index)
        .collect();
    let headers: Vec<String> = t.headers.iter().map(|h| h.trim().to_lowercase()).collect();
    let numeric_share = |i: usize| {
        let present: Vec<&str> = t.column(i).filter(|c| !phi::is_missing(c)).collect();
        if present.is_empty() {
            0.0
        } else {
            present.iter().filter(|c| rating_value(c).is_some()).count() as f64
                / present.len() as f64
        }
    };
    let mut used: Vec<usize> = Vec::new();
    let mut factors = Vec::new();
    let mut unmatched = Vec::new();
    for (k, f) in DOCTRINE.feedback_form.factors.iter().enumerate() {
        let qn = [
            format!("q{}", k + 1),
            format!("question {}", k + 1),
            format!("item {}", k + 1),
            format!("q {}", k + 1),
        ];
        let usable =
            |i: usize| !used.contains(&i) && !ident.contains(&i) && numeric_share(i) >= 0.6;
        // Named columns first (statement id or keywords), then q1-q8 by position.
        let idx = headers
            .iter()
            .enumerate()
            .position(|(i, h)| {
                usable(i)
                    && (h == &f.id
                        || h.replace([' ', '-'], "_") == f.id
                        || f.keywords.iter().any(|kw| h.contains(kw.as_str())))
            })
            .or_else(|| {
                headers
                    .iter()
                    .enumerate()
                    .position(|(i, h)| usable(i) && qn.contains(h))
            });
        let Some(i) = idx else {
            unmatched.push(f.text.clone());
            continue;
        };
        used.push(i);
        let vals: Vec<u8> = t.column(i).filter_map(rating_value).collect();
        let n = vals.len();
        let counts_raw: Vec<(String, u64)> = (1..=5u8)
            .map(|v| {
                (
                    v.to_string(),
                    vals.iter().filter(|x| **x == v).count() as u64,
                )
            })
            .collect();
        let enough = n >= threshold as usize;
        let (shown, _) = crate::core::practice::suppressed_counts(&counts_raw, threshold);
        let counts: BTreeMap<String, String> = if enough {
            shown.into_iter().collect()
        } else {
            counts_raw
                .iter()
                .map(|(l, _)| (l.clone(), "*".to_string()))
                .collect()
        };
        let mean = enough.then(|| {
            let m = vals.iter().map(|v| *v as f64).sum::<f64>() / n as f64;
            (m * 100.0).round() / 100.0
        });
        let percent_agree = enough.then(|| {
            let a = vals.iter().filter(|v| **v >= 4).count() as f64;
            (a / n as f64 * 1000.0).round() / 10.0
        });
        factors.push(FactorSummary {
            id: f.id.clone(),
            statement: f.text.clone(),
            column: t.headers[i].clone(),
            responses: n,
            mean,
            percent_agree,
            counts,
        });
    }
    // Role column.
    let mut roles = BTreeMap::new();
    if let Some(i) = headers
        .iter()
        .enumerate()
        .position(|(i, h)| h.contains("role") && !used.contains(&i) && !ident.contains(&i))
    {
        used.push(i);
        let mut raw: BTreeMap<&str, u64> = BTreeMap::new();
        for v in t.column(i).filter_map(role_value) {
            *raw.entry(v).or_default() += 1;
        }
        let list: Vec<(String, u64)> = raw.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let (shown, _) = crate::core::practice::suppressed_counts(&list, threshold);
        roles = shown.into_iter().collect();
    }
    // Comments: other text columns (counted, never returned).
    let mut comment_columns = Vec::new();
    let mut comments = 0usize;
    for (i, h) in t.headers.iter().enumerate() {
        if used.contains(&i) || ident.contains(&i) {
            continue;
        }
        let lower = h.to_lowercase();
        let free_text = [
            "comment",
            "recommend",
            "suggest",
            "how ",
            "what ",
            "which ",
            "other",
            "feedback",
            "note",
            "improve",
            "?",
        ]
        .iter()
        .any(|c| lower.contains(c));
        let present: Vec<&str> = t.column(i).filter(|c| !phi::is_missing(c)).collect();
        if !free_text || present.is_empty() || numeric_share(i) >= 0.6 {
            continue;
        }
        comment_columns.push(h.clone());
        comments += present.len();
    }
    let skipped = {
        let mut v = ident.clone();
        v.sort();
        v.dedup();
        v.len()
    };
    Ok((
        FeedbackSummary {
            file: lt.label.clone(),
            forms,
            factors,
            unmatched_statements: unmatched,
            roles,
            comment_columns,
            comments,
            identifier_columns_skipped: skipped,
        },
        lt.identifier_categories(),
        lt.source(),
    ))
}

const RECOMMENDATION_WORDS: &[&str] = &[
    "should ",
    "recommend",
    "need to ",
    "needs to ",
    "must ",
    "will develop",
    "we will",
    "plan to ",
];

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

struct Obs<'a> {
    input: &'a ObservationInput,
    afi: bool,
    cap: Option<CapabilityRef>,
    /// 1-based number among areas for improvement.
    afi_number: Option<usize>,
}

/// Build an HSEEP After-Action Report and Improvement Plan.
pub fn build_after_action_report(ctx: &Context, args: AarArgs) -> Result<Envelope<AarResult>> {
    let kind = match opt(&args.kind).map(|k| k.to_lowercase()).as_deref() {
        None | Some("exercise") => "exercise",
        Some("incident")
        | Some("real")
        | Some("real_incident")
        | Some("response")
        | Some("event") => "incident",
        Some(o) => {
            return Err(PhnError::InvalidInput(format!(
                "kind '{o}': use exercise or incident"
            )));
        }
    };
    if args.observations.is_empty() && args.objectives.is_empty() {
        return Err(PhnError::InvalidInput(
            "give the objectives and the observations (strengths and areas for improvement) from the hotwash and evaluator notes"
                .into(),
        ));
    }
    let start = date("start_date", opt(&args.start_date))?;
    let end = date("end_date", opt(&args.end_date))?.or(start);
    if let (Some(s), Some(e)) = (start, end)
        && e < s
    {
        return Err(PhnError::InvalidInput(
            "end_date is before start_date".into(),
        ));
    }
    let ar = area::resolve(ctx, opt(&args.area), true)?;
    let town = if ar.level == area::AreaLevel::State && opt(&args.area).is_none() {
        "[Town]".to_string()
    } else {
        ar.name.clone()
    };
    let agency = opt(&args.agency)
        .map(str::to_string)
        .or_else(|| ctx.settings().letterhead.map(|l| l.agency_name))
        .unwrap_or_else(|| format!("{town} Board of Health"));
    let name = opt(&args.name).map(str::to_string).unwrap_or_else(|| {
        format!(
            "{town} {}",
            if kind == "incident" {
                "Incident Response"
            } else {
                "Exercise"
            }
        )
    });
    let dates = match (start, end) {
        (Some(s), Some(e)) if s == e => s.format("%B %-d, %Y").to_string(),
        (Some(s), Some(e)) => format!("{} to {}", s.format("%B %-d, %Y"), e.format("%B %-d, %Y")),
        _ => "[Dates]".into(),
    };
    let mut warnings = Vec::new();
    let src = |id: &str| {
        DOCTRINE
            .aar_rules
            .iter()
            .find(|r| r.id == id)
            .map(|r| {
                format!(
                    "{}, {}",
                    DOCTRINE
                        .sources
                        .get(&r.source)
                        .map(|s| s.title.as_str())
                        .unwrap_or(&r.source),
                    r.section
                )
            })
            .unwrap_or_else(|| "tool rule".into())
    };

    // Objectives.
    let mut objectives = Vec::new();
    let mut all_phep: Vec<&'static PhepCap> = Vec::new();
    let mut all_fema: Vec<&'static FemaCap> = Vec::new();
    let mut analysis: Vec<CapabilityAnalysis> = Vec::new();
    let mut unrated = 0usize;
    for (i, o) in args.objectives.iter().enumerate() {
        let m = hc::map_objective(&o.text, &o.capabilities);
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
        let mut caps: Vec<CapabilityRef> = m.phep.iter().map(|p| hc::phep_ref(p)).collect();
        caps.extend(m.fema.iter().map(|f| hc::fema_ref(f)));
        for c in &caps {
            let rating = o
                .ratings
                .iter()
                .find(|r| hc::resolve(&r.capability).is_ok_and(|x| x.to_ref().id == c.id))
                .map(|r| r.rating.trim().to_uppercase())
                .filter(|r| !r.is_empty());
            if let Some(r) = &rating
                && !["P", "S", "M", "U"].contains(&r.as_str())
            {
                return Err(PhnError::InvalidInput(format!(
                    "rating '{r}' for {}: use P, S, M, or U",
                    c.name
                )));
            }
            if rating.is_none() && c.framework == "PHEP" {
                unrated += 1;
            }
            analysis.push(CapabilityAnalysis {
                objective: i + 1,
                capability: c.clone(),
                rating,
                strengths: 0,
                areas_for_improvement: 0,
            });
        }
        objectives.push(AarObjectiveOut {
            number: i + 1,
            text: o.text.trim().to_string(),
            smart: hc::smart_check(&o.text),
            capabilities: caps,
            notes: m.notes,
        });
    }

    // Observations.
    let mut obs: Vec<Obs> = Vec::new();
    let mut afi_count = 0usize;
    let mut unresolved_caps = Vec::new();
    for o in &args.observations {
        if o.statement.trim().is_empty() {
            return Err(PhnError::InvalidInput(
                "every observation needs a statement".into(),
            ));
        }
        let afi = is_afi(&o.kind)?;
        let cap = match hc::resolve(&o.capability) {
            Ok(c) => Some(c.to_ref()),
            Err(e) => {
                unresolved_caps.push(e);
                None
            }
        };
        if let Some(n) = o.objective
            && (n == 0 || n > objectives.len())
        {
            warnings.push(format!(
                "An observation names objective {n}, but there are {} objectives.",
                objectives.len()
            ));
        }
        let afi_number = afi.then(|| {
            afi_count += 1;
            afi_count
        });
        // Tally into the analysis table.
        if let Some(c) = &cap {
            let mut hit = false;
            for a in analysis
                .iter_mut()
                .filter(|a| a.capability.id == c.id && o.objective.is_none_or(|n| n == a.objective))
            {
                hit = true;
                if afi {
                    a.areas_for_improvement += 1;
                } else {
                    a.strengths += 1;
                }
                break;
            }
            if !hit {
                analysis.push(CapabilityAnalysis {
                    objective: o.objective.unwrap_or(0),
                    capability: c.clone(),
                    rating: None,
                    strengths: usize::from(!afi),
                    areas_for_improvement: usize::from(afi),
                });
            }
        }
        obs.push(Obs {
            input: o,
            afi,
            cap,
            afi_number,
        });
    }
    let strengths = obs.iter().filter(|o| !o.afi).count();

    // Corrective actions → improvement plan.
    let roster: Option<Roster> = match opt(&args.roster_path) {
        Some(p) => Some(Roster::load(ctx, p, opt(&args.roster_sheet))?),
        None => None,
    };
    let mut tally = RosterTally::default();
    let mut ip_rows: Vec<(ImprovementRow, String)> = Vec::new(); // row + local-only contact
    let mut linked = vec![0usize; afi_count + 1];
    let mut cap_order: Vec<String> = Vec::new();
    let afis: Vec<&Obs> = obs.iter().filter(|o| o.afi).collect();
    for a in &afis {
        let key = a
            .cap
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| a.input.capability.clone());
        if !cap_order.contains(&key) {
            cap_order.push(key);
        }
    }
    let number_of = |a: &Obs| -> String {
        let key = a
            .cap
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| a.input.capability.clone());
        let k = cap_order.iter().position(|c| c == &key).unwrap_or(0) + 1;
        let n = afis
            .iter()
            .filter(|x| {
                x.cap
                    .as_ref()
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| x.input.capability.clone())
                    == key
                    && x.afi_number <= a.afi_number
            })
            .count();
        format!("{k}.{n}")
    };
    let end_or_start = end.or(start);
    let mut ca_issues_total = 0usize;
    let mut unlinked = 0usize;
    let mut smart_weak = 0usize;
    let mut build_row = |a: Option<&Obs>,
                         action: &str,
                         ca: Option<&CorrectiveActionInput>,
                         drafted: bool,
                         ip_rows: &mut Vec<(ImprovementRow, String)>|
     -> Result<()> {
        let mut issues = Vec::new();
        let element = match ca.and_then(|c| opt(&c.element)) {
            Some(e) => match hc::element(e) {
                Some(el) => el.name.clone(),
                None => {
                    issues.push(format!("capability element '{e}' is not planning, organization, equipment, training, or exercise"));
                    e.to_string()
                }
            },
            None => {
                issues.push("add the capability element (planning, organization, equipment, training, or exercise)".into());
                String::new()
            }
        };
        let org = ca.and_then(|c| opt(&c.organization)).map(str::to_string);
        let poc_name = ca.and_then(|c| opt(&c.poc_name)).map(str::to_string);
        let poc_pos = ca.and_then(|c| opt(&c.poc_position)).map(str::to_string);
        if org.is_none() {
            issues.push("add the primary responsible organization".into());
        }
        if poc_name.is_none() && poc_pos.is_none() {
            issues.push("add the organization's point of contact (name or position)".into());
        }
        let sd = date("start_date", ca.and_then(|c| opt(&c.start_date)))?;
        let cd = date("completion_date", ca.and_then(|c| opt(&c.completion_date)))?;
        match (sd, cd) {
            (None, _) | (_, None) => issues.push("add start and completion dates".into()),
            (Some(s), Some(c)) if c < s => {
                issues.push("completion date is before the start date".into())
            }
            (_, Some(c)) if end_or_start.is_some_and(|e| c < e) => {
                issues.push("completion date is before the exercise or incident ended".into())
            }
            _ => {}
        }
        let smart = hc::smart_check(action);
        if !smart.specific {
            smart_weak += 1;
            issues.push(
                "make the action specific: start with an action verb and say what will be produced"
                    .into(),
            );
        }
        if !issues.is_empty() {
            ca_issues_total += 1;
        }
        let found = if roster.is_some() && (poc_name.is_some() || poc_pos.is_some()) {
            tally.find(roster.as_ref(), poc_name.as_deref(), poc_pos.as_deref())
        } else {
            None
        };
        let contact = found.map(|f| f.contact_line()).unwrap_or_default();
        ip_rows.push((
            ImprovementRow {
                number: a.map(&number_of).unwrap_or_else(|| "-".into()),
                capability: a
                    .and_then(|x| x.cap.as_ref().map(|c| c.name.clone()))
                    .or_else(|| ca.and_then(|c| c.capability.clone()))
                    .unwrap_or_default(),
                area_for_improvement: a
                    .map(|x| x.input.statement.trim().to_string())
                    .unwrap_or_else(|| "[Not linked to an area for improvement]".into()),
                corrective_action: action.trim().to_string(),
                element,
                organization: org.unwrap_or_default(),
                poc: [poc_name, poc_pos]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", "),
                start_date: sd.map(|d| d.to_string()).unwrap_or_default(),
                completion_date: cd.map(|d| d.to_string()).unwrap_or_default(),
                drafted_from_recommendation: drafted,
                issues,
            },
            contact,
        ));
        Ok(())
    };
    for ca in &args.corrective_actions {
        if ca.action.trim().is_empty() {
            return Err(PhnError::InvalidInput(
                "every corrective action needs an action".into(),
            ));
        }
        let target: Option<&Obs> = match ca.area_for_improvement {
            Some(n) => {
                let a = afis.iter().copied().find(|a| a.afi_number == Some(n));
                if a.is_none() {
                    warnings.push(format!(
                        "A corrective action names area for improvement {n}, but there are {afi_count}."
                    ));
                }
                a
            }
            None => {
                let by_cap: Vec<&Obs> = match ca.capability.as_deref().map(hc::resolve) {
                    Some(Ok(c)) => {
                        let id = c.to_ref().id;
                        afis.iter()
                            .copied()
                            .filter(|a| a.cap.as_ref().is_some_and(|x| x.id == id))
                            .collect()
                    }
                    _ => vec![],
                };
                if by_cap.len() == 1 {
                    Some(by_cap[0])
                } else if afis.len() == 1 {
                    Some(afis[0])
                } else {
                    None
                }
            }
        };
        match target.and_then(|t| t.afi_number) {
            Some(n) => linked[n] += 1,
            None => unlinked += 1,
        }
        build_row(target, &ca.action, Some(ca), false, &mut ip_rows)?;
    }
    // Areas for improvement with a recommendation but no action: draft one from the recommendation.
    for a in &afis {
        let n = a.afi_number.unwrap_or(0);
        if linked[n] == 0
            && let Some(rec) = opt(&a.input.recommendation)
        {
            build_row(Some(a), rec, None, true, &mut ip_rows)?;
            linked[n] += 1;
        }
    }
    drop(build_row);
    ip_rows.sort_by(|x, y| {
        let key = |r: &ImprovementRow| {
            r.number
                .split('.')
                .map(|p| p.parse::<usize>().unwrap_or(usize::MAX))
                .collect::<Vec<_>>()
        };
        key(&x.0).cmp(&key(&y.0))
    });
    let improvement_plan: Vec<ImprovementRow> = ip_rows.iter().map(|(r, _)| r.clone()).collect();

    // Privacy scans of the observation and action text.
    let mut texts: Vec<&str> = Vec::new();
    for o in &args.observations {
        texts.push(&o.statement);
        for s in [&o.reference, &o.analysis, &o.root_cause, &o.recommendation]
            .into_iter()
            .flatten()
        {
            texts.push(s);
        }
    }
    for c in &args.corrective_actions {
        texts.push(&c.action);
    }
    for s in [&args.scenario_summary, &args.scope, &args.hazard]
        .into_iter()
        .flatten()
    {
        texts.push(s);
    }
    let mut names_found = 0usize;
    let mut client_kinds: Vec<String> = Vec::new();
    for t in &texts {
        for f in phi::scan_text(t) {
            match f.kind {
                Identifier::Name => names_found += 1,
                Identifier::Ssn
                | Identifier::BirthDate
                | Identifier::MedicalRecordNumber
                | Identifier::HealthPlanNumber
                | Identifier::StreetAddress
                | Identifier::AccountNumber
                | Identifier::LicenseNumber
                | Identifier::Email
                | Identifier::Phone
                | Identifier::OtherIdNumber => client_kinds.push(f.kind.label().to_string()),
                _ => {}
            }
        }
    }
    let mut contact_texts: Vec<&str> = texts.clone();
    if let Some(p) = &args.point_of_contact {
        contact_texts.push(&p.position);
        if let Some(n) = &p.name {
            contact_texts.push(n);
        }
    }
    for c in &args.corrective_actions {
        for s in [&c.poc_name, &c.poc_position, &c.organization]
            .into_iter()
            .flatten()
        {
            contact_texts.push(s);
        }
    }
    client_kinds.extend(roster::contacts_in_arguments(contact_texts));
    client_kinds.sort();
    client_kinds.dedup();
    let mut identifiers_found = client_kinds.clone();
    if names_found > 0 {
        identifiers_found.push("names".into());
        identifiers_found.sort();
        identifiers_found.dedup();
    }

    // Checks.
    let afis_without: Vec<String> = (1..=afi_count)
        .filter(|n| linked[*n] == 0)
        .map(|n| n.to_string())
        .collect();
    let mut checks = vec![
        AarCheck {
            id: "afi_has_corrective_action".into(),
            passed: afis_without.is_empty(),
            detail: if afi_count == 0 {
                "no areas for improvement were given".into()
            } else if afis_without.is_empty() {
                format!("all {afi_count} areas for improvement have at least one corrective action")
            } else {
                format!(
                    "areas for improvement without a corrective action: {}",
                    afis_without.join(", ")
                )
            },
            source: src("improvement_plan_columns"),
        },
        AarCheck {
            id: "corrective_actions_complete".into(),
            passed: ca_issues_total == 0 && unlinked == 0,
            detail: if ca_issues_total == 0 && unlinked == 0 {
                "every corrective action has an element, organization, point of contact, and dates in order".into()
            } else {
                format!(
                    "{ca_issues_total} corrective action(s) need details (see improvement_plan issues){}",
                    if unlinked > 0 {
                        format!(
                            "; {unlinked} not linked to an area for improvement (give area_for_improvement)"
                        )
                    } else {
                        String::new()
                    }
                )
            },
            source: src("smart_corrective_actions"),
        },
        AarCheck {
            id: "capabilities_mapped".into(),
            passed: unresolved_caps.is_empty()
                && objectives.iter().all(|o| {
                    o.capabilities.iter().any(|c| c.framework == "PHEP")
                        && o.capabilities.iter().any(|c| c.framework == "FEMA")
                }),
            detail: if unresolved_caps.is_empty() {
                "each objective has PHEP and FEMA core capabilities, and every observation names a known capability".into()
            } else {
                unresolved_caps.join("; ")
            },
            source: src("capability_ratings"),
        },
        AarCheck {
            id: "capability_ratings".into(),
            passed: unrated == 0,
            detail: if unrated == 0 {
                "every PHEP capability under each objective has a P, S, M, or U rating".into()
            } else {
                format!(
                    "{unrated} objective-capability pair(s) have no rating; the evaluation team assigns P, S, M, or U"
                )
            },
            source: src("capability_ratings"),
        },
    ];
    let with_rec: Vec<String> = afis
        .iter()
        .filter(|a| {
            let l = format!("{} ", a.input.statement.to_lowercase());
            RECOMMENDATION_WORDS.iter().any(|w| l.contains(w))
        })
        .map(|a| a.afi_number.unwrap_or(0).to_string())
        .collect();
    checks.push(AarCheck {
        id: "observation_statement".into(),
        passed: with_rec.is_empty(),
        detail: if with_rec.is_empty() {
            "areas for improvement state the problem, not the fix".into()
        } else {
            format!("areas for improvement {} read like recommendations; state the gap and move the fix to a corrective action", with_rec.join(", "))
        },
        source: src("observation_statement"),
    });
    let missing_analysis: Vec<String> = afis
        .iter()
        .filter(|a| {
            opt(&a.input.analysis).is_none() && opt(&a.input.root_cause).is_none()
                || opt(&a.input.reference).is_none()
        })
        .map(|a| a.afi_number.unwrap_or(0).to_string())
        .collect();
    checks.push(AarCheck {
        id: "reference_and_analysis".into(),
        passed: missing_analysis.is_empty(),
        detail: if missing_analysis.is_empty() {
            "each area for improvement has a reference and an analysis".into()
        } else {
            format!(
                "add the reference and the analysis or root cause for areas for improvement {}",
                missing_analysis.join(", ")
            )
        },
        source: src("reference_and_analysis"),
    });
    checks.push(AarCheck {
        id: "non_attribution".into(),
        passed: names_found == 0,
        detail: if names_found == 0 {
            "no person names found in observations or actions".into()
        } else {
            format!("{names_found} possible person name(s) found: describe what happened and why, not who (use positions)")
        },
        source: src("non_attribution"),
    });
    checks.push(AarCheck {
        id: "client_identifiers".into(),
        passed: client_kinds.is_empty(),
        detail: if client_kinds.is_empty() {
            "no client identifiers or contact details detected (still check for details that could identify a person)".into()
        } else {
            format!("possible identifiers: {}; remove them before the report is shared", client_kinds.join(", "))
        },
        source: "tool rule (privacy)".into(),
    });
    if smart_weak > 0 {
        warnings.push(format!(
            "{smart_weak} corrective action(s) are not specific; see the HSEEP example: {}",
            DOCTRINE.smart_corrective_action.example
        ));
    }
    for o in &objectives {
        for n in &o.notes {
            warnings.push(format!("Objective {}: {n}", o.number));
        }
    }

    // Feedback forms.
    let mut privacy = Privacy::none();
    let mut extra_sources = Vec::new();
    let feedback = match opt(&args.feedback_path) {
        Some(p) => {
            let (fs, cats, source) = summarize_feedback(ctx, p, opt(&args.feedback_sheet))?;
            privacy.suppression_applied = true;
            privacy.suppression_threshold = Some(ctx.suppression_threshold());
            if !cats.is_empty() {
                privacy.notes.push(format!(
                    "The feedback file contains {}; those columns were skipped and nothing from them is returned.",
                    cats.join(", ")
                ));
            }
            if fs.factors.is_empty() {
                warnings.push(format!(
                    "No rating columns matched the feedback form statements in '{}'. Name columns q1-q8 or after the statements (briefings, scenario, right people, ...), with values 1-5.",
                    fs.file
                ));
            }
            extra_sources.push(source);
            Some(fs)
        }
        None => None,
    };

    // After-action steps.
    let days_re = regex::Regex::new(r"(?i)within (\d+) days").expect("valid regex");
    let after_action_steps: Vec<Milestone> = DOCTRINE
        .after_action_timeline
        .iter()
        .map(|s| {
            let n = days_re
                .captures(&s.when)
                .and_then(|c| c[1].parse::<i64>().ok());
            Milestone {
                milestone: s.step.clone(),
                when: s.when.clone(),
                date: match (end_or_start, n) {
                    (Some(d), Some(n)) => Some((d + chrono::Duration::days(n)).to_string()),
                    _ => None,
                },
                purpose: String::new(),
                tool_default: s.tool_default,
            }
        })
        .collect();

    let mission_areas = hc::mission_areas(&all_fema);
    let participants_total: u32 = args.participants.iter().map(|p| p.count).sum();
    let mut result = AarResult {
        name: name.clone(),
        kind: kind.into(),
        dates: dates.clone(),
        area: town.clone(),
        agency: agency.clone(),
        objectives,
        mission_areas,
        analysis,
        strengths,
        areas_for_improvement: afi_count,
        improvement_plan,
        checks,
        feedback,
        participants_total,
        roster: None,
        identifiers_found,
        after_action_steps,
        files: vec![],
        output_folder: None,
        warnings: vec![],
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
    let poc_line = match &args.point_of_contact {
        Some(p) => {
            let found = tally.find(roster.as_ref(), opt(&p.name), Some(&p.position));
            let mut parts: Vec<String> = Vec::new();
            if let Some(n) = opt(&p.name)
                .map(str::to_string)
                .or_else(|| found.map(|f| f.name.clone()))
                .filter(|n| !n.is_empty())
            {
                parts.push(n);
            }
            parts.push(p.position.clone());
            parts.push(
                opt(&p.organization)
                    .map(str::to_string)
                    .unwrap_or_else(|| agency.clone()),
            );
            parts.push(
                found
                    .map(|f| f.contact_line())
                    .filter(|c| !c.is_empty())
                    .unwrap_or_else(|| "[phone and email]".into()),
            );
            parts.join(", ")
        }
        None => "[Name, title, agency, phone, and email of the primary point of contact]".into(),
    };
    if !formats.is_empty() {
        let r = &result;
        let overview_word = if kind == "incident" {
            "Incident"
        } else {
            "Exercise"
        };
        let caps_of = |fw: &str| {
            let mut v: Vec<String> = Vec::new();
            for o in &r.objectives {
                for c in o.capabilities.iter().filter(|c| c.framework == fw) {
                    if !v.contains(&c.name) {
                        v.push(c.name.clone());
                    }
                }
            }
            v.join("; ")
        };
        let mut b: Vec<Block> = vec![
            para(
                "The After-Action Report/Improvement Plan (AAR/IP) aligns objectives with preparedness doctrine and related frameworks and guidance.",
            ),
            heading(1, format!("{overview_word} overview")),
            table(
                &format!("{overview_word} overview"),
                &["Item", "Detail"],
                vec![
                    vec![format!("{overview_word} name"), r.name.clone()],
                    vec![format!("{overview_word} dates"), r.dates.clone()],
                    vec![
                        "Scope".into(),
                        if kind == "incident" {
                            opt(&args.scope)
                                .map(str::to_string)
                                .unwrap_or_else(|| "[What the response covered]".into())
                        } else {
                            format!(
                                "This exercise is a {}{}. Exercise play is limited to {}.",
                                opt(&args.exercise_type).unwrap_or("[exercise type]"),
                                opt(&args.location)
                                    .map(|l| format!(" at {l}"))
                                    .unwrap_or_default(),
                                opt(&args.scope).unwrap_or("[exercise parameters]")
                            )
                        },
                    ],
                    vec!["Focus area(s)".into(), r.mission_areas.join(", ")],
                    vec!["PHEP capabilities".into(), caps_of("PHEP")],
                    vec!["FEMA core capabilities".into(), caps_of("FEMA")],
                    vec![
                        "Objectives".into(),
                        r.objectives
                            .iter()
                            .map(|o| format!("{}. {}", o.number, o.text))
                            .collect::<Vec<_>>()
                            .join(" "),
                    ],
                    vec![
                        "Threat or hazard".into(),
                        opt(&args.hazard)
                            .unwrap_or("[Threat or hazard]")
                            .to_string(),
                    ],
                    vec![
                        if kind == "incident" {
                            "Incident summary".into()
                        } else {
                            "Scenario".to_string()
                        },
                        opt(&args.scenario_summary)
                            .unwrap_or("[Brief overview, 2-3 sentences]")
                            .to_string(),
                    ],
                    vec![
                        "Sponsor".into(),
                        opt(&args.sponsor).unwrap_or(&agency).to_string(),
                    ],
                    vec![
                        "Participating organizations".into(),
                        if r.participants_total > 0 {
                            format!("{} participants (see Appendix B)", r.participants_total)
                        } else {
                            "[Number of participants and levels (see Appendix B)]".into()
                        },
                    ],
                    vec!["Point of contact".into(), poc_line.clone()],
                ],
            ),
            heading(1, "Analysis of capabilities"),
            para(
                "Aligning objectives and capabilities provides a consistent taxonomy for evaluation. Table 1 lists the objectives, aligned capabilities, and performance ratings as determined by the evaluation team.",
            ),
            table(
                "Table 1. Summary of capability performance",
                &[
                    "Objective",
                    "Capability",
                    "Performed without Challenges (P)",
                    "Performed with Some Challenges (S)",
                    "Performed with Major Challenges (M)",
                    "Unable to be Performed (U)",
                ],
                r.analysis
                    .iter()
                    .filter(|a| a.objective > 0)
                    .map(|a| {
                        let x = |c: &str| {
                            if a.rating.as_deref() == Some(c) {
                                "X".to_string()
                            } else {
                                String::new()
                            }
                        };
                        vec![
                            a.objective.to_string(),
                            a.capability.name.clone(),
                            x("P"),
                            x("S"),
                            x("M"),
                            x("U"),
                        ]
                    })
                    .collect(),
            ),
            heading(2, "Ratings definitions"),
            bullets(
                DOCTRINE
                    .ratings
                    .iter()
                    .map(|r| format!("{} ({}): {}", r.label, r.code, r.definition)),
            ),
        ];
        let obs_block = |o: &Obs, k: usize| -> Vec<Block> {
            let mut v = Vec::new();
            if o.afi {
                v.push(labeled(
                    &format!("Area for improvement {}", o.afi_number.unwrap_or(k)),
                    o.input.statement.trim().to_string(),
                ));
                v.push(labeled(
                    "Reference",
                    opt(&o.input.reference)
                        .unwrap_or(
                            "[List relevant plans, policies, procedures, regulations, or laws.]",
                        )
                        .to_string(),
                ));
                let mut an = opt(&o.input.analysis).unwrap_or("[Root cause analysis or summary of why the full capability level was not achieved.]").to_string();
                if let Some(rc) = opt(&o.input.root_cause) {
                    an.push_str(&format!(" Root cause: {rc}"));
                }
                v.push(labeled("Analysis", an));
            } else {
                v.push(labeled(
                    &format!("Strength {k}"),
                    o.input.statement.trim().to_string(),
                ));
            }
            v
        };
        let mut placed = vec![false; obs.len()];
        for o in &r.objectives {
            b.push(heading(2, format!("Objective {}: {}", o.number, o.text)));
            for c in &o.capabilities {
                let mine: Vec<usize> = obs
                    .iter()
                    .enumerate()
                    .filter(|(i, x)| {
                        !placed[*i]
                            && x.cap.as_ref().is_some_and(|xc| xc.id == c.id)
                            && x.input.objective.is_none_or(|n| n == o.number)
                    })
                    .map(|(i, _)| i)
                    .collect();
                if mine.is_empty() && c.framework == "FEMA" {
                    continue;
                }
                b.push(heading(3, c.name.clone()));
                let st: Vec<usize> = mine.iter().copied().filter(|i| !obs[*i].afi).collect();
                let af: Vec<usize> = mine.iter().copied().filter(|i| obs[*i].afi).collect();
                b.push(heading(4, "Strengths"));
                if st.is_empty() {
                    b.push(para("[No strengths recorded for this capability.]"));
                }
                for (k, i) in st.iter().enumerate() {
                    b.extend(obs_block(&obs[*i], k + 1));
                    placed[*i] = true;
                }
                b.push(heading(4, "Areas for improvement"));
                if af.is_empty() {
                    b.push(para(
                        "[No areas for improvement recorded for this capability.]",
                    ));
                }
                for (k, i) in af.iter().enumerate() {
                    b.extend(obs_block(&obs[*i], k + 1));
                    placed[*i] = true;
                }
            }
        }
        let rest: Vec<usize> = (0..obs.len()).filter(|i| !placed[*i]).collect();
        if !rest.is_empty() {
            b.push(heading(2, "Other observations"));
            for (k, i) in rest.iter().enumerate() {
                let o = &obs[*i];
                b.push(heading(
                    3,
                    o.cap
                        .as_ref()
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| o.input.capability.clone()),
                ));
                b.extend(obs_block(o, k + 1));
            }
        }
        let ip_table_rows: Vec<Vec<String>> = r
            .improvement_plan
            .iter()
            .map(|x| {
                vec![
                    x.number.clone(),
                    x.capability.clone(),
                    x.area_for_improvement.clone(),
                    x.corrective_action.clone(),
                    x.element.clone(),
                    x.organization.clone(),
                    x.poc.clone(),
                    x.start_date.clone(),
                    x.completion_date.clone(),
                ]
            })
            .collect();
        let ip_headers = [
            "#",
            "Capability",
            "Issue/Area for improvement",
            "Corrective action",
            "Capability element",
            "Primary responsible organization",
            "Organization POC",
            "Start date",
            "Completion date",
        ];
        b.push(Block::PageBreak);
        b.push(heading(1, "Appendix A: Improvement Plan"));
        b.push(para(format!(
            "This Improvement Plan is developed specifically for {} as a result of {} conducted on {}.",
            agency, r.name, r.dates
        )));
        b.push(table(
            "Improvement Plan",
            &ip_headers,
            ip_table_rows.clone(),
        ));
        b.push(heading(1, "Appendix B: Participants"));
        if args.participants.is_empty() && args.organizations.is_empty() {
            b.push(para("[List participating organizations by level: federal, state, regional, local, and nongovernmental.]"));
        }
        if !args.participants.is_empty() {
            b.push(table(
                "Participants",
                &["Level or organization", "Participants"],
                args.participants
                    .iter()
                    .map(|p| vec![p.group.clone(), p.count.to_string()])
                    .collect(),
            ));
        }
        if !args.organizations.is_empty() {
            b.push(bullets(args.organizations.iter().cloned()));
        }
        if let Some(f) = &r.feedback {
            b.push(heading(1, "Appendix C: Participant feedback summary"));
            b.push(para(format!(
                "{} feedback forms. Ratings from 1 (strongly disagree) to 5 (strongly agree); counts below {} are hidden (*).",
                f.forms,
                ctx.suppression_threshold()
            )));
            b.push(table(
                "Participant feedback ratings",
                &[
                    "Statement",
                    "Responses",
                    "Mean",
                    "% agree",
                    "1",
                    "2",
                    "3",
                    "4",
                    "5",
                ],
                f.factors
                    .iter()
                    .map(|x| {
                        let mut row = vec![
                            x.statement.clone(),
                            x.responses.to_string(),
                            x.mean.map(|m| md::num(m, 2)).unwrap_or_else(|| "*".into()),
                            x.percent_agree
                                .map(|p| md::num(p, 1))
                                .unwrap_or_else(|| "*".into()),
                        ];
                        for k in 1..=5 {
                            row.push(x.counts.get(&k.to_string()).cloned().unwrap_or_default());
                        }
                        row
                    })
                    .collect(),
            ));
        }
        let main = doc(
            ctx,
            format!("{}: After-Action Report/Improvement Plan", r.name),
            Some(r.dates.clone()),
            b,
        );
        let mut xrows: Vec<Vec<String>> = Vec::new();
        for (x, contact) in &ip_rows {
            xrows.push(vec![
                x.number.clone(),
                x.capability.clone(),
                x.area_for_improvement.clone(),
                x.corrective_action.clone(),
                x.element.clone(),
                x.organization.clone(),
                x.poc.clone(),
                contact.clone(),
                x.start_date.clone(),
                x.completion_date.clone(),
                "Not started".into(),
                String::new(),
            ]);
        }
        let xdoc = doc(
            ctx,
            format!("{}: Improvement Plan tracker", r.name),
            None,
            vec![table(
                "Improvement Plan",
                &[
                    "#",
                    "Capability",
                    "Issue/Area for improvement",
                    "Corrective action",
                    "Capability element",
                    "Primary responsible organization",
                    "Organization POC",
                    "POC contact",
                    "Start date",
                    "Completion date",
                    "Status",
                    "Notes",
                ],
                if xrows.is_empty() {
                    vec![vec![String::new(); 12]]
                } else {
                    xrows
                },
            )],
        );
        let slug: String = r
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        let (dir, shown) = output_folder(ctx, &format!("aar_{slug}"))?;
        let mut files = Vec::new();
        for f in &formats {
            files.push(write_in(
                ctx,
                &dir,
                &format!("after_action_report.{}", f.ext()),
                &f.render(&main)?,
            )?);
        }
        files.push(write_in(
            ctx,
            &dir,
            "improvement_plan.xlsx",
            &phn_docs::xlsx::to_xlsx(&xdoc)?,
        )?);
        privacy.files_written.extend(files.iter().cloned());
        result.files = files;
        result.output_folder = Some(shown);
    }
    if let Some(ro) = &roster {
        result.roster = Some(ro.summary(tally.matched, tally.unmatched));
        privacy.notes.push("Point-of-contact phone numbers and emails from the roster were written only into the files in the output folder.".into());
    }
    if !result.identifiers_found.is_empty() {
        privacy.identifiers_detected = result.identifiers_found.clone();
        privacy.notes.push("Possible identifiers were found in the observation or action text; remove anything that identifies a client, and describe actions by position rather than by name.".into());
    }
    if !client_kinds.is_empty()
        && client_kinds
            .iter()
            .any(|k| k.contains("phone") || k.contains("email"))
    {
        privacy
            .notes
            .push(roster::CONTACTS_IN_ARGUMENTS_NOTE.into());
    }
    result.warnings = warnings;

    // Summary.
    let r = &result;
    let mut s = format!(
        "**{}: After-Action Report/Improvement Plan** ({}, {})\n\n{} strength(s), {} area(s) for improvement, {} corrective action(s).\n\n",
        r.name,
        r.kind,
        r.dates,
        r.strengths,
        r.areas_for_improvement,
        r.improvement_plan.len()
    );
    if !r.analysis.is_empty() {
        s.push_str("**Capability performance**\n\n");
        s.push_str(&md::table(
            &[
                "Objective",
                "Capability",
                "Rating",
                "Strengths",
                "Areas for improvement",
            ],
            &r.analysis
                .iter()
                .filter(|a| {
                    a.capability.framework == "PHEP" || a.strengths + a.areas_for_improvement > 0
                })
                .map(|a| {
                    vec![
                        if a.objective == 0 {
                            "-".into()
                        } else {
                            a.objective.to_string()
                        },
                        a.capability.name.clone(),
                        a.rating.clone().unwrap_or_else(|| "not rated".into()),
                        a.strengths.to_string(),
                        a.areas_for_improvement.to_string(),
                    ]
                })
                .collect::<Vec<_>>(),
        ));
        s.push('\n');
    }
    if !r.improvement_plan.is_empty() {
        s.push_str("**Improvement Plan**\n\n");
        s.push_str(&md::table(
            &[
                "#",
                "Corrective action",
                "Element",
                "Organization",
                "POC",
                "Start",
                "Complete",
                "Issues",
            ],
            &r.improvement_plan
                .iter()
                .map(|x| {
                    vec![
                        x.number.clone(),
                        format!(
                            "{}{}",
                            x.corrective_action,
                            if x.drafted_from_recommendation {
                                " *(drafted from the recommendation)*"
                            } else {
                                ""
                            }
                        ),
                        x.element.clone(),
                        x.organization.clone(),
                        x.poc.clone(),
                        x.start_date.clone(),
                        x.completion_date.clone(),
                        x.issues.join("; "),
                    ]
                })
                .collect::<Vec<_>>(),
        ));
        s.push('\n');
    }
    let failed: Vec<&AarCheck> = r.checks.iter().filter(|c| !c.passed).collect();
    if failed.is_empty() {
        s.push_str("All HSEEP checks passed.\n");
    } else {
        s.push_str("**Checks to fix before the After-Action Meeting**\n");
        for c in &failed {
            s.push_str(&format!("- {}: {}\n", c.id.replace('_', " "), c.detail));
        }
    }
    if let Some(f) = &r.feedback {
        s.push_str(&format!(
            "\n**Participant feedback** ({} forms; counts under {} hidden)\n\n",
            f.forms,
            ctx.suppression_threshold()
        ));
        s.push_str(&md::table(
            &["Statement", "Responses", "Mean", "% agree"],
            &f.factors
                .iter()
                .map(|x| {
                    vec![
                        x.statement.clone(),
                        x.responses.to_string(),
                        x.mean.map(|m| md::num(m, 2)).unwrap_or_else(|| "*".into()),
                        x.percent_agree
                            .map(|p| md::num(p, 1))
                            .unwrap_or_else(|| "*".into()),
                    ]
                })
                .collect::<Vec<_>>(),
        ));
        if f.comments > 0 {
            s.push_str(&format!(
                "\n{} written comment(s) in {} column(s) were not read back; review them in the file.\n",
                f.comments,
                f.comment_columns.len()
            ));
        }
    }
    s.push_str("\n**Next steps**\n");
    for m in &r.after_action_steps {
        s.push_str(&format!(
            "- {}: {}{}\n",
            m.milestone,
            m.when,
            m.date
                .as_ref()
                .map(|d| format!(" ({d})"))
                .unwrap_or_default()
        ));
    }
    if let Some(f) = &r.output_folder {
        s.push_str(&format!(
            "\nFiles written to `{f}`: {}.\n",
            r.files.join(", ")
        ));
    }
    if !r.warnings.is_empty() {
        s.push_str("\n**Notes**\n");
        for w in &r.warnings {
            s.push_str(&format!("- {w}\n"));
        }
    }

    let mut sources = hc::cite(&[
        (HSEEP, "hseep-2020"),
        (HSEEP, "aar-template"),
        (CAPABILITIES, "phep-2018"),
        (CAPABILITIES, "fema-eegs"),
    ]);
    if r.feedback.is_some() {
        sources.extend(hc::cite(&[(HSEEP, "pff-template")]));
    }
    sources.extend(extra_sources);
    if let Some(ro) = &roster {
        sources.push(ro.source());
    }
    let mut caveats = vec![
        "This is a draft for the After-Action Meeting: senior leaders and participating organizations confirm the observations, ratings, corrective actions, owners, and dates before the report is final.".to_string(),
        "Observations come from what you entered; the tool checks structure and wording, not whether the findings are right.".to_string(),
        format!("PHEP to FEMA core capability links are this project's mapping, not an official crosswalk: {}", hc::CAPS.mapping_note.trim()),
        "After-action due dates (30, 45, and 60 days) are tool defaults, not HSEEP requirements; follow your grant or program deadlines.".to_string(),
    ];
    if kind == "incident" {
        caveats.push("For a real incident, check the report with your agency before sharing: it may describe protected health information or security-sensitive details.".into());
    }
    if roster.is_some() {
        caveats.push("The files in the output folder contain staff contact details; share them like other internal plans.".into());
    }
    caveats.extend(super::content::review_caveats(&[CAPABILITIES, HSEEP]));
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
            "Name,Position,Cell Phone,Email\nBo Ren,Health Director,508-555-0102,bo@example.org\n",
        )
        .unwrap();
        let mut csv =
            String::from("Name,Email,Role,q1,q2,Scenario realistic,actively involved,Comments\n");
        for i in 0..12 {
            csv.push_str(&format!(
                "Person {i},p{i}@example.org,{},{},{},{},{},{}\n",
                if i < 9 { "Player" } else { "Evaluator" },
                if i < 2 { 3 } else { 5 },
                4,
                if i % 2 == 0 {
                    "Agree"
                } else {
                    "Strongly agree"
                },
                if i < 3 { 2 } else { 4 },
                if i % 3 == 0 {
                    "More time for module 2"
                } else {
                    ""
                }
            ));
        }
        std::fs::write(dir.path().join("data/feedback.csv"), csv).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn base() -> AarArgs {
        AarArgs {
            name: Some("Amherst Measles Response Tabletop Exercise".into()),
            exercise_type: Some("tabletop exercise".into()),
            start_date: Some("2027-03-17".into()),
            area: Some("Amherst".into()),
            objectives: vec![
                AarObjective {
                    text: "Within 24 hours of DPH notification, discuss how exposed contacts will be identified, in accordance with the DPH measles guide.".into(),
                    capabilities: vec!["13".into()],
                    ratings: vec![CapabilityRating { capability: "13".into(), rating: "S".into() }],
                },
                AarObjective {
                    text: "Within 4 hours of confirmation, discuss how a public message will be approved and released through the public information procedures.".into(),
                    capabilities: vec!["PHEP 4".into()],
                    ratings: vec![],
                },
            ],
            observations: vec![
                ObservationInput {
                    kind: "strength".into(),
                    objective: Some(1),
                    capability: "13".into(),
                    statement: "Players quickly set the exposure windows from the infectious period.".into(),
                    ..Default::default()
                },
                ObservationInput {
                    kind: "area_for_improvement".into(),
                    objective: Some(1),
                    capability: "13".into(),
                    statement: "There was no agreed way to get urgent care patient lists after hours.".into(),
                    reference: Some("Communicable disease procedures".into()),
                    analysis: Some("No after-hours contact list for urgent care centers exists.".into()),
                    ..Default::default()
                },
                ObservationInput {
                    kind: "afi".into(),
                    objective: Some(2),
                    capability: "Emergency Public Information and Warning".into(),
                    statement: "The PIO should create translated templates.".into(),
                    recommendation: Some("Develop translated templates for 3 measles messages by June 30.".into()),
                    ..Default::default()
                },
            ],
            corrective_actions: vec![CorrectiveActionInput {
                area_for_improvement: Some(1),
                action: "Develop an after-hours contact list for the 4 urgent care centers in town".into(),
                element: Some("Planning".into()),
                organization: Some("Amherst Board of Health".into()),
                poc_position: Some("Health Director".into()),
                start_date: Some("2027-04-01".into()),
                completion_date: Some("2027-06-30".into()),
                ..Default::default()
            }],
            participants: vec![ParticipantCount { group: "local".into(), count: 14 }],
            roster_path: Some("roster.csv".into()),
            feedback_path: Some("feedback.csv".into()),
            point_of_contact: Some(ContactInput { position: "Health Director".into(), ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn builds_report_plan_and_checks() {
        let (dir, ctx) = ctx();
        let r = build_after_action_report(&ctx, base()).unwrap();
        let d = &r.data;
        assert_eq!((d.strengths, d.areas_for_improvement), (1, 2));
        assert_eq!(d.improvement_plan.len(), 2);
        assert_eq!(d.improvement_plan[0].number, "1.1");
        assert!(
            d.improvement_plan[0].issues.is_empty(),
            "{:?}",
            d.improvement_plan[0].issues
        );
        assert!(d.improvement_plan[1].drafted_from_recommendation);
        let check = |id: &str| d.checks.iter().find(|c| c.id == id).unwrap().passed;
        assert!(check("afi_has_corrective_action"));
        assert!(
            !check("corrective_actions_complete"),
            "the drafted action lacks owner and dates"
        );
        assert!(!check("capability_ratings"), "objective 2 is not rated");
        assert!(
            !check("observation_statement"),
            "AFI 2 reads like a recommendation"
        );
        assert!(!check("reference_and_analysis"));
        assert!(check("non_attribution") && check("client_identifiers"));
        assert!(d.analysis.iter().any(|a| a.capability.id == "phep-13"
            && a.rating.as_deref() == Some("S")
            && a.strengths == 1));
        // Feedback: aggregates only, identifiers skipped, comments not returned.
        let f = d.feedback.as_ref().unwrap();
        assert_eq!(f.forms, 12);
        let q1 = f.factors.iter().find(|x| x.id == "briefings").unwrap();
        assert_eq!(q1.responses, 12);
        assert_eq!(q1.counts["3"], "*");
        assert!((q1.mean.unwrap() - 4.67).abs() < 0.01);
        let sc = f
            .factors
            .iter()
            .find(|x| x.id == "scenario_realistic")
            .unwrap();
        assert_eq!(sc.percent_agree, Some(100.0));
        assert_eq!(f.comments, 4);
        assert!(f.identifier_columns_skipped >= 2);
        // 9 players and 3 evaluators: the small group and its complement are hidden.
        assert_eq!(f.roles["Evaluator"], "*");
        let json = serde_json::to_string(&r).unwrap();
        for leak in [
            "Person 3",
            "p3@example.org",
            "More time for module",
            "508-555-0102",
            "bo@example.org",
        ] {
            assert!(!json.contains(leak), "{leak} leaked");
        }
        assert_eq!(d.files.len(), 2);
        let bytes = std::fs::read(dir.path().join("data").join(&d.files[0])).unwrap();
        let text = phn_local::docx::parse_docx(&bytes).unwrap().plain_text();
        assert!(
            text.contains("bo@example.org") && text.contains("Area for improvement 1"),
            "{text}"
        );
        assert!(r.privacy.suppression_applied);
    }

    #[test]
    fn flags_names_identifiers_and_bad_input() {
        let (_d, ctx) = ctx();
        let mut a = base();
        a.feedback_path = None;
        a.roster_path = None;
        a.documents = Some("none".into());
        a.observations[0].statement =
            "Nurse Maria Garcia called the family at 413-555-0199 and reviewed MRN: 4455667."
                .into();
        let r = build_after_action_report(&ctx, a).unwrap();
        let d = &r.data;
        let check = |id: &str| d.checks.iter().find(|c| c.id == id).unwrap().passed;
        assert!(!check("non_attribution") && !check("client_identifiers"));
        assert!(
            r.privacy
                .identifiers_detected
                .iter()
                .any(|k| k.contains("names"))
        );
        assert!(d.files.is_empty());
        let mut a = base();
        a.observations[0].kind = "opinion".into();
        assert!(build_after_action_report(&ctx, a).is_err());
        let mut a = base();
        a.objectives[0].ratings[0].rating = "B".into();
        assert!(
            build_after_action_report(&ctx, a)
                .unwrap_err()
                .to_string()
                .contains("P, S, M, or U")
        );
        assert!(build_after_action_report(&ctx, AarArgs::default()).is_err());
    }

    #[test]
    fn feedback_suppresses_small_groups() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/fb.csv"), "q1,q2\n5,4\n4,4\n3,5\n").unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let (f, _, _) = summarize_feedback(&ctx, "fb.csv", None).unwrap();
        assert_eq!(f.forms, 3);
        assert!(
            f.factors
                .iter()
                .all(|x| x.mean.is_none() && x.counts.values().all(|v| v == "*"))
        );
    }

    #[test]
    fn rating_values_parse() {
        assert_eq!(rating_value("5"), Some(5));
        assert_eq!(rating_value("4 - Agree"), Some(4));
        assert_eq!(rating_value("Strongly disagree"), Some(1));
        assert_eq!(rating_value("agree"), Some(4));
        assert_eq!(rating_value("7"), None);
        assert_eq!(rating_value("n/a"), None);
    }
}
