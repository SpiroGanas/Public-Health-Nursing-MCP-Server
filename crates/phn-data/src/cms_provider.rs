//! CMS Provider Data Catalog adapter (data.cms.gov/provider-data) for
//! Massachusetts nursing homes: Provider Information (star ratings,
//! staffing, inspections, penalties), Health Deficiencies (infection control
//! citations), the SNF Quality Reporting Program (staff and resident
//! vaccination), MDS Quality Measures (resident influenza and pneumococcal
//! vaccination), and State/US Averages.
//!
//! Live data come from the DKAN datastore query API (500 rows a page, cached
//! for a day); the bundled `cms-ma-nursing-homes` snapshot (built by
//! `cargo xtask build-nursing-home-snapshot` with the same functions) is the
//! fallback. Dataset IDs were checked against the catalog on 2026-11-08.

use std::collections::BTreeMap;

use phn_core::{PhnError, Result, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::cache::Cache;
use crate::fetch::fetch_with_fallback;
use crate::http::HttpClient;
use crate::snapshots::SnapshotStore;
use crate::socrata::encode;

pub const API: &str = "https://data.cms.gov/provider-data/api/1/datastore/query";
pub const PROVIDER_INFO: &str = "4pq5-n9py";
pub const HEALTH_DEFICIENCIES: &str = "r5ix-sfxw";
pub const SNF_QRP: &str = "fykj-qjee";
pub const MDS_QM: &str = "djen-97ju";
pub const STATE_AVERAGES: &str = "xcdc-v8bm";
pub const SNAPSHOT_ID: &str = "cms-ma-nursing-homes";
pub const FACILITIES_FILE: &str = "cms_ma_nursing_homes.csv";
pub const CITATIONS_FILE: &str = "cms_ma_nh_infection_citations.csv";
pub const AVERAGES_FILE: &str = "cms_ma_nh_averages.csv";
pub const PAGE: usize = 500;
/// Cache lifetime for live pages (CMS refreshes monthly).
pub const TTL_SECS: u64 = 24 * 3600;

/// SNF QRP measure codes: staff COVID-19 up to date, staff flu, residents COVID-19 up to date.
pub const QRP_VACCINATION: [&str; 3] = ["S_040_02_OBS_RATE", "S_041_01_OBS_RATE", "S_045_01_OBS_RATE"];
/// MDS measure codes: long-stay pneumococcal (415), long-stay flu (454),
/// short-stay pneumococcal (430), short-stay flu (472).
pub const MDS_VACCINATION: [&str; 4] = ["415", "454", "430", "472"];

/// Provider Information columns kept.
pub const PROVIDER_PROPERTIES: &[&str] = &[
    "cms_certification_number_ccn",
    "provider_name",
    "provider_address",
    "citytown",
    "zip_code",
    "telephone_number",
    "countyparish",
    "ownership_type",
    "number_of_certified_beds",
    "average_number_of_residents_per_day",
    "provider_type",
    "special_focus_status",
    "abuse_icon",
    "overall_rating",
    "health_inspection_rating",
    "qm_rating",
    "staffing_rating",
    "reported_rn_staffing_hours_per_resident_per_day",
    "reported_total_nurse_staffing_hours_per_resident_per_day",
    "total_number_of_nurse_staff_hours_per_resident_per_day_on_t_4a14",
    "total_nursing_staff_turnover",
    "registered_nurse_turnover",
    "rating_cycle_1_standard_survey_health_date",
    "rating_cycle_1_total_number_of_health_deficiencies",
    "number_of_citations_from_infection_control_inspections",
    "number_of_fines",
    "total_amount_of_fines_in_dollars",
    "total_number_of_penalties",
    "processing_date",
];

/// A DKAN query condition.
#[derive(Debug, Clone)]
pub struct Condition {
    pub property: &'static str,
    /// "=" or "in".
    pub operator: &'static str,
    pub values: Vec<String>,
}

impl Condition {
    pub fn eq(property: &'static str, value: impl Into<String>) -> Self {
        Self {
            property,
            operator: "=",
            values: vec![value.into()],
        }
    }

    pub fn is_in(property: &'static str, values: &[&str]) -> Self {
        Self {
            property,
            operator: "in",
            values: values.iter().map(|v| (*v).to_string()).collect(),
        }
    }
}

/// The GET URL for one page of a datastore query.
pub fn query_url(
    dataset: &str,
    conditions: &[Condition],
    properties: &[&str],
    offset: usize,
) -> String {
    let mut q = vec![format!("limit={PAGE}"), format!("offset={offset}")];
    for (i, c) in conditions.iter().enumerate() {
        q.push(format!("{}={}", encode(&format!("conditions[{i}][property]")), encode(c.property)));
        q.push(format!("{}={}", encode(&format!("conditions[{i}][operator]")), encode(c.operator)));
        if c.operator == "in" {
            for (j, v) in c.values.iter().enumerate() {
                q.push(format!("{}={}", encode(&format!("conditions[{i}][value][{j}]")), encode(v)));
            }
        } else {
            q.push(format!(
                "{}={}",
                encode(&format!("conditions[{i}][value]")),
                encode(c.values.first().map(String::as_str).unwrap_or_default())
            ));
        }
    }
    for (i, p) in properties.iter().enumerate() {
        q.push(format!("{}={}", encode(&format!("properties[{i}]")), encode(p)));
    }
    format!("{API}/{dataset}/0?{}", q.join("&"))
}

/// Parse one page: the total count and the rows.
pub fn parse_page(body: &[u8]) -> Result<(usize, Vec<Map<String, Value>>)> {
    let v: Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("CMS datastore returned invalid JSON: {e}")))?;
    let count = v["count"].as_u64().unwrap_or(0) as usize;
    let rows = v["results"]
        .as_array()
        .ok_or_else(|| PhnError::Network("CMS datastore response has no results".into()))?
        .iter()
        .filter_map(|r| r.as_object().cloned())
        .collect();
    Ok((count, rows))
}

