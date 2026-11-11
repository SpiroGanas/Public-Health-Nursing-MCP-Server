//! CHR-06 `food_access_mapper`: low-income/low-access census tracts from the
//! USDA ERS Food Access Research Atlas for a town, county, or the state (the
//! 2025 SNAP-authorized Retailer Access Map on 2020 tracts, with the 2019
//! Large Retailer Access Map carried to 2020 tracts), people and households
//! affected, poverty and vehicle context, HIP and SNAP information, and the
//! food resources in the nurse's own files (pantries and meal sites, SNAP
//! retailers, HIP vendors), counted by town. Site names and addresses go only
//! into the DOCX resource list and XLSX workbook in /data/output; an optional
//! tract map is drawn as SVG and PNG.

use std::collections::{BTreeMap, BTreeSet};

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::boundaries::{BoundaryLayer, Feature, Layer};
use phn_data::census::AcsSnapshot;
use phn_data::food_access::{self, Definition, Distance, FoodAccess, FoodAccessTract};
use phn_data::geography::MaGeographies;
use phn_data::projection;
use phn_data::svi::SviSnapshot;
use phn_docs::model::{Block, Document, Inline, TableBlock};
use phn_local::phi::Identifier;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epi::choropleth::render::{self, Fill, LegendRow, MapSpec};
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource, LinkDef};
use crate::md;

/// Content id of `content/chronic/food_access.yaml`.
pub const FOOD_ACCESS: &str = "chronic-food-access";

/// Share of a 2020 tract's land in 2010 LRAM low-income/low-access tracts at which
/// the tract is counted (tool default).
pub const LRAM_SHARE_THRESHOLD: f64 = 0.5;
/// LILA tracts listed in the response (the workbook lists them all).
const MAX_LISTED_TRACTS: usize = 60;
/// SVI percentile at or above which a tract is called highly vulnerable (tool default,
/// the top quartile).
const HIGH_SVI: f64 = 0.75;

/// Arguments for `food_access_mapper`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct FoodAccessArgs {
    /// A Massachusetts city or town, a county, or "Massachusetts". Default: your saved town,
    /// or the state when none is saved.
    pub area: Option<String>,
    /// Distance to the nearest store: "straight_line" (default) or "driving".
    pub distance: Option<Distance>,
    /// Low-income/low-access definition for the tract list and map: one_and_ten,
    /// half_and_ten, one_and_twenty, or vehicle. Default: a tract that meets any of them.
    pub definition: Option<Definition>,
    /// A CSV or XLSX file in /data listing food pantries and meal sites (organizations, not
    /// clients): name, type, address, town, hours, phone, website, languages, latitude,
    /// longitude. Only counts by town are returned; the sites go to the output files.
    pub pantries_path: Option<String>,
    /// A CSV file in /data of SNAP-authorized retailers downloaded from USDA's SNAP Retailer
    /// Locator (store name, store type, city, state, ZIP, latitude, longitude, end date).
    pub snap_retailers_path: Option<String>,
    /// A CSV or XLSX file in /data of HIP farm vendors (name, type, address, town, hours),
    /// for example built from DTAFinder.com.
    pub hip_vendors_path: Option<String>,
    /// Draw a tract map (SVG and PNG) to /data/output/maps (default true for a town or
    /// county, false for the state).
    pub map: Option<bool>,
    /// Write the XLSX workbook (tracts and resource lists) and a DOCX resource list to
    /// /data/output (default true).
    pub report: Option<bool>,
}

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct DefinitionDef {
    id: String,
    label: String,
    text: String,
    source: String,
    section: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct MethodDef {
    text: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
struct ColumnDef {
    field: String,
    aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LocalFileDef {
    id: String,
    label: String,
    how_to_get: String,
    confidence: Confidence,
    columns: Vec<ColumnDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct FoodContent {
    sources: BTreeMap<String, DocSource>,
    definitions: Vec<DefinitionDef>,
    background: Vec<Cited>,
    methods: Vec<MethodDef>,
    hip: Vec<Cited>,
    snap: Vec<Cited>,
    local_files: Vec<LocalFileDef>,
    links: Vec<LinkDef>,
}

// ---------------------------------------------------------------- output

/// Counts for one low-income/low-access definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DefinitionSummary {
    /// one_and_ten, half_and_ten, one_and_twenty, or vehicle.
    pub id: String,
    pub label: String,
    /// SRAM (any SNAP-authorized store) low-income/low-access tracts.
    pub lila_tracts: usize,
    /// Everyone living in those tracts (2020 Census).
    pub people_in_lila_tracts: f64,
    /// People beyond the distance in those tracts (not for the vehicle definition).
    pub low_access_people_in_lila_tracts: Option<f64>,
    /// Low-income people beyond the distance in those tracts.
    pub low_access_low_income_people_in_lila_tracts: Option<f64>,
    /// LRAM 2019 (supermarkets): 2020 tracts with at least half their land in a 2010
    /// low-income/low-access tract.
    pub lram_lila_tracts: usize,
    pub people_in_lram_lila_tracts: f64,
    /// The definition, from ERS.
    pub definition: String,
    /// Source and section of the definition.
    pub citation: String,
}

/// One low-income/low-access tract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TractRow {
    pub geoid: String,
    /// "Census Tract 8204".
    pub name: String,
    /// The town holding most of the tract.
    pub town: Option<String>,
    pub urban: Option<bool>,
    pub population: Option<f64>,
    pub poverty_rate: Option<f64>,
    /// Definitions met (SRAM, the chosen distance).
    pub lila: Vec<String>,
    /// Definitions met in the 2019 LRAM (supermarkets), at least half the land area.
    pub lram_lila: Vec<String>,
    pub households: Option<f64>,
    pub households_no_vehicle: Option<f64>,
    pub households_snap: Option<f64>,
    /// Households without a vehicle more than 1/2 mile from a SNAP-authorized store.
    pub no_vehicle_households_beyond_half_mile: Option<f64>,
    /// CDC/ATSDR SVI 2022 overall percentile among Massachusetts tracts (0-1).
    pub svi_ma_overall: Option<f64>,
}

/// Area context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FoodContext {
    pub population: f64,
    pub households: f64,
    /// ERS low-income tracts and the people living in them.
    pub low_income_tracts: usize,
    pub people_in_low_income_tracts: f64,
    /// Housing units without a vehicle (ACS 2020-2024, from the SRAM file).
    pub households_no_vehicle: f64,
    /// Housing units receiving SNAP (ACS 2020-2024, from the SRAM file).
    pub households_snap: f64,
    /// Children and adults 65+ more than 1 mile from a SNAP-authorized store.
    pub children_beyond_1_mile: f64,
    pub older_adults_beyond_1_mile: f64,
    /// ACS percent in poverty and percent of households without a vehicle for the area.
    pub poverty_pct: Option<f64>,
    pub no_vehicle_pct: Option<f64>,
    /// Low-income/low-access tracts (any definition) in the top quartile of the SVI.
    pub lila_tracts_high_svi: usize,
}

/// Count of sites in one town.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownCount {
    pub town: String,
    pub sites: usize,
}

