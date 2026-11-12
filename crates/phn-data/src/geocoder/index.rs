//! The geocoder index: MassGIS address points and TIGER address ranges, by town.
//!
//! Text format, version 1 (written by `xtask/scripts/build_geocoder_index.py`):
//!
//! ```text
//! #PHNGEO 1
//! T<TAB>town id<TAB>town name<TAB>county FIPS (3 digits)
//! B<TAB>block group GEOIDs (12 digits), space separated
//! Z<TAB>ZIP codes, space separated
//! S<TAB>street key<TAB>address points<TAB>address ranges
//! ```
//!
//! Points are `dnum[.sfx],bg,zip,dx,dy` (house number and EPSG:26986 metres as
//! differences from the previous point of the street) and ranges are
//! `from,to,<e|o|b><L|R>,bg,zip,x0,y0,dx,dy`. `bg` and `zip` index the `B` and `Z`
//! lists (`-` when unknown). A file holds one or more `#PHNGEO` sections.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use phn_core::{PhnError, Result};
use serde::{Deserialize, Serialize};

use super::normalize::{ParsedAddress, relaxed_keys};
use crate::snapshots::{SnapshotOrigin, SnapshotStore, sha256_hex};

/// Snapshot id and file of the bundled sample index.
pub const SAMPLE_SNAPSHOT_ID: &str = "ma-geocoder-sample";
pub const SAMPLE_FILE: &str = "ma_geocoder_sample.idx";
/// The only index format version this code reads.
pub const FORMAT_VERSION: u32 = 1;
/// Distance of an interpolated point from the street centre line (metres); must match the builder.
const OFFSET_M: f64 = 15.0;

/// How precisely an address was placed.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MatchLevel {
    /// The house number was found among the town's address points (MassGIS).
    ExactPoint,
    /// Placed between neighboring address points or along a TIGER address range.
    Interpolated,
    /// The street was found but not the house number.
    StreetOnly,
    /// Only a ZIP code could be used (it points to one town).
    ZipOnly,
    /// Only the town (from a town column or the address) could be used.
    TownOnly,
    /// Nothing could be used.
    Unmatched,
}

impl MatchLevel {
    /// Plain-language label.
    pub fn label(self) -> &'static str {
        match self {
            Self::ExactPoint => "Exact address point",
            Self::Interpolated => "Interpolated along the street",
            Self::StreetOnly => "Street only (house number not found)",
            Self::ZipOnly => "ZIP code only",
            Self::TownOnly => "Town only",
            Self::Unmatched => "Not located",
        }
    }

    /// All levels, best first.
    pub const ALL: [MatchLevel; 6] = [
        Self::ExactPoint,
        Self::Interpolated,
        Self::StreetOnly,
        Self::ZipOnly,
        Self::TownOnly,
        Self::Unmatched,
    ];
}

/// A located address (never contains the address text).
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    pub level: MatchLevel,
    /// 12-digit block group GEOID, when the street position fixes one.
    pub block_group: Option<String>,
    /// ZIP code of the matched point or range.
    pub zip: Option<String>,
    /// EPSG:26986 metres (x east, y north) for exact and interpolated matches.
    pub xy: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
