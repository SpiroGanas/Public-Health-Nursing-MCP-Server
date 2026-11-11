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
    /// Adjust for an annual seasonal cycle. Default: true for monthly and weekly data with at
    /// least two full years (24 months or 104 weeks), otherwise false. Asking for it with fewer
    /// than one and a half years of data is refused.
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

/// Month number (1–12) named by a word such as "Jan", "Sept", or "March".
fn month_from_name(word: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let w = word.to_ascii_lowercase();
    if w.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|m| m.starts_with(&w))
        .map(|i| i as u32 + 1)
}

/// Decimal year of the first period, read from its label, and whether its
/// place within the year (month, quarter, or week) could be read. Understood:
/// "2024", "2024-03", "03/2024", "Mar 2024", "2024 Q2", "2024-W05", and dates
/// such as "2024-06-03", "6/3/2024" (month first), or "June 3, 2024". When
/// the place in the year cannot be read the time is the year alone (0 without
/// a year): trends are unaffected, but the calendar position of a seasonal
/// peak is then unknown.
fn start_time(label: &str, unit: TimeUnit) -> (f64, bool) {
    let groups: Vec<u32> = label
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let Some(yi) = groups.iter().position(|d| (1900..=2100).contains(d)) else {
        return (0.0, false);
    };
    let y = f64::from(groups[yi]);
    if unit == TimeUnit::Year {
        return (y, true);
    }
    let named_month = label
        .split(|c: char| !c.is_alphabetic())
        .find_map(month_from_name);
    // Numbers written before and after the year.
    let (before, after) = (&groups[..yi], &groups[yi + 1..]);
    let day_ok = |d: u32| (1..=31).contains(&d);
    // (month, day of the month) when the label names a month or is a date.
    let month_day: Option<(u32, u32)> = match (named_month, before, after) {
        (Some(m), _, _) => Some((
            m,
            before
                .iter()
                .chain(after)
                .copied()
                .find(|d| day_ok(*d))
                .unwrap_or(1),
        )),
        (None, [], [m, d]) if (1..=12).contains(m) && day_ok(*d) => Some((*m, *d)),
        (None, [a, b], []) if (1..=12).contains(a) && day_ok(*b) => Some((*a, *b)),
        (None, [a, b], []) if (1..=12).contains(b) && day_ok(*a) => Some((*b, *a)),
        _ => None,
    };
    // A single number beside the year: the month, quarter, or week number.
    let single: Option<u32> = match (before, after) {
        ([], [v]) | ([v], []) => Some(*v),
        _ => None,
    };
    let fraction = match (unit, month_day, single) {
        (TimeUnit::Month, Some((m, _)), _) => Some(f64::from(m - 1) / 12.0),
        (TimeUnit::Month, None, Some(m)) if (1..=12).contains(&m) => Some(f64::from(m - 1) / 12.0),
        (TimeUnit::Quarter, Some((m, _)), _) => Some(f64::from((m - 1) / 3) / 4.0),
        (TimeUnit::Quarter, None, Some(q)) if (1..=4).contains(&q) => Some(f64::from(q - 1) / 4.0),
        // A week labeled with a date: that date's place in the year.
        (TimeUnit::Week, Some((m, d)), _) => {
            Some((f64::from(m - 1) + f64::from(d - 1) / 31.0) / 12.0)
        }
        (TimeUnit::Week, None, Some(w)) if (1..=53).contains(&w) => Some(f64::from(w - 1) / 52.0),
        _ => None,
    };
    match fraction {
        Some(f) => (y + f, true),
        None => (y, false),
    }
}

