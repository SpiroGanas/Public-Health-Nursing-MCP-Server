//! POL-01 and POL-03 against local mock servers: live data, the one-day cache, fallback to the
//! snapshot, a live-only tool that must say when live data is unavailable, the Regulations.gov key
//! staying out of every output, watchlists and fill files, and the comment outline.

use std::sync::{Arc, Mutex};

use phn_core::Config;
use phn_tools::Context;
use phn_tools::policy::{
    FederalArgs, TrackArgs, monitor_federal_rules_at, track_ma_legislation_at,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type Hits = Arc<Mutex<Vec<String>>>;

/// A mock server: routes are (path suffix without the query, status, body); the longest matching
/// suffix wins. Returns the base URL and the request lines seen.
async fn serve(routes: Vec<(&'static str, &'static str, String)>) -> (String, Hits) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits: Hits = Arc::new(Mutex::new(Vec::new()));
    let seen = hits.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let routes = routes.clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16384];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let line = req.lines().next().unwrap_or("").to_string();
                seen.lock().unwrap().push(line.clone());
                let path = line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .split('?')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let (status, body) = routes
                    .iter()
                    .filter(|(s, _, _)| path.ends_with(s))
                    .max_by_key(|(s, _, _)| s.len())
                    .map(|(_, st, b)| (*st, b.clone()))
                    .unwrap_or(("404 Not Found", "none".into()));
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), hits)
}

fn ctx(offline: bool) -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let mut cfg = Config::for_root(dir.path());
    cfg.offline = offline;
    let c = Context::new(cfg).unwrap();
    (dir, c)
}

/// An address nothing listens on.
async fn dead_base() -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    drop(l);
    format!("http://{a}")
}

fn bill_routes() -> Vec<(&'static str, &'static str, String)> {
    let list = r#"[
      {"BillNumber":"H100","DocketNumber":"HD1","Title":"An Act relative to vaccine access","PrimarySponsor":{"Name":"Zed Seeded"},"Cosponsors":[{"Id":"a"},{"Id":"b"}]},
      {"BillNumber":"S200","DocketNumber":"SD2","Title":"An Act protecting school nurses and student health","PrimarySponsor":{"Name":"Pat Other"},"Cosponsors":[]},
      {"BillNumber":"H300","DocketNumber":"HD3","Title":"An Act relative to highway signs","PrimarySponsor":{"Name":"Q"},"Cosponsors":[]},
      {"BillNumber":"H400","DocketNumber":"HD4","Title":"An Act to expand vaccination clinics","PrimarySponsor":{"Name":"R"},"Cosponsors":[]},
      {"BillNumber":null,"DocketNumber":"HD5","Title":"An Act on vaccine dockets","PrimarySponsor":null,"Cosponsors":[]}
    ]"#;
    let detail = |n: &str, title: &str, text: &str| {
        format!(
            r#"{{"Title":"{title}","BillNumber":"{n}","DocketNumber":"D","LegislationTypeName":"Bill","Pinslip":"x",
            "DocumentText":"{text}","CommitteeRecommendations":[{{"Action":"Accompanied","Committee":{{"CommitteeCode":"J24"}}}}]}}"#
        )
    };
    let long_text = (1..=40).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ")
        + " the department shall offer each vaccine at no cost to residents and report yearly to the board of health about uptake";
    vec![
        ("/GeneralCourts/194/Documents", "200 OK", list.to_string()),
        ("/Documents/H100", "200 OK", detail("H100", "An Act relative to vaccine access", &long_text)),
        (
            "/Documents/H100/DocumentHistoryActions",
            "200 OK",
            r#"[{"Date":"2025-02-27T10:00:00","Branch":"House","Action":"Referred to the committee on Public Health","IsStricken":false},
                {"Date":"2026-10-01T10:00:00","Branch":"Joint","Action":"Hearing scheduled for 12/01/2026 from 11:00 AM-01:00 PM in A-2","IsStricken":false},
                {"Date":"2026-10-02T10:00:00","Branch":"Senate","Action":"Amendment #6 (Pat Member) rejected","IsStricken":false}]"#
                .to_string(),
        ),
        ("/Documents/S200", "200 OK", detail("S200", "An Act protecting school nurses and student health", "Section 1. School nurses shall be supported.")),
        (
            "/Documents/S200/DocumentHistoryActions",
            "200 OK",
            r#"[{"Date":"2025-03-01T10:00:00","Branch":"Senate","Action":"Referred to the committee on Education","IsStricken":false},
                {"Date":"2026-08-05T10:00:00","Branch":"Executive","Action":"Signed by the Governor, Chapter 120 of the Acts of 2026","IsStricken":false}]"#
                .to_string(),
        ),
        // H400's record fails; the tool must not stop.
        ("/Documents/H400", "500 Internal Server Error", "boom".into()),
        ("/Documents/H400/DocumentHistoryActions", "500 Internal Server Error", "boom".into()),
        ("/Documents/H999", "404 Not Found", "none".into()),
    ]
}

