//! Medication reference adapters: openFDA (drug labels, enforcement reports,
//! drug shortages), DailyMed (Structured Product Labels), and RxNorm (RxNav).
//!
//! These are URL builders and response parsers; the tool layer runs the
//! requests through [`crate::fetch::fetch_with_fallback`] so answers come from
//! the live API or the local cache, and says which. There is no bundled
//! snapshot: medication labels change too often for one to be useful, so
//! offline answers say what could not be checked.
//!
//! An optional openFDA key (`PHN_OPENFDA_API_KEY`) raises openFDA's rate
//! limit; everything works without it. The key is appended to the request URL
//! and never returned in any response.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::socrata::encode;

/// Base URLs, overridable for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub openfda: String,
    pub dailymed: String,
    pub rxnav: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            openfda: "https://api.fda.gov".into(),
            dailymed: "https://dailymed.nlm.nih.gov/dailymed/services/v2".into(),
            rxnav: "https://rxnav.nlm.nih.gov/REST".into(),
        }
    }
}

/// Quote a term for an openFDA search (double quotes removed from the term).
fn openfda_phrase(term: &str) -> String {
    let clean: String = term
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | ':' | '(' | ')' | '[' | ']'))
        .collect();
    format!("\"{}\"", clean.trim())
}

fn with_key(mut url: String, api_key: Option<&str>) -> String {
    if let Some(k) = api_key.filter(|k| !k.trim().is_empty()) {
        url.push_str("&api_key=");
        url.push_str(&encode(k.trim()));
    }
    url
}

impl Endpoints {
    /// openFDA drug label search by brand or generic name.
    pub fn openfda_label(&self, term: &str, limit: u32, api_key: Option<&str>) -> String {
        let p = openfda_phrase(term);
        let search = format!("openfda.brand_name:{p}+openfda.generic_name:{p}");
        with_key(
            format!(
                "{}/drug/label.json?search={}&limit={limit}",
                self.openfda,
                encode(&search).replace("%2B", "+")
            ),
            api_key,
        )
    }

    /// openFDA drug enforcement (recall) reports mentioning the term, newest first.
    pub fn openfda_recalls(&self, term: &str, limit: u32, api_key: Option<&str>) -> String {
        let search = format!("product_description:{}", openfda_phrase(term));
        with_key(
            format!(
                "{}/drug/enforcement.json?search={}&sort=report_date:desc&limit={limit}",
                self.openfda,
                encode(&search)
            ),
            api_key,
        )
    }

    /// openFDA drug shortage records for a generic name.
    pub fn openfda_shortages(&self, term: &str, limit: u32, api_key: Option<&str>) -> String {
        let search = format!("generic_name:{}", openfda_phrase(term));
        with_key(
            format!(
                "{}/drug/shortages.json?search={}&limit={limit}",
                self.openfda,
                encode(&search)
            ),
            api_key,
        )
    }

    /// DailyMed SPL search by drug name.
    pub fn dailymed_spls(&self, term: &str, page_size: u32) -> String {
        format!(
            "{}/spls.json?drug_name={}&pagesize={page_size}",
            self.dailymed,
            encode(term.trim())
        )
    }

    /// RxNav approximate match.
    pub fn rxnav_approximate(&self, term: &str, max: u32) -> String {
        format!(
            "{}/approximateTerm.json?term={}&maxEntries={max}",
            self.rxnav,
            encode(term.trim())
        )
    }

    /// RxNav concept properties.
    pub fn rxnav_properties(&self, rxcui: &str) -> String {
        format!("{}/rxcui/{}/properties.json", self.rxnav, encode(rxcui))
    }

    /// RxNav related concepts (ingredients, brand names, clinical and branded drugs).
    pub fn rxnav_related(&self, rxcui: &str) -> String {
        format!(
            "{}/rxcui/{}/related.json?tty=IN+BN+SCD+SBD",
            self.rxnav,
            encode(rxcui)
        )
    }
}

