//! MCH-06 `maternal_infant_health_profile`.
//!
//! Births, low birthweight, preterm birth, prenatal care, infant mortality, and Massachusetts
//! PRAMS indicators from DPH's vital statistics and PRAMS reports, by community (births, low
//! birthweight, preterm, teen births) and by race and Hispanic ethnicity (statewide), compared
//! with Massachusetts and U.S. figures (CDC NCHS provisional). Snapshot `ma-maternal-infant`.

use std::collections::BTreeMap;

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::rates::{RateFlag, rate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Arguments for `maternal_infant_health_profile`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct MaternalInfantArgs {
    /// A city or town for the community profile (births, low birthweight, preterm, teen
    /// births in 2023).
    #[serde(default)]
    pub town: Option<String>,
    /// A county instead of a town ("Hampden" or "Hampden County").
    #[serde(default)]
    pub county: Option<String>,
    /// Include statewide figures by race and Hispanic ethnicity (default true).
    #[serde(default)]
    pub by_race: Option<bool>,
    /// Include trends (2012/2013 onward) (default true).
    #[serde(default)]
    pub trends: Option<bool>,
}

/// One snapshot row.
#[derive(Debug, Clone, Deserialize)]
struct Row {
    geo_type: String,
    geography: String,
    indicator: String,
    group: String,
    period: String,
    numerator: String,
    denominator: String,
    value: String,
    lower: String,
    upper: String,
    unit: String,
    source: String,
}

/// A community indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommunityIndicator {
    pub indicator: String,
    pub period: String,
    /// Percent or count; none when suppressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Exact Poisson 95% interval for percentages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper: Option<f64>,
    /// Count behind the value ("<5" or "*" when suppressed).
    pub numerator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denominator: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub us_value: Option<f64>,
    /// "higher than the state", "lower than the state", "similar to the state" (interval
    /// excludes or includes the state value), or "not compared".
    pub comparison: String,
    /// Reliability or disclosure notes.
    pub flags: Vec<String>,
}

/// A statewide indicator by group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GroupIndicator {
    pub indicator: String,
    pub period: String,
    pub unit: String,
    /// Group -> value (none when suppressed).
    pub values: BTreeMap<String, Option<f64>>,
    /// Group -> 95% CI text, for PRAMS.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub intervals: BTreeMap<String, String>,
    /// Group -> ratio to White non-Hispanic.
    pub ratio_to_white: BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub us_total: Option<f64>,
    pub source: String,
}

/// A trend series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Trend {
    pub indicator: String,
    pub unit: String,
    /// (period, value).
    pub points: Vec<(String, f64)>,
}

/// Result of `maternal_infant_health_profile`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaternalInfantResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
    pub community_indicators: Vec<CommunityIndicator>,
    pub state_indicators: Vec<GroupIndicator>,
    pub prams: Vec<GroupIndicator>,
    pub trends: Vec<Trend>,
    pub suppression_threshold: u32,
}

const LABELS: &[(&str, &str)] = &[
    ("resident_births", "Resident births"),
    ("low_birthweight_pct", "Low birthweight (<2,500 g), %"),
    ("preterm_pct", "Preterm (<37 weeks), %"),
    ("teen_births", "Births to ages 15-19"),
    (
        "adequate_prenatal_care_pct",
        "Adequate prenatal care (APNCU), %",
    ),
    (
        "first_trimester_care_pct",
        "Prenatal care began in 1st trimester, %",
    ),
    ("cesarean_pct", "Cesarean delivery, %"),
    (
        "breastfeeding_in_hospital_pct",
        "Breastfeeding in the hospital, %",
    ),
    (
        "infant_mortality_rate",
        "Infant mortality per 1,000 live births",
    ),
    ("neonatal_mortality_rate", "Neonatal mortality per 1,000"),
    (
        "postneonatal_mortality_rate",
        "Postneonatal mortality per 1,000",
    ),
    (
        "prams_depression_screening_prenatal_pct",
        "Depression screening in prenatal care, %",
    ),
    (
        "prams_depression_screening_postpartum_pct",
        "Depression screening at the postpartum visit, %",
    ),
    (
        "prams_postpartum_depressive_symptoms_pct",
        "Postpartum depressive symptoms, %",
    ),
    (
        "prams_breastfeeding_initiation_pct",
        "Breastfeeding initiation, %",
    ),
    (
        "prams_breastfeeding_8_weeks_pct",
        "Breastfeeding at least 8 weeks, %",
    ),
    (
        "prams_back_to_sleep_pct",
        "Infant placed on back to sleep, %",
    ),
    ("prams_no_bed_sharing_pct", "No bed-sharing, %"),
    (
        "prams_no_soft_bedding_pct",
        "No soft objects or loose bedding, %",
    ),
    ("prams_unintended_pregnancy_pct", "Unintended pregnancy, %"),
    ("low_birthweight_pct_trend", "Low birthweight, %"),
    ("preterm_pct_trend", "Preterm, %"),
    (
        "adequate_prenatal_care_pct_trend",
        "Adequate prenatal care (APNCU), %",
    ),
];

