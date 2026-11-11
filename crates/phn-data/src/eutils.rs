//! NCBI E-utilities client for PubMed and MeSH.
//!
//! * Every request carries `tool=public-health-nursing-mcp-server` and never
//!   an email address. An optional key from `PHN_NCBI_API_KEY` raises NCBI's
//!   limit from 3 to 10 requests per second; it is never cached or cited.
//! * Requests are spaced at least 350 ms apart (110 ms with a key), which
//!   keeps the server under NCBI's limits even when several calls run at once.
//! * Responses are cached for one day. A response that carries an
//!   `<ERROR>` element (NCBI answers some backend failures with HTTP 200) is
//!   treated as a failed request and is never cached.
//!
//! Endpoints: `esearch` (PubMed search and automatic term mapping, MeSH
//! heading checks) and `efetch` (PubMed records as XML), documented in
//! *Entrez Programming Utilities Help* (NCBI Bookshelf NBK25501).

use std::time::{Duration, Instant};

use phn_core::{PhnError, Result, SourceMode};
use quick_xml::events::{BytesRef, Event};
use quick_xml::reader::Reader;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::apifetch::{self, expect_xml};
use crate::cache::Cache;
use crate::http::HttpClient;
use crate::socrata::encode;

/// E-utilities base URL.
pub const BASE: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils";
/// Tool name sent with every request (no email is sent).
pub const TOOL: &str = "public-health-nursing-mcp-server";
/// Environment variable with the optional API key.
pub const KEY_ENV: &str = "PHN_NCBI_API_KEY";
/// Cache TTL (one day).
pub const TTL_SECS: u64 = 24 * 3600;

static LAST_REQUEST: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);

/// Provenance of an E-utilities response.
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    pub mode: SourceMode,
    pub as_of: String,
}

/// E-utilities client.
#[derive(Debug, Clone)]
pub struct Eutils {
    key: Option<String>,
}

impl Default for Eutils {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of `esearch`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub count: u64,
    pub ids: Vec<String>,
    /// PubMed's translation of the query (shows automatic term mapping).
    pub query_translation: String,
    /// (term entered, PubMed's mapping) pairs.
    pub translations: Vec<(String, String)>,
    /// Errors or warnings reported by PubMed (phrases not found, etc.).
    pub warnings: Vec<String>,
}

/// One PubMed record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Article {
    pub pmid: String,
    pub title: String,
    pub journal: String,
    pub journal_abbreviation: String,
    /// Publication year.
    pub year: Option<String>,
    /// Authors as "Last Initials" (collective names as written).
    pub authors: Vec<String>,
    pub abstract_text: String,
    pub doi: Option<String>,
    pub publication_types: Vec<String>,
    pub languages: Vec<String>,
}

impl Article {
    /// PubMed link.
    pub fn url(&self) -> String {
        format!("https://pubmed.ncbi.nlm.nih.gov/{}/", self.pmid)
    }
}

impl Eutils {
    /// Client with the key from `PHN_NCBI_API_KEY` if set.
    pub fn new() -> Self {
        Self {
            key: apifetch::api_key(KEY_ENV),
        }
    }

    fn min_interval(&self) -> Duration {
        if self.key.is_some() {
            Duration::from_millis(110)
        } else {
            Duration::from_millis(350)
        }
    }

    /// The public (key-free) URL for an endpoint and parameters.
    pub fn url(endpoint: &str, params: &[(&str, &str)]) -> String {
        let mut url = format!("{BASE}/{endpoint}.fcgi?");
        let mut parts: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect();
        parts.push(format!("tool={TOOL}"));
        url.push_str(&parts.join("&"));
        url
    }

