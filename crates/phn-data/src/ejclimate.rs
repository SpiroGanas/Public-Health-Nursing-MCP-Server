//! Environmental justice and climate data for ENV-04 and ENV-05:
//!
//! * the Massachusetts 2020 Environmental Justice block groups (EEA/MassGIS,
//!   updated November 2022, corrected June 2024) and EEA's municipal EJ
//!   statistics, with 2020 Census town population (`ma-ej-2020`);
//! * the CDC/ATSDR Heat and Health Index 2024 for Massachusetts ZCTAs
//!   (`cdc-heat-health-index-2024-ma`);
//! * NFIP flood insurance claims by town and the NFIP Community Status Book
//!   (`fema-nfip-ma`), and FEMA hazard mitigation plan statuses
//!   (`fema-hazard-mitigation-plans-ma`); the Community Status Book and the
//!   plan statuses are also fetched live from OpenFEMA;
//! * Municipal Vulnerability Preparedness (MVP) regions, planning reports, and
//!   the MVP 2.0 pilot list (`ma-mvp-communities`).
//!
//! Builder: `xtask/scripts/build_ejclimate_snapshots.py`.

use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source, SourceMode};
use serde_json::{Map, Value};

use crate::apifetch::{expect_json, fetch_validated};
use crate::cache::Cache;
use crate::fetch::SnapshotFallback;
use crate::http::HttpClient;
use crate::snapshots::SnapshotStore;

/// EJ snapshot id and files.
pub const EJ_SNAPSHOT_ID: &str = "ma-ej-2020";
pub const EJ_BLOCK_GROUPS_FILE: &str = "ma_ej_2020_block_groups.csv";
pub const EJ_TOWNS_FILE: &str = "ma_ej_2020_towns.csv";
/// MassGIS page for the layer.
pub const EJ_LAYER_URL: &str =
    "https://www.mass.gov/info-details/massgis-data-2020-environmental-justice-populations";
/// EEA page on EJ populations (criteria and maps).
pub const EJ_POPULATIONS_URL: &str =
    "https://www.mass.gov/info-details/environmental-justice-populations-in-massachusetts";

/// Heat and Health Index snapshot.
pub const HHI_SNAPSHOT_ID: &str = "cdc-heat-health-index-2024-ma";
pub const HHI_FILE: &str = "ma_heat_health_index_2024.csv";
pub const HHI_URL: &str = "https://www.atsdr.cdc.gov/place-health/php/hhi/index.html";

/// NFIP snapshot (claims by town and the Community Status Book).
pub const NFIP_SNAPSHOT_ID: &str = "fema-nfip-ma";
pub const NFIP_COMMUNITIES_FILE: &str = "fema_nfip_community_status_ma.csv";
pub const NFIP_CLAIMS_FILE: &str = "fema_nfip_claims_ma_towns.csv";
pub const NFIP_CLAIMS_PAGE: &str = "https://www.fema.gov/openfema-data-page/nfip-redacted-claims-v3";
pub const NFIP_COMMUNITIES_PAGE: &str =
    "https://www.fema.gov/openfema-data-page/nfip-community-status-book-v1";

/// Hazard mitigation plan statuses.
pub const HMP_SNAPSHOT_ID: &str = "fema-hazard-mitigation-plans-ma";
pub const HMP_FILE: &str = "fema_hazard_mitigation_plans_ma.csv";
pub const HMP_PAGE: &str =
    "https://www.fema.gov/openfema-data-page/hazard-mitigation-plan-statuses-v1";

/// MVP communities.
pub const MVP_SNAPSHOT_ID: &str = "ma-mvp-communities";
pub const MVP_FILE: &str = "ma_mvp_communities.csv";
pub const MVP_URL: &str = "https://www.mass.gov/municipal-vulnerability-preparedness-mvp-program";

/// OpenFEMA cache lifetime (a day).
pub const FEMA_TTL_SECS: u64 = 24 * 3600;

fn bad(file: &str, e: impl std::fmt::Display) -> PhnError {
    PhnError::Internal(format!("{file}: {e}"))
}

fn opt_f64(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.parse::<f64>().ok().filter(|v| v.is_finite())
    }
}

fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Read a CSV snapshot into header-keyed rows.
fn rows(text: &str, file: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| bad(file, e))?
        .iter()
        .map(str::to_string)
        .collect();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let r = rec.map_err(|e| bad(file, e))?;
        out.push(r.iter().map(str::to_string).collect());
    }
    Ok((headers, out))
}

