//! MH-07 `get_youth_risk_behavior_data`: Massachusetts youth survey
//! indicators (persistent sadness, suicidal ideation, bullying, substance use,
//! protective factors, and others) by sex, grade, race/ethnicity, and LGBTQ
//! identity with 95% confidence intervals, compared with the U.S.
//!
//! * DPH's Massachusetts Youth Health Survey (MYHS) 2023 and 2025 reports and
//!   the 2023 joint MYHS/YRBS report (bundled snapshot of the report tables).
//! * CDC's YRBSS mental health indicators for Massachusetts and the nation,
//!   2019-2023 (data.cdc.gov `nu3s-3dwd`), live → cache → snapshot.
//!
//! Differences follow DPH's rule: two estimates differ when their 95%
//! confidence intervals do not overlap.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::socrata::{self, SocrataQuery};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::cd::surveillance::{fallback_caveat, snapshot_fallback};
use crate::comms::content::{self, NamedSource, YOUTH_SURVEY};
use crate::context::Context;
use crate::md;

const DPH_SNAPSHOT: &str = "ma-youth-health-survey";
const DPH_FILE: &str = "ma_youth_survey.csv";
const DPH_INDICATORS: &str = "ma_youth_survey_indicators.csv";
const CDC_SNAPSHOT: &str = "cdc-yrbs-mental-health-ma-us";
const CDC_FILE: &str = "cdc_yrbs_mental_health_ma_us.csv";
const CDC_DATASET: &str = "nu3s-3dwd";
const TTL_SECS: u64 = 7 * 24 * 3600;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, Deserialize)]
struct Cited {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct YouthContent {
    meta: ContentMeta,
    sources: BTreeMap<String, NamedSource>,
    surveys: Vec<Cited>,
    notes: Vec<Cited>,
    crisis: Cited,
}

static CONTENT: LazyLock<YouthContent> = LazyLock::new(|| {
    content::load(YOUTH_SURVEY).expect("content/bh/youth_survey.yaml matches its schema")
});

/// The 988 Lifeline text shown with suicide-related answers (from DPH's 988 page).
pub fn crisis_line() -> String {
    let c = &CONTENT.crisis;
    let url = CONTENT
        .sources
        .get(&c.source)
        .and_then(|s| s.url.clone())
        .unwrap_or_default();
    format!("{} ({url})", c.text)
}

// ---------------------------------------------------------------- arguments and results

/// Arguments for `get_youth_risk_behavior_data`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct YouthSurveyArgs {
    /// Words describing the indicators (e.g. "sad or hopeless", "suicide", "bullied", "vaping",
    /// "alcohol", "marijuana", "belong at school", "concussion"). Default: the mental health and
    /// suicide indicators.
    pub topic: Option<String>,
    /// "high" (default), "middle", or "both".
    pub school_level: Option<String>,
    /// Groups to show: any of "sex", "grade", "race_ethnicity", "lgbtq" (default: all).
    pub by: Vec<String>,
    /// "MYHS", "MYRBS", or omit for both Massachusetts surveys.
    pub survey: Option<String>,
    /// Add CDC's YRBS mental health estimates for Massachusetts and the U.S. (default true).
    pub compare_us: Option<bool>,
    /// Most Massachusetts indicators to return (default 8, at most 25).
    pub max_indicators: Option<usize>,
}

/// A weighted percentage with its 95% confidence interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SurveyEstimate {
    /// "Overall", "Grade", "Gender", "Sex", "Race/Ethnicity", or "Sexual Orientation and Gender Identity".
    pub group_type: String,
    pub group: String,
    pub percent: Option<f64>,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    /// Not presented by the publisher (too few respondents or unstable).
    pub suppressed: bool,
    /// Compared with the reference group (male, White, straight/cisgender), when the 95%
    /// intervals do not overlap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versus_reference: Option<String>,
}

/// One Massachusetts indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndicatorResult {
    /// "MYHS" or "MYRBS".
    pub survey: String,
    pub year: i32,
    /// "high" or "middle".
    pub school_level: String,
    pub topic: String,
    pub indicator: String,
    pub overall: Option<SurveyEstimate>,
    pub groups: Vec<SurveyEstimate>,
    /// Change from the same survey two years earlier, when the same indicator was asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    pub source: String,
}

