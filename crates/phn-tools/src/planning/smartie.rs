//! PLAN-02 `write_smartie_objectives`: SMARTIE (specific, measurable, achievable, relevant,
//! time-bound, inclusive, equitable) objectives. The tool assembles a statement from the
//! parts the nurse gives, checks each objective with a structured checker (baseline, target,
//! unit, date, population, data source, equity dimension, direction), and can look up
//! Healthy People 2030 objectives with their national baselines and targets (live from the
//! objective's ODPHP page, else the bundled dated snapshot).
//!
//! The elements and their questions are CDC's (`content/planning/smartie.yaml`). What the
//! checker needs for each element is this project's reading of those questions. The checker
//! counts what is present; whether a target is achievable, relevant, inclusive, or equitable
//! is the nurse's and the community's judgment, and the tool says so.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::healthy_people::{self, HpSnapshot, Objective};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, DocSource};
use crate::epr::roster;
use crate::md;
use crate::school::templates::{self, DocFormat};

/// Content id of `content/planning/smartie.yaml`.
pub const SMARTIE: &str = "planning-smartie";

const MAX_OBJECTIVES: usize = 20;
const MAX_TEXT_CHARS: usize = 1500;
const MAX_HP_QUERIES: usize = 6;
/// Live page fetches per call (each is one small web page).
const MAX_LIVE_FETCHES: usize = 8;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Element {
    id: String,
    letter: String,
    label: String,
    cdc_name: String,
    cdc_question: String,
    generalized: Option<String>,
    fields: Vec<String>,
    judgment: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Pair {
    strategy: String,
    traditional: String,
    smartie: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CdcExamples {
    pairs: Vec<Pair>,
}

#[derive(Debug, Clone, Deserialize)]
struct EquityPrompts {
    dimensions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UseNote {
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct HpNotes {
    use_notes: Vec<UseNote>,
}

/// `content/planning/smartie.yaml`.
#[derive(Debug, Clone, Deserialize)]
struct SmartieContent {
    sources: BTreeMap<String, DocSource>,
    elements: Vec<Element>,
    cdc_examples: CdcExamples,
    equity_prompts: EquityPrompts,
    healthy_people: HpNotes,
    check_messages: BTreeMap<String, String>,
}

// ---------------------------------------------------------------- arguments and results

/// Direction of the change an objective aims for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Increase,
    Decrease,
    Maintain,
}

impl Direction {
    fn verb(self) -> &'static str {
        match self {
            Direction::Increase => "increase",
            Direction::Decrease => "decrease",
            Direction::Maintain => "maintain",
        }
    }
}

/// Process or outcome objective (CDC's evaluation types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveKind {
    /// About how well the program is implemented (reach, participation, quality).
    Process,
    /// About the changes the program is meant to produce.
    Outcome,
}

/// One objective, in parts. Give what you know; the checker lists what is missing.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ObjectiveInput {
    /// The objective as the nurse or the AI worded it, if there is a draft. It is checked for
    /// a number and a date and kept beside the statement the tool assembles from the parts.
    pub statement: Option<String>,
    /// "process" (how well the program is carried out) or "outcome" (the change it aims for).
    pub objective_type: Option<ObjectiveKind>,
    /// What is measured: "proportion of adults aged 50 to 75 up to date on colorectal cancer
    /// screening", "number of home visits completed".
    pub measure: Option<String>,
    /// "increase", "decrease", or "maintain".
    pub direction: Option<Direction>,
    /// The population of focus: who, and where if it is narrower than the whole jurisdiction.
    pub population: Option<String>,
    /// Place (town, county, neighborhood), when it is not part of the population text.
    pub geography: Option<String>,
    /// The starting value (a number).
    pub baseline: Option<f64>,
    /// The period or date of the baseline ("2025", "FY2025", "Jan-Dec 2025").
    pub baseline_period: Option<String>,
    /// The value to reach (a number).
    pub target: Option<f64>,
    /// The unit of the baseline and target: "percent", "per 100,000", "visits", "people".
    pub unit: Option<String>,
    /// When the target should be reached: YYYY-MM-DD, YYYY-MM, or a four-digit year.
    pub target_date: Option<String>,
    /// When the work starts (YYYY-MM-DD, YYYY-MM, or a year); the target date must follow it.
    pub start_date: Option<String>,
    /// Dates or points where progress will be reviewed ("quarterly", "2026-06-30 interim
    /// review"). CDC asks whether there are review points.
    pub review_points: Vec<String>,
    /// Where the baseline and later values come from: program records, a survey, a
    /// surveillance system, a registry.
    pub data_source: Option<String>,
    /// Why this target is reachable: resources, past results, or evidence.
    pub achievability_basis: Option<String>,
    /// Why it matters: the priority, funder requirement, assessment finding, or Healthy
    /// People objective it supports.
    pub relevance: Option<String>,
    /// How people in the population of focus and community partners gave input.
    pub inclusion: Option<String>,
    /// The group or gap the objective aims to narrow ("Spanish-speaking parents, whose visit
    /// rate is lower than the town's").
    pub equity_dimension: Option<String>,
    /// The groups results will be broken down by ("race and ethnicity", "language", "age group").
    pub breakdown_by: Vec<String>,
    /// A Healthy People 2030 objective ID this objective supports ("D-04", "IID-09").
    pub healthy_people_id: Option<String>,
    /// Role or position responsible (not a person's contact details).
    pub responsible: Option<String>,
}

/// Arguments for `write_smartie_objectives`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SmartieArgs {
    /// The broader goal the objectives serve (one sentence), for the document.
    pub goal: Option<String>,
    /// Objectives to assemble and check (up to 20). Leave empty to only look up Healthy People
    /// 2030 objectives.
    pub objectives: Vec<ObjectiveInput>,
    /// Healthy People 2030 lookups: topics or keywords ("diabetes eye exam", "flu vaccine") or
    /// objective IDs ("D-04"). Each returns the closest objectives with their national
    /// baselines and targets.
    pub healthy_people_queries: Vec<String>,
    /// Matches returned per lookup (default 5, at most 10).
    pub max_matches: Option<u32>,
    /// Re-read each matched objective's page on odphp.health.gov (default true). When the
    /// site cannot be reached, the bundled dated snapshot is used and the response says so.
    pub refresh_live: Option<bool>,
    /// Write the objectives worksheet to /data/output (default false).
    pub write_document: Option<bool>,
    /// Document format: "docx" (default) or "pdf".
    pub format: Option<DocFormat>,
}

/// How an element came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ElementStatus {
    /// Everything the checker looks for is present and consistent.
    Met,
    /// Information is given; whether it is enough is your judgment (achievable, relevant,
    /// inclusive, equitable).
    ProvidedForJudgment,
    /// Something the element needs is missing.
    Missing,
    /// Information is given but something looks wrong (a date that cannot be read, a target
    /// that moves the wrong way).
    Concern,
}

/// One element's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElementResult {
    pub id: String,
    /// "S", "M", "A", "R", "T", "I", or "E".
    pub letter: String,
    pub label: String,
    pub status: ElementStatus,
    /// What is missing or looks wrong, and what to decide.
    pub notes: Vec<String>,
    /// Structured fields the checker looks for in this element that were not given.
    pub missing_fields: Vec<String>,
    /// CDC's question for this element.
    pub question: String,
}

