//! `cargo xtask build-surveillance-snapshots`: rebuild the Phase 6
//! surveillance snapshots and update their entries in
//! `snapshots/manifest.json` (entries built elsewhere are kept).
//!
//! * `cdc-nndss-weekly`: NNDSS Weekly Data (data.cdc.gov `x9gk-5huc`) for
//!   Massachusetts, New England, and U.S. residents: the weeks that
//!   `get_notifiable_disease_trends` needs for the latest published week
//!   (that week and the 7 before it, plus year-end weeks 52 and 53) in the
//!   current and 4 previous MMWR years, in a compact JSON layout.
//! * `cdc-respiratory-ma`: the last 13 weeks of NHSN hospital admissions,
//!   NSSP emergency department visits, NREVSS test positivity, and the ARI
//!   activity level, as returned by data.cdc.gov (trimmed columns).
//! * `cdc-nwss-ma`: the last 13 weeks of CDC wastewater viral activity levels
//!   for Massachusetts sampling sites.
//! * `mwra-deer-island-biobot`: the final 12 months of MWRA's Deer Island
//!   Biobot SARS-CoV-2 series (the program ended with samples collected
//!   through 2026-07-27), read from MWRA's data PDF with `pdftotext -tsv`
//!   (poppler-utils must be installed).
//!
//! The SoQL queries mirror those in `crates/phn-tools/src/cd/nndss.rs` and
//! `crates/phn-tools/src/cd/respiratory.rs`; keep them in step.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Duration, NaiveDate};
use serde_json::{Map, Value, json};

use crate::download::{fetch, sha256};
use crate::snapshots::{Manifest, SnapshotEntry, SnapshotFile};

const CDC_LICENSE: &str = "U.S. federal government work (CDC open data, data.cdc.gov); public domain in the United States.";

/// MWRA's final Biobot data tables (samples collected through 2026-07-27).
const MWRA_PDF_URL: &str = "https://www.mwra.com/media/file/mwradata20260730-datapdf";
const MWRA_PAGE_URL: &str = "https://www.mwra.com/biobot/biobotdata.htm";
/// SHA-256 of the PDF retrieved on 2026-11-08. MWRA said the series is final,
/// so a different checksum means the file changed and must be re-checked.
const MWRA_PDF_SHA256: &str = "53128672f21006b60a6a7e6c279be2cc8dc486261d1757f22037091004996154";

struct Built {
    entry: SnapshotEntry,
    contents: Vec<(String, String)>,
}

pub fn run(root: &Path) -> Result<()> {
    let snap_dir = root.join("snapshots");
    let manifest_path = snap_dir.join("manifest.json");
    let mut manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).context("reading snapshots/manifest.json")?,
    )
    .context("parsing snapshots/manifest.json")?;
    let today = chrono::Local::now().date_naive();

    let built = vec![
        build_nndss(today)?,
        build_respiratory(today)?,
        build_wastewater(today)?,
        build_mwra(today)?,
    ];
    for b in built {
        for (path, text) in &b.contents {
            std::fs::write(snap_dir.join(path), text)?;
            println!("wrote snapshots/{path} ({} bytes)", text.len());
        }
        match manifest.snapshots.iter_mut().find(|e| e.id == b.entry.id) {
            Some(e) => *e = b.entry,
            None => manifest.snapshots.push(b.entry),
        }
    }
    let json = serde_json::to_string_pretty(&manifest)? + "\n";
    std::fs::write(&manifest_path, json)?;
    println!("Updated snapshots/manifest.json.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Socrata helpers
// ---------------------------------------------------------------------------

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn soql(dataset: &str, params: &[(&str, String)]) -> String {
    let qs = params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("https://data.cdc.gov/resource/{dataset}.json?{qs}")
}

/// Rows of a Socrata response and the SHA-256 of the body.
fn get_rows(url: &str) -> Result<(Vec<Map<String, Value>>, String)> {
    let body = fetch(url, None)?;
    let hash = sha256(&body);
    match serde_json::from_slice::<Value>(&body).with_context(|| format!("parsing {url}"))? {
        Value::Array(a) => Ok((
            a.into_iter()
                .filter_map(|x| match x {
                    Value::Object(m) => Some(m),
                    _ => None,
                })
                .collect(),
            hash,
        )),
        other => bail!(
            "unexpected response from {url}: {}",
            other.to_string().chars().take(300).collect::<String>()
        ),
    }
}

fn text<'a>(row: &'a Map<String, Value>, field: &str) -> &'a str {
    row.get(field).and_then(Value::as_str).unwrap_or("").trim()
}

