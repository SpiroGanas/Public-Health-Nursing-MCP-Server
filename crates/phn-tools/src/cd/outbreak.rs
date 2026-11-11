//! CD-04 `analyze_outbreak_line_list`: person, place, and time summaries,
//! food- or exposure-specific attack rates with risk and odds ratios, likely
//! vehicles, and the probable exposure window. Aggregates only.

use chrono::{Duration, NaiveDateTime};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_epi::suppression::{SuppressionOptions, suppress};
use phn_epi::two_by_two::{self, Design, Estimate, Table};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::line_list::{self, ColumnMap, ColumnOptions, LineList, Rec};
use crate::context::Context;
use crate::md;

/// Arguments for `analyze_outbreak_line_list`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct OutbreakArgs {
    /// Line list (CSV or XLSX) in the data folder, one row per person (ill and,
    /// for attack rates, well people who were exposed to the event).
    pub path: String,
    /// Worksheet name for XLSX files.
    #[serde(default)]
    pub sheet: Option<String>,
    /// Column choices and case definition (found automatically when not given).
    #[serde(default, flatten)]
    pub columns: ColumnOptions,
    /// Suspected disease, for incubation periods (e.g. "salmonellosis", "norovirus").
    #[serde(default)]
    pub disease: Option<String>,
    /// Minimum incubation in hours (overrides the disease table).
    #[serde(default)]
    pub incubation_min_hours: Option<f64>,
    /// Typical (median) incubation in hours.
    #[serde(default)]
    pub incubation_typical_hours: Option<f64>,
    /// Maximum incubation in hours.
    #[serde(default)]
    pub incubation_max_hours: Option<f64>,
    /// A known common exposure (meal or event), "YYYY-MM-DD HH:MM" or a date.
    #[serde(default)]
    pub exposure_datetime: Option<String>,
    /// cohort (default: everyone who attended is listed) or case_control
    /// (non-cases are controls; odds ratios only).
    #[serde(default)]
    pub design: Option<Design>,
}

/// One row of a suppressed frequency table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CountRow {
    pub label: String,
    /// Cases (absent when suppressed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cases: Option<u64>,
    /// Non-cases (absent when suppressed or a case series).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_cases: Option<u64>,
    /// Attack rate in percent (absent when not estimable or suppressed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_rate_pct: Option<f64>,
    /// True when a cell in this row is hidden by small-cell suppression.
    pub suppressed: bool,
}

/// One exposure's attack-rate analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExposureRow {
    pub exposure: String,
    /// Ill among the exposed (absent when suppressed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposed_ill: Option<u64>,
    pub exposed_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposed_attack_rate_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unexposed_ill: Option<u64>,
    pub unexposed_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unexposed_attack_rate_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_ratio: Option<Estimate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub odds_ratio: Option<Estimate>,
    /// Fisher's exact test, two-sided.
    pub p_value: f64,
    /// Percent of cases (with a known answer) who were exposed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent_of_cases_exposed: Option<f64>,
    pub suppressed: bool,
}

/// The probable exposure window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExposureWindow {
    /// Earliest likely exposure: last onset − maximum incubation.
    pub earliest: String,
    /// Latest likely exposure: first onset − minimum incubation.
    pub latest: String,
    /// Most likely exposure: median onset − typical incubation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub most_likely: Option<String>,
    /// False when the onsets span more than the incubation range (earliest > latest).
    pub consistent_with_single_exposure: bool,
    pub incubation_used: String,
    /// For a known exposure: median incubation in hours among cases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_incubation_hours: Option<f64>,
    /// For a known exposure: percent of cases whose incubation falls within the range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent_within_incubation_range: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_exposure_in_window: Option<bool>,
    pub method: String,
}

/// Result of `analyze_outbreak_line_list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutbreakResult {
    pub columns: ColumnMap,
    pub design: Design,
    pub records: usize,
    pub cases: usize,
    pub non_cases: usize,
    /// Records excluded by the case definition (e.g. suspect or out-of-window).
    pub excluded: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overall_attack_rate_pct: Option<f64>,
    pub age_groups: Vec<CountRow>,
    pub sex: Vec<CountRow>,
    pub towns: Vec<CountRow>,
    pub onset_by_day: Vec<CountRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_age_of_cases: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_onset: Option<String>,
    /// Symptoms among cases.
    pub symptoms: Vec<CountRow>,
    /// Exposure-specific attack rates, strongest association first.
    pub exposures: Vec<ExposureRow>,
    pub likely_vehicles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure_window: Option<ExposureWindow>,
    pub interpretation: String,
}

pub(crate) struct Incubation {
    pub min_h: f64,
    pub typical_h: Option<f64>,
    pub max_h: f64,
    pub label: String,
    pub source: Option<Source>,
}

