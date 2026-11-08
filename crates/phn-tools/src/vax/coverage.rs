//! VAX-05 `analyze_vaccination_coverage`.
//!
//! Ranks schools (and summarizes towns) from DPH's school immunization survey
//! snapshot, flags those below a population-immunity threshold, and compares
//! Massachusetts with the United States using CDC's National Immunization
//! Survey and VaxView estimates. Optionally analyzes the nurse's own
//! school-level counts from a file in /data, publishing rates only for schools
//! with at least 30 students (DPH's rule).

use std::collections::BTreeMap;

use phn_core::text;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;
use crate::vax::content::{self, THRESH};

const SURVEY_ID: &str = "ma-school-immunization-survey";
const SURVEY_FILE: &str = "ma_school_immunization_survey.csv";
const NIS_ID: &str = "nis-coverage-ma-us";
const NIS_FILE: &str = "nis_coverage_ma_us.csv";
/// DPH does not publish single-year rates for schools with fewer students.
pub const DPH_MIN_STUDENTS: u64 = 30;

/// Arguments for `analyze_vaccination_coverage`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct CoverageArgs {
    /// Massachusetts city or town (default: the configured town; omit both for statewide).
    pub town: Option<String>,
    /// County, e.g. "Hampshire".
    pub county: Option<String>,
    /// Part of a school name to filter on.
    pub school: Option<String>,
    /// "K" (kindergarten, default), "7" (grade 7), or "K3" (kindergarten
    /// 2023-2026 combined, which includes many small schools).
    pub grade: Option<String>,
    /// mmr (default), dtap, polio, hepb, var, tdap, men, series, exemption,
    /// unimmunized, no_record, noncompliance, or influenza (MA vs US only).
    pub measure: Option<String>,
    /// Flag schools below this coverage percent (default: the measure's
    /// threshold, e.g. 95 for MMR/measles).
    pub threshold_pct: Option<f64>,
    /// Schools to list (default 25, max 200).
    pub limit: Option<usize>,
    /// Statewide: ignore the configured town.
    pub statewide: bool,
    /// Optional: your own school-level counts (CSV/XLSX in /data) with columns
    /// school, enrolled, and a count column named after the measure (or
    /// up_to_date). Student-level files are refused.
    pub local_path: Option<String>,
    /// Sheet for an XLSX local file.
    pub sheet: Option<String>,
}

/// One school in the ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SchoolRate {
    pub school: String,
    pub school_type: String,
    pub municipality: String,
    pub county: String,
    /// Percent for the measure (whole percent, as DPH publishes).
    pub pct: f64,
    pub below_threshold: bool,
    /// Exemption percent (any vaccine), when published.
    pub exemption_pct: Option<f64>,
}

/// A town summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TownSummary {
    pub municipality: String,
    pub schools_reported: usize,
    pub schools_below_threshold: usize,
    pub schools_not_released: usize,
    pub min_pct: Option<f64>,
    /// Median of school rates (unweighted; DPH publishes no enrollment counts).
    pub median_pct: Option<f64>,
}

/// A survey estimate for Massachusetts or the U.S.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SurveyEstimate {
    pub survey: String,
    pub description: String,
    pub period: String,
    pub massachusetts_pct: Option<String>,
    pub massachusetts_ci: Option<String>,
    pub us_pct: Option<String>,
    pub us_ci: Option<String>,
}

/// Threshold used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThresholdUsed {
    pub pct: Option<f64>,
    pub basis: String,
    pub diseases: Vec<String>,
}

/// Result of `analyze_vaccination_coverage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageResult {
    pub scope: String,
    pub grade: String,
    pub period: String,
    pub measure: String,
    pub threshold: ThresholdUsed,
    pub schools_in_scope: usize,
    pub schools_with_rates: usize,
    pub schools_not_released: usize,
    pub schools_no_response: usize,
    pub schools_below_threshold: usize,
    /// Lowest first for coverage measures; highest first for exemption and
    /// noncompliance measures.
    pub ranked: Vec<SchoolRate>,
    pub towns: Vec<TownSummary>,
    pub comparison: Vec<SurveyEstimate>,
    /// Present when a local file was analyzed.
    pub local: Option<LocalCoverage>,
}

/// Local-file analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocalCoverage {
    pub file: String,
    pub schools: usize,
    pub schools_released: usize,
    pub schools_withheld_under_30: usize,
    pub overall_pct: Option<f64>,
    pub ranked: Vec<SchoolRate>,
}

