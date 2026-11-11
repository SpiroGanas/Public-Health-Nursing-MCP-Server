//! ENV-04 `environmental_justice_screen`: the Massachusetts 2020 Environmental
//! Justice (EJ) block groups in a town, county, or the state, by criterion,
//! with the people living in them, English isolation (language access),
//! Social Vulnerability Index context, what an EJ designation changes (MEPA,
//! EEA policy, DPH's vulnerable health criteria), and an optional summary of
//! an archived EPA EJScreen file the nurse saves in /data. Optional map
//! (SVG/PNG) and block group table (XLSX) go to /data/output.

use std::collections::{BTreeMap, BTreeSet};

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::boundaries::{BoundaryLayer, Feature, Layer, decode_geometry};
use phn_data::census::{self, AcsSnapshot, GeoLevel};
use phn_data::ejclimate::{EjBlockGroup, EjLayer};
use phn_data::geography::MaGeographies;
use phn_data::svi::SviSnapshot;
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epi::choropleth::render::{Fill, LegendRow, MapSpec, render};
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;

/// Content id of `content/env/environmental_justice.yaml`.
pub const EJ_CONTENT: &str = "env-environmental-justice";

/// Highest EJScreen file size read (the national file is far larger; save
/// the Massachusetts rows).
const EJSCREEN_MAX_BYTES: u64 = 150 * 1024 * 1024;

/// Most towns listed in the result for a county or the state (the table has all).
const MAX_TOWNS_LISTED: usize = 25;

// ------------------------------------------------------------------ content

#[derive(Debug, Clone, Deserialize)]
struct Criterion {
    code: String,
    label: String,
    statute: String,
    data: String,
    note: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StateBlockGroups {
    count: u32,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct VhCriterion {
    name: String,
    criterion: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DphVulnerableHealth {
    intro: String,
    criteria: Vec<VhCriterion>,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct EjscreenIndicator {
    column: String,
    label: String,
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EjscreenContent {
    status: String,
    #[allow(dead_code)]
    status_confidence: Confidence,
    where_to_get: String,
    id_column: String,
    population_column: String,
    indicators: Vec<EjscreenIndicator>,
    indicators_source: String,
    filter: Cited,
    percentile_note: Cited,
}

/// `content/env/environmental_justice.yaml`.
#[derive(Debug, Clone, Deserialize)]
struct EjContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, DocSource>,
    criteria: Vec<Criterion>,
    criteria_source: String,
    criteria_section: String,
    neighborhood_definition: Cited,
    secretary_designation: Cited,
    layer_facts: Vec<Cited>,
    state_block_groups: StateBlockGroups,
    what_designation_means: Vec<Cited>,
    language_access: Vec<Cited>,
    dph_vulnerable_health: DphVulnerableHealth,
    how_to_use: Vec<Cited>,
    ejscreen: EjscreenContent,
    eji: Cited,
}

fn cite(c: &Cited, sources: &BTreeMap<String, DocSource>) -> CitedOut {
    content::cite_all(std::slice::from_ref(c), sources)
        .pop()
        .expect("one item")
}

// ------------------------------------------------------------------ arguments and result

/// Arguments for `environmental_justice_screen`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct EjScreenArgs {
    /// A Massachusetts city or town, a county ("Hampden County"), or "Massachusetts". Default:
    /// your saved town, or the whole state when no town is saved.
    pub area: Option<String>,
    /// An archived EPA EJScreen 2.3 (2024) block group CSV saved in /data (for example
    /// EJScreen_2024_BG_StatePct_with_AS_CNMI_GU_VI.csv, ideally only the Massachusetts rows).
    /// The tool summarizes its percentile columns for the area's block groups; rows are never
    /// returned.
    pub ejscreen_path: Option<String>,
    /// Write a map of the EJ block groups (SVG and PNG) to /data/output/maps (default false).
    pub map: Option<bool>,
    /// Write the area's EJ block groups as a table (XLSX) to /data/output (default false).
    pub table: Option<bool>,
}

/// One EJ criterion or combination of criteria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CriteriaCount {
    /// "M", "I", "E", or a combination such as "MI" (by combination) or the single letter (any).
    pub code: String,
    pub label: String,
    pub block_groups: u32,
    /// 2020 Census population of those block groups.
    pub people: f64,
}

/// EEA's official municipal EJ statistics for a town (November 2022 update).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OfficialTownStats {
    pub criteria: String,
    pub ej_block_groups: Option<u32>,
    pub total_block_groups: Option<u32>,
    pub population_in_ej_block_groups: Option<f64>,
    pub total_population: Option<f64>,
    pub percent_population_in_ej_block_groups: Option<f64>,
}

/// The area's EJ picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EjAreaSummary {
    /// Distinct EJ block groups in the area.
    pub ej_block_groups: u32,
    /// All block groups in the area, when known (EEA statistics for a town; 4,985 statewide).
    pub total_block_groups: Option<u32>,
    /// People living in EJ block groups (2020 Census). For a town this is EEA's figure.
    pub people_in_ej_block_groups: f64,
    /// 2020 Census population of the area.
    pub population_2020: f64,
    pub percent_of_residents: Option<f64>,
    /// Cities and towns in the area with at least one EJ block group.
    pub towns_with_ej_block_groups: u32,
    pub towns_in_area: u32,
}

/// English isolation and language access.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnglishIsolation {
    /// EJ block groups that meet the English isolation criterion.
    pub block_groups: u32,
    pub people: f64,
    /// Limited English speaking households in those block groups (ACS 2016–2020).
    pub limited_english_households: f64,
    /// Limited English speaking households in all of the area's EJ block groups.
    pub limited_english_households_in_ej_block_groups: f64,
    /// Percent of households that are limited English speaking (current ACS), area and state.
    pub acs_limited_english_households_pct: Option<f64>,
    pub acs_limited_english_households_pct_massachusetts: Option<f64>,
    /// "ACS 2020–2024".
    pub acs_period: String,
    /// Largest language groups among residents who speak English less than "very well".
    pub top_languages: Vec<LanguageGroup>,
}

/// A C16001 language group among residents with limited English.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageGroup {
    pub language: String,
    /// Residents 5 and older who speak this language and English less than "very well".
    pub limited_english_speakers: f64,
    /// Share of all residents with limited English in the area.
    pub percent_of_limited_english: f64,
}

/// A town's EJ row (for a county or the state).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownEj {
    pub town: String,
    pub ej_block_groups: u32,
    pub people_in_ej_block_groups: f64,
    /// EEA's percent of the town's population living in EJ block groups.
    pub percent_of_residents: Option<f64>,
    pub criteria: String,
}

/// One EJ block group (public data; listed for a town).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EjBlockGroupRow {
    pub geoid: String,
    pub criteria: String,
    pub population: f64,
    pub pct_minority: Option<f64>,
    /// Block group median household income as a percent of the state's.
    pub median_income_pct_of_state: Option<f64>,
    pub pct_limited_english_households: Option<f64>,
    /// The block group crosses a town line (its whole population is counted in each town).
    pub split_at_town_line: bool,
}

