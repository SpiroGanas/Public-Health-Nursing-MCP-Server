//! Resolving the area a preparedness tool is asked about: Massachusetts, a
//! county, or one of the 351 cities and towns (default: the saved town).

use phn_core::{PhnError, Result};
use phn_data::boundaries::{BoundaryLayer, Layer};
use phn_data::geography::MaGeographies;
use phn_data::projection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// Geographic level of an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AreaLevel {
    State,
    County,
    Town,
}

/// A resolved area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Area {
    pub level: AreaLevel,
    /// "Massachusetts", "Bristol County", or the town name.
    pub name: String,
    /// Census GEOID: "25", a county ("25005"), or a town (10 digits).
    pub geoid: String,
    /// County FIPS for counties and towns.
    pub county_fips: Option<String>,
    /// County name without "County".
    pub county: Option<String>,
    /// Town name for towns.
    pub town: Option<String>,
}

impl Area {
    /// The whole state.
    pub fn state() -> Self {
        Self {
            level: AreaLevel::State,
            name: "Massachusetts".into(),
            geoid: "25".into(),
            county_fips: None,
            county: None,
            town: None,
        }
    }
}

/// County name ("Bristol") for a FIPS code.
pub fn county_name(ctx: &Context, fips: &str) -> Option<String> {
    ctx.ma
        .municipalities
        .iter()
        .find(|m| m.county_fips == fips)
        .map(|m| m.county.clone())
}

/// Resolve `query` (or the saved town when None) to an area. `default_state`
/// decides what an empty query with no saved town means.
pub fn resolve(ctx: &Context, query: Option<&str>, default_state: bool) -> Result<Area> {
    let q = query.map(str::trim).filter(|q| !q.is_empty());
    let q = match q {
        Some(q) => q.to_string(),
        None => match ctx.settings().jurisdiction.town {
            Some(t) => t,
            None if default_state => return Ok(Area::state()),
            None => {
                return Err(PhnError::InvalidInput(
                    "name a Massachusetts city, town, or county (or save your town with \
                     configure_jurisdiction)"
                        .into(),
                ));
            }
        },
    };
    let n = phn_core::text::normalize(&q);
    if matches!(
        n.as_str(),
        "massachusetts" | "ma" | "mass" | "statewide" | "commonwealth" | "state"
    ) {
        return Ok(Area::state());
    }
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let looks_like_county = n.ends_with(" county") || n.ends_with(" co");
    if looks_like_county && let Some(c) = geos.county(&q) {
        return Ok(county_area(c));
    }
    if let Some(m) = ctx.ma.town(&q) {
        let geoid = geos
            .town(&m.name)
            .map(|g| g.geoid.clone())
            .unwrap_or_default();
        return Ok(Area {
            level: AreaLevel::Town,
            name: m.name.clone(),
            geoid,
            county_fips: Some(m.county_fips.clone()),
            county: Some(m.county.clone()),
            town: Some(m.name.clone()),
        });
    }
    if let Some(c) = geos.county(&q) {
        return Ok(county_area(c));
    }
    let suggestions: Vec<String> = ctx
        .ma
        .suggest_towns(&q, 3)
        .into_iter()
        .map(|m| m.name.clone())
        .collect();
    Err(PhnError::NotFound(format!(
        "'{q}' is not a Massachusetts city, town, or county{}",
        if suggestions.is_empty() {
            String::new()
        } else {
            format!(". Did you mean {}?", suggestions.join(", "))
        }
    )))
}

fn county_area(c: &phn_data::geography::Geography) -> Area {
    let short = c
        .name
        .strip_suffix(" County")
        .unwrap_or(&c.name)
        .to_string();
    Area {
        level: AreaLevel::County,
        name: c.name.clone(),
        geoid: c.geoid.clone(),
        county_fips: Some(c.geoid.clone()),
        county: Some(short),
        town: None,
    }
}

/// A representative point (latitude, longitude) for a town: the label point of
/// its largest polygon in the bundled boundaries.
pub fn town_point(ctx: &Context, town: &str) -> Result<(f64, f64)> {
    let layer = BoundaryLayer::shared(&ctx.snapshots, Layer::Town)?;
    let f = layer
        .get(town)
        .ok_or_else(|| PhnError::NotFound(format!("no boundary for {town}")))?;
    let (x, y) = f.label_point();
    Ok(projection::from_mass_mainland(x, y))
}

/// The towns in an area (every town for the state).
pub fn towns_in(ctx: &Context, area: &Area) -> Vec<String> {
    ctx.ma
        .municipalities
        .iter()
        .filter(|m| match area.level {
            AreaLevel::State => true,
            AreaLevel::County => area.county_fips.as_deref() == Some(m.county_fips.as_str()),
            AreaLevel::Town => area.town.as_deref() == Some(m.name.as_str()),
        })
        .map(|m| m.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn resolves_towns_counties_and_the_state() {
        let (_d, c) = ctx();
        let a = resolve(&c, Some("Fall River"), true).unwrap();
        assert_eq!(a.level, AreaLevel::Town);
        assert_eq!(a.county_fips.as_deref(), Some("25005"));
        assert_eq!(a.geoid.len(), 10);
        let a = resolve(&c, Some("Bristol County"), true).unwrap();
        assert_eq!(a.level, AreaLevel::County);
        assert_eq!(a.county.as_deref(), Some("Bristol"));
        assert_eq!(resolve(&c, None, true).unwrap().level, AreaLevel::State);
        assert!(resolve(&c, None, false).is_err());
        assert!(
            resolve(&c, Some("Springfeld"), true)
                .unwrap_err()
                .to_string()
                .contains("Springfield")
        );
        // Worcester is both a city and a county; "County" picks the county.
        assert_eq!(
            resolve(&c, Some("Worcester"), true).unwrap().level,
            AreaLevel::Town
        );
        assert_eq!(
            resolve(&c, Some("Worcester County"), true).unwrap().level,
            AreaLevel::County
        );
        assert_eq!(
            towns_in(&c, &resolve(&c, Some("Nantucket County"), true).unwrap()).len(),
            1
        );
    }

    #[test]
    fn town_points_fall_inside_massachusetts() {
        let (_d, c) = ctx();
        let (lat, lon) = town_point(&c, "Boston").unwrap();
        assert!(
            (42.25..42.45).contains(&lat) && (-71.2..-70.95).contains(&lon),
            "{lat},{lon}"
        );
        let (lat, lon) = town_point(&c, "Pittsfield").unwrap();
        assert!(
            (42.4..42.5).contains(&lat) && (-73.35..-73.15).contains(&lon),
            "{lat},{lon}"
        );
    }
}
