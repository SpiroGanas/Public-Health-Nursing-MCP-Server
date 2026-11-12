//! Healthy People 2030 objectives (PLAN-02 `write_smartie_objectives`).
//!
//! * **Snapshot:** `healthy-people-2030-objectives`, built by
//!   `xtask/scripts/build_design_snapshots.py` from the objective pages on
//!   ODPHP's Healthy People 2030 site (odphp.health.gov/healthypeople): national
//!   status, most recent data, national target, desired direction, national
//!   baseline with its year, target-setting method, and data source for each
//!   objective.
//! * **Live:** ODPHP has no public JSON API for the objectives that could be
//!   found (the `/healthypeople/api` paths return 404), so [`fetch_objective`]
//!   reads one objective's own page through the live → cache chain and parses
//!   the same fields. Only page addresses taken from the bundled snapshot are
//!   fetched, never an address built from user text. With no network the tool
//!   uses the snapshot and says so.

use std::collections::BTreeMap;

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::apifetch::fetch_validated;
use crate::cache::Cache;
use crate::fetch::Fetched;
use crate::http::HttpClient;
use crate::snapshots::{SnapshotEntry, SnapshotStore};

pub const SNAPSHOT_ID: &str = "healthy-people-2030-objectives";
pub const FILE: &str = "healthy_people_2030_objectives.json";
/// Healthy People 2030 home page.
pub const HOME_URL: &str = "https://odphp.health.gov/healthypeople";
/// Prefix every objective page address starts with.
pub const OBJECTIVE_URL_PREFIX: &str =
    "https://odphp.health.gov/healthypeople/objectives-and-data/browse-objectives/";
/// Cache time for an objective page (one week; ODPHP updates data a few times a year).
pub const TTL_SECS: u64 = 7 * 24 * 3600;

/// One Healthy People 2030 objective as read from its page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Objective {
    /// Objective ID, for example `D-04` or `HC/HIT-02`.
    pub id: String,
    pub title: String,
    /// The objective's page on odphp.health.gov.
    pub url: String,
    /// `core`, `developmental`, `research`, or `unknown`. Developmental and research objectives
    /// have no national baseline or target.
    #[serde(rename = "type")]
    pub objective_type: String,
    /// National status: "Improving", "Getting worse", "Baseline only", and so on.
    pub status: Option<String>,
    pub topics: Vec<String>,
    pub workgroup: Option<String>,
    /// "Increase desired", "Decrease desired", or "Maintains".
    pub desired_direction: Option<String>,
    /// The most recent national value as written ("66.0 percent (2023)").
    pub most_recent_text: Option<String>,
    pub most_recent_value: Option<f64>,
    pub most_recent_period: Option<String>,
    /// The national target as written ("70.3 percent", "15.7 per 100,000").
    pub target_text: Option<String>,
    pub target_value: Option<f64>,
    /// The national baseline as written, with its description and year.
    pub baseline_text: Option<String>,
    pub baseline_value: Option<f64>,
    pub baseline_year: Option<String>,
    /// True when the national values are age-adjusted to the 2000 standard population.
    pub age_adjusted: bool,
    /// Notes the page carries, such as "baseline has been revised".
    pub revised: Vec<String>,
    pub target_setting_method: Option<String>,
    pub data_source: Option<String>,
    pub data_source_url: Option<String>,
    /// True for the 23 Leading Health Indicators.
    #[serde(default)]
    pub leading_health_indicator: bool,
}

/// The bundled snapshot file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HpSnapshot {
    pub format_version: u32,
    pub description: String,
    pub retrieved: String,
    pub pages_found: u32,
    pub pages_unreadable: u32,
    pub counts_by_type: BTreeMap<String, u32>,
    pub leading_health_indicator_ids: Vec<String>,
    pub not_included: Vec<String>,
    pub objectives: Vec<Objective>,
}

/// An objective matched by [`HpSnapshot::search`].
#[derive(Debug, Clone, PartialEq)]
pub struct Match<'a> {
    pub objective: &'a Objective,
    pub score: u32,
    /// True when the query was the objective's ID.
    pub by_id: bool,
}

