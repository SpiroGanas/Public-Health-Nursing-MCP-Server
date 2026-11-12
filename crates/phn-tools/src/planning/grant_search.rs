//! PLAN-06 `search_grant_opportunities`: open funding opportunities for a health department.
//!
//! Federal: the Simpler.Grants.gov search API when the nurse sets an optional key
//! (`PHN_GRANTS_GOV_API_KEY`); without a key, or when the request fails, the answer says it is in
//! snapshot mode, and the snapshot holds no federal listings (the federal hosts were not reachable
//! when it was built). Massachusetts: the Operational Services Division's weekly list of new
//! COMMBUYS bids, read live from mass.gov when it can be, otherwise the bundled copy with its date.
//! Requests carry topic words and filters only; the topic is checked for personal identifiers.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::grants::{
    self, CommbuysBid, CommbuysList, FederalOpportunity, FederalOutcome, FederalQuery,
};
use phn_local::phi;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Cited, CitedOut, DocSource};
use crate::md;

/// Content id of `content/planning/grant_sources.yaml`.
pub const GRANT_SOURCES: &str = "planning-grant-sources";
/// Opportunities listed in one answer.
const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;
/// How long a fetched weekly list is reused (the list changes weekly).
const COMMBUYS_TTL_SECS: u64 = 6 * 3600;

/// Whose eligibility to filter federal opportunities by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrantEligibility {
    /// A municipal or county health department or board of health (the default).
    #[default]
    LocalHealthAgency,
    /// A state health agency.
    StateAgency,
    /// A tribal health program.
    TribalHealth,
    /// Do not filter by applicant type.
    Any,
}

impl GrantEligibility {
    fn id(self) -> &'static str {
        match self {
            Self::LocalHealthAgency => "local_health_agency",
            Self::StateAgency => "state_agency",
            Self::TribalHealth => "tribal_health",
            Self::Any => "any",
        }
    }
}

/// Which listings to search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrantSource {
    /// Federal (Grants.gov) and Massachusetts (COMMBUYS) listings (the default).
    #[default]
    Both,
    /// Federal listings only.
    Federal,
    /// COMMBUYS listings only.
    Commbuys,
}

/// Arguments for `search_grant_opportunities`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct GrantSearchArgs {
    /// A subject in plain words, for example "immunization", "overdose prevention", or one of
    /// the topic names the result lists (public_health, behavioral_health, maternal_child,
    /// communicable_disease, chronic_disease, older_adults, environmental_health,
    /// emergency_preparedness, housing_social, workforce_capacity). At most 100 characters; never
    /// a client, family, or case. Default: all health topics.
    pub topic: Option<String>,
    /// Whose eligibility to filter federal opportunities by: local_health_agency (default),
    /// state_agency, tribal_health, or any. COMMBUYS listings do not state eligibility and are
    /// kept with a note to read the solicitation.
    pub eligibility: Option<GrantEligibility>,
    /// Where to search: both (default), federal, or commbuys.
    pub source: Option<GrantSource>,
    /// Keep opportunities that close on or after this date (YYYY-MM-DD). Default: today.
    pub closes_after: Option<String>,
    /// Keep opportunities that close on or before this date (YYYY-MM-DD).
    pub closes_before: Option<String>,
    /// Shortcut for closes_before: close within this many days of today.
    pub within_days: Option<u32>,
    /// Keep federal opportunities whose award ceiling is at least this many dollars. COMMBUYS
    /// listings have no amount; they are kept and flagged.
    pub min_award: Option<u64>,
    /// Keep federal opportunities whose award floor is at most this many dollars.
    pub max_award: Option<u64>,
    /// Include forecasted (not yet posted) federal opportunities (default false).
    pub include_forecasted: Option<bool>,
    /// Show only COMMBUYS bids that look like grants or funding offers (a title with "grant",
    /// "NOFA", "NOFO", or "request for applications"), not contracts or purchases (default false).
    pub grants_only: Option<bool>,
    /// How many opportunities to list (default 25, at most 100).
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct EligibilityDef {
    id: String,
    label: String,
    applicant_types: Vec<String>,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TopicDef {
    id: String,
    label: String,
    keywords: Vec<String>,
    funding_categories: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct DirectoryDef {
    id: String,
    name: String,
    level: String,
    url: String,
    what: String,
    how_to_use: String,
    reached: bool,
    source: String,
    confidence: content::Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct ReachDef {
    host: String,
    result: String,
    used_for: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SourcesContent {
    sources: BTreeMap<String, DocSource>,
    eligibility: Vec<EligibilityDef>,
    topics: Vec<TopicDef>,
    strong_grant_keywords: Vec<String>,
    funding_kind_keywords: Vec<String>,
    health_agency_keywords: Vec<String>,
    directory: Vec<DirectoryDef>,
    massachusetts_notes: Vec<Cited>,
    how_to_search: Vec<String>,
    reach_log: Vec<ReachDef>,
}

// ---------------------------------------------------------------- output

/// How one source of listings was read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceStatus {
    /// "federal" or "commbuys".
    pub source: String,
    /// live, cache, or snapshot.
    pub mode: String,
    /// The date of the data used: today for live data, the list date for COMMBUYS.
    pub as_of: String,
    /// Why this mode was used, when it is not live.
    pub reason: Option<String>,
    /// Listings that passed your filters.
    pub matched: usize,
    /// Listings read from the source before filtering (federal: as returned by the search).
    pub read: usize,
}

/// One opportunity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GrantOpportunity {
    /// "grants.gov" or "commbuys".
    pub source: String,
    /// Opportunity number (federal) or COMMBUYS bid number.
    pub id: String,
    pub title: String,
    pub agency: String,
    /// grant_or_funding, response_request (RFR/RFP), purchase_or_contract, or federal_grant.
    pub kind: String,
    /// Closing date (YYYY-MM-DD).
    pub closes: Option<String>,
    pub close_time: Option<String>,
    /// Days from today to the closing date.
    pub days_left: Option<i64>,
    pub award_floor: Option<u64>,
    pub award_ceiling: Option<u64>,
    pub total_funding: Option<u64>,
    /// Who may apply, as the source states it (federal), or empty.
    pub eligible_applicants: Vec<String>,
    /// True when the listing states who may apply and that fits your agency type; false when the
    /// listing does not say (read the solicitation).
    pub eligibility_stated: bool,
    /// Topic ids matched.
    pub topics: Vec<String>,
    /// The opportunity's own summary, cut short (federal only).
    pub summary: Option<String>,
    /// How to find the full announcement.
    pub how_to_find: String,
    pub notes: Vec<String>,
}

/// A place to look for more.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DirectoryOut {
    pub name: String,
    pub level: String,
    pub url: String,
    pub what: String,
    pub how_to_use: String,
    /// Whether the page could be fetched when the content was built.
    pub reached_when_built: bool,
    pub confidence: content::Confidence,
}

/// Result of `search_grant_opportunities`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GrantSearchResult {
    pub federal: Option<SourceStatus>,
    pub commbuys: Option<SourceStatus>,
    /// The filters that were applied, in words.
    pub filters_applied: Vec<String>,
    /// Filters that could not be applied to every listing, in words.
    pub filters_not_applied: Vec<String>,
    pub opportunities: Vec<GrantOpportunity>,
    /// Listings that matched but are beyond `limit`.
    pub more_matches: usize,
    /// Listings on the COMMBUYS list that had already closed.
    pub already_closed: usize,
    pub topics_available: Vec<String>,
    pub where_to_look: Vec<DirectoryOut>,
    pub how_to_search: Vec<String>,
    pub massachusetts_notes: Vec<CitedOut>,
    /// What could and could not be reached when the content was built.
    pub reach_log: Vec<String>,
}

