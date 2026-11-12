//! PLAN-07 `analyze_funding_announcement`: reads a notice of funding opportunity (NOFO) or a
//! Massachusetts request for response (RFR) that the nurse saved in /data (PDF, DOCX, HTML, TXT, or
//! Markdown) and turns it into a compliance checklist: eligibility, deadlines, page limits,
//! required attachments, scoring criteria with points, and a response outline.
//!
//! The reader is a set of pattern rules (word lists in `content/planning/announcement_patterns.yaml`
//! and regular expressions below), not a language model. It returns structure and counts and short
//! labels only; excerpts of the announcement go into the DOCX and XLSX written to /data/output, and
//! the response says what it could not find. Names, phone numbers, and email addresses in the
//! announcement (procurement contacts) are never returned.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_local::extract::{Extracted, FileKind, extract};
use phn_local::phi::{self, Identifier, RedactionLevel};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::grant_search::{any_phrase, has_phrase, padded};
use crate::context::Context;
use crate::epr::content;
use crate::md;

/// Content id of `content/planning/announcement_patterns.yaml`.
pub const PATTERNS: &str = "planning-announcement-patterns";
/// Largest announcement read.
const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;
/// Longest label returned (characters).
const MAX_LABEL: usize = 100;
/// Longest excerpt written to the checklist files (characters).
const MAX_EXCERPT: usize = 300;
const MAX_DEADLINES: usize = 30;
const MAX_CRITERIA: usize = 30;
const MAX_ATTACHMENTS: usize = 40;
const MAX_LIMITS: usize = 20;
const MAX_ELIGIBILITY: usize = 12;
const MAX_AMOUNTS: usize = 15;

/// What kind of announcement it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnnouncementKind {
    /// Work it out from the wording (the default).
    #[default]
    Auto,
    /// A federal notice of funding opportunity.
    FederalNofo,
    /// A Massachusetts request for response (COMMBUYS).
    MaRfr,
    /// Another call (foundation, request for proposals or applications).
    Other,
}

/// Arguments for `analyze_funding_announcement`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AnalyzeAnnouncementArgs {
    /// The announcement: a PDF, DOCX, HTML, TXT, or Markdown file in /data (for example
    /// "funding/nofo.pdf"). Save the public announcement there first; a client record is refused.
    pub path: String,
    /// What kind it is: auto (default), federal_nofo, ma_rfr, or other. Only changes how the
    /// result labels it.
    pub kind: Option<AnnouncementKind>,
    /// Write the checklist as DOCX and XLSX to /data/output (default true). The XLSX has a
    /// "Scoring criteria" sheet that `draft_grant_narrative` can read.
    pub write_checklist: Option<bool>,
}

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct KindCues {
    federal_nofo: Vec<String>,
    ma_rfr: Vec<String>,
    other_call: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeadlineCues {
    submission: Vec<String>,
    questions: Vec<String>,
    conference: Vec<String>,
    intent: Vec<String>,
    award: Vec<String>,
    start: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Category {
    id: String,
    label: String,
    keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CriterionType {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
    pub narrative_section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OutlineDef {
    label: String,
    criterion_type: String,
    purpose: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Patterns {
    kind_cues: KindCues,
    deadline_cues: DeadlineCues,
    limit_cues: Vec<String>,
    format_cues: Vec<String>,
    eligibility_cues: Vec<String>,
    eligibility_categories: Vec<Category>,
    submission_methods: Vec<Category>,
    required_list_cues: Vec<String>,
    rating_scale_cues: Vec<String>,
    cost_share_cues: Vec<String>,
    funding_cues: Vec<String>,
    term_cues: Vec<String>,
    criterion_types: Vec<CriterionType>,
    default_outline: Vec<OutlineDef>,
}

/// The criterion types, for `draft_grant_narrative`.
pub(crate) fn criterion_types() -> Result<Vec<CriterionType>> {
    Ok(content::load::<Patterns>(PATTERNS)?.criterion_types)
}

/// The criterion type that fits a label (the first type, in file order, with a keyword that
/// starts a word in the label).
pub(crate) fn type_for<'a>(label: &str, types: &'a [CriterionType]) -> Option<&'a CriterionType> {
    let l = format!(" {} ", phn_core::text::normalize(label));
    types.iter().find(|t| {
        t.keywords.iter().any(|k| {
            let k = phn_core::text::normalize(k);
            !k.is_empty() && l.contains(&format!(" {k}"))
        })
    })
}

// ---------------------------------------------------------------- output

/// A date found in the announcement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeadlineFinding {
    /// submission, questions, conference, intent, award, start, or other.
    pub kind: String,
    /// The date (YYYY-MM-DD), or null when the text gives no year.
    pub date: Option<String>,
    /// The date as written (short).
    pub date_text: String,
    pub time: Option<String>,
    /// Days from today (negative when past).
    pub days_from_today: Option<i64>,
    /// A short label (the start of the line the date was on, with the date removed).
    pub label: String,
    pub location: String,
}

/// A page, word, or character limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LimitFinding {
    /// The section the limit appears in (a heading) or "not tied to a heading".
    pub applies_to: String,
    pub limit: u32,
    /// pages, words, or characters.
    pub unit: String,
    pub location: String,
}

/// A required form or attachment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentFinding {
    pub label: String,
    /// attachment, appendix, exhibit, form, enclosure, schedule, or listed item.
    pub kind: String,
    /// True when the text says required, false when optional, null when not stated.
    pub required: Option<bool>,
    pub location: String,
}

/// A scoring or evaluation criterion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CriterionFinding {
    pub order: usize,
    pub label: String,
    /// Points, when the announcement gives them.
    pub points: Option<f64>,
    /// The criterion type this project assigns (need, approach, evaluation, capacity,
    /// partnerships, equity, sustainability, budget, cover), by keyword.
    pub criterion_type: Option<String>,
    /// The narrative section it belongs to: needs_statement, project_narrative, sustainability,
    /// or other.
    pub narrative_section: Option<String>,
    pub location: String,
}

/// An eligibility passage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EligibilityFinding {
    pub location: String,
    /// Applicant categories the passage mentions.
    pub categories: Vec<String>,
    /// True when the passage excludes (not eligible, may not).
    pub excludes: bool,
}

/// A dollar amount near a funding word.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AmountFinding {
    pub amount_usd: f64,
    /// The funding word found with it (for example "maximum", "total", "per year").
    pub context: String,
    pub location: String,
}

/// One row of the response outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutlineRow {
    pub order: usize,
    pub heading: String,
    pub points: Option<f64>,
    /// A page limit that applies (from the announcement), when one could be tied to it.
    pub page_limit: Option<String>,
    /// scoring criterion, submission component, default outline, or required attachments.
    pub basis: String,
    pub narrative_section: Option<String>,
    /// What to put in it.
    pub purpose: String,
}

/// Counts of what was found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementCounts {
    pub pages: Option<usize>,
    pub words: usize,
    pub deadlines: usize,
    pub page_limits: usize,
    pub attachments: usize,
    pub forms_named: usize,
    pub criteria: usize,
    pub criteria_with_points: usize,
    pub eligibility_passages: usize,
    pub amounts: usize,
}

/// Result of `analyze_funding_announcement`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementAnalysis {
    /// The file read (path under /data).
    pub file: String,
    /// federal_nofo, ma_rfr, other, or unclear.
    pub kind: String,
    /// Why that kind: the cue counts.
    pub kind_basis: String,
    /// A short title guess from the first lines (check it).
    pub title_guess: Option<String>,
    /// Announcement, solicitation, or bid numbers found (short identifiers).
    pub identifiers: Vec<String>,
    pub counts: AnnouncementCounts,
    /// The submission deadline, when one was found with a date.
    pub primary_deadline: Option<DeadlineFinding>,
    pub deadlines: Vec<DeadlineFinding>,
    pub eligibility: Vec<EligibilityFinding>,
    /// Applicant categories mentioned in passages that do not exclude them.
    pub applicant_categories: Vec<String>,
    pub page_limits: Vec<LimitFinding>,
    /// Short format rules found (font, spacing, margins, file type).
    pub format_rules: Vec<String>,
    pub attachments: Vec<AttachmentFinding>,
    /// Federal form numbers the announcement names (for example SF-424).
    pub forms_named: Vec<String>,
    /// Items the announcement lists under "must include" and similar.
    pub submission_items: Vec<String>,
    pub criteria: Vec<CriterionFinding>,
    /// Sum of the points found, when criteria have points.
    pub points_total: Option<f64>,
    /// A total the announcement states, when found.
    pub points_total_stated: Option<f64>,
    /// Rating words found when criteria are rated rather than scored.
    pub rating_scale: Vec<String>,
    pub amounts: Vec<AmountFinding>,
    pub cost_sharing_mentioned: bool,
    /// Period or term statements (short).
    pub terms: Vec<String>,
    pub submission_methods: Vec<String>,
    pub outline: Vec<OutlineRow>,
    /// What could not be found, with what to check.
    pub not_found: Vec<String>,
    /// Possible identifiers in the announcement text that were left out of the response.
    pub items_withheld: usize,
    /// Checklist files written to /data/output.
    pub checklist_files: Vec<String>,
}

// ---------------------------------------------------------------- text preparation

