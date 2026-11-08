//! CDC guidance feeds: MMWR, Health Alert Network (HAN) notices, and CDC
//! Stacks.
//!
//! CDC reorganized www.cdc.gov in 2024–2025 and retired several RSS feeds.
//! The methods used here, checked in November 2026:
//!
//! * **MMWR**: the CDC Content Syndication API v2
//!   (`tools.cdc.gov/api/v2/resources/media`). A topic search uses the `title`
//!   parameter, newest first, keeping items whose URL is under `/mmwr/`; with
//!   no topic, the "MMWR (Journal)" feed collection (media 132599) lists the
//!   latest reports.
//! * **HAN**: the HAN archive page is built in the browser from a JSON
//!   module (`/han/php/modules/han-archive.static.json`, notices up to the
//!   module's last refresh); newer notices are found by requesting the next
//!   sequential notice pages (`/han/php/notices/hanNNNNN.html`) until one is
//!   missing, reading each page's title, "Distributed via the CDC Health Alert
//!   Network" date, and message type. Notices from 2023 and earlier come from
//!   the retired HAN feed collection (media 320567).
//! * **CDC Stacks**: its search, OAI, and record pages refused automated
//!   requests (HTTP 403) from the development network, so the server returns a
//!   ready-made Stacks search link instead of results.

use phn_core::{PhnError, Result, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apifetch::{self, expect_json};
use crate::cache::Cache;
use crate::http::HttpClient;
use crate::socrata::encode;

/// Syndication API media search.
pub const SYNDICATION_MEDIA: &str = "https://tools.cdc.gov/api/v2/resources/media";
/// "MMWR (Journal)" feed collection.
pub const MMWR_FEED_ID: u32 = 132_599;
/// Retired HAN feed collection (notices through 2023).
pub const HAN_LEGACY_FEED_ID: u32 = 320_567;
/// HAN archive module.
pub const HAN_ARCHIVE: &str = "https://www.cdc.gov/han/php/modules/han-archive.static.json";
/// HAN notices index (for citations).
pub const HAN_INDEX: &str = "https://www.cdc.gov/han/php/notices/index.html";
/// The most recent notice number confirmed when this module was written; the
/// probe starts after the highest number known from the archive or this.
pub const HAN_KNOWN_LATEST: u32 = 533;
/// At most this many new notice pages are probed per call.
pub const HAN_MAX_PROBES: u32 = 25;

/// Which CDC series an item comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Series {
    /// Morbidity and Mortality Weekly Report.
    Mmwr,
    /// Health Alert Network notice.
    Han,
    /// CDC Stacks digital library.
    Stacks,
}

impl Series {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mmwr => "MMWR",
            Self::Han => "HAN",
            Self::Stacks => "CDC Stacks",
        }
    }
}

/// One item of guidance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GuidanceItem {
    pub series: Series,
    pub title: String,
    /// Publication or distribution date (YYYY-MM-DD) when known.
    pub date: Option<String>,
    pub url: String,
    /// Short summary from CDC's own description.
    pub summary: String,
    /// HAN message type (Health Alert, Health Advisory, Health Update), when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
    /// HAN number (e.g. "00533") when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
}

/// Where a list came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    pub mode: SourceMode,
    pub as_of: String,
    pub url: String,
    pub method: String,
    pub fallback_reason: Option<String>,
}

async fn get_json(
    http: &HttpClient,
    cache: Option<&Cache>,
    url: &str,
    ttl: u64,
) -> Result<(Value, SourceMode, String, Option<String>)> {
    let f = apifetch::fetch_validated(http, cache, url, None, ttl, &expect_json, None).await?;
    let v = serde_json::from_slice(&f.body)
        .map_err(|e| PhnError::Network(format!("CDC returned invalid JSON: {e}")))?;
    Ok((v, f.mode, f.as_of, f.fallback_reason))
}

fn clean_url(u: &str) -> String {
    u.split('?').next().unwrap_or(u).to_string()
}

fn date_part(s: &str) -> Option<String> {
    let d = s.get(..10)?;
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .ok()
        .map(|_| d.to_string())
}