/// Public DailyMed page for a set ID.
pub fn dailymed_setid_url(setid: &str) -> String {
    format!("https://dailymed.nlm.nih.gov/dailymed/drugInfo.cfm?setid={setid}")
}

/// A drug label summary from openFDA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LabelSummary {
    pub brand_names: Vec<String>,
    pub generic_names: Vec<String>,
    pub manufacturer: Option<String>,
    pub product_type: Option<String>,
    pub routes: Vec<String>,
    /// SPL set ID (stable across label versions).
    pub set_id: Option<String>,
    /// SPL version number.
    pub version: Option<String>,
    /// Label effective date (YYYY-MM-DD).
    pub effective_date: Option<String>,
    pub rxcuis: Vec<String>,
    /// Short excerpts of key sections (first ~600 characters each).
    pub boxed_warning: Option<String>,
    pub indications: Option<String>,
    pub dosage_and_administration: Option<String>,
    pub contraindications: Option<String>,
    pub warnings: Option<String>,
    pub storage_and_handling: Option<String>,
}

/// A recall (enforcement report) from openFDA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RecallSummary {
    pub recall_number: String,
    pub classification: String,
    pub status: String,
    pub product_description: String,
    pub reason_for_recall: String,
    pub recalling_firm: String,
    pub recall_initiation_date: Option<String>,
    pub report_date: Option<String>,
}

/// A drug shortage record from openFDA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ShortageSummary {
    pub generic_name: String,
    pub status: String,
    pub presentation: Option<String>,
    pub company: Option<String>,
    pub dosage_form: Option<String>,
    pub related_info: Option<String>,
    pub initial_posting_date: Option<String>,
    pub update_date: Option<String>,
}

/// A DailyMed SPL listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SplSummary {
    pub title: String,
    pub setid: String,
    pub spl_version: Option<i64>,
    pub published_date: Option<String>,
    pub url: String,
}

/// An RxNorm concept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RxConcept {
    pub rxcui: String,
    pub name: String,
    /// RxNorm term type (IN ingredient, BN brand, SCD clinical drug, SBD branded drug).
    pub tty: String,
}

fn strs(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn first_text(v: &Value, key: &str, max: usize) -> Option<String> {
    let t = strs(&v[key]).into_iter().next()?;
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(truncate(&t, max))
}

/// Cut text at a character boundary, adding an ellipsis.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// openFDA dates are YYYYMMDD; DailyMed and shortages use other forms.
pub fn iso_date(s: &str) -> Option<String> {
    let t = s.trim();
    if t.len() == 8 && t.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{}-{}-{}", &t[..4], &t[4..6], &t[6..]));
    }
    if let Some((m, rest)) = t.split_once('/')
        && let Some((d, y)) = rest.split_once('/')
        && y.len() == 4
    {
        return Some(format!("{y}-{m:0>2}-{d:0>2}"));
    }
    chrono::NaiveDate::parse_from_str(t, "%b %d, %Y")
        .ok()
        .map(|d| d.to_string())
        .or_else(|| (!t.is_empty()).then(|| t.to_string()))
}

/// Total number of matches reported by an openFDA response.
pub fn openfda_total(v: &Value) -> u64 {
    v["meta"]["results"]["total"].as_u64().unwrap_or(0)
}

/// openFDA's data date (`meta.last_updated`).
pub fn openfda_last_updated(v: &Value) -> Option<String> {
    v["meta"]["last_updated"].as_str().map(str::to_string)
}

