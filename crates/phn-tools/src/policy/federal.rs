//! POL-03 `monitor_federal_rules`: watches the Federal Register for proposed
//! rules, final rules, and notices from CDC, CMS, HRSA, EPA, USDA, HHS (and a
//! few more agencies) with comment deadlines counted down against today's
//! date, optionally cross-checks Regulations.gov (a free api.data.gov key in
//! `PHN_REGULATIONS_GOV_API_KEY`), and drafts a public comment outline as a
//! Word document with `[NURSE: ...]` placeholders.
//!
//! Live data only: when neither the Federal Register nor a recent cached
//! search can be reached the tool says so and returns no documents; it never
//! presents old data as current. The comment outline arranges the tips EPA
//! publishes for effective comments; it is a template, not an agency form.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::fedreg::{self, Document, Query, RegsDocument};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, DocSource};
use crate::md;
use crate::policy::legislation::people_guard;
use crate::policy::shared::{clean_text, refuse_identifiers, write_docx};

/// Content id of `content/policy/federal_rules_tracking.yaml`.
pub const CONTENT: &str = "policy-federal-rules-tracking";

const DEFAULT_RESULTS: usize = 15;
const MAX_RESULTS: usize = 50;
const MAX_REGS_LOOKUPS: usize = 10;
const MAX_ABSTRACT_CHARS: usize = 500;
const MAX_FILL_CHARS: usize = 2000;
const MAX_FILL_ROWS: usize = 60;
const CLOSING_SOON_DAYS: i64 = 7;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgencyDef {
    pub id: String,
    pub label: String,
    pub slug: String,
    pub verified: bool,
    pub confidence: content::Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocTypeDef {
    pub id: String,
    pub code: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TipDef {
    pub id: String,
    pub quote: String,
    pub paraphrase: String,
    pub source: String,
    pub section: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceholderDef {
    pub field: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableDef {
    pub columns: Vec<String>,
    pub rows: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlineDef {
    pub id: String,
    pub heading: String,
    pub tips: Vec<String>,
    pub guidance: String,
    #[serde(default)]
    pub placeholders: Vec<PlaceholderDef>,
    #[serde(default)]
    pub checklist: Vec<String>,
    #[serde(default)]
    pub table: Option<TableDef>,
}

/// `content/policy/federal_rules_tracking.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FedContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub agencies: Vec<AgencyDef>,
    pub default_agencies: Vec<String>,
    pub document_types: Vec<DocTypeDef>,
    pub tips: Vec<TipDef>,
    pub outline: Vec<OutlineDef>,
    pub countdown_note: String,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `monitor_federal_rules`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct FederalArgs {
    /// Agencies to watch, by id: cdc, cms, hrsa, epa, usda, hhs, fda, samhsa, acf, fns, fsis.
    /// Default: cdc, cms, hrsa, epa, usda, hhs.
    pub agencies: Vec<String>,
    /// Federal Register agency slugs to use instead of or in addition to the ids (for example
    /// "food-and-drug-administration"). Use this if an id finds nothing.
    pub agency_slugs: Vec<String>,
    /// Document types: proposed_rule, final_rule, notice. Default: all types (comment periods are
    /// counted for any document that has one).
    pub document_types: Vec<String>,
    /// A topic to search for in the document text (for example "lead", "school meals").
    pub topic: Option<String>,
    /// Published on or after this date (YYYY-MM-DD). With open_for_comment_only false the default
    /// is 30 days before today.
    pub published_since: Option<String>,
    /// Published on or before this date (YYYY-MM-DD).
    pub published_until: Option<String>,
    /// Show only documents whose comment period has not closed (default true). False lists
    /// everything published in the date range, with closed comment periods marked.
    pub open_for_comment_only: Option<bool>,
    /// Most documents returned (default 15, at most 50).
    pub max_results: Option<u32>,
    /// Look at one Federal Register document by its number (for example 2026-12345).
    pub document_number: Option<String>,
    /// Cross-check open comment periods on Regulations.gov (default true; needs the optional
    /// PHN_REGULATIONS_GOV_API_KEY in the server's environment; skipped when it is not set).
    pub check_regulations_gov: Option<bool>,
    /// Write a comment outline (Word) in your output folder for `document_number`, or for the
    /// document you describe with document_title and comment_deadline (default false).
    pub draft_comment_outline: Option<bool>,
    /// For the outline when the live data is not available: the document's title.
    pub document_title: Option<String>,
    /// For the outline or a countdown without live data: the comment deadline (YYYY-MM-DD).
    pub comment_deadline: Option<String>,
    /// For the outline: the docket ID (for example EPA-HQ-OW-2026-0001).
    pub docket_id: Option<String>,
    /// For the outline: the agency's name.
    pub agency_name: Option<String>,
    /// A CSV or XLSX in your data folder with columns `field` and `value` to fill parts of the
    /// outline: organization, population, position, reasons, evidence, local_data, cost_estimate,
    /// community_effects, department_effects, opposing_view, agency_questions, docket_id,
    /// fr_citation. Values go only into the Word file, never back to the AI. Refused if the file
    /// looks like a list of people or holds identifiers.
    pub fill_path: Option<String>,
    /// Date to use for "today" (YYYY-MM-DD, 1900-2100). Default: today.
    pub as_of: Option<String>,
}

/// A comment period counted against today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Countdown {
    /// "open", "closing_soon" (7 days or fewer), "closes_today", "closed", or "no_comment_period".
    pub status: String,
    /// Calendar days from today to the close date (negative after it closed).
    pub days_remaining: Option<i64>,
    pub label: String,
}

/// A Regulations.gov record for a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegsOut {
    pub document_id: String,
    pub docket_id: Option<String>,
    pub open_for_comment: Option<bool>,
    pub withdrawn: Option<bool>,
    /// The comment end time as Regulations.gov gives it (UTC); read the Federal Register document
    /// for the deadline's time zone.
    pub comment_end_utc: Option<String>,
    pub comment_page: String,
}

/// One Federal Register document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DocOut {
    pub document_number: String,
    pub title: String,
    /// "Proposed Rule", "Rule", "Notice", and so on.
    pub doc_type: String,
    pub agencies: Vec<String>,
    /// The document's abstract, cut at 500 characters.
    pub abstract_text: Option<String>,
    pub publication_date: Option<String>,
    pub comments_close_on: Option<String>,
    pub countdown: Countdown,
    pub effective_on: Option<String>,
    pub docket_ids: Vec<String>,
    pub html_url: Option<String>,
    pub pdf_url: Option<String>,
    pub comment_url: Option<String>,
    pub regulations_gov: Option<RegsOut>,
}

/// An agency searched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgencyOut {
    pub id: String,
    pub label: String,
    pub slug: String,
    /// "high", "medium", or "low": how sure the project is that the slug is the Federal Register's.
    pub slug_confidence: String,
}

/// What was asked of the Federal Register.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueryOut {
    pub agencies: Vec<AgencyOut>,
    pub document_types: Vec<String>,
    pub topic: Option<String>,
    pub published_since: Option<String>,
    pub published_until: Option<String>,
    pub comment_period_open_on_or_after: Option<String>,
}