/// Arithmetic between baseline and target (no clinical meaning).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChangeOut {
    /// Target minus baseline, in the unit given (percentage points when the unit is percent).
    pub absolute_change: f64,
    /// Change as a percent of the baseline (not given when the baseline is 0).
    pub relative_change_percent: Option<f64>,
    pub description: String,
}

/// An objective linked to Healthy People 2030, with how the two targets compare.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HpLink {
    pub objective: HpObjectiveOut,
    /// How your target compares with the national target (given only when both are in the same
    /// unit and the objective is a core objective with a target).
    pub comparison: Option<String>,
    /// Notes: a different direction, a unit that cannot be compared, a developmental objective.
    pub notes: Vec<String>,
}

/// One objective's check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectiveCheck {
    pub number: usize,
    pub objective_type: Option<ObjectiveKind>,
    /// The statement assembled from the parts; missing parts appear in [brackets].
    pub drafted_statement: String,
    /// True when no part of the drafted statement is a placeholder.
    pub statement_complete: bool,
    /// The draft you gave, if any.
    pub original_statement: Option<String>,
    pub elements: Vec<ElementResult>,
    /// How many of the three elements the checker can confirm without your judgment (specific,
    /// measurable, and time-bound) are met. The other four are for you to judge.
    pub elements_met: usize,
    /// Short flags: missing_baseline, missing_target, missing_date, missing_population,
    /// missing_data_source, missing_equity_dimension, and so on.
    pub flags: Vec<String>,
    pub change: Option<ChangeOut>,
    /// The target date as read (YYYY-MM-DD; a year alone is read as December 31, a month as the
    /// last day of the month).
    pub target_date_read: Option<String>,
    pub healthy_people: Option<HpLink>,
}

/// A Healthy People 2030 objective as returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HpObjectiveOut {
    pub id: String,
    pub title: String,
    /// core, developmental, research, or unknown.
    pub objective_type: String,
    pub status: Option<String>,
    pub topics: Vec<String>,
    pub desired_direction: Option<String>,
    /// National baseline as written, with its year.
    pub national_baseline: Option<String>,
    pub national_baseline_value: Option<f64>,
    pub national_baseline_year: Option<String>,
    /// National target as written.
    pub national_target: Option<String>,
    pub national_target_value: Option<f64>,
    pub most_recent: Option<String>,
    pub age_adjusted: bool,
    pub data_source: Option<String>,
    pub target_setting_method: Option<String>,
    pub leading_health_indicator: bool,
    /// Notes on the page, such as "baseline has been revised".
    pub revised: Vec<String>,
    pub url: String,
    /// "live", "cache", or "snapshot".
    pub mode: String,
    /// Date of the data used (the page date for live or cached, else the snapshot date).
    pub as_of: String,
    /// What changed since the bundled snapshot, when a live page differs.
    pub changed_since_snapshot: Vec<String>,
}

/// One Healthy People lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HpLookup {
    pub query: String,
    pub matches: Vec<HpObjectiveOut>,
    pub note: Option<String>,
}

/// Healthy People data used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HpInfo {
    pub snapshot_date: String,
    pub objectives_in_snapshot: usize,
    pub live_pages_read: usize,
    pub live_unavailable_reason: Option<String>,
    pub notes: Vec<String>,
}

/// An example pair from CDC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExamplePair {
    pub strategy: String,
    pub traditional: String,
    pub smartie: String,
}

/// Result of `write_smartie_objectives`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SmartieResult {
    pub goal: Option<String>,
    pub objectives: Vec<ObjectiveCheck>,
    /// Across all objectives: how many have each gap.
    pub gaps: BTreeMap<String, usize>,
    pub healthy_people_lookups: Vec<HpLookup>,
    pub healthy_people: Option<HpInfo>,
    /// CDC's questions for the seven elements.
    pub elements: Vec<ElementRef>,
    /// Equity dimensions to consider (examples; your community assessment decides).
    pub equity_dimension_prompts: Vec<String>,
    /// CDC's traditional versus SMARTIE examples, for wording.
    pub cdc_examples: Vec<ExamplePair>,
    pub file: Option<String>,
}

/// An element and its CDC question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElementRef {
    pub letter: String,
    pub label: String,
    /// The name CDC's sheet uses for this letter.
    pub cdc_name: String,
    pub question: String,
}

// ---------------------------------------------------------------- helpers

fn tidy(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(|x| x.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|x| !x.is_empty())
}

fn check_len(what: &str, s: &Option<String>) -> Result<()> {
    if let Some(t) = s
        && t.chars().count() > MAX_TEXT_CHARS
    {
        return Err(PhnError::InvalidInput(format!(
            "{what} is longer than {MAX_TEXT_CHARS} characters; use a sentence or two"
        )));
    }
    Ok(())
}

/// Read a date written YYYY-MM-DD, YYYY-MM, YYYY, or M/D/YYYY (four-digit year). A year alone
/// is read as December 31 and a month as its last day. Years outside 1900-2100 are refused.
pub fn read_date(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    let ok_year = |y: i32| (1900..=2100).contains(&y);
    let last_day = |y: i32, m: u32| -> Option<NaiveDate> {
        let next = if m == 12 {
            NaiveDate::from_ymd_opt(y + 1, 1, 1)?
        } else {
            NaiveDate::from_ymd_opt(y, m + 1, 1)?
        };
        next.pred_opt()
    };
    let parts: Vec<&str> = t.split('-').collect();
    match parts.as_slice() {
        [y] if y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()) => {
            let y: i32 = y.parse().ok()?;
            ok_year(y).then(|| NaiveDate::from_ymd_opt(y, 12, 31))?
        }
        [y, m] if y.len() == 4 => {
            let (y, m): (i32, u32) = (y.parse().ok()?, m.parse().ok()?);
            if !ok_year(y) || !(1..=12).contains(&m) {
                return None;
            }
            last_day(y, m)
        }
        [y, m, d] if y.len() == 4 => {
            let (y, m, d): (i32, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
            if !ok_year(y) {
                return None;
            }
            NaiveDate::from_ymd_opt(y, m, d)
        }
        _ => {
            let sl: Vec<&str> = t.split('/').collect();
            if let [m, d, y] = sl.as_slice()
                && y.len() == 4
            {
                let (m, d, y): (u32, u32, i32) = (m.parse().ok()?, d.parse().ok()?, y.parse().ok()?);
                if !ok_year(y) {
                    return None;
                }
                return NaiveDate::from_ymd_opt(y, m, d);
            }
            None
        }
    }
}