/// One page (PDF) or section (other formats) of the announcement.
struct PageText {
    page: Option<u32>,
    lines: Vec<String>,
    /// The lines joined by single spaces.
    flat: String,
    /// Offsets in `flat` where a heading starts.
    headings: Vec<(usize, String)>,
    /// A heading in force at the start of the page.
    lead_heading: String,
}

static NUMBERED_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\d{1,2}(?:\.\d{1,2}){0,3}[.)]?|[A-Z][.)]|(?i:section|part|step|article|appendix|attachment)\s+[A-Z0-9]{1,3}[:.]?)\s+[A-Z][^.!?]{2,80}$").unwrap()
});
static CRITERIA_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?i:criteri(?:a|on)\s+\d{1,2}|component\s+\d{1,2})\s*[:.\-–—]\s*\S").unwrap()
});

fn is_heading(line: &str) -> bool {
    let t = line.trim();
    let n = t.chars().count();
    if !(3..=90).contains(&n) {
        return false;
    }
    if NUMBERED_HEADING.is_match(t) || CRITERIA_HEADING.is_match(t) {
        return true;
    }
    let letters: Vec<char> = t.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() >= 4
        && letters.iter().all(|c| c.is_uppercase())
        && !t.ends_with('.')
        && n <= 70
}

fn clip(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        s
    } else {
        let cut: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", cut.trim_end())
    }
}

fn build_pages(ex: &Extracted) -> Vec<PageText> {
    let mut parts: Vec<(Option<u32>, String, String)> = Vec::new();
    if ex.sections.is_empty() {
        parts.push((None, String::new(), ex.text.clone()));
    } else {
        for s in &ex.sections {
            let lead = if s.page.is_some() || s.level == 0 {
                String::new()
            } else {
                s.heading.clone()
            };
            parts.push((s.page, lead, s.text.clone()));
        }
    }
    let mut pages = Vec::new();
    let mut carry = String::new();
    for (page, lead, text) in parts {
        let mut p = PageText {
            page,
            lines: Vec::new(),
            flat: String::new(),
            headings: Vec::new(),
            lead_heading: if lead.is_empty() { carry.clone() } else { lead.clone() },
        };
        if !lead.is_empty() {
            p.headings.push((0, lead.clone()));
            carry = lead;
        }
        for raw in text.lines() {
            let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
            if line.is_empty() {
                continue;
            }
            if is_heading(&line) {
                p.headings.push((p.flat.len(), clip(&line, 80)));
                carry = clip(&line, 80);
            }
            if !p.flat.is_empty() {
                p.flat.push(' ');
            }
            p.flat.push_str(&line);
            p.lines.push(line);
        }
        pages.push(p);
    }
    pages
}

impl PageText {
    fn heading_at(&self, offset: usize) -> String {
        self.headings
            .iter()
            .rev()
            .find(|(o, _)| *o <= offset)
            .map(|(_, h)| h.clone())
            .unwrap_or_else(|| self.lead_heading.clone())
    }

    fn location(&self, offset: usize) -> String {
        let h = self.heading_at(offset);
        match (self.page, h.is_empty()) {
            (Some(p), true) => format!("page {p}"),
            (Some(p), false) => format!("page {p}, \"{h}\""),
            (None, false) => format!("section \"{h}\""),
            (None, true) => "text".into(),
        }
    }

    fn line_heading(&self, idx: usize) -> String {
        // Offset of the line in `flat`.
        let mut off = 0usize;
        for l in self.lines.iter().take(idx) {
            off += l.len() + 1;
        }
        self.heading_at(off)
    }

    fn line_location(&self, idx: usize) -> String {
        let h = self.line_heading(idx);
        match (self.page, h.is_empty()) {
            (Some(p), true) => format!("page {p}"),
            (Some(p), false) => format!("page {p}, \"{h}\""),
            (None, false) => format!("section \"{h}\""),
            (None, true) => "text".into(),
        }
    }
}

// ---------------------------------------------------------------- helpers

/// Replace identifiers; `None` when anything was found, so the caller can drop the item.
fn clean(s: &str, withheld: &mut usize) -> Option<String> {
    let red = phi::redact_text(s, RedactionLevel::Strict);
    if red.text.contains("[REDACTED") {
        *withheld += 1;
        None
    } else {
        Some(s.to_string())
    }
}

fn refuse_if_client_record(text: &str) -> Result<()> {
    let n = phi::scan_text(text)
        .iter()
        .filter(|f| {
            f.confidence == phi::Confidence::High
                && matches!(
                    f.kind,
                    Identifier::Ssn
                        | Identifier::MedicalRecordNumber
                        | Identifier::HealthPlanNumber
                        | Identifier::AccountNumber
                        | Identifier::BirthDate
                )
        })
        .count();
    if n >= 3 {
        return Err(PhnError::Privacy(
            "this file holds several record numbers, social security numbers, or dates of birth: \
             it looks like a client record, not a funding announcement, so it was not read."
                .into(),
        ));
    }
    Ok(())
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let t = out.trim_matches('-').to_string();
    let t: String = t.chars().take(40).collect();
    if t.is_empty() { "announcement".into() } else { t }
}

// ---------------------------------------------------------------- dates

const MONTH_NAMES: &str = "January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sept|Sep|Oct|Nov|Dec";

static DATE_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?P<m>{MONTH_NAMES})\.?\s+(?P<d>\d{{1,2}})(?:st|nd|rd|th)?(?:,?\s*(?P<y>\d{{4}}))?"
    ))
    .unwrap()
});
static DATE_NUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<m>\d{1,2})/(?P<d>\d{1,2})/(?P<y>\d{4}|\d{2})\b").unwrap());
static DATE_ISO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})\b").unwrap());
static TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?P<h>\d{1,2})(?::(?P<min>\d{2}))?\s*(?P<ap>[ap])\.?m\b\.?(?:\s*(?P<tz>ET|EST|EDT|Eastern))?").unwrap()
});

fn month_from_name(s: &str) -> Option<u32> {
    let l = s.to_ascii_lowercase();
    let pre: String = l.chars().take(3).collect();
    ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
        .iter()
        .position(|m| *m == pre)
        .map(|i| i as u32 + 1)
}

struct FoundDate {
    start: usize,
    end: usize,
    date: Option<NaiveDate>,
    text: String,
}

