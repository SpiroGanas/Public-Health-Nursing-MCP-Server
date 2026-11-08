//! `cargo xtask build-public-snapshots`: Phase 4 public-data snapshots.
//!
//! Rebuilds, from the original public sources, the offline copies behind the
//! community profile, language access, SVI, PLACES, and map tools:
//!
//! | Snapshot id | Files | Source |
//! |---|---|---|
//! | `ma-census-geographies` | `ma_census_geographies.csv` | Census Gazetteer 2024, 2020 block assignment and relationship files |
//! | `ma-boundaries` | `ma_boundaries_{towns,tracts,zctas,school_districts}.csv` | Census cartographic boundary files (500k) |
//! | `census-acs5-ma` | `ma_acs5_indicators.csv`, `ma_acs5_languages_detailed.csv` | ACS 2020–2024 5-year table-based summary file |
//! | `cdc-svi-2022-ma` | `ma_svi_2022.csv` | CDC/ATSDR SVI 2022 (Massachusetts and U.S. files) |
//! | `cdc-places-2025-ma` | `ma_places_2025.csv`, `ma_places_2025_measures.csv` | CDC PLACES 2025 release (data.cdc.gov) |
//!
//! The ACS snapshot is built from the keyless summary files on
//! www2.census.gov (about 1.1 GB of downloads), not from api.census.gov,
//! which needs a key from some networks. `--source-dir DIR` caches every
//! download in DIR and reuses it (useful offline or when iterating).
//! `--only ID` rebuilds one snapshot. `--check` compares without writing.
//!
//! Every other manifest entry is kept unchanged.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use geo::{Coord, LineString, MultiPolygon, Polygon, Simplify};
use phn_data::census::{self, Cells, Estimate};
use shapefile::dbase::{FieldValue, Record};

use crate::download::sha256;
use crate::snapshots::{Manifest, SnapshotEntry, SnapshotFile};

const ACS_VINTAGE: u32 = census::SNAPSHOT_VINTAGE;
const SF_BASE: &str =
    "https://www2.census.gov/programs-surveys/acs/summary_file/2024/table-based-SF/data/5YRData";
const GAZ_BASE: &str = "https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2024_Gazetteer";
const GAZ2020_BASE: &str =
    "https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2020_Gazetteer";
const CB_BASE: &str = "https://www2.census.gov/geo/tiger/GENZ2024/shp";
const ZCTA_CB: &str = "https://www2.census.gov/geo/tiger/GENZ2020/shp/cb_2020_us_zcta520_500k.zip";
const BAF: &str = "https://www2.census.gov/geo/docs/maps-data/data/baf2020/BlockAssign_ST25_MA.zip";
const ZCTA_REL: &str =
    "https://www2.census.gov/geo/docs/maps-data/data/rel2020/zcta520/tab20_zcta520_cousub20_natl.txt";
const PUMA_REL: &str =
    "https://www2.census.gov/geo/docs/maps-data/data/rel2020/2020_Census_Tract_to_2020_PUMA.txt";
const SVI_MA: &str = "https://svi.cdc.gov/Documents/Data/2022/csv/states/Massachusetts.csv";
const SVI_US: &str = "https://svi.cdc.gov/Documents/Data/2022/csv/states/SVI_2022_US.csv";

/// PLACES 2025 release dataset ids on data.cdc.gov.
pub const PLACES_COUNTY: &str = "swc5-untb";
pub const PLACES_PLACE: &str = "eav7-hnsx";
pub const PLACES_TRACT: &str = "cwsq-ngmh";
pub const PLACES_ZCTA: &str = "qnzd-25i4";

const COMMONWEALTH_LICENSE: &str = "U.S. federal government work (public domain).";

struct Built {
    entry: SnapshotEntry,
    contents: Vec<(String, String)>,
}

/// Downloads with an optional on-disk cache.
struct Downloader {
    dir: Option<PathBuf>,
    client: reqwest::blocking::Client,
    hashes: std::cell::RefCell<BTreeMap<String, String>>,
}

impl Downloader {
    fn new(dir: Option<PathBuf>) -> Result<Self> {
        if let Some(d) = &dir {
            std::fs::create_dir_all(d)?;
        }
        Ok(Self {
            dir,
            client: reqwest::blocking::Client::builder()
                .user_agent(concat!(
                    "phn-mcp-server-xtask/",
                    env!("CARGO_PKG_VERSION")
                ))
                .timeout(Duration::from_secs(900))
                .build()?,
            hashes: Default::default(),
        })
    }

    /// Download `url` (or read the cached copy named `name`).
    fn get(&self, url: &str, name: &str) -> Result<Vec<u8>> {
        let cached = self.dir.as_ref().map(|d| d.join(name));
        let bytes = match &cached {
            Some(p) if p.exists() => std::fs::read(p)?,
            _ => {
                eprintln!("downloading {url}");
                let resp = self
                    .client
                    .get(url)
                    .send()
                    .with_context(|| format!("GET {url}"))?
                    .error_for_status()
                    .with_context(|| format!("GET {url}"))?;
                let b = resp.bytes()?.to_vec();
                if let Some(p) = &cached {
                    std::fs::write(p, &b)?;
                }
                b
            }
        };
        self.hashes
            .borrow_mut()
            .insert(name.to_string(), sha256(&bytes));
        Ok(bytes)
    }

    fn take_hashes(&self) -> BTreeMap<String, String> {
        std::mem::take(&mut self.hashes.borrow_mut())
    }
}

pub fn run(root: &Path, check: bool, source_dir: Option<PathBuf>, only: Option<String>) -> Result<()> {
    let snap_dir = root.join("snapshots");
    let manifest_path = snap_dir.join("manifest.json");
    let old: Manifest = serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)
        .context("parsing snapshots/manifest.json")?;
    let today = chrono::Local::now().date_naive().to_string();
    let dl = Downloader::new(source_dir)?;
    let wanted = |id: &str| only.as_deref().is_none_or(|o| o == id);

    // The geography crosswalk is needed by every other builder.
    let geos = build_geographies(root, &dl)?;
    let mut built = Vec::new();
    if wanted("ma-census-geographies") {
        built.push(geos.built(&today, dl.take_hashes()));
    } else {
        dl.take_hashes();
    }
    if wanted("ma-boundaries") {
        built.push(build_boundaries(root, &dl, &geos, &today)?);
    }
    if wanted("census-acs5-ma") {
        built.push(build_acs(&dl, &geos, &today)?);
    }
    if wanted("cdc-svi-2022-ma") {
        built.push(build_svi(&dl, &today)?);
    }
    if wanted("cdc-places-2025-ma") {
        built.push(build_places(&dl, &geos, &today)?);
    }

    let mut changed = false;
    let mut report = String::new();
    let mut entries = old.snapshots.clone();
    for b in built {
        let mut entry = b.entry;
        let old_entry = old.snapshots.iter().find(|e| e.id == entry.id);
        let mut entry_changed = old_entry.is_none();
        for (path, text) in &b.contents {
            let committed = std::fs::read_to_string(snap_dir.join(path)).unwrap_or_default();
            if committed != *text {
                entry_changed = true;
                let _ = writeln!(
                    report,
                    "{path}: {} → {} bytes",
                    committed.len(),
                    text.len()
                );
            }
        }
        if let Some(o) = old_entry
            && o.source_sha256 != entry.source_sha256
        {
            entry_changed = true;
            let _ = writeln!(report, "{}: upstream source files changed", entry.id);
        }
        if !entry_changed && let Some(o) = old_entry {
            entry.retrieved = o.retrieved.clone();
        }
        changed |= entry_changed;
        if !check {
            for (path, text) in &b.contents {
                std::fs::write(snap_dir.join(path), text)?;
            }
        }
        match entries.iter_mut().find(|e| e.id == entry.id) {
            Some(slot) => *slot = entry,
            None => entries.push(entry),
        }
    }
    if check {
        if changed {
            println!("Public-data snapshot differences found:\n{report}");
            bail!("public-data snapshots are out of date; run `cargo xtask build-public-snapshots`");
        }
        println!("Public-data snapshots are up to date.");
        return Ok(());
    }
    let manifest = Manifest {
        format_version: old.format_version,
        snapshots: entries,
    };
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    if changed {
        println!("Public-data snapshots updated:\n{report}");
    } else {
        println!("Public-data snapshots unchanged.");
    }
    Ok(())
}

