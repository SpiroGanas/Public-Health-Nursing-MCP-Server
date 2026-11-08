//! EPI-12 `create_choropleth_map`.

pub mod classify;
pub mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::boundaries::{BoundaryLayer, Feature, Layer};
use phn_data::census::GeoLevel;
use phn_data::geography::MaGeographies;
use phn_local::phi;
use phn_local::table::Table;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
pub use classify::Method;
use render::{Fill, LegendRow, MapSpec};

/// Map geography.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MapGeography {
    /// Cities and towns, by name ("Amherst") or 10-digit Census GEOID.
    Town,
    /// 2020 census tracts, by 11-digit GEOID (e.g. "25015820400").
    Tract,
    /// ZIP Code Tabulation Areas, by 5-digit code (e.g. "01002").
    Zcta,
    /// Public school districts, by DESE 8-digit organization code or name.
    SchoolDistrict,
}

impl MapGeography {
    fn layer(self) -> Layer {
        match self {
            Self::Town => Layer::Town,
            Self::Tract => Layer::Tract,
            Self::Zcta => Layer::Zcta,
            Self::SchoolDistrict => Layer::SchoolDistrict,
        }
    }

    fn plural(self) -> &'static str {
        match self {
            Self::Town => "cities and towns",
            Self::Tract => "census tracts",
            Self::Zcta => "ZIP Code Tabulation Areas",
            Self::SchoolDistrict => "school districts",
        }
    }
}

/// Map extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Extent {
    /// Zoom to the areas in the data (default).
    #[default]
    Data,
    /// Show all of Massachusetts.
    State,
}

/// One area's value.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AreaValue {
    /// Area name or code (town name, tract GEOID, ZCTA, or district code/name).
    pub area: String,
    /// The value to map (rate, percent, or count). Omit or null for no data.
    #[serde(default)]
    pub value: Option<f64>,
    /// Number of events behind the value, if known. Areas with a count from 1
    /// to the suppression threshold − 1 are hatched instead of colored.
    #[serde(default)]
    pub count: Option<u64>,
}

/// Arguments for `create_choropleth_map`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateChoroplethArgs {
    /// Geography of the areas.
    pub geography: MapGeography,
    /// Map title, e.g. "Pertussis rate per 100,000, 2024".
    pub title: String,
    /// Values given inline (aggregated data only). Give either `values` or `path`.
    #[serde(default)]
    pub values: Option<Vec<AreaValue>>,
    /// An aggregated CSV/XLSX table in /data with one row per area. Row-level
    /// (record) files are refused.
    #[serde(default)]
    pub path: Option<String>,
    /// Sheet name for spreadsheets.
    #[serde(default)]
    pub sheet: Option<String>,
    /// Column with area names or codes (default: detected).
    #[serde(default)]
    pub area_column: Option<String>,
    /// Column with the values to map (default: the first numeric column).
    #[serde(default)]
    pub value_column: Option<String>,
    /// Column with event counts behind each value, used for suppression.
    #[serde(default)]
    pub count_column: Option<String>,
    /// The mapped values are themselves counts (small counts are suppressed). Default false.
    #[serde(default)]
    pub value_is_count: Option<bool>,
    /// Subtitle, e.g. the time period.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Legend title, e.g. "Rate per 100,000" (default "Value").
    #[serde(default)]
    pub value_label: Option<String>,
    /// natural_breaks (default) or quantile.
    #[serde(default)]
    pub method: Option<Method>,
    /// Number of classes, 3–7 (default 5; fewer when there are few distinct values).
    #[serde(default)]
    pub classes: Option<u8>,
    /// data (zoom to the areas, default) or state.
    #[serde(default)]
    pub extent: Option<Extent>,
    /// Label areas with their names (default: when 25 or fewer areas are mapped).
    #[serde(default)]
    pub labels: Option<bool>,
    /// Decimal places in the legend (default: automatic).
    #[serde(default)]
    pub decimals: Option<u8>,
    /// Data source to print under the map, e.g. "MAVEN, accessed 2026-11-01".
    #[serde(default)]
    pub source_note: Option<String>,
    /// Base file name (default: from the title).
    #[serde(default)]
    pub file_name: Option<String>,
}

