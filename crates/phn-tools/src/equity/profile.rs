//! EQ-01 `build_community_health_profile`.

use phn_core::{Envelope, Privacy, Result, Source, SourceMode};
use phn_data::census::{self, GeoLevel};
use phn_data::geography::MaGeographies;
use phn_data::places::{self, PlacesLevel, PlacesSnapshot};
use phn_data::svi::{Ranks, SviSnapshot};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::acs;
use crate::chronic::places::{
    PlacesRow, place_for_town, rows_for, select_measures, town_from_tracts,
};
use crate::context::Context;
use crate::md;

/// Level of the profiled area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProfileLevel {
    County,
    Town,
    Tract,
}

/// Arguments for `build_community_health_profile`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ProfileArgs {
    /// A Massachusetts city or town ("Lawrence"), county ("Hampden County"),
    /// or census tract (11-digit GEOID). Default: your configured town.
    #[serde(default)]
    pub area: Option<String>,
    /// Level of `area` (default: detected).
    #[serde(default)]
    pub level: Option<ProfileLevel>,
    /// Include CDC PLACES chronic disease estimates (default true).
    #[serde(default)]
    pub include_chronic_disease: Option<bool>,
    /// Include the CDC/ATSDR Social Vulnerability Index (default true).
    #[serde(default)]
    pub include_svi: Option<bool>,
}

/// One profile figure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileIndicator {
    pub id: String,
    pub section: String,
    pub label: String,
    /// ACS detailed table ID.
    pub table: String,
    pub universe: String,
    pub value: Option<f64>,
    /// 90% margin of error.
    pub moe: Option<f64>,
    /// Coefficient of variation (%).
    pub cv: Option<f64>,
    /// CV above 30%: use with caution.
    pub unreliable: bool,
    pub massachusetts: Option<f64>,
    pub united_states: Option<f64>,
}

/// The area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileArea {
    pub name: String,
    pub level: ProfileLevel,
    pub geoid: String,
    pub census_name: String,
    pub county: String,
    pub land_area_sqmi: Option<f64>,
    /// Residents per square mile of land.
    pub population_density: Option<f64>,
    /// Towns a tract lies in (with shares), or tracts in a town.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
}

/// PLACES section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChronicDiseaseSection {
    pub release: String,
    pub method: String,
    pub rows: Vec<PlacesRow>,
}

/// SVI section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SviSection {
    pub release: String,
    /// Tracts summarized (the tract itself, or the tracts in the town or county).
    pub tracts: usize,
    /// Ranks of a single tract (tract profiles only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tract_ranks_massachusetts: Option<Ranks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tract_ranks_national: Option<Ranks>,
    /// Median overall percentile (within Massachusetts) of the tracts.
    pub median_overall_massachusetts: Option<f64>,
    /// Tracts at or above the 75th percentile within Massachusetts.
    pub tracts_high_vulnerability: usize,
    /// Most vulnerable tract (GEOID, overall percentile within Massachusetts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub most_vulnerable_tract: Option<(String, f64)>,
}

/// Result of `build_community_health_profile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileResult {
    pub area: ProfileArea,
    /// ACS period, e.g. "2020–2024".
    pub acs_period: String,
    pub acs_mode: SourceMode,
    pub indicators: Vec<ProfileIndicator>,
    /// Figures with CV above 30%.
    pub unreliable_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chronic_disease: Option<ChronicDiseaseSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svi: Option<SviSection>,
    /// Why births and deaths are not included.
    pub vital_statistics: String,
}

/// PLACES measures shown in the profile (the full set is in get_places_estimates).
pub const PROFILE_MEASURES: [&str; 15] = [
    "DIABETES",
    "OBESITY",
    "BPHIGH",
    "CHD",
    "CASTHMA",
    "COPD",
    "DEPRESSION",
    "MHLTH",
    "CSMOKING",
    "ACCESS2",
    "CHECKUP",
    "DISABILITY",
    "FOODINSECU",
    "HOUSINSECU",
    "LACKTRPT",
];