fn fmt_num(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round() as i64)
    } else {
        let s = format!("{x:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Unit class used to decide when two values can be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitClass {
    Percent,
    Per100k,
    Per10k,
    Per1k,
    Other,
}

fn unit_class(s: &str) -> UnitClass {
    let t = s.to_lowercase();
    let t = t.replace(',', "");
    if t.contains("100000") || t.contains("100k") {
        UnitClass::Per100k
    } else if t.contains("10000") {
        UnitClass::Per10k
    } else if t.contains("per 1000") || t.contains("/1000") {
        UnitClass::Per1k
    } else if t.contains('%') || t.contains("percent") {
        UnitClass::Percent
    } else {
        UnitClass::Other
    }
}

fn with_unit(v: f64, unit: Option<&str>) -> String {
    match unit {
        None => fmt_num(v),
        Some(u) if unit_class(u) == UnitClass::Percent => format!("{}%", fmt_num(v)),
        Some(u) => format!("{} {}", fmt_num(v), u.trim()),
    }
}

fn hp_out(o: &Objective, mode: &str, as_of: &str, changed: Vec<String>, lhi: bool) -> HpObjectiveOut {
    HpObjectiveOut {
        id: o.id.clone(),
        title: o.title.clone(),
        objective_type: o.objective_type.clone(),
        status: o.status.clone(),
        topics: o.topics.clone(),
        desired_direction: o.desired_direction.clone(),
        national_baseline: o.baseline_text.clone(),
        national_baseline_value: o.baseline_value,
        national_baseline_year: o.baseline_year.clone(),
        national_target: o.target_text.clone(),
        national_target_value: o.target_value,
        most_recent: o.most_recent_text.clone(),
        age_adjusted: o.age_adjusted,
        data_source: o.data_source.clone(),
        target_setting_method: o.target_setting_method.clone(),
        leading_health_indicator: lhi || o.leading_health_indicator,
        revised: o.revised.clone(),
        url: o.url.clone(),
        mode: mode.into(),
        as_of: as_of.into(),
        changed_since_snapshot: changed,
    }
}

fn diff(snap: &Objective, live: &Objective) -> Vec<String> {
    let mut v = Vec::new();
    if snap.target_text != live.target_text {
        v.push(format!(
            "national target: {} (snapshot) -> {} (page)",
            snap.target_text.as_deref().unwrap_or("none"),
            live.target_text.as_deref().unwrap_or("none")
        ));
    }
    if snap.baseline_text != live.baseline_text {
        v.push("national baseline wording or value differs from the snapshot".into());
    }
    if snap.most_recent_text != live.most_recent_text {
        v.push(format!(
            "most recent data: {} (snapshot) -> {} (page)",
            snap.most_recent_text.as_deref().unwrap_or("none"),
            live.most_recent_text.as_deref().unwrap_or("none")
        ));
    }
    if snap.status != live.status {
        v.push(format!(
            "national status: {} (snapshot) -> {} (page)",
            snap.status.as_deref().unwrap_or("none"),
            live.status.as_deref().unwrap_or("none")
        ));
    }
    v
}

/// Reads Healthy People objectives through snapshot-first lookups with optional live refresh.
struct HpReader<'a> {
    ctx: &'a Context,
    snap: &'a HpSnapshot,
    refresh: bool,
    live_reads: usize,
    attempts: usize,
    failure: Option<String>,
    sources: Vec<Source>,
    cache: BTreeMap<String, HpObjectiveOut>,
}

impl<'a> HpReader<'a> {
    async fn objective(&mut self, o: &Objective) -> HpObjectiveOut {
        if let Some(x) = self.cache.get(&o.id) {
            return x.clone();
        }
        let snap_as_of = self.snap.retrieved.clone();
        let mut out = hp_out(o, "snapshot", &snap_as_of, Vec::new(), false);
        if self.refresh
            && self.failure.is_none()
            && self.attempts < MAX_LIVE_FETCHES
            && !self.ctx.http.is_offline()
        {
            self.attempts += 1;
            match healthy_people::fetch_objective(&self.ctx.http, self.ctx.cache.as_ref(), &o.url)
                .await
            {
                Ok((live, fetched)) => {
                    let as_of = phn_data::fetch::local_date(&fetched.as_of);
                    if live.id == o.id {
                        let changed = diff(o, &live);
                        out = hp_out(
                            &live,
                            fetched.mode.label(),
                            &as_of,
                            changed,
                            o.leading_health_indicator,
                        );
                        self.live_reads += 1;
                        self.sources.push(Source {
                            title: format!("Healthy People 2030 objective {}: {}", live.id, live.title),
                            publisher:
                                "Office of Disease Prevention and Health Promotion (ODPHP), HHS"
                                    .into(),
                            url: Some(live.url.clone()),
                            effective_date: None,
                            retrieved: Some(as_of),
                            mode: fetched.mode,
                        });
                    } else {
                        self.failure = Some(format!(
                            "the page for {} returned a different objective ({})",
                            o.id, live.id
                        ));
                    }
                }
                Err(e) => {
                    self.failure = Some(phn_data::apifetch::reason(&e));
                }
            }
        }
        self.cache.insert(o.id.clone(), out.clone());
        out
    }
}

fn hp_link(o: &ObjectiveInput, hp: HpObjectiveOut, dir: Option<Direction>) -> HpLink {
    let mut notes = Vec::new();
    let mut comparison = None;
    if hp.objective_type != "core" {
        notes.push(format!(
            "This is a {} objective: it has no national target to compare with.",
            hp.objective_type
        ));
    }
    if let (Some(d), Some(hd)) = (dir, hp.desired_direction.as_deref()) {
        let want = hd.to_lowercase();
        let differs = match d {
            Direction::Increase => !want.starts_with("increase"),
            Direction::Decrease => !want.starts_with("decrease"),
            Direction::Maintain => !want.starts_with("maintain"),
        };
        if differs {
            notes.push(format!(
                "Your objective aims to {} but Healthy People's desired direction is \"{hd}\": check that you measure the same thing.",
                d.verb()
            ));
        }
    }
    if let (Some(t), Some(ht), Some(unit), Some(htext)) = (
        o.target,
        hp.national_target_value,
        o.unit.as_deref(),
        hp.national_target.as_deref(),
    ) {
        let (mine, theirs) = (unit_class(unit), unit_class(htext));
        if mine == theirs && mine != UnitClass::Other {
            let hd = hp.desired_direction.clone().unwrap_or_default().to_lowercase();
            let rel = if (t - ht).abs() < 1e-9 {
                "equal to"
            } else if hd.starts_with("decrease") {
                if t < ht { "better than (lower than)" } else { "short of (higher than)" }
            } else if hd.starts_with("increase") {
                if t > ht { "better than (higher than)" } else { "short of (lower than)" }
            } else if t > ht {
                "higher than"
            } else {
                "lower than"
            };
            comparison = Some(format!(
                "Your target ({}) is {rel} the national Healthy People 2030 target ({htext}).",
                with_unit(t, Some(unit))
            ));
        } else {
            notes.push(
                "Your unit and the national target's unit are not the same kind, so the two targets are not compared."
                    .into(),
            );
        }
    }
    if hp.age_adjusted {
        notes.push(
            "The national values are age-adjusted to the year 2000 standard population; a crude local rate is not directly comparable."
                .into(),
        );
    }
    HpLink {
        objective: hp,
        comparison,
        notes,
    }
}

// ---------------------------------------------------------------- the checker

struct Checked {
    check: ObjectiveCheck,
}