/// Incubation from explicit hours or the disease table.
pub(crate) fn incubation(
    disease: Option<&str>,
    min_h: Option<f64>,
    typ_h: Option<f64>,
    max_h: Option<f64>,
) -> Result<Option<Incubation>> {
    let content = phn_content::disease_rules::content();
    let mut out: Option<Incubation> = None;
    // A disease the table does not hold ("GI illness, agent unknown") is not
    // an error: the analysis runs without a disease-specific incubation period.
    if let Some(name) = disease.filter(|s| !s.trim().is_empty())
        && let Ok(d) = super::engine::find_disease(name)
        && let Some(i) = &d.incubation
        && let (Some(a), Some(b)) = (i.min_days(), i.max_days())
    {
        out = Some(Incubation {
            min_h: a * 24.0,
            typical_h: i.typical_days().map(|t| t * 24.0),
            max_h: b * 24.0,
            label: format!("{}: {}", d.name, i.describe()),
            source: Some(super::engine::rule_source(content, &i.source.source)),
        });
    }
    if min_h.is_some() || max_h.is_some() || typ_h.is_some() {
        let (Some(a), Some(b)) = (
            min_h.or(out.as_ref().map(|o| o.min_h)),
            max_h.or(out.as_ref().map(|o| o.max_h)),
        ) else {
            return Err(PhnError::InvalidInput(
                "give both incubation_min_hours and incubation_max_hours (or a disease)".into(),
            ));
        };
        if !(a >= 0.0 && b > a && b <= 24.0 * 365.0) {
            return Err(PhnError::InvalidInput(
                "incubation hours must satisfy 0 ≤ min < max".into(),
            ));
        }
        out = Some(Incubation {
            min_h: a,
            typical_h: typ_h.or(out.as_ref().and_then(|o| o.typical_h)),
            max_h: b,
            label: format!(
                "{}–{} hours{} (given)",
                md::num(a, 0),
                md::num(b, 0),
                typ_h
                    .map(|t| format!(", typical {} hours", md::num(t, 0)))
                    .unwrap_or_default()
            ),
            source: None,
        });
    }
    Ok(out)
}

/// Apply suppression to rows of [cases, non_cases].
pub(crate) fn suppressed_rows(
    labels: &[String],
    cases: &[u64],
    non_cases: Option<&[u64]>,
    threshold: u32,
) -> (Vec<CountRow>, bool) {
    let cells: Vec<Vec<Option<u64>>> = (0..labels.len())
        .map(|i| match non_cases {
            Some(nc) => vec![Some(cases[i]), Some(nc[i])],
            None => vec![Some(cases[i])],
        })
        .collect();
    let res = suppress(
        &cells,
        &SuppressionOptions {
            threshold,
            ..SuppressionOptions::default()
        },
    );
    let mut any = false;
    let rows = (0..labels.len())
        .map(|i| {
            let hc = res.is_hidden(i, 0);
            let hn = non_cases.is_some() && res.is_hidden(i, 1);
            any |= hc || hn;
            let c = (!hc).then_some(cases[i]);
            let n = non_cases.and_then(|nc| (!hn).then_some(nc[i]));
            let ar = match (c, non_cases) {
                (Some(c), Some(nc)) if !hn && c + nc[i] > 0 => {
                    Some(100.0 * c as f64 / (c + nc[i]) as f64)
                }
                _ => None,
            };
            CountRow {
                label: labels[i].clone(),
                cases: c,
                non_cases: n,
                attack_rate_pct: ar,
                suppressed: hc || hn,
            }
        })
        .collect();
    (rows, any)
}

fn hidden(x: u64, t: u32) -> bool {
    x > 0 && x < u64::from(t)
}

fn show(x: Option<u64>, t: u32) -> String {
    x.map(|v| v.to_string()).unwrap_or_else(|| format!("<{t}"))
}

pub(crate) fn fmt_dt(d: NaiveDateTime, times: bool) -> String {
    if times {
        d.format("%Y-%m-%d %H:%M").to_string()
    } else {
        d.format("%Y-%m-%d").to_string()
    }
}

pub(crate) fn median_dt(v: &mut [NaiveDateTime]) -> Option<NaiveDateTime> {
    if v.is_empty() {
        return None;
    }
    v.sort();
    let m = v.len() / 2;
    if v.len() % 2 == 1 {
        Some(v[m])
    } else {
        Some(v[m - 1] + (v[m] - v[m - 1]) / 2)
    }
}

fn hours(h: f64) -> Duration {
    Duration::minutes((h * 60.0).round() as i64)
}