fn col(headers: &[String], name: &str, file: &str) -> Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| bad(file, format!("missing column {name}")))
}

// ------------------------------------------------------------------ EJ layer

/// One EJ block group polygon (block groups split at a town line appear once per town).
#[derive(Debug, Clone, PartialEq)]
pub struct EjBlockGroup {
    /// 12-digit block group GEOID.
    pub geoid: String,
    pub town: String,
    /// The block group was split at a town line; the attributes are the whole block group's.
    pub split_for_town: bool,
    /// 2020 Census population of the whole block group.
    pub population: f64,
    /// Percent of residents other than non-Hispanic White alone (2020 Census).
    pub pct_minority: Option<f64>,
    /// Households (ACS 2016–2020).
    pub households: Option<f64>,
    /// Limited English speaking households (ACS 2016–2020, table C16002).
    pub limited_english_households: Option<f64>,
    pub pct_limited_english_households: Option<f64>,
    /// Block group median household income (ACS 2016–2020; 2015–2019 when flagged).
    pub median_household_income: Option<f64>,
    /// Block group median household income as a percent of the state's.
    pub mhhi_pct_of_state: Option<f64>,
    /// The income values are from the 2015–2019 ACS (no 2016–2020 estimate).
    pub mhhi_from_2019: bool,
    /// The town's median household income as a percent of the state's.
    pub town_mhhi_pct_of_state: Option<f64>,
    pub minority: bool,
    pub income: bool,
    pub english_isolation: bool,
    /// Encoded geometry (see `boundaries::decode_geometry`).
    pub geometry: String,
}

impl EjBlockGroup {
    /// The criteria code as EEA writes it ("M", "MI", "MIE", ...).
    pub fn criteria(&self) -> String {
        let mut s = String::new();
        if self.minority {
            s.push('M');
        }
        if self.income {
            s.push('I');
        }
        if self.english_isolation {
            s.push('E');
        }
        s
    }

    /// The 11-digit census tract GEOID.
    pub fn tract(&self) -> &str {
        self.geoid.get(..11).unwrap_or(&self.geoid)
    }

    /// Number of criteria met (1–3).
    pub fn criteria_count(&self) -> usize {
        [self.minority, self.income, self.english_isolation]
            .iter()
            .filter(|b| **b)
            .count()
    }
}

/// One city or town: 2020 Census counts and EEA's municipal EJ statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct EjTown {
    pub town: String,
    pub geoid: String,
    pub county: String,
    /// 2020 Census population (P.L. 94-171, via MassGIS).
    pub pop_2020: f64,
    pub housing_units_2020: f64,
    /// EEA's municipal statistics (towns with EJ block groups only).
    pub ej_criteria: Option<String>,
    pub ej_block_groups: Option<u32>,
    pub total_block_groups: Option<u32>,
    pub pop_in_ej_block_groups: Option<f64>,
    pub stats_total_pop: Option<f64>,
    pub pct_pop_in_ej_block_groups: Option<f64>,
}

/// The loaded EJ snapshot.
#[derive(Debug, Clone)]
pub struct EjLayer {
    pub block_groups: Vec<EjBlockGroup>,
    pub towns: Vec<EjTown>,
    pub source: Source,
}