#[allow(clippy::too_many_lines)]
fn check_objective(
    c: &SmartieContent,
    n: usize,
    o: &ObjectiveInput,
    today: NaiveDate,
) -> Checked {
    let msg = |k: &str| c.check_messages.get(k).cloned().unwrap_or_else(|| k.to_string());
    let measure = tidy(&o.measure);
    let population = tidy(&o.population);
    let geography = tidy(&o.geography);
    let data_source = tidy(&o.data_source);
    let unit = tidy(&o.unit);
    let mut flags: Vec<String> = Vec::new();
    let mut flag = |f: &str| {
        if !flags.iter().any(|x| x == f) {
            flags.push(f.to_string());
        }
    };

    // Dates.
    let target_date_raw = tidy(&o.target_date);
    let start_raw = tidy(&o.start_date);
    let target_date = target_date_raw.as_deref().and_then(read_date);
    let start_date = start_raw.as_deref().and_then(read_date);

    // Element notes and fields.
    let mut results: Vec<ElementResult> = Vec::new();
    for e in &c.elements {
        let mut notes: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        let mut concern = false;
        let present = |f: &str| -> bool {
            match f {
                "measure" => measure.is_some(),
                "direction" => o.direction.is_some(),
                "population" => population.is_some(),
                "baseline" => o.baseline.is_some(),
                "target" => o.target.is_some(),
                "data_source" => data_source.is_some(),
                "target_date" => target_date_raw.is_some(),
                "achievability_basis" => tidy(&o.achievability_basis).is_some(),
                "relevance" => tidy(&o.relevance).is_some() || tidy(&o.healthy_people_id).is_some(),
                "inclusion" => tidy(&o.inclusion).is_some(),
                "equity_dimension" => tidy(&o.equity_dimension).is_some(),
                _ => false,
            }
        };
        for f in &e.fields {
            if !present(f) {
                missing.push(f.clone());
            }
        }
        match e.id.as_str() {
            "specific" => {
                if measure.is_none() {
                    notes.push(msg("specific_missing_measure"));
                    flag("missing_measure");
                }
                if o.direction.is_none() {
                    notes.push(msg("specific_missing_direction"));
                    flag("missing_direction");
                }
                if population.is_none() {
                    notes.push(msg("specific_missing_population"));
                    flag("missing_population");
                }
            }
            "measurable" => {
                if o.baseline.is_none() {
                    notes.push(msg("measurable_missing_baseline"));
                    flag("missing_baseline");
                }
                if o.target.is_none() {
                    notes.push(msg("measurable_missing_target"));
                    flag("missing_target");
                }
                if data_source.is_none() {
                    notes.push(msg("measurable_missing_data_source"));
                    flag("missing_data_source");
                }
                if o.target.is_some() && unit.is_none() {
                    notes.push(msg("measurable_missing_unit"));
                    missing.push("unit".into());
                    flag("missing_unit");
                }
                if let (Some(b), Some(t), Some(d)) = (o.baseline, o.target, o.direction) {
                    let wrong = match d {
                        Direction::Increase => t < b,
                        Direction::Decrease => t > b,
                        Direction::Maintain => false,
                    };
                    if wrong {
                        notes.push(msg("direction_mismatch"));
                        flag("direction_mismatch");
                        concern = true;
                    }
                    if (t - b).abs() < 1e-12 && d != Direction::Maintain {
                        notes.push(msg("target_equals_baseline"));
                        flag("target_equals_baseline");
                        concern = true;
                    }
                }
            }
            "time_bound" => {
                if target_date_raw.is_none() {
                    notes.push(msg("time_bound_missing_date"));
                    flag("missing_date");
                } else if target_date.is_none() {
                    notes.push(msg("time_bound_date_unreadable"));
                    flag("date_unreadable");
                    concern = true;
                }
                if let (Some(t), Some(s)) = (target_date, start_date)
                    && t <= s
                {
                    notes.push(msg("time_bound_date_before_start"));
                    flag("date_before_start");
                    concern = true;
                }
                if start_raw.is_some() && start_date.is_none() {
                    notes.push("The start date could not be read; use YYYY-MM-DD, YYYY-MM, or a four-digit year.".into());
                    concern = true;
                }
                if let Some(t) = target_date
                    && t < today
                {
                    notes.push(format!(
                        "The target date ({t}) is in the past; confirm it is not a typing error."
                    ));
                }
                if o.review_points.iter().all(|r| r.trim().is_empty()) {
                    notes.push(msg("time_bound_no_review_points"));
                    flag("no_review_points");
                }
            }
            "achievable" => {
                if missing.contains(&"achievability_basis".to_string()) {
                    notes.push(msg("achievable_missing"));
                    flag("missing_achievability_basis");
                }
            }
            "relevant" => {
                if missing.contains(&"relevance".to_string()) {
                    notes.push(msg("relevant_missing"));
                    flag("missing_relevance");
                }
            }
            "inclusive" => {
                if missing.contains(&"inclusion".to_string()) {
                    notes.push(msg("inclusive_missing"));
                    flag("missing_inclusion");
                }
            }
            "equitable" => {
                if missing.contains(&"equity_dimension".to_string()) {
                    notes.push(msg("equitable_missing"));
                    flag("missing_equity_dimension");
                } else if o.breakdown_by.iter().all(|b| b.trim().is_empty()) {
                    notes.push(msg("equitable_no_breakdown"));
                    flag("no_breakdown");
                }
            }
            _ => {}
        }
        let status = if !missing.is_empty() {
            ElementStatus::Missing
        } else if concern {
            ElementStatus::Concern
        } else if e.judgment {
            ElementStatus::ProvidedForJudgment
        } else {
            ElementStatus::Met
        };
        if status == ElementStatus::ProvidedForJudgment && notes.is_empty() {
            notes.push(format!(
                "Given; whether it is enough is your and your partners' judgment. CDC asks: {}",
                e.generalized.as_deref().unwrap_or(&e.cdc_question)
            ));
        } else if status == ElementStatus::ProvidedForJudgment {
            notes.push("Given; the judgment is yours and your partners'.".into());
        }
        results.push(ElementResult {
            id: e.id.clone(),
            letter: e.letter.clone(),
            label: e.label.clone(),
            status,
            notes,
            missing_fields: missing,
            question: e.generalized.clone().unwrap_or_else(|| e.cdc_question.clone()),
        });
    }

    // Text check of a draft statement.
    if let Some(st) = tidy(&o.statement) {
        let has_digit = st.chars().any(|ch| ch.is_ascii_digit());
        if !has_digit {
            flag("statement_has_no_number");
        }
        let lower = st.to_lowercase();
        let has_by = lower.contains("by ") || lower.contains("before ") || lower.contains("within ");
        if !has_by {
            flag("statement_has_no_deadline_words");
        }
    }

    // Statement assembled from parts.
    let mut complete = true;
    let mut part = |v: Option<String>, ph: &str| -> String {
        v.unwrap_or_else(|| {
            complete = false;
            format!("[{ph}]")
        })
    };
    let date_txt = part(
        target_date.map(|d| d.format("%B %-d, %Y").to_string()),
        "target date",
    );
    let verb = part(o.direction.map(|d| d.verb().to_string()), "increase, decrease, or maintain");
    let measure_txt = part(measure.clone(), "what is measured");
    let pop_txt = {
        let p = part(population.clone(), "population of focus");
        match &geography {
            Some(g) if !p.contains(g.as_str()) => format!("{p} in {g}"),
            _ => p,
        }
    };
    let base_txt = {
        let v = o.baseline.map(|b| with_unit(b, unit.as_deref()));
        let v = match (v, tidy(&o.baseline_period)) {
            (Some(v), Some(p)) => Some(format!("{v} ({p})")),
            (v, _) => v,
        };
        part(v, "baseline")
    };
    let target_txt = part(o.target.map(|t| with_unit(t, unit.as_deref())), "target");
    let source_txt = part(data_source.clone(), "data source");
    let drafted = if o.direction == Some(Direction::Maintain) {
        format!("By {date_txt}, maintain {measure_txt} among {pop_txt} at {target_txt} (baseline {base_txt}), measured by {source_txt}.")
    } else {
        format!("By {date_txt}, {verb} {measure_txt} among {pop_txt} from {base_txt} to {target_txt}, measured by {source_txt}.")
    };
    let drafted = {
        let mut cs = drafted.chars();
        cs.next()
            .map(|f| f.to_uppercase().collect::<String>() + cs.as_str())
            .unwrap_or_default()
    };

    let change = match (o.baseline, o.target) {
        (Some(b), Some(t)) if b.is_finite() && t.is_finite() => {
            let abs = ((t - b) * 1e6).round() / 1e6;
            let rel = (b != 0.0).then(|| (((t - b) / b.abs()) * 1000.0).round() / 10.0);
            let pct = unit.as_deref().map(unit_class) == Some(UnitClass::Percent);
            let desc = match rel {
                Some(r) => format!(
                    "From {} to {}: a change of {}{} ({}{}% of the baseline).",
                    with_unit(b, unit.as_deref()),
                    with_unit(t, unit.as_deref()),
                    fmt_num(abs),
                    if pct { " percentage points" } else { "" },
                    if r > 0.0 { "+" } else { "" },
                    fmt_num(r)
                ),
                None => format!(
                    "From {} to {}: a change of {}{}.",
                    with_unit(b, unit.as_deref()),
                    with_unit(t, unit.as_deref()),
                    fmt_num(abs),
                    if pct { " percentage points" } else { "" }
                ),
            };
            Some(ChangeOut {
                absolute_change: abs,
                relative_change_percent: rel,
                description: desc,
            })
        }
        _ => None,
    };

    let elements_met = results
        .iter()
        .filter(|r| r.status == ElementStatus::Met)
        .count();
    Checked {
        check: ObjectiveCheck {
            number: n,
            objective_type: o.objective_type,
            drafted_statement: drafted,
            statement_complete: complete,
            original_statement: tidy(&o.statement),
            elements: results,
            elements_met,
            flags,
            change,
            target_date_read: target_date.map(|d| d.to_string()),
            healthy_people: None,
        },
    }
}

