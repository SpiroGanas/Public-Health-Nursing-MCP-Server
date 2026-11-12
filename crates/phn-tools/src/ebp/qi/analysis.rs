//! Run charts and control charts from the nurse's measure data, for `plan_quality_improvement`.
//!
//! Data come inline, from an event file (one row per event, grouped by period), or from a
//! period file (one row per period, such as the tracker this tool writes). Local data are
//! aggregated by period and small counts are hidden in the response; with any period hidden the
//! center line and limits are withheld too, because together with the other values they would
//! reveal the hidden number (the same rule as `create_spc_chart`). The chart file, written to
//! the output folder, has every value.

use phn_core::{PhnError, Result, Source};
use phn_docs::chart::{ChartKind, ChartSpec, Flag, RefLine, Series, VMarker};
use phn_epi::spc::{self, ChartType, Input, Rule};

use super::content::QiContent;
use super::rules::{self, Perla, SHIFT_LEN, TREND_LEN};
use super::types::{AnalysisOut, AnalysisRow, Direction, Finding, PhaseOut, QiPlanArgs, RuleSet};
use crate::charts::{ChartFormat, write_chart};
use crate::context::Context;
use crate::ebp::spc::{FileMeasure, Prepared, SpcArgs, prepare_file};
use crate::localdata::{self, LocalTable};
use crate::md;

/// Series ready to analyze.
pub(super) struct Prep {
    pub labels: Vec<String>,
    pub values: Vec<f64>,
    pub denominators: Option<Vec<f64>>,
    /// Hide this period's value in the response (small numbers from local data).
    pub hide: Vec<bool>,
    pub local: Option<LocalTable>,
    pub from: String,
}

/// True when the request carries measure data.
pub(super) fn has_data(a: &QiPlanArgs) -> bool {
    a.values.is_some() || a.path.is_some()
}

fn arg<'a>(v: &'a Option<String>, name: &str) -> Result<&'a str> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| PhnError::InvalidInput(format!("give {name}")))
}

/// Load and aggregate the measure data.
pub(super) fn prepare(ctx: &Context, a: &QiPlanArgs) -> Result<Prep> {
    if a.values.is_some() == a.path.is_some() {
        return Err(PhnError::InvalidInput(
            "for the analysis give either inline values (with labels) or a local file (path)"
                .into(),
        ));
    }
    if let Some(values) = &a.values {
        let labels = a
            .labels
            .clone()
            .unwrap_or_else(|| (1..=values.len()).map(|i| i.to_string()).collect());
        if labels.len() != values.len() {
            return Err(PhnError::InvalidInput("give one label per value".into()));
        }
        if let Some(d) = &a.denominators
            && d.len() != values.len()
        {
            return Err(PhnError::InvalidInput(
                "give one denominator per value".into(),
            ));
        }
        return Ok(Prep {
            hide: vec![false; values.len()],
            labels,
            values: values.clone(),
            denominators: a.denominators.clone(),
            local: None,
            from: "values typed into the request".into(),
        });
    }
    let threshold = ctx.suppression_threshold();
    let path = arg(&a.path, "path")?;
    if a.period_column.is_some() {
        return prepare_period_file(ctx, a, path, threshold);
    }
    let measure = a.measure_kind.unwrap_or(FileMeasure::Count);
    let sa = SpcArgs {
        path: Some(path.to_string()),
        sheet: a.sheet.clone(),
        date_column: a.date_column.clone(),
        period: a.period,
        measure: Some(measure),
        flag_column: a.flag_column.clone(),
        value_column: a.value_column.clone(),
        ..SpcArgs::default()
    };
    let Prepared {
        labels,
        values,
        denominators,
        hide,
        local,
    } = prepare_file(ctx, &sa, measure, threshold)?;
    let from = local
        .as_ref()
        .map(|l| format!("local file {} (events grouped by period)", l.label))
        .unwrap_or_default();
    Ok(Prep {
        labels,
        values,
        denominators,
        hide,
        local,
        from,
    })
}