impl EjLayer {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, _) = store.file_text(EJ_SNAPSHOT_ID, EJ_BLOCK_GROUPS_FILE)?;
        let f = EJ_BLOCK_GROUPS_FILE;
        let (h, data) = rows(&text, f)?;
        let c = |n: &str| col(&h, n, f);
        let (geoid, town, split, pop, minority, hh, leh, lep, mhhi, mhhi_pct, from19, town_pct, crit, geom) = (
            c("geoid")?,
            c("town")?,
            c("split_for_town")?,
            c("population")?,
            c("pct_minority")?,
            c("households")?,
            c("limited_english_households")?,
            c("pct_limited_english_households")?,
            c("bg_median_household_income")?,
            c("bg_mhhi_pct_of_state")?,
            c("mhhi_from_2019")?,
            c("town_mhhi_pct_of_state")?,
            c("criteria")?,
            c("geometry")?,
        );
        let mut block_groups = Vec::with_capacity(data.len());
        for r in &data {
            let criteria = &r[crit];
            block_groups.push(EjBlockGroup {
                geoid: r[geoid].clone(),
                town: r[town].clone(),
                split_for_town: r[split] == "Y",
                population: opt_f64(&r[pop]).ok_or_else(|| bad(f, "missing population"))?,
                pct_minority: opt_f64(&r[minority]),
                households: opt_f64(&r[hh]),
                limited_english_households: opt_f64(&r[leh]),
                pct_limited_english_households: opt_f64(&r[lep]),
                median_household_income: opt_f64(&r[mhhi]),
                mhhi_pct_of_state: opt_f64(&r[mhhi_pct]),
                mhhi_from_2019: r[from19] == "1",
                town_mhhi_pct_of_state: opt_f64(&r[town_pct]),
                minority: criteria.contains('M'),
                income: criteria.contains('I'),
                english_isolation: criteria.contains('E'),
                geometry: r[geom].clone(),
            });
        }
        let (ttext, _, _) = store.file_text(EJ_SNAPSHOT_ID, EJ_TOWNS_FILE)?;
        let f = EJ_TOWNS_FILE;
        let (h, data) = rows(&ttext, f)?;
        let c = |n: &str| col(&h, n, f);
        let idx = [
            c("town")?,
            c("geoid")?,
            c("county")?,
            c("pop_2020")?,
            c("housing_units_2020")?,
            c("ej_criteria")?,
            c("ej_block_groups")?,
            c("total_block_groups")?,
            c("pop_in_ej_block_groups")?,
            c("stats_total_pop")?,
            c("pct_pop_in_ej_block_groups")?,
        ];
        let mut towns = Vec::with_capacity(data.len());
        for r in &data {
            let g = |i: usize| r[idx[i]].as_str();
            towns.push(EjTown {
                town: g(0).to_string(),
                geoid: g(1).to_string(),
                county: g(2).to_string(),
                pop_2020: opt_f64(g(3)).ok_or_else(|| bad(f, "missing population"))?,
                housing_units_2020: opt_f64(g(4)).unwrap_or(0.0),
                ej_criteria: opt_str(g(5)),
                ej_block_groups: opt_f64(g(6)).map(|v| v as u32),
                total_block_groups: opt_f64(g(7)).map(|v| v as u32),
                pop_in_ej_block_groups: opt_f64(g(8)),
                stats_total_pop: opt_f64(g(9)),
                pct_pop_in_ej_block_groups: opt_f64(g(10)),
            });
        }
        let mut source = entry.to_source();
        source.url = Some(EJ_LAYER_URL.into());
        Ok(Self {
            block_groups,
            towns,
            source,
        })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static EjLayer> {
        static CELL: OnceLock<std::result::Result<EjLayer, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("EJ snapshot: {e}")))
    }

    /// A town's row.
    pub fn town(&self, name: &str) -> Option<&EjTown> {
        self.towns.iter().find(|t| t.town == name)
    }
}

// ------------------------------------------------------------------ Heat and Health Index

/// One ZCTA in the Heat and Health Index (percentile ranks are national, 0–1).
#[derive(Debug, Clone, PartialEq)]
pub struct HhiZcta {
    pub zcta: String,
    /// ACS 2015–2019 population.
    pub population: Option<f64>,
    /// Average number of extreme heat days a year, 2018–2022 (days above the
    /// ZCTA's own 95th percentile of 1991–2020 temperatures).
    pub extreme_heat_days: Option<f64>,
    pub pr_extreme_heat_days: Option<f64>,
    /// Percentile rank of heat-related EMS activations, 2020–2022 (NEMSIS).
    pub pr_heat_ems: Option<f64>,
    pub rank_heat_burden: Option<f64>,
    pub rank_sensitivity: Option<f64>,
    pub rank_sociodemographic: Option<f64>,
    pub rank_built_environment: Option<f64>,
    pub rank_overall: Option<f64>,
    /// Percent of the ZCTA covered by impervious surface (NLCD 2021).
    pub pct_impervious: Option<f64>,
    pub pr_impervious: Option<f64>,
    /// Percent of the ZCTA with tree canopy (USFS tree canopy cover, NLCD 2021).
    pub pct_tree_canopy: Option<f64>,
    /// Percentile rank, ranked so that LESS canopy ranks higher.
    pub pr_tree_canopy: Option<f64>,
}

/// The loaded Heat and Health Index snapshot.
#[derive(Debug, Clone)]
pub struct HhiSnapshot {
    pub zctas: Vec<HhiZcta>,
    pub source: Source,
}