struct Point {
    num: u32,
    sfx: String,
    bg: Option<u16>,
    zip: Option<u16>,
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parity {
    Even,
    Odd,
    Both,
}

#[derive(Debug, Clone)]
struct Range {
    from: u32,
    to: u32,
    parity: Parity,
    left: bool,
    bg: Option<u16>,
    zip: Option<u16>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

#[derive(Debug, Clone)]
struct Street {
    key: String,
    points: Vec<Point>,
    ranges: Vec<Range>,
}

/// One town's streets.
#[derive(Debug, Clone)]
pub struct TownIndex {
    pub id: u16,
    pub name: String,
    pub county: String,
    block_groups: Vec<String>,
    zips: Vec<String>,
    streets: Vec<Street>,
    by_key: HashMap<String, usize>,
    by_no_type: HashMap<String, Vec<usize>>,
    by_base: HashMap<String, Vec<usize>>,
}

fn bad(line: usize, what: &str) -> PhnError {
    PhnError::Internal(format!("geocoder index line {line}: {what}"))
}

fn idx(s: &str, line: usize) -> Result<Option<u16>> {
    if s == "-" {
        return Ok(None);
    }
    s.parse().map(Some).map_err(|_| bad(line, "bad list index"))
}

fn int(s: &str, line: usize) -> Result<i32> {
    s.parse().map_err(|_| bad(line, "bad integer"))
}

impl TownIndex {
    /// Parse every `#PHNGEO` section of a text.
    pub fn parse_sections(text: &str) -> Result<Vec<TownIndex>> {
        let mut towns = Vec::new();
        let mut current: Option<TownIndex> = None;
        for (n, line) in text.lines().enumerate() {
            let ln = n + 1;
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("#PHNGEO") {
                if rest.trim() != "1" {
                    return Err(bad(ln, "unsupported index version"));
                }
                if let Some(t) = current.take() {
                    towns.push(t.finish());
                }
                current = Some(TownIndex {
                    id: 0,
                    name: String::new(),
                    county: String::new(),
                    block_groups: Vec::new(),
                    zips: Vec::new(),
                    streets: Vec::new(),
                    by_key: HashMap::new(),
                    by_no_type: HashMap::new(),
                    by_base: HashMap::new(),
                });
                continue;
            }
            if line.starts_with('#') {
                continue;
            }
            let t = current
                .as_mut()
                .ok_or_else(|| bad(ln, "data before a #PHNGEO header"))?;
            let mut f = line.split('\t');
            match f.next() {
                Some("T") => {
                    t.id = f
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or_else(|| bad(ln, "town id"))?;
                    t.name = f.next().unwrap_or("").to_string();
                    t.county = f.next().unwrap_or("").to_string();
                }
                Some("B") => {
                    t.block_groups = f
                        .next()
                        .unwrap_or("")
                        .split(' ')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                }
                Some("Z") => {
                    t.zips = f
                        .next()
                        .unwrap_or("")
                        .split(' ')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                }
                Some("S") => {
                    let key = f.next().ok_or_else(|| bad(ln, "street key"))?.to_string();
                    let points = parse_points(f.next().unwrap_or(""), ln)?;
                    let ranges = parse_ranges(f.next().unwrap_or(""), ln)?;
                    t.streets.push(Street {
                        key,
                        points,
                        ranges,
                    });
                }
                _ => return Err(bad(ln, "unknown record")),
            }
        }
        if let Some(t) = current.take() {
            towns.push(t.finish());
        }
        Ok(towns)
    }

    fn finish(mut self) -> Self {
        for (i, s) in self.streets.iter().enumerate() {
            self.by_key.insert(s.key.clone(), i);
            let (no_type, base) = relaxed_keys(&s.key);
            self.by_no_type.entry(no_type).or_default().push(i);
            self.by_base.entry(base).or_default().push(i);
        }
        self
    }

    /// Number of streets.
    pub fn street_count(&self) -> usize {
        self.streets.len()
    }

    /// Number of address points and address ranges.
    pub fn counts(&self) -> (usize, usize) {
        (
            self.streets.iter().map(|s| s.points.len()).sum(),
            self.streets.iter().map(|s| s.ranges.len()).sum(),
        )
    }

    fn street_for(&self, key: &str) -> Option<&Street> {
        if let Some(&i) = self.by_key.get(key) {
            return Some(&self.streets[i]);
        }
        let (no_type, base) = relaxed_keys(key);
        for (map, k) in [(&self.by_no_type, &no_type), (&self.by_base, &base)] {
            if let Some(v) = map.get(k.as_str())
                && let [only] = v.as_slice()
            {
                return Some(&self.streets[*only]);
            }
        }
        None
    }

    fn bg_geoid(&self, i: Option<u16>) -> Option<String> {
        i.and_then(|i| self.block_groups.get(i as usize)).cloned()
    }

    fn zip_text(&self, i: Option<u16>) -> Option<String> {
        i.and_then(|i| self.zips.get(i as usize)).cloned()
    }

