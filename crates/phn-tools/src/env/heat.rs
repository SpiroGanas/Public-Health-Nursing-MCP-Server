//! ENV-01 `heat_health_action_planner`: NWS forecast and HeatRisk for a town
//! or county, heat alerts, the local heat-vulnerable population, activation
//! steps, and a heat response checklist (DOCX) with cooling centers,
//! outreach scripts, school and outdoor worker guidance, and heat illness
//! teaching.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::boundaries::{BoundaryLayer, Layer};
use phn_data::heatrisk::{self, HeatRiskGrid};
use phn_data::nws::{self, ForecastPeriod};
use phn_data::projection;
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, CitedOut, Heat, HeatIllness, OutreachScript};
use crate::epr::population::{self, PopulationProfile};
use crate::md;

/// Arguments for `heat_health_action_planner`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HeatPlanArgs {
    /// A Massachusetts city or town, or a county. Default: your saved town.
    pub area: Option<String>,
    /// Plan for this HeatRisk level (0-4) instead of the forecast peak (for exercises or
    /// when the forecast cannot be reached).
    pub plan_for_level: Option<u8>,
    /// A CSV or XLSX file in /data listing cooling centers (columns such as name, address, town,
    /// hours, phone, accessible, pets, notes). Its rows go into the checklist only.
    pub cooling_centers_path: Option<String>,
    /// Write the heat response checklist (DOCX) to /data/output (default true).
    pub checklist: Option<bool>,
}

/// HeatRisk for one day over the area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeatRiskDay {
    pub day: u8,
    pub date: Option<String>,
    /// Highest HeatRisk level anywhere in the area (0-4).
    pub max_level: Option<u8>,
    pub level_name: Option<String>,
    pub color: Option<String>,
    /// Share of the area's forecast cells at each level 0-4.
    pub share_by_level: Vec<f64>,
}

/// Summary of a local cooling-center file (its rows are not returned).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoolingCenterSummary {
    pub file: String,
    pub sites: usize,
    pub with_hours: usize,
    pub marked_accessible: usize,
    pub columns_used: Vec<String>,
}

/// Result of `heat_health_action_planner`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeatPlanResult {
    pub area: Area,
    /// "live", "cache", or "unavailable".
    pub heatrisk_status: String,
    pub heatrisk_updated: Option<String>,
    pub heatrisk_days: Vec<HeatRiskDay>,
    pub peak_level: Option<u8>,
    pub peak_date: Option<String>,
    pub forecast_status: String,
    /// Place the forecast is for.
    pub forecast_point: Option<String>,
    pub forecast: Vec<ForecastPeriod>,
    /// Active NWS heat alerts for the county (empty when the feed could not be read; see
    /// `heat_alerts_status`).
    pub heat_alerts: Vec<String>,
    /// Whether the NWS alert feed was read: "live", "cache", or "unavailable". When it is
    /// "unavailable" an empty `heat_alerts` does not mean there are no alerts.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub heat_alerts_status: String,
    pub population: PopulationProfile,
    /// The level the activation steps are for.
    pub activation_level: u8,
    pub activation_steps: Vec<CitedOut>,
    pub who_is_at_risk: Vec<String>,
    pub protective_actions: Vec<String>,
    pub cooling_centers: Option<CoolingCenterSummary>,
    pub outreach_scripts: Vec<OutreachScript>,
    pub schools_and_child_care: Vec<CitedOut>,
    pub outdoor_workers: Vec<CitedOut>,
    pub heat_illness: Vec<HeatIllness>,
    pub medications_and_power: Vec<CitedOut>,
    pub checklist_file: Option<String>,
}