fn track(args: TrackArgs) -> TrackArgs {
    TrackArgs {
        as_of: Some("2026-11-13".into()),
        ..args
    }
}

#[tokio::test]
async fn legislation_live_then_cache_with_stage_hearing_and_excerpt() {
    let (_d, c) = ctx(false);
    let (base, hits) = serve(bill_routes()).await;
    let args = track(TrackArgs {
        topics: vec!["vaccines".into(), "school_child_care".into()],
        ..Default::default()
    });
    let e = track_ma_legislation_at(&c, args.clone(), &base).await.unwrap();
    let d = &e.data;
    assert_eq!(d.data_mode, "live");
    assert_eq!(d.title_matches, 3, "H100, S200, H400 match; highway signs and the docket do not");
    let h100 = d.bills.iter().find(|b| b.bill_number == "H100").unwrap();
    assert_eq!(h100.stage, "in_committee");
    assert_eq!(h100.committee.as_deref(), Some("Public Health"));
    assert!(h100.hearing_upcoming);
    assert_eq!(h100.latest_hearing_date.as_deref(), Some("2026-12-01"));
    assert_eq!(h100.days_until_hearing, Some(18));
    assert_eq!(h100.cosponsors, 2);
    let ex = h100.excerpt.as_ref().unwrap();
    assert!(ex.text.split_whitespace().filter(|w| *w != "…").count() <= 25, "{}", ex.text);
    assert!(ex.text.contains("vaccine") && ex.source.contains("/Bills/194/H100"));
    assert_eq!(h100.url, "https://malegislature.gov/Bills/194/H100");
    // The amendment line's member name is left out.
    let last = h100.last_action.as_ref().unwrap();
    assert_eq!(last.text, "Amendment #6 (member omitted) rejected");
    let s200 = d.bills.iter().find(|b| b.bill_number == "S200").unwrap();
    assert_eq!(s200.stage, "enacted");
    // The bill whose record failed is still listed, from the snapshot or the list alone.
    assert!(d.bills.iter().any(|b| b.bill_number == "H400"));
    // Upcoming hearings come first.
    assert_eq!(d.bills[0].bill_number, "H100");
    // No sponsor names anywhere in the output.
    let all = serde_json::to_string(&e).unwrap() + &e.summary;
    assert!(!all.contains("Zed Seeded") && !all.contains("Pat Other") && !all.contains("Pat Member"));
    assert!(e.notice.is_some() && !e.privacy.row_level_data);
    assert!(e.caveats.iter().any(|c| c.contains("TITLES")));
    // A second call reads the bill list from the cache.
    let e2 = track_ma_legislation_at(&c, args, &base).await.unwrap();
    assert_eq!(e2.data.data_mode, "cache");
    let list_requests = hits
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains("GET /GeneralCourts/194/Documents "))
        .count();
    assert_eq!(list_requests, 1);
}