fn find_dates(line: &str) -> Vec<FoundDate> {
    let mut out: Vec<FoundDate> = Vec::new();
    for c in DATE_TEXT.captures_iter(line) {
        let all = c.get(0).unwrap();
        let month = month_from_name(&c["m"]);
        let day = c["d"].parse::<u32>().ok();
        let year = c.name("y").and_then(|y| y.as_str().parse::<i32>().ok());
        let date = match (month, day, year) {
            (Some(m), Some(d), Some(y)) if (1900..=2100).contains(&y) => NaiveDate::from_ymd_opt(y, m, d),
            _ => None,
        };
        // A month word followed by a day that is not a day ("May 2026") is not a date.
        if day.is_none_or(|d| !(1..=31).contains(&d)) || (month.is_none()) {
            continue;
        }
        out.push(FoundDate {
            start: all.start(),
            end: all.end(),
            date,
            text: all.as_str().trim().to_string(),
        });
    }
    for c in DATE_NUM.captures_iter(line) {
        let all = c.get(0).unwrap();
        let (m, d) = (c["m"].parse::<u32>().ok(), c["d"].parse::<u32>().ok());
        let mut y = c["y"].parse::<i32>().ok();
        if c["y"].len() == 2 {
            y = y.map(|v| 2000 + v);
        }
        let date = match (m, d, y) {
            (Some(m), Some(d), Some(y)) if (1900..=2100).contains(&y) => NaiveDate::from_ymd_opt(y, m, d),
            _ => None,
        };
        if date.is_some() {
            out.push(FoundDate { start: all.start(), end: all.end(), date, text: all.as_str().into() });
        }
    }
    for c in DATE_ISO.captures_iter(line) {
        let all = c.get(0).unwrap();
        let date = match (c["y"].parse::<i32>(), c["m"].parse::<u32>(), c["d"].parse::<u32>()) {
            (Ok(y), Ok(m), Ok(d)) if (1900..=2100).contains(&y) => NaiveDate::from_ymd_opt(y, m, d),
            _ => None,
        };
        if date.is_some() {
            out.push(FoundDate { start: all.start(), end: all.end(), date, text: all.as_str().into() });
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

fn find_time(line: &str) -> Option<String> {
    let c = TIME.captures(line)?;
    let h: u32 = c["h"].parse().ok()?;
    let min: u32 = c.name("min").map(|m| m.as_str().parse().unwrap_or(0)).unwrap_or(0);
    if !(1..=12).contains(&h) || min > 59 {
        return None;
    }
    Some(format!(
        "{h}:{min:02} {}M",
        c["ap"].to_ascii_uppercase()
    ))
}

fn deadline_kind(lower_padded: &str, cues: &DeadlineCues) -> Option<&'static str> {
    // Questions and conferences before submission: "questions due" also contains "due".
    for (name, list) in [
        ("questions", &cues.questions),
        ("conference", &cues.conference),
        ("intent", &cues.intent),
        ("award", &cues.award),
        ("start", &cues.start),
        ("submission", &cues.submission),
    ] {
        if any_phrase(lower_padded, list) {
            return Some(name);
        }
    }
    None
}

fn extract_deadlines(
    pages: &[PageText],
    pat: &Patterns,
    today: NaiveDate,
    withheld: &mut usize,
) -> Vec<DeadlineFinding> {
    let mut out: Vec<DeadlineFinding> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let generic = ["due", "deadline", "no later than", "must be received", "by"]
        .map(String::from)
        .to_vec();
    for p in pages {
        for (i, line) in p.lines.iter().enumerate() {
            let dates = find_dates(line);
            if dates.is_empty() {
                continue;
            }
            let prev = if i > 0 { p.lines[i - 1].as_str() } else { "" };
            let own = padded(line);
            let kind = deadline_kind(&own, &pat.deadline_cues)
                .or_else(|| deadline_kind(&padded(prev), &pat.deadline_cues));
            let generic_cue = any_phrase(&own, &generic);
            let Some(kind) = kind.or(if generic_cue { Some("other") } else { None }) else {
                continue;
            };
            let time = find_time(line);
            for d in &dates {
                let mut label = format!("{} {}", &line[..d.start], &line[d.end..]);
                if let Some(t) = TIME.find(&label) {
                    label.replace_range(t.range(), " ");
                }
                for x in find_dates(&label) {
                    label.replace_range(x.start..x.end, " ");
                    break;
                }
                let label = clip(label.trim_matches(|c: char| !c.is_alphanumeric()), 80);
                let label = if label.chars().count() < 4 {
                    // A table cell: the label is on the line before.
                    clip(prev.trim_matches(|c: char| !c.is_alphanumeric()), 80)
                } else {
                    label
                };
                let Some(label) = clean(&label, withheld) else {
                    continue;
                };
                let key = format!("{kind}|{}|{}", d.date.map(|x| x.to_string()).unwrap_or_else(|| d.text.clone()), time.clone().unwrap_or_default());
                if !seen.insert(key) {
                    continue;
                }
                out.push(DeadlineFinding {
                    kind: kind.to_string(),
                    date: d.date.map(|x| x.to_string()),
                    date_text: d.text.clone(),
                    time: time.clone(),
                    days_from_today: d.date.map(|x| (x - today).num_days()),
                    label,
                    location: p.line_location(i),
                });
            }
        }
    }
    out.truncate(MAX_DEADLINES);
    out
}

// ---------------------------------------------------------------- page limits and format

static LIMIT_A: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:not\s+(?:to\s+)?exceed|no\s+(?:more|longer)\s+than|maximum\s+of|limited\s+to|up\s+to|at\s+most|limit\s+of|must\s+not\s+be\s+(?:longer|more)\s+than|shall\s+not\s+exceed|not\s+be\s+more\s+than)\s+(?:[a-z\-]+\s+)?\(?(?P<n>\d[\d,]{0,5})\)?\s*(?:[a-z\-]+\s+){0,3}?(?P<u>pages?|words?|characters?)\b").unwrap()
});
static LIMIT_B: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?P<n>\d{1,4})[- ](?P<u>page|word|character)s?\s+(?:limit|maximum|max|narrative|limitation)\b").unwrap()
});
static LIMIT_C: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:page|word|character)\s+limit(?:ation)?\s*(?:is|of|:)?\s*(?P<n>\d[\d,]{0,5})\s*(?P<u>pages?|words?|characters?)?").unwrap()
});
static LIMIT_D: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\(\s*(?P<n>\d{1,4})\s*\)\s*(?:double[- ]spaced\s+|single[- ]spaced\s+|typed\s+|numbered\s+)*(?P<u>pages?|words?|characters?)\b").unwrap()
});

fn unit_of(s: &str) -> &'static str {
    let l = s.to_ascii_lowercase();
    if l.starts_with("word") {
        "words"
    } else if l.starts_with("char") {
        "characters"
    } else {
        "pages"
    }
}

fn extract_limits(pages: &[PageText], pat: &Patterns, withheld: &mut usize) -> (Vec<LimitFinding>, Vec<String>) {
    let mut out: Vec<LimitFinding> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut rules: Vec<String> = Vec::new();
    let mut rule_seen: BTreeSet<String> = BTreeSet::new();
    for p in pages {
        for re in [&*LIMIT_A, &*LIMIT_B, &*LIMIT_C, &*LIMIT_D] {
            for c in re.captures_iter(&p.flat) {
                let all = c.get(0).unwrap();
                let Some(n) = c["n"].replace(',', "").parse::<u32>().ok().filter(|n| *n > 0) else {
                    continue;
                };
                let unit = c.name("u").map(|u| unit_of(u.as_str())).unwrap_or("pages");
                // A "(8) double spaced pages" repeats a limit already read with its words.
                let applies = {
                    let h = p.heading_at(all.start());
                    if h.is_empty() { "not tied to a heading".to_string() } else { h }
                };
                let key = format!("{applies}|{n}|{unit}");
                if !seen.insert(key) {
                    continue;
                }
                if let Some(applies) = clean(&applies, withheld) {
                    out.push(LimitFinding { applies_to: applies, limit: n, unit: unit.into(), location: p.location(all.start()) });
                }
            }
        }
        // Format rules: short snippets around a cue.
        let lower = p.flat.to_lowercase();
        for cue in &pat.format_cues {
            let cl = cue.to_lowercase();
            let mut from = 0usize;
            while let Some(i) = lower[from..].find(&cl) {
                let at = from + i;
                from = at + cl.len();
                // Whole-word check (the cue must not sit inside a longer word).
                let before_ok = lower[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
                let after_ok = lower[from..].chars().next().is_none_or(|c| !c.is_alphanumeric());
                if !before_ok || !after_ok {
                    continue;
                }
                let start = p.flat[..at].char_indices().rev().nth(40).map(|(i, _)| i).unwrap_or(0);
                let end = p.flat[from..].char_indices().nth(50).map(|(i, _)| from + i).unwrap_or(p.flat.len());
                let snippet = clip(&p.flat[start..end], 100);
                if rule_seen.insert(cl.clone()) && rules.len() < 8
                    && let Some(s) = clean(&snippet, withheld)
                {
                    rules.push(s);
                }
                break;
            }
        }
    }
    out.truncate(MAX_LIMITS);
    (out, rules)
}

// ---------------------------------------------------------------- attachments

static ATTACHMENT_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<k>(?i:appendix|attachment|exhibit|enclosure|schedule|form))\s+(?P<id>[A-Z0-9]{1,3})\s*[:.\-–—]\s*(?P<t>\S.{2,140})$").unwrap()
});
static ATTACHMENT_BARE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<k>(?i:appendix|attachment|exhibit|enclosure|schedule))\s+(?P<id>[A-Z0-9]{1,3})\s*[:.\-–—]?$").unwrap()
});
static FORM_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:SF[- ]?\d{3}[A-Z]?|SF-LLL)\b").unwrap());
static BULLET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:[\u{2022}\u{25AA}\u{25CF}\u{25E6}\u{00B7}*\-]|\(?[A-Za-z0-9]{1,2}[.)])\s+(?P<t>\S.*)$").unwrap()
});
static LONE_BULLET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[\u{2022}\u{25AA}\u{25CF}\u{25E6}\u{00B7}\-]\s*$").unwrap());

fn required_flag(text: &str) -> Option<bool> {
    let l = text.to_lowercase();
    if l.contains("optional") || l.contains("if applicable") || l.contains("if any") {
        Some(false)
    } else if l.contains("required") || l.contains("must") || l.contains("mandatory") {
        Some(true)
    } else {
        None
    }
}