/// How the Regulations.gov cross-check went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegsStatus {
    pub used: bool,
    pub note: String,
    pub checked: usize,
    pub failed: usize,
}

/// A tip for effective comments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TipOut {
    /// A quotation of 25 words or fewer.
    pub quote: String,
    pub paraphrase: String,
    pub citation: String,
}

/// The comment outline written to the output folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutlineOut {
    /// The Word file in your output folder.
    pub file: String,
    pub document_title: String,
    pub agency: Option<String>,
    pub docket_id: Option<String>,
    pub comment_deadline: Option<String>,
    pub countdown: Countdown,
    pub sections: Vec<String>,
    /// How many of your `fill_path` values were placed in the file (the values are not shown).
    pub fields_filled: usize,
    /// Field names in your file that the outline does not have.
    pub unrecognized_fields: Vec<String>,
    pub tips: Vec<TipOut>,
}

/// Result of `monitor_federal_rules`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FederalResult {
    /// "live", "cache" (a recent search served from the cache), "cache_after_failure" (live request
    /// failed; a cached search of at most two days ago is shown), or "unavailable".
    pub data_status: String,
    /// When the documents were read (YYYY-MM-DD), when any were.
    pub as_of: Option<String>,
    pub status_note: String,
    pub today: String,
    pub query: QueryOut,
    /// Documents matching in the Federal Register (more than were returned when this is larger).
    pub total_matches: u64,
    pub documents: Vec<DocOut>,
    pub regulations_gov: RegsStatus,
    pub outline: Option<OutlineOut>,
    /// Where to look yourself, especially when the live data was unavailable.
    pub manual_links: Vec<String>,
    pub countdown_rule: String,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- logic (pure)

/// A comment period counted against `today`. The close date itself is still open.
pub fn countdown(close: Option<&str>, today: NaiveDate) -> Countdown {
    let Some(date) = close.and_then(|c| NaiveDate::parse_from_str(c, "%Y-%m-%d").ok()) else {
        return Countdown {
            status: "no_comment_period".into(),
            days_remaining: None,
            label: "no comment deadline listed".into(),
        };
    };
    if !(1900..=2100).contains(&date.year()) {
        return Countdown {
            status: "no_comment_period".into(),
            days_remaining: None,
            label: "no valid comment deadline".into(),
        };
    }
    let days = (date - today).num_days();
    let when = date.format("%A, %Y-%m-%d");
    let (status, label) = match days {
        0 => ("closes_today", format!("closes today ({when})")),
        1 => ("closing_soon", format!("1 day left (closes {when})")),
        2..=CLOSING_SOON_DAYS => ("closing_soon", format!("{days} days left (closes {when})")),
        d if d > CLOSING_SOON_DAYS => ("open", format!("{days} days left (closes {when})")),
        -1 => ("closed", format!("closed 1 day ago ({when})")),
        d => ("closed", format!("closed {} days ago ({when})", -d)),
    };
    Countdown {
        status: status.into(),
        days_remaining: Some(days),
        label,
    }
}

fn parse_date(label: &str, s: &str) -> Result<NaiveDate> {
    let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| PhnError::InvalidInput(format!("{label} '{s}' is not a date (YYYY-MM-DD)")))?;
    if !(1900..=2100).contains(&d.year()) {
        return Err(PhnError::InvalidInput(format!(
            "{label} must be a year from 1900 to 2100"
        )));
    }
    Ok(d)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{}…", t.trim_end())
    }
}