/// One class in the legend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClassRow {
    /// 1 = lowest.
    pub class: usize,
    /// Lowest value in the class.
    pub lower: f64,
    /// Highest value in the class.
    pub upper: f64,
    pub color: String,
    /// Number of areas in the class.
    pub areas: usize,
}

/// One mapped area (inline input only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MappedArea {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Class number (1 = lowest), or none when suppressed or missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<usize>,
    /// mapped, suppressed, or no_data.
    pub status: String,
}

/// Result of `create_choropleth_map`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChoroplethResult {
    pub geography: MapGeography,
    pub title: String,
    pub method: String,
    pub classes: Vec<ClassRow>,
    /// Goodness of variance fit of the classes (1 = perfect).
    pub goodness_of_variance_fit: f64,
    pub mapped_areas: usize,
    pub suppressed_areas: usize,
    pub no_data_areas: usize,
    /// SVG and PNG files written to the output folder (paths relative to /data).
    pub files: Vec<String>,
    /// Alternative text for the image (also embedded in the SVG `<desc>`).
    pub alt_text: String,
    /// Per-area classes (inline values only; not returned for local files).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub areas: Option<Vec<MappedArea>>,
    pub suppression_threshold: u32,
}

#[derive(Debug, Clone)]
struct Row {
    label: String,
    value: Option<f64>,
    count: Option<u64>,
    suppressed_marker: bool,
}

fn is_total(s: &str) -> bool {
    let n = s.trim().to_ascii_lowercase();
    matches!(n.as_str(), "total" | "totals" | "all" | "massachusetts" | "state" | "statewide")
        || n.starts_with("total ")
}

fn parse_value(s: &str) -> std::result::Result<Option<f64>, ()> {
    let t = s.trim().replace([',', '%', '$'], "");
    if t.is_empty() || phi::is_missing(&t) || t == "-" || t.eq_ignore_ascii_case("na") {
        return Ok(None);
    }
    t.parse::<f64>().map(Some).map_err(|_| ())
}

fn is_suppressed_marker(s: &str) -> bool {
    let t = s.trim();
    matches!(t, "*" | "**" | "†" | "s" | "S") || t.starts_with('<')
}

/// Resolve an area label to a boundary feature id and name.
fn resolve_area(
    ctx: &Context,
    geos: &MaGeographies,
    geography: MapGeography,
    label: &str,
) -> std::result::Result<(String, String), String> {
    let l = label.trim();
    match geography {
        MapGeography::Town => {
            let digits: String = l.chars().filter(|c| c.is_ascii_digit()).collect();
            let g = if digits.len() == 10 && digits.len() == l.len() {
                geos.get(&digits).filter(|g| g.level == GeoLevel::Town)
            } else {
                geos.town(l)
            };
            g.map(|g| (g.name.clone(), g.name.clone())).ok_or_else(|| {
                let s = geos.suggest_towns(l, 2);
                if s.is_empty() {
                    format!("'{l}'")
                } else {
                    format!("'{l}' (did you mean {}?)", s.join(" or "))
                }
            })
        }
        MapGeography::Tract => geos
            .tract(l)
            .map(|g| (g.geoid.clone(), g.census_name.clone()))
            .ok_or_else(|| format!("'{l}' (use the 11-digit tract GEOID)")),
        MapGeography::Zcta => {
            let digits: String = l.chars().take_while(|c| *c != '-').filter(|c| c.is_ascii_digit()).collect();
            let code = format!("{digits:0>5}");
            geos.get(&code)
                .filter(|g| g.level == GeoLevel::Zcta)
                .map(|g| (g.geoid.clone(), g.geoid.clone()))
                .ok_or_else(|| format!("'{l}' (not a Massachusetts ZCTA)"))
        }
        MapGeography::SchoolDistrict => {
            let digits: String = l.chars().filter(|c| c.is_ascii_digit()).collect();
            let code = if digits.len() == l.len() && !digits.is_empty() {
                format!("{digits:0>8}")
            } else {
                l.to_string()
            };
            ctx.ma
                .district(&code)
                .map(|d| (d.org_code.clone(), d.name.clone()))
                .ok_or_else(|| {
                    let s: Vec<String> = ctx
                        .ma
                        .suggest_districts(l, 2)
                        .into_iter()
                        .map(|d| d.name.clone())
                        .collect();
                    if s.is_empty() {
                        format!("'{l}'")
                    } else {
                        format!("'{l}' (did you mean {}?)", s.join(" or "))
                    }
                })
        }
    }
}