// ---------------------------------------------------------------- the tool

/// Write and check SMARTIE objectives, with Healthy People 2030 lookups.
#[allow(clippy::too_many_lines)]
pub async fn write_smartie_objectives(
    ctx: &Context,
    args: SmartieArgs,
) -> Result<Envelope<SmartieResult>> {
    let c: SmartieContent = content::load(SMARTIE)?;
    if args.objectives.is_empty() && args.healthy_people_queries.iter().all(|q| q.trim().is_empty())
    {
        return Err(PhnError::InvalidInput(
            "give at least one objective (objectives) to check or a Healthy People 2030 topic or objective ID (healthy_people_queries)"
                .into(),
        ));
    }
    if args.objectives.len() > MAX_OBJECTIVES {
        return Err(PhnError::InvalidInput(format!(
            "{} objectives given; check at most {MAX_OBJECTIVES} at a time",
            args.objectives.len()
        )));
    }
    let queries: Vec<String> = args
        .healthy_people_queries
        .iter()
        .map(|q| q.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|q| !q.is_empty())
        .collect();
    if queries.len() > MAX_HP_QUERIES {
        return Err(PhnError::InvalidInput(format!(
            "{} Healthy People lookups given; ask for at most {MAX_HP_QUERIES} at a time",
            queries.len()
        )));
    }
    if queries.iter().any(|q| q.chars().count() > 150) {
        return Err(PhnError::InvalidInput(
            "a Healthy People lookup is longer than 150 characters; give a topic, a few keywords, or an objective ID".into(),
        ));
    }
    for (i, o) in args.objectives.iter().enumerate() {
        for (what, v) in [
            ("statement", &o.statement),
            ("measure", &o.measure),
            ("population", &o.population),
            ("data_source", &o.data_source),
            ("achievability_basis", &o.achievability_basis),
            ("relevance", &o.relevance),
            ("inclusion", &o.inclusion),
            ("equity_dimension", &o.equity_dimension),
        ] {
            check_len(&format!("objective {}: {what}", i + 1), v)?;
        }
        for v in [o.baseline, o.target].into_iter().flatten() {
            if !v.is_finite() {
                return Err(PhnError::InvalidInput(format!(
                    "objective {}: baseline and target must be finite numbers",
                    i + 1
                )));
            }
        }
    }
    let max_matches = args.max_matches.unwrap_or(5).clamp(1, 10) as usize;

    let (snap, entry) = healthy_people::load_snapshot(&ctx.snapshots)?;
    let today = chrono::Local::now().date_naive();
    let mut reader = HpReader {
        ctx,
        snap: &snap,
        refresh: args.refresh_live.unwrap_or(true),
        live_reads: 0,
        attempts: 0,
        failure: None,
        sources: Vec::new(),
        cache: BTreeMap::new(),
    };

    // ---- objectives
    let mut checks: Vec<ObjectiveCheck> = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    for (i, o) in args.objectives.iter().enumerate() {
        let mut ch = check_objective(&c, i + 1, o, today).check;
        if let Some(id) = tidy(&o.healthy_people_id) {
            match snap.by_id(&id) {
                Some(obj) => {
                    let out = reader.objective(obj).await;
                    ch.healthy_people = Some(hp_link(o, out, o.direction));
                }
                None => {
                    caveats.push(format!(
                        "Objective {}: \"{id}\" is not a Healthy People 2030 objective ID in the bundled list ({} objectives). Look it up by topic with healthy_people_queries.",
                        i + 1,
                        snap.objectives.len()
                    ));
                }
            }
        }
        checks.push(ch);
    }
    if !checks.is_empty() && checks.iter().all(|k| k.objective_type.is_none()) {
        caveats.push(
            "No objective says whether it is a process or an outcome objective. CDC pairs process objectives (how well the program is carried out) with process evaluation questions and outcome objectives with outcome evaluation questions."
                .into(),
        );
    }
    let mut gaps: BTreeMap<String, usize> = BTreeMap::new();
    for k in &checks {
        for f in &k.flags {
            *gaps.entry(f.clone()).or_default() += 1;
        }
    }

    // ---- lookups
    let mut lookups: Vec<HpLookup> = Vec::new();
    for q in &queries {
        let hits = snap.search(q, max_matches);
        let mut matches = Vec::new();
        for h in &hits {
            matches.push(reader.objective(h.objective).await);
        }
        let note = if hits.is_empty() {
            Some(format!(
                "No Healthy People 2030 objective in the bundled list matches \"{q}\". Try a topic name (for example \"diabetes\", \"tobacco\", \"vaccination\") or an objective ID; the full list is at {}.",
                healthy_people::HOME_URL
            ))
        } else if hits.iter().all(|h| h.objective.objective_type != "core") {
            Some("Only developmental or research objectives matched: they have no national baseline or target.".into())
        } else {
            None
        };
        lookups.push(HpLookup {
            query: q.clone(),
            matches,
            note,
        });
    }

    // ---- Healthy People info and notes
    let touched = !queries.is_empty() || checks.iter().any(|k| k.healthy_people.is_some());
    let hp_info = touched.then(|| HpInfo {
        snapshot_date: snap.retrieved.clone(),
        objectives_in_snapshot: snap.objectives.len(),
        live_pages_read: reader.live_reads,
        live_unavailable_reason: if !reader.refresh {
            Some("live refresh was turned off".into())
        } else if ctx.http.is_offline() {
            Some("the server is offline".into())
        } else {
            reader.failure.clone()
        },
        notes: c
            .healthy_people
            .use_notes
            .iter()
            .map(|n| n.text.clone())
            .collect(),
    });
    if let Some(info) = &hp_info {
        if info.live_pages_read == 0 {
            caveats.push(format!(
                "Healthy People 2030 values come from the bundled snapshot read on {} ({}). {} Check the objective page before citing a figure.",
                snap.retrieved,
                info.live_unavailable_reason
                    .as_deref()
                    .map(|r| format!("Live pages were not used: {r}."))
                    .unwrap_or_default(),
                "ODPHP revises baselines and targets."
            ));
        } else if let Some(r) = &info.live_unavailable_reason {
            caveats.push(format!(
                "Some Healthy People 2030 values come from the bundled snapshot ({}) because a live page could not be read: {r}.",
                snap.retrieved
            ));
        }
        caveats.push(
            "ODPHP offers no public data API for the objectives; this tool reads each objective's own page on odphp.health.gov. The site displays a notice that it was restored to its January 29, 2025 version under a court order and may be modified, so re-check pages before you cite them."
                .into(),
        );
    }
    let changed: Vec<String> = lookups
        .iter()
        .flat_map(|l| l.matches.iter())
        .chain(checks.iter().filter_map(|k| k.healthy_people.as_ref().map(|h| &h.objective)))
        .filter(|m| !m.changed_since_snapshot.is_empty())
        .map(|m| format!("{}: {}", m.id, m.changed_since_snapshot.join("; ")))
        .collect();
    if !changed.is_empty() {
        caveats.push(format!(
            "The live pages differ from the bundled snapshot for: {}. The live values are shown.",
            changed.join(" | ")
        ));
    }

    // ---- document
    let mut file = None;
    let mut files_written = Vec::new();
    if args.write_document.unwrap_or(false) {
        if checks.is_empty() {
            return Err(PhnError::InvalidInput(
                "write_document needs at least one objective in objectives".into(),
            ));
        }
        let (dir, _) = templates::output_folder(ctx, "smartie_objectives")?;
        let format = args.format.unwrap_or_default();
        let m = document_markdown(&c, tidy(&args.goal).as_deref(), &checks);
        let doc = templates::document(ctx, &m, "en-US", "SMARTIE objectives");
        let f = templates::write_in(
            ctx,
            &dir,
            &format!("smartie_objectives.{}", format.ext()),
            &format.render(&doc)?,
        )?;
        files_written.push(f.clone());
        file = Some(f);
    }

    // ---- summary
    let mut s = String::from("**SMARTIE objectives**\n\n");
    if let Some(g) = tidy(&args.goal) {
        s.push_str(&format!("Goal: {g}\n\n"));
    }
    for k in &checks {
        s.push_str(&format!(
            "### Objective {}{}\n\n",
            k.number,
            match k.objective_type {
                Some(ObjectiveKind::Process) => " (process)",
                Some(ObjectiveKind::Outcome) => " (outcome)",
                None => "",
            }
        ));
        s.push_str(&format!(
            "{}{}\n\n",
            k.drafted_statement,
            if k.statement_complete {
                ""
            } else {
                " _(bracketed parts are missing)_"
            }
        ));
        let rows: Vec<Vec<String>> = k
            .elements
            .iter()
            .map(|e| {
                vec![
                    format!("{} {}", e.letter, e.label),
                    match e.status {
                        ElementStatus::Met => "Met".into(),
                        ElementStatus::ProvidedForJudgment => "Given (your judgment)".into(),
                        ElementStatus::Missing => "**Missing**".into(),
                        ElementStatus::Concern => "**Check**".into(),
                    },
                    e.notes.join(" "),
                ]
            })
            .collect();
        s.push_str(&md::table(&["Element", "Status", "Notes"], &rows));
        if let Some(ch) = &k.change {
            s.push_str(&format!("\n{}\n", ch.description));
        }
        if let Some(h) = &k.healthy_people {
            s.push_str(&format!(
                "\nHealthy People 2030 {} ({}): baseline {}; target {}.{}\n",
                h.objective.id,
                h.objective.title,
                h.objective.national_baseline.as_deref().unwrap_or("none"),
                h.objective.national_target.as_deref().unwrap_or("none"),
                h.comparison.as_deref().map(|x| format!(" {x}")).unwrap_or_default()
            ));
        }
        s.push('\n');
    }
    for l in &lookups {
        s.push_str(&format!("### Healthy People 2030: \"{}\"\n\n", l.query));
        if l.matches.is_empty() {
            s.push_str(&format!("{}\n\n", l.note.clone().unwrap_or_default()));
            continue;
        }
        let rows: Vec<Vec<String>> = l
            .matches
            .iter()
            .map(|m| {
                vec![
                    format!("{} {}", m.id, if m.leading_health_indicator { "(LHI)" } else { "" })
                        .trim()
                        .to_string(),
                    m.title.clone(),
                    m.national_baseline.clone().unwrap_or_else(|| "none".into()),
                    m.national_target.clone().unwrap_or_else(|| "none".into()),
                    m.desired_direction.clone().unwrap_or_default(),
                    format!("{} ({})", m.mode, m.as_of),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["ID", "Objective", "National baseline", "National target", "Direction", "Data"],
            &rows,
        ));
        if let Some(n) = &l.note {
            s.push_str(&format!("\n{n}\n"));
        }
        s.push('\n');
    }
    if let Some(f) = &file {
        s.push_str(&format!("Worksheet written to {f}.\n"));
    }

    caveats.push(
        "The checker counts what you gave it. It cannot tell whether a target is achievable, relevant, inclusive, or equitable: those four are judgments for you, your partners, and the population the objective serves. CDC advises asking the population of focus and partners for input and how they define success."
            .into(),
    );
    caveats.push(
        "CDC's own sheet calls the A \"attainable\" and the T \"time-based\"; its Action Guide writes \"achievable\" and \"timebound\". The seven questions are CDC's (written for its cancer screening program; the equity question is generalized from \"cancer screening disparities\" to \"health disparities\")."
            .into(),
    );
    let texts = roster::argument_strings(
        &args,
        &["refresh_live", "write_document", "format", "max_matches", "target_date", "start_date"],
    );
    let found = roster::contacts_in_arguments(texts.iter().map(String::as_str));
    if !found.is_empty() {
        caveats.push(format!(
            "Your objectives appear to contain {}. Objectives describe a program and a population, not people: remove personal details before sharing them.",
            found.join(", ").to_lowercase()
        ));
    }
    caveats.extend(content::review_caveats(&[SMARTIE]));

    let mut sources = content::sources_for(
        ["cdc-smartie", "cdc-action-guide-step4", "hp2030-about"],
        &c.sources,
    );
    if touched {
        sources.push(entry.to_source());
        sources.extend(reader.sources.clone());
    }
    let data = SmartieResult {
        goal: tidy(&args.goal),
        objectives: checks,
        gaps,
        healthy_people_lookups: lookups,
        healthy_people: hp_info,
        elements: c
            .elements
            .iter()
            .map(|e| ElementRef {
                letter: e.letter.clone(),
                label: e.label.clone(),
                cdc_name: e.cdc_name.clone(),
                question: e.generalized.clone().unwrap_or_else(|| e.cdc_question.clone()),
            })
            .collect(),
        equity_dimension_prompts: c.equity_prompts.dimensions.clone(),
        cdc_examples: c
            .cdc_examples
            .pairs
            .iter()
            .map(|p| ExamplePair {
                strategy: p.strategy.clone(),
                traditional: p.traditional.clone(),
                smartie: p.smartie.clone(),
            })
            .collect(),
        file,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            row_level_data: false,
            identifiers_detected: found,
            files_written,
            ..Privacy::none()
        }))
}