fn extract_attachments(
    pages: &[PageText],
    pat: &Patterns,
    withheld: &mut usize,
) -> (Vec<AttachmentFinding>, Vec<String>, Vec<String>) {
    let mut atts: Vec<AttachmentFinding> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut forms: BTreeSet<String> = BTreeSet::new();
    let mut items: Vec<String> = Vec::new();
    let mut item_seen: BTreeSet<String> = BTreeSet::new();
    for p in pages {
        for m in FORM_TOKEN.find_iter(&p.flat) {
            forms.insert(m.as_str().replace(' ', "-").to_uppercase().replacen("SF", "SF-", 1).replace("SF--", "SF-"));
        }
        let mut i = 0;
        while i < p.lines.len() {
            let line = &p.lines[i];
            let mut label: Option<(String, String)> = None; // (kind, label)
            if let Some(c) = ATTACHMENT_LINE.captures(line) {
                label = Some((
                    c["k"].to_lowercase(),
                    format!("{} {}: {}", capitalize(&c["k"]), &c["id"], clip(&c["t"], 80)),
                ));
            } else if let Some(c) = ATTACHMENT_BARE.captures(line)
                && let Some(next) = p.lines.get(i + 1)
                && next.chars().count() <= 100
                && !is_heading(next)
                && !ATTACHMENT_BARE.is_match(next)
                && next.chars().next().is_some_and(|ch| ch.is_alphabetic())
            {
                label = Some((
                    c["k"].to_lowercase(),
                    format!("{} {}: {}", capitalize(&c["k"]), &c["id"], clip(next, 80)),
                ));
                i += 1;
            }
            if let Some((kind, label)) = label
                && seen.insert(label.to_lowercase())
                && atts.len() < MAX_ATTACHMENTS
                && let Some(label) = clean(&label, withheld)
            {
                atts.push(AttachmentFinding {
                    required: required_flag(&label).or_else(|| required_flag(&p.line_heading(i))),
                    label,
                    kind,
                    location: p.line_location(i),
                });
            }
            // A list introduced by "must include" and similar.
            if any_phrase(&padded(line), &pat.required_list_cues) {
                let mut j = i + 1;
                let mut lone = false;
                let mut taken = 0;
                while j < p.lines.len() && taken < 15 {
                    let l = &p.lines[j];
                    if LONE_BULLET.is_match(l) {
                        lone = true;
                        j += 1;
                        continue;
                    }
                    let text = if lone {
                        lone = false;
                        Some(l.clone())
                    } else {
                        BULLET.captures(l).map(|c| c["t"].to_string())
                    };
                    match text {
                        Some(t) if t.split_whitespace().count() <= 20 => {
                            let t = clip(t.trim_end_matches([';', '.']), MAX_LABEL);
                            if item_seen.insert(t.to_lowercase())
                                && items.len() < 30
                                && let Some(t) = clean(&t, withheld)
                            {
                                items.push(t);
                            }
                            taken += 1;
                        }
                        _ => break,
                    }
                    j += 1;
                }
            }
            i += 1;
        }
    }
    (atts, forms.into_iter().collect(), items)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------- scoring criteria

static POINTS_PAREN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\s\u{2022}\-*\d.)]*(?P<l>[A-Za-z][^()]{2,90}?)\s*\(\s*(?:up\s+to\s+|maximum\s+of\s+)?(?P<p>\d{1,3}(?:\.\d)?)\s*(?i:points?|pts\.?)\s*\)").unwrap()
});
static POINTS_TAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\s\u{2022}\-*\d.)]*(?P<l>[A-Za-z][^:]{2,90}?)\s*(?:[:\-–—]|\s)\s*(?i:up\s+to\s+|maximum\s+of\s+)?(?P<p>\d{1,3}(?:\.\d)?)\s*(?i:points?|pts\.?)\s*[.;]?\s*$").unwrap()
});
static POINTS_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\(?\s*(?i:up\s+to\s+)?(?P<p>\d{1,3}(?:\.\d)?)\s*(?i:points?|pts\.?)\s*\)?$").unwrap()
});
static TOTAL_POINTS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:total(?:\s+of)?|maximum(?:\s+total)?|out\s+of)\s*(?:score\s*)?[:\-]?\s*(?P<p>\d{1,4})\s*(?:total\s+)?points?\b|(?P<q>\d{1,4})\s*points?\s+(?:total|possible|maximum)").unwrap()
});

fn clean_label(s: &str) -> String {
    let mut t = s.trim().trim_matches(|c: char| matches!(c, ':' | '-' | '\u{2013}' | '\u{2014}' | '(' | ')' | '.' | ' ')).to_string();
    // A leading "A." or "1)" numbering.
    static LEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:\d{1,2}|[A-Za-z])[.)]\s+").unwrap());
    t = LEAD.replace(&t, "").to_string();
    clip(&t, MAX_LABEL)
}

struct CriteriaScan {
    criteria: Vec<(String, Option<f64>, String)>, // label, points, location
    total_stated: Option<f64>,
    rating_scale: Vec<String>,
}

fn extract_criteria(pages: &[PageText], ex: &Extracted, pat: &Patterns, withheld: &mut usize) -> CriteriaScan {
    let mut crit: Vec<(String, Option<f64>, String)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut total_stated = None;
    let mut add = |label: String, pts: Option<f64>, loc: String, withheld: &mut usize, crit: &mut Vec<(String, Option<f64>, String)>| {
        let l = label.to_lowercase();
        if l.starts_with("total") || l.chars().count() < 3 || crit.len() >= MAX_CRITERIA {
            return;
        }
        if !seen.insert(l) {
            return;
        }
        if let Some(label) = clean(&label, withheld) {
            crit.push((label, pts, loc));
        }
    };
    for p in pages {
        if let Some(c) = TOTAL_POINTS.captures(&p.flat) {
            let v = c.name("p").or(c.name("q")).and_then(|m| m.as_str().parse::<f64>().ok());
            if total_stated.is_none() {
                total_stated = v;
            }
        }
        let mut i = 0;
        while i < p.lines.len() {
            let line = &p.lines[i];
            let next_pts = p.lines.get(i + 1).and_then(|n| POINTS_ONLY.captures(n)).and_then(|c| c["p"].parse::<f64>().ok());
            let hit = POINTS_PAREN
                .captures(line)
                .or_else(|| POINTS_TAIL.captures(line))
                .and_then(|c| Some((c["l"].to_string(), c["p"].parse::<f64>().ok()?)));
            if let Some((l, pts)) = hit {
                if pts <= 1000.0 {
                    add(clean_label(&l), Some(pts), p.line_location(i), withheld, &mut crit);
                }
            } else if let Some(pts) = next_pts
                && !POINTS_ONLY.is_match(line)
                && line.chars().count() <= 90
                && line.chars().next().is_some_and(|c| c.is_alphabetic())
            {
                add(clean_label(line), Some(pts), p.line_location(i), withheld, &mut crit);
                i += 1;
            }
            i += 1;
        }
    }
    // DOCX tables with a points column.
    for t in &ex.tables {
        let headers: Vec<String> = t.headers.iter().map(|h| h.to_lowercase()).collect();
        let pcol = headers.iter().position(|h| h.contains("point") || h.contains("score") || h.contains("weight"));
        let lcol = headers.iter().position(|h| h.contains("criteri") || h.contains("section") || h.contains("component") || h.contains("factor"));
        if let (Some(pc), Some(lc)) = (pcol, lcol) {
            for r in &t.rows {
                let pts = r.get(pc).and_then(|c| c.trim().trim_end_matches(" points").parse::<f64>().ok());
                if let (Some(label), Some(pts)) = (r.get(lc), pts) {
                    add(clean_label(label), Some(pts), format!("table \"{}\"", clip(&t.name, 40)), withheld, &mut crit);
                }
            }
        }
    }
    // No points: "Criteria N: label" components and bullet lists under an evaluation heading.
    if crit.is_empty() {
        for p in pages {
            for (i, line) in p.lines.iter().enumerate() {
                if CRITERIA_HEADING.is_match(line)
                    && let Some((_, rest)) = line.split_once([':', '\u{2013}', '\u{2014}'])
                {
                    add(clean_label(rest), None, p.line_location(i), withheld, &mut crit);
                }
            }
        }
    }
    if crit.is_empty() {
        for p in pages {
            for (i, line) in p.lines.iter().enumerate() {
                let h = p.line_heading(i).to_lowercase();
                if (h.contains("evaluation criteria") || h.contains("scoring criteria") || h.contains("review criteria") || h.contains("selection criteria") || h.contains("merit review"))
                    && let Some(c) = BULLET.captures(line)
                    && c["t"].split_whitespace().count() <= 14
                {
                    add(clean_label(&c["t"]), None, p.line_location(i), withheld, &mut crit);
                }
            }
        }
    }
    // Rating words (rated rather than scored).
    let all = pages.iter().map(|p| padded(&p.flat)).collect::<String>();
    let mut rating_scale = Vec::new();
    for cue in &pat.rating_scale_cues {
        if has_phrase(&all, cue) {
            rating_scale.push(capitalize_words(cue));
        }
    }
    CriteriaScan { criteria: crit, total_stated, rating_scale }
}

fn capitalize_words(s: &str) -> String {
    s.split(' ').map(capitalize).collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------- eligibility, amounts, terms, methods

struct EligibilityInternal {
    finding: EligibilityFinding,
    excerpt: String,
}

fn sentences(flat: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let chars: Vec<(usize, char)> = flat.char_indices().collect();
    for (k, (i, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | ';' | '!' | '?') && chars.get(k + 1).is_none_or(|(_, n)| n.is_whitespace()) {
            out.push((start, flat[start..=*i].trim().to_string()));
            start = i + c.len_utf8();
        }
    }
    if start < flat.len() {
        out.push((start, flat[start..].trim().to_string()));
    }
    out
}

fn extract_eligibility(pages: &[PageText], pat: &Patterns, withheld: &mut usize) -> Vec<EligibilityInternal> {
    let mut out: Vec<EligibilityInternal> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let cue_re: Vec<Regex> = pat
        .eligibility_cues
        .iter()
        .filter_map(|c| Regex::new(&format!(r"(?i)\b{}\b", c.replace('.', "\\."))).ok())
        .collect();
    for p in pages {
        for (off, s) in sentences(&p.flat) {
            if s.chars().count() < 20 || s.chars().count() > 700 {
                continue;
            }
            let in_elig_section = p.heading_at(off).to_lowercase().contains("eligib");
            if !(in_elig_section || cue_re.iter().any(|r| r.is_match(&s))) {
                continue;
            }
            let pad = padded(&s);
            let categories: Vec<String> = pat
                .eligibility_categories
                .iter()
                .filter(|c| any_phrase(&pad, &c.keywords))
                .map(|c| c.label.clone())
                .collect();
            if categories.is_empty() && !in_elig_section {
                continue;
            }
            let l = s.to_lowercase();
            let excludes = ["not eligible", "ineligible", "may not apply", "cannot apply", "are excluded", "is excluded", "not be eligible"]
                .iter()
                .any(|x| l.contains(x));
            if !seen.insert(l) || out.len() >= MAX_ELIGIBILITY {
                continue;
            }
            let excerpt = match clean(&clip(&s, MAX_EXCERPT), withheld) {
                Some(e) => e,
                None => "[excerpt withheld: it contained a possible personal identifier]".into(),
            };
            out.push(EligibilityInternal {
                finding: EligibilityFinding { location: p.location(off), categories, excludes },
                excerpt,
            });
        }
    }
    out
}

static MONEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\s?(?P<n>\d[\d,]*(?:\.\d+)?)(?:\s?(?P<u>(?i:million|billion|thousand)|[MKmk]\b))?").unwrap()
});

