//! A vulnerable-population profile for preparedness planning: ACS
//! demographics, CDC PLACES adult prevalence, CDC/ATSDR SVI, and HHS emPOWER
//! electricity-dependent Medicare beneficiaries for a town, county, or the
//! state. All figures are public aggregates.

use phn_core::{Result, Source, SourceMode};
use phn_data::census::{self, AcsSnapshot, GeoLevel};
use phn_data::empower::{self, EmpowerLevel, EmpowerRow};
use phn_data::geography::MaGeographies;
use phn_data::places::{PlacesLevel, PlacesSnapshot};
use phn_data::svi::SviSnapshot;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::area::{Area, AreaLevel};
use crate::context::Context;
use crate::md;

/// One indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Indicator {
    pub id: String,
    pub label: String,
    /// Percent of the population (or of adults for PLACES measures).
    pub percent: Option<f64>,
    /// 90% margin of error of the percent (ACS) when published.
    pub moe: Option<f64>,
    /// Approximate number of people: percent × population (rounded).
    pub people: Option<f64>,
    /// Massachusetts value for comparison.
    pub massachusetts: Option<f64>,
    /// "ACS 2020–2024", "PLACES 2025 (2023 BRFSS)", ...
    pub source: String,
    /// True when the ACS coefficient of variation is above 30%.
    pub unreliable: bool,
}

/// Socially vulnerable tracts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SviSummary {
    pub tracts: usize,
    /// Tracts at or above the 75th percentile of Massachusetts tracts (overall SVI).
    pub high_vulnerability_tracts: usize,
    pub population_in_high_tracts: f64,
    pub high_tract_ids: Vec<String>,
}

/// Electricity-dependent Medicare beneficiaries (HHS emPOWER).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EmpowerSummary {
    pub electricity_dependent: f64,
    pub medicare_beneficiaries: f64,
    /// Per 1,000 Medicare beneficiaries.
    pub per_1000_beneficiaries: Option<f64>,
    /// "county", "state", or "ZIP codes allocated to the town by land area".
    pub method: String,
    /// "live", "cache", or "snapshot".
    pub mode: String,
}

/// The profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PopulationProfile {
    pub area: String,
    pub population: Option<f64>,
    pub households: Option<f64>,
    pub demographics: Vec<Indicator>,
    pub health: Vec<Indicator>,
    pub svi: Option<SviSummary>,
    pub electricity_dependent: Option<EmpowerSummary>,
}

const ACS_IDS: &[&str] = &[
    "age_65plus_pct",
    "age_85plus_pct",
    "age_under5_pct",
    "disability_pct",
    "disability_65plus_pct",
    "poverty_pct",
    "no_vehicle_pct",
    "lep_pct",
    "no_internet_pct",
    "renter_pct",
];

