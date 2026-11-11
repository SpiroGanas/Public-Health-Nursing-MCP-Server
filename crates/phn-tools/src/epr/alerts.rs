//! EPR-05 `get_active_hazard_alerts`: current National Weather Service alerts
//! for Massachusetts and FEMA disaster declarations, with likely health
//! effects and suggested public health actions for each hazard.

use std::collections::{BTreeMap, BTreeSet};

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::nws::{self, NwsAlert};
use phn_data::openfema::{self, Declaration};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::area::{self, Area, AreaLevel};
use super::content::{self, CitedOut, HazardActions};
use crate::context::Context;
use crate::md;

/// Arguments for `get_active_hazard_alerts`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HazardAlertsArgs {
    /// A Massachusetts city or town, a county ("Bristol County"), or "Massachusetts".
    /// Default: your saved town; otherwise the whole state.
    pub area: Option<String>,
    /// Also give guidance for these hazards even with no active alert, e.g.
    /// ["extreme_heat", "winter storm", "Hurricane Warning"] (useful when planning ahead).
    pub guidance_for: Vec<String>,
    /// Include alerts for coastal waters only (marine zones). Default false.
    pub include_marine: Option<bool>,
    /// Include FEMA disaster declarations. Default true.
    pub include_declarations: Option<bool>,
    /// How many years of declarations to list (default 10, 1-75).
    pub declaration_years: Option<u32>,
}

/// One active alert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AlertOut {
    pub event: String,
    /// The hazard guidance that applies (see `hazards`), when the event is mapped.
    pub hazard: Option<String>,
    pub severity: String,
    pub urgency: String,
    pub certainty: String,
    /// Alert or Update.
    pub message_type: String,
    pub headline: Option<String>,
    pub area_description: String,
    /// Massachusetts counties covered.
    pub counties: Vec<String>,
    pub onset: Option<String>,
    pub ends: Option<String>,
    pub expires: Option<String>,
    pub issued_by: Option<String>,
    /// NWS protective-action instructions.
    pub instruction: Option<String>,
    /// First part of the NWS description.
    pub description: Option<String>,
    pub url: String,
}

/// Guidance for one hazard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HazardGuidance {
    pub id: String,
    pub label: String,
    /// "active alert", "requested", or "follows <hazard>".
    pub why: String,
    pub health_effects: Vec<CitedOut>,
    pub at_risk: Vec<CitedOut>,
    pub public_health_actions: Vec<CitedOut>,
    pub public_messages: Vec<String>,
    /// Other tools that help with this hazard.
    pub related_tools: Vec<String>,
}

/// One disaster declaration (all designated areas combined).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeclarationOut {
    /// "DR-4780-MA".
    pub id: String,
    /// DR = major disaster, EM = emergency, FM = fire management.
    pub declaration_type: String,
    pub declaration_date: String,
    pub incident_type: String,
    pub title: String,
    pub incident_begin: Option<String>,
    pub incident_end: Option<String>,
    /// Designated counties (and tribal or statewide areas).
    pub designated_areas: Vec<String>,
    pub programs: Vec<String>,
}

/// Declarations by incident type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IncidentCount {
    pub incident_type: String,
    /// Declarations covering the area since 1953.
    pub since_1953: usize,
    /// Declarations in the listing window.
    pub in_window: usize,
    pub most_recent: String,
}

/// A useful link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Link {
    pub label: String,
    pub url: String,
}

/// Result of `get_active_hazard_alerts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HazardAlertsResult {
    pub area: Area,
    /// "live", "cache", or "unavailable".
    pub alerts_status: String,
    /// When NWS last updated the feed.
    pub alerts_updated: Option<String>,
    pub alerts: Vec<AlertOut>,
    /// Active Massachusetts alerts outside the area (not listed).
    pub alerts_elsewhere_in_massachusetts: usize,
    /// Marine-only alerts left out (set include_marine to list them).
    pub marine_alerts_omitted: usize,
    pub hazards: Vec<HazardGuidance>,
    pub general_actions: Vec<CitedOut>,
    pub declarations: Vec<DeclarationOut>,
    pub declaration_counts: Vec<IncidentCount>,
    pub declaration_window_years: u32,
    pub links: Vec<Link>,
}

const DESCRIPTION_CHARS: usize = 700;

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{}…", t.trim_end())
    }
}