fn doc_out(d: &Document, today: NaiveDate, regs: Option<&RegsDocument>) -> DocOut {
    DocOut {
        document_number: d.document_number.clone(),
        title: d.title.clone(),
        doc_type: d.doc_type.clone(),
        agencies: d.agencies.iter().map(|a| a.name.clone()).collect(),
        abstract_text: d
            .abstract_text
            .as_deref()
            .map(|a| truncate_chars(a, MAX_ABSTRACT_CHARS)),
        publication_date: d.publication_date.clone(),
        comments_close_on: d.comments_close_on.clone(),
        countdown: countdown(d.comments_close_on.as_deref(), today),
        effective_on: d.effective_on.clone(),
        docket_ids: d.docket_ids.clone(),
        html_url: d.html_url.clone(),
        pdf_url: d.pdf_url.clone(),
        comment_url: d.comment_url.clone(),
        regulations_gov: regs.map(|r| RegsOut {
            document_id: r.document_id.clone(),
            docket_id: r.docket_id.clone(),
            open_for_comment: r.open_for_comment,
            withdrawn: r.withdrawn,
            comment_end_utc: r.comment_end.clone(),
            comment_page: r.comment_page.clone(),
        }),
    }
}

fn resolve_agency<'a>(c: &'a FedContent, raw: &str) -> Option<&'a AgencyDef> {
    let n = phn_core::text::normalize(raw);
    c.agencies
        .iter()
        .find(|a| a.id == n || phn_core::text::normalize(&a.label) == n)
}

fn resolve_type<'a>(c: &'a FedContent, raw: &str) -> Option<&'a DocTypeDef> {
    let n = phn_core::text::normalize(&raw.replace('_', " "));
    c.document_types.iter().find(|t| {
        phn_core::text::normalize(&t.id.replace('_', " ")) == n
            || t.code.to_lowercase() == n
            || phn_core::text::normalize(&t.label) == n
    })
}

fn valid_slug(s: &str) -> bool {
    (3..=80).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Markdown of the comment outline.
#[allow(clippy::too_many_arguments)]
pub fn outline_markdown(
    c: &FedContent,
    title: &str,
    agency: Option<&str>,
    docket: Option<&str>,
    fr_number: Option<&str>,
    deadline: Option<&str>,
    countdown: &Countdown,
    fills: &BTreeMap<String, String>,
) -> String {
    let get = |field: &str, default: &str| -> String {
        fills
            .get(field)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| default.to_string())
    };
    let mut s = format!("# Public comment outline: {title}\n\n");
    s.push_str("_Draft outline for the nurse to complete. Replace every [NURSE: ...] placeholder before submitting. Comments are usually posted publicly: include no information about individuals and nothing confidential._\n\n");
    s.push_str("| Item | Detail |\n|---|---|\n");
    s.push_str(&format!(
        "| Agency | {} |\n",
        md::cell(agency.unwrap_or("[NURSE: agency]"))
    ));
    s.push_str(&format!(
        "| Docket ID | {} |\n",
        md::cell(&get("docket_id", docket.unwrap_or("[NURSE: docket ID]")))
    ));
    s.push_str(&format!(
        "| Federal Register | {} |\n",
        md::cell(&get(
            "fr_citation",
            fr_number
                .map(|n| format!("document number {n}"))
                .as_deref()
                .unwrap_or("[NURSE: Federal Register document number, date, and page]")
        ))
    ));
    s.push_str(&format!(
        "| Comment deadline | {} |\n\n",
        match deadline {
            Some(d) => format!(
                "{d} ({}; check the document's DATES section for the time)",
                countdown.label
            ),
            None => "[NURSE: comment deadline from the Federal Register document]".into(),
        }
    ));
    for (i, sec) in c.outline.iter().enumerate() {
        s.push_str(&format!("## {}. {}\n\n", i + 1, sec.heading));
        s.push_str(&format!("{}\n\n", sec.guidance));
        let tips: Vec<String> = sec
            .tips
            .iter()
            .filter_map(|id| c.tips.iter().find(|t| &t.id == id))
            .map(|t| {
                format!(
                    "\"{}\" ({})",
                    t.quote,
                    c.sources
                        .get(&t.source)
                        .map_or("EPA", |x| x.publisher.as_str())
                )
            })
            .collect();
        if !tips.is_empty() {
            s.push_str(&format!(
                "_From EPA's tips for effective comments: {}_\n\n",
                tips.join("; ")
            ));
        }
        for p in &sec.placeholders {
            let text = match fills
                .get(&p.field)
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
            {
                Some(v) => v.to_string(),
                None => p.text.clone(),
            };
            s.push_str(&format!("- {text}\n"));
        }
        if !sec.placeholders.is_empty() {
            s.push('\n');
        }
        if let Some(t) = &sec.table {
            let headers: Vec<&str> = t.columns.iter().map(String::as_str).collect();
            let rows: Vec<Vec<String>> = (0..t.rows)
                .map(|_| t.columns.iter().map(|_| "[NURSE: …]".to_string()).collect())
                .collect();
            s.push_str(&md::table(&headers, &rows));
            s.push('\n');
        }
        for item in &sec.checklist {
            s.push_str(&format!("- [ ] {item}\n"));
        }
        if !sec.checklist.is_empty() {
            s.push('\n');
        }
    }
    s.push_str(&format!(
        "---\n\nSource of the structure: {}. This outline is a template that arranges those tips; it is not an agency form, and the agency's own instructions in the Federal Register document come first.\n",
        c.sources.get("epa_tips").map_or("EPA", |x| x.title.as_str())
    ));
    s
}

