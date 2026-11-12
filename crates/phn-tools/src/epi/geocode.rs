//! EPI-13 `geocode_and_aggregate_addresses`: locate the addresses in a local file
//! offline and return only counts by town, census tract, or block group.
//!
//! Addresses are read from the nurse's file in `/data` and matched, inside the
//! container, against an index of MassGIS address points and TIGER address ranges
//! (`phn_data::geocoder`). Nothing but counts leaves the tool: small counts are
//! suppressed (with complementary suppression, because totals are shown), and
//! the response never contains an address, a coordinate, or any other value from
//! the file. Positions with coordinates are written to `/data/output` only when
//! the nurse asks, as an identifiable local file.

use std::collections::{BTreeMap, HashMap};

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::geocoder::index::search_dirs;
use phn_data::geocoder::{Crosswalk, IndexSet, Located, MatchLevel, normalize_place, parse_address};
use phn_data::geography::MaGeographies;
use phn_data::projection;
use phn_epi::suppression::{SuppressionOptions, suppress};
use phn_local::phi::{self, Identifier};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::localdata::{self, LocalTable};
use crate::md;

/// Most rows read from one file.
pub const MAX_ROWS: usize = 500_000;
/// Default smallest share of a ZIP code's addresses one town must hold for a ZIP-only
/// result to be assigned to that town.
pub const DEFAULT_ZIP_MIN_SHARE: f64 = 0.9;
/// A town holding less than this share of a ZIP code is not searched for the address.
const ZIP_SEARCH_MIN_SHARE: f64 = 0.02;
/// Areas listed in the markdown summary (the structured result lists them all).
const MAX_SUMMARY_AREAS: usize = 40;

/// The unit counts are reported for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AggregateLevel {
    /// City or town.
    Town,
    /// 2020 census tract.
    Tract,
    /// 2020 census block group.
    BlockGroup,
}

impl AggregateLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Town => "town",
            Self::Tract => "census tract",
            Self::BlockGroup => "census block group",
        }
    }
}

/// Arguments for `geocode_and_aggregate_addresses`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct GeocodeArgs {
    /// CSV or spreadsheet in /data with one address per row (clients, cases, facilities).
    pub path: String,
    /// Sheet name for a workbook (default: the first sheet).
    pub sheet: Option<String>,
    /// Column with the street address, or the whole address ("12 Elm St, Amherst, MA 01002").
    /// Default: found from the headers (address, street, addr). This is the one identifier
    /// column the tool reads; its values are never returned.
    pub address_column: Option<String>,
    /// Column with the city or town (default: found from the headers).
    pub town_column: Option<String>,
    /// Column with the ZIP code (default: found from the headers).
    pub zip_column: Option<String>,
    /// One town for every row when the file has no town or ZIP column (the street addresses
    /// are then looked up in that town).
    pub town: Option<String>,
    /// Report counts by "town", "tract", or "block_group". Default: the finest level the
    /// addresses could be placed at (tract when a street index covers them, otherwise town).
    pub level: Option<AggregateLevel>,
    /// Smallest share of a ZIP code's addresses that one town must hold for a row that only
    /// has a ZIP code to be counted in that town (default 0.9; ZIP codes cross town lines).
    pub zip_min_share: Option<f64>,
    /// Also write a CSV of row number, match level, town, tract, block group, and latitude
    /// and longitude for each row to /data/output (default false). The file contains the
    /// addresses as written in your file and is identifiable; it is never returned here.
    pub write_matched_file: bool,
}

/// How many addresses were located at each level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelCount {
    pub level: MatchLevel,
    pub label: String,
    /// Addresses; omitted when suppressed.
    pub count: Option<u64>,
    pub suppressed: bool,
    /// Percent of all rows (whole percent; omitted when the count is suppressed).
    pub percent: Option<u32>,
}

/// One row of a counts table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AreaCount {
    /// Town name, tract or block group name (with its GEOID), or a note row.
    pub area: String,
    /// Census GEOID of a tract or block group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geoid: Option<String>,
    /// Addresses; omitted when suppressed.
    pub count: Option<u64>,
    pub suppressed: bool,
}

