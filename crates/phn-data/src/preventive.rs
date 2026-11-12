//! Data for `preventive_services_lookup` (CHR-04).
//!
//! * The bundled `uspstf-ab-recommendations` snapshot: USPSTF A and B
//!   recommendations (grade, date, population, recommendation text) copied from
//!   the Task Force's published recommendation statements, read through PubMed
//!   by `xtask/scripts/build_prevent_snapshots.py`. The USPSTF web site and
//!   API were not reachable from the build network, so there is no USPSTF
//!   adapter; the API is documented as an open item in STATUS.
//! * ODPHP's MyHealthfinder API (v4, no key), the federal list of preventive
//!   services by age, sex, and pregnancy that is built from the USPSTF, ACIP,
//!   and HRSA recommendations. It is fetched live as a cross-check and has no
//!   offline copy (its responses are several hundred kilobytes per query).

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::apifetch::fetch_validated;
use crate::cache::Cache;
use crate::fetch::Fetched;
use crate::http::HttpClient;
use crate::snapshots::{SnapshotEntry, SnapshotStore};

pub const SNAPSHOT_ID: &str = "uspstf-ab-recommendations";
pub const FILE: &str = "uspstf_ab_recommendations.json";
/// MyHealthfinder API (version 4).
pub const MYHF_BASE: &str = "https://odphp.health.gov/myhealthfinder/api/v4/myhealthfinder.json";
/// MyHealthfinder home page.
pub const MYHF_HOME: &str = "https://odphp.health.gov/myhealthfinder";
/// Cache time for MyHealthfinder answers (one week).
pub const MYHF_TTL_SECS: u64 = 7 * 24 * 3600;

/// One USPSTF recommendation statement (the journal article).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Statement {
    pub pmid: String,
    pub title: String,
    pub journal: String,
    /// Journal publication date (YYYY-MM-DD).
    pub published: String,
    pub doi: String,
    /// The statement's population text, when its abstract has one.
    pub population: Option<String>,
    pub url: String,
    pub pubmed_url: String,
}

/// One graded recommendation within a statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Recommendation {
    pub id: String,
    pub topic: String,
    /// "A" or "B".
    pub grade: String,
    pub statement_pmid: String,
    /// The recommendation sentence(s) as written in the statement's abstract.
    pub recommendation: String,
}

/// The bundled snapshot file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UspstfSnapshot {
    pub format_version: u32,
    pub description: String,
    pub retrieved: String,
    pub not_included: Vec<String>,
    pub statements: Vec<Statement>,
    pub recommendations: Vec<Recommendation>,
}

impl UspstfSnapshot {
    /// The statement behind a recommendation.
    pub fn statement(&self, rec: &Recommendation) -> Option<&Statement> {
        self.statements.iter().find(|s| s.pmid == rec.statement_pmid)
    }
}

/// Parse the snapshot text and check it is consistent.
pub fn parse_snapshot(text: &str) -> Result<UspstfSnapshot> {
    let snap: UspstfSnapshot = serde_json::from_str(text)
        .map_err(|e| PhnError::Internal(format!("USPSTF snapshot: {e}")))?;
    for r in &snap.recommendations {
        if !matches!(r.grade.as_str(), "A" | "B") {
            return Err(PhnError::Internal(format!(
                "USPSTF snapshot: {} has grade {}",
                r.id, r.grade
            )));
        }
        if snap.statement(r).is_none() {
            return Err(PhnError::Internal(format!(
                "USPSTF snapshot: {} cites statement {} that is not in the file",
                r.id, r.statement_pmid
            )));
        }
    }
    Ok(snap)
}

/// Load the bundled (or newer cached) snapshot with its manifest entry.
pub fn load_snapshot(store: &SnapshotStore) -> Result<(UspstfSnapshot, SnapshotEntry)> {
    let (text, entry, _) = store.file_text(SNAPSHOT_ID, FILE)?;
    Ok((parse_snapshot(&text)?, entry))
}

// ---------------------------------------------------------------- MyHealthfinder

/// One topic MyHealthfinder lists for a person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MyHfTopic {
    /// MyHealthfinder's topic title ("Get Screened for Colorectal Cancer").
    pub title: String,
    /// Its short label in the personalised list ("Colorectal cancer screening").
    pub short_title: String,
    /// "Screenings/visits/vaccines", "Concerned about", "At risk for", and so on.
    pub category: String,
    pub url: Option<String>,
    /// Date MyHealthfinder last updated the topic (YYYY-MM-DD).
    pub last_update: Option<String>,
}

/// MyHealthfinder's sex values.
fn sex_param(sex: &str) -> &'static str {
    if sex.eq_ignore_ascii_case("male") {
        "male"
    } else {
        "female"
    }
}

/// The request URL for an age (years), sex ("female" or "male"), and pregnancy.
pub fn myhf_url(age_years: u32, sex: &str, pregnant: bool) -> String {
    let mut url = format!(
        "{MYHF_BASE}?lang=en&age={age_years}&sex={}",
        sex_param(sex)
    );
    if pregnant {
        url.push_str("&pregnant=yes");
    }
    url
}

fn expect_myhf(body: &[u8]) -> Result<()> {
    parse_myhf(body).map(|_| ())
}