/// Count of sites of one kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TypeCount {
    pub kind: String,
    pub sites: usize,
}

/// Summary of one local resource file (no names or addresses).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceSummary {
    /// pantries, snap_retailers, or hip_vendors.
    pub kind: String,
    pub file: String,
    pub rows_read: usize,
    /// Sites in the area.
    pub in_area: usize,
    pub by_town: Vec<TownCount>,
    /// Counts by a fixed set of kinds (the file's own labels are not repeated).
    pub by_type: Vec<TypeCount>,
    /// Rows whose town was not a Massachusetts city or town (and had no usable ZIP or
    /// coordinates).
    pub town_not_recognized: usize,
    /// Rows left out: outside Massachusetts, or retailers with an end date.
    pub excluded: usize,
    /// Sites in the area with usable coordinates.
    pub with_coordinates: usize,
    /// Of those, sites inside a low-income/low-access tract (the mapped definition).
    pub in_lila_tracts: Option<usize>,
    /// Low-income/low-access tracts in the area with no site of this kind (needs
    /// coordinates).
    pub lila_tracts_without_site: Option<usize>,
    pub columns_used: Vec<String>,
}

/// Result of `food_access_mapper`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FoodAccessResult {
    pub area: Area,
    pub distance: Distance,
    /// The definition used for the tract list and map ("any" when none was chosen).
    pub mapped_definition: String,
    pub tracts: usize,
    pub definitions: Vec<DefinitionSummary>,
    pub context: FoodContext,
    /// Low-income/low-access tracts (at most 60 here; all are in the workbook).
    pub lila_tracts: Vec<TractRow>,
    pub lila_tracts_total: usize,
    pub resources: Vec<ResourceSummary>,
    pub hip: Vec<CitedOut>,
    pub snap: Vec<CitedOut>,
    /// How to get each local file, for files not given.
    pub files_you_can_add: Vec<String>,
    pub links: Vec<LinkDef>,
    pub methods: Vec<String>,
    pub map_files: Vec<String>,
    pub report_files: Vec<String>,
}

// ---------------------------------------------------------------- helpers

fn def_index(d: Definition) -> usize {
    d as usize
}

fn tract_meets(t: &FoodAccessTract, dist: Distance, def: Option<Definition>) -> bool {
    let m = t.measures(dist);
    match def {
        Some(d) => m.lila(d) == Some(true),
        None => Definition::ALL.iter().any(|d| m.lila(*d) == Some(true)),
    }
}

fn lram_meets(t: &FoodAccessTract, d: Definition) -> bool {
    t.lram_share[def_index(d)].is_some_and(|s| s >= LRAM_SHARE_THRESHOLD)
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

/// Refuse files that look like lists of people (never echoes values).
fn people_guard(lt: &crate::localdata::LocalTable, label: &str) -> Result<()> {
    const PERSON_HEADERS: [&str; 16] = [
        "first name",
        "last name",
        "firstname",
        "lastname",
        "client",
        "client name",
        "patient",
        "patient name",
        "participant",
        "recipient",
        "household member",
        "member",
        "dob",
        "date of birth",
        "birth date",
        "ssn",
    ];
    let bad_headers: Vec<&String> = lt
        .table
        .headers
        .iter()
        .filter(|h| {
            let n = phn_core::text::normalize(h);
            PERSON_HEADERS
                .iter()
                .any(|p| n == *p || n.starts_with(&format!("{p} ")))
        })
        .collect();
    let bad_kinds: Vec<&str> = lt
        .findings
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                Identifier::BirthDate
                    | Identifier::Ssn
                    | Identifier::MedicalRecordNumber
                    | Identifier::HealthPlanNumber
                    | Identifier::AccountNumber
            )
        })
        .map(|f| f.kind.label())
        .collect();
    if bad_headers.is_empty() && bad_kinds.is_empty() {
        return Ok(());
    }
    Err(PhnError::Privacy(format!(
        "'{}' looks like a list of people (columns {}{}), not a list of {label}, so it was not \
         read. List organizations and sites only: name, type, address, town, hours, phone. Keep \
         client lists out of this tool.",
        lt.label,
        bad_headers
            .iter()
            .map(|h| format!("'{h}'"))
            .collect::<Vec<_>>()
            .join(", "),
        if bad_kinds.is_empty() {
            String::new()
        } else {
            format!("; detected {}", bad_kinds.join(", "))
        }
    )))
}

/// Fixed kinds for each file (the file's own labels are never echoed).
fn kind_of(file: &str, raw: &str) -> &'static str {
    let w = crate::core::practice::padded_words(raw);
    let has = |k: &str| w.contains(&format!(" {k} "));
    match file {
        "pantries" => {
            if has("mobile") {
                "Mobile pantry"
            } else if has("meal") || has("meals") || has("kitchen") || has("soup") {
                "Meal site"
            } else if has("pantry") || has("pantries") || has("food") {
                "Food pantry"
            } else {
                "Other or not stated"
            }
        }
        "hip_vendors" => {
            if has("csa") || (has("farm") && has("share")) {
                "CSA farm share"
            } else if has("mobile") {
                "Mobile market"
            } else if has("stand") {
                "Farm stand"
            } else if has("market") || has("farmers") {
                "Farmers market"
            } else {
                "Other or not stated"
            }
        }
        _ => {
            if has("supermarket") || has("super") || has("supercenter") {
                "Supermarket or super store"
            } else if has("grocery") {
                "Grocery store"
            } else if has("convenience") {
                "Convenience store"
            } else if has("combination") {
                "Combination grocery/other"
            } else if has("farmers") || has("farm") {
                "Farmers market or farm"
            } else if has("specialty") || has("meat") || has("bakery") || has("fruits") {
                "Specialty store"
            } else {
                "Other or not stated"
            }
        }
    }
}

/// One site read from a local file (kept in memory for the output files only).
#[derive(Debug, Clone)]
struct Site {
    town: Option<String>,
    tract: Option<String>,
    kind: &'static str,
    /// Values for the output files, by field.
    fields: BTreeMap<String, String>,
}

struct LoadedResource {
    summary: ResourceSummary,
    sites: Vec<Site>,
    identifiers: Vec<String>,
    source: Source,
}