fn extract_amounts(pages: &[PageText], pat: &Patterns) -> Vec<AmountFinding> {
    let mut out: Vec<AmountFinding> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for p in pages {
        for (i, line) in p.lines.iter().enumerate() {
            let pad = padded(line);
            let Some(ctx) = pat.funding_cues.iter().find(|c| has_phrase(&pad, c)) else {
                continue;
            };
            for m in MONEY.captures_iter(line) {
                let Some(base) = m["n"].replace(',', "").parse::<f64>().ok() else { continue };
                let mult = match m.name("u").map(|u| u.as_str().to_lowercase()) {
                    Some(u) if u == "million" || u == "m" => 1e6,
                    Some(u) if u == "billion" => 1e9,
                    Some(u) if u == "thousand" || u == "k" => 1e3,
                    _ => 1.0,
                };
                let v = base * mult;
                if !(1.0..1e12).contains(&v) {
                    continue;
                }
                if seen.insert(format!("{v}|{ctx}")) && out.len() < MAX_AMOUNTS {
                    out.push(AmountFinding { amount_usd: v, context: ctx.clone(), location: p.line_location(i) });
                }
            }
        }
    }
    out
}

static TERM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<cue>period of performance|project period|budget period|contract term|term of the contract|initial term|performance period|grant period|award period)[^.;]{0,100}?\b(?P<n>\d{1,3}|one|two|three|four|five|six|twelve|eighteen|twenty[- ]four|thirty[- ]six)\s*(?:\(\d+\)\s*)?(?P<u>month|year)s?\b").unwrap()
});

fn extract_terms(pages: &[PageText], withheld: &mut usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in pages {
        for m in TERM.find_iter(&p.flat) {
            if let Some(t) = clean(&clip(m.as_str(), 110), withheld)
                && !out.contains(&t)
                && out.len() < 5
            {
                out.push(t);
            }
        }
    }
    out
}

static ANNOUNCEMENT_NO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:funding opportunity number|opportunity number|announcement number|solicitation number|agency document number|rfr agency number|nofo number|procurement number)\s*:?\s*(?P<id>[A-Z0-9][A-Z0-9\-_.]{3,40})").unwrap()
});
static BID_NO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bBD-\d{2}-\d+(?:-[A-Za-z0-9]+){2,4}\b").unwrap());

fn extract_identifiers(pages: &[PageText]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in pages {
        for c in ANNOUNCEMENT_NO.captures_iter(&p.flat) {
            let id = c["id"].trim_end_matches(['.', ',']).to_string();
            if id.chars().any(|ch| ch.is_ascii_digit()) && !out.contains(&id) && out.len() < 4 {
                out.push(id);
            }
        }
        for m in BID_NO.find_iter(&p.flat) {
            let id = m.as_str().to_string();
            if !out.contains(&id) && out.len() < 4 {
                out.push(id);
            }
        }
    }
    out
}

fn guess_title(pages: &[PageText], withheld: &mut usize) -> Option<String> {
    let first = pages.first()?;
    let skip = ["commonwealth of", "executive office", "department of", "this is a single", "page ", "please note", "division of", "office of"];
    let lead: Vec<&String> = first.lines.iter().take(14).collect();
    let is_skip = |l: &str| {
        let low = l.to_lowercase();
        skip.iter().any(|s| low.starts_with(s)) || l.chars().count() < 4
    };
    let type_re = Regex::new(r"(?i)^(request for (responses?|proposals?|applications?)|notice of funding opportunity|funding opportunity announcement|call for proposals)").ok()?;
    let mut title: Option<String> = None;
    for (i, l) in lead.iter().enumerate() {
        if type_re.is_match(l) {
            let next = lead.get(i + 1).filter(|n| !is_skip(n) && !type_re.is_match(n));
            title = Some(match next {
                Some(n) => format!("{} {}", l, n),
                None => (*l).clone(),
            });
            break;
        }
    }
    let t = title.or_else(|| lead.iter().find(|l| !is_skip(l)).map(|l| (*l).clone()))?;
    clean(&clip(&t, 140), withheld)
}

fn detect_kind(pages: &[PageText], pat: &Patterns, forced: AnnouncementKind) -> (String, String) {
    let all: String = pages.iter().map(|p| padded(&p.flat)).collect();
    let count = |cues: &[String]| cues.iter().filter(|c| has_phrase(&all, c)).count();
    let (f, m, o) = (
        count(&pat.kind_cues.federal_nofo),
        count(&pat.kind_cues.ma_rfr),
        count(&pat.kind_cues.other_call),
    );
    let basis = format!("wording cues: federal NOFO {f}, Massachusetts RFR {m}, other call {o}");
    let kind = match forced {
        AnnouncementKind::FederalNofo => "federal_nofo",
        AnnouncementKind::MaRfr => "ma_rfr",
        AnnouncementKind::Other => "other",
        AnnouncementKind::Auto => {
            let best = f.max(m).max(o);
            if best < 2 {
                "unclear"
            } else if f == best && f > m {
                "federal_nofo"
            } else if m == best && m >= f {
                "ma_rfr"
            } else if o == best && o > f && o > m {
                "other"
            } else {
                "federal_nofo"
            }
        }
    };
    let basis = if forced == AnnouncementKind::Auto {
        basis
    } else {
        format!("set by you; {basis}")
    };
    (kind.into(), basis)
}

// ---------------------------------------------------------------- analysis