fn file_entry(path: &str, text: &str) -> SnapshotFile {
    SnapshotFile {
        path: path.into(),
        sha256: sha256(text.as_bytes()),
        bytes: text.len() as u64,
        rows: text.lines().count().saturating_sub(1) as u64,
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn unzip_member(zip_bytes: &[u8], suffix: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        if f.name()?.ends_with(suffix) {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    bail!("{suffix} not found in archive")
}

fn extract_shapefile(zip_bytes: &[u8], dir: &Path) -> Result<PathBuf> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut shp = None;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let name = f.name()?.rsplit('/').next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        std::fs::write(dir.join(&name), buf)?;
        if name.ends_with(".shp") {
            shp = Some(dir.join(&name));
        }
    }
    shp.ok_or_else(|| anyhow!("no .shp in archive"))
}

fn rec_text(rec: &Record, field: &str) -> Option<String> {
    match rec.get(field) {
        Some(FieldValue::Character(Some(s))) => Some(s.trim().to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Geography crosswalk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct GeoRow {
    geoid: String,
    level: &'static str,
    name: String,
    census_name: String,
    county: String,
    town: String,
    shares: Vec<(String, f64)>,
    aland_sqmi: Option<f64>,
    puma: String,
}

struct Geos {
    rows: Vec<GeoRow>,
    /// Census cousub GEOID → MassGIS town name.
    town_by_geoid: BTreeMap<String, String>,
    tracts: BTreeSet<String>,
    zctas: BTreeSet<String>,
    pumas: BTreeSet<String>,
}

impl Geos {
    fn built(&self, today: &str, hashes: BTreeMap<String, String>) -> Built {
        let mut out = String::from(
            "geoid,level,name,census_name,county_geoid,town,town_shares,aland_sqmi,puma\n",
        );
        for r in &self.rows {
            let shares = r
                .shares
                .iter()
                .map(|(t, s)| format!("{t}:{s:.3}"))
                .collect::<Vec<_>>()
                .join(";");
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{}",
                r.geoid,
                r.level,
                csv_field(&r.name),
                csv_field(&r.census_name),
                r.county,
                csv_field(&r.town),
                csv_field(&shares),
                r.aland_sqmi.map(|a| format!("{a:.3}")).unwrap_or_default(),
                r.puma
            );
        }
        let path = "ma_census_geographies.csv";
        Built {
            entry: SnapshotEntry {
                id: "ma-census-geographies".into(),
                title: "Massachusetts census geographies: counties, cities and towns (county subdivisions), 2020 census tracts, and ZCTAs, with crosswalks".into(),
                publisher: "U.S. Census Bureau; crosswalk derived by this project".into(),
                source_urls: vec![
                    format!("{GAZ_BASE}/2024_gaz_cousubs_25.txt"),
                    format!("{GAZ_BASE}/2024_gaz_tracts_25.txt"),
                    format!("{GAZ2020_BASE}/2020_gaz_cousubs_25.txt"),
                    BAF.into(),
                    ZCTA_REL.into(),
                    PUMA_REL.into(),
                ],
                source_sha256: hashes,
                source_vintage: "2024 Gazetteer; 2020 census geography".into(),
                retrieved: today.into(),
                license: COMMONWEALTH_LICENSE.into(),
                derivation: "Towns: the 351 Massachusetts county subdivisions in the 2024 Census Gazetteer, matched to MassGIS town names (Census suffixes such as 'city', 'town', and 'Town city' removed). Tracts: 2020 census tracts; the town of each tract and its share come from the 2020 block assignment file (share of the tract's census blocks in each town). ZCTAs: 2020 ZCTAs that overlap a Massachusetts town in the 2020 ZCTA-to-county-subdivision relationship file; town shares are shares of the ZCTA's land area. PUMAs: 2020 tract-to-PUMA relationship file. Land area in square miles from the Gazetteer.".into(),
                caveats: vec![
                    "Tract-to-town shares count census blocks, not people; a few tracts cross town lines.".into(),
                    "ZIP Code Tabulation Areas approximate USPS ZIP Codes and often cross town lines; town shares are by land area.".into(),
                ],
                files: vec![file_entry(path, &out)],
            },
            contents: vec![(path.into(), out)],
        }
    }
}

/// Strip Census suffixes: "Barnstable Town city" → "Barnstable".
fn census_town_base(name: &str) -> String {
    let mut n = name.trim().to_string();
    for suffix in [" city", " town"] {
        if let Some(s) = n.strip_suffix(suffix) {
            n = s.to_string();
            break;
        }
    }
    if let Some(s) = n.strip_suffix(" Town") {
        n = s.to_string();
    }
    n
}

fn norm(s: &str) -> String {
    phn_core::text::normalize(s)
}

fn parse_gazetteer(text: &str) -> Vec<HashMap<String, String>> {
    let mut lines = text.lines();
    let header: Vec<String> = lines
        .next()
        .unwrap_or_default()
        .split('\t')
        .map(|h| h.trim().trim_start_matches('\u{feff}').to_string())
        .collect();
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            header
                .iter()
                .cloned()
                .zip(l.split('\t').map(|v| v.trim().to_string()))
                .collect()
        })
        .collect()
}

fn build_geographies(root: &Path, dl: &Downloader) -> Result<Geos> {
    // MassGIS town names and counties (the server's reference list).
    let munis = std::fs::read_to_string(root.join("snapshots/ma_municipalities.csv"))?;
    let mut massgis: BTreeMap<String, (String, String)> = BTreeMap::new(); // norm → (name, county_fips)
    let mut county_names: BTreeMap<String, String> = BTreeMap::new();
    for rec in csv::Reader::from_reader(munis.as_bytes()).records() {
        let rec = rec?;
        let name = rec[1].to_string();
        let county = rec[3].to_string();
        let fips = rec[4].to_string();
        county_names.insert(fips.clone(), format!("{county} County"));
        massgis.insert(norm(&name), (name, fips));
    }

    let gaz = String::from_utf8(dl.get(
        &format!("{GAZ_BASE}/2024_gaz_cousubs_25.txt"),
        "2024_gaz_cousubs_25.txt",
    )?)?;
    let mut rows = Vec::new();
    let mut town_by_geoid = BTreeMap::new();
    let mut geoid_by_town = BTreeMap::new();
    for (fips, name) in &county_names {
        rows.push(GeoRow {
            geoid: fips.clone(),
            level: "county",
            name: name.clone(),
            census_name: name.clone(),
            county: fips.clone(),
            town: String::new(),
            shares: vec![],
            aland_sqmi: None,
            puma: String::new(),
        });
    }
    for g in parse_gazetteer(&gaz) {
        let geoid = g["GEOID"].clone();
        let cname = g["NAME"].clone();
        if cname.contains("not defined") {
            continue;
        }
        let base = census_town_base(&cname);
        let (town, county) = massgis
            .get(&norm(&base))
            .cloned()
            .ok_or_else(|| anyhow!("Census county subdivision '{cname}' has no MassGIS town"))?;
        if county != geoid[..5] {
            bail!("{cname}: county {} differs from MassGIS {county}", &geoid[..5]);
        }
        town_by_geoid.insert(geoid.clone(), town.clone());
        geoid_by_town.insert(town.clone(), geoid.clone());
        rows.push(GeoRow {
            geoid,
            level: "town",
            name: town,
            census_name: cname,
            county,
            town: String::new(),
            shares: vec![],
            aland_sqmi: g["ALAND_SQMI"].parse().ok(),
            puma: String::new(),
        });
    }
    if geoid_by_town.len() != 351 {
        bail!("matched {} towns, expected 351", geoid_by_town.len());
    }
    // County land area: sum of towns.
    let mut county_area: BTreeMap<String, f64> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.level == "town") {
        *county_area.entry(r.county.clone()).or_default() += r.aland_sqmi.unwrap_or(0.0);
    }
    for r in rows.iter_mut().filter(|r| r.level == "county") {
        r.aland_sqmi = county_area.get(&r.geoid).copied();
    }

    // The 2020 relationship files use 2020 county subdivision codes; a few
    // changed since (for example Methuen Town city 40710 → Methuen city 40675).
    let gaz20 = String::from_utf8(dl.get(
        &format!("{GAZ2020_BASE}/2020_gaz_cousubs_25.txt"),
        "2020_gaz_cousubs_25.txt",
    )?)?;
    let mut town_by_geoid_2020: BTreeMap<String, String> = BTreeMap::new();
    for g in parse_gazetteer(&gaz20) {
        let cname = &g["NAME"];
        if cname.contains("not defined") {
            continue;
        }
        let town = massgis
            .get(&norm(&census_town_base(cname)))
            .map(|t| t.0.clone())
            .ok_or_else(|| anyhow!("2020 county subdivision '{cname}' has no MassGIS town"))?;
        town_by_geoid_2020.insert(g["GEOID"].clone(), town);
    }
    if town_by_geoid_2020.len() != 351 {
        bail!("matched {} 2020 towns, expected 351", town_by_geoid_2020.len());
    }

    // Tract → town shares from the block assignment file.
    let baf = unzip_member(&dl.get(BAF, "BlockAssign_ST25_MA.zip")?, "_MCD.txt")?;
    let mut blocks: BTreeMap<String, BTreeMap<String, u32>> = BTreeMap::new();
    for line in String::from_utf8(baf)?.lines().skip(1) {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 3 || f[0].len() < 11 {
            continue;
        }
        let tract = f[0][..11].to_string();
        let cousub = format!("25{}{}", f[1], f[2]);
        if let Some(town) = town_by_geoid_2020.get(&cousub) {
            *blocks.entry(tract).or_default().entry(town.clone()).or_default() += 1;
        }
    }
    let puma_rel = String::from_utf8(dl.get(PUMA_REL, "2020_Census_Tract_to_2020_PUMA.txt")?)?;
    let mut puma_of: BTreeMap<String, String> = BTreeMap::new();
    for line in puma_rel.lines().skip(1) {
        let f: Vec<&str> = line.trim_start_matches('\u{feff}').split(',').collect();
        if f.len() == 4 && f[0] == "25" {
            puma_of.insert(format!("25{}{}", f[1], f[2]), format!("25{}", f[3].trim()));
        }
    }
    let gaz_t = String::from_utf8(dl.get(
        &format!("{GAZ_BASE}/2024_gaz_tracts_25.txt"),
        "2024_gaz_tracts_25.txt",
    )?)?;
    let mut tracts = BTreeSet::new();
    for g in parse_gazetteer(&gaz_t) {
        let geoid = g["GEOID"].clone();
        let Some(towns) = blocks.get(&geoid) else {
            continue;
        };
        let total: u32 = towns.values().sum();
        let mut shares: Vec<(String, f64)> = towns
            .iter()
            .map(|(t, n)| (t.clone(), f64::from(*n) / f64::from(total)))
            .collect();
        shares.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let county = geoid[..5].to_string();
        let code = &geoid[5..];
        let num = if code.ends_with("00") {
            code[..4].trim_start_matches('0').to_string()
        } else {
            format!("{}.{}", code[..4].trim_start_matches('0'), &code[4..])
        };
        let num = if num.starts_with('.') { format!("0{num}") } else { num };
        let county_name = county_names.get(&county).cloned().unwrap_or_default();
        tracts.insert(geoid.clone());
        rows.push(GeoRow {
            puma: puma_of.get(&geoid).cloned().unwrap_or_default(),
            name: format!("Census Tract {num}, {county_name}"),
            census_name: format!("Census Tract {num}"),
            geoid,
            level: "tract",
            county,
            town: shares[0].0.clone(),
            shares,
            aland_sqmi: g["ALAND_SQMI"].parse().ok(),
        });
    }

    // ZCTAs overlapping Massachusetts towns, with land-area shares.
    let rel = String::from_utf8(dl.get(ZCTA_REL, "tab20_zcta520_cousub20_natl.txt")?)?;
    let mut zcta_parts: BTreeMap<String, Vec<(String, f64, f64)>> = BTreeMap::new();
    for line in rel.lines().skip(1) {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 18 || f[1].is_empty() {
            continue;
        }
        let zcta = f[1].to_string();
        let cousub = f[9];
        let part: f64 = f[16].parse().unwrap_or(0.0);
        let zland: f64 = f[3].parse().unwrap_or(0.0);
        if let Some(town) = town_by_geoid_2020.get(cousub) {
            zcta_parts
                .entry(zcta)
                .or_default()
                .push((town.clone(), part, zland));
        }
    }
    let mut zctas = BTreeSet::new();
    for (zcta, parts) in zcta_parts {
        let zland = parts.first().map(|p| p.2).unwrap_or(0.0);
        let ma_land: f64 = parts.iter().map(|p| p.1).sum();
        // Keep ZCTAs whose land is mostly in Massachusetts.
        if zland <= 0.0 || ma_land / zland < 0.5 {
            continue;
        }
        let mut shares: Vec<(String, f64)> = parts
            .iter()
            .filter(|p| p.1 > 0.0)
            .map(|p| (p.0.clone(), p.1 / zland))
            .collect();
        shares.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        shares.retain(|s| s.1 >= 0.001);
        if shares.is_empty() {
            continue;
        }
        let county = geoid_by_town
            .get(&shares[0].0)
            .map(|g| g[..5].to_string())
            .unwrap_or_default();
        zctas.insert(zcta.clone());
        rows.push(GeoRow {
            name: format!("ZCTA {zcta}"),
            census_name: format!("ZCTA5 {zcta}"),
            geoid: zcta,
            level: "zcta",
            county,
            town: shares[0].0.clone(),
            shares,
            aland_sqmi: Some(zland / 2_589_988.110336),
            puma: String::new(),
        });
    }
    let pumas = puma_of.values().cloned().collect();
    eprintln!(
        "geographies: {} towns, {} tracts, {} ZCTAs",
        geoid_by_town.len(),
        tracts.len(),
        zctas.len()
    );
    Ok(Geos {
        rows,
        town_by_geoid,
        tracts,
        zctas,
        pumas,
    })
}

