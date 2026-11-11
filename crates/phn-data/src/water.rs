//! Drinking water data for Massachusetts (Phase 14, ENV-03 and CHR-07).
//!
//! * **EPA SDWIS/Fed** through the Envirofacts data service
//!   (`data.epa.gov/dmapservice/sdwis.*`): public water systems, violations,
//!   and lead and copper 90th percentile results. Live for one town at a time
//!   (systems are selected by PWSID prefix: `MA` + MassDEP region digit +
//!   three-digit MassGIS town number), with the bundled `epa-sdwis-ma`
//!   snapshot as the fallback. Operator names, emails, and phone numbers in
//!   the `WATER_SYSTEM` rows are never parsed or kept.
//! * **MassDEP PFAS6** results for finished water from the EEA Data Portal
//!   API (`eeaonline.eea.state.ma.us/EEA/DataLake/.../drinkingWater`), live by
//!   town with the `massdep-pfas6-ma` snapshot (24 months) as the fallback.
//! * **CDC My Water's Fluoride** (`cdc-mwf-ma`) and the **DPH 2023 list of
//!   towns by fluoridation status** (`dph-fluoridation-towns`): snapshots only.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apifetch::{FetchOptions, expect_json, fetch_with_options, reason};
use crate::cache::Cache;
use crate::http::HttpClient;
use crate::snapshots::{SnapshotEntry, SnapshotStore};

/// Snapshot ids.
pub const SDWIS_SNAPSHOT: &str = "epa-sdwis-ma";
pub const PFAS_SNAPSHOT: &str = "massdep-pfas6-ma";
pub const MWF_SNAPSHOT: &str = "cdc-mwf-ma";
pub const DPH_FLUORIDATION_SNAPSHOT: &str = "dph-fluoridation-towns";

/// Envirofacts data service (table queries).
pub const DMAP_BASE: &str = "https://data.epa.gov/dmapservice";
/// EEA Data Portal drinking water results API.
pub const EEA_API: &str =
    "https://eeaonline.eea.state.ma.us/EEA/DataLake/V1.0/DataLakeAPI/drinkingWater";
/// EEA Data Portal search page for people.
pub const EEA_PORTAL: &str = "https://eeaonline.eea.state.ma.us/portal#!/search/drinking-water";
/// SDWIS changes quarterly; PFAS results are posted as labs report.
pub const TTL_SECS: u64 = 24 * 3600;
/// How far back live PFAS6 results are requested (matches the snapshot).
pub const PFAS_DAYS: i64 = 730;

/// An active public water system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WaterSystem {
    pub pwsid: String,
    pub name: String,
    /// "CWS" (community), "NTNCWS" (non-transient non-community: schools,
    /// workplaces), or "TNCWS" (transient non-community: restaurants, campgrounds).
    pub system_type: String,
    /// Massachusetts town in the PWSID (or named, for a tribal system).
    pub town: String,
    pub population: Option<u64>,
    pub connections: Option<u64>,
    /// SDWIS primary source code (GW, SW, GU, GWP, SWP, GUP; P = purchased).
    pub source: String,
    /// Owner type code (L local government, P private, S state, F federal, M public/private, N Native American).
    pub owner: String,
    pub wholesaler: bool,
    pub school_or_child_care: bool,
    /// Primacy agency: "MA" (MassDEP) or "01" (EPA Region 1, tribal systems).
    pub primacy: String,
}

/// One SDWIS violation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Violation {
    pub pwsid: String,
    /// Compliance period begin and end (YYYY-MM-DD).
    pub begin: String,
    pub end: Option<String>,
    pub code: String,
    /// MCL, MRDL, TT, MR, MON, RPT, or Other.
    pub category: String,
    pub health_based: bool,
    pub contaminant: String,
    pub rule: String,
    /// R returned to compliance, K known, O open, I system inactive.
    pub status: String,
    pub returned_to_compliance: Option<String>,
    pub measure: Option<f64>,
    pub unit: Option<String>,
    pub notice_tier: Option<u8>,
}

/// A lead (PB90) or copper (CU90) 90th percentile for a monitoring period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeadCopper {
    pub pwsid: String,
    /// "PB90" or "CU90".
    pub contaminant: String,
    pub period_start: String,
    pub period_end: String,
    pub value_mg_l: f64,
}

/// One PFAS6 result for finished water.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pfas6Result {
    pub pwsid: String,
    pub date: String,
    pub location_code: String,
    pub location: String,
    /// ng/L; None when not detected (ND).
    pub result_ng_l: Option<f64>,
}

/// Code descriptions (violation, category, contaminant, rule, ...).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Codes {
    map: HashMap<(String, String), String>,
}

impl Codes {
    /// Description of `code` of `kind` (for example "CONTAMINANT_CODE").
    pub fn get(&self, kind: &str, code: &str) -> Option<&str> {
        self.map
            .get(&(kind.to_string(), code.to_string()))
            .map(String::as_str)
    }
}

/// The bundled SDWIS snapshot.
#[derive(Debug, Clone)]
pub struct SdwisSnapshot {
    pub systems: Vec<WaterSystem>,
    pub violations: Vec<Violation>,
    pub lead_copper: Vec<LeadCopper>,
    pub codes: Codes,
    pub entry: SnapshotEntry,
}

