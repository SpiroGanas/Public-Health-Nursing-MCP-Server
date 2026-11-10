//! ENV-02 `air_quality_protective_guidance`: current and forecast Air Quality
//! Index (AirNow) for Massachusetts reporting areas, with protective actions
//! for sensitive groups, the public, schools, and outdoor events.

use std::collections::BTreeSet;

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::airnow::{self, AqiRecord, RecordKind};
use phn_data::projection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, AirQuality, BreakpointTable, Confidence};
use crate::md;

/// Arguments for `air_quality_protective_guidance`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AirQualityArgs {
    /// A Massachusetts city or town, a county, or "Massachusetts" (every reporting area).
    /// Default: your saved town; otherwise the whole state.
    pub area: Option<String>,
    /// An AQI value you already have (for example from a forecast or news report); guidance
    /// is given for it even without a live lookup.
    pub aqi: Option<u32>,
    /// The pollutant for `aqi` or `concentration`: "PM2.5", "PM10", "OZONE", "CO", "SO2", or "NO2".
    pub pollutant: Option<String>,
    /// A concentration to convert to an AQI (µg/m³ for PM, ppm for ozone and CO, ppb for SO2 and
    /// NO2), averaged over the pollutant's AQI period (24 hours for PM, 8 hours for ozone).
    pub concentration: Option<f64>,
    /// Ozone averaging period for `concentration`: "8-hour" (default) or "1-hour".
    pub ozone_averaging: Option<String>,
}

/// One AQI reading or forecast.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AqiValue {
    /// "observed", "yesterday", or "forecast".
    pub kind: String,
    /// Date (YYYY-MM-DD).
    pub date: String,
    /// Observation hour ending and time zone, e.g. "14:00 EST".
    pub time: Option<String>,
    pub pollutant: String,
    pub aqi: Option<u32>,
    pub category: String,
    /// True for the pollutant with the highest AQI.
    pub primary: bool,
    pub action_day: bool,
}

/// AQI for one AirNow reporting area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReportingAreaAqi {
    pub reporting_area: String,
    /// Distance from the town's center point, km (towns only).
    pub distance_km: Option<f64>,
    pub current: Vec<AqiValue>,
    pub yesterday: Vec<AqiValue>,
    pub forecasts: Vec<AqiValue>,
    /// Forecaster's discussion, when given.
    pub discussion: Option<String>,
    pub forecaster: Option<String>,
}

/// Guidance for an AQI category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoryGuidance {
    pub category: String,
    pub aqi_range: String,
    pub color: String,
    pub health_concern: String,
    /// Sensitive groups for the pollutant(s) involved.
    pub sensitive_groups: Vec<String>,
    pub general_public: Vec<String>,
    pub for_sensitive_groups: Vec<String>,
    pub schools: Vec<String>,
    pub outdoor_events: Vec<String>,
    /// True when an action is a medium-confidence summary rather than transcribed text.
    pub includes_summaries: bool,
}

/// An AQI computed from a concentration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComputedAqi {
    pub pollutant: String,
    pub averaging: String,
    pub concentration: f64,
    pub unit: String,
    /// The concentration after Appendix G truncation.
    pub truncated: f64,
    pub aqi: u32,
    pub category: String,
    pub breakpoints_used: [f64; 4],
}

/// Result of `air_quality_protective_guidance`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AirQualityResult {
    pub area: Area,
    /// "live", "cache", or "unavailable".
    pub airnow_status: String,
    pub product: Option<String>,
    pub reporting_areas: Vec<ReportingAreaAqi>,
    pub computed: Option<ComputedAqi>,
    /// The AQI category guidance is given for (worst of current, today, and tomorrow), when known.
    pub guidance_category: Option<String>,
    pub guidance: Vec<CategoryGuidance>,
    pub tips: Vec<String>,
}

