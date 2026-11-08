//! CHR-01 `get_places_estimates`.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::census::GeoLevel;
use phn_data::geography::MaGeographies;
use phn_data::ma::MunicipalityType;
use phn_data::places::{self, PlacesLevel, PlacesLocation, PlacesSnapshot, PlacesValue};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Geographic level for PLACES.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlacesArea {
    County,
    /// A city or town (estimated from its census tracts; cities also get the
    /// PLACES place estimate).
    Town,
    /// A PLACES census place by name (cities and census-designated places).
    Place,
    Tract,
    Zcta,
}

/// Arguments for `get_places_estimates`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PlacesArgs {
    /// Area: a town or city name, "X County", an 11-digit tract GEOID, a 5-digit ZIP/ZCTA, or a census place name.
    pub area: String,
    /// Level of `area` (default: detected).
    #[serde(default)]
    pub level: Option<PlacesArea>,
    /// Measure ids or short names (e.g. "DIABETES", "Obesity"); default all 40.
    #[serde(default)]
    pub measures: Option<Vec<String>>,
    /// Limit to a category: "Health Outcomes", "Prevention", "Health Risk Behaviors",
    /// "Disability", "Health Status", or "Health-Related Social Needs".
    #[serde(default)]
    pub category: Option<String>,
    /// Report age-adjusted prevalence where published (counties and places). Default false (crude).
    #[serde(default)]
    pub age_adjusted: Option<bool>,
}

/// One measure's estimate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlacesRow {
    pub measure_id: String,
    pub measure: String,
    pub category: String,
    /// BRFSS survey year behind the estimate.
    pub data_year: String,
    /// Prevalence (%).
    pub value: f64,
    /// 95% confidence interval (none for town estimates built from tracts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<f64>,
    /// Massachusetts reference value (derived; see caveats).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub massachusetts: Option<f64>,
    /// U.S. reference value (derived).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub united_states: Option<f64>,
    /// "higher", "lower", or "similar" compared with Massachusetts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vs_massachusetts: Option<String>,
}

/// Result of `get_places_estimates`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlacesResult {
    pub area: String,
    pub level: PlacesArea,
    /// PLACES location id(s) used.
    pub location_ids: Vec<String>,
    pub release: String,
    /// "crude" or "age_adjusted".
    pub estimate_type: String,
    pub method: String,
    pub adult_population: Option<f64>,
    pub rows: Vec<PlacesRow>,
}

/// Estimates for a town built from its tracts: the mean of tract crude
/// prevalences weighted by adult population × the tract's share in the town.
pub(crate) fn town_from_tracts(
    snap: &PlacesSnapshot,
    geos: &MaGeographies,
    town: &str,
) -> Option<(PlacesLocation, usize)> {
    let tracts = geos.tracts_overlapping_town(town);
    let mut sums: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut pop = 0.0;
    let mut used = 0;
    for (t, share) in tracts {
        let Some(loc) = snap.location(PlacesLevel::Tract, &t.geoid) else {
            continue;
        };
        let w = loc.population_18plus.unwrap_or(0.0) * share;
        if w <= 0.0 {
            continue;
        }
        used += 1;
        pop += w;
        for (m, v) in &loc.crude {
            let e = sums.entry(m.clone()).or_default();
            e.0 += v.value * w;
            e.1 += w;
        }
    }
    if used == 0 {
        return None;
    }
    Some((
        PlacesLocation {
            level: PlacesLevel::Tract,
            location_id: town.to_string(),
            name: town.to_string(),
            population_18plus: Some(pop.round()),
            crude: sums
                .into_iter()
                .map(|(m, (s, w))| {
                    (
                        m,
                        PlacesValue {
                            value: (s / w * 10.0).round() / 10.0,
                            low: None,
                            high: None,
                        },
                    )
                })
                .collect(),
            age_adjusted: BTreeMap::new(),
        },
        used,
    ))
}

/// The PLACES census place that covers the whole town (cities and towns
/// with a city form of government), if any.
pub(crate) fn place_for_town<'a>(
    ctx: &Context,
    snap: &'a PlacesSnapshot,
    town: &str,
) -> Option<&'a PlacesLocation> {
    let m = ctx.ma.town(town)?;
    if m.kind == MunicipalityType::Town {
        return None;
    }
    let want = [m.name.clone(), format!("{} Town", m.name)];
    snap.locations
        .values()
        .find(|l| l.level == PlacesLevel::Place && want.iter().any(|w| l.name.eq_ignore_ascii_case(w)))
}