/// Read a field and value fill file. Returns the fields and the names that are not outline fields.
fn read_fill(
    ctx: &Context,
    c: &FedContent,
    path: &str,
) -> Result<(BTreeMap<String, String>, Vec<String>, String)> {
    let lt = crate::localdata::load(ctx, path, None)?;
    people_guard(
        &lt,
        "an outline fill file",
        "A fill file has two columns: field and value.",
    )?;
    let (Some(fi), Some(vi)) = (
        lt.table.column_index("field"),
        lt.table.column_index("value"),
    ) else {
        return Err(PhnError::InvalidInput(format!(
            "'{}' needs columns named field and value. Columns found: {}",
            lt.label,
            lt.table.headers.join(", ")
        )));
    };
    if lt.table.rows.len() > MAX_FILL_ROWS {
        return Err(PhnError::InvalidInput(format!(
            "'{}' has more than {MAX_FILL_ROWS} rows; an outline fill file has one row per field",
            lt.label
        )));
    }
    let known: Vec<&str> = c
        .outline
        .iter()
        .flat_map(|o| o.placeholders.iter().map(|p| p.field.as_str()))
        .collect();
    let mut fills = BTreeMap::new();
    let mut unknown = Vec::new();
    for row in &lt.table.rows {
        let f = row
            .get(fi)
            .map(|s| s.trim().to_lowercase().replace(' ', "_"))
            .unwrap_or_default();
        let v = row
            .get(vi)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if f.is_empty() || v.is_empty() {
            continue;
        }
        if v.chars().count() > MAX_FILL_CHARS {
            return Err(PhnError::InvalidInput(format!(
                "a value in '{}' is longer than {MAX_FILL_CHARS} characters",
                lt.label
            )));
        }
        if known.contains(&f.as_str()) {
            fills.insert(f, v);
        } else {
            unknown.push(f.chars().take(40).collect());
        }
    }
    Ok((fills, unknown, lt.label.clone()))
}

fn search_link(topic: &Option<String>) -> String {
    match topic {
        Some(t) => format!(
            "https://www.federalregister.gov/documents/search?conditions%5Bterm%5D={}",
            phn_data::socrata::encode(t)
        ),
        None => "https://www.federalregister.gov/documents/search".to_string(),
    }
}

// ---------------------------------------------------------------- the tool

/// Run the tool.
pub async fn monitor_federal_rules(
    ctx: &Context,
    args: FederalArgs,
) -> Result<Envelope<FederalResult>> {
    monitor_federal_rules_at(ctx, args, fedreg::BASE, fedreg::REGULATIONS_BASE, None).await
}