/// One JSON object per line, inside an array (readable diffs).
fn json_lines(rows: &[Map<String, Value>]) -> String {
    let mut s = String::from("[\n");
    for (i, r) in rows.iter().enumerate() {
        s.push_str(&Value::Object(r.clone()).to_string());
        s.push_str(if i + 1 < rows.len() { ",\n" } else { "\n" });
    }
    s.push_str("]\n");
    s
}

fn file_entry(path: &str, text: &str, rows: usize) -> SnapshotFile {
    SnapshotFile {
        path: path.into(),
        sha256: sha256(text.as_bytes()),
        bytes: text.len() as u64,
        rows: rows as u64,
    }
}

// ---------------------------------------------------------------------------
// NNDSS weekly (CD-08)
// ---------------------------------------------------------------------------

const NNDSS: &str = "x9gk-5huc";
const NNDSS_AREAS: [&str; 3] = ["Massachusetts", "New England", "U.S. Residents"];

fn nndss_area(states: &str) -> Option<usize> {
    match states.to_ascii_uppercase().as_str() {
        "MASSACHUSETTS" => Some(0),
        "NEW ENGLAND" => Some(1),
        "U.S. RESIDENTS" | "US RESIDENTS" => Some(2),
        _ => None,
    }
}

/// A count as a compact string: the number (no trailing ".0"), or the CDC
/// flag ("" for "-", no reported cases), or "?" when the row has neither.
fn nndss_value(row: &Map<String, Value>, field: &str) -> String {
    let v = text(row, field);
    if !v.is_empty() {
        return match v.parse::<f64>() {
            Ok(x) if x.fract() == 0.0 => format!("{}", x as i64),
            Ok(x) => format!("{x}"),
            Err(_) => v.to_string(),
        };
    }
    match text(row, &format!("{field}_flag")) {
        "-" => String::new(),
        "" => "?".into(),
        f => f.to_string(),
    }
}

