//! ENV-05 `climate_health_vulnerability_profile`: a one-request town profile
//! for Municipal Vulnerability Preparedness (MVP) and hazard mitigation
//! planning: heat exposure and tree canopy (CDC/ATSDR Heat and Health Index),
//! flood insurance claims history and NFIP status (OpenFEMA), social
//! vulnerability (SVI, EJ block groups, ACS), climate-sensitive health
//! (CDC PLACES), electricity-dependent residents (HHS emPOWER), MVP and FEMA
//! hazard mitigation plan status, a transparent list of the most important
//! vulnerabilities (tool-default rules), and an optional DOCX profile.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::census::GeoLevel;
use phn_data::ejclimate::{
    self, EjLayer, HhiSnapshot, MitigationPlan, MvpSnapshot, NfipClaimsSnapshot, NfipCommunity,
};
use phn_data::geography::MaGeographies;
use phn_docs::model::{Block, Document, Inline, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::area::{self, Area, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, DocSource};
use crate::epr::population::{self, PopulationProfile};
use crate::md;

/// Content id of `content/env/climate_vulnerability.yaml`.
pub const CLIMATE_CONTENT: &str = "env-climate-vulnerability";

// ------------------------------------------------------------------ content

#[derive(Debug, Clone, Deserialize)]
struct IndicatorDef {
    id: String,
    label: String,
    #[allow(dead_code)]
    unit: String,
    #[allow(dead_code)]
    higher_is_worse: bool,
    source: String,
    vintage: String,
    definition: String,
}

/// Tool-default flagging rules.
#[derive(Debug, Clone, Deserialize)]
struct Rules {
    ratio_higher: f64,
    ratio_heat_days: f64,
    ratio_canopy_lower: f64,
    ratio_impervious: f64,
    ratio_nfip: f64,
    recent_claims_min: u32,
    ej_share_pct: f64,
    plan_expiry_months: u32,
    max_listed: usize,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ClimateContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, DocSource>,
    indicators: Vec<IndicatorDef>,
    method_notes: Vec<Cited>,
    rules: Rules,
    planning: Vec<Cited>,
}

impl ClimateContent {
    fn def(&self, id: &str) -> Option<&IndicatorDef> {
        self.indicators.iter().find(|d| d.id == id)
    }
}

// ------------------------------------------------------------------ arguments and result

/// Arguments for `climate_health_vulnerability_profile`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ClimateProfileArgs {
    /// A Massachusetts city or town. Default: your saved town.
    pub area: Option<String>,
    /// Write the profile as a Word document (DOCX) to /data/output (default false).
    pub document: Option<bool>,
}

/// One indicator with its Massachusetts comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileIndicator {
    pub id: String,
    pub label: String,
    pub value: Option<f64>,
    pub unit: String,
    pub massachusetts: Option<f64>,
    /// Town value divided by the Massachusetts value.
    pub ratio_to_state: Option<f64>,
    pub source: String,
    pub vintage: String,
    /// How the value was computed, when it is not published for the town.
    pub method: Option<String>,
}

/// A ZIP Code Tabulation Area that overlaps the town (Heat and Health Index).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HeatZip {
    pub zcta: String,
    /// Share of the town's land in this ZCTA (0–1).
    pub share_of_town_land: f64,
    pub extreme_heat_days: Option<f64>,
    pub tree_canopy_pct: Option<f64>,
    pub impervious_pct: Option<f64>,
    /// HHI overall national percentile rank (0–1; higher = more heat vulnerable).
    pub hhi_overall_rank: Option<f64>,
    /// HHI historical heat and health burden rank (0–1).
    pub hhi_heat_burden_rank: Option<f64>,
}

/// The town's flood insurance claims history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FloodHistory {
    pub claims: u32,
    pub paid_claims: u32,
    /// Dollars paid (building, contents, increased cost of compliance), not inflation-adjusted.
    pub total_paid: f64,
    pub claims_recent: u32,
    pub paid_recent: f64,
    pub recent_from: i32,
    pub recent_to: i32,
    pub first_year: Option<i32>,
    pub peak_year: Option<i32>,
    pub peak_year_claims: Option<u32>,
    pub claims_per_1000_housing_units: Option<f64>,
    pub massachusetts_claims_per_1000_housing_units: Option<f64>,
}

/// NFIP participation (Community Status Book).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NfipStatus {
    pub community_id: String,
    pub participating: bool,
    /// Current effective flood insurance rate map date, as published.
    pub current_map_date: Option<String>,
    /// Community Rating System class, when the town takes part.
    pub crs_class: Option<String>,
    /// Premium discount inside the special flood hazard area, as published.
    pub crs_sfha_discount: Option<String>,
    pub sanction: bool,
    /// "live", "cache", or "snapshot".
    pub mode: String,
}

/// One FEMA hazard mitigation plan covering the town.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanStatus {
    pub title: String,
    pub plan_type: Option<String>,
    pub status: String,
    pub approval_date: Option<String>,
    pub expiration_date: Option<String>,
    /// Months from today to expiration (negative when expired).
    pub months_to_expiration: Option<i64>,
}

/// The town's MVP record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MvpStatus {
    pub region: Option<String>,
    pub planning_reports: Vec<String>,
    pub designated_through_prior_work: bool,
    pub mvp2_fy24_pilot: bool,
}

/// Planning status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningStatus {
    pub mvp: Option<MvpStatus>,
    pub hazard_mitigation_plans: Vec<PlanStatus>,
    /// "live", "cache", "snapshot", or "unavailable".
    pub plans_mode: String,
    pub nfip: Option<NfipStatus>,
    /// Planning gaps found (tool default rule), such as an expired plan.
    pub flags: Vec<String>,
    pub notes: Vec<CitedOut>,
}

/// A flagged vulnerability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Vulnerability {
    pub indicator: String,
    pub label: String,
    pub town: String,
    pub massachusetts: String,
    /// How far the town is from the state (used to rank).
    pub score: f64,
    /// The rule that flagged it (tool default).
    pub rule: String,
}

/// Health effects of a climate hazard (from the hazard content).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HazardHealth {
    pub hazard: String,
    pub health_effects: Vec<CitedOut>,
    pub at_risk: Vec<CitedOut>,
}

/// An indicator definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndicatorDefinition {
    pub id: String,
    pub label: String,
    pub definition: String,
    pub vintage: String,
}

/// Result of `climate_health_vulnerability_profile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClimateProfileResult {
    pub area: Area,
    /// Most important vulnerabilities, by tool-default rules (see `rules`).
    pub most_important: Vec<Vulnerability>,
    pub rules: String,
    pub heat: Vec<ProfileIndicator>,
    pub heat_zips: Vec<HeatZip>,
    pub flood: Vec<ProfileIndicator>,
    pub flood_history: Option<FloodHistory>,
    pub social: Vec<ProfileIndicator>,
    /// ACS demographics, PLACES health measures, SVI, and emPOWER.
    pub population: PopulationProfile,
    pub planning: PlanningStatus,
    pub hazard_health: Vec<HazardHealth>,
    pub definitions: Vec<IndicatorDefinition>,
    pub document_file: Option<String>,
}

