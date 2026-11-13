//! Massachusetts Legislature public API (`malegislature.gov/api`).
//!
//! The API needs no key and the requests carry no personal data: they name a
//! General Court number and a bill number, nothing else. The endpoints used
//! (all `GET`, read from the API's own Swagger document, 2026-11-12):
//!
//! * `/GeneralCourts/{n}/Documents`: every bill and docket of a General Court
//!   (about 12 MB for the 194th; no search parameters exist, so the tool
//!   filters titles itself). Cached for one day.
//! * `/GeneralCourts/{n}/Documents/{number}`: one bill with its text,
//!   pinslip, and committee recommendations.
//! * `/GeneralCourts/{n}/Documents/{number}/DocumentHistoryActions`: the
//!   bill's history (referrals, hearings, votes, reporting dates).
//!
//! Parsing is tolerant of missing fields but refuses bodies that are not the
//! expected shape, so a proxy block page is never cached or shown as data.

use phn_core::{PhnError, Result, SourceMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apifetch::{FetchOptions, fetch_with_options};
use crate::cache::Cache;
use crate::http::HttpClient;

/// Base URL of the public API.
pub const BASE: &str = "https://malegislature.gov/api";
/// The legislature's public web site (bill pages).
pub const SITE: &str = "https://malegislature.gov";
/// The General Court in session in 2025-2026.
pub const CURRENT_GENERAL_COURT: u32 = 194;
/// Time to live of the cached bill list: one day.
pub const LIST_TTL_SECS: u64 = 24 * 3600;
/// Longest bill text kept (characters); long bills are cut.
pub const MAX_TEXT_CHARS: usize = 60_000;

/// One row of the General Court's document list (a bill that has a number).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillListing {
    /// For example "H3444" or "S1234".
    pub bill_number: String,
    pub docket_number: Option<String>,
    pub title: String,
    pub primary_sponsor: Option<String>,
    pub cosponsor_count: usize,
}

/// A bill's detail record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillDetail {
    pub bill_number: String,
    pub title: String,
    /// "Bill", "Resolve", and so on.
    pub legislation_type: Option<String>,
    /// The "pinslip": who filed it and the committee it was filed to.
    pub pinslip: Option<String>,
    /// Bill text (cut at [`MAX_TEXT_CHARS`] characters).
    pub text: String,
    /// Committee codes in the committee recommendations (for example "J14").
    pub committee_codes: Vec<String>,
    /// The action recorded for each committee recommendation (for example "Accompanied").
    pub committee_actions: Vec<String>,
}

/// One entry of a bill's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryAction {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// "House", "Senate", or "Joint".
    pub branch: String,
    pub action: String,
    pub is_stricken: bool,
}

/// The bill list with where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub bills: Vec<BillListing>,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
}

/// Normalize a bill number as a nurse might write it ("h.3444", "S 1234",
/// "H3444") to `H3444`. Returns `None` for anything else.
pub fn normalize_bill_number(input: &str) -> Option<String> {
    let s: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.' && *c != ',')
        .collect();
    let mut chars = s.chars();
    let first = chars.next()?.to_ascii_uppercase();
    if first != 'H' && first != 'S' {
        return None;
    }
    let digits: String = chars.collect();
    let ok = !digits.is_empty() && digits.len() <= 5 && digits.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| format!("{first}{}", digits.trim_start_matches('0')))
        .filter(|n| n.len() > 1)
}

/// The legislature's web page for a bill.
pub fn bill_url(general_court: u32, bill_number: &str) -> String {
    format!("{SITE}/Bills/{general_court}/{bill_number}")
}

/// The first ten characters of an ISO date-time, accepted only when they are
/// a real date between 1900 and 2100.
pub fn iso_date(s: &str) -> Option<String> {
    let head: String = s.trim().chars().take(10).collect();
    let d = chrono::NaiveDate::parse_from_str(&head, "%Y-%m-%d").ok()?;
    let year = chrono::Datelike::year(&d);
    ((1900..=2100).contains(&year) && head.len() == 10).then_some(head)
}

fn text(v: Option<&Value>) -> Option<String> {
    let t = v?.as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then_some(t)
}