// ---------------------------------------------------------------------------
// Boundaries
// ---------------------------------------------------------------------------

/// NAD83 / Massachusetts Mainland (EPSG:26986): Lambert conformal conic,
/// two standard parallels, GRS 1980 ellipsoid. Snyder (1987), eq. 15-1 to 15-10.
pub fn to_ma_state_plane(lon: f64, lat: f64) -> (f64, f64) {
    const A: f64 = 6_378_137.0;
    const F: f64 = 1.0 / 298.257_222_101;
    let e2 = 2.0 * F - F * F;
    let e = e2.sqrt();
    let rad = |d: f64| d.to_radians();
    let (phi1, phi2, phi0, lam0) = (
        rad(42.0 + 41.0 / 60.0),
        rad(41.0 + 43.0 / 60.0),
        rad(41.0),
        rad(-71.5),
    );
    let m = |phi: f64| phi.cos() / (1.0 - e2 * phi.sin().powi(2)).sqrt();
    let t = |phi: f64| {
        (std::f64::consts::FRAC_PI_4 - phi / 2.0).tan()
            / ((1.0 - e * phi.sin()) / (1.0 + e * phi.sin())).powf(e / 2.0)
    };
    let n = (m(phi1).ln() - m(phi2).ln()) / (t(phi1).ln() - t(phi2).ln());
    let ff = m(phi1) / (n * t(phi1).powf(n));
    let rho = |phi: f64| A * ff * t(phi).powf(n);
    let rho0 = rho(phi0);
    let r = rho(rad(lat));
    let theta = n * (rad(lon) - lam0);
    (200_000.0 + r * theta.sin(), 750_000.0 + rho0 - r * theta.cos())
}

