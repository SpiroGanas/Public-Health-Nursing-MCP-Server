//! CD-10 `detect_aberrations`: EARS C1–C3, CUSUM, and Farrington-type
//! aberration detection on a count series (engine: [`phn_epi::aberration`]).

use chrono::{Days, Months, NaiveDate};
use phn_core::{Envelope, PhnError, Result, Source};
use phn_epi::aberration::{
    self, CusumParams, EarsMethod, EarsParams, FarringtonParams, ThresholdMethod,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::charts::{ChartFiles, ChartFormat, write_chart};
use crate::context::Context;
use crate::md;

/// How often the counts were collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    /// Daily counts (EARS was designed for these; Farrington is not available).
    Daily,
    /// Weekly counts (default).
    #[default]
    Weekly,
    /// Monthly counts.
    Monthly,
}

/// An aberration-detection method.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// EARS C1: today versus the mean and SD of the previous 7 periods.
    EarsC1,
    /// EARS C2: as C1 with a 2-period guard band (baseline t−9 … t−3).
    EarsC2,
    /// EARS C3: sum of the C2 excess over the last 3 periods (sustained rises).
    EarsC3,
    /// Poisson-standardized CUSUM (accumulates small, persistent increases).
    Cusum,
    /// Farrington-type (Noufaily 2013 improvements): seasonal model from past years.
    Farrington,
}

impl Method {
    fn label(self) -> &'static str {
        match self {
            Method::EarsC1 => "EARS C1",
            Method::EarsC2 => "EARS C2",
            Method::EarsC3 => "EARS C3",
            Method::Cusum => "CUSUM",
            Method::Farrington => "Farrington (Noufaily)",
        }
    }
    const ALL: [Method; 5] = [
        Method::EarsC1,
        Method::EarsC2,
        Method::EarsC3,
        Method::Cusum,
        Method::Farrington,
    ];
}

/// Arguments for `detect_aberrations`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AberrationArgs {
    /// Counts per period (cases, ED visits for a syndrome), oldest first. At
    /// least 8 periods; Farrington needs at least one year of history plus 4
    /// periods (five years is best).
    pub counts: Vec<f64>,
    /// Daily, weekly (default), or monthly counts.
    #[serde(default)]
    pub frequency: Option<Frequency>,
    /// Labels for each period (same length as counts), e.g. "2026-W14".
    #[serde(default)]
    pub period_labels: Option<Vec<String>>,
    /// First day of the first period (YYYY-MM-DD), used to label periods when
    /// `period_labels` is not given.
    #[serde(default)]
    pub start_date: Option<String>,
    /// Methods to run (default: all five; Farrington is skipped when there is
    /// not enough history or the counts are daily).
    #[serde(default)]
    pub methods: Option<Vec<Method>>,
    /// What is being counted, e.g. "ILI emergency department visits".
    #[serde(default)]
    pub series_name: Option<String>,
    /// Floor for the EARS baseline standard deviation (default 0.5 when the
    /// counts are sparse, i.e. median baseline count under 2; otherwise 0).
    /// A floor stops one or two cases from signaling after a run of zeros.
    #[serde(default)]
    pub ears_min_sigma: Option<f64>,
    /// CUSUM reference value k in standard deviations (default 0.5).
    #[serde(default)]
    pub cusum_k: Option<f64>,
    /// CUSUM decision interval h (default 4). Larger h means fewer false alarms
    /// and slower detection.
    #[serde(default)]
    pub cusum_h: Option<f64>,
    /// Periods at the start of the series used to estimate the CUSUM's
    /// in-control mean (default: the first half, at most 52).
    #[serde(default)]
    pub cusum_baseline_periods: Option<usize>,
    /// Farrington one-sided significance level (default 0.05).
    #[serde(default)]
    pub farrington_alpha: Option<f64>,
    /// Number of most recent periods to show in the table (default 12, at most 104).
    /// Signals are counted over the whole series either way.
    #[serde(default)]
    pub report_last: Option<usize>,
    /// Also draw a chart (counts, thresholds, and signals) in /data/output/charts.
    #[serde(default)]
    pub chart: Option<bool>,
}

