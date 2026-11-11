//! ENV-06 `vector_borne_disease_risk`: DPH's EEE and West Nile virus risk
//! levels for a town, county, or the state (live from DPH's risk workbook
//! during the season, otherwise the last levels, clearly dated), the actions
//! DPH recommends at that level, tick-borne disease cases and rates by county
//! with trends, current-year babesiosis and anaplasmosis counts from NNDSS,
//! and prevention messages for the public, schools, camps, and outdoor
//! workers. Optional one-page DOCX notice.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::vectorborne::{self, PositiveCount, RiskLevel, TickCases, TickEdVisits, TownRisk};
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, DocSource};
use crate::md;

/// Content id of `content/env/vector_borne.yaml`.
pub const VECTOR_BORNE: &str = "env-vector-borne";

/// Most towns listed by name in the summary.
const MAX_TOWNS_LISTED: usize = 40;

// ---------------------------------------------------------------- content

/// One risk level's meaning and actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelDef {
    pub level: String,
    pub rank: u8,
    pub color: String,
    pub meaning: String,
    pub definition: String,
    pub public_actions: Vec<String>,
    pub dph_response: Vec<String>,
}

/// Season facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonDef {
    pub first_month: u32,
    pub last_month: u32,
    pub description: Cited,
    pub hard_frost: Cited,
    pub peak: Cited,
    pub focal_areas: Cited,
}

/// One week of DPH's recommended cancellation times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CancellationWeek {
    pub from: String,
    pub to: String,
    /// Cancel outdoor activities from this time...
    pub start: String,
    /// ...until this time the next morning.
    pub end: String,
}

/// DPH's cancellation schedule for one season.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancellationTimes {
    pub season: i32,
    pub source: String,
    pub section: String,
    pub confidence: content::Confidence,
    pub note: String,
    pub weeks: Vec<CancellationWeek>,
}

/// Prevention messages by audience.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreventionDef {
    pub public: Vec<Cited>,
    pub schools: Vec<Cited>,
    pub camps: Vec<Cited>,
    pub outdoor_workers: Vec<Cited>,
}

/// `content/env/vector_borne.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub season: SeasonDef,
    pub eee_levels: Vec<LevelDef>,
    pub eee_source: String,
    pub eee_section: String,
    pub eee_response_source: String,
    pub eee_response_section: String,
    pub wnv_levels: Vec<LevelDef>,
    pub wnv_source: String,
    pub wnv_section: String,
    pub wnv_response_source: String,
    pub wnv_response_section: String,
    pub cancellation_times: CancellationTimes,
    pub prevention: PreventionDef,
    pub disease_notes: Vec<Cited>,
    pub data_notes: Vec<Cited>,
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `vector_borne_disease_risk`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct VectorRiskArgs {
    /// A Massachusetts city or town, a county (e.g. "Plymouth County"), or "Massachusetts".
    /// Default: your saved town, or the whole state when none is saved.
    pub area: Option<String>,
    /// Arbovirus season (year) to show, from 2022 to the latest DPH has published. Default:
    /// the current season (June to October), or the most recent one outside the season.
    pub season: Option<i32>,
    /// Who the prevention messages are for: "public", "schools", "camps", "outdoor_workers",
    /// or "all" (default all).
    pub audiences: Vec<String>,
    /// Write a one-page DOCX notice (levels, what to do, prevention) to /data/output
    /// (default false). It uses the first audience given (default the public).
    pub notice: Option<bool>,
}

/// Guidance for one risk level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelGuidance {
    /// "EEE" or "WNV".
    pub virus: String,
    pub level: String,
    pub map_color: String,
    /// What the level means (DPH).
    pub meaning: String,
    /// DPH's definition of the level for a focal area (shortened).
    pub definition: String,
    /// What residents can do at this level (DPH, cumulative).
    pub public_actions: Vec<String>,
    /// What DPH's plan recommends at this level (shortened).
    pub dph_response: Vec<String>,
    pub citation: String,
}

/// A town's levels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownLevels {
    pub town: String,
    pub county: String,
    pub eee: String,
    pub wnv: String,
}

/// Towns at each level in the area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelCount {
    pub level: String,
    pub eee_towns: usize,
    pub wnv_towns: usize,
}

/// Positive findings in the area this season.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PositiveSummary {
    pub virus: String,
    /// "Mosquito", "Animal", or "Human".
    pub sample_type: String,
    /// Positive mosquito samples, animal cases, or human cases.
    pub count: u32,
    /// Towns with positive mosquito samples or animal cases (none for human cases).
    pub towns: usize,
    /// Latest collection or onset date (none for human cases).
    pub latest_date: Option<String>,
    /// Where: the town, county, or "Massachusetts". Human cases are counted by county of
    /// residence (DPH does not publish their town).
    pub scope: String,
}

/// One earlier season in the area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SeasonHistory {
    pub season: i32,
    pub as_of: String,
    pub highest_eee: String,
    pub highest_wnv: String,
    /// Towns in the area at EEE moderate or above.
    pub towns_eee_moderate_or_higher: usize,
    /// Towns in the area at WNV moderate or above.
    pub towns_wnv_moderate_or_higher: usize,
}

/// DPH's recommended cancellation window for the week of a date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CancellationWindow {
    pub season: i32,
    pub week: CancellationWeek,
    pub citation: String,
}

/// Mosquito-borne disease part of the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArbovirusSummary {
    pub season: i32,
    /// Date DPH last updated these levels.
    pub as_of: String,
    /// "live", "cache", or "snapshot".
    pub data_mode: String,
    /// True only during the season, for this season's levels, from a live or cached copy.
    pub current: bool,
    /// The town's levels (town requests).
    pub town: Option<TownLevels>,
    /// Towns at each level in the area.
    pub towns_by_level: Vec<LevelCount>,
    /// Towns in the area at EEE moderate or above or WNV high or above (at most 40).
    pub elevated_towns: Vec<TownLevels>,
    /// Elevated towns not listed because of the limit.
    pub elevated_towns_not_listed: usize,
    pub highest_eee: String,
    pub highest_wnv: String,
    /// Guidance for the highest EEE and WNV levels in the area.
    pub guidance: Vec<LevelGuidance>,
    pub positives: Vec<PositiveSummary>,
    /// Earlier seasons in the area (final levels).
    pub history: Vec<SeasonHistory>,
    /// DPH's cancellation window for this week, when EEE is high or critical in season.
    pub cancellation: Option<CancellationWindow>,
}

/// One year of tick-borne disease cases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TickYear {
    pub year: i32,
    pub cases: Option<u64>,
    /// DPH's incidence rate per 100,000 residents, as published.
    pub rate_per_100k: Option<f64>,
    /// Small-number flags ("fewer than 20 cases: unstable, interpret with caution").
    pub flags: Vec<String>,
}

/// A tick-borne disease in the area and in Massachusetts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TickDiseaseSeries {
    pub disease: String,
    /// County as DPH reports it ("Dukes & Nantucket" combined) or "Massachusetts".
    pub area: String,
    pub case_classes: String,
    pub years: Vec<TickYear>,
    pub massachusetts: Vec<TickYear>,
    /// Change in the area's cases from the previous year to the latest (percent).
    pub change_from_previous_year_pct: Option<f64>,
    /// The area's latest rate divided by the Massachusetts rate.
    pub ratio_to_state: Option<f64>,
}

/// Statewide total in DPH's ten-year table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StateTotal {
    pub year: i32,
    pub disease: String,
    pub cases: Option<u64>,
    pub case_classes: String,
}

/// Emergency department visits with a tick-borne disease diagnosis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EdVisitRow {
    pub year: i32,
    pub through_month: String,
    pub county: String,
    pub tickborne_disease_visits: u64,
    pub ed_visits: u64,
    pub per_10k_ed_visits: Option<f64>,
}