/// Read polygons from a shapefile: (key field value, projected multipolygon).
fn read_shapes(
    zip_bytes: &[u8],
    key: &dyn Fn(&Record) -> Option<String>,
) -> Result<Vec<(String, MultiPolygon<f64>)>> {
    let tmp = tempfile::tempdir()?;
    let shp = extract_shapefile(zip_bytes, tmp.path())?;
    let mut reader = shapefile::Reader::from_path(shp)?;
    let mut out = Vec::new();
    for item in reader.iter_shapes_and_records_as::<shapefile::Polygon, Record>() {
        let (shape, rec) = item?;
        let Some(k) = key(&rec) else { continue };
        let mp: MultiPolygon<f64> = shape
            .try_into()
            .map_err(|e| anyhow!("bad polygon for {k}: {e:?}"))?;
        let projected = MultiPolygon::new(
            mp.0.into_iter()
                .map(|p| {
                    let proj = |ls: &LineString<f64>| {
                        LineString::new(
                            ls.0.iter()
                                .map(|c| {
                                    let (x, y) = to_ma_state_plane(c.x, c.y);
                                    Coord { x, y }
                                })
                                .collect(),
                        )
                    };
                    Polygon::new(
                        proj(p.exterior()),
                        p.interiors().iter().map(proj).collect(),
                    )
                })
                .collect(),
        );
        out.push((k, projected));
    }
    Ok(out)
}

/// Simplify a set of features with Douglas–Peucker while keeping shared
/// borders identical: vertices where the set of features using a point
/// changes ("junctions") are fixed, and each run between junctions is
/// simplified on its own, so neighbors get the same simplified border.
fn simplify_coverage(features: &[(String, MultiPolygon<f64>)], tol: f64) -> Vec<Vec<Vec<Vec<(i64, i64)>>>> {
    let key = |c: &Coord<f64>| ((c.x * 100.0).round() as i64, (c.y * 100.0).round() as i64);
    let mut users: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (fi, (_, mp)) in features.iter().enumerate() {
        for p in &mp.0 {
            for ring in std::iter::once(p.exterior()).chain(p.interiors()) {
                for c in &ring.0 {
                    let u = users.entry(key(c)).or_default();
                    if u.last() != Some(&fi) && !u.contains(&fi) {
                        u.push(fi);
                    }
                }
            }
        }
    }
    let quant = |c: &Coord<f64>| ((c.x / 10.0).round() as i64, (c.y / 10.0).round() as i64);
    features
        .iter()
        .map(|(_, mp)| {
            let mut polys = Vec::new();
            for p in &mp.0 {
                let mut rings = Vec::new();
                for (ri, ring) in std::iter::once(p.exterior()).chain(p.interiors()).enumerate() {
                    let pts: Vec<Coord<f64>> = ring.0.clone();
                    if pts.len() < 4 {
                        continue;
                    }
                    let n = pts.len() - 1; // closed ring: last == first
                    let set = |i: usize| -> Vec<usize> {
                        let mut s = users.get(&key(&pts[i % n])).cloned().unwrap_or_default();
                        s.sort_unstable();
                        s
                    };
                    let fixed: Vec<usize> = (0..n)
                        .filter(|&i| {
                            let s = set(i);
                            s.len() >= 3 || s != set((i + n - 1) % n) || s != set(i + 1)
                        })
                        .collect();
                    let mut out: Vec<Coord<f64>> = Vec::new();
                    if fixed.is_empty() {
                        let ls = LineString::new(pts.clone()).simplify(tol);
                        out = ls.0;
                    } else {
                        for (k, &start) in fixed.iter().enumerate() {
                            let end = fixed[(k + 1) % fixed.len()];
                            let mut run = Vec::new();
                            let mut i = start;
                            loop {
                                run.push(pts[i % n]);
                                if i % n == end && run.len() > 1 {
                                    break;
                                }
                                i += 1;
                                if run.len() > n + 1 {
                                    break;
                                }
                            }
                            let simplified = LineString::new(run).simplify(tol).0;
                            let skip_last = simplified.len().saturating_sub(1);
                            out.extend(simplified.into_iter().take(skip_last));
                        }
                        if let Some(first) = out.first().copied() {
                            out.push(first);
                        }
                    }
                    let mut q: Vec<(i64, i64)> = Vec::new();
                    for c in &out {
                        let p = quant(c);
                        if q.last() != Some(&p) {
                            q.push(p);
                        }
                    }
                    // Drop rings that collapsed, and tiny islands/holes.
                    let area = ring_area(&q);
                    if q.len() < 4 || area.abs() < (tol * tol / 100.0) {
                        if ri == 0 {
                            break;
                        }
                        continue;
                    }
                    rings.push(q);
                }
                if !rings.is_empty() {
                    polys.push(rings);
                }
            }
            polys
        })
        .collect()
}

fn ring_area(q: &[(i64, i64)]) -> f64 {
    let mut a = 0i128;
    for w in q.windows(2) {
        a += i128::from(w[0].0) * i128::from(w[1].1) - i128::from(w[1].0) * i128::from(w[0].1);
    }
    a as f64 / 2.0 // in (10 m)² units
}