/// AQI from a concentration with Appendix G truncation and Equation 1.
/// Returns the AQI and the breakpoint row used, or None below the table
/// (1-hour ozone under 0.125 ppm) or above a table that does not reach AQI 500
/// (8-hour ozone above 0.200 ppm, 1-hour SO2 above 304 ppb: Appendix G uses
/// another averaging time there). Only a table whose top row ends at AQI 500 is
/// extrapolated.
pub fn aqi_from_concentration(t: &BreakpointTable, c: f64) -> Option<(u32, f64, [f64; 4])> {
    if !(c.is_finite() && c >= 0.0) {
        return None;
    }
    let f = 10f64.powi(t.truncate_decimals as i32);
    // Truncate (not round), guarding against binary representation (0.07 * 1000 = 69.999…).
    let ct = ((c * f) + 1e-9).floor() / f;
    let row = t
        .rows
        .iter()
        .find(|r| ct >= r[0] && ct <= r[1])
        .or_else(|| t.rows.last().filter(|r| ct > r[1] && r[3] >= 500.0))?;
    let i = (row[3] - row[2]) / (row[1] - row[0]) * (ct - row[0]) + row[2];
    Some((i.round() as u32, ct, *row))
}

fn table_for<'a>(
    aq: &'a AirQuality,
    pollutant: &str,
    ozone_avg: &str,
) -> Result<&'a BreakpointTable> {
    let p = normalize_pollutant(pollutant).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "pollutant must be PM2.5, PM10, OZONE, CO, SO2, or NO2 (got '{pollutant}')"
        ))
    })?;
    let key = if p == "OZONE" {
        if ozone_avg.contains('1') {
            "OZONE 1-hour".to_string()
        } else {
            "OZONE 8-hour".to_string()
        }
    } else {
        p.to_string()
    };
    aq.breakpoints
        .iter()
        .find(|b| b.pollutant == key)
        .ok_or_else(|| PhnError::Internal(format!("breakpoints for {key} missing")))
}

fn normalize_pollutant(p: &str) -> Option<&'static str> {
    let n = p
        .trim()
        .to_ascii_uppercase()
        .replace([' ', '.', '_', '-'], "");
    Some(match n.as_str() {
        "PM25" | "FINEPARTICLES" | "PARTICLEPOLLUTION" | "SMOKE" => "PM2.5",
        "PM10" => "PM10",
        "OZONE" | "O3" => "OZONE",
        "CO" | "CARBONMONOXIDE" => "CO",
        "SO2" | "SULFURDIOXIDE" => "SO2",
        "NO2" | "NITROGENDIOXIDE" => "NO2",
        _ => return None,
    })
}

fn guidance_for(aq: &AirQuality, cat_id: &str, pollutants: &BTreeSet<String>) -> CategoryGuidance {
    let c = aq
        .categories
        .iter()
        .find(|c| c.id == cat_id)
        .expect("category exists");
    let pick = |aud: &str| -> Vec<String> {
        aq.actions
            .iter()
            .filter(|a| a.audience == aud && a.category == cat_id)
            .map(|a| a.text.clone())
            .collect()
    };
    let includes_summaries = aq
        .actions
        .iter()
        .any(|a| a.category == cat_id && a.confidence != Confidence::High);
    let groups: Vec<String> = aq
        .sensitive_groups
        .iter()
        .filter(|g| {
            pollutants.is_empty()
                || pollutants
                    .iter()
                    .any(|p| normalize_pollutant(p).is_some_and(|n| g.pollutant == n))
        })
        .map(|g| format!("{}: {}", g.pollutant, g.groups))
        .collect();
    CategoryGuidance {
        category: c.name.clone(),
        aqi_range: format!("{}–{}", c.min, c.max),
        color: c.color.clone(),
        health_concern: c.health_concern.text.clone(),
        sensitive_groups: groups,
        general_public: pick("general_public"),
        for_sensitive_groups: pick("sensitive_groups"),
        schools: pick("schools"),
        outdoor_events: pick("outdoor_events"),
        includes_summaries,
    }
}