// ---------------------------------------------------------------- matching

/// Lower-case words of `s` joined by single spaces and padded with spaces, for whole-phrase search.
pub(crate) fn padded(s: &str) -> String {
    format!(" {} ", phn_core::text::normalize(s))
}

pub(crate) fn has_phrase(haystack_padded: &str, phrase: &str) -> bool {
    let p = phn_core::text::normalize(phrase);
    !p.is_empty() && haystack_padded.contains(&format!(" {p} "))
}

pub(crate) fn any_phrase(haystack_padded: &str, phrases: &[String]) -> bool {
    phrases.iter().any(|p| has_phrase(haystack_padded, p))
}

/// Resolve the topic text to the keywords to match, and the topic definition when it names one.
fn resolve_topic<'a>(topic: &str, topics: &'a [TopicDef]) -> (Vec<String>, Option<&'a TopicDef>) {
    let t = phn_core::text::normalize(topic);
    let def = topics.iter().find(|d| {
        phn_core::text::normalize(&d.id) == t
            || phn_core::text::normalize(&d.id.replace('_', " ")) == t
            || phn_core::text::normalize(&d.label) == t
            || d.keywords.iter().any(|k| phn_core::text::normalize(k) == t)
    });
    let mut kws = vec![topic.trim().to_string()];
    if let Some(d) = def {
        kws.extend(d.keywords.iter().cloned());
    }
    (kws, def)
}

fn topics_of(text_padded: &str, topics: &[TopicDef]) -> Vec<String> {
    topics
        .iter()
        .filter(|d| any_phrase(text_padded, &d.keywords))
        .map(|d| d.id.clone())
        .collect()
}

fn commbuys_kind(title_padded: &str, c: &SourcesContent) -> &'static str {
    if any_phrase(title_padded, &c.strong_grant_keywords) {
        "grant_or_funding"
    } else if any_phrase(title_padded, &c.funding_kind_keywords) {
        "response_request"
    } else {
        "purchase_or_contract"
    }
}

fn parse_day(label: &str, s: &str) -> Result<NaiveDate> {
    let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| {
        PhnError::InvalidInput(format!("{label} must be a date like 2027-01-31, not '{s}'"))
    })?;
    if !(1900..=2100).contains(&chrono::Datelike::year(&d)) {
        return Err(PhnError::InvalidInput(format!(
            "{label} '{s}' is outside the years 1900 to 2100"
        )));
    }
    Ok(d)
}

fn money(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("${out}")
}

// ---------------------------------------------------------------- reading the weekly list from its PDF

/// Month name (lower case) to number.
fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let n = name.to_ascii_lowercase();
    MONTHS.iter().position(|m| *m == n).map(|i| i as u32 + 1)
}

fn iso_date(month: &str, day: &str, year: &str) -> Option<String> {
    let (m, d, y) = (
        month_number(month)?,
        day.parse::<u32>().ok()?,
        year.parse::<i32>().ok()?,
    );
    if !(1900..=2100).contains(&y) {
        return None;
    }
    NaiveDate::from_ymd_opt(y, m, d).map(|x| x.to_string())
}

static ROW_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{3,5}(?:\s+\S.*)?$").unwrap());
static BID_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"BD-\d{2}-\d{3,5}(?:\s*-\s*[A-Za-z0-9]+){2,4}").unwrap());
static ROW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?P<id>\d{3,5})\s+(?P<org>.+?)\s+(?P<month>January|February|March|April|May|June|July|August|September|October|November|December)\s+(?P<day>\d{1,2}),?\s*(?:(?P<year>\d{4})\s+)?(?P<time>\d{1,2}:\d{2}\s*[AP]M)\s+(?P<desc>.*?)\s*(?P<bid>BD-.*)$",
    )
    .unwrap()
});
static LIST_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday)\s*,?\s*(?P<month>[A-Za-z]+)\s+(?P<day>\d{1,2})\s*,\s*(?P<year>\d{4})").unwrap()
});

