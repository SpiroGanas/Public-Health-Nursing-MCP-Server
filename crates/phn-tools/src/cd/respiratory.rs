//! CD-09 `get_respiratory_and_wastewater_surveillance`: a weekly briefing on
//! COVID-19, influenza, and RSV in Massachusetts from CDC data, with
//! wastewater levels by sampling site.
//!
//! Sources (data.cdc.gov, live → cache → bundled snapshot):
//! * NHSN hospital admissions, levels, and rates (`vdzy-6i9v`), MA and USA.
//! * NSSP emergency department visits and CDC trend (`rdmq-nq56`), state level.
//! * NREVSS test positivity: SARS-CoV-2 (`gvsb-yw6g`) and RSV (`3cxc-4k8q`)
//!   for HHS Region 1, and national influenza (`seuz-s2cv`).
//! * Acute respiratory illness activity level by state (`f3zz-zga5`).
//! * NWSS wastewater viral activity levels by site (`atcp-73re`).
//! * MWRA Deer Island Biobot series (bundled, final; ended July 2026).
//!
//! DPH's respiratory illness dashboard is linked, not scraped.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::socrata::{self, SocrataQuery, SocrataRows};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::surveillance::{fallback_caveat, snapshot_fallback};
use crate::context::Context;
use crate::md;

const TTL_SECS: u64 = 6 * 3600;
const RESP_SNAPSHOT: &str = "cdc-respiratory-ma";
const NWSS_SNAPSHOT: &str = "cdc-nwss-ma";
const MWRA_SNAPSHOT: &str = "mwra-deer-island-biobot";
const MWRA_FILE: &str = "mwra_biobot.csv";
/// DPH's respiratory illness dashboard (hospital admissions and ED visits).
pub const DPH_DASHBOARD_URL: &str =
    "https://www.mass.gov/info-details/respiratory-illness-reporting";
const CDC_RESP_URL: &str = "https://www.cdc.gov/respiratory-viruses/data/index.html";
const CDC_NWSS_URL: &str = "https://www.cdc.gov/wastewater/respiratory-viruses/state.html";
const MWRA_URL: &str = "https://www.mwra.com/biobot/biobotdata.htm";

const METHOD: &str = "For each measure the latest week is compared with the week before. \
    Direction (this server's rule): 'up' when the latest value is at least 10% higher than the \
    previous week, 'down' when at least 10% lower, otherwise 'stable'; 'up' when the previous \
    week was 0 and the latest is above 0. Levels are CDC's categories: NHSN admission levels \
    (Very Low to Very High, from CDC thresholds for each virus), NSSP trends (CDC's \
    Increasing / Decreasing / No Change), the ARI activity level, and wastewater viral activity \
    levels (WVAL categories, pathogen-specific). Test positivity is HHS Region 1 (New England) \
    for SARS-CoV-2 and RSV and national for influenza. The wastewater summary counts the sites \
    that reported in the latest week; its median WVAL is across those sites, which is how CDC \
    computes state values (median across treatment plants; \
    https://www.cdc.gov/wastewater/respiratory-viruses/state.html), though CDC's published state \
    level may use a different set of sites. Data lag is the days from the end of the latest \
    reported week to the briefing date.";

/// Arguments for `get_respiratory_and_wastewater_surveillance`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct RespiratorySurveillanceArgs {
    /// Pathogens to include: "COVID-19", "influenza", "RSV" (default all three).
    pub pathogens: Vec<String>,
    /// Weeks of history to return for each measure (default 4, 2-12).
    pub weeks: Option<usize>,
}

/// One week's value.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct WeekValue {
    pub week_ending: String,
    pub value: f64,
}

/// One measure for one pathogen.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Measure {
    /// What is measured, e.g. "Hospital admissions per 100,000 (NHSN)".
    pub measure: String,
    pub geography: String,
    pub week_ending: String,
    pub value: f64,
    pub unit: String,
    /// CDC category, when the source assigns one.
    pub level: Option<String>,
    /// CDC's own trend label (NSSP).
    pub cdc_trend: Option<String>,
    pub previous_week: Option<f64>,
    pub change_percent: Option<f64>,
    /// "up", "down", "stable", or "no previous week" (see method).
    pub direction: String,
    /// Same measure for the nation (or HHS region) in the same week, for context.
    pub comparison: Option<String>,
    /// Latest weeks, newest first.
    pub history: Vec<WeekValue>,
}

/// Wastewater for one pathogen, latest week per site.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct WastewaterSummary {
    pub pathogen_target: String,
    pub latest_week_ending: String,
    /// Sites that reported in the latest week.
    pub sites: usize,
    /// Sites whose latest data are from an earlier week (not in the counts or median).
    pub sites_with_older_data: usize,
    /// Sites by CDC WVAL category.
    pub by_category: BTreeMap<String, usize>,
    pub median_wval: Option<f64>,
    pub sites_up: usize,
    pub sites_down: usize,
}