// ------------------------------------------------------------------ helpers

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn ratio(v: Option<f64>, ma: Option<f64>) -> Option<f64> {
    match (v, ma) {
        (Some(a), Some(b)) if b > 0.0 => Some(round2(a / b)),
        _ => None,
    }
}

fn source_title(c: &ClimateContent, key: &str) -> String {
    c.sources
        .get(key)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| key.to_string())
}

/// Land-area-weighted mean of a ZCTA field over (ZCTA, weight) pairs; returns
/// the mean and the share of the weight that had data.
fn weighted(
    hhi: &HhiSnapshot,
    zctas: &[(String, f64)],
    f: fn(&ejclimate::HhiZcta) -> Option<f64>,
) -> (Option<f64>, f64) {
    let total: f64 = zctas.iter().map(|(_, w)| *w).sum();
    let (mut num, mut den) = (0.0, 0.0);
    for (z, w) in zctas {
        if let Some(v) = hhi.zcta(z).and_then(f) {
            num += v * w;
            den += w;
        }
    }
    (
        (den > 0.0).then(|| num / den),
        if total > 0.0 { den / total } else { 0.0 },
    )
}

/// The town an NFIP community name ("SCITUATE, TOWN OF") refers to.
fn community_town(ctx: &Context, name: &str) -> Option<String> {
    let base = name.split(',').next().unwrap_or(name).trim();
    let n = phn_core::text::normalize(base);
    ctx.ma
        .municipalities
        .iter()
        .find(|m| phn_core::text::normalize(&m.name) == n)
        .map(|m| m.name.clone())
}

/// The town a FEMA plan row covers: by NFIP community number, else the Census
/// place name ("Agawam Town city"), else a whole-word town name in the title
/// for town and city rows in the same county.
fn plan_town(
    ctx: &Context,
    p: &MitigationPlan,
    communities: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(t) = p.community_id.as_ref().and_then(|c| communities.get(c)) {
        return Some(t.clone());
    }
    if let Some(place) = &p.place_name {
        let lower = place.to_ascii_lowercase();
        let base = ["town city", "city", "town"]
            .iter()
            .find_map(|suffix| {
                lower
                    .strip_suffix(suffix)
                    .map(|b| place[..b.len()].trim().to_string())
            })
            .unwrap_or_else(|| place.clone());
        let n = phn_core::text::normalize(&base);
        if let Some(m) = ctx
            .ma
            .municipalities
            .iter()
            .find(|m| phn_core::text::normalize(&m.name) == n)
        {
            return Some(m.name.clone());
        }
    }
    if matches!(p.jurisdiction_type.as_str(), "Town" | "City") {
        let county = p.county.as_deref().unwrap_or_default();
        let title = phn_core::text::normalize(&p.title);
        let words: Vec<&str> = title.split_whitespace().collect();
        let hits: Vec<&str> = ctx
            .ma
            .municipalities
            .iter()
            .filter(|m| m.county.eq_ignore_ascii_case(county))
            .filter(|m| {
                let n = phn_core::text::normalize(&m.name);
                let parts: Vec<&str> = n.split_whitespace().collect();
                words.windows(parts.len()).any(|w| w == parts.as_slice())
            })
            .map(|m| m.name.as_str())
            .collect();
        if hits.len() == 1 {
            return Some(hits[0].to_string());
        }
    }
    None
}

/// Whole months from `today` to `date` (YYYY-MM-DD); negative when past.
fn months_until(today: chrono::NaiveDate, date: &str) -> Option<i64> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    use chrono::Datelike;
    if !(1900..=2100).contains(&d.year()) {
        return None;
    }
    let mut m =
        (d.year() as i64 - today.year() as i64) * 12 + d.month() as i64 - today.month() as i64;
    if d >= today && d.day() < today.day() {
        m -= 1;
    } else if d < today && d.day() > today.day() {
        m += 1;
    }
    Some(m)
}

fn fmt_value(v: Option<f64>, unit: &str) -> String {
    match v {
        None => "—".into(),
        Some(x) if unit == "percent" || unit == "%" => format!("{}%", md::num(x, 1)),
        Some(x) if unit == "dollars" => format!("${}", md::num(x, 0)),
        Some(x) if unit == "claims" => md::num(x, 0),
        // Counts such as "of 9 tracts".
        Some(x) if unit.starts_with("of ") => format!("{} {unit}", md::num(x, 0)),
        Some(x) => md::num(x, 1),
    }
}

// ------------------------------------------------------------------ the tool

