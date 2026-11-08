//! EPI-09 `analyze_trends`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::chart::{ChartKind, ChartSpec, Series as ChartSeries, VMarker};
use phn_epi::trend::{self, ModelChoice, Options, PercentChange, Series, TrendResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::charts::{ChartFiles, ChartFormat, write_chart};
use crate::context::Context;
use crate::epi::two_by_two::fmt_p;
use crate::md;

/// One time period.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PeriodInput {
    /// Period label, e.g. "2019", "2024-03", "2024 Q2", "2024-W05".
    pub label: String,
    /// Events in the period.
    pub count: f64,
    /// Population or person-time at risk (omit to analyze counts).
    #[serde(default)]
    pub population: Option<f64>,
}

/// Length of each period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    #[default]
    Year,
    Quarter,
    Month,
    Week,
}

impl TimeUnit {
    fn per_year(self) -> f64 {
        match self {
            TimeUnit::Year => 1.0,
            TimeUnit::Quarter => 4.0,
            TimeUnit::Month => 12.0,
            TimeUnit::Week => 52.0,
        }
    }
}

/// Arguments for `analyze_trends`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TrendArgs {
    /// Consecutive periods in time order (no gaps; use 0 for periods with no events).
    pub periods: Vec<PeriodInput>,
    /// year (default), quarter, month, or week.
    #[serde(default)]
    pub time_unit: Option<TimeUnit>,
    /// auto (Poisson, switching to negative binomial when counts are overdispersed; default),
    /// poisson, or negative_binomial.
    #[serde(default)]
    pub model: Option<ModelChoice>,
    /// Adjust for an annual seasonal cycle (default true for monthly and weekly data).
    #[serde(default)]
    pub seasonal: Option<bool>,
    /// Maximum joinpoints to search for (0–3, default 3, limited by the number of periods).
    #[serde(default)]
    pub max_joinpoints: Option<usize>,
    /// Interrupted time series: labels of the first period after each known change
    /// (e.g. a new policy). Disables the joinpoint search.
    #[serde(default)]
    pub interruptions: Option<Vec<String>>,
    /// Rate base (default 100,000).
    #[serde(default)]
    pub per: Option<f64>,
    /// Confidence level, 0.80–0.999 (default 0.95).
    #[serde(default)]
    pub confidence: Option<f64>,
    /// What was counted, e.g. "Lyme disease cases".
    #[serde(default)]
    pub event_description: Option<String>,
    /// Draw a chart of observed and fitted values (default true).
    #[serde(default)]
    pub chart: Option<bool>,
}

/// Result of `analyze_trends`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrendToolResult {
    pub time_unit: TimeUnit,
    pub per: f64,
    pub analysis: TrendResult,
    /// Observed and fitted rates (per `per`) or counts, by period.
    pub fitted: Vec<FittedPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<ChartFiles>,
}

/// One period's observed and fitted value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FittedPoint {
    pub label: String,
    pub observed: f64,
    pub fitted: f64,
}

/// Decimal year of the first period, read from its label when possible.
fn start_time(label: &str, unit: TimeUnit) -> f64 {
    let digits: Vec<u32> = label
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let Some(&year) = digits.iter().find(|d| (1900..=2100).contains(*d)) else {
        return 0.0;
    };
    let sub = digits
        .iter()
        .find(|d| **d != year)
        .copied()
        .unwrap_or(1)
        .max(1);
    let y = f64::from(year);
    match unit {
        TimeUnit::Year => y,
        TimeUnit::Quarter => y + (f64::from(sub.min(4)) - 1.0) / 4.0,
        TimeUnit::Month => y + (f64::from(sub.min(12)) - 1.0) / 12.0,
        TimeUnit::Week => y + (f64::from(sub.min(53)) - 1.0) / 52.0,
    }
}

fn fmt_pc(p: &PercentChange) -> String {
    format!("{:+.1}% ({:+.1}% to {:+.1}%)", p.percent, p.lower, p.upper)
}