/// Encode polygons: polygons separated by `|`, rings by `;`, coordinates as
/// `x0 y0 dx dy dx dy ...` in 10-metre units of EPSG:26986.
fn encode_geometry(polys: &[Vec<Vec<(i64, i64)>>]) -> String {
    polys
        .iter()
        .map(|rings| {
            rings
                .iter()
                .map(|ring| {
                    let mut s = String::new();
                    let mut prev = (0i64, 0i64);
                    for (i, p) in ring.iter().take(ring.len() - 1).enumerate() {
                        if i == 0 {
                            let _ = write!(s, "{} {}", p.0, p.1);
                        } else {
                            let _ = write!(s, " {} {}", p.0 - prev.0, p.1 - prev.1);
                        }
                        prev = *p;
                    }
                    s
                })
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn build_boundaries(root: &Path, dl: &Downloader, geos: &Geos, today: &str) -> Result<Built> {
    let towns_zip = dl.get(&format!("{CB_BASE}/cb_2024_25_cousub_500k.zip"), "cb_2024_25_cousub_500k.zip")?;
    let tracts_zip = dl.get(&format!("{CB_BASE}/cb_2024_25_tract_500k.zip"), "cb_2024_25_tract_500k.zip")?;
    let zcta_zip = dl.get(ZCTA_CB, "cb_2020_us_zcta520_500k.zip")?;

    let mut towns = read_shapes(&towns_zip, &|r| {
        rec_text(r, "GEOID").filter(|g| geos.town_by_geoid.contains_key(g))
    })?;
    towns.sort_by(|a, b| a.0.cmp(&b.0));
    if towns.len() != 351 {
        bail!("expected 351 town shapes, found {}", towns.len());
    }
    let mut tracts = read_shapes(&tracts_zip, &|r| {
        rec_text(r, "GEOID").filter(|g| geos.tracts.contains(g))
    })?;
    tracts.sort_by(|a, b| a.0.cmp(&b.0));
    let mut zctas = read_shapes(&zcta_zip, &|r| {
        rec_text(r, "ZCTA5CE20")
            .or_else(|| rec_text(r, "GEOID20"))
            .filter(|g| geos.zctas.contains(g))
    })?;
    zctas.sort_by(|a, b| a.0.cmp(&b.0));

    // School districts: union of member towns (academic and vocational;
    // charter schools have no district geography).
    let districts_csv = std::fs::read_to_string(root.join("snapshots/ma_school_districts.csv"))?;
    let town_shape: HashMap<String, &MultiPolygon<f64>> = towns
        .iter()
        .map(|(g, mp)| (geos.town_by_geoid[g].clone(), mp))
        .collect();
    let norm_town: HashMap<String, String> = town_shape
        .keys()
        .map(|t| (norm(t), t.clone()))
        .collect();
    let mut districts = Vec::new();
    let mut district_names = HashMap::new();
    for rec in csv::Reader::from_reader(districts_csv.as_bytes()).records() {
        let rec = rec?;
        let (code, name, kind, members) = (&rec[0], &rec[1], &rec[2], &rec[3]);
        if kind.starts_with("Charter") {
            continue;
        }
        let parts: Vec<&MultiPolygon<f64>> = members
            .split(',')
            .filter_map(|m| norm_town.get(&norm(m.trim())))
            .filter_map(|t| town_shape.get(t).copied())
            .collect();
        if parts.is_empty() {
            eprintln!("district {name}: no member town shapes ({members})");
            continue;
        }
        let polys: Vec<Polygon<f64>> = parts.iter().flat_map(|mp| mp.0.iter().cloned()).collect();
        let union = geo::unary_union(&polys);
        district_names.insert(code.to_string(), name.to_string());
        districts.push((code.to_string(), union));
    }

    let layers: Vec<(&str, &str, f64, Vec<(String, MultiPolygon<f64>)>)> = vec![
        ("ma_boundaries_towns.csv", "town", 60.0, towns),
        ("ma_boundaries_tracts.csv", "tract", 25.0, tracts),
        ("ma_boundaries_zctas.csv", "zcta", 50.0, zctas),
        ("ma_boundaries_school_districts.csv", "district", 60.0, districts),
    ];
    let names: HashMap<&str, String> = geos
        .rows
        .iter()
        .map(|r| (r.geoid.as_str(), r.name.clone()))
        .collect();
    let mut contents = Vec::new();
    let mut files = Vec::new();
    for (path, level, tol, features) in layers {
        let simplified = simplify_coverage(&features, tol);
        let mut out = String::from("id,name,geometry\n");
        for ((id, _), polys) in features.iter().zip(simplified) {
            if polys.is_empty() {
                eprintln!("{level} {id}: geometry collapsed; skipped");
                continue;
            }
            let (out_id, name) = match level {
                "town" => (geos.town_by_geoid[id].clone(), geos.town_by_geoid[id].clone()),
                "district" => (id.clone(), district_names[id].clone()),
                _ => (id.clone(), names.get(id.as_str()).cloned().unwrap_or_default()),
            };
            let _ = writeln!(out, "{},{},{}", csv_field(&out_id), csv_field(&name), encode_geometry(&polys));
        }
        eprintln!("{path}: {} bytes", out.len());
        files.push(file_entry(path, &out));
        contents.push((path.to_string(), out));
    }
    Ok(Built {
        entry: SnapshotEntry {
            id: "ma-boundaries".into(),
            title: "Massachusetts map boundaries: cities and towns, 2020 census tracts, ZCTAs, and school districts".into(),
            publisher: "U.S. Census Bureau cartographic boundary files; school districts derived by this project".into(),
            source_urls: vec![
                format!("{CB_BASE}/cb_2024_25_cousub_500k.zip"),
                format!("{CB_BASE}/cb_2024_25_tract_500k.zip"),
                ZCTA_CB.into(),
            ],
            source_sha256: dl.take_hashes(),
            source_vintage: "Cartographic boundary files 2024 (towns, 2020 tracts) and 2020 (ZCTAs)".into(),
            retrieved: today.into(),
            license: COMMONWEALTH_LICENSE.into(),
            derivation: "Polygons from the 1:500,000 cartographic boundary files (clipped to the shoreline), projected to NAD83 / Massachusetts Mainland (EPSG:26986), simplified with Douglas–Peucker (tolerance 60 m for towns and school districts, 25 m for tracts, 50 m for ZCTAs) with shared borders simplified identically, quantized to 10 m, and delta-encoded (polygons separated by '|', rings by ';', 'x y dx dy ...' in 10 m units). School districts (municipal, regional academic, and regional vocational-technical; DESE organization codes from the MassGIS district list) are the union of their member towns.".into(),
            caveats: vec![
                "Boundaries are simplified for maps at town to state scale and are not survey-grade.".into(),
                "Regional secondary and vocational districts overlap the elementary or municipal districts of their member towns.".into(),
                "School district membership follows the 2019 MassGIS district list; newer regionalizations are missing.".into(),
            ],
            files,
        },
        contents,
    })
}

// ---------------------------------------------------------------------------
// ACS 5-year
// ---------------------------------------------------------------------------

fn fmt_num(id: &str, v: f64) -> String {
    let r = census::round_for_storage(id, v);
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r:.1}")
    }
}

fn build_acs(dl: &Downloader, geos: &Geos, today: &str) -> Result<Built> {
    let mut tables: BTreeSet<String> = census::required_cells()
        .iter()
        .map(|c| c.split('_').next().unwrap_or_default().to_string())
        .collect();
    tables.insert("B16001".into());
    let mut cells: BTreeMap<String, Cells> = BTreeMap::new();
    let mut urls = Vec::new();
    for t in &tables {
        let name = format!("acsdt5y{ACS_VINTAGE}-{}.dat", t.to_lowercase());
        let url = format!("{SF_BASE}/{name}");
        urls.push(url.clone());
        let bytes = dl.get(&url, &name)?;
        let text = String::from_utf8(bytes)?;
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().unwrap_or_default().split('|').collect();
        // Column → (cell id, is_moe)
        let cols: Vec<Option<(String, bool)>> = header
            .iter()
            .map(|h| {
                let (tbl, rest) = h.split_once('_')?;
                let (kind, line) = rest.split_at(1);
                Some((format!("{tbl}_{line}"), kind == "M"))
            })
            .collect();
        for line in lines {
            let Some((geo, _)) = line.split_once('|') else { continue };
            let geoid = if geo == "0100000US" {
                "US".to_string()
            } else if geo == "0400000US25" {
                "25".to_string()
            } else if let Some(c) = geo.strip_prefix("0500000US") {
                if !c.starts_with("25") { continue; }
                c.to_string()
            } else if let Some(c) = geo.strip_prefix("0600000US") {
                if !geos.town_by_geoid.contains_key(c) { continue; }
                c.to_string()
            } else if let Some(c) = geo.strip_prefix("1400000US") {
                if !geos.tracts.contains(c) { continue; }
                c.to_string()
            } else if let Some(c) = geo.strip_prefix("795P200US") {
                if t != "B16001" || !geos.pumas.contains(c) { continue; }
                c.to_string()
            } else {
                continue;
            };
            let entry = cells.entry(geoid).or_default();
            let mut est: HashMap<String, f64> = HashMap::new();
            let mut moe: HashMap<String, f64> = HashMap::new();
            for (i, v) in line.split('|').enumerate().skip(1) {
                let Some(Some((cell, is_moe))) = cols.get(i) else { continue };
                let Ok(x) = v.trim().parse::<f64>() else { continue };
                if *is_moe {
                    moe.insert(cell.clone(), x);
                } else {
                    est.insert(cell.clone(), x);
                }
            }
            for (cell, e) in est {
                if let Some(value) = census::decode_estimate(e) {
                    entry.insert(
                        cell.clone(),
                        Estimate {
                            value,
                            moe: moe.get(&cell).and_then(|m| census::decode_moe(*m)),
                        },
                    );
                }
            }
        }
        eprintln!("{t}: {} geographies so far", cells.len());
    }

    // Wide indicator file.
    let lang_cols = census::language_count_columns();
    let mut ids: Vec<String> = census::INDICATORS.iter().map(|d| d.id.to_string()).collect();
    ids.extend(lang_cols.iter().map(|(c, _)| c.clone()));
    let mut out = String::from("geoid,level,name");
    for id in &ids {
        let _ = write!(out, ",{id},{id}_moe");
    }
    out.push('\n');
    let mut order: Vec<(String, &str, String)> = vec![
        ("US".into(), "nation", "United States".into()),
        ("25".into(), "state", "Massachusetts".into()),
    ];
    for r in &geos.rows {
        if matches!(r.level, "county" | "town" | "tract") {
            order.push((r.geoid.clone(), r.level, r.name.clone()));
        }
    }
    let mut rows = 0;
    for (geoid, level, name) in &order {
        let Some(c) = cells.get(geoid) else {
            if *level != "tract" {
                bail!("no ACS data for {name} ({geoid})");
            }
            continue;
        };
        let values = census::compute_all(c);
        if values.get("pop_total").is_none_or(|p| p.value <= 0.0) {
            continue; // water-only tracts
        }
        let _ = write!(out, "{geoid},{level},{}", csv_field(name));
        for id in &ids {
            match values.get(id) {
                Some(e) => {
                    let moe = e.moe.map(|m| fmt_num(id, m)).unwrap_or_default();
                    let _ = write!(out, ",{},{moe}", fmt_num(id, e.value));
                }
                None => out.push_str(",,"),
            }
        }
        out.push('\n');
        rows += 1;
    }
    eprintln!("ACS indicators: {rows} rows, {} bytes", out.len());

    // B16001 detailed languages for Massachusetts and its PUMAs.
    let mut detail = String::from("geoid,language,speakers,speakers_moe,lep,lep_moe\n");
    let mut geo_ids: Vec<&String> = cells
        .keys()
        .filter(|g| *g == "25" || geos.pumas.contains(*g))
        .collect();
    geo_ids.sort();
    for g in geo_ids {
        let c = &cells[g];
        for (line, label) in census::DETAILED_LANGUAGES {
            let (Some(s), Some(l)) = (c.get(line), c.get(&census::detailed_lep_line(line))) else {
                continue;
            };
            if s.value <= 0.0 {
                continue;
            }
            let m = |e: &Estimate| e.moe.map(|m| format!("{m:.0}")).unwrap_or_default();
            let _ = writeln!(
                detail,
                "{g},{},{:.0},{},{:.0},{}",
                csv_field(label),
                s.value,
                m(s),
                l.value,
                m(l)
            );
        }
    }

    let files = vec![
        file_entry(census::SNAPSHOT_FILE, &out),
        file_entry(census::LANGUAGE_DETAIL_FILE, &detail),
    ];
    Ok(Built {
        entry: SnapshotEntry {
            id: census::SNAPSHOT_ID.into(),
            title: census::acs_title(ACS_VINTAGE),
            publisher: "U.S. Census Bureau".into(),
            source_urls: std::iter::once(format!("{SF_BASE}/"))
                .chain(urls.into_iter())
                .collect(),
            source_sha256: dl.take_hashes(),
            source_vintage: format!("{}–{ACS_VINTAGE} (released December {})", ACS_VINTAGE - 4, ACS_VINTAGE + 1),
            retrieved: today.into(),
            license: COMMONWEALTH_LICENSE.into(),
            derivation: "From the ACS 5-year table-based summary file: every cell an indicator needs, for the nation, Massachusetts, its 14 counties, 351 county subdivisions (cities and towns), and populated 2020 census tracts. Indicators are sums and percentages of detailed-table cells (definitions in phn_data::census::INDICATORS, with table IDs). Margins of error (90%) of sums use sqrt(sum of squared MOEs), counting only the largest MOE among zero estimates; percentages use the Census Bureau's derived-proportion formula (ratio formula when the radicand is negative). Counts and dollars are rounded to whole numbers, percentages to one decimal. ma_acs5_languages_detailed.csv holds B16001 detailed languages (speakers and those who speak English less than 'very well') for Massachusetts and its 2020 PUMAs, the smallest areas where B16001 is published.".into(),
            caveats: vec![
                "ACS 5-year estimates describe the five-year period, not a single year, and carry sampling error; check the margin of error and coefficient of variation before comparing areas.".into(),
                "Small towns and tracts have large margins of error; estimates with a coefficient of variation above 30% are flagged as unreliable.".into(),
                "Dollar amounts are in inflation-adjusted dollars of the last year of the period.".into(),
            ],
            files,
        },
        contents: vec![
            (census::SNAPSHOT_FILE.into(), out),
            (census::LANGUAGE_DETAIL_FILE.into(), detail),
        ],
    })
}

// ---------------------------------------------------------------------------
// SVI 2022
// ---------------------------------------------------------------------------

/// SVI columns kept (percent estimates of the 16 variables and flags).
const SVI_EP: [&str; 16] = [
    "EP_POV150", "EP_UNEMP", "EP_HBURD", "EP_NOHSDP", "EP_UNINSUR", "EP_AGE65", "EP_AGE17",
    "EP_DISABL", "EP_SNGPNT", "EP_LIMENG", "EP_MINRTY", "EP_MUNIT", "EP_MOBILE", "EP_CROWD",
    "EP_NOVEH", "EP_GROUPQ",
];
const SVI_RPL: [&str; 5] = ["RPL_THEME1", "RPL_THEME2", "RPL_THEME3", "RPL_THEME4", "RPL_THEMES"];

fn read_svi(bytes: &[u8]) -> Result<Vec<HashMap<String, String>>> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers = rdr.headers()?.clone();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        out.push(
            headers
                .iter()
                .zip(rec.iter())
                .map(|(h, v)| (h.to_string(), v.to_string()))
                .collect(),
        );
    }
    Ok(out)
}

