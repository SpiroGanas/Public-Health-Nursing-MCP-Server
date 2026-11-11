//! ENV-03 `drinking_water_quality_lookup`: public water systems serving a
//! Massachusetts town (EPA SDWIS), their health-based violations, lead and
//! copper 90th percentiles, and MassDEP PFAS6 results, with what residents can
//! do and private well testing guidance.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::census::AcsSnapshot;
use phn_data::water::{
    self, Codes, DatasetStatus, LeadCopper, Pfas6Result, SdwisSnapshot, Violation, WaterSystem,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::area::{self, AreaLevel};
use crate::epr::content::{self, Cited, CitedOut, Confidence, DocSource};
use crate::md;

/// Content id of `content/env/drinking_water.yaml`.
pub const DRINKING_WATER: &str = "env-drinking-water";

// ------------------------------------------------------------------ content

#[derive(Debug, Clone, Deserialize)]
pub struct Threshold {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TenYear {
    pub section: String,
    pub source: String,
    pub confidence: Confidence,
    pub note: String,
    pub parameters: Vec<WellParameter>,
}

/// A parameter in the every-10-years well test list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WellParameter {
    pub parameter: String,
    /// The level above (or outside) which MassDEP suggests follow-up.
    pub limit: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemList {
    pub section: String,
    pub source: String,
    pub confidence: Confidence,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TownList {
    pub text: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
    pub towns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrivateWellContent {
    pub overview: Vec<Cited>,
    pub annual: Vec<Cited>,
    pub every_ten_years: TenYear,
    pub pfas: Vec<Cited>,
    pub more_often: ItemList,
    pub arsenic_uranium: TownList,
    pub bedrock_lead: TownList,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShareDefault {
    pub value: f64,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolDefaults {
    pub well_reliance_share: ShareDefault,
    pub default_years: u32,
    pub max_years: u32,
}

/// A labeled link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Link {
    pub label: String,
    pub url: String,
}

/// `content/env/drinking_water.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct DrinkingWaterContent {
    pub sources: BTreeMap<String, DocSource>,
    pub thresholds: Vec<Threshold>,
    pub pfas_rules: Vec<Cited>,
    pub lead_copper_rules: Vec<Cited>,
    pub public_water_actions: Vec<Cited>,
    pub pfas_actions: Vec<Cited>,
    pub lead_actions: Vec<Cited>,
    pub private_wells: PrivateWellContent,
    pub tool_defaults: ToolDefaults,
    pub links: Vec<Link>,
}

impl DrinkingWaterContent {
    /// A threshold value by id.
    pub fn threshold(&self, id: &str) -> Result<f64> {
        self.thresholds
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.value)
            .ok_or_else(|| PhnError::Internal(format!("drinking water threshold {id} missing")))
    }
}

// --------------------------------------------------------------------- args

/// Arguments for `drinking_water_quality_lookup`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DrinkingWaterArgs {
    /// A Massachusetts city or town. Default: your saved town.
    pub town: Option<String>,
    /// A public water system ID (PWSID such as "MA2002000") or part of a system's name. Shows
    /// only that system (its town is used for context).
    pub system: Option<String>,
    /// Years of violation history to show (default 5, up to 10).
    pub years: Option<u32>,
    /// Also list non-community systems with their own supply: schools, child care, and
    /// workplaces (non-transient) and restaurants, campgrounds, and similar (transient).
    /// Default false (community systems only; non-community systems are counted).
    pub include_non_community: Option<bool>,
    /// Include private well testing guidance. Default: included when part of the town appears
    /// to rely on private wells (or the town has no community system).
    pub private_wells: Option<bool>,
}

// ------------------------------------------------------------------- output

/// A violation as reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViolationOut {
    /// Compliance period begin and end (YYYY-MM-DD).
    pub begin: String,
    pub end: Option<String>,
    /// Contaminant or rule the violation concerns.
    pub contaminant: String,
    /// SDWIS violation type.
    pub violation: String,
    /// MCL, MRDL, treatment technique, monitoring/reporting, or other.
    pub category: String,
    pub rule: String,
    /// "Returned to compliance", "Open", "Known", or as coded.
    pub status: String,
    pub returned_to_compliance: Option<String>,
    /// The measured level, when the violation records one.
    pub measured: Option<String>,
    pub public_notice_tier: Option<u8>,
}

/// The latest lead or copper 90th percentile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeadCopperOut {
    pub period_start: String,
    pub period_end: String,
    pub ninetieth_percentile_mg_l: f64,
    pub action_level_mg_l: f64,
    pub above_action_level: bool,
}

/// PFAS6 results for one system's finished water.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PfasSummary {
    /// Results in the data (finished water, about the last 24 months).
    pub results: usize,
    pub sampling_locations: usize,
    pub latest_date: String,
    /// Highest result on the latest date (ng/L); None when every result that day was ND.
    pub latest_highest_ng_l: Option<f64>,
    /// Highest single result in the 12 months before the data date (ng/L); None when all ND.
    pub highest_12_months_ng_l: Option<f64>,
    /// Results above 20 ng/L in those 12 months.
    pub results_above_20_12_months: usize,
    /// The latest calendar quarter with results ("2026-Q3").
    pub latest_quarter: String,
    /// The highest location average in that quarter (ND counted as 0, a tool default).
    pub latest_quarter_highest_location_average_ng_l: f64,
    /// That average is above the Massachusetts standard of 20 ng/L.
    pub latest_quarter_above_standard: bool,
}