#[tokio::test]
async fn legislation_filters_not_found_and_validation() {
    let (_d, c) = ctx(false);
    let (base, _) = serve(bill_routes()).await;
    let e = track_ma_legislation_at(
        &c,
        track(TrackArgs {
            topics: vec!["vaccines".into(), "School health and child care".into()],
            bill_numbers: vec!["h 999".into()],
            stages: vec!["enacted".into()],
            ..Default::default()
        }),
        &base,
    )
    .await
    .unwrap();
    assert_eq!(e.data.not_found, vec!["H999"]);
    assert!(e.data.bills.iter().all(|b| b.stage == "enacted") && e.data.bills.len() == 1);
    let only = track_ma_legislation_at(
        &c,
        track(TrackArgs {
            keywords: vec!["vaccine access".into(), "school nurses".into()],
            only_upcoming_hearings: Some(true),
            ..Default::default()
        }),
        &base,
    )
    .await
    .unwrap();
    assert_eq!(only.data.bills.len(), 1, "only H100 has an upcoming hearing");
    for (args, needle) in [
        (TrackArgs { topics: vec!["nonsense".into()], ..Default::default() }, "not a topic"),
        (TrackArgs { bill_numbers: vec!["HD2050".into()], ..Default::default() }, "not a bill number"),
        (TrackArgs { stages: vec!["done".into()], ..Default::default() }, "not a stage"),
        (TrackArgs { keywords: vec!["call 413-555-0100".into()], ..Default::default() }, "phone"),
        (TrackArgs { general_court: Some(12), ..Default::default() }, "general_court"),
        (TrackArgs { as_of: Some("0021-01-01".into()), ..Default::default() }, "1900"),
    ] {
        let err = track_ma_legislation_at(&c, args, &base).await.unwrap_err().to_string();
        assert!(err.to_lowercase().contains(needle), "{err}");
        assert!(!err.contains("413-555-0100"));
    }
}

#[tokio::test]
async fn legislation_falls_back_to_the_dated_snapshot_and_says_so() {
    let (_d, c) = ctx(false);
    let dead = dead_base().await;
    let e = track_ma_legislation_at(
        &c,
        track(TrackArgs {
            topics: vec!["vaccines".into()],
            ..Default::default()
        }),
        &dead,
    )
    .await
    .unwrap();
    assert_eq!(e.data.data_mode, "snapshot");
    assert!(e.data.mode_note.contains("snapshot") && e.data.mode_note.contains(&e.data.as_of));
    assert!(!e.data.bills.is_empty());
    assert!(e.data.bills.iter().all(|b| b.data_mode == "snapshot"));
    assert!(e.caveats.iter().any(|c| c.contains("SNAPSHOT DATA")));
    assert!(e.sources.iter().any(|s| s.mode == phn_core::SourceMode::Snapshot));
    // Earlier courts have no snapshot.
    let err = track_ma_legislation_at(
        &c,
        track(TrackArgs {
            topics: vec!["vaccines".into()],
            general_court: Some(193),
            ..Default::default()
        }),
        &dead,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("194"), "{err}");
    // The topic index needs no network.
    let idx = track_ma_legislation_at(&c, track(TrackArgs::default()), &dead).await.unwrap();
    assert!(idx.data.bills.is_empty() && idx.data.topics.len() >= 15);
    assert!(idx.data.topics.iter().any(|t| t.id == "vaccines" && t.bills_matched > 0));
}

#[tokio::test]
async fn watchlist_is_read_saved_and_people_lists_are_refused() {
    let (d, c) = ctx(false);
    let dead = dead_base().await;
    std::fs::write(
        d.path().join("data/watch.csv"),
        "kind,value,note\ntopic,vaccines,board agenda\nkeyword,PFAS,\nbill,H 100,\ntopic,astrology,\nbill,ZZ9,\n",
    )
    .unwrap();
    let e = track_ma_legislation_at(
        &c,
        track(TrackArgs {
            watchlist_path: Some("watch.csv".into()),
            save_watchlist: Some(true),
            ..Default::default()
        }),
        &dead,
    )
    .await
    .unwrap();
    let w = e.data.watchlist.as_ref().unwrap();
    assert_eq!(w.topics, vec!["vaccines"]);
    assert_eq!(w.keywords, vec!["PFAS"]);
    assert_eq!(w.bills, vec!["H100"]);
    assert_eq!(w.unrecognized.len(), 2);
    let saved = w.saved_file.as_ref().unwrap();
    assert!(e.privacy.files_written.contains(saved));
    let text = std::fs::read_to_string(d.path().join("data").join(saved)).unwrap();
    assert!(text.starts_with("kind,value,note,title,stage"));
    assert!(text.contains("topic,vaccines") && text.contains("keyword,PFAS") && text.contains("bill,H100"));
    // A client list is refused without echoing a value.
    std::fs::write(
        d.path().join("data/clients.csv"),
        "Client Name,DOB,kind,value\nAnselma Quiddington,1950-02-03,topic,vaccines\n",
    )
    .unwrap();
    let err = track_ma_legislation_at(
        &c,
        track(TrackArgs {
            watchlist_path: Some("clients.csv".into()),
            ..Default::default()
        }),
        &dead,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("list of people"), "{err}");
    assert!(!err.contains("Quiddington") && !err.contains("1950"));
}

