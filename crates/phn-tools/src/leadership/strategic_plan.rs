//! LEAD-05 `build_strategic_plan`: scaffolding for a health department strategic plan. A SWOT or
//! SOAR analysis, a stakeholder power and interest grid (accessible SVG plus a table), the
//! priority, goal, objective, and strategy structure, and the link to the community health
//! improvement plan (CHIP). The nurse supplies every fact; the tool organizes it, checks for
//! blanks, and writes a draft Word document to /data/output.
//!
//! PHAB's requirements for a strategic plan could not be read (phaboard.org was blocked), so the
//! tool reports them as unverified and never says a plan "meets PHAB requirements".

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, ImageBlock};
use phn_docs::palette::{AXIS, CATEGORICAL, GRID, INK, INK_SECONDARY, SURFACE, label_on};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::strategy_docs::{DocBuilder, clip, tidy, tidy_opt};
use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::epr::roster;
use crate::md;
use crate::planning::smartie::read_date;
use crate::school::templates;

/// Content id of `content/leadership/strategic_planning.yaml`.
pub const STRATEGIC_PLANNING: &str = "leadership-strategic-planning";
/// Content id of the Phase 15 community health assessment framework (reused for the CHIP link).
const CHA_FRAMEWORK: &str = "equity-cha-framework";

const MAX_TEXT: usize = 600;
const MAX_LIST: usize = 20;
const MAX_STAKEHOLDERS: usize = 40;
const MAX_PRIORITIES: usize = 12;
const MAX_GOALS: usize = 8;
const MAX_ITEMS: usize = 10;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct PhabRequirements {
    status: String,
    statement: String,
    read_in_sources: Vec<Cited>,
    to_check_in_phab_document: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Quadrant {
    id: String,
    label: String,
    #[serde(default)]
    location: String,
    prompts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TowsPrompt {
    id: String,
    label: String,
    prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Swot {
    description: String,
    support: String,
    support_confidence: Confidence,
    quadrants: Vec<Quadrant>,
    tows_prompts: Vec<TowsPrompt>,
    tows_note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Soar {
    description: String,
    support: String,
    support_confidence: Confidence,
    quadrants: Vec<Quadrant>,
}

#[derive(Debug, Clone, Deserialize)]
struct StakeQuadrant {
    id: String,
    label: String,
    power: String,
    interest: String,
    approach: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StakeholderDef {
    support: String,
    support_confidence: Confidence,
    scale: String,
    high_threshold: f64,
    threshold_note: String,
    quadrants: Vec<StakeQuadrant>,
    columns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Layer {
    label: String,
    definition: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SectionDef {
    id: String,
    label: String,
    guidance: String,
    basis: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct PlanDef {
    hierarchy_note: String,
    layers: Vec<Layer>,
    sections: Vec<SectionDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChipLinkDef {
    description: String,
    where_chip_falls_note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CheckDef {
    id: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StrategicContent {
    meta: ContentMeta,
    sources: BTreeMap<String, DocSource>,
    phab_requirements: PhabRequirements,
    swot: Swot,
    soar: Soar,
    stakeholder: StakeholderDef,
    plan: PlanDef,
    chip_link: ChipLinkDef,
    checks: Vec<CheckDef>,
    checks_note: String,
    not_verified: Vec<String>,
}

// ---------------------------------------------------------------- arguments

/// Which analysis to scaffold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMethod {
    /// Strengths, weaknesses, opportunities, threats.
    Swot,
    /// Strengths, opportunities, aspirations, results.
    Soar,
}

/// Your SWOT entries (short phrases about your department and its setting).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SwotInput {
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub opportunities: Vec<String>,
    pub threats: Vec<String>,
}

/// Your SOAR entries.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SoarInput {
    pub strengths: Vec<String>,
    pub opportunities: Vec<String>,
    pub aspirations: Vec<String>,
    pub results: Vec<String>,
}

/// One stakeholder (an organization, group, or role).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StakeholderIn {
    /// Organization, group, or role (for example "Board of health", "Hospital community benefits
    /// office", "Residents of the east side").
    pub name: String,
    /// Power to help or block the plan, 1 (lowest) to 5 (highest).
    pub power: Option<f64>,
    /// Interest in the plan, 1 (lowest) to 5 (highest).
    pub interest: Option<f64>,
    /// Group or sector (optional): "government", "health care", "community", "funder".
    pub group: Option<String>,
    pub current_engagement: Option<String>,
    pub desired_engagement: Option<String>,
    /// What you need from them.
    pub needs: Option<String>,
    /// Who on your team owns the relationship (a role is better than a name).
    pub owner: Option<String>,
}

/// An objective under a goal.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ObjectiveIn {
    pub statement: String,
    /// The indicator that will show progress.
    pub measure: Option<String>,
    pub baseline: Option<String>,
    pub target: Option<String>,
    /// Due date: YYYY, YYYY-MM, or YYYY-MM-DD.
    pub due: Option<String>,
}

/// A strategy under a goal.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StrategyIn {
    pub statement: String,
    /// Responsible role or person.
    pub responsible: Option<String>,
    pub timeline: Option<String>,
    pub resources: Option<String>,
}

/// A goal under a priority.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct GoalIn {
    pub statement: String,
    pub objectives: Vec<ObjectiveIn>,
    pub strategies: Vec<StrategyIn>,
}

/// A strategic priority.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PriorityIn {
    pub title: String,
    /// Why it is a priority (your assessment findings).
    pub rationale: Option<String>,
    /// The CHIP priority this one supports (use the same words as in chip_priorities).
    pub chip_priority: Option<String>,
    pub goals: Vec<GoalIn>,
}

/// Arguments for `build_strategic_plan`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct StrategicPlanArgs {
    /// Your department or board of health (for the document title).
    pub department: Option<String>,
    /// First year of the plan (1900-2100).
    pub period_start: Option<i32>,
    /// Last year of the plan (at most 15 years after the start).
    pub period_end: Option<i32>,
    pub mission: Option<String>,
    pub vision: Option<String>,
    pub values: Vec<String>,
    /// "swot" or "soar". Default: the one you gave entries for (SWOT when neither).
    pub method: Option<AnalysisMethod>,
    pub swot: Option<SwotInput>,
    pub soar: Option<SoarInput>,
    pub stakeholders: Vec<StakeholderIn>,
    pub priorities: Vec<PriorityIn>,
    /// Title of your community health improvement plan.
    pub chip_title: Option<String>,
    /// The CHIP's priorities, in its own words.
    pub chip_priorities: Vec<String>,
    /// Write the draft plan (DOCX) and the grid (SVG and PNG) to /data/output (default true).
    pub write_document: Option<bool>,
    /// Return the SVG text in the response (default false; the files hold it anyway).
    pub include_svg: Option<bool>,
}

// ---------------------------------------------------------------- results

/// One analysis quadrant with your entries and the prompts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuadrantOut {
    pub id: String,
    pub label: String,
    pub location: String,
    pub your_entries: Vec<String>,
    pub prompts: Vec<String>,
}

/// A strategy prompt that pairs the quadrants (SWOT only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TowsOut {
    pub id: String,
    pub label: String,
    pub prompt: String,
}

/// The analysis scaffold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisOut {
    pub method: AnalysisMethod,
    pub description: String,
    pub support: String,
    pub support_confidence: Confidence,
    pub quadrants: Vec<QuadrantOut>,
    pub strategy_prompts: Vec<TowsOut>,
    pub entries_total: usize,
}

