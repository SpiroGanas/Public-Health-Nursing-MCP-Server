//! Shared ACS access for the equity tools: live API (when reachable) with
//! the bundled snapshot as fallback, plus formatting helpers.

use std::collections::BTreeMap;

use phn_core::{PhnError, Result, Source, SourceMode};
use phn_data::census::{
    self, AcsApi, AcsSnapshot, ApiGeo, Estimate, GeoLevel, IndicatorDef, IndicatorKind,
};
use phn_data::geography::Geography;

use crate::context::Context;
use crate::md;

/// ACS figures for one area with Massachusetts and U.S. comparisons.
#[derive(Debug, Clone)]
pub(crate) struct AcsFigures {
    pub values: BTreeMap<String, Estimate>,
    pub ma: BTreeMap<String, Estimate>,
    pub us: BTreeMap<String, Estimate>,
    pub vintage: u32,
    pub mode: SourceMode,
    pub sources: Vec<Source>,
    pub caveats: Vec<String>,
}

impl AcsFigures {
    /// "2020–2024".
    pub fn period(&self) -> String {
        format!("{}–{}", self.vintage - 4, self.vintage)
    }
}

fn snapshot_values(snap: &AcsSnapshot, geoid: &str) -> Option<BTreeMap<String, Estimate>> {
    snap.row(geoid).map(|r| r.values.clone())
}

/// Fetch ACS figures for a county, town, or tract.
pub(crate) async fn figures(ctx: &Context, geo: &Geography) -> Result<AcsFigures> {
    let snap = AcsSnapshot::shared(&ctx.snapshots)?;
    let ma_snap = snapshot_values(snap, "25").unwrap_or_default();
    let us_snap = snapshot_values(snap, "US").unwrap_or_default();
    let mut caveats = Vec::new();

    if !ctx.http.is_offline()
        && let Some(api_geo) = ApiGeo::from_geoid(geo.level, &geo.geoid)
    {
        let vintage = census::latest_vintage(&ctx.http, ctx.cache.as_ref()).await;
        let api = AcsApi::new(vintage);
        match api.fetch(&ctx.http, ctx.cache.as_ref(), &api_geo).await {
            Ok(live) => {
                let mut sources = vec![live.source()];
                let (ma, us) = if vintage == snap.vintage {
                    sources.push(snap.source());
                    (ma_snap, us_snap)
                } else {
                    let ma = api
                        .fetch(&ctx.http, ctx.cache.as_ref(), &ApiGeo::State("25".into()))
                        .await;
                    let us = api
                        .fetch(&ctx.http, ctx.cache.as_ref(), &ApiGeo::Nation)
                        .await;
                    match (ma, us) {
                        (Ok(m), Ok(u)) => (m.values, u.values),
                        _ => {
                            caveats.push(format!(
                                "Massachusetts and U.S. comparisons are from the bundled ACS \
                                 {}–{} snapshot because the newer comparison figures could not \
                                 be fetched.",
                                snap.vintage - 4,
                                snap.vintage
                            ));
                            sources.push(snap.source());
                            (ma_snap, us_snap)
                        }
                    }
                };
                if let Some(r) = &live.fallback_reason {
                    caveats.push(format!("Census API: {r}"));
                }
                return Ok(AcsFigures {
                    values: live.values,
                    ma,
                    us,
                    vintage,
                    mode: live.mode,
                    sources,
                    caveats,
                });
            }
            Err(e) => {
                caveats.push(format!(
                    "The live Census API could not be used ({}); figures are from the bundled \
                     ACS {}–{} snapshot{}.",
                    phn_data::apifetch::reason(&e),
                    snap.vintage - 4,
                    snap.vintage,
                    if vintage > snap.vintage {
                        format!(
                            ", although the {}–{vintage} release is now published",
                            vintage - 4
                        )
                    } else {
                        " (the latest release)".to_string()
                    }
                ));
            }
        }
    }
    let values = snapshot_values(snap, &geo.geoid).ok_or_else(|| {
        PhnError::NotFound(format!(
            "the bundled ACS snapshot has no estimates for {} ({}); tracts with no residents are \
             not included. With network access and a Census API key the live API can be used.",
            geo.name, geo.geoid
        ))
    })?;
    Ok(AcsFigures {
        values,
        ma: ma_snap,
        us: us_snap,
        vintage: snap.vintage,
        mode: SourceMode::Snapshot,
        sources: vec![snap.source()],
        caveats,
    })
}

/// Format an estimate for its indicator kind.
pub(crate) fn fmt_value(def: Option<&IndicatorDef>, e: &Estimate) -> String {
    match def.map(|d| d.kind) {
        Some(IndicatorKind::Percent) => format!("{:.1}%", e.value),
        Some(IndicatorKind::Dollars) => format!("${}", md::num(e.value, 0)),
        Some(IndicatorKind::Median) => format!("{:.1}", e.value),
        _ => md::num(e.value, 0),
    }
}

/// Format the margin of error.
pub(crate) fn fmt_moe(def: Option<&IndicatorDef>, e: &Estimate) -> String {
    match (e.moe, def.map(|d| d.kind)) {
        (None, _) => "—".into(),
        (Some(m), Some(IndicatorKind::Percent)) => format!("±{m:.1}"),
        (Some(m), Some(IndicatorKind::Dollars)) => format!("±${}", md::num(m, 0)),
        (Some(m), Some(IndicatorKind::Median)) => format!("±{m:.1}"),
        (Some(m), _) => format!("±{}", md::num(m, 0)),
    }
}

/// The allowed levels for ACS profiles.
pub(crate) fn check_level(geo: &Geography) -> Result<()> {
    match geo.level {
        GeoLevel::County | GeoLevel::Town | GeoLevel::Tract => Ok(()),
        _ => Err(PhnError::InvalidInput(format!(
            "{} is a {}; community profiles are available for counties, cities and towns, and \
             census tracts",
            geo.name,
            geo.level.label()
        ))),
    }
}