const PLACES_IDS: &[&str] = &[
    "CASTHMA",
    "COPD",
    "CHD",
    "DIABETES",
    "OBESITY",
    "MOBILITY",
    "COGNITION",
    "INDEPLIVE",
    "SELFCARE",
    "LONELINESS",
    "SHUTUTILITY",
];

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Build the profile. Returns it with the sources and caveats it used.
pub async fn profile(
    ctx: &Context,
    area: &Area,
    with_empower: bool,
) -> Result<(PopulationProfile, Vec<Source>, Vec<String>)> {
    let mut sources = Vec::new();
    let mut caveats = Vec::new();
    let geos = MaGeographies::shared(&ctx.snapshots)?;

    // ACS (bundled snapshot).
    let acs = AcsSnapshot::shared(&ctx.snapshots)?;
    let row = acs.row(&area.geoid);
    let ma = acs.state();
    let population = row.and_then(|r| r.values.get("pop_total")).map(|e| e.value);
    let households = row
        .and_then(|r| r.values.get("households"))
        .map(|e| e.value);
    let period = format!("ACS {}–{}", acs.vintage - 4, acs.vintage);
    let mut demographics = Vec::new();
    if let Some(r) = row {
        for id in ACS_IDS {
            let Some(e) = r.values.get(*id) else { continue };
            let label = census::indicator(id)
                .map(|d| d.label.to_string())
                .unwrap_or_else(|| id.to_string());
            demographics.push(Indicator {
                id: id.to_string(),
                label,
                percent: Some(round1(e.value)),
                moe: e.moe.map(round1),
                people: population.map(|p| (p * e.value / 100.0).round()),
                massachusetts: ma.and_then(|m| m.values.get(*id)).map(|m| round1(m.value)),
                source: period.clone(),
                unreliable: e.unreliable(),
            });
        }
        sources.push(acs.source());
        if demographics.iter().any(|d| d.unreliable) {
            caveats.push(
                "Some ACS estimates for this area are unreliable (coefficient of variation above \
                 30%); use their margins of error."
                    .into(),
            );
        }
    } else {
        caveats.push(format!(
            "No ACS estimates are bundled for {} ({}).",
            area.name, area.geoid
        ));
    }

    // Tracts in the area.
    let tracts: Vec<&phn_data::geography::Geography> = geos
        .at_level(GeoLevel::Tract)
        .filter(|t| match area.level {
            AreaLevel::State => true,
            AreaLevel::County => Some(t.county_geoid.as_str()) == area.county_fips.as_deref(),
            AreaLevel::Town => t.town.as_deref() == area.town.as_deref(),
        })
        .collect();

    // PLACES.
    let mut health = Vec::new();
    if let Ok(places) = PlacesSnapshot::shared(&ctx.snapshots) {
        for id in PLACES_IDS {
            let Some(m) = places.measure(id) else {
                continue;
            };
            let value = match area.level {
                AreaLevel::State => m.ma_crude,
                AreaLevel::County => places
                    .location(PlacesLevel::County, &area.geoid)
                    .and_then(|l| l.crude.get(*id))
                    .map(|v| v.value),
                AreaLevel::Town => {
                    let (mut num, mut den) = (0.0, 0.0);
                    for t in &tracts {
                        if let Some(l) = places.location(PlacesLevel::Tract, &t.geoid)
                            && let (Some(v), Some(p)) = (l.crude.get(*id), l.population_18plus)
                        {
                            num += v.value * p;
                            den += p;
                        }
                    }
                    (den > 0.0).then(|| num / den)
                }
            };
            let adults = population.and_then(|p| {
                row.and_then(|r| r.values.get("age_under18_pct"))
                    .map(|u| p * (1.0 - u.value / 100.0))
            });
            health.push(Indicator {
                id: id.to_string(),
                label: format!("{} (adults)", m.short_name),
                percent: value.map(round1),
                moe: None,
                people: value.zip(adults).map(|(v, a)| (a * v / 100.0).round()),
                massachusetts: m.ma_crude.map(round1),
                source: format!("PLACES 2025 ({} BRFSS)", m.data_year),
                unreliable: false,
            });
        }
        let mut s = places.entry.to_source();
        s.title =
            "CDC PLACES: Local Data for Better Health, 2025 release (model-based estimates)".into();
        sources.push(s);
        if area.level == AreaLevel::Town {
            caveats.push(
                "Town PLACES values are adult-population-weighted averages of the town's census \
                 tract estimates (model-based, not local survey results)."
                    .into(),
            );
        }
    }

    // SVI.
    let svi = match SviSnapshot::shared(&ctx.snapshots) {
        Ok(snap) => {
            let mut s = SviSummary {
                tracts: 0,
                high_vulnerability_tracts: 0,
                population_in_high_tracts: 0.0,
                high_tract_ids: Vec::new(),
            };
            for t in &tracts {
                if let Some(x) = snap.tract(&t.geoid) {
                    s.tracts += 1;
                    if x.ma.overall.is_some_and(|o| o >= 0.75) {
                        s.high_vulnerability_tracts += 1;
                        s.population_in_high_tracts += x.population.unwrap_or(0.0);
                        s.high_tract_ids.push(t.geoid.clone());
                    }
                }
            }
            sources.push(snap.source.clone());
            (s.tracts > 0).then_some(s)
        }
        Err(_) => None,
    };

    // emPOWER.
    let mut electricity_dependent = None;
    if with_empower {
        match empower_for(ctx, area).await {
            Ok((e, src)) => {
                electricity_dependent = Some(e);
                sources.push(src);
            }
            Err(e) => caveats.push(format!(
                "HHS emPOWER counts could not be read ({}).",
                phn_data::apifetch::reason(&e)
            )),
        }
    }

    Ok((
        PopulationProfile {
            area: area.name.clone(),
            population,
            households,
            demographics,
            health,
            svi,
            electricity_dependent,
        },
        sources,
        caveats,
    ))
}