// ---------------------------------------------------------------- federal

fn fr_search_body() -> String {
    r#"{"count":3,"total_pages":1,"results":[
      {"title":"Notice of a later deadline","type":"Notice","document_number":"2026-30002","publication_date":"2026-11-01",
       "comments_close_on":"2026-12-20","agencies":[{"name":"Centers for Disease Control and Prevention","slug":"centers-for-disease-control-and-prevention"}],
       "html_url":"https://www.federalregister.gov/d/2026-30002","docket_ids":["CDC-2026-0002"]},
      {"title":"Lead in drinking water: proposed changes","type":"Proposed Rule","abstract":"EPA proposes changes to lead and copper requirements.",
       "document_number":"2026-30001","publication_date":"2026-10-15","comments_close_on":"2026-11-16",
       "agencies":[{"name":"Environmental Protection Agency","slug":"environmental-protection-agency"}],
       "html_url":"https://www.federalregister.gov/d/2026-30001","docket_ids":["EPA-HQ-OW-2026-0001"]},
      {"title":"A final rule with no comment period","type":"Rule","document_number":"2026-30003","publication_date":"2026-11-10",
       "comments_close_on":null,"agencies":[{"name":"Health Resources and Services Administration","slug":"health-resources-and-services-administration"}],"docket_ids":[]}
    ]}"#
        .to_string()
}

fn fr_routes() -> Vec<(&'static str, &'static str, String)> {
    vec![
        ("/documents.json", "200 OK", fr_search_body()),
        (
            "/documents/2026-30001.json",
            "200 OK",
            r#"{"title":"Lead in drinking water: proposed changes","type":"Proposed Rule","document_number":"2026-30001",
                "publication_date":"2026-10-15","comments_close_on":"2026-11-16",
                "agencies":[{"name":"Environmental Protection Agency","slug":"environmental-protection-agency"}],
                "docket_ids":["EPA-HQ-OW-2026-0001"]}"#
                .to_string(),
        ),
        (
            "/documents",
            "200 OK",
            r#"{"data":[{"id":"EPA-HQ-OW-2026-0001-0001","type":"documents","attributes":{"docketId":"EPA-HQ-OW-2026-0001",
                "openForComment":true,"withdrawn":false,"commentStartDate":"2026-10-15T04:00:00Z","commentEndDate":"2026-11-17T04:59:59Z","title":"Lead"}}],"meta":{}}"#
                .to_string(),
        ),
    ]
}

fn fed(args: FederalArgs) -> FederalArgs {
    FederalArgs {
        as_of: Some("2026-11-13".into()),
        ..args
    }
}