const VITAL_STATISTICS: &str = "Births and deaths are not included: no machine-readable source of \
Massachusetts town-level vital statistics from DPH or CDC was reachable when this tool was built. \
Use DPH's published vital statistics reports for these figures.";

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// One-call community profile.
pub async fn build_community_health_profile(
    ctx: &Context,
    args: ProfileArgs,
) -> Result<Envelope<ProfileResult>> {
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let query = match args.area.as_deref().filter(|a| !a.trim().is_empty()) {
        Some(a) => a.to_string(),
        None => ctx.settings().jurisdiction.town.ok_or_else(|| {
            phn_core::PhnError::InvalidInput(
                "give an area (town, county, or tract), or set your town with \
                 configure_jurisdiction"
                    .into(),
            )
        })?,
    };
    let level = args.level.map(|l| match l {
        ProfileLevel::County => GeoLevel::County,
        ProfileLevel::Town => GeoLevel::Town,
        ProfileLevel::Tract => GeoLevel::Tract,
    });
    let geo = geos.resolve(&query, level)?;
    acs::check_level(geo)?;
    let plevel = match geo.level {
        GeoLevel::County => ProfileLevel::County,
        GeoLevel::Tract => ProfileLevel::Tract,
        _ => ProfileLevel::Town,
    };
    let fig = acs::figures(ctx, geo).await?;
    let mut sources = fig.sources.clone();
    let mut caveats = fig.caveats.clone();

    // ACS indicators.
    let mut indicators = Vec::new();
    for def in census::INDICATORS
        .iter()
        .filter(|d| d.section != "Language counts")
    {
        let e = fig.values.get(def.id);
        indicators.push(ProfileIndicator {
            id: def.id.into(),
            section: def.section.into(),
            label: def.label.into(),
            table: def.table.into(),
            universe: def.universe.into(),
            value: e.map(|e| e.value),
            moe: e.and_then(|e| e.moe),
            cv: e.and_then(|e| e.cv()).map(|c| (c * 10.0).round() / 10.0),
            unreliable: e.is_some_and(|e| e.unreliable()),
            massachusetts: fig.ma.get(def.id).map(|e| e.value),
            united_states: fig.us.get(def.id).map(|e| e.value),
        });
    }
    let unreliable: Vec<&ProfileIndicator> = indicators.iter().filter(|i| i.unreliable).collect();
    let pop = fig.values.get("pop_total").map(|e| e.value);
    let county = geos
        .get(&geo.county_geoid)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let related = match plevel {
        ProfileLevel::Tract => geo
            .town_shares
            .iter()
            .map(|(t, s)| format!("{t} ({:.0}% of the tract's blocks)", s * 100.0))
            .collect(),
        ProfileLevel::Town => geos
            .tracts_in_town(&geo.name)
            .iter()
            .map(|t| t.geoid.clone())
            .collect(),
        ProfileLevel::County => Vec::new(),
    };
    let area = ProfileArea {
        name: geo.name.clone(),
        level: plevel,
        geoid: geo.geoid.clone(),
        census_name: geo.census_name.clone(),
        county: county.clone(),
        land_area_sqmi: geo.aland_sqmi,
        population_density: match (pop, geo.aland_sqmi) {
            (Some(p), Some(a)) if a > 0.0 => Some((p / a).round()),
            _ => None,
        },
        related,
    };
    sources.push(geos.source.clone());

    // Chronic disease (PLACES).
    let chronic = if args.include_chronic_disease.unwrap_or(true) {
        let snap = PlacesSnapshot::shared(&ctx.snapshots)?;
        let wanted: Vec<String> = PROFILE_MEASURES.iter().map(|s| s.to_string()).collect();
        let measures = select_measures(snap, Some(&wanted), None)?;
        let fetched = match plevel {
            ProfileLevel::County => Some((PlacesLevel::County, geo.geoid.clone())),
            ProfileLevel::Tract => Some((PlacesLevel::Tract, geo.geoid.clone())),
            ProfileLevel::Town => place_for_town(ctx, snap, &geo.name)
                .map(|p| (PlacesLevel::Place, p.location_id.clone())),
        };
        let mut section = None;
        if let Some((lvl, id)) = fetched
            && let Ok(f) =
                places::fetch_location(&ctx.http, ctx.cache.as_ref(), Some(snap), lvl, &id).await
        {
            sources.push(f.source());
            section = Some(ChronicDiseaseSection {
                release: places::RELEASE.into(),
                method: format!("{} (published by CDC)", lvl.dataset_title()),
                rows: rows_for(&f.location, &measures, false),
            });
        }
        if section.is_none()
            && plevel == ProfileLevel::Town
            && let Some((loc, n)) = town_from_tracts(snap, geos, &geo.name)
        {
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
                "Chronic disease figures for {} are {}, derived by this server (no confidence \
                 interval).",
                geo.name,
                if n == 1 {
                    "the PLACES estimates for the one census tract that overlaps the town (it may also \
                     cover neighboring towns)"
                        .to_string()
                } else {
                    format!("the adult-population-weighted average of {n} census-tract PLACES estimates")
                }
            ));
            section = Some(ChronicDiseaseSection {
                release: places::RELEASE.into(),
                method: if n == 1 {
                    "PLACES estimates for the one census tract that overlaps the town".to_string()
                } else {
                    format!("Population-weighted mean of {n} PLACES census-tract estimates")
                },
                rows: rows_for(&loc, &measures, false),
            });
        }
        if section.is_some() {
            caveats.push(
                "PLACES figures are model-based estimates for adults (crude prevalence); the \
                 Massachusetts and U.S. columns are population-weighted means of county \
                 estimates computed by this server."
                    .into(),
            );
        }
        section
    } else {
        None
    };

    // SVI.
    let svi = if args.include_svi.unwrap_or(true) {
        let s = SviSnapshot::shared(&ctx.snapshots)?;
        let fips: Vec<String> = match plevel {
            ProfileLevel::Tract => vec![geo.geoid.clone()],
            ProfileLevel::Town => {
                let (t, shared) = super::svi::town_tracts(geos, &geo.name);
                if shared {
                    caveats.push(super::svi::shared_tracts_caveat(&geo.name, t.len()));
                }
                t
            }
            ProfileLevel::County => geos
                .at_level(GeoLevel::Tract)
                .filter(|t| t.county_geoid == geo.geoid)
                .map(|t| t.geoid.clone())
                .collect(),
        };
        let tracts: Vec<_> = fips.iter().filter_map(|f| s.tract(f)).collect();
        if tracts.is_empty() {
            None
        } else {
            sources.push(s.source.clone());
            let ranked: Vec<(String, f64)> = tracts
                .iter()
                .filter_map(|t| t.ma.overall.map(|r| (t.fips.clone(), r)))
                .collect();
            let single = (plevel == ProfileLevel::Tract).then(|| tracts[0]);
            Some(SviSection {
                release: "SVI 2022".into(),
                tracts: tracts.len(),
                tract_ranks_massachusetts: single.map(|t| t.ma),
                tract_ranks_national: single.map(|t| t.us),
                median_overall_massachusetts: median(ranked.iter().map(|r| r.1).collect()),
                tracts_high_vulnerability: ranked.iter().filter(|r| r.1 >= 0.75).count(),
                most_vulnerable_tract: ranked.iter().max_by(|a, b| a.1.total_cmp(&b.1)).cloned(),
            })
        }
    } else {
        None
    };

    // Summary.
    let mode = match fig.mode {
        SourceMode::Snapshot => "bundled snapshot",
        SourceMode::Cache => "cached copy of the Census API",
        _ => "live Census API",
    };
    let mut summary = format!(
        "**Community health profile: {}** ({}{})\n\n",
        area.name,
        match plevel {
            ProfileLevel::County => "county".to_string(),
            ProfileLevel::Town => format!("{county}; Census name {}", area.census_name),
            ProfileLevel::Tract => format!("census tract in {county}"),
        },
        area.population_density
            .map(|d| format!("; {} residents per square mile", md::num(d, 0)))
            .unwrap_or_default()
    );
    summary.push_str(&format!(
        "Demographic, social, and economic figures: U.S. Census Bureau, American Community Survey \
         {} 5-year estimates ({mode}), each cited by detailed table ID. ± is the 90% margin of \
         error; † marks a coefficient of variation above 30% (unreliable).\n\n",
        fig.period()
    ));
    let mut section = String::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let flush = |summary: &mut String, section: &str, rows: &mut Vec<Vec<String>>| {
        if rows.is_empty() {
            return;
        }
        summary.push_str(&format!("**{section}**\n\n"));
        summary.push_str(&md::table(
            &["Indicator", "Estimate", "MA", "US", "ACS table"],
            rows,
        ));
        summary.push('\n');
        rows.clear();
    };
    for i in &indicators {
        if i.section != section {
            flush(&mut summary, &section, &mut rows);
            section = i.section.clone();
        }
        let def = census::indicator(&i.id);
        let est = match fig.values.get(&i.id) {
            Some(e) => format!(
                "{} {}{}",
                acs::fmt_value(def, e),
                acs::fmt_moe(def, e),
                if i.unreliable { " †" } else { "" }
            ),
            None => "not available".into(),
        };
        let cmp = |m: &std::collections::BTreeMap<String, census::Estimate>| {
            m.get(&i.id)
                .map(|e| acs::fmt_value(def, e))
                .unwrap_or("—".into())
        };
        rows.push(vec![
            i.label.clone(),
            est,
            cmp(&fig.ma),
            cmp(&fig.us),
            i.table.clone(),
        ]);
    }
    flush(&mut summary, &section, &mut rows);
    if let Some(c) = &chronic {
        summary.push_str(&format!(
            "**Chronic disease and health-related social needs (CDC PLACES {} release, adults)**\n\n",
            c.release
        ));
        let t: Vec<Vec<String>> = c
            .rows
            .iter()
            .map(|r| {
                vec![
                    r.measure.clone(),
                    match (r.low, r.high) {
                        (Some(l), Some(h)) => format!("{:.1}% ({l:.1}–{h:.1})", r.value),
                        _ => format!("{:.1}%", r.value),
                    },
                    r.massachusetts
                        .map(|v| format!("{v:.1}%"))
                        .unwrap_or("—".into()),
                    r.united_states
                        .map(|v| format!("{v:.1}%"))
                        .unwrap_or("—".into()),
                    r.data_year.clone(),
                ]
            })
            .collect();
        summary.push_str(&md::table(
            &["Measure", "Estimate (95% CI)", "MA", "US", "Data year"],
            &t,
        ));
        summary.push_str(&format!("\nMethod: {}.\n\n", c.method));
    }
    if let Some(s) = &svi {
        summary.push_str("**Social vulnerability (CDC/ATSDR SVI 2022, percentile ranks 0–1; higher is more vulnerable)**\n\n");
        match (&s.tract_ranks_massachusetts, &s.tract_ranks_national) {
            (Some(m), Some(u)) => {
                let f = |v: Option<f64>| v.map(|x| format!("{x:.2}")).unwrap_or("—".into());
                summary.push_str(&md::table(
                    &["Ranking", "Overall", "Socioeconomic", "Household", "Minority status", "Housing/transport"],
                    &[
                        vec!["Within Massachusetts".into(), f(m.overall), f(m.theme1), f(m.theme2), f(m.theme3), f(m.theme4)],
                        vec!["National".into(), f(u.overall), f(u.theme1), f(u.theme2), f(u.theme3), f(u.theme4)],
                    ],
                ));
            }
            _ => summary.push_str(&format!(
                "{} tracts; median overall percentile within Massachusetts {}; {} tract(s) at or above the 75th percentile{}.\n",
                s.tracts,
                s.median_overall_massachusetts.map(|m| format!("{m:.2}")).unwrap_or("—".into()),
                s.tracts_high_vulnerability,
                s.most_vulnerable_tract
                    .as_ref()
                    .map(|(f, r)| format!("; most vulnerable tract {f} ({r:.2})"))
                    .unwrap_or_default()
            )),
        }
        summary.push('\n');
    }
    summary.push_str(&format!("**Births and deaths:** {VITAL_STATISTICS}\n"));

    if !unreliable.is_empty() {
        caveats.push(format!(
            "{} figure(s) have a coefficient of variation above 30% and are unreliable: {}. \
             Compare with neighboring areas or the county, and report the margin of error.",
            unreliable.len(),
            unreliable
                .iter()
                .map(|i| i.label.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    caveats.push(format!(
        "ACS 5-year estimates describe {} as a whole and carry sampling error; differences \
         smaller than the margins of error may not be real. Dollar figures are in {}-dollars.",
        fig.period(),
        fig.vintage
    ));
    if plevel == ProfileLevel::Town {
        caveats.push(
            "Cities and towns are Census county subdivisions; tract-based summaries (SVI) use the \
             tracts that belong mainly to the town (largest share of their census blocks)."
                .into(),
        );
    }
    let methods = Source::computed(
        "Margins of error for derived ACS estimates: U.S. Census Bureau, Understanding and Using American Community Survey Data (2020), chapter 8",
        "PHN MCP server calculation",
    );
    sources.push(methods);
    Ok(Envelope::new(
        summary,
        ProfileResult {
            area,
            acs_period: fig.period(),
            acs_mode: fig.mode,
            unreliable_count: unreliable.len(),
            indicators,
            chronic_disease: chronic,
            svi,
            vital_statistics: VITAL_STATISTICS.into(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 4 exit criterion (offline half): a profile builds for every one
    /// of the 351 cities and towns from the bundled snapshots, and every ACS
    /// figure carries its table ID and a margin of error.
    #[tokio::test]
    async fn profile_works_offline_for_all_351_towns() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        assert!(ctx.http.is_offline());
        let towns: Vec<String> = ctx
            .ma
            .municipalities
            .iter()
            .map(|m| m.name.clone())
            .collect();
        assert_eq!(towns.len(), 351);
        for t in towns {
            let env = build_community_health_profile(
                &ctx,
                ProfileArgs {
                    area: Some(t.clone()),
                    level: Some(ProfileLevel::Town),
                    ..ProfileArgs::default()
                },
            )
            .await
            .unwrap_or_else(|e| panic!("{t}: {e}"));
            let d = &env.data;
            assert_eq!(d.area.name, t);
            assert_eq!(d.acs_mode, SourceMode::Snapshot);
            let pop = d.indicators.iter().find(|i| i.id == "pop_total").unwrap();
            assert!(pop.value.unwrap() > 0.0, "{t}");
            for i in &d.indicators {
                assert!(i.table.len() == 6, "{t} {}", i.id);
                if i.value.is_some() && i.id != "pop_total" {
                    assert!(i.moe.is_some() || i.massachusetts.is_some(), "{t} {}", i.id);
                }
            }
            assert!(
                d.chronic_disease
                    .as_ref()
                    .is_some_and(|c| !c.rows.is_empty()),
                "{t}"
            );
            assert!(d.svi.as_ref().is_some_and(|s| s.tracts > 0), "{t}");
            assert!(
                env.sources
                    .iter()
                    .any(|s| s.title.contains("American Community Survey"))
            );
            assert!(env.summary.contains("B01001") && env.summary.contains("C16001"));
        }
    }

    #[tokio::test]
    async fn county_and_tract_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = build_community_health_profile(
            &ctx,
            ProfileArgs {
                area: Some("Hampden County".into()),
                ..ProfileArgs::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(env.data.area.level, ProfileLevel::County);
        assert!(env.data.svi.as_ref().unwrap().tracts > 50);
        let geos = MaGeographies::shared(&ctx.snapshots).unwrap();
        let tract = geos.tracts_in_town("Chelsea")[0].geoid.clone();
        let env = build_community_health_profile(
            &ctx,
            ProfileArgs {
                area: Some(tract),
                ..ProfileArgs::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(env.data.area.level, ProfileLevel::Tract);
        assert!(
            env.data
                .svi
                .as_ref()
                .unwrap()
                .tract_ranks_massachusetts
                .is_some()
        );
        let err = build_community_health_profile(
            &ctx,
            ProfileArgs {
                area: Some("01002".into()),
                ..ProfileArgs::default()
            },
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("ZCTA") || err.to_string().contains("zcta"),
            "{err}"
        );
    }
}
