//! MH-03 `get_overdose_data`: overdose deaths for a Massachusetts town,
//! county, or the state, with trends and comparisons.
//!
//! * **Towns:** DPH's table of opioid-related overdose deaths of all intents by
//!   city/town of residence or occurrence, 2015-2022 (posted June 2023; bundled
//!   snapshot). Five-year average annual rates use the ACS 2020-2024 population
//!   and exact Poisson intervals; no rate is given for fewer than 5 deaths
//!   (DPH's rule in its 2024 report).
//! * **Counties:** CDC/NCHS provisional 12 month-ending drug overdose deaths
//!   by county of residence (data.cdc.gov `gb4e-yj24`; counts of 1-9
//!   suppressed by NCHS), live → cache → snapshot.
//! * **State and U.S.:** CDC/NCHS provisional 12 month-ending counts by drug
//!   class (data.cdc.gov `xkb8-kh2a`), live → cache → snapshot.
//! * DPH's statewide findings for 2024 from its March 2026 report.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::text::normalize;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::census::{AcsSnapshot, GeoLevel};
use phn_data::geography::MaGeographies;
use phn_data::socrata::{self, SocrataQuery, SocrataRows};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::cd::surveillance::{fallback_caveat, snapshot_fallback};
use crate::comms::content::{self, NamedSource, OVERDOSE_DATA};
use crate::context::Context;
use crate::md;

const TOWN_SNAPSHOT: &str = "ma-dph-opioid-deaths-town";
const TOWN_FILE: &str = "ma_opioid_deaths_town.csv";
const VSRR_SNAPSHOT: &str = "cdc-vsrr-overdose-ma";
const STATE_FILE: &str = "cdc_vsrr_overdose_ma_us.csv";
const COUNTY_FILE: &str = "cdc_vsrr_overdose_ma_counties.csv";
const STATE_DATASET: &str = "xkb8-kh2a";
const COUNTY_DATASET: &str = "gb4e-yj24";
/// CDC updates the provisional counts monthly; cache for a day.
const TTL_SECS: u64 = 24 * 3600;
const TOWN_YEARS: std::ops::RangeInclusive<i32> = 2015..=2022;
const RATE_YEARS: std::ops::RangeInclusive<i32> = 2018..=2022;
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Drug classes in CDC's provisional counts: (argument, CDC indicator, label).
const DRUGS: &[(&str, &str, &str)] = &[
    (
        "all",
        "Number of Drug Overdose Deaths",
        "all drug overdose deaths",
    ),
    ("opioids", "Opioids (T40.0-T40.4,T40.6)", "any opioid"),
    (
        "synthetic_opioids",
        "Synthetic opioids, excl. methadone (T40.4)",
        "synthetic opioids other than methadone (mainly fentanyl)",
    ),
    ("heroin", "Heroin (T40.1)", "heroin"),
    (
        "prescription_opioids",
        "Natural & semi-synthetic opioids (T40.2)",
        "natural and semi-synthetic opioids (such as oxycodone)",
    ),
    ("methadone", "Methadone (T40.3)", "methadone"),
    ("cocaine", "Cocaine (T40.5)", "cocaine"),
    (
        "stimulants",
        "Psychostimulants with abuse potential (T43.6)",
        "psychostimulants (mainly methamphetamine)",
    ),
];
const PCT_SPECIFIED: &str = "Percent with drugs specified";

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Cited {
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Rules {
    rate_min_deaths: u64,
    rate_min_deaths_source: String,
    county_suppression: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OverdoseContent {
    meta: ContentMeta,
    sources: BTreeMap<String, NamedSource>,
    definitions: Vec<Cited>,
    rules: Rules,
    findings_2024: Vec<Cited>,
}

static CONTENT: LazyLock<OverdoseContent> = LazyLock::new(|| {
    content::load(OVERDOSE_DATA).expect("content/bh/overdose_data.yaml matches its schema")
});

// ---------------------------------------------------------------- arguments and results

/// Arguments for `get_overdose_data`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct OverdoseArgs {
    /// A Massachusetts city or town, a county ("Hampden County"), or "Massachusetts".
    /// Default: your configured town, otherwise Massachusetts.
    pub area: Option<String>,
    /// Other towns or counties to compare with (at most 5).
    pub compare_with: Vec<String>,
    /// For towns: "residence" (deaths of residents, the default) or "occurrence" (deaths that
    /// happened in the town, including non-residents).
    pub basis: Option<String>,
    /// Drug class for the state and U.S. counts: "all" (default), "opioids",
    /// "synthetic_opioids" (fentanyl), "heroin", "prescription_opioids", "methadone",
    /// "cocaine", or "stimulants".
    pub drug: Option<String>,
    /// How many 12 month-ending periods of CDC counts to list (default 13, at most 60).
    pub months: Option<usize>,
}

/// Deaths in one year.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct YearDeaths {
    pub year: i32,
    pub deaths: u64,
}

