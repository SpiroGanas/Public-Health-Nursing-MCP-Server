//! CD-05 `generate_epi_curve`: an epidemic curve from a local line list or
//! inline onset counts, with an interval of about 1/4–1/3 of the incubation
//! period, optional strata, incubation markers, and a described pattern.

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::chart::{Band, ChartKind, ChartSpec, Series, VMarker};
use phn_epi::suppression::{SuppressionOptions, suppress};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::line_list::{self, ColumnOptions};
use super::outbreak::{fmt_dt, incubation};
use crate::charts::{ChartFiles, ChartFormat, write_chart};
use crate::context::Context;
use crate::md;

/// Cases by onset, given inline (aggregate counts, no identifiers).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct OnsetCount {
    /// Onset date (YYYY-MM-DD) or date and time (YYYY-MM-DD HH:MM).
    pub onset: String,
    /// Number of cases with this onset.
    pub count: u64,
    /// Optional stratum (e.g. "Confirmed", "Staff").
    #[serde(default)]
    pub stratum: Option<String>,
}

/// Arguments for `generate_epi_curve`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct EpiCurveArgs {
    /// Line list (CSV or XLSX) in the data folder. Give this or `onset_counts`.
    #[serde(default)]
    pub path: Option<String>,
    /// Worksheet name for XLSX files.
    #[serde(default)]
    pub sheet: Option<String>,
    /// Column choices and case definition for a line list (found automatically).
    #[serde(default, flatten)]
    pub columns: ColumnOptions,
    /// Inline counts of cases by onset (instead of a file).
    #[serde(default)]
    pub onset_counts: Option<Vec<OnsetCount>>,
    /// Column to stack bars by (case status, town, sex, staff/resident …); at
    /// most 8 strata are drawn, the rest become "Other".
    #[serde(default)]
    pub stratify_by: Option<String>,
    /// Suspected disease, for the incubation period and the interval.
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
    /// Interval width in hours (default: about 1/4–1/3 of the typical incubation period).
    #[serde(default)]
    pub interval_hours: Option<f64>,
    /// A known exposure (meal, event), "YYYY-MM-DD HH:MM" or a date: marked on the
    /// curve with the expected onset range.
    #[serde(default)]
    pub exposure_datetime: Option<String>,
    /// Chart title.
    #[serde(default)]
    pub title: Option<String>,
    /// png, svg, or both (default).
    #[serde(default)]
    pub format: Option<ChartFormat>,
}

/// The epidemic pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    PointSource,
    ContinuousCommonSource,
    IntermittentCommonSource,
    Propagated,
    Undetermined,
}

impl Pattern {
    fn label(self) -> &'static str {
        match self {
            Pattern::PointSource => "point source",
            Pattern::ContinuousCommonSource => "continuous common source",
            Pattern::IntermittentCommonSource => "intermittent common source",
            Pattern::Propagated => "propagated (person to person)",
            Pattern::Undetermined => "undetermined",
        }
    }
}

/// One interval of the curve (suppressed counts are absent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IntervalRow {
    /// Interval start.
    pub start: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Counts by stratum (same order as `strata`), when stratified.
    pub by_stratum: Vec<Option<u64>>,
    pub suppressed: bool,
}

/// Result of `generate_epi_curve`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EpiCurveResult {
    pub cases: u64,
    /// Cases left out because the onset was missing or unreadable.
    pub cases_without_onset: u64,
    pub interval_hours: f64,
    pub interval_reason: String,
    pub strata: Vec<String>,
    pub intervals: Vec<IntervalRow>,
    pub pattern: Pattern,
    pub pattern_reasoning: Vec<String>,
    pub uncertainty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incubation: Option<String>,
    pub chart: ChartFiles,
}

/// Years an onset or exposure date may fall in. Dates outside this range are
/// data-entry errors, and date arithmetic on extreme years would overflow.
const PLOT_YEARS: std::ops::RangeInclusive<i32> = 1900..=2100;

const CANDIDATES: [f64; 13] = [
    1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 24.0, 48.0, 72.0, 96.0, 168.0, 336.0,
];