/// CDC YRBS estimates for one question and year.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CdcYear {
    pub year: i32,
    pub massachusetts: Option<SurveyEstimate>,
    pub united_states: Option<SurveyEstimate>,
    /// "higher", "lower", or "not clearly different" for Massachusetts vs the U.S.
    pub comparison: String,
}

/// One CDC YRBS mental health question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CdcQuestion {
    pub question: String,
    pub years: Vec<CdcYear>,
    /// Massachusetts by sex and race/ethnicity in the latest year.
    pub massachusetts_groups: Vec<SurveyEstimate>,
}

/// Result of `get_youth_risk_behavior_data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct YouthSurveyResult {
    /// Indicators that matched before `max_indicators`.
    pub matched: usize,
    pub indicators: Vec<IndicatorResult>,
    pub cdc_comparison: Vec<CdcQuestion>,
    /// Shown when results involve suicide or sadness.
    pub crisis_line: Option<String>,
    pub notes: Vec<String>,
    pub method: String,
}

// ---------------------------------------------------------------- data

#[derive(Debug, Clone)]
struct Indicator {
    id: String,
    survey: String,
    year: i32,
    level: String,
    topic: String,
    text: String,
}

fn num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

/// Indicators, estimates keyed by indicator id, and the snapshot citation.
type DphData = (Vec<Indicator>, Vec<(String, SurveyEstimate)>, Source);

fn load_dph(ctx: &Context) -> Result<DphData> {
    let (itext, entry, _) = ctx.snapshots.file_text(DPH_SNAPSHOT, DPH_INDICATORS)?;
    let (etext, _, _) = ctx.snapshots.file_text(DPH_SNAPSHOT, DPH_FILE)?;
    let mut inds = Vec::new();
    for r in csv::Reader::from_reader(itext.as_bytes()).records() {
        let r = r.map_err(|e| PhnError::Internal(format!("{DPH_INDICATORS}: {e}")))?;
        inds.push(Indicator {
            id: r[0].to_string(),
            survey: r[1].to_string(),
            year: r[2].parse().unwrap_or(0),
            level: r[3].to_string(),
            topic: r[4].to_string(),
            text: r[5].to_string(),
        });
    }
    let mut ests = Vec::new();
    for r in csv::Reader::from_reader(etext.as_bytes()).records() {
        let r = r.map_err(|e| PhnError::Internal(format!("{DPH_FILE}: {e}")))?;
        ests.push((
            r[0].to_string(),
            SurveyEstimate {
                group_type: r[1].to_string(),
                group: r[2]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .replace(" / ", "/")
                    .replace("/ ", "/"),
                percent: num(&r[3]),
                ci_low: num(&r[4]),
                ci_high: num(&r[5]),
                suppressed: &r[6] == "1",
                versus_reference: None,
            },
        ));
    }
    Ok((inds, ests, entry.to_source()))
}

/// Expand a query into lower-case search stems.
fn search_terms(q: &str) -> Vec<String> {
    let q = q.to_lowercase();
    let mut terms = Vec::new();
    for (keys, add) in [
        (
            &["sad", "hopeless", "depress"][..],
            &["sad or hopeless"][..],
        ),
        (&["suicid"], &["suicide"]),
        (
            &[
                "self-harm",
                "self harm",
                "self-injur",
                "self injur",
                "cutting",
            ],
            &["injuring oneself"],
        ),
        (
            &[
                "vape",
                "vaping",
                "e-cig",
                "ecig",
                "electronic vapor",
                "juul",
            ],
            &["vap"],
        ),
        (&["bully", "bullied", "bullying"], &["bull"]),
        (&["marijuana", "cannabis", "weed", "thc"], &["marijuana"]),
        (&["alcohol", "drink", "binge"], &["alcohol", "drinking"]),
        (&["tobacco", "cigarette", "smok"], &["tobacco", "cigarette"]),
        (&["belong", "connected"], &["belong"]),
        (&["mental health"], &["mental health"]),
        (
            &["opioid", "pain medicine", "prescription"],
            &["pain", "prescription", "opioid"],
        ),
        (&["gambl", "betting"], &["gambl"]),
        (&["dating", "sexual violence"], &["dating", "sexual"]),
        (&["sleep"], &["sleep"]),
    ] {
        if keys.iter().any(|k| q.contains(k)) {
            terms.extend(add.iter().map(|s| s.to_string()));
        }
    }
    for w in q.split(|c: char| !c.is_alphanumeric() && c != '-') {
        if w.len() >= 4
            && !matches!(
                w,
                "data"
                    | "students"
                    | "student"
                    | "youth"
                    | "high"
                    | "school"
                    | "middle"
                    | "percent"
                    | "massachusetts"
                    | "with"
                    | "about"
                    | "past"
                    | "year"
            )
        {
            terms.push(w.trim_end_matches('s').to_string());
        }
    }
    terms.sort();
    terms.dedup();
    terms
}