/// One public water system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemReport {
    pub pwsid: String,
    pub name: String,
    pub system_type: String,
    /// "Community", "Non-transient non-community (schools, workplaces)", or "Transient non-community".
    pub type_label: String,
    pub population_served: Option<u64>,
    pub service_connections: Option<u64>,
    /// Ground water, surface water, ... and whether it is purchased.
    pub source: String,
    pub purchases_water: bool,
    pub wholesaler: bool,
    pub school_or_child_care: bool,
    /// "MassDEP" or "EPA Region 1 (tribal system)".
    pub primacy: String,
    /// Health-based violations whose compliance period began in the window.
    pub health_based_violations: Vec<ViolationOut>,
    /// Other (monitoring, reporting, notice) violations in the window, by category.
    pub other_violations: BTreeMap<String, usize>,
    /// Violations in the window with no return to compliance recorded.
    pub unresolved_violations: usize,
    pub lead: Option<LeadCopperOut>,
    pub copper: Option<LeadCopperOut>,
    pub pfas6: Option<PfasSummary>,
    /// Plain-language findings that need attention.
    pub flags: Vec<String>,
}

/// Private well testing guidance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrivateWellGuidance {
    pub overview: Vec<CitedOut>,
    pub test_every_year: Vec<CitedOut>,
    pub test_initially_and_every_10_years: Vec<WellParameter>,
    pub ten_year_note: String,
    pub ten_year_citation: String,
    pub pfas: Vec<CitedOut>,
    pub test_more_often_when: Vec<String>,
    /// The town is on MassDEP's list of towns with a high or medium probability of arsenic
    /// and/or uranium above the standard in bedrock wells (USGS 2011).
    pub arsenic_uranium_town: bool,
    pub arsenic_uranium_note: String,
    /// The town is on MassDEP's list of towns with naturally occurring lead in some bedrock wells.
    pub bedrock_lead_town: bool,
    pub bedrock_lead_note: String,
}

/// Result of `drinking_water_quality_lookup`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DrinkingWaterResult {
    pub town: String,
    pub county: Option<String>,
    /// The system asked about, when one was.
    pub system_query: Option<String>,
    /// Where the EPA SDWIS data came from.
    pub sdwis: DatasetStatus,
    /// Where the MassDEP PFAS6 results came from.
    pub pfas6: DatasetStatus,
    /// Violations shown began on or after this date.
    pub window_start: String,
    pub systems: Vec<SystemReport>,
    /// Community systems listed under the town.
    pub community_systems: usize,
    /// Non-community systems in the town not listed (include_non_community = false).
    pub non_community_not_shown: usize,
    /// Sum of population served reported by the town's community systems.
    pub community_population_served: u64,
    /// Town population (ACS 5-year).
    pub town_population: Option<f64>,
    /// Community population served / town population.
    pub served_share: Option<f64>,
    /// Part of the town likely relies on private wells or a neighboring town's system.
    pub likely_private_wells: bool,
    pub private_well_guidance: Option<PrivateWellGuidance>,
    pub standards: Vec<CitedOut>,
    pub what_residents_can_do: Vec<CitedOut>,
    pub links: Vec<Link>,
}

// -------------------------------------------------------------------- logic

fn type_label(t: &str) -> String {
    match t {
        "CWS" => "Community".into(),
        "NTNCWS" => "Non-transient non-community (schools, workplaces)".into(),
        "TNCWS" => "Transient non-community (restaurants, campgrounds)".into(),
        other => other.into(),
    }
}

fn source_label(codes: &Codes, code: &str) -> String {
    codes
        .get("PRIMARY_SOURCE_CODE", code)
        .map(|d| d.to_lowercase())
        .unwrap_or_else(|| {
            if code.is_empty() {
                "not reported".into()
            } else {
                code.into()
            }
        })
}

/// Name a violation from SDWIS codes, falling back to the rule and category
/// when the violation code has no description (newer rule codes).
fn violation_out(codes: &Codes, v: &Violation) -> ViolationOut {
    let category = codes
        .get("VIOLATION_CATEGORY_CODE", &v.category)
        .map(str::to_string)
        .unwrap_or_else(|| match v.category.as_str() {
            "MON" => "Monitoring".into(),
            "RPT" => "Reporting".into(),
            c => c.to_string(),
        });
    let rule = codes
        .get("RULE_CODE", &v.rule)
        .map(str::to_string)
        .unwrap_or_else(|| v.rule.clone());
    let contaminant = codes
        .get("CONTAMINANT_CODE", &v.contaminant)
        .map(str::to_string)
        .unwrap_or_else(|| format!("contaminant code {}", v.contaminant));
    let violation = codes
        .get("VIOLATION_CODE", &v.code)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{rule} {category} (code {})", v.code));
    let status = codes
        .get("COMPLIANCE_STATUS_CODE", &v.status)
        .map(str::to_string)
        .unwrap_or_else(|| v.status.clone());
    let measured = v.measure.map(|m| {
        format!(
            "{} {}",
            md::num(m, if m < 1.0 { 4 } else { 2 }),
            v.unit.clone().unwrap_or_default()
        )
        .trim()
        .to_string()
    });
    ViolationOut {
        begin: v.begin.clone(),
        end: v.end.clone(),
        contaminant,
        violation,
        category,
        rule,
        status,
        returned_to_compliance: v.returned_to_compliance.clone(),
        measured,
        public_notice_tier: v.notice_tier,
    }
}

