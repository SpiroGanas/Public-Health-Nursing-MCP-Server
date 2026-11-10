//! MH-02 `score_mental_health_screen`: a scoring engine for free mental
//! health screening instruments with safety-alert rules.
//!
//! Instruments, cutoffs, reverse-scored items, safety items, alert steps, and
//! license terms all come from `content/bh/mental_health_screening.yaml`
//! (pending review). The engine scores item responses, validates ranges,
//! interprets the published cutoffs, and raises safety alerts; it never
//! diagnoses.
//!
//! Batch mode reads a file of screens from /data (one row per person), writes
//! per-row results to /data/output, and returns only suppressed counts.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{
    self as c10, CitedOut, CitedText, Confidence, MH_SCREENING, SourceRef, cite_all,
};
use crate::context::Context;
use crate::md;

// ------------------------------------------------------------------ content

/// How an instrument's items are answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResponseKind {
    /// 0–3 frequency ratings (PHQ, GAD).
    Frequency,
    /// Four answer options scored 0–3, some reverse-scored (EPDS).
    Options,
    /// Yes/no items (ASQ, C-SSRS screener).
    YesNo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDef {
    pub n: u32,
    pub topic: String,
    #[serde(default)]
    pub wording: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BandDef {
    pub min: i32,
    pub max: i32,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SafetyItem {
    pub item: u32,
    pub at_or_above: i32,
    pub alert: String,
}

/// License terms recorded for an instrument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LicenseInfo {
    /// public_domain, free, cite_required, or permission_terms_unverified.
    pub reuse: String,
    pub terms: String,
    /// True when the item wording is bundled with this server.
    pub wording_bundled: bool,
    #[serde(default, skip_serializing)]
    #[schemars(skip)]
    pub source: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Instrument {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub purpose: String,
    pub response: ResponseKind,
    pub item_count: u32,
    pub item_min: i32,
    pub item_max: i32,
    pub response_options: Vec<String>,
    pub lookback: String,
    pub items: Vec<ItemDef>,
    pub bands: Vec<BandDef>,
    pub positive_at: Option<i32>,
    pub positive_text: String,
    pub positive_source: String,
    pub safety_items: Vec<SafetyItem>,
    pub reverse_scored_items: Vec<u32>,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
    pub license: LicenseInfo,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlertDef {
    pub id: String,
    pub title: String,
    pub urgency: String,
    #[allow(dead_code)]
    pub applies_to: Vec<String>,
    pub trigger_text: String,
    pub steps: Vec<CitedText>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreeningContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, SourceRef>,
    pub not_a_diagnosis: String,
    pub instruments: Vec<Instrument>,
    pub safety_alerts: Vec<AlertDef>,
    pub resources_for_all: Vec<CitedText>,
}

static CONTENT: LazyLock<ScreeningContent> =
    LazyLock::new(|| c10::load(MH_SCREENING).expect("mental health screening content parses"));

/// The bundled screening content.
pub fn content() -> &'static ScreeningContent {
    &CONTENT
}

/// Find an instrument by id or alias.
pub fn find_instrument(query: &str) -> Option<&'static Instrument> {
    let q = phn_core::text::normalize(query).replace(' ', "");
    content().instruments.iter().find(|i| {
        phn_core::text::normalize(&i.id).replace(' ', "") == q
            || i.aliases
                .iter()
                .any(|a| phn_core::text::normalize(a).replace(' ', "") == q)
    })
}

fn alert_def(id: &str) -> Option<&'static AlertDef> {
    content().safety_alerts.iter().find(|a| a.id == id)
}

// ------------------------------------------------------------------ arguments

/// One item response: a number, true/false, or text such as "yes", "no",
/// "several days", "nearly every day", or "skipped".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ItemResponse {
    Number(i64),
    YesNo(bool),
    Text(String),
}

/// How EPDS answers were entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EpdsEntry {
    /// The scored value (0–3) from the scoring key (default).
    #[default]
    Scores,
    /// The position of the chosen answer, 0 = first (top) answer; the tool
    /// applies the reverse scoring.
    OptionPositions,
}

/// Arguments for `score_mental_health_screen`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ScreenArgs {
    /// Instrument: phq2, phq9, gad2, gad7, epds, asq (NIMH Ask
    /// Suicide-Screening Questions), or cssrs_screener. Omit to list the
    /// instruments with their cutoffs and license terms.
    pub instrument: Option<String>,
    /// Item responses in item order. Frequency items: 0–3 (or "not at all",
    /// "several days", "more than half the days", "nearly every day").
    /// Yes/no items: true/false or "yes"/"no". Use null or "skipped" for an
    /// unanswered item. No names or identifiers.
    pub responses: Vec<Option<ItemResponse>>,
    /// EPDS only: "scores" (default, the scored value from the key) or
    /// "option_positions" (0 = first answer; the tool reverse-scores).
    pub epds_entered_as: Option<EpdsEntry>,
    /// Age in years (ASQ: refusal counts as a positive screen for youth
    /// under 18; also used for the past-attempt exception).
    pub age_years: Option<u32>,
    /// The person refused to answer the screening questions (ASQ).
    pub refused: bool,
    /// ASQ item 4 follow-up: was the most recent attempt within the last 12 months?
    pub asq_attempt_within_12_months: Option<bool>,
    /// ASQ: a brief suicide safety assessment has been documented since that attempt.
    pub asq_bssa_documented_since_attempt: Option<bool>,
    /// C-SSRS item 6 follow-up: was the suicidal behavior within the past 3 months?
    pub cssrs_behavior_past_3_months: Option<bool>,
    /// PHQ-9 functional difficulty answer (recorded, not scored).
    pub difficulty: Option<String>,
    /// Batch mode: a CSV/XLSX in /data with one row per screen and one
    /// column per item (named q1..qN, item1..itemN, or <instrument>_1..).
    /// Per-row results go to /data/output; only suppressed counts are returned.
    pub file: Option<String>,
    /// Sheet name for an XLSX file.
    pub sheet: Option<String>,
}

// ------------------------------------------------------------------ results

/// The overall screening outcome.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ScreenOutcome {
    /// Below the positive cutoff.
    Negative,
    /// At or above the positive cutoff.
    Positive,
    /// ASQ: yes to question 5.
    AcutePositive,
    /// ASQ: yes to any of 1–4 and no to 5 (or a youth's refusal).
    NonAcutePositive,
    /// ASQ: positive, but question 5 has not been answered yet.
    PositiveAcuityUnknown,
    /// C-SSRS: every answer no.
    NoRisk,
    LowRisk,
    ModerateRisk,
    HighRisk,
    /// C-SSRS: a yes answer, but item(s) that decide the risk level were not
    /// answered. At least the moderate (same-day) steps apply until they are.
    RiskNotDetermined,
    /// Not enough items answered to decide.
    Incomplete,
    /// Severity instrument scored (no single positive/negative call).
    Scored,
}

impl ScreenOutcome {
    /// Plain-language label.
    pub fn label(self) -> &'static str {
        match self {
            ScreenOutcome::Negative => "negative screen",
            ScreenOutcome::Positive => "positive screen",
            ScreenOutcome::AcutePositive => "acute positive screen (imminent risk)",
            ScreenOutcome::NonAcutePositive => "non-acute positive screen (potential risk)",
            ScreenOutcome::PositiveAcuityUnknown => "positive screen; acuity question not answered",
            ScreenOutcome::NoRisk => "no risk identified",
            ScreenOutcome::LowRisk => "low risk",
            ScreenOutcome::ModerateRisk => "moderate risk",
            ScreenOutcome::HighRisk => "high risk",
            ScreenOutcome::RiskNotDetermined => {
                "risk level not yet determined (required items not answered)"
            }
            ScreenOutcome::Incomplete => "incomplete",
            ScreenOutcome::Scored => "scored",
        }
    }

    /// True for outcomes that need follow-up.
    pub fn is_positive(self) -> bool {
        matches!(
            self,
            ScreenOutcome::Positive
                | ScreenOutcome::AcutePositive
                | ScreenOutcome::NonAcutePositive
                | ScreenOutcome::PositiveAcuityUnknown
                | ScreenOutcome::LowRisk
                | ScreenOutcome::ModerateRisk
                | ScreenOutcome::HighRisk
                | ScreenOutcome::RiskNotDetermined
        )
    }
}