fn label(id: &str) -> String {
    LABELS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, v)| (*v).to_string())
        .unwrap_or_else(|| id.to_string())
}

const GROUPS: [&str; 7] = [
    "Total",
    "White non-Hispanic",
    "Black non-Hispanic",
    "Hispanic",
    "Asian/Pacific Islander non-Hispanic",
    "Asian non-Hispanic",
    "American Indian/Alaska Native non-Hispanic",
];

fn f(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn source_for(id: &str) -> Source {
    let (title, publisher, url, date) = match id {
        "ma-births-2023" => (
            "Massachusetts Births 2023 (Registry of Vital Records and Statistics)",
            "Massachusetts Department of Public Health",
            "https://www.mass.gov/doc/2023-birth-report/download",
            "2026-08-19",
        ),
        "ma-deaths-2022" => (
            "Massachusetts Deaths 2022, Table 5 (infant mortality by race and Hispanic ethnicity)",
            "Massachusetts Department of Public Health",
            "https://www.mass.gov/doc/2022-death-report-docx/download",
            "2025-01-08",
        ),
        "ma-prams-2019-2021" => (
            "Massachusetts PRAMS 2019-2021 Report",
            "Massachusetts Department of Public Health",
            "https://www.mass.gov/doc/2019-2021-mass-prams-report-docx/download",
            "",
        ),
        "cdc-nchs-vsrr-births" => (
            "NCHS VSRR Quarterly provisional estimates for selected birth indicators (76vv-a7x8)",
            "CDC National Center for Health Statistics",
            "https://data.cdc.gov/resource/76vv-a7x8.json",
            "2024-12-31",
        ),
        _ => (
            "NCHS VSRR Quarterly provisional estimates for infant mortality (jqwm-z2g9)",
            "CDC National Center for Health Statistics",
            "https://data.cdc.gov/resource/jqwm-z2g9.json",
            "2023-12-31",
        ),
    };
    Source {
        title: title.into(),
        publisher: publisher.into(),
        url: Some(url.into()),
        effective_date: (!date.is_empty()).then(|| date.to_string()),
        retrieved: Some("2026-11-09".into()),
        mode: phn_core::SourceMode::Snapshot,
    }
}

/// Maternal and infant health profile for Massachusetts or a community.
pub fn maternal_infant_health_profile(
    ctx: &Context,
    args: MaternalInfantArgs,
) -> Result<Envelope<MaternalInfantResult>> {
    if args.town.is_some() && args.county.is_some() {
        return Err(PhnError::InvalidInput(
            "give a town or a county, not both".into(),
        ));
    }
    let (text, entry, _) = ctx
        .snapshots
        .file_text("ma-maternal-infant", "ma_maternal_infant.csv")?;
    let mut rows: Vec<Row> = Vec::new();
    for r in csv::Reader::from_reader(text.as_bytes()).deserialize::<Row>() {
        rows.push(r.map_err(|e| PhnError::Internal(format!("maternal-infant snapshot: {e}")))?);
    }
    let threshold = ctx.suppression_threshold();
    let community = match (&args.town, &args.county) {
        (Some(t), None) => {
            let m = ctx.ma.town(t).ok_or_else(|| {
                let sugg: Vec<String> = ctx
                    .ma
                    .suggest_towns(t, 3)
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                PhnError::NotFound(format!(
                    "'{t}' is not a Massachusetts city or town{}",
                    if sugg.is_empty() {
                        String::new()
                    } else {
                        format!(". Did you mean {}?", sugg.join(", "))
                    }
                ))
            })?;
            Some(("town", m.name.clone()))
        }
        (None, Some(c)) => {
            let n = phn_core::text::normalize(c);
            let n = n.strip_suffix(" county").unwrap_or(&n).to_string();
            let name = rows
                .iter()
                .filter(|r| r.geo_type == "county")
                .map(|r| r.geography.clone())
                .find(|g| {
                    phn_core::text::normalize(g)
                        .strip_suffix(" county")
                        .is_some_and(|x| x == n)
                })
                .ok_or_else(|| {
                    PhnError::NotFound(format!("'{c}' is not a Massachusetts county"))
                })?;
            Some(("county", name))
        }
        _ => None,
    };
    let state_of = |ind: &str, group: &str| -> Option<f64> {
        rows.iter()
            .find(|r| {
                r.geo_type == "state"
                    && r.indicator == ind
                    && r.group == group
                    && r.period == "2023"
            })
            .and_then(|r| f(&r.value))
    };
    let us_of = |ind: &str, group: &str| -> Option<f64> {
        rows.iter()
            .find(|r| r.geo_type == "nation" && r.indicator == ind && r.group == group)
            .and_then(|r| f(&r.value))
    };

    // ---- Community indicators.
    let mut community_indicators = Vec::new();
    let mut any_suppressed = false;
    if let Some((gt, name)) = &community {
        for r in rows
            .iter()
            .filter(|r| r.geo_type == *gt && &r.geography == name)
        {
            let mut flags = Vec::new();
            let pct = r.indicator.ends_with("_pct");
            let n = f(&r.numerator);
            let d = f(&r.denominator);
            let suppressed =
                r.numerator == "<5" || n.is_some_and(|x| x > 0.0 && (x as u32) < threshold && pct);
            let (mut value, mut lo, mut hi) = (None, None, None);
            let mut numerator = r.numerator.clone();
            if suppressed {
                any_suppressed = true;
                flags.push(format!(
                    "suppressed: fewer than {} events",
                    threshold.max(5)
                ));
                if r.numerator != "<5" {
                    numerator = "*".into();
                }
            } else if pct
                && let (Some(n), Some(d)) = (n, d)
                && d > 0.0
            {
                let rt = rate(&r.indicator, n as u64, d, 100.0, 0.95, threshold);
                value = Some(rt.rate);
                lo = Some(rt.lower);
                hi = Some(rt.upper);
                if rt.flags.contains(&RateFlag::Unreliable) {
                    flags.push("unstable: fewer than 20 events".into());
                }
            } else {
                value = f(&r.value);
            }
            let state_value = if r.indicator == "teen_births" || r.indicator == "resident_births" {
                None
            } else {
                state_of(&r.indicator, "Total")
            };
            let comparison = match (lo, hi, state_value) {
                (Some(l), Some(_), Some(s)) if l > s => "higher than the state".into(),
                (Some(_), Some(h), Some(s)) if h < s => "lower than the state".into(),
                (Some(_), Some(_), Some(_)) => "similar to the state".into(),
                _ => "not compared".into(),
            };
            community_indicators.push(CommunityIndicator {
                indicator: label(&r.indicator),
                period: r.period.clone(),
                value,
                lower: lo,
                upper: hi,
                numerator,
                denominator: d,
                state_value,
                us_value: if pct {
                    us_of(&r.indicator, "Total")
                } else {
                    None
                },
                comparison,
                flags,
            });
        }
    }

    // ---- Statewide by group.
    let by_race = args.by_race.unwrap_or(true);
    let group_ind = |ind: &str, period_pred: &dyn Fn(&str) -> bool| -> Option<GroupIndicator> {
        let sel: Vec<&Row> = rows
            .iter()
            .filter(|r| r.geo_type == "state" && r.indicator == ind && period_pred(&r.period))
            .collect();
        if sel.is_empty() {
            return None;
        }
        let mut values = BTreeMap::new();
        let mut intervals = BTreeMap::new();
        for r in &sel {
            if !by_race && r.group != "Total" {
                continue;
            }
            values.insert(r.group.clone(), f(&r.value));
            if !r.lower.is_empty() {
                intervals.insert(r.group.clone(), format!("{}-{}", r.lower, r.upper));
            }
        }
        let white = values.get("White non-Hispanic").copied().flatten();
        let ratio_to_white = values
            .iter()
            .filter(|(g, _)| g.as_str() != "White non-Hispanic" && g.as_str() != "Total")
            .filter_map(|(g, v)| match (v, white) {
                (Some(v), Some(w)) if w > 0.0 => Some((g.clone(), (v / w * 100.0).round() / 100.0)),
                _ => None,
            })
            .collect();
        Some(GroupIndicator {
            indicator: label(ind),
            period: sel[0].period.clone(),
            unit: sel[0].unit.clone(),
            values,
            intervals,
            ratio_to_white,
            us_total: us_of(ind, "Total"),
            source: sel[0].source.clone(),
        })
    };
    let mut state_indicators = Vec::new();
    for ind in [
        "low_birthweight_pct",
        "preterm_pct",
        "adequate_prenatal_care_pct",
        "first_trimester_care_pct",
        "cesarean_pct",
        "breastfeeding_in_hospital_pct",
    ] {
        if let Some(g) = group_ind(ind, &|p| p == "2023") {
            state_indicators.push(g);
        }
    }
    let latest_im = rows
        .iter()
        .filter(|r| r.indicator == "infant_mortality_rate" && r.geo_type == "state")
        .map(|r| r.period.clone())
        .max()
        .unwrap_or_default();
    for ind in [
        "infant_mortality_rate",
        "neonatal_mortality_rate",
        "postneonatal_mortality_rate",
    ] {
        if let Some(g) = group_ind(ind, &|p| p == latest_im) {
            state_indicators.push(g);
        }
    }
    let mut prams = Vec::new();
    for (id, _) in LABELS.iter().filter(|(k, _)| k.starts_with("prams_")) {
        if let Some(g) = group_ind(id, &|_| true) {
            prams.push(g);
        }
    }
    let mut trends = Vec::new();
    if args.trends.unwrap_or(true) {
        for ind in [
            "low_birthweight_pct_trend",
            "preterm_pct_trend",
            "adequate_prenatal_care_pct_trend",
        ] {
            let points: Vec<(String, f64)> = rows
                .iter()
                .filter(|r| r.indicator == ind)
                .filter_map(|r| f(&r.value).map(|v| (r.period.clone(), v)))
                .collect();
            trends.push(Trend {
                indicator: label(ind),
                unit: "%".into(),
                points,
            });
        }
        let points: Vec<(String, f64)> = rows
            .iter()
            .filter(|r| {
                r.indicator == "infant_mortality_rate"
                    && r.geo_type == "state"
                    && r.group == "Total"
            })
            .filter_map(|r| f(&r.value).map(|v| (r.period.clone(), v)))
            .collect();
        trends.push(Trend {
            indicator: label("infant_mortality_rate"),
            unit: "per 1,000 live births".into(),
            points,
        });
    }

    // ---- Markdown
    let fmt = |v: Option<f64>| v.map(|x| format!("{x:.1}")).unwrap_or_else(|| "—".into());
    let mut s = String::from("**Maternal and infant health profile — Massachusetts**\n\n");
    if let Some((_, name)) = &community {
        s.push_str(&format!("**{name}** (2023 resident births)\n\n"));
        let rws: Vec<Vec<String>> = community_indicators
            .iter()
            .map(|c| {
                vec![
                    c.indicator.clone(),
                    match (c.value, c.lower, c.upper) {
                        (Some(v), Some(l), Some(h)) => format!("{v:.1} ({l:.1}-{h:.1})"),
                        (Some(v), _, _) => format!("{v:.0}"),
                        _ => "suppressed".into(),
                    },
                    c.numerator.clone(),
                    fmt(c.state_value),
                    fmt(c.us_value),
                    format!(
                        "{}{}",
                        c.comparison,
                        if c.flags.is_empty() {
                            String::new()
                        } else {
                            format!("; {}", c.flags.join("; "))
                        }
                    ),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Indicator",
                "Value (95% CI)",
                "Count",
                "MA",
                "U.S.",
                "Comparison",
            ],
            &rws,
        ));
        s.push('\n');
    }
    let show_groups: Vec<&str> = if by_race {
        GROUPS.to_vec()
    } else {
        vec!["Total"]
    };
    let present: Vec<&str> = show_groups
        .iter()
        .copied()
        .filter(|g| state_indicators.iter().any(|i| i.values.contains_key(*g)))
        .collect();
    s.push_str("**Statewide by race and Hispanic ethnicity**\n\n");
    let mut hdr = vec!["Indicator", "Year"];
    hdr.extend(present.iter().copied());
    hdr.push("U.S.");
    let rws: Vec<Vec<String>> = state_indicators
        .iter()
        .map(|i| {
            let mut v = vec![i.indicator.clone(), i.period.clone()];
            v.extend(
                present
                    .iter()
                    .map(|g| fmt(i.values.get(*g).copied().flatten())),
            );
            v.push(fmt(i.us_total));
            v
        })
        .collect();
    s.push_str(&md::table(&hdr, &rws));
    if !prams.is_empty() {
        s.push_str("\n**PRAMS (statewide survey of new mothers, weighted %)**\n\n");
        let pgroups: Vec<&str> = if by_race {
            vec![
                "Total",
                "White non-Hispanic",
                "Black non-Hispanic",
                "Hispanic",
                "Asian non-Hispanic",
            ]
        } else {
            vec!["Total"]
        };
        let mut hdr = vec!["Indicator", "Years"];
        hdr.extend(pgroups.iter().copied());
        let rws: Vec<Vec<String>> = prams
            .iter()
            .map(|i| {
                let mut v = vec![i.indicator.clone(), i.period.clone()];
                v.extend(
                    pgroups
                        .iter()
                        .map(|g| fmt(i.values.get(*g).copied().flatten())),
                );
                v
            })
            .collect();
        s.push_str(&md::table(&hdr, &rws));
    }
    if !trends.is_empty() {
        s.push_str("\n**Trends (Massachusetts)**\n\n");
        for t in &trends {
            if let (Some(a), Some(b)) = (t.points.first(), t.points.last()) {
                s.push_str(&format!(
                    "- {}: {:.1} in {} to {:.1} in {}\n",
                    t.indicator, a.1, a.0, b.1, b.0
                ));
            }
        }
    }

    let mut caveats = vec![
        "Massachusetts figures come from DPH's published reports (births 2023, infant deaths 2012-2022, PRAMS 2019-2021); U.S. figures are CDC NCHS provisional estimates for different 12-month periods (births: 12 months ending 2024 Q4; infant mortality: 12 months ending 2023 Q4), so compare with care.".to_string(),
        "Town and county percentages are computed from DPH's counts with exact Poisson 95% intervals; percentages from fewer than 20 events are unstable, and DPH suppresses counts of 1-4. 'Higher' or 'lower' than the state means the interval excludes the state value.".into(),
        "Community-level infant mortality and PRAMS estimates are not published by DPH; they are statewide only. Ratios to White non-Hispanic are simple ratios of published rates (no intervals).".into(),
        "DPH's machine-readable town-level vital statistics were not available; the snapshot is rebuilt from the annual reports (xtask/scripts/build_maternal_infant.py).".into(),
    ];
    if any_suppressed {
        caveats.push(format!(
            "Counts below the suppression threshold ({threshold}) are not shown."
        ));
    }
    let mut srcs: Vec<String> = rows.iter().map(|r| r.source.clone()).collect();
    srcs.sort();
    srcs.dedup();
    let data = MaternalInfantResult {
        community: community.map(|(_, n)| n),
        community_indicators,
        state_indicators,
        prams,
        trends,
        suppression_threshold: threshold,
    };
    Ok(Envelope::new(s, data)
        .source(entry.to_source())
        .sources(srcs.iter().map(|s| source_for(s)))
        .source(Source::computed(
            "Exact Poisson 95% intervals for community percentages",
            "PHN MCP server",
        ))
        .caveats(caveats)
        .privacy(Privacy {
            suppression_applied: any_suppressed,
            suppression_threshold: Some(threshold),
            ..Privacy::none()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn town_profile_with_state_and_us() {
        let (_d, c) = ctx();
        let env = maternal_infant_health_profile(
            &c,
            MaternalInfantArgs {
                town: Some("New Bedford".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let d = &env.data;
        let lbw = d
            .community_indicators
            .iter()
            .find(|i| i.indicator.starts_with("Low birthweight"))
            .unwrap();
        assert!((lbw.value.unwrap() - 127.0 / 1156.0 * 100.0).abs() < 1e-9);
        assert_eq!(lbw.comparison, "higher than the state");
        assert!(lbw.state_value.is_some() && lbw.us_value.is_some());
        let im = d
            .state_indicators
            .iter()
            .find(|i| i.indicator.starts_with("Infant mortality"))
            .unwrap();
        assert_eq!(im.period, "2022");
        assert!(im.ratio_to_white["Black non-Hispanic"] > 1.0);
        assert!(!d.prams.is_empty());
        assert!(env.summary.contains("New Bedford"));
    }

    #[test]
    fn small_town_counts_are_suppressed_and_county_works() {
        let (_d, c) = ctx();
        let env = maternal_infant_health_profile(
            &c,
            MaternalInfantArgs {
                town: Some("Worthington".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            env.data
                .community_indicators
                .iter()
                .any(|i| i.value.is_none())
        );
        assert!(env.privacy.suppression_applied);
        let env = maternal_infant_health_profile(
            &c,
            MaternalInfantArgs {
                county: Some("hampden".into()),
                by_race: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.community.as_deref(), Some("Hampden County"));
        assert!(
            env.data.state_indicators[0]
                .values
                .keys()
                .all(|k| k == "Total")
        );
    }
}