/// Parse the snapshot text and check it is consistent.
pub fn parse_snapshot(text: &str) -> Result<HpSnapshot> {
    let snap: HpSnapshot = serde_json::from_str(text)
        .map_err(|e| PhnError::Internal(format!("Healthy People snapshot: {e}")))?;
    let mut seen = std::collections::BTreeSet::new();
    for o in &snap.objectives {
        if !seen.insert(o.id.as_str()) {
            return Err(PhnError::Internal(format!(
                "Healthy People snapshot: duplicate objective {}",
                o.id
            )));
        }
        if !is_objective_url(&o.url) {
            return Err(PhnError::Internal(format!(
                "Healthy People snapshot: {} has an address outside ODPHP's objective pages",
                o.id
            )));
        }
    }
    Ok(snap)
}

/// Load the bundled (or newer cached) snapshot with its manifest entry.
pub fn load_snapshot(store: &SnapshotStore) -> Result<(HpSnapshot, SnapshotEntry)> {
    let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
    Ok((parse_snapshot(&text)?, entry))
}

/// True for an objective page address on ODPHP's site.
pub fn is_objective_url(url: &str) -> bool {
    url.starts_with(OBJECTIVE_URL_PREFIX)
        && !url.contains(['?', '#', ' '])
        && url[OBJECTIVE_URL_PREFIX.len()..].matches('/').count() == 1
}

/// An objective ID in a comparable form: upper case, hyphens (not the non-breaking
/// hyphen the site uses), no spaces. "d 04", "D‑04", and "d-04" all give "D-04".
pub fn normalize_id(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        match c {
            '\u{2010}'..='\u{2015}' | '\u{2212}' => out.push('-'),
            c if c.is_whitespace() => {}
            c => out.extend(c.to_uppercase()),
        }
    }
    // "D 04" -> "D04" -> "D-04" when a letter group is followed directly by digits.
    if !out.contains('-') {
        let split = out
            .char_indices()
            .find(|(_, c)| c.is_ascii_digit())
            .map(|(i, _)| i);
        if let Some(i) = split
            && i > 0
        {
            out = format!("{}-{}", &out[..i], &out[i..]);
        }
    }
    out
}

const STOP: &[&str] = &[
    "the", "and", "for", "who", "that", "with", "from", "have", "has", "are", "was", "among",
    "their", "they", "this", "increase", "reduce", "proportion", "people", "persons", "adults",
    "number", "rate", "percent", "health", "get", "use", "more", "less", "than", "all", "any",
    "each", "per",
];

fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in phn_core::text::normalize(s).split_whitespace() {
        let mut w = w.to_string();
        if w.chars().count() < 3 || STOP.contains(&w.as_str()) {
            continue;
        }
        if w.chars().count() > 4 && w.ends_with('s') && !w.ends_with("ss") {
            w.pop();
        }
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

impl HpSnapshot {
    /// An objective by ID (any common spelling of the ID).
    pub fn by_id(&self, id: &str) -> Option<&Objective> {
        let want = normalize_id(id);
        self.objectives
            .iter()
            .find(|o| normalize_id(&o.id) == want)
    }

    /// Objectives matching a topic or keywords, best first. An exact ID is the only match for
    /// itself. Words are matched against the title (weight 3), the topic names (2), and the
    /// data source and workgroup (1); an objective must match at least one title or topic
    /// word. Core objectives (with data) rank ahead of developmental and research ones.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Match<'_>> {
        if let Some(o) = self.by_id(query) {
            return vec![Match {
                objective: o,
                score: 100,
                by_id: true,
            }];
        }
        let words = tokens(query);
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<Match<'_>> = Vec::new();
        for o in &self.objectives {
            let title = tokens(&o.title);
            let topics: Vec<String> = o.topics.iter().flat_map(|t| tokens(t)).collect();
            let other: Vec<String> = o
                .data_source
                .iter()
                .chain(o.workgroup.iter())
                .flat_map(|t| tokens(t))
                .collect();
            let mut score = 0u32;
            let mut core_hit = false;
            for w in &words {
                if title.contains(w) {
                    score += 3;
                    core_hit = true;
                }
                if topics.contains(w) {
                    score += 2;
                    core_hit = true;
                }
                if other.contains(w) {
                    score += 1;
                }
            }
            if core_hit {
                hits.push(Match {
                    objective: o,
                    score,
                    by_id: false,
                });
            }
        }
        let rank = |o: &Objective| match o.objective_type.as_str() {
            "core" => 0,
            "developmental" => 1,
            _ => 2,
        };
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(rank(a.objective).cmp(&rank(b.objective)))
                .then(a.objective.id.cmp(&b.objective.id))
        });
        hits.truncate(limit);
        hits
    }
}

