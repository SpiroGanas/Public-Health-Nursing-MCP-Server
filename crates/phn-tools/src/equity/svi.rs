//! EQ-02 `get_social_vulnerability_index`.

use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_data::census::GeoLevel;
use phn_data::geography::{Geography, MaGeographies};
use phn_data::svi::{self, Ranks, SviSnapshot, SviTract};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::profile::ProfileLevel;
use crate::context::Context;
use crate::md;

/// Which ranking to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Ranking {
    /// Percentile among Massachusetts tracts (default).
    #[default]
    Massachusetts,
    /// Percentile among all U.S. tracts.
    National,
}

/// The theme to sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SviTheme {
    #[default]
    Overall,
    /// Theme 1: socioeconomic status.
    Socioeconomic,
    /// Theme 2: household characteristics.
    Household,
    /// Theme 3: racial and ethnic minority status.
    MinorityStatus,
    /// Theme 4: housing type and transportation.
    HousingTransportation,
}

impl SviTheme {
    fn id(self) -> &'static str {
        match self {
            Self::Overall => "overall",
            Self::Socioeconomic => "theme1",
            Self::Household => "theme2",
            Self::MinorityStatus => "theme3",
            Self::HousingTransportation => "theme4",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Overall => "overall",
            Self::Socioeconomic => "socioeconomic status",
            Self::Household => "household characteristics",
            Self::MinorityStatus => "racial and ethnic minority status",
            Self::HousingTransportation => "housing type and transportation",
        }
    }
}

/// Arguments for `get_social_vulnerability_index`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SviArgs {
    /// A Massachusetts city or town, county, or census tract (11-digit GEOID).
    /// Default: your configured town.
    #[serde(default)]
    pub area: Option<String>,
    #[serde(default)]
    pub level: Option<ProfileLevel>,
    /// massachusetts (default) or national percentile ranks.
    #[serde(default)]
    pub ranking: Option<Ranking>,
    /// Sort tracts by this theme (default overall).
    #[serde(default)]
    pub theme: Option<SviTheme>,
    /// Most vulnerable tracts to list (default 10, max 200).
    #[serde(default)]
    pub top: Option<usize>,
}

/// One tract in the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SviRow {
    pub fips: String,
    pub name: String,
    pub town: Option<String>,
    pub population: Option<f64>,
    /// Ranks in the chosen ranking (0–1).
    pub ranks: Ranks,
    /// Number of the 16 variables at or above the 90th percentile (Massachusetts file).
    pub flags: Option<f64>,
}

/// Result of `get_social_vulnerability_index`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SviResult {
    pub area: String,
    pub ranking: Ranking,
    pub theme: SviTheme,
    pub tracts_total: usize,
    pub tracts_ranked: usize,
    /// Tracts by quartile of the chosen rank: [0–0.25), [0.25–0.5), [0.5–0.75), [0.75–1].
    pub quartile_counts: [usize; 4],
    pub median_rank: Option<f64>,
    /// Most vulnerable tracts first.
    pub tracts: Vec<SviRow>,
    /// The 16 variables (percent estimates) for a single-tract request.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<(String, f64)>,
}

/// The census tracts used for a town's tract-based summaries: tracts with
/// land assigned to the town (the town holds the largest share of their
/// census blocks), or, for small towns without one, every tract with land
/// that overlaps the town. The flag is true in the second case. Water-only
/// tracts (no land, no residents) are left out.
pub(crate) fn town_tracts(geos: &MaGeographies, town: &str) -> (Vec<String>, bool) {
    let land = |t: &Geography| t.aland_sqmi.is_none_or(|a| a > 0.0);
    let main: Vec<String> = geos
        .tracts_in_town(town)
        .into_iter()
        .filter(|t| land(t))
        .map(|t| t.geoid.clone())
        .collect();
    if !main.is_empty() {
        return (main, false);
    }
    (
        geos.tracts_overlapping_town(town)
            .into_iter()
            .filter(|(t, _)| land(t))
            .map(|(t, _)| t.geoid.clone())
            .collect(),
        true,
    )
}