/// Annual percent change, joinpoint, interrupted time series, and seasonality.
pub fn analyze_trends(ctx: &Context, args: TrendArgs) -> Result<Envelope<TrendToolResult>> {
    let n = args.periods.len();
    if !(3..=520).contains(&n) {
        return Err(PhnError::InvalidInput(
            "give between 3 and 520 periods".into(),
        ));
    }
    let unit = args.time_unit.unwrap_or_default();
    let per = args.per.unwrap_or(100_000.0);
    let confidence = args.confidence.unwrap_or(0.95);
    if !(0.80..=0.999).contains(&confidence) || !(per.is_finite() && per > 0.0) {
        return Err(PhnError::InvalidInput(
            "confidence must be 0.80–0.999 and per positive".into(),
        ));
    }
    let with_pop = args
        .periods
        .iter()
        .filter(|p| p.population.is_some())
        .count();
    if with_pop != 0 && with_pop != n {
        return Err(PhnError::InvalidInput(
            "give a population for every period or for none".into(),
        ));
    }
    if args
        .periods
        .iter()
        .any(|p| !(p.count.is_finite() && p.count >= 0.0))
    {
        return Err(PhnError::InvalidInput("counts must be zero or more".into()));
    }
    let t0 = start_time(&args.periods[0].label, unit);
    let time: Vec<f64> = (0..n).map(|i| t0 + i as f64 / unit.per_year()).collect();
    let series = Series {
        time: time.clone(),
        counts: args.periods.iter().map(|p| p.count).collect(),
        population: (with_pop == n).then(|| {
            args.periods
                .iter()
                .map(|p| p.population.unwrap_or(0.0))
                .collect()
        }),
    };
    let mut interruptions = Vec::new();
    for l in args.interruptions.clone().unwrap_or_default() {
        let i = args
            .periods
            .iter()
            .position(|p| p.label.trim().eq_ignore_ascii_case(l.trim()))
            .ok_or_else(|| {
                PhnError::InvalidInput(format!(
                    "interruption '{l}' is not one of the period labels"
                ))
            })?;
        interruptions.push(time[i]);
    }
    let seasonal = args
        .seasonal
        .unwrap_or(matches!(unit, TimeUnit::Month | TimeUnit::Week))
        && unit != TimeUnit::Year;
    let opts = Options {
        model: args.model.unwrap_or_default(),
        seasonal,
        max_joinpoints: if interruptions.is_empty() {
            args.max_joinpoints.unwrap_or(3).min(3)
        } else {
            0
        },
        min_segment_points: 3,
        interruptions,
        confidence,
    };
    let r = trend::analyze(&series, &opts).map_err(PhnError::InvalidInput)?;
    let scale = if series.population.is_some() {
        per
    } else {
        1.0
    };
    let observed: Vec<f64> = match &series.population {
        Some(p) => series
            .counts
            .iter()
            .zip(p)
            .map(|(c, p)| c / p * per)
            .collect(),
        None => series.counts.clone(),
    };
    let fitted: Vec<FittedPoint> = args
        .periods
        .iter()
        .zip(&observed)
        .zip(&r.fitted)
        .map(|((p, o), f)| FittedPoint {
            label: p.label.clone(),
            observed: *o,
            fitted: f * scale,
        })
        .collect();

    let what = args
        .event_description
        .clone()
        .unwrap_or_else(|| "events".into());
    let measure = if series.population.is_some() {
        format!("rate per {}", md::num(per, 0))
    } else {
        "count".into()
    };
    let pctl = format!("{:.0}%", confidence * 100.0);
    let unit_word = match unit {
        TimeUnit::Year => "year",
        TimeUnit::Quarter => "quarter",
        TimeUnit::Month => "month",
        TimeUnit::Week => "week",
    };
    let mut summary = format!(
        "**Trend in {what}** ({measure}), {} {unit_word}s from {} to {}; model: {}{}\n\n",
        n,
        args.periods[0].label,
        args.periods[n - 1].label,
        match r.model {
            phn_epi::glm::Family::NegativeBinomial => "negative binomial (counts overdispersed)",
            _ => "Poisson",
        },
        if seasonal {
            " with an annual seasonal term"
        } else {
            ""
        }
    );
    summary.push_str(&format!(
        "- **Percent change per year** (one straight log-linear trend): {} ({pctl} CI), {}\n",
        fmt_pc(&r.apc),
        fmt_p(r.apc.p_value)
    ));
    if let Some(jp) = &r.joinpoint {
        if jp.joinpoints.is_empty() {
            summary.push_str(
                "- **Joinpoints:** none selected — one straight trend fits best (BIC).\n",
            );
        } else {
            let lab = |t: f64| period_label(&args.periods, &time, t);
            summary.push_str(&format!(
                "- **Joinpoints** (BIC-selected): {}\n\n",
                jp.joinpoints
                    .iter()
                    .map(|t| lab(*t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            let rows: Vec<Vec<String>> = jp
                .segments
                .iter()
                .map(|s| {
                    vec![
                        format!("{} – {}", lab(s.start), lab(s.end)),
                        fmt_pc(&s.apc),
                        fmt_p(s.apc.p_value),
                    ]
                })
                .collect();
            summary.push_str(&md::table(
                &["Segment", &format!("APC ({pctl} CI)"), "p"],
                &rows,
            ));
            summary.push_str(&format!(
                "\n- **Average annual percent change (AAPC)** over the whole period: {} ({pctl} CI)\n",
                fmt_pc(&jp.aapc)
            ));
        }
    }
    for it in &r.interruptions {
        summary.push_str(&format!(
            "- **Change at {}:** immediate level change × {:.2} ({pctl} CI {:.2}–{:.2}; {}); \
             trend {} per year before vs {} after (slope change {})\n",
            period_label(&args.periods, &time, it.at),
            it.level_ratio,
            it.level_lower,
            it.level_upper,
            fmt_p(it.level_p),
            fmt_pc(&it.apc_before),
            fmt_pc(&it.apc_after),
            fmt_p(it.slope_change_p)
        ));
    }
    if let Some(s) = &r.seasonality {
        let month = (s.peak_fraction_of_year * 12.0).floor() as usize % 12;
        let months = [
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
        summary.push_str(&format!(
            "- **Seasonality:** peak around {}; peak-to-trough ratio {:.2} ({})\n",
            months[month],
            s.peak_to_trough,
            fmt_p(s.p_value)
        ));
    }
    summary.push_str(&format!(
        "- **Mann–Kendall** (no model assumptions): {} trend, Kendall's τ = {:.2}, {}\n",
        if r.mann_kendall.s > 0.0 {
            "upward"
        } else if r.mann_kendall.s < 0.0 {
            "downward"
        } else {
            "no"
        },
        r.mann_kendall.tau,
        fmt_p(r.mann_kendall.p_value)
    ));

    let mut chart_files = None;
    if args.chart.unwrap_or(true) {
        let mut spec = ChartSpec::new(
            ChartKind::Line,
            format!("{} — {}", capitalize(&what), measure),
            args.periods.iter().map(|p| p.label.clone()).collect(),
        );
        spec.series = vec![
            ChartSeries {
                name: "Observed".into(),
                values: observed.iter().map(|v| Some(*v)).collect(),
                ..ChartSeries::default()
            },
            ChartSeries {
                name: "Model".into(),
                values: fitted.iter().map(|f| Some(f.fitted)).collect(),
                ..ChartSeries::default()
            },
        ];
        spec.y_label = Some(capitalize(&measure));
        spec.include_zero = Some(true);
        for t in r
            .joinpoint
            .iter()
            .flat_map(|j| j.joinpoints.iter())
            .chain(r.interruptions.iter().map(|i| &i.at))
        {
            if let Some(i) = time.iter().position(|x| (x - t).abs() < 1e-9) {
                spec.markers.push(VMarker {
                    at: i as f64 + 0.5,
                    label: args.periods[i].label.clone(),
                });
            }
        }
        spec.note = Some(
            "Model: log-linear regression with the selected joinpoints or interruptions.".into(),
        );
        match write_chart(
            ctx,
            &spec,
            &format!("trend_{}", what.chars().take(40).collect::<String>()),
            ChartFormat::Both,
        ) {
            Ok((f, _)) => {
                summary.push_str(&format!("\nChart: {}\n", f.png.clone().unwrap_or_default()));
                chart_files = Some(f);
            }
            Err(e) => summary.push_str(&format!("\n(Chart not saved: {e})\n")),
        }
    }

    let mut caveats = r.warnings.clone();
    caveats.push(
        "Joinpoint locations are chosen from the observed periods by BIC (NCI's Joinpoint program \
         uses permutation tests by default), and intervals are conditional on those locations, so \
         they are somewhat too narrow. Confirm important findings with the Joinpoint program."
            .into(),
    );
    caveats.push(
        "Changes in case definitions, testing, reporting, or population estimates can create \
         artificial trends; check for them before interpreting a change."
            .into(),
    );
    if series.population.is_none() {
        caveats.push(
            "No populations were given, so trends in counts may reflect population change.".into(),
        );
    }
    let threshold = ctx.suppression_threshold();
    if series
        .counts
        .iter()
        .any(|c| *c > 0.0 && *c < f64::from(threshold))
    {
        caveats.push(format!(
            "Some periods have counts below your suppression threshold ({threshold}); suppress them in published tables."
        ));
    }
    Ok(Envelope::new(
        summary,
        TrendToolResult {
            time_unit: unit,
            per,
            analysis: r,
            fitted,
            chart: chart_files,
        },
    )
    .sources([Source::computed(
        "Log-linear Poisson/negative binomial regression with continuous joinpoints (Kim HJ et al. \
         Stat Med 2000;19:335-351), segmented regression for interrupted time series (Wagner AK et \
         al. J Clin Pharm Ther 2002;27:299-309), Mann–Kendall test with Sen's slope",
        "PHN MCP server calculation (verified against R glm, MASS::glm.nb, trend::mk.test)",
    )])
    .caveats(caveats)
    .privacy(Privacy {
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    }))
}

fn period_label(periods: &[PeriodInput], time: &[f64], t: f64) -> String {
    time.iter()
        .position(|x| (x - t).abs() < 1e-9)
        .map(|i| periods[i].label.clone())
        .unwrap_or_else(|| format!("{t:.2}"))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