pub(crate) fn exposure_window(
    onsets: &[NaiveDateTime],
    inc: &Incubation,
    known: Option<NaiveDateTime>,
    times: bool,
) -> Option<ExposureWindow> {
    if onsets.is_empty() {
        return None;
    }
    let mut v = onsets.to_vec();
    let med = median_dt(&mut v)?;
    let first = *v.first()?;
    let last = *v.last()?;
    // Without times, onset dates are treated as midday to avoid a half-day bias.
    let adj = |d: NaiveDateTime| if times { d } else { d + Duration::hours(12) };
    let earliest = adj(last) - hours(inc.max_h);
    let latest = adj(first) - hours(inc.min_h);
    let most = inc.typical_h.map(|t| adj(med) - hours(t));
    let (mut med_inc, mut within, mut in_window) = (None, None, None);
    if let Some(k) = known {
        let mut incs: Vec<f64> = v
            .iter()
            .map(|o| (adj(*o) - k).num_minutes() as f64 / 60.0)
            .collect();
        incs.sort_by(f64::total_cmp);
        let m = incs.len() / 2;
        med_inc = Some(if incs.len() % 2 == 1 {
            incs[m]
        } else {
            (incs[m - 1] + incs[m]) / 2.0
        });
        let ok = incs
            .iter()
            .filter(|h| **h >= inc.min_h && **h <= inc.max_h)
            .count();
        within = Some(100.0 * ok as f64 / incs.len() as f64);
        in_window = Some(k >= earliest && k <= latest);
    }
    Some(ExposureWindow {
        earliest: fmt_dt(earliest, true),
        latest: fmt_dt(latest, true),
        most_likely: most.map(|m| fmt_dt(m, true)),
        consistent_with_single_exposure: earliest <= latest,
        incubation_used: inc.label.clone(),
        median_incubation_hours: med_inc,
        percent_within_incubation_range: within,
        known_exposure_in_window: in_window,
        method: format!(
            "Earliest = last onset − maximum incubation; latest = first onset − minimum \
             incubation; most likely = median onset − typical incubation{}.",
            if times {
                ""
            } else {
                " (onset dates without times are placed at midday)"
            }
        ),
    })
}

fn age_group(a: f64) -> usize {
    match a {
        x if x < 5.0 => 0,
        x if x < 18.0 => 1,
        x if x < 50.0 => 2,
        x if x < 65.0 => 3,
        _ => 4,
    }
}

const AGE_LABELS: [&str; 6] = ["0–4", "5–17", "18–49", "50–64", "65+", "Unknown"];

fn table_by<F: Fn(&Rec) -> Option<String>>(
    recs: &[&Rec],
    key: F,
    max_rows: usize,
) -> (Vec<String>, Vec<u64>, Vec<u64>) {
    let mut m: std::collections::BTreeMap<String, (u64, u64)> = Default::default();
    for r in recs {
        let k = key(r).unwrap_or_else(|| "Unknown".into());
        let e = m.entry(k).or_default();
        if r.case == Some(true) {
            e.0 += 1;
        } else {
            e.1 += 1;
        }
    }
    let mut v: Vec<(String, (u64, u64))> = m.into_iter().collect();
    v.sort_by(|a, b| (b.1.0 + b.1.1).cmp(&(a.1.0 + a.1.1)).then(a.0.cmp(&b.0)));
    if v.len() > max_rows {
        let rest: (u64, u64) = v[max_rows - 1..]
            .iter()
            .fold((0, 0), |acc, x| (acc.0 + x.1.0, acc.1 + x.1.1));
        v.truncate(max_rows - 1);
        v.push(("All others".into(), rest));
    }
    (
        v.iter().map(|x| x.0.clone()).collect(),
        v.iter().map(|x| x.1.0).collect(),
        v.iter().map(|x| x.1.1).collect(),
    )
}

/// Suppressed cells in frequency tables (primary and complementary look the same).
fn star(x: Option<u64>) -> String {
    x.map(|v| v.to_string()).unwrap_or_else(|| "*".into())
}

pub(crate) fn count_table(rows: &[CountRow], cohort: bool) -> String {
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut v = vec![r.label.clone(), star(r.cases)];
            if cohort {
                v.push(star(r.non_cases));
                v.push(
                    r.attack_rate_pct
                        .map(|a| format!("{a:.0}%"))
                        .unwrap_or_else(|| "—".into()),
                );
            }
            v
        })
        .collect();
    if cohort {
        md::table(&["", "Cases", "Non-cases", "Attack rate"], &body)
    } else {
        md::table(&["", "Cases"], &body)
    }
}