/// Social Vulnerability Index context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SviContext {
    pub tracts: u32,
    /// Tracts in the top quarter of Massachusetts tracts (overall SVI 0.75 or higher).
    pub high_vulnerability_tracts: u32,
    /// EJ block groups that lie in those tracts.
    pub ej_block_groups_in_high_svi_tracts: u32,
    pub people_in_those_block_groups: f64,
}

/// One EJScreen indicator summarized over the area's block groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EjscreenIndicatorSummary {
    pub column: String,
    pub label: String,
    /// "environmental", "ej_index", or "demographic".
    pub kind: String,
    pub block_groups_with_data: u32,
    /// Population-weighted average percentile (all block groups in the area).
    pub mean_percentile: Option<f64>,
    pub mean_percentile_ej_block_groups: Option<f64>,
    pub mean_percentile_other_block_groups: Option<f64>,
    pub block_groups_at_or_above_80th: u32,
    pub block_groups_at_or_above_90th: u32,
}

/// Summary of the nurse's archived EJScreen file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EjscreenSummary {
    pub file: String,
    /// "state", "national", or "as published in the file".
    pub percentiles: String,
    pub block_groups_in_area: u32,
    /// Block groups with any of the EJ Indexes at or above the 80th percentile.
    pub block_groups_any_ej_index_80th: u32,
    pub indicators: Vec<EjscreenIndicatorSummary>,
    pub columns_missing: Vec<String>,
}

/// A DPH vulnerable health EJ criterion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VulnerableHealthCriterion {
    pub name: String,
    pub criterion: String,
}

/// An EJ criterion as defined in law.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CriterionDefinition {
    pub code: String,
    pub label: String,
    pub statute: String,
    pub data: String,
    pub note: String,
    pub citation: String,
}

/// Result of `environmental_justice_screen`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EjScreenResult {
    pub area: Area,
    /// Vintage of the EJ layer.
    pub layer_vintage: String,
    pub summary: EjAreaSummary,
    /// Block groups by each criterion (a block group meeting two counts under both).
    pub by_criterion: Vec<CriteriaCount>,
    /// Block groups by exact combination of criteria.
    pub by_combination: Vec<CriteriaCount>,
    pub official_statistics: Option<OfficialTownStats>,
    pub english_isolation: EnglishIsolation,
    /// Towns with EJ block groups (county or state; most people first).
    pub towns: Vec<TownEj>,
    /// The town's EJ block groups (town only).
    pub block_groups: Vec<EjBlockGroupRow>,
    pub svi: Option<SviContext>,
    pub ejscreen: Option<EjscreenSummary>,
    /// Why EJScreen is not bundled and where to get it.
    pub ejscreen_status: String,
    pub eji_status: CitedOut,
    pub criteria: Vec<CriterionDefinition>,
    pub what_designation_means: Vec<CitedOut>,
    pub language_access: Vec<CitedOut>,
    pub dph_vulnerable_health_criteria: Vec<VulnerableHealthCriterion>,
    pub dph_vulnerable_health_note: CitedOut,
    /// How the EEA Secretary can add or exclude part of a neighborhood.
    pub secretary_designation: CitedOut,
    pub how_to_use: Vec<CitedOut>,
    pub map_files: Vec<String>,
    pub table_file: Option<String>,
}

// ------------------------------------------------------------------ helpers

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn combination_label(code: &str) -> String {
    let parts: Vec<&str> = code
        .chars()
        .map(|c| match c {
            'M' => "minority",
            'I' => "income",
            _ => "English isolation",
        })
        .collect();
    let s = match parts.len() {
        1 => parts[0].to_string(),
        2 => format!("{} and {}", parts[0], parts[1]),
        _ => format!("{}, {}, and {}", parts[0], parts[1], parts[2]),
    };
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

/// The block groups of an area. A town keeps every polygon in it (split block
/// groups count fully, as EEA counts them); a county or the state counts each
/// block group once.
fn area_block_groups<'a>(
    layer: &'a EjLayer,
    area: &Area,
    towns: &BTreeSet<String>,
) -> Vec<&'a EjBlockGroup> {
    let mut seen = BTreeSet::new();
    layer
        .block_groups
        .iter()
        .filter(|b| towns.contains(&b.town))
        .filter(|b| area.level == AreaLevel::Town || seen.insert(b.geoid.clone()))
        .collect()
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

// ------------------------------------------------------------------ EJScreen file

/// Find the block group id column. Census block group codes are geography,
/// not personal identifiers, but the PHI scan flags a column of unique
/// 12-digit codes as an ID number; such a column is accepted only when nearly
/// every value is a Massachusetts block group code in a known census tract.
fn block_group_column(
    lt: &crate::localdata::LocalTable,
    preferred: &str,
    geos: &MaGeographies,
) -> Result<usize> {
    let idx = [preferred, "GEOID", "BG_ID", "bg_geoid", "FIPS", "ID_BG"]
        .iter()
        .find_map(|n| lt.table.column_index(n))
        .ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{}' does not look like an EJScreen block group file: it needs an '{}' column of \
                 12-digit block group codes and percentile columns such as P_PM25",
                lt.label, preferred
            ))
        })?;
    let flagged = lt.findings.iter().any(|f| f.index == idx);
    let (mut present, mut ok) = (0usize, 0usize);
    for v in lt.table.column(idx) {
        let v = v.trim();
        if v.is_empty() {
            continue;
        }
        present += 1;
        if is_block_group_code(v, geos) {
            ok += 1;
        }
    }
    // Rows outside Massachusetts are allowed (the national file); the codes
    // must still all look like block group codes.
    let looks_like_codes = lt
        .table
        .column(idx)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .all(|v| v.len() == 12 && v.bytes().all(|b| b.is_ascii_digit()));
    if present == 0 || ok == 0 || !looks_like_codes {
        return Err(if flagged {
            PhnError::Privacy(format!(
                "column '{}' in '{}' looks like a personal identifier, not census block group \
                 codes, so the file cannot be used",
                lt.table.headers[idx], lt.label
            ))
        } else {
            PhnError::InvalidInput(format!(
                "column '{}' in '{}' does not hold 12-digit census block group codes",
                lt.table.headers[idx], lt.label
            ))
        });
    }
    Ok(idx)
}

/// A Massachusetts block group code whose tract is in the bundled tract list.
fn is_block_group_code(v: &str, geos: &MaGeographies) -> bool {
    v.len() == 12
        && v.starts_with("25")
        && v.bytes().all(|b| b.is_ascii_digit())
        && geos.get(&v[..11]).is_some_and(|g| g.level == GeoLevel::Tract)
}