// ---------------------------------------------------------------- page parsing

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find(';') {
            Some(j) if j <= 8 => {
                let ent = &after[..j];
                let rep: Option<String> = match ent {
                    "nbsp" => Some(" ".into()),
                    "amp" => Some("&".into()),
                    "lt" => Some("<".into()),
                    "gt" => Some(">".into()),
                    "quot" => Some("\"".into()),
                    "apos" => Some("'".into()),
                    "rsquo" | "lsquo" => Some("'".into()),
                    "ndash" => Some("\u{2013}".into()),
                    "mdash" => Some("\u{2014}".into()),
                    e if e.starts_with("#x") || e.starts_with("#X") => {
                        u32::from_str_radix(&e[2..], 16)
                            .ok()
                            .and_then(char::from_u32)
                            .map(String::from)
                    }
                    e if e.starts_with('#') => e[1..]
                        .parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                        .map(String::from),
                    _ => None,
                };
                match rep {
                    Some(r) => {
                        out.push_str(&r);
                        rest = &after[j + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = after;
                    }
                }
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Remove `<name ...>...</name>` elements (script and style) from HTML.
fn drop_elements(html: &str, name: &str) -> String {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(&close) {
            Some(j) => rest = &rest[i + j + close.len()..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Visible text of an HTML fragment: tags become spaces, entities are decoded, the
/// non-breaking hyphen the site uses in IDs becomes "-", and runs of whitespace collapse.
fn text_of(fragment: &str) -> String {
    let f = drop_elements(&drop_elements(fragment, "script"), "style");
    let mut out = String::with_capacity(f.len());
    let mut in_tag = false;
    for c in f.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            c => out.push(c),
        }
    }
    decode_entities(&out)
        .replace('\u{2011}', "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Remove every `<tag ...>...</tag>` (non-nested) from a fragment.
fn drop_tag(fragment: &str, tag: &str) -> String {
    drop_elements(fragment, tag)
}

/// First number in a string (thousands commas allowed).
fn first_number(s: &str) -> Option<f64> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = if i > 0 && chars[i - 1] == '-' { i - 1 } else { i };
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == ',') {
                j += 1;
            }
            if j < chars.len() && chars[j] == '.' {
                j += 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let t: String = chars[start..j].iter().filter(|c| **c != ',').collect();
            return t.trim_end_matches('.').parse().ok();
        }
        i += 1;
    }
    None
}

/// HTML of one data-header block (`recent`, `target`, `direction`, `baseline`).
fn header_block<'a>(page: &'a str, name: &str) -> &'a str {
    let marker = format!("<div class=\"o-dataheader-{name}");
    let Some(i) = page.find(&marker) else {
        return "";
    };
    let body = &page[i + marker.len()..];
    // Skip the rest of the opening tag.
    let body = body.find('>').map_or("", |g| &body[g + 1..]);
    let end = ["<div class=\"o-data", "<div class=\"o-objective"]
        .iter()
        .filter_map(|m| body.find(m))
        .min()
        .unwrap_or(body.len());
    &body[..end]
}

/// (text with notes removed, notes) for one header block.
fn value_unit(fragment: &str) -> (String, Vec<String>) {
    let mut flags = Vec::new();
    let mut rest = fragment;
    while let Some(i) = rest.find("<sup>") {
        let after = &rest[i..];
        let end = after.find("</sup>").unwrap_or(after.len());
        let sup = &after[..end];
        if let Some(t) = sup.find("title=\"") {
            let v = &sup[t + 7..];
            if let Some(q) = v.find('"') {
                let first = v[..q].split('.').next().unwrap_or("").trim().to_lowercase();
                if !first.is_empty() {
                    flags.push(first);
                }
            }
        }
        rest = &after[end..];
    }
    let mut f = drop_tag(fragment, "small");
    f = drop_tag(&f, "sup");
    // Age-adjustment marker spans carry no text but a star.
    let mut cleaned = String::new();
    let mut r = f.as_str();
    let span = "<span class=\"link-baseline-age-adjustment\">";
    while let Some(i) = r.find(span) {
        cleaned.push_str(&r[..i]);
        match r[i..].find("</span>") {
            Some(j) => r = &r[i + j + 7..],
            None => r = "",
        }
    }
    cleaned.push_str(r);
    (text_of(&cleaned), flags)
}

fn text_after<'a>(page: &'a str, marker: &str) -> Option<&'a str> {
    page.find(marker).map(|i| &page[i + marker.len()..])
}