fn s(r: &Map<String, Value>, k: &str) -> String {
    match r.get(k) {
        Some(Value::String(x)) => x.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn num(r: &Map<String, Value>, k: &str) -> Option<f64> {
    s(r, k).parse::<f64>().ok().filter(|x| x.is_finite())
}

/// One Massachusetts nursing home.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NursingHome {
    pub ccn: String,
    pub name: String,
    pub address: String,
    pub city: String,
    pub zip: String,
    pub county: String,
    pub phone: String,
    pub ownership_type: String,
    pub certified_beds: Option<f64>,
    pub average_residents_per_day: Option<f64>,
    pub provider_type: String,
    /// "SFF" or "SFF Candidate" when on the Special Focus Facility list.
    pub special_focus_status: String,
    /// CMS abuse icon (citation for abuse).
    pub abuse_icon: bool,
    pub overall_rating: Option<f64>,
    pub health_inspection_rating: Option<f64>,
    pub quality_measure_rating: Option<f64>,
    pub staffing_rating: Option<f64>,
    pub rn_hours_per_resident_day: Option<f64>,
    pub total_nurse_hours_per_resident_day: Option<f64>,
    pub weekend_nurse_hours_per_resident_day: Option<f64>,
    pub nursing_staff_turnover_pct: Option<f64>,
    pub rn_turnover_pct: Option<f64>,
    pub last_standard_survey: String,
    pub health_deficiencies_last_survey: Option<f64>,
    pub infection_control_inspection_citations: Option<f64>,
    pub fines: Option<f64>,
    pub fines_total_dollars: Option<f64>,
    pub penalties: Option<f64>,
    /// SNF QRP: % of health care personnel up to date with COVID-19 vaccines.
    pub staff_covid_up_to_date_pct: Option<f64>,
    /// SNF QRP: % of health care personnel who got the current season's flu vaccine.
    pub staff_flu_pct: Option<f64>,
    /// SNF QRP: % of residents up to date with COVID-19 vaccines.
    pub resident_covid_up_to_date_pct: Option<f64>,
    /// MDS: % of long-stay residents assessed and appropriately given the flu vaccine.
    pub long_stay_flu_pct: Option<f64>,
    /// MDS: % of long-stay residents assessed and appropriately given the pneumococcal vaccine.
    pub long_stay_pneumococcal_pct: Option<f64>,
    pub short_stay_flu_pct: Option<f64>,
    pub short_stay_pneumococcal_pct: Option<f64>,
    /// Measurement periods for the vaccination measures, e.g. "SNF QRP to 12/31/2025; MDS 2025Q3-2026Q2".
    pub vaccination_periods: String,
    pub processing_date: String,
}

/// An infection control citation (Health Deficiencies, category "Infection Control Deficiencies").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InfectionCitation {
    pub ccn: String,
    pub survey_date: String,
    /// F-tag, e.g. "F0880".
    pub tag: String,
    pub description: String,
    /// CMS scope and severity code (A-L).
    pub scope_severity: String,
    pub corrected: String,
    pub inspection_cycle: String,
    /// Found on an infection control inspection.
    pub infection_control_inspection: bool,
    /// Found on a complaint inspection.
    pub complaint: bool,
}