#[allow(clippy::too_many_arguments)]
fn summarize_ejscreen(
    ctx: &Context,
    path: &str,
    c: &EjContent,
    area: &Area,
    towns: &BTreeSet<String>,
    ej_geoids: &BTreeSet<&str>,
    caveats: &mut Vec<String>,
    sources: &mut Vec<Source>,
) -> Result<(EjscreenSummary, Vec<String>)> {
    let sandbox = ctx.sandbox()?;
    let p = sandbox.resolve_existing(path)?;
    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    if size > EJSCREEN_MAX_BYTES {
        return Err(PhnError::InvalidInput(format!(
            "'{path}' is {} MB; save only the Massachusetts rows (ID starting with 25, or ST_ABBREV \
             = MA) and try again",
            size / (1024 * 1024)
        )));
    }
    let lt = crate::localdata::load(ctx, path, None)?;
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let id = block_group_column(&lt, &c.ejscreen.id_column, geos)?;
    let pop_idx = lt
        .table
        .column_index(&c.ejscreen.population_column)
        .and_then(|_| lt.column(&c.ejscreen.population_column).ok());
    let pops = pop_idx.map(|i| lt.numeric(i).0);
    // Which block groups are in the area.
    let in_area: Vec<bool> = lt
        .table
        .column(id)
        .map(|v| {
            let v = v.trim();
            if !is_block_group_code(v, geos) {
                return false;
            }
            match area.level {
                AreaLevel::State => true,
                AreaLevel::County => area.county_fips.as_deref() == Some(&v[..5]),
                AreaLevel::Town => geos
                    .get(&v[..11])
                    .and_then(|t| t.town.as_deref())
                    .is_some_and(|t| towns.contains(t)),
            }
        })
        .collect();
    let n_area = in_area.iter().filter(|b| **b).count();
    if n_area == 0 {
        return Err(PhnError::InvalidInput(format!(
            "'{}' has no block groups in {}; is it the Massachusetts (or national) EJScreen block \
             group file?",
            lt.label, area.name
        )));
    }
    let is_ej: Vec<bool> = lt
        .table
        .column(id)
        .map(|v| ej_geoids.contains(v.trim()))
        .collect();
    let mut indicators = Vec::new();
    let mut missing = Vec::new();
    let mut ej_index_values: Vec<Vec<Option<f64>>> = Vec::new();
    for ind in &c.ejscreen.indicators {
        let Some(_) = lt.table.column_index(&ind.column) else {
            missing.push(ind.column.clone());
            continue;
        };
        let idx = match lt.column(&ind.column) {
            Ok(i) => i,
            Err(_) => {
                missing.push(ind.column.clone());
                continue;
            }
        };
        let (vals, parsed) = lt.numeric(idx);
        if parsed < 0.5 {
            missing.push(ind.column.clone());
            continue;
        }
        // EJScreen percentiles are 0-100.
        let vals: Vec<Option<f64>> = vals
            .into_iter()
            .map(|v| v.filter(|x| (0.0..=100.0).contains(x)))
            .collect();
        let mut w = [(0.0, 0.0); 3]; // all, EJ, other: (sum, weight)
        let (mut n, mut ge80, mut ge90) = (0u32, 0u32, 0u32);
        for (i, v) in vals.iter().enumerate() {
            if !in_area[i] {
                continue;
            }
            let Some(x) = v else { continue };
            n += 1;
            if *x >= 80.0 {
                ge80 += 1;
            }
            if *x >= 90.0 {
                ge90 += 1;
            }
            let weight = pops
                .as_ref()
                .and_then(|p| p[i])
                .filter(|p| *p > 0.0)
                .unwrap_or(1.0);
            w[0].0 += x * weight;
            w[0].1 += weight;
            let k = if is_ej[i] { 1 } else { 2 };
            w[k].0 += x * weight;
            w[k].1 += weight;
        }
        let mean = |k: usize| (w[k].1 > 0.0).then(|| round1(w[k].0 / w[k].1));
        if ind.kind == "ej_index" {
            ej_index_values.push(vals.clone());
        }
        indicators.push(EjscreenIndicatorSummary {
            column: ind.column.clone(),
            label: ind.label.clone(),
            kind: ind.kind.clone(),
            block_groups_with_data: n,
            mean_percentile: mean(0),
            mean_percentile_ej_block_groups: mean(1),
            mean_percentile_other_block_groups: mean(2),
            block_groups_at_or_above_80th: ge80,
            block_groups_at_or_above_90th: ge90,
        });
    }
    if indicators.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "'{}' has none of the EJScreen percentile columns this tool reads (for example P_PM25, \
             P_PTRAF, P_D2_PM25)",
            lt.label
        )));
    }
    let any80 = (0..in_area.len())
        .filter(|&i| in_area[i])
        .filter(|&i| {
            ej_index_values
                .iter()
                .any(|v| v[i].is_some_and(|x| x >= 80.0))
        })
        .count() as u32;
    let name = lt.label.to_ascii_lowercase();
    let percentiles = if name.contains("statepct") || name.contains("state_pct") {
        "state"
    } else if name.contains("ejscreen_2024_bg_with") {
        "national"
    } else {
        "as published in the file (national or state; the file name does not say)"
    };
    if pops.is_none() {
        caveats.push(format!(
            "The EJScreen file has no {} column, so averages give each block group equal weight.",
            c.ejscreen.population_column
        ));
    }
    if area.level == AreaLevel::Town {
        caveats.push(
            "EJScreen block groups are placed in towns by their census tract's main town; block \
             groups in tracts that cross a town line may be counted with the neighboring town."
                .into(),
        );
    }
    caveats.push(
        "EJScreen is archived: EPA removed it in February 2025 and no longer maintains or updates \
         the data (version 2.3 uses ACS 2018–2022 and environmental data from about 2017–2022)."
            .into(),
    );
    sources.push(lt.source());
    let ids = lt.identifier_categories();
    Ok((
        EjscreenSummary {
            file: lt.label.clone(),
            percentiles: percentiles.into(),
            block_groups_in_area: n_area as u32,
            block_groups_any_ej_index_80th: any80,
            indicators,
            columns_missing: missing,
        },
        ids,
    ))
}

// ------------------------------------------------------------------ map and table