fn score(text: &str, terms: &[String]) -> usize {
    let t = text.to_lowercase();
    terms.iter().filter(|w| t.contains(w.as_str())).count()
}

fn group_kind(group_type: &str) -> &'static str {
    let g = group_type.to_lowercase();
    if g.contains("sexual orientation") || g.contains("lgbtq") {
        "lgbtq"
    } else if g.contains("race") {
        "race_ethnicity"
    } else if g == "sex" || g == "gender" {
        "sex"
    } else if g == "grade" {
        "grade"
    } else {
        "overall"
    }
}

fn reference_group(kind: &str) -> Option<&'static str> {
    match kind {
        "sex" => Some("Male"),
        "race_ethnicity" => Some("White"),
        "lgbtq" => Some("Straight/Cisgender"),
        _ => None,
    }
}

/// "higher", "lower", or "not clearly different" by non-overlapping 95% intervals.
fn compare(a: &SurveyEstimate, b: &SurveyEstimate) -> Option<&'static str> {
    let (al, ah, bl, bh) = (a.ci_low?, a.ci_high?, b.ci_low?, b.ci_high?);
    Some(if al > bh {
        "higher"
    } else if ah < bl {
        "lower"
    } else {
        "not clearly different"
    })
}

fn fmt_est(e: &SurveyEstimate) -> String {
    match (e.percent, e.ci_low, e.ci_high) {
        _ if e.suppressed => "not presented (too few respondents)".into(),
        (Some(p), Some(l), Some(h)) => format!("{p:.1}% ({l:.1}-{h:.1})"),
        (Some(p), _, _) => format!("{p:.1}%"),
        _ => "not available".into(),
    }
}

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

fn cdc_estimate(row: &Map<String, Value>) -> SurveyEstimate {
    let p = socrata::text(row, "percent").unwrap_or_default();
    let suppressed = p.eq_ignore_ascii_case("suppressed");
    SurveyEstimate {
        group_type: socrata::text(row, "demographics_type")
            .unwrap_or("Total")
            .to_string(),
        group: socrata::text(row, "demographics_value")
            .unwrap_or("Total")
            .to_string(),
        percent: socrata::num(row, "percent"),
        ci_low: socrata::num(row, "low_confidence_interval"),
        ci_high: socrata::num(row, "high_confidence_interval"),
        suppressed,
        versus_reference: None,
    }
}

// ---------------------------------------------------------------- the tool