fn prepare_period_file(ctx: &Context, a: &QiPlanArgs, path: &str, threshold: u32) -> Result<Prep> {
    let lt = localdata::load(ctx, path, a.sheet.as_deref())?;
    let pi = lt.column(arg(&a.period_column, "period_column")?)?;
    let ni = lt.column(arg(
        &a.numerator_column,
        "numerator_column (the count or value)",
    )?)?;
    let n = lt.table.rows.len();
    if !(2..=600).contains(&n) {
        return Err(PhnError::InvalidInput(
            "a chart needs 2 to 600 periods (one per row)".into(),
        ));
    }
    let (nums, share) = lt.numeric(ni);
    if share < 0.999 || nums.iter().any(Option::is_none) {
        return Err(PhnError::InvalidInput(format!(
            "every row of '{}' needs a number in the numerator column",
            lt.label
        )));
    }
    let values: Vec<f64> = nums.into_iter().flatten().collect();
    let dens = match a.denominator_column.as_deref().map(str::trim) {
        Some(d) if !d.is_empty() => {
            let di = lt.column(d)?;
            let (ds, share) = lt.numeric(di);
            if share < 0.999 || ds.iter().any(Option::is_none) {
                return Err(PhnError::InvalidInput(format!(
                    "every row of '{}' needs a number in the denominator column",
                    lt.label
                )));
            }
            Some(ds.into_iter().flatten().collect::<Vec<f64>>())
        }
        _ => None,
    };
    let mut labels = Vec::with_capacity(n);
    for cell in lt.table.column(pi) {
        let t = cell.trim();
        if t.is_empty() || t.chars().count() > 30 {
            return Err(PhnError::InvalidInput(
                "each row needs a short period label (up to 30 characters) in the period column"
                    .into(),
            ));
        }
        // Dates and towns are fine as labels; anything else that looks like an identifier is not.
        if phn_local::phi::scan_text(t)
            .iter()
            .any(|f| !matches!(f.kind, phn_local::phi::Identifier::Date))
        {
            return Err(PhnError::Privacy(
                "values in the period column look like identifiers, so they cannot be used as \
                 period labels"
                    .into(),
            ));
        }
        labels.push(t.to_string());
    }
    let t = f64::from(threshold);
    let hide: Vec<bool> = match &dens {
        Some(d) => values
            .iter()
            .zip(d)
            .map(|(x, den)| (*x > 0.0 && *x < t) || (den - x > 0.0 && den - x < t))
            .collect(),
        None if a.measure_kind == Some(FileMeasure::Mean) => vec![false; n],
        None => values.iter().map(|x| *x > 0.0 && *x < t).collect(),
    };
    let from = format!("local file {} (one row per period)", lt.label);
    Ok(Prep {
        labels,
        values,
        denominators: dens,
        hide,
        local: Some(lt),
        from,
    })
}

/// What the analysis hands back to the orchestrator.
pub(super) struct Analysis {
    pub out: AnalysisOut,
    /// PNG of the chart for the DOCX plan (full values; local output only).
    pub png: Option<Vec<u8>>,
    pub png_path: Option<String>,
    pub files: Vec<String>,
    pub suppression_applied: bool,
    pub identifier_categories: Vec<String>,
    pub local_source: Option<Source>,
    pub caveats: Vec<String>,
    /// The series, for the tracker workbook.
    pub labels: Vec<String>,
    pub numerators: Vec<f64>,
    pub denominators: Option<Vec<f64>>,
    pub baseline_points: Option<usize>,
    /// Some period was hidden in the response.
    pub any_hidden: bool,
}

fn tag(rule: Rule) -> &'static str {
    match rule {
        Rule::OutsideLimits => "outside limits",
        Rule::Shift => "8 on one side",
        Rule::Trend => "trend of 6",
        Rule::TwoOfThreeOuterThird => "2 of 3 beyond 2 sigma",
        Rule::FifteenInnerThird => "15 within 1 sigma",
        Rule::LongRun => "long run",
        Rule::TooFewCrossings => "too few crossings",
    }
}

