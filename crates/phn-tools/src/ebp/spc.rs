//! EBP-06 `create_spc_chart`.

use chrono::{Datelike, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::chart::{ChartKind, ChartSpec, Flag, RefLine, Series, VMarker};
use phn_epi::spc::{self, ChartType, Input, PhaseSummary, Rule};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::charts::{ChartFiles, ChartFormat, write_chart};
use crate::context::Context;
use crate::localdata;
use crate::md;

/// What to measure when reading a local file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileMeasure {
    /// Number of records per period (c chart).
    Count,
    /// Share of records per period where `flag_column` is yes/true (p chart).
    Proportion,
    /// Average of `value_column` per period (XmR chart).
    Mean,
}

/// Period length for grouping dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Week,
    #[default]
    Month,
    Quarter,
}

/// Arguments for `create_spc_chart`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SpcArgs {
    /// run (median line, run rules), xmr (individual values), p (proportions),
    /// u (rates per unit), or c (counts). Default: run for inline data, or
    /// chosen from `measure` for files.
    pub chart_type: Option<ChartType>,
    /// Chart title, e.g. "Hand hygiene compliance, Unit 3".
    pub title: Option<String>,
    /// Inline: period labels in time order.
    pub labels: Option<Vec<String>>,
    /// Inline: values (run, xmr), numerators (p, u), or counts (c).
    pub values: Option<Vec<f64>>,
    /// Inline: denominators (p) or exposure (u: e.g. patient-days, 1,000 doses).
    pub denominators: Option<Vec<f64>>,
    /// File: CSV/XLSX in /data with one row per event or observation.
    pub path: Option<String>,
    pub sheet: Option<String>,
    /// File: date column used to group rows into periods.
    pub date_column: Option<String>,
    /// File: week, month (default), or quarter.
    pub period: Option<Period>,
    /// File: count, proportion, or mean.
    pub measure: Option<FileMeasure>,
    /// File, proportion: yes/no column marking the outcome.
    pub flag_column: Option<String>,
    /// File, mean: numeric column to average.
    pub value_column: Option<String>,
    /// Points at the start (of each phase) used to compute the center and limits.
    pub baseline_points: Option<usize>,
    /// Labels of the first period after each process change (limits are recomputed).
    pub phase_starts: Option<Vec<String>>,
    /// Name of the measure, e.g. "% of doses documented within 24 h".
    pub measure_name: Option<String>,
    /// Multiply p-chart proportions by this for display (default 100 = percent).
    pub display_scale: Option<f64>,
    /// png, svg, or both (default both).
    pub format: Option<ChartFormat>,
}

/// One period in the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpcRow {
    pub label: String,
    /// Plotted value (null when suppressed in this response).
    pub value: Option<f64>,
    pub center: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lcl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ucl: Option<f64>,
    pub signals: Vec<Rule>,
    pub suppressed: bool,
}

/// Result of `create_spc_chart`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpcResult {
    pub chart_type: ChartType,
    pub rows: Vec<SpcRow>,
    pub phases: Vec<PhaseSummary>,
    /// Periods with a special-cause or non-random signal.
    pub signals: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<ChartFiles>,
    pub display_scale: f64,
}

struct Prepared {
    labels: Vec<String>,
    values: Vec<f64>,
    denominators: Option<Vec<f64>>,
    /// Per-period flag: hide this value in the response (small numbers from local data).
    hide: Vec<bool>,
    local: Option<localdata::LocalTable>,
}

fn period_key(d: NaiveDate, p: Period) -> (i32, u32) {
    match p {
        Period::Week => {
            let w = d.iso_week();
            (w.year(), w.week())
        }
        Period::Month => (d.year(), d.month()),
        Period::Quarter => (d.year(), (d.month() - 1) / 3 + 1),
    }
}

fn next_key(k: (i32, u32), p: Period) -> (i32, u32) {
    let max = match p {
        Period::Week => NaiveDate::from_ymd_opt(k.0, 12, 28).map_or(52, |d| d.iso_week().week()),
        Period::Month => 12,
        Period::Quarter => 4,
    };
    if k.1 >= max {
        (k.0 + 1, 1)
    } else {
        (k.0, k.1 + 1)
    }
}

fn key_label(k: (i32, u32), p: Period) -> String {
    match p {
        Period::Week => format!("{}-W{:02}", k.0, k.1),
        Period::Month => format!("{}-{:02}", k.0, k.1),
        Period::Quarter => format!("{} Q{}", k.0, k.1),
    }
}