/// Parse the document list. Items without a bill number (dockets that were
/// never filed as a bill) are left out.
pub fn parse_listing(body: &[u8]) -> Result<Vec<BillListing>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("the Legislature's list is not JSON ({e})")))?;
    let arr = v
        .as_array()
        .ok_or_else(|| PhnError::Network("the Legislature's list is not a list".into()))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(number) = text(item.get("BillNumber")).and_then(|n| normalize_bill_number(&n))
        else {
            continue;
        };
        let Some(title) = text(item.get("Title")) else {
            continue;
        };
        out.push(BillListing {
            bill_number: number,
            docket_number: text(item.get("DocketNumber")),
            title,
            primary_sponsor: item
                .get("PrimarySponsor")
                .and_then(|s| text(s.get("Name"))),
            cosponsor_count: item
                .get("Cosponsors")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        });
    }
    if out.is_empty() && !arr.is_empty() {
        return Err(PhnError::Network(
            "the Legislature's list has no bills in the expected shape".into(),
        ));
    }
    Ok(out)
}

/// Parse a bill detail record.
pub fn parse_detail(body: &[u8]) -> Result<BillDetail> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("the bill record is not JSON ({e})")))?;
    let number = text(v.get("BillNumber"))
        .and_then(|n| normalize_bill_number(&n))
        .ok_or_else(|| PhnError::Network("the bill record has no bill number".into()))?;
    let title = text(v.get("Title"))
        .ok_or_else(|| PhnError::Network("the bill record has no title".into()))?;
    let raw_text = v
        .get("DocumentText")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let body_text: String = raw_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TEXT_CHARS)
        .collect();
    let recs = v.get("CommitteeRecommendations").and_then(Value::as_array);
    let mut codes = Vec::new();
    let mut actions = Vec::new();
    for r in recs.into_iter().flatten() {
        if let Some(c) = r
            .get("Committee")
            .and_then(|c| text(c.get("CommitteeCode")))
        {
            codes.push(c);
        }
        if let Some(a) = text(r.get("Action")) {
            actions.push(a);
        }
    }
    Ok(BillDetail {
        bill_number: number,
        title,
        legislation_type: text(v.get("LegislationTypeName")),
        pinslip: text(v.get("Pinslip")),
        text: body_text,
        committee_codes: codes,
        committee_actions: actions,
    })
}