fn compare(v: &PlacesValue, reference: Option<f64>) -> Option<String> {
    let r = reference?;
    match (v.low, v.high) {
        (Some(l), _) if l > r => Some("higher".into()),
        (_, Some(h)) if h < r => Some("lower".into()),
        (Some(_), Some(_)) => Some("similar".into()),
        _ => None,
    }
}

/// Selected measures (all when none requested).
pub(crate) fn select_measures<'a>(
    snap: &'a PlacesSnapshot,
    wanted: Option<&[String]>,
    category: Option<&str>,
) -> Result<Vec<&'a places::Measure>> {
    let mut out = Vec::new();
    match wanted {
        Some(list) if !list.is_empty() => {
            let mut unknown = Vec::new();
            for w in list {
                match snap.measure(w) {
                    Some(m) => out.push(m),
                    None => unknown.push(w.clone()),
                }
            }
            if !unknown.is_empty() {
                return Err(PhnError::InvalidInput(format!(
                    "unknown PLACES measure(s): {}. Known ids: {}",
                    unknown.join(", "),
                    snap.measures.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", ")
                )));
            }
        }
        _ => out.extend(snap.measures.iter()),
    }
    if let Some(c) = category {
        let c = c.trim().to_lowercase();
        out.retain(|m| m.category.to_lowercase().contains(&c));
        if out.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "no measures in category '{c}'. Categories: Health Outcomes, Prevention, Health \
                 Risk Behaviors, Disability, Health Status, Health-Related Social Needs"
            )));
        }
    }
    Ok(out)
}

/// Build rows for a location.
pub(crate) fn rows_for(
    loc: &PlacesLocation,
    measures: &[&places::Measure],
    age_adjusted: bool,
) -> Vec<PlacesRow> {
    let map = if age_adjusted && !loc.age_adjusted.is_empty() {
        &loc.age_adjusted
    } else {
        &loc.crude
    };
    let use_age = age_adjusted && !loc.age_adjusted.is_empty();
    measures
        .iter()
        .filter_map(|m| {
            let v = map.get(&m.id)?;
            let (ma, us) = if use_age {
                (m.ma_age_adjusted, m.us_age_adjusted)
            } else {
                (m.ma_crude, m.us_crude)
            };
            Some(PlacesRow {
                measure_id: m.id.clone(),
                measure: m.name.clone(),
                category: m.category.clone(),
                data_year: m.data_year.clone(),
                value: v.value,
                low: v.low,
                high: v.high,
                massachusetts: ma,
                united_states: us,
                vs_massachusetts: compare(v, ma),
            })
        })
        .collect()
}

fn resolve(
    ctx: &Context,
    snap: &PlacesSnapshot,
    geos: &MaGeographies,
    area: &str,
    level: Option<PlacesArea>,
) -> Result<(PlacesArea, String, String)> {
    let a = area.trim();
    if let Some(PlacesArea::Place) = level {
        let p = snap
            .locations
            .values()
            .find(|l| l.level == PlacesLevel::Place && l.name.eq_ignore_ascii_case(a))
            .ok_or_else(|| {
                PhnError::NotFound(format!(
                    "'{a}' is not a Massachusetts census place in PLACES; try level 'town'"
                ))
            })?;
        return Ok((PlacesArea::Place, p.location_id.clone(), p.name.clone()));
    }
    let geo_level = level.map(|l| match l {
        PlacesArea::County => GeoLevel::County,
        PlacesArea::Town => GeoLevel::Town,
        PlacesArea::Tract => GeoLevel::Tract,
        PlacesArea::Zcta | PlacesArea::Place => GeoLevel::Zcta,
    });
    match geos.resolve(a, geo_level) {
        Ok(g) => {
            let lvl = match g.level {
                GeoLevel::County => PlacesArea::County,
                GeoLevel::Town => PlacesArea::Town,
                GeoLevel::Tract => PlacesArea::Tract,
                _ => PlacesArea::Zcta,
            };
            let id = if lvl == PlacesArea::Town { g.name.clone() } else { g.geoid.clone() };
            let name = match lvl {
                PlacesArea::Tract => g.name.clone(),
                PlacesArea::Zcta => format!("ZCTA {}", g.geoid),
                _ => g.name.clone(),
            };
            Ok((lvl, id, name))
        }
        Err(e) => {
            // A census-designated place name ("Amherst Center")?
            if level.is_none()
                && let Some(p) = snap
                    .locations
                    .values()
                    .find(|l| l.level == PlacesLevel::Place && l.name.eq_ignore_ascii_case(a))
            {
                return Ok((PlacesArea::Place, p.location_id.clone(), p.name.clone()));
            }
            let _ = ctx;
            Err(e)
        }
    }
}