/// The latest 90th percentile of `contaminant` ("PB90" or "CU90").
pub fn latest_lead_copper(
    rows: &[&LeadCopper],
    contaminant: &str,
    action_level: f64,
) -> Option<LeadCopperOut> {
    rows.iter()
        .filter(|r| r.contaminant == contaminant)
        .max_by(|a, b| {
            (a.period_end.as_str(), a.period_start.as_str())
                .cmp(&(b.period_end.as_str(), b.period_start.as_str()))
        })
        .map(|r| LeadCopperOut {
            period_start: r.period_start.clone(),
            period_end: r.period_end.clone(),
            ninetieth_percentile_mg_l: r.value_mg_l,
            action_level_mg_l: action_level,
            above_action_level: r.value_mg_l > action_level,
        })
}

fn quarter(date: &str) -> Option<String> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    use chrono::Datelike;
    Some(format!("{}-Q{}", d.year(), (d.month() - 1) / 3 + 1))
}

/// Summarize finished-water PFAS6 results for one system. `as_of` is the data
/// date (today for live data, the snapshot date otherwise).
pub fn pfas_summary(results: &[&Pfas6Result], as_of: NaiveDate, mcl: f64) -> Option<PfasSummary> {
    if results.is_empty() {
        return None;
    }
    let latest_date = results.iter().map(|r| r.date.as_str()).max()?.to_string();
    let max_of = |it: &mut dyn Iterator<Item = &&Pfas6Result>| -> Option<f64> {
        it.filter_map(|r| r.result_ng_l).reduce(f64::max)
    };
    let latest_highest_ng_l = max_of(&mut results.iter().filter(|r| r.date == latest_date));
    let from = (as_of - chrono::Duration::days(365)).to_string();
    let recent: Vec<&&Pfas6Result> = results
        .iter()
        .filter(|r| r.date.as_str() >= from.as_str())
        .collect();
    let highest_12_months_ng_l = max_of(&mut recent.iter().copied());
    let results_above_20_12_months = recent
        .iter()
        .filter(|r| r.result_ng_l.is_some_and(|v| v > mcl))
        .count();
    let latest_quarter = quarter(&latest_date).unwrap_or_default();
    let mut by_loc: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
    for r in results
        .iter()
        .filter(|r| quarter(&r.date).as_deref() == Some(latest_quarter.as_str()))
    {
        let key = if r.location_code.is_empty() {
            r.location.as_str()
        } else {
            r.location_code.as_str()
        };
        let e = by_loc.entry(key).or_insert((0.0, 0));
        e.0 += r.result_ng_l.unwrap_or(0.0);
        e.1 += 1;
    }
    let q_avg = by_loc
        .values()
        .map(|(s, n)| s / *n as f64)
        .fold(0.0, f64::max);
    let locations: BTreeSet<&str> = results
        .iter()
        .map(|r| {
            if r.location_code.is_empty() {
                r.location.as_str()
            } else {
                r.location_code.as_str()
            }
        })
        .collect();
    Some(PfasSummary {
        results: results.len(),
        sampling_locations: locations.len(),
        latest_date,
        latest_highest_ng_l,
        highest_12_months_ng_l,
        results_above_20_12_months,
        latest_quarter,
        latest_quarter_highest_location_average_ng_l: (q_avg * 10.0).round() / 10.0,
        latest_quarter_above_standard: q_avg > mcl,
    })
}