/// One wastewater site and pathogen.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct WastewaterSite {
    /// CDC's anonymous site number.
    pub site: String,
    pub counties_served: String,
    pub population_served: Option<f64>,
    /// Who collects and tests the samples (State_Territory, CDC_Verily, WastewaterSCAN).
    pub sample_source: String,
    pub pathogen_target: String,
    pub week_ending: String,
    pub wval: Option<f64>,
    pub category: String,
    pub previous_wval: Option<f64>,
    pub direction: String,
}

/// Briefing for one pathogen.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct PathogenBriefing {
    pub pathogen: String,
    pub hospital_admissions: Option<Measure>,
    pub ed_visits: Option<Measure>,
    pub test_positivity: Option<Measure>,
    pub wastewater: Option<WastewaterSummary>,
}

/// MWRA Deer Island (Biobot) final values.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MwraFinal {
    pub last_sample_date: String,
    /// Last Southern system 7-day average, copies/mL.
    pub southern_7day_avg: Option<f64>,
    /// Date of the last Southern system 7-day average.
    pub southern_date: Option<String>,
    /// Last Northern system 7-day average, copies/mL.
    pub northern_7day_avg: Option<f64>,
    /// Date of the last Northern system 7-day average.
    pub northern_date: Option<String>,
    pub note: String,
}

/// When each source last reported.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Freshness {
    pub source: String,
    pub latest_week_ending: Option<String>,
    /// Days from the end of the latest week to the briefing date.
    pub lag_days: Option<i64>,
    pub mode: SourceMode,
}

/// Result of `get_respiratory_and_wastewater_surveillance`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RespiratoryBriefing {
    pub briefing_date: String,
    /// Massachusetts acute respiratory illness activity level (CDC).
    pub ari_activity_level: Option<String>,
    pub ari_week_ending: Option<String>,
    pub pathogens: Vec<PathogenBriefing>,
    pub wastewater_sites: Vec<WastewaterSite>,
    pub mwra_deer_island: Option<MwraFinal>,
    pub freshness: Vec<Freshness>,
    /// Dashboards to check (DPH is the Massachusetts source of record).
    pub dashboards: Vec<String>,
    pub method: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Pathogen {
    Covid,
    Flu,
    Rsv,
}

impl Pathogen {
    fn name(self) -> &'static str {
        match self {
            Self::Covid => "COVID-19",
            Self::Flu => "Influenza",
            Self::Rsv => "RSV",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match phn_core::text::normalize(s).as_str() {
            "covid" | "covid 19" | "sars cov 2" | "coronavirus" => Some(Self::Covid),
            "flu" | "influenza" | "influenza a" => Some(Self::Flu),
            "rsv" | "respiratory syncytial virus" => Some(Self::Rsv),
            _ => None,
        }
    }
    fn nhsn(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Covid => (
                "totalconfc19newadm",
                "totalconfc19newadmper100k",
                "totalconfc19newadmper100klevel",
            ),
            Self::Flu => (
                "totalconfflunewadm",
                "totalconfflunewadmper100k",
                "totalconfflunewadmper100klevel",
            ),
            Self::Rsv => (
                "totalconfrsvnewadm",
                "totalconfrsvnewadmper100k",
                "totalconfrsvnewadmper100klevel",
            ),
        }
    }
    fn nssp(self) -> (&'static str, &'static str) {
        match self {
            Self::Covid => ("percent_visits_covid", "ed_trends_covid"),
            Self::Flu => ("percent_visits_influenza", "ed_trends_influenza"),
            Self::Rsv => ("percent_visits_rsv", "ed_trends_rsv"),
        }
    }
    fn wastewater_target(self) -> &'static str {
        match self {
            Self::Covid => "SARS-CoV-2",
            Self::Flu => "Influenza A virus",
            Self::Rsv => "RSV",
        }
    }
}

/// Order CDC levels from highest to lowest for display.
fn category_rank(c: &str) -> usize {
    match c {
        "Very High" => 0,
        "High" => 1,
        "Moderate" => 2,
        "Low" => 3,
        "Very Low" => 4,
        _ => 5,
    }
}

fn date10(s: &str) -> String {
    s.chars().take(10).collect()
}

fn direction(latest: f64, prev: Option<f64>) -> (Option<f64>, String) {
    match prev {
        None => (None, "no previous week".into()),
        Some(p) if p <= 0.0 => (None, if latest > p { "up" } else { "stable" }.into()),
        Some(p) => {
            let ch = (latest - p) / p * 100.0;
            let d = if ch >= 10.0 {
                "up"
            } else if ch <= -10.0 {
                "down"
            } else {
                "stable"
            };
            (Some(ch), d.into())
        }
    }
}