fn build_nndss(today: NaiveDate) -> Result<Built> {
    let (latest, _) = get_rows(&soql(
        NNDSS,
        &[
            ("$select", "year, week".into()),
            ("$where", "upper(states) = 'MASSACHUSETTS'".into()),
            ("$order", "year DESC, week DESC".into()),
            ("$limit", "1".into()),
        ],
    ))?;
    let first = latest
        .first()
        .ok_or_else(|| anyhow!("NNDSS returned no rows"))?;
    let year: i32 = text(first, "year").parse().context("NNDSS year")?;
    let week: u32 = text(first, "week")
        .parse::<f64>()
        .map(|w| w as u32)
        .context("NNDSS week")?;
    let mut weeks: BTreeSet<u32> = (week.saturating_sub(7).max(1)..=week).collect();
    weeks.insert(52);
    weeks.insert(53);
    let years: Vec<String> = (year - 4..=year).map(|y| format!("'{y}'")).collect();
    let where_ = format!(
        "upper(states) in ('MASSACHUSETTS','NEW ENGLAND','U.S. RESIDENTS','US RESIDENTS') \
         AND year in ({}) AND week in ({})",
        years.join(","),
        weeks
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut all = Vec::new();
    let mut hashes = BTreeMap::new();
    let page = 50_000usize;
    for offset in (0..).step_by(page) {
        let (rows, hash) = get_rows(&soql(
            NNDSS,
            &[
                (
                    "$select",
                    "states, year, week, label, m1, m1_flag, m3, m3_flag, m4, m4_flag".into(),
                ),
                ("$where", where_.clone()),
                ("$order", ":id".into()),
                ("$limit", page.to_string()),
                ("$offset", offset.to_string()),
            ],
        ))?;
        hashes.insert(format!("{NNDSS}.json (offset {offset})"), hash);
        let n = rows.len();
        all.extend(rows);
        if n < page {
            break;
        }
    }

    // Weeks kept: the latest week and the 7 before it in the current year;
    // the same week, 4 weeks earlier, and weeks 52-53 in earlier years.
    let keep_week = |y: i32, w: u32| {
        if y == year {
            w + 7 >= week && w <= week
        } else {
            w == week || w + 4 == week || w >= 52 || (week == 53 && w == 52)
        }
    };
    // Series (area, label) with at least one reported case in any kept row.
    let mut has_cases: BTreeSet<(usize, &str)> = BTreeSet::new();
    for r in &all {
        if let Some(area) = nndss_area(text(r, "states")) {
            let any = ["m1", "m3", "m4"]
                .iter()
                .any(|f| text(r, f).parse::<f64>().is_ok_and(|x| x > 0.0));
            if any {
                has_cases.insert((area, text(r, "label")));
            }
        }
    }
    let labels: BTreeSet<String> = has_cases.iter().map(|(_, l)| l.to_string()).collect();
    let label_index: BTreeMap<&str, usize> = labels
        .iter()
        .enumerate()
        .map(|(i, l)| (l.as_str(), i))
        .collect();
    let mut out: BTreeSet<(usize, i32, u32, usize, String, String, String)> = BTreeSet::new();
    for r in &all {
        let Some(area) = nndss_area(text(r, "states")) else {
            continue;
        };
        let y: i32 = text(r, "year").parse()?;
        let w: u32 = text(r, "week").parse::<f64>()? as u32;
        if !keep_week(y, w) || !has_cases.contains(&(area, text(r, "label"))) {
            continue;
        }
        let l = label_index[text(r, "label")];
        // m4 (previous year's YTD) is only needed for the earliest year, to
        // give the fifth baseline year.
        let m4 = if y == year - 4 {
            nndss_value(r, "m4")
        } else {
            "?".into()
        };
        out.insert((
            area,
            y,
            w,
            l,
            nndss_value(r, "m1"),
            nndss_value(r, "m3"),
            m4,
        ));
    }
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("\"dataset\": \"{NNDSS}\",\n"));
    s.push_str(&format!(
        "\"latest\": {},\n",
        json!({"year": year, "week": week})
    ));
    s.push_str(&format!("\"areas\": {},\n", json!(NNDSS_AREAS)));
    s.push_str("\"fields\": [\"area\", \"year\", \"week\", \"label\", \"m1\", \"m3\", \"m4\"],\n");
    s.push_str(
        "\"values\": \"m1 current week, m3 cumulative YTD, m4 previous year's cumulative YTD: \
         a number, or the CDC flag ('' = '-', no reported cases; U, N, NN, NP, NC); '?' = \
         not in the row or not kept\",\n",
    );
    s.push_str(&format!("\"labels\": {},\n", json!(labels)));
    s.push_str("\"rows\": [\n");
    let n = out.len();
    for (i, (a, y, w, l, m1, m3, m4)) in out.iter().enumerate() {
        s.push_str(&json!([a, y, w, l, m1, m3, m4]).to_string());
        s.push_str(if i + 1 < n { ",\n" } else { "\n" });
    }
    s.push_str("]\n}\n");
    let path = "nndss_weekly.json";
    Ok(Built {
        entry: SnapshotEntry {
            id: "cdc-nndss-weekly".into(),
            title: "NNDSS Weekly Data: Massachusetts, New England, and U.S. residents".into(),
            publisher: "CDC, Office of Public Health Data, Surveillance, and Technology".into(),
            source_urls: vec![
                format!("https://data.cdc.gov/d/{NNDSS}"),
                "https://www.cdc.gov/nndss/infectious-disease/weekly-and-annual-disease-data-tables.html".into(),
            ],
            source_sha256: hashes,
            source_vintage: format!("MMWR {year} week {week} (provisional)"),
            retrieved: today.to_string(),
            license: CDC_LICENSE.into(),
            derivation: format!(
                "Rows of NNDSS Weekly Data for Massachusetts, New England, and U.S. residents \
                 (reporting-area names in either letter case), MMWR years {}-{year}, weeks \
                 {} (the latest published week for Massachusetts, the 7 weeks before it, and \
                 year-end weeks 52 and 53). Kept: current-week count (m1) and cumulative \
                 year-to-date count (m3) for every row, and the previous year's cumulative \
                 count (m4) for {} rows only. Counts are stored as published; CDC flags are \
                 kept as codes. Built by `cargo xtask build-surveillance-snapshots`.",
                year - 4,
                weeks.iter().map(u32::to_string).collect::<Vec<_>>().join(", "),
                year - 4
            ),
            caveats: vec![
                "Provisional counts as published each week; CDC revises them, and finalized \
                 annual counts (CDC Stacks annual tables) can differ."
                    .into(),
                "Covers only the latest week in the snapshot; other weeks need the live \
                 dataset."
                    .into(),
            ],
            files: vec![file_entry(path, &s, n)],
        },
        contents: vec![(path.into(), s)],
    })
}