/// The bundled PFAS6 snapshot.
#[derive(Debug, Clone)]
pub struct PfasSnapshot {
    pub results: Vec<Pfas6Result>,
    pub entry: SnapshotEntry,
}

fn records(text: &str) -> Result<(HashMap<String, usize>, Vec<csv::StringRecord>)> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(false)
        .from_reader(text.as_bytes());
    let headers: HashMap<String, usize> = rdr
        .headers()
        .map_err(|e| PhnError::Internal(format!("water snapshot header: {e}")))?
        .iter()
        .enumerate()
        .map(|(i, h)| (h.to_string(), i))
        .collect();
    let rows = rdr
        .records()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| PhnError::Internal(format!("water snapshot row: {e}")))?;
    Ok((headers, rows))
}

struct Row<'a> {
    h: &'a HashMap<String, usize>,
    r: &'a csv::StringRecord,
}

impl Row<'_> {
    fn s(&self, k: &str) -> String {
        self.h
            .get(k)
            .and_then(|i| self.r.get(*i))
            .unwrap_or("")
            .trim()
            .to_string()
    }
    fn opt(&self, k: &str) -> Option<String> {
        Some(self.s(k)).filter(|v| !v.is_empty())
    }
    fn f(&self, k: &str) -> Option<f64> {
        self.s(k).parse::<f64>().ok().filter(|v| v.is_finite())
    }
}

fn to_u64(v: Option<f64>) -> Option<u64> {
    v.filter(|x| *x >= 0.0).map(|x| x.round() as u64)
}

/// Parse `ma_water_systems.csv`.
pub fn parse_systems(text: &str) -> Result<Vec<WaterSystem>> {
    let (h, rows) = records(text)?;
    Ok(rows
        .iter()
        .map(|r| {
            let x = Row { h: &h, r };
            WaterSystem {
                pwsid: x.s("pwsid"),
                name: x.s("name"),
                system_type: x.s("type"),
                town: x.s("town"),
                population: to_u64(x.f("population")),
                connections: to_u64(x.f("connections")),
                source: x.s("source"),
                owner: x.s("owner"),
                wholesaler: x.s("wholesaler") == "Y",
                school_or_child_care: x.s("school_or_child_care") == "Y",
                primacy: x.s("primacy"),
            }
        })
        .collect())
}

/// Parse `ma_water_violations.csv`.
pub fn parse_violations(text: &str) -> Result<Vec<Violation>> {
    let (h, rows) = records(text)?;
    Ok(rows
        .iter()
        .map(|r| {
            let x = Row { h: &h, r };
            Violation {
                pwsid: x.s("pwsid"),
                begin: x.s("begin"),
                end: x.opt("end"),
                code: x.s("code"),
                category: x.s("category"),
                health_based: x.s("health_based") == "Y",
                contaminant: x.s("contaminant"),
                rule: x.s("rule"),
                status: x.s("status"),
                returned_to_compliance: x.opt("returned_to_compliance"),
                measure: x.f("measure"),
                unit: x.opt("unit"),
                notice_tier: x.s("notice_tier").parse().ok(),
            }
        })
        .collect())
}

/// Parse `ma_water_lead_copper.csv`.
pub fn parse_lead_copper(text: &str) -> Result<Vec<LeadCopper>> {
    let (h, rows) = records(text)?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let x = Row { h: &h, r };
            Some(LeadCopper {
                pwsid: x.s("pwsid"),
                contaminant: x.s("contaminant"),
                period_start: x.s("period_start"),
                period_end: x.s("period_end"),
                value_mg_l: x.f("value_mg_l")?,
            })
        })
        .collect())
}

/// Parse `ma_water_pfas6.csv`.
pub fn parse_pfas(text: &str) -> Result<Vec<Pfas6Result>> {
    let (h, rows) = records(text)?;
    Ok(rows
        .iter()
        .map(|r| {
            let x = Row { h: &h, r };
            Pfas6Result {
                pwsid: x.s("pwsid"),
                date: x.s("date"),
                location_code: x.s("location_code"),
                location: x.s("location"),
                result_ng_l: x.f("result_ng_l"),
            }
        })
        .collect())
}

/// Parse `sdwis_codes.csv`.
pub fn parse_codes(text: &str) -> Result<Codes> {
    let (h, rows) = records(text)?;
    let mut map = HashMap::new();
    for r in &rows {
        let x = Row { h: &h, r };
        map.insert((x.s("type"), x.s("code")), x.s("description"));
    }
    Ok(Codes { map })
}

impl SdwisSnapshot {
    /// Parse the bundled files.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (s, entry, _) = store.file_text(SDWIS_SNAPSHOT, "ma_water_systems.csv")?;
        let (v, _, _) = store.file_text(SDWIS_SNAPSHOT, "ma_water_violations.csv")?;
        let (l, _, _) = store.file_text(SDWIS_SNAPSHOT, "ma_water_lead_copper.csv")?;
        let (c, _, _) = store.file_text(SDWIS_SNAPSHOT, "sdwis_codes.csv")?;
        Ok(Self {
            systems: parse_systems(&s)?,
            violations: parse_violations(&v)?,
            lead_copper: parse_lead_copper(&l)?,
            codes: parse_codes(&c)?,
            entry,
        })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static SdwisSnapshot> {
        static CELL: OnceLock<std::result::Result<SdwisSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("SDWIS snapshot: {e}")))
    }

    /// A system by PWSID (any spelling: "MA2002000", "ma 2002000", "2002000").
    pub fn system(&self, pwsid: &str) -> Option<&WaterSystem> {
        let p = normalize_pwsid(pwsid)?;
        self.systems.iter().find(|s| s.pwsid == p)
    }
}