/// Rows for one geography, newest first, keyed by date field.
fn series<'a>(
    rows: &'a [Map<String, Value>],
    geo_field: &str,
    geo: &str,
    date_field: &str,
) -> Vec<&'a Map<String, Value>> {
    let mut v: Vec<&Map<String, Value>> = rows
        .iter()
        .filter(|r| socrata::text(r, geo_field) == Some(geo))
        .filter(|r| socrata::text(r, date_field).is_some())
        .collect();
    v.sort_by(|a, b| {
        let da = socrata::text(a, date_field).unwrap_or("");
        let db = socrata::text(b, date_field).unwrap_or("");
        db.cmp(da).then_with(|| {
            socrata::text(b, "posted")
                .unwrap_or("")
                .cmp(socrata::text(a, "posted").unwrap_or(""))
        })
    });
    // One row per week (NREVSS repeats a week each time it is re-posted;
    // the latest post sorts first).
    v.dedup_by(|a, b| socrata::text(a, date_field) == socrata::text(b, date_field));
    v
}

#[allow(clippy::too_many_arguments)]
fn measure(
    rows: &[Map<String, Value>],
    geo_field: &str,
    geo: &str,
    geo_label: &str,
    date_field: &str,
    value_field: &str,
    level_field: Option<&str>,
    trend_field: Option<&str>,
    label: &str,
    unit: &str,
    weeks: usize,
) -> Option<Measure> {
    let s = series(rows, geo_field, geo, date_field);
    let pts: Vec<(String, f64)> = s
        .iter()
        .filter_map(|r| {
            Some((
                date10(socrata::text(r, date_field)?),
                socrata::num(r, value_field)?,
            ))
        })
        .collect();
    let (latest_date, latest) = pts.first()?.clone();
    let latest_row = s.iter().find(|r| {
        socrata::text(r, date_field).map(date10).as_deref() == Some(latest_date.as_str())
    })?;
    let prev = pts.get(1).map(|p| p.1);
    let (change, dir) = direction(latest, prev);
    Some(Measure {
        measure: label.into(),
        geography: geo_label.into(),
        week_ending: latest_date,
        value: latest,
        unit: unit.into(),
        level: level_field
            .and_then(|f| socrata::text(latest_row, f))
            .map(str::to_string),
        cdc_trend: trend_field
            .and_then(|f| socrata::text(latest_row, f))
            .map(str::to_string),
        previous_week: prev,
        change_percent: change,
        direction: dir,
        comparison: None,
        history: pts
            .iter()
            .take(weeks)
            .map(|(d, v)| WeekValue {
                week_ending: d.clone(),
                value: *v,
            })
            .collect(),
    })
}

fn value_on(
    rows: &[Map<String, Value>],
    geo_field: &str,
    geo: &str,
    date_field: &str,
    date: &str,
    field: &str,
) -> Option<f64> {
    series(rows, geo_field, geo, date_field)
        .into_iter()
        .find(|r| socrata::text(r, date_field).map(date10).as_deref() == Some(date))
        .and_then(|r| socrata::num(r, field))
}

struct Fetched {
    label: &'static str,
    rows: SocrataRows,
    snapshot_id: &'static str,
    date_field: &'static str,
}

async fn fetch(
    ctx: &Context,
    label: &'static str,
    q: SocrataQuery,
    snapshot_id: &'static str,
    file: &'static str,
    date_field: &'static str,
) -> Result<Fetched> {
    let fb = snapshot_fallback(ctx, snapshot_id, file, |t| Some(t.as_bytes().to_vec()));
    let rows = socrata::query(&ctx.http, ctx.cache.as_ref(), &q, TTL_SECS, Some(fb)).await?;
    Ok(Fetched {
        label,
        rows,
        snapshot_id,
        date_field,
    })
}

fn latest_week(rows: &[Map<String, Value>], date_field: &str) -> Option<String> {
    rows.iter()
        .filter_map(|r| socrata::text(r, date_field).map(date10))
        .max()
}

fn fmt(x: f64, unit: &str) -> String {
    match unit {
        "%" => format!("{}%", md::num(x, 2)),
        _ => md::num(x, 2),
    }
}