fn write_map(
    ctx: &Context,
    area: &Area,
    bgs: &[&EjBlockGroup],
    towns: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let town_layer = BoundaryLayer::shared(&ctx.snapshots, Layer::Town)?;
    let town_features: Vec<&Feature> = town_layer
        .features
        .iter()
        .filter(|f| towns.contains(&f.id))
        .collect();
    let mut ej_features: Vec<(Feature, usize)> = Vec::new();
    for b in bgs {
        let polygons = decode_geometry(&b.geometry)?;
        if polygons.is_empty() {
            continue;
        }
        ej_features.push((
            Feature {
                id: b.geoid.clone(),
                name: b.criteria(),
                polygons,
            },
            b.criteria_count(),
        ));
    }
    let bbox = town_features
        .iter()
        .map(|f| f.bbox())
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
        .ok_or_else(|| PhnError::Internal("no town boundaries for the map".into()))?;
    let pad = ((bbox.2 - bbox.0).max(bbox.3 - bbox.1) * 0.04).max(500.0);
    let extent = (bbox.0 - pad, bbox.1 - pad, bbox.2 + pad, bbox.3 + pad);
    let colors = phn_docs::palette::sequential_steps(3);
    let mut features: Vec<(&Feature, Fill)> = town_features
        .iter()
        .map(|f| (*f, Fill::Context))
        .collect();
    for (f, n) in &ej_features {
        features.push((f, Fill::Class(n.clamp(&1, &3) - 1)));
    }
    let count = |k: usize| ej_features.iter().filter(|(_, n)| *n == k).count();
    let mut legend: Vec<LegendRow> = (1..=3)
        .rev()
        .map(|k| LegendRow {
            label: format!(
                "{k} criteri{} ({})",
                if k == 1 { "on" } else { "a" },
                count(k)
            ),
            color: Some(colors[k - 1].to_string()),
            hatched: false,
            outline_only: false,
        })
        .collect();
    legend.push(LegendRow {
        label: "Not an EJ block group".into(),
        color: None,
        hatched: false,
        outline_only: true,
    });
    let overlay: Vec<&Feature> = if town_features.len() <= 60 {
        town_features.clone()
    } else {
        Vec::new()
    };
    let labels: Vec<(&Feature, String)> = if town_features.len() <= 30 {
        town_features.iter().map(|f| (*f, f.name.clone())).collect()
    } else {
        Vec::new()
    };
    let title = format!("Environmental justice block groups, {}", area.name);
    let desc = format!(
        "Map of {} showing {} Massachusetts 2020 environmental justice block groups, shaded by the \
         number of EJ criteria they meet: {} meet one criterion (lightest), {} meet two, and {} meet \
         all three (darkest). Areas in white are not EJ block groups.",
        area.name,
        ej_features.len(),
        count(1),
        count(2),
        count(3)
    );
    let spec = MapSpec {
        title: title.clone(),
        subtitle: Some(
            "Criteria: minority, income, English isolation (EEA 2020 layer, updated 2022)".into(),
        ),
        legend_title: "EJ criteria met".into(),
        legend,
        note: format!(
            "Source: Massachusetts EEA 2020 Environmental Justice Populations layer (MassGIS, \
             updated November 2022, corrected June 2024); town boundaries: U.S. Census Bureau \
             cartographic boundary files. Polygons are simplified. Created {} with the Public \
             Health Nursing MCP Server.",
            chrono::Local::now().date_naive()
        ),
        desc,
        features,
        overlay,
        colors: colors.clone(),
        labels,
        extent,
    };
    let svg = render(&spec);
    let png = phn_docs::svg::svg_to_png(&svg, 2.0)?;
    let sandbox = ctx.sandbox()?;
    let dir = sandbox.output_dir()?.join("maps");
    std::fs::create_dir_all(&dir)?;
    let stem = phn_local::sandbox::sanitize_file_name(&format!(
        "ej_block_groups_{}",
        slug(&area.name)
    ));
    let mut n = 1;
    let (svg_path, png_path) = loop {
        let s = if n == 1 {
            stem.clone()
        } else {
            format!("{stem}-{n}")
        };
        let a = dir.join(format!("{s}.svg"));
        let b = dir.join(format!("{s}.png"));
        if (!a.exists() && !b.exists()) || n > 9999 {
            break (a, b);
        }
        n += 1;
    };
    std::fs::write(&svg_path, svg.as_bytes())?;
    std::fs::write(&png_path, &png)?;
    Ok(vec![
        sandbox.display_path(&svg_path),
        sandbox.display_path(&png_path),
    ])
}

fn write_table(ctx: &Context, area: &Area, bgs: &[&EjBlockGroup]) -> Result<String> {
    let opt = |v: Option<f64>| v.map(|x| md::num(x, 1)).unwrap_or_default();
    let rows: Vec<Vec<String>> = bgs
        .iter()
        .map(|b| {
            vec![
                b.geoid.clone(),
                b.town.clone(),
                b.criteria(),
                md::num(b.population, 0),
                opt(b.pct_minority),
                opt(b.mhhi_pct_of_state),
                if b.mhhi_from_2019 { "yes" } else { "" }.into(),
                opt(b.pct_limited_english_households),
                b.limited_english_households
                    .map(|v| md::num(v, 0))
                    .unwrap_or_default(),
                if b.split_for_town { "yes" } else { "" }.into(),
            ]
        })
        .collect();
    let doc = Document {
        title: format!("Environmental justice block groups, {}", area.name),
        subtitle: Some("Massachusetts EEA 2020 EJ layer (updated November 2022)".into()),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks: vec![
            Block::Table(TableBlock {
                caption: Some("EJ block groups".into()),
                headers: [
                    "Block group (GEOID)",
                    "City or town",
                    "Criteria (M = minority, I = income, E = English isolation)",
                    "Population (2020 Census)",
                    "Percent minority",
                    "Median household income, percent of state",
                    "Income from ACS 2015-2019",
                    "Percent limited English households",
                    "Limited English households",
                    "Crosses a town line",
                ]
                .map(String::from)
                .to_vec(),
                rows,
            }),
            Block::Paragraph(vec![Inline::plain(
                "Source: Massachusetts Executive Office of Energy and Environmental Affairs, 2020 \
                 Environmental Justice Populations (MassGIS EJ_POLY), 2020 Census and ACS \
                 2016-2020. Block groups that cross a town line carry the whole block group's \
                 values in each town.",
            )]),
        ],
    };
    let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
    let sandbox = ctx.sandbox()?;
    let path = sandbox.new_output_file(&format!("ej_block_groups_{}.xlsx", slug(&area.name)))?;
    std::fs::write(&path, bytes)?;
    Ok(sandbox.display_path(&path))
}

// ------------------------------------------------------------------ the tool