/// State or national averages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Averages {
    /// "MA" or "NATION".
    pub level: String,
    pub overall_rating: Option<f64>,
    pub health_inspection_rating: Option<f64>,
    pub quality_measure_rating: Option<f64>,
    pub staffing_rating: Option<f64>,
    pub total_nurse_hours_per_resident_day: Option<f64>,
    pub rn_hours_per_resident_day: Option<f64>,
    pub nursing_staff_turnover_pct: Option<f64>,
    pub rn_turnover_pct: Option<f64>,
}

/// Build facilities from Provider Information rows plus vaccination measure rows.
pub fn build_facilities(
    provider: &[Map<String, Value>],
    qrp: &[Map<String, Value>],
    mds: &[Map<String, Value>],
) -> Vec<NursingHome> {
    let mut q: BTreeMap<(String, String), (Option<f64>, String)> = BTreeMap::new();
    for r in qrp {
        q.insert(
            (s(r, "cms_certification_number_ccn"), s(r, "measure_code")),
            (num(r, "score"), s(r, "end_date")),
        );
    }
    let mut m: BTreeMap<(String, String), (Option<f64>, String)> = BTreeMap::new();
    for r in mds {
        m.insert(
            (s(r, "cms_certification_number_ccn"), s(r, "measure_code")),
            (num(r, "four_quarter_average_score"), s(r, "measure_period")),
        );
    }
    let mut out: Vec<NursingHome> = provider
        .iter()
        .map(|r| {
            let ccn = s(r, "cms_certification_number_ccn");
            let qv = |code: &str| q.get(&(ccn.clone(), code.to_string())).and_then(|v| v.0);
            let mv = |code: &str| m.get(&(ccn.clone(), code.to_string())).and_then(|v| v.0);
            let q_end = QRP_VACCINATION
                .iter()
                .filter_map(|c| q.get(&(ccn.clone(), (*c).to_string())).map(|v| v.1.clone()))
                .find(|e| !e.is_empty());
            let m_period = MDS_VACCINATION
                .iter()
                .filter_map(|c| m.get(&(ccn.clone(), (*c).to_string())).map(|v| v.1.clone()))
                .find(|e| !e.is_empty());
            let mut periods = Vec::new();
            if let Some(e) = q_end {
                periods.push(format!("staff and resident COVID-19 and staff flu (SNF QRP) through {e}"));
            }
            if let Some(p) = m_period {
                periods.push(format!("resident flu and pneumococcal (MDS) {p}"));
            }
            NursingHome {
                name: s(r, "provider_name"),
                address: s(r, "provider_address"),
                city: s(r, "citytown"),
                zip: s(r, "zip_code"),
                county: s(r, "countyparish"),
                phone: s(r, "telephone_number"),
                ownership_type: s(r, "ownership_type"),
                certified_beds: num(r, "number_of_certified_beds"),
                average_residents_per_day: num(r, "average_number_of_residents_per_day"),
                provider_type: s(r, "provider_type"),
                special_focus_status: s(r, "special_focus_status"),
                abuse_icon: s(r, "abuse_icon") == "Y",
                overall_rating: num(r, "overall_rating"),
                health_inspection_rating: num(r, "health_inspection_rating"),
                quality_measure_rating: num(r, "qm_rating"),
                staffing_rating: num(r, "staffing_rating"),
                rn_hours_per_resident_day: num(r, "reported_rn_staffing_hours_per_resident_per_day"),
                total_nurse_hours_per_resident_day: num(r, "reported_total_nurse_staffing_hours_per_resident_per_day"),
                weekend_nurse_hours_per_resident_day: num(r, "total_number_of_nurse_staff_hours_per_resident_per_day_on_t_4a14"),
                nursing_staff_turnover_pct: num(r, "total_nursing_staff_turnover"),
                rn_turnover_pct: num(r, "registered_nurse_turnover"),
                last_standard_survey: s(r, "rating_cycle_1_standard_survey_health_date"),
                health_deficiencies_last_survey: num(r, "rating_cycle_1_total_number_of_health_deficiencies"),
                infection_control_inspection_citations: num(r, "number_of_citations_from_infection_control_inspections"),
                fines: num(r, "number_of_fines"),
                fines_total_dollars: num(r, "total_amount_of_fines_in_dollars"),
                penalties: num(r, "total_number_of_penalties"),
                staff_covid_up_to_date_pct: qv(QRP_VACCINATION[0]),
                staff_flu_pct: qv(QRP_VACCINATION[1]),
                resident_covid_up_to_date_pct: qv(QRP_VACCINATION[2]),
                long_stay_pneumococcal_pct: mv(MDS_VACCINATION[0]),
                long_stay_flu_pct: mv(MDS_VACCINATION[1]),
                short_stay_pneumococcal_pct: mv(MDS_VACCINATION[2]),
                short_stay_flu_pct: mv(MDS_VACCINATION[3]),
                vaccination_periods: periods.join("; "),
                processing_date: s(r, "processing_date"),
                ccn,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.ccn.cmp(&b.ccn)));
    out
}

/// Infection control citations from Health Deficiencies rows.
pub fn build_citations(rows: &[Map<String, Value>]) -> Vec<InfectionCitation> {
    let mut out: Vec<InfectionCitation> = rows
        .iter()
        .map(|r| InfectionCitation {
            ccn: s(r, "cms_certification_number_ccn"),
            survey_date: s(r, "survey_date"),
            tag: format!("{}{}", s(r, "deficiency_prefix"), s(r, "deficiency_tag_number")),
            description: s(r, "deficiency_description"),
            scope_severity: s(r, "scope_severity_code"),
            corrected: s(r, "deficiency_corrected"),
            inspection_cycle: s(r, "inspection_cycle"),
            infection_control_inspection: s(r, "infection_control_inspection_deficiency") == "Y",
            complaint: s(r, "complaint_deficiency") == "Y",
        })
        .collect();
    out.sort_by(|a, b| {
        a.ccn
            .cmp(&b.ccn)
            .then(b.survey_date.cmp(&a.survey_date))
            .then(a.tag.cmp(&b.tag))
    });
    out
}

/// MA and national averages from State/US Averages rows.
pub fn build_averages(rows: &[Map<String, Value>]) -> Vec<Averages> {
    rows.iter()
        .map(|r| Averages {
            level: s(r, "state_or_nation"),
            overall_rating: num(r, "overall_rating"),
            health_inspection_rating: num(r, "health_inspection_rating"),
            quality_measure_rating: num(r, "qm_rating"),
            staffing_rating: num(r, "staffing_rating"),
            total_nurse_hours_per_resident_day: num(r, "reported_total_nurse_staffing_hours_per_resident_per_day"),
            rn_hours_per_resident_day: num(r, "reported_rn_staffing_hours_per_resident_per_day"),
            nursing_staff_turnover_pct: num(r, "total_nursing_staff_turnover"),
            rn_turnover_pct: num(r, "registered_nurse_turnover"),
        })
        .collect()
}

/// Serialize records as CSV (snapshot files).
pub fn to_csv<T: Serialize>(items: &[T]) -> Result<String> {
    let mut w = csv::Writer::from_writer(Vec::new());
    for i in items {
        w.serialize(i)
            .map_err(|e| PhnError::Internal(format!("CSV: {e}")))?;
    }
    let bytes = w
        .into_inner()
        .map_err(|e| PhnError::Internal(format!("CSV: {e}")))?;
    String::from_utf8(bytes).map_err(|e| PhnError::Internal(e.to_string()))
}

/// Parse snapshot CSV records.
pub fn from_csv<T: for<'de> Deserialize<'de>>(text: &str) -> Result<Vec<T>> {
    let mut r = csv::Reader::from_reader(text.as_bytes());
    r.deserialize()
        .collect::<std::result::Result<Vec<T>, _>>()
        .map_err(|e| PhnError::Internal(format!("CMS snapshot: {e}")))
}

/// The assembled data and where it came from.
#[derive(Debug, Clone)]
pub struct NursingHomeData {
    pub facilities: Vec<NursingHome>,
    pub citations: Vec<InfectionCitation>,
    pub averages: Vec<Averages>,
    pub mode: SourceMode,
    /// Date of the data (CMS processing date) or of the snapshot.
    pub as_of: String,
    pub fallback_reason: Option<String>,
}

/// The queries the server (and the snapshot builder) runs for Massachusetts.
pub fn ma_queries() -> Vec<(&'static str, Vec<Condition>, Vec<&'static str>)> {
    vec![
        (PROVIDER_INFO, vec![Condition::eq("state", "MA")], PROVIDER_PROPERTIES.to_vec()),
        (
            SNF_QRP,
            vec![
                Condition::eq("state", "MA"),
                Condition::is_in("measure_code", &QRP_VACCINATION),
            ],
            vec!["cms_certification_number_ccn", "measure_code", "score", "end_date"],
        ),
        (
            MDS_QM,
            vec![
                Condition::eq("state", "MA"),
                Condition::is_in("measure_code", &MDS_VACCINATION),
            ],
            vec![
                "cms_certification_number_ccn",
                "measure_code",
                "four_quarter_average_score",
                "measure_period",
            ],
        ),
        (
            HEALTH_DEFICIENCIES,
            vec![
                Condition::eq("state", "MA"),
                Condition::eq("deficiency_category", "Infection Control Deficiencies"),
            ],
            vec![
                "cms_certification_number_ccn",
                "survey_date",
                "deficiency_prefix",
                "deficiency_tag_number",
                "deficiency_description",
                "scope_severity_code",
                "deficiency_corrected",
                "inspection_cycle",
                "infection_control_inspection_deficiency",
                "complaint_deficiency",
            ],
        ),
        (
            STATE_AVERAGES,
            vec![Condition::is_in("state_or_nation", &["MA", "NATION"])],
            Vec::new(),
        ),
    ]
}

/// Assemble the result sets of [`ma_queries`] (in that order).
pub fn assemble(rows: &[Vec<Map<String, Value>>]) -> (Vec<NursingHome>, Vec<InfectionCitation>, Vec<Averages>) {
    (
        build_facilities(&rows[0], &rows[1], &rows[2]),
        build_citations(&rows[3]),
        build_averages(&rows[4]),
    )
}

/// Fetch every page of a query through the cache.
async fn fetch_all(
    http: &HttpClient,
    cache: Option<&Cache>,
    dataset: &str,
    conditions: &[Condition],
    properties: &[&str],
) -> Result<(Vec<Map<String, Value>>, SourceMode)> {
    let mut all = Vec::new();
    let mut offset = 0;
    let mut mode = SourceMode::Live;
    loop {
        let url = query_url(dataset, conditions, properties, offset);
        let f = fetch_with_fallback(http, cache, &url, TTL_SECS, None).await?;
        if f.mode == SourceMode::Cache {
            mode = SourceMode::Cache;
        }
        let (count, rows) = parse_page(&f.body)?;
        let n = rows.len();
        all.extend(rows);
        offset += n;
        if n == 0 || offset >= count || offset > 20_000 {
            break;
        }
    }
    Ok((all, mode))
}

/// Live (or cached) Massachusetts nursing home data.
pub async fn load_live(http: &HttpClient, cache: Option<&Cache>) -> Result<NursingHomeData> {
    let mut sets = Vec::new();
    let mut mode = SourceMode::Live;
    for (dataset, conds, props) in ma_queries() {
        let (rows, m) = fetch_all(http, cache, dataset, &conds, &props).await?;
        if m == SourceMode::Cache {
            mode = SourceMode::Cache;
        }
        sets.push(rows);
    }
    if sets[0].is_empty() {
        return Err(PhnError::Network("CMS Provider Information returned no Massachusetts rows".into()));
    }
    let (facilities, citations, averages) = assemble(&sets);
    let as_of = facilities
        .iter()
        .map(|f| f.processing_date.clone())
        .max()
        .unwrap_or_default();
    Ok(NursingHomeData {
        facilities,
        citations,
        averages,
        mode,
        as_of,
        fallback_reason: None,
    })
}

/// The bundled snapshot.
pub fn load_snapshot(store: &SnapshotStore) -> Result<NursingHomeData> {
    let (f, entry, _) = store.file_text(SNAPSHOT_ID, FACILITIES_FILE)?;
    let (c, _, _) = store.file_text(SNAPSHOT_ID, CITATIONS_FILE)?;
    let (a, _, _) = store.file_text(SNAPSHOT_ID, AVERAGES_FILE)?;
    Ok(NursingHomeData {
        facilities: from_csv(&f)?,
        citations: from_csv(&c)?,
        averages: from_csv(&a)?,
        mode: SourceMode::Snapshot,
        as_of: entry.source_vintage.clone(),
        fallback_reason: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_query_urls() {
        let u = query_url(
            SNF_QRP,
            &[Condition::eq("state", "MA"), Condition::is_in("measure_code", &["A", "B"])],
            &["score"],
            500,
        );
        assert!(u.starts_with("https://data.cms.gov/provider-data/api/1/datastore/query/fykj-qjee/0?limit=500&offset=500"));
        assert!(u.contains("conditions%5B1%5D%5Bvalue%5D%5B1%5D=B"));
        assert!(u.contains("properties%5B0%5D=score"));
    }

    #[test]
    fn snapshot_loads_and_joins_vaccination_measures() {
        let d = load_snapshot(&SnapshotStore::bundled()).unwrap();
        assert!(d.facilities.len() > 300, "{}", d.facilities.len());
        assert!(d.facilities.iter().filter(|f| f.staff_flu_pct.is_some()).count() > 200);
        assert!(d.facilities.iter().filter(|f| f.long_stay_flu_pct.is_some()).count() > 200);
        assert!(d.citations.iter().any(|c| c.tag == "F0880"));
        assert!(d.averages.iter().any(|a| a.level == "MA"));
        assert!(d.averages.iter().any(|a| a.level == "NATION"));
    }
}