#[allow(clippy::too_many_arguments)]
fn load_resource(
    ctx: &Context,
    def: &LocalFileDef,
    path: &str,
    area_towns: &BTreeSet<String>,
    lila_in_area: &BTreeSet<String>,
    geos: &MaGeographies,
    tract_layer: &BoundaryLayer,
) -> Result<LoadedResource> {
    let lt = crate::localdata::load(ctx, path, None)?;
    people_guard(&lt, &def.label.to_lowercase())?;
    let norm = |s: &str| phn_core::text::normalize(s);
    let col = |field: &str| -> Option<usize> {
        let aliases = &def.columns.iter().find(|c| c.field == field)?.aliases;
        lt.table
            .headers
            .iter()
            .position(|h| aliases.iter().any(|a| norm(a) == norm(h)))
    };
    let cols: Vec<(String, Option<usize>)> = def
        .columns
        .iter()
        .map(|c| (c.field.clone(), col(&c.field)))
        .collect();
    let idx = |f: &str| cols.iter().find(|(n, _)| n == f).and_then(|(_, i)| *i);
    if idx("town").is_none() && idx("zip").is_none() && idx("latitude").is_none() {
        return Err(PhnError::InvalidInput(format!(
            "'{}' needs a town (or city) column, a ZIP code column, or latitude and longitude, \
             so sites can be placed. Columns found: {}",
            lt.label,
            lt.table.headers.join(", ")
        )));
    }
    let mut sites = Vec::new();
    let (mut rows_read, mut not_recognized, mut excluded) = (0usize, 0usize, 0usize);
    for row in &lt.table.rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        rows_read += 1;
        let get = |f: &str| -> String {
            idx(f)
                .and_then(|i| row.get(i))
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        if def.id == "snap_retailers" {
            let st = norm(&get("state"));
            if idx("state").is_some() && !st.is_empty() && st != "ma" && st != "massachusetts" {
                excluded += 1;
                continue;
            }
            if !get("end_date").is_empty() {
                excluded += 1;
                continue;
            }
        }
        // Place the site: coordinates, then town name, then ZIP.
        let lat = get("latitude").parse::<f64>().ok();
        let lon = get("longitude").parse::<f64>().ok();
        let mut tract = None;
        if let (Some(la), Some(lo)) = (lat, lon)
            && (41.0..43.2).contains(&la)
            && (-73.6..-69.8).contains(&lo)
        {
            let (x, y) = projection::to_mass_mainland(la, lo);
            tract = tract_layer
                .features
                .iter()
                .find(|f| {
                    let (x0, y0, x1, y1) = f.bbox();
                    x >= x0
                        && x <= x1
                        && y >= y0
                        && y <= y1
                        && f.polygons
                            .iter()
                            .any(|rings| projection::point_in_rings(x, y, rings))
                })
                .map(|f| f.id.clone());
        }
        let mut town = ctx.ma.town(&get("town")).map(|m| m.name.clone());
        if town.is_none() {
            let zip: String = get("zip")
                .chars()
                .filter(char::is_ascii_digit)
                .take(5)
                .collect();
            if zip.len() == 5 {
                town = geos
                    .get(&zip)
                    .filter(|g| g.level == phn_data::census::GeoLevel::Zcta)
                    .and_then(|g| g.town.clone());
            }
        }
        if town.is_none() {
            town = tract
                .as_ref()
                .and_then(|t| geos.get(t))
                .and_then(|g| g.town.clone());
        }
        if town.is_none() {
            not_recognized += 1;
            continue;
        }
        let fields = cols
            .iter()
            .filter(|(_, i)| i.is_some())
            .map(|(f, _)| (f.clone(), get(f)))
            .collect();
        sites.push(Site {
            town,
            tract,
            kind: kind_of(&def.id, &get("type")),
            fields,
        });
    }
    let in_area: Vec<&Site> = sites
        .iter()
        .filter(|s| s.town.as_ref().is_some_and(|t| area_towns.contains(t)))
        .collect();
    let mut by_town: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &in_area {
        if let Some(t) = &s.town {
            *by_town.entry(t.clone()).or_default() += 1;
        }
        *by_type.entry(s.kind).or_default() += 1;
    }
    let mut by_town: Vec<TownCount> = by_town
        .into_iter()
        .map(|(town, sites)| TownCount { town, sites })
        .collect();
    by_town.sort_by(|a, b| b.sites.cmp(&a.sites).then(a.town.cmp(&b.town)));
    let with_coords: Vec<&&Site> = in_area.iter().filter(|s| s.tract.is_some()).collect();
    let (in_lila, lila_without) = if with_coords.is_empty() {
        (None, None)
    } else {
        let in_lila = with_coords
            .iter()
            .filter(|s| s.tract.as_ref().is_some_and(|t| lila_in_area.contains(t)))
            .count();
        let served: BTreeSet<&String> = with_coords
            .iter()
            .filter_map(|s| s.tract.as_ref())
            .collect();
        let without = lila_in_area.iter().filter(|t| !served.contains(t)).count();
        (Some(in_lila), Some(without))
    };
    let summary = ResourceSummary {
        kind: def.id.clone(),
        file: lt.label.clone(),
        rows_read,
        in_area: in_area.len(),
        by_town,
        by_type: by_type
            .into_iter()
            .map(|(k, n)| TypeCount {
                kind: k.to_string(),
                sites: n,
            })
            .collect(),
        town_not_recognized: not_recognized,
        excluded,
        with_coordinates: with_coords.len(),
        in_lila_tracts: in_lila,
        lila_tracts_without_site: lila_without,
        columns_used: cols
            .iter()
            .filter(|(_, i)| i.is_some())
            .map(|(f, _)| f.clone())
            .collect(),
    };
    let in_area_sites: Vec<Site> = in_area.into_iter().cloned().collect();
    Ok(LoadedResource {
        summary,
        sites: in_area_sites,
        identifiers: lt.identifier_categories(),
        source: lt.source(),
    })
}