impl PfasSnapshot {
    /// Parse the bundled file.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (t, entry, _) = store.file_text(PFAS_SNAPSHOT, "ma_water_pfas6.csv")?;
        Ok(Self {
            results: parse_pfas(&t)?,
            entry,
        })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static PfasSnapshot> {
        static CELL: OnceLock<std::result::Result<PfasSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("PFAS6 snapshot: {e}")))
    }
}

/// Normalize a PWSID: "MA" + seven digits for Massachusetts systems, nine
/// digits for tribal systems. "2002000" (EEA style) becomes "MA2002000".
pub fn normalize_pwsid(s: &str) -> Option<String> {
    let t: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    let digits = t.strip_prefix("MA").unwrap_or(&t);
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    match digits.len() {
        7 => Some(format!("MA{digits}")),
        9 if !t.starts_with("MA") => Some(digits.to_string()),
        _ => None,
    }
}

/// The MassGIS town number in a Massachusetts PWSID (digits 4–6).
pub fn pwsid_town_id(pwsid: &str) -> Option<u32> {
    let p = normalize_pwsid(pwsid)?;
    p.strip_prefix("MA")
        .and_then(|d| d.get(1..4))
        .and_then(|t| t.parse().ok())
}

/// The EEA Data Portal spelling of a town (upper case; "MANCHESTER" for
/// Manchester-by-the-Sea).
pub fn eea_town(town: &str) -> String {
    if town.eq_ignore_ascii_case("Manchester-by-the-Sea") {
        "MANCHESTER".into()
    } else {
        town.to_ascii_uppercase()
    }
}

// ------------------------------------------------------------------ live

/// Where one dataset came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DatasetStatus {
    pub mode: SourceMode,
    /// Retrieval time (live or cache) or snapshot date.
    pub as_of: String,
    /// Why the live source was not used, when it was not.
    pub note: Option<String>,
}

/// Everything known about one town's public water systems.
#[derive(Debug, Clone)]
pub struct TownWater {
    pub systems: Vec<WaterSystem>,
    pub violations: Vec<Violation>,
    pub lead_copper: Vec<LeadCopper>,
    pub sdwis: DatasetStatus,
    pub pfas: Vec<Pfas6Result>,
    pub pfas_status: DatasetStatus,
}

/// The four PWSID prefixes a town's systems can have (one per MassDEP region).
pub fn town_prefixes(town_id: u32) -> Vec<String> {
    (1..=4).map(|r| format!("MA{r}{town_id:03}")).collect()
}

/// Envirofacts query URL for a table, filtered to PWSIDs that start with any
/// of the prefixes.
pub fn dmap_url(table: &str, prefixes: &[String]) -> String {
    let filter = prefixes
        .iter()
        .map(|p| format!("pwsid/beginsWith/{p}"))
        .collect::<Vec<_>>()
        .join("/or/");
    format!("{DMAP_BASE}/sdwis.{table}/{filter}/JSON")
}

/// EEA Data Portal URL for a town's finished-water PFAS6 results since `since`.
pub fn eea_pfas_url(town: &str, since: &str) -> String {
    format!(
        "{EEA_API}?Town={}&ChemicalName=PFAS6&RaworFinished=F&FromCollectedDate={since}&_start=0&_end=5000&ColumnName=CollectedDate&Direction=desc",
        crate::socrata::encode(&eea_town(town))
    )
}

fn json_rows(body: &[u8]) -> Result<Vec<serde_json::Map<String, Value>>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("the response was not valid JSON ({e})")))?;
    match v {
        Value::Array(a) => Ok(a
            .into_iter()
            .filter_map(|x| match x {
                Value::Object(o) => Some(o),
                _ => None,
            })
            .collect()),
        Value::Object(o) if o.contains_key("error") => Err(PhnError::Network(format!(
            "Envirofacts answered with an error: {}",
            o.get("error").and_then(Value::as_str).unwrap_or("unknown")
        ))),
        _ => Err(PhnError::Network(
            "Envirofacts answered with something other than a list of rows".into(),
        )),
    }
}

/// Validator for Envirofacts JSON: a JSON array.
pub fn expect_rows(body: &[u8]) -> Result<()> {
    expect_json(body)?;
    json_rows(body).map(|_| ())
}

fn js(o: &serde_json::Map<String, Value>, k: &str) -> String {
    match o.get(k) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn jf(o: &serde_json::Map<String, Value>, k: &str) -> Option<f64> {
    match o.get(k) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|v: &f64| v.is_finite())
}

fn date10(s: &str) -> String {
    s.chars().take(10).collect()
}