/// A crude rate per 100,000 with its 95% exact Poisson interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RateEstimate {
    /// Period, e.g. "2018-2022 (average per year)".
    pub period: String,
    pub deaths: u64,
    /// Population used (per year).
    pub population: f64,
    pub population_source: String,
    /// Null when there are fewer than 5 deaths (DPH does not calculate a rate).
    pub rate_per_100k: Option<f64>,
    pub ci_lower: Option<f64>,
    pub ci_upper: Option<f64>,
    /// "fewer than 5 deaths: no rate", "fewer than 20 deaths: unstable", or empty.
    pub flags: Vec<String>,
}

/// DPH's yearly counts for one town.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownSeries {
    pub town: String,
    pub county: String,
    /// "residence" or "occurrence".
    pub basis: String,
    pub years: Vec<YearDeaths>,
    pub total_2015_2022: u64,
    /// Average annual rate for 2018-2022 (residence basis only).
    pub rate_2018_2022: Option<RateEstimate>,
    /// How the town's 2018-2022 rate compares with the state's.
    pub compared_with_state: Option<String>,
    /// 2022 compared with the 2019-2021 average.
    pub latest_year_note: String,
}

/// One 12 month-ending count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PeriodCount {
    /// Last month of the 12-month period (YYYY-MM).
    pub period_ending: String,
    /// Reported provisional count (null when suppressed or not shown).
    pub deaths: Option<f64>,
    /// CDC's count adjusted for reporting delays (state data only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted: Option<f64>,
    /// Suppressed by NCHS (1-9 deaths) or not shown for data quality.
    pub suppressed: bool,
}

/// A CDC provisional series for a county, the state, or the nation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProvisionalTrend {
    pub area: String,
    /// What is counted (e.g. "all drug overdose deaths").
    pub measure: String,
    pub latest: PeriodCount,
    pub year_earlier: Option<PeriodCount>,
    /// Percent change from the 12 months ending a year earlier (predicted values for the state
    /// and U.S., reported counts for counties).
    pub percent_change: Option<f64>,
    /// Crude rate per 100,000 for the latest 12 months (ACS 2020-2024 population).
    pub rate_per_100k: Option<f64>,
    /// Earlier periods, oldest first.
    pub series: Vec<PeriodCount>,
    /// Data quality notes from CDC.
    pub notes: Vec<String>,
}

/// One drug class in the latest Massachusetts 12 month-ending period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DrugRow {
    pub drug: String,
    pub cdc_indicator: String,
    pub massachusetts: Option<f64>,
    pub massachusetts_change_pct: Option<f64>,
    pub united_states: Option<f64>,
    pub united_states_change_pct: Option<f64>,
}

/// A cited statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CitedFinding {
    pub text: String,
    pub source: String,
    pub section: String,
}

/// Result of `get_overdose_data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OverdoseResult {
    pub area: String,
    /// "town", "county", or "state".
    pub level: String,
    /// DPH yearly counts for the town(s), 2015-2022.
    pub towns: Vec<TownSeries>,
    /// CDC provisional county counts.
    pub counties: Vec<ProvisionalTrend>,
    /// CDC provisional counts for Massachusetts and the United States for the chosen drug.
    pub state_and_us: Vec<ProvisionalTrend>,
    /// Massachusetts and U.S. counts by drug class for the latest period.
    pub by_drug: Vec<DrugRow>,
    /// DPH's statewide findings for 2024.
    pub dph_2024_findings: Vec<CitedFinding>,
    /// Where to find newer town-level data.
    pub newer_town_data: String,
    pub definitions: Vec<CitedFinding>,
    pub method: String,
}

// ---------------------------------------------------------------- DPH town table

#[derive(Debug, Clone)]
struct TownRow {
    basis: String,
    town: String,
    counts: Vec<u64>,
}

fn town_rows(ctx: &Context) -> Result<(Vec<TownRow>, Source)> {
    let (text, entry, _) = ctx.snapshots.file_text(TOWN_SNAPSHOT, TOWN_FILE)?;
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for r in rdr.records() {
        let r = r.map_err(|e| PhnError::Internal(format!("{TOWN_FILE}: {e}")))?;
        let counts = (2..r.len())
            .map(|i| r[i].trim().parse::<u64>().unwrap_or(0))
            .collect();
        rows.push(TownRow {
            basis: r[0].to_string(),
            town: r[1].to_string(),
            counts,
        });
    }
    Ok((rows, entry.to_source()))
}

/// DPH spells two towns differently from MassGIS.
fn dph_name_key(name: &str) -> String {
    let n = normalize(&name.replace('-', " "));
    match n.as_str() {
        "manchester by the sea" => "manchester".into(),
        "north attleborough" => "north attleboro".into(),
        _ => n,
    }
}

fn pop(acs: &AcsSnapshot, geoid: &str) -> Option<f64> {
    acs.row(geoid)
        .and_then(|r| r.values.get("pop_total"))
        .map(|e| e.value)
        .filter(|v| *v > 0.0)
}