/// Current-year NNDSS count for Massachusetts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NndssCount {
    pub condition: String,
    pub mmwr_year: i32,
    pub mmwr_week: u32,
    pub week_ending: String,
    pub year_to_date: Option<f64>,
    /// Why the year-to-date count is missing (CDC flag meaning or "no data").
    pub note: Option<String>,
    /// Mean of the same weeks in the five previous years.
    pub five_year_mean: Option<f64>,
    /// "above 5-year range", "within 5-year range", "below 5-year range", or "no baseline".
    pub assessment: String,
}

/// Tick-borne disease part of the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TickborneSummary {
    /// The county the figures are for ("Massachusetts" for the state).
    pub geography: String,
    pub diseases: Vec<TickDiseaseSeries>,
    pub statewide_ten_year: Vec<StateTotal>,
    pub ed_visits: Vec<EdVisitRow>,
    /// Babesiosis and anaplasmosis year to date for Massachusetts (provisional NNDSS).
    pub current_year: Vec<NndssCount>,
    /// "live", "cache", "snapshot", or "unavailable".
    pub current_year_status: String,
}

/// Prevention messages for one audience.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudienceMessages {
    pub audience: String,
    pub messages: Vec<CitedOut>,
}

/// Result of `vector_borne_disease_risk`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VectorRiskResult {
    pub area: Area,
    /// Today's date (local).
    pub today: String,
    /// True from June through October.
    pub in_season: bool,
    /// Plain statement of where the season stands and how current the levels are.
    pub season_status: String,
    pub arbovirus: ArbovirusSummary,
    pub tickborne: TickborneSummary,
    pub prevention: Vec<AudienceMessages>,
    pub disease_notes: Vec<CitedOut>,
    pub notice_file: Option<String>,
}

// ---------------------------------------------------------------- arbovirus data

/// One season of arbovirus data.
#[derive(Debug, Clone, PartialEq)]
struct ArboSeason {
    season: i32,
    as_of: String,
    towns: Vec<TownRisk>,
    positives: Vec<PositiveCount>,
    mode: SourceMode,
    source: Source,
}

fn cell<'a>(row: &'a [String], i: Option<usize>) -> &'a str {
    i.and_then(|i| row.get(i)).map(|s| s.trim()).unwrap_or("")
}

/// A spreadsheet date (ISO text, or an Excel serial number) as YYYY-MM-DD.
fn sheet_date(v: &str) -> Option<String> {
    let v = v.trim();
    if v.is_empty() {
        return None;
    }
    if let Ok(serial) = v.parse::<f64>() {
        let iso = phn_local::table::excel_serial_to_iso(serial);
        return iso.get(..10).map(str::to_string);
    }
    let d = v.get(..10)?;
    NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
    Some(d.to_string())
}

fn find_col(headers: &[String], names: &[&str]) -> Option<usize> {
    headers.iter().position(|h| {
        let h = h.trim().to_ascii_lowercase();
        names.iter().any(|n| h == *n)
    })
}

/// Parse a DPH risk workbook (its tables as read by `phn_local`).
fn parse_workbook(
    season: i32,
    tables: &[phn_local::table::Table],
) -> Result<(Vec<TownRisk>, Vec<PositiveCount>, String)> {
    let bad = |m: &str| PhnError::Network(format!("DPH risk workbook: {m}"));
    let risk = tables
        .iter()
        .find(|t| {
            find_col(&t.headers, &["town"]).is_some()
                && find_col(&t.headers, &["eee risk level"]).is_some()
        })
        .ok_or_else(|| bad("no 'Risk by Town' sheet"))?;
    let h = &risk.headers;
    let (ct, cc) = (find_col(h, &["town"]), find_col(h, &["county"]));
    let (ce, cw) = (
        find_col(h, &["eee risk level"]),
        find_col(h, &["wnv risk level"]),
    );
    let cd = find_col(h, &["last updated", "data as of"]);
    let mut towns = Vec::new();
    let mut dates = BTreeSet::new();
    for r in &risk.rows {
        let town = cell(r, ct);
        if town.is_empty() {
            continue;
        }
        let (Some(eee), Some(wnv)) = (RiskLevel::parse(cell(r, ce)), RiskLevel::parse(cell(r, cw)))
        else {
            return Err(bad(&format!("unknown risk level for {town}")));
        };
        let as_of = sheet_date(cell(r, cd)).unwrap_or_default();
        dates.insert(as_of.clone());
        towns.push(TownRisk {
            season,
            town: vectorborne::canonical_town(town),
            county: cell(r, cc).to_string(),
            eee,
            wnv,
            as_of,
        });
    }
    if towns.len() < 300 {
        return Err(bad(&format!("only {} towns", towns.len())));
    }
    let as_of = dates
        .into_iter()
        .rfind(|d| !d.is_empty())
        .ok_or_else(|| bad("no update date"))?;
    let mut positives: BTreeMap<(String, String, String, Option<String>), (u32, Option<String>)> =
        BTreeMap::new();
    if let Some(ps) = tables.iter().find(|t| {
        find_col(&t.headers, &["virus"]).is_some()
            && find_col(&t.headers, &["sample type"]).is_some()
    }) {
        let h = &ps.headers;
        let (cv, cty, cco) = (
            find_col(h, &["virus"]),
            find_col(h, &["sample type"]),
            find_col(h, &["county"]),
        );
        let (ccm, cdt) = (find_col(h, &["community"]), find_col(h, &["date"]));
        for r in &ps.rows {
            let virus = cell(r, cv).to_ascii_uppercase();
            if virus.is_empty() {
                continue;
            }
            let kind = match cell(r, cty).to_ascii_lowercase().as_str() {
                "mosquito" => "Mosquito",
                "animal" => "Animal",
                "human" => "Human",
                _ => continue,
            };
            let human = kind == "Human";
            let town = (!human && !cell(r, ccm).is_empty())
                .then(|| vectorborne::canonical_town(cell(r, ccm)));
            let date = if human {
                None
            } else {
                sheet_date(cell(r, cdt))
            };
            let e = positives
                .entry((virus, kind.to_string(), cell(r, cco).to_string(), town))
                .or_insert((0, None));
            e.0 += 1;
            if date > e.1 {
                e.1 = date;
            }
        }
    }
    let positives = positives
        .into_iter()
        .map(
            |((virus, sample_type, county, town), (count, latest_date))| PositiveCount {
                season,
                virus,
                sample_type,
                county,
                town,
                count,
                latest_date,
            },
        )
        .collect();
    Ok((towns, positives, as_of))
}

/// Read workbook bytes through a temporary file in the cache folder.
fn read_workbook_bytes(
    ctx: &Context,
    season: i32,
    bytes: &[u8],
) -> Result<Vec<phn_local::table::Table>> {
    let dir = ctx.config.paths.cache_dir.join("tmp");
    std::fs::create_dir_all(&dir)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let path = dir.join(format!(
        "dph_arbovirus_{season}_{}_{nanos}.xlsx",
        std::process::id()
    ));
    std::fs::write(&path, bytes)?;
    let tables = phn_local::table::read_tables(&path);
    let _ = std::fs::remove_file(&path);
    tables
}

/// Try DPH's live workbook for `season` (or the newest when None).
async fn live_season(ctx: &Context, season: Option<i32>, ttl: u64) -> Result<ArboSeason> {
    let page = vectorborne::fetch_page(&ctx.http, ctx.cache.as_ref(), ttl).await?;
    let html = String::from_utf8_lossy(&page.body);
    let links = vectorborne::workbook_links(&html);
    let (s, url) = match season {
        Some(s) => links
            .into_iter()
            .find(|(y, _)| *y == s)
            .ok_or_else(|| PhnError::NotFound(format!("no {s} workbook on DPH's page")))?,
        None => links
            .into_iter()
            .next()
            .ok_or_else(|| PhnError::NotFound("no risk workbook on DPH's page".into()))?,
    };
    let wb = vectorborne::fetch_workbook(&ctx.http, ctx.cache.as_ref(), &url, ttl).await?;
    let tables = read_workbook_bytes(ctx, s, &wb.body)?;
    let (towns, positives, as_of) = parse_workbook(s, &tables)?;
    Ok(ArboSeason {
        season: s,
        as_of: as_of.clone(),
        towns,
        positives,
        mode: wb.mode,
        source: Source {
            title: format!("{s} EEE and WNV risk level and data (DPH arbovirus update workbook)"),
            publisher: "Massachusetts Department of Public Health".into(),
            url: Some(url),
            effective_date: Some(as_of),
            retrieved: Some(phn_data::fetch::local_date(&wb.as_of)),
            mode: wb.mode,
        },
    })
}