fn read_mwra(ctx: &Context) -> Option<(MwraFinal, Source)> {
    let (text, entry, _) = ctx.snapshots.file_text(MWRA_SNAPSHOT, MWRA_FILE).ok()?;
    let mut last_date: Option<String> = None;
    let mut south: Option<(f64, String)> = None;
    let mut north: Option<(f64, String)> = None;
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 5 {
            continue;
        }
        if let Ok(v) = f[3].parse::<f64>() {
            south = Some((v, f[0].to_string()));
        }
        if let Ok(v) = f[4].parse::<f64>() {
            north = Some((v, f[0].to_string()));
        }
        last_date = Some(f[0].to_string());
    }
    Some((
        MwraFinal {
            last_sample_date: last_date?,
            southern_7day_avg: south.as_ref().map(|x| x.0),
            southern_date: south.map(|x| x.1),
            northern_7day_avg: north.as_ref().map(|x| x.0),
            northern_date: north.map(|x| x.1),
            note: "MWRA's Biobot testing at Deer Island ended with samples collected through \
                   2026-07-27. MWRA reports that DPH is replacing it with its own testing \
                   program and plans a dashboard. Deer Island samples also go to WastewaterSCAN."
                .into(),
        },
        entry.to_source(),
    ))
}

/// Weekly respiratory virus and wastewater briefing for Massachusetts.
pub async fn get_respiratory_and_wastewater_surveillance(
    ctx: &Context,
    args: RespiratorySurveillanceArgs,
) -> Result<Envelope<RespiratoryBriefing>> {
    let mut pathogens: Vec<Pathogen> = Vec::new();
    for p in &args.pathogens {
        let parsed = Pathogen::parse(p).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "unknown pathogen '{p}'. Use COVID-19, influenza, or RSV."
            ))
        })?;
        if !pathogens.contains(&parsed) {
            pathogens.push(parsed);
        }
    }
    if pathogens.is_empty() {
        pathogens = vec![Pathogen::Covid, Pathogen::Flu, Pathogen::Rsv];
    }
    pathogens.sort();
    let weeks = args.weeks.unwrap_or(4);
    if !(2..=12).contains(&weeks) {
        return Err(PhnError::InvalidInput("weeks must be 2-12".into()));
    }
    let today = chrono::Local::now().date_naive();
    let since = format!(
        "{}T00:00:00",
        today - chrono::Duration::weeks(weeks as i64 + 3)
    );

    let (nhsn, nssp, covid_pos, rsv_pos, nat_pos, ari, wval) = tokio::join!(
        fetch(
            ctx,
            "NHSN hospital admissions",
            SocrataQuery::cdc("vdzy-6i9v")
                .and_where(format!("jurisdiction in ('MA','USA') AND weekendingdate >= '{since}'"))
                .order("weekendingdate DESC")
                .limit(500),
            RESP_SNAPSHOT,
            "resp_nhsn.json",
            "weekendingdate",
        ),
        fetch(
            ctx,
            "NSSP emergency department visits",
            SocrataQuery::cdc("rdmq-nq56")
                .select("week_end, geography, percent_visits_covid, percent_visits_influenza, percent_visits_rsv, percent_visits_combined, ed_trends_covid, ed_trends_influenza, ed_trends_rsv, buildnumber")
                .and_where(format!("geography in ('Massachusetts','United States') AND county = 'All' AND week_end >= '{since}'"))
                .order("week_end DESC")
                .limit(500),
            RESP_SNAPSHOT,
            "resp_nssp.json",
            "week_end",
        ),
        fetch(
            ctx,
            "NREVSS SARS-CoV-2 test positivity",
            SocrataQuery::cdc("gvsb-yw6g")
                .select("level, percent_pos, number_tested, posted, mmwrweek_end")
                .and_where(format!("level in ('Region 1','National') AND mmwrweek_end >= '{since}'"))
                .order("mmwrweek_end DESC, posted DESC")
                .limit(1_000),
            RESP_SNAPSHOT,
            "resp_nrevss_covid.json",
            "mmwrweek_end",
        ),
        fetch(
            ctx,
            "NREVSS RSV test positivity",
            SocrataQuery::cdc("3cxc-4k8q")
                .select("level, pcr_percent_positive, pcr_tests, posted, mmwrweek_end")
                .and_where(format!("level in ('Region 1','National') AND mmwrweek_end >= '{since}'"))
                .order("mmwrweek_end DESC, posted DESC")
                .limit(1_000),
            RESP_SNAPSHOT,
            "resp_nrevss_rsv.json",
            "mmwrweek_end",
        ),
        fetch(
            ctx,
            "National test positivity",
            SocrataQuery::cdc("seuz-s2cv")
                .and_where(format!("week_end >= '{since}'"))
                .order("week_end DESC")
                .limit(500),
            RESP_SNAPSHOT,
            "resp_nrevss_national.json",
            "week_end",
        ),
        fetch(
            ctx,
            "ARI activity level",
            SocrataQuery::cdc("f3zz-zga5")
                .select("week_end, geography, label")
                .and_where("geography in ('Massachusetts','National','United States')")
                .order("week_end DESC")
                .limit(50),
            RESP_SNAPSHOT,
            "resp_ari.json",
            "week_end",
        ),
        fetch(
            ctx,
            "NWSS wastewater viral activity levels",
            SocrataQuery::cdc("atcp-73re")
                .select("state_territory, counties_served, site, population_served, source, site_wval, site_wval_category, week_end, pathogen_target, date_updated")
                .and_where(format!("state_territory = 'Massachusetts' AND week_end >= '{since}'"))
                .order("week_end DESC")
                .limit(2_000),
            NWSS_SNAPSHOT,
            "resp_wval.json",
            "week_end",
        ),
    );
    // A source that fails entirely (no network, cache, or snapshot) is left
    // out with a caveat rather than failing the briefing.
    let mut caveats: Vec<String> = Vec::new();
    let mut ok: Vec<Fetched> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for r in [nhsn, nssp, covid_pos, rsv_pos, nat_pos, ari, wval] {
        match r {
            Ok(f) => ok.push(f),
            Err(e) => missing.push(e.to_string()),
        }
    }
    if ok.is_empty() {
        return Err(PhnError::Network(format!(
            "no CDC respiratory data could be retrieved: {}",
            missing.join("; ")
        )));
    }
    for m in &missing {
        caveats.push(format!(
            "A source could not be retrieved and is left out: {m}"
        ));
    }
    let get = |label: &str| {
        ok.iter()
            .find(|f| f.label == label)
            .map(|f| f.rows.rows.as_slice())
            .unwrap_or(&[])
    };
    let nhsn = get("NHSN hospital admissions");
    let nssp = get("NSSP emergency department visits");
    let covid_pos = get("NREVSS SARS-CoV-2 test positivity");
    let rsv_pos = get("NREVSS RSV test positivity");
    let nat_pos = get("National test positivity");
    let ari = get("ARI activity level");
    let wval = get("NWSS wastewater viral activity levels");

    // Wastewater sites: latest and previous week per site and pathogen.
    let mut by_site: BTreeMap<(String, String), Vec<&Map<String, Value>>> = BTreeMap::new();
    for r in wval {
        if let (Some(site), Some(p)) = (
            socrata::text(r, "site"),
            socrata::text(r, "pathogen_target"),
        ) {
            by_site
                .entry((p.to_string(), site.to_string()))
                .or_default()
                .push(r);
        }
    }
    let mut sites: Vec<WastewaterSite> = Vec::new();
    for ((p, site), mut rs) in by_site {
        rs.sort_by(|a, b| socrata::text(b, "week_end").cmp(&socrata::text(a, "week_end")));
        let r = rs[0];
        let wv = socrata::num(r, "site_wval");
        let prev = rs.get(1).and_then(|x| socrata::num(x, "site_wval"));
        let dir = match wv {
            Some(v) => direction(v, prev).1,
            None => "no value".into(),
        };
        sites.push(WastewaterSite {
            site,
            counties_served: socrata::text(r, "counties_served").unwrap_or("").into(),
            population_served: socrata::num(r, "population_served"),
            sample_source: socrata::text(r, "source").unwrap_or("").into(),
            pathogen_target: p,
            week_ending: socrata::text(r, "week_end").map(date10).unwrap_or_default(),
            wval: wv,
            category: socrata::text(r, "site_wval_category")
                .unwrap_or("No category")
                .into(),
            previous_wval: prev,
            direction: dir,
        });
    }

    let mut briefs = Vec::new();
    for p in &pathogens {
        let (_, rate, level) = p.nhsn();
        let mut adm = measure(
            nhsn,
            "jurisdiction",
            "MA",
            "Massachusetts",
            "weekendingdate",
            rate,
            Some(level),
            None,
            "Hospital admissions per 100,000 (NHSN)",
            "per 100,000",
            weeks,
        );
        if let Some(m) = adm.as_mut()
            && let Some(us) = value_on(
                nhsn,
                "jurisdiction",
                "USA",
                "weekendingdate",
                &m.week_ending,
                rate,
            )
        {
            let us_level = series(nhsn, "jurisdiction", "USA", "weekendingdate")
                .into_iter()
                .find(|r| {
                    socrata::text(r, "weekendingdate").map(date10).as_deref()
                        == Some(m.week_ending.as_str())
                })
                .and_then(|r| socrata::text(r, level))
                .unwrap_or("");
            m.comparison = Some(format!("United States {} ({us_level})", md::num(us, 2)));
        }
        let (pct, trend) = p.nssp();
        let mut ed = measure(
            nssp,
            "geography",
            "Massachusetts",
            "Massachusetts",
            "week_end",
            pct,
            None,
            Some(trend),
            "Emergency department visits (% of all visits, NSSP)",
            "%",
            weeks,
        );
        if let Some(m) = ed.as_mut()
            && let Some(us) = value_on(
                nssp,
                "geography",
                "United States",
                "week_end",
                &m.week_ending,
                pct,
            )
        {
            m.comparison = Some(format!("United States {}%", md::num(us, 2)));
        }
        let pos = match p {
            Pathogen::Covid => {
                let mut m = measure(
                    covid_pos,
                    "level",
                    "Region 1",
                    "HHS Region 1 (New England)",
                    "mmwrweek_end",
                    "percent_pos",
                    None,
                    None,
                    "Test positivity (% of NAATs positive, NREVSS)",
                    "%",
                    weeks,
                );
                if let Some(m) = m.as_mut()
                    && let Some(n) = value_on(
                        covid_pos,
                        "level",
                        "National",
                        "mmwrweek_end",
                        &m.week_ending,
                        "percent_pos",
                    )
                {
                    m.comparison = Some(format!("National {}%", md::num(n, 2)));
                }
                m
            }
            Pathogen::Rsv => {
                let mut m = measure(
                    rsv_pos,
                    "level",
                    "Region 1",
                    "HHS Region 1 (New England)",
                    "mmwrweek_end",
                    "pcr_percent_positive",
                    None,
                    None,
                    "Test positivity (% of PCR tests positive, NREVSS)",
                    "%",
                    weeks,
                );
                if let Some(m) = m.as_mut()
                    && let Some(n) = value_on(
                        rsv_pos,
                        "level",
                        "National",
                        "mmwrweek_end",
                        &m.week_ending,
                        "pcr_percent_positive",
                    )
                {
                    m.comparison = Some(format!("National {}%", md::num(n, 2)));
                }
                m
            }
            Pathogen::Flu => measure(
                nat_pos,
                "pathogen",
                "Influenza",
                "United States",
                "week_end",
                "percent_test_positivity",
                None,
                None,
                "Test positivity (% of tests positive, clinical labs)",
                "%",
                weeks,
            ),
        };
        let target = p.wastewater_target();
        let all_ps: Vec<&WastewaterSite> = sites
            .iter()
            .filter(|s| s.pathogen_target == target)
            .collect();
        let latest_ww = all_ps
            .iter()
            .map(|s| s.week_ending.clone())
            .max()
            .unwrap_or_default();
        let ps: Vec<&WastewaterSite> = all_ps
            .iter()
            .copied()
            .filter(|s| s.week_ending == latest_ww)
            .collect();
        let ww = if ps.is_empty() {
            None
        } else {
            let mut by_category = BTreeMap::new();
            for s in &ps {
                *by_category.entry(s.category.clone()).or_insert(0) += 1;
            }
            let mut vals: Vec<f64> = ps.iter().filter_map(|s| s.wval).collect();
            vals.sort_by(f64::total_cmp);
            let median = if vals.is_empty() {
                None
            } else if vals.len() % 2 == 1 {
                Some(vals[vals.len() / 2])
            } else {
                Some((vals[vals.len() / 2 - 1] + vals[vals.len() / 2]) / 2.0)
            };
            Some(WastewaterSummary {
                pathogen_target: target.into(),
                latest_week_ending: latest_ww.clone(),
                sites: ps.len(),
                sites_with_older_data: all_ps.len() - ps.len(),
                by_category,
                median_wval: median,
                sites_up: ps.iter().filter(|s| s.direction == "up").count(),
                sites_down: ps.iter().filter(|s| s.direction == "down").count(),
            })
        };
        briefs.push(PathogenBriefing {
            pathogen: p.name().into(),
            hospital_admissions: adm,
            ed_visits: ed,
            test_positivity: pos,
            wastewater: ww,
        });
    }

    let ari_ma = series(ari, "geography", "Massachusetts", "week_end")
        .into_iter()
        .next();
    let ari_level = ari_ma
        .and_then(|r| socrata::text(r, "label"))
        .map(str::to_string);
    let ari_week = ari_ma
        .and_then(|r| socrata::text(r, "week_end"))
        .map(date10);

    let mwra = read_mwra(ctx);

    // Freshness and sources.
    let mut freshness = Vec::new();
    let mut sources = Vec::new();
    let mut snapshot_used: Vec<&str> = Vec::new();
    for f in &ok {
        let latest = latest_week(&f.rows.rows, f.date_field);
        let lag = latest
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .map(|d| (today - d).num_days());
        freshness.push(Freshness {
            source: f.label.into(),
            latest_week_ending: latest.clone(),
            lag_days: lag,
            mode: f.rows.mode,
        });
        match f.rows.mode {
            SourceMode::Snapshot => snapshot_used.push(f.label),
            SourceMode::Cache if f.rows.fallback_reason.is_some() => {
                if let Some(c) = fallback_caveat(ctx, &f.rows, f.label, f.snapshot_id) {
                    caveats.push(c);
                }
            }
            _ => {}
        }
        let dataset = f
            .rows
            .url
            .split("/resource/")
            .nth(1)
            .and_then(|s| s.split('.').next())
            .unwrap_or("");
        let mut src = f.rows.source(
            format!("{} (data.cdc.gov {dataset})", f.label),
            "Centers for Disease Control and Prevention",
        );
        src.url = Some(format!("https://data.cdc.gov/d/{dataset}"));
        src.effective_date = latest;
        sources.push(src);
    }
    if !snapshot_used.is_empty() {
        let retrieved = ok
            .iter()
            .filter(|f| f.rows.mode == SourceMode::Snapshot)
            .map(|f| date10(&f.rows.as_of))
            .min()
            .unwrap_or_default();
        caveats.push(format!(
            "Live CDC data could not be reached for {}; the bundled snapshots (retrieved \
             {retrieved}) were used, so these figures may be weeks out of date. See the data \
             freshness table.",
            snapshot_used.join(", ")
        ));
    }
    if let Some((_, src)) = &mwra {
        sources.push(src.clone());
    }

    // Summary.
    let latest_all = freshness
        .iter()
        .filter_map(|f| f.latest_week_ending.clone())
        .max()
        .unwrap_or_default();
    let mut s = format!(
        "**Respiratory virus and wastewater briefing for Massachusetts** (CDC data as of \
         {today}; latest week ending {latest_all}).\n\n"
    );
    if let (Some(l), Some(w)) = (&ari_level, &ari_week) {
        s.push_str(&format!(
            "Acute respiratory illness activity (all causes, CDC): **{l}** (week ending {w}).\n\n"
        ));
    }
    let cell = |m: &Option<Measure>| -> String {
        match m {
            None => "no data".into(),
            Some(m) => {
                let mut t = fmt(m.value, &m.unit);
                if let Some(l) = &m.level {
                    t.push_str(&format!(", {l}"));
                }
                match &m.cdc_trend {
                    Some(tr) => t.push_str(&format!(", CDC trend: {tr}")),
                    None => {
                        let prev = m
                            .previous_week
                            .map(|p| fmt(p, &m.unit))
                            .unwrap_or_else(|| "–".into());
                        t.push_str(&format!(", {} (prev. {prev})", m.direction));
                    }
                }
                t
            }
        }
    };
    let rows: Vec<Vec<String>> = briefs
        .iter()
        .map(|b| {
            let ww = match &b.wastewater {
                None => "no MA sites".into(),
                Some(w) => {
                    let mut cats: Vec<(&String, &usize)> = w.by_category.iter().collect();
                    cats.sort_by_key(|(k, _)| category_rank(k));
                    let cats = cats
                        .iter()
                        .map(|(k, v)| format!("{v} {k}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let older = if w.sites_with_older_data > 0 {
                        format!(" ({} more with older data)", w.sites_with_older_data)
                    } else {
                        String::new()
                    };
                    format!(
                        "{} sites{older}: {cats}; median WVAL {}",
                        w.sites,
                        w.median_wval.map_or_else(|| "–".into(), |m| md::num(m, 1))
                    )
                }
            };
            vec![
                b.pathogen.clone(),
                cell(&b.hospital_admissions),
                cell(&b.ed_visits),
                cell(&b.test_positivity),
                ww,
            ]
        })
        .collect();
    s.push_str(&md::table(
        &[
            "Pathogen",
            "Hospital admissions per 100k (MA, NHSN)",
            "ED visits, % (MA, NSSP)",
            "Test positivity (Region 1; flu national)",
            "Wastewater (MA sites)",
        ],
        &rows,
    ));
    let site_rows: Vec<Vec<String>> = sites
        .iter()
        .filter(|w| {
            briefs.iter().any(|b| {
                b.wastewater
                    .as_ref()
                    .is_some_and(|x| x.pathogen_target == w.pathogen_target)
            })
        })
        .map(|w| {
            vec![
                w.pathogen_target.clone(),
                w.site.clone(),
                w.counties_served.clone(),
                w.population_served
                    .map_or_else(|| "–".into(), |p| md::num(p, 0)),
                w.week_ending.clone(),
                w.wval.map_or_else(|| "–".into(), |v| md::num(v, 1)),
                w.category.clone(),
                w.direction.clone(),
            ]
        })
        .collect();
    if !site_rows.is_empty() {
        s.push_str(
            "\nWastewater viral activity level by site (CDC NWSS; sites are anonymous):\n\n",
        );
        s.push_str(&md::table(
            &[
                "Target",
                "Site",
                "Counties served",
                "Population",
                "Week ending",
                "WVAL",
                "Level",
                "Direction",
            ],
            &site_rows,
        ));
    }
    if let Some((m, _)) = &mwra {
        s.push_str(&format!(
            "\nMWRA Deer Island (Biobot SARS-CoV-2): final samples {}; last 7-day averages \
             Southern system {} copies/mL ({}), Northern system {} copies/mL ({}). {}\n",
            m.last_sample_date,
            m.southern_7day_avg
                .map_or_else(|| "–".into(), |v| md::num(v, 0)),
            m.southern_date.as_deref().unwrap_or("–"),
            m.northern_7day_avg
                .map_or_else(|| "–".into(), |v| md::num(v, 0)),
            m.northern_date.as_deref().unwrap_or("–"),
            m.note
        ));
    }
    let fresh_rows: Vec<Vec<String>> = freshness
        .iter()
        .map(|f| {
            vec![
                f.source.clone(),
                f.latest_week_ending.clone().unwrap_or_else(|| "–".into()),
                f.lag_days.map_or_else(|| "–".into(), |d| d.to_string()),
                f.mode.label().into(),
            ]
        })
        .collect();
    s.push_str("\nData freshness:\n\n");
    s.push_str(&md::table(
        &["Source", "Latest week ending", "Days old", "Mode"],
        &fresh_rows,
    ));
    s.push_str(&format!(
        "\nCheck DPH's respiratory illness dashboard for Massachusetts data of record: {DPH_DASHBOARD_URL}\n"
    ));

    caveats.splice(
        0..0,
        [
            "Surveillance data are provisional and revised weekly; the latest week is often \
             incomplete. DPH's dashboard is the Massachusetts source of record."
                .to_string(),
            "NHSN admissions depend on hospital reporting (see the percent of hospitals \
             reporting in the NHSN data); NSSP covers participating emergency departments; \
             NREVSS reflects participating laboratories in HHS Region 1 (the six New England \
             states)."
                .to_string(),
            "Wastewater levels reflect the population served by each sewershed, not individual \
             towns; CDC identifies sites only by number, counties, and population served. The \
             influenza target is influenza A."
                .to_string(),
        ],
    );
    let dashboards = vec![
        DPH_DASHBOARD_URL.to_string(),
        CDC_RESP_URL.to_string(),
        CDC_NWSS_URL.to_string(),
        MWRA_URL.to_string(),
    ];
    let env = Envelope::new(
        s,
        RespiratoryBriefing {
            briefing_date: today.to_string(),
            ari_activity_level: ari_level,
            ari_week_ending: ari_week,
            pathogens: briefs,
            wastewater_sites: sites,
            mwra_deer_island: mwra.map(|(m, _)| m),
            freshness,
            dashboards,
            method: METHOD.into(),
        },
    )
    .sources(sources)
    .source(Source::computed(
        "Week-over-week direction and wastewater site summary (method in data.method)",
        "Public Health Nursing MCP Server",
    ))
    .caveats(caveats)
    .privacy(Privacy {
        notes: vec!["Public aggregate surveillance data; no local data were used.".into()],
        ..Privacy::none()
    });
    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_rule() {
        assert_eq!(direction(1.2, Some(1.0)).1, "up");
        assert_eq!(direction(0.95, Some(1.0)).1, "stable");
        assert_eq!(direction(0.8, Some(1.0)).1, "down");
        assert_eq!(direction(0.5, Some(0.0)).1, "up");
        assert_eq!(direction(0.0, Some(0.0)).1, "stable");
        assert_eq!(direction(1.0, None).1, "no previous week");
    }

    #[test]
    fn nrevss_reposts_are_deduplicated() {
        let rows: Vec<Map<String, Value>> = serde_json::from_str(
            r#"[
            {"level":"Region 1","percent_pos":"3.1","posted":"2026-11-04T10:00:00","mmwrweek_end":"2026-10-31T00:00:00.000"},
            {"level":"Region 1","percent_pos":"3.4","posted":"2026-11-05T10:00:00","mmwrweek_end":"2026-10-31T00:00:00.000"},
            {"level":"Region 1","percent_pos":"2.0","posted":"2026-10-29T10:00:00","mmwrweek_end":"2026-10-24T00:00:00.000"}
            ]"#,
        )
        .unwrap();
        let m = measure(
            &rows,
            "level",
            "Region 1",
            "R1",
            "mmwrweek_end",
            "percent_pos",
            None,
            None,
            "x",
            "%",
            4,
        )
        .unwrap();
        assert_eq!(m.value, 3.4);
        assert_eq!(m.previous_week, Some(2.0));
        assert_eq!(m.direction, "up");
        assert_eq!(m.history.len(), 2);
        assert_eq!(m.week_ending, "2026-10-31");
    }

    #[test]
    fn pathogens_parse() {
        assert_eq!(Pathogen::parse("Covid-19"), Some(Pathogen::Covid));
        assert_eq!(Pathogen::parse("flu"), Some(Pathogen::Flu));
        assert_eq!(Pathogen::parse("RSV"), Some(Pathogen::Rsv));
        assert_eq!(Pathogen::parse("measles"), None);
    }
}