/// One item as scored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemOut {
    pub item: u32,
    /// Topic summary (not the instrument's wording unless `wording` is set).
    pub topic: String,
    /// Verbatim wording, only for instruments whose license allows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wording: Option<String>,
    /// The value entered (null when unanswered).
    pub entered: Option<i32>,
    /// The value after any reverse scoring.
    pub score: Option<i32>,
    /// True when the item triggers a safety alert.
    pub safety_flag: bool,
}

/// A severity band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandOut {
    pub min: i32,
    pub max: i32,
    pub label: String,
}

/// A safety alert with the steps to take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AlertOut {
    pub id: String,
    pub title: String,
    /// immediate, same_day, or routine.
    pub urgency: String,
    /// What triggered it.
    pub trigger: String,
    pub steps: Vec<CitedOut>,
}

/// Scoring for one screen (no identifiers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScoredScreen {
    pub total_score: Option<i32>,
    /// Sum of the answered items when some are missing.
    pub partial_sum: Option<i32>,
    pub severity: Option<String>,
    pub outcome: ScreenOutcome,
    pub items: Vec<ItemOut>,
    pub missing_items: Vec<u32>,
    pub alert_ids: Vec<String>,
    pub notes: Vec<String>,
    /// Safety items (PHQ-9 item 9, EPDS item 10) that were not answered. Ask
    /// them before the person leaves: suicide risk has not been screened.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety_items_unanswered: Vec<u32>,
}

/// An instrument in the catalog listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InstrumentSummary {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub items: u32,
    pub cutoffs: String,
    pub license: LicenseInfo,
}

/// Batch summary (counts only, after suppression).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenBatch {
    pub file: String,
    pub rows_read: usize,
    /// Rows with a result (null when any count below is suppressed, so the
    /// hidden count cannot be found by subtraction).
    pub rows_scored: Option<usize>,
    /// Rows that were incomplete or could not be scored (null when any count
    /// below is suppressed).
    pub rows_incomplete: Option<usize>,
    /// Counts by outcome or severity; "*" when suppressed.
    pub counts: BTreeMap<String, String>,
    /// Rows with a safety alert ("*" when suppressed). Their row numbers are
    /// in the output file only.
    pub rows_with_safety_alerts: String,
    pub output_file: String,
    pub suppression_threshold: u32,
}

/// Result of `score_mental_health_screen`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenResult {
    /// Instrument id (empty when listing instruments).
    pub instrument: String,
    pub instrument_name: String,
    pub purpose: String,
    pub lookback: String,
    pub max_score: Option<i32>,
    /// One screen's scoring (single mode).
    pub scored: Option<ScoredScreen>,
    pub bands: Vec<BandOut>,
    /// Published cutoff interpretation, with its citation.
    pub interpretation: String,
    pub interpretation_citation: String,
    pub safety_alerts: Vec<AlertOut>,
    /// Resources to give everyone screened for suicide risk.
    pub resources: Vec<CitedOut>,
    pub next_steps: Vec<String>,
    pub instrument_notes: Vec<String>,
    pub license: Option<LicenseInfo>,
    pub not_a_diagnosis: String,
    /// The instrument catalog (when no instrument was given).
    pub instruments: Vec<InstrumentSummary>,
    pub batch: Option<ScreenBatch>,
}

// ------------------------------------------------------------------ engine

/// What a response parsed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parsed {
    Value(i32),
    Missing,
}

fn parse_response(inst: &Instrument, item: u32, r: Option<&ItemResponse>) -> Result<Parsed> {
    let bad = |what: &str| {
        PhnError::InvalidInput(format!(
            "{} item {item}: '{what}' is not a valid answer; use {}",
            inst.name,
            match inst.response {
                ResponseKind::YesNo => "yes or no".to_string(),
                _ => format!(
                    "a whole number from {} to {} (or \"skipped\")",
                    inst.item_min, inst.item_max
                ),
            }
        ))
    };
    let v: i64 = match r {
        None => return Ok(Parsed::Missing),
        Some(ItemResponse::Number(n)) => *n,
        Some(ItemResponse::YesNo(b)) => {
            if inst.response != ResponseKind::YesNo {
                return Err(bad(&b.to_string()));
            }
            i64::from(*b)
        }
        Some(ItemResponse::Text(t)) => {
            let n = phn_core::text::normalize(t);
            match n.as_str() {
                "" | "skipped" | "skip" | "missing" | "na" | "n a" | "not answered" | "blank"
                | "unknown" | "refused" | "declined" => return Ok(Parsed::Missing),
                "yes" | "y" | "true" if inst.response == ResponseKind::YesNo => 1,
                "no" | "n" | "false" if inst.response == ResponseKind::YesNo => 0,
                "not at all" if inst.response == ResponseKind::Frequency => 0,
                "several days" if inst.response == ResponseKind::Frequency => 1,
                "more than half the days" if inst.response == ResponseKind::Frequency => 2,
                "nearly every day" if inst.response == ResponseKind::Frequency => 3,
                other => other.parse::<i64>().map_err(|_| bad(t))?,
            }
        }
    };
    if v < i64::from(inst.item_min) || v > i64::from(inst.item_max) {
        return Err(PhnError::InvalidInput(format!(
            "{} item {item}: {v} is out of range; items are scored {} to {}",
            inst.name, inst.item_min, inst.item_max
        )));
    }
    Ok(Parsed::Value(v as i32))
}

/// Options that change how a screen is scored.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScoreOptions {
    pub epds_entry: EpdsEntry,
    pub age_years: Option<u32>,
    pub refused: bool,
    pub asq_attempt_within_12_months: Option<bool>,
    pub asq_bssa_documented_since_attempt: Option<bool>,
    pub cssrs_behavior_past_3_months: Option<bool>,
}