/// The text inside the first `<a ...>text</a>` of a fragment, with its `href`.
fn find_anchor(fragment: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = fragment[from..].find("<a") {
        let at = from + i;
        if fragment[at + 2..].starts_with(char::is_whitespace) {
            return Some(at);
        }
        from = at + 2;
    }
    None
}

fn first_anchor(fragment: &str) -> Option<(String, String)> {
    let i = find_anchor(fragment)?;
    let a = &fragment[i..];
    let href = {
        let h = a.find("href=\"")? + 6;
        let e = a[h..].find('"')?;
        a[h..h + e].to_string()
    };
    let gt = a.find('>')?;
    let end = a.find("</a>")?;
    Some((text_of(&a[gt + 1..end]), href))
}

fn period_at_end(s: &str) -> Option<String> {
    // "... (2023)" or "... (2022-24)"
    let t = s.trim_end();
    let inner = t.strip_suffix(')')?;
    let open = inner.rfind('(')?;
    let p = &inner[open + 1..];
    let ok = p.len() >= 4
        && p[..4].chars().all(|c| c.is_ascii_digit())
        && p[4..].chars().all(|c| c.is_ascii_digit() || c == '-');
    ok.then(|| p.to_string())
}

fn year_at_end(s: &str) -> Option<String> {
    // "... in 2019", "... as reported in 2019-21", "... for 2019", "... during 2019"
    let t = s.trim_end();
    let last = t.rsplit(' ').next()?;
    let ok = last.len() >= 4
        && (last.starts_with("19") || last.starts_with("20"))
        && last[..4].chars().all(|c| c.is_ascii_digit())
        && last[4..].chars().all(|c| c.is_ascii_digit() || c == '-');
    if !ok {
        return None;
    }
    let before = t[..t.len() - last.len()].trim_end();
    ["in", "for", "during"]
        .iter()
        .any(|w| before.ends_with(&format!(" {w}")) || before == *w)
        .then(|| last.to_string())
}