fn prepare_file(
    ctx: &Context,
    a: &SpcArgs,
    measure: FileMeasure,
    threshold: u32,
) -> Result<Prepared> {
    let lt = localdata::load(
        ctx,
        a.path.as_deref().unwrap_or_default(),
        a.sheet.as_deref(),
    )?;
    let dname = a
        .date_column
        .as_deref()
        .ok_or_else(|| PhnError::InvalidInput("give date_column".into()))?;
    let di = lt.column(dname)?;
    let period = a.period.unwrap_or_default();
    let dates: Vec<Option<NaiveDate>> = lt
        .table
        .column(di)
        .map(phn_local::phi::parse_date)
        .collect();
    let keys: Vec<Option<(i32, u32)>> = dates
        .iter()
        .map(|d| d.map(|d| period_key(d, period)))
        .collect();
    let (Some(first), Some(last)) = (
        keys.iter().flatten().min().copied(),
        keys.iter().flatten().max().copied(),
    ) else {
        return Err(PhnError::InvalidInput(format!(
            "no dates could be read in '{dname}'"
        )));
    };
    let mut all = vec![first];
    while *all.last().unwrap_or(&first) < last && all.len() < 600 {
        let n = next_key(*all.last().unwrap_or(&first), period);
        all.push(n);
    }
    let idx = |k: (i32, u32)| all.iter().position(|x| *x == k);
    let n = all.len();
    let mut values = vec![0.0; n];
    let mut dens = vec![0.0; n];
    let mut counts = vec![0usize; n];
    match measure {
        FileMeasure::Count => {
            for k in keys.iter().flatten() {
                if let Some(i) = idx(*k) {
                    values[i] += 1.0;
                }
            }
        }
        FileMeasure::Proportion => {
            let fname = a.flag_column.as_deref().ok_or_else(|| {
                PhnError::InvalidInput("give flag_column for a proportion".into())
            })?;
            let fi = lt.column(fname)?;
            for (k, v) in keys.iter().zip(lt.table.column(fi)) {
                if let (Some(k), Some(b)) = (k, localdata::truthy(v))
                    && let Some(i) = idx(*k)
                {
                    dens[i] += 1.0;
                    if b {
                        values[i] += 1.0;
                    }
                }
            }
        }
        FileMeasure::Mean => {
            let vname = a
                .value_column
                .as_deref()
                .ok_or_else(|| PhnError::InvalidInput("give value_column for a mean".into()))?;
            let vi = lt.column(vname)?;
            let (nums, _) = lt.numeric(vi);
            for (k, v) in keys.iter().zip(&nums) {
                if let (Some(k), Some(v)) = (k, v)
                    && let Some(i) = idx(*k)
                {
                    values[i] += v;
                    counts[i] += 1;
                }
            }
            for i in 0..n {
                if counts[i] == 0 {
                    return Err(PhnError::InvalidInput(format!(
                        "period {} has no values; an XmR chart needs a value in every period",
                        key_label(all[i], period)
                    )));
                }
                values[i] /= counts[i] as f64;
            }
        }
    }
    if measure == FileMeasure::Proportion && dens.contains(&0.0) {
        return Err(PhnError::InvalidInput(
            "some periods have no records with a yes/no value; use a longer period".into(),
        ));
    }
    let t = f64::from(threshold);
    let hide: Vec<bool> = (0..n)
        .map(|i| match measure {
            FileMeasure::Count => values[i] > 0.0 && values[i] < t,
            FileMeasure::Proportion => {
                (values[i] > 0.0 && values[i] < t)
                    || (dens[i] - values[i] > 0.0 && dens[i] - values[i] < t)
            }
            FileMeasure::Mean => counts[i] < threshold as usize,
        })
        .collect();
    Ok(Prepared {
        labels: all.iter().map(|k| key_label(*k, period)).collect(),
        values,
        denominators: (measure == FileMeasure::Proportion).then_some(dens),
        hide,
        local: Some(lt),
    })
}