#[derive(Debug, Clone)]
struct SurveyRow {
    grade: String,
    period: String,
    school: String,
    school_type: String,
    municipality: String,
    county: String,
    status: String,
    values: BTreeMap<String, String>,
}

fn parse_survey(text: &str) -> Result<Vec<SurveyRow>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers = rdr
        .headers()
        .map_err(|e| PhnError::Internal(format!("school survey snapshot: {e}")))?
        .clone();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| PhnError::Internal(format!("school survey snapshot: {e}")))?;
        let mut m: BTreeMap<String, String> = headers
            .iter()
            .zip(rec.iter())
            .map(|(h, v)| (h.to_string(), v.to_string()))
            .collect();
        let mut take = |k: &str| m.remove(k).unwrap_or_default();
        out.push(SurveyRow {
            grade: take("grade"),
            period: take("period"),
            school: take("school"),
            school_type: take("school_type"),
            municipality: take("municipality"),
            county: take("county"),
            status: take("status"),
            values: m,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Deserialize)]
struct NisRow {
    survey: String,
    vaccine: String,
    dose: String,
    geography: String,
    period: String,
    group: String,
    coverage_pct: String,
    ci_95: String,
}

const COVERAGE_MEASURES: [&str; 8] = ["mmr", "dtap", "polio", "hepb", "var", "tdap", "men", "series"];
const BURDEN_MEASURES: [&str; 4] = ["exemption", "unimmunized", "no_record", "noncompliance"];

fn measure_arg(m: Option<&str>) -> Result<String> {
    let t = m.map(|s| text::normalize(s).replace(' ', "_")).unwrap_or_else(|| "mmr".into());
    let t = match t.as_str() {
        "varicella" | "chickenpox" => "var".to_string(),
        "menacwy" | "meningococcal" | "mcv4" => "men".to_string(),
        "hep_b" | "hepatitis_b" => "hepb".to_string(),
        "ipv" => "polio".to_string(),
        "exemptions" => "exemption".to_string(),
        "non_compliance" | "not_compliant" | "gap" => "noncompliance".to_string(),
        "flu" => "influenza".to_string(),
        "measles" => "mmr".to_string(),
        other => other.to_string(),
    };
    if COVERAGE_MEASURES.contains(&t.as_str()) || BURDEN_MEASURES.contains(&t.as_str()) || t == "influenza" {
        Ok(t)
    } else {
        Err(PhnError::InvalidInput(format!(
            "measure '{t}' is not recognized; use one of {}, {}, or influenza",
            COVERAGE_MEASURES.join(", "),
            BURDEN_MEASURES.join(", ")
        )))
    }
}

fn threshold_for(measure: &str, override_pct: Option<f64>) -> Result<ThresholdUsed> {
    if let Some(p) = override_pct {
        if !(1.0..=100.0).contains(&p) {
            return Err(PhnError::InvalidInput("threshold_pct must be between 1 and 100".into()));
        }
        return Ok(ThresholdUsed {
            pct: Some(p),
            basis: "set by you".into(),
            diseases: Vec::new(),
        });
    }
    if BURDEN_MEASURES.contains(&measure) {
        return Ok(ThresholdUsed {
            pct: None,
            basis: "no threshold for this measure; schools are ranked highest first".into(),
            diseases: Vec::new(),
        });
    }
    let diseases = THRESH.measure_map.get(measure).cloned().unwrap_or_default();
    let entries: Vec<&content::ThresholdEntry> = THRESH
        .thresholds
        .iter()
        .filter(|t| diseases.contains(&t.id))
        .collect();
    match entries.iter().map(|t| t.flag_default_pct).fold(None, |a: Option<f64>, b| Some(a.map_or(b, |x| x.max(b)))) {
        Some(p) => Ok(ThresholdUsed {
            pct: Some(p),
            basis: entries
                .iter()
                .map(|t| {
                    format!(
                        "{}: herd immunity about {}% (R0 {}); flag at {}% — {}",
                        t.disease,
                        match (t.threshold_pct_low, t.threshold_pct_high) {
                            (Some(a), Some(b)) => format!("{a}-{b}"),
                            _ => "not established here".into(),
                        },
                        t.r0.clone().unwrap_or_else(|| "n/a".into()),
                        t.flag_default_pct,
                        t.flag_basis
                    )
                })
                .collect::<Vec<_>>()
                .join(" "),
            diseases: entries.iter().map(|t| t.disease.clone()).collect(),
        }),
        None => Ok(ThresholdUsed {
            pct: None,
            basis: "no population-immunity threshold applies to this measure; set threshold_pct to flag schools".into(),
            diseases: Vec::new(),
        }),
    }
}