/// Columns of the file the tool used (header names only).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColumnsUsed {
    pub address: Option<String>,
    pub town: Option<String>,
    pub zip: Option<String>,
}

/// Result of `geocode_and_aggregate_addresses`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeocodeResult {
    pub input: String,
    pub rows: usize,
    pub columns_used: ColumnsUsed,
    /// The level asked for ("best" when none was given).
    pub requested_level: String,
    /// The level of the counts below.
    pub level: AggregateLevel,
    /// Why the level differs from the one asked for, when it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_note: Option<String>,
    /// Which street index was used.
    pub index: phn_data::geocoder::IndexInfo,
    /// Addresses by how precisely they were located, with the total.
    pub match_levels: Vec<LevelCount>,
    /// Share of rows placed on a street position (exact point or interpolated), whole
    /// percent; omitted when a count behind it is small.
    pub positioned_percent: Option<u32>,
    /// Counts by area at `level`, in name order, with notes for rows that could not be
    /// counted at that level, and the total.
    pub areas: Vec<AreaCount>,
    /// Why rows were not located at all (counts suppressed like the others).
    pub not_located_reasons: Vec<AreaCount>,
    pub suppression_threshold: u32,
    pub footnote: String,
    /// File written to /data/output (only when asked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Reason {
    NoAddress,
    ZipSpansTowns,
    NotMassachusetts,
    Other,
}

impl Reason {
    fn label(self) -> &'static str {
        match self {
            Self::NoAddress => "No usable address, town, or ZIP code in the row",
            Self::ZipSpansTowns => "ZIP code reaches several towns and no street match decided between them",
            Self::NotMassachusetts => "Town or ZIP code not recognized as Massachusetts",
            Self::Other => "Other",
        }
    }
}

/// One row's outcome (never keeps the address).
#[derive(Debug, Clone)]
struct RowResult {
    level: MatchLevel,
    town: Option<(u16, String)>,
    bg: Option<String>,
    xy: Option<(f64, f64)>,
    reason: Option<Reason>,
}

fn normalized_header(h: &str) -> String {
    phn_core::text::normalize(h)
}

/// Header words that name each kind of column, best first.
const ADDRESS_HEADERS: &[&str] = &[
    "street address",
    "address",
    "home address",
    "residential address",
    "residence address",
    "address 1",
    "address1",
    "street",
    "addr",
    "location address",
    "mailing address",
    "full address",
    "service address",
];
const TOWN_HEADERS: &[&str] = &[
    "town",
    "city",
    "city town",
    "town city",
    "municipality",
    "city or town",
    "town of residence",
    "city of residence",
    "community",
];
const ZIP_HEADERS: &[&str] = &["zip", "zip code", "zipcode", "zip5", "postal code", "postcode", "zip code 5"];

/// Find a column by its header among `names` (best first). `Ok(None)` when there is none;
/// an error when two columns match the same best name.
fn detect(lt: &LocalTable, names: &[&str], what: &str) -> Result<Option<usize>> {
    for name in names {
        let hits: Vec<usize> = lt
            .table
            .headers
            .iter()
            .enumerate()
            .filter(|(_, h)| normalized_header(h) == *name)
            .map(|(i, _)| i)
            .collect();
        match hits.as_slice() {
            [] => {}
            [one] => return Ok(Some(*one)),
            _ => {
                return Err(PhnError::InvalidInput(format!(
                    "more than one column looks like the {what} column; name the one to use \
                     (columns: {})",
                    lt.table.headers.join(", ")
                )));
            }
        }
    }
    Ok(None)
}

/// Resolve an address, town, or ZIP column. These are the columns the tool exists to read, so
/// street addresses, geography, and ZIP codes are allowed; a column the identifier detector
/// takes for anything else (names, birth dates, phone numbers, ID numbers, free text) is refused.
fn column(
    lt: &LocalTable,
    explicit: Option<&str>,
    names: &[&str],
    what: &str,
    allowed: &[Identifier],
) -> Result<Option<usize>> {
    let idx = match explicit {
        Some(name) => Some(lt.table.column_index(name).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{}' has no column named '{name}'. Columns: {}",
                lt.label,
                lt.table.headers.join(", ")
            ))
        })?),
        None => detect(lt, names, what)?,
    };
    if let Some(i) = idx
        && let Some(f) = lt.findings.iter().find(|f| f.index == i)
        && !allowed.contains(&f.kind)
    {
        return Err(PhnError::Privacy(format!(
            "column '{}' appears to contain {} and cannot be used as the {what} column. This \
             tool reads only the address, town, and ZIP columns; pick one of those, or leave \
             the file's other columns alone (they are not read)",
            f.column,
            f.kind.label()
        )));
    }
    Ok(idx)
}