/// emPOWER counts for an area: the county or state row, or for a town the
/// sum over ZIP codes allocated by the ZCTA's land-area share in the town.
pub async fn empower_for(ctx: &Context, area: &Area) -> Result<(EmpowerSummary, Source)> {
    let (rows, mode, as_of, method): (Vec<EmpowerRow>, SourceMode, String, String) = match area
        .level
    {
        AreaLevel::Town => {
            let f = empower::fetch_zips(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await?;
            (
                f.rows,
                f.mode,
                f.as_of,
                "ZIP codes allocated to the town by the ZCTA's land-area share".into(),
            )
        }
        _ => {
            let f = empower::fetch_counties(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await?;
            (
                f.rows,
                f.mode,
                f.as_of,
                if area.level == AreaLevel::State {
                    "sum of the 14 counties".into()
                } else {
                    "county".into()
                },
            )
        }
    };
    let (dme, benes) = match area.level {
        AreaLevel::State => rows
            .iter()
            .filter(|r| r.level == EmpowerLevel::County)
            .fold((0.0, 0.0), |a, r| {
                (
                    a.0 + r.electricity_dependent as f64,
                    a.1 + r.medicare_beneficiaries as f64,
                )
            }),
        AreaLevel::County => rows
            .iter()
            .find(|r| Some(r.geoid.as_str()) == area.county_fips.as_deref())
            .map(|r| {
                (
                    r.electricity_dependent as f64,
                    r.medicare_beneficiaries as f64,
                )
            })
            .unwrap_or((0.0, 0.0)),
        AreaLevel::Town => {
            let geos = MaGeographies::shared(&ctx.snapshots)?;
            let town = area.town.clone().unwrap_or_default();
            rows.iter().fold((0.0, 0.0), |a, r| {
                let share = geos
                    .get(&r.geoid)
                    .and_then(|z| {
                        z.town_shares
                            .iter()
                            .find(|(t, _)| *t == town)
                            .map(|(_, s)| *s)
                    })
                    .unwrap_or(0.0);
                (
                    a.0 + r.electricity_dependent as f64 * share,
                    a.1 + r.medicare_beneficiaries as f64 * share,
                )
            })
        }
    };
    let src = Source {
        title: "HHS emPOWER Map: Medicare beneficiaries with electricity-dependent DME".into(),
        publisher: "HHS Administration for Strategic Preparedness and Response (ASPR) and CMS"
            .into(),
        url: Some(empower::MAP_URL.into()),
        effective_date: None,
        retrieved: Some(as_of.chars().take(10).collect()),
        mode,
    };
    Ok((
        EmpowerSummary {
            electricity_dependent: dme.round(),
            medicare_beneficiaries: benes.round(),
            per_1000_beneficiaries: (benes > 0.0).then(|| round1(dme / benes * 1000.0)),
            method,
            mode: mode.label().to_string(),
        },
        src,
    ))
}

/// Markdown table of the profile.
pub fn markdown(p: &PopulationProfile) -> String {
    let mut s = String::new();
    if let Some(pop) = p.population {
        s.push_str(&format!(
            "Population {}{}.\n\n",
            md::num(pop, 0),
            p.households
                .map(|h| format!(", {} households", md::num(h, 0)))
                .unwrap_or_default()
        ));
    }
    let rows: Vec<Vec<String>> = p
        .demographics
        .iter()
        .chain(p.health.iter())
        .map(|i| {
            vec![
                i.label.clone(),
                i.percent
                    .map(|v| {
                        format!(
                            "{v}%{}{}",
                            i.moe.map(|m| format!(" ±{m}")).unwrap_or_default(),
                            if i.unreliable { " (unreliable)" } else { "" }
                        )
                    })
                    .unwrap_or_else(|| "—".into()),
                i.people
                    .map(|n| md::num(n, 0))
                    .unwrap_or_else(|| "—".into()),
                i.massachusetts
                    .map(|v| format!("{v}%"))
                    .unwrap_or_else(|| "—".into()),
                i.source.clone(),
            ]
        })
        .collect();
    if !rows.is_empty() {
        s.push_str(&md::table(
            &[
                "Indicator",
                "Percent",
                "About how many people",
                "Massachusetts",
                "Source",
            ],
            &rows,
        ));
    }
    if let Some(v) = &p.svi {
        s.push_str(&format!(
            "\nSocial Vulnerability Index: {} of {} census tracts are in the top quarter for \
             Massachusetts (about {} residents).\n",
            v.high_vulnerability_tracts,
            v.tracts,
            md::num(v.population_in_high_tracts, 0)
        ));
    }
    if let Some(e) = &p.electricity_dependent {
        s.push_str(&format!(
            "HHS emPOWER: about {} Medicare beneficiaries rely on electricity-dependent medical \
             equipment ({}; {} per 1,000 beneficiaries).\n",
            md::num(e.electricity_dependent, 0),
            e.method,
            e.per_1000_beneficiaries
                .map(|v| v.to_string())
                .unwrap_or_else(|| "—".into())
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epr::area;

    #[tokio::test]
    async fn profiles_a_town_offline() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let a = area::resolve(&ctx, Some("Lowell"), true).unwrap();
        let (p, sources, _) = profile(&ctx, &a, true).await.unwrap();
        assert!(p.population.unwrap() > 100_000.0);
        assert!(p.demographics.iter().any(|d| d.id == "age_65plus_pct"));
        assert!(
            p.health
                .iter()
                .any(|d| d.id == "COPD" && d.percent.is_some())
        );
        assert!(p.svi.as_ref().unwrap().tracts > 10);
        let e = p.electricity_dependent.unwrap();
        assert!(e.electricity_dependent > 100.0);
        assert_eq!(e.mode, "snapshot");
        assert!(sources.iter().any(|s| s.mode == SourceMode::Snapshot));
        let c = area::resolve(&ctx, Some("Hampshire County"), true).unwrap();
        let (e, _) = empower_for(&ctx, &c).await.unwrap();
        // Bundled snapshot: Hampshire County, 1,179 of 40,063 beneficiaries.
        assert_eq!(e.electricity_dependent, 1179.0);
        assert_eq!(e.per_1000_beneficiaries, Some(29.4));
        let (p, _, _) = profile(&ctx, &area::Area::state(), false).await.unwrap();
        assert!(p.population.unwrap() > 6_000_000.0);
    }
}