/// Score one screen. Errors are input problems the nurse can fix.
pub fn score(
    inst: &Instrument,
    responses: &[Option<ItemResponse>],
    opts: ScoreOptions,
) -> Result<ScoredScreen> {
    if responses.len() > inst.item_count as usize {
        return Err(PhnError::InvalidInput(format!(
            "{} has {} items, but {} responses were given",
            inst.name,
            inst.item_count,
            responses.len()
        )));
    }
    if responses.is_empty() && !opts.refused {
        return Err(PhnError::InvalidInput(format!(
            "give the {} item responses in order ({} items)",
            inst.name, inst.item_count
        )));
    }
    let mut entered = Vec::with_capacity(inst.item_count as usize);
    for n in 1..=inst.item_count {
        let r = responses.get(n as usize - 1).and_then(|r| r.as_ref());
        entered.push(parse_response(inst, n, r)?);
    }
    let value = |n: u32| match entered[n as usize - 1] {
        Parsed::Value(v) => Some(v),
        Parsed::Missing => None,
    };
    let scored_value = |n: u32| {
        value(n).map(|v| {
            if inst.id == "epds"
                && opts.epds_entry == EpdsEntry::OptionPositions
                && inst.reverse_scored_items.contains(&n)
            {
                inst.item_max - v
            } else {
                v
            }
        })
    };
    let missing: Vec<u32> = (1..=inst.item_count)
        .filter(|n| value(*n).is_none())
        .collect();
    let mut alert_ids: Vec<String> = Vec::new();
    let mut flagged: Vec<u32> = Vec::new();
    for s in &inst.safety_items {
        if scored_value(s.item).is_some_and(|v| v >= s.at_or_above) {
            flagged.push(s.item);
            if !alert_ids.contains(&s.alert) {
                alert_ids.push(s.alert.clone());
            }
        }
    }
    let mut notes = Vec::new();
    // A safety item that was not answered must never pass silently.
    let mut safety_unanswered: Vec<u32> = Vec::new();
    for s in &inst.safety_items {
        if missing.contains(&s.item) && !safety_unanswered.contains(&s.item) {
            safety_unanswered.push(s.item);
        }
    }
    if !safety_unanswered.is_empty() {
        notes.push(format!(
            "SAFETY: item {} (the item that raises the suicide-risk alert) was not answered. Ask it before the person leaves; suicide risk has not been screened until it is answered.",
            join_nums(&safety_unanswered)
        ));
    }
    let mut total = None;
    let mut partial = None;
    let mut severity = None;
    let outcome = match inst.response {
        ResponseKind::Frequency | ResponseKind::Options => {
            let sum: i32 = (1..=inst.item_count).filter_map(scored_value).sum();
            if missing.is_empty() {
                total = Some(sum);
                severity = inst
                    .bands
                    .iter()
                    .find(|b| sum >= b.min && sum <= b.max)
                    .map(|b| b.label.clone());
                match inst.positive_at {
                    Some(cut) if sum >= cut => ScreenOutcome::Positive,
                    Some(_) => ScreenOutcome::Negative,
                    None => ScreenOutcome::Scored,
                }
            } else {
                partial = Some(sum);
                notes.push(format!(
                    "Item(s) {} were not answered, so no total is given (the cutoffs apply only to complete screens). Ask the missing item(s) if you can.",
                    join_nums(&missing)
                ));
                // A partial sum already at or above the cutoff is still positive.
                match inst.positive_at {
                    Some(cut) if sum >= cut => {
                        notes.push(format!(
                            "The answered items already add up to {sum}, at or above the cutoff of {cut}, so the screen is positive whatever the missing answers are."
                        ));
                        ScreenOutcome::Positive
                    }
                    _ => ScreenOutcome::Incomplete,
                }
            }
        }
        ResponseKind::YesNo if inst.id == "asq" => {
            asq_outcome(&value, opts, &mut alert_ids, &mut notes, &mut flagged)
        }
        ResponseKind::YesNo => {
            cssrs_outcome(&value, opts, &mut alert_ids, &mut notes, &mut flagged)
        }
    };
    if inst.response != ResponseKind::YesNo {
        // nothing else
    } else {
        let yes: i32 = (1..=inst.item_count).filter_map(value).sum();
        partial = Some(yes);
    }
    let items = inst
        .items
        .iter()
        .map(|d| ItemOut {
            item: d.n,
            topic: d.topic.clone(),
            wording: if inst.license.wording_bundled {
                d.wording.clone()
            } else {
                None
            },
            entered: value(d.n),
            score: scored_value(d.n),
            safety_flag: flagged.contains(&d.n),
        })
        .collect();
    Ok(ScoredScreen {
        total_score: total,
        partial_sum: if inst.response == ResponseKind::YesNo {
            None
        } else {
            partial
        },
        severity,
        outcome,
        items,
        missing_items: missing,
        alert_ids,
        notes,
        safety_items_unanswered: safety_unanswered,
    })
}

fn push_alert(ids: &mut Vec<String>, id: &str) {
    if !ids.iter().any(|a| a == id) {
        ids.push(id.to_string());
    }
}

fn asq_outcome(
    value: &dyn Fn(u32) -> Option<i32>,
    opts: ScoreOptions,
    alerts: &mut Vec<String>,
    notes: &mut Vec<String>,
    flagged: &mut Vec<u32>,
) -> ScreenOutcome {
    let youth = opts.age_years.map(|a| a < 18);
    let any_answered = (1..=5).any(|n| value(n).is_some());
    if opts.refused && !any_answered {
        return match youth {
            Some(false) => {
                notes.push("An adult's refusal to answer is NOT a positive screen; no intervention is necessary unless there are other safety concerns. Document the refusal.".into());
                ScreenOutcome::Incomplete
            }
            Some(true) => {
                notes.push(
                    "A youth's refusal to answer is treated as a non-acute positive screen.".into(),
                );
                push_alert(alerts, "asq_nonacute_positive");
                ScreenOutcome::NonAcutePositive
            }
            None => {
                notes.push("The person refused, and no age was given. A youth's refusal counts as a non-acute positive screen (an adult's does not), so the safety steps are shown to be safe; give the age to refine this.".into());
                push_alert(alerts, "asq_nonacute_positive");
                ScreenOutcome::NonAcutePositive
            }
        };
    }
    let yes_1_4: Vec<u32> = (1..=4).filter(|n| value(*n) == Some(1)).collect();
    let missing_1_4: Vec<u32> = (1..=4).filter(|n| value(*n).is_none()).collect();
    flagged.extend(yes_1_4.iter().copied());
    let q5 = value(5);
    if q5 == Some(1) {
        flagged.push(5);
        if yes_1_4.is_empty() {
            notes.push("Question 5 was answered yes although no question 1–4 was yes; it is treated as an acute positive screen.".into());
        }
        push_alert(alerts, "asq_acute_positive");
        return ScreenOutcome::AcutePositive;
    }
    if yes_1_4.is_empty() {
        if missing_1_4.is_empty() {
            notes.push("No to all of questions 1–4: negative screen; question 5 is not needed. Clinical judgment can always override a negative screen.".into());
            return ScreenOutcome::Negative;
        }
        notes.push(format!(
            "Question(s) {} were not answered, so the screen cannot be called negative.",
            join_nums(&missing_1_4)
        ));
        if opts.refused && youth != Some(false) {
            push_alert(alerts, "asq_nonacute_positive");
            notes.push("Refusal by a youth (or when age is unknown) is treated as a non-acute positive screen.".into());
            return ScreenOutcome::NonAcutePositive;
        }
        return ScreenOutcome::Incomplete;
    }
    // Positive on 1–4.
    if yes_1_4 == [4] {
        let attempt_old = opts.asq_attempt_within_12_months == Some(false);
        let young = opts.age_years.is_none_or(|a| a < 25);
        if attempt_old && !young {
            notes.push("The only yes is question 4 (a past attempt more than a year ago): for adults, NIMH says a brief suicide safety assessment may not be necessary. It is still a positive screen; use clinical judgment.".into());
        } else if attempt_old && opts.asq_bssa_documented_since_attempt == Some(true) {
            notes.push("The only yes is question 4, the attempt was more than a year ago, and a brief suicide safety assessment has been documented since: for youth and young adults NIMH says another assessment may not be necessary. Use clinical judgment.".into());
        } else if opts.asq_attempt_within_12_months.is_none() {
            notes.push("Question 4 is yes: ask when the most recent attempt was (within the last 12 months, or more than a year ago).".into());
        }
    }
    match q5 {
        Some(_) => {
            push_alert(alerts, "asq_nonacute_positive");
            ScreenOutcome::NonAcutePositive
        }
        None => {
            notes.push("Yes to a question 1–4: ask question 5 (\"Are you having thoughts of killing yourself right now?\") now to decide acuity. Until it is answered, follow at least the non-acute steps.".into());
            push_alert(alerts, "asq_nonacute_positive");
            ScreenOutcome::PositiveAcuityUnknown
        }
    }
}