/// The analysis, without files, for a text already read. `today` counts days to deadlines.
fn analyze(
    label: &str,
    ex: &Extracted,
    pat: &Patterns,
    kind: AnnouncementKind,
    today: NaiveDate,
) -> Result<(AnnouncementAnalysis, Vec<EligibilityInternal>)> {
    let pages = build_pages(ex);
    let mut withheld = 0usize;
    let (kind_s, kind_basis) = detect_kind(&pages, pat, kind);
    let deadlines = extract_deadlines(&pages, pat, today, &mut withheld);
    let (limits, rules) = extract_limits(&pages, pat, &mut withheld);
    let (attachments, forms, items) = extract_attachments(&pages, pat, &mut withheld);
    let scan = extract_criteria(&pages, ex, pat, &mut withheld);
    let elig = extract_eligibility(&pages, pat, &mut withheld);
    let amounts = extract_amounts(&pages, pat);
    let terms = extract_terms(&pages, &mut withheld);
    let all_pad: String = pages.iter().map(|p| padded(&p.flat)).collect();
    let methods: Vec<String> = pat
        .submission_methods
        .iter()
        .filter(|m| any_phrase(&all_pad, &m.keywords))
        .map(|m| m.label.clone())
        .collect();
    let cost_share = any_phrase(&all_pad, &pat.cost_share_cues);
    let identifiers = extract_identifiers(&pages);
    let title_guess = guess_title(&pages, &mut withheld);

    let types = &pat.criterion_types;
    let criteria: Vec<CriterionFinding> = scan
        .criteria
        .iter()
        .enumerate()
        .map(|(i, (l, pts, loc))| {
            let t = type_for(l, types);
            CriterionFinding {
                order: i + 1,
                label: l.clone(),
                points: *pts,
                criterion_type: t.map(|t| t.id.clone()),
                narrative_section: t.map(|t| t.narrative_section.clone()),
                location: loc.clone(),
            }
        })
        .collect();
    let with_points: Vec<f64> = criteria.iter().filter_map(|c| c.points).collect();
    let points_total = (!with_points.is_empty()).then(|| with_points.iter().sum::<f64>());

    // Primary deadline: the first submission date.
    let primary = deadlines
        .iter()
        .find(|d| d.kind == "submission" && d.date.is_some())
        .cloned();

    // Outline.
    let single_limit = {
        let mut u: Vec<String> = limits.iter().map(|l| format!("{} {}", l.limit, l.unit)).collect();
        u.sort();
        u.dedup();
        (u.len() == 1).then(|| u[0].clone())
    };
    let mut outline: Vec<OutlineRow> = Vec::new();
    let page_limit_for = |heading: &str| -> Option<String> {
        let h = heading.to_lowercase();
        limits
            .iter()
            .find(|l| {
                let a = l.applies_to.to_lowercase();
                !a.is_empty() && (a.contains(&h) || h.contains(&a))
            })
            .map(|l| format!("{} {}", l.limit, l.unit))
    };
    if !criteria.is_empty() {
        let basis = if criteria.iter().any(|c| c.points.is_some()) { "scoring criterion" } else { "submission component" };
        for c in &criteria {
            let t = type_for(&c.label, types);
            let purpose = t
                .and_then(|t| pat.default_outline.iter().find(|d| d.criterion_type == t.id))
                .map(|d| d.purpose.clone())
                .unwrap_or_else(|| "Answer this criterion directly and in the order the announcement lists it.".into());
            outline.push(OutlineRow {
                order: outline.len() + 1,
                heading: c.label.clone(),
                points: c.points,
                page_limit: page_limit_for(&c.label),
                basis: basis.into(),
                narrative_section: c.narrative_section.clone(),
                purpose,
            });
        }
    } else {
        for d in &pat.default_outline {
            let t = pat.criterion_types.iter().find(|t| t.id == d.criterion_type);
            outline.push(OutlineRow {
                order: outline.len() + 1,
                heading: d.label.clone(),
                points: None,
                page_limit: None,
                basis: "default outline (the announcement's own criteria were not found)".into(),
                narrative_section: t.map(|t| t.narrative_section.clone()),
                purpose: d.purpose.clone(),
            });
        }
    }
    if let Some(l) = &single_limit
        && let Some(first) = outline.iter_mut().find(|r| r.page_limit.is_none() && r.narrative_section.as_deref() != Some("other"))
        && criteria.is_empty()
    {
        first.page_limit = Some(format!("{l} (the only limit found; check what it covers)"));
    }
    if (!attachments.is_empty() || !items.is_empty() || !forms.is_empty())
        && !outline.iter().any(|r| r.heading.to_lowercase().contains("attachment"))
    {
        outline.push(OutlineRow {
            order: outline.len() + 1,
            heading: "Required forms and attachments".into(),
            points: None,
            page_limit: None,
            basis: "required attachments".into(),
            narrative_section: Some("other".into()),
            purpose: "Collect every form, letter, and attachment in the checklist and check each against the announcement.".into(),
        });
    }

    // Not found.
    let mut not_found: Vec<String> = Vec::new();
    if primary.is_none() {
        not_found.push("No submission deadline with a full date was found: read the announcement's key dates and any amendments.".into());
    }
    if elig.is_empty() {
        not_found.push("No eligibility passage was recognized: read the eligibility section yourself and confirm that a local health department may apply.".into());
    }
    if limits.is_empty() {
        not_found.push("No page, word, or character limit was found: check the application instructions (limits are sometimes in a table, a linked guide, or the portal).".into());
    }
    if attachments.is_empty() && items.is_empty() && forms.is_empty() {
        not_found.push("No required attachments or forms were recognized: look for a required-documents list or a checklist in the announcement.".into());
    }
    if criteria.is_empty() {
        not_found.push("No scoring criteria were found, so the outline is a generic default: find the evaluation or merit review section.".into());
    } else if points_total.is_none() {
        not_found.push("The criteria have no point values (rated, or points are in a table the reader could not use).".into());
    }
    if let (Some(a), Some(b)) = (points_total, scan.total_stated)
        && (a - b).abs() > 0.01
    {
        not_found.push(format!(
            "The points found add to {a} but the announcement states a total of {b}: a criterion may be missing or mis-read."
        ));
    }
    if amounts.is_empty() {
        not_found.push("No award amounts were found.".into());
    }
    if terms.is_empty() {
        not_found.push("No project period or contract term was found.".into());
    }
    if methods.is_empty() {
        not_found.push("No submission method (portal, email, COMMBUYS, Grants.gov) was recognized.".into());
    }

    let words = ex.text.split_whitespace().count();
    let counts = AnnouncementCounts {
        pages: ex.pages,
        words,
        deadlines: deadlines.len(),
        page_limits: limits.len(),
        attachments: attachments.len(),
        forms_named: forms.len(),
        criteria: criteria.len(),
        criteria_with_points: with_points.len(),
        eligibility_passages: elig.len(),
        amounts: amounts.len(),
    };
    let mut cats: Vec<String> = Vec::new();
    for e in elig.iter().filter(|e| !e.finding.excludes) {
        for c in &e.finding.categories {
            if !cats.contains(c) {
                cats.push(c.clone());
            }
        }
    }
    let analysis = AnnouncementAnalysis {
        file: label.to_string(),
        kind: kind_s,
        kind_basis,
        title_guess,
        identifiers,
        counts,
        primary_deadline: primary,
        deadlines,
        eligibility: elig.iter().map(|e| e.finding.clone()).collect(),
        applicant_categories: cats,
        page_limits: limits,
        format_rules: rules,
        attachments,
        forms_named: forms,
        submission_items: items,
        criteria,
        points_total,
        points_total_stated: scan.total_stated,
        rating_scale: scan.rating_scale,
        amounts,
        cost_sharing_mentioned: cost_share,
        terms,
        submission_methods: methods,
        outline,
        not_found,
        items_withheld: withheld,
        checklist_files: Vec::new(),
    };
    Ok((analysis, elig))
}

// ---------------------------------------------------------------- the checklist document

fn tick() -> String {
    "[ ]".into()
}