#[tokio::test]
async fn federal_live_countdown_regulations_gov_and_no_key_in_output() {
    let (_d, c) = ctx(false);
    let (fr, fr_hits) = serve(fr_routes()).await;
    let (regs, regs_hits) = serve(fr_routes()).await;
    let e = monitor_federal_rules_at(
        &c,
        fed(FederalArgs::default()),
        &fr,
        &regs,
        Some("SECRETKEY-do-not-leak".into()),
    )
    .await
    .unwrap();
    let d = &e.data;
    assert_eq!(d.data_status, "live");
    assert_eq!(d.documents.len(), 3);
    // Soonest open deadline first; the one without a deadline last.
    let nums: Vec<&str> = d.documents.iter().map(|x| x.document_number.as_str()).collect();
    assert_eq!(nums, vec!["2026-30001", "2026-30002", "2026-30003"]);
    assert_eq!(d.documents[0].countdown.days_remaining, Some(3));
    assert_eq!(d.documents[0].countdown.status, "closing_soon");
    assert_eq!(d.documents[1].countdown.days_remaining, Some(37));
    assert_eq!(d.documents[2].countdown.status, "no_comment_period");
    let r = d.documents[0].regulations_gov.as_ref().unwrap();
    assert_eq!(r.document_id, "EPA-HQ-OW-2026-0001-0001");
    assert_eq!(r.comment_end_utc.as_deref(), Some("2026-11-17T04:59:59Z"));
    assert!(d.regulations_gov.used);
    // The key reached only Regulations.gov, and no output mentions it.
    assert!(regs_hits.lock().unwrap().iter().any(|l| l.contains("api_key=SECRETKEY-do-not-leak")));
    assert!(!fr_hits.lock().unwrap().iter().any(|l| l.contains("SECRETKEY")));
    let all = serde_json::to_string(&e).unwrap() + &e.summary;
    assert!(!all.contains("SECRETKEY"));
    // The request asked for six default agencies and comment periods open today or later.
    let line = fr_hits.lock().unwrap()[0].clone();
    assert!(line.contains("conditions%5Bagencies%5D%5B%5D=environmental-protection-agency"));
    assert!(line.contains("conditions%5Bcomment_date%5D%5Bgte%5D=2026-11-13"));
    assert!(e.notice.is_some());
    assert!(e.caveats.iter().any(|c| c.contains("slugs")));
}

#[tokio::test]
async fn federal_without_a_key_skips_regulations_gov_and_says_so() {
    let (_d, c) = ctx(false);
    let (fr, _) = serve(fr_routes()).await;
    // The environment variable is not set in the test environment.
    if phn_data::fedreg::regulations_key().is_some() {
        return;
    }
    let e = monitor_federal_rules_at(&c, fed(FederalArgs::default()), &fr, &fr, None)
        .await
        .unwrap();
    assert!(!e.data.regulations_gov.used);
    assert!(e.data.regulations_gov.note.contains("PHN_REGULATIONS_GOV_API_KEY"));
}

#[tokio::test]
async fn federal_says_live_data_is_unavailable_instead_of_listing_old_data() {
    let (_d, c) = ctx(false);
    let dead = dead_base().await;
    let e = monitor_federal_rules_at(&c, fed(FederalArgs::default()), &dead, &dead, None)
        .await
        .unwrap();
    assert_eq!(e.data.data_status, "unavailable");
    assert!(e.data.documents.is_empty() && e.data.as_of.is_none());
    assert!(e.data.status_note.contains("LIVE DATA UNAVAILABLE"));
    assert!(e.summary.contains("LIVE DATA UNAVAILABLE"));
    assert!(e.caveats.iter().any(|c| c.contains("no open comment periods")));
    assert!(!e.data.manual_links.is_empty());
    assert!(e.sources.iter().all(|s| s.mode != phn_core::SourceMode::Live));
    // Offline mode behaves the same.
    let (_d2, off) = ctx(true);
    let e = monitor_federal_rules_at(&off, fed(FederalArgs::default()), &dead, &dead, None)
        .await
        .unwrap();
    assert_eq!(e.data.data_status, "unavailable");
}