    async fn get(
        &self,
        http: &HttpClient,
        cache: Option<&Cache>,
        url: &str,
    ) -> Result<(Vec<u8>, Provenance)> {
        // A fresh cached copy needs no request (and no wait).
        if let Some(c) = cache
            && let Ok(Some(hit)) = c.get(url)
            && hit.fresh
            && expect_eutils(&hit.body).is_ok()
        {
            return Ok((
                hit.body,
                Provenance {
                    mode: SourceMode::Cache,
                    as_of: hit.fetched_at.to_rfc3339(),
                },
            ));
        }
        {
            let mut last = LAST_REQUEST.lock().await;
            if let Some(t) = *last {
                let wait = self.min_interval().saturating_sub(t.elapsed());
                if !wait.is_zero() {
                    tokio::time::sleep(wait).await;
                }
            }
            *last = Some(Instant::now());
        }
        let key = self.key.as_deref().map(|k| ("api_key", k));
        let f = apifetch::fetch_validated(http, cache, url, key, TTL_SECS, &expect_eutils, None)
            .await?;
        Ok((
            f.body,
            Provenance {
                mode: f.mode,
                as_of: f.as_of,
            },
        ))
    }

    /// Search PubMed. `sort` is `relevance` or `pub_date`.
    pub async fn search(
        &self,
        http: &HttpClient,
        cache: Option<&Cache>,
        term: &str,
        retmax: u32,
        sort: &str,
    ) -> Result<(SearchResult, Provenance, String)> {
        let retmax = retmax.to_string();
        let url = Self::url(
            "esearch",
            &[
                ("db", "pubmed"),
                ("term", term),
                ("retmax", &retmax),
                ("sort", sort),
            ],
        );
        let (body, prov) = self.get(http, cache, &url).await?;
        Ok((parse_esearch(&body)?, prov, url))
    }

    /// Check MeSH headings: returns the headings PubMed knows (exact match).
    pub async fn valid_mesh(
        &self,
        http: &HttpClient,
        cache: Option<&Cache>,
        heading: &str,
    ) -> Result<bool> {
        let clean = heading.replace('"', "");
        let term = format!("\"{}\"[mh]", clean.trim());
        let url = Self::url("esearch", &[("db", "mesh"), ("term", &term)]);
        let (body, _) = self.get(http, cache, &url).await?;
        Ok(parse_esearch(&body)?.count > 0)
    }

    /// PubMed's automatic term mapping for a phrase: the MeSH headings it maps
    /// the whole phrase to (empty when PubMed splits the phrase into words).
    pub async fn mesh_for_phrase(
        &self,
        http: &HttpClient,
        cache: Option<&Cache>,
        phrase: &str,
    ) -> Result<Vec<String>> {
        let url = Self::url(
            "esearch",
            &[("db", "pubmed"), ("term", phrase), ("retmax", "0")],
        );
        let (body, _) = self.get(http, cache, &url).await?;
        let r = parse_esearch(&body)?;
        Ok(whole_phrase_mesh(&r, phrase))
    }

    /// Fetch records as XML and parse them.
    pub async fn fetch(
        &self,
        http: &HttpClient,
        cache: Option<&Cache>,
        ids: &[String],
    ) -> Result<(Vec<Article>, Provenance)> {
        if ids.is_empty() {
            return Ok((
                Vec::new(),
                Provenance {
                    mode: SourceMode::Live,
                    as_of: chrono::Utc::now().to_rfc3339(),
                },
            ));
        }
        let joined = ids.join(",");
        let url = Self::url(
            "efetch",
            &[("db", "pubmed"), ("id", &joined), ("retmode", "xml")],
        );
        let (body, prov) = self.get(http, cache, &url).await?;
        let mut articles = parse_efetch(&body)?;
        // Keep the search order.
        articles.sort_by_key(|a| ids.iter().position(|i| *i == a.pmid).unwrap_or(usize::MAX));
        Ok((articles, prov))
    }
}

/// The text of an `<ERROR>` element in an E-utilities response, if any.
pub fn eutils_error(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let start = text.find("<ERROR>")? + "<ERROR>".len();
    let end = text[start..]
        .find("</ERROR>")
        .map_or(text.len(), |i| start + i);
    Some(text[start..end].trim().chars().take(200).collect())
}