/// Parse one Healthy People 2030 objective page.
pub fn parse_objective_page(page: &str, url: &str) -> Result<Objective> {
    let bad = |why: &str| PhnError::Network(format!("Healthy People page is not an objective: {why}"));
    let h1_start = page
        .find("<h1 class=\"page-title\">")
        .ok_or_else(|| bad("no title"))?;
    let h1 = &page[h1_start + 23..];
    let h1 = &h1[..h1.find("</h1>").ok_or_else(|| bad("no title"))?];
    let title_full = text_of(h1);
    let (title, oid) = title_full
        .rsplit_once(" \u{2014} ")
        .or_else(|| title_full.rsplit_once(" \u{2013} "))
        .or_else(|| title_full.rsplit_once(" - "))
        .ok_or_else(|| bad("no objective ID in the title"))?;
    let valid_id = !oid.is_empty()
        && oid
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '/')
        && oid.chars().any(|c| c.is_ascii_digit());
    if !valid_id {
        return Err(bad("no objective ID in the title"));
    }
    let flat = text_of(page);
    let status = flat.find("National Status:").and_then(|i| {
        let after = &flat[i + 16..];
        let end = after.find("Learn more about our data release schedule")?;
        Some(after[..end].trim().to_string()).filter(|s| !s.is_empty())
    });
    let (rec_text, _) = value_unit(header_block(page, "recent"));
    let (tgt_text, tgt_flags) = value_unit(header_block(page, "target"));
    let dir_html = drop_tag(header_block(page, "direction"), "small");
    let dir_text = text_of(&dir_html);
    let (base_text, base_flags) = value_unit(header_block(page, "baseline"));
    let non_empty = |s: String| if s.is_empty() { None } else { Some(s) };
    let tsm = text_after(page, "Target-Setting Method</div>").and_then(|a| {
        let i = a.find("<div class=\"field__item\">")? + 25;
        let e = a[i..].find("</div>")?;
        non_empty(text_of(&a[i..i + e]))
    });
    let ds = text_after(page, "o-objective-data-sources\">").and_then(first_anchor);
    let topics: Vec<String> = text_after(page, "Topics:&nbsp;</strong>")
        .or_else(|| text_after(page, "Topic:&nbsp;</strong>"))
        .map(|a| {
            let end = a.find("</p>").unwrap_or(a.len());
            let mut out = Vec::new();
            let mut rest = &a[..end];
            while let Some(i) = find_anchor(rest) {
                let sub = &rest[i..];
                if let Some((t, _)) = first_anchor(sub) {
                    out.push(t);
                }
                rest = match sub.find("</a>") {
                    Some(e) => &sub[e + 4..],
                    None => "",
                };
            }
            out
        })
        .unwrap_or_default();
    let workgroup = text_after(page, "Workgroup:&nbsp;</strong>")
        .and_then(first_anchor)
        .map(|(t, _)| t);
    let age_adjusted = page.contains("Age adjusted to the year 2000 standard population");
    let objective_type = match status.as_deref() {
        None => "unknown",
        Some("Developmental") => "developmental",
        Some("Research") => "research",
        Some(_) => "core",
    };
    let mut revised: Vec<String> = tgt_flags.into_iter().chain(base_flags).collect();
    revised.sort();
    revised.dedup();
    Ok(Objective {
        id: oid.to_string(),
        title: title.trim().to_string(),
        url: url.to_string(),
        objective_type: objective_type.to_string(),
        status,
        topics,
        workgroup,
        desired_direction: non_empty(dir_text),
        most_recent_value: first_number(&rec_text),
        most_recent_period: period_at_end(&rec_text),
        most_recent_text: non_empty(rec_text),
        target_value: first_number(&tgt_text),
        target_text: non_empty(tgt_text),
        baseline_value: first_number(&base_text),
        baseline_year: year_at_end(&base_text),
        baseline_text: non_empty(base_text),
        age_adjusted,
        revised,
        target_setting_method: tsm,
        data_source_url: ds.as_ref().map(|(_, h)| {
            if h.starts_with('/') {
                format!("https://odphp.health.gov{h}")
            } else {
                h.clone()
            }
        }),
        data_source: ds.map(|(t, _)| t),
        leading_health_indicator: false,
    })
}

fn expect_objective_page(body: &[u8]) -> Result<()> {
    let page = String::from_utf8_lossy(body);
    parse_objective_page(&page, OBJECTIVE_URL_PREFIX).map(|_| ())
}