/// Parse Envirofacts `sdwis.water_system` rows (active systems only). Only the
/// columns below are read; operator names, emails, and phones are ignored.
pub fn parse_live_systems(body: &[u8], town: &str) -> Result<Vec<WaterSystem>> {
    Ok(json_rows(body)?
        .iter()
        .filter(|o| js(o, "pws_activity_code") == "A")
        .map(|o| WaterSystem {
            pwsid: js(o, "pwsid"),
            name: js(o, "pws_name")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            system_type: js(o, "pws_type_code"),
            town: town.to_string(),
            population: to_u64(jf(o, "population_served_count")),
            connections: to_u64(jf(o, "service_connections_count")),
            source: js(o, "primary_source_code"),
            owner: js(o, "owner_type_code"),
            wholesaler: js(o, "is_wholesaler_ind") == "Y",
            school_or_child_care: js(o, "is_school_or_daycare_ind") == "Y",
            primacy: js(o, "primacy_agency_code"),
        })
        .collect())
}

/// Parse Envirofacts `sdwis.violation` rows.
pub fn parse_live_violations(body: &[u8]) -> Result<Vec<Violation>> {
    Ok(json_rows(body)?
        .iter()
        .map(|o| Violation {
            pwsid: js(o, "pwsid"),
            begin: date10(&js(o, "compl_per_begin_date")),
            end: Some(date10(&js(o, "compl_per_end_date"))).filter(|s| !s.is_empty()),
            code: js(o, "violation_code"),
            category: js(o, "violation_category_code"),
            health_based: js(o, "is_health_based_ind") == "Y",
            contaminant: js(o, "contaminant_code"),
            rule: js(o, "rule_code"),
            status: js(o, "compliance_status_code"),
            returned_to_compliance: Some(date10(&js(o, "rtc_date"))).filter(|s| !s.is_empty()),
            measure: jf(o, "viol_measure"),
            unit: Some(js(o, "unit_of_measure")).filter(|s| !s.is_empty()),
            notice_tier: jf(o, "public_notification_tier").map(|t| t as u8),
        })
        .collect())
}

/// Join Envirofacts `sdwis.lcr_sample_result` to `sdwis.lcr_sample` by PWSID
/// and sample id (sample ids repeat across systems).
pub fn parse_live_lead_copper(results: &[u8], samples: &[u8]) -> Result<Vec<LeadCopper>> {
    let periods: HashMap<(String, String), (String, String)> = json_rows(samples)?
        .iter()
        .map(|o| {
            (
                (js(o, "pwsid"), js(o, "sample_id")),
                (
                    date10(&js(o, "sampling_start_date")),
                    date10(&js(o, "sampling_end_date")),
                ),
            )
        })
        .collect();
    let mut out = Vec::new();
    for o in json_rows(results)? {
        let c = js(&o, "contaminant_code");
        if c != "PB90" && c != "CU90" {
            continue;
        }
        if !js(&o, "unit_of_measure").eq_ignore_ascii_case("mg/l") {
            continue;
        }
        let key = (js(&o, "pwsid"), js(&o, "sample_id"));
        if let (Some((start, end)), Some(v)) = (periods.get(&key), jf(&o, "sample_measure")) {
            out.push(LeadCopper {
                pwsid: key.0,
                contaminant: c,
                period_start: start.clone(),
                period_end: end.clone(),
                value_mg_l: v,
            });
        }
    }
    Ok(out)
}

/// Parse EEA Data Portal `drinkingWater` results (finished PFAS6 only).
pub fn parse_live_pfas(body: &[u8]) -> Result<Vec<Pfas6Result>> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("the response was not valid JSON ({e})")))?;
    let items = v
        .get("Items")
        .and_then(Value::as_array)
        .ok_or_else(|| PhnError::Network("the EEA response had no Items list".into()))?;
    let mut out = Vec::new();
    for i in items.iter().filter_map(Value::as_object) {
        if js(i, "ChemicalName") != "PFAS6" || js(i, "RaworFinished") != "F" {
            continue;
        }
        if !js(i, "LowerDetectLimit").eq_ignore_ascii_case("NG/L") {
            continue;
        }
        let res = js(i, "Result").to_ascii_uppercase();
        let result_ng_l = if res == "ND" {
            None
        } else {
            match res.parse::<f64>() {
                Ok(v) if v.is_finite() && v >= 0.0 => Some(v),
                _ => continue,
            }
        };
        let Some(pwsid) = normalize_pwsid(&js(i, "PWSId")) else {
            continue;
        };
        out.push(Pfas6Result {
            pwsid,
            date: date10(&js(i, "CollectedDate")),
            location_code: js(i, "SampleLocCode"),
            location: js(i, "LocationName")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            result_ng_l,
        });
    }
    Ok(out)
}

/// Validator for the EEA response.
pub fn expect_eea(body: &[u8]) -> Result<()> {
    expect_json(body)?;
    parse_live_pfas(body).map(|_| ())
}

async fn live(
    http: &HttpClient,
    cache: Option<&Cache>,
    url: &str,
    validate: &(dyn Fn(&[u8]) -> Result<()> + Sync),
) -> std::result::Result<(Vec<u8>, SourceMode, String, Option<String>), String> {
    match fetch_with_options(
        http,
        cache,
        url,
        TTL_SECS,
        validate,
        None,
        FetchOptions::default(),
    )
    .await
    {
        Ok(f) => Ok((f.body, f.mode, f.as_of, f.fallback_reason)),
        Err(e) => Err(reason(&e.error)),
    }
}