fn cssrs_outcome(
    value: &dyn Fn(u32) -> Option<i32>,
    opts: ScoreOptions,
    alerts: &mut Vec<String>,
    notes: &mut Vec<String>,
    flagged: &mut Vec<u32>,
) -> ScreenOutcome {
    let yes = |n: u32| value(n) == Some(1);
    for n in 1..=6 {
        if yes(n) {
            flagged.push(n);
        }
    }
    if value(2) == Some(0) && (3..=5).any(yes) {
        notes.push("Items 3–5 are normally skipped when item 2 is no; the yes answer(s) were still counted.".into());
    }
    let behavior_recent = if yes(6) {
        match opts.cssrs_behavior_past_3_months {
            Some(b) => Some(b),
            None => {
                notes.push("Item 6 is yes: ask whether the behavior was in the past 3 months. Until then it is treated as recent (high risk) to be safe.".into());
                Some(true)
            }
        }
    } else {
        None
    };
    let outcome = if yes(4) || yes(5) || behavior_recent == Some(true) {
        ScreenOutcome::HighRisk
    } else if yes(3) || behavior_recent == Some(false) {
        ScreenOutcome::ModerateRisk
    } else if yes(1) || yes(2) {
        ScreenOutcome::LowRisk
    } else {
        let required_missing: Vec<u32> = [1u32, 2, 6]
            .into_iter()
            .filter(|n| value(*n).is_none())
            .collect();
        if required_missing.is_empty() {
            ScreenOutcome::NoRisk
        } else {
            notes.push(format!(
                "Item(s) {} were not answered; items 1, 2, and 6 are always asked.",
                join_nums(&required_missing)
            ));
            ScreenOutcome::Incomplete
        }
    };
    // Low or moderate risk can only be called when the items that could raise
    // the level were answered: item 6 always, and items 3–5 unless item 2 is no.
    let outcome = if matches!(
        outcome,
        ScreenOutcome::LowRisk | ScreenOutcome::ModerateRisk
    ) {
        let mut ask: Vec<u32> = Vec::new();
        if value(2) != Some(0) || (3..=5).any(yes) {
            let unanswered: Vec<u32> = (3..=5).filter(|n| value(*n).is_none()).collect();
            if !unanswered.is_empty() {
                if value(2).is_none() {
                    ask.push(2);
                }
                ask.extend(unanswered);
            }
        }
        if value(6).is_none() {
            ask.push(6);
        }
        if ask.is_empty() {
            outcome
        } else {
            notes.push(format!(
                "Item(s) {} were not answered, so the risk level is not yet determined: it is at least {} and may be higher. Ask the item(s) now, before the person leaves, and score the screen again. Until then, follow at least the same-day (moderate risk) steps.",
                join_nums(&ask),
                if outcome == ScreenOutcome::LowRisk {
                    "low"
                } else {
                    "moderate"
                }
            ));
            ScreenOutcome::RiskNotDetermined
        }
    } else {
        outcome
    };
    match outcome {
        ScreenOutcome::HighRisk => push_alert(alerts, "cssrs_high"),
        ScreenOutcome::ModerateRisk | ScreenOutcome::RiskNotDetermined => {
            push_alert(alerts, "cssrs_moderate")
        }
        ScreenOutcome::LowRisk => push_alert(alerts, "cssrs_low"),
        _ => {}
    }
    outcome
}

fn join_nums(v: &[u32]) -> String {
    v.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
}

fn alerts_out(ids: &[String]) -> Vec<AlertOut> {
    let c = content();
    ids.iter()
        .filter_map(|id| alert_def(id))
        .map(|a| AlertOut {
            id: a.id.clone(),
            title: a.title.clone(),
            urgency: a.urgency.clone(),
            trigger: a.trigger_text.clone(),
            steps: cite_all(&c.sources, &a.steps),
        })
        .collect()
}

fn cutoff_summary(i: &Instrument) -> String {
    if !i.bands.is_empty() {
        let b: Vec<String> = i
            .bands
            .iter()
            .map(|b| format!("{}–{} {}", b.min, b.max, b.label))
            .collect();
        match i.positive_at {
            Some(p) => format!("{}; positive at {p}+", b.join(", ")),
            None => b.join(", "),
        }
    } else if let Some(p) = i.positive_at {
        format!("positive at {p} or more")
    } else if i.id == "asq" {
        "yes to any of 1–4 = positive; item 5 decides acute vs non-acute".into()
    } else {
        "risk level from the highest item endorsed (low, moderate, high)".into()
    }
}

fn base_caveats() -> Vec<String> {
    let mut v = vec![
        "Screening is not diagnosis. A positive screen needs evaluation by a qualified clinician, and clinical judgment can override any score.".to_string(),
        "Follow your agency's suicide risk protocol; this tool lists published steps but does not replace it.".to_string(),
    ];
    v.extend(c10::review_caveats(&[MH_SCREENING]));
    v
}

fn instrument_sources(inst: &Instrument, alerts: &[String]) -> Vec<Source> {
    let c = content();
    let mut keys: Vec<&str> = vec![
        inst.source.as_str(),
        inst.positive_source.as_str(),
        inst.license.source.as_str(),
    ];
    for id in alerts {
        if let Some(a) = alert_def(id) {
            keys.extend(a.steps.iter().map(|s| s.source.as_str()));
        }
    }
    if !alerts.is_empty() {
        keys.extend(c.resources_for_all.iter().map(|s| s.source.as_str()));
    }
    let mut out =
        vec![c10::content_source(MH_SCREENING).unwrap_or_else(|_| {
            Source::computed("Mental health screening content", "PHN MCP server")
        })];
    out.extend(c10::sources_for(&c.sources, keys));
    out.push(Source::computed(
        "Screening score calculated from the item responses given",
        "PHN MCP server",
    ));
    out
}

// ------------------------------------------------------------------ tool