fn svi_value(v: Option<&String>) -> String {
    match v.and_then(|s| s.parse::<f64>().ok()) {
        Some(x) if x > -999.0 => {
            let s = format!("{x:.4}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        _ => String::new(),
    }
}

fn build_svi(dl: &Downloader, today: &str) -> Result<Built> {
    let ma = read_svi(&dl.get(SVI_MA, "SVI_2022_Massachusetts.csv")?)?;
    let us = read_svi(&dl.get(SVI_US, "SVI_2022_US.csv")?)?;
    let us_by_fips: HashMap<String, &HashMap<String, String>> = us
        .iter()
        .filter(|r| r.get("ST").is_some_and(|s| s == "25"))
        .map(|r| (r["FIPS"].clone(), r))
        .collect();
    let mut out = String::from("fips,location,pop,pop_moe");
    for c in SVI_RPL {
        let _ = write!(out, ",ma_{}", c.to_lowercase());
    }
    for c in SVI_RPL {
        let _ = write!(out, ",us_{}", c.to_lowercase());
    }
    out.push_str(",f_total");
    for c in SVI_EP {
        let _ = write!(out, ",{}", c.to_lowercase());
    }
    out.push('\n');
    let mut rows: Vec<&HashMap<String, String>> = ma.iter().collect();
    rows.sort_by(|a, b| a["FIPS"].cmp(&b["FIPS"]));
    for r in rows {
        let fips = &r["FIPS"];
        let us_row = us_by_fips.get(fips);
        let _ = write!(
            out,
            "{fips},{},{},{}",
            csv_field(r.get("LOCATION").map(String::as_str).unwrap_or("")),
            svi_value(r.get("E_TOTPOP")),
            svi_value(r.get("M_TOTPOP"))
        );
        for c in SVI_RPL {
            let _ = write!(out, ",{}", svi_value(r.get(c)));
        }
        for c in SVI_RPL {
            let _ = write!(out, ",{}", svi_value(us_row.and_then(|u| u.get(c))));
        }
        let _ = write!(out, ",{}", svi_value(r.get("F_TOTAL")));
        for c in SVI_EP {
            let _ = write!(out, ",{}", svi_value(r.get(c)));
        }
        out.push('\n');
    }
    let path = "ma_svi_2022.csv";
    Ok(Built {
        entry: SnapshotEntry {
            id: "cdc-svi-2022-ma".into(),
            title: "CDC/ATSDR Social Vulnerability Index 2022, Massachusetts census tracts".into(),
            publisher: "Centers for Disease Control and Prevention / Agency for Toxic Substances and Disease Registry, Geospatial Research, Analysis, and Services Program".into(),
            source_urls: vec![
                "https://www.atsdr.cdc.gov/place-health/php/svi/svi-data-documentation-download.html".into(),
                SVI_MA.into(),
                SVI_US.into(),
            ],
            source_sha256: dl.take_hashes(),
            source_vintage: "SVI 2022 (ACS 2018–2022), released 2024-05-21; MOE correction 2024-12-11".into(),
            retrieved: today.into(),
            license: COMMONWEALTH_LICENSE.into(),
            derivation: "Massachusetts tracts from the state-ranked file (percentile ranks within Massachusetts, columns ma_rpl_*) joined to the same tracts in the U.S.-ranked file (national percentile ranks, us_rpl_*). Kept: total population and MOE, overall and four theme percentile ranks, the count of flags (F_TOTAL, tracts at or above the 90th percentile on a variable), and the 16 percent estimates (EP_*). -999 (no data) is stored as blank.".into(),
            caveats: vec![
                "SVI ranks are relative (percentiles); they do not measure absolute need and cannot be compared across SVI releases.".into(),
                "Rankings within Massachusetts and nationally differ; say which one you use.".into(),
                "Built from ACS 2018–2022 5-year estimates, which have sampling error at the tract level.".into(),
            ],
            files: vec![file_entry(path, &out)],
        },
        contents: vec![(path.into(), out)],
    })
}

// ---------------------------------------------------------------------------
// PLACES 2025
// ---------------------------------------------------------------------------

fn socrata_all(dl: &Downloader, dataset: &str, select: &str, where_: &str, tag: &str) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
    let mut out = Vec::new();
    let mut offset = 0u32;
    loop {
        let q = phn_data::socrata::SocrataQuery::cdc(dataset)
            .select(select)
            .and_where(where_)
            .order(":id")
            .limit(50_000)
            .offset(offset);
        let bytes = dl.get(&q.url(), &format!("places_{tag}_{offset}.json"))?;
        let rows = phn_data::socrata::parse_rows(&bytes)?;
        let n = rows.len();
        out.extend(rows);
        if n < 50_000 {
            break;
        }
        offset += 50_000;
    }
    Ok(out)
}

fn build_places(dl: &Downloader, geos: &Geos, today: &str) -> Result<Built> {
    use phn_data::socrata::{num, text};
    let select = "locationid,locationname,measureid,datavaluetypeid,data_value,low_confidence_limit,high_confidence_limit,totalpop18plus,year,measure,category,short_question_text";
    let sets = [
        ("county", PLACES_COUNTY, "stateabbr = 'MA'".to_string()),
        ("place", PLACES_PLACE, "stateabbr = 'MA'".to_string()),
        ("tract", PLACES_TRACT, "stateabbr = 'MA'".to_string()),
        ("zcta", PLACES_ZCTA, "locationid >= '01000' AND locationid < '02800'".to_string()),
    ];
    // measure id → (short name, measure, category, data year)
    let mut measures: BTreeMap<String, (String, String, String, String)> = BTreeMap::new();
    // (level, locationid, type) → (name, pop18, measure → (value, lo, hi))
    type Key = (String, String, String);
    let mut table: BTreeMap<Key, (String, Option<f64>, BTreeMap<String, (f64, Option<f64>, Option<f64>)>)> = BTreeMap::new();
    for (level, id, where_) in &sets {
        let rows = socrata_all(dl, id, select, where_, level)?;
        eprintln!("PLACES {level}: {} rows", rows.len());
        for r in rows {
            let (Some(loc), Some(m), Some(t)) = (text(&r, "locationid"), text(&r, "measureid"), text(&r, "datavaluetypeid")) else {
                continue;
            };
            if *level == "zcta" && !geos.zctas.contains(loc) {
                continue;
            }
            if *level == "tract" && !geos.tracts.contains(loc) {
                continue;
            }
            let Some(v) = num(&r, "data_value") else { continue };
            measures.entry(m.to_string()).or_insert_with(|| {
                (
                    text(&r, "short_question_text").unwrap_or_default().to_string(),
                    text(&r, "measure").unwrap_or_default().to_string(),
                    text(&r, "category").unwrap_or_default().to_string(),
                    text(&r, "year").unwrap_or_default().to_string(),
                )
            });
            let kind = if t == "AgeAdjPrv" { "ageadj" } else { "crude" };
            let name = match *level {
                "county" => format!("{} County", text(&r, "locationname").unwrap_or_default()),
                "place" => text(&r, "locationname").unwrap_or_default().to_string(),
                _ => loc.to_string(),
            };
            let e = table
                .entry((level.to_string(), loc.to_string(), kind.into()))
                .or_insert_with(|| (name, num(&r, "totalpop18plus"), BTreeMap::new()));
            e.2.insert(
                m.to_string(),
                (v, num(&r, "low_confidence_limit"), num(&r, "high_confidence_limit")),
            );
        }
    }
    // U.S. and Massachusetts reference values: adult-population-weighted
    // means of county estimates, computed by Socrata.
    let agg = "measureid,datavaluetypeid,sum(data_value*totalpop18plus)/sum(totalpop18plus) as w";
    let mut reference: BTreeMap<(String, String, String), f64> = BTreeMap::new();
    for (scope, where_) in [("us", "data_value IS NOT NULL"), ("ma", "stateabbr = 'MA' AND data_value IS NOT NULL")] {
        let q = phn_data::socrata::SocrataQuery::cdc(PLACES_COUNTY)
            .select(agg)
            .and_where(where_)
            .group("measureid,datavaluetypeid")
            .limit(1000);
        let rows = phn_data::socrata::parse_rows(&dl.get(&q.url(), &format!("places_ref_{scope}.json"))?)?;
        for r in rows {
            if let (Some(m), Some(t), Some(w)) = (text(&r, "measureid"), text(&r, "datavaluetypeid"), num(&r, "w")) {
                let kind = if t == "AgeAdjPrv" { "ageadj" } else { "crude" };
                reference.insert((scope.into(), m.into(), kind.into()), w);
            }
        }
    }

    let ids: Vec<String> = measures.keys().cloned().collect();
    let mut out = String::from("level,locationid,name,type,pop18");
    for m in &ids {
        let _ = write!(out, ",{m},{m}_lo,{m}_hi");
    }
    out.push('\n');
    let level_rank = |l: &str| match l {
        "county" => 0,
        "place" => 1,
        "zcta" => 2,
        _ => 3,
    };
    let mut keys: Vec<&Key> = table.keys().collect();
    keys.sort_by(|a, b| level_rank(&a.0).cmp(&level_rank(&b.0)).then(a.cmp(b)));
    let f1 = |x: Option<f64>| x.map(|v| format!("{v:.1}")).unwrap_or_default();
    for k in keys {
        let (name, pop, vals) = &table[k];
        let _ = write!(
            out,
            "{},{},{},{},{}",
            k.0,
            k.1,
            csv_field(name),
            k.2,
            pop.map(|p| format!("{p:.0}")).unwrap_or_default()
        );
        for m in &ids {
            match vals.get(m) {
                Some((v, lo, hi)) => {
                    let _ = write!(out, ",{v:.1},{},{}", f1(*lo), f1(*hi));
                }
                None => out.push_str(",,,"),
            }
        }
        out.push('\n');
    }
    let mut mfile = String::from("measureid,short_name,measure,category,data_year,us_crude,us_ageadj,ma_crude,ma_ageadj\n");
    for (m, (short, full, cat, year)) in &measures {
        let r = |s: &str, t: &str| {
            reference
                .get(&(s.to_string(), m.clone(), t.to_string()))
                .map(|v| format!("{v:.1}"))
                .unwrap_or_default()
        };
        let _ = writeln!(
            mfile,
            "{m},{},{},{},{year},{},{},{},{}",
            csv_field(short),
            csv_field(full),
            csv_field(cat),
            r("us", "crude"),
            r("us", "ageadj"),
            r("ma", "crude"),
            r("ma", "ageadj")
        );
    }
    eprintln!("PLACES: {} bytes, measures {} bytes", out.len(), mfile.len());
    let files = vec![
        file_entry("ma_places_2025.csv", &out),
        file_entry("ma_places_2025_measures.csv", &mfile),
    ];
    Ok(Built {
        entry: SnapshotEntry {
            id: "cdc-places-2025-ma".into(),
            title: "CDC PLACES: Local Data for Better Health, 2025 release (Massachusetts counties, places, census tracts, and ZCTAs)".into(),
            publisher: "Centers for Disease Control and Prevention, Division of Population Health".into(),
            source_urls: vec![
                format!("https://data.cdc.gov/d/{PLACES_COUNTY}"),
                format!("https://data.cdc.gov/d/{PLACES_PLACE}"),
                format!("https://data.cdc.gov/d/{PLACES_TRACT}"),
                format!("https://data.cdc.gov/d/{PLACES_ZCTA}"),
            ],
            source_sha256: dl.take_hashes(),
            source_vintage: "PLACES 2025 release (BRFSS 2023 for 35 measures, 2022 for 5; ACS 2019–2023 or 2018–2022)".into(),
            retrieved: today.into(),
            license: COMMONWEALTH_LICENSE.into(),
            derivation: "Rows for Massachusetts from the four PLACES 2025 datasets on data.cdc.gov (county swc5-untb, place eav7-hnsx, tract cwsq-ngmh, ZCTA qnzd-25i4), reshaped to one row per location and estimate type (crude, and for counties and places also age-adjusted) with each measure's estimate and 95% confidence limits. ZCTAs and tracts are limited to those in ma_census_geographies.csv. ma_places_2025_measures.csv lists each measure's name, category, and BRFSS data year, with U.S. and Massachusetts reference values computed by this project as adult-population-weighted means of county estimates (sum of estimate × population 18+ over sum of population 18+), which are not official CDC state estimates and have no confidence interval.".into(),
            caveats: vec![
                "PLACES estimates are model-based small-area estimates, not direct survey measurements; CDC cautions against using them to evaluate local programs or policies.".into(),
                "The data year (BRFSS survey year) differs from the release year.".into(),
                "U.S. and Massachusetts reference values are population-weighted means of county estimates computed by this server, not official CDC state estimates.".into(),
            ],
            files,
        },
        contents: vec![
            ("ma_places_2025.csv".into(), out),
            ("ma_places_2025_measures.csv".into(), mfile),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_to_massachusetts_state_plane() {
        // False origin: 41°N, 71.5°W → (200000, 750000).
        let (x, y) = to_ma_state_plane(-71.5, 41.0);
        assert!((x - 200_000.0).abs() < 0.01 && (y - 750_000.0).abs() < 0.01);
        // Boston State House (42.35808 N, 71.06369 W) is near (236,000, 900,800).
        let (x, y) = to_ma_state_plane(-71.06369, 42.35808);
        assert!((x - 236_000.0).abs() < 1_500.0, "{x}");
        assert!((y - 900_800.0).abs() < 1_500.0, "{y}");
    }

    #[test]
    fn strips_census_town_suffixes() {
        assert_eq!(census_town_base("Barnstable Town city"), "Barnstable");
        assert_eq!(census_town_base("Boston city"), "Boston");
        assert_eq!(census_town_base("Manchester-by-the-Sea town"), "Manchester-by-the-Sea");
    }

    #[test]
    fn encodes_delta_geometry() {
        let ring = vec![(10, 10), (12, 10), (12, 13), (10, 10)];
        assert_eq!(encode_geometry(&[vec![ring]]), "10 10 2 0 0 3");
    }
}