fn document_markdown(c: &SmartieContent, goal: Option<&str>, checks: &[ObjectiveCheck]) -> String {
    let mut m = String::from("# SMARTIE objectives\n\n");
    if let Some(g) = goal {
        m.push_str(&format!("**Goal:** {g}\n\n"));
    }
    m.push_str("Draft for review with partners and the population of focus. Source: CDC, Writing Effective Objectives (SMARTIE).\n\n");
    for k in checks {
        m.push_str(&format!("## Objective {}\n\n{}\n\n", k.number, k.drafted_statement));
        let rows: Vec<Vec<String>> = k
            .elements
            .iter()
            .map(|e| {
                vec![
                    format!("{} {}", e.letter, e.label),
                    match e.status {
                        ElementStatus::Met => "Met".into(),
                        ElementStatus::ProvidedForJudgment => "Given (judgment)".into(),
                        ElementStatus::Missing => "Missing".into(),
                        ElementStatus::Concern => "Check".into(),
                    },
                    if e.notes.is_empty() {
                        e.question.clone()
                    } else {
                        e.notes.join(" ")
                    },
                ]
            })
            .collect();
        m.push_str(&format!("Table: Objective {} checked against the seven SMARTIE elements\n\n", k.number));
        m.push_str(&md::table(&["Element", "Status", "Notes"], &rows));
        if let Some(h) = &k.healthy_people {
            m.push_str(&format!(
                "\nHealthy People 2030 {}: {} (baseline {}; target {}).\n",
                h.objective.id,
                h.objective.title,
                h.objective.national_baseline.as_deref().unwrap_or("none"),
                h.objective.national_target.as_deref().unwrap_or("none")
            ));
        }
        m.push('\n');
    }
    m.push_str("## The seven elements\n\n");
    for e in &c.elements {
        m.push_str(&format!(
            "- **{} {}.** {}\n",
            e.letter,
            e.label,
            e.generalized.as_deref().unwrap_or(&e.cdc_question)
        ));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_core::SourceMode;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn full() -> ObjectiveInput {
        ObjectiveInput {
            statement: Some("By December 2027, raise postpartum visit attendance to 85%".into()),
            objective_type: Some(ObjectiveKind::Outcome),
            measure: Some("proportion of first-time parents who attend a postpartum visit within 14 days".into()),
            direction: Some(Direction::Increase),
            population: Some("first-time parents who deliver at the town's partner hospital".into()),
            geography: None,
            baseline: Some(62.0),
            baseline_period: Some("2025".into()),
            target: Some(85.0),
            unit: Some("percent".into()),
            target_date: Some("2027-12-31".into()),
            start_date: Some("2026-07-01".into()),
            review_points: vec!["quarterly".into()],
            data_source: Some("hospital discharge referral log and visit records".into()),
            achievability_basis: Some("Pilot reached 78% in 2025".into()),
            relevance: Some("Priority 2 of the community health improvement plan".into()),
            inclusion: Some("Parent advisory group reviewed the target".into()),
            equity_dimension: Some("Spanish-speaking parents, whose attendance is lower".into()),
            breakdown_by: vec!["language".into(), "race and ethnicity".into()],
            healthy_people_id: None,
            responsible: Some("Nurse supervisor".into()),
        }
    }

    fn args(o: ObjectiveInput) -> SmartieArgs {
        SmartieArgs {
            objectives: vec![o],
            refresh_live: Some(false),
            ..SmartieArgs::default()
        }
    }

    #[tokio::test]
    async fn a_complete_objective_passes_and_reads_as_a_sentence() {
        let (_d, ctx) = ctx();
        let r = write_smartie_objectives(&ctx, args(full())).await.unwrap();
        let k = &r.data.objectives[0];
        assert!(k.statement_complete);
        assert_eq!(
            k.drafted_statement,
            "By December 31, 2027, increase proportion of first-time parents who attend a postpartum visit within 14 days among first-time parents who deliver at the town's partner hospital from 62% (2025) to 85%, measured by hospital discharge referral log and visit records."
        );
        for e in &k.elements {
            assert_ne!(e.status, ElementStatus::Missing, "{e:?}");
            assert_ne!(e.status, ElementStatus::Concern, "{e:?}");
        }
        assert_eq!(k.elements.len(), 7);
        assert_eq!(k.elements_met, 3, "specific, measurable, time-bound");
        let ch = k.change.as_ref().unwrap();
        assert_eq!(ch.absolute_change, 23.0);
        assert_eq!(ch.relative_change_percent, Some(37.1));
        assert!(ch.description.contains("23 percentage points"));
        assert_eq!(k.target_date_read.as_deref(), Some("2027-12-31"));
        assert!(r.data.healthy_people.is_none());
        assert!(r.data.gaps.is_empty(), "{:?}", r.data.gaps);
    }

    #[tokio::test]
    async fn missing_parts_are_flagged_and_bracketed() {
        let (_d, ctx) = ctx();
        let r = write_smartie_objectives(
            &ctx,
            args(ObjectiveInput {
                statement: Some("Improve screening".into()),
                measure: Some("screening rate".into()),
                ..ObjectiveInput::default()
            }),
        )
        .await
        .unwrap();
        let k = &r.data.objectives[0];
        assert!(!k.statement_complete);
        assert!(k.drafted_statement.contains("[baseline]"));
        assert!(k.drafted_statement.contains("[target date]"));
        for f in [
            "missing_baseline",
            "missing_target",
            "missing_date",
            "missing_population",
            "missing_data_source",
            "missing_equity_dimension",
            "missing_direction",
            "no_review_points",
            "statement_has_no_number",
        ] {
            assert!(k.flags.contains(&f.to_string()), "{f} in {:?}", k.flags);
        }
        assert_eq!(r.data.gaps["missing_baseline"], 1);
        assert!(k.change.is_none());
        let t = k.elements.iter().find(|e| e.id == "time_bound").unwrap();
        assert_eq!(t.status, ElementStatus::Missing);
        assert!(t.notes.iter().any(|n| n.contains("No target date")));
    }

    #[tokio::test]
    async fn wrong_direction_and_bad_dates_are_concerns() {
        let (_d, ctx) = ctx();
        let mut o = full();
        o.target = Some(50.0);
        o.target_date = Some("12/31/2025".into());
        o.start_date = Some("2026-01-01".into());
        let r = write_smartie_objectives(&ctx, args(o)).await.unwrap();
        let k = &r.data.objectives[0];
        assert!(k.flags.contains(&"direction_mismatch".into()));
        assert!(k.flags.contains(&"date_before_start".into()));
        let m = k.elements.iter().find(|e| e.id == "measurable").unwrap();
        assert_eq!(m.status, ElementStatus::Concern);
        let mut o = full();
        o.target_date = Some("next spring".into());
        let r = write_smartie_objectives(&ctx, args(o)).await.unwrap();
        let k = &r.data.objectives[0];
        assert!(k.flags.contains(&"date_unreadable".into()));
        assert_eq!(k.target_date_read, None);
        let mut o = full();
        o.target = Some(62.0);
        let r = write_smartie_objectives(&ctx, args(o)).await.unwrap();
        assert!(r.data.objectives[0].flags.contains(&"target_equals_baseline".into()));
    }

    #[test]
    fn dates_are_read_in_a_few_forms_and_bounded() {
        let d = |s: &str| read_date(s).map(|d| d.to_string());
        assert_eq!(d("2027-12-31").as_deref(), Some("2027-12-31"));
        assert_eq!(d("2027-02").as_deref(), Some("2027-02-28"));
        assert_eq!(d("2028-02").as_deref(), Some("2028-02-29"));
        assert_eq!(d("2027").as_deref(), Some("2027-12-31"));
        assert_eq!(d("3/5/2027").as_deref(), Some("2027-03-05"));
        assert_eq!(d("2027-12").as_deref(), Some("2027-12-31"));
        for bad in ["3/5/27", "0021-05-01", "2101", "1899-12-31", "2027-13", "2027-02-30", "soon", ""] {
            assert_eq!(d(bad), None, "{bad}");
        }
    }

    #[tokio::test]
    async fn healthy_people_lookup_uses_the_snapshot_offline() {
        let (_d, ctx) = ctx();
        let r = write_smartie_objectives(
            &ctx,
            SmartieArgs {
                healthy_people_queries: vec!["diabetes yearly eye exam".into(), "D-04".into(), "zzzz qqqq".into()],
                refresh_live: Some(false),
                ..SmartieArgs::default()
            },
        )
        .await
        .unwrap();
        let l = &r.data.healthy_people_lookups;
        assert_eq!(l[0].matches[0].id, "D-04");
        assert_eq!(l[0].matches[0].mode, "snapshot");
        assert_eq!(l[0].matches[0].national_target_value, Some(70.3));
        assert_eq!(l[1].matches.len(), 1);
        assert!(l[2].matches.is_empty() && l[2].note.is_some());
        let info = r.data.healthy_people.as_ref().unwrap();
        assert_eq!(info.live_pages_read, 0);
        assert!(info.objectives_in_snapshot >= 480);
        assert!(r.caveats.iter().any(|c| c.contains("bundled snapshot")));
        assert!(r.sources.iter().any(|s| s.mode == SourceMode::Snapshot));
        assert!(r.summary.contains("D-04"));
    }

    #[tokio::test]
    async fn a_linked_objective_is_compared_in_the_same_unit_only() {
        let (_d, ctx) = ctx();
        let mut o = full();
        o.healthy_people_id = Some("d-04".into());
        o.unit = Some("percent".into());
        o.direction = Some(Direction::Increase);
        o.target = Some(75.0);
        o.baseline = Some(60.0);
        let r = write_smartie_objectives(&ctx, args(o.clone())).await.unwrap();
        let h = r.data.objectives[0].healthy_people.as_ref().unwrap();
        let cmp = h.comparison.as_deref().unwrap();
        assert!(cmp.contains("better than (higher than)"), "{cmp}");
        assert!(cmp.contains("70.3 percent"));
        assert!(h.notes.iter().any(|n| n.contains("age-adjusted")));
        // A rate unit does not compare with a percent target.
        o.unit = Some("per 100,000".into());
        let r = write_smartie_objectives(&ctx, args(o.clone())).await.unwrap();
        let h = r.data.objectives[0].healthy_people.as_ref().unwrap();
        assert!(h.comparison.is_none());
        assert!(h.notes.iter().any(|n| n.contains("not the same kind")));
        // A different direction is noted.
        o.unit = Some("percent".into());
        o.direction = Some(Direction::Decrease);
        o.target = Some(40.0);
        let r = write_smartie_objectives(&ctx, args(o)).await.unwrap();
        let h = r.data.objectives[0].healthy_people.as_ref().unwrap();
        assert!(h.notes.iter().any(|n| n.contains("desired direction")));
    }

    #[tokio::test]
    async fn unknown_ids_and_bad_input_are_handled() {
        let (_d, ctx) = ctx();
        let mut o = full();
        o.healthy_people_id = Some("ZZ-99".into());
        let r = write_smartie_objectives(&ctx, args(o)).await.unwrap();
        assert!(r.data.objectives[0].healthy_people.is_none());
        assert!(r.caveats.iter().any(|c| c.contains("ZZ-99")));
        let e = write_smartie_objectives(&ctx, SmartieArgs::default()).await.unwrap_err();
        assert!(e.to_string().contains("at least one objective"));
        let e = write_smartie_objectives(
            &ctx,
            SmartieArgs {
                objectives: vec![ObjectiveInput::default(); 21],
                ..SmartieArgs::default()
            },
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("at most 20"));
        let mut o = full();
        o.baseline = Some(f64::NAN);
        let e = write_smartie_objectives(&ctx, args(o)).await.unwrap_err();
        assert!(e.to_string().contains("finite"));
        let e = write_smartie_objectives(
            &ctx,
            SmartieArgs {
                healthy_people_queries: vec!["x".repeat(151)],
                ..SmartieArgs::default()
            },
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("150 characters"));
    }

    #[tokio::test]
    async fn writes_a_worksheet_to_the_output_folder() {
        let (dir, ctx) = ctx();
        let mut a = args(full());
        a.write_document = Some(true);
        a.goal = Some("Improve postpartum care".into());
        let r = write_smartie_objectives(&ctx, a).await.unwrap();
        let f = r.data.file.unwrap();
        assert!(dir.path().join("data").join(&f).is_file(), "{f}");
        assert_eq!(r.privacy.files_written, vec![f]);
        let e = write_smartie_objectives(
            &ctx,
            SmartieArgs {
                healthy_people_queries: vec!["tobacco".into()],
                write_document: Some(true),
                refresh_live: Some(false),
                ..SmartieArgs::default()
            },
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("at least one objective"));
    }

    #[tokio::test]
    async fn contact_details_are_flagged_by_category() {
        let (_d, ctx) = ctx();
        let mut o = full();
        o.inclusion = Some("Asked the family at 12 Oak Street, call 617-555-0142".into());
        let r = write_smartie_objectives(&ctx, args(o)).await.unwrap();
        assert!(!r.privacy.identifiers_detected.is_empty());
        assert!(!r.caveats.iter().any(|c| c.contains("555-0142")));
    }

    #[tokio::test]
    async fn cdc_examples_and_questions_are_returned() {
        let (_d, ctx) = ctx();
        let r = write_smartie_objectives(&ctx, args(full())).await.unwrap();
        assert_eq!(r.data.elements.len(), 7);
        assert!(r.data.elements[2].cdc_name == "Attainable" && r.data.elements[2].label == "Achievable");
        assert!(r.data.elements[6].question.contains("health disparities"));
        assert_eq!(r.data.cdc_examples.len(), 3);
        assert!(r.caveats.iter().any(|c| c.contains("attainable")));
        assert!(r.caveats.iter().any(|c| c.to_lowercase().contains("review")));
    }
}