impl HhiSnapshot {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, _) = store.file_text(HHI_SNAPSHOT_ID, HHI_FILE)?;
        let (h, data) = rows(&text, HHI_FILE)?;
        let c = |n: &str| col(&h, n, HHI_FILE);
        let i = [
            c("zcta")?,
            c("population")?,
            c("extreme_heat_days")?,
            c("pr_extreme_heat_days")?,
            c("pr_heat_ems")?,
            c("rank_heat_burden")?,
            c("rank_sensitivity")?,
            c("rank_sociodemographic")?,
            c("rank_built_environment")?,
            c("rank_overall")?,
            c("pct_impervious")?,
            c("pr_impervious")?,
            c("pct_tree_canopy")?,
            c("pr_tree_canopy")?,
        ];
        let zctas = data
            .iter()
            .map(|r| {
                let v = |k: usize| opt_f64(&r[i[k]]);
                HhiZcta {
                    zcta: r[i[0]].clone(),
                    population: v(1),
                    extreme_heat_days: v(2),
                    pr_extreme_heat_days: v(3),
                    pr_heat_ems: v(4),
                    rank_heat_burden: v(5),
                    rank_sensitivity: v(6),
                    rank_sociodemographic: v(7),
                    rank_built_environment: v(8),
                    rank_overall: v(9),
                    pct_impervious: v(10),
                    pr_impervious: v(11),
                    pct_tree_canopy: v(12),
                    pr_tree_canopy: v(13),
                }
            })
            .collect();
        let mut source = entry.to_source();
        source.url = Some(HHI_URL.into());
        Ok(Self { zctas, source })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static HhiSnapshot> {
        static CELL: OnceLock<std::result::Result<HhiSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("Heat and Health Index snapshot: {e}")))
    }

    /// A ZCTA.
    pub fn zcta(&self, zcta: &str) -> Option<&HhiZcta> {
        self.zctas.iter().find(|z| z.zcta == zcta)
    }
}

// ------------------------------------------------------------------ NFIP claims

/// NFIP claims summed for one town (all years since 1978 and a recent ten-year window).
#[derive(Debug, Clone, PartialEq)]
pub struct NfipClaimsTown {
    pub town: String,
    pub claims: u32,
    /// Claims with any payment.
    pub paid_claims: u32,
    /// Building, contents, and increased-cost-of-compliance payments (dollars, not inflation-adjusted).
    pub total_paid: f64,
    pub claims_recent: u32,
    pub paid_recent: f64,
    pub first_year: Option<i32>,
    pub peak_year: Option<i32>,
    pub peak_year_claims: Option<u32>,
    pub recent_from: i32,
    pub recent_to: i32,
}

/// The loaded claims summary.
#[derive(Debug, Clone)]
pub struct NfipClaimsSnapshot {
    pub towns: Vec<NfipClaimsTown>,
    pub source: Source,
}

impl NfipClaimsSnapshot {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let f = NFIP_CLAIMS_FILE;
        let (text, entry, _) = store.file_text(NFIP_SNAPSHOT_ID, f)?;
        let (h, data) = rows(&text, f)?;
        let c = |n: &str| col(&h, n, f);
        let i = [
            c("town")?,
            c("claims")?,
            c("paid_claims")?,
            c("total_paid")?,
            c("claims_recent")?,
            c("paid_recent")?,
            c("first_year")?,
            c("peak_year")?,
            c("peak_year_claims")?,
            c("recent_from")?,
            c("recent_to")?,
        ];
        let mut towns = Vec::new();
        for r in &data {
            let n = |k: usize| opt_f64(&r[i[k]]);
            towns.push(NfipClaimsTown {
                town: r[i[0]].clone(),
                claims: n(1).unwrap_or(0.0) as u32,
                paid_claims: n(2).unwrap_or(0.0) as u32,
                total_paid: n(3).unwrap_or(0.0),
                claims_recent: n(4).unwrap_or(0.0) as u32,
                paid_recent: n(5).unwrap_or(0.0),
                first_year: n(6).map(|v| v as i32),
                peak_year: n(7).map(|v| v as i32),
                peak_year_claims: n(8).map(|v| v as u32),
                recent_from: n(9).ok_or_else(|| bad(f, "missing recent_from"))? as i32,
                recent_to: n(10).ok_or_else(|| bad(f, "missing recent_to"))? as i32,
            });
        }
        let mut source = entry.to_source();
        source.url = Some(NFIP_CLAIMS_PAGE.into());
        Ok(Self { towns, source })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static NfipClaimsSnapshot> {
        static CELL: OnceLock<std::result::Result<NfipClaimsSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("NFIP claims snapshot: {e}")))
    }

    /// A town's row.
    pub fn town(&self, name: &str) -> Option<&NfipClaimsTown> {
        self.towns.iter().find(|t| t.town == name)
    }
}