/// Score a mental health screening instrument (or a file of screens).
pub fn score_mental_health_screen(
    ctx: &Context,
    args: ScreenArgs,
) -> Result<Envelope<ScreenResult>> {
    let c = content();
    let Some(q) = args.instrument.as_deref().filter(|q| !q.trim().is_empty()) else {
        return Ok(catalog());
    };
    let inst = find_instrument(q).ok_or_else(|| {
        PhnError::NotFound(format!(
            "'{q}' is not a supported instrument. Supported: {}. Licensed instruments such as the Beck Depression Inventory are not included.",
            c.instruments
                .iter()
                .map(|i| format!("{} ({})", i.id, i.name))
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })?;
    let opts = ScoreOptions {
        epds_entry: args.epds_entered_as.unwrap_or_default(),
        age_years: args.age_years,
        refused: args.refused,
        asq_attempt_within_12_months: args.asq_attempt_within_12_months,
        asq_bssa_documented_since_attempt: args.asq_bssa_documented_since_attempt,
        cssrs_behavior_past_3_months: args.cssrs_behavior_past_3_months,
    };
    if let Some(path) = args.file.as_deref().filter(|p| !p.trim().is_empty()) {
        return batch(ctx, inst, path, args.sheet.as_deref(), opts);
    }
    let scored = score(inst, &args.responses, opts)?;
    let mut alerts = alerts_out(&scored.alert_ids);
    if scored.outcome == ScreenOutcome::RiskNotDetermined {
        // The moderate (same-day) steps are shown as the floor, not as the result.
        for a in alerts.iter_mut().filter(|a| a.id == "cssrs_moderate") {
            a.title = "C-SSRS screener: risk level not yet determined".into();
            a.trigger = "A yes answer, but item(s) that decide the risk level were not answered. These same-day steps apply until the item(s) are asked; the level may be higher.".into();
        }
    }
    let resources = cite_all(&c.sources, &c.resources_for_all);
    let max_score =
        (inst.response != ResponseKind::YesNo).then(|| inst.item_max * inst.item_count as i32);

    // ---- next steps
    let mut next_steps = Vec::new();
    match scored.outcome {
        ScreenOutcome::Positive if inst.id == "phq2" => next_steps.push(
            "Complete the PHQ-9 (the PHQ-2 answers are its first two items) and arrange a clinical evaluation.".to_string(),
        ),
        ScreenOutcome::Positive if inst.id == "gad2" => next_steps.push(
            "Complete the GAD-7 and arrange a clinical evaluation.".to_string(),
        ),
        ScreenOutcome::Positive => next_steps.push(
            "Refer for clinical evaluation per your agency protocol; share the score and the date of the screen.".to_string(),
        ),
        ScreenOutcome::Incomplete => next_steps.push(
            "Complete the missing item(s) before interpreting the screen.".to_string(),
        ),
        ScreenOutcome::RiskNotDetermined => next_steps.push(
            "Ask the unanswered C-SSRS item(s) named in the notes now, before the person leaves, and score the screen again.".to_string(),
        ),
        _ => {}
    }
    if matches!(inst.id.as_str(), "phq9" | "gad7" | "epds")
        && scored.alert_ids.is_empty()
        && scored.safety_items_unanswered.is_empty()
    {
        next_steps.push(
            "Rescreen at the interval your program uses; scores can be tracked over time."
                .to_string(),
        );
    }
    if !scored.safety_items_unanswered.is_empty() {
        next_steps.insert(
            0,
            format!(
                "Ask item {} now, before the person leaves: it is the suicide-risk item and it was not answered.",
                join_nums(&scored.safety_items_unanswered)
            ),
        );
    }
    if !alerts.is_empty() {
        next_steps.insert(
            0,
            "Act on the safety alert(s) below first, before anything else.".to_string(),
        );
        next_steps.push("Find Massachusetts crisis services for the person's town (Behavioral Health Help Line, Community Behavioral Health Center, mobile crisis) with find_crisis_resources.".to_string());
    }

    // ---- markdown
    let mut s = format!("**{} — {}**\n\n", inst.name, scored.outcome.label());
    if !alerts.is_empty() {
        for a in &alerts {
            s.push_str(&format!(
                "**SAFETY ALERT ({}): {}** — {}\n",
                a.urgency.replace('_', " "),
                a.title,
                a.trigger
            ));
            for (i, st) in a.steps.iter().enumerate() {
                s.push_str(&format!("{}. {}\n", i + 1, st.text));
            }
            s.push('\n');
        }
    }
    if !scored.safety_items_unanswered.is_empty() {
        s.push_str(&format!(
            "**SAFETY: item {} (the suicide-risk item) was not answered.** Ask it before the person leaves; suicide risk has not been screened.\n\n",
            join_nums(&scored.safety_items_unanswered)
        ));
    }
    match (scored.total_score, max_score) {
        (Some(t), Some(m)) => s.push_str(&format!(
            "Total score: **{t}** of {m}{}.\n",
            scored
                .severity
                .as_deref()
                .map(|v| format!(" ({v})"))
                .unwrap_or_default()
        )),
        (None, Some(_)) => s.push_str(&format!(
            "No total: item(s) {} missing (answered items sum to {}).\n",
            join_nums(&scored.missing_items),
            scored.partial_sum.unwrap_or(0)
        )),
        _ => {}
    }
    s.push_str(&format!("\n{}\n", inst.positive_text.trim()));
    let rows: Vec<Vec<String>> = scored
        .items
        .iter()
        .map(|i| {
            vec![
                i.item.to_string(),
                i.topic.clone(),
                i.entered
                    .map(|v| {
                        if inst.response == ResponseKind::YesNo {
                            if v == 1 { "yes".into() } else { "no".into() }
                        } else {
                            v.to_string()
                        }
                    })
                    .unwrap_or_else(|| "—".into()),
                i.score
                    .filter(|_| inst.response != ResponseKind::YesNo)
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                if i.safety_flag {
                    "⚠".into()
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    s.push('\n');
    s.push_str(&md::table(
        &["Item", "Topic", "Answer", "Score", "Flag"],
        &rows,
    ));
    for n in &scored.notes {
        s.push_str(&format!("\n- {n}"));
    }
    if !scored.notes.is_empty() {
        s.push('\n');
    }
    if !next_steps.is_empty() {
        s.push_str("\n**Next steps**\n");
        for n in &next_steps {
            s.push_str(&format!("- {n}\n"));
        }
    }
    if !alerts.is_empty()
        || !scored.safety_items_unanswered.is_empty()
        || matches!(inst.id.as_str(), "asq" | "cssrs_screener")
    {
        s.push_str("\n**Give to everyone screened:** ");
        s.push_str(
            &resources
                .iter()
                .map(|r| r.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
        );
        s.push('\n');
    }
    s.push_str(&format!(
        "\n_License: {} Item wording {}bundled._\n\n_{}_\n",
        inst.license.terms.trim(),
        if inst.license.wording_bundled {
            ""
        } else {
            "not "
        },
        c.not_a_diagnosis.trim()
    ));

    let mut caveats = base_caveats();
    if inst.confidence != Confidence::High {
        caveats.push(format!(
            "The {} scoring rules are drafted from secondary sources ({} confidence); confirm them against the instrument your program uses.",
            inst.name,
            inst.confidence.label()
        ));
    }
    if inst.license.confidence != Confidence::High {
        caveats.push(format!(
            "{} license terms could not be confirmed with the owner ({} confidence); the nurse should confirm them.",
            inst.name,
            inst.license.confidence.label()
        ));
    }
    if inst.id == "phq9" && args.age_years.is_some_and(|a| a < 18) {
        caveats.push("For adolescents, NIMH's toolkit uses the PHQ-A (PHQ-9 modified for adolescents) paired with the ASQ.".into());
    }
    let mut privacy_notes = vec![
        "Only item answers were used; no names or identifiers are needed or kept.".to_string(),
    ];
    if let Some(d) = args.difficulty.as_deref().filter(|d| !d.trim().is_empty()) {
        privacy_notes.push(format!(
            "Functional difficulty recorded as given: {}.",
            md::cell(d.trim())
        ));
    }
    let interpretation_citation = c10::citation(&c.sources, &inst.positive_source, &inst.section);
    Ok(Envelope::new(
        s,
        ScreenResult {
            instrument: inst.id.clone(),
            instrument_name: inst.name.clone(),
            purpose: inst.purpose.clone(),
            lookback: inst.lookback.clone(),
            max_score,
            scored: Some(scored.clone()),
            bands: inst
                .bands
                .iter()
                .map(|b| BandOut {
                    min: b.min,
                    max: b.max,
                    label: b.label.clone(),
                })
                .collect(),
            interpretation: inst.positive_text.trim().to_string(),
            interpretation_citation,
            safety_alerts: alerts,
            resources,
            next_steps,
            instrument_notes: inst.notes.clone(),
            license: Some(inst.license.clone()),
            not_a_diagnosis: c.not_a_diagnosis.trim().to_string(),
            instruments: Vec::new(),
            batch: None,
        },
    )
    .sources(instrument_sources(inst, &scored.alert_ids))
    .caveats(caveats)
    .privacy(Privacy {
        notes: privacy_notes,
        ..Privacy::none()
    })
    .decision_support())
}

fn catalog() -> Envelope<ScreenResult> {
    let c = content();
    let list: Vec<InstrumentSummary> = c
        .instruments
        .iter()
        .map(|i| InstrumentSummary {
            id: i.id.clone(),
            name: i.name.clone(),
            purpose: i.purpose.clone(),
            items: i.item_count,
            cutoffs: cutoff_summary(i),
            license: i.license.clone(),
        })
        .collect();
    let mut s = String::from(
        "**Mental health screening instruments this tool can score**\n\nGive `instrument` and the item `responses` in order to score one screen, or `file` for a file of screens in /data.\n\n",
    );
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|i| {
            vec![
                format!("`{}`", i.id),
                i.name.clone(),
                i.items.to_string(),
                i.cutoffs.clone(),
                format!(
                    "{}; wording {}",
                    i.license.reuse.replace('_', " "),
                    if i.license.wording_bundled {
                        "bundled"
                    } else {
                        "not bundled"
                    }
                ),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &["Id", "Instrument", "Items", "Cutoffs", "License"],
        &rows,
    ));
    s.push_str(&format!("\n_{}_\n", c.not_a_diagnosis.trim()));
    let mut sources =
        vec![c10::content_source(MH_SCREENING).unwrap_or_else(|_| {
            Source::computed("Mental health screening content", "PHN MCP server")
        })];
    sources.extend(c10::sources_for(
        &c.sources,
        c.instruments.iter().map(|i| i.source.as_str()),
    ));
    Envelope::new(
        s,
        ScreenResult {
            instrument: String::new(),
            instrument_name: String::new(),
            purpose: String::new(),
            lookback: String::new(),
            max_score: None,
            scored: None,
            bands: Vec::new(),
            interpretation: String::new(),
            interpretation_citation: String::new(),
            safety_alerts: Vec::new(),
            resources: cite_all(&c.sources, &c.resources_for_all),
            next_steps: Vec::new(),
            instrument_notes: Vec::new(),
            license: None,
            not_a_diagnosis: c.not_a_diagnosis.trim().to_string(),
            instruments: list,
            batch: None,
        },
    )
    .sources(sources)
    .caveats(base_caveats())
    .decision_support()
}

// ------------------------------------------------------------------ batch

fn item_column(headers: &[String], inst: &Instrument, n: u32) -> Option<usize> {
    let id = inst.id.replace('_', "");
    let candidates = [
        format!("q{n}"),
        format!("item{n}"),
        format!("item {n}"),
        format!("item_{n}"),
        format!("question {n}"),
        format!("{}_{n}", inst.id),
        format!("{id}_{n}"),
        format!("{id} {n}"),
        format!("{id}{n}"),
        format!("{}_q{n}", inst.id),
        format!("{id}_q{n}"),
    ];
    headers.iter().position(|h| {
        let hn = phn_core::text::normalize(h);
        candidates.iter().any(|c| {
            phn_core::text::normalize(c) == hn || c.replace('_', "") == hn.replace(' ', "")
        })
    })
}

fn batch(
    ctx: &Context,
    inst: &Instrument,
    path: &str,
    sheet: Option<&str>,
    opts: ScoreOptions,
) -> Result<Envelope<ScreenResult>> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let table = &lt.table;
    let cols: Vec<Option<usize>> = (1..=inst.item_count)
        .map(|n| item_column(&table.headers, inst, n))
        .collect();
    if cols.iter().all(Option::is_none) {
        return Err(PhnError::InvalidInput(format!(
            "no item columns were found in '{}'. Name them q1..q{n} (or item1..item{n}, {id}_1..{id}_{n}); other columns are ignored and never returned.",
            lt.label,
            n = inst.item_count,
            id = inst.id
        )));
    }
    let age_col = table
        .column_index("age")
        .or_else(|| table.column_index("age_years"));
    let ref_col = table.column_index("refused");
    let recent_col = table
        .column_index("behavior_past_3_months")
        .or_else(|| table.column_index("past_3_months"));
    // Results per row, kept local.
    let mut out_rows: Vec<Vec<String>> = Vec::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut scored_n = 0usize;
    let mut incomplete = 0usize;
    let mut alerts_n = 0u64;
    // The "Safety alerts" cell of a row (rows with text here are listed first).
    let alert_cell = |sc: &ScoredScreen| -> String {
        let mut parts: Vec<String> = alerts_out(&sc.alert_ids)
            .iter()
            .map(|a| format!("{} ({})", a.title, a.urgency.replace('_', " ")))
            .collect();
        if !sc.safety_items_unanswered.is_empty() {
            parts.push(format!(
                "suicide-risk item {} not answered or not readable: ask it",
                join_nums(&sc.safety_items_unanswered)
            ));
        }
        parts.join("; ")
    };
    for (ri, row) in table.rows.iter().enumerate() {
        let responses: Vec<Option<ItemResponse>> = cols
            .iter()
            .map(|c| {
                c.and_then(|i| row.get(i))
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty())
                    .map(|v| match v.parse::<i64>() {
                        Ok(n) => ItemResponse::Number(n),
                        Err(_) => ItemResponse::Text(v.to_string()),
                    })
            })
            .collect();
        let mut o = opts;
        if let Some(a) = age_col
            .and_then(|i| row.get(i))
            .and_then(|v| v.trim().parse::<u32>().ok())
        {
            o.age_years = Some(a);
        }
        if let Some(r) = ref_col
            .and_then(|i| row.get(i))
            .and_then(|v| crate::localdata::truthy(v))
        {
            o.refused = r;
        }
        if let Some(r) = recent_col
            .and_then(|i| row.get(i))
            .and_then(|v| crate::localdata::truthy(v))
        {
            o.cssrs_behavior_past_3_months = Some(r);
        }
        let row_no = (ri + 2).to_string(); // header is row 1
        match score(inst, &responses, o) {
            Ok(sc) => {
                let key = sc
                    .severity
                    .clone()
                    .filter(|_| sc.outcome != ScreenOutcome::Incomplete)
                    .unwrap_or_else(|| sc.outcome.label().to_string());
                if !order.contains(&key) {
                    order.push(key.clone());
                }
                *counts.entry(key).or_default() += 1;
                if sc.outcome == ScreenOutcome::Incomplete {
                    incomplete += 1;
                } else {
                    scored_n += 1;
                }
                if !sc.alert_ids.is_empty() {
                    alerts_n += 1;
                }
                out_rows.push(vec![
                    row_no,
                    sc.total_score.map(|t| t.to_string()).unwrap_or_default(),
                    sc.severity.clone().unwrap_or_default(),
                    sc.outcome.label().to_string(),
                    alert_cell(&sc),
                    join_nums(&sc.missing_items),
                ]);
            }
            Err(e) => {
                incomplete += 1;
                let key = "could not be scored".to_string();
                if !order.contains(&key) {
                    order.push(key.clone());
                }
                *counts.entry(key).or_default() += 1;
                // Score again with the unreadable cells treated as unanswered,
                // so a readable positive safety item still raises its alert.
                let readable: Vec<Option<ItemResponse>> = responses
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        r.as_ref()
                            .filter(|x| parse_response(inst, i as u32 + 1, Some(*x)).is_ok())
                            .cloned()
                    })
                    .collect();
                let alerts_text = match score(inst, &readable, o) {
                    Ok(sc) => {
                        if !sc.alert_ids.is_empty() {
                            alerts_n += 1;
                        }
                        alert_cell(&sc)
                    }
                    Err(_) => String::new(),
                };
                out_rows.push(vec![
                    row_no,
                    String::new(),
                    String::new(),
                    "could not be scored".into(),
                    alerts_text,
                    e.to_string(),
                ]);
            }
        }
    }
    // Sort rows with alerts first, so the nurse sees them first.
    out_rows.sort_by_key(|r| (r[4].is_empty(), r[0].parse::<usize>().unwrap_or(0)));
    let doc = phn_docs::model::Document {
        title: format!("{} results", inst.name),
        subtitle: Some("Rows with safety alerts are listed first. Row numbers refer to the source file (header = row 1).".into()),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks: vec![phn_docs::model::Block::Table(phn_docs::model::TableBlock {
            caption: Some(format!("{} results", inst.name)),
            headers: ["Source row", "Total", "Severity", "Result", "Safety alerts", "Missing items / problem"]
                .map(String::from)
                .to_vec(),
            rows: out_rows,
        })],
    };
    let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
    let sandbox = ctx.sandbox()?;
    let out = sandbox.new_output_file(&format!("{}_screening_results.xlsx", inst.id))?;
    std::fs::write(&out, bytes)?;
    let out_display = sandbox.display_path(&out);

    // Suppressed counts. The outcome counts form one row with their total
    // (every row of the file falls in exactly one outcome), so a second cell
    // is hidden whenever one is and the hidden count cannot be found by
    // subtracting the shown counts from the number of screens.
    let threshold = ctx.suppression_threshold();
    let mut row: Vec<Option<u64>> = order.iter().map(|k| Some(counts[k])).collect();
    row.push(Some(table.rows.len() as u64));
    let sup = suppress(
        &[row.clone()],
        &SuppressionOptions {
            threshold,
            last_column_is_total: true,
            ..SuppressionOptions::default()
        },
    );
    let shown = |i: usize| {
        if sup.is_hidden(0, i) {
            "*".to_string()
        } else {
            row[i].unwrap_or(0).to_string()
        }
    };
    let mut count_map = BTreeMap::new();
    for (i, k) in order.iter().enumerate() {
        count_map.insert(k.clone(), shown(i));
    }
    let outcomes_suppressed = sup.primary + sup.complementary > 0;
    // The alert count overlaps the outcomes (it is not part of that row), so
    // the threshold is applied to it on its own.
    let alerts_hidden = alerts_n > 0 && alerts_n < u64::from(threshold);
    let alerts_shown = if alerts_hidden {
        "*".to_string()
    } else {
        alerts_n.to_string()
    };
    let any_suppressed = outcomes_suppressed || alerts_hidden;
    // The scored / incomplete split is a sum of outcome counts: publish it
    // only when no outcome count is hidden.
    let split = if outcomes_suppressed {
        String::new()
    } else {
        format!("Scored {scored_n}; incomplete or not scorable {incomplete}. ")
    };

    let mut s = format!(
        "**{} — file of {} screen(s)**\n\n{split}Per-row results (row numbers only, alerts first) were written to `{out_display}`; nothing from the rows is shown here.\n\n",
        inst.name,
        table.rows.len()
    );
    let rows: Vec<Vec<String>> = order
        .iter()
        .map(|k| vec![k.clone(), count_map[k].clone()])
        .collect();
    s.push_str(&md::table(&["Result", "Screens"], &rows));
    s.push_str(&format!(
        "\n**Screens with a safety alert: {alerts_shown}.** Open the output file now and follow up on each one today.\n"
    ));
    if !inst.safety_items.is_empty() {
        s.push_str("Rows where the suicide-risk item was not answered or could not be read are listed first too: ask that item.\n");
    }
    if any_suppressed {
        s.push_str(&format!(
            "\n`*` = suppressed (counts from 1 to {} and cells that would reveal them).\n",
            threshold - 1
        ));
    }
    let mut caveats = base_caveats();
    caveats.push(format!(
        "Columns used: {} item column(s) found out of {}; missing columns count as unanswered items.",
        cols.iter().filter(|c| c.is_some()).count(),
        inst.item_count
    ));
    Ok(Envelope::new(
        s,
        ScreenResult {
            instrument: inst.id.clone(),
            instrument_name: inst.name.clone(),
            purpose: inst.purpose.clone(),
            lookback: inst.lookback.clone(),
            max_score: (inst.response != ResponseKind::YesNo)
                .then(|| inst.item_max * inst.item_count as i32),
            scored: None,
            bands: inst
                .bands
                .iter()
                .map(|b| BandOut {
                    min: b.min,
                    max: b.max,
                    label: b.label.clone(),
                })
                .collect(),
            interpretation: inst.positive_text.trim().to_string(),
            interpretation_citation: c10::citation(
                &content().sources,
                &inst.positive_source,
                &inst.section,
            ),
            safety_alerts: Vec::new(),
            resources: cite_all(&content().sources, &content().resources_for_all),
            next_steps: vec![format!(
                "Open `{out_display}` and follow up on every row with a safety alert today."
            )],
            instrument_notes: inst.notes.clone(),
            license: Some(inst.license.clone()),
            not_a_diagnosis: content().not_a_diagnosis.trim().to_string(),
            instruments: Vec::new(),
            batch: Some(ScreenBatch {
                file: lt.label.clone(),
                rows_read: table.rows.len(),
                rows_scored: (!outcomes_suppressed).then_some(scored_n),
                rows_incomplete: (!outcomes_suppressed).then_some(incomplete),
                counts: count_map,
                rows_with_safety_alerts: alerts_shown,
                output_file: out_display.clone(),
                suppression_threshold: threshold,
            }),
        },
    )
    .source(lt.source())
    .sources(instrument_sources(inst, &[]))
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: lt.identifier_categories(),
        files_written: vec![out_display],
        notes: vec!["Only counts are returned; row-level results are in the output file.".into()],
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(v: &[i64]) -> Vec<Option<ItemResponse>> {
        v.iter().map(|x| Some(ItemResponse::Number(*x))).collect()
    }

    fn yn(v: &[Option<bool>]) -> Vec<Option<ItemResponse>> {
        v.iter().map(|x| x.map(ItemResponse::YesNo)).collect()
    }

    fn inst(id: &str) -> &'static Instrument {
        find_instrument(id).unwrap()
    }

    #[test]
    fn content_loads_with_known_sources_and_alerts() {
        let c = content();
        assert_eq!(c.instruments.len(), 7);
        for i in &c.instruments {
            c10::assert_keys_known(
                &c.sources,
                &[&i.source, &i.positive_source, &i.license.source],
                &i.id,
            );
            assert_eq!(i.items.len(), i.item_count as usize, "{}", i.id);
            for s in &i.safety_items {
                assert!(alert_def(&s.alert).is_some(), "{}: {}", i.id, s.alert);
            }
            // Bands cover the whole range without gaps.
            if !i.bands.is_empty() {
                let mut next = 0;
                for b in &i.bands {
                    assert_eq!(b.min, next, "{} band gap", i.id);
                    next = b.max + 1;
                }
                assert_eq!(next - 1, i.item_max * i.item_count as i32, "{}", i.id);
            }
        }
        for a in &c.safety_alerts {
            for s in &a.steps {
                c10::assert_keys_known(&c.sources, &[&s.source], &a.id);
            }
        }
        assert_eq!(find_instrument("PHQ-9").unwrap().id, "phq9");
        assert_eq!(find_instrument("Columbia").unwrap().id, "cssrs_screener");
        assert!(find_instrument("BDI").is_none());
    }

    #[test]
    fn phq9_item9_alert_is_independent_of_total() {
        let s = score(
            inst("phq9"),
            &n(&[0, 0, 0, 0, 0, 0, 0, 0, 1]),
            ScoreOptions::default(),
        )
        .unwrap();
        assert_eq!(s.total_score, Some(1));
        assert_eq!(s.outcome, ScreenOutcome::Negative);
        assert_eq!(s.alert_ids, vec!["suicide_item_positive"]);
        assert!(s.items[8].safety_flag);
    }

    #[test]
    fn epds_positions_are_reverse_scored() {
        // Every answer is the first option: items 1, 2, 4 score 0, the other seven score 3.
        let s = score(
            inst("epds"),
            &n(&[0; 10]),
            ScoreOptions {
                epds_entry: EpdsEntry::OptionPositions,
                ..ScoreOptions::default()
            },
        )
        .unwrap();
        assert_eq!(s.total_score, Some(21));
        assert!(s.alert_ids.contains(&"suicide_item_positive".to_string()));
    }

    #[test]
    fn out_of_range_and_extra_items_are_errors() {
        assert!(
            score(
                inst("gad7"),
                &n(&[0, 1, 4, 0, 0, 0, 0]),
                ScoreOptions::default()
            )
            .is_err()
        );
        assert!(score(inst("phq2"), &n(&[0, 1, 1]), ScoreOptions::default()).is_err());
        let e = score(
            inst("asq"),
            &[Some(ItemResponse::Text("maybe".into()))],
            ScoreOptions::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("yes or no"), "{e}");
    }

    #[test]
    fn asq_and_cssrs_branches() {
        let o = |r: &[Option<bool>], opts: ScoreOptions| {
            score(inst("asq"), &yn(r), opts).unwrap().outcome
        };
        let d = ScoreOptions::default();
        assert_eq!(o(&[Some(false); 4], d), ScreenOutcome::Negative);
        assert_eq!(
            o(
                &[
                    Some(true),
                    Some(false),
                    Some(false),
                    Some(false),
                    Some(false)
                ],
                d
            ),
            ScreenOutcome::NonAcutePositive
        );
        assert_eq!(
            o(
                &[
                    Some(false),
                    Some(false),
                    Some(true),
                    Some(false),
                    Some(true)
                ],
                d
            ),
            ScreenOutcome::AcutePositive
        );
        assert_eq!(
            o(&[Some(true), Some(false), Some(false), Some(false)], d),
            ScreenOutcome::PositiveAcuityUnknown
        );
        let refused_adult = ScoreOptions {
            refused: true,
            age_years: Some(40),
            ..d
        };
        let r = score(inst("asq"), &[], refused_adult).unwrap();
        assert_eq!(r.outcome, ScreenOutcome::Incomplete);
        assert!(r.alert_ids.is_empty());
        let refused_youth = ScoreOptions {
            refused: true,
            age_years: Some(14),
            ..d
        };
        assert_eq!(
            score(inst("asq"), &[], refused_youth).unwrap().outcome,
            ScreenOutcome::NonAcutePositive
        );

        let c = |r: &[Option<bool>], recent: Option<bool>| {
            score(
                inst("cssrs_screener"),
                &yn(r),
                ScoreOptions {
                    cssrs_behavior_past_3_months: recent,
                    ..d
                },
            )
            .unwrap()
            .outcome
        };
        let no = Some(false);
        let yes = Some(true);
        assert_eq!(
            c(&[no, no, None, None, None, no], None),
            ScreenOutcome::NoRisk
        );
        assert_eq!(
            c(&[yes, no, None, None, None, no], None),
            ScreenOutcome::LowRisk
        );
        assert_eq!(
            c(&[yes, yes, yes, no, no, no], None),
            ScreenOutcome::ModerateRisk
        );
        assert_eq!(
            c(&[yes, yes, no, yes, no, no], None),
            ScreenOutcome::HighRisk
        );
        assert_eq!(
            c(&[no, no, None, None, None, yes], Some(false)),
            ScreenOutcome::ModerateRisk
        );
        assert_eq!(
            c(&[no, no, None, None, None, yes], Some(true)),
            ScreenOutcome::HighRisk
        );
        // Unknown timing is treated as recent.
        assert_eq!(
            c(&[no, no, None, None, None, yes], None),
            ScreenOutcome::HighRisk
        );
        assert_eq!(c(&[no, no], None), ScreenOutcome::Incomplete);
    }

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn cssrs_unanswered_deciding_items_are_not_low_risk() {
        let d = ScoreOptions::default();
        let yes = Some(true);
        let no = Some(false);
        // Yes to items 1 and 2 only: items 3-5 and 6 decide the level.
        let s = score(inst("cssrs_screener"), &yn(&[yes, yes]), d).unwrap();
        assert_eq!(s.outcome, ScreenOutcome::RiskNotDetermined);
        assert_eq!(s.alert_ids, vec!["cssrs_moderate"]);
        assert!(
            s.notes.iter().any(|n| n.contains("3, 4, 5, 6")),
            "{:?}",
            s.notes
        );
        // Item 2 is no, so items 3-5 are skipped, but item 6 was not asked.
        let s = score(inst("cssrs_screener"), &yn(&[yes, no]), d).unwrap();
        assert_eq!(s.outcome, ScreenOutcome::RiskNotDetermined);
        assert!(
            s.notes.iter().any(|n| n.contains("Item(s) 6 ")),
            "{:?}",
            s.notes
        );
        // Intent (item 4) is high risk whatever else is missing.
        let s = score(inst("cssrs_screener"), &yn(&[yes, yes, None, yes]), d).unwrap();
        assert_eq!(s.outcome, ScreenOutcome::HighRisk);
        // Complete answers keep their level.
        let s = score(
            inst("cssrs_screener"),
            &yn(&[yes, yes, no, no, no, no]),
            d,
        )
        .unwrap();
        assert_eq!(s.outcome, ScreenOutcome::LowRisk);
        assert!(ScreenOutcome::RiskNotDetermined.is_positive());

        // The tool shows the same-day steps as a floor and says what to ask.
        let (_d, c) = ctx();
        let env = score_mental_health_screen(
            &c,
            ScreenArgs {
                instrument: Some("cssrs_screener".into()),
                responses: yn(&[yes, yes]),
                ..ScreenArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.safety_alerts[0].urgency, "same_day");
        assert!(env.data.safety_alerts[0].title.contains("not yet determined"));
        let headline = env.summary.lines().next().unwrap_or_default();
        assert!(headline.contains("not yet determined"), "{headline}");
        assert!(env.summary.contains("988"));
        assert!(
            env.data
                .next_steps
                .iter()
                .any(|s| s.contains("Ask the unanswered C-SSRS item(s)"))
        );
    }

    #[test]
    fn unanswered_safety_item_is_called_out() {
        let (_d, c) = ctx();
        // PHQ-9 with item 9 skipped: answered items sum to 16.
        let mut r = n(&[2, 2, 2, 2, 2, 2, 2, 2]);
        r.push(None);
        let env = score_mental_health_screen(
            &c,
            ScreenArgs {
                instrument: Some("phq9".into()),
                responses: r,
                ..ScreenArgs::default()
            },
        )
        .unwrap();
        let sc = env.data.scored.as_ref().unwrap();
        assert_eq!(sc.safety_items_unanswered, vec![9]);
        assert!(sc.alert_ids.is_empty());
        assert!(
            env.data.next_steps[0].contains("item 9"),
            "{:?}",
            env.data.next_steps
        );
        assert!(!env.data.next_steps.iter().any(|s| s.contains("Rescreen")));
        assert!(env.summary.contains("SAFETY: item 9"));
        assert!(env.summary.contains("988"));
        // A complete screen is unchanged.
        let s = score(inst("phq9"), &n(&[0; 9]), ScoreOptions::default()).unwrap();
        assert!(s.safety_items_unanswered.is_empty());
        // EPDS item 10.
        let s = score(inst("epds"), &n(&[1; 9]), ScoreOptions::default()).unwrap();
        assert_eq!(s.safety_items_unanswered, vec![10]);
    }

    fn batch_of(c: &Context, dir: &std::path::Path, csv: &str) -> ScreenBatch {
        std::fs::write(dir.join("data/screens.csv"), csv).unwrap();
        score_mental_health_screen(
            c,
            ScreenArgs {
                instrument: Some("phq9".into()),
                file: Some("screens.csv".into()),
                ..ScreenArgs::default()
            },
        )
        .unwrap()
        .data
        .batch
        .unwrap()
    }

    #[test]
    fn batch_alert_survives_an_unreadable_cell() {
        let (dir, c) = ctx();
        // Six rows: item 3 is out of range (a typo), item 9 is positive.
        let mut csv = String::from("q1,q2,q3,q4,q5,q6,q7,q8,q9\n");
        for _ in 0..6 {
            csv.push_str("1,1,9,1,1,1,1,1,3\n");
        }
        let b = batch_of(&c, dir.path(), &csv);
        assert_eq!(b.counts["could not be scored"], "6");
        assert_eq!(b.rows_with_safety_alerts, "6");
    }

    #[test]
    fn batch_hidden_count_cannot_be_found_by_subtraction() {
        let (dir, c) = ctx();
        let mut csv = String::from("q1,q2,q3,q4,q5,q6,q7,q8,q9\n");
        for _ in 0..6 {
            csv.push_str("0,0,0,0,0,0,0,0,1\n"); // minimal, with a safety alert
        }
        for _ in 0..6 {
            csv.push_str("0,0,0,0,0,0,0,0,0\n"); // minimal
        }
        for _ in 0..10 {
            csv.push_str("1,1,1,1,1,0,0,0,0\n"); // mild (5)
        }
        for _ in 0..3 {
            csv.push_str("3,3,3,3,3,3,3,3,0\n"); // severe (24)
        }
        let b = batch_of(&c, dir.path(), &csv);
        assert_eq!(b.rows_read, 25);
        assert_eq!(b.counts["severe"], "*");
        // A second outcome count is hidden with it, not the alert count.
        assert_eq!(b.counts.values().filter(|v| *v == "*").count(), 2);
        assert_eq!(b.rows_with_safety_alerts, "6");
        assert_eq!(b.rows_scored, None);
        assert_eq!(b.rows_incomplete, None);
    }
}