fn rate_estimate(
    deaths: u64,
    population: f64,
    years: f64,
    period: &str,
    pop_source: &str,
) -> RateEstimate {
    let min = CONTENT.rules.rate_min_deaths;
    let mut flags = Vec::new();
    let (mut rate, mut lo, mut hi) = (None, None, None);
    if deaths < min {
        flags.push(format!("fewer than {min} deaths: no rate (DPH)"));
    } else {
        let r = phn_epi::rates::rate("rate", deaths, population * years, 100_000.0, 0.95, 0);
        rate = Some(phn_epi::rates::round_to(r.rate, 1));
        lo = Some(phn_epi::rates::round_to(r.lower, 1));
        hi = Some(phn_epi::rates::round_to(r.upper, 1));
        if deaths < phn_epi::rates::UNRELIABLE_COUNT {
            flags.push("fewer than 20 deaths: unstable, interpret with caution".into());
        }
    }
    RateEstimate {
        period: period.into(),
        deaths,
        population: population.round(),
        population_source: pop_source.into(),
        rate_per_100k: rate,
        ci_lower: lo,
        ci_upper: hi,
        flags,
    }
}

fn year_counts(row: &TownRow) -> Vec<YearDeaths> {
    TOWN_YEARS
        .zip(row.counts.iter())
        .map(|(year, d)| YearDeaths { year, deaths: *d })
        .collect()
}

fn sum_years(row: &TownRow, years: &std::ops::RangeInclusive<i32>) -> u64 {
    TOWN_YEARS
        .zip(row.counts.iter())
        .filter(|(y, _)| years.contains(y))
        .map(|(_, d)| *d)
        .sum()
}

// ---------------------------------------------------------------- CDC provisional counts

fn csv_to_socrata_json(text: &str) -> Option<Vec<u8>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers = rdr.headers().ok()?.clone();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.ok()?;
        let mut m = Map::new();
        for (h, v) in headers.iter().zip(rec.iter()) {
            if !v.is_empty() {
                m.insert(h.to_string(), Value::String(v.to_string()));
            }
        }
        out.push(Value::Object(m));
    }
    serde_json::to_vec(&out).ok()
}

async fn state_rows(ctx: &Context, since_year: i32) -> Result<SocrataRows> {
    let inds: Vec<String> = DRUGS
        .iter()
        .map(|d| socrata::soql_string(d.1))
        .chain(std::iter::once(socrata::soql_string(PCT_SPECIFIED)))
        .collect();
    let q = SocrataQuery::cdc(STATE_DATASET)
        .select("state,year,month,indicator,data_value,predicted_value,percent_complete,percent_pending_investigation,footnote_symbol,footnote")
        .and_where("state in ('MA','US')")
        .and_where(format!("year >= {}", socrata::soql_string(&since_year.to_string())))
        .and_where(format!("indicator in ({})", inds.join(",")))
        .limit(20_000);
    let fb = snapshot_fallback(ctx, VSRR_SNAPSHOT, STATE_FILE, csv_to_socrata_json);
    socrata::query(&ctx.http, ctx.cache.as_ref(), &q, TTL_SECS, Some(fb)).await
}

async fn county_rows(ctx: &Context, fips: &[String], since_year: i32) -> Result<SocrataRows> {
    let list: Vec<String> = fips.iter().map(|f| socrata::soql_string(f)).collect();
    let q = SocrataQuery::cdc(COUNTY_DATASET)
        .select("data_as_of,year,month,countyname,fips,code2013,provisional_drug_overdose,footnote,percentage_of_records_pending")
        .and_where("st_abbrev = 'MA'")
        .and_where(format!("fips in ({})", list.join(",")))
        .and_where(format!("year >= {}", socrata::soql_string(&since_year.to_string())))
        .limit(20_000);
    let fb = snapshot_fallback(ctx, VSRR_SNAPSHOT, COUNTY_FILE, csv_to_socrata_json);
    socrata::query(&ctx.http, ctx.cache.as_ref(), &q, TTL_SECS, Some(fb)).await
}

/// (year, month 1-12) of a row.
fn row_month(row: &Map<String, Value>) -> Option<(i32, u32)> {
    let year = socrata::num(row, "year")? as i32;
    let m = socrata::text(row, "month")?;
    let month = m.parse::<u32>().ok().or_else(|| {
        MONTHS
            .iter()
            .position(|x| x.eq_ignore_ascii_case(m))
            .map(|i| i as u32 + 1)
    })?;
    Some((year, month))
}

fn label(ym: (i32, u32)) -> String {
    format!("{}-{:02}", ym.0, ym.1)
}

fn pct_change(now: Option<f64>, before: Option<f64>) -> Option<f64> {
    match (now, before) {
        (Some(n), Some(b)) if b > 0.0 => Some(((n - b) / b * 1000.0).round() / 10.0),
        _ => None,
    }
}