/// Read the weekly list from the text of the OSD PDF as the server's PDF
/// reader extracts it (cells on separate lines, spacing irregular). A row runs
/// from its organization id to the line holding its bid number. Returns an
/// error unless every bid number in the text produced a row, so a changed
/// layout is reported instead of read wrongly.
pub fn parse_weekly_text(text: &str) -> Result<CommbuysList> {
    let collapse = |l: &str| {
        l.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(" ,", ",")
    };
    let lines: Vec<String> = text
        .lines()
        .map(collapse)
        .filter(|l| !l.is_empty())
        .collect();
    let header_cells = [
        "Org ID",
        "Organization",
        "Bid Date",
        "Bid Time",
        "Description",
        "Bid Number",
    ];
    let mut list_date = None;
    let mut rows: Vec<CommbuysBid> = Vec::new();
    let mut acc: Option<String> = None;
    for line in &lines {
        if list_date.is_none()
            && let Some(c) = LIST_DATE.captures(line)
        {
            list_date = iso_date(&c["month"], &c["day"], &c["year"]);
            continue;
        }
        if header_cells.contains(&line.as_str()) || line.starts_with("New Bids Available") {
            continue;
        }
        match &mut acc {
            None => {
                if ROW_START.is_match(line) {
                    acc = Some(line.clone());
                }
            }
            Some(a) => {
                a.push(' ');
                a.push_str(line);
            }
        }
        if let Some(a) = &acc
            && BID_TOKEN.is_match(a)
        {
            let joined = a.clone();
            acc = None;
            if let Some(c) = ROW.captures(&joined) {
                let mut org = c["org"].trim().to_string();
                let year = match c.name("year") {
                    Some(y) => Some(y.as_str().to_string()),
                    None => {
                        // A row split by a page break can carry its year after the organization.
                        let tail = org
                            .rsplit_once(' ')
                            .map(|(h, t)| (h.to_string(), t.to_string()));
                        match tail {
                            Some((head, t))
                                if t.len() == 4 && t.chars().all(|ch| ch.is_ascii_digit()) =>
                            {
                                org = head;
                                Some(t)
                            }
                            _ => None,
                        }
                    }
                };
                let bid_number: String = BID_TOKEN
                    .find(&c["bid"])
                    .map(|m| m.as_str().split_whitespace().collect())
                    .unwrap_or_default();
                rows.push(CommbuysBid {
                    org_id: c["id"].to_string(),
                    organization: org,
                    bid_date: year.and_then(|y| iso_date(&c["month"], &c["day"], &y)),
                    bid_time: c["time"].split_whitespace().collect::<Vec<_>>().join(" "),
                    description: c["desc"].trim().to_string(),
                    bid_number,
                });
            }
        }
    }
    let expected = BID_TOKEN.find_iter(&lines.join(" ")).count();
    let complete = rows.iter().filter(|r| r.bid_date.is_some()).count();
    match list_date {
        Some(date) if expected > 0 && complete == expected => Ok(CommbuysList {
            list_title: "New Bids Available (COMMBUYS weekly list)".into(),
            list_date: date,
            source_url: grants::COMMBUYS_LIST_URL.into(),
            bids: rows.into_iter().filter(|r| r.bid_date.is_some()).collect(),
        }),
        _ => Err(PhnError::Network(format!(
            "the COMMBUYS weekly list did not have the expected layout ({complete} of {expected} bids read)"
        ))),
    }
}

/// The weekly list as read by `phn_local::pdf` (live or cached), or the bundled copy.
struct CommbuysRead {
    list: CommbuysList,
    mode: SourceMode,
    as_of: String,
    reason: Option<String>,
    source: Source,
}

async fn read_commbuys(ctx: &Context, today: NaiveDate) -> Result<CommbuysRead> {
    let (snap, snap_source) = grants::commbuys_snapshot(&ctx.snapshots)?;
    let snap_date = snap.list_date.clone();
    let validate = |bytes: &[u8]| -> Result<()> {
        let info = phn_local::pdf::parse_pdf(bytes)
            .map_err(|e| PhnError::Network(format!("not a readable PDF: {e}")))?;
        parse_weekly_text(&info.text()).map(|_| ())
    };
    let date_for_fallback = snap_date.clone();
    let fetched = phn_data::apifetch::fetch_validated(
        &ctx.http,
        ctx.cache.as_ref(),
        grants::COMMBUYS_LIST_URL,
        None,
        COMMBUYS_TTL_SECS,
        &validate,
        Some(Box::new(move || {
            Some((Vec::new(), date_for_fallback.clone()))
        })),
    )
    .await;
    let fetched = match fetched {
        Ok(f) => f,
        Err(e) => {
            return Ok(CommbuysRead {
                list: snap,
                mode: SourceMode::Snapshot,
                as_of: snap_date,
                reason: Some(phn_data::apifetch::reason(&e)),
                source: snap_source,
            });
        }
    };
    if fetched.mode == SourceMode::Snapshot {
        return Ok(CommbuysRead {
            list: snap,
            mode: SourceMode::Snapshot,
            as_of: snap_date,
            reason: fetched.fallback_reason,
            source: snap_source,
        });
    }
    let info = phn_local::pdf::parse_pdf(&fetched.body).map_err(PhnError::Internal)?;
    let list = parse_weekly_text(&info.text())?;
    let retrieved = phn_data::fetch::local_date(&fetched.as_of);
    let source = Source {
        title: "COMMBUYS weekly list of new bids".into(),
        publisher: "Massachusetts Operational Services Division (mass.gov)".into(),
        url: Some(grants::COMMBUYS_LIST_URL.into()),
        effective_date: Some(list.list_date.clone()),
        retrieved: Some(retrieved.clone()),
        mode: fetched.mode,
    };
    let _ = today;
    Ok(CommbuysRead {
        as_of: list.list_date.clone(),
        list,
        mode: fetched.mode,
        reason: fetched.fallback_reason,
        source,
    })
}