// ------------------------------------------------------------------ OpenFEMA (live)

/// One NFIP community (Community Status Book).
#[derive(Debug, Clone, PartialEq)]
pub struct NfipCommunity {
    pub community_id: String,
    /// "SCITUATE, TOWN OF".
    pub name: String,
    pub county: String,
    pub participating: bool,
    /// Current effective flood map date as published ("07/16/14").
    pub current_map_date: Option<String>,
    /// Community Rating System class (1–10) when the community takes part.
    pub crs_class: Option<String>,
    pub crs_effective: Option<String>,
    pub sfha_discount: Option<String>,
    pub sanction: bool,
    pub sanction_date: Option<String>,
}

/// One hazard mitigation plan row (one plan and one jurisdiction).
#[derive(Debug, Clone, PartialEq)]
pub struct MitigationPlan {
    pub plan_id: String,
    /// Census GEOID of the jurisdiction (town or place), as FEMA records it.
    pub geoid: Option<String>,
    /// "Abington town", "Boston city".
    pub place_name: Option<String>,
    pub county: Option<String>,
    pub jurisdiction_type: String,
    /// NFIP community number, when given.
    pub community_id: Option<String>,
    pub title: String,
    pub plan_type: Option<String>,
    /// Approved, Expired, Approvable Pending Adoption, Awaiting Revisions, In Review.
    pub plan_status: String,
    pub jurisdiction_status: Option<String>,
    pub approval_date: Option<String>,
    pub expiration_date: Option<String>,
    pub adoption_date: Option<String>,
}

/// Rows from OpenFEMA with where they came from.
#[derive(Debug, Clone)]
pub struct FemaFetch<T> {
    pub rows: Vec<T>,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
}

/// Live query: the Massachusetts Community Status Book.
pub fn communities_url() -> String {
    "https://www.fema.gov/api/open/v1/NfipCommunityStatusBook?$filter=state%20eq%20%27MA%27&$top=1000"
        .into()
}

/// Live query: Massachusetts hazard mitigation plan statuses.
pub fn plans_url() -> String {
    "https://www.fema.gov/api/open/v1/HazardMitigationPlanStatuses?$filter=stateAbbreviation%20eq%20%27MA%27&$top=1000"
        .into()
}

/// The snapshot CSV (whose columns are the OpenFEMA field names) as the
/// OpenFEMA JSON shape.
pub fn snapshot_as_json(csv_text: &str, entity: &str) -> Option<Vec<u8>> {
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());
    let headers: Vec<String> = rdr.headers().ok()?.iter().map(str::to_string).collect();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let r = rec.ok()?;
        let mut m = Map::new();
        for (h, v) in headers.iter().zip(r.iter()) {
            let val = match v {
                "" => Value::Null,
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                s => Value::String(s.to_string()),
            };
            m.insert(h.clone(), val);
        }
        out.push(Value::Object(m));
    }
    let mut root = Map::new();
    root.insert(entity.to_string(), Value::Array(out));
    serde_json::to_vec(&Value::Object(root)).ok()
}

fn entity_rows<'a>(v: &'a Value, entity: &str) -> Result<&'a Vec<Value>> {
    v.get(entity)
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network(format!("OpenFEMA response has no {entity} rows")))
}

fn text(r: &Value, k: &str) -> Option<String> {
    match r.get(k) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn date(r: &Value, k: &str) -> Option<String> {
    text(r, k)
        .map(|s| s.chars().take(10).collect::<String>())
        .filter(|s| s.len() == 10 && s.as_bytes()[4] == b'-')
}

fn flag(r: &Value, k: &str) -> bool {
    match r.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Parse the Community Status Book JSON.
pub fn parse_communities(body: &[u8]) -> Result<Vec<NfipCommunity>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("OpenFEMA response was not JSON ({e})")))?;
    Ok(entity_rows(&v, "NfipCommunityStatusBook")?
        .iter()
        .filter_map(|r| {
            Some(NfipCommunity {
                community_id: text(r, "communityIdNumber")?,
                name: text(r, "communityName")?,
                county: text(r, "county").unwrap_or_default(),
                participating: flag(r, "participatingInNFIP"),
                current_map_date: text(r, "currentlyEffectiveMapDate"),
                crs_class: text(r, "classRating"),
                crs_effective: date(r, "classRatingEffectiveDate"),
                sfha_discount: text(r, "sfhaDiscount"),
                sanction: flag(r, "sanction"),
                sanction_date: date(r, "sanctionDate"),
            })
        })
        .collect())
}