fn trend(
    area: &str,
    measure: &str,
    mut points: Vec<((i32, u32), PeriodCount)>,
    months: usize,
    population: Option<f64>,
    notes: Vec<String>,
) -> Option<ProvisionalTrend> {
    points.sort_by_key(|(ym, _)| *ym);
    points.dedup_by_key(|(ym, _)| *ym);
    let (latest_ym, latest) = points.last().cloned()?;
    let earlier_ym = (latest_ym.0 - 1, latest_ym.1);
    let year_earlier = points
        .iter()
        .find(|(ym, _)| *ym == earlier_ym)
        .map(|(_, p)| p.clone());
    let value = |p: &PeriodCount| p.predicted.or(p.deaths);
    let percent_change = year_earlier
        .as_ref()
        .and_then(|e| pct_change(value(&latest), value(e)));
    let rate_per_100k = match (latest.deaths, population) {
        (Some(d), Some(p)) => Some(((d / p) * 100_000.0 * 10.0).round() / 10.0),
        _ => None,
    };
    let start = points.len().saturating_sub(months);
    Some(ProvisionalTrend {
        area: area.into(),
        measure: measure.into(),
        latest,
        year_earlier,
        percent_change,
        rate_per_100k,
        series: points[start..].iter().map(|(_, p)| p.clone()).collect(),
        notes,
    })
}

fn describe_change(pct: Option<f64>) -> String {
    match pct {
        Some(p) if p > 0.0 => format!("up {p:.1}% from a year earlier"),
        Some(p) if p < 0.0 => format!("down {:.1}% from a year earlier", -p),
        Some(_) => "unchanged from a year earlier".into(),
        None => "no comparison a year earlier".into(),
    }
}

fn period_text(ym: &str) -> String {
    let (y, m) = ym.split_once('-').unwrap_or((ym, "01"));
    let m: usize = m.parse().unwrap_or(1);
    format!("{} {y}", MONTHS[(m.max(1) - 1).min(11)])
}

// ---------------------------------------------------------------- the tool

enum Level {
    Town {
        name: String,
        county_fips: String,
        geoid: String,
    },
    County {
        name: String,
        fips: String,
    },
    State,
}

fn resolve(ctx: &Context, q: &str) -> Result<Level> {
    let n = normalize(q);
    if matches!(
        n.as_str(),
        "massachusetts" | "ma" | "mass" | "statewide" | "state" | "commonwealth"
    ) {
        return Ok(Level::State);
    }
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let g = geos.resolve(q, None)?;
    match g.level {
        GeoLevel::Town => Ok(Level::Town {
            name: g.name.clone(),
            county_fips: g.county_geoid.clone(),
            geoid: g.geoid.clone(),
        }),
        GeoLevel::County => Ok(Level::County {
            name: g.name.clone(),
            fips: g.geoid.clone(),
        }),
        _ => Err(PhnError::InvalidInput(format!(
            "'{q}' is a {}; overdose data are available for towns, counties, and the state",
            g.level.label()
        ))),
    }
}

fn county_name(ctx: &Context, fips: &str) -> String {
    MaGeographies::shared(&ctx.snapshots)
        .ok()
        .and_then(|g| g.get(fips).map(|c| c.name.clone()))
        .unwrap_or_else(|| fips.to_string())
}