/// Lat/lon cell centres inside the given boundary features.
fn sample(grid: &HeatRiskGrid, features: &[&phn_data::boundaries::Feature]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in features {
        let (x0, y0, x1, y1) = f.bbox();
        let corners = [
            projection::from_mass_mainland(x0, y0),
            projection::from_mass_mainland(x0, y1),
            projection::from_mass_mainland(x1, y0),
            projection::from_mass_mainland(x1, y1),
        ];
        let south = corners.iter().map(|c| c.0).fold(f64::MAX, f64::min);
        let north = corners.iter().map(|c| c.0).fold(f64::MIN, f64::max);
        let west = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min);
        let east = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max);
        let mut found = false;
        for (lat, lon) in grid.cell_centres(south, north, west, east) {
            let (x, y) = projection::to_mass_mainland(lat, lon);
            if f.polygons
                .iter()
                .any(|rings| projection::point_in_rings(x, y, rings))
                && let Some(v) = grid.at(lat, lon)
            {
                out.push(v);
                found = true;
            }
        }
        if !found {
            let (x, y) = f.label_point();
            let (lat, lon) = projection::from_mass_mainland(x, y);
            if let Some(v) = grid.at(lat, lon) {
                out.push(v);
            }
        }
    }
    out
}

/// The highest HeatRisk level in the forecast and the first day it is reached.
fn peak_of(days: &[HeatRiskDay]) -> Option<(u8, Option<String>)> {
    // `max_by_key` keeps the last of equal maxima, so search from the last day back.
    days.iter()
        .rev()
        .filter_map(|d| d.max_level.map(|m| (m, d.date.clone())))
        .max_by_key(|(m, _)| *m)
}

/// The checklist's line about active heat alerts. "None" is said only when the
/// NWS feed was actually read.
fn alerts_line(alerts: &[String], alerts_checked: bool) -> String {
    if !alerts_checked {
        "Active heat alerts: NWS alerts could not be checked — see weather.gov.".to_string()
    } else if alerts.is_empty() {
        "Active heat alerts: none at the time of writing".to_string()
    } else {
        format!("Active heat alerts: {}", alerts.join("; "))
    }
}

