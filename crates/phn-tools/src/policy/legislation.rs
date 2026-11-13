//! POL-01 `track_ma_legislation`: finds Massachusetts bills on public health
//! topics in the Legislature's public data (the 194th General Court,
//! 2025-2026), with each bill's stage, committee, hearing dates, last action,
//! and a short cited excerpt of its text. A topic watchlist can be read from
//! and saved to the local folder.
//!
//! The tool compiles what the Legislature publishes and the questions a nurse
//! asks of a bill. It never says what a bill would do: excerpts are at most
//! 25 words, cited, and the "stage" is a label derived from the history by
//! rules shown in the content file (the last action is always shown too).
//!
//! Modes: live (the API, with a one-day cache of the bill list), then the
//! bundled dated snapshot when the API cannot be reached. The result says
//! which was used. No personal data is sent: the requests contain only a
//! General Court number and bill numbers.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::malegislature::{self, BillListing, HistoryAction};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, DocSource};
use crate::md;
use crate::policy::shared::{clean_text, contains_phrase, refuse_identifiers, stem_words};

/// Content id of `content/policy/ma_legislation_tracking.yaml`.
pub const CONTENT: &str = "policy-ma-legislation-tracking";
/// Snapshot id and file (see `xtask/scripts/build_tracking_snapshots.py`).
pub const SNAPSHOT_ID: &str = "ma-legislation-194";
pub const SNAPSHOT_FILE: &str = "ma_legislation_194.json";

const DEFAULT_BILLS: usize = 15;
const MAX_BILLS: usize = 40;
const MAX_KEYWORDS: usize = 10;
const MAX_WORDS: usize = 25;
const DETAIL_BATCH: usize = 8;
/// Words of the bill text kept before the first keyword in an excerpt.
const LEAD_WORDS: usize = 8;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralCourtDef {
    pub number: u32,
    pub years: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicDef {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageRule {
    pub stage: String,
    pub label: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageDef {
    pub stage: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionDef {
    pub id: String,
    pub question: String,
}

/// `content/policy/ma_legislation_tracking.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub general_court: GeneralCourtDef,
    pub how_to_search: Vec<String>,
    pub topics: Vec<TopicDef>,
    pub stage_rules: Vec<StageRule>,
    pub default_stage: StageDef,
    pub analysis_questions: Vec<QuestionDef>,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- snapshot

#[derive(Debug, Clone, Deserialize)]
pub struct SnapBill {
    pub bill: String,
    pub docket: Option<String>,
    pub title: String,
    pub cosponsors: usize,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub recommendations: Vec<(String, String)>,
    pub excerpt: Option<String>,
    pub history: Vec<(String, String, String, bool)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnapFile {
    pub general_court: u32,
    pub as_of: String,
    pub topic_definitions: Vec<TopicDef>,
    pub bills: Vec<SnapBill>,
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `track_ma_legislation`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TrackArgs {
    /// Topics to watch, by id or name: vaccines, communicable_disease, tobacco_nicotine,
    /// substance_use, mental_health, maternal_child, school_child_care, older_adults,
    /// environmental_health, food_nutrition, local_public_health, nursing_workforce,
    /// health_equity, oral_health, emergency_preparedness, health_coverage,
    /// reproductive_sexual_health, injury_safety. Leave topics, keywords, bill numbers, and the
    /// watchlist empty to list the topics.
    pub topics: Vec<String>,
    /// Extra words to find in bill TITLES (for example "PFAS", "school nurse"). Up to 10, each at
    /// most 60 characters; no names of people.
    pub keywords: Vec<String>,
    /// Specific bills to follow, for example "H3444" or "S 2518".
    pub bill_numbers: Vec<String>,
    /// Show only bills at these stages: filed, in_committee, reported_out, floor,
    /// awaiting_governor, enacted, study_or_new_draft. Default: all stages.
    pub stages: Vec<String>,
    /// Show only bills whose latest scheduled hearing is today or later.
    pub only_upcoming_hearings: Option<bool>,
    /// Most bills examined and shown (default 15, at most 40). In live mode each bill costs two
    /// requests, so stage and hearing filters apply to the bills examined.
    pub max_bills: Option<u32>,
    /// Include a cited excerpt (25 words or fewer) of each bill's text (default true).
    pub include_excerpts: Option<bool>,
    /// General Court number (default 194, 2025-2026; the live API also has earlier courts, from
    /// 189). The offline snapshot covers only the 194th.
    pub general_court: Option<u32>,
    /// A watchlist CSV or XLSX in your data folder with columns `kind` (topic, keyword, or bill),
    /// `value`, and optionally `note`. Its topics, keywords, and bills are added to the search. The
    /// file is refused if it looks like a list of people.
    pub watchlist_path: Option<String>,
    /// Save the topics, keywords, and bills of this search, with the latest stage and last action
    /// of each bill, as a watchlist CSV in your output folder (default false). The file can be
    /// given back as `watchlist_path` next time.
    pub save_watchlist: Option<bool>,
    /// Date to use for "today" (YYYY-MM-DD, 1900-2100). Default: today.
    pub as_of: Option<String>,
}

/// A topic and how many bills in the list matched it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TopicOut {
    pub id: String,
    pub label: String,
    /// Bills in the General Court's list (or the snapshot) whose titles match this topic.
    pub bills_matched: usize,
    /// True when this search asked for the topic.
    pub requested: bool,
}

/// One history action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActionOut {
    pub date: String,
    pub branch: String,
    pub text: String,
}

/// A hearing line from the bill's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HearingOut {
    /// Date the history line was recorded.
    pub recorded: String,
    /// "scheduled", "rescheduled", or "cancelled".
    pub kind: String,
    /// The hearing date in the line (YYYY-MM-DD), when it has one.
    pub hearing_date: Option<String>,
    /// The line as the Legislature recorded it (time and room).
    pub text: String,
}

/// A short quotation from the bill text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExcerptOut {
    /// At most 25 words; "…" marks where text was left out.
    pub text: String,
    pub source: String,
}

/// One bill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BillOut {
    pub bill_number: String,
    pub title: String,
    pub url: String,
    /// Number of cosponsors (names are on the bill page).
    pub cosponsors: usize,
    /// Topic ids whose keywords appear in the title.
    pub topics: Vec<String>,
    /// This project's reading of the history (see the stage rules in the content): filed,
    /// in_committee, reported_out, floor, awaiting_governor, enacted, or study_or_new_draft.
    pub stage: String,
    pub stage_label: String,
    /// The latest committee the bill was referred or discharged to, from the history.
    pub committee: Option<String>,
    pub last_action: Option<ActionOut>,
    pub days_since_last_action: Option<i64>,
    /// The latest hearing lines (at most three).
    pub hearings: Vec<HearingOut>,
    /// Date of the latest hearing line that is not a cancellation.
    pub latest_hearing_date: Option<String>,
    /// True when that date is today or later.
    pub hearing_upcoming: bool,
    pub days_until_hearing: Option<i64>,
    /// The committee's reporting deadline from the latest "Reporting date extended" line.
    pub reporting_deadline: Option<String>,
    /// What the committee recommended, when recorded (for example "Accompanied").
    pub committee_recommendations: Vec<String>,
    pub excerpt: Option<ExcerptOut>,
    /// "live", "cache", or "snapshot".
    pub data_mode: String,
    /// When this bill's data was read (RFC 3339 or YYYY-MM-DD).
    pub as_of: String,
}

/// A watchlist read from or saved to a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WatchlistOut {
    /// The file read (path under your data folder), if any.
    pub read_from: Option<String>,
    pub topics: Vec<String>,
    pub keywords: Vec<String>,
    pub bills: Vec<String>,
    /// Entries of the file that were not understood (unknown topic ids, bad bill numbers).
    pub unrecognized: Vec<String>,
    /// The file written to your output folder, if `save_watchlist` was true.
    pub saved_file: Option<String>,
}