/// One evaluated period for one method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PeriodResult {
    /// 0-based position in the series.
    pub index: usize,
    pub label: String,
    pub observed: f64,
    /// Expected count: EARS baseline mean, CUSUM in-control mean, or the Farrington model's prediction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<f64>,
    /// Smallest count that would have signaled (EARS C1/C2, CUSUM) or the Farrington upper threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// The method's statistic (C1–C3, CUSUM S_t, Farrington exceedance score);
    /// absent when infinite (a flat baseline of identical counts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statistic: Option<f64>,
    pub alarm: bool,
}

/// Results for one method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MethodResult {
    pub method: Method,
    pub label: String,
    /// Settings used, in words.
    pub settings: String,
    /// True when the method could not run (see `skipped_reason`).
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
    /// Periods the method evaluated.
    pub periods_evaluated: usize,
    /// Signals over the whole evaluated series.
    pub signals: usize,
    /// Labels of the periods that signaled (at most 52, most recent last).
    pub signal_periods: Vec<String>,
    /// The most recent periods (see `report_last`).
    pub recent: Vec<PeriodResult>,
}

/// Result of `detect_aberrations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AberrationResult {
    pub series_name: String,
    pub frequency: Frequency,
    pub periods: usize,
    pub total_count: f64,
    pub methods: Vec<MethodResult>,
    /// Methods signaling in the most recent period.
    pub latest_period: String,
    pub latest_signals: Vec<Method>,
    /// Plain-language interpretation.
    pub interpretation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<ChartFiles>,
}

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

fn labels(args: &AberrationArgs, freq: Frequency, n: usize) -> Result<Vec<String>> {
    if let Some(l) = &args.period_labels {
        if l.len() != n {
            return Err(PhnError::InvalidInput(format!(
                "period_labels has {} labels but counts has {n} values",
                l.len()
            )));
        }
        return Ok(l.iter().map(|s| s.trim().to_string()).collect());
    }
    if let Some(s) = &args.start_date {
        let d0 = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| {
            PhnError::InvalidInput(format!("start_date '{s}' is not a date (YYYY-MM-DD)"))
        })?;
        return Ok((0..n)
            .map(|i| {
                let d = match freq {
                    Frequency::Daily => d0.checked_add_days(Days::new(i as u64)),
                    Frequency::Weekly => d0.checked_add_days(Days::new(7 * i as u64)),
                    Frequency::Monthly => d0.checked_add_months(Months::new(i as u32)),
                };
                match (d, freq) {
                    (Some(d), Frequency::Monthly) => d.format("%Y-%m").to_string(),
                    (Some(d), _) => d.format("%Y-%m-%d").to_string(),
                    (None, _) => format!("Period {}", i + 1),
                }
            })
            .collect());
    }
    Ok((0..n).map(|i| format!("Period {}", i + 1)).collect())
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    if s.is_empty() {
        return 0.0;
    }
    let m = s.len() / 2;
    if s.len().is_multiple_of(2) {
        (s[m - 1] + s[m]) / 2.0
    } else {
        s[m]
    }
}

struct Run {
    points: Vec<PeriodResult>,
    settings: String,
}

fn skipped(m: Method, settings: String, reason: String) -> MethodResult {
    MethodResult {
        method: m,
        label: m.label().into(),
        settings,
        skipped: true,
        skipped_reason: Some(reason),
        periods_evaluated: 0,
        signals: 0,
        signal_periods: Vec::new(),
        recent: Vec::new(),
    }
}