/// Parse syndication API items (search results or a feed's children).
pub fn parse_syndication_items(items: &[Value], mmwr_only: bool) -> Vec<GuidanceItem> {
    let mut out: Vec<GuidanceItem> = Vec::new();
    for r in items {
        let url = r.get("sourceUrl").and_then(Value::as_str).unwrap_or("");
        if mmwr_only && !url.contains("/mmwr/") {
            continue;
        }
        let url = clean_url(url);
        if out.iter().any(|i| i.url == url) {
            continue;
        }
        let title = r.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let mut summary = r
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if summary.starts_with("The Morbidity and Mortality Weekly Report (MMWR) Series is prepared") {
            summary.clear();
        }
        out.push(GuidanceItem {
            series: Series::Mmwr,
            title,
            date: r
                .get("datePublished")
                .and_then(Value::as_str)
                .and_then(date_part),
            url,
            summary,
            message_type: None,
            number: None,
        });
    }
    out
}

/// MMWR reports whose title matches `query`, newest first (or the latest
/// reports when `query` is empty).
pub async fn mmwr(
    http: &HttpClient,
    cache: Option<&Cache>,
    query: &str,
) -> Result<(Vec<GuidanceItem>, Provenance)> {
    let q = query.trim();
    let (url, method) = if q.is_empty() {
        (
            format!("{SYNDICATION_MEDIA}/{MMWR_FEED_ID}?showchildlevel=1"),
            "CDC Content Syndication API v2, MMWR (Journal) feed collection".to_string(),
        )
    } else {
        (
            format!(
                "{SYNDICATION_MEDIA}?title={}&max=100&sort=-datePublished",
                encode(q)
            ),
            "CDC Content Syndication API v2 title search, filtered to /mmwr/ pages".to_string(),
        )
    };
    let (v, mode, as_of, fallback_reason) = get_json(http, cache, &url, 6 * 3600).await?;
    let items: Vec<Value> = if q.is_empty() {
        v.pointer("/results/0/children")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        v.get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut list = parse_syndication_items(&items, true);
    list.sort_by(|a, b| b.date.cmp(&a.date));
    Ok((
        list,
        Provenance {
            mode,
            as_of,
            url,
            method,
            fallback_reason,
        },
    ))
}

/// Parse the HAN archive module (a search-engine response with `docs`).
pub fn parse_han_archive(v: &Value) -> Vec<GuidanceItem> {
    let docs = v
        .pointer("/response/docs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    docs.iter()
        .filter_map(|d| {
            let url = d.get("permalink").and_then(Value::as_str)?.to_string();
            let title = strip_tags(d.get("title_txt").and_then(Value::as_str).unwrap_or(""));
            let date = d
                .get("cdc_last_reviewed_date_dt")
                .and_then(Value::as_str)
                .and_then(date_part);
            let image = d
                .get("cdc_related_image_str")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            let message_type = if image.contains("advisory") {
                Some("Health Advisory".to_string())
            } else if image.contains("update") {
                Some("Health Update".to_string())
            } else if image.contains("alert") && !image.contains("logo-health-alert-network") {
                Some("Health Alert".to_string())
            } else {
                None
            };
            Some(GuidanceItem {
                series: Series::Han,
                number: han_number(&url),
                title,
                date,
                summary: d
                    .get("excerpt_txt")
                    .and_then(Value::as_str)
                    .map(strip_tags)
                    .unwrap_or_default(),
                url,
                message_type,
            })
        })
        .collect()
}

/// Parse the retired HAN feed collection (2023 and earlier).
pub fn parse_han_legacy(v: &Value) -> Vec<GuidanceItem> {
    v.pointer("/results/0/children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|c| {
            let name = c.get("name").and_then(Value::as_str)?;
            let (title, rest) = name.split_once(" - Health Alert Network (HAN)").unwrap_or((name, ""));
            let url = c.get("sourceUrl").and_then(Value::as_str)?.to_string();
            if !url.contains("/han/") {
                return None;
            }
            let number = rest
                .split("No.")
                .nth(1)
                .map(|n| format!("{:0>5}", n.trim()))
                .or_else(|| han_number(&url));
            Some(GuidanceItem {
                series: Series::Han,
                title: title.trim_start_matches('c').trim().to_string(),
                date: c.get("datePublished").and_then(Value::as_str).and_then(date_part),
                url,
                summary: c
                    .get("description")
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default(),
                message_type: None,
                number,
            })
        })
        .collect()
}

/// "han00533" in a URL → "00533".
pub fn han_number(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let i = lower.rfind("han")?;
    let digits: String = lower[i + 3..].chars().take_while(char::is_ascii_digit).collect();
    (digits.len() == 5).then_some(digits)
}

/// Remove HTML tags and collapse whitespace.
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("&#8217;", "'")
        .replace("&rsquo;", "'")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn meta(html: &str, attr: &str, key: &str) -> Option<String> {
    let needle = format!("{attr}=\"{key}\"");
    let i = html.find(&needle)?;
    let tag_start = html[..i].rfind('<')?;
    let tag_end = i + html[i..].find('>')?;
    let tag = &html[tag_start..tag_end];
    let c = tag.find("content=\"")? + 9;
    let end = tag[c..].find('"')?;
    Some(strip_tags(&tag[c..c + end]))
}

/// "September 10, 2026" → "2026-09-10".
pub fn parse_long_date(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches(',');
    chrono::NaiveDate::parse_from_str(s, "%B %d, %Y")
        .ok()
        .map(|d| d.to_string())
}

/// Parse one HAN notice page.
pub fn parse_han_page(html: &str, url: &str) -> Option<GuidanceItem> {
    let start = html.find("<title>")? + 7;
    let end = start + html[start..].find("</title>")?;
    let title = strip_tags(&html[start..end]);
    let title = title.trim_end_matches(" | HAN | CDC").trim().to_string();
    let text = strip_tags(html);
    let date = text
        .find("Distributed via the CDC Health Alert Network")
        .and_then(|i| {
            let rest = &text[i + "Distributed via the CDC Health Alert Network".len()..];
            let words: Vec<&str> = rest.split_whitespace().take(3).collect();
            parse_long_date(&words.join(" "))
        })
        .or_else(|| meta(html, "property", "cdc:last_reviewed").and_then(|d| parse_long_date(&d)));
    let message_type = ["Health Alert", "Health Advisory", "Health Update"]
        .iter()
        .find(|t| text.contains(&format!("(HAN) {t}")))
        .map(|t| t.to_string());
    let summary = meta(html, "name", "description").unwrap_or_default();
    Some(GuidanceItem {
        series: Series::Han,
        title,
        date,
        url: url.to_string(),
        summary,
        message_type,
        number: han_number(url),
    })
}

/// URL of HAN notice `n`.
pub fn han_url(n: u32) -> String {
    format!("https://www.cdc.gov/han/php/notices/han{n:05}.html")
}

fn expect_html(body: &[u8]) -> Result<()> {
    let head = String::from_utf8_lossy(&body[..body.len().min(4096)]).to_ascii_lowercase();
    if head.contains("<title>") {
        Ok(())
    } else {
        Err(PhnError::Network("not an HTML page".into()))
    }
}

/// Every HAN notice the server can find, newest first, with notes on coverage.
pub async fn han_notices(
    http: &HttpClient,
    cache: Option<&Cache>,
) -> Result<(Vec<GuidanceItem>, Provenance, Vec<String>)> {
    let mut notes = Vec::new();
    let mut items: Vec<GuidanceItem> = Vec::new();
    let mut mode = SourceMode::Live;
    let mut as_of = chrono::Utc::now().to_rfc3339();
    let mut fallback_reason = None;
    match get_json(http, cache, HAN_ARCHIVE, 24 * 3600).await {
        Ok((v, m, a, f)) => {
            items.extend(parse_han_archive(&v));
            if m != SourceMode::Live {
                mode = m;
                as_of = a;
            }
            fallback_reason = f;
        }
        Err(e) => notes.push(format!("HAN archive module unavailable: {e}")),
    }
    let legacy_url = format!("{SYNDICATION_MEDIA}/{HAN_LEGACY_FEED_ID}?showchildlevel=1");
    match get_json(http, cache, &legacy_url, 7 * 24 * 3600).await {
        Ok((v, _, _, _)) => {
            for i in parse_han_legacy(&v) {
                if !items.iter().any(|x| x.number.is_some() && x.number == i.number) {
                    items.push(i);
                }
            }
        }
        Err(e) => notes.push(format!("retired HAN feed (2023 and earlier) unavailable: {e}")),
    }
    let highest = items
        .iter()
        .filter_map(|i| i.number.as_deref()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    // Probe newer pages one by one until a notice is missing.
    let mut n = highest.max(1) + 1;
    let first_probe = n;
    let mut probes = 0;
    let mut probe_error = None;
    while probes < HAN_MAX_PROBES {
        let url = han_url(n);
        let ttl = 30 * 24 * 3600;
        match apifetch::fetch_validated(http, cache, &url, None, ttl, &expect_html, None).await {
            Ok(f) => {
                if let Some(item) = parse_han_page(&String::from_utf8_lossy(&f.body), &url)
                    && !items.iter().any(|x| x.number == item.number)
                {
                    items.push(item);
                }
                if f.mode != SourceMode::Live && mode == SourceMode::Live {
                    mode = f.mode;
                }
            }
            Err(e) => {
                probe_error = Some(e.to_string());
                break;
            }
        }
        n += 1;
        probes += 1;
    }
    if n <= HAN_KNOWN_LATEST {
        notes.push(format!(
            "Notices after HAN {:05} could not be checked ({}); HAN {HAN_KNOWN_LATEST:05} was \
             published by September 2026. See {HAN_INDEX}.",
            n.saturating_sub(1),
            probe_error.as_deref().unwrap_or("probe limit reached")
        ));
    } else if n == first_probe && probe_error.is_some() && highest < HAN_KNOWN_LATEST {
        notes.push(format!("Newer HAN pages could not be reached; see {HAN_INDEX}."));
    }
    items.sort_by(|a, b| b.date.cmp(&a.date).then(b.number.cmp(&a.number)));
    Ok((
        items,
        Provenance {
            mode,
            as_of,
            url: HAN_INDEX.into(),
            method: "HAN archive module, sequential notice pages, and the retired HAN feed".into(),
            fallback_reason,
        },
        notes,
    ))
}

/// A CDC Stacks search link (newest first) for the nurse to open.
pub fn stacks_search_url(query: &str) -> String {
    format!(
        "https://stacks.cdc.gov/gsearch?terms={}&sortBy=fgs.lastModifiedDate+desc",
        encode(query.trim())
    )
}

/// Relevance of an item to query words (count of distinct words matched in
/// the title and summary).
pub fn match_score(item: &GuidanceItem, words: &[String]) -> usize {
    let hay = format!("{} {}", item.title, item.summary).to_lowercase();
    words.iter().filter(|w| hay.contains(w.as_str())).count()
}

/// Query words worth matching (lower case, 3+ characters, no stop words).
pub fn query_words(query: &str) -> Vec<String> {
    const STOP: [&str; 12] = [
        "the", "and", "for", "with", "from", "about", "guidance", "cdc", "new", "latest", "recommendations", "what",
    ];
    let mut out: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !STOP.contains(w))
        .map(str::to_string)
        .collect();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_syndication_search_and_keeps_only_mmwr() {
        let v: Value =
            serde_json::from_str(include_str!("../tests/fixtures/phase4/cdc_syndication_search.json"))
                .unwrap();
        let items = v["results"].as_array().unwrap();
        let all = parse_syndication_items(items, false);
        let mmwr = parse_syndication_items(items, true);
        assert!(all.len() > mmwr.len());
        assert!(mmwr.iter().all(|i| i.url.contains("/mmwr/") && !i.url.contains('?')));
        assert!(mmwr.iter().all(|i| i.date.as_deref().is_some_and(|d| d.len() == 10)));
    }

    #[test]
    fn parses_han_archive_and_pages() {
        let v: Value =
            serde_json::from_str(include_str!("../tests/fixtures/phase4/han_archive_static.json"))
                .unwrap();
        let items = parse_han_archive(&v);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].number.as_deref(), Some("00523"));
        assert_eq!(items[0].date.as_deref(), Some("2025-03-18"));
        assert_eq!(items[1].message_type.as_deref(), Some("Health Advisory"));
        let page = parse_han_page(
            include_str!("../tests/fixtures/phase4/han_notice.html"),
            &han_url(533),
        )
        .unwrap();
        assert!(page.title.starts_with("Nationwide Increase in Reported Human Rabies Exposures"));
        assert_eq!(page.date.as_deref(), Some("2026-09-10"));
        assert_eq!(page.number.as_deref(), Some("00533"));
        assert_eq!(page.message_type.as_deref(), Some("Health Advisory"));
    }

    #[test]
    fn helpers() {
        assert_eq!(han_number("https://www.cdc.gov/han/2023/han00497.asp").as_deref(), Some("00497"));
        assert_eq!(parse_long_date("January 16, 2025").as_deref(), Some("2025-01-16"));
        assert_eq!(strip_tags("Mpox <em>Monkeypox Virus</em> &amp; more"), "Mpox Monkeypox Virus & more");
        assert_eq!(query_words("CDC guidance on measles exposure"), vec!["measles", "exposure"]);
        assert!(stacks_search_url("lead poisoning").contains("terms=lead%20poisoning"));
    }

    #[tokio::test]
    async fn offline_han_lookup_reports_coverage_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&phn_core::Config::for_root(dir.path())).unwrap();
        let (items, _, notes) = han_notices(&http, None, ).await.unwrap();
        assert!(items.is_empty());
        assert!(!notes.is_empty());
    }
}
