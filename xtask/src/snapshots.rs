//! `cargo xtask build-snapshots`: rebuild the Massachusetts reference snapshots
//! from MassGIS and update `snapshots/manifest.json`.
//!
//! With `--check`, nothing is written: the freshly built files are compared
//! with the committed ones and a diff report is printed. The command exits
//! non-zero when the source data or the derived files changed, so a scheduled
//! CI job notices upstream changes.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use geo::{Area, Contains, InteriorPoint, MultiPolygon, Point, Polygon};
use serde::{Deserialize, Serialize};
use shapefile::dbase::{self, FieldValue, Record};

use crate::download::{fetch, sha256};

const MASSGIS_BASE: &str =
    "https://s3.us-east-1.amazonaws.com/download.massgis.digital.mass.gov/shapefiles/state";

const TOWNS_ZIP: &str = "townssurvey_shp.zip";
const REGIONS_ZIP: &str = "regdphep_poly.zip";
const DISTRICTS_ZIP: &str = "schooldistricts.zip";

/// DPH's "HMCC Regional Boundaries Map with communities labeled" (MDPH Office
/// of Preparedness and Emergency Management, dated 2019-12-09), linked from
/// <https://www.mass.gov/info-details/learn-about-the-health-and-medical-coordinating-coalitions>.
/// Retrieved 2026-11-07; PDF SHA-256
/// 2836a3264d837061ed8cffa837aa284c198bb673215b311ac6cae09c7db58dd7.
const HMCC_MAP_URL: &str =
    "https://www.mass.gov/doc/hmcc-regional-boundaries-map-with-communities-labeled/download";

/// Towns whose HMCC region on DPH's map ([`HMCC_MAP_URL`]) differs from the
/// region derived from the 2005 DPH Emergency Preparedness Region layer.
/// Keys are MassGIS `TOWN` names (upper case).
///
/// Found on 2026-11-07 by projecting every MassGIS town polygon onto the
/// rasterized map and reading the region fill color inside it (350 towns
/// classified that way, every one with a single color; Nahant, too small for
/// the sampler, checked by eye). Only Wilmington differed (2005 Region 4A, so
/// 4AB; DPH map Region 3). When DPH publishes a new map, repeat the comparison and
/// update this table. The build fails if an entry no longer changes anything.
const HMCC_OVERRIDES: &[(&str, &str)] = &[("WILMINGTON", "3")];

/// `snapshots/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format_version: u32,
    pub snapshots: Vec<SnapshotEntry>,
}

/// One snapshot in the manifest. Mirrors `phn_data::snapshots::SnapshotEntry`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotEntry {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub source_urls: Vec<String>,
    pub source_sha256: BTreeMap<String, String>,
    pub source_vintage: String,
    pub retrieved: String,
    pub license: String,
    pub derivation: String,
    pub caveats: Vec<String>,
    pub files: Vec<SnapshotFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub rows: u64,
}

struct Built {
    entry: SnapshotEntry,
    contents: Vec<(String, String)>,
}