// ---------------------------------------------------------------------------
// Respiratory (CD-09)
// ---------------------------------------------------------------------------

fn since(today: NaiveDate) -> String {
    format!("{}T00:00:00", today - Duration::weeks(13))
}

struct Query {
    file: &'static str,
    dataset: &'static str,
    select: &'static str,
    where_: String,
    order: &'static str,
    /// Keep only the first row per these fields (NREVSS repeats a week each
    /// time it is re-posted; the order puts the latest post first).
    dedupe: &'static [&'static str],
}

fn respiratory_queries(since: &str) -> Vec<Query> {
    vec![
        Query {
            file: "resp_nhsn.json",
            dataset: "vdzy-6i9v",
            select: "jurisdiction, weekendingdate, totalconfc19newadm, totalconfflunewadm, \
                     totalconfrsvnewadm, totalconfc19newadmper100k, totalconfflunewadmper100k, \
                     totalconfrsvnewadmper100k, totalconfc19newadmper100klevel, \
                     totalconfflunewadmper100klevel, totalconfrsvnewadmper100klevel, \
                     totalconfc19newadmperchosprep",
            where_: format!("jurisdiction in ('MA','USA') AND weekendingdate >= '{since}'"),
            order: "weekendingdate DESC, jurisdiction",
            dedupe: &[],
        },
        Query {
            file: "resp_nssp.json",
            dataset: "rdmq-nq56",
            select: "week_end, geography, percent_visits_covid, percent_visits_influenza, \
                     percent_visits_rsv, percent_visits_combined, ed_trends_covid, \
                     ed_trends_influenza, ed_trends_rsv, buildnumber",
            where_: format!(
                "geography in ('Massachusetts','United States') AND county = 'All' AND \
                 week_end >= '{since}'"
            ),
            order: "week_end DESC, geography",
            dedupe: &[],
        },
        Query {
            file: "resp_nrevss_covid.json",
            dataset: "gvsb-yw6g",
            select: "level, percent_pos, number_tested, posted, mmwrweek_end",
            where_: format!("level in ('Region 1','National') AND mmwrweek_end >= '{since}'"),
            order: "mmwrweek_end DESC, level, posted DESC",
            dedupe: &["level", "mmwrweek_end"],
        },
        Query {
            file: "resp_nrevss_rsv.json",
            dataset: "3cxc-4k8q",
            select: "level, pcr_percent_positive, pcr_tests, posted, mmwrweek_end",
            where_: format!("level in ('Region 1','National') AND mmwrweek_end >= '{since}'"),
            order: "mmwrweek_end DESC, level, posted DESC",
            dedupe: &["level", "mmwrweek_end"],
        },
        Query {
            file: "resp_nrevss_national.json",
            dataset: "seuz-s2cv",
            select: "week_end, pathogen, percent_test_positivity",
            where_: format!("week_end >= '{since}'"),
            order: "week_end DESC, pathogen",
            dedupe: &[],
        },
        Query {
            file: "resp_ari.json",
            dataset: "f3zz-zga5",
            select: "week_end, geography, label",
            where_: "geography in ('Massachusetts','National','United States')".into(),
            order: "week_end DESC, geography",
            dedupe: &[],
        },
    ]
}