/// Validator: an XML document that is not an E-utilities error report.
pub fn expect_eutils(body: &[u8]) -> Result<()> {
    expect_xml(body)?;
    match eutils_error(body) {
        Some(message) => Err(PhnError::Network(format!(
            "NCBI E-utilities reported an error ({message})"
        ))),
        None => Ok(()),
    }
}

/// MeSH headings PubMed mapped the whole phrase to.
pub fn whole_phrase_mesh(r: &SearchResult, phrase: &str) -> Vec<String> {
    let p = phrase.trim().to_lowercase();
    let mut out = Vec::new();
    for (from, to) in &r.translations {
        if from.trim().to_lowercase() != p {
            continue;
        }
        let mut rest = to.as_str();
        while let Some(i) = rest.find("[MeSH Terms]") {
            let before = &rest[..i];
            if let Some(start) = before.rfind('"') {
                let head = &before[..start];
                if let Some(open) = head.rfind('"') {
                    let term = &before[open + 1..start];
                    if !term.is_empty() && !out.iter().any(|t: &String| t == term) {
                        out.push(term.to_string());
                    }
                }
            }
            rest = &rest[i + "[MeSH Terms]".len()..];
        }
    }
    out
}

fn reader(body: &[u8]) -> Reader<&[u8]> {
    let mut r = Reader::from_reader(body);
    r.config_mut().trim_text(false);
    r
}

fn resolve_ref(r: &BytesRef<'_>) -> String {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        return c.to_string();
    }
    match r.xml10_content().as_ref() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
    .to_string()
}

fn xml_err(e: impl std::fmt::Display) -> PhnError {
    PhnError::Network(format!("PubMed returned XML that could not be read: {e}"))
}