fn choose_interval(typical_h: Option<f64>, has_times: bool) -> (f64, String) {
    let floor = if has_times { 1.0 } else { 24.0 };
    match typical_h {
        Some(t) if t > 0.0 => {
            let (lo, hi) = (t / 4.0, t / 3.0);
            let target = t * 0.25;
            let closest = |pool: &mut dyn Iterator<Item = f64>| {
                pool.min_by(|a, b| (a / target).ln().abs().total_cmp(&(b / target).ln().abs()))
            };
            let pick = closest(
                &mut CANDIDATES
                    .iter()
                    .copied()
                    .filter(|c| *c >= floor && *c >= lo && *c <= hi),
            )
            .or_else(|| closest(&mut CANDIDATES.iter().copied().filter(|c| *c >= floor)))
            .unwrap_or(floor);
            let mut why = format!(
                "{} — about 1/4 (up to 1/3) of the typical incubation period ({} hours, so {}–{} hours)",
                describe_interval(pick),
                md::num(t, 0),
                md::num(lo, 1),
                md::num(hi, 1)
            );
            if pick < lo || pick > hi {
                why.push_str("; the nearest convenient width was used");
            }
            if !has_times && t / 3.0 < 24.0 {
                why.push_str(
                    ". Onset times were not available, so 1 day is the smallest interval; add an \
                     onset time column for a finer curve",
                );
            }
            (pick, why)
        }
        _ => (
            floor,
            format!(
                "{} (no incubation period was given; give a disease or incubation hours to size \
                 the interval)",
                describe_interval(floor)
            ),
        ),
    }
}

fn describe_interval(h: f64) -> String {
    if h < 24.0 {
        format!("{} hour{}", md::num(h, 0), if h == 1.0 { "" } else { "s" })
    } else if h == 168.0 {
        "1 week".into()
    } else if h == 336.0 {
        "2 weeks".into()
    } else {
        let d = h / 24.0;
        format!("{} day{}", md::num(d, 0), if d == 1.0 { "" } else { "s" })
    }
}

fn floor_to(d: NaiveDateTime, h: f64) -> NaiveDateTime {
    let midnight = d.date().and_time(NaiveTime::MIN);
    if h < 24.0 {
        let step = (h * 60.0) as i64;
        let mins = (d - midnight).num_minutes();
        midnight + Duration::minutes(mins / step * step)
    } else if h == 168.0 || h == 336.0 {
        // MMWR-style weeks start on Sunday.
        midnight - Duration::days(i64::from(d.weekday().num_days_from_sunday()))
    } else {
        midnight
    }
}

fn label(d: NaiveDateTime, h: f64) -> String {
    if h < 24.0 {
        d.format("%m-%d %H:%M").to_string()
    } else if h >= 168.0 {
        format!("Wk {}", d.format("%m-%d"))
    } else {
        d.format("%m-%d").to_string()
    }
}