/// Result of `track_ma_legislation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrackResult {
    pub general_court: u32,
    pub years: String,
    /// "live", "cache", or "snapshot": where the bill list came from.
    pub data_mode: String,
    /// When the data was read.
    pub as_of: String,
    /// Why a fallback was used, or what the mode means.
    pub mode_note: String,
    pub today: String,
    pub topics: Vec<TopicOut>,
    pub keywords: Vec<String>,
    /// Bills whose titles matched, before stage and hearing filters and the limit.
    pub title_matches: usize,
    pub examined: usize,
    pub bills: Vec<BillOut>,
    /// Bill numbers asked for that the Legislature has no record of.
    pub not_found: Vec<String>,
    pub watchlist: Option<WatchlistOut>,
    /// Questions to put to any bill (your analysis; the tool does not assert what a bill does).
    pub analysis_questions: Vec<String>,
    pub how_to_search: Vec<String>,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- engine (pure)

/// Compiled stage rules.
pub struct StageEngine {
    rules: Vec<(String, String, Vec<Regex>)>,
    default: StageDef,
}

impl StageEngine {
    pub fn new(c: &LegContent) -> Result<Self> {
        let mut rules = Vec::new();
        for r in &c.stage_rules {
            let mut res = Vec::new();
            for p in &r.patterns {
                res.push(Regex::new(&format!("(?i){p}")).map_err(|e| {
                    PhnError::Internal(format!("stage rule '{}' pattern '{p}': {e}", r.stage))
                })?);
            }
            rules.push((r.stage.clone(), r.label.clone(), res));
        }
        Ok(Self {
            rules,
            default: c.default_stage.clone(),
        })
    }

    /// The stage of the latest history action that matches a rule (stricken actions are
    /// ignored); the default stage when none does.
    pub fn stage_of(&self, history: &[HistoryAction]) -> (String, String) {
        for a in history.iter().rev().filter(|a| !a.is_stricken) {
            for (stage, label, pats) in &self.rules {
                if pats.iter().any(|p| p.is_match(&a.action)) {
                    return (stage.clone(), label.clone());
                }
            }
        }
        (self.default.stage.clone(), self.default.label.clone())
    }

    pub fn stage_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self.rules.iter().map(|r| r.0.clone()).collect();
        v.push(self.default.stage.clone());
        v.dedup();
        v
    }
}

/// Amendment lines name the member who offered the amendment; the name is left out.
pub fn omit_member(mut a: HistoryAction) -> HistoryAction {
    if let Ok(re) = Regex::new(r"^(Amendment\s+#?\d+[A-Za-z]?)\s*\([^)]*\)") {
        a.action = re.replace(&a.action, "$1 (member omitted)").into_owned();
    }
    a
}

/// A date from `MM/DD/YYYY` text, between 1900 and 2100.
pub fn us_date(text: &str) -> Option<NaiveDate> {
    let re = Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").ok()?;
    let c = re.captures(text)?;
    let (m, d, y): (u32, u32, i32) = (c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
    let date = NaiveDate::from_ymd_opt(y, m, d)?;
    (1900..=2100).contains(&date.year()).then_some(date)
}

/// Hearing lines of a history, oldest first.
pub fn hearings(history: &[HistoryAction]) -> Vec<HearingOut> {
    let kind_re =
        Regex::new(r"(?i)^hearing\s+(scheduled|rescheduled|cancel\w*|postponed)\b").unwrap();
    history
        .iter()
        .filter(|a| !a.is_stricken)
        .filter_map(|a| {
            let k = kind_re.captures(&a.action)?;
            let word = k[1].to_ascii_lowercase();
            let kind = if word.starts_with("cancel") || word == "postponed" {
                "cancelled"
            } else if word == "rescheduled" {
                "rescheduled"
            } else {
                "scheduled"
            };
            Some(HearingOut {
                recorded: a.date.clone(),
                kind: kind.into(),
                hearing_date: us_date(&a.action).map(|d| d.to_string()),
                text: a.action.clone(),
            })
        })
        .collect()
}

/// The latest hearing that is not cancelled: its date and whether it is today or later.
pub fn latest_hearing(hs: &[HearingOut], today: NaiveDate) -> (Option<String>, bool, Option<i64>) {
    match hs.last() {
        Some(h) if h.kind != "cancelled" => match h.hearing_date.as_deref() {
            Some(d) => {
                let date = NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                let days = date.map(|x| (x - today).num_days());
                (Some(d.to_string()), days.is_some_and(|n| n >= 0), days)
            }
            None => (None, false, None),
        },
        _ => (None, false, None),
    }
}

/// The committee of the latest "referred" or "discharged" action.
pub fn current_committee(history: &[HistoryAction]) -> Option<String> {
    let under = Regex::new(r"(?i)\s*\(under[^)]*\)").unwrap();
    history
        .iter()
        .rev()
        .filter(|a| !a.is_stricken)
        .find_map(|a| {
            let lower = a.action.to_lowercase();
            if !(lower.contains("referred") || lower.contains("discharged")) {
                return None;
            }
            let at = lower.rfind("committee on ")?;
            // `to_lowercase` can change byte offsets for some scripts; the text is ASCII here.
            let rest = a.action.get(at + "committee on ".len()..)?;
            let c = under.replace_all(rest, "");
            let c = c.trim().trim_end_matches(['.', ',']).trim().to_string();
            (!c.is_empty() && c.chars().count() <= 120).then_some(c)
        })
}

/// The committee's reporting deadline from the latest "Reporting date extended" line.
pub fn reporting_deadline(history: &[HistoryAction]) -> Option<String> {
    let re = Regex::new(r"(?i)reporting date extended\s+(?:to|until)\s+(.+?)\s*$").unwrap();
    history
        .iter()
        .rev()
        .filter(|a| !a.is_stricken)
        .find_map(|a| {
            let rest = re.captures(&a.action)?.get(1)?.as_str().to_string();
            let rest = rest.trim_end_matches('.').replace(',', "");
            for f in ["%A %B %d %Y", "%B %d %Y"] {
                if let Ok(d) = NaiveDate::parse_from_str(&rest, f) {
                    return (1900..=2100).contains(&d.year()).then(|| d.to_string());
                }
            }
            None
        })
}

/// At most `MAX_WORDS` words of `text` around the first keyword (`LEAD_WORDS` words of lead-in),
/// with "…" where text was left out. The first words when no keyword occurs.
pub fn excerpt_around(text: &str, keywords: &[String]) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    let flat = words.join(" ");
    let mut best: Option<usize> = None;
    for k in keywords {
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        if let Ok(re) = Regex::new(&format!(r"(?i)\b{}", regex::escape(k)))
            && let Some(m) = re.find(&flat)
        {
            best = Some(best.map_or(m.start(), |b| b.min(m.start())));
        }
    }
    let idx = best.map_or(0, |pos| {
        flat[..pos]
            .split_whitespace()
            .count()
            .saturating_sub(LEAD_WORDS)
    });
    let end = (idx + MAX_WORDS).min(words.len());
    let mut s = words[idx..end].join(" ");
    if idx > 0 {
        s = format!("… {s}");
    }
    if end < words.len() {
        s.push_str(" …");
    }
    Some(s)
}