/// Build a heat response plan.
pub async fn heat_health_action_planner(
    ctx: &Context,
    args: HeatPlanArgs,
) -> Result<Envelope<HeatPlanResult>> {
    let heat: Heat = content::load(content::HEAT)?;
    let area = area::resolve(ctx, args.area.as_deref(), false)?;
    if area.level == AreaLevel::State {
        return Err(PhnError::InvalidInput(
            "heat plans are local: name a city, town, or county".into(),
        ));
    }
    if let Some(l) = args.plan_for_level
        && l > 4
    {
        return Err(PhnError::InvalidInput("plan_for_level must be 0-4".into()));
    }
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let towns = area::towns_in(ctx, &area);
    let layer = BoundaryLayer::shared(&ctx.snapshots, Layer::Town)?;
    let features: Vec<&phn_data::boundaries::Feature> =
        towns.iter().filter_map(|t| layer.get(t)).collect();

    // Population profile.
    let (population, psrc, pcav) = population::profile(ctx, &area, true).await?;
    sources.extend(psrc);
    caveats.extend(pcav);

    // Forecast point: the town, or the county's most populous town.
    let point_town = match area.level {
        AreaLevel::Town => area.town.clone(),
        _ => {
            let acs = phn_data::census::AcsSnapshot::shared(&ctx.snapshots)?;
            let geos = phn_data::geography::MaGeographies::shared(&ctx.snapshots)?;
            towns
                .iter()
                .filter_map(|t| {
                    let g = geos.town(t)?;
                    let p = acs.row(&g.geoid)?.values.get("pop_total")?.value;
                    Some((t.clone(), p))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(t, _)| t)
        }
    };

    // NWS forecast.
    let mut forecast_status = "unavailable".to_string();
    let mut forecast = Vec::new();
    let mut forecast_point = None;
    if let Some(t) = &point_town
        && let Ok((lat, lon)) = area::town_point(ctx, t)
    {
        match nws::fetch_forecast(&ctx.http, ctx.cache.as_ref(), lat, lon).await {
            Ok((info, fc, fetched)) => {
                forecast_status = fetched.mode.label().to_string();
                forecast = fc.periods;
                forecast_point = Some(format!(
                    "{t}{}",
                    info.nearest_place
                        .map(|p| format!(" (NWS point near {p})"))
                        .unwrap_or_default()
                ));
                sources.push(Source {
                    title: format!("NWS 7-day forecast for {t}"),
                    publisher: format!(
                        "National Weather Service{}",
                        info.office.map(|o| format!(" ({o})")).unwrap_or_default()
                    ),
                    url: Some(info.forecast_url.clone()),
                    effective_date: fc.updated,
                    retrieved: Some(fetched.as_of.chars().take(19).collect()),
                    mode: fetched.mode,
                });
            }
            Err(e) => caveats.push(format!(
                "The NWS forecast could not be retrieved ({}).",
                phn_data::apifetch::reason(&e)
            )),
        }
    }

    // HeatRisk.
    let mut heatrisk_status = "unavailable".to_string();
    let mut heatrisk_updated = None;
    let mut days = Vec::new();
    match heatrisk::fetch(&ctx.http, ctx.cache.as_ref(), 7).await {
        Ok(hr) => {
            heatrisk_status = hr.mode.label().to_string();
            heatrisk_updated = hr.updated.clone();
            if let Some(r) = &hr.fallback_reason {
                caveats.push(format!("NWS HeatRisk: {r}."));
            }
            for g in &hr.grids {
                let cells = sample(g, &features);
                let max = cells.iter().copied().max();
                let n = cells.len().max(1) as f64;
                let share: Vec<f64> = (0..5u8)
                    .map(|l| {
                        ((cells.iter().filter(|c| **c == l).count() as f64 / n) * 100.0).round()
                            / 100.0
                    })
                    .collect();
                let lvl = max.and_then(|m| heat.heatrisk_levels.iter().find(|x| x.level == m));
                days.push(HeatRiskDay {
                    day: g.day,
                    date: g.date.clone(),
                    max_level: max,
                    level_name: lvl.map(|l| l.name.clone()),
                    color: lvl.map(|l| l.color.clone()),
                    share_by_level: share,
                });
            }
            sources.push(Source {
                title: "NWS HeatRisk (experimental), days 1-7".into(),
                publisher: "National Weather Service, Weather Prediction Center (with CDC)".into(),
                url: Some(heatrisk::HOME_URL.into()),
                effective_date: hr.updated.clone(),
                retrieved: Some(hr.as_of.chars().take(19).collect()),
                mode: hr.mode,
            });
            if days.iter().all(|d| d.max_level.is_none()) {
                caveats.push(
                    "HeatRisk had no data over this area (the image may have changed format)."
                        .into(),
                );
            }
        }
        Err(e) => caveats.insert(
            0,
            format!(
                "NWS HeatRisk could not be retrieved ({}); plan from the NWS forecast and alerts, \
                 or give plan_for_level.",
                phn_data::apifetch::reason(&e)
            ),
        ),
    }
    let peak = peak_of(&days);
    let (peak_level, peak_date) = match &peak {
        Some((m, d)) => (Some(*m), d.clone()),
        None => (None, None),
    };

    // Heat alerts: real ("Actual") alerts that have not ended. When the feed
    // cannot be read, say so; never report "none".
    let mut heat_alerts = Vec::new();
    let mut heat_alerts_status = "unavailable".to_string();
    if let Ok(f) = nws::fetch_active_alerts(&ctx.http, ctx.cache.as_ref()).await
        && let Ok(feed) = nws::parse_alerts(&f.body)
    {
        heat_alerts_status = f.mode.label().to_string();
        if f.mode == phn_core::SourceMode::Cache {
            caveats.push(format!(
                "NWS alerts are from a cached copy fetched {}; check weather.gov for newer alerts.",
                f.as_of
                    .chars()
                    .take(16)
                    .collect::<String>()
                    .replace('T', " ")
            ));
        }
        let now = chrono::Utc::now();
        let hz: content::HazardActions = content::load(content::HAZARDS)?;
        for a in feed.alerts {
            let is_heat = hz
                .for_event(&a.event)
                .is_some_and(|h| h.id == "extreme_heat");
            if is_heat
                && area
                    .county_fips
                    .as_ref()
                    .is_some_and(|c| a.county_fips.contains(c))
                && crate::epr::alerts::alert_is_current(&a, now)
            {
                heat_alerts.push(format!(
                    "{}{}",
                    a.event,
                    a.ends
                        .or(a.expires)
                        .map(|e| format!(" until {e}"))
                        .unwrap_or_default()
                ));
            }
        }
    }
    let alerts_checked = heat_alerts_status != "unavailable";
    if !alerts_checked {
        // Right after the HeatRisk warning when that is also unavailable.
        let at = usize::from(heatrisk_status == "unavailable").min(caveats.len());
        caveats.insert(
            at,
            "NWS alerts could not be checked — see weather.gov (NWS Boston/Norton, and NWS \
             Albany for Berkshire County). This plan does not show active heat advisories or \
             warnings."
                .into(),
        );
    }

    // Activation.
    let activation_level = args.plan_for_level.or(peak_level).unwrap_or(0);
    let activation_steps: Vec<CitedOut> = heat
        .activation
        .iter()
        .filter(|a| a.level <= activation_level)
        .flat_map(|a| content::cite_all(&a.actions, &heat.sources))
        .collect();
    let lvl = heat
        .heatrisk_levels
        .iter()
        .find(|l| l.level == activation_level)
        .ok_or_else(|| PhnError::Internal("HeatRisk level missing".into()))?;

    // Cooling centers (local file → checklist only).
    let mut cooling = None;
    let mut cooling_rows: Vec<Vec<String>> = Vec::new();
    let mut identifiers = Vec::new();
    let mut local_source = None;
    if let Some(p) = &args.cooling_centers_path {
        let lt = crate::localdata::load(ctx, p, None)?;
        identifiers = lt.identifier_categories();
        let norm = |s: &str| phn_core::text::normalize(s);
        let mut cols: Vec<(String, Option<usize>)> = Vec::new();
        for c in &heat.cooling_center_columns {
            let idx = lt
                .table
                .headers
                .iter()
                .position(|h| c.aliases.iter().any(|a| norm(a) == norm(h)));
            cols.push((c.field.clone(), idx));
        }
        if cols[0].1.is_none() {
            return Err(PhnError::InvalidInput(format!(
                "'{}' needs a column for the site name (name, site, location, or cooling center). \
                 Columns found: {}",
                lt.label,
                lt.table.headers.join(", ")
            )));
        }
        let mut with_hours = 0;
        let mut accessible = 0;
        for row in &lt.table.rows {
            if row.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            let get = |f: &str| -> String {
                cols.iter()
                    .find(|(n, _)| n == f)
                    .and_then(|(_, i)| *i)
                    .and_then(|i| row.get(i))
                    .map(|v| v.trim().to_string())
                    .unwrap_or_default()
            };
            if !get("hours").is_empty() {
                with_hours += 1;
            }
            if crate::localdata::truthy(&get("accessible")) == Some(true) {
                accessible += 1;
            }
            cooling_rows.push(
                [
                    "name",
                    "address",
                    "town",
                    "hours",
                    "phone",
                    "accessible",
                    "pets",
                    "notes",
                ]
                .iter()
                .map(|f| get(f))
                .collect(),
            );
        }
        cooling = Some(CoolingCenterSummary {
            file: lt.label.clone(),
            sites: cooling_rows.len(),
            with_hours,
            marked_accessible: accessible,
            columns_used: cols
                .iter()
                .filter(|(_, i)| i.is_some())
                .map(|(n, _)| n.clone())
                .collect(),
        });
        local_source = Some(lt.source());
    }

    let schools = content::cite_all(&heat.schools_and_child_care, &heat.sources);
    let workers = content::cite_all(&heat.outdoor_workers, &heat.sources);
    let meds = content::cite_all(&heat.medications_and_power, &heat.sources);
    let groups = content::cite_all(&heat.heat_sensitive_groups, &heat.sources);

    // Checklist document.
    let mut files_written = Vec::new();
    let mut checklist_file = None;
    if args.checklist.unwrap_or(true) {
        let doc = checklist(
            ctx,
            &heat,
            &area,
            &days,
            &forecast,
            &heat_alerts,
            alerts_checked,
            &population,
            activation_level,
            &activation_steps,
            &groups,
            &cooling_rows,
            &schools,
            &workers,
            &meds,
        );
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let slug: String = area
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let path = sandbox.new_output_file(&format!("heat_response_checklist_{slug}.docx"))?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        files_written.push(disp.clone());
        checklist_file = Some(disp);
    }

    // Summary.
    let mut s = format!("**Heat response plan — {}**\n\n", area.name);
    if !days.is_empty() {
        let rows: Vec<Vec<String>> = days
            .iter()
            .map(|d| {
                vec![
                    d.date.clone().unwrap_or_else(|| format!("day {}", d.day)),
                    d.max_level
                        .map(|m| {
                            format!(
                                "{m} — {} ({})",
                                d.level_name.clone().unwrap_or_default(),
                                d.color.clone().unwrap_or_default()
                            )
                        })
                        .unwrap_or_else(|| "no data".into()),
                ]
            })
            .collect();
        s.push_str(&md::table(&["Day", "Highest HeatRisk in the area"], &rows));
        s.push('\n');
    } else {
        s.push_str("HeatRisk is not available (see caveats).\n\n");
    }
    if !forecast.is_empty() {
        let highs: Vec<String> = forecast
            .iter()
            .filter(|p| p.is_daytime)
            .take(7)
            .map(|p| {
                format!(
                    "{} {}°F",
                    p.name,
                    p.temperature_f
                        .map(|t| t.round().to_string())
                        .unwrap_or_default()
                )
            })
            .collect();
        s.push_str(&format!(
            "NWS forecast highs ({}): {}.\n\n",
            forecast_point.clone().unwrap_or_default(),
            highs.join(", ")
        ));
    }
    if !alerts_checked {
        s.push_str("**NWS alerts could not be checked — see weather.gov.**\n\n");
    } else if !heat_alerts.is_empty() {
        s.push_str(&format!(
            "**Active heat alerts:** {}\n\n",
            heat_alerts.join("; ")
        ));
    }
    s.push_str(&format!(
        "**Activation level: HeatRisk {activation_level} — {} ({})**{}\n\n",
        lvl.name,
        lvl.color,
        match (args.plan_for_level, &peak_date) {
            (Some(_), _) => " (the level you asked to plan for)".to_string(),
            (None, Some(d)) => format!(" (forecast peak, {d})"),
            _ => " (no forecast peak available)".to_string(),
        }
    ));
    s.push_str(&format!(
        "Who is at risk: {}\n\n",
        lvl.who_at_risk.join(" ")
    ));
    s.push_str(&format!(
        "Protective actions: {}\n\n",
        lvl.actions.join(" ")
    ));
    if !activation_steps.is_empty() {
        s.push_str("**Public health steps**\n");
        for a in &activation_steps {
            s.push_str(&content::bullet(a));
        }
        s.push('\n');
    }
    s.push_str(&format!(
        "**Heat-vulnerable population in {}**\n\n",
        area.name
    ));
    s.push_str(&population::markdown(&population));
    if let Some(c) = &cooling {
        s.push_str(&format!(
            "\nCooling centers: {} site(s) read from `{}` ({} with hours, {} marked accessible); \
             listed in the checklist only.\n",
            c.sites, c.file, c.with_hours, c.marked_accessible
        ));
    } else {
        s.push_str("\nNo cooling-center file given: the checklist has a blank table to fill in; residents can call 2-1-1.\n");
    }
    if let Some(f) = &checklist_file {
        s.push_str(&format!(
            "\nHeat response checklist (activation steps, cooling centers, outreach scripts, \
             schools and child care, outdoor workers, heat illness teaching, medications and power \
             outages) written to `{f}`.\n"
        ));
    }

    caveats.push(
        "HeatRisk is experimental and supplements, but does not replace, NWS heat watches, \
         warnings, and advisories."
            .into(),
    );
    caveats.push(
        "Activation steps and outreach scripts are this project's drafts built from NWS, CDC, \
         and MEMA guidance; adapt them to your agency's heat plan."
            .into(),
    );
    if area.level == AreaLevel::County {
        caveats.push(format!(
            "The forecast is for {} (the county's most populous town); HeatRisk covers every \
             town in the county.",
            point_town.unwrap_or_default()
        ));
    }
    caveats.extend(content::review_caveats(&[content::HEAT]));
    sources.extend(content::sources_for(
        heat.sources.keys().map(String::as_str),
        &heat.sources,
    ));
    if let Some(ls) = local_source {
        sources.push(ls);
    }

    Ok(Envelope::new(
        s,
        HeatPlanResult {
            area,
            heatrisk_status,
            heatrisk_updated,
            heatrisk_days: days,
            peak_level,
            peak_date,
            forecast_status,
            forecast_point,
            forecast,
            heat_alerts,
            heat_alerts_status,
            population,
            activation_level,
            activation_steps,
            who_is_at_risk: lvl.who_at_risk.clone(),
            protective_actions: lvl.actions.clone(),
            cooling_centers: cooling,
            outreach_scripts: heat.outreach_scripts.clone(),
            schools_and_child_care: schools,
            outdoor_workers: workers,
            heat_illness: heat.heat_illness.clone(),
            medications_and_power: meds,
            checklist_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        identifiers_detected: identifiers,
        files_written,
        notes: if args.cooling_centers_path.is_some() {
            vec!["Cooling-center rows from your file were written to the checklist only.".into()]
        } else {
            Vec::new()
        },
        ..Privacy::none()
    })
    .decision_support())
}

#[allow(clippy::too_many_arguments)]
fn checklist(
    ctx: &Context,
    heat: &Heat,
    area: &Area,
    days: &[HeatRiskDay],
    forecast: &[ForecastPeriod],
    alerts: &[String],
    alerts_checked: bool,
    population: &PopulationProfile,
    level: u8,
    steps: &[CitedOut],
    groups: &[CitedOut],
    cooling: &[Vec<String>],
    schools: &[CitedOut],
    workers: &[CitedOut],
    meds: &[CitedOut],
) -> Document {
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
        "Prepared {} for {}. Decision support only: adapt to your agency's heat plan and DPH guidance.",
        chrono::Local::now().date_naive(),
        area.name
    ))];
    blocks.push(h("1. Situation"));
    if days.is_empty() {
        blocks.push(p("HeatRisk was not available when this checklist was made. Check https://www.wpc.ncep.noaa.gov/heatrisk/ and weather.gov."));
    } else {
        blocks.push(Block::Table(TableBlock {
            caption: Some("NWS HeatRisk, highest level in the area".into()),
            headers: vec!["Day".into(), "HeatRisk".into()],
            rows: days
                .iter()
                .map(|d| {
                    vec![
                        d.date.clone().unwrap_or_else(|| format!("day {}", d.day)),
                        d.max_level
                            .map(|m| format!("{m} {}", d.level_name.clone().unwrap_or_default()))
                            .unwrap_or_else(|| "no data".into()),
                    ]
                })
                .collect(),
        }));
    }
    if !forecast.is_empty() {
        blocks.push(Block::Table(TableBlock {
            caption: Some("NWS forecast".into()),
            headers: vec![
                "Period".into(),
                "Temperature (°F)".into(),
                "Forecast".into(),
            ],
            rows: forecast
                .iter()
                .map(|f| {
                    vec![
                        f.name.clone(),
                        f.temperature_f
                            .map(|t| t.round().to_string())
                            .unwrap_or_default(),
                        f.short_forecast.clone(),
                    ]
                })
                .collect(),
        }));
    }
    blocks.push(p(&alerts_line(alerts, alerts_checked)));
    blocks.push(h(&format!(
        "2. Activation steps (HeatRisk {level} and below)"
    )));
    let mut rows: Vec<Vec<String>> = steps
        .iter()
        .map(|s| vec![s.text.clone(), String::new(), String::new()])
        .collect();
    if rows.is_empty() {
        rows.push(vec![
            "Monitor the HeatRisk forecast daily.".into(),
            String::new(),
            String::new(),
        ]);
    }
    blocks.push(Block::Table(TableBlock {
        caption: Some("Activation checklist".into()),
        headers: vec!["Step".into(), "Lead".into(), "Done (time, initials)".into()],
        rows,
    }));
    blocks.push(h("3. Who is at risk here"));
    blocks.push(list(groups.iter().map(|g| g.text.clone()).collect()));
    blocks.push(Block::Table(TableBlock {
        caption: Some(format!("Heat-vulnerable population, {}", area.name)),
        headers: vec![
            "Indicator".into(),
            "Percent".into(),
            "About how many".into(),
            "Source".into(),
        ],
        rows: population
            .demographics
            .iter()
            .chain(population.health.iter())
            .map(|i| {
                vec![
                    i.label.clone(),
                    i.percent.map(|v| format!("{v}%")).unwrap_or_default(),
                    i.count_text(),
                    i.source.clone(),
                ]
            })
            .collect(),
    }));
    if let Some(e) = &population.electricity_dependent {
        blocks.push(p(&format!(
            "HHS emPOWER: about {} Medicare beneficiaries rely on electricity-dependent medical equipment ({}).",
            md::num(e.electricity_dependent, 0),
            e.method
        )));
    }
    blocks.push(h("4. Cooling centers"));
    blocks.push(Block::Table(TableBlock {
        caption: Some("Cooling centers".into()),
        headers: [
            "Site",
            "Address",
            "Town",
            "Hours",
            "Phone",
            "Accessible",
            "Pets",
            "Notes",
        ]
        .map(String::from)
        .to_vec(),
        rows: if cooling.is_empty() {
            (0..5).map(|_| vec![String::new(); 8]).collect()
        } else {
            cooling.to_vec()
        },
    }));
    blocks.push(p(
        "Residents can also dial 2-1-1 for cooling centers and shelters (MEMA).",
    ));
    blocks.push(h("5. Outreach scripts"));
    for sc in &heat.outreach_scripts {
        blocks.push(Block::Heading {
            level: 3,
            text: sc.title.clone(),
        });
        blocks.push(Block::List {
            ordered: true,
            items: sc
                .lines
                .iter()
                .map(|l| vec![Inline::plain(l.clone())])
                .collect(),
        });
    }
    blocks.push(h("6. Schools, child care, camps, and athletics"));
    blocks.push(list(schools.iter().map(|c| c.text.clone()).collect()));
    blocks.push(h("7. Outdoor workers"));
    blocks.push(list(workers.iter().map(|c| c.text.clone()).collect()));
    blocks.push(h("8. Heat illness: recognize and respond"));
    blocks.push(Block::Table(TableBlock {
        caption: Some("Heat illness (MEMA)".into()),
        headers: vec!["Illness".into(), "Signs".into(), "What to do".into()],
        rows: heat
            .heat_illness
            .iter()
            .map(|i| vec![i.name.clone(), i.signs.clone(), i.response.clone()])
            .collect(),
    }));
    blocks.push(h("9. Medications, devices, and power outages"));
    blocks.push(list(meds.iter().map(|c| c.text.clone()).collect()));
    blocks.push(h("10. Sources"));
    blocks.push(list(
        heat.sources
            .values()
            .map(|s| format!("{} — {} ({})", s.title, s.publisher, s.url))
            .collect(),
    ));
    Document {
        title: format!("Heat response checklist — {}", area.name),
        subtitle: Some(format!("HeatRisk level {level}")),
        language: "en-US".into(),
        author: None,
        letterhead: ctx.settings().letterhead,
        logo: None,
        blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checklist_says_none_only_when_the_alert_feed_was_read() {
        let unchecked = alerts_line(&[], false);
        assert!(unchecked.contains("could not be checked — see weather.gov"));
        assert!(!unchecked.contains("none"));
        assert!(alerts_line(&[], true).contains("none at the time of writing"));
        assert_eq!(
            alerts_line(&["Heat Advisory until 8 PM".to_string()], true),
            "Active heat alerts: Heat Advisory until 8 PM"
        );
    }

    #[test]
    fn peak_is_the_first_day_at_the_highest_level() {
        let day = |n: u8, level: Option<u8>| HeatRiskDay {
            day: n,
            date: Some(format!("2026-07-1{n}")),
            max_level: level,
            level_name: None,
            color: None,
            share_by_level: Vec::new(),
        };
        let days = vec![
            day(1, Some(1)),
            day(2, Some(3)),
            day(3, Some(2)),
            day(4, None),
            day(5, Some(3)),
        ];
        assert_eq!(peak_of(&days), Some((3, Some("2026-07-12".to_string()))));
        assert_eq!(peak_of(&[day(1, None)]), None);
    }

    #[tokio::test]
    async fn offline_plan_writes_a_checklist_and_reads_cooling_centers() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("cooling.csv"),
            "Site,Address,Hours,Accessible\nSenior Center,1 Main St,9-5,yes\nLibrary,2 Elm St,,no\n",
        )
        .unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = heat_health_action_planner(
            &ctx,
            HeatPlanArgs {
                area: Some("Springfield".into()),
                plan_for_level: Some(3),
                cooling_centers_path: Some("cooling.csv".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.heatrisk_status, "unavailable");
        assert_eq!(d.activation_level, 3);
        // Steps for levels 1-3.
        assert!(d.activation_steps.len() >= 7);
        assert!(
            d.activation_steps
                .iter()
                .any(|s| s.text.contains("cooling centers"))
        );
        let c = d.cooling_centers.as_ref().unwrap();
        assert_eq!((c.sites, c.with_hours, c.marked_accessible), (2, 1, 1));
        let f = d.checklist_file.as_ref().unwrap();
        assert!(f.ends_with(".docx"));
        assert!(
            data.join("output")
                .join(f.rsplit('/').next().unwrap())
                .exists()
        );
        assert!(!env.summary.contains("1 Main St"));
        assert!(d.population.health.iter().any(|h| h.id == "COPD"));
        assert!(env.caveats[0].contains("HeatRisk could not be retrieved"));
        // Offline, the alert feed cannot be read: say so, never "none".
        assert_eq!(d.heat_alerts_status, "unavailable");
        assert!(d.heat_alerts.is_empty());
        assert!(
            env.summary
                .contains("NWS alerts could not be checked — see weather.gov")
        );
        assert!(env.caveats[1].contains("NWS alerts could not be checked"));
        assert!(
            heat_health_action_planner(
                &ctx,
                HeatPlanArgs {
                    area: Some("Massachusetts".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
    }
}