/// Parse the hazard mitigation plan statuses JSON.
pub fn parse_plans(body: &[u8]) -> Result<Vec<MitigationPlan>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("OpenFEMA response was not JSON ({e})")))?;
    Ok(entity_rows(&v, "HazardMitigationPlanStatuses")?
        .iter()
        .filter_map(|r| {
            Some(MitigationPlan {
                plan_id: text(r, "planId")?,
                geoid: text(r, "mppGeoid"),
                place_name: text(r, "placeName"),
                county: text(r, "countyName"),
                jurisdiction_type: text(r, "jurisdictionType").unwrap_or_default(),
                community_id: text(r, "communityIdNumber"),
                title: text(r, "planTitle").unwrap_or_default(),
                plan_type: text(r, "planType"),
                plan_status: text(r, "planStatus").unwrap_or_default(),
                jurisdiction_status: text(r, "jurisdictionStatus"),
                approval_date: date(r, "planApprovalDate"),
                expiration_date: date(r, "planExpirationDate"),
                adoption_date: date(r, "adoptionDate"),
            })
        })
        .collect())
}

fn validator(entity: &'static str) -> impl Fn(&[u8]) -> Result<()> + Sync {
    move |body: &[u8]| {
        expect_json(body)?;
        let v: Value = serde_json::from_slice(body)
            .map_err(|e| PhnError::Network(format!("OpenFEMA response was not JSON ({e})")))?;
        let n = entity_rows(&v, entity)?.len();
        if n == 0 {
            return Err(PhnError::Network(format!(
                "OpenFEMA returned no {entity} rows"
            )));
        }
        Ok(())
    }
}

async fn fetch_entity(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
    url: &str,
    entity: &'static str,
    snapshot: (&'static str, &'static str),
) -> Result<crate::fetch::Fetched> {
    let store = store.clone();
    let fallback: SnapshotFallback = Box::new(move || {
        let (text, entry, _) = store.file_text(snapshot.0, snapshot.1).ok()?;
        Some((snapshot_as_json(&text, entity)?, entry.retrieved))
    });
    let validate = validator(entity);
    fetch_validated(
        http,
        cache,
        url,
        None,
        FEMA_TTL_SECS,
        &validate,
        Some(fallback),
    )
    .await
}

/// The Massachusetts Community Status Book (live → cache → snapshot).
pub async fn fetch_communities(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
) -> Result<FemaFetch<NfipCommunity>> {
    let f = fetch_entity(
        http,
        cache,
        store,
        &communities_url(),
        "NfipCommunityStatusBook",
        (NFIP_SNAPSHOT_ID, NFIP_COMMUNITIES_FILE),
    )
    .await?;
    Ok(FemaFetch {
        rows: parse_communities(&f.body)?,
        mode: f.mode,
        as_of: f.as_of,
        fallback_reason: f.fallback_reason,
    })
}

/// Massachusetts hazard mitigation plan statuses (live → cache → snapshot).
pub async fn fetch_plans(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
) -> Result<FemaFetch<MitigationPlan>> {
    let f = fetch_entity(
        http,
        cache,
        store,
        &plans_url(),
        "HazardMitigationPlanStatuses",
        (HMP_SNAPSHOT_ID, HMP_FILE),
    )
    .await?;
    Ok(FemaFetch {
        rows: parse_plans(&f.body)?,
        mode: f.mode,
        as_of: f.as_of,
        fallback_reason: f.fallback_reason,
    })
}

// ------------------------------------------------------------------ MVP

/// A town's Municipal Vulnerability Preparedness (MVP) program record.
#[derive(Debug, Clone, PartialEq)]
pub struct MvpCommunity {
    pub town: String,
    /// MVP region ("Southeast Region"); None when the regions page does not list the town.
    pub region: Option<String>,
    /// MVP planning report titles listed for the town on mass.gov.
    pub planning_reports: Vec<String>,
    /// Marked on mass.gov as designated through prior work.
    pub designated_through_prior_work: bool,
    /// In the FY24 MVP 2.0 pilot round (MVP Community Snapshots list).
    pub mvp2_fy24_pilot: bool,
}