fn snapshot_season(ctx: &Context, season: i32) -> Result<Option<ArboSeason>> {
    let (risk, entry) = vectorborne::snapshot_risk(&ctx.snapshots)?;
    let towns: Vec<TownRisk> = risk.into_iter().filter(|r| r.season == season).collect();
    if towns.is_empty() {
        return Ok(None);
    }
    let positives = vectorborne::snapshot_positives(&ctx.snapshots)?
        .into_iter()
        .filter(|p| p.season == season)
        .collect();
    let as_of = towns
        .iter()
        .map(|t| t.as_of.clone())
        .max()
        .unwrap_or_default();
    let mut source = entry.to_source();
    source.title =
        format!("{season} EEE and WNV risk levels by town (bundled snapshot of DPH's workbook)");
    source.effective_date = Some(as_of.clone());
    Ok(Some(ArboSeason {
        season,
        as_of,
        towns,
        positives,
        mode: SourceMode::Snapshot,
        source,
    }))
}

/// Seasons in the snapshot, oldest first.
fn snapshot_seasons(ctx: &Context) -> Result<Vec<i32>> {
    let (risk, _) = vectorborne::snapshot_risk(&ctx.snapshots)?;
    let s: BTreeSet<i32> = risk.iter().map(|r| r.season).collect();
    Ok(s.into_iter().collect())
}

// ---------------------------------------------------------------- helpers

fn in_season(c: &VectorContent, d: NaiveDate) -> bool {
    (c.season.first_month..=c.season.last_month).contains(&d.month())
}

/// The season to show when none is asked for: this year's from June on,
/// otherwise last year's.
fn default_season(c: &VectorContent, d: NaiveDate) -> i32 {
    if d.month() >= c.season.first_month {
        d.year()
    } else {
        d.year() - 1
    }
}

fn towns_in_area<'a>(ctx: &Context, a: &Area, towns: &'a [TownRisk]) -> Vec<&'a TownRisk> {
    let names: BTreeSet<String> = area::towns_in(ctx, a).into_iter().collect();
    towns.iter().filter(|t| names.contains(&t.town)).collect()
}

fn guidance(c: &VectorContent, virus: &str, level: RiskLevel) -> Option<LevelGuidance> {
    let (defs, src, sec, rsrc, rsec) = if virus == "EEE" {
        (
            &c.eee_levels,
            &c.eee_source,
            &c.eee_section,
            &c.eee_response_source,
            &c.eee_response_section,
        )
    } else {
        (
            &c.wnv_levels,
            &c.wnv_source,
            &c.wnv_section,
            &c.wnv_response_source,
            &c.wnv_response_section,
        )
    };
    let d = defs.iter().find(|d| d.level == level.label())?;
    let title = |k: &str| {
        c.sources
            .get(k)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| k.to_string())
    };
    Some(LevelGuidance {
        virus: virus.into(),
        level: d.level.clone(),
        map_color: d.color.clone(),
        meaning: d.meaning.clone(),
        definition: d.definition.clone(),
        public_actions: d.public_actions.clone(),
        dph_response: d.dph_response.clone(),
        citation: format!("{}, {sec}; {}, {rsec}", title(src), title(rsrc)),
    })
}

/// DPH's cancellation window for the week containing `d`.
fn cancellation_for(c: &VectorContent, d: NaiveDate) -> Option<CancellationWindow> {
    let ct = &c.cancellation_times;
    let iso = d.to_string();
    let week = ct
        .weeks
        .iter()
        .find(|w| w.from.as_str() <= iso.as_str() && iso.as_str() <= w.to.as_str())?;
    Some(CancellationWindow {
        season: ct.season,
        week: week.clone(),
        citation: format!(
            "{}, {}",
            c.sources
                .get(&ct.source)
                .map(|s| s.title.as_str())
                .unwrap_or(&ct.source),
            ct.section
        ),
    })
}

fn small_number_flags(cases: Option<u64>) -> Vec<String> {
    match cases {
        Some(0) => vec!["no cases: no rate".into()],
        Some(n) if n < phn_epi::rates::UNRELIABLE_COUNT => {
            vec!["fewer than 20 cases: unstable, interpret with caution".into()]
        }
        _ => Vec::new(),
    }
}

/// DPH's county label for an area ("Dukes & Nantucket" for either island county).
fn dph_county(a: &Area) -> String {
    match a.county.as_deref() {
        Some("Dukes") | Some("Nantucket") => "Dukes & Nantucket".into(),
        Some(c) => c.to_string(),
        None => "Massachusetts".into(),
    }
}

fn tick_years(rows: &[&TickCases]) -> Vec<TickYear> {
    let mut v: Vec<TickYear> = rows
        .iter()
        .map(|r| TickYear {
            year: r.year,
            cases: r.cases,
            rate_per_100k: r.rate_per_100k,
            flags: small_number_flags(r.cases),
        })
        .collect();
    v.sort_by_key(|y| y.year);
    v
}

fn tick_series(cases: &[TickCases], geography: &str) -> Vec<TickDiseaseSeries> {
    let mut diseases: Vec<String> = Vec::new();
    for c in cases {
        if !diseases.contains(&c.disease) {
            diseases.push(c.disease.clone());
        }
    }
    let mut out = Vec::new();
    for d in diseases {
        let area_rows: Vec<&TickCases> = cases
            .iter()
            .filter(|c| c.disease == d && c.area == geography)
            .collect();
        let state_rows: Vec<&TickCases> = cases
            .iter()
            .filter(|c| c.disease == d && c.area == "Massachusetts")
            .collect();
        if area_rows.is_empty() {
            continue;
        }
        let years = tick_years(&area_rows);
        let massachusetts = tick_years(&state_rows);
        let change = match years.as_slice() {
            [.., a, b] => match (a.cases, b.cases) {
                (Some(x), Some(y)) if x > 0 => {
                    Some(((y as f64 - x as f64) / x as f64 * 1000.0).round() / 10.0)
                }
                _ => None,
            },
            _ => None,
        };
        let ratio = years.last().and_then(|l| {
            let s = massachusetts.iter().find(|m| m.year == l.year)?;
            match (l.rate_per_100k, s.rate_per_100k) {
                (Some(a), Some(b)) if b > 0.0 => Some((a / b * 100.0).round() / 100.0),
                _ => None,
            }
        });
        out.push(TickDiseaseSeries {
            disease: d,
            area: geography.to_string(),
            case_classes: area_rows[0].case_classes.clone(),
            years,
            massachusetts,
            change_from_previous_year_pct: change,
            ratio_to_state: if geography == "Massachusetts" {
                None
            } else {
                ratio
            },
        });
    }
    out
}

fn ed_rows(ed: &[TickEdVisits], a: &Area, geography: &str) -> Vec<EdVisitRow> {
    let to_row = |e: &TickEdVisits| EdVisitRow {
        year: e.year,
        through_month: e.through_month.clone(),
        county: e.county.clone(),
        tickborne_disease_visits: e.tickborne_disease_visits,
        ed_visits: e.ed_visits,
        per_10k_ed_visits: e.per_10k_ed_visits,
    };
    if a.level == AreaLevel::State {
        // The latest complete year for every county, highest rate first.
        let latest_full = ed
            .iter()
            .filter(|e| e.through_month.ends_with("-12"))
            .map(|e| e.year)
            .max();
        let mut v: Vec<EdVisitRow> = ed
            .iter()
            .filter(|e| Some(e.year) == latest_full)
            .map(to_row)
            .collect();
        v.sort_by(|x, y| {
            y.per_10k_ed_visits
                .unwrap_or(0.0)
                .total_cmp(&x.per_10k_ed_visits.unwrap_or(0.0))
        });
        v
    } else {
        let mut v: Vec<EdVisitRow> = ed
            .iter()
            .filter(|e| e.county == geography)
            .map(to_row)
            .collect();
        v.sort_by_key(|r| r.year);
        v
    }
}