fn bbox_union(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

fn intersects(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

/// Draw the tract map: low income and low access, low income only, neither.
#[allow(clippy::too_many_arguments)]
fn draw_map(
    ctx: &Context,
    area: &Area,
    tracts: &[&FoodAccessTract],
    dist: Distance,
    def: Option<Definition>,
    geos: &MaGeographies,
    tract_layer: &BoundaryLayer,
    vintage: &str,
) -> Result<(Vec<String>, String)> {
    let colors = phn_docs::palette::sequential_steps(3);
    let mut fills: BTreeMap<String, Fill> = BTreeMap::new();
    let (mut n_lila, mut n_li, mut n_other, mut n_missing) = (0, 0, 0, 0);
    for t in tracts {
        let fill = if tract_meets(t, dist, def) {
            n_lila += 1;
            Fill::Class(2)
        } else if t.low_income == Some(true) {
            n_li += 1;
            Fill::Class(1)
        } else if t.low_income == Some(false) {
            n_other += 1;
            Fill::Class(0)
        } else {
            n_missing += 1;
            Fill::Suppressed
        };
        fills.insert(t.geoid.clone(), fill);
    }
    let data_bbox = fills
        .keys()
        .filter_map(|id| tract_layer.get(id))
        .map(Feature::bbox)
        .reduce(bbox_union)
        .ok_or_else(|| PhnError::Internal("no tract boundaries for the area".into()))?;
    let (w, h) = (data_bbox.2 - data_bbox.0, data_bbox.3 - data_bbox.1);
    let pad = (w.max(h) * 0.04).max(500.0);
    let extent = (
        data_bbox.0 - pad,
        data_bbox.1 - pad,
        data_bbox.2 + pad,
        data_bbox.3 + pad,
    );
    let mut features: Vec<(&Feature, Fill)> = Vec::new();
    for f in &tract_layer.features {
        if !intersects(f.bbox(), extent) {
            continue;
        }
        features.push((f, fills.get(&f.id).copied().unwrap_or(Fill::Context)));
    }
    let towns = BoundaryLayer::shared(&ctx.snapshots, Layer::Town)?;
    let overlay: Vec<&Feature> = {
        let t: Vec<&Feature> = towns
            .features
            .iter()
            .filter(|f| intersects(f.bbox(), extent))
            .collect();
        if t.len() <= 60 { t } else { Vec::new() }
    };
    let labels: Vec<(&Feature, String)> = if fills.len() <= 25 {
        fills
            .keys()
            .filter_map(|id| {
                let f = tract_layer.get(id)?;
                let name = geos.get(id)?.census_name.clone();
                Some((f, name.trim_start_matches("Census Tract ").to_string()))
            })
            .collect()
    } else {
        Vec::new()
    };
    let def_label = def
        .map(|d| d.label())
        .unwrap_or("any of the four definitions");
    let mut legend = vec![
        LegendRow {
            label: format!("Low income and low access ({n_lila})"),
            color: Some(colors[2].to_string()),
            hatched: false,
            outline_only: false,
        },
        LegendRow {
            label: format!("Low income, not low access ({n_li})"),
            color: Some(colors[1].to_string()),
            hatched: false,
            outline_only: false,
        },
        LegendRow {
            label: format!("Not low income ({n_other})"),
            color: Some(colors[0].to_string()),
            hatched: false,
            outline_only: false,
        },
    ];
    if n_missing > 0 {
        legend.push(LegendRow {
            label: format!("No data ({n_missing})"),
            color: None,
            hatched: true,
            outline_only: false,
        });
    }
    legend.push(LegendRow {
        label: "Outside the area".into(),
        color: None,
        hatched: false,
        outline_only: true,
    });
    let title = format!("Low-income and low-access census tracts, {}", area.name);
    let alt = format!(
        "Map of {} census tracts in {}. {n_lila} are low income and low access to a \
         SNAP-authorized store ({def_label}, {}), shown darkest; {n_li} are low income but not \
         low access (medium); {n_other} are not low income (lightest){}.",
        fills.len(),
        area.name,
        dist.label(),
        if n_missing > 0 {
            format!("; {n_missing} have no data (hatched)")
        } else {
            String::new()
        }
    );
    let spec = MapSpec {
        title: title.clone(),
        subtitle: Some(format!(
            "SNAP-authorized Retailer Access Map, {}; {def_label}",
            dist.label()
        )),
        legend_title: "Census tract".into(),
        legend,
        note: format!(
            "Source: USDA Economic Research Service, Food Access Research Atlas, \
             SNAP-authorized Retailer Access Map ({vintage}). Boundaries: U.S. Census Bureau \
             2020 tracts (simplified). Created {} with the Public Health Nursing MCP Server.",
            chrono::Local::now().date_naive()
        ),
        desc: alt.clone(),
        features,
        overlay,
        colors: colors.clone(),
        labels,
        extent,
    };
    let svg = render::render(&spec);
    let png = phn_docs::svg::svg_to_png(&svg, 2.0)?;
    let sandbox = ctx.sandbox()?;
    let dir = sandbox.output_dir()?.join("maps");
    std::fs::create_dir_all(&dir)?;
    let stem = format!("food_access_{}", slug(&area.name));
    let mut svg_path = dir.join(format!("{stem}.svg"));
    let mut n = 2;
    while svg_path.exists() {
        svg_path = dir.join(format!("{stem}_{n}.svg"));
        n += 1;
    }
    let png_path = svg_path.with_extension("png");
    std::fs::write(&svg_path, svg.as_bytes())?;
    std::fs::write(&png_path, &png)?;
    Ok((
        vec![
            sandbox.display_path(&svg_path),
            sandbox.display_path(&png_path),
        ],
        alt,
    ))
}

const SITE_FIELDS: [(&str, &str); 9] = [
    ("name", "Name"),
    ("type", "Type (as listed)"),
    ("address", "Address"),
    ("town", "Town"),
    ("zip", "ZIP"),
    ("hours", "Hours"),
    ("phone", "Phone"),
    ("website", "Website"),
    ("languages", "Languages"),
];

fn site_table(caption: &str, sites: &[Site], lila: &BTreeSet<String>) -> TableBlock {
    let present: Vec<(&str, &str)> = SITE_FIELDS
        .iter()
        .copied()
        .filter(|(f, _)| *f == "town" || sites.iter().any(|s| s.fields.contains_key(*f)))
        .collect();
    let mut headers: Vec<String> = present.iter().map(|(_, h)| h.to_string()).collect();
    headers.push("Kind".into());
    headers.push("In a low-income/low-access tract".into());
    let mut rows: Vec<Vec<String>> = sites
        .iter()
        .map(|s| {
            let mut r: Vec<String> = present
                .iter()
                .map(|(f, _)| {
                    if *f == "town" {
                        s.town.clone().unwrap_or_default()
                    } else {
                        s.fields.get(*f).cloned().unwrap_or_default()
                    }
                })
                .collect();
            r.push(s.kind.to_string());
            r.push(match &s.tract {
                Some(t) if lila.contains(t) => "Yes".into(),
                Some(_) => "No".into(),
                None => "Unknown (no coordinates)".into(),
            });
            r
        })
        .collect();
    rows.sort();
    TableBlock {
        caption: Some(caption.to_string()),
        headers,
        rows,
    }
}

fn tract_table_rows(rows: &[TractRow]) -> Vec<Vec<String>> {
    let n = |v: Option<f64>| v.map(|x| md::num(x, 0)).unwrap_or_default();
    rows.iter()
        .map(|t| {
            vec![
                t.geoid.clone(),
                t.name.clone(),
                t.town.clone().unwrap_or_default(),
                match t.urban {
                    Some(true) => "Urban".into(),
                    Some(false) => "Rural".into(),
                    None => String::new(),
                },
                n(t.population),
                t.poverty_rate.map(|p| format!("{p}%")).unwrap_or_default(),
                t.lila.join(", "),
                t.lram_lila.join(", "),
                n(t.households_no_vehicle),
                n(t.households_snap),
                t.svi_ma_overall
                    .map(|s| format!("{:.2}", s))
                    .unwrap_or_default(),
            ]
        })
        .collect()
}

const TRACT_HEADERS: [&str; 11] = [
    "GEOID",
    "Tract",
    "Town",
    "Urban/rural",
    "People (2020)",
    "Poverty rate",
    "Low income and low access (SRAM)",
    "Low income and low access (LRAM 2019, approx.)",
    "Households without a vehicle",
    "Households receiving SNAP",
    "SVI (MA percentile)",
];

/// Map food access for a town, county, or the state.
pub fn food_access_mapper(
    ctx: &Context,
    args: FoodAccessArgs,
) -> Result<Envelope<FoodAccessResult>> {
    let c: FoodContent = content::load(FOOD_ACCESS)?;
    let area = area::resolve(ctx, args.area.as_deref(), true)?;
    let dist = args.distance.unwrap_or_default();
    let def = args.definition;
    let fa = FoodAccess::shared(&ctx.snapshots)?;
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let svi = SviSnapshot::shared(&ctx.snapshots).ok();
    let tract_layer = BoundaryLayer::shared(&ctx.snapshots, Layer::Tract)?;
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = vec![fa.source.clone()];

    // Tracts in the area.
    let (tract_ids, shared): (Vec<String>, bool) = match area.level {
        AreaLevel::Town => {
            crate::equity::svi::town_tracts(geos, area.town.as_deref().unwrap_or(&area.name))
        }
        AreaLevel::County => (
            fa.tracts
                .iter()
                .filter(|t| Some(&t.geoid[..5]) == area.county_fips.as_deref())
                .map(|t| t.geoid.clone())
                .collect(),
            false,
        ),
        AreaLevel::State => (fa.tracts.iter().map(|t| t.geoid.clone()).collect(), false),
    };
    let tracts: Vec<&FoodAccessTract> = tract_ids.iter().filter_map(|g| fa.tract(g)).collect();
    if tracts.is_empty() {
        return Err(PhnError::NotFound(format!(
            "no Food Access Research Atlas tracts found for {}",
            area.name
        )));
    }
    if shared {
        caveats.push(crate::equity::svi::shared_tracts_caveat(
            &area.name,
            tracts.len(),
        ));
    }
    let area_towns: BTreeSet<String> = area::towns_in(ctx, &area).into_iter().collect();
    let lila_set: BTreeSet<String> = tracts
        .iter()
        .filter(|t| tract_meets(t, dist, def))
        .map(|t| t.geoid.clone())
        .collect();

    // Definition summaries.
    let sum = |it: &mut dyn Iterator<Item = Option<f64>>| -> f64 { it.flatten().sum() };
    let definitions: Vec<DefinitionSummary> = Definition::ALL
        .iter()
        .map(|d| {
            let lila: Vec<&&FoodAccessTract> = tracts
                .iter()
                .filter(|t| t.measures(dist).lila(*d) == Some(true))
                .collect();
            let lram: Vec<&&FoodAccessTract> =
                tracts.iter().filter(|t| lram_meets(t, *d)).collect();
            let dd = c.definitions.iter().find(|x| x.id == d.id());
            let text = dd.map(|x| x.text.clone()).unwrap_or_default();
            let citation = dd
                .map(|x| {
                    format!(
                        "{}, {}{}",
                        c.sources
                            .get(&x.source)
                            .map(|s| s.title.as_str())
                            .unwrap_or(x.source.as_str()),
                        x.section,
                        if x.confidence == Confidence::High {
                            ""
                        } else {
                            " (not high confidence)"
                        }
                    )
                })
                .unwrap_or_default();
            DefinitionSummary {
                id: d.id().into(),
                label: d.label().into(),
                lila_tracts: lila.len(),
                people_in_lila_tracts: sum(&mut lila.iter().map(|t| t.population)),
                low_access_people_in_lila_tracts: (*d != Definition::Vehicle)
                    .then(|| sum(&mut lila.iter().map(|t| t.measures(dist).low_access_people(*d)))),
                low_access_low_income_people_in_lila_tracts: (*d != Definition::Vehicle).then(
                    || {
                        sum(&mut lila
                            .iter()
                            .map(|t| t.measures(dist).low_access_low_income_people(*d)))
                    },
                ),
                lram_lila_tracts: lram.len(),
                people_in_lram_lila_tracts: sum(&mut lram.iter().map(|t| t.population)),
                definition: text,
                citation,
            }
        })
        .collect();

    // Context.
    let any_lila: Vec<&&FoodAccessTract> = tracts
        .iter()
        .filter(|t| tract_meets(t, dist, None))
        .collect();
    let svi_of = |g: &str| svi.and_then(|s| s.tract(g)).and_then(|t| t.ma.overall);
    let acs = AcsSnapshot::shared(&ctx.snapshots).ok();
    let acs_row = acs.and_then(|a| a.row(&area.geoid));
    let acs_pct = |id: &str| acs_row.and_then(|r| r.values.get(id)).map(|e| e.value);
    let context = FoodContext {
        population: sum(&mut tracts.iter().map(|t| t.population)),
        households: sum(&mut tracts.iter().map(|t| t.households)),
        low_income_tracts: tracts.iter().filter(|t| t.low_income == Some(true)).count(),
        people_in_low_income_tracts: sum(&mut tracts
            .iter()
            .filter(|t| t.low_income == Some(true))
            .map(|t| t.population)),
        households_no_vehicle: sum(&mut tracts.iter().map(|t| t.households_no_vehicle)),
        households_snap: sum(&mut tracts.iter().map(|t| t.households_snap)),
        children_beyond_1_mile: sum(&mut tracts.iter().map(|t| t.measures(dist).kids_beyond_1)),
        older_adults_beyond_1_mile: sum(&mut tracts
            .iter()
            .map(|t| t.measures(dist).seniors_beyond_1)),
        poverty_pct: acs_pct("poverty_pct"),
        no_vehicle_pct: acs_pct("no_vehicle_pct"),
        lila_tracts_high_svi: any_lila
            .iter()
            .filter(|t| svi_of(&t.geoid).is_some_and(|s| s >= HIGH_SVI))
            .count(),
    };
    if let Some(a) = acs
        && acs_row.is_some()
    {
        sources.push(a.source());
    }
    if let Some(s) = svi
        && context.lila_tracts_high_svi > 0
    {
        sources.push(s.source.clone());
    }

    // Tract rows.
    let mut all_rows: Vec<TractRow> = tracts
        .iter()
        .filter(|t| lila_set.contains(&t.geoid))
        .map(|t| {
            let g = geos.get(&t.geoid);
            TractRow {
                geoid: t.geoid.clone(),
                name: g.map(|g| g.census_name.clone()).unwrap_or_default(),
                town: g.and_then(|g| g.town.clone()),
                urban: t.urban,
                population: t.population,
                poverty_rate: t.poverty_rate,
                lila: Definition::ALL
                    .iter()
                    .filter(|d| t.measures(dist).lila(**d) == Some(true))
                    .map(|d| d.label().to_string())
                    .collect(),
                lram_lila: Definition::ALL
                    .iter()
                    .filter(|d| lram_meets(t, **d))
                    .map(|d| d.label().to_string())
                    .collect(),
                households: t.households,
                households_no_vehicle: t.households_no_vehicle,
                households_snap: t.households_snap,
                no_vehicle_households_beyond_half_mile: t
                    .measures(dist)
                    .no_vehicle_households_beyond_half,
                svi_ma_overall: svi_of(&t.geoid),
            }
        })
        .collect();
    all_rows.sort_by(|a, b| {
        b.lila
            .len()
            .cmp(&a.lila.len())
            .then(
                b.population
                    .unwrap_or(0.0)
                    .total_cmp(&a.population.unwrap_or(0.0)),
            )
            .then(a.geoid.cmp(&b.geoid))
    });
    let lila_tracts_total = all_rows.len();

    // Local resource files.
    let mut resources: Vec<ResourceSummary> = Vec::new();
    let mut loaded: Vec<(String, Vec<Site>)> = Vec::new();
    let mut identifiers: Vec<String> = Vec::new();
    let mut local_sources: Vec<Source> = Vec::new();
    for (id, path) in [
        ("pantries", &args.pantries_path),
        ("snap_retailers", &args.snap_retailers_path),
        ("hip_vendors", &args.hip_vendors_path),
    ] {
        let Some(p) = path else { continue };
        let fdef = c
            .local_files
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| PhnError::Internal(format!("local file {id} missing in content")))?;
        let lr = load_resource(ctx, fdef, p, &area_towns, &lila_set, geos, tract_layer)?;
        for i in lr.identifiers {
            if !identifiers.contains(&i) {
                identifiers.push(i);
            }
        }
        local_sources.push(lr.source);
        loaded.push((fdef.label.clone(), lr.sites));
        resources.push(lr.summary);
    }
    let files_you_can_add: Vec<String> = c
        .local_files
        .iter()
        .filter(|f| !resources.iter().any(|r| r.kind == f.id))
        .map(|f| {
            format!(
                "{} ({}_path): {}{}",
                f.label,
                f.id,
                f.how_to_get,
                match f.confidence {
                    Confidence::Low => " (low confidence: not checked)",
                    _ => "",
                }
            )
        })
        .collect();

    // Map.
    let mut map_files = Vec::new();
    let mut alt_text = None;
    if args.map.unwrap_or(area.level != AreaLevel::State) {
        let (files, alt) = draw_map(
            ctx,
            &area,
            &tracts,
            dist,
            def,
            geos,
            tract_layer,
            &fa.vintage,
        )?;
        map_files = files;
        alt_text = Some(alt);
        let tl = BoundaryLayer::shared(&ctx.snapshots, Layer::Tract)?;
        sources.push(tl.source.clone());
    }

    let hip = content::cite_all(&c.hip, &c.sources);
    let snap = content::cite_all(&c.snap, &c.sources);
    let background = content::cite_all(&c.background, &c.sources);
    let methods: Vec<String> = c
        .methods
        .iter()
        .map(|m| {
            format!(
                "{}{}",
                m.text,
                match m.confidence {
                    Confidence::High => "",
                    Confidence::Medium => " (this project's method)",
                    Confidence::Low => " (this project's method; low confidence)",
                }
            )
        })
        .collect();
    let mapped_definition = def
        .map(|d| d.id().to_string())
        .unwrap_or_else(|| "any".into());

    // Report files.
    let mut report_files = Vec::new();
    if args.report.unwrap_or(true) {
        let p = |t: &str| Block::Paragraph(vec![Inline::plain(t)]);
        let h = |t: &str| Block::Heading {
            level: 2,
            text: t.to_string(),
        };
        let list = |items: Vec<String>| Block::List {
            ordered: false,
            items: items.into_iter().map(|i| vec![Inline::plain(i)]).collect(),
        };
        let mut blocks = vec![p(&format!(
            "Prepared {} for {} from the USDA ERS Food Access Research Atlas ({}) and your \
             files. Site lists come from your files; verify hours before sharing.",
            chrono::Local::now().date_naive(),
            area.name,
            fa.vintage
        ))];
        blocks.push(h("Low-income and low-access census tracts"));
        blocks.push(Block::Table(TableBlock {
            caption: Some("Tracts by definition".into()),
            headers: [
                "Definition",
                "Tracts (SRAM)",
                "People in those tracts",
                "Tracts (LRAM 2019, approx.)",
            ]
            .map(String::from)
            .to_vec(),
            rows: definitions
                .iter()
                .map(|d| {
                    vec![
                        d.label.clone(),
                        d.lila_tracts.to_string(),
                        md::num(d.people_in_lila_tracts, 0),
                        d.lram_lila_tracts.to_string(),
                    ]
                })
                .collect(),
        }));
        blocks.push(Block::Table(TableBlock {
            caption: Some("Low-income and low-access tracts".into()),
            headers: TRACT_HEADERS.map(String::from).to_vec(),
            rows: tract_table_rows(&all_rows),
        }));
        for (label, sites) in &loaded {
            blocks.push(h(label));
            if sites.is_empty() {
                blocks.push(p("None of the sites in your file are in this area."));
            } else {
                blocks.push(Block::Table(site_table(label, sites, &lila_set)));
            }
        }
        blocks.push(h("Healthy Incentives Program (HIP)"));
        blocks.push(list(hip.iter().map(|x| x.text.clone()).collect()));
        blocks.push(h("SNAP"));
        blocks.push(list(snap.iter().map(|x| x.text.clone()).collect()));
        blocks.push(h("Links"));
        blocks.push(list(
            c.links
                .iter()
                .map(|l| format!("{}: {}", l.label, l.url))
                .collect(),
        ));
        blocks.push(h("Definitions and methods"));
        blocks.push(list(
            c.definitions
                .iter()
                .map(|d| format!("{}: {}", d.label, d.text))
                .chain(background.iter().map(|b| b.text.clone()))
                .chain(methods.iter().cloned())
                .collect(),
        ));
        let doc = Document {
            title: format!("Food access and food resources — {}", area.name),
            subtitle: Some(format!(
                "Low-income and low-access tracts ({}) and local food resources",
                dist.label()
            )),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks,
        };
        let sandbox = ctx.sandbox()?;
        let stem = format!("food_access_{}", slug(&area.name));
        let docx = sandbox.new_output_file(&format!("{stem}.docx"))?;
        std::fs::write(&docx, phn_docs::docx::to_docx(&doc)?)?;
        let xlsx = sandbox.new_output_file(&format!("{stem}.xlsx"))?;
        std::fs::write(&xlsx, phn_docs::xlsx::to_xlsx(&doc)?)?;
        report_files.push(sandbox.display_path(&docx));
        report_files.push(sandbox.display_path(&xlsx));
    }

    // Summary.
    let n0 = |x: f64| md::num(x, 0);
    let mut s = format!(
        "**Food access — {}** ({} census tracts; {}; USDA ERS Food Access Research Atlas)\n\n",
        area.name,
        tracts.len(),
        dist.label()
    );
    s.push_str(&md::table(
        &[
            "Definition",
            "Low-income/low-access tracts (any SNAP store, 2025)",
            "People in those tracts",
            "Low-access people (low-income)",
            "Supermarket access tracts (2019, approx.)",
        ],
        &definitions
            .iter()
            .map(|d| {
                vec![
                    d.label.clone(),
                    d.lila_tracts.to_string(),
                    n0(d.people_in_lila_tracts),
                    match (
                        d.low_access_people_in_lila_tracts,
                        d.low_access_low_income_people_in_lila_tracts,
                    ) {
                        (Some(a), Some(b)) => format!("{} ({})", n0(a), n0(b)),
                        _ => "—".into(),
                    },
                    format!(
                        "{} ({} people)",
                        d.lram_lila_tracts,
                        n0(d.people_in_lram_lila_tracts)
                    ),
                ]
            })
            .collect::<Vec<_>>(),
    ));
    s.push_str(&format!(
        "\n**Context:** {} people in {} households; {} low-income tract(s) ({} people); {} \
         households without a vehicle and {} receiving SNAP (ACS 2020-2024 in the Atlas); {} \
         children and {} adults 65+ live more than 1 mile from a SNAP-authorized store.{}{}\n\n",
        n0(context.population),
        n0(context.households),
        context.low_income_tracts,
        n0(context.people_in_low_income_tracts),
        n0(context.households_no_vehicle),
        n0(context.households_snap),
        n0(context.children_beyond_1_mile),
        n0(context.older_adults_beyond_1_mile),
        match (context.poverty_pct, context.no_vehicle_pct) {
            (Some(p), Some(v)) => format!(
                " ACS 5-year: {p}% of people below poverty; {v}% of households have no vehicle."
            ),
            _ => String::new(),
        },
        if context.lila_tracts_high_svi > 0 {
            format!(
                " {} low-income/low-access tract(s) are also in the top quarter of the \
                 Massachusetts SVI.",
                context.lila_tracts_high_svi
            )
        } else {
            String::new()
        }
    ));
    if all_rows.is_empty() {
        s.push_str(&format!(
            "No tract here is low income and low access to a SNAP-authorized store ({}, {}).\n\n",
            def.map(|d| d.label()).unwrap_or("any definition"),
            dist.label()
        ));
    } else {
        s.push_str(&format!(
            "**Low-income and low-access tracts** ({}{}):\n\n",
            def.map(|d| d.label()).unwrap_or("any definition"),
            if lila_tracts_total > MAX_LISTED_TRACTS {
                format!("; first {MAX_LISTED_TRACTS} of {lila_tracts_total}, all in the workbook")
            } else {
                String::new()
            }
        ));
        let shown: Vec<TractRow> = all_rows.iter().take(MAX_LISTED_TRACTS).cloned().collect();
        s.push_str(&md::table(
            &[
                "Tract",
                "Town",
                "People",
                "Poverty",
                "Definitions met (SRAM)",
                "No-vehicle households",
                "SVI",
            ],
            &shown
                .iter()
                .map(|t| {
                    vec![
                        format!(
                            "{} ({})",
                            t.name.trim_start_matches("Census Tract "),
                            t.geoid
                        ),
                        t.town.clone().unwrap_or_default(),
                        t.population.map(n0).unwrap_or_default(),
                        t.poverty_rate.map(|p| format!("{p}%")).unwrap_or_default(),
                        t.lila.join("; "),
                        t.households_no_vehicle.map(n0).unwrap_or_default(),
                        t.svi_ma_overall
                            .map(|v| format!("{v:.2}"))
                            .unwrap_or_default(),
                    ]
                })
                .collect::<Vec<_>>(),
        ));
        s.push('\n');
    }
    s.push_str("**Food resources**\n");
    if resources.is_empty() {
        s.push_str("- No local resource files were given (see below for what you can add).\n");
    }
    for r in &resources {
        let label = c
            .local_files
            .iter()
            .find(|f| f.id == r.kind)
            .map(|f| f.label.as_str())
            .unwrap_or(r.kind.as_str());
        let towns: Vec<String> = r
            .by_town
            .iter()
            .take(12)
            .map(|t| format!("{} {}", t.town, t.sites))
            .collect();
        s.push_str(&format!(
            "- {label} (`{}`): {} site(s) in {} of {} read{}{}{}{}.\n",
            r.file,
            r.in_area,
            area.name,
            r.rows_read,
            if towns.is_empty() {
                String::new()
            } else {
                format!(
                    "; by town: {}{}",
                    towns.join(", "),
                    if r.by_town.len() > 12 { ", ..." } else { "" }
                )
            },
            if r.by_type.is_empty() {
                String::new()
            } else {
                format!(
                    "; by kind: {}",
                    r.by_type
                        .iter()
                        .map(|t| format!("{} {}", t.kind, t.sites))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            match (r.in_lila_tracts, r.lila_tracts_without_site) {
                (Some(a), Some(b)) => format!(
                    "; {a} inside a low-income/low-access tract; {b} such tract(s) have none"
                ),
                _ => String::new(),
            },
            if r.town_not_recognized + r.excluded > 0 {
                format!(
                    " ({} row(s) not placed in a Massachusetts town, {} left out as out of state \
                     or no longer authorized)",
                    r.town_not_recognized, r.excluded
                )
            } else {
                String::new()
            }
        ));
    }
    s.push_str("\n**Healthy Incentives Program (HIP)**\n");
    for x in &hip {
        s.push_str(&content::bullet(x));
    }
    s.push_str("\n**SNAP**\n");
    for x in &snap {
        s.push_str(&content::bullet(x));
    }
    if !files_you_can_add.is_empty() {
        s.push_str("\n**Files you can add**\n");
        for f in &files_you_can_add {
            s.push_str(&format!("- {f}\n"));
        }
    }
    if !map_files.is_empty() {
        s.push_str(&format!(
            "\nMap written to /data/{} and /data/{}.\n",
            map_files[0],
            map_files.get(1).cloned().unwrap_or_default()
        ));
        if let Some(a) = &alt_text {
            s.push_str(&format!("**Alt text:** {a}\n"));
        }
    }
    if !report_files.is_empty() {
        s.push_str(&format!(
            "\nResource list (DOCX) and workbook (XLSX) written to {}{}.\n",
            report_files.join(" and "),
            if loaded.is_empty() {
                ""
            } else {
                "; site names and addresses from your files are only there"
            }
        ));
    }

    // Caveats.
    caveats.push(
        "The 2025 SNAP-authorized Retailer Access Map counts any SNAP-authorized store \
         (including convenience stores) as a food store, so it finds fewer low-access tracts \
         than the 2019 supermarket-based map; distance is not the only barrier (cost, hours, \
         transportation, culturally appropriate food)."
            .into(),
    );
    caveats.push(format!(
        "2019 LRAM figures are on 2010 tracts and were carried to 2020 tracts by land area; a \
         2020 tract is counted when at least {}% of its land was in a 2010 low-income/low-access \
         tract (tool default). Treat them as approximate.",
        (LRAM_SHARE_THRESHOLD * 100.0).round()
    ));
    caveats.push(
        "Tract counts are ERS estimates from different sources (2020 Census, LandScan 2020, ACS \
         2020-2024), so low-income counts can differ from other ACS tables."
            .into(),
    );
    if resources.iter().any(|r| r.kind == "snap_retailers") {
        caveats.push(
            "SNAP retailer rows with an end date were left out as no longer authorized.".into(),
        );
    }
    if !resources.is_empty() {
        caveats.push(
            "Resource counts come from your files; check that they are current before sharing."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[FOOD_ACCESS]));

    // Sources.
    let mut keys: Vec<&str> = c.definitions.iter().map(|d| d.source.as_str()).collect();
    keys.extend(c.background.iter().map(|b| b.source.as_str()));
    keys.extend(c.hip.iter().map(|b| b.source.as_str()));
    keys.extend(c.snap.iter().map(|b| b.source.as_str()));
    keys.push("census-tract-relationship");
    sources.extend(content::sources_for(keys, &c.sources));
    sources.push(Source::computed(
        "Tract counts, LRAM 2010-to-2020 land-area carry-over, and resource counts by town",
        "PHN MCP server calculation",
    ));
    sources.extend(local_sources);

    let mut files_written = map_files.clone();
    files_written.extend(report_files.iter().cloned());
    let mut notes = Vec::new();
    if !loaded.is_empty() {
        notes.push(
            "Sites from your files are counted by town and kind only; names, addresses, and \
             phone numbers were written to the output files, not returned."
                .into(),
        );
    }
    let listed: Vec<TractRow> = all_rows.into_iter().take(MAX_LISTED_TRACTS).collect();
    let _ = food_access::ATLAS_URL;
    Ok(Envelope::new(
        s,
        FoodAccessResult {
            area,
            distance: dist,
            mapped_definition,
            tracts: tracts.len(),
            definitions,
            context,
            lila_tracts: listed,
            lila_tracts_total,
            resources,
            hip,
            snap,
            files_you_can_add,
            links: c.links.clone(),
            methods,
            map_files,
            report_files,
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
    }))
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

    #[test]
    fn kinds_use_fixed_labels() {
        assert_eq!(kind_of("pantries", "Mobile Pantry"), "Mobile pantry");
        assert_eq!(kind_of("pantries", "Community meal"), "Meal site");
        assert_eq!(kind_of("pantries", "Jane's list"), "Other or not stated");
        assert_eq!(kind_of("hip_vendors", "Farmers Market"), "Farmers market");
        assert_eq!(kind_of("hip_vendors", "CSA"), "CSA farm share");
        assert_eq!(
            kind_of("snap_retailers", "Convenience Store"),
            "Convenience store"
        );
        assert_eq!(
            kind_of("snap_retailers", "Super Store"),
            "Supermarket or super store"
        );
    }

    #[test]
    fn state_town_and_county_summaries_offline() {
        let (dir, c) = ctx();
        let state = food_access_mapper(
            &c,
            FoodAccessArgs {
                report: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(state.data.area.level, AreaLevel::State);
        assert!(state.data.tracts > 1600);
        assert!(state.data.map_files.is_empty());
        let half = &state.data.definitions[1];
        assert!(half.lila_tracts > state.data.definitions[0].lila_tracts);
        assert!(state.data.lila_tracts.len() <= MAX_LISTED_TRACTS);
        assert!(state.summary.contains("HIP"));
        assert!(state.summary.contains("(877) 382-2363"));
        let town = food_access_mapper(
            &c,
            FoodAccessArgs {
                area: Some("Springfield".into()),
                distance: Some(Distance::Driving),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(town.data.map_files.len(), 2);
        assert_eq!(town.data.report_files.len(), 2);
        for f in town.data.map_files.iter().chain(&town.data.report_files) {
            assert!(dir.path().join("data").join(f).exists(), "{f}");
        }
        assert!(town.data.files_you_can_add.len() == 3);
        let county = food_access_mapper(
            &c,
            FoodAccessArgs {
                area: Some("Hampden County".into()),
                definition: Some(Definition::Vehicle),
                map: Some(false),
                report: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(county.data.mapped_definition, "vehicle");
        assert!(county.data.tracts > town.data.tracts);
        assert!(
            food_access_mapper(
                &c,
                FoodAccessArgs {
                    area: Some("Springfeld".into()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn pantry_file_is_counted_and_written_only_to_files() {
        let (dir, c) = ctx();
        std::fs::write(
            dir.path().join("data/pantries.csv"),
            "Organization,Type,Address,Town,Hours,Phone,Latitude,Longitude\n\
             Example Pantry One,Food pantry,1 Main St,Springfield,Tue 9-12,413-555-0190,42.1015,-72.5898\n\
             Example Meals,Community meal,2 Elm St,Springfield,Mon 11-1,413-555-0191,,\n\
             Example Mobile,Mobile pantry,3 Oak St,Holyoke,Wed 3-5,413-555-0192,,\n\
             Elsewhere Pantry,Food pantry,4 Pine St,Nowhere Town,,,,\n",
        )
        .unwrap();
        let env = food_access_mapper(
            &c,
            FoodAccessArgs {
                area: Some("Springfield".into()),
                pantries_path: Some("pantries.csv".into()),
                map: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let r = &env.data.resources[0];
        assert_eq!(r.rows_read, 4);
        assert_eq!(r.in_area, 2);
        assert_eq!(r.town_not_recognized, 1);
        assert_eq!(r.with_coordinates, 1);
        assert!(r.in_lila_tracts.is_some());
        assert!(r.by_type.iter().any(|t| t.kind == "Meal site"));
        let all = serde_json::to_string(&env.data).unwrap() + &env.summary;
        for secret in [
            "Example Pantry One",
            "1 Main St",
            "413-555-0190",
            "Nowhere Town",
        ] {
            assert!(!all.contains(secret), "{secret} leaked");
        }
        assert!(
            env.privacy
                .files_written
                .iter()
                .any(|f| f.ends_with(".xlsx"))
        );
    }

    #[test]
    fn client_lists_are_refused_without_echoing_values() {
        let (dir, c) = ctx();
        std::fs::write(
            dir.path().join("data/clients.csv"),
            "Client Name,DOB,Town\nAnselma Quiddington,1950-02-03,Springfield\n",
        )
        .unwrap();
        for args in [
            FoodAccessArgs {
                pantries_path: Some("clients.csv".into()),
                ..Default::default()
            },
            FoodAccessArgs {
                hip_vendors_path: Some("clients.csv".into()),
                ..Default::default()
            },
            FoodAccessArgs {
                snap_retailers_path: Some("clients.csv".into()),
                ..Default::default()
            },
        ] {
            let e = food_access_mapper(
                &c,
                FoodAccessArgs {
                    area: Some("Springfield".into()),
                    map: Some(false),
                    report: Some(false),
                    ..args
                },
            )
            .unwrap_err()
            .to_string();
            assert!(e.contains("list of people"), "{e}");
            assert!(!e.contains("Quiddington") && !e.contains("1950"));
        }
    }
}