pub fn run(root: &Path, check: bool, source_dir: Option<PathBuf>) -> Result<()> {
    let snap_dir = root.join("snapshots");
    let manifest_path = snap_dir.join("manifest.json");
    let old: Option<Manifest> = std::fs::read_to_string(&manifest_path)
        .ok()
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .context("parsing snapshots/manifest.json")?;
    let today = chrono::Local::now().date_naive().to_string();

    let towns_zip = fetch(
        &format!("{MASSGIS_BASE}/{TOWNS_ZIP}"),
        source_dir.as_deref(),
    )?;
    let regions_zip = fetch(
        &format!("{MASSGIS_BASE}/{REGIONS_ZIP}"),
        source_dir.as_deref(),
    )?;
    let districts_zip = fetch(
        &format!("{MASSGIS_BASE}/{DISTRICTS_ZIP}"),
        source_dir.as_deref(),
    )?;

    let built = vec![
        build_municipalities(&towns_zip, &regions_zip, &today)?,
        build_school_districts(&districts_zip, &today)?,
    ];

    let mut changed = false;
    let mut report = String::new();
    let mut new_manifest = Manifest {
        format_version: 1,
        snapshots: Vec::new(),
    };
    for b in built {
        let old_entry = old
            .as_ref()
            .and_then(|m| m.snapshots.iter().find(|e| e.id == b.entry.id));
        let mut entry = b.entry;
        let mut entry_changed = false;
        for (path, text) in &b.contents {
            let committed = std::fs::read_to_string(snap_dir.join(path)).unwrap_or_default();
            if committed != *text {
                entry_changed = true;
                report.push_str(&diff_report(path, &committed, text));
            }
        }
        match old_entry {
            Some(o) if o.source_sha256 != entry.source_sha256 => {
                entry_changed = true;
                report.push_str(&format!(
                    "{}: upstream source files changed (sha256 differs)\n",
                    entry.id
                ));
            }
            None => entry_changed = true,
            _ => {}
        }
        if !entry_changed && let Some(o) = old_entry {
            // Keep the original retrieval date when nothing changed.
            entry.retrieved = o.retrieved.clone();
        }
        changed |= entry_changed;
        if !check {
            for (path, text) in &b.contents {
                std::fs::write(snap_dir.join(path), text)?;
            }
        }
        new_manifest.snapshots.push(entry);
    }

    // Keep entries this command does not build (for example the surveillance
    // snapshots from `cargo xtask build-surveillance-snapshots`).
    if let Some(old) = &old {
        for e in &old.snapshots {
            if !new_manifest.snapshots.iter().any(|n| n.id == e.id) {
                new_manifest.snapshots.push(e.clone());
            }
        }
    }

    if check {
        if changed {
            println!("Snapshot differences found:\n{report}");
            bail!(
                "snapshots are out of date; run `cargo xtask build-snapshots` and review the diff"
            );
        }
        println!("Snapshots are up to date.");
        return Ok(());
    }
    let json = serde_json::to_string_pretty(&new_manifest)? + "\n";
    std::fs::write(&manifest_path, json)?;
    if changed {
        println!("Snapshots updated:\n{report}");
    } else {
        println!("Snapshots unchanged.");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Municipalities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Town {
    id: u32,
    name: String,
    kind: &'static str,
    county: String,
    county_fips: String,
    largest: Polygon<f64>,
    largest_area: f64,
}

fn build_municipalities(towns_zip: &[u8], regions_zip: &[u8], today: &str) -> Result<Built> {
    let tmp = tempfile::tempdir()?;
    let towns_vintage = extract(towns_zip, "TOWNSSURVEY_POLY", tmp.path())?;
    let regions_vintage = extract(regions_zip, "REGDPHEP_POLY", tmp.path())?;

    // Towns: several polygon records per town (islands). Keep the largest
    // part of each town for the point-in-region test.
    let mut reader = shapefile::Reader::from_path(tmp.path().join("TOWNSSURVEY_POLY.shp"))?;
    let mut towns: BTreeMap<String, Town> = BTreeMap::new();
    for item in reader.iter_shapes_and_records_as::<shapefile::Polygon, Record>() {
        let (shape, rec) = item?;
        let name = text(&rec, "TOWN")?;
        let mp: MultiPolygon<f64> = shape
            .try_into()
            .map_err(|e| anyhow!("bad polygon for {name}: {e:?}"))?;
        for poly in mp.0 {
            let area = poly.unsigned_area();
            let entry = towns.entry(name.clone()).or_insert_with(|| Town {
                id: 0,
                name: name.clone(),
                kind: "",
                county: String::new(),
                county_fips: String::new(),
                largest: poly.clone(),
                largest_area: -1.0,
            });
            if area > entry.largest_area {
                entry.largest = poly;
                entry.largest_area = area;
            }
            entry.id = number(&rec, "TOWN_ID")? as u32;
            entry.kind = match text(&rec, "TYPE")?.as_str() {
                "C" => "city",
                "T" => "town",
                "TC" => "town_with_city_government",
                other => bail!("unexpected TYPE {other} for {name}"),
            };
            entry.county = phn_core::text::title_case(&text(&rec, "COUNTY")?);
            entry.county_fips = format!("{:05}", number(&rec, "FIPS_STCO")? as u32);
        }
    }
    if towns.len() != 351 {
        bail!("expected 351 municipalities, found {}", towns.len());
    }

    let mut regions_reader = shapefile::Reader::from_path(tmp.path().join("REGDPHEP_POLY.shp"))?;
    let mut regions: Vec<(String, MultiPolygon<f64>)> = Vec::new();
    for item in regions_reader.iter_shapes_and_records_as::<shapefile::Polygon, Record>() {
        let (shape, rec) = item?;
        let region = text(&rec, "REGION")?
            .trim_start_matches("REGION")
            .trim()
            .to_string();
        let mp: MultiPolygon<f64> = shape
            .try_into()
            .map_err(|e| anyhow!("bad region polygon: {e:?}"))?;
        regions.push((region, mp));
    }

    let mut out = String::from("town_id,name,type,county,county_fips,dph_ep_region,hmcc_region\n");
    let mut overridden: BTreeSet<&str> = BTreeSet::new();
    let mut sorted: Vec<&Town> = towns.values().collect();
    sorted.sort_by_key(|t| t.id);
    let mut rows = 0;
    for t in sorted {
        let pt: Point<f64> = t
            .largest
            .interior_point()
            .ok_or_else(|| anyhow!("no interior point for {}", t.name))?;
        let hits: Vec<&String> = regions
            .iter()
            .filter(|(_, mp)| mp.contains(&pt))
            .map(|(r, _)| r)
            .collect();
        let region = match hits.as_slice() {
            [one] => (*one).clone(),
            other => bail!("{} falls in {} DPH regions", t.name, other.len()),
        };
        let derived = match region.as_str() {
            "4A" | "4B" => "4AB",
            r => r,
        };
        let hmcc = match HMCC_OVERRIDES.iter().find(|(town, _)| *town == t.name) {
            Some(&(town, r)) => {
                if r == derived {
                    bail!("HMCC override for {town} matches the derived region {r}; remove it");
                }
                overridden.insert(town);
                r.to_string()
            }
            None => derived.to_string(),
        };
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            t.id,
            csv_field(&phn_core::text::title_case(&t.name)),
            t.kind,
            t.county,
            t.county_fips,
            region,
            hmcc
        ));
        rows += 1;
    }
    if let Some((town, _)) = HMCC_OVERRIDES
        .iter()
        .find(|(town, _)| !overridden.contains(town))
    {
        bail!("HMCC override names {town}, which is not a MassGIS town");
    }
    let override_text = HMCC_OVERRIDES
        .iter()
        .map(|(town, r)| format!("{} (Region {r})", phn_core::text::title_case(town)))
        .collect::<Vec<_>>()
        .join(", ");

    let file = SnapshotFile {
        path: "ma_municipalities.csv".into(),
        sha256: sha256(out.as_bytes()),
        bytes: out.len() as u64,
        rows,
    };
    let entry = SnapshotEntry {
        id: "ma-municipalities".into(),
        title: "Massachusetts cities and towns with county and DPH preparedness region".into(),
        publisher:
            "MassGIS (Bureau of Geographic Information); region assignment derived by this project"
                .into(),
        source_urls: vec![
            format!("{MASSGIS_BASE}/{TOWNS_ZIP}"),
            format!("{MASSGIS_BASE}/{REGIONS_ZIP}"),
            HMCC_MAP_URL.to_string(),
        ],
        source_sha256: BTreeMap::from([
            (TOWNS_ZIP.to_string(), sha256(towns_zip)),
            (REGIONS_ZIP.to_string(), sha256(regions_zip)),
        ]),
        source_vintage: format!("towns {towns_vintage}; DPH regions {regions_vintage}"),
        retrieved: today.into(),
        license: "MassGIS data are free public records of the Commonwealth of Massachusetts."
            .into(),
        derivation: format!(
            "Town, type, county, and county FIPS come from the MassGIS municipal boundary \
             layer (TOWNSSURVEY_POLY). Each town's DPH Emergency Preparedness Region is the region \
             polygon (REGDPHEP_POLY) that contains an interior point of the town's largest land \
             part. The HMCC region merges DPH Regions 4A and 4B into 4AB, then applies the towns \
             whose region differs on DPH's HMCC Regional Boundaries map (MDPH OPEM, 2019-12-09; \
             PDF retrieved 2026-11-07, not re-downloaded by the build): {override_text}."
        ),
        caveats: vec![format!(
            "The DPH Emergency Preparedness Region layer dates from 2005. HMCC membership is \
             derived from it (Regions 4A and 4B merged into 4AB) and was compared town by town \
             with DPH's HMCC Regional Boundaries map (December 2019) on 2026-11-07; the towns that \
             differ are set to the map's region: {override_text}. DPH's interactive HMCC locator \
             could not be checked. Confirm with your HMCC."
        )],
        files: vec![file],
    };
    Ok(Built {
        entry,
        contents: vec![("ma_municipalities.csv".into(), out)],
    })
}