    /// Place an address in this town. `None` when the street is not in the index.
    pub fn locate(&self, a: &ParsedAddress) -> Option<Located> {
        if a.street_key.is_empty() {
            return None;
        }
        let street = self.street_for(&a.street_key)?;
        let Some(n) = a.number else {
            return Some(self.street_only(street));
        };
        // 1. The house number is an address point.
        let exact = street
            .points
            .iter()
            .find(|p| p.num == n && p.sfx == a.suffix)
            .or_else(|| {
                street
                    .points
                    .iter()
                    .find(|p| p.num == n && p.sfx.is_empty())
            })
            .or_else(|| street.points.iter().find(|p| p.num == n));
        if let Some(p) = exact {
            return Some(Located {
                level: MatchLevel::ExactPoint,
                block_group: self.bg_geoid(p.bg),
                zip: self.zip_text(p.zip),
                xy: Some((f64::from(p.x), f64::from(p.y))),
            });
        }
        // 2. Between the nearest address points of the same parity that share a block group.
        let same_parity = |p: &&Point| p.num % 2 == n % 2;
        let lower = street
            .points
            .iter()
            .filter(same_parity)
            .filter(|p| p.num < n)
            .max_by_key(|p| p.num);
        let upper = street
            .points
            .iter()
            .filter(same_parity)
            .filter(|p| p.num > n)
            .min_by_key(|p| p.num);
        if let (Some(lo), Some(hi)) = (lower, upper)
            && lo.bg.is_some()
            && lo.bg == hi.bg
        {
            let t = f64::from(n - lo.num) / f64::from(hi.num - lo.num);
            return Some(Located {
                level: MatchLevel::Interpolated,
                block_group: self.bg_geoid(lo.bg),
                zip: self.zip_text(lo.zip),
                xy: Some((
                    f64::from(lo.x) + t * f64::from(hi.x - lo.x),
                    f64::from(lo.y) + t * f64::from(hi.y - lo.y),
                )),
            });
        }
        // 3. A TIGER address range that holds the number (the narrowest one).
        let range = street
            .ranges
            .iter()
            .filter(|r| {
                let (lo, hi) = (r.from.min(r.to), r.from.max(r.to));
                n >= lo
                    && n <= hi
                    && match r.parity {
                        Parity::Both => true,
                        Parity::Even => n % 2 == 0,
                        Parity::Odd => n % 2 == 1,
                    }
            })
            .min_by_key(|r| r.from.abs_diff(r.to));
        if let Some(r) = range {
            let t = if r.from == r.to {
                0.5
            } else {
                (f64::from(n) - f64::from(r.from)) / (f64::from(r.to) - f64::from(r.from))
            };
            let (dx, dy) = (f64::from(r.x1 - r.x0), f64::from(r.y1 - r.y0));
            let len = dx.hypot(dy).max(1.0);
            let sign = if r.left { 1.0 } else { -1.0 };
            return Some(Located {
                level: MatchLevel::Interpolated,
                block_group: self.bg_geoid(r.bg),
                zip: self.zip_text(r.zip),
                xy: Some((
                    f64::from(r.x0) + t * dx + sign * (-dy / len) * OFFSET_M,
                    f64::from(r.y0) + t * dy + sign * (dx / len) * OFFSET_M,
                )),
            });
        }
        Some(self.street_only(street))
    }