fn wastewater_queries(since: &str) -> Vec<Query> {
    vec![Query {
        file: "resp_wval.json",
        dataset: "atcp-73re",
        select: "state_territory, counties_served, site, population_served, source, site_wval, \
                 site_wval_category, week_end, pathogen_target, date_updated",
        where_: format!("state_territory = 'Massachusetts' AND week_end >= '{since}'"),
        order: "week_end DESC, site, pathogen_target",
        dedupe: &[],
    }]
}

struct QueryResults {
    contents: Vec<(String, String)>,
    files: Vec<SnapshotFile>,
    hashes: BTreeMap<String, String>,
    urls: Vec<String>,
}

fn run_queries(queries: &[Query]) -> Result<QueryResults> {
    let mut contents = Vec::new();
    let mut files = Vec::new();
    let mut hashes = BTreeMap::new();
    let mut urls = Vec::new();
    for q in queries {
        let (rows, hash) = get_rows(&soql(
            q.dataset,
            &[
                (
                    "$select",
                    q.select.split_whitespace().collect::<Vec<_>>().join(" "),
                ),
                (
                    "$where",
                    q.where_.split_whitespace().collect::<Vec<_>>().join(" "),
                ),
                ("$order", q.order.into()),
                ("$limit", "5000".into()),
            ],
        ))?;
        if rows.is_empty() {
            bail!("{} returned no rows", q.dataset);
        }
        let mut seen = BTreeSet::new();
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|r| {
                q.dedupe.is_empty()
                    || seen.insert(
                        q.dedupe
                            .iter()
                            .map(|f| text(r, f).to_string())
                            .collect::<Vec<_>>(),
                    )
            })
            .collect();
        let text = json_lines(&rows);
        files.push(file_entry(q.file, &text, rows.len()));
        hashes.insert(format!("{}.json", q.dataset), hash);
        urls.push(format!("https://data.cdc.gov/d/{}", q.dataset));
        contents.push((q.file.to_string(), text));
    }
    Ok(QueryResults {
        contents,
        files,
        hashes,
        urls,
    })
}

fn latest_date(contents: &[(String, String)]) -> String {
    let mut latest = String::new();
    for (_, t) in contents {
        if let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(t) {
            for r in rows {
                for f in ["weekendingdate", "week_end", "mmwrweek_end"] {
                    if let Some(d) = r.get(f).and_then(Value::as_str) {
                        let d: String = d.chars().take(10).collect();
                        if d > latest {
                            latest = d;
                        }
                    }
                }
            }
        }
    }
    latest
}

fn build_respiratory(today: NaiveDate) -> Result<Built> {
    let QueryResults {
        contents,
        files,
        hashes,
        urls,
    } = run_queries(&respiratory_queries(&since(today)))?;
    let latest = latest_date(&contents);
    Ok(Built {
        entry: SnapshotEntry {
            id: "cdc-respiratory-ma".into(),
            title: "CDC respiratory virus surveillance for Massachusetts (NHSN, NSSP, NREVSS, ARI level)".into(),
            publisher: "Centers for Disease Control and Prevention".into(),
            source_urls: urls,
            source_sha256: hashes,
            source_vintage: format!("weeks ending through {latest}"),
            retrieved: today.to_string(),
            license: CDC_LICENSE.into(),
            derivation: "The last 13 weeks of rows returned by data.cdc.gov for NHSN Weekly \
                Hospital Respiratory Admission Levels and Rates (vdzy-6i9v; MA and USA), NSSP \
                ED visit trajectories (rdmq-nq56; Massachusetts and United States, state level), \
                NREVSS percent positivity for SARS-CoV-2 (gvsb-yw6g) and RSV (3cxc-4k8q) for \
                HHS Region 1 and the nation, national percent of tests positive (seuz-s2cv), \
                and the acute respiratory illness activity level by state (f3zz-zga5), with \
                the columns the tool uses. Built by `cargo xtask build-surveillance-snapshots`."
                .into(),
            caveats: vec![
                "Snapshot of recent weeks only; data are provisional and CDC revises them."
                    .into(),
            ],
            files,
        },
        contents,
    })
}