fn load_file(
    ctx: &Context,
    args: &CreateChoroplethArgs,
    geos: &MaGeographies,
) -> Result<(Vec<Row>, String)> {
    let path = args.path.as_deref().unwrap_or_default();
    let lt = crate::localdata::load(ctx, path, args.sheet.as_deref())?;
    let table = &lt.table;
    let label = lt.label.clone();
    if table.rows.len() > 5_000 {
        return Err(PhnError::Privacy(
            "this looks like record-level data (over 5,000 rows). Maps need one row per area; \
             aggregate first (for example with profile_local_dataset or a pivot table)."
                .into(),
        ));
    }
    // The area column may hold town names that look like surnames, so it is
    // found by name here and checked below; value and count columns go
    // through the shared guardrail, which refuses identifier columns.
    let area_col = match &args.area_column {
        Some(n) => table.column_index(n).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "area column '{n}' not found; columns are: {}",
                table.headers.join(", ")
            ))
        })?,
        None => detect_area_column(table),
    };
    let col = |name: &Option<String>| -> Result<Option<usize>> {
        name.as_deref().map(|n| lt.column(n)).transpose()
    };
    // Privacy guard: refuse record-level files. The area column is exempt when
    // its values are recognized geographies (town names can look like surnames).
    let area_values_known = {
        let vals: Vec<&str> = table.column(area_col).filter(|v| !v.trim().is_empty()).collect();
        let known = vals
            .iter()
            .filter(|v| is_total(v) || resolve_area(ctx, geos, args.geography, v).is_ok())
            .count();
        !vals.is_empty() && known * 10 >= vals.len() * 9
    };
    let direct: Vec<String> = lt
        .findings
        .iter()
        .filter(|f| f.kind.is_direct() && f.kind != phi::Identifier::FreeText)
        .filter(|f| !(area_values_known && f.index == area_col))
        .map(|f| format!("{} ({})", f.column, f.kind.label()))
        .collect();
    if !direct.is_empty() {
        return Err(PhnError::Privacy(format!(
            "this looks like record-level data with identifiers ({}). Maps take one row per area \
             with a rate, percent, or count; aggregate first.",
            direct.join(", ")
        )));
    }
    let value_col = match col(&args.value_column)? {
        Some(c) => c,
        None => (0..table.headers.len())
            .filter(|c| *c != area_col)
            .find(|c| {
                let vals: Vec<&str> = table.column(*c).filter(|v| !v.trim().is_empty()).collect();
                !vals.is_empty()
                    && vals
                        .iter()
                        .all(|v| parse_value(v).is_ok() || is_suppressed_marker(v))
            })
            .ok_or_else(|| {
                PhnError::InvalidInput(
                    "no numeric value column found; name it with value_column".into(),
                )
            })?,
    };
    let count_col = col(&args.count_column)?;
    let mut rows = Vec::new();
    let mut bad = Vec::new();
    for (i, r) in table.rows.iter().enumerate() {
        let label = r.get(area_col).cloned().unwrap_or_default();
        if label.trim().is_empty() {
            continue;
        }
        let raw = r.get(value_col).map(String::as_str).unwrap_or("");
        let suppressed_marker = is_suppressed_marker(raw);
        let value = if suppressed_marker {
            None
        } else {
            match parse_value(raw) {
                Ok(v) => v,
                Err(()) => {
                    bad.push(format!("row {}: '{raw}'", i + 2));
                    None
                }
            }
        };
        let count = count_col
            .and_then(|c| r.get(c))
            .and_then(|v| parse_value(v).ok().flatten())
            .filter(|v| *v >= 0.0)
            .map(|v| v.round() as u64);
        rows.push(Row {
            label,
            value,
            count,
            suppressed_marker,
        });
    }
    if !bad.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "values must be numbers: {}",
            bad.into_iter().take(5).collect::<Vec<_>>().join("; ")
        )));
    }
    Ok((rows, label))
}