/// Run charts and control charts with special-cause detection.
pub fn create_spc_chart(ctx: &Context, args: SpcArgs) -> Result<Envelope<SpcResult>> {
    let threshold = ctx.suppression_threshold();
    let file = args.path.is_some();
    if file == args.values.is_some() {
        return Err(PhnError::InvalidInput(
            "give either inline values (with labels) or a local file (path)".into(),
        ));
    }
    let (chart_type, prep) = if file {
        let measure = args.measure.unwrap_or(FileMeasure::Count);
        let default = match measure {
            FileMeasure::Count => ChartType::C,
            FileMeasure::Proportion => ChartType::P,
            FileMeasure::Mean => ChartType::Xmr,
        };
        let ct = args.chart_type.unwrap_or(default);
        if ct == ChartType::U {
            return Err(PhnError::InvalidInput(
                "u charts need an exposure for each period; give inline values and denominators"
                    .into(),
            ));
        }
        if (measure == FileMeasure::Proportion) != (ct == ChartType::P) && ct != ChartType::Run {
            return Err(PhnError::InvalidInput(
                "use a p chart for proportions (or a run chart)".into(),
            ));
        }
        (ct, prepare_file(ctx, &args, measure, threshold)?)
    } else {
        let values = args.values.clone().unwrap_or_default();
        let labels = args
            .labels
            .clone()
            .unwrap_or_else(|| (1..=values.len()).map(|i| i.to_string()).collect());
        if labels.len() != values.len() {
            return Err(PhnError::InvalidInput("give one label per value".into()));
        }
        (
            args.chart_type.unwrap_or(ChartType::Run),
            Prepared {
                hide: vec![false; values.len()],
                labels,
                values,
                denominators: args.denominators.clone(),
                local: None,
            },
        )
    };
    let n = prep.values.len();
    if !(2..=600).contains(&n) {
        return Err(PhnError::InvalidInput(
            "a chart needs 2 to 600 periods".into(),
        ));
    }
    let mut phase_starts = Vec::new();
    for l in args.phase_starts.clone().unwrap_or_default() {
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
    let run_values: Vec<f64> = match (&prep.denominators, chart_type) {
        (Some(d), ChartType::Run) => prep.values.iter().zip(d).map(|(x, n)| x / n).collect(),
        _ => prep.values.clone(),
    };
    let input = Input {
        chart: chart_type,
        values: run_values,
        denominators: if chart_type == ChartType::Run {
            None
        } else {
            prep.denominators.clone()
        },
        phase_starts: phase_starts.clone(),
        baseline: args.baseline_points,
    };
    let chart = spc::chart(&input).map_err(PhnError::InvalidInput)?;
    let scale = if chart_type == ChartType::P
        || (chart_type == ChartType::Run && prep.denominators.is_some())
    {
        args.display_scale.unwrap_or(100.0)
    } else {
        1.0
    };
    let rows: Vec<SpcRow> = chart
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| SpcRow {
            label: prep.labels[i].clone(),
            value: (!prep.hide[i]).then_some(p.value * scale),
            center: p.center * scale,
            lcl: p.lcl.map(|v| v * scale),
            ucl: p.ucl.map(|v| v * scale),
            signals: p.signals.clone(),
            suppressed: prep.hide[i],
        })
        .collect();
    let signals: Vec<String> = rows
        .iter()
        .filter(|r| !r.signals.is_empty())
        .map(|r| {
            format!(
                "{}: {}",
                r.label,
                r.signals
                    .iter()
                    .map(|s| s.describe())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })
        .collect();

    // Chart file (local, full values).
    let title = args.title.clone().unwrap_or_else(|| {
        format!(
            "{} chart{}",
            match chart_type {
                ChartType::Run => "Run",
                ChartType::Xmr => "XmR (individuals)",
                ChartType::P => "p",
                ChartType::U => "u",
                ChartType::C => "c",
            },
            args.measure_name
                .as_ref()
                .map(|m| format!(": {m}"))
                .unwrap_or_default()
        )
    });
    let mut spec = ChartSpec::new(ChartKind::Line, title.clone(), prep.labels.clone());
    spec.series = vec![Series {
        name: args.measure_name.clone().unwrap_or_else(|| "Value".into()),
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
    spec.flags = chart
        .points
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.signals.is_empty())
        .map(|(i, p)| Flag {
            series: 0,
            index: i,
            label: p
                .signals
                .first()
                .map(|s| s.describe())
                .unwrap_or_default()
                .to_string(),
        })
        .collect();
    spec.flag_legend = Some(if chart_type == ChartType::Run {
        "Non-random signal".into()
    } else {
        "Special cause".into()
    });
    for &s in &phase_starts {
        spec.markers.push(VMarker {
            at: s as f64,
            label: format!("Change: {}", prep.labels[s]),
        });
    }
    spec.include_zero = Some(matches!(
        chart_type,
        ChartType::P | ChartType::C | ChartType::U
    ));
    spec.y_label = args.measure_name.clone();
    let chart_files = match write_chart(
        ctx,
        &spec,
        &format!("spc_{}", title.chars().take(40).collect::<String>()),
        args.format.unwrap_or_default(),
    ) {
        Ok((mut f, _)) => {
            if prep.local.is_some() {
                // The returned alt text must not carry values hidden by suppression.
                f.alt_text = format!(
                    "{} with {} periods from {} to {}; {} period(s) flagged. Values are in the \
                     chart file in your output folder.",
                    title,
                    n,
                    prep.labels[0],
                    prep.labels[n - 1],
                    signals.len()
                );
                f.embed_markdown = format!(
                    "![{}]({})",
                    f.alt_text,
                    f.png.clone().or(f.svg.clone()).unwrap_or_default()
                );
            }
            Some(f)
        }
        Err(e) if prep.local.is_none() => return Err(e),
        Err(_) => None,
    };

    let mut s = format!("**{title}** — {} periods\n\n", n);
    for (k, ph) in chart.phases.iter().enumerate() {
        let pl = if chart.phases.len() > 1 {
            format!(
                "Phase {} ({} – {}): ",
                k + 1,
                prep.labels[ph.start],
                prep.labels[ph.end]
            )
        } else {
            String::new()
        };
        match chart_type {
            ChartType::Run => s.push_str(&format!(
                "- {pl}median {}; longest run {} (limit {}); crossings {} (minimum {}); {} useful observations\n",
                md::num(ph.center * scale, 2),
                ph.longest_run.unwrap_or(0),
                ph.longest_run_limit.unwrap_or(0),
                ph.crossings.unwrap_or(0),
                ph.crossings_limit.unwrap_or(0),
                ph.useful_observations.unwrap_or(0)
            )),
            _ => s.push_str(&format!(
                "- {pl}center line {} from {} baseline points{}\n",
                md::num(ph.center * scale, 3),
                ph.baseline_points,
                ph.average_moving_range.map(|m| format!("; average moving range {}", md::num(m, 3))).unwrap_or_default()
            )),
        }
    }
    s.push('\n');
    if signals.is_empty() {
        s.push_str(if chart_type == ChartType::Run {
            "**No non-random signals:** the variation looks like common-cause (random) variation.\n\n"
        } else {
            "**No special-cause signals:** the process appears stable (in statistical control).\n\n"
        });
    } else {
        s.push_str("**Signals**\n");
        for x in &signals {
            s.push_str(&format!("- {x}\n"));
        }
        s.push('\n');
    }
    let show_limits = chart_type != ChartType::Run;
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut v = vec![
                r.label.clone(),
                r.value
                    .map(|v| md::num(v, 2))
                    .unwrap_or_else(|| "suppressed".into()),
                md::num(r.center, 2),
            ];
            if show_limits {
                v.push(r.lcl.map(|x| md::num(x, 2)).unwrap_or_default());
                v.push(r.ucl.map(|x| md::num(x, 2)).unwrap_or_default());
            }
            v.push(if r.signals.is_empty() {
                String::new()
            } else {
                "⚑".into()
            });
            v
        })
        .collect();
    let mut heads = vec![
        "Period",
        "Value",
        if chart_type == ChartType::Run {
            "Median"
        } else {
            "Center"
        },
    ];
    if show_limits {
        heads.extend(["LCL", "UCL"]);
    }
    heads.push("Signal");
    s.push_str(&md::table(&heads, &table_rows));
    if let Some(f) = &chart_files {
        s.push_str(&format!(
            "\nChart: {}\n",
            f.png.clone().or(f.svg.clone()).unwrap_or_default()
        ));
    }

    let mut caveats = chart.warnings.clone();
    caveats.push(
        "A signal says something changed, not why. Investigate special causes with the people who \
         do the work before acting; do not adjust a stable process in response to a single point."
            .into(),
    );
    if chart_type == ChartType::C {
        caveats.push("A c chart assumes the same opportunity for events in every period; use a u chart if the population or patient-days change.".into());
    }
    let mut sources = vec![Source::computed(
        "Provost LP, Murray SK. The Health Care Data Guide (2011) control-chart rules; Anhøj J, \
         Olesen AV. Run charts revisited. PLoS One 2014;9:e113825",
        "PHN MCP server calculation (limits verified against R qcc)",
    )];
    let mut privacy = Privacy {
        suppression_threshold: Some(threshold),
        ..Privacy::none()
    };
    if let Some(lt) = &prep.local {
        sources.push(lt.source());
        privacy.identifiers_detected = lt.identifier_categories();
        privacy.suppression_applied = prep.hide.iter().any(|h| *h);
        privacy.notes.push(
            "Rows were aggregated by period; values based on fewer records than the suppression \
             threshold are hidden in this response but drawn in the chart saved to your output \
             folder."
                .into(),
        );
        if let Some(f) = &chart_files {
            privacy.files_written = f.png.iter().chain(f.svg.iter()).cloned().collect();
        }
    }
    Ok(Envelope::new(
        s,
        SpcResult {
            chart_type,
            rows,
            phases: chart.phases,
            signals,
            chart: chart_files,
            display_scale: scale,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}