/// Words in an excerpt, not counting the ellipses.
pub fn excerpt_words(s: &str) -> usize {
    s.replace('…', " ").split_whitespace().count()
}

/// Whole days from `today` to `date` (negative when `date` has passed).
pub fn days_from(today: NaiveDate, date: &str) -> Option<i64> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|d| (d - today).num_days())
}

// ---------------------------------------------------------------- records

/// What the tool knows about one bill, from either mode.
struct Record {
    number: String,
    title: String,
    cosponsors: usize,
    recommendations: Vec<String>,
    excerpt_text: Option<String>,
    history: Vec<HistoryAction>,
    mode: String,
    as_of: String,
}

impl Record {
    fn from_snapshot(b: &SnapBill, as_of: &str) -> Self {
        Self {
            number: b.bill.clone(),
            title: b.title.clone(),
            cosponsors: b.cosponsors,
            recommendations: b
                .recommendations
                .iter()
                .map(|(_, a)| a.clone())
                .filter(|a| !a.is_empty())
                .collect(),
            excerpt_text: b.excerpt.clone(),
            history: b
                .history
                .iter()
                .map(|(d, br, a, s)| {
                    omit_member(HistoryAction {
                        date: d.clone(),
                        branch: br.clone(),
                        action: a.clone(),
                        is_stricken: *s,
                    })
                })
                .collect(),
            mode: "snapshot".into(),
            as_of: as_of.into(),
        }
    }
}

fn build_bill(
    rec: &Record,
    c: &LegContent,
    engine: &StageEngine,
    today: NaiveDate,
    general_court: u32,
    with_excerpt: bool,
) -> BillOut {
    let (stage, stage_label) = engine.stage_of(&rec.history);
    let hs = hearings(&rec.history);
    let (latest, upcoming, days_to) = latest_hearing(&hs, today);
    let last = rec.history.iter().rev().find(|a| !a.is_stricken);
    let words = stem_words(&rec.title);
    let topics: Vec<String> = c
        .topics
        .iter()
        .filter(|t| t.keywords.iter().any(|k| contains_phrase(&words, k)))
        .map(|t| t.id.clone())
        .collect();
    let start = hs.len().saturating_sub(3);
    BillOut {
        url: malegislature::bill_url(general_court, &rec.number),
        bill_number: rec.number.clone(),
        title: rec.title.clone(),
        cosponsors: rec.cosponsors,
        topics,
        stage,
        stage_label,
        committee: current_committee(&rec.history),
        last_action: last.map(|a| ActionOut {
            date: a.date.clone(),
            branch: a.branch.clone(),
            text: a.action.clone(),
        }),
        days_since_last_action: last.and_then(|a| days_from(today, &a.date)).map(|n| -n),
        hearings: hs[start..].to_vec(),
        latest_hearing_date: latest,
        hearing_upcoming: upcoming,
        days_until_hearing: days_to,
        reporting_deadline: reporting_deadline(&rec.history),
        committee_recommendations: rec.recommendations.clone(),
        excerpt: if with_excerpt {
            rec.excerpt_text.clone().map(|t| ExcerptOut {
                text: t,
                source: format!(
                    "Text of {} as filed, {}",
                    rec.number,
                    malegislature::bill_url(general_court, &rec.number)
                ),
            })
        } else {
            None
        },
        data_mode: rec.mode.clone(),
        as_of: rec.as_of.clone(),
    }
}

// ---------------------------------------------------------------- inputs

/// What a search asks for after the arguments and the watchlist are merged.
#[derive(Debug, Default)]
struct Wanted {
    topics: Vec<String>,
    keywords: Vec<String>,
    bills: Vec<String>,
}

fn resolve_topic(c: &LegContent, raw: &str) -> Option<String> {
    let norm = phn_core::text::normalize(&raw.replace('_', " "));
    c.topics
        .iter()
        .find(|t| {
            phn_core::text::normalize(&t.id.replace('_', " ")) == norm
                || phn_core::text::normalize(&t.label) == norm
        })
        .map(|t| t.id.clone())
}

fn today_from(arg: &Option<String>) -> Result<NaiveDate> {
    match arg {
        Some(s) => {
            let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| {
                PhnError::InvalidInput(format!("as_of '{s}' is not a date (YYYY-MM-DD)"))
            })?;
            if !(1900..=2100).contains(&d.year()) {
                return Err(PhnError::InvalidInput(
                    "as_of must be a year from 1900 to 2100".into(),
                ));
            }
            Ok(d)
        }
        None => Ok(chrono::Local::now().date_naive()),
    }
}

/// Refuse watchlists that look like lists of people (values are never echoed).
pub(crate) fn people_guard(
    lt: &crate::localdata::LocalTable,
    expected: &str,
    hint: &str,
) -> Result<()> {
    use phn_local::phi::Identifier as I;
    const PERSON_HEADERS: [&str; 14] = [
        "first name",
        "last name",
        "firstname",
        "lastname",
        "client",
        "client name",
        "patient",
        "patient name",
        "participant",
        "recipient",
        "member",
        "dob",
        "date of birth",
        "ssn",
    ];
    let bad_headers: Vec<&String> = lt
        .table
        .headers
        .iter()
        .filter(|h| {
            let n = phn_core::text::normalize(h);
            PERSON_HEADERS
                .iter()
                .any(|p| n == *p || n.starts_with(&format!("{p} ")))
        })
        .collect();
    let mut kinds: BTreeSet<&str> = lt
        .findings
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                I::BirthDate
                    | I::Ssn
                    | I::MedicalRecordNumber
                    | I::HealthPlanNumber
                    | I::AccountNumber
            )
        })
        .map(|f| f.kind.label())
        .collect();
    for row in &lt.table.rows {
        for cell in row {
            for f in phn_local::phi::scan_text(cell) {
                if matches!(
                    f.kind,
                    I::Ssn
                        | I::Phone
                        | I::Email
                        | I::StreetAddress
                        | I::MedicalRecordNumber
                        | I::HealthPlanNumber
                        | I::BirthDate
                ) {
                    kinds.insert(f.kind.label());
                }
            }
        }
    }
    if bad_headers.is_empty() && kinds.is_empty() {
        return Ok(());
    }
    Err(PhnError::Privacy(format!(
        "'{}' looks like a list of people or contains identifiers (columns {}{}), not {expected}, \
         so it was not read. {hint} Keep client lists out of this tool.",
        lt.label,
        bad_headers
            .iter()
            .map(|h| format!("'{h}'"))
            .collect::<Vec<_>>()
            .join(", "),
        if kinds.is_empty() {
            String::new()
        } else {
            format!(
                "; detected {}",
                kinds.into_iter().collect::<Vec<_>>().join(", ")
            )
        }
    )))
}