/// Build a climate and health vulnerability profile for a town.
pub async fn climate_health_vulnerability_profile(
    ctx: &Context,
    args: ClimateProfileArgs,
) -> Result<Envelope<ClimateProfileResult>> {
    let c: ClimateContent = content::load(CLIMATE_CONTENT)?;
    let area = area::resolve(ctx, args.area.as_deref(), false)?;
    if area.level != AreaLevel::Town {
        return Err(PhnError::InvalidInput(
            "the climate and health vulnerability profile is for one city or town (MVP and \
             hazard mitigation plans are municipal); name a city or town"
                .into(),
        ));
    }
    let town = area.town.clone().unwrap_or_default();
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let today = chrono::Local::now().date_naive();

    // Population, health, SVI, emPOWER.
    let (population, psrc, pcav) = population::profile(ctx, &area, true).await?;
    sources.extend(psrc);
    caveats.extend(pcav);
    let ma_empower = population::empower_for(ctx, &Area::state())
        .await
        .ok()
        .and_then(|(e, _)| e.per_1000_beneficiaries);

    // Heat exposure (Heat and Health Index, ZCTAs weighted by land area).
    let hhi = HhiSnapshot::shared(&ctx.snapshots)?;
    let town_zctas: Vec<(String, f64)> = geos
        .zctas_for_town(&town)
        .into_iter()
        .map(|(g, share)| (g.geoid.clone(), share * g.aland_sqmi.unwrap_or(0.0)))
        .filter(|(_, w)| *w > 0.0)
        .collect();
    let state_zctas: Vec<(String, f64)> = geos
        .at_level(GeoLevel::Zcta)
        .map(|g| (g.geoid.clone(), g.aland_sqmi.unwrap_or(0.0)))
        .filter(|(_, w)| *w > 0.0)
        .collect();
    let heat_method = "Average of the ZIP Code Tabulation Areas overlapping the town, weighted by \
                       each one's land area in the town (this server's method; HHI publishes ZCTA \
                       values only)";
    let mut heat = Vec::new();
    let mut low_coverage = false;
    for (id, unit, f) in [
        (
            "extreme_heat_days",
            "days a year",
            (|z: &ejclimate::HhiZcta| z.extreme_heat_days)
                as fn(&ejclimate::HhiZcta) -> Option<f64>,
        ),
        ("tree_canopy_pct", "percent", |z| z.pct_tree_canopy),
        ("impervious_pct", "percent", |z| z.pct_impervious),
    ] {
        let (v, cov) = weighted(hhi, &town_zctas, f);
        let (ma, _) = weighted(hhi, &state_zctas, f);
        if cov < 0.5 {
            low_coverage = true;
        }
        let d = c.def(id);
        heat.push(ProfileIndicator {
            id: id.into(),
            label: d.map(|d| d.label.clone()).unwrap_or_else(|| id.into()),
            value: v.map(round1),
            unit: unit.into(),
            massachusetts: ma.map(round1),
            ratio_to_state: ratio(v, ma),
            source: "CDC/ATSDR Heat and Health Index 2024".into(),
            vintage: d.map(|d| d.vintage.clone()).unwrap_or_default(),
            method: Some(heat_method.into()),
        });
    }
    if low_coverage {
        caveats.push(format!(
            "Heat and Health Index values cover less than half of {town}'s land (ZIP areas \
             without data); treat the heat and canopy figures with caution."
        ));
    }
    let total_w: f64 = town_zctas.iter().map(|(_, w)| *w).sum();
    let mut heat_zips: Vec<HeatZip> = town_zctas
        .iter()
        .map(|(z, w)| {
            let h = hhi.zcta(z);
            HeatZip {
                zcta: z.clone(),
                share_of_town_land: if total_w > 0.0 {
                    round2(w / total_w)
                } else {
                    0.0
                },
                extreme_heat_days: h.and_then(|h| h.extreme_heat_days),
                tree_canopy_pct: h.and_then(|h| h.pct_tree_canopy),
                impervious_pct: h.and_then(|h| h.pct_impervious),
                hhi_overall_rank: h.and_then(|h| h.rank_overall),
                hhi_heat_burden_rank: h.and_then(|h| h.rank_heat_burden),
            }
        })
        .filter(|z| z.share_of_town_land >= 0.01)
        .collect();
    heat_zips.sort_by(|a, b| {
        b.share_of_town_land
            .total_cmp(&a.share_of_town_land)
            .then_with(|| a.zcta.cmp(&b.zcta))
    });
    sources.push(hhi.source.clone());

    // Flood: NFIP claims.
    let ej = EjLayer::shared(&ctx.snapshots)?;
    let claims = NfipClaimsSnapshot::shared(&ctx.snapshots)?;
    let housing = ej.town(&town).map(|t| t.housing_units_2020);
    let ma_claims: f64 = claims.towns.iter().map(|t| t.claims as f64).sum();
    let ma_housing: f64 = ej.towns.iter().map(|t| t.housing_units_2020).sum();
    let ma_rate = (ma_housing > 0.0).then(|| round1(ma_claims / ma_housing * 1000.0));
    let flood_history = claims.town(&town).map(|t| {
        let rate = housing
            .filter(|h| *h > 0.0)
            .map(|h| round1(t.claims as f64 / h * 1000.0));
        FloodHistory {
            claims: t.claims,
            paid_claims: t.paid_claims,
            total_paid: t.total_paid,
            claims_recent: t.claims_recent,
            paid_recent: t.paid_recent,
            recent_from: t.recent_from,
            recent_to: t.recent_to,
            first_year: t.first_year,
            peak_year: t.peak_year,
            peak_year_claims: t.peak_year_claims,
            claims_per_1000_housing_units: rate,
            massachusetts_claims_per_1000_housing_units: ma_rate,
        }
    });
    let mut flood = Vec::new();
    if let Some(f) = &flood_history {
        let d = c.def("nfip_claims_per_1000");
        flood.push(ProfileIndicator {
            id: "nfip_claims_per_1000".into(),
            label: d.map(|d| d.label.clone()).unwrap_or_default(),
            value: f.claims_per_1000_housing_units,
            unit: "per 1,000 housing units".into(),
            massachusetts: ma_rate,
            ratio_to_state: ratio(f.claims_per_1000_housing_units, ma_rate),
            source: "OpenFEMA NFIP Redacted Claims (v3)".into(),
            vintage: d.map(|d| d.vintage.clone()).unwrap_or_default(),
            method: Some("claims for the town ÷ 2020 Census housing units × 1,000".into()),
        });
        let d = c.def("nfip_claims_recent");
        flood.push(ProfileIndicator {
            id: "nfip_claims_recent".into(),
            label: format!(
                "{} ({}–{})",
                d.map(|d| d.label.clone()).unwrap_or_default(),
                f.recent_from,
                f.recent_to
            ),
            value: Some(f.claims_recent as f64),
            unit: "claims".into(),
            massachusetts: None,
            ratio_to_state: None,
            source: "OpenFEMA NFIP Redacted Claims (v3)".into(),
            vintage: d.map(|d| d.vintage.clone()).unwrap_or_default(),
            method: None,
        });
        sources.push(claims.source.clone());
    }

    // NFIP status and hazard mitigation plans (live → cache → snapshot).
    let mut communities: BTreeMap<String, String> = BTreeMap::new();
    let mut nfip = None;
    match ejclimate::fetch_communities(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await {
        Ok(f) => {
            if let Some(r) = &f.fallback_reason {
                caveats.push(format!("NFIP Community Status Book: {r}."));
            }
            let mut mine: Option<&NfipCommunity> = None;
            for row in &f.rows {
                if let Some(t) = community_town(ctx, &row.name) {
                    if t == town {
                        mine = Some(row);
                    }
                    communities.insert(row.community_id.clone(), t);
                }
            }
            nfip = mine.map(|r| NfipStatus {
                community_id: r.community_id.clone(),
                participating: r.participating,
                current_map_date: r.current_map_date.clone(),
                crs_class: r.crs_class.clone(),
                crs_sfha_discount: r.sfha_discount.clone(),
                sanction: r.sanction,
                mode: f.mode.label().to_string(),
            });
            if nfip.is_none() {
                caveats.push(format!(
                    "{town} is not listed in FEMA's NFIP Community Status Book."
                ));
            }
            sources.push(Source {
                title: "NFIP Community Status Book (Massachusetts)".into(),
                publisher: "Federal Emergency Management Agency (OpenFEMA)".into(),
                url: Some(ejclimate::communities_url()),
                effective_date: None,
                retrieved: Some(f.as_of.chars().take(10).collect()),
                mode: f.mode,
            });
        }
        Err(e) => caveats.push(format!(
            "The NFIP Community Status Book could not be read ({}).",
            phn_data::apifetch::reason(&e)
        )),
    }
    let mut plans: Vec<PlanStatus> = Vec::new();
    let mut plans_mode = "unavailable".to_string();
    match ejclimate::fetch_plans(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await {
        Ok(f) => {
            if let Some(r) = &f.fallback_reason {
                caveats.push(format!("FEMA hazard mitigation plan statuses: {r}."));
            }
            plans_mode = f.mode.label().to_string();
            let mut seen = std::collections::BTreeSet::new();
            for p in &f.rows {
                if plan_town(ctx, p, &communities).as_deref() != Some(town.as_str()) {
                    continue;
                }
                if !seen.insert(p.plan_id.clone()) {
                    continue;
                }
                plans.push(PlanStatus {
                    title: p.title.clone(),
                    plan_type: p.plan_type.clone(),
                    status: p.plan_status.clone(),
                    approval_date: p.approval_date.clone(),
                    expiration_date: p.expiration_date.clone(),
                    months_to_expiration: p
                        .expiration_date
                        .as_deref()
                        .and_then(|d| months_until(today, d)),
                });
            }
            plans.sort_by(|a, b| {
                b.expiration_date
                    .cmp(&a.expiration_date)
                    .then_with(|| a.title.cmp(&b.title))
            });
            sources.push(Source {
                title: "FEMA Hazard Mitigation Plan Statuses (Massachusetts)".into(),
                publisher: "Federal Emergency Management Agency (OpenFEMA)".into(),
                url: Some(ejclimate::plans_url()),
                effective_date: None,
                retrieved: Some(f.as_of.chars().take(10).collect()),
                mode: f.mode,
            });
        }
        Err(e) => caveats.push(format!(
            "FEMA hazard mitigation plan statuses could not be read ({}).",
            phn_data::apifetch::reason(&e)
        )),
    }
    let mut planning_flags = Vec::new();
    let approved: Vec<&PlanStatus> = plans
        .iter()
        .filter(|p| p.status.eq_ignore_ascii_case("Approved"))
        .collect();
    if plans_mode != "unavailable" {
        match approved.iter().filter_map(|p| p.months_to_expiration).max() {
            None => planning_flags.push(format!(
                "No FEMA-approved hazard mitigation plan is listed for {town}{}.",
                plans
                    .first()
                    .map(|p| format!(
                        " (most recent: \"{}\", {}{})",
                        p.title,
                        p.status.to_lowercase(),
                        p.expiration_date
                            .as_deref()
                            .map(|d| format!(", expiration {d}"))
                            .unwrap_or_default()
                    ))
                    .unwrap_or_default()
            )),
            Some(m) if m < 0 => planning_flags.push(format!(
                "{town}'s FEMA-approved hazard mitigation plan has expired."
            )),
            Some(m) if m < c.rules.plan_expiry_months as i64 => planning_flags.push(format!(
                "{town}'s FEMA-approved hazard mitigation plan expires within {} months; plans \
                 must be revised and resubmitted within 5 years to stay eligible for mitigation \
                 grants.",
                m.max(0) + 1
            )),
            _ => {}
        }
    }

    // MVP.
    let mvp_snap = MvpSnapshot::shared(&ctx.snapshots)?;
    let mvp = mvp_snap.town(&town).map(|m| MvpStatus {
        region: m.region.clone(),
        planning_reports: m.planning_reports.clone(),
        designated_through_prior_work: m.designated_through_prior_work,
        mvp2_fy24_pilot: m.mvp2_fy24_pilot,
    });
    sources.push(mvp_snap.source.clone());

    // Social vulnerability: EJ share and SVI.
    let mut social = Vec::new();
    let ej_town = ej.town(&town);
    let ej_share = ej_town.map(|t| t.pct_pop_in_ej_block_groups.unwrap_or(0.0));
    let ma_ej_share = {
        let mut seen = std::collections::BTreeSet::new();
        let people: f64 = ej
            .block_groups
            .iter()
            .filter(|b| seen.insert(b.geoid.as_str()))
            .map(|b| b.population)
            .sum();
        let pop: f64 = ej.towns.iter().map(|t| t.pop_2020).sum();
        (pop > 0.0).then(|| round1(people / pop * 100.0))
    };
    social.push(ProfileIndicator {
        id: "ej_population_pct".into(),
        label: "Residents living in environmental justice block groups".into(),
        value: ej_share,
        unit: "percent".into(),
        massachusetts: ma_ej_share,
        ratio_to_state: ratio(ej_share, ma_ej_share),
        source: "EEA 2020 Environmental Justice Populations (municipal statistics)".into(),
        vintage: "2020 Census and ACS 2016–2020 (EEA update November 2022)".into(),
        method: None,
    });
    sources.push(ej.source.clone());
    if let Some(s) = &population.svi {
        social.push(ProfileIndicator {
            id: "svi_high_tracts".into(),
            label: "Census tracts in the top quarter of the Massachusetts Social Vulnerability \
                    Index"
                .into(),
            value: Some(s.high_vulnerability_tracts as f64),
            unit: format!("of {} tracts", s.tracts),
            massachusetts: None,
            ratio_to_state: None,
            source: "CDC/ATSDR SVI 2022".into(),
            vintage: "ACS 2018–2022".into(),
            method: None,
        });
    }

    // Flags (tool default rules).
    let r = &c.rules;
    let mut flags: Vec<Vulnerability> = Vec::new();
    let pct = |v: f64| format!("{}%", md::num(v, 1));
    for i in population
        .demographics
        .iter()
        .chain(population.health.iter())
    {
        if i.unreliable {
            continue;
        }
        if let (Some(v), Some(m)) = (i.percent, i.massachusetts)
            && m > 0.0
            && v / m >= r.ratio_higher
        {
            flags.push(Vulnerability {
                indicator: i.id.clone(),
                label: i.label.clone(),
                town: pct(v),
                massachusetts: pct(m),
                score: round2(v / m),
                rule: format!(
                    "tool default: at least {} times the Massachusetts value",
                    r.ratio_higher
                ),
            });
        }
    }
    for h in &heat {
        let (Some(v), Some(m), Some(q)) = (h.value, h.massachusetts, h.ratio_to_state) else {
            continue;
        };
        let (hit, score, rule) = match h.id.as_str() {
            "extreme_heat_days" => (
                q >= r.ratio_heat_days,
                q,
                format!(
                    "tool default: at least {} times the Massachusetts value",
                    r.ratio_heat_days
                ),
            ),
            "tree_canopy_pct" => (
                q <= r.ratio_canopy_lower,
                if v > 0.0 { round2(m / v) } else { 10.0 },
                format!(
                    "tool default: {} times the Massachusetts value or less",
                    r.ratio_canopy_lower
                ),
            ),
            _ => (
                q >= r.ratio_impervious,
                q,
                format!(
                    "tool default: at least {} times the Massachusetts value",
                    r.ratio_impervious
                ),
            ),
        };
        if hit {
            let f = |x: f64| {
                if h.unit == "percent" {
                    pct(x)
                } else {
                    md::num(x, 1)
                }
            };
            flags.push(Vulnerability {
                indicator: h.id.clone(),
                label: if h.id == "tree_canopy_pct" {
                    "Low tree canopy".into()
                } else {
                    h.label.clone()
                },
                town: f(v),
                massachusetts: f(m),
                score,
                rule,
            });
        }
    }
    if let Some(f) = &flood_history {
        let q = ratio(
            f.claims_per_1000_housing_units,
            f.massachusetts_claims_per_1000_housing_units,
        );
        if q.is_some_and(|q| q >= r.ratio_nfip) || f.claims_recent >= r.recent_claims_min {
            flags.push(Vulnerability {
                indicator: "nfip_claims".into(),
                label: "Flood insurance claims history".into(),
                town: format!(
                    "{} claims per 1,000 housing units; {} claims in {}–{}",
                    f.claims_per_1000_housing_units
                        .map(|v| md::num(v, 1))
                        .unwrap_or_else(|| "—".into()),
                    f.claims_recent,
                    f.recent_from,
                    f.recent_to
                ),
                massachusetts: format!(
                    "{} per 1,000",
                    f.massachusetts_claims_per_1000_housing_units
                        .map(|v| md::num(v, 1))
                        .unwrap_or_else(|| "—".into())
                ),
                score: q.unwrap_or(r.ratio_nfip),
                rule: format!(
                    "tool default: claims rate at least {} times the state rate, or {} or more \
                     claims in the last ten years",
                    r.ratio_nfip, r.recent_claims_min
                ),
            });
        }
    }
    if let Some(s) = &population.svi
        && s.high_vulnerability_tracts > 0
    {
        flags.push(Vulnerability {
            indicator: "svi_high_tracts".into(),
            label: "Socially vulnerable neighborhoods (SVI top quarter)".into(),
            town: format!(
                "{} of {} tracts (about {} residents)",
                s.high_vulnerability_tracts,
                s.tracts,
                md::num(s.population_in_high_tracts, 0)
            ),
            massachusetts: "25% of tracts by definition".into(),
            score: round2(1.0 + s.high_vulnerability_tracts as f64 / s.tracts.max(1) as f64 * 4.0)
                .max(1.0),
            rule: "tool default: any tract in the top quarter of Massachusetts".into(),
        });
    }
    if let Some(v) = ej_share
        && v >= r.ej_share_pct
    {
        flags.push(Vulnerability {
            indicator: "ej_population_pct".into(),
            label: "Residents living in environmental justice block groups".into(),
            town: pct(v),
            massachusetts: ma_ej_share.map(pct).unwrap_or_else(|| "—".into()),
            score: ratio(Some(v), ma_ej_share).unwrap_or(1.0),
            rule: format!("tool default: {}% or more of residents", r.ej_share_pct),
        });
    }
    if let (Some(e), Some(m)) = (&population.electricity_dependent, ma_empower)
        && let Some(v) = e.per_1000_beneficiaries
        && m > 0.0
        && v / m >= r.ratio_higher
    {
        flags.push(Vulnerability {
            indicator: "empower_per_1000".into(),
            label: "Medicare beneficiaries using electricity-dependent equipment".into(),
            town: format!("{} per 1,000", md::num(v, 1)),
            massachusetts: format!("{} per 1,000", md::num(m, 1)),
            score: round2(v / m),
            rule: format!(
                "tool default: at least {} times the Massachusetts rate",
                r.ratio_higher
            ),
        });
    }
    flags.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.label.cmp(&b.label))
    });
    let flagged_total = flags.len();
    flags.truncate(r.max_listed);

    // Hazard health effects (shared hazard content).
    let hz: content::HazardActions = content::load(content::HAZARDS)?;
    let hazard_health: Vec<HazardHealth> = ["extreme_heat", "flood"]
        .iter()
        .filter_map(|id| hz.hazards.iter().find(|h| h.id == *id))
        .map(|h| HazardHealth {
            hazard: h.label.clone(),
            health_effects: content::cite_all(&h.health_effects, &hz.sources)
                .into_iter()
                .take(4)
                .collect(),
            at_risk: content::cite_all(&h.at_risk, &hz.sources)
                .into_iter()
                .take(4)
                .collect(),
        })
        .collect();

    let planning = PlanningStatus {
        mvp,
        hazard_mitigation_plans: plans,
        plans_mode,
        nfip,
        flags: planning_flags,
        notes: content::cite_all(&c.planning, &c.sources),
    };
    let definitions: Vec<IndicatorDefinition> = c
        .indicators
        .iter()
        .map(|d| IndicatorDefinition {
            id: d.id.clone(),
            label: d.label.clone(),
            definition: format!("{} ({})", d.definition, source_title(&c, &d.source)),
            vintage: d.vintage.clone(),
        })
        .collect();

    // Summary.
    let mut s = format!(
        "**Climate and health vulnerability profile — {town}** ({} County)\n\n",
        area.county.clone().unwrap_or_default()
    );
    if let Some(p) = population.population {
        s.push_str(&format!("Population {}. ", md::num(p, 0)));
    }
    s.push_str(
        "For MVP and hazard mitigation planning; every value is compared with Massachusetts.\n\n",
    );
    s.push_str("**Most important vulnerabilities** (tool-default rules; see `rules`)\n\n");
    if flags.is_empty() {
        s.push_str(
            "No indicator met the tool's default flagging rules; review the full tables.\n\n",
        );
    } else {
        let rows: Vec<Vec<String>> = flags
            .iter()
            .enumerate()
            .map(|(i, f)| {
                vec![
                    (i + 1).to_string(),
                    f.label.clone(),
                    f.town.clone(),
                    f.massachusetts.clone(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &["#", "Vulnerability", town.as_str(), "Massachusetts"],
            &rows,
        ));
        if flagged_total > flags.len() {
            s.push_str(&format!(
                "\n{} more indicator(s) met a rule; see the tables below.\n",
                flagged_total - flags.len()
            ));
        }
        s.push('\n');
    }
    let ind_rows = |list: &[ProfileIndicator]| -> Vec<Vec<String>> {
        list.iter()
            .map(|i| {
                vec![
                    i.label.clone(),
                    fmt_value(i.value, &i.unit),
                    fmt_value(i.massachusetts, &i.unit),
                    format!("{} ({})", i.source, i.vintage),
                ]
            })
            .collect()
    };
    s.push_str("**Heat exposure and tree canopy**\n\n");
    s.push_str(&md::table(
        &["Indicator", town.as_str(), "Massachusetts", "Source"],
        &ind_rows(&heat),
    ));
    if let Some(z) = heat_zips
        .iter()
        .filter(|z| z.hhi_overall_rank.is_some())
        .max_by(|a, b| {
            a.hhi_overall_rank
                .unwrap_or(0.0)
                .total_cmp(&b.hhi_overall_rank.unwrap_or(0.0))
                .then_with(|| b.zcta.cmp(&a.zcta))
        })
    {
        s.push_str(&format!(
            "\nHighest Heat and Health Index rank among the town's ZIP areas: {} (national \
             percentile {}).\n",
            z.zcta,
            md::num(z.hhi_overall_rank.unwrap_or(0.0) * 100.0, 0)
        ));
    }
    s.push_str("\n**Flooding**\n\n");
    if let Some(f) = &flood_history {
        s.push_str(&format!(
            "NFIP flood insurance: {} claims since {} ({} paid, ${} total, not adjusted for \
             inflation); {} claims in {}–{}{}. Claims per 1,000 housing units: {} (Massachusetts \
             {}).",
            md::num(f.claims as f64, 0),
            f.first_year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "1978".into()),
            md::num(f.paid_claims as f64, 0),
            md::num(f.total_paid, 0),
            f.claims_recent,
            f.recent_from,
            f.recent_to,
            f.peak_year
                .map(|y| format!("; most claims in {y} ({})", f.peak_year_claims.unwrap_or(0)))
                .unwrap_or_default(),
            fmt_value(f.claims_per_1000_housing_units, ""),
            fmt_value(f.massachusetts_claims_per_1000_housing_units, "")
        ));
    }
    if let Some(n) = &planning.nfip {
        s.push_str(&format!(
            " NFIP: {}; current flood map {}{}{}.",
            if n.participating {
                "participating"
            } else {
                "not participating"
            },
            n.current_map_date.clone().unwrap_or_else(|| "—".into()),
            n.crs_class
                .as_deref()
                .map(|k| format!(
                    "; Community Rating System class {k}{}",
                    n.crs_sfha_discount
                        .as_deref()
                        .map(|d| format!(" ({d} discount in the flood zone)"))
                        .unwrap_or_default()
                ))
                .unwrap_or_default(),
            if n.sanction { "; sanctioned" } else { "" }
        ));
    }
    s.push_str("\n\n**Social vulnerability**\n\n");
    s.push_str(&md::table(
        &["Indicator", town.as_str(), "Massachusetts", "Source"],
        &ind_rows(&social),
    ));
    s.push_str("\n**People and climate-sensitive health**\n\n");
    s.push_str(&population::markdown(&population));
    s.push_str("\n**Planning status**\n\n");
    match &planning.mvp {
        Some(m) => s.push_str(&format!(
            "MVP: {}{}{}{}.\n",
            m.region
                .as_deref()
                .map(|r| r.to_string())
                .unwrap_or_else(|| "region not listed".into()),
            if m.planning_reports.is_empty() {
                "; no MVP planning report is posted on mass.gov".to_string()
            } else {
                format!("; planning report: {}", m.planning_reports.join("; "))
            },
            if m.designated_through_prior_work {
                "; designated through prior work"
            } else {
                ""
            },
            if m.mvp2_fy24_pilot {
                "; in the FY24 MVP 2.0 pilot"
            } else {
                ""
            }
        )),
        None => s.push_str("MVP: no record.\n"),
    }
    if planning.hazard_mitigation_plans.is_empty() {
        s.push_str(&format!(
            "FEMA hazard mitigation plans ({}): none listed for {town}.\n",
            planning.plans_mode
        ));
    } else {
        let rows: Vec<Vec<String>> = planning
            .hazard_mitigation_plans
            .iter()
            .take(4)
            .map(|p| {
                vec![
                    p.title.clone(),
                    p.status.clone(),
                    p.approval_date.clone().unwrap_or_else(|| "—".into()),
                    p.expiration_date.clone().unwrap_or_else(|| "—".into()),
                ]
            })
            .collect();
        s.push_str(&format!(
            "FEMA hazard mitigation plans ({}):\n\n",
            planning.plans_mode
        ));
        s.push_str(&md::table(
            &["Plan", "Status", "Approved", "Expires"],
            &rows,
        ));
    }
    for f in &planning.flags {
        s.push_str(&format!("\n- **Planning flag:** {f}"));
    }
    s.push('\n');

    // Document.
    let mut files_written = Vec::new();
    let mut document_file = None;
    if args.document.unwrap_or(false) {
        let doc = profile_document(
            ctx,
            &area,
            &flags,
            &heat,
            &heat_zips,
            &flood,
            flood_history.as_ref(),
            &social,
            &population,
            &planning,
            &hazard_health,
            &definitions,
            &c,
        );
        let bytes = phn_docs::docx::to_docx(&doc)?;
        let sandbox = ctx.sandbox()?;
        let slug: String = town
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let path = sandbox.new_output_file(&format!("climate_health_vulnerability_{slug}.docx"))?;
        std::fs::write(&path, bytes)?;
        let disp = sandbox.display_path(&path);
        s.push_str(&format!("\nProfile written to `{disp}`.\n"));
        files_written.push(disp.clone());
        document_file = Some(disp);
    }

    // Caveats and sources.
    caveats.insert(0, c.rules.text.clone());
    for n in content::cite_all(&c.method_notes, &c.sources) {
        caveats.push(format!("{} ({})", n.text, n.citation));
    }
    caveats.push(
        "NFIP claims count insured flood losses only; they understate flooding where few homes \
         carry flood insurance and are not a map of flood risk. Check FEMA flood maps (msc.fema.gov) \
         for the special flood hazard area."
            .into(),
    );
    caveats.push(
        "No verified town-level temperature or heat-day series other than the Heat and Health \
         Index was available (ResilientMass and CDC's Tracking Network could not be reached when \
         this was built)."
            .into(),
    );
    if planning
        .mvp
        .as_ref()
        .is_some_and(|m| m.planning_reports.is_empty())
    {
        caveats.push(
            "mass.gov does not publish a list of designated MVP communities; a missing planning \
             report does not mean the town is not an MVP community. Ask your MVP regional \
             coordinator (mvp@mass.gov)."
                .into(),
        );
    }
    caveats.extend(content::review_caveats(&[
        CLIMATE_CONTENT,
        content::HAZARDS,
    ]));
    let mut keys: Vec<&str> = c.indicators.iter().map(|d| d.source.as_str()).collect();
    keys.extend(c.planning.iter().map(|p| p.source.as_str()));
    sources.extend(content::sources_for(keys, &c.sources));
    sources.push(Source {
        title: "Town values computed from ZIP-area Heat and Health Index data and 2020 Census \
                housing units"
            .into(),
        publisher: "Public Health Nursing MCP Server".into(),
        url: None,
        effective_date: None,
        retrieved: Some(today.to_string()),
        mode: SourceMode::Computed,
    });

    Ok(Envelope::new(
        s,
        ClimateProfileResult {
            area,
            most_important: flags,
            rules: c.rules.text.clone(),
            heat,
            heat_zips,
            flood,
            flood_history,
            social,
            population,
            planning,
            hazard_health,
            definitions,
            document_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        files_written,
        ..Privacy::none()
    })
    .decision_support())
}