/// Caveat for a town whose tracts all belong mainly to other towns.
pub(crate) fn shared_tracts_caveat(town: &str, n: usize) -> String {
    format!(
        "No census tract lies mainly in {town}; tract-based figures are for the {n} tract{} that \
         overlap it, which also cover neighboring towns.",
        if n == 1 { "" } else { "s" }
    )
}

/// SVI percentile ranks by tract for a town, county, or tract.
pub fn get_social_vulnerability_index(ctx: &Context, args: SviArgs) -> Result<Envelope<SviResult>> {
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let snap = SviSnapshot::shared(&ctx.snapshots)?;
    let query = match args.area.as_deref().filter(|a| !a.trim().is_empty()) {
        Some(a) => a.to_string(),
        None => ctx.settings().jurisdiction.town.ok_or_else(|| {
            PhnError::InvalidInput(
                "give an area (town, county, or tract), or set your town with configure_jurisdiction"
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
    let mut shared_tracts = false;
    let fips: Vec<String> = match geo.level {
        GeoLevel::Tract => vec![geo.geoid.clone()],
        GeoLevel::Town => {
            let (t, shared) = town_tracts(geos, &geo.name);
            shared_tracts = shared;
            t
        }
        GeoLevel::County => geos
            .at_level(GeoLevel::Tract)
            .filter(|t| t.county_geoid == geo.geoid)
            .map(|t| t.geoid.clone())
            .collect(),
        _ => {
            return Err(PhnError::InvalidInput(format!(
                "SVI is reported by census tract; give a town, county, or tract (not a {})",
                geo.level.label()
            )));
        }
    };
    let ranking = args.ranking.unwrap_or_default();
    let theme = args.theme.unwrap_or_default();
    let top = args.top.unwrap_or(10).clamp(1, 200);
    let tracts: Vec<&SviTract> = fips.iter().filter_map(|f| snap.tract(f)).collect();
    if tracts.is_empty() {
        return Err(PhnError::NotFound(format!("no SVI 2022 tracts found for {}", geo.name)));
    }
    let ranks_of = |t: &SviTract| match ranking {
        Ranking::Massachusetts => t.ma,
        Ranking::National => t.us,
    };
    let mut rows: Vec<SviRow> = tracts
        .iter()
        .map(|t| {
            let g = geos.get(&t.fips);
            SviRow {
                fips: t.fips.clone(),
                name: g.map(|g| g.name.clone()).unwrap_or_else(|| t.location.clone()),
                town: g.and_then(|g| g.town.clone()),
                population: t.population,
                ranks: ranks_of(t),
                flags: t.flags,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.ranks
            .get(theme.id())
            .unwrap_or(-1.0)
            .total_cmp(&a.ranks.get(theme.id()).unwrap_or(-1.0))
    });
    let ranked: Vec<f64> = rows.iter().filter_map(|r| r.ranks.get(theme.id())).collect();
    let mut q = [0usize; 4];
    for r in &ranked {
        q[((r * 4.0).floor() as usize).min(3)] += 1;
    }
    let median_rank = {
        let mut v = ranked.clone();
        v.sort_by(f64::total_cmp);
        (!v.is_empty()).then(|| {
            let n = v.len();
            if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
        })
    };
    let total = rows.len();
    let variables: Vec<(String, f64)> = if geo.level == GeoLevel::Tract {
        svi::VARIABLES
            .iter()
            .filter_map(|(id, label)| tracts[0].percents.get(*id).map(|v| (label.to_string(), *v)))
            .collect()
    } else {
        Vec::new()
    };
    rows.truncate(top);

    let rk = match ranking {
        Ranking::Massachusetts => "within Massachusetts",
        Ranking::National => "nationally",
    };
    let mut summary = format!(
        "**CDC/ATSDR Social Vulnerability Index 2022 — {}** (percentile ranks {rk}, 0 = least and 1 = most vulnerable; sorted by {})\n\n",
        geo.name,
        theme.label()
    );
    if total > 1 {
        summary.push_str(&format!(
            "{total} tracts; median {} rank {}; by quartile: {} low (0–0.25), {} low-moderate, {} moderate-high, {} high (0.75–1).\n\n",
            theme.label(),
            median_rank.map(|m| format!("{m:.2}")).unwrap_or("—".into()),
            q[0], q[1], q[2], q[3]
        ));
    }
    let f = |v: Option<f64>| v.map(|x| format!("{x:.2}")).unwrap_or("—".into());
    let t: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                format!("{} ({})", r.name, r.fips),
                r.town.clone().unwrap_or_default(),
                r.population.map(|p| md::num(p, 0)).unwrap_or("—".into()),
                f(r.ranks.overall),
                f(r.ranks.theme1),
                f(r.ranks.theme2),
                f(r.ranks.theme3),
                f(r.ranks.theme4),
                r.flags.map(|x| format!("{x:.0}")).unwrap_or("—".into()),
            ]
        })
        .collect();
    summary.push_str(&md::table(
        &["Tract", "Town", "Population", "Overall", "Socioeconomic", "Household", "Minority status", "Housing/transport", "Flags"],
        &t,
    ));
    if total > rows.len() {
        summary.push_str(&format!("\nShowing the {} most vulnerable of {total} tracts.\n", rows.len()));
    }
    if !variables.is_empty() {
        summary.push_str("\n**Tract variables (percent, ACS 2018–2022)**\n\n");
        let v: Vec<Vec<String>> = variables
            .iter()
            .map(|(l, x)| vec![l.clone(), format!("{x:.1}%")])
            .collect();
        summary.push_str(&md::table(&["Variable", "Percent"], &v));
    }
    summary.push_str(
        "\nTo map these ranks, pass the tract GEOIDs and ranks to create_choropleth_map (geography tract).\n",
    );
    let mut caveats = vec![
        "SVI ranks are relative percentiles, not measures of absolute need, and are not comparable across SVI releases or between the state and national rankings.".to_string(),
        "SVI 2022 is built from ACS 2018–2022 tract estimates, which have sampling error; flags count variables at or above the 90th percentile within Massachusetts.".to_string(),
        "Tracts with no residents have no ranks.".to_string(),
    ];
    if shared_tracts {
        caveats.push(shared_tracts_caveat(&geo.name, total));
    }
    Ok(Envelope::new(
        summary,
        SviResult {
            area: geo.name.clone(),
            ranking,
            theme,
            tracts_total: total,
            tracts_ranked: ranked.len(),
            quartile_counts: q,
            median_rank,
            tracts: rows,
            variables,
        },
    )
    .source(snap.source.clone())
    .source(geos.source.clone())
    .caveats(caveats)
    .privacy(Privacy::none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn town_county_and_tract() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = get_social_vulnerability_index(
            &ctx,
            SviArgs {
                area: Some("Springfield".into()),
                ..SviArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.tracts_total > 30);
        assert_eq!(env.data.tracts.len(), 10);
        let r: Vec<f64> = env.data.tracts.iter().filter_map(|t| t.ranks.overall).collect();
        assert!(r.windows(2).all(|w| w[0] >= w[1]));
        let fips = env.data.tracts[0].fips.clone();
        let env = get_social_vulnerability_index(
            &ctx,
            SviArgs {
                area: Some(fips),
                ranking: Some(Ranking::National),
                ..SviArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.tracts_total, 1);
        assert_eq!(env.data.variables.len(), 16);
        let env = get_social_vulnerability_index(
            &ctx,
            SviArgs {
                area: Some("Dukes County".into()),
                theme: Some(SviTheme::HousingTransportation),
                ..SviArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.tracts_total >= 4);
    }

    /// Every town gets an answer, including the small towns whose tracts
    /// belong mainly to a neighbor (they get the overlapping tracts).
    #[test]
    fn every_town_has_tracts() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        for m in &ctx.ma.municipalities {
            let env = get_social_vulnerability_index(
                &ctx,
                SviArgs {
                    area: Some(m.name.clone()),
                    level: Some(ProfileLevel::Town),
                    ..SviArgs::default()
                },
            )
            .unwrap_or_else(|e| panic!("{}: {e}", m.name));
            assert!(env.data.tracts_total > 0, "{}", m.name);
        }
        let env = get_social_vulnerability_index(
            &ctx,
            SviArgs {
                area: Some("Alford".into()),
                ..SviArgs::default()
            },
        )
        .unwrap();
        assert!(env.caveats.iter().any(|c| c.contains("No census tract lies mainly in Alford")));
    }
}