fn read_watchlist(
    ctx: &Context,
    path: &str,
    c: &LegContent,
    wanted: &mut Wanted,
) -> Result<WatchlistOut> {
    let lt = crate::localdata::load(ctx, path, None)?;
    people_guard(
        &lt,
        "a topic watchlist",
        "A watchlist has columns kind (topic, keyword, or bill), value, and note.",
    )?;
    let (Some(ki), Some(vi)) = (
        lt.table.column_index("kind"),
        lt.table.column_index("value"),
    ) else {
        return Err(PhnError::InvalidInput(format!(
            "'{}' needs columns named kind and value (and optionally note). Columns found: {}",
            lt.label,
            lt.table.headers.join(", ")
        )));
    };
    let mut out = WatchlistOut {
        read_from: Some(lt.label.clone()),
        topics: Vec::new(),
        keywords: Vec::new(),
        bills: Vec::new(),
        unrecognized: Vec::new(),
        saved_file: None,
    };
    for row in &lt.table.rows {
        let kind = row
            .get(ki)
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default();
        let value = row
            .get(vi)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if value.is_empty() {
            continue;
        }
        match kind.as_str() {
            "topic" => match resolve_topic(c, &value) {
                Some(id) => {
                    out.topics.push(id.clone());
                    wanted.topics.push(id);
                }
                None => out.unrecognized.push(format!("topic '{}'", short(&value))),
            },
            "keyword" => {
                if value.chars().count() <= 60 {
                    out.keywords.push(value.clone());
                    wanted.keywords.push(value);
                } else {
                    out.unrecognized.push("a keyword that is too long".into());
                }
            }
            "bill" => match malegislature::normalize_bill_number(&value) {
                Some(n) => {
                    out.bills.push(n.clone());
                    wanted.bills.push(n);
                }
                None => out.unrecognized.push(format!("bill '{}'", short(&value))),
            },
            _ => out
                .unrecognized
                .push(format!("a row of kind '{}'", short(&kind))),
        }
    }
    Ok(out)
}

fn short(s: &str) -> String {
    s.chars().take(40).collect()
}

fn dedup(v: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    v.retain(|x| seen.insert(x.clone()));
}

// ---------------------------------------------------------------- the tool

fn load_snapshot(ctx: &Context) -> Result<(SnapFile, phn_data::snapshots::SnapshotEntry)> {
    let (text, entry, _origin) = ctx.snapshots.file_text(SNAPSHOT_ID, SNAPSHOT_FILE)?;
    let f: SnapFile = serde_json::from_str(&text)
        .map_err(|e| PhnError::Internal(format!("{SNAPSHOT_FILE}: {e}")))?;
    Ok((f, entry))
}

/// Does a title match any wanted topic keyword or user keyword?
/// Returns the number of distinct topics and keywords matched.
fn title_score(title_words: &[String], c: &LegContent, wanted: &Wanted) -> usize {
    let mut n = 0;
    for t in &c.topics {
        if wanted.topics.contains(&t.id)
            && t.keywords.iter().any(|k| contains_phrase(title_words, k))
        {
            n += 1;
        }
    }
    for k in &wanted.keywords {
        if contains_phrase(title_words, k) {
            n += 1;
        }
    }
    n
}

fn topic_counts<'a>(
    titles: impl Iterator<Item = &'a str>,
    c: &LegContent,
    wanted: &Wanted,
) -> Vec<TopicOut> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for title in titles {
        let w = stem_words(title);
        for t in &c.topics {
            if t.keywords.iter().any(|k| contains_phrase(&w, k)) {
                *counts.entry(t.id.as_str()).or_default() += 1;
            }
        }
    }
    c.topics
        .iter()
        .map(|t| TopicOut {
            id: t.id.clone(),
            label: t.label.clone(),
            bills_matched: counts.get(t.id.as_str()).copied().unwrap_or(0),
            requested: wanted.topics.contains(&t.id),
        })
        .collect()
}

/// A selected bill from the list (live) or the snapshot.
struct Candidate {
    number: String,
    score: usize,
}

/// Run the tool.
pub async fn track_ma_legislation(ctx: &Context, args: TrackArgs) -> Result<Envelope<TrackResult>> {
    track_ma_legislation_at(ctx, args, malegislature::BASE).await
}

