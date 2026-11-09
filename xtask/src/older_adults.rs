//! `cargo xtask build-older-adult-snapshots [--source-dir DIR]`: build the
//! `census-acs5-ma-older-adults` snapshot (older adults living alone, without
//! a vehicle, with limited English, and in poverty) from the ACS 5-year
//! table-based summary files on www2.census.gov (no API key needed; about
//! 460 MB of downloads, or read from DIR), for the nation, Massachusetts,
//! its counties, its 351 cities and towns, and the tracts in
//! `snapshots/ma_census_geographies.csv`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use phn_data::census::{self, Cells, Estimate};
use phn_data::older_adults::{self, INDICATORS, SNAPSHOT_FILE, SNAPSHOT_ID, TABLES};

use crate::download::{fetch, sha256};
use crate::snapshots::{Manifest, SnapshotEntry, SnapshotFile};

const SF_BASE: &str =
    "https://www2.census.gov/programs-surveys/acs/summary_file/2024/table-based-SF/data/5YRData";

pub fn run(root: &Path, source_dir: Option<&Path>) -> Result<()> {
    let snap_dir = root.join("snapshots");
    let manifest_path = snap_dir.join("manifest.json");
    let mut manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).context("reading snapshots/manifest.json")?,
    )?;
    // Geographies: name and level by GEOID.
    let geo_text = std::fs::read_to_string(snap_dir.join("ma_census_geographies.csv"))?;
    let mut rdr = csv::Reader::from_reader(geo_text.as_bytes());
    let mut geos: Vec<(String, String, String)> = vec![
        ("US".into(), "nation".into(), "United States".into()),
        ("25".into(), "state".into(), "Massachusetts".into()),
    ];
    for r in rdr.records() {
        let r = r?;
        let level = r.get(1).unwrap_or_default();
        if matches!(level, "county" | "town" | "tract") {
            geos.push((
                r.get(0).unwrap_or_default().to_string(),
                level.to_string(),
                r.get(2).unwrap_or_default().to_string(),
            ));
        }
    }
    let wanted: HashSet<String> = geos.iter().map(|g| g.0.clone()).collect();
    let needed: HashSet<&str> = older_adults::required_cells().into_iter().collect();
    let mut cells: HashMap<String, Cells> = HashMap::new();
    let mut hashes = BTreeMap::new();
    let mut urls = Vec::new();
    for t in TABLES {
        let name = format!("acsdt5y2024-{}.dat", t.to_lowercase());
        let url = format!("{SF_BASE}/{name}");
        let bytes = fetch(&url, source_dir)?;
        hashes.insert(name.clone(), sha256(&bytes));
        urls.push(url);
        let text = String::from_utf8(bytes)?;
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().unwrap_or_default().split('|').collect();
        let cols: Vec<Option<(String, bool)>> = header
            .iter()
            .map(|h| {
                let (tbl, rest) = h.split_once('_')?;
                let (kind, line) = rest.split_at(1);
                let id = format!("{tbl}_{line}");
                needed.contains(id.as_str()).then_some((id, kind == "M"))
            })
            .collect();
        for line in lines {
            let Some((geo, _)) = line.split_once('|') else {
                continue;
            };
            let geoid = match geo {
                "0100000US" => "US".to_string(),
                "0400000US25" => "25".to_string(),
                g => match g.split_once("US") {
                    Some((p, id))
                        if matches!(p, "0500000" | "0600000" | "1400000") && id.starts_with("25") =>
                    {
                        id.to_string()
                    }
                    _ => continue,
                },
            };
            if !wanted.contains(&geoid) {
                continue;
            }
            let entry = cells.entry(geoid).or_default();
            let mut est: HashMap<String, f64> = HashMap::new();
            let mut moe: HashMap<String, f64> = HashMap::new();
            for (i, v) in line.split('|').enumerate().skip(1) {
                let Some(Some((cell, is_moe))) = cols.get(i) else {
                    continue;
                };
                let Ok(x) = v.trim().parse::<f64>() else {
                    continue;
                };
                if *is_moe {
                    moe.insert(cell.clone(), x);
                } else {
                    est.insert(cell.clone(), x);
                }
            }
            for (cell, v) in est {
                if let Some(value) = census::decode_estimate(v) {
                    let m = moe.get(&cell).copied().and_then(census::decode_moe);
                    entry.insert(cell, Estimate { value, moe: m });
                }
            }
        }
        println!("read {name}");
    }
    let mut header = vec!["geoid".to_string(), "level".to_string(), "name".to_string()];
    for d in INDICATORS {
        header.push(d.id.to_string());
        header.push(format!("{}_moe", d.id));
    }
    let mut out = header.join(",") + "\n";
    let mut written = 0;
    for (geoid, level, name) in &geos {
        let Some(c) = cells.get(geoid) else {
            if level != "tract" {
                bail!("no ACS cells for {name} ({geoid})");
            }
            continue;
        };
        let values = older_adults::compute_all(c);
        if level == "tract" && values.get("pop65").is_none_or(|e| e.value == 0.0) {
            continue;
        }
        let mut row = vec![geoid.clone(), level.clone(), csv_field(name)];
        for d in INDICATORS {
            match values.get(d.id) {
                Some(e) => {
                    row.push(fmt(d.id, e.value));
                    row.push(e.moe.map(|m| fmt(d.id, m)).unwrap_or_default());
                }
                None => {
                    row.push(String::new());
                    row.push(String::new());
                }
            }
        }
        out.push_str(&row.join(","));
        out.push('\n');
        written += 1;
    }
    std::fs::write(snap_dir.join(SNAPSHOT_FILE), &out)?;
    println!("wrote snapshots/{SNAPSHOT_FILE} ({written} rows, {} bytes)", out.len());
    let today = chrono::Local::now().date_naive().to_string();
    let entry = SnapshotEntry {
        id: SNAPSHOT_ID.into(),
        title: "ACS 2020-2024 5-year estimates: older adults living alone, without a vehicle, with limited English, and in poverty (Massachusetts)".into(),
        publisher: "U.S. Census Bureau, American Community Survey".into(),
        source_urls: urls,
        source_sha256: hashes,
        source_vintage: "2020-2024".into(),
        retrieved: today,
        license: "U.S. federal government work (public domain).".into(),
        derivation: "From the ACS 5-year table-based summary file (2024 release): B09020 (living alone: male and female householders living alone, of the population 65+), B25045 (householder 65+ with no vehicle, owner and renter), B16004 (65+ speaking English 'well', 'not well', or 'not at all', i.e. less than 'very well'), and B17001 (65-74 and 75+ below poverty, of those for whom poverty is determined), for the nation, Massachusetts, its counties, its 351 county subdivisions, and populated tracts (tracts with no adults 65+ are omitted). MOEs: sqrt(sum of squared MOEs) for sums, the Census Bureau's derived-proportion formula for percentages (phn_data::older_adults, phn_data::census).".into(),
        caveats: vec![
            "Small-area ACS estimates, especially for tracts, can have large margins of error; check the coefficient of variation before ranking areas.".into(),
            "Living alone counts householders 65+ living alone; people 65+ in group quarters (for example nursing homes) are in the denominator but cannot live alone.".into(),
        ],
        files: vec![SnapshotFile {
            path: SNAPSHOT_FILE.into(),
            sha256: sha256(out.as_bytes()),
            bytes: out.len() as u64,
            rows: written as u64,
        }],
    };
    match manifest.snapshots.iter_mut().find(|e| e.id == entry.id) {
        Some(e) => *e = entry,
        None => manifest.snapshots.push(entry),
    }
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)? + "\n")?;
    println!("Updated snapshots/manifest.json.");
    Ok(())
}

fn fmt(id: &str, v: f64) -> String {
    if id.ends_with("_pct") {
        let r = (v * 10.0).round() / 10.0;
        format!("{r}")
    } else {
        format!("{}", v.round())
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