fn build_wastewater(today: NaiveDate) -> Result<Built> {
    let QueryResults {
        contents,
        files,
        hashes,
        urls,
    } = run_queries(&wastewater_queries(&since(today)))?;
    let latest = latest_date(&contents);
    Ok(Built {
        entry: SnapshotEntry {
            id: "cdc-nwss-ma".into(),
            title: "CDC wastewater viral activity levels for Massachusetts sampling sites (NWSS)"
                .into(),
            publisher: "CDC National Wastewater Surveillance System".into(),
            source_urls: urls,
            source_sha256: hashes,
            source_vintage: format!("weeks ending through {latest}"),
            retrieved: today.to_string(),
            license: CDC_LICENSE.into(),
            derivation: "The last 13 weeks of site-level wastewater viral activity levels \
                (WVAL) for SARS-CoV-2, influenza A, and RSV at Massachusetts sites \
                (atcp-73re), as returned by data.cdc.gov. Built by `cargo xtask \
                build-surveillance-snapshots`."
                .into(),
            caveats: vec![
                "CDC identifies sites only by an arbitrary number, counties served, and \
                 population served."
                    .into(),
            ],
            files,
        },
        contents,
    })
}

// ---------------------------------------------------------------------------
// MWRA Deer Island (Biobot), final data
// ---------------------------------------------------------------------------

/// Column boundaries (PDF points) of the MWRA data table: Southern, Northern,
/// Southern 7-day average, Northern 7-day average. Taken from the header
/// positions on page 1 (left edges 91.9, 140.5, 189.1, 237.7, and 286.3 for
/// the next column) less about 2 points, because numbers are right-aligned
/// to just before the next column's left edge.
const MWRA_BOUNDS: [f64; 5] = [89.0, 138.5, 187.1, 235.7, 284.3];
const MWRA_COLUMNS: [&str; 4] = [
    "southern_copies_per_ml",
    "northern_copies_per_ml",
    "southern_7day_avg",
    "northern_7day_avg",
];

struct Word {
    page: u32,
    left: f64,
    top: f64,
    width: f64,
    text: String,
}

fn pdf_words(pdf: &[u8]) -> Result<Vec<Word>> {
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("mwra.pdf");
    let output = dir.path().join("mwra.tsv");
    std::fs::write(&input, pdf)?;
    let status = std::process::Command::new("pdftotext")
        .arg("-tsv")
        .arg(&input)
        .arg(&output)
        .status()
        .context("running pdftotext (install poppler-utils)")?;
    if !status.success() {
        bail!("pdftotext failed: {status}");
    }
    let tsv = std::fs::read_to_string(&output)?;
    let mut words = Vec::new();
    for line in tsv.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 12 || f[0] != "5" {
            continue;
        }
        words.push(Word {
            page: f[1].parse()?,
            left: f[6].parse()?,
            top: f[7].parse()?,
            width: f[8].parse()?,
            text: f[11].trim().to_string(),
        });
    }
    Ok(words)
}