fn span_label(labels: &[String], first: usize, last: usize) -> String {
    if first == last {
        labels[first].clone()
    } else {
        format!("{} to {}", labels[first], labels[last])
    }
}

/// Run the analysis.
pub(super) fn analyze(
    ctx: &Context,
    a: &QiPlanArgs,
    prep: Prep,
    direction: Option<Direction>,
    content: &QiContent,
) -> Result<Analysis> {
    let n = prep.values.len();
    if !(4..=600).contains(&n) {
        return Err(PhnError::InvalidInput(
            "a chart needs 4 to 600 periods".into(),
        ));
    }
    let chart_type = a.chart_type.unwrap_or(ChartType::Run);
    let ratio_data = prep.denominators.is_some();
    match chart_type {
        ChartType::U if !ratio_data => {
            return Err(PhnError::InvalidInput(
                "a u chart needs an exposure for each period (denominators)".into(),
            ));
        }
        ChartType::P if !ratio_data => {
            return Err(PhnError::InvalidInput(
                "a p chart needs a denominator for each period".into(),
            ));
        }
        ChartType::Xmr | ChartType::C if ratio_data => {
            return Err(PhnError::InvalidInput(
                "an XmR or c chart takes one value per period; leave the denominators out or \
                 choose a p chart (proportions) or a run chart"
                    .into(),
            ));
        }
        _ => {}
    }
    if chart_type == ChartType::U && prep.local.is_some() {
        return Err(PhnError::InvalidInput(
            "u charts need an exposure for each period; give inline values and denominators".into(),
        ));
    }
    let mut phase_starts = Vec::new();
    for l in a.phase_starts.clone().unwrap_or_default() {
        let i = prep
            .labels
            .iter()
            .position(|x| x.trim().eq_ignore_ascii_case(l.trim()))
            .ok_or_else(|| {
                PhnError::InvalidInput(format!("phase start '{l}' is not one of the period labels"))
            })?;
        phase_starts.push(i);
    }
    // A run chart of proportions plots the proportion.
    let plotted: Vec<f64> = match (&prep.denominators, chart_type) {
        (Some(d), ChartType::Run) => prep.values.iter().zip(d).map(|(x, n)| x / n).collect(),
        _ => prep.values.clone(),
    };
    if let (Some(d), ChartType::Run) = (&prep.denominators, chart_type)
        && d.iter().any(|x| *x <= 0.0)
    {
        return Err(PhnError::InvalidInput(
            "each period needs a denominator greater than zero".into(),
        ));
    }
    let input = Input {
        chart: chart_type,
        values: plotted.clone(),
        denominators: (chart_type != ChartType::Run)
            .then(|| prep.denominators.clone())
            .flatten(),
        phase_starts: phase_starts.clone(),
        baseline: a.baseline_points,
    };
    let chart = spc::chart(&input).map_err(PhnError::InvalidInput)?;
    let scale = if chart_type == ChartType::P || (chart_type == ChartType::Run && ratio_data) {
        a.display_scale.unwrap_or(100.0)
    } else {
        1.0
    };
    let rule_set = (chart_type == ChartType::Run).then(|| a.rule_set.unwrap_or_default());
    let use_ihi = matches!(rule_set, Some(RuleSet::Both | RuleSet::Ihi));
    let use_anhoej = matches!(rule_set, Some(RuleSet::Both | RuleSet::Anhoej));
    let perla: Option<Perla> = if use_ihi {
        Some(
            rules::analyze(&plotted, &phase_starts, a.baseline_points)
                .map_err(PhnError::InvalidInput)?,
        )
    } else {
        None
    };

    let any_hidden = prep.hide.iter().any(|h| *h);
    let withhold = prep.local.is_some() && any_hidden;
    let labels = &prep.labels;

    // ---- per-period tags, findings, and chart flags
    let mut tags: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut findings: Vec<Finding> = Vec::new();
    let mut flags: Vec<(usize, String)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let improving = |dir_above: bool| -> Option<bool> {
        direction.map(|d| (d == Direction::Increase) == dir_above)
    };
    let direction_note = |above: bool| -> String {
        match improving(above) {
            Some(true) => " This is the direction your aim wants.".into(),
            Some(false) => " This is the opposite of the direction your aim wants.".into(),
            None => String::new(),
        }
    };
    if let Some(p) = &perla {
        for ph in &p.phases {
            for s in &ph.shifts {
                let above = s.direction == "above";
                findings.push(Finding {
                    rule_set: "ihi".into(),
                    rule: "shift".into(),
                    periods: span_label(labels, s.first, s.last),
                    meaning: format!(
                        "{} consecutive points {} the median (a shift is {SHIFT_LEN} or more).{}",
                        s.length,
                        s.direction,
                        direction_note(above)
                    ),
                });
                for i in &s.indices {
                    tags[*i].push("shift".into());
                }
                flags.push((
                    s.signal_at,
                    format!("Shift: {SHIFT_LEN}+ points {} the median", s.direction),
                ));
            }
            for s in &ph.trends {
                findings.push(Finding {
                    rule_set: "ihi".into(),
                    rule: "trend".into(),
                    periods: span_label(labels, s.first, s.last),
                    meaning: format!(
                        "{} points going {} (a trend is {TREND_LEN} or more; a repeated value counts once).{}",
                        s.length,
                        s.direction,
                        direction_note(s.direction == "up")
                    ),
                });
                for i in &s.indices {
                    tags[*i].push("trend".into());
                }
                flags.push((
                    s.signal_at,
                    format!("Trend: {TREND_LEN}+ points going {}", s.direction),
                ));
            }
            if ph.runs.too_few {
                findings.push(Finding {
                    rule_set: "ihi".into(),
                    rule: "too few runs".into(),
                    periods: span_label(labels, ph.start, ph.end),
                    meaning: format!(
                        "{} runs; at most {} is unusually few for {} points above and {} below the median.",
                        ph.runs.observed,
                        ph.runs.too_few_at_most.unwrap_or(0),
                        ph.runs.points_above,
                        ph.runs.points_below
                    ),
                });
            }
            if ph.runs.too_many {
                findings.push(Finding {
                    rule_set: "ihi".into(),
                    rule: "too many runs".into(),
                    periods: span_label(labels, ph.start, ph.end),
                    meaning: format!(
                        "{} runs; at least {} is unusually many for {} points above and {} below the median (often a sign of overcorrecting, sampling, or a measure that alternates).",
                        ph.runs.observed,
                        ph.runs.too_many_at_least.unwrap_or(0),
                        ph.runs.points_above,
                        ph.runs.points_below
                    ),
                });
            }
            // A hidden period is never named as an extreme point in the response.
            let shown: Vec<usize> = ph
                .astronomical
                .iter()
                .copied()
                .filter(|i| !prep.hide[*i])
                .collect();
            if !shown.is_empty() {
                let which: Vec<&str> = shown.iter().take(5).map(|i| labels[*i].as_str()).collect();
                findings.push(Finding {
                    rule_set: "ihi".into(),
                    rule: "astronomical point (candidate)".into(),
                    periods: which.join(", "),
                    meaning: "A point far from the others by a screening rule (modified z-score above 3.5 against the other points). Perla treats an astronomical point as a judgment call: look at the chart and ask what happened that period.".into(),
                });
                for i in &shown {
                    tags[*i].push("astronomical?".into());
                }
            }
            for i in &ph.astronomical {
                flags.push((*i, "Astronomical point (candidate)".into()));
            }
        }
    }
    if use_anhoej || chart_type != ChartType::Run {
        for (i, pt) in chart.points.iter().enumerate() {
            for r in &pt.signals {
                if *r == Rule::TooFewCrossings {
                    continue;
                }
                tags[i].push(tag(*r).to_string());
            }
        }
    }
    if use_anhoej {
        for (k, ph) in chart.phases.iter().enumerate() {
            let range = span_label(labels, ph.start, ph.end);
            if let (Some(l), Some(lim)) = (ph.longest_run, ph.longest_run_limit)
                && l > lim
            {
                findings.push(Finding {
                    rule_set: "anhoej".into(),
                    rule: "unusually long run".into(),
                    periods: range.clone(),
                    meaning: format!(
                        "The longest run on one side of the median is {l} points; more than {lim} is unusual for {} points off the median.",
                        ph.useful_observations.unwrap_or(0)
                    ),
                });
                if let Some(first) =
                    (ph.start..=ph.end).find(|i| chart.points[*i].signals.contains(&Rule::LongRun))
                {
                    flags.push((first, "Unusually long run (Anhøj)".into()));
                }
            }
            if let (Some(c), Some(lim)) = (ph.crossings, ph.crossings_limit)
                && c < lim
            {
                findings.push(Finding {
                    rule_set: "anhoej".into(),
                    rule: "too few crossings".into(),
                    periods: range,
                    meaning: format!(
                        "The line crosses the median {c} times; fewer than {lim} is unusual for {} points off the median.",
                        ph.useful_observations.unwrap_or(0)
                    ),
                });
            }
            let _ = k;
        }
    }
    if chart_type != ChartType::Run {
        use std::collections::BTreeMap;
        let mut by_rule: BTreeMap<&'static str, Vec<usize>> = BTreeMap::new();
        for (i, pt) in chart.points.iter().enumerate() {
            for r in &pt.signals {
                by_rule.entry(r.describe()).or_default().push(i);
            }
            if let Some(r) = pt.signals.first() {
                flags.push((i, r.describe().to_string()));
            }
        }
        for (rule, idx) in by_rule {
            findings.push(Finding {
                rule_set: "control".into(),
                rule: rule.to_string(),
                periods: format!(
                    "{} ({} point{})",
                    span_label(labels, idx[0], *idx.last().unwrap_or(&idx[0])),
                    idx.len(),
                    if idx.len() == 1 { "" } else { "s" }
                ),
                meaning:
                    "Special-cause signal: the process is not stable here. Look for what changed."
                        .into(),
            });
        }
    }
    flags.sort_by_key(|(i, _)| *i);
    flags.dedup_by_key(|(i, _)| *i);

    // ---- notes on reading the result
    let min = content.run_chart.minimum_points;
    let pref = content.run_chart.preferred_points;
    if chart_type == ChartType::Run {
        let base = a.baseline_points.unwrap_or(n);
        if n < min {
            notes.push(format!(
                "Only {n} points: run chart rules need at least {min}, and {pref} or more are better; treat any signal or lack of signal as provisional."
            ));
        } else if base < pref {
            notes.push(format!(
                "The median rests on {base} points; {pref} or more give a steadier median."
            ));
        }
        let ihi = findings.iter().any(|f| f.rule_set == "ihi");
        let anh = findings.iter().any(|f| f.rule_set == "anhoej");
        if ihi && !anh && use_anhoej {
            notes.push("The IHI rules signal but the Anhøj rules do not. In a simulation study the Perla rules gave many false signals (their trend and too-many-runs tests were judged not useful), so treat this as a prompt to look, not as proof of change.".into());
        }
        if findings.is_empty() {
            notes.push("No non-random signals: the variation looks like common-cause (random) variation, which means no evidence yet that the process changed.".into());
        }
        notes.push(content.run_chart.signal_note.text.clone());
        if !phase_starts.is_empty() {
            notes.push("A new median starts at each phase you named; judge a change after the new phase has enough points of its own.".into());
        }
    } else {
        notes.push("A signal says something changed, not why. Do not adjust a stable process in response to a single point.".into());
        notes.extend(chart.warnings.iter().cloned());
    }
    if let Some(l) = &prep.local {
        notes.push(format!(
            "Rows of {} were grouped into periods and only the period results are shown here.",
            l.label
        ));
    }

    // ---- rows and phases
    let rows: Vec<AnalysisRow> = chart
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| AnalysisRow {
            label: labels[i].clone(),
            value: (!prep.hide[i]).then_some(p.value * scale),
            center: (!withhold).then_some(p.center * scale),
            lcl: p.lcl.filter(|_| !withhold).map(|v| v * scale),
            ucl: p.ucl.filter(|_| !withhold).map(|v| v * scale),
            signals: if prep.hide[i] {
                Vec::new()
            } else {
                tags[i].clone()
            },
            suppressed: prep.hide[i],
        })
        .collect();
    let phases: Vec<PhaseOut> = chart
        .phases
        .iter()
        .enumerate()
        .map(|(k, ph)| {
            let pp = perla.as_ref().and_then(|p| p.phases.get(k));
            PhaseOut {
                first_period: labels[ph.start].clone(),
                last_period: labels[ph.end].clone(),
                baseline_points: ph.baseline_points,
                center: (!withhold).then_some(ph.center * scale),
                useful_points: ph.useful_observations,
                longest_run: ph.longest_run,
                longest_run_limit: use_anhoej.then_some(ph.longest_run_limit).flatten(),
                crossings: ph.crossings,
                crossings_limit: use_anhoej.then_some(ph.crossings_limit).flatten(),
                longest_trend: pp.map(|p| p.longest_trend),
                runs: pp.map(|p| p.runs.observed),
                runs_too_few_at_most: pp.and_then(|p| p.runs.too_few_at_most),
                runs_too_many_at_least: pp.and_then(|p| p.runs.too_many_at_least),
                average_moving_range: ph.average_moving_range.filter(|_| !withhold),
            }
        })
        .collect();

    // ---- chart
    let measure_name = a.measure_name.clone();
    let title = format!(
        "{}{}",
        match chart_type {
            ChartType::Run => "Run chart",
            ChartType::Xmr => "XmR control chart",
            ChartType::P => "p control chart",
            ChartType::U => "u control chart",
            ChartType::C => "c control chart",
        },
        measure_name
            .as_ref()
            .map(|m| format!(": {m}"))
            .unwrap_or_default()
    );
    let mut spec = ChartSpec::new(ChartKind::Line, title.clone(), labels.clone());
    spec.series = vec![Series {
        name: measure_name.clone().unwrap_or_else(|| "Value".into()),
        values: chart.points.iter().map(|p| Some(p.value * scale)).collect(),
        ..Series::default()
    }];
    spec.ref_lines.push(RefLine {
        label: if chart_type == ChartType::Run {
            "Median".into()
        } else {
            "Center line".into()
        },
        values: chart
            .points
            .iter()
            .map(|p| Some(p.center * scale))
            .collect(),
        dashed: false,
    });
    if chart_type != ChartType::Run {
        spec.ref_lines.push(RefLine {
            label: "Upper control limit".into(),
            values: chart
                .points
                .iter()
                .map(|p| p.ucl.map(|v| v * scale))
                .collect(),
            dashed: true,
        });
        spec.ref_lines.push(RefLine {
            label: "Lower control limit".into(),
            values: chart
                .points
                .iter()
                .map(|p| p.lcl.map(|v| v * scale))
                .collect(),
            dashed: true,
        });
    }
    spec.flags = flags
        .iter()
        .map(|(i, l)| Flag {
            series: 0,
            index: *i,
            label: l.clone(),
        })
        .collect();
    spec.flag_legend = Some("Signal".into());
    for &s in &phase_starts {
        spec.markers.push(VMarker {
            at: s as f64,
            label: format!("Change: {}", labels[s]),
        });
    }
    spec.include_zero = Some(matches!(
        chart_type,
        ChartType::P | ChartType::C | ChartType::U
    ));
    spec.y_label = measure_name.clone();

    let mut files = Vec::new();
    let mut caveats = Vec::new();
    let mut png = None;
    let mut png_path = None;
    let stem = format!(
        "qi_{}_{}",
        match chart_type {
            ChartType::Run => "run_chart",
            _ => "control_chart",
        },
        title.chars().take(30).collect::<String>()
    );
    let mut chart_files =
        match write_chart(ctx, &spec, &stem, a.format.unwrap_or(ChartFormat::Both)) {
            Ok((f, rendered)) => {
                png = phn_docs::svg::svg_to_png(&rendered.svg, 2.0).ok();
                png_path = f.png.clone().or_else(|| f.svg.clone());
                files.extend(f.png.iter().chain(f.svg.iter()).cloned());
                Some(f)
            }
            Err(e) => {
                caveats.push(format!(
                    "The chart file was not written: {}",
                    crate::context::error_message(&e)
                ));
                None
            }
        };

    // ---- text alternative
    let first = &labels[0];
    let last = &labels[n - 1];
    let signal_count = findings.len();
    let mut alt = format!(
        "{title}: {n} periods from {first} to {last}, with {} {}.",
        if chart_type == ChartType::Run {
            "a median line"
        } else {
            "a center line and control limits"
        },
        if phases.len() > 1 {
            format!("recalculated at {} process change(s)", phases.len() - 1)
        } else {
            String::new()
        }
    );
    if !withhold {
        let (lo, hi) = rows
            .iter()
            .filter_map(|r| r.value)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
                (l.min(v), h.max(v))
            });
        if lo.is_finite() {
            alt.push_str(&format!(
                " Values run from {} to {}.",
                md::num(lo, 2),
                md::num(hi, 2)
            ));
        }
        if let Some(c) = phases.first().and_then(|p| p.center) {
            alt.push_str(&format!(
                " First {} {}.",
                if chart_type == ChartType::Run {
                    "median"
                } else {
                    "center line"
                },
                md::num(c, 2)
            ));
        }
    } else {
        alt.push_str(" Values are in the chart file in your output folder.");
    }
    alt.push_str(&if signal_count == 0 {
        " No signals of non-random variation.".to_string()
    } else {
        format!(
            " {} signal(s): {}.",
            signal_count,
            findings
                .iter()
                .map(|f| format!("{} ({})", f.rule, f.periods))
                .collect::<Vec<_>>()
                .join("; ")
        )
    });
    if let Some(f) = chart_files.as_mut()
        && prep.local.is_some()
    {
        // The returned alt text must not carry values hidden by suppression.
        f.alt_text = alt.clone();
        f.embed_markdown = format!(
            "![{}]({})",
            alt.replace(['[', ']'], ""),
            f.png.clone().or(f.svg.clone()).unwrap_or_default()
        );
    }

    // ---- Mermaid line chart (only when nothing is hidden)
    let mermaid_chart = (!any_hidden && n <= 60).then(|| {
        let xs: Vec<String> = labels
            .iter()
            .map(|l| format!("\"{}\"", l.replace('"', "'")))
            .collect();
        let vals: Vec<String> = chart
            .points
            .iter()
            .map(|p| format!("{:.4}", p.value * scale))
            .collect();
        let centers: Vec<String> = chart
            .points
            .iter()
            .map(|p| format!("{:.4}", p.center * scale))
            .collect();
        format!(
            "xychart-beta\n    title \"{}\"\n    x-axis [{}]\n    line [{}]\n    line [{}]\n",
            title.replace('"', "'"),
            xs.join(", "),
            vals.join(", "),
            centers.join(", ")
        )
    });

    let identifier_categories = prep
        .local
        .as_ref()
        .map(|l| l.identifier_categories())
        .unwrap_or_default();
    let local_source = prep.local.as_ref().map(|l| l.source());
    Ok(Analysis {
        out: AnalysisOut {
            data_from: prep.from.clone(),
            chart_type,
            rule_set,
            periods: n,
            rows,
            phases,
            findings,
            notes,
            chart: chart_files,
            mermaid_chart,
            text_alternative: alt,
            values_withheld: withhold,
        },
        png,
        png_path,
        files,
        suppression_applied: any_hidden,
        identifier_categories,
        local_source,
        caveats,
        labels: prep.labels,
        numerators: prep.values,
        denominators: prep.denominators,
        baseline_points: a.baseline_points,
        any_hidden,
    })
}