/// Screen a Massachusetts area for environmental justice populations.
pub fn environmental_justice_screen(
    ctx: &Context,
    args: EjScreenArgs,
) -> Result<Envelope<EjScreenResult>> {
    let c: EjContent = content::load(EJ_CONTENT)?;
    let area = area::resolve(ctx, args.area.as_deref(), true)?;
    let layer = EjLayer::shared(&ctx.snapshots)?;
    let towns: BTreeSet<String> = area::towns_in(ctx, &area).into_iter().collect();
    let bgs = area_block_groups(layer, &area, &towns);
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = vec![layer.source.clone()];

    // Summary.
    let population_2020: f64 = layer
        .towns
        .iter()
        .filter(|t| towns.contains(&t.town))
        .map(|t| t.pop_2020)
        .sum();
    let layer_people: f64 = bgs.iter().map(|b| b.population).sum();
    let town_row = area.town.as_deref().and_then(|t| layer.town(t));
    let official = town_row
        .filter(|t| t.ej_block_groups.is_some())
        .map(|t| OfficialTownStats {
            criteria: t.ej_criteria.clone().unwrap_or_default(),
            ej_block_groups: t.ej_block_groups,
            total_block_groups: t.total_block_groups,
            population_in_ej_block_groups: t.pop_in_ej_block_groups,
            total_population: t.stats_total_pop,
            percent_population_in_ej_block_groups: t.pct_pop_in_ej_block_groups,
        });
    let ej_towns: BTreeSet<&str> = bgs.iter().map(|b| b.town.as_str()).collect();
    let (people, percent, total_bgs) = match (&area.level, &official) {
        (AreaLevel::Town, Some(o)) => (
            o.population_in_ej_block_groups.unwrap_or(layer_people),
            o.percent_population_in_ej_block_groups,
            o.total_block_groups,
        ),
        (AreaLevel::Town, None) => (0.0, Some(0.0), None),
        (level, _) => (
            layer_people,
            (population_2020 > 0.0).then(|| round1(layer_people / population_2020 * 100.0)),
            (*level == AreaLevel::State).then_some(c.state_block_groups.count),
        ),
    };
    if area.level == AreaLevel::Town
        && let Some(o) = &official
    {
        let n_layer = bgs.len() as u32;
        if o.ej_block_groups != Some(n_layer)
            || o
                .population_in_ej_block_groups
                .is_some_and(|p| (p - layer_people).abs() > 0.5)
        {
            caveats.push(format!(
                "EEA's municipal statistics list {} EJ block group(s) ({} people) in {}; the \
                 newer layer (June 2024) has {} ({} people). Counts by criterion below come from \
                 the layer.",
                o.ej_block_groups.map(|v| v.to_string()).unwrap_or_default(),
                o.population_in_ej_block_groups
                    .map(|v| md::num(v, 0))
                    .unwrap_or_default(),
                area.name,
                n_layer,
                md::num(layer_people, 0)
            ));
        }
    }
    if bgs.iter().any(|b| b.split_for_town) {
        caveats.push(
            "Some EJ block groups cross a town line; EEA counts the whole block group's \
             population in each town it touches, so town figures can exceed the town's own \
             population living in them."
                .into(),
        );
    }
    let summary = EjAreaSummary {
        ej_block_groups: bgs.len() as u32,
        total_block_groups: total_bgs,
        people_in_ej_block_groups: people,
        population_2020,
        percent_of_residents: percent,
        towns_with_ej_block_groups: ej_towns.len() as u32,
        towns_in_area: towns.len() as u32,
    };

    // By criterion and combination.
    let mut by_criterion = Vec::new();
    for (code, label, f) in [
        ("M", "Minority", (|b: &EjBlockGroup| b.minority) as fn(&EjBlockGroup) -> bool),
        ("I", "Income", |b: &EjBlockGroup| b.income),
        ("E", "English isolation", |b: &EjBlockGroup| b.english_isolation),
    ] {
        let sel: Vec<&&EjBlockGroup> = bgs.iter().filter(|b| f(b)).collect();
        by_criterion.push(CriteriaCount {
            code: code.into(),
            label: label.into(),
            block_groups: sel.len() as u32,
            people: sel.iter().map(|b| b.population).sum(),
        });
    }
    let mut combos: BTreeMap<String, (u32, f64)> = BTreeMap::new();
    for b in &bgs {
        let e = combos.entry(b.criteria()).or_default();
        e.0 += 1;
        e.1 += b.population;
    }
    let order = ["M", "I", "E", "MI", "ME", "IE", "MIE"];
    let by_combination: Vec<CriteriaCount> = order
        .iter()
        .filter_map(|k| {
            combos.get(*k).map(|(n, p)| CriteriaCount {
                code: k.to_string(),
                label: combination_label(k),
                block_groups: *n,
                people: *p,
            })
        })
        .collect();

    // English isolation and language.
    let e_bgs: Vec<&&EjBlockGroup> = bgs.iter().filter(|b| b.english_isolation).collect();
    let acs = AcsSnapshot::shared(&ctx.snapshots)?;
    let acs_row = acs.row(&area.geoid);
    let acs_period = format!("ACS {}–{}", acs.vintage - 4, acs.vintage);
    let mut top_languages = Vec::new();
    if let Some(r) = acs_row {
        let total_lep: f64 = census::LANGUAGE_GROUPS
            .iter()
            .chain(std::iter::once(&census::LANGUAGE_OTHER))
            .filter_map(|(id, _, _, _)| r.values.get(&format!("lang_{id}_lep")).map(|e| e.value))
            .sum();
        let mut groups: Vec<(String, f64)> = census::LANGUAGE_GROUPS
            .iter()
            .chain(std::iter::once(&census::LANGUAGE_OTHER))
            .filter_map(|(id, label, _, _)| {
                r.values
                    .get(&format!("lang_{id}_lep"))
                    .map(|e| (label.to_string(), e.value))
            })
            .filter(|(_, v)| *v > 0.0)
            .collect();
        groups.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top_languages = groups
            .into_iter()
            .take(3)
            .map(|(language, n)| LanguageGroup {
                language,
                limited_english_speakers: n.round(),
                percent_of_limited_english: if total_lep > 0.0 {
                    round1(n / total_lep * 100.0)
                } else {
                    0.0
                },
            })
            .collect();
        sources.push(acs.source());
    }
    let english_isolation = EnglishIsolation {
        block_groups: e_bgs.len() as u32,
        people: e_bgs.iter().map(|b| b.population).sum(),
        limited_english_households: e_bgs
            .iter()
            .filter_map(|b| b.limited_english_households)
            .sum(),
        limited_english_households_in_ej_block_groups: bgs
            .iter()
            .filter_map(|b| b.limited_english_households)
            .sum(),
        acs_limited_english_households_pct: acs_row
            .and_then(|r| r.values.get("lep_households_pct"))
            .map(|e| round1(e.value)),
        acs_limited_english_households_pct_massachusetts: acs
            .state()
            .and_then(|r| r.values.get("lep_households_pct"))
            .map(|e| round1(e.value)),
        acs_period,
        top_languages,
    };

    // Towns (county or state).
    let mut town_rows: Vec<TownEj> = Vec::new();
    if area.level != AreaLevel::Town {
        let mut agg: BTreeMap<&str, (u32, f64, BTreeSet<char>)> = BTreeMap::new();
        for b in layer.block_groups.iter().filter(|b| towns.contains(&b.town)) {
            let e = agg.entry(b.town.as_str()).or_default();
            e.0 += 1;
            e.1 += b.population;
            e.2.extend(b.criteria().chars());
        }
        town_rows = agg
            .into_iter()
            .map(|(t, (n, p, crit))| TownEj {
                town: t.to_string(),
                ej_block_groups: n,
                people_in_ej_block_groups: p,
                percent_of_residents: layer.town(t).and_then(|r| r.pct_pop_in_ej_block_groups),
                criteria: ['M', 'I', 'E']
                    .iter()
                    .filter(|ch| crit.contains(ch))
                    .collect(),
            })
            .collect();
        town_rows.sort_by(|a, b| {
            b.people_in_ej_block_groups
                .total_cmp(&a.people_in_ej_block_groups)
                .then_with(|| a.town.cmp(&b.town))
        });
    }
    let towns_total = town_rows.len();

    // Block group rows (town).
    let block_groups: Vec<EjBlockGroupRow> = if area.level == AreaLevel::Town {
        bgs.iter()
            .map(|b| EjBlockGroupRow {
                geoid: b.geoid.clone(),
                criteria: b.criteria(),
                population: b.population,
                pct_minority: b.pct_minority,
                median_income_pct_of_state: b.mhhi_pct_of_state,
                pct_limited_english_households: b.pct_limited_english_households,
                split_at_town_line: b.split_for_town,
            })
            .collect()
    } else {
        Vec::new()
    };
    if bgs.iter().any(|b| b.mhhi_from_2019) {
        caveats.push(
            "For some block groups the 2016–2020 income estimate was blank, so EEA used the \
             2015–2019 values to test the income criterion."
                .into(),
        );
    }

    // SVI.
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let svi = match SviSnapshot::shared(&ctx.snapshots) {
        Ok(snap) => {
            let tracts: Vec<&phn_data::geography::Geography> = geos
                .at_level(GeoLevel::Tract)
                .filter(|t| match area.level {
                    AreaLevel::State => true,
                    AreaLevel::County => {
                        Some(t.county_geoid.as_str()) == area.county_fips.as_deref()
                    }
                    AreaLevel::Town => t.town.as_deref() == area.town.as_deref(),
                })
                .collect();
            let mut n = 0u32;
            let mut high: BTreeSet<&str> = BTreeSet::new();
            for t in &tracts {
                if let Some(x) = snap.tract(&t.geoid) {
                    n += 1;
                    if x.ma.overall.is_some_and(|o| o >= 0.75) {
                        high.insert(t.geoid.as_str());
                    }
                }
            }
            // Every high-SVI tract statewide, so EJ block groups in a tract
            // whose main town is a neighbor still count.
            let high_any: BTreeSet<&str> = snap
                .tracts
                .iter()
                .filter(|x| x.ma.overall.is_some_and(|o| o >= 0.75))
                .map(|x| x.fips.as_str())
                .collect();
            let in_high: Vec<&&EjBlockGroup> = bgs
                .iter()
                .filter(|b| high_any.contains(b.tract()))
                .collect();
            sources.push(snap.source.clone());
            (n > 0).then(|| SviContext {
                tracts: n,
                high_vulnerability_tracts: high.len() as u32,
                ej_block_groups_in_high_svi_tracts: in_high.len() as u32,
                people_in_those_block_groups: in_high.iter().map(|b| b.population).sum(),
            })
        }
        Err(_) => None,
    };

    // EJScreen (local file).
    let ej_geoids: BTreeSet<&str> = bgs.iter().map(|b| b.geoid.as_str()).collect();
    let mut identifiers = Vec::new();
    let ejscreen = match &args.ejscreen_path {
        Some(p) => {
            let (s, ids) = summarize_ejscreen(
                ctx,
                p,
                &c,
                &area,
                &towns,
                &ej_geoids,
                &mut caveats,
                &mut sources,
            )?;
            identifiers = ids;
            Some(s)
        }
        None => None,
    };

    // Files.
    let mut files_written = Vec::new();
    let mut map_files = Vec::new();
    if args.map.unwrap_or(false) {
        map_files = write_map(ctx, &area, &bgs, &towns)?;
        files_written.extend(map_files.iter().cloned());
    }
    let mut table_file = None;
    if args.table.unwrap_or(false) {
        if bgs.is_empty() {
            caveats.push(format!(
                "No table was written: {} has no EJ block groups.",
                area.name
            ));
        } else {
            let f = write_table(ctx, &area, &bgs)?;
            files_written.push(f.clone());
            table_file = Some(f);
        }
    }

    // Content.
    let src = &c.sources;
    let criteria: Vec<CriterionDefinition> = c
        .criteria
        .iter()
        .map(|k| CriterionDefinition {
            code: k.code.clone(),
            label: k.label.clone(),
            statute: k.statute.clone(),
            data: k.data.clone(),
            note: k.note.clone(),
            citation: format!(
                "{}, {}",
                src.get(&c.criteria_source)
                    .map(|s| s.title.as_str())
                    .unwrap_or(&c.criteria_source),
                c.criteria_section
            ),
        })
        .collect();
    let what = content::cite_all(&c.what_designation_means, src);
    let language_access = content::cite_all(&c.language_access, src);
    let how_to_use = content::cite_all(&c.how_to_use, src);
    let vh = &c.dph_vulnerable_health;
    let vh_note = CitedOut {
        text: vh.intro.clone(),
        citation: format!(
            "{}, {}",
            src.get(&vh.source)
                .map(|s| s.title.as_str())
                .unwrap_or(&vh.source),
            vh.section
        ),
        confidence: vh.confidence,
    };
    let eji_status = cite(&c.eji, src);
    let neighborhood = cite(&c.neighborhood_definition, src);
    let filter = cite(&c.ejscreen.filter, src);

    // Markdown.
    let mut s = format!("**Environmental justice screen — {}**\n\n", area.name);
    match area.level {
        AreaLevel::Town if summary.ej_block_groups == 0 => s.push_str(&format!(
            "{} has **no** 2020 environmental justice (EJ) block groups (2020 Census population \
             {}). EJ designation can still apply to projects near the town: MEPA looks at EJ \
             populations within 1 mile (5 miles for some air and diesel traffic projects) of a \
             project site.\n\n",
            area.name,
            md::num(population_2020, 0)
        )),
        _ => s.push_str(&format!(
            "**{} EJ block group{}**{} — about **{} people** ({}{}) live in them.{}\n\n",
            summary.ej_block_groups,
            if summary.ej_block_groups == 1 { "" } else { "s" },
            summary
                .total_block_groups
                .map(|t| format!(" of {t}"))
                .unwrap_or_default(),
            md::num(people, 0),
            percent
                .map(|p| format!("{}% of residents", md::num(p, 1)))
                .unwrap_or_else(|| "share not available".into()),
            if area.level == AreaLevel::Town {
                ", EEA's figure"
            } else {
                ", 2020 Census"
            },
            if area.level == AreaLevel::Town {
                String::new()
            } else {
                format!(
                    " {} of {} cities and towns have at least one.",
                    summary.towns_with_ej_block_groups, summary.towns_in_area
                )
            }
        )),
    }
    if !bgs.is_empty() {
        let rows: Vec<Vec<String>> = by_criterion
            .iter()
            .map(|k| {
                vec![
                    format!("{} ({})", k.label, k.code),
                    k.block_groups.to_string(),
                    md::num(k.people, 0),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["Criterion met", "EJ block groups", "People (2020 Census)"],
            &rows,
        ));
        s.push_str(&format!(
            "\nBy combination: {}.\n\n",
            by_combination
                .iter()
                .map(|k| format!("{} {}", k.label.to_lowercase(), k.block_groups))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    s.push_str("**Language access**\n\n");
    if english_isolation.block_groups > 0 {
        s.push_str(&format!(
            "{} block group{} meet the English isolation criterion (25% or more of households \
             limited English speaking): about {} people and {} limited-English households. ",
            english_isolation.block_groups,
            if english_isolation.block_groups == 1 {
                ""
            } else {
                "s"
            },
            md::num(english_isolation.people, 0),
            md::num(english_isolation.limited_english_households, 0)
        ));
    } else {
        s.push_str("No EJ block group here meets the English isolation criterion. ");
    }
    if let Some(p) = english_isolation.acs_limited_english_households_pct {
        s.push_str(&format!(
            "Across {}, {}% of households are limited English speaking ({}; Massachusetts {}%).",
            area.name,
            md::num(p, 1),
            english_isolation.acs_period,
            english_isolation
                .acs_limited_english_households_pct_massachusetts
                .map(|v| md::num(v, 1))
                .unwrap_or_else(|| "—".into())
        ));
    }
    if !english_isolation.top_languages.is_empty() {
        s.push_str(&format!(
            " Largest language groups among residents with limited English: {}.",
            english_isolation
                .top_languages
                .iter()
                .map(|l| format!(
                    "{} ({}%)",
                    l.language,
                    md::num(l.percent_of_limited_english, 0)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    s.push_str(
        " Use `language_access_profile` for tract-level detail; MEPA asks proponents to consult \
         local Boards of Health on languages.\n\n",
    );
    if let Some(v) = &svi {
        s.push_str(&format!(
            "**Social Vulnerability Index:** {} of {} census tracts are in the top quarter for \
             Massachusetts; {} EJ block group(s) (about {} people) lie in high-SVI tracts.\n\n",
            v.high_vulnerability_tracts,
            v.tracts,
            v.ej_block_groups_in_high_svi_tracts,
            md::num(v.people_in_those_block_groups, 0)
        ));
    }
    if !town_rows.is_empty() {
        let rows: Vec<Vec<String>> = town_rows
            .iter()
            .take(15)
            .map(|t| {
                vec![
                    t.town.clone(),
                    t.ej_block_groups.to_string(),
                    md::num(t.people_in_ej_block_groups, 0),
                    t.percent_of_residents
                        .map(|p| format!("{}%", md::num(p, 1)))
                        .unwrap_or_else(|| "—".into()),
                    t.criteria.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "City or town",
                "EJ block groups",
                "People in them",
                "Share of residents (EEA)",
                "Criteria",
            ],
            &rows,
        ));
        if towns_total > 15 {
            s.push_str(&format!(
                "\n… and {} more towns (see data.towns or the XLSX table).\n",
                towns_total - 15
            ));
        }
        s.push('\n');
    }
    if let Some(e) = &ejscreen {
        s.push_str(&format!(
            "**Archived EPA EJScreen ({} percentiles, `{}`)**: {} block groups in {}; {} have at \
             least one EJ Index at or above the 80th percentile (EPA's former starting point for \
             further review, not a designation).\n\n",
            e.percentiles,
            e.file,
            e.block_groups_in_area,
            area.name,
            e.block_groups_any_ej_index_80th
        ));
        let mut rows: Vec<&EjscreenIndicatorSummary> = e
            .indicators
            .iter()
            .filter(|i| i.kind == "environmental")
            .collect();
        rows.sort_by(|a, b| {
            b.mean_percentile
                .unwrap_or(0.0)
                .total_cmp(&a.mean_percentile.unwrap_or(0.0))
        });
        let table: Vec<Vec<String>> = rows
            .iter()
            .take(8)
            .map(|i| {
                let f = |v: Option<f64>| v.map(|x| md::num(x, 0)).unwrap_or_else(|| "—".into());
                vec![
                    i.label.clone(),
                    f(i.mean_percentile),
                    f(i.mean_percentile_ej_block_groups),
                    f(i.mean_percentile_other_block_groups),
                    i.block_groups_at_or_above_80th.to_string(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Environmental indicator",
                "Average percentile",
                "In EJ block groups",
                "In other block groups",
                "Block groups ≥ 80th",
            ],
            &table,
        ));
        s.push('\n');
    } else {
        s.push_str(&format!(
            "**EJScreen:** not bundled (EPA removed it in 2025). Save an archived EJScreen 2.3 \
             block group CSV in /data and pass `ejscreen_path` to add environmental indicators.\n\n"
        ));
    }
    s.push_str("**What an EJ designation changes**\n");
    for w in what.iter().take(2) {
        s.push_str(&content::bullet(w));
    }
    s.push_str(&format!(
        "\nDPH's vulnerable health EJ criteria (any one, at 110% or more of the state rate): {}. \
         See the DPH EJ Tool for town rates.\n",
        vh.criteria
            .iter()
            .map(|k| k.name.to_lowercase())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if !map_files.is_empty() {
        s.push_str(&format!(
            "\nMap written to `{}` and `{}`.\n",
            map_files[0], map_files[1]
        ));
    }
    if let Some(f) = &table_file {
        s.push_str(&format!("\nEJ block group table written to `{f}`.\n"));
    }

    // Caveats and sources.
    caveats.insert(
        0,
        "EJ block groups are 2020 Census geography with ACS 2016–2020 income and language \
         estimates (EEA update of November 2022, one correction in June 2024); designations change \
         when EEA updates the layer. Confirm a specific site on EEA's EJ Maps Viewer."
            .into(),
    );
    caveats.push(format!(
        "Neighborhood, in the law, means: {}",
        neighborhood.text
    ));
    if area.level == AreaLevel::State {
        caveats.push(format!(
            "Statewide counts give each block group once; EEA's published totals (2,604 block \
             groups, 3,487,681 people) count the three block groups split at town lines once per \
             town. The {} block groups in Massachusetts are from {} ({}).",
            c.state_block_groups.count,
            src.get(&c.state_block_groups.source)
                .map(|s| s.title.as_str())
                .unwrap_or(&c.state_block_groups.source),
            c.state_block_groups.section
        ));
    }
    if ejscreen.is_none() {
        caveats.push(c.ejscreen.status.clone());
    }
    caveats.push(format!("{} ({})", filter.text, filter.citation));
    if ejscreen.is_some() {
        let n = cite(&c.ejscreen.percentile_note, src);
        caveats.push(format!("{} ({})", n.text, n.citation));
    }
    caveats.push(eji_status.text.clone());
    caveats.extend(content::review_caveats(&[EJ_CONTENT]));
    let mut keys: Vec<&str> = vec![c.criteria_source.as_str()];
    keys.extend(c.layer_facts.iter().map(|x| x.source.as_str()));
    keys.extend(c.what_designation_means.iter().map(|x| x.source.as_str()));
    keys.extend(c.language_access.iter().map(|x| x.source.as_str()));
    keys.push(vh.source.as_str());
    keys.push(c.eji.source.as_str());
    keys.push(c.state_block_groups.source.as_str());
    if ejscreen.is_some() {
        keys.push(c.ejscreen.filter.source.as_str());
        keys.push(c.ejscreen.indicators_source.as_str());
    }
    sources.extend(content::sources_for(keys, src));

    let mut notes = Vec::new();
    if args.ejscreen_path.is_some() {
        notes.push(
            "Only block group averages and counts from your EJScreen file are returned; its rows \
             are not."
                .into(),
        );
    }
    let ejscreen_status = format!("{} {}", c.ejscreen.status, c.ejscreen.where_to_get);
    let town_rows_out: Vec<TownEj> = if area.level == AreaLevel::State {
        town_rows.into_iter().take(MAX_TOWNS_LISTED).collect()
    } else {
        town_rows
    };
    if area.level == AreaLevel::State && towns_total > MAX_TOWNS_LISTED {
        caveats.push(format!(
            "data.towns lists the {MAX_TOWNS_LISTED} towns with the most people in EJ block \
             groups (of {towns_total}); ask for table: true for all of them."
        ));
    }

    Ok(Envelope::new(
        s,
        EjScreenResult {
            area,
            layer_vintage: "2020 Census and ACS 2016–2020; EEA update November 2022 (corrected \
                            June 2024)"
                .into(),
            summary,
            by_criterion,
            by_combination,
            official_statistics: official,
            english_isolation,
            towns: town_rows_out,
            block_groups,
            svi,
            ejscreen,
            ejscreen_status,
            eji_status,
            criteria,
            what_designation_means: what,
            language_access,
            dph_vulnerable_health_criteria: vh
                .criteria
                .iter()
                .map(|k| VulnerableHealthCriterion {
                    name: k.name.clone(),
                    criterion: k.criterion.clone(),
                })
                .collect(),
            dph_vulnerable_health_note: vh_note,
            secretary_designation: cite(&c.secretary_designation, src),
            how_to_use,
            map_files,
            table_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: identifiers,
        files_written,
        notes,
        ..Privacy::none()
    })
    .decision_support())
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

    fn run(ctx: &Context, area: &str) -> Envelope<EjScreenResult> {
        environmental_justice_screen(
            ctx,
            EjScreenArgs {
                area: Some(area.into()),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn content_parses_and_cites_known_sources() {
        let c: EjContent = content::load(EJ_CONTENT).unwrap();
        assert_eq!(c.criteria.len(), 3);
        let keys: Vec<&String> = c
            .what_designation_means
            .iter()
            .chain(&c.language_access)
            .chain(&c.layer_facts)
            .chain(&c.how_to_use)
            .map(|x| &x.source)
            .collect();
        for k in keys {
            assert!(c.sources.contains_key(k), "unknown source {k}");
        }
        assert!(c.sources.contains_key(&c.criteria_source));
        assert_eq!(c.state_block_groups.count, 4985);
        assert_eq!(c.dph_vulnerable_health.criteria.len(), 4);
    }

    #[test]
    fn lowell_matches_eea_statistics() {
        let (_d, c) = ctx();
        let env = run(&c, "Lowell");
        let d = &env.data;
        // EEA municipal statistics (November 2022): 77 of 81, 111,001 people, 96.1%.
        assert_eq!(d.summary.ej_block_groups, 77);
        assert_eq!(d.summary.total_block_groups, Some(81));
        assert_eq!(d.summary.people_in_ej_block_groups, 111_001.0);
        assert_eq!(d.summary.percent_of_residents, Some(96.1));
        let e = d.by_criterion.iter().find(|k| k.code == "E").unwrap();
        assert!(e.block_groups > 0);
        assert_eq!(d.english_isolation.block_groups, e.block_groups);
        assert_eq!(d.block_groups.len(), 77);
        assert!(d.svi.as_ref().unwrap().tracts > 20);
        assert!(env.summary.contains("77 EJ block groups"));
        assert!(env.notice.is_some());
        assert!(d.ejscreen.is_none());
        assert!(env.caveats.iter().any(|c| c.contains("EPA removed")));
        // Combinations add up to the total.
        let sum: u32 = d.by_combination.iter().map(|k| k.block_groups).sum();
        assert_eq!(sum, 77);
    }

    #[test]
    fn towns_without_ej_block_groups_and_the_state() {
        let (_d, c) = ctx();
        let env = run(&c, "Abington");
        assert_eq!(env.data.summary.ej_block_groups, 0);
        assert_eq!(env.data.summary.percent_of_residents, Some(0.0));
        assert!(env.summary.contains("no** 2020 environmental justice"));
        let env = run(&c, "Massachusetts");
        let s = &env.data.summary;
        // 2,604 polygons; 2,599 distinct block groups.
        assert_eq!(s.ej_block_groups, 2599);
        assert_eq!(s.total_block_groups, Some(4985));
        assert_eq!(s.towns_with_ej_block_groups, 187);
        assert_eq!(s.population_2020, 7_029_917.0);
        let p = s.percent_of_residents.unwrap();
        assert!((49.0..50.5).contains(&p), "{p}");
        assert_eq!(env.data.towns.len(), MAX_TOWNS_LISTED);
        assert_eq!(env.data.towns[0].town, "Boston");
        let env = run(&c, "Hampden County");
        assert!(env.data.towns.iter().any(|t| t.town == "Springfield"));
        assert!(env.data.block_groups.is_empty());
    }

    #[test]
    fn chelmsford_notes_the_layer_and_statistics_differ() {
        let (_d, c) = ctx();
        let env = run(&c, "Chelmsford");
        assert_eq!(env.data.summary.ej_block_groups, 6);
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("municipal statistics list 5"))
        );
        // Split block groups are flagged.
        let env = run(&c, "Monroe");
        assert!(env.data.block_groups[0].split_at_town_line);
        assert!(env.caveats.iter().any(|c| c.contains("cross a town line")));
    }

    #[test]
    fn writes_map_and_table_and_summarizes_an_ejscreen_file() {
        let (dir, c) = ctx();
        // A synthetic EJScreen extract: three Lowell block groups (two EJ, one
        // made up as non-EJ in the same tracts) and one outside the town.
        let layer = EjLayer::shared(&c.snapshots).unwrap();
        let lowell: Vec<&EjBlockGroup> = layer
            .block_groups
            .iter()
            .filter(|b| b.town == "Lowell")
            .take(2)
            .collect();
        let mut csv = String::from("ID,ST_ABBREV,ACSTOTPOP,P_PM25,P_PTRAF,P_D2_PTRAF,P_D2_PM25\n");
        csv.push_str(&format!("{},MA,1000,85,95,92,70\n", lowell[0].geoid));
        csv.push_str(&format!("{},MA,3000,65,75,50,40\n", lowell[1].geoid));
        csv.push_str("250250101031,MA,500,99,99,99,99\n");
        std::fs::write(dir.path().join("data/ejscreen_ma.csv"), csv).unwrap();
        let env = environmental_justice_screen(
            &c,
            EjScreenArgs {
                area: Some("Lowell".into()),
                ejscreen_path: Some("ejscreen_ma.csv".into()),
                map: Some(true),
                table: Some(true),
            },
        )
        .unwrap();
        let e = env.data.ejscreen.as_ref().unwrap();
        assert_eq!(e.block_groups_in_area, 2);
        assert_eq!(e.block_groups_any_ej_index_80th, 1);
        let pm = e.indicators.iter().find(|i| i.column == "P_PM25").unwrap();
        // Population-weighted: (85*1000 + 65*3000) / 4000 = 70.
        assert_eq!(pm.mean_percentile, Some(70.0));
        assert_eq!(pm.block_groups_at_or_above_80th, 1);
        assert!(e.columns_missing.contains(&"P_OZONE".to_string()));
        assert_eq!(env.data.map_files.len(), 2);
        let svg =
            std::fs::read_to_string(dir.path().join("data").join(&env.data.map_files[0])).unwrap();
        assert!(svg.contains("Environmental justice block groups, Lowell"));
        let png = std::fs::read(dir.path().join("data").join(&env.data.map_files[1])).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let t = env.data.table_file.as_ref().unwrap();
        assert!(t.ends_with(".xlsx"));
        assert_eq!(env.privacy.files_written.len(), 3);
        // A file that is not an EJScreen file is refused without echoing values.
        std::fs::write(
            dir.path().join("data/other.csv"),
            "Name,Phone\nAda Testperson,617-555-0101\n",
        )
        .unwrap();
        let err = environmental_justice_screen(
            &c,
            EjScreenArgs {
                area: Some("Lowell".into()),
                ejscreen_path: Some("other.csv".into()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(!err.contains("Testperson") && !err.contains("555"), "{err}");
    }

    #[test]
    fn combination_labels_read_naturally() {
        assert_eq!(combination_label("M"), "Minority");
        assert_eq!(combination_label("MI"), "Minority and income");
        assert_eq!(
            combination_label("MIE"),
            "Minority, income, and English isolation"
        );
    }
}