/// Resolve `system` to a SDWIS system: a PWSID or a unique name match.
fn find_system<'a>(snap: &'a SdwisSnapshot, q: &str) -> Result<Option<&'a WaterSystem>> {
    if let Some(p) = water::normalize_pwsid(q) {
        return Ok(snap.systems.iter().find(|s| s.pwsid == p));
    }
    let ranked = phn_core::text::rank(q, &snap.systems, |s| vec![s.name.clone()], 0.86);
    match ranked.as_slice() {
        [] => Err(PhnError::NotFound(format!(
            "no active Massachusetts public water system matches '{q}'. Give the PWSID (for \
             example MA2002000) or a town."
        ))),
        [one] => Ok(Some(&snap.systems[one.index])),
        [a, b, ..] if (a.score - b.score).abs() > 1e-9 && a.score >= 0.95 => {
            Ok(Some(&snap.systems[a.index]))
        }
        many => Err(PhnError::InvalidInput(format!(
            "'{q}' matches more than one system: {}. Give the PWSID.",
            many.iter()
                .take(5)
                .map(|r| format!(
                    "{} ({}, {})",
                    snap.systems[r.index].name,
                    snap.systems[r.index].pwsid,
                    snap.systems[r.index].town
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ))),
    }
}

fn cite(items: &[Cited], c: &DrinkingWaterContent) -> Vec<CitedOut> {
    content::cite_all(items, &c.sources)
}

fn mode_note(label: &str, st: &DatasetStatus) -> Option<String> {
    match st.mode {
        SourceMode::Snapshot => Some(format!(
            "{label} COULD NOT BE REACHED LIVE ({}); the bundled snapshot retrieved {} is shown.",
            st.note.as_deref().unwrap_or("no reason given"),
            st.as_of
        )),
        SourceMode::Cache => Some(format!(
            "{label} data are from a cached copy fetched {}{}.",
            phn_data::fetch::local_date(&st.as_of),
            st.note
                .as_deref()
                .map(|n| format!(" ({n})"))
                .unwrap_or_default()
        )),
        _ => None,
    }
}

/// Public water systems, violations, lead and copper, and PFAS6 for a town.
pub async fn drinking_water_quality_lookup(
    ctx: &Context,
    args: DrinkingWaterArgs,
) -> Result<Envelope<DrinkingWaterResult>> {
    let c: DrinkingWaterContent = content::load(DRINKING_WATER)?;
    let snap = SdwisSnapshot::shared(&ctx.snapshots)?;
    let max_years = c.tool_defaults.max_years;
    let years = args.years.unwrap_or(c.tool_defaults.default_years);
    if years == 0 || years > max_years {
        return Err(PhnError::InvalidInput(format!(
            "years must be between 1 and {max_years}"
        )));
    }

    // Which town (and system).
    let system_query = args
        .system
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mut only_pwsid: Option<String> = None;
    let town_name: String = match system_query {
        Some(q) => {
            let found = find_system(snap, q)?;
            match found {
                Some(s) => {
                    only_pwsid = Some(s.pwsid.clone());
                    if s.town.is_empty() {
                        return Err(PhnError::InvalidInput(format!(
                            "{} ({}) is a wholesale system that serves many towns; ask about a \
                             town it supplies instead.",
                            s.name, s.pwsid
                        )));
                    }
                    s.town.clone()
                }
                None => {
                    // A PWSID not in the snapshot: use the town number in it.
                    let p = water::normalize_pwsid(q).unwrap_or_default();
                    let tid = water::pwsid_town_id(&p).ok_or_else(|| {
                        PhnError::NotFound(format!("no active public water system {p}"))
                    })?;
                    let m = ctx
                        .ma
                        .municipalities
                        .iter()
                        .find(|m| m.town_id == tid)
                        .ok_or_else(|| {
                            PhnError::NotFound(format!(
                                "{p} is not a Massachusetts public water system ID"
                            ))
                        })?;
                    only_pwsid = Some(p);
                    m.name.clone()
                }
            }
        }
        None => {
            let a = area::resolve(ctx, args.town.as_deref(), false)?;
            if a.level != AreaLevel::Town {
                return Err(PhnError::InvalidInput(format!(
                    "{} is not a city or town; name one Massachusetts city or town (water systems \
                     are listed town by town).",
                    a.name
                )));
            }
            a.name
        }
    };
    let muni = ctx
        .ma
        .town(&town_name)
        .ok_or_else(|| PhnError::NotFound(format!("town {town_name}")))?
        .clone();

    let tw = water::fetch_town(
        &ctx.http,
        ctx.cache.as_ref(),
        &ctx.snapshots,
        muni.town_id,
        &muni.name,
    )
    .await?;
    let codes = &snap.codes;
    let today = chrono::Local::now().date_naive();
    let window_start = {
        use chrono::Datelike;
        NaiveDate::from_ymd_opt(today.year() - years as i32, today.month(), 1)
            .unwrap_or(today)
            .to_string()
    };
    let pfas_as_of = match tw.pfas_status.mode {
        SourceMode::Snapshot => {
            NaiveDate::parse_from_str(&tw.pfas_status.as_of, "%Y-%m-%d").unwrap_or(today)
        }
        _ => today,
    };
    let lead_al = c.threshold("lead_action_level")?;
    let copper_al = c.threshold("copper_action_level")?;
    let pfas_mcl = c.threshold("ma_pfas6_mcl")?;

    let include_nc = args.include_non_community.unwrap_or(false) || only_pwsid.is_some();
    let mut shown: Vec<&WaterSystem> = tw
        .systems
        .iter()
        .filter(|s| match &only_pwsid {
            Some(p) => &s.pwsid == p,
            None => include_nc || s.system_type == "CWS",
        })
        .collect();
    if let Some(p) = &only_pwsid
        && shown.is_empty()
    {
        return Err(PhnError::NotFound(format!(
            "{p} is not an active public water system in {} (EPA SDWIS)",
            muni.name
        )));
    }
    shown.sort_by(|a, b| {
        (a.system_type != "CWS")
            .cmp(&(b.system_type != "CWS"))
            .then(b.population.cmp(&a.population))
            .then(a.name.cmp(&b.name))
    });
    let community: Vec<&WaterSystem> = tw
        .systems
        .iter()
        .filter(|s| s.system_type == "CWS")
        .collect();
    let non_community_not_shown = if include_nc {
        0
    } else {
        tw.systems.iter().filter(|s| s.system_type != "CWS").count()
    };

    let mut systems = Vec::new();
    for s in &shown {
        let vs: Vec<&Violation> = tw
            .violations
            .iter()
            .filter(|v| v.pwsid == s.pwsid && v.begin.as_str() >= window_start.as_str())
            .collect();
        let mut health: Vec<ViolationOut> = vs
            .iter()
            .filter(|v| v.health_based)
            .map(|v| violation_out(codes, v))
            .collect();
        health.sort_by(|a, b| b.begin.cmp(&a.begin));
        let mut other: BTreeMap<String, usize> = BTreeMap::new();
        for v in vs.iter().filter(|v| !v.health_based) {
            *other
                .entry(violation_out(codes, v).category)
                .or_insert(0) += 1;
        }
        let unresolved = vs
            .iter()
            .filter(|v| v.status != "R" && v.returned_to_compliance.is_none() && v.status != "I")
            .count();
        let lc: Vec<&LeadCopper> = tw
            .lead_copper
            .iter()
            .filter(|l| l.pwsid == s.pwsid)
            .collect();
        let lead = latest_lead_copper(&lc, "PB90", lead_al);
        let copper = latest_lead_copper(&lc, "CU90", copper_al);
        let pr: Vec<&Pfas6Result> = tw.pfas.iter().filter(|r| r.pwsid == s.pwsid).collect();
        let pfas6 = pfas_summary(&pr, pfas_as_of, pfas_mcl);
        let mut flags = Vec::new();
        if !health.is_empty() {
            flags.push(format!(
                "{} health-based violation(s) since {window_start}",
                health.len()
            ));
        }
        if unresolved > 0 {
            flags.push(format!(
                "{unresolved} violation(s) in the window with no return to compliance recorded"
            ));
        }
        if let Some(l) = &lead
            && l.above_action_level
        {
            flags.push(format!(
                "lead 90th percentile {} µg/L is above the 15 µg/L action level ({} to {})",
                md::num(l.ninetieth_percentile_mg_l * 1000.0, 1),
                l.period_start,
                l.period_end
            ));
        }
        if let Some(cu) = &copper
            && cu.above_action_level
        {
            flags.push(format!(
                "copper 90th percentile {} mg/L is above the 1.3 mg/L action level",
                md::num(cu.ninetieth_percentile_mg_l, 2)
            ));
        }
        if let Some(p) = &pfas6 {
            if p.latest_quarter_above_standard {
                flags.push(format!(
                    "PFAS6 average at a sampling location in {} was {} ng/L, above the 20 ng/L \
                     Massachusetts standard (informational; MassDEP determines compliance)",
                    p.latest_quarter,
                    md::num(p.latest_quarter_highest_location_average_ng_l, 1)
                ));
            } else if p.results_above_20_12_months > 0 {
                flags.push(format!(
                    "{} PFAS6 result(s) above 20 ng/L in the last 12 months of data",
                    p.results_above_20_12_months
                ));
            }
        }
        let purchases = s.source.ends_with('P');
        if pfas6.is_none() && purchases && s.system_type == "CWS" {
            flags.push(
                "buys its water; the seller's PFAS6 results apply to the purchased water".into(),
            );
        }
        systems.push(SystemReport {
            pwsid: s.pwsid.clone(),
            name: s.name.clone(),
            system_type: s.system_type.clone(),
            type_label: type_label(&s.system_type),
            population_served: s.population,
            service_connections: s.connections,
            source: source_label(codes, &s.source),
            purchases_water: purchases,
            wholesaler: s.wholesaler,
            school_or_child_care: s.school_or_child_care,
            primacy: if s.primacy == "MA" {
                "MassDEP".into()
            } else {
                "EPA Region 1 (tribal system)".into()
            },
            health_based_violations: health,
            other_violations: other,
            unresolved_violations: unresolved,
            lead,
            copper,
            pfas6,
            flags,
        });
    }

    // Population on community systems.
    let community_population_served: u64 = community.iter().filter_map(|s| s.population).sum();
    let geoid = phn_data::geography::MaGeographies::shared(&ctx.snapshots)
        .ok()
        .and_then(|g| g.town(&muni.name).map(|t| t.geoid.clone()));
    let acs = AcsSnapshot::shared(&ctx.snapshots).ok();
    let town_population = geoid
        .as_deref()
        .and_then(|g| acs.and_then(|a| a.row(g)))
        .and_then(|r| r.values.get("pop_total"))
        .map(|e| e.value)
        .filter(|v| *v > 0.0);
    let served_share = town_population.map(|p| community_population_served as f64 / p);
    let share_default = c.tool_defaults.well_reliance_share.value;
    let likely_private_wells =
        community.is_empty() || served_share.is_some_and(|s| s < share_default);
    let want_wells = args.private_wells.unwrap_or(likely_private_wells);
    let pw = &c.private_wells;
    let on = |list: &[String]| {
        list.iter()
            .any(|t| phn_core::text::normalize(t) == phn_core::text::normalize(&muni.name))
    };
    let private_well_guidance = want_wells.then(|| {
        let src = |key: &str| {
            c.sources
                .get(key)
                .map(|s| s.title.clone())
                .unwrap_or_else(|| key.to_string())
        };
        PrivateWellGuidance {
            overview: cite(&pw.overview, &c),
            test_every_year: cite(&pw.annual, &c),
            test_initially_and_every_10_years: pw.every_ten_years.parameters.clone(),
            ten_year_note: pw.every_ten_years.note.clone(),
            ten_year_citation: format!(
                "{}, {}",
                src(&pw.every_ten_years.source),
                pw.every_ten_years.section
            ),
            pfas: cite(&pw.pfas, &c),
            test_more_often_when: pw.more_often.items.clone(),
            arsenic_uranium_town: on(&pw.arsenic_uranium.towns),
            arsenic_uranium_note: pw.arsenic_uranium.text.clone(),
            bedrock_lead_town: on(&pw.bedrock_lead.towns),
            bedrock_lead_note: pw.bedrock_lead.text.clone(),
        }
    });

    // Standards and actions.
    let any_pfas_concern = systems.iter().any(|s| {
        s.pfas6.as_ref().is_some_and(|p| {
            p.latest_quarter_above_standard || p.results_above_20_12_months > 0
        })
    });
    let any_lead_concern = systems.iter().any(|s| {
        s.lead.as_ref().is_some_and(|l| l.above_action_level)
            || s.copper.as_ref().is_some_and(|l| l.above_action_level)
    });
    let mut standards = cite(&c.pfas_rules, &c);
    standards.extend(cite(&c.lead_copper_rules, &c));
    let mut actions = cite(&c.public_water_actions, &c);
    actions.extend(cite(&c.pfas_actions, &c));
    actions.extend(cite(&c.lead_actions, &c));

    // Summary.
    let mut s = format!("**Drinking water — {}**", muni.name);
    if let Some(q) = system_query {
        s.push_str(&format!(" (system: {q})"));
    }
    s.push_str("\n\n");
    if systems.is_empty() {
        s.push_str(&format!(
            "EPA SDWIS lists no active community water system under {}. Residents likely use \
             private wells or a neighboring town's system; ask the board of health.\n\n",
            muni.name
        ));
    } else {
        let rows: Vec<Vec<String>> = systems
            .iter()
            .map(|r| {
                let hb = r.health_based_violations.len();
                vec![
                    format!("{} ({})", r.name, r.pwsid),
                    if r.system_type == "CWS" {
                        "community".into()
                    } else {
                        r.system_type.clone()
                    },
                    r.population_served
                        .map(|p| md::num(p as f64, 0))
                        .unwrap_or_else(|| "—".into()),
                    r.source.clone(),
                    if hb == 0 {
                        "none".into()
                    } else {
                        format!(
                            "{hb}: {}",
                            r.health_based_violations
                                .iter()
                                .map(|v| v.contaminant.clone())
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    },
                    r.lead
                        .as_ref()
                        .map(|l| {
                            format!(
                                "{} µg/L ({}){}",
                                md::num(l.ninetieth_percentile_mg_l * 1000.0, 1),
                                l.period_end.chars().take(4).collect::<String>(),
                                if l.above_action_level { " ABOVE AL" } else { "" }
                            )
                        })
                        .unwrap_or_else(|| "—".into()),
                    r.pfas6
                        .as_ref()
                        .map(|p| {
                            format!(
                                "{} avg {} ng/L; latest {} {}",
                                p.latest_quarter,
                                md::num(p.latest_quarter_highest_location_average_ng_l, 1),
                                p.latest_date,
                                p.latest_highest_ng_l
                                    .map(|v| format!("{} ng/L", md::num(v, 1)))
                                    .unwrap_or_else(|| "ND".into())
                            )
                        })
                        .unwrap_or_else(|| {
                            if r.purchases_water {
                                "see seller".into()
                            } else {
                                "no results".into()
                            }
                        }),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "System",
                "Type",
                "Population",
                "Source",
                &format!("Health-based violations since {window_start}"),
                "Lead 90th pct (period end)",
                "PFAS6 (MA standard 20 ng/L)",
            ],
            &rows,
        ));
        s.push('\n');
    }
    if non_community_not_shown > 0 {
        s.push_str(&format!(
            "{non_community_not_shown} non-community system(s) (schools, child care, workplaces, \
             restaurants, campgrounds) are not listed; ask with include_non_community.\n\n"
        ));
    }
    let flagged: Vec<String> = systems
        .iter()
        .flat_map(|r| r.flags.iter().map(move |f| format!("- **{}:** {f}", r.name)))
        .collect();
    if !flagged.is_empty() {
        s.push_str("**Needs attention:**\n");
        s.push_str(&flagged.join("\n"));
        s.push_str("\n\n");
    }
    if only_pwsid.is_none() {
        match (town_population, served_share) {
            (Some(p), Some(sh)) => s.push_str(&format!(
                "Community systems listed under {} report serving {} people; the town has about \
                 {} residents ({}%).{}\n\n",
                muni.name,
                md::num(community_population_served as f64, 0),
                md::num(p, 0),
                md::num(sh * 100.0, 0),
                if likely_private_wells {
                    " Part of the town likely uses private wells or a neighboring town's system."
                } else {
                    ""
                }
            )),
            _ => {}
        }
    }
    if let Some(g) = &private_well_guidance {
        s.push_str("**Private wells (MassDEP):** test every year (spring) for total coliform bacteria and nitrate/nitrite; test initially and at least every 10 years for arsenic, lead, manganese, copper, fluoride, sodium, radionuclides (gross alpha, radon, radium, uranium), VOCs, and PFAS; use a MassDEP-certified lab; the board of health regulates private wells and may require testing.");
        if g.arsenic_uranium_town {
            s.push_str(&format!(
                " {} is on MassDEP's list of towns with a high or medium probability of arsenic and/or uranium above the standard in bedrock wells.",
                muni.name
            ));
        }
        if g.bedrock_lead_town {
            s.push_str(&format!(
                " Naturally occurring lead has been found in some bedrock wells in {}.",
                muni.name
            ));
        }
        s.push_str("\n\n");
    }
    s.push_str("**What residents can do:**\n");
    s.push_str("- Read the water system's annual Consumer Confidence Report and call the system with questions; public notices are required when there is a problem.\n");
    if any_pfas_concern {
        s.push_str("- PFAS6 above 20 ng/L: pregnant or nursing people, infants, and people with a weakened immune system should consider bottled water tested for PFAS for drinking, cooking, and infant formula; boiling does not remove PFAS (MassDEP).\n");
    }
    if any_lead_concern {
        s.push_str("- Lead: use cold water for drinking, cooking, and formula; flush the tap; use a filter certified for lead; ask about lead service lines; ask a pediatrician about a blood lead test for young children (EPA).\n");
    }
    s.push_str("\nStandards: Massachusetts PFAS6 20 ng/L (quarterly average of monthly samples); lead action level 15 µg/L and copper 1.3 mg/L (90th percentile; exceeding is not itself a violation); EPA's 2024 PFAS rule is being revised (see standards).\n");

    // Caveats.
    let mut caveats = Vec::new();
    if let Some(n) = mode_note("EPA SDWIS (Envirofacts)", &tw.sdwis) {
        caveats.push(n);
    }
    if let Some(n) = mode_note("MassDEP PFAS6 results (EEA Data Portal)", &tw.pfas_status) {
        caveats.push(n);
    }
    caveats.push(
        "EPA SDWIS is updated quarterly from MassDEP reports; recent violations and results can be \
         missing or revised. MassDEP and the water system are the authorities on current \
         compliance."
            .into(),
    );
    caveats.push(format!(
        "Systems are listed under the town in their PWSID. A system can serve parts of \
         neighboring towns, and part of {} may be served by a neighboring town's system that is \
         not listed here. Population served is reported by each system.",
        muni.name
    ));
    caveats.push(
        "PFAS6 figures are single finished-water results and simple averages computed by this \
         tool (results reported as not detected counted as 0 — a tool default); they are not \
         MassDEP compliance determinations."
            .into(),
    );
    if served_share.is_some_and(|x| x > 1.2) {
        caveats.push(
            "Reported population served is well above the town's population: a system may serve \
             other towns or report seasonal or wholesale customers."
                .into(),
        );
    }
    caveats.push(c.tool_defaults.well_reliance_share.note.clone());
    caveats.extend(content::review_caveats(&[DRINKING_WATER]));

    // Sources.
    let mut sources = vec![
        Source {
            title: "EPA Safe Drinking Water Information System (SDWIS/Fed): water systems, \
                    violations, lead and copper"
                .into(),
            publisher: "U.S. EPA (Envirofacts); data reported by MassDEP".into(),
            url: Some(match tw.sdwis.mode {
                SourceMode::Snapshot => "https://www.epa.gov/enviro/envirofacts-data-service-api".into(),
                _ => water::dmap_url("water_system", &water::town_prefixes(muni.town_id)),
            }),
            effective_date: (tw.sdwis.mode == SourceMode::Snapshot)
                .then(|| snap.entry.source_vintage.clone()),
            retrieved: Some(phn_data::fetch::local_date(&tw.sdwis.as_of)),
            mode: tw.sdwis.mode,
        },
        Source {
            title: "MassDEP PFAS6 results for finished drinking water (EEA Data Portal)".into(),
            publisher: "Massachusetts Department of Environmental Protection".into(),
            url: Some(water::EEA_PORTAL.into()),
            effective_date: None,
            retrieved: Some(phn_data::fetch::local_date(&tw.pfas_status.as_of)),
            mode: tw.pfas_status.mode,
        },
    ];
    if town_population.is_some()
        && let Some(a) = acs
    {
        sources.push(a.source());
    }
    let mut keys: Vec<&str> = vec![
        "massdep-pfas",
        "massdep-pfas-qrg",
        "epa-pfas-rescission",
        "epa-pfas-extension",
        "epa-lcr",
        "epa-lcri-qa",
        "massdep-pfas-consumer",
        "epa-lead-basics",
    ];
    if private_well_guidance.is_some() {
        keys.extend([
            "massdep-private-wells",
            "massdep-pfas-private",
            "massdep-private-labs",
        ]);
    }
    sources.extend(content::sources_for(keys, &c.sources));

    Ok(Envelope::new(
        s,
        DrinkingWaterResult {
            town: muni.name.clone(),
            county: Some(muni.county.clone()),
            system_query: system_query.map(str::to_string),
            sdwis: tw.sdwis.clone(),
            pfas6: tw.pfas_status.clone(),
            window_start,
            systems,
            community_systems: community.len(),
            non_community_not_shown,
            community_population_served,
            town_population,
            served_share: served_share.map(|x| (x * 1000.0).round() / 1000.0),
            likely_private_wells,
            private_well_guidance,
            standards,
            what_residents_can_do: actions,
            links: c.links.clone(),
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

    fn r(date: &str, loc: &str, v: Option<f64>) -> Pfas6Result {
        Pfas6Result {
            pwsid: "MA2002000".into(),
            date: date.into(),
            location_code: loc.into(),
            location: String::new(),
            result_ng_l: v,
        }
    }

    #[test]
    fn content_loads_with_thresholds() {
        let c: DrinkingWaterContent = content::load(DRINKING_WATER).unwrap();
        assert_eq!(c.threshold("ma_pfas6_mcl").unwrap(), 20.0);
        assert_eq!(c.threshold("lead_action_level").unwrap(), 0.015);
        assert_eq!(c.threshold("copper_action_level").unwrap(), 1.3);
        for x in c
            .pfas_rules
            .iter()
            .chain(&c.lead_copper_rules)
            .chain(&c.public_water_actions)
            .chain(&c.pfas_actions)
            .chain(&c.lead_actions)
            .chain(&c.private_wells.overview)
            .chain(&c.private_wells.annual)
            .chain(&c.private_wells.pfas)
        {
            assert!(c.sources.contains_key(&x.source), "{}", x.source);
        }
        let ma = phn_data::ma::MaReference::load(&phn_data::SnapshotStore::bundled()).unwrap();
        for t in c
            .private_wells
            .arsenic_uranium
            .towns
            .iter()
            .chain(&c.private_wells.bedrock_lead.towns)
        {
            assert!(ma.town(t).is_some(), "{t} is not a town");
        }
    }

    #[test]
    fn pfas_summary_uses_the_latest_quarter_and_counts_exceedances() {
        let a = [
            r("2026-07-10", "A", Some(18.0)),
            r("2026-08-10", "A", Some(30.0)),
            r("2026-09-10", "A", None),
            r("2026-09-10", "B", Some(4.0)),
            r("2025-01-15", "A", Some(50.0)),
        ];
        let refs: Vec<&Pfas6Result> = a.iter().collect();
        let as_of = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let p = pfas_summary(&refs, as_of, 20.0).unwrap();
        assert_eq!(p.latest_date, "2026-09-10");
        assert_eq!(p.latest_quarter, "2026-Q3");
        // Location A: (18 + 30 + 0) / 3 = 16.0; B: 4.0.
        assert_eq!(p.latest_quarter_highest_location_average_ng_l, 16.0);
        assert!(!p.latest_quarter_above_standard);
        assert_eq!(p.latest_highest_ng_l, Some(4.0));
        // The 50 ng/L result is older than 12 months.
        assert_eq!(p.highest_12_months_ng_l, Some(30.0));
        assert_eq!(p.results_above_20_12_months, 1);
        assert_eq!(p.sampling_locations, 2);
        // All not detected.
        let nd = [r("2026-09-10", "A", None)];
        let p = pfas_summary(&nd.iter().collect::<Vec<_>>(), as_of, 20.0).unwrap();
        assert_eq!(p.latest_highest_ng_l, None);
        assert_eq!(p.latest_quarter_highest_location_average_ng_l, 0.0);
        assert!(pfas_summary(&[], as_of, 20.0).is_none());
    }

    #[test]
    fn lead_uses_the_latest_period_and_the_action_level() {
        let rows = [
            LeadCopper {
                pwsid: "X".into(),
                contaminant: "PB90".into(),
                period_start: "2021-01-01".into(),
                period_end: "2023-12-31".into(),
                value_mg_l: 0.020,
            },
            LeadCopper {
                pwsid: "X".into(),
                contaminant: "PB90".into(),
                period_start: "2024-01-01".into(),
                period_end: "2026-12-31".into(),
                value_mg_l: 0.015,
            },
        ];
        let refs: Vec<&LeadCopper> = rows.iter().collect();
        let l = latest_lead_copper(&refs, "PB90", 0.015).unwrap();
        assert_eq!(l.period_end, "2026-12-31");
        // Exactly at the action level is not above it.
        assert!(!l.above_action_level);
        assert!(latest_lead_copper(&refs, "CU90", 1.3).is_none());
    }

    #[test]
    fn violations_are_named_from_codes_with_a_fallback() {
        let snap = SdwisSnapshot::shared(&phn_data::SnapshotStore::bundled()).unwrap();
        let v = Violation {
            pwsid: "X".into(),
            begin: "2024-01-01".into(),
            end: None,
            code: "02".into(),
            category: "MCL".into(),
            health_based: true,
            contaminant: "2950".into(),
            rule: "220".into(),
            status: "R".into(),
            returned_to_compliance: Some("2024-06-01".into()),
            measure: Some(0.0839),
            unit: Some("MG/L".into()),
            notice_tier: Some(2),
        };
        let o = violation_out(&snap.codes, &v);
        assert_eq!(o.contaminant, "TTHM");
        assert_eq!(o.violation, "MCL, Average");
        assert_eq!(o.status, "Returned to Compliance");
        assert_eq!(o.measured.as_deref(), Some("0.0839 MG/L"));
        // Revised Total Coliform Rule codes have no description: rule + category.
        let v2 = Violation {
            code: "2A".into(),
            category: "TT".into(),
            contaminant: "8000".into(),
            rule: "110".into(),
            ..v
        };
        let o = violation_out(&snap.codes, &v2);
        assert!(o.violation.contains("code 2A"), "{}", o.violation);
        assert_eq!(o.contaminant, "REVISED TOTAL COLIFORM RULE");
    }

    #[test]
    fn systems_are_found_by_pwsid_or_unique_name() {
        let snap = SdwisSnapshot::shared(&phn_data::SnapshotStore::bundled()).unwrap();
        assert_eq!(
            find_system(snap, "MA2002000").unwrap().unwrap().town,
            "Acton"
        );
        assert_eq!(
            find_system(snap, "ma-2002000").unwrap().unwrap().pwsid,
            "MA2002000"
        );
        assert!(find_system(snap, "zzzz nonexistent qqqq").is_err());
        // A generic word matches many systems: refused, not guessed.
        assert!(find_system(snap, "water department").is_err());
    }

    #[tokio::test]
    async fn offline_lookup_uses_snapshots_and_flags_wells() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = drinking_water_quality_lookup(
            &ctx,
            DrinkingWaterArgs {
                town: Some("Acton".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let d = &env.data;
        assert_eq!(d.town, "Acton");
        assert_eq!(d.sdwis.mode, SourceMode::Snapshot);
        assert!(env.caveats[0].contains("COULD NOT BE REACHED LIVE"));
        assert!(d.systems.iter().any(|s| s.pwsid == "MA2002000"));
        assert!(d.systems.iter().all(|s| s.system_type == "CWS"));
        assert!(env.notice.is_some());
        assert!(env.summary.contains("Drinking water — Acton"));

        // A town with no community system gets well guidance.
        let env = drinking_water_quality_lookup(
            &ctx,
            DrinkingWaterArgs {
                town: Some("Alford".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(env.data.likely_private_wells);
        assert!(env.data.private_well_guidance.is_some());
        assert!(env.summary.contains("Private wells"));

        // Counties and bad inputs are tool errors.
        assert!(
            drinking_water_quality_lookup(
                &ctx,
                DrinkingWaterArgs {
                    town: Some("Middlesex County".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
        assert!(
            drinking_water_quality_lookup(
                &ctx,
                DrinkingWaterArgs {
                    town: Some("Acton".into()),
                    years: Some(30),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
        // A system by PWSID.
        let env = drinking_water_quality_lookup(
            &ctx,
            DrinkingWaterArgs {
                system: Some("MA2002000".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(env.data.systems.len(), 1);
        assert_eq!(env.data.town, "Acton");
    }
}