/// [`track_ma_legislation`] against another API base URL (tests).
pub async fn track_ma_legislation_at(
    ctx: &Context,
    args: TrackArgs,
    base: &str,
) -> Result<Envelope<TrackResult>> {
    let c: LegContent = content::load(CONTENT)?;
    let engine = StageEngine::new(&c)?;
    let today = today_from(&args.as_of)?;
    let gc = args.general_court.unwrap_or(c.general_court.number);
    if !(189..=200).contains(&gc) {
        return Err(PhnError::InvalidInput(
            "general_court must be from 189 (2015-2016) to 200".into(),
        ));
    }
    let max = args
        .max_bills
        .map(|m| (m as usize).clamp(1, MAX_BILLS))
        .unwrap_or(DEFAULT_BILLS);
    let with_excerpt = args.include_excerpts.unwrap_or(true);

    // Arguments.
    let mut wanted = Wanted::default();
    for t in &args.topics {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        match resolve_topic(&c, t) {
            Some(id) => wanted.topics.push(id),
            None => {
                return Err(PhnError::InvalidInput(format!(
                    "'{}' is not a topic. Topics: {}",
                    short(t),
                    c.topics
                        .iter()
                        .map(|t| t.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
    }
    if args.keywords.len() > MAX_KEYWORDS {
        return Err(PhnError::InvalidInput(format!(
            "give at most {MAX_KEYWORDS} keywords"
        )));
    }
    for k in &args.keywords {
        let k = clean_text("keyword", k, 60)?;
        if k.is_empty() {
            continue;
        }
        refuse_identifiers("keyword", &k)?;
        wanted.keywords.push(k);
    }
    for b in &args.bill_numbers {
        match malegislature::normalize_bill_number(b) {
            Some(n) => wanted.bills.push(n),
            None => {
                return Err(PhnError::InvalidInput(format!(
                    "'{}' is not a bill number (for example H3444 or S2518)",
                    short(b)
                )));
            }
        }
    }
    let stage_ids = engine.stage_ids();
    let mut stages: Vec<String> = Vec::new();
    for s in &args.stages {
        let s = s.trim().to_lowercase().replace([' ', '-'], "_");
        if !stage_ids.contains(&s) {
            return Err(PhnError::InvalidInput(format!(
                "'{}' is not a stage. Stages: {}",
                short(&s),
                stage_ids.join(", ")
            )));
        }
        stages.push(s);
    }
    let only_hearings = args.only_upcoming_hearings.unwrap_or(false);

    let mut watchlist = match &args.watchlist_path {
        Some(p) if !p.trim().is_empty() => Some(read_watchlist(ctx, p.trim(), &c, &mut wanted)?),
        _ => None,
    };
    dedup(&mut wanted.topics);
    dedup(&mut wanted.keywords);
    dedup(&mut wanted.bills);

    let asked =
        !(wanted.topics.is_empty() && wanted.keywords.is_empty() && wanted.bills.is_empty());
    let mut excerpt_keywords: Vec<String> = c
        .topics
        .iter()
        .filter(|t| wanted.topics.contains(&t.id))
        .flat_map(|t| t.keywords.clone())
        .collect();
    excerpt_keywords.extend(wanted.keywords.clone());

    // ---- Nothing asked: the topic index, with counts from the snapshot (no network call).
    if !asked {
        return topic_index(ctx, &c, &wanted, gc, today, watchlist);
    }

    // ---- Live attempt.
    let mut mode = "snapshot".to_string();
    let mut as_of = String::new();
    let mut mode_note = String::new();
    let mut records: Vec<Record> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();
    let mut title_matches = 0usize;
    let mut topics_out: Vec<TopicOut> = Vec::new();
    let mut snapshot_entry = None;
    let mut live_ok = false;
    let mut failure = String::new();

    let listing = malegislature::fetch_listing(&ctx.http, ctx.cache.as_ref(), base, gc).await;
    match listing {
        Ok(l) => {
            live_ok = true;
            mode = l.mode.label().to_string();
            as_of = phn_data::fetch::local_date(&l.as_of);
            mode_note = match &l.fallback_reason {
                Some(r) => {
                    format!("{r}. The list was last read on {as_of}; bill details are live.")
                }
                None if l.mode == SourceMode::Cache => format!(
                    "The bill list is a cached copy read on {as_of} (refreshed daily); bill details are live."
                ),
                None => format!("Live from the Massachusetts Legislature's public API on {as_of}."),
            };
            topics_out = topic_counts(l.bills.iter().map(|b| b.title.as_str()), &c, &wanted);
            let mut cands: Vec<Candidate> = Vec::new();
            if !wanted.topics.is_empty() || !wanted.keywords.is_empty() {
                for b in &l.bills {
                    let w = stem_words(&b.title);
                    let s = title_score(&w, &c, &wanted);
                    if s > 0 {
                        cands.push(Candidate {
                            number: b.bill_number.clone(),
                            score: s,
                        });
                    }
                }
            }
            title_matches = cands.len();
            cands.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| bill_sort_key(&a.number).cmp(&bill_sort_key(&b.number)))
            });
            // Explicit bills first, then title matches, up to the limit.
            let mut order: Vec<String> = wanted.bills.clone();
            for cnd in &cands {
                if !order.contains(&cnd.number) {
                    order.push(cnd.number.clone());
                }
            }
            order.truncate(wanted.bills.len() + max);
            let by_number: BTreeMap<&str, &BillListing> = l
                .bills
                .iter()
                .map(|b| (b.bill_number.as_str(), b))
                .collect();
            let snap = load_snapshot(ctx).ok();
            let fetched = fetch_details(ctx, base, gc, &order).await;
            for (n, res) in order.iter().zip(fetched) {
                match res {
                    Ok((d, h)) => {
                        let ex = d.text.as_str();
                        let kws = excerpt_keywords.clone();
                        records.push(Record {
                            number: d.bill_number.clone(),
                            title: d.title.clone(),
                            cosponsors: by_number.get(n.as_str()).map_or(0, |b| b.cosponsor_count),
                            recommendations: d.committee_actions.clone(),
                            excerpt_text: if with_excerpt {
                                excerpt_around(ex, &kws)
                            } else {
                                None
                            },
                            history: h.into_iter().map(omit_member).collect(),
                            mode: "live".into(),
                            as_of: as_of.clone(),
                        });
                    }
                    Err(PhnError::NotFound(_)) => not_found.push(n.clone()),
                    Err(_) => {
                        // This bill's details failed: use the snapshot copy when there is one.
                        if let Some((sf, _)) = &snap
                            && gc == sf.general_court
                            && let Some(sb) = sf.bills.iter().find(|b| &b.bill == n)
                        {
                            records.push(Record::from_snapshot(sb, &sf.as_of));
                        } else if let Some(b) = by_number.get(n.as_str()) {
                            records.push(Record {
                                number: b.bill_number.clone(),
                                title: b.title.clone(),
                                cosponsors: b.cosponsor_count,
                                recommendations: Vec::new(),
                                excerpt_text: None,
                                history: Vec::new(),
                                mode: "live (list only)".into(),
                                as_of: as_of.clone(),
                            });
                        }
                    }
                }
            }
        }
        Err(e) => failure = e.to_string(),
    }

    // ---- Snapshot fallback.
    if !live_ok {
        let (sf, entry) = load_snapshot(ctx).map_err(|e| {
            PhnError::Network(format!(
                "the Legislature's API could not be reached ({failure}) and the bundled snapshot could not be read ({e})"
            ))
        })?;
        if gc != sf.general_court {
            return Err(PhnError::Network(format!(
                "the Legislature's API could not be reached ({failure}); the offline snapshot covers only the {}th General Court, not the {gc}th",
                sf.general_court
            )));
        }
        as_of = sf.as_of.clone();
        mode = "snapshot".into();
        mode_note = format!(
            "The Legislature's API could not be reached ({}), so this is the bundled snapshot of {}. Status, hearings, and last actions may be out of date; check the bill pages.",
            failure.trim_end_matches('.'),
            sf.as_of
        );
        topics_out = topic_counts(sf.bills.iter().map(|b| b.title.as_str()), &c, &wanted);
        let mut cands: Vec<(usize, &SnapBill)> = Vec::new();
        for b in &sf.bills {
            let w = stem_words(&b.title);
            let s = title_score(&w, &c, &wanted);
            if s > 0 || wanted.bills.contains(&b.bill) {
                cands.push((s, b));
            }
        }
        title_matches = cands.iter().filter(|(s, _)| *s > 0).count();
        cands.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| bill_sort_key(&a.1.bill).cmp(&bill_sort_key(&b.1.bill)))
        });
        // Explicit bills first.
        for n in &wanted.bills {
            match sf.bills.iter().find(|b| &b.bill == n) {
                Some(b) => records.push(Record::from_snapshot(b, &sf.as_of)),
                None => not_found.push(n.clone()),
            }
        }
        for (_, b) in cands {
            if records.len() >= wanted.bills.len() + max {
                break;
            }
            if !wanted.bills.contains(&b.bill) {
                records.push(Record::from_snapshot(b, &sf.as_of));
            }
        }
        if !not_found.is_empty() {
            mode_note.push_str(
                " Bills not found may simply not be in the snapshot, which holds only bills whose titles match the topic lists.",
            );
        }
        snapshot_entry = Some(entry);
    }

    // ---- Build, filter, limit.
    let examined = records.len();
    let mut bills: Vec<BillOut> = records
        .iter()
        .map(|r| build_bill(r, &c, &engine, today, gc, with_excerpt))
        .collect();
    if !stages.is_empty() {
        bills.retain(|b| stages.contains(&b.stage));
    }
    if only_hearings {
        bills.retain(|b| b.hearing_upcoming);
    }
    // Upcoming hearings first, then most recently acted on.
    bills.sort_by(|a, b| {
        b.hearing_upcoming
            .cmp(&a.hearing_upcoming)
            .then_with(|| {
                a.latest_hearing_date
                    .as_ref()
                    .filter(|_| a.hearing_upcoming)
                    .cmp(
                        &b.latest_hearing_date
                            .as_ref()
                            .filter(|_| b.hearing_upcoming),
                    )
            })
            .then_with(|| {
                a.days_since_last_action
                    .unwrap_or(i64::MAX)
                    .cmp(&b.days_since_last_action.unwrap_or(i64::MAX))
            })
    });

    // ---- Watchlist file.
    let want_save = args.save_watchlist.unwrap_or(false);
    let mut saved: Option<String> = None;
    if want_save {
        saved = Some(save_watchlist(ctx, &wanted, &bills, today)?);
    }
    if let Some(w) = watchlist.as_mut() {
        w.saved_file = saved.clone();
    } else if saved.is_some() {
        watchlist = Some(WatchlistOut {
            read_from: None,
            topics: wanted.topics.clone(),
            keywords: wanted.keywords.clone(),
            bills: wanted.bills.clone(),
            unrecognized: Vec::new(),
            saved_file: saved.clone(),
        });
    }

    let questions: Vec<String> = c
        .analysis_questions
        .iter()
        .map(|q| q.question.clone())
        .collect();
    let data = TrackResult {
        general_court: gc,
        years: if gc == c.general_court.number {
            c.general_court.years.clone()
        } else {
            format!("{}th General Court", gc)
        },
        data_mode: mode.clone(),
        as_of: as_of.clone(),
        mode_note: mode_note.clone(),
        today: today.to_string(),
        topics: topics_out,
        keywords: wanted.keywords.clone(),
        title_matches,
        examined,
        bills,
        not_found: not_found.clone(),
        watchlist,
        analysis_questions: questions,
        how_to_search: c.how_to_search.clone(),
        limits: c.limits.clone(),
    };

    let summary = summary_text(&data, &c, asked);
    let mut sources: Vec<Source> = Vec::new();
    if live_ok {
        sources.push(Source {
            title: format!("Massachusetts Legislature public API, {gc}th General Court (bill list, bill records, and histories)"),
            publisher: "Massachusetts Legislature".into(),
            url: Some(format!("{base}/GeneralCourts/{gc}/Documents")),
            effective_date: None,
            retrieved: Some(as_of.clone()),
            mode: if mode == "cache" { SourceMode::Cache } else { SourceMode::Live },
        });
    }
    if let Some(e) = &snapshot_entry {
        sources.push(e.to_source());
    }
    sources.push(c.sources["bill_search"].to_source());

    let mut caveats = vec![
        "This tool compiles what the Legislature publishes. It does not say what a bill would do or what it would mean for local health; the excerpts are short quotations of 25 words or fewer from the bill as filed, and the questions are for your own analysis. Read the bill. Not legal advice.".to_string(),
        "Bills are found by words in their TITLES only; a bill on your topic whose title uses other words will not appear. Add keywords, or search https://malegislature.gov/Bills/Search.".to_string(),
        "The stage label is this project's reading of the bill's history (rules in the content file). The last action and the hearing lines are the Legislature's own wording; trust those over the label.".to_string(),
    ];
    if mode == "snapshot" {
        caveats.push(format!(
            "SNAPSHOT DATA: status as of {as_of}, not current. {}",
            if live_ok {
                "A bill's live record could not be read."
            } else {
                "The API could not be reached."
            }
        ));
    }
    if examined > 0 && (!stages.is_empty() || only_hearings) {
        caveats.push(format!(
            "Stage and hearing filters were applied to the {examined} bills examined, out of {title_matches} title matches; raise max_bills to examine more."
        ));
    }
    if title_matches > max {
        caveats.push(format!(
            "{title_matches} bills matched by title; the first {max} (strongest match first) were examined. Narrow the topics or add keywords."
        ));
    }
    caveats.extend(content::review_caveats(&[CONTENT]));

    let mut privacy = Privacy {
        row_level_data: false,
        notes: vec![
            "Requests to the Legislature carry only a General Court number and bill numbers; no personal data is sent."
                .into(),
        ],
        ..Privacy::none()
    };
    if let Some(w) = &data.watchlist {
        if let Some(f) = &w.saved_file {
            privacy.files_written.push(f.clone());
        }
        if let Some(r) = &w.read_from {
            privacy
                .notes
                .push(format!("Read your watchlist {r}; only its topics, keywords, and bill numbers were used, and no other value is returned."));
        }
    }

    let mut env = Envelope::new(summary, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy);
    if let Some(r) = &env.data.watchlist
        && let Some(f) = &r.read_from
    {
        env.sources.push(Source::local_file(f.clone()));
    }
    Ok(env.decision_support())
}