/// True when an alert is a real ("Actual"), not cancelled, alert that has not
/// ended: its `ends` time (or `expires` when there is no `ends`) is not before
/// `now`. An alert with no readable end time is kept.
pub(crate) fn alert_is_current(a: &NwsAlert, now: chrono::DateTime<chrono::Utc>) -> bool {
    if a.message_type.eq_ignore_ascii_case("cancel") || !a.status.eq_ignore_ascii_case("actual") {
        return false;
    }
    let end = a
        .ends
        .as_deref()
        .or(a.expires.as_deref())
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t.trim()).ok());
    end.is_none_or(|e| e.with_timezone(&chrono::Utc) >= now)
}

/// Group declaration rows for the area into one entry per declaration,
/// newest first. County areas include statewide designations.
pub fn declarations_for(rows: &[Declaration], area: &Area) -> Vec<DeclarationOut> {
    let mut by: BTreeMap<u32, DeclarationOut> = BTreeMap::new();
    let covers = |d: &Declaration| match area.level {
        AreaLevel::State => true,
        _ => {
            d.county_fips.as_deref() == area.county_fips.as_deref()
                || d.designated_area.eq_ignore_ascii_case("statewide")
        }
    };
    let covering: BTreeSet<u32> = rows
        .iter()
        .filter(|d| covers(d))
        .map(|d| d.disaster_number)
        .collect();
    for d in rows
        .iter()
        .filter(|d| covering.contains(&d.disaster_number))
    {
        let e = by
            .entry(d.disaster_number)
            .or_insert_with(|| DeclarationOut {
                id: d.id.clone(),
                declaration_type: d.declaration_type.clone(),
                declaration_date: d.declaration_date.clone(),
                incident_type: d.incident_type.clone(),
                title: d.title.clone(),
                incident_begin: d.incident_begin.clone(),
                incident_end: d.incident_end.clone(),
                designated_areas: Vec::new(),
                programs: Vec::new(),
            });
        let name = d
            .designated_area
            .split(" (County)")
            .next()
            .unwrap_or(&d.designated_area)
            .trim()
            .to_string();
        if !e.designated_areas.contains(&name) {
            e.designated_areas.push(name);
        }
        for p in &d.programs {
            if !e.programs.contains(p) {
                e.programs.push(p.clone());
            }
        }
    }
    let mut out: Vec<DeclarationOut> = by.into_values().collect();
    for d in &mut out {
        d.designated_areas.sort();
    }
    out.sort_by(|a, b| {
        b.declaration_date
            .cmp(&a.declaration_date)
            .then(b.id.cmp(&a.id))
    });
    out
}

/// Counts of declarations by incident type (all years and since `since`).
pub fn incident_counts(decls: &[DeclarationOut], since: &str) -> Vec<IncidentCount> {
    let mut m: BTreeMap<String, IncidentCount> = BTreeMap::new();
    for d in decls {
        let e = m
            .entry(d.incident_type.clone())
            .or_insert_with(|| IncidentCount {
                incident_type: d.incident_type.clone(),
                since_1953: 0,
                in_window: 0,
                most_recent: String::new(),
            });
        e.since_1953 += 1;
        if d.declaration_date.as_str() >= since {
            e.in_window += 1;
        }
        if d.declaration_date > e.most_recent {
            e.most_recent = d.declaration_date.clone();
        }
    }
    let mut v: Vec<IncidentCount> = m.into_values().collect();
    v.sort_by(|a, b| {
        b.since_1953
            .cmp(&a.since_1953)
            .then(a.incident_type.cmp(&b.incident_type))
    });
    v
}

fn guidance(h: &HazardActions, id: &str, why: String) -> Option<HazardGuidance> {
    if let Some(z) = h.hazards.iter().find(|z| z.id == id) {
        return Some(HazardGuidance {
            id: z.id.clone(),
            label: z.label.clone(),
            why,
            health_effects: content::cite_all(&z.health_effects, &h.sources),
            at_risk: content::cite_all(&z.at_risk, &h.sources),
            public_health_actions: content::cite_all(&z.public_health_actions, &h.sources),
            public_messages: z.public_messages.clone(),
            related_tools: z.related_tools.clone(),
        });
    }
    let z = h.secondary_hazards.iter().find(|z| z.id == id)?;
    Some(HazardGuidance {
        id: z.id.clone(),
        label: z.label.clone(),
        why,
        health_effects: content::cite_all(&z.health_effects, &h.sources),
        at_risk: Vec::new(),
        public_health_actions: content::cite_all(&z.public_health_actions, &h.sources),
        public_messages: z.public_messages.clone(),
        related_tools: if z.id == "power_outage" {
            vec!["get_electricity_dependent_population".into()]
        } else {
            Vec::new()
        },
    })
}