/// [`monitor_federal_rules`] against other API base URLs, and with the Regulations.gov key given
/// instead of read from the environment (tests).
pub async fn monitor_federal_rules_at(
    ctx: &Context,
    args: FederalArgs,
    fr_base: &str,
    regs_base: &str,
    key_override: Option<String>,
) -> Result<Envelope<FederalResult>> {
    let c: FedContent = content::load(CONTENT)?;
    let today = match &args.as_of {
        Some(s) => parse_date("as_of", s)?,
        None => chrono::Local::now().date_naive(),
    };
    let open_only = args.open_for_comment_only.unwrap_or(true);
    let max = args
        .max_results
        .map(|m| (m as usize).clamp(1, MAX_RESULTS))
        .unwrap_or(DEFAULT_RESULTS);

    // ---- Arguments.
    let mut agencies: Vec<AgencyOut> = Vec::new();
    let ids: Vec<String> = if args.agencies.is_empty() && args.agency_slugs.is_empty() {
        c.default_agencies.clone()
    } else {
        args.agencies.clone()
    };
    for raw in ids.iter().filter(|a| !a.trim().is_empty()) {
        let a = resolve_agency(&c, raw).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{}' is not an agency id. Ids: {} (or give agency_slugs)",
                raw.chars().take(40).collect::<String>(),
                c.agencies
                    .iter()
                    .map(|a| a.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        agencies.push(AgencyOut {
            id: a.id.clone(),
            label: a.label.clone(),
            slug: a.slug.clone(),
            slug_confidence: format!("{:?}", a.confidence).to_lowercase(),
        });
    }
    for s in &args.agency_slugs {
        let s = s.trim().to_lowercase();
        if s.is_empty() {
            continue;
        }
        if !valid_slug(&s) {
            return Err(PhnError::InvalidInput(
                "an agency slug has lower-case letters, digits, and hyphens only (for example food-and-drug-administration)".into(),
            ));
        }
        agencies.push(AgencyOut {
            id: s.clone(),
            label: format!("{s} (slug you gave)"),
            slug: s,
            slug_confidence: "given".into(),
        });
    }
    agencies.dedup_by(|a, b| a.slug == b.slug);
    let mut types: Vec<&DocTypeDef> = Vec::new();
    for raw in args.document_types.iter().filter(|t| !t.trim().is_empty()) {
        let t = resolve_type(&c, raw).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{}' is not a document type. Types: {}",
                raw.chars().take(40).collect::<String>(),
                c.document_types
                    .iter()
                    .map(|t| t.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        if !types.iter().any(|x| x.id == t.id) {
            types.push(t);
        }
    }
    let topic = match &args.topic {
        Some(t) if !t.trim().is_empty() => {
            let t = clean_text("topic", t, 100)?;
            refuse_identifiers("topic", &t)?;
            Some(t)
        }
        _ => None,
    };
    let since = match &args.published_since {
        Some(s) => Some(parse_date("published_since", s)?),
        None if !open_only => Some(today - chrono::Duration::days(30)),
        None => None,
    };
    let until = match &args.published_until {
        Some(s) => Some(parse_date("published_until", s)?),
        None => None,
    };
    if let (Some(a), Some(b)) = (since, until)
        && a > b
    {
        return Err(PhnError::InvalidInput(
            "published_since is after published_until".into(),
        ));
    }
    let manual_deadline = match &args.comment_deadline {
        Some(s) if !s.trim().is_empty() => Some(parse_date("comment_deadline", s)?),
        _ => None,
    };
    let want_outline = args.draft_comment_outline.unwrap_or(false);
    let text_arg = |label: &str, v: &Option<String>, max: usize| -> Result<Option<String>> {
        match v {
            Some(t) if !t.trim().is_empty() => {
                let t = clean_text(label, t, max)?;
                refuse_identifiers(label, &t)?;
                Ok(Some(t))
            }
            _ => Ok(None),
        }
    };
    let manual_title = text_arg("document_title", &args.document_title, 300)?;
    let manual_docket = text_arg("docket_id", &args.docket_id, 60)?;
    let manual_agency = text_arg("agency_name", &args.agency_name, 120)?;
    if want_outline && args.document_number.is_none() && manual_title.is_none() {
        return Err(PhnError::InvalidInput(
            "to draft an outline give document_number, or document_title (and comment_deadline) for a document you describe".into(),
        ));
    }
    let comment_close_gte = if open_only && args.document_number.is_none() {
        Some(today.to_string())
    } else {
        None
    };
    let query_out = QueryOut {
        agencies: agencies.clone(),
        document_types: types.iter().map(|t| t.label.clone()).collect(),
        topic: topic.clone(),
        published_since: since.map(|d| d.to_string()),
        published_until: until.map(|d| d.to_string()),
        comment_period_open_on_or_after: comment_close_gte.clone(),
    };

    // ---- Live fetch.
    let mut docs: Vec<Document> = Vec::new();
    let mut total: u64 = 0;
    let mut status = "unavailable".to_string();
    let mut as_of: Option<String> = None;
    let mut status_note = String::new();
    let mut failure: Option<String> = None;
    let mut src_mode = SourceMode::Live;

    if let Some(n) = &args.document_number {
        match fedreg::fetch_document(&ctx.http, ctx.cache.as_ref(), fr_base, n).await {
            Ok((d, mode, at, reason)) => {
                docs.push(d);
                total = 1;
                (status, status_note, as_of, src_mode) = describe(mode, &at, reason);
            }
            Err(e @ PhnError::InvalidInput(_)) | Err(e @ PhnError::NotFound(_)) => return Err(e),
            Err(e) => failure = Some(e.to_string()),
        }
    } else {
        let q = Query {
            agencies: agencies.iter().map(|a| a.slug.clone()).collect(),
            types: types.iter().map(|t| t.code.clone()).collect(),
            term: topic.clone(),
            published_gte: since.map(|d| d.to_string()),
            published_lte: until.map(|d| d.to_string()),
            comments_close_gte: comment_close_gte.clone(),
            accepting_comments: false,
            per_page: max as u32,
        };
        match fedreg::search(&ctx.http, ctx.cache.as_ref(), fr_base, &q).await {
            Ok(r) => {
                total = r.page.count;
                docs = r.page.documents;
                (status, status_note, as_of, src_mode) =
                    describe(r.mode, &r.as_of, r.fallback_reason);
            }
            Err(e) => failure = Some(e.to_string()),
        }
    }
    if let Some(f) = &failure {
        status_note = format!(
            "LIVE DATA UNAVAILABLE: the Federal Register could not be reached ({}) and there is no recent cached search, so no documents are shown. Nothing here should be read as 'no new rules'. Check {} and https://www.regulations.gov directly.",
            f.trim_end_matches('.'),
            search_link(&topic)
        );
    }
    // Soonest deadline first among open comment periods; then newest.
    docs.sort_by(|a, b| {
        let key = |d: &Document| {
            d.comments_close_on
                .as_deref()
                .and_then(|c| NaiveDate::parse_from_str(c, "%Y-%m-%d").ok())
                .filter(|c| *c >= today)
        };
        match (key(a), key(b)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.publication_date.cmp(&a.publication_date),
        }
    });
    docs.truncate(max);

    // ---- Regulations.gov cross-check.
    let key = key_override.or_else(fedreg::regulations_key);
    let mut regs: BTreeMap<String, RegsDocument> = BTreeMap::new();
    let mut regs_status = RegsStatus {
        used: false,
        note: String::new(),
        checked: 0,
        failed: 0,
    };
    let want_regs = args.check_regulations_gov.unwrap_or(true);
    if !want_regs {
        regs_status.note = "Not requested.".into();
    } else if key.is_none() {
        regs_status.note = format!(
            "Skipped: no Regulations.gov key. Get a free api.data.gov key and set {} in the server's environment to cross-check open comment periods (the key is never shown or sent anywhere but Regulations.gov).",
            fedreg::REGULATIONS_KEY_ENV
        );
    } else if let Some(k) = &key {
        regs_status.used = true;
        for d in docs
            .iter()
            .filter(|d| {
                d.comments_close_on
                    .as_deref()
                    .and_then(|c| NaiveDate::parse_from_str(c, "%Y-%m-%d").ok())
                    .is_some_and(|c| c >= today)
            })
            .take(MAX_REGS_LOOKUPS)
        {
            match fedreg::regulations_lookup(
                &ctx.http,
                ctx.cache.as_ref(),
                regs_base,
                k,
                &d.document_number,
            )
            .await
            {
                Ok(Some(r)) => {
                    regs_status.checked += 1;
                    regs.insert(d.document_number.clone(), r);
                }
                Ok(None) => regs_status.checked += 1,
                Err(_) => regs_status.failed += 1,
            }
        }
        regs_status.note = format!(
            "Regulations.gov checked for {} open comment period(s) (at most {MAX_REGS_LOOKUPS}); {} lookup(s) failed.",
            regs_status.checked, regs_status.failed
        );
    }

    let documents: Vec<DocOut> = docs
        .iter()
        .map(|d| doc_out(d, today, regs.get(&d.document_number)))
        .collect();

    // ---- Outline.
    let mut outline: Option<OutlineOut> = None;
    let mut files_written: Vec<String> = Vec::new();
    let mut fill_label: Option<String> = None;
    if want_outline {
        let target = docs.first();
        let title = target
            .map(|d| d.title.clone())
            .or(manual_title.clone())
            .ok_or_else(|| {
                PhnError::Network(
                    "the Federal Register could not be reached, so the document's details are unknown; give document_title (and comment_deadline) to draft the outline anyway".into(),
                )
            })?;
        let agency = target
            .and_then(|d| {
                (!d.agencies.is_empty()).then(|| {
                    d.agencies
                        .iter()
                        .map(|a| a.name.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
            })
            .or(manual_agency.clone());
        let docket = target
            .and_then(|d| d.docket_ids.first().cloned())
            .or(manual_docket.clone());
        let deadline = target
            .and_then(|d| d.comments_close_on.clone())
            .or(manual_deadline.map(|d| d.to_string()));
        let cd = countdown(deadline.as_deref(), today);
        let (fills, unknown, label) = match &args.fill_path {
            Some(p) if !p.trim().is_empty() => read_fill(ctx, &c, p.trim())?,
            _ => (BTreeMap::new(), Vec::new(), String::new()),
        };
        if !label.is_empty() {
            fill_label = Some(label);
        }
        let md_text = outline_markdown(
            &c,
            &title,
            agency.as_deref(),
            docket.as_deref(),
            target.map(|d| d.document_number.as_str()),
            deadline.as_deref(),
            &cd,
            &fills,
        );
        let stem = docket
            .as_deref()
            .or(target.map(|d| d.document_number.as_str()))
            .unwrap_or("document")
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>();
        let file = write_docx(
            ctx,
            &format!("comment_outline_{stem}"),
            &format!("Public comment outline: {title}"),
            &md_text,
        )?;
        files_written.push(file.clone());
        outline = Some(OutlineOut {
            file,
            document_title: title,
            agency,
            docket_id: docket,
            comment_deadline: deadline,
            countdown: cd,
            sections: c.outline.iter().map(|o| o.heading.clone()).collect(),
            fields_filled: fills.len(),
            unrecognized_fields: unknown,
            tips: c
                .tips
                .iter()
                .map(|t| TipOut {
                    quote: t.quote.clone(),
                    paraphrase: t.paraphrase.clone(),
                    citation: format!(
                        "{}, {}",
                        c.sources
                            .get(&t.source)
                            .map_or(t.source.as_str(), |s| s.title.as_str()),
                        t.section
                    ),
                })
                .collect(),
        });
    }

    let data = FederalResult {
        data_status: status.clone(),
        as_of: as_of.clone(),
        status_note: status_note.clone(),
        today: today.to_string(),
        query: query_out,
        total_matches: total,
        documents,
        regulations_gov: regs_status,
        outline,
        manual_links: vec![
            search_link(&topic),
            "https://www.regulations.gov/search".into(),
            "https://www.federalregister.gov/documents/current".into(),
        ],
        countdown_rule: c.countdown_note.clone(),
        limits: c.limits.clone(),
    };

    let summary = summary_text(&data);
    let mut sources: Vec<Source> = Vec::new();
    if status != "unavailable" {
        sources.push(Source {
            title: "Federal Register API v1 (documents)".into(),
            publisher: "Office of the Federal Register, National Archives".into(),
            url: Some(format!("{fr_base}/documents.json")),
            effective_date: None,
            retrieved: as_of.clone(),
            mode: src_mode,
        });
    }
    if data.regulations_gov.used {
        sources.push(Source {
            title: "Regulations.gov API v4 (documents)".into(),
            publisher: "General Services Administration".into(),
            url: Some("https://api.regulations.gov/v4/documents".into()),
            effective_date: None,
            retrieved: Some(today.to_string()),
            mode: SourceMode::Live,
        });
    }
    if data.outline.is_some() {
        sources.push(c.sources["epa_tips"].to_source());
    }
    sources.push(c.sources["fr_code"].to_source());
    if let Some(f) = &fill_label {
        sources.push(Source::local_file(f.clone()));
    }
    sources.push(Source::computed(
        "Comment period countdown (calendar days from today to the close date)",
        "This server",
    ));

    let mut caveats = vec![
        "Decision support. The Federal Register document's own DATES and ADDRESSES sections state the deadline, its time of day, and how to comment; confirm them there. This tool does not file comments.".to_string(),
        c.countdown_note.clone(),
    ];
    match status.as_str() {
        "unavailable" => caveats.push(
            "LIVE DATA WAS UNAVAILABLE: no documents or deadlines could be listed. Do not treat the empty list as 'no open comment periods'.".into(),
        ),
        "cache_after_failure" => caveats.push(format!(
            "LIVE DATA WAS UNAVAILABLE: these documents come from a cached search read {}. New or changed documents since then are not shown; the countdowns are computed against today ({}).",
            as_of.clone().unwrap_or_default(),
            today
        )),
        _ => {}
    }
    if agencies.iter().any(|a| a.slug_confidence == "low") {
        caveats.push("Some agency slugs (CDC, CMS, HRSA, and others marked low) were not verified against the Federal Register; if an agency shows nothing, give agency_slugs.".into());
    }
    if data.total_matches > data.documents.len() as u64 && status != "unavailable" {
        caveats.push(format!(
            "{} documents matched; {} are shown (soonest comment deadline first). Narrow the agencies, document types, topic, or dates.",
            data.total_matches,
            data.documents.len()
        ));
    }
    if data.outline.is_some() {
        caveats.push("The comment outline follows the tips EPA publishes for effective comments; other agencies may ask for something different. It is a template with placeholders, not a draft position.".into());
    }
    caveats.extend(content::review_caveats(&[CONTENT]));

    let mut privacy = Privacy {
        row_level_data: false,
        files_written,
        notes: vec![
            "Requests to the Federal Register (and Regulations.gov) carry only agency names, document types, dates, and the topic words you gave; no personal data is sent. The Regulations.gov key, if set, is never returned.".into(),
        ],
        ..Privacy::none()
    };
    if fill_label.is_some() {
        privacy.notes.push("Your fill file's values were placed only in the Word file in your output folder; none are returned.".into());
    }
    Ok(Envelope::new(summary, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy)
        .decision_support())
}

/// Status, note, date, and source mode for a fetch result.
fn describe(
    mode: SourceMode,
    at: &str,
    reason: Option<String>,
) -> (String, String, Option<String>, SourceMode) {
    let date = phn_data::fetch::local_date(at);
    match (mode, reason) {
        (SourceMode::Live, _) => (
            "live".into(),
            format!("Live from the Federal Register API on {date}."),
            Some(date),
            SourceMode::Live,
        ),
        (_, Some(r)) => (
            "cache_after_failure".into(),
            format!(
                "LIVE DATA UNAVAILABLE ({}). Showing a cached search read on {date}; it may be missing newer documents.",
                r.trim_end_matches(';')
            ),
            Some(date),
            SourceMode::Cache,
        ),
        (_, None) => (
            "cache".into(),
            format!(
                "A recent search served from the cache (read {date}; searches are cached for 3 hours)."
            ),
            Some(date),
            SourceMode::Cache,
        ),
    }
}

fn summary_text(d: &FederalResult) -> String {
    let mut s = String::from("**Federal rules and notices**\n\n");
    s.push_str(&format!(
        "Data: **{}**. {}\n\n",
        d.data_status, d.status_note
    ));
    s.push_str(&format!(
        "Agencies: {}. Today is {}.\n\n",
        if d.query.agencies.is_empty() {
            "none".to_string()
        } else {
            d.query
                .agencies
                .iter()
                .map(|a| a.label.clone())
                .collect::<Vec<_>>()
                .join("; ")
        },
        d.today
    ));
    if d.data_status != "unavailable" {
        s.push_str(&format!(
            "{} document(s) matched; {} shown, soonest comment deadline first.\n\n",
            d.total_matches,
            d.documents.len()
        ));
    }
    if !d.documents.is_empty() {
        let rows: Vec<Vec<String>> = d
            .documents
            .iter()
            .map(|x| {
                vec![
                    x.html_url
                        .as_ref()
                        .map(|u| format!("[{}]({u})", x.document_number))
                        .unwrap_or_else(|| x.document_number.clone()),
                    x.doc_type.clone(),
                    x.title.clone(),
                    x.agencies.join("; "),
                    x.publication_date.clone().unwrap_or_default(),
                    x.countdown.label.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Document",
                "Type",
                "Title",
                "Agency",
                "Published",
                "Comment period",
            ],
            &rows,
        ));
        s.push('\n');
        for x in d.documents.iter().filter(|x| x.regulations_gov.is_some()) {
            if let Some(r) = &x.regulations_gov {
                s.push_str(&format!(
                    "- Regulations.gov {}: open for comment {}; ends {} (UTC)\n",
                    r.document_id,
                    r.open_for_comment
                        .map_or("unknown".to_string(), |b| b.to_string()),
                    r.comment_end_utc
                        .clone()
                        .unwrap_or_else(|| "not stated".into())
                ));
            }
        }
        s.push('\n');
    }
    s.push_str(&format!("Regulations.gov: {}\n\n", d.regulations_gov.note));
    if let Some(o) = &d.outline {
        s.push_str(&format!(
            "**Comment outline** for \"{}\" written to `{}` (deadline: {}; {} fields filled from your file). It has {} sections with [NURSE: ...] placeholders.\n\n",
            o.document_title,
            o.file,
            o.comment_deadline.clone().unwrap_or_else(|| "not known".into()),
            o.fields_filled,
            o.sections.len()
        ));
        for t in o.tips.iter().take(4) {
            s.push_str(&format!("- \"{}\" _({})_\n", t.quote, t.citation));
        }
        s.push('\n');
    }
    s.push_str(&format!("_{}_\n", d.countdown_rule));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn content_loads() {
        let c: FedContent = content::load(CONTENT).unwrap();
        assert_eq!(c.default_agencies.len(), 6);
        for id in &c.default_agencies {
            assert!(resolve_agency(&c, id).is_some(), "{id}");
        }
        assert!(resolve_type(&c, "Proposed rule").is_some());
        assert!(resolve_type(&c, "PRORULE").is_some());
        assert!(resolve_type(&c, "bulletin").is_none());
        for o in &c.outline {
            for t in &o.tips {
                assert!(c.tips.iter().any(|x| &x.id == t), "{t}");
            }
        }
        // Every quotation is 25 words or fewer.
        for t in &c.tips {
            assert!(t.quote.split_whitespace().count() <= 25, "{}", t.id);
        }
    }

    #[test]
    fn countdown_states() {
        let today = d(2026, 11, 13);
        let cd = |close: &str| countdown(Some(close), today);
        assert_eq!(cd("2026-11-13").status, "closes_today");
        assert_eq!(cd("2026-11-14").days_remaining, Some(1));
        assert_eq!(cd("2026-11-14").status, "closing_soon");
        assert_eq!(cd("2026-11-20").status, "closing_soon");
        assert_eq!(cd("2026-11-21").status, "open");
        assert_eq!(cd("2026-11-12").status, "closed");
        assert_eq!(
            cd("2026-11-12").label,
            "closed 1 day ago (Thursday, 2026-11-12)"
        );
        assert_eq!(countdown(None, today).status, "no_comment_period");
        assert_eq!(
            countdown(Some("0026-01-01"), today).status,
            "no_comment_period"
        );
        assert_eq!(
            countdown(Some("garbage"), today).status,
            "no_comment_period"
        );
    }

    #[derive(Deserialize)]
    struct Case {
        today: String,
        close: String,
        days: i64,
        weekday: String,
    }

    /// Reference values from Python's datetime arithmetic
    /// (`xtask/scripts/gen_countdown_golden.py`), including month, year, and leap-day crossings.
    #[test]
    fn countdown_matches_independent_date_arithmetic() {
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../../tests/fixtures/countdown_golden.json"))
                .unwrap();
        assert!(cases.len() >= 30);
        for c in cases {
            let today = NaiveDate::parse_from_str(&c.today, "%Y-%m-%d").unwrap();
            let r = countdown(Some(&c.close), today);
            assert_eq!(r.days_remaining, Some(c.days), "{} -> {}", c.today, c.close);
            assert!(r.label.contains(&c.weekday), "{}: {}", c.close, r.label);
        }
    }

    #[test]
    fn outline_has_every_section_and_fills_only_known_fields() {
        let c: FedContent = content::load(CONTENT).unwrap();
        let mut fills = BTreeMap::new();
        fills.insert(
            "organization".to_string(),
            "Example Health Department".to_string(),
        );
        let cd = countdown(Some("2026-12-14"), d(2026, 11, 13));
        let md_text = outline_markdown(
            &c,
            "Lead in drinking water",
            Some("Environmental Protection Agency"),
            Some("EPA-HQ-OW-2026-0001"),
            Some("2026-20001"),
            Some("2026-12-14"),
            &cd,
            &fills,
        );
        for o in &c.outline {
            assert!(md_text.contains(&o.heading), "{}", o.heading);
        }
        assert!(md_text.contains("Example Health Department"));
        assert!(md_text.contains("[NURSE: support / oppose / support with changes]"));
        assert!(md_text.contains("EPA-HQ-OW-2026-0001") && md_text.contains("31 days left"));
        assert!(
            !md_text.contains("[NURSE: organization and role]"),
            "filled field is replaced"
        );
    }

    #[test]
    fn long_text_is_cut_on_characters() {
        let s = "ñ".repeat(600);
        let t = truncate_chars(&s, 500);
        assert_eq!(t.chars().count(), 501);
        assert!(t.ends_with('…'));
    }
}
