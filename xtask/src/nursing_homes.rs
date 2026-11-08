//! `cargo xtask build-nursing-home-snapshot`: rebuild the `cms-ma-nursing-homes`
//! snapshot (Massachusetts nursing homes from the CMS Provider Data Catalog)
//! with the same queries and assembly the server uses live
//! (`phn_data::cms_provider`), and update its manifest entry.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use phn_data::cms_provider::{
    self, AVERAGES_FILE, CITATIONS_FILE, FACILITIES_FILE, SNAPSHOT_ID, assemble, ma_queries,
    parse_page, query_url,
};

use crate::download::{fetch, sha256};
use crate::snapshots::{Manifest, SnapshotEntry, SnapshotFile};

const LICENSE: &str = "U.S. federal government work (CMS Provider Data Catalog); public domain in the United States.";

fn file_entry(path: &str, text: &str) -> SnapshotFile {
    SnapshotFile {
        path: path.to_string(),
        sha256: sha256(text.as_bytes()),
        bytes: text.len() as u64,
        rows: text.lines().count().saturating_sub(1) as u64,
    }
}

pub fn run(root: &Path) -> Result<()> {
    let snap_dir = root.join("snapshots");
    let manifest_path = snap_dir.join("manifest.json");
    let mut manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).context("reading snapshots/manifest.json")?,
    )?;
    let today = chrono::Local::now().date_naive().to_string();
    let mut sets = Vec::new();
    let mut urls = Vec::new();
    for (dataset, conds, props) in ma_queries() {
        let mut rows = Vec::new();
        let mut offset = 0;
        loop {
            let url = query_url(dataset, &conds, &props, offset);
            let body = fetch(&url, None)?;
            let (count, page) = parse_page(&body).map_err(|e| anyhow::anyhow!("{e}"))?;
            let n = page.len();
            rows.extend(page);
            offset += n;
            if n == 0 || offset >= count {
                break;
            }
        }
        println!("{dataset}: {} rows", rows.len());
        urls.push(format!("{}/{dataset}/0", cms_provider::API));
        sets.push(rows);
    }
    let (facilities, citations, averages) = assemble(&sets);
    if facilities.len() < 300 {
        bail!("only {} Massachusetts nursing homes returned", facilities.len());
    }
    let processing = facilities
        .iter()
        .map(|f| f.processing_date.clone())
        .max()
        .unwrap_or_default();
    let f_text = cms_provider::to_csv(&facilities).map_err(|e| anyhow::anyhow!("{e}"))?;
    let c_text = cms_provider::to_csv(&citations).map_err(|e| anyhow::anyhow!("{e}"))?;
    let a_text = cms_provider::to_csv(&averages).map_err(|e| anyhow::anyhow!("{e}"))?;
    for (p, t) in [(FACILITIES_FILE, &f_text), (CITATIONS_FILE, &c_text), (AVERAGES_FILE, &a_text)] {
        std::fs::write(snap_dir.join(p), t)?;
        println!("wrote snapshots/{p} ({} bytes)", t.len());
    }
    let entry = SnapshotEntry {
        id: SNAPSHOT_ID.into(),
        title: "Massachusetts nursing homes: CMS star ratings, staffing, infection control citations, and vaccination".into(),
        publisher: "Centers for Medicare & Medicaid Services, Provider Data Catalog (Care Compare)".into(),
        source_urls: urls,
        source_sha256: BTreeMap::new(),
        source_vintage: processing.clone(),
        retrieved: today,
        license: LICENSE.into(),
        derivation: "Massachusetts rows from Provider Information (4pq5-n9py; selected columns), SNF Quality Reporting Program measures S_040_02 (staff COVID-19 up to date), S_041_01 (staff flu), and S_045_01 (residents COVID-19 up to date) (fykj-qjee), MDS quality measures 415, 454, 430, and 472 (resident pneumococcal and flu) (djen-97ju), Health Deficiencies in the 'Infection Control Deficiencies' category (r5ix-sfxw), and State/US Averages (xcdc-v8bm), joined by CMS Certification Number (xtask/src/nursing_homes.rs, phn_data::cms_provider).".into(),
        caveats: vec![
            format!("CMS processing date {processing}; CMS refreshes Care Compare monthly. Star ratings and citations change after each inspection."),
            "Vaccination measures cover different periods (SNF QRP quarters; MDS four-quarter averages); CMS removed its separate COVID-19 vaccination datasets in July 2025.".into(),
        ],
        files: vec![
            file_entry(FACILITIES_FILE, &f_text),
            file_entry(CITATIONS_FILE, &c_text),
            file_entry(AVERAGES_FILE, &a_text),
        ],
    };
    match manifest.snapshots.iter_mut().find(|e| e.id == entry.id) {
        Some(e) => *e = entry,
        None => manifest.snapshots.push(entry),
    }
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)? + "\n")?;
    println!("Updated snapshots/manifest.json.");
    Ok(())
}