/// Systems, violations, lead and copper, and PFAS6 results for one town:
/// live (or cached) when the sources answer, otherwise the bundled snapshots.
/// Tribal systems (EPA Region 1 primacy) always come from the snapshot.
pub async fn fetch_town(
    http: &HttpClient,
    cache: Option<&Cache>,
    store: &SnapshotStore,
    town_id: u32,
    town: &str,
) -> Result<TownWater> {
    let snap = SdwisSnapshot::shared(store)?;
    let prefixes = town_prefixes(town_id);
    let in_town = |p: &str| prefixes.iter().any(|x| p.starts_with(x.as_str()));
    let tribal: Vec<WaterSystem> = snap
        .systems
        .iter()
        .filter(|s| s.primacy != "MA" && s.town == town)
        .cloned()
        .collect();
    let tribal_ids: Vec<String> = tribal.iter().map(|s| s.pwsid.clone()).collect();

    // SDWIS: four queries; all must succeed for live data, else the snapshot.
    let mut sdwis_live = None;
    let mut sdwis_error = None;
    let urls = [
        dmap_url("water_system", &prefixes),
        dmap_url("violation", &prefixes),
        dmap_url("lcr_sample_result", &prefixes),
        dmap_url("lcr_sample", &prefixes),
    ];
    let since = (chrono::Local::now().date_naive() - chrono::Duration::days(PFAS_DAYS)).to_string();
    let pfas_url = eea_pfas_url(town, &since);
    // The five requests run at the same time (each Envirofacts query takes a
    // few seconds).
    let (a, b, c, d, pfas_live) = tokio::join!(
        live(http, cache, &urls[0], &expect_rows),
        live(http, cache, &urls[1], &expect_rows),
        live(http, cache, &urls[2], &expect_rows),
        live(http, cache, &urls[3], &expect_rows),
        live(http, cache, &pfas_url, &expect_eea),
    );
    let mut bodies = Vec::new();
    for r in [a, b, c, d] {
        match r {
            Ok(b) => bodies.push(b),
            Err(e) => {
                sdwis_error.get_or_insert(e);
            }
        }
    }
    if bodies.len() == 4 {
        let parsed = (|| -> Result<_> {
            Ok((
                parse_live_systems(&bodies[0].0, town)?,
                parse_live_violations(&bodies[1].0)?,
                parse_live_lead_copper(&bodies[2].0, &bodies[3].0)?,
            ))
        })();
        match parsed {
            Ok(p) => {
                let mode = if bodies.iter().all(|b| b.1 == SourceMode::Live) {
                    SourceMode::Live
                } else {
                    SourceMode::Cache
                };
                let as_of = bodies.iter().map(|b| b.2.clone()).min().unwrap_or_default();
                let note = bodies.iter().find_map(|b| b.3.clone());
                sdwis_live = Some((p, mode, as_of, note));
            }
            Err(e) => sdwis_error = Some(reason(&e)),
        }
    }
    let (systems, violations, lead_copper, sdwis) = match sdwis_live {
        Some(((mut systems, mut violations, mut lead_copper), mode, as_of, note)) => {
            systems.extend(tribal);
            violations.extend(
                snap.violations
                    .iter()
                    .filter(|v| tribal_ids.contains(&v.pwsid))
                    .cloned(),
            );
            lead_copper.extend(
                snap.lead_copper
                    .iter()
                    .filter(|v| tribal_ids.contains(&v.pwsid))
                    .cloned(),
            );
            (
                systems,
                violations,
                lead_copper,
                DatasetStatus { mode, as_of, note },
            )
        }
        None => {
            let ids: Vec<&str> = snap
                .systems
                .iter()
                .filter(|s| in_town(&s.pwsid) || tribal_ids.contains(&s.pwsid))
                .map(|s| s.pwsid.as_str())
                .collect();
            (
                snap.systems
                    .iter()
                    .filter(|s| ids.contains(&s.pwsid.as_str()))
                    .cloned()
                    .collect(),
                snap.violations
                    .iter()
                    .filter(|v| ids.contains(&v.pwsid.as_str()))
                    .cloned()
                    .collect(),
                snap.lead_copper
                    .iter()
                    .filter(|v| ids.contains(&v.pwsid.as_str()))
                    .cloned()
                    .collect(),
                DatasetStatus {
                    mode: SourceMode::Snapshot,
                    as_of: snap.entry.retrieved.clone(),
                    note: sdwis_error,
                },
            )
        }
    };

    // PFAS6.
    let (pfas, pfas_status) = match pfas_live {
        Ok((body, mode, as_of, note)) => {
            (parse_live_pfas(&body)?, DatasetStatus { mode, as_of, note })
        }
        Err(e) => {
            let ps = PfasSnapshot::shared(store)?;
            let ids: Vec<&str> = systems.iter().map(|s| s.pwsid.as_str()).collect();
            (
                ps.results
                    .iter()
                    .filter(|r| in_town(&r.pwsid) || ids.contains(&r.pwsid.as_str()))
                    .cloned()
                    .collect(),
                DatasetStatus {
                    mode: SourceMode::Snapshot,
                    as_of: ps.entry.retrieved.clone(),
                    note: Some(e),
                },
            )
        }
    };
    Ok(TownWater {
        systems,
        violations,
        lead_copper,
        sdwis,
        pfas,
        pfas_status,
    })
}

// ----------------------------------------------------------- fluoridation