/// Report overdose deaths for a town, county, or the state.
pub async fn get_overdose_data(
    ctx: &Context,
    args: OverdoseArgs,
) -> Result<Envelope<OverdoseResult>> {
    let area_q = args
        .area
        .clone()
        .filter(|a| !a.trim().is_empty())
        .or_else(|| ctx.settings().jurisdiction.town)
        .unwrap_or_else(|| "Massachusetts".into());
    if args.compare_with.len() > 5 {
        return Err(PhnError::InvalidInput(
            "compare with at most 5 places".into(),
        ));
    }
    let basis = match args.basis.as_deref().map(normalize).as_deref() {
        None | Some("") | Some("residence") | Some("resident") | Some("residents") => "residence",
        Some("occurrence") | Some("death occurrence") | Some("location") => "occurrence",
        Some(other) => {
            return Err(PhnError::InvalidInput(format!(
                "basis '{other}' is not recognized; use residence or occurrence"
            )));
        }
    };
    let drug_key = args
        .drug
        .as_deref()
        .map(|d| normalize(d).replace(' ', "_"))
        .unwrap_or_else(|| "all".into());
    let drug_key = match drug_key.as_str() {
        "fentanyl" | "synthetic" | "synthetic_opioid" => "synthetic_opioids".to_string(),
        "stimulant" | "methamphetamine" | "psychostimulants" | "meth" => "stimulants".to_string(),
        "opioid" => "opioids".to_string(),
        "prescription" | "oxycodone" | "natural_and_semi_synthetic_opioids" => {
            "prescription_opioids".to_string()
        }
        "any" | "all_drugs" | "drugs" | "overdose" => "all".to_string(),
        other => other.to_string(),
    };
    let drug = DRUGS.iter().find(|d| d.0 == drug_key).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "drug '{}' is not recognized; use one of {}",
            args.drug.clone().unwrap_or_default(),
            DRUGS.iter().map(|d| d.0).collect::<Vec<_>>().join(", ")
        ))
    })?;
    let months = args.months.unwrap_or(13).clamp(2, 60);

    let primary = resolve(ctx, &area_q)?;
    let mut places = vec![primary];
    for c in &args.compare_with {
        places.push(resolve(ctx, c)?);
    }
    let acs = AcsSnapshot::shared(&ctx.snapshots)?;
    let pop_source = format!(
        "ACS {}-{} 5-year total population",
        acs.vintage - 4,
        acs.vintage
    );

    // ---- DPH towns
    let (rows, town_source) = town_rows(ctx)?;
    let find = |basis: &str, town: &str| {
        let key = dph_name_key(town);
        rows.iter()
            .find(|r| r.basis == basis && dph_name_key(&r.town) == key)
    };
    let state_total = find("residence", "Total");
    let state_rate = state_total.and_then(|t| {
        pop(acs, "25").map(|p| {
            rate_estimate(
                sum_years(t, &RATE_YEARS),
                p,
                5.0,
                "2018-2022 (average per year)",
                &pop_source,
            )
        })
    });
    let mut towns = Vec::new();
    let mut county_fips: Vec<String> = Vec::new();
    let mut area_label = String::new();
    let mut level_label = String::new();
    for (i, p) in places.iter().enumerate() {
        let (label, lvl) = match p {
            Level::Town {
                name,
                county_fips: cf,
                geoid,
            } => {
                let row = find(basis, name).ok_or_else(|| {
                    PhnError::Internal(format!("{name} is missing from DPH's town table"))
                })?;
                let years = year_counts(row);
                let total = row.counts.iter().sum();
                let rate = if basis == "residence" {
                    pop(acs, geoid).map(|pp| {
                        rate_estimate(
                            sum_years(row, &RATE_YEARS),
                            pp,
                            5.0,
                            "2018-2022 (average per year)",
                            &pop_source,
                        )
                    })
                } else {
                    None
                };
                let compared = match (&rate, &state_rate) {
                    (Some(r), Some(s)) => {
                        match (r.rate_per_100k, r.ci_lower, r.ci_upper, s.rate_per_100k) {
                            (Some(tr), Some(lo), Some(hi), Some(sr)) => Some(if lo > sr {
                                format!(
                                    "higher than the state ({tr:.1} vs {sr:.1} per 100,000; the state rate is below the town's 95% interval)"
                                )
                            } else if hi < sr {
                                format!(
                                    "lower than the state ({tr:.1} vs {sr:.1} per 100,000; the state rate is above the town's 95% interval)"
                                )
                            } else {
                                format!(
                                    "not clearly different from the state ({tr:.1} vs {sr:.1} per 100,000; the state rate is inside the town's 95% interval)"
                                )
                            }),
                            (None, _, _, Some(sr)) => Some(format!(
                                "too few deaths for a rate; the state rate was {sr:.1} per 100,000"
                            )),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let last = row.counts.last().copied().unwrap_or(0);
                let prior: f64 = row.counts
                    [row.counts.len().saturating_sub(4)..row.counts.len() - 1]
                    .iter()
                    .sum::<u64>() as f64
                    / 3.0;
                let latest_year_note = format!(
                    "{last} death(s) in 2022, compared with an average of {prior:.1} a year in 2019-2021{}",
                    if last + prior.round() as u64 >= 20 {
                        ""
                    } else {
                        " (small numbers: year-to-year changes are mostly chance)"
                    }
                );
                if !county_fips.contains(cf) {
                    county_fips.push(cf.clone());
                }
                towns.push(TownSeries {
                    town: name.clone(),
                    county: county_name(ctx, cf),
                    basis: basis.into(),
                    years,
                    total_2015_2022: total,
                    rate_2018_2022: rate,
                    compared_with_state: compared,
                    latest_year_note,
                });
                (name.clone(), "town")
            }
            Level::County { name, fips } => {
                if !county_fips.contains(fips) {
                    county_fips.push(fips.clone());
                }
                (name.clone(), "county")
            }
            Level::State => ("Massachusetts".to_string(), "state"),
        };
        if i == 0 {
            area_label = label;
            level_label = lvl.into();
        }
    }

    // ---- CDC provisional counts
    let mut caveats = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let now_year = chrono::Utc::now()
        .format("%Y")
        .to_string()
        .parse::<i32>()
        .unwrap_or(2026);
    let since = now_year - (months as i32 / 12) - 2;
    let mut counties = Vec::new();
    if !county_fips.is_empty() {
        match county_rows(ctx, &county_fips, since).await {
            Ok(cr) => {
                if let Some(c) = fallback_caveat(ctx, &cr, "CDC county counts", VSRR_SNAPSHOT) {
                    caveats.push(c);
                }
                sources.push(cr.source(
                    "VSRR Provisional County-Level Drug Overdose Death Counts (12 month-ending, by county of residence)",
                    "CDC, National Center for Health Statistics",
                ));
                for f in &county_fips {
                    let mut pts = Vec::new();
                    let mut pending = None;
                    let mut as_of = None;
                    for r in cr
                        .rows
                        .iter()
                        .filter(|r| socrata::text(r, "fips") == Some(f.as_str()))
                    {
                        let Some(ym) = row_month(r) else { continue };
                        let deaths = socrata::num(r, "provisional_drug_overdose");
                        pts.push((
                            ym,
                            PeriodCount {
                                period_ending: label(ym),
                                deaths,
                                predicted: None,
                                suppressed: deaths.is_none(),
                            },
                        ));
                        pending = socrata::num(r, "percentage_of_records_pending").or(pending);
                        as_of = socrata::text(r, "data_as_of")
                            .map(|s| s.chars().take(10).collect::<String>())
                            .or(as_of);
                    }
                    let mut notes = Vec::new();
                    if let Some(p) = pending {
                        notes.push(format!(
                            "{p:.1}% of records pending investigation in the latest period"
                        ));
                    }
                    if let Some(a) = as_of {
                        notes.push(format!("CDC data as of {a}"));
                    }
                    if pts.iter().any(|(_, p)| p.suppressed) {
                        notes.push(CONTENT.rules.county_suppression.clone());
                    }
                    if let Some(t) = trend(
                        &county_name(ctx, f),
                        "all drug overdose deaths (residents)",
                        pts,
                        months,
                        pop(acs, f),
                        notes,
                    ) {
                        counties.push(t);
                    }
                }
            }
            Err(e) => caveats.push(format!("CDC county counts could not be retrieved: {e}")),
        }
    }

    let mut state_and_us = Vec::new();
    let mut by_drug = Vec::new();
    match state_rows(ctx, since).await {
        Ok(sr) => {
            if let Some(c) = fallback_caveat(ctx, &sr, "CDC state counts", VSRR_SNAPSHOT) {
                caveats.push(c);
            }
            sources.push(sr.source(
                "VSRR Provisional Drug Overdose Death Counts (12 month-ending, by state of occurrence)",
                "CDC, National Center for Health Statistics",
            ));
            let series_for = |state: &str, indicator: &str| -> Vec<((i32, u32), PeriodCount)> {
                sr.rows
                    .iter()
                    .filter(|r| {
                        socrata::text(r, "state") == Some(state)
                            && socrata::text(r, "indicator") == Some(indicator)
                    })
                    .filter_map(|r| {
                        let ym = row_month(r)?;
                        let deaths = socrata::num(r, "data_value");
                        Some((
                            ym,
                            PeriodCount {
                                period_ending: label(ym),
                                deaths,
                                predicted: socrata::num(r, "predicted_value"),
                                suppressed: deaths.is_none(),
                            },
                        ))
                    })
                    .collect()
            };
            for (state, name, geo) in [("MA", "Massachusetts", "25"), ("US", "United States", "US")]
            {
                let pts = series_for(state, drug.1);
                let mut notes = Vec::new();
                if let Some(pct) = series_for(state, PCT_SPECIFIED)
                    .iter()
                    .max_by_key(|(ym, _)| *ym)
                    .and_then(|(_, p)| p.deaths)
                {
                    notes.push(format!("{pct:.1}% of overdose deaths in the latest period had specific drugs recorded"));
                }
                if pts.iter().any(|(_, p)| p.suppressed) {
                    notes.push(
                        "Some periods are not shown by CDC because of low data quality.".into(),
                    );
                }
                if let Some(t) = trend(name, drug.2, pts, months, pop(acs, geo), notes) {
                    state_and_us.push(t);
                }
            }
            for (_, indicator, label) in DRUGS {
                let ma = trend(
                    "Massachusetts",
                    label,
                    series_for("MA", indicator),
                    1,
                    None,
                    vec![],
                );
                let us = trend(
                    "United States",
                    label,
                    series_for("US", indicator),
                    1,
                    None,
                    vec![],
                );
                by_drug.push(DrugRow {
                    drug: (*label).into(),
                    cdc_indicator: (*indicator).into(),
                    massachusetts: ma
                        .as_ref()
                        .and_then(|t| t.latest.predicted.or(t.latest.deaths)),
                    massachusetts_change_pct: ma.as_ref().and_then(|t| t.percent_change),
                    united_states: us
                        .as_ref()
                        .and_then(|t| t.latest.predicted.or(t.latest.deaths)),
                    united_states_change_pct: us.as_ref().and_then(|t| t.percent_change),
                });
            }
        }
        Err(e) => caveats.push(format!("CDC state counts could not be retrieved: {e}")),
    }

    // ---- summary
    let mut summary = format!("**Overdose deaths: {area_label}**\n\n");
    for t in &towns {
        summary.push_str(&format!(
            "**{} ({}), DPH, opioid-related overdose deaths of all intents by {}:** ",
            t.town,
            t.county,
            if t.basis == "residence" {
                "residence"
            } else {
                "place of death"
            }
        ));
        summary.push_str(
            &t.years
                .iter()
                .map(|y| format!("{} {}", y.year, y.deaths))
                .collect::<Vec<_>>()
                .join(", "),
        );
        summary.push_str(&format!(
            " (total {}). {}.",
            t.total_2015_2022, t.latest_year_note
        ));
        if let Some(r) = &t.rate_2018_2022 {
            match (r.rate_per_100k, r.ci_lower, r.ci_upper) {
                (Some(v), Some(lo), Some(hi)) => summary.push_str(&format!(
                    " 2018-2022: {} deaths, {v:.1} per 100,000 a year (95% CI {lo:.1}-{hi:.1}){}.",
                    r.deaths,
                    if r.flags.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", r.flags.join("; "))
                    }
                )),
                _ => summary.push_str(&format!(
                    " 2018-2022: {} deaths ({}).",
                    r.deaths,
                    r.flags.join("; ")
                )),
            }
        }
        if let Some(c) = &t.compared_with_state {
            summary.push_str(&format!(" Compared with Massachusetts: {c}."));
        }
        summary.push_str("\n\n");
    }
    if let Some(s) = &state_rate
        && !towns.is_empty()
    {
        summary.push_str(&format!(
            "Massachusetts 2018-2022 (DPH): {} deaths, {:.1} per 100,000 a year.\n\n",
            md::num(s.deaths as f64, 0),
            s.rate_per_100k.unwrap_or_default()
        ));
    }
    if !counties.is_empty() {
        summary.push_str("**CDC provisional drug overdose deaths (all drugs, county of residence), 12 months ending:**\n\n");
        let rows: Vec<Vec<String>> = counties
            .iter()
            .map(|c| {
                vec![
                    c.area.clone(),
                    period_text(&c.latest.period_ending),
                    c.latest
                        .deaths
                        .map(|d| md::num(d, 0))
                        .unwrap_or_else(|| "1-9 (suppressed)".into()),
                    c.rate_per_100k
                        .map(|r| format!("{r:.1}"))
                        .unwrap_or_else(|| "—".into()),
                    c.year_earlier
                        .as_ref()
                        .and_then(|y| y.deaths)
                        .map(|d| md::num(d, 0))
                        .unwrap_or_else(|| "—".into()),
                    c.percent_change
                        .map(|p| format!("{p:+.1}%"))
                        .unwrap_or_else(|| "—".into()),
                ]
            })
            .collect();
        summary.push_str(&md::table(
            &[
                "County",
                "12 months ending",
                "Deaths",
                "Per 100,000",
                "A year earlier",
                "Change",
            ],
            &rows,
        ));
        summary.push('\n');
    }
    if !state_and_us.is_empty() {
        summary.push_str(&format!(
            "**CDC provisional counts, {} (place of death):**\n",
            drug.2
        ));
        for t in &state_and_us {
            let v = t.latest.predicted.or(t.latest.deaths);
            summary.push_str(&format!(
                "- {}: {} in the 12 months ending {} (predicted, adjusted for reporting delays), {}.\n",
                t.area,
                v.map(|d| md::num(d, 0)).unwrap_or_else(|| "not shown".into()),
                period_text(&t.latest.period_ending),
                describe_change(t.percent_change)
            ));
        }
        summary.push('\n');
    }
    if level_label == "state" && !by_drug.is_empty() {
        let rows: Vec<Vec<String>> = by_drug
            .iter()
            .map(|d| {
                vec![
                    d.drug.clone(),
                    d.massachusetts
                        .map(|v| md::num(v, 0))
                        .unwrap_or_else(|| "—".into()),
                    d.massachusetts_change_pct
                        .map(|p| format!("{p:+.1}%"))
                        .unwrap_or_else(|| "—".into()),
                    d.united_states_change_pct
                        .map(|p| format!("{p:+.1}%"))
                        .unwrap_or_else(|| "—".into()),
                ]
            })
            .collect();
        summary.push_str(
            "**By drug (latest 12 months; a death can involve more than one drug):**\n\n",
        );
        summary.push_str(&md::table(
            &["Drug", "Massachusetts", "MA change", "U.S. change"],
            &rows,
        ));
        summary.push('\n');
    }
    let findings: Vec<CitedFinding> = CONTENT
        .findings_2024
        .iter()
        .map(|f| CitedFinding {
            text: f.text.clone(),
            source: CONTENT
                .sources
                .get(&f.source)
                .map(|s| s.title.clone())
                .unwrap_or_default(),
            section: f.section.clone(),
        })
        .collect();
    summary.push_str("**DPH statewide findings for 2024** (Massachusetts 2024 Opioid-Involved Overdose Report, March 2026):\n");
    for f in findings
        .iter()
        .take(if level_label == "state" { 6 } else { 2 })
    {
        summary.push_str(&format!("- {} ({})\n", f.text, f.section));
    }
    let newer = format!(
        "DPH's town-level counts after 2022 (through 2025) are on the BSAS dashboard Community Profile: {}",
        CONTENT
            .sources
            .get("bsas_dashboard")
            .and_then(|s| s.url.clone())
            .unwrap_or_default()
    );
    summary.push_str(&format!("\n{newer}\n"));

    // ---- sources and caveats
    let prepared = CONTENT.meta.prepared.clone();
    let mut all_sources = Vec::new();
    if !towns.is_empty() {
        all_sources.push(town_source);
        all_sources.push(acs.source());
    }
    all_sources.extend(sources);
    all_sources.extend(content::cite(
        &CONTENT.sources,
        &["dph_2024_report", "bsas_dashboard"],
        &prepared,
    ));
    if !towns.is_empty() {
        caveats.push("DPH's town table (posted June 2023) is the latest town-level table DPH published as a document; 2020-2022 counts were preliminary then. DPH's 2024 report uses an updated 'opioid-involved' method, so its counts differ slightly: do not mix the two series.".into());
        caveats.push(format!(
            "Rates: 5-year average annual crude rates with exact Poisson 95% intervals, using the {pop_source} as the yearly population (DPH uses UMass Donahue Institute estimates, so DPH's own rates differ slightly). {}",
            CONTENT.rules.rate_min_deaths_source
        ));
        if basis == "occurrence" {
            caveats.push("Occurrence counts include non-residents who died in the town (for example at a hospital), so no rate is calculated.".into());
        }
    }
    caveats.push("CDC counts are provisional and change monthly. State counts are by the state where the death occurred; county counts are by county of residence; town counts are DPH's. The three sources use different definitions and periods.".into());
    caveats.push("Drug-specific counts overlap (one death can involve several drugs). CDC advises against comparing drug-specific death rates between jurisdictions because reporting of specific drugs varies.".into());
    caveats.extend(content::review_caveats(&[OVERDOSE_DATA]));
    let definitions = CONTENT
        .definitions
        .iter()
        .map(|d| CitedFinding {
            text: d.text.clone(),
            source: CONTENT
                .sources
                .get(&d.source)
                .map(|s| s.title.clone())
                .unwrap_or_default(),
            section: d.section.clone(),
        })
        .collect();
    let mut privacy = Privacy::none();
    privacy.notes.push("Published aggregate counts only; town counts are shown as DPH published them, county counts of 1-9 stay suppressed as CDC published them.".into());
    if all_sources.iter().all(|s| s.mode == SourceMode::Snapshot) {
        privacy
            .notes
            .push("All figures came from bundled snapshots.".into());
    }
    Ok(Envelope::new(
        summary,
        OverdoseResult {
            area: area_label,
            level: level_label,
            towns,
            counties,
            state_and_us,
            by_drug,
            dph_2024_findings: findings,
            newer_town_data: newer,
            definitions,
            method: "Towns: DPH counts by year; 2018-2022 average annual rate = deaths / (population x 5) x 100,000 with Garwood exact Poisson 95% limits; no rate under 5 deaths; 'higher' or 'lower' than the state when the state rate falls outside the town's interval. Counties, state, and U.S.: CDC 12 month-ending provisional counts; percent change compares the latest period with the 12 months ending a year earlier (predicted values for the state and U.S., reported counts for counties); crude rates use ACS 2020-2024 population.".into(),
        },
    )
    .sources(all_sources)
    .caveats(caveats)
    .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let mut c = phn_core::Config::for_root(dir.path());
        c.offline = true;
        (dir, Context::new(c).unwrap())
    }

    #[test]
    fn town_table_has_every_town_and_matches_totals() {
        let (_d, ctx) = ctx();
        let (rows, _) = town_rows(&ctx).unwrap();
        let geos = MaGeographies::shared(&ctx.snapshots).unwrap();
        for basis in ["residence", "occurrence"] {
            let named: Vec<&TownRow> = rows
                .iter()
                .filter(|r| {
                    r.basis == basis
                        && !matches!(
                            r.town.as_str(),
                            "Total" | "Unknown" | "Out of Massachusetts"
                        )
                })
                .collect();
            assert_eq!(named.len(), 351, "{basis}");
            for t in geos.at_level(GeoLevel::Town) {
                assert!(
                    named
                        .iter()
                        .any(|r| dph_name_key(&r.town) == dph_name_key(&t.name)),
                    "{} missing ({basis})",
                    t.name
                );
            }
        }
        let boston = rows
            .iter()
            .find(|r| r.basis == "residence" && r.town == "Boston")
            .unwrap();
        assert_eq!(boston.counts, vec![155, 197, 203, 183, 173, 248, 251, 247]);
        let total = rows
            .iter()
            .find(|r| r.basis == "residence" && r.town == "Total")
            .unwrap();
        assert_eq!(total.counts[7], 2310);
    }

    #[test]
    fn rates_follow_dph_and_nchs_small_number_rules() {
        let r = rate_estimate(4, 10_000.0, 5.0, "p", "ACS");
        assert!(r.rate_per_100k.is_none());
        let r = rate_estimate(10, 10_000.0, 5.0, "p", "ACS");
        assert_eq!(r.rate_per_100k, Some(20.0));
        assert!(r.flags[0].contains("unstable"));
        // Exact Poisson limits for 10 events: 4.795 to 18.390 per 50,000 person-years.
        assert_eq!(r.ci_lower, Some(9.6));
        assert_eq!(r.ci_upper, Some(36.8));
        let r = rate_estimate(100, 100_000.0, 5.0, "p", "ACS");
        assert!(r.flags.is_empty());
    }

    #[test]
    fn csv_snapshot_converts_to_socrata_rows() {
        let body = csv_to_socrata_json("a,b\n1,\n2,x\n").unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v[0]["a"], "1");
        assert!(v[0].get("b").is_none());
        assert_eq!(v[1]["b"], "x");
        assert_eq!(pct_change(Some(110.0), Some(100.0)), Some(10.0));
        assert_eq!(period_text("2026-03"), "March 2026");
    }
}