/// Run aberration detection on a count series.
pub fn detect_aberrations(
    ctx: &Context,
    args: AberrationArgs,
) -> Result<Envelope<AberrationResult>> {
    let counts = args.counts.clone();
    let n = counts.len();
    if n < 8 {
        return Err(PhnError::InvalidInput(format!(
            "give at least 8 periods of counts (got {n}); EARS C1 needs a 7-period baseline"
        )));
    }
    if n > 5_000 {
        return Err(PhnError::InvalidInput("at most 5,000 periods".into()));
    }
    if counts.iter().any(|c| !c.is_finite() || *c < 0.0) {
        return Err(PhnError::InvalidInput(
            "counts must be non-negative numbers".into(),
        ));
    }
    let freq = args.frequency.unwrap_or_default();
    let labels = labels(&args, freq, n)?;
    let report_last = args.report_last.unwrap_or(12).clamp(1, 104);
    let mut methods: Vec<Method> = args.methods.clone().unwrap_or_else(|| Method::ALL.to_vec());
    methods.sort();
    methods.dedup();
    if methods.is_empty() {
        return Err(PhnError::InvalidInput("choose at least one method".into()));
    }
    let series_name = args
        .series_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Counts".into());

    let sparse = median(&counts) < 2.0;
    let min_sigma = match args.ears_min_sigma {
        Some(s) if !(0.0..=100.0).contains(&s) => {
            return Err(PhnError::InvalidInput(
                "ears_min_sigma must be between 0 and 100".into(),
            ));
        }
        Some(s) => s,
        None if sparse => 0.5,
        None => 0.0,
    };
    let k = args.cusum_k.unwrap_or(0.5);
    let h = args.cusum_h.unwrap_or(4.0);
    if !(0.0..=5.0).contains(&k) || !(0.1..=20.0).contains(&h) {
        return Err(PhnError::InvalidInput(
            "cusum_k must be 0–5 and cusum_h 0.1–20".into(),
        ));
    }
    let alpha = args.farrington_alpha.unwrap_or(0.05);
    if !(0.001..=0.2).contains(&alpha) {
        return Err(PhnError::InvalidInput(
            "farrington_alpha must be between 0.001 and 0.2".into(),
        ));
    }

    let mut caveats: Vec<String> = Vec::new();
    let mut results: Vec<MethodResult> = Vec::new();
    for &m in &methods {
        let run: std::result::Result<Run, (String, String)> = match m {
            Method::EarsC1 | Method::EarsC2 | Method::EarsC3 => {
                let em = match m {
                    Method::EarsC1 => EarsMethod::C1,
                    Method::EarsC2 => EarsMethod::C2,
                    _ => EarsMethod::C3,
                };
                let p = EarsParams {
                    min_sigma,
                    ..EarsParams::classic(em)
                };
                let settings = format!(
                    "7-period baseline{}, signal when the statistic exceeds {}{}",
                    if em == EarsMethod::C1 {
                        " (t−7 … t−1)"
                    } else {
                        " after a 2-period guard band (t−9 … t−3)"
                    },
                    p.threshold,
                    if min_sigma > 0.0 {
                        format!("; baseline SD floored at {min_sigma}")
                    } else {
                        String::new()
                    }
                );
                aberration::ears(&counts, em, &p)
                    .map(|pts| Run {
                        points: pts
                            .into_iter()
                            .map(|q| PeriodResult {
                                index: q.index,
                                label: labels[q.index].clone(),
                                observed: q.observed,
                                expected: finite(q.baseline_mean),
                                threshold: q.upper_bound.and_then(finite),
                                statistic: finite(q.statistic),
                                alarm: q.alarm,
                            })
                            .collect(),
                        settings: settings.clone(),
                    })
                    .map_err(|e| (settings, e))
            }
            Method::Cusum => {
                let base = args
                    .cusum_baseline_periods
                    .unwrap_or_else(|| (n / 2).clamp(2, 52));
                let p = CusumParams {
                    k,
                    h,
                    baseline_periods: base,
                    mean: None,
                    reset: true,
                };
                let settings = format!(
                    "k = {k}, h = {h}; in-control mean from the first {base} periods; reset to 0 \
                     after each signal"
                );
                aberration::cusum(&counts, &p)
                    .map(|r| Run {
                        points: r
                            .points
                            .into_iter()
                            .map(|q| PeriodResult {
                                index: q.index,
                                label: labels[q.index].clone(),
                                observed: q.observed,
                                expected: Some(r.mean),
                                threshold: finite(q.upper_bound),
                                statistic: finite(q.cusum),
                                alarm: q.alarm,
                            })
                            .collect(),
                        settings: settings.clone(),
                    })
                    .map_err(|e| (settings, e))
            }
            Method::Farrington => {
                let mut p = match freq {
                    Frequency::Weekly => FarringtonParams::noufaily_weekly(),
                    Frequency::Monthly => FarringtonParams {
                        freq: 12,
                        w: 2,
                        no_periods: 1,
                        past_periods_not_included: 2,
                        ..FarringtonParams::noufaily_weekly()
                    },
                    Frequency::Daily => FarringtonParams::noufaily_weekly(),
                };
                p.alpha = alpha;
                p.threshold_method = ThresholdMethod::NbPlugin;
                if freq == Frequency::Daily {
                    Err((
                        String::new(),
                        "Farrington-type detection is for weekly or monthly counts; aggregate \
                         daily counts to weeks to use it"
                            .to_string(),
                    ))
                } else {
                    let years = (n.saturating_sub(p.w + 1)) / p.freq;
                    p.b = years.min(5);
                    if p.b == 0 {
                        Err((
                            String::new(),
                            format!(
                                "needs at least one year of history plus {} periods ({} periods); \
                                 the series has {n}",
                                p.w + 1,
                                p.freq + p.w + 1
                            ),
                        ))
                    } else {
                        let settings = format!(
                            "{} year(s) of history (b = {}), window ±{} periods, {} seasonal \
                             levels, outbreak down-weighting (Anscombe residuals > {}), trend \
                             kept only if significant and b ≥ 3, the most recent {} periods left \
                             out of the baseline, negative-binomial threshold at α = {}, at least \
                             {} cases in the last {} periods to signal",
                            p.b,
                            p.b,
                            p.w,
                            p.no_periods,
                            p.weights_threshold,
                            p.past_periods_not_included,
                            p.alpha,
                            p.limit_cases,
                            p.limit_periods
                        );
                        if p.b < 5 {
                            caveats.push(format!(
                                "Farrington used {} year(s) of history; the Noufaily settings \
                                 assume 5. With fewer years, seasonal estimates are less stable \
                                 and the trend term is not used when b < 3.",
                                p.b
                            ));
                        }
                        if freq == Frequency::Monthly {
                            caveats.push(
                                "Monthly Farrington settings (window ±2 months, no extra seasonal \
                                 levels) are an adaptation; Noufaily et al. evaluated weekly data."
                                    .into(),
                            );
                        }
                        aberration::farrington(&counts, &p, None)
                            .map(|pts| Run {
                                points: pts
                                    .into_iter()
                                    .map(|q| PeriodResult {
                                        index: q.index,
                                        label: labels[q.index].clone(),
                                        observed: q.observed,
                                        expected: q.expected.and_then(finite),
                                        threshold: q.upper_bound.and_then(finite),
                                        statistic: q.score.and_then(finite),
                                        alarm: q.alarm,
                                    })
                                    .collect(),
                                settings: settings.clone(),
                            })
                            .map_err(|e| (settings, e))
                    }
                }
            }
        };
        match run {
            Ok(r) => {
                let alarms: Vec<&PeriodResult> = r.points.iter().filter(|p| p.alarm).collect();
                let signal_periods: Vec<String> = alarms
                    .iter()
                    .rev()
                    .take(52)
                    .rev()
                    .map(|p| p.label.clone())
                    .collect();
                let recent: Vec<PeriodResult> = r
                    .points
                    .iter()
                    .filter(|p| p.index + report_last >= n)
                    .cloned()
                    .collect();
                results.push(MethodResult {
                    method: m,
                    label: m.label().into(),
                    settings: r.settings,
                    skipped: false,
                    skipped_reason: None,
                    periods_evaluated: r.points.len(),
                    signals: alarms.len(),
                    signal_periods,
                    recent,
                });
            }
            Err((settings, reason)) => results.push(skipped(m, settings, reason)),
        }
    }
    if results.iter().all(|r| r.skipped) {
        let why: Vec<String> = results
            .iter()
            .map(|r| {
                format!(
                    "{}: {}",
                    r.label,
                    r.skipped_reason.clone().unwrap_or_default()
                )
            })
            .collect();
        return Err(PhnError::InvalidInput(format!(
            "no method could run on this series. {}",
            why.join("; ")
        )));
    }

    // Latest period.
    let last = n - 1;
    let latest_signals: Vec<Method> = results
        .iter()
        .filter(|r| r.recent.iter().any(|p| p.index == last && p.alarm))
        .map(|r| r.method)
        .collect();

    // Markdown.
    let total: f64 = counts.iter().sum();
    let mut summary = format!(
        "**Aberration detection: {series_name}** ({} {} periods, {} in total; latest period {} = {})\n\n",
        n,
        match freq {
            Frequency::Daily => "daily",
            Frequency::Weekly => "weekly",
            Frequency::Monthly => "monthly",
        },
        md::num(total, 0),
        labels[last],
        md::num(counts[last], 0)
    );
    let ran: Vec<&MethodResult> = results.iter().filter(|r| !r.skipped).collect();
    let mut headers = vec!["Period".to_string(), "Observed".to_string()];
    headers.extend(ran.iter().map(|r| r.label.clone()));
    let first_shown = n.saturating_sub(report_last);
    let mut rows = Vec::new();
    for i in first_shown..n {
        let mut row = vec![labels[i].clone(), md::num(counts[i], 0)];
        for r in &ran {
            let cell = match r.recent.iter().find(|p| p.index == i) {
                None => "—".to_string(),
                Some(p) => {
                    let thr = p
                        .threshold
                        .map(|t| format!(" (threshold {})", md::num(t, 1)))
                        .unwrap_or_default();
                    if p.alarm {
                        format!("**SIGNAL**{thr}")
                    } else {
                        format!("no{thr}")
                    }
                }
            };
            row.push(cell);
        }
        rows.push(row);
    }
    let h: Vec<&str> = headers.iter().map(String::as_str).collect();
    summary.push_str(&md::table(&h, &rows));
    summary.push_str(
        "\n_Threshold: the smallest count that would have signaled (EARS C1/C2, CUSUM) or the \
         model's upper limit (Farrington). C3 has no single threshold._\n\n",
    );
    summary.push_str("**Signals over the whole series**\n\n");
    for r in &results {
        if r.skipped {
            summary.push_str(&format!(
                "- {}: not run — {}\n",
                r.label,
                r.skipped_reason.clone().unwrap_or_default()
            ));
        } else {
            let list = if r.signal_periods.is_empty() {
                String::new()
            } else {
                let shown: Vec<&String> = r.signal_periods.iter().rev().take(8).rev().collect();
                format!(
                    " ({}{})",
                    if r.signal_periods.len() > 8 {
                        "… "
                    } else {
                        ""
                    },
                    shown
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            summary.push_str(&format!(
                "- {}: {} signal(s) in {} periods evaluated{list}. Settings: {}.\n",
                r.label, r.signals, r.periods_evaluated, r.settings
            ));
        }
    }

    let interpretation = interpret(&latest_signals, &ran, &labels[last]);
    summary.push_str(&format!("\n**Interpretation**\n\n{interpretation}\n"));
    summary.push_str(
        "\n**How the methods differ**\n\n\
         - **C1** reacts fastest to a sudden one-period spike but its 7-period baseline absorbs a \
         gradual rise; **C2** adds a 2-period guard band so a building outbreak does not hide in \
         its own baseline; **C3** sums C2 excesses over 3 periods and catches sustained moderate \
         increases, at the cost of more alarms. EARS needs only 7–11 periods of history and \
         ignores seasonality, so false alarms are more frequent than the nominal 3-SD threshold \
         suggests, especially with sparse counts.\n\
         - **CUSUM** accumulates small excesses period after period, so it detects persistent \
         shifts that never cross a single-period threshold; it needs a stable in-control mean.\n\
         - **Farrington (Noufaily)** compares each period with the same season in past years, \
         down-weights past outbreaks, and allows a trend, so it has the fewest false alarms on \
         seasonal series — but it needs years of consistent data and reacts to the size, not the \
         shape, of a rise.\n\
         - A signal is a prompt to look (data quality, reporting delays, duplicates, a real \
         cluster), not a confirmed outbreak. No signal does not rule one out.\n",
    );

    // Chart.
    let mut chart_files = None;
    if args.chart.unwrap_or(false) {
        let mut spec = phn_docs::chart::ChartSpec::new(
            phn_docs::chart::ChartKind::Line,
            format!("{series_name}: aberration detection"),
            labels.clone(),
        );
        spec.series.push(phn_docs::chart::Series {
            name: series_name.clone(),
            values: counts.iter().map(|c| Some(*c)).collect(),
            ..Default::default()
        });
        // The threshold of the first method with thresholds, as a stepped line.
        if let Some(r) = ran.iter().find(|r| {
            matches!(
                r.method,
                Method::Farrington | Method::EarsC2 | Method::EarsC1
            )
        }) {
            let pts = recent_thresholds(&results, r.method);
            let mut v = vec![None; n];
            for p in &pts {
                v[p.0] = Some(p.1);
            }
            if v.iter().any(Option::is_some) {
                spec.ref_lines.push(phn_docs::chart::RefLine {
                    label: format!("{} threshold", r.label),
                    values: v,
                    dashed: true,
                });
            }
        }
        for r in &ran {
            for p in &r.recent {
                if p.alarm {
                    spec.flags.push(phn_docs::chart::Flag {
                        series: 0,
                        index: p.index,
                        label: r.label.clone(),
                    });
                }
            }
        }
        spec.flag_legend = Some("Signal (recent periods)".into());
        spec.y_label = Some("Count".into());
        spec.note =
            Some("Aberration detection by the PHN MCP server (phn_epi::aberration).".into());
        let (files, _) = write_chart(ctx, &spec, "aberrations", ChartFormat::Both)?;
        summary.push_str(&format!(
            "\nChart saved: {}\n",
            files.png.clone().or(files.svg.clone()).unwrap_or_default()
        ));
        chart_files = Some(files);
    }

    if sparse {
        caveats.push(format!(
            "Counts are sparse (median {} per period). EARS statistics are unstable when the \
             baseline is mostly zeros{}.",
            md::num(median(&counts), 1),
            if args.ears_min_sigma.is_none() {
                "; a baseline SD floor of 0.5 was applied (set ears_min_sigma to change it)"
            } else {
                ""
            }
        ));
    }
    caveats.push(
        "Recent periods are often incomplete because of reporting delays; a dip or a missed \
         signal in the last 1–2 periods may be an artifact."
            .into(),
    );
    caveats.push(
        "Each method tested on many periods will produce some false alarms by chance; judge \
         signals together with case details and other surveillance data."
            .into(),
    );

    let mut files_written = Vec::new();
    if let Some(f) = &chart_files {
        files_written.extend(f.png.iter().cloned());
        files_written.extend(f.svg.iter().cloned());
    }
    let mut privacy = phn_core::Privacy::none();
    privacy.files_written = files_written;
    privacy.notes.push(
        "Only the counts supplied in the request were analyzed; no local file was read.".into(),
    );

    let data = AberrationResult {
        series_name,
        frequency: freq,
        periods: n,
        total_count: total,
        methods: results,
        latest_period: labels[last].clone(),
        latest_signals,
        interpretation,
        chart: chart_files,
    };
    Ok(Envelope::new(summary, data)
        .sources(method_sources())
        .caveats(caveats)
        .privacy(privacy))
}

/// (index, threshold) for every evaluated period of `m` (only the recent
/// periods are stored, which is what the chart shows as a stepped line).
fn recent_thresholds(results: &[MethodResult], m: Method) -> Vec<(usize, f64)> {
    results
        .iter()
        .find(|r| r.method == m)
        .map(|r| {
            r.recent
                .iter()
                .filter_map(|p| p.threshold.map(|t| (p.index, t)))
                .collect()
        })
        .unwrap_or_default()
}

fn interpret(latest: &[Method], ran: &[&MethodResult], last_label: &str) -> String {
    let names = |ms: &[Method]| ms.iter().map(|m| m.label()).collect::<Vec<_>>().join(", ");
    if latest.is_empty() {
        let recent_any: Vec<&str> = ran
            .iter()
            .filter(|r| r.recent.iter().any(|p| p.alarm))
            .map(|r| r.label.as_str())
            .collect();
        if recent_any.is_empty() {
            format!(
                "No method signals in the latest period ({last_label}) or in the other periods \
                 shown. The counts are within what each method expects from the baseline."
            )
        } else {
            format!(
                "No method signals in the latest period ({last_label}), but {} signaled earlier \
                 in the periods shown. Check whether the earlier rise was investigated and \
                 whether the latest period is complete.",
                recent_any.join(", ")
            )
        }
    } else {
        let agree = latest.len();
        let of = ran.len();
        let strength = if agree >= 3 || (agree * 2 > of && of > 1) {
            "Several methods agree, which makes a real increase more likely than a chance alarm."
        } else if latest == [Method::Cusum] || latest == [Method::EarsC3] {
            "Only a method that accumulates evidence signals: this suggests a sustained moderate \
             rise rather than a single spike."
        } else if latest
            .iter()
            .all(|m| matches!(m, Method::EarsC1 | Method::EarsC2))
            && ran.iter().any(|r| r.method == Method::Farrington)
        {
            "Only short-baseline EARS methods signal; the seasonal Farrington model does not. The \
             rise may be seasonal or within the usual range for this time of year."
        } else {
            "Only some methods signal; treat this as a prompt to review the data."
        };
        format!(
            "The latest period ({last_label}) signals with {} ({agree} of {of} methods run). \
             {strength} Next steps: check data quality (duplicates, reporting delays, a new \
             reporting source), review the cases for common features (place, time, exposure), \
             and discuss with DPH epidemiology if the signal persists.",
            names(latest)
        )
    }
}

fn method_sources() -> Vec<Source> {
    vec![
        Source {
            title: "The bioterrorism preparedness and response Early Aberration Reporting System \
                    (EARS). J Urban Health 2003;80(2 Suppl 1):i89–i96 (Hutwagner et al.)"
                .into(),
            publisher: "Centers for Disease Control and Prevention".into(),
            url: Some("https://doi.org/10.1007/PL00022319".into()),
            effective_date: Some("2003".into()),
            retrieved: None,
            mode: phn_core::SourceMode::Computed,
        },
        Source {
            title: "An improved algorithm for outbreak detection in multiple surveillance \
                    systems. Stat Med 2013;32(7):1206–1222 (Noufaily et al.)"
                .into(),
            publisher: "Statistics in Medicine".into(),
            url: Some("https://doi.org/10.1002/sim.5595".into()),
            effective_date: Some("2013".into()),
            retrieved: None,
            mode: phn_core::SourceMode::Computed,
        },
        Source {
            title: "R package surveillance (earsC, algo.cusum, farringtonFlexible): reference \
                    implementation the engine is tested against (version 1.26.1)"
                .into(),
            publisher: "CRAN".into(),
            url: Some("https://cran.r-project.org/package=surveillance".into()),
            effective_date: None,
            retrieved: None,
            mode: phn_core::SourceMode::Computed,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::create_dir_all(dir.path().join("cache")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn args(counts: Vec<f64>) -> AberrationArgs {
        AberrationArgs {
            counts,
            frequency: None,
            period_labels: None,
            start_date: None,
            methods: None,
            series_name: None,
            ears_min_sigma: None,
            cusum_k: None,
            cusum_h: None,
            cusum_baseline_periods: None,
            farrington_alpha: None,
            report_last: None,
            chart: None,
        }
    }

    #[test]
    fn spike_signals_and_farrington_is_skipped_without_history() {
        let (_d, c) = ctx();
        let mut v = vec![5.0, 6.0, 4.0, 5.0, 7.0, 5.0, 6.0, 5.0, 4.0, 6.0, 5.0, 5.0];
        v.push(25.0);
        let mut a = args(v);
        a.start_date = Some("2026-01-05".into());
        let env = detect_aberrations(&c, a).unwrap();
        let d = &env.data;
        assert!(d.latest_signals.contains(&Method::EarsC1));
        assert_eq!(d.latest_period, "2026-03-30");
        let far = d
            .methods
            .iter()
            .find(|m| m.method == Method::Farrington)
            .unwrap();
        assert!(far.skipped);
        assert!(env.summary.contains("SIGNAL"));
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("null"), "no NaN/inf leaks into JSON");
    }

    #[test]
    fn farrington_runs_with_two_years_of_weekly_data() {
        let (_d, c) = ctx();
        let mut v: Vec<f64> = (0..110)
            .map(|i| 6.0 + 3.0 * ((i as f64) * std::f64::consts::TAU / 52.0).sin().round())
            .collect();
        let n = v.len();
        v[n - 1] += 20.0;
        let env = detect_aberrations(&c, args(v)).unwrap();
        let far = env
            .data
            .methods
            .iter()
            .find(|m| m.method == Method::Farrington)
            .unwrap();
        assert!(!far.skipped, "{:?}", far.skipped_reason);
        assert!(env.data.latest_signals.contains(&Method::Farrington));
        assert!(env.caveats.iter().any(|c| c.contains("2 year")));
    }

    #[test]
    fn bad_input_is_rejected() {
        let (_d, c) = ctx();
        assert!(detect_aberrations(&c, args(vec![1.0; 5])).is_err());
        assert!(detect_aberrations(&c, args(vec![-1.0; 10])).is_err());
        let mut a = args(vec![1.0; 10]);
        a.period_labels = Some(vec!["a".into()]);
        assert!(detect_aberrations(&c, a).is_err());
    }
}