fn detect_area_column(table: &Table) -> usize {
    const NAMES: [&str; 12] = [
        "town", "city", "municipality", "community", "tract", "geoid", "zcta", "zip", "zip code",
        "district", "school district", "area",
    ];
    table
        .headers
        .iter()
        .position(|h| NAMES.contains(&h.trim().to_ascii_lowercase().as_str()))
        .unwrap_or(0)
}

fn auto_decimals(values: &[f64]) -> usize {
    if values.iter().all(|v| v.fract() == 0.0) {
        return 0;
    }
    let max = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if max < 1.0 {
        2
    } else if max < 1000.0 {
        1
    } else {
        0
    }
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
    let t = out.trim_matches('_').to_string();
    let t: String = t.chars().take(60).collect();
    if t.is_empty() { "map".into() } else { t }
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let clean = phn_local::sandbox::sanitize_file_name(stem);
    let first = dir.join(format!("{clean}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|n| dir.join(format!("{clean}-{n}.{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn bbox_union(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

fn intersects(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

/// Map an indicator by town, tract, ZCTA, or school district.
pub fn create_choropleth_map(
    ctx: &Context,
    args: CreateChoroplethArgs,
) -> Result<Envelope<ChoroplethResult>> {
    let title = args.title.trim().to_string();
    if title.is_empty() {
        return Err(PhnError::InvalidInput("give the map a title".into()));
    }
    let k = usize::from(args.classes.unwrap_or(5));
    if !(3..=7).contains(&k) {
        return Err(PhnError::InvalidInput("classes must be between 3 and 7".into()));
    }
    let method = args.method.unwrap_or_default();
    let threshold = ctx.suppression_threshold();
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let layer = BoundaryLayer::shared(&ctx.snapshots, args.geography.layer())?;

    let (rows, local_label) = match (&args.values, &args.path) {
        (Some(v), None) => {
            if v.is_empty() {
                return Err(PhnError::InvalidInput("values is empty".into()));
            }
            if v.len() > 5_000 {
                return Err(PhnError::InvalidInput("at most 5,000 areas per map".into()));
            }
            (
                v.iter()
                    .map(|a| Row {
                        label: a.area.clone(),
                        value: a.value,
                        count: a.count,
                        suppressed_marker: false,
                    })
                    .collect::<Vec<_>>(),
                None,
            )
        }
        (None, Some(_)) => {
            let (r, l) = load_file(ctx, &args, geos)?;
            (r, Some(l))
        }
        _ => {
            return Err(PhnError::InvalidInput(
                "give either `values` (inline) or `path` (an aggregated file in /data)".into(),
            ));
        }
    };

    // Resolve areas.
    let mut unmatched = Vec::new();
    let mut ignored_totals = 0;
    let mut by_id: BTreeMap<String, (String, Option<f64>, bool)> = BTreeMap::new(); // id → (name, value, suppressed)
    let mut duplicates = BTreeSet::new();
    let value_is_count = args.value_is_count.unwrap_or(false);
    for r in &rows {
        if is_total(&r.label) {
            ignored_totals += 1;
            continue;
        }
        match resolve_area(ctx, geos, args.geography, &r.label) {
            Ok((id, name)) => {
                let small = |c: u64| c > 0 && c < u64::from(threshold);
                let suppressed = r.suppressed_marker
                    || r.count.is_some_and(small)
                    || (value_is_count
                        && r.value.is_some_and(|v| v > 0.0 && v < f64::from(threshold)));
                let value = r.value.filter(|v| v.is_finite());
                if by_id.insert(id.clone(), (name, value, suppressed)).is_some() {
                    duplicates.insert(id);
                }
            }
            Err(e) => unmatched.push(e),
        }
    }
    if !duplicates.is_empty() {
        let list: Vec<String> = duplicates.into_iter().take(5).collect();
        return Err(if local_label.is_some() {
            PhnError::Privacy(format!(
                "several rows share the same area ({}); this looks like record-level data. Maps \
                 take one row per area; aggregate first.",
                list.join(", ")
            ))
        } else {
            PhnError::InvalidInput(format!("areas listed more than once: {}", list.join(", ")))
        });
    }
    if !unmatched.is_empty() {
        return Err(PhnError::InvalidInput(format!(
            "{} area(s) are not Massachusetts {}: {}",
            unmatched.len(),
            args.geography.plural(),
            unmatched.into_iter().take(8).collect::<Vec<_>>().join(", ")
        )));
    }
    if by_id.is_empty() {
        return Err(PhnError::InvalidInput("no areas to map".into()));
    }
    let mut missing_shapes = Vec::new();
    by_id.retain(|id, (name, _, _)| {
        let ok = layer.get(id).is_some();
        if !ok {
            missing_shapes.push(name.clone());
        }
        ok
    });

    let mapped_values: Vec<f64> = by_id
        .values()
        .filter(|(_, v, s)| !s && v.is_some())
        .filter_map(|(_, v, _)| *v)
        .collect();
    if mapped_values.is_empty() {
        return Err(PhnError::InvalidInput(
            "no area has a value that can be mapped (all are missing or suppressed)".into(),
        ));
    }
    let breaks = classify::breaks(&mapped_values, k, method);
    let k_used = breaks.len();
    let colors = phn_docs::palette::sequential_steps(k_used);
    let gvf = classify::goodness_of_variance_fit(&mapped_values, &breaks);

    // Class membership.
    let mut class_members: Vec<Vec<(String, f64)>> = vec![Vec::new(); k_used];
    let mut areas = Vec::new();
    let (mut suppressed_n, mut no_data_n) = (0, 0);
    let mut fills: BTreeMap<String, Fill> = BTreeMap::new();
    for (id, (name, value, suppressed)) in &by_id {
        let (fill, class, status) = match (suppressed, value) {
            (true, _) => {
                suppressed_n += 1;
                (Fill::Suppressed, None, "suppressed")
            }
            (false, None) => {
                no_data_n += 1;
                (Fill::Suppressed, None, "no_data")
            }
            (false, Some(v)) => {
                let c = classify::class_of(*v, &breaks);
                class_members[c].push((name.clone(), *v));
                (Fill::Class(c), Some(c + 1), "mapped")
            }
        };
        fills.insert(id.clone(), fill);
        areas.push(MappedArea {
            id: id.clone(),
            name: name.clone(),
            value: if *suppressed { None } else { *value },
            class,
            status: status.into(),
        });
    }
    let decimals = args
        .decimals
        .map(usize::from)
        .unwrap_or_else(|| auto_decimals(&mapped_values));
    let classes: Vec<ClassRow> = class_members
        .iter()
        .enumerate()
        .map(|(i, m)| ClassRow {
            class: i + 1,
            lower: m.iter().map(|x| x.1).fold(f64::INFINITY, f64::min),
            upper: m.iter().map(|x| x.1).fold(f64::NEG_INFINITY, f64::max),
            color: colors[i].to_string(),
            areas: m.len(),
        })
        .collect();

    // Extent.
    let data_bbox = fills
        .keys()
        .filter_map(|id| layer.get(id))
        .map(Feature::bbox)
        .reduce(bbox_union)
        .expect("at least one feature");
    let extent = match args.extent.unwrap_or_default() {
        Extent::State => BoundaryLayer::shared(&ctx.snapshots, Layer::Town)?
            .features
            .iter()
            .map(Feature::bbox)
            .reduce(bbox_union)
            .expect("towns"),
        Extent::Data => data_bbox,
    };
    let (w, h) = (extent.2 - extent.0, extent.3 - extent.1);
    let pad = (w.max(h) * 0.04).max(500.0);
    let extent = (extent.0 - pad, extent.1 - pad, extent.2 + pad, extent.3 + pad);

    let mut features: Vec<(&Feature, Fill)> = Vec::new();
    let mut context_n = 0;
    for f in &layer.features {
        if !intersects(f.bbox(), extent) {
            continue;
        }
        match fills.get(&f.id) {
            Some(fill) => features.push((f, *fill)),
            None => {
                context_n += 1;
                features.push((f, Fill::Context));
            }
        }
    }
    let towns = BoundaryLayer::shared(&ctx.snapshots, Layer::Town)?;
    let overlay: Vec<&Feature> = if args.geography != MapGeography::Town {
        let t: Vec<&Feature> = towns
            .features
            .iter()
            .filter(|f| intersects(f.bbox(), extent))
            .collect();
        if t.len() <= 60 { t } else { Vec::new() }
    } else {
        Vec::new()
    };
    let label_on = args.labels.unwrap_or(by_id.len() <= 25);
    let labels: Vec<(&Feature, String)> = if label_on {
        by_id
            .iter()
            .filter_map(|(id, (name, _, _))| {
                let f = layer.get(id)?;
                let text = match args.geography {
                    MapGeography::Tract => name.trim_start_matches("Census Tract ").to_string(),
                    _ => name.clone(),
                };
                Some((f, text))
            })
            .collect()
    } else {
        Vec::new()
    };

    let fmt = |v: f64| md::num(v, decimals);
    let mut legend: Vec<LegendRow> = classes
        .iter()
        .map(|c| LegendRow {
            label: if (c.upper - c.lower).abs() < 1e-12 {
                format!("{} ({})", fmt(c.lower), c.areas)
            } else {
                format!("{} – {} ({})", fmt(c.lower), fmt(c.upper), c.areas)
            },
            color: Some(c.color.clone()),
            hatched: false,
            outline_only: false,
        })
        .collect();
    legend.reverse(); // darkest (highest) first
    if suppressed_n + no_data_n > 0 {
        legend.push(LegendRow {
            label: format!(
                "Suppressed (count below {threshold}) or no data ({})",
                suppressed_n + no_data_n
            ),
            color: None,
            hatched: true,
            outline_only: false,
        });
    }
    if context_n > 0 {
        legend.push(LegendRow {
            label: "Not in the data".into(),
            color: None,
            hatched: false,
            outline_only: true,
        });
    }
    let value_label = args
        .value_label
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Value".into());
    let today = chrono::Local::now().date_naive();
    let note = format!(
        "Source: {}. Boundaries: U.S. Census Bureau cartographic boundary files (simplified){}. \
         Classes: {}, {} class{}. Hatched areas are suppressed (count below {threshold}) or have \
         no data. Created {today} with the Public Health Nursing MCP Server.",
        args.source_note
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("data supplied by the user"),
        if args.geography == MapGeography::SchoolDistrict {
            "; districts are unions of member towns"
        } else {
            ""
        },
        method.label(),
        k_used,
        if k_used == 1 { "" } else { "es" }
    );

    // Alt text.
    let names = |m: &[(String, f64)]| {
        let mut n: Vec<&str> = m.iter().map(|x| x.0.as_str()).collect();
        n.sort_unstable();
        let shown: Vec<&str> = n.iter().take(6).copied().collect();
        if n.len() > 6 {
            format!("{}, and {} more", shown.join(", "), n.len() - 6)
        } else {
            shown.join(", ")
        }
    };
    let min = mapped_values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = mapped_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let top = classes.last().expect("one class");
    let bottom = classes.first().expect("one class");
    let mut alt = format!(
        "Map of {title} for {} Massachusetts {}. {value_label} ranges from {} to {}, shown in {} \
         {} classes from light blue (lowest) to dark blue (highest). Highest class ({} to {}): {}.",
        by_id.len(),
        args.geography.plural(),
        fmt(min),
        fmt(max),
        k_used,
        method.label(),
        fmt(top.lower),
        fmt(top.upper),
        names(&class_members[k_used - 1])
    );
    if k_used > 1 {
        alt.push_str(&format!(
            " Lowest class ({} to {}): {}.",
            fmt(bottom.lower),
            fmt(bottom.upper),
            names(&class_members[0])
        ));
    }
    if suppressed_n + no_data_n > 0 {
        alt.push_str(&format!(
            " {} area(s) are hatched because the count is suppressed or there is no data.",
            suppressed_n + no_data_n
        ));
    }

    let spec = MapSpec {
        title: title.clone(),
        subtitle: args.subtitle.clone().filter(|s| !s.trim().is_empty()),
        legend_title: value_label.clone(),
        legend,
        note,
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
    let stem = args
        .file_name
        .as_deref()
        .map(|f| f.trim_end_matches(".svg").trim_end_matches(".png").to_string())
        .unwrap_or_else(|| slug(&title));
    let svg_path = unique_path(&dir, &stem, "svg");
    let final_stem = svg_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(stem);
    let png_path = dir.join(format!("{final_stem}.png"));
    std::fs::write(&svg_path, svg.as_bytes())?;
    std::fs::write(&png_path, &png)?;
    let files = vec![sandbox.display_path(&svg_path), sandbox.display_path(&png_path)];

    // Summary.
    let mut summary = format!(
        "**Map: {title}**\n\nWritten to /data/{} and /data/{}.\n\n",
        files[0], files[1]
    );
    let class_rows: Vec<Vec<String>> = classes
        .iter()
        .rev()
        .map(|c| {
            vec![
                c.class.to_string(),
                if (c.upper - c.lower).abs() < 1e-12 {
                    fmt(c.lower)
                } else {
                    format!("{} – {}", fmt(c.lower), fmt(c.upper))
                },
                c.areas.to_string(),
                c.color.clone(),
            ]
        })
        .collect();
    summary.push_str(&md::table(
        &["Class", &value_label, "Areas", "Color"],
        &class_rows,
    ));
    summary.push_str(&format!(
        "\n{} areas mapped ({}; goodness of variance fit {:.2}); {} suppressed; {} without data.\n\n**Alt text:** {alt}\n",
        by_id.len() - suppressed_n - no_data_n,
        method.label(),
        gvf,
        suppressed_n,
        no_data_n
    ));

    let mut caveats = Vec::new();
    if suppressed_n > 0 {
        caveats.push(format!(
            "{suppressed_n} area(s) had counts from 1 to {} and are hatched, not colored, so small \
             numbers cannot be read from the map.",
            threshold - 1
        ));
    }
    if !value_is_count && args.count_column.is_none() && rows.iter().all(|r| r.count.is_none()) {
        caveats.push(
            "No counts were given, so the server could not check for rates based on small \
             numbers. Suppress or combine areas with fewer than 20 events (unstable rates) and \
             counts below your threshold before sharing."
                .into(),
        );
    }
    if ignored_totals > 0 {
        caveats.push(format!("{ignored_totals} total row(s) were not mapped."));
    }
    if !missing_shapes.is_empty() {
        caveats.push(format!(
            "No boundary is bundled for: {} (water-only or new areas); they are not drawn.",
            missing_shapes.join(", ")
        ));
    }
    if gvf < 0.8 && mapped_values.len() > k_used {
        caveats.push(format!(
            "The classes fit the data loosely (goodness of variance fit {gvf:.2}); try natural \
             breaks or more classes."
        ));
    }
    if args.geography == MapGeography::Zcta {
        caveats.push(
            "ZCTAs approximate ZIP Codes and cross town lines; match the geography your counts \
             were tabulated by."
                .into(),
        );
    }
    if args.geography == MapGeography::SchoolDistrict {
        caveats.push(
            "Regional secondary and vocational districts overlap the elementary or municipal \
             districts of their member towns; map one kind of district at a time."
                .into(),
        );
    }
    caveats.push(
        "Choropleth maps of rates for areas with very different populations can mislead: small \
         areas show extreme rates by chance. Show the counts or confidence intervals alongside."
            .into(),
    );

    let mut sources = vec![
        layer.source.clone(),
        Source::computed(
            format!(
                "Choropleth classification: {} (Fisher WD. On grouping for maximum homogeneity. J Am Stat Assoc 1958;53:789-798; Jenks GF 1967)",
                method.label()
            ),
            "PHN MCP server calculation",
        ),
    ];
    let mut privacy = Privacy {
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        files_written: files.clone(),
        ..Privacy::none()
    };
    if let Some(l) = &local_label {
        sources.push(Source::local_file(l.clone()));
        privacy.notes.push(
            "Values from your file are drawn on the map but not returned; the response lists \
             only the class ranges and counts of areas."
                .into(),
        );
    }
    Ok(Envelope::new(
        summary,
        ChoroplethResult {
            geography: args.geography,
            title,
            method: method.label().into(),
            classes,
            goodness_of_variance_fit: (gvf * 1000.0).round() / 1000.0,
            mapped_areas: by_id.len() - suppressed_n - no_data_n,
            suppressed_areas: suppressed_n,
            no_data_areas: no_data_n,
            files,
            alt_text: alt,
            areas: local_label.is_none().then_some(areas),
            suppression_threshold: threshold,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    fn args(geography: MapGeography, values: Vec<(&str, Option<f64>, Option<u64>)>) -> CreateChoroplethArgs {
        CreateChoroplethArgs {
            geography,
            title: "Test map".into(),
            values: Some(
                values
                    .into_iter()
                    .map(|(a, v, c)| AreaValue {
                        area: a.into(),
                        value: v,
                        count: c,
                    })
                    .collect(),
            ),
            path: None,
            sheet: None,
            area_column: None,
            value_column: None,
            count_column: None,
            value_is_count: None,
            subtitle: Some("2024".into()),
            value_label: Some("Rate per 100,000".into()),
            method: None,
            classes: Some(3),
            extent: None,
            labels: None,
            decimals: None,
            source_note: None,
            file_name: None,
        }
    }

    #[test]
    fn maps_towns_with_suppression_and_writes_svg_and_png() {
        let (dir, ctx) = ctx();
        let env = create_choropleth_map(
            &ctx,
            args(
                MapGeography::Town,
                vec![
                    ("Amherst", Some(30.6), Some(12)),
                    ("Hadley", Some(56.3), Some(3)),
                    ("Northampton", Some(43.1), Some(25)),
                    ("Pelham", None, None),
                    ("Hatfield", Some(10.0), Some(20)),
                    ("Belchertown", Some(20.0), Some(30)),
                ],
            ),
        )
        .unwrap();
        let d = &env.data;
        assert_eq!(d.suppressed_areas, 1);
        assert_eq!(d.no_data_areas, 1);
        assert_eq!(d.mapped_areas, 4);
        assert_eq!(d.files.len(), 2);
        let svg = std::fs::read_to_string(dir.path().join("data").join(&d.files[0])).unwrap();
        assert!(svg.contains("<title id=\"t\">Test map</title>"));
        assert!(svg.contains("url(#hatch)"));
        assert!(svg.contains("Amherst"));
        // Hadley's suppressed value is not in the output.
        assert!(!env.summary.contains("56.3"));
        assert!(!svg.contains("– 56.3") && !svg.contains(">56.3"));
        let png = std::fs::read(dir.path().join("data").join(&d.files[1])).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(env.privacy.files_written.len() == 2);
    }

    #[test]
    fn unknown_areas_and_duplicates_are_errors() {
        let (_dir, ctx) = ctx();
        let err = create_choropleth_map(
            &ctx,
            args(MapGeography::Town, vec![("Worchester", Some(1.0), None)]),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("Worcester"), "{err}");
        let err = create_choropleth_map(
            &ctx,
            args(
                MapGeography::Town,
                vec![("Amherst", Some(1.0), None), ("amherst", Some(2.0), None)],
            ),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("more than once"), "{err}");
    }

    #[test]
    fn maps_tracts_and_zctas() {
        let (_dir, ctx) = ctx();
        let geos = MaGeographies::shared(&ctx.snapshots).unwrap();
        let tracts: Vec<(String, f64)> = geos
            .tracts_in_town("Springfield")
            .iter()
            .enumerate()
            .map(|(i, g)| (g.geoid.clone(), i as f64))
            .collect();
        let vals = tracts
            .iter()
            .map(|(g, v)| (g.as_str(), Some(*v), None))
            .collect();
        let env = create_choropleth_map(&ctx, args(MapGeography::Tract, vals)).unwrap();
        assert_eq!(env.data.classes.len(), 3);
        let env = create_choropleth_map(
            &ctx,
            args(
                MapGeography::Zcta,
                vec![("01002", Some(1.0), None), ("1003", Some(2.0), None), ("01007-1234", Some(3.0), None)],
            ),
        )
        .unwrap();
        assert_eq!(env.data.mapped_areas, 3);
    }
}