fn comparison(ctx: &Context, grade: &str, measure: &str) -> Result<(Vec<SurveyEstimate>, Source)> {
    let (text, entry, _) = ctx.snapshots.file_text(NIS_ID, NIS_FILE)?;
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let rows: Vec<NisRow> = rdr
        .deserialize()
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| PhnError::Internal(format!("NIS snapshot: {e}")))?;
    let wants: Vec<(&str, &str, &str, &str, &str)> = match (grade, measure) {
        (_, "influenza") => vec![
            ("flu", "Seasonal Influenza", "", "6 Months - 17 Years", "Influenza, 6 months-17 years (FluVaxView, May estimate)"),
            ("flu", "Seasonal Influenza", "", ">=18 Years", "Influenza, adults 18+ (FluVaxView, May estimate)"),
            ("flu", "Seasonal Influenza", "", ">=65 Years", "Influenza, adults 65+ (FluVaxView, May estimate)"),
        ],
        ("7", "mmr") => vec![("nis_teen", "≥2 Doses MMR", "", "13-17 Years", "2+ doses MMR, ages 13-17 (NIS-Teen)")],
        ("7", "hepb") => vec![("nis_teen", "≥3 Doses HepB", "", "13-17 Years", "3+ doses HepB, ages 13-17 (NIS-Teen)")],
        ("7", "var") => vec![("nis_teen", "Varicella", "≥2 Doses or history of disease", "13-17 Years", "2+ varicella doses or history, ages 13-17 (NIS-Teen)")],
        ("7", "tdap") => vec![("nis_teen", "Tetanus", "≥1 Dose Tdap", "13-17 Years", "1+ dose Tdap, ages 13-17 (NIS-Teen)")],
        ("7", "men") => vec![("nis_teen", "≥1 Dose MenACWY", "", "13-17 Years", "1+ dose MenACWY, ages 13-17 (NIS-Teen)")],
        (_, "mmr") => vec![
            ("kindergarten", "MMR", "", "Kindergarten", "MMR, kindergarten (SchoolVaxView)"),
            ("nis_child", "≥1 Dose MMR", "", "24 Months", "1+ dose MMR by 24 months (NIS-Child, birth year)"),
        ],
        (_, "dtap") => vec![
            ("kindergarten", "DTP, DTaP, or DT", "", "Kindergarten", "DTaP, kindergarten (SchoolVaxView)"),
            ("nis_child", "DTaP", "≥4 Doses", "24 Months", "4+ doses DTaP by 24 months (NIS-Child)"),
        ],
        (_, "polio") => vec![
            ("kindergarten", "Polio", "", "Kindergarten", "Polio, kindergarten (SchoolVaxView)"),
            ("nis_child", "Polio", "≥3 Doses", "24 Months", "3+ doses polio by 24 months (NIS-Child)"),
        ],
        (_, "hepb") => vec![
            ("kindergarten", "Hepatitis B", "", "Kindergarten", "Hepatitis B, kindergarten (SchoolVaxView)"),
            ("nis_child", "Hep B", "≥3 Doses", "24 Months", "3+ doses HepB by 24 months (NIS-Child)"),
        ],
        (_, "var") => vec![
            ("kindergarten", "Varicella", "2 Doses (or disease history)", "Kindergarten", "2 doses varicella or history, kindergarten (SchoolVaxView)"),
        ],
        (_, "exemption") => vec![
            ("kindergarten", "Exemption", "Any Exemption", "Kindergarten", "Any exemption, kindergarten (SchoolVaxView)"),
            ("kindergarten", "Exemption", "Non-Medical Exemption", "Kindergarten", "Non-medical exemption, kindergarten (SchoolVaxView)"),
        ],
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for (survey, vaccine, dose, group, desc) in wants {
        let matching: Vec<&NisRow> = rows
            .iter()
            .filter(|r| r.survey == survey && r.vaccine == vaccine && r.dose == dose && r.group == group)
            .collect();
        let Some(period) = matching
            .iter()
            .filter(|r| r.geography == "Massachusetts")
            .map(|r| r.period.clone())
            .max()
        else {
            continue;
        };
        let get = |geo: &str| matching.iter().find(|r| r.geography == geo && r.period == period).copied();
        let ma = get("Massachusetts");
        let us = get("United States");
        let clean = |s: &str| (!s.is_empty()).then(|| s.to_string());
        out.push(SurveyEstimate {
            survey: survey.into(),
            description: desc.into(),
            period: period.clone(),
            massachusetts_pct: ma.and_then(|r| clean(&r.coverage_pct)),
            massachusetts_ci: ma.and_then(|r| clean(&r.ci_95)),
            us_pct: us.and_then(|r| clean(&r.coverage_pct)),
            us_ci: us.and_then(|r| clean(&r.ci_95)),
        });
    }
    Ok((out, entry.to_source()))
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

/// Rank schools and towns and compare Massachusetts with the U.S.
pub fn analyze_vaccination_coverage(
    ctx: &Context,
    args: CoverageArgs,
) -> Result<Envelope<CoverageResult>> {
    let measure = measure_arg(args.measure.as_deref())?;
    let grade = match args.grade.as_deref().map(|g| g.trim().to_ascii_uppercase()) {
        None => "K".to_string(),
        Some(g) if g == "K" || g == "KINDERGARTEN" => "K".into(),
        Some(g) if g == "7" || g == "GRADE 7" || g == "7TH" => "7".into(),
        Some(g)
            if ["K3", "K-3", "K 3"].contains(&g.as_str())
                || g.contains("COMBINED")
                || g.contains("3-YEAR")
                || g.contains("3 YEAR")
                || g.contains("THREE") =>
        {
            "K3".into()
        }
        Some(g) => {
            return Err(PhnError::InvalidInput(format!(
                "grade '{g}' is not available; DPH publishes school-level data for kindergarten (K, or K3 for 2023-2026 combined) and grade 7"
            )));
        }
    };
    if grade == "7" && ["dtap", "polio"].contains(&measure.as_str()) {
        return Err(PhnError::InvalidInput(format!(
            "DPH's grade 7 file does not include {measure}; use mmr, hepb, var, tdap, men, series, exemption, unimmunized, no_record, or noncompliance"
        )));
    }
    if grade != "7" && ["tdap", "men"].contains(&measure.as_str()) {
        return Err(PhnError::InvalidInput(format!(
            "{measure} is reported for grade 7, not kindergarten; set grade to 7"
        )));
    }
    let threshold = threshold_for(&measure, args.threshold_pct)?;
    let limit = args.limit.unwrap_or(25).clamp(1, 200);
    let burden = BURDEN_MEASURES.contains(&measure.as_str());

    // Scope.
    let town = match (&args.town, args.statewide) {
        (Some(t), _) => Some(
            ctx.ma
                .town(t)
                .ok_or_else(|| PhnError::InvalidInput(format!("'{t}' is not one of the 351 Massachusetts cities and towns")))?
                .name
                .clone(),
        ),
        (None, false) if args.county.is_none() && args.school.is_none() => ctx.settings().jurisdiction.town,
        _ => None,
    };
    let county = args.county.as_ref().map(|c| c.trim().trim_end_matches(" County").to_ascii_uppercase());
    let scope = match (&town, &county, &args.school) {
        (Some(t), _, _) => t.clone(),
        (None, Some(c), _) => format!("{} County", text::title_case(c)),
        (None, None, Some(s)) => format!("schools matching '{s}'"),
        _ => "Massachusetts (statewide)".into(),
    };

    let mut sources: Vec<Source> = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    let mut ranked = Vec::new();
    let mut towns = Vec::new();
    let (mut in_scope, mut with_rates, mut not_released, mut no_response, mut below) = (0, 0, 0, 0, 0);
    let mut period = String::new();
    if measure != "influenza" {
        let (text, entry, _) = ctx.snapshots.file_text(SURVEY_ID, SURVEY_FILE)?;
        sources.push(entry.to_source());
        let rows = parse_survey(&text)?;
        let (want_grade, want_period) = match grade.as_str() {
            "K3" => ("K", "2023-26 combined"),
            "7" => ("7", "2025-26"),
            _ => ("K", "2025-26"),
        };
        period = want_period.to_string();
        let town_key = town.as_ref().map(|t| text::normalize(t));
        let school_q = args.school.as_ref().map(|s| text::normalize(s));
        let selected: Vec<&SurveyRow> = rows
            .iter()
            .filter(|r| r.grade == want_grade && r.period == want_period)
            .filter(|r| town_key.as_ref().is_none_or(|t| text::normalize(&r.municipality) == *t))
            .filter(|r| county.as_ref().is_none_or(|c| r.county.eq_ignore_ascii_case(c)))
            .filter(|r| school_q.as_ref().is_none_or(|q| text::normalize(&r.school).contains(q.as_str())))
            .collect();
        if selected.is_empty() {
            return Err(PhnError::NotFound(format!(
                "no schools found for {scope} in DPH's {} file",
                if grade == "7" { "grade 7" } else { "kindergarten" }
            )));
        }
        in_scope = selected.len();
        let mut by_town: BTreeMap<String, (usize, usize, usize, Vec<f64>)> = BTreeMap::new();
        for r in &selected {
            let t = by_town.entry(text::title_case(&r.municipality)).or_default();
            match r.status.as_str() {
                "reported" => {}
                "no_response" => {
                    no_response += 1;
                    continue;
                }
                _ => {
                    not_released += 1;
                    t.2 += 1;
                    continue;
                }
            }
            let Some(pct) = r.values.get(&measure).and_then(|v| v.parse::<f64>().ok()) else {
                continue;
            };
            with_rates += 1;
            let flag = threshold.pct.is_some_and(|th| !burden && pct < th);
            if flag {
                below += 1;
                t.1 += 1;
            }
            t.0 += 1;
            t.3.push(pct);
            ranked.push(SchoolRate {
                school: text::title_case(&r.school),
                school_type: r.school_type.to_lowercase(),
                municipality: text::title_case(&r.municipality),
                county: text::title_case(&r.county),
                pct,
                below_threshold: flag,
                exemption_pct: r.values.get("exemption").and_then(|v| v.parse().ok()),
            });
        }
        if burden {
            ranked.sort_by(|a, b| b.pct.total_cmp(&a.pct).then(a.school.cmp(&b.school)));
        } else {
            ranked.sort_by(|a, b| a.pct.total_cmp(&b.pct).then(a.school.cmp(&b.school)));
        }
        for (m, (n, b, nr, mut v)) in by_town {
            // The extreme rate: the lowest for coverage, the highest for burden measures.
            let min = v.iter().copied().fold(None, |a: Option<f64>, x| {
                Some(a.map_or(x, |y| if burden { y.max(x) } else { y.min(x) }))
            });
            towns.push(TownSummary {
                municipality: m,
                schools_reported: n,
                schools_below_threshold: b,
                schools_not_released: nr,
                min_pct: min,
                median_pct: median(&mut v),
            });
        }
        towns.sort_by(|a, b| {
            b.schools_below_threshold
                .cmp(&a.schools_below_threshold)
                .then(a.min_pct.unwrap_or(101.0).total_cmp(&b.min_pct.unwrap_or(101.0)))
        });
        caveats.push(format!("{} Rates are self-reported by schools and can change during the year.", THRESH.dph_small_school_rule));
        caveats.push(
            "Town figures summarize school rates without weighting by enrollment, because DPH's files \
             publish rates but not counts. Use them to find places to look, not as town coverage estimates."
                .into(),
        );
    }

    let (comparison, nis_source) = comparison(ctx, if grade == "7" { "7" } else { "K" }, &measure)?;
    if !comparison.is_empty() {
        sources.push(nis_source);
        caveats.push(
            "National Immunization Survey and VaxView figures are survey estimates with sampling \
             error; compare confidence intervals. NIS-Child is reported by birth year and lags the \
             calendar."
                .into(),
        );
    }

    // Optional local file.
    let local = match &args.local_path {
        Some(p) => Some(local_coverage(ctx, p, args.sheet.as_deref(), &measure, threshold.pct, burden, &mut sources)?),
        None => None,
    };

    // Summary.
    let measure_label = match measure.as_str() {
        "var" => "varicella".to_string(),
        "men" => "MenACWY".to_string(),
        "series" => "all required vaccines (series complete)".to_string(),
        other => other.to_uppercase(),
    };
    let mut summary = format!(
        "**Vaccination coverage — {measure_label}, {}, {scope}**{}\n\n",
        match grade.as_str() {
            "7" => "grade 7",
            "K3" => "kindergarten (2023-2026 combined)",
            _ => "kindergarten",
        },
        if period.is_empty() { String::new() } else { format!(" ({period})") }
    );
    if measure != "influenza" {
        summary.push_str(&format!(
            "{in_scope} school(s) in scope: {with_rates} with published rates, {not_released} not released by DPH (fewer than 30 students or unresolved discrepancies), {no_response} did not return a survey.\n",
        ));
        if let Some(th) = threshold.pct
            && !burden
        {
            summary.push_str(&format!("**{below} school(s) below {th}%.** {}\n\n", threshold.basis));
        } else {
            summary.push_str(&format!("{}\n\n", threshold.basis));
        }
        let rows: Vec<Vec<String>> = ranked
            .iter()
            .take(limit)
            .map(|r| {
                vec![
                    r.school.clone(),
                    r.municipality.clone(),
                    r.school_type.clone(),
                    format!("{}%{}", r.pct, if r.below_threshold { " ⚠" } else { "" }),
                    r.exemption_pct.map(|e| format!("{e}%")).unwrap_or_default(),
                ]
            })
            .collect();
        summary.push_str(&md::table(&["School", "Town", "Type", &measure_label, "Exemption"], &rows));
        if ranked.len() > limit {
            summary.push_str(&format!("\n…and {} more (raise limit to see them).\n", ranked.len() - limit));
        }
        if town.is_none() && towns.len() > 1 {
            if burden {
                // Highest rates first for exemptions and other burden measures.
                towns.sort_by(|a, b| {
                    b.min_pct
                        .unwrap_or(-1.0)
                        .total_cmp(&a.min_pct.unwrap_or(-1.0))
                        .then(a.municipality.cmp(&b.municipality))
                });
            }
            summary.push_str(match (threshold.pct.is_some() && !burden, burden) {
                (true, _) => "\n**Towns with the most schools below the threshold**\n\n",
                (false, true) => "\n**Towns by school rate (highest first)**\n\n",
                (false, false) => "\n**Towns by lowest school rate**\n\n",
            });
            let rows: Vec<Vec<String>> = towns
                .iter()
                .filter(|t| t.schools_reported > 0)
                .take(10)
                .map(|t| {
                    vec![
                        t.municipality.clone(),
                        t.schools_reported.to_string(),
                        t.schools_below_threshold.to_string(),
                        t.min_pct.map(|m| format!("{m}%")).unwrap_or_default(),
                        t.median_pct.map(|m| format!("{m}%")).unwrap_or_default(),
                    ]
                })
                .collect();
            summary.push_str(&md::table(
                &[
                    "Town",
                    "Schools with rates",
                    "Below threshold",
                    if burden { "Highest" } else { "Lowest" },
                    "Median (unweighted)",
                ],
                &rows,
            ));
        }
    }
    if !comparison.is_empty() {
        summary.push_str("\n**Massachusetts vs United States**\n\n");
        let rows: Vec<Vec<String>> = comparison
            .iter()
            .map(|c| {
                let f = |p: &Option<String>, ci: &Option<String>| match (p, ci) {
                    (Some(p), Some(ci)) => format!("{p}% ({ci})"),
                    (Some(p), None) => format!("{p}%"),
                    _ => "—".into(),
                };
                vec![c.description.clone(), c.period.clone(), f(&c.massachusetts_pct, &c.massachusetts_ci), f(&c.us_pct, &c.us_ci)]
            })
            .collect();
        summary.push_str(&md::table(&["Measure", "Period", "Massachusetts (95% CI)", "United States (95% CI)"], &rows));
    }
    if let Some(l) = &local {
        summary.push_str(&format!(
            "\n**Your file ({})**: {} school(s); rates shown for {} with 30 or more students, {} withheld (fewer than 30, following DPH).{}\n",
            l.file,
            l.schools,
            l.schools_released,
            l.schools_withheld_under_30,
            l.overall_pct.map(|p| format!(" Overall: {p:.1}%.")).unwrap_or_default()
        ));
        let rows: Vec<Vec<String>> = l
            .ranked
            .iter()
            .take(limit)
            .map(|r| vec![r.school.clone(), format!("{}%{}", r.pct, if r.below_threshold { " ⚠" } else { "" })])
            .collect();
        summary.push_str(&md::table(&["School", &measure_label], &rows));
    }
    caveats.extend(content::review_caveats(&[content::THRESHOLDS]));
    if let Ok(s) = content::content_source(content::THRESHOLDS) {
        sources.push(s);
    }
    let privacy = if local.is_some() {
        Privacy {
            suppression_applied: true,
            suppression_threshold: Some(ctx.suppression_threshold()),
            notes: vec!["Your file's rates are shown only for schools with 30 or more students; no counts are returned.".into()],
            ..Privacy::none()
        }
    } else {
        Privacy::none()
    };
    Ok(Envelope::new(
        summary,
        CoverageResult {
            scope,
            grade,
            period,
            measure,
            threshold,
            schools_in_scope: in_scope,
            schools_with_rates: with_rates,
            schools_not_released: not_released,
            schools_no_response: no_response,
            schools_below_threshold: below,
            ranked: ranked.into_iter().take(limit).collect(),
            towns,
            comparison,
            local,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}

fn local_coverage(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    measure: &str,
    threshold: Option<f64>,
    burden: bool,
    sources: &mut Vec<Source>,
) -> Result<LocalCoverage> {
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let (table, label, findings) = (&lt.table, lt.label.clone(), &lt.findings);
    if findings.iter().any(|f| f.kind.is_direct()) {
        return Err(PhnError::Privacy(format!(
            "'{label}' looks like a student-level file (it has identifier columns). This tool needs \
             school-level counts (school, enrolled, and a count column). For a student roster use \
             check_ma_school_immunization_requirements with roster_path, which writes student \
             results to /data/output and returns only counts."
        )));
    }
    let col = |names: &[&str]| {
        table.headers.iter().position(|h| {
            let n = text::normalize(h).replace(' ', "_");
            names.contains(&n.as_str())
        })
    };
    let school_col = col(&["school", "school_name", "name"]);
    let enrolled_col = col(&["enrolled", "enrollment", "total", "students", "n"]);
    let measure_names: Vec<&str> = match measure {
        "var" => vec!["var", "varicella"],
        "men" => vec!["men", "menacwy"],
        other => vec![other],
    };
    let mut wanted = measure_names.clone();
    wanted.extend(["up_to_date", "vaccinated", "count"]);
    let count_col = col(&wanted);
    let (Some(sc), Some(ec), Some(cc)) = (school_col, enrolled_col, count_col) else {
        return Err(PhnError::InvalidInput(format!(
            "'{label}' needs columns 'school', 'enrolled', and a count column named '{}' (or 'up_to_date'). Columns found: {}",
            measure_names[0],
            table.headers.join(", ")
        )));
    };
    let mut ranked = Vec::new();
    let (mut withheld, mut tot_n, mut tot_c) = (0usize, 0u64, 0u64);
    for row in &table.rows {
        let get = |i: usize| row.get(i).map(|s| s.trim()).unwrap_or("");
        let (Ok(n), Ok(c)) = (get(ec).replace(',', "").parse::<u64>(), get(cc).replace(',', "").parse::<u64>()) else {
            continue;
        };
        if n == 0 || c > n {
            continue;
        }
        tot_n += n;
        tot_c += c;
        if n < DPH_MIN_STUDENTS {
            withheld += 1;
            continue;
        }
        let pct = (c as f64 * 100.0 / n as f64).round();
        let name = get(sc);
        if !phn_local::phi::scan_text(name).is_empty() {
            return Err(PhnError::Privacy(format!(
                "values in the school column of '{label}' look like identifiers, so they cannot be shown"
            )));
        }
        ranked.push(SchoolRate {
            school: name.to_string(),
            school_type: String::new(),
            municipality: String::new(),
            county: String::new(),
            pct,
            below_threshold: threshold.is_some_and(|t| !burden && pct < t),
            exemption_pct: None,
        });
    }
    if burden {
        ranked.sort_by(|a, b| b.pct.total_cmp(&a.pct));
    } else {
        ranked.sort_by(|a, b| a.pct.total_cmp(&b.pct));
    }
    sources.push(Source::local_file(label.clone()));
    Ok(LocalCoverage {
        file: label,
        schools: ranked.len() + withheld,
        schools_released: ranked.len(),
        schools_withheld_under_30: withheld,
        overall_pct: (tot_n >= DPH_MIN_STUDENTS).then(|| (tot_c as f64 * 1000.0 / tot_n as f64).round() / 10.0),
        ranked,
    })
}