/// PLACES estimates for one area, with Massachusetts and U.S. comparisons.
pub async fn get_places_estimates(
    ctx: &Context,
    args: PlacesArgs,
) -> Result<Envelope<PlacesResult>> {
    let snap = PlacesSnapshot::shared(&ctx.snapshots)?;
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let (level, id, name) = resolve(ctx, snap, geos, &args.area, args.level)?;
    let measures = select_measures(snap, args.measures.as_deref(), args.category.as_deref())?;
    let age_adjusted = args.age_adjusted.unwrap_or(false);
    let mut sources = Vec::new();
    let mut caveats = Vec::new();
    let mut location_ids = vec![id.clone()];
    let (loc, method) = match level {
        PlacesArea::Town => {
            let place = match place_for_town(ctx, snap, &id) {
                Some(p) => places::fetch_location(
                    &ctx.http,
                    ctx.cache.as_ref(),
                    Some(snap),
                    PlacesLevel::Place,
                    &p.location_id,
                )
                .await
                .ok()
                .map(|f| (p, f)),
                None => None,
            };
            match place {
                Some((p, f)) => {
                    sources.push(f.source());
                    location_ids = vec![p.location_id.clone()];
                    caveats.push(format!(
                        "{name} is a city whose census place ({}) has the same boundary as the \
                         town, so CDC's published place estimates are shown.",
                        p.name
                    ));
                    (
                        f.location,
                        format!("PLACES place estimate for {} (published by CDC)", p.name),
                    )
                }
                None => {
                    let (loc, n) = town_from_tracts(snap, geos, &id).ok_or_else(|| {
                        PhnError::NotFound(format!("no PLACES tract estimates for {name}"))
                    })?;
                    let mut s = snap.entry.to_source();
                    s.title = format!(
                        "{} (town estimate derived from census tracts)",
                        PlacesLevel::Tract.dataset_title()
                    );
                    s.url = Some(format!(
                        "https://data.cdc.gov/d/{}",
                        PlacesLevel::Tract.dataset_id()
                    ));
                    sources.push(s);
                    caveats.push(format!(
                        "Town values are {}. They are derived by this server, are not \
                         published by CDC, and have no confidence interval.",
                        if n == 1 {
                            "the estimates for the one census tract that overlaps the town (it may also \
                             cover neighboring towns)"
                                .to_string()
                        } else {
                            format!(
                                "the average of {n} census-tract estimates weighted by adult \
                                 population (and by each tract's share in the town)"
                            )
                        }
                    ));
                    (
                        loc,
                        if n == 1 {
                            "PLACES estimates for the one census tract that overlaps the town (crude prevalence)"
                                .to_string()
                        } else {
                            format!(
                                "Population-weighted mean of {n} PLACES census-tract estimates (crude prevalence)"
                            )
                        },
                    )
                }
            }
        }
        _ => {
            let pl = match level {
                PlacesArea::County => PlacesLevel::County,
                PlacesArea::Place => PlacesLevel::Place,
                PlacesArea::Tract => PlacesLevel::Tract,
                _ => PlacesLevel::Zcta,
            };
            let f = places::fetch_location(&ctx.http, ctx.cache.as_ref(), Some(snap), pl, &id).await?;
            if let Some(r) = &f.fallback_reason
                && f.mode == SourceMode::Snapshot
            {
                caveats.push(format!("Live PLACES data unavailable ({r}); the bundled {} release snapshot was used.", places::RELEASE));
            }
            sources.push(f.source());
            (f.location, format!("{} (published by CDC)", pl.dataset_title()))
        }
    };
    let rows = rows_for(&loc, &measures, age_adjusted);
    if rows.is_empty() {
        return Err(PhnError::NotFound(format!(
            "PLACES has none of the requested measures for {name}"
        )));
    }
    let used_age = age_adjusted && !loc.age_adjusted.is_empty();
    if age_adjusted && !used_age {
        caveats.push(
            "Age-adjusted estimates are published only for counties and places; crude \
             prevalence is shown."
                .into(),
        );
    }

    // Summary.
    let mut summary = format!(
        "**CDC PLACES {} release — {name}** ({} prevalence among adults{})\n\n",
        places::RELEASE,
        if used_age { "age-adjusted" } else { "crude" },
        loc.population_18plus
            .map(|p| format!("; adult population {}", md::num(p, 0)))
            .unwrap_or_default()
    );
    let mut by_cat: BTreeMap<&str, Vec<&PlacesRow>> = BTreeMap::new();
    for r in &rows {
        by_cat.entry(r.category.as_str()).or_default().push(r);
    }
    for (cat, rs) in by_cat {
        summary.push_str(&format!("**{cat}**\n\n"));
        let t: Vec<Vec<String>> = rs
            .iter()
            .map(|r| {
                vec![
                    r.measure.clone(),
                    match (r.low, r.high) {
                        (Some(l), Some(h)) => format!("{:.1}% ({l:.1}–{h:.1})", r.value),
                        _ => format!("{:.1}%", r.value),
                    },
                    r.massachusetts.map(|v| format!("{v:.1}%")).unwrap_or("—".into()),
                    r.united_states.map(|v| format!("{v:.1}%")).unwrap_or("—".into()),
                    r.data_year.clone(),
                    r.vs_massachusetts.clone().unwrap_or_default(),
                ]
            })
            .collect();
        summary.push_str(&md::table(
            &["Measure", "Estimate (95% CI)", "MA", "US", "Data year", "vs MA"],
            &t,
        ));
        summary.push('\n');
    }
    summary.push_str(&format!("Method: {method}.\n"));
    caveats.push(
        "PLACES values are model-based small-area estimates from BRFSS and Census data, not \
         local survey results; CDC advises against using them to evaluate local programs or \
         policies."
            .into(),
    );
    caveats.push(format!(
        "Release {} uses BRFSS {} data for most measures; the data year is shown for each \
         measure.",
        places::RELEASE,
        rows.iter().map(|r| r.data_year.as_str()).max().unwrap_or("")
    ));
    caveats.push(
        "Massachusetts and U.S. columns are adult-population-weighted means of county estimates \
         computed by this server, not official CDC state estimates; 'higher' or 'lower' means \
         the area's 95% CI excludes the Massachusetts value."
            .into(),
    );
    sources.push(Source {
        title: "PLACES measure definitions".into(),
        publisher: "Centers for Disease Control and Prevention".into(),
        url: Some(places::DEFINITIONS_URL.into()),
        effective_date: Some(places::RELEASE.into()),
        retrieved: Some(snap.entry.retrieved.clone()),
        mode: SourceMode::Snapshot,
    });
    Ok(Envelope::new(
        summary,
        PlacesResult {
            area: name,
            level,
            location_ids,
            release: places::RELEASE.into(),
            estimate_type: if used_age { "age_adjusted" } else { "crude" }.into(),
            method,
            adult_population: loc.population_18plus,
            rows,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[tokio::test]
    async fn town_county_tract_and_zcta_offline() {
        let (_d, ctx) = ctx();
        for (area, level) in [
            ("Amherst", PlacesArea::Town),
            ("Pelham", PlacesArea::Town),
            ("Hampden County", PlacesArea::County),
            ("01002", PlacesArea::Zcta),
        ] {
            let env = get_places_estimates(
                &ctx,
                PlacesArgs {
                    area: area.into(),
                    level: None,
                    measures: None,
                    category: None,
                    age_adjusted: None,
                },
            )
            .await
            .unwrap_or_else(|e| panic!("{area}: {e}"));
            assert_eq!(env.data.level, level, "{area}");
            assert!(env.data.rows.len() >= 30, "{area}");
        }
        let geos = MaGeographies::shared(&ctx.snapshots).unwrap();
        let tract = geos.tracts_in_town("Holyoke")[0].geoid.clone();
        let env = get_places_estimates(
            &ctx,
            PlacesArgs {
                area: tract,
                level: None,
                measures: Some(vec!["diabetes".into(), "CASTHMA".into()]),
                category: None,
                age_adjusted: Some(true),
            },
        )
        .await
        .unwrap();
        assert_eq!(env.data.rows.len(), 2);
        assert!(env.data.rows[0].low.is_some());
        assert_eq!(env.data.estimate_type, "crude");
    }

    #[test]
    fn boston_uses_the_city_place() {
        let (_d, ctx) = ctx();
        let snap = PlacesSnapshot::shared(&ctx.snapshots).unwrap();
        assert!(place_for_town(&ctx, snap, "Boston").is_some());
        assert!(place_for_town(&ctx, snap, "Barnstable").is_some());
        assert!(place_for_town(&ctx, snap, "Pelham").is_none());
    }
}