fn classify(
    counts: &[u64],
    interval_h: f64,
    inc: Option<(f64, Option<f64>, f64)>,
) -> (Pattern, Vec<String>, String) {
    let n: u64 = counts.iter().sum();
    let mut why = Vec::new();
    let first = counts.iter().position(|c| *c > 0).unwrap_or(0);
    let last = counts.iter().rposition(|c| *c > 0).unwrap_or(0);
    let span_h = (last - first + 1) as f64 * interval_h;
    let max = *counts.iter().max().unwrap_or(&0);
    // Peaks: local maxima of at least half the highest count, separated by a trough.
    let mut peaks: Vec<usize> = Vec::new();
    for i in first..=last {
        let c = counts[i];
        if (c as f64) < 0.5 * max as f64 || c == 0 {
            continue;
        }
        let left = if i == 0 { 0 } else { counts[i - 1] };
        let right = counts.get(i + 1).copied().unwrap_or(0);
        if c >= left && c >= right {
            if let Some(&p) = peaks.last() {
                let trough = counts[p..=i].iter().min().copied().unwrap_or(0);
                if (trough as f64) >= 0.5 * (counts[p].min(c) as f64) {
                    // Same peak (a plateau or shoulder).
                    if c > counts[p] {
                        peaks.pop();
                        peaks.push(i);
                    }
                    continue;
                }
            }
            peaks.push(i);
        }
    }
    let busy = counts[first..=last]
        .iter()
        .filter(|c| (**c as f64) >= 0.5 * max as f64)
        .count() as f64
        / (last - first + 1) as f64;
    why.push(format!(
        "{n} cases over {} ({} intervals); {} peak(s); {:.0}% of intervals are at least half the \
         highest count.",
        describe_interval(span_h),
        last - first + 1,
        peaks.len(),
        100.0 * busy
    ));
    let uncertainty = if n < 10 {
        "Low: with fewer than 10 cases the shape of the curve is not reliable.".to_string()
    } else if inc.is_none() {
        "Moderate to low: without an incubation period the spread of onsets cannot be compared \
         with what a single exposure would produce."
            .to_string()
    } else if n < 30 {
        "Moderate: small numbers make the shape sensitive to a few cases.".to_string()
    } else {
        "Moderate: curve shape suggests, but does not prove, a mode of spread; combine it with \
         the exposure history."
            .to_string()
    };
    if n < 5 {
        why.push("Too few cases to describe a pattern.".into());
        return (Pattern::Undetermined, why, uncertainty);
    }
    let Some((min_h, typ_h, max_h)) = inc else {
        let p = if peaks.len() >= 2 {
            Pattern::IntermittentCommonSource
        } else if busy > 0.5 {
            Pattern::ContinuousCommonSource
        } else {
            Pattern::Undetermined
        };
        why.push(
            "No incubation period was given, so the pattern is judged from shape only.".into(),
        );
        return (p, why, uncertainty);
    };
    let range_h = (max_h - min_h).max(interval_h);
    if peaks.len() >= 2 {
        let gaps: Vec<f64> = peaks
            .windows(2)
            .map(|w| (w[1] - w[0]) as f64 * interval_h)
            .collect();
        let mean_gap = gaps.iter().sum::<f64>() / gaps.len() as f64;
        let typ = typ_h.unwrap_or((min_h + max_h) / 2.0);
        why.push(format!(
            "Peaks are about {} apart; the typical incubation period is about {}.",
            describe_interval(mean_gap),
            describe_interval(typ)
        ));
        if mean_gap >= 0.5 * typ && mean_gap <= 1.6 * typ {
            why.push(
                "Successive peaks about one incubation period apart suggest generations of \
                 person-to-person spread."
                    .into(),
            );
            return (Pattern::Propagated, why, uncertainty);
        }
        why.push(
            "Irregular peaks not spaced by the incubation period suggest repeated exposures to a \
             common source."
                .into(),
        );
        return (Pattern::IntermittentCommonSource, why, uncertainty);
    }
    if span_h <= 1.5 * range_h + interval_h {
        why.push(format!(
            "One peak, and the onsets span {} — within about one incubation range ({}–{} hours): \
             consistent with a single exposure.",
            describe_interval(span_h),
            md::num(min_h, 0),
            md::num(max_h, 0)
        ));
        (Pattern::PointSource, why, uncertainty)
    } else {
        why.push(format!(
            "Onsets span {}, longer than one incubation range ({} hours), without separate peaks: \
             consistent with an exposure that continued over time.",
            describe_interval(span_h),
            md::num(range_h, 0)
        ));
        (Pattern::ContinuousCommonSource, why, uncertainty)
    }
}