/// Parse a bill's history, oldest first (same-day entries keep their order).
pub fn parse_history(body: &[u8]) -> Result<Vec<HistoryAction>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("the bill history is not JSON ({e})")))?;
    let arr = v
        .as_array()
        .ok_or_else(|| PhnError::Network("the bill history is not a list".into()))?;
    let mut out: Vec<HistoryAction> = arr
        .iter()
        .filter_map(|a| {
            Some(HistoryAction {
                date: iso_date(a.get("Date")?.as_str()?)?,
                branch: text(a.get("Branch")).unwrap_or_default(),
                action: text(a.get("Action"))?,
                is_stricken: a
                    .get("IsStricken")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(out)
}

/// Validator for the bill list: a JSON array whose first element has a title
/// (checked without parsing the whole 12 MB body).
fn expect_listing(body: &[u8]) -> Result<()> {
    let start = body.iter().find(|b| !b.is_ascii_whitespace()).copied();
    if start != Some(b'[') {
        return Err(PhnError::Network(
            "the server did not return the bill list".into(),
        ));
    }
    let head = String::from_utf8_lossy(&body[..body.len().min(4096)]).into_owned();
    if body.len() > 2 && !head.contains("\"Title\"") {
        return Err(PhnError::Network(
            "the bill list is not in the expected shape".into(),
        ));
    }
    Ok(())
}

/// URL of the document list for a General Court.
pub fn listing_url(base: &str, general_court: u32) -> String {
    format!("{base}/GeneralCourts/{general_court}/Documents")
}

/// Fetch the document list: fresh cache, live, then a stale cached copy.
/// There is no snapshot here; the caller falls back to its own.
pub async fn fetch_listing(
    http: &HttpClient,
    cache: Option<&Cache>,
    base: &str,
    general_court: u32,
) -> Result<Listing> {
    let url = listing_url(base, general_court);
    let f = fetch_with_options(
        http,
        cache,
        &url,
        LIST_TTL_SECS,
        &expect_listing,
        None,
        FetchOptions::default(),
    )
    .await
    .map_err(|f| f.error)?;
    Ok(Listing {
        bills: parse_listing(&f.body)?,
        mode: f.mode,
        as_of: f.as_of,
        fallback_reason: f.fallback_reason,
    })
}

/// Fetch one bill's record and history (live only; small responses).
pub async fn fetch_bill(
    http: &HttpClient,
    base: &str,
    general_court: u32,
    bill_number: &str,
) -> Result<(BillDetail, Vec<HistoryAction>)> {
    let n = normalize_bill_number(bill_number)
        .ok_or_else(|| PhnError::InvalidInput(format!("'{bill_number}' is not a bill number")))?;
    let url = format!("{base}/GeneralCourts/{general_court}/Documents/{n}");
    let detail = match http.get_bytes_status(&url).await {
        Ok(b) => parse_detail(&b)?,
        Err(f) if f.status == Some(404) => {
            return Err(PhnError::NotFound(format!(
                "the Legislature has no bill {n} in the {general_court}th General Court"
            )));
        }
        Err(f) => return Err(PhnError::Network(f.message)),
    };
    let hist = http
        .get_bytes(&format!("{url}/DocumentHistoryActions"))
        .await
        .and_then(|b| parse_history(&b))?;
    Ok((detail, hist))
}

#[cfg(test)]
mod tests {
    use super::*;
    use phn_core::Config;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const LIST: &str = r#"[
      {"BillNumber":"H3444","DocketNumber":"HD2050","Title":"An Act relative to s-license compliance",
       "PrimarySponsor":{"Id":"X1","Name":"A. Sponsor"},"Cosponsors":[{"Id":"X1"},{"Id":"X2"}],
       "JointSponsor":null,"GeneralCourtNumber":194,"IsDocketBookOnly":false},
      {"BillNumber":null,"DocketNumber":"HD903","Title":"An Act on pollinator plantings",
       "PrimarySponsor":null,"Cosponsors":[],"GeneralCourtNumber":194,"IsDocketBookOnly":false}
    ]"#;
    const DETAIL: &str = r#"{"Title":"An Act relative to vaccine access","BillNumber":"S12",
      "DocketNumber":"SD1","GeneralCourtNumber":194,"LegislationTypeName":"Bill",
      "Pinslip":"By Ms. Example, a petition relative to vaccine access. Public Health.",
      "DocumentText":"\tSection 1.   Vaccines shall be\r\n available.\r\n",
      "CommitteeRecommendations":[{"Action":"Accompanied","Committee":{"CommitteeCode":"J24"}}]}"#;
    const HISTORY: &str = r#"[
      {"Date":"2025-04-30T14:29:25.14","Branch":"Joint","Action":"Hearing scheduled for 05/06/2025 from 11:00 AM-01:00 PM in A-2    ","IsStricken":false},
      {"Date":"2025-02-27T10:38:52.3833333","Branch":"House","Action":"Referred to the committee on Public Health","IsStricken":false},
      {"Date":"0021-01-01T00:00:00","Branch":"House","Action":"bad date","IsStricken":false}
    ]"#;

    #[test]
    fn bill_numbers_are_normalized() {
        assert_eq!(normalize_bill_number("h.3444").as_deref(), Some("H3444"));
        assert_eq!(normalize_bill_number(" S 0012 ").as_deref(), Some("S12"));
        assert_eq!(normalize_bill_number("HD2050"), None);
        assert_eq!(normalize_bill_number("H"), None);
        assert_eq!(normalize_bill_number("H000"), None);
        assert_eq!(normalize_bill_number("H123456"), None);
        assert_eq!(normalize_bill_number("X12"), None);
    }

    #[test]
    fn dates_are_checked() {
        assert_eq!(
            iso_date("2025-02-27T10:38:52.38").as_deref(),
            Some("2025-02-27")
        );
        assert_eq!(iso_date("0021-01-01T00:00:00"), None);
        assert_eq!(iso_date("2025-13-01"), None);
        assert_eq!(iso_date("garbage"), None);
    }

    #[test]
    fn parses_list_detail_and_history() {
        let l = parse_listing(LIST.as_bytes()).unwrap();
        assert_eq!(l.len(), 1, "a docket without a bill number is left out");
        assert_eq!(l[0].bill_number, "H3444");
        assert_eq!(l[0].cosponsor_count, 2);
        assert_eq!(l[0].primary_sponsor.as_deref(), Some("A. Sponsor"));
        let d = parse_detail(DETAIL.as_bytes()).unwrap();
        assert_eq!(d.bill_number, "S12");
        assert_eq!(d.text, "Section 1. Vaccines shall be available.");
        assert_eq!(d.committee_codes, vec!["J24"]);
        let h = parse_history(HISTORY.as_bytes()).unwrap();
        assert_eq!(h.len(), 2, "the entry with an impossible date is dropped");
        assert_eq!(h[0].date, "2025-02-27");
        assert!(h[1].action.ends_with("in A-2"));
        assert!(parse_listing(b"<html>blocked</html>").is_err());
        assert!(parse_detail(b"{}").is_err());
        assert!(expect_listing(b"<html>").is_err());
        assert!(expect_listing(br#"[{"x":1}]"#).is_err());
        assert!(expect_listing(LIST.as_bytes()).is_ok());
    }

    async fn serve(routes: Vec<(&'static str, &'static str, &'static str)>) -> String {
        // (path suffix, status, body); serves connections until the test ends.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let routes = routes.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let path = req
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("")
                        .to_string();
                    let (status, body) = routes
                        .iter()
                        .filter(|(s, _, _)| path.ends_with(s))
                        .max_by_key(|(s, _, _)| s.len())
                        .map(|(_, st, b)| (*st, *b))
                        .unwrap_or(("404 Not Found", "none"));
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}/api")
    }

    fn client() -> (tempfile::TempDir, HttpClient) {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::for_root(dir.path());
        cfg.offline = false;
        (dir, HttpClient::new(&cfg).unwrap())
    }

    #[tokio::test]
    async fn fetches_the_list_live_then_from_the_cache_and_a_bill() {
        let (_d, http) = client();
        let cache = Cache::in_memory().unwrap();
        let base = serve(vec![
            ("/Documents", "200 OK", LIST),
            ("/Documents/S12", "200 OK", DETAIL),
            ("/Documents/S12/DocumentHistoryActions", "200 OK", HISTORY),
            ("/Documents/S99", "404 Not Found", "no"),
        ])
        .await;
        let a = fetch_listing(&http, Some(&cache), &base, 194).await.unwrap();
        assert_eq!(a.mode, SourceMode::Live);
        assert_eq!(a.bills.len(), 1);
        let b = fetch_listing(&http, Some(&cache), &base, 194).await.unwrap();
        assert_eq!(b.mode, SourceMode::Cache);
        let (d, h) = fetch_bill(&http, &base, 194, "s12").await.unwrap();
        assert_eq!(d.bill_number, "S12");
        assert_eq!(h.len(), 2);
        let e = fetch_bill(&http, &base, 194, "S99").await.unwrap_err();
        assert!(matches!(e, PhnError::NotFound(_)), "{e}");
        assert!(matches!(
            fetch_bill(&http, &base, 194, "nonsense").await.unwrap_err(),
            PhnError::InvalidInput(_)
        ));
    }

    #[tokio::test]
    async fn a_block_page_is_not_cached_and_offline_fails_without_a_request() {
        let (_d, http) = client();
        let cache = Cache::in_memory().unwrap();
        let base = serve(vec![("/Documents", "200 OK", "<html>blocked</html>")]).await;
        assert!(
            fetch_listing(&http, Some(&cache), &base, 194)
                .await
                .is_err()
        );
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::for_root(dir.path());
        cfg.offline = true;
        let off = HttpClient::new(&cfg).unwrap();
        assert!(fetch_listing(&off, None, BASE, 194).await.is_err());
    }

    /// Live check against the real API (run with `cargo test -- --ignored`).
    #[tokio::test]
    #[ignore = "needs network access to malegislature.gov"]
    async fn live_one_bill() {
        let (_d, http) = client();
        let (d, h) = fetch_bill(&http, BASE, CURRENT_GENERAL_COURT, "H3444")
            .await
            .unwrap();
        assert_eq!(d.bill_number, "H3444");
        assert!(!h.is_empty());
    }
}