fn value(r: &AqiRecord) -> AqiValue {
    AqiValue {
        kind: match r.kind {
            RecordKind::Observed => "observed",
            RecordKind::Yesterday => "yesterday",
            RecordKind::Forecast => "forecast",
        }
        .into(),
        date: r.valid_date.clone(),
        time: r.valid_time.as_ref().map(|t| {
            format!(
                "{t}{}",
                r.time_zone
                    .as_deref()
                    .map(|z| format!(" {z}"))
                    .unwrap_or_default()
            )
        }),
        pollutant: r.parameter.clone(),
        aqi: r.aqi,
        category: r.category.clone(),
        primary: r.primary,
        action_day: r.action_day,
    }
}

fn category_rank(aq: &AirQuality, v: &AqiValue) -> Option<usize> {
    let cat = match v.aqi {
        Some(a) => Some(aq.category_for(a)),
        None => aq.category_named(&v.category),
    }?;
    aq.categories.iter().position(|c| c.id == cat.id)
}

/// Current and forecast AQI with protective actions.
pub async fn air_quality_protective_guidance(
    ctx: &Context,
    args: AirQualityArgs,
) -> Result<Envelope<AirQualityResult>> {
    let aq: AirQuality = content::load(content::AIR_QUALITY)?;
    let area = area::resolve(ctx, args.area.as_deref(), true)?;
    if let Some(a) = args.aqi
        && a > 999
    {
        return Err(PhnError::InvalidInput(
            "aqi must be between 0 and 999".into(),
        ));
    }
    let mut caveats = Vec::new();
    let mut sources: Vec<Source> = Vec::new();

    // Concentration → AQI.
    let computed = match args.concentration {
        Some(c) => {
            let p = args.pollutant.as_deref().ok_or_else(|| {
                PhnError::InvalidInput("give the pollutant with the concentration".into())
            })?;
            let t = table_for(&aq, p, args.ozone_averaging.as_deref().unwrap_or("8-hour"))?;
            let (i, ct, row) = aqi_from_concentration(t, c).ok_or_else(|| {
                match t.rows.last().filter(|r| c > r[1]) {
                    // Above a table that stops short of AQI 500.
                    Some(top) => PhnError::InvalidInput(format!(
                        "{c} {} is above the highest {} breakpoint ({} {}, AQI {}), and this \
                         table does not define higher AQI values. {}",
                        t.unit,
                        t.pollutant,
                        top[1],
                        t.unit,
                        top[3],
                        t.note
                            .as_deref()
                            .unwrap_or("Use the averaging time 40 CFR 58 Appendix G gives for higher values.")
                    )),
                    None => PhnError::InvalidInput(format!(
                        "{c} {} is below the {} {} breakpoints; use the 8-hour average for ozone",
                        t.unit, t.averaging, t.pollutant
                    )),
                }
            })?;
            if i > 500 {
                caveats.push(
                    "The concentration is above the AQI's highest breakpoint; the value was \
                     extrapolated from the top two breakpoints as Appendix G allows."
                        .into(),
                );
            }
            Some(ComputedAqi {
                pollutant: t.pollutant.clone(),
                averaging: t.averaging.clone(),
                concentration: c,
                unit: t.unit.clone(),
                truncated: ct,
                aqi: i,
                category: aq.category_for(i).name.clone(),
                breakpoints_used: row,
            })
        }
        None => None,
    };

    // AirNow.
    let mut airnow_status = "unavailable".to_string();
    let mut product = None;
    let mut reporting_areas: Vec<ReportingAreaAqi> = Vec::new();
    let town_point = match area.level {
        AreaLevel::Town => area::town_point(ctx, area.town.as_deref().unwrap_or_default()).ok(),
        _ => None,
    };
    let fetched = match town_point {
        Some((lat, lon)) => airnow::fetch_near(&ctx.http, ctx.cache.as_ref(), lat, lon).await,
        None => airnow::fetch_file(&ctx.http, ctx.cache.as_ref(), &["MA"]).await,
    };
    match fetched {
        Ok(f) => {
            airnow_status = f.mode.label().to_string();
            product = Some(f.product.clone());
            caveats.extend(f.notes.iter().cloned());
            if f.mode == SourceMode::Cache {
                caveats.push(format!(
                    "AirNow data are from a cached copy fetched {}.",
                    f.as_of
                        .chars()
                        .take(16)
                        .collect::<String>()
                        .replace('T', " ")
                ));
            }
            let mut names: Vec<(String, f64, f64)> = Vec::new();
            for r in f
                .records
                .iter()
                .filter(|r| r.state == "MA" || f.product == "AirNow API")
            {
                if !names.iter().any(|(n, _, _)| n == &r.reporting_area) {
                    names.push((r.reporting_area.clone(), r.latitude, r.longitude));
                }
            }
            let wanted: Vec<(String, Option<f64>)> = match area.level {
                AreaLevel::State => names.iter().map(|(n, _, _)| (n.clone(), None)).collect(),
                AreaLevel::Town => {
                    let (lat, lon) = town_point.unwrap_or_default();
                    names
                        .iter()
                        .map(|(n, la, lo)| {
                            (n.clone(), projection::distance_km((lat, lon), (*la, *lo)))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(n, d)| vec![(n, Some((d * 10.0).round() / 10.0))])
                        .unwrap_or_default()
                }
                AreaLevel::County => {
                    let mut set: Vec<String> = Vec::new();
                    for t in area::towns_in(ctx, &area) {
                        if let Ok((lat, lon)) = area::town_point(ctx, &t)
                            && let Some((n, _, _)) = names.iter().min_by(|a, b| {
                                projection::distance_km((lat, lon), (a.1, a.2))
                                    .total_cmp(&projection::distance_km((lat, lon), (b.1, b.2)))
                            })
                            && !set.contains(n)
                        {
                            set.push(n.clone());
                        }
                    }
                    set.into_iter().map(|n| (n, None)).collect()
                }
            };
            let today = chrono::Local::now().date_naive().to_string();
            for (name, dist) in wanted {
                let recs: Vec<&AqiRecord> = f
                    .records
                    .iter()
                    .filter(|r| r.reporting_area == name)
                    .collect();
                let latest_obs = recs
                    .iter()
                    .filter(|r| r.kind == RecordKind::Observed)
                    .map(|r| (r.valid_date.clone(), r.valid_time.clone()))
                    .max();
                let current: Vec<AqiValue> = recs
                    .iter()
                    .filter(|r| {
                        r.kind == RecordKind::Observed
                            && Some((r.valid_date.clone(), r.valid_time.clone())) == latest_obs
                    })
                    .map(|r| value(r))
                    .collect();
                let yesterday: Vec<AqiValue> = recs
                    .iter()
                    .filter(|r| r.kind == RecordKind::Yesterday)
                    .map(|r| value(r))
                    .collect();
                let mut forecasts: Vec<AqiValue> = recs
                    .iter()
                    .filter(|r| r.kind == RecordKind::Forecast && r.valid_date >= today)
                    .map(|r| value(r))
                    .collect();
                forecasts.sort_by(|a, b| a.date.cmp(&b.date).then(b.primary.cmp(&a.primary)));
                let fc_rec = recs.iter().find(|r| r.kind == RecordKind::Forecast);
                reporting_areas.push(ReportingAreaAqi {
                    reporting_area: name,
                    distance_km: dist,
                    current,
                    yesterday,
                    forecasts,
                    discussion: recs.iter().find_map(|r| r.discussion.clone()),
                    forecaster: fc_rec.and_then(|r| r.agency.clone()),
                });
            }
            let url = if f.product == "AirNow API" {
                airnow::API_BASE
            } else {
                airnow::REPORTING_AREA_URL
            };
            sources.push(Source {
                title: format!("{} (current and forecast AQI)", f.product),
                publisher: "U.S. EPA AirNow; forecasts by the Massachusetts Department of Environmental Protection".into(),
                url: Some(url.into()),
                effective_date: None,
                retrieved: Some(f.as_of.chars().take(19).collect()),
                mode: f.mode,
            });
        }
        Err(e) => caveats.insert(
            0,
            format!(
                "CURRENT AIR QUALITY COULD NOT BE RETRIEVED from AirNow ({}). Check airnow.gov \
                 or MassDEP for today's AQI; guidance for every AQI category is listed instead.",
                phn_data::apifetch::reason(&e)
            ),
        ),
    }

    // Which category to give guidance for.
    let today = chrono::Local::now().date_naive();
    let horizon = (today + chrono::Duration::days(1)).to_string();
    let mut worst: Option<(usize, String)> = None;
    let mut pollutants: BTreeSet<String> = BTreeSet::new();
    let mut consider = |rank: Option<usize>, pollutant: &str, why: String| {
        if let Some(r) = rank {
            if worst.as_ref().is_none_or(|(w, _)| r > *w) {
                worst = Some((r, why));
                pollutants.clear();
            }
            if worst.as_ref().is_some_and(|(w, _)| r == *w) {
                pollutants.insert(pollutant.to_string());
            }
        }
    };
    if let Some(a) = args.aqi {
        let c = aq.category_for(a);
        consider(
            aq.categories.iter().position(|x| x.id == c.id),
            args.pollutant.as_deref().unwrap_or(""),
            format!("your AQI of {a}"),
        );
    }
    if let Some(c) = &computed {
        let cat = aq.category_for(c.aqi);
        consider(
            aq.categories.iter().position(|x| x.id == cat.id),
            &c.pollutant,
            format!("computed AQI {} ({})", c.aqi, c.pollutant),
        );
    }
    for ra in &reporting_areas {
        for v in ra
            .current
            .iter()
            .chain(ra.forecasts.iter().filter(|f| f.date <= horizon))
        {
            consider(
                category_rank(&aq, v),
                &v.pollutant,
                format!(
                    "{} {} {} in {}",
                    v.kind, v.date, v.pollutant, ra.reporting_area
                ),
            );
        }
    }
    let (guidance_category, guidance) = match &worst {
        Some((r, _)) => {
            let id = aq.categories[*r].id.clone();
            (
                Some(aq.categories[*r].name.clone()),
                vec![guidance_for(&aq, &id, &pollutants)],
            )
        }
        None => (
            None,
            aq.categories
                .iter()
                .map(|c| guidance_for(&aq, &c.id, &BTreeSet::new()))
                .collect(),
        ),
    };

    // Summary.
    let mut s = format!("**Air quality — {}**\n\n", area.name);
    if let Some(c) = &computed {
        s.push_str(&format!(
            "{} {} {} ({} average, truncated to {}) → **AQI {} ({})** using breakpoints {}–{} → {}–{}.\n\n",
            c.pollutant, c.concentration, c.unit, c.averaging, c.truncated, c.aqi, c.category,
            c.breakpoints_used[0], c.breakpoints_used[1], c.breakpoints_used[2], c.breakpoints_used[3]
        ));
    }
    if !reporting_areas.is_empty() {
        let rows: Vec<Vec<String>> = reporting_areas
            .iter()
            .map(|ra| {
                let cur = ra
                    .current
                    .iter()
                    .find(|v| v.primary)
                    .or(ra.current.first())
                    .map(|v| {
                        format!(
                            "{} {} ({}){}",
                            v.aqi.map(|a| a.to_string()).unwrap_or_default(),
                            v.category,
                            v.pollutant,
                            v.time
                                .as_deref()
                                .map(|t| format!(", {t}"))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_else(|| "—".into());
                let fc = |d: usize| {
                    let dates: Vec<&String> = {
                        let mut v: Vec<&String> = ra.forecasts.iter().map(|f| &f.date).collect();
                        v.dedup();
                        v
                    };
                    dates
                        .get(d)
                        .and_then(|date| ra.forecasts.iter().find(|f| &f.date == *date))
                        .map(|f| {
                            format!(
                                "{}: {}{}{}",
                                f.date,
                                f.category,
                                f.aqi.map(|a| format!(" ({a})")).unwrap_or_default(),
                                if f.action_day { " — ACTION DAY" } else { "" }
                            )
                        })
                        .unwrap_or_else(|| "—".into())
                };
                vec![
                    format!(
                        "{}{}",
                        ra.reporting_area,
                        ra.distance_km
                            .map(|d| format!(" ({d} km)"))
                            .unwrap_or_default()
                    ),
                    cur,
                    fc(0),
                    fc(1),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["AirNow reporting area", "Now", "Forecast", "Next forecast"],
            &rows,
        ));
        s.push('\n');
    }
    match (&worst, guidance.first()) {
        (Some((_, why)), Some(g)) => {
            s.push_str(&format!(
                "**Guidance for {} (AQI {}, {})** — based on {why}.\n\n{}\n\n",
                g.category, g.aqi_range, g.color, g.health_concern
            ));
            if !g.sensitive_groups.is_empty() && g.category != "Good" {
                s.push_str(&format!(
                    "Sensitive groups — {}\n\n",
                    g.sensitive_groups.join("; ")
                ));
            }
            for (label, items) in [
                ("Sensitive groups", &g.for_sensitive_groups),
                ("Everyone", &g.general_public),
                ("Schools", &g.schools),
                ("Outdoor events", &g.outdoor_events),
            ] {
                if !items.is_empty() {
                    s.push_str(&format!("- **{label}:** {}\n", items.join(" ")));
                }
            }
        }
        _ => {
            s.push_str("**Protective actions by AQI category**\n\n");
            let rows: Vec<Vec<String>> = guidance
                .iter()
                .map(|g| {
                    vec![
                        format!("{} ({})", g.category, g.aqi_range),
                        g.for_sensitive_groups.join(" "),
                        g.schools.join(" "),
                        g.outdoor_events.join(" "),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &[
                    "AQI category",
                    "Sensitive groups",
                    "Schools",
                    "Outdoor events",
                ],
                &rows,
            ));
        }
    }
    let tips: Vec<String> = aq.tips.iter().map(|t| t.text.clone()).collect();
    s.push_str("\n**Tips:**\n");
    for t in &tips {
        s.push_str(&format!("- {t}\n"));
    }

    caveats.push(
        "AirNow data are preliminary and not for regulatory use. Current values are NowCast \
         estimates for the latest hour; forecasts are issued by MassDEP, usually once or twice a \
         day."
            .into(),
    );
    if area.level == AreaLevel::Town {
        caveats.push(
            "Your town is matched to the nearest AirNow reporting area; local conditions (smoke, \
             traffic, terrain) can differ."
                .into(),
        );
    }
    caveats.push(aq.pm25_change.text.clone());
    if guidance.iter().any(|g| g.includes_summaries) {
        caveats.push(
            "Some actions (general public at Unhealthy and above, outdoor events) are this \
             project's summaries of EPA and CDC guidance, not AirNow's own wording; AirNow's \
             activity guides could not be retrieved to check them."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[content::AIR_QUALITY]));
    let mut keys: Vec<&str> = vec![
        "ecfr-58g",
        "epa-pm-course",
        "epa-ozone-course",
        "epa-school-guidance",
        "cdc-heat-clinical",
        "airnow-fact",
    ];
    if guidance
        .iter()
        .any(|g| g.category.contains("Very") || g.category == "Hazardous")
    {
        keys.push("cdc-wildfire-smoke");
    }
    sources.extend(content::sources_for(keys, &aq.sources));

    Ok(Envelope::new(
        s,
        AirQualityResult {
            area,
            airnow_status,
            product,
            reporting_areas,
            computed,
            guidance_category,
            guidance,
            tips,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none())
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aq() -> AirQuality {
        content::load(content::AIR_QUALITY).unwrap()
    }

    #[test]
    fn appendix_g_worked_example_and_2024_pm25_breakpoints() {
        let aq = aq();
        // 40 CFR 58 App. G 3.4(d): PM10 210 µg/m³ → 128; 1-hour ozone 0.156 ppm → 140.
        let pm10 = table_for(&aq, "PM10", "").unwrap();
        assert_eq!(aqi_from_concentration(pm10, 210.0).unwrap().0, 128);
        let o3 = table_for(&aq, "ozone", "1-hour").unwrap();
        assert_eq!(aqi_from_concentration(o3, 0.156).unwrap().0, 140);
        assert!(aqi_from_concentration(o3, 0.100).is_none());
        // 2024 PM2.5: 9.0 is the top of Good, 35.5 starts USG; 12.0 is now Moderate (56).
        let pm = table_for(&aq, "PM2.5", "").unwrap();
        assert_eq!(aqi_from_concentration(pm, 9.0).unwrap().0, 50);
        assert_eq!(aqi_from_concentration(pm, 9.05).unwrap().0, 50); // truncated to 9.0
        assert_eq!(aqi_from_concentration(pm, 12.0).unwrap().0, 56);
        assert_eq!(aqi_from_concentration(pm, 35.5).unwrap().0, 101);
        assert_eq!(aqi_from_concentration(pm, 325.4).unwrap().0, 500);
        // 8-hour ozone 0.070 ppm (the standard) → 100.
        let o8 = table_for(&aq, "O3", "8-hour").unwrap();
        assert_eq!(aqi_from_concentration(o8, 0.070).unwrap().0, 100);
        assert_eq!(aqi_from_concentration(o8, 0.0709).unwrap().0, 100);
        // 8-hour ozone and 1-hour SO2 stop at AQI 300 and 200: no extrapolation above them.
        assert_eq!(aqi_from_concentration(o8, 0.200).unwrap().0, 300);
        assert!(aqi_from_concentration(o8, 0.250).is_none());
        let so2 = table_for(&aq, "SO2", "").unwrap();
        assert_eq!(aqi_from_concentration(so2, 304.0).unwrap().0, 200);
        assert!(aqi_from_concentration(so2, 400.0).is_none());
        // A table that reaches AQI 500 is still extrapolated above its top row.
        assert!(aqi_from_concentration(pm, 400.0).unwrap().0 > 500);
        assert_eq!(aq.category_for(101).name, "Unhealthy for Sensitive Groups");
        assert_eq!(aq.category_for(900).name, "Hazardous");
    }

    #[tokio::test]
    async fn offline_with_an_aqi_gives_that_categorys_actions() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = air_quality_protective_guidance(
            &ctx,
            AirQualityArgs {
                area: Some("Worcester".into()),
                aqi: Some(135),
                pollutant: Some("ozone".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.airnow_status, "unavailable");
        assert_eq!(
            d.guidance_category.as_deref(),
            Some("Unhealthy for Sensitive Groups")
        );
        let g = &d.guidance[0];
        assert!(g.schools[0].contains("recess"));
        assert!(g.sensitive_groups[0].starts_with("OZONE"));
        assert!(env.caveats[0].contains("COULD NOT BE RETRIEVED"));

        // Without an AQI, every category is listed.
        let env = air_quality_protective_guidance(&ctx, AirQualityArgs::default())
            .await
            .unwrap();
        assert_eq!(env.data.guidance.len(), 6);
        assert!(env.summary.contains("Protective actions by AQI category"));

        // A concentration is converted.
        let env = air_quality_protective_guidance(
            &ctx,
            AirQualityArgs {
                pollutant: Some("PM2.5".into()),
                concentration: Some(60.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let c = env.data.computed.unwrap();
        assert_eq!(c.aqi, 154);
        assert_eq!(c.category, "Unhealthy");
        assert!(
            air_quality_protective_guidance(
                &ctx,
                AirQualityArgs {
                    concentration: Some(5.0),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
    }
}