/// Parse a MyHealthfinder response into its topics (links and promos without a
/// topic title are skipped).
pub fn parse_myhf(body: &[u8]) -> Result<Vec<MyHfTopic>> {
    let v: serde_json::Value = serde_json::from_slice(body)
        .map_err(|_| PhnError::Network("MyHealthfinder did not answer with JSON".into()))?;
    let result = v
        .get("Result")
        .ok_or_else(|| PhnError::Network("MyHealthfinder answer has no Result".into()))?;
    if result.get("Error").and_then(|e| e.as_str()) != Some("False") {
        return Err(PhnError::Network(
            "MyHealthfinder reported an error for this request".into(),
        ));
    }
    let items = result
        .pointer("/Resources/All/Resource")
        .and_then(|r| r.as_array())
        .ok_or_else(|| PhnError::Network("MyHealthfinder answer has no resources".into()))?;
    let text = |item: &serde_json::Value, key: &str| {
        item.get(key)
            .and_then(|x| x.as_str())
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for item in items {
        if text(item, "Type") != "Topic" {
            continue;
        }
        let title = text(item, "Title");
        if title.is_empty() {
            continue;
        }
        let last_update = text(item, "LastUpdate")
            .parse::<i64>()
            .ok()
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
            .map(|t| t.date_naive().to_string())
            // Dates outside the plausible range are not shown.
            .filter(|d| d.as_str() >= "2000-01-01" && d.as_str() <= "2100-12-31");
        let url = Some(text(item, "AccessibleVersion"))
            .filter(|u| u.starts_with("https://odphp.health.gov/"));
        out.push(MyHfTopic {
            title,
            short_title: text(item, "MyHFTitle"),
            category: text(item, "MyHFCategory"),
            url,
            last_update,
        });
    }
    Ok(out)
}

/// Fetch the MyHealthfinder topics for a person (live, or the cached answer to
/// the same question). There is no snapshot, so offline this fails.
pub async fn fetch_myhf(
    http: &HttpClient,
    cache: Option<&Cache>,
    age_years: u32,
    sex: &str,
    pregnant: bool,
) -> Result<(Vec<MyHfTopic>, Fetched)> {
    let url = myhf_url(age_years, sex, pregnant);
    let fetched = fetch_validated(
        http,
        cache,
        &url,
        None,
        MYHF_TTL_SECS,
        &expect_myhf,
        None,
    )
    .await?;
    let topics = parse_myhf(&fetched.body)?;
    Ok((topics, fetched))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshot_is_consistent() {
        let (snap, entry) = load_snapshot(&SnapshotStore::bundled()).unwrap();
        assert_eq!(entry.id, SNAPSHOT_ID);
        assert!(snap.recommendations.len() >= 50);
        // Every grade tag in the sentence agrees with the grade field.
        for r in &snap.recommendations {
            let tag = format!("({} recommendation)", r.grade);
            assert!(r.recommendation.contains(&tag), "{}: {}", r.id, r.recommendation);
            let s = snap.statement(r).unwrap();
            assert!(s.published.as_str() >= "2016-01-01", "{}", r.id);
            assert!(s.url.starts_with("https://doi.org/10."));
        }
        let ids: std::collections::BTreeSet<_> =
            snap.recommendations.iter().map(|r| &r.id).collect();
        assert_eq!(ids.len(), snap.recommendations.len());
    }

    #[test]
    fn colorectal_grades_match_the_statement() {
        let (snap, _) = load_snapshot(&SnapshotStore::bundled()).unwrap();
        let a = snap
            .recommendations
            .iter()
            .find(|r| r.id == "colorectal_cancer_screening_50_75")
            .unwrap();
        let b = snap
            .recommendations
            .iter()
            .find(|r| r.id == "colorectal_cancer_screening_45_49")
            .unwrap();
        assert_eq!((a.grade.as_str(), b.grade.as_str()), ("A", "B"));
        assert_eq!(snap.statement(a).unwrap().published, "2021-05-18");
    }

    #[test]
    fn myhf_url_has_pregnancy_only_when_asked() {
        assert!(myhf_url(30, "female", true).ends_with("&age=30&sex=female&pregnant=yes"));
        assert!(!myhf_url(66, "male", false).contains("pregnant"));
    }

    #[test]
    fn parses_a_myhf_answer() {
        let body = br#"{"Result":{"Error":"False","Total":2,"Resources":{"All":{"Resource":[
            {"Type":"Link","Id":"","Title":"","MyHFTitle":"Promo","MyHFCategory":"X"},
            {"Type":"Topic","Id":"538","Title":"Get Screened for Colorectal Cancer",
             "MyHFTitle":"Colorectal cancer screening ","MyHFCategory":"Screenings/visits/vaccines",
             "LastUpdate":"1783436370",
             "AccessibleVersion":"https://odphp.health.gov/myhealthfinder/doctor-visits/screening-tests/get-screened-colorectal-cancer"},
            {"Type":"Topic","Id":"1","Title":"Other","MyHFTitle":"Other","MyHFCategory":"Y",
             "LastUpdate":"x","AccessibleVersion":"http://evil.example/"}
        ]}}}}"#;
        let t = parse_myhf(body).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].short_title, "Colorectal cancer screening");
        assert_eq!(t[0].last_update.as_deref(), Some("2026-07-07"));
        assert!(t[0].url.as_deref().unwrap().starts_with("https://odphp.health.gov/"));
        // A link outside ODPHP is not passed on.
        assert_eq!(t[1].url, None);
        assert_eq!(t[1].last_update, None);
    }

    #[test]
    fn rejects_error_and_non_json_answers() {
        assert!(parse_myhf(b"<html>blocked</html>").is_err());
        assert!(parse_myhf(br#"{"Result":{"Error":"True"}}"#).is_err());
        assert!(parse_myhf(br#"{"Result":{"Error":"False"}}"#).is_err());
    }
}