fn mode_word(m: SourceMode) -> &'static str {
    match m {
        SourceMode::Live => "live",
        SourceMode::Cache => "cache",
        _ => "snapshot",
    }
}

fn days_between(today: NaiveDate, iso: &str) -> Option<i64> {
    NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .ok()
        .map(|d| (d - today).num_days())
}

/// Does a bid's text match a free-text topic: the whole phrase, or every word of three letters or more?
fn free_topic_matches(text_padded: &str, topic: &str) -> bool {
    if has_phrase(text_padded, topic) {
        return true;
    }
    let words: Vec<String> = phn_core::text::normalize(topic)
        .split(' ')
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_string)
        .collect();
    !words.is_empty() && words.iter().all(|w| has_phrase(text_padded, w))
}

fn federal_row(
    o: &FederalOpportunity,
    today: NaiveDate,
    stated: bool,
    topics: Vec<String>,
) -> GrantOpportunity {
    let mut notes = Vec::new();
    if o.status == "forecasted" {
        notes.push("Forecasted: not yet posted; dates and amounts can change.".into());
    }
    if o.is_cost_sharing == Some(true) {
        notes.push("The listing says cost sharing is required.".into());
    }
    GrantOpportunity {
        source: "grants.gov".into(),
        id: o
            .opportunity_number
            .clone()
            .unwrap_or_else(|| o.opportunity_id.clone()),
        title: o.title.clone(),
        agency: o
            .agency_name
            .clone()
            .or_else(|| o.top_level_agency_name.clone())
            .unwrap_or_else(|| "(agency not stated)".into()),
        kind: "federal_grant".into(),
        closes: o.close_date.clone(),
        close_time: None,
        days_left: o.close_date.as_deref().and_then(|d| days_between(today, d)),
        award_floor: o.award_floor,
        award_ceiling: o.award_ceiling,
        total_funding: o.estimated_total_program_funding,
        eligible_applicants: o.applicant_types.clone(),
        eligibility_stated: stated,
        topics,
        summary: o.summary.clone(),
        how_to_find: format!(
            "Search simpler.grants.gov for opportunity number {}.",
            o.opportunity_number.as_deref().unwrap_or(&o.opportunity_id)
        ),
        notes,
    }
}

fn commbuys_row(
    b: &CommbuysBid,
    today: NaiveDate,
    kind: &str,
    topics: Vec<String>,
    health_agency: bool,
) -> GrantOpportunity {
    let mut notes = vec![
        "The weekly list gives no amount, eligibility, or description: read the solicitation on \
         COMMBUYS before deciding."
            .to_string(),
    ];
    if kind == "purchase_or_contract" {
        notes.push("Looks like a purchase or service contract, not a grant.".into());
    }
    if health_agency {
        notes.push("Issued by a health or human services organization.".into());
    }
    GrantOpportunity {
        source: "commbuys".into(),
        id: b.bid_number.clone(),
        title: b.description.clone(),
        agency: b.organization.clone(),
        kind: kind.into(),
        closes: b.bid_date.clone(),
        close_time: Some(b.bid_time.clone()),
        days_left: b.bid_date.as_deref().and_then(|d| days_between(today, d)),
        award_floor: None,
        award_ceiling: None,
        total_funding: None,
        eligible_applicants: Vec::new(),
        eligibility_stated: false,
        topics,
        summary: None,
        how_to_find: format!("Search COMMBUYS for bid number {}.", b.bid_number),
        notes,
    }
}

/// Search grant and bid listings.
pub async fn search_grant_opportunities(
    ctx: &Context,
    args: GrantSearchArgs,
) -> Result<Envelope<GrantSearchResult>> {
    search_at(ctx, args, chrono::Local::now().date_naive()).await
}