/// Current-year babesiosis and anaplasmosis counts for Massachusetts (NNDSS).
async fn nndss_counts(ctx: &Context) -> (Vec<NndssCount>, String, Vec<Source>, Vec<String>) {
    let mut out = Vec::new();
    let mut sources = Vec::new();
    let mut caveats = Vec::new();
    let mut status = "unavailable".to_string();
    for (query, label) in [
        ("Babesiosis", "Babesiosis"),
        (
            "Anaplasma phagocytophilum",
            "Anaplasmosis (Anaplasma phagocytophilum infection)",
        ),
    ] {
        let args = crate::cd::NotifiableTrendsArgs {
            condition: Some(query.into()),
            areas: vec!["Massachusetts".into()],
            ..Default::default()
        };
        match crate::cd::get_notifiable_disease_trends(ctx, args).await {
            Ok(env) => {
                let d = &env.data;
                if let Some(t) = d.trends.first() {
                    out.push(NndssCount {
                        condition: label.into(),
                        mmwr_year: d.mmwr_year,
                        mmwr_week: d.mmwr_week,
                        week_ending: d.week_ending.clone(),
                        year_to_date: t.year_to_date.current,
                        note: t.year_to_date.current_note.clone(),
                        five_year_mean: t.year_to_date.baseline_mean,
                        assessment: t.year_to_date.assessment.clone(),
                    });
                }
                for s in env.sources {
                    if status == "unavailable" {
                        status = s.mode.label().to_string();
                    }
                    if !sources.contains(&s) {
                        sources.push(s);
                    }
                }
            }
            Err(e) => caveats.push(format!(
                "Current-year NNDSS counts for {label} could not be read ({e})."
            )),
        }
    }
    (out, status, sources, caveats)
}

fn audiences(list: &[String]) -> Result<Vec<&'static str>> {
    const ALL: [&str; 4] = ["public", "schools", "camps", "outdoor_workers"];
    let mut out: Vec<&'static str> = Vec::new();
    for a in list {
        let n = phn_core::text::normalize(a).replace(' ', "_");
        let pick: &[&'static str] = match n.as_str() {
            "" | "all" => &ALL,
            "public" | "residents" | "general_public" | "families" => &["public"],
            "schools" | "school" | "school_nurses" => &["schools"],
            "camps" | "camp" | "recreational_camps" => &["camps"],
            "outdoor_workers" | "workers" | "outdoor_worker" => &["outdoor_workers"],
            _ => {
                return Err(PhnError::InvalidInput(format!(
                    "unknown audience '{a}': use public, schools, camps, outdoor_workers, or all"
                )));
            }
        };
        for p in pick {
            if !out.contains(p) {
                out.push(p);
            }
        }
    }
    if out.is_empty() {
        out = ALL.to_vec();
    }
    Ok(out)
}

fn audience_label(a: &str) -> &'static str {
    match a {
        "public" => "The public and families",
        "schools" => "Schools",
        "camps" => "Recreational camps",
        _ => "Outdoor workers",
    }
}

fn messages_for<'a>(c: &'a VectorContent, a: &str) -> &'a [Cited] {
    match a {
        "public" => &c.prevention.public,
        "schools" => &c.prevention.schools,
        "camps" => &c.prevention.camps,
        _ => &c.prevention.outdoor_workers,
    }
}

// ---------------------------------------------------------------- the tool

/// Mosquito- and tick-borne disease risk for an area.
pub async fn vector_borne_disease_risk(
    ctx: &Context,
    args: VectorRiskArgs,
) -> Result<Envelope<VectorRiskResult>> {
    let c: VectorContent = content::load(VECTOR_BORNE)?;
    let today = chrono::Local::now().date_naive();
    run(ctx, args, &c, today).await
}