/// Five-digit ZIP from a cell ("1002" from a spreadsheet that dropped the zero, "01002-1234").
fn zip_from_cell(s: &str) -> Option<String> {
    let t = s.trim();
    let head = t.split('-').next()?.trim();
    let head = head.strip_suffix(".0").unwrap_or(head);
    if head.is_empty() || !head.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    match head.len() {
        5 => Some(head.to_string()),
        3 | 4 => Some(format!("{head:0>5}")),
        _ => None,
    }
}

struct Matcher<'a> {
    index: &'a IndexSet,
    crosswalk: &'a Crosswalk,
    zip_min_share: f64,
}

impl Matcher<'_> {
    fn locate_in(
        &self,
        town: (u16, &str),
        addr: &phn_data::geocoder::ParsedAddress,
    ) -> Option<Located> {
        self.index.town(town.0)?.locate(addr)
    }

    /// Place one row. `town_text` is the town column (or the tool's `town` argument).
    fn row(&self, address: &str, town_text: &str, zip_cell: &str) -> RowResult {
        let is_place = |p: &str| self.crosswalk.is_place(p);
        let parsed = parse_address(address, &is_place);
        let zip = zip_from_cell(zip_cell).or_else(|| parsed.zip.clone());
        let mut note_not_ma = false;

        // 1. A town named in the file or the address.
        let named = [Some(town_text.to_string()), parsed.town.clone()]
            .into_iter()
            .flatten()
            .filter(|t| !t.trim().is_empty())
            .find_map(|t| {
                let r = self.crosswalk.town_by_name(&normalize_place(&t));
                if r.is_none() {
                    note_not_ma = true;
                }
                r
            });
        if let Some((id, name)) = named {
            let town = Some((id, name.clone()));
            if !parsed.po_box
                && let Some(l) = self.locate_in((id, &name), &parsed)
            {
                return RowResult {
                    level: l.level,
                    town,
                    bg: l.block_group,
                    xy: l.xy,
                    reason: None,
                };
            }
            return RowResult { level: MatchLevel::TownOnly, town, bg: None, xy: None, reason: None };
        }

        // 2. A ZIP code.
        if let Some(z) = zip.as_deref() {
            let towns = self.crosswalk.zip_towns(z);
            let candidates: Vec<_> = towns.iter().filter(|t| t.share >= ZIP_SEARCH_MIN_SHARE).collect();
            if candidates.is_empty() {
                return RowResult {
                    level: MatchLevel::Unmatched,
                    town: None,
                    bg: None,
                    xy: None,
                    reason: Some(Reason::NotMassachusetts),
                };
            }
            // Street lookup in the towns the ZIP code reaches: accepted when exactly one of
            // them (all of them covered by an index) holds the address.
            if !parsed.po_box
                && !parsed.street_key.is_empty()
                && candidates.iter().all(|t| self.index.covers(t.town_id))
            {
                let hits: Vec<_> = candidates
                    .iter()
                    .filter_map(|t| {
                        self.locate_in((t.town_id, &t.town), &parsed)
                            .filter(|l| matches!(l.level, MatchLevel::ExactPoint))
                            .map(|l| ((t.town_id, t.town.clone()), l))
                    })
                    .collect();
                if let [(town, l)] = hits.as_slice() {
                    return RowResult {
                        level: l.level,
                        town: Some(town.clone()),
                        bg: l.block_group.clone(),
                        xy: l.xy,
                        reason: None,
                    };
                }
            }
            if let Some(t) = towns.first().filter(|t| t.share >= self.zip_min_share) {
                // A single-town ZIP code: look the street up in that town, else ZIP only.
                if !parsed.po_box
                    && let Some(l) = self.locate_in((t.town_id, &t.town), &parsed)
                {
                    return RowResult {
                        level: l.level,
                        town: Some((t.town_id, t.town.clone())),
                        bg: l.block_group,
                        xy: l.xy,
                        reason: None,
                    };
                }
                return RowResult {
                    level: MatchLevel::ZipOnly,
                    town: Some((t.town_id, t.town.clone())),
                    bg: None,
                    xy: None,
                    reason: None,
                };
            }
            return RowResult {
                level: MatchLevel::Unmatched,
                town: None,
                bg: None,
                xy: None,
                reason: Some(Reason::ZipSpansTowns),
            };
        }
        RowResult {
            level: MatchLevel::Unmatched,
            town: None,
            bg: None,
            xy: None,
            reason: Some(if note_not_ma {
                Reason::NotMassachusetts
            } else if address.trim().is_empty() {
                Reason::NoAddress
            } else {
                Reason::Other
            }),
        }
    }
}