fn source_keys(h: &HazardActions, ids: &[String]) -> Vec<String> {
    let mut keys = Vec::new();
    for id in ids {
        if let Some(z) = h.hazards.iter().find(|z| &z.id == id) {
            for c in z
                .health_effects
                .iter()
                .chain(&z.at_risk)
                .chain(&z.public_health_actions)
            {
                keys.push(c.source.clone());
            }
            keys.extend(z.message_sources.iter().cloned());
        } else if let Some(z) = h.secondary_hazards.iter().find(|z| &z.id == id) {
            for c in z.health_effects.iter().chain(&z.public_health_actions) {
                keys.push(c.source.clone());
            }
            keys.extend(z.message_sources.iter().cloned());
        }
    }
    keys
}

/// Current NWS alerts and FEMA declarations for an area, with health guidance.
pub async fn get_active_hazard_alerts(
    ctx: &Context,
    args: HazardAlertsArgs,
) -> Result<Envelope<HazardAlertsResult>> {
    let area = area::resolve(ctx, args.area.as_deref(), true)?;
    let h: HazardActions = content::load(content::HAZARDS)?;
    let years = args.declaration_years.unwrap_or(10);
    if !(1..=75).contains(&years) {
        return Err(PhnError::InvalidInput(
            "declaration_years must be between 1 and 75".into(),
        ));
    }
    let mut requested = Vec::new();
    for g in &args.guidance_for {
        let z = h.find(g).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "no hazard guidance matches '{g}'. Hazards: {}",
                h.hazards
                    .iter()
                    .map(|z| z.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        if !requested.contains(&z.id) {
            requested.push(z.id.clone());
        }
    }
    let include_marine = args.include_marine.unwrap_or(false);
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();

    // ---- NWS alerts
    let mut alerts_status = "unavailable".to_string();
    let mut alerts_updated = None;
    let mut alerts: Vec<AlertOut> = Vec::new();
    let mut elsewhere = 0usize;
    let mut marine_omitted = 0usize;
    match nws::fetch_active_alerts(&ctx.http, ctx.cache.as_ref()).await {
        Ok(f) => {
            let feed = nws::parse_alerts(&f.body)?;
            alerts_status = f.mode.label().to_string();
            alerts_updated = feed.updated.clone();
            if f.mode == SourceMode::Cache {
                caveats.push(format!(
                    "NWS alerts are from a cached copy fetched {}{}; check weather.gov for \
                     newer alerts.",
                    f.as_of
                        .chars()
                        .take(16)
                        .collect::<String>()
                        .replace('T', " "),
                    f.fallback_reason
                        .as_deref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                ));
            }
            sources.push(Source {
                title: "NWS active alerts for Massachusetts (api.weather.gov)".into(),
                publisher: "National Weather Service".into(),
                url: Some(nws::ALERTS_ACTIVE_MA.into()),
                effective_date: feed.updated.clone(),
                retrieved: Some(f.as_of.chars().take(19).collect()),
                mode: f.mode,
            });
            let now = chrono::Utc::now();
            let mut ended = 0usize;
            for a in feed.alerts {
                if a.message_type.eq_ignore_ascii_case("cancel")
                    || !a.status.eq_ignore_ascii_case("actual")
                {
                    continue;
                }
                if !alert_is_current(&a, now) {
                    ended += 1;
                    continue;
                }
                if !include_marine && a.is_marine_only() {
                    marine_omitted += 1;
                    continue;
                }
                let in_area = match area.level {
                    AreaLevel::State => true,
                    _ => area
                        .county_fips
                        .as_ref()
                        .is_some_and(|c| a.county_fips.contains(c)),
                };
                if !in_area {
                    elsewhere += 1;
                    continue;
                }
                alerts.push(alert_out(ctx, &h, &a));
            }
            if ended > 0 {
                caveats.push(format!(
                    "{ended} alert(s) in the NWS feed had already ended and are not shown."
                ));
            }
        }
        Err(e) => {
            caveats.insert(
                0,
                format!(
                    "CURRENT NWS ALERTS COULD NOT BE RETRIEVED ({}). NWS alerts could not be \
                     checked — see weather.gov (NWS Boston/Norton, and NWS Albany for Berkshire \
                     County) or NOAA Weather Radio. This answer does not show active watches or \
                     warnings.",
                    phn_data::apifetch::reason(&e)
                ),
            );
        }
    }
    // Most urgent first.
    let rank = |s: &str| match s {
        "Extreme" => 0,
        "Severe" => 1,
        "Moderate" => 2,
        "Minor" => 3,
        _ => 4,
    };
    alerts.sort_by(|a, b| {
        rank(&a.severity)
            .cmp(&rank(&b.severity))
            .then(a.event.cmp(&b.event))
    });

    // ---- hazard guidance
    let mut hazard_ids: Vec<(String, String)> = Vec::new();
    for a in &alerts {
        if let Some(id) = &a.hazard
            && !hazard_ids.iter().any(|(x, _)| x == id)
        {
            hazard_ids.push((id.clone(), "active alert".into()));
        }
    }
    for id in &requested {
        if !hazard_ids.iter().any(|(x, _)| x == id) {
            hazard_ids.push((id.clone(), "requested".into()));
        }
    }
    let primary: Vec<(String, String)> = hazard_ids.clone();
    for (id, _) in &primary {
        if let Some(z) = h.hazards.iter().find(|z| &z.id == id) {
            for s in &z.secondary {
                if !hazard_ids.iter().any(|(x, _)| x == s) {
                    hazard_ids.push((s.clone(), format!("follows {}", z.label.to_lowercase())));
                }
            }
        }
    }
    let hazards: Vec<HazardGuidance> = hazard_ids
        .iter()
        .filter_map(|(id, why)| guidance(&h, id, why.clone()))
        .collect();
    let unmapped: BTreeSet<&str> = alerts
        .iter()
        .filter(|a| a.hazard.is_none())
        .map(|a| a.event.as_str())
        .collect();
    if !unmapped.is_empty() {
        caveats.push(format!(
            "No health guidance is bundled for: {}. Follow the NWS instructions in the alert.",
            unmapped.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    // ---- FEMA declarations
    let mut declarations = Vec::new();
    let mut counts = Vec::new();
    if args.include_declarations.unwrap_or(true) {
        match openfema::fetch_massachusetts(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await {
            Ok(f) => {
                let all = declarations_for(&f.declarations, &area);
                let today = chrono::Local::now().date_naive();
                let since = format!(
                    "{}{}",
                    chrono::Datelike::year(&today) - years as i32,
                    today.format("-%m-%d")
                );
                counts = incident_counts(&all, &since);
                declarations = all
                    .into_iter()
                    .filter(|d| d.declaration_date >= since)
                    .collect();
                for d in &mut declarations {
                    d.incident_type = h.incident_label(&d.incident_type);
                }
                for c in &mut counts {
                    c.incident_type = h.incident_label(&c.incident_type);
                }
                let mut s = match f.mode {
                    SourceMode::Snapshot => ctx
                        .snapshots
                        .entry(openfema::SNAPSHOT_ID)
                        .map(|(e, _)| e.to_source())
                        .unwrap_or_else(|| {
                            Source::computed("FEMA declarations", "FEMA")
                                .with_url(openfema::DATASET_PAGE)
                        }),
                    _ => Source {
                        title: "Disaster Declarations Summaries v2 (OpenFEMA), Massachusetts"
                            .into(),
                        publisher: "Federal Emergency Management Agency".into(),
                        url: Some(openfema::DATASET_PAGE.into()),
                        effective_date: f.refreshed.clone(),
                        retrieved: Some(f.as_of.chars().take(10).collect()),
                        mode: f.mode,
                    },
                };
                if f.mode == SourceMode::Snapshot {
                    s.mode = SourceMode::Snapshot;
                    caveats.push(format!(
                        "FEMA declarations are from the bundled snapshot (retrieved {}); \
                         declarations made since then are not shown.",
                        f.as_of
                    ));
                }
                sources.push(s);
            }
            Err(e) => caveats.push(format!(
                "FEMA declarations could not be retrieved ({}).",
                phn_data::apifetch::reason(&e)
            )),
        }
    }

    // ---- sources for the guidance
    let mut keys = source_keys(
        &h,
        &hazard_ids
            .iter()
            .map(|(i, _)| i.clone())
            .collect::<Vec<_>>(),
    );
    keys.extend(h.general_actions.iter().map(|c| c.source.clone()));
    sources.extend(content::sources_for(
        keys.iter().map(String::as_str),
        &h.sources,
    ));
    let general_actions = content::cite_all(&h.general_actions, &h.sources);

    let links = vec![
        Link {
            label: "NWS Boston/Norton (eastern and central Massachusetts)".into(),
            url: "https://www.weather.gov/box/".into(),
        },
        Link {
            label: "NWS Albany (Berkshire County)".into(),
            url: "https://www.weather.gov/aly/".into(),
        },
        Link {
            label: "MEMA power outage map".into(),
            url: "http://mema.mapsonline.net/public.html".into(),
        },
        Link {
            label: "FEMA declared disasters".into(),
            url: "https://www.fema.gov/disaster/declarations".into(),
        },
    ];

    // ---- summary
    let mut s = format!("**Hazard alerts — {}**\n\n", area.name);
    match alerts_status.as_str() {
        "unavailable" => s.push_str(
            "⚠️ NWS alerts could not be checked — see weather.gov. Active watches and warnings \
             are NOT shown.\n\n",
        ),
        _ => {
            if alerts.is_empty() {
                s.push_str(&format!(
                    "No active NWS watches, warnings, or advisories for {}{}.\n\n",
                    area.name,
                    alerts_updated
                        .as_deref()
                        .map(|u| format!(" (NWS feed updated {u})"))
                        .unwrap_or_default()
                ));
            } else {
                s.push_str(&format!(
                    "{} active NWS alert(s){}:\n\n",
                    alerts.len(),
                    alerts_updated
                        .as_deref()
                        .map(|u| format!(" (NWS feed updated {u})"))
                        .unwrap_or_default()
                ));
                let rows: Vec<Vec<String>> = alerts
                    .iter()
                    .map(|a| {
                        vec![
                            a.event.clone(),
                            format!("{} / {} / {}", a.severity, a.urgency, a.certainty),
                            if a.counties.is_empty() {
                                truncate(&a.area_description, 80)
                            } else {
                                a.counties.join(", ")
                            },
                            a.onset.clone().unwrap_or_default(),
                            a.ends
                                .clone()
                                .or_else(|| a.expires.clone())
                                .unwrap_or_default(),
                        ]
                    })
                    .collect();
                s.push_str(&md::table(
                    &[
                        "Alert",
                        "Severity / urgency / certainty",
                        "Counties",
                        "Onset",
                        "Ends",
                    ],
                    &rows,
                ));
                for a in &alerts {
                    if let Some(i) = &a.instruction {
                        s.push_str(&format!("\n_{}_: {}\n", a.event, truncate(i, 400)));
                    }
                }
                s.push('\n');
            }
            if elsewhere > 0 && area.level != AreaLevel::State {
                s.push_str(&format!(
                    "{elsewhere} other alert(s) are active elsewhere in Massachusetts.\n\n"
                ));
            }
        }
    }
    for g in &hazards {
        s.push_str(&format!("**{}** ({})\n\n", g.label, g.why));
        if !g.health_effects.is_empty() {
            s.push_str("Likely health effects:\n");
            for c in &g.health_effects {
                s.push_str(&content::bullet(c));
            }
        }
        if !g.at_risk.is_empty() {
            s.push_str("Who is most at risk:\n");
            for c in &g.at_risk {
                s.push_str(&content::bullet(c));
            }
        }
        s.push_str("Suggested public health actions:\n");
        for c in &g.public_health_actions {
            s.push_str(&content::bullet(c));
        }
        if !g.public_messages.is_empty() {
            s.push_str(&format!(
                "Public messages: {}\n",
                g.public_messages.join(" • ")
            ));
        }
        if !g.related_tools.is_empty() {
            s.push_str(&format!("Related tools: {}\n", g.related_tools.join(", ")));
        }
        s.push('\n');
    }
    if !declarations.is_empty() || !counts.is_empty() {
        s.push_str(&format!(
            "**FEMA disaster declarations covering {} — last {years} years**\n\n",
            area.name
        ));
        if declarations.is_empty() {
            s.push_str("None in this period.\n\n");
        } else {
            let rows: Vec<Vec<String>> = declarations
                .iter()
                .take(25)
                .map(|d| {
                    vec![
                        d.id.clone(),
                        d.declaration_date.clone(),
                        d.incident_type.clone(),
                        d.title.clone(),
                        d.programs.join(", "),
                    ]
                })
                .collect();
            s.push_str(&md::table(
                &["Declaration", "Declared", "Incident", "Title", "Programs"],
                &rows,
            ));
            if declarations.len() > 25 {
                s.push_str(&format!(
                    "…and {} more (see data).\n",
                    declarations.len() - 25
                ));
            }
        }
        let rows: Vec<Vec<String>> = counts
            .iter()
            .map(|c| {
                vec![
                    c.incident_type.clone(),
                    c.since_1953.to_string(),
                    c.in_window.to_string(),
                    c.most_recent.clone(),
                ]
            })
            .collect();
        s.push_str(&format!(
            "\nDeclarations by incident type since 1953:\n\n{}",
            md::table(
                &[
                    "Incident",
                    "Since 1953",
                    &format!("Last {years} years"),
                    "Most recent"
                ],
                &rows,
            )
        ));
        s.push_str(
            "Programs: IH = Individuals and Households, IA = Individual Assistance, PA = Public \
             Assistance, HM = Hazard Mitigation.\n",
        );
    }

    caveats.push(
        "NWS alerts are the official source for watches and warnings; this tool adds health \
         guidance but does not replace NWS, MEMA, or DPH instructions. Alerts change quickly: \
         re-check before acting."
            .into(),
    );
    if area.level == AreaLevel::Town {
        caveats.push(format!(
            "Alerts are matched to {} by county ({} County): NWS zones and county-based \
             warnings may cover only part of the county.",
            area.name,
            area.county.as_deref().unwrap_or_default()
        ));
    }
    caveats.push(
        "FEMA declarations record federal assistance, not every hazard event; many damaging \
         storms, floods, and heat waves were never declared."
            .into(),
    );
    caveats.extend(content::review_caveats(&[content::HAZARDS]));

    Ok(Envelope::new(
        s,
        HazardAlertsResult {
            area,
            alerts_status,
            alerts_updated,
            alerts,
            alerts_elsewhere_in_massachusetts: elsewhere,
            marine_alerts_omitted: marine_omitted,
            hazards,
            general_actions,
            declarations,
            declaration_counts: counts,
            declaration_window_years: years,
            links,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none())
    .decision_support())
}

fn alert_out(ctx: &Context, h: &HazardActions, a: &NwsAlert) -> AlertOut {
    let counties: Vec<String> = a
        .county_fips
        .iter()
        .filter(|f| f.starts_with("25"))
        .filter_map(|f| area::county_name(ctx, f))
        .collect();
    AlertOut {
        event: a.event.clone(),
        hazard: h.for_event(&a.event).map(|z| z.id.clone()),
        severity: a.severity.clone(),
        urgency: a.urgency.clone(),
        certainty: a.certainty.clone(),
        message_type: a.message_type.clone(),
        headline: a.headline.clone(),
        area_description: a.area_desc.clone(),
        counties,
        onset: a.onset.clone().or_else(|| a.effective.clone()),
        ends: a.ends.clone(),
        expires: a.expires.clone(),
        issued_by: a.sender_name.clone(),
        instruction: a.instruction.clone(),
        description: a
            .description
            .as_deref()
            .map(|d| truncate(d, DESCRIPTION_CHARS)),
        url: a.url(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[tokio::test]
    async fn offline_says_alerts_are_unavailable_and_uses_the_declaration_snapshot() {
        let (_d, c) = ctx();
        let env = get_active_hazard_alerts(
            &c,
            HazardAlertsArgs {
                area: Some("Fall River".into()),
                guidance_for: vec!["hurricane".into()],
                declaration_years: Some(5),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.alerts_status, "unavailable");
        assert!(env.caveats[0].contains("COULD NOT BE RETRIEVED"));
        assert!(env.caveats[0].contains("could not be checked"));
        assert!(env.summary.contains("NOT"));
        assert!(
            env.summary
                .contains("NWS alerts could not be checked — see weather.gov")
        );
        assert!(!env.summary.contains("No active NWS"));
        // Requested hazard plus its secondary hazards.
        let ids: Vec<&str> = d.hazards.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["tropical_cyclone", "carbon_monoxide", "power_outage"]
        );
        assert!(
            d.hazards[0]
                .public_health_actions
                .iter()
                .all(|a| !a.citation.is_empty())
        );
        // DR-4780-MA (September 2023 flooding) covered Bristol County.
        assert!(d.declarations.iter().any(|x| x.id == "DR-4780-MA"));
        assert!(
            d.declaration_counts
                .iter()
                .any(|x| x.incident_type.starts_with("Hurricane"))
        );
        assert!(env.notice.is_some());
        assert!(env.sources.iter().any(|s| s.mode == SourceMode::Snapshot));
    }

    #[test]
    fn ended_cancelled_and_test_alerts_are_not_current() {
        let body = serde_json::json!({"features": [
            {"properties": {"id": "a", "event": "Heat Advisory", "status": "Actual",
                "messageType": "Alert", "ends": "2026-07-15T20:00:00-04:00",
                "expires": "2026-07-15T12:00:00-04:00"}},
            {"properties": {"id": "b", "event": "Heat Advisory", "status": "Actual",
                "messageType": "Alert", "expires": "2026-07-14T20:00:00-04:00"}},
            {"properties": {"id": "c", "event": "Heat Advisory", "status": "Test",
                "messageType": "Alert", "ends": "2026-07-16T20:00:00-04:00"}},
            {"properties": {"id": "d", "event": "Heat Advisory", "status": "Actual",
                "messageType": "Cancel", "ends": "2026-07-16T20:00:00-04:00"}},
            {"properties": {"id": "e", "event": "Heat Advisory", "status": "Actual",
                "messageType": "Update"}}
        ]});
        let feed = nws::parse_alerts(body.to_string().as_bytes()).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-15T18:00:00-04:00")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let current: Vec<&str> = feed
            .alerts
            .iter()
            .filter(|a| alert_is_current(a, now))
            .map(|a| a.id.as_str())
            .collect();
        // "a" ends later today; "b" expired yesterday; "c" is a test message;
        // "d" is a cancellation; "e" has no end time and is kept.
        assert_eq!(current, vec!["a", "e"]);
    }

    #[test]
    fn groups_declarations_and_filters_by_county() {
        let rows = vec![
            Declaration {
                id: "DR-1-MA".into(),
                disaster_number: 1,
                declaration_type: "DR".into(),
                declaration_date: "2020-01-02".into(),
                incident_type: "Flood".into(),
                title: "FLOODING".into(),
                designated_area: "Bristol (County)(in (P)MSA 1120)".into(),
                county_fips: Some("25005".into()),
                incident_begin: None,
                incident_end: None,
                programs: vec!["PA".into()],
            },
            Declaration {
                id: "DR-1-MA".into(),
                disaster_number: 1,
                declaration_type: "DR".into(),
                declaration_date: "2020-01-02".into(),
                incident_type: "Flood".into(),
                title: "FLOODING".into(),
                designated_area: "Worcester (County)".into(),
                county_fips: Some("25027".into()),
                incident_begin: None,
                incident_end: None,
                programs: vec!["IH".into()],
            },
            Declaration {
                id: "EM-2-MA".into(),
                disaster_number: 2,
                declaration_type: "EM".into(),
                declaration_date: "2021-01-02".into(),
                incident_type: "Snowstorm".into(),
                title: "SNOW".into(),
                designated_area: "Statewide".into(),
                county_fips: None,
                incident_begin: None,
                incident_end: None,
                programs: vec![],
            },
        ];
        let worcester = Area {
            level: AreaLevel::County,
            name: "Worcester County".into(),
            geoid: "25027".into(),
            county_fips: Some("25027".into()),
            county: Some("Worcester".into()),
            town: None,
        };
        let d = declarations_for(&rows, &worcester);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].id, "EM-2-MA");
        assert_eq!(d[1].designated_areas, vec!["Bristol", "Worcester"]);
        assert_eq!(d[1].programs, vec!["PA", "IH"]);
        let c = incident_counts(&d, "2021-01-01");
        assert_eq!(c.len(), 2);
        assert_eq!(
            c.iter()
                .find(|x| x.incident_type == "Flood")
                .unwrap()
                .in_window,
            0
        );
    }
}