/// One stakeholder in the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderRow {
    pub number: usize,
    pub name: String,
    pub power: Option<f64>,
    pub interest: Option<f64>,
    /// Quadrant label, or "Not rated" when power or interest is missing.
    pub quadrant: String,
    pub approach: String,
    pub group: Option<String>,
    pub current_engagement: Option<String>,
    pub desired_engagement: Option<String>,
    pub needs: Option<String>,
    pub owner: Option<String>,
}

/// The stakeholder analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StakeholderOut {
    pub scale: String,
    pub threshold_note: String,
    pub support: String,
    pub support_confidence: Confidence,
    pub rows: Vec<StakeholderRow>,
    pub quadrant_counts: Vec<(String, usize)>,
    /// Alt text for the grid.
    pub alt_text: String,
    /// The grid as accessible SVG (only when include_svg is true).
    pub svg: Option<String>,
}

/// An objective with its code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectiveOut {
    pub code: String,
    pub statement: String,
    pub measure: Option<String>,
    pub baseline: Option<String>,
    pub target: Option<String>,
    pub due: Option<String>,
}

/// A strategy with its code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StrategyOut {
    pub code: String,
    pub statement: String,
    pub responsible: Option<String>,
    pub timeline: Option<String>,
    pub resources: Option<String>,
}

/// A goal with its code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GoalOut {
    pub code: String,
    pub statement: String,
    pub objectives: Vec<ObjectiveOut>,
    pub strategies: Vec<StrategyOut>,
}

/// A priority with its code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PriorityOut {
    pub code: String,
    pub title: String,
    pub rationale: Option<String>,
    pub chip_priority: Option<String>,
    pub goals: Vec<GoalOut>,
}

/// A CHIP priority and the strategic priorities linked to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChipRow {
    pub chip_priority: String,
    pub linked_strategic_priorities: Vec<String>,
}

/// The CHIP link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChipOut {
    pub title: Option<String>,
    pub rows: Vec<ChipRow>,
    /// Strategic priorities that name a CHIP priority not in your CHIP list.
    pub unmatched_links: Vec<String>,
    /// MAPP 2.0 phases from the community health assessment framework (Phase 15 content).
    pub assessment_framework_phases: Vec<String>,
    pub note: String,
}

/// A blank or inconsistency found in the draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub id: String,
    /// "needs_work" for a blank in the plan itself, "consider" otherwise.
    pub severity: String,
    pub message: String,
}

/// An outline section and whether it has content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutlineOut {
    pub label: String,
    pub guidance: String,
    pub has_content: bool,
    pub basis: String,
    pub confidence: Confidence,
}

/// What is and is not known about PHAB's strategic plan requirements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhabStatus {
    /// Always "unverified" until PHAB's document is read.
    pub status: String,
    pub statement: String,
    pub read_in_sources: Vec<CitedOut>,
    pub to_check_in_phab_document: Vec<String>,
}

/// Files written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StrategyFiles {
    pub folder: String,
    pub document: String,
    pub svg: Option<String>,
    pub png: Option<String>,
}

/// Result of `build_strategic_plan`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StrategicPlanResult {
    pub department: Option<String>,
    pub period: Option<String>,
    pub analysis: AnalysisOut,
    pub stakeholders: StakeholderOut,
    pub layers: Vec<String>,
    pub priorities: Vec<PriorityOut>,
    pub chip: ChipOut,
    pub findings: Vec<Finding>,
    pub outline: Vec<OutlineOut>,
    pub phab: PhabStatus,
    pub files: Option<StrategyFiles>,
}

// ---------------------------------------------------------------- input checks

fn text_in(field: &str, s: &str) -> Result<String> {
    let t = tidy(s);
    if t.chars().count() > MAX_TEXT {
        return Err(PhnError::InvalidInput(format!(
            "{field} is longer than {MAX_TEXT} characters; use a short phrase or sentence"
        )));
    }
    Ok(t)
}

fn opt_in(field: &str, s: &Option<String>) -> Result<Option<String>> {
    match tidy_opt(s) {
        Some(t) => text_in(field, &t).map(Some),
        None => Ok(None),
    }
}

fn list_in(field: &str, v: &[String]) -> Result<Vec<String>> {
    if v.len() > MAX_LIST {
        return Err(PhnError::InvalidInput(format!(
            "{field} has {} entries; keep at most {MAX_LIST}",
            v.len()
        )));
    }
    v.iter()
        .map(|s| text_in(field, s))
        .filter(|r| !matches!(r, Ok(t) if t.is_empty()))
        .collect()
}

fn rating_in(field: &str, v: Option<f64>) -> Result<Option<f64>> {
    match v {
        None => Ok(None),
        Some(x) if x.is_finite() && (1.0..=5.0).contains(&x) => Ok(Some(x)),
        Some(_) => Err(PhnError::InvalidInput(format!(
            "{field} must be a number from 1 to 5"
        ))),
    }
}

fn norm(s: &str) -> String {
    tidy(&s.to_lowercase())
}

// ---------------------------------------------------------------- the stakeholder grid

fn esc(s: &str) -> String {
    phn_docs::docx::esc(s)
}

fn quadrant_of(def: &StakeholderDef, power: f64, interest: f64) -> &StakeQuadrant {
    let hp = if power > def.high_threshold { "high" } else { "low" };
    let hi = if interest > def.high_threshold { "high" } else { "low" };
    def.quadrants
        .iter()
        .find(|q| q.power == hp && q.interest == hi)
        .unwrap_or(&def.quadrants[0])
}

/// Marker drawn for a quadrant (shape is a second cue besides color and the number).
fn marker_svg(kind: usize, cx: f32, cy: f32, fill: &str) -> String {
    match kind % 4 {
        0 => format!("<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"11\" fill=\"{fill}\" stroke=\"{INK}\" stroke-width=\"1\"/>"),
        1 => format!("<rect x=\"{:.1}\" y=\"{:.1}\" width=\"22\" height=\"22\" fill=\"{fill}\" stroke=\"{INK}\" stroke-width=\"1\"/>", cx - 11.0, cy - 11.0),
        2 => format!("<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"{fill}\" stroke=\"{INK}\" stroke-width=\"1\"/>", cx, cy - 13.0, cx - 12.0, cy + 10.0, cx + 12.0, cy + 10.0),
        _ => format!("<polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"{fill}\" stroke=\"{INK}\" stroke-width=\"1\"/>", cx, cy - 13.0, cx + 13.0, cy, cx, cy + 13.0, cx - 13.0, cy),
    }
}