/// [`search_grant_opportunities`] with the date to count days from (for tests).
pub async fn search_at(
    ctx: &Context,
    args: GrantSearchArgs,
    today: NaiveDate,
) -> Result<Envelope<GrantSearchResult>> {
    let c: SourcesContent = content::load(GRANT_SOURCES)?;
    let topic = args
        .topic
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if let Some(t) = topic {
        if t.chars().count() > grants::MAX_QUERY_CHARS {
            return Err(PhnError::InvalidInput(format!(
                "topic must be at most {} characters: name the subject in a few words",
                grants::MAX_QUERY_CHARS
            )));
        }
        if phi::scan_text(t)
            .iter()
            .any(|f| f.confidence == phi::Confidence::High)
        {
            return Err(PhnError::Privacy(
                "the topic looks like it contains a personal identifier (a name, number, date of \
                 birth, or contact detail). Searches are sent to outside services: use plain \
                 subject words such as 'immunization' or 'overdose prevention'."
                    .into(),
            ));
        }
    }
    let elig = args.eligibility.unwrap_or_default();
    let source = args.source.unwrap_or_default();
    let after = match &args.closes_after {
        Some(s) => parse_day("closes_after", s)?,
        None => today,
    };
    let mut before = match &args.closes_before {
        Some(s) => Some(parse_day("closes_before", s)?),
        None => None,
    };
    if let Some(n) = args.within_days {
        let b = today + chrono::Duration::days(i64::from(n.min(3660)));
        before = Some(before.map_or(b, |x| x.min(b)));
    }
    if let Some(b) = before
        && b < after
    {
        return Err(PhnError::InvalidInput(
            "the closing window ends before it starts".into(),
        ));
    }
    if let (Some(lo), Some(hi)) = (args.min_award, args.max_award)
        && lo > hi
    {
        return Err(PhnError::InvalidInput(
            "min_award is larger than max_award".into(),
        ));
    }
    let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let include_forecasted = args.include_forecasted.unwrap_or(false);
    let grants_only = args.grants_only.unwrap_or(false);

    let (topic_kws, topic_def) = match topic {
        Some(t) => {
            let (k, d) = resolve_topic(t, &c.topics);
            (Some(k), d)
        }
        None => (None, None),
    };
    let elig_def = c
        .eligibility
        .iter()
        .find(|e| e.id == elig.id())
        .ok_or_else(|| PhnError::Internal("eligibility mapping missing".into()))?
        .clone();

    let mut filters_applied = vec![format!("closing on or after {after}")];
    if let Some(b) = before {
        filters_applied.push(format!("closing on or before {b}"));
    }
    if let Some(t) = topic {
        filters_applied.push(format!("topic: {t}"));
    }
    if elig != GrantEligibility::Any {
        filters_applied.push(format!("federal eligibility: {}", elig_def.label));
    }
    if let Some(v) = args.min_award {
        filters_applied.push(format!("federal award ceiling of at least {}", money(v)));
    }
    if let Some(v) = args.max_award {
        filters_applied.push(format!("federal award floor of at most {}", money(v)));
    }
    if grants_only {
        filters_applied.push("COMMBUYS: grant-like titles only".into());
    }
    let mut filters_not_applied: Vec<String> = Vec::new();
    let mut opportunities: Vec<GrantOpportunity> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    let mut federal_status = None;
    let mut commbuys_status = None;
    let mut already_closed = 0usize;

    // ---- federal
    if source != GrantSource::Commbuys {
        let q = FederalQuery {
            query: topic_kws.as_ref().map(|k| {
                if topic_def.is_some() {
                    k.iter()
                        .skip(1)
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    k[0].clone()
                }
            }),
            any_word: topic_def.is_some(),
            applicant_types: if elig == GrantEligibility::Any {
                Vec::new()
            } else {
                elig_def.applicant_types.clone()
            },
            funding_categories: topic_def
                .map(|d| d.funding_categories.clone())
                .unwrap_or_else(|| vec!["health".into()]),
            statuses: if include_forecasted {
                vec!["posted".into(), "forecasted".into()]
            } else {
                vec!["posted".into()]
            },
            close_from: if include_forecasted {
                None
            } else {
                Some(after)
            },
            close_to: if include_forecasted { None } else { before },
            award_ceiling_min: args.min_award,
            award_floor_max: args.max_award,
            agency: None,
            page_size: 100,
        };
        let outcome = if ctx.http.is_offline() {
            FederalOutcome::Failed("offline mode is on (PHN_OFFLINE)".into())
        } else {
            grants::search_federal(&ctx.http, &q).await
        };
        match outcome {
            FederalOutcome::Live(r) => {
                let read = r.opportunities.len();
                let mut rows: Vec<GrantOpportunity> = Vec::new();
                for o in &r.opportunities {
                    if o.status != "posted" && !(include_forecasted && o.status == "forecasted") {
                        continue;
                    }
                    if let Some(d) = o
                        .close_date
                        .as_deref()
                        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                    {
                        if d < after || before.is_some_and(|b| d > b) {
                            continue;
                        }
                    } else if !include_forecasted {
                        // A posted listing with no closing date cannot be placed in the window.
                    }
                    if let Some(lo) = args.min_award
                        && o.award_ceiling.is_some_and(|x| x < lo)
                    {
                        continue;
                    }
                    if let Some(hi) = args.max_award
                        && o.award_floor.is_some_and(|x| x > hi)
                    {
                        continue;
                    }
                    let stated = !o.applicant_types.is_empty();
                    if elig != GrantEligibility::Any
                        && stated
                        && !o
                            .applicant_types
                            .iter()
                            .any(|t| elig_def.applicant_types.contains(t))
                    {
                        continue;
                    }
                    let text = padded(&format!(
                        "{} {}",
                        o.title,
                        o.summary.clone().unwrap_or_default()
                    ));
                    rows.push(federal_row(o, today, stated, topics_of(&text, &c.topics)));
                }
                rows.sort_by(|a, b| {
                    (a.closes.is_none(), a.closes.clone())
                        .cmp(&(b.closes.is_none(), b.closes.clone()))
                });
                federal_status = Some(SourceStatus {
                    source: "federal".into(),
                    mode: "live".into(),
                    as_of: today.to_string(),
                    reason: None,
                    matched: rows.len(),
                    read,
                });
                if let Some(t) = r.total_records
                    && t as usize > read
                {
                    caveats.push(format!(
                        "The federal search reported {t} matches and returned the first {read}; narrow the topic or the dates to see the rest."
                    ));
                }
                sources.push(grants::federal_source());
                opportunities.extend(rows);
            }
            FederalOutcome::NoKey => {
                federal_status = Some(SourceStatus {
                    source: "federal".into(),
                    mode: "snapshot".into(),
                    as_of: today.to_string(),
                    reason: Some(
                        "No Simpler.Grants.gov key is set (PHN_GRANTS_GOV_API_KEY), so the live federal search was not run. The bundled snapshot has no federal listings because Grants.gov could not be reached when it was built."
                            .into(),
                    ),
                    matched: 0,
                    read: 0,
                });
            }
            FederalOutcome::Failed(why) => {
                federal_status = Some(SourceStatus {
                    source: "federal".into(),
                    mode: "snapshot".into(),
                    as_of: today.to_string(),
                    reason: Some(format!(
                        "The live federal search failed ({why}). The bundled snapshot has no federal listings because Grants.gov could not be reached when it was built."
                    )),
                    matched: 0,
                    read: 0,
                });
            }
        }
    }

    // ---- COMMBUYS
    if source != GrantSource::Federal {
        let read = read_commbuys(ctx, today).await?;
        let total = read.list.bids.len();
        let mut rows: Vec<GrantOpportunity> = Vec::new();
        for b in &read.list.bids {
            let Some(d) = b
                .bid_date
                .as_deref()
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            else {
                continue;
            };
            if d < today {
                already_closed += 1;
            }
            if d < after || before.is_some_and(|x| d > x) {
                continue;
            }
            let text = padded(&format!("{} {}", b.organization, b.description));
            let health_agency = any_phrase(&padded(&b.organization), &c.health_agency_keywords);
            let topic_ids = topics_of(&text, &c.topics);
            let included = match (&topic_kws, topic) {
                (Some(kws), Some(t)) => any_phrase(&text, kws) || free_topic_matches(&text, t),
                _ => health_agency || !topic_ids.is_empty(),
            };
            if !included {
                continue;
            }
            let kind = commbuys_kind(&padded(&b.description), &c);
            if grants_only && kind != "grant_or_funding" {
                continue;
            }
            rows.push(commbuys_row(b, today, kind, topic_ids, health_agency));
        }
        let age = NaiveDate::parse_from_str(&read.list.list_date, "%Y-%m-%d")
            .ok()
            .map(|d| (today - d).num_days());
        if read.mode == SourceMode::Snapshot {
            caveats.push(format!(
                "The COMMBUYS list is the bundled copy dated {}{}: bids posted since then are not shown. Open the current weekly list on mass.gov or search COMMBUYS.",
                read.list.list_date,
                age.map(|a| format!(" ({a} days old)")).unwrap_or_default()
            ));
        }
        commbuys_status = Some(SourceStatus {
            source: "commbuys".into(),
            mode: mode_word(read.mode).into(),
            as_of: read.as_of.clone(),
            reason: read.reason.clone(),
            matched: rows.len(),
            read: total,
        });
        sources.push(read.source.clone());
        opportunities.extend(rows);
    }

    // Notes on filters that cannot apply to every listing.
    if source != GrantSource::Federal {
        if elig != GrantEligibility::Any {
            filters_not_applied.push("Eligibility: COMMBUYS listings do not say who may respond; each is flagged to read the solicitation.".into());
        }
        if args.min_award.is_some() || args.max_award.is_some() {
            filters_not_applied.push("Award amount: COMMBUYS listings state no amount; they are shown unfiltered and flagged.".into());
        }
    }
    if !elig_def.note.is_empty() && source != GrantSource::Commbuys && elig != GrantEligibility::Any
    {
        caveats.push(format!(
            "Eligibility mapping for {}: {}",
            elig_def.label, elig_def.note
        ));
    }

    opportunities.sort_by(|a, b| {
        (a.closes.is_none(), a.closes.clone(), a.id.clone()).cmp(&(
            b.closes.is_none(),
            b.closes.clone(),
            b.id.clone(),
        ))
    });
    let matched_total = opportunities.len();
    let more_matches = matched_total.saturating_sub(limit);
    opportunities.truncate(limit);

    let directory: Vec<DirectoryOut> = c
        .directory
        .iter()
        .map(|d| DirectoryOut {
            name: d.name.clone(),
            level: d.level.clone(),
            url: d.url.clone(),
            what: d.what.clone(),
            how_to_use: d.how_to_use.clone(),
            reached_when_built: d.reached,
            confidence: d.confidence,
        })
        .collect();
    let notes_out = content::cite_all(&c.massachusetts_notes, &c.sources);
    let reach_log: Vec<String> = c
        .reach_log
        .iter()
        .map(|r| format!("{}: {} ({})", r.host, r.result, r.used_for))
        .collect();

    let result = GrantSearchResult {
        federal: federal_status,
        commbuys: commbuys_status,
        filters_applied,
        filters_not_applied,
        opportunities,
        more_matches,
        already_closed,
        topics_available: c
            .topics
            .iter()
            .map(|t| format!("{} ({})", t.id, t.label))
            .collect(),
        where_to_look: directory,
        how_to_search: c.how_to_search.clone(),
        massachusetts_notes: notes_out,
        reach_log,
    };

    let summary = summary_markdown(&result, matched_total);
    let mut keys: Vec<&str> = c.directory.iter().map(|d| d.source.as_str()).collect();
    keys.extend(c.massachusetts_notes.iter().map(|n| n.source.as_str()));
    // Only the sources that were used for the directory and notes are cited from content.
    sources.extend(content::sources_for(keys, &c.sources));
    caveats.push(
        "Listings change daily and a match is a lead, not a determination: read the full announcement, its eligibility section, and any amendments before you plan an application."
            .into(),
    );
    caveats.extend(content::review_caveats(&[GRANT_SOURCES]));
    Ok(Envelope::new(summary, result)
        .sources(sources)
        .caveats(caveats)
        .privacy(Privacy {
            notes: vec![
                "Only the topic words and filters you gave are sent to outside services; nothing about a client, a family, or your agency is included, and the API key (if you set one) is never returned or cached."
                    .into(),
            ],
            ..Privacy::none()
        })
        .decision_support())
}