fn checklist_document(ctx: &Context, a: &AnnouncementAnalysis, elig: &[EligibilityInternal]) -> Document {
    let p = |t: &str| Block::Paragraph(vec![Inline::plain(t)]);
    let h = |t: &str| Block::Heading { level: 2, text: t.to_string() };
    let table = |caption: &str, headers: &[&str], rows: Vec<Vec<String>>| {
        Block::Table(TableBlock {
            caption: Some(caption.to_string()),
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows,
        })
    };
    let mut blocks = vec![p(&format!(
        "Prepared {} from {}. This checklist was built by pattern matching, not by a person reading \
         the announcement: check every row against the announcement and its amendments before \
         relying on it. Items marked 'not found' need your own reading.",
        chrono::Local::now().date_naive(),
        a.file
    ))];
    blocks.push(h("Summary"));
    let mut summary = vec![
        vec!["Kind".into(), a.kind.replace('_', " "), a.kind_basis.clone()],
        vec!["Title (guess)".into(), a.title_guess.clone().unwrap_or_else(|| "not found".into()), String::new()],
        vec!["Numbers".into(), if a.identifiers.is_empty() { "not found".into() } else { a.identifiers.join(", ") }, String::new()],
        vec![
            "Submission deadline".into(),
            a.primary_deadline
                .as_ref()
                .map(|d| format!("{}{}", d.date.clone().unwrap_or_default(), d.time.as_ref().map(|t| format!(" {t}")).unwrap_or_default()))
                .unwrap_or_else(|| "not found".into()),
            a.primary_deadline.as_ref().map(|d| d.location.clone()).unwrap_or_default(),
        ],
        vec!["Submission method".into(), if a.submission_methods.is_empty() { "not found".into() } else { a.submission_methods.join("; ") }, String::new()],
        vec![
            "Points".into(),
            match (a.points_total, a.points_total_stated) {
                (Some(t), Some(s)) => format!("{t} found; announcement states {s}"),
                (Some(t), None) => format!("{t} found"),
                (None, _) => "no point values found".into(),
            },
            if a.rating_scale.is_empty() { String::new() } else { format!("Rating words: {}", a.rating_scale.join(", ")) },
        ],
    ];
    if a.cost_sharing_mentioned {
        summary.push(vec!["Cost sharing or match".into(), "mentioned".into(), "Read the passage: it may be a requirement.".into()]);
    }
    blocks.push(table("Summary", &["Item", "Found", "Notes"], summary));
    blocks.push(h("Deadlines"));
    blocks.push(table(
        "Deadlines",
        &["Done", "Type", "Date", "Time", "Days from today", "Label", "Where"],
        a.deadlines
            .iter()
            .map(|d| vec![tick(), d.kind.clone(), d.date.clone().unwrap_or_else(|| d.date_text.clone()), d.time.clone().unwrap_or_default(), d.days_from_today.map(|n| n.to_string()).unwrap_or_default(), d.label.clone(), d.location.clone()])
            .collect(),
    ));
    blocks.push(h("Eligibility"));
    blocks.push(table(
        "Eligibility",
        &["Confirmed", "Applicant types mentioned", "Excludes", "Where", "Excerpt (first 300 characters)"],
        elig.iter()
            .map(|e| vec![tick(), e.finding.categories.join("; "), if e.finding.excludes { "yes".into() } else { String::new() }, e.finding.location.clone(), e.excerpt.clone()])
            .collect(),
    ));
    blocks.push(h("Page limits and format"));
    blocks.push(table(
        "Page limits",
        &["Done", "Applies to", "Limit", "Unit", "Where"],
        a.page_limits.iter().map(|l| vec![tick(), l.applies_to.clone(), l.limit.to_string(), l.unit.clone(), l.location.clone()]).collect(),
    ));
    if !a.format_rules.is_empty() {
        blocks.push(Block::List { ordered: false, items: a.format_rules.iter().map(|r| vec![Inline::plain(format!("Format: {r}"))]).collect() });
    }
    blocks.push(h("Required forms and attachments"));
    let mut att_rows: Vec<Vec<String>> = a
        .attachments
        .iter()
        .map(|x| vec![tick(), x.label.clone(), match x.required { Some(true) => "required".into(), Some(false) => "optional".into(), None => "not stated".into() }, x.location.clone()])
        .collect();
    for f in &a.forms_named {
        att_rows.push(vec![tick(), format!("Form {f} (named in the text)"), "check".into(), String::new()]);
    }
    for i in &a.submission_items {
        att_rows.push(vec![tick(), i.clone(), "listed as something to include".into(), String::new()]);
    }
    blocks.push(table("Attachments", &["Done", "Item", "Required", "Where"], att_rows));
    blocks.push(h("Scoring criteria"));
    blocks.push(table(
        "Scoring criteria",
        &["#", "Criterion", "Points", "Type", "Narrative section", "Where"],
        a.criteria
            .iter()
            .map(|c| vec![c.order.to_string(), c.label.clone(), c.points.map(|x| md::num(x, if x.fract() == 0.0 { 0 } else { 1 })).unwrap_or_default(), c.criterion_type.clone().unwrap_or_default(), c.narrative_section.clone().unwrap_or_default(), c.location.clone()])
            .collect(),
    ));
    blocks.push(h("Response outline"));
    blocks.push(table(
        "Response outline",
        &["Order", "Section", "Points", "Page limit", "Basis", "What to include", "Done"],
        a.outline
            .iter()
            .map(|r| vec![r.order.to_string(), r.heading.clone(), r.points.map(|x| md::num(x, if x.fract() == 0.0 { 0 } else { 1 })).unwrap_or_default(), r.page_limit.clone().unwrap_or_default(), r.basis.clone(), r.purpose.clone(), tick()])
            .collect(),
    ));
    if !a.amounts.is_empty() || !a.terms.is_empty() {
        blocks.push(h("Funding and period"));
        let mut rows: Vec<Vec<String>> = a.amounts.iter().map(|x| vec![format!("${}", md::num(x.amount_usd, 0)), x.context.clone(), x.location.clone()]).collect();
        for t in &a.terms {
            rows.push(vec![String::new(), t.clone(), String::new()]);
        }
        blocks.push(table("Funding", &["Amount", "Context", "Where"], rows));
    }
    blocks.push(h("Not found"));
    if a.not_found.is_empty() {
        blocks.push(p("Every item the reader looks for was found; still check each against the announcement."));
    } else {
        blocks.push(Block::List { ordered: false, items: a.not_found.iter().map(|n| vec![Inline::plain(n.clone())]).collect() });
    }
    Document {
        title: format!("Funding announcement checklist — {}", a.title_guess.clone().unwrap_or_else(|| a.file.clone())),
        subtitle: Some("Compliance checklist and response outline".into()),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

fn summary_markdown(a: &AnnouncementAnalysis) -> String {
    let mut s = format!(
        "**Funding announcement: {}** ({}; {}; {} words{})\n\n",
        a.title_guess.clone().unwrap_or_else(|| a.file.clone()),
        a.kind.replace('_', " "),
        a.kind_basis,
        a.counts.words,
        a.counts.pages.map(|p| format!(", {p} pages")).unwrap_or_default()
    );
    s.push_str(&md::table(
        &["Found", "Count"],
        &[
            vec!["Dates".into(), a.counts.deadlines.to_string()],
            vec!["Eligibility passages".into(), a.counts.eligibility_passages.to_string()],
            vec!["Page, word, or character limits".into(), a.counts.page_limits.to_string()],
            vec!["Attachments and forms".into(), (a.counts.attachments + a.counts.forms_named).to_string()],
            vec!["Scoring criteria (with points)".into(), format!("{} ({})", a.counts.criteria, a.counts.criteria_with_points)],
            vec!["Dollar amounts".into(), a.counts.amounts.to_string()],
        ],
    ));
    s.push('\n');
    match &a.primary_deadline {
        Some(d) => s.push_str(&format!(
            "\nSubmission deadline found: **{}**{} ({}).\n",
            d.date.clone().unwrap_or_default(),
            d.time.as_ref().map(|t| format!(" at {t}")).unwrap_or_default(),
            d.days_from_today.map(|n| format!("{n} days from today")).unwrap_or_default()
        )),
        None => s.push_str("\nNo submission deadline with a full date was found.\n"),
    }
    if !a.criteria.is_empty() {
        s.push_str("\n**Scoring criteria** (in the order found):\n\n");
        s.push_str(&md::table(
            &["#", "Criterion", "Points", "Narrative section"],
            &a.criteria
                .iter()
                .map(|c| vec![c.order.to_string(), md::cell(&c.label), c.points.map(|x| md::num(x, if x.fract() == 0.0 { 0 } else { 1 })).unwrap_or_else(|| "n/a".into()), c.narrative_section.clone().unwrap_or_default()])
                .collect::<Vec<_>>(),
        ));
        s.push('\n');
    }
    if !a.page_limits.is_empty() {
        s.push_str("\n**Limits found:** ");
        s.push_str(&a.page_limits.iter().map(|l| format!("{} {} ({})", l.limit, l.unit, md::cell(&l.applies_to))).collect::<Vec<_>>().join("; "));
        s.push('\n');
    }
    if !a.not_found.is_empty() {
        s.push_str("\n**Could not find:**\n");
        for n in &a.not_found {
            s.push_str(&format!("- {n}\n"));
        }
    }
    if !a.checklist_files.is_empty() {
        s.push_str(&format!("\nChecklist written to {}.\n", a.checklist_files.join(" and ")));
    }
    s
}

/// Read a funding announcement and build the compliance checklist and outline.
pub fn analyze_funding_announcement(
    ctx: &Context,
    args: AnalyzeAnnouncementArgs,
) -> Result<Envelope<AnnouncementAnalysis>> {
    analyze_at(ctx, args, chrono::Local::now().date_naive())
}

/// [`analyze_funding_announcement`] with the date to count days from (for tests).
pub fn analyze_at(
    ctx: &Context,
    args: AnalyzeAnnouncementArgs,
    today: NaiveDate,
) -> Result<Envelope<AnnouncementAnalysis>> {
    if args.path.trim().is_empty() {
        return Err(PhnError::InvalidInput(
            "path is required: the funding announcement (PDF, DOCX, HTML, TXT, or Markdown) saved in /data".into(),
        ));
    }
    let pat: Patterns = content::load(PATTERNS)?;
    let sandbox = ctx.sandbox()?;
    let file = sandbox.resolve_existing(&args.path)?;
    let label = sandbox.display_path(&file);
    match FileKind::of(&file) {
        FileKind::Pdf | FileKind::Docx | FileKind::Html | FileKind::Text | FileKind::Markdown => {}
        _ => {
            return Err(PhnError::Unsupported(format!(
                "'{label}' is not a document: a funding announcement is a PDF, DOCX, HTML, TXT, or Markdown file. Spreadsheets and data files are not read here."
            )));
        }
    }
    let size = std::fs::metadata(&file)?.len();
    if size > MAX_FILE_BYTES {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' is larger than {} MB",
            MAX_FILE_BYTES / 1024 / 1024
        )));
    }
    let ex = extract(&file)?;
    refuse_if_client_record(&ex.text)?;
    if ex.text.trim().chars().count() < 200 {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' has almost no readable text (a scanned PDF has none): save a text version of the announcement, or copy its text into a .txt or .docx file"
        )));
    }
    let (mut analysis, elig) = analyze(&label, &ex, &pat, args.kind.unwrap_or_default(), today)?;

    let mut files_written = Vec::new();
    if args.write_checklist.unwrap_or(true) {
        let doc = checklist_document(ctx, &analysis, &elig);
        let stem = format!("funding_checklist_{}", slug(&analysis.title_guess.clone().unwrap_or_else(|| label.clone())));
        let docx = sandbox.new_output_file(&format!("{stem}.docx"))?;
        std::fs::write(&docx, phn_docs::docx::to_docx(&doc)?)?;
        let xlsx = sandbox.new_output_file(&format!("{stem}.xlsx"))?;
        std::fs::write(&xlsx, phn_docs::xlsx::to_xlsx(&doc)?)?;
        files_written = vec![sandbox.display_path(&docx), sandbox.display_path(&xlsx)];
        analysis.checklist_files = files_written.clone();
    }

    let mut caveats = vec![
        "The checklist comes from pattern matching on the announcement's text. It can miss items written in tables, images, or linked documents, and can mistake a sentence for a requirement: check every row against the announcement and its amendments.".to_string(),
        "A match on eligibility is a lead, not a determination: confirm with the funder that a local health department may apply.".to_string(),
    ];
    if analysis.items_withheld > 0 {
        caveats.push(format!(
            "{} item(s) were left out of the response because they contained a possible personal identifier (a name, phone number, or email address, for example a procurement contact).",
            analysis.items_withheld
        ));
    }
    if matches!(analysis.kind.as_str(), "federal_nofo") {
        caveats.push("The reader was tried on public Massachusetts announcements and synthetic notices, not on a real federal NOFO (Grants.gov could not be reached): federal layouts may be read less completely.".into());
    }
    caveats.extend(content::review_caveats(&[PATTERNS]));
    let mut notes = vec![
        "Only structure, counts, dates, and short labels are returned; the announcement's text and any contact people in it are not. Excerpts of eligibility passages go only to the checklist files in /data/output.".to_string(),
    ];
    if !files_written.is_empty() {
        notes.push("The checklist files may repeat short excerpts of the announcement.".into());
    }
    let summary = summary_markdown(&analysis);
    let sources = vec![
        Source::local_file(label.clone()),
        Source::computed("Pattern-based reading of the announcement", "PHN MCP server"),
    ];
    Ok(Envelope::new(summary, analysis)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            row_level_data: false,
            files_written,
            notes,
            ..Privacy::none()
        })
        .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_local::extract::Section;

    fn ex_from(text: &str) -> Extracted {
        Extracted {
            kind: FileKind::Text,
            text: text.to_string(),
            sections: vec![Section { heading: String::new(), level: 0, page: Some(1), text: text.to_string() }],
            tables: Vec::new(),
            pages: Some(1),
            notes: Vec::new(),
        }
    }

    fn pat() -> Patterns {
        content::load::<Patterns>(PATTERNS).unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()
    }

    const SAMPLE: &str = "NOTICE OF FUNDING OPPORTUNITY\nCommunity Prevention Grants\nFunding Opportunity Number: PHX-2027-014\n\
1. Eligible Applicants\nEligible applicants are county and municipal health departments and boards of health. Individuals and for-profit businesses are not eligible.\n\
2. Key Dates\nApplications due: February 12, 2027 at 5:00 PM ET\nLetter of intent due January 8, 2027\nApplicant webinar: December 9, 2026\nAnticipated award date: 4/1/2027\n\
3. Application Requirements\nThe project narrative must not exceed ten (10) pages, double spaced, 12-point Times New Roman font, with one inch margins.\n\
Applicants must include the following:\n- SF-424 Application for Federal Assistance\n- Budget narrative\n- Letters of support\n\
Attachment A: Work Plan Template\nAttachment B: Letters of Support (optional)\n\
4. Merit Review Criteria\nStatement of Need (20 points)\nApproach (30 points)\nEvaluation Plan (25 points)\nOrganizational Capacity (15 points)\nSustainability (10 points)\nTotal: 100 points\n\
Awards of up to $250,000 per year for a 3 year period of performance. Cost sharing is not required. Applications are submitted through Grants.gov; applicants need a UEI in SAM.gov.\n";

    #[test]
    fn finds_deadlines_limits_attachments_and_criteria() {
        let (a, elig) = analyze("sample.txt", &ex_from(SAMPLE), &pat(), AnnouncementKind::Auto, today()).unwrap();
        assert_eq!(a.kind, "federal_nofo");
        assert_eq!(a.identifiers, vec!["PHX-2027-014"]);
        let due = a.primary_deadline.as_ref().unwrap();
        assert_eq!(due.date.as_deref(), Some("2027-02-12"));
        assert_eq!(due.time.as_deref(), Some("5:00 PM"));
        assert_eq!(due.days_from_today, Some(92));
        assert!(a.deadlines.iter().any(|d| d.kind == "intent" && d.date.as_deref() == Some("2027-01-08")));
        assert!(a.deadlines.iter().any(|d| d.kind == "conference" && d.date.as_deref() == Some("2026-12-09")));
        assert!(a.deadlines.iter().any(|d| d.kind == "award" && d.date.as_deref() == Some("2027-04-01")));
        assert!(a.page_limits.iter().any(|l| l.limit == 10 && l.unit == "pages"));
        assert!(a.format_rules.iter().any(|r| r.to_lowercase().contains("times new roman") || r.to_lowercase().contains("margin")));
        assert!(a.attachments.iter().any(|x| x.label.starts_with("Attachment A: Work Plan")));
        let b = a.attachments.iter().find(|x| x.label.starts_with("Attachment B")).unwrap();
        assert_eq!(b.required, Some(false));
        assert_eq!(a.forms_named, vec!["SF-424"]);
        assert!(a.submission_items.iter().any(|i| i.contains("Budget narrative")));
        assert_eq!(a.criteria.len(), 5);
        assert_eq!(a.criteria[0].label, "Statement of Need");
        assert_eq!(a.criteria[0].points, Some(20.0));
        assert_eq!(a.criteria[0].narrative_section.as_deref(), Some("needs_statement"));
        assert_eq!(a.criteria[4].narrative_section.as_deref(), Some("sustainability"));
        assert_eq!(a.points_total, Some(100.0));
        assert_eq!(a.points_total_stated, Some(100.0));
        assert!(a.amounts.iter().any(|m| m.amount_usd == 250000.0));
        assert!(a.terms.iter().any(|t| t.contains("period of performance")));
        assert!(a.submission_methods.iter().any(|m| m.contains("Grants.gov")));
        assert!(a.cost_sharing_mentioned);
        assert_eq!(a.outline.len(), 6, "{:?}", a.outline.iter().map(|r| &r.heading).collect::<Vec<_>>());
        assert_eq!(a.outline[1].points, Some(30.0));
        // Eligibility: municipalities are named, individuals are excluded.
        assert!(a.applicant_categories.iter().any(|c| c.contains("Municipality")));
        assert!(elig.iter().any(|e| e.finding.excludes));
        assert!(a.not_found.iter().all(|n| !n.contains("deadline") && !n.contains("scoring")));
    }

    #[test]
    fn says_what_it_could_not_find() {
        let text = "This is a short letter about a community program. ".repeat(10);
        let (a, _) = analyze("x.txt", &ex_from(&text), &pat(), AnnouncementKind::Auto, today()).unwrap();
        assert_eq!(a.kind, "unclear");
        assert!(a.primary_deadline.is_none());
        assert!(a.criteria.is_empty());
        assert!(a.not_found.iter().any(|n| n.contains("submission deadline")));
        assert!(a.not_found.iter().any(|n| n.contains("scoring criteria")));
        // Without criteria the outline is the generic default and says so.
        assert!(a.outline.iter().all(|r| r.basis.contains("default outline")));
    }

    #[test]
    fn criteria_without_points_use_components_and_rating_words() {
        let text = "REQUEST FOR RESPONSES (RFR)\nHealth Outreach Services\nCriteria 1: Cover Letter\nCriteria 2: Experience and Qualifications Narrative\nThe narrative must not exceed eight (8) double spaced pages.\nCriteria 3: Fee Proposal\nResponses are evaluated as Highly Advantageous, Advantageous, or Not Advantageous. Responses are submitted through COMMBUYS. Bidder questions due 10/3/2026. Bidder RFR responses due 10/20/2026 5:00 PM bidders commonwealth of massachusetts rfr ".repeat(1);
        let (a, _) = analyze("rfr.txt", &ex_from(&text), &pat(), AnnouncementKind::Auto, today()).unwrap();
        assert_eq!(a.kind, "ma_rfr");
        assert_eq!(a.criteria.len(), 3);
        assert!(a.criteria.iter().all(|c| c.points.is_none()));
        assert_eq!(a.criteria[1].label, "Experience and Qualifications Narrative");
        assert_eq!(a.criteria[1].narrative_section.as_deref(), Some("project_narrative"));
        assert_eq!(a.outline[0].basis, "submission component");
        assert!(a.rating_scale.iter().any(|r| r == "Highly Advantageous"));
        assert!(a.page_limits.iter().any(|l| l.limit == 8));
        assert!(a.deadlines.iter().any(|d| d.kind == "questions" && d.date.as_deref() == Some("2026-10-03")));
        let due = a.primary_deadline.as_ref().unwrap();
        assert_eq!(due.date.as_deref(), Some("2026-10-20"));
        assert_eq!(due.days_from_today, Some(-23));
        // The page limit is tied to the criterion's outline row.
        assert!(a.not_found.iter().any(|n| n.contains("no point values")));
    }

    #[test]
    fn contact_people_are_not_returned() {
        let text = format!(
            "{SAMPLE}\nQuestions due January 5, 2027 to Veronika Haldane at veronika.haldane@example.org or 413-555-0142.\n"
        );
        let (a, _) = analyze("c.txt", &ex_from(&text), &pat(), AnnouncementKind::Auto, today()).unwrap();
        let json = serde_json::to_string(&a).unwrap();
        for needle in ["Haldane", "veronika", "555-0142"] {
            assert!(!json.contains(needle), "{needle} leaked");
        }
        assert!(a.items_withheld >= 1);
    }

    #[test]
    fn dates_outside_1900_2100_and_two_digit_years() {
        assert!(find_dates("due 3/5/1850").is_empty());
        let d = find_dates("due 3/5/27");
        assert_eq!(d[0].date, NaiveDate::from_ymd_opt(2027, 3, 5));
        let d = find_dates("Due March 5, 2027 and May 2026");
        assert_eq!(d.len(), 1);
        assert_eq!(find_time("by 5 pm"), Some("5:00 PM".into()));
        assert_eq!(find_time("by 13:00 pm"), None);
    }

    #[test]
    fn labels_are_clipped_by_characters() {
        let s = "é".repeat(300);
        let c = clip(&s, MAX_LABEL);
        assert_eq!(c.chars().count(), MAX_LABEL);
        assert!(c.ends_with("..."));
    }

    #[test]
    fn criterion_types_follow_file_order() {
        let types = criterion_types().unwrap();
        let t = |l: &str| type_for(l, &types).map(|t| t.id.clone());
        assert_eq!(t("Sustainability Plan").as_deref(), Some("sustainability"));
        assert_eq!(t("Statement of Need").as_deref(), Some("need"));
        assert_eq!(t("Project Approach and Work Plan").as_deref(), Some("approach"));
        assert_eq!(t("Evaluation Plan").as_deref(), Some("evaluation"));
        assert_eq!(t("Budget and Budget Justification").as_deref(), Some("budget"));
        assert_eq!(t("Past Performance and Experience").as_deref(), Some("capacity"));
        assert_eq!(t("Zebra"), None);
    }
}