/// The topic index: topics with the number of snapshot bills whose titles match each.
fn topic_index(
    ctx: &Context,
    c: &LegContent,
    wanted: &Wanted,
    gc: u32,
    today: NaiveDate,
    watchlist: Option<WatchlistOut>,
) -> Result<Envelope<TrackResult>> {
    let (sf, entry) = load_snapshot(ctx)?;
    let topics = topic_counts(sf.bills.iter().map(|b| b.title.as_str()), c, wanted);
    let data = TrackResult {
        general_court: gc,
        years: c.general_court.years.clone(),
        data_mode: "snapshot".into(),
        as_of: sf.as_of.clone(),
        mode_note: format!(
            "Topic index only: the counts come from the bundled snapshot of {} (no request was made). Ask about a topic to get live bills.",
            sf.as_of
        ),
        today: today.to_string(),
        topics,
        keywords: Vec::new(),
        title_matches: 0,
        examined: 0,
        bills: Vec::new(),
        not_found: Vec::new(),
        watchlist,
        analysis_questions: c
            .analysis_questions
            .iter()
            .map(|q| q.question.clone())
            .collect(),
        how_to_search: c.how_to_search.clone(),
        limits: c.limits.clone(),
    };
    let summary = summary_text(&data, c, false);
    let mut caveats = vec![
        "Bills are found by words in their TITLES only; add keywords to find others.".to_string(),
        format!(
            "Counts are from the snapshot of {} and change daily.",
            sf.as_of
        ),
    ];
    caveats.extend(content::review_caveats(&[CONTENT]));
    Ok(Envelope::new(summary, data)
        .source(entry.to_source())
        .source(c.sources["bill_search"].to_source())
        .caveats(caveats)
        .privacy(Privacy {
            notes: vec!["No request was made and no files were read or written.".into()],
            ..Privacy::none()
        })
        .decision_support())
}

fn bill_sort_key(n: &str) -> (char, u32) {
    (
        n.chars().next().unwrap_or('Z'),
        n.get(1..).and_then(|d| d.parse().ok()).unwrap_or(0),
    )
}

type Fetched = Result<(malegislature::BillDetail, Vec<HistoryAction>)>;

/// Fetch details for the bills in order, a few at a time.
async fn fetch_details(ctx: &Context, base: &str, gc: u32, numbers: &[String]) -> Vec<Fetched> {
    let mut out: Vec<Fetched> = Vec::with_capacity(numbers.len());
    for chunk in numbers.chunks(DETAIL_BATCH) {
        let mut set = tokio::task::JoinSet::new();
        for (i, n) in chunk.iter().enumerate() {
            let http = ctx.http.clone();
            let n = n.clone();
            let base = base.to_string();
            set.spawn(async move { (i, malegislature::fetch_bill(&http, &base, gc, &n).await) });
        }
        let mut part: Vec<Option<Fetched>> = (0..chunk.len()).map(|_| None).collect();
        while let Some(j) = set.join_next().await {
            match j {
                Ok((i, r)) => part[i] = Some(r),
                Err(e) => tracing::warn!(error = %e, "bill fetch task failed"),
            }
        }
        out.extend(part.into_iter().map(|p| {
            p.unwrap_or_else(|| Err(PhnError::Network("the request did not finish".into())))
        }));
    }
    out
}

fn save_watchlist(
    ctx: &Context,
    wanted: &Wanted,
    bills: &[BillOut],
    today: NaiveDate,
) -> Result<String> {
    let sandbox = ctx.sandbox()?;
    let path = sandbox.new_output_file("ma_legislation_watchlist.csv")?;
    let mut w = csv::Writer::from_writer(Vec::new());
    let header = [
        "kind",
        "value",
        "note",
        "title",
        "stage",
        "last_action_date",
        "last_action",
        "committee",
        "latest_hearing_date",
        "url",
        "checked_on",
    ];
    w.write_record(header)
        .map_err(|e| PhnError::Internal(e.to_string()))?;
    let checked = today.to_string();
    let blank = |n: usize| vec![String::new(); n];
    for t in &wanted.topics {
        let mut r = vec!["topic".to_string(), t.clone()];
        r.extend(blank(9));
        r[10] = checked.clone();
        w.write_record(&r)
            .map_err(|e| PhnError::Internal(e.to_string()))?;
    }
    for k in &wanted.keywords {
        let mut r = vec!["keyword".to_string(), k.clone()];
        r.extend(blank(9));
        r[10] = checked.clone();
        w.write_record(&r)
            .map_err(|e| PhnError::Internal(e.to_string()))?;
    }
    let mut listed: BTreeSet<&str> = BTreeSet::new();
    for b in bills {
        if !listed.insert(b.bill_number.as_str()) {
            continue;
        }
        let la = b.last_action.as_ref();
        let r = vec![
            "bill".to_string(),
            b.bill_number.clone(),
            String::new(),
            b.title.clone(),
            b.stage.clone(),
            la.map(|a| a.date.clone()).unwrap_or_default(),
            la.map(|a| a.text.clone()).unwrap_or_default(),
            b.committee.clone().unwrap_or_default(),
            b.latest_hearing_date.clone().unwrap_or_default(),
            b.url.clone(),
            checked.clone(),
        ];
        w.write_record(&r)
            .map_err(|e| PhnError::Internal(e.to_string()))?;
    }
    for n in &wanted.bills {
        if !listed.contains(n.as_str()) {
            let mut r = vec!["bill".to_string(), n.clone()];
            r.extend(blank(9));
            r[10] = checked.clone();
            w.write_record(&r)
                .map_err(|e| PhnError::Internal(e.to_string()))?;
        }
    }
    let bytes = w
        .into_inner()
        .map_err(|e| PhnError::Internal(e.to_string()))?;
    std::fs::write(&path, bytes)?;
    Ok(sandbox.display_path(&path))
}