fn summary_markdown(r: &GrantSearchResult, matched_total: usize) -> String {
    let mut s = String::from("**Grant and bid opportunities**\n\n");
    for st in [&r.federal, &r.commbuys].into_iter().flatten() {
        let name = if st.source == "federal" {
            "Federal (Simpler.Grants.gov)"
        } else {
            "Massachusetts (COMMBUYS weekly list)"
        };
        s.push_str(&format!(
            "- **{name}: mode {}**, data as of {}; {} matched your filters{}.{}\n",
            st.mode,
            st.as_of,
            st.matched,
            if st.read > 0 {
                format!(" of {} read", st.read)
            } else {
                String::new()
            },
            st.reason
                .as_ref()
                .map(|x| format!(" {x}"))
                .unwrap_or_default()
        ));
    }
    s.push_str(&format!("\nFilters: {}.\n", r.filters_applied.join("; ")));
    for f in &r.filters_not_applied {
        s.push_str(&format!("\n- Not applied: {f}"));
    }
    if r.opportunities.is_empty() {
        s.push_str("\n\nNo listings matched. ");
        s.push_str("Try a broader topic, a later closing date, or the places listed under where_to_look.\n");
    } else {
        s.push_str(&format!(
            "\n\n{} of {} matching listings (soonest closing first):\n\n",
            r.opportunities.len(),
            matched_total
        ));
        let rows: Vec<Vec<String>> = r
            .opportunities
            .iter()
            .map(|o| {
                vec![
                    o.closes
                        .clone()
                        .map(|d| match o.days_left {
                            Some(n) => format!("{d} ({n} d)"),
                            None => d,
                        })
                        .unwrap_or_else(|| "not stated".into()),
                    o.source.clone(),
                    md::cell(&o.id),
                    md::cell(&o.title),
                    md::cell(&o.agency),
                    match (o.award_floor, o.award_ceiling) {
                        (Some(a), Some(b)) => format!("{} to {}", money(a), money(b)),
                        (None, Some(b)) => format!("up to {}", money(b)),
                        (Some(a), None) => format!("from {}", money(a)),
                        _ => "not stated".into(),
                    },
                    o.kind.replace('_', " "),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Closes", "Source", "ID", "Title", "Agency", "Award", "Kind"],
            &rows,
        ));
        s.push('\n');
    }
    if r.already_closed > 0 {
        s.push_str(&format!(
            "\n{} bids on the COMMBUYS list had already closed.\n",
            r.already_closed
        ));
    }
    s.push_str("\n**Where else to look:** ");
    s.push_str(
        &r.where_to_look
            .iter()
            .map(|d| format!("{} ({})", d.name, d.url))
            .collect::<Vec<_>>()
            .join("; "),
    );
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: (i32, u32, u32) = (2026, 11, 12);

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(TODAY.0, TODAY.1, TODAY.2).unwrap()
    }

    fn offline_ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let mut cfg = phn_core::Config::for_root(dir.path());
        cfg.offline = true;
        let c = Context::new(cfg).unwrap();
        (dir, c)
    }

    #[test]
    fn weekly_text_in_the_pdf_readers_layout_is_parsed() {
        let text = "New Bids  Available  Monday ,  November  9, 2026   \nOrg  ID  \nOrganization  \nBid  Date  \nBid  Time  \nDescription  \nBid  Number  \n\
1669  Town of Tolland  \nNovember 12,  \n2026\n \n9:00 AM  \nForming and Pouring a concrete pad at the Tolland Transfer  \nStation \n \nBD-27-1669-A1-A1-134841 \n\
1666  Town of Sandisfield  \nNovember 12,  \n2026\n \n2:00 PM  S. Beech Plain  -  E3400  BD-27-1666-TSTH -TSTH -135057 \n\
1020  \nDepartment of Conservation  \nand Recreation \n \nNovember 17,  \n2026\n \n5:00 PM  PRF86 Conference Room Space  BD-27-1020-DCRFS -DC367 -135073  \n\
1208  Massachusetts Convention  \nCenter Authority 2027 \nJanuary 11,  \n12:00 PM  Master Planner  BD-27-1208-1208C -1208L -134564 \n";
        let list = parse_weekly_text(text).unwrap();
        assert_eq!(list.list_date, "2026-11-09");
        assert_eq!(list.bids.len(), 4);
        assert_eq!(list.bids[0].organization, "Town of Tolland");
        assert_eq!(list.bids[0].bid_date.as_deref(), Some("2026-11-12"));
        assert_eq!(list.bids[0].bid_time, "9:00 AM");
        assert_eq!(
            list.bids[0].description,
            "Forming and Pouring a concrete pad at the Tolland Transfer Station"
        );
        assert_eq!(list.bids[1].bid_number, "BD-27-1666-TSTH-TSTH-135057");
        assert_eq!(
            list.bids[2].organization,
            "Department of Conservation and Recreation"
        );
        assert_eq!(list.bids[3].bid_date.as_deref(), Some("2027-01-11"));
        assert_eq!(
            list.bids[3].organization,
            "Massachusetts Convention Center Authority"
        );
        // A page that is not the list is rejected, not guessed at.
        assert!(parse_weekly_text("<html>Access denied</html>").is_err());
        let broken = text.replace("November 17,", "Nov. 17");
        assert!(parse_weekly_text(&broken).is_err());
    }

    #[test]
    fn content_loads() {
        let c: SourcesContent = content::load(GRANT_SOURCES).unwrap();
        assert!(c.topics.len() >= 8);
        assert!(c.eligibility.iter().any(|e| e.id == "local_health_agency"));
        assert!(c.directory.iter().any(|d| d.id == "dph-rfrs"));
        let m = content::meta(GRANT_SOURCES).unwrap();
        assert_eq!(m.review.review_due, "2027-02-01");
    }

    #[tokio::test]
    async fn offline_search_uses_both_snapshots_and_says_so() {
        let (_d, c) = offline_ctx();
        let r = search_at(&c, GrantSearchArgs::default(), day())
            .await
            .unwrap();
        let fed = r.data.federal.as_ref().unwrap();
        assert_eq!(fed.mode, "snapshot");
        assert_eq!(fed.matched, 0);
        assert!(
            fed.reason
                .as_deref()
                .unwrap()
                .contains("PHN_GRANTS_GOV_API_KEY")
                || fed.reason.as_deref().unwrap().contains("offline")
        );
        let cb = r.data.commbuys.as_ref().unwrap();
        assert_eq!(cb.mode, "snapshot");
        assert!(r.summary.contains("mode snapshot"));
        // The EOHHS request for response closes 2026-12-11 and is on the health list.
        let rfr = r
            .data
            .opportunities
            .iter()
            .find(|o| o.id == "BD-27-1039-EHS01-ASHWA-135088")
            .expect("EOHHS RFR");
        assert_eq!(rfr.closes.as_deref(), Some("2026-12-11"));
        assert_eq!(rfr.days_left, Some(29));
        assert_eq!(rfr.kind, "response_request");
        assert!(!rfr.eligibility_stated);
        // Soonest first.
        let dates: Vec<_> = r
            .data
            .opportunities
            .iter()
            .filter_map(|o| o.closes.clone())
            .collect();
        let mut sorted = dates.clone();
        sorted.sort();
        assert_eq!(dates, sorted);
        assert!(
            r.caveats
                .iter()
                .any(|x| x.contains("bundled copy dated 2026-11-09"))
        );
        assert!(
            r.caveats
                .iter()
                .any(|x| x.contains("not yet been reviewed"))
        );
        assert!(
            r.sources
                .iter()
                .any(|s| s.title.contains("COMMBUYS") && s.mode == SourceMode::Snapshot)
        );
    }

    #[tokio::test]
    async fn filters_topic_window_and_grants_only() {
        let (_d, c) = offline_ctx();
        let a = GrantSearchArgs {
            source: Some(GrantSource::Commbuys),
            grants_only: Some(true),
            ..Default::default()
        };
        let r = search_at(&c, a, day()).await.unwrap();
        assert!(!r.data.opportunities.is_empty());
        assert!(
            r.data
                .opportunities
                .iter()
                .all(|o| o.kind == "grant_or_funding")
        );
        // A closing window.
        let a = GrantSearchArgs {
            source: Some(GrantSource::Commbuys),
            closes_after: Some("2026-12-01".into()),
            closes_before: Some("2026-12-31".into()),
            ..Default::default()
        };
        let r = search_at(&c, a, day()).await.unwrap();
        assert!(r.data.opportunities.iter().all(|o| {
            let d = o.closes.as_deref().unwrap();
            ("2026-12-01"..="2026-12-31").contains(&d)
        }));
        // A free-text topic matches title words.
        let a = GrantSearchArgs {
            source: Some(GrantSource::Commbuys),
            topic: Some("mosquito control".into()),
            closes_after: Some("2030-01-01".into()),
            ..Default::default()
        };
        let r = search_at(&c, a, day()).await.unwrap();
        assert_eq!(r.data.opportunities.len(), 1);
        assert!(r.data.opportunities[0].title.contains("Mosquito"));
        // An amount filter cannot apply to COMMBUYS and says so.
        let a = GrantSearchArgs {
            source: Some(GrantSource::Commbuys),
            min_award: Some(50_000),
            ..Default::default()
        };
        let r = search_at(&c, a, day()).await.unwrap();
        assert!(
            r.data
                .filters_not_applied
                .iter()
                .any(|f| f.contains("Award amount"))
        );
        assert!(r.data.federal.is_none());
    }

    #[tokio::test]
    async fn bad_input_is_a_tool_error() {
        let (_d, c) = offline_ctx();
        let bad = |a: GrantSearchArgs| {
            let c = &c;
            async move { search_at(c, a, day()).await.unwrap_err().to_string() }
        };
        assert!(
            bad(GrantSearchArgs {
                closes_after: Some("13/45/2026".into()),
                ..Default::default()
            })
            .await
            .contains("closes_after")
        );
        assert!(
            bad(GrantSearchArgs {
                closes_after: Some("1850-01-01".into()),
                ..Default::default()
            })
            .await
            .contains("1900")
        );
        assert!(
            bad(GrantSearchArgs {
                closes_after: Some("2027-01-01".into()),
                closes_before: Some("2026-12-01".into()),
                ..Default::default()
            })
            .await
            .contains("ends before")
        );
        assert!(
            bad(GrantSearchArgs {
                min_award: Some(10),
                max_award: Some(5),
                ..Default::default()
            })
            .await
            .contains("min_award")
        );
        assert!(
            bad(GrantSearchArgs {
                topic: Some("x".repeat(101)),
                ..Default::default()
            })
            .await
            .contains("100")
        );
        let e = bad(GrantSearchArgs {
            topic: Some("diabetes for SSN 123-45-6789".into()),
            ..Default::default()
        })
        .await;
        assert!(e.contains("identifier") && !e.contains("123-45"));
    }

    #[test]
    fn phrase_matching_uses_whole_words() {
        let t = padded("Annual Contract for Trash & Recycling");
        assert!(has_phrase(&t, "trash"));
        assert!(!has_phrase(&t, "rash"));
        assert!(has_phrase(&t, "Recycling"));
        assert!(free_topic_matches(&t, "recycling trash"));
        assert!(!free_topic_matches(&t, "recycling vaccine"));
        assert_eq!(money(1234567), "$1,234,567");
        assert_eq!(money(950), "$950");
    }
}