/// Parse an openFDA label response.
pub fn parse_labels(v: &Value) -> Vec<LabelSummary> {
    let Some(results) = v["results"].as_array() else {
        return Vec::new();
    };
    results
        .iter()
        .map(|r| {
            let o = &r["openfda"];
            LabelSummary {
                brand_names: strs(&o["brand_name"]),
                generic_names: strs(&o["generic_name"]),
                manufacturer: strs(&o["manufacturer_name"]).into_iter().next(),
                product_type: strs(&o["product_type"]).into_iter().next(),
                routes: strs(&o["route"]),
                set_id: r["set_id"].as_str().map(str::to_string),
                version: r["version"].as_str().map(str::to_string),
                effective_date: r["effective_time"].as_str().and_then(iso_date),
                rxcuis: strs(&o["rxcui"]),
                boxed_warning: first_text(r, "boxed_warning", 600),
                indications: first_text(r, "indications_and_usage", 600),
                dosage_and_administration: first_text(r, "dosage_and_administration", 600),
                contraindications: first_text(r, "contraindications", 600),
                warnings: first_text(r, "warnings_and_cautions", 600)
                    .or_else(|| first_text(r, "warnings", 600)),
                storage_and_handling: first_text(r, "storage_and_handling", 400)
                    .or_else(|| first_text(r, "how_supplied", 400)),
            }
        })
        .collect()
}

/// Parse an openFDA enforcement response.
pub fn parse_recalls(v: &Value) -> Vec<RecallSummary> {
    let Some(results) = v["results"].as_array() else {
        return Vec::new();
    };
    let s = |r: &Value, k: &str| r[k].as_str().unwrap_or("").trim().to_string();
    results
        .iter()
        .map(|r| RecallSummary {
            recall_number: s(r, "recall_number"),
            classification: s(r, "classification"),
            status: s(r, "status"),
            product_description: truncate(&s(r, "product_description"), 300),
            reason_for_recall: truncate(&s(r, "reason_for_recall"), 300),
            recalling_firm: s(r, "recalling_firm"),
            recall_initiation_date: r["recall_initiation_date"].as_str().and_then(iso_date),
            report_date: r["report_date"].as_str().and_then(iso_date),
        })
        .collect()
}

/// Parse an openFDA drug shortage response.
pub fn parse_shortages(v: &Value) -> Vec<ShortageSummary> {
    let Some(results) = v["results"].as_array() else {
        return Vec::new();
    };
    let o = |r: &Value, k: &str| r[k].as_str().map(|x| truncate(x.trim(), 300));
    results
        .iter()
        .map(|r| ShortageSummary {
            generic_name: o(r, "generic_name").unwrap_or_default(),
            status: o(r, "status").unwrap_or_default(),
            presentation: o(r, "presentation"),
            company: o(r, "company_name"),
            dosage_form: o(r, "dosage_form"),
            related_info: o(r, "related_info"),
            initial_posting_date: r["initial_posting_date"].as_str().and_then(iso_date),
            update_date: r["update_date"].as_str().and_then(iso_date),
        })
        .collect()
}

/// Parse a DailyMed `spls.json` response.
pub fn parse_spls(v: &Value) -> Vec<SplSummary> {
    let Some(data) = v["data"].as_array() else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|d| {
            let setid = d["setid"].as_str()?.to_string();
            Some(SplSummary {
                title: d["title"].as_str().unwrap_or("").to_string(),
                url: dailymed_setid_url(&setid),
                setid,
                spl_version: d["spl_version"].as_i64(),
                published_date: d["published_date"].as_str().and_then(iso_date),
            })
        })
        .collect()
}

/// DailyMed total matches.
pub fn dailymed_total(v: &Value) -> u64 {
    v["metadata"]["total_elements"].as_u64().unwrap_or(0)
}

/// Best RxCUI candidates from an approximate-term response, de-duplicated,
/// in rank order.
pub fn parse_approximate(v: &Value) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    if let Some(c) = v["approximateGroup"]["candidate"].as_array() {
        for cand in c {
            let Some(id) = cand["rxcui"].as_str() else {
                continue;
            };
            let name = cand["name"].as_str().map(str::to_string);
            match out.iter_mut().find(|(r, _)| r == id) {
                Some(existing) => {
                    if existing.1.is_none() {
                        existing.1 = name;
                    }
                }
                None => out.push((id.to_string(), name)),
            }
        }
    }
    out
}

/// Parse `properties.json`.
pub fn parse_properties(v: &Value) -> Option<RxConcept> {
    let p = &v["properties"];
    Some(RxConcept {
        rxcui: p["rxcui"].as_str()?.to_string(),
        name: p["name"].as_str()?.to_string(),
        tty: p["tty"].as_str().unwrap_or("").to_string(),
    })
}