fn grid_svg(
    def: &StakeholderDef,
    title: &str,
    rows: &[StakeholderRow],
    alt: &str,
    desc: &str,
) -> String {
    const W: f32 = 760.0;
    const PX: f32 = 90.0; // plot left
    const PY: f32 = 70.0; // plot top
    const PW: f32 = 600.0;
    const PH: f32 = 440.0;
    let pos = |v: f64, size: f32| -> f32 { ((v - 1.0) / 4.0) as f32 * size };
    let mid_x = PX + pos(def.high_threshold, PW);
    let mid_y = PY + PH - pos(def.high_threshold, PH);
    // Legend rows below the plot.
    let legend_top = PY + PH + 64.0;
    let legend_h = rows.len() as f32 * 17.0 + 10.0;
    let height = legend_top + legend_h + 14.0;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" aria-labelledby=\"t d\" viewBox=\"0 0 {W} {height:.0}\" width=\"{W}\" height=\"{height:.0}\" font-family=\"DejaVu Sans, Arial, sans-serif\">\n<title id=\"t\">{}</title>\n<desc id=\"d\">{}</desc>\n<rect width=\"{W}\" height=\"{height:.0}\" fill=\"{SURFACE}\"/>\n",
        esc(alt),
        esc(desc)
    );
    s.push_str(&format!(
        "<text x=\"{PX}\" y=\"30\" font-size=\"17\" font-weight=\"bold\" fill=\"{INK}\">{}</text>\n<text x=\"{PX}\" y=\"50\" font-size=\"12\" fill=\"{INK_SECONDARY}\">{}</text>\n",
        esc(&clip(title, 80)),
        esc("Power to help or block the plan (up) and interest in the plan (right), each rated 1 to 5")
    ));
    // Quadrant tints (very light) and labels.
    let tints = ["#f4f8fd", "#fdf6f1", "#f1f8f5", "#f6f4fb"];
    // top-right: manage closely (index by definition order is not assumed; find by power/interest)
    let quads: [(&str, &str, f32, f32, f32, f32, &str); 4] = [
        ("high", "high", mid_x, PY, PX + PW - mid_x, mid_y - PY, tints[0]),
        ("high", "low", PX, PY, mid_x - PX, mid_y - PY, tints[1]),
        ("low", "high", mid_x, mid_y, PX + PW - mid_x, PY + PH - mid_y, tints[2]),
        ("low", "low", PX, mid_y, mid_x - PX, PY + PH - mid_y, tints[3]),
    ];
    for (p, i, x, y, w, h, tint) in quads {
        let label = def
            .quadrants
            .iter()
            .find(|q| q.power == p && q.interest == i)
            .map(|q| q.label.as_str())
            .unwrap_or("");
        s.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{tint}\"/>\n<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" font-weight=\"bold\" fill=\"{INK_SECONDARY}\">{}</text>\n",
            x + 8.0,
            y + 18.0,
            esc(label)
        ));
    }
    // Gridlines and ticks.
    for v in 1..=5 {
        let gx = PX + pos(v as f64, PW);
        let gy = PY + PH - pos(v as f64, PH);
        s.push_str(&format!(
            "<line x1=\"{gx:.1}\" y1=\"{PY}\" x2=\"{gx:.1}\" y2=\"{:.1}\" stroke=\"{GRID}\" stroke-width=\"1\"/>\n<line x1=\"{PX}\" y1=\"{gy:.1}\" x2=\"{:.1}\" y2=\"{gy:.1}\" stroke=\"{GRID}\" stroke-width=\"1\"/>\n<text x=\"{gx:.1}\" y=\"{:.1}\" font-size=\"12\" text-anchor=\"middle\" fill=\"{INK_SECONDARY}\">{v}</text>\n<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\" text-anchor=\"end\" fill=\"{INK_SECONDARY}\">{v}</text>\n",
            PY + PH,
            PX + PW,
            PY + PH + 16.0,
            PX - 8.0,
            gy + 4.0
        ));
    }
    s.push_str(&format!(
        "<rect x=\"{PX}\" y=\"{PY}\" width=\"{PW}\" height=\"{PH}\" fill=\"none\" stroke=\"{AXIS}\" stroke-width=\"1.5\"/>\n<line x1=\"{mid_x:.1}\" y1=\"{PY}\" x2=\"{mid_x:.1}\" y2=\"{:.1}\" stroke=\"{AXIS}\" stroke-width=\"1.5\" stroke-dasharray=\"6 4\"/>\n<line x1=\"{PX}\" y1=\"{mid_y:.1}\" x2=\"{:.1}\" y2=\"{mid_y:.1}\" stroke=\"{AXIS}\" stroke-width=\"1.5\" stroke-dasharray=\"6 4\"/>\n",
        PY + PH,
        PX + PW
    ));
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" text-anchor=\"middle\" fill=\"{INK}\">Interest in the plan (1 low to 5 high)</text>\n<text transform=\"translate(28 {:.1}) rotate(-90)\" font-size=\"13\" text-anchor=\"middle\" fill=\"{INK}\">Power to help or block (1 low to 5 high)</text>\n",
        PX + PW / 2.0,
        PY + PH + 40.0,
        PY + PH / 2.0
    ));
    // Points: stakeholders at the same cell are spread out.
    let mut used: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    for r in rows {
        let (Some(p), Some(i)) = (r.power, r.interest) else {
            continue;
        };
        let key = ((p * 10.0).round() as i64, (i * 10.0).round() as i64);
        let k = *used.entry(key).and_modify(|n| *n += 1).or_insert(0);
        let dx = (k % 3) as f32 * 26.0;
        let dy = (k / 3) as f32 * 26.0;
        let cx = (PX + pos(i, PW) + dx).clamp(PX + 14.0, PX + PW - 14.0);
        let cy = (PY + PH - pos(p, PH) + dy).clamp(PY + 14.0, PY + PH - 14.0);
        let q = quadrant_of(def, p, i);
        let qi = def.quadrants.iter().position(|x| x.id == q.id).unwrap_or(0);
        let fill = CATEGORICAL[[0usize, 1, 5, 6][qi % 4]];
        s.push_str(&marker_svg(qi, cx, cy, fill));
        s.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{:.1}\" font-size=\"12\" font-weight=\"bold\" text-anchor=\"middle\" fill=\"{}\">{}</text>\n",
            cy + 4.5,
            label_on(fill),
            r.number
        ));
    }
    // Legend (the same information as the table).
    s.push_str(&format!(
        "<text x=\"{PX}\" y=\"{:.1}\" font-size=\"13\" font-weight=\"bold\" fill=\"{INK}\">Key (number, stakeholder, power, interest, quadrant)</text>\n",
        legend_top - 8.0
    ));
    for (n, r) in rows.iter().enumerate() {
        let rating = match (r.power, r.interest) {
            (Some(p), Some(i)) => format!("power {p}, interest {i}"),
            _ => "not rated, not drawn".to_string(),
        };
        s.push_str(&format!(
            "<text x=\"{PX}\" y=\"{:.1}\" font-size=\"12\" fill=\"{INK}\">{}. {} ({}): {}</text>\n",
            legend_top + 8.0 + n as f32 * 17.0,
            r.number,
            esc(&clip(&r.name, 60)),
            esc(&rating),
            esc(&r.quadrant)
        ));
    }
    s.push_str("</svg>\n");
    s
}

// ---------------------------------------------------------------- the tool