/// Parse an `esearch` response.
pub fn parse_esearch(body: &[u8]) -> Result<SearchResult> {
    let mut r = reader(body);
    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut out = SearchResult {
        count: 0,
        ids: Vec::new(),
        query_translation: String::new(),
        translations: Vec::new(),
        warnings: Vec::new(),
    };
    let mut from = String::new();
    let mut seen_count = false;
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                path.push(e.local_name().as_ref().to_string());
                text.clear();
            }
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::GeneralRef(g) => text.push_str(&resolve_ref(&g)),
            Event::End(_) => {
                let name = path.pop().unwrap_or_default();
                let parent = path.last().map(String::as_str).unwrap_or("");
                let t = text.trim().to_string();
                match (name.as_str(), parent) {
                    ("Count", "eSearchResult") if !seen_count => {
                        out.count = t.parse().unwrap_or(0);
                        seen_count = true;
                    }
                    ("Id", "IdList") => out.ids.push(t),
                    ("QueryTranslation", _) => out.query_translation = t,
                    ("From", "Translation") => from = t,
                    ("To", "Translation") => out.translations.push((from.clone(), t)),
                    ("PhraseNotFound", _) => out.warnings.push(format!("phrase not found: {t}")),
                    ("OutputMessage", _) | ("QuotedPhraseNotFound", _) => out.warnings.push(t),
                    ("ERROR", _) => out.warnings.push(format!("error: {t}")),
                    _ => {}
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// Parse an `efetch` PubMed XML response.
pub fn parse_efetch(body: &[u8]) -> Result<Vec<Article>> {
    let mut r = reader(body);
    let mut path: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut cur = Article::default();
    let mut text = String::new();
    let mut last_name = String::new();
    let mut initials = String::new();
    let mut collective = String::new();
    let mut abstract_label: Option<String> = None;
    let mut id_type: Option<String> = None;
    let mut medline_date = String::new();
    // Depth of the element whose text is being collected (inline markup such
    // as <i> or <sup> inside titles and abstracts is flattened).
    let mut collect_depth: Option<usize> = None;
    loop {
        match r.read_event().map_err(xml_err)? {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_string();
                match name.as_str() {
                    "PubmedArticle" => cur = Article::default(),
                    "AbstractText" => {
                        abstract_label = e.attributes().flatten().find_map(|a| {
                            (a.key.local_name().as_ref() == "Label")
                                .then(|| {
                                    a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                        .ok()
                                        .map(|v| v.into_owned())
                                })
                                .flatten()
                        });
                    }
                    "ArticleId" => {
                        id_type = e.attributes().flatten().find_map(|a| {
                            (a.key.local_name().as_ref() == "IdType")
                                .then(|| {
                                    a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                        .ok()
                                        .map(|v| v.into_owned())
                                })
                                .flatten()
                        });
                    }
                    "Author" => {
                        last_name.clear();
                        initials.clear();
                        collective.clear();
                    }
                    _ => {}
                }
                path.push(name);
                if collect_depth.is_none() {
                    text.clear();
                    let n = path.last().map(String::as_str).unwrap_or("");
                    if matches!(n, "ArticleTitle" | "AbstractText") {
                        collect_depth = Some(path.len());
                    }
                }
            }
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::GeneralRef(g) => text.push_str(&resolve_ref(&g)),
            Event::End(_) => {
                let depth = path.len();
                let name = path.pop().unwrap_or_default();
                let in_citation = path.iter().any(|p| p == "MedlineCitation");
                let parent = path.last().map(String::as_str).unwrap_or("");
                if collect_depth.is_some_and(|d| d < depth) {
                    continue; // inline markup inside a title or abstract
                }
                collect_depth = None;
                let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
                match (name.as_str(), parent) {
                    ("PMID", "MedlineCitation") if cur.pmid.is_empty() => cur.pmid = t,
                    ("ArticleTitle", _) => cur.title = t,
                    ("Title", "Journal") => cur.journal = t,
                    ("ISOAbbreviation", "Journal") => cur.journal_abbreviation = t,
                    ("Year", "PubDate") if in_citation => cur.year = Some(t),
                    ("MedlineDate", "PubDate") if in_citation => medline_date = t,
                    ("AbstractText", _) => {
                        if !cur.abstract_text.is_empty() {
                            cur.abstract_text.push(' ');
                        }
                        if let Some(l) = abstract_label.take() {
                            cur.abstract_text
                                .push_str(&format!("{}: ", l.to_uppercase()));
                        }
                        cur.abstract_text.push_str(&t);
                    }
                    ("LastName", "Author") => last_name = t,
                    ("Initials", "Author") => initials = t,
                    ("CollectiveName", "Author") => collective = t,
                    ("Author", "AuthorList") => {
                        let a = if !collective.is_empty() {
                            collective.clone()
                        } else {
                            format!("{last_name} {initials}").trim().to_string()
                        };
                        if !a.is_empty() {
                            cur.authors.push(a);
                        }
                    }
                    ("PublicationType", _) => cur.publication_types.push(t),
                    ("Language", "Article") => cur.languages.push(t),
                    ("ELocationID", _) if cur.doi.is_none() && t.starts_with("10.") => {
                        cur.doi = Some(t)
                    }
                    ("ArticleId", "ArticleIdList") => {
                        if id_type.as_deref() == Some("doi")
                            && cur.doi.is_none()
                            && !path.iter().any(|p| p == "ReferenceList")
                        {
                            cur.doi = Some(t);
                        }
                    }
                    ("PubmedArticle", _) => {
                        if cur.year.is_none() && !medline_date.is_empty() {
                            cur.year = medline_date.get(..4).map(str::to_string);
                        }
                        medline_date.clear();
                        out.push(std::mem::take(&mut cur));
                    }
                    _ => {}
                }
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_esearch_and_term_mapping() {
        let r = parse_esearch(include_bytes!(
            "../tests/fixtures/phase4/pubmed_esearch.xml"
        ))
        .unwrap();
        assert_eq!(r.count, 69);
        assert_eq!(r.ids, vec!["42309234", "42317980", "41759747"]);
        assert!(
            r.query_translation
                .contains("\"whooping cough\"[MeSH Terms]")
        );
        let atm = parse_esearch(include_bytes!(
            "../tests/fixtures/phase4/pubmed_esearch_atm.xml"
        ))
        .unwrap();
        assert_eq!(
            whole_phrase_mesh(&atm, "Whooping cough"),
            vec!["whooping cough"]
        );
        assert!(whole_phrase_mesh(&atm, "school nurse").is_empty());
    }

    #[test]
    fn parses_efetch_records() {
        let a = parse_efetch(include_bytes!("../tests/fixtures/phase4/pubmed_efetch.xml")).unwrap();
        assert_eq!(a.len(), 2);
        let first = &a[0];
        assert_eq!(first.pmid, "42309234");
        assert!(
            first
                .title
                .starts_with("Association of asthma and COPD with pertussis risk")
        );
        assert_eq!(first.journal_abbreviation, "Respir Med");
        assert_eq!(first.year.as_deref(), Some("2026"));
        assert_eq!(first.doi.as_deref(), Some("10.1016/j.rmed.2026.108968"));
        assert!(first.abstract_text.starts_with("BACKGROUND: Pertussis"));
        // Entity references and inline markup are kept as text.
        assert!(first.abstract_text.contains("≥3 studies"));
        assert!(first.abstract_text.contains("I2"));
        assert!(!first.authors.is_empty());
        assert!(first.languages.contains(&"eng".to_string()));
        assert!(
            first
                .publication_types
                .iter()
                .any(|p| p.contains("Meta-Analysis") || p.contains("Systematic Review")),
            "{:?}",
            first.publication_types
        );
    }

    #[tokio::test]
    async fn error_reports_are_rejected_and_not_served_from_the_cache() {
        let error = br#"<?xml version="1.0" encoding="UTF-8" ?>
<eSearchResult><ERROR>Search Backend failed: read request has timed out</ERROR></eSearchResult>"#;
        assert_eq!(
            eutils_error(error).as_deref(),
            Some("Search Backend failed: read request has timed out")
        );
        assert!(expect_eutils(error).is_err());
        // A search with no matches is a real answer, not an error report.
        let empty = br#"<?xml version="1.0" encoding="UTF-8" ?>
<eSearchResult><Count>0</Count><IdList/><ErrorList><PhraseNotFound>zzzz</PhraseNotFound></ErrorList></eSearchResult>"#;
        assert!(expect_eutils(empty).is_ok());
        assert!(
            expect_eutils(include_bytes!(
                "../tests/fixtures/phase4/pubmed_esearch.xml"
            ))
            .is_ok()
        );
        assert!(
            expect_eutils(include_bytes!("../tests/fixtures/phase4/pubmed_efetch.xml")).is_ok()
        );

        // An error report cached by an earlier version is not used.
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&phn_core::Config::for_root(dir.path())).unwrap();
        let cache = Cache::in_memory().unwrap();
        let url = Eutils::url(
            "esearch",
            &[
                ("db", "pubmed"),
                ("term", "pertussis"),
                ("retmax", "5"),
                ("sort", "relevance"),
            ],
        );
        cache.put(&url, error, TTL_SECS).unwrap();
        let eutils = Eutils { key: None };
        assert!(
            eutils
                .search(&http, Some(&cache), "pertussis", 5, "relevance")
                .await
                .is_err()
        );
    }

    #[test]
    fn urls_carry_tool_and_no_email() {
        let u = Eutils::url("esearch", &[("db", "pubmed"), ("term", "a b[mh]")]);
        assert!(u.ends_with("tool=public-health-nursing-mcp-server"));
        assert!(u.contains("term=a%20b%5Bmh%5D"));
        assert!(!u.contains("email"));
    }
}