/// Report Massachusetts youth survey indicators with intervals, compared with the U.S.
pub async fn get_youth_risk_behavior_data(
    ctx: &Context,
    args: YouthSurveyArgs,
) -> Result<Envelope<YouthSurveyResult>> {
    let levels: Vec<&str> = match args
        .school_level
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .as_deref()
    {
        None | Some("") | Some("high") | Some("high school") | Some("hs") => vec!["high"],
        Some("middle") | Some("middle school") | Some("ms") => vec!["middle"],
        Some("both") | Some("all") => vec!["high", "middle"],
        Some(other) => {
            return Err(PhnError::InvalidInput(format!(
                "school_level '{other}' is not recognized; use high, middle, or both"
            )));
        }
    };
    let survey = match args
        .survey
        .as_deref()
        .map(|s| s.trim().to_uppercase())
        .as_deref()
    {
        None | Some("") | Some("ALL") | Some("BOTH") => None,
        Some("MYHS") | Some("YHS") => Some("MYHS"),
        Some("MYRBS") | Some("YRBS") => Some("MYRBS"),
        Some(other) => {
            return Err(PhnError::InvalidInput(format!(
                "survey '{other}' is not recognized; use MYHS or MYRBS"
            )));
        }
    };
    let mut wanted_kinds = Vec::new();
    for b in &args.by {
        let k = match b
            .trim()
            .to_lowercase()
            .replace([' ', '-', '/'], "_")
            .as_str()
        {
            "sex" | "gender" => "sex",
            "grade" => "grade",
            "race" | "race_ethnicity" | "ethnicity" | "race_and_ethnicity" => "race_ethnicity",
            "lgbtq" | "sexual_orientation" | "sogi" | "lgbt" => "lgbtq",
            other => {
                return Err(PhnError::InvalidInput(format!(
                    "by '{other}' is not recognized; use sex, grade, race_ethnicity, or lgbtq"
                )));
            }
        };
        wanted_kinds.push(k);
    }
    let max = args.max_indicators.unwrap_or(8).clamp(1, 25);
    let topic = args.topic.clone().filter(|t| !t.trim().is_empty());
    let terms = topic.as_deref().map(search_terms).unwrap_or_default();
    if topic.is_some() && terms.is_empty() {
        return Err(PhnError::InvalidInput(
            "give a topic of at least one meaningful word, e.g. \"suicide\", \"vaping\", or \"bullied\"".into(),
        ));
    }

    let (inds, ests, dph_source) = load_dph(ctx)?;
    let mut scored: Vec<(usize, &Indicator)> = inds
        .iter()
        .filter(|i| levels.contains(&i.level.as_str()))
        .filter(|i| survey.is_none_or(|s| i.survey == s))
        .filter_map(|i| {
            let text = format!("{} {}", i.topic, i.text);
            match &topic {
                None => {
                    let t = text.to_lowercase();
                    (t.contains("sad or hopeless")
                        || t.contains("suicid")
                        || t.contains("injuring oneself")
                        || t.contains("mental health"))
                    .then_some((1, i))
                }
                Some(_) => {
                    let s = score(&text, &terms);
                    (s > 0).then_some((s, i))
                }
            }
        })
        .collect();
    // Best match first; newer years first; MYHS before MYRBS within a year.
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.year.cmp(&a.1.year))
            .then(a.1.level.cmp(&b.1.level))
            .then(a.1.survey.cmp(&b.1.survey))
            .then(a.1.id.cmp(&b.1.id))
    });
    if scored.is_empty() && topic.is_some() {
        let topics: std::collections::BTreeSet<&str> =
            inds.iter().map(|i| i.topic.as_str()).collect();
        return Err(PhnError::NotFound(format!(
            "no Massachusetts youth survey indicator matches '{}'. Topics: {}",
            topic.unwrap_or_default(),
            topics.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    let matched = scored.len();
    let mut indicators = Vec::new();
    for (_, ind) in scored.into_iter().take(max) {
        let rows: Vec<&SurveyEstimate> = ests
            .iter()
            .filter(|(id, _)| *id == ind.id)
            .map(|(_, e)| e)
            .collect();
        let overall = rows
            .iter()
            .find(|e| e.group_type == "Overall")
            .map(|e| (*e).clone());
        let mut groups: Vec<SurveyEstimate> = rows
            .iter()
            .filter(|e| e.group_type != "Overall")
            .filter(|e| {
                wanted_kinds.is_empty() || wanted_kinds.contains(&group_kind(&e.group_type))
            })
            .map(|e| (*e).clone())
            .collect();
        for i in 0..groups.len() {
            let kind = group_kind(&groups[i].group_type);
            let Some(refname) = reference_group(kind) else {
                continue;
            };
            if groups[i].group == refname {
                continue;
            }
            let reference = rows
                .iter()
                .find(|e| group_kind(&e.group_type) == kind && e.group == refname)
                .map(|e| (*e).clone());
            if let Some(r) = reference
                && let Some(c) = compare(&groups[i], &r)
                && c != "not clearly different"
            {
                groups[i].versus_reference = Some(format!(
                    "{c} than {} students (95% CIs do not overlap)",
                    refname.to_lowercase()
                ));
            }
        }
        // Change from the same survey two years earlier.
        let change = inds
            .iter()
            .find(|o| {
                o.survey == ind.survey
                    && o.level == ind.level
                    && o.text == ind.text
                    && o.year == ind.year - 2
            })
            .and_then(|o| {
                let prev = ests
                    .iter()
                    .find(|(id, e)| *id == o.id && e.group_type == "Overall")
                    .map(|(_, e)| e)?;
                let now = overall.as_ref()?;
                let c = compare(now, prev)?;
                Some(format!(
                    "{} {}: {}; {}: {} ({})",
                    ind.survey,
                    o.year,
                    fmt_est(prev),
                    ind.year,
                    fmt_est(now),
                    match c {
                        "higher" => "higher, 95% CIs do not overlap",
                        "lower" => "lower, 95% CIs do not overlap",
                        _ => "no clear change",
                    }
                ))
            });
        let source_key = match (ind.survey.as_str(), ind.year) {
            (_, 2025) => "myhs_2025",
            ("MYHS", 2023) if ind.level == "high" || ind.level == "middle" => "myhs_2023",
            _ => "joint_2023",
        };
        indicators.push(IndicatorResult {
            survey: ind.survey.clone(),
            year: ind.year,
            school_level: ind.level.clone(),
            topic: ind.topic.clone(),
            indicator: ind.text.clone(),
            overall,
            groups,
            change,
            source: CONTENT
                .sources
                .get(source_key)
                .map(|s| s.title.clone())
                .unwrap_or_default(),
        });
    }

    // ---- CDC YRBS mental health indicators, Massachusetts vs U.S.
    let mut caveats = Vec::new();
    let mut sources = vec![dph_source];
    let mut cdc_comparison = Vec::new();
    if args.compare_us.unwrap_or(true) && levels.contains(&"high") {
        let q = SocrataQuery::cdc(CDC_DATASET)
            .select("year,question,area_abbr,demographics_type,demographics_value,percent,low_confidence_interval,high_confidence_interval")
            .and_where("area_abbr in ('MA','XX')")
            .limit(5_000);
        let fb = snapshot_fallback(ctx, CDC_SNAPSHOT, CDC_FILE, csv_to_socrata_json);
        match socrata::query(&ctx.http, ctx.cache.as_ref(), &q, TTL_SECS, Some(fb)).await {
            Ok(rows) => {
                if let Some(c) = fallback_caveat(ctx, &rows, "CDC YRBS estimates", CDC_SNAPSHOT) {
                    caveats.push(c);
                }
                sources.push(rows.source(
                    "Youth Risk Behavioral Surveillance System (YRBSS) - Mental Health Indicators (Massachusetts and national YRBS)",
                    "Centers for Disease Control and Prevention",
                ));
                let mut questions: Vec<String> = rows
                    .rows
                    .iter()
                    .filter(|r| socrata::text(r, "area_abbr") == Some("MA"))
                    .filter_map(|r| socrata::text(r, "question").map(str::to_string))
                    .collect();
                questions.sort();
                questions.dedup();
                for qn in questions {
                    if topic.is_some() && score(&qn, &terms) == 0 {
                        continue;
                    }
                    let of = |area: &str, year: i32, total: bool| -> Vec<SurveyEstimate> {
                        rows.rows
                            .iter()
                            .filter(|r| socrata::text(r, "question") == Some(qn.as_str()))
                            .filter(|r| socrata::text(r, "area_abbr") == Some(area))
                            .filter(|r| socrata::num(r, "year") == Some(f64::from(year)))
                            .filter(|r| {
                                (socrata::text(r, "demographics_type") == Some("Total")) == total
                            })
                            .map(cdc_estimate)
                            .collect()
                    };
                    let mut years: Vec<i32> = rows
                        .rows
                        .iter()
                        .filter(|r| socrata::text(r, "question") == Some(qn.as_str()))
                        .filter_map(|r| socrata::num(r, "year").map(|y| y as i32))
                        .collect();
                    years.sort();
                    years.dedup();
                    let mut yrs = Vec::new();
                    for y in &years {
                        let ma = of("MA", *y, true).into_iter().next();
                        let us = of("XX", *y, true).into_iter().next();
                        let comparison = match (&ma, &us) {
                            (Some(a), Some(b)) => {
                                compare(a, b).unwrap_or("not available").to_string()
                            }
                            _ => "not available".into(),
                        };
                        if ma.is_some() || us.is_some() {
                            yrs.push(CdcYear {
                                year: *y,
                                massachusetts: ma,
                                united_states: us,
                                comparison,
                            });
                        }
                    }
                    let latest = yrs
                        .iter()
                        .rev()
                        .find(|y| y.massachusetts.is_some())
                        .map(|y| y.year);
                    let mut groups = latest.map(|y| of("MA", y, false)).unwrap_or_default();
                    groups.retain(|g| {
                        wanted_kinds.is_empty() || wanted_kinds.contains(&group_kind(&g.group_type))
                    });
                    if !yrs.is_empty() {
                        cdc_comparison.push(CdcQuestion {
                            question: qn,
                            years: yrs,
                            massachusetts_groups: groups,
                        });
                    }
                }
            }
            Err(e) => caveats.push(format!("CDC YRBS estimates could not be retrieved: {e}")),
        }
    }

    if indicators.is_empty() && cdc_comparison.is_empty() {
        return Err(PhnError::NotFound(
            "no youth survey data match the request".into(),
        ));
    }

    // ---- summary
    let mut summary = String::from(
        "**Massachusetts youth survey data** (weighted percentages of public school students, 95% confidence intervals)\n\n",
    );
    for ind in indicators.iter().take(6) {
        summary.push_str(&format!(
            "**{}** — {} {}, {} school ({})\n\n",
            ind.indicator, ind.survey, ind.year, ind.school_level, ind.topic
        ));
        let mut rows = Vec::new();
        if let Some(o) = &ind.overall {
            rows.push(vec!["Overall".to_string(), fmt_est(o), String::new()]);
        }
        for g in &ind.groups {
            rows.push(vec![
                g.group.clone(),
                fmt_est(g),
                g.versus_reference.clone().unwrap_or_default(),
            ]);
        }
        summary.push_str(&md::table(
            &["Group", "Percent (95% CI)", "Difference"],
            &rows,
        ));
        if let Some(c) = &ind.change {
            summary.push_str(&format!("\nTrend: {c}\n"));
        }
        summary.push('\n');
    }
    if indicators.len() > 6 {
        summary.push_str(&format!(
            "Also matched: {}.\n\n",
            indicators[6..]
                .iter()
                .map(|i| format!(
                    "{} ({} {}, {})",
                    i.indicator, i.survey, i.year, i.school_level
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if matched > indicators.len() {
        summary.push_str(&format!(
            "{} more indicator(s) matched; narrow the topic or raise max_indicators.\n\n",
            matched - indicators.len()
        ));
    }
    if !cdc_comparison.is_empty() {
        summary.push_str("**Massachusetts vs the U.S. (CDC YRBS, high school)**\n\n");
        let rows: Vec<Vec<String>> = cdc_comparison
            .iter()
            .flat_map(|q| {
                q.years.iter().map(move |y| {
                    vec![
                        q.question.clone(),
                        y.year.to_string(),
                        y.massachusetts
                            .as_ref()
                            .map(fmt_est)
                            .unwrap_or_else(|| "—".into()),
                        y.united_states
                            .as_ref()
                            .map(fmt_est)
                            .unwrap_or_else(|| "—".into()),
                        y.comparison.clone(),
                    ]
                })
            })
            .collect();
        summary.push_str(&md::table(
            &["Question", "Year", "Massachusetts", "U.S.", "MA vs U.S."],
            &rows,
        ));
        summary.push('\n');
    }
    let sensitive = indicators
        .iter()
        .map(|i| i.indicator.to_lowercase())
        .chain(cdc_comparison.iter().map(|q| q.question.to_lowercase()))
        .any(|t| {
            t.contains("suicid")
                || t.contains("sad or hopeless")
                || t.contains("injur")
                || t.contains("mental health")
        });
    let crisis = sensitive.then(crisis_line);
    if let Some(c) = &crisis {
        summary.push_str(&format!("_{c}_\n"));
    }

    // ---- sources and caveats
    let prepared = CONTENT.meta.prepared.clone();
    sources.extend(content::cite(
        &CONTENT.sources,
        &["myhs_2025", "myhs_2023", "joint_2023"],
        &prepared,
    ));
    if crisis.is_some() {
        sources.extend(content::cite(&CONTENT.sources, &["ma_988"], &prepared));
    }
    let notes: Vec<String> = CONTENT
        .notes
        .iter()
        .map(|n| format!("{} ({})", n.text, n.section))
        .chain(CONTENT.surveys.iter().map(|s| {
            format!(
                "{} ({}): {}",
                s.name.clone().unwrap_or_default(),
                s.id.clone().unwrap_or_default(),
                s.text
            )
        }))
        .collect();
    caveats.push("The MYHS, the Massachusetts YRBS (MYRBS), and CDC's YRBS figures come from different surveys and questions; compare years within one survey only.".into());
    caveats.push("Differences are shown only when 95% confidence intervals do not overlap (DPH's rule); overlapping intervals can still hide a real difference, and many comparisons are made, so treat single differences with care.".into());
    if crisis.is_some() {
        caveats.push("When sharing suicide-related data publicly, follow safe-messaging guidance: do not describe methods, and include the 988 Lifeline.".into());
    }
    caveats.extend(content::review_caveats(&[YOUTH_SURVEY]));
    let mut privacy = Privacy::none();
    privacy.notes.push("Published weighted survey percentages only; estimates DPH or CDC did not present stay suppressed.".into());
    Ok(Envelope::new(
        summary,
        YouthSurveyResult {
            matched,
            indicators,
            cdc_comparison,
            crisis_line: crisis,
            notes,
            method: "Indicators are matched to the topic words (with synonyms) in DPH's MYHS 2023 and 2025 and 2023 joint MYHS/MYRBS report tables. Groups are compared with males, White students, and straight/cisgender students; a difference is reported when the 95% confidence intervals do not overlap (DPH's rule). Trends compare the same indicator in the same survey two years earlier. CDC's YRBSS mental health indicators compare Massachusetts with the national YRBS the same way.".into(),
        },
    )
    .sources(sources)
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
    fn snapshot_matches_the_published_2025_mental_health_table() {
        let (_d, ctx) = ctx();
        let (inds, ests, _) = load_dph(&ctx).unwrap();
        let sad = inds
            .iter()
            .find(|i| {
                i.year == 2025 && i.level == "high" && i.text.starts_with("Feeling sad or hopeless")
            })
            .unwrap();
        let get = |g: &str| {
            ests.iter()
                .find(|(id, e)| *id == sad.id && e.group == g)
                .map(|(_, e)| e.clone())
                .unwrap()
        };
        // MYHS 2025 report, Mental Health and Suicidality, high school.
        let o = get("Overall");
        assert_eq!(
            (o.percent, o.ci_low, o.ci_high),
            (Some(27.7), Some(24.5), Some(31.0))
        );
        let f = get("Female");
        assert_eq!(
            (f.percent, f.ci_low, f.ci_high),
            (Some(36.2), Some(31.5), Some(40.8))
        );
        let m = get("Male");
        assert_eq!(compare(&f, &m), Some("higher"));
        // "**" is kept as suppressed.
        assert!(
            ests.iter()
                .any(|(_, e)| e.suppressed && e.percent.is_none())
        );
    }

    #[test]
    fn search_terms_expand_synonyms() {
        assert!(search_terms("vaping").contains(&"vap".to_string()));
        assert!(search_terms("kids who are depressed").contains(&"sad or hopeless".to_string()));
        assert!(search_terms("self-harm").contains(&"injuring oneself".to_string()));
        assert!(
            score(
                "Being bullied on school property",
                &search_terms("bullying")
            ) > 0
        );
    }
}