fn build_mwra(today: NaiveDate) -> Result<Built> {
    let pdf = fetch(MWRA_PDF_URL, None)?;
    let hash = sha256(&pdf);
    if hash != MWRA_PDF_SHA256 {
        bail!(
            "MWRA's data PDF changed (sha256 {hash}); MWRA said the Biobot series was final. \
             Check {MWRA_PAGE_URL}, re-verify the column layout, and update MWRA_PDF_SHA256."
        );
    }
    let words = pdf_words(&pdf)?;
    // Check the header layout this parser relies on.
    for (label, left) in [
        ("Southern", 91.88),
        ("Northern", 140.48),
        ("Southern", 189.08),
        ("Northern", 237.68),
    ] {
        if !words
            .iter()
            .any(|w| w.page == 1 && w.text == label && (w.left - left).abs() < 1.0)
        {
            bail!("MWRA PDF header layout changed (no '{label}' at x={left})");
        }
    }
    let mut lines: BTreeMap<(u32, i64), Vec<&Word>> = BTreeMap::new();
    for w in &words {
        lines
            .entry((w.page, w.top.round() as i64))
            .or_default()
            .push(w);
    }
    let mut series: BTreeMap<NaiveDate, [String; 4]> = BTreeMap::new();
    for ws in lines.values() {
        let mut ws = ws.clone();
        ws.sort_by(|a, b| a.left.total_cmp(&b.left));
        let Some(first) = ws.first() else { continue };
        let Ok(date) = NaiveDate::parse_from_str(&first.text, "%m/%d/%Y") else {
            continue;
        };
        let mut vals: [String; 4] = Default::default();
        for w in &ws[1..] {
            let center = w.left + w.width / 2.0;
            if center >= MWRA_BOUNDS[4] {
                continue; // confidence intervals and variant columns
            }
            let col = MWRA_BOUNDS
                .windows(2)
                .position(|b| center >= b[0] && center < b[1])
                .ok_or_else(|| anyhow!("{date}: value '{}' outside the table", w.text))?;
            // Codes printed in the table (no legend in the PDF); kept as printed.
            let value = if w.text.eq_ignore_ascii_case("nd") {
                "nd".to_string()
            } else if w.text == "NS" {
                "NS".to_string()
            } else {
                let v = w.text.replace(',', "");
                if v.parse::<f64>().is_err() {
                    bail!("{date}: unexpected value '{}'", w.text);
                }
                v
            };
            if !vals[col].is_empty() {
                bail!("{date}: two values in column {}", MWRA_COLUMNS[col]);
            }
            vals[col] = value;
        }
        if vals.iter().any(|v| !v.is_empty()) {
            series.insert(date, vals);
        }
    }
    let last = *series
        .keys()
        .next_back()
        .ok_or_else(|| anyhow!("no MWRA data rows parsed"))?;
    let start = last - Duration::days(365);
    let mut csv = format!("sample_date,{}\n", MWRA_COLUMNS.join(","));
    let mut n = 0;
    for (d, v) in series.range(start..) {
        csv.push_str(&format!("{d},{}\n", v.join(",")));
        n += 1;
    }
    let path = "mwra_biobot.csv";
    let mut hashes = BTreeMap::new();
    hashes.insert("mwradata20260730-data.pdf".into(), hash);
    Ok(Built {
        entry: SnapshotEntry {
            id: "mwra-deer-island-biobot".into(),
            title: "MWRA Deer Island wastewater SARS-CoV-2 (Biobot), final 12 months".into(),
            publisher:
                "Massachusetts Water Resources Authority (MWRA); testing by Biobot Analytics".into(),
            source_urls: vec![MWRA_PAGE_URL.into(), MWRA_PDF_URL.into()],
            source_sha256: hashes,
            source_vintage: format!("samples collected through {last} (final)"),
            retrieved: today.to_string(),
            license: "Public data published by MWRA, a Massachusetts public authority.".into(),
            derivation: format!(
                "Sample date, Southern and Northern system SARS-CoV-2 RNA (copies/mL), and \
                 their 7-day averages for {start} to {last}, read from MWRA's data PDF with \
                 `pdftotext -tsv` and assigned to columns by x position (boundaries {:?} \
                 points, checked against the page-1 header). The codes 'nd' (also printed \
                 'ND') and 'NS' are kept as printed; the PDF has no legend for them. \
                 Confidence intervals and variant results are not kept. Built by `cargo xtask \
                 build-surveillance-snapshots`.",
                MWRA_BOUNDS
            ),
            caveats: vec![
                "MWRA's Biobot testing ended with samples collected through 2026-07-27; DPH \
                 is moving Deer Island testing to its own program and dashboard."
                    .into(),
                "Values are as transcribed from a PDF table; check the PDF before quoting a \
                 single value."
                    .into(),
            ],
            files: vec![file_entry(path, &csv, n)],
        },
        contents: vec![(path.into(), csv)],
    })
}