/// Counts of a list of keys, suppressed with a total row.
fn suppressed_table(
    rows: &[(String, Option<String>, u64)],
    threshold: u32,
) -> (Vec<AreaCount>, usize, usize) {
    let mut cells: Vec<Vec<Option<u64>>> = rows.iter().map(|r| vec![Some(r.2)]).collect();
    let total: u64 = rows.iter().map(|r| r.2).sum();
    cells.push(vec![Some(total)]);
    let res = suppress(
        &cells,
        &SuppressionOptions {
            threshold,
            suppress_zeros: false,
            complementary: true,
            last_column_is_total: false,
            last_row_is_total: true,
        },
    );
    let mut out = Vec::new();
    for (i, (name, geoid, n)) in rows.iter().enumerate() {
        let hidden = res.is_hidden(i, 0);
        out.push(AreaCount {
            area: name.clone(),
            geoid: geoid.clone(),
            count: (!hidden).then_some(*n),
            suppressed: hidden,
        });
    }
    let hidden = res.is_hidden(rows.len(), 0);
    out.push(AreaCount {
        area: "Total rows".into(),
        geoid: None,
        count: (!hidden).then_some(total),
        suppressed: hidden,
    });
    (out, res.primary, res.complementary)
}

fn cell_text(row: &[String], idx: Option<usize>) -> &str {
    idx.and_then(|i| row.get(i)).map_or("", |s| s.trim())
}

fn tract_of(bg: &str) -> &str {
    bg.get(..11).unwrap_or(bg)
}

fn count_text(c: &AreaCount) -> String {
    match c.count {
        Some(n) => md::num(n as f64, 0),
        None => "*".into(),
    }
}