/// Whether to fit the annual seasonal term, and whether the default was
/// turned off because the series is short. The term defaults to on for
/// monthly and weekly data only with at least two full years: fitted to
/// less, a seasonal wave cannot be told apart from the trend and the percent
/// change per year becomes meaningless. An explicit request with fewer than
/// one and a half years is refused for the same reason.
fn seasonal_choice(requested: Option<bool>, unit: TimeUnit, n: usize) -> Result<(bool, bool)> {
    let (word, enough) = match unit {
        TimeUnit::Year => return Ok((false, false)),
        TimeUnit::Quarter => ("quarters", 6),
        TimeUnit::Month => ("months", 18),
        TimeUnit::Week => ("weeks", 78),
    };
    let years = n as f64 / unit.per_year();
    match requested {
        Some(true) if n < enough => Err(PhnError::InvalidInput(format!(
            "seasonal = true needs at least {enough} {word} (one and a half years); with {n} the \
             seasonal pattern cannot be separated from the trend. Use seasonal = false, or give \
             more periods."
        ))),
        Some(s) => Ok((s, false)),
        None => {
            let wanted = matches!(unit, TimeUnit::Month | TimeUnit::Week);
            Ok((wanted && years >= 2.0, wanted && years < 2.0))
        }
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
    let (t0, start_known) = start_time(&args.periods[0].label, unit);
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
    let (seasonal, seasonal_off_short) = seasonal_choice(args.seasonal, unit, n)?;
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
        if jp.joinpoints.is_empty() && trend::recommended_max_joinpoints(n) == 0 {
            summary.push_str(&format!(
                "- **Joinpoints:** not tested — looking for a change in trend needs at least 7 \
                 periods (you gave {n}).\n"
            ));
        } else if jp.joinpoints.is_empty() {
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
        // Period i sits at the start of its interval on the time axis, so
        // the middle of the peak period is half a period later.
        let per = unit.per_year();
        let mid = (s.peak_fraction_of_year + 0.5 / per).rem_euclid(1.0);
        let month = (mid * 12.0).floor() as usize % 12;
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
        let peak = if start_known {
            format!("around {}", months[month])
        } else {
            // The calendar position of the first period is unknown: count
            // from it instead of naming a month.
            let k = (mid * per).floor() as usize % (per as usize) + 1;
            format!(
                "about {unit_word} {k} of each {}-{unit_word} cycle, counting from the first \
                 period given",
                per as usize
            )
        };
        summary.push_str(&format!(
            "- **Seasonality:** peak {peak}; peak-to-trough ratio {:.2} ({})\n",
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
    if seasonal_off_short {
        caveats.push(format!(
            "Fewer than two full years of data ({n} {unit_word}s): the seasonal term was left out, \
             because a seasonal wave fitted to less cannot be told apart from the trend. A \
             seasonal pattern can therefore look like a trend here; treat the percent change per \
             year with caution."
        ));
    }
    if r.seasonality.is_some() && !start_known {
        caveats.push(
            "The first period label could not be read as a calendar month, quarter, or week (use \
             labels such as 2024-03, Mar 2024, 2024 Q2, 2024-W05, or a date), so the seasonal \
             peak is given as a position counted from the first period, not as a month."
                .into(),
        );
    }
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

    fn args(labels: Vec<String>, counts: &[f64], unit: TimeUnit) -> TrendArgs {
        TrendArgs {
            periods: labels
                .into_iter()
                .zip(counts)
                .map(|(label, count)| PeriodInput {
                    label,
                    count: *count,
                    population: None,
                })
                .collect(),
            time_unit: Some(unit),
            model: Some(ModelChoice::Poisson),
            seasonal: None,
            max_joinpoints: Some(0),
            interruptions: None,
            per: None,
            confidence: None,
            event_description: None,
            chart: Some(false),
        }
    }

    fn close(got: (f64, bool), want: f64) -> bool {
        got.1 && (got.0 - want).abs() < 1e-9
    }

    #[test]
    fn first_period_labels_are_read() {
        assert!(close(start_time("2019", TimeUnit::Year), 2019.0));
        assert!(close(
            start_time("2024-03", TimeUnit::Month),
            2024.0 + 2.0 / 12.0
        ));
        assert!(close(
            start_time("03/2024", TimeUnit::Month),
            2024.0 + 2.0 / 12.0
        ));
        // Month names used to be read as January.
        assert!(close(start_time("Jul 2023", TimeUnit::Month), 2023.5));
        assert!(close(
            start_time("September 2023", TimeUnit::Month),
            2023.0 + 8.0 / 12.0
        ));
        assert!(close(start_time("2024 Q2", TimeUnit::Quarter), 2024.25));
        assert!(close(start_time("2024-04-01", TimeUnit::Quarter), 2024.25));
        assert!(close(
            start_time("2024-W05", TimeUnit::Week),
            2024.0 + 4.0 / 52.0
        ));
        // A week labeled by its starting date (3 June) used to be read as week 6.
        let june3 = 2024.0 + (5.0 + 2.0 / 31.0) / 12.0;
        assert!(close(start_time("2024-06-03", TimeUnit::Week), june3));
        assert!(close(start_time("6/3/2024", TimeUnit::Week), june3));
        assert!(close(start_time("June 3, 2024", TimeUnit::Week), june3));
        // No place in the year, or no year at all: flagged as unknown.
        assert_eq!(start_time("2023", TimeUnit::Month), (2023.0, false));
        assert_eq!(start_time("Week 1", TimeUnit::Week), (0.0, false));
        assert_eq!(start_time("P1", TimeUnit::Month), (0.0, false));
    }

    #[test]
    fn seasonal_term_needs_enough_cycles() {
        let pick = |req, unit, n| seasonal_choice(req, unit, n).unwrap();
        // Default: on with two full years of monthly or weekly data.
        assert_eq!(pick(None, TimeUnit::Month, 36), (true, false));
        assert_eq!(pick(None, TimeUnit::Month, 24), (true, false));
        assert_eq!(pick(None, TimeUnit::Month, 23), (false, true));
        assert_eq!(pick(None, TimeUnit::Week, 104), (true, false));
        assert_eq!(pick(None, TimeUnit::Week, 103), (false, true));
        assert_eq!(pick(None, TimeUnit::Quarter, 40), (false, false));
        assert_eq!(pick(None, TimeUnit::Year, 30), (false, false));
        // Explicit choices.
        assert_eq!(pick(Some(false), TimeUnit::Month, 60), (false, false));
        assert_eq!(pick(Some(true), TimeUnit::Month, 18), (true, false));
        assert_eq!(pick(Some(true), TimeUnit::Year, 30), (false, false));
        let e = seasonal_choice(Some(true), TimeUnit::Month, 17)
            .unwrap_err()
            .to_string();
        assert!(e.contains("18 months"), "{e}");
        assert!(seasonal_choice(Some(true), TimeUnit::Quarter, 5).is_err());
        assert!(seasonal_choice(Some(true), TimeUnit::Week, 77).is_err());
    }

    #[test]
    fn short_monthly_series_gets_no_seasonal_term_by_default() {
        let (_dir, ctx) = ctx();
        // Eight flat months. With the seasonal term (the old default) the
        // percent change per year had a 95% interval of −91% to +988%.
        let counts = [52.0, 47.0, 55.0, 49.0, 51.0, 46.0, 53.0, 48.0];
        let labels: Vec<String> = (1..=8).map(|m| format!("2024-{m:02}")).collect();
        let r = analyze_trends(&ctx, args(labels.clone(), &counts, TimeUnit::Month)).unwrap();
        assert!(r.data.analysis.seasonality.is_none());
        assert!(
            r.data.analysis.apc.upper < 100.0,
            "{:?}",
            r.data.analysis.apc
        );
        assert!(
            r.caveats
                .iter()
                .any(|c| c.contains("seasonal term was left out")),
            "{:?}",
            r.caveats
        );
        let mut asked = args(labels, &counts, TimeUnit::Month);
        asked.seasonal = Some(true);
        let e = analyze_trends(&ctx, asked).unwrap_err().to_string();
        assert!(e.contains("18 months"), "{e}");
    }

    /// Thirty-six months starting in July 2023 with more events every
    /// December, January, and February.
    fn winter_peak(label: impl Fn(usize) -> String) -> TrendArgs {
        let counts: Vec<f64> = (0..36)
            .map(|i| match (6 + i) % 12 {
                11 | 0 | 1 => 80.0,
                _ => 50.0,
            })
            .collect();
        args((0..36).map(label).collect(), &counts, TimeUnit::Month)
    }

    #[test]
    fn seasonal_peak_is_named_from_the_labels() {
        let (_dir, ctx) = ctx();
        const NAMES: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        // "Jul 2023" used to be read as January 2023, moving the peak to July.
        let named = winter_peak(|i| format!("{} {}", NAMES[(6 + i) % 12], 2023 + (6 + i) / 12));
        let r = analyze_trends(&ctx, named).unwrap();
        assert!(r.summary.contains("peak around January"), "{}", r.summary);
        // Labels without a readable date: a position, not a month, and a caveat.
        let plain = winter_peak(|i| format!("P{}", i + 1));
        let r = analyze_trends(&ctx, plain).unwrap();
        assert!(
            r.summary.contains("about month 7 of each 12-month cycle"),
            "{}",
            r.summary
        );
        assert!(!r.summary.contains("peak around"));
        assert!(r.caveats.iter().any(|c| c.contains("could not be read")));
    }

    #[test]
    fn joinpoints_are_reported_as_not_tested_for_short_series() {
        let (_dir, ctx) = ctx();
        let labels: Vec<String> = (2018..=2023).map(|y| y.to_string()).collect();
        let mut a = args(
            labels,
            &[40.0, 44.0, 39.0, 47.0, 45.0, 50.0],
            TimeUnit::Year,
        );
        a.max_joinpoints = None;
        let r = analyze_trends(&ctx, a).unwrap();
        assert!(
            r.summary.contains("**Joinpoints:** not tested"),
            "{}",
            r.summary
        );
        assert!(!r.summary.contains("none selected"));
    }
}