    /// The street is known but not the position on it: a block group only when every point
    /// and range of the street lies in one.
    fn street_only(&self, street: &Street) -> Located {
        let mut bgs = street
            .points
            .iter()
            .map(|p| p.bg)
            .chain(street.ranges.iter().map(|r| r.bg));
        let first = bgs.next().flatten();
        let one = first.is_some() && bgs.all(|b| b == first);
        Located {
            level: MatchLevel::StreetOnly,
            block_group: if one { self.bg_geoid(first) } else { None },
            zip: None,
            xy: None,
        }
    }
}

fn parse_points(field: &str, ln: usize) -> Result<Vec<Point>> {
    let mut out = Vec::new();
    let (mut num, mut x, mut y) = (0u32, 0i32, 0i32);
    for item in field.split(' ').filter(|s| !s.is_empty()) {
        let mut f = item.split(',');
        let head = f.next().ok_or_else(|| bad(ln, "point"))?;
        let (d, sfx) = head.split_once('.').unwrap_or((head, ""));
        num = num
            .checked_add(d.parse::<u32>().map_err(|_| bad(ln, "house number"))?)
            .ok_or_else(|| bad(ln, "house number"))?;
        let bg = idx(f.next().ok_or_else(|| bad(ln, "point"))?, ln)?;
        let zip = idx(f.next().ok_or_else(|| bad(ln, "point"))?, ln)?;
        x += int(f.next().ok_or_else(|| bad(ln, "point"))?, ln)?;
        y += int(f.next().ok_or_else(|| bad(ln, "point"))?, ln)?;
        out.push(Point {
            num,
            sfx: sfx.to_string(),
            bg,
            zip,
            x,
            y,
        });
    }
    Ok(out)
}

fn parse_ranges(field: &str, ln: usize) -> Result<Vec<Range>> {
    let mut out = Vec::new();
    for item in field.split(' ').filter(|s| !s.is_empty()) {
        let f: Vec<&str> = item.split(',').collect();
        if f.len() != 9 || f[2].len() != 2 {
            return Err(bad(ln, "range"));
        }
        let parity = match &f[2][..1] {
            "e" => Parity::Even,
            "o" => Parity::Odd,
            "b" => Parity::Both,
            _ => return Err(bad(ln, "range parity")),
        };
        let x0 = int(f[5], ln)?;
        let y0 = int(f[6], ln)?;
        out.push(Range {
            from: f[0].parse().map_err(|_| bad(ln, "range number"))?,
            to: f[1].parse().map_err(|_| bad(ln, "range number"))?,
            parity,
            left: &f[2][1..] == "L",
            bg: idx(f[3], ln)?,
            zip: idx(f[4], ln)?,
            x0,
            y0,
            x1: x0 + int(f[7], ln)?,
            y1: y0 + int(f[8], ln)?,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Installed and bundled indexes
// ---------------------------------------------------------------------------

/// `manifest.json` of an installed index.
#[derive(Debug, Clone, Deserialize)]
struct InstalledManifest {
    format: String,
    format_version: u32,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    built: String,
    towns: HashMap<String, InstalledTown>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstalledTown {
    file: String,
    sha256: String,
}

/// Which index served the lookups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IndexInfo {
    /// "installed" (full index in /data/geocoder or /cache/geocoder), "sample" (the bundled
    /// four-town sample), or "none".
    pub kind: String,
    /// Where an installed index was found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Date the installed index was built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built: Option<String>,
    /// Number of towns the index covers.
    pub towns_covered: usize,
    /// Problems found with an installed index (bad checksum, unreadable file).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

struct Installed {
    dir: PathBuf,
    manifest: InstalledManifest,
}

/// The street indexes available to one tool call: an installed one if present, and the
/// bundled sample. Town files are read (and checked against their checksum) on first use.
pub struct IndexSet {
    installed: Option<Installed>,
    sample_text: Option<std::borrow::Cow<'static, str>>,
    sample_origin: Option<SnapshotOrigin>,
    sample: OnceLock<HashMap<u16, Arc<TownIndex>>>,
    cache: Mutex<HashMap<u16, Option<Arc<TownIndex>>>>,
    warnings: Mutex<Vec<String>>,
}

impl IndexSet {
    /// Look for an installed index in the given folders (in order), and load the bundled sample.
    pub fn open(search: &[PathBuf], store: &SnapshotStore) -> Self {
        let mut warnings = Vec::new();
        let mut installed = None;
        for dir in search {
            let mp = dir.join("manifest.json");
            if !mp.is_file() {
                continue;
            }
            match std::fs::read_to_string(&mp)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<InstalledManifest>(&t).map_err(|e| e.to_string()))
            {
                Ok(m) if m.format == "phn-geocoder-index" && m.format_version == FORMAT_VERSION => {
                    installed = Some(Installed { dir: dir.clone(), manifest: m });
                    break;
                }
                Ok(_) => warnings.push(format!(
                    "the geocoder index in {} has an unsupported format version; rebuild it with this release's builder",
                    dir.display()
                )),
                Err(e) => warnings.push(format!("the geocoder index manifest in {} cannot be read ({e})", dir.display())),
            }
        }
        let (sample_text, sample_origin) = match store.file_text(SAMPLE_SNAPSHOT_ID, SAMPLE_FILE) {
            Ok((t, _, o)) => (Some(t), Some(o)),
            Err(_) => (None, None),
        };
        Self {
            installed,
            sample_text,
            sample_origin,
            sample: OnceLock::new(),
            cache: Mutex::new(HashMap::new()),
            warnings: Mutex::new(warnings),
        }
    }

    /// An index set with only the given sections (for tests).
    pub fn from_text(text: &str) -> Result<Self> {
        let mut cache = HashMap::new();
        for t in TownIndex::parse_sections(text)? {
            cache.insert(t.id, Some(Arc::new(t)));
        }
        Ok(Self {
            installed: None,
            sample_text: None,
            sample_origin: None,
            sample: OnceLock::new(),
            cache: Mutex::new(cache),
            warnings: Mutex::new(Vec::new()),
        })
    }

    fn warn(&self, w: String) {
        let mut v = self.warnings.lock().unwrap_or_else(|p| p.into_inner());
        if !v.contains(&w) {
            v.push(w);
        }
    }

    /// The street index of a town (MassGIS town id), from the installed index, else the sample.
    pub fn town(&self, id: u16) -> Option<Arc<TownIndex>> {
        if let Some(hit) = self
            .cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
        {
            return hit.clone();
        }
        let loaded = self.load(id);
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, loaded.clone());
        loaded
    }

    fn load(&self, id: u16) -> Option<Arc<TownIndex>> {
        if let Some(inst) = &self.installed
            && let Some(t) = inst.manifest.towns.get(&id.to_string())
        {
            let path = inst.dir.join(&t.file);
            if t.file.contains('/') || t.file.contains('\\') || t.file.contains("..") {
                self.warn(format!(
                    "the geocoder index lists an unsafe file name for town {id}"
                ));
            } else {
                match std::fs::read(&path) {
                    Ok(bytes) if sha256_hex(&bytes) == t.sha256 => {
                        match String::from_utf8(bytes)
                            .map_err(|e| e.to_string())
                            .and_then(|s| TownIndex::parse_sections(&s).map_err(|e| e.to_string()))
                        {
                            Ok(mut v) if !v.is_empty() => return Some(Arc::new(v.remove(0))),
                            Ok(_) => self.warn(format!("town file {} is empty", t.file)),
                            Err(e) => self.warn(format!("town file {} cannot be read: {e}", t.file)),
                        }
                    }
                    Ok(_) => self.warn(format!(
                        "town file {} does not match the checksum in the manifest; it was not used (rebuild or reinstall the index)",
                        t.file
                    )),
                    Err(e) => self.warn(format!("town file {} cannot be read: {e}", t.file)),
                }
            }
        }
        self.sample_map().get(&id).cloned()
    }

    fn sample_map(&self) -> &HashMap<u16, Arc<TownIndex>> {
        self.sample.get_or_init(|| {
            let Some(text) = &self.sample_text else {
                return HashMap::new();
            };
            match TownIndex::parse_sections(text) {
                Ok(v) => v.into_iter().map(|t| (t.id, Arc::new(t))).collect(),
                Err(e) => {
                    self.warn(format!("the bundled sample index cannot be read: {e}"));
                    HashMap::new()
                }
            }
        })
    }

    /// Ids of the towns the sample covers.
    pub fn sample_towns(&self) -> Vec<u16> {
        let mut v: Vec<u16> = self.sample_map().keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// True when a street index exists for the town.
    pub fn covers(&self, id: u16) -> bool {
        self.town(id).is_some()
    }

    /// Where the lookups came from.
    pub fn info(&self) -> IndexInfo {
        let warnings = self
            .warnings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(inst) = &self.installed {
            return IndexInfo {
                kind: "installed".into(),
                location: Some(inst.dir.display().to_string()),
                built: (!inst.manifest.built.is_empty()).then(|| inst.manifest.built.clone()),
                towns_covered: inst.manifest.towns.len(),
                warnings,
            };
        }
        let towns = self.sample_towns().len();
        IndexInfo {
            kind: if towns > 0 {
                "sample".into()
            } else {
                "none".into()
            },
            location: None,
            built: None,
            towns_covered: towns,
            warnings,
        }
    }

    /// Whether the sample came from the bundled snapshot or a newer cached copy.
    pub fn sample_origin(&self) -> Option<SnapshotOrigin> {
        self.sample_origin
    }

    /// Whether a full (installed) index is in use.
    pub fn is_installed(&self) -> bool {
        self.installed.is_some()
    }

    /// Installed-index kind recorded in its manifest ("full" or "sample"), if any.
    pub fn installed_kind(&self) -> Option<&str> {
        self.installed.as_ref().map(|i| i.manifest.kind.as_str())
    }
}

/// Folders searched for an installed index: `<data>/geocoder`, then `<cache>/geocoder`.
pub fn search_dirs(data_dir: &Path, cache_dir: &Path) -> Vec<PathBuf> {
    vec![data_dir.join("geocoder"), cache_dir.join("geocoder")]
}