/// One CDC My Water's Fluoride community water system record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MwfSystem {
    /// PWSID as CDC lists it ("MA-2002000").
    pub mwf_pwsid: String,
    pub name: String,
    pub county: String,
    pub population: Option<u64>,
    pub fluoridated: bool,
    /// "Adjusted", "Consecutive", "Natural", or "Non-Adjusted".
    pub status: String,
    /// "Ground", "Surface", or "Mixed".
    pub water_source: String,
    pub town: String,
    /// The matching active SDWIS PWSID, when one matched.
    pub sdwis_pwsid: Option<String>,
    /// How the record was matched (or "duplicate of MA-...").
    pub matched_by: String,
    /// Annual average fluoride level reported for an adjusted system (mg/L).
    pub average_fluoride_mg_l: Option<f64>,
    pub report_year: String,
}

impl MwfSystem {
    /// True when another record for the same SDWIS system has the matching PWSID.
    pub fn is_duplicate(&self) -> bool {
        self.matched_by.starts_with("duplicate of ")
    }
}

/// One row of the CDC monthly summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MwfSummaryRow {
    /// "All community water systems", "Fluoridated", or "Non-fluoridated".
    pub group: String,
    pub status: String,
    pub systems: u64,
    pub population: u64,
}

/// One row of the DPH 2023 list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DphTownStatus {
    pub town: String,
    /// "Fully Fluoridated", "Partially Fluoridated", or "Non-Fluoridated".
    pub status: String,
    pub population: Option<u64>,
    pub year_began: Option<String>,
    /// Receives fluoridated water from the MWRA.
    pub mwra: bool,
    /// DPH's additional information, verbatim.
    pub information: String,
}

/// The bundled fluoridation snapshots.
#[derive(Debug, Clone)]
pub struct FluoridationData {
    pub systems: Vec<MwfSystem>,
    pub summary: Vec<MwfSummaryRow>,
    /// The month of the summary ("November, 2025").
    pub summary_period: String,
    pub mwf_entry: SnapshotEntry,
    pub towns: Vec<DphTownStatus>,
    pub dph_entry: SnapshotEntry,
}

/// Parse `ma_fluoridation_systems.csv`.
pub fn parse_mwf_systems(text: &str) -> Result<Vec<MwfSystem>> {
    let (h, rows) = records(text)?;
    Ok(rows
        .iter()
        .map(|r| {
            let x = Row { h: &h, r };
            MwfSystem {
                mwf_pwsid: x.s("mwf_pwsid"),
                name: x.s("name"),
                county: x.s("county"),
                population: to_u64(x.f("population")),
                fluoridated: x.s("fluoridated").eq_ignore_ascii_case("yes"),
                status: x.s("status"),
                water_source: x.s("water_source"),
                town: x.s("town"),
                sdwis_pwsid: x.opt("sdwis_pwsid"),
                matched_by: x.s("match"),
                average_fluoride_mg_l: x.f("avg_fluoride_mg_l"),
                report_year: x.s("report_year"),
            }
        })
        .collect())
}

/// Parse `ma_fluoridation_summary.csv`.
pub fn parse_mwf_summary(text: &str) -> Result<Vec<MwfSummaryRow>> {
    let (h, rows) = records(text)?;
    Ok(rows
        .iter()
        .map(|r| {
            let x = Row { h: &h, r };
            MwfSummaryRow {
                group: x.s("group"),
                status: x.s("status"),
                systems: to_u64(x.f("systems")).unwrap_or(0),
                population: to_u64(x.f("population")).unwrap_or(0),
            }
        })
        .collect())
}

/// Parse `ma_fluoridation_towns.csv`.
pub fn parse_dph_towns(text: &str) -> Result<Vec<DphTownStatus>> {
    let (h, rows) = records(text)?;
    Ok(rows
        .iter()
        .map(|r| {
            let x = Row { h: &h, r };
            DphTownStatus {
                town: x.s("town"),
                status: x.s("status"),
                population: to_u64(x.f("population")),
                year_began: x
                    .opt("year_began")
                    .filter(|y| y.chars().all(|c| c.is_ascii_digit())),
                mwra: x.s("mwra") == "Y",
                information: x.s("information"),
            }
        })
        .collect())
}

impl FluoridationData {
    /// Parse the bundled files.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (s, mwf_entry, _) = store.file_text(MWF_SNAPSHOT, "ma_fluoridation_systems.csv")?;
        let (m, _, _) = store.file_text(MWF_SNAPSHOT, "ma_fluoridation_summary.csv")?;
        let (t, dph_entry, _) =
            store.file_text(DPH_FLUORIDATION_SNAPSHOT, "ma_fluoridation_towns.csv")?;
        let summary_period = mwf_entry
            .source_vintage
            .split("Monthly Summary Report for ")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .unwrap_or("")
            .trim()
            .to_string();
        Ok(Self {
            systems: parse_mwf_systems(&s)?,
            summary: parse_mwf_summary(&m)?,
            summary_period,
            mwf_entry,
            towns: parse_dph_towns(&t)?,
            dph_entry,
        })
    }

    /// The shared, lazily parsed snapshots.
    pub fn shared(store: &SnapshotStore) -> Result<&'static FluoridationData> {
        static CELL: OnceLock<std::result::Result<FluoridationData, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("fluoridation snapshots: {e}")))
    }

    /// DPH rows for a town (one, or two for a partially fluoridated town).
    pub fn dph_rows(&self, town: &str) -> Vec<&DphTownStatus> {
        self.towns
            .iter()
            .filter(|t| t.town.eq_ignore_ascii_case(town))
            .collect()
    }

    /// CDC records assigned to a town.
    pub fn mwf_for_town(&self, town: &str) -> Vec<&MwfSystem> {
        self.systems
            .iter()
            .filter(|s| s.town.eq_ignore_ascii_case(town))
            .collect()
    }

    /// The summary row for a group and status.
    pub fn summary_row(&self, group: &str, status: &str) -> Option<&MwfSummaryRow> {
        self.summary
            .iter()
            .find(|r| r.group == group && r.status == status)
    }

    /// Citations for the two snapshots.
    pub fn sources(&self) -> Vec<Source> {
        vec![self.mwf_entry.to_source(), self.dph_entry.to_source()]
    }
}