/// Build a strategic plan scaffold from the nurse's own content.
pub fn build_strategic_plan(
    ctx: &Context,
    args: StrategicPlanArgs,
) -> Result<Envelope<StrategicPlanResult>> {
    let c: StrategicContent = content::load(STRATEGIC_PLANNING)?;

    // ---- validate and tidy inputs
    let department = opt_in("department", &args.department)?;
    if let Some(d) = &department
        && d.chars().count() > 150
    {
        return Err(PhnError::InvalidInput(
            "department is longer than 150 characters; use the short name".into(),
        ));
    }
    let (start, end) = (args.period_start, args.period_end);
    for (f, y) in [("period_start", start), ("period_end", end)] {
        if let Some(y) = y
            && !(1900..=2100).contains(&y)
        {
            return Err(PhnError::InvalidInput(format!(
                "{f} {y} is outside 1900-2100"
            )));
        }
    }
    if let (Some(s), Some(e)) = (start, end) {
        if e < s {
            return Err(PhnError::InvalidInput(
                "period_end is before period_start".into(),
            ));
        }
        if e - s > 15 {
            return Err(PhnError::InvalidInput(
                "a strategic plan period longer than 15 years is not a plan; use 3 to 5 years".into(),
            ));
        }
    }
    let period = match (start, end) {
        (Some(s), Some(e)) => Some(format!("{s}-{e}")),
        (Some(s), None) => Some(format!("{s} onward")),
        (None, Some(e)) => Some(format!("through {e}")),
        (None, None) => None,
    };
    let mission = opt_in("mission", &args.mission)?;
    let vision = opt_in("vision", &args.vision)?;
    let values = list_in("values", &args.values)?;
    let chip_title = opt_in("chip_title", &args.chip_title)?;
    let chip_priorities = list_in("chip_priorities", &args.chip_priorities)?;

    let method = match (args.method, &args.swot, &args.soar) {
        (Some(m), _, _) => m,
        (None, Some(_), Some(_)) => {
            return Err(PhnError::InvalidInput(
                "you gave both SWOT and SOAR entries; choose one with method (\"swot\" or \"soar\")".into(),
            ));
        }
        (None, None, Some(_)) => AnalysisMethod::Soar,
        _ => AnalysisMethod::Swot,
    };
    let entries: BTreeMap<&str, Vec<String>> = match method {
        AnalysisMethod::Swot => {
            let s = args.swot.clone().unwrap_or_default();
            BTreeMap::from([
                ("strengths", list_in("swot.strengths", &s.strengths)?),
                ("weaknesses", list_in("swot.weaknesses", &s.weaknesses)?),
                ("opportunities", list_in("swot.opportunities", &s.opportunities)?),
                ("threats", list_in("swot.threats", &s.threats)?),
            ])
        }
        AnalysisMethod::Soar => {
            let s = args.soar.clone().unwrap_or_default();
            BTreeMap::from([
                ("strengths", list_in("soar.strengths", &s.strengths)?),
                ("opportunities", list_in("soar.opportunities", &s.opportunities)?),
                ("aspirations", list_in("soar.aspirations", &s.aspirations)?),
                ("results", list_in("soar.results", &s.results)?),
            ])
        }
    };
    if args.stakeholders.len() > MAX_STAKEHOLDERS {
        return Err(PhnError::InvalidInput(format!(
            "{} stakeholders given; keep at most {MAX_STAKEHOLDERS} (group small ones)",
            args.stakeholders.len()
        )));
    }
    if args.priorities.len() > MAX_PRIORITIES {
        return Err(PhnError::InvalidInput(format!(
            "{} strategic priorities given; plans usually have 3 to 6 (at most {MAX_PRIORITIES})",
            args.priorities.len()
        )));
    }

    // ---- analysis scaffold
    let (quads, desc, support, support_conf, tows): (Vec<&Quadrant>, &str, &str, Confidence, Vec<TowsOut>) =
        match method {
            AnalysisMethod::Swot => (
                c.swot.quadrants.iter().collect(),
                c.swot.description.as_str(),
                c.swot.support.as_str(),
                c.swot.support_confidence,
                c.swot
                    .tows_prompts
                    .iter()
                    .map(|t| TowsOut {
                        id: t.id.clone(),
                        label: t.label.clone(),
                        prompt: t.prompt.clone(),
                    })
                    .collect(),
            ),
            AnalysisMethod::Soar => (
                c.soar.quadrants.iter().collect(),
                c.soar.description.as_str(),
                c.soar.support.as_str(),
                c.soar.support_confidence,
                Vec::new(),
            ),
        };
    let quadrants: Vec<QuadrantOut> = quads
        .iter()
        .map(|q| QuadrantOut {
            id: q.id.clone(),
            label: q.label.clone(),
            location: q.location.clone(),
            your_entries: entries.get(q.id.as_str()).cloned().unwrap_or_default(),
            prompts: q.prompts.clone(),
        })
        .collect();
    let entries_total: usize = quadrants.iter().map(|q| q.your_entries.len()).sum();
    let analysis = AnalysisOut {
        method,
        description: tidy(desc),
        support: tidy(support),
        support_confidence: support_conf,
        quadrants,
        strategy_prompts: tows,
        entries_total,
    };

    // ---- stakeholders
    let mut rows: Vec<StakeholderRow> = Vec::new();
    for (i, s) in args.stakeholders.iter().enumerate() {
        let name = text_in("stakeholder name", &s.name)?;
        if name.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "stakeholder {} has no name",
                i + 1
            )));
        }
        let power = rating_in("power", s.power)?;
        let interest = rating_in("interest", s.interest)?;
        let (quadrant, approach) = match (power, interest) {
            (Some(p), Some(i)) => {
                let q = quadrant_of(&c.stakeholder, p, i);
                (q.label.clone(), q.approach.clone())
            }
            _ => (
                "Not rated".to_string(),
                "Rate power and interest (1 to 5) to place this stakeholder on the grid.".to_string(),
            ),
        };
        rows.push(StakeholderRow {
            number: i + 1,
            name,
            power,
            interest,
            quadrant,
            approach,
            group: opt_in("stakeholder group", &s.group)?,
            current_engagement: opt_in("current_engagement", &s.current_engagement)?,
            desired_engagement: opt_in("desired_engagement", &s.desired_engagement)?,
            needs: opt_in("needs", &s.needs)?,
            owner: opt_in("owner", &s.owner)?,
        });
    }
    let quadrant_counts: Vec<(String, usize)> = c
        .stakeholder
        .quadrants
        .iter()
        .map(|q| (q.label.clone(), rows.iter().filter(|r| r.quadrant == q.label).count()))
        .chain(std::iter::once((
            "Not rated".to_string(),
            rows.iter().filter(|r| r.quadrant == "Not rated").count(),
        )))
        .collect();
    let title = match &department {
        Some(d) => format!("Stakeholder power and interest grid: {d}"),
        None => "Stakeholder power and interest grid".to_string(),
    };
    let alt_text = if rows.is_empty() {
        "Stakeholder power and interest grid with no stakeholders yet.".to_string()
    } else {
        format!(
            "Stakeholder power and interest grid with {} stakeholders: {}.",
            rows.len(),
            quadrant_counts
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(q, n)| format!("{n} in {q}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let long_desc = if rows.is_empty() {
        alt_text.clone()
    } else {
        format!(
            "{alt_text} {}",
            rows.iter()
                .map(|r| format!(
                    "{}. {}: {}",
                    r.number,
                    r.name,
                    match (r.power, r.interest) {
                        (Some(p), Some(i)) => format!("power {p}, interest {i}, {}", r.quadrant),
                        _ => "not rated".into(),
                    }
                ))
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    let svg = grid_svg(&c.stakeholder, &title, &rows, &alt_text, &long_desc);

    // ---- priorities
    let mut priorities: Vec<PriorityOut> = Vec::new();
    for (pi, p) in args.priorities.iter().enumerate() {
        if p.goals.len() > MAX_GOALS {
            return Err(PhnError::InvalidInput(format!(
                "priority {} has {} goals; keep at most {MAX_GOALS}",
                pi + 1,
                p.goals.len()
            )));
        }
        let pcode = format!("P{}", pi + 1);
        let mut goals = Vec::new();
        for (gi, g) in p.goals.iter().enumerate() {
            if g.objectives.len() > MAX_ITEMS || g.strategies.len() > MAX_ITEMS {
                return Err(PhnError::InvalidInput(format!(
                    "goal {pcode}.{} has more than {MAX_ITEMS} objectives or strategies; split it",
                    gi + 1
                )));
            }
            let gcode = format!("G{}.{}", pi + 1, gi + 1);
            let mut objectives = Vec::new();
            for (oi, o) in g.objectives.iter().enumerate() {
                let due = opt_in("objective due", &o.due)?;
                if let Some(d) = &due
                    && read_date(d).is_none()
                {
                    return Err(PhnError::InvalidInput(format!(
                        "objective {gcode}.{}: the due date must be YYYY, YYYY-MM, or YYYY-MM-DD (between 1900 and 2100)",
                        oi + 1
                    )));
                }
                objectives.push(ObjectiveOut {
                    code: format!("O{}.{}.{}", pi + 1, gi + 1, oi + 1),
                    statement: text_in("objective", &o.statement)?,
                    measure: opt_in("objective measure", &o.measure)?,
                    baseline: opt_in("objective baseline", &o.baseline)?,
                    target: opt_in("objective target", &o.target)?,
                    due,
                });
            }
            let mut strategies = Vec::new();
            for (si, st) in g.strategies.iter().enumerate() {
                strategies.push(StrategyOut {
                    code: format!("S{}.{}.{}", pi + 1, gi + 1, si + 1),
                    statement: text_in("strategy", &st.statement)?,
                    responsible: opt_in("strategy responsible", &st.responsible)?,
                    timeline: opt_in("strategy timeline", &st.timeline)?,
                    resources: opt_in("strategy resources", &st.resources)?,
                });
            }
            goals.push(GoalOut {
                code: gcode,
                statement: text_in("goal", &g.statement)?,
                objectives,
                strategies,
            });
        }
        priorities.push(PriorityOut {
            code: pcode,
            title: text_in("priority title", &p.title)?,
            rationale: opt_in("priority rationale", &p.rationale)?,
            chip_priority: opt_in("chip_priority", &p.chip_priority)?,
            goals,
        });
    }

    // ---- CHIP link
    let mut chip_rows: Vec<ChipRow> = chip_priorities
        .iter()
        .map(|cp| ChipRow {
            chip_priority: cp.clone(),
            linked_strategic_priorities: priorities
                .iter()
                .filter(|p| p.chip_priority.as_deref().is_some_and(|x| norm(x) == norm(cp)))
                .map(|p| format!("{}: {}", p.code, p.title))
                .collect(),
        })
        .collect();
    chip_rows.dedup_by(|a, b| a.chip_priority == b.chip_priority);
    let unmatched: Vec<String> = priorities
        .iter()
        .filter_map(|p| {
            p.chip_priority.as_ref().and_then(|x| {
                (!chip_priorities.iter().any(|cp| norm(cp) == norm(x)))
                    .then(|| format!("{}: {}", p.code, x))
            })
        })
        .collect();
    let phases: Vec<String> = content::load::<serde_json::Value>(CHA_FRAMEWORK)
        .ok()
        .and_then(|v| {
            v["framework"]["phases"].as_array().map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some(format!(
                            "Phase {}: {}",
                            p["number"].as_i64()?,
                            p["name"].as_str()?
                        ))
                    })
                    .collect()
            })
        })
        .unwrap_or_default();
    let chip = ChipOut {
        title: chip_title,
        rows: chip_rows,
        unmatched_links: unmatched,
        assessment_framework_phases: phases,
        note: format!(
            "{} {}",
            tidy(&c.chip_link.description),
            tidy(&c.chip_link.where_chip_falls_note)
        ),
    };

    // ---- checks
    let text_of = |id: &str| -> String {
        c.checks
            .iter()
            .find(|x| x.id == id)
            .map(|x| x.text.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let mut findings: Vec<Finding> = Vec::new();
    let mut find = |id: &str, severity: &str, detail: String| {
        findings.push(Finding {
            id: id.to_string(),
            severity: severity.to_string(),
            message: if detail.is_empty() {
                text_of(id)
            } else {
                format!("{} {detail}", text_of(id))
            },
        });
    };
    if priorities.is_empty() {
        find("needs_priority", "needs_work", String::new());
    }
    for p in &priorities {
        if p.goals.is_empty() {
            find("needs_goal", "needs_work", format!("({}: {})", p.code, clip(&p.title, 60)));
        }
        for g in &p.goals {
            if g.objectives.is_empty() {
                find("needs_objective", "needs_work", format!("({})", g.code));
            }
            if g.strategies.is_empty() {
                find("objective_no_strategy", "needs_work", format!("({})", g.code));
            }
            for o in &g.objectives {
                let mut missing = Vec::new();
                if o.measure.is_none() {
                    missing.push("measure");
                }
                if o.target.is_none() {
                    missing.push("target");
                }
                if o.due.is_none() {
                    missing.push("date");
                }
                if !missing.is_empty() {
                    find(
                        "objective_not_smart",
                        "needs_work",
                        format!("({}: no {})", o.code, missing.join(", no ")),
                    );
                }
                if let (Some(d), Some(s), Some(e)) = (o.due.as_deref().and_then(read_date), start, end) {
                    use chrono::Datelike;
                    if d.year() < s || d.year() > e {
                        find("dates_out_of_period", "consider", format!("({})", o.code));
                    }
                }
            }
            for st in &g.strategies {
                if st.responsible.is_none() {
                    find("strategy_no_owner", "consider", format!("({})", st.code));
                }
            }
        }
    }
    if entries_total == 0 {
        find("no_assessment", "consider", String::new());
    }
    let strategy_text: String = priorities
        .iter()
        .flat_map(|p| &p.goals)
        .flat_map(|g| &g.strategies)
        .flat_map(|s| {
            [
                Some(s.statement.as_str()),
                s.responsible.as_deref(),
                s.resources.as_deref(),
            ]
        })
        .flatten()
        .map(norm)
        .collect::<Vec<_>>()
        .join(" | ");
    for r in &rows {
        if let (Some(p), Some(i)) = (r.power, r.interest)
            && p > c.stakeholder.high_threshold
            && i > c.stakeholder.high_threshold
            && !strategy_text.contains(&norm(&r.name))
        {
            find("stakeholder_gap", "consider", format!("({})", clip(&r.name, 60)));
        }
    }
    if chip_priorities.is_empty() {
        find("no_chip", "consider", String::new());
    }
    for r in &chip.rows {
        if r.linked_strategic_priorities.is_empty() {
            find("chip_unlinked", "consider", format!("({})", clip(&r.chip_priority, 60)));
        }
    }
    let findings_out = findings;

    // ---- outline
    let has = |id: &str| -> bool {
        match id {
            "purpose" => department.is_some() || period.is_some(),
            "mission_vision_values" => mission.is_some() || vision.is_some() || !values.is_empty(),
            "assessment" => entries_total > 0 || !rows.is_empty(),
            "priorities" => !priorities.is_empty(),
            "chip_link" => !chip_priorities.is_empty(),
            "monitoring" => priorities
                .iter()
                .flat_map(|p| &p.goals)
                .flat_map(|g| &g.objectives)
                .any(|o| o.measure.is_some()),
            _ => false,
        }
    };
    let outline: Vec<OutlineOut> = c
        .plan
        .sections
        .iter()
        .map(|s| OutlineOut {
            label: s.label.clone(),
            guidance: s.guidance.clone(),
            has_content: has(&s.id),
            basis: c
                .sources
                .get(&s.basis)
                .map(|x| x.title.clone())
                .unwrap_or_else(|| s.basis.clone()),
            confidence: s.confidence,
        })
        .collect();

    // ---- documents
    let write = args.write_document.unwrap_or(true);
    let mut files = None;
    let mut files_written = Vec::new();
    if write {
        let png = phn_docs::svg::svg_to_png(&svg, 2.0)?;
        let doc = build_document(
            ctx,
            &c,
            department.as_deref(),
            period.as_deref(),
            mission.as_deref(),
            vision.as_deref(),
            &values,
            &analysis,
            &rows,
            &alt_text,
            png.clone(),
            &priorities,
            &chip,
            &findings_out,
            &outline,
        );
        let (dir, folder) = templates::output_folder(ctx, "strategic_plan")?;
        let docx = templates::write_in(
            ctx,
            &dir,
            "strategic_plan.docx",
            &phn_docs::docx::to_docx(&doc)?,
        )?;
        let svg_file = templates::write_in(ctx, &dir, "stakeholder_grid.svg", svg.as_bytes())?;
        let png_file = templates::write_in(ctx, &dir, "stakeholder_grid.png", &png)?;
        files_written.extend([docx.clone(), svg_file.clone(), png_file.clone()]);
        files = Some(StrategyFiles {
            folder,
            document: docx,
            svg: Some(svg_file),
            png: Some(png_file),
        });
    }

    // ---- response
    let mut s = format!(
        "**Strategic plan scaffold{}{}.** {} analysis with {} entries, {} stakeholders, {} priorities, {} finding(s).\n\n",
        department.as_deref().map(|d| format!(": {d}")).unwrap_or_default(),
        period.as_deref().map(|p| format!(" ({p})")).unwrap_or_default(),
        match method {
            AnalysisMethod::Swot => "SWOT",
            AnalysisMethod::Soar => "SOAR",
        },
        entries_total,
        rows.len(),
        priorities.len(),
        findings_out.len()
    );
    s.push_str(&format!(
        "**PHAB requirements: {}.** {}\n\n",
        c.phab_requirements.status,
        tidy(&c.phab_requirements.statement)
    ));
    let orows: Vec<Vec<String>> = outline
        .iter()
        .map(|o| {
            vec![
                o.label.clone(),
                if o.has_content { "has content" } else { "blank" }.to_string(),
            ]
        })
        .collect();
    s.push_str(&md::table(&["Plan section", "Status"], &orows));
    if !rows.is_empty() {
        s.push_str("\n**Stakeholders by quadrant:** ");
        s.push_str(
            &quadrant_counts
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(q, n)| format!("{q} {n}"))
                .collect::<Vec<_>>()
                .join("; "),
        );
        s.push('\n');
    }
    if !findings_out.is_empty() {
        s.push_str("\n**To fill in or check**\n\n");
        for f in findings_out.iter().take(12) {
            s.push_str(&format!("- {}\n", f.message));
        }
        if findings_out.len() > 12 {
            s.push_str(&format!("- ... and {} more in the response data\n", findings_out.len() - 12));
        }
    }
    if let Some(f) = &files {
        s.push_str(&format!("\nDraft plan and grid written to {}.\n", f.folder));
    }

    // ---- caveats and privacy
    let texts = roster::argument_strings(
        &args,
        &["period_start", "period_end", "method", "write_document", "include_svg"],
    );
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    let mut caveats = vec![
        "This tool organizes what you give it. It adds no facts: every priority, goal, objective, strategy, and stakeholder rating is yours, and the analysis prompts are this project's drafts.".to_string(),
        format!("PHAB requirements are {}: {}", c.phab_requirements.status, tidy(&c.phab_requirements.statement)),
        tidy(&c.checks_note),
        tidy(&c.plan.hierarchy_note),
        format!("{} ({:?} confidence)", tidy(&c.stakeholder.support), c.stakeholder.support_confidence),
        tidy(&c.stakeholder.threshold_note),
        "The CHIP link only connects the priorities you name; it does not check that your strategic plan matches the community health improvement plan.".to_string(),
    ];
    if analysis.method == AnalysisMethod::Swot {
        caveats.push(tidy(&c.swot.tows_note));
    }
    if !found.is_empty() {
        caveats.push(format!(
            "Your inputs appear to contain {}. A strategic plan describes a department, not people: use roles and organization names, and remove personal details before sharing.",
            found.join(", ").to_lowercase()
        ));
    }
    caveats.extend(content::review_caveats(&[STRATEGIC_PLANNING]));
    caveats.extend(c.not_verified.iter().map(|n| format!("Not verified: {n}")));
    let sources: Vec<Source> = content::sources_for(
        [
            "cdc-crosswalk",
            "cdc-drivers",
            "jphmp-2025-plans",
            "stakeholder-review-2000",
            "soar-nurs-2016",
            "cha-framework",
        ],
        &c.sources,
    );
    let phab = PhabStatus {
        status: c.phab_requirements.status.clone(),
        statement: tidy(&c.phab_requirements.statement),
        read_in_sources: content::cite_all(&c.phab_requirements.read_in_sources, &c.sources),
        to_check_in_phab_document: c.phab_requirements.to_check_in_phab_document.clone(),
    };
    Ok(Envelope::new(
        s,
        StrategicPlanResult {
            department,
            period,
            analysis,
            stakeholders: StakeholderOut {
                scale: c.stakeholder.scale.clone(),
                threshold_note: tidy(&c.stakeholder.threshold_note),
                support: tidy(&c.stakeholder.support),
                support_confidence: c.stakeholder.support_confidence,
                rows,
                quadrant_counts,
                alt_text,
                svg: args.include_svg.unwrap_or(false).then_some(svg),
            },
            layers: c
                .plan
                .layers
                .iter()
                .map(|l| format!("{}: {}", l.label, l.definition))
                .collect(),
            priorities,
            chip,
            findings: findings_out,
            outline,
            phab,
            files,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: found,
        files_written,
        notes: vec!["Nothing was read from your files; the draft contains only what you typed.".into()],
        ..Privacy::none()
    }))
}