/// Locate the addresses in a local file and return counts by town, tract, or block group.
pub fn geocode_and_aggregate_addresses(
    ctx: &Context,
    args: GeocodeArgs,
) -> Result<Envelope<GeocodeResult>> {
    let lt = localdata::load(ctx, &args.path, args.sheet.as_deref())?;
    if lt.table.rows.len() > MAX_ROWS {
        return Err(PhnError::InvalidInput(format!(
            "the file has more than {MAX_ROWS} rows; split it and run the tool on each part"
        )));
    }
    let share = args.zip_min_share.unwrap_or(DEFAULT_ZIP_MIN_SHARE);
    if !(0.5..=1.0).contains(&share) {
        return Err(PhnError::InvalidInput(
            "zip_min_share must be between 0.5 and 1".into(),
        ));
    }
    let addr_i = column(
        &lt,
        args.address_column.as_deref(),
        ADDRESS_HEADERS,
        "address",
        &[Identifier::StreetAddress, Identifier::Geography],
    )?;
    let town_i = column(
        &lt,
        args.town_column.as_deref(),
        TOWN_HEADERS,
        "town",
        &[Identifier::Geography],
    )?;
    let zip_i = column(
        &lt,
        args.zip_column.as_deref(),
        ZIP_HEADERS,
        "ZIP code",
        &[Identifier::ZipCode, Identifier::Geography],
    )?;
    let default_town = args.town.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if addr_i.is_none() && town_i.is_none() && zip_i.is_none() && default_town.is_none() {
        return Err(PhnError::InvalidInput(format!(
            "no address, town, or ZIP column was found in '{}'. Name them with address_column, \
             town_column, or zip_column. Columns: {}",
            lt.label,
            lt.table.headers.join(", ")
        )));
    }
    let town_default = match default_town {
        Some(t) => {
            let m = ctx.ma.town(t).ok_or_else(|| {
                let s = ctx.ma.suggest_towns(t, 3);
                PhnError::InvalidInput(format!(
                    "'{t}' is not a Massachusetts city or town{}",
                    if s.is_empty() {
                        String::new()
                    } else {
                        format!(". Did you mean {}?", s.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", "))
                    }
                ))
            })?;
            Some(m.name.clone())
        }
        None => None,
    };

    let towns: Vec<(u16, String)> = ctx
        .ma
        .municipalities
        .iter()
        .filter_map(|m| u16::try_from(m.town_id).ok().map(|id| (id, m.name.clone())))
        .collect();
    let crosswalk = Crosswalk::load(&ctx.snapshots, &towns)?;
    let index = IndexSet::open(
        &search_dirs(&ctx.config.paths.data_dir, &ctx.config.paths.cache_dir),
        &ctx.snapshots,
    );
    let matcher = Matcher { index: &index, crosswalk: &crosswalk, zip_min_share: share };

    // ---- Locate every row.
    let mut results: Vec<RowResult> = Vec::with_capacity(lt.table.rows.len());
    for row in &lt.table.rows {
        let town_text = match (cell_text(row, town_i), town_default.as_deref()) {
            ("", Some(d)) => d,
            (t, _) => t,
        };
        results.push(matcher.row(cell_text(row, addr_i), town_text, cell_text(row, zip_i)));
    }
    let n_rows = results.len();
    let threshold = ctx.suppression_threshold();

    // ---- Match levels.
    let mut by_level: BTreeMap<MatchLevel, u64> = BTreeMap::new();
    for r in &results {
        *by_level.entry(r.level).or_insert(0) += 1;
    }
    let level_rows: Vec<(String, Option<String>, u64)> = MatchLevel::ALL
        .iter()
        .map(|l| (l.label().to_string(), None, by_level.get(l).copied().unwrap_or(0)))
        .collect();
    let (level_tab, _, _) = suppressed_table(&level_rows, threshold);
    let match_levels: Vec<LevelCount> = MatchLevel::ALL
        .iter()
        .zip(level_tab.iter())
        .map(|(l, c)| LevelCount {
            level: *l,
            label: l.label().to_string(),
            count: c.count,
            suppressed: c.suppressed,
            percent: c
                .count
                .filter(|_| n_rows > 0)
                .map(|n| ((n as f64) * 100.0 / n_rows as f64).round() as u32),
        })
        .collect();
    let positioned = by_level.get(&MatchLevel::ExactPoint).copied().unwrap_or(0)
        + by_level.get(&MatchLevel::Interpolated).copied().unwrap_or(0);
    let t = u64::from(threshold);
    let positioned_percent = (positioned >= t && (n_rows as u64).saturating_sub(positioned) >= t)
        .then(|| ((positioned as f64) * 100.0 / n_rows as f64).round() as u32);

    // ---- Level of the counts.
    let any_bg = results.iter().any(|r| r.bg.is_some());
    let requested_text = match args.level {
        Some(AggregateLevel::Town) => "town",
        Some(AggregateLevel::Tract) => "tract",
        Some(AggregateLevel::BlockGroup) => "block_group",
        None => "best",
    };
    let (level, level_note) = match args.level {
        None if any_bg => (AggregateLevel::Tract, None),
        None => (
            AggregateLevel::Town,
            Some(
                "No address could be placed on a street position, so counts are by town. Install the full geocoder index (see docs/adr/0011-offline-geocoder.md) for tract-level counts."
                    .to_string(),
            ),
        ),
        Some(l) if l != AggregateLevel::Town && !any_bg => (
            AggregateLevel::Town,
            Some(format!(
                "No address could be placed on a {} (the street index covers none of the towns in this file, or the file has no street addresses), so counts are by town.",
                l.label()
            )),
        ),
        Some(l) => (l, None),
    };

    // ---- Areas at that level.
    let geos = MaGeographies::shared(&ctx.snapshots).ok();
    let mut named: BTreeMap<String, (Option<String>, u64)> = BTreeMap::new();
    let mut not_at_level = 0u64;
    let mut not_located = 0u64;
    for r in &results {
        if r.level == MatchLevel::Unmatched || r.town.is_none() {
            not_located += 1;
            continue;
        }
        match level {
            AggregateLevel::Town => {
                let name = r.town.as_ref().map(|t| t.1.clone()).unwrap_or_default();
                named.entry(name).or_insert((None, 0)).1 += 1;
            }
            AggregateLevel::Tract | AggregateLevel::BlockGroup => match &r.bg {
                Some(bg) => {
                    let (geoid, label) = if level == AggregateLevel::Tract {
                        let g = tract_of(bg).to_string();
                        let l = geos
                            .and_then(|g2| g2.get(&g))
                            .map(|x| match &x.town {
                                Some(t) => format!("{} (mainly {t})", x.census_name),
                                None => x.census_name.clone(),
                            })
                            .unwrap_or_else(|| "Census tract".into());
                        (g, l)
                    } else {
                        let t = tract_of(bg);
                        let tn = geos.and_then(|g2| g2.get(t)).map_or_else(
                            || "Census tract".to_string(),
                            |x| x.census_name.clone(),
                        );
                        let bgn = bg.get(11..).unwrap_or("");
                        (bg.clone(), format!("Block Group {bgn}, {tn}"))
                    };
                    named
                        .entry(format!("{geoid}\u{0}{label}"))
                        .or_insert((Some(geoid), 0))
                        .1 += 1;
                }
                None => not_at_level += 1,
            },
        }
    }
    let mut area_rows: Vec<(String, Option<String>, u64)> = named
        .into_iter()
        .map(|(k, (g, n))| {
            let name = k.split_once('\u{0}').map_or(k.clone(), |(_, l)| l.to_string());
            (name, g, n)
        })
        .collect();
    if not_at_level > 0 {
        area_rows.push((
            format!("Counted in a town only (no {} for these addresses)", level.label()),
            None,
            not_at_level,
        ));
    }
    if not_located > 0 {
        area_rows.push(("Not located".to_string(), None, not_located));
    }
    let (areas, primary, complementary) = suppressed_table(&area_rows, threshold);

    // ---- Why rows were not located.
    let mut reasons: BTreeMap<Reason, u64> = BTreeMap::new();
    for r in &results {
        if let Some(why) = r.reason {
            *reasons.entry(why).or_insert(0) += 1;
        }
    }
    let reason_rows: Vec<(String, Option<String>, u64)> = reasons
        .iter()
        .map(|(k, n)| (k.label().to_string(), None, *n))
        .collect();
    let not_located_reasons = if reason_rows.is_empty() {
        Vec::new()
    } else {
        suppressed_table(&reason_rows, threshold).0
    };

    // ---- Optional local file with positions.
    let sandbox = ctx.sandbox()?;
    let mut output_file = None;
    if args.write_matched_file {
        let stem = lt
            .label
            .rsplit('/')
            .next()
            .and_then(|n| n.rsplit_once('.').map(|(s, _)| s.to_string()))
            .unwrap_or_else(|| "addresses".into());
        let mut rows_out: Vec<Vec<String>> = Vec::with_capacity(n_rows);
        for (i, (r, src)) in results.iter().zip(lt.table.rows.iter()).enumerate() {
            let (lat, lon) = r
                .xy
                .map(|(x, y)| projection::from_mass_mainland(x, y))
                .map(|(a, b)| (format!("{a:.6}"), format!("{b:.6}")))
                .unwrap_or_default();
            rows_out.push(vec![
                (i + 1).to_string(),
                cell_text(src, addr_i).to_string(),
                format!("{:?}", r.level).to_lowercase(),
                r.town.as_ref().map(|t| t.1.clone()).unwrap_or_default(),
                r.bg.as_deref().map(|b| tract_of(b).to_string()).unwrap_or_default(),
                r.bg.clone().unwrap_or_default(),
                lat,
                lon,
            ]);
        }
        let block = phn_docs::model::TableBlock {
            caption: None,
            headers: [
                "row", "address", "match_level", "town", "tract_geoid", "block_group_geoid",
                "latitude", "longitude",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            rows: rows_out,
        };
        let path = sandbox.new_output_file(&format!("{stem}_geocoded.csv"))?;
        std::fs::write(&path, phn_docs::csv_out::table_to_csv(&block)?)?;
        output_file = Some(sandbox.display_path(&path));
    }

    // ---- Summary.
    let info = index.info();
    let index_text = match info.kind.as_str() {
        "installed" => format!(
            "installed street index ({} towns, built {})",
            info.towns_covered,
            info.built.as_deref().unwrap_or("date not recorded")
        ),
        "sample" => format!(
            "bundled four-town sample index (Alford, Amherst, Northampton, Pelham); other towns are counted by town only"
        ),
        _ => "no street index; counts by town or ZIP code only".to_string(),
    };
    let mut summary = format!(
        "**Addresses counted by {}** from {} rows of /data/{}. Street index: {index_text}.\n\n",
        level.label(),
        md::num(n_rows as f64, 0),
        lt.label
    );
    if let Some(n) = &level_note {
        summary.push_str(&format!("{n}\n\n"));
    }
    summary.push_str("### How precisely addresses were located\n\n");
    let rows: Vec<Vec<String>> = match_levels
        .iter()
        .map(|l| {
            vec![
                l.label.clone(),
                l.count.map_or("*".into(), |n| md::num(n as f64, 0)),
                l.percent.map_or(String::new(), |p| format!("{p}%")),
            ]
        })
        .collect();
    summary.push_str(&md::table(&["Located as", "Addresses", "Share"], &rows));
    match positioned_percent {
        Some(p) => summary.push_str(&format!(
            "\nMatch rate to a street position (exact point or interpolated): {p}%.\n"
        )),
        None => summary.push_str(
            "\nThe match rate to a street position is not shown because one of its counts is small.\n",
        ),
    }
    summary.push_str(&format!("\n### Counts by {}\n\n", level.label()));
    let shown: Vec<&AreaCount> = areas.iter().take(MAX_SUMMARY_AREAS).collect();
    let rows: Vec<Vec<String>> = shown
        .iter()
        .map(|a| {
            vec![
                match &a.geoid {
                    Some(g) => format!("{} ({g})", a.area),
                    None => a.area.clone(),
                },
                count_text(a),
            ]
        })
        .collect();
    summary.push_str(&md::table(&["Area", "Addresses"], &rows));
    if areas.len() > shown.len() {
        summary.push_str(&format!(
            "\n{} more rows are in the structured result.\n",
            areas.len() - shown.len()
        ));
    }
    if !not_located_reasons.is_empty() {
        summary.push_str("\n### Why some rows were not located\n\n");
        let rows: Vec<Vec<String>> = not_located_reasons
            .iter()
            .take(not_located_reasons.len() - 1)
            .map(|a| vec![a.area.clone(), count_text(a)])
            .collect();
        summary.push_str(&md::table(&["Reason", "Rows"], &rows));
    }
    let footnote = format!(
        "* Counts under {threshold} are suppressed; further cells are also hidden so a hidden count cannot be found by subtraction from the total. {primary} cell(s) below the threshold, {complementary} hidden to protect them."
    );
    summary.push_str(&format!("\n{footnote}\n"));
    summary.push_str(
        "\nNo address, name, or other value from your file is in this response. \
         Counts are numbers of rows (addresses), not people or households.\n",
    );
    if let Some(f) = &output_file {
        summary.push_str(&format!(
            "\nA file with each row's match level, town, tract, block group, and coordinates was written to /data/{f}. It contains the addresses from your file and is identifiable: keep it in your secure folder.\n"
        ));
    }

    // ---- Sources and caveats.
    let mut sources = vec![Source::local_file(&lt.label)];
    match info.kind.as_str() {
        "installed" => sources.push(Source {
            title: format!(
                "Installed geocoder index at {} (MassGIS Master Address Data and TIGER/Line, built {})",
                info.location.as_deref().unwrap_or("the geocoder folder"),
                info.built.as_deref().unwrap_or("date not recorded")
            ),
            publisher: "MassGIS (Commonwealth of Massachusetts); U.S. Census Bureau; built on this computer".into(),
            url: Some("https://www.mass.gov/info-details/massgis-data-master-address-data".into()),
            effective_date: None,
            retrieved: info.built.clone(),
            mode: SourceMode::Local,
        }),
        "sample" => {
            if let Some((e, _)) = ctx.snapshots.entry(phn_data::geocoder::index::SAMPLE_SNAPSHOT_ID) {
                sources.push(e.to_source());
            }
        }
        _ => {}
    }
    if let Some((e, _)) = ctx.snapshots.entry(phn_data::geocoder::crosswalk::SNAPSHOT_ID) {
        sources.push(e.to_source());
    }
    sources.push(
        Source::computed(
            "Offline address matching: street-name normalization, exact address-point match, interpolation between points and along TIGER address ranges, ZIP-code and town fallback",
            "This server (docs/adr/0011-offline-geocoder.md)",
        ),
    );
    let mut caveats = vec![
        "Addresses are matched inside this server against bundled and locally installed data; nothing is sent to a geocoding service, and nothing from the file is returned.".to_string(),
        format!(
            "Counts under {threshold} are suppressed with complementary suppression. These are counts of rows in your file; they are not rates (use the area's population as the denominator, for example from ACS)."
        ),
        "MassGIS address points mostly mark building centers (sometimes parcel centers), so a point can fall in a different block group than the front door. Interpolated positions assume numbers are spread evenly along a straight line.".to_string(),
        "Tracts and block groups are the 2020 Census geographies; they do not change before the 2030 Census.".to_string(),
        "Boston has no MassGIS address-point download, so Boston addresses are placed by TIGER address ranges only (interpolated or street-only matches).".to_string(),
    ];
    if info.kind != "installed" {
        caveats.push("Only the bundled four-town sample index is available, so addresses elsewhere are counted by town (from a town column, a town written in the address, or a ZIP code). Install the full index to place them on streets and count by tract (docs/adr/0011-offline-geocoder.md).".to_string());
    }
    if by_level.get(&MatchLevel::ZipOnly).copied().unwrap_or(0) > 0 {
        caveats.push(format!(
            "ZIP-only rows are counted in the town that holds at least {:.0}% of the ZIP code's addresses; ZIP codes cross town lines, so these counts are approximate. Rows whose ZIP code reaches several towns were not assigned to one.",
            share * 100.0
        ));
    }
    caveats.extend(info.warnings.iter().cloned());
    caveats.extend(crosswalk.caveats.iter().cloned());

    let mut kinds = lt.identifier_categories();
    if !kinds.iter().any(|k| k == Identifier::StreetAddress.label()) && addr_i.is_some() {
        kinds.push(Identifier::StreetAddress.label().to_string());
        kinds.sort();
    }
    let _ = phi::is_missing; // keep the detector import used by tests
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: kinds,
        files_written: output_file.iter().cloned().collect(),
        notes: {
            let mut n = vec![
                "Only the address, town, and ZIP columns were read. Addresses, other columns, and coordinates are not in this response.".to_string(),
            ];
            if output_file.is_some() {
                n.push("The file written to the output folder contains addresses and coordinates (identifiable); its contents are not returned.".into());
            }
            n
        },
    };
    let cols = ColumnsUsed {
        address: addr_i.map(|i| lt.table.headers[i].clone()),
        town: town_i.map(|i| lt.table.headers[i].clone()),
        zip: zip_i.map(|i| lt.table.headers[i].clone()),
    };
    let _: HashMap<(), ()> = HashMap::new();
    Ok(Envelope::new(
        summary,
        GeocodeResult {
            input: lt.label.clone(),
            rows: n_rows,
            columns_used: cols,
            requested_level: requested_text.to_string(),
            level,
            level_note,
            index: info,
            match_levels,
            positioned_percent,
            areas,
            not_located_reasons,
            suppression_threshold: threshold,
            footnote,
            output_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}