/// Count rows by key (for summaries).
pub fn count_by<T, K: Ord, F: Fn(&T) -> K>(items: &[T], key: F) -> BTreeMap<K, usize> {
    let mut m = BTreeMap::new();
    for i in items {
        *m.entry(key(i)).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pwsids_normalize_and_give_the_town_number() {
        assert_eq!(normalize_pwsid("MA2002000").as_deref(), Some("MA2002000"));
        assert_eq!(normalize_pwsid("ma-2002000").as_deref(), Some("MA2002000"));
        assert_eq!(normalize_pwsid("2002000").as_deref(), Some("MA2002000"));
        assert_eq!(normalize_pwsid("010307001").as_deref(), Some("010307001"));
        assert_eq!(normalize_pwsid("MA20020"), None);
        assert_eq!(normalize_pwsid("Acton"), None);
        assert_eq!(pwsid_town_id("MA2002000"), Some(2));
        assert_eq!(pwsid_town_id("MA3035000"), Some(35));
        assert_eq!(pwsid_town_id("010307001"), None);
        assert_eq!(town_prefixes(8)[0], "MA1008");
        assert_eq!(eea_town("Manchester-by-the-Sea"), "MANCHESTER");
        assert!(
            dmap_url("violation", &town_prefixes(2))
                .ends_with("sdwis.violation/pwsid/beginsWith/MA1002/or/pwsid/beginsWith/MA2002/or/pwsid/beginsWith/MA3002/or/pwsid/beginsWith/MA4002/JSON")
        );
    }

    #[test]
    fn bundled_snapshots_parse() {
        let store = SnapshotStore::bundled();
        let s = SdwisSnapshot::shared(&store).unwrap();
        assert!(s.systems.len() > 1500);
        // Every system has a town except the MWRA wholesale system (town number 000).
        assert!(
            s.systems
                .iter()
                .all(|x| !x.town.is_empty() || x.pwsid.starts_with("MA6000"))
        );
        let acton = s.system("MA2002000").unwrap();
        assert_eq!(acton.town, "Acton");
        assert_eq!(acton.system_type, "CWS");
        assert!(
            s.violations
                .iter()
                .all(|v| v.begin.as_str() >= "2016-01-01")
        );
        assert!(
            s.lead_copper
                .iter()
                .all(|l| l.contaminant == "PB90" || l.contaminant == "CU90")
        );
        assert_eq!(
            s.codes.get("CONTAMINANT_CODE", "2950"),
            Some("TTHM"),
            "trihalomethanes code"
        );
        assert_eq!(s.codes.get("VIOLATION_CODE", "02"), Some("MCL, Average"));
        // The tribal system is placed in Aquinnah.
        assert_eq!(
            s.system("010307001").map(|x| x.town.as_str()),
            Some("Aquinnah")
        );
        let p = PfasSnapshot::shared(&store).unwrap();
        assert!(p.results.len() > 1000);
        assert!(p.results.iter().any(|r| r.result_ng_l.is_none()));
        let f = FluoridationData::shared(&store).unwrap();
        assert_eq!(f.summary_period, "November 2025");
        assert!(f.systems.len() > 500);
        assert_eq!(f.towns.len(), 361);
        assert_eq!(f.dph_rows("Pelham").len(), 2);
        let total = f
            .summary_row("All community water systems", "Total")
            .unwrap();
        assert!(total.population > 6_000_000);
    }

    #[test]
    fn live_json_is_parsed_without_contact_fields() {
        let ws =
            br#"[{"pwsid":"MA2002000","pws_name":"ACTON  WATER DISTRICT","pws_activity_code":"A",
            "pws_type_code":"CWS","population_served_count":23000,"service_connections_count":7000,
            "primary_source_code":"GW","owner_type_code":"L","is_wholesaler_ind":"N",
            "is_school_or_daycare_ind":"N","primacy_agency_code":"MA","admin_name":"PAT EXAMPLE",
            "email_addr":"pat@example.org","phone_number":"5555550100"},
            {"pwsid":"MA2002099","pws_name":"CLOSED","pws_activity_code":"I"}]"#;
        let s = parse_live_systems(ws, "Acton").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "ACTON WATER DISTRICT");
        assert_eq!(s[0].population, Some(23000));
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("EXAMPLE") && !dbg.contains("5555550100"));

        let v = br#"[{"pwsid":"MA2002000","compl_per_begin_date":"2024-01-01 00:00:00",
            "compl_per_end_date":"2024-03-31 00:00:00","violation_code":"02",
            "violation_category_code":"MCL","is_health_based_ind":"Y","contaminant_code":"2950",
            "rule_code":"220","compliance_status_code":"R","rtc_date":"2024-06-01 00:00:00",
            "viol_measure":0.0839,"unit_of_measure":"MG/L","public_notification_tier":2}]"#;
        let v = parse_live_violations(v).unwrap();
        assert_eq!(v[0].begin, "2024-01-01");
        assert_eq!(v[0].notice_tier, Some(2));
        assert!(v[0].health_based);

        // Sample ids repeat across systems: join on both.
        let r = br#"[{"pwsid":"MA2002000","sample_id":"1434","contaminant_code":"PB90","sample_measure":0.004,"unit_of_measure":"mg/L"},
                     {"pwsid":"MA2002000","sample_id":"1435","contaminant_code":"CU90","sample_measure":0.3,"unit_of_measure":"mg/L"}]"#;
        let sm = br#"[{"pwsid":"MA9999000","sample_id":"1434","sampling_start_date":"2010-01-01 00:00:00","sampling_end_date":"2010-12-31 00:00:00"},
                      {"pwsid":"MA2002000","sample_id":"1434","sampling_start_date":"2025-01-01 00:00:00","sampling_end_date":"2025-12-31 00:00:00"},
                      {"pwsid":"MA2002000","sample_id":"1435","sampling_start_date":"2025-01-01 00:00:00","sampling_end_date":"2025-12-31 00:00:00"}]"#;
        let l = parse_live_lead_copper(r, sm).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].period_end, "2025-12-31");

        assert!(expect_rows(br#"{"error": "table not available"}"#).is_err());
        assert!(expect_rows(b"[]").is_ok());
        assert!(expect_rows(b"<html>blocked</html>").is_err());

        let p = br#"{"Items":[{"PWSId":"2002000","ChemicalName":"PFAS6","RaworFinished":"F","LowerDetectLimit":"NG/L",
            "Result":"7.4","CollectedDate":"2026-09-22T00:00:00","SampleLocCode":"MULT17","LocationName":"WTP  AFTER"},
            {"PWSId":"2002000","ChemicalName":"PFAS6","RaworFinished":"F","LowerDetectLimit":"NG/L","Result":"ND",
            "CollectedDate":"2026-09-28T00:00:00","SampleLocCode":"X","LocationName":"Y"},
            {"PWSId":"2002000","ChemicalName":"PFAS6","RaworFinished":"R","LowerDetectLimit":"NG/L","Result":"50",
            "CollectedDate":"2026-09-28T00:00:00","SampleLocCode":"RAW","LocationName":"Raw well"}],"TotalCount":3}"#;
        let p = parse_live_pfas(p).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].pwsid, "MA2002000");
        assert_eq!(p[0].result_ng_l, Some(7.4));
        assert_eq!(p[0].location, "WTP AFTER");
        assert_eq!(p[1].result_ng_l, None);
        assert!(expect_eea(b"[]").is_err());
    }

    #[tokio::test]
    async fn offline_town_lookup_uses_the_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let http = HttpClient::new(&phn_core::Config::for_root(dir.path())).unwrap();
        let store = SnapshotStore::bundled();
        let t = fetch_town(&http, None, &store, 2, "Acton").await.unwrap();
        assert_eq!(t.sdwis.mode, SourceMode::Snapshot);
        assert_eq!(t.pfas_status.mode, SourceMode::Snapshot);
        assert!(t.sdwis.note.as_deref().unwrap_or("").contains("offline"));
        assert!(t.systems.iter().any(|s| s.pwsid == "MA2002000"));
        assert!(t.systems.iter().all(|s| pwsid_town_id(&s.pwsid) == Some(2)));
        assert!(t.pfas.iter().all(|r| pwsid_town_id(&r.pwsid) == Some(2)));
        // Aquinnah's tribal system (EPA Region 1 primacy) comes from the snapshot.
        let a = fetch_town(&http, None, &store, 104, "Aquinnah")
            .await
            .unwrap();
        assert!(a.systems.iter().any(|s| s.pwsid == "010307001"));
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Run with `cargo test -p phn-data --lib water::live_tests -- --ignored`
    /// (needs data.epa.gov and eeaonline.eea.state.ma.us).
    #[tokio::test]
    #[ignore]
    async fn live_town_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = phn_core::Config::for_root(dir.path());
        config.offline = false;
        let http = HttpClient::new(&config).unwrap();
        let store = SnapshotStore::bundled();
        let t = fetch_town(&http, None, &store, 2, "Acton").await.unwrap();
        assert_eq!(t.sdwis.mode, SourceMode::Live, "{:?}", t.sdwis.note);
        assert_eq!(
            t.pfas_status.mode,
            SourceMode::Live,
            "{:?}",
            t.pfas_status.note
        );
        assert!(t.systems.iter().any(|s| s.pwsid == "MA2002000"));
        assert!(!t.pfas.is_empty());
        eprintln!(
            "{} systems, {} violations, {} lead/copper, {} PFAS6",
            t.systems.len(),
            t.violations.len(),
            t.lead_copper.len(),
            t.pfas.len()
        );
    }
}