/// Parse `related.json` into concepts grouped by term type.
pub fn parse_related(v: &Value) -> Vec<RxConcept> {
    let mut out = Vec::new();
    if let Some(groups) = v["relatedGroup"]["conceptGroup"].as_array() {
        for g in groups {
            if let Some(props) = g["conceptProperties"].as_array() {
                for p in props {
                    if let (Some(id), Some(name)) = (p["rxcui"].as_str(), p["name"].as_str()) {
                        out.push(RxConcept {
                            rxcui: id.to_string(),
                            name: name.to_string(),
                            tty: p["tty"].as_str().unwrap_or("").to_string(),
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let text = match name {
            "label" => include_str!("../tests/fixtures/medication/openfda_label.json"),
            "recall" => include_str!("../tests/fixtures/medication/openfda_enforcement.json"),
            "shortage" => include_str!("../tests/fixtures/medication/openfda_shortages.json"),
            "spls" => include_str!("../tests/fixtures/medication/dailymed_spls.json"),
            "approx" => include_str!("../tests/fixtures/medication/rxnav_approximate.json"),
            "props" => include_str!("../tests/fixtures/medication/rxnav_properties.json"),
            "related" => include_str!("../tests/fixtures/medication/rxnav_related.json"),
            _ => unreachable!(),
        };
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn builds_urls_with_encoding_and_optional_key() {
        let e = Endpoints::default();
        let u = e.openfda_label("amoxicillin \"x\"", 2, None);
        assert!(u.starts_with("https://api.fda.gov/drug/label.json?search=openfda.brand_name"));
        assert!(!u.contains("api_key"));
        assert!(u.contains("limit=2"));
        let k = e.openfda_recalls("amoxicillin", 3, Some("abc 123"));
        assert!(k.ends_with("&api_key=abc%20123"));
        assert!(e.rxnav_related("723").contains("/rxcui/723/related.json"));
        assert!(
            e.dailymed_spls("fluzone high-dose", 5)
                .contains("drug_name=fluzone%20high-dose")
        );
    }

    #[test]
    fn parses_real_responses() {
        let labels = parse_labels(&fixture("label"));
        assert_eq!(labels.len(), 1);
        let l = &labels[0];
        assert_eq!(l.effective_date.as_deref(), Some("2026-07-23"));
        assert_eq!(
            l.set_id.as_deref(),
            Some("00b86913-50c8-443f-8467-f4f499d358af")
        );
        assert!(l.indications.as_ref().unwrap().chars().count() <= 601);
        let recalls = parse_recalls(&fixture("recall"));
        assert_eq!(recalls[0].recall_number, "D-0151-2026");
        assert_eq!(recalls[0].report_date.as_deref(), Some("2025-11-12"));
        let sh = parse_shortages(&fixture("shortage"));
        assert_eq!(sh[0].status, "To Be Discontinued");
        assert_eq!(sh[0].update_date.as_deref(), Some("2026-08-13"));
        let spls = parse_spls(&fixture("spls"));
        assert_eq!(spls[0].published_date.as_deref(), Some("2026-11-05"));
        assert!(spls[0].url.contains("setid="));
        let approx = parse_approximate(&fixture("approx"));
        assert_eq!(approx[0].0, "723");
        assert_eq!(approx.len(), 1);
        assert_eq!(parse_properties(&fixture("props")).unwrap().tty, "IN");
        assert!(
            parse_related(&fixture("related"))
                .iter()
                .any(|c| c.tty == "BN")
        );
    }

    #[test]
    fn dates_normalize() {
        assert_eq!(iso_date("20251112").as_deref(), Some("2025-11-12"));
        assert_eq!(iso_date("08/13/2026").as_deref(), Some("2026-08-13"));
        assert_eq!(iso_date("Nov 05, 2026").as_deref(), Some("2026-11-05"));
    }
}