#[tokio::test]
async fn federal_outline_from_a_live_document_and_from_a_description() {
    let (d, c) = ctx(false);
    let (fr, _) = serve(fr_routes()).await;
    std::fs::write(
        d.path().join("data/fill.csv"),
        "field,value\norganization,Example Town Health Department\nposition,support with changes\nastrology,x\n",
    )
    .unwrap();
    let e = monitor_federal_rules_at(
        &c,
        fed(FederalArgs {
            document_number: Some("2026-30001".into()),
            draft_comment_outline: Some(true),
            fill_path: Some("fill.csv".into()),
            check_regulations_gov: Some(false),
            ..Default::default()
        }),
        &fr,
        &fr,
        None,
    )
    .await
    .unwrap();
    let o = e.data.outline.as_ref().unwrap();
    assert_eq!(o.docket_id.as_deref(), Some("EPA-HQ-OW-2026-0001"));
    assert_eq!(o.comment_deadline.as_deref(), Some("2026-11-16"));
    assert_eq!(o.fields_filled, 2);
    assert_eq!(o.unrecognized_fields, vec!["astrology"]);
    assert_eq!(o.sections.len(), 9);
    assert!(o.tips.iter().all(|t| t.quote.split_whitespace().count() <= 25));
    assert!(o.file.ends_with(".docx") && d.path().join("data").join(&o.file).exists());
    assert!(e.privacy.files_written.contains(&o.file));
    // The fill values are in the file, not in anything returned.
    let all = serde_json::to_string(&e).unwrap() + &e.summary;
    assert!(!all.contains("Example Town Health Department") && !all.contains("support with changes"));
    // With live data unavailable, the outline can still be drafted from a description.
    let dead = dead_base().await;
    let e = monitor_federal_rules_at(
        &c,
        fed(FederalArgs {
            draft_comment_outline: Some(true),
            document_title: Some("A proposed rule on school meals".into()),
            comment_deadline: Some("2026-12-31".into()),
            docket_id: Some("FNS-2026-0009".into()),
            ..Default::default()
        }),
        &dead,
        &dead,
        None,
    )
    .await
    .unwrap();
    assert_eq!(e.data.data_status, "unavailable");
    let o = e.data.outline.as_ref().unwrap();
    assert_eq!(o.countdown.days_remaining, Some(48));
    // Without a title or number there is nothing to outline.
    let err = monitor_federal_rules_at(
        &c,
        fed(FederalArgs {
            draft_comment_outline: Some(true),
            ..Default::default()
        }),
        &dead,
        &dead,
        None,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("document_number"), "{err}");
}

#[tokio::test]
async fn federal_refuses_people_lists_and_bad_arguments() {
    let (d, c) = ctx(false);
    let (fr, _) = serve(fr_routes()).await;
    std::fs::write(
        d.path().join("data/clients.csv"),
        "First Name,Last Name,field,value\nAnselma,Quiddington,organization,x\n",
    )
    .unwrap();
    let err = monitor_federal_rules_at(
        &c,
        fed(FederalArgs {
            document_number: Some("2026-30001".into()),
            draft_comment_outline: Some(true),
            fill_path: Some("clients.csv".into()),
            ..Default::default()
        }),
        &fr,
        &fr,
        None,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("list of people") && !err.contains("Quiddington"), "{err}");
    for (args, needle) in [
        (FederalArgs { agencies: vec!["navy".into()], ..Default::default() }, "not an agency id"),
        (FederalArgs { document_types: vec!["memo".into()], ..Default::default() }, "not a document type"),
        (FederalArgs { agency_slugs: vec!["Bad Slug!".into()], ..Default::default() }, "slug"),
        (FederalArgs { published_since: Some("2026-12-01".into()), published_until: Some("2026-11-01".into()), ..Default::default() }, "after"),
        (FederalArgs { topic: Some("call 413-555-0100".into()), ..Default::default() }, "phone"),
        (FederalArgs { comment_deadline: Some("2026-02-30".into()), ..Default::default() }, "not a date"),
        (FederalArgs { document_number: Some("../etc".into()), ..Default::default() }, "document number"),
    ] {
        let err = monitor_federal_rules_at(&c, fed(args), &fr, &fr, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.to_lowercase().contains(needle), "{err}");
    }
}

/// Live checks (run with `cargo test -p phn-tools --test phase19_tracking_mock -- --ignored`).
#[tokio::test]
#[ignore = "needs network access to malegislature.gov and www.federalregister.gov"]
async fn live_both_tools() {
    let (_d, c) = ctx(false);
    let e = track_ma_legislation_at(
        &c,
        TrackArgs {
            topics: vec!["vaccines".into()],
            max_bills: Some(3),
            ..Default::default()
        },
        phn_data::malegislature::BASE,
    )
    .await
    .unwrap();
    assert_eq!(e.data.data_mode, "live");
    let e = monitor_federal_rules_at(
        &c,
        FederalArgs {
            agencies: vec!["epa".into()],
            open_for_comment_only: Some(false),
            max_results: Some(3),
            ..Default::default()
        },
        phn_data::fedreg::BASE,
        phn_data::fedreg::REGULATIONS_BASE,
        None,
    )
    .await
    .unwrap();
    assert_eq!(e.data.data_status, "live");
}