/// Analyze an outbreak line list.
pub fn analyze_outbreak_line_list(
    ctx: &Context,
    args: OutbreakArgs,
) -> Result<Envelope<OutbreakResult>> {
    let t = ctx.suppression_threshold();
    let inc = incubation(
        args.disease.as_deref(),
        args.incubation_min_hours,
        args.incubation_typical_hours,
        args.incubation_max_hours,
    )?;
    let known = match args.exposure_datetime.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s) => Some(line_list::parse_datetime(s).map(|x| x.0).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "exposure_datetime '{s}' is not a date or date-time (YYYY-MM-DD HH:MM)"
            ))
        })?),
    };
    let ll: LineList = line_list::load(ctx, &args.path, args.sheet.as_deref(), &args.columns)?;
    let recs: Vec<&Rec> = ll.recs.iter().filter(|r| r.case.is_some()).collect();
    let cases: Vec<&Rec> = recs
        .iter()
        .copied()
        .filter(|r| r.case == Some(true))
        .collect();
    let n_cases = cases.len();
    let n_non = recs.len() - n_cases;
    let excluded = ll.recs.len() - recs.len();
    if n_cases == 0 {
        return Err(PhnError::InvalidInput(format!(
            "no records meet the case definition (case column {}, values {}). Check case_column \
             and case_values.",
            ll.map.case.clone().unwrap_or_else(|| "(none)".into()),
            ll.map.case_values.join(", ")
        )));
    }
    let cohort_data = n_non > 0;
    let design = args.design.unwrap_or(Design::Cohort);
    let show_ar = cohort_data && design == Design::Cohort;
    let mut any_suppressed = false;

    // Person.
    let (labels, c, nc) = {
        let mut cc = [0u64; 6];
        let mut nn = [0u64; 6];
        for r in &recs {
            let g = r.age.map(age_group).unwrap_or(5);
            if r.case == Some(true) {
                cc[g] += 1;
            } else {
                nn[g] += 1;
            }
        }
        let keep: Vec<usize> = (0..6).filter(|&i| i < 5 || cc[i] + nn[i] > 0).collect();
        (
            keep.iter()
                .map(|&i| AGE_LABELS[i].to_string())
                .collect::<Vec<_>>(),
            keep.iter().map(|&i| cc[i]).collect::<Vec<_>>(),
            keep.iter().map(|&i| nn[i]).collect::<Vec<_>>(),
        )
    };
    let (age_groups, s1) = suppressed_rows(&labels, &c, cohort_data.then_some(&nc[..]), t);
    let (l, c, nc) = table_by(&recs, |r| r.sex.clone(), 6);
    let (sex, s2) = suppressed_rows(&l, &c, cohort_data.then_some(&nc[..]), t);
    let (l, c, nc) = table_by(&recs, |r| r.town.clone(), 15);
    let (towns, s3) = if ll.map.town.is_some() {
        suppressed_rows(&l, &c, cohort_data.then_some(&nc[..]), t)
    } else {
        (Vec::new(), false)
    };
    let mut by_day: std::collections::BTreeMap<String, u64> = Default::default();
    for r in &cases {
        let k = r
            .onset
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Unknown".into());
        *by_day.entry(k).or_default() += 1;
    }
    let (dl, dc): (Vec<String>, Vec<u64>) = by_day.into_iter().unzip();
    let (onset_by_day, s4) = suppressed_rows(&dl, &dc, None, t);
    any_suppressed |= s1 || s2 || s3 || s4;
    let mut ages: Vec<f64> = cases.iter().filter_map(|r| r.age).collect();
    ages.sort_by(f64::total_cmp);
    let median_age = (ages.len() >= t as usize && !ages.is_empty()).then(|| {
        let m = ages.len() / 2;
        if ages.len() % 2 == 1 {
            ages[m]
        } else {
            (ages[m - 1] + ages[m]) / 2.0
        }
    });
    let mut onsets: Vec<NaiveDateTime> = cases.iter().filter_map(|r| r.onset).collect();
    // With only a few onsets, the median and the exposure window (first and
    // last onset minus the incubation limits) give away individual onset
    // times, so both are held back below the suppression threshold, as the
    // median age is.
    let n_onsets = onsets.len();
    let enough_onsets = n_onsets >= t as usize;
    let median_onset = if enough_onsets {
        median_dt(&mut onsets.clone()).map(|d| fmt_dt(d, ll.has_times))
    } else {
        None
    };

    // Symptoms among cases.
    let sym_labels = ll.map.symptoms.clone();
    let sym_counts: Vec<u64> = (0..sym_labels.len())
        .map(|j| cases.iter().filter(|r| r.symptoms[j] == Some(true)).count() as u64)
        .collect();
    let (mut symptoms, s5) = suppressed_rows(&sym_labels, &sym_counts, None, t);
    any_suppressed |= s5;
    for (j, row) in symptoms.iter_mut().enumerate() {
        let known_n = cases.iter().filter(|r| r.symptoms[j].is_some()).count();
        if let Some(cn) = row.cases
            && known_n > 0
        {
            row.attack_rate_pct = Some(100.0 * cn as f64 / known_n as f64);
        }
    }

    // Exposures.
    let mut exposures = Vec::new();
    let mut skipped_exposures: Vec<String> = Vec::new();
    if cohort_data {
        for (j, name) in ll.map.exposures.iter().enumerate() {
            let (mut a, mut b, mut cc, mut d) = (0u64, 0u64, 0u64, 0u64);
            for r in &recs {
                match (r.exposures[j], r.case == Some(true)) {
                    (Some(true), true) => a += 1,
                    (Some(true), false) => b += 1,
                    (Some(false), true) => cc += 1,
                    (Some(false), false) => d += 1,
                    _ => {}
                }
            }
            if a + b + cc + d == 0 || a + b == 0 || cc + d == 0 {
                skipped_exposures.push(name.clone());
                continue;
            }
            let res = two_by_two::analyze(Table::new(a, b, cc, d), design, 0.95);
            let p = res
                .exact
                .iter()
                .find(|x| x.name.starts_with("Fisher exact, two-sided"))
                .map(|x| x.p_value)
                .unwrap_or(1.0);
            let ha = hidden(a, t);
            let hc = hidden(cc, t);
            any_suppressed |= ha || hc;
            exposures.push(ExposureRow {
                exposure: name.clone(),
                exposed_ill: (!ha).then_some(a),
                exposed_total: a + b,
                exposed_attack_rate_pct: (!ha && show_ar)
                    .then(|| 100.0 * a as f64 / (a + b) as f64),
                unexposed_ill: (!hc).then_some(cc),
                unexposed_total: cc + d,
                unexposed_attack_rate_pct: (!hc && show_ar)
                    .then(|| 100.0 * cc as f64 / (cc + d) as f64),
                risk_ratio: if design == Design::Cohort {
                    res.risk_ratio.clone()
                } else {
                    None
                },
                odds_ratio: res.odds_ratio.clone().or(res.odds_ratio_exact.clone()),
                p_value: p,
                percent_of_cases_exposed: (a + cc > 0).then(|| 100.0 * a as f64 / (a + cc) as f64),
                suppressed: ha || hc,
            });
        }
    }
    let strength = |e: &ExposureRow| {
        e.risk_ratio
            .as_ref()
            .or(e.odds_ratio.as_ref())
            .map(|x| x.value)
            .unwrap_or(0.0)
    };
    exposures.sort_by(|x, y| {
        x.p_value
            .partial_cmp(&y.p_value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(strength(y).total_cmp(&strength(x)))
    });
    let likely: Vec<String> = exposures
        .iter()
        .filter(|e| {
            let m = e.risk_ratio.as_ref().or(e.odds_ratio.as_ref());
            e.p_value < 0.05 && m.is_some_and(|m| m.value > 1.0 && m.lower > 1.0)
        })
        .map(|e| e.exposure.clone())
        .collect();

    // Exposure window.
    let window = if enough_onsets {
        inc.as_ref()
            .and_then(|i| exposure_window(&onsets, i, known, ll.has_times))
    } else {
        None
    };
    onsets.clear();
    let unknown_disease = args
        .disease
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty() && super::engine::find_disease(d).is_err());

    // ---- Markdown
    let mut s = format!(
        "**Outbreak line list: {}** — {} records; {} cases, {} non-cases{}\n\n",
        ll.table.label,
        ll.recs.len(),
        n_cases,
        n_non,
        if excluded > 0 {
            format!(", {excluded} excluded by the case definition")
        } else {
            String::new()
        }
    );
    let overall_ar = show_ar.then(|| 100.0 * n_cases as f64 / recs.len() as f64);
    if let Some(ar) = overall_ar {
        s.push_str(&format!(
            "Overall attack rate: {ar:.0}% ({n_cases}/{}).\n\n",
            recs.len()
        ));
    }
    s.push_str(&format!(
        "Columns used: onset {}{}, case {} (case = {}), age {}, sex {}, town {}.\n\n",
        ll.map.onset_date.clone().unwrap_or_else(|| "none".into()),
        ll.map
            .onset_time
            .as_ref()
            .map(|t| format!(" + {t}"))
            .unwrap_or_default(),
        ll.map.case.clone().unwrap_or_else(|| "none".into()),
        ll.map.case_values.join("/"),
        ll.map.age.clone().unwrap_or_else(|| "none".into()),
        ll.map.sex.clone().unwrap_or_else(|| "none".into()),
        ll.map.town.clone().unwrap_or_else(|| "none".into()),
    ));
    if !exposures.is_empty() {
        s.push_str(&format!(
            "**Exposure-specific attack rates** ({})\n\n",
            if design == Design::Cohort {
                "risk ratio with 95% CI; Fisher exact p"
            } else {
                "case-control: odds ratio with 95% CI; Fisher exact p"
            }
        ));
        let rows: Vec<Vec<String>> = exposures
            .iter()
            .map(|e| {
                let ar = |x: Option<f64>| x.map(|v| format!(" ({v:.0}%)")).unwrap_or_default();
                let est = |m: &Option<Estimate>| {
                    m.as_ref()
                        .map(|m| {
                            format!(
                                "{} ({}–{})",
                                md::num(m.value, 2),
                                md::num(m.lower, 2),
                                m.upper.map_or("∞".into(), |u| md::num(u, 2))
                            )
                        })
                        .unwrap_or_else(|| "—".into())
                };
                vec![
                    e.exposure.clone(),
                    format!(
                        "{}/{}{}",
                        show(e.exposed_ill, t),
                        e.exposed_total,
                        ar(e.exposed_attack_rate_pct)
                    ),
                    format!(
                        "{}/{}{}",
                        show(e.unexposed_ill, t),
                        e.unexposed_total,
                        ar(e.unexposed_attack_rate_pct)
                    ),
                    if design == Design::Cohort {
                        est(&e.risk_ratio)
                    } else {
                        est(&e.odds_ratio)
                    },
                    fmt_p(e.p_value),
                    e.percent_of_cases_exposed
                        .map(|p| format!("{p:.0}%"))
                        .unwrap_or_default(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Exposure",
                "Exposed: ill/total",
                "Not exposed: ill/total",
                if design == Design::Cohort {
                    "Risk ratio (95% CI)"
                } else {
                    "Odds ratio (95% CI)"
                },
                "p",
                "% of cases exposed",
            ],
            &rows,
        ));
        s.push('\n');
    } else if !cohort_data {
        s.push_str(
            "Exposure attack rates need well people too: this list has cases only (a case \
             series). Add everyone who attended (ill and well) to compare attack rates.\n\n",
        );
    }
    if !likely.is_empty() {
        s.push_str(&format!(
            "**Likely vehicle(s):** {} — a significant association (p < 0.05, lower confidence \
             limit above 1){}.\n\n",
            likely.join(", "),
            exposures
                .iter()
                .find(|e| e.exposure == likely[0])
                .and_then(|e| e.percent_of_cases_exposed)
                .map(|p| format!("; {p:.0}% of cases reported {}", likely[0]))
                .unwrap_or_default()
        ));
    }
    if let Some(w) = &window {
        s.push_str(&format!(
            "**Probable exposure window** ({}): {} to {}{}.{}{}\n\n",
            w.incubation_used,
            w.earliest,
            w.latest,
            w.most_likely
                .as_ref()
                .map(|m| format!("; most likely around {m}"))
                .unwrap_or_default(),
            if w.consistent_with_single_exposure {
                String::new()
            } else {
                " ⚠ The onsets span more than the incubation range, which suggests ongoing or \
                 several exposures, person-to-person spread, or a different agent."
                    .to_string()
            },
            match (w.known_exposure_in_window, w.median_incubation_hours) {
                (Some(inw), Some(mh)) => format!(
                    " The known exposure is {} the window; median incubation from it is {} hours \
                     ({:.0}% of cases within the incubation range).",
                    if inw { "inside" } else { "outside" },
                    md::num(mh, 0),
                    w.percent_within_incubation_range.unwrap_or(0.0)
                ),
                _ => String::new(),
            }
        ));
    } else if n_onsets > 0 && !enough_onsets {
        s.push_str(&format!(
            "Fewer than {t} cases have an onset date, so the median onset and the probable \
             exposure window are not shown (they would give away individual onset times).\n\n"
        ));
    } else if onsets_empty(&ll) {
        s.push_str("No onset dates were found, so no exposure window was estimated.\n\n");
    } else {
        s.push_str(
            "Give a disease (or incubation hours) to estimate the probable exposure window.\n\n",
        );
    }
    s.push_str(&format!(
        "**Person — age group**\n\n{}",
        count_table(&age_groups, show_ar)
    ));
    if let Some(m) = median_age {
        s.push_str(&format!("\nMedian age of cases: {}.\n", md::num(m, 0)));
    }
    if ll.map.sex.is_some() {
        s.push_str(&format!(
            "\n**Person — sex**\n\n{}",
            count_table(&sex, show_ar)
        ));
    }
    if !towns.is_empty() {
        s.push_str(&format!(
            "\n**Place — town**\n\n{}",
            count_table(&towns, show_ar)
        ));
    }
    s.push_str(&format!(
        "\n**Time — cases by onset date**{}\n\n{}",
        median_onset
            .as_ref()
            .map(|m| format!(" (median onset {m})"))
            .unwrap_or_default(),
        count_table(&onset_by_day, false)
    ));
    if !symptoms.is_empty() {
        let body: Vec<Vec<String>> = symptoms
            .iter()
            .map(|r| {
                vec![
                    r.label.clone(),
                    star(r.cases),
                    r.attack_rate_pct
                        .map(|p| format!("{p:.0}%"))
                        .unwrap_or_else(|| "—".into()),
                ]
            })
            .collect();
        s.push_str(&format!(
            "\n**Symptoms among cases**\n\n{}",
            md::table(&["Symptom", "Cases", "% of cases"], &body)
        ));
    }
    let interpretation = interpret(&likely, &exposures, window.as_ref(), cohort_data, n_cases);
    s.push_str(&format!("\n**Interpretation**\n\n{interpretation}\n"));
    s.push_str(
        "\nNext: draw the epidemic curve (generate_epi_curve), check exclusions for ill food \
         handlers (calculate_isolation_and_exclusion), and report the cluster to DPH through \
         MAVEN.\n",
    );

    // ---- Caveats, privacy
    let mut caveats = Vec::new();
    if any_suppressed {
        caveats.push(format!(
            "Small-cell suppression: in the person, place, time, and symptom tables, * marks \
             counts from 1 to {} and the extra cells hidden so they cannot be recalculated from \
             totals. In the exposure table, counts from 1 to {} are shown as <{t}, but risk \
             ratios, odds ratios, and p-values are computed from all records and, with the \
             totals, can reveal those small counts; the table holds no identifiers.",
            t - 1,
            t - 1
        ));
    }
    if !skipped_exposures.is_empty() {
        caveats.push(format!(
            "Not analyzed because no one was recorded as exposed, or no one as unexposed: {}. If \
             a blank cell means \"did not eat\", enter N in those cells and run the analysis \
             again; an item nearly everyone ate can still be the vehicle.",
            skipped_exposures.join(", ")
        ));
    }
    if let Some(d) = unknown_disease {
        caveats.push(format!(
            "No incubation period was used: '{d}' is not one specific disease in this server's \
             disease table (or the name fits more than one). Give the specific disease, or \
             incubation_min_hours and incubation_max_hours, to estimate the exposure window."
        ));
    }
    if ll.cases_without_onset > 0 {
        caveats.push(format!(
            "{} case(s) have no readable onset date and are left out of the time summary and \
             exposure window.",
            ll.cases_without_onset
        ));
    }
    if n_cases < 20 {
        caveats.push(format!(
            "Only {n_cases} cases: confidence intervals are wide and a real vehicle can fail to \
             reach significance."
        ));
    }
    if exposures.len() > 5 {
        caveats.push(format!(
            "{} exposures were tested; about 1 in 20 can reach p < 0.05 by chance. Look for a \
             strong, plausible association that accounts for most cases.",
            exposures.len()
        ));
    }
    if !ll.map.notes.is_empty() {
        caveats.extend(ll.map.notes.iter().cloned());
    }
    caveats.push(
        "Recall bias and misclassification are common in food histories; confirm with \
         laboratory testing of cases and implicated food (MA State Public Health Laboratory)."
            .into(),
    );
    if inc.is_some() && args.disease.is_some() {
        caveats.push(super::engine::pending_caveat(
            phn_content::disease_rules::content(),
        ));
    }
    let mut sources = vec![
        ll.table.source(),
        Source::computed(
            "Risk ratio (Katz interval), odds ratio (Woolf interval), Fisher exact test",
            "phn_epi::two_by_two (golden-tested against R)",
        ),
    ];
    if let Some(src) = inc.as_ref().and_then(|i| i.source.clone()) {
        sources.push(src);
    }
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: any_suppressed,
        suppression_threshold: Some(t),
        identifiers_detected: ll.table.identifier_categories(),
        files_written: Vec::new(),
        notes: vec![format!(
            "Aggregates only: no names, IDs, or contact details are returned. Onset dates appear \
             only as the row labels of the cases-by-onset-date table (counts under {t} are \
             hidden); the median onset and the exposure window are returned only when at least \
             {t} cases have an onset date."
        )],
    };
    let data = OutbreakResult {
        columns: ll.map.clone(),
        design,
        records: ll.recs.len(),
        cases: n_cases,
        non_cases: n_non,
        excluded,
        overall_attack_rate_pct: overall_ar,
        age_groups,
        sex,
        towns,
        onset_by_day,
        median_age_of_cases: median_age,
        median_onset,
        symptoms,
        exposures,
        likely_vehicles: likely,
        exposure_window: window,
        interpretation,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn onsets_empty(ll: &LineList) -> bool {
    ll.recs.iter().all(|r| r.onset.is_none())
}

fn fmt_p(p: f64) -> String {
    if p < 0.001 {
        "<0.001".into()
    } else {
        format!("{p:.3}")
    }
}

fn interpret(
    likely: &[String],
    exposures: &[ExposureRow],
    w: Option<&ExposureWindow>,
    cohort: bool,
    n_cases: usize,
) -> String {
    let mut out = Vec::new();
    if !cohort {
        out.push(format!(
            "This is a case series of {n_cases} cases: it describes who, where, and when, but \
             cannot compare exposures. A cohort (everyone at the event) or a case-control study \
             is needed to identify a vehicle."
        ));
    } else if likely.is_empty() {
        out.push(
            "No exposure is significantly associated with illness. Consider a missing food \
             item, an exposure not on the questionnaire, misclassification, or person-to-person \
             spread."
                .into(),
        );
    } else {
        let top = exposures.iter().find(|e| e.exposure == likely[0]);
        let explains = top
            .and_then(|e| e.percent_of_cases_exposed)
            .is_some_and(|p| p >= 70.0);
        out.push(format!(
            "{} has the strongest significant association with illness{}. Cases who did not \
             report it may reflect recall error, cross-contamination, or secondary spread.",
            likely[0],
            if explains {
                " and accounts for most cases, as expected of a vehicle"
            } else {
                ", but it does not account for most cases; look for a second vehicle or an \
                 ingredient shared by several foods"
            }
        ));
        if likely.len() > 1 {
            out.push(format!(
                "Other associated items ({}) may be confounded with it (eaten together); \
                 stratify by the top item with analyze_two_by_two to check.",
                likely[1..].join(", ")
            ));
        }
    }
    if let Some(w) = w {
        if w.consistent_with_single_exposure {
            out.push(
                "The onset distribution fits a single common exposure within the window shown."
                    .into(),
            );
        } else {
            out.push(
                "The onsets are spread wider than one incubation period: consider continuing \
                 exposure or secondary transmission."
                    .into(),
            );
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn window_arithmetic() {
        let d = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        let onsets = vec![
            d("2026-05-03 06:00"),
            d("2026-05-03 18:00"),
            d("2026-05-04 12:00"),
        ];
        let inc = Incubation {
            min_h: 6.0,
            typical_h: Some(24.0),
            max_h: 72.0,
            label: "test".into(),
            source: None,
        };
        let w = exposure_window(&onsets, &inc, Some(d("2026-05-02 18:00")), true).unwrap();
        assert_eq!(w.earliest, "2026-05-01 12:00");
        assert_eq!(w.latest, "2026-05-03 00:00");
        assert_eq!(w.most_likely.as_deref(), Some("2026-05-02 18:00"));
        assert!(w.consistent_with_single_exposure);
        assert_eq!(w.median_incubation_hours, Some(24.0));
        assert_eq!(w.known_exposure_in_window, Some(true));
        let _ = NaiveDate::default();
    }

    #[test]
    fn church_supper_fixture_points_to_the_stuffing() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/data/church_supper_outbreak.csv");
        std::fs::copy(fixture, data.join("supper.csv")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = analyze_outbreak_line_list(
            &c,
            OutbreakArgs {
                path: "supper.csv".into(),
                disease: Some("salmonellosis".into()),
                exposure_datetime: Some("2026-05-02 18:00".into()),
                ..OutbreakArgs::default()
            },
        )
        .unwrap();
        let d = &env.data;
        println!("{}", env.to_markdown());
        assert_eq!(d.cases, 34);
        assert_eq!(d.non_cases, 26);
        assert_eq!(
            d.likely_vehicles.first().map(String::as_str),
            Some("Stuffing")
        );
        assert!(d.columns.symptoms.contains(&"Diarrhea".to_string()));
        assert!(!d.columns.exposures.contains(&"Hospitalized".to_string()));
        assert!(!d.columns.exposures.contains(&"Ill".to_string()));
        let w = d.exposure_window.as_ref().unwrap();
        assert_eq!(w.known_exposure_in_window, Some(true));
        assert!(env.privacy.identifiers_detected.len() >= 2);
        let md = env.to_markdown();
        for name in ["Abernathy", "Ardith", "508-555-0140", "CS-2026-001"] {
            assert!(!md.contains(name), "{name}");
        }
    }

    #[test]
    fn few_onsets_hide_the_median_and_window_and_unknown_disease_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let mut csv = String::from("Ill,Onset Date,Onset Time,Stuffing\n");
        for h in ["14:10", "16:20", "19:40"] {
            csv.push_str(&format!("Y,2026-05-03,{h},Y\n"));
        }
        for _ in 0..5 {
            csv.push_str("N,,,N\n");
        }
        std::fs::write(data.join("small.csv"), csv).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let env = analyze_outbreak_line_list(
            &c,
            OutbreakArgs {
                path: "small.csv".into(),
                disease: Some("salmonellosis".into()),
                ..OutbreakArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.cases, 3);
        assert!(env.data.median_onset.is_none());
        assert!(env.data.exposure_window.is_none());
        let md = env.to_markdown();
        assert!(md.contains("are not shown"), "{md}");
        for h in ["14:10", "16:20", "19:40"] {
            assert!(!md.contains(h), "{h}");
        }
        // The prompt's own example of an unknown agent must not fail the analysis.
        let env = analyze_outbreak_line_list(
            &c,
            OutbreakArgs {
                path: "small.csv".into(),
                disease: Some("GI illness, agent unknown".into()),
                ..OutbreakArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.exposure_window.is_none());
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("No incubation period was used"))
        );
        assert!(incubation(Some("gastroenteritis"), None, None, None).unwrap().is_none());
        // Everyone ate the stuffing among people with an answer: it cannot be
        // compared, and the caveat says so instead of dropping it silently.
        let mut csv = String::from("Ill,Onset Date,Stuffing,Pie\n");
        for i in 0..6 {
            csv.push_str(&format!("Y,2026-05-03,Y,{}\n", if i < 3 { "Y" } else { "N" }));
        }
        for i in 0..6 {
            csv.push_str(&format!("N,,Y,{}\n", if i < 2 { "Y" } else { "N" }));
        }
        std::fs::write(data.join("all_ate.csv"), csv).unwrap();
        let env = analyze_outbreak_line_list(
            &c,
            OutbreakArgs {
                path: "all_ate.csv".into(),
                ..OutbreakArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.exposures.iter().all(|e| e.exposure != "Stuffing"));
        assert!(
            env.caveats
                .iter()
                .any(|c| c.contains("Not analyzed") && c.contains("Stuffing"))
        );
        assert!(incubation(Some("salmonellosis"), None, None, None).unwrap().is_some());
    }

    #[test]
    fn suppression_hides_small_cells() {
        let (rows, any) = suppressed_rows(
            &["a".into(), "b".into(), "c".into()],
            &[2, 10, 12],
            Some(&[8, 9, 11]),
            5,
        );
        assert!(any);
        assert!(rows[0].cases.is_none() && rows[0].attack_rate_pct.is_none());
    }
}