/// Draw an epidemic curve.
pub fn generate_epi_curve(ctx: &Context, args: EpiCurveArgs) -> Result<Envelope<EpiCurveResult>> {
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
    if known.is_some_and(|k| !PLOT_YEARS.contains(&k.year())) {
        return Err(PhnError::InvalidInput(
            "exposure_datetime must be between the years 1900 and 2100".into(),
        ));
    }

    // Collect (onset, stratum, count).
    let mut points: Vec<(NaiveDateTime, String, u64)> = Vec::new();
    let mut has_times = false;
    let mut without_onset = 0u64;
    let mut sources: Vec<Source> = Vec::new();
    let mut identifiers = Vec::new();
    let from_file = args.path.as_deref().is_some_and(|p| !p.trim().is_empty());
    match (&args.onset_counts, from_file) {
        (Some(_), true) => {
            return Err(PhnError::InvalidInput(
                "give either path (a line list) or onset_counts, not both".into(),
            ));
        }
        (None, false) => {
            return Err(PhnError::InvalidInput(
                "give path (a line list in the data folder) or onset_counts".into(),
            ));
        }
        (Some(list), false) => {
            if list.is_empty() || list.len() > 5_000 {
                return Err(PhnError::InvalidInput(
                    "onset_counts needs 1 to 5,000 entries".into(),
                ));
            }
            for oc in list {
                let (d, timed) = line_list::parse_datetime(&oc.onset).ok_or_else(|| {
                    PhnError::InvalidInput(format!("onset '{}' is not a date", oc.onset))
                })?;
                if !PLOT_YEARS.contains(&d.year()) {
                    return Err(PhnError::InvalidInput(
                        "onset dates must be between the years 1900 and 2100".into(),
                    ));
                }
                has_times |= timed;
                points.push((
                    d,
                    oc.stratum.clone().unwrap_or_else(|| "Cases".into()),
                    oc.count,
                ));
            }
            sources.push(Source::computed(
                "Onset counts supplied in the request",
                "Request",
            ));
        }
        (None, true) => {
            let ll = line_list::load(
                ctx,
                args.path.as_deref().unwrap_or_default(),
                args.sheet.as_deref(),
                &args.columns,
            )?;
            if ll.map.onset_date.is_none() {
                return Err(PhnError::InvalidInput(format!(
                    "no onset date column was found in '{}'; name it with onset_date_column",
                    ll.table.label
                )));
            }
            let strat: Option<Vec<Option<String>>> = match &args.stratify_by {
                Some(c) => {
                    let i = ll.table.column(c)?;
                    Some(ll.table.categorical(i)?)
                }
                None => None,
            };
            for (r, rec) in ll.recs.iter().enumerate() {
                if rec.case != Some(true) {
                    continue;
                }
                match rec.onset {
                    // A date far outside the plausible range is a data-entry
                    // error; it is counted with the unreadable onsets.
                    Some(d) if PLOT_YEARS.contains(&d.year()) => {
                        let s = strat
                            .as_ref()
                            .map(|v| v[r].clone().unwrap_or_else(|| "Unknown".into()))
                            .unwrap_or_else(|| "Cases".into());
                        points.push((d, s, 1));
                    }
                    _ => without_onset += 1,
                }
            }
            has_times = ll.has_times;
            sources.push(ll.table.source());
            identifiers = ll.table.identifier_categories();
        }
    }
    let total: u64 = points.iter().map(|p| p.2).sum();
    if total == 0 {
        return Err(PhnError::InvalidInput(
            "there are no cases with an onset date to plot".into(),
        ));
    }

    // Interval.
    let (interval_h, interval_reason) = match args.interval_hours {
        Some(h) if !(0.5..=24.0 * 60.0).contains(&h) => {
            return Err(PhnError::InvalidInput(
                "interval_hours must be between 0.5 and 1440".into(),
            ));
        }
        Some(h) if !has_times && h < 24.0 => {
            return Err(PhnError::InvalidInput(
                "the onsets have dates but no times, so the interval must be at least 24 hours"
                    .into(),
            ));
        }
        Some(h) => (h, format!("{} (given)", describe_interval(h))),
        None => choose_interval(inc.as_ref().and_then(|i| i.typical_h), has_times),
    };
    let step = Duration::minutes((interval_h * 60.0).round() as i64);
    let first = points.iter().map(|p| p.0).min().expect("non-empty");
    let last = points.iter().map(|p| p.0).max().expect("non-empty");
    let mut start = floor_to(first, interval_h) - step;
    let mut end = last;
    if let Some(k) = known {
        start = start.min(floor_to(k, interval_h));
        if let Some(i) = &inc {
            let band_end = k + Duration::minutes((i.max_h * 60.0) as i64);
            end = end.max(band_end.min(last + step * 4));
        }
    }
    let nbins = ((end - start).num_minutes() / step.num_minutes() + 2).max(1) as usize;
    if nbins > phn_docs::chart::MAX_CATEGORIES {
        return Err(PhnError::InvalidInput(format!(
            "{nbins} intervals is too many for one chart; use a wider interval_hours"
        )));
    }

    // Strata: most frequent first, at most 8 (the rest become "Other").
    let mut strata_tot: std::collections::BTreeMap<String, u64> = Default::default();
    for p in &points {
        *strata_tot.entry(p.1.clone()).or_default() += p.2;
    }
    let mut strata: Vec<(String, u64)> = strata_tot.into_iter().collect();
    strata.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut names: Vec<String> = strata.iter().map(|s| s.0.clone()).collect();
    if names.len() > 8 {
        names.truncate(7);
        names.push("Other".into());
    }
    let col_of = |s: &str| names.iter().position(|n| n == s).unwrap_or(names.len() - 1);
    let mut grid = vec![vec![0u64; names.len()]; nbins];
    for p in &points {
        let b = ((p.0 - start).num_minutes() / step.num_minutes()) as usize;
        grid[b.min(nbins - 1)][col_of(&p.1)] += p.2;
    }
    let totals: Vec<u64> = grid.iter().map(|r| r.iter().sum()).collect();
    let bin_start = |i: usize| start + step * i as i32;
    let categories: Vec<String> = (0..nbins)
        .map(|i| label(bin_start(i), interval_h))
        .collect();

    // Chart.
    let title = args.title.clone().unwrap_or_else(|| {
        format!(
            "Cases by onset{}",
            args.disease
                .as_deref()
                .map(|d| format!(": {d}"))
                .unwrap_or_default()
        )
    });
    let mut spec = ChartSpec::new(ChartKind::Histogram, title, categories.clone());
    for (j, n) in names.iter().enumerate() {
        spec.series.push(Series {
            name: n.clone(),
            values: grid.iter().map(|r| Some(r[j] as f64)).collect(),
            ..Default::default()
        });
    }
    spec.x_label = Some(format!(
        "Onset ({} intervals)",
        describe_interval(interval_h)
    ));
    spec.y_label = Some("Cases".into());
    spec.decimals = Some(0);
    let pos = |d: NaiveDateTime| (d - start).num_minutes() as f64 / step.num_minutes() as f64;
    if let Some(k) = known {
        spec.markers.push(VMarker {
            at: pos(k),
            label: "Exposure".into(),
        });
        if let Some(i) = &inc {
            let a = k + Duration::minutes((i.min_h * 60.0) as i64);
            let b = k + Duration::minutes((i.max_h * 60.0) as i64);
            spec.bands.push(Band {
                from: pos(a).max(0.0),
                to: pos(b).min(nbins as f64),
                label: "Expected onsets (incubation range)".into(),
            });
            if let Some(tp) = i.typical_h {
                spec.markers.push(VMarker {
                    at: pos(k + Duration::minutes((tp * 60.0) as i64)),
                    label: "Typical incubation".into(),
                });
            }
        }
    }
    spec.note = Some(format!(
        "Interval: {}. Source: {}.",
        describe_interval(interval_h),
        if from_file {
            "local line list"
        } else {
            "counts supplied"
        }
    ));
    let (mut files, _rendered) =
        write_chart(ctx, &spec, "epi_curve", args.format.unwrap_or_default())?;

    // Suppressed table (only intervals from the first to the last case).
    let first_bin = totals.iter().position(|c| *c > 0).unwrap_or(0);
    let last_bin = totals.iter().rposition(|c| *c > 0).unwrap_or(0);
    let cells: Vec<Vec<Option<u64>>> = (first_bin..=last_bin)
        .map(|i| {
            if names.len() > 1 {
                grid[i].iter().map(|c| Some(*c)).collect()
            } else {
                vec![Some(totals[i])]
            }
        })
        .collect();
    let sup = suppress(
        &cells,
        &SuppressionOptions {
            threshold: t,
            ..SuppressionOptions::default()
        },
    );
    let mut any_sup = sup.primary + sup.complementary > 0;
    let intervals: Vec<IntervalRow> = (first_bin..=last_bin)
        .enumerate()
        .map(|(r, i)| {
            let by: Vec<Option<u64>> = if names.len() > 1 {
                (0..names.len())
                    .map(|j| (!sup.is_hidden(r, j)).then_some(grid[i][j]))
                    .collect()
            } else {
                Vec::new()
            };
            let row_hidden = (0..cells[r].len()).any(|j| sup.is_hidden(r, j));
            let tot_hidden = if names.len() > 1 {
                hidden_total(totals[i], t) || by.iter().filter(|x| x.is_none()).count() == 1
            } else {
                row_hidden
            };
            IntervalRow {
                start: fmt_dt(bin_start(i), has_times),
                total: (!tot_hidden).then_some(totals[i]),
                by_stratum: by,
                suppressed: row_hidden || tot_hidden,
            }
        })
        .collect();
    any_sup |= intervals.iter().any(|r| r.suppressed);
    let max_total = totals.iter().max().copied().unwrap_or(0);
    if max_total > 0 && max_total < u64::from(t) {
        // The generated alt text names the highest count; keep small counts out of the reply.
        files.alt_text = format!(
            "Epidemic curve of {total} cases by onset in {} intervals from {} to {}.",
            describe_interval(interval_h),
            categories[first_bin],
            categories[last_bin]
        );
        let target = files.png.clone().or(files.svg.clone()).unwrap_or_default();
        files.embed_markdown = format!("![{}]({target})", files.alt_text);
    }

    let inc_tuple = inc.as_ref().map(|i| (i.min_h, i.typical_h, i.max_h));
    let (pattern, reasoning, uncertainty) = classify(&totals, interval_h, inc_tuple);

    // ---- Markdown
    let mut s = format!(
        "**Epidemic curve** — {total} cases; interval {}.\n\nPattern: **{}**. {}\n\nUncertainty: {uncertainty}\n\n",
        interval_reason,
        pattern.label(),
        reasoning.join(" ")
    );
    s.push_str(&format!(
        "Chart saved: {} (embed with export_document: `{}`)\n\n",
        files.png.clone().or(files.svg.clone()).unwrap_or_default(),
        files.embed_markdown
    ));
    if let Some(i) = &inc {
        s.push_str(&format!("Incubation used: {}.\n\n", i.label));
    }
    let mut headers = vec!["Interval start".to_string(), "Cases".to_string()];
    if names.len() > 1 {
        headers.extend(names.iter().cloned());
    }
    let rows: Vec<Vec<String>> = intervals
        .iter()
        .map(|r| {
            let mut v = vec![
                r.start.clone(),
                r.total.map(|x| x.to_string()).unwrap_or_else(|| "*".into()),
            ];
            v.extend(
                r.by_stratum
                    .iter()
                    .map(|x| x.map(|x| x.to_string()).unwrap_or_else(|| "*".into())),
            );
            v
        })
        .collect();
    let h: Vec<&str> = headers.iter().map(String::as_str).collect();
    if rows.len() <= 60 {
        s.push_str(&md::table(&h, &rows));
    } else {
        s.push_str(&md::table(&h, &rows[..60]));
        s.push_str(&format!(
            "\n_{} more intervals in the structured data._\n",
            rows.len() - 60
        ));
    }

    let mut caveats = Vec::new();
    if any_sup {
        caveats.push(format!(
            "Small-cell suppression: * marks counts from 1 to {} (and cells hidden so they \
             cannot be recalculated) in the table above. The chart saved in your output folder \
             shows exact counts; review it before sharing outside your agency.",
            t - 1
        ));
    }
    if without_onset > 0 {
        caveats.push(format!(
            "{without_onset} case(s) without a readable onset date are not on the curve."
        ));
    }
    caveats.push(
        "The most recent intervals are often incomplete (cases not yet ill, tested, or \
         reported); the curve may look like it is declining when it is not."
            .into(),
    );
    if args.disease.is_some() && inc.is_some() {
        caveats.push(super::engine::pending_caveat(
            phn_content::disease_rules::content(),
        ));
    }
    if let Some(src) = inc.as_ref().and_then(|i| i.source.clone()) {
        sources.push(src);
    }
    sources.extend(epi_curve_sources());
    let mut files_written = Vec::new();
    files_written.extend(files.png.iter().cloned());
    files_written.extend(files.svg.iter().cloned());
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: any_sup,
        suppression_threshold: Some(t),
        identifiers_detected: identifiers,
        files_written,
        notes: vec![
            "Only counts per interval are returned; the chart file keeps exact counts.".into(),
        ],
    };
    let data = EpiCurveResult {
        cases: total,
        cases_without_onset: without_onset,
        interval_hours: interval_h,
        interval_reason,
        strata: if names.len() > 1 { names } else { Vec::new() },
        intervals,
        pattern,
        pattern_reasoning: reasoning,
        uncertainty,
        incubation: inc.as_ref().map(|i| i.label.clone()),
        chart: files,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

/// Method sources for epidemic curves (retrieved 2026-11-08).
pub(crate) fn epi_curve_sources() -> Vec<Source> {
    vec![
        Source {
            title: "Quick-Learn: Create an Epi Curve (\"the unit of time is set at roughly 1/4 of \
                    the average incubation period\")"
                .into(),
            publisher: "Centers for Disease Control and Prevention".into(),
            url: Some("https://www.cdc.gov/training/quicklearns/createepi/".into()),
            effective_date: None,
            retrieved: Some("2026-11-08".into()),
            mode: phn_core::SourceMode::Computed,
        },
        Source {
            title: "The CDC Field Epidemiology Manual, Chapter 6: Describing Epidemiologic Data \
                    (point source: onsets within about 1.5 times the incubation range; secondary \
                    waves one incubation period later)"
                .into(),
            publisher: "Centers for Disease Control and Prevention".into(),
            url: Some(
                "https://www.cdc.gov/field-epi-manual/php/chapters/describing-epi-data.html".into(),
            ),
            effective_date: Some("2024-08-08".into()),
            retrieved: Some("2026-11-07".into()),
            mode: phn_core::SourceMode::Computed,
        },
    ]
}

fn hidden_total(x: u64, t: u32) -> bool {
    x > 0 && x < u64::from(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_follows_incubation() {
        // Salmonella: typical 24 h → 6 or 8 hours.
        let (h, _) = choose_interval(Some(24.0), true);
        assert!(h == 6.0 || h == 8.0, "{h}");
        // Hepatitis A: typical ~28 days → 7 days.
        let (h, _) = choose_interval(Some(28.0 * 24.0), false);
        assert_eq!(h, 168.0);
        // No times: at least a day.
        let (h, why) = choose_interval(Some(24.0), false);
        assert_eq!(h, 24.0);
        assert!(why.contains("Onset times were not available"));
    }

    #[test]
    fn classifies_shapes() {
        let point = [0, 2, 8, 12, 6, 3, 1, 0];
        assert_eq!(
            classify(&point, 6.0, Some((6.0, Some(24.0), 72.0))).0,
            Pattern::PointSource
        );
        let prop = [0, 3, 6, 2, 5, 9, 4, 7, 12, 5, 0];
        assert_eq!(
            classify(&prop, 24.0, Some((24.0, Some(72.0), 120.0))).0,
            Pattern::Propagated
        );
        let cont = [0, 4, 5, 4, 6, 5, 4, 5, 6, 4, 5, 0];
        assert_eq!(
            classify(&cont, 24.0, Some((12.0, Some(24.0), 48.0))).0,
            Pattern::ContinuousCommonSource
        );
        assert_eq!(classify(&[0, 1, 1, 0], 24.0, None).0, Pattern::Undetermined);
    }

    #[test]
    fn dates_far_outside_the_plausible_range_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let counts = |onset: &str| {
            Some(vec![OnsetCount {
                onset: onset.into(),
                count: 6,
                stratum: None,
            }])
        };
        for onset in ["+262142-12-31", "2500-01-01", "1700-06-01"] {
            let r = generate_epi_curve(
                &c,
                EpiCurveArgs {
                    onset_counts: counts(onset),
                    ..EpiCurveArgs::default()
                },
            );
            assert!(r.is_err(), "{onset}");
        }
        let r = generate_epi_curve(
            &c,
            EpiCurveArgs {
                onset_counts: counts("2026-05-03"),
                exposure_datetime: Some("9999-01-01".into()),
                ..EpiCurveArgs::default()
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn inline_counts_need_a_data_folder_for_the_chart() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let counts: Vec<OnsetCount> = [("2026-05-03", 4), ("2026-05-04", 9), ("2026-05-05", 2)]
            .iter()
            .map(|(d, n)| OnsetCount {
                onset: (*d).into(),
                count: *n,
                stratum: None,
            })
            .collect();
        let env = generate_epi_curve(
            &c,
            EpiCurveArgs {
                onset_counts: Some(counts),
                disease: Some("norovirus".into()),
                ..EpiCurveArgs::default()
            },
        )
        .unwrap();
        assert_eq!(env.data.cases, 15);
        assert!(env.data.chart.png.is_some());
        assert!(env.data.intervals.iter().any(|r| r.total.is_none()));
        assert!(env.privacy.suppression_applied);
    }
}