fn summary_text(d: &TrackResult, c: &LegContent, asked: bool) -> String {
    let mut s = format!(
        "**Massachusetts bills, {} General Court ({})**\n\n",
        d.general_court, d.years
    );
    s.push_str(&format!(
        "Data: **{}**, read {}. {}\n\n",
        d.data_mode, d.as_of, d.mode_note
    ));
    s.push_str(
        "This is a compilation of the Legislature's public record for your own analysis. It does not say what a bill would do.\n\n",
    );
    if !asked {
        s.push_str("Pick topics (or give keywords, bill numbers, or a watchlist):\n\n");
        let rows: Vec<Vec<String>> = d
            .topics
            .iter()
            .map(|t| {
                vec![
                    format!("`{}`", t.id),
                    t.label.clone(),
                    t.bills_matched.to_string(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Topic id", "Topic", "Bills with matching titles"],
            &rows,
        ));
        s.push('\n');
        return s;
    }
    s.push_str(&format!(
        "{} bill(s) matched by title; {} examined; {} shown (today is {}).\n\n",
        d.title_matches,
        d.examined,
        d.bills.len(),
        d.today
    ));
    if !d.not_found.is_empty() {
        s.push_str(&format!("Not found: {}.\n\n", d.not_found.join(", ")));
    }
    if d.bills.is_empty() {
        s.push_str("No bills to show with these settings. Try other topics or keywords, or remove the stage and hearing filters.\n\n");
    }
    for b in &d.bills {
        s.push_str(&format!(
            "### [{}]({}): {}\n\n",
            b.bill_number,
            b.url,
            b.title.replace('\n', " ")
        ));
        let mut line = format!("- **Stage:** {}", b.stage_label);
        if let Some(cm) = &b.committee {
            line.push_str(&format!("; committee: {cm}"));
        }
        s.push_str(&line);
        s.push('\n');
        if let Some(a) = &b.last_action {
            s.push_str(&format!(
                "- **Last action** ({}, {}{}): {}\n",
                a.date,
                if a.branch.is_empty() {
                    "branch not stated"
                } else {
                    a.branch.as_str()
                },
                b.days_since_last_action
                    .map(|n| format!(", {n} days ago"))
                    .unwrap_or_default(),
                a.text
            ));
        } else {
            s.push_str("- **Last action:** none recorded\n");
        }
        if let Some(h) = &b.latest_hearing_date {
            s.push_str(&format!(
                "- **Hearing:** {h}{}\n",
                match b.days_until_hearing {
                    Some(n) if n > 0 => format!(" (in {n} days)"),
                    Some(0) => " (today)".into(),
                    Some(n) => format!(" ({} days ago)", -n),
                    None => String::new(),
                }
            ));
        }
        if let Some(r) = &b.reporting_deadline {
            s.push_str(&format!("- **Committee reporting deadline:** {r}\n"));
        }
        s.push_str(&format!(
            "- Cosponsors: {} (sponsors are named on the bill page)\n",
            b.cosponsors
        ));
        if let Some(e) = &b.excerpt {
            s.push_str(&format!(
                "- Excerpt of the bill text: \"{}\" _({})_\n",
                e.text, e.source
            ));
        }
        s.push('\n');
    }
    s.push_str("**Questions to put to any bill** (your analysis):\n\n");
    for q in c.analysis_questions.iter().take(9) {
        s.push_str(&format!("- {}\n", q.question));
    }
    s.push('\n');
    if let Some(w) = &d.watchlist
        && let Some(f) = &w.saved_file
    {
        s.push_str(&format!(
            "Watchlist saved to `{f}` in your output folder.\n"
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(date: &str, branch: &str, action: &str) -> HistoryAction {
        HistoryAction {
            date: date.into(),
            branch: branch.into(),
            action: action.into(),
            is_stricken: false,
        }
    }

    fn content() -> LegContent {
        content::load(CONTENT).unwrap()
    }

    #[test]
    fn content_loads_and_every_stage_pattern_compiles() {
        let c = content();
        let e = StageEngine::new(&c).unwrap();
        assert_eq!(c.general_court.number, 194);
        assert!(e.stage_ids().contains(&"enacted".to_string()));
        assert!(c.topics.len() >= 15);
    }

    #[test]
    fn stages_follow_the_latest_matching_action() {
        let c = content();
        let e = StageEngine::new(&c).unwrap();
        let st = |v: Vec<HistoryAction>| e.stage_of(&v).0;
        assert_eq!(st(vec![]), "filed");
        let mut hist = vec![
            h(
                "2025-02-27",
                "House",
                "Referred to the committee on Public Health",
            ),
            h("2025-02-27", "Senate", "Senate concurred"),
            h(
                "2025-05-06",
                "Joint",
                "Hearing scheduled for 05/06/2025 from 11:00 AM-01:00 PM in A-2",
            ),
        ];
        assert_eq!(st(hist.clone()), "in_committee");
        hist.push(h("2026-03-01", "House", "Bill reported favorably by committee and referred to the committee on House Ways and Means"));
        assert_eq!(st(hist.clone()), "reported_out");
        hist.push(h(
            "2026-04-01",
            "House",
            "Read second and ordered to a third reading",
        ));
        assert_eq!(st(hist.clone()), "floor");
        hist.push(h(
            "2026-04-02",
            "House",
            "Amendment 12 (A. Member) rejected",
        ));
        assert_eq!(
            st(hist.clone()),
            "floor",
            "amendments do not change the stage"
        );
        hist.push(h(
            "2026-07-30",
            "Senate",
            "Enacted and laid before the Governor",
        ));
        assert_eq!(st(hist.clone()), "awaiting_governor");
        hist.push(h(
            "2026-08-05",
            "Executive",
            "Signed by the Governor, Chapter 120 of the Acts of 2026",
        ));
        assert_eq!(st(hist.clone()), "enacted");
        assert_eq!(
            st(vec![
                h(
                    "2025-02-27",
                    "House",
                    "Referred to the committee on Public Health"
                ),
                h(
                    "2026-04-06",
                    "House",
                    "Accompanied a study order, see H5323"
                ),
            ]),
            "study_or_new_draft"
        );
        // A stricken action is ignored.
        let mut stricken = h(
            "2026-04-06",
            "House",
            "Signed by the Governor, Chapter 1 of the Acts of 2026",
        );
        stricken.is_stricken = true;
        assert_eq!(
            st(vec![
                h(
                    "2025-02-27",
                    "House",
                    "Referred to the committee on Public Health"
                ),
                stricken,
            ]),
            "in_committee"
        );
    }

    #[test]
    fn hearings_dates_and_countdown() {
        let hist = vec![
            h(
                "2025-04-30",
                "Joint",
                "Hearing scheduled for 05/06/2025 from 11:00 AM-01:00 PM in A-2",
            ),
            h(
                "2025-05-02",
                "Joint",
                "Hearing rescheduled to 5/20/2025 from 1:00 PM-3:00 PM in B-1",
            ),
            h(
                "2025-05-03",
                "Joint",
                "Hearing scheduled for 13/45/2025 in A-1",
            ),
        ];
        let hs = hearings(&hist);
        assert_eq!(hs.len(), 3);
        assert_eq!(hs[0].hearing_date.as_deref(), Some("2025-05-06"));
        assert_eq!(hs[1].kind, "rescheduled");
        assert_eq!(hs[1].hearing_date.as_deref(), Some("2025-05-20"));
        assert_eq!(hs[2].hearing_date, None, "an impossible date is not read");
        let today = NaiveDate::from_ymd_opt(2025, 5, 10).unwrap();
        let (d, up, days) = latest_hearing(&hs[..2], today);
        assert_eq!(
            (d.as_deref(), up, days),
            (Some("2025-05-20"), true, Some(10))
        );
        let (_, up, days) = latest_hearing(&hs[..1], today);
        assert_eq!((up, days), (false, Some(-4)));
        let mut cancelled = hs[..2].to_vec();
        cancelled.push(HearingOut {
            recorded: "2025-05-12".into(),
            kind: "cancelled".into(),
            hearing_date: None,
            text: "Hearing cancelled".into(),
        });
        assert_eq!(latest_hearing(&cancelled, today), (None, false, None));
        assert_eq!(us_date("on 02/30/2025"), None);
        assert_eq!(us_date("on 2/3/0025"), None);
    }

    #[test]
    fn committee_and_reporting_deadline() {
        let hist = vec![
            h(
                "2025-02-27",
                "House",
                "Referred to the committee on Children, Families and Persons with Disabilities",
            ),
            h(
                "2025-08-04",
                "House",
                "Reporting date extended to Wednesday, December 3, 2025",
            ),
            h(
                "2025-12-04",
                "House",
                "Reporting date extended to Wednesday, March 18, 2026",
            ),
        ];
        assert_eq!(
            current_committee(&hist).as_deref(),
            Some("Children, Families and Persons with Disabilities")
        );
        assert_eq!(reporting_deadline(&hist).as_deref(), Some("2026-03-18"));
        let hist = vec![
            h(
                "2025-02-27",
                "House",
                "Referred to the committee on Public Health",
            ),
            h(
                "2026-03-01",
                "House",
                "Reported, referred to the committee on Joint Rules, reported, rules suspended and referred to the committee on House Ways and Means (under JR12)",
            ),
        ];
        assert_eq!(
            current_committee(&hist).as_deref(),
            Some("House Ways and Means")
        );
        assert_eq!(
            reporting_deadline(&[h(
                "2025-08-04",
                "House",
                "Reporting date extended until Friday, July 31, 2026"
            )])
            .as_deref(),
            Some("2026-07-31")
        );
        // A weekday that does not match the date is not read.
        assert_eq!(
            reporting_deadline(&[h(
                "2025-08-04",
                "House",
                "Reporting date extended to Monday, December 3, 2025"
            )]),
            None
        );
    }

    #[test]
    fn excerpts_are_short_and_centered_on_the_keyword() {
        let text = (1..=60)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ")
            + " the vaccine schedule shall be reviewed by the department each year and more words follow here to pass the limit of words";
        let e = excerpt_around(&text, &["vaccine".into()]).unwrap();
        assert!(excerpt_words(&e) <= MAX_WORDS, "{e}");
        assert!(e.contains("vaccine") && e.starts_with("… ") && e.ends_with(" …"));
        let short = excerpt_around(
            "Section 1. Vaccines shall be available.",
            &["vaccine".into()],
        )
        .unwrap();
        assert_eq!(short, "Section 1. Vaccines shall be available.");
        let first = excerpt_around(&text, &[]).unwrap();
        assert!(first.starts_with("w1 ") && excerpt_words(&first) == MAX_WORDS);
        assert_eq!(excerpt_around("   ", &["x".into()]), None);
        // Non-ASCII text is safe.
        assert!(excerpt_around("Prescripción de vacunas niñas ñandú", &["niñas".into()]).is_some());
    }

    #[test]
    fn topic_names_resolve_and_the_snapshot_agrees_with_the_content() {
        let c = content();
        assert_eq!(
            resolve_topic(&c, "Vaccines and immunization").as_deref(),
            Some("vaccines")
        );
        assert_eq!(
            resolve_topic(&c, "mental health").as_deref(),
            Some("mental_health")
        );
        assert_eq!(
            resolve_topic(&c, "mental_health").as_deref(),
            Some("mental_health")
        );
        assert_eq!(resolve_topic(&c, "nonsense"), None);
    }

    /// The bundled snapshot agrees with the content (same topic lists), every bill's topics follow
    /// from its title by the tool's own matching, excerpts are short, histories are dated sensibly,
    /// and no member names are kept.
    #[test]
    fn the_snapshot_agrees_with_the_content_and_the_tool_matching() {
        let c = content();
        let ctx = crate::context::Context::new(phn_core::Config::for_root(
            tempfile::tempdir().unwrap().path(),
        ))
        .unwrap();
        let (sf, entry) = load_snapshot(&ctx).unwrap();
        assert_eq!(sf.general_court, 194);
        assert_eq!(entry.retrieved, sf.as_of);
        assert_eq!(
            sf.topic_definitions, c.topics,
            "builder TOPICS and the content differ"
        );
        let engine = StageEngine::new(&c).unwrap();
        let named = Regex::new(r"^Amendment\s+#?\d+[A-Za-z]?\s*\(").unwrap();
        let mut seen = BTreeSet::new();
        let mut staged: BTreeMap<String, usize> = BTreeMap::new();
        for b in &sf.bills {
            assert!(seen.insert(b.bill.clone()), "duplicate {}", b.bill);
            let w = stem_words(&b.title);
            let expect: Vec<String> = c
                .topics
                .iter()
                .filter(|t| t.keywords.iter().any(|k| contains_phrase(&w, k)))
                .map(|t| t.id.clone())
                .collect();
            assert_eq!(b.topics, expect, "{}", b.bill);
            if let Some(e) = &b.excerpt {
                assert!(excerpt_words(e) <= MAX_WORDS, "{}", b.bill);
            }
            for (d, _, a, _) in &b.history {
                assert!(
                    d.as_str() >= "2025-01-01" && d.as_str() <= "2027-01-31",
                    "{} {d}",
                    b.bill
                );
                assert!(!named.is_match(a) || a.contains("(member omitted)"), "{a}");
            }
            let rec = Record::from_snapshot(b, &sf.as_of);
            *staged.entry(engine.stage_of(&rec.history).0).or_default() += 1;
        }
        assert!(sf.bills.len() > 500);
        assert!(
            staged.get("in_committee").copied().unwrap_or(0) > 100,
            "{staged:?}"
        );
        assert!(
            staged.contains_key("enacted") || staged.contains_key("study_or_new_draft"),
            "{staged:?}"
        );
    }
}