// ---------------------------------------------------------------------------
// School districts
// ---------------------------------------------------------------------------

fn build_school_districts(zip_bytes: &[u8], today: &str) -> Result<Built> {
    let tmp = tempfile::tempdir()?;
    let vintage = extract(zip_bytes, "SCHOOLDISTRICTS_POLY", tmp.path())?;
    extract(zip_bytes, "SCHOOLDISTRICTS_CCUV_POLY", tmp.path())?;

    // org_code -> (name, type label, members, grades)
    let mut rows: BTreeMap<String, [String; 4]> = BTreeMap::new();
    let mut skipped = BTreeSet::new();
    for rec in dbase::read(tmp.path().join("SCHOOLDISTRICTS_POLY.dbf"))? {
        let code = opt_text(&rec, "ORG8CODE");
        let name = opt_text(&rec, "DISTRICT_N");
        let (Some(code), Some(name)) = (code, name) else {
            continue;
        };
        let raw_type = opt_text(&rec, "DISTRICT_T").unwrap_or_default();
        let label = match raw_type.as_str() {
            "MuniK12" => "Municipal (K-12)",
            "MuniElem" => "Municipal (elementary)",
            "ReglK12" => "Regional academic (K-12)",
            "ReglHS" => "Regional academic (secondary)",
            "ReglElem" => "Regional academic (elementary)",
            "TuitionK12" => "Municipal (tuitions students to other districts)",
            other => {
                skipped.insert(format!("{name} ({other})"));
                continue;
            }
        };
        let grades = match (opt_text(&rec, "START_GRAD"), opt_text(&rec, "END_GRADE")) {
            (Some(a), Some(b)) => format!("{a}-{b}"),
            _ => String::new(),
        };
        let members = opt_text(&rec, "MEMBERLIST").unwrap_or_else(|| name.clone());
        rows.entry(code)
            .or_insert([name, label.into(), members, grades]);
    }
    for rec in dbase::read(tmp.path().join("SCHOOLDISTRICTS_CCUV_POLY.dbf"))? {
        let kind = opt_text(&rec, "TYPE").unwrap_or_default();
        let label = match kind.as_str() {
            "Charter" => "Charter school district",
            "Vocational" => "Regional vocational-technical",
            _ => continue,
        };
        let (Some(code), Some(name)) = (opt_text(&rec, "ORG8CODE"), opt_text(&rec, "NAME")) else {
            continue;
        };
        let members = opt_text(&rec, "MEMBERS").unwrap_or_default();
        rows.entry(code)
            .or_insert([name, label.into(), members, String::new()]);
    }
    if !skipped.is_empty() {
        eprintln!("school districts skipped (unrecognized type): {skipped:?}");
    }

    let mut out = String::from("org_code,name,type,member_towns,grades\n");
    for (code, [name, label, members, grades]) in &rows {
        out.push_str(&format!(
            "{code},{},{},{},{}\n",
            csv_field(name),
            csv_field(label),
            csv_field(members),
            csv_field(grades)
        ));
    }
    let file = SnapshotFile {
        path: "ma_school_districts.csv".into(),
        sha256: sha256(out.as_bytes()),
        bytes: out.len() as u64,
        rows: rows.len() as u64,
    };
    let entry = SnapshotEntry {
        id: "ma-school-districts".into(),
        title: "Massachusetts public school districts (academic, charter, and vocational)".into(),
        publisher: "MassGIS, from Massachusetts DESE district data".into(),
        source_urls: vec![format!("{MASSGIS_BASE}/{DISTRICTS_ZIP}")],
        source_sha256: BTreeMap::from([(DISTRICTS_ZIP.to_string(), sha256(zip_bytes))]),
        source_vintage: vintage,
        retrieved: today.into(),
        license: "MassGIS data are free public records of the Commonwealth of Massachusetts."
            .into(),
        derivation: "Districts from SCHOOLDISTRICTS_POLY (municipal and regional academic) plus \
            charter and vocational-technical districts from SCHOOLDISTRICTS_CCUV_POLY, keyed by \
            DESE 8-digit organization code."
            .into(),
        caveats: vec![
            "District boundaries and membership reflect the MassGIS layer's vintage; districts \
             formed, merged, or renamed since then are missing. Confirm with DESE School and \
             District Profiles."
                .into(),
        ],
        files: vec![file],
    };
    Ok(Built {
        entry,
        contents: vec![("ma_school_districts.csv".into(), out)],
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract every member whose file stem is `stem` into `dir`. Returns the
/// modification date of the `.dbf` member as the layer's vintage.
fn extract(zip_bytes: &[u8], stem: &str, dir: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut vintage = None;
    let mut found = 0;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let name = f.name()?.rsplit('/').next().unwrap_or("").to_string();
        let Some((file_stem, ext)) = name.split_once('.') else {
            continue;
        };
        if file_stem != stem {
            continue;
        }
        if ext.eq_ignore_ascii_case("dbf") {
            vintage = f
                .last_modified()
                .map(|d| format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()));
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        std::fs::write(dir.join(&name), buf)?;
        found += 1;
    }
    if found == 0 {
        bail!("{stem} not found in archive");
    }
    Ok(vintage.unwrap_or_else(|| "unknown".into()))
}

fn opt_text(rec: &Record, field: &str) -> Option<String> {
    match rec.get(field) {
        Some(FieldValue::Character(Some(s))) => {
            let s = s.trim().to_string();
            (!s.is_empty()).then_some(s)
        }
        _ => None,
    }
}

fn text(rec: &Record, field: &str) -> Result<String> {
    opt_text(rec, field).ok_or_else(|| anyhow!("missing text field {field}"))
}

fn number(rec: &Record, field: &str) -> Result<f64> {
    match rec.get(field) {
        Some(FieldValue::Numeric(Some(n))) => Ok(*n),
        Some(FieldValue::Float(Some(n))) => Ok(f64::from(*n)),
        Some(FieldValue::Integer(n)) => Ok(f64::from(*n)),
        Some(FieldValue::Double(n)) => Ok(*n),
        other => Err(anyhow!("missing numeric field {field}: {other:?}")),
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Line-level diff report (added/removed lines) for a CSV snapshot.
fn diff_report(path: &str, old: &str, new: &str) -> String {
    let old_lines: BTreeSet<&str> = old.lines().collect();
    let new_lines: BTreeSet<&str> = new.lines().collect();
    let removed: Vec<_> = old_lines.difference(&new_lines).collect();
    let added: Vec<_> = new_lines.difference(&old_lines).collect();
    let mut out = format!(
        "{path}: {} line(s) removed, {} line(s) added\n",
        removed.len(),
        added.len()
    );
    for l in removed.iter().take(50) {
        out.push_str(&format!("  - {l}\n"));
    }
    for l in added.iter().take(50) {
        out.push_str(&format!("  + {l}\n"));
    }
    out
}