/// Fetch and parse one objective's page (live, or the cached copy of the same page). There is no
/// page-level snapshot: the caller falls back to the bundled [`HpSnapshot`] on error. `url` must
/// be an objective page address from the snapshot.
pub async fn fetch_objective(
    http: &HttpClient,
    cache: Option<&Cache>,
    url: &str,
) -> Result<(Objective, Fetched)> {
    if !is_objective_url(url) {
        return Err(PhnError::InvalidInput(
            "only Healthy People 2030 objective pages can be fetched".into(),
        ));
    }
    let fetched = fetch_validated(
        http,
        cache,
        url,
        None,
        TTL_SECS,
        &expect_objective_page,
        None,
    )
    .await?;
    let page = String::from_utf8_lossy(&fetched.body).into_owned();
    let o = parse_objective_page(&page, url)?;
    Ok((o, fetched))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/hp2030_d04.html");
    const D04_URL: &str = "https://odphp.health.gov/healthypeople/objectives-and-data/browse-objectives/diabetes/increase-proportion-adults-diabetes-who-have-yearly-eye-exam-d-04";

    fn snapshot() -> HpSnapshot {
        load_snapshot(&SnapshotStore::bundled()).unwrap().0
    }

    #[test]
    fn page_parser_matches_the_snapshot_builder() {
        // The fixture is the header of the D-04 page read on the snapshot date; the Python
        // builder wrote the snapshot row from the same page.
        let live = parse_objective_page(FIXTURE, D04_URL).unwrap();
        let snap = snapshot();
        let row = snap.by_id("D-04").unwrap();
        assert_eq!(&live, row);
        assert_eq!(live.target_value, Some(70.3));
        assert_eq!(live.baseline_value, Some(64.8));
        assert_eq!(live.baseline_year.as_deref(), Some("2019"));
        assert_eq!(live.most_recent_period.as_deref(), Some("2023"));
        assert!(live.age_adjusted);
        assert_eq!(live.revised, vec!["baseline has been revised", "target has been revised"]);
    }

    #[test]
    fn page_parser_rejects_other_pages() {
        assert!(parse_objective_page("<html><body>Page not found</body></html>", "x").is_err());
        assert!(
            parse_objective_page("<h1 class=\"page-title\">Diabetes</h1>", "x").is_err(),
            "a topic page has no objective ID"
        );
    }

    #[test]
    fn bundled_snapshot_is_consistent() {
        let (snap, entry) = load_snapshot(&SnapshotStore::bundled()).unwrap();
        assert_eq!(entry.id, SNAPSHOT_ID);
        assert!(snap.objectives.len() >= 480, "{}", snap.objectives.len());
        assert_eq!(snap.leading_health_indicator_ids.len(), 23);
        let core = snap
            .objectives
            .iter()
            .filter(|o| o.objective_type == "core")
            .count();
        assert!(core >= 340);
        for o in snap.objectives.iter().filter(|o| o.objective_type == "core") {
            assert!(o.target_value.is_some(), "{} has no target", o.id);
            assert!(o.baseline_value.is_some(), "{} has no baseline", o.id);
            assert!(o.desired_direction.is_some(), "{}", o.id);
            assert!(o.data_source.is_some(), "{}", o.id);
        }
        for o in snap
            .objectives
            .iter()
            .filter(|o| o.objective_type == "developmental")
        {
            // A few developmental objectives show a baseline, but none has a national target.
            assert!(o.target_value.is_none(), "{}", o.id);
        }
        assert!(snap.retrieved.as_str() >= "2026-01-01" && snap.retrieved.as_str() <= "2100-12-31");
    }

    #[test]
    fn ids_match_in_every_spelling() {
        let snap = snapshot();
        for q in ["D-04", "d-04", "D\u{2011}04", " d 04 ", "D04"] {
            assert_eq!(snap.by_id(q).map(|o| o.id.as_str()), Some("D-04"), "{q}");
        }
        assert_eq!(
            snap.by_id("hc/hit-02").map(|o| o.id.as_str()),
            Some("HC/HIT-02")
        );
        assert!(snap.by_id("ZZ-99").is_none());
    }

    #[test]
    fn keyword_search_finds_the_diabetes_eye_exam() {
        let snap = snapshot();
        let hits = snap.search("diabetes eye exam", 5);
        assert_eq!(hits[0].objective.id, "D-04", "{:?}", hits.iter().map(|h| &h.objective.id).collect::<Vec<_>>());
        let hits = snap.search("flu vaccine every year", 3);
        assert_eq!(hits[0].objective.id, "IID-09");
        assert!(snap.search("zzzz qqqq", 5).is_empty());
        assert!(snap.search("the and of", 5).is_empty());
    }

    #[test]
    fn only_objective_pages_are_fetchable() {
        assert!(is_objective_url(D04_URL));
        assert!(!is_objective_url("https://evil.example/healthypeople"));
        assert!(!is_objective_url(
            "https://odphp.health.gov/healthypeople/objectives-and-data/browse-objectives/diabetes"
        ));
        assert!(!is_objective_url(&format!("{D04_URL}?x=1")));
        for o in &snapshot().objectives {
            assert!(is_objective_url(&o.url), "{}", o.url);
        }
    }
}