async fn run(
    ctx: &Context,
    args: VectorRiskArgs,
    c: &VectorContent,
    today: NaiveDate,
) -> Result<Envelope<VectorRiskResult>> {
    let area = area::resolve(ctx, args.area.as_deref(), true)?;
    let audiences = audiences(&args.audiences)?;
    let seasons = snapshot_seasons(ctx)?;
    let first = *seasons.first().unwrap_or(&2022);
    if let Some(s) = args.season
        && !(1900..=2100).contains(&s)
    {
        return Err(PhnError::InvalidInput(format!(
            "season {s} is not a valid year"
        )));
    }
    if let Some(s) = args.season
        && (s < first || s > today.year())
    {
        return Err(PhnError::InvalidInput(format!(
            "season must be from {first} to {} (DPH's risk workbooks start in {first})",
            today.year()
        )));
    }
    let in_season_now = in_season(c, today);
    let wanted = args.season.unwrap_or_else(|| default_season(c, today));
    let ttl = if in_season_now {
        vectorborne::IN_SEASON_TTL_SECS
    } else {
        vectorborne::OFF_SEASON_TTL_SECS
    };
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();

    // ---- Arbovirus: live for the latest seasons, else the snapshot.
    let latest_snapshot = *seasons.last().unwrap_or(&first);
    let mut live_error: Option<String> = None;
    let mut data: Option<ArboSeason> = None;
    if wanted >= latest_snapshot {
        match live_season(ctx, Some(wanted), ttl).await {
            Ok(live) => data = Some(live),
            Err(e) => live_error = Some(phn_data::apifetch::reason(&e)),
        }
    }
    if let Some(snap) = snapshot_season(ctx, wanted)? {
        let use_snap = match &data {
            Some(l) => snap.as_of > l.as_of,
            None => true,
        };
        if use_snap {
            data = Some(snap);
        }
    }
    let data = match data {
        Some(d) => d,
        None => {
            // No data for the wanted season (DPH has not posted it, or it could not be
            // reached): fall back to the latest season available.
            let fallback = snapshot_season(ctx, latest_snapshot)?
                .ok_or_else(|| PhnError::Internal("arbovirus snapshot has no seasons".into()))?;
            caveats.push(format!(
                "DPH's {wanted} risk levels are not available{}; showing the final {} levels instead.",
                live_error
                    .as_ref()
                    .map(|e| format!(" ({e})"))
                    .unwrap_or_default(),
                fallback.season
            ));
            fallback
        }
    };
    sources.push(data.source.clone());
    let current = in_season_now
        && data.season == today.year()
        && matches!(data.mode, SourceMode::Live | SourceMode::Cache);
    let season_status = if in_season_now && data.season == today.year() {
        if current {
            format!(
                "The {} arbovirus season (June to October) is under way. DPH's levels below were last updated {} ({}).",
                data.season,
                data.as_of,
                data.mode.label()
            )
        } else {
            format!(
                "The {} arbovirus season is under way, but DPH's live levels could not be reached{}. \
                 The levels below are from the bundled copy dated {} and may be out of date: check DPH's \
                 arbovirus update page.",
                data.season,
                live_error
                    .as_ref()
                    .map(|e| format!(" ({e})"))
                    .unwrap_or_default(),
                data.as_of
            )
        }
    } else if in_season_now {
        format!(
            "The {} arbovirus season is under way, but DPH's {} levels were not available. The \
             levels below are the FINAL {} levels (as of {}), not current risk.",
            today.year(),
            today.year(),
            data.season,
            data.as_of
        )
    } else if data.season == today.year() {
        format!(
            "The {} arbovirus season (June to October) is over. These are DPH's last {} levels, as \
             of {}; they are not current risk. Risk from mosquito-borne disease is virtually \
             eliminated by the first local hard frost. DPH posts new levels when testing starts in June.",
            data.season, data.season, data.as_of
        )
    } else if data.season == today.year() - 1 && today.month() < c.season.first_month {
        format!(
            "The {} arbovirus season has not started (DPH begins testing and posting risk levels in \
             June). These are the final {} levels, as of {}; they are not current risk.",
            today.year(),
            data.season,
            data.as_of
        )
    } else {
        format!(
            "These are DPH's final {} levels, as of {} (you asked for an earlier season); they are \
             not current risk.",
            data.season, data.as_of
        )
    };
    if !in_season_now
        && let Some(e) = &live_error
        && data.mode == SourceMode::Snapshot
    {
        caveats.push(format!(
            "DPH's arbovirus page could not be reached ({e}); the bundled copy was used. Outside \
             the season the levels do not change."
        ));
    }

    let area_towns = towns_in_area(ctx, &area, &data.towns);
    let to_levels = |t: &TownRisk| TownLevels {
        town: t.town.clone(),
        county: t.county.clone(),
        eee: t.eee.label().into(),
        wnv: t.wnv.label().into(),
    };
    let town = if area.level == AreaLevel::Town {
        area_towns.first().map(|t| to_levels(t))
    } else {
        None
    };
    if area.level == AreaLevel::Town && town.is_none() {
        caveats.push(format!(
            "{} is not in DPH's {} risk table.",
            area.name, data.season
        ));
    }
    let towns_by_level: Vec<LevelCount> = RiskLevel::ALL
        .iter()
        .map(|l| LevelCount {
            level: l.label().into(),
            eee_towns: area_towns.iter().filter(|t| t.eee == *l).count(),
            wnv_towns: area_towns.iter().filter(|t| t.wnv == *l).count(),
        })
        .collect();
    let mut elevated: Vec<&TownRisk> = area_towns
        .iter()
        .copied()
        .filter(|t| t.eee >= RiskLevel::Moderate || t.wnv >= RiskLevel::High)
        .collect();
    elevated.sort_by(|a, b| {
        b.eee
            .cmp(&a.eee)
            .then(b.wnv.cmp(&a.wnv))
            .then(a.town.cmp(&b.town))
    });
    let elevated_total = elevated.len();
    let elevated_towns: Vec<TownLevels> = elevated
        .iter()
        .take(MAX_TOWNS_LISTED)
        .map(|t| to_levels(t))
        .collect();
    let highest_eee = area_towns
        .iter()
        .map(|t| t.eee)
        .max()
        .unwrap_or(RiskLevel::Remote);
    let highest_wnv = area_towns
        .iter()
        .map(|t| t.wnv)
        .max()
        .unwrap_or(RiskLevel::Low);
    let guidance: Vec<LevelGuidance> = [
        guidance(c, "EEE", highest_eee),
        guidance(c, "WNV", highest_wnv),
    ]
    .into_iter()
    .flatten()
    .collect();

    // Positive findings in the area.
    let names: BTreeSet<String> = area_towns.iter().map(|t| t.town.clone()).collect();
    let counties: BTreeSet<String> = match area.level {
        AreaLevel::State => BTreeSet::new(),
        _ => area.county.iter().cloned().collect(),
    };
    let mut groups: BTreeMap<(String, String), (u32, BTreeSet<String>, Option<String>)> =
        BTreeMap::new();
    for p in &data.positives {
        let human = p.sample_type == "Human";
        let inside = match area.level {
            AreaLevel::State => true,
            AreaLevel::County => counties.contains(&p.county),
            // Human cases: the town's county.
            AreaLevel::Town => {
                if human {
                    counties.contains(&p.county)
                } else {
                    p.town.as_ref().is_some_and(|t| names.contains(t))
                }
            }
        };
        if !inside {
            continue;
        }
        let e = groups
            .entry((p.virus.clone(), p.sample_type.clone()))
            .or_insert((0, BTreeSet::new(), None));
        e.0 += p.count;
        if let Some(t) = &p.town {
            e.1.insert(t.clone());
        }
        if p.latest_date > e.2 {
            e.2 = p.latest_date.clone();
        }
    }
    let positives: Vec<PositiveSummary> = groups
        .into_iter()
        .map(|((virus, sample_type), (count, towns, latest))| {
            let scope = match (area.level, sample_type.as_str()) {
                (AreaLevel::Town, "Human") => {
                    format!(
                        "{} County (county of residence)",
                        area.county.clone().unwrap_or_default()
                    )
                }
                _ => area.name.clone(),
            };
            PositiveSummary {
                virus,
                towns: if sample_type == "Human" {
                    0
                } else {
                    towns.len()
                },
                sample_type,
                count,
                latest_date: latest,
                scope,
            }
        })
        .collect();

    // Earlier seasons from the snapshot.
    let (all_risk, _) = vectorborne::snapshot_risk(&ctx.snapshots)?;
    let mut history = Vec::new();
    for s in seasons.iter().rev().copied().filter(|s| *s != data.season) {
        let rows: Vec<TownRisk> = all_risk.iter().filter(|r| r.season == s).cloned().collect();
        let in_area = towns_in_area(ctx, &area, &rows);
        if in_area.is_empty() {
            continue;
        }
        history.push(SeasonHistory {
            season: s,
            as_of: in_area
                .iter()
                .map(|t| t.as_of.clone())
                .max()
                .unwrap_or_default(),
            highest_eee: in_area
                .iter()
                .map(|t| t.eee)
                .max()
                .unwrap_or(RiskLevel::Remote)
                .label()
                .into(),
            highest_wnv: in_area
                .iter()
                .map(|t| t.wnv)
                .max()
                .unwrap_or(RiskLevel::Low)
                .label()
                .into(),
            towns_eee_moderate_or_higher: in_area
                .iter()
                .filter(|t| t.eee >= RiskLevel::Moderate)
                .count(),
            towns_wnv_moderate_or_higher: in_area
                .iter()
                .filter(|t| t.wnv >= RiskLevel::Moderate)
                .count(),
        });
    }

    let cancellation = if current && highest_eee >= RiskLevel::High {
        let w = cancellation_for(c, today);
        if w.is_none() {
            caveats.push(format!(
                "EEE risk is {} in {}; DPH's recommended cancellation times for outdoor evening \
                 activities are in Appendix 2 of its current Arbovirus Surveillance and Response \
                 Plan (this server has the {} schedule only).",
                highest_eee.label().to_lowercase(),
                area.name,
                c.cancellation_times.season
            ));
        }
        w
    } else {
        None
    };

    let arbovirus = ArbovirusSummary {
        season: data.season,
        as_of: data.as_of.clone(),
        data_mode: data.mode.label().to_string(),
        current,
        town,
        towns_by_level,
        elevated_towns,
        elevated_towns_not_listed: elevated_total.saturating_sub(MAX_TOWNS_LISTED),
        highest_eee: highest_eee.label().into(),
        highest_wnv: highest_wnv.label().into(),
        guidance,
        positives,
        history,
        cancellation,
    };

    // ---- Tick-borne disease.
    let (cases, ed, tick_entry) = vectorborne::snapshot_tickborne(&ctx.snapshots)?;
    let geography = dph_county(&area);
    let diseases = tick_series(&cases, &geography);
    let statewide_ten_year: Vec<StateTotal> = cases
        .iter()
        .filter(|r| r.area == "Massachusetts (ten-year table)")
        .map(|r| StateTotal {
            year: r.year,
            disease: r.disease.clone(),
            cases: r.cases,
            case_classes: r.case_classes.clone(),
        })
        .collect();
    let ed_visits = ed_rows(&ed, &area, &geography);
    let (current_year, current_year_status, nn_sources, nn_caveats) = nndss_counts(ctx).await;
    caveats.extend(nn_caveats);
    let tickborne = TickborneSummary {
        geography: geography.clone(),
        diseases,
        statewide_ten_year,
        ed_visits,
        current_year,
        current_year_status,
    };
    sources.push(tick_entry.to_source());
    sources.extend(nn_sources);

    // ---- Prevention.
    let prevention: Vec<AudienceMessages> = audiences
        .iter()
        .map(|a| AudienceMessages {
            audience: (*a).to_string(),
            messages: content::cite_all(messages_for(c, a), &c.sources),
        })
        .collect();
    let disease_notes = content::cite_all(&c.disease_notes, &c.sources);

    // ---- Notice.
    let mut files_written = Vec::new();
    let mut notice_file = None;
    if args.notice.unwrap_or(false) {
        let doc = notice_document(
            ctx,
            c,
            &area,
            &season_status,
            &arbovirus,
            &tickborne,
            audiences[0],
            today,
        );
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let slug: String = area
            .name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let path = sandbox.new_output_file(&format!("vector_borne_notice_{slug}.docx"))?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        files_written.push(disp.clone());
        notice_file = Some(disp);
    }

    // ---- Summary.
    let s = summary(
        &area,
        &season_status,
        &arbovirus,
        &tickborne,
        &prevention,
        &notice_file,
    );

    // ---- Caveats and sources.
    caveats.push(c.season.focal_areas.text.trim().to_string());
    if area.level == AreaLevel::Town {
        caveats.push(format!(
            "DPH publishes tick-borne disease cases by county of residence only; figures shown \
             are for {geography}."
        ));
    }
    if geography == "Dukes & Nantucket" {
        caveats.push("DPH reports Dukes and Nantucket counties together.".into());
    }
    for n in &c.data_notes {
        caveats.push(n.text.trim().to_string());
    }
    caveats.push(
        "Tick-borne disease rates are DPH's, per 100,000 residents, as published; counts under 20 \
         give unstable rates, and large year-to-year changes can reflect reporting changes."
            .into(),
    );
    if tickborne.current_year_status == "unavailable" {
        caveats.push(
            "Current-year babesiosis and anaplasmosis counts (NNDSS) could not be read.".into(),
        );
    } else {
        caveats.push(
            "NNDSS counts are provisional, statewide, and by MMWR week; they are not comparable \
             with DPH's annual summaries."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[VECTOR_BORNE]));
    let mut keys: Vec<&str> = vec![
        c.eee_source.as_str(),
        c.eee_response_source.as_str(),
        c.season.description.source.as_str(),
    ];
    for a in &audiences {
        for m in messages_for(c, a) {
            keys.push(m.source.as_str());
        }
    }
    for n in c.disease_notes.iter().chain(&c.data_notes) {
        keys.push(n.source.as_str());
    }
    sources.extend(content::sources_for(keys, &c.sources));

    Ok(Envelope::new(
        s,
        VectorRiskResult {
            area,
            today: today.to_string(),
            in_season: in_season_now,
            season_status,
            arbovirus,
            tickborne,
            prevention,
            disease_notes,
            notice_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        files_written,
        notes: vec![
            "Human arbovirus cases are shown only as counts by county, as DPH publishes them."
                .into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

fn fmt_rate(r: Option<f64>) -> String {
    r.map(|v| md::num(v, 1)).unwrap_or_else(|| "—".into())
}

fn summary(
    area: &Area,
    status: &str,
    a: &ArbovirusSummary,
    t: &TickborneSummary,
    prevention: &[AudienceMessages],
    notice: &Option<String>,
) -> String {
    let mut s = format!("**Mosquito- and tick-borne disease — {}**\n\n", area.name);
    s.push_str(&format!("{status}\n\n"));
    if let Some(town) = &a.town {
        s.push_str(&format!(
            "**{} ({} County), {} season:** EEE risk **{}**, West Nile virus risk **{}**.\n\n",
            town.town, town.county, a.season, town.eee, town.wnv
        ));
    } else {
        let rows: Vec<Vec<String>> = a
            .towns_by_level
            .iter()
            .filter(|l| l.eee_towns + l.wnv_towns > 0)
            .map(|l| {
                vec![
                    l.level.clone(),
                    l.eee_towns.to_string(),
                    l.wnv_towns.to_string(),
                ]
            })
            .collect();
        s.push_str(&format!(
            "Towns in {} by risk level ({} season):\n\n",
            area.name, a.season
        ));
        s.push_str(&md::table(&["Level", "EEE", "West Nile virus"], &rows));
        s.push('\n');
        if !a.elevated_towns.is_empty() {
            let list: Vec<String> = a
                .elevated_towns
                .iter()
                .map(|t| format!("{} (EEE {}, WNV {})", t.town, t.eee, t.wnv))
                .collect();
            s.push_str(&format!(
                "Towns at EEE moderate or higher or WNV high or higher: {}{}.\n\n",
                list.join("; "),
                if a.elevated_towns_not_listed > 0 {
                    format!("; and {} more", a.elevated_towns_not_listed)
                } else {
                    String::new()
                }
            ));
        }
    }
    for g in &a.guidance {
        s.push_str(&format!(
            "**{} {} ({})** — {}\n",
            g.virus, g.level, g.map_color, g.meaning
        ));
        s.push_str(&format!(
            "- What residents can do: {}\n",
            g.public_actions.join(" ")
        ));
        s.push_str(&format!(
            "- DPH's plan at this level: {}\n\n",
            g.dph_response.join(" ")
        ));
    }
    if let Some(w) = &a.cancellation {
        s.push_str(&format!(
            "**DPH recommended cancellation window this week ({} to {}): {} to {}** — move or \
             reschedule outdoor evening activities.\n\n",
            w.week.from, w.week.to, w.week.start, w.week.end
        ));
    }
    if !a.positives.is_empty() {
        let rows: Vec<Vec<String>> = a
            .positives
            .iter()
            .map(|p| {
                vec![
                    p.virus.clone(),
                    p.sample_type.clone(),
                    p.count.to_string(),
                    if p.sample_type == "Human" {
                        "—".into()
                    } else {
                        p.towns.to_string()
                    },
                    p.latest_date.clone().unwrap_or_else(|| "—".into()),
                    p.scope.clone(),
                ]
            })
            .collect();
        s.push_str(&format!("Positive findings, {} season:\n\n", a.season));
        s.push_str(&md::table(
            &["Virus", "Type", "Count", "Towns", "Latest", "Where"],
            &rows,
        ));
        s.push('\n');
    } else {
        s.push_str(&format!(
            "No positive mosquito samples, animal cases, or human cases in {} in the {} season.\n\n",
            area.name, a.season
        ));
    }
    if !a.history.is_empty() {
        let h: Vec<String> = a
            .history
            .iter()
            .map(|h| format!("{} EEE {} / WNV {}", h.season, h.highest_eee, h.highest_wnv))
            .collect();
        s.push_str(&format!(
            "Earlier seasons (highest final level in the area): {}.\n\n",
            h.join("; ")
        ));
    }
    s.push_str(&format!("**Tick-borne disease — {}**\n\n", t.geography));
    let mut rows = Vec::new();
    for d in &t.diseases {
        for y in &d.years {
            let st = d.massachusetts.iter().find(|m| m.year == y.year);
            rows.push(vec![
                d.disease.clone(),
                y.year.to_string(),
                y.cases
                    .map(|n| md::num(n as f64, 0))
                    .unwrap_or_else(|| "—".into()),
                fmt_rate(y.rate_per_100k),
                fmt_rate(st.and_then(|m| m.rate_per_100k)),
                y.flags.join("; "),
            ]);
        }
    }
    s.push_str(&md::table(
        &[
            "Disease",
            "Year",
            "Cases",
            "Rate per 100,000",
            "Massachusetts rate",
            "Note",
        ],
        &rows,
    ));
    s.push_str(
        "\nLyme disease counts are probable cases (case definition changed in 2022); babesiosis \
         and anaplasmosis are confirmed and probable. Rates are DPH's.\n",
    );
    if let Some(last) = t
        .ed_visits
        .iter()
        .max_by_key(|e| (e.year, e.through_month.clone()))
    {
        if t.geography == "Massachusetts" {
            s.push_str(&format!(
                "\nEmergency department visits for tick-borne disease, {} (highest rate first): {}.\n",
                last.year,
                t.ed_visits
                    .iter()
                    .take(5)
                    .map(|e| format!("{} {} per 10,000 ED visits", e.county, fmt_rate(e.per_10k_ed_visits)))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        } else {
            let series: Vec<String> = t
                .ed_visits
                .iter()
                .map(|e| {
                    format!(
                        "{}{}: {} ({} per 10,000)",
                        e.year,
                        if e.through_month.ends_with("-12") {
                            String::new()
                        } else {
                            format!(" through {}", e.through_month)
                        },
                        e.tickborne_disease_visits,
                        fmt_rate(e.per_10k_ed_visits)
                    )
                })
                .collect();
            s.push_str(&format!(
                "\nEmergency department visits with a tick-borne disease diagnosis ({}): {}.\n",
                t.geography,
                series.join("; ")
            ));
        }
    }
    if !t.current_year.is_empty() {
        let v: Vec<String> = t
            .current_year
            .iter()
            .map(|n| match n.year_to_date {
                Some(x) => format!(
                    "{} {} so far in {} (through MMWR week {}, {}; 5-year mean {}; {})",
                    n.condition,
                    md::num(x, 0),
                    n.mmwr_year,
                    n.mmwr_week,
                    n.week_ending,
                    n.five_year_mean
                        .map(|m| md::num(m, 0))
                        .unwrap_or_else(|| "—".into()),
                    n.assessment
                ),
                None => format!(
                    "{}: no {} count for Massachusetts in NNDSS through MMWR week {} ({})",
                    n.condition,
                    n.mmwr_year,
                    n.mmwr_week,
                    n.note.clone().unwrap_or_else(|| "not reported".into())
                ),
            })
            .collect();
        s.push_str(&format!(
            "\nMassachusetts this year (NNDSS, provisional, {}): {}.\n",
            t.current_year_status,
            v.join("; ")
        ));
    }
    s.push_str("\n**Prevention messages**\n");
    for a in prevention {
        s.push_str(&format!("\n_{}_\n", audience_label(&a.audience)));
        for m in &a.messages {
            s.push_str(&content::bullet(m));
        }
    }
    if let Some(f) = notice {
        s.push_str(&format!("\nOne-page notice written to `{f}`.\n"));
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn notice_document(
    ctx: &Context,
    c: &VectorContent,
    area: &Area,
    status: &str,
    a: &ArbovirusSummary,
    t: &TickborneSummary,
    audience: &str,
    today: NaiveDate,
) -> Document {
    let p = |x: &str| Block::Paragraph(vec![Inline::plain(x)]);
    let h = |x: &str| Block::Heading {
        level: 2,
        text: x.to_string(),
    };
    let list = |items: Vec<String>| Block::List {
        ordered: false,
        items: items.into_iter().map(|i| vec![Inline::plain(i)]).collect(),
    };
    let mut blocks = vec![p(&format!("Prepared {today}. {status}"))];
    blocks.push(h("Mosquito-borne disease risk"));
    let mut rows = Vec::new();
    for g in &a.guidance {
        rows.push(vec![
            if g.virus == "EEE" {
                "Eastern equine encephalitis (EEE)".to_string()
            } else {
                "West Nile virus (WNV)".to_string()
            },
            format!("{} ({})", g.level, g.map_color),
            g.meaning.clone(),
        ]);
    }
    blocks.push(Block::Table(TableBlock {
        caption: Some(format!(
            "DPH risk level{}, {} season, as of {}",
            if a.town.is_some() {
                ""
            } else {
                " (highest in the area)"
            },
            a.season,
            a.as_of
        )),
        headers: vec!["Virus".into(), "Risk level".into(), "What it means".into()],
        rows,
    }));
    blocks.push(h("What to do"));
    let mut actions: Vec<String> = Vec::new();
    for g in &a.guidance {
        for x in &g.public_actions {
            if !actions.contains(x) {
                actions.push(x.clone());
            }
        }
    }
    blocks.push(list(actions));
    if let Some(w) = &a.cancellation {
        blocks.push(p(&format!(
            "DPH recommends moving or rescheduling outdoor activities between {} and {} this week ({} to {}).",
            w.week.start, w.week.end, w.week.from, w.week.to
        )));
    }
    blocks.push(h(&format!(
        "{} — preventing bites",
        audience_label(audience)
    )));
    blocks.push(list(
        messages_for(c, audience)
            .iter()
            .map(|m| m.text.trim().to_string())
            .collect(),
    ));
    blocks.push(h(&format!("Tick-borne disease in {}", t.geography)));
    let tick_lines: Vec<String> = t
        .diseases
        .iter()
        .filter_map(|d| {
            let y = d.years.last()?;
            Some(format!(
                "{} {}: {} cases ({} per 100,000; Massachusetts {}).",
                d.disease,
                y.year,
                y.cases
                    .map(|n| md::num(n as f64, 0))
                    .unwrap_or_else(|| "—".into()),
                fmt_rate(y.rate_per_100k),
                fmt_rate(
                    d.massachusetts
                        .iter()
                        .find(|m| m.year == y.year)
                        .and_then(|m| m.rate_per_100k)
                )
            ))
        })
        .collect();
    blocks.push(list(tick_lines));
    blocks.push(p(
        "Sources: Massachusetts Department of Public Health (arbovirus update, Arbovirus \
         Surveillance and Response Plan, tick-borne disease surveillance summaries, prevention \
         pages); CDC; NIOSH. Decision support only: follow DPH and your local board of health.",
    ));
    Document {
        title: format!("Mosquito- and tick-borne disease notice — {}", area.name),
        subtitle: Some(format!("{} season", a.season)),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn content() -> VectorContent {
        content::load(VECTOR_BORNE).unwrap()
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn content_loads_and_cites_known_sources() {
        let c = content();
        assert_eq!(c.eee_levels.len(), 5);
        assert_eq!(c.wnv_levels.len(), 4);
        for l in c.eee_levels.iter().chain(&c.wnv_levels) {
            assert!(RiskLevel::parse(&l.level).is_some(), "{}", l.level);
            assert!(!l.public_actions.is_empty() && !l.dph_response.is_empty());
        }
        let all = c
            .prevention
            .public
            .iter()
            .chain(&c.prevention.schools)
            .chain(&c.prevention.camps)
            .chain(&c.prevention.outdoor_workers)
            .chain(&c.disease_notes)
            .chain(&c.data_notes)
            .chain([
                &c.season.description,
                &c.season.hard_frost,
                &c.season.peak,
                &c.season.focal_areas,
            ]);
        for m in all {
            assert!(c.sources.contains_key(&m.source), "{}", m.source);
        }
        for k in [
            &c.eee_source,
            &c.wnv_source,
            &c.eee_response_source,
            &c.wnv_response_source,
            &c.cancellation_times.source,
        ] {
            assert!(c.sources.contains_key(k), "{k}");
        }
        // Weeks are contiguous and in order.
        for w in c.cancellation_times.weeks.windows(2) {
            let next = date(&w[0].to) + chrono::Duration::days(1);
            assert_eq!(next, date(&w[1].from));
        }
    }

    #[test]
    fn seasons_and_cancellation_windows() {
        let c = content();
        assert!(in_season(&c, date("2026-08-15")));
        assert!(!in_season(&c, date("2026-11-11")));
        assert!(!in_season(&c, date("2027-03-01")));
        assert_eq!(default_season(&c, date("2026-11-11")), 2026);
        assert_eq!(default_season(&c, date("2027-03-01")), 2026);
        assert_eq!(default_season(&c, date("2027-06-02")), 2027);
        let w = cancellation_for(&c, date("2026-08-19")).unwrap();
        assert_eq!(
            (w.week.start.as_str(), w.week.end.as_str()),
            ("7:45 PM", "6:00 AM")
        );
        assert!(cancellation_for(&c, date("2026-11-11")).is_none());
    }

    #[test]
    fn audiences_are_checked() {
        assert_eq!(audiences(&[]).unwrap().len(), 4);
        assert_eq!(
            audiences(&["Schools".into(), "camp".into()]).unwrap(),
            vec!["schools", "camps"]
        );
        assert!(audiences(&["pilots".into()]).is_err());
    }

    #[test]
    fn small_numbers_are_flagged() {
        assert!(small_number_flags(Some(6))[0].contains("unstable"));
        assert!(small_number_flags(Some(0))[0].contains("no rate"));
        assert!(small_number_flags(Some(20)).is_empty());
    }

    #[test]
    fn workbook_tables_parse_like_the_snapshot() {
        use phn_local::table::Table;
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let mut rows: Vec<Vec<String>> = (0..349)
            .map(|i| s(&[&format!("Town{i}"), "Worcester", "Low", "Moderate", "46330"]))
            .collect();
        rows.push(s(&["Manchester", "Essex", "Remote", "Low ", "46330"]));
        rows.push(s(&["Amherst", "Hampshire", "High", "Moderate", "46329"]));
        let risk = Table {
            name: "Risk by Town".into(),
            headers: s(&[
                "Town",
                "County",
                "EEE Risk Level",
                "WNV Risk Level",
                "Last updated",
            ]),
            rows,
            truncated: false,
        };
        let pos = Table {
            name: "Positive Samples and Cases".into(),
            headers: s(&[
                "Sample/Case",
                "Virus",
                "Sample Type",
                "County",
                "Community",
                "Date",
                "Mosquito Species",
                "Age Range (Human)",
                "Gender (Human)",
                "Clinical Presentation (Human)",
            ]),
            rows: vec![
                s(&[
                    "1",
                    "WNV",
                    "Mosquito",
                    "Hampshire",
                    "Amherst",
                    "46200",
                    "Culex",
                    "",
                    "",
                    "",
                ]),
                s(&[
                    "2",
                    "WNV",
                    "Mosquito ",
                    "Hampshire",
                    "Amherst",
                    "46210",
                    "Culex",
                    "",
                    "",
                    "",
                ]),
                s(&[
                    "3",
                    "EEE",
                    "Human",
                    "Hampshire",
                    "",
                    "46220",
                    "",
                    "60-69",
                    "Male",
                    "Encephalitis",
                ]),
            ],
            truncated: false,
        };
        let (towns, positives, as_of) = parse_workbook(2026, &[risk, pos]).unwrap();
        assert_eq!(as_of, "2026-11-04");
        assert_eq!(towns.len(), 351);
        assert!(
            towns
                .iter()
                .any(|t| t.town == "Manchester-by-the-Sea" && t.wnv == RiskLevel::Low)
        );
        let m = positives
            .iter()
            .find(|p| p.sample_type == "Mosquito")
            .unwrap();
        assert_eq!((m.count, m.latest_date.as_deref()), (2, Some("2026-07-07")));
        let h = positives.iter().find(|p| p.sample_type == "Human").unwrap();
        assert!(h.town.is_none() && h.latest_date.is_none());
        assert!(parse_workbook(2026, &[]).is_err());
    }

    #[tokio::test]
    async fn offline_town_after_the_season_uses_the_dated_snapshot() {
        let (dir, ctx) = ctx();
        let c = content();
        let env = run(
            &ctx,
            VectorRiskArgs {
                area: Some("Middleborough".into()),
                audiences: vec!["schools".into()],
                notice: Some(true),
                ..Default::default()
            },
            &c,
            date("2026-11-11"),
        )
        .await
        .unwrap();
        let d = &env.data;
        assert!(!d.in_season);
        assert_eq!(d.arbovirus.season, 2026);
        assert_eq!(d.arbovirus.data_mode, "snapshot");
        assert!(!d.arbovirus.current);
        assert!(d.season_status.contains("is over"));
        assert!(d.season_status.contains("2026-11-04"));
        assert!(d.arbovirus.town.is_some());
        assert_eq!(d.arbovirus.guidance.len(), 2);
        assert_eq!(d.tickborne.geography, "Plymouth");
        let lyme = d
            .tickborne
            .diseases
            .iter()
            .find(|x| x.disease == "Lyme disease")
            .unwrap();
        assert!(
            lyme.years
                .iter()
                .any(|y| y.year == 2025 && y.cases == Some(1589))
        );
        assert!(!d.tickborne.ed_visits.is_empty());
        assert_eq!(d.prevention.len(), 1);
        assert_eq!(d.prevention[0].audience, "schools");
        let f = d.notice_file.as_ref().unwrap();
        assert!(
            dir.path()
                .join("data/output")
                .join(f.rsplit('/').next().unwrap())
                .exists(),
            "{f}"
        );
        assert!(env.notice.is_some());
        assert!(env.summary.contains("is over"));
        // NNDSS falls back to its bundled snapshot offline.
        assert_eq!(d.tickborne.current_year_status, "snapshot");
        assert!(
            d.tickborne
                .current_year
                .iter()
                .any(|n| n.condition == "Babesiosis")
        );
    }

    #[tokio::test]
    async fn offline_state_and_county_summaries() {
        let (_dir, ctx) = ctx();
        let c = content();
        let env = run(
            &ctx,
            VectorRiskArgs {
                area: Some("Massachusetts".into()),
                season: Some(2024),
                ..Default::default()
            },
            &c,
            date("2026-11-11"),
        )
        .await
        .unwrap();
        let a = &env.data.arbovirus;
        assert_eq!(a.season, 2024);
        let total: usize = a.towns_by_level.iter().map(|l| l.eee_towns).sum();
        assert_eq!(total, 351);
        assert!(a.elevated_towns.len() <= MAX_TOWNS_LISTED);
        assert!(
            a.positives
                .iter()
                .any(|p| p.virus == "EEE" && p.sample_type == "Human")
        );
        assert!(env.data.season_status.contains("earlier season"));
        assert_eq!(env.data.tickborne.geography, "Massachusetts");
        assert_eq!(env.data.tickborne.ed_visits.len(), 13);
        // Dukes County maps to DPH's combined island figures.
        let env = run(
            &ctx,
            VectorRiskArgs {
                area: Some("Dukes County".into()),
                ..Default::default()
            },
            &c,
            date("2027-03-15"),
        )
        .await
        .unwrap();
        assert_eq!(env.data.tickborne.geography, "Dukes & Nantucket");
        assert!(env.data.season_status.contains("has not started"));
        // Bad input.
        for args in [
            VectorRiskArgs {
                season: Some(2019),
                ..Default::default()
            },
            VectorRiskArgs {
                season: Some(20260),
                ..Default::default()
            },
            VectorRiskArgs {
                area: Some("Springfeld".into()),
                ..Default::default()
            },
        ] {
            assert!(run(&ctx, args, &c, date("2026-11-11")).await.is_err());
        }
    }

    #[tokio::test]
    async fn in_season_without_live_data_says_the_levels_may_be_out_of_date() {
        let (_dir, ctx) = ctx();
        let c = content();
        let env = run(
            &ctx,
            VectorRiskArgs {
                area: Some("Boston".into()),
                ..Default::default()
            },
            &c,
            date("2026-08-20"),
        )
        .await
        .unwrap();
        assert!(env.data.in_season);
        assert!(!env.data.arbovirus.current);
        assert!(env.data.season_status.contains("may be out of date"));
        // The next season before DPH posts anything: last season's final levels.
        let env = run(&ctx, VectorRiskArgs::default(), &c, date("2027-06-10"))
            .await
            .unwrap();
        assert_eq!(env.data.arbovirus.season, 2026);
        assert!(env.data.season_status.contains("FINAL 2026"));
    }
}