#[allow(clippy::too_many_arguments)]
fn build_document(
    ctx: &Context,
    c: &StrategicContent,
    department: Option<&str>,
    period: Option<&str>,
    mission: Option<&str>,
    vision: Option<&str>,
    values: &[String],
    analysis: &AnalysisOut,
    stakeholders: &[StakeholderRow],
    alt_text: &str,
    png: Vec<u8>,
    priorities: &[PriorityOut],
    chip: &ChipOut,
    findings: &[Finding],
    outline: &[OutlineOut],
) -> phn_docs::model::Document {
    let blank = "[to be completed]";
    let mut b = DocBuilder::new();
    b.heading(1, "Status of this draft");
    b.para(
        "A draft strategic plan built from content the planning team entered. Items in [brackets] are blank. The tool added no facts.",
    );
    b.labeled(
        "PHAB requirements:",
        format!("{} {}", c.phab_requirements.status, tidy(&c.phab_requirements.statement)),
    );
    b.heading(1, "1. Purpose, period, and how the plan was developed");
    b.labeled("Department:", department.unwrap_or(blank).to_string());
    b.labeled("Planning period:", period.unwrap_or(blank).to_string());
    b.labeled("Who took part and how the plan was adopted:", blank);
    b.heading(1, "2. Mission, vision, and values");
    b.labeled("Mission:", mission.unwrap_or(blank).to_string());
    b.labeled("Vision:", vision.unwrap_or(blank).to_string());
    if values.is_empty() {
        b.labeled("Values:", blank);
    } else {
        b.para("Values:");
        b.list(values);
    }
    b.heading(1, "3. Where we are now");
    b.heading(2, match analysis.method {
        AnalysisMethod::Swot => "SWOT analysis",
        AnalysisMethod::Soar => "SOAR analysis",
    });
    b.para(analysis.description.clone());
    let qrows: Vec<Vec<String>> = analysis
        .quadrants
        .iter()
        .map(|q| {
            vec![
                q.label.clone(),
                if q.your_entries.is_empty() {
                    blank.to_string()
                } else {
                    q.your_entries.join("; ")
                },
                q.prompts.join(" "),
            ]
        })
        .collect();
    b.table("Analysis", &["Quadrant", "Your entries", "Prompts"], qrows);
    if !analysis.strategy_prompts.is_empty() {
        b.para("Strategy prompts that pair the quadrants (answer these in your planning meeting):");
        b.table(
            "Strategy prompts",
            &["Pairing", "Question", "Your answer"],
            analysis
                .strategy_prompts
                .iter()
                .map(|t| vec![t.label.clone(), t.prompt.clone(), blank.to_string()])
                .collect(),
        );
    }
    b.heading(2, "Stakeholder power and interest");
    b.para(c.stakeholder.scale.clone());
    if stakeholders.is_empty() {
        b.para("No stakeholders were entered yet.");
    } else {
        b.blocks.push(Block::Image(ImageBlock {
            path: "stakeholder_grid.png".into(),
            alt: alt_text.to_string(),
            data: Some(png),
        }));
        b.table(
            "Stakeholders",
            &[
                "No.",
                "Stakeholder",
                "Power",
                "Interest",
                "Quadrant",
                "Approach",
                "Current engagement",
                "Desired engagement",
                "What we need",
                "Owner",
            ],
            stakeholders
                .iter()
                .map(|r| {
                    let n = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
                    vec![
                        r.number.to_string(),
                        r.name.clone(),
                        n(r.power),
                        n(r.interest),
                        r.quadrant.clone(),
                        r.approach.clone(),
                        r.current_engagement.clone().unwrap_or_default(),
                        r.desired_engagement.clone().unwrap_or_default(),
                        r.needs.clone().unwrap_or_default(),
                        r.owner.clone().unwrap_or_default(),
                    ]
                })
                .collect(),
        );
    }
    b.heading(1, "4. Strategic priorities, goals, objectives, and strategies");
    if priorities.is_empty() {
        b.para(blank);
    }
    for p in priorities {
        b.heading(2, format!("{} {}", p.code, if p.title.is_empty() { blank } else { &p.title }));
        if let Some(r) = &p.rationale {
            b.labeled("Why this is a priority:", r.clone());
        }
        if let Some(cp) = &p.chip_priority {
            b.labeled("Supports CHIP priority:", cp.clone());
        }
        for g in &p.goals {
            b.heading(3, format!("{} {}", g.code, if g.statement.is_empty() { blank } else { &g.statement }));
            if !g.objectives.is_empty() {
                b.table(
                    &format!("{} objectives", g.code),
                    &["Code", "Objective", "Measure", "Baseline", "Target", "Due"],
                    g.objectives
                        .iter()
                        .map(|o| {
                            let f = |v: &Option<String>| v.clone().unwrap_or_else(|| "[ ]".into());
                            vec![
                                o.code.clone(),
                                o.statement.clone(),
                                f(&o.measure),
                                f(&o.baseline),
                                f(&o.target),
                                f(&o.due),
                            ]
                        })
                        .collect(),
                );
            }
            if !g.strategies.is_empty() {
                b.table(
                    &format!("{} strategies", g.code),
                    &["Code", "Strategy", "Responsible", "Timeline", "Resources"],
                    g.strategies
                        .iter()
                        .map(|s| {
                            let f = |v: &Option<String>| v.clone().unwrap_or_else(|| "[ ]".into());
                            vec![
                                s.code.clone(),
                                s.statement.clone(),
                                f(&s.responsible),
                                f(&s.timeline),
                                f(&s.resources),
                            ]
                        })
                        .collect(),
                );
            }
        }
    }
    b.heading(1, "5. Link to the community health improvement plan");
    b.labeled("CHIP:", chip.title.clone().unwrap_or_else(|| blank.to_string()));
    if chip.rows.is_empty() {
        b.para(blank);
    } else {
        b.table(
            "CHIP link",
            &["CHIP priority", "Strategic priorities that support it"],
            chip.rows
                .iter()
                .map(|r| {
                    vec![
                        r.chip_priority.clone(),
                        if r.linked_strategic_priorities.is_empty() {
                            "[none linked yet]".to_string()
                        } else {
                            r.linked_strategic_priorities.join("; ")
                        },
                    ]
                })
                .collect(),
        );
    }
    b.para(chip.note.clone());
    b.heading(1, "6. How progress will be tracked and reported");
    b.para("Indicators, data sources, who reports, how often, and to whom (for example your board of health): [to be completed]");
    b.heading(1, "7. Related plans");
    b.para("Quality improvement plan, workforce development plan, emergency operations plan, and the CHIP, and how this plan connects to each: [to be completed]");
    b.heading(1, "Checks before you share this draft");
    if findings.is_empty() {
        b.para("No blanks were found. This is not a quality review.");
    } else {
        b.list(&findings.iter().map(|f| f.message.clone()).collect::<Vec<_>>());
    }
    b.heading(1, "Plan outline and what each section rests on");
    b.table(
        "Plan outline",
        &["Section", "Guidance", "Status", "Basis", "Confidence"],
        outline
            .iter()
            .map(|o| {
                vec![
                    o.label.clone(),
                    o.guidance.clone(),
                    if o.has_content { "has content" } else { "blank" }.into(),
                    o.basis.clone(),
                    format!("{:?}", o.confidence).to_lowercase(),
                ]
            })
            .collect(),
    );
    b.heading(1, "PHAB: what to check in PHAB's own document");
    b.list(&c.phab_requirements.to_check_in_phab_document);
    b.heading(2, "What was read");
    let read: Vec<String> = content::cite_all(&c.phab_requirements.read_in_sources, &c.sources)
        .iter()
        .map(|x| format!("{} ({})", x.text, x.citation))
        .collect();
    b.list(&read);
    b.finish(
        ctx,
        &match department {
            Some(d) => format!("Strategic plan (draft): {d}"),
            None => "Strategic plan (draft)".to_string(),
        },
        period.map(|p| format!("Planning period {p}")),
    )
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

    fn st(name: &str, p: f64, i: f64) -> StakeholderIn {
        StakeholderIn {
            name: name.into(),
            power: Some(p),
            interest: Some(i),
            ..StakeholderIn::default()
        }
    }

    #[test]
    fn quadrants_follow_the_threshold() {
        let (_d, ctx) = ctx();
        let r = build_strategic_plan(
            &ctx,
            StrategicPlanArgs {
                stakeholders: vec![
                    st("Board of health", 5.0, 5.0),
                    st("Select board", 5.0, 2.0),
                    st("Residents' group", 2.0, 4.0),
                    st("Distant agency", 1.0, 1.0),
                    st("On the line", 3.0, 3.0),
                    StakeholderIn {
                        name: "Unrated".into(),
                        ..StakeholderIn::default()
                    },
                ],
                write_document: Some(false),
                include_svg: Some(true),
                ..StrategicPlanArgs::default()
            },
        )
        .unwrap();
        let q = |n: &str| {
            r.data
                .stakeholders
                .rows
                .iter()
                .find(|x| x.name == n)
                .unwrap()
                .quadrant
                .clone()
        };
        assert_eq!(q("Board of health"), "Manage closely");
        assert_eq!(q("Select board"), "Keep satisfied");
        assert_eq!(q("Residents' group"), "Keep informed");
        assert_eq!(q("Distant agency"), "Monitor");
        assert_eq!(q("On the line"), "Monitor");
        assert_eq!(q("Unrated"), "Not rated");
        let svg = r.data.stakeholders.svg.unwrap();
        assert!(svg.contains("role=\"img\"") && svg.contains("<title") && svg.contains("<desc"));
        // Straight apostrophes are escaped, and no script or external reference is present.
        assert!(!svg.contains("<script") && !svg.contains("href="));
        assert!(svg.contains("Residents&#39; group") || svg.contains("Residents' group") || svg.contains("Residents&apos; group"));
        // Unrated stakeholders are listed in the key but not drawn.
        assert!(svg.contains("not rated, not drawn"));
    }

    #[test]
    fn checks_find_blanks_and_chip_links() {
        let (_d, ctx) = ctx();
        let r = build_strategic_plan(
            &ctx,
            StrategicPlanArgs {
                department: Some("Example Health Department".into()),
                period_start: Some(2027),
                period_end: Some(2031),
                chip_priorities: vec!["Behavioral health".into(), "Housing".into()],
                priorities: vec![
                    PriorityIn {
                        title: "Strengthen the workforce".into(),
                        chip_priority: Some("behavioral health".into()),
                        goals: vec![GoalIn {
                            statement: "Fill vacant positions".into(),
                            objectives: vec![ObjectiveIn {
                                statement: "Fill two nurse positions".into(),
                                measure: Some("Positions filled".into()),
                                target: Some("2".into()),
                                due: Some("2035-06".into()),
                                ..ObjectiveIn::default()
                            }],
                            strategies: vec![],
                        }],
                    },
                    PriorityIn {
                        title: "Empty priority".into(),
                        chip_priority: Some("Transportation".into()),
                        ..PriorityIn::default()
                    },
                ],
                stakeholders: vec![st("Mayor's office", 5.0, 5.0)],
                write_document: Some(false),
                ..StrategicPlanArgs::default()
            },
        )
        .unwrap();
        let ids: Vec<&str> = r.data.findings.iter().map(|f| f.id.as_str()).collect();
        for want in [
            "needs_goal",
            "objective_no_strategy",
            "dates_out_of_period",
            "no_assessment",
            "stakeholder_gap",
            "chip_unlinked",
        ] {
            assert!(ids.contains(&want), "{want} missing from {ids:?}");
        }
        // The objective has a measure, target, and date, so it is not flagged as incomplete.
        assert!(!ids.contains(&"objective_not_smart"));
        // Linking is case-insensitive; the unmatched link is reported.
        assert_eq!(r.data.chip.rows[0].linked_strategic_priorities.len(), 1);
        assert_eq!(r.data.chip.rows[1].linked_strategic_priorities.len(), 0);
        assert_eq!(r.data.chip.unmatched_links, vec!["P2: Transportation"]);
        assert!(!r.data.chip.assessment_framework_phases.is_empty());
        assert_eq!(r.data.phab.status, "unverified");
        assert_eq!(r.data.priorities[0].goals[0].objectives[0].code, "O1.1.1");
    }

    #[test]
    fn writes_the_draft_and_the_grid() {
        let (dir, ctx) = ctx();
        let r = build_strategic_plan(
            &ctx,
            StrategicPlanArgs {
                method: Some(AnalysisMethod::Soar),
                soar: Some(SoarInput {
                    strengths: vec!["Trusted nurses".into()],
                    ..SoarInput::default()
                }),
                stakeholders: vec![st("Hospital", 4.0, 4.0), st("Clinic", 4.0, 4.0)],
                write_document: Some(true),
                ..StrategicPlanArgs::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.analysis.method, AnalysisMethod::Soar);
        assert!(r.data.analysis.strategy_prompts.is_empty());
        let f = r.data.files.unwrap();
        for rel in [Some(f.document), f.svg, f.png].into_iter().flatten() {
            assert!(dir.path().join("data").join(&rel).exists(), "{rel}");
        }
    }

    #[test]
    fn inputs_are_checked() {
        let (_d, ctx) = ctx();
        let bad = |args: StrategicPlanArgs| build_strategic_plan(&ctx, args).unwrap_err().to_string();
        assert!(bad(StrategicPlanArgs {
            period_start: Some(1850),
            ..StrategicPlanArgs::default()
        })
        .contains("1900-2100"));
        assert!(bad(StrategicPlanArgs {
            period_start: Some(2030),
            period_end: Some(2027),
            ..StrategicPlanArgs::default()
        })
        .contains("before"));
        assert!(bad(StrategicPlanArgs {
            stakeholders: vec![st("X", 9.0, 1.0)],
            ..StrategicPlanArgs::default()
        })
        .contains("1 to 5"));
        assert!(bad(StrategicPlanArgs {
            swot: Some(SwotInput::default()),
            soar: Some(SoarInput::default()),
            ..StrategicPlanArgs::default()
        })
        .contains("choose one"));
        assert!(bad(StrategicPlanArgs {
            priorities: vec![PriorityIn {
                title: "P".into(),
                goals: vec![GoalIn {
                    statement: "G".into(),
                    objectives: vec![ObjectiveIn {
                        statement: "O".into(),
                        due: Some("soon".into()),
                        ..ObjectiveIn::default()
                    }],
                    strategies: vec![],
                }],
                ..PriorityIn::default()
            }],
            ..StrategicPlanArgs::default()
        })
        .contains("due date"));
        assert!(bad(StrategicPlanArgs {
            mission: Some("x".repeat(700)),
            ..StrategicPlanArgs::default()
        })
        .contains("600 characters"));
    }

    #[test]
    fn contact_details_in_inputs_are_reported_not_echoed_as_findings() {
        let (_d, ctx) = ctx();
        let r = build_strategic_plan(
            &ctx,
            StrategicPlanArgs {
                stakeholders: vec![StakeholderIn {
                    name: "Coalition".into(),
                    power: Some(3.0),
                    interest: Some(4.0),
                    owner: Some("call 413-555-0199".into()),
                    ..StakeholderIn::default()
                }],
                write_document: Some(false),
                ..StrategicPlanArgs::default()
            },
        )
        .unwrap();
        assert!(r.privacy.identifiers_detected.iter().any(|k| k.to_lowercase().contains("phone")));
    }
}