/// The loaded MVP snapshot.
#[derive(Debug, Clone)]
pub struct MvpSnapshot {
    pub towns: Vec<MvpCommunity>,
    pub source: Source,
}

impl MvpSnapshot {
    /// Parse the snapshot.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, _) = store.file_text(MVP_SNAPSHOT_ID, MVP_FILE)?;
        let (h, data) = rows(&text, MVP_FILE)?;
        let c = |n: &str| col(&h, n, MVP_FILE);
        let i = [
            c("town")?,
            c("mvp_region")?,
            c("planning_reports")?,
            c("designated_through_prior_work")?,
            c("mvp2_fy24_pilot")?,
        ];
        let towns = data
            .iter()
            .map(|r| MvpCommunity {
                town: r[i[0]].clone(),
                region: opt_str(&r[i[1]]),
                planning_reports: r[i[2]]
                    .split("; ")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                designated_through_prior_work: r[i[3]] == "true",
                mvp2_fy24_pilot: r[i[4]] == "true",
            })
            .collect();
        let mut source = entry.to_source();
        source.url = Some(MVP_URL.into());
        Ok(Self { towns, source })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static MvpSnapshot> {
        static CELL: OnceLock<std::result::Result<MvpSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("MVP snapshot: {e}")))
    }

    /// A town's row.
    pub fn town(&self, name: &str) -> Option<&MvpCommunity> {
        self.towns.iter().find(|t| t.town == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshots_parse() {
        let store = SnapshotStore::bundled();
        let ej = EjLayer::load(&store).unwrap();
        // EEA: 2,604 EJ block groups in 187 municipalities (2022 update).
        assert_eq!(ej.block_groups.len(), 2604);
        let towns: std::collections::BTreeSet<&str> =
            ej.block_groups.iter().map(|b| b.town.as_str()).collect();
        assert_eq!(towns.len(), 187);
        assert_eq!(ej.towns.len(), 351);
        let total: f64 = ej.towns.iter().map(|t| t.pop_2020).sum();
        assert_eq!(total, 7_029_917.0);
        let acton = ej.town("Acton").unwrap();
        assert_eq!(acton.ej_block_groups, Some(3));
        assert_eq!(acton.pop_2020, 24_021.0);
        assert!(ej.block_groups.iter().all(|b| b.criteria_count() >= 1));
        let hhi = HhiSnapshot::load(&store).unwrap();
        assert!(hhi.zctas.len() > 500);
        assert!(hhi.zcta("01002").unwrap().pct_tree_canopy.is_some());
        let claims = NfipClaimsSnapshot::load(&store).unwrap();
        assert_eq!(claims.towns.len(), 351);
        assert!(claims.town("Scituate").unwrap().claims > 1000);
        let mvp = MvpSnapshot::load(&store).unwrap();
        assert_eq!(mvp.towns.len(), 351);
        assert!(mvp.town("Boston").unwrap().designated_through_prior_work);
    }

    #[test]
    fn snapshot_csv_converts_to_openfema_json() {
        let store = SnapshotStore::bundled();
        let (text, _, _) = store
            .file_text(NFIP_SNAPSHOT_ID, NFIP_COMMUNITIES_FILE)
            .unwrap();
        let json = snapshot_as_json(&text, "NfipCommunityStatusBook").unwrap();
        let c = parse_communities(&json).unwrap();
        assert!(c.len() > 300);
        assert!(c.iter().any(|x| x.participating && x.crs_class.is_some()));
        let (text, _, _) = store.file_text(HMP_SNAPSHOT_ID, HMP_FILE).unwrap();
        let json = snapshot_as_json(&text, "HazardMitigationPlanStatuses").unwrap();
        let p = parse_plans(&json).unwrap();
        assert!(p.len() > 400);
        assert!(p.iter().any(|x| x.plan_status == "Approved"));
        assert!(validator("HazardMitigationPlanStatuses")(&json).is_ok());
        assert!(validator("HazardMitigationPlanStatuses")(b"<html></html>").is_err());
        assert!(validator("HazardMitigationPlanStatuses")(b"{\"HazardMitigationPlanStatuses\":[]}").is_err());
    }
}