#[allow(clippy::too_many_arguments)]
fn profile_document(
    ctx: &Context,
    area: &Area,
    flags: &[Vulnerability],
    heat: &[ProfileIndicator],
    heat_zips: &[HeatZip],
    flood: &[ProfileIndicator],
    flood_history: Option<&FloodHistory>,
    social: &[ProfileIndicator],
    population: &PopulationProfile,
    planning: &PlanningStatus,
    hazard_health: &[HazardHealth],
    definitions: &[IndicatorDefinition],
    c: &ClimateContent,
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
    let ind_table = |caption: &str, list: &[ProfileIndicator]| {
        Block::Table(TableBlock {
            caption: Some(caption.into()),
            headers: ["Indicator", "Town", "Massachusetts", "Source and vintage"]
                .map(String::from)
                .to_vec(),
            rows: list
                .iter()
                .map(|i| {
                    vec![
                        i.label.clone(),
                        fmt_value(i.value, &i.unit),
                        fmt_value(i.massachusetts, &i.unit),
                        format!("{} ({})", i.source, i.vintage),
                    ]
                })
                .collect(),
        })
    };
    let town = area.name.clone();
    let mut blocks = vec![p(&format!(
        "Prepared {} for {} for Municipal Vulnerability Preparedness (MVP) and hazard mitigation \
         planning. Decision support only: confirm figures with the cited sources.",
        chrono::Local::now().date_naive(),
        town
    ))];
    blocks.push(h("1. Most important vulnerabilities"));
    if flags.is_empty() {
        blocks.push(p("No indicator met the tool's default flagging rules."));
    } else {
        blocks.push(Block::Table(TableBlock {
            caption: Some("Most important vulnerabilities (tool-default rules)".into()),
            headers: ["Vulnerability", "Town", "Massachusetts", "Rule"]
                .map(String::from)
                .to_vec(),
            rows: flags
                .iter()
                .map(|f| {
                    vec![
                        f.label.clone(),
                        f.town.clone(),
                        f.massachusetts.clone(),
                        f.rule.clone(),
                    ]
                })
                .collect(),
        }));
    }
    blocks.push(p(&c.rules.text));
    blocks.push(h("2. Heat exposure and tree canopy"));
    blocks.push(ind_table("Heat exposure and tree canopy", heat));
    if !heat_zips.is_empty() {
        let f = |v: Option<f64>, d: usize| v.map(|x| md::num(x, d)).unwrap_or_else(|| "—".into());
        blocks.push(Block::Table(TableBlock {
            caption: Some("Heat and Health Index by ZIP area".into()),
            headers: [
                "ZIP area (ZCTA)",
                "Share of town land",
                "Extreme heat days",
                "Tree canopy (%)",
                "Impervious (%)",
                "HHI overall national percentile",
            ]
            .map(String::from)
            .to_vec(),
            rows: heat_zips
                .iter()
                .map(|z| {
                    vec![
                        z.zcta.clone(),
                        format!("{}%", md::num(z.share_of_town_land * 100.0, 0)),
                        f(z.extreme_heat_days, 1),
                        f(z.tree_canopy_pct, 1),
                        f(z.impervious_pct, 1),
                        f(z.hhi_overall_rank.map(|r| r * 100.0), 0),
                    ]
                })
                .collect(),
        }));
    }
    blocks.push(h("3. Flooding"));
    if !flood.is_empty() {
        blocks.push(ind_table("Flood insurance claims", flood));
    }
    if let Some(f) = flood_history {
        blocks.push(p(&format!(
            "NFIP claims since {}: {} ({} with payment; ${} paid, not adjusted for inflation). \
             Most claims in one year: {}.",
            f.first_year.map(|y| y.to_string()).unwrap_or_default(),
            md::num(f.claims as f64, 0),
            md::num(f.paid_claims as f64, 0),
            md::num(f.total_paid, 0),
            f.peak_year
                .map(|y| format!("{y} ({})", f.peak_year_claims.unwrap_or(0)))
                .unwrap_or_else(|| "—".into())
        )));
    }
    if let Some(n) = &planning.nfip {
        blocks.push(p(&format!(
            "NFIP community {}: {}; current effective flood map {}; Community Rating System \
             class {}.",
            n.community_id,
            if n.participating {
                "participating"
            } else {
                "not participating"
            },
            n.current_map_date.clone().unwrap_or_else(|| "—".into()),
            n.crs_class.clone().unwrap_or_else(|| "none".into())
        )));
    }
    blocks.push(h("4. Social vulnerability"));
    blocks.push(ind_table("Social vulnerability", social));
    blocks.push(Block::Table(TableBlock {
        caption: Some(format!("People and climate-sensitive health, {town}")),
        headers: [
            "Indicator",
            "Percent",
            "About how many",
            "Massachusetts",
            "Source",
        ]
        .map(String::from)
        .to_vec(),
        rows: population
            .demographics
            .iter()
            .chain(population.health.iter())
            .map(|i| {
                vec![
                    i.label.clone(),
                    i.percent.map(|v| format!("{v}%")).unwrap_or_default(),
                    i.count_text(),
                    i.massachusetts.map(|v| format!("{v}%")).unwrap_or_default(),
                    i.source.clone(),
                ]
            })
            .collect(),
    }));
    if let Some(e) = &population.electricity_dependent {
        blocks.push(p(&format!(
            "HHS emPOWER: about {} Medicare beneficiaries rely on electricity-dependent medical \
             equipment ({}).",
            md::num(e.electricity_dependent, 0),
            e.method
        )));
    }
    blocks.push(h("5. Planning status"));
    if let Some(m) = &planning.mvp {
        blocks.push(p(&format!(
            "MVP region: {}. Planning report(s) on mass.gov: {}.{}{}",
            m.region.clone().unwrap_or_else(|| "not listed".into()),
            if m.planning_reports.is_empty() {
                "none posted".into()
            } else {
                m.planning_reports.join("; ")
            },
            if m.designated_through_prior_work {
                " Designated through prior work."
            } else {
                ""
            },
            if m.mvp2_fy24_pilot {
                " In the FY24 MVP 2.0 pilot."
            } else {
                ""
            }
        )));
    }
    blocks.push(Block::Table(TableBlock {
        caption: Some("FEMA hazard mitigation plans".into()),
        headers: ["Plan", "Type", "Status", "Approved", "Expires"]
            .map(String::from)
            .to_vec(),
        rows: if planning.hazard_mitigation_plans.is_empty() {
            vec![vec![
                "None listed".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]]
        } else {
            planning
                .hazard_mitigation_plans
                .iter()
                .map(|x| {
                    vec![
                        x.title.clone(),
                        x.plan_type.clone().unwrap_or_default(),
                        x.status.clone(),
                        x.approval_date.clone().unwrap_or_default(),
                        x.expiration_date.clone().unwrap_or_default(),
                    ]
                })
                .collect()
        },
    }));
    if !planning.flags.is_empty() {
        blocks.push(list(planning.flags.clone()));
    }
    blocks.push(list(
        planning
            .notes
            .iter()
            .map(|n| format!("{} ({})", n.text, n.citation))
            .collect(),
    ));
    blocks.push(h("6. Health effects of heat and flooding"));
    for hh in hazard_health {
        blocks.push(Block::Heading {
            level: 3,
            text: hh.hazard.clone(),
        });
        blocks.push(list(
            hh.health_effects
                .iter()
                .chain(hh.at_risk.iter())
                .map(|x| format!("{} ({})", x.text, x.citation))
                .collect(),
        ));
    }
    blocks.push(h("7. Definitions and sources"));
    blocks.push(list(
        definitions
            .iter()
            .map(|d| format!("{}: {} Vintage: {}.", d.label, d.definition, d.vintage))
            .collect(),
    ));
    blocks.push(list(
        c.sources
            .values()
            .map(|s| format!("{} — {} ({})", s.title, s.publisher, s.url))
            .collect(),
    ));
    Document {
        title: format!("Climate and health vulnerability profile — {town}"),
        subtitle: Some("For MVP and hazard mitigation planning".into()),
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

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn months_until_counts_whole_months() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        assert_eq!(months_until(d, "2027-11-11"), Some(12));
        assert_eq!(months_until(d, "2027-11-10"), Some(11));
        assert_eq!(months_until(d, "2026-12-01"), Some(0));
        assert_eq!(months_until(d, "2026-10-01"), Some(-1));
        assert_eq!(months_until(d, "2025-11-11"), Some(-12));
        assert_eq!(months_until(d, "1800-01-01"), None);
        assert_eq!(months_until(d, "not a date"), None);
    }

    #[test]
    fn content_parses() {
        let c: ClimateContent = content::load(CLIMATE_CONTENT).unwrap();
        for d in &c.indicators {
            assert!(c.sources.contains_key(&d.source), "{}", d.source);
        }
        for p in c.planning.iter().chain(&c.method_notes) {
            assert!(c.sources.contains_key(&p.source), "{}", p.source);
        }
        assert!(c.rules.text.contains("Tool default"));
    }

    #[tokio::test]
    async fn profiles_a_coastal_town_offline_and_writes_a_document() {
        let (dir, c) = ctx();
        let env = climate_health_vulnerability_profile(
            &c,
            ClimateProfileArgs {
                area: Some("Scituate".into()),
                document: Some(true),
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        let f = d.flood_history.as_ref().unwrap();
        assert!(f.claims > 1000);
        assert!(
            d.most_important
                .iter()
                .any(|v| v.indicator == "nfip_claims")
        );
        assert_eq!(d.heat.len(), 3);
        assert!(
            d.heat
                .iter()
                .all(|h| h.value.is_some() && h.massachusetts.is_some())
        );
        assert!(!d.heat_zips.is_empty());
        // Offline: plans and the Community Status Book come from the snapshot.
        assert_eq!(d.planning.plans_mode, "snapshot");
        assert!(!d.planning.hazard_mitigation_plans.is_empty());
        let n = d.planning.nfip.as_ref().unwrap();
        assert!(n.participating);
        assert_eq!(n.mode, "snapshot");
        assert!(
            d.planning
                .mvp
                .as_ref()
                .unwrap()
                .designated_through_prior_work
        );
        assert!(d.population.health.iter().any(|h| h.id == "CASTHMA"));
        assert_eq!(d.hazard_health.len(), 2);
        let file = d.document_file.as_ref().unwrap();
        assert!(dir.path().join("data").join(file).exists());
        assert!(env.summary.contains("Most important vulnerabilities"));
        assert!(env.caveats[0].contains("Tool default"));
        assert!(env.notice.is_some());
    }

    #[tokio::test]
    async fn an_urban_town_flags_heat_and_social_vulnerability() {
        let (_d, c) = ctx();
        let env = climate_health_vulnerability_profile(
            &c,
            ClimateProfileArgs {
                area: Some("Chelsea".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let ids: Vec<&str> = env
            .data
            .most_important
            .iter()
            .map(|v| v.indicator.as_str())
            .collect();
        assert!(ids.contains(&"impervious_pct"), "{ids:?}");
        assert!(ids.contains(&"tree_canopy_pct"), "{ids:?}");
        assert!(ids.contains(&"ej_population_pct"), "{ids:?}");
        assert!(env.data.most_important.len() <= 8);
        // Ranked by score.
        let scores: Vec<f64> = env.data.most_important.iter().map(|v| v.score).collect();
        assert!(scores.windows(2).all(|w| w[0] >= w[1]));
        assert!(env.data.document_file.is_none());
    }

    #[tokio::test]
    async fn refuses_counties_and_the_state() {
        let (_d, c) = ctx();
        for a in ["Bristol County", "Massachusetts"] {
            assert!(
                climate_health_vulnerability_profile(
                    &c,
                    ClimateProfileArgs {
                        area: Some(a.into()),
                        ..Default::default()
                    }
                )
                .await
                .is_err()
            );
        }
    }

    #[test]
    fn plans_map_to_towns_by_community_place_or_title() {
        let (_d, c) = ctx();
        let communities: BTreeMap<String, String> =
            [("250133".to_string(), "Agawam".to_string())].into();
        let plan =
            |cid: Option<&str>, place: Option<&str>, title: &str, county: &str| MitigationPlan {
                plan_id: "1".into(),
                geoid: None,
                place_name: place.map(String::from),
                county: Some(county.into()),
                jurisdiction_type: "Town".into(),
                community_id: cid.map(String::from),
                title: title.into(),
                plan_type: None,
                plan_status: "Approved".into(),
                jurisdiction_status: None,
                approval_date: None,
                expiration_date: None,
                adoption_date: None,
            };
        assert_eq!(
            plan_town(
                &c,
                &plan(Some("250133"), None, "x", "Hampden"),
                &communities
            )
            .as_deref(),
            Some("Agawam")
        );
        assert_eq!(
            plan_town(
                &c,
                &plan(None, Some("Amesbury Town city"), "x", "Essex"),
                &communities
            )
            .as_deref(),
            Some("Amesbury")
        );
        assert_eq!(
            plan_town(
                &c,
                &plan(
                    None,
                    None,
                    "The Town of Amherst Hazard Mitigation Plan",
                    "Hampshire"
                ),
                &communities
            )
            .as_deref(),
            Some("Amherst")
        );
        // "North Attleborough" must not also match "Attleboro".
        assert_eq!(
            plan_town(
                &c,
                &plan(None, None, "Town of North Attleborough, MA Plan", "Bristol"),
                &communities
            )
            .as_deref(),
            Some("North Attleborough")
        );
    }
}
